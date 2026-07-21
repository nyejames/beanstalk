//! Compiler-owned stable semantic identity values that cross compiler and build stages.
//!
//! WHAT: owns the portable, hashable, cross-build identity values for source packages, canonical
//! module origins and exported declarations. These values are the stable semantic identity
//! contract used by later exported-declaration identity, public-interface binding and
//! cross-module call targets. The dense `ModuleId` and identity table remain Stage 0-owned and
//! live in `build_system::create_project_modules::module_identity`; this module owns only the
//! portable value types those dense handles refer to.
//! WHY: the compiler owns semantic identity. Stage 0 owns discovery, the dense assignment table
//! and structural ancestry, but the cross-build identity values must not be build-local. Moving
//! the value types here keeps one owner for the cross-stage identity vocabulary so later phases
//! embed stable origins without reaching into the build system and without leaking process-local
//! IDs, source files, declaration order or export aliases into identity.
//!
//! Boundary: reusable evidence identity is deliberately not owned here. Evidence identity needs
//! canonical target-type and trait/evidence semantics that a later phase owns, so this module
//! does not invent a string-based or placeholder evidence key.

use crate::builder_surface::PackageOrigin;
use crate::compiler_frontend::compiler_errors::CompilerError;

use std::path::{Component, Path};

/// Owned, hashable, cross-build identity for one source package within one build boundary.
///
/// WHAT: carries the package origin and the canonical package/project name. For the project
/// graph it is constructed from [`PackageOrigin::ProjectLocal`] and the exact configured
/// `Config.project_name`. It stores neither absolute filesystem paths nor process-local
/// string-table IDs, so the same logical package resolves to the same identity across checkout
/// roots, processes and cosmetic root-filename suffixes.
/// WHY: later exported declaration identities embed the package identity so origin identities
/// remain stable when source moves across machines or checkouts. Identity is never inferred from
/// checkout-directory names or absolute paths.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StablePackageIdentity {
    origin: PackageOrigin,
    name: String,
}

impl StablePackageIdentity {
    /// Project-local package identity for the project graph, from the configured project name.
    ///
    /// The configured name is preserved exactly as supplied. Validation of empty or malformed
    /// project names belongs to config/bootstrap owners and is intentionally not added here.
    pub(crate) fn project_local(project_name: &str) -> Self {
        Self {
            origin: PackageOrigin::ProjectLocal,
            name: project_name.to_owned(),
        }
    }

    /// The package origin classification.
    #[allow(dead_code)]
    pub(crate) fn origin(&self) -> PackageOrigin {
        self.origin
    }

    /// The canonical package/project name spelling.
    #[allow(dead_code)]
    pub(crate) fn name(&self) -> &str {
        &self.name
    }
}

/// The structural role of one canonical module root.
///
/// `Normal` roots (`#*.bst`) are entry candidates. `Support` roots (`+*.bst`) are scoped package
/// roots that are never entry candidates. `ProjectPackageFacade` is the optional project-root
/// `+*.bst` beside `config.bst`; Stage 0 assigns it from location rather than filename alone and
/// it never participates in entry-root containment or import-resolution lookup.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum ModuleRootRole {
    Normal,
    Support,
    ProjectPackageFacade,
}

/// Owned, hashable, cross-build origin identity for one canonical module.
///
/// WHAT: derives a stable module origin from the owning [`StablePackageIdentity`], the canonical
/// portable logical module path (forward-slash logical spelling, including the empty entry-root
/// path) and the [`ModuleRootRole`]. It stores no `PathBuf`, `StringId`, `InternedPath`, dense
/// `ModuleId` or absolute filesystem path, so identity is stable across checkout roots,
/// traversal order, cosmetic root-filename suffixes and the ordinary source file that contains
/// a declaration.
/// WHY: later exported declaration identities embed this module origin identity. Keeping the
/// dense `ModuleId` as the build-local handle prevents process-local indexes from leaking across
/// module boundaries or into persistent artefacts.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StableModuleOriginIdentity {
    package: StablePackageIdentity,
    logical_module_path: String,
    role: ModuleRootRole,
}

impl StableModuleOriginIdentity {
    /// Build the cross-build identity from a base-relative logical module path.
    ///
    /// `relative_logical_path` is the `PathBuf` produced by stripping the canonical root
    /// directory against its base (the entry root, or the project root for the facade). It is
    /// converted to a portable forward-slash spelling so the identity is self-contained and
    /// platform-independent.
    ///
    /// Only normal relative components are accepted. `CurDir`, `ParentDir`, `RootDir`, `Prefix`
    /// and non-UTF-8 components are rejected through an internal `CompilerError` so two invalid
    /// inputs can never collapse to the same stable identity. Stage 0's earlier UTF-8 and
    /// base-relative validation makes these invariant failures, but the constructor remains
    /// total rather than panicking.
    pub(crate) fn from_relative_logical_path(
        package: StablePackageIdentity,
        relative_logical_path: &Path,
        role: ModuleRootRole,
    ) -> Result<Self, CompilerError> {
        Ok(Self {
            package,
            logical_module_path: portable_logical_module_path_from(relative_logical_path)?,
            role,
        })
    }

    /// Construct a stable module origin from already-portable spelling inputs.
    ///
    /// Compiler-internal helper for tests and later phases that already hold the portable
    /// spelling and the owned package identity. Production Stage 0 construction goes through
    /// [`StableModuleOriginIdentity::from_relative_logical_path`].
    #[cfg(test)]
    pub(crate) fn from_portable_path(
        package: StablePackageIdentity,
        logical_module_path: String,
        role: ModuleRootRole,
    ) -> Self {
        Self {
            package,
            logical_module_path,
            role,
        }
    }

    /// The owning stable package identity.
    #[allow(dead_code)]
    pub(crate) fn package(&self) -> &StablePackageIdentity {
        &self.package
    }

    /// The canonical portable logical module path spelling (forward slashes, empty for the
    /// entry root).
    #[allow(dead_code)]
    pub(crate) fn logical_module_path(&self) -> &str {
        &self.logical_module_path
    }

    /// The structural root role.
    #[allow(dead_code)]
    pub(crate) fn role(&self) -> ModuleRootRole {
        self.role
    }
}

/// Convert a base-relative logical module path into a portable forward-slash logical spelling.
///
/// The entry-root module yields the empty string; deeper normal components are joined with `/`.
/// Only normal relative components are accepted. `CurDir`, `ParentDir`, `RootDir` and `Prefix`
/// components are rejected through an internal `CompilerError` so two invalid inputs cannot
/// collapse to the same stable identity. Stage 0 traversal already rejects non-UTF-8 module path
/// components through structured diagnostics, so a non-UTF-8 normal component here is a proven
/// internal invariant; it is still surfaced as an explicit `CompilerError` rather than a panic.
fn portable_logical_module_path_from(relative: &Path) -> Result<String, CompilerError> {
    let mut spelling = String::new();
    for component in relative.components() {
        match component {
            Component::Normal(name) => {
                let name = name.to_str().ok_or_else(|| {
                    CompilerError::compiler_error(format!(
                        "stable logical module path component {name:?} in {relative:?} is not \
                         UTF-8; Stage 0 rejects non-UTF-8 module path components before identity \
                         construction, so this is an internal invariant violation"
                    ))
                })?;
                if !spelling.is_empty() {
                    spelling.push('/');
                }
                spelling.push_str(name);
            }
            Component::CurDir
            | Component::ParentDir
            | Component::RootDir
            | Component::Prefix(_) => {
                return Err(CompilerError::compiler_error(format!(
                    "stable logical module path {relative:?} contains an invalid component \
                     {component:?}; only normal relative components are permitted, so two invalid \
                     inputs cannot collapse to the same stable identity"
                )));
            }
        }
    }
    Ok(spelling)
}

/// The semantic category of one exported source type that receives a stable origin identity.
///
/// Struct, choice and transparent alias are distinguished so changing a declaration's category
/// changes its origin identity: a struct and a transparent alias with the same defining name in
/// the same module are distinct exported declarations and must not share an [`OriginTypeId`].
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum OriginTypeCategory {
    Struct,
    Choice,
    TransparentAlias,
}

/// Distinguishes a free function from a receiver method for stable function origin identity.
///
/// Receiver methods are not independent namespace exports: they live on their receiver type's
/// surface. A method exported on a receiver surface still needs a stable function origin, so the
/// `Receiver` variant carries the receiver's [`OriginTypeId`] inline. Two methods named `run` on
/// distinct receiver types cannot collapse because the receiver type identity is embedded in the
/// variant. This is the sole authoritative receiver state: a free function with a receiver or a
/// receiver method without a receiver type is unrepresentable.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum FunctionOriginKind {
    Free,
    Receiver(OriginTypeId),
}

/// Owned, hashable, cross-build origin identity for one exported source type.
///
/// WHAT: derives a stable type origin from the owning [`StableModuleOriginIdentity`], the exact
/// defining declaration name and the [`OriginTypeCategory`]. It stores no `StringId`,
/// `InternedPath`, `FileId`, source location, ordinary source-file path, declaration order,
/// export alias or dense build-local ID, so identity is stable when source moves between files,
/// reordered or aliased at export time.
/// WHY: cross-module type references and generated instances key off this origin so changing a
/// declaration's name, category or module alters identity while cosmetic moves do not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OriginTypeId {
    module_origin: StableModuleOriginIdentity,
    defining_name: String,
    category: OriginTypeCategory,
}

impl OriginTypeId {
    /// Construct the stable origin identity for one exported source type.
    ///
    /// Compiler-internal: only declarations admitted to a consumer-visible or exported surface
    /// receive an origin ID. Private source types remain without origin IDs by policy; this
    /// slice does not add identity fields to private headers.
    #[allow(dead_code)]
    pub(crate) fn new(
        module_origin: StableModuleOriginIdentity,
        defining_name: String,
        category: OriginTypeCategory,
    ) -> Self {
        Self {
            module_origin,
            defining_name,
            category,
        }
    }

    /// The owning module origin identity.
    #[allow(dead_code)]
    pub(crate) fn module_origin(&self) -> &StableModuleOriginIdentity {
        &self.module_origin
    }

    /// The exact defining declaration name.
    #[allow(dead_code)]
    pub(crate) fn defining_name(&self) -> &str {
        &self.defining_name
    }

    /// The semantic source type category.
    #[allow(dead_code)]
    pub(crate) fn category(&self) -> OriginTypeCategory {
        self.category
    }
}

/// Owned, hashable, cross-build origin identity for one exported function.
///
/// WHAT: derives a stable function origin from the owning [`StableModuleOriginIdentity`], the
/// exact defining declaration name and the sole [`FunctionOriginKind`], which embeds the receiver
/// type identity for a method. It stores no `StringId`, `InternedPath`, `FileId`, source
/// location, ordinary source-file path, declaration order, export alias or dense build-local ID.
/// WHY: identifies stable cross-module source call targets so a free function and a method of the
/// same name are distinct, and renaming a function or moving it between modules alters identity
/// while reordering and aliasing do not. Binding-backed external call identities are a separate
/// target class owned by a later phase, not this origin.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OriginFunctionId {
    module_origin: StableModuleOriginIdentity,
    defining_name: String,
    kind: FunctionOriginKind,
}

impl OriginFunctionId {
    /// Construct the stable origin identity for one exported free function.
    ///
    /// Compiler-internal: only declarations admitted to a consumer-visible or exported surface
    /// receive an origin ID. Private functions remain without origin IDs by policy.
    #[allow(dead_code)]
    pub(crate) fn new_free(
        module_origin: StableModuleOriginIdentity,
        defining_name: String,
    ) -> Self {
        Self {
            module_origin,
            defining_name,
            kind: FunctionOriginKind::Free,
        }
    }

    /// Construct the stable origin identity for one exported receiver method.
    ///
    /// `receiver` is the stable [`OriginTypeId`] of the receiver type. Compiler-internal: only
    /// methods admitted to a receiver surface that is itself consumer-visible or exported receive
    /// an origin ID. A method is not an independent namespace export, but its exported receiver
    /// surface still needs a stable function origin.
    #[allow(dead_code)]
    pub(crate) fn new_receiver(
        module_origin: StableModuleOriginIdentity,
        defining_name: String,
        receiver: OriginTypeId,
    ) -> Self {
        Self {
            module_origin,
            defining_name,
            kind: FunctionOriginKind::Receiver(receiver),
        }
    }

    /// The owning module origin identity.
    #[allow(dead_code)]
    pub(crate) fn module_origin(&self) -> &StableModuleOriginIdentity {
        &self.module_origin
    }

    /// The exact defining declaration name.
    #[allow(dead_code)]
    pub(crate) fn defining_name(&self) -> &str {
        &self.defining_name
    }

    /// Whether this origin is a free function or a receiver method.
    ///
    /// Returns the sole authoritative state: `Free` or `Receiver(OriginTypeId)`. The receiver
    /// type identity for a method is embedded in the variant, not stored in a parallel field.
    #[allow(dead_code)]
    pub(crate) fn kind(&self) -> &FunctionOriginKind {
        &self.kind
    }

    /// The stable type origin of the receiver for a method, or `None` for a free function.
    #[allow(dead_code)]
    pub(crate) fn receiver(&self) -> Option<&OriginTypeId> {
        match &self.kind {
            FunctionOriginKind::Free => None,
            FunctionOriginKind::Receiver(receiver) => Some(receiver),
        }
    }
}

/// Owned, hashable, cross-build origin identity for one exported constant.
///
/// WHAT: derives a stable constant origin from the owning [`StableModuleOriginIdentity`] and the
/// exact defining declaration name. It stores no `StringId`, `InternedPath`, `FileId`, source
/// location, ordinary source-file path, declaration order, export alias or dense build-local ID.
/// WHY: cross-module constant references key off this origin so renaming a constant or moving it
/// between modules alters identity while reordering and aliasing do not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OriginConstantId {
    module_origin: StableModuleOriginIdentity,
    defining_name: String,
}

impl OriginConstantId {
    /// Construct the stable origin identity for one exported constant.
    ///
    /// Compiler-internal: only declarations admitted to a consumer-visible or exported surface
    /// receive an origin ID. Private constants remain without origin IDs by policy.
    #[allow(dead_code)]
    pub(crate) fn new(module_origin: StableModuleOriginIdentity, defining_name: String) -> Self {
        Self {
            module_origin,
            defining_name,
        }
    }

    /// The owning module origin identity.
    #[allow(dead_code)]
    pub(crate) fn module_origin(&self) -> &StableModuleOriginIdentity {
        &self.module_origin
    }

    /// The exact defining declaration name.
    #[allow(dead_code)]
    pub(crate) fn defining_name(&self) -> &str {
        &self.defining_name
    }
}

/// Owned, hashable, cross-build origin identity for one exported trait.
///
/// WHAT: derives a stable trait origin from the owning [`StableModuleOriginIdentity`] and the
/// exact defining declaration name. It stores no `StringId`, `InternedPath`, `FileId`, source
/// location, ordinary source-file path, declaration order, export alias or dense build-local ID.
/// WHY: cross-module conformance evidence and trait references key off this origin so renaming a
/// trait or moving it between modules alters identity while reordering and aliasing do not.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct OriginTraitId {
    module_origin: StableModuleOriginIdentity,
    defining_name: String,
}

impl OriginTraitId {
    /// Construct the stable origin identity for one exported trait.
    ///
    /// Compiler-internal: only declarations admitted to a consumer-visible or exported surface
    /// receive an origin ID. Private traits remain without origin IDs by policy.
    #[allow(dead_code)]
    pub(crate) fn new(module_origin: StableModuleOriginIdentity, defining_name: String) -> Self {
        Self {
            module_origin,
            defining_name,
        }
    }

    /// The owning module origin identity.
    #[allow(dead_code)]
    pub(crate) fn module_origin(&self) -> &StableModuleOriginIdentity {
        &self.module_origin
    }

    /// The exact defining declaration name.
    #[allow(dead_code)]
    pub(crate) fn defining_name(&self) -> &str {
        &self.defining_name
    }
}

/// Unified stable origin identity for one exported declaration, regardless of category.
///
/// WHAT: a typed sum over the exported declaration origin IDs. It preserves the typed category
/// so a type, function, constant and trait with the same defining name in the same module remain
/// distinct, and so later phases can dispatch over exported declarations without re-parsing.
/// WHY: public-interface binding and exported-symbol tables key over one stable identity value
/// while still distinguishing declaration category. Identity still derives only from module
/// origin, defining name, category and receiver type identity where applicable.
#[allow(dead_code)]
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum OriginDeclarationId {
    Function(OriginFunctionId),
    Type(OriginTypeId),
    Constant(OriginConstantId),
    Trait(OriginTraitId),
}

impl OriginDeclarationId {
    /// The owning module origin identity for this declaration.
    #[allow(dead_code)]
    pub(crate) fn module_origin(&self) -> &StableModuleOriginIdentity {
        match self {
            OriginDeclarationId::Function(function) => function.module_origin(),
            OriginDeclarationId::Type(type_id) => type_id.module_origin(),
            OriginDeclarationId::Constant(constant) => constant.module_origin(),
            OriginDeclarationId::Trait(trait_id) => trait_id.module_origin(),
        }
    }
}
