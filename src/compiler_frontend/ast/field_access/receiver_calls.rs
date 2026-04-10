//! User-defined receiver-method lookup and call parsing.
//!
//! WHAT: resolves declared receiver methods and validates call-site receiver semantics.
//! WHY: user receiver methods follow different rules than compiler-owned builtin members.

use super::{MemberStepContext, ReceiverAccessMode};
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_receiver_method_signature, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments;
use crate::compiler_frontend::ast::place_access::{
    ast_node_is_mutable_place, ast_node_is_place, receiver_access_hint,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, ReceiverKey};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;

fn lookup_receiver_method<'a>(
    context: &'a ScopeContext,
    receiver_type: &DataType,
    member_name: StringId,
) -> Option<&'a crate::compiler_frontend::ast::receiver_methods::ReceiverMethodEntry> {
    if receiver_type.is_const_record_struct() {
        receiver_type
            .struct_nominal_path()
            .map(|path| ReceiverKey::Struct(path.to_owned()))
            .as_ref()
            .and_then(|receiver| context.lookup_receiver_method(receiver, member_name))
    } else {
        receiver_type
            .receiver_key_from_type()
            .as_ref()
            .and_then(|receiver| context.lookup_receiver_method(receiver, member_name))
    }
}

/// Parses a receiver-method call when the member name resolves to a declared receiver method.
///
/// Returns `None` when no declared receiver method matches the current receiver/member pair.
pub(super) fn parse_receiver_method_call(
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

    let context = scope_context;
    let Some(method_entry) = lookup_receiver_method(context, receiver_type, member_name) else {
        return Ok(None);
    };

    if receiver_type.is_const_record_struct() {
        return_rule_error!(
            format!(
                "Const struct records are data-only and do not support runtime method calls like '{}'.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call methods on a runtime struct value instead of a '#'-coerced const record",
            }
        );
    }

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return_rule_error!(
            format!(
                "'{}' is a receiver method and must be called with parentheses.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Call the method with 'value.method(...)'",
            }
        );
    }

    token_stream.advance();

    if method_entry.receiver_mutable {
        if !ast_node_is_place(&receiver_node) {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' requires a mutable place receiver.",
                    receiver_type.display_with_table(string_table),
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Call this mutable method on a mutable variable or mutable field path, not on a temporary expression",
                }
            );
        }

        if !ast_node_is_mutable_place(&receiver_node) {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' requires a mutable place receiver.",
                    receiver_type.display_with_table(string_table),
                    string_table.resolve(member_name)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Use a mutable receiver place for this mutable receiver call",
                }
            );
        }

        if receiver_access_mode == ReceiverAccessMode::Shared {
            return_rule_error!(
                format!(
                    "Mutable receiver method '{}.{}(...)' expects mutable access at the receiver call site. Call this with `~{}`.",
                    receiver_type.display_with_table(string_table),
                    string_table.resolve(member_name),
                    receiver_access_hint(&receiver_node, string_table)
                ),
                member_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Prefix the receiver with '~' when calling mutable receiver methods",
                }
            );
        }
    } else if receiver_access_mode == ReceiverAccessMode::Mutable {
        return_rule_error!(
            format!(
                "Receiver method '{}.{}(...)' does not accept explicit mutable access marker '~'.",
                receiver_type.display_with_table(string_table),
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Remove '~' from this receiver call",
            }
        );
    }

    let raw_args = parse_call_arguments(token_stream, context, string_table)?;
    let expectations =
        expectations_from_receiver_method_signature(&method_entry.signature.parameters[1..]);
    let method_name = string_table.resolve(member_name).to_owned();
    let args = resolve_call_arguments(
        CallDiagnosticContext::receiver_method(&method_name),
        &raw_args,
        &expectations,
        member_location.clone(),
        string_table,
    )?;
    let result_types = method_entry.signature.return_data_types();

    Ok(Some(AstNode {
        kind: NodeKind::MethodCall {
            receiver: Box::new(receiver_node),
            method_path: method_entry.function_path.to_owned(),
            method: member_name,
            builtin: None,
            args,
            result_types,
            location: member_location.clone(),
        },
        scope: context.scope.to_owned(),
        location: member_location,
    }))
}
