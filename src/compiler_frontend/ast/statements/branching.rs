use crate::compiler_frontend::ast::ast::{ContextKind, ScopeContext};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::ast::function_body_to_ast::function_body_to_ast;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
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
        &context.new_child_control_flow(ContextKind::Condition, string_table),
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
    let then_context = context.new_child_control_flow(ContextKind::Branch, string_table);
    let then_block = function_body_to_ast(
        token_stream,
        then_context.to_owned(),
        warnings,
        string_table,
    )?;

    // Check for else condition
    let else_block = if token_stream.current_token_kind() == &TokenKind::Else {
        token_stream.advance();
        let else_context = context.new_child_control_flow(ContextKind::Branch, string_table);
        Some(function_body_to_ast(
            token_stream,
            else_context,
            warnings,
            string_table,
        )?)
    } else {
        None
    };

    Ok(vec![AstNode {
        kind: NodeKind::If(then_condition, then_block, else_block),
        location: token_stream.current_location(),
        scope: then_context.scope,
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
    let match_context = context.new_child_control_flow(ContextKind::Branch, string_table);

    // SYNTAX EXAMPLE
    // Each branch MUST have an open and closed block
    // This is because every
    // if subject is:
    //     0: host_io_functions("Choice is 0");
    //     1: host_io_functions("Choice is 1");
    //     else: host_io_functions("Choice is 2");
    // ;

    let mut arms: Vec<MatchArm> = Vec::new();
    let mut else_block = None;
    let mut seen_else = false;

    loop {
        token_stream.skip_newlines();

        match token_stream.current_token_kind() {
            TokenKind::End => {
                token_stream.advance();
                break;
            }
            TokenKind::Else => {
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

                if seen_else {
                    return_rule_error!(
                        "Match statement can only have one 'else' arm",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Remove duplicate else arms",
                        }
                    )
                }
                seen_else = true;

                token_stream.advance();
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

                token_stream.advance();

                else_block = Some(function_body_to_ast(
                    token_stream,
                    match_context.new_child_control_flow(ContextKind::Branch, string_table),
                    warnings,
                    string_table,
                )?);
            }
            TokenKind::Eof => {
                return_rule_error!(
                    "Unexpected end of file in match statement",
                    token_stream.current_location().to_error_location(string_table),
                    {
                        CompilationStage => "Match Statement Parsing",
                        PrimarySuggestion => "Terminate this match statement with ';'",
                        SuggestedInsertion => ";",
                    }
                )
            }
            _ => {
                if seen_else {
                    return_rule_error!(
                        "Match arms cannot appear after an 'else' arm",
                        token_stream.current_location().to_error_location(string_table),
                        {
                            CompilationStage => "Match Statement Parsing",
                            PrimarySuggestion => "Move this arm before the else arm",
                        }
                    )
                }

                let condition = create_expression(
                    token_stream,
                    &match_context.new_child_control_flow(ContextKind::Condition, string_table),
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

                token_stream.advance();

                let block = function_body_to_ast(
                    token_stream,
                    match_context.new_child_control_flow(ContextKind::Branch, string_table),
                    warnings,
                    string_table,
                )?;

                arms.push(MatchArm {
                    condition,
                    body: block,
                });
            }
        }
    }

    Ok(AstNode {
        kind: NodeKind::Match(subject, arms, else_block),
        location: token_stream.current_location(),
        scope: match_context.scope,
    })
}
