//! Shared condition-type validation for control-flow statement headers.
//!
//! WHAT: validates that a condition expression resolves to `Bool`.
//! WHY: `if` and conditional `loop` diagnostics should share one type-error shape so users get
//! consistent metadata (`ExpectedType`, `FoundType`) and suggestion quality.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::return_type_error;

fn ensure_boolean_condition(
    condition: &Expression,
    context_name: &str,
    location: &SourceLocation,
    stage: &str,
    suggestion: &str,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if condition.is_boolean() {
        return Ok(());
    }

    let found_type = condition.data_type.display_with_table(string_table);
    return_type_error!(
        format!("{context_name} requires a Bool condition, found '{found_type}'."),
        location.clone(),
        {
            CompilationStage => stage,
            ExpectedType => "Bool",
            FoundType => found_type,
            PrimarySuggestion => suggestion,
        }
    )
}

/// Validate `if` statement condition type with centralized diagnostics policy.
pub(crate) fn ensure_if_statement_condition(
    condition: &Expression,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    ensure_boolean_condition(
        condition,
        "If statement condition",
        &condition.location,
        "If Statement Parsing",
        "Use a boolean expression in the if condition (for example 'value is 0' or 'flag')",
        string_table,
    )
}

/// Validate conditional-loop header type with centralized diagnostics policy.
pub(crate) fn ensure_loop_condition(
    condition: &Expression,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    ensure_boolean_condition(
        condition,
        "Loop condition",
        &condition.location,
        "Loop Parsing",
        "Use a boolean expression after 'loop', e.g. loop is_ready():",
        string_table,
    )
}
