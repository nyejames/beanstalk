//! TIR fold parity tests.
//!
//! WHAT: compares the legacy render-plan fold output with the new TIR-native
//! fold output for a representative set of template shapes.
//!
//! WHY: Phase B2 routes production folding through TIR. These tests prove that
//! the new path preserves the old path's output byte-for-byte for the shapes
//! that matter most: text, sequences, scalar expressions, branches, loops,
//! nested children, slots, and aggregate wrappers.

use crate::compiler_frontend::ast::ast_nodes::LoopBindings;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::template::{
    SlotKey, SlotPlaceholder, TemplateAtom, TemplateContent, TemplateSegment, TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateAggregatePiece, TemplateAggregateRenderPlan, TemplateBranchChain,
    TemplateBranchSelector, TemplateConditionalBranch, TemplateControlFlow, TemplateFallbackBranch,
    TemplateLoopControlFlow, TemplateLoopHeader,
};
use crate::compiler_frontend::ast::templates::template_folding::{
    TemplateEmission, TemplateFoldContext, apply_conditional_child_wrappers, fold_control_flow,
    fold_plan,
};
use crate::compiler_frontend::ast::templates::template_render_plan::TemplateRenderPlan;
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::templates::tir::convert_from_template::convert_template_to_tir;
use crate::compiler_frontend::ast::templates::tir::fold::fold_tir_template;
use crate::compiler_frontend::ast::templates::tir::store::TemplateIrStore;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::builtin_type_ids;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS;

// -------------------------
//  Test helpers
// -------------------------

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
) -> TemplateFoldContext<'a> {
    TemplateFoldContext {
        string_table,
        project_path_resolver: resolver,
        path_format_config: path_format,
        source_file_scope: source_scope,
        template_const_loop_iteration_limit: DEFAULT_TEMPLATE_CONST_LOOP_ITERATIONS,
        bindings: vec![],
    }
}

fn string_expression(string_table: &mut StringTable, text: &str) -> Expression {
    Expression::string_slice(
        string_table.intern(text),
        empty_location(),
        ValueMode::ImmutableOwned,
    )
}

fn int_expression(value: i32) -> Expression {
    Expression::int(value, empty_location(), ValueMode::ImmutableOwned)
}

fn bool_expression(value: bool) -> Expression {
    Expression::bool(value, empty_location(), ValueMode::ImmutableOwned)
}

fn make_template(content: TemplateContent) -> Template {
    let mut template = Template::empty();
    template.content = content;
    template.location = empty_location();
    template
}

fn identity_aggregate_plan() -> TemplateAggregateRenderPlan {
    TemplateAggregateRenderPlan {
        pieces: vec![TemplateAggregatePiece::Aggregate],
    }
}

fn make_template_with_control_flow(
    content: TemplateContent,
    control_flow: TemplateControlFlow,
) -> Template {
    let mut template = Template::empty();
    template.content = content;
    template.control_flow = Some(control_flow);
    template.location = empty_location();
    template
}

fn list_item_wrapper(string_table: &mut StringTable) -> Template {
    make_template(TemplateContent {
        atoms: vec![
            TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "<li>"),
                TemplateSegmentOrigin::Body,
            )),
            TemplateAtom::Slot(SlotPlaceholder::new(SlotKey::Default)),
            TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "</li>"),
                TemplateSegmentOrigin::Body,
            )),
        ],
    })
}

/// Folds a template through the legacy render-plan path using the provided
/// string table (which must already contain any strings referenced by the
/// template).
fn fold_template_via_old_path(template: &Template, string_table: &mut StringTable) -> String {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(string_table, &resolver, &path_format, &source_scope);

    match &template.control_flow {
        None => {
            let plan = TemplateRenderPlan::from_content(&template.content);
            let output = fold_plan(&plan, &mut fold_context).expect("old path should fold");
            fold_context.string_table.resolve(output).to_owned()
        }
        Some(control_flow) => {
            let emission = fold_control_flow(control_flow, &mut fold_context)
                .expect("old control-flow path should fold");
            let emission = apply_conditional_child_wrappers(template, emission, &mut fold_context)
                .expect("old wrapper application should succeed");
            emission_to_string(emission, &mut fold_context)
        }
    }
}

/// Folds a template through the new TIR path using the provided string table.
fn fold_template_via_tir(template: &Template, string_table: &mut StringTable) -> String {
    let resolver = test_project_path_resolver();
    let path_format = PathStringFormatConfig::default();
    let source_scope = InternedPath::new();
    let mut fold_context =
        build_test_fold_context(string_table, &resolver, &path_format, &source_scope);

    let mut store = TemplateIrStore::new();
    let tir_id = convert_template_to_tir(template, &mut store, fold_context.string_table);
    let emission =
        fold_tir_template(&store, tir_id, &mut fold_context).expect("TIR path should fold");
    emission_to_string(emission, &mut fold_context)
}

fn emission_to_string(
    emission: TemplateEmission,
    fold_context: &mut TemplateFoldContext<'_>,
) -> String {
    match emission {
        TemplateEmission::NoOutput => String::new(),
        TemplateEmission::Output(output) => fold_context.string_table.resolve(output).to_owned(),
        TemplateEmission::Break(Some(output)) | TemplateEmission::Continue(Some(output)) => {
            fold_context.string_table.resolve(output).to_owned()
        }
        TemplateEmission::Break(None) | TemplateEmission::Continue(None) => String::new(),
    }
}

fn assert_fold_parity<F>(build_template: F)
where
    F: Fn(&mut StringTable) -> Template,
{
    let mut old_string_table = StringTable::new();
    let old_template = build_template(&mut old_string_table);
    let old_output = fold_template_via_old_path(&old_template, &mut old_string_table);

    let mut new_string_table = StringTable::new();
    let new_template = build_template(&mut new_string_table);
    let new_output = fold_template_via_tir(&new_template, &mut new_string_table);

    assert_eq!(
        old_output, new_output,
        "TIR fold output must match legacy render-plan fold output"
    );
}

// -------------------------
//  Plain text parity
// -------------------------

#[test]
fn parity_single_text_atom() {
    assert_fold_parity(|string_table| {
        make_template(TemplateContent {
            atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "hello"),
                TemplateSegmentOrigin::Body,
            ))],
        })
    });
}

#[test]
fn parity_multiple_text_atoms() {
    assert_fold_parity(|string_table| {
        make_template(TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, "hello "),
                    TemplateSegmentOrigin::Body,
                )),
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, "world"),
                    TemplateSegmentOrigin::Body,
                )),
            ],
        })
    });
}

// -------------------------
//  Scalar expression parity
// -------------------------

#[test]
fn parity_int_expression() {
    let content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            int_expression(42),
            TemplateSegmentOrigin::Body,
        ))],
    };

    assert_fold_parity(|_| make_template(content.clone()));
}

#[test]
fn parity_bool_expression() {
    let content = TemplateContent {
        atoms: vec![TemplateAtom::Content(TemplateSegment::new(
            bool_expression(true),
            TemplateSegmentOrigin::Body,
        ))],
    };

    assert_fold_parity(|_| make_template(content.clone()));
}

// -------------------------
//  Branch-chain parity
// -------------------------

#[test]
fn parity_bool_branch_true() {
    assert_fold_parity(|string_table| {
        let branch_chain = TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                selector: TemplateBranchSelector::Bool(bool_expression(true)),
                content: TemplateContent {
                    atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                        string_expression(string_table, "yes"),
                        TemplateSegmentOrigin::Body,
                    ))],
                },
                render_plan: None,
                location: empty_location(),
            }],
            fallback: None,
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::BranchChain(Box::new(branch_chain)),
        )
    });
}

#[test]
fn parity_bool_branch_false_with_fallback() {
    assert_fold_parity(|string_table| {
        let branch_chain = TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                selector: TemplateBranchSelector::Bool(bool_expression(false)),
                content: TemplateContent {
                    atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                        string_expression(string_table, "yes"),
                        TemplateSegmentOrigin::Body,
                    ))],
                },
                render_plan: None,
                location: empty_location(),
            }],
            fallback: Some(TemplateFallbackBranch {
                content: TemplateContent {
                    atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                        string_expression(string_table, "no"),
                        TemplateSegmentOrigin::Body,
                    ))],
                },
                render_plan: None,
                location: empty_location(),
            }),
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::BranchChain(Box::new(branch_chain)),
        )
    });
}

#[test]
fn parity_bool_branch_false_no_fallback() {
    assert_fold_parity(|string_table| {
        let branch_chain = TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                selector: TemplateBranchSelector::Bool(bool_expression(false)),
                content: TemplateContent {
                    atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                        string_expression(string_table, "yes"),
                        TemplateSegmentOrigin::Body,
                    ))],
                },
                render_plan: None,
                location: empty_location(),
            }],
            fallback: None,
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::BranchChain(Box::new(branch_chain)),
        )
    });
}

// -------------------------
//  Loop parity
// -------------------------

#[test]
fn parity_range_loop() {
    assert_fold_parity(|string_table| {
        let body = TemplateContent {
            atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "."),
                TemplateSegmentOrigin::Body,
            ))],
        };

        let loop_cf = TemplateLoopControlFlow {
            header: TemplateLoopHeader::Range {
                bindings: Box::new(LoopBindings {
                    item: None,
                    index: None,
                }),
                range: Box::new(crate::compiler_frontend::ast::ast_nodes::RangeLoopSpec {
                    start: int_expression(0),
                    end: int_expression(3),
                    step: None,
                    end_kind: crate::compiler_frontend::ast::ast_nodes::RangeEndKind::Exclusive,
                }),
            },
            body_content: body,
            body_render_plan: None,
            aggregate_render_plan: Some(identity_aggregate_plan()),
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::Loop(Box::new(loop_cf)),
        )
    });
}

#[test]
fn parity_collection_loop() {
    assert_fold_parity(|string_table| {
        let body = TemplateContent {
            atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, ","),
                TemplateSegmentOrigin::Body,
            ))],
        };

        let items = Expression {
            kind: ExpressionKind::Collection(vec![
                int_expression(1),
                int_expression(2),
                int_expression(3),
            ]),
            type_id: builtin_type_ids::INT,
            diagnostic_type: DataType::Int,
            function_receiver: None,
            value_mode: ValueMode::ImmutableOwned,
            location: empty_location(),
            reactive_source: None,
            reactive_template: None,
            const_record_state: crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState::RuntimeValue,
            contains_regular_division: false,
            value_shape: crate::compiler_frontend::ast::expressions::expression::ExpressionValueShape::Ordinary,
        };

        let loop_cf = TemplateLoopControlFlow {
            header: TemplateLoopHeader::Collection {
                bindings: Box::new(LoopBindings {
                    item: None,
                    index: None,
                }),
                iterable: Box::new(items),
            },
            body_content: body,
            body_render_plan: None,
            aggregate_render_plan: Some(identity_aggregate_plan()),
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::Loop(Box::new(loop_cf)),
        )
    });
}

// -------------------------
//  Nested child template parity
// -------------------------

#[test]
fn parity_nested_child_template() {
    assert_fold_parity(|string_table| {
        let child = make_template(TemplateContent {
            atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "child"),
                TemplateSegmentOrigin::Body,
            ))],
        });

        make_template(TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, "before "),
                    TemplateSegmentOrigin::Body,
                )),
                TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(child, ValueMode::ImmutableOwned),
                    TemplateSegmentOrigin::Body,
                )),
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, " after"),
                    TemplateSegmentOrigin::Body,
                )),
            ],
        })
    });
}

#[test]
fn parity_nested_control_flow_child_applies_conditional_wrapper() {
    assert_fold_parity(|string_table| {
        let branch_chain = TemplateBranchChain {
            branches: vec![TemplateConditionalBranch {
                selector: TemplateBranchSelector::Bool(bool_expression(true)),
                content: TemplateContent {
                    atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                        string_expression(string_table, "item"),
                        TemplateSegmentOrigin::Body,
                    ))],
                },
                render_plan: None,
                location: empty_location(),
            }],
            fallback: None,
            location: empty_location(),
        };

        let mut child = make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::BranchChain(Box::new(branch_chain)),
        );
        child
            .conditional_child_wrappers
            .push(list_item_wrapper(string_table));

        make_template(TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, "before "),
                    TemplateSegmentOrigin::Body,
                )),
                TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(child, ValueMode::ImmutableOwned),
                    TemplateSegmentOrigin::Body,
                )),
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, " after"),
                    TemplateSegmentOrigin::Body,
                )),
            ],
        })
    });
}

// -------------------------
//  Slot parity
// -------------------------

#[test]
fn parity_unresolved_slot_folds_to_empty() {
    assert_fold_parity(|string_table| {
        make_template(TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, "["),
                    TemplateSegmentOrigin::Body,
                )),
                TemplateAtom::Slot(SlotPlaceholder::with_wrappers(
                    SlotKey::Default,
                    vec![],
                    vec![],
                    false,
                )),
                TemplateAtom::Content(TemplateSegment::new(
                    string_expression(string_table, "]"),
                    TemplateSegmentOrigin::Body,
                )),
            ],
        })
    });
}

// -------------------------
//  Aggregate wrapper parity
// -------------------------

#[test]
fn parity_loop_with_aggregate_wrapper() {
    assert_fold_parity(|string_table| {
        let body = TemplateContent {
            atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "."),
                TemplateSegmentOrigin::Body,
            ))],
        };

        let aggregate_plan = TemplateAggregateRenderPlan {
            pieces: vec![
                TemplateAggregatePiece::Render(Box::new(
                    crate::compiler_frontend::ast::templates::template_render_plan::RenderPiece::Text(
                        crate::compiler_frontend::ast::templates::template_render_plan::RenderTextPiece {
                            text: string_table.intern("["),
                            location: empty_location(),
                        },
                    ),
                )),
                TemplateAggregatePiece::Aggregate,
                TemplateAggregatePiece::Render(Box::new(
                    crate::compiler_frontend::ast::templates::template_render_plan::RenderPiece::Text(
                        crate::compiler_frontend::ast::templates::template_render_plan::RenderTextPiece {
                            text: string_table.intern("]"),
                            location: empty_location(),
                        },
                    ),
                )),
            ],
        };

        let loop_cf = TemplateLoopControlFlow {
            header: TemplateLoopHeader::Range {
                bindings: Box::new(LoopBindings {
                    item: None,
                    index: None,
                }),
                range: Box::new(crate::compiler_frontend::ast::ast_nodes::RangeLoopSpec {
                    start: int_expression(0),
                    end: int_expression(3),
                    step: None,
                    end_kind: crate::compiler_frontend::ast::ast_nodes::RangeEndKind::Exclusive,
                }),
            },
            body_content: body,
            body_render_plan: None,
            aggregate_render_plan: Some(aggregate_plan),
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::Loop(Box::new(loop_cf)),
        )
    });
}

// -------------------------
//  Structural no-output parity
// -------------------------

#[test]
fn parity_zero_iteration_loop_is_no_output() {
    assert_fold_parity(|string_table| {
        let body = TemplateContent {
            atoms: vec![TemplateAtom::Content(TemplateSegment::new(
                string_expression(string_table, "."),
                TemplateSegmentOrigin::Body,
            ))],
        };

        let loop_cf = TemplateLoopControlFlow {
            header: TemplateLoopHeader::Range {
                bindings: Box::new(LoopBindings {
                    item: None,
                    index: None,
                }),
                range: Box::new(crate::compiler_frontend::ast::ast_nodes::RangeLoopSpec {
                    start: int_expression(0),
                    end: int_expression(0),
                    step: None,
                    end_kind: crate::compiler_frontend::ast::ast_nodes::RangeEndKind::Exclusive,
                }),
            },
            body_content: body,
            body_render_plan: None,
            aggregate_render_plan: Some(identity_aggregate_plan()),
            location: empty_location(),
        };

        make_template_with_control_flow(
            TemplateContent::default(),
            TemplateControlFlow::Loop(Box::new(loop_cf)),
        )
    });
}
