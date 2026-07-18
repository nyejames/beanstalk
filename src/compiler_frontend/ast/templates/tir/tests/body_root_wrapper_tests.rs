//! Tests for TIR-native inherited child-wrapper application on control-flow
//! body roots.
//!
//! WHAT: exercises `apply_inherited_child_wrappers_to_body_root` directly to
//!       prove that branch/fallback body roots can inherit `$children(..)`
//!       wrappers through the authoritative body-root TIR.
//! WHY: these tests protect direct children, `$fresh` suppression and
//!      control-flow children at the render-unit boundary.

use super::super::builder::TemplateIrBuilder;
use super::super::ids::{TemplateIrId, TemplateIrNodeId};
use super::super::node::{TemplateIrBranch, TemplateIrNodeKind};
use super::super::overlays::TemplateOverlaySetId;
use super::super::refs::TemplateWrapperReference;
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::TemplateTirPhase;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
use crate::compiler_frontend::ast::templates::tir::apply_inherited_child_wrappers_to_body_root;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

fn string_id(
    string_table: &mut StringTable,
    text: &str,
) -> crate::compiler_frontend::symbols::string_interning::StringId {
    string_table.intern(text)
}

fn build_single_text_tir_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    build_text_tir_template(
        store,
        string_table,
        text,
        Style::default(),
        TemplateIrSummary::default(),
    )
}

fn build_text_tir_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
    style: Style,
    summary: TemplateIrSummary,
) -> TemplateIrId {
    let text_id = string_id(string_table, text);
    let text_len = u32::try_from(string_table.resolve(text_id).len()).unwrap_or(u32::MAX);
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_text_node(
        text_id,
        text_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    builder.finish_template(root, style, TemplateType::String, summary, empty_location())
}

fn build_child_template_node(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
) -> TemplateIrNodeId {
    let mut builder = TemplateIrBuilder::new(store);
    builder.push_child_template_node(template_id, empty_location())
}

fn build_body_root_from_children(
    store: &mut TemplateIrStore,
    children: Vec<TemplateIrNodeId>,
) -> TemplateIrNodeId {
    let mut builder = TemplateIrBuilder::new(store);
    builder.push_sequence_node(children, empty_location())
}

fn build_control_flow_child_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    branch_output_text: &str,
) -> TemplateIrId {
    let branch_body = build_single_text_tir_template(store, string_table, branch_output_text);
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression(true)),
        store
            .get_template(branch_body)
            .expect("branch body exists")
            .root,
        empty_location(),
    );

    let mut builder = TemplateIrBuilder::new(store);
    let branch_chain = builder.push_branch_chain_node(vec![branch], None, empty_location());

    let summary = TemplateIrSummary {
        has_control_flow: true,
        is_const_evaluable_shape: false,
        ..TemplateIrSummary::default()
    };

    builder.finish_template(
        branch_chain,
        Style::default(),
        TemplateType::StringFunction,
        summary,
        empty_location(),
    )
}

fn expect_child_template_id(node_id: TemplateIrNodeId, store: &TemplateIrStore) -> TemplateIrId {
    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference.root,
        other => panic!("expected ChildTemplate node, found {other:?}"),
    }
}

fn root_sequence_children(
    node_id: TemplateIrNodeId,
    store: &TemplateIrStore,
) -> Vec<TemplateIrNodeId> {
    let node = store.get_node(node_id).expect("node should exist");
    match &node.kind {
        TemplateIrNodeKind::Sequence { children } => children.clone(),
        other => panic!("expected Sequence node, found {other:?}"),
    }
}

#[test]
fn body_root_wraps_non_control_flow_direct_child() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_single_text_tir_template(&mut store, &mut string_table, "prefix-");
    let child = build_single_text_tir_template(&mut store, &mut string_table, "child");
    let child_node = build_child_template_node(&mut store, child);
    let body_root = build_body_root_from_children(&mut store, vec![child_node]);
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            wrapper,
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &mut store,
        &string_table,
    )
    .expect("wrapper application should succeed");

    let children = root_sequence_children(wrapped_root, &store);
    assert_eq!(children.len(), 1);

    let wrapped_template_id = expect_child_template_id(children[0], &store);
    let wrapped_children = root_sequence_children(
        store
            .get_template(wrapped_template_id)
            .expect("wrapper template exists")
            .root,
        &store,
    );
    assert_eq!(
        wrapped_children.len(),
        2,
        "slot-less wrapper prepends its content before the child"
    );
    assert_eq!(
        expect_child_template_id(wrapped_children[1], &store),
        child,
        "wrapper should contain the original child template after the wrapper prefix"
    );
}

#[test]
fn body_root_skips_fresh_direct_child() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_single_text_tir_template(&mut store, &mut string_table, "prefix-");
    let child_tir_id = build_text_tir_template(
        &mut store,
        &mut string_table,
        "fresh-child",
        Style {
            skip_parent_child_wrappers: true,
            ..Style::default()
        },
        TemplateIrSummary::default(),
    );
    let child_node = build_child_template_node(&mut store, child_tir_id);
    let body_root = build_body_root_from_children(&mut store, vec![child_node]);
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            wrapper,
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &mut store,
        &string_table,
    )
    .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, body_root,
        "$fresh child should not be wrapped and root should be unchanged"
    );
}

#[test]
fn body_root_wraps_control_flow_direct_child_with_conditional_wrappers() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_single_text_tir_template(&mut store, &mut string_table, "wrapped-");
    let control_flow_child =
        build_control_flow_child_template(&mut store, &mut string_table, "output");
    let child_node = build_child_template_node(&mut store, control_flow_child);
    let body_root = build_body_root_from_children(&mut store, vec![child_node]);
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            wrapper,
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &mut store,
        &string_table,
    )
    .expect("wrapper application should succeed");

    let children = root_sequence_children(wrapped_root, &store);
    assert_eq!(children.len(), 1);

    let wrapper_template_id = expect_child_template_id(children[0], &store);
    let wrapper_template = store
        .get_template(wrapper_template_id)
        .expect("wrapper template should exist");

    assert!(
        wrapper_template.conditional_child_wrapper_set.is_some(),
        "control-flow child wrapper should set conditional_child_wrapper_set"
    );

    assert_eq!(
        expect_child_template_id(wrapper_template.root, &store),
        control_flow_child,
        "wrapper root should be a single ChildTemplate reference to the original control-flow child"
    );
}

#[test]
fn body_root_leaves_slot_bearing_child_unwrapped() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let wrapper = build_single_text_tir_template(&mut store, &mut string_table, "prefix-");
    let mut builder = TemplateIrBuilder::new(&mut store);
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    let slot_template = builder.finish_template(
        slot_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary {
            slot_count: 1,
            has_slots: true,
            ..TemplateIrSummary::default()
        },
        empty_location(),
    );
    let slot_child_node = build_child_template_node(&mut store, slot_template);
    let body_root = build_body_root_from_children(&mut store, vec![slot_child_node]);
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            wrapper,
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &mut store,
        &string_table,
    )
    .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, body_root,
        "slot-bearing child should be left for head-chain composition"
    );
}

// ---------------------
//  Body-root authority
// ---------------------

#[test]
fn apply_inherited_wrappers_rejects_missing_body_root_with_empty_wrappers() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let missing_root = TemplateIrNodeId::new(99);
    let result =
        apply_inherited_child_wrappers_to_body_root(missing_root, &[], &mut store, &string_table);

    assert!(
        result.is_err(),
        "missing body root should be rejected even with empty wrappers"
    );
}

#[test]
fn apply_inherited_wrappers_rejects_non_sequence_body_root_with_empty_wrappers() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let text = build_single_text_tir_template(&mut store, &mut string_table, "leaf");
    // A bare template root node is a Text node, not a Sequence.
    let result = apply_inherited_child_wrappers_to_body_root(
        store.get_template(text).expect("template exists").root,
        &[],
        &mut store,
        &string_table,
    );

    assert!(
        result.is_err(),
        "non-sequence body root should be rejected even with empty wrappers"
    );
}
