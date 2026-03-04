use super::*;
use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::templates::code::{
    CodeLanguage, code_formatter, highlight_code_html,
};
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

fn folded_template_output(source: &str) -> String {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");
    let folded = template
        .fold_into_stringid(&None, &mut string_table)
        .expect("folding should succeed");

    string_table.resolve(folded).to_owned()
}

fn template_parse_error(source: &str) -> String {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(source, &mut string_table);
    let context = ScopeContext::new_constant(token_stream.src_path.to_owned());

    Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("template should fail to parse")
        .msg
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

#[test]
fn code_without_argument_uses_generic_highlighting() {
    let rendered = folded_template_output("[$code:\nloop(x + 1)\n]");

    assert!(rendered.contains("<code class='codeblock'>"));
    assert!(rendered.contains("<span class='bs-code-parenthesis'>(</span>"));
    assert!(!rendered.contains("bs-code-keyword"));
}

#[test]
fn code_bst_argument_highlights_beanstalk_rules() {
    let rendered = folded_template_output("[$code(\"bst\"):\nloop x\n-- hi\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>loop</span>"));
    assert!(rendered.contains("<span class='bs-code-comment'>-- hi</span>"));
}

#[test]
fn code_javascript_argument_highlights_js_comments() {
    let rendered = folded_template_output("[$code(\"js\"):\nconst x = 1\n// hi\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>const</span>"));
    assert!(rendered.contains("<span class='bs-code-comment'>// hi</span>"));
}

#[test]
fn code_python_argument_highlights_python_comments() {
    let rendered = folded_template_output("[$code(\"py\"):\ndef run():\n# hi\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>def</span>"));
    assert!(rendered.contains("<span class='bs-code-comment'># hi</span>"));
}

#[test]
fn code_typescript_argument_highlights_typescript_types() {
    let rendered = folded_template_output("[$code(\"ts\"):\ntype Name = string\n]");

    assert!(rendered.contains("<span class='bs-code-keyword'>type</span>"));
    assert!(rendered.contains("<span class='bs-code-type'>string</span>"));
}

#[test]
fn code_empty_parentheses_error_cleanly() {
    let error = template_parse_error("[$code(): body]");

    assert!(error.contains("$code()"));
    assert!(error.contains("generic highlighting"));
}

#[test]
fn code_requires_a_quoted_string_literal_argument() {
    let error = template_parse_error("[$code(lang): body]");

    assert!(error.contains("quoted string literal"));
}

#[test]
fn code_rejects_unknown_language_aliases() {
    let error = template_parse_error("[$code(\"unknown\"): body]");

    assert!(error.contains("Unsupported '$code(...)' language"));
    assert!(error.contains("\"unknown\""));
}

#[test]
fn code_rejects_multiple_language_arguments() {
    let error = template_parse_error("[$code(\"bst\", \"js\"): body]");

    assert!(error.contains("only one language argument"));
}

#[test]
fn runtime_templates_with_code_format_only_static_body_strings() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[value, $code(\"bst\"):\nloop x\n]", &mut string_table);
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

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

    let rendered = string_table.resolve(formatted_body);
    assert!(rendered.contains("<code class='codeblock'>"));
    assert!(rendered.contains("<span class='bs-code-keyword'>loop</span>"));
}

#[test]
fn generic_code_highlighter_marks_syntax_but_not_keywords() {
    let highlighted = highlight_code_html("loop(x + 1)", CodeLanguage::Generic);

    assert!(highlighted.contains("<span class='bs-code-parenthesis'>(</span>"));
    assert!(highlighted.contains("<span class='bs-code-operator'>+</span>"));
    assert!(highlighted.contains("<span class='bs-code-number'>1</span>"));
    assert!(!highlighted.contains("bs-code-keyword"));
    assert!(highlighted.contains("loop"));
}

#[test]
fn direct_beanstalk_highlighter_marks_comments_and_keywords() {
    let highlighted = highlight_code_html("loop x\n-- hi", CodeLanguage::Beanstalk);

    assert!(highlighted.contains("<span class='bs-code-keyword'>loop</span>"));
    assert!(highlighted.contains("<span class='bs-code-comment'>-- hi</span>"));
}

#[test]
fn direct_javascript_highlighter_marks_line_comments() {
    let highlighted = highlight_code_html("const x = 1\n// hi", CodeLanguage::JavaScript);

    assert!(highlighted.contains("<span class='bs-code-keyword'>const</span>"));
    assert!(highlighted.contains("<span class='bs-code-comment'>// hi</span>"));
}

#[test]
fn direct_python_highlighter_marks_hash_comments() {
    let highlighted = highlight_code_html("def run():\n# hi", CodeLanguage::Python);

    assert!(highlighted.contains("<span class='bs-code-keyword'>def</span>"));
    assert!(highlighted.contains("<span class='bs-code-comment'># hi</span>"));
}

#[test]
fn direct_typescript_highlighter_marks_type_keywords() {
    let highlighted = highlight_code_html("type Name = string", CodeLanguage::TypeScript);

    assert!(highlighted.contains("<span class='bs-code-keyword'>type</span>"));
    assert!(highlighted.contains("<span class='bs-code-type'>string</span>"));
}

#[test]
fn direct_code_highlighter_preserves_trailing_words_at_eof() {
    let highlighted = highlight_code_html("value", CodeLanguage::Generic);

    assert!(highlighted.ends_with("value"));
}

#[test]
fn direct_code_highlighter_preserves_single_quoted_strings() {
    let highlighted = highlight_code_html("'value'", CodeLanguage::Generic);

    assert!(highlighted.contains("&#39;value&#39;"));
}

#[test]
fn direct_code_highlighter_escapes_html_sensitive_content() {
    let highlighted = highlight_code_html("<tag>", CodeLanguage::Generic);

    assert!(highlighted.contains("&lt;"));
    assert!(highlighted.contains("tag"));
    assert!(highlighted.contains("&gt;"));
    assert!(!highlighted.contains("<tag>"));
}

#[test]
fn code_formatter_wrapper_preserves_newlines_after_dedent() {
    let formatter = code_formatter(CodeLanguage::Generic);
    let mut content = String::from("    x\n    y");

    formatter.formatter.format(&mut content);

    assert!(content.starts_with("<code class='codeblock'>"));
    assert!(content.ends_with("</code>"));
    assert!(content.contains("x\ny"));
}
