//! Relational pattern parsing.
//!
//! WHAT: parses `<`, `<=`, `>`, `>=` match patterns and validates the subject
//! type is an ordered scalar.
//! WHY: relational patterns share literal parsing but have distinct validation
//! rules, so they live in a dedicated submodule.

use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidMatchPatternReason};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};

use super::literal::parse_literal_pattern;
use super::types::{MatchPattern, RelationalPatternOp};

/// Parse a relational comparison pattern (`<`, `<=`, `>`, `>=`).
///
/// Validates that the subject type supports ordering, then parses the literal
/// operand that follows the operator.
#[allow(clippy::result_large_err)]
pub(super) fn parse_relational_pattern(
    token_stream: &mut FileTokens,
    subject_type_id: TypeId,
    string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<MatchPattern, CompilerDiagnostic> {
    let location = token_stream.current_location();

    let op = match token_stream.current_token_kind() {
        TokenKind::LessThan => RelationalPatternOp::LessThan,
        TokenKind::LessThanOrEqual => RelationalPatternOp::LessThanOrEqual,
        TokenKind::GreaterThan => RelationalPatternOp::GreaterThan,
        TokenKind::GreaterThanOrEqual => RelationalPatternOp::GreaterThanOrEqual,
        _ => unreachable!("caller checked relational lead token"),
    };

    token_stream.advance();
    token_stream.skip_newlines();

    // Reject relational patterns on unsupported subject types before attempting
    // to parse the literal value. This ensures the diagnostic refers to the pattern
    // category (relational) rather than a literal type mismatch.
    ensure_relational_subject_type(subject_type_id, &location, string_table, type_environment)?;

    let value = parse_literal_pattern(
        token_stream,
        subject_type_id,
        string_table,
        type_environment,
    )?;

    Ok(MatchPattern::Relational {
        op,
        value,
        location,
    })
}

/// Ensure the subject type supports relational ordering.
///
/// Only `int`, `float`, `char`, and `string` may appear in relational patterns.
#[allow(clippy::result_large_err)]
fn ensure_relational_subject_type(
    subject_type_id: TypeId,
    location: &SourceLocation,
    _string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    let builtins = type_environment.builtins();

    let is_ordered_scalar = subject_type_id == builtins.int
        || subject_type_id == builtins.float
        || subject_type_id == builtins.char
        || subject_type_id == builtins.string;

    if !is_ordered_scalar {
        return Err(CompilerDiagnostic::invalid_match_pattern(
            InvalidMatchPatternReason::ScrutineeTypeUnsupportedForRelational,
            None,
            None,
            location.clone(),
        ));
    }

    Ok(())
}
