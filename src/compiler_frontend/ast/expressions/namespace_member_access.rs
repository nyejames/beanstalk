//! Namespace import record member access parsing.
//!
//! WHAT: parses `namespace.member` value-position access for shallow import records.
//! WHY: namespace records are a distinct source/external dispatch surface and should not
//! live inside the generic identifier parser.

use super::error::ExpressionParseError;
use super::expression_rpn::ExpressionRpnItem;
use super::external_namespace_members::{
    ExternalNamespaceConstantMemberInput, ExternalNamespaceFunctionMemberInput,
    parse_external_namespace_constant_member, parse_external_namespace_function_member,
};
use super::parse_expression_dispatch::{
    ExpressionOperandInput, push_expression_operand_at_location,
};
use super::source_function_calls::{SourceCallableMemberInput, parse_source_callable_member};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::field_access::reference_expression_from_declaration;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidFieldAccessReason, NamespaceTypeValueMisuseKind,
};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::{
    NamespaceRecord, NamespaceValueMember,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

/// Input bundle for namespace member access parsing.
///
/// WHAT: carries everything needed to resolve a dot-accessed member of a namespace import record.
/// WHY: avoids threading a long argument list through the identifier dispatch path.
pub(super) struct NamespaceMemberAccessInput<'a, 'env> {
    pub(super) token_stream: &'a mut FileTokens,
    pub(super) context: &'a ScopeContext,
    pub(super) type_interner: &'a mut AstTypeInterner<'env>,
    pub(super) expression: &'a mut Vec<ExpressionRpnItem>,
    pub(super) allow_boundary_catch: bool,
    pub(super) expected_result_evidence_allowed: bool,
    pub(super) record_name: StringId,
    pub(super) record: &'a NamespaceRecord,
    pub(super) string_table: &'a mut StringTable,
}

/// Parse `namespace.member` or `namespace.member()` access on an import record.
///
/// WHAT: resolves a dot-accessed member of a namespace import record into a reference
/// or function call. Rejects nested traversal, type-member misuse, and missing members.
/// WHY: namespace records are shallow; this function enforces that constraint and
/// routes source function members through the shared source callable parser.
pub(super) fn parse_namespace_member_access(
    input: NamespaceMemberAccessInput<'_, '_>,
) -> Result<(), ExpressionParseError> {
    let NamespaceMemberAccessInput {
        token_stream,
        context,
        type_interner,
        expression,
        allow_boundary_catch,
        expected_result_evidence_allowed,
        record_name,
        record,
        string_table,
    } = input;

    token_stream.advance(); // move from namespace name to '.'
    token_stream.advance(); // move from '.' to member name

    let member_location = token_stream.current_location();
    let TokenKind::Symbol(member_name) = token_stream.current_token_kind().to_owned() else {
        return Err(CompilerDiagnostic::invalid_field_access(
            InvalidFieldAccessReason::ExpectedNameAfterDot,
            None,
            None,
            member_location,
        )
        .into());
    };

    // Reject nested traversal: namespace records are shallow.
    if token_stream.peek_next_token() == Some(&TokenKind::Dot) {
        return Err(CompilerDiagnostic::nested_traversal(
            record_name,
            token_stream.current_location(),
        )
        .into());
    }

    // Try value member first.
    if let Some(member) = record.value_members.get(&member_name) {
        match member {
            NamespaceValueMember::SourceDeclaration(symbol_path) => {
                let Some(declaration) = context
                    .shared
                    .lookups
                    .declaration_table
                    .get_by_path(symbol_path)
                else {
                    return Err(CompilerDiagnostic::unknown_value_name(
                        member_name,
                        member_location,
                    )
                    .into());
                };

                // Namespace fields are not first-class function values. A function member must
                // continue through the normal call parser so missing `(...)` reports at the call
                // boundary instead of lowering a `NoValue` declaration reference.
                if let Some(signature) = context.source_callable_signature(declaration) {
                    let generic_template = context.lookup_generic_function_template(symbol_path);

                    parse_source_callable_member(SourceCallableMemberInput {
                        token_stream,
                        function_path: symbol_path,
                        signature,
                        generic_template,
                        visible_name: member_name,
                        call_location: member_location.clone(),
                        context,
                        expression,
                        allow_boundary_catch,
                        expected_result_evidence_allowed,
                        type_interner,
                        string_table,
                    })?;

                    return Ok(());
                }

                let reference_expression = reference_expression_from_declaration(
                    declaration,
                    context,
                    type_interner,
                    member_location.clone(),
                );
                token_stream.advance();

                push_expression_operand_at_location(
                    token_stream,
                    context,
                    type_interner,
                    string_table,
                    expression,
                    allow_boundary_catch,
                    ExpressionOperandInput {
                        operand: reference_expression,
                        wrapper_location: member_location,
                    },
                )?;
                return Ok(());
            }

            NamespaceValueMember::ExternalSymbol(symbol_id) => match symbol_id {
                // External function member: delegate to the external function call parser.
                ExternalSymbolId::Function(function_id) => {
                    return parse_external_namespace_function_member(
                        ExternalNamespaceFunctionMemberInput {
                            function_id: *function_id,
                            member_name,
                            member_location,
                            token_stream,
                            context,
                            type_interner,
                            expression,
                            allow_boundary_catch,
                            string_table,
                        },
                    );
                }

                // External constant member: delegate to the external constant parser.
                ExternalSymbolId::Constant(constant_id) => {
                    return parse_external_namespace_constant_member(
                        ExternalNamespaceConstantMemberInput {
                            constant_id: *constant_id,
                            member_name,
                            member_location,
                            token_stream,
                            context,
                            type_interner,
                            expression,
                            allow_boundary_catch,
                            string_table,
                        },
                    );
                }

                // Type symbol used in value position: report misuse.
                ExternalSymbolId::Type(_) => {
                    return Err(CompilerDiagnostic::namespace_type_value_misuse(
                        member_name,
                        NamespaceTypeValueMisuseKind::Value,
                        NamespaceTypeValueMisuseKind::Type,
                        member_location,
                    )
                    .into());
                }
            },
        }
    }

    // Check if the member exists as a type member (misuse in value position).
    if record.type_members.contains_key(&member_name) {
        return Err(CompilerDiagnostic::namespace_type_value_misuse(
            member_name,
            NamespaceTypeValueMisuseKind::Value,
            NamespaceTypeValueMisuseKind::Type,
            member_location,
        )
        .into());
    }

    Err(CompilerDiagnostic::unknown_value_name(member_name, member_location).into())
}
