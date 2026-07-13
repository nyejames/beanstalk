//! Reactive template metadata structural traversal.
//!
//! WHAT: walks the structural shape of a `Template` and merges reactive metadata
//! using a caller-supplied expression resolver. Runtime handoffs are walked
//! through owned expression payloads. Raw templates are traversed only with a
//! `TemplateIrStore`: control-flow bodies come from same-store TIR roots and
//! linear templates use same-store `Composed`-or-later roots.
//! When the module-local `TemplateIrRegistry` is also available and the template's
//! same-store TIR reference is `Composed` or later, the effective `TirView` is
//! authoritative for expression overlays: metadata is read through those overlay
//! lookups for dynamic splices, branch selectors, and loop headers.
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
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::{
    ExpressionSiteId, TemplateIrId, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrRegistry,
    TemplateIrStore, TemplateLoopHeaderExpressionSites, TemplateNodeRef, TemplateSlotPlanId,
    TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan, TemplateTirPhase, TirView,
};
use std::sync::Arc;

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
/// Missing or below-Composed roots are not an invitation to fall back to a
/// non-TIR representation.
pub(crate) fn merge_reactive_template_metadata_with_store_and_resolver(
    template: &Template,
    store: &TemplateIrStore,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    if let Some(root) = authoritative_tir_root_for_template(template, store) {
        merge_tir_node_metadata(store, root, metadata, resolver);
    }
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
) -> Option<TemplateIrNodeId> {
    let reference = template.tir_reference.as_ref()?;
    if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
        return None;
    }

    let store_owner = store.owner();
    if !std::sync::Arc::ptr_eq(&reference.store_owner, &store_owner) {
        return None;
    }

    if reference.root.store_id != store.store_id() {
        return None;
    }

    store
        .get_template(reference.root.template_id)
        .map(|tir_template| tir_template.root)
}

fn merge_owned_runtime_template_handoff_metadata(
    handoff: &OwnedRuntimeTemplateHandoff,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    runtime_handoff::walk_owned_runtime_template_handoff(handoff, &mut |node| {
        merge_owned_runtime_template_node_metadata(node, metadata, resolver)
    });
}

fn merge_owned_runtime_slot_application_handoff_metadata(
    handoff: &OwnedRuntimeSlotApplicationHandoff,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    runtime_handoff::walk_owned_runtime_slot_application_handoff(handoff, &mut |node| {
        merge_owned_runtime_template_node_metadata(node, metadata, resolver)
    });
}

/// Computes reactive template metadata for an owned runtime-template handoff.
///
/// WHAT: merges subscriptions and parameter dependencies from the handoff body,
/// including nested runtime slot applications, using the caller's expression resolver.
/// WHY: handoff expressions that reach AST reactive metadata propagation before
/// HIR normalization carry only a template-backed shell. This helper lets the
/// annotation pass fill in the structural metadata through the existing
/// owned-handoff walker instead of duplicating it.
pub(crate) fn metadata_for_owned_runtime_template_handoff(
    handoff: &OwnedRuntimeTemplateHandoff,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) -> ReactiveTemplateMetadata {
    let mut metadata = ReactiveTemplateMetadata::template_backed();
    merge_owned_runtime_template_handoff_metadata(handoff, &mut metadata, resolver);
    metadata
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) -> ReactiveTemplateMetadata {
    let mut metadata = ReactiveTemplateMetadata::template_backed();
    merge_owned_runtime_slot_application_handoff_metadata(handoff, &mut metadata, resolver);
    metadata
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    match node {
        OwnedRuntimeTemplateNode::DynamicExpression {
            expression,
            reactive_subscription,
            ..
        } => {
            if let Some(subscription) = reactive_subscription {
                metadata.push_subscription(subscription.clone());
            }
            merge_expression_metadata(expression, metadata, resolver);
        }

        OwnedRuntimeTemplateNode::BranchChain { branches, .. } => {
            for branch in branches {
                merge_branch_selector_metadata(&branch.selector, metadata, resolver);
            }
        }

        OwnedRuntimeTemplateNode::Loop { header, .. } => {
            merge_loop_header_metadata(header, metadata, resolver);
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    if let Some(expression_metadata) = resolver(expression) {
        metadata.merge_from(&expression_metadata);
        return;
    }

    match &expression.kind {
        ExpressionKind::RuntimeTemplateHandoff(handoff) => {
            merge_owned_runtime_template_handoff_metadata(handoff, metadata, resolver);
        }

        ExpressionKind::RuntimeSlotApplicationHandoff(handoff) => {
            merge_owned_runtime_slot_application_handoff_metadata(handoff, metadata, resolver);
        }

        _ => {}
    }
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    merge_tir_node_metadata_with_slot_site_mode(
        store,
        node_id,
        RuntimeSlotSiteMetadataMode::WalkRenderPieces,
        metadata,
        resolver,
    );
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let Some(node) = store.get_node(node_id) else {
        return;
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
                );
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
            merge_expression_metadata(expression, metadata, resolver);
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(template_id) = reference.template_id_in_store(store.store_id()) {
                merge_tir_template_metadata(store, template_id, metadata, resolver);
            }
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            merge_tir_template_metadata(store, *template, metadata, resolver);
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                merge_branch_selector_metadata(&branch.selector, metadata, resolver);
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    branch.body,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
            }

            if let Some(fallback) = fallback {
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    *fallback,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            body,
            aggregate_wrapper,
            ..
        } => {
            merge_loop_header_metadata(header, metadata, resolver);
            merge_tir_node_metadata_with_slot_site_mode(
                store,
                *body,
                runtime_slot_site_mode,
                metadata,
                resolver,
            );

            if let Some(aggregate_wrapper) = aggregate_wrapper {
                merge_tir_node_metadata_with_slot_site_mode(
                    store,
                    *aggregate_wrapper,
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
            if matches!(
                runtime_slot_site_mode,
                RuntimeSlotSiteMetadataMode::WrapperNodeOnly
            ) {
                return;
            }

            // The slot plan lives in the same store. Walk the render pieces for
            // this concrete site so nested subscriptions inside site render roots
            // are discovered, matching the owned runtime-slot-handoff traversal.
            // Contribution-source pieces do not directly carry a TIR render root;
            // their metadata is reached through the source's own `render_root`.
            let Some(slot_plan) = store.get_slot_plan(*plan) else {
                return;
            };
            let Some(site_plan) = slot_plan.slot_sites.iter().find(|s| s.site == *site) else {
                return;
            };
            merge_tir_slot_site_render_plan_metadata(
                store,
                &site_plan.render_plan,
                metadata,
                resolver,
            );
        }

        // Text, Slot, AggregateOutput, and LoopControl carry no reactive
        // expression metadata.
        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => {}
    }
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let Some(tir_template) = store.get_template(template_id) else {
        return;
    };

    if let Some(slot_plan_id) = tir_template.runtime_slot_plan {
        merge_tir_runtime_slot_application_metadata(
            store,
            tir_template.root,
            slot_plan_id,
            metadata,
            resolver,
        );
        return;
    }

    merge_tir_node_metadata(store, tir_template.root, metadata, resolver);
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    merge_tir_node_metadata_with_slot_site_mode(
        store,
        wrapper_root,
        RuntimeSlotSiteMetadataMode::WrapperNodeOnly,
        metadata,
        resolver,
    );

    let Some(slot_plan) = store.get_slot_plan(slot_plan_id) else {
        return;
    };

    // Runtime slot application metadata mirrors the owned handoff traversal:
    // wrapper first, contribution-source render roots once, and direct render
    // pieces through the site plan after routed sources.
    for source in &slot_plan.contribution_sources {
        merge_tir_node_metadata(store, source.render_root, metadata, resolver);
    }

    for site in &slot_plan.slot_sites {
        merge_tir_slot_site_render_plan_metadata(store, &site.render_plan, metadata, resolver);
    }
}

/// Merges reactive metadata from a TIR slot-site render plan.
///
/// WHAT: walks each `Render` piece root and ignores `ContributionSource` pieces;
///       their metadata is reached through the source's own render root.
fn merge_tir_slot_site_render_plan_metadata(
    store: &TemplateIrStore,
    render_plan: &TemplateSlotSiteRenderPlan,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    for piece in &render_plan.pieces {
        match piece {
            TemplateSlotSiteRenderPiece::Render(root) => {
                merge_tir_node_metadata(store, *root, metadata, resolver);
            }

            TemplateSlotSiteRenderPiece::ContributionSource(_) => {}
        }
    }
}

/// Merges reactive metadata from a branch selector expression.
///
/// WHAT: resolves the boolean condition or option-present scrutinee through the
///       caller's expression resolver.
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

/// Merges reactive metadata from a loop header.
///
/// WHAT: resolves the condition, range bounds, or collection iterable through the
///       caller's expression resolver.
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

/// Store-and-registry-aware metadata traversal.
///
/// WHAT: prefers a final effective `TirView` when the template owns a
///       same-store Composed-or-later TIR reference and the module-local registry is
///       available. The view path resolves effective expressions through the
///       expression-overlay dimension for dynamic-expression splices, branch
///       selectors, and loop-header expressions. All unsupported roots
///       (missing registry, non-finalized phase, cross-store reference,
///       non-expression overlay dimensions, malformed view identity, or any
///       other surface not safely covered) fall back to the raw store-aware
///       traversal.
/// WHY: after AST normalization the finalized TIR tree is the authoritative
///      template representation. Reading reactive metadata through `TirView`
///      keeps it aligned with expression overlays for templates that already
///      carry TIR-backed roots.
pub(crate) fn merge_reactive_template_metadata_with_store_and_registry(
    template: &Template,
    store: &TemplateIrStore,
    registry: &TemplateIrRegistry,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    if let Some(view) = try_finalized_tir_view_for_template(template, store, registry) {
        merge_reactive_template_metadata_from_tir_view(template, &view, store, metadata, resolver);
    } else {
        merge_reactive_template_metadata_with_store_and_resolver(
            template, store, metadata, resolver,
        );
    }
}

/// Tries to construct an effective expression-overlay `TirView` for a same-store template.
///
/// WHAT: returns a view only when the template carries a same-store
///       Composed-or-later TIR reference, the registry owns that store and overlay set, and the
///       overlay set does not use unsupported dimensions. Slot-resolution and
///       wrapper-context overlays are not yet safely readable through this
///       metadata path, so their presence causes a conservative fallback.
/// WHY: the caller decides whether to use the view or fall back to the raw
///      store-aware path. Returning `None` for every unsupported root keeps the
///      fallback decision local and explicit without reviving compatibility
///      content.
fn try_finalized_tir_view_for_template<'a>(
    template: &Template,
    store: &TemplateIrStore,
    registry: &'a TemplateIrRegistry,
) -> Option<TirView<'a>> {
    let reference = template.tir_reference.as_ref()?;

    if !reference.phase.is_at_least(TemplateTirPhase::Composed) {
        return None;
    }

    if !Arc::ptr_eq(&reference.store_owner, &store.owner()) {
        return None;
    }

    if reference.root.store_id != store.store_id() {
        return None;
    }

    let overlay_set = registry.overlay_set(reference.overlay_set_id)?;
    if overlay_set.slot_resolution.is_some() || overlay_set.wrapper_context.is_some() {
        return None;
    }

    // Expression-overlay normalization stores every reachable same-store
    // expression payload on the root reference's overlay set before metadata
    // refresh runs. With no expression overlay, the raw-store traversal is
    // equivalent for this first slice and avoids a view lookup on every node.
    overlay_set.expression_overrides?;

    TirView::with_minimum_phase(
        registry,
        reference.root,
        reference.phase,
        TemplateTirPhase::Composed,
        reference.overlay_set_id,
    )
    .ok()
}

/// Merges reactive metadata by walking the effective expression-overlay `TirView`.
///
/// WHAT: walks the finalized TIR body tree, reading effective expressions from
///       the view's expression-overlay dimension when present and falling back
///       to the stored structural expression otherwise. It preserves runtime
///       slot-site traversal through the store's slot plan and the owned
///       conditional child-wrapper node from the AST template.
/// WHY: this is the narrow view-based replacement for the raw-store body walk.
///      Keeping slot-plan enumeration in the store and child-wrapper handling
///      on the AST template preserves the metadata behavior established by the
///      existing store-aware path.
fn merge_reactive_template_metadata_from_tir_view(
    _template: &Template,
    view: &TirView<'_>,
    store: &TemplateIrStore,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let slot_plan_id = match view.root_template() {
        Ok(root_template) => root_template.runtime_slot_plan,
        Err(_) => {
            // `try_finalized_tir_view_for_template` validates the root before
            // constructing the view. If this fails, the registry is internally
            // inconsistent and there is no safe metadata to add from this path.
            return;
        }
    };

    if let Some(slot_plan_id) = slot_plan_id {
        merge_tir_view_root_metadata(
            view,
            store,
            RuntimeSlotSiteMetadataMode::WrapperNodeOnly,
            metadata,
            resolver,
        );

        if let Some(slot_plan) = store.get_slot_plan(slot_plan_id) {
            for source in &slot_plan.contribution_sources {
                merge_tir_view_node_metadata(
                    view,
                    store,
                    TemplateNodeRef::new(view.root_ref().store_id, source.render_root),
                    RuntimeSlotSiteMetadataMode::WalkRenderPieces,
                    metadata,
                    resolver,
                );
            }

            for site_plan in &slot_plan.slot_sites {
                merge_tir_view_slot_site_render_plan_metadata(
                    view,
                    store,
                    &site_plan.render_plan,
                    metadata,
                    resolver,
                );
            }
        }
    } else {
        merge_tir_view_root_metadata(
            view,
            store,
            RuntimeSlotSiteMetadataMode::WalkRenderPieces,
            metadata,
            resolver,
        );
    }
}

fn merge_tir_view_root_metadata(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    runtime_slot_site_mode: RuntimeSlotSiteMetadataMode,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let root_node_id = match view.root_template() {
        Ok(root_template) => root_template.root,
        Err(_) => return,
    };

    merge_tir_view_node_metadata(
        view,
        store,
        TemplateNodeRef::new(view.root_ref().store_id, root_node_id),
        runtime_slot_site_mode,
        metadata,
        resolver,
    );
}

/// View-based TIR node metadata walker.
///
/// WHAT: mirrors the raw-store `merge_tir_node_metadata_with_slot_site_mode`,
///       but reads effective expressions through `TirView` overlay lookups.
///       Dynamic-expression splices, branch selectors, and loop-header
///       expressions prefer the override expression when the view provides
///       one, falling back to the stored structural expression directly.
/// WHY: this keeps the walker aligned with the final effective TIR without
///      copying nodes or expressions in the common no-overlay case.
fn merge_tir_view_node_metadata(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    node_ref: TemplateNodeRef,
    runtime_slot_site_mode: RuntimeSlotSiteMetadataMode,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let store_id = view.root_ref().store_id;

    let node = match view.effective_node(node_ref) {
        Ok(node) => node,
        Err(_) => return,
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let children = children.clone();
            drop(node);
            for child in children {
                merge_tir_view_node_metadata(
                    view,
                    store,
                    TemplateNodeRef::new(store_id, child),
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
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
                view,
                *site_id,
                expression.as_ref(),
                metadata,
                resolver,
            );
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let reference = *reference;
            drop(node);

            if reference.root.store_id != store_id {
                return;
            }

            let child_view =
                match view.child_view(reference.root, reference.phase, reference.overlay_set_id) {
                    Ok(view) => view,
                    Err(_) => {
                        merge_tir_template_metadata(
                            store,
                            reference.root.template_id,
                            metadata,
                            resolver,
                        );
                        return;
                    }
                };

            let child_overlay_set = match child_view.overlay_set() {
                Ok(set) => set,
                Err(_) => {
                    merge_tir_template_metadata(
                        store,
                        reference.root.template_id,
                        metadata,
                        resolver,
                    );
                    return;
                }
            };

            if child_overlay_set.slot_resolution.is_some()
                || child_overlay_set.wrapper_context.is_some()
                || child_overlay_set.expression_overrides.is_none()
            {
                merge_tir_template_metadata(store, reference.root.template_id, metadata, resolver);
                return;
            }

            merge_tir_view_root_metadata(
                &child_view,
                store,
                RuntimeSlotSiteMetadataMode::WalkRenderPieces,
                metadata,
                resolver,
            );
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            let template_id = *template;
            drop(node);
            merge_tir_template_metadata(store, template_id, metadata, resolver);
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                merge_effective_expression_metadata(
                    view,
                    branch.selector_site_id,
                    branch.condition_expression(),
                    metadata,
                    resolver,
                );
            }

            let bodies: Vec<_> = branches.iter().map(|branch| branch.body).collect();
            let fallback = *fallback;
            drop(node);

            for body in bodies {
                merge_tir_view_node_metadata(
                    view,
                    store,
                    TemplateNodeRef::new(store_id, body),
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
            }

            if let Some(fallback) = fallback {
                merge_tir_view_node_metadata(
                    view,
                    store,
                    TemplateNodeRef::new(store_id, fallback),
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
            }
        }

        TemplateIrNodeKind::Loop {
            header,
            header_sites,
            body,
            aggregate_wrapper,
            ..
        } => {
            merge_tir_view_loop_header_metadata(view, header, header_sites, metadata, resolver);

            let body = *body;
            let aggregate_wrapper = *aggregate_wrapper;
            drop(node);

            merge_tir_view_node_metadata(
                view,
                store,
                TemplateNodeRef::new(store_id, body),
                runtime_slot_site_mode,
                metadata,
                resolver,
            );

            if let Some(aggregate_wrapper) = aggregate_wrapper {
                merge_tir_view_node_metadata(
                    view,
                    store,
                    TemplateNodeRef::new(store_id, aggregate_wrapper),
                    runtime_slot_site_mode,
                    metadata,
                    resolver,
                );
            }
        }

        TemplateIrNodeKind::RuntimeSlotSite { plan, site } => {
            if matches!(
                runtime_slot_site_mode,
                RuntimeSlotSiteMetadataMode::WrapperNodeOnly
            ) {
                return;
            }

            let plan = *plan;
            let site = *site;
            drop(node);

            let Some(slot_plan) = store.get_slot_plan(plan) else {
                return;
            };
            let Some(site_plan) = slot_plan.slot_sites.iter().find(|s| s.site == site) else {
                return;
            };
            merge_tir_view_slot_site_render_plan_metadata(
                view,
                store,
                &site_plan.render_plan,
                metadata,
                resolver,
            );
        }

        TemplateIrNodeKind::Text { .. }
        | TemplateIrNodeKind::Slot { .. }
        | TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::LoopControl { .. } => {}
    }
}

fn merge_tir_view_loop_header_metadata(
    view: &TirView<'_>,
    header: &TemplateLoopHeader,
    header_sites: &TemplateLoopHeaderExpressionSites,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
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
            );
        }

        (
            TemplateLoopHeader::Range { range, .. },
            TemplateLoopHeaderExpressionSites::Range { start, end, step },
        ) => {
            merge_effective_expression_metadata(view, *start, &range.start, metadata, resolver);
            merge_effective_expression_metadata(view, *end, &range.end, metadata, resolver);
            if let (Some(step_expr), Some(step_site_id)) = (&range.step, *step) {
                merge_effective_expression_metadata(
                    view,
                    step_site_id,
                    step_expr,
                    metadata,
                    resolver,
                );
            } else if let Some(step_expr) = &range.step {
                merge_expression_metadata(step_expr, metadata, resolver);
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
            );
        }

        // Mismatched header/site shape is an internal invariant issue. Use
        // only the stored header expressions and do not try to resolve sites.
        _ => match header {
            TemplateLoopHeader::Conditional { condition } => {
                merge_expression_metadata(condition.as_ref(), metadata, resolver);
            }
            TemplateLoopHeader::Range { range, .. } => {
                merge_expression_metadata(&range.start, metadata, resolver);
                merge_expression_metadata(&range.end, metadata, resolver);
                if let Some(step) = &range.step {
                    merge_expression_metadata(step, metadata, resolver);
                }
            }
            TemplateLoopHeader::Collection { iterable, .. } => {
                merge_expression_metadata(iterable.as_ref(), metadata, resolver);
            }
        },
    }
}

fn merge_tir_view_slot_site_render_plan_metadata(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    render_plan: &TemplateSlotSiteRenderPlan,
    metadata: &mut ReactiveTemplateMetadata,
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    let store_id = view.root_ref().store_id;

    for piece in &render_plan.pieces {
        match piece {
            TemplateSlotSiteRenderPiece::Render(root) => {
                merge_tir_view_node_metadata(
                    view,
                    store,
                    TemplateNodeRef::new(store_id, *root),
                    RuntimeSlotSiteMetadataMode::WalkRenderPieces,
                    metadata,
                    resolver,
                );
            }

            TemplateSlotSiteRenderPiece::ContributionSource(_) => {}
        }
    }
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
    resolver: &mut impl FnMut(&Expression) -> Option<ReactiveTemplateMetadata>,
) {
    match view.effective_expression_for_site(site_id) {
        Ok(Some(expression)) => merge_expression_metadata(expression, metadata, resolver),
        _ => merge_expression_metadata(stored, metadata, resolver),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::ast::expressions::expression::{
        ReactiveSource, ReactiveSourceKind,
    };
    use crate::compiler_frontend::ast::templates::runtime_handoff::{
        OwnedRuntimeTemplateBody, OwnedRuntimeTemplateHandoff,
    };
    use crate::compiler_frontend::ast::templates::template::{
        ReactiveSubscription, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
    };
    use crate::compiler_frontend::ast::templates::template_control_flow::{
        TemplateBranchSelector, TemplateLoopHeader,
    };
    use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotContributionSourceId;
    use crate::compiler_frontend::ast::templates::tir::{
        ExpressionSiteId, TemplateIrBranch, TemplateIrBuilder, TemplateIrId, TemplateIrNode,
        TemplateIrNodeId, TemplateIrNodeKind, TemplateIrRegistry, TemplateIrStore,
        TemplateIrSummary, TemplateLoopHeaderExpressionSites, TemplateOverlaySet,
        TemplateOverlaySetId, TemplateRef, TemplateSlotContributionSourcePlan, TemplateSlotPlan,
        TemplateStoreId, TemplateTirPhase, TemplateTirReference, TirExpressionOverlay,
    };
    use crate::compiler_frontend::compiler_messages::source_location::CharPosition;
    use crate::compiler_frontend::datatypes::DataType;
    use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
    use crate::compiler_frontend::symbols::interned_path::InternedPath;
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
    use crate::compiler_frontend::value_mode::ValueMode;
    use std::cell::RefCell;
    use std::rc::Rc;

    fn test_location() -> SourceLocation {
        SourceLocation::default()
    }

    fn location_at(line: i32, column: i32) -> SourceLocation {
        SourceLocation::new(
            InternedPath::default(),
            CharPosition {
                line_number: line,
                char_column: column,
            },
            CharPosition {
                line_number: line,
                char_column: column,
            },
        )
    }

    fn source_expression(source: ReactiveSource) -> Expression {
        Expression::new(
            ExpressionKind::Reference(source.path.clone()),
            test_location(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableReference,
        )
        .with_reactive_source(source)
    }

    fn subscription_expression_at(
        source_name: &str,
        location: SourceLocation,
        string_table: &mut StringTable,
    ) -> (Expression, ReactiveSubscription) {
        let source = ReactiveSource {
            path: InternedPath::from_single_str(source_name, string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let subscription = ReactiveSubscription {
            source: source.clone(),
            type_id: builtin_type_ids::INT,
            location: location.clone(),
        };
        let expression = Expression::new(
            ExpressionKind::Reference(source.path.clone()),
            location,
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

    fn metadata_for_finalized_expression_overlay(
        store_rc: &Rc<RefCell<TemplateIrStore>>,
        template_id: TemplateIrId,
        site_id: ExpressionSiteId,
        overlay_expression: Expression,
    ) -> ReactiveTemplateMetadata {
        let mut registry = TemplateIrRegistry::new();
        let store_id = registry.adopt_store(Rc::clone(store_rc));
        let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(site_id, Box::new(overlay_expression))],
        });
        let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(expression_overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        });
        let store_ref = registry
            .store(store_id)
            .expect("adopted store should remain registered");

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store_ref.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_registry(
            &template,
            &store_ref,
            &registry,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );
        metadata
    }

    /// Builds a TIR dynamic-expression node carrying a named subscription.
    ///
    /// Store-aware body-root tests need distinct subscription identities so
    /// they can assert exact metadata parity.
    fn reactive_dynamic_expression_node(
        builder: &mut TemplateIrBuilder,
        string_table: &mut StringTable,
        source_name: &str,
    ) -> (TemplateIrNodeId, ReactiveSubscription) {
        let source = ReactiveSource {
            path: InternedPath::from_single_str(source_name, string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let subscription = ReactiveSubscription {
            source: source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let expression = Expression::new(
            ExpressionKind::Reference(source.path.clone()),
            test_location(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableReference,
        )
        .with_reactive_source(source);

        let node_id = builder.push_dynamic_expression_node(
            expression,
            TemplateSegmentOrigin::Body,
            Some(subscription.clone()),
            test_location(),
        );
        (node_id, subscription)
    }

    fn bool_selector() -> TemplateBranchSelector {
        TemplateBranchSelector::Bool(Expression::bool(
            true,
            test_location(),
            ValueMode::ImmutableOwned,
        ))
    }

    fn conditional_loop_header() -> TemplateLoopHeader {
        TemplateLoopHeader::Conditional {
            condition: Box::new(Expression::bool(
                true,
                test_location(),
                ValueMode::ImmutableOwned,
            )),
        }
    }

    /// Finishes a simple `StringFunction` template from its root node.
    fn finish_string_function_template(
        builder: &mut TemplateIrBuilder,
        root: TemplateIrNodeId,
    ) -> crate::compiler_frontend::ast::templates::tir::TemplateIrId {
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::empty(),
            test_location(),
        )
    }
    /// Collects reactive metadata through the store-aware TIR body-root traversal.
    fn collect_store_aware_metadata(
        template: &Template,
        store: &TemplateIrStore,
    ) -> ReactiveTemplateMetadata {
        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_resolver(
            template,
            store,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );
        metadata
    }

    #[test]
    fn store_aware_control_flow_metadata_prefers_tir_body_root() {
        let mut string_table = StringTable::new();
        let source = ReactiveSource {
            path: InternedPath::from_single_str("count", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let subscription = ReactiveSubscription {
            source: source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };

        let mut store = TemplateIrStore::new();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let body = builder.push_dynamic_expression_node(
            source_expression(source.clone()),
            TemplateSegmentOrigin::Body,
            Some(subscription.clone()),
            test_location(),
        );
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(Expression::bool(
                true,
                test_location(),
                ValueMode::ImmutableOwned,
            )),
            body,
            test_location(),
        );
        let branch_chain = builder.push_branch_chain_node(vec![branch], None, test_location());
        let root = builder.push_sequence_node(vec![branch_chain], test_location());
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            test_location(),
        );

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            is_composed: false,
            phase: crate::compiler_frontend::ast::templates::tir::TemplateTirPhase::Composed,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_resolver(
            &template,
            &store,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );

        assert_eq!(metadata.subscriptions, vec![subscription]);
    }

    #[test]
    fn metadata_walks_owned_handoff_expressions_without_precomputed_metadata() {
        let mut string_table = StringTable::new();
        let template_source = ReactiveSource {
            path: InternedPath::from_single_str("template_count", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let slot_source = ReactiveSource {
            path: InternedPath::from_single_str("slot_count", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let template_subscription = ReactiveSubscription {
            source: template_source,
            type_id: builtin_type_ids::STRING,
            location: test_location(),
        };
        let slot_subscription = ReactiveSubscription {
            source: slot_source,
            type_id: builtin_type_ids::STRING,
            location: test_location(),
        };
        let template_handoff = OwnedRuntimeTemplateHandoff {
            body: OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Text {
                text: string_table.intern("template"),
                byte_len: "template".len() as u32,
                reactive_subscription: Some(template_subscription.clone()),
                location: test_location(),
            }),
            location: test_location(),
        };
        let slot_handoff = OwnedRuntimeSlotApplicationHandoff {
            wrapper: OwnedRuntimeTemplateNode::Text {
                text: string_table.intern("slot"),
                byte_len: "slot".len() as u32,
                reactive_subscription: Some(slot_subscription.clone()),
                location: test_location(),
            },
            contribution_sources: Vec::new(),
            slot_sites: Vec::new(),
            location: test_location(),
        };
        let mut metadata =
            metadata_for_owned_runtime_template_handoff(&template_handoff, &mut |_| None);
        let slot_metadata =
            metadata_for_owned_runtime_slot_application_handoff(&slot_handoff, &mut |_| None);
        metadata.merge_from(&slot_metadata);

        assert_eq!(
            metadata.subscriptions,
            vec![template_subscription, slot_subscription]
        );
    }

    #[test]
    fn store_aware_metadata_finds_subscription_in_fallback_body_root() {
        let mut string_table = StringTable::new();
        let mut store = TemplateIrStore::new();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let (fallback_body, subscription) =
            reactive_dynamic_expression_node(&mut builder, &mut string_table, "fallback_count");
        let empty_branch_body = builder.push_sequence_node(vec![], test_location());
        let branch = TemplateIrBranch::new(bool_selector(), empty_branch_body, test_location());
        let branch_chain =
            builder.push_branch_chain_node(vec![branch], Some(fallback_body), test_location());
        let root = builder.push_sequence_node(vec![branch_chain], test_location());
        let template_id = finish_string_function_template(&mut builder, root);

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            is_composed: false,
            phase: crate::compiler_frontend::ast::templates::tir::TemplateTirPhase::Composed,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        });

        let metadata = collect_store_aware_metadata(&template, &store);

        assert_eq!(metadata.subscriptions, vec![subscription]);
    }

    #[test]
    fn store_aware_metadata_finds_subscription_in_loop_body_root() {
        let mut string_table = StringTable::new();
        let mut store = TemplateIrStore::new();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let (body_root, subscription) =
            reactive_dynamic_expression_node(&mut builder, &mut string_table, "loop_body_count");
        let loop_node =
            builder.push_loop_node(conditional_loop_header(), body_root, None, test_location());
        let root = builder.push_sequence_node(vec![loop_node], test_location());
        let template_id = finish_string_function_template(&mut builder, root);

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            is_composed: false,
            phase: crate::compiler_frontend::ast::templates::tir::TemplateTirPhase::Composed,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        });

        let metadata = collect_store_aware_metadata(&template, &store);

        assert_eq!(metadata.subscriptions, vec![subscription]);
    }

    #[test]
    fn store_aware_metadata_reads_tir_loop_aggregate_wrapper_subscription() {
        let mut string_table = StringTable::new();
        let mut store = TemplateIrStore::new();
        let mut builder = TemplateIrBuilder::new(&mut store);

        // Put the subscription inside the authoritative TIR loop
        // aggregate-wrapper subtree. The store-aware path must discover it.
        let (tir_wrapper_root, tir_subscription) = reactive_dynamic_expression_node(
            &mut builder,
            &mut string_table,
            "tir_aggregate_count",
        );

        let body_root = builder.push_sequence_node(vec![], test_location());
        let loop_node = builder.push_loop_node(
            conditional_loop_header(),
            body_root,
            Some(tir_wrapper_root),
            test_location(),
        );
        let root = builder.push_sequence_node(vec![loop_node], test_location());
        let template_id = finish_string_function_template(&mut builder, root);

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            is_composed: false,
            phase: crate::compiler_frontend::ast::templates::tir::TemplateTirPhase::Composed,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        });

        let metadata = collect_store_aware_metadata(&template, &store);

        assert_eq!(
            metadata.subscriptions,
            vec![tir_subscription],
            "store-aware metadata should collect the TIR aggregate-wrapper subscription"
        );
    }

    #[test]
    fn store_aware_metadata_discovers_subscription_from_formatted_tir_root() {
        // A linear template with a same-store `Formatted` TIR root must have its
        // body-origin reactive subscriptions discovered directly from the
        // authoritative TIR root.
        let mut string_table = StringTable::new();
        let mut store = TemplateIrStore::new();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let (body_root, subscription) =
            reactive_dynamic_expression_node(&mut builder, &mut string_table, "body_count");
        let template_id = finish_string_function_template(&mut builder, body_root);

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Formatted,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        });

        let metadata = collect_store_aware_metadata(&template, &store);

        assert_eq!(
            metadata.subscriptions,
            vec![subscription],
            "store-aware metadata must discover the reactive subscription from the formatted TIR root"
        );
    }

    #[test]
    fn store_aware_metadata_skips_below_composed_tir_root_subscription() {
        // A below-Composed TIR root is not authoritative for this consumer.
        // Even when the root contains a subscription, the store-aware path
        // must skip it.
        let mut string_table = StringTable::new();
        let mut store = TemplateIrStore::new();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let (body_root, _) =
            reactive_dynamic_expression_node(&mut builder, &mut string_table, "tir_count");
        let template_id = finish_string_function_template(&mut builder, body_root);

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Parsed,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        });

        let metadata = collect_store_aware_metadata(&template, &store);

        assert!(
            metadata.subscriptions.is_empty(),
            "store-aware metadata must not collect subscriptions from a below-Composed TIR root"
        );
    }

    fn registry_adopting_store(
        store_rc: &Rc<RefCell<TemplateIrStore>>,
    ) -> (TemplateIrRegistry, TemplateStoreId, TemplateOverlaySetId) {
        let mut registry = TemplateIrRegistry::new();
        let store_id = registry.adopt_store(Rc::clone(store_rc));
        let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
        (registry, store_id, overlay_set_id)
    }

    fn dynamic_expression_node_with_site(
        store: &mut TemplateIrStore,
        expression: Expression,
        reactive_subscription: Option<ReactiveSubscription>,
    ) -> (TemplateIrNodeId, ExpressionSiteId) {
        let site_id = store.next_expression_site_id();
        let node_id = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription,
                site_id,
            },
            test_location(),
        ));
        (node_id, site_id)
    }

    #[test]
    fn finalized_tir_view_metadata_reads_dynamic_expression_subscription() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let (body_root, subscription) = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            reactive_dynamic_expression_node(&mut builder, &mut string_table, "body_count")
        };

        let template_id = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            finish_string_function_template(&mut builder, body_root)
        };

        let (registry, store_id, overlay_set_id) = registry_adopting_store(&store_rc);
        let store_ref = registry.store(store_id).unwrap();

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store_ref.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_registry(
            &template,
            &store_ref,
            &registry,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );

        assert_eq!(
            metadata.subscriptions,
            vec![subscription],
            "final metadata should collect subscriptions from dynamic expression nodes, either through the view path or the equivalent raw-store path"
        );
    }

    #[test]
    fn finalized_tir_view_metadata_reads_expression_overlay() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let stored_source = ReactiveSource {
            path: InternedPath::from_single_str("stored_source", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let stored_expression = Expression::new(
            ExpressionKind::Reference(stored_source.path.clone()),
            test_location(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableReference,
        )
        .with_reactive_source(stored_source);

        let (body_root, site_id) = {
            let mut store = store_rc.borrow_mut();
            dynamic_expression_node_with_site(&mut store, stored_expression, None)
        };

        let template_id = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            finish_string_function_template(&mut builder, body_root)
        };

        let overlay_source = ReactiveSource {
            path: InternedPath::from_single_str("overlay_source", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let overlay_subscription = ReactiveSubscription {
            source: overlay_source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let overlay_expression = source_expression(overlay_source).with_reactive_template_metadata(
            ReactiveTemplateMetadata {
                template_backed: false,
                subscriptions: vec![overlay_subscription.clone()],
                template_value_parameters: vec![],
            },
        );

        let mut registry = TemplateIrRegistry::new();
        let store_id = registry.adopt_store(Rc::clone(&store_rc));
        let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(site_id, Box::new(overlay_expression))],
        });
        let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(expression_overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        });
        let store_ref = registry.store(store_id).unwrap();

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store_ref.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_registry(
            &template,
            &store_ref,
            &registry,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );

        assert_eq!(
            metadata.subscriptions,
            vec![overlay_subscription],
            "final TirView metadata should read effective expressions from expression overlays"
        );
    }

    #[test]
    fn finalized_tir_view_metadata_walks_runtime_slot_contribution_sources() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let wrapper_expression = Expression::bool(true, test_location(), ValueMode::ImmutableOwned);
        let (wrapper_root, wrapper_site_id) = {
            let mut store = store_rc.borrow_mut();
            dynamic_expression_node_with_site(&mut store, wrapper_expression, None)
        };

        let contribution_source = ReactiveSource {
            path: InternedPath::from_single_str("slot_source", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let contribution_subscription = ReactiveSubscription {
            source: contribution_source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let contribution_root = {
            let mut store = store_rc.borrow_mut();
            let (root, _) = dynamic_expression_node_with_site(
                &mut store,
                source_expression(contribution_source),
                Some(contribution_subscription.clone()),
            );
            root
        };

        let slot_plan_id = {
            let mut store = store_rc.borrow_mut();
            store.push_slot_plan(TemplateSlotPlan {
                location: test_location(),
                contribution_sources: vec![TemplateSlotContributionSourcePlan {
                    source: RuntimeSlotContributionSourceId(0),
                    target: SlotKey::Default,
                    render_root: contribution_root,
                    renders_wrapper_unconditionally: false,
                    location: test_location(),
                }],
                slot_sites: vec![],
            })
        };

        let template_id = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            let template_id = finish_string_function_template(&mut builder, wrapper_root);
            store.templates[template_id.index()].runtime_slot_plan = Some(slot_plan_id);
            template_id
        };

        let wrapper_overlay_source = ReactiveSource {
            path: InternedPath::from_single_str("wrapper_overlay", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let wrapper_overlay_subscription = ReactiveSubscription {
            source: wrapper_overlay_source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let wrapper_overlay_expression = source_expression(wrapper_overlay_source)
            .with_reactive_template_metadata(ReactiveTemplateMetadata {
                template_backed: false,
                subscriptions: vec![wrapper_overlay_subscription.clone()],
                template_value_parameters: vec![],
            });

        let mut registry = TemplateIrRegistry::new();
        let store_id = registry.adopt_store(Rc::clone(&store_rc));
        let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(wrapper_site_id, Box::new(wrapper_overlay_expression))],
        });
        let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(expression_overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        });
        let store_ref = registry.store(store_id).unwrap();

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store_ref.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_registry(
            &template,
            &store_ref,
            &registry,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );

        assert_eq!(
            metadata.subscriptions,
            vec![wrapper_overlay_subscription, contribution_subscription],
            "view-based runtime slot metadata must preserve contribution-source render roots"
        );
    }

    #[test]
    fn registry_metadata_skips_parsed_phase_tir_subscription() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let tir_source = ReactiveSource {
            path: InternedPath::from_single_str("tir_count", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let tir_subscription = ReactiveSubscription {
            source: tir_source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let tir_expression = source_expression(tir_source.clone()).with_reactive_template_metadata(
            ReactiveTemplateMetadata {
                template_backed: false,
                subscriptions: vec![tir_subscription.clone()],
                template_value_parameters: vec![],
            },
        );

        let (body_root, _site_id) = {
            let mut store = store_rc.borrow_mut();
            dynamic_expression_node_with_site(
                &mut store,
                tir_expression,
                Some(tir_subscription.clone()),
            )
        };

        let template_id = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            finish_string_function_template(&mut builder, body_root)
        };

        let (registry, store_id, overlay_set_id) = registry_adopting_store(&store_rc);
        let store_ref = registry.store(store_id).unwrap();

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store_ref.owner(),
            is_composed: false,
            // Phase is below Composed, so neither the view nor raw root is authoritative.
            phase: TemplateTirPhase::Parsed,
            overlay_set_id,
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_registry(
            &template,
            &store_ref,
            &registry,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );

        assert!(
            metadata.subscriptions.is_empty(),
            "registry-backed metadata must not collect subscriptions from a parsed-phase TIR root"
        );
    }

    #[test]
    fn formatted_tir_view_metadata_reads_expression_overlay() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let stored_source = ReactiveSource {
            path: InternedPath::from_single_str("stored_source", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let stored_expression = Expression::new(
            ExpressionKind::Reference(stored_source.path.clone()),
            test_location(),
            builtin_type_ids::INT,
            DataType::Int,
            ValueMode::ImmutableReference,
        )
        .with_reactive_source(stored_source);

        let (body_root, site_id) = {
            let mut store = store_rc.borrow_mut();
            dynamic_expression_node_with_site(&mut store, stored_expression, None)
        };

        let template_id = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            finish_string_function_template(&mut builder, body_root)
        };

        let overlay_source = ReactiveSource {
            path: InternedPath::from_single_str("overlay_source", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let overlay_subscription = ReactiveSubscription {
            source: overlay_source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let overlay_expression = source_expression(overlay_source).with_reactive_template_metadata(
            ReactiveTemplateMetadata {
                template_backed: false,
                subscriptions: vec![overlay_subscription.clone()],
                template_value_parameters: vec![],
            },
        );

        let mut registry = TemplateIrRegistry::new();
        let store_id = registry.adopt_store(Rc::clone(&store_rc));
        let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(site_id, Box::new(overlay_expression))],
        });
        let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(expression_overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        });
        let store_ref = registry.store(store_id).unwrap();

        let mut template = Template::empty();
        template.tir_reference = Some(TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store_ref.owner(),
            is_composed: false,
            phase: TemplateTirPhase::Formatted,
            overlay_set_id,
        });

        let mut metadata = ReactiveTemplateMetadata::template_backed();
        merge_reactive_template_metadata_with_store_and_registry(
            &template,
            &store_ref,
            &registry,
            &mut metadata,
            &mut |expression| expression.reactive_template.clone(),
        );

        assert_eq!(
            metadata.subscriptions,
            vec![overlay_subscription],
            "formatted TirView metadata must resolve the effective expression overlay"
        );
    }

    /// Proves that the finalized `TirView` metadata path keeps the overlay
    /// expression's source location attached to the collected reactive subscription.
    ///
    /// WHAT: installs an expression overlay for a dynamic-expression site whose
    ///       payload carries a subscription whose `location` matches the overlay
    ///       expression location; then asserts the merged metadata contains that
    ///       subscription with the overlay location, not the stored structural
    ///       subscription's location.
    /// WHY: source-location drift here would mislead diagnostics that report the
    ///      reactive source behind a runtime template handoff or fragment.
    #[test]
    fn finalized_tir_view_metadata_preserves_dynamic_expression_overlay_source_location() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let stored_source = ReactiveSource {
            path: InternedPath::from_single_str("stored_source", &mut string_table),
            kind: ReactiveSourceKind::Declaration,
        };
        let stored_subscription = ReactiveSubscription {
            source: stored_source.clone(),
            type_id: builtin_type_ids::INT,
            location: test_location(),
        };
        let stored_expression = source_expression(stored_source.clone())
            .with_reactive_template_metadata(ReactiveTemplateMetadata {
                template_backed: false,
                subscriptions: vec![stored_subscription],
                template_value_parameters: vec![],
            });

        let (body_root, site_id) = {
            let mut store = store_rc.borrow_mut();
            dynamic_expression_node_with_site(&mut store, stored_expression, None)
        };

        let template_id = {
            let mut store = store_rc.borrow_mut();
            let mut builder = TemplateIrBuilder::new(&mut store);
            finish_string_function_template(&mut builder, body_root)
        };

        let overlay_location = location_at(12, 5);
        let (overlay_expression, overlay_subscription) = subscription_expression_at(
            "overlay_source",
            overlay_location.clone(),
            &mut string_table,
        );
        let metadata = metadata_for_finalized_expression_overlay(
            &store_rc,
            template_id,
            site_id,
            overlay_expression,
        );

        assert_eq!(metadata.subscriptions, vec![overlay_subscription.clone()]);
        assert_eq!(
            metadata.subscriptions[0].location, overlay_location,
            "dynamic-expression overlay source location must be preserved in reactive metadata"
        );
    }

    /// Same shape as the dynamic-expression test, but the overlay target is a
    /// `BranchChain` selector site. The metadata walker resolves the effective
    /// selector expression through the view and must keep its source location.
    #[test]
    fn finalized_tir_view_metadata_preserves_branch_selector_overlay_source_location() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let stored_selector = Expression::bool(true, test_location(), ValueMode::ImmutableOwned);
        let overlay_location = location_at(14, 3);
        let (overlay_selector, overlay_subscription) = subscription_expression_at(
            "overlay_branch_source",
            overlay_location.clone(),
            &mut string_table,
        );

        let (template_id, selector_site_id) = {
            let mut store = store_rc.borrow_mut();
            let (template_id, branch_chain_node_id) = {
                let mut builder = TemplateIrBuilder::new(&mut store);
                let branch_body = builder.push_sequence_node(vec![], test_location());
                let branch = TemplateIrBranch::new(
                    TemplateBranchSelector::Bool(stored_selector),
                    branch_body,
                    test_location(),
                );
                let branch_chain_node_id =
                    builder.push_branch_chain_node(vec![branch], None, test_location());
                let template_id = builder.finish_template(
                    branch_chain_node_id,
                    Style::default(),
                    TemplateType::StringFunction,
                    TemplateIrSummary::empty(),
                    test_location(),
                );
                (template_id, branch_chain_node_id)
            };

            let selector_site_id = match &store
                .get_node(branch_chain_node_id)
                .expect("branch chain node should exist")
                .kind
            {
                TemplateIrNodeKind::BranchChain { branches, .. } => branches[0].selector_site_id,
                other => panic!("expected branch chain node, got {other:?}"),
            };

            (template_id, selector_site_id)
        };

        let metadata = metadata_for_finalized_expression_overlay(
            &store_rc,
            template_id,
            selector_site_id,
            overlay_selector,
        );

        assert_eq!(metadata.subscriptions, vec![overlay_subscription]);
        assert_eq!(
            metadata.subscriptions[0].location, overlay_location,
            "branch selector overlay source location must be preserved in reactive metadata"
        );
    }

    /// Same shape again, but the overlay target is a `Loop` header condition site.
    #[test]
    fn finalized_tir_view_metadata_preserves_loop_header_overlay_source_location() {
        let mut string_table = StringTable::new();
        let store_rc = Rc::new(RefCell::new(TemplateIrStore::new()));

        let stored_condition = Expression::bool(false, test_location(), ValueMode::ImmutableOwned);
        let overlay_location = location_at(22, 8);
        let (overlay_condition, overlay_subscription) = subscription_expression_at(
            "overlay_loop_source",
            overlay_location.clone(),
            &mut string_table,
        );

        let (template_id, condition_site_id) = {
            let mut store = store_rc.borrow_mut();
            let (template_id, loop_node_id) = {
                let mut builder = TemplateIrBuilder::new(&mut store);
                let loop_body = builder.push_sequence_node(vec![], test_location());
                let header = TemplateLoopHeader::Conditional {
                    condition: Box::new(stored_condition),
                };
                let loop_node_id = builder.push_loop_node(header, loop_body, None, test_location());
                let template_id = builder.finish_template(
                    loop_node_id,
                    Style::default(),
                    TemplateType::StringFunction,
                    TemplateIrSummary::empty(),
                    test_location(),
                );
                (template_id, loop_node_id)
            };

            let condition_site_id = match &store
                .get_node(loop_node_id)
                .expect("loop node should exist")
                .kind
            {
                TemplateIrNodeKind::Loop { header_sites, .. } => match header_sites {
                    TemplateLoopHeaderExpressionSites::Conditional { condition } => *condition,
                    other => panic!("expected conditional loop header sites, got {other:?}"),
                },
                other => panic!("expected loop node, got {other:?}"),
            };

            (template_id, condition_site_id)
        };

        let metadata = metadata_for_finalized_expression_overlay(
            &store_rc,
            template_id,
            condition_site_id,
            overlay_condition,
        );

        assert_eq!(metadata.subscriptions, vec![overlay_subscription]);
        assert_eq!(
            metadata.subscriptions[0].location, overlay_location,
            "loop header overlay source location must be preserved in reactive metadata"
        );
    }
}
