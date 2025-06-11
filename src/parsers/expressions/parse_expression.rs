#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

use super::eval_expression::evaluate_expression;
use crate::parsers::ast_nodes::{Expr, Operator};
use crate::parsers::build_ast::TokenContext;
use crate::parsers::collections::new_collection;
use crate::parsers::scene::{SceneType, Style};
use crate::parsers::variables::{create_reference, get_reference};
use crate::{
    CompileError, ErrorType, Token,
    bs_types::DataType,
    parsers::{
        ast_nodes::{Arg, AstNode},
        create_scene_node::new_scene,
    },
};
use std::collections::HashMap;
// use crate::parsers::builtin_methods::get_builtin_methods;
// use crate::parsers::functions::create_function_arguments;

// For multiple returns or function calls
// MUST know all the types
pub fn create_multiple_expressions(
    x: &mut TokenContext,
    returned_types: &[DataType],
    captured_declarations: &[Arg],
) -> Result<Vec<Expr>, CompileError> {
    let mut expressions: Vec<Expr> = Vec::new();
    let mut type_index = 0;

    while x.index < x.length && type_index < returned_types.len() {
        let expression = create_expression(
            x,
            &mut returned_types[type_index].to_owned(),
            false,
            captured_declarations,
            &[],
        )?;

        expressions.push(expression);
        type_index += 1;

        // Check for tokens breaking out of the expression chain
        match x.current_token() {
            &Token::Comma => {
                if type_index >= returned_types.len() {
                    return Err(CompileError {
                        msg: format!(
                            "Too many arguments provided. Expected: {}. Provided: {}.",
                            returned_types.len(),
                            expressions.len()
                        ),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_start_position(),
                        error_type: ErrorType::Type,
                    });
                }

                x.advance(); // Skip the comma
            }
            _ => {
                if type_index < returned_types.len() {
                    return Err(CompileError {
                        msg: "Missing a required argument. Have you provided enough values?"
                            .to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_start_position(),
                        error_type: ErrorType::Type,
                    });
                }
            }
        }
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
    starting_expression: &[AstNode],
) -> Result<Expr, CompileError> {
    let mut expression: Vec<AstNode> = Vec::from(starting_expression);
    // let mut number_union = get_any_number_datatype(false);

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

                    // This is for the case this parenthesis is consumed
                    x.skip_newlines();
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

                let value = create_expression(x, data_type, true, captured_declarations, &[])?;

                expression.push(AstNode::Reference(value, x.token_start_position()));
            }

            // COLLECTION
            Token::OpenCurly => {
                match data_type {
                    DataType::Collection(inner_type) => {
                        expression.push(AstNode::Reference(
                            new_collection(x, inner_type, captured_declarations)?,
                            x.token_start_position(),
                        ));
                    }

                    DataType::Inferred(mutable) => {
                        expression.push(AstNode::Reference(
                            new_collection(
                                x,
                                &DataType::Inferred(mutable.to_owned()),
                                captured_declarations,
                            )?,
                            x.token_start_position(),
                        ));
                    }

                    // Need to error here as a collection literal is being made with the wrong type declaration
                    _ => {
                        return Err(CompileError {
                            msg: format!(
                                "Expected a collection, but assigned variable with a literal type of: {:?}",
                                data_type
                            ),
                            start_pos: x.token_start_position(),
                            end_pos: x.token_end_position(),
                            error_type: ErrorType::Type,
                        });
                    }
                };
            }

            Token::CloseCurly
            | Token::ArgConstructor
            | Token::Comma
            | Token::EOF
            | Token::SceneClose
            | Token::Arrow
            | Token::Colon
            | Token::End => {
                if consume_closing_parenthesis {
                    return Err( CompileError {
                        msg: "Not enough closing parenthesis for expression. Need more ')' at the end of the expression".to_string(),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Syntax,
                    });
                }

                break;
            }

            Token::Newline => {
                // Fine if inside parenthesis (not closed yet)
                // Otherwise break out of the expression
                if consume_closing_parenthesis {
                    x.skip_newlines();
                    continue;
                } else {

                    // Check ahead if the next token must continue the expression
                    // So something like:
                    // x = 1 + 2
                    // + 3
                    // '+' would be a valid continuation,
                    // as '+' doesn't make sense outside expressions like this anyway
                    x.skip_newlines();

                    match x.current_token() {
                        Token::Add | Token::Subtract | Token::Multiply |
                        Token::Root | Token::Divide | Token::Modulus |
                        Token::Is | Token::GreaterThan | Token::GreaterThanOrEqual |
                        Token::LessThan | Token::LessThanOrEqual | Token::Exponent |
                        Token::Not | Token::Or | Token::Remainder |
                        Token::RemainderAssign | Token::Log
                        => continue,
                        _ => break,
                    }
                }
            }

            // Check if the name is a reference to another variable or function call
            Token::Variable(ref name, ..) => {
                if let Some(arg) = get_reference(name, captured_declarations) {
                    expression.push(create_reference(x, &arg, captured_declarations)?);
                    continue; // Will have moved onto the next token already
                } else {
                    return Err(CompileError {
                        msg: format!("Variable '{}' does not exist in this scope.", name,),
                        start_pos: x.token_start_position(),
                        end_pos: x.token_end_position(),
                        error_type: ErrorType::Syntax,
                    });
                }
            }

            // Check if is a literal
            Token::FloatLiteral(mut float) => {
                if next_number_negative {
                    float = -float;
                    next_number_negative = false;
                }

                expression.push(AstNode::Reference(
                    Expr::Float(float),
                    x.token_start_position(),
                ));
            }

            Token::IntLiteral(int) => {
                let int_value = if next_number_negative {
                    next_number_negative = false;
                    -int
                } else {
                    int
                };

                expression.push(AstNode::Reference(
                    Expr::Int(int_value),
                    x.token_start_position(),
                ));
            }

            Token::StringLiteral(ref string) => {
                expression.push(AstNode::Reference(
                    Expr::String(string.to_owned()),
                    x.token_start_position(),
                ));
            }

            Token::SceneHead | Token::ParentScene => {

                let scene_type = new_scene(
                    x,
                    captured_declarations,
                    &mut HashMap::new(),
                    &mut Style::default(),
                )?;

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
                            start_pos: x.token_start_position(),
                            end_pos: x.token_end_position(),
                            error_type: ErrorType::Type,
                        });
                    }
                }
            }

            Token::BoolLiteral(value) => {
                expression.push(AstNode::Reference(
                    Expr::Bool(value.to_owned()),
                    x.token_start_position(),
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
                expression.push(AstNode::Operator(Operator::Add, x.token_start_position()));
            }

            Token::Subtract => {
                expression.push(AstNode::Operator(
                    Operator::Subtract,
                    x.token_start_position(),
                ));
            }

            Token::Multiply => {
                expression.push(AstNode::Operator(
                    Operator::Multiply,
                    x.token_start_position(),
                ));
            }

            Token::Divide => {
                expression.push(AstNode::Operator(
                    Operator::Divide,
                    x.token_start_position(),
                ));
            }

            Token::Exponent => {
                expression.push(AstNode::Operator(
                    Operator::Exponent,
                    x.token_start_position(),
                ));
            }

            Token::Modulus => {
                expression.push(AstNode::Operator(
                    Operator::Modulus,
                    x.token_start_position(),
                ));
            }

            // LOGICAL OPERATORS
            Token::Is => {
                // Check if the next token is a not
                if let Some(Token::Not) = x.tokens.get(x.index + 1) {
                    x.advance();
                    expression.push(AstNode::Operator(
                        Operator::NotEqual,
                        x.token_start_position(),
                    ));
                } else {
                    expression.push(AstNode::Operator(
                        Operator::Equality,
                        x.token_start_position(),
                    ));
                }
            }

            Token::LessThan => {
                expression.push(AstNode::Operator(
                    Operator::LessThan,
                    x.token_start_position(),
                ));
            }
            Token::LessThanOrEqual => {
                expression.push(AstNode::Operator(
                    Operator::LessThanOrEqual,
                    x.token_start_position(),
                ));
            }
            Token::GreaterThan => {
                expression.push(AstNode::Operator(
                    Operator::GreaterThan,
                    x.token_start_position(),
                ));
            }
            Token::GreaterThanOrEqual => {
                expression.push(AstNode::Operator(
                    Operator::GreaterThanOrEqual,
                    x.token_start_position(),
                ));
            }
            Token::And => {
                expression.push(AstNode::Operator(Operator::And, x.token_start_position()));
            }
            Token::Or => {
                expression.push(AstNode::Operator(Operator::Or, x.token_start_position()));
            }

            // For mutating references
            Token::AddAssign => {}

            _ => {
                return Err(CompileError {
                    msg: format!(
                        "Invalid Value used in expression: '{:?}'. Expressions must be assigned with only valid datatypes",
                        token
                    ),
                    start_pos: x.token_start_position(),
                    end_pos: x.token_end_position(),
                    error_type: ErrorType::Type,
                });
            }
        }

        x.advance();
    }

    evaluate_expression(expression, data_type)
}

// This is used to unpack all the 'self' values of a block into multiple arguments
pub fn create_args_from_types(data_types: &[DataType]) -> Vec<Arg> {
    let mut arguments = Vec::new();

    for data_type in data_types {
        if let DataType::Object(inner_args) = data_type {
            arguments.extend(inner_args.to_owned());
        }
    }

    arguments
}
