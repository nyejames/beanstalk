//! Expression evaluation and AST-side constant folding implementation.
//!
//! WHAT: resolves parsed infix expression fragments into typed AST expressions.
//! WHY: AST is the stage that owns operator typing, constant folding, and the decision about
//!      whether an expression can stay compile-time or must survive as runtime RPN.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::optimizers::constant_folding::{
    ConstantFoldResult, constant_fold, fold_compile_time_expression,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::expected_found_clause;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{eval_log, return_syntax_error, return_type_error};

use super::ordering;
use super::result_type::resolve_expression_result_type;

/// WHAT: turns one parsed expression fragment into a fully typed AST `Expression`.
/// WHY: AST is the stage that owns operator typing, constant folding, and the decision about
///      whether an expression can stay compile-time or must survive as runtime RPN.
pub fn evaluate_expression(
    context: &ScopeContext,
    nodes: Vec<AstNode>,
    current_type: &mut DataType,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let (output_queue, location) = ordering::order_expression_nodes(nodes)?;

    if output_queue.len() == 1 {
        let only_expression = fold_compile_time_expression(
            &output_queue[0].get_expr()?,
            string_table,
            context.kind.is_constant_context(),
        )?;
        validate_expression_result_type(
            current_type,
            &only_expression.data_type,
            &output_queue[0].location,
            string_table,
        )?;

        if let ExpressionKind::Reference(..) = only_expression.kind {
            *current_type = DataType::Reference(Box::new(only_expression.data_type.to_owned()));
        } else if matches!(current_type, DataType::Inferred) {
            *current_type = only_expression.data_type.to_owned();
        }

        return Ok(only_expression);
    }

    let resolved_type = resolve_expression_result_type(&output_queue, &location, string_table)?;
    validate_expression_result_type(current_type, &resolved_type, &location, string_table)?;

    if matches!(current_type, DataType::Inferred) {
        *current_type = resolved_type.to_owned();
    }

    let value_mode = value_mode.as_owned();
    eval_log!("Attempting to Fold: ", Pretty output_queue);

    match constant_fold(&output_queue, string_table)? {
        ConstantFoldResult::Unchanged => {
            increment_ast_counter(AstCounter::RuntimeRpnUnchangedFolds);
            eval_log!("Fold unchanged — reusing owned RPN");

            if output_queue.len() == 1 {
                return output_queue.into_iter().next().unwrap().get_expr();
            }

            let first_node_start = output_queue[0].location.start_pos;
            let last_node_end = output_queue[output_queue.len() - 1].location.end_pos;

            Ok(Expression::runtime(
                output_queue,
                resolved_type,
                SourceLocation::new(context.scope.to_owned(), first_node_start, last_node_end),
                value_mode,
            ))
        }

        ConstantFoldResult::Folded(stack) => {
            eval_log!("Stack after folding: ", Pretty stack);

            if stack.len() == 1 {
                return stack[0].get_expr();
            }

            if stack.is_empty() {
                let expected_type_str = current_type.display_with_table(string_table);
                return_syntax_error!(
                    "Invalid expression: no valid operands found during evaluation.",
                    SourceLocation::default(),
                    {
                        ExpectedType => expected_type_str,
                        CompilationStage => String::from("Expression Evaluation"),
                        PrimarySuggestion => String::from("Ensure the expression contains valid operands and operators"),
                    }
                );
            }

            let first_node_start = stack[0].location.start_pos;
            let last_node_end = stack[stack.len() - 1].location.end_pos;

            Ok(Expression::runtime(
                stack,
                resolved_type,
                SourceLocation::new(context.scope.to_owned(), first_node_start, last_node_end),
                value_mode,
            ))
        }
    }
}

fn validate_expression_result_type(
    expected_type: &mut DataType,
    actual_type: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if matches!(expected_type, DataType::Inferred) {
        return Ok(());
    }

    if is_type_compatible(expected_type, actual_type) {
        return Ok(());
    }

    return_type_error!(
        format!(
            "Expression result type mismatch. {}",
            expected_found_clause(expected_type, actual_type, string_table)
        ),
        location.clone(),
        {
            CompilationStage => "Expression Evaluation",
            ExpectedType => expected_type.display_with_table(string_table),
            FoundType => actual_type.display_with_table(string_table),
            PrimarySuggestion => "Ensure the expression produces the declared type, or add an explicit cast/handler first",
        }
    )
}
