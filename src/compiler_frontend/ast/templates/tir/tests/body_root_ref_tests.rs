//! Focused tests for the `TemplateTirBodyReference` view/root identity.
//!
//! WHAT: exercises construction with full view identity, same-store root
//! resolution, cross-store rejection, and round-tripping of phase, overlay set,
//! source location, and the store-qualified node reference.
//!
//! WHY: branch/fallback/loop and aggregate-wrapper body roots are now required
//! to carry store-qualified identity plus phase/overlay/location context. These
//! tests pin the invariants so consumers can rely on the shape without
//! re-deriving context.

use super::super::TemplateTirReference;
use super::super::body_root_ref::TemplateTirBodyReference;
use super::super::builder::TemplateIrBuilder;
use super::super::control_flow_roots::{
    ControlFlowBodyKind, finalized_control_flow_body_tir_reference,
};
use super::super::ids::{ExpressionSiteId, TemplateIrNodeId};
use super::super::overlays::{TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay};
use super::super::refs::{TemplateNodeRef, TemplateRef, TemplateStoreId};
use super::super::registry::TemplateIrRegistry;
use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::view::{TemplateTirPhase, TirSubtreeView};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ExpressionValueShape,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::TemplateSegmentOrigin;
use crate::compiler_frontend::ast::templates::template::{Style, TemplateType};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateControlFlow, TemplateLoopControlFlow, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

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

fn build_single_text_template(store: &mut TemplateIrStore) -> (usize, TemplateIrNodeId) {
    let mut string_table = crate::compiler_frontend::symbols::string_interning::StringTable::new();
    let text_id = string_table.intern("body");
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_text_node(
        text_id,
        4,
        crate::compiler_frontend::ast::templates::template::TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    (template_id.index(), root)
}

fn build_dynamic_expression_template(
    store: &mut TemplateIrStore,
) -> (usize, TemplateIrNodeId, ExpressionSiteId) {
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

    (template_id.index(), root, ExpressionSiteId::new(0))
}

#[test]
fn body_reference_round_trips_identity() {
    let mut store = TemplateIrStore::new();
    let (_template_index, root) = build_single_text_template(&mut store);

    let reference = TemplateTirBodyReference::new(
        store.owner(),
        TemplateStoreId::new(0),
        root,
        TemplateTirPhase::Formatted,
        TemplateOverlaySetId::empty(),
        empty_location(),
    );

    assert_eq!(
        reference.node_ref,
        TemplateNodeRef::new(TemplateStoreId::new(0), root)
    );
    assert_eq!(reference.phase, TemplateTirPhase::Formatted);
    assert_eq!(reference.overlay_set_id, TemplateOverlaySetId::empty());
    assert_eq!(reference.same_store_root(&store), Some(root));
}

#[test]
fn body_reference_rejects_different_store_owner() {
    let mut store_a = TemplateIrStore::new();
    let (_, root) = build_single_text_template(&mut store_a);

    let store_b = TemplateIrStore::new();
    let reference = TemplateTirBodyReference::new(
        store_a.owner(),
        TemplateStoreId::new(0),
        root,
        TemplateTirPhase::Composed,
        TemplateOverlaySetId::empty(),
        empty_location(),
    );

    assert!(
        reference.same_store_root(&store_b).is_none(),
        "a reference built from store A must not resolve against store B"
    );
}

#[test]
fn body_reference_rejects_same_owner_with_wrong_store_id() {
    let mut store = TemplateIrStore::new();
    let (_, root) = build_single_text_template(&mut store);
    let wrong_store_id = TemplateStoreId::new(store.store_id().index() + 1);

    let reference = TemplateTirBodyReference::new(
        store.owner(),
        wrong_store_id,
        root,
        TemplateTirPhase::Composed,
        TemplateOverlaySetId::empty(),
        empty_location(),
    );

    assert!(
        reference.same_store_root(&store).is_none(),
        "same-store lookup must reject stale or mismatched registry identity"
    );
}

#[test]
fn body_reference_store_local_identity_helper_uses_store_id() {
    let mut store = TemplateIrStore::new();
    let (_, root) = build_single_text_template(&mut store);

    let reference =
        TemplateTirBodyReference::with_store_local_identity(&store, root, TemplateTirPhase::Parsed);

    assert_eq!(reference.node_ref.store_id, store.store_id());
    assert_eq!(reference.node_ref.node_id, root);
    assert_eq!(reference.phase, TemplateTirPhase::Parsed);
    assert_eq!(reference.overlay_set_id, TemplateOverlaySetId::empty());
}

#[test]
fn body_reference_preserves_source_location() {
    let mut store = TemplateIrStore::new();
    let (_, root) = build_single_text_template(&mut store);
    let location = SourceLocation::default();

    let reference = TemplateTirBodyReference::new(
        store.owner(),
        TemplateStoreId::new(0),
        root,
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
        location.clone(),
    );

    assert_eq!(*reference.location(), location);
}

#[test]
fn body_reference_constructs_subtree_view() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (reference, root) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let (_, root) = build_single_text_template(&mut store);
        let reference = TemplateTirBodyReference::new(
            store.owner(),
            store_id,
            root,
            TemplateTirPhase::Composed,
            overlay_set_id,
            empty_location(),
        );
        (reference, root)
    };

    let view =
        TirSubtreeView::with_minimum_phase(&registry, &reference, TemplateTirPhase::Composed)
            .expect("body-root subtree view should construct");

    assert_eq!(view.root_node_ref(), TemplateNodeRef::new(store_id, root));
    assert_eq!(view.phase(), TemplateTirPhase::Composed);
    assert_eq!(view.overlay_set_id(), overlay_set_id);
    assert_eq!(*view.location(), empty_location());
    assert_eq!(
        view.root_node().expect("root node should resolve").location,
        empty_location()
    );
}

#[test]
fn aggregate_wrapper_reference_constructs_subtree_view() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let reference = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let (_, aggregate_wrapper_root) = build_single_text_template(&mut store);
        TemplateTirBodyReference::new(
            store.owner(),
            store_id,
            aggregate_wrapper_root,
            TemplateTirPhase::Composed,
            overlay_set_id,
            empty_location(),
        )
    };

    let view = TirSubtreeView::new(&registry, &reference)
        .expect("aggregate-wrapper body-root view should construct");

    assert_eq!(view.root_node_ref().store_id, store_id);
}

#[test]
fn body_reference_subtree_view_rejects_low_phase() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let reference = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let (_, root) = build_single_text_template(&mut store);
        TemplateTirBodyReference::new(
            store.owner(),
            store_id,
            root,
            TemplateTirPhase::Parsed,
            TemplateOverlaySetId::empty(),
            empty_location(),
        )
    };

    let error =
        TirSubtreeView::with_minimum_phase(&registry, &reference, TemplateTirPhase::Composed)
            .expect_err("parsed body root should not satisfy composed view consumers");

    assert!(error.msg.contains("does not satisfy minimum phase"));
}

#[test]
fn body_reference_subtree_view_uses_expression_overlay() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (root, site_id, store_owner) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let (_, root, site_id) = build_dynamic_expression_template(&mut store);
        (root, site_id, store.owner())
    };

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(bool_expression()))],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let reference = TemplateTirBodyReference::new(
        store_owner,
        store_id,
        root,
        TemplateTirPhase::Finalized,
        overlay_set_id,
        empty_location(),
    );

    let view =
        TirSubtreeView::with_minimum_phase(&registry, &reference, TemplateTirPhase::Finalized)
            .expect("finalized body-root view should construct");

    assert!(
        view.effective_expression_for_site(site_id)
            .expect("overlay lookup should succeed")
            .is_some(),
        "subtree view should observe expression overlays through the registry"
    );
}

#[test]
fn body_reference_exposes_view_identity() {
    let mut store = TemplateIrStore::new();
    let (_, root) = build_single_text_template(&mut store);

    let body_ref = TemplateTirBodyReference::with_store_local_identity(
        &store,
        root,
        TemplateTirPhase::Composed,
    );

    assert_eq!(body_ref.node_ref().node_id, root);
    assert_eq!(body_ref.phase, TemplateTirPhase::Composed);
    assert_eq!(body_ref.overlay_set_id, TemplateOverlaySetId::empty());
}

/// A finalized control-flow root may be nested under wrapper and composition
/// sequences. Body-root lookup must find the owner's node without traversing
/// unrelated child-template references.
#[test]
fn finalized_control_flow_body_reference_finds_nested_control_flow_node() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let body_text = string_table.intern("body");
    let mut builder = TemplateIrBuilder::new(&mut store);
    let body_root =
        builder.push_text_node(body_text, 4, TemplateSegmentOrigin::Body, empty_location());
    let loop_node = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression()),
        },
        body_root,
        None,
        empty_location(),
    );

    let unrelated_body =
        builder.push_text_node(body_text, 4, TemplateSegmentOrigin::Body, empty_location());
    let unrelated_loop = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression()),
        },
        unrelated_body,
        None,
        empty_location(),
    );
    let unrelated_template_id = builder.finish_template(
        unrelated_loop,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let unrelated_child = builder.push_child_template_node(unrelated_template_id, empty_location());

    let wrapper_root = builder.push_sequence_node(vec![loop_node], empty_location());
    let sibling_text =
        builder.push_text_node(body_text, 4, TemplateSegmentOrigin::Body, empty_location());
    let composed_root = builder.push_sequence_node(
        vec![unrelated_child, wrapper_root, sibling_text],
        empty_location(),
    );
    let wrapped_template_id = builder.finish_template(
        composed_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let mut template = Template::empty();
    template.location = empty_location();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), wrapped_template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });
    template.control_flow = Some(TemplateControlFlow::Loop(Box::new(
        TemplateLoopControlFlow {
            body_tir_reference: TemplateTirBodyReference::with_store_local_identity(
                &store,
                TemplateIrNodeId::new(0),
                TemplateTirPhase::Parsed,
            ),
            header: TemplateLoopHeader::Conditional {
                condition: Box::new(bool_expression()),
            },
            aggregate_wrapper_tir_reference: None,
            location: empty_location(),
        },
    )));

    let reference =
        finalized_control_flow_body_tir_reference(&template, &store, ControlFlowBodyKind::LoopBody)
            .expect("body root reference should be found for nested control-flow node");

    assert_eq!(reference.same_store_root(&store), Some(body_root));
}

/// A single-child forwarding root is a valid finalized owner shape for a
/// control-flow template wrapped by runtime slot preparation.
#[test]
fn finalized_control_flow_body_reference_follows_single_child_forwarding_root() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();

    let body_text = string_table.intern("body");
    let mut builder = TemplateIrBuilder::new(&mut store);
    let body_root =
        builder.push_text_node(body_text, 4, TemplateSegmentOrigin::Body, empty_location());
    let loop_node = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(bool_expression()),
        },
        body_root,
        None,
        empty_location(),
    );
    let loop_template_id = builder.finish_template(
        loop_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let forwarding_child = builder.push_child_template_node(loop_template_id, empty_location());
    let forwarding_root = builder.push_sequence_node(vec![forwarding_child], empty_location());
    let forwarding_template_id = builder.finish_template(
        forwarding_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );

    let mut template = Template::empty();
    template.location = empty_location();
    template.tir_reference = Some(TemplateTirReference {
        root: TemplateRef::new(store.store_id(), forwarding_template_id),
        store_owner: store.owner(),
        is_composed: true,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
    });
    template.control_flow = Some(TemplateControlFlow::Loop(Box::new(
        TemplateLoopControlFlow {
            body_tir_reference: TemplateTirBodyReference::with_store_local_identity(
                &store,
                TemplateIrNodeId::new(0),
                TemplateTirPhase::Parsed,
            ),
            header: TemplateLoopHeader::Conditional {
                condition: Box::new(bool_expression()),
            },
            aggregate_wrapper_tir_reference: None,
            location: empty_location(),
        },
    )));

    let reference =
        finalized_control_flow_body_tir_reference(&template, &store, ControlFlowBodyKind::LoopBody)
            .expect("body root reference should follow a single-child forwarding root");

    assert_eq!(reference.same_store_root(&store), Some(body_root));
}
