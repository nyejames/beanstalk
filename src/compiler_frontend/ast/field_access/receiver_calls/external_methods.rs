//! External package receiver method dispatch.
//!
//! WHAT: looks up backend-provided external receiver methods and parses the
//!       corresponding host-function call AST node.
//! WHY: external methods have their own signature expectations, access kinds,
//!      and fallible-result rules that differ from source-method dispatch.

use super::ReceiverAccessMode;
use super::shared::receiver_result_type_ids_for_call;
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallArgumentResolutionContext, CallDiagnosticContext, expectations_from_external_method,
    resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments_typed;
use crate::compiler_frontend::ast::field_access::receiver_access::{
    ReceiverAccessDiagnostic, ReceiverAccessRequirement, validate_receiver_access,
};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type_typed;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidReceiverCallReason};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::ExternalAccessKind;
use crate::compiler_frontend::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};

pub(super) struct ExternalReceiverMethodCallInput<'a, 'interner> {
    pub(super) token_stream: &'a mut FileTokens,
    pub(super) receiver_node: &'a AstNode,
    pub(super) receiver_type_id: TypeId,
    pub(super) member_name: StringId,
    pub(super) member_location: SourceLocation,
    pub(super) receiver_access_mode: ReceiverAccessMode,
    pub(super) scope_context: &'a ScopeContext,
    pub(super) type_interner: &'a mut AstTypeInterner<'interner>,
    pub(super) string_table: &'a mut StringTable,
}

pub(super) fn parse_external_receiver_method_call(
    input: ExternalReceiverMethodCallInput<'_, '_>,
) -> Result<Option<AstNode>, ExpressionParseError> {
    let ExternalReceiverMethodCallInput {
        token_stream,
        receiver_node,
        receiver_type_id,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
        type_interner,
        string_table,
    } = input;

    let method_name_str = string_table.resolve(member_name).to_owned();
    let Some((external_id, external_def)) = scope_context.lookup_visible_external_method(
        receiver_type_id,
        member_name,
        type_interner.environment(),
    ) else {
        return Ok(None);
    };

    if token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis) {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::MustUseParentheses,
            None,
            Some(member_name),
            member_location,
        )
        .into());
    }

    token_stream.advance();

    let requires_mutable = external_def.receiver_access == ExternalAccessKind::Mutable;
    validate_receiver_access(
        receiver_node,
        receiver_access_mode,
        &member_location,
        ReceiverAccessRequirement {
            requires_mutable,
            diagnostic: ReceiverAccessDiagnostic::ReceiverMethod {
                method_name: member_name,
            },
        },
    )?;

    let raw_args =
        parse_call_arguments_typed(token_stream, scope_context, type_interner, string_table)?;
    let expectations = {
        let env = type_interner.environment_mut_for_derived_types();
        expectations_from_external_method(external_def, env)
    };
    let type_check_context = type_interner.type_check_context();
    let mut args = resolve_call_arguments(
        CallDiagnosticContext::receiver_method(&method_name_str),
        &raw_args,
        &expectations,
        member_location.clone(),
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
        },
    )?;

    // Prepend the receiver as the first argument (mirrors user-method lowering).
    increment_ast_counter(AstCounter::PostfixReceiverNodesCopied);
    let receiver_expr = receiver_node.get_expr()?.to_owned();
    let receiver_access = if requires_mutable {
        CallAccessMode::Mutable
    } else {
        CallAccessMode::Shared
    };
    let receiver_arg =
        CallArgument::positional(receiver_expr, receiver_access, member_location.clone());
    args.insert(0, receiver_arg);

    let builtin_error_type =
        resolve_builtin_error_type_typed(scope_context, &member_location, string_table)?;
    let success_return_type_ids = external_def.success_return_type_ids(
        type_interner.environment_mut_for_derived_types(),
        builtin_error_type.type_id,
    );
    let error_return_type_id = external_def.error_return_type_id(
        type_interner.environment_mut_for_derived_types(),
        builtin_error_type.type_id,
    );

    let error_return_type_id = if external_def.is_fallible() {
        let Some(error_return_type_id) = error_return_type_id else {
            return Err(CompilerError::compiler_error(format!(
                "Fallible external receiver method '{}' has no frontend-visible concrete error slot.",
                external_def.name
            ))
            .into());
        };

        Some(error_return_type_id)
    } else {
        None
    };
    let result_type_ids = receiver_result_type_ids_for_call(
        success_return_type_ids,
        error_return_type_id,
        token_stream,
        type_interner,
    )?;

    Ok(Some(AstNode {
        kind: NodeKind::HostFunctionCall {
            name: external_id,
            args,
            result_type_ids,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    }))
}
