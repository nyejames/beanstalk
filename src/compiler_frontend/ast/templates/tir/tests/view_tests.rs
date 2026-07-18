//! Focused tests for the `TirView` read API.
//!
//! WHAT: exercises phase ordering, constructor validation (root existence,
//! view context existence, minimum-phase checks), root template/node lookup,
//! effective node lookup, child view construction, overlay-dimension entry
//! accessors, and invariant errors for invalid refs.
//!
//! WHY: `TirView` is the central read API for all future template consumers.
//! These tests guard the invariants later phases depend on: invalid store
//! IDs produce `CompilerError` instead of panics, minimum-phase checks reject
//! unready roots, and overlay dimension accessors resolve entries through the
//! value-carried view context.

use super::super::builder::TemplateIrBuilder;
use super::super::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
};
use super::super::node::TemplateIrNodeKind;
use super::super::overlays::{
    TemplateViewContext, TirExpressionOverlay, TirExpressionOverlayId, TirSlotResolution,
    TirSlotResolutionOverlay, TirWrapperApplicationMode, TirWrapperContext,
    TirWrapperContextOverlay,
};
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ExpressionValueShape,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Test helpers
// -------------------------

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn bool_expression() -> Expression {
    Expression {
        kind: ExpressionKind::Bool(true),
        type_id: builtin_type_ids::BOOL,
        diagnostic_type: DataType::Bool,
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

/// Builds a template whose root is a single `DynamicExpression` node.
///
/// WHAT: returns the template ID and the root node ID so tests can construct a
///       `TemplateIrNodeId` and query the view for effective expressions.
fn build_template_with_dynamic_expression(
    store: &mut super::super::store::TemplateIrStore,
) -> (TemplateIrId, TemplateIrNodeId) {
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_dynamic_expression_node(
        bool_expression(),
        TemplateSegmentOrigin::Body,
        None,
        empty_location(),
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    (template_id, root)
}

/// Builds a single empty template inside `store` and returns its `TemplateIrId`.
fn build_empty_template(store: &mut super::super::store::TemplateIrStore) -> TemplateIrId {
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

/// Builds a template whose root node is a sequence with one text child.
///
/// WHAT: the child node is a `Text` node so tests can verify `effective_node`
///       resolves a non-root node through the view.
fn build_template_with_text_child(
    store: &mut super::super::store::TemplateIrStore,
    text_string_id: crate::compiler_frontend::symbols::string_interning::StringId,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let text_node = builder.push_text_node(
        text_string_id,
        5,
        crate::compiler_frontend::ast::templates::template::TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![text_node], empty_location());
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

/// Creates one module-local store containing an empty template and an empty view context.
struct TestStore {
    store: TemplateIrStore,
    root_ref: TemplateIrId,
    context: TemplateViewContext,
}

fn build_test_store() -> TestStore {
    let mut store = TemplateIrStore::new();
    let template_id = build_empty_template(&mut store);
    let context = TemplateViewContext::default();

    TestStore {
        store,
        root_ref: template_id,
        context,
    }
}

// -------------------------
//  Phase ordering tests
// -------------------------

#[test]
fn phase_ordering_is_monotonic() {
    assert!(TemplateTirPhase::Parsed < TemplateTirPhase::Composed);
    assert!(TemplateTirPhase::Composed < TemplateTirPhase::Formatted);
    assert!(TemplateTirPhase::Formatted < TemplateTirPhase::Finalized);
}

#[test]
fn phase_is_at_least_succeeds_for_equal_or_higher() {
    assert!(TemplateTirPhase::Parsed.is_at_least(TemplateTirPhase::Parsed));
    assert!(TemplateTirPhase::Composed.is_at_least(TemplateTirPhase::Parsed));
    assert!(TemplateTirPhase::Formatted.is_at_least(TemplateTirPhase::Composed));
    assert!(TemplateTirPhase::Finalized.is_at_least(TemplateTirPhase::Formatted));
}

#[test]
fn phase_is_at_least_fails_for_lower() {
    assert!(!TemplateTirPhase::Parsed.is_at_least(TemplateTirPhase::Composed));
    assert!(!TemplateTirPhase::Composed.is_at_least(TemplateTirPhase::Formatted));
    assert!(!TemplateTirPhase::Formatted.is_at_least(TemplateTirPhase::Finalized));
}

#[test]
fn phase_display_matches_variant_names() {
    assert_eq!(TemplateTirPhase::Parsed.to_string(), "Parsed");
    assert_eq!(TemplateTirPhase::Composed.to_string(), "Composed");
    assert_eq!(TemplateTirPhase::Formatted.to_string(), "Formatted");
    assert_eq!(TemplateTirPhase::Finalized.to_string(), "Finalized");
}

// -------------------------
//  Constructor validation tests
// -------------------------

#[test]
fn new_succeeds_for_valid_root_and_view_context() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context);

    assert!(view.is_ok());
}

#[test]
fn new_fails_for_missing_root_template() {
    let TestStore { store, context, .. } = build_test_store();

    let missing_root = TemplateIrId::new(99);
    let error = TirView::new(&store, missing_root, TemplateTirPhase::Parsed, context)
        .expect_err("missing root should be rejected");

    assert!(error.msg.contains("does not exist"));
}

// -------------------------
//  Occurrence-keyed overlay lookup tests
// -------------------------

/// Extracts the `ExpressionSiteId` from a `DynamicExpression` root node.
fn dynamic_expression_site_id(
    store: &super::super::store::TemplateIrStore,
    node_id: TemplateIrNodeId,
) -> ExpressionSiteId {
    let node = store
        .get_node(node_id)
        .expect("dynamic expression node should exist");
    match &node.kind {
        TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
        _ => panic!("expected DynamicExpression node"),
    }
}

#[test]
fn effective_expression_for_site_returns_override_when_present() {
    let mut store = TemplateIrStore::new();

    let (template_id, root_node) = { build_template_with_dynamic_expression(&mut store) };

    let site_id = { dynamic_expression_site_id(&store, root_node) };

    let expression_overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(bool_expression()))],
    });
    let context = TemplateViewContext {
        expression_overlay: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.effective_expression_for_site(site_id)
            .expect("expression lookup should succeed")
            .is_some(),
        "override should be present for the site"
    );
}

#[test]
fn effective_expression_for_site_returns_none_without_overlay() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.effective_expression_for_site(ExpressionSiteId::new(0))
            .expect("expression lookup should succeed")
            .is_none(),
        "no override should exist without an expression overlay"
    );
}

#[test]
fn effective_expression_for_site_returns_none_for_uncovered_site() {
    let mut store = TemplateIrStore::new();

    let (template_id, root_node) = { build_template_with_dynamic_expression(&mut store) };

    let site_id = { dynamic_expression_site_id(&store, root_node) };

    // The overlay covers a different site than the one we query.
    let other_site = store.next_expression_site_id();
    assert_ne!(other_site, site_id);
    let expression_overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(other_site, Box::new(bool_expression()))],
    });
    let context = TemplateViewContext {
        expression_overlay: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.effective_expression_for_site(site_id)
            .expect("expression lookup should succeed")
            .is_none(),
        "no override should exist for an uncovered site"
    );
}

#[test]
fn effective_expression_for_node_returns_override_for_dynamic_expression() {
    let mut store = TemplateIrStore::new();

    let (template_id, root_node) = { build_template_with_dynamic_expression(&mut store) };

    let site_id = { dynamic_expression_site_id(&store, root_node) };

    let expression_overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(bool_expression()))],
    });
    let context = TemplateViewContext {
        expression_overlay: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let node_ref = root_node;
    assert!(
        view.effective_expression_for_node(node_ref)
            .expect("expression lookup should succeed")
            .is_some(),
        "override should be present for the dynamic expression node"
    );
}

#[test]
fn effective_expression_for_node_returns_none_for_non_expression_node() {
    let mut store = TemplateIrStore::new();

    let (template_id, root_node) = {
        let mut string_table =
            crate::compiler_frontend::symbols::string_interning::StringTable::new();
        build_template_with_text_child(&mut store, string_table.intern("text"));
        let template = store
            .get_template(TemplateIrId::new(0))
            .expect("template should exist")
            .clone();
        (TemplateIrId::new(0), template.root)
    };

    let unused_site = store.next_expression_site_id();
    let expression_overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(unused_site, Box::new(bool_expression()))],
    });
    let context = TemplateViewContext {
        expression_overlay: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    // The root node is a Sequence, not a DynamicExpression, so no override
    // should be returned even though the overlay has an entry for site 0.
    let node_ref = root_node;
    assert!(
        view.effective_expression_for_node(node_ref)
            .expect("expression lookup should succeed")
            .is_none(),
        "no override for a non-DynamicExpression node"
    );
}

#[test]
fn effective_slot_resolution_returns_resolution_when_present() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let source = template_id;
    let occurrence_id = SlotOccurrenceId::new(0);
    let slot_overlay_id = store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            occurrence_id,
            TirSlotResolution::resolved(SlotKey::Default, vec![source]),
        )],
    });
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let resolution = view
        .effective_slot_resolution(occurrence_id)
        .expect("slot resolution lookup should succeed")
        .expect("resolution should be present");
    assert_eq!(resolution.sources(), &[source]);
}

#[test]
fn effective_slot_resolution_returns_none_without_overlay() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.effective_slot_resolution(SlotOccurrenceId::new(0))
            .expect("slot resolution lookup should succeed")
            .is_none(),
        "no resolution without a slot-resolution overlay"
    );
}

#[test]
fn effective_wrapper_context_returns_context_when_present() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let occurrence_id = ChildTemplateOccurrenceId::new(0);
    let wrapper_context = TirWrapperContext {
        inherited_wrapper_set: None,
        skip_parent_child_wrappers: true,
        application_mode: TirWrapperApplicationMode::IfChildEmits,
    };
    let wrapper_overlay_id = store.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
        contexts: vec![(occurrence_id, wrapper_context.clone())],
    });
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let found = view
        .effective_wrapper_context(occurrence_id)
        .expect("wrapper context lookup should succeed")
        .expect("wrapper context should be present");
    assert_eq!(found, &wrapper_context);
}

#[test]
fn effective_wrapper_context_returns_none_without_overlay() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.effective_wrapper_context(ChildTemplateOccurrenceId::new(0))
            .expect("wrapper context lookup should succeed")
            .is_none(),
        "no context without a wrapper-context overlay"
    );
}

#[test]
fn effective_wrapper_context_returns_none_for_uncovered_occurrence() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let wrapper_overlay_id = store.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
        contexts: vec![(
            ChildTemplateOccurrenceId::new(1),
            TirWrapperContext::empty(),
        )],
    });
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.effective_wrapper_context(ChildTemplateOccurrenceId::new(99))
            .expect("wrapper context lookup should succeed")
            .is_none(),
        "no context for an occurrence not covered by the overlay"
    );
}

#[test]
fn new_fails_for_missing_view_context() {
    let TestStore {
        store, root_ref, ..
    } = build_test_store();

    let missing_context = TemplateViewContext {
        expression_overlay: Some(TirExpressionOverlayId::new(99)),
        ..TemplateViewContext::default()
    };
    let error = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, missing_context)
        .expect_err("missing view context should be rejected");

    assert!(error.msg.contains("expression overlay"));
    assert!(error.msg.contains("does not exist"));
}

#[test]
fn with_minimum_phase_succeeds_when_phase_satisfies_minimum() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::with_minimum_phase(
        &store,
        root_ref,
        TemplateTirPhase::Formatted,
        TemplateTirPhase::Composed,
        context,
    );

    assert!(view.is_ok());
    assert_eq!(view.unwrap().phase(), TemplateTirPhase::Formatted);
}

#[test]
fn with_minimum_phase_fails_when_phase_is_below_minimum() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let error = TirView::with_minimum_phase(
        &store,
        root_ref,
        TemplateTirPhase::Parsed,
        TemplateTirPhase::Composed,
        context,
    )
    .expect_err("phase below minimum should be rejected");

    assert!(error.msg.contains("does not satisfy minimum phase"));
}

// -------------------------
//  Read accessor tests
// -------------------------

#[test]
fn root_ref_returns_the_constructor_root() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_eq!(view.root_ref(), root_ref);
}

#[test]
fn phase_returns_the_constructor_phase() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Composed, context)
        .expect("view should construct");

    assert_eq!(view.phase(), TemplateTirPhase::Composed);
}

#[test]
fn context_returns_the_constructor_context() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_eq!(view.context(), context);
}

#[test]
fn root_template_resolves_the_root_template_entry() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let template = view.root_template().expect("root template should resolve");
    assert_eq!(template.kind, TemplateType::String);
}

#[test]
fn root_node_resolves_the_root_body_node() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let node = view.root_node().expect("root node should resolve");
    assert!(matches!(node.kind, TemplateIrNodeKind::Sequence { .. }));
}

#[test]
fn effective_node_resolves_a_non_root_node() {
    use crate::compiler_frontend::symbols::string_interning::StringTable;

    let mut store = TemplateIrStore::new();

    let mut string_table = StringTable::new();
    let text_id = string_table.intern("hello");

    let (template_id, child_node_id) = {
        let template_id = build_template_with_text_child(&mut store, text_id);

        // Recover the text child node ID from the root sequence.
        let root = store
            .get_template(template_id)
            .expect("template should exist")
            .root;
        let root_node = store.get_node(root).expect("root node should exist");
        let child_node_id = match &root_node.kind {
            TemplateIrNodeKind::Sequence { children } => children[0],
            other => panic!("root should be a sequence, got {other:?}"),
        };
        (template_id, child_node_id)
    };

    let context = TemplateViewContext::default();
    let root_ref = template_id;

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let node_ref = child_node_id;
    let node = view
        .effective_node(node_ref)
        .expect("effective node should resolve");

    assert!(matches!(node.kind, TemplateIrNodeKind::Text { .. }));
}

#[test]
fn effective_node_errors_for_invalid_node_ref() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let invalid_node_ref = TemplateIrNodeId::new(99);
    let error = view
        .effective_node(invalid_node_ref)
        .expect_err("invalid node ref should be rejected");

    assert!(error.msg.contains("does not exist"));
}

// -------------------------
//  Child view construction tests
// -------------------------

#[test]
fn child_view_constructs_a_valid_view_for_a_child_template() {
    let mut store = TemplateIrStore::new();

    let parent_id = { build_empty_template(&mut store) };
    let child_id = { build_empty_template(&mut store) };

    let context = TemplateViewContext::default();
    let parent_ref = parent_id;
    let child_ref = child_id;

    let parent_view = TirView::new(&store, parent_ref, TemplateTirPhase::Parsed, context)
        .expect("parent view should construct");

    let child_view = parent_view
        .child_view(child_ref, TemplateTirPhase::Parsed, context)
        .expect("child view should construct");

    assert_eq!(child_view.root_ref(), child_ref);
    assert_eq!(child_view.phase(), TemplateTirPhase::Parsed);
}

#[test]
fn child_view_rejects_a_missing_view_context() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    // child_view no longer validates template existence through the store
    // (that would borrow the store's RefCell, which panics when the caller holds
    // a mutable store borrow). It validates the view context only.
    let missing_context = TemplateViewContext {
        expression_overlay: Some(TirExpressionOverlayId::new(999)),
        ..TemplateViewContext::default()
    };
    let error = view
        .child_view(
            TemplateIrId::new(0),
            TemplateTirPhase::Parsed,
            missing_context,
        )
        .expect_err("missing view context should be rejected");

    assert!(error.msg.contains("does not exist"));
}

// -------------------------
//  Overlay-dimension entry accessor tests
// -------------------------

#[test]
fn overlay_dimension_accessors_return_none_for_empty_view_context() {
    let TestStore {
        store,
        root_ref,
        context,
        ..
    } = build_test_store();

    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.expression_overlay()
            .expect("expression overlay lookup should succeed")
            .is_none()
    );
    assert!(
        view.slot_resolution_overlay()
            .expect("slot overlay lookup should succeed")
            .is_none()
    );
    assert!(
        view.wrapper_context_overlay()
            .expect("wrapper overlay lookup should succeed")
            .is_none()
    );
}

#[test]
fn expression_overlay_accessor_returns_the_entry_when_set() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let expression_overlay_id = store.allocate_expression_overlay(TirExpressionOverlay::default());
    let context = TemplateViewContext {
        expression_overlay: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.expression_overlay()
            .expect("expression overlay lookup should succeed")
            .is_some()
    );
    assert!(
        view.slot_resolution_overlay()
            .expect("slot overlay lookup should succeed")
            .is_none()
    );
    assert!(
        view.wrapper_context_overlay()
            .expect("wrapper overlay lookup should succeed")
            .is_none()
    );
}

#[test]
fn slot_resolution_overlay_accessor_returns_the_entry_when_set() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let slot_overlay_id =
        store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.expression_overlay()
            .expect("expression overlay lookup should succeed")
            .is_none()
    );
    assert!(
        view.slot_resolution_overlay()
            .expect("slot overlay lookup should succeed")
            .is_some()
    );
    assert!(
        view.wrapper_context_overlay()
            .expect("wrapper overlay lookup should succeed")
            .is_none()
    );
}

#[test]
fn wrapper_context_overlay_accessor_returns_the_entry_when_set() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let wrapper_overlay_id =
        store.allocate_wrapper_context_overlay(TirWrapperContextOverlay::default());
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    };

    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert!(
        view.expression_overlay()
            .expect("expression overlay lookup should succeed")
            .is_none()
    );
    assert!(
        view.slot_resolution_overlay()
            .expect("slot overlay lookup should succeed")
            .is_none()
    );
    assert!(
        view.wrapper_context_overlay()
            .expect("wrapper overlay lookup should succeed")
            .is_some()
    );
}

#[test]
fn new_rejects_missing_expression_overlay_entry() {
    let mut store = TemplateIrStore::new();

    let template_id = { build_empty_template(&mut store) };

    let context = TemplateViewContext {
        expression_overlay: Some(TirExpressionOverlayId::new(99)),
        slot_resolution: None,
        wrapper_context: None,
    };

    let root_ref = template_id;
    let error = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect_err("missing expression overlay entry should be rejected");

    assert!(error.msg.contains("does not exist"));
}

// -------------------------
//  Source-location recovery tests
// -------------------------

/// Creates a `SourceLocation` with a specific line and column so tests can
/// distinguish locations by their position data.
///
/// WHAT: builds a `SourceLocation` using the default interned scope and the
///       given start/end line and column. Using non-default positions lets
///       assertions prove the correct location was returned rather than a
///       coincidental `Default`.
fn location_at(line: i32, column: i32) -> SourceLocation {
    use crate::compiler_frontend::compiler_messages::source_location::CharPosition;
    use crate::compiler_frontend::symbols::interned_path::InternedPath;

    SourceLocation::new(
        InternedPath::default(),
        CharPosition {
            line_number: line,
            char_column: column,
        },
        CharPosition {
            line_number: line,
            char_column: column,
        },
    )
}

/// Asserts that an optional location result matches the expected line and column.
fn assert_location(
    result: Result<
        Option<SourceLocation>,
        crate::compiler_frontend::compiler_errors::CompilerError,
    >,
    line: i32,
    column: i32,
) {
    let location = result
        .expect("location lookup should succeed")
        .expect("location should be found");
    assert_eq!(location.start_pos.line_number, line);
    assert_eq!(location.start_pos.char_column, column);
}

/// Builds a template whose root is a `Sequence` containing one `Slot` node.
///
/// WHAT: returns the template ID, the root node ID, and the slot occurrence ID
///       so tests can query the view for the slot's source location.
fn build_template_with_slot(
    store: &mut super::super::store::TemplateIrStore,
    slot_location: SourceLocation,
) -> (TemplateIrId, SlotOccurrenceId) {
    let mut builder = TemplateIrBuilder::new(store);
    let slot_node = builder.push_slot_node(
        crate::compiler_frontend::ast::templates::template::SlotKey::Default,
        slot_location,
    );
    let root = builder.push_sequence_node(vec![slot_node], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let occurrence_id = {
        let node = store.get_node(slot_node).expect("slot node should exist");
        match &node.kind {
            TemplateIrNodeKind::Slot { placeholder } => placeholder.occurrence_id,
            _ => panic!("expected Slot node"),
        }
    };
    (template_id, occurrence_id)
}

/// Builds a template whose root is a `Sequence` containing one `ChildTemplate`
/// node referencing a second empty template in the same store.
///
/// WHAT: returns the parent template ID, the child template ID, and the
///       child-template occurrence ID so tests can verify the occurrence location
///       is recovered and that traversal does not cross into the child root.
fn build_template_with_child_template(
    store: &mut super::super::store::TemplateIrStore,
    child_template_location: SourceLocation,
    child_occurrence_location: SourceLocation,
) -> (
    TemplateIrId,
    TemplateIrId,
    super::super::ids::ChildTemplateOccurrenceId,
) {
    let mut builder = TemplateIrBuilder::new(store);

    // Build the child template first so the parent can reference it.
    let child_root = builder.push_sequence_node(vec![], empty_location());
    let child_template_id = builder.finish_template(
        child_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        child_template_location,
    );

    let child_node = builder.push_child_template_node(child_template_id, child_occurrence_location);
    let root = builder.push_sequence_node(vec![child_node], empty_location());
    let parent_template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let occurrence_id = {
        let node = store.get_node(child_node).expect("child node should exist");
        match &node.kind {
            TemplateIrNodeKind::ChildTemplate { occurrence_id, .. } => *occurrence_id,
            _ => panic!("expected ChildTemplate node"),
        }
    };

    (parent_template_id, child_template_id, occurrence_id)
}

/// Builds a template whose root is a `Sequence` containing one
/// `DynamicExpression` node, using a caller-provided source location.
fn build_template_with_dynamic_expression_at(
    store: &mut super::super::store::TemplateIrStore,
    expression_location: SourceLocation,
) -> (TemplateIrId, ExpressionSiteId) {
    let mut builder = TemplateIrBuilder::new(store);
    let expr_node = builder.push_dynamic_expression_node(
        bool_expression_with_location(&expression_location),
        TemplateSegmentOrigin::Body,
        None,
        expression_location,
    );
    let root = builder.push_sequence_node(vec![expr_node], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let site_id = {
        let node = store
            .get_node(expr_node)
            .expect("expression node should exist");
        match &node.kind {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            _ => panic!("expected DynamicExpression node"),
        }
    };
    (template_id, site_id)
}

/// A bool expression that carries a specific source location.
fn bool_expression_with_location(location: &SourceLocation) -> Expression {
    Expression {
        kind: ExpressionKind::Bool(true),
        type_id: builtin_type_ids::BOOL,
        diagnostic_type: DataType::Bool,
        function_receiver: None,
        value_mode: ValueMode::ImmutableOwned,
        location: location.clone(),
        reactive_source: None,
        reactive_template: None,
        const_record_state: ConstRecordState::RuntimeValue,
        contains_regular_division: false,
        value_shape: ExpressionValueShape::Ordinary,
    }
}

/// Builds a template whose root is a `BranchChain` with one branch whose
/// selector is a `Bool` expression, plus a fallback body.
///
/// WHAT: returns the template ID and the branch selector's `ExpressionSiteId`
///       so tests can verify the selector site location is recovered from
///       `TemplateIrBranch::location`.
fn build_template_with_branch_chain(
    store: &mut super::super::store::TemplateIrStore,
    branch_location: SourceLocation,
) -> (TemplateIrId, ExpressionSiteId) {
    use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;

    let mut builder = TemplateIrBuilder::new(store);

    let branch_body = builder.push_sequence_node(vec![], empty_location());
    let fallback_body = builder.push_sequence_node(vec![], empty_location());

    let branch = super::super::node::TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression_with_location(&branch_location)),
        branch_body,
        branch_location,
    );

    let root = builder.push_branch_chain_node(vec![branch], Some(fallback_body), empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let site_id = {
        let node = store
            .get_node(root)
            .expect("branch chain node should exist");
        match &node.kind {
            TemplateIrNodeKind::BranchChain { branches, .. } => branches[0].selector_site_id,
            _ => panic!("expected BranchChain node"),
        }
    };

    (template_id, site_id)
}

/// Builds a template whose root is a `Loop` with a `Conditional` (while) header,
/// so tests can verify the loop-header expression-site location is recovered from
/// the `Loop` node location.
fn build_template_with_conditional_loop(
    store: &mut super::super::store::TemplateIrStore,
    loop_location: SourceLocation,
) -> (TemplateIrId, ExpressionSiteId) {
    use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopHeader;

    let mut builder = TemplateIrBuilder::new(store);
    let body = builder.push_sequence_node(vec![], empty_location());
    let root = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression_with_location(&loop_location)),
        },
        body,
        None,
        loop_location,
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let site_id = {
        let node = store.get_node(root).expect("loop node should exist");
        match &node.kind {
            TemplateIrNodeKind::Loop { header_sites, .. } => match header_sites {
                super::super::node::TemplateLoopHeaderExpressionSites::Conditional {
                    condition,
                } => *condition,
                _ => panic!("expected Conditional loop header sites"),
            },
            _ => panic!("expected Loop node"),
        }
    };

    (template_id, site_id)
}

#[test]
fn source_location_for_slot_occurrence_returns_node_location() {
    let mut store = TemplateIrStore::new();

    let (template_id, occurrence_id) = { build_template_with_slot(&mut store, location_at(7, 12)) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_location(
        view.source_location_for_slot_occurrence(occurrence_id),
        7,
        12,
    );
}

#[test]
fn source_location_for_slot_occurrence_returns_none_for_missing_id() {
    let mut store = TemplateIrStore::new();

    let (template_id, _) = { build_template_with_slot(&mut store, location_at(7, 12)) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let missing = super::super::ids::SlotOccurrenceId::new(99);
    assert!(
        view.source_location_for_slot_occurrence(missing)
            .expect("lookup should succeed")
            .is_none(),
        "missing slot occurrence should return Ok(None)"
    );
}

#[test]
fn source_location_for_child_template_occurrence_returns_node_location() {
    let mut store = TemplateIrStore::new();

    let (parent_template_id, _child_template_id, occurrence_id) =
        { build_template_with_child_template(&mut store, location_at(1, 1), location_at(9, 20)) };

    let context = TemplateViewContext::default();
    let root_ref = parent_template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_location(
        view.source_location_for_child_template_occurrence(occurrence_id),
        9,
        20,
    );
}

#[test]
fn source_location_for_child_template_occurrence_returns_none_for_missing_id() {
    let mut store = TemplateIrStore::new();

    let (parent_template_id, _, _) =
        { build_template_with_child_template(&mut store, location_at(1, 1), location_at(9, 20)) };

    let context = TemplateViewContext::default();
    let root_ref = parent_template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let missing = super::super::ids::ChildTemplateOccurrenceId::new(99);
    assert!(
        view.source_location_for_child_template_occurrence(missing)
            .expect("lookup should succeed")
            .is_none(),
        "missing child-template occurrence should return Ok(None)"
    );
}

#[test]
fn source_location_for_expression_site_returns_dynamic_expression_location() {
    let mut store = TemplateIrStore::new();

    let (template_id, site_id) =
        { build_template_with_dynamic_expression_at(&mut store, location_at(11, 30)) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_location(view.source_location_for_expression_site(site_id), 11, 30);
}

#[test]
fn source_location_for_expression_site_returns_branch_selector_location() {
    let mut store = TemplateIrStore::new();

    let (template_id, site_id) =
        { build_template_with_branch_chain(&mut store, location_at(15, 8)) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_location(view.source_location_for_expression_site(site_id), 15, 8);
}

#[test]
fn source_location_for_expression_site_returns_loop_header_location() {
    let mut store = TemplateIrStore::new();

    let (template_id, site_id) =
        { build_template_with_conditional_loop(&mut store, location_at(21, 4)) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    assert_location(view.source_location_for_expression_site(site_id), 21, 4);
}

#[test]
fn source_location_for_expression_site_returns_none_for_missing_site() {
    let mut store = TemplateIrStore::new();

    let (template_id, _) =
        { build_template_with_dynamic_expression_at(&mut store, location_at(11, 30)) };

    let context = TemplateViewContext::default();
    let root_ref = template_id;
    let view = TirView::new(&store, root_ref, TemplateTirPhase::Parsed, context)
        .expect("view should construct");

    let missing = ExpressionSiteId::new(99);
    assert!(
        view.source_location_for_expression_site(missing)
            .expect("lookup should succeed")
            .is_none(),
        "missing expression site should return Ok(None)"
    );
}

#[test]
fn source_location_lookup_does_not_cross_into_child_template() {
    let mut store = TemplateIrStore::new();

    // Build a parent template that references a child template. The child has
    // its own slot with occurrence ID 0, while the parent has no slot of its own.
    let (parent_template_id, child_template_id, child_slot_occurrence_id) = {
        let (parent_template_id, child_template_id, child_slot_node) = {
            let mut builder = TemplateIrBuilder::new(&mut store);

            let child_slot_node = builder.push_slot_node(
                crate::compiler_frontend::ast::templates::template::SlotKey::Default,
                location_at(31, 6),
            );
            let child_root = builder.push_sequence_node(vec![child_slot_node], empty_location());
            let child_template_id = builder.finish_template(
                child_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                location_at(1, 1),
            );

            let parent_child_node =
                builder.push_child_template_node(child_template_id, location_at(9, 20));
            let parent_root = builder.push_sequence_node(vec![parent_child_node], empty_location());
            let parent_template_id = builder.finish_template(
                parent_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            );

            (parent_template_id, child_template_id, child_slot_node)
        };

        let child_slot_occurrence_id = {
            let node = store
                .get_node(child_slot_node)
                .expect("child slot node should exist");
            match &node.kind {
                TemplateIrNodeKind::Slot { placeholder } => placeholder.occurrence_id,
                _ => panic!("expected child Slot node"),
            }
        };

        (
            parent_template_id,
            child_template_id,
            child_slot_occurrence_id,
        )
    };

    // Look up the child's slot occurrence ID from the parent view: it must
    // not cross into the child root, so it should return Ok(None).
    let context = TemplateViewContext::default();
    let parent_ref = parent_template_id;
    let parent_view = TirView::new(&store, parent_ref, TemplateTirPhase::Parsed, context)
        .expect("parent view should construct");

    // The child-owned slot exists, but the parent view must not traverse into it.
    assert!(
        parent_view
            .source_location_for_slot_occurrence(child_slot_occurrence_id)
            .expect("lookup should succeed")
            .is_none(),
        "parent view must not cross into child template for slot occurrence lookup"
    );

    // A child view over the child template root should find the child's own
    // slot occurrence, proving the lookup works when the correct root is used.
    let child_view = TirView::new(&store, child_template_id, TemplateTirPhase::Parsed, context)
        .expect("child view should construct");

    assert_location(
        child_view.source_location_for_slot_occurrence(child_slot_occurrence_id),
        31,
        6,
    );
}
