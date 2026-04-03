//! Call-expression lowering helpers.
//!
//! WHAT: lowers resolved user and host calls into explicit HIR call statements and values.
//! WHY: call lowering is reused across AST expression forms and needs one place to manage
//! prelude sequencing, tuple return shaping, and temporary bindings.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ResultCallHandling};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpressionKind, HirPlace, HirStatement, HirStatementKind, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    pub(crate) fn lower_receiver_method_call_expression(
        &mut self,
        method_path: &InternedPath,
        receiver: &AstNode,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let function_id = self.resolve_function_id_or_error(method_path, location)?;
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push(receiver.get_expr()?);
        full_args.extend(args.iter().cloned());

        self.lower_call_expression(
            CallTarget::UserFunction(function_id),
            &full_args,
            result_types,
            location,
        )
    }

    // WHAT: lowers a resolved call target plus arguments into HIR call statements and values.
    // WHY: calls may emit preludes, temporary bindings, and tuple shaping, so the lowering needs
    //      one dedicated path instead of being duplicated across expression forms.
    pub(crate) fn lower_call_expression(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut prelude = Vec::new();
        let mut lowered_args = Vec::with_capacity(args.len());

        let result_carrying_user_call = match &target {
            CallTarget::UserFunction(function_id) => {
                // Some unit tests lower isolated call expressions without registering a full
                // function table in the module; in that mode we treat calls as plain values.
                self.function_index_by_id
                    .get(function_id)
                    .copied()
                    .and_then(|function_index| {
                        let function_return_type = self.module.functions[function_index].return_type;
                        match self.type_context.get(function_return_type).kind {
                            HirTypeKind::Result { ok, .. } => Some((function_return_type, ok)),
                            _ => None,
                        }
                    })
            }
            CallTarget::HostFunction(_) => None,
        };

        if result_carrying_user_call.is_some() {
            return_hir_transformation_error!(
                "Raw call to an error-returning function reached HIR lowering without explicit call-site handling",
                self.hir_error_location(location)
            );
        }

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            prelude.extend(lowered.prelude);
            lowered_args.push(lowered.value);
        }

        let no_return = result_types.is_empty();
        let statement_id = self.allocate_node_id();
        let region = self.current_region_or_error(location)?;

        if no_return {
            let statement = HirStatement {
                id: statement_id,
                kind: HirStatementKind::Call {
                    target,
                    args: lowered_args,
                    result: None,
                },
                location: location.to_owned(),
            };

            self.side_table.map_statement(location, &statement);
            prelude.push(statement);

            let value = self.unit_expression(location, region);
            self.log_call_result_binding(location, None, &value);
            return Ok(LoweredExpression { prelude, value });
        }

        let call_result_type = if result_types.len() == 1 {
            self.lower_data_type(&result_types[0], location)?
        } else {
            let field_types = result_types
                .iter()
                .map(|ret| self.lower_data_type(ret, location))
                .collect::<Result<Vec<_>, _>>()?;
            self.intern_type_kind(HirTypeKind::Tuple {
                fields: field_types,
            })
        };

        let temp_local = self.allocate_temp_local(call_result_type, Some(location.to_owned()))?;

        let statement = HirStatement {
            id: statement_id,
            kind: HirStatementKind::Call {
                target,
                args: lowered_args,
                result: Some(temp_local),
            },
            location: location.to_owned(),
        };

        self.side_table.map_statement(location, &statement);
        prelude.push(statement);

        let value = self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(temp_local)),
            call_result_type,
            ValueKind::RValue,
            region,
        );

        self.log_call_result_binding(location, Some(temp_local), &value);

        Ok(LoweredExpression { prelude, value })
    }

    pub(crate) fn lower_result_handled_call_expression(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        result_types: &[DataType],
        handling: &ResultCallHandling,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut prelude = Vec::new();
        let mut lowered_args = Vec::with_capacity(args.len());

        let (carrier_type, ok_type) = match &target {
            CallTarget::UserFunction(function_id) => {
                let Some(function_index) = self.function_index_by_id.get(function_id).copied()
                else {
                    return_hir_transformation_error!(
                        format!(
                            "Function {:?} is not registered in HIR module",
                            function_id
                        ),
                        self.hir_error_location(location)
                    );
                };

                let function_return_type = self.module.functions[function_index].return_type;
                match self.type_context.get(function_return_type).kind {
                    HirTypeKind::Result { ok, .. } => (function_return_type, ok),
                    _ => {
                        return_hir_transformation_error!(
                            "Result-handled call targeted a function without an internal Result return type",
                            self.hir_error_location(location)
                        );
                    }
                }
            }
            CallTarget::HostFunction(_) => {
                return_hir_transformation_error!(
                    "Result-handled call targeted a host function",
                    self.hir_error_location(location)
                );
            }
        };

        let requested_ok_type = self.lower_call_result_type(result_types, location)?;
        if requested_ok_type != ok_type {
            return_hir_transformation_error!(
                "Handled Result call lowered with mismatched ok type",
                self.hir_error_location(location)
            );
        }

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            prelude.extend(lowered.prelude);
            lowered_args.push(lowered.value);
        }

        let statement_id = self.allocate_node_id();
        let region = self.current_region_or_error(location)?;
        let temp_local = self.allocate_temp_local(carrier_type, Some(location.to_owned()))?;
        let statement = HirStatement {
            id: statement_id,
            kind: HirStatementKind::Call {
                target,
                args: lowered_args,
                result: Some(temp_local),
            },
            location: location.to_owned(),
        };

        self.side_table.map_statement(location, &statement);
        prelude.push(statement);

        let result_carrier = self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(temp_local)),
            carrier_type,
            ValueKind::RValue,
            region,
        );

        let handled_value = match handling {
            ResultCallHandling::Propagate => self.make_expression(
                location,
                HirExpressionKind::ResultPropagate {
                    result: Box::new(result_carrier),
                },
                ok_type,
                ValueKind::RValue,
                region,
            ),
            ResultCallHandling::Fallback(fallback_values) => {
                let fallback = self.lower_result_fallback_value(
                    fallback_values,
                    result_types,
                    ok_type,
                    location,
                )?;
                self.make_expression(
                    location,
                    HirExpressionKind::ResultFallback {
                        result: Box::new(result_carrier),
                        fallback: Box::new(fallback),
                    },
                    ok_type,
                    ValueKind::RValue,
                    region,
                )
            }
        };

        self.log_call_result_binding(location, Some(temp_local), &handled_value);

        Ok(LoweredExpression {
            prelude,
            value: handled_value,
        })
    }

    fn lower_call_result_type(
        &mut self,
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        if result_types.is_empty() {
            return Ok(self.intern_type_kind(HirTypeKind::Unit));
        }

        if result_types.len() == 1 {
            return self.lower_data_type(&result_types[0], location);
        }

        let field_types = result_types
            .iter()
            .map(|ret| self.lower_data_type(ret, location))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.intern_type_kind(HirTypeKind::Tuple {
            fields: field_types,
        }))
    }

    fn lower_result_fallback_value(
        &mut self,
        fallback_values: &[Expression],
        result_types: &[DataType],
        expected_ok_type: TypeId,
        location: &SourceLocation,
    ) -> Result<crate::compiler_frontend::hir::hir_nodes::HirExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;

        if result_types.is_empty() {
            let unit = self.unit_expression(location, region);
            if unit.ty != expected_ok_type {
                return_hir_transformation_error!(
                    "Result fallback expected unit success type but found different ok type",
                    self.hir_error_location(location)
                );
            }
            return Ok(unit);
        }

        if fallback_values.len() != result_types.len() {
            return_hir_transformation_error!(
                "Result fallback arity does not match the handled call success arity",
                self.hir_error_location(location)
            );
        }

        if fallback_values.len() == 1 {
            let lowered = self.lower_expression(&fallback_values[0])?;
            if !lowered.prelude.is_empty() {
                return_hir_transformation_error!(
                    "Fallback expressions with side effects are not supported in this lowering pass",
                    self.hir_error_location(location)
                );
            }

            if lowered.value.ty != expected_ok_type {
                return_hir_transformation_error!(
                    "Fallback value type does not match handled-call success type",
                    self.hir_error_location(location)
                );
            }

            return Ok(lowered.value);
        }

        let mut lowered_elements = Vec::with_capacity(fallback_values.len());
        for fallback in fallback_values {
            let lowered = self.lower_expression(fallback)?;
            if !lowered.prelude.is_empty() {
                return_hir_transformation_error!(
                    "Fallback expressions with side effects are not supported in this lowering pass",
                    self.hir_error_location(location)
                );
            }
            lowered_elements.push(lowered.value);
        }

        let tuple_ty = self.intern_type_kind(HirTypeKind::Tuple {
            fields: lowered_elements.iter().map(|value| value.ty).collect(),
        });
        if tuple_ty != expected_ok_type {
            return_hir_transformation_error!(
                "Fallback tuple type does not match handled-call success type",
                self.hir_error_location(location)
            );
        }

        Ok(self.make_expression(
            location,
            HirExpressionKind::TupleConstruct {
                elements: lowered_elements,
            },
            tuple_ty,
            ValueKind::RValue,
            region,
        ))
    }
}
