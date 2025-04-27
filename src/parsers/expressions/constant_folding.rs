use crate::parsers::ast_nodes::{Expr, Operator};
use crate::{CompileError, ErrorType, bs_types::DataType, parsers::ast_nodes::AstNode};

#[allow(unused_imports)]
use colour::{blue_ln, green_ln_bold, red_ln};

// This will evaluate everything possible at compile time
// returns either a literal or an evaluated runtime expression
// Output stack must be in RPN order
pub fn constant_fold(
    output_stack: Vec<AstNode>,
    current_type: DataType,
) -> Result<Expr, CompileError> {
    let mut stack: Vec<AstNode> = Vec::new();
    let mut first_line_number = 0;

    for node in &output_stack {
        match node {
            AstNode::Operator(op, token_position) => {

                if token_position.line_number != first_line_number {
                    first_line_number = token_position.line_number;
                }

                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return Err(CompileError {
                        msg: format!(
                            "Not enough nodes on the stack for binary operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}",
                            output_stack, stack
                        ),
                        start_pos: token_position.to_owned(),
                        end_pos: token_position.to_owned(),
                        error_type: ErrorType::Syntax,
                    });
                }

                let rhs = stack.pop().unwrap();
                let lhs = stack.pop().unwrap();

                let lhs_expr = lhs.get_value();
                let rhs_expr = rhs.get_value();
                
                // Check if both operands are literals that can be folded
                // We can only fold constants at compile time
                let is_foldable_literal = matches!(
                    (&lhs_expr, &rhs_expr),
                    // Numeric types 
                    (Expr::Float(_), _) | (Expr::Int(_), _) | 
                    (_, Expr::Float(_)) | (_, Expr::Int(_)) |
                    // Boolean types
                    (Expr::Bool(_), _) | (_, Expr::Bool(_)) |
                    // String types  
                    (Expr::String(_), _) | (_, Expr::String(_)) |
                    // None type
                    (Expr::None, _) | (_, Expr::None)
                );
                
                // Special case for logical operators - they always work on literals regardless of type
                let is_logical_op = matches!(
                    op,
                    Operator::Equality | Operator::NotEqual | 
                    Operator::And | Operator::Or |
                    Operator::GreaterThan | Operator::GreaterThanOrEqual |
                    Operator::LessThan | Operator::LessThanOrEqual
                );
                
                // We can fold if they're both literals, or if it's a logical operation on any types
                if !is_foldable_literal && !is_logical_op {
                    // Not foldable at compile time, push back to stack as runtime expression
                    stack.push(lhs);
                    stack.push(rhs);
                    stack.push(node.to_owned());
                    continue;
                }
                
                // Try to evaluate the operation
                if let Some(result) = lhs_expr.evaluate_operator(&rhs_expr, op) {
                    // Successfully evaluated - push a result onto the stack
                    let new_literal = AstNode::Literal(
                        result,
                        token_position.to_owned(),
                    );
                    stack.push(new_literal);
                } else {
                    
                    // Operation isn't supported between these specific types
                    return Err(CompileError {
                        msg: format!(
                            "Cannot apply {:?} operator to values of types {:?} and {:?}.",
                            op,
                            lhs_expr,
                            rhs_expr
                        ),
                        start_pos: token_position.to_owned(),
                        end_pos: token_position.to_owned(),
                        error_type: ErrorType::Type,
                    });
                }
            }

            // Literal or anything else
            _ => {
                stack.push(node.to_owned());
            }
        }
    }

    if stack.len() == 1 {
        return Ok(stack[0].get_value());
    }

    if stack.is_empty() {
        return Ok(Expr::None);
    }

    Ok(Expr::Runtime(stack, current_type))
}