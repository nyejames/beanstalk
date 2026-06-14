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
    Expression, ExpressionKind, ExpressionValueShape, FallibleCarrierVariant, Operator,
    type_id_hint_for_diagnostic_type,
};
use crate::compiler_frontend::ast::expressions::expression_kind::ResolvedCastExpression;
use crate::compiler_frontend::ast::expressions::expression_types::{
    CastHandling, FallibleHandling, ResolvedCastEvidence,
};
use crate::compiler_frontend::builtins::casts::{BuiltinCastLiteral, apply_builtin_cast_policy};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompileTimeEvaluationErrorReason, CompilerDiagnostic, InvalidCastReason,
};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::value_mode::ValueMode;

#[derive(Debug)]
pub(crate) enum ConstantFoldError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl From<CompilerDiagnostic> for ConstantFoldError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        ConstantFoldError::Diagnostic(Box::new(diagnostic))
    }
}

impl From<CompilerError> for ConstantFoldError {
    fn from(error: CompilerError) -> Self {
        ConstantFoldError::Infrastructure(Box::new(error))
    }
}

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
/// Returns [`ConstantFoldError`] so compile-time source failures stay as
/// [`CompilerDiagnostic`] values while malformed internal RPN state remains an
/// infrastructure [`CompilerError`]. This keeps constant-folding diagnostics on the
/// AST-owned user-diagnostic path without hiding compiler invariants.
pub fn constant_fold(
    output_stack: &[AstNode],
    string_table: &mut StringTable,
) -> Result<ConstantFoldResult, ConstantFoldError> {
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
                // Validate stack arity before popping operands so malformed RPN is reported as an
                // internal compiler invariant failure instead of panicking.
                if stack.len() < required_values {
                    return Err(CompilerError::new(
                        format!(
                            "Not enough nodes on the stack for the {} operator when parsing an expression. Starting Stack: {:?}. Stack being folded: {:?}",
                            op.to_str(),
                            output_stack,
                            stack
                        ),
                        node.location.to_owned(),
                        ErrorType::Compiler,
                    )
                    .into());
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
) -> Result<Expression, ConstantFoldError> {
    match &expression.kind {
        ExpressionKind::Cast(cast) => {
            let folded_source =
                fold_compile_time_expression(&cast.source, string_table, constant_context)?;
            fold_resolved_cast(
                expression,
                cast,
                &folded_source,
                string_table,
                constant_context,
            )
        }
        ExpressionKind::HandledFallibleExpression { value, handling } => {
            let folded_value = fold_compile_time_expression(value, string_table, constant_context)?;

            match &folded_value.kind {
                ExpressionKind::FallibleCarrierConstruct {
                    variant: FallibleCarrierVariant::Success,
                    value,
                } => Ok(value.as_ref().to_owned()),
                ExpressionKind::FallibleCarrierConstruct {
                    variant: FallibleCarrierVariant::Error,
                    ..
                } => Ok(Expression::handled_result_with_type_id(
                    folded_value,
                    handling.to_owned(),
                    expression.type_id,
                    expression.diagnostic_type.to_owned(),
                    expression.location.clone(),
                )),
                _ => Ok(Expression::handled_result_with_type_id(
                    folded_value,
                    handling.to_owned(),
                    expression.type_id,
                    expression.diagnostic_type.to_owned(),
                    expression.location.clone(),
                )),
            }
        }
        _ => Ok(expression.to_owned()),
    }
}

/// Folds a resolved explicit `ExpressionKind::Cast` when its source has folded to
/// a supported builtin literal.
///
/// WHAT: builtin evidence is evaluated here; user-defined or generic-bound
///      evidence is rejected in const-required contexts because the compiler
///      cannot execute user code or validate an unresolved generic bound at
///      compile time.
/// WHY: keeping this logic in the constant folder means HIR lowering only sees
///      runtime casts that could not be folded away.
fn fold_resolved_cast(
    original_expression: &Expression,
    cast: &ResolvedCastExpression,
    folded_source: &Expression,
    string_table: &mut StringTable,
    constant_context: bool,
) -> Result<Expression, ConstantFoldError> {
    match &cast.evidence {
        ResolvedCastEvidence::Builtin { policy } => {
            if !policy.is_const_foldable() {
                if constant_context {
                    return Err(CompilerDiagnostic::invalid_cast(
                        InvalidCastReason::BuiltinEvidenceNotConstFoldable,
                        Some(cast.source_type_id),
                        Some(cast.target_type_id),
                        original_expression.location.clone(),
                    )
                    .into());
                }

                return Ok(original_expression.to_owned());
            }

            let source_literal =
                match builtin_cast_literal_from_expression(folded_source, string_table) {
                    Some(literal) => literal,
                    None => return Ok(original_expression.to_owned()),
                };

            match apply_builtin_cast_policy(*policy, &source_literal) {
                Ok(folded_literal) => {
                    let Some(mut folded_expression) = builtin_cast_expression_from_literal(
                        &folded_literal,
                        &folded_source.location,
                        string_table,
                    ) else {
                        return Ok(original_expression.to_owned());
                    };

                    if cast.requires_optional_wrap_after_cast {
                        folded_expression =
                            Expression::coerced(folded_expression, original_expression.type_id);
                    }

                    Ok(folded_expression)
                }
                Err(_) if !constant_context => Ok(original_expression.to_owned()),
                Err(_) => {
                    // A const-required fallible cast with a local recovery handler should
                    // fold to the handler's produced value when the source folded but the
                    // builtin policy failed. If the handler itself cannot fold, report that
                    // as a separate diagnostic so the user knows the recovery path is the
                    // remaining obstacle.
                    if let CastHandling::Recover(FallibleHandling::Handler { body, .. }) =
                        &cast.handling
                        && let Some(folded_handler) = fold_cast_recovery_handler(
                            body,
                            cast.target_type_id,
                            cast.requires_optional_wrap_after_cast,
                            original_expression.type_id,
                            &original_expression.location,
                            string_table,
                        )?
                    {
                        return Ok(folded_handler);
                    }

                    Err(CompilerDiagnostic::invalid_cast(
                        InvalidCastReason::BuiltinCastFailedInConst,
                        Some(cast.source_type_id),
                        Some(cast.target_type_id),
                        original_expression.location.clone(),
                    )
                    .into())
                }
            }
        }

        ResolvedCastEvidence::UserDefined { .. } => {
            if constant_context {
                return Err(CompilerDiagnostic::invalid_cast(
                    InvalidCastReason::UserDefinedEvidenceNotConstFoldable,
                    Some(cast.source_type_id),
                    Some(cast.target_type_id),
                    original_expression.location.clone(),
                )
                .into());
            }

            Ok(original_expression.to_owned())
        }

        ResolvedCastEvidence::GenericBound { .. } => {
            if constant_context {
                return Err(CompilerDiagnostic::invalid_cast(
                    InvalidCastReason::GenericBoundEvidenceNotConstFoldable,
                    Some(cast.source_type_id),
                    Some(cast.target_type_id),
                    original_expression.location.clone(),
                )
                .into());
            }

            Ok(original_expression.to_owned())
        }
    }
}

/// Folds a `cast ... catch:` handler body to its produced value in a const-required context.
///
/// WHAT: when a builtin cast failed at compile time, the handler body is the only remaining
///      source for the result. This helper extracts the single produced value, folds it, and
///      returns it if it collapsed to a compile-time value. If the handler cannot be folded, it
///      reports a dedicated diagnostic so the failure is attributed to the recovery path, not the
///      cast.
/// WHY: keeping this small and local to the constant folder means HIR lowering does not need to
///      interpret general catch handler bodies at compile time.
fn fold_cast_recovery_handler(
    handler_body: &[AstNode],
    target_type_id: TypeId,
    requires_optional_wrap_after_cast: bool,
    result_type_id: TypeId,
    diagnostic_location: &SourceLocation,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, ConstantFoldError> {
    let Some(handler_expression) = extract_single_produced_value(handler_body) else {
        return Err(CompilerDiagnostic::invalid_cast(
            InvalidCastReason::CatchHandlerNotConstFoldable,
            None,
            Some(target_type_id),
            diagnostic_location.to_owned(),
        )
        .into());
    };

    let folded_handler = fold_compile_time_expression(handler_expression, string_table, true)?;

    if !folded_handler.is_compile_time_constant() {
        return Err(CompilerDiagnostic::invalid_cast(
            InvalidCastReason::CatchHandlerNotConstFoldable,
            None,
            Some(target_type_id),
            folded_handler.location,
        )
        .into());
    }

    let mut result = folded_handler;
    if requires_optional_wrap_after_cast {
        result = Expression::coerced(result, result_type_id);
    }

    Ok(Some(result))
}

/// Extracts a direct single-value `then` expression from a value-producing body.
///
/// WHAT: catch handlers for cast recovery must produce exactly one value in a shape the constant
///      folder can evaluate without interpreting statements or control-flow conditions.
/// WHY: nested `if` or `match` handlers require real const statement evaluation to choose the
///      executed branch. Until that owner exists, the safe frontend behavior is to reject those
///      handlers in const-required casts instead of guessing at the first branch.
fn extract_single_produced_value(body: &[AstNode]) -> Option<&Expression> {
    for node in body {
        match &node.kind {
            NodeKind::ThenValue(produced_values) if produced_values.expressions.len() == 1 => {
                return Some(&produced_values.expressions[0]);
            }

            NodeKind::If(..)
            | NodeKind::Match { .. }
            | NodeKind::ScopedBlock { .. }
            | NodeKind::RangeLoop { .. }
            | NodeKind::CollectionLoop { .. }
            | NodeKind::WhileLoop(..)
            | NodeKind::Return(_)
            | NodeKind::ReturnError(_) => return None,

            _ => {}
        }
    }

    None
}

/// Converts an AST `Expression` into a `BuiltinCastLiteral` for policy lookup.
///
/// WHAT: extracts the literal scalar value from supported `ExpressionKind`
///      variants so the policy owner can answer in policy space.
/// WHY: explicit casts and direct policy tests share the same policy table, so this
///      narrow converter is reused for any folded scalar source that the builtin
///      evidence catalogue accepts.
fn builtin_cast_literal_from_expression(
    value: &Expression,
    string_table: &StringTable,
) -> Option<BuiltinCastLiteral> {
    match &value.kind {
        ExpressionKind::Bool(value) => Some(BuiltinCastLiteral::Bool(*value)),
        ExpressionKind::Int(int) => Some(BuiltinCastLiteral::Int(*int)),
        ExpressionKind::Float(float) => Some(BuiltinCastLiteral::Float(*float)),
        ExpressionKind::StringSlice(string) => Some(BuiltinCastLiteral::String(
            string_table.resolve(*string).to_owned(),
        )),
        ExpressionKind::Char(value) => Some(BuiltinCastLiteral::Char(*value)),
        _ => None,
    }
}

/// Builds an `Expression` literal from a `BuiltinCastLiteral`.
fn builtin_cast_expression_from_literal(
    literal: &BuiltinCastLiteral,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> Option<Expression> {
    match literal {
        BuiltinCastLiteral::Bool(value) => Some(Expression::bool(
            *value,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        BuiltinCastLiteral::Int(value) => Some(Expression::int(
            *value,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        BuiltinCastLiteral::Float(value) => Some(Expression::float(
            *value,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        BuiltinCastLiteral::String(value) => Some(Expression::string_slice(
            string_table.get_or_intern(value.to_owned()),
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        BuiltinCastLiteral::Char(value) => Some(Expression::char(
            *value,
            location.clone(),
            ValueMode::ImmutableOwned,
        )),
        BuiltinCastLiteral::Error { .. } => None,
    }
}

fn compile_time_evaluation_diagnostic(
    reason: CompileTimeEvaluationErrorReason,
    operation: Option<String>,
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> ConstantFoldError {
    let operation = operation.map(|operation| string_table.get_or_intern(operation));

    CompilerDiagnostic::compile_time_evaluation_error(reason, operation, location.to_owned()).into()
}

fn integer_overflow_error(
    op: &Operator,
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    Err(compile_time_evaluation_diagnostic(
        CompileTimeEvaluationErrorReason::IntegerOverflow,
        Some(op.to_str().to_string()),
        string_table,
        location,
    ))
}

fn float_non_finite_error(
    op: &Operator,
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    Err(compile_time_evaluation_diagnostic(
        CompileTimeEvaluationErrorReason::FloatOverflow,
        Some(op.to_str().to_string()),
        string_table,
        location,
    ))
}

fn checked_float_result(
    value: f64,
    op: &Operator,
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    if value.is_finite() {
        Ok(ExpressionKind::Float(value))
    } else {
        float_non_finite_error(op, string_table, location)
    }
}

fn checked_int_binary_result(
    lhs: i64,
    rhs: i64,
    op: &Operator,
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    let checked = match op {
        Operator::Add => lhs.checked_add(rhs),
        Operator::Subtract => lhs.checked_sub(rhs),
        Operator::Multiply => lhs.checked_mul(rhs),
        Operator::IntDivide => lhs.checked_div(rhs),
        Operator::Modulus => lhs.checked_rem(rhs),
        Operator::Exponent => lhs.checked_pow(rhs as u32),
        _ => {
            return Err(CompilerError::compiler_error(format!(
                "Checked integer folding does not support '{}'",
                op.to_str()
            ))
            .into());
        }
    };

    match checked {
        Some(value) => Ok(ExpressionKind::Int(value)),
        None => integer_overflow_error(op, string_table, location),
    }
}

fn divide_by_zero_error(
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    Err(compile_time_evaluation_diagnostic(
        CompileTimeEvaluationErrorReason::DivideByZero,
        None,
        string_table,
        location,
    ))
}

fn integer_division_operand_error(
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    Err(compile_time_evaluation_diagnostic(
        CompileTimeEvaluationErrorReason::IntegerDivisionOnlyIntInt,
        None,
        string_table,
        location,
    ))
}

fn invalid_operator_for_compile_time_type(
    op: &Operator,
    string_table: &mut StringTable,
    location: &SourceLocation,
) -> Result<ExpressionKind, ConstantFoldError> {
    Err(compile_time_evaluation_diagnostic(
        CompileTimeEvaluationErrorReason::InvalidOperatorForType,
        Some(op.to_str().to_string()),
        string_table,
        location,
    ))
}

impl Expression {
    // Evaluates a binary operation between two expressions based on the operator
    // This helps with constant folding by handling type-specific operations
    pub(crate) fn evaluate_operator(
        &self,
        rhs: &Expression,
        op: &Operator,
        string_table: &mut StringTable,
    ) -> Result<Option<Expression>, ConstantFoldError> {
        let kind: ExpressionKind = match (&self.kind, &rhs.kind) {
            // Float operations
            (ExpressionKind::Float(lhs_val), ExpressionKind::Float(rhs_val)) => match op {
                Operator::Add => {
                    checked_float_result(lhs_val + rhs_val, op, string_table, &self.location)?
                }
                Operator::Subtract => {
                    checked_float_result(lhs_val - rhs_val, op, string_table, &self.location)?
                }
                Operator::Multiply => {
                    checked_float_result(lhs_val * rhs_val, op, string_table, &self.location)?
                }
                Operator::Divide => {
                    if *rhs_val == 0.0 {
                        divide_by_zero_error(string_table, &self.location)?
                    } else {
                        checked_float_result(lhs_val / rhs_val, op, string_table, &self.location)?
                    }
                }
                Operator::Modulus => {
                    checked_float_result(lhs_val % rhs_val, op, string_table, &self.location)?
                }
                Operator::Exponent => {
                    checked_float_result(lhs_val.powf(*rhs_val), op, string_table, &self.location)?
                }

                // Logical operations with float operands
                Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),
                Operator::GreaterThan => ExpressionKind::Bool(lhs_val > rhs_val),
                Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs_val >= rhs_val),
                Operator::LessThan => ExpressionKind::Bool(lhs_val < rhs_val),
                Operator::LessThanOrEqual => ExpressionKind::Bool(lhs_val <= rhs_val),

                // Other operations are not applicable to floats.
                _ => invalid_operator_for_compile_time_type(op, string_table, &self.location)?,
            },

            // Integer operations
            (ExpressionKind::Int(lhs_val), ExpressionKind::Int(rhs_val)) => match op {
                Operator::Add | Operator::Subtract | Operator::Multiply => {
                    checked_int_binary_result(*lhs_val, *rhs_val, op, string_table, &self.location)?
                }
                Operator::Divide => {
                    if *rhs_val == 0 {
                        divide_by_zero_error(string_table, &self.location)?
                    } else {
                        checked_float_result(
                            *lhs_val as f64 / *rhs_val as f64,
                            op,
                            string_table,
                            &self.location,
                        )?
                    }
                }
                Operator::IntDivide => {
                    if *rhs_val == 0 {
                        divide_by_zero_error(string_table, &self.location)?
                    } else {
                        checked_int_binary_result(
                            *lhs_val,
                            *rhs_val,
                            op,
                            string_table,
                            &self.location,
                        )?
                    }
                }
                Operator::Modulus => {
                    if *rhs_val == 0 {
                        divide_by_zero_error(string_table, &self.location)?
                    } else {
                        checked_int_binary_result(
                            *lhs_val,
                            *rhs_val,
                            op,
                            string_table,
                            &self.location,
                        )?
                    }
                }
                Operator::Exponent => {
                    if *rhs_val < 0 {
                        checked_float_result(
                            (*lhs_val as f64).powf(*rhs_val as f64),
                            op,
                            string_table,
                            &self.location,
                        )?
                    } else {
                        checked_int_binary_result(
                            *lhs_val,
                            *rhs_val,
                            op,
                            string_table,
                            &self.location,
                        )?
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

                _ => invalid_operator_for_compile_time_type(op, string_table, &self.location)?,
            },

            (ExpressionKind::Int(lhs_val), ExpressionKind::Float(rhs_val)) => {
                let lhs = *lhs_val as f64;
                match op {
                    Operator::Add => {
                        checked_float_result(lhs + rhs_val, op, string_table, &self.location)?
                    }
                    Operator::Subtract => {
                        checked_float_result(lhs - rhs_val, op, string_table, &self.location)?
                    }
                    Operator::Multiply => {
                        checked_float_result(lhs * rhs_val, op, string_table, &self.location)?
                    }
                    Operator::Divide => {
                        if *rhs_val == 0.0 {
                            divide_by_zero_error(string_table, &self.location)?
                        } else {
                            checked_float_result(lhs / rhs_val, op, string_table, &self.location)?
                        }
                    }
                    Operator::Modulus => {
                        checked_float_result(lhs % rhs_val, op, string_table, &self.location)?
                    }
                    Operator::Exponent => {
                        checked_float_result(lhs.powf(*rhs_val), op, string_table, &self.location)?
                    }
                    Operator::Equality => ExpressionKind::Bool(lhs == *rhs_val),
                    Operator::NotEqual => ExpressionKind::Bool(lhs != *rhs_val),
                    Operator::GreaterThan => ExpressionKind::Bool(lhs > *rhs_val),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(lhs >= *rhs_val),
                    Operator::LessThan => ExpressionKind::Bool(lhs < *rhs_val),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(lhs <= *rhs_val),
                    Operator::IntDivide => {
                        integer_division_operand_error(string_table, &self.location)?
                    }
                    _ => invalid_operator_for_compile_time_type(op, string_table, &self.location)?,
                }
            }

            (ExpressionKind::Float(lhs_val), ExpressionKind::Int(rhs_val)) => {
                let rhs = *rhs_val as f64;
                match op {
                    Operator::Add => {
                        checked_float_result(lhs_val + rhs, op, string_table, &self.location)?
                    }
                    Operator::Subtract => {
                        checked_float_result(lhs_val - rhs, op, string_table, &self.location)?
                    }
                    Operator::Multiply => {
                        checked_float_result(lhs_val * rhs, op, string_table, &self.location)?
                    }
                    Operator::Divide => {
                        if *rhs_val == 0 {
                            divide_by_zero_error(string_table, &self.location)?
                        } else {
                            checked_float_result(lhs_val / rhs, op, string_table, &self.location)?
                        }
                    }
                    Operator::Modulus => {
                        checked_float_result(lhs_val % rhs, op, string_table, &self.location)?
                    }
                    Operator::Exponent => {
                        checked_float_result(lhs_val.powf(rhs), op, string_table, &self.location)?
                    }
                    Operator::Equality => ExpressionKind::Bool(*lhs_val == rhs),
                    Operator::NotEqual => ExpressionKind::Bool(*lhs_val != rhs),
                    Operator::GreaterThan => ExpressionKind::Bool(*lhs_val > rhs),
                    Operator::GreaterThanOrEqual => ExpressionKind::Bool(*lhs_val >= rhs),
                    Operator::LessThan => ExpressionKind::Bool(*lhs_val < rhs),
                    Operator::LessThanOrEqual => ExpressionKind::Bool(*lhs_val <= rhs),
                    Operator::IntDivide => {
                        integer_division_operand_error(string_table, &self.location)?
                    }
                    _ => invalid_operator_for_compile_time_type(op, string_table, &self.location)?,
                }
            }

            // Boolean operations
            (ExpressionKind::Bool(lhs_val), ExpressionKind::Bool(rhs_val)) => match op {
                Operator::And => ExpressionKind::Bool(*lhs_val && *rhs_val),
                Operator::Or => ExpressionKind::Bool(*lhs_val || *rhs_val),
                Operator::Equality => ExpressionKind::Bool(lhs_val == rhs_val),
                Operator::NotEqual => ExpressionKind::Bool(lhs_val != rhs_val),

                _ => invalid_operator_for_compile_time_type(op, string_table, &self.location)?,
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
                    _ => invalid_operator_for_compile_time_type(op, string_table, &self.location)?,
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
            _ => self.diagnostic_type.to_owned(),
        };

        // Preserve value-shape metadata for string results so folded concatenations do not
        // accidentally promote template/path-shaped values into plain string operands.
        let result_value_shape = match &kind {
            ExpressionKind::StringSlice(_) => {
                if self.value_shape == ExpressionValueShape::TemplateString
                    || rhs.value_shape == ExpressionValueShape::TemplateString
                {
                    ExpressionValueShape::TemplateString
                } else if self.value_shape == ExpressionValueShape::PlainStringSlice
                    && rhs.value_shape == ExpressionValueShape::PlainStringSlice
                {
                    ExpressionValueShape::PlainStringSlice
                } else {
                    ExpressionValueShape::Ordinary
                }
            }
            _ => ExpressionValueShape::Ordinary,
        };

        let mut result_expression = Expression::new(
            kind,
            self.location.to_owned(),
            type_id_hint_for_diagnostic_type(&result_type),
            result_type,
            value_mode,
        )
        .with_regular_division_provenance(contains_regular_division);
        result_expression.value_shape = result_value_shape;

        Ok(Some(result_expression))
    }
}

#[cfg(test)]
#[path = "tests/constant_folding_tests.rs"]
mod tests;
