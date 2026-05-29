//! Runtime slot application lowering.
//!
//! WHAT: consumes AST-routed `RuntimeSlotApplicationPlan`s and lowers them with
//! ordinary string accumulators.
//! WHY: HIR should execute finalized slot applications, not rediscover or
//! validate source-level `$slot` / `$insert(...)` semantics.

use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBodyEmission;
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotApplicationPlan, RuntimeSlotContributionContent,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_expression::LoweredExpression;
use crate::compiler_frontend::hir::ids::LocalId;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::render_append::{RuntimeSlotAccumulatorContext, RuntimeTemplateAppendContext};

impl<'a> HirBuilder<'a> {
    pub(super) fn lower_runtime_slot_application_template_expression(
        &mut self,
        plan: &RuntimeSlotApplicationPlan,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        let output_accumulator = self.initialize_runtime_template_accumulator(location)?;
        let slot_accumulators = self.initialize_runtime_slot_accumulators(plan, location)?;

        self.append_runtime_slot_contributions(plan, &slot_accumulators, location)?;

        let append_context = RuntimeTemplateAppendContext::new(output_accumulator)
            .with_slot_accumulators(&slot_accumulators);
        let wrapper_emission = self.append_template_render_plan_with_context(
            &plan.wrapper_plan,
            append_context,
            location,
        )?;
        if matches!(
            wrapper_emission,
            TemplateBodyEmission::Break | TemplateBodyEmission::Continue
        ) {
            return_hir_transformation_error!(
                "Runtime slot application wrapper emitted loop control outside a template loop body.",
                self.hir_error_location(location)
            );
        }

        let region = self.current_region_or_error(location)?;
        let value = self.make_expression(
            location,
            HirExpressionKind::Copy(HirPlace::Local(output_accumulator)),
            builtin_type_ids::STRING,
            ValueKind::RValue,
            region,
        );

        Ok(LoweredExpression {
            prelude: vec![],
            value,
        })
    }

    fn initialize_runtime_slot_accumulators(
        &mut self,
        plan: &RuntimeSlotApplicationPlan,
        location: &SourceLocation,
    ) -> Result<RuntimeSlotAccumulatorContext, CompilerError> {
        let ordered_slot_keys = plan
            .contribution_plan
            .schema
            .ordered_slot_keys(self.string_table);
        let mut context = RuntimeSlotAccumulatorContext::new();

        for slot_key in ordered_slot_keys {
            let accumulator = self.initialize_runtime_template_accumulator(location)?;
            context.insert(slot_key, accumulator);
        }

        Ok(context)
    }

    fn append_runtime_slot_contributions(
        &mut self,
        plan: &RuntimeSlotApplicationPlan,
        slot_accumulators: &RuntimeSlotAccumulatorContext,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        for contribution in &plan.contribution_plan.contributions {
            let Some(target_accumulator) = slot_accumulators.local_for(&contribution.target) else {
                return_hir_transformation_error!(
                    "Runtime slot contribution referenced a target with no allocated accumulator.",
                    self.hir_error_location(&contribution.location)
                );
            };

            self.append_runtime_slot_contribution_content(
                &contribution.content,
                target_accumulator,
                fallback_location,
            )?;

            let current_block = self.current_block_id_or_error(fallback_location)?;
            if self.block_has_explicit_terminator(current_block, fallback_location)? {
                break;
            }
        }

        Ok(())
    }

    fn append_runtime_slot_contribution_content(
        &mut self,
        content: &RuntimeSlotContributionContent,
        target_accumulator: LocalId,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let append_context = RuntimeTemplateAppendContext::new(target_accumulator);

        let emission = match content {
            RuntimeSlotContributionContent::Static(content) => {
                let render_plan = TemplateRenderPlan::from_content(content);
                self.append_template_render_plan_with_context(
                    &render_plan,
                    append_context,
                    fallback_location,
                )?
            }

            RuntimeSlotContributionContent::Runtime(render_plan) => self
                .append_template_render_plan_with_context(
                    render_plan,
                    append_context,
                    fallback_location,
                )?,
        };

        if matches!(
            emission,
            TemplateBodyEmission::Break | TemplateBodyEmission::Continue
        ) {
            return_hir_transformation_error!(
                "Runtime slot contribution emitted loop control outside a template loop body.",
                self.hir_error_location(fallback_location)
            );
        }

        Ok(())
    }
}
