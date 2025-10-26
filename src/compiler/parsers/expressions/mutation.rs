use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::ast_nodes::{Arg, AstNode, NodeKind};
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::tokens::{FileTokens, TokenKind};
use crate::{ast_log, return_rule_error, return_syntax_error};
use crate::compiler::parsers::ast::ScopeContext;

/// Handle mutation of existing mutable variables
/// Called when we encounter a variable reference followed by an assignment operator
pub fn handle_mutation(
    token_stream: &mut FileTokens,
    variable_arg: &Arg,
    context: &ScopeContext,
) -> Result<AstNode, CompileError> {
    let location = token_stream.current_location();

    // Check if the variable is mutable
    let ownership = &variable_arg.value.ownership;
    ast_log!(
        "Handling mutation for {:?}: '{}'",
        ownership,
        variable_arg.name
    );

    if !ownership.is_mutable() {
        return_rule_error!(
            location,
            "Cannot mutate immutable variable '{}'. Use '~' to declare a mutable variable",
            variable_arg.name
        );
    }

    // Determine the assignment type and handle accordingly
    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            // Simple mutation: variable = new_value
            token_stream.advance();

            let mut expected_type = variable_arg.value.data_type.clone();
            let new_value =
                create_expression(token_stream, context, &mut expected_type, ownership, false)?;

            Ok(AstNode {
                kind: NodeKind::Mutation(variable_arg.name.to_owned(), new_value, false),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            })
        }

        TokenKind::AddAssign => {
            // Compound assignment: variable += value
            token_stream.advance();

            let mut expected_type = variable_arg.value.data_type.clone();
            let add_value =
                create_expression(token_stream, context, &mut expected_type, ownership, false)?;

            // Create an addition expression in RPN order: variable, add_value, +
            let variable_ref = AstNode {
                kind: NodeKind::Expression(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let add_value_node = AstNode {
                kind: NodeKind::Expression(add_value),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let add_op = AstNode {
                kind: NodeKind::Operator(
                    crate::compiler::parsers::expressions::expression::Operator::Add,
                ),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };

            let addition_expr = Expression::runtime(
                vec![variable_ref, add_value_node, add_op],
                expected_type,
                location.to_owned(),
                variable_arg.value.ownership.to_owned(),
            );

            Ok(AstNode {
                kind: NodeKind::Mutation(variable_arg.name.to_owned(), addition_expr, false),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            })
        }

        TokenKind::SubtractAssign => {
            // Compound assignment: variable -= value
            token_stream.advance();

            let mut expected_type = variable_arg.value.data_type.clone();
            let subtract_value =
                create_expression(token_stream, context, &mut expected_type, ownership, false)?;

            // Create a subtraction expression in RPN order: variable, subtract_value, -
            let variable_ref = AstNode {
                kind: NodeKind::Expression(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let subtract_value_node = AstNode {
                kind: NodeKind::Expression(subtract_value),
                location: location.to_owned(),
                scope: context.scope_name.to_owned(),
            };
            let subtract_op = AstNode {
                kind: NodeKind::Operator(
                    crate::compiler::parsers::expressions::expression::Operator::Subtract,
                ),
                location: location.to_owned(),
                scope: context.scope_name.to_owned(),
            };

            let subtraction_expr = Expression::runtime(
                vec![variable_ref, subtract_value_node, subtract_op],
                expected_type,
                location.to_owned(),
                variable_arg.value.ownership.to_owned(),
            );

            Ok(AstNode {
                kind: NodeKind::Mutation(variable_arg.name.to_owned(), subtraction_expr, false),
                location: location.to_owned(),
                scope: context.scope_name.to_owned(),
            })
        }

        TokenKind::MultiplyAssign => {
            // Compound assignment: variable *= value
            token_stream.advance();

            let mut expected_type = variable_arg.value.data_type.clone();
            let multiply_value =
                create_expression(token_stream, context, &mut expected_type, ownership, false)?;

            // Create a multiplication expression in RPN order: variable, multiply_value, *
            let variable_ref = AstNode {
                kind: NodeKind::Expression(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let multiply_value_node = AstNode {
                kind: NodeKind::Expression(multiply_value),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let multiply_op = AstNode {
                kind: NodeKind::Operator(
                    crate::compiler::parsers::expressions::expression::Operator::Multiply,
                ),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };

            let multiplication_expr = Expression::runtime(
                vec![variable_ref, multiply_value_node, multiply_op],
                expected_type,
                location.clone(),
                variable_arg.value.ownership.to_owned(),
            );

            Ok(AstNode {
                kind: NodeKind::Mutation(variable_arg.name.to_owned(), multiplication_expr, false),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            })
        }

        TokenKind::DivideAssign => {
            // Compound assignment: variable /= value
            token_stream.advance();

            let mut expected_type = variable_arg.value.data_type.clone();
            let divide_value =
                create_expression(token_stream, context, &mut expected_type, ownership, false)?;

            // Create a division expression in RPN order: variable, divide_value, /
            let variable_ref = AstNode {
                kind: NodeKind::Expression(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let divide_value_node = AstNode {
                kind: NodeKind::Expression(divide_value),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };
            let divide_op = AstNode {
                kind: NodeKind::Operator(
                    crate::compiler::parsers::expressions::expression::Operator::Divide,
                ),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            };

            let division_expr = Expression::runtime(
                vec![variable_ref, divide_value_node, divide_op],
                expected_type,
                location.clone(),
                variable_arg.value.ownership.to_owned(),
            );

            Ok(AstNode {
                kind: NodeKind::Mutation(variable_arg.name.to_owned(), division_expr, false),
                location: location.clone(),
                scope: context.scope_name.to_owned(),
            })
        }

        _ => {
            return_syntax_error!(
                location.clone(),
                "Expected assignment operator after variable '{}', found '{:?}'",
                variable_arg.name,
                token_stream.current_token_kind()
            );
        }
    }
}
