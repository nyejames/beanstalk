//! Place-sensitive expression parsing helpers.
//!
//! WHAT: parses place-sensitive expression forms such as `copy` and mutable receiver syntax.
//! WHY: place rules differ from general expression parsing and benefit from one focused module.

use super::error::ExpressionParseError;
use super::parse_expression_dispatch::push_expression_node;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::field_access::{
    ReceiverAccessMode, parse_field_access_with_receiver_access,
};
use crate::compiler_frontend::ast::place_access::ast_node_is_place;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidCopyTargetReason, InvalidReceiverCallReason, NameNamespace,
};
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

// WHAT: parses a `~name.<chain>` receiver expression.
// WHY: mutable receiver syntax is a distinct place expression that must resolve to a field-access
//      chain so the backend can pass the receiver by mutable reference.
pub(super) fn parse_mutable_receiver_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    expression: &mut Vec<AstNode>,
    allow_boundary_catch: bool,
    string_table: &mut StringTable,
) -> Result<(), ExpressionParseError> {
    let marker_location = token_stream.current_location();
    token_stream.advance();

    let TokenKind::Symbol(symbol_id) = token_stream.current_token_kind().to_owned() else {
        return Err(CompilerDiagnostic::unexpected_token(
            token_stream.current_token_kind().to_owned(),
            marker_location,
        )
        .into());
    };

    let Some(receiver_declaration) = context.get_reference(&symbol_id) else {
        if context.is_visible_type_alias_name(symbol_id) {
            return Err(CompilerDiagnostic::namespace_misuse(
                symbol_id,
                NameNamespace::Value,
                NameNamespace::Type,
                token_stream.current_location(),
            )
            .into());
        }
        return Err(CompilerDiagnostic::unknown_value_name(
            symbol_id,
            token_stream.current_location(),
        )
        .into());
    };

    // The mutable marker must be followed by a field-access chain; bare `~name` is not valid.
    if token_stream.peek_next_token() != Some(&TokenKind::Dot) {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::MutableMarkerOnNonReceiverCall,
            None,
            None,
            marker_location,
        )
        .into());
    }

    token_stream.advance();
    let receiver_node = parse_field_access_with_receiver_access(
        token_stream,
        receiver_declaration,
        context,
        ReceiverAccessMode::Mutable,
        type_interner,
        string_table,
    )?;

    push_expression_node(
        token_stream,
        context,
        type_interner,
        string_table,
        expression,
        allow_boundary_catch,
        receiver_node,
    )
}

// WHAT: parses the operand of a `copy` expression, which must resolve to a place.
// WHY: `copy` clones the current stored value at a place; arbitrary expressions do not have
//      stable storage, so the parser restricts this to names and parenthesized places.
pub(super) fn parse_copy_place_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    match token_stream.current_token_kind() {
        // Parenthesized places are allowed for grouping; the outer `(` location is preserved
        // so diagnostics point at the `copy(` call site rather than the inner name.
        TokenKind::OpenParenthesis => {
            let open_location = token_stream.current_location();
            token_stream.advance();

            let place =
                parse_copy_place_expression(token_stream, context, type_interner, string_table)?;

            if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
                return Err(CompilerDiagnostic::expected_token(
                    TokenKind::CloseParenthesis,
                    Some(token_stream.current_token_kind().to_owned()),
                    token_stream.current_location(),
                )
                .into());
            }

            token_stream.advance();
            Ok(AstNode {
                location: open_location,
                ..place
            })
        }

        // Named places: resolve the symbol and verify it denotes a place-capable value, not a function.
        TokenKind::Symbol(symbol_id) => {
            let Some(place_declaration) = context.get_reference(symbol_id) else {
                if context.is_visible_type_alias_name(*symbol_id) {
                    return Err(CompilerDiagnostic::namespace_misuse(
                        *symbol_id,
                        NameNamespace::Value,
                        NameNamespace::Type,
                        token_stream.current_location(),
                    )
                    .into());
                }
                return Err(CompilerDiagnostic::unknown_value_name(
                    *symbol_id,
                    token_stream.current_location(),
                )
                .into());
            };

            if context
                .source_callable_signature(place_declaration)
                .is_some()
            {
                Err(CompilerDiagnostic::invalid_copy_target(
                    InvalidCopyTargetReason::FunctionValue,
                    token_stream.current_location(),
                )
                .into())
            } else {
                let place = create_reference(
                    token_stream,
                    place_declaration,
                    context,
                    type_interner,
                    string_table,
                )?;

                // create_reference may produce a non-place node in edge cases (e.g. certain
                // builtin aliases), so verify the AST shape before accepting it.
                if !ast_node_is_place(&place) {
                    return Err(CompilerDiagnostic::invalid_copy_target(
                        InvalidCopyTargetReason::NonPlace,
                        token_stream.current_location(),
                    )
                    .into());
                }

                Ok(place)
            }
        }

        // Reserved trait keywords are not valid place expressions.
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "Expression Parsing",
                "copy-place parsing",
            )?;

            Err(reserved_trait_keyword_error(keyword, token_stream.current_location()).into())
        }

        // Any other token cannot begin a place expression.
        _ => Err(CompilerDiagnostic::unexpected_token(
            token_stream.current_token_kind().to_owned(),
            token_stream.current_location(),
        )
        .into()),
    }
}
