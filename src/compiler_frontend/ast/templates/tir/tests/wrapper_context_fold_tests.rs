//! TIR wrapper-context overlay fold tests.
//!
//! WHAT: exercises view-native folding of inherited `$children(..)` wrappers
//!       and `$fresh` suppression applied through wrapper-context overlays.
//!
//! WHY: wrapper-context overlays replace the structural mutation of
//!      `conditional_child_wrapper_set`. These tests prove the overlay path
//!      produces the same output as the current-state wrapper composition path.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::template::TemplateType;
use crate::compiler_frontend::ast::templates::template::{SlotKey, Style, TemplateSegmentOrigin};
use crate::compiler_frontend::ast::templates::template_control_flow::TemplateBranchSelector;
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold::{fold_tir_view, fold_tir_view_prepared};
use crate::compiler_frontend::ast::templates::tir::fold_cache::TirFoldCache;
use crate::compiler_frontend::ast::templates::tir::fold_safety::{
    TirFoldFallbackReason, classify_view_native_fold_safety, prepare_tir_view_fold,
};
use crate::compiler_frontend::ast::templates::tir::ids::{
    ChildTemplateOccurrenceId, ExpressionSiteId, SlotOccurrenceId, TemplateIrId,
    TemplateWrapperSetId,
};
use crate::compiler_frontend::ast::templates::tir::node::{TemplateIrBranch, TemplateIrNodeKind};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirSlotResolution,
    TirSlotResolutionOverlay, TirWrapperApplicationMode, TirWrapperContext,
    TirWrapperContextOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateRef, TemplateStoreId, TemplateTirChildReference, TemplateWrapperReference,
    TemplateWrapperSetRef,
};
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::slot_plan::TemplateSlotPlan;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeTemplateBody, OwnedRuntimeTemplateHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
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

fn runtime_string_expression() -> Expression {
    Expression::new(
        ExpressionKind::Reference(InternedPath::new()),
        empty_location(),
        builtin_type_ids::STRING,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    )
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

fn build_expression_wrapper_template(
    store: &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
    string_table: &mut StringTable,
) -> (
    crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    ExpressionSiteId,
) {
    let structural_id = string_table.intern("structural-wrapper");
    build_expression_wrapper_template_with_expression(
        store,
        Expression::string_slice(structural_id, empty_location(), ValueMode::ImmutableOwned),
    )
}

fn build_expression_wrapper_template_with_expression(
    store: &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
    expression: Expression,
) -> (
    crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    ExpressionSiteId,
) {
    let mut builder = TemplateIrBuilder::new(store);
    let dynamic_node = builder.push_dynamic_expression_node(
        expression,
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
        .expect("expression wrapper node should exist")
        .kind
    {
        TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
        other => panic!("expected dynamic expression wrapper node, got {other:?}"),
    };

    (template_id, site_id)
}

fn build_two_slot_wrapper_template(
    store: &mut crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore,
    string_table: &mut StringTable,
) -> (
    crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    SlotOccurrenceId,
    SlotKey,
) {
    let before_id = string_table.intern("before");
    let named_id = string_table.intern("named");
    let after_id = string_table.intern("after");
    let named_key = SlotKey::named(named_id);
    let mut builder = TemplateIrBuilder::new(store);
    let before_node = builder.push_text_node(
        before_id,
        "before".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let default_slot = builder.push_slot_node(SlotKey::Default, empty_location());
    let named_slot = builder.push_slot_node(named_key.clone(), empty_location());
    let after_node = builder.push_text_node(
        after_id,
        "after".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let root = builder.push_sequence_node(
        vec![before_node, default_slot, named_slot, after_node],
        empty_location(),
    );
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    );
    let named_occurrence_id = match &store
        .get_node(named_slot)
        .expect("named slot node should exist")
        .kind
    {
        TemplateIrNodeKind::Slot { placeholder } => placeholder.occurrence_id,
        other => panic!("expected named slot node, got {other:?}"),
    };

    (template_id, named_occurrence_id, named_key)
}

struct WrapperContextFixture {
    registry: Rc<RefCell<TemplateIrRegistry>>,
    store_id: TemplateStoreId,
    parent_template_id: crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    wrapper_template_id: Option<crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId>,
    overlay_set_id: TemplateOverlaySetId,
}

fn allocate_wrapper_context_overlay(
    registry: &mut TemplateIrRegistry,
    store_id: TemplateStoreId,
    wrapper_set_id: TemplateWrapperSetId,
    child_occurrence_id: ChildTemplateOccurrenceId,
) -> TemplateOverlaySetId {
    let wrapper_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
            contexts: vec![(
                child_occurrence_id,
                TirWrapperContext {
                    inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                        store_id,
                        wrapper_set_id,
                    )),
                    ..TirWrapperContext::default()
                },
            )],
        });
    registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_context_overlay_id),
    })
}

fn build_same_store_expression_wrapper_fixture(
    string_table: &mut StringTable,
) -> (WrapperContextFixture, ExpressionSiteId) {
    let wrapper_text_id = string_table.intern("same-store-overlay");
    build_same_store_expression_wrapper_fixture_with_expressions(
        string_table,
        Expression::string_slice(wrapper_text_id, empty_location(), ValueMode::ImmutableOwned),
        None,
    )
}

fn build_same_store_expression_wrapper_fixture_with_expressions(
    string_table: &mut StringTable,
    wrapper_expression: Expression,
    outer_expression: Option<Expression>,
) -> (WrapperContextFixture, ExpressionSiteId) {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let wrapper_expression_overlay = wrapper_expression.clone();

    let (parent_template_id, wrapper_template_id, wrapper_set_id, child_occurrence_id, site_id) = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        let child_template_id = build_text_template(&mut store, string_table, "child");
        let (wrapper_template_id, site_id) =
            build_expression_wrapper_template_with_expression(&mut store, wrapper_expression);
        let child_reference = TemplateTirChildReference::same_store(
            child_template_id,
            store_id,
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let mut builder = TemplateIrBuilder::new(&mut store);
        let child_node =
            builder.push_child_template_node_with_reference(child_reference, empty_location());
        let root = builder.push_sequence_node(vec![child_node], empty_location());
        let parent_template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let wrapper_reference = TemplateWrapperReference::new(
            store.qualify_template_ref(wrapper_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let wrapper_set_id = store.push_or_reuse_wrapper_set(vec![wrapper_reference]);
        (
            parent_template_id,
            wrapper_template_id,
            wrapper_set_id,
            ChildTemplateOccurrenceId::new(0),
            site_id,
        )
    };

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(site_id, Box::new(wrapper_expression_overlay))],
    });
    let wrapper_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    {
        let store_handle = registry.store_handle(store_id).expect("store should exist");
        let mut store = store_handle.borrow_mut();
        store.wrapper_sets[wrapper_set_id.index()].wrappers[0].overlay_set_id =
            wrapper_overlay_set_id;
    }

    let wrapper_context_only_overlay_set_id = allocate_wrapper_context_overlay(
        &mut registry,
        store_id,
        wrapper_set_id,
        child_occurrence_id,
    );
    let parent_overlay_set_id = if let Some(outer_expression) = outer_expression {
        let wrapper_context_overlay_id = registry
            .overlay_set(wrapper_context_only_overlay_set_id)
            .and_then(|overlay_set| overlay_set.wrapper_context)
            .expect("wrapper-context overlay should contain its context ID");
        let outer_expression_overlay_id =
            registry.allocate_expression_overlay(TirExpressionOverlay {
                overrides: vec![(site_id, Box::new(outer_expression))],
            });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(outer_expression_overlay_id),
            slot_resolution: None,
            wrapper_context: Some(wrapper_context_overlay_id),
        })
    } else {
        wrapper_context_only_overlay_set_id
    };

    (
        WrapperContextFixture {
            registry: Rc::new(RefCell::new(registry)),
            store_id,
            parent_template_id,
            wrapper_template_id: Some(wrapper_template_id),
            overlay_set_id: parent_overlay_set_id,
        },
        site_id,
    )
}

fn build_foreign_expression_wrapper_fixture(
    string_table: &mut StringTable,
) -> WrapperContextFixture {
    let mut registry = TemplateIrRegistry::new();
    let parent_store_id = registry.allocate_store();
    let foreign_store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (wrapper_template_id, site_id) = {
        let mut foreign_store = registry
            .store_mut(foreign_store_id)
            .expect("foreign store should exist");
        build_expression_wrapper_template(&mut foreign_store, string_table)
    };
    let (parent_template_id, wrapper_set_id, child_occurrence_id) = {
        let mut parent_store = registry
            .store_mut(parent_store_id)
            .expect("parent store should exist");
        let child_template_id = build_text_template(&mut parent_store, string_table, "child");
        let child_reference = TemplateTirChildReference::same_store(
            child_template_id,
            parent_store_id,
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let mut builder = TemplateIrBuilder::new(&mut parent_store);
        let child_node =
            builder.push_child_template_node_with_reference(child_reference, empty_location());
        let root = builder.push_sequence_node(vec![child_node], empty_location());
        let parent_template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let wrapper_reference = TemplateWrapperReference::new(
            TemplateRef::new(foreign_store_id, wrapper_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let wrapper_set_id = parent_store.push_or_reuse_wrapper_set(vec![wrapper_reference]);
        (
            parent_template_id,
            wrapper_set_id,
            ChildTemplateOccurrenceId::new(0),
        )
    };

    let overlay_text_id = string_table.intern("foreign-overlay");
    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            site_id,
            Box::new(Expression::string_slice(
                overlay_text_id,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let wrapper_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    {
        let store_handle = registry
            .store_handle(parent_store_id)
            .expect("parent store should exist");
        let mut store = store_handle.borrow_mut();
        store.wrapper_sets[wrapper_set_id.index()].wrappers[0].overlay_set_id =
            wrapper_overlay_set_id;
    }

    let parent_overlay_set_id = allocate_wrapper_context_overlay(
        &mut registry,
        parent_store_id,
        wrapper_set_id,
        child_occurrence_id,
    );

    WrapperContextFixture {
        registry: Rc::new(RefCell::new(registry)),
        store_id: parent_store_id,
        parent_template_id,
        wrapper_template_id: Some(wrapper_template_id),
        overlay_set_id: parent_overlay_set_id,
    }
}

fn build_slot_resolution_wrapper_fixture(string_table: &mut StringTable) -> WrapperContextFixture {
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let (
        parent_template_id,
        wrapper_template_id,
        wrapper_set_id,
        child_occurrence_id,
        named_slot_id,
        named_key,
        source_template_id,
    ) = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        let child_template_id = build_text_template(&mut store, string_table, "injected");
        let source_template_id = build_text_template(&mut store, string_table, "resolved");
        let (wrapper_template_id, named_slot_id, named_key) =
            build_two_slot_wrapper_template(&mut store, string_table);
        let child_reference = TemplateTirChildReference::same_store(
            child_template_id,
            store_id,
            TemplateTirPhase::Composed,
            empty_overlay_set_id,
        );
        let mut builder = TemplateIrBuilder::new(&mut store);
        let child_node =
            builder.push_child_template_node_with_reference(child_reference, empty_location());
        let root = builder.push_sequence_node(vec![child_node], empty_location());
        let parent_template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        );
        let wrapper_reference = TemplateWrapperReference::new(
            store.qualify_template_ref(wrapper_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let wrapper_set_id = store.push_or_reuse_wrapper_set(vec![wrapper_reference]);
        (
            parent_template_id,
            wrapper_template_id,
            wrapper_set_id,
            ChildTemplateOccurrenceId::new(0),
            named_slot_id,
            named_key,
            source_template_id,
        )
    };

    let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
        resolutions: vec![(
            named_slot_id,
            TirSlotResolution::resolved(
                named_key,
                vec![TemplateRef::new(store_id, source_template_id)],
            ),
        )],
    });
    let wrapper_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_overlay_id),
        wrapper_context: None,
    });
    {
        let store_handle = registry.store_handle(store_id).expect("store should exist");
        let mut store = store_handle.borrow_mut();
        store.wrapper_sets[wrapper_set_id.index()].wrappers[0].overlay_set_id =
            wrapper_overlay_set_id;
    }

    let parent_overlay_set_id = allocate_wrapper_context_overlay(
        &mut registry,
        store_id,
        wrapper_set_id,
        child_occurrence_id,
    );

    WrapperContextFixture {
        registry: Rc::new(RefCell::new(registry)),
        store_id,
        parent_template_id,
        wrapper_template_id: Some(wrapper_template_id),
        overlay_set_id: parent_overlay_set_id,
    }
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

fn build_nested_virtual_wrapper_fixture(string_table: &mut StringTable) -> WrapperContextFixture {
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();
    let empty_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let (
        parent_template_id,
        outer_wrapper_template_id,
        outer_wrapper_set_id,
        inner_wrapper_set_id,
        parent_occurrence_id,
        nested_occurrence_id,
        outer_expression_site_id,
    ) = {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");

        let parent_child_template_id = build_text_template(&mut store, string_table, "parent");
        let nested_child_template_id = build_text_template(&mut store, string_table, "nested");
        let inner_wrapper_template_id =
            build_slot_wrapper_template(&mut store, string_table, "inner-before", "inner-after");
        let inner_wrapper_reference = TemplateWrapperReference::new(
            store.qualify_template_ref(inner_wrapper_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let inner_wrapper_set_id = store.push_or_reuse_wrapper_set(vec![inner_wrapper_reference]);

        let outer_expression = string_table.intern("outer-structural");
        let outer_dynamic_node = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            builder.push_dynamic_expression_node(
                Expression::string_slice(
                    outer_expression,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
                None,
                empty_location(),
            )
        };
        let (outer_wrapper_template_id, nested_child_node) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let nested_child_node = builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    nested_child_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
                ),
                empty_location(),
            );
            let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
            let after_text = string_table.intern("outer-after");
            let after_node = builder.push_text_node(
                after_text,
                "outer-after".len() as u32,
                TemplateSegmentOrigin::Body,
                empty_location(),
            );
            let root = builder.push_sequence_node(
                vec![outer_dynamic_node, nested_child_node, slot_node, after_node],
                empty_location(),
            );
            let template_id = builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::empty(),
                empty_location(),
            );
            (template_id, nested_child_node)
        };

        let parent_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let parent_child_node = builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    parent_child_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
                ),
                empty_location(),
            );
            let root = builder.push_sequence_node(vec![parent_child_node], empty_location());
            builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::empty(),
                empty_location(),
            )
        };

        let parent_occurrence_id = match &store
            .get_node(
                store
                    .get_template(parent_template_id)
                    .expect("parent template should exist")
                    .root,
            )
            .expect("parent root should exist")
            .kind
        {
            TemplateIrNodeKind::Sequence { children } => match &store
                .get_node(children[0])
                .expect("parent child node should exist")
                .kind
            {
                TemplateIrNodeKind::ChildTemplate { occurrence_id, .. } => *occurrence_id,
                _ => panic!("expected parent child-template node"),
            },
            _ => panic!("expected parent sequence root"),
        };
        let nested_occurrence_id = match &store
            .get_node(nested_child_node)
            .expect("nested child node should exist")
            .kind
        {
            TemplateIrNodeKind::ChildTemplate { occurrence_id, .. } => *occurrence_id,
            _ => panic!("expected nested child-template node"),
        };
        let outer_expression_site_id = match &store
            .get_node(outer_dynamic_node)
            .expect("outer dynamic node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            _ => panic!("expected outer dynamic-expression node"),
        };
        let outer_wrapper_reference = TemplateWrapperReference::new(
            store.qualify_template_ref(outer_wrapper_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let outer_wrapper_set_id = store.push_or_reuse_wrapper_set(vec![outer_wrapper_reference]);

        (
            parent_template_id,
            outer_wrapper_template_id,
            outer_wrapper_set_id,
            inner_wrapper_set_id,
            parent_occurrence_id,
            nested_occurrence_id,
            outer_expression_site_id,
        )
    };

    let nested_context_overlay_id =
        registry
            .borrow_mut()
            .allocate_wrapper_context_overlay(TirWrapperContextOverlay {
                contexts: vec![(
                    nested_occurrence_id,
                    TirWrapperContext {
                        inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                            store_id,
                            inner_wrapper_set_id,
                        )),
                        ..TirWrapperContext::default()
                    },
                )],
            });
    let outer_expression_overlay_id =
        registry
            .borrow_mut()
            .allocate_expression_overlay(TirExpressionOverlay {
                overrides: vec![(
                    outer_expression_site_id,
                    Box::new(Expression::string_slice(
                        string_table.intern("outer-overlay"),
                        empty_location(),
                        ValueMode::ImmutableOwned,
                    )),
                )],
            });
    let outer_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(outer_expression_overlay_id),
            slot_resolution: None,
            wrapper_context: Some(nested_context_overlay_id),
        });

    let store_handle = registry
        .borrow()
        .store_handle(store_id)
        .expect("store should exist");
    let mut store = store_handle.borrow_mut();
    store.wrapper_sets[outer_wrapper_set_id.index()].wrappers[0].overlay_set_id =
        outer_overlay_set_id;
    drop(store);

    let parent_context_overlay_id =
        registry
            .borrow_mut()
            .allocate_wrapper_context_overlay(TirWrapperContextOverlay {
                contexts: vec![(
                    parent_occurrence_id,
                    TirWrapperContext {
                        inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                            store_id,
                            outer_wrapper_set_id,
                        )),
                        ..TirWrapperContext::default()
                    },
                )],
            });
    let parent_overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: None,
            wrapper_context: Some(parent_context_overlay_id),
        });

    WrapperContextFixture {
        registry,
        store_id,
        parent_template_id,
        wrapper_template_id: Some(outer_wrapper_template_id),
        overlay_set_id: parent_overlay_set_id,
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
    let parent_phase = fixture_parent_view_phase(&registry_borrow, fixture.overlay_set_id);
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        parent_phase,
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

fn prepared_fold_fixture_result(
    fixture: &WrapperContextFixture,
    string_table: &mut StringTable,
) -> Result<TemplateEmission, TemplateError> {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let registry_borrow = fixture.registry.borrow();
    let parent_phase = fixture_parent_view_phase(&registry_borrow, fixture.overlay_set_id);
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        parent_phase,
        fixture.overlay_set_id,
    )
    .expect("test view should construct");
    let store = registry_borrow
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();
    let preparation = prepare_tir_view_fold(&view, &store, string_table)?;
    assert!(
        preparation.fold_eligible(),
        "supported nested wrapper fixture should pass the production fold gate: {:?}",
        preparation.fallback_reason()
    );

    let mut fold_context = build_test_fold_context(
        string_table,
        &resolver,
        &path_format,
        &source_scope,
        &fixture.registry,
    );

    fold_tir_view_prepared(&view, &store, &mut fold_context, preparation)
}

fn fixture_parent_view_phase(
    registry: &TemplateIrRegistry,
    overlay_set_id: TemplateOverlaySetId,
) -> TemplateTirPhase {
    let has_expression_overlay = registry
        .overlay_set(overlay_set_id)
        .is_some_and(|overlay_set| overlay_set.expression_overrides.is_some());
    if has_expression_overlay {
        TemplateTirPhase::Finalized
    } else {
        TemplateTirPhase::Composed
    }
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
fn fold_tir_view_applies_nested_wrapper_context_after_entering_exact_wrapper_view() {
    let mut string_table = StringTable::new();
    let fixture = build_nested_virtual_wrapper_fixture(&mut string_table);

    let emission = prepared_fold_fixture_result(&fixture, &mut string_table)
        .expect("supported nested wrapper should pass the production fold gate");
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {other:?}"),
    };
    assert_eq!(
        string_table.resolve(output_id),
        "outer-overlayinner-beforenestedinner-afterparentouter-after",
        "nested wrapper contexts must apply inside the virtual wrapper using its exact overlays"
    );

    let handoff = handoff_fixture(&fixture, &mut string_table);
    let outer = expect_single_render_child(&handoff.body);
    let OwnedRuntimeTemplateNode::Sequence { children, .. } = outer else {
        panic!("expected the outer wrapper sequence, got {outer:?}");
    };
    assert_eq!(children.len(), 4);
    let OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } = &children[0] else {
        panic!("expected the exact outer expression overlay in the handoff");
    };
    assert!(matches!(
        expression.kind,
        ExpressionKind::StringSlice(text) if string_table.resolve(text) == "outer-overlay"
    ));

    let OwnedRuntimeTemplateNode::Sequence {
        children: inner_children,
    } = &children[1]
    else {
        panic!("expected the nested occurrence wrapper in the handoff");
    };
    assert_eq!(inner_children.len(), 3);
    assert_text_or_single_sequence_node(&inner_children[0], "inner-before", &string_table);
    assert_child_or_text_node(&inner_children[1], "nested", &string_table);
    assert_text_or_single_sequence_node(&inner_children[2], "inner-after", &string_table);
    assert_child_or_text_node(&children[2], "parent", &string_table);
    assert_text_or_single_sequence_node(&children[3], "outer-after", &string_table);
}

#[test]
fn fold_tir_view_applies_same_store_wrapper_expression_overlay() {
    let mut string_table = StringTable::new();
    let (fixture, _) = build_same_store_expression_wrapper_fixture(&mut string_table);

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {other:?}"),
    };

    assert_eq!(
        string_table.resolve(output_id),
        "same-store-overlaychild",
        "same-store inherited wrappers must fold through their exact expression overlay"
    );
}

#[test]
fn wrapper_safety_preserves_outer_runtime_expression_override_in_handoff() {
    let mut string_table = StringTable::new();
    let wrapper_text = string_table.intern("wrapper-local");
    let (fixture, _) = build_same_store_expression_wrapper_fixture_with_expressions(
        &mut string_table,
        Expression::string_slice(wrapper_text, empty_location(), ValueMode::ImmutableOwned),
        Some(runtime_string_expression()),
    );

    let preparation = {
        let registry = fixture.registry.borrow();
        let view = TirView::new(
            &registry,
            TemplateRef::new(fixture.store_id, fixture.parent_template_id),
            TemplateTirPhase::Finalized,
            fixture.overlay_set_id,
        )
        .expect("parent view should construct");
        let store = registry
            .store_handle(fixture.store_id)
            .expect("store should exist")
            .borrow()
            .clone();
        prepare_tir_view_fold(&view, &store, &string_table)
            .expect("outer runtime wrapper override should be a valid fallback")
    };
    assert!(
        !preparation.fold_eligible(),
        "a runtime outer override must not be classified as a const wrapper"
    );

    let handoff = handoff_fixture(&fixture, &mut string_table);
    let wrapped = expect_single_render_child(&handoff.body);
    let OwnedRuntimeTemplateNode::Sequence { children } = wrapped else {
        panic!("expected wrapper sequence in the owned handoff, got {wrapped:?}");
    };
    let expression_node = match children.first() {
        Some(OwnedRuntimeTemplateNode::DynamicExpression { .. }) => &children[0],
        Some(OwnedRuntimeTemplateNode::Sequence { children }) if children.len() == 1 => {
            &children[0]
        }
        Some(other) => {
            panic!("expected the wrapper expression to survive in the handoff, got {other:?}");
        }
        None => panic!("expected a wrapper expression in the owned handoff"),
    };
    let OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } = expression_node else {
        panic!("expected the wrapper expression node, got {expression_node:?}");
    };
    assert!(
        matches!(expression.kind, ExpressionKind::Reference(_)),
        "the outer runtime expression must override the const wrapper-local expression"
    );
}

#[test]
fn wrapper_safety_folds_const_outer_override_over_runtime_wrapper_expression() {
    let mut string_table = StringTable::new();
    let outer_text = string_table.intern("outer-const");
    let (fixture, _) = build_same_store_expression_wrapper_fixture_with_expressions(
        &mut string_table,
        runtime_string_expression(),
        Some(Expression::string_slice(
            outer_text,
            empty_location(),
            ValueMode::ImmutableOwned,
        )),
    );

    let emission =
        prepared_fold_fixture_result(&fixture, &mut string_table).unwrap_or_else(|error| {
            panic!("const outer override should make the wrapper foldable: {error:?}");
        });
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {other:?}"),
    };
    assert_eq!(
        string_table.resolve(output_id),
        "outer-constchild",
        "the const outer override must replace the runtime wrapper-local expression"
    );
}

#[test]
fn fold_tir_view_applies_foreign_wrapper_expression_overlay() {
    let mut string_table = StringTable::new();
    let fixture = build_foreign_expression_wrapper_fixture(&mut string_table);

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {other:?}"),
    };

    assert_eq!(
        string_table.resolve(output_id),
        "foreign-overlaychild",
        "foreign inherited wrappers must fold through their owning-store expression overlay"
    );
}

#[test]
fn fold_tir_view_injects_child_before_resolving_other_wrapper_slots() {
    let mut string_table = StringTable::new();
    let fixture = build_slot_resolution_wrapper_fixture(&mut string_table);

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {other:?}"),
    };

    assert_eq!(
        string_table.resolve(output_id),
        "beforeinjectedresolvedafter",
        "the injected target must win while other slots preserve overlay-resolved sources"
    );
}

#[test]
fn preparation_falls_back_for_runtime_non_injected_slot_source() {
    let mut string_table = StringTable::new();
    let fixture = build_slot_resolution_wrapper_fixture(&mut string_table);
    let resolved_text = string_table.intern("resolved");
    let source_template_id = {
        let registry = fixture.registry.borrow();
        let store_handle = registry
            .store_handle(fixture.store_id)
            .expect("store should exist");
        let store = store_handle.borrow();
        store
            .templates
            .iter()
            .enumerate()
            .find_map(|(index, template)| {
                let TemplateIrNodeKind::Sequence { children } =
                    &store.get_node(template.root)?.kind
                else {
                    return None;
                };
                let [child] = children.as_slice() else {
                    return None;
                };
                match &store.get_node(*child)?.kind {
                    TemplateIrNodeKind::Text { text, .. } if *text == resolved_text => {
                        Some(TemplateIrId::new(index))
                    }
                    _ => None,
                }
            })
            .expect("resolved slot source template should be present")
    };
    {
        let store_handle = fixture
            .registry
            .borrow()
            .store_handle(fixture.store_id)
            .expect("store should exist");
        let mut store = store_handle.borrow_mut();
        let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
            location: empty_location(),
            contribution_sources: Vec::new(),
            slot_sites: Vec::new(),
        });
        store.templates[source_template_id.index()].runtime_slot_plan = Some(slot_plan_id);
    }

    let fallback_reason = {
        let registry = fixture.registry.borrow();
        let view = TirView::new(
            &registry,
            TemplateRef::new(fixture.store_id, fixture.parent_template_id),
            TemplateTirPhase::Composed,
            fixture.overlay_set_id,
        )
        .expect("parent view should construct");
        let store = registry
            .store_handle(fixture.store_id)
            .expect("store should exist")
            .borrow()
            .clone();
        let preparation = prepare_tir_view_fold(&view, &store, &string_table)
            .expect("runtime slot source should be an eligible preparation fallback");
        preparation.fallback_reason()
    };

    assert_eq!(
        fallback_reason,
        Some(TirFoldFallbackReason::WrapperContextOverlay),
        "a runtime source in a non-injected wrapper slot must stay on the handoff path"
    );

    let handoff = handoff_fixture(&fixture, &mut string_table);
    assert!(
        format!("{:?}", handoff.body).contains("RuntimeSlotApplication"),
        "owned handoff must retain the runtime slot source instead of losing it during folding"
    );
}

#[test]
fn below_composed_wrapper_reference_uses_structural_root_without_overlay_lookup() {
    let mut string_table = StringTable::new();
    let fixture = build_wrapper_context_fixture(&mut string_table, TirWrapperContext::default());
    let wrapper_template_id = fixture
        .wrapper_template_id
        .expect("fixture should have a wrapper template");

    let store_handle = fixture
        .registry
        .borrow()
        .store_handle(fixture.store_id)
        .expect("store handle should exist");
    let mut store = store_handle.borrow_mut();
    let wrapper_set = store
        .wrapper_sets
        .iter_mut()
        .find(|wrapper_set| {
            wrapper_set
                .wrappers
                .iter()
                .any(|wrapper| wrapper.root.template_id == wrapper_template_id)
        })
        .expect("fixture should have an inherited wrapper set");
    let wrapper = wrapper_set
        .wrappers
        .first_mut()
        .expect("inherited wrapper set should not be empty");
    wrapper.phase = TemplateTirPhase::Parsed;
    wrapper.overlay_set_id = TemplateOverlaySetId::new(999);
    drop(store);

    let emission = fold_fixture(&fixture, &mut string_table);
    let output_id = match emission {
        TemplateEmission::Output(id) => id,
        other => panic!("expected Output emission, got {other:?}"),
    };
    assert_eq!(string_table.resolve(output_id), "beforechildafter");

    let handoff = handoff_fixture(&fixture, &mut string_table);
    let wrapped = expect_single_render_child(&handoff.body);
    let OwnedRuntimeTemplateNode::Sequence { children, .. } = wrapped else {
        panic!("expected structural wrapper sequence, got {wrapped:?}");
    };
    assert_eq!(children.len(), 3);
    assert_text_node(&children[0], "before", &string_table);
    assert_child_or_text_node(&children[1], "child", &string_table);
    assert_text_node(&children[2], "after", &string_table);
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
    let parent_phase = fixture_parent_view_phase(&registry_borrow, fixture.overlay_set_id);
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        parent_phase,
        fixture.overlay_set_id,
    )
    .expect("test view should construct");

    let store = registry_borrow
        .store_handle(fixture.store_id)
        .expect("store handle should exist")
        .borrow()
        .clone();

    let reason = classify_view_native_fold_safety(&view, &store)
        .expect("fold safety authority should resolve");
    assert!(
        reason.is_none(),
        "safe wrapper-context overlay should be foldable, got {:?}",
        reason
    );
}

#[test]
fn preparation_falls_back_for_runtime_wrapper_dynamic_expression() {
    let mut string_table = StringTable::new();
    let (fixture, site_id) = build_same_store_expression_wrapper_fixture(&mut string_table);
    let runtime_expression = Expression::new(
        ExpressionKind::Reference(InternedPath::new()),
        empty_location(),
        builtin_type_ids::STRING,
        DataType::StringSlice,
        ValueMode::ImmutableOwned,
    );
    let runtime_overlay_set_id = {
        let mut registry = fixture.registry.borrow_mut();
        let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
            overrides: vec![(site_id, Box::new(runtime_expression))],
        });
        registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: Some(expression_overlay_id),
            slot_resolution: None,
            wrapper_context: None,
        })
    };
    {
        let store_handle = fixture
            .registry
            .borrow()
            .store_handle(fixture.store_id)
            .expect("store should exist");
        let mut store = store_handle.borrow_mut();
        let wrapper_template_id = fixture
            .wrapper_template_id
            .expect("expression wrapper should be present");
        let wrapper_set = store
            .wrapper_sets
            .iter_mut()
            .find(|wrapper_set| {
                wrapper_set
                    .wrappers
                    .iter()
                    .any(|wrapper| wrapper.root.template_id == wrapper_template_id)
            })
            .expect("expression wrapper set should be present");
        wrapper_set.wrappers[0].overlay_set_id = runtime_overlay_set_id;
    }

    let fallback_reason = {
        let registry = fixture.registry.borrow();
        let view = TirView::new(
            &registry,
            TemplateRef::new(fixture.store_id, fixture.parent_template_id),
            TemplateTirPhase::Composed,
            fixture.overlay_set_id,
        )
        .expect("parent view should construct");
        let store = registry
            .store_handle(fixture.store_id)
            .expect("store should exist")
            .borrow()
            .clone();
        prepare_tir_view_fold(&view, &store, &string_table)
            .expect("runtime wrapper expression should be a semantic fallback")
            .fallback_reason()
    };

    assert_eq!(
        fallback_reason,
        Some(TirFoldFallbackReason::WrapperContextOverlay),
        "effective runtime wrapper expressions must not be folded away"
    );
    let handoff = handoff_fixture(&fixture, &mut string_table);
    assert!(
        format!("{:?}", handoff.body).contains("DynamicExpression"),
        "owned handoff must retain the runtime wrapper expression"
    );
}

#[test]
fn preparation_terminates_for_cyclic_nested_wrapper_contexts() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let (wrapper_template_id, parent_template_id) = {
        let mut store = registry.store_mut(store_id).expect("store should exist");
        let child_template_id = build_text_template(&mut store, &mut string_table, "child");

        let wrapper_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child = builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    TemplateIrId::new(2),
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
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

        let parent_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child = builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    child_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
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

        (wrapper_template_id, parent_template_id)
    };

    let nested_wrapper_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
            contexts: vec![(
                ChildTemplateOccurrenceId::new(0),
                TirWrapperContext {
                    inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                        store_id,
                        TemplateWrapperSetId::new(0),
                    )),
                    ..TirWrapperContext::default()
                },
            )],
        });
    let nested_wrapper_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(nested_wrapper_context_overlay_id),
    });
    let wrapper_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
            contexts: vec![(
                ChildTemplateOccurrenceId::new(1),
                TirWrapperContext {
                    inherited_wrapper_set: Some(TemplateWrapperSetRef::new(
                        store_id,
                        TemplateWrapperSetId::new(0),
                    )),
                    ..TirWrapperContext::default()
                },
            )],
        });
    let parent_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_context_overlay_id),
    });

    {
        let store_handle = registry.store_handle(store_id).expect("store should exist");
        let mut store = store_handle.borrow_mut();
        store.push_wrapper_set(
            crate::compiler_frontend::ast::templates::tir::store::TemplateWrapperSet {
                wrappers: vec![TemplateWrapperReference::new(
                    TemplateRef::new(store_id, wrapper_template_id),
                    TemplateTirPhase::Finalized,
                    nested_wrapper_overlay_set_id,
                )],
            },
        );
    }

    let registry = Rc::new(RefCell::new(registry));
    let registry_borrow = registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Composed,
        parent_overlay_set_id,
    )
    .expect("cyclic wrapper view should construct");
    let store = registry_borrow
        .store_handle(store_id)
        .expect("store should exist")
        .borrow()
        .clone();

    let preparation = prepare_tir_view_fold(&view, &store, &string_table)
        .expect("cyclic wrapper contexts should fall back without recursing");
    assert!(
        preparation.fallback_reason().is_some(),
        "cyclic wrapper-context applications must be semantic fallbacks"
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

    let reason = classify_view_native_fold_safety(&view, &store)
        .expect("fold safety authority should resolve");
    assert!(
        reason.is_none(),
        "IfChildEmits mode should be accepted by the fold-safety gate, got {:?}",
        reason
    );
}

fn handoff_fixture_result(
    fixture: &WrapperContextFixture,
    string_table: &mut StringTable,
) -> Result<OwnedRuntimeTemplateHandoff, CompilerError> {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let registry_borrow = fixture.registry.borrow();
    let parent_phase = fixture_parent_view_phase(&registry_borrow, fixture.overlay_set_id);
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.parent_template_id),
        parent_phase,
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
    handoff_fixture_result(fixture, string_table).expect("handoff should succeed")
}

fn assert_text_node(node: &OwnedRuntimeTemplateNode, expected: &str, string_table: &StringTable) {
    match node {
        OwnedRuntimeTemplateNode::Text { text, .. } => {
            assert_eq!(string_table.resolve(*text), expected);
        }
        other => panic!("expected Text node, got {:?}", other),
    }
}

fn assert_text_or_single_sequence_node(
    node: &OwnedRuntimeTemplateNode,
    expected: &str,
    string_table: &StringTable,
) {
    match node {
        OwnedRuntimeTemplateNode::Sequence { children } if children.len() == 1 => {
            assert_text_node(&children[0], expected, string_table)
        }
        _ => assert_text_node(node, expected, string_table),
    }
}

fn assert_text_body(body: &OwnedRuntimeTemplateBody, expected: &str, string_table: &StringTable) {
    match body {
        OwnedRuntimeTemplateBody::Render(node) => {
            assert_text_or_single_sequence_node(node, expected, string_table)
        }
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
        OwnedRuntimeTemplateNode::AggregateOutput
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
