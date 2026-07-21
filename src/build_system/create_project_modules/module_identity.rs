//! Stage 0 durable module identity and structural topology.
//!
//! WHAT: owns the canonical module identities, stable cross-build origin identities, root
//! roles, logical module paths and structural ancestry produced by the one Stage 0 source-tree
//! traversal, and derives the narrow frontend module-root lookup table from the normal roots.
//! WHY: durable identity and topology are build-system-owned data. The frontend resolver
//! consumes only the derived normal-root lookup table, so import resolution never sees support
//! or facade records in this slice. Later graph-construction phases consume the identity and
//! ancestry directly from this table. The dense `ModuleId` is the build-local handle for one
//! build boundary; the owned `StableModuleOriginIdentity` is the cross-build semantic identity
//! that later exported declaration identities will embed.

use crate::builder_surface::PackageOrigin;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::paths::module_roots::{ModuleRootRecord, ModuleRootTable};
use crate::compiler_frontend::source_packages::root_file::{
    file_name_is_hash_root_file, file_name_is_support_root_file,
};

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Dense deterministic handle for one canonical module inside one build boundary.
///
/// `ModuleId` is the build-local index assigned by sorting canonical logical module paths, so
/// it is independent of traversal completion order and cosmetic root filename suffixes
/// (`#mod.bst` and `#page.bst` in the same directory collapse to one root whose identity comes
/// from the directory, not the filename).
///
/// It is deliberately not the persistent semantic identity: its numeric value is a build-local
/// table slot that may cross stages inside that build boundary but must not identify a module
/// across builds or reach persistent artefacts. The cross-build semantic identity is the owned
/// [`StableModuleOriginIdentity`](self::StableModuleOriginIdentity).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) struct ModuleId(usize);

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
    use std::path::Component;

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

/// The root role implied by a canonical root filename, or `None` for non-root files.
///
/// WHAT: maps the filename marker to a root role. A `+*.bst` filename maps to `Support`; Stage 0
///      discovery upgrades a project-root `+*.bst` beside `config.bst` to
///      `ProjectPackageFacade` from its location, which this filename-only classifier cannot
///      infer.
pub(crate) fn module_root_role_for_file_name(file_name: &str) -> Option<ModuleRootRole> {
    if file_name_is_hash_root_file(file_name) {
        Some(ModuleRootRole::Normal)
    } else if file_name_is_support_root_file(file_name) {
        Some(ModuleRootRole::Support)
    } else {
        None
    }
}

/// One canonical module root with its durable structural identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModuleIdentityRecord {
    root_directory: PathBuf,
    root_file: PathBuf,
    role: ModuleRootRole,
    logical_module_path: PathBuf,
    stable_origin: StableModuleOriginIdentity,
}

impl ModuleIdentityRecord {
    /// Construct one canonical module record and derive its cross-build origin identity.
    ///
    /// `package` is the shared stable package identity for the project graph (or, later, a
    /// dependency package graph). The portable stable origin identity is derived from
    /// `logical_module_path` so the dense `PathBuf` remains the single source of truth for the
    /// build-local relative path while the stable identity stores only the portable spelling.
    /// Returns an internal `CompilerError` if `logical_module_path` contains an invalid or
    /// non-UTF-8 component; the caller surfaces it at the Stage 0 boundary.
    pub(crate) fn new(
        root_directory: PathBuf,
        root_file: PathBuf,
        role: ModuleRootRole,
        logical_module_path: PathBuf,
        package: &StablePackageIdentity,
    ) -> Result<Self, CompilerError> {
        let stable_origin = StableModuleOriginIdentity::from_relative_logical_path(
            package.clone(),
            &logical_module_path,
            role,
        )?;
        Ok(Self {
            root_directory,
            root_file,
            role,
            logical_module_path,
            stable_origin,
        })
    }

    // Phase 2 identity accessors. Consumed by later source-set and graph phases and exercised by
    // focused tests; allowed dead until the first production consumer lands.
    #[allow(dead_code)]
    pub(crate) fn root_directory(&self) -> &Path {
        &self.root_directory
    }

    #[allow(dead_code)]
    pub(crate) fn root_file(&self) -> &Path {
        &self.root_file
    }

    #[allow(dead_code)]
    pub(crate) fn role(&self) -> ModuleRootRole {
        self.role
    }

    #[allow(dead_code)]
    pub(crate) fn logical_module_path(&self) -> &Path {
        &self.logical_module_path
    }

    /// The owned cross-build origin identity for this module.
    #[allow(dead_code)]
    pub(crate) fn stable_origin(&self) -> &StableModuleOriginIdentity {
        &self.stable_origin
    }
}

/// Canonical module identities, root roles and structural ancestry for one build.
///
/// `ModuleId` assignment sorts records by canonical logical module path so identities are
/// deterministic. Nearest ancestry is computed over non-facade roots by directory containment;
/// the project package facade carries identity but no ancestry because it sits outside the
/// entry-root containment tree.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleIdentityTable {
    records: Vec<ModuleIdentityRecord>,
    by_root_directory: HashMap<PathBuf, ModuleId>,
    nearest_ancestor: Vec<Option<ModuleId>>,
    direct_children: Vec<Vec<ModuleId>>,
}

impl ModuleIdentityTable {
    #[cfg(test)]
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    /// Build the identity table from Stage 0 root records.
    ///
    /// Records are sorted by canonical logical module path (then root directory and role as
    /// tie-breakers) so `ModuleId` assignment is deterministic. Ancestry is computed over
    /// non-facade roots by nearest-module directory containment. The facade shares the project
    /// root directory when the project root equals the entry root, so it is registered for
    /// directory identity only when no non-facade root already claims that directory.
    pub(crate) fn from_records(mut records: Vec<ModuleIdentityRecord>) -> Self {
        records.sort_by(|left, right| {
            left.logical_module_path
                .cmp(&right.logical_module_path)
                .then_with(|| left.root_directory.cmp(&right.root_directory))
                .then_with(|| left.role.cmp(&right.role))
        });

        let record_count = records.len();
        let mut by_root_directory: HashMap<PathBuf, ModuleId> = HashMap::new();
        let mut non_facade_by_directory: HashMap<PathBuf, ModuleId> = HashMap::new();

        for (index, record) in records.iter().enumerate() {
            let module_id = ModuleId(index);
            if record.role != ModuleRootRole::ProjectPackageFacade {
                non_facade_by_directory.insert(record.root_directory.clone(), module_id);
                by_root_directory.insert(record.root_directory.clone(), module_id);
            }
        }

        // The facade may share the project root directory when the project root equals the entry
        // root. Register it for directory identity only when no non-facade root already claims
        // that directory, so the shared directory keeps resolving to its module root.
        for (index, record) in records.iter().enumerate() {
            if record.role == ModuleRootRole::ProjectPackageFacade {
                by_root_directory
                    .entry(record.root_directory.clone())
                    .or_insert(ModuleId(index));
            }
        }

        let nearest_ancestor = compute_nearest_ancestors(&records, &non_facade_by_directory);
        let direct_children = compute_direct_children(record_count, &nearest_ancestor);

        Self {
            records,
            by_root_directory,
            nearest_ancestor,
            direct_children,
        }
    }

    /// Derive the narrow frontend module-root lookup table from the normal roots.
    ///
    /// Only `Normal` roots enter the frontend table so support and facade records stay out of
    /// import-resolution and header-role lookup in this slice. The frontend table preserves its
    /// pre-slice shape and responsibility.
    pub(crate) fn derive_module_root_table(&self) -> ModuleRootTable {
        let normal_records = self
            .records
            .iter()
            .filter(|record| record.role == ModuleRootRole::Normal)
            .map(|record| {
                ModuleRootRecord::new(record.root_directory.clone(), record.root_file.clone())
            })
            .collect();

        ModuleRootTable::from_records(normal_records)
    }

    /// All module identities in deterministic canonical logical path order.
    #[allow(dead_code)]
    pub(crate) fn module_ids(&self) -> impl Iterator<Item = ModuleId> + '_ {
        (0..self.records.len()).map(ModuleId)
    }

    /// The canonical record for one module identity.
    #[allow(dead_code)]
    pub(crate) fn record(&self, module_id: ModuleId) -> &ModuleIdentityRecord {
        &self.records[module_id.0]
    }

    /// The stable identity for a canonical root directory, across all roles.
    #[allow(dead_code)]
    pub(crate) fn module_id_for_directory(&self, directory: &Path) -> Option<ModuleId> {
        self.by_root_directory.get(directory).copied()
    }

    /// The nearest enclosing non-facade module by directory containment, or `None` for the entry
    /// root or the project package facade.
    #[allow(dead_code)]
    pub(crate) fn nearest_ancestor_module(&self, module_id: ModuleId) -> Option<ModuleId> {
        self.nearest_ancestor[module_id.0]
    }

    /// Direct child modules whose nearest ancestor is `module_id`, in deterministic order.
    #[allow(dead_code)]
    pub(crate) fn direct_child_modules(&self, module_id: ModuleId) -> &[ModuleId] {
        &self.direct_children[module_id.0]
    }
}

/// Compute the nearest enclosing non-facade module for each record by walking parent directories.
///
/// The walk uses the non-facade directory map so the project package facade never becomes an
/// ancestor of entry-root modules. Facade records have no ancestor.
fn compute_nearest_ancestors(
    records: &[ModuleIdentityRecord],
    non_facade_by_directory: &HashMap<PathBuf, ModuleId>,
) -> Vec<Option<ModuleId>> {
    records
        .iter()
        .map(|record| {
            if record.role == ModuleRootRole::ProjectPackageFacade {
                return None;
            }

            let mut current = record.root_directory.parent();
            while let Some(directory) = current {
                if let Some(ancestor_id) = non_facade_by_directory.get(directory) {
                    return Some(*ancestor_id);
                }
                current = directory.parent();
            }

            None
        })
        .collect()
}

/// Collect direct children for each module from the nearest-ancestor relationships.
///
/// Children are gathered in deterministic `ModuleId` order so the resulting vectors are stable.
fn compute_direct_children(
    record_count: usize,
    nearest_ancestor: &[Option<ModuleId>],
) -> Vec<Vec<ModuleId>> {
    let mut direct_children = vec![Vec::new(); record_count];

    for (index, ancestor) in nearest_ancestor.iter().enumerate() {
        if let Some(ancestor_id) = ancestor {
            direct_children[ancestor_id.0].push(ModuleId(index));
        }
    }

    direct_children
}
