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
use crate::compiler_frontend::ast::templates::styles::css::{
    configure_css_style, validate_css_template,
};
use crate::compiler_frontend::ast::templates::styles::escape_html::configure_escape_html_style;
use crate::compiler_frontend::ast::templates::styles::html::{
    configure_html_style, validate_html_template,
};
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
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveSource;
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

                for path in paths {
                    let interned_path = string_table.get_or_intern(path.to_string(string_table));
                    template.content.add_with_origin(
                        Expression::string_slice(
                            interned_path,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ),
                        TemplateSegmentOrigin::Head,
                    );
                }

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
                            PrimarySuggestion => "Register this directive in the project builder frontend_style_directives list or use a supported built-in directive",
                        }
                    )
                };

                let mut handled_slot_insert = false;

                if spec.source == StyleDirectiveSource::BuiltIn && directive_name == "slot" {
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
                } else if spec.source == StyleDirectiveSource::BuiltIn && directive_name == "insert"
                {
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
                    && spec.source == StyleDirectiveSource::BuiltIn
                    && matches!(directive_name, "note" | "todo" | "doc")
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
                PrimarySuggestion => "Register this directive in the project builder frontend_style_directives list or use a supported built-in directive",
            }
        )
    };

    if spec.source == StyleDirectiveSource::Builder {
        consume_optional_directive_arguments(token_stream, &directive_name, string_table)?;
        mark_template_body_whitespace_style_controlled(template);
        return Ok(false);
    }

    let parse_result = match directive_name.as_str() {
        "markdown" => {
            apply_markdown_style(template);
            Ok(false)
        }

        "code" => {
            // Keep the directive-specific parsing in the code formatter module so
            // this general template parser does not accumulate every built-in style.
            configure_code_style(token_stream, template, string_table)?;
            Ok(false)
        }

        "css" => {
            configure_css_style(token_stream, template, string_table)?;
            Ok(false)
        }

        "html" => {
            configure_html_style(token_stream, template, string_table)?;
            Ok(false)
        }

        "raw" => {
            configure_raw_style(template);
            Ok(false)
        }

        "escape_html" => {
            configure_escape_html_style(template);
            Ok(false)
        }

        "children" => {
            parse_children_style_directive(token_stream, context, template, string_table)?;
            Ok(false)
        }

        "reset" => {
            // `$reset` wipes the inherited style state first, then later directives
            // in the same head can layer fresh settings back on top.
            template.apply_style(Style::default());
            template.apply_style_updates(|style| style.clear_inherited = true);
            Ok(false)
        }

        "note" => {
            reject_directive_arguments(token_stream, "note", string_table)?;
            template.kind = TemplateType::Comment(CommentDirectiveKind::Note);
            template.apply_style(Style::default());
            Ok(false)
        }

        "todo" => {
            reject_directive_arguments(token_stream, "todo", string_table)?;
            template.kind = TemplateType::Comment(CommentDirectiveKind::Todo);
            template.apply_style(Style::default());
            Ok(false)
        }

        "doc" => {
            reject_directive_arguments(token_stream, "doc", string_table)?;
            apply_doc_comment_defaults(template);
            Ok(false)
        }

        other => {
            return_compiler_error!(
                "Built-in style directive '${}' reached AST parsing but has no built-in handler.",
                other
            )
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

pub(crate) fn apply_doc_comment_defaults(template: &mut Template) {
    template.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    template.apply_style(Style::default());
    // Doc comments use markdown formatting with balanced bracket escaping.
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
        style.formatter_precedence = 0;
        style.css_mode = None;
        style.html_mode = false;
    });
}

/// Consumes optional parenthesised arguments for builder-defined directives.
/// These arguments are recognised syntactically but not interpreted by the AST stage.
fn consume_optional_directive_arguments(
    token_stream: &mut FileTokens,
    directive_name: &str,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Ok(());
    }

    // Move from '$directive' to the opening parenthesis.
    token_stream.advance();
    let mut parenthesis_depth = 1usize;

    while parenthesis_depth > 0 {
        token_stream.advance();
        match token_stream.current_token_kind() {
            TokenKind::OpenParenthesis => parenthesis_depth = parenthesis_depth.saturating_add(1),
            TokenKind::CloseParenthesis => parenthesis_depth = parenthesis_depth.saturating_sub(1),
            TokenKind::Eof => {
                return_syntax_error!(
                    format!(
                        "Unexpected end of template head while parsing '${directive_name}(...)'. Missing ')' to close the directive arguments."
                    ),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        PrimarySuggestion => "Close the directive argument list with ')'",
                        SuggestedInsertion => ")",
                    }
                )
            }
            _ => {}
        }
    }

    Ok(())
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

// -------------------------
// POST-PARSE VALIDATION
// -------------------------

/// Emits CSS validation warnings for const templates with `$css` style.
pub(crate) fn emit_css_template_warnings(
    template: &Template,
    context: &ScopeContext,
    string_table: &StringTable,
) {
    if !template.is_const_renderable_string() {
        return;
    }

    let diagnostics = validate_css_template(template, context.build_profile, string_table);
    for diagnostic in diagnostics {
        let file_path = diagnostic.location.scope.to_path_buf(string_table);
        context.emit_warning(CompilerWarning::new(
            &diagnostic.message,
            diagnostic.location.to_error_location(string_table),
            WarningKind::MalformedCssTemplate,
            file_path,
        ));
    }
}

/// Emits HTML validation warnings for const templates with `$html` style.
pub(crate) fn emit_html_template_warnings(
    template: &Template,
    context: &ScopeContext,
    string_table: &StringTable,
) {
    if !template.is_const_renderable_string() {
        return;
    }

    let diagnostics = validate_html_template(template, string_table);
    for diagnostic in diagnostics {
        let file_path = diagnostic.location.scope.to_path_buf(string_table);
        context.emit_warning(CompilerWarning::new(
            &diagnostic.message,
            diagnostic.location.to_error_location(string_table),
            WarningKind::MalformedHtmlTemplate,
            file_path,
        ));
    }
}
