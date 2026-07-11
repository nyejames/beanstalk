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
    TemplateOverlaySet, TirWrapperContext,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};

use std::collections::HashSet;

/// Returns true when a `TirView` can be folded through the narrow shortcut used
/// by callers that still expect empty-overlay behavior.
///
/// Wrapper-context-only overlays are accepted here because wrapper application
/// is now a production fold/handoff responsibility. Expression and slot overlays
/// keep using the broader `tir_view_is_expression_overlay_linear_fold_safe`
/// entry point so this compatibility gate does not expand unrelated surfaces.
pub(crate) fn tir_view_is_empty_overlay_linear_fold_safe(
    view: &TirView<'_>,
    store: &TemplateIrStore,
) -> bool {
    let root = view.root_ref();
    if root.store_id != store.store_id() {
        return false;
    }

    let overlay_set = match view.overlay_set() {
        Ok(overlay_set) => overlay_set,
        Err(_) => return false,
    };

    if overlay_set.is_empty() {
        return template_root_is_linear_fold_safe(store, root.template_id);
    }

    if overlay_set.expression_overrides.is_none()
        && overlay_set.slot_resolution.is_none()
        && overlay_set.wrapper_context.is_some()
    {
        return classify_view_native_fold_safety(view, store).is_none();
    }

    false
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

    /// A slot-resolution overlay ID is set but the overlay entry does not exist
    /// in the registry, so the fold cannot apply the resolution.
    DanglingSlotOverlay,

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

    /// A template or node referenced by the walk was not found in the store.
    MissingTemplateOrNode,

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
/// returning `None` when the view is safe and `Some(reason)` when it is not.
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
) -> Option<TirFoldFallbackReason> {
    let root = view.root_ref();
    if root.store_id != store.store_id() {
        return Some(TirFoldFallbackReason::MissingTemplateOrNode);
    }

    let overlay_set = match view.overlay_set() {
        Ok(overlay_set) => overlay_set,
        Err(_) => return Some(TirFoldFallbackReason::MissingTemplateOrNode),
    };

    let has_expression_overlay = overlay_set.expression_overrides.is_some();
    let has_slot_overlay = overlay_set.slot_resolution.is_some();

    // Expression overrides require at least Finalized so the normalized
    // expression payload is stable before folding reads it.
    if has_expression_overlay && !view.phase().is_at_least(TemplateTirPhase::Finalized) {
        return Some(TirFoldFallbackReason::ExpressionOverlayBelowFinalized);
    }

    // When no overlay dimension is active, the view behaves as an empty-overlay
    // view. The view-native fold walker handles empty-overlay views correctly
    // (every overlay lookup returns `None`, so it falls back to structural
    // reads). Use the view-native fold-safety check so empty-overlay views with
    // branches, loops, and child templates can borrow the live store instead of
    // falling back to current-state materialization.
    let slot_resolution_active = has_slot_overlay;

    if has_slot_overlay && view.slot_resolution_overlay().ok().flatten().is_none() {
        return Some(TirFoldFallbackReason::DanglingSlotOverlay);
    }

    let mut walk = ViewNativeWalkContext {
        visiting: HashSet::new(),
        slot_resolution_active,
    };

    check_template_root_view_native_overlay_fold_safety(
        store,
        view,
        root.template_id,
        false,
        &mut walk,
    )
}

/// Returns true when a `TirView` can be folded through the view-native path
/// without cloning or mutating the module `TemplateIrStore`.
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
) -> bool {
    classify_view_native_fold_safety(view, store).is_none()
}

fn template_root_is_linear_fold_safe(store: &TemplateIrStore, template_id: TemplateIrId) -> bool {
    let Some(template_ir) = store.get_template(template_id) else {
        return false;
    };

    tir_node_is_linear_fold_safe(store, template_ir.root)
}

fn tir_node_is_linear_fold_safe(store: &TemplateIrStore, node_id: TemplateIrNodeId) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children
            .iter()
            .all(|child| tir_node_is_linear_fold_safe(store, *child)),

        // Reactive segments must stay on the structured handoff path until
        // folded output can carry equivalent subscription metadata.
        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(node_id).is_none(),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => reactive_subscription.is_none(),

        TemplateIrNodeKind::AggregateOutput
        | TemplateIrNodeKind::BranchChain { .. }
        | TemplateIrNodeKind::ChildTemplate { .. }
        | TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::Loop { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. }
        | TemplateIrNodeKind::Slot { .. } => false,
    }
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
) -> Option<TirFoldFallbackReason> {
    // A template already on the visiting stack would recurse indefinitely in
    // the fold walker, which has no child-template cycle guard.
    if !walk.visiting.insert(template_id) {
        return Some(TirFoldFallbackReason::ChildTemplateCycle);
    }

    let Some(template) = store.get_template(template_id) else {
        return Some(TirFoldFallbackReason::MissingTemplateOrNode);
    };

    // Runtime slot plans require HIR/runtime lowering, not compile-time folding.
    if template.runtime_slot_plan.is_some() {
        return Some(TirFoldFallbackReason::RuntimeSlotPlan);
    }

    // Conditional child wrappers are folded through a virtual wrapper path that
    // does not push synthetic nodes into the store. Keep the gate matched to
    // the shapes that path can fold so fallback handles malformed or still
    // unsupported wrapper subtrees.
    if let Some(wrapper_set_id) = template.conditional_child_wrapper_set
        && !wrapper_set_is_virtual_fold_safe(store, view, wrapper_set_id, &mut walk.visiting)
    {
        return Some(TirFoldFallbackReason::UnsafeWrapperTree);
    }

    // Slot overlays on templates that declare `$children(..)` wrappers need
    // wrapper context while folding resolved slot sources. Keep these shapes on
    // the current-state fallback until slot-source folding can expand wrapper
    // context locally.
    if walk.slot_resolution_active && template.summary.wrapper_count > 0 {
        return Some(TirFoldFallbackReason::SlotWrapperContext);
    }

    let reason = check_tir_node_view_native_overlay_fold_safety(
        store,
        view,
        template.root,
        in_aggregate_wrapper,
        walk,
    );
    walk.visiting.remove(&template_id);
    reason
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
) -> Option<TirFoldFallbackReason> {
    let Some(node) = store.get_node(node_id) else {
        return Some(TirFoldFallbackReason::MissingTemplateOrNode);
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children.iter().find_map(|child| {
            check_tir_node_view_native_overlay_fold_safety(
                store,
                view,
                *child,
                in_aggregate_wrapper,
                walk,
            )
        }),

        // Reactive segments must stay on the structured handoff path.
        TemplateIrNodeKind::Text { .. } => {
            if store.node_reactive_subscription(node_id).is_some() {
                Some(TirFoldFallbackReason::ReactiveSegment)
            } else {
                None
            }
        }

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => {
            if reactive_subscription.is_some() {
                Some(TirFoldFallbackReason::ReactiveSegment)
            } else {
                None
            }
        }

        TemplateIrNodeKind::Slot { placeholder } => {
            if !walk.slot_resolution_active {
                return Some(TirFoldFallbackReason::SlotWithoutResolution);
            }

            // Slot-local wrapper context changes how resolved sources are
            // rendered. Keep these shapes on the current-state fallback until
            // slot-source folding can expand wrapper context locally.
            if slot_placeholder_has_wrapper_context(placeholder) {
                Some(TirFoldFallbackReason::SlotWrapperContext)
            } else {
                None
            }
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            let branch_reason = branches.iter().find_map(|branch| {
                check_tir_node_view_native_overlay_fold_safety(
                    store,
                    view,
                    branch.body,
                    in_aggregate_wrapper,
                    walk,
                )
            });

            branch_reason.or_else(|| {
                fallback.and_then(|fallback_id| {
                    check_tir_node_view_native_overlay_fold_safety(
                        store,
                        view,
                        fallback_id,
                        in_aggregate_wrapper,
                        walk,
                    )
                })
            })
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let body_reason =
                check_tir_node_view_native_overlay_fold_safety(store, view, *body, false, walk);

            body_reason.or_else(|| {
                aggregate_wrapper.and_then(|wrapper_id| {
                    check_tir_node_view_native_overlay_fold_safety(
                        store, view, wrapper_id, true, walk,
                    )
                })
            })
        }

        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
            ..
        } => {
            // Only same-store child references can be folded.
            let Some(child_template_id) = reference.template_id_in_store(store.store_id()) else {
                return Some(TirFoldFallbackReason::CrossStoreChild);
            };

            // Wrapper-context overlays apply inherited `$children(..)` wrappers
            // at the child occurrence boundary. They are safe when the wrappers
            // are virtual-fold-safe, same-store, and carry no unsupported modes.
            // This check uses the parent view because wrapper context is
            // inherited from the parent, not from the child's own overlay set.
            let effective_wrapper_context = match view.effective_wrapper_context(*occurrence_id) {
                Ok(context) => context,
                Err(_) => return Some(TirFoldFallbackReason::MissingTemplateOrNode),
            };
            if let Some(context) = effective_wrapper_context
                && !wrapper_context_is_view_native_fold_safe(store, view, context)
            {
                return Some(TirFoldFallbackReason::WrapperContextOverlay);
            }

            // Check the child's overlay set. Empty-overlay children recurse
            // with the parent view as before. Non-empty-overlay children get
            // a child view so expression overrides and slot resolution within
            // the child's subtree are visible during the safety walk.
            let child_overlay_set = view.registry_ref().overlay_set(reference.overlay_set_id);
            let child_overlay_is_empty =
                child_overlay_set.is_some_and(TemplateOverlaySet::is_empty);

            if child_overlay_is_empty {
                check_template_root_view_native_overlay_fold_safety(
                    store,
                    view,
                    child_template_id,
                    in_aggregate_wrapper,
                    walk,
                )
            } else {
                let child_view = match view.child_view(
                    reference.root,
                    reference.phase,
                    reference.overlay_set_id,
                ) {
                    Ok(child_view) => child_view,
                    Err(_) => return Some(TirFoldFallbackReason::MissingTemplateOrNode),
                };
                // Update slot_resolution_active for the child's subtree so
                // Slot nodes inside the child are checked against the child's
                // slot-resolution overlay, not the parent's.
                let child_has_slot_resolution =
                    child_overlay_set.is_some_and(|set| set.slot_resolution.is_some());
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
        TemplateIrNodeKind::LoopControl { .. } => None,

        // AggregateOutput markers are valid only inside aggregate wrapper
        // subtrees, where the wrapper fold path consumes them.
        TemplateIrNodeKind::AggregateOutput => {
            if in_aggregate_wrapper {
                None
            } else {
                Some(TirFoldFallbackReason::AggregateOutputOutsideWrapper)
            }
        }

        // Insert contributions should have been consumed by slot composition.
        TemplateIrNodeKind::InsertContribution { .. } => {
            Some(TirFoldFallbackReason::InsertContribution)
        }

        // Runtime slot sites require HIR/runtime lowering.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => Some(TirFoldFallbackReason::RuntimeSlotSite),
    }
}

fn slot_placeholder_has_wrapper_context(placeholder: &TirSlotPlaceholder) -> bool {
    placeholder.applied_child_wrapper_set.is_some()
        || placeholder.child_wrapper_set.is_some()
        || placeholder.skip_parent_child_wrappers
}

// -------------------------
//  Read-only (snapshot-free) fold safety
// -------------------------

/// Returns `true` when a `TirView` root can be folded without cloning or
/// mutating the module `TemplateIrStore`.
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
pub(crate) fn tir_view_is_read_only_fold_safe(view: &TirView<'_>, store: &TemplateIrStore) -> bool {
    let root = view.root_ref();
    if root.store_id != store.store_id() {
        return false;
    }

    // Only empty-overlay views are safe for the read-only path in Phase 3A.
    // Expression, slot, and wrapper-context overlays require applying
    // effective data to a cloned store, which belongs to Phase 4 and later.
    let overlay_set = match view.overlay_set() {
        Ok(overlay_set) => overlay_set,
        Err(_) => return false,
    };
    if !overlay_set.is_empty() {
        return false;
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
) -> bool {
    // A template already on the visiting stack would recurse indefinitely in
    // the fold walker, which has no child-template cycle guard.
    if !visiting.insert(template_id) {
        return false;
    }

    let Some(template) = store.get_template(template_id) else {
        return false;
    };

    // Runtime slot plans require HIR/runtime lowering, not compile-time folding.
    if template.runtime_slot_plan.is_some() {
        return false;
    }

    // Conditional child wrappers are folded through a virtual wrapper path that
    // does not push synthetic nodes into the store. Keep the gate matched to
    // the shapes that path can fold so fallback handles malformed or still
    // unsupported wrapper subtrees.
    if let Some(wrapper_set_id) = template.conditional_child_wrapper_set
        && !wrapper_set_is_virtual_fold_safe(store, view, wrapper_set_id, visiting)
    {
        return false;
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
) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children.iter().all(|child| {
            tir_node_is_read_only_fold_safe(store, view, *child, in_aggregate_wrapper, visiting)
        }),

        // Reactive segments must stay on the structured handoff path.
        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(node_id).is_none(),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => reactive_subscription.is_none(),

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            // Only same-store child references can be folded read-only.
            let Some(child_template_id) = reference.template_id_in_store(store.store_id()) else {
                return false;
            };

            // The child's overlay set must also be empty. Look it up through
            // the registry that backs the parent view.
            let child_overlay_safe = view
                .registry_ref()
                .overlay_set(reference.overlay_set_id)
                .is_some_and(TemplateOverlaySet::is_empty);

            if !child_overlay_safe {
                return false;
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
            let branches_safe = branches.iter().all(|branch| {
                tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    branch.body,
                    in_aggregate_wrapper,
                    visiting,
                )
            });

            let fallback_safe = match fallback {
                Some(fallback_id) => tir_node_is_read_only_fold_safe(
                    store,
                    view,
                    *fallback_id,
                    in_aggregate_wrapper,
                    visiting,
                ),
                None => true,
            };

            branches_safe && fallback_safe
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            let body_safe = tir_node_is_read_only_fold_safe(store, view, *body, false, visiting);
            let wrapper_safe = match aggregate_wrapper {
                Some(wrapper_id) => {
                    tir_node_is_read_only_fold_safe(store, view, *wrapper_id, true, visiting)
                }
                None => true,
            };
            body_safe && wrapper_safe
        }

        // Loop-control signals are safe: the fold walker just returns them.
        TemplateIrNodeKind::LoopControl { .. } => true,

        // Empty-overlay slots are rejected because read-only folding cannot
        // tell whether the slot is genuinely empty or still needs insert
        // contribution resolution. The view-native path only accepts slots
        // with an explicit slot-resolution overlay.
        TemplateIrNodeKind::Slot { .. } => false,

        // AggregateOutput markers are valid only inside aggregate wrapper
        // subtrees, where the wrapper fold path consumes them.
        TemplateIrNodeKind::AggregateOutput => in_aggregate_wrapper,

        // Insert contributions should have been consumed by slot composition.
        TemplateIrNodeKind::InsertContribution { .. } => false,

        // Runtime slot sites require HIR/runtime lowering.
        TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}

/// Returns true when a wrapper-context overlay entry can be folded around a
/// child-template emission without falling back to current-state materialization.
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
) -> bool {
    // `$fresh` suppresses parent-applied wrappers, so there is nothing to fold.
    if context.skip_parent_child_wrappers {
        return true;
    }

    let Some(wrapper_set_ref) = context.inherited_wrapper_set else {
        return true;
    };

    if wrapper_set_ref.store_id != store.store_id() {
        return false;
    }

    // Wrapper templates are checked independently of the outer template walk so
    // that wrapper-set membership does not create false cycle reports against
    // the parent/child recursion guard.
    let mut visiting = HashSet::new();
    wrapper_set_is_virtual_fold_safe(store, view, wrapper_set_ref.wrapper_set_id, &mut visiting)
}

/// Returns true when every wrapper in the set can be folded by the virtual
/// conditional-wrapper path.
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
) -> bool {
    let Some(wrapper_set) = store.get_wrapper_set(wrapper_set_id) else {
        return false;
    };

    wrapper_set.wrappers.iter().all(|wrapper| {
        wrapper.root.store_id == store.store_id()
            && wrapper_template_is_virtual_fold_safe(
                store,
                view,
                wrapper.root.template_id,
                false,
                visiting,
            )
    })
}

fn wrapper_template_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    template_id: TemplateIrId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> bool {
    if !visiting.insert(template_id) {
        return false;
    }

    let safe = match store.get_template(template_id) {
        Some(template) if template.runtime_slot_plan.is_none() => {
            wrapper_node_is_virtual_fold_safe(
                store,
                view,
                template.root,
                in_aggregate_wrapper,
                visiting,
            )
        }
        _ => false,
    };

    visiting.remove(&template_id);
    safe
}

fn wrapper_node_is_virtual_fold_safe(
    store: &TemplateIrStore,
    view: &TirView<'_>,
    node_id: TemplateIrNodeId,
    in_aggregate_wrapper: bool,
    visiting: &mut HashSet<TemplateIrId>,
) -> bool {
    let Some(node) = store.get_node(node_id) else {
        return false;
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children.iter().all(|child| {
            wrapper_node_is_virtual_fold_safe(store, view, *child, in_aggregate_wrapper, visiting)
        }),

        TemplateIrNodeKind::Text { .. } => store.node_reactive_subscription(node_id).is_none(),

        TemplateIrNodeKind::DynamicExpression {
            reactive_subscription,
            ..
        } => reactive_subscription.is_none(),

        TemplateIrNodeKind::Slot { placeholder } => {
            !in_aggregate_wrapper && !slot_placeholder_has_wrapper_context(placeholder)
        }

        TemplateIrNodeKind::ChildTemplate { reference, .. } => {
            let Some(child_template_id) = reference.template_id_in_store(store.store_id()) else {
                return false;
            };

            let child_overlay_safe = view
                .registry_ref()
                .overlay_set(reference.overlay_set_id)
                .is_some_and(TemplateOverlaySet::is_empty);

            child_overlay_safe
                && wrapper_template_is_virtual_fold_safe(
                    store,
                    view,
                    child_template_id,
                    in_aggregate_wrapper,
                    visiting,
                )
        }

        TemplateIrNodeKind::BranchChain { branches, fallback } => {
            !in_aggregate_wrapper
                && branches.iter().all(|branch| {
                    wrapper_node_is_virtual_fold_safe(
                        store,
                        view,
                        branch.body,
                        in_aggregate_wrapper,
                        visiting,
                    )
                })
                && fallback.is_none_or(|fallback_id| {
                    wrapper_node_is_virtual_fold_safe(
                        store,
                        view,
                        fallback_id,
                        in_aggregate_wrapper,
                        visiting,
                    )
                })
        }

        TemplateIrNodeKind::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            !in_aggregate_wrapper
                && wrapper_node_is_virtual_fold_safe(store, view, *body, false, visiting)
                && aggregate_wrapper.is_none_or(|wrapper_id| {
                    wrapper_node_is_virtual_fold_safe(store, view, wrapper_id, true, visiting)
                })
        }

        TemplateIrNodeKind::AggregateOutput => in_aggregate_wrapper,

        TemplateIrNodeKind::InsertContribution { .. }
        | TemplateIrNodeKind::LoopControl { .. }
        | TemplateIrNodeKind::RuntimeSlotSite { .. } => false,
    }
}
