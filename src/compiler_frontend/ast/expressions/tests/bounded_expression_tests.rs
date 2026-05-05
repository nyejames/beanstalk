//! Bounded expression parsing regression tests.
//!
//! WHAT: validates `create_expression_until` edge cases after the token-window refactor.
//! WHY: the bounded window replaces token-copying with a temporary `length` cap; these tests
//!      prove the cap behaves correctly for delimiters, nesting, and EOF boundaries.

use super::*;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, TopLevelDeclarationTable};
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use std::rc::Rc;

fn test_scope(string_table: &mut StringTable) -> (InternedPath, ScopeContext) {
    let scope = InternedPath::from_single_str("test.bst", string_table);
    let context = ScopeContext::new(
        ContextKind::Expression,
        scope.clone(),
        Rc::new(TopLevelDeclarationTable::new(vec![])),
        ExternalPackageRegistry::new(),
        vec![],
    );
    (scope, context)
}

fn token(kind: TokenKind, scope: &InternedPath) -> Token {
    Token::new(
        kind,
        SourceLocation::new(scope.clone(), Default::default(), Default::default()),
    )
}

#[test]
fn bounded_expression_empty_at_delimiter_errors() {
    let mut string_table = StringTable::new();
    let (scope, context) = test_scope(&mut string_table);

    let tokens = vec![
        token(TokenKind::Comma, &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope, tokens);
    let mut data_type = DataType::Inferred;

    let error = create_expression_until(
        &mut stream,
        &context,
        &mut data_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Comma],
        &mut string_table,
    )
    .expect_err("empty expression should error");

    assert_eq!(error.error_type, ErrorType::Syntax);
}

#[test]
fn bounded_expression_parses_simple_literal() {
    let mut string_table = StringTable::new();
    let (scope, context) = test_scope(&mut string_table);

    let tokens = vec![
        token(TokenKind::IntLiteral(42), &scope),
        token(TokenKind::Comma, &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope.clone(), tokens);
    let mut data_type = DataType::Inferred;

    let expression = create_expression_until(
        &mut stream,
        &context,
        &mut data_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Comma],
        &mut string_table,
    )
    .expect("simple literal should parse");

    assert!(matches!(expression.kind, ExpressionKind::Int(42)));
    // The stop token (comma) should not be consumed.
    assert_eq!(stream.index, 1);
    assert_eq!(stream.current_token_kind(), &TokenKind::Comma);
}

#[test]
fn bounded_expression_nested_parentheses() {
    let mut string_table = StringTable::new();
    let (scope, context) = test_scope(&mut string_table);

    let tokens = vec![
        token(TokenKind::IntLiteral(1), &scope),
        token(TokenKind::Add, &scope),
        token(TokenKind::OpenParenthesis, &scope),
        token(TokenKind::IntLiteral(2), &scope),
        token(TokenKind::Add, &scope),
        token(TokenKind::IntLiteral(3), &scope),
        token(TokenKind::CloseParenthesis, &scope),
        token(TokenKind::Comma, &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope.clone(), tokens);
    let mut data_type = DataType::Inferred;

    let expression = create_expression_until(
        &mut stream,
        &context,
        &mut data_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Comma],
        &mut string_table,
    )
    .expect("nested parentheses should parse");

    // All literals fold to a single Int(6).
    assert!(matches!(expression.kind, ExpressionKind::Int(6)));
    // Stop token should remain unconsumed.
    assert_eq!(stream.index, 7);
    assert_eq!(stream.current_token_kind(), &TokenKind::Comma);
}

#[test]
fn bounded_expression_nested_curly_braces() {
    let mut string_table = StringTable::new();
    let (scope, context) = test_scope(&mut string_table);

    // A collection literal `{2, 3}` followed by a comma.
    // The comma inside the collection must not terminate the bounded expression.
    let tokens = vec![
        token(TokenKind::OpenCurly, &scope),
        token(TokenKind::IntLiteral(2), &scope),
        token(TokenKind::Comma, &scope),
        token(TokenKind::IntLiteral(3), &scope),
        token(TokenKind::CloseCurly, &scope),
        token(TokenKind::Comma, &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope.clone(), tokens);
    let mut data_type = DataType::Inferred;

    let expression = create_expression_until(
        &mut stream,
        &context,
        &mut data_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Comma],
        &mut string_table,
    )
    .expect("nested curly braces should parse");

    // Should parse as a collection expression.
    assert!(matches!(expression.kind, ExpressionKind::Collection(_)));
    assert_eq!(stream.index, 5);
    assert_eq!(stream.current_token_kind(), &TokenKind::Comma);
}

#[test]
fn bounded_expression_missing_delimiter_reaches_eof() {
    let mut string_table = StringTable::new();
    let (scope, context) = test_scope(&mut string_table);

    let tokens = vec![
        token(TokenKind::IntLiteral(1), &scope),
        token(TokenKind::Add, &scope),
        token(TokenKind::IntLiteral(2), &scope),
        token(TokenKind::Eof, &scope),
    ];
    let mut stream = FileTokens::new(scope, tokens);
    let mut data_type = DataType::Inferred;

    let error = create_expression_until(
        &mut stream,
        &context,
        &mut data_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Comma],
        &mut string_table,
    )
    .expect_err("missing delimiter should error");

    assert_eq!(error.error_type, ErrorType::Syntax);
}
