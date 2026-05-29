//! Structured runtime template control-flow lowering.
//!
//! WHAT: lowers runtime template `if` and `loop` control flow inline into the enclosing HIR CFG.
//! WHY: branch and loop bodies must remain lazy; eager helper-call arguments would evaluate
//! inactive template content before dispatch.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::match_patterns::MatchPattern;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBodyEmission, TemplateBranchChain, TemplateBranchSelector, TemplateConditionalBranch,
    TemplateControlFlow, TemplateLoopControlFlow, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::hir::expressions::{HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_expression::LoweredExpression;
use crate::compiler_frontend::hir::ids::LocalId;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::loop_aggregate::RuntimeTemplateLoopAggregateAppend;
use super::render_append::RuntimeTemplateAppendContext;

struct RuntimeTemplateBranchChainAppend<'a, 'context> {
    branch_chain: &'a TemplateBranchChain,
    branch_index: usize,
    append_context: RuntimeTemplateAppendContext<'context>,
}

impl<'a> HirBuilder<'a> {
    // WHAT: Lowers structured runtime template control flow directly into the enclosing CFG.
    // WHY: branch content must stay lazy; branch chunks must not be lowered before the condition
    //      chooses the active render path.
    pub(super) fn lower_runtime_control_flow_template_expression(
        &mut self,
        template: &Template,
        location: &SourceLocation,
    ) -> Result<LoweredExpression, CompilerError> {
        if template.has_unresolved_slots() {
            return_hir_transformation_error!(
                "Runtime template control flow reached HIR with unresolved slot placeholders.",
                self.hir_error_location(location)
            );
        }

        if template.contains_slot_insertions() {
            return_hir_transformation_error!(
                "Runtime template control flow reached HIR with unresolved slot insertion helpers.",
                self.hir_error_location(location)
            );
        }

        let accumulator = self.initialize_runtime_template_accumulator(location)?;
        let Some(control_flow) = &template.control_flow else {
            return_hir_transformation_error!(
                "Runtime control-flow template lowering was called for a linear template.",
                self.hir_error_location(location)
            );
        };

        self.append_runtime_template_control_flow_with_emitted_flag(
            template,
            control_flow,
            accumulator,
            None,
            location,
        )?;

        let region = self.current_region_or_error(location)?;
        let value = self.make_expression(
            location,
            HirExpressionKind::Copy(HirPlace::Local(accumulator)),
            builtin_type_ids::STRING,
            ValueKind::RValue,
            region,
        );

        Ok(LoweredExpression {
            prelude: vec![],
            value,
        })
    }

    pub(super) fn append_runtime_template_control_flow_with_emitted_flag(
        &mut self,
        template: &Template,
        control_flow: &TemplateControlFlow,
        accumulator: LocalId,
        emitted_any_iteration: Option<LocalId>,
        location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let append_context = RuntimeTemplateAppendContext::new(accumulator)
            .with_emitted_any_iteration(emitted_any_iteration);

        self.append_runtime_template_control_flow_with_context(
            template,
            control_flow,
            append_context,
            location,
        )
    }

    pub(super) fn append_runtime_template_control_flow_with_context(
        &mut self,
        template: &Template,
        control_flow: &TemplateControlFlow,
        append_context: RuntimeTemplateAppendContext<'_>,
        location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        match control_flow {
            TemplateControlFlow::BranchChain(branch_chain) => {
                self.append_runtime_template_branch_chain(branch_chain, 0, append_context)
            }

            TemplateControlFlow::Loop(template_loop) => {
                self.append_runtime_template_loop(
                    template,
                    template_loop,
                    append_context,
                    location,
                )?;
                Ok(TemplateBodyEmission::Output)
            }

            TemplateControlFlow::LoopControl(signal) => match signal.kind {
                TemplateLoopControlKind::Break => {
                    self.emit_break_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Break)
                }
                TemplateLoopControlKind::Continue => {
                    self.emit_continue_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Continue)
                }
            },
        }
    }

    fn append_runtime_template_branch_chain(
        &mut self,
        branch_chain: &TemplateBranchChain,
        branch_index: usize,
        append_context: RuntimeTemplateAppendContext<'_>,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let Some(branch) = branch_chain.branches.get(branch_index) else {
            return self.append_runtime_template_fallback_branch(branch_chain, append_context);
        };

        match &branch.selector {
            TemplateBranchSelector::Bool(condition) => {
                let Some(branch_plan) = branch.render_plan.as_ref() else {
                    return_hir_transformation_error!(
                        "Runtime template Bool branch reached HIR without a render plan.",
                        self.hir_error_location(&branch.location)
                    );
                };

                self.lower_if_with_body_emitters(
                    condition,
                    &branch.location,
                    |builder| {
                        builder.append_template_render_plan_with_context(
                            branch_plan,
                            append_context,
                            &branch.location,
                        )?;
                        Ok(())
                    },
                    |builder| {
                        builder.append_runtime_template_branch_chain(
                            branch_chain,
                            branch_index + 1,
                            append_context,
                        )?;
                        Ok(())
                    },
                )?;
                Ok(TemplateBodyEmission::Output)
            }

            TemplateBranchSelector::OptionPresentCapture { scrutinee, pattern } => self
                .append_runtime_option_present_branch_chain_arm(
                    branch,
                    scrutinee,
                    pattern,
                    RuntimeTemplateBranchChainAppend {
                        branch_chain,
                        branch_index,
                        append_context,
                    },
                ),
        }
    }

    fn append_runtime_option_present_branch_chain_arm(
        &mut self,
        branch: &TemplateConditionalBranch,
        scrutinee: &Expression,
        pattern: &MatchPattern,
        append: RuntimeTemplateBranchChainAppend<'_, '_>,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let Some(branch_plan) = branch.render_plan.as_ref() else {
            return_hir_transformation_error!(
                "Runtime template option-present branch reached HIR without a render plan.",
                self.hir_error_location(&branch.location)
            );
        };

        self.append_runtime_option_present_template_branch(
            scrutinee,
            pattern,
            &branch.location,
            |builder| {
                builder.append_template_render_plan_with_context(
                    branch_plan,
                    append.append_context,
                    &branch.location,
                )?;
                Ok(())
            },
            |builder| {
                builder.append_runtime_template_branch_chain(
                    append.branch_chain,
                    append.branch_index + 1,
                    append.append_context,
                )?;
                Ok(())
            },
        )?;
        Ok(TemplateBodyEmission::Output)
    }

    fn append_runtime_template_fallback_branch(
        &mut self,
        branch_chain: &TemplateBranchChain,
        append_context: RuntimeTemplateAppendContext<'_>,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let Some(fallback) = &branch_chain.fallback else {
            return Ok(TemplateBodyEmission::NoOutput);
        };

        let Some(fallback_plan) = fallback.render_plan.as_ref() else {
            return_hir_transformation_error!(
                "Runtime template fallback branch reached HIR without a render plan.",
                self.hir_error_location(&fallback.location)
            );
        };

        self.append_template_render_plan_with_context(
            fallback_plan,
            append_context,
            &fallback.location,
        )
    }

    fn append_runtime_template_loop(
        &mut self,
        template: &Template,
        template_loop: &TemplateLoopControlFlow,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let Some(body_plan) = template_loop.body_render_plan.as_ref() else {
            return_hir_transformation_error!(
                "Runtime template loop reached HIR without a per-iteration body render plan.",
                self.hir_error_location(&template_loop.location)
            );
        };
        let Some(aggregate_plan) = template_loop.aggregate_render_plan.as_ref() else {
            return_hir_transformation_error!(
                "Runtime template loop reached HIR without an aggregate render plan.",
                self.hir_error_location(&template_loop.location)
            );
        };

        let aggregate = self.initialize_runtime_template_accumulator(&template_loop.location)?;
        let emitted_any_iteration =
            self.initialize_runtime_template_emitted_flag(&template_loop.location)?;

        match &template_loop.header {
            TemplateLoopHeader::Conditional { condition } => {
                self.lower_while_with_body_emitter(
                    condition,
                    &template_loop.location,
                    |builder| {
                        let iteration_context = append_context
                            .with_target_accumulator(aggregate)
                            .with_emitted_any_iteration(Some(emitted_any_iteration));

                        builder.append_runtime_template_loop_body_iteration(
                            body_plan,
                            iteration_context,
                            &template_loop.location,
                        )
                    },
                )?;
            }

            TemplateLoopHeader::Range { bindings, range } => {
                self.lower_range_loop_with_body_emitter(
                    bindings,
                    range,
                    &template_loop.location,
                    |builder| {
                        let iteration_context = append_context
                            .with_target_accumulator(aggregate)
                            .with_emitted_any_iteration(Some(emitted_any_iteration));

                        builder.append_runtime_template_loop_body_iteration(
                            body_plan,
                            iteration_context,
                            &template_loop.location,
                        )
                    },
                )?;
            }

            TemplateLoopHeader::Collection { bindings, iterable } => {
                self.lower_collection_loop_with_body_emitter(
                    bindings,
                    iterable,
                    &template_loop.location,
                    |builder| {
                        let iteration_context = append_context
                            .with_target_accumulator(aggregate)
                            .with_emitted_any_iteration(Some(emitted_any_iteration));

                        builder.append_runtime_template_loop_body_iteration(
                            body_plan,
                            iteration_context,
                            &template_loop.location,
                        )
                    },
                )?;
            }
        }

        self.append_runtime_template_loop_aggregate_if_emitted(
            template,
            aggregate_plan,
            RuntimeTemplateLoopAggregateAppend {
                aggregate,
                emitted_any_iteration,
                append_context,
            },
            fallback_location,
        )
    }

    fn append_runtime_template_loop_body_iteration(
        &mut self,
        body_plan: &TemplateRenderPlan,
        append_context: RuntimeTemplateAppendContext<'_>,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        self.append_template_render_plan_with_context(body_plan, append_context, location)?;

        Ok(())
    }
}
