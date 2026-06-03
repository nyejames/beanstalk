//! Conformance target and trait reference resolution.
//!
//! WHAT: Resolves the semantic target type (struct, choice, builtin scalar, or external type)
//!       and the trait reference for an explicit `Type must TRAIT` conformance declaration.
//! WHY: Translates syntax-stage name spellings and visibility contexts into compile-time types
//!      and trait definition IDs so matching/validation can proceed.

use super::diagnostics::invalid_conformance;
use super::environment::TraitEvidenceKind;
use crate::compiler_frontend::ast::ReceiverMethodKind;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, InvalidTraitConformanceReason,
};
use crate::compiler_frontend::datatypes::definitions::TypeDefinition;
use crate::compiler_frontend::datatypes::environment::TypeEnvironment;
use crate::compiler_frontend::datatypes::ids::{BuiltinTypeKey, TypeId};
use crate::compiler_frontend::datatypes::{BuiltinScalarReceiver, ReceiverKey};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::FileVisibility;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::traits::environment::TraitEnvironment;
use crate::compiler_frontend::traits::ids::TraitId;
use crate::compiler_frontend::traits::syntax::{
    ConformanceTargetKind, ConformanceTargetSyntax, TraitReferenceSyntax,
};
use rustc_hash::FxHashMap;

#[derive(Clone)]
pub(super) struct ConformanceTarget {
    pub(super) type_id: TypeId,
    pub(super) receiver_key: ReceiverKey,
    pub(super) path: Option<InternedPath>,
    pub(super) is_generic_constructor: bool,
    pub(super) evidence_kind: TraitEvidenceKind,
    pub(super) required_method_kind: ReceiverMethodKind,
}

pub(super) struct ResolveConformanceTargetContext<'a> {
    pub(super) conformance_source_file: &'a InternedPath,
    pub(super) visibility: &'a FileVisibility,
    pub(super) nominal_type_ids_by_path: &'a FxHashMap<InternedPath, TypeId>,
    pub(super) struct_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(super) choice_source_by_path: &'a FxHashMap<InternedPath, InternedPath>,
    pub(super) type_environment: &'a TypeEnvironment,
    pub(super) string_table: &'a StringTable,
}

pub(super) fn resolve_conformance_target(
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
        required_method_kind: ReceiverMethodKind::FileLocalExtension,
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

pub(super) fn resolve_trait_reference(
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

    if let Some(id) = trait_environment.displayable_trait_id_for_name(trait_ref.name, string_table)
    {
        return Ok(id);
    }

    Err(CompilerDiagnostic::unknown_trait_name(
        trait_ref.name,
        trait_ref.location.clone(),
    ))
}
