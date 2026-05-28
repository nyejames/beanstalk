//! Collection builtin receiver-member parsing.
//!
//! WHAT: parses compiler-owned collection members (`get/set/push/remove/length`).
//! WHY: collection builtin policy should stay separate from user field/method dispatch.

use super::MemberStepContext;
use super::builtin_call_args::parse_builtin_method_args_typed;
use super::receiver_access::{
    ReceiverAccessDiagnostic, ReceiverAccessRequirement, validate_receiver_access,
};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::CollectionBuiltinOp;
use crate::compiler_frontend::builtins::error_type::{
    ResolvedBuiltinType, resolve_builtin_error_type_typed,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidAssignmentTargetReason, InvalidBuiltinCallReason,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

// --------------------------
//  Constants
// --------------------------

const COLLECTION_GET_NAME: &str = "get";
const COLLECTION_SET_NAME: &str = "set";
const COLLECTION_PUSH_NAME: &str = "push";
const COLLECTION_REMOVE_NAME: &str = "remove";
const COLLECTION_LENGTH_NAME: &str = "length";

// --------------------------
//  Helpers
// --------------------------

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

fn fallible_collection_result(
    ok_type_id: crate::compiler_frontend::datatypes::ids::TypeId,
    error_type: ResolvedBuiltinType,
    type_interner: &mut AstTypeInterner<'_>,
) -> Vec<crate::compiler_frontend::datatypes::ids::TypeId> {
    // Temporary carrier bridge: collection fallibility is public `Error!` control flow, not a
    // first-class `Result` value. HIR consumes this carrier immediately while lowering the
    // handled call into explicit success/error edges.
    vec![type_interner.intern_fallible_carrier(ok_type_id, error_type.type_id)]
}

fn is_fallible_collection_builtin(builtin: CollectionBuiltinOp) -> bool {
    matches!(
        builtin,
        CollectionBuiltinOp::Get | CollectionBuiltinOp::Set | CollectionBuiltinOp::Remove
    )
}

fn token_starts_fallible_handling_suffix(token_stream: &FileTokens) -> bool {
    token_stream.current_token_kind() == &TokenKind::Bang
        || token_stream.current_token_kind() == &TokenKind::Catch
        || (matches!(token_stream.current_token_kind(), TokenKind::Symbol(_))
            && token_stream.peek_next_token() == Some(&TokenKind::Bang))
}

// --------------------------
//  Main parser
// --------------------------

pub(super) fn parse_collection_builtin_member_typed(
    token_stream: &mut FileTokens,
    context: MemberStepContext<'_>,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<Option<AstNode>, ExpressionParseError> {
    let MemberStepContext {
        receiver_node,
        receiver_type_id,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
    } = context;

    let Some(element_type_id) = type_interner
        .environment()
        .collection_element_type(receiver_type_id)
    else {
        return Ok(None);
    };

    let Some(builtin) = collection_builtin_method_name(member_name, string_table) else {
        return Ok(None);
    };
    let int_type_id = type_interner.builtins().int;
    let none_type_id = type_interner.builtins().none;
    let member_name_text = string_table.resolve(member_name).to_owned();

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::MissingParentheses,
            Some(member_name),
            member_location,
        )
        .into());
    }

    let mutating_receiver_required = matches!(
        builtin,
        CollectionBuiltinOp::Set | CollectionBuiltinOp::Push | CollectionBuiltinOp::Remove
    );

    validate_receiver_access(
        receiver_node,
        receiver_access_mode,
        &member_location,
        ReceiverAccessRequirement {
            requires_mutable: mutating_receiver_required,
            diagnostic: ReceiverAccessDiagnostic::CollectionBuiltin {
                method_name: member_name,
            },
        },
    )?;

    token_stream.advance();

    let (args, result_type_ids) = match builtin {
        CollectionBuiltinOp::Get => {
            let expected_type_ids = [int_type_id];
            let args = parse_builtin_method_args_typed(
                token_stream,
                &member_name_text,
                &expected_type_ids,
                scope_context,
                type_interner,
                &member_location,
                string_table,
            )?;
            let error_type =
                resolve_builtin_error_type_typed(scope_context, &member_location, string_table)?;
            let result_type_ids =
                fallible_collection_result(element_type_id, error_type, type_interner);
            (args, result_type_ids)
        }

        CollectionBuiltinOp::Set => {
            let expected_type_ids = [int_type_id, element_type_id];
            let args = parse_builtin_method_args_typed(
                token_stream,
                &member_name_text,
                &expected_type_ids,
                scope_context,
                type_interner,
                &member_location,
                string_table,
            )?;
            let error_type =
                resolve_builtin_error_type_typed(scope_context, &member_location, string_table)?;
            let result_type_ids =
                fallible_collection_result(none_type_id, error_type, type_interner);
            (args, result_type_ids)
        }

        CollectionBuiltinOp::Push => {
            let expected_type_ids = [element_type_id];
            let args = parse_builtin_method_args_typed(
                token_stream,
                &member_name_text,
                &expected_type_ids,
                scope_context,
                type_interner,
                &member_location,
                string_table,
            )?;
            (args, Vec::new())
        }

        CollectionBuiltinOp::Remove => {
            let expected_type_ids = [int_type_id];
            let args = parse_builtin_method_args_typed(
                token_stream,
                &member_name_text,
                &expected_type_ids,
                scope_context,
                type_interner,
                &member_location,
                string_table,
            )?;
            let error_type =
                resolve_builtin_error_type_typed(scope_context, &member_location, string_table)?;
            let result_type_ids =
                fallible_collection_result(element_type_id, error_type, type_interner);
            (args, result_type_ids)
        }

        CollectionBuiltinOp::Length => {
            let args = parse_builtin_method_args_typed(
                token_stream,
                &member_name_text,
                &[],
                scope_context,
                type_interner,
                &member_location,
                string_table,
            )?;
            (args, vec![int_type_id])
        }
    };

    if matches!(builtin, CollectionBuiltinOp::Get)
        && token_stream.current_token_kind().is_assignment_operator()
    {
        return Err(CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::CollectionIndexedWriteRemoved,
            None,
            Some(element_type_id),
            token_stream.current_location(),
        )
        .into());
    }

    // Collection `get`, `set`, and `remove` produce fallible carriers, so the parser rejects raw
    // values before HIR can mistake them for ordinary runtime data.
    if is_fallible_collection_builtin(builtin)
        && !token_starts_fallible_handling_suffix(token_stream)
    {
        return Err(CompilerDiagnostic::invalid_builtin_call(
            InvalidBuiltinCallReason::MustHandleFallibleResult,
            Some(member_name),
            token_stream.current_location(),
        )
        .into());
    }

    increment_ast_counter(AstCounter::PostfixReceiverNodesCopied);

    Ok(Some(AstNode {
        kind: NodeKind::CollectionBuiltinCall {
            receiver: Box::new(receiver_node.to_owned()),
            op: builtin,
            args,
            result_type_ids,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    }))
}
