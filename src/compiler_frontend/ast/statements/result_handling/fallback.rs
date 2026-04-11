//! Result-handler fallback parsing helpers.
//!
//! WHAT: parses fallback-value lists for `!` result handling and named-handler suffixes.
//! WHY: fallback arity/type policy is shared by call and expression paths, so it must stay in one
//! place to avoid grammar drift.

use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_multiple_expressions;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_rule_error, return_type_error};

use super::propagation::is_result_propagation_boundary;

pub(crate) fn parse_result_fallback_values(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    success_result_types: &[DataType],
    fallback_label: &str,
    compilation_stage: &str,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    let fallback_context = context.new_child_expression(success_result_types.to_owned());
    let fallback_values = create_multiple_expressions(
        token_stream,
        &fallback_context,
        fallback_label,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_type_error!(
            format!(
                "{} provide more entries than the success return arity (expected {}).",
                fallback_label,
                success_result_types.len()
            ),
            token_stream.current_location(),
            {
                CompilationStage => compilation_stage,
                PrimarySuggestion => "Provide exactly one fallback value per success return slot",
            }
        );
    }

    Ok(fallback_values)
}

pub(crate) fn result_success_types(result_type: &DataType) -> Vec<DataType> {
    let Some(inner_type) = result_type.result_ok_type() else {
        return vec![];
    };

    match inner_type {
        DataType::Returns(values) => values.clone(),
        DataType::None => vec![],
        other => vec![other.clone()],
    }
}

pub(super) fn parse_named_handler_fallback(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    success_result_types: &[DataType],
    string_table: &mut StringTable,
    handler_name: &str,
    compilation_stage: &str,
    bare_handler_suggestion: &str,
) -> Result<Option<Vec<Expression>>, CompilerError> {
    if token_stream.current_token_kind() == &TokenKind::Colon {
        return Ok(None);
    }

    if is_result_propagation_boundary(token_stream.current_token_kind()) {
        return_rule_error!(
            format!(
                "Bare '{}!' is invalid for result handling. Add ': ... ;' for a scoped handler.",
                handler_name
            ),
            token_stream.current_location(),
            {
                CompilationStage => compilation_stage,
                PrimarySuggestion => bare_handler_suggestion,
            }
        );
    }

    if success_result_types.is_empty() {
        return_rule_error!(
            "This result has no success return values, so handler fallback values are not allowed here",
            token_stream.current_location(),
            {
                CompilationStage => compilation_stage,
                PrimarySuggestion => "Use 'err!:' without fallback values for error-only results",
            }
        );
    }

    Ok(Some(parse_result_fallback_values(
        token_stream,
        context,
        success_result_types,
        "Handler fallback values",
        compilation_stage,
        string_table,
    )?))
}
