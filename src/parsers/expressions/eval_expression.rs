use super::constant_folding::constant_fold;
use crate::parsers::ast_nodes::{Expr, Operator};
use crate::parsers::scene::{SceneContent, Style};
use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType, bs_types::DataType, parsers::ast_nodes::AstNode};

// This function will turn a series of ast nodes into a Value enum.
// A Value enum can also be a runtime expression that contains a series of nodes.
// It will fold constants (not working yet) down to a single Value if possible
pub fn evaluate_expression(
    expr: Vec<AstNode>,
    current_type: &mut DataType,
) -> Result<Expr, CompileError> {
    let mut simplified_expression: Vec<AstNode> = Vec::new();

    // SHUNTING YARD ALGORITHM
    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();

    'outer: for node in expr {
        match node {
            AstNode::Reference(ref value, _) => {
                if let DataType::Inferred(is_mutable) = current_type {
                    *current_type = value.get_type(is_mutable.to_owned());
                }

                if let DataType::CoerceToString(_) | DataType::String(_) = current_type {
                    simplified_expression.push(node);
                    continue 'outer;
                }

                output_queue.push(node.to_owned());
            }

            AstNode::FunctionCall(..) => {
                simplified_expression.push(node.to_owned());
            }

            AstNode::Operator(ref op, ref position) => {
                match current_type {
                    DataType::String(_) | DataType::Scene(_) => {
                        if op != &Operator::Add {
                            return Err(CompileError {
                                msg: "Can only use the '+' operator to manipulate strings or scenes inside expressions".to_string(),
                                start_pos: position.to_owned(),
                                end_pos: TokenPosition {
                                    line_number: position.line_number,
                                    char_column: position.char_column + 1,
                                },
                                error_type: ErrorType::Syntax,
                            });
                        }
                        continue 'outer;
                    }

                    DataType::CoerceToString(_) => {
                        simplified_expression.push(node);
                        continue 'outer;
                    }

                    _ => {}
                }

                let node_precedence = node.get_precedence();
                let left_associative = node.is_left_associative();

                pop_higher_precedence(
                    &mut operators_stack,
                    &mut output_queue,
                    node_precedence,
                    left_associative,
                );

                operators_stack.push(node);
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
        return Ok(simplified_expression[0].get_expr());
    }

    match current_type {
        DataType::Scene(_) => concat_scene(&mut simplified_expression),

        DataType::String(_) => concat_strings(&mut simplified_expression),

        DataType::CoerceToString(_) => {
            let mut new_string = String::new();

            for node in simplified_expression {
                new_string += &node.get_expr().as_string();
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
            Ok(Expr::Runtime(
                simplified_expression,
                current_type.to_owned(),
            ))
        }

        _ => {
            // MATHS EXPRESSIONS
            // Push everything into the stack, is now in RPN notation
            while let Some(operator) = operators_stack.pop() {
                output_queue.push(operator);
            }

            // Evaluate all constants in the maths expression
            constant_fold(output_queue, current_type.to_owned())
        }
    }
}

fn pop_higher_precedence(
    operators_stack: &mut Vec<AstNode>,
    output_queue: &mut Vec<AstNode>,
    current_precedence: u32,
    left_associative: bool,
) {
    while let Some(top_op_node) = operators_stack.last() {
        let o2_precedence = top_op_node.get_precedence();

        let should_pop = if left_associative {
            o2_precedence >= current_precedence
        } else {
            o2_precedence > current_precedence
        };

        if should_pop {
            output_queue.push(operators_stack.pop().unwrap());
        } else {
            break;
        }
    }
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_scene(simplified_expression: &mut Vec<AstNode>) -> Result<Expr, CompileError> {
    let mut scene_body: SceneContent = SceneContent::default();
    let mut style = Style::default();

    for node in simplified_expression {
        match node.get_expr() {
            Expr::Scene(body, ref mut scene_style, ..) => {
                scene_body.before.extend(body.before);
                scene_body.after.extend(body.after);

                if !style.unlocks_override {
                    if scene_style.unlocks_override {
                        style.unlocks_override = true;
                        style.unlocked_scenes = scene_style.unlocked_scenes.to_owned();
                    } else {
                        style.unlocked_scenes.extend(scene_style.unlocked_scenes.to_owned());
                    }
                }

                // TODO - scene style precedence
                // Some styles will override others based on their precedence
                style.format = scene_style.format.to_owned();
                style.child_default = scene_style.child_default.to_owned();
                style.compatibility = scene_style.compatibility.to_owned();
                style.precedence = scene_style.precedence.to_owned();
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

    Ok(Expr::Scene(scene_body, style, String::new()))
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
fn concat_strings(simplified_expression: &mut Vec<AstNode>) -> Result<Expr, CompileError> {
    let mut final_string_expression: Vec<AstNode> = Vec::with_capacity(1);
    for node in simplified_expression {
        let expr = node.get_expr();
        match expr {
            Expr::String(ref string) => {
                let mut last_node = final_string_expression.last();
                match &mut last_node {
                    Some(AstNode::Expression(expr, ..)) => {
                        expr.evaluate_operator(&Expr::String(string.to_string()), &Operator::Add);
                    }
                    _ => {
                        final_string_expression
                            .push(AstNode::Operator(Operator::Add, node.get_position()));
                        final_string_expression.push(node.to_owned());
                    }
                }
            }

            Expr::Runtime(_, _) => final_string_expression.push(node.to_owned()),

            Expr::Reference(..) => {
                final_string_expression.push(AstNode::Operator(Operator::Add, node.get_position()));
                final_string_expression.push(node.to_owned());
            }

            _ => {
                return Err(CompileError {
                    msg: format!("Used value of type: {:?} in string expression", node),
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::Type,
                });
            }
        }
    }

    if final_string_expression.len() == 1 {
        return Ok(final_string_expression[0].to_owned().get_expr());
    }

    Ok(Expr::Runtime(
        final_string_expression,
        DataType::String(false),
    ))
}
