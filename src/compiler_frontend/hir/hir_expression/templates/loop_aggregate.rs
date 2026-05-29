//! Runtime template loop aggregate lowering.
//!
//! WHAT: appends a loop aggregate render plan only when at least one iteration emitted output.
//! WHY: template loop heads/wrappers apply to the aggregate once, while zero-iteration loops
//! preserve structural no-output semantics.

use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopAggregateRenderPlan;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::ids::LocalId;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use super::render_append::RuntimeTemplateAppendContext;

pub(super) struct RuntimeTemplateLoopAggregateAppend<'context> {
    pub(super) aggregate: LocalId,
    pub(super) emitted_any_iteration: LocalId,
    pub(super) append_context: RuntimeTemplateAppendContext<'context>,
}

impl<'a> HirBuilder<'a> {
    pub(super) fn append_runtime_template_loop_aggregate_if_emitted(
        &mut self,
        template: &Template,
        aggregate_plan: &TemplateLoopAggregateRenderPlan,
        append: RuntimeTemplateLoopAggregateAppend,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let condition_block = self.current_block_id_or_error(fallback_location)?;
        let parent_region = self.current_region_or_error(fallback_location)?;
        let then_region = self.create_child_region(parent_region);
        let else_region = self.create_child_region(parent_region);
        let then_block =
            self.create_block(then_region, fallback_location, "template-loop-emitted")?;
        let else_block =
            self.create_block(else_region, fallback_location, "template-loop-skipped")?;
        let condition = self.make_local_load_expression(
            append.emitted_any_iteration,
            builtin_type_ids::BOOL,
            fallback_location,
            parent_region,
        );

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition,
                then_block,
                else_block,
            },
            fallback_location,
        )?;
        self.set_current_block(then_block, fallback_location)?;
        if let Some(parent_flag) = append.append_context.emitted_any_iteration() {
            self.mark_runtime_template_loop_iteration_emitted(parent_flag, fallback_location)?;
        }
        self.append_template_loop_aggregate_plan_with_context(
            template,
            aggregate_plan,
            append.aggregate,
            append.append_context,
            fallback_location,
        )?;

        let then_tail_block = self.current_block_id_or_error(fallback_location)?;
        let then_terminated =
            self.block_has_explicit_terminator(then_tail_block, fallback_location)?;

        self.set_current_block(else_block, fallback_location)?;
        let else_tail_block = self.current_block_id_or_error(fallback_location)?;

        if then_terminated {
            return self.set_current_block(else_tail_block, fallback_location);
        }

        let merge_block =
            self.create_block(parent_region, fallback_location, "template-loop-merge")?;
        self.emit_jump_to(
            then_tail_block,
            merge_block,
            fallback_location,
            "template-loop.emitted.merge",
        )?;
        self.emit_jump_to(
            else_tail_block,
            merge_block,
            fallback_location,
            "template-loop.skipped.merge",
        )?;

        self.set_current_block(merge_block, fallback_location)
    }

    pub(super) fn initialize_runtime_template_emitted_flag(
        &mut self,
        location: &SourceLocation,
    ) -> Result<LocalId, CompilerError> {
        let flag = self.allocate_temp_local(builtin_type_ids::BOOL, Some(location.clone()))?;
        let region = self.current_region_or_error(location)?;
        let false_value = self.make_expression(
            location,
            HirExpressionKind::Bool(false),
            builtin_type_ids::BOOL,
            ValueKind::Const,
            region,
        );

        self.emit_statement_kind(
            crate::compiler_frontend::hir::statements::HirStatementKind::Assign {
                target: HirPlace::Local(flag),
                value: false_value,
            },
            location,
        )?;

        Ok(flag)
    }

    pub(super) fn mark_runtime_template_loop_iteration_emitted(
        &mut self,
        emitted_any_iteration: LocalId,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let region = self.current_region_or_error(location)?;
        let true_value = self.make_expression(
            location,
            HirExpressionKind::Bool(true),
            builtin_type_ids::BOOL,
            ValueKind::Const,
            region,
        );

        self.emit_statement_kind(
            crate::compiler_frontend::hir::statements::HirStatementKind::Assign {
                target: HirPlace::Local(emitted_any_iteration),
                value: true_value,
            },
            location,
        )
    }
}
