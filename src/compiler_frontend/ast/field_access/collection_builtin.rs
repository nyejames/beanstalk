//! Collection builtin receiver-member parsing.
//!
//! WHAT: parses compiler-owned collection members (`get/set/push/remove/length`).
//! WHY: collection builtin policy should stay separate from user field/method dispatch.

use super::builtin_call_args::parse_builtin_method_args;
use super::{MemberStepContext, ReceiverAccessMode};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::place_access::{
    ast_node_is_mutable_place, ast_node_is_place, receiver_access_hint,
};
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;

const COLLECTION_GET_NAME: &str = "get";
const COLLECTION_SET_NAME: &str = "set";
const COLLECTION_PUSH_NAME: &str = "push";
const COLLECTION_REMOVE_NAME: &str = "remove";
const COLLECTION_LENGTH_NAME: &str = "length";

fn collection_builtin_method_name(
    member_name: StringId,
    string_table: &StringTable,
) -> Option<CollectionBuiltinOp> {
    match string_table.resolve(member_name) {
        COLLECTION_GET_NAME => Some(CollectionBuiltinOp::Get),
        COLLECTION_SET_NAME => Some(CollectionBuiltinOp::Set),
        COLLECTION_PUSH_NAME => Some(CollectionBuiltinOp::Push),
        COLLECTION_REMOVE_NAME => Some(CollectionBuiltinOp::Remove),
        COLLECTION_LENGTH_NAME => Some(CollectionBuiltinOp::Length),
        _ => None,
    }
}

/// Parses one collection builtin receiver member in postfix position.
pub(super) fn parse_collection_builtin_member(
    token_stream: &mut FileTokens,
    context: MemberStepContext<'_>,
    string_table: &mut StringTable,
) -> Result<Option<AstNode>, CompilerError> {
    let MemberStepContext {
        receiver_node,
        receiver_type,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
    } = context;

    let DataType::Collection(inner_type, _) = receiver_type else {
        return Ok(None);
    };

    let Some(builtin) = collection_builtin_method_name(member_name, string_table) else {
        return Ok(None);
    };
    let member_name_text = string_table.resolve(member_name).to_owned();

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_rule_error!(
            format!(
                "Collection method '{}' must be called with parentheses.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call this collection method with '(...)'",
            }
        );
    }

    let mutating_receiver_required = matches!(
        builtin,
        CollectionBuiltinOp::Set | CollectionBuiltinOp::Push | CollectionBuiltinOp::Remove
    );

    if receiver_access_mode == ReceiverAccessMode::Mutable && !mutating_receiver_required {
        return_rule_error!(
            format!(
                "Collection method '{}(...)' does not accept explicit mutable access marker '~'.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Remove '~' from this receiver call",
            }
        );
    }

    if mutating_receiver_required {
        if !ast_node_is_place(&receiver_node) {
            return_rule_error!(
                format!(
                    "Collection mutating method '{}(...)' requires a mutable place receiver.",
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call this method on a mutable variable or mutable field path, not on a temporary expression",
                }
            );
        }

        if !ast_node_is_mutable_place(&receiver_node) {
            return_rule_error!(
                format!(
                    "Collection mutating method '{}(...)' requires a mutable collection receiver.",
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use a mutable receiver place for this mutating collection method",
                }
            );
        }

        if receiver_access_mode == ReceiverAccessMode::Shared {
            return_rule_error!(
                format!(
                    "Collection mutating method '{}(...)' expects mutable access at the receiver call site. Call this with `~{}`.",
                    string_table.resolve(member_name),
                    receiver_access_hint(&receiver_node, string_table)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Prefix the receiver with '~' for this mutating collection call",
                }
            );
        }
    }

    token_stream.advance();

    let (args, result_types) = match builtin {
        CollectionBuiltinOp::Get => {
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[DataType::Int],
                scope_context,
                &member_location,
                string_table,
            )?;
            let error_type =
                resolve_builtin_error_type(scope_context, &member_location, string_table)?;
            let get_result_type = DataType::Result {
                ok: Box::new(inner_type.as_ref().to_owned()),
                err: Box::new(error_type),
            };
            (args, vec![get_result_type])
        }
        CollectionBuiltinOp::Set => {
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[DataType::Int, inner_type.as_ref().to_owned()],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }
        CollectionBuiltinOp::Push => {
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[inner_type.as_ref().to_owned()],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }
        CollectionBuiltinOp::Remove => {
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[DataType::Int],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }
        CollectionBuiltinOp::Length => {
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, vec![DataType::Int])
        }
    };

    if matches!(builtin, CollectionBuiltinOp::Get)
        && token_stream.current_token_kind() != &TokenKind::Bang
        && !(matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
            && token_stream.peek_next_token() == Some(&TokenKind::Bang))
        && !token_stream.current_token_kind().is_assignment_operator()
    {
        return_rule_error!(
            "Calls to collection 'get(index)' must be explicitly handled with '!' syntax.",
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use '.get(index)!' to handle/propagate errors, or assign through '.get(index) = value' for indexed writes",
            }
        );
    }

    Ok(Some(AstNode {
        kind: NodeKind::CollectionBuiltinCall {
            receiver: Box::new(receiver_node),
            op: builtin,
            args,
            result_types,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    }))
}
