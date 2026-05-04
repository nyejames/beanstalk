//! Collection literal parsing helpers.
//!
//! WHAT: parses `{...}` literals into `ExpressionKind::Collection` during AST construction.
//! WHY: collection parsing must share the normal expression parser for item type-checking.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;
use crate::compiler_frontend::type_coercion::diagnostics::{
    expected_found_clause, offending_value_clause,
};
use crate::compiler_frontend::type_coercion::numeric::coerce_expression_to_declared_type;
use crate::compiler_frontend::type_coercion::parse_context::parse_expectation_for_target_type;
use crate::compiler_frontend::value_mode::ValueMode;
use crate::compiler_frontend::value_mode::ValueMode::MutableOwned;
use crate::{return_syntax_error, return_type_error};

/// Parse a collection literal with homogeneous item expressions.
pub fn new_collection(
    token_stream: &mut FileTokens,
    collection_type: &DataType,
    context: &ScopeContext,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerError> {
    let mut items: Vec<Expression> = Vec::new();
    let mut inferred_inner_type = match collection_type {
        DataType::Inferred => None,
        explicit => Some(explicit.to_owned()),
    };
    let has_explicit_element_type = inferred_inner_type.is_some();
    let collection_location = token_stream.current_location();
    let mut consumed_close_curly = false;

    // The current token is an open curly brace; skip to the first value.
    token_stream.advance();

    let mut next_item: bool = true;

    while token_stream.index < token_stream.length {
        match token_stream.current_token_kind() {
            TokenKind::CloseCurly => {
                consumed_close_curly = true;
                break;
            }

            TokenKind::Newline => {
                token_stream.advance();
            }

            TokenKind::Comma => {
                if next_item {
                    return_syntax_error!(
                        "Expected a collection item after the comma",
                        token_stream.current_location()
                    )
                }

                next_item = true;
                token_stream.advance();
            }

            _ => {
                if !next_item {
                    return_syntax_error!(
                        "Expected a collection item after the comma",
                        token_stream.current_location()
                    )
                }

                let expected_item_type =
                    inferred_inner_type.as_ref().unwrap_or(&DataType::Inferred);
                let mut expr_type = parse_expectation_for_target_type(expected_item_type);
                let raw = create_expression(
                    token_stream,
                    context,
                    &mut expr_type,
                    &MutableOwned,
                    false,
                    string_table,
                )?;
                let expected_item_type =
                    inferred_inner_type.get_or_insert_with(|| raw.data_type.to_owned());

                validate_collection_item_type(
                    expected_item_type,
                    &raw,
                    has_explicit_element_type,
                    string_table,
                )?;

                let item = coerce_expression_to_declared_type(raw, expected_item_type);

                items.push(item);

                next_item = false;
            }
        }
    }

    if !consumed_close_curly {
        return_syntax_error!(
            "Unterminated collection literal. Expected '}' before end of expression.",
            token_stream.current_location()
        )
    }

    let Some(inner_type) = inferred_inner_type else {
        return_type_error!(
            "Cannot infer the element type of empty collection literal '{}'. Empty collections require an explicit collection type annotation.",
            collection_location,
            {
                CompilationStage => "Collection Literal Parsing",
                PrimarySuggestion => "Add an explicit type, for example `values ~{Int} = {}`",
            }
        )
    };

    Ok(Expression::collection(
        items,
        inner_type,
        token_stream.current_location(),
        value_mode.to_owned(),
    ))
}

fn validate_collection_item_type(
    expected: &DataType,
    item: &Expression,
    has_explicit_element_type: bool,
    string_table: &StringTable,
) -> Result<(), CompilerError> {
    if is_declaration_compatible(expected, &item.data_type) {
        return Ok(());
    }

    let message = if has_explicit_element_type {
        format!(
            "Collection literal item has incompatible type. {} {}",
            expected_found_clause(expected, &item.data_type, string_table),
            offending_value_clause(item, string_table)
        )
    } else {
        format!(
            "Collection literal has inconsistent item types. {} {}",
            expected_found_clause(expected, &item.data_type, string_table),
            offending_value_clause(item, string_table)
        )
    };

    return_type_error!(
        message,
        item.location.clone(),
        {
            CompilationStage => "Collection Literal Parsing",
            ExpectedType => expected.display_with_table(string_table),
            FoundType => item.data_type.display_with_table(string_table),
            PrimarySuggestion => if has_explicit_element_type {
                "Use values that match the declared collection element type"
            } else {
                "Use one element type in the collection literal or add an explicit collection type annotation"
            },
        }
    )
}

#[cfg(test)]
#[path = "tests/collections_tests.rs"]
mod collections_tests;
