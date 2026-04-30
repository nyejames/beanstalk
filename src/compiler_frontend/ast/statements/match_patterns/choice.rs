//! Choice-variant pattern parsing.
//!
//! WHAT: parses `case Variant` and `case Variant(field)` patterns, resolving
//! variant names against the scrutinee choice declaration.
//! WHY: choice patterns have unique syntax (qualified names, payload captures)
//! and validation (exact field names, no reordering) that justify a dedicated file.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::choice::{ChoiceVariant, ChoiceVariantPayload};
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;
use rustc_hash::FxHashMap;

use super::diagnostics::reject_deferred_pattern_lead_token;
use super::types::ParsedChoicePattern;
use super::types::ParsedChoicePayloadCapture;

/// Resolve a choice variant pattern to its deterministic variant index.
///
/// WHAT: accepts bare (`Ready`) or qualified (`Status::Ready`) variant names and
/// resolves them against the scrutinee choice metadata.
/// WHY: later lowering uses the stable variant index in `HirPattern::ChoiceVariant`,
/// while payload captures are materialized separately at arm entry.
pub fn parse_choice_variant_pattern(
    token_stream: &mut FileTokens,
    match_context: &ScopeContext,
    choice_nominal_path: &InternedPath,
    variants: &[ChoiceVariant],
    string_table: &StringTable,
) -> Result<ParsedChoicePattern, CompilerError> {
    // Choice patterns support exact variant names plus constructor-like payload captures.
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
            "Capture/tagged patterns using '|...|' are deferred.",
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

    /// Resolve the leaf name of a choice payload field declaration.
    ///
    /// WHAT: extracts the terminal identifier from a payload field's interned path.
    /// WHY: payload fields are declarations with interned paths; during match-pattern
    ///      parsing we need the leaf string ID for capture-name validation.
    ///      This is an internal invariant: every payload field must have a leaf name.
    fn choice_payload_field_name(
        field: &Declaration,
        location: &SourceLocation,
        string_table: &StringTable,
    ) -> Result<StringId, CompilerError> {
        field.id.name().ok_or_else(|| {
            CompilerError::new(
                format!(
                    "Choice payload field '{}' has no leaf name during match-pattern parsing",
                    field.id.to_string(string_table)
                ),
                location.clone(),
                ErrorType::Compiler,
            )
        })
    }

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

                if token_stream.current_token_kind() == &TokenKind::Wildcard {
                    return_rule_error!(
                        "Wildcard pattern '_' is not supported in Beanstalk. Use 'else =>' for a catch-all arm.",
                        capture_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Use 'else =>' for a catch-all arm",
                        }
                    );
                }

                let field_name = match token_stream.current_token_kind() {
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

                let mut binding_name = field_name;
                let mut binding_location = capture_location.clone();

                // Parse optional `as <local_binding>` rename syntax
                if token_stream.current_token_kind() == &TokenKind::As {
                    token_stream.advance();
                    binding_location = token_stream.current_location();
                    let after_as_token = token_stream.current_token_kind().to_owned();
                    binding_name = match after_as_token {
                        TokenKind::Symbol(name) => {
                            token_stream.advance();
                            name
                        }
                        TokenKind::End
                        | TokenKind::Eof
                        | TokenKind::CloseParenthesis
                        | TokenKind::Comma => {
                            return_rule_error!(
                                "Expected local binding name after `as` in choice payload pattern.",
                                binding_location,
                                {
                                    CompilationStage => "Match Statement Parsing",
                                    PrimarySuggestion => "Provide a local binding name after `as`",
                                }
                            );
                        }
                        _ => {
                            return_rule_error!(
                                "Choice payload alias must be a local binding name.",
                                binding_location,
                                {
                                    CompilationStage => "Match Statement Parsing",
                                    PrimarySuggestion => "Use a simple identifier as the local binding name",
                                }
                            );
                        }
                    };
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
                            string_table.resolve(field_name)
                        ),
                    ));
                }

                // Check duplicate capture binding name (uses the local alias when present)
                if let Some(_first_loc) = seen_names.get(&binding_name) {
                    return_rule_error!(
                        format!(
                            "Duplicate capture binding '{}' in pattern for variant '{}'.",
                            string_table.resolve(binding_name),
                            variant_name_str
                        ),
                        binding_location,
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Remove the duplicate capture binding",
                        }
                    );
                }
                seen_names.insert(binding_name, binding_location.clone());

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

                let expected_field_name =
                    choice_payload_field_name(field_decl, &capture_location, string_table)?;
                if field_name != expected_field_name {
                    return_rule_error!(
                        format!(
                            "Capture binding '{}' does not match payload field name '{}' at position {} in variant '{}'.",
                            string_table.resolve(field_name),
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
                    field_name,
                    binding_name,
                    field_index,
                    field_type: field_decl.value.data_type.clone(),
                    location: capture_location,
                    binding_location,
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
