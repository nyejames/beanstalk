use crate::compiler::compiler_errors::CompilerError;
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::compiler::datatypes::{DataType, Ownership};
use crate::compiler::parsers::ast::{ContextKind, ScopeContext};
use crate::compiler::parsers::ast_nodes::{AstNode, NodeKind};
use crate::compiler::parsers::build_ast::function_body_to_ast;
use crate::compiler::parsers::expressions::expression::Expression;
use crate::compiler::parsers::expressions::parse_expression::create_expression;
use crate::compiler::parsers::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler::string_interning::StringTable;
use crate::{ast_log, return_rule_error};
// IF STATEMENTS / MATCH STATEMENTS
// Possibly will be expressions in the future too?
// Example:

// if x < 5:
//     host_io_functions("x is less than 5")
// else
//     host_io_functions("x is greater than 5")
// ;
//
// Match statements example:
//
// if choice is:
//     0: host_io_functions("Choice is 0");
//     1: host_io_functions("Choice is 1");
//     else: host_io_functions("Choice is 2");
// ;

#[derive(Debug, Clone)]
pub struct MatchArm {
    pub condition: Expression,
    pub body: Vec<AstNode>,
}

pub fn create_branch(
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<Vec<AstNode>, CompilerError> {
    let then_condition = create_expression(
        token_stream,
        &context.new_child_control_flow(ContextKind::Condition),
        &mut DataType::Bool,
        &Ownership::ImmutableOwned,
        false,
        string_table,
    )?;

    // Check if this is a match statement rather than a regular if statement
    if token_stream.current_token_kind() == &TokenKind::Is {
        // create_expression will only NOT consume the 'is' token if it's a match statement
        token_stream.advance(); // Consume 'is'
        let match_statement = create_match_node(
            then_condition,
            token_stream,
            context,
            warnings,
            string_table,
        )?;
        return Ok(vec![match_statement]);
    }

    ast_log!("Creating If Statement");
    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_rule_error!(
            format!(
                "Expected ':' after the if condition to open a new scope, found '{:?}' instead",
                token_stream.current_token_kind()
            ),
            token_stream.current_location().to_error_location(&string_table),
            {
                CompilationStage => "If Statement Parsing",
                PrimarySuggestion => "Add ':' after the if condition to open the if body",
                SuggestedInsertion => ":",
            }
        )
    }

    token_stream.advance(); // Consume ':'
    let if_context = context.new_child_control_flow(ContextKind::Branch);
    let then_block =
        function_body_to_ast(token_stream, if_context.to_owned(), warnings, string_table)?;

    // Check for else condition
    let else_block = if token_stream.current_token_kind() == &TokenKind::Else {
        token_stream.advance();
        Some(function_body_to_ast(
            token_stream,
            if_context.to_owned(),
            warnings,
            string_table,
        )?)
    } else {
        None
    };

    // Fold evaluated if statements.
    // If the "then" condition isn't runtime,
    // The statement can be removed completely.
    if then_condition.kind.is_foldable() {
        let mut flattened_statement = then_block;
        if else_block.is_some() {
            flattened_statement.push(AstNode {
                kind: NodeKind::Warning(String::from(
                    "This else block is never reached due to the if condition always being true.",
                )),
                location: token_stream.current_location(),
                scope: if_context.scope,
            })
        }
        return Ok(flattened_statement);
    }

    Ok(vec![AstNode {
        kind: NodeKind::If(then_condition, then_block, else_block),
        location: token_stream.current_location(),
        scope: if_context.scope,
    }])
}

fn create_match_node(
    subject: Expression,
    token_stream: &mut FileTokens,
    context: &mut ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    ast_log!("Creating Match Statement");

    if token_stream.current_token_kind() != &TokenKind::Colon {
        return_rule_error!(
            format!(
                "Expected ':' after the if condition to open a new scope, found '{:?}' instead",
                token_stream.current_token_kind()
            ),
            token_stream.current_location().to_error_location(string_table),
            {
                CompilationStage => "Match Statement Parsing",
                PrimarySuggestion => "Add ':' after 'is' to open the match body",
                SuggestedInsertion => ":",
            }
        )
    }

    token_stream.advance(); // Consume ':'
    let match_context = context.new_child_control_flow(ContextKind::Branch);

    // SYNTAX EXAMPLE
    // Each branch MUST have an open and closed block
    // This is because every
    // if subject is:
    //     0: host_io_functions("Choice is 0");
    //     1: host_io_functions("Choice is 1");
    //     else: host_io_functions("Choice is 2");
    // ;

    // Parse each arm
    let mut arms: Vec<MatchArm> = Vec::new();
    let mut else_block = None;
    loop {
        // Check for else condition
        if token_stream.current_token_kind() == &TokenKind::Else {
            if arms.is_empty() {
                return_rule_error!(
                    "Should be at least one condition in the match statement before the 'else' arm",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => "Add at least one match arm before the 'else' arm",
                    }
                )
            }

            if token_stream.current_token_kind() != &TokenKind::Colon {
                return_rule_error!(
                    format!(
                        "Expected ':' after the else arm to open a new scope, found '{:?}' instead",
                        token_stream.current_token_kind()
                    ),
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => "Add ':' after 'else' to open the else body",
                        SuggestedInsertion => ":",
                    }
                )
            }

            // Move past the colon
            token_stream.advance();

            else_block = Some(function_body_to_ast(
                token_stream,
                match_context.to_owned(),
                warnings,
                string_table,
            )?);

            continue;
        }

        let condition = create_expression(
            token_stream,
            &match_context.new_child_control_flow(ContextKind::Condition),
            &mut DataType::Int,
            &Ownership::ImmutableOwned,
            false,
            string_table,
        )?;

        if token_stream.current_token_kind() != &TokenKind::Colon {
            return_rule_error!(
                format!(
                    "Expected ':' after the match condition to open a new scope, found '{:?}' instead",
                    token_stream.current_token_kind()
                ),
                token_stream.current_location().to_error_location(&string_table),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Add ':' after the match arm condition to open the arm body",
                    SuggestedInsertion => ":",
                }
            )
        }

        // Move past the colon
        token_stream.advance();

        let block = function_body_to_ast(
            token_stream,
            match_context.to_owned(),
            warnings,
            string_table,
        )?;

        arms.push(MatchArm {
            condition,
            body: block,
        });

        // Check for double semicolon to close this match statement
        if token_stream.current_token_kind() != &TokenKind::End {
            // Move past the end token
            token_stream.advance();
            break;
        }
    }

    Ok(AstNode {
        kind: NodeKind::Match(subject, arms, else_block),
        location: token_stream.current_location(),
        scope: match_context.scope,
    })
}
