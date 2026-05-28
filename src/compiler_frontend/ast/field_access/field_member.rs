//! Field/member-name parsing and field-access AST construction.
//!
//! WHAT: owns member token parsing and field access node construction.
//! WHY: field reads and inlined const-field reads should not be mixed with builtin or method policy.

use super::MemberStepContext;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind};
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::expressions::expression_types::ConstRecordState;
use crate::compiler_frontend::ast::instrumentation::{AstCounter, increment_ast_counter};
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidFieldAccessReason};
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::diagnostic_type_spelling;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::reserved_trait_syntax::{
    reserved_trait_keyword_error, reserved_trait_keyword_or_dispatch_mismatch,
};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashMap;

// --------------------------
//  Types
// --------------------------

struct ResolvedFieldMember {
    field_name: StringId,
    type_id: TypeId,
    diagnostic_type: DataType,
    value_mode: ValueMode,
    const_record_state: ConstRecordState,
    const_inline_value: Option<Expression>,
}

// --------------------------
//  Helpers
// --------------------------

fn const_field_value<'a>(
    receiver_type_id: TypeId,
    field_name: StringId,
    type_environment: &TypeEnvironment,
    resolved_struct_fields_by_path: Option<&'a FxHashMap<InternedPath, Vec<Declaration>>>,
) -> Option<&'a Expression> {
    if type_environment
        .generic_instance_key(receiver_type_id)
        .is_some()
    {
        return None;
    }

    let nominal_path = type_environment.nominal_path(receiver_type_id)?;
    let fields = resolved_struct_fields_by_path.and_then(|map| map.get(nominal_path))?;
    let field = fields
        .iter()
        .find(|field| field.id.name() == Some(field_name))?;

    Some(&field.value)
}

fn const_inline_field_value(
    receiver_type_id: TypeId,
    field_name: StringId,
    type_environment: &TypeEnvironment,
    resolved_struct_fields_by_path: Option<&FxHashMap<InternedPath, Vec<Declaration>>>,
) -> Option<Expression> {
    let field_value = const_field_value(
        receiver_type_id,
        field_name,
        type_environment,
        resolved_struct_fields_by_path,
    )?;

    if field_value.is_compile_time_constant() {
        let mut inlined_expression = field_value.to_owned();
        inlined_expression.value_mode = ValueMode::ImmutableOwned;
        Some(inlined_expression)
    } else {
        None
    }
}

fn resolve_field_member(
    receiver_type_id: TypeId,
    field_name: StringId,
    type_environment: &TypeEnvironment,
    resolved_struct_fields_by_path: Option<&FxHashMap<InternedPath, Vec<Declaration>>>,
    receiver_is_const_record: bool,
    should_try_const_inline: bool,
) -> Option<ResolvedFieldMember> {
    // Try canonical TypeEnvironment first; fall back to AST-owned struct shells
    // when the struct was registered with an empty field list during early
    // identity-only registration (e.g. during constant resolution before final
    // field types are resolved).
    let field_type_id = type_environment
        .field_for(receiver_type_id, field_name)
        .map(|field| field.type_id)
        .or_else(|| {
            let nominal_path = type_environment.nominal_path(receiver_type_id)?;
            let fields = resolved_struct_fields_by_path?.get(nominal_path)?;
            let declaration = fields.iter().find(|f| f.id.name() == Some(field_name))?;
            Some(declaration.value.type_id)
        })?;

    let const_field_value = const_field_value(
        receiver_type_id,
        field_name,
        type_environment,
        resolved_struct_fields_by_path,
    );

    let const_inline_value = if should_try_const_inline {
        // Const records need full declaration values for field inlining. The
        // TypeEnvironment owns semantic field types, while the resolved struct
        // field side table owns foldable default expressions.
        const_inline_field_value(
            receiver_type_id,
            field_name,
            type_environment,
            resolved_struct_fields_by_path,
        )
    } else {
        None
    };
    let field_value_is_const_record = const_field_value
        .map(Expression::is_const_record_value)
        .unwrap_or(false);
    let field_type_is_struct = matches!(
        type_environment.get(field_type_id),
        Some(TypeDefinition::Struct(_))
    );
    // A struct-valued field read from a const record is still a compile-time
    // member group. It may be chained into another field access, but the field
    // value itself must not be materialized as a runtime struct.
    let const_record_state =
        if receiver_is_const_record && (field_value_is_const_record || field_type_is_struct) {
            ConstRecordState::ConstRecord
        } else {
            ConstRecordState::RuntimeValue
        };

    Some(ResolvedFieldMember {
        field_name,
        type_id: field_type_id,
        diagnostic_type: diagnostic_type_spelling(field_type_id, type_environment),
        value_mode: ValueMode::ImmutableOwned,
        const_record_state,
        const_inline_value,
    })
}

pub(super) fn parse_member_name_typed(
    token_stream: &FileTokens,
    string_table: &mut StringTable,
) -> Result<StringId, ExpressionParseError> {
    match token_stream.current_token_kind() {
        TokenKind::Symbol(id) => Ok(*id),
        TokenKind::IntLiteral(value) => Ok(string_table.get_or_intern(value.to_string())),
        TokenKind::Must | TokenKind::TraitThis => {
            let keyword = reserved_trait_keyword_or_dispatch_mismatch(
                token_stream.current_token_kind(),
                token_stream.current_location(),
                "AST Construction",
                "postfix/member parsing",
            )?;

            Err(reserved_trait_keyword_error(keyword, token_stream.current_location()).into())
        }

        _ => Err(CompilerDiagnostic::invalid_field_access(
            InvalidFieldAccessReason::ExpectedNameAfterDot,
            None,
            None,
            token_stream.current_location(),
        )
        .into()),
    }
}

// --------------------------
//  Main parsers
// --------------------------

pub(super) fn parse_field_member_access_typed(
    token_stream: &mut FileTokens,
    context: MemberStepContext<'_>,
    type_interner: &mut AstTypeInterner<'_>,
    _string_table: &StringTable,
) -> Result<Option<AstNode>, ExpressionParseError> {
    let MemberStepContext {
        receiver_node,
        receiver_type_id,
        member_name,
        member_location,
        scope_context,
        ..
    } = context;
    let receiver_is_const_record = receiver_node.expression_is_const_record_value()?;

    let Some(field) = resolve_field_member(
        receiver_type_id,
        member_name,
        type_interner.environment(),
        scope_context.resolved_struct_fields_by_path.as_deref(),
        receiver_is_const_record,
        scope_context.kind.is_constant_context(),
    ) else {
        return Ok(None);
    };

    token_stream.advance();

    if token_stream.current_token_kind() == &TokenKind::OpenParenthesis {
        return Err(CompilerDiagnostic::invalid_field_access(
            InvalidFieldAccessReason::FieldNotMethod,
            Some(member_name),
            Some(receiver_type_id),
            member_location,
        )
        .into());
    }

    let result_node = if let Some(inlined_expression) = field.const_inline_value {
        AstNode {
            kind: NodeKind::Rvalue(inlined_expression),
            scope: scope_context.scope.clone(),
            location: member_location,
        }
    } else {
        increment_ast_counter(AstCounter::PostfixReceiverNodesCopied);

        AstNode {
            kind: NodeKind::FieldAccess {
                base: Box::new(receiver_node.to_owned()),
                field: field.field_name,
                diagnostic_type: field.diagnostic_type,
                type_id: field.type_id,
                const_record_state: field.const_record_state,
                value_mode: field.value_mode,
            },
            scope: scope_context.scope.to_owned(),
            location: member_location,
        }
    };

    Ok(Some(result_node))
}
