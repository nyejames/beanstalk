use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::{ast_log, return_rule_error, return_syntax_error};
use std::collections::HashSet;

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub condition: Expression,
    pub body: Vec<AstNode>,
}

struct ParsedCaseArm {
    arm: MatchArm,
    // Tracks which choice variant this arm consumes so duplicates can be rejected early.
    matched_choice_variant: Option<StringId>,
    pattern_location: SourceLocation,
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
        &Ownership::ImmutableOwned,
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

    if !is_boolean_expression(&then_condition) {
        let found_type = then_condition.data_type.display_with_table(string_table);
        return_rule_error!(
            format!("If condition must be a boolean expression. Found '{}'", found_type),
            token_stream.current_location(),
            {
                CompilationStage => "If Statement Parsing",
                PrimarySuggestion => "Use a boolean expression in the if condition (for example 'value is 0' or 'flag')",
                FoundType => found_type,
                ExpectedType => "Bool",
            }
        )
    }

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
    let then_block = function_body_to_ast(
        token_stream,
        then_context.to_owned(),
        warnings,
        string_table,
    )?;

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
        scope: then_context.scope,
    }])
}

fn is_boolean_expression(expression: &Expression) -> bool {
    match &expression.data_type {
        DataType::Bool => true,
        DataType::Reference(inner) => matches!(inner.as_ref(), DataType::Bool),
        _ => false,
    }
}

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
    // Choice exhaustiveness/duplication checks rely on the set of consumed variant names.
    let mut matched_choice_variants: HashSet<StringId> = HashSet::new();

    loop {
        token_stream.skip_newlines();

        match token_stream.current_token_kind() {
            TokenKind::End => {
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
        match_context.new_child_control_flow(ContextKind::Branch, string_table),
        warnings,
        string_table,
    )
}

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
    let (condition, matched_choice_variant, pattern_location) = match normalized_subject_type {
        DataType::Choices(variants) => {
            let (choice_pattern, matched_variant_name, location) =
                parse_choice_variant_pattern(token_stream, variants, string_table)?;
            (choice_pattern, Some(matched_variant_name), location)
        }
        subject_type => {
            let literal_pattern = parse_literal_pattern(token_stream, subject_type, string_table)?;
            let location = literal_pattern.location.to_owned();
            (literal_pattern, None, location)
        }
    };

    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        return_rule_error!(
            "Capture/tagged patterns using '|...|' are deferred for Alpha.",
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use simple literal or choice-variant patterns only",
            }
        );
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
        match_context.new_child_control_flow(ContextKind::Branch, string_table),
        warnings,
        string_table,
    )?;

    Ok(ParsedCaseArm {
        arm: MatchArm { condition, body },
        matched_choice_variant,
        pattern_location,
    })
}

fn parse_choice_variant_pattern(
    token_stream: &mut FileTokens,
    variants: &[Declaration],
    string_table: &StringTable,
) -> Result<(Expression, StringId, SourceLocation), CompilerError> {
    // Alpha only supports exact choice-variant names in match patterns.
    reject_deferred_pattern_lead_token(token_stream)?;

    let choice_name = choice_type_name_id(variants);
    let choice_name_display = choice_name
        .map(|id| string_table.resolve(id))
        .unwrap_or("<choice>");

    let leading_token = token_stream.current_token_kind().to_owned();
    let (variant_name, variant_location) = match leading_token {
        TokenKind::Symbol(first_name) => {
            let first_location = token_stream.current_location();
            token_stream.advance();

            if token_stream.current_token_kind() == &TokenKind::DoubleColon {
                if let Some(expected_choice_name) = choice_name
                    && first_name != expected_choice_name
                {
                    return_rule_error!(
                        format!(
                            "Match arm qualifier '{}::' does not match the scrutinee choice '{}'.",
                            string_table.resolve(first_name),
                            choice_name_display
                        ),
                        first_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Use the scrutinee choice name for qualified patterns, or use a bare variant name",
                        }
                    );
                }

                token_stream.advance();
                token_stream.skip_newlines();

                match token_stream.current_token_kind().to_owned() {
                    TokenKind::Symbol(qualified_variant_name) => {
                        let qualified_location = token_stream.current_location();
                        token_stream.advance();
                        (qualified_variant_name, qualified_location)
                    }
                    _ => {
                        return_rule_error!(
                            format!(
                                "Expected a variant name after '{}::' in this case pattern.",
                                choice_name_display
                            ),
                            token_stream.current_location(),
                            {
                                CompilationStage => "Match Statement Parsing",
                                PrimarySuggestion => "Use 'case Choice::Variant => ...' with a declared variant name",
                            }
                        );
                    }
                }
            } else {
                (first_name, first_location)
            }
        }

        TokenKind::IntLiteral(_)
        | TokenKind::FloatLiteral(_)
        | TokenKind::BoolLiteral(_)
        | TokenKind::CharLiteral(_)
        | TokenKind::StringSliceLiteral(_)
        | TokenKind::Negative => {
            return_rule_error!(
                "Choice match arms must use variant names, not raw literals.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use a choice variant pattern such as 'case Ready =>' or 'case Choice::Ready =>'",
                }
            );
        }

        _ => {
            return_rule_error!(
                "Choice match arms must start with a declared variant name.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use 'case Variant =>' or 'case Choice::Variant =>' for choice scrutinees",
                }
            );
        }
    };

    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        return_rule_error!(
            "Capture/tagged patterns using '|...|' are deferred for Alpha.",
            token_stream.current_location(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use simple choice variant patterns only in this phase",
            }
        );
    }

    // Match lowering compares tag indices today, so we normalize variant names to their index.
    let Some(variant_index) = variants
        .iter()
        .position(|variant| variant.id.name() == Some(variant_name))
    else {
        let available_variants = variants
            .iter()
            .filter_map(|variant| variant.id.name())
            .map(|name| string_table.resolve(name).to_owned())
            .collect::<Vec<_>>()
            .join(", ");

        return_rule_error!(
            format!(
                "Unknown variant '{}' for choice '{}'. Available variants: [{}].",
                string_table.resolve(variant_name),
                choice_name_display,
                available_variants
            ),
            variant_location,
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use one of the declared variants for this choice",
            }
        );
    };

    Ok((
        Expression::int(variant_index as i64, variant_location.clone(), Ownership::ImmutableOwned),
        variant_name,
        variant_location,
    ))
}

fn parse_literal_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<Expression, CompilerError> {
    reject_deferred_pattern_lead_token(token_stream)?;

    let pattern = match token_stream.current_token_kind() {
        TokenKind::IntLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::int(*value, location, Ownership::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::FloatLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::float(*value, location, Ownership::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::BoolLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::bool(*value, location, Ownership::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::CharLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::char(*value, location, Ownership::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::StringSliceLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::string_slice(*value, location, Ownership::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::Negative => {
            let negative_location = token_stream.current_location();
            token_stream.advance();
            match token_stream.current_token_kind() {
                TokenKind::IntLiteral(value) => {
                    let expression = Expression::int(-(*value), negative_location, Ownership::ImmutableOwned);
                    token_stream.advance();
                    expression
                }
                TokenKind::FloatLiteral(value) => {
                    let expression =
                        Expression::float(-(*value), negative_location, Ownership::ImmutableOwned);
                    token_stream.advance();
                    expression
                }
                _ => {
                    return_rule_error!(
                        "Negative literal patterns must be numeric literals (for example '-1' or '-3.2').",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Use a numeric literal after '-' or switch to a supported literal pattern",
                        }
                    );
                }
            }
        }
        _ => {
            return_rule_error!(
                "Literal match patterns currently support only literal int/float/bool/char/string values.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use a literal value pattern (for example 'case 1 =>', 'case true =>', or 'case \"ok\" =>')",
                }
            );
        }
    };

    if !subject_type.accepts_value_type(&pattern.data_type) {
        return_rule_error!(
            format!(
                "Match arm literal type '{}' does not match scrutinee type '{}'.",
                pattern.data_type.display_with_table(string_table),
                subject_type.display_with_table(string_table),
            ),
            pattern.location.clone(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use literal patterns that match the scrutinee type",
                ExpectedType => subject_type.display_with_table(string_table),
                FoundType => pattern.data_type.display_with_table(string_table),
            }
        );
    }

    Ok(pattern)
}

fn reject_deferred_pattern_lead_token(token_stream: &FileTokens) -> Result<(), CompilerError> {
    // These forms intentionally fail fast so unsupported syntax never drifts silently.
    match token_stream.current_token_kind() {
        TokenKind::Wildcard => {
            return_rule_error!(
                "Wildcard patterns ('case _ =>') are deferred for Alpha. Use 'else =>' for the default arm.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Replace wildcard arms with an explicit 'else =>' arm",
                }
            );
        }
        TokenKind::LessThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterThanOrEqual => {
            return_rule_error!(
                "Relational match patterns (for example '<', '<=', '>', '>=') are deferred for Alpha.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use literal/choice-variant patterns for now and move relational checks into arm bodies",
                }
            );
        }
        TokenKind::Not => {
            return_rule_error!(
                "Negated match patterns (for example 'case not ... =>') are deferred for Alpha.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use explicit positive case arms and an 'else =>' fallback in this phase",
                }
            );
        }
        TokenKind::TypeParameterBracket => {
            return_rule_error!(
                "Capture/tagged patterns using '|...|' are deferred for Alpha.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use simple literal or choice-variant patterns only",
                }
            );
        }
        _ => {}
    }

    Ok(())
}

fn normalized_subject_type(data_type: &DataType) -> &DataType {
    // Pattern checks run against the value type, not the borrow wrapper.
    match data_type {
        DataType::Reference(inner) => inner.as_ref(),
        _ => data_type,
    }
}

fn choice_type_name_id(variants: &[Declaration]) -> Option<StringId> {
    let mut names = variants
        .iter()
        .filter_map(|variant| variant.id.parent().and_then(|parent| parent.name()));

    let first = names.next()?;
    if names.all(|name| name == first) {
        Some(first)
    } else {
        None
    }
}

fn enforce_match_exhaustiveness(
    subject: &Expression,
    else_block: &Option<Vec<AstNode>>,
    matched_choice_variants: &HashSet<StringId>,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let normalized_subject_type = normalized_subject_type(&subject.data_type);

    match normalized_subject_type {
        DataType::Choices(variants) => {
            // `else` intentionally acts as an explicit "future variants" fallback in Alpha.
            if else_block.is_some() {
                return Ok(());
            }

            let missing_variants = variants
                .iter()
                .filter_map(|variant| variant.id.name())
                .filter(|variant_name| !matched_choice_variants.contains(variant_name))
                .map(|variant_name| string_table.resolve(variant_name).to_owned())
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
