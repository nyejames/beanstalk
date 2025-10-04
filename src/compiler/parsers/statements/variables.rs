use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::build_ast::{ContextKind, ScopeContext, new_ast};
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::functions::{
    create_function_signature, parse_function_call,
};
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::compiler::parsers::{
    ast_nodes::{Arg, NodeKind},
    expressions::parse_expression::create_expression,
};
use crate::return_syntax_error;


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
            // TODO: Can we actually know if this is a move or a mutable reference here?
            // Maybe it's possible to do some spooky action at a distance in the variable declarations.
            // We could just always say it's a move, then if we encounter another reference, edit the previous instance of the reference.
            // While doing this we would have to check if there was already a mutable reference (and throw an error if so)
            // If its immutable or being copied that's fine, but need to get the reference syntax working here.
            kind: NodeKind::Expression(arg.value.to_owned()),
            location: token_stream.current_location(),
            scope: context.scope_name.to_owned(),
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
) -> Result<Arg, CompileError> {
    // Move past the name
    token_stream.advance();

    let ownership = match token_stream.current_token_kind() {
        TokenKind::Mutable => {
            token_stream.advance();
            Ownership::MutableOwned(false)
        }
        _ => Ownership::ImmutableOwned(false),
    };

    let mut data_type: DataType;

    match token_stream.current_token_kind() {
        // Go straight to the assignment
        TokenKind::Assign => {
            // Cringe Code
            // This whole function can be reworked to avoid this go_back() later.
            // For now, it's easy to read and parse this way while working on the specifics of the syntax
            token_stream.go_back();
            data_type = DataType::Inferred(ownership);
        }

        // New Function
        TokenKind::StructBracket => {
            let (constructor_args, return_types) =
                create_function_signature(token_stream, &mut true, context)?;

            let context =
                context.new_child_function(name, &return_types, constructor_args.to_owned());

            // TODO: fast check for function without signature
            // let context = context.new_child_function(name, &[]);
            // return Ok(Arg {
            //     name: name.to_owned(),
            //     value: Expression::function_without_signature(
            //         new_ast(token_stream, context, false)?.ast,
            //         token_stream.current_location(),
            //     ),
            // });

            let function_body = new_ast(token_stream, context, false)?.ast;

            return Ok(Arg {
                name: name.to_owned(),
                value: Expression::function(
                    constructor_args,
                    function_body,
                    return_types,
                    token_stream.current_location(),
                ),
            });
        }

        // TODO Class / Object / Struct
        // TokenKind::Colon => {
        //     token_stream.advance();
        // }

        // Has a type declaration
        TokenKind::DatatypeInt => data_type = DataType::Int(ownership),
        TokenKind::DatatypeFloat => data_type = DataType::Float(ownership),
        TokenKind::DatatypeBool => data_type = DataType::Bool(ownership),
        TokenKind::DatatypeString => data_type = DataType::String(ownership),
        TokenKind::DatatypeStyle => data_type = DataType::Template(ownership),

        // Collection Type Declaration
        TokenKind::OpenCurly => {
            token_stream.advance();

            // Check if the datatype inside the curly braces is mutable
            let inner_ownership = match token_stream.current_token_kind() {
                TokenKind::Mutable => {
                    token_stream.advance();
                    Ownership::MutableOwned(false)
                }
                _ => Ownership::ImmutableOwned(false),
            };

            // Check if there is a type inside the curly braces
            data_type = match token_stream
                .current_token_kind()
                .to_datatype(inner_ownership)
            {
                Some(data_type) => DataType::Collection(Box::new(data_type), ownership),
                _ => DataType::Collection(
                    Box::new(DataType::Inferred(ownership)),
                    Ownership::MutableOwned(false),
                ),
            };

            // Make sure there is a closing curly brace
            if token_stream.current_token_kind() != &TokenKind::CloseCurly {
                return_syntax_error!(
                    token_stream.current_location(),
                    "Missing closing curly brace for collection type declaration"
                )
            }
        }

        TokenKind::Newline => {
            data_type = DataType::Inferred(ownership);
            // Ignore
        }

        // Anything else is a syntax error
        _ => {
            return_syntax_error!(
                token_stream.current_location(),
                "Invalid operator: {:?} after variable: {}",
                token_stream.tokens[token_stream.index],
                name
            )
        }
    };

    // Check for the assignment operator next
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Assign => {
            token_stream.advance();
        }

        // If end of statement, then it's a zero-value variable
        // Struct bracket should only be hit here in the context of the end of some parameters
        TokenKind::Comma | TokenKind::Eof | TokenKind::Newline | TokenKind::StructBracket => {
            // If this is Parameters, then instead of a zero-value, we want to return None
            if context.kind == ContextKind::Parameters {
                return Ok(Arg {
                    name: name.to_owned(),
                    value: Expression::none(),
                });
            }

            return Ok(Arg {
                name: name.to_owned(),
                value: data_type.get_zero_value(token_stream.current_location()),
            });
        }

        _ => {
            return_syntax_error!(
                token_stream.current_location(),
                "Unexpected Token: {:?}. Are you trying to reference a variable that doesn't exist yet?",
                token_stream.current_token_kind()
            )
        }
    }

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
