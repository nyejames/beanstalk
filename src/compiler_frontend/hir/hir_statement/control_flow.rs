//! Control-flow lowering helpers for HIR statements.
//!
//! WHAT: lowers structured control-flow constructs into explicit CFG blocks and terminators.
//! WHY: if/match/loop lowering is the densest CFG-building logic in HIR and benefits from a
//! dedicated module boundary.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_builder::{HirBuilder, LoopTargets};
use crate::compiler_frontend::hir::hir_datatypes::{HirTypeKind, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirBlock, HirExpression, HirExpressionKind, HirMatchArm, HirPattern, HirRegion,
    HirTerminator, ValueKind,
};
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::return_hir_transformation_error;

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_return_statement(
        &mut self,
        values: &[Expression],
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let function_id = self.current_function_id_or_error(location)?;
        let return_aliases = self
            .function_by_id_or_error(function_id, location)?
            .return_aliases
            .clone();
        let mut lowered_values = Vec::with_capacity(values.len());

        for (return_index, value) in values.iter().enumerate() {
            let lowered = self.lower_expression(value)?;
            for prelude in lowered.prelude {
                self.emit_statement_to_current_block(prelude, location)?;
            }

            let should_alias = return_aliases
                .get(return_index)
                .and_then(|candidates| candidates.as_ref())
                .is_some();

            let lowered_value = if should_alias {
                match lowered.value.kind {
                    HirExpressionKind::Load(_) => lowered.value,
                    _ => {
                        return_hir_transformation_error!(
                            "Explicit alias returns must return a place expression",
                            self.hir_error_location(location)
                        )
                    }
                }
            } else {
                match lowered.value.kind {
                    HirExpressionKind::Load(place) => self.make_expression(
                        location,
                        HirExpressionKind::Copy(place),
                        lowered.value.ty,
                        ValueKind::RValue,
                        lowered.value.region,
                    ),
                    _ => lowered.value,
                }
            };

            lowered_values.push(lowered_value);
        }

        let return_value = self.expression_from_return_values(&lowered_values, location)?;
        let current_block = self.current_block_id_or_error(location)?;

        self.emit_terminator(current_block, HirTerminator::Return(return_value), location)
    }

    pub(super) fn lower_if_statement(
        &mut self,
        condition: &Expression,
        then_body: &[AstNode],
        else_body: Option<&[AstNode]>,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;
        let condition_lowered = self.lower_expression(condition)?;

        for prelude in condition_lowered.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let parent_region = self.current_region_or_error(location)?;
        let then_region = self.create_child_region(parent_region);
        let else_region = self.create_child_region(parent_region);
        let then_block = self.create_block(then_region, location, "if-then")?;
        let else_block = self.create_block(else_region, location, "if-else")?;

        self.emit_terminator(
            current_block,
            HirTerminator::If {
                condition: condition_lowered.value,
                then_block,
                else_block,
            },
            location,
        )?;

        self.log_control_flow_edge(current_block, then_block, "if.true");
        self.log_control_flow_edge(current_block, else_block, "if.false");

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
        location: &TextLocation,
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
        location: &TextLocation,
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
        location: &TextLocation,
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

        self.emit_terminator(
            header_block,
            HirTerminator::If {
                condition: lowered_condition.value,
                then_block: body_block,
                else_block: exit_block,
            },
            location,
        )?;

        self.log_control_flow_edge(header_block, body_block, "while.true");
        self.log_control_flow_edge(header_block, exit_block, "while.false");

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

    pub(super) fn lower_match_statement(
        &mut self,
        scrutinee: &Expression,
        arms: &[MatchArm],
        default: Option<&[AstNode]>,
        location: &TextLocation,
    ) -> Result<(), CompilerError> {
        let current_block = self.current_block_id_or_error(location)?;

        let lowered_scrutinee = self.lower_expression(scrutinee)?;
        for prelude in lowered_scrutinee.prelude {
            self.emit_statement_to_current_block(prelude, location)?;
        }

        let parent_region = self.current_region_or_error(location)?;
        let mut arm_blocks = Vec::with_capacity(arms.len());
        for _ in arms {
            let arm_region = self.create_child_region(parent_region);
            arm_blocks.push(self.create_block(arm_region, location, "match-arm")?);
        }

        let default_block = if default.is_some() {
            let default_region = self.create_child_region(parent_region);
            Some(self.create_block(default_region, location, "match-default")?)
        } else {
            None
        };
        let mut merge_block = if default.is_none() {
            Some(self.create_block(parent_region, location, "match-merge")?)
        } else {
            None
        };

        let mut hir_arms = Vec::with_capacity(arms.len() + 1);
        for (index, arm) in arms.iter().enumerate() {
            let lowered_pattern = self.lower_match_literal_pattern(&arm.condition)?;

            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Literal(lowered_pattern),
                guard: None,
                body: arm_blocks[index],
            });
        }

        if let Some(default_block_id) = default_block {
            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: default_block_id,
            });
        } else {
            hir_arms.push(HirMatchArm {
                pattern: HirPattern::Wildcard,
                guard: None,
                body: merge_block.expect("match merge block exists when default arm is absent"),
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

        self.set_current_block(current_block, location)
    }

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

    fn ensure_match_merge_block(
        &mut self,
        region: crate::compiler_frontend::hir::hir_nodes::RegionId,
        location: &TextLocation,
        merge_block: &mut Option<BlockId>,
    ) -> Result<BlockId, CompilerError> {
        if let Some(existing) = *merge_block {
            return Ok(existing);
        }

        let created = self.create_block(region, location, "match-merge")?;
        *merge_block = Some(created);
        Ok(created)
    }

    pub(super) fn create_child_region(
        &mut self,
        parent: crate::compiler_frontend::hir::hir_nodes::RegionId,
    ) -> crate::compiler_frontend::hir::hir_nodes::RegionId {
        let region_id = self.allocate_region_id();
        self.push_region(HirRegion::lexical(region_id, Some(parent)));
        region_id
    }

    pub(super) fn create_block(
        &mut self,
        region: crate::compiler_frontend::hir::hir_nodes::RegionId,
        source_location: &TextLocation,
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

    fn expression_from_return_values(
        &mut self,
        values: &[HirExpression],
        location: &TextLocation,
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

    pub(super) fn emit_jump_to(
        &mut self,
        from_block: BlockId,
        target: BlockId,
        location: &TextLocation,
        edge_label: &str,
    ) -> Result<(), CompilerError> {
        self.emit_terminator(
            from_block,
            HirTerminator::Jump {
                target,
                args: vec![],
            },
            location,
        )?;

        self.log_control_flow_edge(from_block, target, edge_label);
        Ok(())
    }

    pub(super) fn emit_terminator(
        &mut self,
        block_id: BlockId,
        terminator: HirTerminator,
        location: &TextLocation,
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
        location: &TextLocation,
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

    pub(super) fn is_unit_type(&self, ty: TypeId) -> bool {
        matches!(self.type_context.get(ty).kind, HirTypeKind::Unit)
    }
}
