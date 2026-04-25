//! Builtin error-helper receiver-member parsing.
//!
//! WHAT: parses compiler-owned error helper members (`with_location`, `push_trace`, `bubble`).
//! WHY: builtin error helpers are language policy, not user receiver-method declarations.

use super::builtin_call_args::parse_builtin_method_args;
use super::{MemberStepContext, ReceiverAccessMode};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::builtins::BuiltinMethodKind;
use crate::compiler_frontend::builtins::error_type::{
    ERROR_HELPER_BUBBLE, ERROR_HELPER_PUSH_TRACE, ERROR_HELPER_WITH_LOCATION,
    is_builtin_error_data_type, resolve_builtin_error_location_type, resolve_builtin_error_type,
    resolve_builtin_stack_frame_type,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::external_packages::{
    ERROR_BUBBLE_HOST_NAME, ERROR_PUSH_TRACE_HOST_NAME, ERROR_WITH_LOCATION_HOST_NAME,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;

#[derive(Clone, Copy, PartialEq, Eq)]
enum ErrorBuiltinMethod {
    WithLocation,
    PushTrace,
    Bubble,
}

fn error_builtin_method_name(
    member_name: StringId,
    string_table: &StringTable,
) -> Option<ErrorBuiltinMethod> {
    match string_table.resolve(member_name) {
        ERROR_HELPER_WITH_LOCATION => Some(ErrorBuiltinMethod::WithLocation),
        ERROR_HELPER_PUSH_TRACE => Some(ErrorBuiltinMethod::PushTrace),
        ERROR_HELPER_BUBBLE => Some(ErrorBuiltinMethod::Bubble),
        _ => None,
    }
}

fn error_builtin_path(builtin: ErrorBuiltinMethod, string_table: &mut StringTable) -> InternedPath {
    let builtin_name = match builtin {
        ErrorBuiltinMethod::WithLocation => ERROR_WITH_LOCATION_HOST_NAME,
        ErrorBuiltinMethod::PushTrace => ERROR_PUSH_TRACE_HOST_NAME,
        ErrorBuiltinMethod::Bubble => ERROR_BUBBLE_HOST_NAME,
    };

    InternedPath::from_single_str(builtin_name, string_table)
}

fn error_builtin_kind(builtin: ErrorBuiltinMethod) -> BuiltinMethodKind {
    match builtin {
        ErrorBuiltinMethod::WithLocation => BuiltinMethodKind::WithLocation,
        ErrorBuiltinMethod::PushTrace => BuiltinMethodKind::PushTrace,
        ErrorBuiltinMethod::Bubble => BuiltinMethodKind::Bubble,
    }
}

/// Parses one builtin error helper member in postfix position.
pub(super) fn parse_error_builtin_member(
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

    if !is_builtin_error_data_type(receiver_type, string_table) {
        return Ok(None);
    }

    let Some(builtin) = error_builtin_method_name(member_name, string_table) else {
        return Ok(None);
    };
    let member_name_text = string_table.resolve(member_name).to_owned();

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_rule_error!(
            format!(
                "Builtin error method '{}' must be called with parentheses.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call this builtin error helper with '(...)'",
            }
        );
    }

    if receiver_access_mode == ReceiverAccessMode::Mutable {
        return_rule_error!(
            format!(
                "Builtin error method '{}(...)' does not accept explicit mutable access marker '~'.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Remove '~' from this receiver call",
            }
        );
    }

    token_stream.advance();

    let error_type = resolve_builtin_error_type(scope_context, &member_location, string_table)?;
    let (args, result_types) = match builtin {
        ErrorBuiltinMethod::WithLocation => {
            let location_type =
                resolve_builtin_error_location_type(scope_context, &member_location, string_table)?;
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[location_type],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, vec![error_type])
        }
        ErrorBuiltinMethod::PushTrace => {
            let frame_type =
                resolve_builtin_stack_frame_type(scope_context, &member_location, string_table)?;
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[frame_type],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, vec![error_type])
        }
        ErrorBuiltinMethod::Bubble => {
            let args = parse_builtin_method_args(
                token_stream,
                &member_name_text,
                &[],
                scope_context,
                &member_location,
                string_table,
            )?;
            (args, vec![error_type])
        }
    };

    Ok(Some(AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(receiver_node),
            method_path: error_builtin_path(builtin, string_table),
            method: member_name,
            builtin: Some(error_builtin_kind(builtin)),
            args,
            result_types,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    }))
}
