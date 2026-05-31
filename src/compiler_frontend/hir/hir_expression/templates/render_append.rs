//! Runtime template render-plan append helpers.
//!
//! WHAT: appends prepared render pieces into a string accumulator and performs final string
//! coercion for dynamic chunks.
//! WHY: inline control-flow templates and aggregate wrapping share the same HIR
//! concatenation semantics, and runtime slot source/site plans use that same append path after
//! AST has finished routing and validation.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBodyEmission, TemplateControlFlow,
    TemplateLoopControlKind,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotContributionSourceId, RuntimeSlotSiteId, RuntimeSlotSitePiece,
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
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_hir_transformation_error;

use super::aggregate::RuntimeTemplateAggregateAppend;
use super::append_context::{RuntimeSlotLoopControlFlush, RuntimeTemplateAppendContext};

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

    pub(super) fn append_template_aggregate_plan_with_context(
        &mut self,
        template: &Template,
        aggregate_plan: &TemplateAggregateRenderPlan,
        aggregate: LocalId,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        for piece in &aggregate_plan.pieces {
            match piece {
                TemplateAggregatePiece::Render(render_piece) => {
                    self.append_render_piece_to_accumulator(
                        render_piece,
                        append_context,
                        fallback_location,
                    )?;
                }

                TemplateAggregatePiece::Aggregate => {
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
                    if !emitted_output && let Some(flag) = append_context.emitted_output {
                        self.mark_runtime_template_output_emitted(flag, fallback_location)?;
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
                if let Some(emission) = self.append_runtime_template_expression_to_accumulator(
                    &dynamic.expression,
                    append_context,
                )? {
                    return Ok(emission);
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
                if let Some(emission) = self.append_runtime_template_expression_to_accumulator(
                    &child.expression,
                    append_context,
                )? {
                    return Ok(emission);
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
                    if let Some(flush) = append_context.loop_control_flush {
                        self.flush_runtime_slot_application_for_loop_control(
                            flush,
                            TemplateLoopControlKind::Break,
                            &signal.location,
                        )?;
                        return Ok(TemplateBodyEmission::Break);
                    }

                    self.emit_break_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Break)
                }
                TemplateLoopControlKind::Continue => {
                    if let Some(flush) = append_context.loop_control_flush {
                        self.flush_runtime_slot_application_for_loop_control(
                            flush,
                            TemplateLoopControlKind::Continue,
                            &signal.location,
                        )?;
                        return Ok(TemplateBodyEmission::Continue);
                    }

                    self.emit_continue_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Continue)
                }
            },

            RenderPiece::Slot(_) => {
                // Wrapper-shaped templates can reach HIR as runtime values when they are not used
                // as helpers. Their slot placeholders are structural insertion points, not
                // renderable chunks, so linear rendering skips them just as the old flattened
                // expression path did. Control-flow templates are still guarded before this point
                // by `has_unresolved_slots()`.
                Ok(TemplateBodyEmission::NoOutput)
            }

            RenderPiece::RuntimeSlotSite(site_id) => self.append_runtime_slot_site_to_accumulator(
                *site_id,
                append_context,
                fallback_location,
            ),
        }
    }

    fn append_runtime_template_expression_to_accumulator(
        &mut self,
        expression: &Expression,
        append_context: RuntimeTemplateAppendContext<'_>,
    ) -> Result<Option<TemplateBodyEmission>, CompilerError> {
        let Some(template) = runtime_template_for_expression(expression) else {
            return Ok(None);
        };

        if let Some(plan) = &template.runtime_slot_application {
            return self
                .append_runtime_slot_application_with_context(
                    plan,
                    append_context,
                    &expression.location,
                )
                .map(Some);
        }

        if template.control_flow.is_some() {
            return self
                .append_nested_runtime_template_control_flow(
                    template,
                    append_context,
                    &expression.location,
                )
                .map(Some);
        }

        // Slot helper wrappers can reach this point as linear templates around
        // an inner runtime slot application. Append only that shape directly so
        // ordinary template expressions keep their value-lowering codegen.
        if let Some(render_plan) = &template.render_plan
            && render_plan_contains_runtime_slot_application(render_plan)
        {
            return self
                .append_template_render_plan_with_context(
                    render_plan,
                    append_context,
                    &expression.location,
                )
                .map(Some);
        }

        Ok(None)
    }

    fn flush_runtime_slot_application_for_loop_control(
        &mut self,
        flush: RuntimeSlotLoopControlFlush<'_>,
        control_kind: TemplateLoopControlKind,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        let condition_block = self.current_block_id_or_error(location)?;
        let parent_region = self.current_region_or_error(location)?;
        let flush_region = self.create_child_region(parent_region);
        let skip_region = self.create_child_region(parent_region);
        let flush_block = self.create_block(flush_region, location, "runtime-slot-flush")?;
        let skip_block = self.create_block(skip_region, location, "runtime-slot-skip")?;
        let condition = self.make_local_load_expression(
            flush.contribution_emitted_flag,
            builtin_type_ids::BOOL,
            location,
            parent_region,
        );

        self.emit_terminator(
            condition_block,
            HirTerminator::If {
                condition,
                then_block: flush_block,
                else_block: skip_block,
            },
            location,
        )?;

        // If a slot contribution produced output before loop control, replay the
        // wrapper on this terminating path before jumping to the surrounding
        // template loop target. The skip path still emits the same loop control
        // without rendering an empty wrapper.
        self.set_current_block(flush_block, location)?;
        let wrapper_context = RuntimeTemplateAppendContext::new(flush.target_accumulator)
            .with_runtime_slot_sites(flush.source_accumulators, flush.slot_sites)
            .with_emitted_output(flush.parent_emitted_flag);
        self.append_template_render_plan_with_context(
            flush.wrapper_plan,
            wrapper_context,
            location,
        )?;

        let flush_tail = self.current_block_id_or_error(location)?;
        if !self.block_has_explicit_terminator(flush_tail, location)? {
            self.emit_template_loop_control(control_kind, location)?;
        }

        self.set_current_block(skip_block, location)?;
        self.emit_template_loop_control(control_kind, location)
    }

    fn append_runtime_slot_site_to_accumulator(
        &mut self,
        site_id: RuntimeSlotSiteId,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let Some(slot_sites) = append_context.slot_sites else {
            return_hir_transformation_error!(
                "Runtime slot site appeared outside an active runtime slot application.",
                self.hir_error_location(fallback_location)
            );
        };
        let Some(site) = slot_sites.get(site_id.0).filter(|site| site.id == site_id) else {
            return_hir_transformation_error!(
                "Runtime slot application wrapper referenced a missing slot site.",
                self.hir_error_location(fallback_location)
            );
        };

        let mut emitted_output = false;

        for piece in &site.render_plan.pieces {
            let emission = match piece {
                RuntimeSlotSitePiece::Render(render_piece) => self
                    .append_render_piece_to_accumulator(
                        render_piece,
                        append_context,
                        &site.location,
                    )?,

                RuntimeSlotSitePiece::ContributionSource(source_id) => self
                    .append_runtime_slot_source_to_accumulator(
                        *source_id,
                        append_context,
                        &site.location,
                    )?,
            };

            match emission {
                TemplateBodyEmission::NoOutput => {}
                TemplateBodyEmission::Output => emitted_output = true,
                TemplateBodyEmission::Break | TemplateBodyEmission::Continue => {
                    return Ok(emission);
                }
            }
        }

        Ok(if emitted_output {
            TemplateBodyEmission::Output
        } else {
            TemplateBodyEmission::NoOutput
        })
    }

    fn append_runtime_slot_source_to_accumulator(
        &mut self,
        source_id: RuntimeSlotContributionSourceId,
        append_context: RuntimeTemplateAppendContext<'_>,
        fallback_location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let Some(source_accumulators) = append_context.source_accumulators else {
            return_hir_transformation_error!(
                "Runtime slot source appeared outside an active runtime slot application.",
                self.hir_error_location(fallback_location)
            );
        };
        let Some(source_accumulator) = source_accumulators.local_for(source_id) else {
            return_hir_transformation_error!(
                "Runtime slot site referenced a missing contribution source.",
                self.hir_error_location(fallback_location)
            );
        };

        let region = self.current_region_or_error(fallback_location)?;
        let source_value = self.make_expression(
            fallback_location,
            HirExpressionKind::Load(HirPlace::Local(source_accumulator)),
            builtin_type_ids::STRING,
            ValueKind::Place,
            region,
        );
        self.append_template_chunk_to_accumulator(
            source_value,
            append_context.target_accumulator,
            fallback_location,
        )?;

        Ok(TemplateBodyEmission::Output)
    }

    fn emit_template_loop_control(
        &mut self,
        control_kind: TemplateLoopControlKind,
        location: &SourceLocation,
    ) -> Result<(), CompilerError> {
        match control_kind {
            TemplateLoopControlKind::Break => self.emit_break_to_current_loop(location),
            TemplateLoopControlKind::Continue => self.emit_continue_to_current_loop(location),
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
                    if let Some(flush) = append_context.loop_control_flush {
                        self.flush_runtime_slot_application_for_loop_control(
                            flush,
                            TemplateLoopControlKind::Break,
                            &signal.location,
                        )?;
                        return Ok(TemplateBodyEmission::Break);
                    }

                    self.emit_break_to_current_loop(&signal.location)?;
                    Ok(TemplateBodyEmission::Break)
                }
                TemplateLoopControlKind::Continue => {
                    if let Some(flush) = append_context.loop_control_flush {
                        self.flush_runtime_slot_application_for_loop_control(
                            flush,
                            TemplateLoopControlKind::Continue,
                            &signal.location,
                        )?;
                        return Ok(TemplateBodyEmission::Continue);
                    }

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

                if append_context.emitted_output().is_some()
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
        wrapper_plan: &TemplateAggregateRenderPlan,
        append_context: RuntimeTemplateAppendContext<'_>,
        location: &SourceLocation,
    ) -> Result<TemplateBodyEmission, CompilerError> {
        let child_accumulator = self.initialize_runtime_template_accumulator(location)?;
        let child_emitted = self.initialize_runtime_template_emitted_flag(location)?;
        let child_context = append_context
            .with_target_accumulator(child_accumulator)
            .with_emitted_output(Some(child_emitted));

        let emission = self.append_runtime_template_control_flow_with_context(
            template,
            control_flow,
            child_context,
            location,
        )?;

        self.append_runtime_template_aggregate_if_emitted(
            template,
            wrapper_plan,
            RuntimeTemplateAggregateAppend {
                aggregate: child_accumulator,
                emitted_output: child_emitted,
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

        if append_context.emitted_output().is_some() && emission == TemplateBodyEmission::Output {
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

fn runtime_template_for_expression(expression: &Expression) -> Option<&Template> {
    match &expression.kind {
        ExpressionKind::Template(template) => Some(template),

        // String-boundary coercions are inserted around template helpers before
        // HIR lowering. Append-mode slot applications must see through that
        // wrapper so loop control does not escape through expression lowering
        // before the outer template accumulator receives the rendered wrapper.
        ExpressionKind::Coerced { value, .. } => runtime_template_for_expression(value),

        ExpressionKind::Runtime(nodes) if nodes.len() == 1 => match &nodes[0].kind {
            NodeKind::Rvalue(value) => runtime_template_for_expression(value),
            _ => None,
        },

        _ => None,
    }
}

fn render_plan_contains_runtime_slot_application(render_plan: &TemplateRenderPlan) -> bool {
    render_plan
        .pieces
        .iter()
        .any(render_piece_contains_runtime_slot_application)
}

fn render_piece_contains_runtime_slot_application(piece: &RenderPiece) -> bool {
    match piece {
        RenderPiece::DynamicExpression(dynamic) => {
            expression_contains_runtime_slot_application(&dynamic.expression)
        }

        RenderPiece::ChildTemplate(child) => {
            expression_contains_runtime_slot_application(&child.expression)
        }

        RenderPiece::RuntimeSlotSite(_) => true,

        RenderPiece::Text(_) | RenderPiece::HeadContent(_) | RenderPiece::Slot(_) => false,
        RenderPiece::LoopControl(_) => false,
    }
}

fn expression_contains_runtime_slot_application(expression: &Expression) -> bool {
    let Some(template) = runtime_template_for_expression(expression) else {
        return false;
    };

    if template.runtime_slot_application.is_some() {
        return true;
    }

    template
        .render_plan
        .as_ref()
        .is_some_and(render_plan_contains_runtime_slot_application)
}
