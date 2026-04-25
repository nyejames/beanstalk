//! Match and if/else branching AST construction.
//!
//! WHAT: parses `if`/`else` conditionals and `if value is:` match statements
//! into AST branch and match nodes.
//! WHY: match parsing centralizes exhaustiveness checking, deferred-feature
//! rejection, and choice-variant resolution at the AST level so HIR lowering
//! receives validated, normalized match structures.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::{
    create_expression, create_expression_until,
};
use crate::compiler_frontend::ast::function_body_to_ast;
use crate::compiler_frontend::ast::statements::condition_validation::{
    ensure_if_statement_condition, ensure_match_guard_condition,
};
use crate::compiler_frontend::ast::statements::match_patterns::{
    MatchArm, normalized_subject_type, parse_choice_variant_pattern, parse_non_choice_pattern,
};
use crate::compiler_frontend::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use crate::{ast_log, return_rule_error, return_syntax_error};
use rustc_hash::FxHashSet;

struct ParsedCaseArm {
    arm: MatchArm,
    /// Tracks which choice variant this arm consumes so duplicates can be rejected early.
    matched_choice_variant: Option<StringId>,
    pattern_location: SourceLocation,
}

fn peek_next_non_newline_token(token_stream: &FileTokens) -> Option<&Token> {
    token_stream
        .tokens
        .iter()
        .skip(token_stream.index + 1)
        .find(|token| token.kind != TokenKind::Newline)
}

pub fn create_branch(
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    let mut condition_type = DataType::Inferred;
    let then_condition = create_expression(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition, string_table),
        &mut condition_type,
        &ValueMode::ImmutableOwned,
        false,
        string_table,
    )?;

    // `if value is:` starts a statement-style match arm block.
    if token_stream.current_token_kind() == &TokenKind::Is {
        token_stream.advance();
        let match_statement = create_match_node(
            then_condition,
            token_stream,
            context,
            warnings,
            string_table,
        )?;
        return Ok(vec![match_statement]);
    }

    ensure_if_statement_condition(&then_condition, string_table)?;

    ast_log!("Creating If Statement");
    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_rule_error!(
            format!(
                "Expected ':' after the if condition to open a new scope, found '{:?}' instead",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "If Statement Parsing",
                PrimarySuggestion => "Add ':' after the if condition to open the if body",
                SuggestedInsertion => ":",
            }
        )
    }

    token_stream.advance();
    let then_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    let then_scope = then_context.scope.clone();
    let then_block = function_body_to_ast(token_stream, then_context, warnings, string_table)?;

    let else_block = if token_stream.current_token_kind() == &TokenKind::Else {
        token_stream.advance();
        let else_context = context.new_child_control_flow(ContextKind::Branch, string_table);
        Some(function_body_to_ast(
            token_stream,
            else_context,
            warnings,
            string_table,
        )?)
    } else {
        None
    };

    Ok(vec![AstNode {
        kind: NodeKind::If(then_condition, then_block, else_block),
        location: token_stream.current_location(),
        scope: then_scope,
    }])
}

/// Parse a complete `if <subject> is:` match statement into a `NodeKind::Match`.
///
/// WHAT: loops through `case`/`else` arms, validates ordering and uniqueness, then
/// delegates exhaustiveness checking before returning the match node.
/// WHY: all match-level invariants (at least one case before else, no duplicates,
/// exhaustiveness) are enforced here so downstream HIR lowering can assume valid input.
fn create_match_node(
    subject: Expression,
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    ast_log!("Creating Match Statement");

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_rule_error!(
            format!(
                "Expected ':' after the if condition to open a new scope, found '{:?}' instead",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Add ':' after 'is' to open the match body",
                SuggestedInsertion => ":",
            }
        )
    }

    token_stream.advance();
    let match_context = context.new_child_control_flow(ContextKind::Branch, string_table);

    let mut arms: Vec<MatchArm> = Vec::new();
    let mut else_block = None;
    let mut seen_else = false;
    let mut has_guarded_arms = false;
    let mut match_arm_indent: Option<i32> = None;
    // Choice exhaustiveness/duplication checks rely on the set of consumed variant names.
    let mut matched_choice_variants: FxHashSet<StringId> = FxHashSet::default();

    loop {
        token_stream.skip_newlines();

        match token_stream.current_token_kind() {
            TokenKind::End => {
                let next_token = peek_next_non_newline_token(token_stream);
                let semicolon_separates_same_level_arms = match (match_arm_indent, next_token) {
                    (Some(arm_indent), Some(next))
                        if matches!(next.kind, TokenKind::Case | TokenKind::Else) =>
                    {
                        next.location.start_pos.char_column == arm_indent
                    }
                    _ => false,
                };

                if semicolon_separates_same_level_arms {
                    return_syntax_error!(
                        "Match arms are not closed with semicolons. Use the next 'case', 'else', or the final match ';' to delimit arms.",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Remove the ';' between match arms and keep only the final ';' that closes the full match block",
                        }
                    );
                }
                token_stream.advance();
                break;
            }

            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file in match statement",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => "Terminate this match statement with ';'",
                        SuggestedInsertion => ";",
                    }
                )
            }

            TokenKind::Else => {
                match_arm_indent
                    .get_or_insert(token_stream.current_location().start_pos.char_column);

                if arms.is_empty() {
                    return_rule_error!(
                        "Match statements require at least one 'case' arm before 'else =>'",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Add one or more 'case <pattern> =>' arms before the default arm",
                        }
                    )
                }

                if seen_else {
                    return_rule_error!(
                        "Match statement can only have one 'else =>' arm",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Remove duplicate else arms",
                        }
                    )
                }
                seen_else = true;

                else_block = Some(parse_else_arm(
                    token_stream,
                    &match_context,
                    warnings,
                    string_table,
                )?);
            }

            TokenKind::Case => {
                match_arm_indent
                    .get_or_insert(token_stream.current_location().start_pos.char_column);

                if seen_else {
                    return_rule_error!(
                        "Match arms cannot appear after an 'else =>' arm",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Move this arm before the else arm",
                        }
                    )
                }

                let parsed_case = parse_case_arm(
                    &subject,
                    token_stream,
                    &match_context,
                    warnings,
                    string_table,
                )?;

                if let Some(variant_name) = parsed_case.matched_choice_variant
                    && !matched_choice_variants.insert(variant_name)
                {
                    return_rule_error!(
                        format!(
                            "Duplicate match arm for choice variant '{}'. Each variant can only be matched once.",
                            string_table.resolve(variant_name)
                        ),
                        parsed_case.pattern_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Remove duplicate variant arms or merge their logic into one arm",
                        }
                    );
                }

                has_guarded_arms |= parsed_case.arm.guard.is_some();
                arms.push(parsed_case.arm);
            }

            // Old syntax migration path: `<pattern>:` is now `case <pattern> =>`.
            _ => {
                return_syntax_error!(
                    "Legacy match arm syntax is no longer supported. Match arms must start with 'case' and use '=>'.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => "Rewrite this arm as 'case <pattern> => <body>'",
                    }
                )
            }
        }
    }

    enforce_match_exhaustiveness(
        &subject,
        &else_block,
        has_guarded_arms,
        &matched_choice_variants,
        string_table,
    )?;

    Ok(AstNode {
        kind: NodeKind::Match(subject, arms, else_block),
        location: token_stream.current_location(),
        scope: match_context.scope,
    })
}

fn parse_else_arm(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    token_stream.advance();
    token_stream.skip_newlines();

    if token_stream.current_token_kind() == &TokenKind::Colon {
        return_syntax_error!(
            "Legacy default-arm syntax 'else:' is no longer supported. Use 'else =>'.",
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Replace 'else:' with 'else =>'",
                SuggestedReplacement => "=>",
            }
        );
    }

    if token_stream.current_token_kind() == &TokenKind::Arrow {
        return_syntax_error!(
            "Unexpected '->' after 'else'. Match default arms use '=>'.",
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Replace '->' with '=>'",
                SuggestedReplacement => "=>",
            }
        );
    }

    if token_stream.current_token_kind() != &TokenKind::FatArrow {
        return_rule_error!(
            format!(
                "Expected '=>' after 'else' in a match statement, found '{:?}'.",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use 'else => <body>' for the default match arm",
                SuggestedInsertion => "=>",
            }
        )
    }

    token_stream.advance();
    function_body_to_ast(
        token_stream,
        match_context.new_child_control_flow(ContextKind::MatchArm, string_table),
        warnings,
        string_table,
    )
}

/// Parse an optional `if <condition>` guard before the `=>` separator.
///
/// WHY: guard parsing is self-contained (token check, expression parse, validation)
/// and extracting it removes ~15 lines from `parse_case_arm`.
fn parse_match_guard(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Option<Expression>, CompilerError> {
    if token_stream.current_token_kind() != &TokenKind::If {
        return Ok(None);
    }

    token_stream.advance();
    token_stream.skip_newlines();

    let mut guard_type = DataType::Inferred;
    let guard_expression = create_expression_until(
        token_stream,
        &match_context.new_child_control_flow(ContextKind::Condition, string_table),
        &mut guard_type,
        &ValueMode::ImmutableOwned,
        &[TokenKind::FatArrow],
        string_table,
    )?;
    ensure_match_guard_condition(&guard_expression, string_table)?;

    Ok(Some(guard_expression))
}

/// Parse a single `case <pattern> => <body>` arm.
///
/// WHAT: dispatches to choice-variant or literal pattern parsing based on the
/// scrutinee type, validates the `=>` separator, and parses the arm body.
/// WHY: separating choice and literal paths here keeps each pattern parser focused
/// on one concern while this function handles shared arm-level syntax validation.
fn parse_case_arm(
    subject: &Expression,
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<ParsedCaseArm, CompilerError> {
    token_stream.advance();
    token_stream.skip_newlines();

    let normalized_subject_type = normalized_subject_type(&subject.data_type);

    // Choice scrutinees resolve symbols to variants; all other scrutinees stay literal-only.
    let (pattern, matched_choice_variant, pattern_location) = match normalized_subject_type {
        DataType::Choices {
            nominal_path,
            variants,
        } => {
            let parsed =
                parse_choice_variant_pattern(token_stream, nominal_path, variants, string_table)?;
            (parsed.pattern, Some(parsed.variant), parsed.location)
        }
        subject_type => {
            let pattern = parse_non_choice_pattern(token_stream, subject_type, string_table)?;
            let location = pattern.location().to_owned();
            (pattern, None, location)
        }
    };

    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        return Err(deferred_feature_rule_error(
            "Capture/tagged patterns using '|...|' are deferred for Alpha.",
            token_stream.current_location(),
            "Match Statement Parsing",
            "Use simple literal or choice-variant patterns only.",
        ));
    }

    if token_stream.current_token_kind() == &TokenKind::Colon {
        return_syntax_error!(
            "Legacy match arm syntax '<pattern>:' is no longer supported. Use 'case <pattern> =>'.",
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Replace ':' with '=>' after the case pattern",
                SuggestedReplacement => "=>",
            }
        );
    }

    if token_stream.current_token_kind() == &TokenKind::Arrow {
        return_syntax_error!(
            "Unexpected '->' in match arm. Match arms use '=>'.",
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Replace '->' with '=>'",
                SuggestedReplacement => "=>",
            }
        );
    }

    let guard = parse_match_guard(token_stream, match_context, string_table)?;

    if token_stream.current_token_kind() != &TokenKind::FatArrow {
        return_rule_error!(
            format!(
                "Expected '=>' after the match arm pattern, found '{:?}'.",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use 'case <pattern> => <body>' for match arms",
                SuggestedInsertion => "=>",
            }
        )
    }

    token_stream.advance();
    let body = function_body_to_ast(
        token_stream,
        match_context.new_child_control_flow(ContextKind::MatchArm, string_table),
        warnings,
        string_table,
    )?;

    Ok(ParsedCaseArm {
        arm: MatchArm {
            pattern,
            guard,
            body,
        },
        matched_choice_variant,
        pattern_location,
    })
}

/// Verify that a match statement covers all possible values.
///
/// WHAT: for choice scrutinees, checks that every declared variant has an arm or an
/// `else` fallback exists; for non-choice types, requires an explicit `else =>` arm.
/// WHY: exhaustiveness at parse time prevents silent fallthrough bugs and gives users
/// actionable diagnostics listing the specific missing variants.
fn enforce_match_exhaustiveness(
    subject: &Expression,
    else_block: &Option<Vec<AstNode>>,
    has_guarded_arms: bool,
    matched_choice_variants: &FxHashSet<StringId>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let normalized_subject_type = normalized_subject_type(&subject.data_type);

    match normalized_subject_type {
        DataType::Choices { variants, .. } => {
            // `else` intentionally acts as an explicit "future variants" fallback in Alpha.
            if else_block.is_some() {
                return Ok(());
            }

            if has_guarded_arms {
                return_rule_error!(
                    "Choice matches with guarded arms must include an explicit 'else =>' arm in Alpha.",
                    subject.location.clone(),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => "Add an 'else =>' arm when any choice match arm uses a guard",
                    }
                );
            }

            let missing_variants = variants
                .iter()
                .filter(|variant| !matched_choice_variants.contains(&variant.id))
                .map(|variant| string_table.resolve(variant.id).to_owned())
                .collect::<Vec<_>>();

            if missing_variants.is_empty() {
                return Ok(());
            }

            return_rule_error!(
                format!(
                    "Non-exhaustive choice match. Missing variants: [{}].",
                    missing_variants.join(", ")
                ),
                subject.location.clone(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Add match arms for each missing variant, or add an 'else =>' arm",
                }
            );
        }

        non_choice_type => {
            if else_block.is_some() {
                return Ok(());
            }

            return_rule_error!(
                format!(
                    "Non-choice matches must include an 'else =>' arm in Alpha. Scrutinee type: '{}'.",
                    non_choice_type.display_with_table(string_table)
                ),
                subject.location.clone(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Add an 'else =>' arm to make this match exhaustive",
                }
            );
        }
    }
}

#[cfg(test)]
#[path = "tests/branching_tests.rs"]
mod branching_tests;
