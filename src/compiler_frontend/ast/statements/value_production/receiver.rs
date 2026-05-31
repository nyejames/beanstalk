//! Central receiving-site helper for value-producing control-flow blocks.
//!
//! WHAT: detects `if` at expression positions that are closed receivers (declaration
//! initialisers and assignment RHS), parses inline or block form, validates arity and
//! completeness, and returns a `ValueBlock` expression.
//! WHY: general expression parsing must still reject bare `if` so that statement-level
//! `if` and value-producing `if` remain syntactically unambiguous.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, MatchExhaustiveness, NodeKind};
use crate::compiler_frontend::ast::statements::match_patterns::MatchArm;
use crate::compiler_frontend::ast::statements::value_production::types::ProducedValues;

use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_expression_until,
};
use crate::compiler_frontend::ast::statements::branching::parse_match_block;
use crate::compiler_frontend::ast::statements::condition_validation::ensure_if_statement_condition;
use crate::compiler_frontend::ast::statements::match_headers::parse_single_predicate_match_pattern;
use crate::compiler_frontend::ast::statements::value_production::completeness::analyze_branch_flow;
use crate::compiler_frontend::ast::statements::value_production::extract_single_produced_type;
use crate::compiler_frontend::ast::statements::value_production::parse_values::{
    ProducedValuesParseInput, parse_produced_values_typed,
};
use crate::compiler_frontend::ast::statements::value_production::types::{
    ActiveValueProductionTarget, BranchFlow, ValueBlock, ValueIfBlock, ValueMatchBlock,
    ValueReceiverKind,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, function_body_to_ast};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidControlFlowStatementReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::token_scan::find_expression_end_index;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::contextual::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;

/// Attempts to parse a value-producing block when the current token is `if` at a
/// closed receiving site.
///
/// WHAT: returns `None` if the current token is not `If`, otherwise parses the value
/// block and returns the resulting expression (or a diagnostic on failure).
/// WHY: this is the only place where `if` is permitted in expression position;
/// `create_expression` continues to reject it everywhere else.
#[allow(clippy::result_large_err)]
pub fn try_parse_value_block_at_receiver(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_result_type_ids: &[TypeId],
    receiver_kind: ValueReceiverKind,
    string_table: &mut StringTable,
) -> Option<Result<Expression, CompilerDiagnostic>> {
    if token_stream.current_token_kind() != &TokenKind::If {
        return None;
    }

    let location = token_stream.current_location();
    token_stream.advance();

    if let Some(reason) = unsupported_optional_single_predicate_reason(
        token_stream,
        context,
        type_interner.environment(),
    ) {
        return Some(Err(CompilerDiagnostic::invalid_control_flow_statement(
            reason,
            token_stream.current_location(),
        )));
    }

    if current_if_header_is_full_match(token_stream) {
        return Some(parse_value_match_at_receiver(ValueMatchParseInput {
            token_stream,
            context,
            type_interner,
            expected_result_type_ids,
            receiver_kind,
            string_table,
            location,
        }));
    }

    if current_if_header_is_inline_single_predicate(token_stream)
        && let Some(result) = try_parse_inline_single_predicate_value_match(
            token_stream,
            context,
            type_interner,
            expected_result_type_ids,
            receiver_kind,
            string_table,
            location.clone(),
        )
    {
        return Some(result);
    }

    let mut condition_type = ExpectedType::Infer;
    let condition = match create_expression_until(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition, string_table),
        type_interner,
        &mut condition_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Then, TokenKind::Colon],
        string_table,
    ) {
        Ok(expr) => expr,
        Err(err) => return Some(Err(err.into())),
    };
    if let Err(diagnostic) = ensure_if_statement_condition(&condition, type_interner.environment())
    {
        return Some(Err(diagnostic));
    }

    // Inline form: `if condition then expr else expr`
    if token_stream.current_token_kind() == &TokenKind::Then {
        if !same_logical_line(&location, &token_stream.current_location()) {
            return Some(Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                token_stream.current_location(),
            )));
        }

        return Some(parse_inline_value_if(ValueIfParseInput {
            token_stream,
            context,
            type_interner,
            expected_result_type_ids,
            receiver_kind,
            string_table,
            condition,
            location,
        }));
    }

    // Block form: `if condition: ... else ...`
    if token_stream.current_token_kind() == &TokenKind::Colon {
        return Some(parse_block_value_if(ValueIfParseInput {
            token_stream,
            context,
            type_interner,
            expected_result_type_ids,
            receiver_kind,
            string_table,
            condition,
            location,
        }));
    }

    Some(Err(CompilerDiagnostic::invalid_control_flow_statement(
        InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
        token_stream.current_location(),
    )))
}

pub(super) fn current_if_header_is_full_match(token_stream: &FileTokens) -> bool {
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Then, TokenKind::Colon],
    );
    if is_index >= token_stream.length {
        return false;
    }
    if token_stream.tokens[is_index].kind != TokenKind::Is {
        return false;
    }

    token_stream
        .tokens
        .iter()
        .skip(is_index + 1)
        .find(|token| token.kind != TokenKind::Newline)
        .is_some_and(|token| token.kind == TokenKind::Colon)
}

fn current_if_header_is_inline_single_predicate(token_stream: &FileTokens) -> bool {
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Then, TokenKind::Colon],
    );
    if is_index >= token_stream.length || token_stream.tokens[is_index].kind != TokenKind::Is {
        return false;
    }

    let Some(pattern_index) = next_non_newline_index(token_stream, is_index + 1) else {
        return false;
    };
    if !matches!(
        token_stream.tokens[pattern_index].kind,
        TokenKind::Symbol(_) | TokenKind::TypeParameterBracket
    ) {
        return false;
    }

    token_stream
        .tokens
        .iter()
        .skip(pattern_index + 1)
        .take_while(|token| {
            !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::End | TokenKind::Eof | TokenKind::Colon
            )
        })
        .any(|token| token.kind == TokenKind::Then)
}

fn unsupported_optional_single_predicate_reason(
    token_stream: &FileTokens,
    context: &ScopeContext,
    type_environment: &TypeEnvironment,
) -> Option<InvalidControlFlowStatementReason> {
    // Inline optional value recovery must use present capture. Full option
    // matches remain available for literal and `none` patterns.
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Then, TokenKind::Colon],
    );
    if is_index >= token_stream.length || token_stream.tokens[is_index].kind != TokenKind::Is {
        return None;
    }

    let TokenKind::Symbol(scrutinee_name) = token_stream.current_token_kind() else {
        return None;
    };
    if token_stream.index + 1 != is_index {
        return None;
    }

    let scrutinee_type_id = context.get_reference(scrutinee_name)?.value.type_id;
    type_environment.option_inner_type(scrutinee_type_id)?;

    let pattern_index = next_non_newline_index(token_stream, is_index + 1)?;
    let pattern_token = &token_stream.tokens[pattern_index].kind;

    if matches!(pattern_token, TokenKind::NoneLiteral) {
        return Some(InvalidControlFlowStatementReason::ValueIfOptionNonePredicate);
    }

    if token_is_literal_pattern(pattern_token)
        && header_has_inline_then_after(token_stream, pattern_index + 1)
    {
        return Some(InvalidControlFlowStatementReason::ValueIfOptionLiteralPredicate);
    }

    None
}

fn next_non_newline_index(token_stream: &FileTokens, start_index: usize) -> Option<usize> {
    token_stream
        .tokens
        .iter()
        .enumerate()
        .skip(start_index)
        .find(|(_, token)| token.kind != TokenKind::Newline)
        .map(|(index, _)| index)
}

fn token_is_literal_pattern(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::StringSliceLiteral(_)
            | TokenKind::RawStringLiteral(_)
            | TokenKind::IntLiteral(_)
            | TokenKind::FloatLiteral(_)
            | TokenKind::CharLiteral(_)
            | TokenKind::BoolLiteral(_)
    )
}

fn header_has_inline_then_after(token_stream: &FileTokens, start_index: usize) -> bool {
    token_stream
        .tokens
        .iter()
        .skip(start_index)
        .take_while(|token| {
            !matches!(
                token.kind,
                TokenKind::Newline | TokenKind::End | TokenKind::Eof
            )
        })
        .any(|token| token.kind == TokenKind::Then)
}

struct ValueMatchParseInput<'a, 'b> {
    token_stream: &'a mut FileTokens,
    context: &'a ScopeContext,
    type_interner: &'a mut AstTypeInterner<'b>,
    expected_result_type_ids: &'a [TypeId],
    receiver_kind: ValueReceiverKind,
    string_table: &'a mut StringTable,
    location: SourceLocation,
}

fn try_parse_inline_single_predicate_value_match(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expected_result_type_ids: &[TypeId],
    receiver_kind: ValueReceiverKind,
    string_table: &mut StringTable,
    location: SourceLocation,
) -> Option<Result<Expression, CompilerDiagnostic>> {
    let start_index = token_stream.index;
    let mut scrutinee_type = ExpectedType::Infer;
    let scrutinee = match create_expression_until(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition, string_table),
        type_interner,
        &mut scrutinee_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Is],
        string_table,
    ) {
        Ok(expression) => expression,
        Err(_) => {
            token_stream.index = start_index;
            return None;
        }
    };

    if token_stream.current_token_kind() != &TokenKind::Is {
        token_stream.index = start_index;
        return None;
    }

    let type_environment = type_interner.environment();
    let is_option_present_capture = type_environment
        .option_inner_type(scrutinee.type_id)
        .is_some()
        && next_non_newline_index(token_stream, token_stream.index + 1).is_some_and(|index| {
            token_stream.tokens[index].kind == TokenKind::TypeParameterBracket
        });
    let is_choice_predicate = type_environment.variants_for(scrutinee.type_id).is_some();

    if !is_option_present_capture && !is_choice_predicate {
        token_stream.index = start_index;
        return None;
    }

    token_stream.advance(); // consume `is`
    // Inline single-predicate arms still need an arm-local scope so captures such as
    // `name = if maybe is |name| then name else "guest"` do not reuse the receiving
    // declaration's path or leak into the else expression.
    let match_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    let parsed_pattern = match parse_single_predicate_match_pattern(
        &scrutinee,
        token_stream,
        &match_context,
        type_interner,
        string_table,
    ) {
        Ok(pattern) => pattern,
        Err(diagnostic) => return Some(Err(diagnostic)),
    };

    if token_stream.current_token_kind() != &TokenKind::Then {
        return Some(Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
            token_stream.current_location(),
        )));
    }

    if !same_logical_line(&location, &token_stream.current_location()) {
        return Some(Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        )));
    }

    Some(parse_inline_value_match(InlineValueMatchParseInput {
        token_stream,
        context,
        then_context: &parsed_pattern.arm_scope,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
        scrutinee,
        pattern: parsed_pattern.pattern,
        location,
    }))
}

struct InlineValueMatchParseInput<'a, 'b> {
    token_stream: &'a mut FileTokens,
    context: &'a ScopeContext,
    then_context: &'a ScopeContext,
    type_interner: &'a mut AstTypeInterner<'b>,
    expected_result_type_ids: &'a [TypeId],
    receiver_kind: ValueReceiverKind,
    string_table: &'a mut StringTable,
    scrutinee: Expression,
    pattern: crate::compiler_frontend::ast::statements::match_patterns::MatchPattern,
    location: SourceLocation,
}

fn parse_inline_value_match(
    input: InlineValueMatchParseInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let InlineValueMatchParseInput {
        token_stream,
        context,
        then_context,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
        scrutinee,
        pattern,
        location,
    } = input;

    let then_location = token_stream.current_location();
    token_stream.advance(); // consume `then`

    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    if expected_result_type_ids.len() > 1 {
        return parse_inline_multi_value_match(InlineValueMatchParseInput {
            token_stream,
            context,
            then_context,
            type_interner,
            expected_result_type_ids,
            receiver_kind,
            string_table,
            scrutinee,
            pattern,
            location,
        });
    }

    let expected_type_id = expected_result_type_ids.first().copied();
    let mut then_expr_type = expected_type_id
        .map(ExpectedType::Known)
        .unwrap_or(ExpectedType::Infer);
    let then_expr = create_expression_until(
        token_stream,
        then_context,
        type_interner,
        &mut then_expr_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Else],
        string_table,
    )
    .map_err(|e| -> CompilerDiagnostic { e.into() })?;

    if token_stream.current_token_kind() != &TokenKind::Else {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfMissingElse,
            token_stream.current_location(),
        ));
    }
    if !same_logical_line(&then_location, &token_stream.current_location()) {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    token_stream.advance(); // consume `else`

    if token_stream.current_token_kind() == &TokenKind::Then {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfElseThen,
            token_stream.current_location(),
        ));
    }
    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    let mut else_expr_type = expected_type_id
        .map(ExpectedType::Known)
        .unwrap_or(ExpectedType::Infer);
    let else_expr = create_expression(
        token_stream,
        context,
        type_interner,
        &mut else_expr_type,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )
    .map_err(|e| -> CompilerDiagnostic { e.into() })?;
    if !same_logical_line(&then_location, &else_expr.location) {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            else_expr.location.clone(),
        ));
    }

    let result_type_id = unify_branch_types(
        then_expr.type_id,
        else_expr.type_id,
        expected_type_id,
        type_interner,
        &then_expr.location,
        receiver_kind,
    )?;

    let then_expr = coerce_branch_expression(then_expr, result_type_id, type_interner);
    let else_expr = coerce_branch_expression(else_expr, result_type_id, type_interner);
    let then_body = vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: vec![then_expr],
            location: location.clone(),
        }),
        location: location.clone(),
        scope: then_context.scope.clone(),
    }];
    let else_body = vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: vec![else_expr],
            location: location.clone(),
        }),
        location: location.clone(),
        scope: context.scope.clone(),
    }];

    let result_type_ids = if expected_result_type_ids.is_empty() {
        vec![result_type_id]
    } else {
        expected_result_type_ids.to_vec()
    };

    build_inline_value_match_expression(InlineValueMatchBuildInput {
        scrutinee,
        pattern,
        then_body,
        else_body,
        location,
        result_type_id,
        result_type_ids,
        type_environment: type_interner.environment(),
    })
}

fn parse_inline_multi_value_match(
    input: InlineValueMatchParseInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let InlineValueMatchParseInput {
        token_stream,
        context,
        then_context,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
        scrutinee,
        pattern,
        location,
    } = input;

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
    .map_err(|e| -> CompilerDiagnostic { e.into() })?;

    if token_stream.current_token_kind() != &TokenKind::Else {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfMissingElse,
            token_stream.current_location(),
        ));
    }
    if !same_logical_line(&location, &token_stream.current_location()) {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    token_stream.advance(); // consume `else`

    if token_stream.current_token_kind() == &TokenKind::Then {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfElseThen,
            token_stream.current_location(),
        ));
    }
    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    let else_values = parse_produced_values_typed(ProducedValuesParseInput {
        token_stream,
        context,
        type_interner,
        target: &target,
        label: "else branch",
        string_table,
    })
    .map_err(|e| -> CompilerDiagnostic { e.into() })?;

    let result_type_id = type_interner
        .environment_mut_for_derived_types()
        .intern_tuple(expected_result_type_ids.to_vec());

    let then_body = vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: then_values,
            location: location.clone(),
        }),
        location: location.clone(),
        scope: then_context.scope.clone(),
    }];
    let else_body = vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: else_values,
            location: location.clone(),
        }),
        location: location.clone(),
        scope: context.scope.clone(),
    }];

    build_inline_value_match_expression(InlineValueMatchBuildInput {
        scrutinee,
        pattern,
        then_body,
        else_body,
        location,
        result_type_id,
        result_type_ids: expected_result_type_ids.to_vec(),
        type_environment: type_interner.environment(),
    })
}

struct InlineValueMatchBuildInput<'a> {
    scrutinee: Expression,
    pattern: crate::compiler_frontend::ast::statements::match_patterns::MatchPattern,
    then_body: Vec<AstNode>,
    else_body: Vec<AstNode>,
    location: SourceLocation,
    result_type_id: TypeId,
    result_type_ids: Vec<TypeId>,
    type_environment: &'a TypeEnvironment,
}

fn build_inline_value_match_expression(
    input: InlineValueMatchBuildInput<'_>,
) -> Result<Expression, CompilerDiagnostic> {
    let InlineValueMatchBuildInput {
        scrutinee,
        pattern,
        then_body,
        else_body,
        location,
        result_type_id,
        result_type_ids,
        type_environment,
    } = input;

    let value_match = ValueMatchBlock {
        scrutinee,
        arms: vec![MatchArm {
            pattern,
            guard: None,
            body: then_body,
        }],
        default: Some(else_body),
        exhaustiveness: MatchExhaustiveness::HasDefault,
        location: location.clone(),
        result_type_ids,
    };

    Ok(Expression::new(
        ExpressionKind::ValueBlock {
            block: Box::new(ValueBlock::Match(value_match)),
        },
        location,
        result_type_id,
        diagnostic_type_spelling(result_type_id, type_environment),
        ValueMode::ImmutableOwned,
    ))
}

fn parse_value_match_at_receiver(
    input: ValueMatchParseInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let ValueMatchParseInput {
        token_stream,
        context,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
        location,
    } = input;

    let mut scrutinee_type = ExpectedType::Infer;
    let scrutinee = create_expression_until(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition, string_table),
        type_interner,
        &mut scrutinee_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Is],
        string_table,
    )?;

    if token_stream.current_token_kind() != &TokenKind::Is {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
            token_stream.current_location(),
        ));
    }
    token_stream.advance();

    let active_target = ActiveValueProductionTarget {
        result_type_ids: expected_result_type_ids.to_vec(),
        receiver_kind,
        expected_arity: None,
    };
    let mut warnings = Vec::new();
    let parsed_match = parse_match_block(
        scrutinee,
        token_stream,
        context,
        type_interner,
        &mut warnings,
        Some(active_target),
        string_table,
    )?;
    emit_collected_warnings(context, warnings);

    validate_value_match_completeness(
        &parsed_match.arms,
        parsed_match.default.as_deref(),
        &location,
    )?;

    let result_type_id = infer_value_match_result_type(
        &parsed_match.arms,
        parsed_match.default.as_deref(),
        expected_result_type_ids,
        type_interner,
        &location,
        receiver_kind,
    )?;
    let result_type_ids = if expected_result_type_ids.is_empty() {
        vec![result_type_id]
    } else {
        expected_result_type_ids.to_vec()
    };

    let value_match = ValueMatchBlock {
        scrutinee: parsed_match.scrutinee,
        arms: parsed_match.arms,
        default: parsed_match.default,
        exhaustiveness: parsed_match.exhaustiveness,
        location: location.clone(),
        result_type_ids,
    };

    Ok(Expression::new(
        ExpressionKind::ValueBlock {
            block: Box::new(ValueBlock::Match(value_match)),
        },
        location,
        result_type_id,
        diagnostic_type_spelling(result_type_id, type_interner.environment()),
        ValueMode::ImmutableOwned,
    ))
}

struct ValueIfParseInput<'a, 'b> {
    token_stream: &'a mut FileTokens,
    context: &'a ScopeContext,
    type_interner: &'a mut AstTypeInterner<'b>,
    expected_result_type_ids: &'a [TypeId],
    receiver_kind: ValueReceiverKind,
    string_table: &'a mut StringTable,
    condition: Expression,
    location: SourceLocation,
}

fn parse_inline_value_if(
    input: ValueIfParseInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let ValueIfParseInput {
        token_stream,
        context,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
        condition,
        location,
    } = input;

    let then_location = token_stream.current_location();
    token_stream.advance(); // consume `then`

    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    // Multi-value inline form: reuse the shared produced-values parser so arity
    // and coercion are validated identically to block-form `then` statements.
    if expected_result_type_ids.len() > 1 {
        let target = ActiveValueProductionTarget {
            result_type_ids: expected_result_type_ids.to_vec(),
            receiver_kind,
            expected_arity: None,
        };

        let then_values = parse_produced_values_typed(ProducedValuesParseInput {
            token_stream,
            context,
            type_interner,
            target: &target,
            label: "then branch",
            string_table,
        })
        .map_err(|e| -> CompilerDiagnostic { e.into() })?;

        if token_stream.current_token_kind() != &TokenKind::Else {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::ValueIfMissingElse,
                token_stream.current_location(),
            ));
        }
        if !same_logical_line(&then_location, &token_stream.current_location()) {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                token_stream.current_location(),
            ));
        }

        token_stream.advance(); // consume `else`

        if token_stream.current_token_kind() == &TokenKind::Then {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfElseThen,
                token_stream.current_location(),
            ));
        }
        if token_stream.current_token_kind() == &TokenKind::Newline {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::InlineValueIfMultiline,
                token_stream.current_location(),
            ));
        }

        let else_values = parse_produced_values_typed(ProducedValuesParseInput {
            token_stream,
            context,
            type_interner,
            target: &target,
            label: "else branch",
            string_table,
        })
        .map_err(|e| -> CompilerDiagnostic { e.into() })?;

        let result_type_id = type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(expected_result_type_ids.to_vec());

        let then_body = vec![AstNode {
            kind: NodeKind::ThenValue(ProducedValues {
                expressions: then_values,
                location: location.clone(),
            }),
            location: location.clone(),
            scope: context.scope.clone(),
        }];

        let else_body = vec![AstNode {
            kind: NodeKind::ThenValue(ProducedValues {
                expressions: else_values,
                location: location.clone(),
            }),
            location: location.clone(),
            scope: context.scope.clone(),
        }];

        let value_if = ValueIfBlock {
            condition,
            then_body,
            else_body,
            location: location.clone(),
            result_type_ids: expected_result_type_ids.to_vec(),
        };

        return Ok(Expression::new(
            ExpressionKind::ValueBlock {
                block: Box::new(ValueBlock::If(value_if)),
            },
            location,
            result_type_id,
            diagnostic_type_spelling(result_type_id, type_interner.environment()),
            ValueMode::ImmutableOwned,
        ));
    }

    // Single-value inline form (preserves existing single-result behavior).
    let expected_type_id = expected_result_type_ids.first().copied();
    let mut then_expr_type = expected_type_id
        .map(ExpectedType::Known)
        .unwrap_or(ExpectedType::Infer);
    let then_expr = create_expression_until(
        token_stream,
        context,
        type_interner,
        &mut then_expr_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Else],
        string_table,
    )
    .map_err(|e| -> CompilerDiagnostic { e.into() })?;

    if token_stream.current_token_kind() != &TokenKind::Else {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfMissingElse,
            token_stream.current_location(),
        ));
    }
    if !same_logical_line(&then_location, &token_stream.current_location()) {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    token_stream.advance(); // consume `else`

    if token_stream.current_token_kind() == &TokenKind::Then {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfElseThen,
            token_stream.current_location(),
        ));
    }
    if token_stream.current_token_kind() == &TokenKind::Newline {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            token_stream.current_location(),
        ));
    }

    let mut else_expr_type = expected_type_id
        .map(ExpectedType::Known)
        .unwrap_or(ExpectedType::Infer);
    let else_expr = create_expression(
        token_stream,
        context,
        type_interner,
        &mut else_expr_type,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )
    .map_err(|e| -> CompilerDiagnostic { e.into() })?;
    if !same_logical_line(&then_location, &else_expr.location) {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::InlineValueIfMultiline,
            else_expr.location.clone(),
        ));
    }

    let result_type_id = unify_branch_types(
        then_expr.type_id,
        else_expr.type_id,
        expected_type_id,
        type_interner,
        &then_expr.location,
        receiver_kind,
    )?;

    // Coerce each branch expression to the unified result type so HIR sees
    // compatible types when assigning into the result local.
    let then_expr = coerce_branch_expression(then_expr, result_type_id, type_interner);
    let else_expr = coerce_branch_expression(else_expr, result_type_id, type_interner);

    let then_body = vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: vec![then_expr],
            location: location.clone(),
        }),
        location: location.clone(),
        scope: context.scope.clone(),
    }];

    let else_body = vec![AstNode {
        kind: NodeKind::ThenValue(ProducedValues {
            expressions: vec![else_expr],
            location: location.clone(),
        }),
        location: location.clone(),
        scope: context.scope.clone(),
    }];

    let value_if = ValueIfBlock {
        condition,
        then_body,
        else_body,
        location: location.clone(),
        result_type_ids: expected_result_type_ids.to_vec(),
    };

    Ok(Expression::new(
        ExpressionKind::ValueBlock {
            block: Box::new(ValueBlock::If(value_if)),
        },
        location,
        result_type_id,
        diagnostic_type_spelling(result_type_id, type_interner.environment()),
        ValueMode::ImmutableOwned,
    ))
}

fn parse_block_value_if(
    input: ValueIfParseInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let ValueIfParseInput {
        token_stream,
        context,
        type_interner,
        expected_result_type_ids,
        receiver_kind,
        string_table,
        condition,
        location,
    } = input;

    token_stream.advance(); // consume `:`

    let active_target = ActiveValueProductionTarget {
        result_type_ids: expected_result_type_ids.to_vec(),
        receiver_kind,
        expected_arity: None,
    };

    let mut then_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    then_context.active_value_target = Some(active_target.clone());
    let mut then_warnings = Vec::new();
    let then_body = function_body_to_ast(
        token_stream,
        then_context,
        type_interner,
        &mut then_warnings,
        string_table,
    )?;
    emit_collected_warnings(context, then_warnings);

    if token_stream.current_token_kind() != &TokenKind::Else {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfMissingElse,
            token_stream.current_location(),
        ));
    }
    token_stream.advance(); // consume `else`

    let mut else_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    else_context.active_value_target = Some(active_target);
    let mut else_warnings = Vec::new();
    let else_body = function_body_to_ast(
        token_stream,
        else_context,
        type_interner,
        &mut else_warnings,
        string_table,
    )?;
    emit_collected_warnings(context, else_warnings);

    let then_flow = analyze_branch_flow(&then_body);
    let else_flow = analyze_branch_flow(&else_body);

    let then_produces = matches!(then_flow, BranchFlow::ProducesValue);
    let then_terminates = matches!(then_flow, BranchFlow::Terminates);
    let else_produces = matches!(else_flow, BranchFlow::ProducesValue);
    let else_terminates = matches!(else_flow, BranchFlow::Terminates);

    if !then_produces && !then_terminates {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfBranchFallsThrough,
            location.clone(),
        ));
    }
    if !else_produces && !else_terminates {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfBranchFallsThrough,
            location.clone(),
        ));
    }
    if !then_produces && !else_produces {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        ));
    }

    let result_type_id = infer_block_result_type(
        &then_body,
        &else_body,
        expected_result_type_ids,
        type_interner,
        &location,
        receiver_kind,
    )?;

    let value_if = ValueIfBlock {
        condition,
        then_body,
        else_body,
        location: location.clone(),
        result_type_ids: expected_result_type_ids.to_vec(),
    };

    Ok(Expression::new(
        ExpressionKind::ValueBlock {
            block: Box::new(ValueBlock::If(value_if)),
        },
        location,
        result_type_id,
        diagnostic_type_spelling(result_type_id, type_interner.environment()),
        ValueMode::ImmutableOwned,
    ))
}

pub(super) fn same_logical_line(left: &SourceLocation, right: &SourceLocation) -> bool {
    left.start_pos.line_number == right.start_pos.line_number
}

/// Unifies the types of two branch expressions for an inline value-producing `if`.
///
/// WHAT: when the expected type is known, validates both branches are compatible
/// and returns it. When inferred, ensures both branches agree and returns the shared type.
fn type_mismatch_context_for_receiver(kind: ValueReceiverKind) -> TypeMismatchContext {
    match kind {
        ValueReceiverKind::Return => TypeMismatchContext::ReturnValue,
        ValueReceiverKind::Declaration => TypeMismatchContext::Declaration,
        _ => TypeMismatchContext::Assignment,
    }
}

fn coerce_branch_expression(
    expr: Expression,
    target_type_id: TypeId,
    type_interner: &mut AstTypeInterner<'_>,
) -> Expression {
    if expr.type_id == target_type_id {
        return expr;
    }
    coerce_expression_to_declared_type(expr, target_type_id, type_interner.environment())
}

fn unify_branch_types(
    then_type: TypeId,
    else_type: TypeId,
    expected_type_id: Option<TypeId>,
    type_interner: &mut AstTypeInterner<'_>,
    location: &SourceLocation,
    receiver_kind: ValueReceiverKind,
) -> Result<TypeId, CompilerDiagnostic> {
    let context = type_mismatch_context_for_receiver(receiver_kind);
    if let Some(expected) = expected_type_id {
        let env = type_interner.environment();
        if !is_declaration_compatible(expected, then_type, env) {
            return Err(CompilerDiagnostic::type_mismatch(
                expected,
                then_type,
                context,
                location.clone(),
            ));
        }
        if !is_declaration_compatible(expected, else_type, env) {
            return Err(CompilerDiagnostic::type_mismatch(
                expected,
                else_type,
                context,
                location.clone(),
            ));
        }
        Ok(expected)
    } else {
        if then_type != else_type {
            return Err(CompilerDiagnostic::type_mismatch(
                then_type,
                else_type,
                context,
                location.clone(),
            ));
        }
        Ok(then_type)
    }
}

/// Infers the result type from block-form branch bodies.
///
/// WHAT: when the receiver expects known types, returns the corresponding expression
/// type (single type or internal tuple type for multi-value). For inferred single-value
/// declarations, scans each branch for `ThenValue` nodes and returns the produced type.
/// WHY: block bodies may contain nested control flow; this extracts the type from
/// the first producing path it finds.
fn infer_block_result_type(
    then_body: &[AstNode],
    else_body: &[AstNode],
    expected_result_type_ids: &[TypeId],
    type_interner: &mut AstTypeInterner<'_>,
    location: &SourceLocation,
    receiver_kind: ValueReceiverKind,
) -> Result<TypeId, CompilerDiagnostic> {
    if expected_result_type_ids.len() > 1 {
        return Ok(type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(expected_result_type_ids.to_vec()));
    }

    if let Some(expected) = expected_result_type_ids.first().copied() {
        return Ok(expected);
    }

    let then_type = extract_single_produced_type(then_body);
    let else_type = extract_single_produced_type(else_body);

    let context = type_mismatch_context_for_receiver(receiver_kind);
    match (then_type, else_type) {
        (Some(t), Some(e)) => {
            if t != e {
                return Err(CompilerDiagnostic::type_mismatch(
                    t,
                    e,
                    context,
                    location.clone(),
                ));
            }
            Ok(t)
        }
        (Some(t), None) => Ok(t),
        (None, Some(e)) => Ok(e),
        (None, None) => Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        )),
    }
}

pub(super) fn validate_value_match_completeness(
    arms: &[MatchArm],
    default: Option<&[AstNode]>,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    let mut has_producing_path = false;

    for arm in arms {
        let flow = analyze_branch_flow(&arm.body);
        match flow {
            BranchFlow::ProducesValue => has_producing_path = true,
            BranchFlow::Terminates => {}
            BranchFlow::FallsThrough => {
                return Err(CompilerDiagnostic::invalid_control_flow_statement(
                    InvalidControlFlowStatementReason::ValueIfBranchFallsThrough,
                    location.clone(),
                ));
            }
        }
    }

    if let Some(default_body) = default {
        let flow = analyze_branch_flow(default_body);
        match flow {
            BranchFlow::ProducesValue => has_producing_path = true,
            BranchFlow::Terminates => {}
            BranchFlow::FallsThrough => {
                return Err(CompilerDiagnostic::invalid_control_flow_statement(
                    InvalidControlFlowStatementReason::ValueIfBranchFallsThrough,
                    location.clone(),
                ));
            }
        }
    }

    if has_producing_path {
        return Ok(());
    }

    Err(CompilerDiagnostic::invalid_control_flow_statement(
        InvalidControlFlowStatementReason::ValueIfNoProducingPath,
        location.clone(),
    ))
}

fn infer_value_match_result_type(
    arms: &[MatchArm],
    default: Option<&[AstNode]>,
    expected_result_type_ids: &[TypeId],
    type_interner: &mut AstTypeInterner<'_>,
    location: &SourceLocation,
    receiver_kind: ValueReceiverKind,
) -> Result<TypeId, CompilerDiagnostic> {
    if expected_result_type_ids.len() > 1 {
        return Ok(type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(expected_result_type_ids.to_vec()));
    }

    if let Some(expected) = expected_result_type_ids.first().copied() {
        return Ok(expected);
    }

    let produced_types = collect_value_match_single_produced_types(arms, default);
    let Some(first_type) = produced_types.first().copied() else {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        ));
    };

    let context = type_mismatch_context_for_receiver(receiver_kind);
    for produced_type in produced_types.iter().copied().skip(1) {
        if produced_type != first_type {
            return Err(CompilerDiagnostic::type_mismatch(
                first_type,
                produced_type,
                context,
                location.clone(),
            ));
        }
    }

    Ok(first_type)
}

pub(super) fn emit_collected_warnings(context: &ScopeContext, warnings: Vec<CompilerDiagnostic>) {
    for warning in warnings {
        context.emit_warning(warning);
    }
}

fn collect_value_match_single_produced_types(
    arms: &[MatchArm],
    default: Option<&[AstNode]>,
) -> Vec<TypeId> {
    let mut produced_types = Vec::new();

    for arm in arms {
        if let Some(type_id) = extract_single_produced_type(&arm.body) {
            produced_types.push(type_id);
        }
    }

    if let Some(default_body) = default
        && let Some(type_id) = extract_single_produced_type(default_body)
    {
        produced_types.push(type_id);
    }

    produced_types
}
