use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::{Expression, ExpressionKind, Operator};
use crate::{return_rule_error, return_syntax_error};
#[allow(unused_imports)]
use colour::{blue_ln, green_ln_bold, red_ln};

// This will evaluate everything possible at compile time
// returns either a literal or an evaluated runtime expression
// Output stack must be in RPN order
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
            (ExpressionKind::ConstFloat(lhs_val), ExpressionKind::ConstFloat(rhs_val)) => {
                match op {
                    Operator::Add => ExpressionKind::ConstFloat(lhs_val + rhs_val),
                    Operator::Subtract => ExpressionKind::ConstFloat(lhs_val - rhs_val),
                    Operator::Multiply => ExpressionKind::ConstFloat(lhs_val * rhs_val),
                    Operator::Divide => ExpressionKind::ConstFloat(lhs_val / rhs_val),
                    Operator::Modulus => ExpressionKind::ConstFloat(lhs_val % rhs_val),
                    Operator::Exponent => ExpressionKind::ConstFloat(lhs_val.powf(*rhs_val)),

                    // Logical operations with float operands
                    Operator::Equality => ExpressionKind::ConstBool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::ConstBool(lhs_val != rhs_val),
                    Operator::GreaterThan => ExpressionKind::ConstBool(lhs_val > rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::ConstBool(lhs_val >= rhs_val),
                    Operator::LessThan => ExpressionKind::ConstBool(lhs_val < rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::ConstBool(lhs_val <= rhs_val),

                    // Other operations are not applicable to floats
                    _ => return_rule_error!(
                        self.location.to_owned(),
                        "Can't perform operation {} on floats",
                        op.to_str()
                    ),
                }
            }

            // Integer operations
            (ExpressionKind::ConstInt(lhs_val), ExpressionKind::ConstInt(rhs_val)) => {
                match op {
                    Operator::Add => ExpressionKind::ConstInt(lhs_val + rhs_val),
                    Operator::Subtract => ExpressionKind::ConstInt(lhs_val - rhs_val),
                    Operator::Multiply => ExpressionKind::ConstInt(lhs_val * rhs_val),
                    Operator::Divide => {
                        // Handle division by zero and integer division
                        if *rhs_val == 0 {
                            return_rule_error!(self.location.to_owned(), "Can't divide by zero")
                        }

                        ExpressionKind::ConstInt(lhs_val / rhs_val)
                    }
                    Operator::Modulus => {
                        if *rhs_val == 0 {
                            return_rule_error!(self.location.to_owned(), "Can't modulus by zero")
                        }

                        ExpressionKind::ConstInt(lhs_val % rhs_val)
                    }
                    Operator::Exponent => {
                        // For integer exponentiation, we need to be careful with negative exponents
                        if *rhs_val < 0 {
                            // Convert to float for negative exponents
                            let lhs_float = *lhs_val as f64;
                            let rhs_float = *rhs_val as f64;
                            ExpressionKind::ConstFloat(lhs_float.powf(rhs_float))
                        } else {
                            // Use integer exponentiation for positive exponents
                            ExpressionKind::ConstInt(lhs_val.pow(*rhs_val as u32))
                        }
                    }

                    // Logical operations with integer operands
                    Operator::Equality => ExpressionKind::ConstBool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::ConstBool(lhs_val != rhs_val),
                    Operator::GreaterThan => ExpressionKind::ConstBool(lhs_val > rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::ConstBool(lhs_val >= rhs_val),
                    Operator::LessThan => ExpressionKind::ConstBool(lhs_val < rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::ConstBool(lhs_val <= rhs_val),

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
            (ExpressionKind::ConstBool(lhs_val), ExpressionKind::ConstBool(rhs_val)) => match op {
                Operator::And => ExpressionKind::ConstBool(*lhs_val && *rhs_val),
                Operator::Or => ExpressionKind::ConstBool(*lhs_val || *rhs_val),
                Operator::Equality => ExpressionKind::ConstBool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::ConstBool(lhs_val != rhs_val),

                _ => return_rule_error!(
                    self.location.to_owned(),
                    "Can't perform operation {} on booleans",
                    op.to_str()
                ),
            },

            // String operations
            (ExpressionKind::ConstString(lhs_val), ExpressionKind::ConstString(rhs_val)) => match op {
                Operator::Add => ExpressionKind::ConstString(format!("{}{}", lhs_val, rhs_val)),
                Operator::Equality => ExpressionKind::ConstBool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::ConstBool(lhs_val != rhs_val),
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
