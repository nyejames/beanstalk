//! Trait conformance evidence metadata and validation.
//!
//! WHAT: validates explicit `Type must TRAIT` declarations against resolved trait requirements
//! and same-file receiver methods, then stores the selected requirement-to-method mapping.
//! WHY: later trait-call and generic-bound phases need indexed evidence facts. They must not
//! rescan conformance headers or infer structural conformance at call sites.

use crate::compiler_frontend::ast::statements::functions::ReturnSlot;
use crate::compiler_frontend::ast::{
    ReceiverMethodCatalog, ReceiverMethodEntry, ReceiverMethodKind,
};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, DiagnosticLabel, DiagnosticLabelMessage,
    InvalidTraitConformanceReason,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{BuiltinTypeKey, TypeId};
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, ReceiverKey};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::{
    FileVisibility, HeaderImportEnvironment,
};
use crate::compiler_frontend::headers::parse_file_headers::{FileRole, Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringIdRemap, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::compiler_frontend::traits::definitions::{
    ResolvedTraitDefinition, ResolvedTraitRequirement, TraitReceiverRequirement,
};
use crate::compiler_frontend::traits::environment::{DISPLAYABLE_TRAIT_NAME, TraitEnvironment};
use crate::compiler_frontend::traits::ids::{TraitEvidenceId, TraitId, TraitRequirementId};
use crate::compiler_frontend::traits::syntax::{
    ConformanceTargetKind, ConformanceTargetSyntax, TraitReferenceSyntax,
};
use rustc_hash::FxHashMap;

/// The ownership class of an evidence record.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[allow(dead_code)] // Compiler-owned builtin evidence is scaffolded but not registered yet.
pub(crate) enum TraitEvidenceKind {
    Canonical,
    FileLocalExtension,
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
    file_local_by_source_target_and_trait:
        FxHashMap<(InternedPath, TypeId, TraitId), TraitEvidenceId>,
    builtin_by_target_and_trait: FxHashMap<(TypeId, TraitId), TraitEvidenceId>,
    reusable_by_target: FxHashMap<TypeId, Vec<TraitEvidenceId>>,
    file_local_by_source_and_target: FxHashMap<(InternedPath, TypeId), Vec<TraitEvidenceId>>,
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

    #[allow(dead_code)] // File-local evidence is unavailable to generic bounds in current lowering.
    pub(crate) fn file_local_for(
        &self,
        source_file: &InternedPath,
        target_type_id: TypeId,
        trait_id: TraitId,
    ) -> Option<TraitEvidenceId> {
        self.file_local_by_source_target_and_trait
            .get(&(source_file.clone(), target_type_id, trait_id))
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
    /// WHAT: exposes reusable canonical/builtin evidence for a target type plus file-local
    /// evidence authored in the current source file.
    /// WHY: receiver-call parsing needs an evidence-backed fallback without scanning raw
    /// conformance headers or accidentally exporting file-local extension evidence.
    pub(crate) fn receiver_fallback_candidates(
        &self,
        target_type_id: TypeId,
        source_file: &InternedPath,
    ) -> Vec<TraitEvidenceId> {
        let reusable = self
            .reusable_by_target
            .get(&target_type_id)
            .into_iter()
            .flatten()
            .copied();
        let file_local = self
            .file_local_by_source_and_target
            .get(&(source_file.clone(), target_type_id))
            .into_iter()
            .flatten()
            .copied();

        reusable.chain(file_local).collect()
    }

    pub(crate) fn remap_string_ids(&mut self, remap: &StringIdRemap) {
        for definition in &mut self.evidence {
            definition.source_file.remap_string_ids(remap);
            definition.declaration_location.remap_string_ids(remap);

            for requirement in &mut definition.requirements {
                requirement.method_path.remap_string_ids(remap);
            }
        }

        self.rebuild_indexes();
    }

    #[allow(dead_code)] // No compiler-owned builtin conformances are registered yet.
    pub(crate) fn insert_builtin(&mut self, mut definition: TraitEvidenceDefinition) {
        let id = TraitEvidenceId(self.evidence.len() as u32);
        definition.id = id;

        self.index_definition(&definition);
        self.evidence.push(definition);
    }

    fn insert_validated(&mut self, mut definition: TraitEvidenceDefinition) {
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

            TraitEvidenceKind::FileLocalExtension => {
                self.file_local_by_source_target_and_trait.insert(
                    (
                        definition.source_file.clone(),
                        definition.target_type_id,
                        definition.trait_id,
                    ),
                    definition.id,
                );
                self.file_local_by_source_and_target
                    .entry((definition.source_file.clone(), definition.target_type_id))
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

    fn rebuild_indexes(&mut self) {
        self.canonical_by_target_and_trait.clear();
        self.file_local_by_source_target_and_trait.clear();
        self.builtin_by_target_and_trait.clear();
        self.reusable_by_target.clear();
        self.file_local_by_source_and_target.clear();

        let definitions = self.evidence.clone();
        for definition in &definitions {
            self.index_definition(definition);
        }
    }
}

/// Inputs needed to validate evidence after trait definitions and receiver methods exist.
pub(crate) struct ValidateTraitEvidenceInput<'a> {
    pub(crate) sorted_headers: &'a [Header],
    pub(crate) trait_environment: &'a TraitEnvironment,
    pub(crate) receiver_methods: &'a ReceiverMethodCatalog,
    pub(crate) type_environment: &'a TypeEnvironment,
    pub(crate) import_environment: &'a HeaderImportEnvironment,
    pub(crate) nominal_type_ids_by_path: &'a FxHashMap<InternedPath, TypeId>,
    pub(crate) struct_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) choice_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) string_table: &'a mut StringTable,
}

#[derive(Clone)]
struct ConformanceTarget {
    type_id: TypeId,
    receiver_key: ReceiverKey,
    path: Option<InternedPath>,
    is_generic_constructor: bool,
    evidence_kind: TraitEvidenceKind,
    required_method_kind: ReceiverMethodKind,
}

struct PendingConformanceEvidence {
    target: ConformanceTarget,
    target_name: StringId,
    trait_id: TraitId,
    trait_name: StringId,
    source_file: InternedPath,
    declaration_location: SourceLocation,
    trait_location: SourceLocation,
}

struct ResolveConformanceTargetContext<'a> {
    conformance_source_file: &'a InternedPath,
    visibility: &'a FileVisibility,
    nominal_type_ids_by_path: &'a FxHashMap<InternedPath, TypeId>,
    struct_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    choice_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    type_environment: &'a TypeEnvironment,
    string_table: &'a StringTable,
}

struct ImplementationMethod<'a> {
    entry: &'a ReceiverMethodEntry,
    receiver_type_id: TypeId,
}

struct RequirementValidationContext<'a, 'strings> {
    receiver_methods: &'a ReceiverMethodCatalog,
    type_environment: &'a TypeEnvironment,
    target_name: StringId,
    trait_name: StringId,
    conformance_location: SourceLocation,
    string_table: &'strings mut StringTable,
}

/// Validate explicit conformance declarations and store indexed evidence facts.
///
/// WHAT: classifies each declaration as canonical or file-local extension evidence before
/// matching trait requirements against receiver methods in the declaring file.
/// WHY: later dispatch phases need stable evidence indexes and must not rediscover conformance
/// headers or infer structural conformance from arbitrary matching methods.
pub(crate) fn validate_trait_evidence(
    input: ValidateTraitEvidenceInput<'_>,
) -> Result<TraitEvidenceEnvironment, CompilerDiagnostic> {
    let mut evidence_environment = TraitEvidenceEnvironment::new();
    let mut pending_evidence = Vec::new();
    let mut pending_canonical_locations: FxHashMap<(TypeId, TraitId), SourceLocation> =
        FxHashMap::default();
    let mut pending_file_local_locations: FxHashMap<
        (InternedPath, TypeId, TraitId),
        SourceLocation,
    > = FxHashMap::default();

    for header in input.sorted_headers {
        let HeaderKind::TraitConformance { conformance } = &header.kind else {
            continue;
        };

        if header.file_role == FileRole::ModuleFacade {
            return Err(invalid_conformance(
                conformance.target.name,
                conformance.traits.first().map(|trait_ref| trait_ref.name),
                InvalidTraitConformanceReason::ModuleFacade,
                conformance.location.clone(),
                Vec::new(),
            ));
        }

        let visibility = input
            .import_environment
            .visibility_for(&header.source_file)
            .map_err(|_| {
                invalid_conformance(
                    conformance.target.name,
                    conformance.traits.first().map(|trait_ref| trait_ref.name),
                    InvalidTraitConformanceReason::NonCanonicalTarget,
                    conformance.location.clone(),
                    Vec::new(),
                )
            })?;
        let conformance_source_file = header.canonical_source_file(input.string_table);

        let target_context = ResolveConformanceTargetContext {
            conformance_source_file: &conformance_source_file,
            visibility,
            nominal_type_ids_by_path: input.nominal_type_ids_by_path,
            struct_source_by_path: input.struct_source_by_path,
            choice_source_by_path: input.choice_source_by_path,
            type_environment: input.type_environment,
            string_table: input.string_table,
        };
        let target = resolve_conformance_target(&conformance.target, target_context)?;

        for trait_ref in &conformance.traits {
            let trait_id = resolve_trait_reference(
                trait_ref,
                visibility,
                input.trait_environment,
                input.string_table,
            )?;

            if let Some(existing_id) = evidence_environment.builtin_for(target.type_id, trait_id) {
                let previous_location = evidence_environment
                    .get(existing_id)
                    .map(|definition| definition.declaration_location.clone());

                return Err(invalid_conformance(
                    conformance.target.name,
                    Some(trait_ref.name),
                    InvalidTraitConformanceReason::BuiltinEvidenceOverride,
                    trait_ref.location.clone(),
                    previous_declaration_label(previous_location),
                ));
            }

            match target.evidence_kind {
                TraitEvidenceKind::Canonical => {
                    let key = (target.type_id, trait_id);
                    if let Some(previous_location) = pending_canonical_locations.get(&key) {
                        return Err(invalid_conformance(
                            conformance.target.name,
                            Some(trait_ref.name),
                            InvalidTraitConformanceReason::DuplicateCanonicalEvidence,
                            trait_ref.location.clone(),
                            previous_declaration_label(Some(previous_location.clone())),
                        ));
                    }

                    pending_canonical_locations.insert(key, conformance.location.clone());
                }

                TraitEvidenceKind::FileLocalExtension => {
                    let key = (conformance_source_file.clone(), target.type_id, trait_id);
                    if let Some(previous_location) = pending_file_local_locations.get(&key) {
                        return Err(invalid_conformance(
                            conformance.target.name,
                            Some(trait_ref.name),
                            InvalidTraitConformanceReason::DuplicateFileLocalExtensionEvidence,
                            trait_ref.location.clone(),
                            previous_declaration_label(Some(previous_location.clone())),
                        ));
                    }

                    pending_file_local_locations.insert(key, conformance.location.clone());
                }

                TraitEvidenceKind::Builtin => {}
            }

            pending_evidence.push(PendingConformanceEvidence {
                target: target.clone(),
                target_name: conformance.target.name,
                trait_id,
                trait_name: trait_ref.name,
                source_file: conformance_source_file.clone(),
                declaration_location: conformance.location.clone(),
                trait_location: trait_ref.location.clone(),
            });
        }
    }

    for pending in &pending_evidence {
        if pending.target.evidence_kind == TraitEvidenceKind::FileLocalExtension
            && let Some(canonical_location) =
                pending_canonical_locations.get(&(pending.target.type_id, pending.trait_id))
        {
            return Err(invalid_conformance(
                pending.target_name,
                Some(pending.trait_name),
                InvalidTraitConformanceReason::FileLocalExtensionOverridesCanonicalEvidence,
                pending.trait_location.clone(),
                previous_declaration_label(Some(canonical_location.clone())),
            ));
        }
    }

    for pending in pending_evidence {
        let Some(trait_definition) = input.trait_environment.get(pending.trait_id) else {
            return Err(CompilerDiagnostic::unknown_trait_name(
                pending.trait_name,
                pending.trait_location.clone(),
            ));
        };

        let mut requirement_context = RequirementValidationContext {
            receiver_methods: input.receiver_methods,
            type_environment: input.type_environment,
            target_name: pending.target_name,
            trait_name: pending.trait_name,
            conformance_location: pending.declaration_location.clone(),
            string_table: input.string_table,
        };
        let requirement_methods = validate_requirements(
            trait_definition,
            &pending.target,
            &pending.source_file,
            &mut requirement_context,
        )?;

        let evidence = TraitEvidenceDefinition {
            id: TraitEvidenceId(0),
            kind: pending.target.evidence_kind,
            target_type_id: pending.target.type_id,
            trait_id: pending.trait_id,
            source_file: pending.source_file,
            declaration_location: pending.declaration_location,
            requirements: requirement_methods,
        };

        evidence_environment.insert_validated(evidence);
    }

    Ok(evidence_environment)
}

fn resolve_conformance_target(
    target: &ConformanceTargetSyntax,
    context: ResolveConformanceTargetContext<'_>,
) -> Result<ConformanceTarget, CompilerDiagnostic> {
    if target.kind == ConformanceTargetKind::SpecializedGenericInstance {
        return Err(CompilerDiagnostic::deferred_feature_reason(
            DeferredFeatureReason::NamedFeature {
                feature: target.name,
            },
            target.location.clone(),
        ));
    }

    if let Some(builtin_target) =
        resolve_builtin_scalar_target(target.name, context.type_environment, context.string_table)
    {
        return Ok(builtin_target);
    }

    if let Some(symbol_id) = context
        .visibility
        .visible_external_symbols
        .get(&target.name)
        && let ExternalSymbolId::Type(external_type_id) = symbol_id
    {
        let Some(type_id) = context
            .type_environment
            .type_id_for_external(*external_type_id)
        else {
            return Err(invalid_conformance(
                target.name,
                None,
                InvalidTraitConformanceReason::NonCanonicalTarget,
                target.location.clone(),
                Vec::new(),
            ));
        };

        return Ok(ConformanceTarget {
            type_id,
            receiver_key: ReceiverKey::External(*external_type_id),
            path: None,
            is_generic_constructor: false,
            evidence_kind: TraitEvidenceKind::FileLocalExtension,
            required_method_kind: ReceiverMethodKind::FileLocalExtension,
        });
    }

    if context
        .visibility
        .visible_type_alias_names
        .contains_key(&target.name)
    {
        return Err(invalid_conformance(
            target.name,
            None,
            InvalidTraitConformanceReason::AliasTarget,
            target.location.clone(),
            Vec::new(),
        ));
    }

    let Some(target_path) = context.visibility.visible_source_names.get(&target.name) else {
        return Err(CompilerDiagnostic::unknown_type_name(
            target.name,
            target.location.clone(),
        ));
    };
    let Some(type_id) = context.nominal_type_ids_by_path.get(target_path).copied() else {
        return Err(invalid_conformance(
            target.name,
            None,
            InvalidTraitConformanceReason::NonCanonicalTarget,
            target.location.clone(),
            Vec::new(),
        ));
    };

    let Some(definition) = context.type_environment.get(type_id) else {
        return Err(invalid_conformance(
            target.name,
            None,
            InvalidTraitConformanceReason::NonCanonicalTarget,
            target.location.clone(),
            Vec::new(),
        ));
    };

    match definition {
        TypeDefinition::Struct(definition) => {
            let target_is_declared_here = context
                .struct_source_by_path
                .get(&definition.path)
                .is_some_and(|source_file| source_file == context.conformance_source_file);
            let evidence_kind = evidence_kind_for_source_target(target_is_declared_here);
            let required_method_kind = method_kind_for_source_target(target_is_declared_here);

            Ok(ConformanceTarget {
                type_id,
                receiver_key: ReceiverKey::Struct(definition.path.clone()),
                path: Some(definition.path.clone()),
                is_generic_constructor: definition.generic_parameters.is_some(),
                evidence_kind,
                required_method_kind,
            })
        }

        TypeDefinition::Choice(definition) => {
            let target_is_declared_here = context
                .choice_source_by_path
                .get(&definition.path)
                .is_some_and(|source_file| source_file == context.conformance_source_file);
            let evidence_kind = evidence_kind_for_source_target(target_is_declared_here);
            let required_method_kind = method_kind_for_source_target(target_is_declared_here);

            Ok(ConformanceTarget {
                type_id,
                receiver_key: ReceiverKey::Choice(definition.path.clone()),
                path: Some(definition.path.clone()),
                is_generic_constructor: definition.generic_parameters.is_some(),
                evidence_kind,
                required_method_kind,
            })
        }

        _ => Err(invalid_conformance(
            target.name,
            None,
            InvalidTraitConformanceReason::NonCanonicalTarget,
            target.location.clone(),
            Vec::new(),
        )),
    }
}

fn resolve_builtin_scalar_target(
    name: StringId,
    type_environment: &TypeEnvironment,
    string_table: &StringTable,
) -> Option<ConformanceTarget> {
    let (builtin_key, receiver) = match string_table.resolve(name) {
        "Int" => (BuiltinTypeKey::Int, BuiltinScalarReceiver::Int),
        "Float" => (BuiltinTypeKey::Float, BuiltinScalarReceiver::Float),
        "Bool" => (BuiltinTypeKey::Bool, BuiltinScalarReceiver::Bool),
        "String" => (BuiltinTypeKey::String, BuiltinScalarReceiver::String),
        "Char" => (BuiltinTypeKey::Char, BuiltinScalarReceiver::Char),
        _ => return None,
    };

    let type_id = type_environment.type_id_for_builtin(builtin_key)?;

    Some(ConformanceTarget {
        type_id,
        receiver_key: ReceiverKey::BuiltinScalar(receiver),
        path: None,
        is_generic_constructor: false,
        evidence_kind: TraitEvidenceKind::FileLocalExtension,
        required_method_kind: ReceiverMethodKind::Canonical,
    })
}

fn evidence_kind_for_source_target(target_is_declared_here: bool) -> TraitEvidenceKind {
    if target_is_declared_here {
        TraitEvidenceKind::Canonical
    } else {
        TraitEvidenceKind::FileLocalExtension
    }
}

fn method_kind_for_source_target(target_is_declared_here: bool) -> ReceiverMethodKind {
    if target_is_declared_here {
        ReceiverMethodKind::Canonical
    } else {
        ReceiverMethodKind::FileLocalExtension
    }
}

fn resolve_trait_reference(
    trait_ref: &TraitReferenceSyntax,
    visibility: &FileVisibility,
    trait_environment: &TraitEnvironment,
    string_table: &mut StringTable,
) -> Result<TraitId, CompilerDiagnostic> {
    if let Some(path) = visibility.visible_trait_names.get(&trait_ref.name)
        && let Some(id) = trait_environment.id_for_path(path)
    {
        return Ok(id);
    }

    let displayable_name = string_table.intern(DISPLAYABLE_TRAIT_NAME);
    if trait_ref.name == displayable_name
        && let Some(id) = trait_environment.displayable_trait_id()
    {
        return Ok(id);
    }

    Err(CompilerDiagnostic::unknown_trait_name(
        trait_ref.name,
        trait_ref.location.clone(),
    ))
}

fn validate_requirements(
    trait_definition: &ResolvedTraitDefinition,
    target: &ConformanceTarget,
    conformance_source_file: &InternedPath,
    context: &mut RequirementValidationContext<'_, '_>,
) -> Result<Vec<TraitRequirementEvidence>, CompilerDiagnostic> {
    let mut requirement_methods = Vec::with_capacity(trait_definition.requirements.len());

    for requirement in &trait_definition.requirements {
        let method = find_same_file_method(
            context.receiver_methods,
            target,
            requirement.name,
            conformance_source_file,
            context.type_environment,
        )
        .ok_or_else(|| {
            invalid_conformance(
                context.target_name,
                Some(context.trait_name),
                InvalidTraitConformanceReason::MissingMethod {
                    requirement_name: requirement.name,
                },
                context.conformance_location.clone(),
                requirement_label(requirement, context.string_table),
            )
        })?;

        validate_requirement_signature(requirement, trait_definition.this_type, &method, context)?;

        requirement_methods.push(TraitRequirementEvidence {
            requirement_id: requirement.id,
            method_path: method.entry.function_path.clone(),
        });
    }

    Ok(requirement_methods)
}

fn find_same_file_method<'a>(
    receiver_methods: &'a ReceiverMethodCatalog,
    target: &ConformanceTarget,
    method_name: StringId,
    conformance_source_file: &InternedPath,
    type_environment: &TypeEnvironment,
) -> Option<ImplementationMethod<'a>> {
    let entries = receiver_methods
        .by_receiver_and_name
        .get(&(target.receiver_key.clone(), method_name))?;

    for entry in entries {
        if entry.source_file != *conformance_source_file
            || entry.kind != target.required_method_kind
        {
            continue;
        }

        let Some(receiver_parameter) = entry.signature.parameters.first() else {
            continue;
        };
        let receiver_type_id = receiver_parameter.value.type_id;
        if receiver_type_matches_target(receiver_type_id, target, type_environment) {
            return Some(ImplementationMethod {
                entry,
                receiver_type_id,
            });
        }
    }

    None
}

fn receiver_type_matches_target(
    receiver_type_id: TypeId,
    target: &ConformanceTarget,
    type_environment: &TypeEnvironment,
) -> bool {
    if receiver_type_id == target.type_id {
        return true;
    }

    if !target.is_generic_constructor {
        return false;
    }

    let Some(TypeDefinition::GenericInstance(instance)) = type_environment.get(receiver_type_id)
    else {
        return false;
    };
    type_environment
        .nominal_path_by_id(instance.base)
        .is_some_and(|base_path| target.path.as_ref().is_some_and(|path| base_path == path))
}

fn validate_requirement_signature(
    requirement: &ResolvedTraitRequirement,
    trait_this_type: TypeId,
    method: &ImplementationMethod<'_>,
    context: &mut RequirementValidationContext<'_, '_>,
) -> Result<(), CompilerDiagnostic> {
    let required_receiver_mutable = match requirement.receiver {
        TraitReceiverRequirement::Immutable { .. } => false,
        TraitReceiverRequirement::Mutable { .. } => true,
    };

    if required_receiver_mutable != method.entry.receiver_mutable {
        return Err(invalid_conformance(
            context.target_name,
            Some(context.trait_name),
            InvalidTraitConformanceReason::ReceiverMutabilityMismatch {
                requirement_name: requirement.name,
            },
            context.conformance_location.clone(),
            requirement_and_method_labels(requirement, method.entry, context.string_table),
        ));
    }

    validate_parameters(requirement, trait_this_type, method, context)?;

    validate_returns(requirement, trait_this_type, method, context)
}

fn validate_parameters(
    requirement: &ResolvedTraitRequirement,
    trait_this_type: TypeId,
    method: &ImplementationMethod<'_>,
    context: &mut RequirementValidationContext<'_, '_>,
) -> Result<(), CompilerDiagnostic> {
    let method_parameters = method
        .entry
        .signature
        .parameters
        .iter()
        .skip(1)
        .collect::<Vec<_>>();
    if requirement.parameters.len() != method_parameters.len() {
        return Err(invalid_conformance(
            context.target_name,
            Some(context.trait_name),
            InvalidTraitConformanceReason::ParameterCountMismatch {
                requirement_name: requirement.name,
                expected: requirement.parameters.len(),
                found: method_parameters.len(),
            },
            context.conformance_location.clone(),
            requirement_and_method_labels(requirement, method.entry, context.string_table),
        ));
    }

    for (index, (required, actual)) in requirement
        .parameters
        .iter()
        .zip(method_parameters.iter())
        .enumerate()
    {
        if required.value_mode != actual.value.value_mode {
            return Err(invalid_conformance(
                context.target_name,
                Some(context.trait_name),
                InvalidTraitConformanceReason::ParameterModeMismatch {
                    requirement_name: requirement.name,
                    parameter_index: index + 1,
                },
                context.conformance_location.clone(),
                requirement_and_method_labels(requirement, method.entry, context.string_table),
            ));
        }

        let expected_type =
            replace_trait_this(required.type_id, trait_this_type, method.receiver_type_id);
        if expected_type != actual.value.type_id {
            return Err(invalid_conformance(
                context.target_name,
                Some(context.trait_name),
                InvalidTraitConformanceReason::ParameterTypeMismatch {
                    requirement_name: requirement.name,
                    parameter_index: index + 1,
                    expected_type,
                    found_type: actual.value.type_id,
                },
                context.conformance_location.clone(),
                requirement_and_method_labels(requirement, method.entry, context.string_table),
            ));
        }
    }

    Ok(())
}

fn validate_returns(
    requirement: &ResolvedTraitRequirement,
    trait_this_type: TypeId,
    method: &ImplementationMethod<'_>,
    context: &mut RequirementValidationContext<'_, '_>,
) -> Result<(), CompilerDiagnostic> {
    let method_returns = &method.entry.signature.returns;
    if requirement.returns.len() != method_returns.len() {
        return Err(invalid_conformance(
            context.target_name,
            Some(context.trait_name),
            InvalidTraitConformanceReason::ReturnCountMismatch {
                requirement_name: requirement.name,
                expected: requirement.returns.len(),
                found: method_returns.len(),
            },
            context.conformance_location.clone(),
            requirement_and_method_labels(requirement, method.entry, context.string_table),
        ));
    }

    for (index, (required, actual)) in requirement.returns.iter().zip(method_returns).enumerate() {
        if required.channel != actual.channel {
            return Err(invalid_conformance(
                context.target_name,
                Some(context.trait_name),
                InvalidTraitConformanceReason::ReturnChannelMismatch {
                    requirement_name: requirement.name,
                    return_index: index + 1,
                },
                context.conformance_location.clone(),
                requirement_and_method_labels(requirement, method.entry, context.string_table),
            ));
        }

        let Some(actual_type) = return_type_id(actual) else {
            return Err(invalid_conformance(
                context.target_name,
                Some(context.trait_name),
                InvalidTraitConformanceReason::ReturnTypeMismatch {
                    requirement_name: requirement.name,
                    return_index: index + 1,
                    expected_type: replace_trait_this(
                        required.type_id,
                        trait_this_type,
                        method.receiver_type_id,
                    ),
                    found_type: method.receiver_type_id,
                },
                context.conformance_location.clone(),
                requirement_and_method_labels(requirement, method.entry, context.string_table),
            ));
        };

        let expected_type =
            replace_trait_this(required.type_id, trait_this_type, method.receiver_type_id);
        if expected_type != actual_type {
            return Err(invalid_conformance(
                context.target_name,
                Some(context.trait_name),
                InvalidTraitConformanceReason::ReturnTypeMismatch {
                    requirement_name: requirement.name,
                    return_index: index + 1,
                    expected_type,
                    found_type: actual_type,
                },
                context.conformance_location.clone(),
                requirement_and_method_labels(requirement, method.entry, context.string_table),
            ));
        }
    }

    Ok(())
}

fn return_type_id(return_slot: &ReturnSlot) -> Option<TypeId> {
    return_slot.type_id
}

fn replace_trait_this(
    type_id: TypeId,
    trait_this_type: TypeId,
    receiver_type_id: TypeId,
) -> TypeId {
    if type_id == trait_this_type {
        receiver_type_id
    } else {
        type_id
    }
}

fn invalid_conformance(
    target_name: StringId,
    trait_name: Option<StringId>,
    reason: InvalidTraitConformanceReason,
    primary_location: SourceLocation,
    mut secondary_labels: Vec<DiagnosticLabel>,
) -> CompilerDiagnostic {
    let mut labels = vec![DiagnosticLabel::primary(primary_location.clone())];
    labels.append(&mut secondary_labels);

    CompilerDiagnostic::invalid_trait_conformance(target_name, trait_name, reason, primary_location)
        .with_labels(labels)
}

fn previous_declaration_label(previous_location: Option<SourceLocation>) -> Vec<DiagnosticLabel> {
    previous_location
        .map(|location| {
            vec![DiagnosticLabel::secondary(
                location,
                Some(DiagnosticLabelMessage::PreviousDeclaration),
            )]
        })
        .unwrap_or_default()
}

fn requirement_label(
    requirement: &ResolvedTraitRequirement,
    string_table: &mut StringTable,
) -> Vec<DiagnosticLabel> {
    vec![DiagnosticLabel::secondary(
        requirement.location.clone(),
        Some(DiagnosticLabelMessage::RenderedText(
            string_table.intern("trait requirement"),
        )),
    )]
}

fn requirement_and_method_labels(
    requirement: &ResolvedTraitRequirement,
    method: &ReceiverMethodEntry,
    string_table: &mut StringTable,
) -> Vec<DiagnosticLabel> {
    vec![
        DiagnosticLabel::secondary(
            requirement.location.clone(),
            Some(DiagnosticLabelMessage::RenderedText(
                string_table.intern("trait requirement"),
            )),
        ),
        DiagnosticLabel::secondary(
            method
                .signature
                .parameters
                .first()
                .map(|parameter| parameter.value.location.clone())
                .unwrap_or_default(),
            Some(DiagnosticLabelMessage::RenderedText(
                string_table.intern("receiver method"),
            )),
        ),
    ]
}
