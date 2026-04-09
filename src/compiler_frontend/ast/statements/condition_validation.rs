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

pub(crate) fn ensure_boolean_condition(
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
