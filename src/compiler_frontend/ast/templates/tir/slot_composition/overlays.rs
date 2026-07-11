//! TIR slot-resolution overlay materialization and merging.
//!
//! WHAT: converts routed slot contributions into registry-owned
//!       `TirSlotResolutionOverlay` payloads, attaches them to canonical overlay
//!       sets, and merges multiple overlay sets without losing slot-resolution
//!       entries.
//!
//! WHY: the overlay path lets `TirView` resolve slot placeholders by occurrence
//!      ID rather than by re-running structural composition. Keeping overlay
//!      allocation, attachment, and merge in one module makes the
//!      route→materialize→attach→merge lifecycle explicit.

use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::tir::TemplateIrId;
use crate::compiler_frontend::ast::templates::tir::ids::SlotOccurrenceId;
use crate::compiler_frontend::ast::templates::tir::node::TirSlotPlaceholder;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirSlotResolution, TirSlotResolutionOverlay,
    TirSlotResolutionOverlayId,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, compiler_errors::compiler_error_to_diagnostic,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

use rustc_hash::{FxHashMap, FxHashSet};

use super::contributions::{RoutedTirSlotContributions, route_tir_slot_contributions};
use super::helpers::{SlotResolutionComposition, build_tir_fill_template, internal_compiler_error};
use super::schema::collect_tir_slot_placeholders_in_order;

/// Boxed diagnostic result for slot-resolution overlay construction and merging.
type SlotCompositionResult<T> = Result<T, Box<CompilerDiagnostic>>;

/// Test-only materialization of routed slot contributions into a registry-owned
/// `TirSlotResolutionOverlay` keyed by `SlotOccurrenceId`.
///
/// WHAT: walks the wrapper template slot placeholders in document order,
///       builds one store-qualified `TemplateRef` source per slot key that
///       received routed contributions, and records one `TirSlotResolution`
///       per slot occurrence. Occurrences of slots that received no content
///       become explicit `Missing` resolutions; repeated occurrences of the
///       same key share the same source list so replay is represented by data,
///       not by consuming the routed bucket.
/// WHY: focused tests exercise the overlay payload shape without exposing this
///      route/materialize boundary as a production module API. Production
///      overlay allocation goes through the registry-level composition helpers
///      that collect and merge wrapper/fill pairs.
///
/// The overlay is allocated on the supplied registry and the source templates
/// are created in the registry-owned `store_id`, so every source `TemplateRef`
/// remains resolvable by the same registry that owns the overlay.
#[cfg(test)]
pub(crate) fn materialize_tir_slot_resolution_overlay(
    registry: &mut TemplateIrRegistry,
    store_id: TemplateStoreId,
    wrapper_reference: TemplateTirChildReference,
    routed: &RoutedTirSlotContributions,
) -> SlotCompositionResult<TirSlotResolutionOverlayId> {
    let overlay =
        build_tir_slot_resolution_overlay_payload(registry, store_id, wrapper_reference, routed)?;

    Ok(registry.allocate_slot_resolution_overlay(overlay))
}

/// Builds a slot-resolution overlay payload while leaving allocation to the caller.
///
/// WHY: single-pair callers allocate the returned payload directly, while
///      multi-pair composition merges several payloads into one registry-owned
///      overlay so a `TemplateOverlaySet` never has to carry competing
///      slot-resolution dimensions.
fn build_tir_slot_resolution_overlay_payload(
    registry: &mut TemplateIrRegistry,
    store_id: TemplateStoreId,
    wrapper_reference: TemplateTirChildReference,
    routed: &RoutedTirSlotContributions,
) -> SlotCompositionResult<TirSlotResolutionOverlay> {
    let wrapper_template_id = wrapper_reference
        .template_id_in_store(store_id)
        .ok_or_else(|| {
            internal_compiler_error(
                "TIR slot-overlay materialization: effective wrapper reference is not in the composition store.",
            )
        })?;

    let mut store = registry
        .store_mut(store_id)
        .map_err(|error| compiler_error_to_diagnostic(&error))?;

    let root_node_id = super::helpers::root_node_id_for_template(&store, wrapper_template_id)?;
    let placeholders = collect_tir_slot_placeholders_in_order(&store, root_node_id)?;

    let resolutions = build_slot_resolution_entries(&mut store, placeholders, routed)?;

    Ok(TirSlotResolutionOverlay { resolutions })
}

/// Builds occurrence-keyed slot-resolution entries from collected placeholders
/// and routed contributions.
///
/// WHAT: walks the placeholders in document order, looks up the routed
///       contribution nodes for each slot key, builds one fill/source template
///       per key in the composition store, and returns occurrence-keyed
///       `TirSlotResolution` entries. Repeated occurrences of the same key
///       reuse the same `TemplateRef` so the overlay represents replay by
///       shared data, matching the structural expansion non-consuming reuse.
/// WHY: shared between same-store and cross-store overlay payload builders.
///      Both paths collect placeholders (from the same or foreign store) and
///      build fill source templates in the composition store. Extracting this
///      logic prevents duplicating the entry-building traversal.
pub(super) fn build_slot_resolution_entries(
    store: &mut TemplateIrStore,
    placeholders: Vec<TirSlotPlaceholder>,
    routed: &RoutedTirSlotContributions,
) -> SlotCompositionResult<Vec<(SlotOccurrenceId, TirSlotResolution)>> {
    let mut source_refs_by_key: FxHashMap<SlotKey, TemplateRef> = FxHashMap::default();
    let mut resolutions = Vec::with_capacity(placeholders.len());

    for placeholder in placeholders {
        let key = &placeholder.key;
        let contribution_nodes = routed.contributions.nodes_for_slot(key);

        let resolution = if contribution_nodes.is_empty() {
            // A declared slot that received no routed content renders empty,
            // matching the structural expansion empty-Sequence behavior.
            TirSlotResolution::missing(key.clone())
        } else {
            let source_ref = match source_refs_by_key.get(key) {
                Some(existing) => *existing,
                None => {
                    // Build an internal fill/source template from the routed
                    // node bucket and qualify it with this store ID so the
                    // overlay carries stable, store-qualified `TemplateRef`s
                    // rather than bare store-local node IDs.
                    let first_node_id = contribution_nodes[0];
                    let source_template_id =
                        build_tir_fill_template(store, contribution_nodes.to_vec(), first_node_id)?;
                    let source_ref = store.qualify_template_ref(source_template_id);
                    source_refs_by_key.insert(key.clone(), source_ref);
                    source_ref
                }
            };

            TirSlotResolution::resolved(key.clone(), vec![source_ref])
        };

        resolutions.push((placeholder.occurrence_id, resolution));
    }

    Ok(resolutions)
}

/// Attaches a materialized slot-resolution overlay to a registry-owned
/// `TemplateOverlaySet` and returns the canonical set ID.
///
/// WHAT: allocates a minimal one-dimension overlay set whose `slot_resolution`
///       field carries `slot_resolution_overlay_id`, leaving the expression and
///       wrapper-context dimensions unset. The registry canonicalizes the set so
///       equivalent attachments share one ID.
/// WHY: this is the second Phase 6 overlay-composition step. After
///      `materialize_tir_slot_resolution_overlay` converts routed contributions
///      into a registry-owned overlay, consumers need a single
///      `TemplateOverlaySetId` to thread that overlay through `TirView` and
///      later composition paths. Allocating the set here keeps the join between
///      slot routing and the view read API in the slot-composition owner, so
///      callers never assemble overlay sets ad hoc. Production structural
///      expansion remains unchanged until the overlay-backed composition path is
///      explicitly wired.
pub(crate) fn attach_tir_slot_resolution_overlay(
    registry: &mut TemplateIrRegistry,
    slot_resolution_overlay_id: TirSlotResolutionOverlayId,
) -> TemplateOverlaySetId {
    registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_resolution_overlay_id),
        wrapper_context: None,
    })
}

/// Test-only composition of the slot-resolution overlay set for one
/// wrapper/fill pair on a registry-owned store.
///
/// WHAT: routes fill contributions against the wrapper's slot schema,
///       materializes a `TirSlotResolutionOverlay`, and attaches it to a
///       canonical `TemplateOverlaySet`. The caller passes registry/store
///       identity instead of holding a separate store borrow.
/// WHY: focused tests compare the bundled route/materialize/attach sequence
///      against manual overlay construction. Production callers use
///      `compose_tir_head_chain_with_overlays` and
///      `apply_tir_child_wrappers_with_overlays`, which collect all
///      wrapper/fill pairs and allocate one merged overlay set.
///
/// Structural expansion (`expand_tir_slot_placeholders`) is intentionally left
/// on its existing store-local path. This helper allocates only the overlay
/// context so tests can inspect that context through `TirView`.
#[cfg(test)]
pub(crate) fn compose_tir_slot_resolution_overlay_set(
    registry: &mut TemplateIrRegistry,
    store_id: TemplateStoreId,
    wrapper_reference: TemplateTirChildReference,
    fill_reference: TemplateRef,
    string_table: &StringTable,
) -> SlotCompositionResult<TemplateOverlaySetId> {
    let wrapper_template_id = wrapper_reference
        .template_id_in_store(store_id)
        .ok_or_else(|| {
            internal_compiler_error(
                "TIR slot-overlay composition: effective wrapper reference is not in the composition store.",
            )
        })?;
    let fill_template_id = template_id_in_store(
        fill_reference,
        store_id,
        "TIR slot-overlay composition: fill reference is not in the composition store.",
    )?;

    // Route read-only through the registry-owned store borrow. The borrow is
    // scoped so it is dropped before `materialize_tir_slot_resolution_overlay`
    // re-borrows the same store mutably through the registry.
    let routed = {
        let store = registry.store(store_id).ok_or_else(|| {
            internal_compiler_error(
                "TIR slot-overlay composition: store ID was not present in the registry.",
            )
        })?;

        route_tir_slot_contributions(&store, wrapper_template_id, fill_template_id, string_table)?
    };

    let overlay_id =
        materialize_tir_slot_resolution_overlay(registry, store_id, wrapper_reference, &routed)?;

    Ok(attach_tir_slot_resolution_overlay(registry, overlay_id))
}

/// Allocates one non-empty slot-resolution overlay set from collected
/// wrapper/fill pairs, returning `None` when no slots were resolved.
///
/// WHAT: routes every collected wrapper/fill pair, materializes each pair into
///       a slot-resolution payload, merges those occurrence-keyed entries, and
///       allocates one overlay set for the merged payload.
/// WHY: `TemplateOverlaySet` carries one slot-resolution dimension. Combining
///      multiple wrapper/fill pairs by composing overlay sets would let later
///      slot overlays overwrite earlier ones, so the slot-composition owner
///      merges the payloads before allocation instead.
pub(super) fn allocate_slot_resolution_overlay_set(
    registry: &mut TemplateIrRegistry,
    store_id: TemplateStoreId,
    slot_compositions: &[SlotResolutionComposition],
    string_table: &StringTable,
) -> SlotCompositionResult<Option<TemplateOverlaySetId>> {
    if slot_compositions.is_empty() {
        return Ok(None);
    }

    let mut resolutions = Vec::new();
    let mut seen_occurrences = FxHashSet::default();

    for pair in slot_compositions {
        let wrapper_template_id = pair
            .wrapper_reference
            .template_id_in_store(store_id)
            .ok_or_else(|| {
                internal_compiler_error(
                    "TIR slot-overlay composition: effective wrapper reference is not in the composition store.",
                )
            })?;
        let fill_template_id = template_id_in_store(
            pair.fill_reference,
            store_id,
            "TIR slot-overlay composition: fill reference is not in the composition store.",
        )?;

        let routed = {
            let store = registry.store(store_id).ok_or_else(|| {
                internal_compiler_error(
                    "TIR slot-overlay composition: store ID was not present in the registry.",
                )
            })?;

            route_tir_slot_contributions(
                &store,
                wrapper_template_id,
                fill_template_id,
                string_table,
            )?
        };

        let overlay = build_tir_slot_resolution_overlay_payload(
            registry,
            store_id,
            pair.wrapper_reference,
            &routed,
        )?;

        merge_slot_resolution_entries(
            &mut resolutions,
            &mut seen_occurrences,
            overlay.resolutions,
        )?;
    }

    let overlay_id =
        registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay { resolutions });

    Ok(Some(attach_tir_slot_resolution_overlay(
        registry, overlay_id,
    )))
}

/// Recovers a store-local template ID only after proving the qualified ref
/// belongs to the active structural-composition store.
fn template_id_in_store(
    reference: TemplateRef,
    store_id: TemplateStoreId,
    error_message: &str,
) -> SlotCompositionResult<TemplateIrId> {
    if reference.store_id != store_id {
        return Err(Box::new(internal_compiler_error(error_message)));
    }

    Ok(reference.template_id)
}

/// Merges one slot-resolution entry list into `merged`, rejecting duplicate keys.
///
/// WHY: `SlotOccurrenceId`s are store-owned occurrence identities. A duplicate
///      key in one merged overlay would make `TirView::effective_slot_resolution`
///      ambiguous because lookup returns the first match.
fn merge_slot_resolution_entries(
    merged: &mut Vec<(SlotOccurrenceId, TirSlotResolution)>,
    seen_occurrences: &mut FxHashSet<SlotOccurrenceId>,
    entries: Vec<(SlotOccurrenceId, TirSlotResolution)>,
) -> SlotCompositionResult<()> {
    for (occurrence_id, resolution) in entries {
        if !seen_occurrences.insert(occurrence_id) {
            return Err(Box::new(internal_compiler_error(&format!(
                "TIR slot-overlay composition: duplicate slot occurrence {} while merging overlays.",
                occurrence_id
            ))));
        }

        merged.push((occurrence_id, resolution));
    }

    Ok(())
}

/// Merges a newly produced slot-resolution overlay set into an existing set.
///
/// WHAT: preserves non-slot dimensions from `base_set_id`, merges slot
///       resolution payloads from both sets when both are present, and returns a
///       canonical overlay-set ID for the combined context.
/// WHY: production composition can apply child wrappers and then head-chain
///      composition. Both passes may resolve slots, and composing overlay sets
///      directly would overwrite one slot-resolution dimension with the other.
pub(crate) fn merge_tir_slot_resolution_overlay_sets(
    registry: &mut TemplateIrRegistry,
    base_set_id: TemplateOverlaySetId,
    next_set_id: TemplateOverlaySetId,
) -> SlotCompositionResult<TemplateOverlaySetId> {
    let base_set = registry.overlay_set(base_set_id).cloned().ok_or_else(|| {
        internal_compiler_error(&format!(
            "TIR slot-overlay merge: base overlay set {} was not present in the registry.",
            base_set_id
        ))
    })?;

    let next_set = registry.overlay_set(next_set_id).cloned().ok_or_else(|| {
        internal_compiler_error(&format!(
            "TIR slot-overlay merge: next overlay set {} was not present in the registry.",
            next_set_id
        ))
    })?;

    let slot_resolution = match (base_set.slot_resolution, next_set.slot_resolution) {
        (None, next) => next,
        (base, None) => base,
        (Some(base_overlay_id), Some(next_overlay_id)) => {
            let mut resolutions = Vec::new();
            let mut seen_occurrences = FxHashSet::default();

            let base_overlay = registry
                .slot_resolution_overlay(base_overlay_id)
                .cloned()
                .ok_or_else(|| {
                    internal_compiler_error(&format!(
                        "TIR slot-overlay merge: base slot overlay {} was not present in the registry.",
                        base_overlay_id
                    ))
                })?;
            merge_slot_resolution_entries(
                &mut resolutions,
                &mut seen_occurrences,
                base_overlay.resolutions,
            )?;

            let next_overlay = registry
                .slot_resolution_overlay(next_overlay_id)
                .cloned()
                .ok_or_else(|| {
                    internal_compiler_error(&format!(
                        "TIR slot-overlay merge: next slot overlay {} was not present in the registry.",
                        next_overlay_id
                    ))
                })?;
            merge_slot_resolution_entries(
                &mut resolutions,
                &mut seen_occurrences,
                next_overlay.resolutions,
            )?;

            Some(
                registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay { resolutions }),
            )
        }
    };

    Ok(registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: next_set
            .expression_overrides
            .or(base_set.expression_overrides),
        slot_resolution,
        wrapper_context: next_set.wrapper_context.or(base_set.wrapper_context),
    }))
}
