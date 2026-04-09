//! Choice declaration/expression parsing helpers.
//!
//! WHAT: centralizes alpha-scope choice parsing for both header declarations and
//! `Choice::Variant` value construction.
//! WHY: this keeps future choice/tagged-union expansion in one place instead of
//! spreading logic across header parsing and expression parsing modules.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::{return_compiler_error, return_rule_error};
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct ChoiceHeaderMetadata {
    pub variants: Vec<ChoiceVariantMetadata>,
}

#[derive(Clone, Debug)]
pub struct ChoiceVariantMetadata {
    pub name: StringId,
    pub location: SourceLocation,
}

/// Parsed choice header payload returned to header parsing.
///
/// WHAT: keeps token/body and metadata bundled so header parsing can stay a thin
/// classifier that delegates detailed choice grammar handling here.
#[derive(Clone, Debug)]
pub struct ParsedChoiceHeaderPayload {
    pub body: Vec<Token>,
    pub metadata: ChoiceHeaderMetadata,
}

fn starts_choice_payload_type(token: &TokenKind) -> bool {
    matches!(
        token,
        TokenKind::DatatypeInt
            | TokenKind::DatatypeFloat
            | TokenKind::DatatypeBool
            | TokenKind::DatatypeString
            | TokenKind::DatatypeChar
            | TokenKind::DatatypeNone
            | TokenKind::OpenCurly
            | TokenKind::Mutable
            | TokenKind::Symbol(_)
    )
}

/// Parse `Choice :: VariantA, VariantB, ...;` declarations for alpha unit variants.
///
/// WHAT: accepts only identifier variants with comma/newline separators and optional
/// trailing comma.
/// WHY: alpha scope intentionally excludes payload/default/tagged forms, so this
/// parser emits targeted deferred-feature diagnostics for those richer syntaxes.
pub(crate) fn parse_choice_header_payload(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
    warnings: &mut Vec<CompilerWarning>,
) -> Result<ParsedChoiceHeaderPayload, CompilerError> {
    let mut body = Vec::new();
    let mut variants = Vec::new();
    let mut seen_variants: HashSet<StringId> = HashSet::new();

    // Caller is positioned on the `::` token.
    token_stream.advance();

    loop {
        token_stream.skip_newlines();
        let current_location = token_stream.current_location();
        let current_token = token_stream.current_token_kind().to_owned();

        match current_token {
            TokenKind::Must | TokenKind::TraitThis => {
                let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                    .expect("reserved trait token should map to a keyword");

                return Err(reserved_trait_keyword_error(
                    keyword,
                    current_location,
                    "Header Parsing",
                    "Use a normal choice variant name until traits are implemented",
                ));
            }

            TokenKind::Symbol(variant_name) => {
                ensure_not_keyword_shadow_identifier(
                    string_table.resolve(variant_name),
                    current_location.to_owned(),
                    "Header Parsing",
                )?;

                if !seen_variants.insert(variant_name) {
                    return_rule_error!(
                        format!(
                            "Duplicate choice variant '{}'. Variant names must be unique within a choice declaration.",
                            string_table.resolve(variant_name)
                        ),
                        current_location,
                        {
                            CompilationStage => "Header Parsing",
                            ConflictType => "DuplicateChoiceVariant",
                            PrimarySuggestion => "Rename the duplicate variant so each choice variant name is unique",
                        }
                    );
                }

                if let Some(warning) = naming_warning_for_identifier(
                    string_table.resolve(variant_name),
                    current_location.to_owned(),
                    IdentifierNamingKind::TypeLike,
                ) {
                    warnings.push(warning);
                }

                variants.push(ChoiceVariantMetadata {
                    name: variant_name,
                    location: current_location.clone(),
                });
                body.push(token_stream.current_token());
                token_stream.advance();

                // The token immediately after a parsed variant decides whether this stays in
                // alpha-scope syntax or enters deferred territory.
                let next_token = token_stream.current_token_kind().to_owned();
                if let Some(keyword) = reserved_trait_keyword(&next_token) {
                    return Err(reserved_trait_keyword_error(
                        keyword,
                        token_stream.current_location(),
                        "Header Parsing",
                        "Use a normal choice variant form until traits are implemented",
                    ));
                }
                if starts_choice_payload_type(&next_token) {
                    return_rule_error!(
                        "Choice payload variants are deferred for Alpha. Only unit variants ('Choice :: VariantA, VariantB;') are supported in this phase.",
                        token_stream.current_location(), {
                            CompilationStage => "Header Parsing",
                            PrimarySuggestion => "Remove the payload type and keep this variant as a unit variant for now",
                        }
                    );
                }

                match next_token {
                    TokenKind::OpenParenthesis => {
                        return_rule_error!(
                            "Choice payload variants using constructor-style declarations ('Variant(...)') are deferred for Alpha.",
                            token_stream.current_location(), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Declare this as a unit variant for now, for example: 'Choice :: Variant;'",
                            }
                        );
                    }
                    TokenKind::TypeParameterBracket => {
                        return_rule_error!(
                            "Tagged choice variant bodies using '| ... |' are deferred for Alpha.",
                            token_stream.current_location(), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Use only flat unit variants in this phase",
                            }
                        );
                    }
                    TokenKind::Assign => {
                        return_rule_error!(
                            "Choice variant default values are deferred for Alpha.",
                            token_stream.current_location(), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Remove the default assignment and keep this variant as a unit variant",
                            }
                        );
                    }
                    TokenKind::Comma => {
                        body.push(token_stream.current_token());
                        token_stream.advance();
                        continue;
                    }
                    TokenKind::End => {
                        token_stream.advance();
                        break;
                    }
                    TokenKind::Newline => {
                        token_stream.skip_newlines();
                        match token_stream.current_token_kind() {
                            TokenKind::Comma => {
                                body.push(token_stream.current_token());
                                token_stream.advance();
                                continue;
                            }
                            TokenKind::End => {
                                token_stream.advance();
                                break;
                            }
                            TokenKind::Must | TokenKind::TraitThis => {
                                let keyword =
                                    reserved_trait_keyword(token_stream.current_token_kind())
                                        .expect("reserved trait token should map to a keyword");

                                return Err(reserved_trait_keyword_error(
                                    keyword,
                                    token_stream.current_location(),
                                    "Header Parsing",
                                    "Use a normal choice variant name until traits are implemented",
                                ));
                            }
                            TokenKind::Symbol(_) => {
                                continue;
                            }
                            TokenKind::TypeParameterBracket => {
                                return_rule_error!(
                                    "Tagged choice variant bodies using '| ... |' are deferred for Alpha.",
                                    token_stream.current_location(), {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Use only flat unit variants in this phase",
                                    }
                                );
                            }
                            TokenKind::Assign => {
                                return_rule_error!(
                                    "Choice variant default values are deferred for Alpha.",
                                    token_stream.current_location(), {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Remove the default assignment and keep this variant as a unit variant",
                                    }
                                );
                            }
                            payload_token if starts_choice_payload_type(payload_token) => {
                                return_rule_error!(
                                    "Choice payload variants are deferred for Alpha. Only unit variants ('Choice :: VariantA, VariantB;') are supported in this phase.",
                                    token_stream.current_location(), {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Remove the payload type and keep this variant as a unit variant for now",
                                    }
                                );
                            }
                            _ => {
                                return_rule_error!(
                                    format!(
                                        "Expected ',', newline, or ';' after choice variant '{}'.",
                                        string_table.resolve(variant_name)
                                    ),
                                    token_stream.current_location(), {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Separate variants with commas/newlines and end the choice declaration with ';'",
                                    }
                                );
                            }
                        }
                    }
                    TokenKind::Eof => {
                        return_rule_error!(
                            "Unexpected end of file while parsing choice declaration. Missing ';' to close this declaration.",
                            token_stream.current_location(), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Close the choice declaration with ';'",
                                SuggestedInsertion => ";",
                            }
                        );
                    }
                    _ => {
                        return_rule_error!(
                            format!(
                                "Expected ',', newline, or ';' after choice variant '{}'.",
                                string_table.resolve(variant_name)
                            ),
                            token_stream.current_location(), {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Separate variants with commas/newlines and end the choice declaration with ';'",
                            }
                        );
                    }
                }
            }
            TokenKind::TypeParameterBracket => {
                return_rule_error!(
                    "Tagged choice variant bodies using '| ... |' are deferred for Alpha.",
                    current_location, {
                        CompilationStage => "Header Parsing",
                        PrimarySuggestion => "Use only flat unit variants in this phase",
                    }
                );
            }
            TokenKind::End => {
                if variants.is_empty() {
                    return_rule_error!(
                        "Choice declarations must define at least one variant.",
                        current_location, {
                            CompilationStage => "Header Parsing",
                            PrimarySuggestion => "Add one or more unit variants, for example: 'State :: Ready, Busy;'",
                        }
                    );
                }

                token_stream.advance();
                break;
            }
            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file while parsing choice declaration. Missing ';' to close this declaration.",
                    current_location, {
                        CompilationStage => "Header Parsing",
                        PrimarySuggestion => "Close the choice declaration with ';'",
                        SuggestedInsertion => ";",
                    }
                );
            }
            _ => {
                return_rule_error!(
                    "Expected a choice variant name after '::'.",
                    current_location, {
                        CompilationStage => "Header Parsing",
                        PrimarySuggestion => "Declare unit variants with names only, for example: 'Choice :: First, Second;'",
                    }
                );
            }
        }
    }

    Ok(ParsedChoiceHeaderPayload {
        body,
        metadata: ChoiceHeaderMetadata { variants },
    })
}

/// Parse `Choice::Variant` as a typed alpha choice value.
///
/// WHAT: resolves the variant against the declared choice and encodes the selected
/// variant as a deterministic internal tag index.
/// WHY: alpha reuses existing literal lowering (no new HIR expression variant) while
/// preserving full choice type identity on the expression.
pub(crate) fn parse_choice_variant_value(
    token_stream: &mut FileTokens,
    choice_declaration: &Declaration,
    string_table: &StringTable,
) -> Result<Expression, CompilerError> {
    let choice_name = choice_declaration
        .id
        .name_str(string_table)
        .unwrap_or("<choice>")
        .to_owned();

    let DataType::Choices(variants) = &choice_declaration.value.data_type else {
        return_compiler_error!(
            "Choice variant parser was called with a non-choice declaration '{}'.",
            choice_name
        );
    };

    token_stream.advance();
    if token_stream.current_token_kind() != &TokenKind::DoubleColon {
        return_compiler_error!(
            "Choice variant parser expected '::' after choice name '{}'.",
            choice_name
        );
    }

    token_stream.advance();
    token_stream.skip_newlines();

    let variant_location = token_stream.current_location();
    let variant_name = match token_stream.current_token_kind() {
        TokenKind::Symbol(name) => *name,
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");

            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "Expression Parsing",
                "Use a normal choice variant name until traits are implemented",
            ));
        }
        _ => {
            return_rule_error!(
                format!("Expected a variant name after '{}::'.", choice_name),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use namespaced unit variant syntax like 'Choice::Variant'",
                }
            );
        }
    };

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
                "Unknown variant '{}::{}'. Available variants: [{}].",
                choice_name,
                string_table.resolve(variant_name),
                available_variants
            ),
            variant_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use one of the declared variants for this choice",
            }
        );
    };

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::OpenParenthesis {
        return_rule_error!(
            format!(
                "Constructor-call syntax '{}::{}(...)' is deferred for Alpha because choice payload variants are not supported yet.",
                choice_name,
                string_table.resolve(variant_name)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use unit variants only for now: 'Choice::Variant'",
            }
        );
    }

    Ok(Expression::new(
        ExpressionKind::Int(variant_index as i64),
        variant_location,
        choice_declaration.value.data_type.to_owned(),
        Ownership::ImmutableOwned,
    ))
}
