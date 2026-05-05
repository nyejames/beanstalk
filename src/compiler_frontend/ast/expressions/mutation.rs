use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::eval_expression::evaluate_expression;
use crate::compiler_frontend::ast::expressions::expression::{Expression, Operator};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::field_access::parse_field_access;
use crate::compiler_frontend::ast::place_access::{ast_node_is_mutable_place, ast_node_is_place};
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause, regular_division_int_context_guidance,
    should_report_regular_division_int_context,
};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::{ast_log, return_rule_error, return_syntax_error, return_type_error};

fn assignment_target_value_type(target: &AstNode) -> Result<DataType, CompilerError> {
    if let NodeKind::CollectionBuiltinCall {
        op: CollectionBuiltinOp::Get,
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
            receiver, method, ..
        } => {
            let receiver_path = assignment_target_path(receiver, string_table)?;
            Some(format!(
                "{receiver_path}.{}(...)",
                string_table.resolve(*method)
            ))
        }
        NodeKind::CollectionBuiltinCall { receiver, op, .. } => {
            let receiver_path = assignment_target_path(receiver, string_table)?;
            let op_text = match op {
                CollectionBuiltinOp::Get => "get",
                CollectionBuiltinOp::Set => "set",
                CollectionBuiltinOp::Push => "push",
                CollectionBuiltinOp::Remove => "remove",
                CollectionBuiltinOp::Length => "length",
            };
            Some(format!("{receiver_path}.{op_text}(...)"))
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
    let suggestion = if should_report_regular_division_int_context(
        expected_type,
        &actual_value.data_type,
        actual_value,
    ) {
        regular_division_int_context_guidance()
    } else {
        "Use a value whose type matches the assignment target, or cast explicitly before assignment"
    };
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
            PrimarySuggestion => suggestion,
        }
    )
}

fn compound_assignment_operator(token_kind: &TokenKind) -> Option<(Operator, &'static str)> {
    match token_kind {
        TokenKind::AddAssign => Some((Operator::Add, "Compound assignment '+='")),
        TokenKind::SubtractAssign => Some((Operator::Subtract, "Compound assignment '-='")),
        TokenKind::MultiplyAssign => Some((Operator::Multiply, "Compound assignment '*='")),
        TokenKind::DivideAssign => Some((Operator::Divide, "Compound assignment '/='")),
        TokenKind::IntDivideAssign => Some((Operator::IntDivide, "Compound assignment '//='")),
        _ => None,
    }
}

fn evaluate_compound_assignment_value(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    variable_declaration: &Declaration,
    target: &AstNode,
    target_type: &DataType,
    compound_assignment: (Operator, &str),
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let (operator, assignment_label) = compound_assignment;
    let location = target.location.clone();
    let mut expr_type = DataType::Inferred;
    let rhs = create_expression(
        token_stream,
        context,
        &mut expr_type,
        &variable_declaration.value.value_mode,
        false,
        string_table,
    )?;

    let rhs_node = AstNode {
        kind: NodeKind::Rvalue(rhs),
        location: location.clone(),
        scope: context.scope.clone(),
    };
    let operator_node = AstNode {
        kind: NodeKind::Operator(operator),
        location,
        scope: context.scope.clone(),
    };
    let mut inferred = DataType::Inferred;
    let value = evaluate_expression(
        context,
        vec![target.clone(), rhs_node, operator_node],
        &mut inferred,
        &variable_declaration.value.value_mode,
        string_table,
    )?;

    validate_assignment_value_type(target_type, &value, target, assignment_label, string_table)?;
    Ok(value)
}

fn build_mutation_from_target(
    token_stream: &mut FileTokens,
    variable_declaration: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();
    let target_type = assignment_target_value_type(&target)?;
    let is_collection_get_index_write = matches!(
        &target.kind,
        NodeKind::CollectionBuiltinCall {
            op: CollectionBuiltinOp::Get,
            ..
        }
    );

    ast_log!(
        "Handling mutation for ",
        #variable_declaration.value.value_mode, " ",
        Blue variable_declaration.id.to_string(string_table)
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
            format!("Cannot mutate immutable variable '{}'. Use '~' to declare a mutable variable", variable_declaration.id.to_string(string_table)),
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

    let value = match token_stream.current_token_kind() {
        TokenKind::Assign => {
            // Simple mutation: variable = new_value. Parse-time context is
            // preserved only for context-sensitive literals. Compound
            // assignments below use Inferred because that context does not
            // apply to arithmetic operators.
            token_stream.advance();

            let mut expr_type = parse_expectation_for_target_type(&target_type);
            let rhs = create_expression(
                token_stream,
                context,
                &mut expr_type,
                &variable_declaration.value.value_mode,
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

        compound_token => {
            let Some((operator, label)) = compound_assignment_operator(compound_token) else {
                return_syntax_error!(
                    format!("Expected assignment operator after variable '{}', found '{:?}'", variable_declaration.id.to_string(string_table), token_stream.current_token_kind()),
                    location,
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Use '=', '+=', '-=', '*=', '/=', or '//=' for assignment",
                    }
                );
            };
            token_stream.advance();
            evaluate_compound_assignment_value(
                token_stream,
                context,
                variable_declaration,
                &target,
                &target_type,
                (operator, label),
                string_table,
            )?
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
    variable_declaration: &Declaration,
    target: AstNode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    build_mutation_from_target(
        token_stream,
        variable_declaration,
        target,
        context,
        string_table,
    )
}

/// Handle mutation of existing mutable variables.
/// Called when we encounter a variable reference followed by an assignment operator.
pub fn handle_mutation(
    token_stream: &mut FileTokens,
    variable_declaration: &Declaration,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let target = parse_field_access(token_stream, variable_declaration, context, string_table)?;
    build_mutation_from_target(
        token_stream,
        variable_declaration,
        target,
        context,
        string_table,
    )
}

#[cfg(test)]
#[path = "tests/mutation_tests.rs"]
mod mutation_tests;
