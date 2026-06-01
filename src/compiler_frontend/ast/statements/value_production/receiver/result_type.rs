//! Branch result-type inference and coercion for value-producing control flow.
//!
//! WHAT: unifies then/else branch types, coerces individual expressions, and infers
//! result types for block-form value-if and full value-match receivers.
//! WHY: receiver sites need contextual compatibility checks on canonical `TypeId`s;
//! `DataType` must not be used for semantic decisions once type IDs exist.

use crate::compiler_frontend::ast::ast_nodes::AstNode;
use crate::compiler_frontend::ast::statements::match_patterns::MatchArm;
use crate::compiler_frontend::ast::statements::value_production::completeness::extract_single_produced_type;
use crate::compiler_frontend::ast::statements::value_production::types::ValueReceiverKind;
use crate::compiler_frontend::ast::type_interner::AstTypeInterner;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidControlFlowStatementReason, TypeMismatchContext,
};
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::type_coercion::compatibility::is_declaration_compatible;

/// Maps a receiver kind to the diagnostic context used when branch types mismatch.
pub(super) fn receiver_type_mismatch_context(kind: ValueReceiverKind) -> TypeMismatchContext {
    match kind {
        ValueReceiverKind::Return => TypeMismatchContext::ReturnValue,
        ValueReceiverKind::Declaration => TypeMismatchContext::Declaration,
        _ => TypeMismatchContext::Assignment,
    }
}

/// Unifies the types of two inline branch expressions.
///
/// WHAT: when the expected type is known, validates both branches are compatible
/// and returns it. When inferred, ensures both branches agree and returns the shared type.
pub(super) fn infer_inline_result_type(
    then_type: TypeId,
    else_type: TypeId,
    expected_type_id: Option<TypeId>,
    type_interner: &mut AstTypeInterner<'_>,
    location: &SourceLocation,
    receiver_kind: ValueReceiverKind,
) -> Result<TypeId, CompilerDiagnostic> {
    let context = receiver_type_mismatch_context(receiver_kind);

    if let Some(expected) = expected_type_id {
        let env = type_interner.environment();

        if !is_declaration_compatible(expected, then_type, env) {
            return Err(CompilerDiagnostic::type_mismatch(
                expected,
                then_type,
                context,
                location.clone(),
            ));
        }

        if !is_declaration_compatible(expected, else_type, env) {
            return Err(CompilerDiagnostic::type_mismatch(
                expected,
                else_type,
                context,
                location.clone(),
            ));
        }

        return Ok(expected);
    }

    if then_type != else_type {
        return Err(CompilerDiagnostic::type_mismatch(
            then_type,
            else_type,
            context,
            location.clone(),
        ));
    }

    Ok(then_type)
}

/// Infers the result type from block-form branch bodies.
///
/// WHAT: when the receiver expects known types, returns the corresponding expression
/// type (single type or internal tuple type for multi-value). For inferred single-value
/// declarations, scans each branch for `ThenValue` nodes and returns the produced type.
/// WHY: block bodies may contain nested control flow; this extracts the type from
/// the first producing path it finds.
pub(super) fn infer_block_if_result_type(
    then_body: &[AstNode],
    else_body: &[AstNode],
    expected_result_type_ids: &[TypeId],
    type_interner: &mut AstTypeInterner<'_>,
    location: &SourceLocation,
    receiver_kind: ValueReceiverKind,
) -> Result<TypeId, CompilerDiagnostic> {
    if expected_result_type_ids.len() > 1 {
        return Ok(type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(expected_result_type_ids.to_vec()));
    }

    if let Some(expected) = expected_result_type_ids.first().copied() {
        return Ok(expected);
    }

    let then_type = extract_single_produced_type(then_body);
    let else_type = extract_single_produced_type(else_body);

    let context = receiver_type_mismatch_context(receiver_kind);
    match (then_type, else_type) {
        (Some(then_type_id), Some(else_type_id)) => {
            if then_type_id != else_type_id {
                return Err(CompilerDiagnostic::type_mismatch(
                    then_type_id,
                    else_type_id,
                    context,
                    location.clone(),
                ));
            }

            Ok(then_type_id)
        }

        (Some(then_type_id), None) => Ok(then_type_id),
        (None, Some(else_type_id)) => Ok(else_type_id),

        (None, None) => Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        )),
    }
}

/// Infers the result type for a full value-producing match.
///
/// WHAT: for multi-value receivers, returns the interned tuple type.
/// For single-value inferred receivers, collects produced types from all arms
/// and default and ensures they agree.
pub(super) fn infer_value_match_result_type(
    arms: &[MatchArm],
    default: Option<&[AstNode]>,
    expected_result_type_ids: &[TypeId],
    type_interner: &mut AstTypeInterner<'_>,
    location: &SourceLocation,
    receiver_kind: ValueReceiverKind,
) -> Result<TypeId, CompilerDiagnostic> {
    if expected_result_type_ids.len() > 1 {
        return Ok(type_interner
            .environment_mut_for_derived_types()
            .intern_tuple(expected_result_type_ids.to_vec()));
    }

    if let Some(expected) = expected_result_type_ids.first().copied() {
        return Ok(expected);
    }

    let produced_types = collect_value_match_single_produced_types(arms, default);
    let Some(first_type) = produced_types.first().copied() else {
        return Err(CompilerDiagnostic::invalid_control_flow_statement(
            InvalidControlFlowStatementReason::ValueIfNoProducingPath,
            location.clone(),
        ));
    };

    let context = receiver_type_mismatch_context(receiver_kind);
    for produced_type in produced_types.iter().copied().skip(1) {
        if produced_type != first_type {
            return Err(CompilerDiagnostic::type_mismatch(
                first_type,
                produced_type,
                context,
                location.clone(),
            ));
        }
    }

    Ok(first_type)
}

/// Collects the produced types from every arm and optional default body.
pub(super) fn collect_value_match_single_produced_types(
    arms: &[MatchArm],
    default: Option<&[AstNode]>,
) -> Vec<TypeId> {
    let mut produced_types = Vec::new();

    for arm in arms {
        if let Some(type_id) = extract_single_produced_type(&arm.body) {
            produced_types.push(type_id);
        }
    }

    if let Some(default_body) = default
        && let Some(type_id) = extract_single_produced_type(default_body)
    {
        produced_types.push(type_id);
    }

    produced_types
}
