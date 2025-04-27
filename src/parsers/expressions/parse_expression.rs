#[allow(unused_imports)]
use colour::{grey_ln, red_ln};
use std::collections::HashMap;

use super::eval_expression::evaluate_expression;
//use crate::bs_types::get_any_number_datatype;
// use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::{Operator, Expr};
use crate::parsers::build_ast::TokenContext;
// use crate::parsers::expressions::function_call_inline::inline_function_call;
use crate::parsers::scene::{SceneType, Style};
use crate::parsers::variables::create_new_var_or_ref;
use crate::tokenizer::TokenPosition;
use crate::{
    CompileError, ErrorType, Token,
    bs_types::DataType,
    parsers::{
        ast_nodes::{Arg, AstNode},
        create_scene_node::new_scene,
        structs::new_fixed_collection,
    },
};

// If the datatype is a collection,
// the expression must only contain references to collections
// or collection literals.
pub fn create_expression(
    x: &mut TokenContext,
    inside_collection: bool,
    ast: &[AstNode],
    data_type: &mut DataType,
    inside_parenthesis: bool,
    captured_declarations: &mut Vec<Arg>,
) -> Result<Expr, CompileError> {
    let mut expression: Vec<AstNode> = Vec::new();
    // let mut number_union = get_any_number_datatype(false);

    // Loop through the expression and create the AST nodes
    // Figure out the type it should be from the data
    // DOES NOT MOVE TOKENS PAST THE CLOSING TOKEN
    let mut next_number_negative = false;
    while x.index < x.length {
        let token = x.current_token().to_owned();
        match token {
            // Conditions that close the expression
            Token::CloseParenthesis => {
                if inside_parenthesis {
                    if expression.is_empty() {
                        return Ok(Expr::None);
                    }
                    break;
                } else {
                    x.index += 1;

                    // Mismatched brackets return an error
                    return Err(CompileError {
                        msg: "Mismatched parenthesis in expression".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
            }

            Token::CloseCurly => {
                if inside_collection {
                    break;
                }

                x.index += 1;

                // Mismatched brackets return an error
                return Err(CompileError {
                    msg: "Mismatched curly brackets in expression".to_string(),
                    start_pos: x.current_position(),
                    end_pos: TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            Token::OpenParenthesis => {
                // Move past the open parenthesis before calling this function again
                // Removed this at one point for a test caused a wonderful infinite loop
                x.index += 1;

                let value =
                    create_expression(x, false, ast, data_type, true, captured_declarations)?;
                expression.push(AstNode::Literal(value, x.current_position()));
            }

            Token::OpenCurly => {
                x.index += 1;

                return match data_type {
                    // TODO - do we need to handle unions here? or are they always collapsed into one type before being parsed?
                    DataType::Structure(inner_types) => {
                        // HAS DEFINED INNER TYPES FOR THE struct
                        // could this still result in None if the inner types are defined and not optional?
                        let structure = new_fixed_collection(
                            x,
                            Expr::None,
                            inner_types,
                            ast,
                            captured_declarations,
                        )?;

                        Ok(Expr::StructLiteral(structure))
                    }

                    // If this is inside parenthesis, and we don't know the type.
                    // It must be a struct
                    // This is enforced! If it's a single expression wrapped in parentheses,
                    // it will be flatted into that single value anyway by struct_to_value
                    DataType::Inferred(_) => {
                        // NO DEFINED TYPES FOR THE struct
                        let structure = new_fixed_collection(
                            x,
                            Expr::None,
                            // The difference is this is inferred
                            &Vec::new(),
                            ast,
                            captured_declarations,
                        )?;

                        // And then the type is set here
                        *data_type = DataType::Structure(structure.to_owned());
                        Ok(Expr::StructLiteral(structure))
                    }

                    // Need to error here as a collection literal is being made with wrong explicit type
                    _ => Err(CompileError {
                        msg: format!(
                            "Expected a struct literal, but found a collection literal with type: {:?}",
                            data_type
                        ),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::Type,
                    }),
                };
            }

            Token::EOF | Token::SceneClose | Token::Arrow | Token::Colon | Token::End => {
                if inside_parenthesis {
                    return Err( CompileError {
                        msg: "Not enough closing parenthesis for expression. Need more ')' at the end of the expression".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                if inside_collection {
                    return Err( CompileError {
                        msg: "Not enough closing curly brackets to close the collection. Need more '}' at the end of the collection".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                break;
            }

            Token::Newline => {
                // Fine if inside parenthesis (not closed yet)
                // Otherwise break out of the expression
                if inside_parenthesis {
                    x.index += 1;
                    continue;
                } else {
                    break;
                }
            }

            Token::Comma => {
                // This is just one element inside a struct
                if inside_collection {
                    break;
                }

                x.index += 1;

                // TODO - this is a bit of a mess
                // Are we going to have special rules for return statements and return signatures?
                // So we can just use this function for everything?
                // Or does it make more sense to just use the parse_return_type function for cases without parenthesis?
                // This function already breaks out if there is an End token or Colon token (if not inside parenthesis)

                return Err(CompileError {
                    msg: "Comma found outside of curly brackets (collection): If this is error is for return arguments, this might change in the future".to_string(),
                    start_pos: x.current_position(),
                    end_pos: TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            // Check if the name is a reference to another variable or function call
            Token::Variable(name, is_public) => {
                // This is never reached (I think) if we are inside a struct or collection
                let new_ref = create_new_var_or_ref(
                    x,
                    name.to_owned(),
                    captured_declarations,
                    is_public,
                    ast,
                    false,
                )?;

                match new_ref {
                    AstNode::Literal(ref value, ..) => {
                        expression.push(new_ref);
                    }

                    AstNode::FunctionCall(..) => {
                        expression.push(new_ref);
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Variable '{}' is not a valid reference - it's a: {:?}",
                                name,
                                new_ref.get_type()
                            ),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + name.len() as i32,
                            },
                            error_type: ErrorType::Syntax,
                        });
                    }
                }
            }

            // Check if is a literal
            Token::FloatLiteral(mut float) => {
                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }

                expression.push(AstNode::Literal(
                    Expr::Float(float),
                    x.current_position(),
                ));
            }

            Token::IntLiteral(int) => {
                let int_value = if next_number_negative {
                    next_number_negative = false;
                    -int
                } else {
                    int
                };

                expression.push(AstNode::Literal(
                    Expr::Int(int_value),
                    x.current_position(),
                ));
            }

            Token::StringLiteral(ref string) => {
                expression.push(AstNode::Literal(
                    Expr::String(string.to_owned()),
                    x.current_position(),
                ));
            }

            Token::SceneHead | Token::ParentScene => {
                let scene_type = new_scene(x, ast, captured_declarations, &mut HashMap::new(), Style::default())?;
                match scene_type {
                    SceneType::Scene(scene) => return Ok(scene),

                    // Ignore comments
                    SceneType::Comment => {}

                    // Error for anything else for now
                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Unexpected scene type used in expression: {:?}",
                                scene_type
                            ),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 1,
                            },
                            error_type: ErrorType::Type,
                        });
                    }
                }
            }

            Token::BoolLiteral(value) => {
                expression.push(AstNode::Literal(
                    Expr::Bool(value.to_owned()),
                    x.current_position(),
                ));
            }

            // OPERATORS
            // Will push as a string, so shunting yard can handle it later just as a string
            Token::Negative => {
                next_number_negative = true;
            }

            // Ranges and Loops
            Token::In => {
                // Breaks out of the current expression and changes the type to Range
                *data_type = DataType::Range;
                x.index += 1;
                return evaluate_expression(expression, data_type);
            }

            // BINARY OPERATORS
            Token::Add => {
                expression.push(AstNode::Operator(
                    Operator::Add,
                    x.current_position(),
                ));
            }

            Token::Subtract => {
                expression.push(AstNode::Operator(
                    Operator::Subtract,
                    x.current_position(),
                ));
            }

            Token::Multiply => {
                expression.push(AstNode::Operator(
                    Operator::Multiply,
                    x.current_position(),
                ));
            }

            Token::Divide => {
                expression.push(AstNode::Operator(
                    Operator::Divide,
                    x.current_position(),
                ));
            }

            Token::Exponent => {
                expression.push(AstNode::Operator(
                    Operator::Exponent,
                    x.current_position(),
                ));
            }

            Token::Modulus => {
                expression.push(AstNode::Operator(
                    Operator::Modulus,
                    x.current_position(),
                ));
            }

            // LOGICAL OPERATORS
            Token::Is => {
                // Check if the next token is a not
                if let Some(Token::Not) = x.tokens.get(x.index + 1) {
                    x.index += 1;
                    expression.push(AstNode::Operator(
                        Operator::NotEqual,
                        x.current_position(),
                    ));
                } else {
                    expression.push(AstNode::Operator(
                        Operator::Equality,
                        x.current_position(),
                    ));
                }
            }

            Token::LessThan => {
                expression.push(AstNode::Operator(
                    Operator::LessThan,
                    x.current_position(),
                ));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::Operator(
                    Operator::LessThanOrEqual,
                    x.current_position(),
                ));
            }
            Token::GreaterThan => {
                expression.push(AstNode::Operator(
                    Operator::GreaterThan,
                    x.current_position(),
                ));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::Operator(
                    Operator::GreaterThanOrEqual,
                    x.current_position(),
                ));
            }
            Token::And => {
                expression.push(AstNode::Operator(
                    Operator::And,
                    x.current_position(),
                ));
            }
            Token::Or => {
                expression.push(AstNode::Operator(
                    Operator::Or,
                    x.current_position(),
                ));
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Value used in expression: '{:?}'. Expressions must be assigned with only valid datatypes",
                        token
                    ),
                    start_pos: x.current_position(),
                    end_pos: TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column
                            + token.dimensions().char_column,
                    },
                    error_type: ErrorType::Type,
                });
            }
        }

        x.index += 1;
    }

    evaluate_expression(expression, data_type)
}

pub fn get_accessed_args(
    x: &mut TokenContext,
    collection_name: &String,
    data_type: &DataType,
    accessed_args: &mut Vec<usize>,
) -> Result<Vec<usize>, CompileError> {
    // Check if there is an access
    // Should be at the variable name in the token stream
    if let Some(Token::Dot) = x.tokens.get(x.index + 1) {
        // Move past the dot
        x.index += 2;

        match x.tokens.get(x.index) {
            // INTEGER INDEX ACCESS
            Some(Token::IntLiteral(index)) => {
                // Check this is a valid index
                // Usize will flip to max number if negative
                // Maybe in future negative indexes with be supported (minus from the end)

                // for now just error if it's negative
                if index < &0 {
                    return Err(CompileError {
                        msg: format!(
                            "Can't use negative index: {} to access a collection or struct '{}'",
                            x.index, collection_name
                        ),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                let idx: usize = *index as usize;
                match data_type {
                    DataType::Structure(inner_types) => {
                        if idx >= inner_types.len() {
                            return Err(CompileError {
                                msg: format!(
                                    "Index {} out of range for any arguments in '{}'",
                                    idx, collection_name
                                ),
                                start_pos: x.current_position(),
                                end_pos: TokenPosition {
                                    line_number: x.current_position().line_number,
                                    char_column: x.current_position().char_column + 1,
                                },
                                error_type: ErrorType::Rule,
                            });
                        }

                        accessed_args.push(idx);
                    }

                    DataType::Collection(..) => {
                        accessed_args.push(idx);
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Can't access '{}' with an index as it's a {:?}. Only collections can be accessed with an index",
                                collection_name, data_type
                            ),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 1,
                            },
                            error_type: ErrorType::Rule,
                        });
                    }
                }
            }

            // NAMED ARGUMENT ACCESS
            Some(Token::Variable(name, ..)) => match data_type {
                DataType::Structure(inner_types) => {
                    if let Some(idx) = inner_types.iter().position(|arg| arg.name == *name) {
                        accessed_args.push(idx);
                    } else {
                        return Err(CompileError {
                            msg: format!(
                                "Name '{}' not found in struct '{}'",
                                name, collection_name
                            ),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 1,
                            },
                            error_type: ErrorType::Rule,
                        });
                    }
                }

                _ => {
                    return Err(CompileError {
                        msg: "Compiler only supports named access for structs".to_string(),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::Rule,
                    });
                }
            },

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Expected an index or name to access struct '{}'",
                        collection_name
                    ),
                    start_pos: x.current_position(),
                    end_pos: TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column + 1,
                    },
                    error_type: ErrorType::Rule,
                });
            }
        }

        // Recursively call this function until there are no more accessed args
        return get_accessed_args(x, collection_name, data_type, accessed_args);
    }

    Ok(Vec::new())
}
