//! Foreign SlotInsert proxy construction and routing tests.
//!
//! WHAT: proves that a cross-store `SlotInsert` head is proxied into the current
//!       store as an `InsertContribution` node, that the proxy preserves
//!       identity (no deep copy), and that both simple and nested recursive
//!       routing through `route_tir_slot_contributions` match the same-store
//!       contract.
//!
//! WHY: the proxy path is the only way a foreign `SlotInsert` can participate in
//!      TIR-native slot routing — `InsertContribution` carries a bare local
//!      `TemplateIrId` and cannot represent a foreign reference directly. These
//!      tests establish the identity, routing, and recursive-discovery
//!      invariants that the proxy must preserve.

use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ReactiveSource, ReactiveSourceKind,
};
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId,
};
use crate::compiler_frontend::ast::templates::tir::refs::{TemplateRef, TemplateStoreId};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::render_unit::build_aggregate_wrapper_candidate_from_tir_nodes;
use crate::compiler_frontend::ast::templates::tir::slot_composition::route_tir_slot_contributions;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::tir::{TemplateIrStoreOwner, TemplateTirReference};
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use std::sync::Arc;

// -------------------------
//  Local test helpers
// -------------------------

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn push_text_node(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrNodeId {
    let interned = string_table.intern(text);
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: interned,
            byte_len: text.len() as u32,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ))
}

fn push_dynamic_expression(
    store: &mut TemplateIrStore,
    expression: Expression,
) -> TemplateIrNodeId {
    let site_id = store.next_expression_site_id();
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(expression),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id,
        },
        empty_location(),
    ))
}

/// Builds a `Template` with a `SlotInsert` kind and a foreign `tir_reference`.
fn foreign_slot_insert_template(
    foreign_store_id: TemplateStoreId,
    foreign_template_id: TemplateIrId,
    store_owner: Arc<TemplateIrStoreOwner>,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
    slot_key: SlotKey,
) -> Template {
    Template {
        kind: TemplateType::SlotInsert(slot_key),
        tir_reference: TemplateTirReference {
            root: TemplateRef::new(foreign_store_id, foreign_template_id),
            store_owner,
            is_composed: false,
            phase,
            overlay_set_id,
        },
        id: String::new(),
        location: empty_location(),
    }
}

/// Extracts the `InsertContribution` template ID from a built aggregate
/// wrapper candidate, panicking if none is found.
fn first_insert_contribution_template_id(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> TemplateIrId {
    let template_ir = store
        .get_template(template_id)
        .expect("built template should exist");
    let root = store
        .get_node(template_ir.root)
        .expect("root node should exist");

    let TemplateIrNodeKind::Sequence { children } = &root.kind else {
        panic!("expected Sequence root, got {:?}", root.kind);
    };

    for &child_id in children {
        let child = store.get_node(child_id).expect("child node should exist");
        if let TemplateIrNodeKind::InsertContribution { template } = &child.kind {
            return *template;
        }
    }

    panic!("no InsertContribution node found in built aggregate wrapper");
}

/// Returns the node ID of the first `InsertContribution` child in the given
/// template's root sequence.
fn first_insert_contribution_node_id(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> TemplateIrNodeId {
    let template_ir = store
        .get_template(template_id)
        .expect("built template should exist");
    let root = store
        .get_node(template_ir.root)
        .expect("root node should exist");

    let TemplateIrNodeKind::Sequence { children } = &root.kind else {
        panic!("expected Sequence root, got {:?}", root.kind);
    };

    for &child_id in children {
        let child = store.get_node(child_id).expect("child node should exist");
        if let TemplateIrNodeKind::InsertContribution { .. } = &child.kind {
            return child_id;
        }
    }

    panic!("no InsertContribution node found in template");
}

/// Builds a wrapper template whose root is a `Sequence` of `$slot` placeholders
/// for the given slot keys.
fn build_wrapper_with_named_slots(
    store: &mut TemplateIrStore,
    slot_keys: &[SlotKey],
) -> TemplateIrId {
    let mut slot_nodes = Vec::with_capacity(slot_keys.len());
    for key in slot_keys {
        let occurrence_id = store.next_slot_occurrence_id();
        let placeholder = TirSlotPlaceholder::new(key.clone(), occurrence_id, empty_location());
        let slot_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot { placeholder },
            empty_location(),
        ));
        slot_nodes.push(slot_node);
    }
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: slot_nodes,
        },
        empty_location(),
    ));
    let slot_count = u32::try_from(slot_keys.len()).unwrap_or(u32::MAX);
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary {
            slot_count,
            has_slots: slot_count > 0,
            ..crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary::default()
        },
        empty_location(),
    ))
}

/// Builds a fill template whose root is a `Sequence` of the given node IDs.
fn build_fill_template_from_nodes(
    store: &mut TemplateIrStore,
    nodes: Vec<TemplateIrNodeId>,
) -> TemplateIrId {
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: nodes },
        empty_location(),
    ));
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary::default(),
        empty_location(),
    ))
}

/// Builds a `SlotInsert` template whose root is a `Sequence` of the given body
/// children, carrying the given target slot key.
fn build_slot_insert_template_with_body(
    store: &mut TemplateIrStore,
    target: SlotKey,
    body_children: Vec<TemplateIrNodeId>,
) -> TemplateIrId {
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: body_children,
        },
        empty_location(),
    ));
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::SlotInsert(target),
        crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary::default(),
        empty_location(),
    ))
}

fn push_insert_contribution_node(
    store: &mut TemplateIrStore,
    template_id: TemplateIrId,
) -> TemplateIrNodeId {
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::InsertContribution {
            template: template_id,
        },
        empty_location(),
    ))
}

/// Builds a reactive subscription suitable for attaching to a text node in the
/// TIR side table.
fn reactive_subscription(
    source_name: &str,
    string_table: &mut StringTable,
) -> ReactiveSubscription {
    let source = ReactiveSource {
        path: InternedPath::from_single_str(source_name, string_table),
        kind: ReactiveSourceKind::Declaration,
    };
    ReactiveSubscription {
        source,
        type_id: builtin_type_ids::INT,
        location: empty_location(),
    }
}

// -------------------------
//  Identity and structural tests
// -------------------------

/// Proves that a simple foreign `SlotInsert` head is proxied as an
/// `InsertContribution` node whose template carries the same target key, and
/// that the proxy references the foreign store (not a deep copy).
#[test]
fn foreign_slot_insert_head_routes_through_local_proxy() {
    let mut string_table = StringTable::new();
    let slot_name = string_table.intern("content");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign SlotInsert template with a simple text body.
    let foreign_template_id;
    let foreign_node_count;
    {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let body_text = push_text_node(&mut foreign_store, &mut string_table, "inserted body");
        let foreign_root = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![body_text],
            },
            empty_location(),
        ));
        foreign_template_id = foreign_store.push_template(TemplateIr::new(
            foreign_root,
            Style::default(),
            TemplateType::SlotInsert(SlotKey::Named(slot_name)),
            crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary::default(),
            empty_location(),
        ));
        foreign_node_count = foreign_store.node_count();
    }

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    // Build a DynamicExpression node in the current store carrying the
    // foreign SlotInsert template expression.
    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(slot_name),
    );
    let child_expr = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    // Build the aggregate wrapper candidate from the head-prefix node.
    let wrapper_template_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build");

    // The candidate must contain an InsertContribution node, not a
    // ChildTemplate, because the head child is a SlotInsert helper.
    let proxy_template_id =
        first_insert_contribution_template_id(&current_store, wrapper_template_id);

    // The proxy template must carry the same SlotInsert target key as the
    // foreign template so TIR-native slot routing can bucket it correctly.
    let proxy_template = current_store
        .get_template(proxy_template_id)
        .expect("proxy template should exist");
    assert_eq!(
        proxy_template.kind,
        TemplateType::SlotInsert(SlotKey::Named(slot_name)),
        "proxy template must carry the foreign SlotInsert target key"
    );

    // The proxy's root is a Sequence mirroring the foreign SlotInsert body.
    // For a simple text body, the Sequence contains one ChildTemplate node
    // referencing a narrow derived foreign template that wraps the text node.
    let proxy_root = current_store
        .get_node(proxy_template.root)
        .expect("proxy root should exist");
    let TemplateIrNodeKind::Sequence { children } = &proxy_root.kind else {
        panic!("proxy root should be a Sequence");
    };
    let child_node = current_store
        .get_node(children[0])
        .expect("proxy child node should exist");
    let TemplateIrNodeKind::ChildTemplate { reference, .. } = &child_node.kind else {
        panic!(
            "proxy child should be a ChildTemplate, got {:?}",
            child_node.kind
        );
    };

    assert_eq!(
        reference.root.store_id, foreign_store_id,
        "proxy ChildTemplate must point to the foreign store"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Composed,
        "proxy ChildTemplate must preserve the foreign phase exactly"
    );
    assert_eq!(
        reference.overlay_set_id, overlay_set_id,
        "proxy ChildTemplate must preserve the foreign overlay-set ID exactly"
    );

    // The foreign store receives one narrow derived template (one new Sequence
    // node + one new template entry) wrapping the text body. No existing
    // nodes or templates are modified.
    let foreign_store_handle = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle");
    let foreign_store = foreign_store_handle.borrow();
    assert_eq!(
        foreign_store.node_count(),
        foreign_node_count + 1,
        "foreign store should gain one derived Sequence node, not a deep copy"
    );
}

// -------------------------
//  Routing tests
// -------------------------

/// Proves that a simple foreign `SlotInsert` body reaches its named slot
/// through the TIR-native routing path after the proxy is created by the
/// aggregate-wrapper head conversion.
#[test]
fn foreign_slot_insert_body_routes_to_named_slot() {
    let mut string_table = StringTable::new();
    let slot_name = string_table.intern("content");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign SlotInsert template with a simple text body.
    let foreign_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let body_text = push_text_node(&mut foreign_store, &mut string_table, "inserted body");
        build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(slot_name),
            vec![body_text],
        )
    };

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(slot_name),
    );
    let child_expr = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    let wrapper_candidate_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build");

    let insert_contribution_node_id =
        first_insert_contribution_node_id(&current_store, wrapper_candidate_id);

    let wrapper_id = build_wrapper_with_named_slots(
        &mut current_store,
        &[SlotKey::Named(slot_name), SlotKey::Default],
    );

    let fill_id =
        build_fill_template_from_nodes(&mut current_store, vec![insert_contribution_node_id]);

    let routed = route_tir_slot_contributions(&current_store, wrapper_id, fill_id, &string_table)
        .expect("routing should succeed");

    let content_nodes = routed
        .contributions
        .nodes_for_slot(&SlotKey::Named(slot_name));
    assert_eq!(
        content_nodes.len(),
        1,
        "exactly one node should reach the named slot"
    );

    let routed_node = current_store
        .get_node(content_nodes[0])
        .expect("routed node should exist");
    assert!(
        matches!(&routed_node.kind, TemplateIrNodeKind::ChildTemplate { .. }),
        "routed node should be a ChildTemplate reference to the derived foreign template"
    );

    assert!(
        routed
            .contributions
            .nodes_for_slot(&SlotKey::Default)
            .is_empty(),
        "default slot should be empty"
    );
}

/// Proves that same-store nested `InsertContribution` nodes inside a
/// `SlotInsert` body route recursively to their own target slots.
#[test]
fn same_store_nested_insert_routes_recursively() {
    let mut string_table = StringTable::new();
    let header_name = string_table.intern("header");
    let title_name = string_table.intern("title");

    let mut store = TemplateIrStore::new();

    let wrapper_id = build_wrapper_with_named_slots(
        &mut store,
        &[SlotKey::Named(header_name), SlotKey::Named(title_name)],
    );

    let inner_text = push_text_node(&mut store, &mut string_table, "title text");
    let inner_insert_id = build_slot_insert_template_with_body(
        &mut store,
        SlotKey::Named(title_name),
        vec![inner_text],
    );

    let outer_text = push_text_node(&mut store, &mut string_table, "header text");
    let nested_insert_node = push_insert_contribution_node(&mut store, inner_insert_id);
    let outer_insert_id = build_slot_insert_template_with_body(
        &mut store,
        SlotKey::Named(header_name),
        vec![outer_text, nested_insert_node],
    );

    let outer_insert_node = push_insert_contribution_node(&mut store, outer_insert_id);
    let fill_id = build_fill_template_from_nodes(&mut store, vec![outer_insert_node]);

    let routed = route_tir_slot_contributions(&store, wrapper_id, fill_id, &string_table)
        .expect("routing should succeed");

    let header_nodes = routed
        .contributions
        .nodes_for_slot(&SlotKey::Named(header_name));
    assert_eq!(
        header_nodes.len(),
        1,
        "header slot should receive the outer insert's text body"
    );

    let title_nodes = routed
        .contributions
        .nodes_for_slot(&SlotKey::Named(title_name));
    assert_eq!(
        title_nodes.len(),
        1,
        "title slot should receive the inner insert's text body — recursive routing contract"
    );
}

/// Proves that nested foreign `$insert` helpers inside a foreign `SlotInsert`
/// body are discovered recursively and route to their own target slots, exactly
/// like the same-store contract.
#[test]
fn foreign_slot_insert_nested_recursive_routing() {
    let mut string_table = StringTable::new();
    let header_name = string_table.intern("header");
    let title_name = string_table.intern("title");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign SlotInsert("header") template whose body contains both
    // text content and a nested InsertContribution targeting "title".
    let foreign_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");

        let inner_text = push_text_node(&mut foreign_store, &mut string_table, "title text");
        let inner_insert_id = build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(title_name),
            vec![inner_text],
        );

        let outer_text = push_text_node(&mut foreign_store, &mut string_table, "header text");
        let nested_insert_node = push_insert_contribution_node(&mut foreign_store, inner_insert_id);
        build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(header_name),
            vec![outer_text, nested_insert_node],
        )
    };

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(header_name),
    );
    let child_expr = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    let wrapper_candidate_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build");

    let insert_contribution_node_id =
        first_insert_contribution_node_id(&current_store, wrapper_candidate_id);

    let wrapper_id = build_wrapper_with_named_slots(
        &mut current_store,
        &[SlotKey::Named(header_name), SlotKey::Named(title_name)],
    );

    let fill_id =
        build_fill_template_from_nodes(&mut current_store, vec![insert_contribution_node_id]);

    let routed = route_tir_slot_contributions(&current_store, wrapper_id, fill_id, &string_table)
        .expect("routing should succeed");

    let header_nodes = routed
        .contributions
        .nodes_for_slot(&SlotKey::Named(header_name));
    assert_eq!(
        header_nodes.len(),
        1,
        "header slot should receive the text body as a ChildTemplate"
    );
    let header_node = current_store
        .get_node(header_nodes[0])
        .expect("header node should exist");
    assert!(
        matches!(&header_node.kind, TemplateIrNodeKind::ChildTemplate { .. }),
        "header slot should contain a ChildTemplate reference to the derived foreign template"
    );

    let title_nodes = routed
        .contributions
        .nodes_for_slot(&SlotKey::Named(title_name));
    assert_eq!(
        title_nodes.len(),
        1,
        "title slot should receive the nested insert's text body — recursive routing contract"
    );

    let title_node = current_store
        .get_node(title_nodes[0])
        .expect("title node should exist");
    assert!(
        matches!(&title_node.kind, TemplateIrNodeKind::ChildTemplate { .. }),
        "title slot should contain a ChildTemplate reference to the derived foreign template for the title insert body"
    );
}

/// Proves that cycle preflight treats its set as an active recursion path,
/// allowing a valid foreign insert DAG to reuse one nested helper twice.
#[test]
fn foreign_slot_insert_allows_repeated_nested_helper() {
    let mut string_table = StringTable::new();
    let header_name = string_table.intern("header");
    let title_name = string_table.intern("title");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let foreign_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");

        let inner_text = push_text_node(&mut foreign_store, &mut string_table, "title text");
        let inner_insert_id = build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(title_name),
            vec![inner_text],
        );

        let first_nested = push_insert_contribution_node(&mut foreign_store, inner_insert_id);
        let second_nested = push_insert_contribution_node(&mut foreign_store, inner_insert_id);
        build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(header_name),
            vec![first_nested, second_nested],
        )
    };

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();
    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(header_name),
    );
    let dynamic_node = push_dynamic_expression(
        &mut current_store,
        Expression::template(child_template, ValueMode::ImmutableOwned),
    );

    let wrapper_candidate_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("a repeated nested helper should not be mistaken for a cycle");
    let contribution_node_id =
        first_insert_contribution_node_id(&current_store, wrapper_candidate_id);
    let wrapper_id = build_wrapper_with_named_slots(
        &mut current_store,
        &[SlotKey::Named(header_name), SlotKey::Named(title_name)],
    );
    let fill_id = build_fill_template_from_nodes(&mut current_store, vec![contribution_node_id]);

    let routed = route_tir_slot_contributions(&current_store, wrapper_id, fill_id, &string_table)
        .expect("routing should replay the shared nested helper");
    assert_eq!(
        routed
            .contributions
            .nodes_for_slot(&SlotKey::Named(title_name))
            .len(),
        2,
        "both references to the shared nested helper should reach the target slot"
    );
}

// -------------------------
//  Reactive text summary test
// -------------------------

/// Proves that `summarize_existing_nodes` sets `has_reactivity` when a text
/// node carries a reactive subscription in the TIR side table, and that the
/// derived foreign content template built by the proxy path reflects that
/// reactivity in its summary.
#[test]
fn derived_foreign_content_template_summary_includes_reactive_text() {
    let mut string_table = StringTable::new();
    let slot_name = string_table.intern("content");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign SlotInsert template whose body is a text node with a
    // reactive subscription stored in the side table.
    let foreign_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let body_text = push_text_node(&mut foreign_store, &mut string_table, "reactive body");
        foreign_store.set_node_reactive_subscription(
            body_text,
            reactive_subscription("source", &mut string_table),
        );
        build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(slot_name),
            vec![body_text],
        )
    };

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(slot_name),
    );
    let child_expr = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    let wrapper_template_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build");

    // Extract the proxy template and find its ChildTemplate child pointing to
    // the derived foreign template.
    let proxy_template_id =
        first_insert_contribution_template_id(&current_store, wrapper_template_id);
    let proxy_template = current_store
        .get_template(proxy_template_id)
        .expect("proxy template should exist");
    let proxy_root = current_store
        .get_node(proxy_template.root)
        .expect("proxy root should exist");
    let TemplateIrNodeKind::Sequence { children } = &proxy_root.kind else {
        panic!("proxy root should be a Sequence");
    };
    let child_node = current_store
        .get_node(children[0])
        .expect("proxy child should exist");
    let TemplateIrNodeKind::ChildTemplate { reference, .. } = &child_node.kind else {
        panic!("proxy child should be a ChildTemplate");
    };

    // The derived foreign template summary must reflect the text node's
    // reactive subscription stored in the side table, not just the text bytes.
    let foreign_store = registry
        .store(foreign_store_id)
        .expect("foreign store should exist");
    let derived_template = foreign_store
        .get_template(reference.root.template_id)
        .expect("derived template should exist");

    assert_eq!(
        derived_template.summary.text_node_count, 1,
        "derived template summary should count the wrapped reactive text node"
    );
    assert_eq!(
        derived_template.summary.text_byte_count,
        "reactive body".len(),
        "derived template summary should count the text bytes"
    );
    assert!(
        derived_template.summary.has_reactivity,
        "derived template summary must set has_reactivity for text nodes with a side-table reactive subscription"
    );
}

// -------------------------
//  Cycle rejection test
// -------------------------

/// Proves that a cyclic foreign insert graph is rejected before any mutation,
/// leaving both the current store and the foreign store unchanged.
#[test]
fn foreign_slot_insert_proxy_rejects_cyclic_insert_graph() {
    let mut string_table = StringTable::new();
    let header_name = string_table.intern("header");
    let title_name = string_table.intern("title");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a cyclic foreign insert graph: template A (SlotInsert "header")
    // has a nested InsertContribution pointing to template B (SlotInsert
    // "title"), and template B has a nested InsertContribution pointing back
    // to template A.
    let (foreign_template_a_id, _foreign_template_b_id) = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");

        // Build the two SlotInsert template shells with empty bodies first so
        // we can reference each other by ID before installing the cycle.
        let template_a_id = build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(header_name),
            Vec::new(),
        );
        let template_b_id = build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(title_name),
            Vec::new(),
        );

        // Build the cyclic body for template A: a text node + nested insert
        // pointing to template B.
        let outer_text = push_text_node(&mut foreign_store, &mut string_table, "header text");
        let nested_b_node = push_insert_contribution_node(&mut foreign_store, template_b_id);
        let cyclic_a_root = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![outer_text, nested_b_node],
            },
            empty_location(),
        ));
        foreign_store.templates[template_a_id.index()].root = cyclic_a_root;

        // Build the cyclic body for template B: a text node + nested insert
        // pointing back to template A.
        let inner_text = push_text_node(&mut foreign_store, &mut string_table, "title text");
        let nested_a_node = push_insert_contribution_node(&mut foreign_store, template_a_id);
        let cyclic_b_root = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![inner_text, nested_a_node],
            },
            empty_location(),
        ));
        foreign_store.templates[template_b_id.index()].root = cyclic_b_root;

        (template_a_id, template_b_id)
    };

    // Record the foreign store's node and template count before proxy
    // construction so we can prove no partial mutation occurred on rejection.
    let foreign_node_count_before = {
        let foreign_store = registry
            .store(foreign_store_id)
            .expect("foreign store should exist");
        foreign_store.node_count()
    };
    let foreign_template_count_before = {
        let foreign_store = registry
            .store(foreign_store_id)
            .expect("foreign store should exist");
        foreign_store.template_count()
    };

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_a_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(header_name),
    );
    let child_expr = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    // Record the current store's node count after pushing the head node but
    // before the failed proxy construction, so we can prove no proxy nodes
    // were allocated before the preflight rejected the graph.
    let current_node_count_before = current_store.node_count();

    // The proxy construction must fail because the foreign insert graph is
    // cyclic (A -> B -> A).
    let result = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    );

    assert!(
        result.is_err(),
        "proxy construction must reject a cyclic insert graph"
    );

    // The current store should not have gained any proxy nodes from the
    // failed construction (only the pre-existing dynamic_node was added).
    assert_eq!(
        current_store.node_count(),
        current_node_count_before,
        "current store must be unchanged after cycle rejection — no partial proxy nodes"
    );

    // The foreign store should not have gained any derived templates or nodes.
    let foreign_store = registry
        .store(foreign_store_id)
        .expect("foreign store should exist");
    assert_eq!(
        foreign_store.node_count(),
        foreign_node_count_before,
        "foreign store must be unchanged after cycle rejection — no partial derived nodes"
    );
    assert_eq!(
        foreign_store.template_count(),
        foreign_template_count_before,
        "foreign store must be unchanged after cycle rejection — no partial derived templates"
    );
}

// -------------------------
//  Lifecycle enforcement test
// -------------------------

/// Proves that foreign-store mutation goes through `TemplateIrRegistry::store_mut`
/// and returns a precise internal error when the foreign store is frozen.
#[test]
fn foreign_slot_insert_proxy_rejects_frozen_foreign_store() {
    let mut string_table = StringTable::new();
    let slot_name = string_table.intern("content");

    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign SlotInsert template with a text body while the store is
    // still Building.
    let foreign_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let body_text = push_text_node(&mut foreign_store, &mut string_table, "inserted body");
        build_slot_insert_template_with_body(
            &mut foreign_store,
            SlotKey::Named(slot_name),
            vec![body_text],
        )
    };

    // Freeze the foreign store so derived-template creation must fail.
    registry
        .freeze_store(foreign_store_id)
        .expect("foreign store should freeze");

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");

    let child_template = foreign_slot_insert_template(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
        SlotKey::Named(slot_name),
    );
    let child_expr = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    // The proxy construction must fail because the foreign store is frozen and
    // create_derived_foreign_content_template routes through store_mut.
    let result = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    );

    assert!(
        result.is_err(),
        "proxy construction must fail when the foreign store is frozen"
    );
}
