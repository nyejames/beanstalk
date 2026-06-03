//! Header-stage trait declaration and conformance syntax parsing.
//!
//! WHAT: owns parsing of trait declaration shells (`TRAIT must: ... ;`) and conformance shells
//!       (`Type must TRAIT` / `Type of Generic must TRAIT`) discovered during header parsing.
//! WHY: extracting these helpers keeps `header_dispatch.rs` focused on top-level declaration
//!      dispatch/orchestration, while trait-specific syntax rules live in one focused module.
//!      This module parses syntax shells only; semantic trait resolution and evidence validation
//!      are owned by AST.

use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidDeclarationReason, InvalidSignatureMemberReason,
};
use crate::compiler_frontend::declaration_syntax::signature_members::parse_trait_requirement_signature_syntax;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::identifier_policy::is_uppercase_constant_name;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::traits::syntax::{
    ConformanceTargetKind, ConformanceTargetSyntax, TraitConformanceSyntax, TraitDeclarationSyntax,
    TraitReferenceSyntax, TraitRequirementSyntax, TraitThisUsage,
};

use super::types::HeaderBuildContext;

// ------------------------
//  Trait declaration parsing
// ------------------------

pub(super) fn parse_trait_declaration(
    token_stream: &mut FileTokens,
    declaration_name: crate::compiler_frontend::symbols::string_interning::StringId,
    name_location: SourceLocation,
    context: &mut HeaderBuildContext<'_>,
) -> Result<TraitDeclarationSyntax, CompilerDiagnostic> {
    let mut requirements = Vec::new();

    token_stream.skip_newlines();

    loop {
        match token_stream.current_token_kind() {
            TokenKind::End => {
                token_stream.advance();
                break;
            }

            TokenKind::Eof => {
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    Some(context.string_table.intern(";")),
                    token_stream.current_location(),
                ));
            }

            TokenKind::Newline => {
                token_stream.skip_newlines();
            }

            _ => {
                let requirement = parse_trait_requirement(token_stream, context)?;
                requirements.push(requirement);
            }
        }
    }

    Ok(TraitDeclarationSyntax {
        name: declaration_name,
        name_location: name_location.clone(),
        requirements,
        location: name_location,
    })
}

fn parse_trait_requirement(
    token_stream: &mut FileTokens,
    context: &mut HeaderBuildContext<'_>,
) -> Result<TraitRequirementSyntax, CompilerDiagnostic> {
    let name_location = token_stream.current_location();

    let TokenKind::Symbol(method_name) = token_stream.current_token_kind() else {
        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        ));
    };
    let method_name = *method_name;
    token_stream.advance();

    if token_stream.current_token_kind() != &TokenKind::TypeParameterBracket {
        return Err(CompilerDiagnostic::unexpected_token_in_declaration(
            token_stream.current_location(),
        ));
    }

    let method_path = InternedPath::from_single_str("trait_requirement", context.string_table)
        .append(method_name);

    let signature = parse_trait_requirement_signature_syntax(
        token_stream,
        context.warnings,
        context.string_table,
        &method_path,
    )?;

    // Every non-empty requirement must start with `This` or `~This`.
    let this_usage = if let Some(first_param) = signature.parameters.first() {
        let param_name = first_param
            .id
            .name()
            .map(|id| context.string_table.resolve(id))
            .unwrap_or("");
        if param_name != "This" {
            return Err(CompilerDiagnostic::invalid_signature_member(
                InvalidSignatureMemberReason::TraitReceiverMustBeThis,
                first_param.location.clone(),
            ));
        }

        if first_param.value_mode.is_mutable() {
            TraitThisUsage::Mutable
        } else {
            TraitThisUsage::Immutable
        }
    } else {
        return Err(CompilerDiagnostic::invalid_signature_member(
            InvalidSignatureMemberReason::TraitReceiverMustBeThis,
            name_location.clone(),
        ));
    };

    Ok(TraitRequirementSyntax {
        name: method_name,
        name_location: name_location.clone(),
        this_usage,
        signature,
        location: name_location,
    })
}

// ------------------------
//  Trait conformance parsing
// ------------------------

pub(super) fn parse_trait_conformance(
    token_stream: &mut FileTokens,
    target: ConformanceTargetSyntax,
    context: &mut HeaderBuildContext<'_>,
) -> Result<TraitConformanceSyntax, CompilerDiagnostic> {
    let mut traits = Vec::new();

    loop {
        match token_stream.current_token_kind() {
            TokenKind::Symbol(trait_name) => {
                let trait_location = token_stream.current_location();
                ensure_trait_name_is_all_caps(
                    *trait_name,
                    trait_location.clone(),
                    context.string_table,
                )?;

                traits.push(TraitReferenceSyntax {
                    name: *trait_name,
                    location: trait_location,
                });
                token_stream.advance();
            }

            _ => {
                if traits.is_empty() {
                    return Err(CompilerDiagnostic::invalid_declaration(
                        InvalidDeclarationReason::TraitConformanceMissingTrait,
                        Some(target.name),
                        token_stream.current_location(),
                    ));
                }

                return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                    token_stream.current_location(),
                ));
            }
        }

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                let comma_location = token_stream.current_location();
                token_stream.advance();
                token_stream.skip_newlines();

                // A comma may continue across newlines, but it must still be followed by a trait.
                if matches!(
                    token_stream.current_token_kind(),
                    TokenKind::End | TokenKind::Eof
                ) {
                    return Err(CompilerDiagnostic::unexpected_trailing_comma(
                        comma_location,
                    ));
                }
            }

            TokenKind::Newline | TokenKind::Eof => {
                break;
            }

            TokenKind::End => {
                return Err(CompilerDiagnostic::invalid_declaration(
                    InvalidDeclarationReason::TraitConformanceSemicolon,
                    Some(target.name),
                    token_stream.current_location(),
                ));
            }

            _ => {
                return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                    token_stream.current_location(),
                ));
            }
        }
    }

    Ok(TraitConformanceSyntax {
        location: target.location.clone(),
        target,
        traits,
    })
}

pub(super) fn parse_specialized_conformance_target(
    token_stream: &mut FileTokens,
    target_name: crate::compiler_frontend::symbols::string_interning::StringId,
    name_location: SourceLocation,
) -> Result<ConformanceTargetSyntax, CompilerDiagnostic> {
    token_stream.advance(); // past `of`

    loop {
        match token_stream.current_token_kind() {
            TokenKind::Must => {
                return Ok(ConformanceTargetSyntax {
                    name: target_name,
                    kind: ConformanceTargetKind::SpecializedGenericInstance,
                    location: name_location,
                });
            }

            TokenKind::Newline | TokenKind::End | TokenKind::Eof => {
                return Err(CompilerDiagnostic::unexpected_token_in_declaration(
                    token_stream.current_location(),
                ));
            }

            _ => token_stream.advance(),
        }
    }
}

pub(super) fn conformance_header_path(
    target_path: &InternedPath,
    location: &SourceLocation,
    string_table: &mut StringTable,
) -> InternedPath {
    target_path.join_str(
        &format!(
            "__trait_conformance_{}_{}",
            location.start_pos.line_number, location.start_pos.char_column
        ),
        string_table,
    )
}

pub(super) fn ensure_trait_name_is_all_caps(
    trait_name: crate::compiler_frontend::symbols::string_interning::StringId,
    location: SourceLocation,
    string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    if is_uppercase_constant_name(string_table.resolve(trait_name)) {
        return Ok(());
    }

    Err(CompilerDiagnostic::invalid_declaration(
        InvalidDeclarationReason::InvalidTraitName,
        Some(trait_name),
        location,
    ))
}
