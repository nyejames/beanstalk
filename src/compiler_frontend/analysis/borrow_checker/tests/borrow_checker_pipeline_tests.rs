//! Borrow-checker frontend pipeline regression tests.
//!
//! WHAT: runs full frontend entrypoints and asserts borrow-check failures surface through them.
//! WHY: the borrow checker is only useful if orchestration preserves and reports its diagnostics.

use crate::build_system::build::Module;
use crate::compiler_frontend::CompilerFrontend;
use crate::compiler_frontend::analysis::borrow_checker::tests::test_support::{
    assignment_target, build_ast, default_host_registry, entry_and_start, function_node, location,
    lower_hir, make_test_variable, node, reference_expr, run_borrow_checker, symbol,
};
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::projects::settings::Config;

#[test]
fn frontend_check_borrows_propagates_failures() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);
    let z = symbol("z", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    Expression::reference(
                        x.clone(),
                        DataType::Int,
                        location(2),
                        Ownership::MutableReference,
                    ),
                )),
                location(2),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    z,
                    reference_expr(x, DataType::Int, location(3)),
                )),
                location(3),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);

    let config = Config::default();
    let frontend = CompilerFrontend::new(
        &config,
        string_table,
        StyleDirectiveRegistry::built_ins(),
        None,
        NewlineMode::NormalizeToLf,
    );
    let messages = frontend
        .check_borrows(&hir)
        .expect_err("borrow checking should fail");

    assert!(
        messages
            .errors
            .iter()
            .any(|error| error.error_type == ErrorType::BorrowChecker)
    );
}

#[test]
fn successful_borrow_report_can_be_stored_on_module() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

    let counter = symbol("counter", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    counter.clone(),
                    Expression::int(0, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(
                        counter.clone(),
                        DataType::Int,
                        location(2),
                    )),
                    value: Expression::int(1, location(2), Ownership::ImmutableOwned),
                },
                location(2),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let borrow_analysis = run_borrow_checker(&hir, &host_registry, &string_table)
        .expect("borrow checking should pass");

    let module = Module {
        entry_point: std::path::PathBuf::from("main.bst"),
        hir,
        borrow_analysis,
        warnings: Vec::new(),
        const_top_level_fragments: Vec::new(),
        entry_runtime_fragment_count: 0,
    };

    assert!(module.borrow_analysis.stats.functions_analyzed >= 1);
    assert!(module.borrow_analysis.analysis.total_state_snapshots() >= 1);
    assert!(!module.borrow_analysis.analysis.statement_facts.is_empty());
    assert!(!module.borrow_analysis.analysis.terminator_facts.is_empty());
    assert!(!module.borrow_analysis.analysis.value_facts.is_empty());
}
