use colour::yellow_ln;
use super::{
    ast_nodes::{AstNode, Arg},
    expressions::parse_expression::create_expression,
    styles::{Action, Style, Tag},
    util::{count_newlines_at_end_of_string, count_newlines_at_start_of_string},
};
use crate::{bs_types::DataType, CompileError, Token};
use crate::parsers::tuples::{create_node_from_tuple, new_tuple};

// Recursive function to parse scenes
pub fn new_scene(
    tokens: &Vec<Token>,
    i: &mut usize,
    ast: &Vec<AstNode>,
    token_line_numbers: &Vec<u32>,
    variable_declarations: &mut Vec<Arg>,
) -> Result<AstNode, CompileError> {
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
                return Ok(AstNode::Scene(scene, scene_tags, scene_styles, scene_actions, token_line_numbers[*i]));
            }

            Token::Id => {
                let eval_arg = new_tuple(
                    None,
                    tokens,
                    i,
                    &Vec::new(),
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                // ID can accept multiple arguments, first arg must be unique (regular ID)
                // Remaining args are sort of like classes to group together elements
                scene_tags.push(Tag::Id(eval_arg));
            }

            Token::A => {
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &Vec::from([Arg {
                        name: "href".to_string(),
                        data_type: DataType::String,
                        value: AstNode::Empty(token_line_numbers[*i]),
                    }]),
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_tags.push(Tag::A(eval_arg));
            }

            Token::Padding => {
                let required_args: Vec<Arg> = vec![
                    Arg {
                        name: "all".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(1.5), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "top".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "right".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(1.5), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "bottom".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "left".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(1.5), token_line_numbers[*i]),
                    },
                ];
                
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::Padding(eval_arg));
            }

            Token::Margin => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "margin".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Literal(Token::FloatLiteral(2.0), token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::Margin(eval_arg));
            }

            // For positioning inside a flex container / grid
            Token::Order => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "order".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Empty(token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::Order(eval_arg));
            }

            Token::BG => {
                let required_args: Vec<Arg> = vec![
                    Arg {
                        name: "red".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Empty(token_line_numbers[*i]),
                    },
                    Arg {
                        name: "green".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Empty(token_line_numbers[*i]),
                    },
                    Arg {
                        name: "blue".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Empty(token_line_numbers[*i]),
                    },
                    Arg {
                        name: "alpha".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(1.0), token_line_numbers[*i]),
                    },
                ];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::BackgroundColor(eval_arg));
            }

            // Colours
            Token::Rgb | Token::Hsv => {
                let required_args: Vec<Arg> = vec![
                    Arg {
                        name: "red".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "green".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "blue".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                    },
                    Arg {
                        name: "alpha".to_string(),
                        data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                        value: AstNode::Literal(Token::FloatLiteral(1.0), token_line_numbers[*i]),
                    },
                ];

                let color_type = token.to_owned();
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::TextColor(eval_arg, color_type));
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
                let required_args: Vec<Arg> = vec![Arg {
                    name: "shade".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Literal(Token::FloatLiteral(1.0), token_line_numbers[*i]),
                }];
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::TextColor(eval_arg, color_type));
            }

            Token::Center => {
                scene_styles.push(Style::Center(false));
            }

            Token::Size => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "size".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Literal(Token::FloatLiteral(1.0), token_line_numbers[*i]),
                }];
                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_styles.push(Style::Size(eval_arg));
            }

            Token::Blank => {
                scene_styles.push(Style::Blank);
            }

            Token::Hide => {
                scene_styles.push(Style::Hide);
            }

            Token::Table => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "columns".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Literal(Token::FloatLiteral(1.0), token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;

                // Just doing this comp time only for now
                // TODO - make sure this can work at runtime
                match eval_arg {
                    AstNode::Literal(literal_token, _) => match literal_token {
                        Token::FloatLiteral(value) => {
                            scene_tags.push(Tag::Table(value as u32));
                        }
                        Token::IntLiteral(value) => {
                            scene_tags.push(Tag::Table(value as u32));
                        }
                        _ => {
                            return Err(CompileError {
                                msg: "Incorrect arguments passed into table declaration".to_string(),
                                line_number: token_line_numbers[*i].to_owned(),
                            });
                        }
                    },
                    _ => {
                        return Err(CompileError {
                            msg: "Table must have a literal that can be evaluated at compile time (currently)".to_string(),
                            line_number: token_line_numbers[*i].to_owned(),
                        });
                    }
                }
            }

            Token::Img => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "src".to_string(),
                    data_type: DataType::String,
                    value: AstNode::Empty(token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_tags.push(Tag::Img(eval_arg));
            }

            Token::Video => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "src".to_string(),
                    data_type: DataType::String,
                    value: AstNode::Empty(token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    &mut *i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_tags.push(Tag::Video(eval_arg));
            }

            Token::Audio => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "src".to_string(),
                    data_type: DataType::String,
                    value: AstNode::Empty(token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_tags.push(Tag::Audio(eval_arg));
            }

            // Expressions to Parse
            Token::Variable(_)
            | Token::FloatLiteral(_)
            | Token::BoolLiteral(_)
            | Token::IntLiteral(_)
            | Token::StringLiteral(_)
            | Token::RawStringLiteral(_) => {
                *i -= 1;
                scene.push(create_expression(
                    tokens,
                    &mut *i,
                    false,
                    &ast,
                    &mut DataType::CoerseToString,
                    inside_brackets,
                    variable_declarations,
                    token_line_numbers
                )?);
            }

            Token::Comma => {
                //TODO - decide if this should be enforced as a syntax error or allowed
                yellow_ln!("Warning: Should there be a comma in the scene head? (ignored by compiler)");
            }

            Token::Newline | Token::Empty => {}

            Token::Ignore => {
                // Just create a comment
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

                return Ok(AstNode::Comment("Ignored Scene".to_string()));
            }

            Token::CodeKeyword => {
                scene_tags.clear();
            }

            Token::CodeBlock(content) => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "language".to_string(),
                    data_type: DataType::String,
                    value: AstNode::Literal(Token::StringLiteral("bs".to_string()), token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;

                match eval_arg {
                    AstNode::Literal(Token::StringLiteral(lang), line_number) => {
                        scene.push(AstNode::CodeBlock(content.to_owned(), lang, line_number.to_owned()));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Code block must have a literal that can be evaluated at compile time (currently)".to_string(),
                            line_number: token_line_numbers[*i].to_owned(),
                        });
                    }
                };
            }

            Token::Nav => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "style".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;

                match eval_arg {
                    AstNode::Literal(Token::FloatLiteral(value), _) => {
                        scene_tags.push(Tag::Nav(value));
                    }
                    AstNode::Literal(Token::IntLiteral(value), _) => {
                        scene_tags.push(Tag::Nav(value as f64));
                    }
                    _ => {
                        return Err(CompileError {
                            msg: "Nav must have a literal that can be evaluated at compile time (currently)".to_string(),
                            line_number: token_line_numbers[*i].to_owned(),
                        });
                    }
                };
            }

            Token::Title => {
                let required_args: Vec<Arg> = vec![Arg {
                    name: "size".to_string(),
                    data_type: DataType::Union(vec![DataType::Float, DataType::Int]),
                    value: AstNode::Literal(Token::FloatLiteral(0.0), token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    &mut *i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_tags.push(Tag::Title(eval_arg));
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
                    value: AstNode::Empty(token_line_numbers[*i]),
                }];

                let tuple = new_tuple(
                    None,
                    tokens,
                    i,
                    &required_args,
                    ast,
                    variable_declarations,
                    token_line_numbers,
                )?;

                let eval_arg = create_node_from_tuple(tuple, token_line_numbers[*i])?;
                scene_tags.push(Tag::Redirect(eval_arg));
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
                scene.push(nested_scene);
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

            Token::DeadVarible(name) => {
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

    Ok(AstNode::Scene(scene, scene_tags, scene_styles, scene_actions, token_line_numbers[*i]))
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

    // Iterate back through tokens to find the last token that isn't Initialise, Scenehead or Sceneclose
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
