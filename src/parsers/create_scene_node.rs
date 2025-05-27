use super::{ast_nodes::Arg, expressions::parse_expression::create_expression};
use crate::parsers::ast_nodes::Expr;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::scene::{SceneContent, SceneType, Style, StyleFormat};
use crate::parsers::structs::create_args;
use crate::settings::BS_VAR_PREFIX;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use colour::yellow_ln;
use std::collections::HashMap;
// const DEFAULT_SLOT_NAME: &str = "_slot";

// Recursive function to parse scenes
pub fn new_scene(
    x: &mut TokenContext,
    declarations: &[Arg],
    unlocked_scenes: &mut HashMap<String, Expr>,
    scene_style: &mut Style,
) -> Result<SceneType, CompileError> {
    // These are variables or special keywords passed into the scene head
    // let mut scene_head: SceneContent = SceneContent::default();

    // The content of the scene
    // There are 3 Vecs here as any slots from scenes in the scene head need to be inserted around the body
    let mut scene_body: SceneContent = SceneContent::default();
    let mut this_scene_body: Vec<Expr> = Vec::new();

    // Set a default ID just in case none is set manually
    // This guarantees each scene ID will be unique
    let module_name = match x.tokens.first() {
        Some(Token::ModuleStart(name)) => name.to_owned(),
        _ => {
            return Err(CompileError {
                msg: "No module name found for this scene".to_owned(),
                start_pos: x.token_positions[x.index].to_owned(),
                end_pos: TokenPosition {
                    line_number: x.token_positions[x.index].line_number,
                    char_column: x.token_positions[x.index].char_column + 1,
                },
                error_type: ErrorType::Compiler,
            });
        }
    };

    let mut scene_id: String = format!("sceneID_{module_name}_{}", x.index);

    x.advance();

    // SCENE HEAD PARSING
    while x.index < x.length {
        let token = x.current_token().to_owned();

        let inside_brackets = token == Token::OpenParenthesis;

        x.advance();

        match token {
            Token::Colon => {
                break;
            }

            Token::SceneClose => {
                x.go_back();

                create_final_scene_body(&mut scene_body, this_scene_body);

                return Ok(SceneType::Scene(Expr::Scene(
                    scene_body,
                    scene_style.to_owned(),
                    scene_id,
                )));
            }

            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            Token::Id(name) => {
                scene_id = format!("{BS_VAR_PREFIX}{}", name);
            }

            // For now, this will function as a special scene in the compiler
            // That has a special ID based on the parent scene's ID
            // So the compiler can insert things into the slot using the special ID automatically
            Token::Slot => return Ok(SceneType::Slot),

            Token::Markdown => {
                scene_style.format = StyleFormat::Markdown;
            }

            // If this is a scene, we have to do some clever parsing here
            Token::Variable(name, ..) => {
                // TODO - sort all this out.
                // Should unlocked styles just be passed in as normal declarations?

                // Check if this is an unlocked scene
                if let Some(Expr::Scene(body, style, ..)) = unlocked_scenes.to_owned().get(&name) {
                    scene_style.child_default = style.child_default.to_owned();

                    if style.unlocks_override {
                        unlocked_scenes.clear();
                    }

                    // Insert this style's unlocked scenes into the unlocked scenes map
                    if !style.unlocked_scenes.is_empty() {
                        for (name, style) in style.unlocked_scenes.iter() {
                            // Should this overwrite? Or skip if already unlocked?
                            unlocked_scenes.insert(name.to_owned(), style.to_owned());
                        }
                    }

                    // Unpack this scene into this scene's body
                    scene_body.before.extend(body.before.to_owned());
                    scene_body.after.splice(0..0, body.after.to_owned());

                    continue;
                }

                // Otherwise, check if it's a regular scene or variable reference
                // If this is a reference to a function or variable
                if let Some(arg) = declarations.iter().find(|a| a.name == name) {
                    match &arg.expr {
                        Expr::Scene(body, style, ..) => {
                            scene_style.child_default = style.child_default.to_owned();

                            if style.unlocks_override {
                                unlocked_scenes.clear();
                            }

                            // Insert this style's unlocked scenes into the unlocked scenes map
                            if !style.unlocked_scenes.is_empty() {
                                for (name, style) in style.unlocked_scenes.iter() {
                                    // Should this overwrite? Or skip if already unlocked?
                                    unlocked_scenes.insert(name.to_owned(), style.to_owned());
                                }
                            }

                            // Unpack this scene into this scene's body
                            scene_body.before.extend(body.before.to_owned());
                            scene_body.after.splice(0..0, body.after.to_owned());

                            continue;
                        }
                        _ => {
                            let expr = create_expression(
                                x,
                                &mut DataType::CoerceToString(false),
                                inside_brackets,
                                declarations,
                                &[],
                            )?;
                            this_scene_body.push(expr);
                        }
                    }
                } else {
                    return Err(CompileError {
                        msg: format!(
                            "Cannot declare new variables inside of a scene head. Variable '{}' is not declared",
                            name
                        ),
                        start_pos: x.token_positions[x.index].to_owned(),
                        end_pos: TokenPosition {
                            line_number: x.token_positions[x.index].line_number,
                            char_column: x.token_positions[x.index].char_column + name.len() as i32,
                        },
                        error_type: ErrorType::Syntax,
                    });
                };
            }

            // Expressions to Parse
            Token::FloatLiteral(_)
            | Token::BoolLiteral(_)
            | Token::IntLiteral(_)
            | Token::StringLiteral(_)
            | Token::RawStringLiteral(_) => {
                x.go_back();

                this_scene_body.push(create_expression(
                    x,
                    &mut DataType::CoerceToString(false),
                    inside_brackets,
                    declarations,
                    &[],
                )?);
            }

            Token::Comma => {
                // TODO - decide if this should be enforced as a syntax error or allowed
                // Currently working around commas not ever being needed in scene heads
                // So may enforce it with full error in the future (especially if it causes havoc in the emitter stage)
                yellow_ln!(
                    "Warning: Should there be a comma in the scene head? (ignored by compiler)"
                );
            }

            // Newlines / empty things in the scene head are ignored
            Token::Newline | Token::Empty => {}

            Token::CodeKeyword => {
                scene_style.format = StyleFormat::Codeblock;
                scene_style.child_default = None;
            }

            Token::OpenParenthesis => {
                let structure = create_args(x, Expr::None, &[], declarations)?;

                this_scene_body.push(Expr::Args(structure));
            }

            Token::Ignore => {
                // Should also clear any styles or tags in the scene
                *scene_style = Style::default();

                // Keep track of how many scene opens there are
                // This is to make sure the scene close is at the correct place
                let mut extra_scene_opens = 1;
                while x.index < x.length {
                    match x.current_token() {
                        Token::SceneClose => {
                            extra_scene_opens -= 1;
                            if extra_scene_opens == 0 {
                                x.advance(); // Skip the closing scene close
                                break;
                            }
                        }
                        Token::SceneOpen => {
                            extra_scene_opens += 1;
                        }
                        Token::EOF => {
                            break;
                        }
                        _ => {}
                    }
                    x.advance();
                }

                return Ok(SceneType::Comment);
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Token Used Inside scene head when creating scene node. Token: {:?}",
                        token
                    ),
                    start_pos: x.token_positions[x.index].to_owned(),
                    end_pos: TokenPosition {
                        line_number: x.token_positions[x.index].line_number,
                        char_column: x.token_positions[x.index].char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }
    }

    // SCENE BODY PARSING
    while x.index < x.tokens.len() {
        let token_line_number = x.token_positions[x.index].line_number;
        let token_char_column = x.token_positions[x.index].char_column;

        match &x.current_token() {
            Token::EOF => {
                break;
            }

            Token::SceneClose => {
                break;
            }

            Token::SceneHead => {
                let nested_scene =
                    new_scene(x, declarations, unlocked_scenes, scene_style)?;

                match nested_scene {
                    SceneType::Scene(scene) => {
                        this_scene_body.push(scene);
                    }

                    SceneType::Slot => {
                        // Now we need to move everything from this scene so far into the before part
                        scene_body.before.extend(this_scene_body.to_owned());
                        this_scene_body.clear();

                        // Everything else always gets moved to the scene after at the end
                    }

                    // Ignore everything else for now
                    _ => {}
                }
            }

            Token::RawStringLiteral(content) | Token::StringLiteral(content) => {
                this_scene_body.push(Expr::String(content.to_string()));
            }

            // For templating values in scene heads in the body of scenes
            // Token::EmptyScene(spaces) => {
            //     scene_body.push(AstNode::SceneTemplate);
            //     for _ in 0..*spaces {
            //         scene_body.push(AstNode::Spaces(token_line_number));
            //     }
            // }
            Token::Newline => {
                this_scene_body.push(Expr::String("\n".to_string()));
            }

            Token::Empty | Token::Colon => {}

            Token::DeadVariable(name) => {
                return Err(CompileError {
                    msg: format!("Dead Variable used in scene. '{}' was never defined", name),
                    start_pos: x.token_positions[x.index].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column + name.len() as i32,
                    },
                    error_type: ErrorType::Caution,
                });
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Syntax Used Inside scene body when creating scene node: {:?}",
                        x.current_token()
                    ),
                    start_pos: x.token_positions[x.index].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }

        x.advance();
    }

    // The body of this scene is now added to the final scene body
    create_final_scene_body(&mut scene_body, this_scene_body);

    Ok(SceneType::Scene(Expr::Scene(
        scene_body,
        scene_style.to_owned(),
        scene_id,
    )))
}

fn create_final_scene_body(scene_body: &mut SceneContent, this_scene_body: Vec<Expr>) {
    scene_body.after.splice(0..0, this_scene_body);
}
