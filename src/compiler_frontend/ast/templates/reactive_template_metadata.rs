//! Reactive template metadata structural traversal.
//!
//! WHAT: walks the structural shape of a `Template` and merges reactive metadata
//! using a caller-supplied expression resolver. Runtime handoffs are walked
//! through owned expression payloads. The pre-overlay structural pass traverses
//! raw templates with a `TemplateIrStore`; the post-normalization view path
//! requires a Finalized `TirView`, validates its overlay authority, and reads
//! effective node and expression state.
//! Runtime slot-site render pieces are traversed through the store's slot plan so
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
use crate::compiler_frontend::ast::templates::template::Template;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIrId, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrStore,
    TemplateLoopHeaderExpressionSites, TemplateSlotPlanId, TemplateSlotSiteRenderPiece,
    TemplateSlotSiteRenderPlan, TemplateTirPhase, TirView, TirViewIdentity,
    finalized_tir_view_for_template,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use std::collections::HashSet;

type ReactiveMetadataResolver<'a> =
    dyn FnMut(&Expression) -> Result<Option<ReactiveTemplateMetadata>, CompilerError> + 'a;

/// Store-aware reactive-template metadata traversal.
///
/// WHAT: walks same-store TIR roots where they are authoritative. Both linear
///       and control-flow templates read their body metadata from the
///       `Composed`-or-later TIR root. The TIR node walker discovers
///       `BranchChain` selectors, `Loop` headers, branch/fallback/loop bodies
///       and aggregate wrappers directly from the TIR tree.
/// WHY: AST finalization has access to the module-scoped `TemplateIrStore`.
/// Walking the TIR root keeps reactive metadata aligned with the finalized
/// TIR representation that render-unit preparation wrote into the store.
/// Below-Composed roots remain semantic absence; once a same-store
/// Composed-or-later root is selected, missing authority is an internal error.
pub(crate) fn merge_reactive_template_metadata_with_store_and_resolver(
    template: &Template,
    store: &TemplateIrStore,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    if let Some(template_id) = authoritative_tir_root_for_template(template, store)? {
        merge_tir_template_metadata(store, template_id, metadata, resolver)?;
    }

    Ok(())
}

/// Returns the authoritative same-store TIR root for a template.
///
/// WHAT: accepts same-store TIR references whose phase is Composed or later.
///       The reference must belong to the current store. Both linear and
///       control-flow templates use this root because the TIR node walker
///       handles `BranchChain` and `Loop` nodes directly.
/// WHY: the finalized TIR root is the structural authority for template output.
///      Reactive metadata must follow that authority or subscriptions can be
///      dropped and hide reactive backend requirements.
fn authoritative_tir_root_for_template(
    template: &Template,
    store: &TemplateIrStore,
) -> Result<Option<TemplateIrId>, CompilerError> {
    let reference = &template.tir_reference;
    if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
        return Ok(None);
    }

    if store.get_template(reference.root).is_none() {
        return Err(CompilerError::compiler_error(format!(
            "reactive TIR metadata: template root {} does not exist in the store",
            reference.root
        )));
    }

    Ok(Some(reference.root))
}

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

/// Read-only TIR node metadata walker.
///
/// WHAT: recursively walks a TIR node from `store` and merges reactive metadata
/// — dynamic expression metadata, reactive subscriptions, child template bodies,
/// branch selectors, loop headers, loop bodies, and aggregate wrappers —
/// consistently with the existing owned-handoff and content traversals.
/// WHY: the store-aware control-flow body path reads finalized TIR body roots.
///      This walker mirrors the owned-handoff node walker so reactive metadata
///      parity is preserved across representations.
fn merge_tir_node_metadata(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    merge_tir_node_metadata_with_slot_site_mode(
        store,
        node_id,
        RuntimeSlotSiteMetadataMode::WalkRenderPieces,
        metadata,
        resolver,
    )
}

#[derive(Clone, Copy)]
enum RuntimeSlotSiteMetadataMode {
    WalkRenderPieces,
    WrapperNodeOnly,
}

/// Raw-store TIR node metadata walker.
///
/// WHAT: recursively walks a TIR node tree from `store`, merging dynamic-expression
///       metadata, reactive subscriptions, nested templates, branch selectors,
///       loop headers, and runtime slot-site render plans.
/// WHY: this is the implementation of `merge_tir_node_metadata` and the store-aware
///      body-root path. The slot-site mode lets callers decide whether a runtime
///      slot site should walk its render pieces (body traversal) or stop at the
///      wrapper node (runtime slot application wrapper traversal).
fn merge_tir_node_metadata_with_slot_site_mode(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    runtime_slot_site_mode: RuntimeSlotSiteMetadataMode,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    let Some(node) = store.get_node(node_id) else {
        return Err(CompilerError::compiler_error(format!(
            "reactive TIR metadata: node {} does not exist in store {}",
            node_id, "the store"
        )));
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    *child,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                )?;
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            expression,
            reactive_subscription,
            ..
        } => {
            if let Some(subscription) = reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
            merge_expression_metadata(expression, metadata, resolver)?;
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if reference.phase.is_at_least(TemplateTirPhase::Composed) {
                merge_tir_template_metadata(store, reference.root, metadata, resolver)?;
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            merge_tir_template_metadata(store, *template, metadata, resolver)?;
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                merge_branch_selector_metadata(&branch.selector, metadata, resolver)?;
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    branch.body,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                )?;
            }

            if let Some(fallback) = fallback {
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    *fallback,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                )?;
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            body,
            aggregate_wrapper,
            ..
        } => {
            merge_loop_header_metadata(header, metadata, resolver)?;
            merge_tir_node_metadata_with_slot_site_mode(
                store,
                *body,
                runtime_slot_site_mode,
                metadata,
                resolver,
            )?;

            if let Some(aggregate_wrapper) = aggregate_wrapper {
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    *aggregate_wrapper,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                )?;
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
            // The slot plan lives in the same store. Walk the render pieces for
            // this concrete site so nested subscriptions inside site render roots
            // are discovered, matching the owned runtime-slot-handoff traversal.
            // Contribution-source pieces do not directly carry a TIR render root;
            // their metadata is reached through the source's own `render_root`.
            let slot_plan = store.get_slot_plan(*plan).ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "reactive TIR metadata: slot plan {} does not exist in store {}",
                    plan, "the store"
                ))
            })?;
            let site_plan = slot_plan
                .slot_sites
                .iter()
                .find(|s| s.site == *site)
                .ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "reactive TIR metadata: slot site {:?} does not exist in slot plan {}",
                        site, plan
                    ))
                })?;

            if matches!(
                runtime_slot_site_mode,
                RuntimeSlotSiteMetadataMode::WrapperNodeOnly
            ) {
                return Ok(());
            }

            merge_tir_slot_site_render_plan_metadata(
                store,
                &site_plan.render_plan,
                slot_plan.contribution_sources.len(),
                metadata,
                resolver,
            )?;
        }

        // Text, Slot, AggregateOutput, and LoopControl carry no reactive
        // expression metadata.
        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => {}
    }

    Ok(())
}

/// Merges reactive metadata from a TIR template root.
///
/// WHAT: reads the template from `store`; if it carries a runtime slot plan,
///       delegates to the runtime-slot application walker, otherwise walks the root node.
/// WHY: runtime slot application templates have a wrapper root and a separate plan;
///      both must be walked to match the owned handoff traversal.
fn merge_tir_template_metadata(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    let tir_template = store.get_template(template_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "reactive TIR metadata: template {} does not exist in store {}",
            template_id, "the store"
        ))
    })?;

    if let Some(slot_plan_id) = tir_template.runtime_slot_plan {
        merge_tir_runtime_slot_application_metadata(
            store,
            tir_template.root,
            slot_plan_id,
            metadata,
            resolver,
        )?;
        return Ok(());
    }

    merge_tir_node_metadata(store, tir_template.root, metadata, resolver)
}

/// Merges reactive metadata for a runtime slot application template.
///
/// WHAT: walks the wrapper root in wrapper-only mode, then walks contribution-source
///       render roots and site render plans in normal mode.
/// WHY: mirrors the owned runtime-slot handoff traversal so subscriptions inside
///      contribution sources and site render pieces are discovered consistently.
fn merge_tir_runtime_slot_application_metadata(
    store: &TemplateIrStore,
    wrapper_root: TemplateIrNodeId,
    slot_plan_id: TemplateSlotPlanId,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    merge_tir_node_metadata_with_slot_site_mode(
        store,
        wrapper_root,
        RuntimeSlotSiteMetadataMode::WrapperNodeOnly,
        metadata,
        resolver,
    )?;

    let slot_plan = store.get_slot_plan(slot_plan_id).ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "reactive TIR metadata: slot plan {} does not exist in store {}",
            slot_plan_id, "the store"
        ))
    })?;

    // Runtime slot application metadata mirrors the owned handoff traversal:
    // wrapper first, contribution-source render roots once, and direct render
    // pieces through the site plan after routed sources.
    for source in &slot_plan.contribution_sources {
        merge_tir_node_metadata(store, source.render_root, metadata, resolver)?;
    }

    for site in &slot_plan.slot_sites {
        merge_tir_slot_site_render_plan_metadata(
            store,
            &site.render_plan,
            slot_plan.contribution_sources.len(),
            metadata,
            resolver,
        )?;
    }

    Ok(())
}

/// Merges reactive metadata from a TIR slot-site render plan.
///
/// WHAT: walks each `Render` piece root and ignores `ContributionSource` pieces;
///       their metadata is reached through the source's own render root.
fn merge_tir_slot_site_render_plan_metadata(
    store: &TemplateIrStore,
    render_plan: &TemplateSlotSiteRenderPlan,
    contribution_source_count: usize,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    for piece in &render_plan.pieces {
        match piece {
            TemplateSlotSiteRenderPiece::Render(root) => {
                merge_tir_node_metadata(store, *root, metadata, resolver)?;
            }

            TemplateSlotSiteRenderPiece::ContributionSource(source_id) => {
                if source_id.0 >= contribution_source_count {
                    return Err(CompilerError::compiler_error(format!(
                        "reactive TIR metadata: slot render plan references missing contribution source {:?}",
                        source_id
                    )));
                }
            }
        }
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

/// Store-backed metadata traversal.
///
/// WHAT: resolves the required finalized effective `TirView` and walks every
///       reachable TIR root through its exact phase and overlay identity.
///       Missing authority is reported as an internal compiler error rather than
///       downgraded to a raw-store interpretation.
/// WHY: post-normalization metadata is part of the AST-to-HIR boundary. The
///      finalized module-store view is the representation that includes
///      expression, slot-resolution, and wrapper-context overlay dimensions.
pub(crate) fn merge_reactive_template_metadata_with_store(
    template: &Template,
    store: &TemplateIrStore,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
) -> Result<(), CompilerError> {
    let view = finalized_tir_view_for_template(template, store)?;
    let mut traversal = TirViewMetadataTraversal::default();
    merge_reactive_template_metadata_from_tir_view(&view, metadata, resolver, &mut traversal)
}

#[derive(Default)]
struct TirViewMetadataTraversal {
    active_views: HashSet<TirViewIdentity>,
    completed_views: HashSet<TirViewIdentity>,
}

/// Merges reactive metadata by walking the finalized effective `TirView`.
///
/// WHAT: walks the TIR body and every referenced child view, resolving each
///       view's own store for slot-plan side tables. Expression overrides are
///       optional semantic values; all structural authority is required.
/// WHY: this is the post-normalization metadata owner. Keeping traversal on the
///      finalized view preserves every overlay dimension without bypassing the
///      exact view identity.
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
    let (root_node_id, slot_plan_id) = {
        view.expression_overlay()?;
        view.slot_resolution_overlay()?;
        view.wrapper_context_overlay()?;

        let root_template = view.root_template()?;
        (root_template.root, root_template.runtime_slot_plan)
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

    Ok(())
}

/// View-based TIR node metadata walker.
///
/// WHAT: reads finalized nodes and expressions through `TirView` overlay lookups.
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
            merge_effective_expression_metadata(view, *site_id, expression, metadata, resolver)?;
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;

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
            merge_tir_view_loop_header_metadata(view, header, header_sites, metadata, resolver)?;

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

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => {}
    }

    Ok(())
}

fn merge_tir_view_loop_header_metadata(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    header_sites: &TemplateLoopHeaderExpressionSites,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut ReactiveMetadataResolver<'_>,
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
            )?;
        }

        (
            TemplateLoopHeader::Range { range, .. },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            merge_effective_expression_metadata(view, *start, &range.start, metadata, resolver)?;
            merge_effective_expression_metadata(view, *end, &range.end, metadata, resolver)?;
            if let (Some(step_expr), Some(step_site_id)) = (&range.step, *step) {
                merge_effective_expression_metadata(
                    view,
                    step_site_id,
                    step_expr,
                    metadata,
                    resolver,
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

    let render_roots = match site_id {
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
            site_plan
                .render_plan
                .pieces
                .iter()
                .filter_map(|piece| match piece {
                    TemplateSlotSiteRenderPiece::Render(root) => Some(*root),
                    TemplateSlotSiteRenderPiece::ContributionSource(_) => None,
                })
                .collect()
        }
        None => slot_plan
            .slot_sites
            .iter()
            .flat_map(|site_plan| site_plan.render_plan.pieces.iter())
            .filter_map(|piece| match piece {
                TemplateSlotSiteRenderPiece::Render(root) => Some(*root),
                TemplateSlotSiteRenderPiece::ContributionSource(_) => None,
            })
            .collect(),
    };

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
) -> Result<(), CompilerError> {
    if let Some(expression) = view.effective_expression_for_site(site_id)? {
        merge_expression_metadata(expression, metadata, resolver)?;
    } else {
        // No override is a semantic absence, so the structural expression remains
        // the effective payload for this site.
        merge_expression_metadata(stored, metadata, resolver)?;
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
        ReactiveSubscription, Style, TemplateSegmentOrigin, TemplateType,
    };
    use crate::compiler_frontend::ast::templates::tir::{
        TemplateIr, TemplateIrId, TemplateIrNode, TemplateIrNodeId, TemplateIrNodeKind,
        TemplateIrStore, TemplateIrSummary, TemplateTirPhase, TemplateTirReference,
        TemplateViewContext, TirExpressionOverlay,
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
            kind: TemplateType::StringFunction,
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
        merge_reactive_template_metadata_with_store_and_resolver(
            template,
            store,
            &mut metadata,
            &mut |expression| Ok(expression.reactive_template.clone()),
        )?;
        Ok(metadata)
    }

    #[test]
    fn store_tir_walk_collects_dynamic_subscription_metadata() {
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
        merge_reactive_template_metadata_with_store(
            &template,
            &store,
            &mut metadata,
            &mut |expression| Ok(expression.reactive_template.clone()),
        )
        .expect("effective metadata walk should succeed");
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
            kind: TemplateType::StringFunction,
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
