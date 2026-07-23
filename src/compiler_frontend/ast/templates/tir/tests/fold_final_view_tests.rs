//! Final-view TIR folding tests.
//!
//! WHAT: exercises prepared exact-view folding for final effective views rooted at
//!       control-flow bodies, aggregate wrappers, formatted text, and runtime
//!       slot application rejection. These tests prove the store-backed fold
//!       path handles the supported final-view shapes.
//!
//! WHY: production finalization folds through stable store-backed `TirView`s,
//!      so the final-view entry point needs focused coverage for those surfaces.

use crate::compiler_frontend::ast::ast_nodes::{
    Declaration, LoopBindings, RangeEndKind, RangeLoopSpec,
};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::templates::error::TemplateError;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{
    Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBranchSelector, TemplateLoopControlKind, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext, TemplateFoldResult,
};
use crate::compiler_frontend::ast::templates::tir::builder::TemplateIrBuilder;
use crate::compiler_frontend::ast::templates::tir::fold::fold_prepared_template;
use crate::compiler_frontend::ast::templates::tir::fold_cache::{TirFoldCache, TirFoldCacheKey};
use crate::compiler_frontend::ast::templates::tir::node::{
    TemplateIrBranch, TemplateIrNode, TemplateIrNodeKind,
};
use crate::compiler_frontend::ast::templates::tir::overlays::TemplateViewContext;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::ast::templates::tir::summary::TemplateIrSummary;
use crate::compiler_frontend::ast::templates::tir::view::{TemplateTirPhase, TirView};
use crate::compiler_frontend::ast::templates::tir::{
    PreparedTemplate, RuntimeTemplateReason, TemplatePreparationMode, prepare_tir_view,
};
use crate::compiler_frontend::ast::templates::{
    OwnedRuntimeSlotApplicationHandoff, OwnedRuntimeTemplateNode,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::synthetic_interface_provenance::{
    SyntheticInterfaceClass, SyntheticInterfaceMemberIdentity, SyntheticInterfaceProvenance,
};
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

/// Builds a shared store and a view over a freshly constructed template,
/// then folds it through the prepared exact-view entry point.
struct FinalViewFoldFixture {
    store: Rc<RefCell<TemplateIrStore>>,
    template_id: crate::compiler_frontend::ast::templates::tir::ids::TemplateIrId,
    context: crate::compiler_frontend::ast::templates::tir::overlays::TemplateViewContext,
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
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let context = TemplateViewContext::default();

    let template_id = {
        let mut store_borrow = store.borrow_mut();
        build_template(string_table, &mut store_borrow)
    };

    FinalViewFoldFixture {
        store,
        template_id,
        context,
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

    let store = fixture.store.borrow();
    let view = TirView::new(&store, fixture.template_id, phase, fixture.context)
        .expect("test view should construct");

    let mut fold_context =
        build_test_fold_context(string_table, &resolver, &path_format, &source_scope);

    let prepared = match prepare_tir_view(&view, TemplatePreparationMode::Value)? {
        PreparedTemplate::Foldable(prepared) => prepared,
        PreparedTemplate::Runtime(_) | PreparedTemplate::Helper(_) => {
            return Err(TemplateError::Infrastructure(Box::new(
                CompilerError::compiler_error("test view was not foldable"),
            )));
        }
    };
    // Existing final-view text assertions do not own semantic provenance.
    let TemplateFoldResult {
        emission,
        provenance: _,
    } = fold_prepared_template(&prepared, view, &mut fold_context)?;
    Ok(emission)
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
fn final_view_fold_loop_binding_provenance_reaches_exact_result() {
    let mut string_table = StringTable::new();
    let member = SyntheticInterfaceMemberIdentity::new(
        SyntheticInterfaceClass::ProjectContext,
        "render",
        "range",
    );
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let item_path = InternedPath::from_single_str("item", string_table);
        let mut builder = TemplateIrBuilder::new(store);
        let body = builder.push_dynamic_expression_node(
            Expression::reference_with_type_id(
                item_path.clone(),
                DataType::Int,
                builtin_type_ids::INT,
                empty_location(),
                ValueMode::ImmutableReference,
                ConstRecordState::RuntimeValue,
            ),
            TemplateSegmentOrigin::Body,
            None,
            empty_location(),
        );
        let range_provenance = SyntheticInterfaceProvenance::single(member.clone());
        let header = TemplateLoopHeader::Range {
            bindings: Box::new(LoopBindings {
                item: Some(Declaration {
                    id: item_path,
                    value: Expression::int(0, empty_location(), ValueMode::ImmutableOwned),
                }),
                index: None,
            }),
            range: Box::new(RangeLoopSpec {
                start: Expression::int(0, empty_location(), ValueMode::ImmutableOwned)
                    .with_synthetic_interface_provenance(range_provenance.clone()),
                end: Expression::int(2, empty_location(), ValueMode::ImmutableOwned)
                    .with_synthetic_interface_provenance(range_provenance),
                step: None,
                end_kind: RangeEndKind::Exclusive,
            }),
        };
        let root = builder.push_loop_node(header, body, None, empty_location());
        builder.finish_template(
            root,
            Style::default(),
            TemplateType::String,
            TemplateIrSummary::empty(),
            empty_location(),
        )
    });

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let store = fixture.store.borrow();
    let view = TirView::new(
        &store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("range loop view should construct");
    let prepared = match prepare_tir_view(&view, TemplatePreparationMode::Value)
        .expect("range loop view should prepare")
    {
        PreparedTemplate::Foldable(prepared) => prepared,
        PreparedTemplate::Runtime(_) | PreparedTemplate::Helper(_) => {
            panic!("range loop fixture should be foldable")
        }
    };
    let mut fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);
    let result = fold_prepared_template(&prepared, view, &mut fold_context)
        .expect("range loop exact fold should succeed");

    assert_eq!(
        emission_to_string(result.emission, &string_table),
        "01",
        "the selected range loop should render its bound values"
    );
    assert_eq!(
        result.provenance.members(),
        &[member],
        "range provenance must reach the exact folded result through the resolved binding"
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
fn final_view_fold_zero_iteration_loop_rejects_missing_body_authority() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        build_range_loop_template(
            string_table,
            store,
            0,
            0,
            crate::compiler_frontend::ast::templates::tir::ids::TemplateIrNodeId::new(999),
            None,
        )
    });

    let error = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect_err("zero-iteration loops must not hide malformed body authority");

    let TemplateError::Infrastructure(error) = error else {
        panic!("missing loop-body authority should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR preparation: node"),
        "expected a stable preparation node error, got: {}",
        error.msg
    );
}

#[test]
fn final_view_fold_loop_preserves_output_before_break_and_continue() {
    let mut string_table = StringTable::new();

    // [break] stops the loop after the first iteration, preserving only the
    // output produced before the break signal.
    let break_fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
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
    let break_emission = fold_final_view_fixture(
        &break_fixture,
        &mut string_table,
        TemplateTirPhase::Composed,
    )
    .expect("break fold should succeed");
    assert_eq!(
        emission_to_string(break_emission, &string_table),
        ".",
        "output before [break] should be preserved once and iteration should stop"
    );

    // [continue] skips the rest of the body but continues iterating, so the
    // output before the continue signal accumulates across all iterations.
    let continue_fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
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
    let continue_emission = fold_final_view_fixture(
        &continue_fixture,
        &mut string_table,
        TemplateTirPhase::Composed,
    )
    .expect("continue fold should succeed");
    assert_eq!(
        emission_to_string(continue_emission, &string_table),
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
fn final_view_fold_validates_present_aggregate_wrapper_without_body_output() {
    let mut string_table = StringTable::new();
    let fixture = build_final_view_fixture(&mut string_table, |string_table, store| {
        let mut builder = TemplateIrBuilder::new(store);
        let empty_body = builder.push_sequence_node(vec![], empty_location());
        build_range_loop_template(
            string_table,
            store,
            0,
            1,
            empty_body,
            Some(crate::compiler_frontend::ast::templates::tir::ids::TemplateIrNodeId::new(999)),
        )
    });

    let error = fold_final_view_fixture(&fixture, &mut string_table, TemplateTirPhase::Composed)
        .expect_err("a present aggregate wrapper must be validated even when the body is empty");

    let TemplateError::Infrastructure(error) = error else {
        panic!("missing aggregate-wrapper authority should remain on the infrastructure lane");
    };
    assert!(
        error.msg.contains("TIR preparation: node"),
        "expected a stable aggregate-wrapper authority error, got: {}",
        error.msg
    );
}

#[test]
fn final_view_aggregate_output_outside_wrapper_classifies_as_runtime() {
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

    // AggregateOutput outside an aggregate wrapper is not foldable: preparation
    // classifies it as runtime so the fold path is never reached.
    let store = fixture.store.borrow();
    let view = TirView::new(
        &store,
        fixture.template_id,
        TemplateTirPhase::Composed,
        fixture.context,
    )
    .expect("view should construct");
    let prepared = prepare_tir_view(&view, TemplatePreparationMode::Value)
        .expect("preparation should classify AggregateOutput outside a wrapper");
    assert!(
        matches!(
            prepared,
            PreparedTemplate::Runtime(runtime)
                if runtime.reason == RuntimeTemplateReason::AggregateOutput
        ),
        "AggregateOutput outside a wrapper should classify as runtime, got: {prepared:?}"
    );
}

// -------------------------
//  Formatted text
// -------------------------

fn build_formatted_markdown_fixture(string_table: &mut StringTable) -> FinalViewFoldFixture {
    let store = Rc::new(RefCell::new(TemplateIrStore::new()));
    let context = TemplateViewContext::default();
    let style = Style {
        formatter: Some(markdown_formatter()),
        ..Style::default()
    };

    let template_id = {
        let mut store_borrow = store.borrow_mut();
        let mut builder = TemplateIrBuilder::new(&mut store_borrow);
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
        let mut store_borrow = store.borrow_mut();
        crate::compiler_frontend::ast::templates::tir::formatter_view::format_tir_template(
            &mut store_borrow,
            template_id,
            TemplateTirPhase::Parsed,
            context,
            &style,
            string_table,
        )
        .expect("TIR formatter should succeed")
        .root
    };

    {
        let mut store_borrow = store.borrow_mut();
        store_borrow
            .templates
            .get_mut(template_id.index())
            .expect("formatted template should exist")
            .root = formatted_root;
    }

    FinalViewFoldFixture {
        store,
        template_id,
        context,
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
fn final_view_runtime_slot_application_requires_handoff() {
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

    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let store = fixture.store.borrow();
    let view = TirView::new(
        &store,
        fixture.template_id,
        TemplateTirPhase::Finalized,
        fixture.context,
    )
    .expect("final view should construct");
    let fold_context =
        build_test_fold_context(&mut string_table, &resolver, &path_format, &source_scope);

    let first = prepare_tir_view(&view, TemplatePreparationMode::Value)
        .expect("runtime slot application should prepare as runtime");
    assert!(matches!(first, PreparedTemplate::Runtime(_)));

    let key = TirFoldCacheKey {
        identity: view.identity(),
        loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings_empty: true,
    };
    assert!(
        fold_context.fold_cache.get(&key).is_none(),
        "runtime slot application must not populate the fold cache"
    );

    let second = prepare_tir_view(&view, TemplatePreparationMode::Value)
        .expect("runtime slot application should remain a runtime result");
    assert!(matches!(second, PreparedTemplate::Runtime(_)));
    assert!(fold_context.fold_cache.get(&key).is_none());
}
