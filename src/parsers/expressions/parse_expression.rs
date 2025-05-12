#[allow(unused_imports)]
use colour::{grey_ln, red_ln};
use std::collections::HashMap;

use super::eval_expression::evaluate_expression;
use crate::parsers::ast_nodes::{Operator, Expr};
use crate::parsers::build_ast::TokenContext;
use crate::parsers::scene::{SceneType, Style};
use crate::parsers::variables::create_new_var_or_ref;
use crate::tokenizer::TokenPosition;
use crate::{
    CompileError, ErrorType, Token,
    bs_types::DataType,
    parsers::{
        ast_nodes::{Arg, AstNode},
        create_scene_node::new_scene,
        structs::create_args,
    },
};

// For multiple returns
pub fn create_multiple_expressions(
    x: &mut TokenContext,
    data_types: &mut Vec<DataType>,
    captured_declarations: &[Arg],
) -> Result<Vec<Expr>, CompileError> {
    let mut expressions: Vec<Expr> = Vec::new();

    while x.index < x.length {
        let expression = create_expression(
            x,
            &mut data_types[data_types.len() - 1].clone(),
            false,
            captured_declarations,
        )?;

        // Make sure there was a comma after the expression
        // Unless this is the last expression
        if x.current_token() != &Token::Comma {
            if expressions.len() < data_types.len() - 1 {
                return Err(CompileError {
                    msg: "Missing comma to separate expressions. Have you provided enough values?".to_string(),
                    start_pos: x.current_position(),
                    end_pos: x.current_position(),
                    error_type: ErrorType::Syntax,
                })
            }
        } else {
            x.advance(); // Skip the comma
        }

        expressions.push(expression);
    }

    Ok(expressions)
}

// If the datatype is a collection,
// the expression must only contain references to collections
// or collection literals.
pub fn create_expression(
    x: &mut TokenContext,
    data_type: &mut DataType,
    consume_closing_parenthesis: bool,
    captured_declarations: &[Arg],
) -> Result<Expr, CompileError> {
    let mut expression: Vec<AstNode> = Vec::new();
    // let mut number_union = get_any_number_datatype(false);

    // Ignore any newlines at the start of the expression
    while x.index < x.length && x.current_token() == &Token::Newline {}

    // Loop through the expression and create the AST nodes
    // Figure out the type it should be from the data
    // DOES NOT MOVE TOKENS PAST THE CLOSING TOKEN
    let mut next_number_negative = false;
    while x.index < x.length {
        let token = x.current_token().to_owned();
        match token {

            Token::CloseParenthesis => {
                if consume_closing_parenthesis {
                    x.advance();
                }

                if expression.is_empty() {
                    return Ok(Expr::None);
                }
                
                break;
            }

            Token::OpenParenthesis => {
                // Move past the open parenthesis before calling this function again
                // Removed this at one point for a test caused a wonderful infinite loop
                x.advance();

                let value =
                    create_expression(x, data_type, true, captured_declarations)?;

                expression.push(AstNode::Literal(value, x.current_position()));
            }

            Token::OpenCurly => {
                x.advance();

                return match data_type {
                    // TODO - do we need to handle unions here? or are they always collapsed into one type before being parsed?
                    DataType::Arguments(inner_types) => {
                        // HAS DEFINED INNER TYPES FOR THE struct
                        // could this still result in None if the inner types are defined and not optional?
                        let structure = create_args(
                            x,
                            Expr::None,
                            inner_types,
                            captured_declarations,
                        )?;

                        Ok(Expr::Args(structure))
                    }

                    // If this is inside parenthesis, and we don't know the type.
                    // It must be a struct
                    // This is enforced! If it's a single expression wrapped in parentheses,
                    // it will be flatted into that single value anyway by struct_to_value
                    DataType::Inferred(_) => {
                        // NO DEFINED TYPES FOR THE struct
                        let structure = create_args(
                            x,
                            Expr::None,
                            // The difference is this is inferred
                            &Vec::new(),
                            captured_declarations,
                        )?;

                        // And then the type is set here
                        *data_type = DataType::Arguments(structure.to_owned());
                        Ok(Expr::Args(structure))
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

            Token::CloseCurly | Token::Comma | Token::EOF | Token::SceneClose | Token::Arrow | Token::Colon | Token::End => {
                if consume_closing_parenthesis {
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

                break;
            }

            Token::Newline => {
                // Fine if inside parenthesis (not closed yet)
                // Otherwise break out of the expression
                if consume_closing_parenthesis {
                    x.advance();
                    continue;
                } else {
                    break;
                }
            }

            // Check if the name is a reference to another variable or function call
            Token::Variable(ref name, ref visibility, ..) => {
                // This is never reached (I think) if we are inside a struct or collection
                let new_ref = create_new_var_or_ref(
                    x,
                    name,
                    captured_declarations,
                    visibility,
                )?;

                match new_ref {
                    AstNode::Literal(..) => {
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
                let scene_type = new_scene(x, captured_declarations, &mut HashMap::new(), Style::default())?;
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
                x.advance();
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
                    x.advance();
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

        x.advance();
    }

    evaluate_expression(expression, data_type)
}

pub fn get_accessed_args(
    x: &mut TokenContext,
    collection_name: &str,
    data_types: &[DataType],
    accessed_args: &mut Vec<String>,
) -> Result<Vec<String>, CompileError> {
    // Check if there is access
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

                // for now just an error if it's negative
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
                if idx >= data_types.len() {
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
                };

                let data_type = &data_types[idx];
                return match data_type {
                    DataType::Arguments(inner_args) => {
                        accessed_args.push(idx.to_string());

                        let inner_types = inner_args.iter().map(|arg| arg.data_type.to_owned()).collect::<Vec<DataType>>();

                        // Recursively call this function until there are no more accessed args
                        get_accessed_args(x, collection_name, &inner_types, accessed_args)
                    }

                    DataType::Collection(data_type) => {
                        accessed_args.push(idx.to_string());

                        // Recursively call this function until there are no more accessed args
                        get_accessed_args(x, collection_name, &[*data_type.to_owned()], accessed_args)
                    }

                    _ => {
                        Err(CompileError {
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
                        })
                    }
                }
            }

            // NAMED ARGUMENT ACCESS
            Some(Token::Variable(name, ..)) => {
                // Make sure this data type is arguments (named values)
                // Collect any returned types that are arguments
                let mut arguments = Vec::new();
                
                for data_type in data_types {
                    match data_type {
                        DataType::Arguments(inner_args) => {
                            arguments.extend(inner_args);
                        }
                        _ => {}
                    }
                }
                
                let access = match arguments.iter().find(|arg| arg.name == *name) {
                    Some(access) => *access,
                    None => {
                        return Err(CompileError {
                            msg: format!(
                                "Name '{}' not found inside '{}'",
                                name, collection_name
                            ),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 1,
                            },
                            error_type: ErrorType::Rule,
                        })
                    }
                };

                match &access.data_type {
                    DataType::Arguments(inner_types) => {
                        if let Some(idx) = inner_types.iter().position(|arg| arg.name == *name) {
                            accessed_args.push(access.name.to_owned());
                        }
                    }

                    _ => {
                        return Err(CompileError {
                            msg: "Parse expression named access for non argument type (compiler error - may change in future)".to_string(),
                            start_pos: x.current_position(),
                            end_pos: TokenPosition {
                                line_number: x.current_position().line_number,
                                char_column: x.current_position().char_column + 1,
                            },
                            error_type: ErrorType::Compiler,
                        });
                    }
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
    }

    Ok(Vec::new())
}
