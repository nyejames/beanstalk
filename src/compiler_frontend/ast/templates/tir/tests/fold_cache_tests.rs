//! TIR fold-cache unit tests.
//!
//! WHAT: exercises the cache key, result, and map types used by TIR folding.
//!
//! WHY: the cache data shape must be deterministic and correct before any
//!      production folding integration. These tests prove key equality,
//!      inequality, and lookup/insert behavior without relying on the full
//!      fold pipeline.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{SlotKey, Template};
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateFoldBinding,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold::fold_tir_template;
use crate::compiler_frontend::ast::templates::tir::fold_cache::{TirFoldCache, TirFoldCacheKey};
use crate::compiler_frontend::ast::templates::tir::fold_safety::classify_view_native_fold_safety;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
    TemplateSlotPlanId, TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirExpressionOverlayId,
    TirSlotResolution, TirSlotResolutionOverlay, TirWrapperApplicationMode, TirWrapperContext,
    TirWrapperContextOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference, TemplateWrapperReference,
    TemplateWrapperSetRef,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotPlan;
use crate::compiler_frontend::ast::templates::tir::store::{TemplateIrStore, TemplateWrapperSet};
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::summary::summarize_existing_root;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateTirReference, fold_tir_view, fold_tir_view_prepared, fold_tir_view_read_only,
    prepare_tir_view_fold, tir_view_is_expression_overlay_linear_fold_safe,
    tir_view_is_read_only_fold_safe,
};
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, InvalidTemplateStructureReason,
};
#[cfg(feature = "benchmark_counters")]
use crate::compiler_frontend::instrumentation::ast_counters::{
    AstCounter, reset_ast_counters, test_read_ast_counter,
};
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;
use std::cell::RefCell;
use std::rc::Rc;

fn sample_key() -> TirFoldCacheKey {
    TirFoldCacheKey {
        root: TemplateRef::new(TemplateStoreId::new(0), TemplateIrId::new(0)),
        phase: TemplateTirPhase::Parsed,
        overlay_set_id: TemplateOverlaySetId::empty_for_test(),
        loop_iteration_limit: 1024,
        bindings_empty: true,
    }
}

#[test]
fn cache_key_equality_for_identical_fields() {
    let a = sample_key();
    let b = sample_key();
    assert_eq!(a, b);
}

#[test]
fn cache_key_inequality_for_different_root() {
    let mut key = sample_key();
    key.root = TemplateRef::new(TemplateStoreId::new(0), TemplateIrId::new(1));
    assert_ne!(sample_key(), key);
}

#[test]
fn cache_key_inequality_for_different_phase() {
    let mut key = sample_key();
    key.phase = TemplateTirPhase::Formatted;
    assert_ne!(sample_key(), key);
}

#[test]
fn cache_key_inequality_for_different_overlay_set() {
    let mut key = sample_key();
    // Overlay-set IDs are opaque; any different numeric value is a different set.
    key.overlay_set_id = TemplateOverlaySetId::new(7);
    assert_ne!(sample_key(), key);
}

#[test]
fn cache_key_inequality_for_different_loop_limit() {
    let mut key = sample_key();
    key.loop_iteration_limit = 512;
    assert_ne!(sample_key(), key);
}

#[test]
fn cache_key_inequality_for_different_bindings_empty() {
    let mut key = sample_key();
    key.bindings_empty = false;
    assert_ne!(sample_key(), key);
}

#[test]
fn cache_lookup_miss_then_hit() {
    let mut string_table = StringTable::new();
    let output_id = string_table.intern("cached output");

    let mut cache = TirFoldCache::new();
    let key = sample_key();
    let emission = TemplateEmission::Output(output_id);

    assert!(cache.get(&key).is_none(), "fresh cache must miss");

    cache.insert(key, emission);

    let cached = cache
        .get(&key)
        .expect("cache must return the inserted emission");
    assert_eq!(*cached, emission);
}

#[test]
fn cache_insert_overwrites_previous_emission() {
    let mut string_table = StringTable::new();
    let first_id = string_table.intern("first");
    let second_id = string_table.intern("second");

    let mut cache = TirFoldCache::new();
    let key = sample_key();

    let first = TemplateEmission::Output(first_id);
    let second = TemplateEmission::Output(second_id);

    cache.insert(key, first);
    let previous = cache.insert(key, second);

    assert_eq!(previous, Some(first));
    assert_eq!(cache.get(&key), Some(&second));
}

// -------------------------
//  TirView fold entrypoint tests
// -------------------------

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(),
        &crate::builder_surface::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

fn build_test_fold_context<'a>(
    string_table: &'a mut StringTable,
    resolver: &'a ProjectPathResolver,
    path_format: &'a PathStringFormatConfig,
    source_scope: &'a InternedPath,
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver: resolver,
        path_format_config: path_format,
        source_file_scope: source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: None,
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    }
}

/// Builds a registry with one store containing one text-only template and an
/// empty overlay set. Returns the registry, the store-local template ID, and
/// the overlay set ID.
struct TestTextTemplate {
    registry: TemplateIrRegistry,
    store_id: TemplateStoreId,
    template_id: TemplateIrId,
    overlay_set_id: TemplateOverlaySetId,
}

fn build_text_template_registry(string_table: &mut StringTable, text: &str) -> TestTextTemplate {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let text_id = string_table.intern(text);

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut store);
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
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    TestTextTemplate {
        registry,
        store_id,
        template_id,
        overlay_set_id,
    }
}

fn build_child_template_registry() -> (TestTextTemplate, StringTable) {
    let mut string_table = StringTable::new();
    let text_id = string_table.intern("child");

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let child_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text_node =
            builder.push_text_node(text_id, 5, TemplateSegmentOrigin::Body, empty_location());
        let child_root = builder.push_sequence_node(vec![text_node], empty_location());
        builder.finish_template(
            child_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    (
        TestTextTemplate {
            registry,
            store_id,
            template_id: child_template_id,
            overlay_set_id,
        },
        string_table,
    )
}

/// Synthetic two-store fixture for store-qualified child-fold tests.
struct CrossStoreChildFixture {
    registry: TemplateIrRegistry,
    parent_store_id: TemplateStoreId,
    child_store_id: TemplateStoreId,
    parent_template_id: TemplateIrId,
    child_template_id: TemplateIrId,
    child_overlay_set_id: TemplateOverlaySetId,
}

fn finish_text_template(
    store: &mut TemplateIrStore,
    text: StringId,
    byte_len: u32,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let text = builder.push_text_node(
        text,
        byte_len,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![text], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

fn finish_single_child_template(
    store: &mut TemplateIrStore,
    child_reference: TemplateTirChildReference,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let child = builder.push_child_template_node_with_reference(child_reference, empty_location());
    let root = builder.push_sequence_node(vec![child], empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

fn build_cross_store_child_fixture(string_table: &mut StringTable) -> CrossStoreChildFixture {
    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let child_store_id = registry.allocate_store();

    // Allocate a separate empty set so the child's non-zero overlay identity
    // is observable in the fold-cache assertion.
    registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let child_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let child_text = string_table.intern("child");
    let child_template_id = {
        let mut child_store = registry
            .store_mut(child_store_id)
            .expect("child store should be mutable");

        finish_text_template(&mut child_store, child_text, "child".len() as u32)
    };

    let parent_template_id = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut parent_store);
        let reference = TemplateTirChildReference::new(
            TemplateRef::new(child_store_id, child_template_id),
            TemplateTirPhase::Formatted,
            child_overlay_set_id,
        );
        let first_child =
            builder.push_child_template_node_with_reference(reference, empty_location());
        let second_child =
            builder.push_child_template_node_with_reference(reference, empty_location());
        let root = builder.push_sequence_node(vec![first_child, second_child], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    CrossStoreChildFixture {
        registry,
        parent_store_id,
        child_store_id,
        parent_template_id,
        child_template_id,
        child_overlay_set_id,
    }
}

#[test]
fn fold_tir_view_matches_fold_tir_template_for_simple_text() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "hello view");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let via_view =
        fold_tir_view(&view, &store, &mut fold_context).expect("fold_tir_view should succeed");

    let fresh_store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut fresh_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    let via_template = fold_tir_template(&fresh_store, fixture.template_id, &mut fresh_context)
        .expect("fold_tir_template should succeed");

    assert_eq!(via_view, via_template);
    assert_eq!(
        via_view,
        TemplateEmission::Output(string_table.intern("hello view"))
    );
}

#[test]
fn prepared_view_fold_rejects_root_phase_and_overlay_identity_mismatches() {
    let mut string_table = StringTable::new();
    let mut fixture = build_text_template_registry(&mut string_table, "prepared identity");
    let alternate_template_id = {
        let store_handle = fixture
            .registry
            .store_handle(fixture.store_id)
            .expect("store handle should exist");
        let mut store = store_handle.borrow_mut();
        finish_text_template(
            &mut store,
            string_table.intern("alternate root"),
            "alternate root".len() as u32,
        )
    };
    let alternate_expression_overlay_id = fixture
        .registry
        .allocate_expression_overlay(TirExpressionOverlay::default());
    let alternate_overlay_set_id = fixture.registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(alternate_expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    assert_ne!(alternate_template_id, fixture.template_id);
    assert_ne!(alternate_overlay_set_id, fixture.overlay_set_id);

    let original_view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("original view should construct");
    let different_root_view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, alternate_template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("different-root view should construct");
    let different_phase_view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Formatted,
        fixture.overlay_set_id,
    )
    .expect("different-phase view should construct");
    let different_overlay_view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        alternate_overlay_set_id,
    )
    .expect("different-overlay view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let mut assert_mismatch = |target: &TirView<'_>, expected: &str| {
        let preparation = prepare_tir_view_fold(&original_view, &store, &string_table)
            .expect("original view preparation should succeed");
        let mut fold_context =
            build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
        let error = fold_tir_view_prepared(target, &store, &mut fold_context, preparation)
            .expect_err("prepared identity mismatch must fail before folding");
        assert!(
            format!("{error:?}").contains(expected),
            "expected {expected} identity error, got {error:?}"
        );
    };

    assert_mismatch(&different_root_view, "root");
    assert_mismatch(&different_phase_view, "phase");
    assert_mismatch(&different_overlay_view, "overlay set");
}

#[test]
fn prepared_view_fold_rejects_same_numeric_identity_with_different_store_owners() {
    let mut string_table = StringTable::new();
    let original_fixture = build_text_template_registry(&mut string_table, "original owner");
    let foreign_fixture = build_text_template_registry(&mut string_table, "foreign owner");

    assert_eq!(original_fixture.store_id, foreign_fixture.store_id);
    assert_eq!(original_fixture.template_id, foreign_fixture.template_id);
    assert_eq!(
        original_fixture.overlay_set_id,
        foreign_fixture.overlay_set_id
    );

    let original_view = TirView::new(
        &original_fixture.registry,
        TemplateRef::new(original_fixture.store_id, original_fixture.template_id),
        TemplateTirPhase::Composed,
        original_fixture.overlay_set_id,
    )
    .expect("original view should construct");
    let foreign_view = TirView::new(
        &foreign_fixture.registry,
        TemplateRef::new(foreign_fixture.store_id, foreign_fixture.template_id),
        TemplateTirPhase::Composed,
        foreign_fixture.overlay_set_id,
    )
    .expect("foreign view should construct");
    let original_store = original_fixture
        .registry
        .store_handle(original_fixture.store_id)
        .expect("original store handle should exist")
        .borrow()
        .clone();
    let foreign_store = foreign_fixture
        .registry
        .store_handle(foreign_fixture.store_id)
        .expect("foreign store handle should exist")
        .borrow()
        .clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let cache_key = TirFoldCacheKey {
        root: TemplateRef::new(original_fixture.store_id, original_fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: original_fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };

    let preparation = prepare_tir_view_fold(&original_view, &original_store, &string_table)
        .expect("original view preparation should succeed");
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    let error = fold_tir_view_prepared(
        &original_view,
        &foreign_store,
        &mut fold_context,
        preparation,
    )
    .expect_err("foreign supplied store must be rejected before cache lookup");
    assert!(
        format!("{error:?}").contains("store owner does not match the supplied store"),
        "unexpected supplied-store owner error: {error:?}"
    );
    assert!(
        fold_context.fold_cache.get(&cache_key).is_none(),
        "supplied-store owner rejection must not touch the cache"
    );

    let preparation = prepare_tir_view_fold(&original_view, &original_store, &string_table)
        .expect("original view preparation should succeed again");
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    let error = fold_tir_view_prepared(
        &foreign_view,
        &original_store,
        &mut fold_context,
        preparation,
    )
    .expect_err("foreign view owner must be rejected before cache lookup");
    assert!(
        format!("{error:?}").contains("store owner does not match the view's registered store"),
        "unexpected view-store owner error: {error:?}"
    );
    assert!(
        fold_context.fold_cache.get(&cache_key).is_none(),
        "view-store owner rejection must not touch the cache"
    );
}

#[test]
fn fold_tir_view_caches_result_for_empty_bindings() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "cache me");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let first = fold_tir_view(&view, &store, &mut fold_context).expect("first fold should succeed");

    let cache_key = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&cache_key).is_some(),
        "empty-binding fold should be cached"
    );

    let second =
        fold_tir_view(&view, &store, &mut fold_context).expect("second fold should succeed");
    assert_eq!(first, second);
}

#[test]
fn fold_tir_view_cache_hit_still_validates_malformed_authority() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "cached authority");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let mut store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    fold_tir_view(&view, &store, &mut fold_context).expect("first fold should populate cache");
    store.nodes.clear();

    let error = fold_tir_view(&view, &store, &mut fold_context)
        .expect_err("cache hits must not hide malformed current-store authority");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing cached node should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: node"),
        "expected a stable cache-boundary authority error, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_runtime_plan_early_return_validates_plan_authority() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "runtime plan");
    let missing_slot_plan_id = TemplateSlotPlanId::new(999);
    fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow_mut()
        .templates[fixture.template_id.index()]
    .runtime_slot_plan = Some(missing_slot_plan_id);

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");
    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let error = fold_tir_view(&view, &store, &mut fold_context)
        .expect_err("runtime-plan early return must validate its required plan");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing runtime slot plan should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: slot plan"),
        "expected a stable runtime-plan authority error, got: {}",
        error.msg
    );
}

fn fold_dynamic_ast_template_with_malformed_authority(
    malformed_runtime_slot_plan: bool,
) -> TemplateError {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let outer_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let nested_template_id = if malformed_runtime_slot_plan {
            let nested_text = string_table.intern("nested");
            let nested_template_id = {
                let mut builder = TemplateIrBuilder::new(&mut store);
                let nested_text_node = builder.push_text_node(
                    nested_text,
                    "nested".len() as u32,
                    TemplateSegmentOrigin::Body,
                    empty_location(),
                );
                let nested_root =
                    builder.push_sequence_node(vec![nested_text_node], empty_location());
                builder.finish_template(
                    nested_root,
                    Style::default(),
                    TemplateType::String,
                    TemplateIrSummary::empty(),
                    empty_location(),
                )
            };
            store.templates[nested_template_id.index()].runtime_slot_plan =
                Some(TemplateSlotPlanId::new(999));
            nested_template_id
        } else {
            store.push_template(TemplateIr::new(
                TemplateIrNodeId::new(999),
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::empty(),
                empty_location(),
            ))
        };

        let nested_template = Template {
            kind: TemplateType::String,
            tir_reference: TemplateTirReference {
                root: TemplateRef::new(store_id, nested_template_id),
                store_owner: store.owner(),
                phase: TemplateTirPhase::Composed,
                overlay_set_id,
            },
            location: empty_location(),
        };

        let mut builder = TemplateIrBuilder::new(&mut store);
        let dynamic_node = builder.push_dynamic_expression_node(
            Expression::template(nested_template, ValueMode::ImmutableOwned),
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let outer_root = builder.push_sequence_node(vec![dynamic_node], empty_location());
        builder.finish_template(
            outer_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let registry = Rc::new(RefCell::new(registry));
    let registry_borrow = registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(store_id, outer_template_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("outer view should construct");
    let store = registry_borrow
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    fold_tir_view(&view, &store, &mut fold_context)
        .expect_err("dynamic AST templates must enter their own fold authority boundary")
}

#[test]
fn fold_tir_view_dynamic_ast_template_validates_malformed_root_authority() {
    let error = fold_dynamic_ast_template_with_malformed_authority(false);
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed dynamic template root should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: node"),
        "expected dynamic-template root authority failure, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_dynamic_ast_template_validates_runtime_slot_plan_authority() {
    let error = fold_dynamic_ast_template_with_malformed_authority(true);
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed dynamic template slot plan should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: slot plan"),
        "expected dynamic-template runtime-plan authority failure, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_validates_local_wrapper_context_before_foreign_child_opacity() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let foreign_child_text = string_table.intern("foreign child");

    let foreign_child_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");
        finish_text_template(
            &mut foreign_store,
            foreign_child_text,
            "foreign child".len() as u32,
        )
    };

    let parent_template_id = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");
        finish_single_child_template(
            &mut parent_store,
            TemplateTirChildReference::new(
                TemplateRef::new(foreign_store_id, foreign_child_template_id),
                TemplateTirPhase::Composed,
                empty_overlay_set_id,
            ),
        )
    };

    let wrapper_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
            contexts: vec![(
                ChildTemplateOccurrenceId::new(0),
                TirWrapperContext {
                    inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                        parent_store_id,
                        TemplateWrapperSetId::new(999),
                    )),
                    application_mode: TirWrapperApplicationMode::Always,
                    skip_parent_child_wrappers: false,
                },
            )],
        });
    let parent_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_context_overlay_id),
    });

    let registry = Rc::new(RefCell::new(registry));
    let registry_borrow = registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(parent_store_id, parent_template_id),
        TemplateTirPhase::Composed,
        parent_overlay_set_id,
    )
    .expect("parent view should construct");
    let parent_store = registry_borrow
        .store_handle(parent_store_id)
        .expect("parent store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let error = fold_tir_view(&view, &parent_store, &mut fold_context)
        .expect_err("local wrapper authority must be checked before foreign fallback");
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed local wrapper context should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: wrapper set"),
        "expected local wrapper-set authority failure, got: {}",
        error.msg
    );
}

fn fold_foreign_wrapper_with_malformed_authority(
    malformed_runtime_slot_plan: bool,
    application_mode: TirWrapperApplicationMode,
    child_emits: bool,
) -> TemplateError {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let wrapper_store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let wrapper_template_id = {
        let mut wrapper_store = registry
            .store_mut(wrapper_store_id)
            .expect("wrapper store should be mutable");
        if malformed_runtime_slot_plan {
            let wrapper_text = string_table.intern("wrapper");
            let wrapper_template_id = {
                let mut builder = TemplateIrBuilder::new(&mut wrapper_store);
                let wrapper_text_node = builder.push_text_node(
                    wrapper_text,
                    "wrapper".len() as u32,
                    TemplateSegmentOrigin::Body,
                    empty_location(),
                );
                let wrapper_root =
                    builder.push_sequence_node(vec![wrapper_text_node], empty_location());
                builder.finish_template(
                    wrapper_root,
                    Style::default(),
                    TemplateType::String,
                    TemplateIrSummary::empty(),
                    empty_location(),
                )
            };
            wrapper_store.templates[wrapper_template_id.index()].runtime_slot_plan =
                Some(TemplateSlotPlanId::new(999));
            wrapper_template_id
        } else {
            wrapper_store.push_template(TemplateIr::new(
                TemplateIrNodeId::new(999),
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::empty(),
                empty_location(),
            ))
        }
    };

    let (parent_template_id, wrapper_set_id) = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");
        let child_template_id = if child_emits {
            finish_text_template(
                &mut parent_store,
                string_table.intern("child"),
                "child".len() as u32,
            )
        } else {
            let hidden_text = string_table.intern("hidden");
            let mut builder = TemplateIrBuilder::new(&mut parent_store);
            let hidden_node = builder.push_text_node(
                hidden_text,
                "hidden".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let branch = TemplateIrBranch::new(
                TemplateBranchSelector::Bool(Expression::bool(
                    false,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                )),
                hidden_node,
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
        };
        let child_reference = TemplateTirChildReference::same_store(
            child_template_id,
            parent_store_id,
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let child_node = {
            let mut builder = TemplateIrBuilder::new(&mut parent_store);
            builder.push_child_template_node_with_reference(child_reference, empty_location())
        };
        let wrapper_set_id = parent_store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![TemplateWrapperReference::new(
                TemplateRef::new(wrapper_store_id, wrapper_template_id),
                TemplateTirPhase::Composed,
                empty_overlay_set_id,
            )],
        });
        let parent_root = parent_store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![child_node],
            },
            empty_location(),
        ));
        let parent_template_id = parent_store.push_template(TemplateIr::new(
            parent_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ));

        (parent_template_id, wrapper_set_id)
    };

    let wrapper_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
            contexts: vec![(
                ChildTemplateOccurrenceId::new(0),
                TirWrapperContext {
                    inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                        parent_store_id,
                        wrapper_set_id,
                    )),
                    application_mode,
                    skip_parent_child_wrappers: false,
                },
            )],
        });
    let parent_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_context_overlay_id),
    });

    let registry = Rc::new(RefCell::new(registry));
    let registry_borrow = registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(parent_store_id, parent_template_id),
        TemplateTirPhase::Composed,
        parent_overlay_set_id,
    )
    .expect("parent view should construct");
    let parent_store = registry_borrow
        .store_handle(parent_store_id)
        .expect("parent store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    fold_tir_view(&view, &parent_store, &mut fold_context)
        .expect_err("foreign wrapper authority must be validated at its fold boundary")
}

#[test]
fn fold_tir_view_foreign_wrapper_validates_malformed_root_authority() {
    let error = fold_foreign_wrapper_with_malformed_authority(
        false,
        TirWrapperApplicationMode::Always,
        true,
    );
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed foreign wrapper root should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: node"),
        "expected foreign-wrapper root authority failure, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_foreign_wrapper_validates_runtime_slot_plan_authority() {
    let error = fold_foreign_wrapper_with_malformed_authority(
        true,
        TirWrapperApplicationMode::Always,
        true,
    );
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed foreign wrapper slot plan should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: slot plan"),
        "expected foreign-wrapper runtime-plan authority failure, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_preflights_malformed_foreign_root_for_if_child_emits_no_output() {
    let error = fold_foreign_wrapper_with_malformed_authority(
        false,
        TirWrapperApplicationMode::IfChildEmits,
        false,
    );
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed foreign wrapper root should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: node"),
        "expected malformed foreign root validation before NoOutput shortcut, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_preflights_malformed_foreign_runtime_plan_for_if_child_emits_no_output() {
    let error = fold_foreign_wrapper_with_malformed_authority(
        true,
        TirWrapperApplicationMode::IfChildEmits,
        false,
    );
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed foreign wrapper slot plan should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: slot plan"),
        "expected malformed foreign runtime-plan validation before NoOutput shortcut, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_folds_production_summary_constant_dynamic_child_to_text() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let constant_text = string_table.intern("constant child");

    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");

        let site_id = store.next_expression_site_id();
        let dynamic_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(Expression::string_slice(
                    constant_text,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                )),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            empty_location(),
        ));
        let child_summary = summarize_existing_root(&store, dynamic_node);
        let child_template_id = store.push_template(TemplateIr::new(
            dynamic_node,
            Style::default(),
            TemplateType::String,
            child_summary,
            empty_location(),
        ));
        assert_eq!(
            store
                .get_template(child_template_id)
                .expect("child template should exist")
                .summary
                .dynamic_expression_count,
            1,
            "fixture must use the production dynamic-expression summary"
        );

        let occurrence_id = store.next_child_template_occurrence_id();
        let child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::same_store(
                    child_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    overlay_set_id,
                ),
                occurrence_id,
            },
            empty_location(),
        ));
        let parent_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![child_node],
            },
            empty_location(),
        ));
        let parent_summary = summarize_existing_root(&store, parent_root);
        store.push_template(TemplateIr::new(
            parent_root,
            Style::default(),
            TemplateType::String,
            parent_summary,
            empty_location(),
        ))
    };

    let registry_borrow = registry;
    let store = registry_borrow
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("parent view should construct");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let emission = fold_tir_view(&view, &store, &mut fold_context)
        .expect("constant dynamic child should fold successfully");
    assert_eq!(
        emission,
        TemplateEmission::Output(constant_text),
        "constant dynamic expressions remain foldable despite structural summary counts"
    );
}

#[test]
fn fold_tir_view_below_composed_child_ignores_unconsumed_overlay_identity() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let parent_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let missing_overlay_set_id = TemplateOverlaySetId::new(999);
    let child_text = string_table.intern("parsed child");

    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");
        let child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Text {
                text: child_text,
                byte_len: "parsed child".len() as u32,
                origin: TemplateSegmentOrigin::Body,
            },
            empty_location(),
        ));
        let child_summary = summarize_existing_root(&store, child_node);
        let child_template_id = store.push_template(TemplateIr::new(
            child_node,
            Style::default(),
            TemplateType::String,
            child_summary,
            empty_location(),
        ));
        let occurrence_id = store.next_child_template_occurrence_id();
        let parent_child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::same_store(
                    child_template_id,
                    store_id,
                    TemplateTirPhase::Parsed,
                    missing_overlay_set_id,
                ),
                occurrence_id,
            },
            empty_location(),
        ));
        let parent_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![parent_child_node],
            },
            empty_location(),
        ));
        let parent_summary = summarize_existing_root(&store, parent_root);
        store.push_template(TemplateIr::new(
            parent_root,
            Style::default(),
            TemplateType::String,
            parent_summary,
            empty_location(),
        ))
    };

    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Composed,
        parent_overlay_set_id,
    )
    .expect("parent view should construct");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    assert_eq!(
        fold_tir_view(&view, &store, &mut fold_context)
            .expect("below-Composed child should fold from its structural root"),
        TemplateEmission::Output(child_text)
    );
}

#[test]
fn fold_tir_view_preserves_registry_for_later_phase_descendant_of_parsed_child() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let root_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let expression_overlay_id =
        registry.allocate_expression_overlay(TirExpressionOverlay::default());
    let formatted_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let child_text = string_table.intern("later-phase child");

    let outer_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");
        let leaf_template_id =
            finish_text_template(&mut store, child_text, "later-phase child".len() as u32);
        let parsed_middle_template_id = finish_single_child_template(
            &mut store,
            TemplateTirChildReference::same_store(
                leaf_template_id,
                store_id,
                TemplateTirPhase::Formatted,
                formatted_overlay_set_id,
            ),
        );

        finish_single_child_template(
            &mut store,
            TemplateTirChildReference::same_store(
                parsed_middle_template_id,
                store_id,
                TemplateTirPhase::Parsed,
                root_overlay_set_id,
            ),
        )
    };

    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, outer_template_id),
        TemplateTirPhase::Composed,
        root_overlay_set_id,
    )
    .expect("outer view should construct");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    assert_eq!(
        fold_tir_view(&view, &store, &mut fold_context)
            .expect("later-phase descendant overlay should resolve through the root registry"),
        TemplateEmission::Output(child_text)
    );
}

#[test]
fn fold_tir_view_rejects_direct_sequence_node_cycle_as_infrastructure() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");
        let root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![TemplateIrNodeId::new(0)],
            },
            empty_location(),
        ));
        store.push_template(TemplateIr::new(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ))
    };
    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("cyclic view should construct");
    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut string_table = StringTable::new();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let TemplateError::Infrastructure(error) =
        fold_tir_view(&view, &store, &mut fold_context).expect_err("direct cycle must fail")
    else {
        panic!("direct node cycle must remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("recursively referenced directly"),
        "expected a direct-cycle authority error, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_reports_same_store_child_cycle_as_non_foldable_diagnostic() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let template_a_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");
        let template_a_id = store.push_template(TemplateIr::new(
            TemplateIrNodeId::new(0),
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ));
        let template_b_id = store.push_template(TemplateIr::new(
            TemplateIrNodeId::new(0),
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ));
        let occurrence_a = store.next_child_template_occurrence_id();
        let node_a = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::same_store(
                    template_b_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    overlay_set_id,
                ),
                occurrence_id: occurrence_a,
            },
            empty_location(),
        ));
        let occurrence_b = store.next_child_template_occurrence_id();
        let node_b = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::same_store(
                    template_a_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    overlay_set_id,
                ),
                occurrence_id: occurrence_b,
            },
            empty_location(),
        ));
        store.templates[template_a_id.index()].root = node_a;
        store.templates[template_b_id.index()].root = node_b;
        template_a_id
    };
    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, template_a_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("cyclic view should construct");
    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut string_table = StringTable::new();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let TemplateError::Diagnostic(diagnostic) =
        fold_tir_view(&view, &store, &mut fold_context).expect_err("child cycle must terminate")
    else {
        panic!("same-store child cycle must use the ordinary diagnostic lane");
    };
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::NonFoldableConstTemplate,
        }
    ));
}

#[test]
fn fold_tir_view_reports_cross_store_child_cycle_as_non_foldable_diagnostic() {
    let mut registry = TemplateIrRegistry::new();
    let store_a_id = registry.allocate_store();
    let store_b_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let template_a_id = {
        let mut store_a = registry
            .store_mut(store_a_id)
            .expect("store A should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut store_a);
        let child = builder.push_child_template_node_with_reference(
            TemplateTirChildReference::new(
                TemplateRef::new(store_b_id, TemplateIrId::new(0)),
                TemplateTirPhase::Composed,
                overlay_set_id,
            ),
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![child], empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };
    let _template_b_id = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut store_b);
        let child = builder.push_child_template_node_with_reference(
            TemplateTirChildReference::new(
                TemplateRef::new(store_a_id, template_a_id),
                TemplateTirPhase::Composed,
                overlay_set_id,
            ),
            empty_location(),
        );
        let root = builder.push_sequence_node(vec![child], empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let registry = Rc::new(RefCell::new(registry));
    let registry_borrow = registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(store_a_id, template_a_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("cross-store cycle view should construct");
    let store_a = registry_borrow
        .store_handle(store_a_id)
        .expect("store A handle should exist")
        .borrow()
        .clone();
    let mut string_table = StringTable::new();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let TemplateError::Diagnostic(diagnostic) = fold_tir_view(&view, &store_a, &mut fold_context)
        .expect_err("cross-store child cycle must terminate")
    else {
        panic!("cross-store child cycle must use the ordinary diagnostic lane");
    };
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::NonFoldableConstTemplate,
        }
    ));
}

#[test]
fn fold_tir_view_does_not_cache_with_active_bindings() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "binding test");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let unused_binding_value = string_table.intern("unused");
    let expected_output = string_table.intern("binding test");

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    fold_context.bindings.push(TemplateFoldBinding {
        path: InternedPath::new(),
        value: crate::compiler_frontend::ast::expressions::expression::Expression::string_slice(
            unused_binding_value,
            empty_location(),
            crate::compiler_frontend::value_mode::ValueMode::ImmutableOwned,
        ),
    });

    let result = fold_tir_view(&view, &store, &mut fold_context)
        .expect("fold with bindings should still succeed");
    assert_eq!(result, TemplateEmission::Output(expected_output));

    let cache_key = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: false,
    };
    assert!(
        fold_context.fold_cache.get(&cache_key).is_none(),
        "non-empty-binding fold should not be cached"
    );
}

#[test]
fn fold_tir_view_rejects_store_mismatch_before_folding() {
    let mut string_table = StringTable::new();
    let mut fixture = build_text_template_registry(&mut string_table, "store mismatch");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let wrong_store_id = fixture.registry.allocate_store();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let wrong_store = fixture
        .registry
        .store_handle(wrong_store_id)
        .expect("wrong store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let error = fold_tir_view(&view, &wrong_store, &mut fold_context)
        .expect_err("mismatched view/store should be rejected before folding");

    assert!(
        matches!(error, TemplateError::Infrastructure(_)),
        "view/store mismatch is an internal TIR invariant"
    );
    assert!(
        fold_context
            .fold_cache
            .get(&TirFoldCacheKey {
                root: TemplateRef::new(fixture.store_id, fixture.template_id),
                phase: TemplateTirPhase::Composed,
                overlay_set_id: fixture.overlay_set_id,
                loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
                bindings_empty: true,
            })
            .is_none(),
        "failed store mismatch should not populate the fold cache"
    );
}

#[test]
fn fold_tir_view_cache_key_includes_phase() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "phase key");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    for phase in [TemplateTirPhase::Composed, TemplateTirPhase::Formatted] {
        let view = TirView::new(
            &fixture.registry,
            TemplateRef::new(fixture.store_id, fixture.template_id),
            phase,
            fixture.overlay_set_id,
        )
        .expect("view should construct");

        let store = fixture
            .registry
            .store_handle(fixture.store_id)
            .expect("store handle should exist")
            .borrow()
            .clone();

        fold_tir_view(&view, &store, &mut fold_context).expect("fold should succeed");
    }

    let key_composed = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    let key_formatted = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Formatted,
        overlay_set_id: fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&key_composed).is_some(),
        "cache should contain an entry for Composed phase"
    );
    assert!(
        fold_context.fold_cache.get(&key_formatted).is_some(),
        "cache should contain an entry for Formatted phase"
    );
}

#[test]
fn fold_tir_view_rejects_parsed_phase_without_caching() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "parsed phase");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Parsed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let error = fold_tir_view(&view, &store, &mut fold_context)
        .expect_err("Parsed view should not fold through the view entrypoint");

    assert!(
        matches!(error, TemplateError::Infrastructure(_)),
        "below-minimum phase is an internal TIR invariant"
    );
    assert!(
        fold_context
            .fold_cache
            .get(&TirFoldCacheKey {
                root: TemplateRef::new(fixture.store_id, fixture.template_id),
                phase: TemplateTirPhase::Parsed,
                overlay_set_id: fixture.overlay_set_id,
                loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
                bindings_empty: true,
            })
            .is_none(),
        "failed phase gate should not populate the fold cache"
    );
}

#[test]
fn fold_tir_view_cache_key_includes_overlay_set() {
    let mut string_table = StringTable::new();
    let mut fixture = build_text_template_registry(&mut string_table, "overlay key");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let second_overlay_set_id = fixture
        .registry
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    for overlay_set_id in [fixture.overlay_set_id, second_overlay_set_id] {
        let view = TirView::new(
            &fixture.registry,
            TemplateRef::new(fixture.store_id, fixture.template_id),
            TemplateTirPhase::Composed,
            overlay_set_id,
        )
        .expect("view should construct");

        let store = fixture
            .registry
            .store_handle(fixture.store_id)
            .expect("store handle should exist")
            .borrow()
            .clone();

        fold_tir_view(&view, &store, &mut fold_context).expect("fold should succeed");
    }

    let key_first_overlay = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    let key_second_overlay = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: second_overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&key_first_overlay).is_some(),
        "cache should contain an entry for the first overlay set"
    );
    assert!(
        fold_context.fold_cache.get(&key_second_overlay).is_some(),
        "cache should contain an entry for the second overlay set"
    );
}

#[test]
fn fold_tir_view_cache_key_includes_loop_limit() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "loop limit key");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let low_cached = {
        let mut fold_context = TemplateFoldContext {
            string_table: &mut string_table,
            project_path_resolver: &resolver,
            path_format_config: &path_format,
            source_file_scope: &source_scope,
            template_const_loop_iteration_limit: 100,
            template_ir_registry: None,
            bindings: vec![],
            fold_cache: TirFoldCache::new(),
        };
        fold_tir_view(&view, &store, &mut fold_context)
            .expect("fold with low limit should succeed");
        let key_low = TirFoldCacheKey {
            root: TemplateRef::new(fixture.store_id, fixture.template_id),
            phase: TemplateTirPhase::Composed,
            overlay_set_id: fixture.overlay_set_id,
            loop_iteration_limit: 100,
            bindings_empty: true,
        };
        fold_context.fold_cache.get(&key_low).is_some()
    };

    let high_cached = {
        let mut fold_context = TemplateFoldContext {
            string_table: &mut string_table,
            project_path_resolver: &resolver,
            path_format_config: &path_format,
            source_file_scope: &source_scope,
            template_const_loop_iteration_limit: 200,
            template_ir_registry: None,
            bindings: vec![],
            fold_cache: TirFoldCache::new(),
        };
        fold_tir_view(&view, &store, &mut fold_context)
            .expect("fold with high limit should succeed");
        let key_high = TirFoldCacheKey {
            root: TemplateRef::new(fixture.store_id, fixture.template_id),
            phase: TemplateTirPhase::Composed,
            overlay_set_id: fixture.overlay_set_id,
            loop_iteration_limit: 200,
            bindings_empty: true,
        };
        fold_context.fold_cache.get(&key_high).is_some()
    };

    assert!(
        low_cached,
        "cache should contain an entry for the low loop limit"
    );
    assert!(
        high_cached,
        "cache should contain an entry for the high loop limit"
    );
}

#[test]
fn fold_tir_view_repeated_child_template_folding_hits_cache() {
    let (fixture, mut string_table) = build_child_template_registry();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let child_view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("child view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let expected_output = string_table.intern("child");

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let first = fold_tir_view(&child_view, &store, &mut fold_context)
        .expect("first child fold should succeed");
    assert_eq!(first, TemplateEmission::Output(expected_output));

    let cache_key = TirFoldCacheKey {
        root: TemplateRef::new(fixture.store_id, fixture.template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id: fixture.overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    let before_second = fold_context.fold_cache.get(&cache_key).cloned();

    let second = fold_tir_view(&child_view, &store, &mut fold_context)
        .expect("second child fold should succeed");

    assert!(
        before_second.is_some(),
        "first child fold should have populated the cache"
    );
    assert_eq!(
        fold_context.fold_cache.get(&cache_key),
        before_second.as_ref(),
        "repeated child-template fold should return the cached result without changing output"
    );
    assert_eq!(first, second);
}

#[test]
fn fold_tir_template_child_nodes_use_view_cache_when_registry_is_available() {
    let mut string_table = StringTable::new();
    let child_text_id = string_table.intern("child");

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (child_template_id, parent_template_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let child_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child_text_node = builder.push_text_node(
                child_text_id,
                "child".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let child_root = builder.push_sequence_node(vec![child_text_node], empty_location());
            builder.finish_template(
                child_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            )
        };

        let parent_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child_reference = TemplateTirChildReference::same_store(
                child_template_id,
                store_id,
                TemplateTirPhase::Composed,
                overlay_set_id,
            );
            let first_child =
                builder.push_child_template_node_with_reference(child_reference, empty_location());
            let second_child =
                builder.push_child_template_node_with_reference(child_reference, empty_location());
            let parent_root =
                builder.push_sequence_node(vec![first_child, second_child], empty_location());
            builder.finish_template(
                parent_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            )
        };

        (child_template_id, parent_template_id)
    };

    let registry = Rc::new(RefCell::new(registry));
    let store_handle = {
        let registry = registry.borrow();
        registry
            .store_handle(store_id)
            .expect("store handle should exist")
    };
    let store = store_handle.borrow().clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let expected_output = string_table.intern("childchild");

    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let result = fold_tir_template(&store, parent_template_id, &mut fold_context)
        .expect("parent fold should recurse through child view");

    assert_eq!(result, TemplateEmission::Output(expected_output));

    let child_cache_key = TirFoldCacheKey {
        root: TemplateRef::new(store_id, child_template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&child_cache_key).is_some(),
        "registry-backed child folding should populate the child TirView cache entry"
    );
}

/// Parsed child references are valid construction-time fold inputs. They must
/// bypass the Composed `TirView` shortcut and fold from their structural roots
/// in both the current store and a registered foreign store.
#[test]
fn fold_tir_template_accepts_parsed_same_and_cross_store_children() {
    let mut string_table = StringTable::new();
    let child_text_id = string_table.intern("child");

    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();

    let foreign_child_template_id = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should be mutable");

        finish_text_template(&mut foreign_store, child_text_id, "child".len() as u32)
    };

    let (same_store_parent_id, cross_store_parent_id) = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");

        let local_child_template_id =
            finish_text_template(&mut parent_store, child_text_id, "child".len() as u32);

        let same_store_parent_id = finish_single_child_template(
            &mut parent_store,
            TemplateTirChildReference::same_store(
                local_child_template_id,
                parent_store_id,
                TemplateTirPhase::Parsed,
                TemplateOverlaySetId::empty(),
            ),
        );

        let cross_store_parent_id = finish_single_child_template(
            &mut parent_store,
            TemplateTirChildReference::new(
                TemplateRef::new(foreign_store_id, foreign_child_template_id),
                TemplateTirPhase::Parsed,
                TemplateOverlaySetId::empty(),
            ),
        );

        (same_store_parent_id, cross_store_parent_id)
    };

    let registry = Rc::new(RefCell::new(registry));
    let parent_store = registry
        .borrow()
        .store_handle(parent_store_id)
        .expect("parent store should exist")
        .borrow()
        .clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let expected_output = TemplateEmission::Output(child_text_id);
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let same_store_output =
        fold_tir_template(&parent_store, same_store_parent_id, &mut fold_context)
            .expect("Parsed same-store child should fold from its structural root");
    let cross_store_output =
        fold_tir_template(&parent_store, cross_store_parent_id, &mut fold_context)
            .expect("Parsed cross-store child should fold from its structural root");

    assert_eq!(same_store_output, expected_output);
    assert_eq!(cross_store_output, expected_output);
}

#[test]
fn fold_tir_template_resolves_cross_store_child_with_exact_view_identity() {
    let mut string_table = StringTable::new();
    let fixture = build_cross_store_child_fixture(&mut string_table);
    let expected_output = string_table.intern("childchild");
    let child_cache_key = TirFoldCacheKey {
        root: TemplateRef::new(fixture.child_store_id, fixture.child_template_id),
        phase: TemplateTirPhase::Formatted,
        overlay_set_id: fixture.child_overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };

    let registry = Rc::new(RefCell::new(fixture.registry));
    let parent_store_handle = registry
        .borrow()
        .store_handle(fixture.parent_store_id)
        .expect("parent store should exist");
    let parent_store = parent_store_handle.borrow().clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let result = fold_tir_template(&parent_store, fixture.parent_template_id, &mut fold_context)
        .expect("cross-store child fold should use its registered store");

    assert_eq!(result, TemplateEmission::Output(expected_output));
    assert!(
        fold_context.fold_cache.get(&child_cache_key).is_some(),
        "cross-store child fold should cache the referenced root, phase, and overlay set"
    );
}

#[test]
fn fold_tir_template_rejects_cross_store_child_without_registry() {
    let mut string_table = StringTable::new();
    let fixture = build_cross_store_child_fixture(&mut string_table);
    let parent_store = fixture
        .registry
        .store_handle(fixture.parent_store_id)
        .expect("parent store should exist")
        .borrow()
        .clone();
    let child_root = TemplateRef::new(fixture.child_store_id, fixture.child_template_id);

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let error = fold_tir_template(&parent_store, fixture.parent_template_id, &mut fold_context)
        .expect_err("cross-store child fold without a registry should fail");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing registry should be an internal TIR invariant");
    };

    assert_eq!(
        error.msg,
        format!(
            "TIR fold: cross-store child template {} requires the module-local registry, but none is available.",
            child_root
        )
    );
}

#[test]
fn fold_tir_template_rejects_unregistered_cross_store_child_store() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let missing_store_id = TemplateStoreId::new(1);
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let missing_child_reference = TemplateTirChildReference::new(
        TemplateRef::new(missing_store_id, TemplateIrId::new(0)),
        TemplateTirPhase::Composed,
        overlay_set_id,
    );

    let parent_template_id = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");
        let mut builder = TemplateIrBuilder::new(&mut parent_store);
        let child = builder
            .push_child_template_node_with_reference(missing_child_reference, empty_location());
        let root = builder.push_sequence_node(vec![child], empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let registry = Rc::new(RefCell::new(registry));
    let parent_store_handle = registry
        .borrow()
        .store_handle(parent_store_id)
        .expect("parent store should exist");
    let parent_store = parent_store_handle.borrow().clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let error = fold_tir_template(&parent_store, parent_template_id, &mut fold_context)
        .expect_err("unregistered child store should fail before view construction");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing child store should be an internal TIR invariant");
    };

    assert_eq!(
        error.msg,
        format!(
            "TIR fold: cross-store child template store {} is not registered.",
            missing_store_id
        )
    );
}

// -------------------------
//  Read-only view folding
// -------------------------

#[test]
fn read_only_view_fold_uses_live_store_for_safe_text_root() {
    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "borrowed");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store_handle = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist");
    let store = store_handle.borrow();

    assert!(
        tir_view_is_read_only_fold_safe(&view, &store)
            .expect("fold safety authority should resolve"),
        "text-only view should be safe for read-only folding"
    );

    let expected_output = string_table.intern("borrowed");
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    let result = fold_tir_view_read_only(&view, &store, &mut fold_context)
        .expect("read-only fold should succeed");

    assert_eq!(result, TemplateEmission::Output(expected_output));
}

#[test]
fn read_only_view_fold_accepts_same_store_child_templates() {
    let mut string_table = StringTable::new();
    let child_text_id = string_table.intern("child");

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let child_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child_text_node = builder.push_text_node(
                child_text_id,
                "child".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let child_root = builder.push_sequence_node(vec![child_text_node], empty_location());
            builder.finish_template(
                child_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            )
        };

        let child_reference = TemplateTirChildReference::same_store(
            child_template_id,
            store_id,
            TemplateTirPhase::Composed,
            overlay_set_id,
        );
        let mut builder = TemplateIrBuilder::new(&mut store);
        let first_child =
            builder.push_child_template_node_with_reference(child_reference, empty_location());
        let second_child =
            builder.push_child_template_node_with_reference(child_reference, empty_location());
        let parent_root =
            builder.push_sequence_node(vec![first_child, second_child], empty_location());
        builder.finish_template(
            parent_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("parent view should construct");

    let store_handle = registry
        .store_handle(store_id)
        .expect("store handle should exist");
    let store = store_handle.borrow();

    assert!(
        tir_view_is_read_only_fold_safe(&view, &store)
            .expect("fold safety authority should resolve"),
        "same-store child templates with empty overlays should be safe"
    );

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let expected_output = string_table.intern("childchild");
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    let result = fold_tir_view_read_only(&view, &store, &mut fold_context)
        .expect("parent read-only fold should succeed");

    assert_eq!(result, TemplateEmission::Output(expected_output));
}

#[test]
fn read_only_fold_safety_rejects_shapes_that_mutate_or_need_overlays() {
    let mut string_table = StringTable::new();

    let mut slot_fixture = build_same_store_slot_fixture(&mut string_table);
    let empty_slot_overlay_set_id = slot_fixture
        .registry
        .allocate_overlay_set(TemplateOverlaySet::empty());
    let slot_view = TirView::new(
        &slot_fixture.registry,
        TemplateRef::new(
            slot_fixture.wrapper_store_id,
            slot_fixture.wrapper_template_id,
        ),
        TemplateTirPhase::Composed,
        empty_slot_overlay_set_id,
    )
    .expect("slot view should construct");
    let slot_store_handle = slot_fixture
        .registry
        .store_handle(slot_fixture.wrapper_store_id)
        .expect("slot store handle should exist");
    let slot_store = slot_store_handle.borrow();
    assert!(
        !tir_view_is_read_only_fold_safe(&slot_view, &slot_store)
            .expect("fold safety authority should resolve"),
        "unresolved slots are rejected by the read-only gate; slot-overlay views use the overlay gate"
    );
    drop(slot_store);

    let slot_overlay_set_id = build_resolved_slot_overlay_set(&mut slot_fixture);
    let slot_overlay_view = TirView::new(
        &slot_fixture.registry,
        TemplateRef::new(
            slot_fixture.wrapper_store_id,
            slot_fixture.wrapper_template_id,
        ),
        TemplateTirPhase::Composed,
        slot_overlay_set_id,
    )
    .expect("slot overlay view should construct");
    let slot_overlay_store = slot_store_handle.borrow();
    assert!(
        !tir_view_is_read_only_fold_safe(&slot_overlay_view, &slot_overlay_store)
            .expect("fold safety authority should resolve"),
        "non-empty overlays are handled by the overlay fold-safety gate, not the read-only gate"
    );
    assert!(
        tir_view_is_expression_overlay_linear_fold_safe(&slot_overlay_view, &slot_overlay_store)
            .expect("fold safety authority should resolve"),
        "a plain resolved slot overlay can use the Phase 4 view-native fold path"
    );
    drop(slot_overlay_store);

    {
        let mut store = slot_store_handle.borrow_mut();
        store.templates[slot_fixture.wrapper_template_id.index()]
            .summary
            .wrapper_count = 1;
    }
    let slot_wrapper_store = slot_store_handle.borrow();
    assert!(
        !tir_view_is_expression_overlay_linear_fold_safe(&slot_overlay_view, &slot_wrapper_store)
            .expect("fold safety authority should resolve"),
        "slot overlays with $children wrapper metadata stay on the fallback until Phase 5"
    );
    drop(slot_wrapper_store);

    let mut slot_context_fixture = build_same_store_slot_fixture(&mut string_table);
    let slot_context_overlay_set_id = build_resolved_slot_overlay_set(&mut slot_context_fixture);
    let slot_context_view = TirView::new(
        &slot_context_fixture.registry,
        TemplateRef::new(
            slot_context_fixture.wrapper_store_id,
            slot_context_fixture.wrapper_template_id,
        ),
        TemplateTirPhase::Composed,
        slot_context_overlay_set_id,
    )
    .expect("slot-context view should construct");
    let slot_context_store_handle = slot_context_fixture
        .registry
        .store_handle(slot_context_fixture.wrapper_store_id)
        .expect("slot-context store handle should exist");
    {
        let mut store = slot_context_store_handle.borrow_mut();
        let wrapper_ref = TemplateRef::new(
            slot_context_fixture.wrapper_store_id,
            slot_context_fixture.fill_template_id,
        );
        let wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![TemplateWrapperReference::new(
                wrapper_ref,
                TemplateTirPhase::Finalized,
                TemplateOverlaySetId::empty(),
            )],
        });
        let root_id = store.templates[slot_context_fixture.wrapper_template_id.index()].root;
        let root_node = store.get_node(root_id).expect("wrapper root should exist");
        let TemplateIrNodeKind::Sequence { children } = &root_node.kind else {
            panic!("wrapper root should be a sequence");
        };
        let slot_node_id = children
            .iter()
            .copied()
            .find(|child_id| {
                store
                    .get_node(*child_id)
                    .is_some_and(|node| matches!(&node.kind, TemplateIrNodeKind::Slot { .. }))
            })
            .expect("wrapper root should contain a slot node");
        let TemplateIrNodeKind::Slot { placeholder } = &mut store.nodes[slot_node_id.index()].kind
        else {
            panic!("selected node should be a slot");
        };
        placeholder.child_wrapper_set = Some(wrapper_set_id);
    }
    let slot_context_store = slot_context_store_handle.borrow();
    assert!(
        !tir_view_is_expression_overlay_linear_fold_safe(&slot_context_view, &slot_context_store)
            .expect("fold safety authority should resolve"),
        "slot overlays with slot-local $children wrapper context stay on the fallback until Phase 5"
    );

    let wrapper_fixture = build_text_template_registry(&mut string_table, "wrapped");
    let wrapper_store_handle = wrapper_fixture
        .registry
        .store_handle(wrapper_fixture.store_id)
        .expect("wrapper store handle should exist");
    let wrapper_text_id = string_table.intern("wrapper");
    {
        let mut store = wrapper_store_handle.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let wrapper_text = builder.push_text_node(
            wrapper_text_id,
            "wrapper".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let wrapper_root = builder.push_sequence_node(vec![wrapper_text], empty_location());
        let wrapper_template_id = builder.finish_template(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let wrapper_ref = TemplateRef::new(wrapper_fixture.store_id, wrapper_template_id);
        let wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![TemplateWrapperReference::new(
                wrapper_ref,
                TemplateTirPhase::Finalized,
                TemplateOverlaySetId::empty(),
            )],
        });
        store.templates[wrapper_fixture.template_id.index()].conditional_child_wrapper_set =
            Some(wrapper_set_id);
    }
    let wrapper_view = TirView::new(
        &wrapper_fixture.registry,
        TemplateRef::new(wrapper_fixture.store_id, wrapper_fixture.template_id),
        TemplateTirPhase::Composed,
        wrapper_fixture.overlay_set_id,
    )
    .expect("wrapper view should construct");
    let wrapper_store = wrapper_store_handle.borrow();
    assert!(
        tir_view_is_read_only_fold_safe(&wrapper_view, &wrapper_store)
            .expect("fold safety authority should resolve"),
        "simple same-store conditional child wrappers are safe for view-native folding"
    );
    drop(wrapper_store);

    let unsafe_wrapper_fixture = build_text_template_registry(&mut string_table, "unsafe child");
    let unsafe_wrapper_store_handle = unsafe_wrapper_fixture
        .registry
        .store_handle(unsafe_wrapper_fixture.store_id)
        .expect("unsafe wrapper store handle should exist");
    let unsafe_inner_text_id = string_table.intern("inner");
    {
        let mut store = unsafe_wrapper_store_handle.borrow_mut();

        let mut inner_builder = TemplateIrBuilder::new(&mut store);
        let inner_text = inner_builder.push_text_node(
            unsafe_inner_text_id,
            "inner".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let inner_root = inner_builder.push_sequence_node(vec![inner_text], empty_location());
        let inner_wrapper_template_id = inner_builder.finish_template(
            inner_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let inner_wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![TemplateWrapperReference::new(
                TemplateRef::new(unsafe_wrapper_fixture.store_id, inner_wrapper_template_id),
                TemplateTirPhase::Finalized,
                TemplateOverlaySetId::empty(),
            )],
        });

        let mut outer_builder = TemplateIrBuilder::new(&mut store);
        let slot_node = outer_builder.push_slot_node(SlotKey::Default, empty_location());
        let wrapper_root = outer_builder.push_sequence_node(vec![slot_node], empty_location());
        let outer_wrapper_template_id = outer_builder.finish_template(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );

        let TemplateIrNodeKind::Slot { placeholder } = &mut store.nodes[slot_node.index()].kind
        else {
            panic!("selected node should be a slot");
        };
        placeholder.child_wrapper_set = Some(inner_wrapper_set_id);

        let outer_wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![TemplateWrapperReference::new(
                TemplateRef::new(unsafe_wrapper_fixture.store_id, outer_wrapper_template_id),
                TemplateTirPhase::Finalized,
                TemplateOverlaySetId::empty(),
            )],
        });
        store.templates[unsafe_wrapper_fixture.template_id.index()].conditional_child_wrapper_set =
            Some(outer_wrapper_set_id);
    }
    let unsafe_wrapper_view = TirView::new(
        &unsafe_wrapper_fixture.registry,
        TemplateRef::new(
            unsafe_wrapper_fixture.store_id,
            unsafe_wrapper_fixture.template_id,
        ),
        TemplateTirPhase::Composed,
        unsafe_wrapper_fixture.overlay_set_id,
    )
    .expect("unsafe wrapper view should construct");
    let unsafe_wrapper_store = unsafe_wrapper_store_handle.borrow();
    assert!(
        !tir_view_is_read_only_fold_safe(&unsafe_wrapper_view, &unsafe_wrapper_store)
            .expect("fold safety authority should resolve"),
        "same-store wrappers with slot-local wrapper context must stay on fallback"
    );

    let runtime_fixture = build_text_template_registry(&mut string_table, "runtime");
    let runtime_store_handle = runtime_fixture
        .registry
        .store_handle(runtime_fixture.store_id)
        .expect("runtime store handle should exist");
    {
        let mut store = runtime_store_handle.borrow_mut();
        let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
            location: empty_location(),
            contribution_sources: vec![],
            slot_sites: vec![],
        });
        store.templates[runtime_fixture.template_id.index()].runtime_slot_plan = Some(slot_plan_id);
    }
    let runtime_view = TirView::new(
        &runtime_fixture.registry,
        TemplateRef::new(runtime_fixture.store_id, runtime_fixture.template_id),
        TemplateTirPhase::Composed,
        runtime_fixture.overlay_set_id,
    )
    .expect("runtime view should construct");
    let runtime_store = runtime_store_handle.borrow();
    assert!(
        !tir_view_is_read_only_fold_safe(&runtime_view, &runtime_store)
            .expect("fold safety authority should resolve"),
        "runtime slot plans are HIR/runtime handoff data, not const-fold output"
    );

    let mut aggregate_registry = TemplateIrRegistry::new();
    let aggregate_store_id = aggregate_registry.allocate_store();
    let aggregate_overlay_set_id =
        aggregate_registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let aggregate_template_id = {
        let mut store = aggregate_registry
            .store_mut(aggregate_store_id)
            .expect("aggregate store should be mutable");
        let aggregate_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::AggregateOutput,
            empty_location(),
        ));
        store.push_template(TemplateIr::new(
            aggregate_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ))
    };
    let aggregate_view = TirView::new(
        &aggregate_registry,
        TemplateRef::new(aggregate_store_id, aggregate_template_id),
        TemplateTirPhase::Composed,
        aggregate_overlay_set_id,
    )
    .expect("aggregate marker view should construct");
    let aggregate_store_handle = aggregate_registry
        .store_handle(aggregate_store_id)
        .expect("aggregate store handle should exist");
    let aggregate_store = aggregate_store_handle.borrow();
    assert!(
        !tir_view_is_read_only_fold_safe(&aggregate_view, &aggregate_store)
            .expect("fold safety authority should resolve"),
        "aggregate markers outside aggregate wrappers must not take the read-only fold path"
    );
    assert!(
        !tir_view_is_expression_overlay_linear_fold_safe(&aggregate_view, &aggregate_store)
            .expect("fold safety authority should resolve"),
        "aggregate markers outside aggregate wrappers must not take the view-native fold path"
    );
}

#[test]
fn read_only_fold_safety_rejects_child_template_cycles() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let template_a_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let template_a_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let placeholder_root = builder.push_sequence_node(vec![], empty_location());
            builder.finish_template(
                placeholder_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            )
        };

        let template_b_id = {
            let child_a = TemplateTirChildReference::same_store(
                template_a_id,
                store_id,
                TemplateTirPhase::Composed,
                overlay_set_id,
            );
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child_a_node =
                builder.push_child_template_node_with_reference(child_a, empty_location());
            builder.finish_template(
                child_a_node,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            )
        };

        let child_b = TemplateTirChildReference::same_store(
            template_b_id,
            store_id,
            TemplateTirPhase::Composed,
            overlay_set_id,
        );
        let mut builder = TemplateIrBuilder::new(&mut store);
        let child_b_node =
            builder.push_child_template_node_with_reference(child_b, empty_location());
        store.templates[template_a_id.index()].root = child_b_node;

        template_a_id
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, template_a_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("cyclic view should construct");
    let store_handle = registry
        .store_handle(store_id)
        .expect("store handle should exist");
    let store = store_handle.borrow();

    assert!(
        !tir_view_is_read_only_fold_safe(&view, &store)
            .expect("fold safety authority should resolve"),
        "read-only folding must reject child-template cycles because the fold walker has no cycle guard"
    );
}

#[test]
fn fold_safety_reports_malformed_authority_as_error() {
    let mut string_table = StringTable::new();

    let missing_node_fixture = build_text_template_registry(&mut string_table, "missing node");
    let missing_node_view = TirView::new(
        &missing_node_fixture.registry,
        TemplateRef::new(
            missing_node_fixture.store_id,
            missing_node_fixture.template_id,
        ),
        TemplateTirPhase::Composed,
        missing_node_fixture.overlay_set_id,
    )
    .expect("view should construct before the store is malformed");
    let missing_node_store_handle = missing_node_fixture
        .registry
        .store_handle(missing_node_fixture.store_id)
        .expect("store handle should exist");
    missing_node_store_handle.borrow_mut().nodes.clear();
    let missing_node_store = missing_node_store_handle.borrow();
    let missing_node_error =
        classify_view_native_fold_safety(&missing_node_view, &missing_node_store)
            .expect_err("missing root node must be an authority error");
    assert!(
        format!("{missing_node_error:?}").contains("node"),
        "missing-node error should identify the malformed node authority"
    );
    drop(missing_node_store);

    let missing_template_fixture =
        build_text_template_registry(&mut string_table, "missing template");
    let missing_template_view = TirView::new(
        &missing_template_fixture.registry,
        TemplateRef::new(
            missing_template_fixture.store_id,
            missing_template_fixture.template_id,
        ),
        TemplateTirPhase::Composed,
        missing_template_fixture.overlay_set_id,
    )
    .expect("view should construct before the store is malformed");
    let missing_template_store_handle = missing_template_fixture
        .registry
        .store_handle(missing_template_fixture.store_id)
        .expect("store handle should exist");
    missing_template_store_handle.borrow_mut().templates.clear();
    let missing_template_store = missing_template_store_handle.borrow();
    let missing_template_error =
        tir_view_is_read_only_fold_safe(&missing_template_view, &missing_template_store)
            .expect_err("missing root template must be an authority error");
    assert!(
        format!("{missing_template_error:?}").contains("template"),
        "missing-template error should identify the malformed template authority"
    );
    drop(missing_template_store);

    let mut missing_overlay_fixture =
        build_text_template_registry(&mut string_table, "missing overlay dimension");
    let missing_dimension_overlay_set_id =
        missing_overlay_fixture
            .registry
            .allocate_overlay_set(TemplateOverlaySet {
                expression_overrides: Some(TirExpressionOverlayId::new(999)),
                slot_resolution: None,
                wrapper_context: None,
            });
    let missing_dimension_view = TirView::new(
        &missing_overlay_fixture.registry,
        TemplateRef::new(
            missing_overlay_fixture.store_id,
            missing_overlay_fixture.template_id,
        ),
        TemplateTirPhase::Finalized,
        missing_dimension_overlay_set_id,
    )
    .expect("view should construct with an existing malformed overlay set");
    let missing_dimension_store = missing_overlay_fixture
        .registry
        .store_handle(missing_overlay_fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let missing_dimension_error =
        classify_view_native_fold_safety(&missing_dimension_view, &missing_dimension_store)
            .expect_err("missing overlay dimension must be an authority error");
    assert!(
        format!("{missing_dimension_error:?}").contains("expression overlay"),
        "overlay-dimension error should identify the missing expression entry"
    );

    let wrapper_fixture = build_text_template_registry(&mut string_table, "missing wrapper set");
    let wrapper_store_handle = wrapper_fixture
        .registry
        .store_handle(wrapper_fixture.store_id)
        .expect("store handle should exist");
    let wrapper_view = TirView::new(
        &wrapper_fixture.registry,
        TemplateRef::new(wrapper_fixture.store_id, wrapper_fixture.template_id),
        TemplateTirPhase::Composed,
        wrapper_fixture.overlay_set_id,
    )
    .expect("view should construct before the store is malformed");
    wrapper_store_handle.borrow_mut().templates[wrapper_fixture.template_id.index()]
        .conditional_child_wrapper_set = Some(TemplateWrapperSetId::new(999));
    let wrapper_store = wrapper_store_handle.borrow();
    let missing_wrapper_error = tir_view_is_read_only_fold_safe(&wrapper_view, &wrapper_store)
        .expect_err("missing wrapper set must be an authority error");
    assert!(
        format!("{missing_wrapper_error:?}").contains("wrapper set"),
        "wrapper-set error should identify the malformed wrapper authority"
    );
}

#[test]
fn fold_tir_view_rejects_missing_node_in_untaken_branch() {
    use crate::compiler_frontend::ast::expressions::expression::Expression;
    use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
    use crate::compiler_frontend::value_mode::ValueMode;

    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let fallback_text = string_table.intern("fallback");
        let mut builder = TemplateIrBuilder::new(&mut store);
        let fallback_node = builder.push_text_node(
            fallback_text,
            "fallback".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        let branch = crate::compiler_frontend::ast::templates::tir::node::TemplateIrBranch::new(
            TemplateBranchSelector::Bool(Expression::bool(
                false,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            crate::compiler_frontend::ast::templates::tir::ids::TemplateIrNodeId::new(999),
            empty_location(),
        );
        let root =
            builder.push_branch_chain_node(vec![branch], Some(fallback_node), empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("view should construct");
    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let error = fold_tir_view(&view, &store, &mut fold_context)
        .expect_err("untaken branch authority must be validated before selection");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing untaken-branch node should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR fold safety: node"),
        "expected a stable untaken-branch authority error, got: {}",
        error.msg
    );
}

#[test]
fn read_only_fold_safety_reports_missing_child_before_overlay_fallback() {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let parent_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let child_slot_overlay_id =
        registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
            resolutions: vec![],
        });
    let child_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(child_slot_overlay_id),
        wrapper_context: None,
    });
    let missing_child_template_id = TemplateIrId::new(999);

    let parent_template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");
        let child_reference = TemplateTirChildReference::same_store(
            missing_child_template_id,
            store_id,
            TemplateTirPhase::Composed,
            child_overlay_set_id,
        );
        finish_single_child_template(&mut store, child_reference)
    };

    let view = TirView::new(
        &registry,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Composed,
        parent_overlay_set_id,
    )
    .expect("parent view should construct");
    let store_handle = registry
        .store_handle(store_id)
        .expect("store handle should exist");
    let store = store_handle.borrow();

    let error = tir_view_is_read_only_fold_safe(&view, &store)
        .expect_err("a non-empty child overlay must not hide missing child authority");
    assert!(
        error.msg.contains("template"),
        "missing-child error should identify the missing template authority"
    );
}

// -------------------------
//  Slot-resolution overlay folding
// -------------------------

/// Controls whether the fill template shares the wrapper's store or lives in
/// a separate child store, exercising same-store and cross-store
/// slot-resolution paths respectively.
enum SlotFixtureStoreLayout {
    SharedStore,
    SeparateStores,
}

/// Holds a registry with a wrapper template ("before" + $slot(default) +
/// "after") and a fill template whose root is a single "filled" text node.
/// The store layout depends on the `SlotFixtureStoreLayout` passed to the
/// builder.
struct SlotResolutionFixture {
    registry: TemplateIrRegistry,
    wrapper_store_id: TemplateStoreId,
    fill_store_id: TemplateStoreId,
    wrapper_template_id: TemplateIrId,
    fill_template_id: TemplateIrId,
    slot_occurrence_id: SlotOccurrenceId,
}

fn build_slot_fill_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrId {
    let fill_text_id = string_table.intern("filled");
    let mut builder = TemplateIrBuilder::new(store);
    let fill_text_node = builder.push_text_node(
        fill_text_id,
        "filled".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let fill_root = builder.push_sequence_node(vec![fill_text_node], empty_location());

    builder.finish_template(
        fill_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

fn build_slot_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> (TemplateIrId, SlotOccurrenceId) {
    let before_id = string_table.intern("before");
    let after_id = string_table.intern("after");
    let mut builder = TemplateIrBuilder::new(store);
    let before_node = builder.push_text_node(
        before_id,
        "before".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let slot_node_id = builder.push_slot_node(SlotKey::Default, empty_location());
    let after_node = builder.push_text_node(
        after_id,
        "after".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let wrapper_root = builder.push_sequence_node(
        vec![before_node, slot_node_id, after_node],
        empty_location(),
    );
    let wrapper_template_id = builder.finish_template(
        wrapper_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    );

    let slot_occurrence_id = match &store
        .get_node(slot_node_id)
        .expect("slot node should exist")
        .kind
    {
        TemplateIrNodeKind::Slot { placeholder } => placeholder.occurrence_id,
        _ => panic!("node should be a slot"),
    };

    (wrapper_template_id, slot_occurrence_id)
}

/// Builds a slot-resolution fixture with the given store layout.
///
/// `SharedStore` places both wrapper and fill in one store (same-store path).
/// `SeparateStores` places the wrapper in a parent store and the fill in a
/// child store (cross-store path).
fn build_slot_resolution_fixture(
    string_table: &mut StringTable,
    layout: SlotFixtureStoreLayout,
) -> SlotResolutionFixture {
    let mut registry = TemplateIrRegistry::new();
    let wrapper_store_id = registry.allocate_store();
    let fill_store_id = match layout {
        SlotFixtureStoreLayout::SharedStore => wrapper_store_id,
        SlotFixtureStoreLayout::SeparateStores => registry.allocate_store(),
    };

    let fill_template_id = {
        let mut fill_store = registry
            .store_mut(fill_store_id)
            .expect("fill store should be mutable");

        build_slot_fill_template(&mut fill_store, string_table)
    };

    let (wrapper_template_id, slot_occurrence_id) = {
        let mut wrapper_store = registry
            .store_mut(wrapper_store_id)
            .expect("wrapper store should be mutable");

        build_slot_wrapper_template(&mut wrapper_store, string_table)
    };

    SlotResolutionFixture {
        registry,
        wrapper_store_id,
        fill_store_id,
        wrapper_template_id,
        fill_template_id,
        slot_occurrence_id,
    }
}

/// Same-store entry helper for call-site readability.
fn build_same_store_slot_fixture(string_table: &mut StringTable) -> SlotResolutionFixture {
    build_slot_resolution_fixture(string_table, SlotFixtureStoreLayout::SharedStore)
}

/// Cross-store entry helper for call-site readability.
fn build_cross_store_slot_fixture(string_table: &mut StringTable) -> SlotResolutionFixture {
    build_slot_resolution_fixture(string_table, SlotFixtureStoreLayout::SeparateStores)
}

/// Builds an overlay set that resolves the wrapper's default slot to the fill
/// template.
fn build_resolved_slot_overlay_set(fixture: &mut SlotResolutionFixture) -> TemplateOverlaySetId {
    let fill_ref = TemplateRef::new(fixture.fill_store_id, fixture.fill_template_id);
    let slot_overlay_id =
        fixture
            .registry
            .allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
                resolutions: vec![(
                    fixture.slot_occurrence_id,
                    TirSlotResolution::resolved(SlotKey::Default, vec![fill_ref]),
                )],
            });
    fixture.registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    })
}

/// Folding a wrapper template with a resolved slot-resolution overlay must
/// produce the same output as structural expansion: the fill content replaces
/// the slot placeholder.
#[test]
fn fold_tir_view_with_resolved_slot_overlay_produces_filled_output() {
    let mut string_table = StringTable::new();
    let mut fixture = build_same_store_slot_fixture(&mut string_table);
    let overlay_set_id = build_resolved_slot_overlay_set(&mut fixture);

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::with_minimum_phase(
        &fixture.registry,
        TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("wrapper view should resolve");

    let store = fixture
        .registry
        .store_handle(fixture.wrapper_store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let result = fold_tir_view(&view, &store, &mut fold_context)
        .expect("fold with resolved slot overlay should succeed");

    let expected = string_table.intern("beforefilledafter");
    assert_eq!(
        result,
        TemplateEmission::Output(expected),
        "resolved slot overlay must produce before+filled+after"
    );
}

/// Folding a wrapper template with a missing slot-resolution overlay must
/// produce the same output as an unfilled slot: empty output at the slot site.
#[test]
fn fold_tir_view_with_missing_slot_overlay_produces_empty_slot_output() {
    let mut string_table = StringTable::new();
    let mut fixture = build_same_store_slot_fixture(&mut string_table);

    // Build a missing-resolution overlay for the slot occurrence.
    let slot_overlay_id =
        fixture
            .registry
            .allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
                resolutions: vec![(
                    fixture.slot_occurrence_id,
                    TirSlotResolution::missing(SlotKey::Default),
                )],
            });
    let overlay_set_id = fixture.registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::with_minimum_phase(
        &fixture.registry,
        TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("wrapper view should resolve");

    let store = fixture
        .registry
        .store_handle(fixture.wrapper_store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let result = fold_tir_view(&view, &store, &mut fold_context)
        .expect("fold with missing slot overlay should succeed");

    let expected = string_table.intern("beforeafter");
    assert_eq!(
        result,
        TemplateEmission::Output(expected),
        "missing slot overlay must produce before+after (empty slot)"
    );
}

/// Fold cache must remain context-local: a slot-overlay fold with empty
/// bindings is cached, but a fold with active bindings is not.
#[test]
fn fold_tir_view_slot_overlay_caches_empty_bindings_not_active_bindings() {
    let mut string_table = StringTable::new();
    let mut fixture = build_same_store_slot_fixture(&mut string_table);
    let overlay_set_id = build_resolved_slot_overlay_set(&mut fixture);

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::with_minimum_phase(
        &fixture.registry,
        TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("wrapper view should resolve");

    let store = fixture
        .registry
        .store_handle(fixture.wrapper_store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    // First fold with empty bindings — should be cached.
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let first = fold_tir_view(&view, &store, &mut fold_context).expect("first fold should succeed");

    let cache_key = TirFoldCacheKey {
        root: TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&cache_key).is_some(),
        "empty-binding slot-overlay fold should be cached"
    );

    // Second fold with active bindings — must not be cached.
    let unused_binding_value = string_table.intern("unused");
    let mut binding_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    binding_context.bindings.push(TemplateFoldBinding {
        path: InternedPath::new(),
        value: crate::compiler_frontend::ast::expressions::expression::Expression::string_slice(
            unused_binding_value,
            empty_location(),
            crate::compiler_frontend::value_mode::ValueMode::ImmutableOwned,
        ),
    });

    let binding_result = fold_tir_view(&view, &store, &mut binding_context)
        .expect("fold with bindings should succeed");
    assert_eq!(binding_result, first);

    let binding_cache_key = TirFoldCacheKey {
        root: TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
        phase: TemplateTirPhase::Composed,
        overlay_set_id,
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: false,
    };
    assert!(
        binding_context.fold_cache.get(&binding_cache_key).is_none(),
        "non-empty-binding slot-overlay fold should not be cached"
    );
}

// -------------------------
//  Cross-store slot-resolution overlay folding
// -------------------------

/// Folding a wrapper template with a cross-store resolved slot-resolution
/// overlay must produce filled output by routing through the registry.
#[test]
fn fold_tir_view_with_cross_store_slot_overlay_produces_filled_output() {
    let mut string_table = StringTable::new();
    let mut fixture = build_cross_store_slot_fixture(&mut string_table);
    let overlay_set_id = build_resolved_slot_overlay_set(&mut fixture);

    // The fill ref is needed for the cache-key assertion below.
    let fill_ref = TemplateRef::new(fixture.fill_store_id, fixture.fill_template_id);

    let registry = Rc::new(RefCell::new(fixture.registry));
    let parent_store = registry
        .borrow()
        .store_handle(fixture.wrapper_store_id)
        .expect("parent store should exist")
        .borrow()
        .clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let expected = string_table.intern("beforefilledafter");

    let registry_borrow = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("wrapper view should resolve");

    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let result = fold_tir_view(&view, &parent_store, &mut fold_context)
        .expect("cross-store slot overlay fold should succeed");

    assert_eq!(
        result,
        TemplateEmission::Output(expected),
        "cross-store resolved slot overlay must produce before+filled+after"
    );

    let fill_cache_key = TirFoldCacheKey {
        root: fill_ref,
        phase: TemplateTirPhase::Composed,
        overlay_set_id: TemplateOverlaySetId::empty(),
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&fill_cache_key).is_some(),
        "cross-store slot-resolution fold should cache the fill template with qualified identity"
    );
}

/// Folding a wrapper template with a cross-store resolved slot-resolution
/// overlay but no registry must fail with a precise missing-registry error.
#[test]
fn fold_tir_view_rejects_cross_store_slot_overlay_without_registry() {
    let mut string_table = StringTable::new();
    let mut fixture = build_cross_store_slot_fixture(&mut string_table);
    let overlay_set_id = build_resolved_slot_overlay_set(&mut fixture);

    // The fill ref appears in the missing-registry error message.
    let fill_ref = TemplateRef::new(fixture.fill_store_id, fixture.fill_template_id);

    let parent_store = fixture
        .registry
        .store_handle(fixture.wrapper_store_id)
        .expect("parent store should exist")
        .borrow()
        .clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let error = fold_tir_view(
        &TirView::with_minimum_phase(
            &fixture.registry,
            TemplateRef::new(fixture.wrapper_store_id, fixture.wrapper_template_id),
            TemplateTirPhase::Composed,
            TemplateTirPhase::Composed,
            overlay_set_id,
        )
        .expect("wrapper view should resolve"),
        &parent_store,
        &mut fold_context,
    )
    .expect_err("cross-store slot overlay without registry should fail");

    let TemplateError::Infrastructure(error) = error else {
        panic!("missing registry should be an internal TIR invariant");
    };

    assert_eq!(
        error.msg,
        format!(
            "TIR fold: cross-store child template {} requires the module-local registry, but none is available.",
            fill_ref
        )
    );
}

/// Folding a wrapper template with a cross-store resolved slot-resolution
/// overlay but an unregistered child store must fail with a precise error.
#[test]
fn fold_tir_view_rejects_cross_store_slot_overlay_with_unregistered_store() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let missing_store_id = TemplateStoreId::new(1);

    let (wrapper_template_id, slot_occurrence_id) = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should be mutable");

        build_slot_wrapper_template(&mut parent_store, &mut string_table)
    };

    let missing_fill_ref = TemplateRef::new(missing_store_id, TemplateIrId::new(0));
    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            slot_occurrence_id,
            TirSlotResolution::resolved(SlotKey::Default, vec![missing_fill_ref]),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });

    let registry = Rc::new(RefCell::new(registry));
    let parent_store = registry
        .borrow()
        .store_handle(parent_store_id)
        .expect("parent store should exist")
        .borrow()
        .clone();

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let registry_borrow = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        TemplateRef::new(parent_store_id, wrapper_template_id),
        TemplateTirPhase::Composed,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("wrapper view should resolve");

    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let error = fold_tir_view(&view, &parent_store, &mut fold_context)
        .expect_err("cross-store slot overlay with unregistered store should fail");

    let TemplateError::Infrastructure(error) = error else {
        panic!("unregistered store should be an internal TIR invariant");
    };

    assert_eq!(
        error.msg,
        format!(
            "TIR fold: cross-store child template store {} is not registered.",
            missing_store_id
        )
    );
}

/// Phase 1 TIR attribution counters must increment through the real
/// `fold_tir_view` path so benchmark runs can attribute view-fold work.
///
/// WHAT: folds an empty-overlay view twice with empty bindings and asserts the
///       view-fold, cache-miss, cache-hit, and empty-overlay counters moved.
/// WHY: proves the production call-site wiring without asserting fold output
///      or broader implementation details.
#[cfg(feature = "benchmark_counters")]
#[test]
fn fold_tir_view_increments_phase1_attribution_counters() {
    let _guard = crate::compiler_frontend::instrumentation::lock_counter_test();

    let mut string_table = StringTable::new();
    let fixture = build_text_template_registry(&mut string_table, "counter probe");
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::new(
        &fixture.registry,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        TemplateTirPhase::Composed,
        fixture.overlay_set_id,
    )
    .expect("view should construct");

    let store = fixture
        .registry
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    reset_ast_counters();

    let first = fold_tir_view(&view, &store, &mut fold_context).expect("first fold should succeed");
    // Second fold with empty bindings must hit the fold cache.
    let second =
        fold_tir_view(&view, &store, &mut fold_context).expect("second fold should succeed");
    assert_eq!(first, second, "cached fold must equal the first fold");

    assert_eq!(
        test_read_ast_counter(AstCounter::TirViewFoldsAttempted),
        2,
        "fold_tir_view should be attempted twice"
    );
    assert_eq!(
        test_read_ast_counter(AstCounter::TirFoldCacheMisses),
        1,
        "first empty-binding fold should miss the cache once"
    );
    assert_eq!(
        test_read_ast_counter(AstCounter::TirFoldCacheHits),
        1,
        "second empty-binding fold should hit the cache once"
    );
    // The second fold hits the cache and returns before the overlay-shape
    // attribution runs, so the empty-overlay counter records only the one
    // real fold (the cache miss).
    assert_eq!(
        test_read_ast_counter(AstCounter::TirViewFoldOverlayEmpty),
        1,
        "only the cache-miss fold attributes its overlay shape"
    );
}

/// Folding a template with an expression overlay through `fold_tir_view` must
/// use the view-effective expression at each dynamic-expression site, not the
/// structural expression stored on the node.
///
/// WHAT: builds a template with text + a dynamic expression + text, then
///       creates an expression overlay that replaces the dynamic expression
///       with a different const value. The view-native fold walker reads the
///       overlay expression during folding instead of cloning and mutating
///       the store.
#[test]
fn fold_tir_view_with_expression_overlay_uses_effective_expression() {
    use crate::compiler_frontend::ast::expressions::expression::Expression;
    use crate::compiler_frontend::ast::templates::tir::overlays::TirExpressionOverlay;
    use crate::compiler_frontend::value_mode::ValueMode;

    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let template_id = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        // Build: "before" + $(42) + "after"
        let before_id = string_table.intern("before");
        let after_id = string_table.intern("after");

        let mut builder = TemplateIrBuilder::new(&mut store);
        let before_node = builder.push_text_node(
            before_id,
            "before".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );

        // The structural expression is int(42); the overlay will replace it
        // with a string slice so we can distinguish the output.
        let structural_expression =
            Expression::int(42, empty_location(), ValueMode::ImmutableOwned);
        let dynamic_node = builder.push_dynamic_expression_node(
            structural_expression,
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );

        let after_node = builder.push_text_node(
            after_id,
            "after".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );

        let root = builder.push_sequence_node(
            vec![before_node, dynamic_node, after_node],
            empty_location(),
        );
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    // Read the expression site ID from the dynamic expression node. The store
    // is cloned locally so the RefCell borrow does not need to outlive the
    // scan — the site ID is a Copy value extracted from the node kind.
    let site_id = {
        let store = registry
            .store_handle(store_id)
            .expect("store handle should exist")
            .borrow()
            .clone();
        let template = store
            .get_template(template_id)
            .expect("template should exist");
        let root_node = store
            .get_node(template.root)
            .expect("root node should exist");
        let crate::compiler_frontend::ast::templates::tir::node::TemplateIrNodeKind::Sequence {
            children,
        } = &root_node.kind
        else {
            panic!("template root should be a Sequence");
        };
        children
            .iter()
            .find_map(|child_id| {
                store.get_node(*child_id).and_then(|child| match &child.kind {
                    crate::compiler_frontend::ast::templates::tir::node::TemplateIrNodeKind::DynamicExpression { site_id, .. } => Some(*site_id),
                    _ => None,
                })
            })
            .expect("at least one DynamicExpression node should exist in the root")
    };

    // Build the expression overlay: replace the site with a string "X".
    let overlay_string_id = string_table.intern("X");
    let overlay_expression = Expression::string_slice(
        overlay_string_id,
        empty_location(),
        ValueMode::ImmutableOwned,
    );
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(overlay_expression))],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("expression overlay view should resolve");

    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let result = fold_tir_view(&view, &store, &mut fold_context)
        .expect("fold with expression overlay should succeed");

    // The overlay expression ("X") should appear, not the structural int (42).
    let expected = string_table.intern("beforeXafter");
    assert_eq!(
        result,
        TemplateEmission::Output(expected),
        "expression overlay must replace the structural expression during view-native folding"
    );
}

/// A root expression overlay remains active through multiple same-store child
/// entries, and a nested child result is not cached without that outer context.
#[test]
fn fold_tir_view_preserves_root_expression_overlay_through_nested_children() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (root_template_id, leaf_template_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");
        let structural_text = string_table.intern("structural-leaf");

        let leaf_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let leaf_expression = builder.push_dynamic_expression_node(
                Expression::string_slice(
                    structural_text,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
                None,
                empty_location(),
            );
            let leaf_root = builder.push_sequence_node(vec![leaf_expression], empty_location());
            builder.finish_template(
                leaf_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                empty_location(),
            )
        };

        let middle_template_id = finish_single_child_template(
            &mut store,
            TemplateTirChildReference::same_store(
                leaf_template_id,
                store_id,
                TemplateTirPhase::Composed,
                empty_overlay_set_id,
            ),
        );
        let root_template_id = finish_single_child_template(
            &mut store,
            TemplateTirChildReference::same_store(
                middle_template_id,
                store_id,
                TemplateTirPhase::Composed,
                empty_overlay_set_id,
            ),
        );

        (root_template_id, leaf_template_id)
    };

    // The builder assigns the expression site while creating the dynamic node;
    // recover the authoritative site from the leaf node before allocating the
    // root overlays.
    let leaf_site_id = {
        let store_handle = registry
            .store_handle(store_id)
            .expect("store handle should exist");
        let store = store_handle.borrow();
        let leaf_root = store
            .get_template(leaf_template_id)
            .expect("leaf template should exist")
            .root;
        let leaf_node = store.get_node(leaf_root).expect("leaf root should exist");
        let TemplateIrNodeKind::Sequence { children } = &leaf_node.kind else {
            panic!("leaf root should be a sequence");
        };
        let expression_node = store
            .get_node(children[0])
            .expect("leaf expression node should exist");
        let TemplateIrNodeKind::DynamicExpression { site_id, .. } = expression_node.kind else {
            panic!("leaf child should be a dynamic expression");
        };
        site_id
    };

    let first_text = string_table.intern("first-root-overlay");
    let second_text = string_table.intern("second-root-overlay");
    let first_expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            leaf_site_id,
            Box::new(Expression::string_slice(
                first_text,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let second_expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            leaf_site_id,
            Box::new(Expression::string_slice(
                second_text,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let first_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(first_expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let second_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(second_expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let registry = Rc::new(RefCell::new(registry));
    let store = registry
        .borrow()
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_registry: Some(Rc::clone(&registry)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    let first = {
        let registry_borrow = registry.borrow();
        let view = TirView::new(
            &registry_borrow,
            TemplateRef::new(store_id, root_template_id),
            TemplateTirPhase::Composed,
            first_overlay_set_id,
        )
        .expect("first root view should construct");
        fold_tir_view(&view, &store, &mut fold_context).expect("first root fold should succeed")
    };
    let second = {
        let registry_borrow = registry.borrow();
        let view = TirView::new(
            &registry_borrow,
            TemplateRef::new(store_id, root_template_id),
            TemplateTirPhase::Composed,
            second_overlay_set_id,
        )
        .expect("second root view should construct");
        fold_tir_view(&view, &store, &mut fold_context).expect("second root fold should succeed")
    };

    assert_eq!(first, TemplateEmission::Output(first_text));
    assert_eq!(second, TemplateEmission::Output(second_text));
    assert!(
        fold_context
            .fold_cache
            .get(&TirFoldCacheKey {
                root: TemplateRef::new(store_id, leaf_template_id),
                phase: TemplateTirPhase::Composed,
                overlay_set_id: empty_overlay_set_id,
                loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
                bindings_empty: true,
            })
            .is_none(),
        "nested child cache must be suppressed while an outer expression overlay is active"
    );
}

/// Branch selector overlays must drive branch selection during view-native
/// folding.
///
/// WHAT: builds a branch whose structural selector is `false`, then overlays
/// the selector site with `true`.
/// WHY: Phase 4 must cover branch selectors directly instead of only dynamic
/// expression nodes.
#[test]
fn fold_tir_view_with_branch_selector_overlay_selects_effective_branch() {
    use crate::compiler_frontend::ast::expressions::expression::Expression;
    use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
    use crate::compiler_frontend::ast::templates::tir::node::{
        TemplateIrBranch, TemplateIrNodeKind,
    };
    use crate::compiler_frontend::ast::templates::tir::overlays::TirExpressionOverlay;
    use crate::compiler_frontend::value_mode::ValueMode;

    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (template_id, selector_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let selected_id = string_table.intern("selected");
        let fallback_id = string_table.intern("fallback");

        let (template_id, root) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let selected_node = builder.push_text_node(
                selected_id,
                "selected".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let fallback_node = builder.push_text_node(
                fallback_id,
                "fallback".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let branch = TemplateIrBranch::new(
                TemplateBranchSelector::Bool(Expression::bool(
                    false,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                )),
                selected_node,
                empty_location(),
            );
            let root =
                builder.push_branch_chain_node(vec![branch], Some(fallback_node), empty_location());
            let template_id = builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::empty(),
                empty_location(),
            );

            (template_id, root)
        };

        let selector_site_id = match &store.get_node(root).expect("branch node should exist").kind {
            TemplateIrNodeKind::BranchChain { branches, .. } => branches[0].selector_site_id,
            other => panic!("expected BranchChain node, got {other:?}"),
        };

        (template_id, selector_site_id)
    };

    let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            selector_site_id,
            Box::new(Expression::bool(
                true,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("branch overlay view should resolve");
    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let result =
        fold_tir_view(&view, &store, &mut fold_context).expect("branch overlay fold succeeds");

    let expected = string_table.intern("selected");
    assert_eq!(
        result,
        TemplateEmission::Output(expected),
        "branch selector overlay must select the effective branch"
    );
}

/// Loop-header overlays must drive range-loop execution during view-native
/// folding.
///
/// WHAT: builds a `0..0` loop that would emit nothing, then overlays the end
/// bound with `2`.
/// WHY: Phase 4 must cover loop headers directly because they use expression
/// sites outside dynamic-expression nodes.
#[test]
fn fold_tir_view_with_range_loop_header_overlay_uses_effective_bound() {
    use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeEndKind, RangeLoopSpec};
    use crate::compiler_frontend::ast::expressions::expression::Expression;
    use crate::compiler_frontend::ast::templates::template_control_flow::TemplateLoopHeader;
    use crate::compiler_frontend::ast::templates::tir::node::{
        TemplateIrNodeKind, TemplateLoopHeaderExpressionSites,
    };
    use crate::compiler_frontend::ast::templates::tir::overlays::TirExpressionOverlay;
    use crate::compiler_frontend::value_mode::ValueMode;

    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (template_id, end_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("store should be mutable");

        let tick_id = string_table.intern("x");
        let (template_id, root) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let body_node = builder.push_text_node(
                tick_id,
                "x".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let root = builder.push_loop_node(
                TemplateLoopHeader::Range {
                    bindings: Box::new(LoopBindings {
                        item: None,
                        index: None,
                    }),
                    range: Box::new(RangeLoopSpec {
                        start: Expression::int(0, empty_location(), ValueMode::ImmutableOwned),
                        end: Expression::int(0, empty_location(), ValueMode::ImmutableOwned),
                        end_kind: RangeEndKind::Exclusive,
                        step: None,
                    }),
                },
                body_node,
                None,
                empty_location(),
            );
            let template_id = builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::empty(),
                empty_location(),
            );

            (template_id, root)
        };

        let end_site_id = match &store.get_node(root).expect("loop node should exist").kind {
            TemplateIrNodeKind::Loop {
                header_sites: TemplateLoopHeaderExpressionSites::Range { end, .. },
                ..
            } => *end,
            other => panic!("expected Loop node with range header sites, got {other:?}"),
        };

        (template_id, end_site_id)
    };

    let overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            end_site_id,
            Box::new(Expression::int(
                2,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let view = TirView::with_minimum_phase(
        &registry,
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Composed,
        overlay_set_id,
    )
    .expect("range-loop overlay view should resolve");
    let store = registry
        .store_handle(store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let result =
        fold_tir_view(&view, &store, &mut fold_context).expect("range-loop overlay fold succeeds");

    let expected = string_table.intern("xx");
    assert_eq!(
        result,
        TemplateEmission::Output(expected),
        "range-loop header overlay must drive the effective iteration count"
    );
}
