use crate::ast_log;
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::Declaration;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_rule_error, return_syntax_error};

#[derive(Clone, Copy)]
pub enum SignatureTypeContext {
    Parameter,
    Return,
}

pub fn create_struct_definition(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<Declaration>, CompilerError> {
    // Should start at the parameter bracket
    // Need to skip it,
    token_stream.advance();

    let arguments = parse_parameters(token_stream, &mut true, string_table, true, Some(context))?;

    // Skip the Parameters token
    token_stream.advance();

    validate_struct_default_values(&arguments, string_table)?;

    Ok(arguments)
}

// Used by both functions and structs.
pub fn parse_parameters(
    token_stream: &mut FileTokens,
    pure: &mut bool,
    string_table: &mut StringTable,
    _is_const: bool, // False for function definitions, true for struct definitions
    expression_context: Option<&ScopeContext>,
) -> Result<Vec<Declaration>, CompilerError> {
    let mut args: Vec<Declaration> = Vec::with_capacity(1);
    let mut next_in_list: bool = true;

    // This should be starting after the first parameter bracket

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            // Return the args if the closing token is found
            // Don't skip the closing token
            TokenKind::TypeParameterBracket => {
                return Ok(args);
            }

            TokenKind::End => {
                return_syntax_error!(
                    "Unexpected end to this scope while parsing function parameters",
                    token_stream.current_location().to_error_location(string_table),
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
                    token_stream.current_location().to_error_location(string_table),
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
                        token_stream.current_location().to_error_location(string_table),
                        {
                            CompilationStage => "Struct/Parameter Parsing",
                            PrimarySuggestion => "Add ',' between struct fields or function parameters",
                            SuggestedInsertion => ",",
                        }
                    )
                }

                // Create a new variable
                let argument = new_parameter(
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

            TokenKind::Newline => {
                token_stream.advance();
            }

            // If the EOF is encountered, give an error that a closing token is missing
            TokenKind::Eof => {
                return_syntax_error!(
                    "Unexpected end of file. Type definition is missing a closing bracket. Expected: '|'",
                    token_stream.current_location().to_error_location(string_table),
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
                    token_stream.current_location().to_error_location(string_table),
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

pub fn parse_explicit_signature_type(
    token_stream: &mut FileTokens,
    string_table: &StringTable,
    collection_ownership: Ownership,
    context: SignatureTypeContext,
) -> Result<DataType, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            Ok(DataType::Int)
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            Ok(DataType::Float)
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            Ok(DataType::Bool)
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            Ok(DataType::StringSlice)
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
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
        TokenKind::OpenCurly => {
            let data_type = parse_collection_signature_type(
                token_stream,
                string_table,
                collection_ownership,
                context,
            )?;
            Ok(data_type)
        }
        _ => {
            let (message, stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "Expected a parameter type declaration",
                    "Parameter Type Parsing",
                    "Add a type declaration (Int, String, Float, Bool, or a collection type) after the parameter name",
                ),
                SignatureTypeContext::Return => (
                    "Expected a concrete return type",
                    "Function Signature Parsing",
                    "Use a supported return type such as Int, String, Float, Bool, or a collection type",
                ),
            };

            return_syntax_error!(
                message,
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
    }
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
            token_stream.current_location().to_error_location(string_table),
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
    string_table: &StringTable,
    context: SignatureTypeContext,
) -> Result<DataType, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::DatatypeInt => {
            token_stream.advance();
            Ok(DataType::Int)
        }
        TokenKind::DatatypeFloat => {
            token_stream.advance();
            Ok(DataType::Float)
        }
        TokenKind::DatatypeBool => {
            token_stream.advance();
            Ok(DataType::Bool)
        }
        TokenKind::DatatypeString => {
            token_stream.advance();
            Ok(DataType::StringSlice)
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
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
        _ => {
            let (message, stage, suggestion) = match context {
                SignatureTypeContext::Parameter => (
                    "Expected a collection item type declaration",
                    "Parameter Type Parsing",
                    "Use a concrete item type such as Int, String, Float, or Bool inside the collection type",
                ),
                SignatureTypeContext::Return => (
                    "Expected a collection item type in the function return",
                    "Function Signature Parsing",
                    "Use a concrete item type such as Int, String, Float, or Bool inside the collection return type",
                ),
            };

            return_syntax_error!(
                message,
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => stage,
                    PrimarySuggestion => suggestion,
                }
            )
        }
    }
}

// The declaration syntax for parameters in function signatures or structs
// Differences to regular Arg:
// 1. They MUST have a type declaration
// 2. The assigned values (default values) are optional and must be constants if assigned
pub fn new_parameter(
    token_stream: &mut FileTokens,
    full_name: InternedPath,
    expression_context: Option<&ScopeContext>,
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

    // Check for the assignment operator next
    // If this is parameters or a struct, then we can instead break out with a comma or struct close bracket

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        // If end of statement, then it's unassigned.
        // For the time being, this is a syntax error.
        // When the compiler_frontend becomes more sophisticated,
        // it will be possible to statically ensure there is an assignment on all future branches.

        // Struct bracket should only be hit here in the context of the end of some parameters
        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            ast_log!("Created new parameter of type: ", data_type);
            return Ok(Declaration {
                id: full_name,
                value: Expression::new(
                    ExpressionKind::None,
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
                token_stream.current_location().to_error_location(string_table),
                {
                    CompilationStage => "Parameter Parsing",
                    PrimarySuggestion => "Check that all referenced variables are declared before use",
                }
            )
        }
    }

    // The current token should be whatever is after the assignment operator

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away
    let parameter_context = expression_context
        .cloned()
        .unwrap_or_else(|| ScopeContext::new_constant(token_stream.src_path.to_owned()));

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
        data_type
    );

    Ok(Declaration {
        id: full_name,
        value: parsed_expr,
    })
}

fn validate_struct_default_values(
    fields: &[Declaration],
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    for field in fields {
        if matches!(field.value.kind, ExpressionKind::None) {
            continue;
        }

        if !field.value.is_compile_time_constant() {
            let field_name = field.id.name_str(string_table).unwrap_or("<field>");
            return_rule_error!(
                format!(
                    "Struct field '{}' default value must fold to a single compile-time value.",
                    field_name
                ),
                field.value.location.to_error_location(string_table), {
                    CompilationStage => "Struct/Parameter Parsing",
                    PrimarySuggestion => "Use only compile-time constants and constant expressions in struct default values",
                }
            );
        }
    }

    Ok(())
}
