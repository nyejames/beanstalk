//! Postfix/member parsing coordinator.
//!
//! WHAT: drives chained postfix parsing and dispatches each member step to focused handlers.
//! WHY: field access, receiver methods, and compiler-owned builtin members evolve independently,
//! so the chain driver should stay thin while policy lives in dedicated modules.

mod builtin_call_args;
mod collection_builtin;
mod error_builtin;
mod field_member;
mod receiver_calls;

use self::collection_builtin::parse_collection_builtin_member;
use self::error_builtin::parse_error_builtin_member;
use self::field_member::{parse_field_member_access, parse_member_name};
use self::receiver_calls::parse_receiver_method_call;
use crate::compiler_frontend::ast::ast::ScopeContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::place_access::ast_node_is_place;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, SourceLocation, TokenKind};
use crate::return_rule_error;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum ReceiverAccessMode {
    Shared,
    Mutable,
}

/// Shared parse state for one postfix member step.
///
/// WHAT: carries the receiver expression/type plus current member token metadata.
/// WHY: each member handler needs the same context, and this keeps helper signatures compact.
#[derive(Clone)]
pub(super) struct MemberStepContext<'a> {
    pub receiver_node: AstNode,
    pub receiver_type: &'a DataType,
    pub member_name: StringId,
    pub member_location: SourceLocation,
    pub receiver_access_mode: ReceiverAccessMode,
    pub scope_context: &'a ScopeContext,
}

fn receiver_reference_node(
    reference_arg: &Declaration,
    context: &ScopeContext,
    base_location: SourceLocation,
) -> AstNode {
    if context.kind.is_constant_context() {
        let mut inlined_expression = reference_arg.value.to_owned();
        inlined_expression.ownership = Ownership::ImmutableOwned;
        AstNode {
            kind: NodeKind::Rvalue(inlined_expression),
            location: base_location,
            scope: context.scope.clone(),
        }
    } else {
        AstNode {
            kind: NodeKind::Rvalue(Expression::reference(
                reference_arg.id.to_owned(),
                reference_arg.value.data_type.to_owned(),
                base_location.clone(),
                reference_arg.value.ownership.to_owned(),
            )),
            scope: context.scope.to_owned(),
            location: base_location,
        }
    }
}

fn receiver_node_type(node: &AstNode) -> Result<DataType, CompilerError> {
    Ok(node.get_expr()?.data_type)
}

pub(crate) fn parse_postfix_chain(
    token_stream: &mut FileTokens,
    mut receiver_node: AstNode,
    receiver_access_mode: ReceiverAccessMode,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    // WHAT: parses chained postfix member access and receiver method calls (`a.b.c(...)`).
    // WHY: expression parsing, assignment parsing, and mutation targets share this chain policy,
    //      so one coordinator preserves behavior while dedicated handlers own each responsibility.
    let mut saw_method_call = false;

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
            return_rule_error!(
                "Expected property or method name after '.', but reached the end of input.",
                fallback_location,
                {
                    CompilationStage => "AST Construction",
                    PrimarySuggestion => "Add a property or method name after the dot",
                }
            );
        }

        let member_name = parse_member_name(token_stream, string_table)?;
        let receiver_type = receiver_node_type(&receiver_node)?;
        let member_location = token_stream.current_location();
        let member_context = MemberStepContext {
            receiver_node: receiver_node.to_owned(),
            receiver_type: &receiver_type,
            member_name,
            member_location: member_location.clone(),
            receiver_access_mode,
            scope_context: context,
        };

        if let Some(field_access) =
            parse_field_member_access(token_stream, member_context.to_owned(), string_table)?
        {
            receiver_node = field_access;
            continue;
        }

        if let Some(collection_builtin_call) =
            parse_collection_builtin_member(token_stream, member_context.to_owned(), string_table)?
        {
            receiver_node = collection_builtin_call;
            saw_method_call = true;
            continue;
        }

        if let Some(error_builtin_call) =
            parse_error_builtin_member(token_stream, member_context.to_owned(), string_table)?
        {
            receiver_node = error_builtin_call;
            saw_method_call = true;
            continue;
        }

        if let Some(receiver_method_call) =
            parse_receiver_method_call(token_stream, member_context, string_table)?
        {
            receiver_node = receiver_method_call;
            saw_method_call = true;
            continue;
        }

        return_rule_error!(
            format!(
                "Property or method '{}' not found for '{}'.",
                string_table.resolve(member_name),
                receiver_type.display_with_table(string_table)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Check the available fields and receiver methods for this type",
            }
        );
    }

    if token_stream.current_token_kind().is_assignment_operator()
        && !ast_node_is_place(&receiver_node)
    {
        let receiver_type = receiver_node_type(&receiver_node)?;
        return_rule_error!(
            format!(
                "Field assignment requires a mutable place receiver. '{}' is a temporary expression, not a mutable place.",
                receiver_type.display_with_table(string_table)
            ),
            receiver_node.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Bind this value to a mutable variable first, then assign through that variable's field path",
            }
        );
    }

    if receiver_access_mode == ReceiverAccessMode::Mutable && !saw_method_call {
        return_rule_error!(
            "Mutable receiver marker '~' is only valid for receiver method calls like '~value.method(...)'.",
            receiver_node.location.clone(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Apply '~' directly to a receiver method call",
            }
        );
    }

    Ok(receiver_node)
}

pub fn parse_field_access(
    token_stream: &mut FileTokens,
    base_arg: &Declaration,
    context: &ScopeContext,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    parse_field_access_with_receiver_access(
        token_stream,
        base_arg,
        context,
        ReceiverAccessMode::Shared,
        string_table,
    )
}

pub(crate) fn parse_field_access_with_receiver_access(
    token_stream: &mut FileTokens,
    base_arg: &Declaration,
    context: &ScopeContext,
    receiver_access_mode: ReceiverAccessMode,
    string_table: &mut StringTable,
) -> Result<AstNode, CompilerError> {
    let base_location = if token_stream.index > 0 {
        token_stream.tokens[token_stream.index - 1].location.clone()
    } else {
        token_stream.current_location()
    };

    parse_postfix_chain(
        token_stream,
        receiver_reference_node(base_arg, context, base_location),
        receiver_access_mode,
        context,
        string_table,
    )
}
