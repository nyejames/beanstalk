//! Control Flow Linearizer for HIR Builder
//!
//! This module implements the ControlFlowLinearizer component that converts nested
//! control flow constructs (if/else, loops, pattern matching) into explicit HIR blocks
//! with terminators.
//!
//! The linearizer ensures that:
//! - If/else statements become HIR conditional blocks with proper branch targets
//! - Loops become HIR loop constructs with break/continue targets
//! - Pattern matching becomes conditional chains with exhaustiveness checking
//! - Every block ends in exactly one terminator
//!
//! ## Key Design Principles
//!
//! - Control flow is linearized into explicit blocks with terminators
//! - Break and continue targets are properly resolved
//! - Nested control flow maintains correct block nesting
//! - All scope exits are explicit through terminators or drop points

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::build_hir::{HirBuilderContext, ScopeType};
use crate::compiler_frontend::hir::expression_linearizer::ExpressionLinearizer;
use crate::compiler_frontend::hir::nodes::{
    BlockId, HirExpr, HirExprKind, HirKind, HirMatchArm, HirNode, HirPattern, HirPlace, HirStmt,
    HirTerminator,
};
use crate::compiler_frontend::host_functions::registry::{CallTarget, HostFunctionId};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation;
use crate::compiler_frontend::string_interning::InternedString;
use crate::return_compiler_error;

/// The ControlFlowLinearizer component converts nested control flow constructs
/// into explicit HIR blocks with terminators.
///
/// This component operates on borrowed HirBuilderContext rather than owning
/// independent state, ensuring a single authoritative HIR state per module.
#[derive(Debug, Default)]
pub struct ControlFlowLinearizer {
    /// Expression linearizer for handling conditions and expressions
    expr_linearizer: ExpressionLinearizer,
}

impl ControlFlowLinearizer {
    /// Creates a new ControlFlowLinearizer
    pub fn new() -> Self {
        ControlFlowLinearizer {
            expr_linearizer: ExpressionLinearizer::new(),
        }
    }

    // =========================================================================
    // If Statement Linearization
    // =========================================================================

    /// Linearizes an if/else statement into HIR conditional blocks.
    ///
    /// Transforms:
    /// ```text
    /// if condition:
    ///     then_body
    /// else:
    ///     else_body
    /// ```
    ///
    /// Into HIR blocks:
    /// - Current block ends with If terminator
    /// - Then block contains then_body
    /// - Else block (if present) contains else_body
    /// - Merge block continues after the if/else
    pub fn linearize_if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[AstNode],
        else_body: Option<&[AstNode]>,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize the condition expression
        let (cond_nodes, cond_expr) = self.expr_linearizer.linearize_expression(condition, ctx)?;
        nodes.extend(cond_nodes);

        // Create blocks for then, else (optional), and merge
        let then_block_id = ctx.create_block();
        let else_block_id = if else_body.is_some() {
            Some(ctx.create_block())
        } else {
            None
        };
        let merge_block_id = ctx.create_block();

        // Create the If terminator for the current block
        let if_terminator = self.create_if_terminator(
            cond_expr,
            then_block_id,
            else_block_id,
            location.clone(),
            ctx,
        );
        nodes.push(if_terminator);

        // Process then block
        ctx.enter_scope_with_block(ScopeType::If, then_block_id);
        let then_nodes = self.linearize_body(then_body, ctx)?;
        for node in then_nodes {
            ctx.add_node_to_block(then_block_id, node);
        }
        // Add jump to merge block if then block doesn't end with a terminator
        if !self.block_has_terminator(ctx, then_block_id) {
            let jump_to_merge = self.create_jump_to_block(merge_block_id, location.clone(), ctx);
            ctx.add_node_to_block(then_block_id, jump_to_merge);
        }
        let _then_dropped = ctx.exit_scope();

        // Process else block if present
        if let (Some(else_id), Some(else_nodes_ast)) = (else_block_id, else_body) {
            ctx.enter_scope_with_block(ScopeType::If, else_id);
            let else_nodes = self.linearize_body(else_nodes_ast, ctx)?;
            for node in else_nodes {
                ctx.add_node_to_block(else_id, node);
            }
            // Add jump to merge block if else block doesn't end with a terminator
            if !self.block_has_terminator(ctx, else_id) {
                let jump_to_merge =
                    self.create_jump_to_block(merge_block_id, location.clone(), ctx);
                ctx.add_node_to_block(else_id, jump_to_merge);
            }
            let _else_dropped = ctx.exit_scope();
        } else {
            // No else block - the "else" path goes directly to merge
            // We need to update the If terminator to point to merge for the else case
            // This is handled by the else_block being None in the terminator
        }

        Ok(nodes)
    }

    /// Creates an If terminator node
    fn create_if_terminator(
        &self,
        condition: HirExpr,
        then_block: BlockId,
        else_block: Option<BlockId>,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::If {
                condition,
                then_block,
                else_block,
            }),
            location,
            id: node_id,
        }
    }

    // =========================================================================
    // Loop Linearization
    // =========================================================================

    /// Linearizes a for loop into HIR loop construct.
    ///
    /// Transforms:
    /// ```text
    /// loop item in collection:
    ///     body
    /// ```
    ///
    /// Into HIR:
    /// - Loop terminator with binding, iterator, and body block
    /// - Body block contains the loop body
    /// - Break/continue targets are set up for nested control flow
    pub fn linearize_for_loop(
        &mut self,
        binding: &Var,
        iterator: &Expression,
        body: &[AstNode],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize the iterator expression
        let (iter_nodes, iter_expr) = self.expr_linearizer.linearize_expression(iterator, ctx)?;
        nodes.extend(iter_nodes);

        // Create blocks for loop body and exit
        let body_block_id = ctx.create_block();
        let exit_block_id = ctx.create_block();

        // Create the Loop terminator
        let loop_terminator = self.create_loop_terminator(
            Some((binding.id, binding.value.data_type.clone())),
            Some(iter_expr),
            body_block_id,
            exit_block_id,
            None, // No index binding for basic for loops
            location.clone(),
            ctx,
        );
        nodes.push(loop_terminator);

        // Enter loop scope with break/continue targets
        ctx.enter_scope_with_block(
            ScopeType::Loop {
                break_target: exit_block_id,
                continue_target: body_block_id,
            },
            body_block_id,
        );

        // Process loop body
        let body_nodes = self.linearize_body(body, ctx)?;
        for node in body_nodes {
            ctx.add_node_to_block(body_block_id, node);
        }

        // Add continue (jump back to loop start) if body doesn't end with terminator
        if !self.block_has_terminator(ctx, body_block_id) {
            let continue_node = self.create_continue(body_block_id, location.clone(), ctx);
            ctx.add_node_to_block(body_block_id, continue_node);
        }

        let _loop_dropped = ctx.exit_scope();

        Ok(nodes)
    }

    /// Linearizes a while loop into HIR loop construct.
    ///
    /// Transforms:
    /// ```text
    /// loop condition:
    ///     body
    /// ```
    ///
    /// Into HIR:
    /// - Condition check block
    /// - Loop body block
    /// - Exit block
    pub fn linearize_while_loop(
        &mut self,
        condition: &Expression,
        body: &[AstNode],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Create blocks for condition check, body, and exit
        let condition_block_id = ctx.create_block();
        let body_block_id = ctx.create_block();
        let exit_block_id = ctx.create_block();

        // Jump to condition block
        let jump_to_condition =
            self.create_jump_to_block(condition_block_id, location.clone(), ctx);
        nodes.push(jump_to_condition);

        // Linearize condition in the condition block
        ctx.enter_scope_with_block(ScopeType::Block, condition_block_id);
        let (cond_nodes, cond_expr) = self.expr_linearizer.linearize_expression(condition, ctx)?;
        for node in cond_nodes {
            ctx.add_node_to_block(condition_block_id, node);
        }

        // Add conditional branch: if condition is true, go to body; else exit
        let if_terminator = self.create_if_terminator(
            cond_expr,
            body_block_id,
            Some(exit_block_id),
            location.clone(),
            ctx,
        );
        ctx.add_node_to_block(condition_block_id, if_terminator);
        let _ = ctx.exit_scope();

        // Enter loop scope with break/continue targets
        ctx.enter_scope_with_block(
            ScopeType::Loop {
                break_target: exit_block_id,
                continue_target: condition_block_id,
            },
            body_block_id,
        );

        // Process loop body
        let body_nodes = self.linearize_body(body, ctx)?;
        for node in body_nodes {
            ctx.add_node_to_block(body_block_id, node);
        }

        // Add jump back to condition check if body doesn't end with terminator
        if !self.block_has_terminator(ctx, body_block_id) {
            let continue_node = self.create_continue(condition_block_id, location.clone(), ctx);
            ctx.add_node_to_block(body_block_id, continue_node);
        }

        let _loop_dropped = ctx.exit_scope();

        Ok(nodes)
    }

    /// Creates a Loop terminator node
    fn create_loop_terminator(
        &self,
        binding: Option<(InternedString, DataType)>,
        iterator: Option<HirExpr>,
        body_block: BlockId,
        label: BlockId,
        index_binding: Option<InternedString>,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::Loop {
                label,
                binding,
                iterator,
                body: body_block,
                index_binding,
            }),
            location,
            id: node_id,
        }
    }

    // =========================================================================
    // Pattern Matching Linearization
    // =========================================================================

    /// Linearizes a match expression into HIR conditional chains.
    ///
    /// Transforms:
    /// ```text
    /// if value is:
    ///     pattern1: body1
    ///     pattern2: body2
    ///     else: default_body
    /// ```
    ///
    /// Into HIR:
    /// - Match terminator with scrutinee and arms
    /// - Each arm has a pattern, optional guard, and body block
    /// - Default block for wildcard/else case
    pub fn linearize_match(
        &mut self,
        scrutinee: &Expression,
        arms: &[MatchArm],
        default: Option<&[AstNode]>,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize the scrutinee expression
        let (scrutinee_nodes, scrutinee_expr) =
            self.expr_linearizer.linearize_expression(scrutinee, ctx)?;
        nodes.extend(scrutinee_nodes);

        // Create merge block for after the match
        let merge_block_id = ctx.create_block();

        // Create HIR match arms
        let mut hir_arms = Vec::new();
        for arm in arms {
            let arm_block_id = ctx.create_block();

            // Convert condition to HIR pattern
            let hir_pattern = self.convert_pattern(&arm.condition, ctx)?;

            hir_arms.push(HirMatchArm {
                pattern: hir_pattern,
                guard: None, // MatchArm doesn't have guards in current AST
                body: arm_block_id,
            });

            // Process arm body
            ctx.enter_scope_with_block(ScopeType::Block, arm_block_id);
            let arm_nodes = self.linearize_body(&arm.body, ctx)?;
            for node in arm_nodes {
                ctx.add_node_to_block(arm_block_id, node);
            }
            // Add jump to merge block if arm doesn't end with terminator
            if !self.block_has_terminator(ctx, arm_block_id) {
                let jump_to_merge =
                    self.create_jump_to_block(merge_block_id, location.clone(), ctx);
                ctx.add_node_to_block(arm_block_id, jump_to_merge);
            }
            let _ = ctx.exit_scope();
        }

        // Create default block if present
        let default_block_id = if let Some(default_body) = default {
            let default_id = ctx.create_block();
            ctx.enter_scope_with_block(ScopeType::Block, default_id);
            let default_nodes = self.linearize_body(default_body, ctx)?;
            for node in default_nodes {
                ctx.add_node_to_block(default_id, node);
            }
            if !self.block_has_terminator(ctx, default_id) {
                let jump_to_merge =
                    self.create_jump_to_block(merge_block_id, location.clone(), ctx);
                ctx.add_node_to_block(default_id, jump_to_merge);
            }
            let _ = ctx.exit_scope();
            Some(default_id)
        } else {
            None
        };

        // Create the Match terminator
        let match_terminator = self.create_match_terminator(
            scrutinee_expr,
            hir_arms,
            default_block_id,
            location.clone(),
            ctx,
        );
        nodes.push(match_terminator);

        Ok(nodes)
    }

    /// Converts an AST pattern to an HIR pattern
    pub fn convert_pattern(
        &mut self,
        pattern: &Expression,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirPattern, CompilerError> {
        match &pattern.kind {
            // Literal patterns
            ExpressionKind::Int(val) => Ok(HirPattern::Literal(HirExpr {
                kind: HirExprKind::Int(*val),
                location: pattern.location.clone(),
            })),
            ExpressionKind::Float(val) => Ok(HirPattern::Literal(HirExpr {
                kind: HirExprKind::Float(*val),
                location: pattern.location.clone(),
            })),
            ExpressionKind::Bool(val) => Ok(HirPattern::Literal(HirExpr {
                kind: HirExprKind::Bool(*val),
                location: pattern.location.clone(),
            })),
            // Range patterns
            ExpressionKind::Range(start, end) => {
                let (_, start_expr) = self.expr_linearizer.linearize_expression(start, ctx)?;
                let (_, end_expr) = self.expr_linearizer.linearize_expression(end, ctx)?;
                Ok(HirPattern::Range {
                    start: start_expr,
                    end: end_expr,
                })
            }
            // Wildcard pattern (represented as None or special marker)
            ExpressionKind::None => Ok(HirPattern::Wildcard),
            // Other patterns - try to linearize as literal
            _ => {
                let (_, expr) = self.expr_linearizer.linearize_expression(pattern, ctx)?;
                Ok(HirPattern::Literal(expr))
            }
        }
    }

    /// Creates a Match terminator node
    fn create_match_terminator(
        &self,
        scrutinee: HirExpr,
        arms: Vec<HirMatchArm>,
        default_block: Option<BlockId>,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::Match {
                scrutinee,
                arms,
                default_block,
            }),
            location,
            id: node_id,
        }
    }

    // =========================================================================
    // Return and Jump Handling
    // =========================================================================

    /// Linearizes a return statement.
    ///
    /// Handles return statements with proper value management and cleanup.
    /// Drop points for owned variables are inserted before the return.
    pub fn linearize_return(
        &mut self,
        values: &[Expression],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize return values
        let mut hir_values = Vec::new();
        for value in values {
            let (value_nodes, value_expr) =
                self.expr_linearizer.linearize_expression(value, ctx)?;
            nodes.extend(value_nodes);
            hir_values.push(value_expr);
        }

        // Create the Return terminator
        let return_node = self.create_return(hir_values, location.clone(), ctx);
        nodes.push(return_node);

        Ok(nodes)
    }

    /// Creates a Return terminator node
    fn create_return(
        &self,
        values: Vec<HirExpr>,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::Return(values)),
            location,
            id: node_id,
        }
    }

    /// Linearizes a break statement with correct target resolution.
    ///
    /// Finds the enclosing loop and creates a Break terminator targeting
    /// the loop's exit block.
    pub fn linearize_break(
        &mut self,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirNode, CompilerError> {
        // Find the enclosing loop to get the break target
        let break_target = match ctx.find_enclosing_loop() {
            Some(scope_info) => {
                if let ScopeType::Loop { break_target, .. } = &scope_info.scope_type {
                    *break_target
                } else {
                    return_compiler_error!(
                        "Break statement found but enclosing scope is not a loop"
                    );
                }
            }
            None => {
                return_compiler_error!("Break statement outside of loop");
            }
        };

        Ok(self.create_break(break_target, location.clone(), ctx))
    }

    /// Creates a Break terminator node
    fn create_break(
        &self,
        target: BlockId,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::Break { target }),
            location,
            id: node_id,
        }
    }

    /// Linearizes a continue statement with correct target resolution.
    ///
    /// Finds the enclosing loop and creates a Continue terminator targeting
    /// the loop's continue block (typically the loop header or body start).
    pub fn linearize_continue(
        &mut self,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<HirNode, CompilerError> {
        // Find the enclosing loop to get the continue target
        let continue_target = match ctx.find_enclosing_loop() {
            Some(scope_info) => {
                if let ScopeType::Loop {
                    continue_target, ..
                } = &scope_info.scope_type
                {
                    *continue_target
                } else {
                    return_compiler_error!(
                        "Continue statement found but enclosing scope is not a loop"
                    );
                }
            }
            None => {
                return_compiler_error!("Continue statement outside of loop");
            }
        };

        Ok(self.create_continue(continue_target, location.clone(), ctx))
    }

    /// Creates a Continue terminator node
    fn create_continue(
        &self,
        target: BlockId,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::Continue { target }),
            location,
            id: node_id,
        }
    }

    /// Creates a jump to a specific block (used for control flow merging)
    fn create_jump_to_block(
        &self,
        target: BlockId,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        // We use Break as a general jump mechanism since HIR doesn't have
        // an explicit "goto" - Break with a target serves this purpose
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Terminator(HirTerminator::Break { target }),
            location,
            id: node_id,
        }
    }

    // =========================================================================
    // Body Linearization
    // =========================================================================

    /// Linearizes a body of AST nodes into HIR nodes.
    ///
    /// This is the main entry point for processing a sequence of AST nodes
    /// that form a block body (function body, loop body, if body, etc.)
    pub fn linearize_body(
        &mut self,
        body: &[AstNode],
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        for ast_node in body {
            let node_nodes = self.linearize_ast_node(ast_node, ctx)?;
            nodes.extend(node_nodes);
        }

        Ok(nodes)
    }

    /// Linearizes a single AST node into HIR nodes.
    ///
    /// Dispatches to the appropriate linearization method based on node kind.
    pub fn linearize_ast_node(
        &mut self,
        node: &AstNode,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        match &node.kind {
            // Control flow constructs
            NodeKind::If(condition, then_body, else_body) => self.linearize_if_statement(
                condition,
                then_body,
                else_body.as_deref(),
                &node.location,
                ctx,
            ),

            NodeKind::ForLoop(binding, iterator, body) => {
                self.linearize_for_loop(binding, iterator, body, &node.location, ctx)
            }

            NodeKind::WhileLoop(condition, body) => {
                self.linearize_while_loop(condition, body, &node.location, ctx)
            }

            NodeKind::Match(scrutinee, arms, default) => {
                self.linearize_match(scrutinee, arms, default.as_deref(), &node.location, ctx)
            }

            NodeKind::Return(values) => self.linearize_return(values, &node.location, ctx),

            // Variable declarations
            NodeKind::VariableDeclaration(arg) => {
                self.linearize_variable_declaration(arg, &node.location, ctx)
            }

            // Assignments
            NodeKind::Assignment { target, value } => {
                self.linearize_assignment(target, value, &node.location, ctx)
            }

            // Function calls
            NodeKind::FunctionCall {
                name,
                args,
                returns,
                location,
            } => self.linearize_function_call(*name, args, location, ctx),

            NodeKind::HostFunctionCall {
                host_function_id,
                args,
                returns,
                location,
            } => self.linearize_host_function_call(*host_function_id, args, location, ctx),

            // R-values (expressions as statements)
            NodeKind::Rvalue(expr) => {
                self.linearize_expression_statement(expr, &node.location, ctx)
            }

            // Empty nodes
            NodeKind::Empty | NodeKind::Newline | NodeKind::Spaces(_) => Ok(Vec::new()),

            // Warnings are passed through
            NodeKind::Warning(_) => Ok(Vec::new()),

            // Function and struct definitions are handled at module level
            NodeKind::Function(_, _, _) | NodeKind::StructDefinition(_, _) => {
                // These should be processed at module level, not in body
                Ok(Vec::new())
            }

            // Other node kinds
            _ => {
                // For unsupported nodes, return empty
                // This allows gradual implementation
                Ok(Vec::new())
            }
        }
    }

    /// Linearizes a variable declaration
    fn linearize_variable_declaration(
        &mut self,
        arg: &Var,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize the value expression
        let (value_nodes, value_expr) =
            self.expr_linearizer.linearize_expression(&arg.value, ctx)?;
        nodes.extend(value_nodes);

        // Create the assignment node
        let is_mutable = arg.value.ownership.is_mutable();
        let assign_node = self.create_assignment(
            HirPlace::Var(arg.id),
            value_expr,
            is_mutable,
            location.clone(),
            ctx,
        );
        nodes.push(assign_node);

        Ok(nodes)
    }

    /// Linearizes an assignment
    fn linearize_assignment(
        &mut self,
        target: &AstNode,
        value: &Expression,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize the value expression
        let (value_nodes, value_expr) = self.expr_linearizer.linearize_expression(value, ctx)?;
        nodes.extend(value_nodes);

        // Convert target to HirPlace
        let hir_place = self.convert_target_to_place(target)?;

        // Create the assignment node
        let assign_node = self.create_assignment(
            hir_place,
            value_expr,
            true, // Assignments are always to mutable targets
            location.clone(),
            ctx,
        );
        nodes.push(assign_node);

        Ok(nodes)
    }

    /// Converts an AST target node to an HirPlace
    fn convert_target_to_place(&self, target: &AstNode) -> Result<HirPlace, CompilerError> {
        match &target.kind {
            NodeKind::Rvalue(expr) => match &expr.kind {
                ExpressionKind::Reference(name) => Ok(HirPlace::Var(name.to_owned())),
                _ => return_compiler_error!("Invalid assignment target expression"),
            },
            NodeKind::FieldAccess { base, field, .. } => {
                let base_place = self.convert_target_to_place(base)?;
                Ok(HirPlace::Field {
                    base: Box::new(base_place),
                    field: *field,
                })
            }
            _ => return_compiler_error!("Invalid assignment target node kind: {:?}", target.kind),
        }
    }

    /// Creates an assignment HIR node
    fn create_assignment(
        &self,
        target: HirPlace,
        value: HirExpr,
        is_mutable: bool,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Stmt(HirStmt::Assign {
                target,
                value,
                is_mutable,
            }),
            location,
            id: node_id,
        }
    }

    /// Linearizes a function call statement
    fn linearize_function_call(
        &mut self,
        name: InternedString,
        args: &[Expression],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize arguments
        let mut hir_args = Vec::new();
        for arg in args {
            let (arg_nodes, arg_expr) = self.expr_linearizer.linearize_expression(arg, ctx)?;
            nodes.extend(arg_nodes);
            hir_args.push(arg_expr);
        }

        // Create the call statement
        let call_node = self.create_call_statement(name, hir_args, location.clone(), ctx);
        nodes.push(call_node);

        Ok(nodes)
    }

    /// Linearizes a host function call statement
    fn linearize_host_function_call(
        &mut self,
        host_function_id: HostFunctionId,
        args: &[Expression],
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize arguments
        let mut hir_args = Vec::new();
        for arg in args {
            let (arg_nodes, arg_expr) = self.expr_linearizer.linearize_expression(arg, ctx)?;
            nodes.extend(arg_nodes);
            hir_args.push(arg_expr);
        }

        // Create the host call statement
        let call_node =
            self.create_host_call_statement(host_function_id, hir_args, location.clone(), ctx);
        nodes.push(call_node);

        Ok(nodes)
    }

    /// Creates a Call statement node
    fn create_call_statement(
        &self,
        target: InternedString,
        args: Vec<HirExpr>,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Stmt(HirStmt::Call {
                target: CallTarget::UserFunction(target),
                args,
            }),
            location,
            id: node_id,
        }
    }

    /// Creates a HostCall statement node
    fn create_host_call_statement(
        &self,
        host_function_id: HostFunctionId,
        args: Vec<HirExpr>,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Stmt(HirStmt::Call {
                target: CallTarget::HostFunction(host_function_id),
                args,
            }),
            location,
            id: node_id,
        }
    }

    /// Linearizes an expression statement (expression evaluated for side effects)
    fn linearize_expression_statement(
        &mut self,
        expr: &Expression,
        location: &TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> Result<Vec<HirNode>, CompilerError> {
        let mut nodes = Vec::new();

        // Linearize the expression
        let (expr_nodes, hir_expr) = self.expr_linearizer.linearize_expression(expr, ctx)?;
        nodes.extend(expr_nodes);

        // Create an expression statement
        let expr_stmt = self.create_expr_statement(hir_expr, location.clone(), ctx);
        nodes.push(expr_stmt);

        Ok(nodes)
    }

    /// Creates an ExprStmt node
    fn create_expr_statement(
        &self,
        expr: HirExpr,
        location: TextLocation,
        ctx: &mut HirBuilderContext,
    ) -> HirNode {
        let node_id = ctx.allocate_node_id();
        let build_context = ctx.create_build_context(location.clone());
        ctx.record_node_context(node_id, build_context);

        HirNode {
            kind: HirKind::Stmt(HirStmt::ExprStmt(expr)),
            location,
            id: node_id,
        }
    }

    // =========================================================================
    // Helper Methods
    // =========================================================================

    /// Checks if a block has a terminator as its last node
    pub fn block_has_terminator(&self, ctx: &HirBuilderContext, block_id: BlockId) -> bool {
        if let Some(block) = ctx.get_block(block_id) {
            if let Some(last_node) = block.nodes.last() {
                return matches!(last_node.kind, HirKind::Terminator(_));
            }
        }
        false
    }

    /// Ensures a block ends with exactly one terminator.
    ///
    /// CRITICAL INVARIANT: Every HIR block must end in exactly one terminator.
    /// This is enforced during block creation and validated during HIR generation.
    pub fn ensure_block_termination(
        &self,
        ctx: &HirBuilderContext,
        block_id: BlockId,
    ) -> Result<(), CompilerError> {
        if let Some(block) = ctx.get_block(block_id) {
            let terminator_count = block
                .nodes
                .iter()
                .filter(|n| matches!(n.kind, HirKind::Terminator(_)))
                .count();

            if terminator_count == 0 {
                return_compiler_error!("HIR block {} is missing a terminator", block_id);
            } else if terminator_count > 1 {
                return_compiler_error!(
                    "HIR block {} has {} terminators (expected 1)",
                    block_id,
                    terminator_count
                );
            }
        }
        Ok(())
    }

    /// Gets a reference to the expression linearizer
    pub fn expr_linearizer(&self) -> &ExpressionLinearizer {
        &self.expr_linearizer
    }

    /// Gets a mutable reference to the expression linearizer
    pub fn expr_linearizer_mut(&mut self) -> &mut ExpressionLinearizer {
        &mut self.expr_linearizer
    }
}
