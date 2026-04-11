//! Call-expression lowering helpers.
//!
//! WHAT: lowers resolved user and host calls into explicit HIR call statements and values.
//! WHY: call lowering is reused across AST expression forms and needs one place to manage
//! prelude sequencing, tuple return shaping, and temporary bindings.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ResultCallHandling};
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpressionKind, HirPlace, HirStatement, HirStatementKind, HirTerminator, LocalId,
    ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::host_functions::ERROR_BUBBLE_HOST_NAME;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

/// Shared handled-result call metadata used by branching lowering helpers.
///
/// WHAT: carries the resolved Result carrier types, handler policy, and location metadata.
/// WHY: both helper layers need the same bundle, and passing one struct keeps signatures short.
struct HandledResultBranchingContext<'a> {
    result_types: &'a [DataType],
    handling: &'a ResultCallHandling,
    carrier_type: TypeId,
    ok_type: TypeId,
    err_type: TypeId,
    value_required: bool,
    location: &'a SourceLocation,
}

/// Branch-entry metadata once the call has already produced a result local.
///
/// WHAT: extends handled-result metadata with CFG entry block and temporary local identifiers.
/// WHY: the carrier-branch helper should receive one coherent context instead of many scalars.
struct ResultCarrierBranchingContext<'a> {
    current_block: BlockId,
    result_local: LocalId,
    handled_result: HandledResultBranchingContext<'a>,
}

impl<'a> HirBuilder<'a> {
    pub(crate) fn lower_handled_result_expression(
        &mut self,
        value: &Expression,
        handling: &ResultCallHandling,
        location: &SourceLocation,
        expr_type: &DataType,
    ) -> Result<LoweredExpression, CompilerError> {
        let lowered = self.lower_expression(value)?;
        let (carrier_type, ok_type, err_type) = match self.type_context.get(lowered.value.ty).kind {
            HirTypeKind::Result { ok, err } => (lowered.value.ty, ok, err),
            _ => {
                return_hir_transformation_error!(
                    "Handled result expression reached HIR lowering without an internal Result type",
                    self.hir_error_location(location)
                );
            }
        };

        let expected_ok_type = self.lower_data_type(expr_type, location)?;
        if expected_ok_type != ok_type {
            return_hir_transformation_error!(
                "Handled Result expression lowered with mismatched ok type",
                self.hir_error_location(location)
            );
        }

        if matches!(handling, ResultCallHandling::Propagate) {
            let region = self.current_region_or_error(location)?;
            return Ok(LoweredExpression {
                prelude: lowered.prelude,
                value: self.make_expression(
                    location,
                    HirExpressionKind::ResultPropagate {
                        result: Box::new(lowered.value),
                    },
                    ok_type,
                    ValueKind::RValue,
                    region,
                ),
            });
        }

        let current_block = self.current_block_id_or_error(location)?;
        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let result_local = self.allocate_temp_local(carrier_type, Some(location.to_owned()))?;
        self.emit_assign_local_statement(result_local, lowered.value, location)?;

        let result_types = self.extract_return_types_from_datatype(expr_type);
        self.lower_result_carrier_with_branching(ResultCarrierBranchingContext {
            current_block,
            result_local,
            handled_result: HandledResultBranchingContext {
                result_types: &result_types,
                handling,
                carrier_type,
                ok_type,
                err_type,
                value_required: true,
                location,
            },
        })
    }

    pub(crate) fn lower_receiver_method_call_expression(
        &mut self,
        method_path: &InternedPath,
        builtin: Option<BuiltinMethodKind>,
        receiver: &AstNode,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        if let Some(builtin) = builtin {
            return self.lower_builtin_receiver_call_expression(
                builtin,
                method_path,
                receiver,
                args,
                result_types,
                location,
            );
        }

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

    fn lower_builtin_receiver_call_expression(
        &mut self,
        builtin: BuiltinMethodKind,
        method_path: &InternedPath,
        receiver: &AstNode,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        match builtin {
            BuiltinMethodKind::CollectionGet
            | BuiltinMethodKind::CollectionPush
            | BuiltinMethodKind::CollectionRemove
            | BuiltinMethodKind::CollectionLength => {
                let mut full_args = Vec::with_capacity(args.len() + 1);
                full_args.push(receiver.get_expr()?);
                full_args.extend(args.iter().cloned());
                self.lower_call_expression(
                    CallTarget::HostFunction(method_path.to_owned()),
                    &full_args,
                    result_types,
                    location,
                )
            }

            BuiltinMethodKind::CollectionSet => {
                self.lower_collection_set_call_expression(receiver, args, location)
            }

            BuiltinMethodKind::ErrorWithLocation | BuiltinMethodKind::ErrorPushTrace => {
                let mut full_args = Vec::with_capacity(args.len() + 1);
                full_args.push(receiver.get_expr()?);
                full_args.extend(args.iter().cloned());
                self.lower_call_expression(
                    CallTarget::HostFunction(method_path.to_owned()),
                    &full_args,
                    result_types,
                    location,
                )
            }

            BuiltinMethodKind::ErrorBubble => self.lower_error_bubble_call_expression(
                method_path,
                receiver,
                args,
                result_types,
                location,
            ),
        }
    }

    fn lower_error_bubble_call_expression(
        &mut self,
        method_path: &InternedPath,
        receiver: &AstNode,
        args: &[Expression],
        result_types: &[DataType],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        if !args.is_empty() {
            return_hir_transformation_error!(
                format!(
                    "Error bubble lowering expected 0 explicit arguments, found {}",
                    args.len()
                ),
                self.hir_error_location(location)
            );
        }

        let mut full_args = Vec::with_capacity(5);
        full_args.push(receiver.get_expr()?);
        full_args.extend(self.make_error_bubble_context_args(method_path, location)?);

        self.lower_call_expression(
            CallTarget::HostFunction(method_path.to_owned()),
            &full_args,
            result_types,
            location,
        )
    }

    fn make_error_bubble_context_args(
        &mut self,
        method_path: &InternedPath,
        location: &SourceLocation,
    ) -> Result<Vec<Expression>, CompilerError> {
        let method_name = method_path.name_str(self.string_table).unwrap_or_default();
        if method_name != ERROR_BUBBLE_HOST_NAME {
            return_hir_transformation_error!(
                format!(
                    "Error bubble context argument synthesis used for non-bubble builtin '{}'",
                    method_name
                ),
                self.hir_error_location(location)
            );
        }

        let file_text = location.scope.to_portable_string(self.string_table);
        let file_id = self.string_table.get_or_intern(file_text);
        let line = (location.start_pos.line_number + 1).max(0) as i64;
        let column = (location.start_pos.char_column + 1).max(0) as i64;

        let function_name_text = self
            .current_function_id_or_error(location)
            .ok()
            .and_then(|function_id| {
                self.side_table
                    .resolve_function_name(function_id, self.string_table)
            })
            .unwrap_or_default()
            .to_owned();
        let function_name_id = self.string_table.get_or_intern(function_name_text);

        Ok(vec![
            Expression::string_slice(file_id, location.to_owned(), Ownership::ImmutableOwned),
            Expression::int(line, location.to_owned(), Ownership::ImmutableOwned),
            Expression::int(column, location.to_owned(), Ownership::ImmutableOwned),
            Expression::string_slice(
                function_name_id,
                location.to_owned(),
                Ownership::ImmutableOwned,
            ),
        ])
    }

    fn lower_collection_set_call_expression(
        &mut self,
        receiver: &AstNode,
        args: &[Expression],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        // WHAT: lowers `collection.set(index, value)` to direct indexed assignment.
        // WHY: collection `set` is frontend-owned syntax, not a runtime host helper call.
        if args.len() != 2 {
            return_hir_transformation_error!(
                format!(
                    "Collection set lowering expected 2 arguments, found {}",
                    args.len()
                ),
                self.hir_error_location(location)
            );
        }

        let (receiver_prelude, receiver_place) = self.lower_ast_node_to_place(receiver)?;
        let lowered_index = self.lower_expression(&args[0])?;
        let lowered_value = self.lower_expression(&args[1])?;

        let mut prelude = receiver_prelude;
        prelude.extend(lowered_index.prelude);
        prelude.extend(lowered_value.prelude);

        let index_place = HirPlace::Index {
            base: Box::new(receiver_place),
            index: Box::new(lowered_index.value),
        };

        let assign_statement = HirStatement {
            id: self.allocate_node_id(),
            kind: HirStatementKind::Assign {
                target: index_place,
                value: lowered_value.value,
            },
            location: location.to_owned(),
        };
        self.side_table.map_statement(location, &assign_statement);
        prelude.push(assign_statement);

        let region = self.current_region_or_error(location)?;
        let value = self.unit_expression(location, region);

        Ok(LoweredExpression { prelude, value })
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
                HandledResultBranchingContext {
                    result_types,
                    handling,
                    carrier_type,
                    ok_type,
                    err_type,
                    value_required,
                    location,
                },
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
        context: HandledResultBranchingContext<'_>,
    ) -> Result<LoweredExpression, CompilerError> {
        let HandledResultBranchingContext {
            result_types,
            handling,
            carrier_type,
            ok_type,
            err_type,
            value_required,
            location,
        } = context;
        let current_block = self.current_block_id_or_error(location)?;
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

        self.lower_result_carrier_with_branching(ResultCarrierBranchingContext {
            current_block,
            result_local,
            handled_result: HandledResultBranchingContext {
                result_types,
                handling,
                carrier_type,
                ok_type,
                err_type,
                value_required,
                location,
            },
        })
    }

    fn lower_result_carrier_with_branching(
        &mut self,
        context: ResultCarrierBranchingContext<'_>,
    ) -> Result<LoweredExpression, CompilerError> {
        let ResultCarrierBranchingContext {
            current_block,
            result_local,
            handled_result,
        } = context;
        let HandledResultBranchingContext {
            result_types,
            handling,
            carrier_type,
            ok_type,
            err_type,
            value_required,
            location,
        } = handled_result;

        let region = self.current_region_or_error(location)?;
        let bool_type = self.intern_type_kind(HirTypeKind::Bool);
        let result_for_test =
            self.make_local_load_expression(result_local, carrier_type, location, region);
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

        let needs_merge_value = value_required && !self.is_unit_type(ok_type);
        let merge_local = if needs_merge_value {
            Some(self.allocate_temp_local(ok_type, Some(location.to_owned()))?)
        } else {
            None
        };

        self.set_current_block(success_block, location)?;
        if let Some(ok_local) = merge_local {
            let success_region = self.current_region_or_error(location)?;
            let success_result = self.make_local_load_expression(
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
        let error_result =
            self.make_local_load_expression(result_local, carrier_type, location, error_region);
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
                        self.make_local_load_expression(ok_local, ok_type, location, merge_region)
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
            self.make_local_load_expression(ok_local, ok_type, location, merge_region)
        } else {
            self.unit_expression(location, merge_region)
        };

        Ok(LoweredExpression {
            prelude: vec![],
            value,
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
