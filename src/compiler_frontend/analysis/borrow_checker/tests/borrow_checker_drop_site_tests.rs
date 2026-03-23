use crate::compiler_frontend::analysis::borrow_checker::BorrowDropSiteKind;
use crate::compiler_frontend::analysis::borrow_checker::tests::test_support::{
    assignment_target, build_ast, default_host_registry, entry_and_start, function_node, location,
    node, run_borrow_checker, symbol, var,
};
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;

fn lower_hir(
    ast: crate::compiler_frontend::ast::ast::Ast,
    string_table: &mut StringTable,
) -> crate::compiler_frontend::hir::hir_nodes::HirModule {
    HirBuilder::new(string_table, PathStringFormatConfig::default())
        .build_hir_module(ast)
        .expect("HIR lowering should succeed")
}

#[test]
fn emits_advisory_return_drop_sites() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

    let value = symbol("value", &mut string_table);
    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::VariableDeclaration(var(
                value,
                Expression::int(1, location(1), Ownership::MutableOwned),
            )),
            location(1),
        )],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &host_registry, &string_table)
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
    let host_registry = default_host_registry(&mut string_table);

    let x = symbol("x", &mut string_table);
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
                NodeKind::If(
                    Expression::bool(true, location(2), Ownership::ImmutableOwned),
                    vec![node(
                        NodeKind::Assignment {
                            target: Box::new(assignment_target(
                                x.clone(),
                                DataType::Int,
                                location(3),
                            )),
                            value: Expression::int(2, location(3), Ownership::ImmutableOwned),
                        },
                        location(3),
                    )],
                    Some(vec![node(
                        NodeKind::Assignment {
                            target: Box::new(assignment_target(x, DataType::Int, location(4))),
                            value: Expression::int(3, location(4), Ownership::ImmutableOwned),
                        },
                        location(4),
                    )]),
                ),
                location(2),
            ),
            node(
                NodeKind::WhileLoop(
                    Expression::bool(true, location(5), Ownership::ImmutableOwned),
                    vec![node(NodeKind::Break, location(6))],
                ),
                location(5),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start_fn], entry_path), &mut string_table);
    let report = run_borrow_checker(&hir, &host_registry, &string_table)
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
