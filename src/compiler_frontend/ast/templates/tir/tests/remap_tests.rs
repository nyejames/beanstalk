//! TIR string-table remap tests.
//!
//! WHAT: verifies that `TemplateIrStore::remap_string_ids` rewrites every
//! interned string identity stored inside TIR templates, nodes, wrapper sets,
//! and slot plans without touching store-local typed IDs.
//! WHY: per-file frontend string-table merges require all AST-local template
//! state to be remapped before module-wide compilation continues.

use super::super::builder::TemplateIrBuilder;
use super::super::ids::TemplateIrId;
use super::super::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
use super::super::refs::TemplateWrapperReference;
use super::super::slot_plan::{
    TemplateSlotContributionSourcePlan, TemplateSlotPlan, TemplateSlotSitePlan,
    TemplateSlotSiteRenderPiece, TemplateSlotSiteRenderPlan,
};
use super::super::store::{TemplateIrStore, TemplateWrapperSet};
use super::super::summary::TemplateIrSummary;
use super::super::{TemplateOverlaySetId, TemplateTirChildReference, TemplateTirPhase};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    RuntimeSlotContributionSourceId, RuntimeSlotSiteId,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use std::path::Path;

/// Builds a non-identity remap where every source ID is rewritten to a different
/// target ID.
///
/// Source order: path, text, slot-name.
/// Target order: slot-name, path, text.
fn build_non_identity_remap() -> (StringIdRemap, StringTable, StringTable, RemapFixtures) {
    let mut source_table = StringTable::new();
    source_table.intern("path");
    let text_id = source_table.intern("text");
    let slot_name_id = source_table.intern("slot-name");
    source_table.intern("");

    let mut target_table = StringTable::new();
    target_table.intern("slot-name");
    target_table.intern("path");
    target_table.intern("text");
    target_table.intern("");

    let remap = target_table.merge_from(&source_table);

    (
        remap,
        source_table,
        target_table,
        RemapFixtures {
            text_id,
            slot_name_id,
        },
    )
}

struct RemapFixtures {
    text_id: StringId,
    slot_name_id: StringId,
}

fn source_location_with_path(path: &str, string_table: &mut StringTable) -> SourceLocation {
    SourceLocation::from_path(Path::new(path), string_table)
}

fn text_node(text: StringId, location: SourceLocation) -> TemplateIrNode {
    TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text,
            byte_len: 4,
            origin: TemplateSegmentOrigin::Body,
        },
        location,
    )
}

fn empty_text_node(string_table: &mut StringTable) -> TemplateIrNode {
    let empty_id = string_table.intern("");
    TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: empty_id,
            byte_len: 0,
            origin: TemplateSegmentOrigin::Body,
        },
        SourceLocation::default(),
    )
}

fn named_slot_key_name<'a>(key: &'a SlotKey, string_table: &'a StringTable) -> Option<&'a str> {
    match key {
        SlotKey::Named(id) => Some(string_table.resolve(*id)),
        _ => None,
    }
}

fn build_wrapper_template_with_text(
    store: &mut TemplateIrStore,
    text_id: StringId,
    location: SourceLocation,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_text_node(text_id, 4, TemplateSegmentOrigin::Body, location.clone());
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        location,
    )
}

#[test]
fn store_remap_is_no_op_for_identity_remap() {
    let mut store = TemplateIrStore::new();
    let mut string_table = StringTable::new();
    let text_id = string_table.intern("text");
    let location = SourceLocation::from_path(Path::new("path"), &mut string_table);

    let node_id = store.push_node(text_node(text_id, location.clone()));
    let template_id = store.push_template(TemplateIr::new(
        node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        location.clone(),
    ));

    let identity_source = string_table.clone();
    let remap = string_table.merge_from(&identity_source);
    assert!(remap.is_identity());

    store.remap_string_ids(&remap);

    let template = store
        .get_template(template_id)
        .expect("template should exist");
    assert_eq!(template.location, location);

    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::Text { text, .. } => assert_eq!(*text, text_id),
        other => panic!("expected Text node, got {other:?}"),
    }
}

#[test]
fn store_remaps_template_location_and_kind() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);

    let root = store.push_node(empty_text_node(&mut source_table));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::SlotDefinition(SlotKey::Named(fixtures.slot_name_id)),
        TemplateIrSummary::default(),
        source_location.clone(),
    ));

    store.remap_string_ids(&remap);

    let template = store
        .get_template(template_id)
        .expect("template should exist");
    assert_ne!(template.location, source_location);
    assert_eq!(
        template.location.scope.name_str(&target_table),
        Some("path")
    );

    match &template.kind {
        TemplateType::SlotDefinition(SlotKey::Named(id)) => {
            assert_eq!(target_table.resolve(*id), "slot-name");
        }
        other => panic!("expected SlotDefinition with named key, got {other:?}"),
    }
}

#[test]
fn store_remaps_text_node_string_id_and_location() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);
    let node_id = store.push_node(text_node(fixtures.text_id, source_location.clone()));

    store.remap_string_ids(&remap);

    let node = store.get_node(node_id).expect("node should exist");
    assert_ne!(node.location, source_location);
    assert_eq!(node.location.scope.name_str(&target_table), Some("path"));

    match &node.kind {
        TemplateIrNodeKind::Text { text, .. } => {
            assert_eq!(target_table.resolve(*text), "text");
        }
        other => panic!("expected Text node, got {other:?}"),
    }
}

#[test]
fn store_remaps_dynamic_expression_payload() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);
    let expression = Expression::string_slice(
        fixtures.text_id,
        source_location.clone(),
        ValueMode::ImmutableOwned,
    );
    let site_id = store.next_expression_site_id();
    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id,
        },
        source_location,
    ));

    store.remap_string_ids(&remap);

    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::DynamicExpression { expression, .. } => {
            assert_eq!(expression.as_string(&target_table), "text".to_string());
        }
        other => panic!("expected DynamicExpression node, got {other:?}"),
    }
}

#[test]
fn store_remaps_slot_placeholder_key() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);
    let occurrence_id = store.next_slot_occurrence_id();
    let placeholder = TirSlotPlaceholder::new(
        SlotKey::Named(fixtures.slot_name_id),
        occurrence_id,
        source_location.clone(),
    );
    let node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Slot { placeholder },
        source_location,
    ));

    store.remap_string_ids(&remap);

    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::Slot { placeholder } => {
            assert_eq!(
                named_slot_key_name(&placeholder.key, &target_table),
                Some("slot-name")
            );
        }
        other => panic!("expected Slot node, got {other:?}"),
    }
}

#[test]
fn store_remaps_branch_selector_and_loop_header() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);
    let condition_location = source_location_with_path("path", &mut source_table);
    let condition = Expression::string_slice(
        fixtures.text_id,
        condition_location,
        ValueMode::ImmutableOwned,
    );

    let body = store.push_node(empty_text_node(&mut source_table));
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(condition),
        body,
        source_location.clone(),
    )
    .with_selector_site_id(store.next_expression_site_id());
    let branch_node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch],
            fallback: None,
        },
        source_location.clone(),
    ));

    let loop_condition = Expression::string_slice(
        fixtures.text_id,
        source_location.clone(),
        ValueMode::ImmutableOwned,
    );
    let loop_body = store.push_node(empty_text_node(&mut source_table));
    let loop_header = TemplateLoopHeader::Conditional {
        condition: Box::new(loop_condition),
    };
    let loop_header_sites = store.allocate_loop_header_expression_sites(&loop_header);
    let loop_node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Loop {
            header: loop_header,
            header_sites: loop_header_sites,
            body: loop_body,
            aggregate_wrapper: None,
        },
        source_location,
    ));

    store.remap_string_ids(&remap);

    let branch_node = store
        .get_node(branch_node_id)
        .expect("branch node should exist");
    match &branch_node.kind {
        TemplateIrNodeKind::BranchChain { branches, .. } => {
            let branch = branches.first().expect("branch should exist");
            assert_eq!(branch.location.scope.name_str(&target_table), Some("path"));
            assert_eq!(
                branch.condition_expression().as_string(&target_table),
                "text".to_string()
            );
        }
        other => panic!("expected BranchChain node, got {other:?}"),
    }

    let loop_node = store
        .get_node(loop_node_id)
        .expect("loop node should exist");
    match &loop_node.kind {
        TemplateIrNodeKind::Loop { header, .. } => match header {
            TemplateLoopHeader::Conditional { condition } => {
                assert_eq!(condition.as_string(&target_table), "text".to_string());
            }
            other => panic!("expected conditional loop header, got {other:?}"),
        },
        other => panic!("expected Loop node, got {other:?}"),
    }
}

#[test]
fn store_preserves_wrapper_set_refs_and_remaps_underlying_templates() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let wrapper_id = build_wrapper_template_with_text(
        &mut store,
        fixtures.text_id,
        source_location_with_path("path", &mut source_table),
    );
    let wrapper_ref = store.qualify_template_ref(wrapper_id);
    let wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
        wrappers: vec![TemplateWrapperReference::new(
            wrapper_ref,
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
    });

    store.remap_string_ids(&remap);

    let wrapper_set = store
        .get_wrapper_set(wrapper_set_id)
        .expect("wrapper set should exist");
    assert_eq!(wrapper_set.wrappers.len(), 1);
    assert_eq!(wrapper_set.wrappers[0].root, wrapper_ref);

    let wrapper_template = store
        .get_template(wrapper_id)
        .expect("wrapper template should exist");
    let root_node = store
        .get_node(wrapper_template.root)
        .expect("wrapper root node should exist");

    // Verify the wrapper's interned text identity was remapped through the
    // store-level walk.
    let text_id = match &root_node.kind {
        TemplateIrNodeKind::Text { text, .. } => *text,
        other => panic!("expected Text wrapper root, got {other:?}"),
    };
    assert_eq!(
        target_table.resolve(text_id),
        "text",
        "wrapper text should be remapped to the target table"
    );
}

#[test]
fn store_remaps_slot_plan_location_and_keys() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);
    let render_root = store.push_node(empty_text_node(&mut source_table));

    let slot_plan = TemplateSlotPlan {
        location: source_location.clone(),
        contribution_sources: vec![TemplateSlotContributionSourcePlan {
            source: RuntimeSlotContributionSourceId(0),
            target: SlotKey::Named(fixtures.slot_name_id),
            render_root,
            renders_wrapper_unconditionally: false,
            location: source_location.clone(),
        }],
        slot_sites: vec![TemplateSlotSitePlan {
            site: RuntimeSlotSiteId(0),
            key: SlotKey::Named(fixtures.slot_name_id),
            render_plan: TemplateSlotSiteRenderPlan {
                pieces: vec![
                    TemplateSlotSiteRenderPiece::Render(render_root),
                    TemplateSlotSiteRenderPiece::ContributionSource(
                        RuntimeSlotContributionSourceId(0),
                    ),
                ],
            },
            location: source_location,
        }],
    };
    let slot_plan_id = store.push_slot_plan(slot_plan.clone());

    store.remap_string_ids(&remap);

    let slot_plan = store
        .get_slot_plan(slot_plan_id)
        .expect("slot plan should exist");
    assert_eq!(
        slot_plan.location.scope.name_str(&target_table),
        Some("path")
    );

    assert_eq!(slot_plan.contribution_sources.len(), 1);
    assert_eq!(
        named_slot_key_name(&slot_plan.contribution_sources[0].target, &target_table),
        Some("slot-name")
    );
    assert_eq!(
        slot_plan.contribution_sources[0]
            .location
            .scope
            .name_str(&target_table),
        Some("path")
    );

    assert_eq!(slot_plan.slot_sites.len(), 1);
    assert_eq!(
        named_slot_key_name(&slot_plan.slot_sites[0].key, &target_table),
        Some("slot-name")
    );
    assert_eq!(
        slot_plan.slot_sites[0]
            .location
            .scope
            .name_str(&target_table),
        Some("path")
    );

    // Store-local IDs must be unchanged.
    assert_eq!(
        slot_plan.contribution_sources[0].source,
        RuntimeSlotContributionSourceId(0)
    );
    assert_eq!(slot_plan.slot_sites[0].site, RuntimeSlotSiteId(0));
    assert_eq!(slot_plan.slot_sites[0].render_plan.pieces.len(), 2);
}

#[test]
fn store_remap_visits_all_templates_and_nodes() {
    let (remap, mut source_table, target_table, fixtures) = build_non_identity_remap();
    let mut store = TemplateIrStore::new();

    let source_location = source_location_with_path("path", &mut source_table);

    // Child template with its own text node.
    let child_text = store.push_node(text_node(fixtures.text_id, source_location.clone()));
    let child_template_id = store.push_template(TemplateIr::new(
        child_text,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        source_location.clone(),
    ));

    // Parent template references the child.
    let occurrence_id = store.next_child_template_occurrence_id();
    let parent_reference = TemplateTirChildReference::same_store(
        child_template_id,
        store.store_id(),
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );
    let parent_node_id = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: parent_reference,
            occurrence_id,
        },
        source_location.clone(),
    ));
    let parent_template_id = store.push_template(TemplateIr::new(
        parent_node_id,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        source_location,
    ));

    store.remap_string_ids(&remap);

    let parent_template = store
        .get_template(parent_template_id)
        .expect("parent template should exist");
    assert_eq!(
        parent_template.location.scope.name_str(&target_table),
        Some("path")
    );

    let child_template = store
        .get_template(child_template_id)
        .expect("child template should exist");
    assert_eq!(
        child_template.location.scope.name_str(&target_table),
        Some("path")
    );

    let child_node = store
        .get_node(child_text)
        .expect("child text node should exist");
    match &child_node.kind {
        TemplateIrNodeKind::Text { text, .. } => {
            assert_eq!(target_table.resolve(*text), "text");
        }
        other => panic!("expected Text node, got {other:?}"),
    }
}
