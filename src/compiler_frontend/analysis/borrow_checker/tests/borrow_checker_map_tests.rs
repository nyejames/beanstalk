//! Borrow-checker map operation regression tests.
//!
//! WHAT: validates borrow semantics for hashmap get, set, remove, clear, contains, and length.
//! WHY: map aliasing and consumption rules are language-level guarantees that must be enforced.

use crate::compiler_frontend::tests::borrow_fixture_support::run_borrow_checker;
use crate::compiler_frontend::tests::external_package_support::default_external_package_registry;
use crate::compiler_frontend::tests::hir_fixture_support::lower_hir;
use crate::compiler_frontend::tests::parse_support::parse_single_file_ast;

#[test]
fn map_get_alias_blocks_set_when_used_after_mutation() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
score = scores.get("Ada") catch:
    then 0
;
~scores.set("Linus", 7) catch:
;
io.line([: [score]])
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("a live get alias used after mutation should block the set");
}

#[test]
fn map_get_alias_allows_set_when_used_before_mutation() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
score = scores.get("Ada") catch:
    then 0
;
io.line([: [score]])
~scores.set("Linus", 7) catch:
;
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("a get alias consumed before mutation should allow the later set");
}

#[test]
fn map_get_alias_blocks_remove_when_used_after_mutation() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
score = scores.get("Ada") catch:
    then 0
;
removed = ~scores.remove("Linus") catch:
    then 0
;
io.line([: [score]])
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("a live get alias used after mutation should block the remove");
}

#[test]
fn map_get_alias_blocks_clear_when_used_after_mutation() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
score = scores.get("Ada") catch:
    then 0
;
~scores.clear()
io.line([: [score]])
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("a live get alias used after mutation should block the clear");
}

#[test]
fn map_contains_and_length_do_not_create_long_lived_aliases() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
has = scores.contains("Ada")
count = scores.length
~scores.set("Linus", 7) catch:
;
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("contains and length should not block later mutation");
}

#[test]
fn map_remove_result_is_owned_and_can_outlive_mutation() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
removed = ~scores.remove("Ada") catch:
    then 0
;
~scores.set("Linus", 7) catch:
;
value = removed
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("remove result should be owned and outlive later mutation");
}

#[test]
fn map_set_consumes_inserted_non_copy_values() {
    let source = r#"
scores ~{String = String} = {}
value String = "hello"
~scores.set("key", value) catch:
;
io.line([: [value]])
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("set should consume inserted non-copy values");
}

#[test]
fn map_literal_consumes_inserted_non_copy_values() {
    let source = r#"
value String = "hello"
scores ~{String = String} = {"key" = value}
io.line([: [value]])
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("map literal should consume inserted non-copy values");
}

#[test]
fn map_literal_consumes_inserted_non_copy_keys() {
    let source = r#"
key String = "Ada"
scores ~{String = Int} = {key = 10}
label = key
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("map literal should consume inserted non-copy keys");
}

#[test]
fn nested_map_literal_consumes_inner_inserted_values() {
    let source = r#"
value String = "hello"
scores ~{String = {String = String}} = {"outer" = {"inner" = value}}
io.line([: [value]])
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("nested map literal should consume inner inserted values");
}

#[test]
fn map_literal_allows_copy_values() {
    let source = r#"
value Int = 42
scores ~{String = Int} = {"key" = value}
label = value
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("map literal should allow copy values without explicit copy");
}

#[test]
fn map_get_key_is_not_consumed() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
key String = "Ada"
score = scores.get(key) catch:
    then 0
;
label = key
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("get key should not be consumed");
}

#[test]
fn map_contains_key_is_not_consumed() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
key String = "Ada"
has = scores.contains(key)
label = key
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("contains key should not be consumed");
}

#[test]
fn map_remove_key_is_not_consumed() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
key String = "Ada"
removed = ~scores.remove(key) catch:
    then 0
;
label = key
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("remove key should not be consumed");
}

#[test]
fn map_set_after_nothing_passes() {
    let source = r#"
scores ~{String = Int} = {"Ada" = 10}
~scores.set("Linus", 7) catch:
;
"#;
    let (ast, mut string_table) = parse_single_file_ast(source);
    let hir = lower_hir(ast, &mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("set without prior get should pass");
}
