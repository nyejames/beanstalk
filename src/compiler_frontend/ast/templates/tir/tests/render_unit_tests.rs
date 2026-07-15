//! TIR render-unit construction tests.
//!
//! WHAT: exercises wrapper-reference normalization, aggregate-wrapper
//! candidate construction and required render-unit node authority.
//!
//! WHY: these focused tests protect store-qualified wrapper and child identity
//! without reconstructing obsolete aggregate-wrapper source mirrors.

use crate::compiler_frontend::ast::expressions::expression::ExpressionValueShape;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::tir::ids::{TemplateIrId, TemplateIrNodeId};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::render_unit::build_aggregate_wrapper_candidate_from_tir_nodes;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::TemplateTirPhase;
use crate::compiler_frontend::ast::templates::tir::wrapper_sets::wrapper_reference_for_template;
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIrStoreOwner, TemplateTirReference, head_prefix_tir_nodes, sequence_children,
    trim_whitespace_before_loop_control_boundary,
};
use crate::compiler_frontend::compiler_messages::DiagnosticPayload;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use std::sync::Arc;

use crate::compiler_frontend::ast::templates::template::Template;

/// Constructs a `Template` directly from a real registry-qualified TIR reference.
fn template_with_reference(
    reference: TemplateTirReference,
    kind: TemplateType,
    location: SourceLocation,
) -> Template {
    Template {
        kind,
        tir_reference: reference,
        location,
    }
}

/// Builds a standalone `Template` with a valid registry-owned store and overlay
/// set. The caller must retain the returned `TemplateIrRegistry` for the
/// entire Template use lifetime so the store data stays alive.
fn standalone_test_template() -> (Template, TemplateIrRegistry) {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_handle = registry.store_handle(store_id).expect("allocated store");
    let template_id = {
        let mut store = store_handle.borrow_mut();
        let root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children: vec![] },
            empty_location(),
        ));
        push_template_entry(&mut store, root)
    };
    let store_owner = store_handle.borrow().owner();
    let template = Template {
        kind: TemplateType::StringFunction,
        tir_reference: TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner,
            phase: TemplateTirPhase::Parsed,
            overlay_set_id,
        },
        location: SourceLocation::default(),
    };
    (template, registry)
}
fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn int_expression(value: i32) -> Expression {
    Expression::int(value, empty_location(), ValueMode::ImmutableOwned)
}

/// Extracts the rendered message from a `TemplateError` for assertion.
///
/// WHAT: converts the error to its diagnostic payload and returns the
///      infrastructure error message string, which is what all aggregate-wrapper
///      walker failures carry.
fn error_message(error: TemplateError) -> String {
    let diagnostic = error.into_diagnostic();
    match diagnostic.payload {
        DiagnosticPayload::InfrastructureError { msg, .. } => msg,
        _ => format!("{:?}", diagnostic.kind),
    }
}

/// Pushes a simple text node into `store` and returns its node ID.
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

/// Pushes a `DynamicExpression` node carrying `expression` into `store`.
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

/// Pushes a single-template TIR entry whose root is `root` and returns its ID.
fn push_template_entry(store: &mut TemplateIrStore, root: TemplateIrNodeId) -> TemplateIrId {
    let summary = TemplateIrSummary::default();
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        summary,
        empty_location(),
    ))
}

/// Builds a template fixture with one store-qualified TIR reference.
fn wrapper_template_with_reference(
    root: TemplateRef,
    store_owner: Arc<TemplateIrStoreOwner>,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
) -> Template {
    Template {
        kind: TemplateType::StringFunction,
        tir_reference: TemplateTirReference {
            root,
            store_owner,
            phase,
            overlay_set_id,
        },
        location: SourceLocation::default(),
    }
}

/// Allocates a registry with one current store, one foreign store, and the
/// canonical empty overlay set pre-allocated at index 0.
///
/// WHAT: returns the store IDs and the empty overlay-set ID so each test can
///       build templates and references without repeating the registry
///       boilerplate.
/// WHY: keeps the focused wrapper-reference tests compact while mirroring the
///      production registry setup (stores created through the registry, empty
///      overlay set canonicalized at index 0).
fn wrapper_test_registry() -> (
    TemplateIrRegistry,
    TemplateStoreId,
    TemplateStoreId,
    TemplateOverlaySetId,
) {
    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let empty_overlay = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    (registry, current_store_id, foreign_store_id, empty_overlay)
}

fn push_wrapper_test_template(store: &mut TemplateIrStore) -> TemplateIrId {
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::AggregateOutput,
        empty_location(),
    ));
    push_template_entry(store, root)
}

#[test]
fn same_store_wrapper_reference_is_normalized_without_materialization() {
    let (registry, current_store_id, _, empty_overlay) = wrapper_test_registry();

    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");
    let template_id = {
        let mut store = current_handle.borrow_mut();
        push_wrapper_test_template(&mut store)
    };

    let store_owner = current_handle.borrow().owner();
    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(current_store_id, template_id),
        store_owner,
        TemplateTirPhase::Parsed,
        empty_overlay,
    );

    let store = current_handle.borrow();
    let reference = wrapper_reference_for_template(&wrapper, &store, &registry)
        .expect("same-store wrapper should normalize");

    assert_eq!(
        reference.root,
        TemplateRef::new(current_store_id, template_id)
    );
    assert_eq!(reference.phase, TemplateTirPhase::Parsed);
    assert_eq!(reference.overlay_set_id, empty_overlay);
}

#[test]
fn foreign_wrapper_reference_preserves_identity_without_copying() {
    let (registry, current_store_id, foreign_store_id, empty_overlay) = wrapper_test_registry();

    let foreign_handle = registry
        .store_handle(foreign_store_id)
        .expect("foreign store");
    let foreign_template_id = {
        let mut store = foreign_handle.borrow_mut();
        push_wrapper_test_template(&mut store)
    };

    let foreign_owner = foreign_handle.borrow().owner();
    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(foreign_store_id, foreign_template_id),
        foreign_owner,
        TemplateTirPhase::Composed,
        empty_overlay,
    );

    // The current store is borrowed immutably; the helper must validate the
    // foreign wrapper through the registry without re-borrowing the current
    // store's RefCell.
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");
    let current_store = current_handle.borrow();
    let reference = wrapper_reference_for_template(&wrapper, &current_store, &registry)
        .expect("foreign wrapper should normalize");

    assert_eq!(
        reference.root,
        TemplateRef::new(foreign_store_id, foreign_template_id),
        "foreign wrapper root must keep the foreign store/template identity"
    );
    assert_eq!(reference.phase, TemplateTirPhase::Composed);
    assert_eq!(reference.overlay_set_id, empty_overlay);
}

#[test]
fn wrapper_with_mismatched_store_owner_returns_error() {
    let (registry, current_store_id, _, _) = wrapper_test_registry();
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");

    let (wrapper, _mismatched_registry) = standalone_test_template();
    let store = current_handle.borrow();
    let refs = wrapper_reference_for_template(&wrapper, &store, &registry);
    assert!(
        refs.is_err(),
        "wrapper whose TIR reference belongs to a different store should be rejected"
    );
}

#[test]
fn wrapper_with_missing_overlay_set_returns_error() {
    let (registry, current_store_id, _, _) = wrapper_test_registry();
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");

    let store_owner = current_handle.borrow().owner();
    // An overlay set ID that was never allocated in the registry.
    let bogus_overlay = TemplateOverlaySetId::new(999);
    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(current_store_id, TemplateIrId::new(0)),
        store_owner,
        TemplateTirPhase::Parsed,
        bogus_overlay,
    );

    let store = current_handle.borrow();
    let refs = wrapper_reference_for_template(&wrapper, &store, &registry);
    assert!(refs.is_err(), "missing overlay set should be rejected");
}

#[test]
fn wrapper_with_missing_store_returns_error() {
    let (registry, current_store_id, _, _) = wrapper_test_registry();
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");

    let store_owner = current_handle.borrow().owner();
    // A store ID that the registry never allocated.
    let missing_store = TemplateStoreId::new(999);
    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(missing_store, TemplateIrId::new(0)),
        store_owner,
        TemplateTirPhase::Parsed,
        TemplateOverlaySetId::empty(),
    );

    let store = current_handle.borrow();
    let refs = wrapper_reference_for_template(&wrapper, &store, &registry);
    assert!(refs.is_err(), "missing store should be rejected");
}

#[test]
fn wrapper_with_missing_template_returns_error() {
    let (registry, current_store_id, _, empty_overlay) = wrapper_test_registry();
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");

    let store_owner = current_handle.borrow().owner();
    // The store exists but no template with this ID has been pushed.
    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(current_store_id, TemplateIrId::new(0)),
        store_owner,
        TemplateTirPhase::Parsed,
        empty_overlay,
    );

    let store = current_handle.borrow();
    let refs = wrapper_reference_for_template(&wrapper, &store, &registry);
    assert!(
        refs.is_err(),
        "missing template in the current store should be rejected"
    );
}

#[test]
fn foreign_wrapper_with_missing_template_returns_error() {
    let (registry, current_store_id, foreign_store_id, empty_overlay) = wrapper_test_registry();
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");
    let foreign_handle = registry
        .store_handle(foreign_store_id)
        .expect("foreign store");

    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(foreign_store_id, TemplateIrId::new(0)),
        foreign_handle.borrow().owner(),
        TemplateTirPhase::Parsed,
        empty_overlay,
    );

    let current_store = current_handle.borrow();
    let reference = wrapper_reference_for_template(&wrapper, &current_store, &registry);
    assert!(
        reference.is_err(),
        "missing template in a foreign store should be rejected"
    );
}

#[test]
fn foreign_wrapper_with_mismatched_owner_token_returns_error() {
    let (registry, current_store_id, foreign_store_id, empty_overlay) = wrapper_test_registry();

    let foreign_handle = registry
        .store_handle(foreign_store_id)
        .expect("foreign store");
    let foreign_template_id = {
        let mut store = foreign_handle.borrow_mut();
        push_wrapper_test_template(&mut store)
    };

    // A fresh owner token that does not match the foreign store's real owner.
    let bogus_owner = TemplateIrStoreOwner::new();
    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(foreign_store_id, foreign_template_id),
        bogus_owner,
        TemplateTirPhase::Composed,
        empty_overlay,
    );

    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");
    let current_store = current_handle.borrow();
    let refs = wrapper_reference_for_template(&wrapper, &current_store, &registry);
    assert!(refs.is_err(), "owner-token mismatch should be rejected");
}

#[test]
fn same_store_owner_mismatch_does_not_reborrow_current_store() {
    let (registry, current_store_id, _, empty_overlay) = wrapper_test_registry();
    let current_handle = registry
        .store_handle(current_store_id)
        .expect("current store");
    let template_id = {
        let mut store = current_handle.borrow_mut();
        push_wrapper_test_template(&mut store)
    };

    let wrapper = wrapper_template_with_reference(
        TemplateRef::new(current_store_id, template_id),
        TemplateIrStoreOwner::new(),
        TemplateTirPhase::Composed,
        empty_overlay,
    );

    // Production holds this mutable borrow while normalizing wrapper refs. An
    // invalid owner token must return `Err` without re-entering the RefCell.
    let current_store = current_handle.borrow_mut();
    let refs = wrapper_reference_for_template(&wrapper, &current_store, &registry);
    assert!(refs.is_err(), "owner-token mismatch should be rejected");
}

// ---------------------------------------------------------------------------
//  Cross-store head-node conversion tests
//
// These tests prove that `convert_head_node_for_aggregate_wrapper` (exercised
// through `build_aggregate_wrapper_candidate_from_tir_nodes`) preserves a
// registry-validated foreign child as a store-qualified
// `TemplateTirChildReference` rather than converting it to a bare local ID.
// ---------------------------------------------------------------------------

/// Builds a `Template` with a `tir_reference` pointing to a foreign store,
/// wrapped in an `ExpressionKind::Template` expression suitable for a
/// `DynamicExpression` node.
fn foreign_template_expression(
    foreign_store_id: TemplateStoreId,
    foreign_template_id: TemplateIrId,
    store_owner: Arc<TemplateIrStoreOwner>,
    phase: TemplateTirPhase,
    overlay_set_id: TemplateOverlaySetId,
) -> Expression {
    let child_template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(foreign_store_id, foreign_template_id),
            store_owner,
            phase,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        empty_location(),
    );

    Expression {
        kind: ExpressionKind::Template(Box::new(child_template)),
        type_id: builtin_type_ids::STRING,
        diagnostic_type: DataType::StringSlice,
        function_receiver: None,
        value_mode: ValueMode::ImmutableOwned,
        location: empty_location(),
        reactive_source: None,
        reactive_template: None,
        const_record_state: ConstRecordState::RuntimeValue,
        contains_regular_division: false,
        value_shape: ExpressionValueShape::Ordinary,
    }
}

#[test]
fn aggregate_wrapper_preserves_same_store_child_identity() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let mut store = registry
        .store_mut(store_id)
        .expect("current store should be mutable");
    let child_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence { children: vec![] },
        empty_location(),
    ));
    let child_template_id = push_template_entry(&mut store, child_root);
    let child_template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, child_template_id),
            store_owner: store.owner(),
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );
    let child_expression = Expression::template(child_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut store, child_expression);

    let wrapper_template_id =
        build_aggregate_wrapper_candidate_from_tir_nodes(&[dynamic_node], &mut store, &registry)
            .expect("same-store child should retain parser TIR authority");
    let reference = first_child_template_reference(&store, wrapper_template_id);

    assert_eq!(
        reference.root,
        TemplateRef::new(store_id, child_template_id)
    );
    assert_eq!(reference.phase, TemplateTirPhase::Finalized);
    assert_eq!(reference.overlay_set_id, overlay_set_id);
}

#[test]
fn aggregate_wrapper_rejects_child_with_mismatched_tir_authority() {
    let registry = TemplateIrRegistry::new();
    let mut store = TemplateIrStore::new();
    let (mismatched_template, _mismatched_registry) = standalone_test_template();
    let child_expression = Expression::template(mismatched_template, ValueMode::ImmutableOwned);
    let dynamic_node = push_dynamic_expression(&mut store, child_expression);

    let error =
        build_aggregate_wrapper_candidate_from_tir_nodes(&[dynamic_node], &mut store, &registry)
            .expect_err("child with mismatched-store TIR authority should be rejected");

    assert!(
        error_message(error).contains("did not carry a same-store parser-emitted reference"),
        "mismatched child authority should retain the invariant diagnostic"
    );
}

/// Extracts the first `ChildTemplate` reference from a template root, or
/// panics with a descriptive message.
fn first_child_template_reference(
    store: &TemplateIrStore,
    template_id: TemplateIrId,
) -> TemplateTirChildReference {
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
        if let TemplateIrNodeKind::ChildTemplate { reference, .. } = &child.kind {
            return *reference;
        }
    }

    panic!("no ChildTemplate node found in built aggregate wrapper");
}

#[test]
fn aggregate_wrapper_preserves_foreign_child_as_store_qualified_reference() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a valid foreign template in store B.
    let foreign_template_id;
    {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let text = push_text_node(&mut foreign_store, &mut string_table, "hello");
        let root = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![text],
            },
            empty_location(),
        ));
        foreign_template_id = push_template_entry(&mut foreign_store, root);
    }
    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    // Build a DynamicExpression node in the current store carrying the
    // foreign child template expression.
    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");
    let child_expr = foreign_template_expression(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    // Build the aggregate wrapper candidate from the head-prefix node.
    let wrapper_template_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build");

    let reference = first_child_template_reference(&current_store, wrapper_template_id);

    // The reference must point to the foreign store, not the current store.
    assert_eq!(
        reference.root.store_id, foreign_store_id,
        "foreign child should be preserved as a store-qualified reference to the foreign store"
    );
    assert_eq!(
        reference.root.template_id, foreign_template_id,
        "foreign child reference should name the original template ID"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Composed,
        "foreign child phase should be preserved exactly"
    );
    assert_eq!(
        reference.overlay_set_id, overlay_set_id,
        "foreign child overlay-set ID should be preserved exactly"
    );
}

#[test]
fn aggregate_wrapper_rejects_missing_foreign_template_instead_of_using_cached_kind() {
    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");
    let child_expression = foreign_template_expression(
        foreign_store_id,
        TemplateIrId::new(0),
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expression);

    build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect_err("a durable kind cache must not replace a missing foreign TIR template");
}

#[test]
fn aggregate_wrapper_preserves_foreign_child_expression_overlay_identity() {
    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a foreign template with a DynamicExpression.
    let foreign_template_id;
    let foreign_site_id;
    {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let site_id = foreign_store.next_expression_site_id();
        let dynamic = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(int_expression(1)),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            empty_location(),
        ));
        let root = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![dynamic],
            },
            empty_location(),
        ));
        foreign_template_id = push_template_entry(&mut foreign_store, root);
        foreign_site_id = site_id;
    }

    // Allocate a non-empty expression overlay on the foreign template.
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(foreign_site_id, Box::new(int_expression(99)))],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");
    let child_expr = foreign_template_expression(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    );
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    let wrapper_template_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build with overlay identity");

    let reference = first_child_template_reference(&current_store, wrapper_template_id);

    assert_eq!(
        reference.root.store_id, foreign_store_id,
        "foreign child with expression overlay should be preserved as a foreign reference"
    );
    assert_eq!(
        reference.overlay_set_id, overlay_set_id,
        "non-empty overlay-set identity should be preserved on the foreign child reference"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Finalized,
        "foreign child phase should be preserved exactly"
    );
}

#[test]
fn aggregate_wrapper_foreign_child_not_flattened_to_local_id() {
    // Control test: verify the foreign child's reference store_id differs
    // from the current store. Rebuilding it locally would give the reference
    // the current store's store_id and a freshly allocated template ID, losing
    // the foreign identity.
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let current_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let foreign_template_id;
    {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        let text = push_text_node(&mut foreign_store, &mut string_table, "child");
        let root = foreign_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![text],
            },
            empty_location(),
        ));
        foreign_template_id = push_template_entry(&mut foreign_store, root);
    }
    let foreign_owner = registry
        .store_handle(foreign_store_id)
        .expect("foreign store handle")
        .borrow()
        .owner();

    let mut current_store = registry
        .store_mut(current_store_id)
        .expect("current store should be mutable");
    let child_expr = foreign_template_expression(
        foreign_store_id,
        foreign_template_id,
        foreign_owner,
        TemplateTirPhase::Composed,
        overlay_set_id,
    );
    let dynamic_node = push_dynamic_expression(&mut current_store, child_expr);

    let wrapper_template_id = build_aggregate_wrapper_candidate_from_tir_nodes(
        &[dynamic_node],
        &mut current_store,
        &registry,
    )
    .expect("aggregate wrapper candidate should build");

    let reference = first_child_template_reference(&current_store, wrapper_template_id);

    assert_ne!(
        reference.root.store_id, current_store_id,
        "foreign child must not be flattened to a local (same-store) reference"
    );
}

// -----------------------
//  Render-unit authority
// -----------------------

#[test]
fn sequence_children_rejects_missing_node() {
    let store = TemplateIrStore::new();
    let result = sequence_children(&store, TemplateIrNodeId::new(99));
    assert!(
        result.is_err(),
        "missing sequence-children node should be rejected"
    );
}

#[test]
fn sequence_children_rejects_non_sequence_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let text_node = push_text_node(&mut store, &mut string_table, "leaf");

    let result = sequence_children(&store, text_node);
    assert!(result.is_err(), "non-sequence root should be rejected");
}

#[test]
fn head_prefix_tir_nodes_rejects_missing_root_child() {
    let store = TemplateIrStore::new();
    let result = head_prefix_tir_nodes(&store, &[TemplateIrNodeId::new(99)]);
    assert!(
        result.is_err(),
        "missing root child should be rejected during head-prefix extraction"
    );
}

#[test]
fn head_prefix_tir_nodes_accepts_empty_prefix() {
    let store = TemplateIrStore::new();
    let prefix = head_prefix_tir_nodes(&store, &[]).expect("empty prefix should be valid");
    assert!(
        prefix.is_empty(),
        "empty root-children list should yield an empty prefix"
    );
}

#[test]
fn trim_whitespace_rejects_missing_body_root() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let result = trim_whitespace_before_loop_control_boundary(
        TemplateIrNodeId::new(99),
        &mut store,
        &string_table,
    );
    assert!(result.is_err(), "missing loop body root should be rejected");
}

#[test]
fn trim_whitespace_rejects_non_sequence_body_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let text_node = push_text_node(&mut store, &mut string_table, "leaf");

    let result = trim_whitespace_before_loop_control_boundary(text_node, &mut store, &string_table);
    assert!(
        result.is_err(),
        "non-sequence loop body root should be rejected"
    );
}

#[test]
fn trim_whitespace_rejects_missing_child_in_sequence() {
    let string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let missing_child = TemplateIrNodeId::new(99);
    let body_root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![missing_child],
        },
        empty_location(),
    ));

    let result = trim_whitespace_before_loop_control_boundary(body_root, &mut store, &string_table);
    assert!(
        result.is_err(),
        "a sequence child missing from the store should be rejected"
    );
}
