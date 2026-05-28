//! Literal pattern parsing.
//!
//! WHAT: parses int, float, bool, char, string, and negative numeric literals
//! and dispatches to relational pattern parsing when the lead token is a comparator.
//! WHY: separating literal parsing from relational and choice parsing keeps each
//! submodule focused on one pattern category.

use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidMatchPatternReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::value_mode::ValueMode;

use super::diagnostics::reject_deferred_pattern_lead_token;
use super::relational::parse_relational_pattern;
use super::types::MatchPattern;

/// Parse a non-choice match pattern, dispatching to relational or literal parsers.
#[allow(clippy::result_large_err)]
pub fn parse_non_choice_pattern(
    token_stream: &mut FileTokens,
    subject_type_id: TypeId,
    string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<MatchPattern, CompilerDiagnostic> {
    match token_stream.current_token_kind() {
        TokenKind::LessThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterThanOrEqual => parse_relational_pattern(
            token_stream,
            subject_type_id,
            string_table,
            type_environment,
        ),

        _ => {
            let literal = parse_literal_pattern(
                token_stream,
                subject_type_id,
                string_table,
                type_environment,
            )?;
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
#[allow(clippy::result_large_err)]
pub(super) fn parse_literal_pattern(
    token_stream: &mut FileTokens,
    subject_type_id: TypeId,
    _string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<Expression, CompilerDiagnostic> {
    reject_deferred_pattern_lead_token(token_stream)?;

    let pattern = match token_stream.current_token_kind() {
        // Simple scalar and string literals.
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

        // Negative numeric literals (e.g. `-42`, `-3.14`).
        TokenKind::Negative => {
            let minus_sign_location = token_stream.current_location();
            token_stream.advance();

            match token_stream.current_token_kind() {
                TokenKind::IntLiteral(value) => {
                    let negated_value = -(*value);
                    let expression = Expression::int(
                        negated_value,
                        minus_sign_location,
                        ValueMode::ImmutableOwned,
                    );
                    token_stream.advance();
                    expression
                }
                TokenKind::FloatLiteral(value) => {
                    let negated_value = -(*value);
                    let expression = Expression::float(
                        negated_value,
                        minus_sign_location,
                        ValueMode::ImmutableOwned,
                    );
                    token_stream.advance();
                    expression
                }
                _ => {
                    return Err(CompilerDiagnostic::invalid_match_pattern(
                        InvalidMatchPatternReason::NegativeLiteralNotNumeric,
                        None,
                        None,
                        token_stream.current_location(),
                    ));
                }
            }
        }

        // Patterns that are never valid as literal matches.
        TokenKind::NoneLiteral => {
            return Err(CompilerDiagnostic::invalid_match_pattern(
                InvalidMatchPatternReason::NonePatternRequiresOptionalScrutinee,
                None,
                None,
                token_stream.current_location(),
            ));
        }
        _ => {
            return Err(CompilerDiagnostic::invalid_match_pattern(
                InvalidMatchPatternReason::LiteralTypeUnsupported,
                None,
                None,
                token_stream.current_location(),
            ));
        }
    };

    // Verify the literal type is compatible with the scrutinee.
    if !is_type_compatible(subject_type_id, pattern.type_id, type_environment) {
        return Err(CompilerDiagnostic::type_mismatch(
            subject_type_id,
            pattern.type_id,
            TypeMismatchContext::MatchPattern,
            pattern.location.clone(),
        ));
    }

    Ok(pattern)
}
