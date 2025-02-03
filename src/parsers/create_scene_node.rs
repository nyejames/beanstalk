use super::{
    ast_nodes::{Arg, AstNode},
    expressions::parse_expression::create_expression,
    styles::{Action, Style, Tag},
    util::{count_newlines_at_end_of_string, count_newlines_at_start_of_string},
};
use crate::bs_types::{get_any_number_datatype, get_rgba_args};
use crate::parsers::ast_nodes::Value;
use crate::parsers::structs::new_struct;
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, CompileError, ErrorType, Token};
use colour::yellow_ln;

// Recursive function to parse scenes
pub fn new_scene(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_positions: &Vec<TokenPosition>,
    variable_declarations: &mut Vec<Arg>,
) -> Result<Value, CompileError> {
    let mut scene: Vec<AstNode> = Vec::new();
    *i += 1;

    let mut scene_tags: Vec<Tag> = Vec::new();
    let mut scene_styles: Vec<Style> = Vec::new();
    let scene_actions: Vec<Action> = Vec::new();
    let mut merge_next_p_line: bool = true;

    // SCENE HEAD PARSING
    while *i < tokens.len() {
        let token = &tokens[*i];

        let inside_brackets = token == &Token::OpenParenthesis;

        *i += 1;

        // red_ln!("token being parsed for AST: {:?}", token);

        match token {
            Token::Colon => {
                break;
            }

            Token::SceneClose(spaces) => {
                if spaces > &0 {
                    scene.push(AstNode::Space(*spaces));
                }

                *i -= 1;
                return Ok(Value::Scene(scene, scene_tags, scene_styles, scene_actions));
            }

            // TODO - all of these 'styles' need to become structs rather than function calls
            // If they have functions they use, those should be methods accessed on those structs
            // Is this going to be done in an HTML styles standard lib? (they get parsed as variables etc)
            Token::Id => {
                // ID can accept multiple arguments, first arg must be unique (regular ID)
                // Remaining args are sort of like classes to group together elements
                // Currently the ID can be a struct of any type
                scene_tags.push(Tag::Id(create_expression(
                    tokens,
                    i,
                    false,
                    ast,
                    &mut DataType::Collection(Box::new(DataType::String)),
                    true,
                    variable_declarations,
                    token_positions,
                )?));
            }

            Token::Link => {
                // Inside brackets is set to true for these
                // So it will enforce the parenthesis syntax in create_expression
                scene_tags.push(Tag::A(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::CoerceToString,
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Padding => {
                let required_args: Vec<Arg> = vec![
                    Arg {
                        name: "all".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(1.5),
                    },
                    Arg {
                        name: "top".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "right".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(1.5),
                    },
                    Arg {
                        name: "bottom".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "left".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(1.5),
                    },
                ];

                scene_styles.push(Style::Padding(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Structure(required_args),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Margin => {
                scene_styles.push(Style::Margin(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut get_any_number_datatype(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            // For positioning inside a flex container / grid
            Token::Order => {
                scene_styles.push(Style::Order(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut get_any_number_datatype(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::BG => {
                scene_styles.push(Style::BackgroundColor(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut get_rgba_args(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            // Colours
            Token::Rgb => {
                let color_type = token.to_owned();
                scene_styles.push(Style::TextColor(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut get_rgba_args(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    color_type,
                    token_positions[*i].to_owned(),
                ));
            }

            // TODO - HSL and HSV
            // Token::Hsv | Token::Hsl => {}
            Token::Red
            | Token::Green
            | Token::Blue
            | Token::Yellow
            | Token::Cyan
            | Token::Magenta
            | Token::White
            | Token::Black => {
                let color_type = token.to_owned();

                scene_styles.push(Style::TextColor(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::CoerceToString,
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    color_type,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Center => {
                scene_styles.push(Style::Center(false, token_positions[*i].to_owned()));
            }

            Token::Size => {
                scene_styles.push(Style::Size(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut get_any_number_datatype(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Blank => {
                scene_styles.push(Style::Blank);
            }

            Token::Hide => {
                scene_styles.push(Style::Hide(token_positions[*i].to_owned()));
            }

            Token::Table => {
                scene_tags.push(Tag::Table(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Int,
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Img => {
                scene_tags.push(Tag::Img(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::CoerceToString,
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Video => {
                scene_tags.push(Tag::Video(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::CoerceToString,
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Audio => {
                scene_tags.push(Tag::Audio(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::CoerceToString,
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            // Expressions to Parse
            Token::Variable(_)
            | Token::FloatLiteral(_)
            | Token::BoolLiteral(_)
            | Token::IntLiteral(_)
            | Token::StringLiteral(_)
            | Token::RawStringLiteral(_) => {
                *i -= 1;

                scene.push(AstNode::Literal(
                    create_expression(
                        tokens,
                        &mut *i,
                        false,
                        &ast,
                        &mut DataType::CoerceToString,
                        inside_brackets,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
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

            // Completely skips parsing this whole scene and returns empty.
            // This is useful for comments / prototypes inside of other scenes
            Token::Ignore => {
                // Should also clear any styles or tags in the scene
                scene_styles.clear();
                scene_tags.clear();

                // Keep track of how many scene opens there are
                // This is to make sure the scene close is at the correct place
                let mut extra_scene_opens = 1;
                while *i < tokens.len() {
                    match &tokens[*i] {
                        Token::SceneClose(_) => {
                            extra_scene_opens -= 1;
                            if extra_scene_opens == 0 {
                                *i += 1; // Skip the closing scene close
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
                    *i += 1;
                }

                return Ok(Value::None);
            }

            // TODO - Honestly not sure if this is still needed?
            Token::CodeKeyword => {
                scene_tags.clear();
            }

            Token::CodeBlock(content) => {
                scene_tags.push(Tag::Code(
                    content.to_string(),
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Nav => {
                scene_tags.push(Tag::Nav(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        // Maybe accept more arguments in the future for more control over nav styles
                        // For now this is just a number that selects a predetermined style
                        &mut get_any_number_datatype(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Title => {
                scene_tags.push(Tag::Title(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut get_any_number_datatype(),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::Main => {
                scene_tags.push(Tag::Main);
            }
            Token::Header => {
                scene_tags.push(Tag::Header);
            }
            Token::Footer => {
                scene_tags.push(Tag::Footer);
            }
            Token::Section => {
                scene_tags.push(Tag::Section);
            }

            Token::Redirect => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "href".to_string(),
                    data_type: DataType::String,
                    value: Value::None,
                }];

                scene_tags.push(Tag::Redirect(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Structure(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_positions,
                    )?,
                    token_positions[*i].to_owned(),
                ));
            }

            Token::OpenParenthesis => {
                let structure = new_struct(
                    Value::None,
                    tokens,
                    &mut *i,
                    &Vec::new(),
                    ast,
                    variable_declarations,
                    token_positions,
                )?;

                scene.push(AstNode::Literal(
                    Value::Structure(structure),
                    token_positions[*i].to_owned(),
                ));
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Token Used Inside scene head when creating scene node. Token: {:?}",
                        token
                    ),
                    start_pos: token_positions[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }
    }

    //look through everything that can be added to the scene body
    while *i < tokens.len() {
        let token_line_number = token_positions[*i].line_number;
        let token_char_column = token_positions[*i].char_column;

        match &tokens[*i] {
            Token::EOF => {
                break;
            }

            Token::SceneClose(spaces) => {
                for _ in 0..*spaces {
                    scene.push(AstNode::Space(token_line_number));
                }
                break;
            }

            Token::SceneHead => {
                let nested_scene =
                    new_scene(tokens, i, ast, token_positions, variable_declarations)?;
                scene.push(AstNode::Literal(
                    nested_scene,
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column,
                    },
                ));
            }

            Token::P(content) => {
                scene.push(if !check_if_inline(tokens, *i, &mut merge_next_p_line) {
                    AstNode::P(
                        content.clone(),
                        TokenPosition {
                            line_number: token_line_number,
                            char_column: token_char_column,
                        },
                    )
                } else {
                    AstNode::Span(
                        content.clone(),
                        TokenPosition {
                            line_number: token_line_number,
                            char_column: token_char_column,
                        },
                    )
                });
            }

            // Special Markdown Syntax Elements
            Token::HeadingStart(size) => {
                merge_next_p_line = false;
                scene.push(AstNode::Heading(*size));
            }

            Token::BulletPointStart(size) => {
                merge_next_p_line = false;
                scene.push(AstNode::BulletPoint(*size));
            }

            Token::Em(size, content) => {
                scene.push(AstNode::Em(
                    *size,
                    content.clone(),
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column,
                    },
                ));
            }

            Token::Superscript(content) => {
                scene.push(AstNode::Superscript(
                    content.clone(),
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column,
                    },
                ));
            }

            Token::RawStringLiteral(content) => {
                scene.push(AstNode::Span(
                    content.to_string(),
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column,
                    },
                ));
            }

            Token::Pre(content) => {
                scene.push(AstNode::Pre(
                    content.to_string(),
                    TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column,
                    },
                ));
            }

            // For templating values in scene heads in the body of scenes
            Token::EmptyScene(spaces) => {
                scene.push(AstNode::SceneTemplate);
                for _ in 0..*spaces {
                    scene.push(AstNode::Space(token_line_number));
                }
            }

            Token::Newline => {
                scene.push(AstNode::Newline);
            }

            Token::Empty | Token::Colon => {}

            Token::DeadVariable(name) => {
                return Err(CompileError {
                    msg: format!("Dead Variable used in scene. '{}' was never defined", name),
                    start_pos: token_positions[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column + name.len() as u32,
                    },
                    error_type: ErrorType::Caution,
                });
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Syntax Used Inside scene body when creating scene node: {:?}",
                        tokens[*i]
                    ),
                    start_pos: token_positions[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_line_number,
                        char_column: token_char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }
        }

        *i += 1;
    }

    Ok(Value::Scene(scene, scene_tags, scene_styles, scene_actions))
}

fn check_if_inline(tokens: &Vec<Token>, i: usize, merge_next_p_line: &mut bool) -> bool {
    // If the element itself starts with Newlines, it should not be inlined
    let current_element = &tokens[i];
    let p_newlines_to_separate: usize = if *merge_next_p_line { 2 } else { 1 };
    match current_element {
        Token::P(content) => {
            if count_newlines_at_start_of_string(content) > 0 {
                return false;
            }
        }
        _ => {}
    }

    // Iterate back through tokens to find the last token that isn't Initialise, SceneHead or SceneClose
    let mut previous_element = &Token::Empty;
    let mut j = i - 1;
    while j > 0 {
        match &tokens[j] {
            // Ignore these tokens, keep searching back
            Token::Colon | Token::SceneClose(_) | Token::SceneHead => {
                j -= 1;
            }

            // Can't go any further back
            Token::ParentScene => {
                return false;
            }

            _ => {
                previous_element = &tokens[j];
                break;
            }
        }
    }

    // If the current element is the same as the previous element
    // It doesn't have 2 newlines ending. It can also be inlined
    // Then return true
    match previous_element {
        Token::Empty | Token::Newline | Token::Pre(_) => false,

        Token::P(content)
        | Token::Span(content)
        | Token::Em(_, content)
        | Token::Superscript(content) => {
            if count_newlines_at_end_of_string(content) >= p_newlines_to_separate {
                *merge_next_p_line = true;
                false
            } else {
                true
            }
        }

        _ => true,
    }
}
