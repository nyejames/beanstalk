//! Result-call suffix parsing helpers.
//!
//! WHAT: parses fallback and named-handler suffixes for calls to functions with error return
//! slots.
//! WHY: result handling has its own control-flow rules and statement-body parsing, which would
//! otherwise make the general function-call parser too large and too coupled to function bodies.

use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ResultCallHandling};
use crate::compiler_frontend::ast::expressions::parse_expression::create_multiple_expressions;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::traits::ContainsReferences;
use crate::{return_rule_error, return_syntax_error, return_type_error};

pub(crate) struct ResultHandledCall {
    pub(crate) name: InternedPath,
    pub(crate) args: Vec<Expression>,
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

pub(crate) fn parse_result_fallback_values(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    success_result_types: &[DataType],
    fallback_label: &str,
    string_table: &mut StringTable,
) -> Result<Vec<Expression>, CompilerError> {
    let fallback_context = context.new_child_expression(success_result_types.to_owned());
    let fallback_values =
        create_multiple_expressions(token_stream, &fallback_context, false, string_table)?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_type_error!(
            format!(
                "{} provide more entries than the success return arity (expected {}).",
                fallback_label,
                success_result_types.len()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Provide exactly one fallback value per success return slot",
            }
        );
    }

    Ok(fallback_values)
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
    let TokenKind::Symbol(handler_name) = token_stream.current_token_kind().to_owned() else {
        unreachable!("named handler parsing must start at the handler symbol");
    };

    if context.get_reference(&handler_name).is_some() {
        return_rule_error!(
            format!(
                "Named handler '{}' conflicts with an existing visible declaration.",
                string_table.resolve(handler_name)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Choose a unique handler variable name",
            }
        );
    }

    token_stream.advance();
    token_stream.advance();
    let handler_name_text = string_table.resolve(handler_name).to_owned();

    let handler_fallback = parse_named_handler_fallback(
        token_stream,
        context,
        &call.result_types,
        string_table,
        &handler_name_text,
    )?;

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_syntax_error!(
            "Expected ':' to start the named handler scope.",
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Use 'call(...) err!: ... ;' or 'call(...) err! fallback: ... ;'",
                SuggestedInsertion => ":",
            }
        );
    }

    token_stream.advance();
    let mut handler_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    let handler_error_id = handler_context.scope.append(handler_name);
    handler_context.add_var(Declaration {
        id: handler_error_id.to_owned(),
        value: Expression::no_value(
            token_stream.current_location(),
            error_return_type.to_owned(),
            Ownership::ImmutableOwned,
        ),
    });

    let mut local_handler_warnings: Vec<CompilerWarning> = Vec::new();
    let warnings = match warnings {
        Some(warnings) => warnings,
        None => &mut local_handler_warnings,
    };
    let handler_body = function_body_to_ast(token_stream, handler_context, warnings, string_table)?;

    // WHAT: rejects handler bodies that can simply fall through when the call expression must
    // still produce success values for the surrounding statement/expression.
    // WHY: without fallback values, a fallthrough path would leave the handled call with no value
    // continuation to merge back into.
    if handler_fallback.is_none()
        && value_required
        && !call.result_types.is_empty()
        && !scope_guarantees_exit(&handler_body)
    {
        return_rule_error!(
            "Named handler without fallback can fall through while success values are required.",
            call.call_location.clone(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Add fallback values before ':' or make the handler body terminate with return/return!",
            }
        );
    }

    Ok(call.into_ast_node(
        ResultCallHandling::Handler {
            error_name: handler_name,
            error_binding: handler_error_id,
            fallback: handler_fallback,
            body: handler_body,
        },
        token_stream.current_location(),
        &context.scope,
    ))
}

pub(crate) fn is_result_propagation_boundary(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::CloseParenthesis
            | TokenKind::Comma
            | TokenKind::Newline
            | TokenKind::End
            | TokenKind::Eof
            | TokenKind::Colon
            | TokenKind::TemplateClose
            | TokenKind::CloseCurly
            | TokenKind::Dot
    )
}

fn parse_named_handler_fallback(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    success_result_types: &[DataType],
    string_table: &mut StringTable,
    handler_name: &str,
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
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Use 'call(...)!' for propagation, 'call(...) ! fallback' for fallback values, or 'call(...) err!: ... ;' for a scoped handler",
            }
        );
    }

    if success_result_types.is_empty() {
        return_rule_error!(
            "This function has no success return values, so handler fallback values are not allowed here",
            token_stream.current_location(),
            {
                CompilationStage => "Function Call Parsing",
                PrimarySuggestion => "Use 'err!:' without fallback values for error-only functions",
            }
        );
    }

    Ok(Some(parse_result_fallback_values(
        token_stream,
        context,
        success_result_types,
        "Handler fallback values",
        string_table,
    )?))
}

fn statement_guarantees_exit(statement: &AstNode) -> bool {
    match &statement.kind {
        NodeKind::Return(_) | NodeKind::ReturnError(_) => true,
        NodeKind::If(_, then_body, Some(else_body)) => {
            scope_guarantees_exit(then_body) && scope_guarantees_exit(else_body)
        }
        NodeKind::Match(_, arms, Some(default_body)) => {
            arms.iter().all(|arm| scope_guarantees_exit(&arm.body))
                && scope_guarantees_exit(default_body)
        }
        _ => false,
    }
}

fn scope_guarantees_exit(body: &[AstNode]) -> bool {
    body.iter().any(statement_guarantees_exit)
}
