use super::constant_folding::constant_fold;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::compiler::parsers::template::{Style, TemplateContent};
use crate::compiler::parsers::tokens::TextLocation;
use crate::{
    compiler::datatypes::DataType, compiler::parsers::ast_nodes::NodeKind, eval_log,
    return_compiler_error, return_syntax_error,
};

// This function will turn a series of ast nodes into a Value enum.
// A Value enum can also be a runtime expression that contains a series of nodes.
// It will fold constants (not working yet) down to a single Value if possible
pub fn evaluate_expression(
    nodes: Vec<AstNode>,
    current_type: &mut DataType,
) -> Result<Expression, CompileError> {
    let mut simplified_expression: Vec<AstNode> = Vec::with_capacity(2);

    eval_log!("Evaluating expression: {:#?}", nodes);

    // SHUNTING YARD ALGORITHM
    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();

    // Should always be at least one node in the expression being evaluated
    let location = match nodes.first() {
        Some(node) => node.location,
        None => return_compiler_error!("No nodes found in expression. This should never happen."),
    };

    'outer: for node in nodes {
        match node.kind {
            NodeKind::Reference(ref expr, ..) => {
                if let DataType::Inferred(..) = current_type {
                    *current_type = expr.data_type.to_mutable();
                }

                if let DataType::CoerceToString(_) | DataType::String(_) = current_type {
                    simplified_expression.push(node.to_owned());
                    continue 'outer;
                }

                output_queue.push(node.to_owned());
            }

            NodeKind::Expression(ref expr, ..) => {
                if let DataType::Inferred(..) = current_type {
                    *current_type = expr.data_type.to_mutable();
                }

                if let DataType::CoerceToString(_) | DataType::String(_) = current_type {
                    simplified_expression.push(node.to_owned());
                    continue 'outer;
                }

                output_queue.push(node.to_owned());
            }

            NodeKind::FunctionCall(..) => {
                simplified_expression.push(node.to_owned());
            }

            NodeKind::Operator(ref op) => {
                match current_type {
                    DataType::String(_) | DataType::Template(_) => {
                        if op != &Operator::Add {
                            return_syntax_error!(
                                node.location,
                                "You can't use the '{:?}' operator with strings or templates",
                                op
                            )
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
                return_compiler_error!("Unsupported AST node found in expression: {:?}", node.kind)
            }
        }
    }

    // If nothing to evaluate at compile time, just one value, return that value
    if simplified_expression.len() == 1 {
        return Ok(simplified_expression[0].get_expr()?);
    }

    match current_type {
        DataType::Template(_) | DataType::String(_) => concat_template(&mut simplified_expression),

        DataType::CoerceToString(_) => {
            let mut new_string = String::new();

            for node in simplified_expression {
                new_string += &node.get_expr()?.as_string();
            }

            Ok(Expression::string(new_string, location))
        }

        // At this stage, inferred should only be possible if only variables of unknown types
        // have been used in the expression.
        // So we need to mark this expression to be evaluated later on in the compiler once we know those types.
        // This can happen due to imports.
        DataType::Inferred(_) => {
            // If there were any explicit numerical types, then this will be passed to math_constant_fold.
            // This is just to skip calling that function if no numerical constants were found.
            Ok(Expression::runtime(
                simplified_expression,
                current_type.to_owned(),
                location,
            ))
        }

        _ => {
            // MATHS EXPRESSIONS
            // Push everything into the stack, is now in RPN notation
            while let Some(operator) = operators_stack.pop() {
                output_queue.push(operator);
            }

            eval_log!("Attempting to Fold: {:#?}", output_queue);

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
fn concat_template(simplified_expression: &mut Vec<AstNode>) -> Result<Expression, CompileError> {
    let mut template_body: TemplateContent = TemplateContent::default();
    let mut style = Style::default();

    // Should always be at least one node in the expression being evaluated
    let location = match simplified_expression.first() {
        Some(node) => node.location,
        None => return_compiler_error!("No nodes found in expression. This should never happen."),
    };

    for node in simplified_expression {
        match node.get_expr()?.kind {
            ExpressionKind::Template(body, ref mut template_style, ..) => {
                template_body.before.extend(body.before);
                template_body.after.extend(body.after);

                if !style.unlocks_override {
                    if template_style.unlocks_override {
                        style.unlocks_override = true;
                        style.unlocked_templates = template_style.unlocked_templates.to_owned();
                    } else {
                        style
                            .unlocked_templates
                            .extend(template_style.unlocked_templates.to_owned());
                    }
                }

                // TODO - scene style precedence
                // Some styles will override others based on their precedence
                style.format = template_style.format.to_owned();
                style.child_default = template_style.child_default.to_owned();
                style.compatibility = template_style.compatibility.to_owned();
                style.precedence = template_style.precedence.to_owned();
            }

            _ => {
                return_compiler_error!(
                    "Non-template value found in template expression (you can only concatenate templates with other templates)"
                )
            }
        }
    }

    Ok(Expression::template(
        template_body,
        style,
        String::new(),
        location,
    ))
}

// TODO - needs to check what can be concatenated at compile time
// Everything else should be left for runtime
// fn concat_strings(simplified_expression: &mut Vec<AstNode>) -> Result<Expression, CompileError> {
//     let mut final_string_expression: Vec<AstNode> = Vec::with_capacity(2);
//     for node in simplified_expression {
//         let expr = node.get_expr()?;
//         match expr.kind {
//             ExpressionKind::String(ref string) => {
//                 let mut last_node = final_string_expression.last();
//                 match &mut last_node.get_expr()?.kind {
//                     Some(AstNode::Expression(expr, ..)) => {
//                         expr.kind.evaluate_operator(
//                             &ExpressionKind::String(string.to_string()),
//                             &Operator::Add,
//                         );
//                     }
//                     _ => {
//                         final_string_expression.push(node.to_owned());
//                         final_string_expression.push(AstNode::Operator(Operator::Add));
//                     }
//                 }
//             }
//
//             ExpressionKind::Runtime(_) => final_string_expression.push(node.to_owned()),
//
//             ExpressionKind::Reference(..) => {
//                 final_string_expression
//                     .push(NodeKind::Operator(Operator::Add, node.get_position()));
//                 final_string_expression.push(node.to_owned());
//             }
//
//             _ => {
//                 return_type_error!(
//                     TextLocation::default(),
//                     "You can only concatenate strings with other strings or numbers. Found: {:?}",
//                     node,
//                 )
//             }
//         }
//     }
//
//     if final_string_expression.len() == 1 {
//         return Ok(final_string_expression[0].to_owned().get_expr());
//     }
//
//     Ok(ExpressionKind::Runtime(
//         final_string_expression,
//         DataType::String(false),
//     ))
// }
