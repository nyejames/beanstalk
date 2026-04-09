//! Shared `| ... |` signature-definition parsing for functions and structs.
//!
//! WHAT: owns the shared syntax for parameter/field declarations inside `| ... |` delimiters,
//! covering name, optional `~` mutability, explicit type, and optional `= default`.
//! WHY: function signatures and struct definitions use an identical syntactic form. Centralising
//! the shared parser here keeps `statements/functions.rs` and `statements/structs.rs` free of
//! duplicated implementation and makes the shared concept visible at the module level.

use crate::ast_log;
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::identifier_policy::{
    IdentifierNamingKind, ensure_not_keyword_shadow_identifier, naming_warning_for_identifier,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_syntax::{TypeAnnotationContext, parse_type_annotation};
use crate::return_syntax_error;

/// Parses a `| name [~]Type [= default], ... |` member list.
///
/// WHAT: shared parser for both function parameters and struct field declarations.
/// WHY: both syntactic forms are identical; one parser serves both.
///
/// Starts after the opening `|`. Stops when `TypeParameterBracket` (`|`) is reached, leaving
/// the stream positioned on the closing `|`.
pub fn parse_signature_members(
    token_stream: &mut FileTokens,
    string_table: &mut StringTable,
    expression_context: &ScopeContext,
) -> Result<Vec<Declaration>, CompilerError> {
    let mut members = Vec::with_capacity(1);
    let mut expecting_member = true;

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::TypeParameterBracket => {
                return Ok(members);
            }

            TokenKind::End => {
                return_syntax_error!(
                    "Unexpected end to this scope while parsing function parameters",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Add closing bracket '|' for function parameters",
                        SuggestedInsertion => "|",
                    }
                )
            }

            TokenKind::Arrow | TokenKind::Colon => {
                return_syntax_error!(
                    "Function/struct parameters are missing a closing '|'.",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Close the parameter list with '|' before writing '->' or ':'",
                        SuggestedInsertion => "|",
                    }
                )
            }

            TokenKind::Symbol(arg_name) => {
                if !expecting_member {
                    return_syntax_error!(
                        "Should have a comma to separate arguments",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Struct/Parameter Parsing",
                            PrimarySuggestion => "Add ',' between struct fields or function parameters",
                            SuggestedInsertion => ",",
                        }
                    )
                }

                let member = parse_signature_member(
                    token_stream,
                    token_stream.src_path.append(arg_name),
                    expression_context,
                    string_table,
                )?;

                members.push(member);

                expecting_member = false;
            }

            TokenKind::Comma => {
                token_stream.advance();
                expecting_member = true;
            }

            TokenKind::Must | TokenKind::TraitThis => {
                let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                    .expect("reserved trait token should map to a keyword");

                return Err(reserved_trait_keyword_error(
                    keyword,
                    token_stream.current_location(),
                    "Struct/Parameter Parsing",
                    "Use a normal parameter or field name until traits are implemented",
                ));
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Eof => {
                return_syntax_error!(
                    "Unexpected end of file. Type definition is missing a closing bracket. Expected: '|'",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Add closing bracket '|' to complete the type definition",
                        SuggestedInsertion => "|",
                    }
                )
            }

            _ => {
                return_syntax_error!(
                    format!(
                        "Unexpected token used in function arguments: {:?}",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Use valid parameter syntax: name Type or name ~Type for mutable",
                    }
                )
            }
        }
    }

    Ok(members)
}

/// Parses a single `name [~]Type [= default]` member declaration inside a `| ... |` list.
///
/// WHAT: the canonical parser for one parameter or struct field declaration.
/// WHY: function parameters and struct fields share this syntax; a single implementation
/// avoids drift between the two forms.
///
/// Starts with the stream positioned on the name token (already matched by the caller).
fn parse_signature_member(
    token_stream: &mut FileTokens,
    full_name: InternedPath,
    expression_context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    let member_name = full_name
        .name()
        .map(|id| string_table.resolve(id).to_owned())
        .unwrap_or_else(|| String::from("<unknown>"));
    ensure_not_keyword_shadow_identifier(
        &member_name,
        token_stream.current_location(),
        "Struct/Parameter Parsing",
    )?;
    if let Some(warning) = naming_warning_for_identifier(
        &member_name,
        token_stream.current_location(),
        IdentifierNamingKind::ValueLike,
    ) {
        expression_context.emit_warning(warning);
    }

    // Move past the name.
    token_stream.advance();

    let mut ownership = Ownership::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        ownership = Ownership::MutableOwned;
    };

    while token_stream.current_token_kind() == &TokenKind::Newline {
        token_stream.advance();
    }

    let parsed_type =
        parse_type_annotation(token_stream, TypeAnnotationContext::SignatureParameter)?.data_type;
    let mut data_type = apply_collection_ownership(parsed_type, &ownership);

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            ast_log!(
                "Created new parameter of type: ",
                data_type.display_with_table(string_table)
            );
            return Ok(Declaration {
                id: full_name,
                value: Expression::new(
                    ExpressionKind::NoValue,
                    token_stream.current_location(),
                    data_type,
                    ownership,
                ),
            });
        }

        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Parameter Parsing",
                    PrimarySuggestion => "Check that all referenced variables are declared before use",
                }
            )
        }
    }

    let parameter_context = expression_context.to_owned();

    let parsed_expr = create_expression(
        token_stream,
        &parameter_context,
        &mut data_type,
        &ownership,
        false,
        string_table,
    )?;

    ast_log!(
        "Created new ",
        #ownership,
        " variable of type: ",
        data_type.display_with_table(string_table)
    );

    Ok(Declaration {
        id: full_name,
        value: parsed_expr,
    })
}

fn apply_collection_ownership(data_type: DataType, ownership: &Ownership) -> DataType {
    match data_type {
        DataType::Collection(inner, _) => DataType::Collection(
            Box::new(apply_collection_ownership(*inner, ownership)),
            ownership.to_owned(),
        ),
        DataType::Option(inner) => {
            DataType::Option(Box::new(apply_collection_ownership(*inner, ownership)))
        }
        DataType::Reference(inner) => {
            DataType::Reference(Box::new(apply_collection_ownership(*inner, ownership)))
        }
        DataType::Returns(values) => DataType::Returns(
            values
                .into_iter()
                .map(|value| apply_collection_ownership(value, ownership))
                .collect(),
        ),
        other => other,
    }
}
