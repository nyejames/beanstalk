//! HIR lowering test modules and shared harness utilities.
//!
//! WHAT: groups the HIR test suites and exposes common naming helpers for them.
//! WHY: HIR tests should discover one another through a single module entry.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

mod hir_expression_lowering_tests;
mod hir_function_origin_tests;
mod hir_statement_lowering_tests;
mod hir_validation_tests;
mod loop_lowering_tests;

pub(super) fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

pub(super) fn entry_path_and_start_name(
    string_table: &mut StringTable,
) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);
    (entry_path, start_name)
}
