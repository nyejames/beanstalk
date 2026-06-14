//! Postfix/member chain parsing implementation.
//!
//! WHAT: drives chained postfix parsing and dispatches each member step to focused handlers.
//! WHY: field access, receiver methods, and compiler-owned builtin members evolve independently,
//! so the chain driver should stay thin while policy lives in dedicated modules.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionValueShape};
use crate::compiler_frontend::ast::place_access::ast_node_is_place;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidAssignmentTargetReason, InvalidFieldAccessReason,
    InvalidReceiverCallReason,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::ids::TypeId;

use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;

use super::collection_builtin::parse_collection_builtin_member_typed;
use super::field_member::{parse_field_member_access_typed, parse_member_name_typed};
use super::map_builtin::parse_map_builtin_member_typed;
use super::receiver_calls::parse_receiver_method_call_typed;
use super::{MemberStepContext, ReceiverAccessMode};

fn receiver_reference_node(
    reference_arg: &Declaration,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    base_location: SourceLocation,
) -> AstNode {
    if context.kind.is_constant_context() {
        if reference_arg.is_unresolved_constant_placeholder() {
            let placeholder_type = context
                .expected_result_type_ids
                .first()
                .map(|type_id| diagnostic_type_spelling(*type_id, type_interner.environment()))
                .unwrap_or_else(|| reference_arg.value.diagnostic_type.to_owned());
            let placeholder_type_id = context
                .expected_result_type_ids
                .first()
                .copied()
                .unwrap_or(reference_arg.value.type_id);
            let ref_expr = Expression::reference_with_type_id(
                reference_arg.id.to_owned(),
                placeholder_type,
                placeholder_type_id,
                base_location.clone(),
                ValueMode::ImmutableOwned,
                reference_arg.value.const_record_state,
            );
            return AstNode {
                kind: NodeKind::Rvalue(ref_expr),
                location: base_location,
                scope: context.scope.clone(),
            };
        }

        let mut inlined_expression = reference_arg.value.to_owned();
        inlined_expression.value_mode = ValueMode::ImmutableOwned;
        AstNode {
            kind: NodeKind::Rvalue(inlined_expression),
            location: base_location,
            scope: context.scope.clone(),
        }
    } else {
        let mut ref_expr = Expression::reference_with_type_id(
            reference_arg.id.to_owned(),
            reference_arg.value.diagnostic_type.to_owned(),
            reference_arg.value.type_id,
            base_location.clone(),
            reference_arg.value.value_mode.to_owned(),
            reference_arg.value.const_record_state,
        );
        // Preserve explicit source shape (template, path, etc.) over the diagnostic-type
        // fallback used by the generic reference constructor.
        if reference_arg.value.value_shape != ExpressionValueShape::Ordinary {
            ref_expr.value_shape = reference_arg.value.value_shape;
        }
        if let Some(source) = reference_arg.value.reactive_source.clone() {
            ref_expr = ref_expr.with_reactive_source(source);
        }
        if let Some(template_metadata) = reference_arg.value.reactive_template.clone() {
            ref_expr = ref_expr.with_reactive_template_metadata(template_metadata);
        }

        AstNode {
            kind: NodeKind::Rvalue(ref_expr),
            scope: context.scope.to_owned(),
            location: base_location,
        }
    }
}

fn receiver_node_type_id(node: &AstNode) -> Result<TypeId, CompilerError> {
    node.expression_type_id()
}

fn type_id_is_external(type_id: TypeId, type_interner: &AstTypeInterner<'_>) -> bool {
    matches!(
        type_interner.environment().get(type_id),
        Some(TypeDefinition::External(..))
    )
}

pub(crate) fn parse_postfix_chain(
    token_stream: &mut FileTokens,
    receiver_node: AstNode,
    receiver_access_mode: ReceiverAccessMode,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    parse_postfix_chain_typed(
        token_stream,
        receiver_node,
        receiver_access_mode,
        context,
        type_interner,
        string_table,
    )
}

fn parse_postfix_chain_typed(
    token_stream: &mut FileTokens,
    mut receiver_node: AstNode,
    receiver_access_mode: ReceiverAccessMode,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    let mut encountered_receiver_call = false;

    // ----------------------------
    //  Walk postfix member-access chain
    // ----------------------------
    while token_stream.index < token_stream.length
        && token_stream.current_token_kind() == &TokenKind::Dot
    {
        token_stream.advance();

        if token_stream.index >= token_stream.length {
            let fallback_location = token_stream
                .tokens
                .last()
                .map(|token| token.location.clone())
                .unwrap_or_default();
            return Err(CompilerDiagnostic::invalid_field_access(
                InvalidFieldAccessReason::ExpectedNameAfterDot,
                None,
                None,
                fallback_location,
            )
            .into());
        }

        let member_name = parse_member_name_typed(token_stream, string_table)?;
        let receiver_type_id = receiver_node_type_id(&receiver_node)?;
        let member_location = token_stream.current_location();
        let member_context = MemberStepContext {
            receiver_node: &receiver_node,
            receiver_type_id,
            member_name,
            member_location: member_location.clone(),
            receiver_access_mode,
            scope_context: context,
        };

        if let Some(field_access) = parse_field_member_access_typed(
            token_stream,
            member_context.to_owned(),
            type_interner,
            string_table,
        )? {
            receiver_node = field_access;
            continue;
        }

        if let Some(collection_builtin_call) = parse_collection_builtin_member_typed(
            token_stream,
            member_context.to_owned(),
            type_interner,
            string_table,
        )? {
            receiver_node = collection_builtin_call;
            encountered_receiver_call = true;
            continue;
        }

        if let Some(map_builtin_call) = parse_map_builtin_member_typed(
            token_stream,
            member_context.to_owned(),
            type_interner,
            string_table,
        )? {
            receiver_node = map_builtin_call;
            encountered_receiver_call = true;
            continue;
        }

        if let Some(receiver_method_call) = parse_receiver_method_call_typed(
            token_stream,
            member_context,
            type_interner,
            string_table,
        )? {
            receiver_node = receiver_method_call;
            encountered_receiver_call = true;
            continue;
        }

        // No handler matched. Preserve the user-facing distinction between
        // deferred choice payload access, opaque externals, and ordinary
        // missing members while routing all cases through one typed diagnostic.
        let reason = if type_interner
            .environment()
            .variants_for(receiver_type_id)
            .is_some()
            && token_stream.peek_next_token() != Some(&TokenKind::OpenParenthesis)
        {
            let next_token = token_stream.peek_next_token();
            if next_token
                .map(|t| t.is_assignment_operator())
                .unwrap_or(false)
            {
                InvalidFieldAccessReason::ChoicePayloadMutation
            } else {
                InvalidFieldAccessReason::ChoicePayloadDeferred
            }
        } else if type_id_is_external(receiver_type_id, type_interner) {
            InvalidFieldAccessReason::UnknownExternalMember
        } else {
            InvalidFieldAccessReason::UnknownMember
        };

        return Err(CompilerDiagnostic::invalid_field_access(
            reason,
            Some(member_name),
            Some(receiver_type_id),
            member_location,
        )
        .into());
    }

    // ----------------------------
    //  Validate assignment receiver is a place
    // ----------------------------
    if token_stream.current_token_kind().is_assignment_operator()
        && !ast_node_is_place(&receiver_node)
    {
        let receiver_type_id = receiver_node_type_id(&receiver_node)?;
        let diagnostic = CompilerDiagnostic::invalid_assignment_target(
            InvalidAssignmentTargetReason::NotMutablePlace,
            None,
            Some(receiver_type_id),
            receiver_node.location.clone(),
        );
        return Err(diagnostic.into());
    }

    if receiver_access_mode == ReceiverAccessMode::Mutable && !encountered_receiver_call {
        return Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::MutableMarkerOnNonReceiverCall,
            None,
            None,
            receiver_node.location.clone(),
        )
        .into());
    }

    Ok(receiver_node)
}

pub fn parse_field_access(
    token_stream: &mut FileTokens,
    base_arg: &Declaration,
    context: &ScopeContext,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    parse_field_access_with_receiver_access(
        token_stream,
        base_arg,
        context,
        ReceiverAccessMode::Shared,
        type_interner,
        string_table,
    )
}

pub(crate) fn parse_field_access_with_receiver_access(
    token_stream: &mut FileTokens,
    base_arg: &Declaration,
    context: &ScopeContext,
    receiver_access_mode: ReceiverAccessMode,
    type_interner: &mut AstTypeInterner<'_>,
    string_table: &mut StringTable,
) -> Result<AstNode, ExpressionParseError> {
    let base_location = if token_stream.index > 0 {
        token_stream.tokens[token_stream.index - 1].location.clone()
    } else {
        token_stream.current_location()
    };

    parse_postfix_chain_typed(
        token_stream,
        receiver_reference_node(base_arg, context, type_interner, base_location),
        receiver_access_mode,
        context,
        type_interner,
        string_table,
    )
}
