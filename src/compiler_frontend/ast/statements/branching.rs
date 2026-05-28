//! Match and if/else branching AST construction.
//!
//! WHAT: parses `if`/`else` conditionals and `if value is:` match statements
//! into AST branch and match nodes.
//! WHY: match parsing centralizes exhaustiveness checking, deferred-feature
//! rejection, and choice-variant resolution at the AST level so HIR lowering
//! receives validated, normalized match structures.

use crate::ast_log;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, MatchExhaustiveness, NodeKind,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_expression_until,
};
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::condition_validation::{
    ensure_if_statement_condition, ensure_match_guard_condition,
};
use crate::compiler_frontend::ast::statements::match_arm_boundaries::{
    current_line_contains_top_level_colon, current_token_starts_match_arm_header,
    token_index_has_top_level_fat_arrow, token_is_line_initial,
};
use crate::compiler_frontend::ast::statements::match_patterns::{
    ChoicePayloadCapture, MatchArm, MatchPattern, ParsedChoicePattern,
    parse_choice_variant_pattern, parse_non_choice_pattern, parse_option_pattern,
};
use crate::compiler_frontend::ast::statements::value_production::types::ActiveValueProductionTarget;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, InvalidControlFlowStatementReason,
    InvalidMatchArmReason, InvalidMatchPatternReason, NonExhaustiveMatchReason,
};
use crate::compiler_frontend::datatypes::definitions::ChoiceVariantPayloadDefinition;
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::datatypes::queries::TypeKind;
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_reason_diagnostic;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::token_scan::find_expression_end_index;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::type_coercion::parse_context::ExpectedType;
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashSet;

/// Intermediate result of parsing a single match arm, carrying the arm itself
/// plus metadata needed for duplicate and exhaustiveness checking.
struct ParsedMatchArm {
    arm: MatchArm,
    /// Tracks which choice variant this arm consumes so duplicates can be rejected early.
    matched_choice_variant: Option<StringId>,
    pattern_location: SourceLocation,
}

/// Parsed pattern header shared by full match arms and inline single-predicate value `if`.
///
/// WHAT: carries the normalized pattern plus any arm-local scope introduced by
/// captures.
/// WHY: inline `if value is Pattern then ... else ...` must use the same pattern
/// parser as full match arms without manufacturing a temporary `=>` body.
pub(crate) struct ParsedSinglePredicatePattern {
    pub pattern: MatchPattern,
    pub arm_scope: ScopeContext,
}

struct ParsedMatchPatternHeader {
    pattern: MatchPattern,
    matched_choice_variant: Option<StringId>,
    pattern_location: SourceLocation,
    arm_scope: ScopeContext,
}

/// Parsed full-match block payload shared by statement and value-producing matches.
///
/// WHAT: carries the arm/default bodies plus the exhaustiveness contract after all
/// pattern parsing and validation has completed.
/// WHY: value-producing matches must reuse statement match parsing rules without
/// manufacturing a temporary statement node.
pub(crate) struct ParsedMatchBlock {
    pub scrutinee: Expression,
    pub arms: Vec<MatchArm>,
    pub default: Option<Vec<AstNode>>,
    pub exhaustiveness: MatchExhaustiveness,
    pub location: SourceLocation,
    pub scope: crate::compiler_frontend::interned_path::InternedPath,
}

/// Hashable key for comparing literal match patterns.
///
/// WHAT: extracts the normalized value from an `ExpressionKind` so duplicate literal
/// arms can be detected without requiring `PartialEq` on the full `Expression` type.
/// WHY: keeps the comparison local to the match parser and avoids adding derived
/// equality to the entire expression hierarchy.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum LiteralPatternKey {
    Int(i64),
    Float(u64), // stored as to_bits for hashable equality
    StringSlice(StringId),
    Bool(bool),
    Char(char),
}

/// Extract a hashable key from a literal expression for duplicate-arm detection.
fn extract_literal_key(expression: &Expression) -> Option<LiteralPatternKey> {
    match &expression.kind {
        ExpressionKind::Int(value) => Some(LiteralPatternKey::Int(*value)),
        ExpressionKind::Float(value) => Some(LiteralPatternKey::Float(value.to_bits())),
        ExpressionKind::StringSlice(id) => Some(LiteralPatternKey::StringSlice(*id)),
        ExpressionKind::Bool(value) => Some(LiteralPatternKey::Bool(*value)),
        ExpressionKind::Char(value) => Some(LiteralPatternKey::Char(*value)),
        _ => None,
    }
}

/// Peek at the next non-newline token without advancing the stream.
fn peek_next_non_newline_token(token_stream: &FileTokens) -> Option<&Token> {
    token_stream
        .tokens
        .iter()
        .skip(token_stream.index + 1)
        .find(|token| token.kind != TokenKind::Newline)
}

/// Peek at the index of the next non-newline token without advancing the stream.
fn peek_next_non_newline_token_index(token_stream: &FileTokens) -> Option<usize> {
    token_stream
        .tokens
        .iter()
        .enumerate()
        .skip(token_stream.index + 1)
        .find(|(_, token)| token.kind != TokenKind::Newline)
        .map(|(i, _)| i)
}

/// Check whether the upcoming tokens form `if <expr> is |...|:`.
///
/// WHAT: scans for a top-level `is` before a top-level colon. If `is` exists and
/// the next token is `|`, this is a single-predicate option match statement.
/// WHY: `|name|` is not a valid expression, so the normal expression parser would
/// fail. Detecting the shape early lets us parse the scrutinee and pattern separately.
fn is_single_predicate_option_match(token_stream: &FileTokens) -> bool {
    let is_index = find_expression_end_index(
        &token_stream.tokens,
        token_stream.index,
        &[TokenKind::Is, TokenKind::Colon],
    );
    if is_index >= token_stream.length {
        return false;
    }
    if token_stream.tokens[is_index].kind != TokenKind::Is {
        return false;
    }
    token_stream
        .tokens
        .get(is_index + 1)
        .is_some_and(|t| t.kind == TokenKind::TypeParameterBracket)
}

/// Parse an `if` statement or an `if <subject> is:` match statement.
///
/// WHAT: detects single-predicate option matches (`if option is |name|:`) before
/// falling back to normal `if`/`else` parsing or statement-style match parsing.
/// WHY: the single-predicate and statement-match shapes share the `if` keyword,
/// so this entry point disambiguates them early based on token lookahead.
#[allow(clippy::result_large_err)]
pub fn create_branch(
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerDiagnostic> {
    // Single-predicate option match: `if option is |name|:`.
    // Detected before normal expression parsing because `|name|` is not a valid expression.
    if is_single_predicate_option_match(token_stream) {
        let mut condition_type = ExpectedType::Infer;
        let scrutinee = create_expression_until(
            token_stream,
            &context.new_child_control_flow(ContextKind::Condition, string_table),
            type_interner,
            &mut condition_type,
            &ValueMode::ImmutableOwned,
            &[TokenKind::Is],
            string_table,
        )?;
        token_stream.advance(); // consume `is`

        let type_environment = type_interner.environment();
        let Some(inner_type_id) = type_environment.option_inner_type(scrutinee.type_id) else {
            return Err(CompilerDiagnostic::invalid_match_pattern(
                InvalidMatchPatternReason::OptionPresentCaptureOnNonOptional,
                None,
                None,
                scrutinee.location.clone(),
            ));
        };

        let pattern =
            parse_option_pattern(token_stream, inner_type_id, string_table, type_environment)?;

        let (arm_scope, pattern) = match &pattern {
            MatchPattern::OptionPresentCapture {
                name,
                binding_location,
                inner_type_id: capture_inner_type_id,
                location: pattern_location,
                ..
            } => build_arm_scope_with_option_present_capture(
                context,
                *name,
                binding_location,
                *capture_inner_type_id,
                pattern_location,
                type_interner,
                string_table,
            )?,
            _ => {
                return Err(CompilerDiagnostic::invalid_match_pattern(
                    InvalidMatchPatternReason::ExpectedBindingInOptionPresentCapture,
                    None,
                    None,
                    pattern.location().clone(),
                ));
            }
        };

        if token_stream.current_token_kind() != &TokenKind::Colon {
            return Err(CompilerDiagnostic::invalid_control_flow_statement(
                InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
                token_stream.current_location(),
            ));
        }
        token_stream.advance();

        let body = function_body_to_ast(
            token_stream,
            arm_scope.new_child_control_flow(ContextKind::Branch, string_table),
            type_interner,
            warnings,
            string_table,
        )?;

        let else_block = if token_stream.current_token_kind() == &TokenKind::Else {
            token_stream.advance();
            let else_context = context.new_child_control_flow(ContextKind::Branch, string_table);
            Some(function_body_to_ast(
                token_stream,
                else_context,
                type_interner,
                warnings,
                string_table,
            )?)
        } else {
            None
        };

        // Single-predicate option match without `else` gets an implicit empty default
        // so the `none` case silently falls through, consistent with `if` statement semantics.
        let (default, exhaustiveness) = if let Some(else_body) = else_block {
            (Some(else_body), MatchExhaustiveness::HasDefault)
        } else {
            (Some(vec![]), MatchExhaustiveness::HasDefault)
        };

        return Ok(vec![AstNode {
            kind: NodeKind::Match {
                scrutinee,
                arms: vec![MatchArm {
                    pattern,
                    guard: None,
                    body,
                }],
                default,
                exhaustiveness,
            },
            location: token_stream.current_location(),
            scope: context.scope.clone(),
        }]);
    }

    let mut condition_type = ExpectedType::Infer;
    let condition = create_expression(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition, string_table),
        type_interner,
        &mut condition_type,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    // `if value is:` starts a statement-style match arm block.
    if token_stream.current_token_kind() == &TokenKind::Is {
        token_stream.advance();
        let match_statement = create_match_node(
            condition,
            token_stream,
            context,
            type_interner,
            warnings,
            string_table,
        )?;
        return Ok(vec![match_statement]);
    }

    let type_environment = type_interner.environment();
    ensure_if_statement_condition(&condition, type_environment)?;

    ast_log!("Creating If Statement");
    if token_stream.current_token_kind() != &TokenKind::Colon {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
            token_stream.current_location(),
        ));
    }

    token_stream.advance();
    let then_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    let then_scope = then_context.scope.clone();
    let then_block = function_body_to_ast(
        token_stream,
        then_context,
        type_interner,
        warnings,
        string_table,
    )?;

    let else_block = if token_stream.current_token_kind() == &TokenKind::Else {
        token_stream.advance();
        let else_context = context.new_child_control_flow(ContextKind::Branch, string_table);
        Some(function_body_to_ast(
            token_stream,
            else_context,
            type_interner,
            warnings,
            string_table,
        )?)
    } else {
        None
    };

    Ok(vec![AstNode {
        kind: NodeKind::If(condition, then_block, else_block),
        location: token_stream.current_location(),
        scope: then_scope,
    }])
}

/// Parse a complete `if <subject> is:` match statement into a `NodeKind::Match`.
///
/// WHAT: loops through pattern/`else` arms, validates ordering and uniqueness, then
/// delegates exhaustiveness checking before returning the match node.
/// WHY: all match-level invariants (at least one pattern arm before else, no duplicates,
/// exhaustiveness) are enforced here so downstream HIR lowering can assume valid input.
#[allow(clippy::result_large_err)]
fn create_match_node(
    scrutinee: Expression,
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerDiagnostic> {
    let parsed_match = parse_match_block(
        scrutinee,
        token_stream,
        context,
        type_interner,
        warnings,
        None,
        string_table,
    )?;

    Ok(AstNode {
        kind: NodeKind::Match {
            scrutinee: parsed_match.scrutinee,
            arms: parsed_match.arms,
            default: parsed_match.default,
            exhaustiveness: parsed_match.exhaustiveness,
        },
        location: parsed_match.location,
        scope: parsed_match.scope,
    })
}

/// Parse the shared contents of a full `if <subject> is:` match block.
///
/// WHAT: validates arm syntax, captures, guards, duplicate/unreachable arms, and
/// exhaustiveness, then returns the parsed match payload.
/// WHY: statement matches and value-producing matches have identical pattern
/// semantics; the value form only changes the active value target while parsing
/// arm bodies.
#[allow(clippy::result_large_err)]
pub(crate) fn parse_match_block(
    scrutinee: Expression,
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    warnings: &mut Vec<CompilerDiagnostic>,
    active_value_target: Option<ActiveValueProductionTarget>,
    string_table: &mut StringTable,
) -> Result<ParsedMatchBlock, CompilerDiagnostic> {
    ast_log!("Creating Match Statement");

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ExpectedColonAfterCondition,
            token_stream.current_location(),
        ));
    }

    token_stream.advance();
    let mut match_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    match_context.active_value_target = active_value_target;

    let mut arms: Vec<MatchArm> = Vec::new();
    let mut else_block = None;
    let mut seen_else = false;
    let mut has_guarded_arms = false;
    let mut match_arm_indent: Option<i32> = None;
    // Choice exhaustiveness/duplication checks rely on the set of consumed variant names.
    let mut matched_choice_variants: FxHashSet<StringId> = FxHashSet::default();
    // Tracks literal patterns already seen so duplicate literal arms can be warned.
    let mut matched_literal_patterns: FxHashSet<LiteralPatternKey> = FxHashSet::default();
    // Once an unconditional capture pattern is seen, all later arms are unreachable.
    let mut seen_unconditional_capture = false;
    // Tracks whether an unguarded `none` arm has been seen for duplicate detection.
    let mut seen_unguarded_none = false;
    // Tracks whether an unguarded `|name|` arm has been seen for exhaustiveness.
    let mut seen_unguarded_present_capture = false;

    // ----------------------------
    //  Parse match arms
    // ----------------------------
    loop {
        token_stream.skip_newlines();

        match token_stream.current_token_kind() {
            TokenKind::End => {
                let next_token = peek_next_non_newline_token(token_stream);
                let next_index = peek_next_non_newline_token_index(token_stream);
                let semicolon_separates_same_level_arms =
                    match (match_arm_indent, next_token, next_index) {
                        (Some(arm_indent), Some(next), Some(idx))
                            if next.kind == TokenKind::Else
                                || token_index_has_top_level_fat_arrow(token_stream, idx) =>
                        {
                            next.location.start_pos.char_column == arm_indent
                        }
                        _ => false,
                    };

                if semicolon_separates_same_level_arms {
                    return Err(CompilerDiagnostic::invalid_match_arm(
                        InvalidMatchArmReason::SemicolonDelimiter,
                        token_stream.current_location(),
                    ));
                }
                token_stream.advance();
                break;
            }

            TokenKind::Eof => {
                return Err(CompilerDiagnostic::invalid_control_flow_statement(
                    InvalidControlFlowStatementReason::UnexpectedEndOfFileInMatch,
                    token_stream.current_location(),
                ));
            }

            TokenKind::Else => {
                match_arm_indent
                    .get_or_insert(token_stream.current_location().start_pos.char_column);

                if arms.is_empty() {
                    return Err(CompilerDiagnostic::invalid_control_flow_statement(
                        InvalidControlFlowStatementReason::CaseRequiredBeforeElse,
                        token_stream.current_location(),
                    ));
                }

                if seen_else {
                    return Err(CompilerDiagnostic::invalid_control_flow_statement(
                        InvalidControlFlowStatementReason::DuplicateElseArm,
                        token_stream.current_location(),
                    ));
                }
                seen_else = true;

                if seen_unconditional_capture {
                    warnings.push(CompilerDiagnostic::unreachable_match_arm(
                        token_stream.current_location(),
                    ));
                }

                else_block = Some(parse_else_arm(
                    token_stream,
                    &match_context,
                    type_interner,
                    warnings,
                    string_table,
                )?);
            }

            TokenKind::Case => {
                return Err(CompilerDiagnostic::invalid_match_arm(
                    InvalidMatchArmReason::RemovedCaseKeyword,
                    token_stream.current_location(),
                ));
            }

            // Normal pattern arm or malformed header — delegate to dedicated parsing.
            _ => {
                if let Some(candidate) = current_token_starts_match_arm_header(token_stream) {
                    debug_assert_eq!(candidate.start_index, token_stream.index);
                    debug_assert!(candidate.arrow_index > candidate.start_index);
                    match_arm_indent.get_or_insert(candidate.start_location.start_pos.char_column);

                    let parsed = parse_match_arm(
                        &scrutinee,
                        token_stream,
                        &match_context,
                        type_interner,
                        warnings,
                        string_table,
                    )?;

                    let option_present_arm_after_catch_all = seen_unguarded_present_capture
                        && matches!(
                            parsed.arm.pattern,
                            MatchPattern::OptionValue { .. }
                                | MatchPattern::Relational { .. }
                                | MatchPattern::OptionPresentCapture { .. }
                        );

                    if seen_else || seen_unconditional_capture || option_present_arm_after_catch_all
                    {
                        warnings.push(CompilerDiagnostic::unreachable_match_arm(
                            parsed.pattern_location.clone(),
                        ));
                    } else {
                        if let Some(variant_name) = parsed.matched_choice_variant
                            && !matched_choice_variants.insert(variant_name)
                        {
                            warnings.push(CompilerDiagnostic::unreachable_match_arm(
                                parsed.pattern_location.clone(),
                            ));
                        }

                        if let MatchPattern::Literal(expr) = &parsed.arm.pattern
                            && let Some(key) = extract_literal_key(expr)
                            && !matched_literal_patterns.insert(key)
                        {
                            warnings.push(CompilerDiagnostic::unreachable_match_arm(
                                parsed.pattern_location.clone(),
                            ));
                        }

                        match &parsed.arm.pattern {
                            MatchPattern::Capture { .. } if parsed.arm.guard.is_none() => {
                                seen_unconditional_capture = true;
                            }
                            MatchPattern::OptionPresentCapture { .. }
                                if parsed.arm.guard.is_none() =>
                            {
                                seen_unguarded_present_capture = true;
                            }
                            MatchPattern::OptionNone { .. } if parsed.arm.guard.is_none() => {
                                if seen_unguarded_none {
                                    warnings.push(CompilerDiagnostic::unreachable_match_arm(
                                        parsed.pattern_location.clone(),
                                    ));
                                }
                                seen_unguarded_none = true;
                            }
                            _ => {}
                        }
                    }

                    has_guarded_arms |= parsed.arm.guard.is_some();
                    arms.push(parsed.arm);
                    continue;
                }

                if token_is_line_initial(token_stream, token_stream.index)
                    && current_line_contains_top_level_colon(token_stream)
                {
                    return Err(CompilerDiagnostic::invalid_match_arm(
                        InvalidMatchArmReason::LegacyColonSyntax,
                        token_stream.current_location(),
                    ));
                }
                return Err(CompilerDiagnostic::invalid_match_arm(
                    InvalidMatchArmReason::ExpectedArmHeader,
                    token_stream.current_location(),
                ));
            }
        }
    }

    // ----------------------------
    //  Enforce exhaustiveness and build node
    // ----------------------------
    enforce_match_exhaustiveness(
        &scrutinee,
        &else_block,
        has_guarded_arms,
        &matched_choice_variants,
        OptionExhaustivenessState {
            seen_unguarded_none,
            seen_unguarded_present_capture,
        },
        string_table,
        type_interner.environment(),
    )?;

    let exhaustiveness = if else_block.is_some() {
        MatchExhaustiveness::HasDefault
    } else {
        MatchExhaustiveness::ExhaustiveChoice
    };

    Ok(ParsedMatchBlock {
        arms,
        default: else_block,
        exhaustiveness,
        location: token_stream.current_location(),
        scope: match_context.scope,
        scrutinee,
    })
}

#[allow(clippy::result_large_err)]
fn parse_else_arm(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerDiagnostic> {
    token_stream.advance();
    token_stream.skip_newlines();

    if token_stream.current_token_kind() == &TokenKind::Colon {
        return Err(CompilerDiagnostic::invalid_match_arm(
            InvalidMatchArmReason::LegacyElseSyntax,
            token_stream.current_location(),
        ));
    }

    if token_stream.current_token_kind() == &TokenKind::Arrow {
        return Err(CompilerDiagnostic::invalid_match_arm(
            InvalidMatchArmReason::InvalidArrow,
            token_stream.current_location(),
        ));
    }

    if token_stream.current_token_kind() != &TokenKind::FatArrow {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ExpectedFatArrow,
            token_stream.current_location(),
        ));
    }

    token_stream.advance();
    let arm_context = new_match_arm_body_context(match_context, string_table);
    function_body_to_ast(
        token_stream,
        arm_context,
        type_interner,
        warnings,
        string_table,
    )
}

/// Parse an optional `if <condition>` guard before the `=>` separator.
///
/// WHY: guard parsing is self-contained (token check, expression parse, validation)
/// and extracting it keeps `parse_match_arm` focused on arm-level syntax.
#[allow(clippy::result_large_err)]
fn parse_match_guard(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerDiagnostic> {
    if token_stream.current_token_kind() != &TokenKind::If {
        return Ok(None);
    }

    token_stream.advance();
    token_stream.skip_newlines();

    let mut guard_type = ExpectedType::Infer;
    let guard_expression = create_expression_until(
        token_stream,
        &match_context.new_child_control_flow(ContextKind::Condition, string_table),
        type_interner,
        &mut guard_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::FatArrow],
        string_table,
    )?;
    let type_environment = type_interner.environment();
    ensure_match_guard_condition(&guard_expression, type_environment)?;

    Ok(Some(guard_expression))
}

/// Parse a single `<pattern> => <body>` arm.
///
/// WHAT: dispatches to choice-variant or literal pattern parsing based on the
/// scrutinee type, validates the `=>` separator, and parses the arm body.
/// WHY: separating choice and literal paths here keeps each pattern parser focused
/// on one concern while this function handles shared arm-level syntax validation.
///
/// ENTRY INVARIANT: the token stream is already positioned at the first token of the
/// pattern; this function does not advance before parsing.
#[allow(clippy::result_large_err)]
fn parse_match_arm(
    scrutinee: &Expression,
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
) -> Result<ParsedMatchArm, CompilerDiagnostic> {
    let ParsedMatchPatternHeader {
        pattern,
        matched_choice_variant,
        pattern_location,
        arm_scope,
    } = parse_match_pattern_header(
        scrutinee,
        token_stream,
        match_context,
        type_interner,
        string_table,
    )?;

    reject_invalid_pattern_suffix(token_stream)?;

    let guard = parse_match_guard(token_stream, &arm_scope, type_interner, string_table)?;

    if token_stream.current_token_kind() != &TokenKind::FatArrow {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ExpectedFatArrow,
            token_stream.current_location(),
        ));
    }

    token_stream.advance();
    let arm_body_context = new_match_arm_body_context(&arm_scope, string_table);
    let body = function_body_to_ast(
        token_stream,
        arm_body_context,
        type_interner,
        warnings,
        string_table,
    )?;

    Ok(ParsedMatchArm {
        arm: MatchArm {
            pattern,
            guard,
            body,
        },
        matched_choice_variant,
        pattern_location,
    })
}

/// Parse one pattern after `if <scrutinee> is` for inline value-producing `if`.
///
/// WHAT: reuses the same pattern resolution as full match arms, including choice
/// variant qualification and capture scope construction.
/// WHY: inline single-predicate value `if` must not resolve variant names as
/// ordinary values; full match parsing is the owner of that semantics.
#[allow(clippy::result_large_err)]
pub(crate) fn parse_single_predicate_match_pattern(
    scrutinee: &Expression,
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<ParsedSinglePredicatePattern, CompilerDiagnostic> {
    let parsed = parse_match_pattern_header(
        scrutinee,
        token_stream,
        match_context,
        type_interner,
        string_table,
    )?;

    reject_invalid_pattern_suffix(token_stream)?;

    Ok(ParsedSinglePredicatePattern {
        pattern: parsed.pattern,
        arm_scope: parsed.arm_scope,
    })
}

#[allow(clippy::result_large_err)]
fn parse_match_pattern_header(
    scrutinee: &Expression,
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<ParsedMatchPatternHeader, CompilerDiagnostic> {
    // Choice scrutinees resolve symbols to variants; all other scrutinees stay literal-only.
    let type_environment = type_interner.environment();
    let is_choice = matches!(
        type_environment.type_kind(scrutinee.type_id),
        Some(TypeKind::Choice | TypeKind::GenericInstance)
    );

    let (pattern, matched_choice_variant, pattern_location, arm_scope) = if is_choice {
        let type_environment = type_interner.environment();
        let variants = choice_variants_for_type(scrutinee.type_id, type_environment);
        let nominal_path = type_environment
            .nominal_path(scrutinee.type_id)
            .cloned()
            .unwrap_or_else(|| match_context.scope.clone());
        // If the lead token is a bare symbol that is not a known variant name
        // and is not introducing a qualified variant pattern, treat it as a
        // general capture pattern that binds the whole choice value.
        let maybe_capture = if let TokenKind::Symbol(name) = token_stream.current_token_kind() {
            let name = *name;
            let is_qualified = token_stream
                .tokens
                .get(token_stream.index + 1)
                .is_some_and(|t| t.kind == TokenKind::DoubleColon);
            if !is_qualified && !variants.iter().any(|v| v.id == name) {
                Some((name, token_stream.current_location()))
            } else {
                None
            }
        } else {
            None
        };

        if let Some((capture_name, capture_location)) = maybe_capture {
            token_stream.advance();
            let (arm_scope, pattern) = build_arm_scope_with_capture(
                match_context,
                capture_name,
                &capture_location,
                scrutinee.type_id,
                type_interner,
                string_table,
            )?;
            (pattern, None, capture_location.clone(), arm_scope)
        } else {
            let parsed = parse_choice_variant_pattern(
                token_stream,
                match_context,
                &nominal_path,
                &variants,
                string_table,
            )?;
            let matched_choice_variant = Some(parsed.variant);
            let pattern_location = parsed.location.clone();
            let (arm_scope, pattern) = build_arm_scope_with_choice_captures(
                match_context,
                parsed,
                type_environment,
                string_table,
            )?;
            (pattern, matched_choice_variant, pattern_location, arm_scope)
        }
    } else {
        let option_inner_type_id = type_environment.option_inner_type(scrutinee.type_id);
        if let Some(inner_type_id) = option_inner_type_id {
            // Optional scrutinees must use option-specific patterns.
            // Bare capture symbols are rejected because `|name|` is the only
            // valid capture form for optional values.
            if let TokenKind::Symbol(_) = token_stream.current_token_kind()
                && !option_pattern_constructor_like(token_stream)
            {
                return Err(CompilerDiagnostic::invalid_match_pattern(
                    InvalidMatchPatternReason::BareCaptureOnOptionalScrutinee,
                    None,
                    None,
                    token_stream.current_location(),
                ));
            }

            let pattern =
                parse_option_pattern(token_stream, inner_type_id, string_table, type_environment)?;
            let location = pattern.location().to_owned();

            let (arm_scope, pattern) = if let MatchPattern::OptionPresentCapture {
                name,
                binding_location,
                inner_type_id: capture_inner_type_id,
                location: pattern_location,
                ..
            } = &pattern
            {
                build_arm_scope_with_option_present_capture(
                    match_context,
                    *name,
                    binding_location,
                    *capture_inner_type_id,
                    pattern_location,
                    type_interner,
                    string_table,
                )?
            } else {
                (match_context.clone(), pattern)
            };

            (pattern, None, location, arm_scope)
        } else if let TokenKind::Symbol(name) = token_stream.current_token_kind()
            && !option_pattern_constructor_like(token_stream)
        {
            let name = *name;
            let location = token_stream.current_location();
            token_stream.advance();
            let (arm_scope, pattern) = build_arm_scope_with_capture(
                match_context,
                name,
                &location,
                scrutinee.type_id,
                type_interner,
                string_table,
            )?;
            (pattern, None, location.clone(), arm_scope)
        } else {
            let pattern = parse_non_choice_pattern(
                token_stream,
                scrutinee.type_id,
                string_table,
                type_environment,
            )?;
            let location = pattern.location().to_owned();
            (pattern, None, location, match_context.clone())
        }
    };

    Ok(ParsedMatchPatternHeader {
        pattern,
        matched_choice_variant,
        pattern_location,
        arm_scope,
    })
}

#[allow(clippy::result_large_err)]
fn reject_invalid_pattern_suffix(token_stream: &FileTokens) -> Result<(), CompilerDiagnostic> {
    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        return Err(deferred_feature_reason_diagnostic(
            DeferredFeatureReason::CaptureTaggedPattern,
            token_stream.current_location(),
        ));
    }

    if token_stream.current_token_kind() == &TokenKind::As {
        return Err(CompilerDiagnostic::invalid_match_pattern(
            InvalidMatchPatternReason::AsNotValid,
            None,
            None,
            token_stream.current_location(),
        ));
    }

    if token_stream.current_token_kind() == &TokenKind::Colon {
        return Err(CompilerDiagnostic::invalid_match_arm(
            InvalidMatchArmReason::LegacyColonSyntax,
            token_stream.current_location(),
        ));
    }

    if token_stream.current_token_kind() == &TokenKind::Arrow {
        return Err(CompilerDiagnostic::invalid_match_arm(
            InvalidMatchArmReason::InvalidArrow,
            token_stream.current_location(),
        ));
    }

    Ok(())
}

fn new_match_arm_body_context(
    match_context: &ScopeContext,
    string_table: &mut StringTable,
) -> ScopeContext {
    let active_value_target = match_context.active_value_target.clone();
    let mut arm_context = match_context.new_child_control_flow(ContextKind::MatchArm, string_table);
    arm_context.active_value_target = active_value_target;
    arm_context
}

/// Returns true when a symbol-shaped option pattern looks constructor-like.
///
/// WHY: bare capture symbols are rejected for optional scrutinees, but constructor-like
/// syntax should continue into the pattern parser so it receives the more specific
/// unsupported-pattern diagnostic instead of being mistaken for a capture.
fn option_pattern_constructor_like(token_stream: &FileTokens) -> bool {
    token_stream
        .tokens
        .get(token_stream.index + 1)
        .is_some_and(|token| {
            matches!(
                token.kind,
                TokenKind::OpenParenthesis | TokenKind::DoubleColon
            )
        })
}

/// Build a choice arm scope and final pattern with fully resolved capture binding paths.
///
/// WHAT: clones the parent match context and adds `Declaration` entries for each parsed capture.
/// WHY: captures must be visible in both the guard and the body, but must not leak to other arms.
///
/// Validates:
/// - No capture name shadows an existing visible local (Beanstalk no-shadowing rule).
#[allow(clippy::result_large_err)]
fn build_arm_scope_with_choice_captures(
    match_context: &ScopeContext,
    parsed_pattern: ParsedChoicePattern,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<(ScopeContext, MatchPattern), CompilerDiagnostic> {
    let mut arm_scope = match_context.clone();
    let mut captures = Vec::with_capacity(parsed_pattern.captures.len());

    for capture in parsed_pattern.captures {
        let binding_name = capture.binding_name;

        // Enforce no-shadowing: the local binding name must not collide with any visible local.
        if let Some(_existing) = arm_scope.get_reference(&binding_name) {
            return Err(CompilerDiagnostic::invalid_match_pattern(
                InvalidMatchPatternReason::CaptureBindingShadowsVariable,
                None,
                None,
                capture.binding_location.clone(),
            ));
        }

        let binding_name_str = string_table.resolve(binding_name).to_owned();
        let binding_path = arm_scope.scope.join_str(&binding_name_str, string_table);

        let declaration = Declaration {
            id: binding_path.clone(),
            value: Expression::new(
                ExpressionKind::NoValue,
                capture.binding_location.clone(),
                capture.type_id,
                diagnostic_type_spelling(capture.type_id, type_environment),
                ValueMode::ImmutableOwned,
            ),
        };

        arm_scope.add_var(declaration);
        captures.push(ChoicePayloadCapture {
            field_name: capture.field_name,
            binding_name: capture.binding_name,
            field_index: capture.field_index,
            type_id: capture.type_id,
            binding_path,
            location: capture.location,
            binding_location: capture.binding_location,
        });
    }

    Ok((
        arm_scope,
        MatchPattern::ChoiceVariant {
            nominal_path: parsed_pattern.nominal_path,
            variant: parsed_pattern.variant,
            tag: parsed_pattern.tag,
            captures,
            location: parsed_pattern.location,
        },
    ))
}

/// Build a capture arm scope and pattern with a resolved binding path.
///
/// WHAT: clones the parent match context and adds a `Declaration` entry for the
/// general capture binding so the arm guard and body can reference the scrutinee value.
/// WHY: capture patterns must be scoped to a single arm and participate in normal
/// no-shadowing rules.
///
/// Validates:
/// - No capture name shadows an existing visible local (Beanstalk no-shadowing rule).
#[allow(clippy::result_large_err)]
fn build_arm_scope_with_capture(
    match_context: &ScopeContext,
    capture_name: StringId,
    capture_location: &SourceLocation,
    capture_type_id: TypeId,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<(ScopeContext, MatchPattern), CompilerDiagnostic> {
    let mut arm_scope = match_context.clone();

    if let Some(_existing) = arm_scope.get_reference(&capture_name) {
        return Err(CompilerDiagnostic::invalid_match_pattern(
            InvalidMatchPatternReason::CaptureBindingShadowsVariable,
            None,
            None,
            capture_location.clone(),
        ));
    }

    let binding_name_str = string_table.resolve(capture_name).to_owned();
    let binding_path = arm_scope.scope.join_str(&binding_name_str, string_table);

    let capture_data_type = diagnostic_type_spelling(capture_type_id, type_interner.environment());
    let declaration = Declaration {
        id: binding_path.clone(),
        value: Expression::new(
            ExpressionKind::NoValue,
            capture_location.clone(),
            capture_type_id,
            capture_data_type,
            ValueMode::ImmutableOwned,
        ),
    };

    arm_scope.add_var(declaration);

    let pattern = MatchPattern::Capture {
        name: capture_name,
        binding_path,
        location: capture_location.clone(),
    };

    Ok((arm_scope, pattern))
}

/// Build an option-present capture arm scope and pattern.
///
/// WHAT: clones the parent match context and adds a `Declaration` entry for the
/// inner payload binding so the arm guard and body can reference the unwrapped value.
/// WHY: option present captures share the same local-registration model as choice
/// payload captures, but the payload is always field index 0 of the `some` variant.
///
/// Validates:
/// - No capture name shadows an existing visible local (Beanstalk no-shadowing rule).
#[allow(clippy::result_large_err)]
fn build_arm_scope_with_option_present_capture(
    match_context: &ScopeContext,
    capture_name: StringId,
    binding_location: &SourceLocation,
    inner_type_id: TypeId,
    pattern_location: &SourceLocation,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<(ScopeContext, MatchPattern), CompilerDiagnostic> {
    let mut arm_scope = match_context.clone();

    if let Some(_existing) = arm_scope.get_reference(&capture_name) {
        return Err(CompilerDiagnostic::invalid_match_pattern(
            InvalidMatchPatternReason::CaptureBindingShadowsVariable,
            None,
            None,
            binding_location.clone(),
        ));
    }

    let binding_name_str = string_table.resolve(capture_name).to_owned();
    let binding_path = arm_scope.scope.join_str(&binding_name_str, string_table);

    let capture_data_type = diagnostic_type_spelling(inner_type_id, type_interner.environment());
    let declaration = Declaration {
        id: binding_path.clone(),
        value: Expression::new(
            ExpressionKind::NoValue,
            binding_location.clone(),
            inner_type_id,
            capture_data_type,
            ValueMode::ImmutableOwned,
        ),
    };

    arm_scope.add_var(declaration);

    let pattern = MatchPattern::OptionPresentCapture {
        name: capture_name,
        binding_path,
        inner_type_id,
        location: pattern_location.clone(),
        binding_location: binding_location.clone(),
    };

    Ok((arm_scope, pattern))
}

/// Tracks option-pattern exhaustiveness state to avoid threading multiple booleans.
struct OptionExhaustivenessState {
    seen_unguarded_none: bool,
    seen_unguarded_present_capture: bool,
}

/// Verify that a match statement covers all possible values.
///
/// WHAT: for choice scrutinees, checks that every declared variant has an arm or an
/// `else` fallback exists; for non-choice types, requires an explicit `else =>` arm.
/// WHY: exhaustiveness at parse time prevents silent fallthrough bugs and gives users
/// actionable diagnostics listing the specific missing variants.
#[allow(clippy::result_large_err)]
fn enforce_match_exhaustiveness(
    scrutinee: &Expression,
    else_block: &Option<Vec<AstNode>>,
    has_guarded_arms: bool,
    matched_choice_variants: &FxHashSet<StringId>,
    option_state: OptionExhaustivenessState,
    string_table: &StringTable,
    type_environment: &TypeEnvironment,
) -> Result<(), CompilerDiagnostic> {
    let is_choice = matches!(
        type_environment.type_kind(scrutinee.type_id),
        Some(TypeKind::Choice | TypeKind::GenericInstance)
    );

    if is_choice {
        let variants = choice_variants_for_type(scrutinee.type_id, type_environment);
        // `else` intentionally acts as an explicit "future variants" fallback in Alpha.
        if else_block.is_some() {
            return Ok(());
        }

        if has_guarded_arms {
            return Err(CompilerDiagnostic::non_exhaustive_match(
                NonExhaustiveMatchReason::GuardedArmsRequireElse,
                vec![],
                None,
                scrutinee.location.clone(),
            ));
        }

        let missing_variants: Vec<StringId> = variants
            .iter()
            .filter(|variant| !matched_choice_variants.contains(&variant.id))
            .map(|variant| variant.id)
            .collect();

        if missing_variants.is_empty() {
            return Ok(());
        }

        let missing_variant_names: Vec<String> = missing_variants
            .iter()
            .map(|&v| string_table.resolve(v).to_owned())
            .collect();
        return Err(CompilerDiagnostic::non_exhaustive_match(
            NonExhaustiveMatchReason::MissingVariants,
            missing_variants,
            Some(format!(
                "Non-exhaustive choice match. Missing variants: [{}].",
                missing_variant_names.join(", ")
            )),
            scrutinee.location.clone(),
        ));
    }

    if else_block.is_some() {
        return Ok(());
    }

    // For optional scrutinees, unguarded `none` + unguarded `|name|` covers all cases.
    let is_option = type_environment
        .option_inner_type(scrutinee.type_id)
        .is_some();
    if is_option && option_state.seen_unguarded_none && option_state.seen_unguarded_present_capture
    {
        return Ok(());
    }

    let reason = if is_option {
        NonExhaustiveMatchReason::MissingOptionPatterns
    } else {
        NonExhaustiveMatchReason::MissingElseArm
    };

    Err(CompilerDiagnostic::non_exhaustive_match(
        reason,
        vec![],
        None,
        scrutinee.location.clone(),
    ))
}

/// Fetch choice variants for a type, converting environment definitions into
/// AST-facing `ChoiceVariant` shapes.
///
/// WHAT: queries `TypeEnvironment` for variant metadata and maps payload
/// definitions into the local `ChoiceVariant` / `ChoiceVariantPayload` types.
/// WHY: match parsing needs AST-level variant info (names, field types, locations)
/// to validate choice arms, but it must not depend on the internal `TypeEnvironment`
/// representation beyond this boundary.
fn choice_variants_for_type(type_id: TypeId, env: &TypeEnvironment) -> Vec<ChoiceVariant> {
    env.variants_for(type_id)
        .map(|variants| {
            variants
                .iter()
                .map(|variant| ChoiceVariant {
                    id: variant.name,
                    payload: match &variant.payload {
                        ChoiceVariantPayloadDefinition::Unit => ChoiceVariantPayload::Unit,
                        ChoiceVariantPayloadDefinition::Record { fields } => {
                            ChoiceVariantPayload::Record {
                                fields: fields
                                    .iter()
                                    .map(|field| Declaration {
                                        id: field.name.clone(),
                                        value: Expression::new(
                                            ExpressionKind::NoValue,
                                            field.location.clone(),
                                            field.type_id,
                                            diagnostic_type_spelling(field.type_id, env),
                                            ValueMode::ImmutableOwned,
                                        ),
                                    })
                                    .collect(),
                            }
                        }
                    },
                    location: variant.location.clone(),
                })
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
#[path = "tests/branching_tests.rs"]
mod branching_tests;
