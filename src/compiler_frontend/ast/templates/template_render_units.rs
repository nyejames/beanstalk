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
    TemplateControlFlow, TemplateLoopAggregatePiece, TemplateLoopAggregateRenderPlan,
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
use crate::compiler_frontend::symbols::string_interning::StringTable;

// Source-authored positional slots are one-based. Index zero is therefore a
// private marker for synthetic loop-aggregate composition and must be converted
// before the aggregate render plan is returned.
const LOOP_AGGREGATE_MARKER_SLOT_INDEX: usize = 0;

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

/// Builds parsed content for a branch by prefixing it with the shared template
/// head chain. Template `if` uses this for each selectable branch; no-else
/// remains structural `NoOutput` and therefore never gets a synthetic unit.
pub(crate) fn content_with_shared_head_prefix(
    head_prefix: &TemplateContent,
    body_content: &TemplateContent,
) -> TemplateContent {
    let mut content = head_prefix.to_owned();
    content.extend(body_content.to_owned());
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
                let branch_content =
                    content_with_shared_head_prefix(shared_head_prefix, &branch.content);
                let branch_unit = prepare_template_render_unit(
                    branch_content,
                    style,
                    context,
                    can_fold,
                    string_table,
                    slot_resolution_mode,
                )?;
                branch.content = branch_unit.content.clone();
                branch.render_plan = Some(branch_unit.render_plan);
            }

            if let Some(fallback) = &mut branch_chain.fallback {
                let fallback_content =
                    content_with_shared_head_prefix(shared_head_prefix, &fallback.content);
                let fallback_unit = prepare_template_render_unit(
                    fallback_content,
                    style,
                    context,
                    can_fold,
                    string_table,
                    slot_resolution_mode,
                )?;
                fallback.content = fallback_unit.content.clone();
                fallback.render_plan = Some(fallback_unit.render_plan);
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            let body_unit = prepare_template_render_unit(
                template_loop.body_content.to_owned(),
                style,
                context,
                can_fold,
                string_table,
                slot_resolution_mode,
            )?;
            template_loop.body_content = body_unit.content.clone();
            template_loop.body_render_plan = Some(body_unit.render_plan);
            template_loop.aggregate_render_plan = Some(prepare_loop_aggregate_render_plan(
                shared_head_prefix,
                string_table,
            )?);
        }

        TemplateControlFlow::LoopControl(_) => {}
    }

    Ok(())
}

fn prepare_loop_aggregate_render_plan(
    shared_head_prefix: &TemplateContent,
    string_table: &StringTable,
) -> Result<TemplateLoopAggregateRenderPlan, TemplateError> {
    let mut content = shared_head_prefix.to_owned();
    content.atoms.push(loop_aggregate_placeholder_atom());

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
    let mut pieces = Vec::new();
    for piece in plan.pieces {
        pieces.extend(loop_aggregate_pieces_from_render_piece(piece));
    }

    Ok(TemplateLoopAggregateRenderPlan { pieces })
}

pub(in crate::compiler_frontend::ast::templates) fn prepare_conditional_child_wrapper_render_plan(
    child_wrappers: &[Template],
    string_table: &StringTable,
) -> Result<TemplateLoopAggregateRenderPlan, TemplateSlotError> {
    let aggregate_atom = loop_aggregate_placeholder_atom();
    let wrapped_atom = wrap_direct_child_atom(
        &aggregate_atom,
        child_wrappers,
        string_table,
        SlotResolutionMode::ComposeOnly,
    )?;
    let plan = TemplateRenderPlan::from_content(&TemplateContent {
        atoms: vec![wrapped_atom],
    });
    let mut pieces = Vec::new();

    for piece in plan.pieces {
        pieces.extend(loop_aggregate_pieces_from_render_piece(piece));
    }

    Ok(TemplateLoopAggregateRenderPlan { pieces })
}

fn loop_aggregate_placeholder_atom() -> TemplateAtom {
    TemplateAtom::Slot(SlotPlaceholder::with_wrappers(
        SlotKey::Positional(LOOP_AGGREGATE_MARKER_SLOT_INDEX),
        Vec::new(),
        Vec::new(),
        true,
    ))
}

fn loop_aggregate_pieces_from_render_piece(piece: RenderPiece) -> Vec<TemplateLoopAggregatePiece> {
    match piece {
        RenderPiece::Slot(slot) if is_loop_aggregate_placeholder(&slot) => {
            vec![TemplateLoopAggregatePiece::Aggregate]
        }

        RenderPiece::DynamicExpression(dynamic) => {
            if let ExpressionKind::Template(template) = &dynamic.expression.kind
                && template_contains_loop_aggregate_placeholder(template)
            {
                return loop_aggregate_pieces_from_template(template);
            }

            vec![TemplateLoopAggregatePiece::Render(Box::new(
                RenderPiece::DynamicExpression(dynamic),
            ))]
        }

        RenderPiece::ChildTemplate(child) => {
            if let ExpressionKind::Template(template) = &child.expression.kind
                && template_contains_loop_aggregate_placeholder(template)
            {
                return loop_aggregate_pieces_from_template(template);
            }

            vec![TemplateLoopAggregatePiece::Render(Box::new(
                RenderPiece::ChildTemplate(child),
            ))]
        }

        RenderPiece::LoopControl(signal) => vec![TemplateLoopAggregatePiece::Render(Box::new(
            RenderPiece::LoopControl(signal),
        ))],

        _ => vec![TemplateLoopAggregatePiece::Render(Box::new(piece))],
    }
}

fn is_loop_aggregate_placeholder(slot: &SlotPlaceholder) -> bool {
    matches!(
        &slot.key,
        SlotKey::Positional(index) if *index == LOOP_AGGREGATE_MARKER_SLOT_INDEX
    )
}

fn loop_aggregate_pieces_from_template(template: &Template) -> Vec<TemplateLoopAggregatePiece> {
    let plan = TemplateRenderPlan::from_content(&template.content);
    let mut pieces = Vec::new();

    for piece in plan.pieces {
        pieces.extend(loop_aggregate_pieces_from_render_piece(piece));
    }

    pieces
}

fn template_contains_loop_aggregate_placeholder(template: &Template) -> bool {
    template
        .content
        .atoms
        .iter()
        .any(atom_contains_loop_aggregate_placeholder)
}

fn atom_contains_loop_aggregate_placeholder(atom: &TemplateAtom) -> bool {
    match atom {
        TemplateAtom::Slot(slot) => is_loop_aggregate_placeholder(slot),

        TemplateAtom::Content(segment) => match &segment.expression.kind {
            ExpressionKind::Template(template) => {
                template_contains_loop_aggregate_placeholder(template)
            }
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
        return Ok(parsed_content.to_owned());
    }

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
        let mut content = input.render_plan.rebuild_content();

        if input.requires_post_format_recomposition {
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
