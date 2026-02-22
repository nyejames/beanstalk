#![cfg(test)]

use crate::build_system::build::Module;
use crate::build_system::create_project_modules::ExternalImport;
use crate::compiler_frontend::CompilerFrontend;
use crate::compiler_frontend::analysis::borrow_checker::tests::test_support::{
    assignment_target, build_ast, default_host_registry, entry_and_start, function_node, location,
    lower_hir, node, reference_expr, run_borrow_checker, symbol, var,
};
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::Config;

#[test]
fn frontend_check_borrows_propagates_failures() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);

    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::MutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y,
                    reference_expr(x.clone(), DataType::Int, location(2)),
                )),
                location(2),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(3))),
                    value: Expression::int(2, location(3), Ownership::ImmutableOwned),
                },
                location(3),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);

    let config = Config::default();
    let frontend = CompilerFrontend::new(&config, string_table);
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
                NodeKind::VariableDeclaration(var(
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
        folder_name: "test".to_string(),
        entry_point: std::path::PathBuf::from("main.bst"),
        hir,
        borrow_analysis,
        required_module_imports: Vec::<ExternalImport>::new(),
        exported_functions: Vec::new(),
        warnings: Vec::new(),
        string_table,
    };

    assert!(module.borrow_analysis.stats.functions_analyzed >= 1);
    assert!(module.borrow_analysis.analysis.total_state_snapshots() >= 1);
    assert!(!module.borrow_analysis.analysis.statement_facts.is_empty());
    assert!(!module.borrow_analysis.analysis.terminator_facts.is_empty());
    assert!(!module.borrow_analysis.analysis.value_facts.is_empty());
}
