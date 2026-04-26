//! Place-sensitive expression parsing helpers.
//!
//! WHAT: parses place-sensitive expression forms such as `copy` and mutable receiver syntax.
//! WHY: place rules differ from general expression parsing and benefit from one focused module.

use super::parse_expression_dispatch::push_expression_node;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::field_access::{
    ReceiverAccessMode, parse_field_access_with_receiver_access,
};
use crate::compiler_frontend::ast::place_access::ast_node_is_place;
use crate::compiler_frontend::ast::statements::declarations::create_reference;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::{return_rule_error, return_syntax_error};

pub(super) fn parse_mutable_receiver_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    expression: &mut Vec<AstNode>,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    let marker_location = token_stream.current_location();
    token_stream.advance();

    let TokenKind::Symbol(id) = token_stream.current_token_kind().to_owned() else {
        return_rule_error!(
            "Mutable receiver marker '~' must be followed by a receiver symbol.",
            marker_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Use receiver-call syntax like '~value.method(...)'",
            }
        );
    };

    let Some(reference_arg) = context.get_reference(&id) else {
        if context.is_visible_type_alias_name(id) {
            return_rule_error!(
                format!(
                    "`{}` is a type alias and cannot be used as a receiver.",
                    string_table.resolve(id)
                ),
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use the type alias only in type annotations, not in expressions",
                }
            );
        }
        return_rule_error!(
            format!(
                "Undefined variable '{}'. Mutable receiver calls require a declared receiver place.",
                string_table.resolve(id)
            ),
            token_stream.current_location(),
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Declare this receiver variable before using '~receiver.method(...)'",
            }
        );
    };

    if token_stream.peek_next_token() != Some(&TokenKind::Dot) {
        return_rule_error!(
            "Mutable receiver marker '~' is only valid for receiver calls like '~value.method(...)' or '~values.push(...)'.",
            marker_location,
            {
                CompilationStage => "Expression Parsing",
                PrimarySuggestion => "Apply '~' directly to a receiver call",
            }
        );
    }

    token_stream.advance();
    let receiver_node = parse_field_access_with_receiver_access(
        token_stream,
        reference_arg,
        context,
        ReceiverAccessMode::Mutable,
        string_table,
    )?;
    push_expression_node(
        token_stream,
        context,
        string_table,
        expression,
        receiver_node,
    )
}

pub(super) fn parse_copy_place_expression(
    token_stream: &mut FileTokens,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // `copy` only accepts places because the backend clones the current stored value, not an
    // arbitrary temporary expression result.
    match token_stream.current_token_kind() {
        TokenKind::OpenParenthesis => {
            let open_location = token_stream.current_location();
            token_stream.advance();

            let place = parse_copy_place_expression(token_stream, context, string_table)?;
            if token_stream.current_token_kind() != &TokenKind::CloseParenthesis {
                return_syntax_error!(
                    "Expected ')' after copy operand",
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Wrap only a single place expression in parentheses after 'copy'",
                    }
                );
            }

            token_stream.advance();
            Ok(AstNode {
                location: open_location,
                ..place
            })
        }

        TokenKind::Symbol(symbol) => {
            let Some(reference_arg) = context.get_reference(symbol) else {
                if context.is_visible_type_alias_name(*symbol) {
                    return_rule_error!(
                        format!(
                            "`{}` is a type alias and cannot be copied.",
                            string_table.resolve(*symbol)
                        ),
                        token_stream.current_location(),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Use the type alias only in type annotations, not in expressions",
                        }
                    );
                }
                return_rule_error!(
                    format!(
                        "Undefined variable '{}'. Explicit copies require a declared place.",
                        string_table.resolve(*symbol)
                    ),
                    token_stream.current_location(),
                    {
                        CompilationStage => "Expression Parsing",
                        PrimarySuggestion => "Declare the variable before using 'copy'",
                    }
                );
            };

            match &reference_arg.value.data_type {
                DataType::Function(_, _) => {
                    return_rule_error!(
                        "The 'copy' keyword only accepts places, not function values or calls",
                        token_stream.current_location(),
                        {
                            CompilationStage => "Expression Parsing",
                            PrimarySuggestion => "Copy a variable or field, not a function symbol",
                        }
                    );
                }

                _ => {
                    let place =
                        create_reference(token_stream, reference_arg, context, string_table)?;
                    if !ast_node_is_place(&place) {
                        return_rule_error!(
                            "The 'copy' keyword only accepts a place expression",
                            token_stream.current_location(),
                            {
                                CompilationStage => "Expression Parsing",
                                PrimarySuggestion => "Use 'copy' before a variable or field access such as 'copy value' or 'copy user.name'",
                            }
                        );
                    }

                    Ok(place)
                }
            }
        }

        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "Expression Parsing",
                "copy-place parsing",
            )?;

            Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "Expression Parsing",
                "Use a normal place expression until traits are implemented",
            ))
        }

        _ => {
            return_syntax_error!(
                "The 'copy' keyword only accepts a place expression",
                token_stream.current_location(),
                {
                    CompilationStage => "Expression Parsing",
                    PrimarySuggestion => "Use 'copy' before a variable or field access such as 'copy value' or 'copy user.name'",
                }
            )
        }
    }
}
