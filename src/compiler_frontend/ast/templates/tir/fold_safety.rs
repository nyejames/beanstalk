//! Shared safety gates for view-backed template folding.
//!
//! WHAT: centralizes the conservative checks used before callers fold a stable
//! `TirView` through the direct registry-backed path.
//!
//! WHY: keeping these policies in TIR avoids slightly different finalization
//! and HIR-handoff gates as individual overlay dimensions become foldable.

use crate::compiler_frontend::ast::templates::tir::ids::{
    TemplateIrId, TemplateIrNodeId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{TemplateIrNodeKind, TirSlotPlaceholder};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirWrapperContext,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::compiler_errors::CompilerError;

use std::collections::HashSet;

/// Returns `Ok(true)` when a `TirView` can be folded through the narrow
/// shortcut used by callers that still expect empty-overlay behavior.
/// Malformed same-store authority returns `Err`; valid unsupported shapes
/// return `Ok(false)`.
///
/// Wrapper-context-only overlays are accepted here because wrapper application
/// is now a production fold/handoff responsibility. Expression and slot overlays
/// keep using the broader `tir_view_is_expression_overlay_linear_fold_safe`
/// entry point so this compatibility gate does not expand unrelated surfaces.
pub(crate) fn tir_view_is_empty_overlay_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> Result<bool, CompilerError> {
    let root = view.root_ref();
    if root.store_id != store.store_id() {
        // The HIR handoff shortcut is intentionally unavailable when the child
        // belongs to a foreign store. Its owning-store path remains responsible
        // for materializing that child.
        return Ok(false);
    }

    let overlay_set = view.overlay_set()?;
    validate_overlay_set_dimensions(view.registry_ref(), view.overlay_set_id(), overlay_set)?;

    if overlay_set.is_empty() {
        let mut visiting = HashSet::new();
        return template_root_is_linear_fold_safe(view, store, root.template_id, &mut visiting);
    }

    if overlay_set.expression_overrides.is_none()
        && overlay_set.slot_resolution.is_none()
        && overlay_set.wrapper_context.is_some()
    {
        return Ok(classify_view_native_fold_safety(view, store)?.is_none());
    }

    validate_template_root_authority(view, store, root.template_id)?;
    Ok(false)
}

/// Named reason why a `TirView` was rejected by the view-native fold safety
/// gate.
///
/// WHAT: attributes each registry-backed fold fallback to a specific overlay or
/// structural shape so counter evidence can rank which shapes dominate the
/// remaining current-state materialization volume.
///
/// WHY: the generic `TirRegistryBackedFoldFallbacks` counter only shows how many
/// attempts fell back. Named reasons let the performance plan decide whether the
/// dominant blocker is an overlay dimension (potentially fixable through
/// Phase 3-5 fold shapes) or a structural tree shape (potentially needing
/// broader design work).
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub(crate) enum TirFoldFallbackReason {
    // --- Overlay-level reasons ---
    /// The overlay set carries a wrapper-context overlay with an unsupported
    /// shape such as a cross-store/non-virtual wrapper set, so the view-native
    /// fold walker cannot expand it.
    WrapperContextOverlay,

    /// An expression overlay is present but the view phase has not reached
    /// `Finalized`, so the normalized expression payload is not yet stable.
    ExpressionOverlayBelowFinalized,

    // --- Structural tree reasons (named by code shape) ---
    /// The template declares a runtime slot plan, which requires HIR/runtime
    /// lowering rather than compile-time folding.
    RuntimeSlotPlan,

    /// The template's conditional child wrapper set contains a wrapper subtree
    /// that the virtual wrapper fold path cannot handle.
    UnsafeWrapperTree,

    /// A text or dynamic-expression node carries a reactive subscription,
    /// which must stay on the structured handoff path.
    ReactiveSegment,

    /// A slot node is present but no slot-resolution overlay is active. The
    /// current TIR cannot yet distinguish a genuinely empty slot from one that
    /// needs AST/current-state insert contribution resolution.
    SlotWithoutResolution,

    /// A slot placeholder carries its own wrapper context, or a slot-resolution
    /// overlay is active on a template that declares `$children(..)` wrappers.
    /// Both shapes need wrapper-aware view folding (Phase 5).
    SlotWrapperContext,

    /// A child-template reference points to a different store, so the
    /// view-native fold walker cannot cross into it.
    CrossStoreChild,

    /// An insert-contribution node was not consumed by slot composition.
    InsertContribution,

    /// A runtime slot site node requires HIR/runtime lowering.
    RuntimeSlotSite,

    /// An aggregate-output marker appears outside an aggregate-wrapper subtree.
    AggregateOutputOutsideWrapper,

    /// The walk detected a child-template cycle, which the fold walker cannot
    /// guard against.
    ChildTemplateCycle,
}

/// Mutable context threaded through the view-native fold-safety walk.
///
/// WHAT: bundles the child-template cycle guard and the constant
///       `slot_resolution_active` flag.
///
/// WHY: the walk functions previously took `slot_resolution_active` and
///      `visiting` as separate parameters. Bundling them into one context keeps
///      the parameter list readable. The `visiting` set is owned (not borrowed)
///      so the shared wrapper-safety helpers can receive `&mut walk.visiting`
///      without double-reference issues.
struct ViewNativeWalkContext {
    visiting: HashSet<TemplateIrId>,
    slot_resolution_active: bool,
}

/// Classifies why a `TirView` cannot be folded through the view-native path,
/// returning `Ok(None)` when the view is safe and `Ok(Some(reason))` when it is
/// not. Malformed authority returns `Err` instead of a fallback reason.
///
/// WHAT: mirrors the overlay-dimension checks and structural tree walk from
///       `tir_view_is_expression_overlay_linear_fold_safe`, but returns a named
///       reason instead of a bare `bool`. The bool entrypoint delegates here so
///       all callers share one safety authority and no duplicate walk logic is
///       introduced.
///
/// WHY: finalization fold fallback attribution needs to know *which* overlay or
///      structural shape caused the fallback, not just that the view was
///      rejected. Keeping the classification in the safety module preserves
///      single ownership of the fold-safety policy.
pub(crate) fn classify_view_native_fold_safety(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> Result<Option<TirFoldFallbackReason>, CompilerError> {
    let root = view.root_ref();
    if root.store_id != store.store_id() {
        return Err(CompilerError::compiler_error(format!(
            "TIR view-native fold safety: view root {} does not belong to supplied store {}.",
            root,
            store.store_id()
        )));
    }

    let overlay_set = view.overlay_set()?;
    validate_overlay_set_dimensions(view.registry_ref(), view.overlay_set_id(), overlay_set)?;

    let has_expression_overlay = overlay_set.expression_overrides.is_some();
    let has_slot_overlay = overlay_set.slot_resolution.is_some();

    // Expression overrides require at least Finalized so the normalized
    // expression payload is stable before folding reads it. Keep the result
    // pending until after the structural walk so malformed nested authority
    // still propagates instead of being hidden by this valid fallback reason.
    let expression_overlay_below_finalized =
        has_expression_overlay && !view.phase().is_at_least(TemplateTirPhase::Finalized);

    // When no overlay dimension is active, the view behaves as an empty-overlay
    // view. The view-native fold walker handles empty-overlay views correctly
    // (every overlay lookup returns `None`, so it falls back to structural
    // reads). Use the view-native fold-safety check so empty-overlay views with
    // branches, loops, and child templates can borrow the live store instead of
    // falling back to current-state materialization.
    let slot_resolution_active = has_slot_overlay;

    let mut walk = ViewNativeWalkContext {
        visiting: HashSet::new(),
        slot_resolution_active,
    };

    let reason = check_template_root_view_native_overlay_fold_safety(
        store,
        view,
        root.template_id,
        false,
        &mut walk,
    )?;

    if expression_overlay_below_finalized {
        Ok(Some(TirFoldFallbackReason::ExpressionOverlayBelowFinalized))
    } else {
        Ok(reason)
    }
}

/// Returns `Ok(true)` when a `TirView` can be folded through the view-native
/// path without cloning or mutating the module `TemplateIrStore`. Malformed
/// authority returns `Err`; valid unsupported shapes return `Ok(false)`.
///
/// WHAT: delegates to `classify_view_native_fold_safety` so the bool and
///       reason-returning callers share one safety authority.
///
/// WHY: the fold walker consults `TirView` for effective expressions and
///      slot resolutions instead of cloning and mutating the store. This safety
///      gate is the conservative boundary between shapes that can borrow the
///      live store and shapes that still need the current-state fallback.
#[cfg(test)]
pub(crate) fn tir_view_is_expression_overlay_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> Result<bool, CompilerError> {
    Ok(classify_view_native_fold_safety(view, store)?.is_none())
}

fn template_root_is_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    template_id: TemplateIrId,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    if !visiting.insert(template_id) {
        return Ok(false);
    }

    validate_template_root_authority(view, store, template_id)?;
    let template_ir = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(store, template_id))?;

    let is_linear = tir_node_is_linear_fold_safe(view, store, template_ir.root, visiting);
    visiting.remove(&template_id);
    is_linear
}

fn tir_node_is_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(store, node_id))?;

    let is_linear = match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut is_linear = true;
            for child in children {
                is_linear &= tir_node_is_linear_fold_safe(view, store, *child, visiting)?;
            }
            is_linear
        }

        // Reactive segments must stay on the structured handoff path until
        // folded output can carry equivalent subscription metadata.
        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(node_id).is_none(),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => reactive_subscription.is_none(),

        TemplateIrNodeKind::AggregateOutput | TemplateIrNodeKind::LoopControl { .. } => false,

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                tir_node_is_linear_fold_safe(view, store, branch.body, visiting)?;
            }
            if let Some(fallback) = fallback {
                tir_node_is_linear_fold_safe(view, store, *fallback, visiting)?;
            }
            false
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            tir_node_is_linear_fold_safe(view, store, *body, visiting)?;
            if let Some(aggregate_wrapper) = aggregate_wrapper {
                tir_node_is_linear_fold_safe(view, store, *aggregate_wrapper, visiting)?;
            }
            false
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            if let Some(child_template_id) = reference.template_id_in_store(store.store_id()) {
                validate_overlay_set_authority(view.registry_ref(), reference.overlay_set_id)?;
                template_root_is_linear_fold_safe(view, store, child_template_id, visiting)?;
            }
            false
        }

        TemplateIrNodeKind::InsertContribution { template } => {
            template_root_is_linear_fold_safe(view, store, *template, visiting)?;
            false
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            validate_slot_placeholder_wrapper_authority(view, store, placeholder)?;
            false
        }

        TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    };

    Ok(is_linear)
}

/// Checks one template root for view-native overlay fold safety, returning the
/// first named rejection reason or `None` when the root is safe.
///
/// WHAT: mirrors `template_root_is_read_only_fold_safe` but allows expression
///       and slot overlays on the root view. Child templates must have empty
///       overlay sets so `fold_child_template_reference` can fall back to the
///       store-local `fold_tir_template` path without registry re-borrow.
///
/// The walk context carries the cycle guard (`visiting`) and the constant
/// `slot_resolution_active` flag.
fn check_template_root_view_native_overlay_fold_safety(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    walk: &mut ViewNativeWalkContext,
) -> Result<Option<TirFoldFallbackReason>, CompilerError> {
    // A template already on the visiting stack would recurse indefinitely in
    // the fold walker, which has no child-template cycle guard.
    if !walk.visiting.insert(template_id) {
        return Ok(Some(TirFoldFallbackReason::ChildTemplateCycle));
    }

    validate_template_root_authority(view, store, template_id)?;
    let template = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(store, template_id))?;

    // Runtime slot plans require HIR/runtime lowering, not compile-time folding.
    if template.runtime_slot_plan.is_some() {
        return Ok(Some(TirFoldFallbackReason::RuntimeSlotPlan));
    }

    // Conditional child wrappers are folded through a virtual wrapper path that
    // does not push synthetic nodes into the store. Keep the gate matched to
    // the shapes that path can fold so fallback handles still-unsupported
    // wrapper subtrees while malformed authority propagates as an error.
    if let Some(wrapper_set_id) = template.conditional_child_wrapper_set
        && !wrapper_set_is_virtual_fold_safe(store, view, wrapper_set_id, &mut walk.visiting)?
    {
        return Ok(Some(TirFoldFallbackReason::UnsafeWrapperTree));
    }

    // Slot overlays on templates that declare `$children(..)` wrappers need
    // wrapper context while folding resolved slot sources. Keep these shapes on
    // the current-state fallback until slot-source folding can expand wrapper
    // context locally.
    if walk.slot_resolution_active && template.summary.wrapper_count > 0 {
        return Ok(Some(TirFoldFallbackReason::SlotWrapperContext));
    }

    let result = check_tir_node_view_native_overlay_fold_safety(
        store,
        view,
        template.root,
        in_aggregate_wrapper,
        walk,
    );
    walk.visiting.remove(&template_id);
    result
}

/// Checks one TIR node subtree for view-native overlay fold safety, returning
/// the first named rejection reason or `None` when the subtree is safe.
///
/// WHAT: the view-native fold walker reads effective expressions and slot
///       resolutions from `TirView` during folding, so branches, loops, and
///       child templates are safe as long as they don't require store mutation.
///       Reactive segments, insert contributions, runtime slot sites, and
///       aggregate-output markers outside wrapper context are rejected.
///
/// The walk context carries the constant `slot_resolution_active` flag and
/// the cycle guard.
fn check_tir_node_view_native_overlay_fold_safety(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    in_aggregate_wrapper: bool,
    walk: &mut ViewNativeWalkContext,
) -> Result<Option<TirFoldFallbackReason>, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(store, node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            for child in children {
                if let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    *child,
                    in_aggregate_wrapper,
                    walk,
                )? {
                    return Ok(Some(reason));
                }
            }
            Ok(None)
        }

        // Reactive segments must stay on the structured handoff path.
        TemplateIrNodeKind::Text { .. } => {
            if store.node_reactive_subscription(node_id).is_some() {
                Ok(Some(TirFoldFallbackReason::ReactiveSegment))
            } else {
                Ok(None)
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => {
            if reactive_subscription.is_some() {
                Ok(Some(TirFoldFallbackReason::ReactiveSegment))
            } else {
                Ok(None)
            }
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            validate_slot_placeholder_wrapper_authority(view, store, placeholder)?;

            if !walk.slot_resolution_active {
                return Ok(Some(TirFoldFallbackReason::SlotWithoutResolution));
            }

            // Slot-local wrapper context changes how resolved sources are
            // rendered. Keep these shapes on the current-state fallback until
            // slot-source folding can expand wrapper context locally.
            if slot_placeholder_has_wrapper_context(placeholder) {
                Ok(Some(TirFoldFallbackReason::SlotWrapperContext))
            } else {
                Ok(None)
            }
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            for branch in branches {
                if let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    branch.body,
                    in_aggregate_wrapper,
                    walk,
                )? {
                    return Ok(Some(reason));
                }
            }
            if let Some(fallback_id) = fallback
                && let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    *fallback_id,
                    in_aggregate_wrapper,
                    walk,
                )?
            {
                return Ok(Some(reason));
            }
            Ok(None)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if let Some(reason) =
                check_tir_node_view_native_overlay_fold_safety(store, view, *body, false, walk)?
            {
                return Ok(Some(reason));
            }
            if let Some(wrapper_id) = aggregate_wrapper
                && let Some(reason) = check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    *wrapper_id,
                    true,
                    walk,
                )?
            {
                return Ok(Some(reason));
            }
            Ok(None)
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
            ..
        } => {
            // Only same-store child references can be folded.
            let Some(child_template_id) = reference.template_id_in_store(store.store_id()) else {
                return Ok(Some(TirFoldFallbackReason::CrossStoreChild));
            };
            validate_template_root_authority(view, store, child_template_id)?;

            // Wrapper-context overlays apply inherited `$children(..)` wrappers
            // at the child occurrence boundary. They are safe when the wrappers
            // are virtual-fold-safe, same-store, and carry no unsupported modes.
            // This check uses the parent view because wrapper context is
            // inherited from the parent, not from the child's own overlay set.
            let effective_wrapper_context = view.effective_wrapper_context(*occurrence_id)?;
            if let Some(context) = effective_wrapper_context
                && !wrapper_context_is_view_native_fold_safe(store, view, context)?
            {
                return Ok(Some(TirFoldFallbackReason::WrapperContextOverlay));
            }

            // Check the child's overlay set. Empty-overlay children recurse
            // with the parent view as before. Non-empty-overlay children get
            // a child view so expression overrides and slot resolution within
            // the child's subtree are visible during the safety walk.
            let child_overlay_set = view
                .registry_ref()
                .overlay_set(reference.overlay_set_id)
                .ok_or_else(|| missing_overlay_set_error(reference.overlay_set_id))?;
            validate_overlay_set_dimensions(
                view.registry_ref(),
                reference.overlay_set_id,
                child_overlay_set,
            )?;
            let child_overlay_is_empty = child_overlay_set.is_empty();

            if child_overlay_is_empty {
                check_template_root_view_native_overlay_fold_safety(
                    store,
                    view,
                    child_template_id,
                    in_aggregate_wrapper,
                    walk,
                )
            } else {
                let child_view =
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?;
                // Update slot_resolution_active for the child's subtree so
                // Slot nodes inside the child are checked against the child's
                // slot-resolution overlay, not the parent's.
                let child_has_slot_resolution = child_overlay_set.slot_resolution.is_some();
                let saved_slot_resolution_active = walk.slot_resolution_active;
                walk.slot_resolution_active = child_has_slot_resolution;
                let result = check_template_root_view_native_overlay_fold_safety(
                    store,
                    &child_view,
                    child_template_id,
                    in_aggregate_wrapper,
                    walk,
                );
                walk.slot_resolution_active = saved_slot_resolution_active;
                result
            }
        }

        // Loop-control signals are safe: the fold walker just returns them.
        TemplateIrNodeKind::LoopControl { .. } => Ok(None),

        // AggregateOutput markers are valid only inside aggregate wrapper
        // subtrees, where the wrapper fold path consumes them.
        TemplateIrNodeKind::AggregateOutput => {
            if in_aggregate_wrapper {
                Ok(None)
            } else {
                Ok(Some(TirFoldFallbackReason::AggregateOutputOutsideWrapper))
            }
        }

        // Insert contributions should have been consumed by slot composition.
        // Validate the referenced local helper before preserving the ordinary
        // insert-contribution fallback for a well-formed store.
        TemplateIrNodeKind::InsertContribution { template } => {
            validate_template_root_authority(view, store, *template)?;
            Ok(Some(TirFoldFallbackReason::InsertContribution))
        }

        // Runtime slot sites require HIR/runtime lowering.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => {
            Ok(Some(TirFoldFallbackReason::RuntimeSlotSite))
        }
    }
}

fn slot_placeholder_has_wrapper_context(placeholder: &TirSlotPlaceholder) -> bool {
    placeholder.applied_child_wrapper_set.is_some()
        || placeholder.child_wrapper_set.is_some()
        || placeholder.skip_parent_child_wrappers
}

fn validate_overlay_set_dimensions(
    registry: &TemplateIrRegistry,
    overlay_set_id: TemplateOverlaySetId,
    overlay_set: &TemplateOverlaySet,
) -> Result<(), CompilerError> {
    if let Some(overlay_id) = overlay_set.expression_overrides
        && registry.expression_overlay(overlay_id).is_none()
    {
        return Err(missing_overlay_dimension_error(
            overlay_set_id,
            "expression",
            overlay_id,
        ));
    }

    if let Some(overlay_id) = overlay_set.slot_resolution
        && registry.slot_resolution_overlay(overlay_id).is_none()
    {
        return Err(missing_overlay_dimension_error(
            overlay_set_id,
            "slot-resolution",
            overlay_id,
        ));
    }

    if let Some(overlay_id) = overlay_set.wrapper_context
        && registry.wrapper_context_overlay(overlay_id).is_none()
    {
        return Err(missing_overlay_dimension_error(
            overlay_set_id,
            "wrapper-context",
            overlay_id,
        ));
    }

    Ok(())
}

fn validate_overlay_set_authority(
    registry: &TemplateIrRegistry,
    overlay_set_id: TemplateOverlaySetId,
) -> Result<(), CompilerError> {
    let overlay_set = registry
        .overlay_set(overlay_set_id)
        .ok_or_else(|| missing_overlay_set_error(overlay_set_id))?;
    validate_overlay_set_dimensions(registry, overlay_set_id, overlay_set)
}

fn validate_template_root_authority(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> Result<(), CompilerError> {
    let template = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(store, template_id))?;

    if store.get_node(template.root).is_none() {
        return Err(missing_node_error(store, template.root));
    }

    if let Some(wrapper_set_id) = template.conditional_child_wrapper_set {
        validate_wrapper_set_authority(store, view, wrapper_set_id)?;
    }

    Ok(())
}

fn validate_wrapper_set_authority(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    wrapper_set_id: TemplateWrapperSetId,
) -> Result<(), CompilerError> {
    let wrapper_set = store
        .get_wrapper_set(wrapper_set_id)
        .ok_or_else(|| missing_wrapper_set_error(store, wrapper_set_id))?;

    for wrapper in &wrapper_set.wrappers {
        if wrapper.root.store_id != store.store_id() {
            // Cross-store wrappers are a valid unsupported fold shape. Their
            // owning-store path will resolve them during structural handoff.
            continue;
        }

        let wrapper_template = store
            .get_template(wrapper.root.template_id)
            .ok_or_else(|| missing_template_error(store, wrapper.root.template_id))?;
        if store.get_node(wrapper_template.root).is_none() {
            return Err(missing_node_error(store, wrapper_template.root));
        }

        let overlay_set = view
            .registry_ref()
            .overlay_set(wrapper.overlay_set_id)
            .ok_or_else(|| missing_overlay_set_error(wrapper.overlay_set_id))?;
        validate_overlay_set_dimensions(view.registry_ref(), wrapper.overlay_set_id, overlay_set)?;
    }

    Ok(())
}

fn validate_slot_placeholder_wrapper_authority(
    view: &TirView<'_>,
    store: &TemplateIrStore,
    placeholder: &TirSlotPlaceholder,
) -> Result<(), CompilerError> {
    for wrapper_set_id in [
        placeholder.applied_child_wrapper_set,
        placeholder.child_wrapper_set,
    ]
    .into_iter()
    .flatten()
    {
        validate_wrapper_set_authority(store, view, wrapper_set_id)?;
    }

    Ok(())
}

fn missing_template_error(store: &TemplateIrStore, template_id: TemplateIrId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: template {} is not present in store {}.",
        template_id,
        store.store_id()
    ))
}

fn missing_node_error(store: &TemplateIrStore, node_id: TemplateIrNodeId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: node {} is not present in store {}.",
        node_id,
        store.store_id()
    ))
}

fn missing_wrapper_set_error(
    store: &TemplateIrStore,
    wrapper_set_id: TemplateWrapperSetId,
) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: wrapper set {} is not present in store {}.",
        wrapper_set_id,
        store.store_id()
    ))
}

fn missing_overlay_set_error(overlay_set_id: TemplateOverlaySetId) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: overlay set {} is not present in the registry.",
        overlay_set_id
    ))
}

fn missing_overlay_dimension_error(
    overlay_set_id: TemplateOverlaySetId,
    dimension: &str,
    overlay_id: impl std::fmt::Display,
) -> CompilerError {
    CompilerError::compiler_error(format!(
        "TIR fold safety: overlay set {} references missing {} overlay {}.",
        overlay_set_id, dimension, overlay_id
    ))
}

// -------------------------
//  Read-only (snapshot-free) fold safety
// -------------------------

/// Returns `Ok(true)` when a `TirView` root can be folded without cloning or
/// mutating the module `TemplateIrStore`. Malformed same-store authority
/// returns `Err`; valid unsupported shapes return `Ok(false)`.
///
/// WHAT: walks the structural TIR tree — crossing same-store child-template
///       boundaries — and verifies that no reachable template or node requires
///       store mutation during folding. Runtime slot plans and runtime slot sites
///       are rejected because their output must be resolved during HIR lowering,
///       not compile-time folding.
///
/// WHY: Phase 3A of the TIR performance plan removes the full-store clone for
///      the common empty-overlay case. This safety gate is the conservative
///      boundary between shapes that can borrow the live store and shapes that
///      still need a cloned workspace.
pub(crate) fn tir_view_is_read_only_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> Result<bool, CompilerError> {
    let root = view.root_ref();
    if root.store_id != store.store_id() {
        return Err(CompilerError::compiler_error(format!(
            "TIR read-only fold safety: view root {} does not belong to supplied store {}.",
            root,
            store.store_id()
        )));
    }

    // Only empty-overlay views are safe for the read-only path in Phase 3A.
    // Expression, slot, and wrapper-context overlays require applying
    // effective data to a cloned store, which belongs to Phase 4 and later.
    let overlay_set = view.overlay_set()?;
    validate_overlay_set_dimensions(view.registry_ref(), view.overlay_set_id(), overlay_set)?;
    if !overlay_set.is_empty() {
        let _fallback_reason = classify_view_native_fold_safety(view, store)?;
        return Ok(false);
    }

    let mut visiting = HashSet::new();
    template_root_is_read_only_fold_safe(store, view, root.template_id, false, &mut visiting)
}

/// Checks one template root for read-only fold safety, crossing into same-store
/// child templates recursively.
fn template_root_is_read_only_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    // A template already on the visiting stack would recurse indefinitely in
    // the fold walker, which has no child-template cycle guard.
    if !visiting.insert(template_id) {
        return Ok(false);
    }

    validate_template_root_authority(view, store, template_id)?;
    let template = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(store, template_id))?;

    // Runtime slot plans require HIR/runtime lowering, not compile-time folding.
    if template.runtime_slot_plan.is_some() {
        return Ok(false);
    }

    // Conditional child wrappers are folded through a virtual wrapper path that
    // does not push synthetic nodes into the store. Keep the gate matched to
    // the shapes that path can fold so fallback handles still-unsupported
    // wrapper subtrees while malformed authority propagates as an error.
    if let Some(wrapper_set_id) = template.conditional_child_wrapper_set
        && !wrapper_set_is_virtual_fold_safe(store, view, wrapper_set_id, visiting)?
    {
        return Ok(false);
    }

    let safe =
        tir_node_is_read_only_fold_safe(store, view, template.root, in_aggregate_wrapper, visiting);
    visiting.remove(&template_id);
    safe
}

/// Checks one TIR node subtree for read-only fold safety.
fn tir_node_is_read_only_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(store, node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut safe = true;
            for child in children {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    *child,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            Ok(safe)
        }

        // Reactive segments must stay on the structured handoff path.
        TemplateIrNodeKind::Text { .. } => Ok(store.node_reactive_subscription(node_id).is_none()),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => Ok(reactive_subscription.is_none()),

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            // Only same-store child references can be folded read-only.
            let Some(child_template_id) = reference.template_id_in_store(store.store_id()) else {
                return Ok(false);
            };
            validate_template_root_authority(view, store, child_template_id)?;

            // The child's overlay set must also be empty. Look it up through
            // the registry that backs the parent view.
            let child_overlay_safe = view
                .registry_ref()
                .overlay_set(reference.overlay_set_id)
                .ok_or_else(|| missing_overlay_set_error(reference.overlay_set_id))?;
            validate_overlay_set_dimensions(
                view.registry_ref(),
                reference.overlay_set_id,
                child_overlay_safe,
            )?;

            if !child_overlay_safe.is_empty() {
                let child_view =
                    view.child_view(reference.root, reference.phase, reference.overlay_set_id)?;
                let _fallback_reason = classify_view_native_fold_safety(&child_view, store)?;
                return Ok(false);
            }

            template_root_is_read_only_fold_safe(
                store,
                view,
                child_template_id,
                in_aggregate_wrapper,
                visiting,
            )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let mut safe = true;
            for branch in branches {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    branch.body,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            if let Some(fallback_id) = fallback {
                safe &= tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    *fallback_id,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }

            Ok(safe)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let mut safe = tir_node_is_read_only_fold_safe(store, view, *body, false, visiting)?;
            if let Some(wrapper_id) = aggregate_wrapper {
                safe &= tir_node_is_read_only_fold_safe(store, view, *wrapper_id, true, visiting)?;
            }
            Ok(safe)
        }

        // Loop-control signals are safe: the fold walker just returns them.
        TemplateIrNodeKind::LoopControl { .. } => Ok(true),

        // Empty-overlay slots are rejected because read-only folding cannot
        // tell whether the slot is genuinely empty or still needs insert
        // contribution resolution. The view-native path only accepts slots
        // with an explicit slot-resolution overlay.
        TemplateIrNodeKind::Slot { placeholder } => {
            validate_slot_placeholder_wrapper_authority(view, store, placeholder)?;
            Ok(false)
        }

        // AggregateOutput markers are valid only inside aggregate wrapper
        // subtrees, where the wrapper fold path consumes them.
        TemplateIrNodeKind::AggregateOutput => Ok(in_aggregate_wrapper),

        // Insert contributions should have been consumed by slot composition.
        TemplateIrNodeKind::InsertContribution { template } => {
            template_root_is_read_only_fold_safe(store, view, *template, false, visiting)?;
            Ok(false)
        }

        // Runtime slot sites require HIR/runtime lowering.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}

/// Returns `Ok(true)` when a wrapper-context overlay entry can be folded around
/// a child-template emission without falling back to current-state
/// materialization. Malformed authority returns `Err`; valid unsupported
/// shapes return `Ok(false)`.
/// WHAT: validates the same-store wrapper set referenced by the overlay and
///       walks each wrapper template through the node shapes that the virtual
///       wrapper fold path supports. `IfChildEmits` is safe because the fold
///       helper receives the already-folded child emission and can decide from
///       that structural result. Cross-store wrapper sets remain rejected
///       because the view-native fold cannot reach them.
/// WHY: wrapper-context overlays are now a supported overlay dimension, but
///      only same-store virtual wrapper trees are foldable today. This gate
///      keeps unsupported cross-store or non-virtual shapes on the current-state
///      fallback.
fn wrapper_context_is_view_native_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    context: &TirWrapperContext,
) -> Result<bool, CompilerError> {
    // `$fresh` suppresses parent-applied wrappers, so there is nothing to fold.
    if context.skip_parent_child_wrappers {
        return Ok(true);
    }

    let Some(wrapper_set_ref) = context.inherited_wrapper_set else {
        return Ok(true);
    };

    if wrapper_set_ref.store_id != store.store_id() {
        return Ok(false);
    }

    // Wrapper templates are checked independently of the outer template walk so
    // that wrapper-set membership does not create false cycle reports against
    // the parent/child recursion guard.
    let mut visiting = HashSet::new();
    wrapper_set_is_virtual_fold_safe(store, view, wrapper_set_ref.wrapper_set_id, &mut visiting)
}

/// Returns `Ok(true)` when every wrapper in the set can be folded by the
/// virtual conditional-wrapper path. Malformed authority returns `Err`; valid
/// unsupported shapes return `Ok(false)`.
///
/// WHAT: checks same-store identity first, then walks each wrapper template
///       through the exact node shapes supported by
///       `fold_tir_wrapper_around_child_output`.
/// WHY: same-store identity alone is not enough. Slot placeholders with their
///      own wrapper context and slots inside loop aggregate-wrapper subtrees
///      still need the current-state fallback because the virtual wrapper fold
///      does not expand those contexts.
fn wrapper_set_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    wrapper_set_id: TemplateWrapperSetId,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    let wrapper_set = store
        .get_wrapper_set(wrapper_set_id)
        .ok_or_else(|| missing_wrapper_set_error(store, wrapper_set_id))?;
    validate_wrapper_set_authority(store, view, wrapper_set_id)?;

    let mut safe = true;
    for wrapper in &wrapper_set.wrappers {
        if wrapper.root.store_id != store.store_id() {
            safe = false;
            continue;
        }

        safe &= wrapper_template_is_virtual_fold_safe(
            store,
            view,
            wrapper.root.template_id,
            false,
            visiting,
        )?;
    }

    Ok(safe)
}

fn wrapper_template_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    if !visiting.insert(template_id) {
        return Ok(false);
    }

    validate_template_root_authority(view, store, template_id)?;
    let template = store
        .get_template(template_id)
        .ok_or_else(|| missing_template_error(store, template_id))?;
    let safe = if template.runtime_slot_plan.is_none() {
        wrapper_node_is_virtual_fold_safe(
            store,
            view,
            template.root,
            in_aggregate_wrapper,
            visiting,
        )?
    } else {
        false
    };

    visiting.remove(&template_id);
    Ok(safe)
}

fn wrapper_node_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> Result<bool, CompilerError> {
    let node = store
        .get_node(node_id)
        .ok_or_else(|| missing_node_error(store, node_id))?;

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => {
            let mut safe = true;
            for child in children {
                safe &= wrapper_node_is_virtual_fold_safe(
                    store,
                    view,
                    *child,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            Ok(safe)
        }

        TemplateIrNodeKind::Text { .. } => Ok(store.node_reactive_subscription(node_id).is_none()),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => Ok(reactive_subscription.is_none()),

        TemplateIrNodeKind::Slot { placeholder } => {
            validate_slot_placeholder_wrapper_authority(view, store, placeholder)?;
            Ok(!in_aggregate_wrapper && !slot_placeholder_has_wrapper_context(placeholder))
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let Some(child_template_id) = reference.template_id_in_store(store.store_id()) else {
                return Ok(false);
            };
            validate_template_root_authority(view, store, child_template_id)?;

            let child_overlay_safe = view
                .registry_ref()
                .overlay_set(reference.overlay_set_id)
                .ok_or_else(|| missing_overlay_set_error(reference.overlay_set_id))?;
            validate_overlay_set_dimensions(
                view.registry_ref(),
                reference.overlay_set_id,
                child_overlay_safe,
            )?;

            if !child_overlay_safe.is_empty() {
                return Ok(false);
            }

            wrapper_template_is_virtual_fold_safe(
                store,
                view,
                child_template_id,
                in_aggregate_wrapper,
                visiting,
            )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            if in_aggregate_wrapper {
                return Ok(false);
            }

            let mut safe = true;
            for branch in branches {
                safe &= wrapper_node_is_virtual_fold_safe(
                    store,
                    view,
                    branch.body,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            if let Some(fallback_id) = fallback {
                safe &= wrapper_node_is_virtual_fold_safe(
                    store,
                    view,
                    *fallback_id,
                    in_aggregate_wrapper,
                    visiting,
                )?;
            }
            Ok(safe)
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            if in_aggregate_wrapper {
                return Ok(false);
            }

            let mut safe = wrapper_node_is_virtual_fold_safe(store, view, *body, false, visiting)?;
            if let Some(wrapper_id) = aggregate_wrapper {
                safe &=
                    wrapper_node_is_virtual_fold_safe(store, view, *wrapper_id, true, visiting)?;
            }
            Ok(safe)
        }

        TemplateIrNodeKind::AggregateOutput => Ok(in_aggregate_wrapper),

        TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => Ok(false),
    }
}
