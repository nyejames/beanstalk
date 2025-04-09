use crate::parsers::ast_nodes::Value;
use crate::{bs_types::DataType, parsers::ast_nodes::AstNode, CompileError, ErrorType, Token};

use crate::tokenizer::TokenPosition;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln_bold, red_ln};

// TODO - currently doesn't work lol
// This will evaluate everything possible at compile time
// returns either a literal or an evaluated runtime expression
// Output stack must be in RPN order
pub fn math_constant_fold(
    output_stack: Vec<AstNode>,
    current_type: DataType,
) -> Result<Value, CompileError> {
    let mut stack: Vec<AstNode> = Vec::new();
    let mut first_line_number = 0;

    // for node in &output_stack {
    //     blue_ln!("output_stack: {:?}", node);
    // }
    for node in &output_stack {
        // red_ln!("output_stack: {:?}", stack);

        match node {
            AstNode::BinaryOperator(op, token_position) => {
                let line_number = token_position.line_number;
                let char_column = token_position.char_column;

                if line_number != first_line_number {
                    first_line_number = line_number;
                }

                // green_ln_bold!("Binary operator found: {:?}", op);

                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return Err(CompileError {
                        msg: format!("Not enough nodes on the stack for binary operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}", output_stack, stack),
                        start_pos: token_position.to_owned(),
                        end_pos: TokenPosition {
                            line_number,
                            char_column,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }

                let rhs = stack.pop().unwrap();
                let lhs = stack.pop().unwrap();

                // Check if top 2 of stack are literals
                // if at least one is not then this must be a runtime expression
                // And just push the operator onto the stack instead of evaluating
                // TO DO: GENERICS FOR THIS TO SUPPORT INTS CORRECTLY
                let lhs_value = match lhs.get_value() {
                    Value::Float(value) => value,
                    Value::Int(value) => value as f64,

                    // TODO - some runtime thing
                    _ => {
                        stack.push(rhs);
                        stack.push(lhs);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                let rhs_value = match rhs.get_value() {
                    Value::Float(value) => value,
                    Value::Int(value) => value as f64,

                    // TODO - some runtime thing
                    _ => {
                        stack.push(rhs);
                        stack.push(lhs);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                let new_number = AstNode::Literal(
                    Value::Float(match op {
                        Token::Add => lhs_value + rhs_value,
                        Token::Subtract => lhs_value - rhs_value,
                        Token::Multiply => lhs_value * rhs_value,
                        Token::Divide => lhs_value / rhs_value,
                        Token::Modulus => lhs_value % rhs_value,
                        _ => {
                            return Err(CompileError {
                                msg: format!("Unsupported operator found in operator stack when parsing an expression into WAT: {:?}", op),
                                start_pos: token_position.to_owned(),
                                end_pos: TokenPosition {
                                    line_number,
                                    char_column,
                                },
                                error_type: ErrorType::Syntax,
                            });
                        }
                    }),
                    TokenPosition {
                        line_number,
                        char_column,
                    },
                );

                stack.push(new_number);
            }

            // Literal or anything else
            _ => {
                stack.push(node.to_owned());
            }
        }
    }

    // red_ln!("final stack: {:?}", stack);

    if stack.len() == 1 {
        return Ok(stack[0].get_value());
    }

    if stack.is_empty() {
        return Ok(Value::None);
    }

    Ok(Value::Runtime(stack, current_type))
}

pub fn logical_constant_fold(
    output_stack: Vec<AstNode>,
    current_type: DataType,
) -> Result<Value, CompileError> {
    let mut stack: Vec<AstNode> = Vec::new();

    let mut first_line_number = 0;

    for node in &output_stack {
        match node {
            AstNode::LogicalOperator(op, token_position) => {
                let line_number = token_position.line_number;
                let char_column = token_position.char_column;

                if first_line_number == 0 {
                    first_line_number = line_number;
                }

                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return Err(CompileError {
                        msg: format!("Not enough nodes on the stack for logical operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}", output_stack, stack),
                        start_pos: token_position.to_owned(),
                        end_pos: TokenPosition {
                            line_number,
                            char_column,
                        },
                        error_type: ErrorType::Syntax,
                    });
                }
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();

                // Check if top 2 of stack are literals
                // if at least one is not then this must be a runtime expression
                // And just push the operator onto the stack instead of evaluating
                let left_value = match left {
                    AstNode::Literal(Value::Bool(value), ..) => value,
                    _ => {
                        stack.push(left);
                        stack.push(right);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                let right_value = match right {
                    AstNode::Literal(Value::Bool(value), ..) => value,
                    _ => {
                        stack.push(left);
                        stack.push(right);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                let new_bool = AstNode::Literal(
                    Value::Bool(match op {
                        Token::Equal => left_value == right_value,
                        Token::And => left_value && right_value,
                        Token::Or => left_value || right_value,
                        _ => {
                            return Err(CompileError {
                            msg: format!("Unsupported operator found in operator stack when parsing an expression into WAT: {:?}", op),
                            start_pos: token_position.to_owned(),
                            end_pos: TokenPosition {
                                line_number,
                                char_column,
                            },
                            error_type: ErrorType::Syntax,
                        });
                        }
                    }),
                    TokenPosition {
                        line_number,
                        char_column,
                    },
                );

                stack.push(new_bool);
            }

            // Some runtime thing
            _ => {
                stack.push(node.to_owned());
            }
        }
    }

    if stack.len() == 1 {
        return match stack.pop() {
            Some(node) => Ok(node.get_value()),
            None => Err(CompileError {
                msg: "No node found in stack when parsing an expression in Constant_folding"
                    .to_string(),
                start_pos: TokenPosition {
                    line_number: 0,
                    char_column: 0,
                },
                end_pos: TokenPosition {
                    line_number: 0,
                    char_column: 0,
                },
                error_type: ErrorType::Compiler,
            }),
        };
    }

    Ok(Value::Runtime(stack, current_type))
}
