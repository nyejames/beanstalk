//! Dynamic trait coercion metadata and evidence selection.
//!
//! WHAT: determines when a concrete value can be implicitly wrapped into a dynamic
//!       trait value at an explicit typed boundary, and selects the evidence that
//!       backs the wrapper.
//! WHY: dynamic coercion is frontend-owned; the backend must not rediscover trait
//!      evidence or method shapes.

use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, InvalidDynamicTraitTypeReason,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::traits::evidence::TraitEvidenceEnvironment;
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitId};

/// Metadata for a concrete-to-dynamic-trait coercion decided at an AST boundary.
///
/// WHAT: carries every fact the backend needs to lower the wrapper and dispatch
///       without rediscovering traits or scanning conformance headers.
/// WHY: evidence selection happens once in the frontend and is frozen into HIR.
#[derive(Clone, Debug)]
pub struct DynamicTraitCoercion {
    pub(crate) source_concrete_type_id: TypeId,
    pub(crate) target_dynamic_trait_type_id: TypeId,
    pub(crate) target_trait_id: TraitId,
    pub(crate) selected_evidence_id: TraitEvidenceId,
    #[allow(dead_code)] // Retained for source diagnostics across AST/HIR/backend validation.
    pub(crate) location: SourceLocation,
}

/// Select dynamic trait coercion metadata for an expression at an explicit typed boundary.
///
/// WHAT: turns a typed-boundary expected `DynamicTrait` `TypeId` plus a concrete expression into
/// evidence-backed coercion metadata.
/// WHY: declarations, returns, collection elements, and calls should all use one evidence
/// selection policy instead of growing local copies of trait lookup rules.
pub(crate) fn select_dynamic_trait_coercion_for_expression(
    expression: &Expression,
    expected_type_id: TypeId,
    type_environment: &TypeEnvironment,
    scope_context: &ScopeContext,
) -> Result<Option<DynamicTraitCoercion>, CompilerDiagnostic> {
    try_select_dynamic_trait_evidence(
        expression.type_id,
        expected_type_id,
        type_environment,
        scope_context.trait_evidence_environment(),
        scope_context.source_file_scope.as_ref(),
        |trait_id| scope_context.trait_id_is_visible(trait_id),
        expression.location.clone(),
    )
}

/// Attempts to select evidence for a concrete value being coerced to a dynamic trait type.
///
/// WHAT: checks whether `expected_type_id` is a `DynamicTrait` and, if so, whether
///       visible evidence exists for `actual_type_id` coercing to that trait.
/// WHY: this is the single frontend-owned decision point for dynamic coercion.
///
/// Returns:
/// - `Ok(Some(coercion))` when evidence is found and the coercion is valid.
/// - `Ok(None)` when `expected_type_id` is not a dynamic trait (no coercion applies).
/// - `Err(diagnostic)` when `expected_type_id` is a dynamic trait but no visible
///   evidence exists for the concrete type.
pub(crate) fn try_select_dynamic_trait_evidence(
    actual_type_id: TypeId,
    expected_type_id: TypeId,
    type_environment: &TypeEnvironment,
    evidence_environment: &TraitEvidenceEnvironment,
    source_file_scope: Option<&InternedPath>,
    trait_id_is_visible: impl Fn(TraitId) -> bool,
    location: SourceLocation,
) -> Result<Option<DynamicTraitCoercion>, CompilerDiagnostic> {
    let Some(TypeDefinition::DynamicTrait(dynamic_def)) = type_environment.get(expected_type_id)
    else {
        return Ok(None);
    };

    let target_trait_id = dynamic_def.trait_id;

    if !trait_id_is_visible(target_trait_id) {
        return Err(CompilerDiagnostic::invalid_dynamic_trait_type(
            dynamic_def.name,
            InvalidDynamicTraitTypeReason::MissingEvidence {
                concrete_type_id: actual_type_id,
            },
            location,
        ));
    }

    // Evidence selection priority: builtin > canonical > file-local extension.
    let file_local_evidence_id = source_file_scope.and_then(|source_file| {
        evidence_environment.file_local_for(source_file, actual_type_id, target_trait_id)
    });

    let evidence_id = evidence_environment
        .builtin_for(actual_type_id, target_trait_id)
        .or_else(|| evidence_environment.canonical_for(actual_type_id, target_trait_id))
        .or(file_local_evidence_id);

    let Some(selected_evidence_id) = evidence_id else {
        return Err(CompilerDiagnostic::invalid_dynamic_trait_type(
            dynamic_def.name,
            InvalidDynamicTraitTypeReason::MissingEvidence {
                concrete_type_id: actual_type_id,
            },
            location,
        ));
    };

    debug_assert!(
        evidence_environment.get(selected_evidence_id).is_some(),
        "selected dynamic trait evidence id must exist in the evidence environment"
    );

    Ok(Some(DynamicTraitCoercion {
        source_concrete_type_id: actual_type_id,
        target_dynamic_trait_type_id: expected_type_id,
        target_trait_id,
        selected_evidence_id,
        location,
    }))
}

/// Wraps `expr` in a dynamic trait value constructor when coercion metadata is present.
///
/// WHAT: produces an explicit AST node that carries the selected evidence facts.
/// WHY: backends lower this node directly instead of rediscovering trait evidence.
pub(crate) fn construct_dynamic_trait_value(
    expr: Expression,
    coercion: DynamicTraitCoercion,
) -> Expression {
    let location = expr.location.clone();
    let value_mode = expr.value_mode.clone();
    let contains_regular_division = expr.contains_regular_division;
    let const_record_state = expr.const_record_state;

    let mut expression = Expression::new(
        ExpressionKind::ConstructDynamicTraitValue {
            value: Box::new(expr),
            coercion: coercion.clone(),
        },
        location,
        // The expression type is the dynamic trait type, not the concrete source type.
        // This is what makes the coercion explicit in the AST type system.
        coercion.target_dynamic_trait_type_id,
        crate::compiler_frontend::datatypes::DataType::Inferred,
        value_mode,
    )
    .with_regular_division_provenance(contains_regular_division);
    expression.const_record_state = const_record_state;
    expression
}
