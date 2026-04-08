//! Field/member-name parsing and field-access AST construction.
//!
//! WHAT: owns member token parsing and field access node construction.
//! WHY: field reads and inlined const-field reads should not be mixed with builtin or method policy.

use super::MemberStepContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword, reserved_trait_keyword_error,
};
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;

fn field_member(receiver_type: &DataType, field_id: StringId) -> Option<Declaration> {
    receiver_type.struct_fields().and_then(|fields| {
        fields
            .iter()
            .find(|field| field.id.name() == Some(field_id))
            .cloned()
    })
}

/// Parses a member identifier that appears after a dot token.
pub(super) fn parse_member_name(
    token_stream: &FileTokens,
    string_table: &mut StringTable,
) -> Result<StringId, CompilerError> {
    match token_stream.current_token_kind() {
        TokenKind::Symbol(id) => Ok(*id),
        TokenKind::IntLiteral(value) => Ok(string_table.get_or_intern(value.to_string())),
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword(token_stream.current_token_kind())
                .expect("reserved trait token should map to a keyword");

            Err(reserved_trait_keyword_error(
                keyword,
                token_stream.current_location(),
                "AST Construction",
                "Use a normal field or receiver method name until traits are implemented",
            ))
        }
        _ => return_rule_error!(
            format!(
                "Expected property or method name after '.', found '{:?}'",
                token_stream.current_token_kind()
            ),
            token_stream.current_location(),
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Use a valid property or method name after the dot",
            }
        ),
    }
}

/// Parses field-only member access and returns `None` when the member is not a field.
pub(super) fn parse_field_member_access(
    token_stream: &mut FileTokens,
    context: MemberStepContext<'_>,
    string_table: &StringTable,
) -> Result<Option<AstNode>, CompilerError> {
    let MemberStepContext {
        receiver_node,
        receiver_type,
        member_name,
        member_location,
        scope_context,
        ..
    } = context;

    let Some(field) = field_member(receiver_type, member_name) else {
        return Ok(None);
    };

    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::OpenParenthesis {
        return_rule_error!(
            format!(
                "'{}' is a field, not a receiver method. Dot-call syntax is reserved for declared receiver methods.",
                string_table.resolve(member_name)
            ),
            member_location,
            {
                CompilationStage => "AST Construction",
                PrimarySuggestion => "Remove the parentheses to read the field, or declare a receiver method with this name instead",
            }
        );
    }

    let node = if scope_context.kind.is_constant_context() && field.value.is_compile_time_constant()
    {
        let mut inlined_expression = field.value;
        inlined_expression.ownership = Ownership::ImmutableOwned;
        AstNode {
            kind: NodeKind::Rvalue(inlined_expression),
            scope: scope_context.scope.clone(),
            location: member_location,
        }
    } else {
        AstNode {
            kind: NodeKind::FieldAccess {
                base: Box::new(receiver_node),
                field: member_name,
                data_type: field.value.data_type,
                ownership: field.value.ownership,
            },
            scope: scope_context.scope.to_owned(),
            location: member_location,
        }
    };

    Ok(Some(node))
}
