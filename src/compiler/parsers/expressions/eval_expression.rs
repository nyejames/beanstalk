use crate::compiler::compiler_errors::CompileError;
use crate::compiler::optimizers::constant_folding::constant_fold;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::create_template_node::Template;

use crate::compiler::parsers::tokens::TextLocation;
use crate::{
    compiler::datatypes::DataType, compiler::parsers::ast_nodes::NodeKind, eval_log,
    return_compiler_error, return_syntax_error,
};
use std::path::PathBuf;

pub fn evaluate_expression(
    scope: PathBuf,
    nodes: Vec<AstNode>,
    current_type: &mut DataType,
) -> Result<Expression, CompileError> {
    let mut simplified_expression: Vec<AstNode> = Vec::with_capacity(2);

    eval_log!("Evaluating expression: {:#?}", nodes);

    // SHUNTING YARD ALGORITHM
    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();

    // Should always be at least one node in the expression being evaluated
    let location = extract_location(&nodes)?;

    'outer: for node in nodes {
        match node.kind {
            NodeKind::Expression(ref expr, ..) => {
                if let DataType::Inferred(..) = current_type {
                    *current_type = expr.data_type.to_compiler_owned();
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
                        return_syntax_error!(
                            node.location,
                            "You can't use the '{:?}' operator with strings or templates",
                            op
                        )
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
        return simplified_expression[0].get_expr();
    }

    match current_type {
        DataType::Template(_) | DataType::String(_) => concat_template(&mut simplified_expression),

        DataType::CoerceToString(_) => {
            let mut new_string = String::new();

            // red_ln!("Treating this as simplified exp: {:#?}", simplified_expression);

            for node in simplified_expression {
                new_string += &node.get_expr()?.as_string();
            }

            Ok(Expression::string(new_string, location))
        }

        DataType::Inferred(..) => {
            return_compiler_error!(
                "Inferred data type made it into eval_expression! Everything should be type checked by now"
            )
        }

        _ => {
            // MATHS EXPRESSIONS
            // Push everything into the stack, is now in RPN notation
            while let Some(operator) = operators_stack.pop() {
                output_queue.push(operator);
            }

            eval_log!("Attempting to Fold: {:#?}", output_queue);

            // Evaluate all constants in the maths expression
            let stack = constant_fold(&output_queue)?;

            eval_log!("Stack after folding: {:#?}", stack);

            if stack.len() == 1 {
                return stack[0].get_expr();
            }

            if stack.is_empty() {
                return_syntax_error!(
                    TextLocation::default(),
                    "Invalid expression: no valid operands found during evaluation."
                );
            }

            // Safe because of the previous two if statements.
            let first_node_start = stack[0].location.start_pos;
            let last_node_end = stack[stack.len() - 1].location.end_pos;

            Ok(Expression::runtime(
                stack,
                current_type.to_owned(),
                TextLocation::new(scope, first_node_start, last_node_end),
            ))
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
    let mut template: Template = Template::create_default(None);
    let _location = extract_location(simplified_expression)?;

    for node in simplified_expression {
        match node.get_expr()?.kind {
            ExpressionKind::Template(template_to_concat) => {
                template
                    .content
                    .before
                    .extend(template_to_concat.content.before);
                template
                    .content
                    .after
                    .extend(template_to_concat.content.after);

                if !template.style.unlocks_override {
                    if template_to_concat.style.unlocks_override {
                        template.style.unlocks_override = true;
                        template.style.unlocked_templates =
                            template_to_concat.style.unlocked_templates.to_owned();
                    } else {
                        template
                            .style
                            .unlocked_templates
                            .extend(template_to_concat.style.unlocked_templates.to_owned());
                    }
                }

                // TODO - scene style precedence
                // Some styles will override others based on their precedence
                template.style.formatter = template_to_concat.style.formatter.to_owned();
                template.style.child_default = template_to_concat.style.child_default.to_owned();
                template.style.compatibility = template_to_concat.style.compatibility.to_owned();
                template.style.override_precedence =
                    template_to_concat.style.override_precedence.to_owned();
            }

            _ => {
                return_compiler_error!(
                    "Non-template value found in template expression (you can only concatenate templates with other templates)"
                )
            }
        }
    }

    Ok(Expression::template(template))
}

fn extract_location(nodes: &[AstNode]) -> Result<TextLocation, CompileError> {
    // TODO: Just in case the first node is an operator or something
    // This should PROBABLY iterate through until it hits the first expression node
    match nodes.first() {
        Some(node) => Ok(node.location.to_owned()),
        None => return_compiler_error!("No nodes found in expression. This should never happen."),
    }
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
