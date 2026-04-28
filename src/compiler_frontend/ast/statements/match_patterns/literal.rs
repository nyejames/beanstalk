//! Literal pattern parsing.
//!
//! WHAT: parses int, float, bool, char, string, and negative numeric literals
//! and dispatches to relational pattern parsing when the lead token is a comparator.
//! WHY: separating literal parsing from relational and choice parsing keeps each
//! submodule focused on one pattern category.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_rule_error;

use super::diagnostics::reject_deferred_pattern_lead_token;
use super::relational::parse_relational_pattern;
use super::types::MatchPattern;

/// Parse a non-choice match pattern, dispatching to relational or literal parsers.
pub fn parse_non_choice_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<MatchPattern, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::LessThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterThanOrEqual => {
            parse_relational_pattern(token_stream, subject_type, string_table)
        }

        _ => {
            let literal = parse_literal_pattern(token_stream, subject_type, string_table)?;
            Ok(MatchPattern::Literal(literal))
        }
    }
}

/// Parse a literal value pattern and type-check it against the scrutinee.
///
/// WHAT: accepts int, float, bool, char, string, and negative numeric literals and
/// verifies the pattern type is compatible with the scrutinee type.
/// WHY: catching type mismatches at parse time produces better source-located errors
/// than deferring the check to HIR lowering.
pub(super) fn parse_literal_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<Expression, CompilerError> {
    reject_deferred_pattern_lead_token(token_stream)?;

    let pattern = match token_stream.current_token_kind() {
        TokenKind::IntLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::int(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::FloatLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::float(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::BoolLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::bool(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::CharLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::char(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::StringSliceLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::string_slice(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::Negative => {
            let negative_location = token_stream.current_location();
            token_stream.advance();
            match token_stream.current_token_kind() {
                TokenKind::IntLiteral(value) => {
                    let expression =
                        Expression::int(-(*value), negative_location, ValueMode::ImmutableOwned);
                    token_stream.advance();
                    expression
                }
                TokenKind::FloatLiteral(value) => {
                    let expression =
                        Expression::float(-(*value), negative_location, ValueMode::ImmutableOwned);
                    token_stream.advance();
                    expression
                }
                _ => {
                    return_rule_error!(
                        "Negative literal patterns must be numeric literals (for example '-1' or '-3.2').",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Use a numeric literal after '-' or switch to a supported literal pattern",
                        }
                    );
                }
            }
        }
        _ => {
            return_rule_error!(
                "Literal match patterns currently support only literal int/float/bool/char/string values.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use a literal value pattern (for example 'case 1 =>', 'case true =>', or 'case \"ok\" =>')",
                }
            );
        }
    };

    if !is_type_compatible(subject_type, &pattern.data_type) {
        return_rule_error!(
            format!(
                "Match arm literal type '{}' does not match scrutinee type '{}'.",
                pattern.data_type.display_with_table(string_table),
                subject_type.display_with_table(string_table),
            ),
            pattern.location.clone(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use literal patterns that match the scrutinee type",
                ExpectedType => subject_type.display_with_table(string_table),
                FoundType => pattern.data_type.display_with_table(string_table),
            }
        );
    }

    Ok(pattern)
}
