use crate::parsers::ast_nodes::{NodeInfo, Value};
use crate::{bs_types::DataType, parsers::ast_nodes::AstNode, CompileError, Token};

// TODO - currently doesn't work lol
// This will evaluate everything possible at compile time
// returns either a literal or an evaluated runtime expression
pub fn math_constant_fold(
    output_stack: Vec<AstNode>,
    current_type: DataType,
) -> Result<Value, CompileError> {
    let mut stack: Vec<AstNode> = Vec::new();
    let mut first_line_number = 0;

    for node in &output_stack {
        match node {
            AstNode::BinaryOperator(op, _, line_number) => {
                if first_line_number == 0 {
                    first_line_number = line_number.to_owned();
                }

                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return Err(CompileError {
                        msg: format!("Not enough nodes on the stack for binary operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}", output_stack, stack),
                        line_number: line_number.to_owned(),
                    });
                }
                let right = stack.pop().unwrap();
                let left = stack.pop().unwrap();

                // Check if top 2 of stack are literals
                // if at least one is not then this must be a runtime expression
                // And just push the operator onto the stack instead of evaluating
                // TO DO: GENERICS FOR THIS TO SUPPORT INTS CORRECTLY
                let left_value = match left {
                    AstNode::Literal(ref value, _) => match value {
                        Value::Float(value) => *value,
                        Value::Int(value) => *value as f64,
                        _ => {
                            stack.push(left);
                            stack.push(right);
                            stack.push(node.to_owned());
                            continue;
                        }
                    },
                    _ => {
                        stack.push(left);
                        stack.push(right);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                let right_value = match right {
                    AstNode::Literal(ref value, _) => match value {
                        Value::Float(value) => *value,
                        Value::Int(value) => *value as f64,
                        _ => {
                            stack.push(left);
                            stack.push(right);
                            stack.push(node.to_owned());
                            continue;
                        }
                    },
                    _ => {
                        stack.push(left);
                        stack.push(right);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                let new_number = AstNode::Literal(
                    Value::Float(match op {
                        Token::Add => left_value + right_value,
                        Token::Subtract => left_value - right_value,
                        Token::Multiply => left_value * right_value,
                        Token::Divide => left_value / right_value,
                        Token::Modulus => left_value % right_value,
                        _ => {
                            return Err(CompileError {
                                msg: format!("Unsupported operator found in operator stack when parsing an expression into WAT: {:?}", op),
                                line_number: line_number.to_owned(),
                            });
                        }
                    }),
                    line_number.to_owned(),
                );

                stack.push(new_number);
            }

            // Some runtime thing
            _ => {
                stack.push(node.to_owned());
            }
        }
    }

    if stack.len() == 1 {
        return Ok(stack[0].get_value());
    }

    if stack.len() == 0 {
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
            AstNode::LogicalOperator(op, _, line_number) => {
                if first_line_number == 0 {
                    first_line_number = line_number.to_owned();
                }

                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return Err(CompileError {
                        msg: format!("Not enough nodes on the stack for logical operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}", output_stack, stack),
                        line_number: line_number.to_owned(),
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
                            line_number: line_number.to_owned(),
                        });
                        }
                    }),
                    line_number.to_owned(),
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
                msg: "Compiler Bug: No node found in stack when parsing an expression in Constant_folding".to_string(),
                line_number: 0,
            }),
        };
    }

    Ok(Value::Runtime(stack, current_type))
}
