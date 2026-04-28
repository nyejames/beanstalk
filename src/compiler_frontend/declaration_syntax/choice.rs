//! Choice declaration shell parsing for the header stage.
//!
//! WHAT: defines the header-level choice metadata types and the parser that produces them
//! from `Choice :: VariantA, VariantB, ...;` syntax.
//! WHY: choice header metadata is consumed by both the dependency-sorting stage (for type-ref
//! edges) and AST construction. Centralising these types here keeps the header stage free of
//! direct AST imports for the choice shell contract.
//!
//! Body-context choice expression parsing (`Choice::Variant` values) lives in
//! `ast/expressions/parse_expression_identifiers.rs` and is intentionally separate.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::declaration_syntax::signature_members::SignatureMemberContext;
use crate::compiler_frontend::declaration_syntax::r#struct::parse_record_body;
use crate::compiler_frontend::declaration_syntax::type_syntax::for_each_named_type_in_data_type;
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
    reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;
use rustc_hash::FxHashMap;

#[derive(Clone, Debug)]
pub struct ChoiceVariant {
    pub id: StringId,
    pub payload: ChoiceVariantPayload,
    pub location: SourceLocation,
}

#[derive(Clone, Debug)]
pub enum ChoiceVariantPayload {
    /// Unit variant with no payload fields.
    Unit,
    /// Record payload: `Variant | field Type, ... |`.
    Record { fields: Vec<Declaration> },
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

/// Parse `Choice :: VariantA, VariantB, ...;` declarations.
///
/// WHAT: accepts unit variants and record-body payload variants.
///
/// - `Variant` alone => `ChoiceVariantPayload::Unit`.
/// - `Variant | field Type, ... |` => `ChoiceVariantPayload::Record`.
///
/// Rejects shorthand payloads (`Variant Type`), constructor-style declarations
/// (`Variant(...)`), and default values (`Variant = ...`).
pub(crate) fn parse_choice_shell(
    token_stream: &mut FileTokens,
    choice_path: &InternedPath,
    context: &ScopeContext,
    string_table: &mut StringTable,
    warnings: &mut Vec<CompilerWarning>,
) -> Result<Vec<ChoiceVariant>, CompilerError> {
    let mut variants = Vec::new();
    let mut seen_variants: FxHashMap<StringId, SourceLocation> = FxHashMap::default();

    // Caller is positioned on the `::` token.
    token_stream.advance();

    loop {
        token_stream.skip_newlines();
        let current_location = token_stream.current_location();
        let current_token = token_stream.current_token_kind().to_owned();

        match current_token {
            // RESERVED SYNTAX ERROR
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

                // Make sure this is not a duplicate variant name
                if let Some(_first_location) = seen_variants.get(&variant_name) {
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
                seen_variants.insert(variant_name, current_location.clone());

                if let Some(warning) = naming_warning_for_identifier(
                    string_table.resolve(variant_name),
                    current_location.to_owned(),
                    IdentifierNamingKind::TypeLike,
                ) {
                    warnings.push(warning);
                }

                // Advance past the variant name
                token_stream.advance();
                token_stream.skip_newlines();

                // The token immediately after a parsed variant decides whether this stays in
                // alpha-scope syntax or enters richer syntax.
                if let Some(keyword) = reserved_trait_keyword(token_stream.current_token_kind()) {
                    return Err(reserved_trait_keyword_error(
                        keyword,
                        token_stream.current_location(),
                        "Header Parsing",
                        "Use a normal choice variant form until traits are implemented",
                    ));
                }

                // Determine payload form based on the next token.
                let payload = match token_stream.current_token_kind() {
                    TokenKind::TypeParameterBracket => {
                        // Record body: Variant | field Type, ... |
                        let fields = parse_record_body(
                            token_stream,
                            context,
                            string_table,
                            SignatureMemberContext::ChoicePayloadField,
                            choice_path,
                        )?;

                        if fields.is_empty() {
                            return_rule_error!(
                                format!(
                                    "Empty record body in choice variant '{}'. Use a unit variant instead.",
                                    string_table.resolve(variant_name)
                                ),
                                current_location.clone(),
                                {
                                    CompilationStage => "Header Parsing",
                                    PrimarySuggestion => "Use a unit variant (no '| ... |') or add payload fields",
                                }
                            );
                        }

                        // Check for duplicate field names within this variant.
                        let mut seen_field_names: FxHashMap<StringId, SourceLocation> =
                            FxHashMap::default();
                        for field in &fields {
                            if let Some(field_name) = field.id.name() {
                                if let Some(_first_loc) = seen_field_names.get(&field_name) {
                                    return_rule_error!(
                                        format!(
                                            "Duplicate field '{}' in choice variant '{}'. Payload field names must be unique within a variant.",
                                            string_table.resolve(field_name),
                                            string_table.resolve(variant_name)
                                        ),
                                        field.value.location.clone(),
                                        {
                                            CompilationStage => "Header Parsing",
                                            ConflictType => "DuplicateChoicePayloadField",
                                            PrimarySuggestion => "Rename the duplicate field so each payload field name is unique",
                                        }
                                    );
                                }
                                seen_field_names.insert(field_name, field.value.location.clone());
                            }

                            // Reject direct recursive choice declarations.
                            let mut is_recursive = false;
                            for_each_named_type_in_data_type(
                                &field.value.data_type,
                                &mut |type_name| {
                                    if Some(type_name) == choice_path.name() {
                                        is_recursive = true;
                                    }
                                },
                            );
                            if is_recursive {
                                let choice_name = choice_path
                                    .name()
                                    .map(|name| string_table.resolve(name))
                                    .unwrap_or("<choice>");
                                return_rule_error!(
                                    format!(
                                        "Recursive choice declarations are not supported. Choice '{choice_name}' cannot appear in its own variant payload.",
                                    ),
                                    field.value.location.clone(),
                                    {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Use an indirect representation (for example, a reference or index) instead of direct recursion",
                                    }
                                );
                            }
                        }

                        ChoiceVariantPayload::Record { fields }
                    }

                    TokenKind::OpenParenthesis => {
                        return_rule_error!(
                            "Constructor-style choice declarations are not supported. Use `Variant | field Type |`.",
                            token_stream.current_location(),
                            {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Use record-body syntax, for example: `Variant | message String |`",
                            }
                        );
                    }

                    TokenKind::Assign => {
                        return Err(deferred_feature_rule_error(
                            "Choice variant default values are deferred. Construct a value explicitly with `Choice::Variant(...)`.",
                            token_stream.current_location(),
                            "Header Parsing",
                            "Remove the default assignment and keep this as a unit variant or use a record payload.",
                        ));
                    }

                    token if starts_choice_payload_type(token) => {
                        return_rule_error!(
                            "Choice payload shorthand is not supported. Use a record payload body: `Variant | field Type |`.",
                            token_stream.current_location(),
                            {
                                CompilationStage => "Header Parsing",
                                PrimarySuggestion => "Use record-body syntax, for example: `Variant | message String |`",
                            }
                        );
                    }

                    // Unit variant: comma, end, newline, or EOF follows.
                    _ => ChoiceVariantPayload::Unit,
                };

                variants.push(ChoiceVariant {
                    id: variant_name,
                    payload,
                    location: current_location.clone(),
                });

                // Handle the separator after the variant (or after its record body).
                match token_stream.current_token_kind() {
                    TokenKind::Comma => {
                        token_stream.advance();
                        continue;
                    }
                    TokenKind::End => {
                        token_stream.advance();
                        break;
                    }
                    TokenKind::Assign => {
                        return Err(deferred_feature_rule_error(
                            "Choice variant default values are deferred. Construct a value explicitly with `Choice::Variant(...)`.",
                            token_stream.current_location(),
                            "Header Parsing",
                            "Remove the default assignment and keep this as a unit variant or use a record payload.",
                        ));
                    }
                    TokenKind::Newline => {
                        token_stream.skip_newlines();
                        match token_stream.current_token_kind() {
                            TokenKind::Comma => {
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
                                return_rule_error!(
                                    "Unexpected record body separator. Record bodies must follow the variant name directly: `Variant | field Type |`.",
                                    token_stream.current_location(),
                                    {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Place '|' immediately after the variant name, not on a separate line after a separator",
                                    }
                                );
                            }
                            TokenKind::Assign => {
                                return Err(deferred_feature_rule_error(
                                    "Choice variant default values are deferred. Construct a value explicitly with `Choice::Variant(...)`.",
                                    token_stream.current_location(),
                                    "Header Parsing",
                                    "Remove the default assignment and keep this as a unit variant or use a record payload.",
                                ));
                            }
                            payload_token if starts_choice_payload_type(payload_token) => {
                                return_rule_error!(
                                    "Choice payload shorthand is not supported. Use a record payload body: `Variant | field Type |`.",
                                    token_stream.current_location(),
                                    {
                                        CompilationStage => "Header Parsing",
                                        PrimarySuggestion => "Use record-body syntax, for example: `Variant | message String |`",
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
                    "Unexpected record body separator '|' at the start of a choice variant. Record bodies must follow a variant name: `Variant | field Type |`.",
                    current_location,
                    {
                        CompilationStage => "Header Parsing",
                        PrimarySuggestion => "Place '|' immediately after the variant name",
                    }
                );
            }
            TokenKind::End => {
                if variants.is_empty() {
                    return_rule_error!(
                        "Choice declarations must define at least one variant.",
                        current_location, {
                            CompilationStage => "Header Parsing",
                            PrimarySuggestion => "Add one or more variants, for example: 'State :: Ready, Busy;'",
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
                        PrimarySuggestion => "Declare variants with names, for example: 'Choice :: First, Second;'",
                    }
                );
            }
        }
    }

    Ok(variants)
}
