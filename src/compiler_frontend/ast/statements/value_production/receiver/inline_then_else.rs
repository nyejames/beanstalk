//! Shared inline `then` / `else` parsing for value-producing receivers.
//!
//! WHAT: consumes the shared `then ... else ...` shape that appears in both
//! inline Bool value-if and inline single-predicate value-match.
//! WHY: these two syntactic forms previously duplicated structural validation
//! (newline rejection, `else then` rejection, same-line checks, and coercion).

use super::result_type::{infer_inline_result_type, receiver_type_mismatch_context};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression_until, create_expression_with_trailing_newline_policy,
};
use crate::compiler_frontend::ast::expressions::parse_expression_input::{
    ExpressionParseInput, ExpressionParseResources,
};
use crate::compiler_frontend::ast::statements::value_production::parse_values::{
    ProducedValuesParseInput, parse_produced_values_typed,
};
use crate::compiler_frontend::ast::statements::value_production::types::{
    ActiveValueProductionTarget, ValueReceiverKind,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidControlFlowStatementReason,
};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::type_coercion::contextual::coerce_expression_to_explicit_type_boundary;
use crate::compiler_frontend::type_coercion::parse_context::{
    CastTargetContext, ExpectedType, cast_target_context_for_type_id,
};
use crate::compiler_frontend::value_mode::ValueMode;

/// Input for the shared inline then/else parser.
pub(super) struct InlineThenElseInput<'a, 'b> {
    pub(super) token_stream: &'a mut FileTokens,
    pub(super) then_context: &'a ScopeContext,
    pub(super) else_context: &'a ScopeContext,
    pub(super) type_interner: &'a mut AstTypeInterner<'b>,
    pub(super) expected_result_type_ids: &'a [TypeId],
    pub(super) receiver_kind: ValueReceiverKind,
    pub(super) string_table: &'a mut StringTable,
}

/// Output of the shared inline then/else parser.
pub(super) struct InlineThenElseOutput {
    pub(super) then_values: Vec<Expression>,
    pub(super) else_values: Vec<Expression>,
    pub(super) result_type_id: TypeId,
    pub(super) result_type_ids: Vec<TypeId>,
}

/// Returns `true` when both source locations are on the same logical line.
///
/// WHAT: used to enforce that inline value-if/match arms stay on one line.
pub(in crate::compiler_frontend::ast::statements::value_production) fn same_logical_line(
    left: &SourceLocation,
    right: &SourceLocation,
) -> bool {
    left.start_pos.line_number == right.start_pos.line_number
}

/// File-local boxed diagnostic result alias.
///
/// WHAT: the shared inline then/else parser family returns
/// `Result<T, Box<CompilerDiagnostic>>` through this alias.
/// WHY: `CompilerDiagnostic` is large enough to trigger `clippy::result_large_err` when
/// stored directly in a `Result` variant. Boxing the error at this owner boundary keeps
/// the `Result` envelope small without changing `DiagnosticBag`, `CompilerMessages`, or
/// any shared error type. The direct callers `inline_if.rs` and `inline_match.rs` consume
/// the boxed result directly without unbox/rebox churn.
type InlineThenElseResult<T> = Result<T, Box<CompilerDiagnostic>>;

/// Parses the shared `then <branch> else <branch>` inline shape.
///
/// WHAT: assumes the current token is `then`. Consumes it, parses then and else
/// branches, validates same-line constraints, infers/coerces the result type.
/// WHY: consolidates duplicated logic from inline Bool value-if and inline
/// single-predicate value-match.
pub(super) fn parse_inline_then_else(
    input: InlineThenElseInput<'_, '_>,
) -> InlineThenElseResult<InlineThenElseOutput> {
    let InlineThenElseInput {
        token_stream,
        then_context,
        else_context,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
    } = input;

    let then_location = token_stream.current_location();
    token_stream.advance(); // consume `then`

    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(Box::new(
            CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                token_stream.current_location(),
            ),
        ));
    }

    if expected_result_type_ids.len() > 1 {
        // Multi-value inline form: reuse the shared produced-values parser so arity
        // and coercion are validated identically to block-form `then` statements.
        let target = ActiveValueProductionTarget {
            result_type_ids: expected_result_type_ids.to_vec(),
            receiver_kind,
            expected_arity: None,
        };

        let then_values = parse_produced_values_typed(ProducedValuesParseInput {
            token_stream,
            context: then_context,
            type_interner,
            target: &target,
            label: "then branch",
            string_table,
        })
        .map_err(|error| -> CompilerDiagnostic { error.into() })?;

        require_else_inline(token_stream, &then_location)?;
        token_stream.advance(); // consume `else`

        reject_else_then(token_stream)?;
        reject_newline_after_else(token_stream)?;

        let else_values = parse_produced_values_typed(ProducedValuesParseInput {
            token_stream,
            context: else_context,
            type_interner,
            target: &target,
            label: "else branch",
            string_table,
        })
        .map_err(|error| -> CompilerDiagnostic { error.into() })?;

        let result_type_id = type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(expected_result_type_ids.to_vec());

        return Ok(InlineThenElseOutput {
            then_values,
            else_values,
            result_type_id,
            result_type_ids: expected_result_type_ids.to_vec(),
        });
    }

    // Single-value inline form (preserves existing single-result behavior).
    let expected_type_id = expected_result_type_ids.first().copied();
    let mut then_expr_type = expected_type_id
        .map(ExpectedType::Known)
        .unwrap_or(ExpectedType::Infer);
    let mut then_cast_target_context =
        cast_target_context_for_inline_branch(expected_type_id, type_interner, string_table);

    let input = ExpressionParseInput::until(ExpressionParseResources {
        token_stream,
        scope_context: then_context,
        type_interner,
        expected_type: &mut then_expr_type,
        cast_target_context: &mut then_cast_target_context,
        value_mode: &ValueMode::ImmutableOwned,
        string_table,
    });
    let then_expr = create_expression_until(input, &[TokenKind::Else])
        .map_err(|error| -> CompilerDiagnostic { error.into() })?;

    require_else_inline(token_stream, &then_location)?;
    token_stream.advance(); // consume `else`

    reject_else_then(token_stream)?;
    reject_newline_after_else(token_stream)?;

    let mut else_expr_type = expected_type_id
        .map(ExpectedType::Known)
        .unwrap_or(ExpectedType::Infer);
    let mut else_cast_target_context =
        cast_target_context_for_inline_branch(expected_type_id, type_interner, string_table);
    let input = ExpressionParseInput::ordinary(
        ExpressionParseResources {
            token_stream,
            scope_context: else_context,
            type_interner,
            expected_type: &mut else_expr_type,
            cast_target_context: &mut else_cast_target_context,
            value_mode: &ValueMode::ImmutableOwned,
            string_table,
        },
        false,
    );
    let else_expr = create_expression_with_trailing_newline_policy(input)
        .map_err(|error| -> CompilerDiagnostic { error.into() })?;

    if !same_logical_line(&then_location, &else_expr.location) {
        return Err(Box::new(
            CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                else_expr.location.clone(),
            ),
        ));
    }

    let result_type_id = if let Some(expected_type_id) = expected_type_id {
        expected_type_id
    } else {
        infer_inline_result_type(
            then_expr.type_id,
            else_expr.type_id,
            None,
            type_interner,
            &then_expr.location,
            receiver_kind,
        )?
    };

    let mismatch_context = receiver_type_mismatch_context(receiver_kind);
    let then_expr = coerce_expression_to_explicit_type_boundary(
        then_expr,
        result_type_id,
        type_interner.environment(),
        then_context,
        mismatch_context,
    )?;
    let else_expr = coerce_expression_to_explicit_type_boundary(
        else_expr,
        result_type_id,
        type_interner.environment(),
        else_context,
        mismatch_context,
    )?;

    Ok(InlineThenElseOutput {
        then_values: vec![then_expr],
        else_values: vec![else_expr],
        result_type_id,
        result_type_ids: if expected_result_type_ids.is_empty() {
            vec![result_type_id]
        } else {
            expected_result_type_ids.to_vec()
        },
    })
}

fn cast_target_context_for_inline_branch(
    expected_type_id: Option<TypeId>,
    type_interner: &AstTypeInterner<'_>,
    string_table: &StringTable,
) -> CastTargetContext {
    expected_type_id
        .map(|type_id| {
            cast_target_context_for_type_id(type_id, type_interner.environment(), string_table)
        })
        .unwrap_or(CastTargetContext::None)
}

/// Requires that the current token is `else` and that it is on the same logical line.
fn require_else_inline(
    token_stream: &FileTokens,
    then_location: &SourceLocation,
) -> InlineThenElseResult<()> {
    if token_stream.current_token_kind() != &TokenKind::Else {
        return Err(Box::new(
            CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::ValueIfMissingElse,
                token_stream.current_location(),
            ),
        ));
    }

    if !same_logical_line(then_location, &token_stream.current_location()) {
        return Err(Box::new(
            CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                token_stream.current_location(),
            ),
        ));
    }

    Ok(())
}

/// Rejects `else then`, which is never valid in inline value-producing `if`.
fn reject_else_then(token_stream: &FileTokens) -> InlineThenElseResult<()> {
    if token_stream.current_token_kind() == &TokenKind::Then {
        return Err(Box::new(
            CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfElseThen,
                token_stream.current_location(),
            ),
        ));
    }

    Ok(())
}

/// Rejects a newline immediately after `else` in inline form.
fn reject_newline_after_else(token_stream: &FileTokens) -> InlineThenElseResult<()> {
    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(Box::new(
            CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                token_stream.current_location(),
            ),
        ));
    }

    Ok(())
}
