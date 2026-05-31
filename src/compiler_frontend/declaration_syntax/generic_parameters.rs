//! Generic parameter-list parsing shared by top-level declaration headers.
//!
//! WHAT: parses `type T, U` after a declaration name into `GenericParameterList`.
//! WHY: functions, structs, and choices share exactly one generic-parameter syntax and
//! should not grow parallel validation paths as generics expand.

#![allow(clippy::result_large_err)]

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorType};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, InvalidGenericParameterReason,
};
use crate::compiler_frontend::datatypes::generic_parameters::{
    GenericParameter, GenericParameterList, GenericParameterScope, TypeParameterId,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use rustc_hash::FxHashSet;

/// Parse a generic parameter list after the current `type` keyword.
///
/// The parser stops with the token stream positioned on the declaration delimiter
/// (`|`, `=`, `::`, or `as`) so the owning header parser can continue normally.
pub(crate) fn parse_generic_parameter_list_after_type_keyword(
    token_stream: &mut FileTokens,
    forbidden_names: &FxHashSet<StringId>,
    string_table: &StringTable,
) -> Result<GenericParameterList, CompilerDiagnostic> {
    if token_stream.current_token_kind() != &TokenKind::Type {
        return Err(CompilerError::new(
            "Generic parameter parser was called when the current token was not `type`.",
            token_stream.current_location(),
            ErrorType::Compiler,
        )
        .into());
    }

    let type_keyword_location = token_stream.current_location();
    token_stream.advance();

    let mut parameters = Vec::new();
    let mut expecting_parameter = true;

    loop {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::Symbol(name) if expecting_parameter => {
                parameters.push(GenericParameter {
                    id: TypeParameterId(parameters.len() as u32),
                    name,
                    location: token_stream.current_location(),
                });
                token_stream.advance();
                expecting_parameter = false;
            }

            TokenKind::Symbol(_) => {
                return Err(CompilerDiagnostic::unexpected_token(
                    token_stream.current_token_kind().to_owned(),
                    token_stream.current_location(),
                ));
            }

            TokenKind::Comma => {
                if expecting_parameter {
                    return Err(CompilerDiagnostic::unexpected_token(
                        token_stream.current_token_kind().to_owned(),
                        token_stream.current_location(),
                    ));
                }

                token_stream.advance();
                expecting_parameter = true;
            }

            TokenKind::Is if !expecting_parameter => {
                return Err(CompilerDiagnostic::deferred_feature_reason(
                    DeferredFeatureReason::GenericConstraints,
                    token_stream.current_location(),
                ));
            }

            TokenKind::TypeParameterBracket
            | TokenKind::Assign
            | TokenKind::DoubleColon
            | TokenKind::As => {
                if parameters.is_empty() {
                    return Err(CompilerDiagnostic::invalid_generic_parameter(
                        InvalidGenericParameterReason::EmptyParameterList,
                        type_keyword_location,
                    ));
                }

                if expecting_parameter {
                    return Err(CompilerDiagnostic::unexpected_token(
                        token_stream.current_token_kind().to_owned(),
                        token_stream.current_location(),
                    ));
                }

                let parameter_list = GenericParameterList { parameters };
                GenericParameterScope::from_parameter_list(
                    &parameter_list,
                    None,
                    forbidden_names,
                    string_table,
                    "Header Parsing",
                )?;
                return Ok(parameter_list);
            }

            TokenKind::Must | TokenKind::Colon => {
                return Err(CompilerDiagnostic::invalid_generic_parameter(
                    InvalidGenericParameterReason::BoundsNotSupported,
                    token_stream.current_location(),
                ));
            }

            TokenKind::Newline => {
                return Err(CompilerDiagnostic::invalid_generic_parameter(
                    InvalidGenericParameterReason::ListMustStayWithHeader,
                    token_stream.current_location(),
                ));
            }

            TokenKind::Eof | TokenKind::End => {
                return Err(CompilerDiagnostic::unexpected_end_of_file(
                    None,
                    token_stream.current_location(),
                ));
            }

            other => {
                return Err(CompilerDiagnostic::invalid_generic_parameter(
                    InvalidGenericParameterReason::InvalidToken { found: other },
                    token_stream.current_location(),
                ));
            }
        }
    }
}
