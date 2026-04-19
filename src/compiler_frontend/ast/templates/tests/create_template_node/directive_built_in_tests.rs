use super::*;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::styles::code::{
    CodeLanguage, code_formatter, highlight_code_html,
};
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, TemplateAtom, TemplateSegment, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_render_plan::RenderPiece;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::compiler_warnings::WarningKind;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::{
    StyleDirectiveArgumentType, StyleDirectiveEffects, StyleDirectiveHandlerSpec,
    StyleDirectiveRegistry, StyleDirectiveSpec,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::lexer::tokenize;
use crate::compiler_frontend::tokenizer::newline_handling::NewlineMode;
use crate::compiler_frontend::tokenizer::tokens::{
    CharPosition, FileTokens, SourceLocation, TemplateBodyMode, Token, TokenKind,
};
use crate::projects::html_project::style_directives::html_project_style_directives;
use std::rc::Rc;

#[test]
fn html_directive_rejects_arguments() {
    let style_directives = html_project_test_style_directives();
    let error = template_parse_error_with_style_directives(
        "[$html(\"inline\"):\n<div>Hello</div>\n]",
        &style_directives,
    );
    assert!(error.contains("does not accept arguments"));
}

#[test]
fn html_directive_sets_formatter_via_handler_behavior() {
    let style_directives = html_project_test_style_directives();
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        "[$html:\n<div class=\"card\">x</div>\n]",
        &style_directives,
        &mut string_table,
    );
    let context = new_constant_context_with_style_directives(
        token_stream.src_path.to_owned(),
        &style_directives,
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("html template should parse");

    assert_eq!(template.style.id, "html");
    assert!(template.style.formatter.is_some());
}

#[test]
fn css_directive_sets_style_and_formatter_identity() {
    let style_directives = html_project_test_style_directives();
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        "[$css:\n.button { color: red; }\n]",
        &style_directives,
        &mut string_table,
    );
    let context = new_constant_context_with_style_directives(
        token_stream.src_path.to_owned(),
        &style_directives,
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("css template should parse");

    assert_eq!(template.style.id, "css");
    assert!(template.style.formatter.is_some());
}

#[test]
fn markdown_directive_sets_style_and_formatter_identity() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$markdown:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown template should parse");

    assert_eq!(template.style.id, "markdown");
    assert!(template.style.formatter.is_some());
}

#[test]
fn code_directive_sets_style_and_formatter_identity() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source("[$code:\nloop x\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("code template should parse");

    assert_eq!(template.style.id, "code");
    assert!(template.style.formatter.is_some());
}

#[test]
fn escape_html_directive_sets_style_and_formatter_identity() {
    let style_directives = html_project_test_style_directives();
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        "[$escape_html:\n<b>Hello</b>\n]",
        &style_directives,
        &mut string_table,
    );
    let context = new_constant_context_with_style_directives(
        token_stream.src_path.to_owned(),
        &style_directives,
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("escape_html template should parse");

    assert_eq!(template.style.id, "escape_html");
    assert!(template.style.formatter.is_some());
}

#[test]
fn const_html_template_emits_sanitation_warnings() {
    let style_directives = html_project_test_style_directives();
    let warnings = template_warnings_with_style_directives(
        "[$html:\n<script>alert(1)</script>\n<div onclick=\"run()\"></div>\n<a href=\"javascript:alert(1)\">x</a>\n]",
        false,
        &style_directives,
    );

    assert!(!warnings.is_empty());
    assert!(
        warnings
            .iter()
            .all(|warning| { matches!(warning.warning_kind, WarningKind::MalformedHtmlTemplate) })
    );
    assert!(
        warnings
            .iter()
            .any(|warning| warning.msg.contains("<script"))
    );
    assert!(warnings.iter().any(|warning| warning.msg.contains("on*=")));
    assert!(
        warnings
            .iter()
            .any(|warning| warning.msg.contains("javascript:"))
    );
}

#[test]
fn html_validation_warnings_keep_non_default_locations() {
    let style_directives = html_project_test_style_directives();
    let warnings = template_warnings_with_style_directives(
        "[$html:\n<script>alert(1)</script>\n<a href=\"javascript:bad()\">x</a>\n<div onclick=\"run()\"></div>\n]",
        false,
        &style_directives,
    );

    assert!(!warnings.is_empty());
    assert!(
        warnings
            .iter()
            .all(|warning| !is_default_error_location(&warning.location)),
        "html warnings should keep meaningful source locations"
    );
}

#[test]
fn runtime_html_templates_emit_warnings_for_static_body_segments() {
    let style_directives = html_project_test_style_directives();
    let warnings = template_warnings_with_style_directives(
        "[value, $html:\n<script>alert(1)</script>\n]",
        true,
        &style_directives,
    );
    assert!(!warnings.is_empty());
    assert!(
        warnings
            .iter()
            .all(|warning| matches!(warning.warning_kind, WarningKind::MalformedHtmlTemplate))
    );
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
    // Head references remain in template.content — check there.
    assert!(template_segments(&template).iter().any(|segment| {
        segment.origin == TemplateSegmentOrigin::Head
            && matches!(segment.expression.kind, ExpressionKind::Reference(_))
    }));

    // Formatted body text (after $markdown pass) lives in render_plan, not template.content.
    let body_texts = collect_body_text_from_render_plan(&template, &string_table);
    assert!(
        body_texts
            .iter()
            .any(|text| text.contains("<h1>Hello</h1>")),
        "expected formatted body text to contain markdown-rendered heading in render_plan"
    );
}
