//! Reactive template metadata structural traversal.
//!
//! WHAT: walks the structural shape of a `Template` and merges reactive metadata
//! using a caller-supplied expression resolver. Runtime handoffs are walked
//! through owned expression payloads. Template-backed metadata uses one exact
//! `TirView` structural path and reads effective node and expression state.
//! Runtime slot-site render pieces are traversed through the view's slot plan so
//! nested subscriptions inside site render roots are discovered.
//! WHY: template shape is owned by the template subsystem, but expression metadata
//! resolution differs by caller. AST finalization supplies flow-aware resolution
//! using function-flow maps and the value environment.

use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ReactiveTemplateMetadata,
};
use crate::compiler_frontend::ast::templates::runtime_handoff;
use crate::compiler_frontend::ast::templates::runtime_handoff::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIrNodeId, TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
    TemplateSlotPlanId, TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan, TemplateTirPhase,
    TemplateWrapperSetId, TirView, TirViewIdentity,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use std::collections::HashSet;

type ReactiveMetadataResolver<'a> =
    dyn FnMut(&Expression) -> Result<Option<ReactiveTemplateMetadata>, CompilerError> + 'a;

fn merge_owned_runtime_template_handoff_metadata(
    handoff: &OwnedRuntimeTemplateHandoff,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    runtime_handoff::walk_owned_runtime_template_handoff(handoff, &mut |node| {
        merge_owned_runtime_template_node_metadata(node, metadata, resolver)
    })
}

fn merge_owned_runtime_slot_application_handoff_metadata(
    handoff: &OwnedRuntimeSlotApplicationHandoff,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    runtime_handoff::walk_owned_runtime_slot_application_handoff(handoff, &mut |node| {
        merge_owned_runtime_template_node_metadata(node, metadata, resolver)
    })
}

/// Computes reactive template metadata for an owned runtime-template handoff.
///
/// WHAT: merges subscriptions and parameter dependencies from the handoff body,
/// including nested runtime slot applications, using the caller's expression resolver.
/// WHY: handoff expressions that reach AST reactive metadata propagation before
/// HIR normalization carry only a template-backed shell. This result-aware
/// traversal lets the annotation pass fill in structural metadata while
/// propagating resolver failures to the AST finalizer.
pub(crate) fn metadata_for_owned_runtime_template_handoff(
    handoff: &OwnedRuntimeTemplateHandoff,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<ReactiveTemplateMetadata, CompilerError> {
    let mut metadata = ReactiveTemplateMetadata::template_backed();
    merge_owned_runtime_template_handoff_metadata(handoff, &mut metadata, resolver)?;
    Ok(metadata)
}

/// Computes reactive template metadata for an owned runtime slot application handoff.
///
/// WHAT: merges subscriptions and parameter dependencies from the wrapper,
/// contribution sources, and slot-site render pieces using the caller's expression
/// resolver.
/// WHY: runtime slot application handoffs constructed before HIR normalization need
/// their structural reactive metadata discovered by the finalization annotation pass,
/// matching the collection path already used for raw `Template` values.
pub(crate) fn metadata_for_owned_runtime_slot_application_handoff(
    handoff: &OwnedRuntimeSlotApplicationHandoff,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<ReactiveTemplateMetadata, CompilerError> {
    let mut metadata = ReactiveTemplateMetadata::template_backed();
    merge_owned_runtime_slot_application_handoff_metadata(handoff, &mut metadata, resolver)?;
    Ok(metadata)
}

/// Merges reactive metadata from a single owned runtime-template node.
///
/// WHAT: handles the node kinds that can carry reactive metadata directly:
///       dynamic expressions, reactive text, branch selectors, and loop headers.
/// WHY: owned runtime-template nodes are the post-composition representation of
///      runtime template handoffs; this walker keeps metadata collection aligned
///      with the handoff structure produced by HIR normalization.
fn merge_owned_runtime_template_node_metadata(
    node: &OwnedRuntimeTemplateNode,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    match node {
        OwnedRuntimeTemplateNode::DynamicExpression {
            expression,
            reactive_subscription,
            ..
        } => {
            if let Some(subscription) = reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
            merge_expression_metadata(expression, metadata, resolver)?;
        }

        OwnedRuntimeTemplateNode::BranchChain { branches, .. } => {
            for branch in branches {
                merge_branch_selector_metadata(&branch.selector, metadata, resolver)?;
            }
        }

        OwnedRuntimeTemplateNode::Loop { header, .. } => {
            merge_loop_header_metadata(header, metadata, resolver)?;
        }

        OwnedRuntimeTemplateNode::Text {
            reactive_subscription,
            ..
        } => {
            if let Some(subscription) = reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
        }

        OwnedRuntimeTemplateNode::Sequence { .. }
        | OwnedRuntimeTemplateNode::ChildTemplate { .. }
        | OwnedRuntimeTemplateNode::ConditionalWrapper { .. }
        | OwnedRuntimeTemplateNode::AggregateOutput
        | OwnedRuntimeTemplateNode::LoopControl { .. }
        | OwnedRuntimeTemplateNode::RuntimeSlotSite { .. }
        | OwnedRuntimeTemplateNode::Slot { .. } => {}
    }

    Ok(())
}

/// Merges reactive metadata for an expression reached during the template walk.
///
/// WHAT: asks the caller's resolver for metadata; if the resolver returns none,
///       falls back to walking runtime-template and runtime-slot handoff payloads.
/// WHY: the resolver decides whether to recurse into nested templates, so the
///      fallback only handles handoff expressions that bypass the resolver path.
fn merge_expression_metadata(
    expression: &Expression,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    if let Some(expression_metadata) = resolver(expression)? {
        metadata.merge_from(&expression_metadata);
        return Ok(());
    }

    match &expression.kind {
        ExpressionKind::RuntimeTemplateHandoff(handoff) => {
            merge_owned_runtime_template_handoff_metadata(handoff, metadata, resolver)?;
        }

        ExpressionKind::RuntimeSlotApplicationHandoff(handoff) => {
            merge_owned_runtime_slot_application_handoff_metadata(handoff, metadata, resolver)?;
        }

        _ => {}
    }

    Ok(())
}

/// Merges reactive metadata from a branch selector expression.
///
/// WHAT: resolves the boolean condition or option-present scrutinee through the
///       caller's expression resolver.
fn merge_branch_selector_metadata(
    selector: &TemplateBranchSelector,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    match selector {
        TemplateBranchSelector::Bool(condition) => {
            merge_expression_metadata(condition, metadata, resolver)?;
        }

        TemplateBranchSelector::OptionPresentCapture { scrutinee, .. } => {
            merge_expression_metadata(scrutinee, metadata, resolver)?;
        }
    }

    Ok(())
}

/// Merges reactive metadata from a loop header.
///
/// WHAT: resolves the condition, range bounds, or collection iterable through the
///       caller's expression resolver.
fn merge_loop_header_metadata(
    header: &TemplateLoopHeader,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    match header {
        TemplateLoopHeader::Conditional { condition } => {
            merge_expression_metadata(condition, metadata, resolver)?;
        }

        TemplateLoopHeader::Range { range, .. } => {
            merge_expression_metadata(&range.start, metadata, resolver)?;
            merge_expression_metadata(&range.end, metadata, resolver)?;
            if let Some(step) = &range.step {
                merge_expression_metadata(step, metadata, resolver)?;
            }
        }

        TemplateLoopHeader::Collection { iterable, .. } => {
            merge_expression_metadata(iterable, metadata, resolver)?;
        }
    }

    Ok(())
}

/// Merges template-backed metadata through one exact TIR view.
///
/// WHAT: walks every reachable structural root through exact view transitions,
///       resolving effective expressions, child templates, helpers, wrappers,
///       control-flow bodies, and runtime slot plans.
/// WHY: callers own the phase boundary. Flow-aware collection supplies a
///      Composed-or-later view, while post-normalization collection supplies a
///      Finalized view. The reducer owns only read-only traversal state.
pub(crate) fn merge_reactive_template_metadata(
    view: &TirView<'_>,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    if !view.phase().is_at_least(TemplateTirPhase::Composed) {
        return Err(CompilerError::compiler_error(format!(
            "reactive TIR metadata: view rooted at {} is below the required Composed phase",
            view.root_ref()
        )));
    }

    let mut traversal = TirViewMetadataTraversal::default();
    merge_reactive_template_metadata_from_tir_view(view, metadata, resolver, &mut traversal)
}

#[derive(Default)]
struct TirViewMetadataTraversal {
    active_views: HashSet<TirViewIdentity>,
    completed_views: HashSet<TirViewIdentity>,
}

#[derive(Clone, Copy)]
enum RuntimeSlotSiteMetadataMode {
    WalkRenderPieces,
    WrapperNodeOnly,
}

/// Merges reactive metadata by walking one exact effective `TirView`.
///
/// WHAT: walks the TIR body and every referenced child view, resolving each
///       view's own store for slot-plan side tables. Expression overrides are
///       optional semantic values; all structural authority is required.
/// WHY: keeping traversal on the exact view preserves every overlay dimension
///      without bypassing the view identity supplied by the caller.
fn merge_reactive_template_metadata_from_tir_view(
    view: &TirView<'_>,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    let identity = view.identity();
    if traversal.completed_views.contains(&identity) {
        return Ok(());
    }
    if !traversal.active_views.insert(identity) {
        return Ok(());
    }

    let result = merge_tir_view_root_contents(view, metadata, resolver, traversal);

    traversal.active_views.remove(&identity);
    if result.is_ok() {
        traversal.completed_views.insert(identity);
    }

    result
}

fn merge_tir_view_root_contents(
    view: &TirView<'_>,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    let (root_node_id, slot_plan_id, conditional_child_wrapper_set) = {
        view.expression_overlay()?;
        view.slot_resolution_overlay()?;
        view.wrapper_context_overlay()?;

        let root_template = view.root_template()?;
        (
            root_template.root,
            root_template.runtime_slot_plan,
            root_template.conditional_child_wrapper_set,
        )
    };

    if let Some(slot_plan_id) = slot_plan_id {
        merge_tir_view_node_metadata(
            view,
            root_node_id,
            RuntimeSlotSiteMetadataMode::WrapperNodeOnly,
            metadata,
            resolver,
            traversal,
        )?;

        let (contribution_roots, site_render_roots) =
            view_slot_plan_render_roots(view, slot_plan_id, None)?;

        for source_root in contribution_roots {
            merge_tir_view_node_metadata(
                view,
                source_root,
                RuntimeSlotSiteMetadataMode::WalkRenderPieces,
                metadata,
                resolver,
                traversal,
            )?;
        }

        for site_render_root in site_render_roots {
            merge_tir_view_node_metadata(
                view,
                site_render_root,
                RuntimeSlotSiteMetadataMode::WalkRenderPieces,
                metadata,
                resolver,
                traversal,
            )?;
        }
    } else {
        merge_tir_view_node_metadata(
            view,
            root_node_id,
            RuntimeSlotSiteMetadataMode::WalkRenderPieces,
            metadata,
            resolver,
            traversal,
        )?;
    }

    if let Some(wrapper_set_id) = conditional_child_wrapper_set {
        merge_tir_view_wrapper_set_metadata(view, wrapper_set_id, metadata, resolver, traversal)?;
    }

    Ok(())
}

/// Exact-view TIR node metadata walker.
///
/// WHAT: reads nodes and expressions through `TirView` overlay lookups.
///       Dynamic-expression splices, branch selectors, and loop-header
///       expressions prefer the override expression when the view provides
///       one and otherwise use the stored structural expression directly.
/// WHY: this keeps the walker aligned with the final effective TIR without
///      copying nodes or expressions in the common no-overlay case.
fn merge_tir_view_node_metadata(
    view: &TirView<'_>,
    node_ref: TemplateIrNodeId,
    runtime_slot_site_mode: RuntimeSlotSiteMetadataMode,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    let node = view.effective_node(node_ref)?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            for child in children {
                merge_tir_view_node_metadata(
                    view,
                    child,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                    traversal,
                )?;
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            expression,
            reactive_subscription,
            site_id,
            ..
        } => {
            if let Some(subscription) = reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
            merge_effective_expression_metadata(
                view, *site_id, expression, metadata, resolver, traversal,
            )?;
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
        } => {
            let reference = *reference;
            let occurrence_id = *occurrence_id;

            let wrapper_set_id =
                view.effective_wrapper_context(occurrence_id)?
                    .and_then(|context| {
                        (!context.skip_parent_child_wrappers)
                            .then_some(context.inherited_wrapper_set)
                            .flatten()
                    });
            merge_optional_wrapper_set_metadata(
                view,
                wrapper_set_id,
                metadata,
                resolver,
                traversal,
            )?;

            let child_view = view.structural_child(reference)?;
            merge_reactive_template_metadata_from_tir_view(
                &child_view,
                metadata,
                resolver,
                traversal,
            )?;
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            let insert_view = view.structural_helper(template_id)?;
            merge_reactive_template_metadata_from_tir_view(
                &insert_view,
                metadata,
                resolver,
                traversal,
            )?;
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let mut body_ids = Vec::with_capacity(branches.len());
            for branch in branches {
                merge_effective_expression_metadata(
                    view,
                    branch.selector_site_id,
                    branch.condition_expression(),
                    metadata,
                    resolver,
                    traversal,
                )?;
                body_ids.push(branch.body);
            }
            let fallback = *fallback;

            for body in body_ids {
                merge_tir_view_node_metadata(
                    view,
                    body,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                    traversal,
                )?;
            }

            if let Some(fallback) = fallback {
                merge_tir_view_node_metadata(
                    view,
                    fallback,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                    traversal,
                )?;
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            merge_tir_view_loop_header_metadata(
                view,
                header,
                header_sites,
                metadata,
                resolver,
                traversal,
            )?;

            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;

            merge_tir_view_node_metadata(
                view,
                body,
                runtime_slot_site_mode,
                metadata,
                resolver,
                traversal,
            )?;

            if let Some(aggregate_wrapper) = aggregate_wrapper {
                merge_tir_view_node_metadata(
                    view,
                    aggregate_wrapper,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                    traversal,
                )?;
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
            let plan = *plan;
            let site = *site;

            let (_, render_roots) = view_slot_plan_render_roots(view, plan, Some(site))?;
            if matches!(
                runtime_slot_site_mode,
                RuntimeSlotSiteMetadataMode::WrapperNodeOnly
            ) {
                return Ok(());
            }

            for render_root in render_roots {
                merge_tir_view_node_metadata(
                    view,
                    render_root,
                    RuntimeSlotSiteMetadataMode::WalkRenderPieces,
                    metadata,
                    resolver,
                    traversal,
                )?;
            }
        }

        TemplateIrNodeKind::Text { .. } => {}

        TemplateIrNodeKind::Slot { placeholder } => {
            merge_optional_wrapper_set_metadata(
                view,
                placeholder.applied_child_wrapper_set,
                metadata,
                resolver,
                traversal,
            )?;
            merge_optional_wrapper_set_metadata(
                view,
                placeholder.child_wrapper_set,
                metadata,
                resolver,
                traversal,
            )?;

            if let Some(resolution) = view.effective_slot_resolution(placeholder.occurrence_id)? {
                for source in resolution.sources() {
                    let source_view = view.resolved_slot_source(*source)?;
                    merge_reactive_template_metadata_from_tir_view(
                        &source_view,
                        metadata,
                        resolver,
                        traversal,
                    )?;
                }
            }
        }

        TemplateIrNodeKind::AggregateOutput | TemplateIrNodeKind::LoopControl { .. } => {}
    }

    Ok(())
}

fn merge_optional_wrapper_set_metadata(
    view: &TirView<'_>,
    wrapper_set_id: Option<TemplateWrapperSetId>,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    if let Some(wrapper_set_id) = wrapper_set_id {
        merge_tir_view_wrapper_set_metadata(view, wrapper_set_id, metadata, resolver, traversal)?;
    }

    Ok(())
}

fn merge_tir_view_wrapper_set_metadata(
    view: &TirView<'_>,
    wrapper_set_id: TemplateWrapperSetId,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    let wrapper_references = view
        .store()
        .get_wrapper_set(wrapper_set_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "reactive TIR metadata: wrapper set {} does not exist in owning store {}",
                wrapper_set_id,
                view.root_ref()
            ))
        })?
        .wrappers
        .clone();

    for wrapper_reference in wrapper_references {
        let wrapper_view = view.wrapper(wrapper_reference)?;
        merge_reactive_template_metadata_from_tir_view(
            &wrapper_view,
            metadata,
            resolver,
            traversal,
        )?;
    }

    Ok(())
}

fn merge_tir_view_loop_header_metadata(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    header_sites: &TemplateLoopHeaderExpressionSites,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    match (header, header_sites) {
        (
            TemplateLoopHeader::Conditional { condition },
            TemplateLoopHeaderExpressionSites::Conditional { condition: site_id },
        ) => {
            merge_effective_expression_metadata(
                view,
                *site_id,
                condition.as_ref(),
                metadata,
                resolver,
                traversal,
            )?;
        }

        (
            TemplateLoopHeader::Range { range, .. },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            merge_effective_expression_metadata(
                view,
                *start,
                &range.start,
                metadata,
                resolver,
                traversal,
            )?;
            merge_effective_expression_metadata(
                view, *end, &range.end, metadata, resolver, traversal,
            )?;
            if let (Some(step_expr), Some(step_site_id)) = (&range.step, *step) {
                merge_effective_expression_metadata(
                    view,
                    step_site_id,
                    step_expr,
                    metadata,
                    resolver,
                    traversal,
                )?;
            } else if range.step.is_some() {
                return Err(CompilerError::compiler_error(
                    "reactive TIR metadata: loop range step expression is missing its expression site",
                ));
            }
        }

        (
            TemplateLoopHeader::Collection { iterable, .. },
            TemplateLoopHeaderExpressionSites::Collection { iterable: site_id },
        ) => {
            merge_effective_expression_metadata(
                view,
                *site_id,
                iterable.as_ref(),
                metadata,
                resolver,
                traversal,
            )?;
        }

        _ => {
            return Err(CompilerError::compiler_error(
                "reactive TIR metadata: loop header shape does not match its expression sites",
            ));
        }
    }

    Ok(())
}

fn view_slot_plan_render_roots(
    view: &TirView<'_>,
    plan_id: TemplateSlotPlanId,
    site_id: Option<RuntimeSlotSiteId>,
) -> Result<(Vec<TemplateIrNodeId>, Vec<TemplateIrNodeId>), CompilerError> {
    let store = view.store();
    let slot_plan = store.get_slot_plan(plan_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "reactive TIR metadata: slot plan {} does not exist in owning store {}",
            plan_id,
            view.root_ref()
        ))
    })?;

    let contribution_roots = if site_id.is_none() {
        slot_plan
            .contribution_sources
            .iter()
            .map(|source| source.render_root)
            .collect()
    } else {
        Vec::new()
    };

    let render_plans: Vec<&TemplateSlotSiteRenderPlan> = match site_id {
        Some(site_id) => {
            let site_plan = slot_plan
                .slot_sites
                .iter()
                .find(|site_plan| site_plan.site == site_id)
                .ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "reactive TIR metadata: slot site {:?} does not exist in slot plan {}",
                        site_id, plan_id
                    ))
                })?;
            vec![&site_plan.render_plan]
        }
        None => slot_plan
            .slot_sites
            .iter()
            .map(|site_plan| &site_plan.render_plan)
            .collect(),
    };

    let mut render_roots = Vec::new();
    for render_plan in render_plans {
        for piece in &render_plan.pieces {
            match piece {
                TemplateSlotSiteRenderPiece::Render(root) => render_roots.push(*root),
                TemplateSlotSiteRenderPiece::ContributionSource(source_id) => {
                    if slot_plan.contribution_sources.get(source_id.0).is_none() {
                        return Err(CompilerError::compiler_error(format!(
                            "reactive TIR metadata: slot render plan references missing contribution source {:?}",
                            source_id
                        )));
                    }
                }
            }
        }
    }

    Ok((contribution_roots, render_roots))
}

/// Merges metadata for the effective expression at `site_id`.
///
/// WHAT: resolves the view's expression override for `site_id` and merges the
///       effective expression's metadata. When no override exists, the stored
///       structural expression is merged directly without copying.
/// WHY: avoids cloning the common case where no overlay is present, and keeps
///      the node borrow alive only for the merge call.
fn merge_effective_expression_metadata(
    view: &TirView<'_>,
    site_id: ExpressionSiteId,
    stored: &Expression,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    if let Some(expression) = view.effective_expression_for_site(site_id)? {
        merge_view_expression_metadata(view, expression, metadata, resolver, traversal)?;
    } else {
        // No override is a semantic absence, so the structural expression remains
        // the effective payload for this site.
        merge_view_expression_metadata(view, stored, metadata, resolver, traversal)?;
    }

    Ok(())
}

/// Merges an expression using the current structural or nested-value view.
///
/// WHAT: enters a direct template or a template reached through top-level
///       coercions with its durable nested-value reference, while leaving all
///       other expressions at the caller's outer resolver boundary.
/// WHY: nested values do not inherit structural overlays from their container,
///      and coercion fallback must retain the caller's outer-expression policy.
fn merge_view_expression_metadata(
    view: &TirView<'_>,
    expression: &Expression,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
    traversal: &mut TirViewMetadataTraversal,
) -> Result<(), CompilerError> {
    let mut candidate = expression;
    let template = loop {
        match &candidate.kind {
            ExpressionKind::Template(template) => break Some(template),
            ExpressionKind::Coerced { value, .. } => candidate = value,
            _ => break None,
        }
    };

    if let Some(template) = template {
        let nested_view = view.nested_template_value(template.tir_reference)?;
        merge_reactive_template_metadata_from_tir_view(
            &nested_view,
            metadata,
            resolver,
            traversal,
        )?;
    } else {
        merge_expression_metadata(expression, metadata, resolver)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::ast::expressions::expression::{
        ReactiveSource, ReactiveSourceKind,
    };
    use crate::compiler_frontend::ast::templates::runtime_handoff::{
        OwnedRuntimeTemplateBody, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
    };
    use crate::compiler_frontend::ast::templates::template::{
        ReactiveSubscription, SlotKey, Style, Template, TemplateSegmentOrigin, TemplateType,
    };
    use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirChildReference;
    use crate::compiler_frontend::ast::templates::tir::{
        TemplateIr, TemplateIrId, TemplateIrNode, TemplateIrNodeId, TemplateIrNodeKind,
        TemplateIrStore, TemplateIrSummary, TemplateTirPhase, TemplateTirReference,
        TemplateViewContext, TemplateWrapperReference, TemplateWrapperSet, TirExpressionOverlay,
        TirSlotPlaceholder, TirSlotResolution, TirSlotResolutionOverlay,
    };
    use crate::compiler_frontend::compiler_errors::CompilerError;
    use crate::compiler_frontend::datatypes::DataType;
    use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
    use crate::compiler_frontend::symbols::interned_path::InternedPath;
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
    use crate::compiler_frontend::value_mode::ValueMode;

    fn location() -> SourceLocation {
        SourceLocation::default()
    }

    fn reactive_expression(
        string_table: &mut StringTable,
        name: &str,
    ) -> (Expression, ReactiveSubscription) {
        let source = ReactiveSource {
            path: InternedPath::from_single_str(name, string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let subscription = ReactiveSubscription {
            source: source.clone(),
            type_id: builtin_type_ids::INT,
            location: location(),
        };
        let expression = Expression::new(
            ExpressionKind::Reference(source.path.clone()),
            location(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableReference,
        )
        .with_reactive_source(source)
        .with_reactive_template_metadata(ReactiveTemplateMetadata {
            template_backed: false,
            subscriptions: vec![subscription.clone()],
            template_value_parameters: vec![],
        });
        (expression, subscription)
    }

    fn template_from_node(
        store: &mut TemplateIrStore,
        node: TemplateIrNodeId,
        phase: TemplateTirPhase,
        context: TemplateViewContext,
    ) -> Template {
        let root = store.push_template(TemplateIr::new(
            node,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            location(),
        ));
        Template {
            tir_reference: TemplateTirReference {
                root,
                phase,
                context,
            },
            location: location(),
        }
    }

    fn merge(
        template: &Template,
        store: &TemplateIrStore,
    ) -> Result<ReactiveTemplateMetadata, CompilerError> {
        let mut metadata = ReactiveTemplateMetadata::template_backed();
        let reference = template.tir_reference;
        let view = TirView::with_minimum_phase(
            store,
            reference.root,
            reference.phase,
            TemplateTirPhase::Composed,
            reference.context,
        )?;
        merge_reactive_template_metadata(&view, &mut metadata, &mut |expression| {
            Ok(expression.reactive_template.clone())
        })?;
        Ok(metadata)
    }

    #[test]
    fn composed_view_walk_collects_dynamic_subscription_metadata() {
        let mut strings = StringTable::new();
        let (expression, subscription) = reactive_expression(&mut strings, "value");
        let mut store = TemplateIrStore::new();
        let site_id = store.next_expression_site_id();
        let node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: Some(subscription.clone()),
                site_id,
            },
            location(),
        ));
        let template = template_from_node(
            &mut store,
            node,
            TemplateTirPhase::Composed,
            TemplateViewContext::default(),
        );

        let metadata = merge(&template, &store).expect("metadata walk should succeed");
        assert!(metadata.subscriptions.contains(&subscription));
    }

    #[test]
    fn finalized_view_walk_reads_expression_overlay_metadata() {
        let mut strings = StringTable::new();
        let (structural, _) = reactive_expression(&mut strings, "structural");
        let (overlay_expression, subscription) = reactive_expression(&mut strings, "overlay");
        let mut store = TemplateIrStore::new();
        let site_id = store.next_expression_site_id();
        let node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(structural),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            location(),
        ));
        let overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(site_id, Box::new(overlay_expression))],
        });
        let context = TemplateViewContext {
            expression_overlay: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        };
        let template = template_from_node(&mut store, node, TemplateTirPhase::Finalized, context);

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        let view = TirView::with_minimum_phase(
            &store,
            template.tir_reference.root,
            template.tir_reference.phase,
            TemplateTirPhase::Finalized,
            template.tir_reference.context,
        )
        .expect("finalized view should be available");
        merge_reactive_template_metadata(&view, &mut metadata, &mut |expression| {
            Ok(expression.reactive_template.clone())
        })
        .expect("effective metadata walk should succeed");
        assert!(metadata.subscriptions.contains(&subscription));
    }

    #[test]
    fn composed_view_walk_enters_parsed_structural_child() {
        let mut strings = StringTable::new();
        let (expression, subscription) = reactive_expression(&mut strings, "parsed-child");
        let mut store = TemplateIrStore::new();
        let child_site_id = store.next_expression_site_id();
        let child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: Some(subscription.clone()),
                site_id: child_site_id,
            },
            location(),
        ));
        let child = template_from_node(
            &mut store,
            child_node,
            TemplateTirPhase::Parsed,
            TemplateViewContext::default(),
        );
        let child_reference = TemplateTirChildReference::new(
            child.tir_reference.root,
            TemplateTirPhase::Parsed,
            TemplateViewContext::default(),
        );
        let child_occurrence_id = store.next_child_template_occurrence_id();
        let root_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: child_reference,
                occurrence_id: child_occurrence_id,
            },
            location(),
        ));
        let root = template_from_node(
            &mut store,
            root_node,
            TemplateTirPhase::Composed,
            TemplateViewContext::default(),
        );

        let metadata = merge(&root, &store).expect("parsed structural child should be readable");
        assert!(metadata.subscriptions.contains(&subscription));
    }

    #[test]
    fn resolved_slot_source_contributes_metadata_through_exact_view_context() {
        let mut strings = StringTable::new();
        let (expression, subscription) = reactive_expression(&mut strings, "slot-source");
        let mut store = TemplateIrStore::new();
        let source_site_id = store.next_expression_site_id();
        let source_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: Some(subscription.clone()),
                site_id: source_site_id,
            },
            location(),
        ));
        let source = template_from_node(
            &mut store,
            source_node,
            TemplateTirPhase::Composed,
            TemplateViewContext::default(),
        );
        let occurrence_id = store.next_slot_occurrence_id();
        let slot_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot {
                placeholder: TirSlotPlaceholder::new(SlotKey::Default, occurrence_id, location()),
            },
            location(),
        ));
        let slot_resolution_overlay =
            store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
                resolutions: vec![(
                    occurrence_id,
                    TirSlotResolution::resolved(SlotKey::Default, vec![source.tir_reference.root]),
                )],
            });
        let root = template_from_node(
            &mut store,
            slot_node,
            TemplateTirPhase::Composed,
            TemplateViewContext {
                slot_resolution: Some(slot_resolution_overlay),
                ..TemplateViewContext::default()
            },
        );

        let metadata = merge(&root, &store).expect("resolved slot source should be readable");
        assert!(metadata.subscriptions.contains(&subscription));
    }

    #[test]
    fn non_template_coercion_is_resolved_at_the_outer_expression_boundary() {
        let mut strings = StringTable::new();
        let (inner, inner_subscription) = reactive_expression(&mut strings, "coerced-inner");
        let (_, outer_subscription) = reactive_expression(&mut strings, "coerced-outer");
        let mut coerced = Expression::coerced(inner, builtin_type_ids::FLOAT);
        coerced.reactive_template = Some(ReactiveTemplateMetadata {
            template_backed: false,
            subscriptions: vec![outer_subscription.clone()],
            template_value_parameters: vec![],
        });
        let mut store = TemplateIrStore::new();
        let site_id = store.next_expression_site_id();
        let node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(coerced),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            location(),
        ));
        let template = template_from_node(
            &mut store,
            node,
            TemplateTirPhase::Composed,
            TemplateViewContext::default(),
        );

        let metadata = merge(&template, &store).expect("outer coercion should be resolved");
        assert!(metadata.subscriptions.contains(&outer_subscription));
        assert!(!metadata.subscriptions.contains(&inner_subscription));
    }

    #[test]
    fn wrapper_transition_contributes_metadata_through_exact_view() {
        let mut strings = StringTable::new();
        let (expression, subscription) = reactive_expression(&mut strings, "wrapper");
        let mut store = TemplateIrStore::new();
        let wrapper_site_id = store.next_expression_site_id();
        let wrapper_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: Some(subscription.clone()),
                site_id: wrapper_site_id,
            },
            location(),
        ));
        let wrapper = template_from_node(
            &mut store,
            wrapper_node,
            TemplateTirPhase::Composed,
            TemplateViewContext::default(),
        );
        let wrapper_set = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![TemplateWrapperReference::new(
                wrapper.tir_reference.root,
                TemplateTirPhase::Composed,
                TemplateViewContext::default(),
            )],
        });
        let root_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Text {
                text: strings.intern("root"),
                byte_len: 4,
                origin: TemplateSegmentOrigin::Body,
            },
            location(),
        ));
        let root = store.push_template({
            let mut template = TemplateIr::new(
                root_node,
                Style::default(),
                TemplateType::StringFunction,
                TemplateIrSummary::default(),
                location(),
            );
            template.conditional_child_wrapper_set = Some(wrapper_set);
            template
        });
        let root_template = Template {
            tir_reference: TemplateTirReference {
                root,
                phase: TemplateTirPhase::Composed,
                context: TemplateViewContext::default(),
            },
            location: location(),
        };

        let metadata = merge(&root_template, &store).expect("wrapper should be readable");
        assert!(metadata.subscriptions.contains(&subscription));
    }

    #[test]
    fn owned_runtime_handoff_metadata_is_traversed() {
        let mut strings = StringTable::new();
        let (expression, subscription) = reactive_expression(&mut strings, "handoff");
        let handoff = OwnedRuntimeTemplateHandoff {
            body: OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::DynamicExpression {
                expression: Box::new(expression),
                reactive_subscription: Some(subscription.clone()),
            }),
            location: location(),
        };

        let metadata = metadata_for_owned_runtime_template_handoff(&handoff, &mut |expression| {
            Ok(expression.reactive_template.clone())
        })
        .expect("handoff metadata walk should succeed");
        assert!(metadata.subscriptions.contains(&subscription));
    }

    #[test]
    fn missing_composed_root_returns_compiler_error() {
        let store = TemplateIrStore::new();
        let template = Template {
            tir_reference: TemplateTirReference {
                root: TemplateIrId::new(99),
                phase: TemplateTirPhase::Composed,
                context: TemplateViewContext::default(),
            },
            location: location(),
        };

        let error = merge(&template, &store).expect_err("missing root should fail");
        assert!(error.msg.contains("does not exist"));
    }
}
