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
fn formatter_directive_is_unknown_without_builder_registration() {
    let error = template_parse_error("[$formatter(markdown, 10): body]");

    assert!(error.contains("Style directive '$formatter' is unsupported here"));
    assert!(error.contains("$formatter"));
}

#[test]
fn unknown_style_directives_error_cleanly() {
    let error = template_parse_error("[$unknown: body]");

    assert!(error.contains("Style directive '$unknown' is unsupported here"));
    assert!(error.contains("$unknown"));
}

#[test]
fn ignore_is_rejected_as_unsupported_style_directive() {
    let error = template_parse_error("[$ignore: body]");

    assert!(error.contains("Style directive '$ignore' is unsupported here"));
    assert!(error.contains("$ignore"));
}

#[test]
fn template_head_fallback_unknown_directive_uses_standard_metadata() {
    let tokenization_registry =
        StyleDirectiveRegistry::merged(&[StyleDirectiveSpec::handler_no_op(
            "brand",
            TemplateBodyMode::Normal,
        )])
        .expect("test directive should merge for tokenization");
    let parser_registry = frontend_test_style_directives();

    // Re-parse with a context that lacks '$brand' to exercise template-head fallback dispatch.
    let mut string_table = StringTable::new();
    let mut token_stream = template_tokens_from_source_with_style_directives(
        "[$brand: body]",
        &tokenization_registry,
        &mut string_table,
    );
    let context = new_constant_context_with_style_directives(
        token_stream.src_path.to_owned(),
        &parser_registry,
    );
    let fallback_error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("template-head fallback should reject missing registry directives");

    assert!(
        fallback_error
            .msg
            .contains("Style directive '$brand' is unsupported here")
    );
    assert_eq!(
        fallback_error
            .metadata
            .get(&ErrorMetaDataKey::CompilationStage)
            .map(String::as_str),
        Some("Template Head Parsing")
    );
    assert_eq!(
        fallback_error
            .metadata
            .get(&ErrorMetaDataKey::PrimarySuggestion)
            .map(String::as_str),
        Some(
            "Use a registered style directive here or register this directive in the active project builder."
        )
    );
    assert!(fallback_error.location.start_pos.char_column > 0);
}

#[test]
fn builder_registered_style_directive_parses_as_noop_scaffold() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler_no_op(
        "brand",
        TemplateBodyMode::Normal,
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand: body]",
        &directives,
        &mut string_table,
    );
    let context =
        new_constant_context(token_stream.src_path.to_owned()).with_style_directives(&registry);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("builder-registered directives should parse in scaffold mode");

    assert_eq!(template.style.id, "");
    assert!(matches!(template.kind, TemplateType::String));
}

#[test]
fn builder_effects_only_handler_updates_style_without_formatter() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler(
        "brand",
        TemplateBodyMode::Normal,
        StyleDirectiveHandlerSpec::new(
            None,
            StyleDirectiveEffects {
                style_id: Some("brand"),
                ..StyleDirectiveEffects::default()
            },
            None,
        ),
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand: body]",
        &directives,
        &mut string_table,
    );
    let context =
        new_constant_context(token_stream.src_path.to_owned()).with_style_directives(&registry);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("effects-only directive should parse");

    assert_eq!(template.style.id, "brand");
    assert!(template.style.formatter.is_none());
}

#[test]
fn builder_registered_noop_directive_rejects_parenthesized_arguments_by_default() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler_no_op(
        "brand",
        TemplateBodyMode::Normal,
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand(\"tone\"): body]",
        &directives,
        &mut string_table,
    );
    let context =
        new_constant_context(token_stream.src_path.to_owned()).with_style_directives(&registry);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("default no-op directives should reject parenthesized arguments");

    assert!(error.msg.contains("does not accept arguments"));
}

#[test]
fn builder_registered_handler_directive_accepts_declared_optional_argument_type() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler(
        "brand",
        TemplateBodyMode::Normal,
        StyleDirectiveHandlerSpec::new(
            Some(StyleDirectiveArgumentType::String),
            Default::default(),
            None,
        ),
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand(\"theme\"): body]",
        &directives,
        &mut string_table,
    );
    let context =
        new_constant_context(token_stream.src_path.to_owned()).with_style_directives(&registry);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("provided directives should parse optional arguments when configured");

    assert!(matches!(template.kind, TemplateType::String));
}

#[test]
fn builder_registered_handler_directive_rejects_multiple_arguments() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler(
        "brand",
        TemplateBodyMode::Normal,
        StyleDirectiveHandlerSpec::new(
            Some(StyleDirectiveArgumentType::String),
            Default::default(),
            None,
        ),
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand(\"theme\", \"extra\"): body]",
        &directives,
        &mut string_table,
    );
    let context =
        new_constant_context(token_stream.src_path.to_owned()).with_style_directives(&registry);

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("handler directives should reject multiple arguments");
    assert!(error.msg.contains("accepts at most one argument"));
}

#[test]
fn builder_registered_handler_directive_rejects_runtime_argument_values() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler(
        "brand",
        TemplateBodyMode::Normal,
        StyleDirectiveHandlerSpec::new(
            Some(StyleDirectiveArgumentType::String),
            Default::default(),
            None,
        ),
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand(value): body]",
        &directives,
        &mut string_table,
    );
    let context = runtime_template_context_with_style_directives(
        &token_stream.src_path,
        &registry,
        &mut string_table,
    );

    let error = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect_err("handler directives should reject runtime-only argument values");
    assert!(error.msg.contains("compile-time argument value"));
}

#[test]
fn builder_registered_style_directive_preserves_raw_body_whitespace() {
    let mut string_table = StringTable::new();
    let directives = vec![StyleDirectiveSpec::handler_no_op(
        "brand",
        TemplateBodyMode::Normal,
    )];
    let registry = StyleDirectiveRegistry::merged(&directives)
        .expect("provided directive should merge with core directives");
    let mut token_stream = template_tokens_from_source_with_directives(
        "[$brand:\n    Hello\n    World\n]",
        &directives,
        &mut string_table,
    );
    let context =
        new_constant_context(token_stream.src_path.to_owned()).with_style_directives(&registry);

    let template = Template::new(&mut token_stream, &context, vec![], &mut string_table)
        .expect("builder-registered directives should parse in scaffold mode");
    let folded = fold_template_in_context(&template, &context, &mut string_table);

    assert_eq!(string_table.resolve(folded), "\n    Hello\n    World\n");
}

#[test]
fn builder_directive_cannot_override_builtin_slot_name() {
    let directives = vec![StyleDirectiveSpec::handler_no_op(
        "slot",
        TemplateBodyMode::Normal,
    )];
    let error = StyleDirectiveRegistry::merged(&directives)
        .expect_err("frontend-owned directive overrides should fail during registry merge");
    assert!(
        error
            .msg
            .contains("cannot override frontend-owned directive")
    );
}
