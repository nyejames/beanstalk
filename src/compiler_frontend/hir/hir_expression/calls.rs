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
    HirExpression, HirExpressionKind, HirPlace, HirStatement, HirStatementKind, HirTerminator,
    LocalId, ValueKind,
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
                        let function_return_type =
                            self.module.functions[function_index].return_type;
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
        value_required: bool,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut prelude = Vec::new();
        let mut lowered_args = Vec::with_capacity(args.len());

        let (carrier_type, ok_type, err_type) = match &target {
            CallTarget::UserFunction(function_id) => {
                let Some(function_index) = self.function_index_by_id.get(function_id).copied()
                else {
                    return_hir_transformation_error!(
                        format!("Function {:?} is not registered in HIR module", function_id),
                        self.hir_error_location(location)
                    );
                };

                let function_return_type = self.module.functions[function_index].return_type;
                match self.type_context.get(function_return_type).kind {
                    HirTypeKind::Result { ok, err } => (function_return_type, ok, err),
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

        if !matches!(handling, ResultCallHandling::Propagate) {
            return self.lower_result_handled_call_with_branching(
                target,
                args,
                result_types,
                handling,
                carrier_type,
                ok_type,
                err_type,
                value_required,
                location,
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
            ResultCallHandling::Fallback(_) | ResultCallHandling::Handler { .. } => {
                return_hir_transformation_error!(
                    "Non-propagating handled call unexpectedly reached expression-only lowering",
                    self.hir_error_location(location)
                );
            }
        };

        self.log_call_result_binding(location, Some(temp_local), &handled_value);

        Ok(LoweredExpression {
            prelude,
            value: handled_value,
        })
    }

    fn lower_result_handled_call_with_branching(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        result_types: &[DataType],
        handling: &ResultCallHandling,
        carrier_type: TypeId,
        ok_type: TypeId,
        err_type: TypeId,
        value_required: bool,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let region = self.current_region_or_error(location)?;
        let mut lowered_args = Vec::with_capacity(args.len());

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            for prelude in lowered.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }
            lowered_args.push(lowered.value);
        }

        let result_local = self.allocate_temp_local(carrier_type, Some(location.to_owned()))?;
        let call_statement = HirStatement {
            id: self.allocate_node_id(),
            kind: HirStatementKind::Call {
                target,
                args: lowered_args,
                result: Some(result_local),
            },
            location: location.to_owned(),
        };
        self.side_table.map_statement(location, &call_statement);
        self.emit_statement_to_current_block(call_statement, location)?;

        let bool_type = self.intern_type_kind(HirTypeKind::Bool);
        let result_for_test =
            self.make_result_carrier_load_expression(result_local, carrier_type, location, region);
        let result_test = self.make_expression(
            location,
            HirExpressionKind::ResultIsOk {
                result: Box::new(result_for_test),
            },
            bool_type,
            ValueKind::RValue,
            region,
        );

        let success_block = self.create_block(region, location, "handled-result-ok")?;
        let error_region = self.create_child_region(region);
        let error_block = self.create_block(error_region, location, "handled-result-err")?;
        let merge_block = self.create_block(region, location, "handled-result-merge")?;

        self.emit_terminator(
            current_block,
            HirTerminator::If {
                condition: result_test,
                then_block: success_block,
                else_block: error_block,
            },
            location,
        )?;

        // WHAT: keeps one explicit continuation slot for handled calls that still need to yield a
        // value after branching.
        // WHY: both the ok branch and any non-terminating recovery path must merge back into one
        // stable value source instead of duplicating continuation logic downstream.
        let needs_merge_value = value_required && !self.is_unit_type(ok_type);
        let merge_local = if needs_merge_value {
            Some(self.allocate_temp_local(ok_type, Some(location.to_owned()))?)
        } else {
            None
        };

        self.set_current_block(success_block, location)?;
        if let Some(ok_local) = merge_local {
            let success_region = self.current_region_or_error(location)?;
            let success_result = self.make_result_carrier_load_expression(
                result_local,
                carrier_type,
                location,
                success_region,
            );
            let success_payload = self.make_expression(
                location,
                HirExpressionKind::ResultUnwrapOk {
                    result: Box::new(success_result),
                },
                ok_type,
                ValueKind::RValue,
                success_region,
            );
            self.emit_assign_local_statement(ok_local, success_payload, location)?;
        }

        self.emit_jump_to(
            success_block,
            merge_block,
            location,
            "handled-result.success.merge",
        )?;

        self.set_current_block(error_block, location)?;
        let error_region = self.current_region_or_error(location)?;
        let error_result = self.make_result_carrier_load_expression(
            result_local,
            carrier_type,
            location,
            error_region,
        );
        let error_payload = self.make_expression(
            location,
            HirExpressionKind::ResultUnwrapErr {
                result: Box::new(error_result),
            },
            err_type,
            ValueKind::RValue,
            error_region,
        );

        match handling {
            ResultCallHandling::Fallback(fallback_values) => {
                let fallback = self.lower_result_fallback_value(
                    fallback_values,
                    result_types,
                    ok_type,
                    location,
                )?;

                for prelude in fallback.prelude {
                    self.emit_statement_to_current_block(prelude, location)?;
                }

                if let Some(ok_local) = merge_local {
                    self.emit_assign_local_statement(ok_local, fallback.value, location)?;
                }
            }
            ResultCallHandling::Handler {
                error_name: _,
                error_binding,
                fallback,
                body,
            } => {
                let handler_error_local = self.allocate_named_local(
                    error_binding.to_owned(),
                    err_type,
                    false,
                    Some(location.to_owned()),
                )?;

                self.emit_assign_local_statement(handler_error_local, error_payload, location)?;
                self.lower_statement_sequence(body)?;

                let error_tail_block = self.current_block_id_or_error(location)?;
                if self.block_has_explicit_terminator(error_tail_block, location)? {
                    self.set_current_block(merge_block, location)?;
                    let value = if let Some(ok_local) = merge_local {
                        let merge_region = self.current_region_or_error(location)?;
                        self.make_expression(
                            location,
                            HirExpressionKind::Load(HirPlace::Local(ok_local)),
                            ok_type,
                            ValueKind::RValue,
                            merge_region,
                        )
                    } else {
                        self.unit_expression(location, self.current_region_or_error(location)?)
                    };
                    return Ok(LoweredExpression {
                        prelude: vec![],
                        value,
                    });
                }

                if let Some(fallback_values) = fallback {
                    let fallback = self.lower_result_fallback_value(
                        fallback_values,
                        result_types,
                        ok_type,
                        location,
                    )?;

                    for prelude in fallback.prelude {
                        self.emit_statement_to_current_block(prelude, location)?;
                    }

                    if let Some(ok_local) = merge_local {
                        self.emit_assign_local_statement(ok_local, fallback.value, location)?;
                    }
                } else if merge_local.is_some() {
                    return_hir_transformation_error!(
                        "Named handler without fallback reached HIR fallthrough while a value continuation is required",
                        self.hir_error_location(location)
                    );
                }
            }
            ResultCallHandling::Propagate => {
                return_hir_transformation_error!(
                    "Propagation handling unexpectedly reached branching lowering",
                    self.hir_error_location(location)
                );
            }
        }

        let error_tail_block = self.current_block_id_or_error(location)?;
        if !self.block_has_explicit_terminator(error_tail_block, location)? {
            self.emit_jump_to(
                error_tail_block,
                merge_block,
                location,
                "handled-result.error.merge",
            )?;
        }

        self.set_current_block(merge_block, location)?;
        let merge_region = self.current_region_or_error(location)?;
        let value = if let Some(ok_local) = merge_local {
            self.make_expression(
                location,
                HirExpressionKind::Load(HirPlace::Local(ok_local)),
                ok_type,
                ValueKind::RValue,
                merge_region,
            )
        } else {
            self.unit_expression(location, merge_region)
        };

        Ok(LoweredExpression {
            prelude: vec![],
            value,
        })
    }

    fn emit_assign_local_statement(
        &mut self,
        local: LocalId,
        value: HirExpression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let assign_statement = HirStatement {
            id: self.allocate_node_id(),
            kind: HirStatementKind::Assign {
                target: HirPlace::Local(local),
                value,
            },
            location: location.to_owned(),
        };

        self.side_table.map_statement(location, &assign_statement);
        self.emit_statement_to_current_block(assign_statement, location)
    }

    fn make_result_carrier_load_expression(
        &mut self,
        result_local: LocalId,
        carrier_type: TypeId,
        location: &SourceLocation,
        region: crate::compiler_frontend::hir::hir_nodes::RegionId,
    ) -> HirExpression {
        self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(result_local)),
            carrier_type,
            ValueKind::RValue,
            region,
        )
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
    ) -> Result<LoweredExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;

        if result_types.is_empty() {
            let unit = self.unit_expression(location, region);
            if unit.ty != expected_ok_type {
                return_hir_transformation_error!(
                    "Result fallback expected unit success type but found different ok type",
                    self.hir_error_location(location)
                );
            }
            return Ok(LoweredExpression {
                prelude: vec![],
                value: unit,
            });
        }

        if fallback_values.len() != result_types.len() {
            return_hir_transformation_error!(
                "Result fallback arity does not match the handled call success arity",
                self.hir_error_location(location)
            );
        }

        if fallback_values.len() == 1 {
            let lowered = self.lower_expression(&fallback_values[0])?;
            if lowered.value.ty != expected_ok_type {
                return_hir_transformation_error!(
                    "Fallback value type does not match handled-call success type",
                    self.hir_error_location(location)
                );
            }

            return Ok(lowered);
        }

        let mut prelude = Vec::new();
        let mut lowered_elements = Vec::with_capacity(fallback_values.len());
        for fallback in fallback_values {
            let lowered = self.lower_expression(fallback)?;
            prelude.extend(lowered.prelude);
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

        Ok(LoweredExpression {
            prelude,
            value: self.make_expression(
                location,
                HirExpressionKind::TupleConstruct {
                    elements: lowered_elements,
                },
                tuple_ty,
                ValueKind::RValue,
                region,
            ),
        })
    }
}
