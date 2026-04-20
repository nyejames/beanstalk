use super::*;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::{ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use std::rc::Rc;

#[test]
fn parse_literal_expression_rejects_int_negation_overflow() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("test.bst", &mut string_table);
    let context = ScopeContext::new(
        ContextKind::Expression,
        scope.clone(),
        Rc::new(vec![]),
        HostRegistry::new(),
        vec![],
    );

    let tokens = vec![
        Token::new(TokenKind::IntLiteral(i64::MIN), SourceLocation::default()),
        Token::new(TokenKind::Eof, SourceLocation::default()),
    ];
    let mut token_stream = FileTokens::new(scope, tokens);
    let mut expression = Vec::new();
    let mut next_number_negative = true;

    let error = parse_literal_expression(
        &mut token_stream,
        &context,
        &DataType::Inferred,
        &Ownership::ImmutableOwned,
        &mut expression,
        &mut next_number_negative,
        &mut string_table,
    )
    .expect_err("negating Int::MIN should return a rule error");

    assert_eq!(error.error_type, ErrorType::Rule);
    assert!(
        error
            .msg
            .contains("Compile-time integer overflow while negating Int literal"),
        "{}",
        error.msg
    );
    assert_eq!(
        error
            .metadata
            .get(&ErrorMetaDataKey::CompilationStage)
            .map(String::as_str),
        Some("Expression Parsing")
    );
}
