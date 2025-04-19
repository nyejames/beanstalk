use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
};
use crate::parsers::ast_nodes::Value;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::scene::{Style, StyleFormat};
use crate::parsers::structs::new_fixed_collection;
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, Token, bs_types::DataType};
use colour::yellow_ln;
use std::collections::HashMap;

// Recursive function to parse scenes
pub fn new_scene(
    x: &mut TokenContext,
    ast: &[AstNode],
    declarations: &mut Vec<Arg>,
    unlocked_styles: &mut HashMap<String, Style>,
) -> Result<Value, CompileError> {
    let mut scene_body: Vec<Value> = Vec::new();
    let mut scene_id: String = String::new();

    x.index += 1;

    let mut scene_styles: Vec<Style> = Vec::new();

    // SCENE HEAD PARSING
    while x.index < x.length {
        let token = x.current_token().to_owned();

        let inside_brackets = token == Token::OpenParenthesis;

        x.index += 1;

        // red_ln!("token being parsed for AST: {:?}", token);

        match token {
            Token::Colon => {
                break;
            }

            Token::SceneClose => {
                x.index -= 1;
                return Ok(Value::Scene(scene_body, scene_styles, scene_id));
            }

            // This is a declaration of the ID by using the export prefix followed by a variable name
            // This doesn't follow regular declaration rules.
            Token::Id(name) => {
                scene_id = name.to_string();
            }

            // This could be a config or style for the scene itself.
            // So the type must be figured out first to see if it's passed into the scene directly or not
            // It could also be an unlocked style, so unlocked styles are checked first
            Token::Variable(name, _) => {
                // Check if this is an unlocked style
                if let Some(style) = unlocked_styles.to_owned().get(&name) {
                    scene_styles.push(style.to_owned());

                    if style.unlocks_override {
                        unlocked_styles.clear();
                    }

                    // Insert this style's unlocked styles into the unlocked styles map
                    if !style.unlocked_styles.is_empty() {
                        for (name, style) in style.unlocked_styles.iter() {
                            // Should this overwrite? Or skip if already unlocked?
                            unlocked_styles.insert(name.to_owned(), style.to_owned());
                        }
                    }

                    continue;
                }

                // Otherwise check if it's a regular style or variable reference
                // If this is a reference to a function or variable
                let value = if let Some(arg) = declarations.iter().find(|a| a.name == name) {
                    // Here we need to evaluate the expression
                    // This is because functions can be folded into styles (or at least eventually can be)
                    create_expression(
                        x,
                        false,
                        ast,
                        &mut DataType::CoerceToString(arg.data_type.is_mutable()),
                        false,
                        declarations,
                    )?
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

                match value {
                    // Check if this is a style or reference to a value
                    // Must follow all the rules with how a new style overrides the current style.
                    Value::StructLiteral(structure) => {
                        let mut structure_args: Vec<Arg> = Vec::new();
                        let mut style: Style = Style::default();

                        for arg in structure {
                            match arg.name.as_str() {
                                // pub format: StyleFormat,
                                //
                                // // Removes any parent wrappers lower than this precedence
                                // // Before adding its own wrappers
                                // pub parent_override: i32,
                                //
                                // pub neighbour_rule: NeighbourRule,
                                //
                                // // Passes a default style for any children to start with
                                // // Wrappers can be overridden with parent overrides
                                // // Or child wrappers that are higher precedence
                                // pub child_default: Option<Box<PrecedenceStyle>>,
                                //
                                // pub compatibility: SceneCompatibility,
                                //
                                // // Styles that children of this scene can now use
                                // pub unlocked_styles: HashMap<String, Style>,
                                //
                                // // If this is true, no unlocked styles will be inherited from the parent
                                // pub unlocks_override: bool,
                                "format" => {
                                    style.format = match arg.value {
                                        Value::Int(int) => int,
                                        _ => {
                                            return Err(CompileError {
                                                msg: "Expected an integer for the format field of a scene".to_string(),
                                                start_pos: arg.value.dimensions(),
                                                end_pos: TokenPosition {
                                                    line_number: arg.value.dimensions().line_number,
                                                    char_column: arg.value.dimensions().char_column + arg.name.len() as i32,
                                                },
                                                error_type: ErrorType::Syntax,
                                            });
                                        }
                                    };
                                }

                                _ => {
                                    structure_args.push(arg);
                                }
                            }
                        }

                        // Insert this style's unlocked styles into the unlocked styles map
                        for (name, style) in style.unlocked_styles.iter() {
                            // Should this overwrite? Or skip if already unlocked?
                            unlocked_styles.insert(name.to_owned(), style.to_owned());
                        }

                        scene_styles.push(style);

                        // Anything that isn't a style field should be added to the scene body
                        scene_body.push(Value::StructLiteral(structure_args));
                    }

                    _ => scene_body.push(value),
                }
            }

            // Expressions to Parse
            Token::FloatLiteral(_)
            | Token::BoolLiteral(_)
            | Token::IntLiteral(_)
            | Token::StringLiteral(_)
            | Token::RawStringLiteral(_) => {
                x.index -= 1;

                scene_body.push(create_expression(
                    x,
                    false,
                    ast,
                    &mut DataType::CoerceToString(false),
                    inside_brackets,
                    declarations,
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
                scene_styles.push(Style {
                    format: StyleFormat::Codeblock as i32,
                    parent_override: 10,
                    ..Style::default()
                });
            }

            Token::OpenParenthesis => {
                let structure =
                    new_fixed_collection(x, Value::None, &Vec::new(), ast, declarations)?;

                scene_body.push(Value::StructLiteral(structure));
            }

            Token::Ignore => {
                // Should also clear any styles or tags in the scene
                scene_styles.clear();

                // Keep track of how many scene opens there are
                // This is to make sure the scene close is at the correct place
                let mut extra_scene_opens = 1;
                while x.index < x.length {
                    match x.current_token() {
                        Token::SceneClose => {
                            extra_scene_opens -= 1;
                            if extra_scene_opens == 0 {
                                x.index += 1; // Skip the closing scene close
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
                    x.index += 1;
                }

                return Ok(Value::None);
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

    // look through everything that can be added to the scene body
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
                let nested_scene = new_scene(x, ast, declarations, unlocked_styles)?;

                scene_body.push(nested_scene);
            }

            Token::RawStringLiteral(content) | Token::StringLiteral(content) => {
                scene_body.push(Value::String(content.to_string()));
            }

            // For templating values in scene heads in the body of scenes
            // Token::EmptyScene(spaces) => {
            //     scene_body.push(AstNode::SceneTemplate);
            //     for _ in 0..*spaces {
            //         scene_body.push(AstNode::Spaces(token_line_number));
            //     }
            // }
            Token::Newline => {
                scene_body.push(Value::String("\n".to_string()));
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

        x.index += 1;
    }

    Ok(Value::Scene(scene_body, scene_styles, scene_id))
}
