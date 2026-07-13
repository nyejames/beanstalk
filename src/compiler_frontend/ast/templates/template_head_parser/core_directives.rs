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
use crate::compiler_frontend::ast::templates::template_build_state::TemplateBuildState;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::style_directives::{CoreStyleDirectiveKind, StyleDirectiveKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;

/// Boxed diagnostic result for the connected core-directive family.
///
/// Slot/insert dispatch and generic core-style directive parsing propagate
/// diagnostics through one small boxed boundary, then unbox once at the
/// genuine plain-diagnostic head-parser caller.
type CoreDirectiveResult<T> = Result<T, Box<CompilerDiagnostic>>;

pub(super) fn maybe_parse_slot_or_insert_helper_directive(
    directive_kind: &StyleDirectiveKind,
    token_stream: &mut FileTokens,
    build_state: &mut TemplateBuildState,
    string_table: &mut StringTable,
) -> CoreDirectiveResult<bool> {
    if matches!(
        directive_kind,
        StyleDirectiveKind::Core(CoreStyleDirectiveKind::Slot)
    ) {
        let slot_name = string_table.intern("slot");
        let slot_key = parse_optional_slot_target_argument(slot_name, token_stream, string_table)?;
        build_state.kind = TemplateType::SlotDefinition(slot_key);
        return Ok(true);
    }

    if matches!(
        directive_kind,
        StyleDirectiveKind::Core(CoreStyleDirectiveKind::Insert)
    ) {
        let insert_name = string_table.intern("insert");
        let slot_name = parse_required_slot_name_argument(insert_name, token_stream)?;
        build_state.kind = TemplateType::SlotInsert(SlotKey::named(slot_name));
        return Ok(true);
    }

    Ok(false)
}

pub(super) fn parse_core_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    build_state: &mut TemplateBuildState,
    directive_name: &str,
    kind: CoreStyleDirectiveKind,
    string_table: &mut StringTable,
) -> CoreDirectiveResult<()> {
    match kind {
        CoreStyleDirectiveKind::Raw => {
            let raw_name = string_table.intern("raw");
            reject_unexpected_directive_arguments(raw_name, token_stream)?;
            configure_raw_style(build_state);
        }

        CoreStyleDirectiveKind::Children => {
            let children_name = string_table.intern("children");
            // The children-directive family already returns `Box<CompilerDiagnostic>`,
            // so it propagates directly through this boxed boundary without unboxing.
            parse_children_style_directive(
                children_name,
                token_stream,
                context,
                type_interner,
                build_state,
                string_table,
            )?;
        }

        CoreStyleDirectiveKind::Fresh => {
            // `$fresh` opt-outs this template from parent-applied `$children(..)`
            // wrappers while still allowing local directives/wrappers in the same head.
            build_state.apply_style_updates(|style| style.skip_parent_child_wrappers = true);
        }

        CoreStyleDirectiveKind::Note => {
            let note_name = string_table.intern("note");
            reject_unexpected_directive_arguments(note_name, token_stream)?;
            build_state.kind = TemplateType::Comment(CommentDirectiveKind::Note);
            build_state.apply_style(Style::default());
        }

        CoreStyleDirectiveKind::Todo => {
            let todo_name = string_table.intern("todo");
            reject_unexpected_directive_arguments(todo_name, token_stream)?;
            build_state.kind = TemplateType::Comment(CommentDirectiveKind::Todo);
            build_state.apply_style(Style::default());
        }

        CoreStyleDirectiveKind::Doc => {
            let doc_name = string_table.intern("doc");
            reject_unexpected_directive_arguments(doc_name, token_stream)?;
            apply_doc_comment_defaults(build_state);
        }

        CoreStyleDirectiveKind::Slot | CoreStyleDirectiveKind::Insert => {
            return Err(Box::new(
                CompilerError::new(
                    format!(
                        "Core style directive '{directive_name}' reached generic style parsing but should have been handled by slot helper dispatch."
                    ),
                    token_stream.current_location(),
                    ErrorType::Compiler,
                )
                .into(),
            ));
        }
    }

    Ok(())
}

pub(crate) fn apply_doc_comment_defaults(build_state: &mut TemplateBuildState) {
    build_state.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    build_state.apply_style(Style::default());

    // Doc comments use Markdown formatting with balanced bracket escaping.
    // Nested child templates are suppressed — `[...]` brackets in the body are
    // treated as literal text.
    apply_markdown_style(build_state);
    build_state.apply_style_updates(|style| {
        style.suppress_child_templates = true;
    });
}

fn apply_markdown_style(build_state: &mut TemplateBuildState) {
    build_state.apply_style_updates(|style| {
        style.id = "markdown";
        style.formatter = Some(markdown_formatter());
    });
}

pub(super) fn mark_template_body_whitespace_style_controlled(build_state: &mut TemplateBuildState) {
    build_state.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}
