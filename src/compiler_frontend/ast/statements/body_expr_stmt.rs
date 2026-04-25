//! Function-body expression statement filtering.
//!
//! WHAT: parses expression candidates in statement position and enforces the subset that can
//! stand alone as statements.
//! WHY: expression parsing is broader than statement grammar, so this module centralizes
//! statement-position filtering and targeted diagnostics.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_syntax_error;

fn is_expression_statement(expr: &Expression) -> bool {
    match &expr.kind {
        ExpressionKind::FunctionCall(..)
        | ExpressionKind::ResultHandledFunctionCall { .. }
        | ExpressionKind::HandledResult { .. }
        | ExpressionKind::HostFunctionCall(..) => true,
        ExpressionKind::Runtime(nodes) => nodes.iter().any(|node| {
            matches!(
                node.kind,
                NodeKind::MethodCall { .. }
                    | NodeKind::CollectionBuiltinCall { .. }
                    | NodeKind::FunctionCall { .. }
                    | NodeKind::HostFunctionCall { .. }
            )
        }),
        _ => false,
    }
}

pub(crate) fn parse_expression_statement_candidate(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut inferred = DataType::Inferred;
    let expr = create_expression(
        token_stream,
        context,
        &mut inferred,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    if !is_expression_statement(&expr) {
        return_syntax_error!(
            "Standalone expression is not a valid statement in this position.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use an assignment, call, control-flow statement, or declaration here",
            }
        );
    }

    Ok(expr)
}

pub(crate) fn parse_symbol_expression_statement_candidate(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    symbol_id: StringId,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut inferred = DataType::Inferred;
    let expr = create_expression(
        token_stream,
        context,
        &mut inferred,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    if !is_expression_statement(&expr) {
        return_syntax_error!(
            format!(
                "Unexpected token '{:?}' after variable reference '{}'. Expected an assignment or callable expression.",
                token_stream.current_token_kind(),
                string_table.resolve(symbol_id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use an assignment operator, a function call, or a receiver method call in statement position",
            }
        );
    }

    Ok(expr)
}
