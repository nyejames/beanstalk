use super::{
    ast_nodes::{Arg, NodeKind},
    expressions::parse_expression::create_expression,
};
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::DataType;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::build_ast::{ScopeContext, new_ast};
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::functions::{
    create_function_signature, parse_function_call,
};
use crate::compiler::parsers::tokens::VarVisibility;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::return_syntax_error;
#[allow(unused_imports)]
use colour::{blue_ln, green_ln, red_ln};

pub fn create_reference(
    token_stream: &mut TokenContext,
    arg: &Arg,
    context: &ScopeContext,
) -> Result<AstNode, CompileError> {
    // Move past the name
    token_stream.advance();

    match arg.value.data_type {
        // Function Call
        DataType::Function(ref argument_refs, ref return_types) => parse_function_call(
            token_stream,
            &arg.name,
            context,
            argument_refs,
            return_types,
        ),

        _ => Ok(AstNode {
            kind: NodeKind::Reference(Expression::reference(
                arg.name.to_owned(),
                token_stream.current_location(),
            )),
            location: token_stream.current_location(),
            scope: context.scope.to_owned(),
        }),
    }
}

// The standard declaration syntax.
// Parses any new variable, function, type or struct argument must be structured.
// [name] [optional mutability '~'] [optional type] [assignment operator '='] [value]
pub fn new_arg(
    token_stream: &mut TokenContext,
    name: &str,
    context: &ScopeContext,
    visibility: &mut VarVisibility,
) -> Result<Arg, CompileError> {
    // Move past the name
    token_stream.advance();

    let mutable = match token_stream.current_token_kind() {
        TokenKind::Mutable => {
            token_stream.advance();
            true
        }
        _ => false,
    };

    let mut data_type = DataType::Inferred(mutable);

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        //
        TokenKind::AddAssign => {}

        TokenKind::SubtractAssign => {}

        TokenKind::MultiplyAssign => {}

        TokenKind::DivideAssign => {}

        // New Function
        TokenKind::StructDefinition => {
            let (constructor_args, return_types) =
                create_function_signature(token_stream, &mut true, &context)?;

            let context = context.new_child_function(name, &return_types);

            return Ok(Arg {
                name: name.to_owned(),
                value: Expression::function(
                    constructor_args,
                    new_ast(token_stream, context, false)?,
                    return_types,
                    token_stream.current_location(),
                ),
            });
        }

        // Function with no args
        TokenKind::Colon => {
            token_stream.advance();

            let context = context.new_child_function(name, &[]);

            return Ok(Arg {
                name: name.to_owned(),
                value: Expression::function_without_signature(
                    new_ast(token_stream, context, false)?,
                    token_stream.current_location(),
                ),
            });
        }

        // Has a type declaration
        TokenKind::DatatypeLiteral(type_keyword) => {
            data_type = type_keyword.to_owned();

            // Variables with explicit type declarations are public
            *visibility = VarVisibility::Public;

            token_stream.advance();

            match token_stream.current_token_kind() {
                TokenKind::Assign => {
                    token_stream.advance();
                }

                // If end of statement, then it's a zero-value variable
                TokenKind::Comma
                | TokenKind::EOF
                | TokenKind::Newline
                | TokenKind::StructDefinition => {
                    return Ok(Arg {
                        name: name.to_owned(),
                        value: data_type.get_zero_value(token_stream.current_location()),
                    });
                }

                _ => {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Variable of type: {:?} does not exist in this scope",
                        data_type
                    )
                }
            }
        }

        // Collection Type Declaration
        TokenKind::OpenCurly => {
            token_stream.advance();

            // Check if the datatype inside the curly braces is mutable
            let mutable = match token_stream.current_token_kind() {
                TokenKind::Mutable => {
                    token_stream.advance();
                    true
                }
                _ => false,
            };

            // Check if there is a type inside the curly braces
            data_type = match token_stream.current_token_kind().to_owned() {
                TokenKind::DatatypeLiteral(data_type) => {
                    token_stream.advance();
                    DataType::Collection(Box::new(data_type))
                }
                _ => DataType::Collection(Box::new(DataType::Inferred(mutable))),
            };

            // Make sure there is a closing curly brace
            match token_stream.current_token_kind() {
                TokenKind::CloseCurly => {
                    token_stream.advance();
                }
                _ => {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Missing closing curly brace for collection type declaration"
                    )
                }
            }

            // Should have an assignment operator now
            match token_stream.current_token_kind() {
                TokenKind::Assign => {
                    token_stream.advance();
                }

                // If end of statement, then it's a zero-value variable
                TokenKind::Comma
                | TokenKind::EOF
                | TokenKind::Newline
                | TokenKind::StructDefinition => {
                    return Ok(Arg {
                        name: name.to_owned(),
                        value: data_type.get_zero_value(token_stream.current_location()),
                    });
                }

                _ => {
                    return_syntax_error!(
                        token_stream.current_location(),
                        "Variable of type: {:?} does not exist in this scope",
                        data_type
                    )
                }
            }
        }

        TokenKind::Newline => {
            // Ignore
            token_stream.advance();
        }

        // Anything else is a syntax error
        _ => {
            return_syntax_error!(
                token_stream.current_location(),
                "Invalid operator: {:?} after variable: {}"
                token_stream.tokens[token_stream.index],
                name
            )
        }
    };

    // The current token should be whatever is after the assignment operator

    // Check if this whole expression is nested in brackets.
    // This is just so we don't wastefully call create_expression recursively right away
    let parsed_expr = match token_stream.current_token_kind() {
        TokenKind::OpenParenthesis => {
            token_stream.advance();
            create_expression(token_stream, context, &mut data_type, true)?
        }
        _ => create_expression(token_stream, context, &mut data_type, false)?,
    };

    Ok(Arg {
        name: name.to_owned(),
        value: parsed_expr,
    })
}
