//! Centralized template-head directive argument parsing.
//!
//! WHAT:
//! - Shared helpers for parsing parenthesized arguments after `$directive` tokens.
//! - Common validation for empty parens, missing close-paren, extra commas, and
//!   compile-time constness.
//!
//! WHY:
//! - Directive argument syntax was duplicated across `$children`, handler styles,
//!   `$slot`, `$insert`, and `$code`. Centralizing it eliminates drift and makes
//!   new directives easier to add correctly.
//!
//! ## Ownership boundary
//!
//! - **This module** owns token-level syntax: `(` detection, paren balancing,
//!   single-expression parsing, string-literal extraction.
//! - **Directive modules** own semantic validation: type checking, compile-time
//!   restrictions, and normalization into template/style state.
//! - **Slot modules** own slot schema and composition; they do not parse tokens.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::template::SlotKey;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_syntax_error;

/// Returns true if the next token after the current directive is `(`.
pub(crate) fn directive_has_arguments(token_stream: &FileTokens) -> bool {
    token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis)
}

/// Advances the token stream from the directive token past `(` into the
/// first argument position.
///
/// Precondition: `directive_has_arguments` returned `true`.
pub(crate) fn advance_into_directive_arguments(token_stream: &mut FileTokens) {
    token_stream.advance(); // past directive token
    token_stream.advance(); // past '('
}

/// Rejects parenthesized arguments for directives that do not accept them.
pub(crate) fn reject_unexpected_directive_arguments(
    token_stream: &FileTokens,
    directive_name: &str,
) -> Result<(), CompilerError> {
    if directive_has_arguments(token_stream) {
        return_syntax_error!(
            format!("'${directive_name}' does not accept arguments."),
            token_stream.current_location()
        );
    }
    Ok(())
}

/// Returns an error if the current token is `)`, signalling empty directive
/// parentheses.
pub(crate) fn reject_empty_directive_parens(
    token_stream: &FileTokens,
) -> Result<(), CompilerError> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Directive arguments cannot be empty. Remove the parentheses or provide an argument.",
            token_stream.current_location()
        );
    }
    Ok(())
}

/// Expects the current token to be `)`. Returns a syntax error with a
/// suggestion if it is not.
pub(crate) fn expect_directive_close_paren(token_stream: &FileTokens) -> Result<(), CompilerError> {
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return Ok(());
    }

    return_syntax_error!(
        "Expected ')' after directive argument.",
        token_stream.current_location(),
        {
            SuggestedInsertion => ")",
        }
    );
}

/// Parses a single compile-time expression inside already-opened directive
/// parentheses.
///
/// The caller must have already advanced past `(` (e.g. via
/// `advance_into_directive_arguments`). On success the parser is positioned
/// at the closing `)` token.
///
/// Performs generic validation:
/// - rejects empty parentheses
/// - rejects extra comma-separated arguments
/// - expects `)` after the expression
fn parse_single_expression_in_directive_parens(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    reject_empty_directive_parens(token_stream)?;

    let mut inferred = DataType::Inferred;
    let expression = create_expression(
        token_stream,
        context,
        &mut inferred,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_syntax_error!(
            "Directive arguments do not support multiple values.",
            token_stream.current_location()
        );
    }

    expect_directive_close_paren(token_stream)?;

    Ok(expression)
}

/// Parses an optional parenthesized compile-time expression after a directive.
///
/// Returns `Ok(None)` if no `(` follows the directive.
/// Returns `Ok(Some(expression))` if a single expression was parsed.
pub(crate) fn parse_optional_parenthesized_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return Ok(None);
    }

    advance_into_directive_arguments(token_stream);
    let expression =
        parse_single_expression_in_directive_parens(token_stream, context, string_table)?;
    Ok(Some(expression))
}

/// Parses a required parenthesized compile-time expression after a directive.
///
/// Returns an error if no `(` follows the directive.
pub(crate) fn parse_required_parenthesized_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return_syntax_error!(
            "This directive requires a parenthesized argument.",
            token_stream.current_location()
        );
    }

    advance_into_directive_arguments(token_stream);
    parse_single_expression_in_directive_parens(token_stream, context, string_table)
}

/// Parses an optional parenthesized string-literal argument.
///
/// Returns `Ok(None)` if no `(` follows the directive.
pub(crate) fn parse_optional_string_literal_argument(
    token_stream: &mut FileTokens,
) -> Result<Option<StringId>, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return Ok(None);
    }

    advance_into_directive_arguments(token_stream);
    reject_empty_directive_parens(token_stream)?;

    let result = expect_string_literal(token_stream)?;
    token_stream.advance();
    reject_extra_comma_in_directive_args(token_stream)?;
    expect_directive_close_paren(token_stream)?;
    Ok(Some(result))
}

/// Parses a required parenthesized string-literal argument.
///
/// Returns an error if no `(` follows the directive or the argument is not a
/// quoted string literal.
#[cfg(test)]
pub(crate) fn parse_required_string_literal_argument(
    token_stream: &mut FileTokens,
) -> Result<StringId, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return_syntax_error!(
            "This directive requires a parenthesized string argument.",
            token_stream.current_location()
        );
    }

    advance_into_directive_arguments(token_stream);
    reject_empty_directive_parens(token_stream)?;

    let result = expect_string_literal(token_stream)?;
    token_stream.advance();
    reject_extra_comma_in_directive_args(token_stream)?;
    expect_directive_close_paren(token_stream)?;
    Ok(result)
}

/// Expects the current token to be a `StringSliceLiteral`.
fn expect_string_literal(token_stream: &FileTokens) -> Result<StringId, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => Ok(*name),
        _ => {
            return_syntax_error!(
                "Expected a quoted string literal argument.",
                token_stream.current_location()
            )
        }
    }
}

fn reject_extra_comma_in_directive_args(token_stream: &FileTokens) -> Result<(), CompilerError> {
    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_syntax_error!(
            "Directive arguments do not support multiple values.",
            token_stream.current_location()
        );
    }
    Ok(())
}

// ----------------------------------------------------------------------------
// Slot-target argument parsers (moved from template_slots.rs)
// ----------------------------------------------------------------------------

/// Parses the optional argument to `$slot`: default (no parens), named string,
/// or positive positional integer.
pub(crate) fn parse_optional_slot_target_argument(
    token_stream: &mut FileTokens,
) -> Result<SlotKey, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return Ok(SlotKey::Default);
    }

    advance_into_directive_arguments(token_stream);

    let target = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => SlotKey::Named(*name),
        TokenKind::IntLiteral(index) => {
            if *index <= 0 {
                return_syntax_error!(
                    format!("'$slot({index})' is invalid. Positional slots start at 1."),
                    token_stream.current_location()
                );
            }
            SlotKey::Positional(*index as usize)
        }
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$slot()' cannot use empty parentheses. Use '$slot' for default, a quoted name like '$slot(\"style\")', or a positive integer like '$slot(1)'.",
                token_stream.current_location()
            );
        }
        _ => {
            return_syntax_error!(
                "'$slot(...)' only accepts a quoted string literal name or a positive integer.",
                token_stream.current_location()
            );
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(target)
}

/// Parses the required named target argument to `$insert("name")`.
pub(crate) fn parse_required_slot_name_argument(
    token_stream: &mut FileTokens,
) -> Result<StringId, CompilerError> {
    if !directive_has_arguments(token_stream) {
        return_syntax_error!(
            "'$insert' requires a quoted named target like '$insert(\"style\")'.",
            token_stream.current_location()
        );
    }

    advance_into_directive_arguments(token_stream);

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::IntLiteral(_) => {
            return_syntax_error!(
                "'$insert(...)' only accepts quoted string literal names.",
                token_stream.current_location()
            );
        }
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$insert()' cannot use empty parentheses. Use quoted names like '$insert(\"style\")'.",
                token_stream.current_location()
            );
        }
        _ => {
            return_syntax_error!(
                "'$insert(...)' only accepts quoted string literal names.",
                token_stream.current_location()
            );
        }
    };

    token_stream.advance();
    expect_directive_close_paren(token_stream)?;
    Ok(slot_name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
    use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationIndex};
    use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
    use crate::compiler_frontend::interned_path::InternedPath;
    use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
    use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
    use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
    use crate::compiler_frontend::symbols::string_interning::StringTable;
    use crate::compiler_frontend::tokenizer::lexer::tokenize;
    use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
    use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind, TokenizeMode};
    use std::rc::Rc;

    fn directive_tokens(source: &str, string_table: &mut StringTable) -> FileTokens {
        let scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);
        let style_directives = StyleDirectiveRegistry::built_ins();
        let mut tokens = tokenize(
            source,
            &scope,
            TokenizeMode::Normal,
            NewlineMode::NormalizeToLf,
            &style_directives,
            string_table,
            None,
        )
        .expect("tokenization should succeed");

        tokens.index = tokens
            .tokens
            .iter()
            .position(|token| matches!(token.kind, TokenKind::StyleDirective(_)))
            .expect("expected a style directive token");

        tokens
    }

    fn test_context(scope: InternedPath) -> ScopeContext {
        let cwd = std::env::temp_dir();
        let resolver = ProjectPathResolver::new(cwd.clone(), cwd, &[])
            .expect("test path resolver should be valid");
        ScopeContext::new(
            ContextKind::Constant,
            scope.clone(),
            Rc::new(TopLevelDeclarationIndex::new(vec![])),
            ExternalPackageRegistry::default(),
            vec![],
        )
        .with_project_path_resolver(Some(resolver))
        .with_source_file_scope(scope)
        .with_path_format_config(PathStringFormatConfig::default())
    }

    // ------------------------------------------------------------------------
    // reject_unexpected_directive_arguments
    // ------------------------------------------------------------------------

    #[test]
    fn reject_arguments_succeeds_when_no_parens() {
        let mut string_table = StringTable::new();
        let tokens = directive_tokens("[$note]", &mut string_table);
        let result = reject_unexpected_directive_arguments(&tokens, "note");
        assert!(result.is_ok());
    }

    #[test]
    fn reject_arguments_fails_when_parens_present() {
        let mut string_table = StringTable::new();
        let tokens = directive_tokens("[$note()]", &mut string_table);
        let result = reject_unexpected_directive_arguments(&tokens, "note");
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("does not accept arguments")
        );
    }

    // ------------------------------------------------------------------------
    // parse_optional_slot_target_argument
    // ------------------------------------------------------------------------

    #[test]
    fn optional_slot_target_no_parens_returns_default() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert_eq!(result.unwrap(), SlotKey::Default);
    }

    #[test]
    fn optional_slot_target_named_string() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot(\"style\")]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert!(matches!(result.unwrap(), SlotKey::Named(_)));
    }

    #[test]
    fn optional_slot_target_positive_positional() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot(1)]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert_eq!(result.unwrap(), SlotKey::Positional(1));
    }

    #[test]
    fn optional_slot_target_zero_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot(0)]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("Positional slots start at 1")
        );
    }

    #[test]
    fn optional_slot_target_negative_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot(-1)]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert!(result.is_err());
    }

    #[test]
    fn optional_slot_target_empty_parens_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot()]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("cannot use empty parentheses")
        );
    }

    #[test]
    fn optional_slot_target_missing_close_paren_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$slot(\"style\"]", &mut string_table);
        let result = parse_optional_slot_target_argument(&mut tokens);
        assert!(result.is_err());
        assert!(result.unwrap_err().msg.contains("Expected ')'"));
    }

    // ------------------------------------------------------------------------
    // parse_required_slot_name_argument
    // ------------------------------------------------------------------------

    #[test]
    fn required_slot_name_missing_parens_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$insert]", &mut string_table);
        let result = parse_required_slot_name_argument(&mut tokens);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("requires a quoted named target")
        );
    }

    #[test]
    fn required_slot_name_string_literal_ok() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$insert(\"style\")]", &mut string_table);
        let result = parse_required_slot_name_argument(&mut tokens);
        assert!(result.is_ok());
    }

    #[test]
    fn required_slot_name_positional_rejected() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$insert(1)]", &mut string_table);
        let result = parse_required_slot_name_argument(&mut tokens);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("only accepts quoted string literal names")
        );
    }

    #[test]
    fn required_slot_name_empty_parens_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$insert()]", &mut string_table);
        let result = parse_required_slot_name_argument(&mut tokens);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("cannot use empty parentheses")
        );
    }

    // ------------------------------------------------------------------------
    // parse_required_string_literal_argument
    // ------------------------------------------------------------------------

    #[test]
    fn required_string_literal_ok() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$code(\"bst\")]", &mut string_table);
        let result = parse_required_string_literal_argument(&mut tokens);
        assert!(result.is_ok());
    }

    #[test]
    fn required_string_literal_missing_parens_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$code]", &mut string_table);
        let result = parse_required_string_literal_argument(&mut tokens);
        assert!(result.is_err());
    }

    #[test]
    fn required_string_literal_not_a_string_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$code(42)]", &mut string_table);
        let result = parse_required_string_literal_argument(&mut tokens);
        assert!(result.is_err());
        assert!(result.unwrap_err().msg.contains("quoted string literal"));
    }

    // ------------------------------------------------------------------------
    // parse_optional_parenthesized_expression
    // ------------------------------------------------------------------------

    #[test]
    fn optional_expression_no_parens_returns_none() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$children]", &mut string_table);
        let context = test_context(tokens.src_path.to_owned());
        let result =
            parse_optional_parenthesized_expression(&mut tokens, &context, &mut string_table);
        assert!(matches!(result, Ok(None)));
    }

    #[test]
    fn optional_expression_with_parens_returns_some() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$children(\"wrap\")]", &mut string_table);
        let context = test_context(tokens.src_path.to_owned());
        let result =
            parse_optional_parenthesized_expression(&mut tokens, &context, &mut string_table);
        assert!(matches!(result, Ok(Some(_))));
    }

    #[test]
    fn optional_expression_empty_parens_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$children()]", &mut string_table);
        let context = test_context(tokens.src_path.to_owned());
        let result =
            parse_optional_parenthesized_expression(&mut tokens, &context, &mut string_table);
        assert!(result.is_err());
    }

    #[test]
    fn optional_expression_extra_comma_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$children(\"a\", \"b\")]", &mut string_table);
        let context = test_context(tokens.src_path.to_owned());
        let result =
            parse_optional_parenthesized_expression(&mut tokens, &context, &mut string_table);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("do not support multiple values")
        );
    }

    // ------------------------------------------------------------------------
    // parse_required_parenthesized_expression
    // ------------------------------------------------------------------------

    #[test]
    fn required_expression_missing_parens_errors() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$children]", &mut string_table);
        let context = test_context(tokens.src_path.to_owned());
        let result =
            parse_required_parenthesized_expression(&mut tokens, &context, &mut string_table);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .msg
                .contains("requires a parenthesized argument")
        );
    }

    #[test]
    fn required_expression_compile_time_constant_ok() {
        let mut string_table = StringTable::new();
        let mut tokens = directive_tokens("[$children(\"wrap\")]", &mut string_table);
        let context = test_context(tokens.src_path.to_owned());
        let result =
            parse_required_parenthesized_expression(&mut tokens, &context, &mut string_table);
        assert!(result.is_ok());
        let expr = result.unwrap();
        assert!(matches!(expr.kind, ExpressionKind::StringSlice(_)));
    }
}
