use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::{
    consume_balanced_template_region, find_expression_end_index,
    has_top_level_comma_before_statement_end,
};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};

fn token(kind: TokenKind) -> Token {
    Token::new(kind, SourceLocation::default())
}

fn stream_from_kinds(kinds: Vec<TokenKind>, string_table: &mut StringTable) -> FileTokens {
    let tokens = kinds.into_iter().map(token).collect();
    FileTokens::new(
        InternedPath::from_single_str("token_scan_tests", string_table),
        tokens,
    )
}

#[test]
fn top_level_comma_detection_ignores_nested_commas() {
    let mut string_table = StringTable::new();

    let nested_only = stream_from_kinds(
        vec![
            TokenKind::OpenParenthesis,
            TokenKind::IntLiteral(1),
            TokenKind::Comma,
            TokenKind::IntLiteral(2),
            TokenKind::CloseParenthesis,
            TokenKind::Newline,
            TokenKind::Eof,
        ],
        &mut string_table,
    );
    assert!(!has_top_level_comma_before_statement_end(&nested_only));

    let top_level = stream_from_kinds(
        vec![
            TokenKind::Symbol(string_table.intern("a")),
            TokenKind::Comma,
            TokenKind::Symbol(string_table.intern("b")),
            TokenKind::Assign,
            TokenKind::IntLiteral(1),
            TokenKind::Newline,
            TokenKind::Eof,
        ],
        &mut string_table,
    );
    assert!(has_top_level_comma_before_statement_end(&top_level));
}

#[test]
fn expression_end_index_respects_nested_depth() {
    let mut string_table = StringTable::new();

    let tokens = vec![
        token(TokenKind::Symbol(string_table.intern("call"))),
        token(TokenKind::OpenParenthesis),
        token(TokenKind::Symbol(string_table.intern("a"))),
        token(TokenKind::Comma),
        token(TokenKind::Symbol(string_table.intern("b"))),
        token(TokenKind::CloseParenthesis),
        token(TokenKind::Comma),
        token(TokenKind::Symbol(string_table.intern("tail"))),
        token(TokenKind::Eof),
    ];

    let end_index = find_expression_end_index(&tokens, 0, &[TokenKind::Comma]);
    assert_eq!(
        end_index, 6,
        "expected top-level comma after call expression"
    );
}

#[test]
fn balanced_template_region_consumes_nested_templates() {
    let mut string_table = StringTable::new();

    let text_outer = string_table.intern("outer");
    let text_inner = string_table.intern("inner");

    // Stream starts just after an opening TemplateHead that the caller already consumed.
    let mut stream = stream_from_kinds(
        vec![
            TokenKind::StringSliceLiteral(text_outer),
            TokenKind::TemplateHead,
            TokenKind::StringSliceLiteral(text_inner),
            TokenKind::TemplateClose,
            TokenKind::TemplateClose,
            TokenKind::Eof,
        ],
        &mut string_table,
    );

    let mut consumed = Vec::new();
    consume_balanced_template_region(
        &mut stream,
        |token, _kind| consumed.push(token.kind),
        |_location| String::from("unexpected eof"),
    )
    .expect("balanced template scan should succeed");

    assert_eq!(
        consumed,
        vec![
            TokenKind::StringSliceLiteral(text_outer),
            TokenKind::TemplateHead,
            TokenKind::StringSliceLiteral(text_inner),
            TokenKind::TemplateClose,
            TokenKind::TemplateClose,
        ]
    );
    assert_eq!(stream.current_token_kind(), &TokenKind::Eof);
}

#[test]
fn balanced_template_region_errors_on_eof_before_close() {
    let mut string_table = StringTable::new();

    let mut stream = stream_from_kinds(
        vec![
            TokenKind::StringSliceLiteral(string_table.intern("x")),
            TokenKind::Eof,
        ],
        &mut string_table,
    );

    let error = consume_balanced_template_region(
        &mut stream,
        |_token, _kind| {},
        |_location| String::from("missing template close"),
    )
    .expect_err("unterminated template should fail");

    assert_eq!(error, "missing template close");
}
