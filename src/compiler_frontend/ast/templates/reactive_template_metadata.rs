//! Reactive template metadata structural traversal.
//!
//! WHAT: walks the structural shape of a `Template`: content, control flow,
//! render plans, aggregate render plans, and runtime slot plans, then merges
//! reactive metadata using a caller-supplied expression resolver.
//! WHY: template shape is owned by the template subsystem, but expression metadata
//! resolution differs by caller. AST finalization needs flow-aware resolution using
//! function-flow maps and the value environment, while the default template query
//! only needs already-computed `Expression::reactive_template` fields.

use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::templates::template::{
    TemplateAtom, TemplateContent, TemplateSegment,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBranchSelector,
    TemplateControlFlow, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_render_plan::{
    RenderPiece, TemplateRenderPlan,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotApplicationPlan, RuntimeSlotSitePiece,
};
use crate::compiler_frontend::ast::templates::template_types::Template;

/// Merges reactive template metadata from `template` into `metadata` using the default
/// expression resolver.
///
/// The default resolver uses an expression's already-computed `reactive_template` field
/// when present, and recurses into nested `ExpressionKind::Template` values otherwise.
pub(crate) fn merge_reactive_template_metadata(
    template: &Template,
    metadata: &mut ReactiveTemplateMetadata,
) {
    merge_reactive_template_metadata_with_resolver(template, metadata, &mut default_resolver);
}

/// Merges reactive template metadata from `template` into `metadata` using `resolver`
/// to compute metadata for each expression encountered during the structural walk.
///
/// The resolver is called for every expression carried by the template structure.
/// It should decide how to derive metadata for that expression, including any
/// recursion into nested `ExpressionKind::Template` values. This keeps flow-aware
/// expression resolution in AST finalization and lets the template subsystem own
/// the structural traversal.
pub(crate) fn merge_reactive_template_metadata_with_resolver(
    template: &Template,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    merge_content_metadata(&template.content, metadata, resolver);
    merge_control_flow_metadata(&template.control_flow, metadata, resolver);

    if let Some(plan) = &template.runtime_slot_application {
        merge_runtime_slot_application_metadata(plan, metadata, resolver);
    }
}

fn default_resolver(expression: &Expression) -> Option<ReactiveTemplateMetadata> {
    if let Some(metadata) = &expression.reactive_template {
        return Some(metadata.clone());
    }

    if let ExpressionKind::Template(template) = &expression.kind {
        return template.reactive_template_metadata();
    }

    None
}

fn merge_content_metadata(
    content: &TemplateContent,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    for atom in &content.atoms {
        if let TemplateAtom::Content(segment) = atom {
            merge_segment_metadata(segment, metadata, resolver);
        }
    }
}

fn merge_segment_metadata(
    segment: &TemplateSegment,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    if let Some(subscription) = &segment.reactive_subscription {
        metadata.push_subscription(subscription.clone());
    }

    merge_expression_metadata(&segment.expression, metadata, resolver);
}

fn merge_expression_metadata(
    expression: &Expression,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    if let Some(expression_metadata) = resolver(expression) {
        metadata.merge_from(&expression_metadata);
    }
}

fn merge_control_flow_metadata(
    control_flow: &Option<TemplateControlFlow>,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let Some(control_flow) = control_flow else {
        return;
    };

    match control_flow {
        TemplateControlFlow::BranchChain(branch_chain) => {
            for branch in &branch_chain.branches {
                merge_branch_selector_metadata(&branch.selector, metadata, resolver);
                merge_content_metadata(&branch.content, metadata, resolver);
                if let Some(render_plan) = &branch.render_plan {
                    merge_render_plan_metadata(render_plan, metadata, resolver);
                }
            }

            if let Some(fallback) = &branch_chain.fallback {
                merge_content_metadata(&fallback.content, metadata, resolver);
                if let Some(render_plan) = &fallback.render_plan {
                    merge_render_plan_metadata(render_plan, metadata, resolver);
                }
            }
        }

        TemplateControlFlow::Loop(template_loop) => {
            merge_loop_header_metadata(&template_loop.header, metadata, resolver);
            merge_content_metadata(&template_loop.body_content, metadata, resolver);
            if let Some(render_plan) = &template_loop.body_render_plan {
                merge_render_plan_metadata(render_plan, metadata, resolver);
            }
            if let Some(aggregate_plan) = &template_loop.aggregate_render_plan {
                merge_aggregate_render_plan_metadata(aggregate_plan, metadata, resolver);
            }
        }

        TemplateControlFlow::LoopControl(_) => {}
    }
}

fn merge_branch_selector_metadata(
    selector: &TemplateBranchSelector,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    match selector {
        TemplateBranchSelector::Bool(condition) => {
            merge_expression_metadata(condition, metadata, resolver);
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => {
            merge_expression_metadata(scrutinee, metadata, resolver);
        }
    }
}

fn merge_loop_header_metadata(
    header: &TemplateLoopHeader,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            merge_expression_metadata(condition, metadata, resolver);
        }

        TemplateLoopHeader::Range { range, .. } => {
            merge_expression_metadata(&range.start, metadata, resolver);
            merge_expression_metadata(&range.end, metadata, resolver);
            if let Some(step) = &range.step {
                merge_expression_metadata(step, metadata, resolver);
            }
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            merge_expression_metadata(iterable, metadata, resolver);
        }
    }
}

fn merge_render_plan_metadata(
    plan: &TemplateRenderPlan,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    for piece in &plan.pieces {
        merge_render_piece_metadata(piece, metadata, resolver);
    }
}

fn merge_render_piece_metadata(
    piece: &RenderPiece,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    match piece {
        RenderPiece::DynamicExpression(dynamic) => {
            if let Some(subscription) = &dynamic.reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
            merge_expression_metadata(&dynamic.expression, metadata, resolver);
        }

        RenderPiece::ChildTemplate(child) => {
            merge_expression_metadata(&child.expression, metadata, resolver);
        }

        RenderPiece::Text(_)
        | RenderPiece::HeadContent(_)
        | RenderPiece::LoopControl(_)
        | RenderPiece::Slot(_)
        | RenderPiece::RuntimeSlotSite(_) => {}
    }
}

fn merge_aggregate_render_plan_metadata(
    plan: &TemplateAggregateRenderPlan,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    for piece in &plan.pieces {
        match piece {
            TemplateAggregatePiece::Render(render_piece) => {
                merge_render_piece_metadata(render_piece, metadata, resolver);
            }

            TemplateAggregatePiece::Aggregate => {}
        }
    }
}

fn merge_runtime_slot_application_metadata(
    plan: &RuntimeSlotApplicationPlan,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    merge_render_plan_metadata(&plan.wrapper_plan, metadata, resolver);

    for source in &plan.contribution_sources {
        merge_render_plan_metadata(&source.render_plan, metadata, resolver);
    }

    for site in &plan.slot_sites {
        for piece in &site.render_plan.pieces {
            match piece {
                RuntimeSlotSitePiece::Render(render_piece) => {
                    merge_render_piece_metadata(render_piece, metadata, resolver);
                }

                RuntimeSlotSitePiece::ContributionSource(_) => {}
            }
        }
    }
}
