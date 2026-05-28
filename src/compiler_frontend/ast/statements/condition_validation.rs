//! Shared condition-type validation for control-flow statement headers.
//!
//! WHAT: validates that a condition expression resolves to `Bool`.
//! WHY: `if`, match guards, and conditional `loop` headers should all emit the same
//! typed diagnostic payload so that error messages and suggestions are consistent.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, TypeMismatchContext};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

/// Shared helper that checks a condition expression against `Bool`.
///
/// WHY: all control-flow condition sites (`if`, `loop`, match guard) share one
/// diagnostic path so the message, context label, and suggestion stay uniform.
fn ensure_boolean_condition(
    condition: &Expression,
    location: &SourceLocation,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    if condition.type_id == type_environment.builtins().bool {
        return Ok(());
    }

    Err(CompilerDiagnostic::type_mismatch(
        type_environment.builtins().bool,
        condition.type_id,
        TypeMismatchContext::Condition,
        location.clone(),
    ))
}

/// Validate `if` statement condition type with centralized diagnostics policy.
pub(crate) fn ensure_if_statement_condition(
    condition: &Expression,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    ensure_boolean_condition(condition, &condition.location, type_environment)
}

/// Validate `<pattern> if <guard> =>` guard type with match-specific diagnostics.
pub(crate) fn ensure_match_guard_condition(
    condition: &Expression,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    ensure_boolean_condition(condition, &condition.location, type_environment)
}

/// Validate conditional-loop header type with centralized diagnostics policy.
pub(crate) fn ensure_loop_condition(
    condition: &Expression,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    ensure_boolean_condition(condition, &condition.location, type_environment)
}
