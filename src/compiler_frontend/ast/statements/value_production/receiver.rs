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
use crate::compiler_frontend::ast::statements::branching::{
    parse_match_block, parse_single_predicate_match_pattern,
};
use crate::compiler_frontend::ast::statements::condition_validation::ensure_if_statement_condition;
use crate::compiler_frontend::ast::statements::value_production::completeness::analyze_branch_flow;
use crate::compiler_frontend::ast::statements::value_production::extract_single_produced_type;
use crate::compiler_frontend::ast::statements::value_production::parse_values::{
    ProducedValuesParseInput, parse_fixed_arity_inferred_values, parse_produced_values_typed,
};
use crate::compiler_frontend::ast::statements::value_production::types::{
    ActiveValueProductionTarget, BranchFlow, ValueBlock, ValueIfBlock, ValueMatchBlock,
    ValueReceiverKind,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, function_body_to_ast};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidControlFlowStatementReason, InvalidReturnShapeReason,
    TypeMismatchContext,
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

fn current_if_header_is_full_match(token_stream: &FileTokens) -> bool {
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

fn same_logical_line(left: &SourceLocation, right: &SourceLocation) -> bool {
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

fn validate_value_match_completeness(
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

// ----------------------------
//  Multi-bind value blocks
// ----------------------------

/// Attempts to parse an `if`-headed value-producing block for multi-bind.
///
/// WHAT: when the current token is `if` and the receiver is a multi-bind site,
/// parses inline boolean `if`, block boolean `if`, or full-match forms, validates
/// arity, and returns a `ValueBlock` expression whose type is an internal tuple
/// with one slot per target.
/// WHY: multi-bind target inference means some slot types may not be known before
/// the RHS is parsed, so the standard `try_parse_value_block_at_receiver` (which
/// requires all expected types upfront) cannot handle every case.
#[allow(clippy::result_large_err)]
pub fn try_parse_multi_bind_value_block(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    target_count: usize,
    known_slot_types: &[Option<TypeId>],
    string_table: &mut StringTable,
) -> Option<Result<Expression, CompilerDiagnostic>> {
    if token_stream.current_token_kind() != &TokenKind::If {
        return None;
    }

    if let Some(expected_types) = collect_known_slot_types(known_slot_types) {
        return try_parse_value_block_at_receiver(
            token_stream,
            context,
            type_interner,
            &expected_types,
            ValueReceiverKind::MultiBind,
            string_table,
        );
    }

    Some(parse_inferred_multi_bind_value_block(
        token_stream,
        context,
        type_interner,
        target_count,
        known_slot_types,
        string_table,
    ))
}

fn parse_inferred_multi_bind_value_block(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    target_count: usize,
    known_slot_types: &[Option<TypeId>],
    string_table: &mut StringTable,
) -> Result<Expression, CompilerDiagnostic> {
    let location = token_stream.current_location();
    token_stream.advance(); // consume `if`

    if current_if_header_is_full_match(token_stream) {
        return parse_inferred_multi_bind_value_match(InferredMultiBindValueMatchInput {
            token_stream,
            context,
            type_interner,
            target_count,
            known_slot_types,
            string_table,
            location,
        });
    }

    let mut condition_type = ExpectedType::Infer;
    let condition = create_expression_until(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition, string_table),
        type_interner,
        &mut condition_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::Then, TokenKind::Colon],
        string_table,
    )?;
    ensure_if_statement_condition(&condition, type_interner.environment())?;

    if token_stream.current_token_kind() == &TokenKind::Then {
        return parse_inferred_inline_multi_bind_value_if(InferredMultiBindValueIfInput {
            token_stream,
            context,
            type_interner,
            target_count,
            known_slot_types,
            string_table,
            condition,
            location,
        });
    }

    if token_stream.current_token_kind() == &TokenKind::Colon {
        return parse_inferred_block_multi_bind_value_if(InferredMultiBindValueIfInput {
            token_stream,
            context,
            type_interner,
            target_count,
            known_slot_types,
            string_table,
            condition,
            location,
        });
    }

    Err(CompilerDiagnostic::invalid_control_flow_statement(
        InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
        token_stream.current_location(),
    ))
}

fn collect_known_slot_types(known_slot_types: &[Option<TypeId>]) -> Option<Vec<TypeId>> {
    let mut expected_types = Vec::with_capacity(known_slot_types.len());

    for slot_type in known_slot_types {
        expected_types.push((*slot_type)?);
    }

    Some(expected_types)
}

struct InferredMultiBindValueIfInput<'a, 'b> {
    token_stream: &'a mut FileTokens,
    context: &'a ScopeContext,
    type_interner: &'a mut AstTypeInterner<'b>,
    target_count: usize,
    known_slot_types: &'a [Option<TypeId>],
    string_table: &'a mut StringTable,
    condition: Expression,
    location: SourceLocation,
}

struct InferredMultiBindValueMatchInput<'a, 'b> {
    token_stream: &'a mut FileTokens,
    context: &'a ScopeContext,
    type_interner: &'a mut AstTypeInterner<'b>,
    target_count: usize,
    known_slot_types: &'a [Option<TypeId>],
    string_table: &'a mut StringTable,
    location: SourceLocation,
}

fn parse_inferred_multi_bind_value_match(
    input: InferredMultiBindValueMatchInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let InferredMultiBindValueMatchInput {
        token_stream,
        context,
        type_interner,
        target_count,
        known_slot_types,
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
        result_type_ids: vec![],
        receiver_kind: ValueReceiverKind::MultiBind,
        expected_arity: Some(target_count),
    };
    let mut warnings = Vec::new();
    let mut parsed_match = parse_match_block(
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

    let produced_value_sets =
        collect_match_multi_produced_values(&parsed_match.arms, parsed_match.default.as_deref());
    if produced_value_sets.is_empty() {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        ));
    }

    for values in &produced_value_sets {
        validate_optional_produced_arity(Some(values), target_count, &location)?;
    }

    let result_type_ids = infer_multi_bind_match_result_slots(
        &produced_value_sets,
        known_slot_types,
        type_interner.environment(),
        &location,
    )?;

    for arm in &mut parsed_match.arms {
        coerce_produced_values_in_body(
            &mut arm.body,
            &result_type_ids,
            type_interner.environment(),
        )?;
    }
    if let Some(default_body) = &mut parsed_match.default {
        coerce_produced_values_in_body(
            default_body,
            &result_type_ids,
            type_interner.environment(),
        )?;
    }

    build_multi_bind_value_match_expression(
        parsed_match.scrutinee,
        parsed_match.arms,
        parsed_match.default,
        parsed_match.exhaustiveness,
        result_type_ids,
        type_interner,
        location,
    )
}

fn parse_inferred_inline_multi_bind_value_if(
    input: InferredMultiBindValueIfInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let InferredMultiBindValueIfInput {
        token_stream,
        context,
        type_interner,
        target_count,
        known_slot_types,
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

    let then_values = parse_fixed_arity_inferred_values(
        token_stream,
        context,
        type_interner,
        target_count,
        string_table,
    )
    .map_err(|err| -> CompilerDiagnostic { err.into() })?;

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

    let else_values = parse_fixed_arity_inferred_values(
        token_stream,
        context,
        type_interner,
        target_count,
        string_table,
    )
    .map_err(|err| -> CompilerDiagnostic { err.into() })?;

    let result_type_ids = unify_and_validate_inferred_slots(
        &then_values,
        &else_values,
        known_slot_types,
        type_interner.environment(),
        &location,
    )?;

    let coerced_then =
        apply_coercion_to_values(then_values, &result_type_ids, type_interner.environment());
    let coerced_else =
        apply_coercion_to_values(else_values, &result_type_ids, type_interner.environment());

    build_multi_bind_value_if_expression(
        condition,
        coerced_then,
        coerced_else,
        result_type_ids,
        type_interner,
        location,
        context,
    )
}

fn parse_inferred_block_multi_bind_value_if(
    input: InferredMultiBindValueIfInput<'_, '_>,
) -> Result<Expression, CompilerDiagnostic> {
    let InferredMultiBindValueIfInput {
        token_stream,
        context,
        type_interner,
        target_count,
        known_slot_types,
        string_table,
        condition,
        location,
    } = input;

    token_stream.advance(); // consume `:`

    let active_target = ActiveValueProductionTarget {
        result_type_ids: vec![],
        receiver_kind: ValueReceiverKind::MultiBind,
        expected_arity: Some(target_count),
    };

    let mut then_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    then_context.active_value_target = Some(active_target.clone());
    let mut then_warnings = Vec::new();
    let mut then_body = function_body_to_ast(
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
    let mut else_body = function_body_to_ast(
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

    let then_values = extract_first_multi_produced_values(&then_body);
    let else_values = extract_first_multi_produced_values(&else_body);

    validate_optional_produced_arity(then_values.as_deref(), target_count, &location)?;
    validate_optional_produced_arity(else_values.as_deref(), target_count, &location)?;

    let result_type_ids = infer_multi_bind_result_slots(
        then_values.as_deref(),
        else_values.as_deref(),
        known_slot_types,
        type_interner.environment(),
        &location,
    )?;

    coerce_produced_values_in_body(
        &mut then_body,
        &result_type_ids,
        type_interner.environment(),
    )?;
    coerce_produced_values_in_body(
        &mut else_body,
        &result_type_ids,
        type_interner.environment(),
    )?;

    build_multi_bind_value_if_expression(
        condition,
        vec![], // not used for block form
        vec![], // not used for block form
        result_type_ids,
        type_interner,
        location,
        context,
    )
    .map(|mut expr| {
        // Replace the inline-constructed bodies with the real parsed bodies.
        if let ExpressionKind::ValueBlock { block } = &mut expr.kind
            && let ValueBlock::If(value_if) = block.as_mut()
        {
            value_if.then_body = then_body;
            value_if.else_body = else_body;
        }
        expr
    })
}

/// Derives slot types from branch expressions and validates them against known slots.
fn unify_and_validate_inferred_slots(
    then_values: &[Expression],
    else_values: &[Expression],
    known_slot_types: &[Option<TypeId>],
    type_environment: &TypeEnvironment,
    location: &SourceLocation,
) -> Result<Vec<TypeId>, CompilerDiagnostic> {
    let mut result_types = Vec::with_capacity(known_slot_types.len());

    for ((then_expr, else_expr), known_type) in then_values
        .iter()
        .zip(else_values.iter())
        .zip(known_slot_types.iter())
    {
        let slot_type = if let Some(known) = known_type {
            if then_expr.type_id != *known
                && !is_declaration_compatible(*known, then_expr.type_id, type_environment)
            {
                return Err(CompilerDiagnostic::type_mismatch(
                    *known,
                    then_expr.type_id,
                    TypeMismatchContext::Assignment,
                    then_expr.location.clone(),
                ));
            }
            if else_expr.type_id != *known
                && !is_declaration_compatible(*known, else_expr.type_id, type_environment)
            {
                return Err(CompilerDiagnostic::type_mismatch(
                    *known,
                    else_expr.type_id,
                    TypeMismatchContext::Assignment,
                    else_expr.location.clone(),
                ));
            }
            *known
        } else {
            if then_expr.type_id != else_expr.type_id {
                return Err(CompilerDiagnostic::type_mismatch(
                    then_expr.type_id,
                    else_expr.type_id,
                    TypeMismatchContext::Assignment,
                    location.clone(),
                ));
            }
            then_expr.type_id
        };
        result_types.push(slot_type);
    }

    Ok(result_types)
}

/// Infers block-form multi-bind result slots from whichever branch paths produce values.
///
/// WHAT: combines first produced values from the true and false branch, while allowing either
/// branch to terminate instead of producing values.
/// WHY: value-producing blocks are complete when every path either produces or terminates;
/// inferred multi-bind must not require both top-level branches to produce just to learn a type.
fn infer_multi_bind_result_slots(
    then_values: Option<&[Expression]>,
    else_values: Option<&[Expression]>,
    known_slot_types: &[Option<TypeId>],
    type_environment: &TypeEnvironment,
    location: &SourceLocation,
) -> Result<Vec<TypeId>, CompilerDiagnostic> {
    if then_values.is_none() && else_values.is_none() {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        ));
    }

    let mut result_types = Vec::with_capacity(known_slot_types.len());

    for (slot_index, known_type) in known_slot_types.iter().enumerate() {
        let then_expr = then_values.and_then(|values| values.get(slot_index));
        let else_expr = else_values.and_then(|values| values.get(slot_index));

        let slot_type = if let Some(known_type) = known_type {
            validate_expression_against_slot(then_expr, *known_type, type_environment, location)?;
            validate_expression_against_slot(else_expr, *known_type, type_environment, location)?;
            *known_type
        } else {
            infer_unknown_slot_type(then_expr, else_expr, location)?
        };

        result_types.push(slot_type);
    }

    Ok(result_types)
}

fn emit_collected_warnings(context: &ScopeContext, warnings: Vec<CompilerDiagnostic>) {
    for warning in warnings {
        context.emit_warning(warning);
    }
}

fn collect_match_multi_produced_values(
    arms: &[MatchArm],
    default: Option<&[AstNode]>,
) -> Vec<Vec<Expression>> {
    let mut produced_value_sets = Vec::new();

    for arm in arms {
        if let Some(values) = extract_first_multi_produced_values(&arm.body) {
            produced_value_sets.push(values);
        }
    }

    if let Some(default_body) = default
        && let Some(values) = extract_first_multi_produced_values(default_body)
    {
        produced_value_sets.push(values);
    }

    produced_value_sets
}

fn infer_multi_bind_match_result_slots(
    produced_value_sets: &[Vec<Expression>],
    known_slot_types: &[Option<TypeId>],
    type_environment: &TypeEnvironment,
    location: &SourceLocation,
) -> Result<Vec<TypeId>, CompilerDiagnostic> {
    let mut result_types = Vec::with_capacity(known_slot_types.len());

    for (slot_index, known_type) in known_slot_types.iter().enumerate() {
        let slot_type = if let Some(known_type) = known_type {
            for values in produced_value_sets {
                validate_expression_against_slot(
                    values.get(slot_index),
                    *known_type,
                    type_environment,
                    location,
                )?;
            }
            *known_type
        } else {
            infer_unknown_match_slot_type(produced_value_sets, slot_index, location)?
        };

        result_types.push(slot_type);
    }

    Ok(result_types)
}

fn infer_unknown_match_slot_type(
    produced_value_sets: &[Vec<Expression>],
    slot_index: usize,
    location: &SourceLocation,
) -> Result<TypeId, CompilerDiagnostic> {
    let mut inferred_type: Option<TypeId> = None;

    for values in produced_value_sets {
        let Some(expression) = values.get(slot_index) else {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::ValueIfNoProducingPath,
                location.clone(),
            ));
        };

        if let Some(existing) = inferred_type {
            if existing != expression.type_id {
                return Err(CompilerDiagnostic::type_mismatch(
                    existing,
                    expression.type_id,
                    TypeMismatchContext::Assignment,
                    location.clone(),
                ));
            }
        } else {
            inferred_type = Some(expression.type_id);
        }
    }

    inferred_type.ok_or_else(|| {
        CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        )
    })
}

fn infer_unknown_slot_type(
    then_expr: Option<&Expression>,
    else_expr: Option<&Expression>,
    location: &SourceLocation,
) -> Result<TypeId, CompilerDiagnostic> {
    match (then_expr, else_expr) {
        (Some(then_expr), Some(else_expr)) => {
            if then_expr.type_id != else_expr.type_id {
                return Err(CompilerDiagnostic::type_mismatch(
                    then_expr.type_id,
                    else_expr.type_id,
                    TypeMismatchContext::Assignment,
                    location.clone(),
                ));
            }

            Ok(then_expr.type_id)
        }

        (Some(expression), None) | (None, Some(expression)) => Ok(expression.type_id),

        (None, None) => Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        )),
    }
}

fn validate_expression_against_slot(
    expression: Option<&Expression>,
    expected_type: TypeId,
    type_environment: &TypeEnvironment,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    let Some(expression) = expression else {
        return Ok(());
    };

    if expression.type_id == expected_type
        || is_declaration_compatible(expected_type, expression.type_id, type_environment)
    {
        return Ok(());
    }

    Err(CompilerDiagnostic::type_mismatch(
        expected_type,
        expression.type_id,
        TypeMismatchContext::Assignment,
        location.clone(),
    ))
}

fn validate_optional_produced_arity(
    values: Option<&[Expression]>,
    target_count: usize,
    location: &SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    let Some(values) = values else {
        return Ok(());
    };

    if values.len() == target_count {
        return Ok(());
    }

    if values.len() > target_count {
        return Err(CompilerDiagnostic::invalid_return_shape(
            InvalidReturnShapeReason::TooManyReturnValues {
                expected_count: target_count,
            },
            location.clone(),
        ));
    }

    Err(CompilerDiagnostic::invalid_return_shape(
        InvalidReturnShapeReason::TooFewReturnValues {
            expected_count: target_count,
            provided_count: values.len(),
        },
        location.clone(),
    ))
}

/// Wraps expressions in `Coerced` nodes where the target type differs from the natural type.
fn apply_coercion_to_values(
    values: Vec<Expression>,
    target_types: &[TypeId],
    type_environment: &TypeEnvironment,
) -> Vec<Expression> {
    values
        .into_iter()
        .zip(target_types.iter())
        .map(|(expr, target_type)| {
            if expr.type_id != *target_type
                && is_declaration_compatible(*target_type, expr.type_id, type_environment)
            {
                return Expression::coerced(expr, *target_type);
            }
            expr
        })
        .collect()
}

/// Extracts the first multi-value `ThenValue` found on a reachable path.
fn extract_first_multi_produced_values(body: &[AstNode]) -> Option<Vec<Expression>> {
    for statement in body {
        match &statement.kind {
            NodeKind::ThenValue(produced_values) => {
                return Some(produced_values.expressions.clone());
            }

            NodeKind::If(_, then_body, Some(else_body)) => {
                if let Some(then_values) = extract_first_multi_produced_values(then_body) {
                    return Some(then_values);
                }
                return extract_first_multi_produced_values(else_body);
            }

            NodeKind::If(_, then_body, None) => {
                return extract_first_multi_produced_values(then_body);
            }

            NodeKind::Match { arms, default, .. } => {
                for arm in arms {
                    if let Some(arm_values) = extract_first_multi_produced_values(&arm.body) {
                        return Some(arm_values);
                    }
                }
                if let Some(default_body) = default {
                    return extract_first_multi_produced_values(default_body);
                }
                return None;
            }

            NodeKind::Return(_) | NodeKind::ReturnError(_) => return None,

            _ => {}
        }
    }

    None
}

/// Mutates `ThenValue` expressions in a body to apply coercion when needed.
fn coerce_produced_values_in_body(
    body: &mut [AstNode],
    expected_types: &[TypeId],
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    for node in body {
        match &mut node.kind {
            NodeKind::ThenValue(produced_values) => {
                if produced_values.expressions.len() != expected_types.len() {
                    return validate_optional_produced_arity(
                        Some(&produced_values.expressions),
                        expected_types.len(),
                        &produced_values.location,
                    );
                }

                for (expr, expected_type) in produced_values
                    .expressions
                    .iter_mut()
                    .zip(expected_types.iter())
                {
                    if expr.type_id == *expected_type {
                        continue;
                    }

                    if !is_declaration_compatible(*expected_type, expr.type_id, type_environment) {
                        return Err(CompilerDiagnostic::type_mismatch(
                            *expected_type,
                            expr.type_id,
                            TypeMismatchContext::Assignment,
                            expr.location.clone(),
                        ));
                    }

                    *expr = Expression::coerced(expr.clone(), *expected_type);
                }
            }

            NodeKind::If(_, then_body, Some(else_body)) => {
                coerce_produced_values_in_body(then_body, expected_types, type_environment)?;
                coerce_produced_values_in_body(else_body, expected_types, type_environment)?;
            }

            NodeKind::If(_, then_body, None) => {
                coerce_produced_values_in_body(then_body, expected_types, type_environment)?;
            }

            NodeKind::Match { arms, default, .. } => {
                for arm in arms.iter_mut() {
                    coerce_produced_values_in_body(
                        &mut arm.body,
                        expected_types,
                        type_environment,
                    )?;
                }
                if let Some(default_body) = default {
                    coerce_produced_values_in_body(default_body, expected_types, type_environment)?;
                }
            }

            NodeKind::Return(_) | NodeKind::ReturnError(_) => {}

            _ => {}
        }
    }

    Ok(())
}

/// Builds the final `ValueBlock::If` expression for multi-bind.
fn build_multi_bind_value_if_expression(
    condition: Expression,
    then_values: Vec<Expression>,
    else_values: Vec<Expression>,
    result_type_ids: Vec<TypeId>,
    type_interner: &mut AstTypeInterner<'_>,
    location: SourceLocation,
    context: &ScopeContext,
) -> Result<Expression, CompilerDiagnostic> {
    let result_type_id = type_interner
        .environment_mut_for_derived_types()
        .intern_tuple(result_type_ids.clone());

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
        result_type_ids,
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

fn build_multi_bind_value_match_expression(
    scrutinee: Expression,
    arms: Vec<MatchArm>,
    default: Option<Vec<AstNode>>,
    exhaustiveness: MatchExhaustiveness,
    result_type_ids: Vec<TypeId>,
    type_interner: &mut AstTypeInterner<'_>,
    location: SourceLocation,
) -> Result<Expression, CompilerDiagnostic> {
    let result_type_id = type_interner
        .environment_mut_for_derived_types()
        .intern_tuple(result_type_ids.clone());

    let value_match = ValueMatchBlock {
        scrutinee,
        arms,
        default,
        exhaustiveness,
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
