use super::super::builder::TemplateIrBuilder;
use super::super::ids::TemplateIrId;
use super::super::node::{TemplateIr, TemplateIrNode, TemplateIrNodeKind};
use super::super::overlays::TemplateOverlaySetId;
use super::super::refs::{TemplateStoreId, TemplateWrapperReference};
use super::super::slot_plan::{TemplateSlotPlan, TemplateSlotSitePlan, TemplateSlotSiteRenderPlan};
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use std::sync::Arc;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn build_finalized_tir_template(store: &mut TemplateIrStore) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_sequence_node(vec![], empty_location());
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

fn runtime_slot_plan() -> TemplateSlotPlan {
    TemplateSlotPlan {
        location: empty_location(),
        contribution_sources: vec![],
        slot_sites: vec![TemplateSlotSitePlan {
            site: RuntimeSlotSiteId(0),
            key: SlotKey::Default,
            render_plan: TemplateSlotSiteRenderPlan::default(),
            location: empty_location(),
        }],
    }
}

#[test]
fn store_starts_empty() {
    let store = TemplateIrStore::new();
    assert_eq!(store.template_count(), 0);
    assert_eq!(store.node_count(), 0);
}

#[test]
fn detached_snapshot_keeps_data_but_uses_fresh_owner() {
    let mut store = TemplateIrStore::new();
    let template_id = build_finalized_tir_template(&mut store);

    let snapshot = store.detached_snapshot();

    assert_eq!(snapshot.template_count(), store.template_count());
    assert!(snapshot.get_template(template_id).is_some());
    assert!(
        !Arc::ptr_eq(&snapshot.owner(), &store.owner()),
        "detached snapshots must not pass same-store owner checks"
    );
}

#[test]
fn push_template_returns_sequential_ids() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("test"),
            byte_len: 4,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    let id_a = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));
    let id_b = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    assert_eq!(id_a.index(), 0);
    assert_eq!(id_b.index(), 1);
    assert_eq!(store.template_count(), 2);
}

#[test]
fn push_node_returns_sequential_ids() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let id_a = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("abc"),
            byte_len: 3,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));
    let id_b = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));

    assert_eq!(id_a.index(), 0);
    assert_eq!(id_b.index(), 1);
    assert_eq!(store.node_count(), 2);
}

#[test]
fn get_template_returns_stored_entry() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern(""),
            byte_len: 0,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    let id = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let retrieved = store.get_template(id).expect("template should exist");
    assert_eq!(retrieved.root, node_id);
}

#[test]
fn get_node_returns_stored_entry() {
    let mut store = TemplateIrStore::new();

    let id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));

    let retrieved = store.get_node(id).expect("node should exist");
    assert!(matches!(
        retrieved.kind,
        TemplateIrNodeKind::Sequence { .. }
    ));
}

#[test]
fn out_of_bounds_lookup_returns_none() {
    let store = TemplateIrStore::new();
    assert!(
        store
            .get_template(super::super::ids::TemplateIrId::new(99))
            .is_none()
    );
    assert!(
        store
            .get_node(super::super::ids::TemplateIrNodeId::new(99))
            .is_none()
    );
    assert!(
        store
            .get_wrapper_set(super::super::ids::TemplateWrapperSetId::new(99))
            .is_none()
    );
    assert!(
        store
            .get_slot_plan(super::super::ids::TemplateSlotPlanId::new(99))
            .is_none()
    );
}

#[test]
fn get_wrapper_set_returns_stored_entry() {
    let mut store = TemplateIrStore::new();

    let wrapper_id = build_finalized_tir_template(&mut store);
    let wrapper_ref = store.qualify_template_ref(wrapper_id);
    let id = store.push_wrapper_set(super::super::store::TemplateWrapperSet {
        wrappers: vec![TemplateWrapperReference::new(
            wrapper_ref,
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
    });

    let retrieved = store.get_wrapper_set(id).expect("wrapper set should exist");
    assert_eq!(retrieved.wrappers.len(), 1);
    assert_eq!(retrieved.wrappers[0].root, wrapper_ref);
}

#[test]
fn get_slot_plan_returns_stored_entry() {
    let mut store = TemplateIrStore::new();

    let id = store.push_slot_plan(runtime_slot_plan());

    let retrieved = store.get_slot_plan(id).expect("slot plan should exist");
    assert_eq!(retrieved.location, empty_location());
    assert_eq!(retrieved.slot_sites.len(), 1);
}

#[test]
fn push_or_reuse_wrapper_set_reuses_equivalent_empty_set() {
    let mut store = TemplateIrStore::new();

    let id_a = store.push_or_reuse_wrapper_set(vec![]);
    let id_b = store.push_or_reuse_wrapper_set(vec![]);

    assert_eq!(id_a, id_b, "empty wrapper vectors should be reused");
    assert_eq!(store.wrapper_sets.len(), 1);
}

#[test]
fn push_or_reuse_wrapper_set_creates_new_for_different_lengths() {
    let mut store = TemplateIrStore::new();

    let wrapper_id = build_finalized_tir_template(&mut store);

    let id_a = store.push_or_reuse_wrapper_set(vec![]);
    let id_b = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        store.qualify_template_ref(wrapper_id),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )]);

    assert_ne!(
        id_a, id_b,
        "wrapper sets with different lengths should not be reused"
    );
    assert_eq!(store.wrapper_sets.len(), 2);
}

#[test]
fn push_or_reuse_wrapper_set_reuses_same_template_id() {
    let mut store = TemplateIrStore::new();
    let template_id = build_finalized_tir_template(&mut store);

    let id_a = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        store.qualify_template_ref(template_id),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )]);
    let id_b = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        store.qualify_template_ref(template_id),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )]);

    assert_eq!(
        id_a, id_b,
        "wrapper sets referencing the same TemplateIrId should reuse one wrapper set"
    );
    assert_eq!(store.wrapper_sets.len(), 1);
}

#[test]
fn push_or_reuse_wrapper_set_does_not_reuse_different_template_ids() {
    let mut store = TemplateIrStore::new();

    let wrapper_a = build_finalized_tir_template(&mut store);
    let wrapper_b = build_finalized_tir_template(&mut store);

    let id_a = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        store.qualify_template_ref(wrapper_a),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )]);
    let id_b = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        store.qualify_template_ref(wrapper_b),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )]);

    assert_ne!(
        id_a, id_b,
        "wrapper sets referencing different TemplateIrIds should not reuse"
    );
    assert_eq!(store.wrapper_sets.len(), 2);
}

// -------------------------
//  Store-Qualified Wrapper Ref Tests
// -------------------------

#[test]
fn push_or_reuse_wrapper_set_stores_store_qualified_refs() {
    let mut store = TemplateIrStore::new();

    let wrapper_id = build_finalized_tir_template(&mut store);
    let set_id = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        store.qualify_template_ref(wrapper_id),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )]);

    let wrapper_set = store
        .get_wrapper_set(set_id)
        .expect("wrapper set should exist");

    // Each entry must be a store-qualified TemplateRef, not a bare TemplateIrId.
    assert_eq!(wrapper_set.wrappers.len(), 1);
    let reference = &wrapper_set.wrappers[0];
    assert_eq!(reference.root.store_id, store.store_id());
    assert_eq!(reference.root.template_id, wrapper_id);
}

#[test]
fn qualify_template_ref_pairs_store_id_with_template_id() {
    let store = TemplateIrStore::new();

    let template_id = TemplateIrId::new(3);
    let reference = store.qualify_template_ref(template_id);

    assert_eq!(reference.store_id, store.store_id());
    assert_eq!(reference.template_id, template_id);
}

#[test]
fn store_id_defaults_to_zero_for_directly_constructed_stores() {
    let store = TemplateIrStore::new();
    assert_eq!(store.store_id(), TemplateStoreId::new(0));
}

#[test]
fn set_store_id_stamps_registry_assigned_id() {
    let mut store = TemplateIrStore::new();
    let new_id = TemplateStoreId::new(7);
    store.set_store_id(new_id);

    assert_eq!(store.store_id(), new_id);

    // Qualifying a template ref should use the new store ID.
    let reference = store.qualify_template_ref(TemplateIrId::new(0));
    assert_eq!(reference.store_id, new_id);
}
