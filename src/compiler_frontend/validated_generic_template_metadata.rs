//! Validated generic-function template body artefacts retained as compiler metadata.
//!
//! WHAT: owns the one total extraction/join owner that moves the already-validated
//! [`GenericFunctionTemplate`] body payloads for directly exported generic free functions out of
//! the donor-local AST template map and into one deterministic store keyed by the stable
//! [`OriginFunctionId`] already retained by the [`PublicInterfaceDraft`].
//!
//! WHY: locked decision 10 in the canonical-module plan separates consumer-visible generic
//! semantic identity (the draft's [`PublicGenericTemplateDescriptor`]) from the declaring
//! module's retained template body. This store is one validated body-artefact checkpoint for the
//! future build-owned generated sidecar worklist (R3), not public semantic identity and not a
//! backend-consumed artefact yet.
//!
//! The retained [`GenericFunctionTemplate`] is the one existing body payload produced during
//! signature resolution and body validation. It carries the body tokens, resolved signature,
//! generic parameter list handle, source identity and declaration location. It is TIR-free,
//! contains no `Rc`, `RefCell`, `Ast`, `TemplateIrStore` or `TypeEnvironment`, and is `Send`
//! across directory compilation.
//!
//! This is a body-artefact checkpoint only, not the complete materialisation context. Complete
//! materialisation also needs declaration, file-visibility, generic/type and related frontend
//! context that this slice intentionally does not retain; that context is deferred to a later
//! bounded R2/R3 slice.
//!
//! Boundary: the store lives on [`ModuleCompilerMetadata`] during compilation. The legacy
//! flat-module handoff drops it before string-table remap because the retained
//! [`FunctionSignature`] carries donor-local `StringId`s whose remap owner is not in scope for
//! this slice. R3 will consume the store for the generated sidecar worklist before that drop.

use crate::compiler_frontend::ast::generic_functions::GenericFunctionTemplate;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::public_interface_draft::{
    PublicDeclarationRecord, PublicDeclarationSemantics, PublicInterfaceDraft,
};
use crate::compiler_frontend::semantic_identity::{OriginDeclarationId, OriginFunctionId};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use rustc_hash::{FxHashMap, FxHashSet};

// Verifies the retained template payload is `Send` across directory compilation. The
// `GenericFunctionTemplate` carries only owned token, signature, path, location and `u32`
// handle data: no `Rc`, `RefCell`, `Ast`, `TemplateIrStore` or `TypeEnvironment`.
const _: () = {
    const fn assert_send<T: Send>() {}
    assert_send::<GenericFunctionTemplate>();
    assert_send::<ValidatedGenericTemplateArtefact>();
    assert_send::<ValidatedGenericTemplateStore>();
};

/// One validated generic-function template body artefact retained as compiler metadata.
///
/// WHAT: pairs the stable [`OriginFunctionId`] of one directly exported generic free function
/// with the one existing [`GenericFunctionTemplate`] body payload produced during signature
/// resolution and body validation. The template carries the body tokens, resolved signature,
/// generic parameter list handle, source identity and declaration location: a validated
/// body-artefact checkpoint, not the complete materialisation context (see module docs).
///
/// WHY: keyed by the exact stable origin already retained by the draft so aliases, declaration
/// order, local paths and source positions never define artefact identity. The template is moved,
/// not cloned: there is no second body-token representation.
#[derive(Debug, Clone)]
pub(crate) struct ValidatedGenericTemplateArtefact {
    pub(crate) origin: OriginFunctionId,
    // The template body payload is retained as compiler metadata for the future generated
    // sidecar worklist (R3). Production code moves the artefact into the store and drops it at
    // the legacy handoff; test accessors read the field until R3 consumes it.
    #[allow(dead_code)]
    pub(crate) template: GenericFunctionTemplate,
}

/// Deterministic store of validated generic-template body artefacts for one compiled module.
///
/// WHAT: owns zero or one artefact per directly exported generic free-function origin and none
/// for non-generic, private or receiver-method functions. Artefacts are sorted by origin for
/// deterministic iteration independent of hash-map or declaration-scheduling order.
///
/// WHY: gives [`crate::build_system::build::ModuleCompilerMetadata`] one owned lane for
/// validated generic-template body artefacts. R3 will consume this checkpoint alongside the
/// declaration, file-visibility, generic/type and related frontend context this slice
/// intentionally does not retain, so concrete instances can be materialised without reopening
/// donor AST state.
#[derive(Debug, Clone, Default)]
pub(crate) struct ValidatedGenericTemplateStore {
    // The artefacts vector is retained as compiler metadata for the future generated sidecar
    // worklist (R3). Production code constructs and drops the store at the legacy handoff;
    // test accessors read the field until R3 consumes it.
    #[allow(dead_code)]
    artefacts: Vec<ValidatedGenericTemplateArtefact>,
}

impl ValidatedGenericTemplateStore {
    /// Construct a store from already-validated, already-joined artefacts.
    ///
    /// Compiler-internal: the extraction/join owner is the sole caller. It passes artefacts in
    /// the documented deterministic origin order.
    pub(crate) fn from_artefacts(artefacts: Vec<ValidatedGenericTemplateArtefact>) -> Self {
        Self { artefacts }
    }

    /// Whether the store contains no artefacts.
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.artefacts.is_empty()
    }

    /// The number of retained artefacts.
    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.artefacts.len()
    }

    /// Read-only access to the retained artefacts in deterministic origin order.
    #[cfg(test)]
    pub(crate) fn artefacts(&self) -> &[ValidatedGenericTemplateArtefact] {
        &self.artefacts
    }
}

/// Extract validated generic-template body artefacts for directly exported generic free
/// functions.
///
/// WHAT: the sole total extraction/join owner. It runs after generic body validation and before
/// HIR consumes AST state. It joins each exported generic free-function [`OriginFunctionId`]
/// already retained by the [`PublicInterfaceDraft`] to the one existing
/// [`GenericFunctionTemplate`] by defining name, moves the template out of the donor-local map,
/// and returns one deterministic store keyed by origin.
///
/// WHY: the draft is the authority for which exported free functions are generic (via
/// [`PublicGenericTemplateDescriptor`]). The template map is the authority for the validated body
/// payload. Joining by defining name — the last path component shared by the header `src_path`,
/// the template `function_path` and the export binding `public_name` — preserves the stable
/// origin as artefact identity without path, alias or order dependence.
///
/// Total validation:
/// - A draft generic free function with no matching template is a `CompilerError`: the
///   declaring module validated a generic body, so a missing template is an internal invariant
///   violation.
/// - A duplicate origin in the draft is a `CompilerError`.
/// - A template map key that does not equal the template's own `function_path` is a
///   `CompilerError`: the donor-local map and the template disagree on identity.
/// - A duplicate template defining name (even via distinct donor paths) is a `CompilerError`.
/// - A template whose defining name matches an exported non-generic free function is a
///   `CompilerError`: the draft says the function is non-generic but a generic template exists.
/// - A template whose defining name does not match any exported free function is a private
///   generic function and remains an intentional exclusion.
///
/// [`PublicGenericTemplateDescriptor`]: crate::compiler_frontend::public_interface_draft::PublicGenericTemplateDescriptor
pub(crate) fn extract_validated_generic_template_artefacts(
    draft: &PublicInterfaceDraft,
    templates: FxHashMap<InternedPath, GenericFunctionTemplate>,
    string_table: &StringTable,
) -> Result<ValidatedGenericTemplateStore, CompilerError> {
    // Collect exported generic free-function origins and all exported free-function defining
    // names from the draft. The draft is the authority for exported declaration identity.
    let mut generic_origins: Vec<OriginFunctionId> = Vec::new();
    let mut exported_free_function_names: FxHashSet<String> = FxHashSet::default();

    for record in &draft.declarations {
        let PublicDeclarationRecord { origin, semantics } = record;

        let OriginDeclarationId::Function(function_origin) = origin else {
            continue;
        };

        let defining_name = function_origin.defining_name().to_owned();
        exported_free_function_names.insert(defining_name.clone());

        if let PublicDeclarationSemantics::Function(function_semantics) = semantics
            && function_semantics.generic_template.is_some()
        {
            generic_origins.push(function_origin.clone());
        }
    }

    // Detect duplicate generic origins in the draft. Each unique declaration origin produces one
    // draft record, so a duplicate is an internal invariant violation.
    let mut seen_origins: FxHashSet<&OriginFunctionId> = FxHashSet::default();
    for origin in &generic_origins {
        if !seen_origins.insert(origin) {
            return Err(CompilerError::compiler_error(format!(
                "validated generic-template extraction: the draft carries a duplicate generic \
                 free-function origin for '{}'; a duplicate origin is an internal invariant \
                 violation",
                origin.defining_name()
            )));
        }
    }

    // Index the donor-local template map by defining name. Each template path is unique within
    // the module, so two templates with the same defining name is an internal invariant
    // violation.
    let mut templates_by_name: FxHashMap<String, GenericFunctionTemplate> = FxHashMap::default();

    for (path, template) in templates {
        // The map key must equal the template's own `function_path`. The donor-local template
        // map is keyed by the same path stored on the template, so a disagreement is an internal
        // invariant violation. The stable origin remains the artefact identity; the path is used
        // only for this donor-local defining-name join inside the defining module.
        if path != template.function_path {
            return Err(CompilerError::compiler_error(format!(
                "validated generic-template extraction: a generic function template map key {:?} \
                 does not equal the template's own function_path {:?}; a map-key/template-path \
                 mismatch is an internal invariant violation",
                path, template.function_path
            )));
        }

        let name = path.name_str(string_table).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "validated generic-template extraction: a generic function template path {:?} \
                 has no resolvable defining name; a missing defining name is an internal \
                 invariant violation",
                path
            ))
        })?;

        if templates_by_name
            .insert(name.to_owned(), template)
            .is_some()
        {
            return Err(CompilerError::compiler_error(format!(
                "validated generic-template extraction: two generic function templates share the \
                 defining name '{}'; a duplicate template name is an internal invariant \
                 violation",
                name
            )));
        }
    }

    // Join each exported generic free-function origin to its template by defining name. A
    // missing template for a draft-identified generic export is an internal invariant violation.
    let mut artefacts: Vec<ValidatedGenericTemplateArtefact> =
        Vec::with_capacity(generic_origins.len());

    for origin in generic_origins {
        let defining_name = origin.defining_name();

        let template = templates_by_name.remove(defining_name).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "validated generic-template extraction: the exported generic free function \
                     '{}' has no matching validated template; a missing exported template is an \
                     internal invariant violation",
                defining_name
            ))
        })?;

        artefacts.push(ValidatedGenericTemplateArtefact { origin, template });
    }

    // Any remaining template whose defining name matches an exported non-generic free function
    // is a generic/non-generic mismatch: the draft says the function is non-generic but a generic
    // template exists. A remaining template whose name does not match any exported free function
    // is a private generic function and remains an intentional exclusion.
    for leftover_name in templates_by_name.keys() {
        if exported_free_function_names.contains(leftover_name) {
            return Err(CompilerError::compiler_error(format!(
                "validated generic-template extraction: the exported free function '{}' has a \
                 validated generic template but the draft marks it non-generic; a \
                 generic/non-generic mismatch is an internal invariant violation",
                leftover_name
            )));
        }
    }

    // Deterministic order independent of hash-map iteration. All origins in one module's
    // store share the same `StableModuleOriginIdentity`, so sorting by defining name alone is
    // unambiguous within a single module's store.
    artefacts.sort_by(|left, right| {
        left.origin
            .defining_name()
            .cmp(right.origin.defining_name())
    });

    Ok(ValidatedGenericTemplateStore::from_artefacts(artefacts))
}

#[cfg(test)]
#[path = "tests/validated_generic_template_metadata_tests.rs"]
mod tests;
