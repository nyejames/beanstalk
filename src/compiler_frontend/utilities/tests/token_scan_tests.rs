use crate::compiler_frontend::numeric_text::token::NumericLiteralToken;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::utilities::token_scan::{
    collect_declaration_initializer_tokens, consume_balanced_template_region,
    find_expression_end_index, has_top_level_comma_before_statement_end,
};

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
            TokenKind::NumericLiteral(NumericLiteralToken::test_new("1", &mut string_table)),
            TokenKind::Comma,
            TokenKind::NumericLiteral(NumericLiteralToken::test_new("2", &mut string_table)),
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
            TokenKind::NumericLiteral(NumericLiteralToken::test_new("1", &mut string_table)),
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
fn declaration_initializer_tokens_include_terminal_catch_then_block() {
    let mut string_table = StringTable::new();
    let load_name = string_table.intern("load");
    let next_statement_name = string_table.intern("next_statement");
    let mut stream = stream_from_kinds(
        vec![
            TokenKind::Symbol(load_name),
            TokenKind::OpenParenthesis,
            TokenKind::CloseParenthesis,
            TokenKind::Catch,
            TokenKind::Colon,
            TokenKind::Newline,
            TokenKind::Then,
            TokenKind::NumericLiteral(NumericLiteralToken::test_new("0", &mut string_table)),
            TokenKind::Newline,
            TokenKind::End,
            TokenKind::Newline,
            TokenKind::Symbol(next_statement_name),
            TokenKind::Eof,
        ],
        &mut string_table,
    );

    let collected = collect_declaration_initializer_tokens(&mut stream, &mut string_table)
        .expect("catch initializer should scan successfully");
    let collected_kinds: Vec<_> = collected.into_iter().map(|token| token.kind).collect();

    assert_eq!(
        collected_kinds,
        vec![
            TokenKind::Symbol(load_name),
            TokenKind::OpenParenthesis,
            TokenKind::CloseParenthesis,
            TokenKind::Catch,
            TokenKind::Colon,
            TokenKind::Newline,
            TokenKind::Then,
            TokenKind::NumericLiteral(NumericLiteralToken::test_new("0", &mut string_table)),
            TokenKind::Newline,
            TokenKind::End,
        ]
    );
    assert_eq!(stream.current_token_kind(), &TokenKind::Newline);
}

#[test]
fn declaration_initializer_tokens_balance_nested_blocks_inside_catch() {
    let mut string_table = StringTable::new();
    let load_name = string_table.intern("load");
    let flag_name = string_table.intern("flag");
    let io_name = string_table.intern("io");
    let next_statement_name = string_table.intern("next_statement");
    let mut stream = stream_from_kinds(
        vec![
            TokenKind::Symbol(load_name),
            TokenKind::OpenParenthesis,
            TokenKind::CloseParenthesis,
            TokenKind::Catch,
            TokenKind::Colon,
            TokenKind::Newline,
            TokenKind::If,
            TokenKind::Symbol(flag_name),
            TokenKind::Colon,
            TokenKind::Newline,
            TokenKind::Symbol(io_name),
            TokenKind::OpenParenthesis,
            TokenKind::CloseParenthesis,
            TokenKind::Newline,
            TokenKind::End,
            TokenKind::Newline,
            TokenKind::Then,
            TokenKind::NumericLiteral(NumericLiteralToken::test_new("0", &mut string_table)),
            TokenKind::Newline,
            TokenKind::End,
            TokenKind::Newline,
            TokenKind::Symbol(next_statement_name),
            TokenKind::Eof,
        ],
        &mut string_table,
    );

    let collected = collect_declaration_initializer_tokens(&mut stream, &mut string_table)
        .expect("nested catch initializer should scan successfully");
    let collected_kinds: Vec<_> = collected.into_iter().map(|token| token.kind).collect();

    assert!(
        collected_kinds.contains(&TokenKind::Then),
        "catch fallback should remain inside the initializer token slice"
    );
    assert_eq!(
        collected_kinds
            .iter()
            .filter(|kind| matches!(kind, TokenKind::End))
            .count(),
        2,
        "both the nested if and the catch block terminators should be collected"
    );
    assert_eq!(stream.current_token_kind(), &TokenKind::Newline);
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

#[test]
fn collect_symbol_references_matches_initializer_behavior_for_bare_symbol() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("value");
    let tokens = vec![
        token(TokenKind::Symbol(name)),
        token(TokenKind::Newline),
        token(TokenKind::Eof),
    ];
    let refs = crate::compiler_frontend::utilities::token_scan::collect_symbol_references(&tokens);
    assert_eq!(refs.len(), 1);
    assert_eq!(string_table.resolve(refs[0].name), "value");
    assert!(refs[0].dot_member.is_none());
    assert!(!refs[0].followed_by_call);
    assert!(!refs[0].followed_by_choice_namespace);
}

#[test]
fn collect_symbol_references_matches_initializer_behavior_for_dot_member() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("config");
    let member = string_table.intern("setting");
    let tokens = vec![
        token(TokenKind::Symbol(name)),
        token(TokenKind::Dot),
        token(TokenKind::Symbol(member)),
        token(TokenKind::Newline),
    ];
    let refs = crate::compiler_frontend::utilities::token_scan::collect_symbol_references(&tokens);
    assert_eq!(refs.len(), 1);
    assert_eq!(string_table.resolve(refs[0].name), "config");
    assert_eq!(
        refs[0].dot_member.map(|m| string_table.resolve(m)),
        Some("setting")
    );
    assert!(!refs[0].followed_by_call);
}

#[test]
fn collect_symbol_references_matches_initializer_behavior_for_call() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("helper");
    let tokens = vec![
        token(TokenKind::Symbol(name)),
        token(TokenKind::OpenParenthesis),
        token(TokenKind::CloseParenthesis),
    ];
    let refs = crate::compiler_frontend::utilities::token_scan::collect_symbol_references(&tokens);
    assert_eq!(refs.len(), 1);
    assert_eq!(string_table.resolve(refs[0].name), "helper");
    assert!(refs[0].followed_by_call);
    assert!(!refs[0].followed_by_choice_namespace);
}

#[test]
fn collect_symbol_references_matches_initializer_behavior_for_choice_namespace() {
    let mut string_table = StringTable::new();
    let name = string_table.intern("Status");
    let tokens = vec![
        token(TokenKind::Symbol(name)),
        token(TokenKind::DoubleColon),
        token(TokenKind::Symbol(string_table.intern("Ready"))),
    ];
    let refs = crate::compiler_frontend::utilities::token_scan::collect_symbol_references(&tokens);
    assert_eq!(refs.len(), 1);
    assert_eq!(string_table.resolve(refs[0].name), "Status");
    assert!(!refs[0].followed_by_call);
    assert!(refs[0].followed_by_choice_namespace);
}
