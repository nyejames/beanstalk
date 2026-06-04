//! Tests for compiler-owned symbol preseed behavior.
//!
//! WHAT: exercises deterministic preseeding, stable IDs across independent tables, and correct
//!      merge/remap behavior when the shared prefix contains compiler symbols.
//! WHY: parallel per-file frontend preparation depends on every local table starting from the same
//!      fixed symbol universe with identical IDs.

use super::compiler_symbols::{CompilerSymbolIds, CompilerSymbolSet, PreseededStringTable};
use super::string_interning::StringTable;

fn preseeded_table() -> PreseededStringTable {
    CompilerSymbolSet::preseeded_table(0)
}

fn assert_compiler_symbols_resolve(table: &StringTable, ids: CompilerSymbolIds) {
    assert_eq!(table.resolve(ids.start), "start");
    assert_eq!(table.resolve(ids.this), "this");
    assert_eq!(table.resolve(ids.error), "Error");
    assert_eq!(table.resolve(ids.error_message), "message");
    assert_eq!(table.resolve(ids.error_code), "code");
    assert_eq!(table.resolve(ids.semicolon), ";");
    assert_eq!(table.resolve(ids.closing_bracket), "]");
    assert_eq!(table.resolve(ids.unknown_placeholder), "<unknown>");
}

#[test]
fn preseeded_table_helper_returns_ids_that_resolve_to_fixed_strings() {
    let preseeded = CompilerSymbolSet::preseeded_table(32);

    assert_compiler_symbols_resolve(&preseeded.string_table, preseeded.compiler_symbol_ids);
}

#[test]
fn two_independently_preseeded_tables_assign_same_ids() {
    let first = preseeded_table();
    let second = preseeded_table();

    assert_eq!(first.compiler_symbol_ids, second.compiler_symbol_ids);
    assert_eq!(first.string_table.len(), second.string_table.len());
}

#[test]
fn source_strings_interned_after_preseed_are_not_compiler_symbols() {
    let mut preseeded = preseeded_table();
    let table = &mut preseeded.string_table;
    let ids = &preseeded.compiler_symbol_ids;

    let user_id = table.intern("my_user_function");

    // Every compiler symbol ID should be distinct from user-interned strings that come after
    // the preseed.
    assert_ne!(user_id, ids.start);
    assert_ne!(user_id, ids.this);
    assert_ne!(user_id, ids.error);
    assert_ne!(user_id, ids.error_message);
    assert_ne!(user_id, ids.error_code);
    assert_ne!(user_id, ids.semicolon);
    assert_ne!(user_id, ids.closing_bracket);
    assert_ne!(user_id, ids.unknown_placeholder);

    // The table should have grown by exactly one entry for the user string.
    assert_eq!(table.len(), preseeded_table().string_table.len() + 1);
}

#[test]
fn preseeded_fork_tables_merge_with_identity_prefix() {
    let preseeded = preseeded_table();
    let compiler_symbol_ids = preseeded.compiler_symbol_ids;
    let mut build_table = preseeded.string_table;

    let fork_source = build_table.fork_source();

    let first_fork = fork_source.fork_for_module();
    let (mut first_table, first_base_len) = first_fork.into_parts();
    first_table.intern("first-only");

    let second_fork = fork_source.fork_for_module();
    let (mut second_table, second_base_len) = second_fork.into_parts();
    second_table.intern("second-only");

    build_table.merge_delta_from(&first_table, first_base_len);
    let second_remap = build_table.merge_delta_from(&second_table, second_base_len);

    // Preseeded IDs belong to the shared prefix, so fork remaps should keep them addressable
    // without re-interning fixed symbols after local source strings.
    let remapped_compiler_symbols = CompilerSymbolIds {
        start: second_remap.get(compiler_symbol_ids.start),
        this: second_remap.get(compiler_symbol_ids.this),
        error: second_remap.get(compiler_symbol_ids.error),
        error_message: second_remap.get(compiler_symbol_ids.error_message),
        error_code: second_remap.get(compiler_symbol_ids.error_code),
        semicolon: second_remap.get(compiler_symbol_ids.semicolon),
        closing_bracket: second_remap.get(compiler_symbol_ids.closing_bracket),
        unknown_placeholder: second_remap.get(compiler_symbol_ids.unknown_placeholder),
    };

    assert_compiler_symbols_resolve(&build_table, remapped_compiler_symbols);
}
