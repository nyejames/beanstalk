//! Expression ordering helpers for AST evaluation.
//!
//! WHAT: converts a flat infix fragment into RPN using a shunting-yard pass.
//! WHY: operator typing and folding need deterministic precedence/associativity ordering before
//! they can validate or reduce the expression.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::{eval_log, return_compiler_error};

/// Order a parsed expression fragment into RPN and return the expression source location anchor.
pub(super) fn order_expression_nodes(
    nodes: Vec<AstNode>,
) -> Result<(Vec<AstNode>, SourceLocation), CompilerError> {
    if nodes.is_empty() {
        return_compiler_error!("No nodes found in expression. This should never happen.");
    }

    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operator_stack: Vec<AstNode> = Vec::new();
    let location = extract_expression_location(&nodes)?;

    // The parser already handled parentheses recursively, so this pass only needs to order the
    // flat infix fragment by precedence and associativity before typing/folding it.

    for node in nodes {
        eval_log!("Evaluating node in expression: ", Pretty node);
        match &node.kind {
            NodeKind::Rvalue(..)
            | NodeKind::FieldAccess { .. }
            | NodeKind::FunctionCall { .. }
            | NodeKind::ResultHandledFunctionCall { .. }
            | NodeKind::MethodCall { .. }
            | NodeKind::CollectionBuiltinCall { .. }
            | NodeKind::HostFunctionCall { .. } => {
                output_queue.push(node);
            }

            NodeKind::Operator(..) => {
                let node_precedence = node.get_precedence();
                let left_associative = node.is_left_associative();

                pop_higher_precedence(
                    &mut operator_stack,
                    &mut output_queue,
                    node_precedence,
                    left_associative,
                )?;

                operator_stack.push(node);
            }

            _ => {
                return_compiler_error!(format!(
                    "Unsupported AST node found in expression: {:?}",
                    node.kind
                ))
            }
        }
    }

    while let Some(operator) = operator_stack.pop() {
        output_queue.push(operator);
    }

    Ok((output_queue, location))
}

// Standard shunting-yard pop rule: earlier operators leave the stack when they bind at least
// as tightly as the new operator, adjusted for right-associative cases like exponentiation.
fn pop_higher_precedence(
    operator_stack: &mut Vec<AstNode>,
    output_queue: &mut Vec<AstNode>,
    current_precedence: u32,
    left_associative: bool,
) -> Result<(), CompilerError> {
    while let Some(top_operator_node) = operator_stack.last() {
        let existing_precedence = top_operator_node.get_precedence();

        let should_pop = if left_associative {
            existing_precedence >= current_precedence
        } else {
            existing_precedence > current_precedence
        };

        if should_pop {
            let Some(operator) = operator_stack.pop() else {
                return_compiler_error!(
                    "Expression ordering lost operator stack state during shunting-yard pop."
                );
            };
            output_queue.push(operator);
        } else {
            break;
        }
    }

    Ok(())
}

pub(super) fn extract_expression_location(
    nodes: &[AstNode],
) -> Result<SourceLocation, CompilerError> {
    if nodes.is_empty() {
        return_compiler_error!("No nodes found in expression. This should never happen.");
    }

    // Skip operator nodes and return the location of the first expression node.
    for node in nodes {
        if !matches!(node.kind, NodeKind::Operator(_)) {
            return Ok(node.location.to_owned());
        }
    }

    // Fallback to first node if all nodes are operators (should not happen).
    Ok(nodes[0].location.to_owned())
}
