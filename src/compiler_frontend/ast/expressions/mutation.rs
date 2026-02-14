use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, Var};
use crate::compiler_frontend::ast::expressions::expression::{Expression, Operator};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::string_interning::StringTable;
use crate::{ast_log, return_rule_error, return_syntax_error};

/// Handle mutation of existing mutable variables
/// Called when we encounter a variable reference followed by an assignment operator
pub fn handle_mutation(
    token_stream: &mut FileTokens,
    variable_arg: &Var,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();

    // Check for field access on this arg,
    // Or just provide the reference AST node
    let node = parse_field_access(token_stream, variable_arg, &context, string_table)?;

    // Check if the variable is mutable
    let ownership = &variable_arg.value.ownership;
    ast_log!(
        "Handling mutation for {:?}: '{}'",
        ownership,
        string_table.resolve(variable_arg.id)
    );

    if !ownership.is_mutable() {
        let var_name_static: &'static str = Box::leak(
            string_table
                .resolve(variable_arg.id)
                .to_string()
                .into_boxed_str(),
        );
        return_rule_error!(
            format!("Cannot mutate immutable variable '{}'. Use '~' to declare a mutable variable", var_name_static),
            location.to_error_location(&string_table),
            {
                VariableName => var_name_static,
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

            let mut expected_type = variable_arg.value.data_type.clone();

            create_expression(
                token_stream,
                context,
                &mut expected_type,
                ownership,
                false,
                string_table,
            )?
        }

        TokenKind::AddAssign => {
            // Compound assignment: variable += value
            token_stream.advance();

            let mut expected_type = variable_arg.value.data_type.clone();
            let add_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                ownership,
                false,
                string_table,
            )?;

            // Create an addition expression in RPN order: variable, add_value, +
            let variable_ref = AstNode {
                kind: NodeKind::Rvalue(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let add_value_node = AstNode {
                kind: NodeKind::Rvalue(add_value),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let add_op = AstNode {
                kind: NodeKind::Operator(
                   Operator::Add,
                ),
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

            let mut expected_type = variable_arg.value.data_type.clone();
            let subtract_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                ownership,
                false,
                string_table,
            )?;

            // Create a subtraction expression in RPN order: variable, subtract_value, -
            let variable_ref = AstNode {
                kind: NodeKind::Rvalue(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let subtract_value_node = AstNode {
                kind: NodeKind::Rvalue(subtract_value),
                location: location.to_owned(),
                scope: context.scope.clone(),
            };
            let subtract_op = AstNode {
                kind: NodeKind::Operator(
                    Operator::Subtract,
                ),
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

            let mut expected_type = variable_arg.value.data_type.clone();
            let multiply_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                ownership,
                false,
                string_table,
            )?;

            // Create a multiplication expression in RPN order: variable, multiply_value, *
            let variable_ref = AstNode {
                kind: NodeKind::Rvalue(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let multiply_value_node = AstNode {
                kind: NodeKind::Rvalue(multiply_value),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let multiply_op = AstNode {
                kind: NodeKind::Operator(
                    Operator::Multiply,
                ),
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

            let mut expected_type = variable_arg.value.data_type.clone();
            let divide_value = create_expression(
                token_stream,
                context,
                &mut expected_type,
                ownership,
                false,
                string_table,
            )?;

            // Create a division expression in RPN order: variable, divide_value, /
            let variable_ref = AstNode {
                kind: NodeKind::Rvalue(variable_arg.value.clone()),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let divide_value_node = AstNode {
                kind: NodeKind::Rvalue(divide_value),
                location: location.clone(),
                scope: context.scope.clone(),
            };
            let divide_op = AstNode {
                kind: NodeKind::Operator(
                    Operator::Divide,
                ),
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
            let var_name_static: &'static str = Box::leak(
                string_table
                    .resolve(variable_arg.id)
                    .to_string()
                    .into_boxed_str(),
            );
            return_syntax_error!(
                format!("Expected assignment operator after variable '{}', found '{:?}'", var_name_static, token_stream.current_token_kind()),
                location.to_error_location(&string_table),
                {
                    VariableName => var_name_static,
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use '=', '+=', '-=', '*=', or '/=' for assignment",
                }
            );
        }
    };

    Ok(AstNode {
        kind: NodeKind::Assignment {
            target: Box::new(node),
            value,
        },
        location: location.clone(),
        scope: context.scope.clone(),
    })
}
