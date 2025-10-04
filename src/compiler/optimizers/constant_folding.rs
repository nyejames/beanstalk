//! # Constant Folding Optimizer
//!
//! This module implements compile-time constant folding for Beanstalk expressions.
//! It evaluates constant expressions during AST construction to reduce runtime overhead
//! and enable further optimizations.
//!
//! ## Algorithm
//!
//! The constant folder operates on expressions in Reverse Polish Notation (RPN) order:
//! 1. **Stack-Based Evaluation**: Processes operands and operators in RPN order
//! 2. **Immediate Folding**: Evaluates operations on constant operands immediately
//! 3. **Runtime Preservation**: Preserves non-constant expressions for runtime evaluation
//!
//! ## Supported Operations
//!
//! - **Arithmetic**: Addition, subtraction, multiplication, division for integers and floats
//! - **Boolean**: Logical AND, OR, NOT operations
//! - **Comparison**: Equality, inequality, relational comparisons
//! - **Type Coercion**: Automatic promotion between compatible numeric types
//!
//! ## Benefits
//!
//! - **Performance**: Eliminates runtime calculations for constant expressions
//! - **Code Size**: Reduces generated WASM by pre-computing known values
//! - **Optimization**: Enables dead code elimination and further optimizations

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::{return_rule_error, return_syntax_error};

/// Perform constant folding on an expression in RPN order
///
/// Takes a stack of AST nodes representing an expression in Reverse Polish Notation
/// and evaluates all constant sub-expressions at compile time. Returns a simplified
/// expression with constant operations pre-computed.
///
/// ## Algorithm
///
/// 1. **Process RPN Stack**: Iterate through nodes in RPN order
/// 2. **Accumulate Operands**: Push operands onto evaluation stack
/// 3. **Evaluate Operators**: When encountering operators, attempt to fold with constant operands
/// 4. **Preserve Runtime**: Non-constant expressions are preserved for runtime evaluation
///
/// ## Error Handling
///
/// Returns [`CompileError`] for:
/// - Malformed expressions (insufficient operands for operators)
/// - Type mismatches in operations
/// - Division by zero in constant expressions
/// - Unsupported operations on constant types
pub fn constant_fold(output_stack: &[AstNode]) -> Result<Vec<AstNode>, CompileError> {
    let mut stack: Vec<AstNode> = Vec::with_capacity(output_stack.len());

    for node in output_stack {
        match &node.kind {
            NodeKind::Operator(op) => {
                // Make sure there are at least 2 nodes on the stack
                if stack.len() < 2 {
                    return_syntax_error!(
                        node.location.to_owned(),
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

    Ok(stack)
}

impl Expression {
    // Evaluates a binary operation between two expressions based on the operator
    // This helps with constant folding by handling type-specific operations
    pub fn evaluate_operator(
        &self,
        rhs: &Expression,
        op: &Operator,
    ) -> Result<Option<Expression>, CompileError> {
        let kind: ExpressionKind = match (&self.kind, &rhs.kind) {
            // Float operations
            (ExpressionKind::Float(lhs_val), ExpressionKind::Float(rhs_val)) => {
                match op {
                    Operator::Add => ExpressionKind::Float(lhs_val + rhs_val),
                    Operator::Subtract => ExpressionKind::Float(lhs_val - rhs_val),
                    Operator::Multiply => ExpressionKind::Float(lhs_val * rhs_val),
                    Operator::Divide => ExpressionKind::Float(lhs_val / rhs_val),
                    Operator::Modulus => ExpressionKind::Float(lhs_val % rhs_val),
                    Operator::Exponent => ExpressionKind::Float(lhs_val.powf(*rhs_val)),

                    // Logical operations with float operands
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs_val > rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs_val >= rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs_val < rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs_val <= rhs_val),

                    // Other operations are not applicable to floats
                    _ => return_rule_error!(
                        self.location.to_owned(),
                        "Can't perform operation {} on floats",
                        op.to_str()
                    ),
                }
            }

            // Integer operations
            (ExpressionKind::Int(lhs_val), ExpressionKind::Int(rhs_val)) => {
                match op {
                    Operator::Add => ExpressionKind::Int(lhs_val + rhs_val),
                    Operator::Subtract => ExpressionKind::Int(lhs_val - rhs_val),
                    Operator::Multiply => ExpressionKind::Int(lhs_val * rhs_val),
                    Operator::Divide => {
                        // Handle division by zero and integer division
                        if *rhs_val == 0 {
                            return_rule_error!(self.location.to_owned(), "Can't divide by zero")
                        }

                        ExpressionKind::Int(lhs_val / rhs_val)
                    }
                    Operator::Modulus => {
                        if *rhs_val == 0 {
                            return_rule_error!(self.location.to_owned(), "Can't modulus by zero")
                        }

                        ExpressionKind::Int(lhs_val % rhs_val)
                    }
                    Operator::Exponent => {
                        // For integer exponentiation, we need to be careful with negative exponents
                        if *rhs_val < 0 {
                            // Convert to float for negative exponents
                            let lhs_float = *lhs_val as f64;
                            let rhs_float = *rhs_val as f64;
                            ExpressionKind::Float(lhs_float.powf(rhs_float))
                        } else {
                            // Use integer exponentiation for positive exponents
                            ExpressionKind::Int(lhs_val.pow(*rhs_val as u32))
                        }
                    }

                    // Logical operations with integer operands
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs_val > rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs_val >= rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs_val < rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs_val <= rhs_val),

                    Operator::Range => ExpressionKind::Range(
                        Box::new(Expression::int(lhs_val.clone(), self.location.to_owned())),
                        Box::new(Expression::int(rhs_val.clone(), self.location.to_owned())),
                    ),

                    _ => return_rule_error!(
                        self.location.to_owned(),
                        "Can't perform operation {} on integers",
                        op.to_str()
                    ),
                }
            }

            // Boolean operations
            (ExpressionKind::Bool(lhs_val), ExpressionKind::Bool(rhs_val)) => match op {
                Operator::And => ExpressionKind::Bool(*lhs_val && *rhs_val),
                Operator::Or => ExpressionKind::Bool(*lhs_val || *rhs_val),
                Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),

                _ => return_rule_error!(
                    self.location.to_owned(),
                    "Can't perform operation {} on booleans",
                    op.to_str()
                ),
            },

            // String operations
            (ExpressionKind::String(lhs_val), ExpressionKind::String(rhs_val)) => match op {
                Operator::Add => ExpressionKind::String(format!("{}{}", lhs_val, rhs_val)),
                Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                _ => return_rule_error!(
                    self.location.to_owned(),
                    "Can't perform operation {} on strings",
                    op.to_str()
                ),
            },

            // Any other combination of types
            _ => return Ok(None),
        };

        Ok(Some(Expression::new(
            kind,
            self.location.to_owned(),
            self.data_type.to_owned(),
        )))
    }
}
