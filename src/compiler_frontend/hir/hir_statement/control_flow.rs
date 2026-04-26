//! Control-flow lowering helpers for HIR statements.
//!
//! WHAT: lowers structured control-flow constructs into explicit CFG blocks and terminators.
//! WHY: if/match/loop lowering is the densest CFG-building logic in HIR and benefits from a
//! dedicated module boundary.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, MatchExhaustiveness};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::{
    MatchArm, MatchPattern, RelationalPatternOp,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::{HirBuilder, LoopTargets};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::ids::BlockId;
use crate::compiler_frontend::hir::patterns::{HirMatchArm, HirPattern, HirRelationalPatternOp};
use crate::compiler_frontend::hir::regions::HirRegion;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

fn lower_relational_pattern_op(op: RelationalPatternOp) -> HirRelationalPatternOp {
    match op {
        RelationalPatternOp::LessThan => HirRelationalPatternOp::LessThan,
        RelationalPatternOp::LessThanOrEqual => HirRelationalPatternOp::LessThanOrEqual,
        RelationalPatternOp::GreaterThan => HirRelationalPatternOp::GreaterThan,
        RelationalPatternOp::GreaterThanOrEqual => HirRelationalPatternOp::GreaterThanOrEqual,
    }
}

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_scoped_block_statement(
        &mut self,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let entry_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;
        let body_region = self.create_child_region(parent_region);
        let body_block = self.create_block(body_region, location, "scoped-block")?;

        self.emit_jump_to(entry_block, body_block, location, "block.enter")?;
        self.set_current_block(body_block, location)?;
        self.lower_statement_sequence(body)?;

        let body_tail_block = self.current_block_id_or_error(location)?;
        if self.block_has_explicit_terminator(body_tail_block, location)? {
            return self.set_current_block(body_tail_block, location);
        }

        let after_block = self.create_block(parent_region, location, "scoped-block-after")?;
        self.emit_jump_to(body_tail_block, after_block, location, "block.exit")?;
        self.set_current_block(after_block, location)
    }

    pub(super) fn lower_if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[AstNode],
        else_body: Option<&[AstNode]>,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let condition_lowered = self.lower_expression(condition)?;

        for prelude in condition_lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let condition_block = self.current_block_id_or_error(location)?;

        let parent_region = self.current_region_or_error(location)?;
        let then_region = self.create_child_region(parent_region);
        let else_region = self.create_child_region(parent_region);
        let then_block = self.create_block(then_region, location, "if-then")?;
        let else_block = self.create_block(else_region, location, "if-else")?;

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition: condition_lowered.value,
                then_block,
                else_block,
            },
            location,
        )?;

        self.log_control_flow_edge(condition_block, then_block, "if.true");
        self.log_control_flow_edge(condition_block, else_block, "if.false");

        let mut terminated_anchor: Option<BlockId> = None;

        self.set_current_block(then_block, location)?;
        self.lower_statement_sequence(then_body)?;
        let then_tail_block = self.current_block_id_or_error(location)?;
        let then_terminated = self.block_has_explicit_terminator(then_tail_block, location)?;
        if then_terminated {
            terminated_anchor = Some(then_tail_block);
        }

        self.set_current_block(else_block, location)?;
        if let Some(else_nodes) = else_body {
            self.lower_statement_sequence(else_nodes)?;
        }

        let else_tail_block = self.current_block_id_or_error(location)?;
        let else_terminated = self.block_has_explicit_terminator(else_tail_block, location)?;
        if else_terminated && terminated_anchor.is_none() {
            terminated_anchor = Some(else_tail_block);
        }

        if then_terminated && else_terminated {
            // No continuation path exists after this branch.
            return self.set_current_block(terminated_anchor.unwrap_or(then_block), location);
        }

        let merge_block = self.create_block(parent_region, location, "if-merge")?;
        if !then_terminated {
            self.emit_jump_to(then_tail_block, merge_block, location, "if.then.merge")?;
        }
        if !else_terminated {
            self.emit_jump_to(else_tail_block, merge_block, location, "if.else.merge")?;
        }

        self.set_current_block(merge_block, location)
    }

    pub(super) fn lower_break_statement(
        &mut self,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let targets = self.current_loop_targets_or_error("break", location)?;

        self.emit_terminator(
            current_block,
            HirTerminator::Break {
                target: targets.break_target,
            },
            location,
        )?;

        self.log_control_flow_edge(current_block, targets.break_target, "loop.break");
        Ok(())
    }

    pub(super) fn lower_continue_statement(
        &mut self,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let targets = self.current_loop_targets_or_error("continue", location)?;

        self.emit_terminator(
            current_block,
            HirTerminator::Continue {
                target: targets.continue_target,
            },
            location,
        )?;

        self.log_control_flow_edge(current_block, targets.continue_target, "loop.continue");
        Ok(())
    }

    pub(super) fn lower_while_statement(
        &mut self,
        condition: &Expression,
        body: &[AstNode],
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let pre_header_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;

        let header_block = self.create_block(parent_region, location, "while-header")?;
        let body_region = self.create_child_region(parent_region);
        let body_block = self.create_block(body_region, location, "while-body")?;
        let exit_block = self.create_block(parent_region, location, "while-exit")?;

        self.emit_jump_to(pre_header_block, header_block, location, "while.enter")?;

        self.set_current_block(header_block, location)?;
        let lowered_condition = self.lower_expression(condition)?;
        for prelude in lowered_condition.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let condition_block = self.current_block_id_or_error(location)?;

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition: lowered_condition.value,
                then_block: body_block,
                else_block: exit_block,
            },
            location,
        )?;

        self.log_control_flow_edge(condition_block, body_block, "while.true");
        self.log_control_flow_edge(condition_block, exit_block, "while.false");

        self.set_current_block(body_block, location)?;
        self.push_loop_targets(exit_block, header_block);
        self.lower_statement_sequence(body)?;
        self.pop_loop_targets();

        let body_tail_block = self.current_block_id_or_error(location)?;
        if !self.block_has_explicit_terminator(body_tail_block, location)? {
            self.emit_jump_to(body_tail_block, header_block, location, "while.backedge")?;
        }

        self.set_current_block(exit_block, location)
    }

    /// Lower an AST match statement into explicit CFG blocks and a `Match` terminator.
    ///
    /// WHAT: creates a block per arm (plus optional default and merge blocks), emits
    /// the `HirTerminator::Match`, then lowers each arm body and wires non-terminal
    /// arms to a shared merge block.
    /// WHY: HIR represents control flow as a flat block graph, so structured match
    /// syntax must be decomposed here. Lazy merge-block creation avoids empty blocks
    /// when every arm terminates explicitly.
    pub(super) fn lower_match_statement(
        &mut self,
        scrutinee: &Expression,
        arms: &[MatchArm],
        default: Option<&[AstNode]>,
        exhaustiveness: MatchExhaustiveness,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        match exhaustiveness {
            MatchExhaustiveness::HasDefault if default.is_none() => {
                return_hir_transformation_error!(
                    "Match marked as having a default arm but no default body was provided",
                    self.hir_error_location(location)
                );
            }
            MatchExhaustiveness::ExhaustiveChoice if default.is_some() => {
                return_hir_transformation_error!(
                    "Match marked as exhaustive choice but also provided a default arm",
                    self.hir_error_location(location)
                );
            }
            _ => {}
        }

        let lowered_scrutinee = self.lower_expression(scrutinee)?;
        for prelude in lowered_scrutinee.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }
        let current_block = self.current_block_id_or_error(location)?;

        let parent_region = self.current_region_or_error(location)?;
        let mut arm_blocks = Vec::with_capacity(arms.len());
        for _ in arms {
            let arm_region = self.create_child_region(parent_region);
            arm_blocks.push(self.create_block(arm_region, location, "match-arm")?);
        }

        // AST owns exhaustiveness validation; HIR only lowers the contract it receives.
        let default_block = match exhaustiveness {
            MatchExhaustiveness::HasDefault => {
                let default_region = self.create_child_region(parent_region);
                Some(self.create_block(default_region, location, "match-default")?)
            }
            MatchExhaustiveness::ExhaustiveChoice => None,
        };
        let mut merge_block = None;

        let mut hir_arms = Vec::with_capacity(arms.len() + 1);
        for (index, arm) in arms.iter().enumerate() {
            let lowered_pattern = self.lower_match_pattern(&arm.pattern, &scrutinee.data_type)?;
            let lowered_guard = match &arm.guard {
                Some(guard) => Some(self.lower_match_guard_expression(guard)?),
                None => None,
            };

            hir_arms.push(HirMatchArm {
                pattern: lowered_pattern,
                guard: lowered_guard,
                body: arm_blocks[index],
            });
        }

        if let Some(default_block_id) = default_block {
            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: default_block_id,
            });
        }

        self.emit_terminator(
            current_block,
            HirTerminator::Match {
                scrutinee: lowered_scrutinee.value,
                arms: hir_arms,
            },
            location,
        )?;

        let mut terminated_anchor: Option<BlockId> = None;

        for (index, arm) in arms.iter().enumerate() {
            let arm_block = arm_blocks[index];
            self.set_current_block(arm_block, location)?;
            self.lower_statement_sequence(&arm.body)?;

            let arm_tail_block = self.current_block_id_or_error(location)?;
            let arm_terminated = self.block_has_explicit_terminator(arm_tail_block, location)?;
            if arm_terminated {
                if terminated_anchor.is_none() {
                    terminated_anchor = Some(arm_tail_block);
                }
            } else {
                let merge_target =
                    self.ensure_match_merge_block(parent_region, location, &mut merge_block)?;
                self.emit_jump_to(arm_tail_block, merge_target, location, "match.arm.merge")?;
            }
        }

        if let (Some(default_block_id), Some(default_body)) = (default_block, default) {
            self.set_current_block(default_block_id, location)?;
            self.lower_statement_sequence(default_body)?;

            let default_tail_block = self.current_block_id_or_error(location)?;
            let default_terminated =
                self.block_has_explicit_terminator(default_tail_block, location)?;
            if default_terminated {
                if terminated_anchor.is_none() {
                    terminated_anchor = Some(default_tail_block);
                }
            } else {
                let merge_target =
                    self.ensure_match_merge_block(parent_region, location, &mut merge_block)?;
                self.emit_jump_to(
                    default_tail_block,
                    merge_target,
                    location,
                    "match.default.merge",
                )?;
            }
        }

        if let Some(merge_block_id) = merge_block {
            return self.set_current_block(merge_block_id, location);
        }

        if let Some(anchor_block) = terminated_anchor {
            return self.set_current_block(anchor_block, location);
        }

        return_hir_transformation_error!(
            "Match lowering produced no merge block and no terminated anchor block",
            self.hir_error_location(location)
        )
    }

    /// Validate and lower a match arm pattern, rejecting non-literal expressions.
    ///
    /// WHAT: lowers the pattern expression and verifies it has no side-effect prelude,
    /// is a compile-time constant, and is one of the supported literal kinds.
    /// WHY: match dispatch relies on constant comparison values; catching non-literals
    /// here produces clear HIR-stage errors instead of miscompilation.
    fn lower_match_literal_pattern(
        &mut self,
        condition: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        let lowered_pattern = self.lower_expression(condition)?;
        if !lowered_pattern.prelude.is_empty() {
            return_hir_transformation_error!(
                "Match arm pattern lowering produced side-effect statements; only literal patterns are supported",
                self.hir_error_location(&condition.location)
            );
        }

        if lowered_pattern.value.value_kind != ValueKind::Const {
            return_hir_transformation_error!(
                "Match arm patterns must be compile-time literals",
                self.hir_error_location(&condition.location)
            );
        }

        if !matches!(
            lowered_pattern.value.kind,
            HirExpressionKind::Int(_)
                | HirExpressionKind::Float(_)
                | HirExpressionKind::Bool(_)
                | HirExpressionKind::Char(_)
                | HirExpressionKind::StringLiteral(_)
        ) {
            return_hir_transformation_error!(
                "Match arm patterns currently support only literal int/float/bool/char/string values",
                self.hir_error_location(&condition.location)
            );
        }

        Ok(lowered_pattern.value)
    }

    /// Lower an AST match pattern into its HIR counterpart.
    fn lower_match_pattern(
        &mut self,
        pattern: &MatchPattern,
        subject_type: &DataType,
    ) -> Result<HirPattern, CompilerError> {
        match pattern {
            MatchPattern::Literal(expression) => {
                let lowered = self.lower_match_literal_pattern(expression)?;
                Ok(HirPattern::Literal(lowered))
            }

            MatchPattern::Wildcard { .. } => Ok(HirPattern::Wildcard),

            MatchPattern::Relational { op, value, .. } => {
                let lowered_value = self.lower_match_literal_pattern(value)?;

                Ok(HirPattern::Relational {
                    op: lower_relational_pattern_op(*op),
                    value: lowered_value,
                })
            }
            MatchPattern::ChoiceVariant {
                nominal_path,
                tag,
                location,
                ..
            } => {
                let DataType::Choices { variants, .. } = subject_type else {
                    return_hir_transformation_error!(
                        "ChoiceVariant pattern used with non-choice scrutinee type",
                        self.hir_error_location(location)
                    );
                };
                let choice_id =
                    self.resolve_or_create_choice_id(nominal_path, variants, location)?;
                Ok(HirPattern::ChoiceVariant {
                    choice_id,
                    variant_index: *tag,
                })
            }
        }
    }

    /// Lower a match arm guard and ensure it remains a pure boolean expression.
    fn lower_match_guard_expression(
        &mut self,
        guard: &Expression,
    ) -> Result<HirExpression, CompilerError> {
        let lowered_guard = self.lower_expression(guard)?;
        if !lowered_guard.prelude.is_empty() {
            return_hir_transformation_error!(
                "Match arm guard lowering produced side-effect statements; guards must stay pure boolean expressions",
                self.hir_error_location(&guard.location)
            );
        }

        let HirTypeKind::Bool = self.type_context.get(lowered_guard.value.ty).kind else {
            return_hir_transformation_error!(
                "Match arm guards must lower to Bool expressions",
                self.hir_error_location(&guard.location)
            );
        };

        Ok(lowered_guard.value)
    }

    /// Lazily create the shared merge block on the first non-terminal arm that needs it.
    fn ensure_match_merge_block(
        &mut self,
        region: crate::compiler_frontend::hir::ids::RegionId,
        location: &SourceLocation,
        merge_block: &mut Option<BlockId>,
    ) -> Result<BlockId, CompilerError> {
        if let Some(existing) = *merge_block {
            return Ok(existing);
        }

        let created = self.create_block(region, location, "match-merge")?;
        *merge_block = Some(created);
        Ok(created)
    }

    pub(crate) fn create_child_region(
        &mut self,
        parent: crate::compiler_frontend::hir::ids::RegionId,
    ) -> crate::compiler_frontend::hir::ids::RegionId {
        let region_id = self.allocate_region_id();
        self.push_region(HirRegion::lexical(region_id, Some(parent)));
        region_id
    }

    pub(crate) fn create_block(
        &mut self,
        region: crate::compiler_frontend::hir::ids::RegionId,
        source_location: &SourceLocation,
        label: &str,
    ) -> Result<BlockId, CompilerError> {
        let block = HirBlock {
            id: self.allocate_block_id(),
            region,
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Panic { message: None },
        };

        self.side_table.map_block(source_location, &block);
        self.log_block_created(block.id, label, source_location);

        let id = block.id;
        self.push_block(block);
        Ok(id)
    }

    pub(super) fn expression_from_return_values(
        &mut self,
        values: &[HirExpression],
        location: &SourceLocation,
    ) -> Result<HirExpression, CompilerError> {
        let region = self.current_region_or_error(location)?;

        match values {
            [] => Ok(self.unit_expression(location, region)),
            [single] => Ok(single.to_owned()),
            many => {
                let field_types = many.iter().map(|value| value.ty).collect::<Vec<_>>();
                let tuple_type = self.intern_type_kind(HirTypeKind::Tuple {
                    fields: field_types,
                });

                Ok(self.make_expression(
                    location,
                    HirExpressionKind::TupleConstruct {
                        elements: many.to_vec(),
                    },
                    tuple_type,
                    ValueKind::RValue,
                    region,
                ))
            }
        }
    }

    pub(crate) fn emit_jump_to(
        &mut self,
        from_block: BlockId,
        target: BlockId,
        location: &SourceLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.emit_jump_with_args(from_block, target, vec![], location, edge_label)
    }

    pub(crate) fn emit_jump_with_args(
        &mut self,
        from_block: BlockId,
        target: BlockId,
        args: Vec<crate::compiler_frontend::hir::ids::LocalId>,
        location: &SourceLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.emit_terminator(from_block, HirTerminator::Jump { target, args }, location)?;

        self.log_control_flow_edge(from_block, target, edge_label);
        Ok(())
    }

    pub(crate) fn emit_terminator(
        &mut self,
        block_id: BlockId,
        terminator: HirTerminator,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        self.log_terminator_emitted(block_id, &terminator, location);
        self.set_block_terminator(block_id, terminator, location)
    }

    pub(super) fn push_loop_targets(&mut self, break_target: BlockId, continue_target: BlockId) {
        self.loop_targets.push(LoopTargets {
            break_target,
            continue_target,
        });
    }

    pub(super) fn pop_loop_targets(&mut self) {
        let _ = self.loop_targets.pop();
    }

    pub(super) fn current_loop_targets_or_error(
        &self,
        keyword: &str,
        location: &SourceLocation,
    ) -> Result<LoopTargets, CompilerError> {
        let Some(targets) = self.loop_targets.last().copied() else {
            return_hir_transformation_error!(
                format!(
                    "'{}' reached HIR lowering without an active loop context",
                    keyword
                ),
                self.hir_error_location(location)
            );
        };

        Ok(targets)
    }

    pub(crate) fn is_unit_type(&self, ty: TypeId) -> bool {
        matches!(self.type_context.get(ty).kind, HirTypeKind::Unit)
    }
}
