//! Tests for TIR-native slot schema extraction, contribution routing, and
//! placeholder expansion.
//!
//! WHAT: exercises `collect_tir_slot_schema`, `route_tir_slot_contributions`,
//!       and `expand_tir_slot_placeholders` over a variety of TIR tree shapes
//!       (sequences, nested child templates, branch chains, loops) and verifies
//!       the query methods on `TirSlotSchema` and `TirSlotContributions`.
//! WHY: slot composition depends on discovering declared slot targets
//!      directly from TIR nodes, routing fill content into the right buckets,
//!      and expanding placeholders from TIR nodes; these tests pin that
//!      behavior in isolation.

use super::super::builder::TemplateIrBuilder;
use super::super::ids::SlotOccurrenceId;
use super::super::ids::{TemplateIrId, TemplateIrNodeId};
use super::super::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
use super::super::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirSlotResolutionKind,
    TirSlotResolutionOverlay,
};
use super::super::refs::{TemplateRef, TemplateStoreId, TemplateTirChildReference};
use super::super::registry::TemplateIrRegistry;
use super::super::slot_composition::{
    RoutedTirSlotContributions, TirSlotContributions, TirSlotSchema, apply_tir_child_wrappers,
    apply_tir_child_wrappers_with_overlays, attach_tir_slot_resolution_overlay,
    collect_tir_slot_schema, compose_tir_head_chain, compose_tir_head_chain_with_overlays,
    compose_tir_slot_resolution_overlay_set, expand_tir_slot_placeholders,
    materialize_tir_slot_resolution_overlay, route_tir_slot_contributions,
};
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticPayload, InvalidTemplateSlotReason,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

use std::cell::RefCell;
use std::rc::Rc;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

/// Builds a summary that records the given number of slot placeholders.
///
/// WHAT: composition uses `TemplateIrSummary::slot_count` as a cheap guard to
///       decide whether a child template is a wrapper receiver. Tests that
///       build wrapper templates by hand must supply a summary with accurate
///       slot metadata, otherwise the receiver fast path will skip them.
fn slot_summary(slot_count: u32) -> TemplateIrSummary {
    TemplateIrSummary {
        slot_count,
        has_slots: slot_count > 0,
        ..TemplateIrSummary::default()
    }
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

fn build_single_slot_template(store: &mut TemplateIrStore, key: SlotKey) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let slot_node = builder.push_slot_node(key, empty_location());
    builder.finish_template(
        slot_node,
        Style::default(),
        TemplateType::String,
        slot_summary(1),
        empty_location(),
    )
}

fn build_slot_insert_template(
    store: &mut TemplateIrStore,
    target: SlotKey,
    string_table: &mut StringTable,
) -> TemplateIrId {
    let text_id = string_table.intern("insert");
    let byte_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);

    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_text_node(
        text_id,
        byte_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::SlotInsert(target),
        TemplateIrSummary::default(),
        empty_location(),
    )
}

fn build_text_node(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
    origin: TemplateSegmentOrigin,
) -> TemplateIrNodeId {
    let text_id = string_table.intern(text);
    let byte_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);

    let mut builder = TemplateIrBuilder::new(store);
    builder.push_text_node(text_id, byte_len, origin, empty_location())
}

fn build_child_template_node(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrNodeId {
    let text_id = string_table.intern("child");
    let byte_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);

    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_text_node(
        text_id,
        byte_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let child_template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    builder.push_child_template_node(child_template_id, empty_location())
}

fn build_fill_template(store: &mut TemplateIrStore, nodes: Vec<TemplateIrNodeId>) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_sequence_node(nodes, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

/// Builds a template whose root is a sequence of the given child nodes.
fn build_template_with_children(
    store: &mut TemplateIrStore,
    children: Vec<TemplateIrNodeId>,
) -> TemplateIrId {
    let child_template_count = children
        .iter()
        .filter(|&&child_id| {
            store
                .get_node(child_id)
                .is_some_and(|node| matches!(node.kind, TemplateIrNodeKind::ChildTemplate { .. }))
        })
        .count() as u32;

    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_sequence_node(children, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary {
            child_template_count,
            ..TemplateIrSummary::default()
        },
        empty_location(),
    )
}

/// Builds a `ChildTemplate` node referencing an existing template.
fn build_child_template_node_for_template(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
) -> TemplateIrNodeId {
    let mut builder = TemplateIrBuilder::new(store);
    builder.push_child_template_node(template_id, empty_location())
}

fn build_child_template_node_with_reference(
    store: &mut TemplateIrStore,
    reference: TemplateTirChildReference,
) -> TemplateIrNodeId {
    let mut builder = TemplateIrBuilder::new(store);
    builder.push_child_template_node_with_reference(reference, empty_location())
}

fn build_wrapper_with_slots(store: &mut TemplateIrStore, keys: Vec<SlotKey>) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);

    let slot_nodes: Vec<TemplateIrNodeId> = keys
        .into_iter()
        .map(|key| builder.push_slot_node(key, empty_location()))
        .collect();

    let slot_count = slot_nodes.len() as u32;
    let root = builder.push_sequence_node(slot_nodes, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        slot_summary(slot_count),
        empty_location(),
    )
}

fn assert_invalid_template_slot_reason(
    error: &CompilerDiagnostic,
    expected: InvalidTemplateSlotReason,
) {
    match &error.payload {
        DiagnosticPayload::InvalidTemplateSlot { reason, .. } => {
            assert_eq!(*reason, expected);
        }
        other => panic!("expected InvalidTemplateSlot diagnostic, got {other:?}"),
    }
}

// -------------------------
//  Expansion Test Helpers
// -------------------------

/// Returns the root node ID of a template.
fn template_root_node_id(template_id: TemplateIrId, store: &TemplateIrStore) -> TemplateIrNodeId {
    store
        .get_template(template_id)
        .expect("template should exist in store")
        .root
}

/// Returns the text content of a single-text template's root node.
fn template_root_text(
    template_id: TemplateIrId,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Option<String> {
    text_node_text(
        template_root_node_id(template_id, store),
        store,
        string_table,
    )
}

/// Returns the direct children of a template's root sequence.
fn root_child_node_ids(
    template_id: TemplateIrId,
    store: &TemplateIrStore,
) -> Vec<TemplateIrNodeId> {
    let template_ir = store
        .get_template(template_id)
        .expect("template should exist in store");
    let root = store
        .get_node(template_ir.root)
        .expect("template should have a root node");

    match &root.kind {
        TemplateIrNodeKind::Sequence { children } => children.clone(),
        other => panic!("expected sequence root, found {other:?}"),
    }
}

/// Returns references to the kinds of the root sequence children.
fn root_child_kinds(
    template_id: TemplateIrId,
    store: &TemplateIrStore,
) -> Vec<&TemplateIrNodeKind> {
    root_child_node_ids(template_id, store)
        .into_iter()
        .map(|node_id| {
            &store
                .get_node(node_id)
                .expect("root child node should exist")
                .kind
        })
        .collect()
}

/// Returns the text content of a Text node, if the node is a Text node.
fn text_node_text(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
    string_table: &StringTable,
) -> Option<String> {
    let node = store.get_node(node_id)?;

    match &node.kind {
        TemplateIrNodeKind::Text { text, .. } => Some(string_table.resolve(*text).to_owned()),
        _ => None,
    }
}

/// Builds a wrapper template containing a sequence of the given slot keys.
fn build_wrapper_with_slot_sequence(
    store: &mut TemplateIrStore,
    keys: Vec<SlotKey>,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);

    let slot_nodes: Vec<TemplateIrNodeId> = keys
        .into_iter()
        .map(|key| builder.push_slot_node(key, empty_location()))
        .collect();

    let slot_count = slot_nodes.len() as u32;
    let root = builder.push_sequence_node(slot_nodes, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        slot_summary(slot_count),
        empty_location(),
    )
}

/// Builds a wrapper template with alternating Text and Slot children.
fn build_text_slot_text_wrapper(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    key: SlotKey,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);

    let before_text_id = string_table.intern("before");
    let before_text_len =
        u32::try_from(string_table.resolve(before_text_id).len()).unwrap_or(u32::MAX);
    let before_text = builder.push_text_node(
        before_text_id,
        before_text_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );

    let slot_node = builder.push_slot_node(key, empty_location());

    let after_text_id = string_table.intern("after");
    let after_text_len =
        u32::try_from(string_table.resolve(after_text_id).len()).unwrap_or(u32::MAX);
    let after_text = builder.push_text_node(
        after_text_id,
        after_text_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );

    let root =
        builder.push_sequence_node(vec![before_text, slot_node, after_text], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        slot_summary(1),
        empty_location(),
    )
}

/// Builds a TIR template whose root is a single Text node.
fn build_single_text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let text_id = string_table.intern(text);
    let text_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);
    let root = builder.push_text_node(
        text_id,
        text_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

#[test]
fn schema_from_default_slot_only() {
    let mut store = TemplateIrStore::new();
    let template_id = build_single_slot_template(&mut store, SlotKey::Default);

    let schema = collect_tir_slot_schema(&store, template_id, &StringTable::new())
        .expect("schema extraction should succeed");

    assert!(schema.has_default_slot);
    assert!(schema.named_slots.is_empty());
    assert!(schema.positional_slots.is_empty());
    assert!(schema.has_any_slots());
}

#[test]
fn schema_from_named_slots() {
    let mut string_table = StringTable::new();
    let name_alpha = string_table.intern("alpha");
    let name_beta = string_table.intern("beta");

    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let alpha_slot = builder.push_slot_node(SlotKey::Named(name_alpha), empty_location());
    let beta_slot = builder.push_slot_node(SlotKey::Named(name_beta), empty_location());
    let root = builder.push_sequence_node(vec![alpha_slot, beta_slot], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &string_table)
        .expect("schema extraction should succeed");

    assert!(!schema.has_default_slot);
    assert!(schema.named_slots.contains(&name_alpha));
    assert!(schema.named_slots.contains(&name_beta));
    assert!(schema.positional_slots.is_empty());
}

#[test]
fn schema_from_positional_slots() {
    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let slot_0 = builder.push_slot_node(SlotKey::Positional(0), empty_location());
    let slot_1 = builder.push_slot_node(SlotKey::Positional(1), empty_location());
    let slot_2 = builder.push_slot_node(SlotKey::Positional(2), empty_location());
    let root = builder.push_sequence_node(vec![slot_0, slot_1, slot_2], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &StringTable::new())
        .expect("schema extraction should succeed");

    assert!(!schema.has_default_slot);
    assert!(schema.named_slots.is_empty());
    assert!(schema.positional_slots.contains(&0));
    assert!(schema.positional_slots.contains(&1));
    assert!(schema.positional_slots.contains(&2));
}

#[test]
fn schema_from_mixed_slot_types() {
    let mut string_table = StringTable::new();
    let name_id = string_table.intern("title");

    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let default_slot = builder.push_slot_node(SlotKey::Default, empty_location());
    let named_slot = builder.push_slot_node(SlotKey::Named(name_id), empty_location());
    let positional_slot = builder.push_slot_node(SlotKey::Positional(0), empty_location());
    let root = builder.push_sequence_node(
        vec![default_slot, named_slot, positional_slot],
        empty_location(),
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &string_table)
        .expect("schema extraction should succeed");

    assert!(schema.has_default_slot);
    assert!(schema.named_slots.contains(&name_id));
    assert!(schema.positional_slots.contains(&0));
}

#[test]
fn schema_from_nested_child_template_containing_slot() {
    let mut string_table = StringTable::new();
    let name_id = string_table.intern("child_slot");

    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let child_slot = builder.push_slot_node(SlotKey::Named(name_id), empty_location());
    let child_template_id = builder.finish_template(
        child_slot,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let child_reference = builder.push_child_template_node(child_template_id, empty_location());
    let root = builder.push_sequence_node(vec![child_reference], empty_location());
    let parent_template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, parent_template_id, &string_table)
        .expect("schema extraction should succeed");

    assert!(schema.named_slots.contains(&name_id));
}

#[test]
fn schema_from_branch_chain_containing_slot() {
    let mut string_table = StringTable::new();
    let name_id = string_table.intern("branch_slot");

    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let branch_body_slot = builder.push_slot_node(SlotKey::Named(name_id), empty_location());
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression(true)),
        branch_body_slot,
        empty_location(),
    );
    let branch_chain = builder.push_branch_chain_node(vec![branch], None, empty_location());
    let template_id = builder.finish_template(
        branch_chain,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &string_table)
        .expect("schema extraction should succeed");

    assert!(schema.named_slots.contains(&name_id));
}

#[test]
fn schema_from_loop_containing_slot() {
    let mut string_table = StringTable::new();
    let name_id = string_table.intern("loop_slot");

    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let body_slot = builder.push_slot_node(SlotKey::Named(name_id), empty_location());
    let loop_node = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression(true)),
        },
        body_slot,
        None,
        empty_location(),
    );
    let template_id = builder.finish_template(
        loop_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &string_table)
        .expect("schema extraction should succeed");

    assert!(schema.named_slots.contains(&name_id));
}

#[test]
fn multiple_default_slots_produces_diagnostic() {
    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let first_default = builder.push_slot_node(SlotKey::Default, empty_location());
    let second_default = builder.push_slot_node(SlotKey::Default, empty_location());
    let root = builder.push_sequence_node(vec![first_default, second_default], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let result = collect_tir_slot_schema(&store, template_id, &StringTable::new());
    let error = result.expect_err("two default slots should produce an error");

    match &error.payload {
        DiagnosticPayload::InvalidTemplateSlot {
            reason: InvalidTemplateSlotReason::MultipleDefaultSlots,
            ..
        } => {}
        other => panic!("expected MultipleDefaultSlots diagnostic, got {other:?}"),
    }
}

#[test]
fn ordered_slot_keys_returns_deterministic_order() {
    let mut string_table = StringTable::new();
    let name_z = string_table.intern("zulu");
    let name_a = string_table.intern("alpha");

    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);

    let named_z = builder.push_slot_node(SlotKey::Named(name_z), empty_location());
    let positional_2 = builder.push_slot_node(SlotKey::Positional(2), empty_location());
    let default_slot = builder.push_slot_node(SlotKey::Default, empty_location());
    let named_a = builder.push_slot_node(SlotKey::Named(name_a), empty_location());
    let positional_0 = builder.push_slot_node(SlotKey::Positional(0), empty_location());
    let root = builder.push_sequence_node(
        vec![named_z, positional_2, default_slot, named_a, positional_0],
        empty_location(),
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &string_table)
        .expect("schema extraction should succeed");

    let ordered = schema.ordered_slot_keys(&string_table);
    assert_eq!(ordered.len(), 5);
    assert_eq!(ordered[0], SlotKey::Default);
    assert_eq!(ordered[1], SlotKey::Positional(0));
    assert_eq!(ordered[2], SlotKey::Positional(2));
    assert_eq!(ordered[3], SlotKey::Named(name_a));
    assert_eq!(ordered[4], SlotKey::Named(name_z));
}

#[test]
fn accepts_target_validates_correctly() {
    let mut string_table = StringTable::new();
    let known_name = string_table.intern("known");

    let mut schema = TirSlotSchema {
        has_default_slot: true,
        ..TirSlotSchema::default()
    };
    schema.named_slots.insert(known_name);
    schema.positional_slots.insert(1);

    assert!(schema.accepts_target(&SlotKey::Default));
    assert!(schema.accepts_target(&SlotKey::Named(known_name)));
    assert!(schema.accepts_target(&SlotKey::Positional(1)));

    let unknown_name = string_table.intern("unknown");
    assert!(!schema.accepts_target(&SlotKey::Named(unknown_name)));
    assert!(!schema.accepts_target(&SlotKey::Positional(0)));
}

#[test]
fn skipped_node_kinds_do_not_contribute_to_schema() {
    let mut string_table = StringTable::new();
    let text_id = string_table.intern("literal");

    let mut store = TemplateIrStore::new();

    // Push the aggregate node directly via the store so the builder borrow is
    // not active while mutating the store through a second path.
    let aggregate_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::AggregateOutput,
        empty_location(),
    ));

    let mut builder = TemplateIrBuilder::new(&mut store);

    let text_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);
    let text_node = builder.push_text_node(
        text_id,
        text_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let expression = Expression::string_slice(text_id, empty_location(), ValueMode::ImmutableOwned);
    let dynamic_node = builder.push_dynamic_expression_node(
        expression,
        TemplateSegmentOrigin::Body,
        None,
        empty_location(),
    );
    let loop_control_node =
        builder.push_loop_control_node(TemplateLoopControlKind::Break, empty_location());

    let root = builder.push_sequence_node(
        vec![text_node, dynamic_node, aggregate_node, loop_control_node],
        empty_location(),
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let schema = collect_tir_slot_schema(&store, template_id, &string_table)
        .expect("schema extraction should succeed");

    assert!(!schema.has_any_slots());
}

// -------------------------
//  Routing Tests
// -------------------------

#[test]
fn route_explicit_insert_to_named_slot() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("title");

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Named(name));
    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Named(name), &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let fill = build_fill_template(&mut store, vec![insert_node]);

    // TIR slot routing expands the InsertContribution marker and routes the
    // insert helper's body content to the target slot.
    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    let insert_body_node = template_root_node_id(insert_template, &store);
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Named(name)),
        &[insert_body_node]
    );
    assert!(
        routed
            .contributions
            .nodes_for_slot(&SlotKey::Default)
            .is_empty()
    );
    assert!(routed.schema.accepts_target(&SlotKey::Named(name)));
}

#[test]
fn route_explicit_insert_to_default_slot() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Default);
    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Default, &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let fill = build_fill_template(&mut store, vec![insert_node]);

    // TIR slot routing expands the InsertContribution marker and routes the
    // insert helper's body content to the target slot.
    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    let insert_body_node = template_root_node_id(insert_template, &store);
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Default),
        &[insert_body_node]
    );
}

#[test]
fn route_explicit_insert_to_positional_slot() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Positional(0));
    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Positional(0), &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let fill = build_fill_template(&mut store, vec![insert_node]);

    // TIR slot routing expands the InsertContribution marker and routes the
    // insert helper's body content to the target slot.
    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    let insert_body_node = template_root_node_id(insert_template, &store);
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Positional(0)),
        &[insert_body_node]
    );
}

#[test]
fn route_loose_content_to_positional_slots() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper = build_wrapper_with_slots(
        &mut store,
        vec![SlotKey::Positional(0), SlotKey::Positional(1)],
    );

    let first_child = build_child_template_node(&mut store, &mut string_table);
    let second_child = build_child_template_node(&mut store, &mut string_table);
    let fill = build_fill_template(&mut store, vec![first_child, second_child]);

    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Positional(0)),
        &[first_child]
    );
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Positional(1)),
        &[second_child]
    );
}

#[test]
fn route_loose_content_to_default_slot_after_positional_exhaustion() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper =
        build_wrapper_with_slots(&mut store, vec![SlotKey::Positional(0), SlotKey::Default]);

    let leading_text = build_text_node(
        &mut store,
        &mut string_table,
        "before",
        TemplateSegmentOrigin::Body,
    );
    let child = build_child_template_node(&mut store, &mut string_table);
    let trailing_text = build_text_node(
        &mut store,
        &mut string_table,
        "after",
        TemplateSegmentOrigin::Body,
    );
    let fill = build_fill_template(&mut store, vec![leading_text, child, trailing_text]);

    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Positional(0)),
        &[leading_text]
    );
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Default),
        &[child, trailing_text]
    );
}

#[test]
fn route_loose_whitespace_after_head_fill_to_next_child_contribution() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper =
        build_wrapper_with_slots(&mut store, vec![SlotKey::Positional(0), SlotKey::Default]);

    let head_fill = build_text_node(
        &mut store,
        &mut string_table,
        "head fill",
        TemplateSegmentOrigin::Head,
    );
    let separator = build_text_node(
        &mut store,
        &mut string_table,
        " ",
        TemplateSegmentOrigin::Body,
    );
    let child = build_child_template_node(&mut store, &mut string_table);
    let fill = build_fill_template(&mut store, vec![head_fill, separator, child]);

    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Positional(0)),
        &[head_fill],
        "whitespace after a head contribution must not become trailing positional content"
    );
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Default),
        &[separator, child],
        "the separator before the body child belongs to the body contribution"
    );
}

#[test]
fn route_loose_content_to_default_slot_only() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Default);

    let text = build_text_node(
        &mut store,
        &mut string_table,
        "body",
        TemplateSegmentOrigin::Body,
    );
    let child = build_child_template_node(&mut store, &mut string_table);
    let fill = build_fill_template(&mut store, vec![text, child]);

    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Default),
        &[text, child]
    );
}

#[test]
fn unknown_insert_target_produces_diagnostic() {
    let mut string_table = StringTable::new();
    let unknown_name = string_table.intern("unknown");

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Default);
    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Named(unknown_name), &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let fill = build_fill_template(&mut store, vec![insert_node]);

    let error = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect_err("unknown insert target should produce an error");

    assert_invalid_template_slot_reason(
        &error,
        InvalidTemplateSlotReason::InsertTargetsUnknownNamedSlot,
    );
}

#[test]
fn loose_content_without_default_or_positional_slots_produces_diagnostic() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("only_named");

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Named(name));

    let text = build_text_node(
        &mut store,
        &mut string_table,
        "loose",
        TemplateSegmentOrigin::Body,
    );
    let fill = build_fill_template(&mut store, vec![text]);

    let error = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect_err("loose content with no default slot should produce an error");

    assert_invalid_template_slot_reason(
        &error,
        InvalidTemplateSlotReason::LooseContentWithoutDefaultSlot,
    );
}

#[test]
fn named_only_slots_discard_loose_whitespace_around_insert_contributions() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("title");

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Named(name));
    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Named(name), &mut string_table);

    let leading_whitespace = build_text_node(
        &mut store,
        &mut string_table,
        "\n    ",
        TemplateSegmentOrigin::Body,
    );
    let trailing_whitespace = build_text_node(
        &mut store,
        &mut string_table,
        "\n",
        TemplateSegmentOrigin::Body,
    );
    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let fill = build_fill_template(
        &mut store,
        vec![leading_whitespace, insert_node, trailing_whitespace],
    );

    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("formatting whitespace should not require a default slot");

    let insert_body_node = template_root_node_id(insert_template, &store);
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Named(name)),
        &[insert_body_node]
    );
    assert!(
        routed
            .contributions
            .nodes_for_slot(&SlotKey::Default)
            .is_empty()
    );
}

#[test]
fn extra_loose_content_beyond_positional_capacity_produces_diagnostic() {
    let mut string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Positional(0));

    let first_child = build_child_template_node(&mut store, &mut string_table);
    let second_child = build_child_template_node(&mut store, &mut string_table);
    let fill = build_fill_template(&mut store, vec![first_child, second_child]);

    let error = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect_err("extra loose content should produce an error");

    assert_invalid_template_slot_reason(
        &error,
        InvalidTemplateSlotReason::ExtraLooseContentWithoutDefaultSlot,
    );
}

#[test]
fn nodes_for_slot_returns_correct_nodes() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("title");

    let mut store = TemplateIrStore::new();
    let wrapper =
        build_wrapper_with_slots(&mut store, vec![SlotKey::Named(name), SlotKey::Default]);

    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Named(name), &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let loose_text = build_text_node(
        &mut store,
        &mut string_table,
        "body",
        TemplateSegmentOrigin::Body,
    );
    let fill = build_fill_template(&mut store, vec![insert_node, loose_text]);

    // TIR slot routing expands the InsertContribution marker and routes the
    // insert helper's body content to the target slot.
    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    let insert_body_node = template_root_node_id(insert_template, &store);
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Named(name)),
        &[insert_body_node]
    );
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Default),
        &[loose_text]
    );
    assert!(
        routed
            .contributions
            .nodes_for_slot(&SlotKey::Positional(0))
            .is_empty()
    );
}

#[test]
fn empty_fill_template_with_slots_wrapper_routes_to_empty_contributions() {
    let string_table = StringTable::new();

    let mut store = TemplateIrStore::new();
    let wrapper = build_single_slot_template(&mut store, SlotKey::Default);
    let fill = build_fill_template(&mut store, vec![]);

    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    assert!(routed.schema.has_default_slot);
    assert!(
        routed
            .contributions
            .nodes_for_slot(&SlotKey::Default)
            .is_empty()
    );
    assert!(routed.contributions.named_nodes.is_empty());
    assert!(routed.contributions.positional_nodes.is_empty());
}

#[test]
fn mixed_explicit_inserts_and_loose_content_are_bucketed() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("title");

    let mut store = TemplateIrStore::new();
    let wrapper =
        build_wrapper_with_slots(&mut store, vec![SlotKey::Named(name), SlotKey::Default]);

    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Named(name), &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
    let loose_text = build_text_node(
        &mut store,
        &mut string_table,
        "body",
        TemplateSegmentOrigin::Body,
    );
    let loose_child = build_child_template_node(&mut store, &mut string_table);
    let fill = build_fill_template(&mut store, vec![insert_node, loose_text, loose_child]);

    // TIR slot routing expands the InsertContribution marker and routes the
    // insert helper's body content to the target slot.
    let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
        .expect("routing should succeed");

    let insert_body_node = template_root_node_id(insert_template, &store);
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Named(name)),
        &[insert_body_node]
    );
    assert_eq!(
        routed.contributions.nodes_for_slot(&SlotKey::Default),
        &[loose_text, loose_child]
    );
}

// -------------------------
//  Expansion Tests
// -------------------------

#[test]
fn expand_default_slot_with_single_contribution() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let contribution = build_single_text_template(&mut store, &mut string_table, "filled");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            has_default_slot: true,
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            default_nodes: vec![template_root_node_id(contribution, &store)],
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let child_kinds = root_child_kinds_for_node(expanded_root, &store);
    assert_eq!(child_kinds.len(), 1);
    assert!(
        matches!(child_kinds[0], TemplateIrNodeKind::Text { .. }),
        "default slot contribution should be spliced into the wrapper root"
    );
    assert_eq!(
        text_node_text(
            root_child_node_ids_for_node(expanded_root, &store)[0],
            &store,
            &string_table
        ),
        Some("filled".to_owned())
    );
}

#[test]
fn expand_named_slot_with_contribution() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("title");
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(name)]);
    let contribution = build_single_text_template(&mut store, &mut string_table, "heading");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            named_slots: [name].into_iter().collect(),
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            named_nodes: [(name, vec![template_root_node_id(contribution, &store)])]
                .into_iter()
                .collect(),
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let sequence_children = children_of_sequence_node(expanded_root, &store);
    assert_eq!(sequence_children.len(), 1);
    assert_eq!(
        text_node_text(sequence_children[0], &store, &string_table),
        Some("heading".to_owned())
    );
}

#[test]
fn expand_positional_slot_with_contribution() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Positional(0)]);
    let contribution = build_single_text_template(&mut store, &mut string_table, "first");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            positional_slots: [0].into_iter().collect(),
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            positional_nodes: [(0, vec![template_root_node_id(contribution, &store)])]
                .into_iter()
                .collect(),
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let sequence_children = children_of_sequence_node(expanded_root, &store);
    assert_eq!(sequence_children.len(), 1);
    assert_eq!(
        text_node_text(sequence_children[0], &store, &string_table),
        Some("first".to_owned())
    );
}

#[test]
fn missing_slot_renders_as_empty() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("missing");
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(name)]);

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            named_slots: [name].into_iter().collect(),
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions::default(),
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let sequence_children = children_of_sequence_node(expanded_root, &store);
    assert!(
        sequence_children.is_empty(),
        "missing slot should expand to an empty Sequence node"
    );
}

#[test]
fn repeated_slot_replays_same_contributions() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper =
        build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default, SlotKey::Default]);
    let contribution = build_single_text_template(&mut store, &mut string_table, "shared");
    let contribution_node_id = template_root_node_id(contribution, &store);

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            has_default_slot: true,
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            default_nodes: vec![contribution_node_id],
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let root_children = root_child_node_ids_for_node(expanded_root, &store);
    assert_eq!(
        root_children.len(),
        2,
        "two default slots should splice the same contribution twice"
    );

    for child_id in root_children {
        assert_eq!(
            text_node_text(child_id, &store, &string_table),
            Some("shared".to_owned())
        );
        assert_eq!(child_id, contribution_node_id);
    }
}

#[test]
fn mixed_slots_and_text() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_text_slot_text_wrapper(&mut store, &mut string_table, SlotKey::Default);
    let contribution = build_single_text_template(&mut store, &mut string_table, "body");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            has_default_slot: true,
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            default_nodes: vec![template_root_node_id(contribution, &store)],
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let child_kinds = root_child_kinds_for_node(expanded_root, &store);
    assert_eq!(child_kinds.len(), 3);
    assert!(matches!(child_kinds[0], TemplateIrNodeKind::Text { .. }));
    assert!(
        matches!(child_kinds[1], TemplateIrNodeKind::Text { .. }),
        "slot contribution should be spliced between the surrounding text nodes"
    );
    assert!(matches!(child_kinds[2], TemplateIrNodeKind::Text { .. }));

    let root_children = root_child_node_ids_for_node(expanded_root, &store);
    assert_eq!(
        text_node_text(root_children[1], &store, &string_table),
        Some("body".to_owned())
    );
}

#[test]
fn nested_child_template_with_slots_is_expanded() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("inner");
    let mut store = TemplateIrStore::new();

    let inner_wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(name)]);
    let parent_wrapper = {
        let mut builder = TemplateIrBuilder::new(&mut store);
        let child_reference = builder.push_child_template_node(inner_wrapper, empty_location());
        let root = builder.push_sequence_node(vec![child_reference], empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let contribution_template =
        build_single_text_template(&mut store, &mut string_table, "inner text");
    let contribution_node_id = template_root_node_id(contribution_template, &store);

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            named_slots: [name].into_iter().collect(),
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            named_nodes: [(name, vec![contribution_node_id])].into_iter().collect(),
            ..TirSlotContributions::default()
        },
    };

    let expanded_root =
        expand_tir_slot_placeholders(&mut store, parent_wrapper, &routed, &string_table)
            .expect("expansion should succeed");

    let parent_children = root_child_node_ids_for_node(expanded_root, &store);
    assert_eq!(parent_children.len(), 1);

    let expanded_child_id = match &store
        .get_node(parent_children[0])
        .expect("child node should exist")
        .kind
    {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference.root.template_id,
        other => panic!("expected ChildTemplate node, found {other:?}"),
    };

    assert_ne!(
        expanded_child_id, inner_wrapper,
        "child template should be a new expanded entry"
    );

    let expanded_child_root_children = root_child_kinds(expanded_child_id, &store);
    assert_eq!(expanded_child_root_children.len(), 1);
    assert!(
        matches!(
            expanded_child_root_children[0],
            TemplateIrNodeKind::Text { .. }
        ),
        "inner slot contribution should be spliced into the child template root"
    );
    assert_eq!(
        text_node_text(
            root_child_node_ids(expanded_child_id, &store)[0],
            &store,
            &string_table
        ),
        Some("inner text".to_owned())
    );
}

#[test]
fn nested_child_template_without_slots_is_unchanged() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let inner_template = build_single_text_template(&mut store, &mut string_table, "no slots");
    let parent_wrapper = {
        let mut builder = TemplateIrBuilder::new(&mut store);
        let child_reference = builder.push_child_template_node(inner_template, empty_location());
        let root = builder.push_sequence_node(vec![child_reference], empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema::default(),
        contributions: TirSlotContributions::default(),
    };

    let expanded_root =
        expand_tir_slot_placeholders(&mut store, parent_wrapper, &routed, &string_table)
            .expect("expansion should succeed");

    let parent_children = root_child_node_ids_for_node(expanded_root, &store);
    assert_eq!(parent_children.len(), 1);

    let child_id = match &store
        .get_node(parent_children[0])
        .expect("child node should exist")
        .kind
    {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference.root.template_id,
        other => panic!("expected ChildTemplate node, found {other:?}"),
    };

    assert_eq!(
        child_id, inner_template,
        "child template without slots should keep its original ID"
    );
}

#[test]
fn branch_chain_with_slots_in_body_is_expanded() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let occurrence_id = store.next_slot_occurrence_id();
    let placeholder = TirSlotPlaceholder::new(SlotKey::Default, occurrence_id, empty_location());
    let body_slot = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Slot { placeholder },
        empty_location(),
    ));

    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression(true)),
        body_slot,
        empty_location(),
    );

    let mut builder = TemplateIrBuilder::new(&mut store);
    let branch_chain = builder.push_branch_chain_node(vec![branch], None, empty_location());
    let wrapper = builder.finish_template(
        branch_chain,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let contribution = build_single_text_template(&mut store, &mut string_table, "branch body");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            has_default_slot: true,
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            default_nodes: vec![template_root_node_id(contribution, &store)],
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let expanded_branch_chain = match &store
        .get_node(expanded_root)
        .expect("root should exist")
        .kind
    {
        TemplateIrNodeKind::BranchChain { branches, .. } => {
            assert_eq!(branches.len(), 1);
            &branches[0]
        }
        other => panic!("expected BranchChain root, found {other:?}"),
    };

    let branch_body_children = root_child_kinds_for_node(expanded_branch_chain.body, &store);
    assert_eq!(branch_body_children.len(), 1);
    assert!(
        matches!(branch_body_children[0], TemplateIrNodeKind::Text { .. }),
        "branch body slot should be spliced into the body sequence"
    );
    assert_eq!(
        text_node_text(
            root_child_node_ids_for_node(expanded_branch_chain.body, &store)[0],
            &store,
            &string_table
        ),
        Some("branch body".to_owned())
    );
}

#[test]
fn loop_with_slots_in_body_is_expanded() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let mut builder = TemplateIrBuilder::new(&mut store);
    let body_slot = builder.push_slot_node(SlotKey::Default, empty_location());
    let loop_node = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression(true)),
        },
        body_slot,
        None,
        empty_location(),
    );
    let wrapper = builder.finish_template(
        loop_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let contribution = build_single_text_template(&mut store, &mut string_table, "iteration");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            has_default_slot: true,
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            default_nodes: vec![template_root_node_id(contribution, &store)],
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let expanded_loop = match &store
        .get_node(expanded_root)
        .expect("root should exist")
        .kind
    {
        TemplateIrNodeKind::Loop { body, .. } => *body,
        other => panic!("expected Loop root, found {other:?}"),
    };

    let body_children = root_child_kinds_for_node(expanded_loop, &store);
    assert_eq!(body_children.len(), 1);
    assert!(
        matches!(body_children[0], TemplateIrNodeKind::Text { .. }),
        "loop body slot should be spliced into the body sequence"
    );
    assert_eq!(
        text_node_text(
            root_child_node_ids_for_node(expanded_loop, &store)[0],
            &store,
            &string_table
        ),
        Some("iteration".to_owned())
    );
}

#[test]
fn no_slots_in_wrapper_returns_original_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_single_text_template(&mut store, &mut string_table, "plain");
    let original_root = store
        .get_template(wrapper)
        .expect("wrapper should exist")
        .root;

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema::default(),
        contributions: TirSlotContributions::default(),
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    assert_eq!(
        expanded_root, original_root,
        "wrapper with no slots should return the original root node ID"
    );
}

#[test]
fn expand_preserves_non_slot_nodes() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let text_id = string_table.intern("literal");
    let text_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);

    let text_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: text_id,
            byte_len: text_len,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));

    let aggregate_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::AggregateOutput,
        empty_location(),
    ));

    let loop_control_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::LoopControl {
            kind: TemplateLoopControlKind::Break,
        },
        empty_location(),
    ));

    let expression = Expression::string_slice(text_id, empty_location(), ValueMode::ImmutableOwned);
    let site_id = store.next_expression_site_id();
    let dynamic_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id,
        },
        empty_location(),
    ));

    let runtime_slot_plan_id = store.push_slot_plan(
        crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotPlan {
            location: empty_location(),
            contribution_sources: vec![],
            slot_sites: vec![],
        },
    );

    let runtime_slot_site_id =
        crate::compiler_frontend::ast::templates::template_slots::RuntimeSlotSiteId(0);
    let runtime_slot_site = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::RuntimeSlotSite {
            plan: runtime_slot_plan_id,
            site: runtime_slot_site_id,
        },
        empty_location(),
    ));

    let occurrence_id = store.next_slot_occurrence_id();
    let placeholder = TirSlotPlaceholder::new(SlotKey::Default, occurrence_id, empty_location());
    let slot_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Slot { placeholder },
        empty_location(),
    ));

    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![
                text_node,
                aggregate_node,
                loop_control_node,
                dynamic_node,
                runtime_slot_site,
                slot_node,
            ],
        },
        empty_location(),
    ));

    let wrapper = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));

    let contribution = build_single_text_template(&mut store, &mut string_table, "only slot");

    let routed = RoutedTirSlotContributions {
        schema: TirSlotSchema {
            has_default_slot: true,
            ..TirSlotSchema::default()
        },
        contributions: TirSlotContributions {
            default_nodes: vec![template_root_node_id(contribution, &store)],
            ..TirSlotContributions::default()
        },
    };

    let expanded_root = expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
        .expect("expansion should succeed");

    let child_kinds = root_child_kinds_for_node(expanded_root, &store);
    assert_eq!(child_kinds.len(), 6);
    assert!(matches!(child_kinds[0], TemplateIrNodeKind::Text { .. }));
    assert!(matches!(
        child_kinds[1],
        TemplateIrNodeKind::AggregateOutput
    ));
    assert!(matches!(
        child_kinds[2],
        TemplateIrNodeKind::LoopControl { .. }
    ));
    assert!(matches!(
        child_kinds[3],
        TemplateIrNodeKind::DynamicExpression { .. }
    ));
    assert!(matches!(
        child_kinds[4],
        TemplateIrNodeKind::RuntimeSlotSite { .. }
    ));
    assert!(
        matches!(child_kinds[5], TemplateIrNodeKind::Text { .. }),
        "slot contribution should be spliced without wrapping it in a nested sequence"
    );
    assert_eq!(
        text_node_text(
            root_child_node_ids_for_node(expanded_root, &store)[5],
            &store,
            &string_table
        ),
        Some("only slot".to_owned())
    );
}

// -------------------------
//  Head-Chain Composition Tests
// -------------------------

#[test]
fn no_head_atoms_returns_original_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body",
        TemplateSegmentOrigin::Body,
    );
    let template_id = build_template_with_children(&mut store, vec![body_text]);
    let original_root = template_root_node_id(template_id, &store);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    assert_eq!(
        composed_root, original_root,
        "template with only body children should return the original root unchanged"
    );
}

#[test]
fn head_atoms_but_no_receivers_returns_original_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let head_text = build_text_node(
        &mut store,
        &mut string_table,
        "head",
        TemplateSegmentOrigin::Head,
    );
    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body",
        TemplateSegmentOrigin::Body,
    );
    let template_id = build_template_with_children(&mut store, vec![head_text, body_text]);
    let original_root = template_root_node_id(template_id, &store);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    assert_eq!(
        composed_root, original_root,
        "template with head atoms but no receivers should return the original root unchanged"
    );
}

#[test]
fn single_receiver_with_body_fill() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);
    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body fill",
        TemplateSegmentOrigin::Body,
    );

    let template_id = build_template_with_children(&mut store, vec![wrapper_node, body_text]);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(
        composed_children.len(),
        1,
        "composed root should contain the resolved wrapper"
    );

    let resolved_wrapper_template_id = expect_child_template_id(composed_children[0], &store);
    let resolved_wrapper_children = root_child_node_ids(resolved_wrapper_template_id, &store);
    assert_eq!(resolved_wrapper_children.len(), 1);
    assert_eq!(
        text_node_text(resolved_wrapper_children[0], &store, &string_table),
        Some("body fill".to_owned())
    );
}

#[test]
fn single_receiver_with_head_fill() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);
    let head_fill = build_text_node(
        &mut store,
        &mut string_table,
        "head fill",
        TemplateSegmentOrigin::Head,
    );

    let template_id = build_template_with_children(&mut store, vec![wrapper_node, head_fill]);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(composed_children.len(), 1);

    let resolved_wrapper_template_id = expect_child_template_id(composed_children[0], &store);
    let resolved_wrapper_children = root_child_node_ids(resolved_wrapper_template_id, &store);
    assert_eq!(resolved_wrapper_children.len(), 1);
    assert_eq!(
        text_node_text(resolved_wrapper_children[0], &store, &string_table),
        Some("head fill".to_owned())
    );
}

#[test]
fn nested_receivers() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let inner_wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let inner_wrapper_node = build_child_template_node_for_template(&mut store, inner_wrapper);

    let outer_wrapper =
        build_text_slot_text_wrapper(&mut store, &mut string_table, SlotKey::Default);
    let outer_wrapper_node = build_child_template_node_for_template(&mut store, outer_wrapper);

    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "nested body",
        TemplateSegmentOrigin::Body,
    );

    let template_id = build_template_with_children(
        &mut store,
        vec![outer_wrapper_node, inner_wrapper_node, body_text],
    );

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(
        composed_children.len(),
        1,
        "outer wrapper should be the only root child"
    );

    let outer_resolved_id = expect_child_template_id(composed_children[0], &store);
    let outer_children = root_child_node_ids(outer_resolved_id, &store);
    assert_eq!(
        outer_children.len(),
        3,
        "outer wrapper should keep before/after text around the slot"
    );

    let inner_resolved_id = expect_child_template_id(outer_children[1], &store);
    let inner_children = root_child_node_ids(inner_resolved_id, &store);
    assert_eq!(inner_children.len(), 1);
    assert_eq!(
        text_node_text(inner_children[0], &store, &string_table),
        Some("nested body".to_owned())
    );
}

#[test]
fn receiver_with_no_fill_stays_unresolved() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);

    let template_id = build_template_with_children(&mut store, vec![wrapper_node]);
    let original_root = template_root_node_id(template_id, &store);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    assert_eq!(
        composed_root, original_root,
        "receiver with no fill should keep the original root unchanged"
    );
}

#[test]
fn receiver_with_named_slots() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("title");

    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(name)]);
    let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);

    let insert_template =
        build_slot_insert_template(&mut store, SlotKey::Named(name), &mut string_table);
    let mut builder = TemplateIrBuilder::new(&mut store);
    let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());

    let template_id = build_template_with_children(&mut store, vec![wrapper_node, insert_node]);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(composed_children.len(), 1);

    let resolved_wrapper_template_id = expect_child_template_id(composed_children[0], &store);
    let resolved_wrapper_children = root_child_node_ids(resolved_wrapper_template_id, &store);
    assert_eq!(resolved_wrapper_children.len(), 1);

    // Slot expansion places the insert helper's body content directly into the
    // wrapper's slot; the InsertContribution marker is resolved during routing.
    assert_eq!(
        text_node_text(resolved_wrapper_children[0], &store, &string_table),
        Some("insert".to_owned())
    );
}

#[test]
fn mixed_head_text_and_receiver() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let head_text = build_text_node(
        &mut store,
        &mut string_table,
        "head text",
        TemplateSegmentOrigin::Head,
    );

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);

    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body fill",
        TemplateSegmentOrigin::Body,
    );

    let template_id =
        build_template_with_children(&mut store, vec![head_text, wrapper_node, body_text]);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(
        composed_children.len(),
        2,
        "head text and resolved wrapper should remain"
    );

    assert_eq!(
        text_node_text(composed_children[0], &store, &string_table),
        Some("head text".to_owned()),
        "head text should appear before the resolved wrapper"
    );

    let resolved_wrapper_template_id = expect_child_template_id(composed_children[1], &store);
    let resolved_wrapper_children = root_child_node_ids(resolved_wrapper_template_id, &store);
    assert_eq!(resolved_wrapper_children.len(), 1);
    assert_eq!(
        text_node_text(resolved_wrapper_children[0], &store, &string_table),
        Some("body fill".to_owned())
    );
}

#[test]
fn multiple_receivers_in_sequence() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let first_wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let first_wrapper_node = build_child_template_node_for_template(&mut store, first_wrapper);
    let first_fill = build_text_node(
        &mut store,
        &mut string_table,
        "first fill",
        TemplateSegmentOrigin::Body,
    );

    let second_wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let second_wrapper_node = build_child_template_node_for_template(&mut store, second_wrapper);
    let second_fill = build_text_node(
        &mut store,
        &mut string_table,
        "second fill",
        TemplateSegmentOrigin::Body,
    );

    // Both wrappers are head-origin, so they appear before any body fill in
    // parser emission order. Body fill flows to the deepest active receiver,
    // which means the second wrapper is nested inside the first.
    let template_id = build_template_with_children(
        &mut store,
        vec![
            first_wrapper_node,
            second_wrapper_node,
            first_fill,
            second_fill,
        ],
    );

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(
        composed_children.len(),
        1,
        "outer wrapper should be the only root child"
    );

    let outer_resolved_id = expect_child_template_id(composed_children[0], &store);
    let outer_children = root_child_node_ids(outer_resolved_id, &store);
    assert_eq!(
        outer_children.len(),
        1,
        "outer wrapper should contain the resolved inner wrapper"
    );

    let inner_resolved_id = expect_child_template_id(outer_children[0], &store);
    let inner_children = root_child_node_ids(inner_resolved_id, &store);
    assert_eq!(inner_children.len(), 2);
    assert_eq!(
        text_node_text(inner_children[0], &store, &string_table),
        Some("first fill".to_owned())
    );
    assert_eq!(
        text_node_text(inner_children[1], &store, &string_table),
        Some("second fill".to_owned())
    );
}

#[test]
fn body_content_without_active_receiver_goes_to_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let head_text = build_text_node(
        &mut store,
        &mut string_table,
        "head text",
        TemplateSegmentOrigin::Head,
    );
    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body text",
        TemplateSegmentOrigin::Body,
    );

    let template_id = build_template_with_children(&mut store, vec![head_text, body_text]);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(composed_children.len(), 2);
    assert_eq!(
        text_node_text(composed_children[0], &store, &string_table),
        Some("head text".to_owned())
    );
    assert_eq!(
        text_node_text(composed_children[1], &store, &string_table),
        Some("body text".to_owned())
    );
}

#[test]
fn compose_preserves_non_receiver_head_child_templates() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let non_receiver = build_single_text_template(&mut store, &mut string_table, "no slots");
    let non_receiver_node = build_child_template_node_for_template(&mut store, non_receiver);
    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body text",
        TemplateSegmentOrigin::Body,
    );

    let template_id = build_template_with_children(&mut store, vec![non_receiver_node, body_text]);

    let composed_root = compose_tir_head_chain(&mut store, template_id, &string_table, false)
        .expect("composition should succeed");

    let composed_children = root_child_node_ids_for_node(composed_root, &store);
    assert_eq!(composed_children.len(), 2);

    let preserved_child_id = expect_child_template_id(composed_children[0], &store);
    assert_eq!(
        preserved_child_id, non_receiver,
        "non-receiver head child template should keep its original template ID"
    );

    assert_eq!(
        text_node_text(composed_children[1], &store, &string_table),
        Some("body text".to_owned())
    );
}

/// Asserts that a node is a `ChildTemplate` reference and returns the template ID.
fn expect_child_template_id(node_id: TemplateIrNodeId, store: &TemplateIrStore) -> TemplateIrId {
    let node = store.get_node(node_id).expect("node should exist");

    match &node.kind {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference.root.template_id,
        other => panic!("expected ChildTemplate node, found {other:?}"),
    }
}

// -------------------------
//  Expansion Test Inspection Helpers
// -------------------------

/// Returns the direct children of any Sequence node.
fn children_of_sequence_node(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
) -> Vec<TemplateIrNodeId> {
    let node = store.get_node(node_id).expect("node should exist");

    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children.clone(),
        other => panic!("expected Sequence node, found {other:?}"),
    }
}

/// Returns the direct children of a node that is expected to be a Sequence root.
fn root_child_node_ids_for_node(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
) -> Vec<TemplateIrNodeId> {
    children_of_sequence_node(node_id, store)
}

/// Returns references to the kinds of the children of a Sequence node.
fn root_child_kinds_for_node(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
) -> Vec<&TemplateIrNodeKind> {
    root_child_node_ids_for_node(node_id, store)
        .into_iter()
        .map(|node_id| {
            &store
                .get_node(node_id)
                .expect("child node should exist")
                .kind
        })
        .collect()
}

// -------------------------
//  Child Wrapper Composition Tests
// -------------------------

#[test]
fn wrap_direct_child_in_single_wrapper_with_default_slot() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let child = build_single_text_template(&mut store, &mut string_table, "child");
    let child_node = build_child_template_node_for_template(&mut store, child);
    let parent = build_template_with_children(&mut store, vec![child_node]);
    let original_root = template_root_node_id(parent, &store);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[wrapper], &string_table)
        .expect("wrapper application should succeed");

    assert_ne!(
        wrapped_root, original_root,
        "wrapper application should produce a new root"
    );

    let parent_children = root_child_node_ids_for_node(wrapped_root, &store);
    assert_eq!(parent_children.len(), 1);

    let wrapped_template_id = expect_child_template_id(parent_children[0], &store);
    let wrapped_children = root_child_node_ids(wrapped_template_id, &store);
    assert_eq!(wrapped_children.len(), 1);

    let resolved_child_id = expect_child_template_id(wrapped_children[0], &store);
    assert_eq!(
        resolved_child_id, child,
        "wrapper with a single default slot should contain the original child template"
    );
    assert_eq!(
        template_root_text(resolved_child_id, &store, &string_table),
        Some("child".to_owned())
    );
}

#[test]
fn wrap_direct_child_in_wrapper_without_slots() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_single_text_template(&mut store, &mut string_table, "prefix");
    let child = build_single_text_template(&mut store, &mut string_table, "child");
    let child_node = build_child_template_node_for_template(&mut store, child);
    let parent = build_template_with_children(&mut store, vec![child_node]);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[wrapper], &string_table)
        .expect("wrapper application should succeed");

    let parent_children = root_child_node_ids_for_node(wrapped_root, &store);
    assert_eq!(parent_children.len(), 1);

    let combined_template_id = expect_child_template_id(parent_children[0], &store);
    let combined_children = root_child_node_ids(combined_template_id, &store);
    assert_eq!(combined_children.len(), 2);

    let wrapper_child_id = expect_child_template_id(combined_children[0], &store);
    assert_eq!(
        wrapper_child_id, wrapper,
        "slot-less wrapper should prepend its content before the original child"
    );
    assert_eq!(
        template_root_text(wrapper_child_id, &store, &string_table),
        Some("prefix".to_owned())
    );

    let inner_child_id = expect_child_template_id(combined_children[1], &store);
    assert_eq!(
        inner_child_id, child,
        "slot-less wrapper should keep the original child after the wrapper content"
    );
}

#[test]
fn wrap_direct_child_in_multiple_wrappers() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let inner_wrapper =
        build_text_slot_text_wrapper(&mut store, &mut string_table, SlotKey::Default);
    let outer_wrapper =
        build_text_slot_text_wrapper(&mut store, &mut string_table, SlotKey::Default);

    let child = build_single_text_template(&mut store, &mut string_table, "child");
    let child_node = build_child_template_node_for_template(&mut store, child);
    let parent = build_template_with_children(&mut store, vec![child_node]);

    // Wrappers are stored innermost-first; outermost is applied first by reverse
    // iteration, so the final nesting is outer(inner(child)).
    let wrapper_ids = vec![inner_wrapper, outer_wrapper];

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &wrapper_ids, &string_table)
        .expect("wrapper application should succeed");

    let parent_children = root_child_node_ids_for_node(wrapped_root, &store);
    assert_eq!(parent_children.len(), 1);

    let outer_resolved_id = expect_child_template_id(parent_children[0], &store);
    let outer_children = root_child_node_ids(outer_resolved_id, &store);
    assert_eq!(outer_children.len(), 3);

    assert_eq!(
        text_node_text(outer_children[0], &store, &string_table),
        Some("before".to_owned())
    );

    let inner_resolved_id = expect_child_template_id(outer_children[1], &store);
    let inner_children = root_child_node_ids(inner_resolved_id, &store);
    assert_eq!(inner_children.len(), 3);
    assert_eq!(
        text_node_text(inner_children[0], &store, &string_table),
        Some("before".to_owned())
    );
    assert_eq!(
        text_node_text(inner_children[2], &store, &string_table),
        Some("after".to_owned())
    );

    let innermost_child_id = expect_child_template_id(inner_children[1], &store);
    assert_eq!(innermost_child_id, child);

    assert_eq!(
        text_node_text(outer_children[2], &store, &string_table),
        Some("after".to_owned())
    );
}

#[test]
fn child_with_slots_is_not_wrapped() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let child_with_slots = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
    let child_node = build_child_template_node_for_template(&mut store, child_with_slots);
    let parent = build_template_with_children(&mut store, vec![child_node]);
    let original_root = template_root_node_id(parent, &store);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[wrapper], &string_table)
        .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, original_root,
        "a child that is itself a wrapper receiver should not be wrapped"
    );
}

#[test]
fn no_wrappers_returns_original_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let child = build_single_text_template(&mut store, &mut string_table, "child");
    let child_node = build_child_template_node_for_template(&mut store, child);
    let parent = build_template_with_children(&mut store, vec![child_node]);
    let original_root = template_root_node_id(parent, &store);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[], &string_table)
        .expect("wrapper application should succeed");

    assert_eq!(wrapped_root, original_root);
}

#[test]
fn branch_chain_child_is_not_wrapped() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let branch_body = build_single_text_template(&mut store, &mut string_table, "branch body");
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression(true)),
        template_root_node_id(branch_body, &store),
        empty_location(),
    );

    let mut builder = TemplateIrBuilder::new(&mut store);
    let branch_chain = builder.push_branch_chain_node(vec![branch], None, empty_location());
    let parent = builder.finish_template(
        branch_chain,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let wrapper = build_single_text_template(&mut store, &mut string_table, "wrapper");
    let original_root = template_root_node_id(parent, &store);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[wrapper], &string_table)
        .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, original_root,
        "control-flow branch chain should not be wrapped during composition"
    );
}

#[test]
fn loop_child_is_not_wrapped() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let body = build_single_text_template(&mut store, &mut string_table, "iteration");
    let body_root = template_root_node_id(body, &store);
    let mut builder = TemplateIrBuilder::new(&mut store);
    let loop_node = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression(true)),
        },
        body_root,
        None,
        empty_location(),
    );
    let parent = builder.finish_template(
        loop_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let wrapper = build_single_text_template(&mut store, &mut string_table, "wrapper");
    let original_root = template_root_node_id(parent, &store);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[wrapper], &string_table)
        .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, original_root,
        "control-flow loop should not be wrapped during composition"
    );
}

#[test]
fn mixed_children_wraps_only_direct_child() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);

    let body_text = build_text_node(
        &mut store,
        &mut string_table,
        "body text",
        TemplateSegmentOrigin::Body,
    );

    let child = build_single_text_template(&mut store, &mut string_table, "child");
    let child_node = build_child_template_node_for_template(&mut store, child);

    let branch_body = build_single_text_template(&mut store, &mut string_table, "branch body");
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression(true)),
        template_root_node_id(branch_body, &store),
        empty_location(),
    );
    let mut builder = TemplateIrBuilder::new(&mut store);
    let branch_chain = builder.push_branch_chain_node(vec![branch], None, empty_location());

    let parent =
        build_template_with_children(&mut store, vec![body_text, child_node, branch_chain]);

    let wrapped_root = apply_tir_child_wrappers(&mut store, parent, &[wrapper], &string_table)
        .expect("wrapper application should succeed");

    let parent_children = root_child_node_ids_for_node(wrapped_root, &store);
    assert_eq!(parent_children.len(), 3);

    assert!(
        matches!(
            store.get_node(parent_children[0]).unwrap().kind,
            TemplateIrNodeKind::Text { .. }
        ),
        "plain text should pass through unchanged"
    );

    let wrapped_template_id = expect_child_template_id(parent_children[1], &store);
    let wrapped_children = root_child_node_ids(wrapped_template_id, &store);
    assert_eq!(wrapped_children.len(), 1);

    let resolved_child_id = expect_child_template_id(wrapped_children[0], &store);
    assert_eq!(
        resolved_child_id, child,
        "only the direct body child template should be wrapped"
    );
    assert_eq!(
        template_root_text(resolved_child_id, &store, &string_table),
        Some("child".to_owned())
    );

    assert!(
        matches!(
            store.get_node(parent_children[2]).unwrap().kind,
            TemplateIrNodeKind::BranchChain { .. }
        ),
        "branch chain should pass through unchanged"
    );
}

// -------------------------
//  Slot Resolution Overlay Materialization
// -------------------------
//
// These tests exercise `materialize_tir_slot_resolution_overlay`, the first
// Phase 6 overlay-composition step. They assert that routed TIR slot
// contributions become a registry-owned `TirSlotResolutionOverlay` keyed by
// `SlotOccurrenceId` with store-qualified `TemplateRef` sources, while the
// structural `expand_tir_slot_placeholders` behavior is left unchanged.

/// Retrieves the materialized overlay from the registry, panicking if missing.
fn slot_resolution_overlay(
    registry: &TemplateIrRegistry,
    overlay_id: super::super::overlays::TirSlotResolutionOverlayId,
) -> &TirSlotResolutionOverlay {
    registry
        .slot_resolution_overlay(overlay_id)
        .expect("materialized overlay should be registry-owned and retrievable")
}

fn registry_with_store() -> (TemplateIrRegistry, TemplateStoreId) {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    (registry, store_id)
}

fn same_store_child_reference(
    store_id: TemplateStoreId,
    template_id: TemplateIrId,
) -> TemplateTirChildReference {
    TemplateTirChildReference::same_store(
        template_id,
        store_id,
        TemplateTirPhase::Parsed,
        super::super::overlays::TemplateOverlaySetId::empty(),
    )
}

fn same_store_template_ref(store_id: TemplateStoreId, template_id: TemplateIrId) -> TemplateRef {
    TemplateRef::new(store_id, template_id)
}

fn materialize_same_store_slot_resolution_overlay(
    registry: &mut TemplateIrRegistry,
    store_id: TemplateStoreId,
    wrapper_template_id: TemplateIrId,
    routed: &RoutedTirSlotContributions,
) -> Result<super::super::overlays::TirSlotResolutionOverlayId, Box<CompilerDiagnostic>> {
    materialize_tir_slot_resolution_overlay(
        registry,
        store_id,
        same_store_child_reference(store_id, wrapper_template_id),
        routed,
    )
}

#[test]
fn overlay_materializes_default_slot_resolution() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay = slot_resolution_overlay(&registry, overlay_id);
    assert_eq!(overlay.resolutions.len(), 1, "one default slot occurrence");

    let (occurrence_id, resolution) = &overlay.resolutions[0];
    assert_eq!(*occurrence_id, SlotOccurrenceId::new(0));
    assert_eq!(resolution.key, SlotKey::Default);
    assert!(
        matches!(resolution.kind, TirSlotResolutionKind::Resolved { .. }),
        "default slot with contributions should resolve to a source list"
    );
    assert_eq!(resolution.sources().len(), 1, "one source template ref");
    let source_ref = resolution.sources()[0];
    assert_eq!(source_ref.store_id, store_id);
    assert!(
        registry.template(source_ref).is_some(),
        "overlay source ref should resolve through the same registry"
    );
}

#[test]
fn overlay_materializes_named_slot_resolution() {
    let mut string_table = StringTable::new();
    let title = string_table.intern("title");
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(title)]);
        let insert_template =
            build_slot_insert_template(&mut store, SlotKey::Named(title), &mut string_table);
        let mut builder = TemplateIrBuilder::new(&mut store);
        let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
        let fill = build_fill_template(&mut store, vec![insert_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay = slot_resolution_overlay(&registry, overlay_id);
    assert_eq!(overlay.resolutions.len(), 1, "one named slot occurrence");

    let (occurrence_id, resolution) = &overlay.resolutions[0];
    assert_eq!(*occurrence_id, SlotOccurrenceId::new(0));
    assert_eq!(resolution.key, SlotKey::Named(title));
    assert_eq!(
        resolution.sources().len(),
        1,
        "named slot should have one source"
    );
}

#[test]
fn overlay_materializes_positional_slot_resolution() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Positional(0)]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "first");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay = slot_resolution_overlay(&registry, overlay_id);
    assert_eq!(
        overlay.resolutions.len(),
        1,
        "one positional slot occurrence"
    );

    let (occurrence_id, resolution) = &overlay.resolutions[0];
    assert_eq!(*occurrence_id, SlotOccurrenceId::new(0));
    assert_eq!(resolution.key, SlotKey::Positional(0));
    assert_eq!(
        resolution.sources().len(),
        1,
        "positional slot should have one source"
    );
}

#[test]
fn overlay_materializes_missing_slot_as_missing() {
    let mut string_table = StringTable::new();
    let missing_name = string_table.intern("missing");
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper =
            build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(missing_name)]);

        // An empty fill template routes no content, leaving the named slot missing.
        let empty_fill = build_fill_template(&mut store, vec![]);

        let routed = route_tir_slot_contributions(&store, wrapper, empty_fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay = slot_resolution_overlay(&registry, overlay_id);
    assert_eq!(overlay.resolutions.len(), 1, "one slot occurrence");

    let (occurrence_id, resolution) = &overlay.resolutions[0];
    assert_eq!(*occurrence_id, SlotOccurrenceId::new(0));
    assert_eq!(resolution.key, SlotKey::Named(missing_name));
    assert!(
        resolution.is_missing(),
        "slot with no routed contributions should be a Missing resolution"
    );
    assert!(
        resolution.sources().is_empty(),
        "missing slot should carry no source refs"
    );
}

#[test]
fn overlay_materializes_repeated_slot_sharing_source_list() {
    let mut string_table = StringTable::new();
    let title = string_table.intern("title");
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        // Two occurrences of the same named slot in one wrapper. Named/positional
        // slots are idempotent in the schema, so repeated occurrences are valid;
        // only a second default slot is rejected.
        let wrapper = build_wrapper_with_slot_sequence(
            &mut store,
            vec![SlotKey::Named(title), SlotKey::Named(title)],
        );
        let insert_template =
            build_slot_insert_template(&mut store, SlotKey::Named(title), &mut string_table);
        let mut builder = TemplateIrBuilder::new(&mut store);
        let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
        let fill = build_fill_template(&mut store, vec![insert_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay = slot_resolution_overlay(&registry, overlay_id);
    assert_eq!(overlay.resolutions.len(), 2, "two named slot occurrences");

    let (first_occurrence, first_resolution) = &overlay.resolutions[0];
    let (second_occurrence, second_resolution) = &overlay.resolutions[1];
    assert_eq!(*first_occurrence, SlotOccurrenceId::new(0));
    assert_eq!(*second_occurrence, SlotOccurrenceId::new(1));

    assert_eq!(first_resolution.sources().len(), 1);
    assert_eq!(second_resolution.sources().len(), 1);

    let first_source = first_resolution.sources()[0];
    let second_source = second_resolution.sources()[0];
    assert_eq!(
        first_source, second_source,
        "repeated slot occurrences should share the same replayable source ref"
    );
}

#[test]
fn overlay_materialization_preserves_structural_expansion() {
    // The overlay path must not alter the structural expansion behavior.
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    // Materialize the overlay (exercising the new path) before expansion.
    let _overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    // Structural expansion should still produce the same filled result.
    let expanded_root = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
            .expect("structural expansion should still succeed")
    };

    let store = registry.store(store_id).expect("store should exist");
    let child_kinds = root_child_kinds_for_node(expanded_root, &store);
    assert_eq!(child_kinds.len(), 1);
    assert!(
        matches!(child_kinds[0], TemplateIrNodeKind::Text { .. }),
        "structural expansion should still splice the default slot contribution"
    );
}

// -------------------------
//  Overlay-Set Attachment Tests
// -------------------------

// These tests exercise `attach_tir_slot_resolution_overlay`, the second Phase 6
// overlay-composition step. They verify that a materialized slot-resolution
// overlay is attached to a registry-owned `TemplateOverlaySet`, that a `TirView`
// constructed with the wrapper template ref observes the resolution through the
// canonical overlay path, and that structural slot expansion remains unchanged.

#[test]
fn attach_overlay_set_carries_slot_resolution() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay_set_id = attach_tir_slot_resolution_overlay(&mut registry, overlay_id);

    let overlay_set = registry
        .overlay_set(overlay_set_id)
        .expect("attached overlay set should be registry-owned");
    assert_eq!(
        overlay_set.slot_resolution,
        Some(overlay_id),
        "overlay set should carry the materialized slot-resolution overlay ID"
    );
    assert!(
        overlay_set.expression_overrides.is_none(),
        "no expression overlay dimension should be set"
    );
    assert!(
        overlay_set.wrapper_context.is_none(),
        "no wrapper-context overlay dimension should be set"
    );
}

#[test]
fn tir_view_observes_attached_slot_resolution() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    let overlay_set_id = attach_tir_slot_resolution_overlay(&mut registry, overlay_id);

    // Qualify the wrapper template ref so the view can resolve it through the
    // same registry that owns the overlay set.
    let wrapper_ref = {
        let store = registry.store(store_id).expect("store should exist");
        store.qualify_template_ref(wrapper)
    };

    let view = TirView::new(
        &registry,
        wrapper_ref,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("view should construct with the attached overlay set");

    let occurrence_id = SlotOccurrenceId::new(0);
    let resolution = view
        .effective_slot_resolution(occurrence_id)
        .expect("slot resolution lookup should succeed")
        .expect("resolution should be present for the default slot occurrence");
    assert_eq!(
        resolution.key,
        SlotKey::Default,
        "view should observe the default slot resolution"
    );
    assert!(
        matches!(resolution.kind, TirSlotResolutionKind::Resolved { .. }),
        "view should observe a resolved slot through the overlay path"
    );
    assert_eq!(
        resolution.sources().len(),
        1,
        "view should observe one source template ref"
    );
    let source_ref = resolution.sources()[0];
    assert_eq!(
        source_ref.store_id, store_id,
        "view-observed source ref should resolve through the same store"
    );
    assert!(
        registry.template(source_ref).is_some(),
        "view-observed source ref should be registry-resolvable"
    );
}

#[test]
fn overlay_set_attachment_preserves_structural_expansion() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, routed)
    };

    let overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("overlay materialization should succeed");

    // Attach the overlay set before structural expansion to confirm the new
    // path does not alter production slot expansion.
    let _overlay_set_id = attach_tir_slot_resolution_overlay(&mut registry, overlay_id);

    let expanded_root = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
            .expect("structural expansion should still succeed after attachment")
    };

    let store = registry.store(store_id).expect("store should exist");
    let child_kinds = root_child_kinds_for_node(expanded_root, &store);
    assert_eq!(child_kinds.len(), 1);
    assert!(
        matches!(child_kinds[0], TemplateIrNodeKind::Text { .. }),
        "structural expansion should still splice the default slot contribution"
    );
}

// -------------------------
//  Registry-Owned Slot-Overlay Composition API
// -------------------------
//
// These tests exercise `compose_tir_slot_resolution_overlay_set`, the
// registry-owned entry point that bundles route, materialize, and attach for a
// single wrapper/fill pair. They confirm the bundled API produces the same
// overlay shape as the manual primitive sequence, works for named slots, and
// leaves structural expansion unchanged.

#[test]
fn compose_tir_slot_resolution_overlay_set_default_slot_matches_manual_sequence() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, fill, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, fill, routed)
    };

    // Manual primitive sequence: materialize then attach. The store borrow is
    // released before this so the registry can re-borrow the store mutably
    // inside materialization.
    let manual_overlay_id =
        materialize_same_store_slot_resolution_overlay(&mut registry, store_id, wrapper, &routed)
            .expect("manual materialization should succeed");
    let manual_set_id = attach_tir_slot_resolution_overlay(&mut registry, manual_overlay_id);

    // Bundled API: one call with registry/store identity, no separate store
    // borrow held by the caller. The registry owns the store access. Each
    // materialization allocates its own store-local source template, so the
    // two overlay sets carry different source `TemplateRef`s and are not
    // canonicalized to one ID; the assertion compares overlay shape, not ID.
    let bundled_set_id = compose_tir_slot_resolution_overlay_set(
        &mut registry,
        store_id,
        same_store_child_reference(store_id, wrapper),
        same_store_template_ref(store_id, fill),
        &string_table,
    )
    .expect("bundled overlay composition should succeed");

    let manual_set = registry
        .overlay_set(manual_set_id)
        .expect("manual overlay set should be registry-owned");
    let bundled_set = registry
        .overlay_set(bundled_set_id)
        .expect("bundled overlay set should be registry-owned");

    // Both sets carry only the slot-resolution dimension.
    assert!(
        bundled_set.slot_resolution.is_some(),
        "bundled overlay set should carry a slot-resolution overlay"
    );
    assert!(
        bundled_set.expression_overrides.is_none(),
        "no expression overlay dimension should be set"
    );
    assert!(
        bundled_set.wrapper_context.is_none(),
        "no wrapper-context overlay dimension should be set"
    );

    // The bundled overlay resolves the same slot key with the same source count
    // as the manual primitive sequence.
    let manual_overlay = slot_resolution_overlay(&registry, manual_set.slot_resolution.unwrap());
    let bundled_overlay = slot_resolution_overlay(&registry, bundled_set.slot_resolution.unwrap());

    assert_eq!(
        manual_overlay.resolutions.len(),
        bundled_overlay.resolutions.len(),
        "bundled API should resolve the same number of slot occurrences"
    );

    let (manual_occurrence, manual_resolution) = &manual_overlay.resolutions[0];
    let (bundled_occurrence, bundled_resolution) = &bundled_overlay.resolutions[0];
    assert_eq!(manual_occurrence, bundled_occurrence);
    assert_eq!(manual_resolution.key, bundled_resolution.key);
    assert_eq!(
        manual_resolution.sources().len(),
        bundled_resolution.sources().len(),
        "bundled API should produce the same number of source refs"
    );

    let source_ref = bundled_resolution.sources()[0];
    assert_eq!(source_ref.store_id, store_id);
    assert!(
        registry.template(source_ref).is_some(),
        "bundled overlay source ref should resolve through the same registry"
    );
}

#[test]
fn compose_tir_slot_resolution_overlay_set_named_slot_attaches_resolution() {
    let mut string_table = StringTable::new();
    let title = string_table.intern("title");
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, fill) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Named(title)]);
        let insert_template =
            build_slot_insert_template(&mut store, SlotKey::Named(title), &mut string_table);
        let mut builder = TemplateIrBuilder::new(&mut store);
        let insert_node = builder.push_insert_contribution_node(insert_template, empty_location());
        let fill = build_fill_template(&mut store, vec![insert_node]);
        (wrapper, fill)
    };

    let overlay_set_id = compose_tir_slot_resolution_overlay_set(
        &mut registry,
        store_id,
        same_store_child_reference(store_id, wrapper),
        same_store_template_ref(store_id, fill),
        &string_table,
    )
    .expect("bundled overlay composition should succeed for a named slot");

    let overlay_set = registry
        .overlay_set(overlay_set_id)
        .expect("overlay set should be registry-owned");

    let overlay_id = overlay_set
        .slot_resolution
        .expect("overlay set should carry a slot-resolution overlay");
    let overlay = slot_resolution_overlay(&registry, overlay_id);

    assert_eq!(overlay.resolutions.len(), 1, "one named slot occurrence");
    let (_occurrence_id, resolution) = &overlay.resolutions[0];
    assert_eq!(resolution.key, SlotKey::Named(title));
    assert!(
        matches!(resolution.kind, TirSlotResolutionKind::Resolved { .. }),
        "named slot with contributions should resolve to a source list"
    );
}

#[test]
fn compose_tir_slot_resolution_overlay_set_preserves_structural_expansion() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (wrapper, fill, routed) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);

        let routed = route_tir_slot_contributions(&store, wrapper, fill, &string_table)
            .expect("routing should succeed");

        (wrapper, fill, routed)
    };

    // Allocate the overlay set through the bundled registry-owned API. This
    // must not alter the structural expansion produced by the existing
    // store-local path.
    let _overlay_set_id = compose_tir_slot_resolution_overlay_set(
        &mut registry,
        store_id,
        same_store_child_reference(store_id, wrapper),
        same_store_template_ref(store_id, fill),
        &string_table,
    )
    .expect("bundled overlay composition should succeed");

    let expanded_root = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        expand_tir_slot_placeholders(&mut store, wrapper, &routed, &string_table)
            .expect("structural expansion should still succeed after overlay allocation")
    };

    let store = registry.store(store_id).expect("store should exist");
    let child_kinds = root_child_kinds_for_node(expanded_root, &store);
    assert_eq!(child_kinds.len(), 1);
    assert!(
        matches!(child_kinds[0], TemplateIrNodeKind::Text { .. }),
        "structural expansion should still splice the default slot contribution"
    );
}

#[test]
fn slot_overlay_pair_rejects_refs_outside_the_composition_store() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();
    let foreign_store_id = registry.allocate_store();

    let (wrapper, fill) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("composition store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let contribution = build_single_text_template(&mut store, &mut string_table, "filled");
        let contribution_node = template_root_node_id(contribution, &store);
        let fill = build_fill_template(&mut store, vec![contribution_node]);
        (wrapper, fill)
    };

    let wrong_wrapper = compose_tir_slot_resolution_overlay_set(
        &mut registry,
        store_id,
        same_store_child_reference(foreign_store_id, wrapper),
        same_store_template_ref(store_id, fill),
        &string_table,
    );
    assert!(
        wrong_wrapper.is_err(),
        "overlay allocation must reject a wrapper ref from another store before routing"
    );

    let wrong_fill = compose_tir_slot_resolution_overlay_set(
        &mut registry,
        store_id,
        same_store_child_reference(store_id, wrapper),
        same_store_template_ref(foreign_store_id, fill),
        &string_table,
    );
    assert!(
        wrong_fill.is_err(),
        "overlay allocation must reject a fill ref from another store before routing"
    );
}

// -------------------------
//  Registry-Level Composition With Overlay Threading Tests
// -------------------------

// These tests exercise `compose_tir_head_chain_with_overlays` and
// `apply_tir_child_wrappers_with_overlays`, the registry-level entry points that
// run the existing store-local structural composition and then allocate a
// non-empty slot-resolution overlay set from the collected wrapper/fill pairs.
// They confirm that a single slot-bearing wrapper produces a non-empty overlay
// set, that the overlay set carries the slot-resolution dimension, and that
// structural expansion output matches the store-local path.

#[test]
fn head_chain_with_overlays_threads_slot_overlay_for_single_receiver() {
    let mut string_table = StringTable::new();
    let (registry, store_id) = registry_with_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);
        let head_fill = build_text_node(
            &mut store,
            &mut string_table,
            "head fill",
            TemplateSegmentOrigin::Head,
        );
        build_template_with_children(&mut store, vec![wrapper_node, head_fill])
    };

    let original_root = {
        let store = registry.store(store_id).expect("store should exist");
        template_root_node_id(template_id, &store)
    };

    let registry = Rc::new(RefCell::new(registry));
    let composed = compose_tir_head_chain_with_overlays(
        &registry,
        store_id,
        template_id,
        &string_table,
        false,
    )
    .expect("registry-level head-chain composition should succeed");

    assert_ne!(
        composed.root, original_root,
        "composition should produce a new root"
    );

    let overlay_set_id = composed
        .slot_overlay_set_id
        .expect("one slot-bearing wrapper should produce a non-empty overlay set");

    let registry_binding = registry.borrow();
    let overlay_set = registry_binding
        .overlay_set(overlay_set_id)
        .expect("overlay set should be registry-owned");
    assert!(
        overlay_set.slot_resolution.is_some(),
        "overlay set should carry a slot-resolution overlay"
    );
    assert!(
        overlay_set.expression_overrides.is_none(),
        "no expression overlay dimension should be set"
    );
    assert!(
        overlay_set.wrapper_context.is_none(),
        "no wrapper-context overlay dimension should be set"
    );
}

#[test]
fn head_chain_preserves_the_effective_wrapper_view_identity() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: Vec::new(),
    });
    let wrapper_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let (template_id, wrapper_node_id, wrapper_reference) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let wrapper_reference = TemplateTirChildReference::same_store(
            wrapper,
            store_id,
            TemplateTirPhase::Formatted,
            wrapper_overlay_set_id,
        );
        let wrapper_node_id =
            build_child_template_node_with_reference(&mut store, wrapper_reference);
        let body_fill = build_text_node(
            &mut store,
            &mut string_table,
            "body fill",
            TemplateSegmentOrigin::Body,
        );
        let template_id =
            build_template_with_children(&mut store, vec![wrapper_node_id, body_fill]);
        (template_id, wrapper_node_id, wrapper_reference)
    };

    let registry = Rc::new(RefCell::new(registry));
    let composed = compose_tir_head_chain_with_overlays(
        &registry,
        store_id,
        template_id,
        &string_table,
        false,
    )
    .expect("same-store effective wrapper identity should compose");
    assert!(
        composed.slot_overlay_set_id.is_some(),
        "slot-bearing effective wrapper should reach overlay allocation"
    );

    let registry_binding = registry.borrow();
    let store = registry_binding
        .store(store_id)
        .expect("store should exist");
    let source_wrapper_node = store
        .get_node(wrapper_node_id)
        .expect("source wrapper node should remain in the store");
    let TemplateIrNodeKind::ChildTemplate { reference, .. } = &source_wrapper_node.kind else {
        panic!("source wrapper should remain a child-template node");
    };
    assert_eq!(
        *reference, wrapper_reference,
        "structural composition must not rewrite the wrapper's phase or overlay identity"
    );
}

#[test]
fn head_chain_with_overlays_returns_none_when_no_slots_resolved() {
    let mut string_table = StringTable::new();
    let (registry, store_id) = registry_with_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        // A template with only body text and no receivers: composition returns
        // the original root and no overlay.
        let body_text = build_text_node(
            &mut store,
            &mut string_table,
            "body",
            TemplateSegmentOrigin::Body,
        );
        build_template_with_children(&mut store, vec![body_text])
    };

    let original_root = {
        let store = registry.store(store_id).expect("store should exist");
        template_root_node_id(template_id, &store)
    };

    let registry = Rc::new(RefCell::new(registry));
    let composed = compose_tir_head_chain_with_overlays(
        &registry,
        store_id,
        template_id,
        &string_table,
        false,
    )
    .expect("registry-level head-chain composition should succeed");

    assert_eq!(
        composed.root, original_root,
        "template with no receivers should return the original root"
    );
    assert!(
        composed.slot_overlay_set_id.is_none(),
        "no slot-bearing wrapper should produce no overlay set"
    );
}

#[test]
fn head_chain_with_overlays_preserves_structural_expansion() {
    let mut string_table = StringTable::new();
    let (registry, store_id) = registry_with_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_text_slot_text_wrapper(&mut store, &mut string_table, SlotKey::Default);
        let wrapper_node = build_child_template_node_for_template(&mut store, wrapper);
        let body_text = build_text_node(
            &mut store,
            &mut string_table,
            "body fill",
            TemplateSegmentOrigin::Body,
        );
        build_template_with_children(&mut store, vec![wrapper_node, body_text])
    };

    // Run the registry-level overlay path.
    let registry = Rc::new(RefCell::new(registry));
    let overlay_composed = compose_tir_head_chain_with_overlays(
        &registry,
        store_id,
        template_id,
        &string_table,
        false,
    )
    .expect("registry-level composition should succeed");

    // Run the store-local structural path on a fresh store with the same shape.
    // The composed root children should match, confirming the overlay path does
    // not alter structural expansion.
    let mut local_store = TemplateIrStore::new();
    let local_wrapper =
        build_text_slot_text_wrapper(&mut local_store, &mut string_table, SlotKey::Default);
    let local_wrapper_node =
        build_child_template_node_for_template(&mut local_store, local_wrapper);
    let local_body_text = build_text_node(
        &mut local_store,
        &mut string_table,
        "body fill",
        TemplateSegmentOrigin::Body,
    );
    let local_template_id =
        build_template_with_children(&mut local_store, vec![local_wrapper_node, local_body_text]);

    let local_composed_root =
        compose_tir_head_chain(&mut local_store, local_template_id, &string_table, false)
            .expect("store-local composition should succeed");

    let registry_binding = registry.borrow();
    let store = registry_binding
        .store(store_id)
        .expect("store should exist");
    let overlay_child_count = root_child_kinds_for_node(overlay_composed.root, &store).len();
    let local_child_count = root_child_kinds_for_node(local_composed_root, &local_store).len();

    assert_eq!(
        overlay_child_count, local_child_count,
        "overlay path should produce the same number of root children"
    );
}

#[test]
fn child_wrappers_with_overlays_threads_slot_overlay_for_slot_bearing_wrapper() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (parent, wrapper) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let child = build_single_text_template(&mut store, &mut string_table, "child");
        let child_node = build_child_template_node_for_template(&mut store, child);
        let parent = build_template_with_children(&mut store, vec![child_node]);
        (parent, wrapper)
    };

    let original_root = {
        let store = registry.store(store_id).expect("store should exist");
        template_root_node_id(parent, &store)
    };

    let composed = apply_tir_child_wrappers_with_overlays(
        &mut registry,
        store_id,
        parent,
        &[wrapper],
        &string_table,
    )
    .expect("registry-level child-wrapper composition should succeed");

    assert_ne!(
        composed.root, original_root,
        "wrapper application should produce a new root"
    );

    let overlay_set_id = composed
        .slot_overlay_set_id
        .expect("one slot-bearing wrapper should produce a non-empty overlay set");

    let overlay_set = registry
        .overlay_set(overlay_set_id)
        .expect("overlay set should be registry-owned");
    assert!(
        overlay_set.slot_resolution.is_some(),
        "overlay set should carry a slot-resolution overlay"
    );
}

#[test]
fn child_wrappers_with_overlays_merges_multiple_slot_bearing_wrappers() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (parent, first_wrapper, second_wrapper) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let first_wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let second_wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let child = build_single_text_template(&mut store, &mut string_table, "child");
        let child_node = build_child_template_node_for_template(&mut store, child);
        let parent = build_template_with_children(&mut store, vec![child_node]);
        (parent, first_wrapper, second_wrapper)
    };

    let composed = apply_tir_child_wrappers_with_overlays(
        &mut registry,
        store_id,
        parent,
        &[first_wrapper, second_wrapper],
        &string_table,
    )
    .expect("registry-level child-wrapper composition should succeed");

    let overlay_set_id = composed
        .slot_overlay_set_id
        .expect("slot-bearing wrappers should produce one merged overlay set");
    let overlay_set = registry
        .overlay_set(overlay_set_id)
        .expect("overlay set should be registry-owned");
    let slot_overlay_id = overlay_set
        .slot_resolution
        .expect("merged overlay set should carry slot resolution");
    let slot_overlay = registry
        .slot_resolution_overlay(slot_overlay_id)
        .expect("slot overlay should be registry-owned");

    assert_eq!(
        slot_overlay.resolutions.len(),
        2,
        "two slot-bearing wrappers should merge into one payload"
    );

    for (_, resolution) in &slot_overlay.resolutions {
        assert!(
            matches!(resolution.kind, TirSlotResolutionKind::Resolved { .. }),
            "each wrapper slot should resolve to the current wrapped child"
        );
    }
}

#[test]
fn child_wrappers_with_overlays_returns_none_for_slot_less_wrapper() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (parent, wrapper) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        // A wrapper without slots: structural composition prepends the wrapper
        // content, but no slot-resolution overlay is allocated.
        let wrapper = build_single_text_template(&mut store, &mut string_table, "prefix");
        let child = build_single_text_template(&mut store, &mut string_table, "child");
        let child_node = build_child_template_node_for_template(&mut store, child);
        let parent = build_template_with_children(&mut store, vec![child_node]);
        (parent, wrapper)
    };

    let composed = apply_tir_child_wrappers_with_overlays(
        &mut registry,
        store_id,
        parent,
        &[wrapper],
        &string_table,
    )
    .expect("registry-level child-wrapper composition should succeed");

    assert!(
        composed.slot_overlay_set_id.is_none(),
        "slot-less wrapper should produce no overlay set"
    );
}

#[test]
fn child_wrappers_with_overlays_preserves_structural_expansion() {
    let mut string_table = StringTable::new();
    let (mut registry, store_id) = registry_with_store();

    let (parent, wrapper) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_wrapper_with_slot_sequence(&mut store, vec![SlotKey::Default]);
        let child = build_single_text_template(&mut store, &mut string_table, "child");
        let child_node = build_child_template_node_for_template(&mut store, child);
        let parent = build_template_with_children(&mut store, vec![child_node]);
        (parent, wrapper)
    };

    let overlay_composed = apply_tir_child_wrappers_with_overlays(
        &mut registry,
        store_id,
        parent,
        &[wrapper],
        &string_table,
    )
    .expect("registry-level composition should succeed");

    // Run the store-local structural path on a fresh store with the same shape.
    let mut local_store = TemplateIrStore::new();
    let local_wrapper = build_wrapper_with_slot_sequence(&mut local_store, vec![SlotKey::Default]);
    let local_child = build_single_text_template(&mut local_store, &mut string_table, "child");
    let local_child_node = build_child_template_node_for_template(&mut local_store, local_child);
    let local_parent = build_template_with_children(&mut local_store, vec![local_child_node]);

    let local_composed_root = apply_tir_child_wrappers(
        &mut local_store,
        local_parent,
        &[local_wrapper],
        &string_table,
    )
    .expect("store-local wrapper application should succeed");

    let store = registry.store(store_id).expect("store should exist");
    let overlay_child_count = root_child_kinds_for_node(overlay_composed.root, &store).len();
    let local_child_count = root_child_kinds_for_node(local_composed_root, &local_store).len();

    assert_eq!(
        overlay_child_count, local_child_count,
        "overlay path should produce the same number of root children"
    );
}

// -------------------------
//  Cross-Store Head-Chain Composition Tests
// -------------------------

/// Builds a registry with two stores so tests can exercise cross-store wrapper
/// references. The first store is the composition store; the second store
/// holds foreign wrapper templates.
fn registry_with_two_stores() -> (TemplateIrRegistry, TemplateStoreId, TemplateStoreId) {
    let mut registry = TemplateIrRegistry::new();
    let composition_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    (registry, composition_store_id, foreign_store_id)
}

/// Fixture for cross-store head-chain composition tests.
///
/// WHAT: holds the wrapped registry, composition store ID, foreign wrapper
///       template ID, composition template ID, original root, and string table
///       so the three cross-store tests share one setup path without repeating
///       registry/store/template construction.
struct CrossStoreHeadChainFixture {
    registry: Rc<RefCell<TemplateIrRegistry>>,
    composition_store_id: TemplateStoreId,
    foreign_wrapper: TemplateIrId,
    template_id: TemplateIrId,
    original_root: TemplateIrNodeId,
    string_table: StringTable,
}

/// Builds a cross-store head-chain fixture: a foreign slot-bearing wrapper and
/// a composition template whose head references that wrapper with the given
/// phase and overlay-set identity, followed by body fill text.
///
/// WHAT: the caller supplies a registry (with any pre-allocated overlays), the
///       store IDs, the wrapper reference phase and overlay-set ID, and the
///       body fill text. The builder constructs a default-slot wrapper in the
///       foreign store, a composition template in the composition store, and
///       wraps the registry in `Rc<RefCell>`.
/// WHY: the three cross-store tests differ only in the wrapper reference
///      identity and the body text. Sharing the setup avoids duplicating
///      registry/store/template construction while keeping each test's
///      assertions self-contained.
fn build_cross_store_head_chain_fixture(
    registry: TemplateIrRegistry,
    composition_store_id: TemplateStoreId,
    foreign_store_id: TemplateStoreId,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
    body_fill_text: &str,
) -> CrossStoreHeadChainFixture {
    let mut string_table = StringTable::new();

    let foreign_wrapper = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        build_wrapper_with_slot_sequence(&mut foreign_store, vec![SlotKey::Default])
    };

    let wrapper_reference = TemplateTirChildReference::same_store(
        foreign_wrapper,
        foreign_store_id,
        phase,
        overlay_set_id,
    );

    let template_id = {
        let mut store = registry
            .store_mut(composition_store_id)
            .expect("composition store should be mutable");
        let wrapper_node = build_child_template_node_with_reference(&mut store, wrapper_reference);
        let body_fill = build_text_node(
            &mut store,
            &mut string_table,
            body_fill_text,
            TemplateSegmentOrigin::Body,
        );
        build_template_with_children(&mut store, vec![wrapper_node, body_fill])
    };

    let original_root = {
        let store = registry
            .store(composition_store_id)
            .expect("composition store should exist");
        template_root_node_id(template_id, &store)
    };

    CrossStoreHeadChainFixture {
        registry: Rc::new(RefCell::new(registry)),
        composition_store_id,
        foreign_wrapper,
        template_id,
        original_root,
        string_table,
    }
}

/// Composes a head-chain with a foreign slot-bearing wrapper and a default slot,
/// verifying that the overlay-only path resolves the wrapper through its owning
/// store without copying the foreign tree.
#[test]
fn head_chain_composes_foreign_wrapper_with_default_slot() {
    let (registry, composition_store_id, foreign_store_id) = registry_with_two_stores();

    let fixture = build_cross_store_head_chain_fixture(
        registry,
        composition_store_id,
        foreign_store_id,
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
        "foreign fill",
    );

    let composed = compose_tir_head_chain_with_overlays(
        &fixture.registry,
        fixture.composition_store_id,
        fixture.template_id,
        &fixture.string_table,
        false,
    )
    .expect("cross-store head-chain composition should succeed");

    // The composed root must differ from the original because the cross-store
    // wrapper was resolved through the overlay-only path, producing a new
    // ChildTemplate node that carries the slot-resolution overlay set.
    assert!(
        composed.root != fixture.original_root,
        "cross-store composition should produce a new root"
    );

    // The template-level slot_overlay_set_id should be None because the
    // cross-store overlay is attached to the individual ChildTemplate node.
    assert!(
        composed.slot_overlay_set_id.is_none(),
        "cross-store overlay should be on the ChildTemplate node, not the template-level set"
    );

    let registry_binding = fixture.registry.borrow();
    let store = registry_binding
        .store(fixture.composition_store_id)
        .expect("composition store should exist");
    let child_kinds = root_child_kinds_for_node(composed.root, &store);

    assert_eq!(
        child_kinds.len(),
        1,
        "composed root should have one child (the resolved foreign wrapper)"
    );

    let TemplateIrNodeKind::ChildTemplate { reference, .. } = child_kinds[0] else {
        panic!("composed child should be a ChildTemplate node");
    };

    assert_eq!(
        reference.root.store_id, foreign_store_id,
        "composed reference should retain the foreign wrapper's store"
    );
    assert_eq!(
        reference.root.template_id, fixture.foreign_wrapper,
        "composed reference should retain the foreign wrapper's template ID"
    );

    // The overlay set on the composed reference should carry a slot-resolution
    // overlay with one resolution for the default slot.
    let overlay_set = registry_binding
        .overlay_set(reference.overlay_set_id)
        .expect("overlay set should be registry-owned");
    let slot_overlay_id = overlay_set
        .slot_resolution
        .expect("overlay set should carry a slot-resolution overlay");
    let slot_overlay = registry_binding
        .slot_resolution_overlay(slot_overlay_id)
        .expect("slot overlay should be registry-owned");

    assert_eq!(
        slot_overlay.resolutions.len(),
        1,
        "foreign wrapper with one default slot should have one resolution"
    );

    for (_, resolution) in &slot_overlay.resolutions {
        assert!(
            matches!(resolution.kind, TirSlotResolutionKind::Resolved { .. }),
            "default slot should resolve to the body fill"
        );
    }
}

/// Verifies that a foreign wrapper with a specific phase and overlay set
/// preserves that identity on the composed ChildTemplate node, with the
/// slot-resolution overlay merged into the overlay set alongside the
/// pre-existing expression overlay.
#[test]
fn head_chain_preserves_foreign_wrapper_phase_and_overlay_identity() {
    let (mut registry, composition_store_id, foreign_store_id) = registry_with_two_stores();

    // Allocate a non-empty expression overlay on the registry so the wrapper
    // carries a distinct overlay identity.
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: Vec::new(),
    });
    let wrapper_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let fixture = build_cross_store_head_chain_fixture(
        registry,
        composition_store_id,
        foreign_store_id,
        TemplateTirPhase::Formatted,
        wrapper_overlay_set_id,
        "fill content",
    );

    let composed = compose_tir_head_chain_with_overlays(
        &fixture.registry,
        fixture.composition_store_id,
        fixture.template_id,
        &fixture.string_table,
        false,
    )
    .expect("cross-store head-chain composition should succeed");

    let registry_binding = fixture.registry.borrow();
    let store = registry_binding
        .store(fixture.composition_store_id)
        .expect("composition store should exist");
    let child_kinds = root_child_kinds_for_node(composed.root, &store);

    assert_eq!(child_kinds.len(), 1, "composed root should have one child");

    let TemplateIrNodeKind::ChildTemplate { reference, .. } = child_kinds[0] else {
        panic!("composed child should be a ChildTemplate node");
    };

    // The composed reference must preserve the foreign wrapper's root and phase.
    assert_eq!(
        reference.root.store_id, foreign_store_id,
        "composed reference should retain the foreign wrapper's store"
    );
    assert_eq!(
        reference.root.template_id, fixture.foreign_wrapper,
        "composed reference should retain the foreign wrapper's template ID"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Formatted,
        "composed reference should preserve the wrapper's Formatted phase"
    );

    // The composed overlay set must carry both the slot-resolution overlay
    // (newly allocated) and the original expression overlay ID (preserved
    // from the wrapper's pre-existing overlay set). This proves every
    // pre-existing overlay dimension survives composition.
    let overlay_set = registry_binding
        .overlay_set(reference.overlay_set_id)
        .expect("overlay set should be registry-owned");
    assert!(
        overlay_set.slot_resolution.is_some(),
        "composed overlay set should carry a slot-resolution overlay"
    );
    assert_eq!(
        overlay_set.expression_overrides,
        Some(expression_overlay_id),
        "composed overlay set should preserve the wrapper's expression overlay ID"
    );
}

/// Verifies that a missing foreign store produces a precise internal
/// diagnostic rather than a content fallback or silent success.
#[test]
fn head_chain_foreign_wrapper_missing_store_produces_diagnostic() {
    let mut string_table = StringTable::new();
    let (registry, composition_store_id) = registry_with_store();

    // Build a head-origin child referencing a non-existent foreign store.
    let template_id = {
        let mut store = registry
            .store_mut(composition_store_id)
            .expect("composition store should be mutable");
        let bogus_store_id = TemplateStoreId::new(999);
        let wrapper_reference = TemplateTirChildReference::same_store(
            TemplateIrId::new(0),
            bogus_store_id,
            TemplateTirPhase::Parsed,
            TemplateOverlaySetId::empty(),
        );
        let wrapper_node = build_child_template_node_with_reference(&mut store, wrapper_reference);
        let body_fill = build_text_node(
            &mut store,
            &mut string_table,
            "fill",
            TemplateSegmentOrigin::Body,
        );
        build_template_with_children(&mut store, vec![wrapper_node, body_fill])
    };

    let registry = Rc::new(RefCell::new(registry));
    let result = compose_tir_head_chain_with_overlays(
        &registry,
        composition_store_id,
        template_id,
        &string_table,
        false,
    );

    assert!(
        result.is_err(),
        "missing foreign store should produce a diagnostic, not a silent success"
    );

    let diagnostic = result.unwrap_err();
    let CompilerDiagnostic {
        payload: DiagnosticPayload::InfrastructureError { msg, .. },
        ..
    } = diagnostic.as_ref()
    else {
        panic!(
            "expected an InfrastructureError diagnostic for missing foreign store, got: {:?}",
            diagnostic.payload
        );
    };
    assert!(
        msg.contains("cross-store child template store was not present in the registry"),
        "diagnostic message should report the missing foreign store, got: {msg}",
    );
}
