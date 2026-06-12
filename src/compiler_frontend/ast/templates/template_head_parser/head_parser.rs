//! Template head parsing orchestration.
//!
//! WHAT:
//! - Implements the top-level `parse_template_head(...)` loop.
//! - Owns token-category dispatch, separator handling, and stream-boundary checks.
//! - Delegates expression and directive behavior to focused helper modules.
//!
//! WHY:
//! - Keeps the main head parser readable while preserving strict control of which
//!   token kinds are valid in the head grammar.

#![allow(clippy::result_large_err)]

use super::control_flow_suffix::{parse_if_suffix, parse_loop_suffix};
use super::core_directives::{
    mark_template_body_whitespace_style_controlled, maybe_parse_slot_or_insert_helper_directive,
    parse_core_style_directive,
};
use super::handler_directives::apply_handler_style_directive;
use super::head_expressions::{
    handle_template_value_in_template_head, push_template_head_expression,
    push_template_head_path_expression,
};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::template_control_flow::{
    TemplateBodyParseMode, TemplateControlFlowValidationMode,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;

use crate::ast_log;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidTemplateDirectiveReason, InvalidTemplateStructureReason,
};
use crate::compiler_frontend::style_directives::{
    StyleDirectiveKind, StyleDirectiveSpec, TemplateHeadCompatibility, TemplateHeadTag,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind, path_token_paths};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::utilities::token_scan::NestingDepth;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::projects::settings::BS_VAR_PREFIX;

/// Result of parsing a template head.
pub(crate) struct ParsedTemplateHead {
    pub(crate) body_mode: TemplateBodyParseMode,
}

#[derive(Clone, Copy, Debug, Default)]
struct TemplateHeadState {
    seen_tags: TemplateHeadTag,
    blocked_future_tags: TemplateHeadTag,
}

fn enforce_head_compatibility(
    state: &TemplateHeadState,
    incoming: &TemplateHeadCompatibility,
    token_stream: &FileTokens,
) -> Result<(), CompilerDiagnostic> {
    if !state.blocked_future_tags.intersects(incoming.presence_tags)
        && !state.seen_tags.intersects(incoming.required_absent_tags)
    {
        Ok(())
    } else {
        Err(CompilerDiagnostic::invalid_template_structure(
            InvalidTemplateStructureReason::IncompatibleHeadItem,
            token_stream.current_location(),
        ))
    }
}

fn apply_head_compatibility(
    state: &mut TemplateHeadState,
    compatibility: &TemplateHeadCompatibility,
) {
    state.seen_tags |= compatibility.presence_tags;
    state.blocked_future_tags |= compatibility.blocks_future_tags;
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TemplateHeadSeparatorState {
    ExpectItem,
    ExpectSeparatorOrBody,
}

/// Parses meaningful head items until `:`, `]`, or a control-flow suffix body.
///
/// The explicit early returns make token-state exits visible in this parser
/// state machine: each accepted boundary or diagnostic exits immediately.
pub fn parse_template_head(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    template: &mut Template,
    foldable: &mut bool,
    control_flow_validation: TemplateControlFlowValidationMode,
    string_table: &mut StringTable,
) -> Result<ParsedTemplateHead, CompilerDiagnostic> {
    template.id = format!("{BS_VAR_PREFIX}templateID_{}", token_stream.index);

    // Each meaningful head item must be separated with a comma before another
    // item can start. Naming the state keeps suffix and body-boundary handling
    // readable in this parser state machine.
    let mut separator_state = TemplateHeadSeparatorState::ExpectItem;
    let mut head_state = TemplateHeadState::default();
    let meaningful_item_compatibility = TemplateHeadCompatibility::fully_compatible_meaningful();
    token_stream.advance();

    let mut last_known_location = token_stream.current_location();
    while token_stream.index < token_stream.length {
        last_known_location = token_stream.current_location();
        let token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing template head: ", #token);

        // We are doing something similar to new_ast()
        // But with the specific template head syntax,
        // expressions are allowed and should be folded where possible.
        // Loops and if statements can end the template head.

        // EOF inside a template head means the source was truncated before a
        // closing ] delimiter. This is a malformed template, not a valid stream
        // boundary; the user needs a structured diagnostic.
        if token == TokenKind::Eof {
            return Err(CompilerDiagnostic::unexpected_end_of_file(
                Some(string_table.intern("]")),
                token_stream.current_location(),
            ));
        }

        // A closing ] without a body is a valid empty template.
        if token == TokenKind::TemplateClose {
            return Ok(ParsedTemplateHead {
                body_mode: TemplateBodyParseMode::Normal,
            });
        }

        if token == TokenKind::StartTemplateBody {
            if head_state
                .seen_tags
                .intersects(TemplateHeadTag::SLOT_DIRECTIVE)
            {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::SlotInHead,
                    token_stream.current_location(),
                ));
            }

            token_stream.advance();
            return Ok(ParsedTemplateHead {
                body_mode: TemplateBodyParseMode::Normal,
            });
        }

        if separator_state == TemplateHeadSeparatorState::ExpectItem
            && !matches!(token, TokenKind::If | TokenKind::Loop)
            && let Some(control_flow_location) = find_unseparated_control_flow_suffix(token_stream)
        {
            return Err(CompilerDiagnostic::invalid_template_structure(
                InvalidTemplateStructureReason::MissingCommaBeforeControlFlowSuffix,
                control_flow_location,
            ));
        }

        // Make sure there is a comma before the next token.
        if separator_state == TemplateHeadSeparatorState::ExpectSeparatorOrBody {
            if matches!(token, TokenKind::If | TokenKind::Loop) {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::MissingCommaBeforeControlFlowSuffix,
                    token_stream.current_location(),
                ));
            }

            if token != TokenKind::Comma {
                return Err(CompilerDiagnostic::expected_token(
                    TokenKind::Comma,
                    Some(token),
                    token_stream.current_location(),
                ));
            }

            separator_state = TemplateHeadSeparatorState::ExpectItem;
            token_stream.advance();
            continue;
        }

        let mut defer_comma_advance = false;

        match token {
            TokenKind::If => {
                if head_state
                    .seen_tags
                    .intersects(TemplateHeadTag::SLOT_DIRECTIVE)
                {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::SlotInHead,
                        token_stream.current_location(),
                    ));
                }

                let body_mode = parse_if_suffix(
                    token_stream,
                    context,
                    type_interner,
                    control_flow_validation,
                    string_table,
                )?;
                return Ok(ParsedTemplateHead { body_mode });
            }

            TokenKind::Loop => {
                if head_state
                    .seen_tags
                    .intersects(TemplateHeadTag::SLOT_DIRECTIVE)
                {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::SlotInHead,
                        token_stream.current_location(),
                    ));
                }

                let body_mode = parse_loop_suffix(
                    token_stream,
                    context,
                    type_interner,
                    control_flow_validation,
                    string_table,
                )?;
                return Ok(ParsedTemplateHead { body_mode });
            }

            TokenKind::Else => {
                return Err(CompilerDiagnostic::invalid_template_structure(
                    InvalidTemplateStructureReason::ElseInTemplateHead,
                    token_stream.current_location(),
                ));
            }

            // Variable and template references
            TokenKind::Symbol(name) => {
                // Check if it's a regular template reference or variable reference.
                // If this is a reference to a function or variable.
                if let Some(reference) = context.get_reference(&name) {
                    enforce_head_compatibility(
                        &head_state,
                        &meaningful_item_compatibility,
                        token_stream,
                    )?;
                    let value_location = token_stream.current_location();
                    match &reference.value.kind {
                        // Direct template references should preserve wrapper/slot semantics.
                        ExpressionKind::Template(inserted_template) => {
                            handle_template_value_in_template_head(
                                inserted_template.as_ref(),
                                context,
                                template,
                                foldable,
                                &value_location,
                            )?;
                        }

                        // Otherwise this is a reference to some other variable:
                        // string, number, bool, etc.
                        _ => {
                            let mut inferred = ExpectedType::Infer;
                            let expression = create_expression(
                                token_stream,
                                context,
                                type_interner,
                                &mut inferred,
                                &reference.value.value_mode,
                                false,
                                string_table,
                            )?;

                            push_template_head_expression(
                                expression,
                                context,
                                type_interner.environment(),
                                template,
                                foldable,
                                &value_location,
                            )?;
                            defer_comma_advance = true;
                        }
                    }

                    apply_head_compatibility(&mut head_state, &meaningful_item_compatibility);
                } else {
                    return Err(CompilerDiagnostic::unexpected_token(
                        TokenKind::Symbol(name),
                        token_stream.current_location(),
                    ));
                }
            }

            // Receiver self-reference
            TokenKind::This => {
                let this_id = string_table.intern("this");
                if let Some(reference) = context.get_reference(&this_id) {
                    enforce_head_compatibility(
                        &head_state,
                        &meaningful_item_compatibility,
                        token_stream,
                    )?;
                    let value_location = token_stream.current_location();
                    let mut inferred = ExpectedType::Infer;
                    let expression = create_expression(
                        token_stream,
                        context,
                        type_interner,
                        &mut inferred,
                        &reference.value.value_mode,
                        false,
                        string_table,
                    )?;

                    push_template_head_expression(
                        expression,
                        context,
                        type_interner.environment(),
                        template,
                        foldable,
                        &value_location,
                    )?;
                    defer_comma_advance = true;
                    apply_head_compatibility(&mut head_state, &meaningful_item_compatibility);
                } else {
                    return Err(CompilerDiagnostic::unexpected_token(
                        TokenKind::This,
                        token_stream.current_location(),
                    ));
                }
            }

            // Constants can be inserted directly into head content.
            // Literal values
            TokenKind::FloatLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::RawStringLiteral(_) => {
                enforce_head_compatibility(
                    &head_state,
                    &meaningful_item_compatibility,
                    token_stream,
                )?;
                let value_location = token_stream.current_location();
                let mut inferred = ExpectedType::Infer;
                let expression = create_expression(
                    token_stream,
                    context,
                    type_interner,
                    &mut inferred,
                    &ValueMode::ImmutableOwned,
                    false,
                    string_table,
                )?;

                push_template_head_expression(
                    expression,
                    context,
                    type_interner.environment(),
                    template,
                    foldable,
                    &value_location,
                )?;
                defer_comma_advance = true;
                apply_head_compatibility(&mut head_state, &meaningful_item_compatibility);
            }

            // Import path references
            TokenKind::Path(items) => {
                enforce_head_compatibility(
                    &head_state,
                    &meaningful_item_compatibility,
                    token_stream,
                )?;
                if items.iter().any(|item| item.alias.is_some()) {
                    return Err(CompilerDiagnostic::invalid_template_structure(
                        InvalidTemplateStructureReason::PathAliasInTemplateHead,
                        token_stream.current_location(),
                    ));
                }
                let paths = path_token_paths(&items);
                push_template_head_path_expression(
                    &paths,
                    token_stream,
                    context,
                    template,
                    string_table,
                )?;
                apply_head_compatibility(&mut head_state, &meaningful_item_compatibility);
            }

            // Parenthesized sub-expressions
            TokenKind::OpenParenthesis => {
                enforce_head_compatibility(
                    &head_state,
                    &meaningful_item_compatibility,
                    token_stream,
                )?;
                let value_location = token_stream.current_location();
                let mut inferred = ExpectedType::Infer;
                let expression = create_expression(
                    token_stream,
                    context,
                    type_interner,
                    &mut inferred,
                    &ValueMode::ImmutableOwned,
                    true,
                    string_table,
                )?;

                push_template_head_expression(
                    expression,
                    context,
                    type_interner.environment(),
                    template,
                    foldable,
                    &value_location,
                )?;
                defer_comma_advance = true;
                apply_head_compatibility(&mut head_state, &meaningful_item_compatibility);
            }

            // Style and setting directives
            TokenKind::StyleDirective(directive) => {
                // Template directives share the `$name` token shape with style directives.
                // Parse `$slot` / `$insert` first, then fall back to style handling.
                let directive_name = string_table.resolve(directive).to_owned();
                let Some(spec) = context.style_directives.find(&directive_name) else {
                    return Err(CompilerDiagnostic::invalid_template_directive(
                        Some(directive),
                        InvalidTemplateDirectiveReason::UnknownDirective,
                        token_stream.current_location(),
                    ));
                };

                enforce_head_compatibility(&head_state, &spec.head_compatibility, token_stream)?;

                let handled_slot_insert = maybe_parse_slot_or_insert_helper_directive(
                    &spec.kind,
                    token_stream,
                    template,
                )?;

                if handled_slot_insert {
                    apply_head_compatibility(&mut head_state, &spec.head_compatibility);
                } else {
                    defer_comma_advance = parse_style_directive_from_spec(
                        token_stream,
                        context,
                        type_interner,
                        template,
                        &directive_name,
                        spec,
                        string_table,
                    )?;
                    apply_head_compatibility(&mut head_state, &spec.head_compatibility);
                }
            }

            // Separators
            TokenKind::Comma => {
                // Multiple commas in succession.
                return Err(CompilerDiagnostic::unexpected_token(
                    TokenKind::Comma,
                    token_stream.current_location(),
                ));
            }

            // Newlines / empty things in the template head are ignored.
            // Whitespace
            TokenKind::Newline => {
                token_stream.advance();
                continue;
            }

            _ => {
                return Err(CompilerDiagnostic::unexpected_token(
                    token,
                    token_stream.current_location(),
                ));
            }
        }

        // Guard against malformed or truncated synthetic token streams.
        if token_stream.index >= token_stream.length {
            return Err(CompilerDiagnostic::unexpected_end_of_file(
                Some(string_table.intern("]")),
                last_known_location,
            ));
        }

        if token_stream.current_token_kind() == &TokenKind::StartTemplateBody {
            token_stream.advance();
            return Ok(ParsedTemplateHead {
                body_mode: TemplateBodyParseMode::Normal,
            });
        }

        if token_stream.current_token_kind() == &TokenKind::Eof {
            return Err(CompilerDiagnostic::unexpected_end_of_file(
                Some(string_table.intern("]")),
                token_stream.current_location(),
            ));
        }

        if token_stream.current_token_kind() == &TokenKind::TemplateClose {
            return Ok(ParsedTemplateHead {
                body_mode: TemplateBodyParseMode::Normal,
            });
        }

        separator_state = TemplateHeadSeparatorState::ExpectSeparatorOrBody;
        if !defer_comma_advance {
            token_stream.advance();
        }
    }

    Err(CompilerDiagnostic::unexpected_end_of_file(
        Some(string_table.intern("]")),
        last_known_location,
    ))
}

/// Dispatches a `$directive` token using the already-resolved registry spec.
/// Returns `true` if the caller should defer separator-token advancement because
/// the directive parser consumed trailing tokens directly.
fn parse_style_directive_from_spec(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    template: &mut Template,
    directive_name: &str,
    spec: &StyleDirectiveSpec,
    string_table: &mut StringTable,
) -> Result<bool, CompilerDiagnostic> {
    let directive_result = match &spec.kind {
        StyleDirectiveKind::Core(kind) => parse_core_style_directive(
            token_stream,
            context,
            type_interner,
            template,
            directive_name,
            *kind,
            string_table,
        ),
        StyleDirectiveKind::Handler(handler_spec) => apply_handler_style_directive(
            token_stream,
            context,
            type_interner,
            template,
            directive_name,
            handler_spec,
            string_table,
        ),
    };

    if directive_result.is_ok() {
        // Any explicit style directive switches the template into style-controlled
        // whitespace mode. Individual formatters can opt into shared whitespace
        // passes explicitly via `Formatter` pre/post pass profiles.
        mark_template_body_whitespace_style_controlled(template);
    }

    directive_result.map(|_| false)
}

/// Scans ahead for unseparated `if` / `loop` suffix tokens.
///
/// Early returns make the first top-level boundary or suffix location explicit.
fn find_unseparated_control_flow_suffix(
    token_stream: &FileTokens,
) -> Option<crate::compiler_frontend::tokenizer::tokens::SourceLocation> {
    let mut nesting_depth = NestingDepth::default();
    let mut index = token_stream.index + 1;

    while index < token_stream.length {
        let token = &token_stream.tokens[index];

        if nesting_depth.is_top_level() {
            match token.kind {
                TokenKind::Comma | TokenKind::StartTemplateBody | TokenKind::TemplateClose => {
                    return None;
                }
                TokenKind::If | TokenKind::Loop => {
                    return Some(token.location.clone());
                }
                _ => {}
            }
        }

        nesting_depth.step(&token.kind);
        index += 1;
    }

    None
}
