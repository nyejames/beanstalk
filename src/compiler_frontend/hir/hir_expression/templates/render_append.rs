//! Runtime template render-plan append helpers.
//!
//! WHAT: appends prepared render pieces into a string accumulator and performs final string
//! coercion for dynamic chunks.
//! WHY: both inline control-flow templates and loop aggregate wrapping share the same HIR
//! concatenation semantics.

use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBodyEmission, TemplateControlFlow, TemplateLoopAggregatePiece,
    TemplateLoopAggregateRenderPlan, TemplateLoopControlKind,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::hir::expressions::{HirExpression, HirExpressionKind, ValueKind};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::ids::{LocalId, RegionId};
use crate::compiler_frontend::hir::operators::HirBinOp;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;
use rustc_hash::FxHashMap;

use super::loop_aggregate::RuntimeTemplateLoopAggregateAppend;

/// Slot locals available while HIR lowers a runtime slot application wrapper.
///
/// WHAT: maps AST-routed slot keys to already-initialized string accumulators.
/// WHY: render-plan appending should remain a single path; only active runtime
/// slot applications reinterpret `RenderPiece::Slot` as an accumulator append.
pub(super) struct RuntimeSlotAccumulatorContext {
    locals_by_key: FxHashMap<SlotKey, LocalId>,
}

impl RuntimeSlotAccumulatorContext {
    pub(super) fn new() -> Self {
        Self {
            locals_by_key: FxHashMap::default(),
        }
    }

    pub(super) fn insert(&mut self, key: SlotKey, local: LocalId) {
        self.locals_by_key.insert(key, local);
    }

    pub(super) fn local_for(&self, key: &SlotKey) -> Option<LocalId> {
        self.locals_by_key.get(key).copied()
    }
}

/// Append target plus optional runtime-slot state for render-plan lowering.
#[derive(Clone, Copy)]
pub(super) struct RuntimeTemplateAppendContext<'a> {
    target_accumulator: LocalId,
    emitted_any_iteration: Option<LocalId>,
    slot_accumulators: Option<&'a RuntimeSlotAccumulatorContext>,
}

impl<'a> RuntimeTemplateAppendContext<'a> {
    pub(super) fn new(target_accumulator: LocalId) -> Self {
        Self {
            target_accumulator,
            emitted_any_iteration: None,
            slot_accumulators: None,
        }
    }

    pub(super) fn with_emitted_any_iteration(mut self, flag: Option<LocalId>) -> Self {
        self.emitted_any_iteration = flag;
        self
    }

    pub(super) fn with_target_accumulator(mut self, target_accumulator: LocalId) -> Self {
        self.target_accumulator = target_accumulator;
        self
    }

    pub(super) fn with_slot_accumulators(
        mut self,
        slot_accumulators: &'a RuntimeSlotAccumulatorContext,
    ) -> Self {
        self.slot_accumulators = Some(slot_accumulators);
        self
    }

    pub(super) fn target_accumulator(&self) -> LocalId {
        self.target_accumulator
    }

    pub(super) fn emitted_any_iteration(&self) -> Option<LocalId> {
        self.emitted_any_iteration
    }
}

impl<'a> HirBuilder<'a> {
    pub(super) fn initialize_runtime_template_accumulator(
        &mut self,
        location: &SourceLocation,
    ) -> Result<LocalId, CompilerError> {
        let string_ty = builtin_type_ids::STRING;
        let accumulator = self.allocate_temp_local(string_ty, Some(location.clone()))?;
        let region = self.current_region_or_error(location)?;
        let empty_string = self.make_expression(
            location,
            HirExpressionKind::StringLiteral(String::new()),
            string_ty,
            ValueKind::Const,
            region,
        );

        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: HirPlace::Local(accumulator),
                value: empty_string,
            },
            location,
        )?;

        Ok(accumulator)
    }

    pub(super) fn append_template_render_plan_to_accumulator(
        &mut self,
        render_plan: &TemplateRenderPlan,
        accumulator: LocalId,
        fallback_location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let append_context = RuntimeTemplateAppendContext::new(accumulator);
        self.append_template_render_plan_with_context(
            render_plan,
            append_context,
            fallback_location,
        )
    }

    pub(super) fn append_template_loop_aggregate_plan_with_context(
        &mut self,
        template: &Template,
        aggregate_plan: &TemplateLoopAggregateRenderPlan,
        aggregate: LocalId,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        for piece in &aggregate_plan.pieces {
            match piece {
                TemplateLoopAggregatePiece::Render(render_piece) => {
                    self.append_render_piece_to_accumulator(
                        render_piece,
                        append_context,
                        fallback_location,
                    )?;
                }

                TemplateLoopAggregatePiece::Aggregate => {
                    let region = self.current_region_or_error(fallback_location)?;
                    let aggregate_value = self.make_expression(
                        &template.location,
                        HirExpressionKind::Load(HirPlace::Local(aggregate)),
                        builtin_type_ids::STRING,
                        ValueKind::Place,
                        region,
                    );
                    self.append_template_chunk_to_accumulator(
                        aggregate_value,
                        append_context.target_accumulator(),
                        fallback_location,
                    )?;
                }
            }
        }

        Ok(())
    }

    pub(super) fn append_template_render_plan_with_context(
        &mut self,
        render_plan: &TemplateRenderPlan,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let mut emitted_output = false;

        for piece in &render_plan.pieces {
            let emission =
                self.append_render_piece_to_accumulator(piece, append_context, fallback_location)?;

            match emission {
                TemplateBodyEmission::NoOutput => {}

                TemplateBodyEmission::Output => {
                    if !emitted_output && let Some(flag) = append_context.emitted_any_iteration {
                        self.mark_runtime_template_loop_iteration_emitted(flag, fallback_location)?;
                    }
                    emitted_output = true;
                }

                TemplateBodyEmission::Break | TemplateBodyEmission::Continue => {
                    return Ok(emission);
                }
            }

            let current_block = self.current_block_id_or_error(fallback_location)?;
            if self.block_has_explicit_terminator(current_block, fallback_location)? {
                break;
            }
        }

        Ok(if emitted_output {
            TemplateBodyEmission::Output
        } else {
            TemplateBodyEmission::NoOutput
        })
    }

    fn append_render_piece_to_accumulator(
        &mut self,
        piece: &RenderPiece,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        match piece {
            RenderPiece::Text(text) | RenderPiece::HeadContent(text) => {
                let text_value = self.string_table.resolve(text.text).to_owned();
                let region = self.current_region_or_error(&text.location)?;
                let chunk = self.make_expression(
                    &text.location,
                    HirExpressionKind::StringLiteral(text_value),
                    builtin_type_ids::STRING,
                    ValueKind::Const,
                    region,
                );

                self.append_template_chunk_to_accumulator(
                    chunk,
                    append_context.target_accumulator,
                    &text.location,
                )?;
                Ok(TemplateBodyEmission::Output)
            }

            RenderPiece::DynamicExpression(dynamic) => {
                if let ExpressionKind::Template(template) = &dynamic.expression.kind
                    && template.control_flow.is_some()
                {
                    return self.append_nested_runtime_template_control_flow(
                        template,
                        append_context,
                        &dynamic.expression.location,
                    );
                }

                let chunk = self.lower_expression_value_to_current_block(&dynamic.expression)?;
                self.append_template_chunk_to_accumulator(
                    chunk,
                    append_context.target_accumulator,
                    &dynamic.expression.location,
                )?;
                Ok(TemplateBodyEmission::Output)
            }

            RenderPiece::ChildTemplate(child) => {
                if let ExpressionKind::Template(template) = &child.expression.kind
                    && template.control_flow.is_some()
                {
                    return self.append_nested_runtime_template_control_flow(
                        template,
                        append_context,
                        &child.expression.location,
                    );
                }

                let chunk = self.lower_expression_value_to_current_block(&child.expression)?;
                self.append_template_chunk_to_accumulator(
                    chunk,
                    append_context.target_accumulator,
                    &child.expression.location,
                )?;
                Ok(TemplateBodyEmission::Output)
            }

            RenderPiece::LoopControl(signal) => match signal.kind {
                TemplateLoopControlKind::Break => {
                    self.emit_break_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Break)
                }
                TemplateLoopControlKind::Continue => {
                    self.emit_continue_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Continue)
                }
            },

            RenderPiece::Slot(slot) => {
                if let Some(slot_accumulators) = append_context.slot_accumulators {
                    let Some(slot_accumulator) = slot_accumulators.local_for(&slot.key) else {
                        return_hir_transformation_error!(
                            "Runtime slot application wrapper referenced a slot with no allocated accumulator.",
                            self.hir_error_location(fallback_location)
                        );
                    };

                    let region = self.current_region_or_error(fallback_location)?;
                    let slot_value = self.make_expression(
                        fallback_location,
                        HirExpressionKind::Load(HirPlace::Local(slot_accumulator)),
                        builtin_type_ids::STRING,
                        ValueKind::Place,
                        region,
                    );
                    self.append_template_chunk_to_accumulator(
                        slot_value,
                        append_context.target_accumulator,
                        fallback_location,
                    )?;

                    return Ok(TemplateBodyEmission::Output);
                }

                // Wrapper-shaped templates can reach HIR as runtime values when they are not used
                // as helpers. Their slot placeholders are structural insertion points, not
                // renderable chunks, so linear rendering skips them just as the old flattened
                // expression path did. Control-flow templates are still guarded before this point
                // by `has_unresolved_slots()`.
                Ok(TemplateBodyEmission::NoOutput)
            }
        }
    }

    fn append_nested_runtime_template_control_flow(
        &mut self,
        template: &Template,
        append_context: RuntimeTemplateAppendContext<'_>,
        location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let Some(control_flow) = &template.control_flow else {
            return Ok(TemplateBodyEmission::NoOutput);
        };

        match control_flow {
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

            _ => {
                if let Some(wrapper_plan) = &template.conditional_child_wrapper_plan {
                    return self.append_wrapped_nested_runtime_template_control_flow(
                        template,
                        control_flow,
                        wrapper_plan,
                        append_context,
                        location,
                    );
                }

                let emission = self.append_runtime_template_control_flow_with_context(
                    template,
                    control_flow,
                    append_context,
                    location,
                )?;

                if append_context.emitted_any_iteration().is_some()
                    && emission == TemplateBodyEmission::Output
                {
                    return Ok(TemplateBodyEmission::NoOutput);
                }

                Ok(emission)
            }
        }
    }

    fn append_wrapped_nested_runtime_template_control_flow(
        &mut self,
        template: &Template,
        control_flow: &TemplateControlFlow,
        wrapper_plan: &TemplateLoopAggregateRenderPlan,
        append_context: RuntimeTemplateAppendContext<'_>,
        location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let child_accumulator = self.initialize_runtime_template_accumulator(location)?;
        let child_emitted = self.initialize_runtime_template_emitted_flag(location)?;
        let child_context = append_context
            .with_target_accumulator(child_accumulator)
            .with_emitted_any_iteration(Some(child_emitted));

        let emission = self.append_runtime_template_control_flow_with_context(
            template,
            control_flow,
            child_context,
            location,
        )?;

        self.append_runtime_template_loop_aggregate_if_emitted(
            template,
            wrapper_plan,
            RuntimeTemplateLoopAggregateAppend {
                aggregate: child_accumulator,
                emitted_any_iteration: child_emitted,
                append_context,
            },
            location,
        )?;

        if matches!(
            emission,
            TemplateBodyEmission::Break | TemplateBodyEmission::Continue
        ) {
            return Ok(emission);
        }

        if append_context.emitted_any_iteration().is_some()
            && emission == TemplateBodyEmission::Output
        {
            return Ok(TemplateBodyEmission::NoOutput);
        }

        Ok(emission)
    }

    fn append_template_chunk_to_accumulator(
        &mut self,
        chunk: HirExpression,
        accumulator: LocalId,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let string_ty = builtin_type_ids::STRING;
        let region = self.current_region_or_error(location)?;
        let accumulated = self.make_expression(
            location,
            HirExpressionKind::Load(HirPlace::Local(accumulator)),
            string_ty,
            ValueKind::Place,
            region,
        );
        let chunk_as_string = self.coerce_expression_to_string(chunk, location, string_ty, region);
        let next_value = self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(accumulated),
                op: HirBinOp::Add,
                right: Box::new(chunk_as_string),
            },
            string_ty,
            ValueKind::RValue,
            region,
        );

        self.emit_statement_kind(
            HirStatementKind::Assign {
                target: HirPlace::Local(accumulator),
                value: next_value,
            },
            location,
        )
    }

    pub(super) fn coerce_expression_to_string(
        &mut self,
        expression: HirExpression,
        location: &SourceLocation,
        string_ty: TypeId,
        region: RegionId,
    ) -> HirExpression {
        if expression.ty == builtin_type_ids::STRING {
            return expression;
        }

        if expression.ty == self.type_environment.builtins().none {
            return self.make_expression(
                location,
                HirExpressionKind::StringLiteral(String::new()),
                string_ty,
                ValueKind::Const,
                region,
            );
        }

        let empty = self.make_expression(
            location,
            HirExpressionKind::StringLiteral(String::new()),
            string_ty,
            ValueKind::Const,
            region,
        );

        self.make_expression(
            location,
            HirExpressionKind::BinOp {
                left: Box::new(empty),
                op: crate::compiler_frontend::hir::operators::HirBinOp::Add,
                right: Box::new(expression),
            },
            string_ty,
            ValueKind::RValue,
            region,
        )
    }
}
