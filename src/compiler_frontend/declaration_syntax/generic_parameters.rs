//! Generic parameter-list parsing shared by top-level declaration headers.
//!
//! WHAT: parses `type T, U` after a declaration name into `GenericParameterList`.
//! WHY: functions, structs, and choices share exactly one generic-parameter syntax and
//! should not grow parallel validation paths as generics expand.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::datatypes::generics::{
    GenericParameter, GenericParameterList, GenericParameterScope, TypeParameterId,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use rustc_hash::FxHashSet;
use std::collections::HashMap;

/// Parse a generic parameter list after the current `type` keyword.
///
/// The parser stops with the token stream positioned on the declaration delimiter
/// (`|`, `=`, `::`, or `as`) so the owning header parser can continue normally.
pub(crate) fn parse_generic_parameter_list_after_type_keyword(
    token_stream: &mut FileTokens,
    forbidden_names: &FxHashSet<StringId>,
    string_table: &StringTable,
) -> Result<GenericParameterList, CompilerError> {
    if token_stream.current_token_kind() != &TokenKind::Type {
        return Err(CompilerError::compiler_error(
            "Generic parameter parser was called when the current token was not `type`.",
        ));
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
                return Err(generic_parameter_syntax_error(
                    "Expected ',' between generic parameters.",
                    token_stream.current_location(),
                    "Separate generic parameters with commas, for example `type T, U`",
                ));
            }

            TokenKind::Comma => {
                if expecting_parameter {
                    return Err(generic_parameter_syntax_error(
                        "Expected a generic parameter name after `type` or ','.",
                        token_stream.current_location(),
                        "Write generic parameters as PascalCase names, for example `type T` or `type Item`",
                    ));
                }

                token_stream.advance();
                expecting_parameter = true;
            }

            TokenKind::TypeParameterBracket
            | TokenKind::Assign
            | TokenKind::DoubleColon
            | TokenKind::As => {
                if parameters.is_empty() {
                    return Err(generic_parameter_syntax_error(
                        "Expected at least one generic parameter after `type`.",
                        type_keyword_location,
                        "Add a PascalCase parameter name after `type`, for example `type T`",
                    ));
                }

                if expecting_parameter {
                    return Err(generic_parameter_syntax_error(
                        "Expected a generic parameter name after ','.",
                        token_stream.current_location(),
                        "Remove the trailing comma or add another generic parameter name",
                    ));
                }

                let parameter_list = GenericParameterList { parameters };
                GenericParameterScope::from_parameter_list(
                    &parameter_list,
                    forbidden_names,
                    string_table,
                    "Header Parsing",
                )?;
                return Ok(parameter_list);
            }

            TokenKind::Must | TokenKind::Colon => {
                return Err(generic_parameter_syntax_error(
                    "Generic parameter bounds are not supported yet.",
                    token_stream.current_location(),
                    "Remove the bound syntax and keep only the generic parameter name",
                ));
            }

            TokenKind::Newline => {
                return Err(generic_parameter_syntax_error(
                    "Generic parameter lists must stay with the declaration header.",
                    token_stream.current_location(),
                    "Keep the `type ...` parameter list before the declaration delimiter on the same header",
                ));
            }

            TokenKind::Eof | TokenKind::End => {
                return Err(generic_parameter_syntax_error(
                    "Unexpected end of declaration while parsing generic parameters.",
                    token_stream.current_location(),
                    "Finish the declaration after the generic parameter list",
                ));
            }

            other => {
                return Err(generic_parameter_syntax_error(
                    format!(
                        "Invalid generic parameter token '{other:?}'. Generic parameter names must be PascalCase or a single uppercase letter."
                    ),
                    token_stream.current_location(),
                    "Write generic parameters as names such as `T`, `Item`, or `ValueType`",
                ));
            }
        }
    }
}

fn generic_parameter_syntax_error(
    message: impl Into<String>,
    location: SourceLocation,
    suggestion: &str,
) -> CompilerError {
    let mut metadata = HashMap::new();
    metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        "Header Parsing".to_owned(),
    );
    metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion.to_owned());

    let mut error = CompilerError::new_syntax_error(message.into(), location);
    for (key, value) in metadata {
        error.new_metadata_entry(key, value);
    }
    error
}
