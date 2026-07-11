//! Tests for TIR-native inherited child-wrapper application on control-flow
//! body roots.
//!
//! WHAT: exercises `apply_inherited_child_wrappers_to_body_root` directly to
//!       prove that branch/fallback body roots can inherit `$children(..)`
//!       wrappers without falling back to the atom-level content mirror.
//! WHY: the render-unit TIR path previously bailed out whenever the owning
//!      template carried inherited wrappers; these tests pin the replacement
//!      behavior for direct children, `$fresh` suppression, and control-flow
//!      children.

use super::super::builder::TemplateIrBuilder;
use super::super::ids::{TemplateIrId, TemplateIrNodeId};
use super::super::node::{TemplateIrBranch, TemplateIrNodeKind};
use super::super::overlays::{TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay};
use super::super::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference, TemplateWrapperReference,
};
use super::super::registry::TemplateIrRegistry;
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::TemplateTirPhase;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
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
        TemplateIrNodeKind::ChildTemplate { reference, .. } => reference.root.template_id,
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

    let registry = TemplateIrRegistry::new();
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            TemplateRef::new(store.store_id(), wrapper),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &registry,
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

    let registry = TemplateIrRegistry::new();
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            TemplateRef::new(store.store_id(), wrapper),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &registry,
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

    let registry = TemplateIrRegistry::new();
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            TemplateRef::new(store.store_id(), wrapper),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &registry,
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

    let registry = TemplateIrRegistry::new();
    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[TemplateWrapperReference::new(
            TemplateRef::new(store.store_id(), wrapper),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        )],
        &registry,
        &mut store,
        &string_table,
    )
    .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, body_root,
        "slot-bearing child should be left for head-chain composition"
    );
}

// -----------------------------
//  Cross-store direct children
// -----------------------------

fn build_cross_store_registry() -> (TemplateIrRegistry, TemplateStoreId, TemplateStoreId) {
    let mut registry = TemplateIrRegistry::new();
    let empty_overlay = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    assert_eq!(empty_overlay, TemplateOverlaySetId::empty());

    let foreign_store_id = registry.allocate_store();
    let local_store_id = registry.allocate_store();
    (registry, foreign_store_id, local_store_id)
}

fn foreign_child_reference(
    foreign_store_id: TemplateStoreId,
    foreign_child_id: TemplateIrId,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
) -> TemplateTirChildReference {
    TemplateTirChildReference::new(
        TemplateRef::new(foreign_store_id, foreign_child_id),
        phase,
        overlay_set_id,
    )
}

fn build_slot_tir_template(store: &mut TemplateIrStore) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    builder.finish_template(
        slot_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary {
            slot_count: 1,
            has_slots: true,
            ..TemplateIrSummary::default()
        },
        empty_location(),
    )
}

fn apply_wrappers_to_foreign_child(
    registry: &TemplateIrRegistry,
    local_store_id: TemplateStoreId,
    child_reference: TemplateTirChildReference,
    string_table: &mut StringTable,
) -> Result<(TemplateIrNodeId, TemplateIrNodeId), TemplateError> {
    let local_store_rc = registry
        .store_handle(local_store_id)
        .expect("local store should be registered");
    let mut local_store = local_store_rc.borrow_mut();

    let wrapper = build_single_text_tir_template(&mut local_store, string_table, "wrapper-prefix");
    let child_node = TemplateIrBuilder::new(&mut local_store)
        .push_child_template_node_with_reference(child_reference, empty_location());
    let body_root = build_body_root_from_children(&mut local_store, vec![child_node]);
    let wrapper_reference = TemplateWrapperReference::new(
        TemplateRef::new(local_store_id, wrapper),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    );

    let wrapped_root = apply_inherited_child_wrappers_to_body_root(
        body_root,
        &[wrapper_reference],
        registry,
        &mut local_store,
        string_table,
    )?;

    Ok((body_root, wrapped_root))
}

fn assert_infrastructure_error(
    result: Result<(TemplateIrNodeId, TemplateIrNodeId), TemplateError>,
    expected_message: &str,
) {
    let compiler_error = match result {
        Err(TemplateError::Infrastructure(compiler_error)) => compiler_error,
        Err(other) => panic!("expected an infrastructure error, got: {other:?}"),
        Ok(_) => panic!("expected inherited-wrapper application to fail"),
    };

    assert!(
        compiler_error.msg.contains(expected_message),
        "expected error to contain {expected_message:?}, got: {}",
        compiler_error.msg
    );
}

#[test]
fn body_root_skips_foreign_fresh_direct_child() {
    let mut string_table = StringTable::new();
    let (registry, foreign_store_id, local_store_id) = build_cross_store_registry();

    let foreign_child_id = {
        let foreign_store_rc = registry
            .store_handle(foreign_store_id)
            .expect("foreign store should be registered");
        let mut foreign_store = foreign_store_rc.borrow_mut();
        build_text_tir_template(
            &mut foreign_store,
            &mut string_table,
            "fresh-foreign",
            Style {
                skip_parent_child_wrappers: true,
                ..Style::default()
            },
            TemplateIrSummary::default(),
        )
    };
    let child_reference = foreign_child_reference(
        foreign_store_id,
        foreign_child_id,
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    );

    let (body_root, wrapped_root) = apply_wrappers_to_foreign_child(
        &registry,
        local_store_id,
        child_reference,
        &mut string_table,
    )
    .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, body_root,
        "foreign $fresh child should remain unwrapped"
    );
}

#[test]
fn body_root_leaves_foreign_slot_bearing_child_unwrapped() {
    let mut string_table = StringTable::new();
    let (registry, foreign_store_id, local_store_id) = build_cross_store_registry();

    let foreign_child_id = {
        let foreign_store_rc = registry
            .store_handle(foreign_store_id)
            .expect("foreign store should be registered");
        let mut foreign_store = foreign_store_rc.borrow_mut();
        build_slot_tir_template(&mut foreign_store)
    };
    let child_reference = foreign_child_reference(
        foreign_store_id,
        foreign_child_id,
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    );

    let (body_root, wrapped_root) = apply_wrappers_to_foreign_child(
        &registry,
        local_store_id,
        child_reference,
        &mut string_table,
    )
    .expect("wrapper application should succeed");

    assert_eq!(
        wrapped_root, body_root,
        "foreign slot-bearing child should remain a head-chain receiver"
    );
}

#[test]
fn body_root_wraps_foreign_control_flow_child_preserving_reference_identity() {
    let mut string_table = StringTable::new();
    let (mut registry, foreign_store_id, local_store_id) = build_cross_store_registry();

    let foreign_child_id = {
        let foreign_store_rc = registry
            .store_handle(foreign_store_id)
            .expect("foreign store should be registered");
        let mut foreign_store = foreign_store_rc.borrow_mut();
        build_control_flow_child_template(&mut foreign_store, &mut string_table, "output")
    };
    let expression_overlay = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: Vec::new(),
    });
    let foreign_overlay = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay),
        ..TemplateOverlaySet::empty()
    });
    assert_ne!(foreign_overlay, TemplateOverlaySetId::empty());

    let child_reference = foreign_child_reference(
        foreign_store_id,
        foreign_child_id,
        TemplateTirPhase::Formatted,
        foreign_overlay,
    );
    let (_, wrapped_root) = apply_wrappers_to_foreign_child(
        &registry,
        local_store_id,
        child_reference,
        &mut string_table,
    )
    .expect("wrapper application should succeed");

    let local_store_rc = registry
        .store_handle(local_store_id)
        .expect("local store should be registered");
    let local_store = local_store_rc.borrow();
    let children = root_sequence_children(wrapped_root, &local_store);
    assert_eq!(children.len(), 1);

    let wrapper_template_id = expect_child_template_id(children[0], &local_store);
    let wrapper_template = local_store
        .get_template(wrapper_template_id)
        .expect("derived wrapper template should exist");
    assert!(wrapper_template.conditional_child_wrapper_set.is_some());

    let inner_child_node = local_store
        .get_node(wrapper_template.root)
        .expect("derived wrapper root should exist");
    let TemplateIrNodeKind::ChildTemplate { reference, .. } = &inner_child_node.kind else {
        panic!("expected a ChildTemplate inside the derived wrapper");
    };
    assert_eq!(
        *reference, child_reference,
        "derived wrapper should preserve the foreign root, phase and overlay set"
    );
}

#[test]
fn body_root_foreign_child_missing_store_returns_precise_error() {
    let mut string_table = StringTable::new();
    let (registry, _, local_store_id) = build_cross_store_registry();
    let child_reference = foreign_child_reference(
        TemplateStoreId::new(99),
        TemplateIrId::new(0),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    );

    let result = apply_wrappers_to_foreign_child(
        &registry,
        local_store_id,
        child_reference,
        &mut string_table,
    );

    assert_infrastructure_error(result, "not in the module-local TIR registry");
}

#[test]
fn body_root_foreign_child_missing_template_returns_precise_error() {
    let mut string_table = StringTable::new();
    let (registry, foreign_store_id, local_store_id) = build_cross_store_registry();
    let child_reference = foreign_child_reference(
        foreign_store_id,
        TemplateIrId::new(99),
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    );

    let result = apply_wrappers_to_foreign_child(
        &registry,
        local_store_id,
        child_reference,
        &mut string_table,
    );

    assert_infrastructure_error(result, "missing in store");
}
