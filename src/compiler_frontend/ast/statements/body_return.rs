//! Function-body return statement parsing.
//!
//! WHAT: parses `return` and `return!` statements and emits validated AST return nodes.
//! WHY: return handling is signature-sensitive (arity, channels, coercion), so isolating this
//! logic keeps body dispatch simple and prevents return rules from leaking across modules.

use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_multiple_expressions,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_numeric_coercible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause,
};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_return_type;
use crate::{return_rule_error, return_type_error};

fn is_return_terminator(token: &TokenKind) -> bool {
    matches!(token, TokenKind::Newline | TokenKind::End | TokenKind::Eof)
}

fn normalize_return_expression_type(data_type: &DataType) -> DataType {
    // Runtime templates lower into string-producing functions.
    // Treat them as string returns during signature validation.
    match data_type {
        DataType::Template | DataType::TemplateWrapper => DataType::StringSlice,
        _ => data_type.to_owned(),
    }
}

pub(crate) fn parse_return_statement(
    token_stream: &mut FileTokens,
    ast: &mut Vec<AstNode>,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    if context.expected_result_types.is_empty() && !matches!(context.kind, ContextKind::Function) {
        return_rule_error!(
            "Return statements can only be used inside functions",
            token_stream.current_location()
        )
    }

    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::Bang {
        let Some(expected_error_type) = context.expected_error_type.as_ref() else {
            return_rule_error!(
                "return! can only be used inside functions that declare an error return slot",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use plain 'return' or add an error slot like 'Error!' to the function signature",
                }
            );
        };

        token_stream.advance();
        if is_return_terminator(token_stream.current_token_kind()) {
            return_type_error!(
                "return! requires an error value",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Provide one value that matches the function error return type",
                }
            );
        }

        let mut expected_error = expected_error_type.to_owned();
        let returned_error = create_expression(
            token_stream,
            context,
            &mut expected_error,
            &Ownership::ImmutableOwned,
            false,
            string_table,
        )?;

        let normalized_actual = normalize_return_expression_type(&returned_error.data_type);
        if &normalized_actual != expected_error_type {
            return_type_error!(
                format!(
                    "return! value has incorrect type. {} {}",
                    expected_found_clause(expected_error_type, &normalized_actual, string_table),
                    offending_value_clause(&returned_error, string_table)
                ),
                returned_error.location.clone(),
                {
                    CompilationStage => "AST Construction",
                    ExpectedType => expected_error_type.display_with_table(string_table),
                    FoundType => normalized_actual.display_with_table(string_table),
                    PrimarySuggestion => "Return an error value that exactly matches the function error slot type",
                }
            );
        }

        ast.push(AstNode {
            kind: NodeKind::ReturnError(returned_error),
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        });

        return Ok(());
    }

    let return_values = if context.expected_result_types.is_empty() {
        if is_return_terminator(token_stream.current_token_kind()) {
            Vec::new()
        } else {
            return_type_error!(
                "This function has no return signature, so 'return' must be bare (no return values).",
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use bare 'return' with no value in this function",
                    AlternativeSuggestion => "If you intended to return a value, add a return signature (for example '|args| -> String:')",
                }
            )
        }
    } else {
        if is_return_terminator(token_stream.current_token_kind()) {
            let expected_count = context.expected_result_types.len();
            return_type_error!(
                format!(
                    "This function must return {} value{}, but this return statement is bare.",
                    expected_count,
                    if expected_count == 1 { "" } else { "s" }
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Provide return values that match the function signature",
                }
            )
        }

        let parsed_returns = create_multiple_expressions(
            token_stream,
            context,
            "return values",
            false,
            string_table,
        )?;

        if token_stream.current_token_kind() == &TokenKind::Comma {
            let expected_count = context.expected_result_types.len();
            return_type_error!(
                format!(
                    "This function signature declares {} return value{}, but this return statement provides more.",
                    expected_count,
                    if expected_count == 1 { "" } else { "s" }
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Remove extra return values or update the function return signature",
                }
            );
        }

        let mut coerced_returns: Vec<Expression> = Vec::with_capacity(parsed_returns.len());

        for (index, (returned_value, expected_type)) in parsed_returns
            .into_iter()
            .zip(context.expected_result_types.iter())
            .enumerate()
        {
            let normalized_actual = normalize_return_expression_type(&returned_value.data_type);

            if &normalized_actual == expected_type {
                coerced_returns.push(returned_value);
                continue;
            }

            // Allow Int → Float at return sites and rewrite the expression.
            if is_numeric_coercible(&normalized_actual, expected_type) {
                coerced_returns.push(coerce_expression_to_return_type(
                    returned_value,
                    expected_type,
                ));
                continue;
            }

            return_type_error!(
                format!(
                    "Return value {} has incorrect type. {} {} Return values must match the function signature exactly.",
                    index + 1,
                    expected_found_clause(expected_type, &normalized_actual, string_table),
                    offending_value_clause(&returned_value, string_table),
                ),
                returned_value.location.clone(),
                {
                    CompilationStage => "AST Construction",
                    ExpectedType => expected_type.display_with_table(string_table),
                    FoundType => normalized_actual.display_with_table(string_table),
                    PrimarySuggestion => "Update the returned expression to match the declared return type",
                    AlternativeSuggestion => "If this value is intended, change the function return signature to the correct type",
                }
            );
        }

        coerced_returns
    };

    ast.push(AstNode {
        kind: NodeKind::Return(return_values),
        location: token_stream.current_location(),
        scope: context.scope.clone(),
    });

    Ok(())
}
