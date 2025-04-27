use super::constant_folding::{logical_constant_fold, constant_fold};
use crate::parsers::ast_nodes::{Operator, Expr};
use crate::parsers::scene::{SceneContent, Style};
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, bs_types::DataType, parsers::ast_nodes::AstNode};

// This function will turn a series of ast nodes into a Value enum.
// A Value enum can also be a runtime expression that contains a series of nodes.
// It will fold constants (not working yet) down to a single Value if possible
pub fn evaluate_expression(
    expr: Vec<AstNode>,
    type_declaration: &DataType,
) -> Result<Expr, CompileError> {
    let mut current_type = type_declaration.to_owned();
    let mut simplified_expression: Vec<AstNode> = Vec::new();

    // SHUNTING YARD ALGORITHM
    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();

    'outer: for node in expr {
        match node {
            AstNode::Literal(ref value, _) => {
                // Ignore shunting yard for strings and coerced strings
                match current_type {
                    DataType::CoerceToString(_) | DataType::String(_) => {
                        simplified_expression.push(node.to_owned());
                        continue 'outer;
                    }
                    DataType::Inferred(_) => {
                        current_type = value.get_type();
                    }
                    _ => {}
                }

                match value {
                    Expr::Float(_) | Expr::Int(_) | Expr::Bool(_) => {
                        output_queue.push(node.to_owned());
                    }

                    _ => {
                        simplified_expression.push(node.to_owned());
                    }
                }
            }

            AstNode::FunctionCall(..) => {
                simplified_expression.push(node.to_owned());
            }

            AstNode::Operator(ref op, ref position) => {
                // If the current type is a string or scene, an added operator is assumed.
                match current_type {
                    DataType::String(_) | DataType::Scene(_) => {
                        if op != &Operator::Add {
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
                        // As the only kind of string expression is contaminating them,
                        // So simplified string expressions are just a list of strings
                        // Maybe other kinds of string expression will be valid in the future, so more logic is needed here
                        // simplified_expression.push(node.to_owned());
                        continue 'outer;
                    }

                    DataType::CoerceToString(_) => {
                        simplified_expression.push(node.to_owned());
                        continue 'outer;
                    }

                    _ => {}
                }

                while let Some(top_op_node) = operators_stack.last() {
                    // Stop if top is not an operator (e.g. left parenthesis)
                    match top_op_node {
                        AstNode::Operator(..) => {}
                        _ => {
                            break;
                        }
                    }

                    let o2_precedence = top_op_node.get_precedence();
                    let node_precedence = node.get_precedence();

                    if o2_precedence >= node_precedence {
                        output_queue.push(operators_stack.pop().unwrap()); // Pop from stack to output
                    } else {
                        // Current 'node' has higher precedence, stop popping
                        break;
                    }
                }

                operators_stack.push(node.to_owned());
            }

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

    match current_type {
        DataType::Bool(_) => {
            for operator in operators_stack {
                output_queue.push(operator);
            }

            logical_constant_fold(output_queue, current_type)
        }

        DataType::Scene(_) => concat_scene(&mut simplified_expression),

        DataType::String(_) => concat_strings(&mut simplified_expression),

        DataType::CoerceToString(_) => {
            let mut new_string = String::new();

            for node in simplified_expression {
                new_string += &node.get_value().as_string();
            }
            Ok(Expr::String(new_string))
        }

        // At this stage, inferred should only be possible if only variables of unknown types
        // have been used in the expression.
        // So we need to mark this expression to be evaluated later on in the compiler once we know those types.
        // This can happen due to imports.
        DataType::Inferred(_) => {
            // If there were any explicit numerical types, then this will be passed to math_constant_fold.
            // This is just to skip calling that function if no numerical constants were found.
            Ok(Expr::Runtime(simplified_expression, current_type))
        }

        _ => {
            // MATHS EXPRESSIONS
            // Push everything into the stack, is now in RPN notation
            while let Some(operator) = operators_stack.pop() {
                output_queue.push(operator);
            }

            // Evaluate all constants in the maths expression
            constant_fold(output_queue, current_type)
        }
    }
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_scene(simplified_expression: &mut Vec<AstNode>) -> Result<Expr, CompileError> {
    let mut scene_body: SceneContent = SceneContent::default();
    let mut style = Style::default();
    let mut head_nodes: SceneContent = SceneContent::default();

    for node in simplified_expression {
        match node.get_value() {
            Expr::Scene(body, ref mut scene_style, head, ..) => {
                scene_body.before.extend(body.before);
                scene_body.after.extend(body.after);
                
                // TODO - scene style precedence
                // Some styles will override others
                head_nodes.before.extend(head.before);
                head_nodes.after.extend(head.after);
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

    Ok(Expr::Scene(scene_body, style, head_nodes, String::new()))
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_strings(simplified_expression: &mut Vec<AstNode>) -> Result<Expr, CompileError> {
    let mut new_string = String::new();

    // String simplified expressions are just a list of strings atm.
    // So we can just concatenate them into a single String.
    // This will eventually need to be more complex to handle functions and other string manipulations.
    // The more complex things will be Runtime values.
    // However, there should also be compile-time folding for some of this stuff.

    for node in simplified_expression {
        match node.get_value() {
            Expr::String(ref string) => {
                new_string.push_str(string);
            }

            Expr::Runtime(_, _) => {
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

    Ok(Expr::String(new_string))
}
