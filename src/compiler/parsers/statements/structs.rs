use crate::compiler::compiler_errors::ErrorType;
#[allow(unused_imports)]
use colour::grey_ln;

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::Arg;
use crate::compiler::parsers::build_ast::ScopeContext;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::statements::variables::new_arg;
use crate::compiler::parsers::tokens::VarVisibility;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::return_syntax_error;

pub fn create_args(
    token_stream: &mut TokenContext,
    initial_value: Expression,
    required_args: &[Arg],
    context: &ScopeContext,
) -> Result<Vec<Arg>, CompileError> {
    let mut item_args = required_args.to_owned();

    let mut items: Vec<Arg> = if initial_value.is_none() {
        Vec::new()
    } else {
        vec![Arg {
            name: "0".to_string(),

            // TODO: Should items be able to be declared as mutable here?
            // check for mutable token before?
            value: initial_value,
        }]
    };

    let mut next_item: bool = true;
    let mut item_name: String = "0".to_string();

    // ASSUMES A '(' HAS JUST BEEN PASSED
    while token_stream.index < token_stream.tokens.len() {
        match token_stream.current_token_kind().to_owned() {
            TokenKind::CloseParenthesis => {
                token_stream.index += 1;
                break;
            }

            TokenKind::Comma => {
                if next_item {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Expected a collection item after the comma"
                    )
                }
                next_item = true;
                token_stream.advance();
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Symbol(ref name, ..) => {
                if !next_item {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Expected a comma between struct items"
                    )
                }

                let item_arg = new_arg(token_stream, name, context, &mut VarVisibility::Private)?;

                items.push(item_arg.to_owned());
                item_args.push(item_arg);
                item_name = items.len().to_string();

                next_item = false;
            }

            _ => {
                if !next_item {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Expected a comma between struct items"
                    )
                }

                next_item = false;

                let mut data_type = if required_args.is_empty() {
                    DataType::Inferred(false)
                } else if required_args.len() < items.len() {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Too many arguments provided to struct"
                    )
                } else {
                    required_args[items.len()].value.data_type.to_owned()
                };

                let arg_value = create_expression(token_stream, context, &mut data_type, false)?;

                // Get the arg of this struct item
                let item_arg = match item_args.get(items.len()) {
                    Some(arg) => arg.to_owned(),
                    None => Arg {
                        name: item_name,
                        value: arg_value,
                    },
                };

                items.push(item_arg.to_owned());
                item_args.push(item_arg);
                item_name = items.len().to_string();
            }
        }
    }

    Ok(items)
}
