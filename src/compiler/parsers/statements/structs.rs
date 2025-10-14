use crate::CompileError;
use crate::compiler::datatypes::Ownership;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::return_syntax_error;

pub fn create_struct_definition(
    token_stream: &mut TokenContext,
    context: &ScopeContext,
) -> Result<Expression, CompileError> {
    // Should start at the colon,
    // move past the colon.
    token_stream.advance();

    let struct_context = context.new_parameters();
    let arguments = parse_multiple_args(token_stream, struct_context, &TokenKind::End, &mut true)?;

    Ok(Expression::structure(
        arguments,
        token_stream.current_location(),
        Ownership::ImmutableOwned,
    ))
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
            token_kind if &token_kind == closing_token => {
                token_stream.advance();
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
