//! Expression evaluation and AST-side constant folding.
//!
//! WHAT: resolves parsed infix expression fragments into typed AST expressions.
//! WHY: AST is the semantic boundary that owns operator typing, result handling checks, and the
//! final decision about whether an expression can collapse at compile time or must survive to HIR.

mod operator_policy;
mod ordering;
mod result_type;

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::optimizers::constant_folding::{
    constant_fold, fold_compile_time_expression,
};

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
#[cfg(test)]
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::expected_found_clause;
#[cfg(test)]
use crate::return_compiler_error;
use crate::return_type_error;
use crate::{eval_log, return_syntax_error};
use ordering::order_expression_nodes;
use result_type::resolve_expression_result_type;

/// WHAT: turns one parsed expression fragment into a fully typed AST `Expression`.
/// WHY: AST is the stage that owns operator typing, constant folding, and the decision about
///      whether an expression can stay compile-time or must survive as runtime RPN.
pub fn evaluate_expression(
    context: &ScopeContext,
    nodes: Vec<AstNode>,
    current_type: &mut DataType,
    ownership: &Ownership,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let (output_queue, location) = order_expression_nodes(nodes)?;

    if output_queue.len() == 1 {
        // Standalone expressions still need folding here so builtin casts and handled results can
        // collapse before we decide whether the surrounding declaration keeps a plain value or a
        // result type.
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

    // Resolve the result type from the RPN shape before folding. This keeps operator rules in AST
    // and avoids relying on later stages to rediscover mixed numeric promotion or result misuse.
    //
    // Logical operators are typed here in AST as part of normal expression validation. HIR later
    // lowers them with dedicated control-flow behavior, but the type-policy boundary stays here.
    let resolved_type = resolve_expression_result_type(&output_queue, &location, string_table)?;
    validate_expression_result_type(current_type, &resolved_type, &location, string_table)?;

    if matches!(current_type, DataType::Inferred) {
        *current_type = resolved_type.to_owned();
    }

    let ownership = ownership.get_owned();
    eval_log!("Attempting to Fold: ", Pretty output_queue);
    let stack = constant_fold(&output_queue, string_table)?;
    eval_log!("Stack after folding: ", Pretty stack);

    if stack.len() == 1 {
        // Fully folded expressions become the folded node itself, which lets callers keep compile-
        // time constants instead of wrapping them in an unnecessary runtime expression shell.
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
        ownership,
    ))
}

fn validate_expression_result_type(
    expected_type: &mut DataType,
    actual_type: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    // Declaration parsing can leave the surrounding type open until the expression proves what it
    // is. Once a concrete type exists, this helper enforces the boundary in one place.
    if matches!(expected_type, DataType::Inferred) {
        return Ok(());
    }

    // Strict boundary check: the expression must produce a type that is exactly
    // compatible with the expected type. Contextual numeric coercions (e.g.
    // Int → Float) are applied by callers before reaching here, so each site
    // that needs promotion — declarations, mutations, struct constructors,
    // collection literals — calls coerce_expression_to_declared_type itself
    // and passes DataType::Inferred here instead.
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

// Planned: allow mixed const/runtime concatenation by splitting foldable template segments
// from runtime segments instead of rejecting non-template values wholesale.
#[cfg(test)]
pub fn concat_template(
    simplified_expression: &mut Vec<AstNode>,
    ownership: Ownership,
) -> Result<Expression, CompilerError> {
    let mut template: Template = Template::create_default(vec![]);
    let _location = ordering::extract_expression_location(simplified_expression)?;

    for node in simplified_expression {
        match node.get_expr()?.kind {
            ExpressionKind::Template(template_to_concat) => {
                template
                    .content
                    .extend(template_to_concat.content.to_owned());

                // Preserve full style state deterministically by applying the most
                // recent template style, mirroring left-to-right concatenation order.
                template.style = template_to_concat.style.to_owned();
                template.explicit_style = template_to_concat.explicit_style.to_owned();
            }

            _ => {
                return_compiler_error!(
                    "Non-template value found in template expression (you can only concatenate templates with other templates)"
                )
            }
        }
    }

    Ok(Expression::template(template, ownership))
}

#[cfg(test)]
#[path = "../tests/eval_expression_tests.rs"]
mod eval_expression_tests;
