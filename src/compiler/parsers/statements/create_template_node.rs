#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::statements::structs::create_args;
use crate::compiler::parsers::template::{Style, StyleFormat, TemplateContent, TemplateType};
use crate::compiler::parsers::tokens::{TextLocation, TokenContext, TokenKind};
use crate::compiler::traits::ContainsReferences;
use crate::{return_compiler_error, return_rule_error, return_syntax_error};
use crate::settings::{BEANSTALK_FILE_EXTENSION, BS_VAR_PREFIX};
use std::collections::HashMap;
use crate::compiler::html5_codegen::code_block_highlighting::highlight_html_code_block;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::markdown::to_markdown;

#[derive(Debug, Clone, PartialEq)]
pub struct Template {
    pub head: Vec<AstNode>,
    pub content: TemplateContent,
    pub kind: TemplateType,
    pub style: Style,
    pub id: String,
    pub location: TextLocation,
}

impl Template {
    pub fn default() -> Template {
        Template {
            head: Vec::new(),
            content: TemplateContent::default(),
            kind: TemplateType::Comment,
            style: Style::default(),
            id: String::new(),
            location: TextLocation::default(),
        }
    }
    pub fn string_template(
        head: Vec<AstNode>,
        content: TemplateContent,
        style: Style,
        id: String,
        location: TextLocation,
    ) -> Template {
        Template {
            head,
            content,
            kind: TemplateType::StringTemplate,
            style,
            id,
            location,
        }
    }
    pub fn slot(id: String, location: TextLocation) -> Template {
        Template {
            head: Vec::new(),
            content: TemplateContent::default(),
            kind: TemplateType::Slot,
            style: Style::default(),
            location,
            id,
        }
    }
    pub fn comment(location: TextLocation) -> Template {
        Template {
            head: Vec::new(),
            content: TemplateContent::default(),
            kind: TemplateType::Comment,
            style: Style::default(),
            id: String::new(),
            location,
        }
    }

    // Returns a regular string containing the parsed template
    // SOME TEMPORARY NONSENSE THAT WILL PROBABLY BE REMOVED
    pub fn parse_into_string(
        &mut self,
        inherited_style: Option<Style>,
        position: &TextLocation,
    ) -> Result<String, CompileError> {

        // Set everything apart from the wrappers for the new style
        let mut final_style = match inherited_style {
            Some(style) => style.to_owned(),
            None => Style::default(),
        };

        // Format. How will the content be parsed?
        // Each format has a different precedence, using the highest precedence
        if self.style.format > final_style.format {
            final_style.format = self.style.format.to_owned();
        }

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

        // Now we start combining everything into one string
        let mut final_string = String::new();

        // Everything inserted into the body
        // This needs to be done now
        // so Markdown will parse any added literals correctly
        let mut content = String::new();

        // template content
        for value in self.content.flatten() {
            match &value.kind {
                ExpressionKind::String(string) => {
                    content.push_str(string);
                }

                ExpressionKind::Float(float) => {
                    content.push_str(&float.to_string());
                }

                ExpressionKind::Int(int) => {
                    content.push_str(&int.to_string());
                }

                // Add the string representation of the bool
                ExpressionKind::Bool(value) => {
                    content.push_str(&value.to_string());
                }

                ExpressionKind::Template(template) => {
                    let new_template = template.to_owned().parse_into_string(
                        final_style.child_default.to_owned().map(|b| *b),
                        position,
                    )?;

                    content.push_str(&new_template);
                }

                ExpressionKind::None => {
                    // Ignore this
                    // Currently 'ignored' or hidden templates result in a None value being added to a template,
                    // So it's not an error
                    // Hopefully the compiler will always catch unintended use of None in templates.
                    // May emit a warning in future if this is possible from user error.
                }

                ExpressionKind::Runtime(_nodes) => {
                    // TODO
                }

                ExpressionKind::Reference(name) => {
                    // TODO: Variable references in templates - if reference can't be folded at compile time,
                    // evaluation and string coercion must happen at runtime
                    content.push_str(&format!("${}", name));
                }

                ExpressionKind::Function(..) => {
                    return_rule_error!(
                    position.to_owned(),
                    "Functions are not supported in Template Heads"
                )
                }

                // At this point, if this structure was a style, those fields and inner template would have been parsed
                // So we can just unpack any other public fields into the template as strings
                ExpressionKind::Struct(..) => {
                    return_rule_error!(
                    position.to_owned(),
                    "You can't declare new variables inside of Template Heads"
                )
                }

                // Collections will be unpacked into a template
                ExpressionKind::Collection(_) => {
                    return_compiler_error!(
                    "Collections inside template heads not yet implemented in the compiler."
                )
                }

                ExpressionKind::Range(..) => {
                    // TODO: chuck all values into the template
                }
            }
        }

        // If this is a Markdown template, and the parent isn't one,
        // parse the content into Markdown
        // If the parent is parsing the Markdown already,
        // skip this as it should be done at the highest level possible
        if final_style.format == StyleFormat::Markdown && self.style.format != StyleFormat::Markdown {
            let default_tag = "p";

            final_string.push_str(&to_markdown(&content, default_tag));

        // If the parent is outputting Markdown and the style is now a Codeblock
        // Codeblocks can't have children, so there's no need to check that like above
        } else if final_style.format == StyleFormat::Codeblock
            && self.style.format == StyleFormat::Markdown
        {
            // Add a special object replace character to signal to the parent that this tag should not be parsed into Markdown
            final_string.push_str(&format!(
                "\u{FFFC}<pre><code>{}</code></pre>\u{FFFC}",
                highlight_html_code_block(&content, BEANSTALK_FILE_EXTENSION)
            ));

        // No need to do any additional parsing to the content
        // Might already be parsed by the parent, or just raw
        } else {
            final_string.push_str(&content);
        }

        Ok(final_string)
    }
}

// Recursive function to parse templates
pub fn new_template(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    unlocked_templates: &mut HashMap<String, ExpressionKind>,
    template_style: &mut Style,
) -> Result<Template, CompileError> {
    // These are variables or special keywords passed into the template head
    // let mut scene_template: TemplateContent = TemplateContent::default();

    // The content of the scene
    let mut template_content: TemplateContent = TemplateContent::default();
    let mut this_template_content: TemplateContent = TemplateContent::default();

    token_stream.advance();

    let (template_head, template_id) = parse_template_head(
        token_stream,
        context,
        &mut this_template_content,
        unlocked_templates,
        template_style,
        &mut template_content,
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
                break;
            }

            TokenKind::TemplateHead => {
                let nested_template =
                    new_template(token_stream, context, unlocked_templates, template_style)?;

                match nested_template.kind {
                    TemplateType::StringTemplate => {
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
    Ok(Template::string_template(
        template_head,
        template_content,
        template_style.to_owned(),
        template_id,
        token_stream.current_location(),
    ))
}

// ---------------------
// TEMPLATE HEAD PARSING
// ---------------------
pub fn parse_template_head(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
    this_template_content: &mut TemplateContent,
    unlocked_templates: &mut HashMap<String, ExpressionKind>,
    template_style: &mut Style,
    template_content: &mut TemplateContent,
) -> Result<(Vec<AstNode>, String), CompileError> {

    let mut template_head: Vec<AstNode> = Vec::new();
    let mut template_id: String = format!("{BS_VAR_PREFIX}templateID_{}", token_stream.index);

    while token_stream.index < token_stream.length {
        let token = token_stream.current_token_kind().to_owned();

        token_stream.advance();

        // We are doing something similar to new_ast()
        // But with the specific scene head syntax,
        // so expressions are allowed and should be folded where possible.
        // Loops and if statements can end the scene head.
        match token {
            TokenKind::Colon => {
                break;
            }

            // Returning without a scene body
            // EOF is in here for template repl atm and for the convenience
            // of not having to explicitly close the template head from a repl session.
            // This MIGHT lead to some overly forgiving behavior (not warning about an unclosed template head)
            TokenKind::TemplateClose | TokenKind::Eof => {
                token_stream.go_back();

                return Ok((template_head, template_id));
            }

            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            TokenKind::Id(name) => {
                template_id = format!("{BS_VAR_PREFIX}{}", name);
            }

            // If this is a template, we have to do some clever parsing here
            TokenKind::Symbol(name) => {
                // TODO - sort all this out.
                // Should unlocked styles just be passed in as normal declarations?

                // Check if this is an unlocked scene
                if let Some(ExpressionKind::Template(template)) =
                    unlocked_templates.to_owned().get(&name)
                {
                    template_style.child_default = template.style.child_default.to_owned();

                    if template.style.unlocks_override {
                        unlocked_templates.clear();
                    }

                    // Insert this style's unlocked scenes into the unlocked scenes map
                    if template.style.has_no_unlocked_templates() {
                        for (name, style) in template.style.unlocked_templates.iter() {
                            // Should this overwrite? Or skip if already unlocked?
                            unlocked_templates.insert(name.to_owned(), style.to_owned());
                        }
                    }

                    // Unpack this scene into this scene's body
                    template_content.before.extend(template.content.before.to_owned());
                    template_content.after.splice(0..0, template.content.after.to_owned());

                    continue;
                }

                // Otherwise, check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = context.find_reference(&name) {
                    match &arg.value.kind {

                        // Reference to another string template
                        ExpressionKind::Template(template) => {
                            template_style.child_default = template.style.child_default.to_owned();

                            if template.style.unlocks_override {
                                unlocked_templates.clear();
                            }

                            // Insert this style's unlocked scenes into the unlocked scenes map
                            if template.style.has_no_unlocked_templates() {
                                for (name, style) in template.style.unlocked_templates.iter() {
                                    // Should this overwrite? Or skip if already unlocked?
                                    unlocked_templates.insert(name.to_owned(), style.to_owned());
                                }
                            }

                            // Unpack this scene into this scene's body
                            template_content.concat(template.content.to_owned());

                            continue;
                        }

                        _ => {
                            token_stream.go_back();

                            let expr = create_expression(
                                token_stream,
                                &context,
                                &mut DataType::CoerceToString(Ownership::default()),
                                false,
                            )?;

                            this_template_content.after.push(expr);
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

            // Constants to Parse
            // Can chuck these directly into the content
            TokenKind::FloatLiteral(_)
            | TokenKind::BoolLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::StringLiteral(_)
            | TokenKind::RawStringLiteral(_) => {
                token_stream.go_back();

                this_template_content.after.push(create_expression(
                    token_stream,
                    &context,
                    &mut DataType::CoerceToString(Ownership::default()),
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

            TokenKind::OpenParenthesis => {
                let structure = create_args(token_stream, None, &[], &context)?;

                this_template_content.after.push(Expression::structure(
                    structure,
                    token_stream.current_location(),
                ));
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

    Ok((template_head, template_id))
}
