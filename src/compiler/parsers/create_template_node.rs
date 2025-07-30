use crate::compiler::compiler_errors::ErrorType;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use super::expressions::parse_expression::create_expression;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::structs::create_args;
use crate::compiler::parsers::template::{Style, StyleFormat, TemplateContent, TemplateType};
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::return_syntax_error;
use crate::settings::BS_VAR_PREFIX;
use std::collections::HashMap;
// const DEFAULT_SLOT_NAME: &str = "_slot";

// Recursive function to parse templates
pub fn new_template(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    unlocked_templates: &mut HashMap<String, ExpressionKind>,
    template_style: &mut Style,
) -> Result<TemplateType, CompileError> {
    // These are variables or special keywords passed into the template head
    // let mut scene_template: TemplateContent = TemplateContent::default();

    // The content of the scene
    // There are 3 Vecs here as any slots from scenes in the scene head need to be inserted around the body
    let mut template_body: TemplateContent = TemplateContent::default();
    let mut this_template_body: Vec<Expression> = Vec::new();

    let mut template_id: String = format!("{BS_VAR_PREFIX}templateID_{}", token_stream.index);

    token_stream.advance();

    // TEMPLATE HEAD PARSING
    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();

        // let inside_brackets = token == TokenKind::OpenParenthesis;

        token_stream.advance();

        match token {
            TokenKind::Colon => {
                break;
            }

            TokenKind::TemplateClose => {
                token_stream.go_back();

                create_final_template_body(&mut template_body, this_template_body);

                return Ok(TemplateType::Template(Expression::template(
                    template_body,
                    template_style.to_owned(),
                    template_id,
                    token_stream.current_location(),
                )));
            }

            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            TokenKind::Id(name) => {
                template_id = format!("{BS_VAR_PREFIX}{}", name);
            }

            // TODO, this will function as a special template in the compiler
            // It will have a usize in it that will determine the order of how elements from the template head are inserted into the body,
            // Like traditional template strings.
            // So the compiler can insert things into the slot
            TokenKind::Slot => return Ok(TemplateType::Slot(0)),

            TokenKind::Markdown => {
                template_style.format = StyleFormat::Markdown;
            }

            // If this is a template, we have to do some clever parsing here
            TokenKind::Symbol(name, ..) => {
                // TODO - sort all this out.
                // Should unlocked styles just be passed in as normal declarations?

                // Check if this is an unlocked scene
                if let Some(ExpressionKind::Template(body, style, ..)) =
                    unlocked_templates.to_owned().get(&name)
                {
                    template_style.child_default = style.child_default.to_owned();

                    if style.unlocks_override {
                        unlocked_templates.clear();
                    }

                    // Insert this style's unlocked scenes into the unlocked scenes map
                    if style.has_no_unlocked_templates() {
                        for (name, style) in style.unlocked_templates.iter() {
                            // Should this overwrite? Or skip if already unlocked?
                            unlocked_templates.insert(name.to_owned(), style.to_owned());
                        }
                    }

                    // Unpack this scene into this scene's body
                    template_body.before.extend(body.before.to_owned());
                    template_body.after.splice(0..0, body.after.to_owned());

                    continue;
                }

                // Otherwise, check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = context.find_reference(&name) {
                    match &arg.value.kind {
                        ExpressionKind::Template(body, style, ..) => {
                            template_style.child_default = style.child_default.to_owned();

                            if style.unlocks_override {
                                unlocked_templates.clear();
                            }

                            // Insert this style's unlocked scenes into the unlocked scenes map
                            if style.has_no_unlocked_templates() {
                                for (name, style) in style.unlocked_templates.iter() {
                                    // Should this overwrite? Or skip if already unlocked?
                                    unlocked_templates.insert(name.to_owned(), style.to_owned());
                                }
                            }

                            // Unpack this scene into this scene's body
                            template_body.before.extend(body.before.to_owned());
                            template_body.after.splice(0..0, body.after.to_owned());

                            continue;
                        }

                        _ => {
                            token_stream.go_back();

                            let expr = create_expression(
                                token_stream,
                                &context,
                                &mut DataType::CoerceToString(false),
                                false,
                            )?;

                            this_template_body.push(expr);
                        }
                    }
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

            // Expressions to Parse
            TokenKind::FloatLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringLiteral(_)
            | TokenKind::RawStringLiteral(_) => {
                token_stream.go_back();

                this_template_body.push(create_expression(
                    token_stream,
                    &context,
                    &mut DataType::CoerceToString(false),
                    false,
                )?);
            }

            TokenKind::Comma => {
                // TODO - decide if this should be enforced as a syntax error or allowed
                // Currently working around commas not ever being needed in scene heads
                // So may enforce it with full error in the future (especially if it causes havoc in the emitter stage)
                red_ln!(
                    "Warning: Should there be a comma in the template head? (ignored by compiler)"
                );
            }

            // Newlines / empty things in the scene head are ignored
            TokenKind::Newline | TokenKind::Empty => {}

            TokenKind::CodeKeyword => {
                template_style.format = StyleFormat::Codeblock;
                template_style.child_default = None;
            }

            TokenKind::OpenParenthesis => {
                let structure = create_args(token_stream, Expression::none(), &[], &context)?;

                this_template_body.push(Expression::structure(
                    structure,
                    token_stream.current_location(),
                ));
            }

            TokenKind::Ignore => {
                // Should also clear any styles or tags in the scene
                *template_style = Style::default();

                // Keep track of how many scene opens there are
                // This is to make sure the scene close is at the correct place
                let mut extra_template_opens = 1;
                while token_stream.index < token_stream.length {
                    match token_stream.current_token_kind() {
                        TokenKind::TemplateClose => {
                            extra_template_opens -= 1;
                            if extra_template_opens == 0 {
                                token_stream.advance(); // Skip the closing scene close
                                break;
                            }
                        }
                        TokenKind::TemplateOpen => {
                            extra_template_opens += 1;
                        }
                        TokenKind::Eof => {
                            break;
                        }
                        _ => {}
                    }
                    token_stream.advance();
                }

                return Ok(TemplateType::Comment);
            }

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Invalid Token Used Inside template head when creating template node. Token: {:?}",
                    token
                )
            }
        }
    }

    // TEMPLATE BODY PARSING
    while token_stream.index < token_stream.tokens.len() {
        match &token_stream.current_token_kind() {
            TokenKind::Eof => {
                break;
            }

            TokenKind::TemplateClose => {
                break;
            }

            TokenKind::TemplateHead => {
                let nested_template =
                    new_template(token_stream, context, unlocked_templates, template_style)?;

                match nested_template {
                    TemplateType::Template(template) => {
                        this_template_body.push(template);
                    }

                    TemplateType::Slot(..) => {
                        // Now we need to move everything from this scene so far into the before part
                        template_body.before.extend(this_template_body.to_owned());
                        this_template_body.clear();

                        // Everything else always gets moved to the scene after at the end
                    }

                    // Ignore everything else for now
                    _ => {}
                }
            }

            TokenKind::RawStringLiteral(content) | TokenKind::StringLiteral(content) => {
                this_template_body.push(Expression::string(
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
                this_template_body.push(Expression::string(
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

    // The body of this scene is now added to the final scene body
    create_final_template_body(&mut template_body, this_template_body);

    Ok(TemplateType::Template(Expression::template(
        template_body,
        template_style.to_owned(),
        template_id,
        token_stream.current_location(),
    )))
}

fn create_final_template_body(
    template_body: &mut TemplateContent,
    this_template_body: Vec<Expression>,
) {
    template_body.after.splice(0..0, this_template_body);
}
