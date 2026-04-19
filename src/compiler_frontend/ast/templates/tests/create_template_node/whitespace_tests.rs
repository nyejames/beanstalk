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
fn default_whitespace_normalizer_trims_initial_newline_and_dedents() {
    let rendered = folded_template_output("[:\n    Hello\n    World\n]");
    assert_eq!(rendered, "Hello\nWorld");
}

#[test]
fn default_whitespace_normalizer_preserves_consecutive_blank_lines() {
    let rendered = folded_template_output("[:\n    Hello\n\n    World\n]");
    assert_eq!(rendered, "Hello\n\nWorld");
}

#[test]
fn default_whitespace_normalizer_trims_only_from_final_newline() {
    let rendered = folded_template_output("[:\n    Hello   \n]");
    assert_eq!(rendered, "Hello   ");
}

#[test]
fn default_whitespace_normalizer_preserves_leading_spaces_without_initial_newline() {
    let rendered = folded_template_output("[: world]");
    assert_eq!(rendered, " world");
}

#[test]
fn default_whitespace_normalizer_preserves_middle_run_newline_boundaries() {
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source(
        "[:\n    before\n    [value]\n    after\n]",
        &mut string_table,
    );
    let context = runtime_template_context(&token_stream.src_path, &mut string_table);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    // Formatting is applied to the render plan, not written back into template.content.
    // Read the whitespace-normalised body text slices from render_plan pieces directly.
    let body_slices = collect_body_text_from_render_plan(&template, &string_table);

    assert_eq!(
        body_slices,
        vec!["before\n".to_string(), "\nafter".to_string()]
    );
}

#[test]
fn raw_directive_preserves_authored_whitespace() {
    let rendered = folded_template_output("[$raw:\n    Hello\n    World\n]");
    assert_eq!(rendered, "\n    Hello\n    World\n");
}

#[test]
fn escape_html_escapes_body_html_sensitive_characters() {
    let style_directives = html_project_test_style_directives();
    let rendered = folded_template_output_with_style_directives(
        "[$escape_html:\n    <b>Hello & \"World\" 'x'</b>\n]",
        &style_directives,
    );

    assert!(rendered.contains("&lt;b&gt;Hello &amp; &quot;World&quot; &#39;x&#39;&lt;/b&gt;"));
    assert!(!rendered.contains("<b>Hello"));
}

#[test]
fn escape_html_preserves_runtime_head_references() {
    let style_directives = html_project_test_style_directives();
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        "[value, $escape_html:\n    <b>body</b>\n]",
        &style_directives,
        &mut string_table,
    );
    let context = runtime_template_context_with_style_directives(
        &token_stream.src_path,
        &style_directives,
        &mut string_table,
    );

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("template should parse");

    // Head references remain in template.content — check there.
    assert!(template_segments(&template).iter().any(|segment| {
        segment.origin == TemplateSegmentOrigin::Head
            && matches!(segment.expression.kind, ExpressionKind::Reference(_))
    }));

    // Formatted body text is in render_plan after the escape pass — check there.
    let body_texts = collect_body_text_from_render_plan(&template, &string_table);
    let escaped_body = body_texts
        .into_iter()
        .find(|text| !text.is_empty())
        .expect("expected escaped body text in render plan");

    assert_eq!(escaped_body, "\n    &lt;b&gt;body&lt;/b&gt;\n");
}
