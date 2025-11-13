use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::return_syntax_error;
use crate::{CompileError, ast_log};
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::string_interning::{InternedString, StringTable};

// Currently only ever called from build_ast
// Since structs can only exist in function bodies or at the top level of a file.as
pub fn create_struct_definition(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Vec<Arg>, CompileError> {
    // Should start at the Colon
    // Need to skip it,
    token_stream.advance();

    let arguments = parse_parameters(token_stream, context, &mut true, string_table, true)?;

    // Skip the Parameters token
    token_stream.advance();

    Ok(arguments)
}

pub fn parse_parameters(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    pure: &mut bool,
    string_table: &mut StringTable,
    is_definition: bool,
) -> Result<Vec<Arg>, CompileError> {
    let mut args: Vec<Arg> = Vec::with_capacity(1);
    let mut next_in_list: bool = true;

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            // Return the args if the closing token is found
            // Don't skip the closing token
            TokenKind::TypeParameterBracket => {
                if !is_definition {
                    return Ok(args);
                }

                if !next_in_list {
                    return_syntax_error!(
                        "Should have a comma to separate arguments",
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Struct/Parameter Parsing",
                            PrimarySuggestion => "Add ',' between struct fields or function parameters",
                            SuggestedInsertion => ",",
                        }
                    )
                }

                // TODO: new constructor override?
            }

            TokenKind::End => {
                if is_definition {
                    return Ok(args)
                }
                return_syntax_error!(
                    "Unexpected end to this scope while parsing function parameters",
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        CompilationStage => "Struct/Parameter Parsing",
                        PrimarySuggestion => "Add closing bracket '|' for function parameters",
                        SuggestedInsertion => "|",
                    }
                )
            }

            TokenKind::Symbol(arg_name) => {
                if !next_in_list {
                    return_syntax_error!(
                        "Should have a comma to separate arguments",
                        token_stream.current_location().to_error_location(&string_table),
                        {
                            CompilationStage => "Struct/Parameter Parsing",
                            PrimarySuggestion => "Add ',' between struct fields or function parameters",
                            SuggestedInsertion => ",",
                        }
                    )
                }

                // Create a new variable
                // TODO: This needs to be updated to use string table when available
                let argument = new_parameter(token_stream, arg_name, &context, string_table)?;

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

            // If the EOF is encountered, give an error that a closing token is missing
            TokenKind::Eof => {
                return_syntax_error!(
                    "Unexpected end of file. Type definition is missing a closing bracket. Expected: '|'",
                    token_stream.current_location().to_error_location(&string_table),
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
                    token_stream.current_location().to_error_location(&string_table),
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

// The declaration syntax for parameters in function signatures or structs
// Differences to regular Arg:
// 1. They MUST have a type declaration
// 2. The assigned values (default values) are optional and must be constants if assigned
pub fn new_parameter(
    token_stream: &mut FileTokens,
    name: InternedString,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<Arg, CompileError> {
    // Move past the name
    token_stream.advance();

    let mut ownership = Ownership::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        ownership = Ownership::MutableOwned;
    };

    // Get the type declaration (REQUIRED FOR PARAMETERS)
    let mut data_type: DataType;
    match token_stream.current_token_kind() {
        // Has a type declaration
        TokenKind::DatatypeInt => data_type = DataType::Int,
        TokenKind::DatatypeFloat => data_type = DataType::Float,
        TokenKind::DatatypeBool => data_type = DataType::Bool,
        TokenKind::DatatypeString => data_type = DataType::String,

        // Collection Type Declaration
        TokenKind::OpenCurly => {
            token_stream.advance();

            // Check if there is a type inside the curly braces
            data_type = match token_stream.current_token_kind().to_datatype() {
                Some(data_type) => DataType::Collection(Box::new(data_type), ownership.to_owned()),
                _ => DataType::Collection(Box::new(DataType::Inferred), Ownership::MutableOwned),
            };

            // Make sure there is a closing curly brace
            if token_stream.current_token_kind() != &TokenKind::CloseCurly {
                return_syntax_error!(
                    "Missing closing curly brace for collection type declaration",
                    token_stream.current_location().to_error_location(&string_table),
                    {
                        CompilationStage => "Parameter Type Parsing",
                        PrimarySuggestion => "Add '}' to close the collection type declaration",
                        SuggestedInsertion => "}",
                    }
                )
            }
        }

        TokenKind::Newline => {
            data_type = DataType::Inferred;
            // Ignore
        }

        // Anything else is a syntax error
        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?} after parameter name for {}. Expected a type declaration.",
                    token_stream.tokens[token_stream.index].kind,
                    string_table.resolve(name)
                ),
                token_stream.current_location().to_error_location(&string_table),
                {
                    CompilationStage => "Parameter Type Parsing",
                    PrimarySuggestion => "Add a type declaration (Int, String, Float, Bool) after the parameter name",
                }
            )
        }
    };

    // Check for the assignment operator next
    // If this is parameters or a struct, then we can instead break out with a comma or struct close bracket
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        // If end of statement, then it's unassigned.
        // For the time being, this is a syntax error.
        // When the compiler becomes more sophisticated,
        // it will be possible to statically ensure there is an assignment on all future branches.

        // Struct bracket should only be hit here in the context of the end of some parameters
        TokenKind::Comma
        | TokenKind::Eof
        | TokenKind::Newline
        | TokenKind::TypeParameterBracket => {
            ast_log!("Created new parameter of type: {}", data_type);
            return Ok(Arg {
                id: name,
                value: Expression::none(),
            });
        }

        _ => {
            return_syntax_error!(
                format!(
                    "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(&string_table),
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
    let parsed_expr = match token_stream.current_token_kind() {
        TokenKind::OpenParenthesis => {
            token_stream.advance();
            create_expression(token_stream, context, &mut data_type, &ownership, true, string_table)?
        }
        _ => create_expression(token_stream, context, &mut data_type, &ownership, false, string_table)?,
    };

    ast_log!(
        "Created new {:?} variable of type: {}",
        ownership,
        data_type
    );

    Ok(Arg {
        id: name,
        value: parsed_expr,
    })
}
