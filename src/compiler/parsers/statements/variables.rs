use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast::ScopeContext;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::compiler::parsers::build_ast::new_ast;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::statements::functions::{FunctionSignature, parse_function_call};
use crate::compiler::parsers::statements::structs::create_struct_definition;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::parsers::{
    ast_nodes::{Arg, NodeKind},
    expressions::parse_expression::create_expression,
};
use crate::{ast_log, return_rule_error, return_syntax_error};

pub fn create_reference(
    token_stream: &mut FileTokens,
    reference_arg: &Arg,
    context: &ScopeContext,
) -> Result<AstNode, CompileError> {
    // Move past the name
    token_stream.advance();

    match reference_arg.value.data_type {
        // Function Call
        DataType::Function(ref signature) => {
            parse_function_call(token_stream, &reference_arg.name, context, signature)
        }

        _ => {
            let ownership = if reference_arg.value.ownership.is_mutable() {
                Ownership::MutableReference
            } else {
                Ownership::ImmutableReference
            };

            Ok(AstNode {
                kind: NodeKind::Expression(Expression::reference(
                    reference_arg.name.to_owned(),
                    reference_arg.value.data_type.clone(),
                    token_stream.current_location(),
                    ownership,
                )),
                location: token_stream.current_location(),
                scope: context.scope_name.to_owned(),
            })
        }
    }
}

// The standard declaration syntax.
// Parses any new variable, function, type or struct argument must be structured.
// [name] [optional mutability '~'] [optional type] [assignment operator '='] [value]
pub fn new_arg(
    token_stream: &mut FileTokens,
    name: &str,
    context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
) -> Result<Arg, CompileError> {
    // Move past the name
    token_stream.advance();

    let mut ownership = Ownership::ImmutableOwned;

    if token_stream.current_token_kind() == &TokenKind::Mutable {
        token_stream.advance();
        ownership = Ownership::MutableOwned;
    };

    let mut data_type: DataType;

    match token_stream.current_token_kind() {
        // Go straight to the assignment
        TokenKind::Assign => {
            // Cringe Code
            // This whole function can be reworked to avoid this go_back() later.
            // For now, it's easy to read and parse this way while working on the specifics of the syntax
            token_stream.go_back();
            data_type = DataType::Inferred;
        }

        TokenKind::TypeParameterBracket => {
            let func_sig = FunctionSignature::new(token_stream, &context)?;

            let func_context = context.new_child_function(name, func_sig.to_owned());

            // TODO: fast check for function without signature
            // let context = context.new_child_function(name, &[]);
            // return Ok(Arg {
            //     name: name.to_owned(),
            //     value: Expression::function_without_signature(
            //         new_ast(token_stream, context, false)?.ast,
            //         token_stream.current_location(),
            //     ),
            // });

            let function_body = new_ast(token_stream, func_context.to_owned(), warnings)?;

            return Ok(Arg {
                name: name.to_owned(),
                value: Expression::function(
                    func_sig,
                    function_body,
                    token_stream.current_location(),
                ),
            });
        }

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
                    token_stream.current_location(),
                    "Missing closing curly brace for collection type declaration"
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
                token_stream.current_location(),
                "Invalid operator: {:?} after new variable declaration: '{}'. Expect a type or assignment operator.",
                token_stream.tokens[token_stream.index].kind,
                name
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
            return_rule_error!(
                token_stream.current_location(),
                "All variables must be initialized with an assignment operator."
            )
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
        // Check if this is a struct definition (type)
        TokenKind::TypeParameterBracket => {
            let struct_def = create_struct_definition(token_stream, context)?;
            Expression::struct_definition(struct_def, token_stream.current_location(), ownership)
        }
        TokenKind::OpenParenthesis => {
            token_stream.advance();
            create_expression(token_stream, context, &mut data_type, &ownership, true)?
        }
        _ => create_expression(token_stream, context, &mut data_type, &ownership, false)?,
    };

    ast_log!("Created new variable: '{}' of type: {}", name, data_type);

    Ok(Arg {
        name: name.to_owned(),
        value: parsed_expr,
    })
}
