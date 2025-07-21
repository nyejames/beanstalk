use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::tokens::TextLocation;
use crate::return_syntax_error;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln_bold, red_ln};

// This will evaluate everything possible at compile time
// returns either a literal or an evaluated runtime expression
// Output stack must be in RPN order
pub fn constant_fold(
    output_stack: Vec<AstNode>,
    current_type: DataType,
) -> Result<Expression, CompileError> {
    let mut stack: Vec<AstNode> = Vec::new();

    for node in &output_stack {
        match &node.kind {
            NodeKind::Operator(op) => {
                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return_syntax_error!(
                        node.location,
                        "Not enough nodes on the stack for binary operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}",
                        output_stack,
                        stack
                    )
                }

                let rhs = stack.pop().unwrap();
                let lhs = stack.pop().unwrap();

                let lhs_expr = lhs.get_expr()?;
                let rhs_expr = rhs.get_expr()?;

                // // We can fold if they're both literals
                // if !lhs_expr.is_foldable() || !rhs_expr.is_foldable() {
                //     // Not foldable at compile time, push back to stack as runtime expression
                //     stack.push(lhs);
                //     stack.push(rhs);
                //     stack.push(node.to_owned());
                //     continue;
                // }

                // Try to evaluate the operation
                if let Some(result) = lhs_expr.evaluate_operator(&rhs_expr, op)? {
                    // Successfully evaluated - push a result onto the stack
                    let new_literal = AstNode {
                        kind: NodeKind::Expression(result.to_owned()),
                        location: result.location,
                        scope: node.scope.clone(),
                    };
                    stack.push(new_literal);
                } else {
                    // Not foldable at this compile time stage, push back to stack as runtime expression
                    stack.push(lhs);
                    stack.push(node.to_owned());
                    stack.push(rhs);
                    continue;
                }
            }

            // Literal or anything else
            _ => {
                stack.push(node.to_owned());
            }
        }
    }

    if stack.len() == 1 {
        return Ok(stack[0].get_expr()?);
    }

    if stack.is_empty() {
        return Ok(Expression::none());
    }

    // Safe because of the previous two if statements.
    let first_node_start = stack[0].location.start_pos;
    let last_node_end = stack[stack.len() - 1].location.end_pos;

    Ok(Expression::runtime(
        stack,
        current_type,
        TextLocation::new(first_node_start, last_node_end),
    ))
}
