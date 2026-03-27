//! HIR Statement Lowering
//!
//! Lowers AST statements and control-flow nodes into explicit HIR blocks, statements, and
//! terminators.

use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, ForLoopRange, NodeKind, TextLocation,
};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirStatement, HirStatementKind, HirTerminator,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::return_hir_transformation_error;

use crate::hir_log;

mod control_flow;
mod declarations;
mod for_loop_lowering;

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

            NodeKind::Warning(_) | NodeKind::Empty | NodeKind::Newline | NodeKind::Spaces(_) => {
                Ok(())
            }

            NodeKind::Return(_) => return_hir_transformation_error!(
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
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.resolve_function_id_or_error(function_name, location)?;

        self.enter_function(function_id, location)?;

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

            NodeKind::FunctionCall {
                name,
                args,
                result_types: _,
                location,
            } => {
                let function_id = self.resolve_function_id_or_error(name, location)?;
                self.lower_call_statement(CallTarget::UserFunction(function_id), args, location)
            }

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

            NodeKind::ForLoop(binding, range, body) => {
                self.lower_for_statement(binding, range, body, &node.location)
            }

            NodeKind::Warning(_)
            | NodeKind::Operator(_)
            | NodeKind::Empty
            | NodeKind::Newline
            | NodeKind::Spaces(_) => Ok(()),

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
        location: &TextLocation,
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
        location: &TextLocation,
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

    fn lower_call_statement(
        &mut self,
        target: CallTarget,
        args: &[Expression],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let mut lowered_args = Vec::with_capacity(args.len());

        for arg in args {
            let lowered = self.lower_expression(arg)?;
            for prelude in lowered.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }
            lowered_args.push(lowered.value);
        }

        self.emit_statement_kind(
            HirStatementKind::Call {
                target,
                args: lowered_args,
                result: None,
            },
            location,
        )
    }

    fn lower_expression_statement(
        &mut self,
        expression: &Expression,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let lowered = self.lower_expression(expression)?;

        for prelude in lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        if self.is_unit_type(lowered.value.ty) {
            return Ok(());
        }

        self.emit_statement_kind(HirStatementKind::Expr(lowered.value), location)
    }

    fn lower_field_access_statement(
        &mut self,
        field_access_node: &AstNode,
        location: &TextLocation,
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

    fn lower_for_statement(
        &mut self,
        binding: &Declaration,
        range: &ForLoopRange,
        body: &[AstNode],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        // For-loop lowering is intentionally split into a dedicated submodule to keep this file
        // focused on statement dispatch and shared lowering helpers.
        self.lower_for_statement_impl(binding, range, body, location)
    }

    fn emit_statement_kind(
        &mut self,
        kind: HirStatementKind,
        location: &TextLocation,
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

    fn log_block_created(&self, _block_id: BlockId, _label: &str, _location: &TextLocation) {
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
        _location: &TextLocation,
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
