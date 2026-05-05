//! # Constant Folding Optimizer
//!
//! WHAT: folds fully compile-time expression fragments during AST construction.
//! WHY: conservative folding keeps runtime semantics stable while still collapsing obvious
//! literal operations before HIR lowering.
//!
//! ## Algorithm
//!
//! The constant folder operates on expressions in Reverse Polish Notation (RPN) order:
//! 1. **Stack-Based Evaluation**: Processes operands and operators in RPN order
//! 2. **Immediate Folding**: Evaluates operations only when required operands are foldable literals
//! 3. **Runtime Preservation**: Keeps non-foldable expressions in runtime RPN form
//!
//! ## Supported Operations
//!
//! - **Arithmetic**: Addition, subtraction, multiplication, division for integers and floats
//! - **Boolean**: Logical AND, OR, NOT operations
//! - **Comparison**: Equality, inequality, relational comparisons
//! - **Type Coercion**: Automatic promotion between compatible numeric types
//!
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{
    BuiltinCastKind, Expression, ExpressionKind, Operator, ResultCallHandling, ResultVariant,
};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{return_rule_error, return_syntax_error};

const CONSTANT_FOLDING_STAGE: &str = "Constant Folding";
const INTEGER_OVERFLOW_SUGGESTION: &str =
    "Reduce the value range or compute this at runtime instead";
const FLOAT_NON_FINITE_SUGGESTION: &str = "Use smaller values or compute this at runtime instead";

/// Result of constant folding over an RPN expression stack.
///
/// WHAT: distinguishes expressions that were fully or partially folded from those
///       that stayed unchanged, so callers can avoid cloning runtime RPN.
pub enum ConstantFoldResult {
    /// The expression contained runtime-dependent operands and the original RPN is unchanged.
    Unchanged,
    /// The expression was fully or partially reduced; the contained stack is the new RPN.
    Folded(Vec<AstNode>),
}

/// Perform conservative constant folding on an expression in RPN order.
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
/// 4. **Preserve Runtime**: Non-foldable operations are preserved for runtime evaluation
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
) -> Result<ConstantFoldResult, CompilerError> {
    // If any operand is runtime-dependent, keep the original RPN intact.
    // Partial folding can invalidate operand/operator ordering for chained runtime expressions.
    if output_stack.iter().any(|node| {
        matches!(
            &node.kind,
            NodeKind::Rvalue(expr) if !expr.kind.is_foldable()
        ) || !matches!(&node.kind, NodeKind::Rvalue(_) | NodeKind::Operator(_))
    }) {
        return Ok(ConstantFoldResult::Unchanged);
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
                    let operand = stack
                        .pop()
                        .expect("unary NOT should have one operand after the stack-length guard");

                    if let NodeKind::Rvalue(expression) = &operand.kind
                        && let ExpressionKind::Bool(value) = expression.kind
                    {
                        let folded = AstNode {
                            kind: NodeKind::Rvalue(Expression::bool(
                                !value,
                                expression.location.clone(),
                                expression.value_mode.to_owned(),
                            )),
                            location: operand.location.to_owned(),
                            scope: operand.scope.clone(),
                        };
                        stack.push(folded);
                    } else {
                        // Keep unary-not as runtime RPN when operand is not a boolean literal.
                        stack.push(operand);
                        stack.push(node.to_owned());
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

    Ok(ConstantFoldResult::Folded(stack))
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
            ValueMode::ImmutableOwned,
        )),
        Err(error) if constant_context => {
            return_rule_error!(error, original_expression.location.clone())
        }
        Err(_) => Ok(original_expression.to_owned()),
    }
}

fn integer_overflow_error(
    op: &Operator,
    location: &SourceLocation,
) -> Result<ExpressionKind, CompilerError> {
    return_rule_error!(
        format!(
            "Compile-time integer overflow while evaluating '{}'",
            op.to_str()
        ),
        location.to_owned(),
        {
            CompilationStage => CONSTANT_FOLDING_STAGE,
            PrimarySuggestion => INTEGER_OVERFLOW_SUGGESTION,
        }
    )
}

fn float_non_finite_error(
    op: &Operator,
    location: &SourceLocation,
) -> Result<ExpressionKind, CompilerError> {
    return_rule_error!(
        format!(
            "Compile-time float overflow or non-finite result while evaluating '{}'",
            op.to_str()
        ),
        location.to_owned(),
        {
            CompilationStage => CONSTANT_FOLDING_STAGE,
            PrimarySuggestion => FLOAT_NON_FINITE_SUGGESTION,
        }
    )
}

fn checked_float_result(
    value: f64,
    op: &Operator,
    location: &SourceLocation,
) -> Result<ExpressionKind, CompilerError> {
    if value.is_finite() {
        Ok(ExpressionKind::Float(value))
    } else {
        float_non_finite_error(op, location)
    }
}

fn checked_int_binary_result(
    lhs: i64,
    rhs: i64,
    op: &Operator,
    location: &SourceLocation,
) -> Result<ExpressionKind, CompilerError> {
    let checked = match op {
        Operator::Add => lhs.checked_add(rhs),
        Operator::Subtract => lhs.checked_sub(rhs),
        Operator::Multiply => lhs.checked_mul(rhs),
        Operator::IntDivide => lhs.checked_div(rhs),
        Operator::Modulus => lhs.checked_rem(rhs),
        Operator::Exponent => lhs.checked_pow(rhs as u32),
        _ => {
            return_rule_error!(
                format!("Checked integer folding does not support '{}'", op.to_str()),
                location.to_owned(),
                {
                    CompilationStage => CONSTANT_FOLDING_STAGE,
                    PrimarySuggestion => "This is a compiler bug - please report it",
                }
            )
        }
    };

    match checked {
        Some(value) => Ok(ExpressionKind::Int(value)),
        None => integer_overflow_error(op, location),
    }
}

fn float_to_int_cast_result(float: f64, display: &str) -> Result<i64, String> {
    if !float.is_finite() {
        return Err(format!(
            "Cannot cast Float {display} to Int because it is not finite"
        ));
    }

    if float.fract() != 0.0 {
        return Err(format!(
            "Cannot cast Float {display} to Int because it is not an exact integer value"
        ));
    }

    if float < i64::MIN as f64 || float >= i64::MAX as f64 {
        return Err(format!(
            "Cannot cast Float {display} to Int because it exceeds Int range"
        ));
    }

    Ok(float as i64)
}

fn eval_int_cast(value: &Expression, string_table: &StringTable) -> Result<Expression, String> {
    match &value.kind {
        ExpressionKind::Int(int) => Ok(Expression::int(
            *int,
            value.location.clone(),
            ValueMode::ImmutableOwned,
        )),
        ExpressionKind::Float(float) => Ok(Expression::int(
            float_to_int_cast_result(*float, &float.to_string())?,
            value.location.clone(),
            ValueMode::ImmutableOwned,
        )),
        ExpressionKind::StringSlice(string) => {
            let raw = string_table.resolve(*string);
            let normalized = normalize_numeric_cast_text(raw);

            if is_signed_integer_text(&normalized) {
                let parsed = normalized
                    .parse::<i64>()
                    .map_err(|_| format!("Cannot parse '{raw}' as Int"))?;
                return Ok(Expression::int(
                    parsed,
                    value.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            if is_signed_decimal_text(&normalized) {
                let parsed = normalized
                    .parse::<f64>()
                    .map_err(|_| format!("Cannot parse '{raw}' as Int"))?;
                return Ok(Expression::int(
                    float_to_int_cast_result(parsed, &normalized)?,
                    value.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            Err(format!("Cannot parse '{raw}' as Int"))
        }
        _ => Err("Int(...) only accepts Int, Float, or string values".to_string()),
    }
}

fn eval_float_cast(value: &Expression, string_table: &StringTable) -> Result<Expression, String> {
    match &value.kind {
        ExpressionKind::Float(float) => Ok(Expression::float(
            *float,
            value.location.clone(),
            ValueMode::ImmutableOwned,
        )),
        ExpressionKind::Int(int) => Ok(Expression::float(
            *int as f64,
            value.location.clone(),
            ValueMode::ImmutableOwned,
        )),
        ExpressionKind::StringSlice(string) => {
            let raw = string_table.resolve(*string);
            let normalized = normalize_numeric_cast_text(raw);

            if is_signed_integer_text(&normalized) || is_signed_decimal_text(&normalized) {
                let parsed = normalized
                    .parse::<f64>()
                    .map_err(|_| format!("Cannot parse '{raw}' as Float"))?;
                if !parsed.is_finite() {
                    return Err(format!(
                        "Cannot parse '{raw}' as Float because it is not finite"
                    ));
                }
                return Ok(Expression::float(
                    parsed,
                    value.location.clone(),
                    ValueMode::ImmutableOwned,
                ));
            }

            Err(format!("Cannot parse '{raw}' as Float"))
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
            (ExpressionKind::Float(lhs_val), ExpressionKind::Float(rhs_val)) => match op {
                Operator::Add => checked_float_result(lhs_val + rhs_val, op, &self.location)?,
                Operator::Subtract => checked_float_result(lhs_val - rhs_val, op, &self.location)?,
                Operator::Multiply => checked_float_result(lhs_val * rhs_val, op, &self.location)?,
                Operator::Divide => {
                    if *rhs_val == 0.0 {
                        return_rule_error!("Can't divide by zero", self.location.to_owned())
                    }
                    checked_float_result(lhs_val / rhs_val, op, &self.location)?
                }
                Operator::Modulus => checked_float_result(lhs_val % rhs_val, op, &self.location)?,
                Operator::Exponent => {
                    checked_float_result(lhs_val.powf(*rhs_val), op, &self.location)?
                }

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
            },

            // Integer operations
            (ExpressionKind::Int(lhs_val), ExpressionKind::Int(rhs_val)) => match op {
                Operator::Add | Operator::Subtract | Operator::Multiply => {
                    checked_int_binary_result(*lhs_val, *rhs_val, op, &self.location)?
                }
                Operator::Divide => {
                    if *rhs_val == 0 {
                        return_rule_error!("Can't divide by zero", self.location.to_owned())
                    }

                    checked_float_result(*lhs_val as f64 / *rhs_val as f64, op, &self.location)?
                }
                Operator::IntDivide => {
                    if *rhs_val == 0 {
                        return_rule_error!("Can't divide by zero", self.location.to_owned())
                    }

                    checked_int_binary_result(*lhs_val, *rhs_val, op, &self.location)?
                }
                Operator::Modulus => {
                    if *rhs_val == 0 {
                        return_rule_error!("Can't modulus by zero", self.location.to_owned())
                    }

                    checked_int_binary_result(*lhs_val, *rhs_val, op, &self.location)?
                }
                Operator::Exponent => {
                    if *rhs_val < 0 {
                        checked_float_result(
                            (*lhs_val as f64).powf(*rhs_val as f64),
                            op,
                            &self.location,
                        )?
                    } else {
                        checked_int_binary_result(*lhs_val, *rhs_val, op, &self.location)?
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
                        ValueMode::ImmutableOwned,
                    )),
                    Box::new(Expression::int(
                        *rhs_val,
                        self.location.to_owned(),
                        ValueMode::ImmutableOwned,
                    )),
                ),

                _ => return_rule_error!(
                    format!("Can't perform operation {} on integers", op.to_str()),
                    self.location.to_owned()
                ),
            },

            (ExpressionKind::Int(lhs_val), ExpressionKind::Float(rhs_val)) => {
                let lhs = *lhs_val as f64;
                match op {
                    Operator::Add => checked_float_result(lhs + rhs_val, op, &self.location)?,
                    Operator::Subtract => checked_float_result(lhs - rhs_val, op, &self.location)?,
                    Operator::Multiply => checked_float_result(lhs * rhs_val, op, &self.location)?,
                    Operator::Divide => {
                        if *rhs_val == 0.0 {
                            return_rule_error!("Can't divide by zero", self.location.to_owned())
                        }
                        checked_float_result(lhs / rhs_val, op, &self.location)?
                    }
                    Operator::Modulus => checked_float_result(lhs % rhs_val, op, &self.location)?,
                    Operator::Exponent => {
                        checked_float_result(lhs.powf(*rhs_val), op, &self.location)?
                    }
                    Operator::Equality => ExpressionKind::Bool(lhs == *rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs != *rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs > *rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs >= *rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs < *rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs <= *rhs_val),
                    Operator::IntDivide => {
                        return_rule_error!(
                            "Integer division operator '//' only supports Int and Int operands",
                            self.location.to_owned()
                        )
                    }
                    _ => return_rule_error!(
                        format!("Can't perform operation {} on Int and Float", op.to_str()),
                        self.location.to_owned()
                    ),
                }
            }

            (ExpressionKind::Float(lhs_val), ExpressionKind::Int(rhs_val)) => {
                let rhs = *rhs_val as f64;
                match op {
                    Operator::Add => checked_float_result(lhs_val + rhs, op, &self.location)?,
                    Operator::Subtract => checked_float_result(lhs_val - rhs, op, &self.location)?,
                    Operator::Multiply => checked_float_result(lhs_val * rhs, op, &self.location)?,
                    Operator::Divide => {
                        if *rhs_val == 0 {
                            return_rule_error!("Can't divide by zero", self.location.to_owned())
                        }
                        checked_float_result(lhs_val / rhs, op, &self.location)?
                    }
                    Operator::Modulus => checked_float_result(lhs_val % rhs, op, &self.location)?,
                    Operator::Exponent => {
                        checked_float_result(lhs_val.powf(rhs), op, &self.location)?
                    }
                    Operator::Equality => ExpressionKind::Bool(*lhs_val == rhs),
                    Operator::NotEqual => ExpressionKind::Bool(*lhs_val != rhs),
                    Operator::GreaterThan => ExpressionKind::Bool(*lhs_val > rhs),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(*lhs_val >= rhs),
                    Operator::LessThan => ExpressionKind::Bool(*lhs_val < rhs),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(*lhs_val <= rhs),
                    Operator::IntDivide => {
                        return_rule_error!(
                            "Integer division operator '//' only supports Int and Int operands",
                            self.location.to_owned()
                        )
                    }
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
                        let concatenated = format!("{lhs_str}{rhs_str}");
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

        let value_mode = if self.value_mode.is_mutable() || rhs.value_mode.is_mutable() {
            ValueMode::MutableOwned
        } else {
            ValueMode::ImmutableOwned
        };
        let contains_regular_division = self.contains_regular_division
            || rhs.contains_regular_division
            || matches!(op, Operator::Divide);

        let result_type = match &kind {
            ExpressionKind::Int(_) => DataType::Int,
            ExpressionKind::Float(_) => DataType::Float,
            ExpressionKind::Bool(_) => DataType::Bool,
            ExpressionKind::StringSlice(_) => DataType::StringSlice,
            ExpressionKind::Range(_, _) => DataType::Range,
            ExpressionKind::Char(_) => DataType::Char,
            _ => self.data_type.to_owned(),
        };

        Ok(Some(
            Expression::new(kind, self.location.to_owned(), result_type, value_mode)
                .with_regular_division_provenance(contains_regular_division),
        ))
    }
}

#[cfg(test)]
#[path = "tests/constant_folding_tests.rs"]
mod tests;
