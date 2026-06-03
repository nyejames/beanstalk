//! Concrete trait evidence fallback dispatch.
//!
//! WHAT: after direct source methods, generic bounds, and external methods have
//!       been tried, looks up visible trait evidence for the concrete receiver
//!       type and builds a trait-surface call if exactly one unambiguous match
//!       exists.
//! WHY: concrete evidence fallback must run after all direct dispatch paths so
//!      that explicit receiver methods and external package methods take priority
//!      over implicit trait conformance.

use super::shared::{
    TraitSurfaceReceiverMethod, method_path_from_evidence, requirement_receiver_is_mutable,
    signature_from_trait_requirement,
};
use crate::compiler_frontend::ast::ScopeContext;
use crate::compiler_frontend::ast::expressions::error::ExpressionParseError;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidReceiverCallReason};
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::definitions::{
    ResolvedTraitDefinition, ResolvedTraitRequirement,
};
use crate::compiler_frontend::traits::evidence::TraitEvidenceDefinition;

struct ConcreteTraitEvidenceRequirementCandidate<'a> {
    trait_definition: &'a ResolvedTraitDefinition,
    requirement: &'a ResolvedTraitRequirement,
    evidence: &'a TraitEvidenceDefinition,
}

fn concrete_trait_evidence_requirement_candidates<'a>(
    receiver_type_id: TypeId,
    member_name: StringId,
    scope_context: &'a ScopeContext,
) -> Result<Vec<ConcreteTraitEvidenceRequirementCandidate<'a>>, ExpressionParseError> {
    let source_file_scope =
        scope_context.required_source_file_scope("concrete trait receiver fallback")?;
    let evidence_environment = scope_context.trait_evidence_environment();
    let mut candidates = Vec::new();

    for evidence_id in
        evidence_environment.receiver_fallback_candidates(receiver_type_id, source_file_scope)
    {
        let Some(evidence) = evidence_environment.get(evidence_id) else {
            continue;
        };

        if !scope_context.trait_id_is_visible(evidence.trait_id) {
            continue;
        }

        let Some(trait_definition) = scope_context.trait_environment().get(evidence.trait_id)
        else {
            continue;
        };

        for requirement in &trait_definition.requirements {
            if requirement.name == member_name {
                candidates.push(ConcreteTraitEvidenceRequirementCandidate {
                    trait_definition,
                    requirement,
                    evidence,
                });
            }
        }
    }

    Ok(candidates)
}

pub(super) fn lookup_concrete_trait_evidence_receiver_method(
    scope_context: &ScopeContext,
    receiver_type_id: TypeId,
    member_name: StringId,
    member_location: &SourceLocation,
    type_environment: &TypeEnvironment,
    string_table: &mut StringTable,
) -> Result<Option<TraitSurfaceReceiverMethod>, ExpressionParseError> {
    if super::generic_bound_methods::generic_parameter_id_for_receiver_type(
        receiver_type_id,
        type_environment,
    )
    .is_some()
    {
        return Ok(None);
    }

    let candidates = concrete_trait_evidence_requirement_candidates(
        receiver_type_id,
        member_name,
        scope_context,
    )?;
    match candidates.as_slice() {
        [] => Ok(None),
        [_first, _second, ..] => Err(CompilerDiagnostic::invalid_receiver_call(
            InvalidReceiverCallReason::AmbiguousTraitEvidenceMethod,
            None,
            Some(member_name),
            member_location.clone(),
        )
        .into()),
        [candidate] => {
            let Some(method_path) =
                method_path_from_evidence(candidate.evidence, candidate.requirement)
            else {
                return Ok(None);
            };
            let signature = signature_from_trait_requirement(
                &method_path,
                candidate.trait_definition,
                candidate.requirement,
                receiver_type_id,
                type_environment,
                string_table,
            );

            Ok(Some(TraitSurfaceReceiverMethod {
                method_path,
                signature,
                receiver_mutable: requirement_receiver_is_mutable(candidate.requirement),
            }))
        }
    }
}
