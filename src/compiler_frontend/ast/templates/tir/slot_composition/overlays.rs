//! TIR slot-resolution overlay materialization and merging.
//!
//! WHAT: converts routed slot contributions into store-owned
//!       `TirSlotResolutionOverlay` payloads, attaches them to store-owned
//!       overlays, and merges multiple value-carried view contexts without
//!       losing slot-resolution entries.
//!
//! WHY: the overlay path lets `TirView` resolve slot placeholders by occurrence
//!      ID rather than by re-running structural composition. Keeping overlay
//!      allocation, attachment, and merge in one module makes the
//!      route→materialize→contextualize→merge lifecycle explicit.

use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::tir::ids::SlotOccurrenceId;
use crate::compiler_frontend::ast::templates::tir::node::TirSlotPlaceholder;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateViewContext, TirSlotResolution, TirSlotResolutionOverlay, TirSlotResolutionOverlayId,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateIrId, TemplateTirChildReference,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use rustc_hash::{FxHashMap, FxHashSet};

use super::contributions::{RoutedTirSlotContributions, route_tir_slot_contributions};
use super::helpers::{SlotResolutionComposition, build_tir_fill_template, internal_compiler_error};
use super::schema::collect_tir_slot_placeholders_in_order;

/// Boxed diagnostic result for slot-resolution overlay construction and merging.
type SlotCompositionResult<T> = Result<T, Box<CompilerDiagnostic>>;

/// Test-only materialization of routed slot contributions into a store-owned
/// `TirSlotResolutionOverlay` keyed by `SlotOccurrenceId`.
///
/// WHAT: walks the wrapper template slot placeholders in document order,
///       builds one module-local `TemplateIrId` source per slot key that
///       received routed contributions, and records one `TirSlotResolution`
///       per slot occurrence. Occurrences of slots that received no content
///       become explicit `Missing` resolutions; repeated occurrences of the
///       same key share the same source list so replay is represented by data,
///       not by consuming the routed bucket.
/// WHY: focused tests exercise the overlay payload shape without exposing this
///      route/materialize boundary as a production module API. Production
///      overlay allocation goes through the store-level composition helpers
///      that collect and merge wrapper/fill pairs.
///
/// The overlay is allocated on the supplied module store and the source
/// templates are created in that same store, so every source `TemplateIrId`
/// remains resolvable by the store that owns the overlay.
#[cfg(test)]
pub(crate) fn materialize_tir_slot_resolution_overlay(
    store: &mut TemplateIrStore,
    wrapper_reference: TemplateTirChildReference,
    routed: &RoutedTirSlotContributions,
) -> SlotCompositionResult<TirSlotResolutionOverlayId> {
    let overlay = build_tir_slot_resolution_overlay_payload(store, wrapper_reference, routed)?;

    Ok(store.allocate_slot_resolution_overlay(overlay))
}

/// Builds a slot-resolution overlay payload while leaving allocation to the caller.
///
/// WHY: single-pair callers allocate the returned payload directly, while
///      multi-pair composition merges several payloads into one store-owned
///      overlay so a `TemplateViewContext` never has to carry competing
///      slot-resolution dimensions.
fn build_tir_slot_resolution_overlay_payload(
    store: &mut TemplateIrStore,
    wrapper_reference: TemplateTirChildReference,
    routed: &RoutedTirSlotContributions,
) -> SlotCompositionResult<TirSlotResolutionOverlay> {
    let wrapper_template_id = wrapper_reference.root;
    let root_node_id = super::helpers::root_node_id_for_template(store, wrapper_template_id)?;
    let placeholders = collect_tir_slot_placeholders_in_order(store, root_node_id)?;

    let resolutions = build_slot_resolution_entries(&mut *store, placeholders, routed)?;

    Ok(TirSlotResolutionOverlay { resolutions })
}

/// Builds occurrence-keyed slot-resolution entries from collected placeholders
/// and routed contributions.
///
/// WHAT: walks the placeholders in document order, looks up the routed
///       contribution nodes for each slot key, builds one fill/source template
///       per key in the composition store, and returns occurrence-keyed
///       `TirSlotResolution` entries. Repeated occurrences of the same key
///       reuse the same `TemplateIrId` so the overlay represents replay by
///       shared data, matching the structural expansion non-consuming reuse.
/// WHY: shared by the overlay payload builders. Both call sites collect
///      placeholders from the module store and build fill source templates in
///      the composition store. Extracting this logic prevents duplicating the
///      entry-building traversal.
pub(super) fn build_slot_resolution_entries(
    store: &mut TemplateIrStore,
    placeholders: Vec<TirSlotPlaceholder>,
    routed: &RoutedTirSlotContributions,
) -> SlotCompositionResult<Vec<(SlotOccurrenceId, TirSlotResolution)>> {
    let mut source_refs_by_key: FxHashMap<SlotKey, TemplateIrId> = FxHashMap::default();
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
                    // node bucket so the overlay carries stable module-local
                    // `TemplateIrId`s rather than bare `TemplateIrNodeId`s.
                    let first_node_id = contribution_nodes[0];
                    let source_template_id =
                        build_tir_fill_template(store, contribution_nodes.to_vec(), first_node_id)?;
                    source_refs_by_key.insert(key.clone(), source_template_id);
                    source_template_id
                }
            };

            TirSlotResolution::resolved(key.clone(), vec![source_ref])
        };

        resolutions.push((placeholder.occurrence_id, resolution));
    }

    Ok(resolutions)
}

/// Creates a value-carried `TemplateViewContext` from a materialized
/// slot-resolution overlay ID.
///
/// WHAT: returns a minimal one-dimension view context whose `slot_resolution`
///       field carries `slot_resolution_overlay_id`, leaving the expression and
///       wrapper-context dimensions unset.
/// WHY: this is the second Phase 6 overlay-composition step. After
///      `materialize_tir_slot_resolution_overlay` converts routed contributions
///      into a store-owned overlay, consumers need a single
///      `TemplateViewContext` to thread that overlay through `TirView` and
///      later composition paths. Keeping the join between
///      slot routing and the view read API in the slot-composition owner, so
///      callers never assemble view contexts ad hoc. Production structural
///      expansion remains unchanged until the overlay-backed composition path is
///      explicitly wired.
pub(crate) fn view_context_from_slot_resolution_overlay(
    slot_resolution_overlay_id: TirSlotResolutionOverlayId,
) -> TemplateViewContext {
    TemplateViewContext {
        expression_overlay: None,
        slot_resolution: Some(slot_resolution_overlay_id),
        wrapper_context: None,
    }
}

/// Test-only composition of the slot-resolution view context for one
/// wrapper/fill pair on one module-local store.
///
/// WHAT: routes fill contributions against the wrapper's slot schema,
///       materializes a `TirSlotResolutionOverlay`, and builds a value-carried
///       `TemplateViewContext` from its ID. The caller passes store identity
///       instead of holding a separate store borrow.
/// WHY: focused tests compare the bundled route/materialize/context sequence
///      against manual overlay construction. Production callers use
///      `compose_tir_head_chain_with_overlays`, which collects all
///      wrapper/fill pairs (via `wrap_tir_node_in_wrappers_into`) and
///      constructs one merged value context.
///
/// Structural expansion (`expand_tir_slot_placeholders`) is intentionally left
/// on its existing store-local path. This helper allocates only the overlay
/// payload so tests can inspect the resulting context through `TirView`.
#[cfg(test)]
pub(crate) fn compose_tir_slot_resolution_context(
    store: &mut TemplateIrStore,
    wrapper_reference: TemplateTirChildReference,
    fill_reference: TemplateIrId,
    string_table: &StringTable,
) -> SlotCompositionResult<TemplateViewContext> {
    let wrapper_template_id = wrapper_reference.root;
    let fill_template_id = fill_reference;

    // Route read-only through the module-store borrow. The borrow is
    // scoped so it is dropped before `materialize_tir_slot_resolution_overlay`
    // re-borrows the same store mutably through the store.
    let routed = {
        route_tir_slot_contributions(store, wrapper_template_id, fill_template_id, string_table)?
    };

    let overlay_id = materialize_tir_slot_resolution_overlay(store, wrapper_reference, &routed)?;

    Ok(view_context_from_slot_resolution_overlay(overlay_id))
}

/// Constructs one non-empty slot-resolution view context from collected
/// wrapper/fill pairs, returning `None` when no slots were resolved.
///
/// WHAT: routes every collected wrapper/fill pair, materializes each pair into
///       a slot-resolution payload, merges those occurrence-keyed entries, and
///       returns one value context for the merged payload.
/// WHY: `TemplateViewContext` carries one slot-resolution dimension. Combining
///      multiple wrapper/fill pairs by composing view contexts would let later
///      slot overlays overwrite earlier ones, so the slot-composition owner
///      merges the payloads before constructing the value context instead.
pub(super) fn allocate_slot_resolution_context(
    store: &mut TemplateIrStore,
    slot_compositions: &[SlotResolutionComposition],
    string_table: &StringTable,
) -> SlotCompositionResult<Option<TemplateViewContext>> {
    if slot_compositions.is_empty() {
        return Ok(None);
    }

    let mut resolutions = Vec::new();
    let mut seen_occurrences = FxHashSet::default();

    for pair in slot_compositions {
        let wrapper_template_id = pair.wrapper_reference.root;
        let fill_template_id = pair.fill_reference;

        let routed = route_tir_slot_contributions(
            store,
            wrapper_template_id,
            fill_template_id,
            string_table,
        )?;

        let overlay =
            build_tir_slot_resolution_overlay_payload(store, pair.wrapper_reference, &routed)?;

        merge_slot_resolution_entries(
            &mut resolutions,
            &mut seen_occurrences,
            overlay.resolutions,
        )?;
    }

    let overlay_id =
        store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay { resolutions });

    Ok(Some(view_context_from_slot_resolution_overlay(overlay_id)))
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

/// Merges a newly produced slot-resolution view context into an existing context.
///
/// WHAT: preserves non-slot dimensions from `base_context`, merges slot
///       resolution payloads from both contexts when both are present, and
///       returns the combined value context.
/// WHY: production composition can apply child wrappers and then head-chain
///      composition. Both passes may resolve slots, and composing view contexts
///      directly would overwrite one slot-resolution dimension with the other.
pub(crate) fn merge_tir_slot_resolution_contexts(
    store: &mut TemplateIrStore,
    base_context: TemplateViewContext,
    next_context: TemplateViewContext,
) -> SlotCompositionResult<TemplateViewContext> {
    let slot_resolution = match (base_context.slot_resolution, next_context.slot_resolution) {
        (None, next) => next,
        (base, None) => base,
        (Some(base_overlay_id), Some(next_overlay_id)) => {
            let mut resolutions = Vec::new();
            let mut seen_occurrences = FxHashSet::default();

            let base_overlay = store
                .slot_resolution_overlay(base_overlay_id)
                .cloned()
                .ok_or_else(|| {
                    internal_compiler_error(&format!(
                        "TIR slot-overlay merge: base slot overlay {} was not present in the store.",
                        base_overlay_id
                    ))
                })?;
            merge_slot_resolution_entries(
                &mut resolutions,
                &mut seen_occurrences,
                base_overlay.resolutions,
            )?;

            let next_overlay = store
                .slot_resolution_overlay(next_overlay_id)
                .cloned()
                .ok_or_else(|| {
                    internal_compiler_error(&format!(
                        "TIR slot-overlay merge: next slot overlay {} was not present in the store.",
                        next_overlay_id
                    ))
                })?;
            merge_slot_resolution_entries(
                &mut resolutions,
                &mut seen_occurrences,
                next_overlay.resolutions,
            )?;

            Some(store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay { resolutions }))
        }
    };

    Ok(TemplateViewContext {
        expression_overlay: next_context
            .expression_overlay
            .or(base_context.expression_overlay),
        slot_resolution,
        wrapper_context: next_context
            .wrapper_context
            .or(base_context.wrapper_context),
    })
}
