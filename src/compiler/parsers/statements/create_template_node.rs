#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::template::{
    Style, TemplateContent, TemplateControlFlow, TemplateType,
};
use crate::compiler::parsers::tokens::{TextLocation, TokenContext, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::settings::BS_VAR_PREFIX;
use crate::{ast_log, return_compiler_error, return_syntax_error};
use std::collections::HashMap;

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
    pub fn default() -> Template {
        Template {
            content: TemplateContent::default(),
            kind: TemplateType::Comment,
            style: Style::default(),
            control_flow: TemplateControlFlow::None,
            id: String::new(),
            location: TextLocation::default(),
        }
    }
    pub fn function_template(
        content: TemplateContent,
        style: Style,
        id: String,
        control_flow: TemplateControlFlow,
        location: TextLocation,
    ) -> Template {
        Template {
            content,
            kind: TemplateType::FunctionTemplate,
            style,
            control_flow,
            id,
            location,
        }
    }
    pub fn folded_string(
        content: TemplateContent,
        style: Style,
        id: String,
        control_flow: TemplateControlFlow,
        location: TextLocation,
    ) -> Template {
        Template {
            content,
            kind: TemplateType::FoldedString,
            style,
            control_flow,
            id,
            location,
        }
    }

    pub fn slot(id: String, location: TextLocation) -> Template {
        Template {
            content: TemplateContent::default(),
            kind: TemplateType::Slot,
            style: Style::default(),
            control_flow: TemplateControlFlow::None,
            location,
            id,
        }
    }
    pub fn comment(location: TextLocation) -> Template {
        Template {
            content: TemplateContent::default(),
            kind: TemplateType::Comment,
            style: Style::default(),
            control_flow: TemplateControlFlow::None,
            id: String::new(),
            location,
        }
    }

    pub fn fold(&mut self, inherited_style: &Option<Style>) -> Result<String, CompileError> {
        // Now we start combining everything into one string
        let mut final_string = String::with_capacity(3);

        // Format. How will the content be parsed at compile time?
        if let Some(style) = inherited_style {
            // Each format has a different precedence, using the highest precedence.
            // But children with a lower precedence than the parent should reset their format to None.
            // This is because the parent will already parse that formatting over all its children.
            if style.formatter_precedence > self.style.formatter_precedence {
                self.style.compile_time_parser = None;

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
                ExpressionKind::ConstString(string) => {
                    final_string.push_str(string);
                }

                ExpressionKind::ConstFloat(float) => {
                    final_string.push_str(&float.to_string());
                }

                ExpressionKind::ConstInt(int) => {
                    final_string.push_str(&int.to_string());
                }

                // Add the string representation of the bool
                ExpressionKind::ConstBool(value) => {
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
        if let Some(formatter) = &self.style.compile_time_parser {
            formatter.format(&mut final_string);

            if inherited_style.is_some() {
                final_string.push(TEMPLATE_SPECIAL_IGNORE_CHAR);
            }
        }

        Ok(final_string)
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

// Recursive function to parse templates
pub fn new_template(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    unlocked_templates: &HashMap<String, ExpressionKind>,
    template_style: &mut Style,
) -> Result<Template, CompileError> {
    // These are variables or special keywords passed into the template head
    // let mut scene_template: TemplateContent = TemplateContent::default();

    // Templates that call any functions or have children that call functions
    // Can't be folded at compile time (EVENTUALLY CAN FOLD THE CONST FUNCTIONS TOO).
    // This is because the template might be changing at runtime.
    // If the entire template can be folded, it just becomes a string after the AST stage.
    let mut foldable = true;

    // The content of the scene
    let mut template_content: TemplateContent = TemplateContent::default();
    let mut this_template_content: TemplateContent = TemplateContent::default();

    // We can't modify what the parent template has unlocked.
    let mut this_template_unlocks = unlocked_templates.to_owned();

    let (control_flow, template_id) = parse_template_head(
        token_stream,
        context,
        &mut this_template_content,
        &mut this_template_unlocks,
        template_style,
        &mut template_content,
        &mut foldable,
    )?;

    // TODO, this will function as a special template in the compiler
    // It will have a usize in it that will determine the order of how elements from the template head are inserted into the body,
    // Like traditional template strings.
    // So the compiler can insert things into the slot
    // TokenKind::Slot => {
    //     return Ok(Template::slot(template_id, token_stream.current_location()));
    // }

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

    // ---------------------
    // TEMPLATE BODY PARSING
    // ---------------------
    while token_stream.index < token_stream.tokens.len() {
        match &token_stream.current_token_kind() {
            TokenKind::Eof => {
                break;
            }

            TokenKind::TemplateClose => {
                // Need to skip the closer
                token_stream.advance();
                break;
            }

            TokenKind::TemplateHead => {
                let nested_template =
                    new_template(token_stream, context, unlocked_templates, template_style)?;

                match nested_template.kind {
                    TemplateType::FunctionTemplate => {
                        this_template_content.concat(nested_template.content);
                    }

                    TemplateType::Slot => {
                        // TODO: wtf was this? redo slots, this is maybe the default unlabeled behaviour
                        // But labels need to place content into the template based on their argument order
                        // Arguments that are inserted into scenes later on can then use this slot behaviour for clever placement
                        // Should also support spread [..] for spreading all argument into that slot place

                        // Now we need to move everything from this scene so far into the before part
                        //template_content.before.extend(this_template_content.to_owned());
                        // this_template_content.clear();

                        // Everything else always gets moved to the scene after at the end
                    }

                    // Ignore everything else for now
                    _ => {}
                }
            }

            TokenKind::RawStringLiteral(content) | TokenKind::StringLiteral(content) => {
                this_template_content.after.push(Expression::string(
                    content.to_string(),
                    token_stream.current_location(),
                ));
            }

            // For templating values in scene heads in the body of scenes
            // Token::EmptyScene(spaces) => {
            //     scene_body.push(AstNode::SceneTemplate);
            //     for _ in 0..*spaces {
            //         scene_body.push(AstNode::Spaces(token_line_number));
            //     }
            // }
            TokenKind::Newline => {
                this_template_content.after.push(Expression::string(
                    "\n".to_string(),
                    token_stream.current_location(),
                ));
            }

            TokenKind::Empty | TokenKind::Colon => {}

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Invalid Token Used Inside template body when creating template node. Token: {:?}",
                    token_stream.current_token_kind()
                )
            }
        }

        token_stream.advance();
    }

    template_content.concat(this_template_content);

    if foldable {
        return Ok(Template::folded_string(
            template_content,
            template_style.to_owned(),
            template_id,
            control_flow,
            token_stream.current_location(),
        ));
    };

    Ok(Template::function_template(
        template_content,
        template_style.to_owned(),
        template_id,
        control_flow,
        token_stream.current_location(),
    ))
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
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    this_template_content: &mut TemplateContent,
    unlocked_templates: &mut HashMap<String, ExpressionKind>,
    template_style: &mut Style,
    template_content: &mut TemplateContent,
    foldable: &mut bool,
) -> Result<(TemplateControlFlow, String), CompileError> {
    // TODO: Add control flow parsing
    let mut control_flow = TemplateControlFlow::None;
    let mut template_id: String = format!("{BS_VAR_PREFIX}templateID_{}", token_stream.index);

    // Each expression must be separated with a comma
    let mut comma_separator = true;

    while token_stream.index < token_stream.length {
        token_stream.advance();
        let token = token_stream.current_token_kind().to_owned();

        ast_log!("Parsing template head token: {:?}", token);

        // We are doing something similar to new_ast()
        // But with the specific scene head syntax,
        // so expressions are allowed and should be folded where possible.
        // Loops and if statements can end the scene head.

        // Returning without a scene body
        // EOF is in here for template repl atm and for the convenience
        // of not having to explicitly close the template head from a repl session.
        // This MIGHT lead to some overly forgiving behavior (not warning about an unclosed template head)
        if token == TokenKind::TemplateClose || token == TokenKind::Eof {
            return Ok((control_flow, template_id));
        }

        if token == TokenKind::Colon {
            token_stream.advance();
            return Ok((control_flow, template_id));
        }

        // Make sure there is a comma before the next token
        if !comma_separator {
            if token != TokenKind::Comma {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Expected a comma before the next token in the template head. Token: {:?}",
                    token
                )
            }

            comma_separator = true;
            continue;
        };

        match token {
            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            TokenKind::Id(name) => {
                template_id = format!("{BS_VAR_PREFIX}{name}");
            }

            // If this is a template, we have to do some clever parsing here
            TokenKind::Symbol(name) => {
                // TODO - sort out the final design for inherited styles / templates
                // Should unlocked styles just be passed in as normal declarations?

                // Check if this is an unlocked scene (inherited from an ancestor)
                // This has to be done eagerly here as any previous scene or style passed into the scene head will add to this
                match unlocked_templates.to_owned().get(&name) {
                    Some(ExpressionKind::Template(template)) => {
                        // This can't be folded anymore as a non-constant is being referenced
                        *foldable = false;

                        template_style.child_default = template.style.child_default.to_owned();

                        if template.style.unlocks_override {
                            unlocked_templates.clear();
                        }

                        // Insert this style's unlocked scenes into the unlocked scenes map
                        for (name, style) in template.style.unlocked_templates.iter() {
                            // Should this overwrite? Or skip if already unlocked?
                            unlocked_templates.insert(name.to_owned(), style.to_owned());
                        }

                        // Unpack this scene into this scene's body
                        template_content
                            .before
                            .extend(template.content.before.to_owned());
                        template_content
                            .after
                            .splice(0..0, template.content.after.to_owned());

                        continue;
                    }

                    // Constant inherited
                    Some(ExpressionKind::ConstString(string)) => {
                        this_template_content.after.push(create_expression(
                            token_stream,
                            context,
                            &mut DataType::String(Ownership::default()),
                            false,
                        )?);
                    }
                    _ => {}
                }

                // Otherwise, check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = context.find_reference(&name) {
                    match &arg.value.kind {
                        // Reference to another string template
                        ExpressionKind::Template(template) => {
                            *foldable = false;

                            // Override the current child_default if there is a new one coming in
                            template_style.child_default = template.style.child_default.to_owned();

                            if template.style.unlocks_override {
                                unlocked_templates.clear();
                            }

                            // Insert this style's unlocked scenes into the unlocked scenes map
                            for (name, style) in template.style.unlocked_templates.iter() {
                                // Should this overwrite? Or skip if already unlocked?
                                // Which is less efficient?
                                unlocked_templates.insert(name.to_owned(), style.to_owned());
                            }

                            // Unpack this scene into this scene's body
                            template_content.concat(template.content.to_owned());
                        }

                        // Otherwise this is a reference to some other variable
                        // String, Number, Bool, etc. References
                        _ => {
                            let expr = create_expression(
                                token_stream,
                                context,
                                &mut DataType::CoerceToString(Ownership::default()),
                                false,
                            )?;

                            // Any non-constant expression can't be folded
                            if !expr.kind.is_foldable() {
                                *foldable = false;
                            }

                            this_template_content.after.push(expr);
                        }
                    }

                    continue;
                } else {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Cannot declare new variables inside of a template head. Variable '{}' is not declared.
                        \n Here are all the variables in scope: {:#?}",
                        name,
                        context.declarations
                    )
                };
            }

            // Possible Constants to Parse
            // Can chuck these directly into the content
            TokenKind::FloatLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringLiteral(_)
            | TokenKind::RawStringLiteral(_) => {
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::Inferred(Ownership::default()),
                    false,
                )?;

                if !expr.kind.is_foldable() {
                    *foldable = false;
                }

                this_template_content.after.push(expr);
            }

            TokenKind::OpenParenthesis => {
                let expr = create_expression(
                    token_stream,
                    context,
                    &mut DataType::CoerceToString(Ownership::default()),
                    true,
                )?;

                if !expr.kind.is_foldable() {
                    *foldable = false;
                }

                this_template_content.after.push(expr);
            }

            TokenKind::Comma => {
                // Multiple commas in succession
                return_syntax_error!(
                    token_stream.current_location(),
                    "Multiple commas used back to back in the template head. You must have a valid expression between each comma"
                )
            }

            // Newlines / empty things in the scene head are ignored
            TokenKind::Newline | TokenKind::Empty => {}

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Invalid Token Used Inside template head when creating template node. Token: {:?}",
                    token
                )
            }
        }
    }

    Ok((control_flow, template_id))
}
