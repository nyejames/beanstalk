use super::*;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{TemplateSegmentOrigin, TemplateType};
use crate::compiler_frontend::datatypes::Ownership;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokenizer::tokenize;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, Token, TokenKind};

fn token(kind: TokenKind, line: i32) -> Token {
    Token::new(kind, TextLocation::new_just_line(line))
}

fn template_tokens_from_source(source: &str, string_table: &mut StringTable) -> FileTokens {
    let scope = InternedPath::from_single_str("main.bst/#const_template0", string_table);
    let mut tokens = tokenize(
        source,
        &scope,
        crate::compiler_frontend::tokenizer::tokens::TokenizeMode::Normal,
        string_table,
    )
    .expect("tokenization should succeed");

    tokens.index = tokens
        .tokens
        .iter()
        .position(|token| {
            matches!(
                token.kind,
                TokenKind::TemplateHead | TokenKind::StyleTemplateHead
            )
        })
        .expect("expected a template opener");

    tokens
}

fn runtime_template_context(scope: &InternedPath, string_table: &mut StringTable) -> ScopeContext {
    let value_name = string_table.intern("value");
    let declaration = Declaration {
        id: scope.append(value_name),
        value: Expression::string_slice(
            string_table.intern("dynamic"),
            TextLocation::new_just_line(1),
            Ownership::ImmutableOwned,
        ),
    };

    ScopeContext::new(
        ContextKind::Template,
        scope.to_owned(),
        &[declaration],
        HostRegistry::default(),
        vec![],
    )
}

#[test]
fn parse_template_head_handles_truncated_stream_without_panicking() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = ScopeContext::new_constant(scope.to_owned());

    let mut token_stream = FileTokens::new(
        scope,
        vec![
            token(TokenKind::TemplateHead, 1),
            token(TokenKind::IntLiteral(3), 1),
        ],
    );

    let result = Template::new(&mut token_stream, &context, vec![], &mut string_table);
    assert!(
        result.is_ok(),
        "truncated template-head streams should not panic the parser"
    );
}

#[test]
fn single_item_template_head_with_close_is_foldable() {
    let mut string_table = StringTable::new();
    let scope = InternedPath::from_single_str("main.bst/#const_template0", &mut string_table);
    let context = ScopeContext::new_constant(scope.to_owned());

    let mut token_stream = FileTokens::new(
        scope,
        vec![
            token(TokenKind::TemplateHead, 1),
            token(TokenKind::IntLiteral(3), 1),
            token(TokenKind::TemplateClose, 1),
            token(TokenKind::Eof, 1),
        ],
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("single-item head template should parse");

    assert!(matches!(template.kind, TemplateType::String));
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("folding should succeed");
    assert_eq!(string_table.resolve(folded), "3");
}

#[test]
fn markdown_formats_only_template_body_content() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[\"prefix\", $markdown:\n# Hello\n]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert!(matches!(template.kind, TemplateType::String));
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("folding should succeed");
    let rendered = string_table.resolve(folded);

    assert!(rendered.starts_with("prefix"));
    assert!(rendered.contains("<h1>Hello</h1>"));
    assert!(!rendered.starts_with("<p>prefix"));
}

#[test]
fn runtime_templates_format_static_body_strings_only() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value, $markdown:\n# Hello\n]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert!(matches!(template.kind, TemplateType::StringFunction));
    assert!(template.content.before.iter().any(|segment| {
        segment.origin == TemplateSegmentOrigin::Head
            && matches!(segment.expression.kind, ExpressionKind::Reference(_))
    }));

    let formatted_body = template
        .content
        .before
        .iter()
        .find_map(
            |segment| match (&segment.origin, &segment.expression.kind) {
                (TemplateSegmentOrigin::Body, ExpressionKind::StringSlice(text)) => Some(*text),
                _ => None,
            },
        )
        .expect("expected a formatted body segment");

    assert!(
        string_table
            .resolve(formatted_body)
            .contains("<h1>Hello</h1>")
    );
}

#[test]
fn ignore_clears_inherited_style_before_reapplying_markdown() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$ignore, $markdown:\n# Hello\n]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let mut inherited = Template::create_default(vec![]);
    inherited.style.formatter = Some(markdown_formatter());
    inherited.style.formatter_precedence = 0;
    inherited
        .style
        .child_templates
        .push(Template::create_default(vec![]));

    let template = Template::new(
        &mut token_stream,
        &context,
        vec![inherited],
        &mut string_table,
    )
    .expect("template should parse");

    assert!(template.style.formatter.is_some());
    assert!(template.style.child_templates.is_empty());
}

#[test]
fn stores_style_child_templates_from_dollar_bracket_syntax() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[$[:prefix], : body]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    assert_eq!(template.style.child_templates.len(), 1);
}

#[test]
fn formatter_directive_errors_until_implemented() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$formatter(markdown, 10): body]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("$formatter should not be implemented yet");

    assert!(error.msg.contains("$formatter"));
    assert!(error.msg.contains("not implemented yet"));
}

#[test]
fn unknown_style_directives_error_cleanly() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[$unknown: body]", &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("unknown directives should fail");

    assert!(error.msg.contains("Unsupported style directive"));
    assert!(error.msg.contains("$unknown"));
}
