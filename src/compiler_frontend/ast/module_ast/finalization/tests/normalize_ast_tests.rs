//! Tests for AST template normalization at the HIR boundary.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::{
    ExpressionKind, ReactiveSource, ReactiveSourceKind,
};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::{
    ReactiveSubscription, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateIr, TemplateIrBranch, TemplateIrBuilder, TemplateIrNode, TemplateIrNodeKind,
    TemplateIrRegistry, TemplateIrStore, TemplateIrSummary, TemplateLoopHeaderExpressionSites,
    TemplateNodeRef, TemplateRef, TemplateSlotPlan, TemplateStoreId, TemplateTirChildReference,
    TemplateTirPhase, TemplateTirReference, TemplateWrapperReference, TemplateWrapperSet,
    TemplateWrapperSetRef, TirView,
};
use crate::compiler_frontend::ast::templates::tir::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirSlotResolution,
    TirSlotResolutionOverlay, TirWrapperContext, TirWrapperContextOverlay,
};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeSlotSiteRenderPiece, OwnedRuntimeTemplateBody, OwnedRuntimeTemplateHandoff,
    OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::compiler_messages::DiagnosticPayload;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;

#[cfg(feature = "benchmark_counters")]
use crate::compiler_frontend::instrumentation::ast_counters::{
    reset_ast_counters, test_read_ast_counter,
};
use std::cell::RefCell;
use std::rc::Rc;

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

/// Builds a `Template` carrying a registered TIR root with a single text node,
/// matching the production shape parser-created const text templates carry
/// before finalization normalizes their enclosing payload.
fn registered_text_template(
    text: crate::compiler_frontend::symbols::string_interning::StringId,
    store_id: TemplateStoreId,
    overlay_set_id: TemplateOverlaySetId,
    template_ir_store: &Rc<RefCell<TemplateIrStore>>,
    string_table: &StringTable,
) -> Template {
    let byte_len = string_table.resolve(text).len() as u32;
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text_node = builder.push_text_node(
            text,
            byte_len,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(vec![text_node], SourceLocation::default());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };
    template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::String,
        SourceLocation::default(),
    )
}

/// Builds a nested wrapper-context graph whose wrapper references carry their
/// own exact overlay views. The unsafe variant places a runtime slot plan only
/// on the nested wrapper reached through the outer wrapper's overlay.
fn nested_wrapper_finalization_fixture(
    string_table: &mut StringTable,
    unsafe_nested_wrapper: bool,
) -> (
    Template,
    Rc<RefCell<TemplateIrStore>>,
    Rc<RefCell<TemplateIrRegistry>>,
) {
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (
        parent_template_id,
        parent_occurrence_id,
        outer_wrapper_set_id,
        nested_occurrence_id,
        outer_expression_site_id,
        inner_wrapper_set_id,
    ) = {
        let mut store = template_ir_store.borrow_mut();

        let child_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let text = string_table.intern("parent");
            let text_node = builder.push_text_node(
                text,
                "parent".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(vec![text_node], SourceLocation::default());
            builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            )
        };

        let nested_child_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let text = string_table.intern("nested");
            let text_node = builder.push_text_node(
                text,
                "nested".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(vec![text_node], SourceLocation::default());
            builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            )
        };

        let inner_wrapper_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let before = string_table.intern("inner-before");
            let after = string_table.intern("inner-after");
            let before_node = builder.push_text_node(
                before,
                "inner-before".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let slot_node = builder.push_slot_node(SlotKey::Default, SourceLocation::default());
            let after_node = builder.push_text_node(
                after,
                "inner-after".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(
                vec![before_node, slot_node, after_node],
                SourceLocation::default(),
            );
            builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            )
        };

        if unsafe_nested_wrapper {
            let runtime_slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
                location: SourceLocation::default(),
                contribution_sources: Vec::new(),
                slot_sites: Vec::new(),
            });
            store.templates[inner_wrapper_template_id.index()].runtime_slot_plan =
                Some(runtime_slot_plan_id);
        }

        let inner_wrapper_reference = TemplateWrapperReference::new(
            store.qualify_template_ref(inner_wrapper_template_id),
            TemplateTirPhase::Finalized,
            empty_overlay_set_id,
        );
        let inner_wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![inner_wrapper_reference],
        });

        let (outer_wrapper_template_id, nested_child_node, outer_dynamic_node) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let outer_dynamic_text = string_table.intern("outer-structural");
            let outer_dynamic_node = builder.push_dynamic_expression_node(
                Expression::string_slice(
                    outer_dynamic_text,
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
                None,
                SourceLocation::default(),
            );
            let nested_child_node = builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    nested_child_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
                ),
                SourceLocation::default(),
            );
            let slot_node = builder.push_slot_node(SlotKey::Default, SourceLocation::default());
            let after = string_table.intern("outer-after");
            let after_node = builder.push_text_node(
                after,
                "outer-after".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(
                vec![outer_dynamic_node, nested_child_node, slot_node, after_node],
                SourceLocation::default(),
            );
            let template_id = builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            (template_id, nested_child_node, outer_dynamic_node)
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
        let outer_wrapper_set_id = store.push_wrapper_set(TemplateWrapperSet {
            wrappers: vec![outer_wrapper_reference],
        });

        let parent_child_node = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    child_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
                ),
                SourceLocation::default(),
            )
        };
        let parent_occurrence_id = match &store
            .get_node(parent_child_node)
            .expect("parent child node should exist")
            .kind
        {
            TemplateIrNodeKind::ChildTemplate { occurrence_id, .. } => *occurrence_id,
            _ => panic!("expected parent child-template node"),
        };
        let parent_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let root =
                builder.push_sequence_node(vec![parent_child_node], SourceLocation::default());
            builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            )
        };

        (
            parent_template_id,
            parent_occurrence_id,
            outer_wrapper_set_id,
            nested_occurrence_id,
            outer_expression_site_id,
            inner_wrapper_set_id,
        )
    };

    let nested_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
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
    let outer_expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            outer_expression_site_id,
            Box::new(Expression::string_slice(
                string_table.intern("outer-overlay"),
                SourceLocation::default(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let outer_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(outer_expression_overlay_id),
        slot_resolution: None,
        wrapper_context: Some(nested_context_overlay_id),
    });
    template_ir_store.borrow_mut().wrapper_sets[outer_wrapper_set_id.index()].wrappers[0]
        .overlay_set_id = outer_overlay_set_id;

    let parent_context_overlay_id =
        registry.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
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
    let parent_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(parent_context_overlay_id),
    });
    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, parent_template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Finalized,
            overlay_set_id: parent_overlay_set_id,
        },
        TemplateType::String,
        SourceLocation::default(),
    );

    (template, template_ir_store, Rc::new(RefCell::new(registry)))
}

fn location_at(line: i32, column: i32) -> SourceLocation {
    use crate::compiler_frontend::compiler_messages::source_location::CharPosition;

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

fn assert_expression_site_location(
    view: &TirView<'_>,
    site_id: ExpressionSiteId,
    line: i32,
    column: i32,
) {
    let location = view
        .source_location_for_expression_site(site_id)
        .expect("source-location lookup should succeed")
        .expect("source location should be present");

    assert_eq!(location.start_pos.line_number, line);
    assert_eq!(location.start_pos.char_column, column);
}

#[test]
fn finalization_fold_composed_tir_root_folds_view_text() {
    let mut string_table = StringTable::new();
    let view_text = string_table.intern("registry-backed view");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let template = registered_text_template(
        view_text,
        store_id,
        overlay_set_id,
        &template_ir_store,
        &string_table,
    );

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("composed TIR root fold should succeed")
    .folded
    .expect("composed template should fold");

    assert_eq!(
        folded, view_text,
        "finalization should fold the composed TIR view text"
    );
}

#[test]
fn finalization_normalizes_dynamic_expression_payloads_into_expression_overlay() {
    let mut string_table = StringTable::new();
    let normalized_text = string_table.intern("normalized dynamic payload");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let registry = Rc::new(RefCell::new(registry));

    let dynamic_expression = Expression::template(
        registered_text_template(
            normalized_text,
            store_id,
            overlay_set_id,
            &template_ir_store,
            &string_table,
        ),
        ValueMode::ImmutableOwned,
    );
    let expression_location = location_at(31, 7);
    let (template_id, dynamic_node_id, site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let (template_id, dynamic_node_id) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let dynamic_node_id = builder.push_dynamic_expression_node(
                dynamic_expression,
                TemplateSegmentOrigin::Body,
                None,
                expression_location.clone(),
            );
            let template_id = builder.finish_template(
                dynamic_node_id,
                Style::default(),
                TemplateType::StringFunction,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            (template_id, dynamic_node_id)
        };

        let site_id = match &store
            .get_node(dynamic_node_id)
            .expect("dynamic node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            other => panic!("expected dynamic expression node, got {other:?}"),
        };

        (template_id, dynamic_node_id, site_id)
    };

    let mut template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::clone(&registry),
    };

    normalize_template_for_hir(&mut template, &mut context)
        .expect("template normalization should install the dynamic expression overlay");

    let reference = &template.tir_reference;
    assert_ne!(
        reference.overlay_set_id, overlay_set_id,
        "normalization should update the template reference to the expression-overlay set"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Finalized,
        "normalization should advance the effective reference to the finalized phase"
    );

    let registry = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry,
        reference.root,
        reference.phase,
        TemplateTirPhase::Finalized,
        reference.overlay_set_id,
    )
    .expect("updated template reference should build a finalized TirView");

    let expression_by_site = view
        .effective_expression_for_site(site_id)
        .expect("site lookup should be valid")
        .expect("normalized dynamic expression should be visible by site");
    assert!(
        matches!(expression_by_site.kind, ExpressionKind::StringSlice(text) if text == normalized_text)
    );
    assert_expression_site_location(&view, site_id, 31, 7);

    let expression_by_node = view
        .effective_expression_for_node(TemplateNodeRef::new(store_id, dynamic_node_id))
        .expect("node lookup should be valid")
        .expect("normalized dynamic expression should be visible by node");
    assert!(
        matches!(expression_by_node.kind, ExpressionKind::StringSlice(text) if text == normalized_text)
    );

    let structural_expression_is_unchanged = {
        let store = template_ir_store.borrow();
        let node = store
            .get_node(dynamic_node_id)
            .expect("dynamic node should remain in the structural store");
        matches!(
            &node.kind,
            TemplateIrNodeKind::DynamicExpression { expression, .. }
                if matches!(expression.kind, ExpressionKind::Template(_))
        )
    };
    assert!(
        structural_expression_is_unchanged,
        "Phase 10 dynamic-expression normalization should layer the normalized payload through an overlay"
    );
}

#[test]
fn finalization_does_not_mark_parsed_expression_overlay_reference_finalized() {
    let mut string_table = StringTable::new();
    let normalized_text = string_table.intern("normalized parsed dynamic payload");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let registry = Rc::new(RefCell::new(registry));

    let dynamic_expression = Expression::template(
        registered_text_template(
            normalized_text,
            store_id,
            overlay_set_id,
            &template_ir_store,
            &string_table,
        ),
        ValueMode::ImmutableOwned,
    );
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let dynamic_node_id = builder.push_dynamic_expression_node(
            dynamic_expression,
            TemplateSegmentOrigin::Body,
            None,
            SourceLocation::default(),
        );
        builder.finish_template(
            dynamic_node_id,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };

    let mut template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Parsed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::clone(&registry),
    };

    normalize_template_for_hir(&mut template, &mut context)
        .expect("template normalization should preserve parsed reference identity");

    let reference = &template.tir_reference;
    assert_ne!(
        reference.overlay_set_id, overlay_set_id,
        "parsed references may receive expression overlays without becoming finalized views"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Parsed,
        "parsed references are not stable finalization views and must keep their parsed phase"
    );
}

#[test]
fn finalization_normalizes_branch_selector_payloads_into_expression_overlay() {
    let mut string_table = StringTable::new();
    let normalized_text = string_table.intern("normalized branch selector payload");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let registry = Rc::new(RefCell::new(registry));

    let selector_expression = Expression::template(
        registered_text_template(
            normalized_text,
            store_id,
            overlay_set_id,
            &template_ir_store,
            &string_table,
        ),
        ValueMode::ImmutableOwned,
    );
    let selector_location = location_at(41, 9);
    let (template_id, branch_chain_node_id, selector_site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let branch_body = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children: vec![] },
            SourceLocation::default(),
        ));
        let selector_site_id = store.next_expression_site_id();
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(selector_expression),
            branch_body,
            selector_location.clone(),
        )
        .with_selector_site_id(selector_site_id);
        let branch_chain_node_id = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::BranchChain {
                branches: vec![branch],
                fallback: None,
            },
            SourceLocation::default(),
        ));
        let template_id = store.push_template(TemplateIr::new(
            branch_chain_node_id,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        ));

        (template_id, branch_chain_node_id, selector_site_id)
    };

    let mut template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::clone(&registry),
    };

    normalize_template_for_hir(&mut template, &mut context)
        .expect("template normalization should install the branch selector overlay");

    let reference = &template.tir_reference;
    assert_ne!(
        reference.overlay_set_id, overlay_set_id,
        "normalization should update the template reference to the expression-overlay set"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Finalized,
        "normalization should advance the effective reference to the finalized phase"
    );

    let registry = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry,
        reference.root,
        reference.phase,
        TemplateTirPhase::Finalized,
        reference.overlay_set_id,
    )
    .expect("updated template reference should build a finalized TirView");

    let expression_by_site = view
        .effective_expression_for_site(selector_site_id)
        .expect("site lookup should be valid")
        .expect("normalized branch selector should be visible by site");
    assert!(
        matches!(expression_by_site.kind, ExpressionKind::StringSlice(text) if text == normalized_text)
    );
    assert_expression_site_location(&view, selector_site_id, 41, 9);

    let structural_selector_is_unchanged = {
        let store = template_ir_store.borrow();
        let node = store
            .get_node(branch_chain_node_id)
            .expect("branch chain node should remain in the structural store");
        matches!(
            &node.kind,
            TemplateIrNodeKind::BranchChain { branches, .. }
                if matches!(branches[0].condition_expression().kind, ExpressionKind::Template(_))
        )
    };
    assert!(
        structural_selector_is_unchanged,
        "Phase 10 branch-selector normalization should layer the normalized payload through an overlay"
    );
}

#[test]
fn finalization_normalizes_loop_header_payloads_into_expression_overlay() {
    let mut string_table = StringTable::new();
    let normalized_text = string_table.intern("normalized loop header payload");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let registry = Rc::new(RefCell::new(registry));

    let header_expression = Expression::template(
        registered_text_template(
            normalized_text,
            store_id,
            overlay_set_id,
            &template_ir_store,
            &string_table,
        ),
        ValueMode::ImmutableOwned,
    );
    let loop_location = location_at(51, 11);
    let (template_id, loop_node_id, condition_site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let loop_body = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence { children: vec![] },
            SourceLocation::default(),
        ));
        let header = TemplateLoopHeader::Conditional {
            condition: Box::new(header_expression),
        };
        let header_sites = store.allocate_loop_header_expression_sites(&header);
        let condition_site_id = match header_sites {
            TemplateLoopHeaderExpressionSites::Conditional { condition } => condition,
            _ => panic!("expected conditional loop header sites"),
        };
        let loop_node_id = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Loop {
                header,
                header_sites,
                body: loop_body,
                aggregate_wrapper: None,
            },
            loop_location.clone(),
        ));
        let template_id = store.push_template(TemplateIr::new(
            loop_node_id,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        ));

        (template_id, loop_node_id, condition_site_id)
    };

    let mut template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::clone(&registry),
    };

    normalize_template_for_hir(&mut template, &mut context)
        .expect("template normalization should install the loop header overlay");

    let reference = &template.tir_reference;
    assert_ne!(
        reference.overlay_set_id, overlay_set_id,
        "normalization should update the template reference to the expression-overlay set"
    );
    assert_eq!(
        reference.phase,
        TemplateTirPhase::Finalized,
        "normalization should advance the effective reference to the finalized phase"
    );

    let registry = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry,
        reference.root,
        reference.phase,
        TemplateTirPhase::Finalized,
        reference.overlay_set_id,
    )
    .expect("updated template reference should build a finalized TirView");

    let expression_by_site = view
        .effective_expression_for_site(condition_site_id)
        .expect("site lookup should be valid")
        .expect("normalized loop header expression should be visible by site");
    assert!(
        matches!(expression_by_site.kind, ExpressionKind::StringSlice(text) if text == normalized_text)
    );
    assert_expression_site_location(&view, condition_site_id, 51, 11);

    let structural_header_is_unchanged = {
        let store = template_ir_store.borrow();
        let node = store
            .get_node(loop_node_id)
            .expect("loop node should remain in the structural store");
        matches!(
            &node.kind,
            TemplateIrNodeKind::Loop {
                header: TemplateLoopHeader::Conditional { condition },
                ..
            } if matches!(condition.kind, ExpressionKind::Template(_))
        )
    };
    assert!(
        structural_header_is_unchanged,
        "Phase 10 loop-header normalization should layer the normalized payload through an overlay"
    );
}

#[test]
fn finalization_fold_uses_finalized_expression_overlay_view() {
    #[cfg(feature = "benchmark_counters")]
    let _guard = crate::compiler_frontend::instrumentation::lock_counter_test();

    let mut string_table = StringTable::new();
    let structural_text = string_table.intern("structural dynamic payload");
    let overlay_text = string_table.intern("finalized expression overlay");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (template_id, dynamic_node) = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let dynamic_node = builder.push_dynamic_expression_node(
            Expression::string_slice(
                structural_text,
                SourceLocation::default(),
                ValueMode::ImmutableOwned,
            ),
            TemplateSegmentOrigin::Body,
            None,
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(vec![dynamic_node], SourceLocation::default());
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        );

        (template_id, dynamic_node)
    };

    let site_id = {
        let store = template_ir_store.borrow();
        match &store
            .get_node(dynamic_node)
            .expect("dynamic node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            _ => panic!("expected dynamic expression node"),
        }
    };

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            site_id,
            Box::new(Expression::string_slice(
                overlay_text,
                SourceLocation::default(),
                ValueMode::ImmutableOwned,
            )),
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

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Finalized,
            overlay_set_id,
        },
        TemplateType::String,
        SourceLocation::default(),
    );

    #[cfg(feature = "benchmark_counters")]
    reset_ast_counters();

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("expression-overlay view fold should succeed")
    .folded
    .expect("finalized expression-overlay view should fold");

    assert_eq!(
        folded, overlay_text,
        "finalized expression overlays must fold from the same effective TirView instead of the structural payload"
    );

    #[cfg(feature = "benchmark_counters")]
    assert_eq!(
        test_read_ast_counter(AstCounter::TirStoreCloneFinalization),
        0,
        "finalized expression-overlay folding must borrow the live store instead of cloning it"
    );
}

#[test]
fn finalization_classifies_root_expression_overlay_through_nested_same_store_children() {
    let mut string_table = StringTable::new();
    let dynamic_text = string_table.intern("root-overlay-dynamic");
    let branch_text = string_table.intern("root-overlay-branch");
    let loop_text = string_table.intern("root-overlay-loop");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (root_template_id, dynamic_site_id, selector_site_id, loop_site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let (leaf_template_id, dynamic_node, branch_node, loop_node) = {
            let mut builder = TemplateIrBuilder::new(&mut store);

            let dynamic_node = builder.push_dynamic_expression_node(
                Expression::reference_with_type_id(
                    InternedPath::from_single_str("nested_dynamic", &mut string_table),
                    DataType::StringSlice,
                    builtin_type_ids::STRING,
                    SourceLocation::default(),
                    ValueMode::ImmutableReference,
                    ConstRecordState::RuntimeValue,
                ),
                TemplateSegmentOrigin::Body,
                None,
                SourceLocation::default(),
            );
            let branch_text_node = builder.push_text_node(
                branch_text,
                "root-overlay-branch".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let branch_node = builder.push_branch_chain_node(
                vec![TemplateIrBranch::new(
                    TemplateBranchSelector::Bool(Expression::reference_with_type_id(
                        InternedPath::from_single_str("nested_selector", &mut string_table),
                        DataType::Bool,
                        builtin_type_ids::BOOL,
                        SourceLocation::default(),
                        ValueMode::ImmutableReference,
                        ConstRecordState::RuntimeValue,
                    )),
                    branch_text_node,
                    SourceLocation::default(),
                )],
                None,
                SourceLocation::default(),
            );
            let loop_text_node = builder.push_text_node(
                loop_text,
                "root-overlay-loop".len() as u32,
                TemplateSegmentOrigin::Body,
                SourceLocation::default(),
            );
            let loop_node = builder.push_loop_node(
                TemplateLoopHeader::Conditional {
                    condition: Box::new(Expression::reference_with_type_id(
                        InternedPath::from_single_str("nested_loop", &mut string_table),
                        DataType::Bool,
                        builtin_type_ids::BOOL,
                        SourceLocation::default(),
                        ValueMode::ImmutableReference,
                        ConstRecordState::RuntimeValue,
                    )),
                },
                loop_text_node,
                None,
                SourceLocation::default(),
            );
            let leaf_root = builder.push_sequence_node(
                vec![dynamic_node, branch_node, loop_node],
                SourceLocation::default(),
            );
            let leaf_template_id = builder.finish_template(
                leaf_root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            (leaf_template_id, dynamic_node, branch_node, loop_node)
        };

        let dynamic_site_id = match &store
            .get_node(dynamic_node)
            .expect("dynamic node should exist")
            .kind
        {
            TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
            _ => panic!("expected a dynamic-expression node"),
        };
        let (selector_site_id, loop_site_id) = match &store
            .get_node(branch_node)
            .expect("branch node should exist")
            .kind
        {
            TemplateIrNodeKind::BranchChain { branches, .. } => {
                let selector_site_id = branches[0].selector_site_id;
                let loop_site_id = match &store
                    .get_node(loop_node)
                    .expect("loop node should exist")
                    .kind
                {
                    TemplateIrNodeKind::Loop {
                        header_sites: TemplateLoopHeaderExpressionSites::Conditional { condition },
                        ..
                    } => *condition,
                    _ => panic!("expected a conditional loop node"),
                };
                (selector_site_id, loop_site_id)
            }
            _ => panic!("expected a branch-chain node"),
        };

        let mut builder = TemplateIrBuilder::new(&mut store);
        let mut descendant_template_id = leaf_template_id;
        for _ in 0..3 {
            let child_reference = TemplateTirChildReference::same_store(
                descendant_template_id,
                store_id,
                TemplateTirPhase::Composed,
                empty_overlay_set_id,
            );
            let child_node = builder.push_child_template_node_with_reference(
                child_reference,
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(vec![child_node], SourceLocation::default());
            descendant_template_id = builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
        }

        (
            descendant_template_id,
            dynamic_site_id,
            selector_site_id,
            loop_site_id,
        )
    };

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![
            (
                dynamic_site_id,
                Box::new(Expression::string_slice(
                    dynamic_text,
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                )),
            ),
            (
                selector_site_id,
                Box::new(Expression::bool(
                    true,
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                )),
            ),
            (
                loop_site_id,
                Box::new(Expression::bool(
                    false,
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                )),
            ),
        ],
    });
    let root_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, root_template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Finalized,
            overlay_set_id: root_overlay_set_id,
        },
        TemplateType::String,
        SourceLocation::default(),
    );

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("root overlay should classify and fold through nested descendants")
    .folded
    .expect("root overlay should produce a folded string");

    assert_eq!(
        string_table.resolve(folded),
        "root-overlay-dynamicroot-overlay-branch",
        "dynamic, branch-selector, and loop-header overlays must all reach the nested leaf"
    );
}

#[test]
fn finalization_ignores_parsed_child_overlay_before_later_composed_descendant() {
    let mut string_table = StringTable::new();
    let structural_text = string_table.intern("structural");
    let override_text = string_table.intern("root-override");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let missing_overlay_set_id = TemplateOverlaySetId::new(999);

    let (root_template_id, descendant_site_id) = {
        let mut store = template_ir_store.borrow_mut();
        let (descendant_template_id, descendant_site_id) = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let dynamic_node = builder.push_dynamic_expression_node(
                Expression::string_slice(
                    structural_text,
                    SourceLocation::default(),
                    ValueMode::ImmutableOwned,
                ),
                TemplateSegmentOrigin::Body,
                None,
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(vec![dynamic_node], SourceLocation::default());
            let template_id = builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            );
            let site_id = match &store
                .get_node(dynamic_node)
                .expect("descendant dynamic node should exist")
                .kind
            {
                TemplateIrNodeKind::DynamicExpression { site_id, .. } => *site_id,
                _ => panic!("expected descendant dynamic-expression node"),
            };
            (template_id, site_id)
        };

        let parsed_child_template_id = {
            let mut builder = TemplateIrBuilder::new(&mut store);
            let child_node = builder.push_child_template_node_with_reference(
                TemplateTirChildReference::same_store(
                    descendant_template_id,
                    store_id,
                    TemplateTirPhase::Composed,
                    empty_overlay_set_id,
                ),
                SourceLocation::default(),
            );
            let root = builder.push_sequence_node(vec![child_node], SourceLocation::default());
            builder.finish_template(
                root,
                Style::default(),
                TemplateType::String,
                TemplateIrSummary::default(),
                SourceLocation::default(),
            )
        };

        let mut builder = TemplateIrBuilder::new(&mut store);
        let child_node = builder.push_child_template_node_with_reference(
            TemplateTirChildReference::same_store(
                parsed_child_template_id,
                store_id,
                TemplateTirPhase::Parsed,
                missing_overlay_set_id,
            ),
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(vec![child_node], SourceLocation::default());
        let root_template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        );

        (root_template_id, descendant_site_id)
    };

    let expression_overlay_id = registry.allocate_expression_overlay(TirExpressionOverlay {
        overrides: vec![(
            descendant_site_id,
            Box::new(Expression::string_slice(
                override_text,
                SourceLocation::default(),
                ValueMode::ImmutableOwned,
            )),
        )],
    });
    let root_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    });
    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, root_template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Finalized,
            overlay_set_id: root_overlay_set_id,
        },
        TemplateType::String,
        SourceLocation::default(),
    );

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("a Parsed child must not consume its missing overlay during finalization")
    .folded
    .expect("the composed descendant should remain foldable");

    assert_eq!(
        folded, override_text,
        "the finalized root expression overlay must reach the later composed descendant"
    );
}

#[test]
fn finalization_rejects_nested_runtime_wrapper_in_exact_wrapper_overlay() {
    let mut string_table = StringTable::new();
    let (template, template_ir_store, template_ir_registry) =
        nested_wrapper_finalization_fixture(&mut string_table, true);
    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();

    let result = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry,
        },
    )
    .expect("runtime nested wrapper should be a valid non-foldable shape");

    assert!(
        result.folded.is_none(),
        "the production safety gate must not fold through a runtime nested wrapper hidden in the exact wrapper overlay"
    );
}

#[test]
fn finalization_keeps_valid_runtime_slot_plan_out_of_folded_string() {
    let mut string_table = StringTable::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let text = string_table.intern("runtime root");
    let template = registered_text_template(
        text,
        store_id,
        overlay_set_id,
        &template_ir_store,
        &string_table,
    );
    let template_id = template.tir_reference.root.template_id;

    {
        let mut store = template_ir_store.borrow_mut();
        let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
            location: SourceLocation::default(),
            contribution_sources: Vec::new(),
            slot_sites: Vec::new(),
        });
        store.templates[template_id.index()].runtime_slot_plan = Some(slot_plan_id);
    }

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let result = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("valid runtime slot plan should use the handoff path");

    assert!(
        result.folded.is_none(),
        "a valid runtime slot plan must not become a folded empty string"
    );
    assert!(
        template_ir_store.borrow().templates[template_id.index()]
            .runtime_slot_plan
            .is_some(),
        "the runtime slot plan must remain available for owned handoff"
    );
}

#[test]
fn finalization_replaces_renderable_runtime_slot_plan_with_owned_handoff() {
    let mut string_table = StringTable::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let text = string_table.intern("runtime handoff");
    let template = registered_text_template(
        text,
        store_id,
        overlay_set_id,
        &template_ir_store,
        &string_table,
    );
    let template_id = template.tir_reference.root.template_id;

    {
        let mut store = template_ir_store.borrow_mut();
        let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
            location: SourceLocation::default(),
            contribution_sources: Vec::new(),
            slot_sites: Vec::new(),
        });
        store.templates[template_id.index()].runtime_slot_plan = Some(slot_plan_id);
    }

    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);
    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_registry = Rc::new(RefCell::new(registry));
    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry,
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("renderable runtime slot plans should use the owned handoff path");

    let ExpressionKind::RuntimeSlotApplicationHandoff(handoff) = expression.kind else {
        panic!("expected renderable runtime slot plan to become an owned slot handoff");
    };
    assert!(
        handoff.slot_sites.is_empty(),
        "the owned handoff must retain the valid empty slot plan"
    );
    assert!(
        template_ir_store.borrow().templates[template_id.index()]
            .runtime_slot_plan
            .is_some(),
        "normalization must retain the source runtime slot plan"
    );
}

#[test]
fn module_constant_normalization_rejects_runtime_slot_plan_with_structured_diagnostic() {
    let mut string_table = StringTable::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let text = string_table.intern("module constant runtime plan");
    let template = registered_text_template(
        text,
        store_id,
        overlay_set_id,
        &template_ir_store,
        &string_table,
    );
    let template_id = template.tir_reference.root.template_id;

    {
        let mut store = template_ir_store.borrow_mut();
        let slot_plan_id = store.push_slot_plan(TemplateSlotPlan {
            location: SourceLocation::default(),
            contribution_sources: Vec::new(),
            slot_sites: Vec::new(),
        });
        store.templates[template_id.index()].runtime_slot_plan = Some(slot_plan_id);
    }

    let expression = Expression::template(template, ValueMode::ImmutableOwned);
    let ExpressionKind::Template(template) = &expression.kind else {
        panic!("module constant regression must start from a template expression");
    };
    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let result = super::super::normalize_constants::normalize_module_constant_template_expression(
        &expression,
        template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    );

    let TemplateNormalizationError::Diagnostic(diagnostic) =
        result.expect_err("runtime-plan module constants must be rejected structurally")
    else {
        panic!(
            "runtime-plan module constants must not report the old internal fold transformation error"
        );
    };
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidTemplateStructure {
            reason: InvalidTemplateStructureReason::NonFoldableConstTemplate,
        }
    ));
    assert_eq!(
        diagnostic.primary_location, expression.location,
        "the established const diagnostic must retain the template source location"
    );
}

#[test]
fn finalization_accepts_supported_nested_wrapper_exact_view() {
    let mut string_table = StringTable::new();
    let (template, template_ir_store, template_ir_registry) =
        nested_wrapper_finalization_fixture(&mut string_table, false);
    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry,
        },
    )
    .expect("supported nested wrapper should fold through the exact views")
    .folded
    .expect("supported nested wrapper should produce const output");

    assert_eq!(
        string_table.resolve(folded),
        "outer-overlayinner-beforenestedinner-afterparentouter-after",
        "supported exact-view wrapper traversal must preserve fold output and wrapper order"
    );
}

#[test]
fn finalization_fold_uses_resolved_slot_overlay_set() {
    let mut string_table = StringTable::new();
    let before_text = string_table.intern("before");
    let after_text = string_table.intern("after");
    let fill_text = string_table.intern("filled");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));

    let reference = {
        let mut store = template_ir_store.borrow_mut();
        let mut fill_builder = TemplateIrBuilder::new(&mut store);
        let fill_node = fill_builder.push_text_node(
            fill_text,
            "filled".len() as u32,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let fill_root = fill_builder.push_sequence_node(vec![fill_node], SourceLocation::default());
        let fill_template_id = fill_builder.finish_template(
            fill_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        );

        let mut wrapper_builder = TemplateIrBuilder::new(&mut store);
        let before_node = wrapper_builder.push_text_node(
            before_text,
            "before".len() as u32,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let slot_node = wrapper_builder.push_slot_node(SlotKey::Default, SourceLocation::default());
        let after_node = wrapper_builder.push_text_node(
            after_text,
            "after".len() as u32,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let wrapper_root = wrapper_builder.push_sequence_node(
            vec![before_node, slot_node, after_node],
            SourceLocation::default(),
        );
        let wrapper_template_id = wrapper_builder.finish_template(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        );

        let slot_occurrence_id = match &store
            .get_node(slot_node)
            .expect("slot node should exist")
            .kind
        {
            TemplateIrNodeKind::Slot { placeholder } => placeholder.occurrence_id,
            _ => panic!("expected slot node"),
        };

        let slot_overlay_id = registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
            resolutions: vec![(
                slot_occurrence_id,
                TirSlotResolution::resolved(
                    SlotKey::Default,
                    vec![TemplateRef::new(store_id, fill_template_id)],
                ),
            )],
        });
        let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: Some(slot_overlay_id),
            wrapper_context: None,
        });
        assert!(
            !registry
                .overlay_set(overlay_set_id)
                .expect("overlay set should exist")
                .is_empty(),
            "test must exercise a real non-empty slot overlay set"
        );

        TemplateTirReference {
            root: TemplateRef::new(store_id, wrapper_template_id),
            store_owner: store.owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        }
    };

    let template =
        template_with_reference(reference, TemplateType::String, SourceLocation::default());

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("resolved slot-overlay fold should succeed")
    .folded
    .expect("resolved slot-overlay view should fold");

    let expected = string_table.intern("beforefilledafter");
    assert_eq!(
        folded, expected,
        "composed slot overlays must fold from the effective TirView"
    );
}

#[test]
fn finalization_fold_composed_root_with_unfilled_slot_emits_no_slot_output() {
    let mut string_table = StringTable::new();
    let text_id = string_table.intern("text before unfilled slot");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // An unfilled slot contributes no output. Finalization folds that rule
    // directly from the composed TIR root.
    let reference = {
        let location = SourceLocation::default();
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text_node = builder.push_text_node(
            text_id,
            "text before unfilled slot".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let slot_node = builder.push_slot_node(SlotKey::Default, location.clone());
        let root = builder.push_sequence_node(vec![text_node, slot_node], location.clone());
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            location,
        );

        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store.owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        }
    };

    let template =
        template_with_reference(reference, TemplateType::String, SourceLocation::default());

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("composed slot-root fold should succeed")
    .folded
    .expect("unfilled slot template should fold");

    assert_eq!(
        folded, text_id,
        "the unfilled slot must contribute no output to the composed TIR root"
    );
}

#[test]
fn finalization_fold_formatted_root_with_unfilled_slot_emits_no_slot_output() {
    #[cfg(feature = "benchmark_counters")]
    let _guard = crate::compiler_frontend::instrumentation::lock_counter_test();

    let mut string_table = StringTable::new();
    let text_id = string_table.intern("formatted text before unfilled slot");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let reference = {
        let location = SourceLocation::default();
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text_node = builder.push_text_node(
            text_id,
            "formatted text before unfilled slot".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let slot_node = builder.push_slot_node(SlotKey::Default, location.clone());
        let root = builder.push_sequence_node(vec![text_node, slot_node], location.clone());
        let template_id = builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary {
                has_slots: true,
                slot_count: 1,
                ..TemplateIrSummary::default()
            },
            location,
        );

        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: store.owner(),
            phase: TemplateTirPhase::Formatted,
            overlay_set_id,
        }
    };

    let template =
        template_with_reference(reference, TemplateType::String, SourceLocation::default());

    #[cfg(feature = "benchmark_counters")]
    reset_ast_counters();

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("formatted slot-root fold should succeed")
    .folded
    .expect("unfilled formatted slot template should fold");

    assert_eq!(
        folded, text_id,
        "the unfilled slot must contribute no output to the formatted TIR root"
    );

    #[cfg(feature = "benchmark_counters")]
    {
        assert_eq!(
            test_read_ast_counter(AstCounter::TirRegistryBackedFoldAttempts),
            1,
            "slot-bearing formatted roots are now real registry fold attempts"
        );
        assert_eq!(
            test_read_ast_counter(AstCounter::TirReadOnlyFoldAttempts),
            1,
            "read-only fold safety is attempted before view-native overlay classification"
        );
        assert_eq!(
            test_read_ast_counter(AstCounter::TirReadOnlyFoldFallbacks),
            1,
            "slot nodes reject read-only fold safety"
        );
        assert_eq!(
            test_read_ast_counter(AstCounter::TirRegistryBackedFoldSuccesses),
            1,
            "the registry-backed fold completes directly"
        );
    }
}

fn runtime_template_handoff_from_expression(expression: Expression) -> OwnedRuntimeTemplateHandoff {
    let ExpressionKind::RuntimeTemplateHandoff(handoff) = expression.kind else {
        panic!("expected expression normalization to return an owned runtime-template handoff");
    };

    *handoff
}

#[test]
fn branch_tir_root_normalizes_into_owned_runtime_handoff() {
    let mut string_table = StringTable::new();
    let location = SourceLocation::default();
    let branch_text = string_table.intern("branch body");
    let fallback_text = string_table.intern("fallback body");
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let branch_body = builder.push_text_node(
            branch_text,
            "branch body".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let fallback_body = builder.push_text_node(
            fallback_text,
            "fallback body".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(Expression::reference_with_type_id(
                InternedPath::from_single_str("show_branch", &mut string_table),
                DataType::Bool,
                builtin_type_ids::BOOL,
                location.clone(),
                ValueMode::ImmutableReference,
                ConstRecordState::RuntimeValue,
            )),
            branch_body,
            location.clone(),
        );
        let root =
            builder.push_branch_chain_node(vec![branch], Some(fallback_body), location.clone());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            location,
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );

    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);
    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("branch TIR root should normalize through the finalized effective view");

    let handoff = runtime_template_handoff_from_expression(expression);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::BranchChain {
        branches,
        fallback,
        ..
    }) = handoff.body
    else {
        panic!("expected a branch-chain runtime handoff");
    };
    assert_eq!(branches.len(), 1);
    assert!(
        fallback.is_some(),
        "the fallback must remain owned by the handoff"
    );
    assert!(matches!(
        branches[0].selector,
        TemplateBranchSelector::Bool(Expression {
            kind: ExpressionKind::Reference(_),
            ..
        })
    ));
    assert!(matches!(
        branches[0].body,
        OwnedRuntimeTemplateNode::Text { text, .. } if text == branch_text
    ));
}

#[test]
fn loop_tir_root_normalizes_into_owned_runtime_handoff() {
    let mut string_table = StringTable::new();
    let location = SourceLocation::default();
    let loop_text = string_table.intern("loop body");
    let open_text = string_table.intern("[");
    let close_text = string_table.intern("]");
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let aggregate_output = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::AggregateOutput,
            location.clone(),
        ));
        let mut builder = TemplateIrBuilder::new(&mut store);
        let body = builder.push_text_node(
            loop_text,
            "loop body".len() as u32,
            TemplateSegmentOrigin::Body,
            location.clone(),
        );
        let open =
            builder.push_text_node(open_text, 1, TemplateSegmentOrigin::Body, location.clone());
        let close =
            builder.push_text_node(close_text, 1, TemplateSegmentOrigin::Body, location.clone());
        let aggregate_wrapper =
            builder.push_sequence_node(vec![open, aggregate_output, close], location.clone());
        let header = TemplateLoopHeader::Conditional {
            condition: Box::new(Expression::reference_with_type_id(
                InternedPath::from_single_str("keep_looping", &mut string_table),
                DataType::Bool,
                builtin_type_ids::BOOL,
                location.clone(),
                ValueMode::ImmutableReference,
                ConstRecordState::RuntimeValue,
            )),
        };
        let root = builder.push_loop_node(header, body, Some(aggregate_wrapper), location.clone());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            location,
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );

    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);
    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("loop TIR root should normalize through the finalized effective view");

    let handoff = runtime_template_handoff_from_expression(expression);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Loop {
        header,
        body,
        aggregate_wrapper,
        ..
    }) = handoff.body
    else {
        panic!("expected a loop runtime handoff");
    };
    assert!(matches!(
        header,
        TemplateLoopHeader::Conditional { condition }
            if matches!(condition.kind, ExpressionKind::Reference(_))
    ));
    assert!(matches!(
        body.as_ref(),
        OwnedRuntimeTemplateNode::Text { text, .. } if *text == loop_text
    ));
    let Some(aggregate_wrapper) = aggregate_wrapper else {
        panic!("expected the loop aggregate wrapper in the handoff");
    };
    let OwnedRuntimeTemplateNode::Sequence { children, .. } = *aggregate_wrapper else {
        panic!("expected the loop aggregate wrapper sequence in the handoff");
    };
    assert!(
        children
            .iter()
            .any(|child| matches!(child, OwnedRuntimeTemplateNode::AggregateOutput))
    );
}

fn collect_owned_handoff_string_slice_expressions(
    handoff: &OwnedRuntimeTemplateHandoff,
    string_slices: &mut Vec<crate::compiler_frontend::symbols::string_interning::StringId>,
) {
    match &handoff.body {
        OwnedRuntimeTemplateBody::Render(root) => {
            collect_owned_node_string_slice_expressions(root, string_slices);
        }

        OwnedRuntimeTemplateBody::RuntimeSlotApplication(slot_handoff) => {
            collect_owned_node_string_slice_expressions(&slot_handoff.wrapper, string_slices);
            for source in &slot_handoff.contribution_sources {
                collect_owned_node_string_slice_expressions(&source.render_root, string_slices);
            }
            for site in &slot_handoff.slot_sites {
                for piece in &site.render_plan.pieces {
                    if let OwnedRuntimeSlotSiteRenderPiece::Render(node) = piece {
                        collect_owned_node_string_slice_expressions(node, string_slices);
                    }
                }
            }
        }
    }
}

fn collect_owned_node_string_slice_expressions(
    node: &OwnedRuntimeTemplateNode,
    string_slices: &mut Vec<crate::compiler_frontend::symbols::string_interning::StringId>,
) {
    match node {
        OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } => {
            if let ExpressionKind::StringSlice(text) = &expression.kind {
                string_slices.push(*text);
            }
        }

        OwnedRuntimeTemplateNode::Sequence { children, .. } => {
            for child in children {
                collect_owned_node_string_slice_expressions(child, string_slices);
            }
        }

        OwnedRuntimeTemplateNode::BranchChain {
            branches, fallback, ..
        } => {
            for branch in branches {
                collect_owned_node_string_slice_expressions(&branch.body, string_slices);
            }
            if let Some(fallback) = fallback {
                collect_owned_node_string_slice_expressions(fallback, string_slices);
            }
        }

        OwnedRuntimeTemplateNode::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            collect_owned_node_string_slice_expressions(body, string_slices);
            if let Some(wrapper) = aggregate_wrapper {
                collect_owned_node_string_slice_expressions(wrapper, string_slices);
            }
        }

        OwnedRuntimeTemplateNode::ChildTemplate { template, .. } => {
            collect_owned_handoff_string_slice_expressions(template, string_slices);
        }

        OwnedRuntimeTemplateNode::ConditionalWrapper { child, wrapper, .. } => {
            collect_owned_node_string_slice_expressions(child, string_slices);
            collect_owned_node_string_slice_expressions(wrapper, string_slices);
        }

        OwnedRuntimeTemplateNode::Text { .. }
        | OwnedRuntimeTemplateNode::AggregateOutput
        | OwnedRuntimeTemplateNode::LoopControl { .. }
        | OwnedRuntimeTemplateNode::RuntimeSlotSite { .. }
        | OwnedRuntimeTemplateNode::Slot { .. } => {}
    }
}

/// Builds a `Template` with a registered TIR root containing a text segment and
/// a runtime reference expression, matching the production shape for ordinary
/// runtime templates that are not const-foldable.
///
/// WHAT: the resulting template is not const-foldable because the reference is
///       a runtime value, so it must go through the runtime-template handoff path.
/// WHY: gives the new store-focused test a simple, representative input shape.
fn registered_runtime_template(
    text: crate::compiler_frontend::symbols::string_interning::StringId,
    reference_name: &str,
    store_id: TemplateStoreId,
    overlay_set_id: TemplateOverlaySetId,
    template_ir_store: &Rc<RefCell<TemplateIrStore>>,
    string_table: &mut StringTable,
) -> Template {
    let byte_len = string_table.resolve(text).len() as u32;
    let reference_path = InternedPath::from_single_str(reference_name, string_table);
    let reference_expression = Expression::reference_with_type_id(
        reference_path,
        DataType::StringSlice,
        builtin_type_ids::STRING,
        SourceLocation::default(),
        ValueMode::ImmutableReference,
        ConstRecordState::RuntimeValue,
    );
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text_node = builder.push_text_node(
            text,
            byte_len,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let dynamic_node = builder.push_dynamic_expression_node(
            reference_expression,
            TemplateSegmentOrigin::Body,
            None,
            SourceLocation::default(),
        );
        let root =
            builder.push_sequence_node(vec![text_node, dynamic_node], SourceLocation::default());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };
    template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    )
}

#[test]
fn ordinary_runtime_template_handoff_uses_module_tir_store() {
    let mut string_table = StringTable::new();
    let text = string_table.intern("hello ");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let template = registered_runtime_template(
        text,
        "name",
        store_id,
        overlay_set_id,
        &template_ir_store,
        &mut string_table,
    );

    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("ordinary runtime template expression normalization should succeed");

    let handoff = runtime_template_handoff_from_expression(expression);
    assert!(
        matches!(handoff.body, OwnedRuntimeTemplateBody::Render(_)),
        "ordinary runtime templates must materialize a render body handoff"
    );
}

#[test]
fn runtime_template_expression_normalization_replaces_template_with_owned_handoff() {
    let mut string_table = StringTable::new();
    let text = string_table.intern("hello ");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let template = registered_runtime_template(
        text,
        "name",
        store_id,
        overlay_set_id,
        &template_ir_store,
        &mut string_table,
    );

    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("runtime template expression normalization should succeed");

    let ExpressionKind::RuntimeTemplateHandoff(handoff) = &expression.kind else {
        panic!("runtime template expression should be replaced with an owned handoff");
    };
    assert!(
        matches!(handoff.body, OwnedRuntimeTemplateBody::Render(_)),
        "ordinary runtime templates must keep using the render handoff body"
    );
    assert_eq!(expression.diagnostic_type, DataType::Template);
    assert_eq!(expression.value_mode, ValueMode::ImmutableOwned);
    assert!(
        expression
            .reactive_template
            .as_ref()
            .is_some_and(|metadata| metadata.template_backed),
        "runtime handoff expressions must preserve template-backed metadata"
    );
}

#[test]
fn runtime_template_expression_handoff_uses_finalized_expression_overlay_view() {
    let mut string_table = StringTable::new();
    let overlay_text = string_table.intern("normalized overlay text");
    let runtime_path = InternedPath::from_single_str("runtime_name", &mut string_table);

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let empty_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());
    let nested_template_expression = Expression::template(
        registered_text_template(
            overlay_text,
            store_id,
            empty_overlay_set_id,
            &template_ir_store,
            &string_table,
        ),
        ValueMode::ImmutableOwned,
    );

    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let normalized_dynamic_node = builder.push_dynamic_expression_node(
            nested_template_expression,
            TemplateSegmentOrigin::Body,
            None,
            SourceLocation::default(),
        );
        let runtime_dynamic_node = builder.push_dynamic_expression_node(
            Expression::reference_with_type_id(
                runtime_path,
                DataType::StringSlice,
                builtin_type_ids::STRING,
                SourceLocation::default(),
                ValueMode::ImmutableReference,
                ConstRecordState::RuntimeValue,
            ),
            TemplateSegmentOrigin::Body,
            None,
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(
            vec![normalized_dynamic_node, runtime_dynamic_node],
            SourceLocation::default(),
        );
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id: empty_overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );
    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("runtime template normalization should use the finalized view handoff");

    let ExpressionKind::RuntimeTemplateHandoff(handoff) = &expression.kind else {
        panic!("runtime template expression should be replaced with an owned handoff");
    };

    let mut string_slices = Vec::new();
    collect_owned_handoff_string_slice_expressions(handoff, &mut string_slices);
    assert!(
        string_slices.contains(&overlay_text),
        "runtime handoff must materialize normalized dynamic expressions from the final effective TirView"
    );
    assert!(
        expression.reactive_template.is_some(),
        "runtime handoff replacement should preserve template metadata"
    );
}

/// Proves that a nested runtime template inside a TIR dynamic expression node
/// is normalized through the final effective view.
#[test]
fn nested_runtime_template_normalizes_through_final_view() {
    let mut string_table = StringTable::new();
    let nested_text = string_table.intern("nested runtime text");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a TIR whose sole dynamic expression holds a nested runtime
    // template (text plus a runtime reference, so it is not const-foldable).
    let nested_template = registered_runtime_template(
        nested_text,
        "runtime_ref",
        store_id,
        overlay_set_id,
        &template_ir_store,
        &mut string_table,
    );

    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let dynamic_node = builder.push_dynamic_expression_node(
            Expression::template(nested_template, ValueMode::ImmutableOwned),
            TemplateSegmentOrigin::Body,
            None,
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(vec![dynamic_node], SourceLocation::default());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );
    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("nested runtime template normalization should succeed through the final TIR view");

    let ExpressionKind::RuntimeTemplateHandoff(handoff) = &expression.kind else {
        panic!("outer template expression should be replaced with an owned handoff");
    };

    // The handoff must contain the nested runtime template handoff inside a
    // DynamicExpression node, proving the overlay path normalized it.
    let mut found_nested_handoff = false;
    if let OwnedRuntimeTemplateBody::Render(root) = &handoff.body {
        find_runtime_handoff_in_node(root, &mut found_nested_handoff);
    }
    assert!(
        found_nested_handoff,
        "handoff must contain the nested runtime template handoff materialized from the final TIR view"
    );
}

/// Recursively checks whether any DynamicExpression node in the owned handoff
/// tree carries a RuntimeTemplateHandoff expression kind.
fn find_runtime_handoff_in_node(node: &OwnedRuntimeTemplateNode, found: &mut bool) {
    if *found {
        return;
    }
    match node {
        OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } => {
            if matches!(expression.kind, ExpressionKind::RuntimeTemplateHandoff(_)) {
                *found = true;
            }
        }
        OwnedRuntimeTemplateNode::Sequence { children, .. } => {
            for child in children {
                find_runtime_handoff_in_node(child, found);
            }
        }
        OwnedRuntimeTemplateNode::BranchChain {
            branches, fallback, ..
        } => {
            for branch in branches {
                find_runtime_handoff_in_node(&branch.body, found);
            }
            if let Some(fallback) = fallback {
                find_runtime_handoff_in_node(fallback, found);
            }
        }
        OwnedRuntimeTemplateNode::Loop {
            body,
            aggregate_wrapper,
            ..
        } => {
            find_runtime_handoff_in_node(body, found);
            if let Some(wrapper) = aggregate_wrapper {
                find_runtime_handoff_in_node(wrapper, found);
            }
        }
        OwnedRuntimeTemplateNode::ConditionalWrapper { child, wrapper, .. } => {
            find_runtime_handoff_in_node(child, found);
            find_runtime_handoff_in_node(wrapper, found);
        }
        _ => {}
    }
}

/// Proves that a const child template referenced from the outer TIR view folds
/// correctly through the final view.
#[test]
fn nested_const_template_folds_through_final_view() {
    let mut string_table = StringTable::new();
    let child_text_str = "child folded text";
    let child_text = string_table.intern(child_text_str);
    let child_byte_len = child_text_str.len() as u32;

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a child template (const text) and an outer template whose TIR
    // root is a sequence containing a child-template ref to it.
    let outer_template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let child_root = builder.push_text_node(
            child_text,
            child_byte_len,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let child_id = builder.finish_template(
            child_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        );

        let child_ref_node = builder.push_child_template_node(child_id, SourceLocation::default());
        let outer_root =
            builder.push_sequence_node(vec![child_ref_node], SourceLocation::default());
        builder.finish_template(
            outer_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, outer_template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::String,
        SourceLocation::default(),
    );

    let folded = try_fold_template_to_string(
        &template,
        TemplateFinalizationFoldInputs {
            source_file_scope: &source_file_scope,
            path_format_config: &path_format_config,
            project_path_resolver: &project_path_resolver,
            string_table: &mut string_table,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_store: &template_ir_store,
            template_ir_registry: Rc::new(RefCell::new(registry)),
        },
    )
    .expect("fold through final view should succeed")
    .folded
    .expect("composed template with const child should fold");

    assert_eq!(
        folded, child_text,
        "fold must produce the child template's text from the final TIR view"
    );
}

/// Proves that reactive subscriptions stored on TIR dynamic expression nodes
/// are collected into the expression's reactive metadata through the finalized
/// effective view.
#[test]
fn reactive_metadata_derived_from_nested_final_view() {
    let mut string_table = StringTable::new();
    let reactive_path = InternedPath::from_single_str("reactive_source", &mut string_table);

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a TIR with a dynamic expression carrying a reactive subscription.
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);

        let subscription = ReactiveSubscription {
            source: ReactiveSource {
                path: reactive_path.clone(),
                kind: ReactiveSourceKind::Declaration,
            },
            type_id: builtin_type_ids::STRING,
            location: SourceLocation::default(),
        };

        let dynamic_node = builder.push_dynamic_expression_node(
            Expression::reference_with_type_id(
                reactive_path.clone(),
                DataType::StringSlice,
                builtin_type_ids::STRING,
                SourceLocation::default(),
                ValueMode::ImmutableReference,
                ConstRecordState::RuntimeValue,
            ),
            TemplateSegmentOrigin::Body,
            Some(subscription),
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(vec![dynamic_node], SourceLocation::default());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::StringFunction,
        SourceLocation::default(),
    );
    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    normalize_expression_templates(&mut expression, &mut context)
        .expect("reactive template normalization should succeed");

    let metadata = expression
        .reactive_template
        .as_ref()
        .expect("runtime handoff replacement should preserve reactive template metadata");

    assert!(
        metadata.template_backed,
        "reactive metadata should be template-backed"
    );
    assert!(
        metadata.subscriptions.iter().any(|sub| {
            sub.source.path == reactive_path
                && matches!(sub.source.kind, ReactiveSourceKind::Declaration)
        }),
        "reactive metadata must contain the subscription from the final TIR view"
    );
}

/// Proves that a slot-insert helper artifact surviving composition is rejected
/// after final view traversal, not silently passed to HIR.
#[test]
fn helper_artifact_rejected_after_final_view_traversal() {
    let mut string_table = StringTable::new();
    let text = string_table.intern("slot insert content");

    let project_path_resolver = test_project_path_resolver();
    let path_format_config = PathStringFormatConfig::default();
    let source_file_scope = InternedPath::new();
    let template_ir_store = Rc::new(RefCell::new(TemplateIrStore::new()));

    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.adopt_store(Rc::clone(&template_ir_store));
    let overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    // Build a TIR root with simple text. The template kind is SlotInsert,
    // which finalization must reject as a helper artifact.
    let template_id = {
        let mut store = template_ir_store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text_node = builder.push_text_node(
            text,
            "slot insert content".len() as u32,
            TemplateSegmentOrigin::Body,
            SourceLocation::default(),
        );
        let root = builder.push_sequence_node(vec![text_node], SourceLocation::default());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::SlotInsert(SlotKey::Default),
            TemplateIrSummary::default(),
            SourceLocation::default(),
        )
    };

    let template = template_with_reference(
        TemplateTirReference {
            root: TemplateRef::new(store_id, template_id),
            store_owner: template_ir_store.borrow().owner(),
            phase: TemplateTirPhase::Composed,
            overlay_set_id,
        },
        TemplateType::SlotInsert(SlotKey::Default),
        SourceLocation::default(),
    );

    let mut expression = Expression::template(template, ValueMode::ImmutableOwned);

    let mut context = TemplateNormalizationContext {
        source_file_scope: &source_file_scope,
        path_format_config: &path_format_config,
        project_path_resolver: &project_path_resolver,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        string_table: &mut string_table,
        template_ir_store: Rc::clone(&template_ir_store),
        template_ir_registry: Rc::new(RefCell::new(registry)),
    };

    let result = normalize_expression_templates(&mut expression, &mut context);
    assert!(
        result.is_err(),
        "slot-insert helper artifact must be rejected after final view traversal"
    );

    let TemplateNormalizationError::Diagnostic(diagnostic) =
        result.expect_err("error was asserted above")
    else {
        panic!(
            "helper artifact rejection should produce a diagnostic, not an infrastructure error"
        );
    };
    assert!(
        matches!(
            diagnostic.as_ref().payload,
            DiagnosticPayload::InvalidTemplateStructure {
                reason: InvalidTemplateStructureReason::HelperOutsideWrapperSlot
            }
        ),
        "diagnostic must be HelperOutsideWrapperSlot"
    );
}
