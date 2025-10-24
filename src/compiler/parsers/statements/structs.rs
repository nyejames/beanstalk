use crate::CompileError;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::return_syntax_error;

// Currently only ever called from build_ast
// Since structs can only exist in function bodies or at the top level of a file.as
pub fn create_struct_definition(
    name: &str,
    token_stream: &mut TokenContext,
    context: &ScopeContext,
) -> Result<Vec<Arg>, CompileError> {
    // Should start at the colon,
    // Need to skip it,
    token_stream.advance();

    let struct_context = context.new_parameters();
    let arguments = parse_multiple_args(token_stream, struct_context, &TokenKind::End, &mut true)?;

    // Skip the End token
    token_stream.advance();

    Ok(arguments)
}

pub fn parse_multiple_args(
    token_stream: &mut TokenContext,
    context: ScopeContext,
    closing_token: &TokenKind,
    pure: &mut bool,
) -> Result<Vec<Arg>, CompileError> {
    let mut args: Vec<Arg> = Vec::with_capacity(1);
    let mut next_in_list: bool = true;

    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            // Return the args if the closing token is found
            // Don't skip the closing token
            token_kind if &token_kind == closing_token => {
                return Ok(args);
            }

            TokenKind::Symbol(arg_name, ..) => {
                if !next_in_list {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Should have a comma to separate arguments",
                    )
                }

                // Create a new variable
                let argument = new_arg(token_stream, &arg_name, &context)?;

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
                    token_stream.current_location(),
                    "Unexpected end of file. Missing closing token: {:?}",
                    closing_token
                )
            }

            _ => {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Unexpected token used in function arguments: {:?}",
                    token_stream.current_token_kind()
                )
            }
        }
    }

    Ok(args)
}
