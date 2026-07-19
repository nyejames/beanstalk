//! Borrow-checker CFG future-use regression tests.
//!
//! WHAT: protects CFG-carried aliases, projected access actors and independent collection roots.
//! WHY: linear last-use order must defer to CFG future use for source locals without extending
//! compiler-temporary aliases beyond their intended expiry.

use crate::compiler_frontend::compiler_messages::BorrowDiagnosticKind;
use crate::compiler_frontend::hir::hir_side_table::HirLocalOriginKind;
use crate::compiler_frontend::hir::ids::LocalId;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::symbols::string_interning::StringTable;
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

#[test]
fn branch_join_future_use_preserves_alias_conflict_after_linear_expiry() {
    let source = r#"
items ~{Int} = {1, 2, 3}
alias = items
if true:
    branch_marker = 0
else
    if true:
        inner_marker = 0
    else
        ~items.push(4) catch:
        ;
    ;
;
value = alias
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("CFG future use through a branch join should preserve the alias conflict");
    assert_borrow_error_kind(&error, BorrowDiagnosticKind::SharedMutableConflict);
}

#[test]
fn projected_assignment_rooted_in_user_local_preserves_source_alias_conflict() {
    let (hir, mut string_table, _) = projected_assignment_branch_fixture();
    let external_package_registry = default_external_package_registry(&mut string_table);

    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("a user-local projected mutation should preserve the source alias conflict");
    assert_borrow_error_kind(&error, BorrowDiagnosticKind::SharedMutableConflict);
}

#[test]
fn projected_assignment_rooted_in_compiler_temp_uses_linear_expiry() {
    let (mut hir, mut string_table, point_local) = projected_assignment_branch_fixture();
    hir.side_table
        .bind_local_origin(point_local, HirLocalOriginKind::CompilerTemp, None, None);
    let external_package_registry = default_external_package_registry(&mut string_table);

    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("compiler-temporary projected mutation should use linear expiry");
}

fn projected_assignment_branch_fixture() -> (HirModule, StringTable, LocalId) {
    let source = r#"
Point = |
    value Int,
|
point ~= Point(1)
alias = point
if true:
    branch_marker = 0
else
    if true:
        inner_marker = 0
    else
        point.value = 2
    ;
;
value = alias.value
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let point_local = hir
        .blocks
        .iter()
        .flat_map(|block| block.locals.iter())
        .find(|local| hir.side_table.resolve_local_name(local.id, &string_table) == Some("point"))
        .map(|local| local.id)
        .expect("projected assignment root should be a named local");

    (hir, string_table, point_local)
}
