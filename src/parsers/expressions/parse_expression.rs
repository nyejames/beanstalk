#[allow(unused_imports)]
use colour::{grey_ln, red_ln};
use std::collections::HashMap;

use super::eval_expression::evaluate_expression;
use crate::bs_types::get_any_number_datatype;
use crate::html_output::html_styles::get_html_styles;
use crate::parsers::ast_nodes::Value;
use crate::parsers::build_ast::TokenContext;
use crate::parsers::expressions::function_call_inline::inline_function_call;
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
) -> Result<Value, CompileError> {
    let mut expression: Vec<AstNode> = Vec::new();
    let mut number_union = get_any_number_datatype(false);

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
                        return Ok(Value::None);
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
                            Value::None,
                            inner_types,
                            ast,
                            captured_declarations,
                        )?;

                        Ok(Value::StructLiteral(structure))
                    }

                    // If this is inside parenthesis, and we don't know the type.
                    // It must be a struct
                    // This is enforced! If it's a single expression wrapped in parentheses,
                    // it will be flatted into that single value anyway by struct_to_value
                    DataType::Inferred(_) => {
                        // NO DEFINED TYPES FOR THE struct
                        let structure = new_fixed_collection(
                            x,
                            Value::None,
                            // Difference is this is inferred
                            &Vec::new(),
                            ast,
                            captured_declarations,
                        )?;

                        // And then the type is set here
                        *data_type = DataType::Structure(structure.to_owned());
                        Ok(Value::StructLiteral(structure))
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
                        error_type: ErrorType::TypeError,
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

                // red_ln!("new ref: {:?}", new_ref);

                match new_ref {
                    AstNode::Literal(ref value, ..) => {
                        // Check the type is correct
                        let reference_data_type = value.get_type();

                        // In the case this is an import, and we don't know the type yet,
                        // The type will be a 'Pointer', which passes this sniff test
                        // TODO: move all type checking like this to a stage after the AST creation
                        if !reference_data_type.is_valid_type(data_type) {
                            return Err(CompileError {
                                msg: format!(
                                    "Variable '{}' is of type {:?}, but used in an expression of type {:?}",
                                    name, reference_data_type, data_type
                                ),
                                start_pos: x.current_position(),
                                end_pos: TokenPosition {
                                    line_number: x.current_position().line_number,
                                    char_column: x.current_position().char_column + 1,
                                },
                                error_type: ErrorType::TypeError,
                            });
                        }

                        expression.push(new_ref);
                    }

                    AstNode::FunctionCall(ref name, ..) => {
                        // Check the type is correct
                        let reference_data_type = new_ref.get_type();
                        if !reference_data_type.is_valid_type(data_type) {
                            return Err(CompileError {
                                msg: format!(
                                    "Function call '{}' is of type {:?}, but used in an expression of type {:?}",
                                    name, reference_data_type, data_type
                                ),
                                start_pos: x.current_position(),
                                end_pos: TokenPosition {
                                    line_number: x.current_position().line_number,
                                    char_column: x.current_position().char_column
                                        + name.len() as i32,
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
                if !DataType::Float(false).is_valid_type(data_type) {
                    return Err(CompileError {
                        msg: format!("Float literal used in expression of type: {:?}", data_type),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column
                                + float.to_string().len() as i32,
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
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::IntLiteral(int) => {
                if !DataType::Int(false).is_valid_type(data_type) {
                    return Err(CompileError {
                        msg: format!("Int literal used in expression of type: {:?}", data_type),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column
                                + int.to_string().len() as i32,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                let int_value = if next_number_negative {
                    next_number_negative = false;
                    -int
                } else {
                    int
                };

                expression.push(AstNode::Literal(
                    Value::Int(int_value),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::StringLiteral(ref string) => {
                if !DataType::String(false).is_valid_type(data_type) {
                    return Err(CompileError {
                        msg: format!("String literal used in expression of type: {:?}", data_type),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column
                                + token.dimensions().char_column,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                expression.push(AstNode::Literal(
                    Value::String(string.to_owned()),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            // Scenes - Create a new scene node
            // Maybe scenes can be added together like strings
            Token::SceneHead | Token::ParentScene => {
                if !DataType::Scene(false).is_valid_type(data_type) {
                    return Err(CompileError {
                        msg: format!("Scene literal used in expression of type: {:?}", data_type),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column
                                + token.dimensions().char_column,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                // Add the default core HTML styles as the initial unlocked styles
                let mut unlocked_styles = HashMap::from(get_html_styles());

                return new_scene(x, ast, captured_declarations, &mut unlocked_styles);
            }

            Token::BoolLiteral(value) => {
                expression.push(AstNode::Literal(
                    Value::Bool(value.to_owned()),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            // OPERATORS
            // Will push as a string, so shunting yard can handle it later just as a string
            Token::Negative => {
                next_number_negative = true;
            }

            // BINARY OPERATORS
            Token::Add => {
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::Subtract => {
                if !data_type.is_valid_type(&mut number_union) {
                    return Err(CompileError {
                        msg: format!(
                            "Subtraction can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }

                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::Multiply => {
                if !data_type.is_valid_type(&mut number_union) {
                    return Err(CompileError {
                        msg: format!(
                            "Multiplication can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::Divide => {
                if !number_union.is_valid_type(data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Division can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::Modulus => {
                if !number_union.is_valid_type(data_type) {
                    return Err(CompileError {
                        msg: format!(
                            "Modulus can't be used in expression of type: {:?}",
                            data_type
                        ),
                        start_pos: x.current_position(),
                        end_pos: TokenPosition {
                            line_number: x.current_position().line_number,
                            char_column: x.current_position().char_column + 1,
                        },
                        error_type: ErrorType::TypeError,
                    });
                }
                expression.push(AstNode::BinaryOperator(
                    token.to_owned(),
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            // LOGICAL OPERATORS
            Token::Equal => {
                expression.push(AstNode::LogicalOperator(
                    Token::Equal,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }

            Token::LessThan => {
                expression.push(AstNode::LogicalOperator(
                    Token::LessThan,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::LogicalOperator(
                    Token::LessThanOrEqual,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }
            Token::GreaterThan => {
                expression.push(AstNode::LogicalOperator(
                    Token::GreaterThan,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::LogicalOperator(
                    Token::GreaterThanOrEqual,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }
            Token::And => {
                expression.push(AstNode::LogicalOperator(
                    Token::And,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
                ));
            }
            Token::Or => {
                expression.push(AstNode::LogicalOperator(
                    Token::Or,
                    TokenPosition {
                        line_number: x.current_position().line_number,
                        char_column: x.current_position().char_column,
                    },
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
                    error_type: ErrorType::TypeError,
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
