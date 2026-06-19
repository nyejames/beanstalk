//! Template render-unit preparation for linear and control-flow templates.
//!
//! WHAT: Owns the shared Stage 3-5 pipeline that turns parsed template content
//! into finalized `TemplateContent` plus a matching flat `TemplateRenderPlan`.
//!
//! WHY: Normal templates, template `if` branches, and template `loop` bodies
//! all need the same composition and formatting rules. Keeping the render-unit
//! shaping here prevents control-flow support from growing a parallel template
//! pipeline.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, SlotPlaceholder, Style, TemplateAtom, TemplateContent, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_composition::{
    apply_inherited_child_templates_to_content, compose_template_head_chain, wrap_direct_child_atom,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateControlFlow,
};
use crate::compiler_frontend::ast::templates::template_formatting::{
    BodyFormattingResult, apply_body_formatter,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    SlotResolutionMode, TemplateSlotError,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::DiagnosticSeverity;
use crate::compiler_frontend::instrumentation::{AstCounter, add_ast_counter};
use crate::compiler_frontend::symbols::string_interning::StringTable;

// Source-authored positional slots are one-based. Index zero is therefore a
// private marker for synthetic aggregate composition and must be converted
// before the aggregate render plan is returned.
const AGGREGATE_MARKER_SLOT_INDEX: usize = 0;

/// Result of preparing parsed content through the shared composition /
/// formatting / finalization pipeline.
pub(crate) struct PreparedTemplateRenderUnit {
    pub(crate) content: TemplateContent,
    pub(crate) unformatted_content: TemplateContent,
    pub(crate) render_plan: TemplateRenderPlan,
}

struct RenderUnitFinalizationInput<'a> {
    parsed_content: TemplateContent,
    render_plan: TemplateRenderPlan,
    content_changed: bool,
    style: &'a Style,
    can_fold: &'a mut bool,
    string_table: &'a StringTable,
    requires_post_format_recomposition: bool,
    slot_resolution_mode: SlotResolutionMode,
}

/// Finalizes one linear render unit from parsed content.
///
/// `parsed_content` is the direct output of head/body parsing for this unit.
/// Formatting intentionally runs before post-format recomposition so body
/// formatters see only source-authored body text and opaque anchors, while
/// slot/head composition remains authoritative for final render order.
pub(in crate::compiler_frontend::ast::templates) fn prepare_template_render_unit(
    parsed_content: TemplateContent,
    style: &Style,
    context: &ScopeContext,
    can_fold: &mut bool,
    string_table: &mut StringTable,
    slot_resolution_mode: SlotResolutionMode,
) -> Result<PreparedTemplateRenderUnit, TemplateError> {
    add_ast_counter(AstCounter::TemplateCompositionPasses, 1);

    let requires_post_format_recomposition =
        content_requires_post_format_recomposition(&parsed_content, style);

    let unformatted_content = build_unformatted_template_content(
        &parsed_content,
        style,
        can_fold,
        string_table,
        requires_post_format_recomposition,
        slot_resolution_mode,
    )?;

    let BodyFormattingResult {
        plan: render_plan,
        content_changed,
        ..
    } = format_template_body(&parsed_content, style, context, string_table)?;

    let (content, render_plan) =
        finalize_render_unit_after_formatting(RenderUnitFinalizationInput {
            parsed_content,
            render_plan,
            content_changed,
            style,
            can_fold,
            string_table,
            requires_post_format_recomposition,
            slot_resolution_mode,
        })?;

    Ok(PreparedTemplateRenderUnit {
        content,
        unformatted_content,
        render_plan,
    })
}

/// Builds parsed content by prefixing an owned body with the shared template
/// head chain. Takes ownership of `body_content` to avoid one clone per call.
///
/// WHY: control-flow branch and loop preparation already owns the body content
/// after parsing. Moving it instead of cloning saves one allocation per
/// control-flow arm.
fn content_with_head_prefix_owned_body(
    head_prefix: &TemplateContent,
    body_content: TemplateContent,
) -> TemplateContent {
    add_ast_counter(AstCounter::TemplateContentClonesForRenderUnits, 1);
    let mut content = head_prefix.to_owned();
    content.extend(body_content);
    content
}

/// Applies composition and formatting to a structured control-flow template in
/// place.
///
/// For `if`, each branch is a complete render unit that includes the shared
/// head prefix. For `loop`, the per-iteration body is finalized independently
/// and the shared head prefix remains on the owning template so later folding /
/// lowering can apply it once around the aggregate.
pub(in crate::compiler_frontend::ast::templates) fn prepare_control_flow_render_units(
    control_flow: &mut TemplateControlFlow,
    shared_head_prefix: &TemplateContent,
    style: &Style,
    context: &ScopeContext,
    can_fold: &mut bool,
    string_table: &mut StringTable,
    slot_resolution_mode: SlotResolutionMode,
) -> Result<(), TemplateError> {
    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            for branch in &mut branch_chain.branches {
                // Move branch content instead of cloning — the branch no longer
                // needs its original parsed content after preparation.
                let branch_content = std::mem::take(&mut branch.content);
                let prefixed =
                    content_with_head_prefix_owned_body(shared_head_prefix, branch_content);
                let PreparedTemplateRenderUnit {
                    content,
                    render_plan,
                    ..
                } = prepare_template_render_unit(
                    prefixed,
                    style,
                    context,
                    can_fold,
                    string_table,
                    slot_resolution_mode,
                )?;
                // Move prepared content back instead of cloning.
                branch.content = content;
                branch.render_plan = Some(render_plan);
            }

            if let Some(fallback) = &mut branch_chain.fallback {
                // Move fallback content instead of cloning.
                let fallback_content = std::mem::take(&mut fallback.content);
                let prefixed =
                    content_with_head_prefix_owned_body(shared_head_prefix, fallback_content);
                let PreparedTemplateRenderUnit {
                    content,
                    render_plan,
                    ..
                } = prepare_template_render_unit(
                    prefixed,
                    style,
                    context,
                    can_fold,
                    string_table,
                    slot_resolution_mode,
                )?;
                // Move prepared content back instead of cloning.
                fallback.content = content;
                fallback.render_plan = Some(render_plan);
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            // Move body content instead of cloning — the loop no longer needs
            // its original parsed body after preparation.
            let body_content = std::mem::take(&mut template_loop.body_content);
            let PreparedTemplateRenderUnit {
                content,
                render_plan,
                ..
            } = prepare_template_render_unit(
                body_content,
                style,
                context,
                can_fold,
                string_table,
                slot_resolution_mode,
            )?;
            // Move prepared content back instead of cloning.
            template_loop.body_content = content;
            template_loop.body_render_plan = Some(render_plan);
            template_loop.aggregate_render_plan = Some(prepare_template_aggregate_render_plan(
                shared_head_prefix,
                string_table,
            )?);
        }

        TemplateControlFlow::LoopControl(_) => {}
    }

    Ok(())
}

pub(in crate::compiler_frontend::ast::templates) fn prepare_template_aggregate_render_plan(
    shared_head_prefix: &TemplateContent,
    string_table: &StringTable,
) -> Result<TemplateAggregateRenderPlan, TemplateError> {
    add_ast_counter(AstCounter::TemplateAggregatePlanBuilds, 1);
    add_ast_counter(AstCounter::TemplateContentClonesForRenderUnits, 1);
    let mut content = shared_head_prefix.to_owned();
    content.atoms.push(aggregate_placeholder_atom());

    // The aggregate placeholder is structural and local to this synthetic
    // composition pass. It lets normal head-chain/slot composition place the
    // aggregate inside wrappers without representing the marker as a user
    // expression that could leak into folding, diagnostics, or HIR lowering.
    let mut aggregate_plan_can_fold = true;
    let composed = compose_template_head_chain(
        &content,
        &mut aggregate_plan_can_fold,
        string_table,
        SlotResolutionMode::ComposeOnly,
    )?;
    let plan = TemplateRenderPlan::from_content(&composed);
    let mut pieces = Vec::with_capacity(plan.pieces.len());
    for piece in plan.pieces {
        pieces.extend(aggregate_pieces_from_render_piece(piece));
    }

    Ok(TemplateAggregateRenderPlan { pieces })
}

pub(in crate::compiler_frontend::ast::templates) fn prepare_conditional_child_wrapper_render_plan(
    child_wrappers: &[Template],
    string_table: &StringTable,
) -> Result<TemplateAggregateRenderPlan, TemplateSlotError> {
    add_ast_counter(AstCounter::TemplateAggregatePlanBuilds, 1);
    add_ast_counter(
        AstCounter::TemplateWrapperApplications,
        child_wrappers.len(),
    );

    let aggregate_atom = aggregate_placeholder_atom();
    let wrapped_atom = wrap_direct_child_atom(
        &aggregate_atom,
        child_wrappers,
        string_table,
        SlotResolutionMode::ComposeOnly,
    )?;
    let plan = TemplateRenderPlan::from_content(&TemplateContent {
        atoms: vec![wrapped_atom],
    });
    let mut pieces = Vec::with_capacity(child_wrappers.len().saturating_add(1));

    for piece in plan.pieces {
        pieces.extend(aggregate_pieces_from_render_piece(piece));
    }

    Ok(TemplateAggregateRenderPlan { pieces })
}

fn aggregate_placeholder_atom() -> TemplateAtom {
    TemplateAtom::Slot(SlotPlaceholder::with_wrappers(
        SlotKey::Positional(AGGREGATE_MARKER_SLOT_INDEX),
        Vec::new(),
        Vec::new(),
        true,
    ))
}

fn aggregate_pieces_from_render_piece(piece: RenderPiece) -> Vec<TemplateAggregatePiece> {
    match piece {
        RenderPiece::Slot(slot) if is_aggregate_placeholder(&slot) => {
            vec![TemplateAggregatePiece::Aggregate]
        }

        RenderPiece::DynamicExpression(dynamic) => {
            if let ExpressionKind::Template(template) = &dynamic.expression.kind
                && template_contains_aggregate_placeholder(template)
            {
                return aggregate_pieces_from_template(template);
            }

            vec![TemplateAggregatePiece::Render(Box::new(
                RenderPiece::DynamicExpression(dynamic),
            ))]
        }

        RenderPiece::ChildTemplate(child) => {
            if let ExpressionKind::Template(template) = &child.expression.kind
                && template_contains_aggregate_placeholder(template)
            {
                return aggregate_pieces_from_template(template);
            }

            vec![TemplateAggregatePiece::Render(Box::new(
                RenderPiece::ChildTemplate(child),
            ))]
        }

        RenderPiece::LoopControl(signal) => vec![TemplateAggregatePiece::Render(Box::new(
            RenderPiece::LoopControl(signal),
        ))],

        _ => vec![TemplateAggregatePiece::Render(Box::new(piece))],
    }
}

fn is_aggregate_placeholder(slot: &SlotPlaceholder) -> bool {
    matches!(
        &slot.key,
        SlotKey::Positional(index) if *index == AGGREGATE_MARKER_SLOT_INDEX
    )
}

fn aggregate_pieces_from_template(template: &Template) -> Vec<TemplateAggregatePiece> {
    // Use the existing authoritative render plan when available instead of
    // rebuilding from content. This avoids a full `from_content` traversal
    // for templates that were already prepared.
    let plan = if let Some(existing_plan) = &template.render_plan {
        existing_plan.clone_recording_template_churn()
    } else {
        TemplateRenderPlan::from_content(&template.content)
    };
    let mut pieces = Vec::with_capacity(plan.pieces.len());

    for piece in plan.pieces {
        pieces.extend(aggregate_pieces_from_render_piece(piece));
    }

    pieces
}

fn template_contains_aggregate_placeholder(template: &Template) -> bool {
    template
        .content
        .atoms
        .iter()
        .any(atom_contains_aggregate_placeholder)
}

fn atom_contains_aggregate_placeholder(atom: &TemplateAtom) -> bool {
    match atom {
        TemplateAtom::Slot(slot) => is_aggregate_placeholder(slot),

        TemplateAtom::Content(segment) => match &segment.expression.kind {
            ExpressionKind::Template(template) => template_contains_aggregate_placeholder(template),
            _ => false,
        },
    }
}

/// Returns true when this template or one of its descendants carries structured
/// template control flow.
pub(crate) fn template_contains_control_flow(
    template: &crate::compiler_frontend::ast::templates::template_types::Template,
) -> bool {
    if template.control_flow.is_some() {
        return true;
    }

    template.content.atoms.iter().any(|atom| {
        let TemplateAtom::Content(segment) = atom else {
            return false;
        };

        let ExpressionKind::Template(child_template) = &segment.expression.kind else {
            return false;
        };

        template_contains_control_flow(child_template)
    })
}

fn build_unformatted_template_content(
    parsed_content: &TemplateContent,
    style: &Style,
    can_fold: &mut bool,
    string_table: &StringTable,
    requires_post_format_recomposition: bool,
    slot_resolution_mode: SlotResolutionMode,
) -> Result<TemplateContent, TemplateError> {
    if !requires_post_format_recomposition {
        add_ast_counter(AstCounter::TemplateContentClonesForRenderUnits, 1);
        return Ok(parsed_content.to_owned());
    }

    add_ast_counter(
        AstCounter::TemplateWrapperApplications,
        style.child_templates.len(),
    );
    add_ast_counter(AstCounter::TemplateContentClonesForRenderUnits, 1);

    let mut content = apply_inherited_child_templates_to_content(
        parsed_content.to_owned(),
        &style.child_templates,
        string_table,
        slot_resolution_mode,
    )?;

    content = compose_template_head_chain(&content, can_fold, string_table, slot_resolution_mode)?;

    Ok(content)
}

fn format_template_body(
    content: &TemplateContent,
    style: &Style,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<BodyFormattingResult, TemplateError> {
    let formatter_result = match apply_body_formatter(content, style, string_table) {
        Ok(result) => result,

        Err(messages) => {
            let mut error_diagnostic = None;
            for diagnostic in messages.into_diagnostics() {
                if diagnostic.severity == DiagnosticSeverity::Warning {
                    context.emit_warning(diagnostic);
                } else if diagnostic.severity == DiagnosticSeverity::Error
                    && error_diagnostic.is_none()
                {
                    error_diagnostic = Some(diagnostic);
                }
            }

            return Err(error_diagnostic
                .map(TemplateError::from)
                .unwrap_or_else(|| {
                    CompilerError::compiler_error(
                        "Template formatter failed without returning a compiler error.",
                    )
                    .into()
                }));
        }
    };

    for warning in &formatter_result.warnings {
        context.emit_warning(warning.clone());
    }

    Ok(formatter_result)
}

fn finalize_render_unit_after_formatting(
    input: RenderUnitFinalizationInput<'_>,
) -> Result<(TemplateContent, TemplateRenderPlan), TemplateError> {
    if input.content_changed || input.requires_post_format_recomposition {
        add_ast_counter(AstCounter::TemplateContentRebuildsAfterFormatting, 1);
        let mut content = input.render_plan.rebuild_content();

        if input.requires_post_format_recomposition {
            add_ast_counter(
                AstCounter::TemplateWrapperApplications,
                input.style.child_templates.len(),
            );

            content = apply_inherited_child_templates_to_content(
                content,
                &input.style.child_templates,
                input.string_table,
                input.slot_resolution_mode,
            )?;

            content = compose_template_head_chain(
                &content,
                input.can_fold,
                input.string_table,
                input.slot_resolution_mode,
            )?;
        }

        let render_plan = TemplateRenderPlan::from_content(&content);
        return Ok((content, render_plan));
    }

    // Formatting was a no-op and composition will not run again, so keep the
    // parsed atom stream intact. That preserves child-template source metadata
    // while still storing the render plan produced from the same content.
    Ok((input.parsed_content, input.render_plan))
}

fn content_requires_post_format_recomposition(content: &TemplateContent, style: &Style) -> bool {
    if !style.child_templates.is_empty() {
        return true;
    }

    content.atoms.iter().any(|atom| {
        matches!(
            atom,
            TemplateAtom::Content(segment)
                if segment.origin == TemplateSegmentOrigin::Head
        )
    })
}
