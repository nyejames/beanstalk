//! Neutral `| ... |` signature and record member shell parsing.
//!
//! WHAT: parses function parameters, function returns, struct fields, and choice payload fields
//! into syntax shells that preserve parsed type references and default-expression tokens.
//! WHY: header parsing owns declaration-shell discovery, but AST owns type resolution and
//! expression parsing. Keeping this module AST-free preserves that stage boundary.

#![allow(clippy::result_large_err)]

use crate::compiler_frontend::compiler_messages::trait_keyword_diagnostics::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidFunctionSignatureReason, InvalidSignatureMemberReason,
    TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::parsed::ParsedTypeRef;
use crate::compiler_frontend::declaration_syntax::binding_mode::BindingMode;
use crate::compiler_frontend::declaration_syntax::declaration_shell::require_binding_marker_adjacent;
use crate::compiler_frontend::declaration_syntax::type_syntax::{
    TypeAnnotationContext, parse_type_annotation,
};
use crate::compiler_frontend::symbols::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::syntax_errors::signature_position::check_signature_common_mistake;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, Token, TokenKind};
use crate::compiler_frontend::utilities::token_scan::NestingDepth;
use crate::compiler_frontend::value_mode::ValueMode;

/// Distinguishes the two syntactic contexts that share `| ... |` member parsing.
///
/// WHAT: `this` is valid only in function parameter lists, not in struct fields.
/// WHY: the shell parser is shared, but legal names and defaults differ by context.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SignatureMemberContext {
    FunctionParameter,
    StructField,
    ChoicePayloadField,
    TraitRequirement,
}

/// One parsed parameter/field shell before AST type resolution.
#[derive(Clone, Debug)]
pub struct SignatureMemberSyntax {
    pub id: InternedPath,
    pub value_mode: ValueMode,
    pub is_reactive: bool,
    pub type_annotation: ParsedTypeRef,
    pub default_tokens: Vec<Token>,
    pub location: SourceLocation,
}

/// Function return-channel syntax before it becomes an AST `ReturnChannel`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReturnChannelSyntax {
    Success,
    Error,
}

/// One parsed function return item before AST type resolution.
#[derive(Clone, Debug)]
pub enum FunctionReturnSyntax {
    Value {
        type_annotation: ParsedTypeRef,
        location: SourceLocation,
    },
    AliasCandidates {
        parameter_indices: Vec<usize>,
        location: SourceLocation,
    },
}

#[derive(Clone, Debug)]
pub struct ReturnSlotSyntax {
    pub value: FunctionReturnSyntax,
    pub channel: ReturnChannelSyntax,
    pub location: SourceLocation,
}

/// Parsed function signature shell.
#[derive(Clone, Debug, Default)]
pub struct FunctionSignatureSyntax {
    pub parameters: Vec<SignatureMemberSyntax>,
    pub returns: Vec<ReturnSlotSyntax>,
}

impl SignatureMemberSyntax {
    /// Remap all interned names, paths, type refs, tokens, and source locations.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.id.remap_string_ids(remap);
        self.type_annotation.remap_string_ids(remap);
        for token in &mut self.default_tokens {
            token.remap_string_ids(remap);
        }
        self.location.remap_string_ids(remap);
    }
}

impl FunctionReturnSyntax {
    /// Remap return type references and source locations into the merged string table.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        match self {
            FunctionReturnSyntax::Value {
                type_annotation,
                location,
            } => {
                type_annotation.remap_string_ids(remap);
                location.remap_string_ids(remap);
            }

            FunctionReturnSyntax::AliasCandidates { location, .. } => {
                location.remap_string_ids(remap);
            }
        }
    }
}

impl ReturnSlotSyntax {
    /// Remap this return slot's nested syntax.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.value.remap_string_ids(remap);
        self.location.remap_string_ids(remap);
    }
}

impl FunctionSignatureSyntax {
    /// Remap all interned string IDs in the signature shell.
    // Called by per-file frontend output remapping before module-wide dependency sorting.
    pub fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for parameter in &mut self.parameters {
            parameter.remap_string_ids(remap);
        }
        for return_slot in &mut self.returns {
            return_slot.remap_string_ids(remap);
        }
    }
}

/// Parse a full function signature shell from `| params | -> returns:`.
///
/// ENTRY INVARIANT: the stream is positioned on the opening `|`.
/// EXIT INVARIANT: the stream is positioned immediately after the terminating `:`.
pub fn parse_function_signature_syntax(
    token_stream: &mut FileTokens,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
    function_path: &InternedPath,
) -> Result<FunctionSignatureSyntax, CompilerDiagnostic> {
    token_stream.advance();

    let parameters = parse_signature_members_syntax(
        token_stream,
        string_table,
        warnings,
        SignatureMemberContext::FunctionParameter,
        function_path,
    )?;
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Arrow => {}

        TokenKind::Colon => {
            token_stream.advance();
            return Ok(FunctionSignatureSyntax {
                parameters,
                returns: Vec::new(),
            });
        }

        TokenKind::DatatypeInt
        | TokenKind::DatatypeFloat
        | TokenKind::DatatypeBool
        | TokenKind::DatatypeString
        | TokenKind::DatatypeChar
        | TokenKind::DatatypeNone
        | TokenKind::OpenCurly
        | TokenKind::Symbol(_) => {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::MissingArrowOrColon {
                    found: token_stream.current_token_kind().clone(),
                },
                token_stream.current_location(),
            ));
        }

        TokenKind::Newline | TokenKind::Eof | TokenKind::End => {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::UnexpectedEndAfterParameters,
                token_stream.current_location(),
            ));
        }

        _ => {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::MissingArrowOrColon {
                    found: token_stream.current_token_kind().clone(),
                },
                token_stream.current_location(),
            ));
        }
    }

    let returns = parse_return_list_syntax(token_stream, &parameters, string_table)?;

    Ok(FunctionSignatureSyntax {
        parameters,
        returns,
    })
}

/// Parses a `| name [~]Type [= default], ... |` member list into neutral shells.
///
/// ENTRY INVARIANT: the stream is positioned just after the opening `|`.
/// EXIT INVARIANT: the stream is positioned on the closing `|`.
pub fn parse_signature_members_syntax(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
    warnings: &mut Vec<CompilerDiagnostic>,
    member_context: SignatureMemberContext,
    owner_path: &InternedPath,
) -> Result<Vec<SignatureMemberSyntax>, CompilerDiagnostic> {
    let mut members = Vec::with_capacity(1);
    let mut expecting_member = true;
    let mut member_index = 0;

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::TypeParameterBracket => {
                return Ok(members);
            }

            TokenKind::End => {
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    None,
                    token_stream.current_location(),
                ));
            }

            TokenKind::Arrow | TokenKind::Colon => {
                return Err(CompilerDiagnostic::unexpected_token(
                    token_stream.current_token_kind().to_owned(),
                    token_stream.current_location(),
                ));
            }

            TokenKind::Symbol(member_name) => {
                if !expecting_member {
                    return Err(CompilerDiagnostic::expected_token(
                        TokenKind::Comma,
                        Some(token_stream.current_token_kind().to_owned()),
                        token_stream.current_location(),
                    ));
                }

                let member = parse_signature_member_syntax(
                    token_stream,
                    owner_path.append(member_name),
                    string_table,
                    warnings,
                    false,
                    member_context,
                )?;

                members.push(member);
                expecting_member = false;
                member_index += 1;
            }

            TokenKind::This if member_context == SignatureMemberContext::FunctionParameter => {
                if !expecting_member {
                    return Err(CompilerDiagnostic::expected_token(
                        TokenKind::Comma,
                        Some(token_stream.current_token_kind().to_owned()),
                        token_stream.current_location(),
                    ));
                }

                let this_id = string_table.intern("this");
                let member = parse_signature_member_syntax(
                    token_stream,
                    owner_path.append(this_id),
                    string_table,
                    warnings,
                    true,
                    member_context,
                )?;

                members.push(member);
                expecting_member = false;
                member_index += 1;
            }

            TokenKind::This => {
                return Err(CompilerDiagnostic::invalid_signature_member(
                    InvalidSignatureMemberReason::ThisNotAllowed,
                    token_stream.current_location(),
                ));
            }

            TokenKind::TraitThis if member_context == SignatureMemberContext::TraitRequirement => {
                if !expecting_member {
                    return Err(CompilerDiagnostic::expected_token(
                        TokenKind::Comma,
                        Some(token_stream.current_token_kind().to_owned()),
                        token_stream.current_location(),
                    ));
                }

                if member_index > 0 {
                    return Err(CompilerDiagnostic::invalid_signature_member(
                        InvalidSignatureMemberReason::TraitBareThisOnlyReceiver,
                        token_stream.current_location(),
                    ));
                }

                let this_id = string_table.intern("This");
                let member = parse_trait_this_member_syntax(
                    token_stream,
                    owner_path.append(this_id),
                    ValueMode::ImmutableOwned,
                )?;

                members.push(member);
                expecting_member = false;
                member_index += 1;
            }

            TokenKind::Mutable if member_context == SignatureMemberContext::TraitRequirement => {
                if !expecting_member {
                    return Err(CompilerDiagnostic::expected_token(
                        TokenKind::Comma,
                        Some(token_stream.current_token_kind().to_owned()),
                        token_stream.current_location(),
                    ));
                }

                token_stream.advance();

                if token_stream.current_token_kind() != &TokenKind::TraitThis {
                    return Err(CompilerDiagnostic::invalid_signature_member(
                        InvalidSignatureMemberReason::TraitReceiverMustBeThis,
                        token_stream.current_location(),
                    ));
                }

                if member_index > 0 {
                    return Err(CompilerDiagnostic::invalid_signature_member(
                        InvalidSignatureMemberReason::TraitMutableThisOnlyFirstParameter,
                        token_stream.current_location(),
                    ));
                }

                let this_id = string_table.intern("This");
                let member = parse_trait_this_member_syntax(
                    token_stream,
                    owner_path.append(this_id),
                    ValueMode::MutableOwned,
                )?;

                members.push(member);
                expecting_member = false;
                member_index += 1;
            }

            TokenKind::Comma => {
                token_stream.advance();
                if token_stream.current_token_kind() == &TokenKind::TypeParameterBracket {
                    return Err(CompilerDiagnostic::unexpected_trailing_comma(
                        token_stream.current_location(),
                    ));
                }
                expecting_member = true;
            }

            TokenKind::Must | TokenKind::TraitThis => {
                let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                    token_stream.current_token_kind(),
                    token_stream.current_location(),
                    "Struct/Parameter Parsing",
                    "signature member parsing",
                )?;

                return Err(reserved_trait_keyword_error(
                    keyword,
                    token_stream.current_location(),
                ));
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Eof => {
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    None,
                    token_stream.current_location(),
                ));
            }

            _ => {
                if let Some(error) = check_signature_common_mistake(token_stream) {
                    return Err(error);
                }

                return Err(CompilerDiagnostic::unexpected_token(
                    token_stream.current_token_kind().to_owned(),
                    token_stream.current_location(),
                ));
            }
        }
    }

    Ok(members)
}

fn parse_signature_member_syntax(
    token_stream: &mut FileTokens,
    full_name: InternedPath,
    string_table: &mut StringTable,
    warnings: &mut Vec<CompilerDiagnostic>,
    allow_reserved_this: bool,
    member_context: SignatureMemberContext,
) -> Result<SignatureMemberSyntax, CompilerDiagnostic> {
    let member_location = token_stream.current_location();
    let member_name = full_name
        .name()
        .map(|id| string_table.resolve(id).to_owned())
        .unwrap_or_else(|| String::from("<unknown>"));

    if (!allow_reserved_this || member_name != "this")
        && let Some(name_id) = full_name.name()
    {
        ensure_not_keyword_shadow_identifier(name_id, member_location.clone(), string_table)?;
    }

    if let Some(name_id) = full_name.name()
        && let Some(warning) = naming_warning_for_identifier(
            name_id,
            member_location.clone(),
            IdentifierNamingKind::ValueLike,
            string_table,
        )
    {
        warnings.push(warning);
    }

    token_stream.advance();

    let mut value_mode = ValueMode::ImmutableOwned;
    let mut is_reactive = false;
    match token_stream.current_token_kind() {
        TokenKind::Mutable => {
            token_stream.advance();
            value_mode = ValueMode::MutableOwned;
        }

        TokenKind::Reactive => {
            if member_context != SignatureMemberContext::FunctionParameter {
                return Err(CompilerDiagnostic::invalid_signature_member(
                    InvalidSignatureMemberReason::ReactiveAccessNotAllowed,
                    token_stream.current_location(),
                ));
            }

            require_binding_marker_adjacent(token_stream, BindingMode::ReactiveRuntime)?;
            token_stream.advance();
            is_reactive = true;
        }

        _ => {}
    }

    if token_stream.current_token_kind() == &TokenKind::Hash {
        return Err(CompilerDiagnostic::invalid_signature_member(
            InvalidSignatureMemberReason::CompileTimeParameterDeferred,
            token_stream.current_location(),
        ));
    }

    if member_context == SignatureMemberContext::ChoicePayloadField
        && value_mode == ValueMode::MutableOwned
    {
        return Err(CompilerDiagnostic::invalid_signature_member(
            InvalidSignatureMemberReason::ChoicePayloadMutable,
            token_stream.current_location(),
        ));
    }

    while token_stream.current_token_kind() == &TokenKind::Newline {
        token_stream.advance();
    }

    let type_annotation = parse_type_annotation(
        token_stream,
        type_annotation_context_for_member(member_context),
        string_table,
    )?;
    let default_tokens = match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
            if is_reactive {
                return Err(CompilerDiagnostic::invalid_signature_member(
                    InvalidSignatureMemberReason::ReactiveParameterDefaultValue,
                    token_stream.current_location(),
                ));
            }
            if member_context == SignatureMemberContext::TraitRequirement {
                return Err(CompilerDiagnostic::invalid_signature_member(
                    InvalidSignatureMemberReason::TraitRequirementDefaultValue,
                    token_stream.current_location(),
                ));
            }
            if member_context == SignatureMemberContext::ChoicePayloadField {
                return Err(CompilerDiagnostic::invalid_signature_member(
                    InvalidSignatureMemberReason::ChoicePayloadDefaultValue,
                    token_stream.current_location(),
                ));
            }

            collect_member_default_tokens(token_stream)?
        }

        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => Vec::new(),

        TokenKind::As => {
            return Err(CompilerDiagnostic::unexpected_token(
                token_stream.current_token_kind().to_owned(),
                token_stream.current_location(),
            ));
        }

        _ => {
            return Err(CompilerDiagnostic::unexpected_token(
                token_stream.current_token_kind().to_owned(),
                token_stream.current_location(),
            ));
        }
    };

    Ok(SignatureMemberSyntax {
        id: full_name,
        value_mode,
        is_reactive,
        type_annotation,
        default_tokens,
        location: member_location,
    })
}

fn type_annotation_context_for_member(
    member_context: SignatureMemberContext,
) -> TypeAnnotationContext {
    match member_context {
        SignatureMemberContext::TraitRequirement => TypeAnnotationContext::TraitRequirement,
        SignatureMemberContext::FunctionParameter
        | SignatureMemberContext::StructField
        | SignatureMemberContext::ChoicePayloadField => TypeAnnotationContext::SignatureParameter,
    }
}

/// Parse a `This` receiver parameter in a trait requirement.
///
/// ENTRY INVARIANT: the stream is positioned on `This` (TraitThis).
/// EXIT INVARIANT: the stream is positioned on the token after `This`.
fn parse_trait_this_member_syntax(
    token_stream: &mut FileTokens,
    full_name: InternedPath,
    value_mode: ValueMode,
) -> Result<SignatureMemberSyntax, CompilerDiagnostic> {
    let member_location = token_stream.current_location();

    token_stream.advance(); // past This

    // Trait receiver parameters have no explicit type annotation;
    // the type is implicitly the implementing concrete type.
    let type_annotation = ParsedTypeRef::This {
        location: member_location.clone(),
    };

    // Default values are not allowed in trait requirements.
    let default_tokens = Vec::new();

    Ok(SignatureMemberSyntax {
        id: full_name,
        value_mode,
        is_reactive: false,
        type_annotation,
        default_tokens,
        location: member_location,
    })
}

fn collect_member_default_tokens(
    token_stream: &mut FileTokens,
) -> Result<Vec<Token>, CompilerDiagnostic> {
    let mut tokens = Vec::new();
    let mut depth = NestingDepth::default();

    while token_stream.index < token_stream.length {
        let token_kind = token_stream.current_token_kind().clone();

        if depth.is_top_level()
            && matches!(
                token_kind,
                TokenKind::Comma | TokenKind::TypeParameterBracket | TokenKind::Eof
            )
        {
            break;
        }

        if matches!(token_kind, TokenKind::Eof) {
            return Err(CompilerDiagnostic::unexpected_end_of_file(
                None,
                token_stream.current_location(),
            ));
        }

        depth.step(&token_kind);
        tokens.push(token_stream.current_token());
        token_stream.advance();
    }

    if tokens.is_empty() {
        return Err(CompilerDiagnostic::unexpected_token(
            token_stream.current_token_kind().to_owned(),
            token_stream.current_location(),
        ));
    }

    Ok(tokens)
}

/// Parse a return list for a trait requirement, stopping at newline or block end.
///
/// ENTRY INVARIANT: the stream is positioned on the `->` arrow.
/// EXIT INVARIANT: the stream is positioned on the first token after the last return type.
fn parse_trait_requirement_return_list(
    token_stream: &mut FileTokens,
    parameters: &[SignatureMemberSyntax],
    string_table: &mut StringTable,
) -> Result<Vec<ReturnSlotSyntax>, CompilerDiagnostic> {
    let mut return_slots = Vec::new();

    token_stream.advance(); // past ->

    loop {
        return_slots.push(parse_single_return_item_syntax(
            token_stream,
            parameters,
            string_table,
            TypeAnnotationContext::TraitRequirement,
        )?);

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                let comma_location = token_stream.current_location();
                token_stream.advance();

                match token_stream.current_token_kind() {
                    TokenKind::Newline | TokenKind::End | TokenKind::Eof => {
                        return Err(CompilerDiagnostic::unexpected_trailing_comma(
                            comma_location,
                        ));
                    }

                    _ => {}
                }
            }

            TokenKind::Newline | TokenKind::End | TokenKind::Eof => {
                return Ok(return_slots);
            }

            unexpected_token => {
                return Err(CompilerDiagnostic::invalid_function_signature(
                    InvalidFunctionSignatureReason::MissingCommaOrColon {
                        found: unexpected_token.clone(),
                    },
                    token_stream.current_location(),
                ));
            }
        }
    }
}

/// Parse a trait requirement signature from `| params | [-> returns]`.
///
/// ENTRY INVARIANT: the stream is positioned on the opening `|`.
/// EXIT INVARIANT: the stream is positioned on the first token after the signature.
pub fn parse_trait_requirement_signature_syntax(
    token_stream: &mut FileTokens,
    warnings: &mut Vec<CompilerDiagnostic>,
    string_table: &mut StringTable,
    method_path: &InternedPath,
) -> Result<FunctionSignatureSyntax, CompilerDiagnostic> {
    token_stream.advance(); // past |

    let parameters = parse_signature_members_syntax(
        token_stream,
        string_table,
        warnings,
        SignatureMemberContext::TraitRequirement,
        method_path,
    )?;
    token_stream.advance(); // past |

    let returns = if token_stream.current_token_kind() == &TokenKind::Arrow {
        parse_trait_requirement_return_list(token_stream, &parameters, string_table)?
    } else {
        Vec::new()
    };

    Ok(FunctionSignatureSyntax {
        parameters,
        returns,
    })
}

fn parse_return_list_syntax(
    token_stream: &mut FileTokens,
    parameters: &[SignatureMemberSyntax],
    string_table: &mut StringTable,
) -> Result<Vec<ReturnSlotSyntax>, CompilerDiagnostic> {
    let mut return_slots = Vec::new();

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::Colon {
        return Err(CompilerDiagnostic::invalid_function_signature(
            InvalidFunctionSignatureReason::UnexpectedColonAfterArrow,
            token_stream.current_location(),
        ));
    }

    loop {
        return_slots.push(parse_single_return_item_syntax(
            token_stream,
            parameters,
            string_table,
            TypeAnnotationContext::SignatureReturn,
        )?);

        match token_stream.current_token_kind() {
            TokenKind::Comma => {
                let comma_location = token_stream.current_location();
                token_stream.advance();

                match token_stream.current_token_kind() {
                    TokenKind::Colon => {
                        return Err(CompilerDiagnostic::invalid_function_signature(
                            InvalidFunctionSignatureReason::TrailingCommaInReturns,
                            comma_location,
                        ));
                    }

                    TokenKind::Newline | TokenKind::End | TokenKind::Eof => {
                        return Err(CompilerDiagnostic::invalid_function_signature(
                            InvalidFunctionSignatureReason::UnexpectedEndAfterComma,
                            comma_location,
                        ));
                    }

                    _ => {}
                }
            }
            TokenKind::Symbol(symbol) if string_table.resolve(*symbol) == "where" => {
                return Err(CompilerDiagnostic::invalid_function_signature(
                    InvalidFunctionSignatureReason::GenericWhereConstraintsUnsupported,
                    token_stream.current_location(),
                ));
            }
            TokenKind::Colon => {
                token_stream.advance();
                validate_return_slots_syntax(&return_slots, token_stream, string_table)?;
                return Ok(return_slots);
            }
            TokenKind::Eof => {
                return Err(CompilerDiagnostic::invalid_function_signature(
                    InvalidFunctionSignatureReason::UnexpectedEndInReturns,
                    token_stream.current_location(),
                ));
            }
            TokenKind::Newline | TokenKind::End => {
                return Err(CompilerDiagnostic::invalid_function_signature(
                    InvalidFunctionSignatureReason::MissingColonAfterReturns,
                    token_stream.current_location(),
                ));
            }
            TokenKind::Arrow => {
                return Err(CompilerDiagnostic::invalid_function_signature(
                    InvalidFunctionSignatureReason::UnexpectedArrowInReturns,
                    token_stream.current_location(),
                ));
            }
            unexpected_token => {
                return Err(CompilerDiagnostic::invalid_function_signature(
                    InvalidFunctionSignatureReason::MissingCommaOrColon {
                        found: unexpected_token.clone(),
                    },
                    token_stream.current_location(),
                ));
            }
        }
    }
}

fn parse_single_return_item_syntax(
    token_stream: &mut FileTokens,
    parameters: &[SignatureMemberSyntax],
    string_table: &mut StringTable,
    type_context: TypeAnnotationContext,
) -> Result<ReturnSlotSyntax, CompilerDiagnostic> {
    let location = token_stream.current_location();
    if let Some(symbol) = parameter_alias_symbol(token_stream.current_token_kind(), string_table) {
        if type_context == TypeAnnotationContext::TraitRequirement
            && parameters
                .iter()
                .any(|parameter| parameter.id.name() == Some(symbol))
        {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::AliasReturnNotAllowedInTraitRequirement,
                location,
            ));
        }

        if parameters
            .iter()
            .any(|parameter| parameter.id.name() == Some(symbol))
        {
            return parse_alias_return_item_syntax(
                token_stream,
                parameters,
                string_table,
                location,
            );
        }

        if string_table.resolve(symbol) == "Void" {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::VoidNotAllowed,
                location,
            ));
        }
    }

    parse_value_return_type_syntax(token_stream, string_table, type_context)
}

fn parse_value_return_type_syntax(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
    type_context: TypeAnnotationContext,
) -> Result<ReturnSlotSyntax, CompilerDiagnostic> {
    let location = token_stream.current_location();
    let type_annotation = parse_type_annotation(token_stream, type_context, string_table)?;

    if parsed_type_ref_is_void(&type_annotation, string_table) {
        return Err(CompilerDiagnostic::invalid_function_signature(
            InvalidFunctionSignatureReason::VoidNotAllowed,
            location,
        ));
    }

    let channel = if token_stream.current_token_kind() == &TokenKind::Bang {
        token_stream.advance();
        ReturnChannelSyntax::Error
    } else {
        ReturnChannelSyntax::Success
    };

    Ok(ReturnSlotSyntax {
        value: FunctionReturnSyntax::Value {
            type_annotation,
            location: location.clone(),
        },
        channel,
        location,
    })
}

fn parse_alias_return_item_syntax(
    token_stream: &mut FileTokens,
    parameters: &[SignatureMemberSyntax],
    string_table: &mut StringTable,
    location: SourceLocation,
) -> Result<ReturnSlotSyntax, CompilerDiagnostic> {
    let mut candidate_indices = Vec::new();

    loop {
        let Some(current_symbol) =
            parameter_alias_symbol(token_stream.current_token_kind(), string_table)
        else {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::MissingParameterNameInAlias,
                token_stream.current_location(),
            ));
        };

        let Some((parameter_index, _parameter)) = parameters
            .iter()
            .enumerate()
            .find(|(_, parameter)| parameter.id.name() == Some(current_symbol))
        else {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::UnknownReturnAlias {
                    name: current_symbol,
                },
                location,
            ));
        };

        if candidate_indices.contains(&parameter_index) {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::DuplicateParameterInAlias,
                token_stream.current_location(),
            ));
        }

        candidate_indices.push(parameter_index);
        token_stream.advance();

        match token_stream.current_token_kind() {
            TokenKind::Or => {
                token_stream.advance();
                if parameter_alias_symbol(token_stream.current_token_kind(), string_table).is_none()
                {
                    return Err(CompilerDiagnostic::invalid_function_signature(
                        InvalidFunctionSignatureReason::MissingParameterNameInAlias,
                        token_stream.current_location(),
                    ));
                }
            }
            _ => break,
        }
    }

    if candidate_indices.is_empty() {
        return Err(CompilerDiagnostic::invalid_function_signature(
            InvalidFunctionSignatureReason::MissingParameterNameInAlias,
            location,
        ));
    }

    if token_stream.current_token_kind() == &TokenKind::Bang {
        return Err(CompilerDiagnostic::invalid_function_signature(
            InvalidFunctionSignatureReason::AliasCannotBeError,
            token_stream.current_location(),
        ));
    }

    Ok(ReturnSlotSyntax {
        value: FunctionReturnSyntax::AliasCandidates {
            parameter_indices: candidate_indices,
            location: location.clone(),
        },
        channel: ReturnChannelSyntax::Success,
        location,
    })
}

fn validate_return_slots_syntax(
    returns: &[ReturnSlotSyntax],
    token_stream: &FileTokens,
    string_table: &StringTable,
) -> Result<(), CompilerDiagnostic> {
    let error_return_slots: Vec<(usize, &ReturnSlotSyntax)> = returns
        .iter()
        .enumerate()
        .filter(|(_, return_slot)| return_slot.channel == ReturnChannelSyntax::Error)
        .collect();

    if error_return_slots.len() > 1 {
        return Err(CompilerDiagnostic::invalid_function_signature(
            InvalidFunctionSignatureReason::MultipleErrorReturnSlots,
            token_stream.current_location(),
        ));
    }

    if let Some((error_index, _)) = error_return_slots.first()
        && *error_index + 1 != returns.len()
    {
        return Err(CompilerDiagnostic::invalid_function_signature(
            InvalidFunctionSignatureReason::ErrorSlotNotLast,
            token_stream.current_location(),
        ));
    }

    for return_slot in returns {
        if let FunctionReturnSyntax::Value {
            type_annotation, ..
        } = &return_slot.value
            && parsed_type_ref_is_void(type_annotation, string_table)
        {
            return Err(CompilerDiagnostic::invalid_function_signature(
                InvalidFunctionSignatureReason::VoidNotAllowed,
                token_stream.current_location(),
            ));
        }
    }

    Ok(())
}

fn parameter_alias_symbol(token: &TokenKind, string_table: &mut StringTable) -> Option<StringId> {
    match token {
        TokenKind::Symbol(symbol) => Some(*symbol),
        TokenKind::This => Some(string_table.intern("this")),
        _ => None,
    }
}

fn parsed_type_ref_is_void(type_ref: &ParsedTypeRef, string_table: &StringTable) -> bool {
    matches!(
        type_ref,
        ParsedTypeRef::Named { name, .. } if string_table.resolve(*name) == "Void"
    )
}

pub(crate) fn alias_return_type_mismatch_diagnostic(
    existing_type_id: crate::compiler_frontend::datatypes::ids::TypeId,
    param_type_id: crate::compiler_frontend::datatypes::ids::TypeId,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::type_mismatch(
        existing_type_id,
        param_type_id,
        TypeMismatchContext::General,
        location,
    )
}
