//! Result-call suffix parsing helpers.
//!
//! WHAT: parses fallback and named-handler suffixes for calls to functions with error return
//! slots.
//! WHY: result handling has its own control-flow rules and statement-body parsing, which would
//! otherwise make the general function-call parser too large and too coupled to function bodies.

mod fallback;
mod named_handler;
mod propagation;
mod termination;
mod validation;

use self::fallback::result_success_types;
use self::named_handler::{NamedResultHandler, NamedResultHandlerSite, parse_named_result_handler};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::CallArgument;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ResultCallHandling};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause,
};
use crate::{return_rule_error, return_type_error};

pub(crate) use self::fallback::parse_result_fallback_values;
pub(crate) use self::propagation::is_result_propagation_boundary;

const FUNCTION_CALL_STAGE: &str = "Function Call Parsing";
const EXPRESSION_STAGE: &str = "Expression Parsing";

pub(crate) struct ResultHandledCall {
    pub(crate) name: InternedPath,
    pub(crate) args: Vec<CallArgument>,
    pub(crate) result_types: Vec<DataType>,
    pub(crate) call_location: SourceLocation,
}

impl ResultHandledCall {
    pub(crate) fn into_ast_node(
        self,
        handling: ResultCallHandling,
        ast_location: SourceLocation,
        scope: &InternedPath,
    ) -> AstNode {
        AstNode {
            kind: NodeKind::ResultHandledFunctionCall {
                name: self.name,
                args: self.args,
                result_types: self.result_types,
                handling,
                location: self.call_location,
            },
            location: ast_location,
            scope: scope.clone(),
        }
    }
}

pub(crate) fn parse_result_handling_suffix_for_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    value: Expression,
    value_required: bool,
    warnings: Option<&mut Vec<CompilerWarning>>,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let Some(error_return_type) = value.data_type.result_error_type().cloned() else {
        return_rule_error!(
            "The '!' result-handling suffix is only valid for Result-valued expressions",
            token_stream.current_location(),
            {
                CompilationStage => EXPRESSION_STAGE,
                PrimarySuggestion => "Apply '!' only to a cast or other Result-valued expression",
            }
        );
    };

    let success_result_types = result_success_types(&value.data_type);

    if token_stream.current_token_kind() == &TokenKind::Bang {
        token_stream.advance();

        if is_result_propagation_boundary(token_stream.current_token_kind()) {
            let Some(expected_error_type) = context.expected_error_type.as_ref() else {
                return_rule_error!(
                    "This expression uses '!' propagation, but the surrounding function does not declare an error return slot",
                    token_stream.current_location(),
                    {
                        CompilationStage => EXPRESSION_STAGE,
                        PrimarySuggestion => "Declare a matching error slot in the surrounding function signature",
                    }
                );
            };

            if expected_error_type != &error_return_type {
                return_type_error!(
                    format!(
                        "Mismatched propagated error type. {} {}",
                        expected_found_clause(expected_error_type, &error_return_type, string_table),
                        offending_value_clause(&value, string_table),
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => EXPRESSION_STAGE,
                        ExpectedType => expected_error_type.display_with_table(string_table),
                        FoundType => error_return_type.display_with_table(string_table),
                        PrimarySuggestion => "Handle the result locally or change the surrounding function error slot type",
                    }
                );
            }

            return Ok(Expression::handled_result(
                value,
                ResultCallHandling::Propagate,
                token_stream.current_location(),
            ));
        }

        if success_result_types.is_empty() {
            return_rule_error!(
                "This Result has no success value, so fallback values cannot be provided here",
                token_stream.current_location(),
                {
                    CompilationStage => EXPRESSION_STAGE,
                    PrimarySuggestion => "Use plain propagation syntax 'expr!' for error-only Results",
                }
            );
        }

        let fallback_values = parse_result_fallback_values(
            token_stream,
            context,
            &success_result_types,
            "Fallback values",
            EXPRESSION_STAGE,
            string_table,
        )?;

        return Ok(Expression::handled_result(
            value,
            ResultCallHandling::Fallback(fallback_values),
            token_stream.current_location(),
        ));
    }

    if matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
        && token_stream.peek_next_token() == Some(&TokenKind::Bang)
    {
        let NamedResultHandler {
            error_name,
            error_binding,
            fallback,
            body,
        } = parse_named_result_handler(
            token_stream,
            context,
            NamedResultHandlerSite {
                success_result_types: &success_result_types,
                error_return_type: &error_return_type,
                value_required,
                compilation_stage: EXPRESSION_STAGE,
                scope_suggestion: "Use 'expr err!: ... ;' or 'expr err! fallback: ... ;'",
                bare_handler_suggestion: "Use 'expr!' for propagation, 'expr ! fallback' for fallback values, or 'expr err!: ... ;' for a scoped handler",
                value_required_location: value.location.clone(),
            },
            warnings,
            string_table,
        )?;

        return Ok(Expression::handled_result(
            value,
            ResultCallHandling::Handler {
                error_name,
                error_binding,
                fallback,
                body,
            },
            token_stream.current_location(),
        ));
    }

    Ok(value)
}

pub(crate) fn parse_named_result_handler_call(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    call: ResultHandledCall,
    error_return_type: &DataType,
    value_required: bool,
    warnings: Option<&mut Vec<CompilerWarning>>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let NamedResultHandler {
        error_name,
        error_binding,
        fallback,
        body,
    } = parse_named_result_handler(
        token_stream,
        context,
        NamedResultHandlerSite {
            success_result_types: &call.result_types,
            error_return_type,
            value_required,
            compilation_stage: FUNCTION_CALL_STAGE,
            scope_suggestion: "Use 'call(...) err!: ... ;' or 'call(...) err! fallback: ... ;'",
            bare_handler_suggestion: "Use 'call(...)!' for propagation, 'call(...) ! fallback' for fallback values, or 'call(...) err!: ... ;' for a scoped handler",
            value_required_location: call.call_location.clone(),
        },
        warnings,
        string_table,
    )?;

    Ok(call.into_ast_node(
        ResultCallHandling::Handler {
            error_name,
            error_binding,
            fallback,
            body,
        },
        token_stream.current_location(),
        &context.scope,
    ))
}

#[cfg(test)]
#[path = "../tests/result_handling_tests.rs"]
mod result_handling_tests;
