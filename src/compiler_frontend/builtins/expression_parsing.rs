//! Builtin expression parsing helpers.
//!
//! WHAT: parses compiler-owned expression forms such as numeric casts and collection literals.
//! WHY: builtin parsing logic should live with builtin metadata so extending language-owned
//! surfaces does not keep bloating the generic expression parser.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_with_trailing_newline_policy;
use crate::compiler_frontend::ast::statements::collections::new_collection;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_compiler_error;
use crate::{return_syntax_error, return_type_error};

/// Parses compiler-owned numeric cast forms (`Int(...)`, `Float(...)`).
///
/// WHAT: validates builtin cast syntax and lowers directly into builtin cast expressions.
/// WHY: these casts are language-owned and should not route through user call resolution.
pub(crate) fn parse_builtin_cast_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    ownership: &Ownership,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let cast_location = token_stream.current_location();
    let cast_kind = token_stream.current_token_kind().to_owned();
    token_stream.advance();

    if token_stream.current_token_kind() != &TokenKind::OpenParenthesis {
        return_syntax_error!(
            "Builtin casts require parentheses and exactly one argument.",
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use 'Int(value)' or 'Float(value)'",
            }
        );
    }

    token_stream.advance();
    if token_stream.current_token_kind() == &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Builtin casts require exactly one argument.",
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Pass one expression to the cast",
            }
        );
    }

    let mut inferred_type = DataType::Inferred;
    let value = create_expression_with_trailing_newline_policy(
        token_stream,
        context,
        &mut inferred_type,
        ownership,
        false,
        false,
        string_table,
    )?;

    if token_stream.current_token_kind() == &TokenKind::Comma {
        return_syntax_error!(
            "Builtin casts take exactly one argument.",
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Remove the extra argument",
            }
        );
    }

    if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
        return_syntax_error!(
            "Expected ')' after builtin cast argument.",
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Close the builtin cast argument list",
                SuggestedInsertion => ")",
            }
        );
    }

    token_stream.advance();

    let error_type = resolve_builtin_error_type(context, &cast_location, string_table)?;

    match cast_kind {
        TokenKind::DatatypeInt => Ok(Expression::builtin_int_cast(
            value,
            error_type,
            cast_location,
        )),
        TokenKind::DatatypeFloat => Ok(Expression::builtin_float_cast(
            value,
            error_type,
            cast_location,
        )),
        other => {
            return_compiler_error!(
                format!(
                    "Builtin cast parser dispatch mismatch: expected Int/Float token, got '{other:?}'."
                );
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "This indicates parser dispatch drift. Please report this compiler bug.",
                }
            )
        }
    }
}

/// Parses collection literal expressions (`{...}`) for declared and inferred collection types.
///
/// WHAT: validates that collection literals are used with a compatible expected type.
/// WHY: collection literals are compiler-owned syntax and should be centralized with builtin
/// parsing helpers for consistency and future extension.
pub(crate) fn parse_collection_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    data_type: &DataType,
    ownership: &Ownership,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    match data_type {
        DataType::Collection(inner_type, _) => {
            expression.push(AstNode {
                kind: NodeKind::Rvalue(new_collection(
                    token_stream,
                    inner_type,
                    context,
                    ownership,
                    string_table,
                )?),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
            Ok(())
        }

        DataType::Inferred => {
            expression.push(AstNode {
                kind: NodeKind::Rvalue(new_collection(
                    token_stream,
                    &DataType::Inferred,
                    context,
                    ownership,
                    string_table,
                )?),
                location: token_stream.current_location(),
                scope: context.scope.clone(),
            });
            Ok(())
        }

        _ => {
            return_type_error!(
                format!(
                    "Expected a collection, but assigned variable with a literal type of: {:?}",
                    data_type
                ),
                token_stream.current_location(),
                {
                    ExpectedType => "Collection",
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Change the variable type to a collection or use a different literal",
                }
            )
        }
    }
}
