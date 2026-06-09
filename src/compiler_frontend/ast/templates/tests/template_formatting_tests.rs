use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, Formatter, FormatterResult, Style, TemplateContent, TemplateFormatter,
    TemplateSegmentOrigin,
};
use crate::compiler_frontend::ast::templates::template_formatting::apply_body_formatter;
use crate::compiler_frontend::ast::templates::template_render_plan::{
    FormatterAnchorId, FormatterInput, FormatterOpaqueKind, FormatterOpaquePiece, FormatterOutput,
    FormatterOutputPiece,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::value_mode::ValueMode;
use std::sync::Arc;

fn make_body_text_content(text: &str, string_table: &mut StringTable) -> TemplateContent {
    let mut content = TemplateContent::default();
    content.add_with_origin(
        Expression::string_slice(
            string_table.intern(text),
            SourceLocation::default(),
            ValueMode::ImmutableOwned,
        ),
        TemplateSegmentOrigin::Body,
    );
    content
}

fn default_style() -> Style {
    Style {
        body_whitespace_policy: BodyWhitespacePolicy::DefaultTemplateBehavior,
        ..Style::default()
    }
}

#[test]
fn no_op_formatting_detects_no_change_for_plain_text() {
    let mut string_table = StringTable::new();
    let content = make_body_text_content("Hello world", &mut string_table);
    let style = default_style();

    let result = apply_body_formatter(&content, &style, &mut string_table)
        .expect("formatting should succeed");

    assert!(
        !result.content_changed,
        "plain text with no formatter should not report content_changed"
    );
}

#[test]
fn no_op_formatting_detects_no_change_for_empty_body() {
    let mut string_table = StringTable::new();
    let content = TemplateContent::default();
    let style = default_style();

    let result = apply_body_formatter(&content, &style, &mut string_table)
        .expect("formatting should succeed");

    assert!(
        !result.content_changed,
        "empty body should not report content_changed"
    );
}

#[test]
fn no_op_formatting_detects_no_change_for_dynamic_expression_anchor() {
    let mut string_table = StringTable::new();
    let mut content = TemplateContent::default();
    content.add_with_origin(
        Expression::int(42, SourceLocation::default(), ValueMode::ImmutableOwned),
        TemplateSegmentOrigin::Body,
    );
    let style = default_style();

    let result = apply_body_formatter(&content, &style, &mut string_table)
        .expect("formatting should succeed");

    assert!(
        !result.content_changed,
        "dynamic expression anchor with no formatter should not report content_changed"
    );
}

struct InvalidAnchorFormatter;

impl TemplateFormatter for InvalidAnchorFormatter {
    fn format(
        &self,
        _input: FormatterInput,
        string_table: &mut StringTable,
    ) -> Result<FormatterResult, CompilerMessages> {
        let _ = string_table;

        Ok(FormatterResult {
            output: FormatterOutput {
                pieces: vec![FormatterOutputPiece::Opaque(FormatterOpaquePiece {
                    id: FormatterAnchorId(999),
                    kind: FormatterOpaqueKind::ChildTemplate,
                })],
            },
            warnings: Vec::new(),
        })
    }
}

#[test]
fn invalid_formatter_anchor_returns_error() {
    let mut string_table = StringTable::new();
    let mut content = TemplateContent::default();
    content.add_with_origin(
        Expression::int(42, SourceLocation::default(), ValueMode::ImmutableOwned),
        TemplateSegmentOrigin::Body,
    );

    let mut style = default_style();
    style.formatter = Some(Formatter {
        pre_format_whitespace_passes: Vec::new(),
        formatter: Arc::new(InvalidAnchorFormatter),
        post_format_whitespace_passes: Vec::new(),
    });

    let err = match apply_body_formatter(&content, &style, &mut string_table) {
        Ok(_) => panic!("invalid anchor should produce an error"),
        Err(e) => e,
    };

    let msg = err
        .first_infrastructure_error_for_tests()
        .map(|(_error_type, message, _location)| message)
        .unwrap_or("");
    assert!(
        msg.contains("invalid opaque anchor id 999"),
        "expected anchor id in error message, got: {msg}"
    );
    assert!(
        msg.contains("only 1 anchors exist"),
        "expected anchor count in error message, got: {msg}"
    );
}

#[test]
fn empty_constructor_produces_default_semantic_shape() {
    let template = Template::empty();
    assert!(template.content.is_empty());
    assert!(template.unformatted_content.is_empty());
    assert!(!template.content_needs_formatting);
    assert!(template.render_plan.is_none());
    assert!(template.doc_children.is_empty());
    assert!(template.id.is_empty());
}
