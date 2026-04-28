//! Match-pattern parsing and validation.
//!
//! WHAT: parses literal, relational, and choice-variant case patterns.
//! WHY: pattern syntax and type validation evolve separately from match arm/body parsing.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_type_compatible;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::return_rule_error;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub pattern: MatchPattern,
    pub guard: Option<Expression>,
    pub body: Vec<AstNode>,
}

/// One payload field capture inside a choice-variant match pattern.
///
/// WHY: match arms can destructure payload variants by binding each field to a
/// local name. For Alpha, captured names must exactly match the declared field
/// names in declaration order.
#[derive(Debug, Clone)]
pub(super) struct ParsedChoicePayloadCapture {
    pub field_name: StringId,
    pub field_index: usize,
    pub field_type: DataType,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub struct ChoicePayloadCapture {
    pub field_name: StringId,
    pub field_index: usize,
    pub field_type: DataType,
    pub binding_path: InternedPath,
    pub location: SourceLocation,
}

#[derive(Debug, Clone)]
pub enum MatchPattern {
    Literal(Expression),

    Wildcard {
        location: SourceLocation,
    },

    Relational {
        op: RelationalPatternOp,
        value: Expression,
        location: SourceLocation,
    },

    ChoiceVariant {
        nominal_path: InternedPath,
        variant: StringId,
        tag: usize,
        captures: Vec<ChoicePayloadCapture>,
        location: SourceLocation,
    },
}

impl MatchPattern {
    pub fn location(&self) -> &SourceLocation {
        match self {
            MatchPattern::Literal(expression) => &expression.location,
            MatchPattern::Wildcard { location }
            | MatchPattern::Relational { location, .. }
            | MatchPattern::ChoiceVariant { location, .. } => location,
        }
    }

    /// Return the capture list if this is a choice-variant pattern.
    pub fn choice_captures(&self) -> Option<&[ChoicePayloadCapture]> {
        match self {
            MatchPattern::ChoiceVariant { captures, .. } => Some(captures),
            _ => None,
        }
    }
}

/// Result of parsing a choice-variant pattern in a match arm.
pub(super) struct ParsedChoicePattern {
    pub(super) nominal_path: InternedPath,
    pub(super) variant: StringId,
    pub(super) tag: usize,
    pub(super) captures: Vec<ParsedChoicePayloadCapture>,
    pub(super) location: SourceLocation,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RelationalPatternOp {
    LessThan,
    LessThanOrEqual,
    GreaterThan,
    GreaterThanOrEqual,
}

/// Parse a non-choice match pattern, dispatching to relational or literal parsers.
pub(super) fn parse_non_choice_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<MatchPattern, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::LessThan
        | TokenKind::LessThanOrEqual
        | TokenKind::GreaterThan
        | TokenKind::GreaterThanOrEqual => {
            parse_relational_pattern(token_stream, subject_type, string_table)
        }

        _ => {
            let literal = parse_literal_pattern(token_stream, subject_type, string_table)?;
            Ok(MatchPattern::Literal(literal))
        }
    }
}

fn parse_relational_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<MatchPattern, CompilerError> {
    let location = token_stream.current_location();

    let op = match token_stream.current_token_kind() {
        TokenKind::LessThan => RelationalPatternOp::LessThan,
        TokenKind::LessThanOrEqual => RelationalPatternOp::LessThanOrEqual,
        TokenKind::GreaterThan => RelationalPatternOp::GreaterThan,
        TokenKind::GreaterThanOrEqual => RelationalPatternOp::GreaterThanOrEqual,
        _ => unreachable!("caller checked relational lead token"),
    };

    token_stream.advance();
    token_stream.skip_newlines();

    let value = parse_literal_pattern(token_stream, subject_type, string_table)?;

    ensure_relational_pattern_type(subject_type, &value, &location, string_table)?;

    Ok(MatchPattern::Relational {
        op,
        value,
        location,
    })
}

fn ensure_relational_pattern_type(
    subject_type: &DataType,
    value: &Expression,
    location: &SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    let is_ordered_scalar = matches!(
        subject_type,
        DataType::Int | DataType::Float | DataType::Char
    );

    if !is_ordered_scalar {
        return_rule_error!(
            format!(
                "Relational match patterns are only supported for ordered scalar types (Int, Float, Char), not '{}'.",
                subject_type.display_with_table(string_table)
            ),
            location.clone(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use literal patterns or an 'else =>' arm for this scrutinee type",
            }
        );
    }

    if !is_type_compatible(subject_type, &value.data_type) {
        return_rule_error!(
            format!(
                "Relational match pattern value type '{}' does not match scrutinee type '{}'.",
                value.data_type.display_with_table(string_table),
                subject_type.display_with_table(string_table),
            ),
            value.location.clone(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use a literal value that matches the scrutinee type",
                ExpectedType => subject_type.display_with_table(string_table),
                FoundType => value.data_type.display_with_table(string_table),
            }
        );
    }

    Ok(())
}

/// Resolve a choice variant pattern to its deterministic tag index.
///
/// WHAT: accepts bare (`Ready`) or qualified (`Status::Ready`) variant names and
/// normalizes them to the variant's positional index expression.
/// WHY: match lowering compares integer tag indices, so normalizing here lets HIR
/// treat choice arms identically to literal-int arms.
pub(super) fn parse_choice_variant_pattern(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    choice_nominal_path: &InternedPath,
    variants: &[ChoiceVariant],
    string_table: &StringTable,
) -> Result<ParsedChoicePattern, CompilerError> {
    // Alpha only supports exact choice-variant names in match patterns.
    reject_deferred_pattern_lead_token(token_stream)?;

    let choice_name_display = choice_display_name(choice_nominal_path, string_table);
    let (variant_name, variant_location) = parse_variant_name(
        token_stream,
        match_context,
        choice_nominal_path,
        &choice_name_display,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
        return Err(deferred_feature_rule_error(
            "Capture/tagged patterns using '|...|' are deferred for Alpha.",
            token_stream.current_location(),
            "Match Statement Parsing",
            "Use simple choice-variant patterns only in this phase.",
        ));
    }

    let variant_index = resolve_variant_to_tag(
        variants,
        variant_name,
        &choice_name_display,
        &variant_location,
        string_table,
    )?;

    let variant = &variants[variant_index];
    let captures =
        parse_choice_pattern_captures(token_stream, variant, &choice_name_display, string_table)?;

    Ok(ParsedChoicePattern {
        nominal_path: choice_nominal_path.to_owned(),
        variant: variant_name,
        tag: variant_index,
        captures,
        location: variant_location,
    })
}

/// Parse optional payload captures after a choice-variant name.
///
/// WHAT: handles `case Err(message)` and `case Success` forms, validating that
/// captures match the variant's payload metadata exactly.
/// WHY: separating capture parsing from name resolution keeps each function focused
/// and makes error messages specific to the payload layer.
fn parse_choice_pattern_captures(
    token_stream: &mut FileTokens,
    variant: &ChoiceVariant,
    _choice_name_display: &str,
    string_table: &StringTable,
) -> Result<Vec<ParsedChoicePayloadCapture>, CompilerError> {
    let variant_name_str = string_table.resolve(variant.id);

    match &variant.payload {
        ChoiceVariantPayload::Unit => {
            if token_stream.current_token_kind() == &TokenKind::OpenParenthesis {
                return_rule_error!(
                    format!(
                        "Unit variant '{}' cannot have payload captures. Use 'case {} =>' without parentheses.",
                        variant_name_str, variant_name_str
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => format!("Use 'case {} =>' for unit variants", variant_name_str),
                    }
                );
            }
            Ok(Vec::new())
        }

        ChoiceVariantPayload::Record { fields } => {
            if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
                let field_list: Vec<String> = fields
                    .iter()
                    .filter_map(|f| f.id.name().map(|n| string_table.resolve(n).to_owned()))
                    .collect();
                return_rule_error!(
                    format!(
                        "Payload variant '{}' requires capture bindings. Expected 'case {}({})'.",
                        variant_name_str,
                        variant_name_str,
                        field_list.join(", ")
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => format!(
                            "Use 'case {}({})' to bind payload fields",
                            variant_name_str,
                            field_list.join(", ")
                        ),
                    }
                );
            }

            token_stream.advance();

            let mut captures = Vec::new();
            let mut seen_names: FxHashMap<StringId, SourceLocation> = FxHashMap::default();

            loop {
                token_stream.skip_newlines();

                if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
                    token_stream.advance();
                    break;
                }

                let capture_location = token_stream.current_location();
                let capture_name = match token_stream.current_token_kind() {
                    TokenKind::Symbol(name) => *name,
                    _ => {
                        return_rule_error!(
                            "Capture binding must be a field name.",
                            capture_location,
                            {
                                CompilationStage => "Match Statement Parsing",
                                PrimarySuggestion => "Use the declared payload field name as the capture binding",
                            }
                        );
                    }
                };
                token_stream.advance();

                // Reject deferred rename syntax: `case Err(text as message)`
                if token_stream.current_token_kind() == &TokenKind::As {
                    return Err(deferred_feature_rule_error(
                        "Payload binding rename syntax is deferred. Use the declared field name directly.",
                        token_stream.current_location(),
                        "Match Statement Parsing",
                        format!(
                            "Use 'case {}({})' with the original field name.",
                            variant_name_str,
                            string_table.resolve(capture_name)
                        ),
                    ));
                }

                // Reject named assignment: `case Err(message = text)`
                if token_stream.current_token_kind() == &TokenKind::Assign {
                    return Err(deferred_feature_rule_error(
                        "Named payload pattern assignment is deferred. Use positional capture with the field name.",
                        token_stream.current_location(),
                        "Match Statement Parsing",
                        format!(
                            "Use 'case {}({})' with the original field name.",
                            variant_name_str,
                            string_table.resolve(capture_name)
                        ),
                    ));
                }

                // Check duplicate capture name
                if let Some(_first_loc) = seen_names.get(&capture_name) {
                    return_rule_error!(
                        format!(
                            "Duplicate capture binding '{}' in pattern for variant '{}'.",
                            string_table.resolve(capture_name),
                            variant_name_str
                        ),
                        capture_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Remove the duplicate capture binding",
                        }
                    );
                }
                seen_names.insert(capture_name, capture_location.clone());

                // Validate capture position and name against declaration metadata
                let field_index = captures.len();
                let Some(field_decl) = fields.get(field_index) else {
                    let expected_count = fields.len();
                    return_rule_error!(
                        format!(
                            "Too many capture bindings for variant '{}'. Expected {} field(s), found {}.",
                            variant_name_str,
                            expected_count,
                            field_index + 1
                        ),
                        capture_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => format!(
                                "Use exactly {} capture(s): {}",
                                expected_count,
                                fields.iter().filter_map(|f| f.id.name().map(|n| string_table.resolve(n).to_owned())).collect::<Vec<_>>().join(", ")
                            ),
                        }
                    );
                };

                let expected_field_name = field_decl
                    .id
                    .name()
                    .expect("choice payload field must have a name");
                if capture_name != expected_field_name {
                    return_rule_error!(
                        format!(
                            "Capture binding '{}' does not match payload field name '{}' at position {} in variant '{}'.",
                            string_table.resolve(capture_name),
                            string_table.resolve(expected_field_name),
                            field_index + 1,
                            variant_name_str
                        ),
                        capture_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => format!(
                                "Use '{}' as the capture name: 'case {}({})'",
                                string_table.resolve(expected_field_name),
                                variant_name_str,
                                fields.iter().filter_map(|f| f.id.name().map(|n| string_table.resolve(n).to_owned())).collect::<Vec<_>>().join(", ")
                            ),
                        }
                    );
                }

                captures.push(ParsedChoicePayloadCapture {
                    field_name: capture_name,
                    field_index,
                    field_type: field_decl.value.data_type.clone(),
                    location: capture_location,
                });

                token_stream.skip_newlines();
                match token_stream.current_token_kind() {
                    TokenKind::Comma => {
                        token_stream.advance();
                        continue;
                    }
                    TokenKind::CloseParenthesis => {
                        token_stream.advance();
                        break;
                    }
                    TokenKind::OpenParenthesis => {
                        return Err(deferred_feature_rule_error(
                            "Nested payload patterns are deferred. Use flat capture bindings with declared field names only.",
                            token_stream.current_location(),
                            "Match Statement Parsing",
                            "Use 'case Variant(field1, field2)' with original field names only",
                        ));
                    }
                    _ => {
                        return_rule_error!(
                            "Expected ',' or ')' after capture binding.",
                            token_stream.current_location(),
                            {
                                CompilationStage => "Match Statement Parsing",
                                PrimarySuggestion => "Separate captures with commas and close with ')'",
                            }
                        );
                    }
                }
            }

            // Check for too few captures
            if captures.len() != fields.len() {
                return_rule_error!(
                    format!(
                        "Too few capture bindings for variant '{}'. Expected {} field(s), found {}.",
                        variant_name_str,
                        fields.len(),
                        captures.len()
                    ),
                    variant.location.clone(),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => format!(
                            "Use exactly {} capture(s): {}",
                            fields.len(),
                            fields.iter().filter_map(|f| f.id.name().map(|n| string_table.resolve(n).to_owned())).collect::<Vec<_>>().join(", ")
                        ),
                    }
                );
            }

            Ok(captures)
        }
    }
}

/// Parse a bare (`Ready`) or qualified (`Status::Ready`) variant name from the token stream.
///
/// WHY: separating token-level parsing from tag resolution keeps each function focused
/// and makes error messages specific to the syntactic layer they diagnose.
fn parse_variant_name(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    choice_nominal_path: &InternedPath,
    choice_name_display: &str,
    string_table: &StringTable,
) -> Result<(StringId, SourceLocation), CompilerError> {
    let leading_token = token_stream.current_token_kind().to_owned();

    match leading_token {
        TokenKind::Symbol(first_name) => {
            let first_location = token_stream.current_location();
            token_stream.advance();

            if token_stream.current_token_kind() == &TokenKind::DoubleColon {
                if let Some(expected_choice_name) = choice_nominal_path.name()
                    && first_name != expected_choice_name
                    && !qualifier_resolves_to_choice(match_context, first_name, choice_nominal_path)
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
                        Ok((qualified_variant_name, qualified_location))
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
                Ok((first_name, first_location))
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
    }
}

fn qualifier_resolves_to_choice(
    match_context: &ScopeContext,
    qualifier: StringId,
    choice_nominal_path: &InternedPath,
) -> bool {
    match_context
        .get_reference(&qualifier)
        .is_some_and(|declaration| {
            matches!(
                &declaration.value.data_type,
                DataType::Choices { nominal_path, .. } if nominal_path == choice_nominal_path
            )
        })
}

/// Look up a variant name in the declared choice variant list and return its positional tag.
///
/// WHY: separating semantic resolution from token parsing produces clearer control flow
/// and keeps the error-construction logic for unknown variants in one place.
fn resolve_variant_to_tag(
    variants: &[ChoiceVariant],
    variant_name: StringId,
    choice_name_display: &str,
    variant_location: &SourceLocation,
    string_table: &StringTable,
) -> Result<usize, CompilerError> {
    let Some(variant_index) = variants
        .iter()
        .position(|variant| variant.id == variant_name)
    else {
        let available_variants = variants
            .iter()
            .map(|variant| string_table.resolve(variant.id).to_owned())
            .collect::<Vec<_>>()
            .join(", ");

        return_rule_error!(
            format!(
                "Unknown variant '{}' for choice '{}'. Available variants: [{}].",
                string_table.resolve(variant_name),
                choice_name_display,
                available_variants
            ),
            variant_location.clone(),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Use one of the declared variants for this choice",
            }
        );
    };

    Ok(variant_index)
}

fn choice_display_name(choice_nominal_path: &InternedPath, string_table: &StringTable) -> String {
    choice_nominal_path
        .name()
        .map(|name| string_table.resolve(name).to_owned())
        .unwrap_or_else(|| String::from("<choice>"))
}

/// Parse a literal value pattern and type-check it against the scrutinee.
///
/// WHAT: accepts int, float, bool, char, string, and negative numeric literals and
/// verifies the pattern type is compatible with the scrutinee type.
/// WHY: catching type mismatches at parse time produces better source-located errors
/// than deferring the check to HIR lowering.
fn parse_literal_pattern(
    token_stream: &mut FileTokens,
    subject_type: &DataType,
    string_table: &StringTable,
) -> Result<Expression, CompilerError> {
    reject_deferred_pattern_lead_token(token_stream)?;

    let pattern = match token_stream.current_token_kind() {
        TokenKind::IntLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::int(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::FloatLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::float(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::BoolLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::bool(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::CharLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::char(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::StringSliceLiteral(value) => {
            let location = token_stream.current_location();
            let expression = Expression::string_slice(*value, location, ValueMode::ImmutableOwned);
            token_stream.advance();
            expression
        }
        TokenKind::Negative => {
            let negative_location = token_stream.current_location();
            token_stream.advance();
            match token_stream.current_token_kind() {
                TokenKind::IntLiteral(value) => {
                    let expression =
                        Expression::int(-(*value), negative_location, ValueMode::ImmutableOwned);
                    token_stream.advance();
                    expression
                }
                TokenKind::FloatLiteral(value) => {
                    let expression =
                        Expression::float(-(*value), negative_location, ValueMode::ImmutableOwned);
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

    if !is_type_compatible(subject_type, &pattern.data_type) {
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

pub(super) fn reject_deferred_pattern_lead_token(
    token_stream: &FileTokens,
) -> Result<(), CompilerError> {
    // These forms intentionally fail fast so unsupported syntax never drifts silently.
    match token_stream.current_token_kind() {
        TokenKind::Wildcard => {
            return Err(deferred_feature_rule_error(
                "Wildcard patterns in 'case' arms are not supported. Use 'else =>' for a catch-all arm.",
                token_stream.current_location(),
                "Match Statement Parsing",
                "Replace 'case _ =>' with 'else =>'.",
            ));
        }
        TokenKind::Not => {
            return Err(deferred_feature_rule_error(
                "Negated match patterns (for example 'case not ... =>') are deferred for Alpha.",
                token_stream.current_location(),
                "Match Statement Parsing",
                "Use explicit positive case arms and an 'else =>' fallback in this phase.",
            ));
        }
        TokenKind::TypeParameterBracket => {
            return Err(deferred_feature_rule_error(
                "Capture/tagged patterns using '|...|' are deferred for Alpha.",
                token_stream.current_location(),
                "Match Statement Parsing",
                "Use simple literal or choice-variant patterns only.",
            ));
        }
        _ => {}
    }

    Ok(())
}

/// Unwrap a `Reference` wrapper so pattern checks compare against the inner value type.
pub(super) fn normalized_subject_type(data_type: &DataType) -> &DataType {
    match data_type {
        DataType::Reference(inner) => inner.as_ref(),
        _ => data_type,
    }
}
