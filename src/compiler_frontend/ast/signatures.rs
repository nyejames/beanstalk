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
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_syntax_error;

/// Distinguishes whether a type annotation appears in a parameter position or a return position.
///
/// WHAT: context tag threaded through signature-type parsing.
/// WHY: the same type-annotation grammar is used in both positions, but error messages and
/// some rules differ depending on where the annotation appears.
#[derive(Clone, Copy)]
pub enum SignatureTypeContext {
    Parameter,
    Return,
}

/// Parses a `| name [~]Type [= default], ... |` parameter/field list.
///
/// WHAT: shared parser for both function parameters and struct field declarations.
/// WHY: both syntactic forms are identical; one parser serves both.
///
/// Starts after the opening `|`. Stops when `TypeParameterBracket` (`|`) is reached, leaving
/// the stream positioned on the closing `|`.
pub fn parse_parameters(
    token_stream: &mut FileTokens,
    pure: &mut bool,
    string_table: &mut StringTable,
    _is_const: bool, // False for function definitions, true for struct definitions
    expression_context: &ScopeContext,
) -> Result<Vec<Declaration>, CompilerError> {
    let mut args: Vec<Declaration> = Vec::with_capacity(1);
    let mut next_in_list: bool = true;

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::TypeParameterBracket => {
                return Ok(args);
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
                if !next_in_list {
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

                let argument = parse_signature_member_declaration(
                    token_stream,
                    token_stream.src_path.append(arg_name),
                    expression_context,
                    string_table,
                )?;

                if argument.value.ownership.is_mutable() {
                    *pure = false;
                }

                args.push(argument);

                next_in_list = false;
            }

            TokenKind::Comma => {
                token_stream.advance();
                next_in_list = true;
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

    Ok(args)
}

/// Parses a single `name [~]Type [= default]` member declaration inside a `| ... |` list.
///
/// WHAT: the canonical parser for one parameter or struct field declaration.
/// WHY: function parameters and struct fields share this syntax; a single implementation
/// avoids drift between the two forms.
///
/// Starts with the stream positioned on the name token (already matched by the caller).
pub fn parse_signature_member_declaration(
    token_stream: &mut FileTokens,
    full_name: InternedPath,
    expression_context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Declaration, CompilerError> {
    // Move past the name
    token_stream.advance();

    let mut ownership = Ownership::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        ownership = Ownership::MutableOwned;
    };

    while token_stream.current_token_kind() == &TokenKind::Newline {
        token_stream.advance();
    }

    let mut data_type = parse_explicit_signature_type(
        token_stream,
        string_table,
        ownership.to_owned(),
        SignatureTypeContext::Parameter,
    )?;

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

/// Parses an explicit type annotation in a signature position.
///
/// WHAT: the shared type-annotation parser for `| ... |` signatures and `->` return lists.
/// WHY: both parameter and return positions share the same type-annotation grammar.
pub fn parse_explicit_signature_type(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
    collection_ownership: Ownership,
    context: SignatureTypeContext,
) -> Result<DataType, CompilerError> {
    let parsed_type = match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            DataType::Int
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            DataType::Float
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            DataType::Bool
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            DataType::StringSlice
        }
        TokenKind::DatatypeNone => {
            let (message, stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "None is not a valid parameter type",
                    "Parameter Type Parsing",
                    "Use a concrete parameter type such as Int, String, Float, Bool, or a collection type",
                ),
                SignatureTypeContext::Return => (
                    "None is not a valid function return type",
                    "Function Signature Parsing",
                    "Functions without return values should omit the return signature entirely",
                ),
            };

            return_syntax_error!(
                message,
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");
            let (stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "Parameter Type Parsing",
                    "Use a normal type name until traits are implemented",
                ),
                SignatureTypeContext::Return => (
                    "Function Signature Parsing",
                    "Use a normal return type until traits are implemented",
                ),
            };

            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                stage,
                suggestion,
            ));
        }
        TokenKind::OpenCurly => parse_collection_signature_type(
            token_stream,
            string_table,
            collection_ownership,
            context,
        )?,
        TokenKind::Symbol(type_name) => {
            let type_name = *type_name;
            token_stream.advance();
            DataType::NamedType(type_name)
        }
        _ => {
            let (message, stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "Expected a parameter type declaration",
                    "Parameter Type Parsing",
                    "Add a type declaration (Int, String, Float, Bool, a struct name, or a collection type) after the parameter name",
                ),
                SignatureTypeContext::Return => (
                    "Expected a concrete return type",
                    "Function Signature Parsing",
                    "Use a supported return type such as Int, String, Float, Bool, a struct name, or a collection type",
                ),
            };

            return_syntax_error!(
                message,
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
    };

    parse_optional_type_suffix(token_stream, parsed_type, context)
}

fn parse_collection_signature_type(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
    collection_ownership: Ownership,
    context: SignatureTypeContext,
) -> Result<DataType, CompilerError> {
    token_stream.advance();

    let inner_type = if token_stream.current_token_kind() == &TokenKind::CloseCurly {
        DataType::Inferred
    } else {
        parse_collection_inner_signature_type(token_stream, string_table, context)?
    };

    if token_stream.current_token_kind() != &TokenKind::CloseCurly {
        let stage = match context {
            SignatureTypeContext::Parameter => "Parameter Type Parsing",
            SignatureTypeContext::Return => "Function Signature Parsing",
        };

        return_syntax_error!(
            "Missing closing curly brace for collection type declaration",
            token_stream.current_location(),
            {
                CompilationStage => stage,
                PrimarySuggestion => "Add '}' to close the collection type declaration",
                SuggestedInsertion => "}",
            }
        )
    }

    token_stream.advance();

    Ok(DataType::Collection(
        Box::new(inner_type),
        collection_ownership,
    ))
}

fn parse_collection_inner_signature_type(
    token_stream: &mut FileTokens,
    _string_table: &StringTable,
    context: SignatureTypeContext,
) -> Result<DataType, CompilerError> {
    let parsed_type = match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            DataType::Int
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            DataType::Float
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            DataType::Bool
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            DataType::StringSlice
        }
        TokenKind::DatatypeNone => {
            let (message, stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "None is not a valid collection item type",
                    "Parameter Type Parsing",
                    "Use a concrete item type such as Int, String, Float, or Bool inside the collection type",
                ),
                SignatureTypeContext::Return => (
                    "None is not a valid collection item type in a function return",
                    "Function Signature Parsing",
                    "Use a concrete item type inside the collection return type",
                ),
            };

            return_syntax_error!(
                message,
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");
            let (stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "Parameter Type Parsing",
                    "Use a normal item type until traits are implemented",
                ),
                SignatureTypeContext::Return => (
                    "Function Signature Parsing",
                    "Use a normal collection item type until traits are implemented",
                ),
            };

            return Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                stage,
                suggestion,
            ));
        }
        TokenKind::Symbol(type_name) => {
            let type_name = *type_name;
            token_stream.advance();
            DataType::NamedType(type_name)
        }
        _ => {
            let (message, stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "Expected a collection item type declaration",
                    "Parameter Type Parsing",
                    "Use a concrete item type such as Int, String, Float, Bool, or a struct name inside the collection type",
                ),
                SignatureTypeContext::Return => (
                    "Expected a collection item type in the function return",
                    "Function Signature Parsing",
                    "Use a concrete item type such as Int, String, Float, Bool, or a struct name inside the collection return type",
                ),
            };

            return_syntax_error!(
                message,
                token_stream.current_location(),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
    };

    parse_optional_type_suffix(token_stream, parsed_type, context)
}

fn parse_optional_type_suffix(
    token_stream: &mut FileTokens,
    parsed_type: DataType,
    context: SignatureTypeContext,
) -> Result<DataType, CompilerError> {
    if token_stream.current_token_kind() != &TokenKind::QuestionMark {
        return Ok(parsed_type);
    }

    if matches!(parsed_type, DataType::Option(_)) {
        let stage = match context {
            SignatureTypeContext::Parameter => "Parameter Type Parsing",
            SignatureTypeContext::Return => "Function Signature Parsing",
        };
        return_syntax_error!(
            "Duplicate optional marker '?' in type declaration",
            token_stream.current_location(),
            {
                CompilationStage => stage,
                PrimarySuggestion => "Use a single '?' suffix for optional types",
            }
        );
    }

    token_stream.advance();
    Ok(DataType::Option(Box::new(parsed_type)))
}
