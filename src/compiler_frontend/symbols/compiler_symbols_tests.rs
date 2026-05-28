//! Tests for compiler-owned symbol preseed behavior.
//!
//! WHAT: exercises deterministic preseeding, stable IDs across independent tables, and correct
//!      merge/remap behavior when the shared prefix contains compiler symbols.
//! WHY: parallel per-file frontend preparation depends on every local table starting from the same
//!      fixed symbol universe with identical IDs.

use super::compiler_symbols::{CompilerSymbolSet, PreseededStringTable};

fn preseeded_table() -> PreseededStringTable {
    CompilerSymbolSet::preseeded_table(0)
}

#[test]
fn preseeded_table_helper_returns_ids_that_resolve_to_fixed_strings() {
    let preseeded = CompilerSymbolSet::preseeded_table(32);
    let table = &preseeded.string_table;
    let ids = &preseeded.compiler_symbol_ids;

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
    let preseed_len = build_table.len();

    let fork_source = build_table.fork_source();

    let first_fork = fork_source.fork_for_module();
    let (mut first_table, first_base_len) = first_fork.into_parts();
    let _first_local = first_table.intern("first-only");

    let second_fork = fork_source.fork_for_module();
    let (mut second_table, second_base_len) = second_fork.into_parts();
    let second_local = second_table.intern("second-only");

    // Merging the first fork back should be identity because its only local string is new to the
    // build table and appends at the same index it held in the fork.
    let first_remap = build_table.merge_delta_from(&first_table, first_base_len);
    assert!(first_remap.is_identity());
    assert!(!first_remap.has_non_identity_after(preseed_len));

    // Merging the second fork should remap its local suffix because "second-only" now collides
    // with the position after the first fork's local string.
    let second_remap = build_table.merge_delta_from(&second_table, second_base_len);
    assert!(!second_remap.is_identity());
    assert!(second_remap.has_non_identity_after(preseed_len));

    // Preseeded IDs belong to the shared prefix, so fork remaps should keep them addressable
    // without re-interning fixed symbols after local source strings.
    assert_eq!(
        build_table.resolve(second_remap.get(compiler_symbol_ids.start)),
        "start"
    );
    assert_eq!(
        build_table.resolve(second_remap.get(compiler_symbol_ids.this)),
        "this"
    );

    // The second fork's local-only string must resolve to the correct merged ID.
    let merged_local_id = second_remap.get(second_local);
    assert_eq!(build_table.resolve(merged_local_id), "second-only");
}
