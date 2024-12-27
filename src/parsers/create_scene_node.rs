use colour::yellow_ln;
use super::{
    ast_nodes::{AstNode, Arg},
    expressions::parse_expression::create_expression,
    styles::{Action, Style, Tag},
    util::{count_newlines_at_end_of_string, count_newlines_at_start_of_string},
};
use crate::{bs_types::DataType, CompileError, Token};
use crate::parsers::ast_nodes::Value;

// Recursive function to parse scenes
pub fn new_scene(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Arg>,
) -> Result<Value, CompileError> {
    let mut scene: Vec<AstNode> = Vec::new();
    *i += 1;

    let mut scene_tags: Vec<Tag> = Vec::new();
    let mut scene_styles: Vec<Style> = Vec::new();
    let scene_actions: Vec<Action> = Vec::new();
    let mut merge_next_p_line: bool = true;

    // Look at all the possible properties that can be added to the scene head
    while *i < tokens.len() {
        let token = &tokens[*i];
        let inside_brackets = token == &Token::OpenParenthesis;
        *i += 1;

        match token {
            Token::Colon => {
                break;
            }

            Token::SceneClose(spaces) => {
                for _ in 0..*spaces {
                    scene.push(AstNode::Space(token_line_numbers[*i]));
                }
                *i -= 1;
                return Ok(Value::Scene(scene, scene_tags, scene_styles, scene_actions));
            }

            Token::Id => {
                // ID can accept multiple arguments, first arg must be unique (regular ID)
                // Remaining args are sort of like classes to group together elements
                // Currently the ID can be a tuple of any type
                scene_tags.push(Tag::Id(create_expression(
                    tokens,
                    i,
                    false,
                    ast,
                    &mut DataType::Tuple(Vec::new()),
                    true,
                    variable_declarations,
                    token_line_numbers,
                )?));
            }

            Token::A => {
                // Inside brackets is set to true for these
                // So it will enforce the parenthesis syntax in create_expression
                scene_tags.push(Tag::A(create_expression(
                    tokens,
                    i,
                    false,
                    ast,
                    &mut DataType::CoerceToString,
                    true,
                    variable_declarations,
                    token_line_numbers,
                )?, token_line_numbers[*i]));
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
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::Margin => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "margin".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: Value::Float(2.0),
                }];

                scene_styles.push(Style::Margin(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            // For positioning inside a flex container / grid
            Token::Order => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "order".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: Value::None,
                }];

                scene_styles.push(Style::Order(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::BG => {
                let required_args: Vec<Arg> = vec![
                    Arg {
                        name: "red".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "green".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "blue".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "alpha".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(1.0),
                    },
                ];

                scene_styles.push(Style::BackgroundColor(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            // Colours
            Token::Rgb | Token::Hsv => {
                let required_args: Vec<Arg> = vec![
                    Arg {
                        name: "red".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "green".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "blue".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(0.0),
                    },
                    Arg {
                        name: "alpha".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: Value::Float(1.0),
                    },
                ];

                let color_type = token.to_owned();
                scene_styles.push(Style::TextColor(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?,
                    color_type,
                    token_line_numbers[*i]
                ));
            }

            Token::Red
            | Token::Green
            | Token::Blue
            | Token::Yellow
            | Token::Cyan
            | Token::Magenta
            | Token::White
            | Token::Black => {
                let color_type = token.to_owned();

                scene_styles.push(Style::TextColor(create_expression(
                    tokens,
                    i,
                    false,
                    ast,
                    &mut DataType::CoerceToString,
                    true,
                    variable_declarations,
                    token_line_numbers,
                )?, color_type, token_line_numbers[*i]));
            }

            Token::Center => {
                scene_styles.push(Style::Center(false, token_line_numbers[*i]));
            }

            Token::Size => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "size".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: Value::Float(1.0),
                }];

                scene_styles.push(Style::Size(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::Blank => {
                scene_styles.push(Style::Blank);
            }

            Token::Hide => {
                scene_styles.push(Style::Hide(token_line_numbers[*i]));
            }

            Token::Table => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "columns".to_string(),
                    data_type: DataType::Int,
                    value: Value::Int(1),
                }];

                scene_tags.push(Tag::Table(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::Img => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "src".to_string(),
                    data_type: DataType::String,
                    value: Value::None,
                }];

                scene_tags.push(Tag::Img(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::Video => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "src".to_string(),
                    data_type: DataType::String,
                    value: Value::None,
                }];

                scene_tags.push(Tag::Video(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::Audio => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "src".to_string(),
                    data_type: DataType::String,
                    value: Value::None,
                }];

                scene_tags.push(Tag::Audio(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
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
                scene.push(AstNode::Literal(create_expression(
                    tokens,
                    &mut *i,
                    false,
                    &ast,
                    &mut DataType::CoerceToString,
                    inside_brackets,
                    variable_declarations,
                    token_line_numbers
                )?, token_line_numbers[*i]));
            }

            Token::Comma => {
                //TODO - decide if this should be enforced as a syntax error or allowed
                yellow_ln!("Warning: Should there be a comma in the scene head? (ignored by compiler)");
            }

            Token::Newline | Token::Empty => {}

            Token::Ignore => {
                // Should also clear any styles or tags in the scene
                scene_styles.clear();
                scene_tags.clear();
                while *i < tokens.len() {
                    match &tokens[*i] {
                        Token::SceneClose(_) | Token::EOF => {
                            break;
                        }
                        _ => {}
                    }
                    *i += 1;
                }

                return Ok(Value::None);
            }

            Token::CodeKeyword => {
                scene_tags.clear();
            }

            Token::CodeBlock(content) => {
                scene_tags.push(Tag::Code(content.to_string(), token_line_numbers[*i]));
            }

            Token::Nav => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "style".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: Value::Float(0.0),
                }];

                scene_tags.push(Tag::Nav(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            Token::Title => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "size".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: Value::Float(0.0),
                }];

                scene_tags.push(Tag::Title(
                    create_expression(
                        tokens,
                        i,
                        false,
                        ast,
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
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
                        &mut DataType::Tuple(required_args.to_owned()),
                        true,
                        variable_declarations,
                        token_line_numbers,
                    )?, token_line_numbers[*i]
                ));
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Token Used Inside scene head when creating scene node. Token: {:?}",
                        token
                    ),
                    line_number: token_line_numbers[*i].to_owned(),
                });
            }
        }
    }

    //look through everything that can be added to the scene body
    while *i < tokens.len() {
        match &tokens[*i] {
            Token::EOF => {
                break;
            }

            Token::SceneClose(spaces) => {
                for _ in 0..*spaces {
                    scene.push(AstNode::Space(token_line_numbers[*i]));
                }
                break;
            }

            Token::SceneHead => {
                let nested_scene =
                    new_scene(tokens, i, ast, token_line_numbers, variable_declarations)?;
                scene.push(AstNode::Literal(nested_scene, token_line_numbers[*i]));
            }

            Token::P(content) => {
                scene.push(if !check_if_inline(tokens, *i, &mut merge_next_p_line) {
                    AstNode::P(content.clone(), token_line_numbers[*i])
                } else {
                    AstNode::Span(content.clone(), token_line_numbers[*i])
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
                scene.push(AstNode::Em(*size, content.clone(), token_line_numbers[*i]));
            }
            Token::Superscript(content) => {
                scene.push(AstNode::Superscript(content.clone(), token_line_numbers[*i]));
            }

            Token::RawStringLiteral(content) => {
                scene.push(AstNode::Span(content.to_string(), token_line_numbers[*i]));
            }

            Token::Pre(content) => {
                scene.push(AstNode::Pre(content.to_string(), token_line_numbers[*i]));
            }

            // For templating values in scene heads in the body of scenes
            Token::EmptyScene(spaces) => {
                scene.push(AstNode::SceneTemplate);
                for _ in 0..*spaces {
                    scene.push(AstNode::Space(token_line_numbers[*i]));
                }
            }

            Token::Newline => {
                scene.push(AstNode::Newline);
            }

            Token::Empty | Token::Colon => {}

            Token::DeadVariable(name) => {
                scene.push(AstNode::Error(
                    format!("Dead Variable used in scene. '{}' was never defined", name),
                    token_line_numbers[*i].to_owned(),
                ));
            }

            _ => {
                scene.push(AstNode::Error(
                    format!(
                        "Invalid Syntax Used Inside scene body when creating scene node: {:?}",
                        tokens[*i]
                    ),
                    token_line_numbers[*i].to_owned(),
                ));
            }
        }

        *i += 1;
    }

    Ok(Value::Scene(scene, scene_tags, scene_styles, scene_actions))
}

fn check_if_inline(tokens: &Vec<Token>, i: usize, merge_next_p_line: &mut bool) -> bool {
    // If the element itself starts with Newlines, it should not be inlined
    let current_element = &tokens[i];
    let p_newlines_to_seperate: usize = if *merge_next_p_line { 2 } else { 1 };
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
            if count_newlines_at_end_of_string(content) >= p_newlines_to_seperate {
                *merge_next_p_line = true;
                false
            } else {
                true
            }
        }

        _ => true,
    }
}
