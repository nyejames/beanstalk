use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, Operator};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::place_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause,
};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

fn assignment_target_value_type(target: &AstNode) -> Result<DataType, CompilerError> {
    if let NodeKind::MethodCall {
        builtin: Some(BuiltinMethodKind::CollectionGet),
        result_types,
        ..
    } = &target.kind
        && let Some(result_type) = result_types.first()
        && let Some(ok_type) = result_type.result_ok_type()
    {
        return Ok(ok_type.to_owned());
    }

    Ok(target.get_expr()?.data_type)
}

fn assignment_target_name(target: &AstNode, string_table: &StringTable) -> String {
    assignment_target_path(target, string_table)
        .map(|path| format!("'{path}'"))
        .unwrap_or_else(|| String::from("<assignment target>"))
}

fn assignment_target_path(target: &AstNode, string_table: &StringTable) -> Option<String> {
    match &target.kind {
        NodeKind::Rvalue(expression) => match &expression.kind {
            crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Reference(
                path,
            ) => Some(
                path.name_str(string_table)
                    .map(str::to_owned)
                    .unwrap_or_else(|| path.to_string(string_table)),
            ),
            crate::compiler_frontend::ast::expressions::expression::ExpressionKind::Runtime(
                nodes,
            ) if nodes.len() == 1 => assignment_target_path(&nodes[0], string_table),
            _ => None,
        },
        NodeKind::FieldAccess { base, field, .. } => assignment_target_path(base, string_table)
            .map(|base_path| format!("{base_path}.{}", string_table.resolve(*field))),
        NodeKind::MethodCall {
            receiver,
            builtin,
            method,
            ..
        } => {
            let receiver_path = assignment_target_path(receiver, string_table)?;
            if matches!(builtin, Some(BuiltinMethodKind::CollectionGet)) {
                Some(format!("{receiver_path}.get(...)"))
            } else {
                Some(format!(
                    "{receiver_path}.{}(...)",
                    string_table.resolve(*method)
                ))
            }
        }
        _ => None,
    }
}

fn validate_assignment_value_type(
    expected_type: &DataType,
    actual_value: &Expression,
    target: &AstNode,
    assignment_operator: &str,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if is_declaration_compatible(expected_type, &actual_value.data_type) {
        return Ok(());
    }

    let target_name = assignment_target_name(target, string_table);
    let mismatch_clause =
        expected_found_clause(expected_type, &actual_value.data_type, string_table);
    return_type_error!(
        format!(
            "{} to {} has incorrect value type. {} {}",
            assignment_operator,
            target_name,
            mismatch_clause,
            offending_value_clause(actual_value, string_table)
        ),
        actual_value.location.clone(),
        {
            CompilationStage => "Expression Parsing",
            ExpectedType => expected_type.display_with_table(string_table),
            FoundType => actual_value.data_type.display_with_table(string_table),
            PrimarySuggestion => "Use a value whose type matches the assignment target, or cast explicitly before assignment",
        }
    )
}

fn build_mutation_from_target(
    token_stream: &mut FileTokens,
    variable_arg: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();
    let target_type = assignment_target_value_type(&target)?;
    let is_collection_get_index_write = matches!(
        &target.kind,
        NodeKind::MethodCall {
            builtin: Some(BuiltinMethodKind::CollectionGet),
            ..
        }
    );
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

    if is_collection_get_index_write && token_stream.current_token_kind() != &TokenKind::Assign {
        return_rule_error!(
            "Collection indexed writes only support '=' assignment.",
            location,
            {
                BorrowKind => "Mutable",
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use 'collection.get(index) = value' or call 'collection.set(index, value)'",
            }
        );
    }

    // Determine the assignment type and handle accordingly
    let value = match token_stream.current_token_kind() {
        TokenKind::Assign => {
            // Simple mutation: variable = new_value
            // Pass parse-time context only for Option(_) targets so that `none`
            // literals can resolve their inner type. Compound assignments below
            // use Inferred unconditionally because optional context does not
            // apply to arithmetic operators.
            token_stream.advance();

            let mut expr_type = parse_expectation_for_target_type(&target_type);
            let rhs = create_expression(
                token_stream,
                context,
                &mut expr_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;
            validate_assignment_value_type(
                &target_type,
                &rhs,
                &target,
                "Assignment",
                string_table,
            )?;
            coerce_expression_to_declared_type(rhs, &target_type)
        }

        TokenKind::AddAssign => {
            // Compound assignment: variable += value
            token_stream.advance();

            let mut expr_type = DataType::Inferred;
            let rhs = create_expression(
                token_stream,
                context,
                &mut expr_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;
            validate_assignment_value_type(
                &target_type,
                &rhs,
                &target,
                "Compound assignment '+='",
                string_table,
            )?;
            let add_value = coerce_expression_to_declared_type(rhs, &target_type);

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
                target_type.to_owned(),
                location.to_owned(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        TokenKind::SubtractAssign => {
            // Compound assignment: variable -= value
            token_stream.advance();

            let mut expr_type = DataType::Inferred;
            let rhs = create_expression(
                token_stream,
                context,
                &mut expr_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;
            validate_assignment_value_type(
                &target_type,
                &rhs,
                &target,
                "Compound assignment '-='",
                string_table,
            )?;
            let subtract_value = coerce_expression_to_declared_type(rhs, &target_type);

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
                target_type.to_owned(),
                location.to_owned(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        TokenKind::MultiplyAssign => {
            // Compound assignment: variable *= value
            token_stream.advance();

            let mut expr_type = DataType::Inferred;
            let rhs = create_expression(
                token_stream,
                context,
                &mut expr_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;
            validate_assignment_value_type(
                &target_type,
                &rhs,
                &target,
                "Compound assignment '*='",
                string_table,
            )?;
            let multiply_value = coerce_expression_to_declared_type(rhs, &target_type);

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
                target_type.to_owned(),
                location.clone(),
                variable_arg.value.ownership.to_owned(),
            )
        }

        TokenKind::DivideAssign => {
            // Compound assignment: variable /= value
            token_stream.advance();

            let mut expr_type = DataType::Inferred;
            let rhs = create_expression(
                token_stream,
                context,
                &mut expr_type,
                &variable_arg.value.ownership,
                false,
                string_table,
            )?;
            validate_assignment_value_type(
                &target_type,
                &rhs,
                &target,
                "Compound assignment '/='",
                string_table,
            )?;
            let divide_value = coerce_expression_to_declared_type(rhs, &target_type);

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
                target_type.to_owned(),
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

#[cfg(test)]
#[path = "tests/mutation_tests.rs"]
mod mutation_tests;
