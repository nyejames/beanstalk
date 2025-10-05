use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::{ContextKind, ScopeContext, new_ast, AstBlock};
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::tokens::{TokenContext, TokenKind};
use crate::{ast_log, return_rule_error};
use crate::compiler::parsers::expressions::expression::Expression;
// IF STATEMENTS / MATCH STATEMENTS
// Can also be expressions (todo)
// Example:

// if x < 5:
//     print("x is less than 5")
// else:
//     print("x is greater than 5")
// ;
//
// Match statements example:
//
// if choice is:
//     0: print("Choice is 0")
//     1: print("Choice is 1")
//     else: print("Choice is 2")
// ;

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub condition: Expression,
    pub body: AstBlock,
}

pub fn create_branch(
    token_stream: &mut TokenContext,
    context: &mut ScopeContext,
) -> Result<Vec<AstNode>, CompileError> {
    let then_condition = create_expression(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition),
        &mut DataType::Bool(Ownership::default()),
        false,
    )?;

    // Check if this is a match statement rather than a regular if statement
    if token_stream.current_token_kind() == &TokenKind::Is {
        // create_expression will only NOT consume the 'is' token if it's a match statement
        token_stream.advance(); // Consume 'is'
        let match_statement = create_match_node(then_condition, token_stream, context)?;
        return Ok(vec![match_statement]);
    }

    ast_log!("Creating If Statement");
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

    // Fold evaluated if statements.
    // If the "then" condition isn't runtime,
    // The statement can be removed completely.
    if then_condition.kind.is_foldable() {
        let mut flattened_statement = then_block.ast;
        if else_block.is_some() {
            flattened_statement.push(AstNode {
                kind: NodeKind::Warning(String::from("This else block is never reached due to the if condition always being true.")),
                location: token_stream.current_location(),
                scope: if_context.scope_name,
            })
        }
        return Ok(flattened_statement)
    }

    Ok(vec![AstNode {
        kind: NodeKind::If(then_condition, then_block, else_block),
        location: token_stream.current_location(),
        scope: if_context.scope_name,
    }])
}

fn create_match_node(
    subject: Expression,
    token_stream: &mut TokenContext,
    context: &mut ScopeContext,
) -> Result<AstNode, CompileError> {
    ast_log!("Creating Match Statement");

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_rule_error!(
            token_stream.current_location(),
            "Expected ':' after the if condition to open a new scope, found '{:?}' instead",
            token_stream.current_token_kind()
        )
    }

    token_stream.advance(); // Consume ':'
    let match_context = &context.new_child_control_flow(ContextKind::Branch);

    // SYNTAX EXAMPLE:
    // if subject is:
    //     0: print("Choice is 0")
    //     1: print("Choice is 1")
    //     else: print("Choice is 2")

    // Parse each arm
    let mut arms: Vec<MatchArm> = Vec::new();
    let mut else_block = None;
    while token_stream.current_token_kind() != &TokenKind::End {

        // Check for else condition
        if token_stream.current_token_kind() == &TokenKind::Else {
            if arms.is_empty() {
                return_rule_error!(
                    token_stream.current_location(),
                    "Should be at least one condition in the match statement before the 'else' arm"
                )
            }

            if token_stream.current_token_kind() != &TokenKind::Colon {
                return_rule_error!(
                    token_stream.current_location(),
                    "Expected ':' after the else arm to open a new scope, found '{:?}' instead",
                    token_stream.current_token_kind()
                )
            }

            // Move past the colon
            token_stream.advance();

            else_block = Some(new_ast(token_stream, match_context.to_owned(), false)?.ast);
        }

        let condition = create_expression(
            token_stream,
            &match_context.new_child_control_flow(ContextKind::Condition),
            &mut DataType::Int(Ownership::default()),
            false,
        )?;

        if token_stream.current_token_kind() != &TokenKind::Colon {
            return_rule_error!(
                token_stream.current_location(),
                "Expected ':' after the match condition to open a new scope, found '{:?}' instead",
                token_stream.current_token_kind()
            )
        }

        // Move past the colon
        token_stream.advance();

        let block = new_ast(token_stream, match_context.to_owned(), false)?.ast;

        arms.push(MatchArm {
            condition,
            body: block,
        });
    }

    Ok(AstNode {
        kind: NodeKind::Match(
            subject,
            arms,
            else_block,
        ),
        location: token_stream.current_location(),
        scope: match_context.scope_name.clone(),
    })
}
