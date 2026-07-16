//! TIR-native head-chain composition.
//!
//! WHAT: resolves wrapper templates in a template's head section against the
//!       body content that fills their slots. It partitions root children by
//!       head/body origin, builds a chain graph of receiving layers, and
//!       resolves each layer by routing contributions and expanding slot
//!       placeholders.
//!
//! WHY: this is the TIR-native equivalent of the atom-level
//!      `compose_template_head_chain_atoms` in `template_composition.rs`. It
//!      replaces the atom-level chain graph with TIR node operations while
//!      reusing the schema, contribution, and expansion helpers.

use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{TemplateSegmentOrigin, TemplateType};
use crate::compiler_frontend::ast::templates::template_slots::{
    materialize_tir_native_runtime_slot_plan, tir_contributions_need_runtime,
};
use crate::compiler_frontend::ast::templates::tir::node::TirSlotPlaceholder;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirSlotResolutionOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::TemplateTirChildReference;
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrId, TemplateIrNode, TemplateIrNodeId, TemplateIrNodeKind, TemplateIrStore,
    TemplateStoreId,
};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use std::cell::RefCell;
use std::rc::Rc;

use super::contributions::route_tir_fill_against_schema;
use super::helpers::{
    ComposedTirRoot, SlotResolutionComposition, build_composed_wrapper_template,
    build_tir_fill_template, children_of_node, internal_compiler_error, rebuild_root_sequence,
    root_node_id_for_template,
};
use super::overlays::{
    allocate_slot_resolution_overlay_set, build_slot_resolution_entries,
    merge_tir_slot_resolution_overlay_sets,
};
use super::schema::{
    collect_tir_slot_placeholders_in_order, collect_tir_slot_schema,
    expand_tir_slot_placeholders_into,
};

/// Boxed diagnostic result for the TIR head-chain composition family.
type HeadChainResult<T> = Result<T, Box<CompilerDiagnostic>>;

/// Bundles the shared state threaded through recursive chain resolution.
///
/// WHAT: carries the string table, accumulated slot compositions, the
///       runtime-plan flag, and optional registry access so recursive
///       `resolve_tir_chain_items` / `resolve_tir_chain_layer` calls stay
///       readable without a long argument list.
/// WHY: the same four values are passed unchanged through every recursion
///      level. Grouping them in one struct keeps the recursive call sites
///      short and makes it obvious that nested layers inherit the caller's
///      real composition state rather than a fresh or default context.
struct HeadChainResolutionInputs<'a> {
    string_table: &'a StringTable,
    slot_compositions: &'a mut Vec<SlotResolutionComposition>,
    allow_runtime_plans: bool,
    registry: &'a Option<Rc<RefCell<TemplateIrRegistry>>>,
}

/// A layer in the TIR head-chain: a wrapper template and the items that should
/// fill its slots.
///
/// WHAT: records one wrapper template opened by a head-origin receiver and the
///       pending items (direct nodes or nested layer references) accumulated as
///       its fill content.
/// WHY: nested head wrappers need one layer-local fill list before the chain is
///      resolved into effective TIR nodes.
struct TirChainLayer {
    /// Effective wrapper identity from the head-origin child occurrence.
    wrapper_reference: TemplateTirChildReference,

    /// Fill content items, in authored order. Items may be direct node IDs or
    /// references to nested chain layers that must be resolved first.
    fill_items: Vec<TirChainItem>,
}

/// Items in the pending TIR head-chain.
///
/// WHAT: each item is either a direct TIR node that passes through unchanged,
///       or a reference to a chain layer that must be resolved into a new
///       `ChildTemplate` node.
/// WHY: this mirrors the legacy `PendingChainItem` from
///      `template_composition.rs` while operating on TIR node IDs.
enum TirChainItem {
    /// A direct node ID (text, dynamic expression, non-receiver child template).
    DirectNode(TemplateIrNodeId),

    /// A reference to a chain layer that needs resolution.
    LayerRef {
        /// Index of the layer in the chain's layer vector.
        layer_index: usize,

        /// The original `ChildTemplate` node ID, used to preserve the source
        /// location when building the resolved `ChildTemplate` node.
        original_node_id: TemplateIrNodeId,
    },
}

/// Composes a template's head-chain by resolving wrapper templates with their
/// fill content, producing a new TIR root node.
///
/// WHAT: walks the template's TIR root `Sequence` children, partitions them by
///       origin (Head vs Body), identifies head-origin wrapper templates that
///       open receiving layers, accumulates fill content for each layer, and
///       resolves each layer by routing and expanding slot contributions.
/// WHY: this is the TIR-native equivalent of the atom-level
///      `compose_template_head_chain_atoms` in `template_composition.rs`. It
///      replaces the atom-level chain graph with TIR node operations, using the
///      already-implemented `route_tir_slot_contributions` and
///      `expand_tir_slot_placeholders`.
pub(crate) fn compose_tir_head_chain(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
    string_table: &StringTable,
    allow_runtime_plans: bool,
) -> HeadChainResult<TemplateIrNodeId> {
    let mut slot_compositions = Vec::new();
    let registry = None;
    let mut inputs = HeadChainResolutionInputs {
        string_table,
        slot_compositions: &mut slot_compositions,
        allow_runtime_plans,
        registry: &registry,
    };
    compose_tir_head_chain_into(store, template_id, &mut inputs)
}

/// Internal head-chain composition that also collects slot-bearing wrapper/fill
/// pairs into `slot_compositions` for later overlay allocation.
///
/// WHY: the registry-level entry point (`compose_tir_head_chain_with_overlays`)
///      needs the pairs to allocate slot-resolution overlays after the store
///      borrow is released. The public `compose_tir_head_chain` discards them.
fn compose_tir_head_chain_into(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
    inputs: &mut HeadChainResolutionInputs,
) -> HeadChainResult<TemplateIrNodeId> {
    let template = store.get_template(template_id).ok_or_else(|| {
        internal_compiler_error(
            "TIR head-chain composition: template ID was not present in the store.",
        )
    })?;

    // Fast path: no child-template references means no wrapper receivers can
    // exist, and the original root is unchanged. The summary is cheap to check
    // and avoids partitioning/walking children for the common case.
    if template.summary.child_template_count == 0 {
        return Ok(template.root);
    }

    let root_node_id = template.root;

    // Fast path: if the root is not a sequence, no receiving layer can exist
    // and the original root is unchanged.
    let Some(root_children) = root_sequence_children(store, root_node_id)? else {
        return Ok(root_node_id);
    };

    // Cheap pre-scan: if no head-origin child template is a wrapper receiver,
    // the original root is unchanged and we can avoid allocating the head/body
    // partition vectors. This matters because many templates contain head
    // references (e.g. function calls) that are not slot-bearing wrappers.
    if !has_tir_head_chain_receiver(store, root_children, inputs.registry)? {
        return Ok(root_node_id);
    }

    let (head_children, body_children) = partition_tir_children_by_origin(store, root_children)?;

    let (root_items, layers) =
        build_tir_chain_graph(store, &head_children, &body_children, inputs.registry)?;

    let resolved_root_children = resolve_tir_chain_items(store, &root_items, &layers, inputs)?;
    let original_root_children = children_of_node(store, root_node_id)?;

    // If no layer produced a new node (for example, every receiver had no fill
    // and stayed unresolved), return the original root to avoid an identical
    // sequence allocation.
    if resolved_root_children == original_root_children {
        return Ok(root_node_id);
    }

    rebuild_root_sequence(store, root_node_id, resolved_root_children)
}

/// Composes the TIR head chain on a registry-owned store and threads a
/// non-empty slot-resolution overlay-set ID onto the result when the
/// composition resolved one or more slot-bearing wrappers.
///
/// WHAT: runs the existing store-local structural head-chain composition
///       (unchanged behavior), collects slot-bearing wrapper/fill pairs, then
///       releases the store borrow and allocates the overlay set through the
///       registry. The store handle is cloned from the registry so the
///       structural borrow is independent of the registry's internal `RefCell`,
///       letting the overlay phase re-borrow the same store through the
///       registry without conflict.
/// WHY: production composition call sites need both the composed root for
///      structural expansion and the overlay-set ID for `TemplateTirReference`
///      threading. Keeping the orchestration in the slot-composition owner
///      avoids ad hoc overlay construction at call sites.
pub(crate) fn compose_tir_head_chain_with_overlays(
    registry: &Rc<RefCell<TemplateIrRegistry>>,
    store_id: TemplateStoreId,
    template_id: TemplateIrId,
    string_table: &StringTable,
    allow_runtime_plans: bool,
) -> HeadChainResult<ComposedTirRoot> {
    let (composed_root, slot_compositions) = {
        let store_handle = registry.borrow().store_handle(store_id).ok_or_else(|| {
            internal_compiler_error(
                "TIR head-chain overlay composition: store ID was not present in the registry.",
            )
        })?;

        let mut store = store_handle.borrow_mut();
        let mut slot_compositions = Vec::new();
        let registry_ref = Some(Rc::clone(registry));
        let mut inputs = HeadChainResolutionInputs {
            string_table,
            slot_compositions: &mut slot_compositions,
            allow_runtime_plans,
            registry: &registry_ref,
        };
        let composed_root = compose_tir_head_chain_into(&mut store, template_id, &mut inputs)?;
        (composed_root, slot_compositions)
    };

    let slot_overlay_set_id = allocate_slot_resolution_overlay_set(
        &mut registry.borrow_mut(),
        store_id,
        &slot_compositions,
        string_table,
    )?;

    Ok(ComposedTirRoot {
        root: composed_root,
        slot_overlay_set_id,
    })
}

/// Returns the children of a `Sequence` root node, or `None` if the root is not
/// a sequence.
///
/// WHAT: centralizes the root-shape check so callers can treat non-sequence
///       roots as uncomposable.
/// WHY: head-chain composition only applies to templates whose body is a
///      sequence of children.
pub(super) fn root_sequence_children(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> HeadChainResult<Option<&[TemplateIrNodeId]>> {
    let Some(node) = store.get_node(node_id) else {
        return Err(Box::new(internal_compiler_error(
            "TIR head-chain composition: root node ID was not present in the store.",
        )));
    };

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => Ok(Some(children)),
        _ => Ok(None),
    }
}

/// Cheaply checks whether a root sequence contains a head-origin wrapper
/// receiver.
///
/// WHAT: walks children in source order, stopping at the first body-origin
///       `Text`/`DynamicExpression`. Within the head prefix, any `ChildTemplate`
///       that references a slot-bearing, non-helper template makes the template
///       a composition candidate.
/// WHY: this avoids allocating head/body vectors for the vast majority of
///      templates whose head references are not slot wrappers.
fn has_tir_head_chain_receiver(
    store: &TemplateIrStore,
    children: &[TemplateIrNodeId],
    registry: &Option<Rc<RefCell<TemplateIrRegistry>>>,
) -> HeadChainResult<bool> {
    let mut saw_body_origin = false;

    for child_id in children {
        let child_node = store.get_node(*child_id).ok_or_else(|| {
            internal_compiler_error(
                "TIR head-chain composition: child node ID was not present in the store while scanning for receivers.",
            )
        })?;

        let is_body = match &child_node.kind {
            TemplateIrNodeKind::Text { origin, .. }
            | TemplateIrNodeKind::DynamicExpression { origin, .. } => {
                *origin == TemplateSegmentOrigin::Body
            }

            TemplateIrNodeKind::AggregateOutput => true,

            _ => saw_body_origin,
        };

        if is_body {
            saw_body_origin = true;
            continue;
        }

        if matches!(child_node.kind, TemplateIrNodeKind::ChildTemplate { .. })
            && is_tir_receiver(store, *child_id, registry)?.is_some()
        {
            return Ok(true);
        }
    }

    Ok(false)
}

/// Partitions root sequence children into head-origin and body-origin groups.
///
/// WHAT: walks children in source order. `Text` and `DynamicExpression` nodes
///       are classified by their `origin` field. `ChildTemplate` and other
///       structural nodes are head-origin until the first body-origin
///       `Text`/`DynamicExpression` is seen, after which they become body-origin.
/// WHY: the parser records head nodes before body nodes, so the first
///      body-origin `Text`/`DynamicExpression` marks the end of the head section.
fn partition_tir_children_by_origin(
    store: &TemplateIrStore,
    children: &[TemplateIrNodeId],
) -> HeadChainResult<(Vec<TemplateIrNodeId>, Vec<TemplateIrNodeId>)> {
    let mut head_children = Vec::new();
    let mut body_children = Vec::new();
    let mut saw_body_origin = false;

    for child_id in children {
        let child_node = store.get_node(*child_id).ok_or_else(|| {
            internal_compiler_error(
                "TIR head-chain composition: child node ID was not present in the store while partitioning.",
            )
        })?;

        let is_body = match &child_node.kind {
            TemplateIrNodeKind::Text { origin, .. }
            | TemplateIrNodeKind::DynamicExpression { origin, .. } => {
                *origin == TemplateSegmentOrigin::Body
            }

            // Aggregate-output markers are compiler-internal fill content for
            // loop aggregate wrappers. They begin the body partition even
            // though they have no text/dynamic origin field.
            TemplateIrNodeKind::AggregateOutput => true,

            // Structural nodes follow the boundary set by Text/DynamicExpression
            // origin. Once a body-origin node has appeared, later structural
            // nodes are treated as body content.
            _ => saw_body_origin,
        };

        if is_body {
            saw_body_origin = true;
            body_children.push(*child_id);
        } else {
            head_children.push(*child_id);
        }
    }

    Ok((head_children, body_children))
}

/// Checks whether a head-origin child node is a wrapper template receiver.
///
/// WHAT: a `ChildTemplate` node is a receiver when its referenced template has
///       slots and its kind is not a slot helper (`SlotInsert` or
///       `SlotDefinition`).
/// WHY: only wrapper templates with unresolved slots open new receiving layers;
///      slot helpers and slot-less templates pass through unchanged.
fn is_tir_receiver(
    store: &TemplateIrStore,
    node_id: TemplateIrNodeId,
    registry: &Option<Rc<RefCell<TemplateIrRegistry>>>,
) -> HeadChainResult<Option<TemplateTirChildReference>> {
    let Some(node) = store.get_node(node_id) else {
        return Err(Box::new(internal_compiler_error(
            "TIR head-chain composition: child node ID was not present in the store while checking receiver.",
        )));
    };

    let TemplateIrNodeKind::ChildTemplate { reference, .. } = &node.kind else {
        return Ok(None);
    };

    // Same-store fast path: read the referenced template directly from the
    // composition store. This covers the vast majority of head-origin child
    // references and avoids a registry borrow.
    if let Some(template_id) = reference.template_id_in_store(store.store_id()) {
        let Some(template_ir) = store.get_template(template_id) else {
            return Err(Box::new(internal_compiler_error(
                "TIR head-chain composition: child template ID was not present in the store.",
            )));
        };

        // Slot helpers are not receivers: they carry contribution content for an
        // immediate parent wrapper and must not open their own layers.
        if matches!(
            template_ir.kind,
            TemplateType::SlotInsert(_) | TemplateType::SlotDefinition(_)
        ) {
            return Ok(None);
        }

        // A child template is a receiver when its TIR tree declares any slot
        // placeholder, including slots nested inside child templates, branch
        // chains, or loops. The cheap `slot_count` summary only counts direct
        // slots, so the schema walk is required to catch wrappers whose slots
        // are not immediate children of the root.
        let schema = collect_tir_slot_schema(store, template_id)?;

        return Ok(if schema.has_any_slots() {
            Some(*reference)
        } else {
            None
        });
    }

    // Cross-store wrapper: read the referenced template from its owning store
    // through the registry so foreign slot-bearing wrappers are recognized as
    // receivers without interpreting foreign IDs in the composition store.
    let Some(registry) = registry else {
        return Err(Box::new(internal_compiler_error(
            "TIR head-chain composition: cross-store child template reference requires a registry, but none is available.",
        )));
    };

    let registry_borrow = registry.borrow();
    let foreign_store_handle = registry_borrow
        .store_handle(reference.root.store_id)
        .ok_or_else(|| {
            internal_compiler_error(
                "TIR head-chain composition: cross-store child template store was not present in the registry.",
            )
        })?;
    let foreign_store = foreign_store_handle.borrow();

    let Some(template_ir) = foreign_store.get_template(reference.root.template_id) else {
        return Err(Box::new(internal_compiler_error(
            "TIR head-chain composition: cross-store child template ID was not present in its owning store.",
        )));
    };

    if matches!(
        template_ir.kind,
        TemplateType::SlotInsert(_) | TemplateType::SlotDefinition(_)
    ) {
        return Ok(None);
    }

    let schema = collect_tir_slot_schema(&foreign_store, reference.root.template_id)?;

    Ok(if schema.has_any_slots() {
        Some(*reference)
    } else {
        None
    })
}

/// Builds the chain graph from partitioned head and body children.
///
/// WHAT: walks head children in order. Receivers open a new layer and become a
///       `LayerRef` item routed to the active layer (or root). Non-receivers
///       become `DirectNode` items. Body children become fill for the deepest
///       active layer, or root items when no layer is active.
/// WHY: this mirrors the atom-level chain-graph construction in
///      `compose_template_head_chain_atoms` while using TIR item references.
fn build_tir_chain_graph(
    store: &TemplateIrStore,
    head_children: &[TemplateIrNodeId],
    body_children: &[TemplateIrNodeId],
    registry: &Option<Rc<RefCell<TemplateIrRegistry>>>,
) -> HeadChainResult<(Vec<TirChainItem>, Vec<TirChainLayer>)> {
    let mut root_items = Vec::new();
    let mut layers = Vec::new();
    let mut active_layer: Option<usize> = None;

    for child_id in head_children {
        if let Some(wrapper_reference) = is_tir_receiver(store, *child_id, registry)? {
            let layer_index = layers.len();

            push_tir_chain_item(
                &mut root_items,
                &mut layers,
                active_layer,
                TirChainItem::LayerRef {
                    layer_index,
                    original_node_id: *child_id,
                },
            );

            layers.push(TirChainLayer {
                wrapper_reference,
                fill_items: Vec::new(),
            });
            active_layer = Some(layer_index);
            continue;
        }

        push_tir_chain_item(
            &mut root_items,
            &mut layers,
            active_layer,
            TirChainItem::DirectNode(*child_id),
        );
    }

    // Body atoms are appended after head parsing. If the head opened a receiving
    // chain, body atoms become contributions to the deepest active receiver.
    for child_id in body_children {
        push_tir_chain_item(
            &mut root_items,
            &mut layers,
            active_layer,
            TirChainItem::DirectNode(*child_id),
        );
    }

    Ok((root_items, layers))
}

/// Routes a chain item to either the root list or the active receiving layer.
fn push_tir_chain_item(
    root_items: &mut Vec<TirChainItem>,
    layers: &mut [TirChainLayer],
    active_layer: Option<usize>,
    item: TirChainItem,
) {
    match active_layer {
        Some(layer_index) => layers[layer_index].fill_items.push(item),
        None => root_items.push(item),
    }
}

/// Recursively resolves pending chain items into concrete TIR node IDs.
///
/// WHAT: direct nodes pass through; layer references trigger bottom-up
///       resolution of the wrapper's slots with the accumulated fill items.
/// WHY: this mirrors the legacy `resolve_pending_chain_items` and
///      `resolve_chain_layer` while using TIR-native routing and expansion.
fn resolve_tir_chain_items(
    store: &mut TemplateIrStore,
    items: &[TirChainItem],
    layers: &[TirChainLayer],
    inputs: &mut HeadChainResolutionInputs,
) -> HeadChainResult<Vec<TemplateIrNodeId>> {
    let mut resolved_nodes = Vec::with_capacity(items.len());

    for item in items {
        match item {
            TirChainItem::DirectNode(node_id) => {
                resolved_nodes.push(*node_id);
            }

            TirChainItem::LayerRef {
                layer_index,
                original_node_id,
            } => {
                let resolved_node = resolve_tir_chain_layer(
                    store,
                    *layer_index,
                    layers,
                    *original_node_id,
                    inputs,
                )?;
                resolved_nodes.push(resolved_node);
            }
        }
    }

    Ok(resolved_nodes)
}

/// Resolves a single chain layer by filling its wrapper's slots with the
/// accumulated fill items.
///
/// WHAT: if the layer has no fill, the wrapper stays as an unresolved
///       `ChildTemplate` reference so later use-sites can still fill its slots.
///       Otherwise, the fill items are resolved recursively, routed against the
///       wrapper's slot schema, and the wrapper's placeholders are expanded.
/// WHY: head-only wrapper references like `[format.table]` must remain usable
///      wrappers; only layers with actual fill content produce a composed
///      template entry.
fn resolve_tir_chain_layer(
    store: &mut TemplateIrStore,
    layer_index: usize,
    layers: &[TirChainLayer],
    original_node_id: TemplateIrNodeId,
    inputs: &mut HeadChainResolutionInputs,
) -> HeadChainResult<TemplateIrNodeId> {
    let layer = &layers[layer_index];

    if layer.fill_items.is_empty() {
        // Head-only wrapper references stay unresolved so they can be filled
        // later at a use-site. This matches the legacy path.
        return Ok(original_node_id);
    }

    // Cross-store wrapper: resolve through the overlay-only path. The wrapper
    // tree stays in its owning (foreign) store; fill nodes and the composed
    // ChildTemplate node stay in the composition store. A slot-resolution
    // overlay is allocated on the registry and attached to the composed
    // ChildTemplate reference so the fold path resolves slots in the wrapper's
    // view context.
    if layer.wrapper_reference.root.store_id != store.store_id() {
        return resolve_cross_store_tir_chain_layer(
            store,
            layer_index,
            layers,
            original_node_id,
            inputs,
        );
    }

    // Same-store wrapper: structural expansion path (existing behavior).
    let wrapper_template_id = layer
        .wrapper_reference
        .template_id_in_store(store.store_id())
        .ok_or_else(|| {
            internal_compiler_error(
                "TIR head-chain composition: effective wrapper reference is not in the current store while resolving a layer.",
            )
        })?;

    let resolved_fill_node_ids = resolve_tir_chain_items(store, &layer.fill_items, layers, inputs)?;

    let fill_template_id =
        build_tir_fill_template(store, resolved_fill_node_ids, original_node_id)?;

    let routed = super::contributions::route_tir_slot_contributions(
        store,
        wrapper_template_id,
        fill_template_id,
        inputs.string_table,
    )?;

    // Decide between structural expansion and runtime slot planning. When the
    // fill content contains non-const-evaluable (runtime) nodes — such as
    // asset references, dynamic expressions, or loop control — the wrapper must
    // lower as a runtime slot plan so the HIR preserves slot-site boundaries
    // and loop-control semantics. Structural expansion would flatten wrapper
    // text and fill content together, which breaks `continue` inside slot
    // fills and drops runtime slot-site metadata.
    let schema = collect_tir_slot_schema(store, wrapper_template_id)?;

    let composed_template_id = if inputs.allow_runtime_plans
        && tir_contributions_need_runtime(
            &schema,
            &routed.contributions,
            inputs.string_table,
            store,
        ) {
        let original_location = store
            .get_node(original_node_id)
            .map(|node| node.location.to_owned())
            .ok_or_else(|| {
                internal_compiler_error(
                    "TIR head-chain composition: original wrapper node ID was not present in the store.",
                )
            })?;

        materialize_tir_native_runtime_slot_plan(
            store,
            wrapper_template_id,
            &routed,
            inputs.string_table,
            &original_location,
        )
        .map_err(|error| TemplateError::from(error).into_diagnostic())?
    } else {
        // Const-evaluable fill: structurally expand slot placeholders into the
        // wrapper tree. This is the compile-time composition path that produces
        // a fully flattened render tree with no runtime slot-plan metadata.
        let expanded_root = expand_tir_slot_placeholders_into(
            store,
            wrapper_template_id,
            &routed,
            inputs.string_table,
            inputs.slot_compositions,
        )?;

        build_composed_wrapper_template(store, wrapper_template_id, expanded_root)?
    };

    // Record the wrapper/fill pair so the registry-level entry point can
    // allocate a slot-resolution overlay after the store borrow is released.
    // The fill template persists in the store, so the overlay path can
    // re-route against it without re-discovering the chain graph.
    let fill_reference = store.qualify_template_ref(fill_template_id);
    inputs
        .slot_compositions
        .push(SlotResolutionComposition::new(
            layer.wrapper_reference,
            fill_reference,
        ));

    let original_node = store.get_node(original_node_id).ok_or_else(|| {
        internal_compiler_error(
            "TIR head-chain composition: original wrapper node ID was not present in the store.",
        )
    })?;

    let original_location = original_node.location.to_owned();

    let occurrence_id = store.next_child_template_occurrence_id();
    let reference = TemplateTirChildReference::same_store(
        composed_template_id,
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    Ok(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
        },
        original_location,
    )))
}

/// Resolves a cross-store chain layer using the overlay-only path.
///
/// WHAT: when the head-origin wrapper template lives in a foreign store, the
///       wrapper tree is not copied into the composition store. Instead, fill
///       content is routed against the foreign wrapper's slot schema, a
///       slot-resolution overlay is allocated on the registry, and the composed
///       `ChildTemplate` node carries the foreign wrapper reference plus the
///       overlay set. The fold path resolves slots in the wrapper's view
///       context through this overlay.
///
/// WHY: "do not eagerly clone/copy the foreign wrapper tree" means structural
///      expansion (which reads and pushes nodes in one store) cannot work
///      cross-store. The overlay path records slot-to-fill mappings as data so
///      `TirView` can resolve slots without structural mutation. This keeps
///      fill nodes and derived nodes in the composition store while preserving
///      the wrapper's root, phase, and overlay identity.
fn resolve_cross_store_tir_chain_layer(
    store: &mut TemplateIrStore,
    layer_index: usize,
    layers: &[TirChainLayer],
    original_node_id: TemplateIrNodeId,
    inputs: &mut HeadChainResolutionInputs,
) -> HeadChainResult<TemplateIrNodeId> {
    let layer = &layers[layer_index];
    let Some(registry) = inputs.registry else {
        return Err(Box::new(internal_compiler_error(
            "TIR head-chain composition: cross-store wrapper reference requires a registry, but none is available.",
        )));
    };

    // Resolve fill items recursively, threading the caller's real
    // `slot_compositions` and `allow_runtime_plans` so nested same-store and
    // cross-store layers inherit the same composition state as the parent.
    let resolved_fill_node_ids = resolve_tir_chain_items(store, &layer.fill_items, layers, inputs)?;

    // Build a fill template in the composition store from the resolved items.
    let fill_template_id =
        build_tir_fill_template(store, resolved_fill_node_ids, original_node_id)?;

    // Read the wrapper's slot schema from its owning (foreign) store. The
    // immutable registry borrow is released before any mutable registry borrow
    // so the two never overlap.
    let wrapper_store_id = layer.wrapper_reference.root.store_id;
    let wrapper_template_id = layer.wrapper_reference.root.template_id;

    let schema = {
        let registry_borrow = registry.borrow();
        let foreign_store_handle = registry_borrow
            .store_handle(wrapper_store_id)
            .ok_or_else(|| {
                internal_compiler_error(
                    "TIR head-chain composition: cross-store wrapper store was not present in the registry.",
                )
            })?;
        let foreign_store = foreign_store_handle.borrow();
        collect_tir_slot_schema(&foreign_store, wrapper_template_id)?
    };

    if !schema.has_any_slots() {
        return Err(Box::new(internal_compiler_error(
            "TIR head-chain composition: cross-store wrapper has no declared slots, so it should not have been identified as a receiver.",
        )));
    }

    // Route fill content from the composition store against the foreign schema.
    // This shares one fill-walking owner with the same-store path, so
    // slot/insert traversal is not duplicated.
    let routed =
        route_tir_fill_against_schema(store, &schema, fill_template_id, inputs.string_table)?;

    // Read slot placeholders from the foreign wrapper's tree so the overlay
    // carries the wrapper's own occurrence IDs. The fill source templates are
    // built in the composition store.
    let placeholders: Vec<TirSlotPlaceholder> = {
        let registry_borrow = registry.borrow();
        let foreign_store_handle = registry_borrow
            .store_handle(wrapper_store_id)
            .ok_or_else(|| {
                internal_compiler_error(
                    "TIR head-chain composition: cross-store wrapper store was not present in the registry.",
                )
            })?;
        let foreign_store = foreign_store_handle.borrow();
        let root_node_id = root_node_id_for_template(&foreign_store, wrapper_template_id)?;
        collect_tir_slot_placeholders_in_order(&foreign_store, root_node_id)?
    };

    let resolutions = build_slot_resolution_entries(store, placeholders, &routed)?;
    let overlay = TirSlotResolutionOverlay { resolutions };

    // Check whether the wrapper carries pre-existing overlay dimensions
    // (expression overrides, wrapper context) that must survive composition.
    // The immutable registry borrow is released before the mutable borrow for
    // overlay allocation so the two never overlap.
    let wrapper_overlay_set_id = layer.wrapper_reference.overlay_set_id;
    let has_existing_overlays = {
        let registry_borrow = registry.borrow();
        registry_borrow
            .overlay_set(wrapper_overlay_set_id)
            .is_some_and(|set| !set.is_empty())
    };

    // Allocate the slot-resolution overlay and compose it with the wrapper's
    // pre-existing overlay set. When the wrapper had no pre-existing overlays,
    // the slot-only set is already complete. Otherwise, merge preserves
    // expression overrides and wrapper context while adding slot resolution.
    let overlay_set_id = {
        let mut registry_borrow = registry.borrow_mut();
        let slot_overlay_id = registry_borrow.allocate_slot_resolution_overlay(overlay);
        let slot_only_set_id = registry_borrow.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: Some(slot_overlay_id),
            wrapper_context: None,
        });

        if has_existing_overlays {
            merge_tir_slot_resolution_overlay_sets(
                &mut registry_borrow,
                wrapper_overlay_set_id,
                slot_only_set_id,
            )?
        } else {
            slot_only_set_id
        }
    };

    // Create a composed ChildTemplate node that carries the foreign wrapper
    // reference with the merged overlay set. The fold path will resolve slots
    // through this overlay in the wrapper's view context while expression
    // overrides and wrapper context from the original wrapper survive.
    let original_node = store.get_node(original_node_id).ok_or_else(|| {
        internal_compiler_error(
            "TIR head-chain composition: original wrapper node ID was not present in the store.",
        )
    })?;

    let original_location = original_node.location.to_owned();
    let occurrence_id = store.next_child_template_occurrence_id();
    let composed_reference = TemplateTirChildReference::new(
        layer.wrapper_reference.root,
        layer.wrapper_reference.phase,
        overlay_set_id,
    );

    Ok(store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: composed_reference,
            occurrence_id,
        },
        original_location,
    )))
}
