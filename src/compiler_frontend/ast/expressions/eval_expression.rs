//! Expression evaluation and AST-side constant folding.
//!
//! WHAT: resolves parsed infix expression fragments into typed AST expressions.
//! WHY: AST is the semantic boundary that owns operator typing, result handling checks, and the
//! final decision about whether an expression can collapse at compile time or must survive to HIR.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::compiler_errors::{CompilerError, SourceLocation};
use crate::compiler_frontend::optimizers::constant_folding::{
    constant_fold, fold_compile_time_expression,
};

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
#[cfg(test)]
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    NUMERIC_MIX_HINT, expected_found_clause,
};
use crate::return_type_error;
use crate::{eval_log, return_compiler_error, return_syntax_error};

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
    if nodes.is_empty() {
        return_compiler_error!("No nodes found in expression. This should never happen.");
    }

    let mut output_queue: Vec<AstNode> = Vec::new();
    let mut operators_stack: Vec<AstNode> = Vec::new();
    let location = extract_location(&nodes)?;

    // The parser already handled parentheses recursively, so this pass only needs to order the
    // flat infix fragment by precedence and associativity before typing/folding it.
    for node in nodes {
        eval_log!("Evaluating node in expression: ", Pretty node);
        match &node.kind {
            NodeKind::Rvalue(..) => {
                output_queue.push(node.to_owned());
            }

            NodeKind::FieldAccess { .. }
            | NodeKind::FunctionCall { .. }
            | NodeKind::ResultHandledFunctionCall { .. }
            | NodeKind::MethodCall { .. }
            | NodeKind::HostFunctionCall { .. } => {
                output_queue.push(node.to_owned());
            }

            NodeKind::Operator(..) => {
                let node_precedence = node.get_precedence();
                let left_associative = node.is_left_associative();

                pop_higher_precedence(
                    &mut operators_stack,
                    &mut output_queue,
                    node_precedence,
                    left_associative,
                );

                operators_stack.push(node);
            }

            _ => {
                return_compiler_error!(format!(
                    "Unsupported AST node found in expression: {:?}",
                    node.kind
                ))
            }
        }
    }

    while let Some(operator) = operators_stack.pop() {
        output_queue.push(operator);
    }

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
            "Type mismatch in expression. {}",
            expected_found_clause(expected_type, actual_type, string_table)
        ),
        location.clone(),
        {
            CompilationStage => "Expression Evaluation",
            PrimarySuggestion => "Ensure the expression produces the declared type, or add an explicit cast/handler first",
        }
    )
}

fn resolve_expression_result_type(
    output_queue: &[AstNode],
    expression_location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    // Mirror the final RPN evaluation shape with a type-only stack so operator diagnostics fire
    // before constant folding mutates any nodes.
    let mut stack: Vec<DataType> = Vec::with_capacity(output_queue.len());

    for node in output_queue {
        match &node.kind {
            NodeKind::Rvalue(expr) => stack.push(expr.data_type.to_owned()),
            NodeKind::FieldAccess { data_type, .. } => stack.push(data_type.to_owned()),
            NodeKind::FunctionCall { result_types, .. }
            | NodeKind::MethodCall { result_types, .. }
            | NodeKind::HostFunctionCall { result_types, .. }
            | NodeKind::ResultHandledFunctionCall { result_types, .. } => {
                stack.push(Expression::call_result_type(result_types.to_owned()));
            }
            NodeKind::Operator(op) => match op.required_values() {
                1 => {
                    let Some(operand) = stack.pop() else {
                        return_syntax_error!(
                            format!("Missing operand for unary operator '{}'.", op.to_str()),
                            node.location.clone(),
                            {
                                CompilationStage => "Expression Evaluation",
                            }
                        );
                    };
                    stack.push(resolve_unary_operator_type(
                        op,
                        &operand,
                        &node.location,
                        string_table,
                    )?);
                }
                2 => {
                    let Some(rhs) = stack.pop() else {
                        return_syntax_error!(
                            format!("Missing right-hand operand for operator '{}'.", op.to_str()),
                            node.location.clone(),
                            {
                                CompilationStage => "Expression Evaluation",
                            }
                        );
                    };
                    let Some(lhs) = stack.pop() else {
                        return_syntax_error!(
                            format!("Missing left-hand operand for operator '{}'.", op.to_str()),
                            node.location.clone(),
                            {
                                CompilationStage => "Expression Evaluation",
                            }
                        );
                    };
                    stack.push(resolve_binary_operator_type(
                        &lhs,
                        &rhs,
                        op,
                        &node.location,
                        string_table,
                    )?);
                }
                _ => {
                    return_compiler_error!(format!(
                        "Unsupported operator arity during expression typing: {:?}",
                        op
                    ));
                }
            },
            _ => {
                return_compiler_error!(format!(
                    "Unsupported AST node found in expression typing: {:?}",
                    node.kind
                ));
            }
        }
    }

    if stack.len() != 1 {
        return_syntax_error!(
            "Invalid expression shape after operator resolution.",
            expression_location.clone(),
            {
                CompilationStage => "Expression Evaluation",
                PrimarySuggestion => "Check the number of operands and operators in this expression",
            }
        );
    }

    Ok(stack
        .pop()
        .expect("validated expression typing stack should contain one result type"))
}

fn resolve_unary_operator_type(
    op: &crate::compiler_frontend::ast::expressions::expression::Operator,
    operand: &DataType,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    match op {
        crate::compiler_frontend::ast::expressions::expression::Operator::Not => {
            if operand == &DataType::Bool {
                Ok(DataType::Bool)
            } else {
                return_type_error!(
                    format!(
                        "Operator '{}' requires Bool, found '{}'.",
                        op.to_str(),
                        operand.display_with_table(string_table)
                    ),
                    location.clone(),
                    {
                        CompilationStage => "Expression Evaluation",
                        PrimarySuggestion => "Use 'not' only with Bool values",
                    }
                )
            }
        }
        other => Ok(match other {
            // Unary minus preserves the numeric payload type. The tokenizer/parser already own the
            // distinction between negative literals and a runtime unary subtraction operator.
            crate::compiler_frontend::ast::expressions::expression::Operator::Subtract => {
                operand.to_owned()
            }
            _ => operand.to_owned(),
        }),
    }
}

fn resolve_binary_operator_type(
    lhs: &DataType,
    rhs: &DataType,
    op: &crate::compiler_frontend::ast::expressions::expression::Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    if lhs.is_result() || rhs.is_result() {
        return_type_error!(
            format!(
                "Operator '{}' does not implicitly unwrap Result values (found '{}' and '{}').",
                op.to_str(),
                lhs.display_with_table(string_table),
                rhs.display_with_table(string_table)
            ),
            location.clone(),
            {
                CompilationStage => "Expression Evaluation",
                PrimarySuggestion => "Handle the Result with '!' syntax before using it in an ordinary expression",
            }
        );
    }

    use crate::compiler_frontend::ast::expressions::expression::Operator;

    if lhs == rhs {
        // Same-type operator handling stays explicit so broad "compatible" types cannot quietly
        // weaken arithmetic rules.
        return match (lhs, op) {
            (
                DataType::Int,
                Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::Modulus
                | Operator::Exponent
                | Operator::Root,
            ) => Ok(DataType::Int),
            (
                DataType::Float,
                Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::Modulus
                | Operator::Exponent
                | Operator::Root,
            ) => Ok(DataType::Float),
            (
                DataType::Decimal,
                Operator::Add
                | Operator::Subtract
                | Operator::Multiply
                | Operator::Divide
                | Operator::Modulus
                | Operator::Exponent
                | Operator::Root,
            ) => Ok(DataType::Decimal),
            (
                DataType::Int | DataType::Float | DataType::Decimal,
                Operator::Equality
                | Operator::NotEqual
                | Operator::GreaterThan
                | Operator::GreaterThanOrEqual
                | Operator::LessThan
                | Operator::LessThanOrEqual,
            ) => Ok(DataType::Bool),
            (
                DataType::Bool,
                Operator::And | Operator::Or | Operator::Equality | Operator::NotEqual,
            ) => Ok(DataType::Bool),
            (DataType::StringSlice, Operator::Add) => Ok(DataType::StringSlice),
            (DataType::StringSlice, Operator::Equality | Operator::NotEqual) => Ok(DataType::Bool),
            (DataType::Char, Operator::Equality | Operator::NotEqual) => Ok(DataType::Bool),
            (DataType::Int, Operator::Range) => Ok(DataType::Range),
            _ => invalid_operator_types(lhs, rhs, op, location, string_table),
        };
    }

    if matches!(
        (lhs, rhs),
        (DataType::Int, DataType::Float) | (DataType::Float, DataType::Int)
    ) {
        // Mixed numeric promotion is intentionally narrow: only Int/Float pairs mix implicitly,
        // and only for numeric arithmetic/comparisons.
        return match op {
            Operator::Add
            | Operator::Subtract
            | Operator::Multiply
            | Operator::Divide
            | Operator::Modulus
            | Operator::Exponent
            | Operator::Root => Ok(DataType::Float),
            Operator::Equality
            | Operator::NotEqual
            | Operator::GreaterThan
            | Operator::GreaterThanOrEqual
            | Operator::LessThan
            | Operator::LessThanOrEqual => Ok(DataType::Bool),
            _ => invalid_operator_types(lhs, rhs, op, location, string_table),
        };
    }

    invalid_operator_types(lhs, rhs, op, location, string_table)
}

fn invalid_operator_types(
    lhs: &DataType,
    rhs: &DataType,
    op: &crate::compiler_frontend::ast::expressions::expression::Operator,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<DataType, CompilerError> {
    // Keep the mixed-type diagnostic centralized so every invalid operator path points users at
    // the same strict-expression rule instead of scattering slightly different wording.
    return_type_error!(
        format!(
            "Operator '{}' cannot be applied to '{}' and '{}'. {}",
            op.to_str(),
            lhs.display_with_table(string_table),
            rhs.display_with_table(string_table),
            NUMERIC_MIX_HINT
        ),
        location.clone(),
        {
            CompilationStage => "Expression Evaluation",
            PrimarySuggestion => "Use matching operand types or add an explicit cast first",
        }
    )
}

fn pop_higher_precedence(
    operators_stack: &mut Vec<AstNode>,
    output_queue: &mut Vec<AstNode>,
    current_precedence: u32,
    left_associative: bool,
) {
    // Standard shunting-yard pop rule: earlier operators leave the stack when they bind at least
    // as tightly as the new operator, adjusted for right-associative cases like exponentiation.
    while let Some(top_op_node) = operators_stack.last() {
        let o2_precedence = top_op_node.get_precedence();

        let should_pop = if left_associative {
            o2_precedence >= current_precedence
        } else {
            o2_precedence > current_precedence
        };

        if should_pop {
            output_queue.push(
                operators_stack
                    .pop()
                    .expect("operator stack should contain the operator returned by last()"),
            );
        } else {
            break;
        }
    }
}

// Planned: allow mixed const/runtime concatenation by splitting foldable template segments
// from runtime segments instead of rejecting non-template values wholesale.
#[cfg(test)]
pub fn concat_template(
    simplified_expression: &mut Vec<AstNode>,
    ownership: Ownership,
) -> Result<Expression, CompilerError> {
    let mut template: Template = Template::create_default(vec![]);
    let _location = extract_location(simplified_expression)?;

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

fn extract_location(nodes: &[AstNode]) -> Result<SourceLocation, CompilerError> {
    if nodes.is_empty() {
        return_compiler_error!("No nodes found in expression. This should never happen.");
    }

    // Skip operator nodes and return the location of the first expression node
    for node in nodes {
        if !matches!(node.kind, NodeKind::Operator(_)) {
            return Ok(node.location.to_owned());
        }
    }

    // Fallback to first node if all nodes are operators (shouldn't happen)
    Ok(nodes[0].location.to_owned())
}

#[cfg(test)]
#[path = "tests/eval_expression_tests.rs"]
mod eval_expression_tests;
