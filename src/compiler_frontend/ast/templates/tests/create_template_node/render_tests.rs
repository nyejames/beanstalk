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
fn markdown_formatter_output_text_uses_non_default_render_plan_locations() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$markdown:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown template should parse");
    let locations = collect_body_text_locations_from_render_plan(&template);

    assert!(
        !locations.is_empty(),
        "expected body text pieces in render plan"
    );
    assert!(
        locations
            .iter()
            .all(|location| !is_default_text_location(location)),
        "formatter-emitted body text should keep coarse source provenance"
    );
}

#[test]
fn unformatted_content_preserves_pre_format_composed_structure() {
    let mut string_table = StringTable::new();
    let mut token_stream =
        template_tokens_from_source("[$markdown:\n# Hello\n]", &mut string_table);
    let context = new_constant_context(token_stream.src_path.to_owned());

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("markdown template should parse");

    let mut unformatted_rendered = String::new();
    collect_static_template_fragments(
        &template.unformatted_content.atoms,
        &string_table,
        &mut unformatted_rendered,
    );
    let formatted_body = collect_body_text_from_render_plan(&template, &string_table);

    assert!(
        unformatted_rendered.contains("# Hello"),
        "unformatted_content should keep pre-format source text"
    );
    assert!(
        formatted_body
            .iter()
            .any(|text| text.contains("<h1>Hello</h1>")),
        "render plan should carry formatted markdown output"
    );
}
