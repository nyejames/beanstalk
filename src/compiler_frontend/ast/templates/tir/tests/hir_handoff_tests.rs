//! Owned HIR-handoff materialization tests.
//!
//! WHAT: checks that B5's owned runtime slot handoff can be materialized from
//! TIR without exposing store/node/slot-plan IDs.
//! WHY: the owned runtime slot handoff preserves routed source order, wrapper
//! site order, repeated slot replay, and child-template boundaries for HIR.

use super::super::ids::{ExpressionSiteId, SlotOccurrenceId, TemplateIrId, TemplateIrNodeId};
use super::super::node::{TemplateIr, TemplateIrNode, TemplateIrNodeKind, TirSlotPlaceholder};
use super::super::overlays::{TirSlotResolution, TirSlotResolutionOverlay};
use super::super::refs::{TemplateRef, TemplateStoreId, TemplateTirChildReference};
use super::super::registry::TemplateIrRegistry;

use super::super::store::TemplateIrStore;
use super::super::summary::TemplateIrSummary;
use super::super::{TemplateOverlaySet, TemplateOverlaySetId, TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_folding::TemplateFoldContext;
use crate::compiler_frontend::ast::templates::tir::TirFoldCache;
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeTemplateBody, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
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

fn test_project_path_resolver() -> ProjectPathResolver {
    let cwd = std::env::temp_dir();
    ProjectPathResolver::new(
        cwd.clone(),
        cwd,
        crate::compiler_frontend::source_libraries::root_file::PreparedSourceLibraryRoots::empty(),
        &crate::libraries::SourceFileKindRegistry::default(),
    )
    .expect("test path resolver should be valid")
}

struct TestFoldContextInputs {
    resolver: ProjectPathResolver,
    path_format: PathStringFormatConfig,
    source_scope: InternedPath,
}

impl TestFoldContextInputs {
    fn new() -> Self {
        Self {
            resolver: test_project_path_resolver(),
            path_format: PathStringFormatConfig::default(),
            source_scope: InternedPath::new(),
        }
    }

    fn context<'a>(
        &'a self,
        string_table: &'a mut StringTable,
        registry: Rc<RefCell<TemplateIrRegistry>>,
    ) -> TemplateFoldContext<'a> {
        TemplateFoldContext {
            string_table,
            project_path_resolver: &self.resolver,
            path_format_config: &self.path_format,
            source_file_scope: &self.source_scope,
            template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
            template_ir_registry: Some(registry),
            bindings: vec![],
            fold_cache: TirFoldCache::new(),
        }
    }
}

/// Pushes a literal text node into the store and returns its ID.
///
/// WHAT: builds a `Text` TIR node from a plain string.
/// WHY: tests need a concise way to create leaf text nodes without
/// repeating byte-length and origin boilerplate.
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
        TemplateIrSummary::empty(),
        empty_location(),
    ))
}

/// Builds a bool-typed reference expression for testing selector/header overrides.
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
    registry: &mut TemplateIrRegistry,
    overrides: Vec<(ExpressionSiteId, Expression)>,
) -> TemplateOverlaySetId {
    let overrides = overrides
        .into_iter()
        .map(|(site_id, expression)| (site_id, Box::new(expression)))
        .collect();
    let expression_overlay_id = registry
        .allocate_expression_overlay(super::super::overlays::TirExpressionOverlay { overrides });
    registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: Some(expression_overlay_id),
        slot_resolution: None,
        wrapper_context: None,
    })
}

/// Allocates an overlay set that resolves the given slot occurrences.
fn slot_resolution_overlay_set(
    registry: &mut TemplateIrRegistry,
    resolutions: Vec<(SlotOccurrenceId, TirSlotResolution)>,
) -> TemplateOverlaySetId {
    let slot_resolution_overlay_id =
        registry.allocate_slot_resolution_overlay(TirSlotResolutionOverlay { resolutions });
    registry.allocate_overlay_set(TemplateOverlaySet {
        expression_overrides: None,
        slot_resolution: Some(slot_resolution_overlay_id),
        wrapper_context: None,
    })
}

#[test]
fn owned_runtime_template_handoff_resolves_slot_resolution_overlay() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (parent_template_id, source_template_id, slot_occurrence_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");

        let source_text = text_node_id(&mut store, &mut string_table, "filled");
        let source_template_id = finish_text_template(&mut store, source_text);

        let slot_occurrence_id = store.next_slot_occurrence_id();
        let placeholder =
            TirSlotPlaceholder::new(SlotKey::Default, slot_occurrence_id, empty_location());
        let slot_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot { placeholder },
            empty_location(),
        ));
        let parent_template_id = finish_text_template(&mut store, slot_node);

        (parent_template_id, source_template_id, slot_occurrence_id)
    };

    let source_ref = TemplateRef::new(store_id, source_template_id);
    let overlay_set_id = slot_resolution_overlay_set(
        &mut registry,
        vec![(
            slot_occurrence_id,
            TirSlotResolution::resolved(SlotKey::Default, vec![source_ref]),
        )],
    );

    let registry = Rc::new(RefCell::new(registry));
    let store_handle = registry
        .borrow()
        .store_handle(store_id)
        .expect("registry store handle should exist");

    let registry_borrow = registry.borrow();
    let store_borrow = store_handle.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should be valid");
    drop(store_borrow);

    let fold_context_inputs = TestFoldContextInputs::new();
    let mut fold_context = fold_context_inputs.context(&mut string_table, Rc::clone(&registry));

    let handoff = store_handle
        .borrow()
        .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut fold_context)
        .expect("handoff materialization should succeed");

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = &handoff.body
    else {
        panic!(
            "expected resolved slot to materialize as a child template, got {:?}",
            handoff.body
        );
    };

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Text { text, .. }) =
        &template.body
    else {
        panic!(
            "expected source template body to materialize as text, got {:?}",
            template.body
        );
    };
    assert_eq!(fold_context.string_table.resolve(*text), "filled");
}

#[test]
fn owned_runtime_template_handoff_missing_slot_resolution_renders_slot_placeholder() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();

    let (parent_template_id, slot_occurrence_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");

        let slot_occurrence_id = store.next_slot_occurrence_id();
        let placeholder =
            TirSlotPlaceholder::new(SlotKey::Default, slot_occurrence_id, empty_location());
        let slot_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::Slot { placeholder },
            empty_location(),
        ));
        let parent_template_id = finish_text_template(&mut store, slot_node);

        (parent_template_id, slot_occurrence_id)
    };

    let overlay_set_id = slot_resolution_overlay_set(
        &mut registry,
        vec![(
            slot_occurrence_id,
            TirSlotResolution::missing(SlotKey::Default),
        )],
    );

    let registry = Rc::new(RefCell::new(registry));
    let store_handle = registry
        .borrow()
        .store_handle(store_id)
        .expect("registry store handle should exist");

    let registry_borrow = registry.borrow();
    let store_borrow = store_handle.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should be valid");
    drop(store_borrow);

    let fold_context_inputs = TestFoldContextInputs::new();
    let mut fold_context = fold_context_inputs.context(&mut string_table, Rc::clone(&registry));

    let handoff = store_handle
        .borrow()
        .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut fold_context)
        .expect("handoff materialization should succeed");

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
fn parent_root_expression_overlay_applies_inside_same_store_child() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_id = registry.allocate_store();
    let child_overlay_set_id = registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let (parent_template_id, child_site_id) = {
        let mut store = registry
            .store_mut(store_id)
            .expect("registry store should be mutable");

        let child_site_id = store.next_expression_site_id();
        let child_expression = bool_reference_expression(&mut string_table, "original");
        let child_root = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(child_expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id: child_site_id,
            },
            empty_location(),
        ));
        let child_template_id = finish_text_template(&mut store, child_root);
        let child_node = child_template_node_id(
            &mut store,
            child_reference(store_id, child_template_id, child_overlay_set_id),
        );
        let parent_template_id = finish_text_template(&mut store, child_node);

        (parent_template_id, child_site_id)
    };

    let parent_overlay_set_id = expression_overlay_set(
        &mut registry,
        vec![(
            child_site_id,
            Expression::bool(true, empty_location(), ValueMode::ImmutableOwned),
        )],
    );
    let body = materialize_parent_handoff(
        registry,
        store_id,
        parent_template_id,
        &mut string_table,
        parent_overlay_set_id,
    );

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

// ---------------------------------------------------------------------------
//  Cross-store child materialization tests
// ---------------------------------------------------------------------------

/// Two-store registry fixture for cross-store child materialization tests.
struct CrossStoreFixture {
    registry: TemplateIrRegistry,
    store_a_id: TemplateStoreId,
    store_b_id: TemplateStoreId,
}

impl CrossStoreFixture {
    fn new() -> Self {
        let mut registry = TemplateIrRegistry::new();
        let store_a_id = registry.allocate_store();
        let store_b_id = registry.allocate_store();
        // TirView validates that the overlay-set ID exists, so pre-allocate
        // the canonical empty set before any view is constructed.
        registry.allocate_overlay_set(TemplateOverlaySet::empty());
        Self {
            registry,
            store_a_id,
            store_b_id,
        }
    }
}

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

fn child_reference(
    store_id: TemplateStoreId,
    template_id: TemplateIrId,
    overlay_set_id: TemplateOverlaySetId,
) -> TemplateTirChildReference {
    TemplateTirChildReference::new(
        TemplateRef::new(store_id, template_id),
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
}

fn push_text_template(
    store: &mut TemplateIrStore,
    string_table: &mut StringTable,
    text: &str,
) -> TemplateIrId {
    let text_node = text_node_id(store, string_table, text);
    finish_text_template(store, text_node)
}

/// Materializes the parent template through the fold-context entry point,
/// returning the full `Result` so success tests can unwrap and error tests
/// can assert on the `CompilerError`.
fn materialize_parent_handoff_result(
    fixture_registry: TemplateIrRegistry,
    store_id: TemplateStoreId,
    parent_template_id: TemplateIrId,
    string_table: &mut StringTable,
    overlay_set_id: TemplateOverlaySetId,
) -> Result<OwnedRuntimeTemplateBody, CompilerError> {
    let registry = Rc::new(RefCell::new(fixture_registry));
    let store_handle = registry
        .borrow()
        .store_handle(store_id)
        .expect("registry store handle should exist");

    let registry_borrow = registry.borrow();
    let store_borrow = store_handle.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        TemplateRef::new(store_id, parent_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        overlay_set_id,
    )
    .expect("test view should be valid");
    drop(store_borrow);

    let fold_context_inputs = TestFoldContextInputs::new();
    let mut fold_context = fold_context_inputs.context(string_table, Rc::clone(&registry));

    store_handle
        .borrow()
        .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut fold_context)
        .map(|handoff| handoff.body)
}

/// Convenience wrapper for success-path tests that expect materialization to
/// succeed.
fn materialize_parent_handoff(
    fixture_registry: TemplateIrRegistry,
    store_id: TemplateStoreId,
    parent_template_id: TemplateIrId,
    string_table: &mut StringTable,
    overlay_set_id: TemplateOverlaySetId,
) -> OwnedRuntimeTemplateBody {
    materialize_parent_handoff_result(
        fixture_registry,
        store_id,
        parent_template_id,
        string_table,
        overlay_set_id,
    )
    .expect("handoff materialization should succeed")
}

#[test]
fn foreign_child_materialized_through_owning_store() {
    let mut string_table = StringTable::new();
    let fixture = CrossStoreFixture::new();

    let child_template_id = {
        let mut store_b = fixture
            .registry
            .store_mut(fixture.store_b_id)
            .expect("store B should be mutable");
        push_text_template(&mut store_b, &mut string_table, "from B")
    };

    let parent_template_id = {
        let mut store_a = fixture
            .registry
            .store_mut(fixture.store_a_id)
            .expect("store A should be mutable");
        let child_node = child_template_node_id(
            &mut store_a,
            child_reference(
                fixture.store_b_id,
                child_template_id,
                TemplateOverlaySetId::empty(),
            ),
        );
        finish_text_template(&mut store_a, child_node)
    };

    let body = materialize_parent_handoff(
        fixture.registry,
        fixture.store_a_id,
        parent_template_id,
        &mut string_table,
        TemplateOverlaySetId::empty(),
    );

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = body
    else {
        panic!("expected cross-store child to materialize as ChildTemplate, got {body:?}");
    };

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::Text { text, .. }) =
        &template.body
    else {
        panic!(
            "expected foreign template body to materialize as text, got {:?}",
            template.body
        );
    };
    assert_eq!(string_table.resolve(*text), "from B");
}

#[test]
fn foreign_child_preserves_expression_overlay() {
    let mut string_table = StringTable::new();
    let mut fixture = CrossStoreFixture::new();

    let (child_template_id, expression_site_id) = {
        let mut store_b = fixture
            .registry
            .store_mut(fixture.store_b_id)
            .expect("store B should be mutable");
        let site_id = store_b.next_expression_site_id();
        let expression = bool_reference_expression(&mut string_table, "original");
        let expr_node = store_b.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::DynamicExpression {
                expression: Box::new(expression),
                origin: TemplateSegmentOrigin::Body,
                reactive_subscription: None,
                site_id,
            },
            empty_location(),
        ));
        (finish_text_template(&mut store_b, expr_node), site_id)
    };

    let overlay_set_id = expression_overlay_set(
        &mut fixture.registry,
        vec![(
            expression_site_id,
            Expression::bool(true, empty_location(), ValueMode::ImmutableOwned),
        )],
    );

    let parent_template_id = {
        let mut store_a = fixture
            .registry
            .store_mut(fixture.store_a_id)
            .expect("store A should be mutable");
        let child_node = child_template_node_id(
            &mut store_a,
            child_reference(fixture.store_b_id, child_template_id, overlay_set_id),
        );
        finish_text_template(&mut store_a, child_node)
    };

    let body = materialize_parent_handoff(
        fixture.registry,
        fixture.store_a_id,
        parent_template_id,
        &mut string_table,
        TemplateOverlaySetId::empty(),
    );

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::ChildTemplate {
        template, ..
    }) = body
    else {
        panic!("expected cross-store child to materialize as ChildTemplate, got {body:?}");
    };

    let OwnedRuntimeTemplateBody::Render(OwnedRuntimeTemplateNode::DynamicExpression {
        expression,
        ..
    }) = &template.body
    else {
        panic!(
            "expected foreign template body to materialize as dynamic expression, got {:?}",
            template.body
        )
    };
    assert!(
        matches!(expression.kind, ExpressionKind::Bool(true)),
        "child expression should be overridden to true, got {:?}",
        expression.kind
    );
}

#[test]
fn foreign_child_missing_registry_returns_error() {
    let mut string_table = StringTable::new();
    let mut registry = TemplateIrRegistry::new();
    let store_a_id = registry.allocate_store();
    let store_b_id = registry.allocate_store();
    registry.allocate_overlay_set(TemplateOverlaySet::empty());

    let child_template_id = {
        let mut store_b = registry
            .store_mut(store_b_id)
            .expect("store B should be mutable");
        push_text_template(&mut store_b, &mut string_table, "from B")
    };

    let parent_template_id = {
        let mut store_a = registry
            .store_mut(store_a_id)
            .expect("store A should be mutable");
        let child_node = child_template_node_id(
            &mut store_a,
            child_reference(store_b_id, child_template_id, TemplateOverlaySetId::empty()),
        );
        finish_text_template(&mut store_a, child_node)
    };

    // This entry point creates a materializer without a registry, so the
    // cross-store child cannot be resolved.
    let store_a_handle = registry
        .store_handle(store_a_id)
        .expect("registry store handle should exist");

    let error = store_a_handle
        .borrow()
        .owned_runtime_template_handoff_for_template(parent_template_id)
        .expect_err("missing registry should produce an error");

    assert_eq!(
        error.msg,
        "TIR HIR handoff: cross-store child template requires a registry, but none is available."
    );
}

#[test]
fn foreign_child_missing_store_returns_error() {
    let mut string_table = StringTable::new();
    let fixture = CrossStoreFixture::new();

    let phantom_store_id = TemplateStoreId::new(99);

    let parent_template_id = {
        let mut store_a = fixture
            .registry
            .store_mut(fixture.store_a_id)
            .expect("store A should be mutable");
        let child_node = child_template_node_id(
            &mut store_a,
            child_reference(
                phantom_store_id,
                TemplateIrId::new(0),
                TemplateOverlaySetId::empty(),
            ),
        );
        finish_text_template(&mut store_a, child_node)
    };

    let error = materialize_parent_handoff_result(
        fixture.registry,
        fixture.store_a_id,
        parent_template_id,
        &mut string_table,
        TemplateOverlaySetId::empty(),
    )
    .expect_err("missing store should produce an error");

    assert_eq!(
        error.msg,
        format!(
            "TIR HIR handoff: cross-store child template store {} not found in registry.",
            phantom_store_id
        )
    );
}

#[test]
fn foreign_child_missing_template_returns_error() {
    let mut string_table = StringTable::new();
    let fixture = CrossStoreFixture::new();

    let parent_template_id = {
        let mut store_a = fixture
            .registry
            .store_mut(fixture.store_a_id)
            .expect("store A should be mutable");
        let child_node = child_template_node_id(
            &mut store_a,
            child_reference(
                fixture.store_b_id,
                TemplateIrId::new(0),
                TemplateOverlaySetId::empty(),
            ),
        );
        finish_text_template(&mut store_a, child_node)
    };

    let error = materialize_parent_handoff_result(
        fixture.registry,
        fixture.store_a_id,
        parent_template_id,
        &mut string_table,
        TemplateOverlaySetId::empty(),
    )
    .expect_err("missing template should produce an error");

    assert_eq!(
        error.msg,
        format!(
            "TIR HIR handoff: cross-store child template {} not found in store {}.",
            TemplateIrId::new(0),
            fixture.store_b_id
        )
    );
}

#[test]
fn qualified_cross_store_child_cycle_rejected() {
    let mut string_table = StringTable::new();
    let fixture = CrossStoreFixture::new();

    // Mutual cross-store cycle: store A references store B's template 0, and
    // store B references store A's parent template back. Child refs are stored
    // unvalidated; cross-store cycle detection fires at materialization time.
    let parent_template_id = {
        let mut store_a = fixture
            .registry
            .store_mut(fixture.store_a_id)
            .expect("store A should be mutable");
        let child_node = child_template_node_id(
            &mut store_a,
            child_reference(
                fixture.store_b_id,
                TemplateIrId::new(0),
                TemplateOverlaySetId::empty(),
            ),
        );
        finish_text_template(&mut store_a, child_node)
    };

    {
        let mut store_b = fixture
            .registry
            .store_mut(fixture.store_b_id)
            .expect("store B should be mutable");
        let back_child_node = child_template_node_id(
            &mut store_b,
            child_reference(
                fixture.store_a_id,
                parent_template_id,
                TemplateOverlaySetId::empty(),
            ),
        );
        finish_text_template(&mut store_b, back_child_node);
    };

    let error = materialize_parent_handoff_result(
        fixture.registry,
        fixture.store_a_id,
        parent_template_id,
        &mut string_table,
        TemplateOverlaySetId::empty(),
    )
    .expect_err("cross-store cycle should produce an error");

    assert_eq!(
        error.msg,
        "TIR HIR handoff materialization found a recursive child template."
    );
}

// -------------------------
//  Strict Store Boundaries
// -------------------------

/// A view whose root store ID differs from the materializing store must be
/// rejected as a `CompilerError`, not silently skipped as `Ok(None)`.
#[test]
fn view_backed_handoff_rejects_wrong_direct_store() {
    let mut string_table = StringTable::new();
    let fixture = CrossStoreFixture::new();

    let store_a_template_id = {
        let mut store_a = fixture
            .registry
            .store_mut(fixture.store_a_id)
            .expect("store A should be mutable");
        push_text_template(&mut store_a, &mut string_table, "a")
    };

    let registry = Rc::new(RefCell::new(fixture.registry));
    let store_b_handle = registry
        .borrow()
        .store_handle(fixture.store_b_id)
        .expect("store B handle should exist");

    let registry_borrow = registry.borrow();
    let view = TirView::with_minimum_phase(
        &registry_borrow,
        TemplateRef::new(fixture.store_a_id, store_a_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )
    .expect("test view should be valid");

    let fold_context_inputs = TestFoldContextInputs::new();
    let mut fold_context = fold_context_inputs.context(&mut string_table, Rc::clone(&registry));

    let error = store_b_handle
        .borrow()
        .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut fold_context)
        .expect_err("wrong-store view should produce a CompilerError");

    assert_eq!(
        error.msg,
        "TIR HIR handoff view materialization view store does not match the supplied store."
    );
}

/// A same-ID foreign-store collision with matching local template and root IDs
/// must still be rejected through exact store ownership.
#[test]
fn view_backed_handoff_rejects_same_id_foreign_store_collision() {
    let mut string_table = StringTable::new();

    // Independent registries can allocate identical store, template and node
    // IDs. The logical owner token must distinguish their actual stores.
    let mut registry_a = TemplateIrRegistry::new();
    let store_a_id = registry_a.allocate_store();
    registry_a.allocate_overlay_set(TemplateOverlaySet::empty());
    let store_a_template_id = {
        let mut store_a = registry_a
            .store_mut(store_a_id)
            .expect("store A should be mutable");
        push_text_template(&mut store_a, &mut string_table, "a")
    };

    let mut registry_b = TemplateIrRegistry::new();
    let store_b_id = registry_b.allocate_store();
    let store_b_template_id = {
        let mut store_b = registry_b
            .store_mut(store_b_id)
            .expect("store B should be mutable");
        push_text_template(&mut store_b, &mut string_table, "b")
    };

    assert_eq!(
        store_a_id, store_b_id,
        "test setup: both stores should share the same store ID index"
    );
    assert_eq!(
        store_a_template_id, store_b_template_id,
        "test setup: both stores should share the same template ID index"
    );

    let registry_a_rc = Rc::new(RefCell::new(registry_a));
    let registry_b_rc = Rc::new(RefCell::new(registry_b));
    let store_b_handle = registry_b_rc
        .borrow()
        .store_handle(store_b_id)
        .expect("store B handle should exist");

    let registry_a_borrow = registry_a_rc.borrow();
    let view = TirView::with_minimum_phase(
        &registry_a_borrow,
        TemplateRef::new(store_a_id, store_a_template_id),
        TemplateTirPhase::Finalized,
        TemplateTirPhase::Finalized,
        TemplateOverlaySetId::empty(),
    )
    .expect("test view should be valid");

    let fold_context_inputs = TestFoldContextInputs::new();
    let mut fold_context =
        fold_context_inputs.context(&mut string_table, Rc::clone(&registry_a_rc));

    let error = store_b_handle
        .borrow()
        .owned_runtime_template_handoff_for_tir_view_with_fold_context(&view, &mut fold_context)
        .expect_err("same-ID foreign-store collision should produce a CompilerError");

    assert_eq!(
        error.msg,
        "TIR HIR handoff view materialization registered store does not match the supplied store."
    );
}
