//! TIR fold-cache and view-fold tests.
//
// WHAT: protects cache identity, same-store view folding, and overlay-aware
// folding at the owning TIR boundary.
// WHY: these tests cover module-local cache identity and semantic fold
// invariants.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, Template, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateFoldBinding,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold::fold_tir_view;
use crate::compiler_frontend::ast::templates::tir::fold_cache::{TirFoldCache, TirFoldCacheKey};
use crate::compiler_frontend::ast::templates::tir::ids::{
    SlotOccurrenceId, TemplateIrId, TemplateIrNodeId, TemplateSlotPlanId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateViewContext, TirExpressionOverlay, TirExpressionOverlayId, TirSlotResolution,
    TirSlotResolutionOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateTirReference,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::{
    TemplateTirPhase, TirView, TirViewIdentity,
};
use crate::compiler_frontend::ast::templates::tir::{
    PreparedTemplate, TemplatePreparationMode, prepare_tir_view,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;
use std::cell::RefCell;
use std::rc::Rc;

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn sample_key() -> TirFoldCacheKey {
    TirFoldCacheKey {
        identity: TirViewIdentity {
            root: TemplateIrId::new(0),
            phase: TemplateTirPhase::Parsed,
            context: TemplateViewContext::default(),
        },
        loop_iteration_limit: 1024,
        bindings_empty: true,
    }
}

#[test]
fn cache_key_equality_for_identical_fields() {
    assert_eq!(sample_key(), sample_key());
}

#[test]
fn cache_key_inequality_for_each_identity_dimension() {
    let mut root = sample_key();
    root.identity.root = TemplateIrId::new(1);
    assert_ne!(sample_key(), root);

    let mut phase = sample_key();
    phase.identity.phase = TemplateTirPhase::Formatted;
    assert_ne!(sample_key(), phase);

    let mut overlay = sample_key();
    overlay.identity.context = TemplateViewContext {
        expression_overlay: Some(TirExpressionOverlayId::new(7)),
        ..TemplateViewContext::default()
    };
    assert_ne!(sample_key(), overlay);

    let mut loop_limit = sample_key();
    loop_limit.loop_iteration_limit = 512;
    assert_ne!(sample_key(), loop_limit);

    let mut bindings = sample_key();
    bindings.bindings_empty = false;
    assert_ne!(sample_key(), bindings);
}

#[test]
fn cache_lookup_miss_then_hit_and_overwrite() {
    let mut string_table = StringTable::new();
    let first_id = string_table.intern("first");
    let second_id = string_table.intern("second");
    let mut cache = TirFoldCache::new();
    let key = sample_key();

    assert!(cache.get(&key).is_none());
    let first = TemplateEmission::Output(first_id);
    let second = TemplateEmission::Output(second_id);
    assert_eq!(cache.insert(key, first), None);
    assert_eq!(cache.insert(key, second), Some(first));
    assert_eq!(cache.get(&key), Some(&second));
}

struct TextFixture {
    store: TemplateIrStore,
    template_id: TemplateIrId,
    context: TemplateViewContext,
}

fn build_text_fixture(string_table: &mut StringTable, text: &str) -> TextFixture {
    let mut store = TemplateIrStore::new();
    let text_id = string_table.intern(text);
    let mut builder = TemplateIrBuilder::new(&mut store);
    let text_node = builder.push_text_node(
        text_id,
        text.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![text_node], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let context = TemplateViewContext::default();

    TextFixture {
        store,
        template_id,
        context,
    }
}

fn fold_context<'a>(string_table: &'a mut StringTable) -> TemplateFoldContext<'a> {
    let project_path_resolver = Box::leak(Box::new(
        crate::compiler_frontend::paths::path_resolution::ProjectPathResolver::new(
            std::env::temp_dir(),
            std::env::temp_dir(),
            crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(
            ),
            &crate::builder_surface::SourceFileKindRegistry::default(),
        )
        .expect("test path resolver should be valid"),
    ));
    TemplateFoldContext {
        string_table,
        project_path_resolver,
        path_format_config: Box::leak(Box::new(
            crate::compiler_frontend::paths::path_format::PathStringFormatConfig::default(),
        )),
        source_file_scope: Box::leak(Box::new(InternedPath::new())),
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_store: None,
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    }
}

#[test]
fn fold_view_matches_direct_template_fold_for_simple_text() {
    let mut string_table = StringTable::new();
    let fixture = build_text_fixture(&mut string_table, "hello");
    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");

    let mut context = fold_context(&mut string_table);
    let emission =
        fold_tir_view(&view, &fixture.store, &mut context).expect("view fold should succeed");

    assert_eq!(
        emission,
        TemplateEmission::Output(string_table.intern("hello"))
    );
}

#[test]
fn fold_view_caches_empty_binding_result() {
    let mut string_table = StringTable::new();
    let fixture = build_text_fixture(&mut string_table, "cached");
    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");

    let mut context = fold_context(&mut string_table);
    let first =
        fold_tir_view(&view, &fixture.store, &mut context).expect("first fold should succeed");
    let key = TirFoldCacheKey {
        identity: TirViewIdentity {
            root: fixture.template_id,
            phase: TemplateTirPhase::Composed,
            context: fixture.context,
        },
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };

    assert!(context.fold_cache.get(&key).is_some());
    let second =
        fold_tir_view(&view, &fixture.store, &mut context).expect("cached fold should succeed");
    assert_eq!(first, second);
}

#[test]
fn fold_view_does_not_cache_active_bindings() {
    let mut string_table = StringTable::new();
    let fixture = build_text_fixture(&mut string_table, "bound");
    let path = InternedPath::from_single_str("value", &mut string_table);
    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");

    let resolver = crate::compiler_frontend::paths::path_resolution::ProjectPathResolver::new(
        std::env::temp_dir(),
        std::env::temp_dir(),
        crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(),
        &crate::builder_surface::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid");
    let path_format =
        crate::compiler_frontend::paths::path_format::PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_store: None,
        bindings: vec![TemplateFoldBinding {
            path,
            value: Expression::int(1, empty_location(), ValueMode::ImmutableOwned),
        }],
        fold_cache: TirFoldCache::new(),
    };

    fold_tir_view(&view, &fixture.store, &mut context)
        .expect("active-binding fold should still succeed");
    let active_binding_key = TirFoldCacheKey {
        identity: TirViewIdentity {
            root: fixture.template_id,
            phase: TemplateTirPhase::Composed,
            context: fixture.context,
        },
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: false,
    };
    assert!(context.fold_cache.get(&active_binding_key).is_none());
}

#[test]
fn prepared_view_rejects_identity_mismatch() {
    let mut string_table = StringTable::new();
    let mut fixture = build_text_fixture(&mut string_table, "identity");
    let alternate_id = {
        let text_id = string_table.intern("alternate");
        let mut builder = TemplateIrBuilder::new(&mut fixture.store);
        let node =
            builder.push_text_node(text_id, 9, TemplateSegmentOrigin::Body, empty_location());
        let root = builder.push_sequence_node(vec![node], empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };
    let original_view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("original view should construct");
    let alternate_view = TirView::new(
        &fixture.store,
        alternate_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("alternate view should construct");
    let preparation = match prepare_tir_view(
        &original_view,
        &fixture.store,
        TemplatePreparationMode::Value,
    )
    .expect("preparation should succeed")
    {
        PreparedTemplate::Foldable(preparation) => preparation,
        PreparedTemplate::Runtime(_) | PreparedTemplate::Helper(_) => {
            panic!("text fixture should be foldable")
        }
    };

    let resolver = crate::compiler_frontend::paths::path_resolution::ProjectPathResolver::new(
        std::env::temp_dir(),
        std::env::temp_dir(),
        crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(),
        &crate::builder_surface::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid");
    let path_format =
        crate::compiler_frontend::paths::path_format::PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: &resolver,
        path_format_config: &path_format,
        source_file_scope: &source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_store: None,
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };
    let error = crate::compiler_frontend::ast::templates::tir::fold_tir_view_prepared(
        &alternate_view,
        &fixture.store,
        &mut context,
        preparation,
    )
    .expect_err("prepared identity mismatch should fail");
    assert!(format!("{error:?}").contains("root"));
}

#[test]
fn foldable_preparation_accepts_simple_text() {
    let mut string_table = StringTable::new();
    let fixture = build_text_fixture(&mut string_table, "safe");
    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");
    let preparation = prepare_tir_view(&view, &fixture.store, TemplatePreparationMode::Value)
        .expect("simple text should have a valid preparation");
    assert!(
        matches!(preparation, PreparedTemplate::Foldable(_)),
        "text-only view should produce a foldable result"
    );
}

#[test]
fn fold_view_with_resolved_slot_overlay_produces_filled_output() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let fill_text = string_table.intern("filled");
    let mut builder = TemplateIrBuilder::new(&mut store);
    let fill_node =
        builder.push_text_node(fill_text, 6, TemplateSegmentOrigin::Body, empty_location());
    let fill_root = builder.push_sequence_node(vec![fill_node], empty_location());
    let fill_template_id = builder.finish_template(
        fill_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    let wrapper_root = builder.push_sequence_node(vec![slot_node], empty_location());
    let wrapper_template_id = builder.finish_template(
        wrapper_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let slot_overlay_id = store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            SlotOccurrenceId::new(0),
            TirSlotResolution::resolved(SlotKey::Default, vec![fill_template_id]),
        )],
    });
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    };
    let view = TirView::new(
        &store,
        wrapper_template_id,
        TemplateTirPhase::Finalized,
        context,
    )
    .expect("view should construct");
    let mut context = fold_context(&mut string_table);
    let emission =
        fold_tir_view(&view, &store, &mut context).expect("slot overlay fold should succeed");

    assert_eq!(
        emission,
        TemplateEmission::Output(string_table.intern("filled"))
    );
}

#[test]
fn fold_view_with_missing_slot_overlay_produces_empty_output() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let mut builder = TemplateIrBuilder::new(&mut store);
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    let wrapper_template_id = builder.finish_template(
        slot_node,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    let slot_overlay_id =
        store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay::default());
    let context = TemplateViewContext {
        expression_overlay: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    };
    let view = TirView::new(
        &store,
        wrapper_template_id,
        TemplateTirPhase::Finalized,
        context,
    )
    .expect("view should construct");
    let mut context = fold_context(&mut string_table);
    let emission = fold_tir_view(&view, &store, &mut context)
        .expect("unresolved slot should fold to no output");
    assert_eq!(emission, TemplateEmission::NoOutput);
}

#[test]
fn same_store_child_cycle_is_rejected() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let template_id = TemplateIrId::new(store.template_count());
    let child_reference = TemplateTirChildReference::new(
        template_id,
        TemplateTirPhase::Composed,
        TemplateViewContext::default(),
    );
    let mut builder = TemplateIrBuilder::new(&mut store);
    let child_node =
        builder.push_child_template_node_with_reference(child_reference, empty_location());
    let root = builder.push_sequence_node(vec![child_node], empty_location());
    let actual_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    );
    assert_eq!(actual_id, template_id);

    let view = TirView::new(
        &store,
        template_id,
        TemplateTirPhase::Composed,
        TemplateViewContext::default(),
    )
    .expect("view should construct");
    let mut context = fold_context(&mut string_table);
    let result = fold_tir_view(&view, &store, &mut context);
    assert!(matches!(result, Err(TemplateError::Diagnostic(_))));
}

// -------------------------
//  Additional surviving fold-view invariants
// -------------------------

/// Builds a template whose root is a single child-template reference.
fn finish_single_child_template(
    store: &mut TemplateIrStore,
    child_reference: TemplateTirChildReference,
) -> TemplateIrId {
    let occurrence_id = store.next_child_template_occurrence_id();
    let child_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference: child_reference,
            occurrence_id,
        },
        empty_location(),
    ));
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![child_node],
        },
        empty_location(),
    ));
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ))
}

/// Builds a finalized text template and returns its id plus the text intern id.
fn text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let text_id = string_table.intern(text);
    let mut builder = TemplateIrBuilder::new(store);
    let node = builder.push_text_node(
        text_id,
        text.len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(vec![node], empty_location());
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    )
}

#[test]
fn fold_tir_view_rejects_parsed_phase_without_caching() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let template_id = text_template(&mut store, &mut string_table, "parsed");
    let context = TemplateViewContext::default();
    let view = TirView::new(&store, template_id, TemplateTirPhase::Parsed, context)
        .expect("view should construct");
    let mut context = fold_context(&mut string_table);

    let error = fold_tir_view(&view, &store, &mut context)
        .expect_err("a Parsed view must not fold or be cached");

    assert!(
        format!("{error:?}").contains("Composed"),
        "error must name the required Composed phase: {error:?}"
    );
}

#[test]
fn fold_tir_view_rejects_missing_node_in_untaken_branch() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let body = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: string_table.intern("taken"),
            byte_len: 5,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ));
    let branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(Expression::bool(
            false,
            empty_location(),
            ValueMode::ImmutableOwned,
        )),
        body,
        empty_location(),
    );
    let missing_body = TemplateIrNodeId::new(999);
    let untaken_branch = TemplateIrBranch::new(
        TemplateBranchSelector::Bool(Expression::bool(
            true,
            empty_location(),
            ValueMode::ImmutableOwned,
        )),
        missing_body,
        empty_location(),
    );
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::BranchChain {
            branches: vec![branch, untaken_branch],
            fallback: None,
        },
        empty_location(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::default(),
        empty_location(),
    ));
    let context = TemplateViewContext::default();
    let view = TirView::new(&store, template_id, TemplateTirPhase::Composed, context)
        .expect("view should construct");
    let mut context = fold_context(&mut string_table);

    let error = fold_tir_view(&view, &store, &mut context)
        .expect_err("a missing node in an untaken branch must still be rejected");

    assert!(
        format!("{error:?}").contains("node"),
        "error must report the missing node: {error:?}"
    );
}

#[test]
fn fold_tir_view_repeated_child_template_folding_hits_cache() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let child_text = string_table.intern("child");
    let child_template_id = {
        let mut builder = TemplateIrBuilder::new(&mut store);
        let node =
            builder.push_text_node(child_text, 5, TemplateSegmentOrigin::Body, empty_location());
        let root = builder.push_sequence_node(vec![node], empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        )
    };
    let view_context = TemplateViewContext::default();
    let view = TirView::new(
        &store,
        child_template_id,
        TemplateTirPhase::Composed,
        view_context,
    )
    .expect("child view should construct");

    let mut context = fold_context(&mut string_table);
    let first =
        fold_tir_view(&view, &store, &mut context).expect("first child fold should succeed");
    let cache_key = TirFoldCacheKey {
        identity: TirViewIdentity {
            root: child_template_id,
            phase: TemplateTirPhase::Composed,
            context: view_context,
        },
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    let before_second = context.fold_cache.get(&cache_key).cloned();
    assert!(
        before_second.is_some(),
        "first child fold should populate the cache under its own view identity"
    );
    let second =
        fold_tir_view(&view, &store, &mut context).expect("second child fold should hit cache");
    assert_eq!(first, second);
    assert_eq!(first, TemplateEmission::Output(child_text));
    assert_eq!(
        context.fold_cache.get(&cache_key),
        before_second.as_ref(),
        "repeated fold should return the cached result without changing output"
    );
}

#[test]
fn fold_tir_view_preserves_root_expression_overlay_through_nested_children() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let empty_context = TemplateViewContext::default();

    let structural_text = string_table.intern("structural-leaf");
    let leaf_template_id = {
        let mut builder = TemplateIrBuilder::new(&mut store);
        let leaf_expression = builder.push_dynamic_expression_node(
            Expression::string_slice(structural_text, empty_location(), ValueMode::ImmutableOwned),
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
        TemplateTirChildReference::new(leaf_template_id, TemplateTirPhase::Composed, empty_context),
    );
    let root_template_id = finish_single_child_template(
        &mut store,
        TemplateTirChildReference::new(
            middle_template_id,
            TemplateTirPhase::Composed,
            empty_context,
        ),
    );

    // Recover the leaf dynamic-expression site id from the leaf template root.
    let leaf_site_id = {
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
    let first_context = {
        let overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(
                leaf_site_id,
                Box::new(Expression::string_slice(
                    first_text,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                )),
            )],
        });
        TemplateViewContext {
            expression_overlay: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        }
    };
    let second_context = {
        let overlay_id = store.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(
                leaf_site_id,
                Box::new(Expression::string_slice(
                    second_text,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                )),
            )],
        });
        TemplateViewContext {
            expression_overlay: Some(overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        }
    };

    let first_view = TirView::new(
        &store,
        root_template_id,
        TemplateTirPhase::Composed,
        first_context,
    )
    .expect("first view should construct");
    let second_view = TirView::new(
        &store,
        root_template_id,
        TemplateTirPhase::Composed,
        second_context,
    )
    .expect("second view should construct");

    let first = {
        let mut context = fold_context(&mut string_table);
        fold_tir_view(&first_view, &store, &mut context)
    }
    .expect("first overlay fold should succeed");
    let second = {
        let mut context = fold_context(&mut string_table);
        fold_tir_view(&second_view, &store, &mut context)
    }
    .expect("second overlay fold should succeed");

    assert_eq!(first, TemplateEmission::Output(first_text));
    assert_eq!(second, TemplateEmission::Output(second_text));
}

#[test]
fn fold_tir_view_below_composed_child_ignores_unconsumed_overlay_identity() {
    let mut string_table = StringTable::new();
    let mut store = TemplateIrStore::new();
    let parent_context = TemplateViewContext::default();
    let missing_context = TemplateViewContext {
        expression_overlay: Some(TirExpressionOverlayId::new(999)),
        ..TemplateViewContext::default()
    };
    let child_text = string_table.intern("parsed child");

    let parent_template_id = {
        let child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Text {
                text: child_text,
                byte_len: "parsed child".len() as u32,
                origin: TemplateSegmentOrigin::Body,
            },
            empty_location(),
        ));
        let child_template_id = store.push_template(TemplateIr::new(
            child_node,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        ));
        let occurrence_id = store.next_child_template_occurrence_id();
        let parent_child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::new(
                    child_template_id,
                    TemplateTirPhase::Parsed,
                    missing_context,
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
        store.push_template(TemplateIr::new(
            parent_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            empty_location(),
        ))
    };

    let view = TirView::new(
        &store,
        parent_template_id,
        TemplateTirPhase::Composed,
        parent_context,
    )
    .expect("parent view should construct");
    let mut context = fold_context(&mut string_table);

    let emission = fold_tir_view(&view, &store, &mut context)
        .expect("a Parsed child's unconsumed overlay identity must not block folding");

    assert_eq!(emission, TemplateEmission::Output(child_text));
}

// -------------------------
//  Deferred fold-authority and attribution invariants
// -------------------------
//
// These tests pin cache-boundary authority validation, runtime-plan authority
// validation, malformed nested-template authority, direct sequence-node cycle
// rejection, and finalization attribution counters on the one-store fold path.

#[test]
fn fold_tir_view_cache_hit_still_validates_malformed_authority() {
    let mut string_table = StringTable::new();
    let mut fixture = build_text_fixture(&mut string_table, "cached authority");
    let mut fold_context = fold_context(&mut string_table);

    {
        let view = TirView::new(
            &fixture.store,
            fixture.template_id,
            TemplateTirPhase::Composed,
            fixture.context,
        )
        .expect("view should construct");
        fold_tir_view(&view, &fixture.store, &mut fold_context)
            .expect("first fold should populate cache");
    }

    // Drop the structural nodes so the cached root no longer resolves to a
    // valid tree. Cache hits must not hide this malformed current-store
    // authority.
    fixture.store.nodes.clear();

    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should still construct after clearing nodes");
    let error = fold_tir_view(&view, &fixture.store, &mut fold_context)
        .expect_err("cache hits must not hide malformed current-store authority");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing cached node should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR preparation: node"),
        "expected a stable cache-boundary authority error, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_runtime_plan_early_return_validates_plan_authority() {
    let mut string_table = StringTable::new();
    let mut fixture = build_text_fixture(&mut string_table, "runtime plan");
    let missing_slot_plan_id = TemplateSlotPlanId::new(999);
    fixture.store.templates[fixture.template_id.index()].runtime_slot_plan =
        Some(missing_slot_plan_id);

    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");
    let mut fold_context = fold_context(&mut string_table);
    let error = fold_tir_view(&view, &fixture.store, &mut fold_context)
        .expect_err("runtime-plan early return must validate its required plan");
    let TemplateError::Infrastructure(error) = error else {
        panic!("missing runtime slot plan should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR preparation: slot plan"),
        "expected a stable runtime-plan authority error, got: {}",
        error.msg
    );
}

/// Folds an outer template whose dynamic-expression payload is a nested AST
/// template whose root node is missing, so the fold walker catches the
/// malformed nested-template authority on the infrastructure lane.
fn fold_dynamic_ast_template_with_missing_root_authority() -> TemplateError {
    let mut string_table = StringTable::new();
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let context = TemplateViewContext::default();

    let outer_template_id = {
        let mut tir = store.borrow_mut();
        let nested_template_id = tir.push_template(TemplateIr::new(
            TemplateIrNodeId::new(999),
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ));

        let nested_template = Template {
            kind: TemplateType::String,
            tir_reference: TemplateTirReference {
                root: nested_template_id,
                phase: TemplateTirPhase::Composed,
                context,
            },
            location: empty_location(),
        };

        let mut builder = TemplateIrBuilder::new(&mut tir);
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
            TemplateIrSummary::default(),
            empty_location(),
        )
    };

    let store_ref = store.borrow();
    let view = TirView::new(
        &store_ref,
        outer_template_id,
        TemplateTirPhase::Composed,
        context,
    )
    .expect("outer view should construct");
    let mut fold_context = TemplateFoldContext {
        string_table: &mut string_table,
        project_path_resolver: Box::leak(Box::new(
            crate::compiler_frontend::paths::path_resolution::ProjectPathResolver::new(
                std::env::temp_dir(),
                std::env::temp_dir(),
                crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(
                ),
                &crate::builder_surface::SourceFileKindRegistry::default(),
            )
            .expect("test path resolver should be valid"),
        )),
        path_format_config: Box::leak(Box::new(
            crate::compiler_frontend::paths::path_format::PathStringFormatConfig::default(),
        )),
        source_file_scope: Box::leak(Box::new(InternedPath::new())),
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_store: Some(Rc::clone(&store)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    };

    fold_tir_view(&view, &store_ref, &mut fold_context)
        .expect_err("dynamic AST templates must enter their own fold authority boundary")
}

#[test]
fn fold_tir_view_dynamic_ast_template_validates_malformed_root_authority() {
    let error = fold_dynamic_ast_template_with_missing_root_authority();
    let TemplateError::Infrastructure(error) = error else {
        panic!("malformed dynamic template root should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("does not exist in the module store"),
        "expected dynamic-template root authority failure, got: {}",
        error.msg
    );
}

#[test]
fn fold_tir_view_rejects_direct_sequence_node_cycle_as_infrastructure() {
    let mut store = TemplateIrStore::new();
    let context = TemplateViewContext::default();
    // The first pushed node gets index 0, so a Sequence root whose only child
    // is `TemplateIrNodeId::new(0)` is a malformed self-cycle.
    let root = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Sequence {
            children: vec![TemplateIrNodeId::new(0)],
        },
        empty_location(),
    ));
    let template_id = store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    ));

    let view = TirView::new(&store, template_id, TemplateTirPhase::Composed, context)
        .expect("cyclic view should construct");
    let mut string_table = StringTable::new();
    let mut fold_context = fold_context(&mut string_table);
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

#[cfg(feature = "benchmark_counters")]
#[test]
fn fold_tir_view_increments_phase1_attribution_counters() {
    use crate::compiler_frontend::instrumentation::{
        AstCounter, lock_counter_test, reset_ast_counters, test_read_ast_counter,
    };

    let _guard = lock_counter_test();

    let mut string_table = StringTable::new();
    let fixture = build_text_fixture(&mut string_table, "counter probe");
    let view = TirView::new(
        &fixture.store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");
    let mut fold_context = fold_context(&mut string_table);

    reset_ast_counters();
    let first =
        fold_tir_view(&view, &fixture.store, &mut fold_context).expect("first fold should succeed");
    // Second fold with empty bindings must hit the fold cache.
    let second = fold_tir_view(&view, &fixture.store, &mut fold_context)
        .expect("second fold should succeed");
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
