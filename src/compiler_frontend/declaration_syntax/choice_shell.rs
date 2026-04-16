//! Choice declaration shell parsing for the header stage.
//!
//! WHAT: defines the header-level choice metadata types and the parser that produces them
//! from `Choice :: VariantA, VariantB, ...;` syntax.
//! WHY: choice header metadata is consumed by both the dependency-sorting stage (for type-ref
//! edges) and AST construction. Centralising these types here keeps the header stage free of
//! direct AST imports for the choice shell contract.
//!
//! Body-context choice expression parsing (`Choice::Variant` values) lives in
//! `ast/statements/choices.rs` and is intentionally separate.

use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
    reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::return_rule_error;
use std::collections::HashSet;

#[derive(Clone, Debug)]
pub struct ChoiceVariant {
    pub declaration: Declaration,
    pub location: SourceLocation,
}

pub(crate) fn starts_choice_payload_type(token: &TokenKind) -> bool {
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
pub(crate) fn parse_choice_shell(
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
                let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                    token_stream.current_token_kind(),
                    current_location.clone(),
                    "Header Parsing",
                    "choice header payload parsing",
                )?;

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
                    return Err(deferred_feature_rule_error(
                        "Choice payload variants are deferred for Alpha.",
                        token_stream.current_location(),
                        "Header Parsing",
                        "Declare this as a unit variant for now, for example: 'Choice :: VariantA, VariantB;'.",
                    ));
                }

                match next_token {
                    TokenKind::OpenParenthesis => {
                        return Err(deferred_feature_rule_error(
                            "Constructor-style choice variant declarations ('Variant(...)') are deferred for Alpha.",
                            token_stream.current_location(),
                            "Header Parsing",
                            "Declare this as a unit variant for now, for example: 'Choice :: Variant;'.",
                        ));
                    }
                    TokenKind::TypeParameterBracket => {
                        return Err(deferred_feature_rule_error(
                            "Tagged choice variant bodies using '| ... |' are deferred for Alpha.",
                            token_stream.current_location(),
                            "Header Parsing",
                            "Use unit variants only in this phase.",
                        ));
                    }
                    TokenKind::Assign => {
                        return Err(deferred_feature_rule_error(
                            "Choice variant default values are deferred for Alpha.",
                            token_stream.current_location(),
                            "Header Parsing",
                            "Remove the default assignment and keep this as a unit variant.",
                        ));
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
                                let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                                    token_stream.current_token_kind(),
                                    token_stream.current_location(),
                                    "Header Parsing",
                                    "choice header payload parsing",
                                )?;

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
                                return Err(deferred_feature_rule_error(
                                    "Tagged choice variant bodies using '| ... |' are deferred for Alpha.",
                                    token_stream.current_location(),
                                    "Header Parsing",
                                    "Use unit variants only in this phase.",
                                ));
                            }
                            TokenKind::Assign => {
                                return Err(deferred_feature_rule_error(
                                    "Choice variant default values are deferred for Alpha.",
                                    token_stream.current_location(),
                                    "Header Parsing",
                                    "Remove the default assignment and keep this as a unit variant.",
                                ));
                            }
                            payload_token if starts_choice_payload_type(payload_token) => {
                                return Err(deferred_feature_rule_error(
                                    "Choice payload variants are deferred for Alpha.",
                                    token_stream.current_location(),
                                    "Header Parsing",
                                    "Declare this as a unit variant for now, for example: 'Choice :: VariantA, VariantB;'.",
                                ));
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
                return Err(deferred_feature_rule_error(
                    "Tagged choice variant bodies using '| ... |' are deferred for Alpha.",
                    current_location,
                    "Header Parsing",
                    "Use unit variants only in this phase.",
                ));
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
