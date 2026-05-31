//! Expression evaluation and AST-side constant folding implementation.
//!
//! WHAT: resolves parsed infix expression fragments into typed AST expressions.
//! WHY: AST is the stage that owns operator typing, constant folding, and the decision about
//!      whether an expression can stay compile-time or must survive as runtime RPN.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, TypeMismatchContext};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::instrumentation::{FrontendCounter, increment_frontend_counter};
use crate::compiler_frontend::optimizers::constant_folding::{
    ConstantFoldResult, constant_fold, fold_compile_time_expression,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{eval_log, return_compiler_error};

use super::ordering;
use super::result_type::resolve_expression_result_type;
use super::typing_error::ExpressionTypingError;

/// Resolve a parsed expression fragment into a fully typed AST `Expression`.
///
/// WHAT: applies shunting-yard ordering, operator type resolution, optional constant folding,
///       and final type validation against the caller's expectation.
/// WHY: this is the single entry point where AST decides whether an expression collapses to a
///      compile-time value or must be preserved as runtime RPN for HIR lowering.
pub fn evaluate_expression(
    context: &ScopeContext,
    nodes: Vec<AstNode>,
    type_interner: &mut AstTypeInterner<'_>,
    expected_type: &mut ExpectedType,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, ExpressionTypingError> {
    let (output_queue, location) = ordering::order_expression_nodes(nodes)?;

    // Fast path: a single R-value needs no operator resolution or RPN assembly.
    if output_queue.len() == 1 {
        let only_expression = fold_compile_time_expression(
            &output_queue[0].get_expr_with_type_environment(type_interner.environment())?,
            string_table,
            context.kind.is_constant_context(),
        )?;

        validate_expression_result_type(
            expected_type,
            only_expression.type_id,
            &output_queue[0].location,
            type_interner.environment_mut_for_derived_types(),
        )?;

        // References tighten the expected type so later fragments in the same statement
        // can resolve against the inferred target type.
        if let ExpressionKind::Reference(..) = only_expression.kind {
            *expected_type = ExpectedType::Known(only_expression.type_id);
        } else if matches!(expected_type, ExpectedType::Infer) {
            *expected_type = ExpectedType::Known(only_expression.type_id);
        }

        return Ok(only_expression);
    }

    // General path: resolve operator types across the full RPN shape, then attempt folding.
    let resolved_type = resolve_expression_result_type(
        &output_queue,
        &location,
        string_table,
        type_interner.environment(),
    )?;

    validate_expression_result_type(
        expected_type,
        resolved_type.type_id,
        &location,
        type_interner.environment_mut_for_derived_types(),
    )?;

    if matches!(expected_type, ExpectedType::Infer) {
        *expected_type = ExpectedType::Known(resolved_type.type_id);
    }

    // Runtime RPN needs an owned value mode for the final expression node.
    let value_mode = value_mode.as_owned();
    eval_log!("Attempting to Fold: ", Pretty output_queue);
    increment_frontend_counter(FrontendCounter::ConstantFoldAttemptCount);

    match constant_fold(&output_queue, string_table)? {
        ConstantFoldResult::Unchanged => {
            increment_ast_counter(AstCounter::RuntimeRpnUnchangedFolds);
            eval_log!("Fold unchanged — reusing owned RPN");

            Ok(runtime_expression_from_nodes(
                output_queue,
                resolved_type.diagnostic_type,
                resolved_type.type_id,
                context,
                value_mode,
            )?)
        }

        ConstantFoldResult::Folded(stack) => {
            increment_frontend_counter(FrontendCounter::ConstantFoldSuccessCount);
            eval_log!("Stack after folding: ", Pretty stack);

            // Fully folded to a single compile-time value.
            if stack.len() == 1 {
                return Ok(stack[0].get_expr_with_type_environment(type_interner.environment())?);
            }

            // Folding consumed every node but produced no result (e.g. empty input).
            if stack.is_empty() {
                return Err(CompilerDiagnostic::invalid_expression(location).into());
            }

            // Partial fold: assemble the reduced stack into runtime RPN.
            Ok(runtime_expression_from_nodes(
                stack,
                resolved_type.diagnostic_type,
                resolved_type.type_id,
                context,
                value_mode,
            )?)
        }
    }
}

/// Assemble a runtime RPN `Expression` from an ordered node stack.
///
/// WHAT: computes the span source location from the first and last nodes, then wraps the stack
///       in an `Expression::runtime_with_type_id`.
/// WHY: every runtime expression needs a single bounding `SourceLocation` for diagnostics.
fn runtime_expression_from_nodes(
    nodes: Vec<AstNode>,
    diagnostic_type: DataType,
    type_id: TypeId,
    context: &ScopeContext,
    value_mode: ValueMode,
) -> Result<Expression, CompilerError> {
    let Some(first_node) = nodes.first() else {
        return_compiler_error!("Runtime expression assembly received an empty node stack.");
    };
    let Some(last_node) = nodes.last() else {
        return_compiler_error!("Runtime expression assembly lost its trailing node.");
    };

    let first_node_start = first_node.location.start_pos;
    let last_node_end = last_node.location.end_pos;

    Ok(Expression::runtime_with_type_id(
        nodes,
        diagnostic_type,
        type_id,
        SourceLocation::new(context.scope.to_owned(), first_node_start, last_node_end),
        value_mode,
    ))
}

/// Validate that an expression's resolved semantic type is compatible with the
/// contextual expectation.
///
/// WHAT: compares canonical `TypeId`s through the `TypeEnvironment`.
/// WHY: semantic type decisions must use `TypeId` equality, not parse-level
///      `DataType` shape matching.
fn validate_expression_result_type(
    expected_type: &mut ExpectedType,
    actual_type_id: TypeId,
    location: &SourceLocation,
    type_environment: &mut TypeEnvironment,
) -> Result<(), ExpressionTypingError> {
    let Some(expected_type_id) = expected_type.known_type_id() else {
        return Ok(());
    };

    if is_declaration_compatible(expected_type_id, actual_type_id, type_environment) {
        return Ok(());
    }

    Err(CompilerDiagnostic::type_mismatch(
        expected_type_id,
        actual_type_id,
        TypeMismatchContext::General,
        location.clone(),
    )
    .into())
}
