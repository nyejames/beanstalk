//! Resolved trait environment.
//!
//! WHAT: owns resolved trait definitions, stable trait IDs, and lookup by canonical path.
//! WHY: trait declarations are compile-time contracts. They must not be registered as ordinary
//! `DataType`s or value declarations, but later AST phases still need deterministic identity.

use crate::compiler_frontend::ast::statements::functions::ReturnChannel;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::TypeId;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringIdRemap;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::definitions::{
    ResolvedTraitDefinition, ResolvedTraitParameter, ResolvedTraitRequirement, ResolvedTraitReturn,
    TraitDynamicSafety, TraitReceiverRequirement, TraitVisibility,
};
use crate::compiler_frontend::traits::ids::{TraitId, TraitRequirementId};
use crate::compiler_frontend::value_mode::ValueMode;
use rustc_hash::FxHashMap;

pub(crate) const DISPLAYABLE_TRAIT_NAME: &str = "DISPLAYABLE";
const DISPLAYABLE_REQUIREMENT_NAME: &str = "display";
const TRAIT_THIS_NAME: &str = "This";

/// Trait metadata lookup table for one compiled module.
#[derive(Clone, Debug, Default)]
pub(crate) struct TraitEnvironment {
    definitions: Vec<ResolvedTraitDefinition>,
    ids_by_path: FxHashMap<InternedPath, TraitId>,
    displayable_trait_id: Option<TraitId>,
}

impl TraitEnvironment {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Registers compiler-owned `DISPLAYABLE` metadata.
    ///
    /// WHAT: inserts `display |This| -> String` as a core trait definition.
    /// WHY: output coercion and primitive conformances are later phases, but the trait identity
    /// must exist now so tests and later conformance metadata can reference the same core ID.
    pub(crate) fn register_core_displayable(
        &mut self,
        type_environment: &mut TypeEnvironment,
        string_table: &mut StringTable,
    ) -> TraitId {
        if let Some(id) = self.displayable_trait_id {
            return id;
        }

        let name = string_table.intern(DISPLAYABLE_TRAIT_NAME);
        let requirement_name = string_table.intern(DISPLAYABLE_REQUIREMENT_NAME);
        let this_name = string_table.intern(TRAIT_THIS_NAME);
        let path = InternedPath::from_single_str(DISPLAYABLE_TRAIT_NAME, string_table);
        let source_file = InternedPath::new();
        let location = SourceLocation::default();
        let this_type = type_environment.register_synthetic_generic_parameter(this_name);
        let string_type = type_environment.builtins().string;

        let id = self.allocate_trait_id();
        let requirement_id = self.allocate_requirement_id();
        let requirement = ResolvedTraitRequirement {
            id: requirement_id,
            name: requirement_name,
            name_location: location.clone(),
            receiver: TraitReceiverRequirement::Immutable { this_type },
            parameters: Vec::new(),
            returns: vec![ResolvedTraitReturn {
                type_id: string_type,
                channel: ReturnChannel::Success,
                location: location.clone(),
            }],
            location: location.clone(),
        };

        let definition = ResolvedTraitDefinition {
            id,
            name,
            canonical_path: path.clone(),
            source_file,
            this_type,
            requirements: vec![requirement],
            declaration_location: location,
            visibility: TraitVisibility::Core,
            dynamic_safety: TraitDynamicSafety::DynamicSafe,
        };

        self.ids_by_path.insert(path, id);
        self.definitions.push(definition);
        self.displayable_trait_id = Some(id);
        id
    }

    pub(crate) fn insert(&mut self, definition: ResolvedTraitDefinition) -> Option<TraitId> {
        if let Some(existing_id) = self.ids_by_path.get(&definition.canonical_path).copied() {
            return Some(existing_id);
        }

        let id = definition.id;
        self.ids_by_path
            .insert(definition.canonical_path.clone(), id);
        self.definitions.push(definition);
        None
    }

    pub(crate) fn get(&self, id: TraitId) -> Option<&ResolvedTraitDefinition> {
        self.definitions.get(id.0 as usize)
    }

    pub(crate) fn id_for_path(&self, path: &InternedPath) -> Option<TraitId> {
        self.ids_by_path.get(path).copied()
    }

    /// Resolves the compiler-owned `DISPLAYABLE` scaffold by source spelling.
    ///
    /// WHY: core trait metadata is not registered through normal file visibility, but user source
    /// still refers to it with the ordinary trait name in conformances and dynamic type positions.
    pub(crate) fn displayable_trait_id_for_name(
        &self,
        trait_name: StringId,
        string_table: &StringTable,
    ) -> Option<TraitId> {
        if string_table.resolve(trait_name) == DISPLAYABLE_TRAIT_NAME {
            return self.displayable_trait_id;
        }

        None
    }

    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        self.ids_by_path.clear();

        for definition in &mut self.definitions {
            definition.canonical_path.remap_string_ids(remap);
            definition.source_file.remap_string_ids(remap);
            definition.declaration_location.remap_string_ids(remap);

            for requirement in &mut definition.requirements {
                requirement.name_location.remap_string_ids(remap);
                requirement.location.remap_string_ids(remap);

                for parameter in &mut requirement.parameters {
                    parameter.name.remap_string_ids(remap);
                    parameter.location.remap_string_ids(remap);
                }

                for return_slot in &mut requirement.returns {
                    return_slot.location.remap_string_ids(remap);
                }
            }

            self.ids_by_path
                .insert(definition.canonical_path.clone(), definition.id);
        }
    }

    pub(crate) fn next_trait_id(&self) -> TraitId {
        TraitId(self.definitions.len() as u32)
    }

    pub(crate) fn next_requirement_id(&self) -> TraitRequirementId {
        let requirement_count = self
            .definitions
            .iter()
            .map(|definition| definition.requirements.len())
            .sum::<usize>();
        TraitRequirementId(requirement_count as u32)
    }

    fn allocate_trait_id(&self) -> TraitId {
        self.next_trait_id()
    }

    fn allocate_requirement_id(&self) -> TraitRequirementId {
        self.next_requirement_id()
    }
}

pub(crate) fn trait_this_name(string_table: &mut StringTable) -> StringId {
    string_table.intern(TRAIT_THIS_NAME)
}

pub(crate) fn requirement_parameter_from_type(
    name: InternedPath,
    value_mode: ValueMode,
    type_id: TypeId,
    location: SourceLocation,
) -> ResolvedTraitParameter {
    ResolvedTraitParameter {
        name,
        value_mode,
        type_id,
        location,
    }
}
