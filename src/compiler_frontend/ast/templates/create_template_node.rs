use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::slots::{
    compose_template_with_slots, ensure_no_slot_insertions_remain,
    parse_optional_slot_name_argument, parse_required_slot_name_argument,
};
use crate::compiler_frontend::ast::templates::styles::TEMPLATE_FORMAT_GUARD_CHAR;
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
use crate::compiler_frontend::ast::templates::styles::whitespace::{
    TemplateBodyRunPosition, TemplateWhitespacePassProfile, apply_whitespace_passes,
};
use crate::compiler_frontend::ast::templates::template::{
    BodyWhitespacePolicy, CommentDirectiveKind, Formatter, SlotKey, Style, TemplateAtom,
    TemplateConstValueKind, TemplateContent, TemplateControlFlow, TemplateSegment,
    TemplateSegmentOrigin, TemplateType,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::style_directives::StyleDirectiveSource;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::projects::settings::BS_VAR_PREFIX;
use crate::{ast_log, return_compiler_error, return_syntax_error};

#[derive(Clone, Debug)]
pub struct Template {
    pub content: TemplateContent,
    pub unformatted_content: TemplateContent,
    pub content_needs_formatting: bool,
    pub kind: TemplateType,
    pub doc_children: Vec<Template>,
    pub style: Style,
    pub explicit_style: Style,
    #[allow(dead_code)] // todo
    pub control_flow: TemplateControlFlow,
    pub id: String,
    pub location: TextLocation,
}

#[derive(Clone, Debug, Default)]
struct TemplateInheritance {
    recursive_style: Option<Style>,
    direct_child_wrappers: Vec<Template>,
}

impl TemplateInheritance {
    fn from_legacy_templates(templates: Vec<Template>) -> Self {
        let recursive_style = templates
            .last()
            .and_then(|template| recursive_inherited_style(&template.style));

        Self {
            recursive_style,
            direct_child_wrappers: templates,
        }
    }
}

impl Template {
    pub fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerError> {
        let inheritance = TemplateInheritance::from_legacy_templates(templates_inherited);
        Self::new_with_doc_context(token_stream, context, inheritance, string_table, false)
    }

    fn new_with_doc_context(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        inheritance: TemplateInheritance,
        string_table: &mut StringTable,
        doc_context: bool,
    ) -> Result<Template, CompilerError> {
        let direct_child_wrappers = inheritance.direct_child_wrappers.to_owned();
        // These are variables or special keywords passed into the template head
        let mut template = Self::create_default_with_inherited_style(inheritance.recursive_style);
        // Capture the opening token location early so style/directive errors can
        // still point at the template even if parsing later advances deeply.
        template.location = token_stream.current_location();

        // Templates that call any functions or have children that call functions
        // Can't be folded at compile time (EVENTUALLY CAN FOLD THE CONST FUNCTIONS TOO).
        // This is because the template might be changing at runtime.
        // If the entire template can be folded, it just becomes a string after the AST stage.
        let mut foldable = true;

        parse_template_head(
            token_stream,
            context,
            &mut template,
            &mut foldable,
            string_table,
        )?;

        if doc_context {
            apply_doc_comment_defaults(&mut template);
        }

        // ---------------------
        // TEMPLATE BODY PARSING
        // ---------------------
        // The tokenizer only allows for strings, templates or slots inside the template body
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
                    let nested_inheritance = TemplateInheritance {
                        recursive_style: recursive_inherited_style(&template.style),
                        direct_child_wrappers: template.style.child_templates.to_owned(),
                    };
                    let nested_template = Self::new_with_doc_context(
                        token_stream,
                        context,
                        nested_inheritance,
                        string_table,
                        matches!(
                            template.kind,
                            TemplateType::Comment(CommentDirectiveKind::Doc)
                        ),
                    )?;

                    if matches!(
                        template.kind,
                        TemplateType::Comment(CommentDirectiveKind::Doc)
                    ) {
                        template.doc_children.push(nested_template);
                        continue;
                    }

                    match &nested_template.kind {
                        TemplateType::String if !nested_template.has_unresolved_slots() => {
                            ast_log!(
                                "Found a compile time foldable template inside a template. Folding into a string slice..."
                            );

                            // Preserve formatter boundaries when folding nested compile-time
                            // templates into this template's body stream.
                            let inherited_style =
                                effective_inherited_style_for_nested_templates(&template.style);

                            let interned_child = nested_template
                                .fold_into_stringid(&inherited_style, string_table)?;

                            template.content.atoms.push(TemplateAtom::Content(
                                TemplateSegment::from_child_template_output(
                                    Expression::string_slice(
                                        interned_child,
                                        token_stream.current_location(),
                                        Ownership::ImmutableOwned,
                                    ),
                                    TemplateSegmentOrigin::Body,
                                    nested_template.clone(),
                                ),
                            ));

                            continue;
                        }

                        TemplateType::StringFunction => {
                            foldable = false;
                        }

                        TemplateType::Comment(_) => {
                            continue;
                        }

                        TemplateType::String | TemplateType::SlotInsert(_) => {}
                        TemplateType::SlotDefinition(slot_key) => {
                            template.content.push_slot_with_child_wrappers(
                                slot_key.to_owned(),
                                direct_child_wrappers.to_owned(),
                                template.style.child_templates.to_owned(),
                                template.style.clear_inherited,
                            );
                            continue;
                        }
                    }

                    let expr = Expression::template(nested_template, Ownership::ImmutableOwned);
                    template.content.add(expr);
                    continue;
                }

                TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                    template.content.add(Expression::string_slice(
                        *content,
                        token_stream.current_location(),
                        Ownership::ImmutableOwned,
                    ));
                }

                TokenKind::Newline => {
                    let newline_id = string_table.intern("\n");
                    template.content.add(Expression::string_slice(
                        newline_id,
                        token_stream.current_location(),
                        Ownership::ImmutableOwned,
                    ));
                }

                _ => {
                    return_syntax_error!(
                        format!(
                            "Invalid Token Used Inside template body when creating template node. Token: {:?}",
                            token_stream.current_token_kind()
                        ),
                        token_stream
                            .current_location()
                            .to_error_location(string_table)
                    )
                }
            }

            token_stream.advance();
        }

        template.unformatted_content = apply_inherited_child_templates_to_content(
            template.content.to_owned(),
            &template.style.child_templates,
            string_table,
        )?;
        template.unformatted_content = compose_template_head_chain(
            &template.unformatted_content,
            &mut foldable,
            string_table,
        )?;

        // Formatting is normalized here, before any later folding/lowering stage.
        // This keeps runtime templates simple: only compile-time-known body strings
        // are rewritten, while dynamic chunks remain untouched and keep their order.
        apply_body_formatter(&mut template.content, &template.style, string_table);

        template.content = apply_inherited_child_templates_to_content(
            template.content,
            &template.style.child_templates,
            string_table,
        )?;

        template.content =
            compose_template_head_chain(&template.content, &mut foldable, string_table)?;
        template.content_needs_formatting = false;

        if matches!(
            template.kind,
            TemplateType::Comment(CommentDirectiveKind::Doc)
        ) && !template.content.is_const_evaluable_value()
        {
            return_syntax_error!(
                "'$doc' comments can only contain compile-time values.",
                template.location.to_error_location(string_table),
                {
                    PrimarySuggestion => "Use constants and foldable template/string values inside '$doc' comments",
                }
            );
        }

        // `$insert(...)` helpers are allowed to survive while a template still has
        // unresolved `$slot` markers, because that template may later compose into
        // an immediate parent and contribute upward. Once a template has no slots
        // left, any remaining `$insert(...)` is out of scope and must error.
        if !matches!(template.kind, TemplateType::SlotInsert(_)) && !template.has_unresolved_slots()
        {
            ensure_no_slot_insertions_remain(&template.content, &template.location, string_table)?;
        }

        if foldable
            && !matches!(
                template.kind,
                TemplateType::SlotInsert(_)
                    | TemplateType::SlotDefinition(_)
                    | TemplateType::Comment(_)
            )
        {
            template.kind = TemplateType::String;
        }

        emit_css_template_warnings(&template, context, string_table);
        emit_html_template_warnings(&template, context, string_table);

        Ok(template)
    }

    pub fn create_default(templates: Vec<Template>) -> Template {
        let inheritance = TemplateInheritance::from_legacy_templates(templates);
        Self::create_default_with_inherited_style(inheritance.recursive_style)
    }

    fn create_default_with_inherited_style(inherited_style: Option<Style>) -> Template {
        let mut style = inherited_style.unwrap_or_else(Style::default);
        style.child_templates.clear();

        Template {
            content: TemplateContent::default(),
            unformatted_content: TemplateContent::default(),
            content_needs_formatting: false,
            kind: TemplateType::StringFunction,
            doc_children: vec![],
            style,
            explicit_style: Style::default(),
            control_flow: TemplateControlFlow::None,
            id: String::new(),
            location: TextLocation::default(),
        }
    }

    pub(crate) fn apply_style(&mut self, style: Style) {
        self.style = style.to_owned();
        self.explicit_style = style;
    }

    pub(crate) fn apply_style_updates(&mut self, mut update: impl FnMut(&mut Style)) {
        update(&mut self.style);
        update(&mut self.explicit_style);
    }

    pub fn has_unresolved_slots(&self) -> bool {
        self.content.has_unresolved_slots()
    }

    pub fn is_const_evaluable_value(&self) -> bool {
        self.const_value_kind().is_compile_time_value()
    }

    pub fn is_const_renderable_string(&self) -> bool {
        self.const_value_kind().is_renderable_string()
    }

    pub fn const_value_kind(&self) -> TemplateConstValueKind {
        // WHAT: classify template const-ness in one place.
        // WHY: AST constant checks and render-required paths need consistent rules.
        if !self.content.is_const_evaluable_value() {
            return TemplateConstValueKind::NonConst;
        }

        if matches!(self.kind, TemplateType::SlotInsert(_)) {
            // Slot insertion templates are compile-time helper values and are only
            // valid when consumed by an active wrapper fill site.
            if self.content.contains_slot_insertions() {
                return TemplateConstValueKind::NonConst;
            }
            return TemplateConstValueKind::SlotInsertHelper;
        }

        if matches!(self.kind, TemplateType::SlotDefinition(_)) {
            return TemplateConstValueKind::NonConst;
        }

        if !matches!(self.kind, TemplateType::String) {
            return TemplateConstValueKind::NonConst;
        }

        if self.has_unresolved_slots() {
            return TemplateConstValueKind::WrapperTemplate;
        }

        if self.content.contains_slot_insertions() {
            return TemplateConstValueKind::NonConst;
        }

        TemplateConstValueKind::RenderableString
    }

    pub fn fold_into_stringid(
        &self,
        inherited_style: &Option<Style>,
        string_table: &mut StringTable,
    ) -> Result<StringId, CompilerError> {
        let content = if self.content_needs_formatting {
            let mut content = self.unformatted_content.to_owned();
            apply_body_formatter(&mut content, &self.style, string_table);
            content
        } else {
            self.content.to_owned()
        };

        fold_atoms(&content.atoms, inherited_style, &self.style, string_table)
    }
}

fn recursive_inherited_style(style: &Style) -> Option<Style> {
    let mut inherited = style.to_owned();
    inherited.child_templates.clear();

    if inherited.formatter.is_none()
        && inherited.css_mode.is_none()
        && inherited.formatter_precedence == -1
        && inherited.override_precedence == -1
        && inherited.id.is_empty()
        && !inherited.clear_inherited
        && inherited.body_whitespace_policy == BodyWhitespacePolicy::DefaultTemplateBehavior
        && !inherited.html_mode
    {
        return None;
    }

    Some(inherited)
}

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

                    let slot_name = parse_optional_slot_name_argument(token_stream, string_table)?;
                    template.kind = TemplateType::SlotDefinition(match slot_name {
                        Some(name) => SlotKey::named(name),
                        None => SlotKey::Default,
                    });
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

                    let slot_name = parse_required_slot_name_argument(token_stream, string_table)?;
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

pub(crate) fn apply_inherited_child_templates_to_content(
    content: TemplateContent,
    inherited_templates: &[Template],
    string_table: &StringTable,
) -> Result<TemplateContent, CompilerError> {
    if inherited_templates.is_empty() {
        return Ok(content);
    }

    let mut wrapped_atoms = Vec::with_capacity(content.atoms.len());

    for atom in content.atoms {
        if is_direct_child_template_atom(&atom) {
            wrapped_atoms.push(wrap_direct_child_atom(
                &atom,
                inherited_templates,
                string_table,
            )?);
        } else {
            wrapped_atoms.push(atom);
        }
    }

    Ok(TemplateContent {
        atoms: wrapped_atoms,
    })
}

fn is_direct_child_template_atom(atom: &TemplateAtom) -> bool {
    let TemplateAtom::Content(segment) = atom else {
        return false;
    };

    if segment.origin != TemplateSegmentOrigin::Body {
        return false;
    }

    if segment.is_child_template_output {
        return true;
    }

    match &segment.expression.kind {
        ExpressionKind::Template(template) => !template.has_unresolved_slots(),
        _ => false,
    }
}

fn wrap_direct_child_atom(
    atom: &TemplateAtom,
    inherited_templates: &[Template],
    string_table: &StringTable,
) -> Result<TemplateAtom, CompilerError> {
    let mut wrapped_atom = atom.to_owned();

    for wrapper in inherited_templates.iter().rev() {
        wrapped_atom = wrap_atom_in_child_template(&wrapped_atom, wrapper, string_table)?;
    }

    Ok(wrapped_atom)
}

fn wrap_atom_in_child_template(
    atom: &TemplateAtom,
    wrapper: &Template,
    string_table: &StringTable,
) -> Result<TemplateAtom, CompilerError> {
    let origin = match atom {
        TemplateAtom::Content(segment) => segment.origin,
        TemplateAtom::Slot(_) => TemplateSegmentOrigin::Body,
    };

    let wrapped_template = if wrapper.has_unresolved_slots() {
        let fill_content = TemplateContent {
            atoms: vec![atom.to_owned()],
        };
        let composed_content =
            compose_template_with_slots(wrapper, &fill_content, &wrapper.location, string_table)?;

        let mut wrapped_template = wrapper.to_owned();
        wrapped_template.content = composed_content;
        wrapped_template.unformatted_content = wrapped_template.content.to_owned();
        wrapped_template.content_needs_formatting = false;
        wrapped_template
    } else {
        let mut wrapped_template = Template::create_default(vec![]);
        wrapped_template.location = wrapper.location.to_owned();
        wrapped_template.content = TemplateContent {
            atoms: vec![
                TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(wrapper.to_owned(), Ownership::ImmutableOwned),
                    TemplateSegmentOrigin::Body,
                )),
                atom.to_owned(),
            ],
        };
        wrapped_template.unformatted_content = wrapped_template.content.to_owned();
        wrapped_template.kind = if wrapped_template.content.is_const_evaluable_value()
            && !wrapped_template.content.contains_slot_insertions()
        {
            TemplateType::String
        } else {
            TemplateType::StringFunction
        };
        wrapped_template
    };

    Ok(TemplateAtom::Content(TemplateSegment::new(
        Expression::template(wrapped_template, Ownership::ImmutableOwned),
        origin,
    )))
}

#[derive(Clone, Debug)]
enum PendingChainItem {
    Atom(TemplateAtom),
    LayerRef {
        layer_index: usize,
        origin: TemplateSegmentOrigin,
    },
}

#[derive(Clone, Debug)]
struct ChainLayer {
    wrapper: Template,
    fill_items: Vec<PendingChainItem>,
}

fn compose_template_head_chain(
    content: &TemplateContent,
    foldable: &mut bool,
    string_table: &StringTable,
) -> Result<TemplateContent, CompilerError> {
    let mut head_atoms = Vec::new();
    let mut body_atoms = Vec::new();

    // Keep head and body atoms separated so only head template arguments can open
    // new receiving layers. Body atoms still flow into the deepest active receiver.
    for atom in &content.atoms {
        match atom {
            TemplateAtom::Content(segment) if segment.origin == TemplateSegmentOrigin::Head => {
                head_atoms.push(atom.to_owned());
            }
            _ => body_atoms.push(atom.to_owned()),
        }
    }

    if head_atoms.is_empty() {
        return Ok(content.to_owned());
    }

    let mut root_items = Vec::new();
    let mut layers = Vec::new();
    let mut active_layer: Option<usize> = None;

    for atom in head_atoms {
        if let Some((receiver, origin)) = receiver_template_from_head_atom(&atom) {
            let layer_index = layers.len();

            push_chain_item(
                &mut root_items,
                &mut layers,
                active_layer,
                PendingChainItem::LayerRef {
                    layer_index,
                    origin,
                },
            );

            if matches!(receiver.kind, TemplateType::StringFunction) {
                *foldable = false;
            }

            layers.push(ChainLayer {
                wrapper: receiver.to_owned(),
                fill_items: Vec::new(),
            });
            active_layer = Some(layer_index);
            continue;
        }

        push_chain_item(
            &mut root_items,
            &mut layers,
            active_layer,
            PendingChainItem::Atom(atom),
        );
    }

    // Body atoms are appended after head parsing. If the head opened a receiving
    // chain, body atoms become contributions to the deepest active receiver.
    for atom in body_atoms {
        push_chain_item(
            &mut root_items,
            &mut layers,
            active_layer,
            PendingChainItem::Atom(atom),
        );
    }

    let mut cache = rustc_hash::FxHashMap::default();
    let atoms = resolve_pending_chain_items(&root_items, &layers, &mut cache, string_table)?;
    Ok(TemplateContent { atoms })
}

fn push_chain_item(
    root_items: &mut Vec<PendingChainItem>,
    layers: &mut [ChainLayer],
    active_layer: Option<usize>,
    item: PendingChainItem,
) {
    match active_layer {
        Some(layer_index) => layers[layer_index].fill_items.push(item),
        None => root_items.push(item),
    }
}

fn receiver_template_from_head_atom(
    atom: &TemplateAtom,
) -> Option<(&Template, TemplateSegmentOrigin)> {
    let TemplateAtom::Content(segment) = atom else {
        return None;
    };

    let ExpressionKind::Template(template) = &segment.expression.kind else {
        return None;
    };

    if !template.has_unresolved_slots() {
        return None;
    }

    if matches!(
        template.kind,
        TemplateType::SlotInsert(_) | TemplateType::SlotDefinition(_)
    ) {
        return None;
    }

    Some((template, segment.origin))
}

fn resolve_pending_chain_items(
    items: &[PendingChainItem],
    layers: &[ChainLayer],
    cache: &mut rustc_hash::FxHashMap<usize, Template>,
    string_table: &StringTable,
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let mut atoms = Vec::with_capacity(items.len());

    for item in items {
        match item {
            PendingChainItem::Atom(atom) => atoms.push(atom.to_owned()),
            PendingChainItem::LayerRef {
                layer_index,
                origin,
            } => {
                let resolved_layer =
                    resolve_chain_layer(*layer_index, layers, cache, string_table)?;
                atoms.push(TemplateAtom::Content(TemplateSegment::new(
                    Expression::template(resolved_layer, Ownership::ImmutableOwned),
                    *origin,
                )));
            }
        }
    }

    Ok(atoms)
}

fn resolve_chain_layer(
    layer_index: usize,
    layers: &[ChainLayer],
    cache: &mut rustc_hash::FxHashMap<usize, Template>,
    string_table: &StringTable,
) -> Result<Template, CompilerError> {
    if let Some(cached) = cache.get(&layer_index) {
        return Ok(cached.to_owned());
    }

    let layer = &layers[layer_index];
    if layer.fill_items.is_empty() {
        // Head-only wrapper references like `[format.table]` must stay as unresolved
        // wrapper templates so later use-sites can still fill their slots.
        cache.insert(layer_index, layer.wrapper.to_owned());
        return Ok(layer.wrapper.to_owned());
    }

    let resolved_fill_atoms =
        resolve_pending_chain_items(&layer.fill_items, layers, cache, string_table)?;
    let resolved_fill = TemplateContent {
        atoms: resolved_fill_atoms,
    };
    let composed_content = compose_template_with_slots(
        &layer.wrapper,
        &resolved_fill,
        &layer.wrapper.location,
        string_table,
    )?;

    let mut resolved_wrapper = layer.wrapper.to_owned();
    resolved_wrapper.content = composed_content;
    cache.insert(layer_index, resolved_wrapper.to_owned());

    Ok(resolved_wrapper)
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
    // formatter otherwise runs over the same bytes again, wrap only those body
    // runs in the invisible guard marker so the parent formatter skips them.
    let should_protect_formatted_body = inherited_style.as_ref().is_some_and(|inherited_style| {
        style.formatter.is_some()
            && inherited_style.formatter_precedence <= style.formatter_precedence
    });
    let protect_head_segments_from_markdown = inherited_style
        .as_ref()
        .is_some_and(|inherited_style| inherited_style.id == "markdown");

    // template content
    for atom in atoms {
        let TemplateAtom::Content(segment) = atom else {
            // When a slot-bearing template is rendered directly, unfilled slots are
            // intentionally ignored, so the surrounding authored content still renders.
            continue;
        };

        let protects_this_segment = should_protect_formatted_body
            && segment.origin == TemplateSegmentOrigin::Body
            && !segment.is_child_template_output
            && matches!(segment.expression.kind, ExpressionKind::StringSlice(_));
        let protect_this_head_segment =
            protect_head_segments_from_markdown && segment.origin == TemplateSegmentOrigin::Head;

        if protects_this_segment && !inside_protected_body_run {
            final_string.push(TEMPLATE_FORMAT_GUARD_CHAR);
            inside_protected_body_run = true;
        } else if !protects_this_segment && inside_protected_body_run {
            final_string.push(TEMPLATE_FORMAT_GUARD_CHAR);
            inside_protected_body_run = false;
        }

        match &segment.expression.kind {
            ExpressionKind::StringSlice(string) => {
                push_folded_segment_str(
                    &mut final_string,
                    string_table.resolve(*string),
                    protect_this_head_segment,
                );
            }

            ExpressionKind::Float(float) => {
                push_folded_segment_str(
                    &mut final_string,
                    &float.to_string(),
                    protect_this_head_segment,
                );
            }

            ExpressionKind::Int(int) => {
                push_folded_segment_str(
                    &mut final_string,
                    &int.to_string(),
                    protect_this_head_segment,
                );
            }

            // Add the string representation of the bool
            ExpressionKind::Bool(value) => {
                push_folded_segment_str(
                    &mut final_string,
                    &value.to_string(),
                    protect_this_head_segment,
                );
            }

            ExpressionKind::Char(value) => {
                if protect_this_head_segment {
                    final_string.push(TEMPLATE_FORMAT_GUARD_CHAR);
                }
                final_string.push(*value);
                if protect_this_head_segment {
                    final_string.push(TEMPLATE_FORMAT_GUARD_CHAR);
                }
            }

            ExpressionKind::Template(template) => {
                if matches!(template.kind, TemplateType::Comment(_)) {
                    continue;
                }

                if matches!(template.kind, TemplateType::SlotInsert(_))
                    || template.content.contains_slot_insertions()
                {
                    return_compiler_error!(
                        "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly."
                    );
                }

                // If nested templates become fully resolved only after wrapper composition,
                // fold them here so authored nesting order is preserved in the final string.
                // Unfilled nested slots intentionally fold to empty strings.
                let nested_inherited_style = effective_inherited_style_for_nested_templates(style);
                let folded_nested =
                    template.fold_into_stringid(&nested_inherited_style, string_table)?;
                push_folded_segment_str(
                    &mut final_string,
                    string_table.resolve(folded_nested),
                    protect_this_head_segment,
                );
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
        final_string.push(TEMPLATE_FORMAT_GUARD_CHAR);
    }

    ast_log!("Folded template into: ", final_string);

    Ok(string_table.intern(&final_string))
}

fn effective_inherited_style_for_nested_templates(style: &Style) -> Option<Style> {
    recursive_inherited_style(style)
}

fn push_folded_segment_str(output: &mut String, value: &str, protect_from_markdown: bool) {
    if protect_from_markdown {
        output.push(TEMPLATE_FORMAT_GUARD_CHAR);
    }
    output.push_str(value);
    if protect_from_markdown {
        output.push(TEMPLATE_FORMAT_GUARD_CHAR);
    }
}

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

fn mark_template_body_whitespace_style_controlled(template: &mut Template) {
    template.apply_style_updates(|style| {
        style.body_whitespace_policy = BodyWhitespacePolicy::StyleDirectiveControlled;
    });
}

fn apply_doc_comment_defaults(template: &mut Template) {
    template.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    template.apply_style(Style::default());
    // Doc comments are parsed as markdown content by default.
    apply_markdown_style(template);
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

fn emit_css_template_warnings(
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

fn emit_html_template_warnings(
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

fn parse_children_style_directive(
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

fn apply_body_formatter(
    content: &mut TemplateContent,
    style: &Style,
    string_table: &mut StringTable,
) {
    let formatter = style.formatter.as_ref();
    let implicit_default_whitespace_pass = (style.body_whitespace_policy
        == BodyWhitespacePolicy::DefaultTemplateBehavior
        && formatter.is_none())
    .then_some(TemplateWhitespacePassProfile::default_template_body());

    if implicit_default_whitespace_pass.is_none() && formatter.is_none() {
        return;
    }

    // Body processing always keeps head/dynamic atoms as hard boundaries. Plain
    // templates run the implicit default whitespace pass, while style directives
    // receive raw body text unless their formatter declares reusable passes.
    format_content_atoms(
        &mut content.atoms,
        formatter,
        implicit_default_whitespace_pass,
        string_table,
    );
}

fn format_content_atoms(
    atoms: &mut Vec<TemplateAtom>,
    formatter: Option<&Formatter>,
    implicit_default_whitespace_pass: Option<TemplateWhitespacePassProfile>,
    string_table: &mut StringTable,
) {
    let mut formatted_atoms = Vec::with_capacity(atoms.len());
    let mut buffered_text = String::new();
    let mut buffer_location: Option<TextLocation> = None;
    let mut has_emitted_body_runs = false;

    // Coalesce adjacent body string slices so the formatter sees the same text a
    // user wrote contiguously in the source, rather than one token at a time.
    for atom in std::mem::take(atoms) {
        let TemplateAtom::Content(segment) = atom else {
            flush_formatted_body_run(
                &mut formatted_atoms,
                &mut buffered_text,
                &mut buffer_location,
                formatter,
                implicit_default_whitespace_pass,
                &mut has_emitted_body_runs,
                false,
                string_table,
            );
            if let TemplateAtom::Slot(slot) = atom {
                formatted_atoms.push(TemplateAtom::Slot(slot));
            }
            continue;
        };

        let ExpressionKind::StringSlice(text) = &segment.expression.kind else {
            flush_formatted_body_run(
                &mut formatted_atoms,
                &mut buffered_text,
                &mut buffer_location,
                formatter,
                implicit_default_whitespace_pass,
                &mut has_emitted_body_runs,
                false,
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
                implicit_default_whitespace_pass,
                &mut has_emitted_body_runs,
                false,
                string_table,
            );
            formatted_atoms.push(TemplateAtom::Content(segment));
            continue;
        }

        if segment.is_child_template_output {
            flush_formatted_body_run(
                &mut formatted_atoms,
                &mut buffered_text,
                &mut buffer_location,
                formatter,
                implicit_default_whitespace_pass,
                &mut has_emitted_body_runs,
                false,
                string_table,
            );
            if formatter.is_some() {
                formatted_atoms.push(TemplateAtom::Content(format_child_template_output_segment(
                    segment,
                    string_table,
                )));
            } else {
                formatted_atoms.push(TemplateAtom::Content(segment));
            }
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
        implicit_default_whitespace_pass,
        &mut has_emitted_body_runs,
        true,
        string_table,
    );

    *atoms = formatted_atoms;
}

fn format_child_template_output_segment(
    segment: TemplateSegment,
    string_table: &mut StringTable,
) -> TemplateSegment {
    if segment.origin != TemplateSegmentOrigin::Body {
        return segment;
    }

    let ExpressionKind::StringSlice(text) = &segment.expression.kind else {
        return segment;
    };

    let Some(source_child_template) = segment.source_child_template.as_ref() else {
        return segment;
    };

    let raw_text = string_table.resolve(*text);
    if !raw_text.contains(TEMPLATE_FORMAT_GUARD_CHAR) {
        return segment;
    }

    let formatted_text = raw_text.replace(TEMPLATE_FORMAT_GUARD_CHAR, "");

    let interned = string_table.intern(&formatted_text);
    let expression = Expression::string_slice(
        interned,
        segment.expression.location.clone(),
        Ownership::ImmutableOwned,
    );

    TemplateSegment::from_child_template_output(
        expression,
        TemplateSegmentOrigin::Body,
        source_child_template.as_ref().to_owned(),
    )
}

fn flush_formatted_body_run(
    atoms: &mut Vec<TemplateAtom>,
    buffered_text: &mut String,
    buffer_location: &mut Option<TextLocation>,
    formatter: Option<&Formatter>,
    implicit_default_whitespace_pass: Option<TemplateWhitespacePassProfile>,
    has_emitted_body_runs: &mut bool,
    is_final_flush: bool,
    string_table: &mut StringTable,
) {
    if buffered_text.is_empty() {
        return;
    }

    let run_position = match (*has_emitted_body_runs, is_final_flush) {
        (false, true) => TemplateBodyRunPosition::Only,
        (false, false) => TemplateBodyRunPosition::First,
        (true, true) => TemplateBodyRunPosition::Last,
        (true, false) => TemplateBodyRunPosition::Middle,
    };

    if let Some(default_pass) = implicit_default_whitespace_pass {
        apply_whitespace_passes(
            buffered_text,
            std::slice::from_ref(&default_pass),
            run_position,
        );
    }

    if let Some(formatter) = formatter {
        apply_whitespace_passes(
            buffered_text,
            &formatter.pre_format_whitespace_passes,
            run_position,
        );
        // Format once per contiguous body run, then collapse it back into a single
        // string-slice segment so later stages do not need any special formatter logic.
        formatter.formatter.format(buffered_text);
        apply_whitespace_passes(
            buffered_text,
            &formatter.post_format_whitespace_passes,
            run_position,
        );
    }

    if buffered_text.is_empty() {
        buffer_location.take();
        return;
    }

    let interned = string_table.intern(buffered_text.as_str());
    let location = buffer_location.take().unwrap_or_default();
    let expression = Expression::string_slice(interned, location, Ownership::ImmutableOwned);
    let segment = TemplateSegment::new(expression, TemplateSegmentOrigin::Body);
    atoms.push(TemplateAtom::Content(segment));
    *has_emitted_body_runs = true;

    buffered_text.clear();
}

#[cfg(test)]
#[path = "tests/create_template_node_tests.rs"]
mod create_template_node_tests;
