#[allow(unused_imports)]
use colour::{grey_ln, red_ln};

use super::eval_expression::evaluate_expression;
use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::parsers::structs::struct_to_value;
use crate::parsers::variables::create_new_var_or_ref;
use crate::tokenizer::TokenPosition;
use crate::{
    bs_types::DataType,
    parsers::{
        ast_nodes::{Arg, AstNode},
        create_scene_node::new_scene,
        structs::new_struct,
    },
    CompileError, ErrorType, Token,
};
use crate::tokens::Length;

// If the datatype is a collection
// The expression must only contain references to collections
// Or collection literals
pub fn create_expression(
    tokens: &Vec<Token>,
    i: &mut usize,
    inside_struct: bool,
    ast: &Vec<AstNode>,
    mut data_type: &mut DataType,
    inside_parenthesis: bool,
    variable_declarations: &mut Vec<Arg>,
    token_positions: &Vec<TokenPosition>,
) -> Result<Value, CompileError> {
    let mut expression = Vec::new();
    let mut number_union = DataType::Union(vec![DataType::Int, DataType::Float]);

    // grey_ln!("Parsing expression of type: {:?}", data_type);
    // grey_ln!("token: {:?}", tokens[*i]);

    // Inside brackets if set to true, means that there is expected to be a struct or collection
    // Or this is currently inside a struct (in which case this first part of data type checking is skipped unless the type needs to be inferred)
    // This first check is to see if a new struct is being created
    // And whether brackets are expected
    if inside_parenthesis {
        match data_type {
            // TODO - do we need to handle unions here? or are they always collapsed into one type before being parsed?
            DataType::Structure(inner_types) => {
                // HAS DEFINED INNER TYPES FOR THE struct
                // could this still result in None if the inner types are defined and not optional?
                let structure = new_struct(
                    Value::None,
                    tokens,
                    &mut *i,
                    inner_types,
                    ast,
                    variable_declarations,
                    token_positions,
                )?;

                return Ok(struct_to_value(&structure));
            }

            // If this is inside of parenthesis, and we don't know the type.
            // It must be a struct
            // This is enforced! If it's a single expression wrapped in parentheses,
            // it will be flatted into that single value anyway by struct_to_value
            DataType::Inferred => {
                // NO DEFINED TYPES FOR THE struct
                let structure = new_struct(
                    Value::None,
                    tokens,
                    &mut *i,
                    &Vec::new(),
                    ast,
                    variable_declarations,
                    token_positions,
                )?;

                *data_type = DataType::Structure(structure.to_owned());
                return Ok(struct_to_value(&structure));
            }

            // There must be at least 1 unclosed bracket that this element is inside
            _ => {}
        }
    }

    // Loop through the expression and create the AST nodes
    // Figure out the type it should be from the data
    // DOES NOT MOVE TOKENS PAST THE CLOSING TOKEN
    let mut next_number_negative = false;
    while let Some(token) = tokens.get(*i) {
        // red_ln!("current token in parse_expression: {:?}", token);

        match token {
            // Conditions that close the expression
            Token::CloseParenthesis => {
                if inside_parenthesis {
                    *i += 1;
                    if expression.is_empty() {
                        return Ok(Value::None);
                    }
                    break;
                } else {
                    if inside_struct {
                        break;
                    }

                    *i += 1;

                    // Mismatched brackets, return an error
                    return Err(CompileError {
                        msg: "Mismatched brackets in expression".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
            }

            Token::OpenParenthesis => {
                // Move past the open parenthesis before calling this function again
                // Removing this at one point for a test caused a wonderful infinite loop
                *i += 1;

                return create_expression(
                    tokens,
                    &mut *i,
                    false,
                    ast,
                    &mut data_type,
                    true,
                    variable_declarations,
                    token_positions,
                );
            }

            Token::EOF | Token::SceneClose(_) | Token::Arrow | Token::Colon | Token::End => {
                if inside_parenthesis {
                    return Err( CompileError {
                        msg: "Not enough closing parenthesis for expression. Need more ')' at the end of the expression!".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                break;
            }

            Token::Newline => {
                // Fine if inside of parenthesis (not closed yet)
                // Otherwise break out of the expression
                if inside_parenthesis {
                    *i += 1;
                    continue;
                } else {
                    break;
                }
            }

            Token::Comma => {
                // This is just one element inside a struct
                if inside_struct {
                    break;
                }

                *i += 1;

                // First time inferring that this is actually a struct type
                if inside_parenthesis {
                    let eval_first_expr = evaluate_expression(expression, &mut data_type)?;

                    let structure = new_struct(
                        eval_first_expr,
                        tokens,
                        i,
                        &Vec::new(),
                        ast,
                        variable_declarations,
                        token_positions,
                    )?;

                    return Ok(Value::Structure(structure));
                }

                // TODO - this is a bit of a mess
                // Are we going to have special rules for return statements and return signatures?
                // So we can just use this function for everything?
                // Or does it make more sense to just use the parse_return_type function for cases without parenthesis?
                // This function already breaks out if there is an End token or Colon token (if not inside parenthesis)

                return Err(CompileError {
                    msg: "Comma found outside of parenthesis: If this is error is for return arguments, this might change in the future".to_string(),
                    start_pos: token_positions[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column + 1,
                    },
                    error_type: ErrorType::Syntax,
                });
            }

            // Check if name is a reference to another variable or function call
            Token::Variable(name) => {
                // This is never reached (I think) if we are inside a struct or collection
                let new_ref = create_new_var_or_ref(
                    name,
                    variable_declarations,
                    tokens,
                    &mut *i,
                    false,
                    ast,
                    token_positions,
                    false,
                )?;

                // red_ln!("new ref: {:?}", new_ref);

                match new_ref {
                    AstNode::Literal(ref value, ..) => {
                        // Check type is correct
                        let reference_data_type = value.get_type();
                        if !check_if_valid_type(&reference_data_type, &mut data_type) {
                            return Err(CompileError {
                                msg: format!(
                                    "Variable '{}' is of type {:?}, but used in an expression of type {:?}",
                                    name, reference_data_type, data_type
                                ),
                                start_pos: token_positions[*i].to_owned(),
                                end_pos: TokenPosition {
                                    line_number: token_positions[*i].line_number,
                                    char_column: token_positions[*i].char_column + 1,
                                },
                                error_type: ErrorType::TypeError,
                            });
                        }

                        expression.push(new_ref);
                    }

                    // Must be evaluated at runtime
                    AstNode::FunctionCall(..) => {
                        // Check type is correct
                        let reference_data_type = new_ref.get_type();
                        if !check_if_valid_type(&reference_data_type, &mut data_type) {
                            return Err(CompileError {
                                msg: format!(
                                    "Function call '{}' is of type {:?}, but used in an expression of type {:?}",
                                    name, reference_data_type, data_type
                                ),
                                start_pos: token_positions[*i].to_owned(),
                                end_pos: TokenPosition {
                                    line_number: token_positions[*i].line_number,
                                    char_column: token_positions[*i].char_column + name.len() as u32,
                                },
                                error_type: ErrorType::TypeError,
                            });
                        }
                        expression.push(new_ref);
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Variable '{}' is not a valid reference - it's a: {:?}",
                                name,
                                new_ref.get_type()
                            ),
                            start_pos: token_positions[*i].to_owned(),
                            end_pos: TokenPosition {
                                line_number: token_positions[*i].line_number,
                                char_column: token_positions[*i].char_column + name.len() as u32,
                            },
                            error_type: ErrorType::Syntax,
                        });
                    }
                }
            }

            // Check if is a literal
            Token::FloatLiteral(mut float) => {
                if !check_if_valid_type(&DataType::Float, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("Float literal used in expression of type: {:?}", data_type),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column
                                + float.to_string().len() as u32,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }

                expression.push(AstNode::Literal(
                    Value::Float(float),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::IntLiteral(int) => {
                if !check_if_valid_type(&DataType::Int, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("Int literal used in expression of type: {:?}", data_type),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column
                                + int.to_string().len() as u32,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                if next_number_negative {
                    expression.push(AstNode::Literal(
                        Value::Int(-(*int)),
                        TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column,
                        },
                    ));
                    next_number_negative = false;
                }

                expression.push(AstNode::Literal(
                    Value::Int(*int),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::StringLiteral(string) => {
                if !check_if_valid_type(&DataType::String, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("String literal used in expression of type: {:?}", data_type),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + token.length(),
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                expression.push(AstNode::Literal(
                    Value::String(string.clone()),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            // Scenes - Create a new scene node
            // Maybe scenes can be added together like strings
            Token::SceneHead | Token::ParentScene => {
                if !check_if_valid_type(&DataType::Scene, &mut data_type) {
                    return Err(CompileError {
                        msg: format!("Scene literal used in expression of type: {:?}", data_type),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + token.length(),
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                return new_scene(tokens, i, &ast, token_positions, variable_declarations);
            }
            
            Token::BoolLiteral(value) => {
                expression.push(AstNode::Literal(
                    Value::Bool(value.to_owned()),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            // OPERATORS
            // Will push as a string so shunting yard can handle it later just as a string
            Token::Negative => {
                next_number_negative = true;
            }

            // BINARY OPERATORS
            Token::Add => {
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::Subtract => {
                if !check_if_valid_type(&data_type, &mut number_union) {
                    return Err(CompileError {
                        msg: format!(
                            "Subtraction can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::Multiply => {
                if !check_if_valid_type(&number_union, &mut data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Multiplication can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::Divide => {
                if !check_if_valid_type(&number_union, &mut data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Division can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::Modulus => {
                if !check_if_valid_type(&number_union, &mut data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Modulus can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            // LOGICAL OPERATORS
            Token::Equal => {
                expression.push(AstNode::LogicalOperator(
                    Token::Equal,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            Token::LessThan => {
                expression.push(AstNode::LogicalOperator(
                    Token::LessThan,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::LogicalOperator(
                    Token::LessThanOrEqual,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }
            Token::GreaterThan => {
                expression.push(AstNode::LogicalOperator(
                    Token::GreaterThan,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::LogicalOperator(
                    Token::GreaterThanOrEqual,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }
            Token::And => {
                expression.push(AstNode::LogicalOperator(
                    Token::And,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }
            Token::Or => {
                expression.push(AstNode::LogicalOperator(
                    Token::Or,
                    TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column,
                    },
                ));
            }

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Value used in expression: '{:?}'. Expressions must be assigned with only valid datatypes",
                        token
                    ),
                    start_pos: token_positions[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column + token.length(),
                    },
                    error_type: ErrorType::TypeError,
                });
            }
        }

        *i += 1;
    }

    evaluate_expression(expression, data_type)
}

fn check_if_valid_type(data_type: &DataType, accepted_type: &mut DataType) -> bool {
    // Has to make sure if either type is a union, that the other type is also a member of the union
    // red_ln!("checking if: {:?} is accepted by: {:?}", data_type, accepted_type);

    match data_type {
        DataType::Union(ref types) => {
            for t in types {
                if check_if_valid_type(t, accepted_type) {
                    return true;
                }
            }
            return false;
        }
        _ => {}
    }

    match accepted_type {
        DataType::Inferred => {
            *accepted_type = data_type.to_owned();
            true
        }
        DataType::CoerceToString => true,
        DataType::Union(ref types) => {
            for t in types {
                if data_type == t {
                    return true;
                }
            }
            false
        }
        _ => {
            if data_type == accepted_type {
                true
            } else {
                false
            }
        }
    }
}

pub fn get_accessed_args(
    collection_name: &String,
    tokens: &Vec<Token>,
    i: &mut usize,
    data_type: &DataType,
    token_positions: &Vec<TokenPosition>,
    accessed_args: &mut Vec<usize>,
) -> Result<Vec<usize>, CompileError> {
    // Check if there is an access
    // Should be at the variable name in the token stream
    if let Some(Token::Dot) = tokens.get(*i + 1) {
        // Move past the dot
        *i += 2;

        match tokens.get(*i) {
            // INTEGER INDEX ACCESS
            Some(Token::IntLiteral(index)) => {
                // Check this is a valid index
                // Usize will flip to max number if negative
                // Maybe in future negative indexes with be supported (minus from the end)

                // for now just error if it's negative
                if *index < 0 {
                    return Err(CompileError {
                        msg: format!(
                            "Can't use negative index: {} to access a collection or struct '{}'",
                            *index, collection_name
                        ),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Rule,
                    });
                }

                let idx: usize = *index as usize;
                match data_type {
                    DataType::Structure(ref inner_types) => {
                        if idx >= inner_types.len() {
                            return Err(CompileError {
                                msg: format!(
                                    "Index {} out of range for any arguments in '{}'",
                                    idx, collection_name
                                ),
                                start_pos: token_positions[*i].to_owned(),
                                end_pos: TokenPosition {
                                    line_number: token_positions[*i].line_number,
                                    char_column: token_positions[*i].char_column + 1,
                                },
                                error_type: ErrorType::Rule
                            });
                        }

                        accessed_args.push(idx);
                    }

                    DataType::Collection(_) => {
                        accessed_args.push(idx);
                    }

                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Can't access '{}' with an index as it's a {:?}. Only collections can be accessed with an index",
                                collection_name,
                                data_type
                            ),
                            start_pos: token_positions[*i].to_owned(),
                            end_pos: TokenPosition {
                                line_number: token_positions[*i].line_number,
                                char_column: token_positions[*i].char_column + 1,
                            },
                            error_type: ErrorType::Rule
                        })
                    }
                }
            }

            // NAMED ARGUMENT ACCESS
            Some(Token::Variable(name)) => match data_type {
                DataType::Structure(ref inner_types) => {
                    if let Some(idx) = inner_types.iter().position(|arg| arg.name == *name) {
                        accessed_args.push(idx);
                    } else {
                        return Err(CompileError {
                            msg: format!(
                                "Name '{}' not found in struct '{}'",
                                name, collection_name
                            ),
                            start_pos: token_positions[*i].to_owned(),
                            end_pos: TokenPosition {
                                line_number: token_positions[*i].line_number,
                                char_column: token_positions[*i].char_column + 1,
                            },
                            error_type: ErrorType::Rule,
                        });
                    }
                }

                _ => {
                    return Err(CompileError {
                        msg: "Compiler only supports named access for structs".to_string(),
                        start_pos: token_positions[*i].to_owned(),
                        end_pos: TokenPosition {
                            line_number: token_positions[*i].line_number,
                            char_column: token_positions[*i].char_column + 1,
                        },
                        error_type: ErrorType::Rule,
                    })
                }
            },

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Expected an index or name to access struct '{}'",
                        collection_name
                    ),
                    start_pos: token_positions[*i].to_owned(),
                    end_pos: TokenPosition {
                        line_number: token_positions[*i].line_number,
                        char_column: token_positions[*i].char_column + 1,
                    },
                    error_type: ErrorType::Rule,
                })
            }
        }

        // Recursively call this function until there are no more accessed args
        return get_accessed_args(
            collection_name,
            tokens,
            i,
            &data_type,
            token_positions,
            accessed_args,
        );
    }

    Ok(Vec::new())
}
