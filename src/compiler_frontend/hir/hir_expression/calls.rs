//! Call-expression lowering helpers.
//!
//! WHAT: lowers resolved user, external, receiver, and collection calls into explicit HIR call
//! statements and values.
//! WHY: ordinary calls may emit preludes, temporary bindings, tuple return shaping, and fresh
//! mutable argument materialization. Fallible propagation and `catch` CFG joins live in
//! `fallible.rs` so call lowering stays focused on call sequencing.
//!
//! ## Diagnostic boundary
//!
//! `CompilerError` / `return_hir_transformation_error!` in this module means an internal
//! HIR transformation or lowering invariant failure only.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::call_argument::{
    CallAccessMode, CallArgument, CallPassingMode,
};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::ids::TypeId as FrontendTypeId;
use crate::compiler_frontend::external_packages::CallTarget;
use crate::compiler_frontend::external_packages::ExternalFunctionId;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::LoweredExpression;

impl<'a> HirBuilder<'a> {
    pub(crate) fn lower_receiver_method_call_expression(
        &mut self,
        method_path: &InternedPath,
        receiver: &AstNode,
        args: &[CallArgument],
        result_type_ids: &[FrontendTypeId],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let function_id = self.resolve_function_id_or_error(method_path, location)?;
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push(Self::shared_call_argument(receiver.get_expr()?, location));
        full_args.extend(args.iter().cloned());

        self.lower_call_expression(
            CallTarget::UserFunction(function_id),
            &full_args,
            result_type_ids,
            location,
        )
    }

    pub(crate) fn lower_collection_builtin_call_expression(
        &mut self,
        op: CollectionBuiltinOp,
        receiver: &AstNode,
        args: &[CallArgument],
        result_type_ids: &[FrontendTypeId],
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let mut full_args = Vec::with_capacity(args.len() + 1);
        full_args.push(Self::shared_call_argument(receiver.get_expr()?, location));
        full_args.extend(args.iter().cloned());

        let id = match op {
            CollectionBuiltinOp::Get => ExternalFunctionId::CollectionGet,
            CollectionBuiltinOp::Set => ExternalFunctionId::CollectionSet,
            CollectionBuiltinOp::Push => ExternalFunctionId::CollectionPush,
            CollectionBuiltinOp::Remove => ExternalFunctionId::CollectionRemove,
            CollectionBuiltinOp::Length => ExternalFunctionId::CollectionLength,
        };

        self.lower_call_expression(
            CallTarget::ExternalFunction(id),
            &full_args,
            result_type_ids,
            location,
        )
    }

    // WHAT: lowers a resolved call target plus arguments into HIR call statements and values.
    // WHY: calls may emit preludes, temporary bindings, and tuple shaping, so the lowering needs
    //      one dedicated path instead of being duplicated across expression forms.
    pub(crate) fn lower_call_expression(
        &mut self,
        target: CallTarget,
        args: &[CallArgument],
        result_type_ids: &[FrontendTypeId],
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
                        match self
                            .type_environment
                            .fallible_carrier_slots(function_return_type)
                        {
                            Some((ok, _)) => Some((function_return_type, ok)),
                            _ => None,
                        }
                    })
            }
            CallTarget::ExternalFunction(_) => None,
        };

        if result_carrying_user_call.is_some() {
            return_hir_transformation_error!(
                "Raw call to an error-returning function reached HIR lowering without explicit call-site handling",
                self.hir_error_location(location)
            );
        }

        for (arg_index, argument) in args.iter().enumerate() {
            if self.expression_needs_current_block_lowering(&argument.value) {
                self.flush_pending_call_prelude(&mut prelude, location)?;
            }

            let lowered = self.lower_call_argument_value(argument, location, arg_index)?;
            prelude.extend(lowered.prelude);
            lowered_args.push(lowered.value);
        }

        let no_return = result_type_ids.is_empty();
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

        let call_result_type = self.lower_call_result_type(result_type_ids, location)?;
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

    pub(super) fn lower_call_result_type(
        &mut self,
        result_type_ids: &[FrontendTypeId],
        location: &SourceLocation,
    ) -> Result<TypeId, CompilerError> {
        if result_type_ids.is_empty() {
            return Ok(self.type_environment.builtins().none);
        }

        if result_type_ids.len() == 1 {
            return self.lower_type_id(result_type_ids[0], location);
        }

        let field_types = result_type_ids
            .iter()
            .map(|ret| self.lower_type_id(*ret, location))
            .collect::<Result<Vec<_>, _>>()?;
        Ok(self.type_environment.intern_tuple(field_types))
    }

    fn shared_call_argument(value: Expression, location: &SourceLocation) -> CallArgument {
        let arg_location = if value.location == SourceLocation::default() {
            location.to_owned()
        } else {
            value.location.clone()
        };
        CallArgument::positional(value, CallAccessMode::Shared, arg_location)
    }

    pub(super) fn lower_call_argument_value(
        &mut self,
        argument: &CallArgument,
        call_location: &SourceLocation,
        argument_index: usize,
    ) -> Result<LoweredExpression, CompilerError> {
        let lowered_value = if self.expression_needs_current_block_lowering(&argument.value) {
            LoweredExpression {
                prelude: vec![],
                value: self.lower_expression_value_to_current_block(&argument.value)?,
            }
        } else {
            self.lower_expression(&argument.value)?
        };

        if argument.passing_mode != CallPassingMode::FreshMutableValue {
            return Ok(lowered_value);
        }

        let mut prelude = lowered_value.prelude;
        let value = lowered_value.value;
        let value_type = value.ty;
        let temp_local = self.allocate_fresh_mutable_call_arg_local(
            value_type,
            Some(argument.location.to_owned()),
            call_location,
            argument_index,
        )?;

        let assign_statement = HirStatement {
            id: self.allocate_node_id(),
            kind: HirStatementKind::Assign {
                target: HirPlace::Local(temp_local),
                value,
            },
            location: argument.location.to_owned(),
        };
        self.side_table
            .map_statement(&argument.location, &assign_statement);
        prelude.push(assign_statement);

        let region = self.current_region_or_error(&argument.location)?;
        let value = self.make_expression(
            &argument.location,
            HirExpressionKind::Load(HirPlace::Local(temp_local)),
            value_type,
            ValueKind::RValue,
            region,
        );

        Ok(LoweredExpression { prelude, value })
    }

    fn flush_pending_call_prelude(
        &mut self,
        prelude: &mut Vec<HirStatement>,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        for statement in prelude.drain(..) {
            self.emit_statement_to_current_block(statement, location)?;
        }

        Ok(())
    }
}
