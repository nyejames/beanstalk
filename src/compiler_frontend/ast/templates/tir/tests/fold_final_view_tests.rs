//! Final-view TIR folding tests.
//!
//! WHAT: exercises `fold_tir_view` for final effective views rooted at
//!       control-flow bodies, aggregate wrappers, formatted text, and foldable
//!       runtime slot applications. These tests prove the registry-backed fold
//!       path handles the shapes that `try_classify_final_effective_template_view`
//!       deems sufficient.
//!
//! WHY: production finalization folds through stable registry-backed `TirView`s,
//!      so the final-view entry point needs focused coverage for those surfaces.

use crate::compiler_frontend::ast::ast_nodes::{LoopBindings, RangeEndKind, RangeLoopSpec};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext,
};
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold::fold_tir_view;
use crate::compiler_frontend::ast::templates::tir::fold_cache::TirFoldCache;
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::TemplateOverlaySet;
use crate::compiler_frontend::ast::templates::tir::registry::TemplateIrRegistry;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::tir::{TemplateRef, format_tir_template};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeTemplateNode,
};
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

fn int_expression(value: i32) -> Expression {
    Expression::int(value, empty_location(), ValueMode::ImmutableOwned)
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

fn emission_to_string(emission: TemplateEmission, string_table: &StringTable) -> String {
    match emission {
        TemplateEmission::NoOutput => String::new(),
        TemplateEmission::Output(output) => string_table.resolve(output).to_owned(),
        TemplateEmission::Break(Some(output)) | TemplateEmission::Continue(Some(output)) => {
            string_table.resolve(output).to_owned()
        }
        TemplateEmission::Break(None) | TemplateEmission::Continue(None) => String::new(),
    }
}

/// Builds a registry and a view over a freshly constructed same-store template,
/// then folds it through `fold_tir_view`.
struct FinalViewFoldFixture {
    registry: Rc<RefCell<TemplateIrRegistry>>,
    store_id: crate::compiler_frontend::ast::templates::tir::refs::TemplateStoreId,
    template_id: crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    overlay_set_id: crate::compiler_frontend::ast::templates::tir::overlays::TemplateOverlaySetId,
}

fn build_final_view_fixture<F>(
    string_table: &mut StringTable,
    build_template: F,
) -> FinalViewFoldFixture
where
    F: FnOnce(
        &mut StringTable,
        &mut TemplateIrStore,
    ) -> crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
{
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();
    let overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());

    let template_id = {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");
        build_template(string_table, &mut store)
    };

    FinalViewFoldFixture {
        registry,
        store_id,
        template_id,
        overlay_set_id,
    }
}

fn fold_final_view_fixture(
    fixture: &FinalViewFoldFixture,
    string_table: &mut StringTable,
    phase: TemplateTirPhase,
) -> Result<TemplateEmission, TemplateError> {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();

    let registry_borrow = fixture.registry.borrow();
    let view = TirView::new(
        &registry_borrow,
        TemplateRef::new(fixture.store_id, fixture.template_id),
        phase,
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

// -------------------------
//  Branch/fallback bodies
// -------------------------

#[test]
fn final_view_fold_branch_selects_body() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let yes_text = string_table.intern("yes");
        let yes_node =
            builder.push_text_node(yes_text, 3, TemplateSegmentOrigin::Body, empty_location());
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(bool_expression(true)),
            yes_node,
            empty_location(),
        );
        let root = builder.push_branch_chain_node(vec![branch], None, empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "yes",
        "true branch body should be selected through the final view"
    );
}

#[test]
fn final_view_fold_false_branch_no_else_is_no_output() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let yes_text = string_table.intern("yes");
        let yes_node =
            builder.push_text_node(yes_text, 3, TemplateSegmentOrigin::Body, empty_location());
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(bool_expression(false)),
            yes_node,
            empty_location(),
        );
        let root = builder.push_branch_chain_node(vec![branch], None, empty_location());

        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission,
        TemplateEmission::NoOutput,
        "false branch with no else should produce structural no-output"
    );
}

#[test]
fn final_view_fold_false_branch_selects_fallback() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let yes_text = string_table.intern("yes");
        let no_text = string_table.intern("no");
        let yes_node =
            builder.push_text_node(yes_text, 3, TemplateSegmentOrigin::Body, empty_location());
        let fallback_node =
            builder.push_text_node(no_text, 2, TemplateSegmentOrigin::Body, empty_location());
        let branch = TemplateIrBranch::new(
            TemplateBranchSelector::Bool(bool_expression(false)),
            yes_node,
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
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "no",
        "fallback body should be selected when all branches are false"
    );
}

// -------------------------
//  Loop bodies
// -------------------------

fn build_range_loop_template(
    _string_table: &mut StringTable,
    store: &mut TemplateIrStore,
    start: i32,
    end: i32,
    body_root: crate::compiler_frontend::ast::templates::tir::ids::TemplateIrNodeId,
    aggregate_wrapper: Option<crate::compiler_frontend::ast::templates::tir::ids::TemplateIrNodeId>,
) -> crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId {
    let mut builder = TemplateIrBuilder::new(store);
    let header = TemplateLoopHeader::Range {
        bindings: Box::new(LoopBindings {
            item: None,
            index: None,
        }),
        range: Box::new(RangeLoopSpec {
            start: int_expression(start),
            end: int_expression(end),
            step: None,
            end_kind: RangeEndKind::Exclusive,
        }),
    };
    let root = builder.push_loop_node(header, body_root, aggregate_wrapper, empty_location());

    builder.finish_template(
        root,
        Style::default(),
        TemplateType::String,
        TemplateIrSummary::empty(),
        empty_location(),
    )
}

#[test]
fn final_view_fold_loop_body_concatenates_iterations() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let dot_text = string_table.intern(".");
        let dot_node =
            builder.push_text_node(dot_text, 1, TemplateSegmentOrigin::Body, empty_location());
        build_range_loop_template(string_table, store, 0, 3, dot_node, None)
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "...",
        "range loop body should be repeated through the final view"
    );
}

#[test]
fn final_view_fold_zero_iteration_loop_is_no_output() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let dot_text = string_table.intern(".");
        let dot_node =
            builder.push_text_node(dot_text, 1, TemplateSegmentOrigin::Body, empty_location());
        build_range_loop_template(string_table, store, 0, 0, dot_node, None)
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission,
        TemplateEmission::NoOutput,
        "zero-iteration loop should produce structural no-output"
    );
}

#[test]
fn final_view_fold_loop_preserves_output_before_break() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let dot_text = string_table.intern(".");
        let after_text = string_table.intern("after");
        let dot_node =
            builder.push_text_node(dot_text, 1, TemplateSegmentOrigin::Body, empty_location());
        let break_node =
            builder.push_loop_control_node(TemplateLoopControlKind::Break, empty_location());
        let after_node =
            builder.push_text_node(after_text, 5, TemplateSegmentOrigin::Body, empty_location());
        let body_root =
            builder.push_sequence_node(vec![dot_node, break_node, after_node], empty_location());
        build_range_loop_template(string_table, store, 0, 3, body_root, None)
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        ".",
        "output before [break] should be preserved and iteration should stop"
    );
}

#[test]
fn final_view_fold_loop_preserves_output_before_continue() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let dot_text = string_table.intern(".");
        let after_text = string_table.intern("after");
        let dot_node =
            builder.push_text_node(dot_text, 1, TemplateSegmentOrigin::Body, empty_location());
        let continue_node =
            builder.push_loop_control_node(TemplateLoopControlKind::Continue, empty_location());
        let after_node =
            builder.push_text_node(after_text, 5, TemplateSegmentOrigin::Body, empty_location());
        let body_root =
            builder.push_sequence_node(vec![dot_node, continue_node, after_node], empty_location());
        build_range_loop_template(string_table, store, 0, 3, body_root, None)
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "...",
        "output before [continue] should be preserved each iteration"
    );
}

// -------------------------
//  Aggregate wrapper root
// -------------------------

#[test]
fn final_view_fold_aggregate_wrapper_preserves_aggregate_output_position() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let aggregate_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::AggregateOutput,
            empty_location(),
        ));

        let mut builder = TemplateIrBuilder::new(store);
        let open_text = string_table.intern("[");
        let close_text = string_table.intern("]");
        let x_text = string_table.intern("x");

        let open_node =
            builder.push_text_node(open_text, 1, TemplateSegmentOrigin::Body, empty_location());
        let close_node =
            builder.push_text_node(close_text, 1, TemplateSegmentOrigin::Body, empty_location());
        let wrapper_root = builder.push_sequence_node(
            vec![open_node, aggregate_node, close_node],
            empty_location(),
        );

        let body_node =
            builder.push_text_node(x_text, 1, TemplateSegmentOrigin::Body, empty_location());
        build_range_loop_template(string_table, store, 0, 3, body_node, Some(wrapper_root))
    });

    let emission = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect("final view fold should succeed");

    assert_eq!(
        emission_to_string(emission, &string_table),
        "[xxx]",
        "aggregate wrapper should replace AggregateOutput with the folded aggregate"
    );
}

#[test]
fn final_view_fold_aggregate_output_outside_wrapper_is_error() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |_string_table, store| {
        let aggregate_node = store.push_node(TemplateIrNode::new(
            TemplateIrNodeKind::AggregateOutput,
            empty_location(),
        ));

        let mut builder = TemplateIrBuilder::new(store);
        builder.finish_template(
            aggregate_node,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    });

    let result = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed);

    assert!(
        result.is_err(),
        "AggregateOutput outside an aggregate wrapper should be a fold error"
    );
}

// -------------------------
//  Formatted text
// -------------------------

fn build_formatted_markdown_fixture(string_table: &mut StringTable) -> FinalViewFoldFixture {
    let registry = Rc::new(RefCell::new(TemplateIrRegistry::new()));
    let store_id = registry.borrow_mut().allocate_store();
    let overlay_set_id = registry
        .borrow_mut()
        .allocate_overlay_set(TemplateOverlaySet::empty());
    let style = Style {
        formatter: Some(markdown_formatter()),
        ..Style::default()
    };

    let template_id = {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");
        let mut builder = TemplateIrBuilder::new(&mut store);
        let text = string_table.intern("Hello `code`");
        let root = builder.push_text_node(
            text,
            "Hello `code`".len() as u32,
            TemplateSegmentOrigin::Body,
            empty_location(),
        );
        builder.finish_template(
            root,
            style.clone(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    };

    let formatted_root = {
        let registry_borrow = registry.borrow();
        let view = TirView::new(
            &registry_borrow,
            TemplateRef::new(store_id, template_id),
            TemplateTirPhase::Parsed,
            overlay_set_id,
        )
        .expect("parsed view should construct");
        format_tir_template(&view, &style, string_table)
            .expect("TIR formatter should succeed")
            .root
    };

    {
        let registry_borrow = registry.borrow_mut();
        let mut store = registry_borrow
            .store_mut(store_id)
            .expect("store should exist");
        store
            .templates
            .get_mut(template_id.index())
            .expect("formatted template should exist")
            .root = formatted_root;
    }

    FinalViewFoldFixture {
        registry,
        store_id,
        template_id,
        overlay_set_id,
    }
}

#[test]
fn final_view_fold_formatted_markdown_text() {
    let mut string_table = StringTable::new();
    let fixture = build_formatted_markdown_fixture(&mut string_table);
    let emission =
        fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Formatted)
            .expect("formatted final view should fold");

    let output = emission_to_string(emission, &string_table);
    assert!(
        output.contains("<code>code</code>"),
        "formatted markdown should fold to rendered HTML, got: {}",
        output
    );
}

// -------------------------
//  Runtime slot applications
// -------------------------

#[test]
fn final_view_fold_runtime_slot_application_is_no_output() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let wrapper_text = string_table.intern("<shell>");
        let handoff = OwnedRuntimeSlotApplicationHandoff {
            wrapper: OwnedRuntimeTemplateNode::Text {
                text: wrapper_text,
                byte_len: "<shell>".len() as u32,
                reactive_subscription: None,
                location: empty_location(),
            },
            contribution_sources: Vec::new(),
            slot_sites: Vec::new(),
            location: empty_location(),
        };
        let expression =
            Expression::runtime_slot_application_handoff(handoff, ValueMode::ImmutableOwned);
        let dynamic_node = builder.push_dynamic_expression_node(
            expression,
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );

        builder.finish_template(
            dynamic_node,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    });

    let emission =
        fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Finalized)
            .expect("final view fold should succeed");

    assert_eq!(
        emission,
        TemplateEmission::NoOutput,
        "runtime slot application should fold to no output in a const context"
    );
}
