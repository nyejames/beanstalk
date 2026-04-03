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

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    BuiltinCastKind, Expression, ExpressionKind, Operator, ResultCallHandling, ResultVariant,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
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
/// Returns [`CompilerError`] for:
/// - Malformed expressions (insufficient operands for operators)
/// - Type mismatches in operations
/// - Division by zero in constant expressions
/// - Unsupported operations on constant types
pub fn constant_fold(
    output_stack: &[AstNode],
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    // If any operand is runtime-dependent, keep the original RPN intact.
    // Partial folding can invalidate operand/operator ordering for chained runtime expressions.
    if output_stack.iter().any(|node| {
        matches!(
            &node.kind,
            NodeKind::Rvalue(expr) if !expr.kind.is_foldable()
        ) || !matches!(&node.kind, NodeKind::Rvalue(_) | NodeKind::Operator(_))
    }) {
        return Ok(output_stack.to_vec());
    }

    let mut stack: Vec<AstNode> = Vec::with_capacity(output_stack.len());

    for node in output_stack {
        match &node.kind {
            NodeKind::Operator(op) => {
                let required_values = op.required_values();
                // Make sure there are at least 2 nodes on the stack if it's a binary operator
                if stack.len() < required_values {
                    return_syntax_error!(
                        format!(
                            "Not enough nodes on the stack for the {} operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}",
                            op.to_str(),
                            output_stack,
                            stack
                        ),
                        node.location.to_owned()
                    )
                }

                if matches!(op, Operator::Not) {
                    let mut boolean = stack
                        .pop()
                        .expect("unary NOT should have one operand after the stack-length guard");
                    if !boolean.flip(string_table)? {
                        stack.push(boolean);
                        stack.push(node.to_owned());
                    } else {
                        stack.push(boolean)
                    }

                    continue;
                }

                let rhs = stack
                    .pop()
                    .expect("binary operator should have a right operand after the length guard");
                let lhs = stack
                    .pop()
                    .expect("binary operator should have a left operand after the length guard");

                let (lhs_expr, rhs_expr) = match (&lhs.kind, &rhs.kind) {
                    (NodeKind::Rvalue(lhs_expr), NodeKind::Rvalue(rhs_expr))
                        if lhs_expr.kind.is_foldable() && rhs_expr.kind.is_foldable() =>
                    {
                        (lhs_expr, rhs_expr)
                    }
                    _ => {
                        // Preserve runtime RPN when either side is not foldable.
                        stack.push(lhs);
                        stack.push(rhs);
                        stack.push(node.to_owned());
                        continue;
                    }
                };

                // Try to evaluate the operation
                if let Some(result) = lhs_expr.evaluate_operator(rhs_expr, op, string_table)? {
                    // Successfully evaluated - push a result onto the stack
                    let new_literal = AstNode {
                        kind: NodeKind::Rvalue(result.to_owned()),
                        location: result.location,
                        scope: node.scope.clone(),
                    };
                    stack.push(new_literal);
                } else {
                    // Not foldable at this compile time stage, push back to stack as runtime expression
                    stack.push(lhs);
                    stack.push(rhs);
                    stack.push(node.to_owned());
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

pub fn fold_compile_time_expression(
    expression: &Expression,
    string_table: &mut StringTable,
    constant_context: bool,
) -> Result<Expression, CompilerError> {
    match &expression.kind {
        ExpressionKind::BuiltinCast { kind, value } => {
            let folded_value = fold_compile_time_expression(value, string_table, constant_context)?;
            fold_builtin_cast(
                expression,
                *kind,
                &folded_value,
                string_table,
                constant_context,
            )
        }
        ExpressionKind::HandledResult { value, handling } => {
            let folded_value = fold_compile_time_expression(value, string_table, constant_context)?;

            match &folded_value.kind {
                ExpressionKind::ResultConstruct {
                    variant: ResultVariant::Ok,
                    value,
                } => Ok(value.as_ref().to_owned()),
                ExpressionKind::ResultConstruct {
                    variant: ResultVariant::Err,
                    ..
                } => match handling {
                    ResultCallHandling::Fallback(fallbacks) if fallbacks.len() == 1 => {
                        fold_compile_time_expression(&fallbacks[0], string_table, constant_context)
                    }
                    _ => Ok(Expression::handled_result(
                        folded_value,
                        handling.to_owned(),
                        expression.location.clone(),
                    )),
                },
                _ => Ok(Expression::handled_result(
                    folded_value,
                    handling.to_owned(),
                    expression.location.clone(),
                )),
            }
        }
        _ => Ok(expression.to_owned()),
    }
}

fn fold_builtin_cast(
    original_expression: &Expression,
    kind: BuiltinCastKind,
    value: &Expression,
    string_table: &mut StringTable,
    constant_context: bool,
) -> Result<Expression, CompilerError> {
    let cast_result = match kind {
        BuiltinCastKind::Int => eval_int_cast(value, string_table),
        BuiltinCastKind::Float => eval_float_cast(value, string_table),
    };

    match cast_result {
        Ok(folded_value) => Ok(Expression::result_construct(
            ResultVariant::Ok,
            folded_value,
            original_expression.data_type.to_owned(),
            original_expression.location.clone(),
            Ownership::ImmutableOwned,
        )),
        Err(error) if constant_context => {
            return_rule_error!(error, original_expression.location.clone())
        }
        Err(_) => Ok(original_expression.to_owned()),
    }
}

fn eval_int_cast(value: &Expression, string_table: &StringTable) -> Result<Expression, String> {
    match &value.kind {
        ExpressionKind::Int(int) => Ok(Expression::int(
            *int,
            value.location.clone(),
            Ownership::ImmutableOwned,
        )),
        ExpressionKind::Float(float) => {
            if float.fract() == 0.0 {
                Ok(Expression::int(
                    *float as i64,
                    value.location.clone(),
                    Ownership::ImmutableOwned,
                ))
            } else {
                Err(format!(
                    "Cannot cast Float {} to Int because it is not an exact integer value",
                    float
                ))
            }
        }
        ExpressionKind::StringSlice(string) => {
            let raw = string_table.resolve(*string);
            let normalized = normalize_numeric_cast_text(raw);

            if is_signed_integer_text(&normalized) {
                let parsed = normalized
                    .parse::<i64>()
                    .map_err(|_| format!("Cannot parse '{}' as Int", raw))?;
                return Ok(Expression::int(
                    parsed,
                    value.location.clone(),
                    Ownership::ImmutableOwned,
                ));
            }

            if is_signed_decimal_text(&normalized) {
                let parsed = normalized
                    .parse::<f64>()
                    .map_err(|_| format!("Cannot parse '{}' as Int", raw))?;
                if parsed.fract() == 0.0 {
                    return Ok(Expression::int(
                        parsed as i64,
                        value.location.clone(),
                        Ownership::ImmutableOwned,
                    ));
                }
                return Err(format!(
                    "Cannot cast Float {} to Int because it is not an exact integer value",
                    normalized
                ));
            }

            Err(format!("Cannot parse '{}' as Int", raw))
        }
        _ => Err("Int(...) only accepts Int, Float, or string values".to_string()),
    }
}

fn eval_float_cast(value: &Expression, string_table: &StringTable) -> Result<Expression, String> {
    match &value.kind {
        ExpressionKind::Float(float) => Ok(Expression::float(
            *float,
            value.location.clone(),
            Ownership::ImmutableOwned,
        )),
        ExpressionKind::Int(int) => Ok(Expression::float(
            *int as f64,
            value.location.clone(),
            Ownership::ImmutableOwned,
        )),
        ExpressionKind::StringSlice(string) => {
            let raw = string_table.resolve(*string);
            let normalized = normalize_numeric_cast_text(raw);

            if is_signed_integer_text(&normalized) || is_signed_decimal_text(&normalized) {
                let parsed = normalized
                    .parse::<f64>()
                    .map_err(|_| format!("Cannot parse '{}' as Float", raw))?;
                return Ok(Expression::float(
                    parsed,
                    value.location.clone(),
                    Ownership::ImmutableOwned,
                ));
            }

            Err(format!("Cannot parse '{}' as Float", raw))
        }
        _ => Err("Float(...) only accepts Int, Float, or string values".to_string()),
    }
}

fn normalize_numeric_cast_text(raw: &str) -> String {
    raw.trim().chars().filter(|ch| *ch != '_').collect()
}

fn is_signed_integer_text(raw: &str) -> bool {
    let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    !digits.is_empty() && digits.chars().all(|ch| ch.is_ascii_digit())
}

fn is_signed_decimal_text(raw: &str) -> bool {
    let digits = raw.strip_prefix(['+', '-']).unwrap_or(raw);
    let Some((left, right)) = digits.split_once('.') else {
        return false;
    };

    !left.is_empty()
        && !right.is_empty()
        && left.chars().all(|ch| ch.is_ascii_digit())
        && right.chars().all(|ch| ch.is_ascii_digit())
}

impl Expression {
    // Evaluates a binary operation between two expressions based on the operator
    // This helps with constant folding by handling type-specific operations
    pub fn evaluate_operator(
        &self,
        rhs: &Expression,
        op: &Operator,
        string_table: &mut StringTable,
    ) -> Result<Option<Expression>, CompilerError> {
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
                        format!("Can't perform operation {} on floats", op.to_str()),
                        self.location.to_owned()
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
                            return_rule_error!("Can't divide by zero", self.location.to_owned())
                        }

                        ExpressionKind::Int(lhs_val / rhs_val)
                    }
                    Operator::Modulus => {
                        if *rhs_val == 0 {
                            return_rule_error!("Can't modulus by zero", self.location.to_owned())
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
                        Box::new(Expression::int(
                            *lhs_val,
                            self.location.to_owned(),
                            Ownership::ImmutableOwned,
                        )),
                        Box::new(Expression::int(
                            *rhs_val,
                            self.location.to_owned(),
                            Ownership::ImmutableOwned,
                        )),
                    ),

                    _ => return_rule_error!(
                        format!("Can't perform operation {} on integers", op.to_str()),
                        self.location.to_owned()
                    ),
                }
            }

            (ExpressionKind::Int(lhs_val), ExpressionKind::Float(rhs_val)) => {
                let lhs = *lhs_val as f64;
                match op {
                    Operator::Add => ExpressionKind::Float(lhs + rhs_val),
                    Operator::Subtract => ExpressionKind::Float(lhs - rhs_val),
                    Operator::Multiply => ExpressionKind::Float(lhs * rhs_val),
                    Operator::Divide => ExpressionKind::Float(lhs / rhs_val),
                    Operator::Modulus => ExpressionKind::Float(lhs % rhs_val),
                    Operator::Exponent => ExpressionKind::Float(lhs.powf(*rhs_val)),
                    Operator::Equality => ExpressionKind::Bool(lhs == *rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs != *rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs > *rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs >= *rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs < *rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs <= *rhs_val),
                    _ => return_rule_error!(
                        format!("Can't perform operation {} on Int and Float", op.to_str()),
                        self.location.to_owned()
                    ),
                }
            }

            (ExpressionKind::Float(lhs_val), ExpressionKind::Int(rhs_val)) => {
                let rhs = *rhs_val as f64;
                match op {
                    Operator::Add => ExpressionKind::Float(lhs_val + rhs),
                    Operator::Subtract => ExpressionKind::Float(lhs_val - rhs),
                    Operator::Multiply => ExpressionKind::Float(lhs_val * rhs),
                    Operator::Divide => ExpressionKind::Float(lhs_val / rhs),
                    Operator::Modulus => ExpressionKind::Float(lhs_val % rhs),
                    Operator::Exponent => ExpressionKind::Float(lhs_val.powf(rhs)),
                    Operator::Equality => ExpressionKind::Bool(*lhs_val == rhs),
                    Operator::NotEqual => ExpressionKind::Bool(*lhs_val != rhs),
                    Operator::GreaterThan => ExpressionKind::Bool(*lhs_val > rhs),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(*lhs_val >= rhs),
                    Operator::LessThan => ExpressionKind::Bool(*lhs_val < rhs),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(*lhs_val <= rhs),
                    _ => return_rule_error!(
                        format!("Can't perform operation {} on Float and Int", op.to_str()),
                        self.location.to_owned()
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
                    format!("Can't perform operation {} on booleans", op.to_str()),
                    self.location.to_owned()
                ),
            },

            // String operations
            (ExpressionKind::StringSlice(lhs_val), ExpressionKind::StringSlice(rhs_val)) => {
                match op {
                    Operator::Add => {
                        // Resolve both interned strings, concatenate, and intern the result
                        let lhs_str = string_table.resolve(*lhs_val);
                        let rhs_str = string_table.resolve(*rhs_val);
                        let concatenated = format!("{}{}", lhs_str, rhs_str);
                        let interned_result = string_table.get_or_intern(concatenated);
                        ExpressionKind::StringSlice(interned_result)
                    }
                    Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                    _ => return_rule_error!(
                        format!("Can't perform operation {} on strings", op.to_str()),
                        self.location.to_owned()
                    ),
                }
            }
            // Any other combination of types
            _ => return Ok(None),
        };

        let ownership = if self.ownership.is_mutable() || rhs.ownership.is_mutable() {
            Ownership::MutableOwned
        } else {
            Ownership::ImmutableOwned
        };

        let result_type = match &kind {
            ExpressionKind::Int(_) => DataType::Int,
            ExpressionKind::Float(_) => DataType::Float,
            ExpressionKind::Bool(_) => DataType::Bool,
            ExpressionKind::StringSlice(_) => DataType::StringSlice,
            ExpressionKind::Range(_, _) => DataType::Range,
            ExpressionKind::Char(_) => DataType::Char,
            _ => self.data_type.to_owned(),
        };

        Ok(Some(Expression::new(
            kind,
            self.location.to_owned(),
            result_type,
            ownership,
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler_frontend::string_interning::StringTable;

    #[test]
    fn evaluate_operator_concatenates_string_literals() {
        let mut string_table = StringTable::new();
        let lhs = Expression::string_slice(
            string_table.intern("bean"),
            Default::default(),
            Ownership::ImmutableOwned,
        );
        let rhs = Expression::string_slice(
            string_table.intern("stalk"),
            Default::default(),
            Ownership::ImmutableOwned,
        );

        let result = lhs
            .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
            .expect("string concatenation should succeed")
            .expect("string concatenation should fold");

        assert!(matches!(result.kind, ExpressionKind::StringSlice(_)));
        let ExpressionKind::StringSlice(interned) = result.kind else {
            unreachable!("checked above");
        };
        assert_eq!(string_table.resolve(interned), "beanstalk");
    }

    #[test]
    fn evaluate_operator_promotes_negative_integer_exponent_to_float() {
        let mut string_table = StringTable::new();
        let lhs = Expression::int(2, Default::default(), Ownership::ImmutableOwned);
        let rhs = Expression::int(-1, Default::default(), Ownership::ImmutableOwned);

        let result = lhs
            .evaluate_operator(&rhs, &Operator::Exponent, &mut string_table)
            .expect("integer exponentiation should succeed")
            .expect("integer exponentiation should fold");

        assert!(
            matches!(result.kind, ExpressionKind::Float(value) if (value - 0.5).abs() < f64::EPSILON)
        );
        assert_eq!(result.data_type, DataType::Float);
    }

    #[test]
    fn evaluate_operator_returns_none_for_mismatched_constant_types() {
        let mut string_table = StringTable::new();
        let lhs = Expression::int(2, Default::default(), Ownership::ImmutableOwned);
        let rhs = Expression::bool(true, Default::default(), Ownership::ImmutableOwned);

        let result = lhs
            .evaluate_operator(&rhs, &Operator::Add, &mut string_table)
            .expect("mismatched types should not error");

        assert!(result.is_none());
    }
}
