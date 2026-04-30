//! Relational pattern parsing.
//!
//! WHAT: parses `<`, `<=`, `>`, `>=` match patterns and validates the scrutinee
//! is an ordered scalar type.
//! WHY: relational patterns share literal parsing but have distinct validation
//! rules, so they deserve their own submodule.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;

use super::literal::parse_literal_pattern;
use super::types::{MatchPattern, RelationalPatternOp};

pub(super) fn parse_relational_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<MatchPattern, CompilerError> {
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

    // Reject relational patterns on unsupported scrutinee types before attempting
    // to parse the literal value. This ensures the diagnostic refers to the pattern
    // category (relational) rather than a literal type mismatch.
    ensure_relational_subject_type(subject_type, &location, string_table)?;

    let value = parse_literal_pattern(token_stream, subject_type, string_table)?;

    Ok(MatchPattern::Relational {
        op,
        value,
        location,
    })
}

fn ensure_relational_subject_type(
    subject_type: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let is_ordered_scalar = matches!(
        subject_type,
        DataType::Int | DataType::Float | DataType::Char | DataType::StringSlice
    );

    if !is_ordered_scalar {
        return_rule_error!(
            format!(
                "Relational match patterns are only supported for ordered scalar types (Int, Float, Char, String), not '{}'.",
                subject_type.display_with_table(string_table)
            ),
            location.clone(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use literal patterns or an 'else =>' arm for this scrutinee type",
            }
        );
    }

    Ok(())
}
