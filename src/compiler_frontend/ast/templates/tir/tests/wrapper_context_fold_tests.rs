//! TIR wrapper-context overlay fold tests.
//!
//! WHAT: exercises view-native folding of inherited `$children(..)` wrappers
//!       and `$fresh` suppression applied through wrapper-context overlays.
//!
//! WHY: wrapper-context overlays replace the structural mutation of
//!      `conditional_child_wrapper_set`. These tests prove the overlay path
//!      produces the same output as the current-state wrapper composition path.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template::{SlotKey, Style, TemplateSegmentOrigin};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::TirWrapperApplicationMode;
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold::fold_tir_view;
use crate::compiler_frontend::ast::templates::tir::fold_cache::TirFoldCache;
use crate::compiler_frontend::ast::templates::tir::fold_safety::classify_view_native_fold_safety;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, SlotOccurrenceId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::TemplateIrBranch;
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirSlotResolution, TirSlotResolutionOverlay,
    TirWrapperContext, TirWrapperContextOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference, TemplateWrapperReference,
    TemplateWrapperSetRef,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeTemplateBody, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;
use std::cell::RefCell;
use std::rc::Rc;

use super::assert_slot_insert_fold_error;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        &crate::libraries::SourceLibraryRegistry::default(),
        &crate::libraries::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

fn build_test_fold_context<'a>(
    string_table: &'a mut StringTable,
    resolver: &'a ProjectPathResolver,
    path_format: &'a PathStringFormatConfig,
    source_scope: &'a InternedPath,
    registry: &'a Rc<RefCell<TemplateIrRegistry>>,
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver: resolver,
        path_format_config: path_format,
        source_file_scope: source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    }
}

fn build_text_template(
    store: &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId {
    let text_id = string_table.intern(text);
    let mut builder = TemplateIrBuilder::new(store);
    let text_node = builder.push_text_node(
        text_id,
        text.len() as u32,
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
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

fn build_false_no_else_branch_template(
    store: &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
    string_table: &mut StringTable,
) -> crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId {
    let body_text = string_table.intern("hidden");
    let mut builder = TemplateIrBuilder::new(store);
    let body_node = builder.push_text_node(
        body_text,
        "hidden".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(bool_expression(false)),
        body_node,
        empty_location(),
    );
    let root = builder.push_branch_chain_node(vec![branch], None, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary {
            has_control_flow: true,
            ..TemplateIrSummary::empty()
        },
        empty_location(),
    )
}

fn build_slot_wrapper_template(
    store: &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
    string_table: &mut StringTable,
    before: &str,
    after: &str,
) -> crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId {
    let before_id = string_table.intern(before);
    let after_id = string_table.intern(after);
    let mut builder = TemplateIrBuilder::new(store);
    let before_node = builder.push_text_node(
        before_id,
        before.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    let after_node = builder.push_text_node(
        after_id,
        after.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root =
        builder.push_sequence_node(vec![before_node, slot_node, after_node], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

struct WrapperContextFixture {
    registry: Rc<RefCell<TemplateIrRegistry>>,
    store_id: TemplateStoreId,
    parent_template_id: crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    wrapper_template_id: Option<crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId>,
    overlay_set_id: TemplateOverlaySetId,
}

fn build_wrapper_context_fixture(
    string_table: &mut StringTable,
    wrapper_context: TirWrapperContext,
) -> WrapperContextFixture {
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    // Build the parent template and wrapper set while borrowing the store. The
    // overlay allocation happens afterward to avoid re-entrant registry borrows.
    let (parent_template_id, wrapper_template_id, wrapper_set_id, child_occurrence_id) = {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");
        let child_template_id = build_text_template(&mut store, string_table, "child");
        let wrapper_template_id =
            build_slot_wrapper_template(&mut store, string_table, "before", "after");

        // The parent contains one child-template occurrence. The wrapper set is
        // attached through the overlay, not through the child's structural
        // `conditional_child_wrapper_set`.
        let mut builder = TemplateIrBuilder::new(&mut store);
        let reference = TemplateTirChildReference::new(
            TemplateRef::new(store_id, child_template_id),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let child_node =
            builder.push_child_template_node_with_reference(reference, empty_location());
        let root = builder.push_sequence_node(vec![child_node], empty_location());

        let parent_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        // Register the wrapper set so the overlay can reference it. This mirrors
        // the store-local side table used by `$children(..)` wrappers.
        let wrapper_ref = TemplateWrapperReference::new(
            store.qualify_template_ref(wrapper_template_id),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        );
        let wrapper_set_id = store.push_or_reuse_wrapper_set(vec![wrapper_ref]);
        let occurrence_id = ChildTemplateOccurrenceId::new(0);

        (
            parent_id,
            wrapper_template_id,
            wrapper_set_id,
            occurrence_id,
        )
    };

    let wrapper_set_ref = TemplateWrapperSetRef::new(store_id, wrapper_set_id);
    let wrapper_overlay_id =
        registry
            .borrow_mut()
            .allocate_wrapper_context_overlay(TirWrapperContextOverlay {
                contexts: vec![(
                    child_occurrence_id,
                    TirWrapperContext {
                        inherited_wrapper_set: Some(wrapper_set_ref),
                        ..wrapper_context
                    },
                )],
            });
    let overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: None,
            wrapper_context: Some(wrapper_overlay_id),
        });

    WrapperContextFixture {
        registry,
        store_id,
        parent_template_id,
        wrapper_template_id: Some(wrapper_template_id),
        overlay_set_id,
    }
}

fn build_wrapper_context_fixture_with_child(
    string_table: &mut StringTable,
    wrapper_context: TirWrapperContext,
    build_child: impl FnOnce(
        &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
        &mut StringTable,
    ) -> crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
) -> WrapperContextFixture {
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let (parent_template_id, wrapper_template_id, wrapper_set_id, child_occurrence_id) = {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");
        let child_template_id = build_child(&mut store, string_table);
        let wrapper_template_id =
            build_slot_wrapper_template(&mut store, string_table, "before", "after");

        let mut builder = TemplateIrBuilder::new(&mut store);
        let reference = TemplateTirChildReference::new(
            TemplateRef::new(store_id, child_template_id),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let child_node =
            builder.push_child_template_node_with_reference(reference, empty_location());
        let root = builder.push_sequence_node(vec![child_node], empty_location());

        let parent_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        let wrapper_ref = TemplateWrapperReference::new(
            store.qualify_template_ref(wrapper_template_id),
            TemplateTirPhase::Finalized,
            TemplateOverlaySetId::empty(),
        );
        let wrapper_set_id = store.push_or_reuse_wrapper_set(vec![wrapper_ref]);

        (
            parent_id,
            wrapper_template_id,
            wrapper_set_id,
            ChildTemplateOccurrenceId::new(0),
        )
    };

    let wrapper_set_ref = TemplateWrapperSetRef::new(store_id, wrapper_set_id);
    let wrapper_overlay_id =
        registry
            .borrow_mut()
            .allocate_wrapper_context_overlay(TirWrapperContextOverlay {
                contexts: vec![(
                    child_occurrence_id,
                    TirWrapperContext {
                        inherited_wrapper_set: Some(wrapper_set_ref),
                        ..wrapper_context
                    },
                )],
            });
    let overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: None,
            wrapper_context: Some(wrapper_overlay_id),
        });

    WrapperContextFixture {
        registry,
        store_id,
        parent_template_id,
        wrapper_template_id: Some(wrapper_template_id),
        overlay_set_id,
    }
}

fn fold_fixture(
    fixture: &WrapperContextFixture,
    string_table: &mut StringTable,
) -> TemplateEmission {
    fold_fixture_result(fixture, string_table).expect("fold should succeed")
}

fn fold_fixture_result(
    fixture: &WrapperContextFixture,
    string_table: &mut StringTable,
) -> Result<TemplateEmission, TemplateError> {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let registry_borrow = fixture.registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("test view should construct");

    let store = registry_borrow
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context = build_test_fold_context(
        string_table,
        &resolver,
        &path_format,
        &source_scope,
        &fixture.registry,
    );

    fold_tir_view(&view, &store, &mut fold_context)
}

#[test]
fn fold_tir_view_applies_inherited_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(&mut string_table, TirWrapperContext::default());

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {:?}", other),
    };

    assert_eq!(
        string_table.resolve(output_id),
        "beforechildafter",
        "inherited wrapper should wrap child output"
    );
}

#[test]
fn fold_tir_view_rejects_slot_insert_from_wrapper_context_set() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(&mut string_table, TirWrapperContext::default());

    {
        let registry = fixture.registry.borrow();
        let mut store = registry
            .store_mut(fixture.store_id)
            .expect("store should remain mutable");
        assert!(
            store.set_template_kind(
                fixture
                    .wrapper_template_id
                    .expect("fixture should include its wrapper template"),
                TemplateType::SlotInsert(SlotKey::Default),
            )
        );
    }

    assert_slot_insert_fold_error(fold_fixture_result(&fixture, &mut string_table));
}

#[test]
fn fold_tir_view_rejects_slot_insert_from_effective_slot_source() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();

    let (wrapper_template_id, source_template_id) = {
        let registry_borrow = registry.borrow();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should be mutable");
        let wrapper = build_slot_wrapper_template(&mut store, &mut string_table, "", "");
        let source = build_text_template(&mut store, &mut string_table, "escaped");
        assert!(store.set_template_kind(source, TemplateType::SlotInsert(SlotKey::Default),));
        (wrapper, source)
    };

    let slot_overlay_id =
        registry
            .borrow_mut()
            .allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
                resolutions: vec![(
                    SlotOccurrenceId::new(0),
                    TirSlotResolution::resolved(
                        SlotKey::Default,
                        vec![TemplateRef::new(store_id, source_template_id)],
                    ),
                )],
            });
    let overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: Some(slot_overlay_id),
            wrapper_context: None,
        });

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let registry_borrow = registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(store_id, wrapper_template_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("slot-overlay view should construct");
    let store = registry_borrow
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut fold_context = build_test_fold_context(
        &mut string_table,
        &resolver,
        &path_format,
        &source_scope,
        &registry,
    );

    assert_slot_insert_fold_error(fold_tir_view(&view, &store, &mut fold_context));
}

#[test]
fn fold_tir_view_honors_fresh_suppression_in_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(
        &mut string_table,
        TirWrapperContext {
            inherited_wrapper_set: None,
            skip_parent_child_wrappers: true,
            application_mode: TirWrapperApplicationMode::Always,
        },
    );

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {:?}", other),
    };

    assert_eq!(
        string_table.resolve(output_id),
        "child",
        "$fresh suppression should prevent wrapper application"
    );
}

#[test]
fn view_native_fold_safety_accepts_safe_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(&mut string_table, TirWrapperContext::default());

    let registry_borrow = fixture.registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("test view should construct");

    let store = registry_borrow
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let reason = classify_view_native_fold_safety(&view, &store);
    assert!(
        reason.is_none(),
        "safe wrapper-context overlay should be foldable, got {:?}",
        reason
    );
}

#[test]
fn fold_tir_view_applies_if_child_emits_wrapper_when_child_outputs() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(
        &mut string_table,
        TirWrapperContext {
            inherited_wrapper_set: None,
            skip_parent_child_wrappers: false,
            application_mode: TirWrapperApplicationMode::IfChildEmits,
        },
    );

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {:?}", other),
    };

    assert_eq!(
        string_table.resolve(output_id),
        "beforechildafter",
        "IfChildEmits should wrap a child that structurally outputs"
    );
}

#[test]
fn fold_tir_view_skips_if_child_emits_wrapper_when_child_has_no_output() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture_with_child(
        &mut string_table,
        TirWrapperContext {
            inherited_wrapper_set: None,
            skip_parent_child_wrappers: false,
            application_mode: TirWrapperApplicationMode::IfChildEmits,
        },
        build_false_no_else_branch_template,
    );

    let emission = fold_fixture(&fixture, &mut string_table);
    assert_eq!(
        emission,
        TemplateEmission::NoOutput,
        "false no-else child should not render inherited wrappers"
    );
}

#[test]
fn view_native_fold_safety_accepts_wrapper_context_with_if_child_emits_mode() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(
        &mut string_table,
        TirWrapperContext {
            inherited_wrapper_set: None,
            skip_parent_child_wrappers: false,
            application_mode: TirWrapperApplicationMode::IfChildEmits,
        },
    );

    let registry_borrow = fixture.registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("test view should construct");

    let store = registry_borrow
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let reason = classify_view_native_fold_safety(&view, &store);
    assert!(
        reason.is_none(),
        "IfChildEmits mode should be accepted by the fold-safety gate, got {:?}",
        reason
    );
}

fn handoff_fixture_result(
    fixture: &WrapperContextFixture,
    string_table: &mut StringTable,
) -> Result<Option<OwnedRuntimeTemplateHandoff>, CompilerError> {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let registry_borrow = fixture.registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("test view should construct");

    let store = registry_borrow
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context = build_test_fold_context(
        string_table,
        &resolver,
        &path_format,
        &source_scope,
        &fixture.registry,
    );

    store.owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut fold_context)
}

fn handoff_fixture(
    fixture: &WrapperContextFixture,
    string_table: &mut StringTable,
) -> OwnedRuntimeTemplateHandoff {
    handoff_fixture_result(fixture, string_table)
        .expect("handoff should succeed")
        .expect("handoff should be present")
}

fn assert_text_node(node: &OwnedRuntimeTemplateNode, expected: &str, string_table: &StringTable) {
    match node {
        OwnedRuntimeTemplateNode::Text { text, .. } => {
            assert_eq!(string_table.resolve(*text), expected);
        }
        other => panic!("expected Text node, got {:?}", other),
    }
}

fn assert_text_body(body: &OwnedRuntimeTemplateBody, expected: &str, string_table: &StringTable) {
    match body {
        OwnedRuntimeTemplateBody::Render(node) => assert_text_node(node, expected, string_table),
        other => panic!("expected Render body, got {:?}", other),
    }
}

fn assert_child_or_text_node(
    node: &OwnedRuntimeTemplateNode,
    expected: &str,
    string_table: &StringTable,
) {
    match node {
        OwnedRuntimeTemplateNode::Text { text, .. } => {
            assert_eq!(string_table.resolve(*text), expected);
        }
        OwnedRuntimeTemplateNode::ChildTemplate { template, .. } => {
            assert_text_body(&template.body, expected, string_table);
        }
        other => panic!("expected Text or ChildTemplate node, got {:?}", other),
    }
}

fn expect_single_render_child(body: &OwnedRuntimeTemplateBody) -> &OwnedRuntimeTemplateNode {
    match body {
        OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Sequence {
            children, ..
        }) => {
            assert_eq!(
                children.len(),
                1,
                "expected parent root to be a single-child sequence, got {:?}",
                children
            );
            &children[0]
        }
        other => panic!("expected Render(Sequence) body, got {:?}", other),
    }
}

#[test]
fn handoff_tir_view_applies_inherited_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(&mut string_table, TirWrapperContext::default());
    let handoff = handoff_fixture(&fixture, &mut string_table);

    let wrapped = expect_single_render_child(&handoff.body);
    let children = match wrapped {
        OwnedRuntimeTemplateNode::Sequence { children, .. } => children,
        other => panic!("expected Sequence wrapper root, got {:?}", other),
    };

    assert_eq!(
        children.len(),
        3,
        "wrapper should produce before + child + after"
    );
    assert_text_node(&children[0], "before", &string_table);
    assert_child_or_text_node(&children[1], "child", &string_table);
    assert_text_node(&children[2], "after", &string_table);
}

#[test]
fn handoff_tir_view_honors_fresh_suppression_in_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(
        &mut string_table,
        TirWrapperContext {
            inherited_wrapper_set: None,
            skip_parent_child_wrappers: true,
            application_mode: TirWrapperApplicationMode::Always,
        },
    );
    let handoff = handoff_fixture(&fixture, &mut string_table);

    let child_node = expect_single_render_child(&handoff.body);
    assert_text_node(child_node, "child", &string_table);
}

#[test]
fn handoff_tir_view_materializes_if_child_emits_as_conditional_wrapper() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(
        &mut string_table,
        TirWrapperContext {
            inherited_wrapper_set: None,
            skip_parent_child_wrappers: false,
            application_mode: TirWrapperApplicationMode::IfChildEmits,
        },
    );
    let handoff = handoff_fixture(&fixture, &mut string_table);

    let wrapped = expect_single_render_child(&handoff.body);
    let OwnedRuntimeTemplateNode::ConditionalWrapper { child, wrapper, .. } = wrapped else {
        panic!("expected ConditionalWrapper, got {:?}", wrapped);
    };

    assert_child_or_text_node(child, "child", &string_table);
    let OwnedRuntimeTemplateNode::Sequence { children, .. } = wrapper.as_ref() else {
        panic!("expected wrapper sequence, got {:?}", wrapper);
    };
    assert_eq!(children.len(), 3);
    assert_text_node(&children[0], "before", &string_table);
    assert!(matches!(
        children[1],
        OwnedRuntimeTemplateNode::AggregateOutput { .. }
    ));
    assert_text_node(&children[2], "after", &string_table);
}

#[test]
fn handoff_tir_view_rejects_cross_store_wrapper_set() {
    let mut string_table = StringTable::new();
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();
    let other_store_id = registry.borrow_mut().allocate_store();
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let (parent_template_id, child_occurrence_id) = {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");
        let child_template_id = build_text_template(&mut store, &mut string_table, "child");

        let mut builder = TemplateIrBuilder::new(&mut store);
        let reference = TemplateTirChildReference::new(
            TemplateRef::new(store_id, child_template_id),
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let child_node =
            builder.push_child_template_node_with_reference(reference, empty_location());
        let root = builder.push_sequence_node(vec![child_node], empty_location());

        let parent_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        (parent_id, ChildTemplateOccurrenceId::new(0))
    };

    let wrapper_set_ref = TemplateWrapperSetRef::new(other_store_id, TemplateWrapperSetId::new(0));
    let wrapper_overlay_id =
        registry
            .borrow_mut()
            .allocate_wrapper_context_overlay(TirWrapperContextOverlay {
                contexts: vec![(
                    child_occurrence_id,
                    TirWrapperContext {
                        inherited_wrapper_set: Some(wrapper_set_ref),
                        ..TirWrapperContext::default()
                    },
                )],
            });
    let overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: None,
            wrapper_context: Some(wrapper_overlay_id),
        });

    let fixture = WrapperContextFixture {
        registry,
        store_id,
        parent_template_id,
        wrapper_template_id: None,
        overlay_set_id,
    };

    let result = handoff_fixture_result(&fixture, &mut string_table);
    assert!(
        result.is_err(),
        "cross-store wrapper set should be rejected by HIR handoff"
    );
}

#[test]
fn handoff_tir_view_folded_child_text_still_fires_under_wrapper_context_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(&mut string_table, TirWrapperContext::default());
    let handoff = handoff_fixture(&fixture, &mut string_table);

    let wrapped = expect_single_render_child(&handoff.body);
    let children = match wrapped {
        OwnedRuntimeTemplateNode::Sequence { children, .. } => children,
        other => panic!("expected Sequence wrapper root, got {:?}", other),
    };

    assert_eq!(
        children.len(),
        3,
        "folded child should still be wrapped as before + text + after"
    );
    assert_text_node(&children[0], "before", &string_table);
    assert_text_node(&children[1], "child", &string_table);
    assert!(
        !matches!(children[1], OwnedRuntimeTemplateNode::ChildTemplate { .. }),
        "folded child shortcut should fire; found ChildTemplate"
    );
    assert_text_node(&children[2], "after", &string_table);
}
