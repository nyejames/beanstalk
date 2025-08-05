use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::{ContextKind, ScopeContext, new_ast};
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::{ast_log, return_rule_error};

// IF STATEMENTS / MATCH STATEMENTS
// Can also be expressions (todo)
pub fn create_branch(
    token_stream: &mut TokenContext,
    context: &mut ScopeContext,
) -> Result<AstNode, CompileError> {
    ast_log!("Creating If Statement");

    let then_condition = create_expression(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition),
        &mut DataType::Bool(Ownership::default()),
        false,
    )?;

    // TODO - fold evaluated if statements
    // If this condition isn't runtime,
    // The statement can be removed completely;

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_rule_error!(
            token_stream.current_location(),
            "Expected ':' after the if condition to open a new scope, found '{:?}' instead",
            token_stream.current_token_kind()
        )
    }

    token_stream.advance(); // Consume ':'
    let if_context = context.new_child_control_flow(ContextKind::Branch);
    let then_block = new_ast(token_stream, if_context.to_owned(), false)?.ast;

    // Check for else condition
    let else_block = if token_stream.current_token_kind() == &TokenKind::Else {
        token_stream.advance();

        // Make sure there is a colon after 'else'
        if token_stream.current_token_kind() != &TokenKind::Colon {
            return_rule_error!(
                token_stream.current_location(),
                "Expected ':' after the 'else' keyword to open a new scope, found '{:?}' instead",
                token_stream.current_token_kind()
            )
        }

        token_stream.advance(); // Consume ':'
        Some(new_ast(token_stream, if_context.to_owned(), false)?.ast)
    } else {
        None
    };

    Ok(AstNode {
        kind: NodeKind::If(then_condition, then_block, else_block),
        location: token_stream.current_location(),
        scope: if_context.scope_name,
    })
}
