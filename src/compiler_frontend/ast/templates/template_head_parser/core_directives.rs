//! Core directive parsing and guardrails.
//!
//! WHAT:
//! - Handles compiler-owned core directives in template heads.
//! - Keeps slot/insert helper parsing separate from generic style-handler logic.
//!
//! WHY:
//! - Core directives encode language semantics and structural helpers, so their
//!   control flow belongs in one dedicated module.

use super::children_directive::parse_children_style_directive;
use super::directive_args::{
    parse_optional_slot_target_argument, parse_required_slot_name_argument,
    reject_unexpected_directive_arguments,
};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::styles::raw::configure_raw_style;
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, CommentDirectiveKind, SlotKey, Style, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::style_directives::{CoreStyleDirectiveKind, StyleDirectiveKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::return_compiler_error;

pub(super) fn maybe_parse_slot_or_insert_helper_directive(
    directive_kind: &StyleDirectiveKind,
    token_stream: &mut FileTokens,
    template: &mut Template,
    _string_table: &StringTable,
) -> Result<bool, CompilerError> {
    if matches!(
        directive_kind,
        StyleDirectiveKind::Core(CoreStyleDirectiveKind::Slot)
    ) {
        let slot_key = parse_optional_slot_target_argument(token_stream)?;
        template.kind = TemplateType::SlotDefinition(slot_key);
        return Ok(true);
    }

    if matches!(
        directive_kind,
        StyleDirectiveKind::Core(CoreStyleDirectiveKind::Insert)
    ) {
        let slot_name = parse_required_slot_name_argument(token_stream)?;
        template.kind = TemplateType::SlotInsert(SlotKey::named(slot_name));
        return Ok(true);
    }

    Ok(false)
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
            reject_unexpected_directive_arguments(token_stream, "note")?;
            template.kind = TemplateType::Comment(CommentDirectiveKind::Note);
            template.apply_style(Style::default());
        }

        CoreStyleDirectiveKind::Todo => {
            reject_unexpected_directive_arguments(token_stream, "todo")?;
            template.kind = TemplateType::Comment(CommentDirectiveKind::Todo);
            template.apply_style(Style::default());
        }

        CoreStyleDirectiveKind::Doc => {
            reject_unexpected_directive_arguments(token_stream, "doc")?;
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

pub(super) fn mark_template_body_whitespace_style_controlled(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}
