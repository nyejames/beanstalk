use super::constant_folding::{logical_constant_fold, math_constant_fold};
use crate::parsers::ast_nodes::Value;
use crate::tokenizer::TokenPosition;
use crate::{bs_types::DataType, parsers::ast_nodes::AstNode, CompileError, ErrorType, Token};

// This function will turn a series of ast nodes into a Value enum
// A Value enum can also be a runtime expression which contains a series of nodes
// It will fold constants (not working yet) down to a single Value if possible
pub fn evaluate_expression(
    expr: Vec<AstNode>,
    type_declaration: &DataType,
) -> Result<Value, CompileError> {
    let mut current_type = type_declaration.to_owned();
    let mut simplified_expression: Vec<AstNode> = Vec::new();

    // SHUNTING YARD ALGORITHM
    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();

    'outer: for ref node in expr {
        match node {
            AstNode::Literal(value, _) => {
                // Ignore shunting yard for strings and coerced strings
                if current_type == DataType::CoerceToString || current_type == DataType::String {
                    simplified_expression.push(node.to_owned());
                    continue;
                }

                match value {
                    /*
                        if the token is:
                            - a number:
                                put it into the output queue
                    */
                    Value::Float(_) | Value::Int(_) | Value::Bool(_) => {
                        output_queue.push(node.to_owned());
                    }

                    // Anything else must can't be folded at compile time
                    _ => {
                        simplified_expression.push(node.to_owned());
                    }
                }

                if current_type == DataType::Inferred {
                    current_type = value.get_type();
                }
            }
            /*
               - a function:
               push it onto the operator stack

            */
            // Should already be type checked
            AstNode::FunctionCall(..) => {
                operators_stack.push(node.to_owned());
            }

            AstNode::BinaryOperator(op, position) => {
                // If the current type is a string or scene, add operator is assumed.
                if current_type == DataType::String || current_type == DataType::Scene {
                    if op != &Token::Add {
                        return Err( CompileError {
                            msg: "Can only use the '+' operator to manipulate strings or scenes inside expressions".to_string(),
                            start_pos: position.to_owned(),
                            end_pos: TokenPosition {
                                line_number: position.line_number,
                                char_column: position.char_column + 1,
                            },
                            error_type: ErrorType::Syntax,
                        });
                    }

                    // We don't push the node into the simplified expression atm
                    // As the only kind of string expression is contaminating them
                    // So simplified string expressions are just a list of strings
                    // Maybe other kinds of string expression will be valid in the future so more logic is needed here
                    // simplified_expression.push(node.to_owned());
                    continue;
                }

                if current_type == DataType::CoerceToString {
                    simplified_expression.push(node.to_owned());
                    continue;
                }

                if current_type == DataType::Bool {
                    if *op != Token::Or
                        || *op != Token::And
                        || *op != Token::Equal
                        || *op != Token::Not
                        || *op != Token::LessThan
                        || *op != Token::LessThanOrEqual
                        || *op != Token::GreaterThan
                        || *op != Token::GreaterThanOrEqual
                    {
                        return Err(CompileError {
                            msg: "Can only use logical operators in booleans expressions"
                                .to_string(),
                            start_pos: position.to_owned(),
                            end_pos: TokenPosition {
                                line_number: position.line_number,
                                char_column: position.char_column + 1,
                            },
                            error_type: ErrorType::Syntax,
                        });
                    }

                    simplified_expression.push(node.to_owned());
                    continue;
                }

                /*
                    - an operator node:

                    while (
                        there is an operator o2 at the top of the operator stack which is not a left parenthesis,
                        and (o2 has greater precedence than node or (node and o2 have the same precedence and node is left-associative))
                    ):
                        pop o2 off the operator stack into the output queue
                        push node onto the operator stack
                */

                while let Some(o2) = operators_stack.pop() {
                    if o2.get_precedence() > node.get_precedence() {
                        output_queue.push(o2);
                        continue;
                    } else {
                        operators_stack.push(node.to_owned());
                        operators_stack.push(o2);
                        continue 'outer;
                    }
                }

                operators_stack.push(node.to_owned());
            }

            /*
                - a ",":
                while the operator at the top of the operator stack is not a left parenthesis:
                    pop the operator from the operator stack into the output queue

            */
            _ => {
                return Err(CompileError {
                    msg: format!("unsupported AST node found in expression: {:?}", node),
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        }
    }

    // If nothing to evaluate at compile time, just one value, return that value
    if simplified_expression.len() == 1 {
        return Ok(simplified_expression[0].get_value());
    }

    // LOGICAL EXPRESSIONS
    if current_type == DataType::Bool {
        for operator in operators_stack {
            output_queue.push(operator);
        }

        return logical_constant_fold(output_queue, current_type);
    }

    // SCENE EXPRESSIONS
    // If constant scene expression, combine the scenes together and return the new scene
    if current_type == DataType::Scene {
        return concat_scene(&mut simplified_expression);
    }

    // STRING EXPRESSIONS
    // If the expression is a constant string, combine and return a string
    if current_type == DataType::String {
        return concat_strings(&mut simplified_expression);
    }

    // Scene Head Coerce to String
    if current_type == DataType::CoerceToString {
        // TODO - line number
        return Ok(Value::Runtime(simplified_expression, current_type));
    }

    // MATHS EXPRESSIONS
    // Push everything into the stack, is now in RPN notation
    for operator in operators_stack {
        output_queue.push(operator);
    }

    // Evaluate all constants in the maths expression
    math_constant_fold(output_queue, current_type)
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_scene(simplified_expression: &mut Vec<AstNode>) -> Result<Value, CompileError> {
    let mut nodes = Vec::new();
    let mut styles = Vec::new();

    for node in simplified_expression {
        match node.get_value() {
            Value::Scene(ref mut body, ref mut vec3, _) => {
                nodes.append(body);
                styles.append(vec3);
            }

            _ => {
                return Err(CompileError {
                    msg: "Non-scene value found in scene expression (you can only concatenate scenes with other scenes)".to_string(),
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        }
    }

    Ok(Value::Scene(nodes, styles, String::new()))
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_strings(simplified_expression: &mut Vec<AstNode>) -> Result<Value, CompileError> {
    let mut new_string = String::new();

    // String simplified expressions are just a list of strings atm
    // So we can just concatenate them into a single string
    // This will eventually need to be more complex to handle functions and other string manipulations
    // The more complex things will be Runtime values
    // However, there should also be compile-time folding for some of this stuff

    for node in simplified_expression {
        match node.get_value() {
            Value::String(ref string) => {
                new_string.push_str(string);
            }

            Value::Runtime(_, _) => {
                return Err(CompileError {
                    msg: "Runtime expressions not supported yet in string expression (concat strings - eval expression). Can only concatenate strings at compile time right now".to_string(),
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::Compiler,
                });
            }

            _ => {
                return Err(CompileError {
                    msg: "Non-string (or runtime string expression) used in string expression (concat strings - eval expression).
                    Compiler should have already caught this, so 'Evaluate Expression' has not done it's job successfully".to_string(),
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::Compiler,
                });
            }
        }
    }

    Ok(Value::String(new_string))
}
