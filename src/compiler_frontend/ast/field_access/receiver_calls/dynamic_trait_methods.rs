//! Dynamic trait receiver method dispatch.
//!
//! WHAT: resolves receiver methods on dynamic trait values and builds the
//!       corresponding AST node with explicit trait/requirement IDs.
//! WHY: dynamic dispatch is a distinct semantic category from static source
//!      methods and generic bounds; keeping it separate preserves the AST/HIR
//!      boundary for dynamic trait operations.

use super::ReceiverAccessMode;
use super::shared::{
    receiver_result_type_ids_for_call, requirement_receiver_is_mutable,
    signature_from_trait_requirement,
};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind};
use crate::compiler_frontend::ast::expressions::call_validation::{
    CallArgumentResolutionContext, CallDiagnosticContext,
    expectations_from_receiver_method_signature, resolve_call_arguments,
};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::function_calls::parse_call_arguments_typed;
use crate::compiler_frontend::ast::field_access::receiver_access::{
    ReceiverAccessDiagnostic, ReceiverAccessRequirement, validate_receiver_access,
};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidReceiverCallReason};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::traits::definitions::{
    ResolvedTraitDefinition, ResolvedTraitRequirement,
};

pub(super) struct DynamicTraitReceiverMethod<'a> {
    pub(super) trait_definition: &'a ResolvedTraitDefinition,
    pub(super) requirement: &'a ResolvedTraitRequirement,
    pub(super) signature: FunctionSignature,
    pub(super) receiver_mutable: bool,
}

pub(super) struct DynamicTraitMethodCallInput<'a, 'interner> {
    pub(super) token_stream: &'a mut FileTokens,
    pub(super) receiver_node: &'a AstNode,
    pub(super) member_name: StringId,
    pub(super) member_location: SourceLocation,
    pub(super) receiver_access_mode: ReceiverAccessMode,
    pub(super) scope_context: &'a ScopeContext,
    pub(super) method: DynamicTraitReceiverMethod<'a>,
    pub(super) type_interner: &'a mut AstTypeInterner<'interner>,
    pub(super) string_table: &'a mut StringTable,
}

pub(super) fn lookup_dynamic_trait_receiver_method<'a>(
    scope_context: &'a ScopeContext,
    receiver_type_id: TypeId,
    member_name: StringId,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> Option<DynamicTraitReceiverMethod<'a>> {
    let Some(TypeDefinition::DynamicTrait(dynamic_definition)) =
        type_environment.get(receiver_type_id)
    else {
        return None;
    };

    let trait_definition = scope_context
        .trait_environment()
        .get(dynamic_definition.trait_id)?;

    let requirement = trait_definition
        .requirements
        .iter()
        .find(|requirement| requirement.name == member_name)?;

    // Dynamic dispatch is requirement-based. The synthetic path is used only to build the
    // already-shared call signature shape; HIR carries the trait/requirement IDs instead.
    let synthetic_method_path = trait_definition.canonical_path.append(requirement.name);
    let receiver_mutable = requirement_receiver_is_mutable(requirement);
    let signature = signature_from_trait_requirement(
        &synthetic_method_path,
        trait_definition,
        requirement,
        receiver_type_id,
        type_environment,
        string_table,
    );

    Some(DynamicTraitReceiverMethod {
        trait_definition,
        requirement,
        signature,
        receiver_mutable,
    })
}

pub(super) fn parse_dynamic_trait_method_call_typed(
    input: DynamicTraitMethodCallInput<'_, '_>,
) -> Result<AstNode, ExpressionParseError> {
    let DynamicTraitMethodCallInput {
        token_stream,
        receiver_node,
        member_name,
        member_location,
        receiver_access_mode,
        scope_context,
        method,
        type_interner,
        string_table,
    } = input;

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

    validate_receiver_access(
        receiver_node,
        receiver_access_mode,
        &member_location,
        ReceiverAccessRequirement {
            requires_mutable: method.receiver_mutable,
            diagnostic: ReceiverAccessDiagnostic::ReceiverMethod {
                method_name: member_name,
            },
        },
    )?;

    let raw_args =
        parse_call_arguments_typed(token_stream, scope_context, type_interner, string_table)?;
    let method_name = string_table.resolve(member_name).to_owned();
    let expectations =
        expectations_from_receiver_method_signature(&method.signature.parameters[1..]);
    let type_check_context = type_interner.type_check_context();
    let args = resolve_call_arguments(
        CallDiagnosticContext::receiver_method(&method_name),
        &raw_args,
        &expectations,
        member_location.clone(),
        CallArgumentResolutionContext {
            string_table,
            type_environment: type_check_context.type_environment,
            compatibility_cache: type_check_context.compatibility_cache,
            scope_context: Some(scope_context),
        },
    )?;

    let result_type_ids = receiver_result_type_ids_for_call(
        method.signature.success_return_type_ids(),
        method.signature.error_return_type_id(),
        token_stream,
        type_interner,
    )?;

    Ok(AstNode {
        kind: NodeKind::DynamicTraitMethodCall {
            receiver: Box::new(receiver_node.to_owned()),
            trait_id: method.trait_definition.id,
            requirement_id: method.requirement.id,
            method: member_name,
            receiver_requires_mutable: method.receiver_mutable,
            args,
            result_type_ids,
            location: member_location.clone(),
        },
        scope: scope_context.scope.to_owned(),
        location: member_location,
    })
}
