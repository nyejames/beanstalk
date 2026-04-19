//! HIR Statement Lowering
//!
//! Lowers AST statements and control-flow nodes into explicit HIR blocks, statements, and
//! terminators.

use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, LoopBindings, MultiBindTarget, MultiBindTargetKind, NodeKind, RangeLoopSpec,
    SourceLocation,
};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ResultCallHandling,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirExpressionKind, HirPlace, HirStatement, HirStatementKind,
    HirTerminator, ResultVariant, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::return_hir_transformation_error;

use crate::hir_log;

mod control_flow;
mod declarations;
mod loop_lowering;

impl<'a> HirBuilder<'a> {
    // WHAT: routes one top-level AST node into the HIR lowering path that owns it.
    // WHY: declaration registration already built the symbol tables, so top-level lowering should
    //      only accept nodes that materially contribute module/runtime semantics.
    pub(crate) fn lower_top_level_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        match &node.kind {
            NodeKind::Function(name, signature, body) => {
                self.lower_function_body(name, signature, body, &node.location)
            }

            NodeKind::StructDefinition(_, _) => Ok(()),

            NodeKind::Return(_) | NodeKind::ReturnError(_) => return_hir_transformation_error!(
                "Top-level return is not valid during HIR lowering",
                self.hir_error_location(&node.location)
            ),

            _ => return_hir_transformation_error!(
                format!(
                    "Top-level AST node is not a supported declaration: {:?}",
                    node.kind
                ),
                self.hir_error_location(&node.location)
            ),
        }
    }

    // WHAT: enters one function's lowering context, lowers its body, then restores builder state.
    // WHY: function lowering needs scoped block/region/current-function state that must not leak
    //      into the next function.
    pub(crate) fn lower_function_body(
        &mut self,
        function_name: &InternedPath,
        signature: &FunctionSignature,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.resolve_function_id_or_error(function_name, location)?;

        self.enter_function(function_id, location)?;

        // WHAT: for entry start(), allocate the Vec<String> fragment accumulator before lowering.
        // WHY: PushStartRuntimeFragment nodes in the body push to this local; the implicit return
        //      at end of entry start loads it as the function result.
        if function_id == self.module.start_function {
            let string_ty = self.intern_type_kind(HirTypeKind::String);
            let vec_ty = self.intern_type_kind(HirTypeKind::Collection { element: string_ty });
            let vec_local = self.allocate_temp_local(vec_ty, Some(location.clone()))?;

            let region = self.current_region_or_error(location)?;
            let empty_collection = self.make_expression(
                location,
                HirExpressionKind::Collection(vec![]),
                vec_ty,
                ValueKind::RValue,
                region,
            );
            self.emit_assign_local_statement(vec_local, empty_collection, location)?;
            self.entry_fragment_vec_local = Some(vec_local);
        }

        let lower_result = self.lower_function_body_inner(function_id, signature, body, location);
        self.leave_function();

        lower_result
    }

    // WHAT: lowers a run of AST statements until a terminating control-flow edge is emitted.
    // WHY: once a block has an explicit terminator, later statements in the sequence are dead for
    //      the current CFG path and must not be appended.
    pub(crate) fn lower_statement_sequence(
        &mut self,
        nodes: &[AstNode],
    ) -> Result<(), CompilerError> {
        for node in nodes {
            let current_block = self.current_block_id_or_error(&node.location)?;
            if self.block_has_explicit_terminator(current_block, &node.location)? {
                break;
            }

            self.lower_statement_node(node)?;
        }

        Ok(())
    }

    // WHAT: lowers one AST statement node into HIR statements, blocks, or terminators.
    // WHY: statement lowering is the control-flow dispatcher for the builder and centralizes the
    //      mapping from AST statement kinds to explicit HIR form.
    pub(crate) fn lower_statement_node(&mut self, node: &AstNode) -> Result<(), CompilerError> {
        self.log_statement_input(node);

        let result = match &node.kind {
            NodeKind::VariableDeclaration(var) => {
                self.lower_variable_declaration_statement(var, &node.location)
            }

            NodeKind::Assignment { target, value } => {
                self.lower_assignment_statement(target, value, &node.location)
            }

            NodeKind::MultiBind { targets, value } => {
                self.lower_multi_bind_statement(targets, value, &node.location)
            }

            NodeKind::FunctionCall {
                name,
                args,
                result_types: _,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                self.lower_call_statement(
                    CallTarget::UserFunction(function_id),
                    args,
                    location,
                )
            }

            NodeKind::ResultHandledFunctionCall {
                name,
                args,
                result_types,
                handling,
                location,
            } => self.lower_result_handled_call_statement(
                name,
                args,
                result_types,
                handling,
                location,
            ),

            NodeKind::HostFunctionCall {
                name: host_function_id,
                args,
                result_types: _,
                location,
            } => self.lower_call_statement(
                CallTarget::HostFunction(host_function_id.to_owned()),
                args,
                location,
            ),

            NodeKind::Rvalue(expr) => self.lower_expression_statement(expr, &node.location),

            NodeKind::FieldAccess { .. } => self.lower_field_access_statement(node, &node.location),

            NodeKind::Return(values) => self.lower_return_statement(values, &node.location),

            NodeKind::ReturnError(value) => {
                self.lower_error_return_statement(value, &node.location)
            }

            NodeKind::If(condition, then_body, else_body) => {
                self.lower_if_statement(condition, then_body, else_body.as_deref(), &node.location)
            }

            NodeKind::WhileLoop(condition, body) => {
                self.lower_while_statement(condition, body, &node.location)
            }

            NodeKind::Break => self.lower_break_statement(&node.location),

            NodeKind::Continue => self.lower_continue_statement(&node.location),

            NodeKind::Match(scrutinee, arms, default) => {
                self.lower_match_statement(scrutinee, arms, default.as_deref(), &node.location)
            }

            NodeKind::RangeLoop {
                bindings,
                range,
                body,
            } => self.lower_range_loop_statement(bindings, range, body, &node.location),

            NodeKind::CollectionLoop {
                bindings,
                iterable,
                body,
            } => self.lower_collection_loop_statement(bindings, iterable, body, &node.location),

            NodeKind::Operator(_) => Ok(()),

            NodeKind::PushStartRuntimeFragment(expr) => {
                // WHAT: lower a top-level runtime template push into a PushRuntimeFragment HIR statement.
                // WHY: the fragment accumulator local was allocated at function entry; each
                //      PushStartRuntimeFragment appends one evaluated string to it.
                let Some(vec_local) = self.entry_fragment_vec_local else {
                    return_hir_transformation_error!(
                        "PushStartRuntimeFragment encountered outside entry start() — no fragment vec local is active",
                        self.hir_error_location(&node.location)
                    );
                };

                let lowered = self.lower_expression(expr)?;

                for prelude in lowered.prelude {
                    self.emit_statement_to_current_block(prelude, &node.location)?;
                }
                self.emit_statement_kind(
                    HirStatementKind::PushRuntimeFragment {
                        vec_local,
                        value: lowered.value,
                    },
                    &node.location,
                )
            }

            _ => return_hir_transformation_error!(
                format!(
                    "Unsupported AST statement node during HIR lowering: {:?}",
                    node.kind
                ),
                self.hir_error_location(&node.location)
            ),
        };

        if result.is_ok() {
            self.log_statement_output(node);
        }

        result
    }

    fn lower_function_body_inner(
        &mut self,
        function_id: FunctionId,
        signature: &FunctionSignature,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let return_type = self
            .function_by_id_or_error(function_id, location)?
            .return_type;

        self.lower_parameter_locals(function_id, signature, location)?;
        self.lower_statement_sequence(body)?;

        let current_block = self.current_block_id_or_error(location)?;
        if self.block_has_explicit_terminator(current_block, location)? {
            return Ok(());
        }

        if self.is_unit_type(return_type) {
            let region = self.current_region_or_error(location)?;
            let unit = self.unit_expression(location, region);
            self.emit_terminator(current_block, HirTerminator::Return(unit), location)?;
            return Ok(());
        }

        if let crate::compiler_frontend::hir::hir_datatypes::HirTypeKind::Result { ok, .. } =
            self.type_context.get(return_type).kind
            && signature.return_data_types().is_empty()
        {
            let region = self.current_region_or_error(location)?;
            let unit = self.unit_expression(location, region);
            if unit.ty != ok {
                return_hir_transformation_error!(
                    "Result function with empty success returns has non-unit ok type",
                    self.hir_error_location(location)
                );
            }

            let ok_result = self.make_expression(
                location,
                HirExpressionKind::ResultConstruct {
                    variant: ResultVariant::Ok,
                    value: Box::new(unit),
                },
                return_type,
                crate::compiler_frontend::hir::hir_nodes::ValueKind::RValue,
                region,
            );
            self.emit_terminator(current_block, HirTerminator::Return(ok_result), location)?;
            return Ok(());
        }

        // WHAT: entry start() has an implicit return of the fragment vec accumulator.
        // WHY: the body contains only PushStartRuntimeFragment nodes with no explicit return;
        //      the return type is Vec<String> which the builder consumes as the fragment list.
        if function_id == self.module.start_function
            && let Some(vec_local) = self.entry_fragment_vec_local
        {
            let vec_type = self.local_type_id_or_error(vec_local, location)?;
            let region = self.current_region_or_error(location)?;
            let load_expr = self.make_local_load_expression(vec_local, vec_type, location, region);
            self.emit_terminator(current_block, HirTerminator::Return(load_expr), location)?;
            return Ok(());
        }

        let function_name = self
            .side_table
            .resolve_function_name(function_id, self.string_table)
            .unwrap_or("<unknown>");

        return_hir_transformation_error!(
            format!(
                "Function '{}' can fall through without returning a value",
                function_name
            ),
            self.hir_error_location(location)
        )
    }

    fn lower_assignment_statement(
        &mut self,
        target: &AstNode,
        value: &Expression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let (target_prelude, target_place) = self.lower_ast_node_to_place(target)?;
        let lowered_value = self.lower_expression(value)?;

        for prelude in target_prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        for prelude in lowered_value.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: target_place,
                value: lowered_value.value,
            },
            location,
        )
    }

    fn lower_result_handled_call_statement(
        &mut self,
        name: &InternedPath,
        args: &[CallArgument],
        result_types: &[DataType],
        handling: &ResultCallHandling,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.resolve_function_id_or_error(name, location)?;
        let lowered = self.lower_result_handled_call_expression(
            CallTarget::UserFunction(function_id),
            args,
            result_types,
            handling,
            false,
            location,
        )?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        if self.is_unit_type(lowered.value.ty) {
            if matches!(handling, ResultCallHandling::Propagate) {
                self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)?;
            }
            return Ok(());
        }

        self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)
    }

    fn lower_multi_bind_statement(
        &mut self,
        targets: &[MultiBindTarget],
        value: &Expression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        if targets.len() < 2 {
            return_hir_transformation_error!(
                "Single-target bind unexpectedly reached multi-bind lowering",
                self.hir_error_location(location)
            );
        }

        let lowered_rhs = self.lower_expression(value)?;
        for prelude in lowered_rhs.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let rhs_type = lowered_rhs.value.ty;
        let rhs_local = self.allocate_temp_local(rhs_type, Some(location.to_owned()))?;
        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: HirPlace::Local(rhs_local),
                value: lowered_rhs.value,
            },
            location,
        )?;

        let tuple_fields = match &self.type_context.get(rhs_type).kind {
            HirTypeKind::Tuple { fields } => fields.to_owned(),
            _ => {
                return_hir_transformation_error!(
                    "Multi-bind right-hand value lowered to a non-tuple shape",
                    self.hir_error_location(location)
                );
            }
        };

        if tuple_fields.len() != targets.len() {
            return_hir_transformation_error!(
                "Multi-bind slot arity does not match lowered tuple shape",
                self.hir_error_location(location)
            );
        }

        for (slot_index, target) in targets.iter().enumerate() {
            let slot_type = tuple_fields[slot_index];
            let target_type = self.lower_data_type(&target.data_type, &target.location)?;

            if slot_type != target_type {
                return_hir_transformation_error!(
                    format!(
                        "Lowered multi-bind slot type mismatch at index {}",
                        slot_index
                    ),
                    self.hir_error_location(&target.location)
                );
            }

            let target_local = match target.kind {
                MultiBindTargetKind::Declaration => self.allocate_named_local(
                    target.id.to_owned(),
                    target_type,
                    target.ownership.is_mutable(),
                    Some(target.location.to_owned()),
                )?,
                MultiBindTargetKind::Assignment => {
                    let Some(local_id) = self.locals_by_name.get(&target.id).copied() else {
                        return_hir_transformation_error!(
                            format!(
                                "Multi-bind assignment target '{}' is missing from local bindings",
                                self.symbol_name_for_diagnostics(&target.id)
                            ),
                            self.hir_error_location(&target.location)
                        );
                    };

                    let Some((block_index, local_index)) =
                        self.local_index_by_id.get(&local_id).copied()
                    else {
                        return_hir_transformation_error!(
                            "Multi-bind assignment target local is not registered in HIR blocks",
                            self.hir_error_location(&target.location)
                        );
                    };

                    let local = &self.module.blocks[block_index].locals[local_index];
                    if !local.mutable {
                        return_hir_transformation_error!(
                            format!(
                                "Multi-bind assignment target '{}' lowered as immutable local",
                                self.symbol_name_for_diagnostics(&target.id)
                            ),
                            self.hir_error_location(&target.location)
                        );
                    }

                    if local.ty != target_type {
                        return_hir_transformation_error!(
                            format!(
                                "Multi-bind assignment target '{}' lowered with mismatched local type",
                                self.symbol_name_for_diagnostics(&target.id)
                            ),
                            self.hir_error_location(&target.location)
                        );
                    }

                    local_id
                }
            };

            let slot_region = self.current_region_or_error(&target.location)?;
            let tuple_value = self.make_expression(
                &target.location,
                HirExpressionKind::Load(HirPlace::Local(rhs_local)),
                rhs_type,
                ValueKind::RValue,
                slot_region,
            );
            let slot_value = self.make_expression(
                &target.location,
                HirExpressionKind::TupleGet {
                    tuple: Box::new(tuple_value),
                    index: slot_index,
                },
                slot_type,
                ValueKind::RValue,
                slot_region,
            );

            self.emit_statement_kind(
                HirStatementKind::Assign {
                    target: HirPlace::Local(target_local),
                    value: slot_value,
                },
                &target.location,
            )?;
        }

        Ok(())
    }

    fn lower_call_statement(
        &mut self,
        target: CallTarget,
        args: &[CallArgument],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let lowered = self.lower_call_expression(target, args, &[], location)?;
        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        Ok(())
    }

    fn lower_expression_statement(
        &mut self,
        expression: &Expression,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let lowered = self.lower_expression(expression)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        if self.is_unit_type(lowered.value.ty) {
            if matches!(
                expression.kind,
                ExpressionKind::ResultHandledFunctionCall { .. }
            ) {
                self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)?;
            }
            return Ok(());
        }

        self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)
    }

    fn lower_field_access_statement(
        &mut self,
        field_access_node: &AstNode,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let lowered = self.lower_ast_node_as_expression(field_access_node)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        if self.is_unit_type(lowered.value.ty) {
            return Ok(());
        }

        self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)
    }

    fn lower_range_loop_statement(
        &mut self,
        bindings: &LoopBindings,
        range: &RangeLoopSpec,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        // Loop lowering is intentionally split into a dedicated submodule to keep this file
        // focused on statement dispatch and shared lowering helpers.
        self.lower_range_loop_statement_impl(bindings, range, body, location)
    }

    fn lower_collection_loop_statement(
        &mut self,
        bindings: &LoopBindings,
        iterable: &Expression,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        self.lower_collection_loop_statement_impl(bindings, iterable, body, location)
    }

    fn emit_statement_kind(
        &mut self,
        kind: HirStatementKind,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let statement = HirStatement {
            id: self.allocate_node_id(),
            kind,
            location: location.clone(),
        };

        self.side_table.map_statement(location, &statement);
        self.emit_statement_to_current_block(statement, location)
    }

    fn log_statement_input(&self, _node: &AstNode) {
        hir_log!(format!("[HIR][Stmt] Lowering {:?}", _node.kind));
    }

    fn log_statement_output(&self, _node: &AstNode) {
        hir_log!(format!("[HIR][Stmt] Lowered {:?}", _node.kind));
    }

    fn log_block_created(&self, _block_id: BlockId, _label: &str, _location: &SourceLocation) {
        hir_log!(format!(
            "[HIR][CFG] Created block {} ({}) @ {:?}",
            _block_id, _label, _location
        ));
    }

    fn log_control_flow_edge(&self, _from: BlockId, _to: BlockId, _label: &str) {
        hir_log!(format!("[HIR][CFG] Edge {} -> {} ({})", _from, _to, _label));
    }

    fn log_terminator_emitted(
        &self,
        _block_id: BlockId,
        _terminator: &HirTerminator,
        _location: &SourceLocation,
    ) {
        hir_log!(format!(
            "[HIR][CFG] Terminator for {} @ {:?}: {}",
            _block_id,
            _location,
            _terminator.display_with_context(
                &crate::compiler_frontend::hir::hir_display::HirDisplayContext::new(
                    self.string_table,
                )
                .with_side_table(&self.side_table)
                .with_type_context(&self.type_context),
            )
        ));
    }
}
