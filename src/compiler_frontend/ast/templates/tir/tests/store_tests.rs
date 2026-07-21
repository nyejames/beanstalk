use super::super::builder::TemplateIrBuilder;
use super::super::ids::TemplateIrId;
use super::super::node::{TemplateIr, TemplateIrNode, TemplateIrNodeKind};
use super::super::overlays::TemplateViewContext;
use super::super::refs::TemplateWrapperReference;
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
fn push_returns_sequential_ids_per_collection() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    // Nodes allocate sequential TemplateIrNodeIds from their own index space.
    let node_a = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("abc"),
            byte_len: 3,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));
    let node_b = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    assert_eq!(node_a.index(), 0);
    assert_eq!(node_b.index(), 1);
    assert_eq!(store.node_count(), 2);

    // Templates allocate sequential TemplateIrIds from a separate index space.
    let template_a = store.push_template(TemplateIr::new(
        node_a,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary::default(),
        empty_location(),
    ));
    let template_b = store.push_template(TemplateIr::new(
        node_a,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));
    assert_eq!(template_a.index(), 0);
    assert_eq!(template_b.index(), 1);
    assert_eq!(store.template_count(), 2);
}

#[test]
fn typed_retrieval_returns_stored_entry() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();

    // Template: round-trips the root node id through get_template.
    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern(""),
            byte_len: 0,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));
    let retrieved_template = store
        .get_template(template_id)
        .expect("template should exist");
    assert_eq!(retrieved_template.root, node_id);

    // Node: round-trips the exact node kind through get_node.
    let sequence_node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let retrieved_node = store.get_node(sequence_node_id).expect("node should exist");
    assert!(matches!(
        retrieved_node.kind,
        TemplateIrNodeKind::Sequence { .. }
    ));

    // Wrapper set: round-trips effective wrapper refs through get_wrapper_set.
    let wrapper_root = build_finalized_tir_template(&mut store);
    let wrapper_set_id = store.push_wrapper_set(super::super::store::TemplateWrapperSet {
        wrappers: vec![TemplateWrapperReference::new(
            wrapper_root,
            TemplateTirPhase::Finalized,
            TemplateViewContext::default(),
        )],
    });
    let retrieved_wrapper_set = store
        .get_wrapper_set(wrapper_set_id)
        .expect("wrapper set should exist");
    assert_eq!(retrieved_wrapper_set.wrappers.len(), 1);
    assert_eq!(retrieved_wrapper_set.wrappers[0].root, wrapper_root);

    // Slot plan: round-trips the routing plan through get_slot_plan.
    let slot_plan_id = store.push_slot_plan(runtime_slot_plan());
    let retrieved_slot_plan = store
        .get_slot_plan(slot_plan_id)
        .expect("slot plan should exist");
    assert_eq!(retrieved_slot_plan.location, empty_location());
    assert!(retrieved_slot_plan.contribution_sources.is_empty());
    assert_eq!(retrieved_slot_plan.slot_sites.len(), 1);
    assert_eq!(retrieved_slot_plan.slot_sites[0].site, RuntimeSlotSiteId(0));
    assert_eq!(retrieved_slot_plan.slot_sites[0].key, SlotKey::Default);
    assert!(
        retrieved_slot_plan.slot_sites[0]
            .render_plan
            .pieces
            .is_empty()
    );
    assert_eq!(retrieved_slot_plan.slot_sites[0].location, empty_location());
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
        wrapper_id,
        TemplateTirPhase::Finalized,
        TemplateViewContext::default(),
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
        template_id,
        TemplateTirPhase::Finalized,
        TemplateViewContext::default(),
    )]);
    let id_b = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        template_id,
        TemplateTirPhase::Finalized,
        TemplateViewContext::default(),
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
        wrapper_a,
        TemplateTirPhase::Finalized,
        TemplateViewContext::default(),
    )]);
    let id_b = store.push_or_reuse_wrapper_set(vec![TemplateWrapperReference::new(
        wrapper_b,
        TemplateTirPhase::Finalized,
        TemplateViewContext::default(),
    )]);

    assert_ne!(
        id_a, id_b,
        "wrapper sets referencing different TemplateIrIds should not reuse"
    );
    assert_eq!(store.wrapper_sets.len(), 2);
}
