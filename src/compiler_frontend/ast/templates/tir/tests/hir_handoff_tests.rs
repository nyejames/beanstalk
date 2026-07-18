//! Owned HIR-handoff materialization tests.
//!
//! WHAT: checks that finalized TIR becomes owned runtime handoff data.
//! WHY: the AST/HIR boundary must consume one shared module store without
//! exposing TIR identity or store-internal traversal.

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold_cache::TirFoldCache;
use crate::compiler_frontend::ast::templates::tir::ids::{
    ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId,
};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIr, TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder,
};
use crate::compiler_frontend::ast::templates::tir::overlays::{
    TemplateOverlaySet, TemplateOverlaySetId, TirExpressionOverlay, TirSlotResolution,
    TirSlotResolutionOverlay, TirWrapperContext, TirWrapperContextOverlay,
};
use crate::compiler_frontend::ast::templates::tir::refs::{
    TemplateTirChildReference, TemplateWrapperReference,
};
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::{
    TemplateIrSummary, summarize_existing_root,
};
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeTemplateBody, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::datatype::DataType;
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

fn empty_location() -> SourceLocation {
    SourceLocation::default()
}

fn fold_context<'a>(
    strings: &'a mut StringTable,
    store: &Rc<RefCell<TemplateIrStore>>,
) -> TemplateFoldContext<'a> {
    let cwd = std::env::temp_dir();
    let resolver = Box::leak(Box::new(
        ProjectPathResolver::new(
            cwd.clone(),
            cwd,
            crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots::empty(
            ),
            &crate::builder_surface::SourceFileKindRegistry::default(),
        )
        .expect("test path resolver should be valid"),
    ));
    let path_format = Box::leak(Box::new(PathStringFormatConfig::default()));
    let source_scope = Box::leak(Box::new(InternedPath::new()));
    TemplateFoldContext {
        string_table: strings,
        project_path_resolver: resolver,
        path_format_config: path_format,
        source_file_scope: source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        template_ir_store: Some(Rc::clone(store)),
        bindings: vec![],
        fold_cache: TirFoldCache::new(),
    }
}

/// Pushes a literal text node into the store and returns its ID.
fn text_node_id(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrNodeId {
    let text_id = string_table.intern(text);
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::Text {
            text: text_id,
            byte_len: text.len() as u32,
            origin: TemplateSegmentOrigin::Body,
        },
        empty_location(),
    ))
}

/// Finishes a simple text-function template from its root node.
fn finish_text_template(store: &mut TemplateIrStore, root: TemplateIrNodeId) -> TemplateIrId {
    store.push_template(TemplateIr::new(
        root,
        Style::default(),
        TemplateType::StringFunction,
        summarize_existing_root(store, root),
        empty_location(),
    ))
}

/// Builds and finishes a one-node text template, returning its root ID.
fn text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let text_node = text_node_id(store, string_table, text);
    finish_text_template(store, text_node)
}

/// Builds a bool-typed reference expression for selector/header overrides.
fn bool_reference_expression(string_table: &mut StringTable, name: &str) -> Expression {
    Expression::reference_with_type_id(
        InternedPath::from_single_str(name, string_table),
        DataType::Bool,
        builtin_type_ids::BOOL,
        empty_location(),
        ValueMode::ImmutableReference,
        ConstRecordState::RuntimeValue,
    )
}

/// Allocates an overlay set that overrides the given expression sites.
fn expression_overlay_set(
    store: &mut TemplateIrStore,
    overrides: Vec<(ExpressionSiteId, Expression)>,
) -> TemplateOverlaySetId {
    let overrides = overrides
        .into_iter()
        .map(|(site_id, expression)| (site_id, Box::new(expression)))
        .collect();
    let expression_overlay_id =
        store.allocate_expression_overlay(TirExpressionOverlay { overrides });
    store.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    })
}

/// Allocates an overlay set that resolves the given slot occurrences.
fn slot_resolution_overlay_set(
    store: &mut TemplateIrStore,
    resolutions: Vec<(SlotOccurrenceId, TirSlotResolution)>,
) -> TemplateOverlaySetId {
    let slot_resolution_overlay_id =
        store.allocate_slot_resolution_overlay(TirSlotResolutionOverlay { resolutions });
    store.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_resolution_overlay_id),
        wrapper_context: None,
    })
}

/// Pushes a child-template reference node and returns its node ID.
fn child_template_node_id(
    store: &mut TemplateIrStore,
    reference: TemplateTirChildReference,
) -> TemplateIrNodeId {
    let occurrence_id = store.next_child_template_occurrence_id();
    store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::ChildTemplate {
            reference,
            occurrence_id,
        },
        empty_location(),
    ))
}

/// Builds a finalized same-store child reference for a template root.
fn child_reference(
    template_id: TemplateIrId,
    overlay_set_id: TemplateOverlaySetId,
) -> TemplateTirChildReference {
    TemplateTirChildReference::new(template_id, TemplateTirPhase::Finalized, overlay_set_id)
}

fn view_for(
    store: &TemplateIrStore,
    root: TemplateIrId,
    overlay_set_id: TemplateOverlaySetId,
) -> TirView<'_> {
    TirView::with_minimum_phase(
        store,
        root,
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("finalized test view should construct")
}

/// Materializes the parent template through the fold-context entry point,
/// returning the full `Result` so success tests can unwrap and error tests
/// can assert on the `CompilerError`.
fn materialize_parent_handoff_result(
    store: Rc<RefCell<TemplateIrStore>>,
    parent_template_id: TemplateIrId,
    string_table: &mut StringTable,
    overlay_set_id: TemplateOverlaySetId,
) -> Result<OwnedRuntimeTemplateBody, CompilerError> {
    let mut context = fold_context(string_table, &store);
    let store_ref = store.borrow();
    let view = view_for(&store_ref, parent_template_id, overlay_set_id);
    store_ref
        .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut context)
        .map(|handoff| handoff.body)
}

/// Convenience wrapper for success-path tests that expect materialization to
/// succeed.
fn materialize_parent_handoff(
    store: Rc<RefCell<TemplateIrStore>>,
    parent_template_id: TemplateIrId,
    string_table: &mut StringTable,
    overlay_set_id: TemplateOverlaySetId,
) -> OwnedRuntimeTemplateBody {
    materialize_parent_handoff_result(store, parent_template_id, string_table, overlay_set_id)
        .expect("handoff materialization should succeed")
}

fn assert_owned_text_node(
    node: &OwnedRuntimeTemplateNode,
    expected: &str,
    string_table: &StringTable,
) {
    let OwnedRuntimeTemplateNode::Text { text, .. } = node else {
        panic!("expected owned text node, got {:?}", node);
    };
    assert_eq!(string_table.resolve(*text), expected);
}

// ---------------------------------------------------------------------------
//  Wrapper template builders
// ---------------------------------------------------------------------------

fn build_branch_wrapper_template(store: &mut TemplateIrStore) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let default_slot = builder.push_slot_node(SlotKey::Default, empty_location());
    let positional_slot = builder.push_slot_node(SlotKey::Positional(2), empty_location());
    let branches = vec![
        TemplateIrBranch::new(
            TemplateBranchSelector::Bool(Expression::bool(
                true,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            default_slot,
            empty_location(),
        ),
        TemplateIrBranch::new(
            TemplateBranchSelector::Bool(Expression::bool(
                false,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
            positional_slot,
            empty_location(),
        ),
    ];
    let root = builder.push_branch_chain_node(branches, None, empty_location());
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

fn build_loop_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let body_default_slot = builder.push_slot_node(SlotKey::Default, empty_location());
    let aggregate_before = builder.push_text_node(
        string_table.intern("aggregate-before"),
        "aggregate-before".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let aggregate_positional_slot =
        builder.push_slot_node(SlotKey::Positional(1), empty_location());
    let aggregate_after = builder.push_text_node(
        string_table.intern("aggregate-after"),
        "aggregate-after".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let aggregate_wrapper = builder.push_sequence_node(
        vec![aggregate_before, aggregate_positional_slot, aggregate_after],
        empty_location(),
    );
    let root = builder.push_loop_node(
        TemplateLoopHeader::Conditional {
            condition: Box::new(Expression::bool(
                true,
                empty_location(),
                ValueMode::ImmutableOwned,
            )),
        },
        body_default_slot,
        Some(aggregate_wrapper),
        empty_location(),
    );
    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

fn build_same_store_child_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let nested_before = builder.push_text_node(
        string_table.intern("nested-before"),
        "nested-before".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let nested_positional_slot = builder.push_slot_node(SlotKey::Positional(0), empty_location());
    let nested_after = builder.push_text_node(
        string_table.intern("nested-after"),
        "nested-after".len() as u32,
        TemplateSegmentOrigin::Body,
        empty_location(),
    );
    let nested_root = builder.push_sequence_node(
        vec![nested_before, nested_positional_slot, nested_after],
        empty_location(),
    );
    let nested_template_id = builder.finish_template(
        nested_root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    );
    let nested_child = builder.push_child_template_node(nested_template_id, empty_location());
    builder.finish_template(
        nested_child,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

fn build_expression_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> (TemplateIrId, ExpressionSiteId) {
    let expression_site_id = store.next_expression_site_id();
    let expression_node = store.push_node(TemplateIrNode::new(
        TemplateIrNodeKind::DynamicExpression {
            expression: Box::new(bool_reference_expression(string_table, "original")),
            origin: TemplateSegmentOrigin::Body,
            reactive_subscription: None,
            site_id: expression_site_id,
        },
        empty_location(),
    ));
    let mut builder = TemplateIrBuilder::new(store);
    let slot_node = builder.push_slot_node(SlotKey::Default, empty_location());
    let root = builder.push_sequence_node(vec![expression_node, slot_node], empty_location());
    let template_id = builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    );
    (template_id, expression_site_id)
}

fn build_slotless_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrId {
    text_template(store, string_table, "slotless-wrapper")
}

fn build_named_only_wrapper_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
) -> TemplateIrId {
    let named_slot_name = string_table.intern("named");
    let mut builder = TemplateIrBuilder::new(store);
    let named_slot = builder.push_slot_node(SlotKey::Named(named_slot_name), empty_location());
    builder.finish_template(
        named_slot,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

/// Builds one parent child occurrence with an inherited wrapper and returns the
/// parent plus the wrapper-context overlay that activates it. The wrapper's own
/// overlay set is `empty_overlay_set_id` unless a separate wrapper overlay is
/// supplied through `build_parent_with_inherited_wrapper_and_overlay`.
fn build_parent_with_inherited_wrapper(
    store: &mut TemplateIrStore,
    wrapper_template_id: TemplateIrId,
    empty_overlay_set_id: TemplateOverlaySetId,
    string_table: &mut StringTable,
) -> (TemplateIrId, TemplateOverlaySetId) {
    build_parent_with_inherited_wrapper_and_overlay(
        store,
        wrapper_template_id,
        empty_overlay_set_id,
        empty_overlay_set_id,
        string_table,
    )
}

fn build_parent_with_inherited_wrapper_and_overlay(
    store: &mut TemplateIrStore,
    wrapper_template_id: TemplateIrId,
    empty_overlay_set_id: TemplateOverlaySetId,
    wrapper_overlay_set_id: TemplateOverlaySetId,
    string_table: &mut StringTable,
) -> (TemplateIrId, TemplateOverlaySetId) {
    let (parent_template_id, wrapper_set_id, child_occurrence_id) = {
        let child_template_id = text_template(store, string_table, "child");
        let child_occurrence_id = store.next_child_template_occurrence_id();
        let child_reference = child_reference(child_template_id, empty_overlay_set_id);
        let child_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: child_reference,
                occurrence_id: child_occurrence_id,
            },
            empty_location(),
        ));
        let parent_template_id = finish_text_template(store, child_node);
        let wrapper_reference = TemplateWrapperReference::new(
            wrapper_template_id,
            TemplateTirPhase::Finalized,
            wrapper_overlay_set_id,
        );
        let wrapper_set_id = store.push_or_reuse_wrapper_set(vec![wrapper_reference]);
        (parent_template_id, wrapper_set_id, child_occurrence_id)
    };

    let wrapper_overlay_id = store.allocate_wrapper_context_overlay(TirWrapperContextOverlay {
        contexts: vec![(
            child_occurrence_id,
            TirWrapperContext::inherited(wrapper_set_id),
        )],
    });
    let overlay_set_id = store.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: None,
        wrapper_context: Some(wrapper_overlay_id),
    });

    (parent_template_id, overlay_set_id)
}

// ---------------------------------------------------------------------------
//  Text and slot handoff
// ---------------------------------------------------------------------------

#[test]
fn owned_handoff_materializes_text_from_the_shared_store() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let template_id = text_template(&mut store.borrow_mut(), &mut strings, "hello");
    let mut context = fold_context(&mut strings, &store);
    let handoff = {
        let store_ref = store.borrow();
        let view = view_for(&store_ref, template_id, TemplateOverlaySetId::empty());
        store_ref
            .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut context)
            .expect("text handoff should succeed")
    };

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Text { text, .. }) =
        handoff.body
    else {
        panic!("text template should materialize as an owned text node");
    };
    assert_eq!(strings.resolve(text), "hello");
}

#[test]
fn owned_handoff_resolves_slot_overlay_to_a_child_template() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, source_id, occurrence_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let source_id = text_template(&mut store_ref, &mut strings, "filled");
        let occurrence_id = store_ref.next_slot_occurrence_id();
        let slot = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot {
                placeholder: TirSlotPlaceholder::new(
                    SlotKey::Default,
                    occurrence_id,
                    empty_location(),
                ),
            },
            empty_location(),
        ));
        let summary = summarize_existing_root(&store_ref, slot);
        let parent_id = store_ref.push_template(TemplateIr::new(
            slot,
            Style::default(),
            TemplateType::StringFunction,
            summary,
            empty_location(),
        ));
        let slot_overlay_id =
            store_ref.allocate_slot_resolution_overlay(TirSlotResolutionOverlay {
                resolutions: vec![(
                    occurrence_id,
                    TirSlotResolution::resolved(SlotKey::Default, vec![source_id]),
                )],
            });
        let overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet {
            expression_overrides: None,
            slot_resolution: Some(slot_overlay_id),
            wrapper_context: None,
        });
        (parent_id, source_id, occurrence_id, overlay_set_id)
    };
    let mut context = fold_context(&mut strings, &store);
    let handoff = {
        let store_ref = store.borrow();
        let view = view_for(&store_ref, parent_id, overlay_set_id);
        store_ref
            .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut context)
            .expect("slot handoff should succeed")
    };

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = handoff.body
    else {
        panic!("resolved slot should materialize as a child-template handoff");
    };
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Text { text, .. }) =
        template.body
    else {
        panic!("slot source should materialize as text");
    };
    assert_eq!(strings.resolve(text), "filled");
    assert_eq!(occurrence_id, SlotOccurrenceId::new(0));
    assert!(store.borrow().get_template(source_id).is_some());
}

#[test]
fn owned_handoff_missing_slot_resolution_renders_slot_placeholder() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, _occurrence_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let occurrence_id = store_ref.next_slot_occurrence_id();
        let slot = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot {
                placeholder: TirSlotPlaceholder::new(
                    SlotKey::Default,
                    occurrence_id,
                    empty_location(),
                ),
            },
            empty_location(),
        ));
        let parent_id = finish_text_template(&mut store_ref, slot);
        let overlay_set_id = slot_resolution_overlay_set(
            &mut store_ref,
            vec![(occurrence_id, TirSlotResolution::missing(SlotKey::Default))],
        );
        (parent_id, occurrence_id, overlay_set_id)
    };
    let mut context = fold_context(&mut strings, &store);
    let handoff = {
        let store_ref = store.borrow();
        let view = view_for(&store_ref, parent_id, overlay_set_id);
        store_ref
            .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut context)
            .expect("handoff materialization should succeed")
    };

    assert!(
        matches!(
            &handoff.body,
            OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Slot { .. })
        ),
        "missing slot resolution should materialize as a structural no-output slot placeholder, got {:?}",
        handoff.body
    );
}

#[test]
fn owned_handoff_preserves_same_store_child_boundary() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, child_id) = {
        let mut store_ref = store.borrow_mut();
        let child_id = text_template(&mut store_ref, &mut strings, "child");
        let occurrence_id = store_ref.next_child_template_occurrence_id();
        let child_node = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: TemplateTirChildReference::new(
                    child_id,
                    TemplateTirPhase::Parsed,
                    TemplateOverlaySetId::empty(),
                ),
                occurrence_id,
            },
            empty_location(),
        ));
        let summary = summarize_existing_root(&store_ref, child_node);
        let parent_id = store_ref.push_template(TemplateIr::new(
            child_node,
            Style::default(),
            TemplateType::StringFunction,
            summary,
            empty_location(),
        ));
        (parent_id, child_id)
    };
    let mut context = fold_context(&mut strings, &store);
    let handoff = {
        let store_ref = store.borrow();
        let view = view_for(&store_ref, parent_id, TemplateOverlaySetId::empty());
        store_ref
            .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut context)
            .expect("child handoff should succeed")
    };

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = handoff.body
    else {
        panic!("child boundary should remain an owned child handoff");
    };
    assert!(matches!(template.body, OwnedRuntimeTemplateBody::Render(_)));
    assert!(store.borrow().get_template(child_id).is_some());
}

// ---------------------------------------------------------------------------
//  Expression-overlay and folded-child handoff
// ---------------------------------------------------------------------------

#[test]
fn parent_root_expression_overlay_applies_inside_same_store_child() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, _child_site_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let child_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let child_site_id = store_ref.next_expression_site_id();
        let child_expression = bool_reference_expression(&mut strings, "original");
        let child_root = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(child_expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: child_site_id,
            },
            empty_location(),
        ));
        let child_template_id = finish_text_template(&mut store_ref, child_root);
        let child_node = child_template_node_id(
            &mut store_ref,
            child_reference(child_template_id, child_overlay_set_id),
        );
        let parent_id = finish_text_template(&mut store_ref, child_node);
        let overlay_set_id = expression_overlay_set(
            &mut store_ref,
            vec![(
                child_site_id,
                Expression::bool(true, empty_location(), ValueMode::ImmutableOwned),
            )],
        );
        (parent_id, child_site_id, overlay_set_id)
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = body
    else {
        panic!("expected same-store child template");
    };
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::DynamicExpression {
        expression,
        ..
    }) = &template.body
    else {
        panic!("expected child dynamic expression, got {:?}", template.body);
    };

    assert!(
        matches!(expression.kind, ExpressionKind::Bool(true)),
        "parent root override should win over the child's empty overlay"
    );
}

#[test]
fn folded_child_shortcut_preserves_root_overlay_through_nested_same_store_children() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (root_id, _leaf_site_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let leaf_site_id = store_ref.next_expression_site_id();
        let stale_structural_text = strings.intern("stale-structural");
        let leaf_root = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(Expression::string_slice(
                    stale_structural_text,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                )),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: leaf_site_id,
            },
            empty_location(),
        ));
        let leaf_template_id = finish_text_template(&mut store_ref, leaf_root);
        let middle_child = child_template_node_id(
            &mut store_ref,
            child_reference(leaf_template_id, empty_overlay_set_id),
        );
        let middle_template_id = finish_text_template(&mut store_ref, middle_child);
        let root_child = child_template_node_id(
            &mut store_ref,
            child_reference(middle_template_id, empty_overlay_set_id),
        );
        let root_id = finish_text_template(&mut store_ref, root_child);
        let effective_root_text = strings.intern("effective-root");
        let overlay_set_id = expression_overlay_set(
            &mut store_ref,
            vec![(
                leaf_site_id,
                Expression::string_slice(
                    effective_root_text,
                    empty_location(),
                    ValueMode::ImmutableOwned,
                ),
            )],
        );
        (root_id, leaf_site_id, overlay_set_id)
    };

    let body = materialize_parent_handoff(store, root_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template: middle_template,
    }) = &body
    else {
        panic!("expected root child template handoff, got {body:?}");
    };
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template: leaf_template,
    }) = &middle_template.body
    else {
        panic!(
            "expected nested leaf child template handoff, got {:?}",
            middle_template.body
        );
    };
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::DynamicExpression {
        expression,
        ..
    }) = &leaf_template.body
    else {
        panic!(
            "stale structural leaf must not be folded into text, got {:?}",
            leaf_template.body
        );
    };

    let ExpressionKind::StringSlice(text) = expression.kind else {
        panic!(
            "expected the root expression overlay to survive structurally, got {:?}",
            expression.kind
        );
    };
    assert_eq!(strings.resolve(text), "effective-root");
}

#[test]
fn folded_child_runtime_reference_falls_back_to_structural_handoff() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let child_site_id = store_ref.next_expression_site_id();
        let child_expression = bool_reference_expression(&mut strings, "runtime");
        let child_root = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(child_expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: child_site_id,
            },
            empty_location(),
        ));
        let child_template_id = finish_text_template(&mut store_ref, child_root);
        let child_node = child_template_node_id(
            &mut store_ref,
            child_reference(child_template_id, empty_overlay_set_id),
        );
        let parent_id = finish_text_template(&mut store_ref, child_node);
        (parent_id, empty_overlay_set_id)
    };

    let body = materialize_parent_handoff_result(store, parent_id, &mut strings, overlay_set_id)
        .expect("runtime reference should use structural handoff");

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = body
    else {
        panic!("expected same-store child template handoff, got {body:?}");
    };
    assert!(
        matches!(
            template.body,
            OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::DynamicExpression { .. })
        ),
        "runtime-reference child should remain an owned dynamic expression"
    );
}

#[test]
fn folded_child_infrastructure_error_propagates_through_hir_handoff() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let missing_child_root = TemplateIrNodeId::new(999);
        let child_template_id = store_ref.push_template(TemplateIr::new(
            missing_child_root,
            Style::default(),
            TemplateType::StringFunction,
            TemplateIrSummary::empty(),
            empty_location(),
        ));
        let child_node = child_template_node_id(
            &mut store_ref,
            child_reference(child_template_id, empty_overlay_set_id),
        );
        let parent_id = finish_text_template(&mut store_ref, child_node);
        (parent_id, empty_overlay_set_id)
    };

    let error = materialize_parent_handoff_result(store, parent_id, &mut strings, overlay_set_id)
        .expect_err("malformed child authority must reach the HIR handoff caller");

    assert!(
        error.msg.contains("TIR fold safety: node"),
        "expected a stable infrastructure lane, got: {}",
        error.msg
    );
}

// ---------------------------------------------------------------------------
//  Inherited wrapper handoff
// ---------------------------------------------------------------------------

#[test]
fn inherited_wrapper_handoff_injects_through_branch_boundaries() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let wrapper_template_id = build_branch_wrapper_template(&mut store_ref);
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::BranchChain {
        branches,
        fallback,
        ..
    }) = body
    else {
        panic!("expected branch-chain wrapper handoff, got {:?}", body);
    };

    assert!(fallback.is_none());
    assert_eq!(branches.len(), 2);
    assert!(matches!(
        branches[0].body,
        OwnedRuntimeTemplateNode::Slot { .. }
    ));
    assert_owned_text_node(&branches[1].body, "child", &strings);
}

#[test]
fn inherited_wrapper_handoff_injects_through_loop_body_and_aggregate() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let wrapper_template_id = build_loop_wrapper_template(&mut store_ref, &mut strings);
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Loop {
        body,
        aggregate_wrapper,
        ..
    }) = body
    else {
        panic!("expected loop wrapper handoff, got {:?}", body);
    };

    assert!(matches!(*body, OwnedRuntimeTemplateNode::Slot { .. }));
    let Some(aggregate_wrapper) = aggregate_wrapper else {
        panic!("expected aggregate wrapper to remain present");
    };
    let OwnedRuntimeTemplateNode::Sequence { children } = aggregate_wrapper.as_ref() else {
        panic!(
            "expected aggregate wrapper sequence, got {:?}",
            aggregate_wrapper
        );
    };
    assert_eq!(children.len(), 3);
    assert_owned_text_node(&children[0], "aggregate-before", &strings);
    assert_owned_text_node(&children[1], "child", &strings);
    assert_owned_text_node(&children[2], "aggregate-after", &strings);
}

#[test]
fn inherited_wrapper_handoff_injects_through_same_store_child_template() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let wrapper_template_id =
            build_same_store_child_wrapper_template(&mut store_ref, &mut strings);
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate { template }) =
        body
    else {
        panic!("expected same-store child wrapper handoff, got {:?}", body);
    };
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Sequence { children }) =
        template.body
    else {
        panic!(
            "expected nested same-store child sequence, got {:?}",
            template.body
        );
    };

    assert_eq!(children.len(), 3);
    assert_owned_text_node(&children[0], "nested-before", &strings);
    assert_owned_text_node(&children[1], "child", &strings);
    assert_owned_text_node(&children[2], "nested-after", &strings);
}

#[test]
fn inherited_same_store_wrapper_handoff_applies_wrapper_overlay() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let (wrapper_template_id, expression_site_id) =
            build_expression_wrapper_template(&mut store_ref, &mut strings);
        let wrapper_overlay_set_id = expression_overlay_set(
            &mut store_ref,
            vec![(
                expression_site_id,
                Expression::bool(true, empty_location(), ValueMode::ImmutableOwned),
            )],
        );
        build_parent_with_inherited_wrapper_and_overlay(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            wrapper_overlay_set_id,
            &mut strings,
        )
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Sequence { children }) = body
    else {
        panic!("expected same-store wrapper sequence, got {:?}", body);
    };

    let OwnedRuntimeTemplateNode::DynamicExpression { expression, .. } = &children[0] else {
        panic!("expected wrapper expression, got {:?}", children[0]);
    };
    assert!(
        matches!(expression.kind, ExpressionKind::Bool(true)),
        "same-store wrapper overlay should override the wrapper expression"
    );
    assert_owned_text_node(&children[1], "child", &strings);
}

#[test]
fn inherited_slotless_wrapper_handoff_appends_child_after_wrapper_content() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let wrapper_template_id = build_slotless_wrapper_template(&mut store_ref, &mut strings);
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Sequence { children }) = body
    else {
        panic!("expected slotless wrapper sequence, got {:?}", body);
    };

    assert_eq!(children.len(), 2);
    assert_owned_text_node(&children[0], "slotless-wrapper", &strings);
    assert_owned_text_node(&children[1], "child", &strings);
}

#[test]
fn inherited_named_only_wrapper_handoff_preserves_named_slot_and_appends_child() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let wrapper_template_id = build_named_only_wrapper_template(&mut store_ref, &mut strings);
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let body = materialize_parent_handoff(store, parent_id, &mut strings, overlay_set_id);
    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Sequence { children }) = body
    else {
        panic!("expected named-only wrapper sequence, got {:?}", body);
    };

    assert_eq!(children.len(), 2);
    assert!(matches!(children[0], OwnedRuntimeTemplateNode::Slot { .. }));
    assert_owned_text_node(&children[1], "child", &strings);
}

// ---------------------------------------------------------------------------
//  Malformed-authority handoff failures
// ---------------------------------------------------------------------------

#[test]
fn malformed_child_overlay_set_propagates_view_failure() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, valid_overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let valid_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let child_template_id = text_template(&mut store_ref, &mut strings, "child text");
        // Use an unallocated overlay set ID so child-view construction fails.
        let invalid_overlay_set_id = TemplateOverlaySetId::new(99);
        let child_node = child_template_node_id(
            &mut store_ref,
            child_reference(child_template_id, invalid_overlay_set_id),
        );
        let parent_id = finish_text_template(&mut store_ref, child_node);
        (parent_id, valid_overlay_set_id)
    };

    let error =
        materialize_parent_handoff_result(store, parent_id, &mut strings, valid_overlay_set_id)
            .expect_err("malformed child overlay should produce a CompilerError");

    assert!(
        error.msg.contains("overlay set"),
        "expected error about missing overlay set, got: {}",
        error.msg
    );
}

#[test]
fn missing_wrapper_tree_node_propagates_schema_extraction_error() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let slot_occurrence_id = store_ref.next_slot_occurrence_id();
        let slot = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot {
                placeholder: TirSlotPlaceholder::new(
                    SlotKey::Default,
                    slot_occurrence_id,
                    empty_location(),
                ),
            },
            empty_location(),
        ));
        let wrapper_root = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Sequence {
                children: vec![slot, TemplateIrNodeId::new(9999)],
            },
            empty_location(),
        ));
        let wrapper_template_id = store_ref.push_template(TemplateIr::new(
            wrapper_root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        ));
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let error = materialize_parent_handoff_result(store, parent_id, &mut strings, overlay_set_id)
        .expect_err("missing wrapper tree node should produce a CompilerError");

    assert!(
        error.msg.contains("TIR slot schema extraction: node ID"),
        "expected schema-owned node error, got: {}",
        error.msg
    );
}

#[test]
fn missing_same_store_child_in_wrapper_propagates_schema_extraction_error() {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let mut strings = StringTable::new();
    let (parent_id, overlay_set_id) = {
        let mut store_ref = store.borrow_mut();
        let empty_overlay_set_id = store_ref.allocate_overlay_set(TemplateOverlaySet::empty());
        let missing_child_reference =
            child_reference(TemplateIrId::new(9999), empty_overlay_set_id);
        let missing_child_occurrence_id = store_ref.next_child_template_occurrence_id();
        let missing_child_node = store_ref.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::ChildTemplate {
                reference: missing_child_reference,
                occurrence_id: missing_child_occurrence_id,
            },
            empty_location(),
        ));
        let wrapper_template_id = finish_text_template(&mut store_ref, missing_child_node);
        build_parent_with_inherited_wrapper(
            &mut store_ref,
            wrapper_template_id,
            empty_overlay_set_id,
            &mut strings,
        )
    };

    let error = materialize_parent_handoff_result(store, parent_id, &mut strings, overlay_set_id)
        .expect_err("missing same-store child in wrapper should produce a CompilerError");

    assert!(
        error
            .msg
            .contains("TIR slot schema extraction: child template ID"),
        "expected schema-owned child-template error, got: {}",
        error.msg
    );
}

#[test]
fn missing_overlay_set_is_rejected_before_handoff() {
    let store = TemplateIrStore::new();
    let missing_template = TemplateIrId::new(99);
    let error = TirView::new(
        &store,
        missing_template,
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )
    .expect_err("missing root should be rejected");
    assert!(error.msg.contains("does not exist"));
}
