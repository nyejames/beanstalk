//! Shared type-diagnostic text helpers.
//!
//! WHAT: centralizes common expected/found formatting and shared numeric-mix
//! hint wording.
//! WHY: parser/evaluator/call-validation sites previously duplicated message
//! fragments that should stay consistent.

use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringTable;

pub(crate) const NUMERIC_MIX_HINT: &str =
    "Only Int + Float and Float + Int mix numeric types implicitly.";

pub(crate) fn expected_found_clause(
    expected: &DataType,
    found: &DataType,
    string_table: &StringTable,
) -> String {
    format!(
        "Expected '{}', but found '{}'.",
        expected.display_with_table(string_table),
        found.display_with_table(string_table)
    )
}

pub(crate) fn argument_conversion_hint(expected: &DataType, found: &DataType) -> &'static str {
    if matches!((expected, found), (DataType::Float, DataType::Int)) {
        NUMERIC_MIX_HINT
    } else {
        "Convert the argument to the expected type."
    }
}
