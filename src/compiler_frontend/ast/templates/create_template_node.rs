use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::templates::code::configure_code_style;
use crate::compiler_frontend::ast::templates::markdown::markdown_formatter;
use crate::compiler_frontend::ast::templates::slots::{
    compose_template_with_slots, ensure_no_slot_insertions_remain,
};
use crate::compiler_frontend::ast::templates::template::{
    CommentDirectiveKind, Formatter, SlotKey, Style, TemplateAtom, TemplateConstValueKind,
    TemplateContent, TemplateControlFlow, TemplateSegment, TemplateSegmentOrigin, TemplateType,
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
    pub doc_children: Vec<Template>,
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
        Self::new_with_doc_context(
            token_stream,
            context,
            templates_inherited,
            string_table,
            false,
        )
    }

    fn new_with_doc_context(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        templates_inherited: Vec<Template>,
        string_table: &mut StringTable,
        doc_context: bool,
    ) -> Result<Template, CompilerError> {
        let inherited_templates = templates_inherited.to_owned();
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
                    let nested_template = Self::new_with_doc_context(
                        token_stream,
                        context,
                        template.style.child_templates.to_owned(),
                        string_table,
                        matches!(
                            template.kind,
                            TemplateType::Comment(CommentDirectiveKind::Doc)
                        ),
                    )?;

                    if matches!(template.kind, TemplateType::Comment(CommentDirectiveKind::Doc)) {
                        template.doc_children.push(nested_template);
                        continue;
                    }

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

                            template.content.add(Expression::string_slice(
                                interned_child,
                                token_stream.current_location(),
                                Ownership::ImmutableOwned,
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
                            template.content.push_slot(slot_key);
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

        // Formatting is normalized here, before any later folding/lowering stage.
        // This keeps runtime templates simple: only compile-time-known body strings
        // are rewritten, while dynamic chunks remain untouched and keep their order.
        apply_body_formatter(
            &mut template.content,
            &template.style.formatter,
            string_table,
        );

        prepend_inherited_child_templates(&mut template.content, &inherited_templates);

        template.content = compose_template_head_chain(&template.content, &mut foldable)?;

        if matches!(template.kind, TemplateType::Comment(CommentDirectiveKind::Doc))
            && !template.content.is_const_evaluable_value()
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
            ensure_no_slot_insertions_remain(&template.content, &template.location)?;
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
            return Ok(template);
        };

        Ok(template)
    }

    pub fn create_default(templates: Vec<Template>) -> Template {
        let style = match templates.last() {
            Some(t) => {
                let mut inherited_style = t.style.clone();
                inherited_style.child_templates = templates;
                inherited_style
            }
            None => Style::default(),
        };

        Template {
            content: TemplateContent::default(),
            kind: TemplateType::StringFunction,
            doc_children: vec![],
            style,
            control_flow: TemplateControlFlow::None,
            id: String::new(),
            location: TextLocation::default(),
        }
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
        fold_atoms(
            &self.content.atoms,
            inherited_style,
            &self.style,
            string_table,
        )
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
    foldable: &mut bool,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // TODO: Add control flow parsing

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
                match directive_name {
                    "slot" => {
                        if saw_meaningful_head_item {
                            return_syntax_error!(
                                "Slot helper template heads can only contain '$slot' before the optional body.",
                                token_stream
                                    .current_location()
                                    .to_error_location(string_table)
                            );
                        }

                        let slot_name =
                            parse_optional_slot_name_argument(token_stream, string_table)?;
                        template.kind = TemplateType::SlotDefinition(match slot_name {
                            Some(name) => SlotKey::named(name),
                            None => SlotKey::Default,
                        });
                        saw_meaningful_head_item = true;
                    }
                    "insert" => {
                        if saw_meaningful_head_item {
                            return_syntax_error!(
                                "Slot helper template heads can only contain '$insert(\"name\")' before the optional body.",
                                token_stream
                                    .current_location()
                                    .to_error_location(string_table)
                            );
                        }

                        let slot_name =
                            parse_required_slot_name_argument(token_stream, string_table)?;
                        template.kind = TemplateType::SlotInsert(SlotKey::named(slot_name));
                        saw_meaningful_head_item = true;
                    }
                    _ => {
                        if saw_meaningful_head_item
                            && matches!(directive_name, "note" | "todo" | "doc")
                        {
                            return_syntax_error!(
                                "Comment template heads cannot mix '$note', '$todo', or '$doc' with other head expressions/directives.",
                                token_stream
                                    .current_location()
                                    .to_error_location(string_table)
                            );
                        }

                        defer_separator_token =
                            parse_style_directive(token_stream, context, template, string_table)?;
                        saw_meaningful_head_item = true;
                    }
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

fn parse_optional_slot_name_argument(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<Option<StringId>, CompilerError> {
    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Ok(None);
    }

    // Move from `StyleDirective("slot")`/`StyleDirective("insert")` to the
    // directive argument and leave the parser positioned at `)` on success.
    token_stream.advance();
    token_stream.advance();

    let slot_name = match token_stream.current_token_kind() {
        TokenKind::StringSliceLiteral(name) => *name,
        TokenKind::CloseParenthesis => {
            return_syntax_error!(
                "'$slot()' and '$insert()' cannot use empty parentheses. Use '$slot' for default or quoted names like '$slot(\"style\")'.",
                token_stream
                    .current_location()
                    .to_error_location(string_table)
            );
        }
        _ => {
            return_syntax_error!(
                "'$slot(...)' and '$insert(...)' only accept quoted string literal names.",
                token_stream
                    .current_location()
                    .to_error_location(string_table)
            );
        }
    };

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Expected ')' after template slot directive argument.",
            token_stream.current_location().to_error_location(string_table),
            {
                SuggestedInsertion => ")",
            }
        );
    }

    Ok(Some(slot_name))
}

fn parse_required_slot_name_argument(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
) -> Result<StringId, CompilerError> {
    let slot_name = parse_optional_slot_name_argument(token_stream, string_table)?;
    let Some(slot_name) = slot_name else {
        return_syntax_error!(
            "'$insert' requires a quoted named target like '$insert(\"style\")'.",
            token_stream
                .current_location()
                .to_error_location(string_table)
        );
    };

    Ok(slot_name)
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

fn prepend_inherited_child_templates(content: &mut TemplateContent, inherited_templates: &[Template]) {
    if inherited_templates.is_empty() {
        return;
    }

    let mut inherited_atoms = Vec::with_capacity(inherited_templates.len() + content.atoms.len());
    for inherited in inherited_templates {
        inherited_atoms.push(TemplateAtom::Content(TemplateSegment::new(
            Expression::template(inherited.to_owned(), Ownership::ImmutableOwned),
            TemplateSegmentOrigin::Head,
        )));
    }

    inherited_atoms.extend(std::mem::take(&mut content.atoms));
    content.atoms = inherited_atoms;
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
    let atoms = resolve_pending_chain_items(&root_items, &layers, &mut cache)?;
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
) -> Result<Vec<TemplateAtom>, CompilerError> {
    let mut atoms = Vec::with_capacity(items.len());

    for item in items {
        match item {
            PendingChainItem::Atom(atom) => atoms.push(atom.to_owned()),
            PendingChainItem::LayerRef {
                layer_index,
                origin,
            } => {
                let resolved_layer = resolve_chain_layer(*layer_index, layers, cache)?;
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
) -> Result<Template, CompilerError> {
    if let Some(cached) = cache.get(&layer_index) {
        return Ok(cached.to_owned());
    }

    let layer = &layers[layer_index];
    let resolved_fill_atoms = resolve_pending_chain_items(&layer.fill_items, layers, cache)?;
    let resolved_fill = TemplateContent {
        atoms: resolved_fill_atoms,
    };
    let composed_content =
        compose_template_with_slots(&layer.wrapper, &resolved_fill, &layer.wrapper.location)?;

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
    // formatter would otherwise run over the same bytes again, wrap only those body
    // runs in the invisible guard marker so the parent formatter skips them.
    let should_protect_formatted_body = inherited_style.as_ref().is_some_and(|inherited_style| {
        style.formatter.is_some()
            && inherited_style.formatter_precedence <= style.formatter_precedence
    });

    // template content
    for atom in atoms {
        let TemplateAtom::Content(segment) = atom else {
            // When a slot-bearing template is rendered directly, unfilled slots are
            // intentionally ignored so the surrounding authored content still renders.
            continue;
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

            ExpressionKind::Template(template) => {
                if matches!(template.kind, TemplateType::Comment(_)) {
                    continue;
                }

                if matches!(template.kind, TemplateType::SlotInsert(_))
                    || template.content.contains_slot_insertions()
                    || template.has_unresolved_slots()
                {
                    return_compiler_error!(
                        "Invalid template content reached string folding: unresolved slot insertions cannot be rendered directly."
                    );
                }

                // If nested templates become fully resolved only after wrapper composition,
                // fold them here so authored nesting order is preserved in the final string.
                let nested_inherited_style = style
                    .child_templates
                    .last()
                    .map(|template| template.style.to_owned());
                let folded_nested =
                    template.fold_into_stringid(&nested_inherited_style, string_table)?;
                final_string.push_str(string_table.resolve(folded_nested));
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

            "children" => {
                parse_children_style_directive(token_stream, context, template, string_table)?;
                Ok(false)
            }

            "ignore" => {
                // `$ignore` wipes the inherited style state first, then later directives
                // in the same head can layer fresh settings back on top.
                template.style = Style::default();
                Ok(false)
            }

            "note" => {
                reject_directive_arguments(token_stream, "note", string_table)?;
                template.kind = TemplateType::Comment(CommentDirectiveKind::Note);
                template.style = Style::default();
                Ok(false)
            }

            "todo" => {
                reject_directive_arguments(token_stream, "todo", string_table)?;
                template.kind = TemplateType::Comment(CommentDirectiveKind::Todo);
                template.style = Style::default();
                Ok(false)
            }

            "doc" => {
                reject_directive_arguments(token_stream, "doc", string_table)?;
                apply_doc_comment_defaults(template);
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
                        "Unsupported style directive '${other}'. Supported directives are '$markdown', '$children(..)', '$code', '$ignore', '$slot', '$insert(..)', '$note', '$todo', '$doc', and '$formatter(...)'."
                    ),
                    token_stream
                        .current_location()
                        .to_error_location(string_table),
                    {
                        PrimarySuggestion => "Use '$markdown', '$children(..)', '$code', '$ignore', '$slot', '$insert(..)', '$note', '$todo', '$doc', or '$formatter(...)' inside the template head",
                    }
                )
            }
        },

        _ => {
            return_compiler_error!("Tried to parse a style directive while not positioned at one.")
        }
    }
}

fn apply_doc_comment_defaults(template: &mut Template) {
    template.kind = TemplateType::Comment(CommentDirectiveKind::Doc);
    template.style = Style::default();
    // Doc comments are parsed as markdown content by default.
    template.style.id = "markdown";
    template.style.formatter = Some(markdown_formatter());
    template.style.formatter_precedence = 0;
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
            wrapper
        }

        _ => {
            return_syntax_error!(
                "The '$children(..)' directive only accepts template or string arguments.",
                argument_location.to_error_location(string_table)
            )
        }
    };

    template.style.child_templates.push(normalized);
    Ok(())
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
            if let TemplateAtom::Slot(slot_key) = atom {
                formatted_atoms.push(TemplateAtom::Slot(slot_key));
            }
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
