use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::statements::template::{
    Formatter, Style, TemplateContent, TemplateControlFlow, TemplateType,
};
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TextLocation, TokenKind};
use crate::compiler::string_interning::{InternedString, StringTable};
use crate::compiler::traits::ContainsReferences;
use crate::settings::BS_VAR_PREFIX;
use crate::{ast_log, return_compiler_error, return_syntax_error};
use colour::red_ln;

pub const TEMPLATE_SPECIAL_IGNORE_CHAR: char = '\u{FFFC}';

#[derive(Clone, Debug)]
pub struct Template {
    pub content: TemplateContent,
    pub kind: TemplateType,
    pub style: Style,
    pub control_flow: TemplateControlFlow,
    pub id: String,
    pub location: TextLocation,
}

impl Template {
    pub fn new(
        token_stream: &mut FileTokens,
        context: &ScopeContext,
        inherited_style: Option<Box<Style>>,
        string_table: &mut StringTable,
    ) -> Result<Template, CompilerError> {
        // These are variables or special keywords passed into the template head
        let mut template = Self::create_default(inherited_style);

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

        // TODO: Also parse (remove from the final output) ignored scenes
        // TokenKind::Ignore => {
        //     // Should also clear any styles or tags in the scene
        //     *template_style = Style::default();
        //
        //     // Keep track of how many scene opens there are
        //     // This is to make sure the scene close is at the correct place
        //     let mut extra_template_opens = 1;
        //     while token_stream.index < token_stream.length {
        //         match token_stream.current_token_kind() {
        //             TokenKind::TemplateClose => {
        //                 extra_template_opens -= 1;
        //                 if extra_template_opens == 0 {
        //                     token_stream.advance(); // Skip the closing scene close
        //                     break;
        //                 }
        //             }
        //             TokenKind::TemplateOpen => {
        //                 extra_template_opens += 1;
        //             }
        //             TokenKind::Eof => {
        //                 break;
        //             }
        //             _ => {}
        //         }
        //         token_stream.advance();
        //     }
        //
        //     return Ok(Template::comment(token_stream.current_location()));
        // }

        let mut is_after_slot = false;

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
                        template.style.child_default.to_owned(),
                        string_table,
                    )?;

                    match nested_template.kind {
                        TemplateType::String => {
                            ast_log!("Found a compile time template inside a template. Folding...");
                            let inherited_style = template
                                .style
                                .child_default
                                .as_ref()
                                .map(|style| *style.to_owned());

                            let interned_child =
                                nested_template.fold(&inherited_style, string_table)?;

                            template.content.add(
                                Expression::string_slice(
                                    interned_child,
                                    token_stream.current_location(),
                                    Ownership::ImmutableOwned,
                                ),
                                is_after_slot,
                            );

                            continue;
                        }

                        TemplateType::StringFunction => {
                            foldable = false;
                            // Insert it into the template
                            let expr =
                                Expression::template(nested_template, Ownership::ImmutableOwned);
                            template.content.add(expr, is_after_slot);
                        }

                        TemplateType::Slot => {
                            // Now body content goes after the slot.
                            // If this template is unpacked from a template head into another template,
                            // then its content can be placed before and after the template its being unpacked into.
                            is_after_slot = true;
                        }

                        // Ignore everything else for now
                        _ => {}
                    }
                }

                TokenKind::RawStringLiteral(content) | TokenKind::StringSliceLiteral(content) => {
                    template.content.add(
                        Expression::string_slice(
                            *content,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ),
                        is_after_slot,
                    );
                }

                // TODO: remove this, or just use label syntax in the future?
                // For templating values in scene heads in the body of scenes
                // TokenKind::EmptyTemplate(spaces) => {
                //
                //     // ADD SOMETHING HERE
                //
                //     for _ in 0..*spaces {
                //         template.content.add(Expression::string(
                //             " ".to_string(),
                //             token_stream.current_location(),
                //         ), after_slot);
                //     }
                // }
                TokenKind::Newline => {
                    let newline_id = string_table.intern("\n");
                    template.content.add(
                        Expression::string_slice(
                            newline_id,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ),
                        is_after_slot,
                    );
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
                            .to_error_location(&string_table),
                        {}
                    )
                }
            }

            token_stream.advance();
        }

        if foldable {
            template.kind = TemplateType::String;
            return Ok(template);
        };

        Ok(template)
    }

    pub fn create_default(style: Option<Box<Style>>) -> Template {
        let style = match style {
            Some(s) => *s,
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
            TemplateType::Slot => {
                // Error, can't use slots in the scene head (would be empty square brackets)
                return_syntax_error!(
                    "Can't use slots in the template head. Token",
                    self.location.to_owned().to_error_location(&string_table),
                    {}
                )
            }
            TemplateType::String => {
                // All good, keep going
            }
        }

        // Override the current child_default if there is a new one coming in
        if template_being_inserted.style.child_default.is_some() {
            self.style.child_default = template_being_inserted.style.child_default.to_owned();
        }

        if template_being_inserted.style.unlocks_override {
            self.style.unlocked_templates.clear();
        }

        // Insert this style's unlocked scenes into the unlocked scenes map
        for (name, style) in template_being_inserted.style.unlocked_templates.iter() {
            // Should this overwrite? Or skip if already unlocked?
            // Which is less efficient?
            self.style
                .unlocked_templates
                .insert(name.to_owned(), style.to_owned());
        }

        // Unpack this scene into this scene's body
        self.content
            .concat(template_being_inserted.content.to_owned());

        Ok(())
    }

    pub fn fold(
        &self,
        inherited_style: &Option<Style>,
        string_table: &mut StringTable,
    ) -> Result<InternedString, CompilerError> {
        // Now we start combining everything into one string
        let mut final_string = String::with_capacity(3);
        let mut formatter: Option<Formatter> = self.style.formatter.to_owned();

        // Format. How will the content be parsed at compile time?
        if let Some(style) = inherited_style {
            // Each format has a different precedence, using the highest precedence.
            // But children with a lower precedence than the parent should reset their format to None.
            // This is because the parent will already parse that formatting over all its children.
            if style.formatter_precedence > self.style.formatter_precedence {
                formatter = None;

            // If the child has a higher precedence format that the parent,
            // Then it inserts special characters around it that indicate to the parent that any formatting should be skipped here.
            // And this template will run its own format parsing.
            // This is only inserted when there is a parent style that will parse the content
            // because the formatters will remove this character when parsing.
            } else {
                final_string.push(TEMPLATE_SPECIAL_IGNORE_CHAR);
            }
        };

        // Compatibility
        // More restrictive compatibility takes precedence over less restrictive ones
        // match style.compatibility {
        //     TemplateCompatibility::None => {
        //         if final_style.compatibility != TemplateCompatibility::None {
        //             final_style.compatibility = TemplateCompatibility::None;
        //         }
        //     }
        //     // TODO: check compatibility of templates
        //     _ => {}
        // }

        // Inlining rule
        // TODO: what the hell is this?
        // Something to do with how surrounding templates are parsed with this one.
        // final_style.neighbour_rule = style.neighbour_rule.to_owned();

        // Everything inserted into the body
        // This needs to be done now
        // so Markdown will parse any added literals correctly

        // template content
        for value in self.content.flatten() {
            match &value.kind {
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

                // Anything else can't be folded and should not get to this stage.
                // This is a compiler error
                _ => {
                    return_compiler_error!(
                        "Invalid Expression Used Inside template when trying to fold into a string.\
                         The compiler should not be trying to fold this template."
                    )
                }
            }
        }

        // The style will be 'None' if the parent has the same style format
        // But if this child has a different format with a higher precedence,
        // then it will insert a special character that will be removed by the parent.
        // This character indicates to the parent that it should skip formatting this content.

        // Otherwise, we parse the content if there is a compile time formatter
        if let Some(formatter) = &formatter {
            formatter.formatter.format(&mut final_string);

            if inherited_style.is_some() {
                final_string.push(TEMPLATE_SPECIAL_IGNORE_CHAR);
            }
        }

        ast_log!("Folded template into: {:?}", final_string);

        Ok(string_table.intern(&final_string))
    }
}

// TOOD: move these old formatters to the new trait style ones

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
    token_stream.advance();

    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing template head token: {:?}", token);

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

        if token == TokenKind::EndTemplateHead {
            token_stream.advance();
            return Ok(());
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
                        .to_error_location(&string_table),
                    {}
                )
            }

            comma_separator = true;
            token_stream.advance();
            continue;
        };

        match token {
            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            TokenKind::Id(name) => {
                template.id = format!("{BS_VAR_PREFIX}{name}");
            }

            // If this is a template, we have to do some clever parsing here
            TokenKind::Symbol(name) => {
                // TODO - sort out the final design for inherited styles / templates
                // Should unlocked styles just be passed in as normal declarations?

                // Check if this is an unlocked scene (inherited from an ancestor)
                // This has to be done eagerly here as any previous scene or style passed into the scene head will add to this
                match template.style.unlocked_templates.to_owned().get(&name) {
                    Some(ExpressionKind::Template(inserted_template)) => {
                        template.insert_template_into_head(
                            inserted_template,
                            foldable,
                            string_table,
                        )?;
                    }

                    // Constant inherited
                    Some(ExpressionKind::StringSlice(string)) => {
                        template.content.before.push(Expression::string_slice(
                            *string,
                            token_stream.current_location(),
                            Ownership::ImmutableOwned,
                        ));
                    }

                    _ => {}
                }

                // Otherwise, check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = context.get_reference(&name) {
                    match &arg.value.kind {
                        // Reference to another string template
                        ExpressionKind::Template(inserted_template) => {
                            template.insert_template_into_head(
                                inserted_template,
                                foldable,
                                string_table,
                            )?;
                        }

                        // TODO: Special stuff for Types (structs)
                        // In the future, Types can implement a Style interface to do cool stuff
                        ExpressionKind::StructInstance(args) => {}

                        // Otherwise this is a reference to some other variable
                        // String, Number, Bool, etc. References
                        _ => {
                            // red_ln!("reference in template head ownership: {:#?}", arg.value.ownership);
                            let expr = create_expression(
                                token_stream,
                                context,
                                &mut DataType::CoerceToString,
                                &arg.value.ownership,
                                false,
                                string_table,
                            )?;

                            // Any non-constant expression can't be folded
                            if !expr.kind.is_foldable() {
                                ast_log!("Template is no longer foldable due to reference");
                                *foldable = false;
                            }

                            template.content.before.push(expr);
                        }
                    }
                } else {
                    return_syntax_error!(
                        format!("Cannot declare new variables inside of a template head. Variable '{}' is not declared.
                        \n Here are all the variables in scope: {:#?}", name, context.declarations),
                        token_stream.current_location().to_error_location(&string_table), {}
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

                template.content.before.push(expr);
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

                if !expr.kind.is_foldable() {
                    ast_log!("Template is no longer foldable");
                    *foldable = false;
                }

                template.content.before.push(expr);
            }

            TokenKind::Comma => {
                // Multiple commas in succession
                return_syntax_error!(
                    "Multiple commas used back to back in the template head. You must have a valid expression between each comma",
                    token_stream
                        .current_location()
                        .to_error_location(&string_table),
                    {}
                )
            }

            // Newlines / empty things in the scene head are ignored
            TokenKind::Newline | TokenKind::Empty => {
                token_stream.advance();
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Invalid Token Used Inside template head when creating template node. Token: {:?}",
                        token
                    ),
                    token_stream
                        .current_location()
                        .to_error_location(&string_table),
                    {}
                )
            }
        }

        comma_separator = false;
    }

    Ok(())
}
