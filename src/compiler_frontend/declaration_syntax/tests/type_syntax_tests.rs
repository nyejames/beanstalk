//! Type-syntax parsing and resolution regression tests.
//!
//! WHAT: validates type annotation parsing and type resolution in composite types.
//! WHY: type syntax is the source of truth for frontend type identity; parser drift here
//!      affects every downstream type check.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, TypeResolutionContext, parse_type_annotation, resolve_type,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;

fn stream_from_tokens(tokens: Vec<Token>, string_table: &mut StringTable) -> FileTokens {
    FileTokens::new(
        InternedPath::from_single_str("type_syntax_tests", string_table),
        tokens,
    )
}

fn token(kind: TokenKind) -> Token {
    Token::new(kind, SourceLocation::default())
}

#[test]
fn declaration_context_allows_inferred_annotations() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Assign), token(TokenKind::Eof)],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("declaration type annotation should parse");

    assert_eq!(parsed, DataType::Inferred);
}

#[test]
fn declaration_context_parses_named_optional_type() {
    let mut string_table = StringTable::new();
    let point = string_table.intern("Point");
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::Symbol(point)),
            token(TokenKind::QuestionMark),
            token(TokenKind::Assign),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let parsed = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect("named optional declaration type annotation should parse");

    assert_eq!(
        parsed,
        DataType::Option(Box::new(DataType::NamedType(point)))
    );
}

#[test]
fn signature_parameter_rejects_none_type() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::DatatypeNone), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureParameter)
        .expect_err("none parameter type should fail");

    assert!(error.msg.contains("None is not a valid parameter type"));
}

#[test]
fn signature_parameter_rejects_reserved_trait_this_type() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::TraitThis), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureParameter)
        .expect_err("reserved trait keyword type should fail");

    assert!(error.msg.contains("Keyword 'This' is reserved for traits"));
    assert!(error.msg.contains("deferred for Alpha"));
}

#[test]
fn declaration_target_rejects_type_keyword_with_deferred_generic_diagnostic() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Type), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::DeclarationTarget)
        .expect_err("type keyword should be reserved");

    assert!(
        error
            .msg
            .contains("Generic declarations using `type` are reserved")
    );
}

#[test]
fn signature_return_rejects_of_keyword_with_structured_syntax_error() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![token(TokenKind::Of), token(TokenKind::Eof)],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureReturn)
        .expect_err("of keyword should fail in type position");

    assert!(error.msg.contains("Unexpected `of`"));
}

#[test]
fn duplicate_optional_marker_is_rejected() {
    let mut string_table = StringTable::new();
    let mut stream = stream_from_tokens(
        vec![
            token(TokenKind::DatatypeString),
            token(TokenKind::QuestionMark),
            token(TokenKind::QuestionMark),
            token(TokenKind::Eof),
        ],
        &mut string_table,
    );

    let error = parse_type_annotation(&mut stream, TypeAnnotationContext::SignatureReturn)
        .expect_err("duplicate optional marker should fail");

    assert!(error.msg.contains("Duplicate optional marker '?'"));
}

#[test]
fn resolves_named_types_recursively_in_composite_types() {
    let mut string_table = StringTable::new();
    let point_name = string_table.intern("Point");

    let unresolved = DataType::Collection(Box::new(DataType::Option(Box::new(
        DataType::NamedType(point_name),
    ))));

    let point_path = InternedPath::from_single_str("Point", &mut string_table);
    let declarations = vec![Declaration {
        id: point_path,
        value: Expression::no_value(
            SourceLocation::default(),
            DataType::Int,
            ValueMode::ImmutableOwned,
        ),
    }];

    let resolution_context = TypeResolutionContext::from_declarations(&declarations);

    let location = SourceLocation::default();
    let resolved = resolve_type(&unresolved, &location, &resolution_context, &string_table)
        .expect("named type resolution should succeed");

    assert_eq!(
        resolved,
        DataType::Collection(Box::new(DataType::Option(Box::new(DataType::Int))))
    );
}

#[test]
fn unknown_named_type_reports_consistent_error() {
    let mut string_table = StringTable::new();
    let missing = string_table.intern("Missing");

    let unresolved = DataType::NamedType(missing);
    let location = SourceLocation::default();
    let resolution_context = TypeResolutionContext::from_declarations(&[]);

    let error = resolve_type(&unresolved, &location, &resolution_context, &string_table)
        .expect_err("unknown type should fail");

    assert!(
        error
            .msg
            .contains("Unknown type 'Missing'. Type names must be declared before use."),
        "{}",
        error.msg
    );
}
