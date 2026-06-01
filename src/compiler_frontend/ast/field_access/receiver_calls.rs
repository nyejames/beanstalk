//! User-defined receiver-method lookup and call parsing.
//!
//! WHAT: resolves declared receiver methods and validates call-site receiver semantics.
//! WHY: user receiver methods follow different rules than compiler-owned builtin members.

use super::MemberStepContext;
use super::receiver_access::{
    ReceiverAccessDiagnostic, ReceiverAccessRequirement, validate_receiver_access,
};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallDiagnosticContext, expectations_from_external_method,
    expectations_from_receiver_method_signature, resolve_call_arguments_typed,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments_typed;
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::receiver_methods::ReceiverMethodEntry;
use crate::compiler_frontend::ast::statements::fallible_handling::token_stream_starts_fallible_handling_suffix;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::builtins::error_type::resolve_builtin_error_type_typed;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidReceiverCallReason, InvalidResultHandlingReason,
};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::external_packages::ExternalAccessKind;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

fn lookup_receiver_method<'a>(
    context: &'a ScopeContext,
    receiver_type_id: TypeId,
    member_name: StringId,
    type_environment: &TypeEnvironment,
) -> Option<&'a ReceiverMethodEntry> {
    let receiver_key = type_environment.receiver_key_for_type_id(receiver_type_id)?;
    context.lookup_receiver_method(&receiver_key, member_name)
}

fn fallible_receiver_result_type_ids(
    success_return_type_ids: Vec<TypeId>,
    error_return_type_id: TypeId,
    type_interner: &mut AstTypeInterner<'_>,
) -> Vec<TypeId> {
    let success_type_id = match success_return_type_ids.as_slice() {
        [] => type_interner.builtins().none,
        [single] => *single,
        multiple => type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(multiple.to_vec()),
    };

    vec![type_interner.intern_fallible_carrier(success_type_id, error_return_type_id)]
}

pub(super) fn parse_receiver_method_call_typed(
    token_stream: &mut FileTokens,
    member_step_context: MemberStepContext<'_>,
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
    } = member_step_context;

    // ----------------------------
    //  Try user-defined receiver method
    // ----------------------------
    if let Some(method_entry) = lookup_receiver_method(
        scope_context,
        receiver_type_id,
        member_name,
        type_interner.environment(),
    ) {
        if receiver_node.expression_is_const_record_value()? {
            return Err(CompilerDiagnostic::invalid_receiver_call(
                InvalidReceiverCallReason::ConstStructNoRuntimeCalls,
                None,
                Some(member_name),
                member_location,
            )
            .into());
        }

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

        let method_name = string_table.resolve(member_name).to_owned();
        validate_receiver_access(
            receiver_node,
            receiver_access_mode,
            &member_location,
            ReceiverAccessRequirement {
                requires_mutable: method_entry.receiver_mutable,
                diagnostic: ReceiverAccessDiagnostic::ReceiverMethod {
                    method_name: member_name,
                },
            },
        )?;

        let raw_args =
            parse_call_arguments_typed(token_stream, scope_context, type_interner, string_table)?;
        let expectations =
            expectations_from_receiver_method_signature(&method_entry.signature.parameters[1..]);
        let type_check_context = type_interner.type_check_context();
        let args = resolve_call_arguments_typed(
            CallDiagnosticContext::receiver_method(&method_name),
            &raw_args,
            &expectations,
            member_location.clone(),
            string_table,
            type_check_context.type_environment,
            type_check_context.compatibility_cache,
        )?;
        let result_type_ids =
            if let Some(error_return_type_id) = method_entry.signature.error_return_type_id() {
                if !token_stream_starts_fallible_handling_suffix(token_stream) {
                    return Err(CompilerDiagnostic::invalid_result_handling(
                        InvalidResultHandlingReason::UnhandledErrorReturn,
                        token_stream.current_location(),
                    )
                    .into());
                }

                fallible_receiver_result_type_ids(
                    method_entry.signature.success_return_type_ids(),
                    error_return_type_id,
                    type_interner,
                )
            } else {
                if matches!(
                    token_stream.current_token_kind(),
                    TokenKind::Bang | TokenKind::Catch
                ) {
                    return Err(CompilerDiagnostic::invalid_result_handling(
                        InvalidResultHandlingReason::NotResultExpression,
                        token_stream.current_location(),
                    )
                    .into());
                }

                method_entry.signature.success_return_type_ids()
            };

        increment_ast_counter(AstCounter::PostfixReceiverNodesCopied);

        return Ok(Some(AstNode {
            kind: NodeKind::MethodCall {
                receiver: Box::new(receiver_node.to_owned()),
                method_path: method_entry.function_path.to_owned(),
                method: member_name,
                args,
                result_type_ids,
                location: member_location.clone(),
            },
            scope: scope_context.scope.to_owned(),
            location: member_location,
        }));
    }

    // ----------------------------
    //  Try external platform-package receiver method
    // ----------------------------
    let method_name_str = string_table.resolve(member_name).to_owned();
    if let Some((external_id, external_def)) = scope_context.lookup_visible_external_method(
        receiver_type_id,
        member_name,
        type_interner.environment(),
    ) {
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
        let mut args = resolve_call_arguments_typed(
            CallDiagnosticContext::receiver_method(&method_name_str),
            &raw_args,
            &expectations,
            member_location.clone(),
            string_table,
            type_check_context.type_environment,
            type_check_context.compatibility_cache,
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

        let result_type_ids = if external_def.is_fallible() {
            let Some(error_return_type_id) = error_return_type_id else {
                return Err(CompilerError::compiler_error(format!(
                    "Fallible external receiver method '{}' has no frontend-visible concrete error slot.",
                    external_def.name
                ))
                .into());
            };

            if !token_stream_starts_fallible_handling_suffix(token_stream) {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::UnhandledErrorReturn,
                    token_stream.current_location(),
                )
                .into());
            }

            fallible_receiver_result_type_ids(
                success_return_type_ids,
                error_return_type_id,
                type_interner,
            )
        } else {
            if matches!(
                token_stream.current_token_kind(),
                TokenKind::Bang | TokenKind::Catch
            ) {
                return Err(CompilerDiagnostic::invalid_result_handling(
                    InvalidResultHandlingReason::NotResultExpression,
                    token_stream.current_location(),
                )
                .into());
            }

            success_return_type_ids
        };

        return Ok(Some(AstNode {
            kind: NodeKind::HostFunctionCall {
                name: external_id,
                args,
                result_type_ids,
                location: member_location.clone(),
            },
            scope: scope_context.scope.to_owned(),
            location: member_location,
        }));
    }

    Ok(None)
}
