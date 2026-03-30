use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, Operator};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::field_access::{
    ast_node_is_mutable_place, ast_node_is_place, parse_field_access,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{ast_log, return_rule_error, return_syntax_error};

fn build_mutation_from_target(
    token_stream: &mut FileTokens,
    variable_arg: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();
    let target_type = target.get_expr()?.data_type;
    ast_log!(
        "Handling mutation for ",
        #variable_arg.value.ownership, " ",
        Blue variable_arg.id.to_string(string_table)
    );

    if !ast_node_is_place(&target) {
        return_rule_error!(
            "Field assignment requires a mutable place receiver. Writing through temporaries or other rvalues is not allowed.",
            location,
            {
                BorrowKind => "Mutable",
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Assign through a mutable variable or field path instead of a temporary expression",
            }
        );
    }

    if !ast_node_is_mutable_place(&target) {
        return_rule_error!(
            format!("Cannot mutate immutable variable '{}'. Use '~' to declare a mutable variable", variable_arg.id.to_string(string_table)),
            location,
            {
                BorrowKind => "Mutable",
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Declare the variable with '~=' to make it mutable",
            }
        );
    }

    // Determine the assignment type and handle accordingly
    let value = match token_stream.current_token_kind() {
        TokenKind::Assign => {
            // Simple mutation: variable = new_value
            token_stream.advance();

            let mut expected_type = target_type.to_owned();

            create_expression(
                token_stream,
                context,
                &mut expected_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?
        }

        TokenKind::AddAssign => {
            // Compound assignment: variable += value
            token_stream.advance();

            let mut expected_type = target_type.to_owned();
            let add_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;

            // Create an addition expression in RPN order: variable, add_value, +
            let variable_ref = target.clone();
            let add_value_node = AstNode {
                kind: NodeKind::Rvalue(add_value),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let add_op = AstNode {
                kind: NodeKind::Operator(Operator::Add),
                location: location.clone(),
                scope: context.scope.clone(),
            };

            Expression::runtime(
                vec![variable_ref, add_value_node, add_op],
                expected_type,
                location.to_owned(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        TokenKind::SubtractAssign => {
            // Compound assignment: variable -= value
            token_stream.advance();

            let mut expected_type = target_type.to_owned();
            let subtract_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;

            // Create a subtraction expression in RPN order: variable, subtract_value, -
            let variable_ref = target.clone();
            let subtract_value_node = AstNode {
                kind: NodeKind::Rvalue(subtract_value),
                location: location.to_owned(),
                scope: context.scope.clone(),
            };
            let subtract_op = AstNode {
                kind: NodeKind::Operator(Operator::Subtract),
                location: location.to_owned(),
                scope: context.scope.clone(),
            };

            Expression::runtime(
                vec![variable_ref, subtract_value_node, subtract_op],
                expected_type,
                location.to_owned(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        TokenKind::MultiplyAssign => {
            // Compound assignment: variable *= value
            token_stream.advance();

            let mut expected_type = target_type.to_owned();
            let multiply_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;

            // Create a multiplication expression in RPN order: variable, multiply_value, *
            let variable_ref = target.clone();
            let multiply_value_node = AstNode {
                kind: NodeKind::Rvalue(multiply_value),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let multiply_op = AstNode {
                kind: NodeKind::Operator(Operator::Multiply),
                location: location.clone(),
                scope: context.scope.clone(),
            };

            Expression::runtime(
                vec![variable_ref, multiply_value_node, multiply_op],
                expected_type,
                location.clone(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        TokenKind::DivideAssign => {
            // Compound assignment: variable /= value
            token_stream.advance();

            let mut expected_type = target_type.to_owned();
            let divide_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;

            // Create a division expression in RPN order: variable, divide_value, /
            let variable_ref = target.clone();
            let divide_value_node = AstNode {
                kind: NodeKind::Rvalue(divide_value),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let divide_op = AstNode {
                kind: NodeKind::Operator(Operator::Divide),
                location: location.clone(),
                scope: context.scope.clone(),
            };

            Expression::runtime(
                vec![variable_ref, divide_value_node, divide_op],
                expected_type,
                location.clone(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        _ => {
            return_syntax_error!(
                format!("Expected assignment operator after variable '{}', found '{:?}'", variable_arg.id.to_string(string_table), token_stream.current_token_kind()),
                location,
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use '=', '+=', '-=', '*=', or '/=' for assignment",
                }
            );
        }
    };

    Ok(AstNode {
        kind: NodeKind::Assignment {
            target: Box::new(target),
            value,
        },
        location: location.clone(),
        scope: context.scope.clone(),
    })
}

pub(crate) fn handle_mutation_target(
    token_stream: &mut FileTokens,
    variable_arg: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    build_mutation_from_target(token_stream, variable_arg, target, context, string_table)
}

/// Handle mutation of existing mutable variables
/// Called when we encounter a variable reference followed by an assignment operator
pub fn handle_mutation(
    token_stream: &mut FileTokens,
    variable_arg: &Declaration,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let target = parse_field_access(token_stream, variable_arg, context, string_table)?;
    build_mutation_from_target(token_stream, variable_arg, target, context, string_table)
}
