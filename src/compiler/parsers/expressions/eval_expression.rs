use crate::compiler::compiler_errors::CompileError;
use crate::compiler::optimizers::constant_folding::constant_fold;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind};
use crate::compiler::parsers::statements::create_template_node::Template;

use crate::compiler::datatypes::Ownership;
use crate::{
    compiler::datatypes::DataType, compiler::parsers::ast_nodes::NodeKind, eval_log,
    return_compiler_error, return_syntax_error,
};
use std::path::PathBuf;
use crate::compiler::parsers::tokenizer::tokens::TextLocation;

/**
 * Evaluates an abstract syntax tree (AST) expression using the shunting-yard algorithm
 * and other utility functions to produce the final `Expression` output.
 * This function also performs the bulk of the type checking in the compiler.
 *
 * # Parameters
 *
 * * `scope` - A `PathBuf` representing the current scope in which the evaluation is performed.
 * * `nodes` - A vector of `AstNode` which represents the sequence of nodes in the expression.
 * * `current_type` - A mutable reference to a `DataType`. Used to determine the data type of the evaluated result.
 *
 * # Returns
 *
 * If successful, returns an `Ok(Expression)` containing the resulting evaluated expression.
 * This will also be type safe.
 * If there is an error during evaluation, returns an `Err(CompileError)` with the appropriate error details.
 * If the expression wasn't folded, it will return a `Runtime` expression (ExpressionKind::Runtime) that contains AST nodes representing the expression.
 *
 * # Algorithm Details
 *
 * - Implements the **Shunting-Yard Algorithm** for parsing expressions and converting them to Reverse Polish Notation (RPN).
 * - Differentiates between handling strings, templates, function calls, and mathematical operators:
 *   - When dealing with strings and templates, it aggregates or processes the string results directly.
 *   - Mathematical operations are processed into RPN before evaluating.
 * - It folds constants where possible for optimization.
 * - Instead of handling parenthesis with shunting yard, every new set of parenthesis is parsed recursively using this function. The result of each nested expression is bubbled up to the first evaluate_expression call to be folded.
 *
 * # Error Handling
 *
 * - Ensures that unsupported AST nodes or invalid syntax result in a `CompileError`.
 * - Returns syntax errors related to invalid operator usage when types like `String` or `Template` are used improperly.
 * - Guards against inferred data types at runtime since they should already be resolved during type checking.
 *
 * # Workflow
 *
 * 1. Parse the AST `nodes` to create an `output_queue` and an `operators_stack`.
 * 2. Evaluate simple cases like single-node expressions directly for efficiency.
 * 3. For string and template expressions, handle concatenation or coercion to a string where necessary.
 * 4. For mathematical operations:
 *    - Convert to RPN via the shunting-yard algorithm.
 *    - Fold constant expressions for optimization.
 *    - Evaluate the resulting expression stack and return the final `Expression`.
 * 5. If no valid result is found, return an appropriate syntax error.
 *
 * # Example
 *
 * ```rust
 * use std::path::PathBuf;
 *
 * // Assume `nodes` is already parsed (parse_expression) and represents a valid AST expression
 * let scope = PathBuf::from("path/to/scope");
 * let mut current_type = DataType::Inferred;
 *
 * let result = evaluate_expression(scope, nodes, &mut current_type);
 *
 * match result {
 *     Ok(expression) => println!("Evaluated Expression: {:?}", expression),
 *     Err(e) => eprintln!("Compile Error: {:?}", e),
 * }
 * ```
 *
 * # Notes
 *
 * - The `eval_log!` macro is used throughout the function for debugging purposes.
 * - The `constant_fold` utility function is called to simplify the constant expressions where possible.
 * - Implements defensive checks for edge cases, such as invalid or unsupported AST nodes.
 */
pub fn evaluate_expression(
    scope: PathBuf,
    nodes: Vec<AstNode>,
    current_type: &mut DataType,
    ownership: &Ownership,
) -> Result<Expression, CompileError> {
    let mut simplified_expression: Vec<AstNode> = Vec::with_capacity(2);

    eval_log!("Evaluating expression: {:#?}", nodes);

    if nodes.is_empty() {
        return_compiler_error!("No nodes found in expression. This should never happen.");
    }

    // SHUNTING YARD ALGORITHM
    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();

    // Should always be at least one node in the expression being evaluated
    let location = extract_location(&nodes)?;

    'outer: for node in nodes {
        match node.kind {
            NodeKind::Expression(ref expr, ..) => {
                if let DataType::Inferred = current_type {
                    *current_type = expr.data_type.to_owned();
                }

                if let DataType::CoerceToString | DataType::String = current_type {
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
                    DataType::String | DataType::Template => {
                        return_syntax_error!(
                            node.location,
                            "You can't use the '{:?}' operator with strings or templates",
                            op
                        )
                    }

                    DataType::CoerceToString => {
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

    // If nothing to evaluate at compile time, just one value, return that value.
    // If the value is a reference, then the data type needs to indicate this is a mutable / immutable reference to something else,
    // or a copy of the value if explicitly copied.
    if simplified_expression.len() == 1 {
        let only_expression = simplified_expression[0].get_expr()?;

        if let ExpressionKind::Reference(..) = only_expression.kind {
            // The current type now becomes a reference (basically a safe pointer rather than a value)
            *current_type = DataType::Reference(
                Box::from(only_expression.data_type.to_owned()),
                ownership.to_owned(),
            );
        }

        return Ok(only_expression);
    }

    // Since there is more than one value in this expression,
    // it will copy the values and become Owned if not already.
    let ownership = ownership.get_owned();

    match current_type {
        DataType::Template | DataType::String => {
            concat_template(&mut simplified_expression, ownership.get_owned())
        }

        DataType::CoerceToString => {
            let mut new_string = String::new();

            // red_ln!("Treating this as simplified exp: {:#?}", simplified_expression);

            for node in simplified_expression {
                new_string += &node.get_expr()?.as_string();
            }

            Ok(Expression::string_slice(new_string, location, ownership))
        }

        DataType::Inferred => {
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
                ownership,
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
fn concat_template(
    simplified_expression: &mut Vec<AstNode>,
    ownership: Ownership,
) -> Result<Expression, CompileError> {
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

    Ok(Expression::template(template, ownership))
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
