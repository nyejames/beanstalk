//! Borrow-checker collection-loop regression tests.
//!
//! WHAT: protects loop-carried iterable aliases and independent collection roots.
//! WHY: collection-loop lowering retains a shared iterable alias across the CFG backedge.

use crate::compiler_frontend::compiler_messages::BorrowDiagnosticKind;
use crate::compiler_frontend::tests::borrow_fixture_support::{
    assert_borrow_error_kind, run_borrow_checker,
};
use crate::compiler_frontend::tests::external_package_support::default_external_package_registry;
use crate::compiler_frontend::tests::hir_fixture_support::lower_hir;
use crate::compiler_frontend::tests::parse_support::parse_single_file_ast;

fn borrow_check_source(source: &str) {
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("source should pass borrow checking");
}

#[test]
fn collection_loop_mutation_of_iterable_reports_shared_mutable_conflict() {
    let source = r#"
items ~{Int} = {1, 2, 3}
loop items |item|:
    ~items.push(4) catch:
    ;
;
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("mutating a collection while iterating it should fail");
    assert_borrow_error_kind(&error, BorrowDiagnosticKind::SharedMutableConflict);
}

#[test]
fn collection_loop_mutation_of_unrelated_root_is_valid() {
    borrow_check_source(
        r#"
items ~{Int} = {1, 2, 3}
other ~{Int} = {4, 5}
loop items |item|:
    ~other.push(6) catch:
    ;
;
"#,
    );
}

#[test]
fn collection_loop_mutation_of_source_copy_is_valid() {
    borrow_check_source(
        r#"
items ~{Int} = {1, 2, 3}
copied = copy items
loop copied |item|:
    ~items.push(4) catch:
    ;
;
"#,
    );
}
