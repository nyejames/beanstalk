//! TIR classification tests.
//!
//! WHAT: proves TIR structural and effective-view classification behavior.
//!
//! WHY: structural walkers and effective views own production classification.
//! Tests target those owners directly instead of preserving compatibility
//! materialization entry points for detached fixtures.

use super::super::builder::TemplateIrBuilder;
use super::super::classification::{
    classify_effective_tir_view_template, same_store_tir_id,
    tir_node_is_const_evaluable_value_with_bindings, tir_view_subtree_is_const_evaluable_value,
};
use super::super::node::TemplateIrNodeKind;
use super::super::registry::TemplateIrRegistry;
use super::super::store::{TemplateIrStore, TemplateStoreState};
use super::super::summary::TemplateIrSummary;
use super::super::{TemplateRef, TemplateTirPhase, TemplateTirReference, TirView};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, ReactiveSource, ReactiveSourceKind,
};
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotKey, Style, TemplateConstValueKind, TemplateSegmentOrigin,
    TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopControlKind;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::ids::SlotOccurrenceId;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirSlotResolution,
    TirSlotResolutionOverlay, TirWrapperContextOverlay,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;

// -------------------------
//  Test helpers
// -------------------------

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn string_expression(string_table: &mut StringTable, text: &str) -> Expression {
    Expression::string_slice(
        string_table.intern(text),
        empty_location(),
        ValueMode::ImmutableOwned,
    )
}

fn child_template_expression(child: Template) -> Expression {
    Expression::template(child, ValueMode::ImmutableOwned)
}

fn string_function_call_expression(string_table: &mut StringTable, name: &str) -> Expression {
    let scope = InternedPath::from_single_str("main.bst", string_table);
    let name_id = string_table.intern(name);

    Expression::new(
        ExpressionKind::FunctionCall {
            name: scope.append(name_id),
            args: Vec::new(),
            result_type_ids: vec![builtin_type_ids::STRING],
        },
        empty_location(),
        builtin_type_ids::STRING,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    )
}

/// Builds a `Template` with a real same-store TIR reference from a trivial
/// TIR tree in `store`.
fn template_with_seeded_reference(store: &mut TemplateIrStore) -> Template {
    let mut builder = TemplateIrBuilder::new(store);
    let root = builder.push_sequence_node(vec![], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    );
    Template {
        kind: TemplateType::StringFunction,
        tir_reference: TemplateTirReference {
            root: TemplateRef::new(store.store_id(), template_id),
            store_owner: store.owner(),
            phase: TemplateTirPhase::Parsed,
            overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        },
        location: SourceLocation::default(),
    }
}

fn string_function_child_id_with_runtime_head(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> super::super::ids::TemplateIrId {
    let runtime_head = string_function_call_expression(string_table, "wrapper");
    let mut builder = TemplateIrBuilder::new(store);
    let dynamic_head = builder.push_dynamic_expression_node(
        runtime_head,
        TemplateSegmentOrigin::Head,
        None,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![dynamic_head], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::StringFunction,
        TemplateIrSummary {
            dynamic_expression_count: 1,
            max_depth: 1,
            is_const_evaluable_shape: false,
            ..TemplateIrSummary::empty()
        },
        empty_location(),
    )
}

fn classify_registry_view_template(
    string_table: &mut StringTable,
    template_kind: TemplateType,
    build_root: impl FnOnce(
        &mut TemplateIrBuilder<'_>,
        &mut StringTable,
    ) -> super::super::ids::TemplateIrNodeId,
) -> TemplateConstValueKind {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let root = build_root(&mut builder, string_table);

        builder.finish_template(
            root,
            Style::default(),
            template_kind.clone(),
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    assert!(matches!(
        registry.store_state(store_id),
        Some(TemplateStoreState::FrozenModuleLocal { .. })
    ));

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("test view should resolve");

    classify_effective_tir_view_template(&view, &store_snapshot, string_table)
        .expect("view classification should succeed")
        .const_value_kind
}

// -------------------------
//  Same-store ownership proof
// -------------------------

#[test]
fn same_store_tir_id_returns_id_for_matching_store() {
    let mut store = TemplateIrStore::new();
    let template = template_with_seeded_reference(&mut store);

    let id = same_store_tir_id(&template, &store);
    assert!(id.is_some(), "same-store reference should resolve");
}

#[test]
fn same_store_tir_id_returns_none_for_cross_store_reference() {
    let store_a = TemplateIrStore::new();
    let mut store_b = TemplateIrStore::new();
    let template = template_with_seeded_reference(&mut store_b);

    let id = same_store_tir_id(&template, &store_a);
    assert!(id.is_none(), "cross-store reference must not resolve");
}

#[test]
fn const_body_evaluation_treats_string_function_children_as_structural() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let child_template = string_function_child_id_with_runtime_head(&mut store, &mut string_table);

    let mut builder = TemplateIrBuilder::new(&mut store);
    let child_node = builder.push_child_template_node(child_template, empty_location());
    let root = builder.push_sequence_node(vec![child_node], empty_location());

    assert!(
        tir_node_is_const_evaluable_value_with_bindings(&mut store, root, &[], &string_table),
        "StringFunction children are wrapper values in const-required body validation"
    );
}

// -------------------------
//  Registry-backed TirView classification
// -------------------------

#[test]
fn view_const_evaluation_follows_foreign_embedded_template_overlay() {
    let mut string_table = StringTable::new();
    let binding_path = InternedPath::from_single_str("item", &mut string_table);
    let mut registry = TemplateIrRegistry::new();
    let outer_store_id = registry.allocate_store();
    let child_store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let child_store_handle = registry
        .store_handle(child_store_id)
        .expect("child store should be allocated");

    let (child_template_id, child_site_id, child_owner) = {
        let mut child_store = child_store_handle.borrow_mut();
        let child_owner = child_store.owner();
        let mut builder = TemplateIrBuilder::new(&mut child_store);
        let runtime_expression = string_function_call_expression(&mut string_table, "runtime");
        let dynamic_node = builder.push_dynamic_expression_node(
            runtime_expression,
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![dynamic_node], empty_location());
        let child_template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let child_site_id = match &child_store
            .get_node(dynamic_node)
            .expect("child dynamic node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            _ => unreachable!("test child should be a dynamic expression"),
        };

        (child_template_id, child_site_id, child_owner)
    };

    let binding_expression = Expression::new(
        ExpressionKind::Reference(binding_path.clone()),
        empty_location(),
        builtin_type_ids::STRING,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    );
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(child_site_id, Box::new(binding_expression))],
    });
    let child_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let child_template = Template {
        kind: TemplateType::String,
        tir_reference: TemplateTirReference {
            root: TemplateRef::new(child_store_id, child_template_id),
            store_owner: child_owner,
            phase: TemplateTirPhase::Finalized,
            overlay_set_id: child_overlay_set_id,
        },
        location: SourceLocation::default(),
    };

    let outer_store_handle = registry
        .store_handle(outer_store_id)
        .expect("outer store should be allocated");
    let (outer_template_id, outer_root) = {
        let mut outer_store = outer_store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut outer_store);
        let child_expression = child_template_expression(child_template);
        let dynamic_node = builder.push_dynamic_expression_node(
            child_expression,
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let outer_root = builder.push_sequence_node(vec![dynamic_node], empty_location());
        let outer_template_id = builder.finish_template(
            outer_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        (outer_template_id, outer_root)
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(outer_store_id, outer_template_id),
        TemplateTirPhase::Composed,
        empty_overlay_set_id,
    )
    .expect("outer view should resolve");
    let outer_store = view.store().expect("outer view store should resolve");

    assert!(
        tir_view_subtree_is_const_evaluable_value(
            &view,
            &outer_store,
            &string_table,
            outer_root,
            &[binding_path],
        )
        .expect("foreign embedded template classification should succeed"),
        "classification should read the foreign child overlay binding"
    );
}

#[test]
fn tir_view_classification_returns_renderable_string_for_text_root() {
    let mut string_table = StringTable::new();

    let const_kind = classify_registry_view_template(
        &mut string_table,
        TemplateType::String,
        |builder, table| {
            let text = table.intern("hello");
            let text_node = builder.push_text_node(
                text,
                "hello".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );

            builder.push_sequence_node(vec![text_node], empty_location())
        },
    );

    assert_eq!(const_kind, TemplateConstValueKind::RenderableString);
}

#[test]
fn tir_view_classification_rejects_reactive_text() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst/#reactive", &mut string_table);
    let subscription = ReactiveSubscription {
        source: ReactiveSource {
            path: source_path,
            kind: ReactiveSourceKind::Declaration,
        },
        type_id: builtin_type_ids::STRING,
        location: empty_location(),
    };

    let const_kind = classify_registry_view_template(
        &mut string_table,
        TemplateType::String,
        move |builder, table| {
            let text = table.intern("reactive text");
            let text_node = builder.push_text_node_with_subscription(
                text,
                "reactive text".len() as u32,
                TemplateSegmentOrigin::Body,
                Some(subscription),
                empty_location(),
            );

            builder.push_sequence_node(vec![text_node], empty_location())
        },
    );

    assert_eq!(const_kind, TemplateConstValueKind::NonConst);
}

#[test]
fn tir_view_classification_unresolved_slot_returns_renderable_string() {
    let mut string_table = StringTable::new();

    let const_kind =
        classify_registry_view_template(&mut string_table, TemplateType::String, |builder, _| {
            let slot = builder.push_slot_node(SlotKey::Default, empty_location());

            builder.push_sequence_node(vec![slot], empty_location())
        });

    assert_eq!(
        const_kind,
        TemplateConstValueKind::RenderableString,
        "an unresolved slot with no slot-resolution overlay folds to empty output"
    );
}

#[test]
fn tir_view_classification_preserves_slot_insert_helper_kind() {
    let mut string_table = StringTable::new();
    let slot_name = string_table.intern("title");

    let const_kind = classify_registry_view_template(
        &mut string_table,
        TemplateType::SlotInsert(SlotKey::Named(slot_name)),
        |builder, table| {
            let text = table.intern("helper");
            let text_node = builder.push_text_node(
                text,
                "helper".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );

            builder.push_sequence_node(vec![text_node], empty_location())
        },
    );

    assert_eq!(const_kind, TemplateConstValueKind::SlotInsertHelper);
}

#[test]
fn tir_view_classification_preserves_loop_control_signal() {
    let mut string_table = StringTable::new();

    let const_kind =
        classify_registry_view_template(&mut string_table, TemplateType::String, |builder, _| {
            builder.push_loop_control_node(TemplateLoopControlKind::Break, empty_location())
        });

    assert_eq!(const_kind, TemplateConstValueKind::LoopControlSignal);
}

#[test]
fn tir_view_classification_returns_non_const_for_runtime_expression() {
    let mut string_table = StringTable::new();

    let const_kind = classify_registry_view_template(
        &mut string_table,
        TemplateType::String,
        |builder, table| {
            let runtime_expression = string_function_call_expression(table, "runtime_text");
            let runtime_node = builder.push_dynamic_expression_node(
                runtime_expression,
                TemplateSegmentOrigin::Body,
                None,
                empty_location(),
            );

            builder.push_sequence_node(vec![runtime_node], empty_location())
        },
    );

    assert_eq!(const_kind, TemplateConstValueKind::NonConst);
}

#[test]
fn expression_overlay_requires_finalized_view_and_drives_classification() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let (template_id, site_id) = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let runtime_expression = string_function_call_expression(&mut string_table, "runtime_text");
        let dynamic_node = builder.push_dynamic_expression_node(
            runtime_expression,
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![dynamic_node], empty_location());
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let site_id = match &store
            .get_node(dynamic_node)
            .expect("dynamic node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            _ => unreachable!("test node should be a dynamic expression"),
        };

        (template_id, site_id)
    };

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            site_id,
            Box::new(string_expression(&mut string_table, "normalized")),
        )],
    });
    let expression_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let overlay_set_id = registry
        .compose_overlay_sets(&[empty_overlay_set_id, expression_overlay_set_id])
        .expect("expression overlay set should compose");

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    let store_snapshot = store_handle.borrow().clone();
    let composed_view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("composed test view should resolve");

    let error = match classify_effective_tir_view_template(
        &composed_view,
        &store_snapshot,
        &string_table,
    ) {
        Ok(_) => panic!("expression overlays must not classify before Finalized"),
        Err(error) => error,
    };
    let crate::compiler_frontend::ast::templates::error::TemplateError::Infrastructure(error) =
        error
    else {
        panic!("phase rejection should be an internal classification invariant");
    };
    assert!(
        error
            .msg
            .contains("expression-overlay classification requires Finalized"),
        "phase rejection should identify the expression-overlay boundary"
    );

    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("finalized effective view classification should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "classification must use the normalized expression overlay instead of the structural runtime expression"
    );
}

/// Finalized view classification with a slot-resolution overlay must succeed.
///
/// WHAT: a template whose overlay set carries a slot-resolution dimension should
///       not be rejected merely because that dimension is present. When the
///       structural tree has no unresolved slots (the overlay is empty here),
///       classification proceeds normally.
/// WHY: slot-resolution overlays are now a supported overlay dimension for
///      classification. Wrapper-context overlays are also supported.
#[test]
fn finalized_tir_view_classification_accepts_slot_resolution_overlay() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: Vec::new(),
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text = string_table.intern("overlay");
        let text_node = builder.push_text_node(
            text,
            "overlay".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![text_node], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("classification with a slot-resolution overlay should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "a text-only template with an empty slot-resolution overlay should classify as RenderableString"
    );
}

/// Composed view classification with a slot-resolution overlay must succeed.
///
/// WHAT: slot-resolution overlays are attached during composition, before
///       expression-overlay normalization requires `Finalized`.
/// WHY: Phase 4 finalization folds Composed slot-overlay views directly. The
///      effective classifier must accept that phase when no expression overlay
///      is present.
#[test]
fn composed_tir_view_classification_accepts_slot_resolution_overlay() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: Vec::new(),
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text = string_table.intern("overlay");
        let text_node = builder.push_text_node(
            text,
            "overlay".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![text_node], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("classification with a Composed slot-resolution overlay should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "a Composed text-only template with a slot-resolution overlay should classify as RenderableString"
    );
}

/// Finalized view classification with a resolved slot-resolution overlay returns
/// `WrapperTemplate` for a `String` template that still has structural slots.
///
/// WHAT: a `String` template with a `Slot` node and a slot-resolution overlay
///       should classify successfully. The structural tree has slots, so the
///       const-value kind is `WrapperTemplate` — the overlay will resolve the
///       slot at fold time, but classification reports the structural shape.
/// WHY: proves classification no longer rejects slot-resolution overlays and
///      still reports the correct const-value kind for slot-bearing templates.
#[test]
fn finalized_tir_view_classification_with_resolved_slot_returns_wrapper_template() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let (wrapper_template_id, fill_template_id) = {
        let mut store = store_handle.borrow_mut();

        // Fill template: "filled".
        let fill_text_id = string_table.intern("filled");
        let mut fill_builder = TemplateIrBuilder::new(&mut store);
        let fill_text_node = fill_builder.push_text_node(
            fill_text_id,
            "filled".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let fill_root = fill_builder.push_sequence_node(vec![fill_text_node], empty_location());
        let fill_template_id = fill_builder.finish_template(
            fill_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        // Wrapper template: "before" + $slot(default) + "after".
        let before_id = string_table.intern("before");
        let after_id = string_table.intern("after");
        let mut wrapper_builder = TemplateIrBuilder::new(&mut store);
        let before_node = wrapper_builder.push_text_node(
            before_id,
            "before".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let slot_node_id = wrapper_builder.push_slot_node(SlotKey::Default, empty_location());
        let after_node = wrapper_builder.push_text_node(
            after_id,
            "after".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let wrapper_root = wrapper_builder.push_sequence_node(
            vec![before_node, slot_node_id, after_node],
            empty_location(),
        );
        let wrapper_template_id = wrapper_builder.finish_template(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        (wrapper_template_id, fill_template_id)
    };

    // Build a slot-resolution overlay that resolves the default slot to the
    // fill template. The slot occurrence ID is 0 (first slot in the store).
    let fill_ref = TemplateRef::new(store_id, fill_template_id);
    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            SlotOccurrenceId::new(0),
            TirSlotResolution::resolved(SlotKey::Default, vec![fill_ref]),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, wrapper_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("classification with resolved slot overlay should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::WrapperTemplate,
        "a String template with structural slots should classify as WrapperTemplate"
    );
    assert!(
        !classification.has_slot_insertions,
        "no escaped slot-insert children should be present"
    );
}

/// Finalized view classification with a wrapper-context overlay succeeds.
///
/// WHAT: wrapper-context overlays are now a supported overlay dimension. Because
///       inherited wrappers wrap child-template emissions, they do not change
///       the parent template's own const-value shape.
/// WHY: classification should not force a fallback merely because a
///      wrapper-context overlay is present.
#[test]
fn finalized_tir_view_classification_accepts_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let wrapper_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay::default());
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    });
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text = string_table.intern("overlay");
        let text_node = builder.push_text_node(
            text,
            "overlay".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![text_node], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("wrapper-context overlays should not prevent classification");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "wrapper-context overlay should not change the parent const-value kind"
    );
}

/// Effective view classification with a structural slot but no slot-resolution
/// overlay returns `RenderableString`.
///
/// WHAT: the template still has a `Slot` node, so `has_unresolved_slots` stays
///       `true`. With no overlay covering the occurrence, the slot folds to no
///       output, so the const-value kind is `RenderableString`.
/// WHY: matches the fold path and the language rule that unfilled slots are
///      structural no-output.
#[test]
fn effective_view_classification_unresolved_slot_with_no_overlay_returns_renderable_string() {
    let string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
        let root = builder.push_sequence_node(vec![slot_node], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("effective view classification should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "an uncovered slot should classify as RenderableString"
    );
    assert!(
        classification.has_unresolved_slots,
        "structural slot flag must remain true"
    );
}

/// Effective view classification with a slot-resolution overlay that does not
/// cover the structural slot returns `RenderableString`.
///
/// WHAT: the overlay dimension exists but has no entry for the slot occurrence,
///       so the slot is still unfilled from the view's perspective.
/// WHY: proves the presence of an empty slot-resolution dimension alone does
///      not force a `WrapperTemplate` classification.
#[test]
fn effective_view_classification_unresolved_slot_with_empty_overlay_returns_renderable_string() {
    let string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
        let root = builder.push_sequence_node(vec![slot_node], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: Vec::new(),
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("effective view classification should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "a slot not covered by the overlay should classify as RenderableString"
    );
}

/// Effective view classification with a resolved slot returns `WrapperTemplate`.
///
/// WHAT: the slot-resolution overlay covers the structural slot with a
///       `Resolved` entry, so the template wraps the resolved source's content.
/// WHY: this is the existing resolved-slot expectation; the new
///      unresolved-slot logic must not regress it.
#[test]
fn effective_view_classification_resolved_slot_returns_wrapper_template() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let (wrapper_template_id, fill_template_id) = {
        let mut store = store_handle.borrow_mut();

        let fill_text_id = string_table.intern("filled");
        let mut fill_builder = TemplateIrBuilder::new(&mut store);
        let fill_text_node = fill_builder.push_text_node(
            fill_text_id,
            "filled".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let fill_root = fill_builder.push_sequence_node(vec![fill_text_node], empty_location());
        let fill_template_id = fill_builder.finish_template(
            fill_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        let before_id = string_table.intern("before");
        let after_id = string_table.intern("after");
        let mut wrapper_builder = TemplateIrBuilder::new(&mut store);
        let before_node = wrapper_builder.push_text_node(
            before_id,
            "before".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let slot_node_id = wrapper_builder.push_slot_node(SlotKey::Default, empty_location());
        let after_node = wrapper_builder.push_text_node(
            after_id,
            "after".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let wrapper_root = wrapper_builder.push_sequence_node(
            vec![before_node, slot_node_id, after_node],
            empty_location(),
        );
        let wrapper_template_id = wrapper_builder.finish_template(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        (wrapper_template_id, fill_template_id)
    };

    let fill_ref = TemplateRef::new(store_id, fill_template_id);
    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            SlotOccurrenceId::new(0),
            TirSlotResolution::resolved(SlotKey::Default, vec![fill_ref]),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, wrapper_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("classification with resolved slot overlay should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::WrapperTemplate,
        "a resolved slot should classify as WrapperTemplate"
    );
    assert!(
        classification.has_unresolved_slots,
        "structural slot flag must remain true"
    );
}

/// Effective view classification with two slots resolves only one returns
/// `WrapperTemplate`.
///
/// WHAT: at least one slot occurrence maps to `Resolved`, so the template wraps
///       content even though the other slot is uncovered.
/// WHY: the decision is "any resolved slot", not "all resolved".
#[test]
fn effective_view_classification_partially_resolved_slots_returns_wrapper_template() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let (wrapper_template_id, fill_template_id) = {
        let mut store = store_handle.borrow_mut();

        let fill_text_id = string_table.intern("filled");
        let mut fill_builder = TemplateIrBuilder::new(&mut store);
        let fill_text_node = fill_builder.push_text_node(
            fill_text_id,
            "filled".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let fill_root = fill_builder.push_sequence_node(vec![fill_text_node], empty_location());
        let fill_template_id = fill_builder.finish_template(
            fill_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        let mut wrapper_builder = TemplateIrBuilder::new(&mut store);
        let first_slot = wrapper_builder.push_slot_node(SlotKey::Default, empty_location());
        let second_slot = wrapper_builder.push_slot_node(SlotKey::Default, empty_location());
        let wrapper_root =
            wrapper_builder.push_sequence_node(vec![first_slot, second_slot], empty_location());
        let wrapper_template_id = wrapper_builder.finish_template(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        (wrapper_template_id, fill_template_id)
    };

    let fill_ref = TemplateRef::new(store_id, fill_template_id);
    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            SlotOccurrenceId::new(0),
            TirSlotResolution::resolved(SlotKey::Default, vec![fill_ref]),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, wrapper_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("classification with partially resolved slots should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::WrapperTemplate,
        "partially resolved slots should still classify as WrapperTemplate"
    );
}

/// Effective view classification with two slots and no resolutions returns
/// `RenderableString`.
///
/// WHAT: every structural slot is uncovered by the slot-resolution overlay, so
///       all of them fold to no output.
/// WHY: confirms the "all unresolved" case for multiple slots.
#[test]
fn effective_view_classification_two_unresolved_slots_returns_renderable_string() {
    let string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let store_handle = registry
        .store_handle(store_id)
        .expect("test store should be allocated");

    let template_id = {
        let mut store = store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let first_slot = builder.push_slot_node(SlotKey::Default, empty_location());
        let second_slot = builder.push_slot_node(SlotKey::Default, empty_location());
        let root = builder.push_sequence_node(vec![first_slot, second_slot], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: Vec::new(),
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    registry
        .freeze_store_with_domain(store_id, super::super::TemplateStringDomainId::new(0))
        .expect("test store should freeze");

    let store_snapshot = store_handle.borrow().clone();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should resolve");

    let classification =
        classify_effective_tir_view_template(&view, &store_snapshot, &string_table)
            .expect("effective view classification should succeed");

    assert_eq!(
        classification.const_value_kind,
        TemplateConstValueKind::RenderableString,
        "two uncovered slots should classify as RenderableString"
    );
}
