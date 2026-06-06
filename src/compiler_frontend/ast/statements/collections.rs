//! Collection literal parsing helpers.
//!
//! WHAT: parses `{...}` literals into `ExpressionKind::Collection` during AST construction.
//! WHY: collection parsing must share the normal expression parser for item type-checking.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{
    CollectionExpressionType, Expression,
};
use crate::compiler_frontend::ast::expressions::parse_expression::create_expression_without_boundary_catch;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidCollectionTypeReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::ids::TypeId;

use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::type_coercion::contextual::coerce_expression_to_explicit_type_boundary;
use crate::compiler_frontend::type_coercion::parse_context::{
    ExpectedCollectionContext, ExpectedType, parse_expectation_for_type_id,
};
use crate::compiler_frontend::value_mode::{ValueMode, ValueMode::MutableOwned};

/// Entry point for parsing a `{...}` collection literal with homogeneous items.
pub fn new_collection(
    token_stream: &mut FileTokens,
    collection_context: ExpectedCollectionContext,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerDiagnostic> {
    parse_collection_literal(
        token_stream,
        collection_context,
        context,
        type_interner,
        value_mode,
        string_table,
    )
}

fn parse_collection_literal(
    token_stream: &mut FileTokens,
    collection_context: ExpectedCollectionContext,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    value_mode: &ValueMode,
    string_table: &mut StringTable,
) -> Result<Expression, CompilerDiagnostic> {
    let mut items: Vec<Expression> = Vec::new();
    let mut inferred_inner_type_id: Option<TypeId> = None;
    let mut explicit_collection_type_id: Option<TypeId> = None;
    let mut fixed_capacity: Option<usize> = None;
    let mut inner_type_spelling = None;
    let collection_location = token_stream.current_location();
    let mut consumed_close_curly = false;

    match collection_context {
        ExpectedCollectionContext::InferGrowable => {
            // Element type will be inferred from the first item.
        }
        ExpectedCollectionContext::Explicit {
            collection_type_id,
            element_type_id,
            fixed_capacity: capacity,
        } => {
            explicit_collection_type_id = Some(collection_type_id);
            inferred_inner_type_id = Some(element_type_id);
            fixed_capacity = capacity;
        }
        ExpectedCollectionContext::CapacityOnlyShorthand {
            fixed_capacity: capacity,
        } => {
            fixed_capacity = Some(capacity);
        }
    }

    // The current token is an open curly brace; skip to the first value.
    token_stream.advance();

    let mut awaiting_item = true;

    // ------------------------
    //  Parse collection items
    // ------------------------
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
                if awaiting_item {
                    return Err(CompilerDiagnostic::missing_collection_item(
                        token_stream.current_location(),
                    ));
                }

                awaiting_item = true;
                token_stream.advance();
            }

            _ => {
                if !awaiting_item {
                    return Err(CompilerDiagnostic::missing_collection_item(
                        token_stream.current_location(),
                    ));
                }

                let mut expression_type = match inferred_inner_type_id {
                    Some(type_id) => {
                        parse_expectation_for_type_id(type_id, type_interner.environment())
                    }
                    None => ExpectedType::Infer,
                };
                let expression_expected_types = inferred_inner_type_id
                    .map(|type_id| vec![type_id])
                    .unwrap_or_default();
                let mut expression_context =
                    context.new_child_expression(expression_expected_types);

                // Propagate the parent context kind so that constant-context
                // restrictions apply to expressions inside collection literals.
                expression_context.kind = context.kind.clone();

                let parsed_item = create_expression_without_boundary_catch(
                    token_stream,
                    &expression_context,
                    type_interner,
                    &mut expression_type,
                    &MutableOwned,
                    false,
                    string_table,
                )?;

                // Get the inferred element type from the first item if no explicit type was given.
                let item_type_id = parsed_item.type_id;
                let expected_item_type_id = inferred_inner_type_id.get_or_insert(item_type_id);

                let coerced_item = coerce_expression_to_explicit_type_boundary(
                    parsed_item,
                    *expected_item_type_id,
                    type_interner.environment(),
                    context,
                    TypeMismatchContext::CollectionElement,
                )?;

                // Capture the diagnostic type spelling from the first item
                // so the collection expression can report its element type.
                if inner_type_spelling.is_none() {
                    inner_type_spelling = Some(coerced_item.diagnostic_type.to_owned());
                }

                items.push(coerced_item);

                awaiting_item = false;
            }
        }
    }

    if !consumed_close_curly {
        return Err(CompilerDiagnostic::missing_closing_delimiter(
            string_table.get_or_intern("}".to_owned()),
            token_stream.current_location(),
        ));
    }

    let Some(inner_type_id) = inferred_inner_type_id else {
        match collection_context {
            ExpectedCollectionContext::CapacityOnlyShorthand { .. } => {
                return Err(CompilerDiagnostic::invalid_collection_type(
                    InvalidCollectionTypeReason::ShorthandEmptyLiteralAmbiguous,
                    collection_location,
                ));
            }
            _ => {
                return Err(CompilerDiagnostic::empty_collection_type_ambiguity(
                    collection_location,
                ));
            }
        }
    };

    if let Some(capacity) = fixed_capacity
        && items.len() > capacity
    {
        return Err(CompilerDiagnostic::invalid_collection_type(
            InvalidCollectionTypeReason::InitializerExceedsFixedCapacity {
                capacity,
                length: items.len(),
            },
            collection_location,
        ));
    }

    let inner_type = inner_type_spelling.unwrap_or_else(|| {
        let type_environment = type_interner.environment();
        diagnostic_type_spelling(inner_type_id, type_environment)
    });

    Ok(Expression::collection_with_type_id(
        items,
        CollectionExpressionType {
            element_type_id: inner_type_id,
            element_diagnostic_type: inner_type,
            fixed_capacity,
            collection_type_id: explicit_collection_type_id,
        },
        type_interner.environment_mut_for_derived_types(),
        token_stream.current_location(),
        value_mode.to_owned(),
    ))
}

#[cfg(test)]
#[path = "tests/collections_tests.rs"]
mod collections_tests;
