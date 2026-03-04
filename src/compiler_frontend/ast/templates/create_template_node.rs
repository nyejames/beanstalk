#![allow(clippy::needless_borrow)]

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::code::configure_code_style;
use crate::compiler_frontend::ast::templates::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::slots::{
    compose_template_with_slots, ensure_no_slot_insertions_remain,
};
use crate::compiler_frontend::ast::templates::template::{
    Formatter, Style, TemplateAtom, TemplateContent, TemplateControlFlow, TemplateSegment,
    TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::projects::settings::BS_VAR_PREFIX;
use crate::{ast_log, return_compiler_error, return_syntax_error};

pub const TEMPLATE_SPECIAL_IGNORE_CHAR: char = '\u{FFFC}';

#[derive(Clone, Debug)]
pub struct Template {
    pub content: TemplateContent,
    pub kind: TemplateType,
    pub style: Style,
    #[allow(dead_code)]
    pub control_flow: TemplateControlFlow,
    pub id: String,
    pub location: TextLocation,
}

impl Template {
    pub fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerError> {
        // These are variables or special keywords passed into the template head
        let mut template = Self::create_default(templates_inherited);
        // Capture the opening token location early so style/directive errors can
        // still point at the template even if parsing later advances deeply.
        template.location = token_stream.current_location();

        // Templates that call any functions or have children that call functions
        // Can't be folded at compile time (EVENTUALLY CAN FOLD THE CONST FUNCTIONS TOO).
        // This is because the template might be changing at runtime.
        // If the entire template can be folded, it just becomes a string after the AST stage.
        let mut foldable = true;
        let mut active_wrapper: Option<Template> = None;

        parse_template_head(
            token_stream,
            context,
            &mut template,
            &mut active_wrapper,
            &mut foldable,
            string_table,
        )?;

        // ---------------------
        // TEMPLATE BODY PARSING
        // ---------------------
        // The tokenizer only allows for strings or templates inside the template body
        while token_stream.index < token_stream.tokens.len() {
            match &token_stream.current_token_kind() {
                TokenKind::Eof => {
                    break;
                }

                TokenKind::TemplateClose => {
                    ast_log!("Breaking out of template body. Found a template close.");
                    // Need to skip the closer
                    token_stream.advance();
                    break;
                }

                TokenKind::TemplateHead => {
                    let nested_template = Self::new(
                        token_stream,
                        context,
                        template.style.child_templates.to_owned(),
                        string_table,
                    )?;

                    match nested_template.kind {
                        TemplateType::String if !nested_template.has_unresolved_slots() => {
                            ast_log!(
                                "Found a compile time foldable template inside a template. Folding into a string slice..."
                            );

                            // Just uses the last inherited template atm
                            // TODO: should this take the highest precedence template?
                            let inherited_style = template
                                .style
                                .child_templates
                                .last()
                                .map(|template| template.style.to_owned());

                            let interned_child = nested_template
                                .fold_into_stringid(&inherited_style, string_table)?;

                            template.content.add(
                                Expression::string_slice(
                                    interned_child,
                                    token_stream.current_location(),
                                    Ownership::ImmutableOwned,
                                ),
                            );

                            continue;
                        }

                        TemplateType::StringFunction => {
                            foldable = false;
                        }

                        TemplateType::Comment => {
                            continue;
                        }

                        TemplateType::String | TemplateType::SlotInsertion(_) => {}
                    }

                    let expr = Expression::template(nested_template, Ownership::ImmutableOwned);
                    template.content.add(expr);
                    continue;
                }

                TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                    template.content.add(
                        Expression::string_slice(
                            *content,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ),
                    );
                }

                TokenKind::Newline => {
                    let newline_id = string_table.intern("\n");
                    template.content.add(
                        Expression::string_slice(
                            newline_id,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ),
                    );
                }

                TokenKind::TemplateSlotMarker => {
                    template.content.push_slot();
                }

                TokenKind::Empty => {}

                _ => {
                    return_syntax_error!(
                        format!(
                            "Invalid Token Used Inside template body when creating template node. Token: {:?}",
                            token_stream.current_token_kind()
                        ),
                        token_stream
                            .current_location()
                            .to_error_location(&string_table)
                    )
                }
            }

            token_stream.advance();
        }

        // Formatting is normalized here, before any later folding/lowering stage.
        // This keeps runtime templates simple: only compile-time-known body strings
        // are rewritten, while dynamic chunks remain untouched and keep their order.
        apply_body_formatter(
            &mut template.content,
            &template.style.formatter,
            string_table,
        );

        if let Some(wrapper) = active_wrapper {
            template.content = compose_template_with_slots(
                &wrapper,
                &template.content,
                &template.location,
            )?;
            foldable &= matches!(wrapper.kind, TemplateType::String);
        } else {
            ensure_no_slot_insertions_remain(&template.content, &template.location)?;
        }

        if foldable && !matches!(template.kind, TemplateType::SlotInsertion(_)) {
            template.kind = TemplateType::String;
            return Ok(template);
        };

        Ok(template)
    }

    pub fn create_default(templates: Vec<Template>) -> Template {
        let style = match templates.last() {
            Some(t) => t.style.clone(),
            None => Style::default(),
        };

        Template {
            content: TemplateContent::default(),
            kind: TemplateType::StringFunction,
            style,
            control_flow: TemplateControlFlow::None,
            id: String::new(),
            location: TextLocation::default(),
        }
    }

    pub fn has_unresolved_slots(&self) -> bool {
        self.content.has_unresolved_slots()
    }

    pub fn insert_template_into_head(
        &mut self,
        template_being_inserted: &Template,
        foldable: &mut bool,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        match template_being_inserted.kind {
            TemplateType::StringFunction => {
                // Keep going, but this template can't be folded at compile time now
                *foldable = false
            }
            TemplateType::Comment => {
                // Ignore this scene completely, don't insert anything
                return Ok(());
            }
            TemplateType::SlotInsertion(_) => {
                // Slot insertion templates only make sense while filling an active wrapper.
                return_syntax_error!(
                    "Can't use labeled slot insertions in the template head.",
                    self.location.to_owned().to_error_location(&string_table)
                )
            }
            TemplateType::String => {
                // All good, keep going
            }
        }

        // Override the current child_default if there is a new one coming in
        if !template_being_inserted.style.child_templates.is_empty() {
            self.style.child_templates = template_being_inserted.style.child_templates.to_owned();
        }

        // A template inserted from the head now behaves like head content in the
        // receiving template. That prevents a later body formatter from treating the
        // inserted compile-time strings as if they were local body literals.
        self.content.extend_retagged(
            template_being_inserted.content.to_owned(),
            TemplateSegmentOrigin::Head,
        );

        Ok(())
    }

    pub fn fold_into_stringid(
        &self,
        inherited_style: &Option<Style>,
        string_table: &mut StringTable,
    ) -> Result<StringId, CompilerError> {
        fold_atoms(&self.content.atoms, inherited_style, &self.style, string_table)
    }
}

// TODO: move these old formatters to the new trait style ones

// StyleFormat::Markdown => {
// let default_tag = "p";
// final_string.push_str(&to_markdown(&content, default_tag));
//
// StyleFormat::Codeblock => {
// final_string.push_str(&highlight_html_code_block(
// &content,
// BEANSTALK_FILE_EXTENSION,
// ));
//

// }

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
    active_wrapper: &mut Option<Template>,
    foldable: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // TODO: Add control flow parsing

    template.id = format!("{BS_VAR_PREFIX}templateID_{}", token_stream.index);

    // Each expression must be separated with a comma
    let mut comma_separator = true;
    let mut slot_target: Option<usize> = None;
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
            token_stream.advance();
            return Ok(());
        }

        if slot_target.is_some() {
            match token {
                TokenKind::Newline | TokenKind::Empty => {
                    token_stream.advance();
                    continue;
                }
                _ => {
                    return_syntax_error!(
                        "Labeled slot insertion heads can only contain the slot label before the optional body.",
                        token_stream
                            .current_location()
                            .to_error_location(&string_table)
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
                        .to_error_location(&string_table)
                )
            }

            comma_separator = true;
            token_stream.advance();
            continue;
        };

        let mut defer_separator_token = false;

        match token {
            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            TokenKind::Id(name) => {
                template.id = format!("{BS_VAR_PREFIX}{name}");
                saw_meaningful_head_item = true;
            }

            // If this is a template, we have to do some clever parsing here
            TokenKind::Symbol(name) => {
                // Check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = context.get_reference(&name) {
                    match &arg.value.kind {
                        // Reference to another string template
                        ExpressionKind::Template(inserted_template) => {
                            if context.kind.is_constant_context()
                                && matches!(inserted_template.kind, TemplateType::StringFunction)
                            {
                                return_syntax_error!(
                                    format!(
                                        "Const templates can only reference compile-time templates. '{}' is runtime.",
                                        name
                                    ),
                                    token_stream
                                        .current_location()
                                        .to_error_location(&string_table)
                                );
                            }

                            if inserted_template.content.slot_count() > 0 {
                                if active_wrapper.is_some() {
                                    return_syntax_error!(
                                        "Only one wrapper template can be applied from a template head.",
                                        token_stream
                                            .current_location()
                                            .to_error_location(&string_table)
                                    );
                                }

                                if !inserted_template.style.child_templates.is_empty() {
                                    template.style.child_templates =
                                        inserted_template.style.child_templates.to_owned();
                                }

                                if matches!(inserted_template.kind, TemplateType::StringFunction) {
                                    *foldable = false;
                                }

                                // Wrapper selection changes how the template body is
                                // interpreted later, so keep it separate from ordinary
                                // head content instead of splicing it in immediately.
                                *active_wrapper = Some(inserted_template.as_ref().to_owned());
                            } else {
                                template.insert_template_into_head(
                                    inserted_template,
                                    foldable,
                                    string_table,
                                )?;
                            }

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

                            if context.kind.is_constant_context()
                                && !expr.is_compile_time_constant()
                            {
                                return_syntax_error!(
                                    format!(
                                        "Const templates can only capture constants. '{}' resolves to a non-constant value.",
                                        name
                                    ),
                                    token_stream
                                        .current_location()
                                        .to_error_location(&string_table)
                                );
                            }

                            // Any non-constant expression can't be folded
                            if !expr.kind.is_foldable() && !expr.is_compile_time_constant() {
                                ast_log!("Template is no longer foldable due to reference");
                                *foldable = false;
                            }

                            template.content.add_with_origin(
                                expr,
                                TemplateSegmentOrigin::Head,
                            );
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
                            .to_error_location(&string_table)
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
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::CoerceToString,
                    &Ownership::ImmutableOwned,
                    false,
                    string_table,
                )?;

                template
                    .content
                    .add_with_origin(expr, TemplateSegmentOrigin::Head);
                defer_separator_token = true;
                saw_meaningful_head_item = true;
            }

            TokenKind::Path(paths) => {
                if paths.is_empty() {
                    return_syntax_error!(
                        "Path token in template head cannot be empty.",
                        token_stream
                            .current_location()
                            .to_error_location(&string_table)
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
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::CoerceToString,
                    &Ownership::ImmutableOwned,
                    true,
                    string_table,
                )?;

                if context.kind.is_constant_context() && !expr.is_compile_time_constant() {
                    return_syntax_error!(
                        "Const templates require compile-time expressions in the template head.",
                        token_stream
                            .current_location()
                            .to_error_location(&string_table)
                    );
                }

                if !expr.kind.is_foldable() && !expr.is_compile_time_constant() {
                    ast_log!("Template is no longer foldable");
                    *foldable = false;
                }

                template
                    .content
                    .add_with_origin(expr, TemplateSegmentOrigin::Head);
                defer_separator_token = true;
                saw_meaningful_head_item = true;
            }

            TokenKind::StyleDirective(_) | TokenKind::StyleTemplateHead => {
                // Style directives live in the same comma-separated list as ordinary
                // head expressions, so we parse them inline and then resume the same
                // separator rules as the rest of the head.
                defer_separator_token =
                    parse_style_directive(token_stream, context, template, string_table)?;
                saw_meaningful_head_item = true;
            }

            TokenKind::Comma => {
                // Multiple commas in succession
                return_syntax_error!(
                    "Multiple commas used back to back in the template head. You must have a valid expression between each comma",
                    token_stream
                        .current_location()
                        .to_error_location(&string_table)
                )
            }

            TokenKind::SlotTarget(slot) => {
                if saw_meaningful_head_item {
                    return_syntax_error!(
                        "Labeled slot insertion heads can only contain the slot label before the optional body.",
                        token_stream
                            .current_location()
                            .to_error_location(&string_table)
                    );
                }

                template.kind = TemplateType::SlotInsertion(slot);
                slot_target = Some(slot);
                saw_meaningful_head_item = true;
            }

            // Newlines / empty things in the scene head are ignored
            TokenKind::Newline | TokenKind::Empty => {
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
                        .to_error_location(&string_table)
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

fn fold_atoms(
    atoms: &[TemplateAtom],
    inherited_style: &Option<Style>,
    style: &Style,
    string_table: &mut StringTable,
) -> Result<StringId, CompilerError> {
    // Now we start combining everything into one string
    let mut final_string = String::with_capacity(3);
    let mut inside_protected_body_run = false;

    // Body strings may already have been formatted by this template. If an inherited
    // formatter would otherwise run over the same bytes again, wrap only those body
    // runs in the invisible guard marker so the parent formatter skips them.
    let should_protect_formatted_body = inherited_style.as_ref().is_some_and(|inherited_style| {
        style.formatter.is_some()
            && inherited_style.formatter_precedence <= style.formatter_precedence
    });

    // template content
    for atom in atoms {
        let TemplateAtom::Content(segment) = atom else {
            return_syntax_error!(
                "Template still contains unresolved slots and cannot be folded into a string.",
                TextLocation::default().to_error_location(string_table)
            );
        };

        let protects_this_segment = should_protect_formatted_body
            && segment.origin == TemplateSegmentOrigin::Body
            && matches!(segment.expression.kind, ExpressionKind::StringSlice(_));

        if protects_this_segment && !inside_protected_body_run {
            final_string.push(TEMPLATE_SPECIAL_IGNORE_CHAR);
            inside_protected_body_run = true;
        } else if !protects_this_segment && inside_protected_body_run {
            final_string.push(TEMPLATE_SPECIAL_IGNORE_CHAR);
            inside_protected_body_run = false;
        }

        match &segment.expression.kind {
            ExpressionKind::StringSlice(string) => {
                final_string.push_str(string_table.resolve(*string));
            }

            ExpressionKind::Float(float) => {
                final_string.push_str(&float.to_string());
            }

            ExpressionKind::Int(int) => {
                final_string.push_str(&int.to_string());
            }

            // Add the string representation of the bool
            ExpressionKind::Bool(value) => {
                final_string.push_str(&value.to_string());
            }

            ExpressionKind::Char(value) => {
                final_string.push(*value);
            }

            // Anything else can't be folded and should not get to this stage.
            // This is a compiler_frontend error
            _ => {
                return_compiler_error!(
                    "Invalid Expression Used Inside template when trying to fold into a string.\
                         The compiler_frontend should not be trying to fold this template."
                )
            }
        }
    }

    if inside_protected_body_run {
        final_string.push(TEMPLATE_SPECIAL_IGNORE_CHAR);
    }

    ast_log!("Folded template into: ", final_string);

    Ok(string_table.intern(&final_string))
}

fn parse_style_directive(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    template: &mut Template,
    string_table: &mut StringTable,
) -> Result<bool, CompilerError> {
    match token_stream.current_token_kind().clone() {
        TokenKind::StyleDirective(directive) => match string_table.resolve(directive) {
            "markdown" => {
                // Built-in formatter sugar. Full `$formatter(...)` support comes later,
                // but this gives the AST a concrete style object to carry right now.
                template.style.id = "markdown";
                template.style.formatter = Some(markdown_formatter());
                template.style.formatter_precedence = 0;
                Ok(false)
            }

            "code" => {
                // Keep the directive-specific parsing in the code formatter module so
                // this general template parser does not accumulate every built-in style.
                configure_code_style(token_stream, template, string_table)?;
                Ok(false)
            }

            "ignore" => {
                // `$ignore` wipes the inherited style state first, then later directives
                // in the same head can layer fresh settings back on top.
                template.style = Style::default();
                Ok(false)
            }

            "formatter" => {
                return_syntax_error!(
                    "The '$formatter(...)' template style directive is not implemented yet.",
                    token_stream
                        .current_location()
                        .to_error_location(string_table),
                    {
                        PrimarySuggestion => "Use '$markdown' or '$code' for now, or remove '$formatter(...)' until formatter callbacks are implemented",
                    }
                )
            }

            other => {
                return_syntax_error!(
                    format!(
                        "Unsupported style directive '${other}'. Supported directives are '$markdown', '$code', '$ignore', and '$['."
                    ),
                    token_stream
                        .current_location()
                        .to_error_location(string_table),
                    {
                        PrimarySuggestion => "Use '$markdown', '$code', '$ignore', or '$[' inside the template head",
                    }
                )
            }
        },

        TokenKind::StyleTemplateHead => {
            // `$[` defines a default child template wrapper. Parse it using the same
            // template parser as any other nested template, then store the result on
            // the style instead of inserting it into this template's content.
            let child_template = Template::new(
                token_stream,
                context,
                template.style.child_templates.to_owned(),
                string_table,
            )?;
            template.style.child_templates.push(child_template);
            Ok(true)
        }

        _ => {
            return_compiler_error!("Tried to parse a style directive while not positioned at one.")
        }
    }
}

fn apply_body_formatter(
    content: &mut TemplateContent,
    formatter: &Option<Formatter>,
    string_table: &mut StringTable,
) {
    let Some(formatter) = formatter else {
        return;
    };

    // The formatter only runs over compile-time body strings. Head segments and
    // dynamic expressions are preserved as-is and act as hard boundaries.
    format_content_atoms(&mut content.atoms, formatter, string_table);
}

fn format_content_atoms(
    atoms: &mut Vec<TemplateAtom>,
    formatter: &Formatter,
    string_table: &mut StringTable,
) {
    let mut formatted_atoms = Vec::with_capacity(atoms.len());
    let mut buffered_text = String::new();
    let mut buffer_location: Option<TextLocation> = None;

    // Coalesce adjacent body string slices so the formatter sees the same text a
    // user wrote contiguously in the source, rather than one token at a time.
    for atom in std::mem::take(atoms) {
        let TemplateAtom::Content(segment) = atom else {
            flush_formatted_body_run(
                &mut formatted_atoms,
                &mut buffered_text,
                &mut buffer_location,
                formatter,
                string_table,
            );
            formatted_atoms.push(TemplateAtom::Slot);
            continue;
        };

        let ExpressionKind::StringSlice(text) = &segment.expression.kind else {
            flush_formatted_body_run(
                &mut formatted_atoms,
                &mut buffered_text,
                &mut buffer_location,
                formatter,
                string_table,
            );
            formatted_atoms.push(TemplateAtom::Content(segment));
            continue;
        };

        if segment.origin != TemplateSegmentOrigin::Body {
            flush_formatted_body_run(
                &mut formatted_atoms,
                &mut buffered_text,
                &mut buffer_location,
                formatter,
                string_table,
            );
            formatted_atoms.push(TemplateAtom::Content(segment));
            continue;
        }

        if buffer_location.is_none() {
            buffer_location = Some(segment.expression.location.clone());
        }

        buffered_text.push_str(string_table.resolve(*text));
    }

    flush_formatted_body_run(
        &mut formatted_atoms,
        &mut buffered_text,
        &mut buffer_location,
        formatter,
        string_table,
    );

    *atoms = formatted_atoms;
}

fn flush_formatted_body_run(
    atoms: &mut Vec<TemplateAtom>,
    buffered_text: &mut String,
    buffer_location: &mut Option<TextLocation>,
    formatter: &Formatter,
    string_table: &mut StringTable,
) {
    if buffered_text.is_empty() {
        return;
    }

    // Format once per contiguous body run, then collapse it back into a single
    // string-slice segment so later stages do not need any special formatter logic.
    formatter.formatter.format(buffered_text);

    let interned = string_table.intern(buffered_text.as_str());
    let location = buffer_location.take().unwrap_or_default();
    atoms.push(TemplateAtom::Content(TemplateSegment::new(
        Expression::string_slice(interned, location, Ownership::ImmutableOwned),
        TemplateSegmentOrigin::Body,
    )));

    buffered_text.clear();
}

#[cfg(test)]
#[path = "tests/create_template_node_tests.rs"]
mod create_template_node_tests;
