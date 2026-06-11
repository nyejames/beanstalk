//! Trait evidence data types and lookup environment.
//!
//! WHAT: Defines the core data structures for trait conformance evidence (`TraitEvidenceEnvironment`,
//!       `TraitEvidenceDefinition`, etc.) and manages indexing/rebuilding of these structures.
//! WHY: Provides a structured side-table of proven conformance facts that can be queried by type checking
//!      and code generation without rescanning syntax.

use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitId, TraitRequirementId};
use rustc_hash::FxHashMap;

/// The ownership class of an evidence record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Compiler-owned builtin evidence is scaffolded but not registered yet.
pub(crate) enum TraitEvidenceKind {
    Canonical,
    Builtin,
}

/// One requirement mapped to the receiver method that implements it.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Stored as complete evidence metadata even when a build uses only static bounds.
pub(crate) struct TraitRequirementEvidence {
    pub(crate) requirement_id: TraitRequirementId,
    pub(crate) method_path: InternedPath,
}

/// Resolved evidence for one accepted conformance declaration.
#[derive(Clone, Debug)]
#[allow(dead_code)] // Kept as the complete frontend-selected evidence fact projected into HIR.
pub(crate) struct TraitEvidenceDefinition {
    pub(crate) id: TraitEvidenceId,
    pub(crate) kind: TraitEvidenceKind,
    pub(crate) target_type_id: TypeId,
    pub(crate) trait_id: TraitId,
    pub(crate) source_file: InternedPath,
    pub(crate) declaration_location: SourceLocation,
    pub(crate) requirements: Vec<TraitRequirementEvidence>,
}

/// Indexed evidence facts for one module.
#[derive(Clone, Debug, Default)]
pub(crate) struct TraitEvidenceEnvironment {
    evidence: Vec<TraitEvidenceDefinition>,
    canonical_by_target_and_trait: FxHashMap<(TypeId, TraitId), TraitEvidenceId>,
    builtin_by_target_and_trait: FxHashMap<(TypeId, TraitId), TraitEvidenceId>,
    reusable_by_target: FxHashMap<TypeId, Vec<TraitEvidenceId>>,
}

impl TraitEvidenceEnvironment {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn get(&self, id: TraitEvidenceId) -> Option<&TraitEvidenceDefinition> {
        self.evidence.get(id.0 as usize)
    }

    #[allow(dead_code)] // Some compile modes validate traits without querying canonical evidence.
    pub(crate) fn canonical_for(
        &self,
        target_type_id: TypeId,
        trait_id: TraitId,
    ) -> Option<TraitEvidenceId> {
        self.canonical_by_target_and_trait
            .get(&(target_type_id, trait_id))
            .copied()
    }

    pub(crate) fn builtin_for(
        &self,
        target_type_id: TypeId,
        trait_id: TraitId,
    ) -> Option<TraitEvidenceId> {
        self.builtin_by_target_and_trait
            .get(&(target_type_id, trait_id))
            .copied()
    }

    /// Return evidence records that may participate in concrete receiver fallback.
    ///
    /// WHAT: exposes reusable canonical/builtin evidence for a target type.
    /// WHY: receiver-call parsing needs an evidence-backed fallback without scanning raw
    /// conformance headers or depending on source-local extension state.
    pub(crate) fn receiver_fallback_candidates(
        &self,
        target_type_id: TypeId,
    ) -> Vec<TraitEvidenceId> {
        self.reusable_by_target
            .get(&target_type_id)
            .into_iter()
            .flatten()
            .copied()
            .collect()
    }

    #[allow(dead_code)] // No compiler-owned builtin conformances are registered yet.
    pub(crate) fn insert_builtin(&mut self, mut definition: TraitEvidenceDefinition) {
        let id = TraitEvidenceId(self.evidence.len() as u32);
        definition.id = id;

        self.index_definition(&definition);
        self.evidence.push(definition);
    }

    pub(crate) fn insert_validated(&mut self, mut definition: TraitEvidenceDefinition) {
        let id = TraitEvidenceId(self.evidence.len() as u32);
        definition.id = id;

        self.index_definition(&definition);
        self.evidence.push(definition);
    }

    fn index_definition(&mut self, definition: &TraitEvidenceDefinition) {
        match definition.kind {
            TraitEvidenceKind::Canonical => {
                self.canonical_by_target_and_trait.insert(
                    (definition.target_type_id, definition.trait_id),
                    definition.id,
                );
                self.reusable_by_target
                    .entry(definition.target_type_id)
                    .or_default()
                    .push(definition.id);
            }

            TraitEvidenceKind::Builtin => {
                self.builtin_by_target_and_trait.insert(
                    (definition.target_type_id, definition.trait_id),
                    definition.id,
                );
                self.reusable_by_target
                    .entry(definition.target_type_id)
                    .or_default()
                    .push(definition.id);
            }
        }
    }
}
