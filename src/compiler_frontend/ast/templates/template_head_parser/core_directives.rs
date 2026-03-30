//! Core directive parsing and guardrails.
//!
//! WHAT:
//! - Handles compiler-owned core directives in template heads.
//! - Enforces slot/insert/comment helper restrictions that must never fall through
//!   to generic handler-based directive logic.
//!
//! WHY:
//! - Core directives encode language semantics and structural helpers, so their
//!   control flow belongs in one dedicated module.

use super::children_directive::parse_children_style_directive;
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::styles::code::configure_code_style;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::styles::raw::configure_raw_style;
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, CommentDirectiveKind, SlotKey, Style, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    parse_required_named_slot_insert_argument, parse_slot_definition_target_argument,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::{CoreStyleDirectiveKind, StyleDirectiveKind};
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::{return_compiler_error, return_syntax_error};

pub(super) fn maybe_parse_slot_or_insert_helper_directive(
    spec_kind: &StyleDirectiveKind,
    token_stream: &mut FileTokens,
    template: &mut Template,
    saw_meaningful_head_item: bool,
    string_table: &StringTable,
) -> Result<bool, CompilerError> {
    if matches!(
        spec_kind,
        StyleDirectiveKind::Core(CoreStyleDirectiveKind::Slot)
    ) {
        if saw_meaningful_head_item {
            return_syntax_error!(
                "Slot helper template heads can only contain '$slot' before the optional body.",
                token_stream.current_location()
            );
        }

        let slot_key = parse_slot_definition_target_argument(token_stream, string_table)?;
        template.kind = TemplateType::SlotDefinition(slot_key);
        return Ok(true);
    }

    if matches!(
        spec_kind,
        StyleDirectiveKind::Core(CoreStyleDirectiveKind::Insert)
    ) {
        if saw_meaningful_head_item {
            return_syntax_error!(
                "Slot helper template heads can only contain '$insert(\"name\")' before the optional body.",
                token_stream.current_location()
            );
        }

        let slot_name = parse_required_named_slot_insert_argument(token_stream, string_table)?;
        template.kind = TemplateType::SlotInsert(SlotKey::named(slot_name));
        return Ok(true);
    }

    Ok(false)
}

pub(super) fn reject_mixed_comment_directive(
    spec_kind: &StyleDirectiveKind,
    saw_meaningful_head_item: bool,
    token_stream: &FileTokens,
    _string_table: &StringTable,
) -> Result<(), CompilerError> {
    if saw_meaningful_head_item
        && matches!(
            spec_kind,
            StyleDirectiveKind::Core(
                CoreStyleDirectiveKind::Note
                    | CoreStyleDirectiveKind::Todo
                    | CoreStyleDirectiveKind::Doc
            )
        )
    {
        return_syntax_error!(
            "Comment template heads cannot mix '$note', '$todo', or '$doc' with other head expressions/directives.",
            token_stream.current_location()
        );
    }

    Ok(())
}

pub(super) fn parse_core_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    directive_name: &str,
    kind: CoreStyleDirectiveKind,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match kind {
        CoreStyleDirectiveKind::Code => {
            // Keep directive-local argument parsing in the code style module.
            configure_code_style(token_stream, template, string_table)?;
        }
        CoreStyleDirectiveKind::Raw => {
            configure_raw_style(template);
        }
        CoreStyleDirectiveKind::Children => {
            parse_children_style_directive(token_stream, context, template, string_table)?;
        }
        CoreStyleDirectiveKind::Fresh => {
            // `$fresh` opt-outs this template from parent-applied `$children(..)`
            // wrappers while still allowing local directives/wrappers in the same head.
            template.apply_style_updates(|style| style.skip_parent_child_wrappers = true);
        }
        CoreStyleDirectiveKind::Note => {
            reject_directive_arguments(token_stream, "note", string_table)?;
            template.kind = TemplateType::Comment(CommentDirectiveKind::Note);
            template.apply_style(Style::default());
        }
        CoreStyleDirectiveKind::Todo => {
            reject_directive_arguments(token_stream, "todo", string_table)?;
            template.kind = TemplateType::Comment(CommentDirectiveKind::Todo);
            template.apply_style(Style::default());
        }
        CoreStyleDirectiveKind::Doc => {
            reject_directive_arguments(token_stream, "doc", string_table)?;
            apply_doc_comment_defaults(template);
        }
        CoreStyleDirectiveKind::Slot | CoreStyleDirectiveKind::Insert => {
            return_compiler_error!(
                "Core style directive '${}' reached generic style parsing but should have been handled by slot helper dispatch.",
                directive_name
            )
        }
    }

    Ok(())
}

pub(crate) fn apply_doc_comment_defaults(template: &mut Template) {
    template.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    template.apply_style(Style::default());
    // Doc comments use Markdown formatting with balanced bracket escaping.
    // Nested child templates are suppressed — `[...]` brackets in the body are
    // treated as literal text.
    apply_markdown_style(template);
    template.apply_style_updates(|style| {
        style.suppress_child_templates = true;
    });
}

fn apply_markdown_style(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.id = "markdown";
        style.formatter = Some(markdown_formatter());
    });
}

/// Rejects parenthesized arguments for directives that do not accept them.
fn reject_directive_arguments(
    token_stream: &FileTokens,
    directive_name: &str,
    _string_table: &StringTable,
) -> Result<(), CompilerError> {
    if token_stream.peek_next_token()
        == Some(&crate::compiler_frontend::tokenizer::tokens::TokenKind::OpenParenthesis)
    {
        return_syntax_error!(
            format!("'${directive_name}' does not accept arguments."),
            token_stream.current_location()
        );
    }

    Ok(())
}

pub(super) fn mark_template_body_whitespace_style_controlled(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}
