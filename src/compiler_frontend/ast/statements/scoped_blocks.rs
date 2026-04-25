//! Keyword-led scoped block parsing.
//!
//! WHAT: parses ordinary `block:` statements into AST nodes with their own lexical parser scope.
//! WHY: scoped blocks are statement syntax, not symbol-led declarations or labels.

use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::{ContextKind, ScopeContext, function_body_to_ast};
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_syntax_error;

pub(crate) fn reserved_block_keyword_as_name_error(
    keyword: &str,
    location: SourceLocation,
) -> CompilerError {
    let mut error = CompilerError::new_rule_error(
        format!("'{keyword}' is a reserved keyword and cannot be used as a declaration name."),
        location,
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        String::from("AST Construction"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        format!("Choose a declaration name that is not the reserved keyword '{keyword}'."),
    );
    error
}

pub(crate) fn parse_scoped_block_statement(
    token_stream: &mut FileTokens,
    parent_context: &ScopeContext,
    warnings: &mut Vec<CompilerWarning>,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let location = token_stream.current_location();
    token_stream.advance();

    match token_stream.current_token_kind() {
        TokenKind::Colon => token_stream.advance(),
        token if token.is_assignment_operator() => {
            return Err(reserved_block_keyword_as_name_error("block", location));
        }
        TokenKind::Symbol(_) => {
            return_syntax_error!(
                "Expected ':' after 'block'. Block keywords do not take names.",
                token_stream.current_location(),
                {
                    CompilationStage => "Scoped Block Parsing",
                    PrimarySuggestion => "Write 'block:' to start an ordinary scoped block",
                    SuggestedInsertion => ":",
                }
            );
        }
        _ => {
            return_syntax_error!(
                "Expected ':' after 'block' block keyword.",
                token_stream.current_location(),
                {
                    CompilationStage => "Scoped Block Parsing",
                    PrimarySuggestion => "Write 'block:' to start an ordinary scoped block",
                    SuggestedInsertion => ":",
                }
            );
        }
    }

    let block_context = parent_context.new_child_control_flow(ContextKind::Block, string_table);
    let block_scope = block_context.scope.clone();
    let body = function_body_to_ast(token_stream, block_context, warnings, string_table)?;

    Ok(AstNode {
        kind: NodeKind::ScopedBlock { body },
        location,
        scope: block_scope,
    })
}

#[cfg(test)]
#[path = "tests/scoped_blocks_tests.rs"]
mod scoped_blocks_tests;
