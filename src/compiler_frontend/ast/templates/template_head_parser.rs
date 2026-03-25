//! Template head parsing, style directive dispatch, and validation warnings.
//!
//! WHAT: Parses template head directives (expressions, `$slot`, `$insert`,
//! style settings), dispatches style configuration, and emits post-parse
//! template validation warnings.
//!
//! WHY: Separates head-specific parsing from body parsing and composition,
//! giving each parsing phase a clear input/output contract.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::styles::code::configure_code_style;
use crate::compiler_frontend::ast::templates::styles::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::styles::raw::configure_raw_style;
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, CommentDirectiveKind, SlotKey, Style, TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::ast::templates::template_slots::{
    parse_required_named_slot_insert_argument, parse_slot_definition_target_argument,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::paths::path_format::format_compile_time_paths;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::{
    CoreStyleDirectiveKind, StyleDirectiveArgumentType, StyleDirectiveArgumentValue,
    StyleDirectiveKind,
};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::projects::settings::BS_VAR_PREFIX;
use crate::{ast_log, return_compiler_error, return_syntax_error};

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

    // Each expression must be separated with a comma
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
        // This MIGHT lead to some overly forgiving behaviour (not warning about an unclosed template head)
        if token == TokenKind::TemplateClose || token == TokenKind::Eof {
            return Ok(());
        }

        if token == TokenKind::StartTemplateBody {
            if matches!(template.kind, TemplateType::SlotDefinition(_)) {
                return_syntax_error!(
                    "'$slot' markers cannot declare a body. Use '[$slot]' or '[$slot(\"name\")]'.",
                    token_stream
                        .current_location()
                        .to_error_location(string_table)
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
                    return_syntax_error!(
                        restriction_message,
                        token_stream
                            .current_location()
                            .to_error_location(string_table)
                    )
                }
            }
        }

        // Make sure there is a comma before the next token
        if !comma_separator {
            if token != TokenKind::Comma {
                return_syntax_error!(
                    format!(
                        "Expected a comma before the next token in the template head. Token: {:?}",
                        token
                    ),
                    token_stream
                        .current_location()
                        .to_error_location(string_table)
                )
            }

            comma_separator = true;
            token_stream.advance();
            continue;
        };

        let mut defer_separator_token = false;

        match token {
            // If this is a template, we have to do some clever parsing here
            TokenKind::Symbol(name) => {
                // Check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = context.get_reference(&name) {
                    let value_location = token_stream.current_location();
                    match &arg.value.kind {
                        // Direct template references should preserve wrapper/slot semantics.
                        ExpressionKind::Template(inserted_template) => {
                            handle_template_value_in_template_head(
                                inserted_template,
                                context,
                                template,
                                foldable,
                                &value_location,
                                string_table,
                            )?;
                            saw_meaningful_head_item = true;
                        }

                        // Otherwise this is a reference to some other variable
                        // String, Number, Bool, etc. References
                        _ => {
                            let expr = create_expression(
                                token_stream,
                                context,
                                &mut DataType::CoerceToString,
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
                        token_stream
                            .current_location()
                            .to_error_location(string_table)
                    )
                };
            }

            // Possible Constants to Parse
            // Can chuck these directly into the content
            TokenKind::FloatLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringSliceLiteral(_)
            | TokenKind::RawStringLiteral(_) => {
                let value_location = token_stream.current_location();
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::CoerceToString,
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
                if paths.is_empty() {
                    return_syntax_error!(
                        "Path token in template head cannot be empty.",
                        token_stream
                            .current_location()
                            .to_error_location(string_table)
                    );
                }

                let resolver =
                    context.required_project_path_resolver("template head path coercion")?;
                let source_scope =
                    context.required_source_file_scope("template head path coercion")?;
                let importer_file = source_scope.to_path_buf(string_table);
                let resolved =
                    resolver.resolve_compile_time_paths(&paths, &importer_file, string_table)?;

                // Warn when a .bst source file path is coerced into template output.
                for p in &resolved.paths {
                    if p.filesystem_path
                        .extension()
                        .is_some_and(|ext| ext == "bst")
                    {
                        let location = token_stream.current_location();
                        let file_path = location.scope.to_path_buf(string_table);
                        context.emit_warning(CompilerWarning::new(
                            &format!(
                                "Path to Beanstalk source file is being inserted into template output: '{}'",
                                p.source_path.to_portable_string(string_table)
                            ),
                            location.to_error_location(string_table),
                            WarningKind::BstFilePathInTemplateOutput,
                            file_path,
                        ));
                    }
                }

                // Format the resolved path(s) into a string for template output.
                // Templates always fold to strings, so we convert eagerly here
                // rather than deferring to HIR lowering.
                let formatted =
                    format_compile_time_paths(&resolved, &context.path_format_config, string_table);
                let interned = string_table.get_or_intern(formatted);
                template.content.add_with_origin(
                    Expression::string_slice(
                        interned,
                        token_stream.current_location(),
                        Ownership::ImmutableOwned,
                    ),
                    TemplateSegmentOrigin::Head,
                );

                saw_meaningful_head_item = true;
            }

            TokenKind::OpenParenthesis => {
                let value_location = token_stream.current_location();
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::CoerceToString,
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
                let directive_name = string_table.resolve(directive);
                let Some(spec) = context.style_directives.find(directive_name) else {
                    return_syntax_error!(
                        format!(
                            "Unsupported style directive '${directive_name}'. Registered directives are {}.",
                            context.style_directives.supported_directives_for_diagnostic()
                        ),
                        token_stream
                            .current_location()
                            .to_error_location(string_table),
                        {
                            PrimarySuggestion => "Register this directive in the active project builder style directive list or use a supported core directive",
                        }
                    )
                };

                let mut handled_slot_insert = false;

                if matches!(
                    spec.kind,
                    StyleDirectiveKind::Core(CoreStyleDirectiveKind::Slot)
                ) {
                    if saw_meaningful_head_item {
                        return_syntax_error!(
                            "Slot helper template heads can only contain '$slot' before the optional body.",
                            token_stream
                                .current_location()
                                .to_error_location(string_table)
                        );
                    }

                    let slot_key =
                        parse_slot_definition_target_argument(token_stream, string_table)?;
                    template.kind = TemplateType::SlotDefinition(slot_key);
                    saw_meaningful_head_item = true;
                    handled_slot_insert = true;
                } else if matches!(
                    spec.kind,
                    StyleDirectiveKind::Core(CoreStyleDirectiveKind::Insert)
                ) {
                    if saw_meaningful_head_item {
                        return_syntax_error!(
                            "Slot helper template heads can only contain '$insert(\"name\")' before the optional body.",
                            token_stream
                                .current_location()
                                .to_error_location(string_table)
                        );
                    }

                    let slot_name =
                        parse_required_named_slot_insert_argument(token_stream, string_table)?;
                    template.kind = TemplateType::SlotInsert(SlotKey::named(slot_name));
                    saw_meaningful_head_item = true;
                    handled_slot_insert = true;
                }

                if !handled_slot_insert
                    && saw_meaningful_head_item
                    && matches!(
                        spec.kind,
                        StyleDirectiveKind::Core(
                            CoreStyleDirectiveKind::Note
                                | CoreStyleDirectiveKind::Todo
                                | CoreStyleDirectiveKind::Doc
                        )
                    )
                {
                    return_syntax_error!(
                        "Comment template heads cannot mix '$note', '$todo', or '$doc' with other head expressions/directives.",
                        token_stream
                            .current_location()
                            .to_error_location(string_table)
                    );
                }

                if !handled_slot_insert {
                    defer_separator_token =
                        parse_style_directive(token_stream, context, template, string_table)?;
                    saw_meaningful_head_item = true;
                }
            }

            TokenKind::Comma => {
                // Multiple commas in succession
                return_syntax_error!(
                    "Multiple commas used back to back in the template head. You must have a valid expression between each comma",
                    token_stream
                        .current_location()
                        .to_error_location(string_table)
                )
            }

            // Newlines / empty things in the scene head are ignored
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
                    token_stream
                        .current_location()
                        .to_error_location(string_table)
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

// -------------------------
// HEAD EXPRESSION HANDLING
// -------------------------

/// Handles a template-typed value found in the template head.
/// Wrapper templates preserve slot semantics; runtime templates mark unfoldable.
fn handle_template_value_in_template_head(
    value: &Template,
    context: &ScopeContext,
    template: &mut Template,
    foldable: &mut bool,
    location: &TextLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if context.kind.is_constant_context() && matches!(value.kind, TemplateType::StringFunction) {
        return_syntax_error!(
            "Const templates can only capture compile-time templates.",
            location.to_owned().to_error_location(string_table)
        );
    }

    if matches!(value.kind, TemplateType::Comment(_)) {
        return Ok(());
    }

    if matches!(value.kind, TemplateType::SlotDefinition(_)) {
        return_syntax_error!(
            "'$slot' markers are only valid as direct nested templates inside template bodies.",
            location.to_owned().to_error_location(string_table)
        );
    }

    if matches!(value.kind, TemplateType::StringFunction) {
        *foldable = false;
    }

    template.content.add_with_origin(
        Expression::template(value.to_owned(), Ownership::ImmutableOwned),
        TemplateSegmentOrigin::Head,
    );

    Ok(())
}

/// Pushes a non-template expression into the head content after validation.
fn push_template_head_expression(
    expr: Expression,
    context: &ScopeContext,
    template: &mut Template,
    foldable: &mut bool,
    location: &TextLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if let ExpressionKind::Template(template_value) = &expr.kind {
        return handle_template_value_in_template_head(
            template_value,
            context,
            template,
            foldable,
            location,
            string_table,
        );
    }

    if context.kind.is_constant_context() && !expr.is_compile_time_constant() {
        return_syntax_error!(
            "Const templates can only capture compile-time values in the template head.",
            location.to_owned().to_error_location(string_table)
        );
    }

    if !expr.kind.is_foldable() && !expr.is_compile_time_constant() {
        ast_log!("Template is no longer foldable due to reference");
        *foldable = false;
    }

    template
        .content
        .add_with_origin(expr, TemplateSegmentOrigin::Head);
    Ok(())
}

// -------------------------
// STYLE DIRECTIVE PARSING
// -------------------------

/// Dispatches a `$directive` token to the correct built-in or builder handler.
/// Returns `true` if the caller should defer the next separator token advance
/// (because the style handler already consumed trailing tokens).
fn parse_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    string_table: &mut StringTable,
) -> Result<bool, CompilerError> {
    let directive_name = match token_stream.current_token_kind().clone() {
        TokenKind::StyleDirective(directive) => string_table.resolve(directive).to_owned(),
        _ => {
            return_compiler_error!("Tried to parse a style directive while not positioned at one.")
        }
    };

    let Some(spec) = context.style_directives.find(&directive_name) else {
        return_syntax_error!(
            format!(
                "Unsupported style directive '${directive_name}'. Registered directives are {}.",
                context.style_directives.supported_directives_for_diagnostic(),
            ),
            token_stream
                .current_location()
                .to_error_location(string_table),
            {
                PrimarySuggestion => "Register this directive in the active project builder style directive list or use a supported core directive",
            }
        )
    };

    let parse_result = match &spec.kind {
        StyleDirectiveKind::Core(kind) => match kind {
            CoreStyleDirectiveKind::Code => {
                // Keep directive-local argument parsing in the code style module.
                configure_code_style(token_stream, template, string_table)?;
                Ok(false)
            }
            CoreStyleDirectiveKind::Raw => {
                configure_raw_style(template);
                Ok(false)
            }
            CoreStyleDirectiveKind::Children => {
                parse_children_style_directive(token_stream, context, template, string_table)?;
                Ok(false)
            }
            CoreStyleDirectiveKind::Fresh => {
                // `$fresh` opt-outs this template from parent-applied `$children(..)`
                // wrappers while still allowing local directives/wrappers in the same head.
                template.apply_style_updates(|style| style.skip_parent_child_wrappers = true);
                Ok(false)
            }
            CoreStyleDirectiveKind::Note => {
                reject_directive_arguments(token_stream, "note", string_table)?;
                template.kind = TemplateType::Comment(CommentDirectiveKind::Note);
                template.apply_style(Style::default());
                Ok(false)
            }
            CoreStyleDirectiveKind::Todo => {
                reject_directive_arguments(token_stream, "todo", string_table)?;
                template.kind = TemplateType::Comment(CommentDirectiveKind::Todo);
                template.apply_style(Style::default());
                Ok(false)
            }
            CoreStyleDirectiveKind::Doc => {
                reject_directive_arguments(token_stream, "doc", string_table)?;
                apply_doc_comment_defaults(template);
                Ok(false)
            }
            CoreStyleDirectiveKind::Slot | CoreStyleDirectiveKind::Insert => {
                return_compiler_error!(
                    "Core style directive '${}' reached generic style parsing but should have been handled by slot helper dispatch.",
                    directive_name
                )
            }
        },
        StyleDirectiveKind::Handler(handler_spec) => {
            apply_handler_style_directive(
                token_stream,
                context,
                template,
                &directive_name,
                handler_spec,
                string_table,
            )?;
            Ok(false)
        }
    };

    if parse_result.is_ok() {
        // Any explicit style directive switches the template into style-controlled
        // whitespace mode. Individual formatters can opt into shared whitespace
        // passes explicitly via `Formatter` pre/post pass profiles.
        mark_template_body_whitespace_style_controlled(template);
    }

    parse_result
}

// -------------------------
// STYLE HELPERS
// -------------------------

fn mark_template_body_whitespace_style_controlled(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}

#[derive(Clone)]
struct ParsedHandlerDirectiveArgument {
    value: Option<StyleDirectiveArgumentValue>,
    error_location: TextLocation,
}

fn apply_handler_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    directive_name: &str,
    handler_spec: &crate::compiler_frontend::style_directives::StyleDirectiveHandlerSpec,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let parsed_argument = parse_optional_handler_style_argument(
        token_stream,
        context,
        directive_name,
        handler_spec.argument_type,
        string_table,
    )?;

    apply_style_directive_effects(template, handler_spec.effects);

    if let Some(factory) = handler_spec.formatter_factory {
        // Frontend parsing/folding always executes the formatter factory here. Ownership of the
        // concrete formatter stays with the directive definition module that registered it.
        let formatter = factory(parsed_argument.value.as_ref()).map_err(|message| {
            CompilerError::new_syntax_error(
                &message,
                parsed_argument
                    .error_location
                    .to_error_location(string_table),
            )
        })?;
        template.apply_style_updates(|style| {
            style.formatter = formatter.clone();
        });
    }

    Ok(())
}

fn apply_style_directive_effects(
    template: &mut Template,
    effects: crate::compiler_frontend::style_directives::StyleDirectiveEffects,
) {
    template.apply_style_updates(|style| {
        if let Some(style_id) = effects.style_id {
            style.id = style_id;
        }
        if let Some(policy) = effects.body_whitespace_policy {
            style.body_whitespace_policy = policy;
        }
        if let Some(suppress_child_templates) = effects.suppress_child_templates {
            style.suppress_child_templates = suppress_child_templates;
        }
        if let Some(skip_parent_wrappers) = effects.skip_parent_child_wrappers {
            style.skip_parent_child_wrappers = skip_parent_wrappers;
        }
    });
}

fn parse_optional_handler_style_argument(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    directive_name: &str,
    argument_type: Option<StyleDirectiveArgumentType>,
    string_table: &mut StringTable,
) -> Result<ParsedHandlerDirectiveArgument, CompilerError> {
    let default_location = token_stream.current_location();

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Ok(ParsedHandlerDirectiveArgument {
            value: None,
            error_location: default_location,
        });
    }

    let Some(argument_type) = argument_type else {
        return_syntax_error!(
            format!("'${directive_name}' does not accept arguments."),
            token_stream
                .current_location()
                .to_error_location(string_table)
        );
    };

    // Move from '$directive' to the opening parenthesis, then to the first token inside.
    token_stream.advance();
    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return_syntax_error!(
            format!(
                "'${directive_name}(...)' requires one compile-time argument when parentheses are present."
            ),
            token_stream.current_location().to_error_location(string_table),
            {
                PrimarySuggestion => "Provide exactly one argument inside the directive parentheses",
            }
        );
    }

    let argument_location = token_stream.current_location();
    let mut inferred = DataType::Inferred;
    let parsed_expression = create_expression(
        token_stream,
        context,
        &mut inferred,
        &Ownership::ImmutableOwned,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_syntax_error!(
            format!("'${directive_name}(...)' accepts at most one argument."),
            token_stream
                .current_location()
                .to_error_location(string_table)
        );
    }

    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            format!("Expected ')' after '${directive_name}(...)' argument."),
            token_stream.current_location().to_error_location(string_table),
            {
                SuggestedInsertion => ")",
            }
        );
    }

    if !parsed_expression.is_compile_time_constant() {
        return_syntax_error!(
            format!("'${directive_name}(...)' requires a compile-time argument value."),
            argument_location.to_error_location(string_table),
            {
                PrimarySuggestion => "Use a literal or constant value that folds at compile time",
            }
        );
    }

    let normalized = normalize_provided_style_argument_value(
        parsed_expression,
        argument_type,
        directive_name,
        &argument_location,
        string_table,
    )?;

    Ok(ParsedHandlerDirectiveArgument {
        value: Some(normalized),
        error_location: argument_location,
    })
}

fn normalize_provided_style_argument_value(
    expression: Expression,
    argument_type: StyleDirectiveArgumentType,
    directive_name: &str,
    argument_location: &TextLocation,
    string_table: &StringTable,
) -> Result<StyleDirectiveArgumentValue, CompilerError> {
    match argument_type {
        StyleDirectiveArgumentType::String => match expression.kind {
            ExpressionKind::StringSlice(text) => Ok(StyleDirectiveArgumentValue::String(
                string_table.resolve(text).to_owned(),
            )),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time string argument."),
                    argument_location.to_error_location(string_table)
                )
            }
        },
        StyleDirectiveArgumentType::Template => match expression.kind {
            ExpressionKind::Template(template) => Ok(StyleDirectiveArgumentValue::Template(
                template.as_ref().to_owned(),
            )),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time template argument."),
                    argument_location.to_error_location(string_table)
                )
            }
        },
        StyleDirectiveArgumentType::Number => match expression.kind {
            ExpressionKind::Int(value) => Ok(StyleDirectiveArgumentValue::Number(value as f64)),
            ExpressionKind::Float(value) => Ok(StyleDirectiveArgumentValue::Number(value)),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time numeric argument."),
                    argument_location.to_error_location(string_table)
                )
            }
        },
        StyleDirectiveArgumentType::Bool => match expression.kind {
            ExpressionKind::Bool(value) => Ok(StyleDirectiveArgumentValue::Bool(value)),
            _ => {
                return_syntax_error!(
                    format!("'${directive_name}(...)' expects a compile-time bool argument."),
                    argument_location.to_error_location(string_table)
                )
            }
        },
    }
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

/// Rejects parenthesised arguments for directives that do not accept them.
fn reject_directive_arguments(
    token_stream: &FileTokens,
    directive_name: &str,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if token_stream.peek_next_token() == Some(&TokenKind::OpenParenthesis) {
        return_syntax_error!(
            format!("'${directive_name}' does not accept arguments."),
            token_stream
                .current_location()
                .to_error_location(string_table)
        );
    }

    Ok(())
}

// -------------------------
// $CHILDREN DIRECTIVE
// -------------------------

/// Parses the `$children(template_or_string)` directive which specifies a
/// wrapper template to apply around all direct child templates in the body.
pub(crate) fn parse_children_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_syntax_error!(
            "The '$children(..)' directive requires one argument: a template or string value.",
            token_stream
                .current_location()
                .to_error_location(string_table),
            {
                PrimarySuggestion => "Use '$children([:prefix])' or '$children(\"prefix\")'",
            }
        );
    }

    // Move from '$children' to the first token inside '(' ... ')'
    token_stream.advance();
    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "The '$children(..)' directive cannot be empty. Provide a template or string argument.",
            token_stream
                .current_location()
                .to_error_location(string_table)
        );
    }

    let argument_location = token_stream.current_location();
    let argument = create_expression(
        token_stream,
        context,
        &mut DataType::CoerceToString,
        &Ownership::ImmutableOwned,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "The '$children(..)' directive supports exactly one argument and must end with ')'.",
            token_stream
                .current_location()
                .to_error_location(string_table),
            {
                PrimarySuggestion => "Use '$children(template_or_string)'",
                SuggestedInsertion => ")",
            }
        );
    }

    if !argument.is_compile_time_constant() {
        return_syntax_error!(
            "The '$children(..)' directive only accepts compile-time values.",
            argument_location.to_error_location(string_table),
            {
                PrimarySuggestion => "Use a template literal, string literal, or constant reference that folds at compile time",
            }
        );
    }

    let normalized = match argument.kind {
        ExpressionKind::Template(child_template) => {
            if matches!(
                child_template.kind,
                TemplateType::StringFunction
                    | TemplateType::SlotDefinition(_)
                    | TemplateType::SlotInsert(_)
                    | TemplateType::Comment(_)
            ) {
                return_syntax_error!(
                    "The '$children(..)' directive only accepts compile-time template/string values.",
                    argument_location.to_error_location(string_table)
                );
            }

            child_template.as_ref().to_owned()
        }

        ExpressionKind::StringSlice(value) => {
            let mut wrapper = Template::create_default(vec![]);
            wrapper.kind = TemplateType::String;
            wrapper.location = argument_location.to_owned();
            wrapper.content.add(Expression::string_slice(
                value,
                argument_location,
                Ownership::ImmutableOwned,
            ));
            wrapper.unformatted_content = wrapper.content.to_owned();
            wrapper
        }

        _ => {
            return_syntax_error!(
                "The '$children(..)' directive only accepts template or string arguments.",
                argument_location.to_error_location(string_table)
            )
        }
    };

    template.style.child_templates.push(normalized.to_owned());
    template.explicit_style.child_templates.push(normalized);
    Ok(())
}
