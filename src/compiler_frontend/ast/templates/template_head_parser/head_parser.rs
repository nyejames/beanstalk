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

use super::core_directives::{
    mark_template_body_whitespace_style_controlled, maybe_parse_slot_or_insert_helper_directive,
    parse_core_style_directive, reject_mixed_comment_directive,
};
use super::handler_directives::apply_handler_style_directive;
use super::head_expressions::{
    handle_template_value_in_template_head, push_template_head_expression,
    push_template_head_path_expression,
};
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::ExpressionKind;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::template::{CommentDirectiveKind, TemplateType};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::deferred_feature_diagnostics::unsupported_style_directive_syntax_error;
use crate::compiler_frontend::style_directives::{StyleDirectiveKind, StyleDirectiveSpec};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::projects::settings::BS_VAR_PREFIX;
use crate::{ast_log, return_syntax_error};

// ---------------------
// TEMPLATE HEAD PARSING
// ---------------------
// This can:
// - Change the style of the template
// - Append more content to the template
// - Specify the control flow of the template (is it looped or conditional)
// - Change the ID of the template
// - Add to the list of inherited expressions
// - Make the scene unfoldable
pub fn parse_template_head(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    foldable: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // Control-flow directives in template heads are intentionally deferred.
    // Current head parsing accepts style/settings directives and expressions only.

    template.id = format!("{BS_VAR_PREFIX}templateID_{}", token_stream.index);

    // Each expression must be separated with a comma.
    let mut comma_separator = true;
    let mut saw_meaningful_head_item = false;
    token_stream.advance();

    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing template head: ", #token);

        // We are doing something similar to new_ast()
        // But with the specific scene head syntax,
        // so expressions are allowed and should be folded where possible.
        // Loops and if statements can end the scene head.

        // Returning without a scene body
        // EOF is in here for template repl atm and for the convenience
        // of not having to explicitly close the template head from a repl session.
        // This MIGHT lead to some overly forgiving behavior (not warning about an unclosed template head)
        if token == TokenKind::TemplateClose || token == TokenKind::Eof {
            return Ok(());
        }

        if token == TokenKind::StartTemplateBody {
            if matches!(template.kind, TemplateType::SlotDefinition(_)) {
                return_syntax_error!(
                    "'$slot' markers cannot declare a body. Use '[$slot]' or '[$slot(\"name\")]'.",
                    token_stream.current_location()
                );
            }

            token_stream.advance();
            return Ok(());
        }

        if matches!(
            template.kind,
            TemplateType::SlotDefinition(_)
                | TemplateType::SlotInsert(_)
                | TemplateType::Comment(_)
        ) {
            match token {
                TokenKind::Newline => {
                    token_stream.advance();
                    continue;
                }
                _ => {
                    let restriction_message = match template.kind {
                        TemplateType::SlotDefinition(_) | TemplateType::SlotInsert(_) => {
                            "Slot helper template heads can only contain one '$slot' or '$insert(\"name\")' directive."
                        }
                        TemplateType::Comment(CommentDirectiveKind::Doc) => {
                            "'$doc' template heads can only contain '$doc' before the optional body."
                        }
                        TemplateType::Comment(CommentDirectiveKind::Note) => {
                            "'$note' template heads can only contain '$note' before the optional body."
                        }
                        TemplateType::Comment(CommentDirectiveKind::Todo) => {
                            "'$todo' template heads can only contain '$todo' before the optional body."
                        }
                        TemplateType::String | TemplateType::StringFunction => {
                            "Template helper heads can only contain one helper directive."
                        }
                    };
                    return_syntax_error!(restriction_message, token_stream.current_location())
                }
            }
        }

        // Make sure there is a comma before the next token.
        if !comma_separator {
            if token != TokenKind::Comma {
                return_syntax_error!(
                    format!(
                        "Expected a comma before the next token in the template head. Token: {:?}",
                        token
                    ),
                    token_stream.current_location()
                )
            }

            comma_separator = true;
            token_stream.advance();
            continue;
        }

        let mut defer_separator_token = false;

        match token {
            // If this is a template, we have to do some clever parsing here.
            TokenKind::Symbol(name) => {
                // Check if it's a regular scene or variable reference.
                // If this is a reference to a function or variable.
                if let Some(arg) = context.get_reference(&name) {
                    let value_location = token_stream.current_location();
                    match &arg.value.kind {
                        // Direct template references should preserve wrapper/slot semantics.
                        ExpressionKind::Template(inserted_template) => {
                            handle_template_value_in_template_head(
                                inserted_template.as_ref(),
                                context,
                                template,
                                foldable,
                                &value_location,
                                string_table,
                            )?;
                            saw_meaningful_head_item = true;
                        }

                        // Otherwise this is a reference to some other variable:
                        // string, number, bool, etc.
                        _ => {
                            let expr = create_expression(
                                token_stream,
                                context,
                                &mut DataType::Inferred,
                                &arg.value.ownership,
                                false,
                                string_table,
                            )?;

                            push_template_head_expression(
                                expr,
                                context,
                                template,
                                foldable,
                                &value_location,
                                string_table,
                            )?;
                            defer_separator_token = true;
                            saw_meaningful_head_item = true;
                        }
                    }
                } else {
                    return_syntax_error!(
                        format!(
                            "Cannot declare new variables inside of a template head. Variable '{}' is not declared.",
                            string_table.resolve(name)
                        ),
                        token_stream.current_location()
                    )
                }
            }

            // Constants can be inserted directly into head content.
            TokenKind::FloatLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::RawStringLiteral(_) => {
                let value_location = token_stream.current_location();
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::Inferred,
                    &Ownership::ImmutableOwned,
                    false,
                    string_table,
                )?;

                push_template_head_expression(
                    expr,
                    context,
                    template,
                    foldable,
                    &value_location,
                    string_table,
                )?;
                defer_separator_token = true;
                saw_meaningful_head_item = true;
            }

            TokenKind::Path(paths) => {
                push_template_head_path_expression(
                    &paths,
                    token_stream,
                    context,
                    template,
                    string_table,
                )?;
                saw_meaningful_head_item = true;
            }

            TokenKind::OpenParenthesis => {
                let value_location = token_stream.current_location();
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::Inferred,
                    &Ownership::ImmutableOwned,
                    true,
                    string_table,
                )?;

                push_template_head_expression(
                    expr,
                    context,
                    template,
                    foldable,
                    &value_location,
                    string_table,
                )?;
                defer_separator_token = true;
                saw_meaningful_head_item = true;
            }

            TokenKind::StyleDirective(directive) => {
                // Template directives share the `$name` token shape with style directives.
                // Parse `$slot` / `$insert` first, then fall back to style handling.
                let directive_name = string_table.resolve(directive).to_owned();
                let Some(spec) = context.style_directives.find(&directive_name) else {
                    return Err(unsupported_style_directive_syntax_error(
                        &directive_name,
                        &context
                            .style_directives
                            .supported_directives_for_diagnostic(),
                        token_stream.current_location(),
                        "Template Head Parsing",
                    ));
                };

                let handled_slot_insert = maybe_parse_slot_or_insert_helper_directive(
                    &spec.kind,
                    token_stream,
                    template,
                    saw_meaningful_head_item,
                    string_table,
                )?;

                if handled_slot_insert {
                    saw_meaningful_head_item = true;
                } else {
                    reject_mixed_comment_directive(
                        &spec.kind,
                        saw_meaningful_head_item,
                        token_stream,
                        string_table,
                    )?;

                    defer_separator_token = parse_style_directive_from_spec(
                        token_stream,
                        context,
                        template,
                        &directive_name,
                        spec,
                        string_table,
                    )?;
                    saw_meaningful_head_item = true;
                }
            }

            TokenKind::Comma => {
                // Multiple commas in succession.
                return_syntax_error!(
                    "Multiple commas used back to back in the template head. You must have a valid expression between each comma",
                    token_stream.current_location()
                )
            }

            // Newlines / empty things in the template head are ignored.
            TokenKind::Newline => {
                token_stream.advance();
                continue;
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Invalid Token Used Inside template head when creating template node. Token: {:?}",
                        token
                    ),
                    token_stream.current_location()
                )
            }
        }

        // Guard against malformed or truncated synthetic token streams.
        // Valid streams should include a close/eof boundary, but avoid panicking
        // if expression parsing advanced exactly to the stream end.
        if token_stream.index >= token_stream.length {
            return Ok(());
        }

        if token_stream.current_token_kind() == &TokenKind::StartTemplateBody {
            token_stream.advance();
            return Ok(());
        }

        if matches!(
            token_stream.current_token_kind(),
            TokenKind::TemplateClose | TokenKind::Eof
        ) {
            return Ok(());
        }

        comma_separator = false;
        if !defer_separator_token {
            token_stream.advance();
        }
    }

    Ok(())
}

/// Dispatches a `$directive` token using the already-resolved registry spec.
/// Returns `true` if the caller should defer separator-token advancement because
/// the directive parser consumed trailing tokens directly.
fn parse_style_directive_from_spec(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    directive_name: &str,
    spec: &StyleDirectiveSpec,
    string_table: &mut StringTable,
) -> Result<bool, CompilerError> {
    let parse_result = match &spec.kind {
        StyleDirectiveKind::Core(kind) => parse_core_style_directive(
            token_stream,
            context,
            template,
            directive_name,
            *kind,
            string_table,
        ),
        StyleDirectiveKind::Handler(handler_spec) => apply_handler_style_directive(
            token_stream,
            context,
            template,
            directive_name,
            handler_spec,
            string_table,
        ),
    };

    if parse_result.is_ok() {
        // Any explicit style directive switches the template into style-controlled
        // whitespace mode. Individual formatters can opt into shared whitespace
        // passes explicitly via `Formatter` pre/post pass profiles.
        mark_template_body_whitespace_style_controlled(template);
    }

    parse_result.map(|_| false)
}
