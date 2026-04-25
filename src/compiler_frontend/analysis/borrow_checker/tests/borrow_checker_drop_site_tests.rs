//! Borrow-checker drop-site regression tests.
//!
//! WHAT: exercises where locals are considered dropped as scopes and statements complete.
//! WHY: incorrect drop placement can silently change borrow lifetimes and ownership outcomes.

use crate::compiler_frontend::analysis::borrow_checker::BorrowDropSiteKind;
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    assignment_target, build_ast, default_external_package_registry, entry_and_start,
    function_node, make_test_variable, node, run_borrow_checker, symbol, test_location,
};
use crate::compiler_frontend::value_mode::ValueMode;

fn lower_hir(
    ast: crate::compiler_frontend::ast::Ast,
    string_table: &mut StringTable,
) -> crate::compiler_frontend::hir::module::HirModule {
    HirBuilder::new(string_table, PathStringFormatConfig::default())
        .build_hir_module(ast)
        .expect("HIR lowering should succeed")
}

#[test]
fn emits_advisory_return_drop_sites() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let value = symbol("value", &mut string_table);
    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::VariableDeclaration(make_test_variable(
                value,
                Expression::int(1, test_location(1), ValueMode::MutableOwned),
            )),
            test_location(1),
        )],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("borrow checking should succeed");

    let has_return_site = report
        .analysis
        .advisory_drop_sites
        .values()
        .flatten()
        .any(|site| matches!(site.kind, BorrowDropSiteKind::Return));
    assert!(
        has_return_site,
        "expected at least one advisory return drop site"
    );

    for site in report.analysis.advisory_drop_sites.values().flatten() {
        let mut sorted = site.locals.clone();
        sorted.sort_by_key(|local| local.0);
        assert_eq!(
            site.locals, sorted,
            "drop-site locals should be in deterministic local-id order"
        );
    }
}

#[test]
fn emits_advisory_break_and_region_exit_drop_sites() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let x = symbol("x", &mut string_table);
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
                    Expression::int(1, test_location(1), ValueMode::MutableOwned),
                )),
                test_location(1),
            ),
            node(
                NodeKind::If(
                    Expression::bool(true, test_location(2), ValueMode::ImmutableOwned),
                    vec![node(
                        NodeKind::Assignment {
                            target: Box::new(assignment_target(
                                x.clone(),
                                DataType::Int,
                                test_location(3),
                            )),
                            value: Expression::int(2, test_location(3), ValueMode::ImmutableOwned),
                        },
                        test_location(3),
                    )],
                    Some(vec![node(
                        NodeKind::Assignment {
                            target: Box::new(assignment_target(x, DataType::Int, test_location(4))),
                            value: Expression::int(3, test_location(4), ValueMode::ImmutableOwned),
                        },
                        test_location(4),
                    )]),
                ),
                test_location(2),
            ),
            node(
                NodeKind::WhileLoop(
                    Expression::bool(true, test_location(5), ValueMode::ImmutableOwned),
                    vec![node(NodeKind::Break, test_location(6))],
                ),
                test_location(5),
            ),
        ],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("borrow checking should succeed");

    let has_break_site = report
        .analysis
        .advisory_drop_sites
        .values()
        .flatten()
        .any(|site| matches!(site.kind, BorrowDropSiteKind::Break));
    assert!(has_break_site, "expected advisory break drop sites");

    let has_region_exit_site = report
        .analysis
        .advisory_drop_sites
        .values()
        .flatten()
        .any(|site| matches!(site.kind, BorrowDropSiteKind::BlockExit));
    assert!(
        has_region_exit_site,
        "expected advisory region-exit (block-exit) drop sites"
    );
}
