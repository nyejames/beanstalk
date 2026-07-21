//! Stage 0 source-tree indexing for directory projects.
//!
//! WHAT: performs the one deterministic entry-root traversal that prepares canonical module
//! identities, root entry candidates, sibling import-name collision facts, and entry-root
//! source-backed package prefix collision facts for the rest of the build. Each discovered root
//! receives a deterministic `ModuleId`, an explicit `ModuleRootRole` and a source-relative
//! logical module path owned by `module_identity`.
//! WHY: filesystem discovery belongs to Stage 0. Keeping it here prevents the frontend resolver,
//! module inventory, and collision validators from repeating the same expensive walk.

use super::module_identity::{
    ModuleIdentityRecord, ModuleIdentityTable, module_root_role_for_file_name,
};
use crate::builder_surface::SourcePackageRegistry;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::module_roots::ModuleRootTable;
use crate::compiler_frontend::semantic_identity::{ModuleRootRole, StablePackageIdentity};
use crate::compiler_frontend::source_packages::root_file::{
    file_name_is_hash_root_file, file_name_is_module_root_file, file_name_is_support_root_file,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::project_structure_diagnostics::{
    non_utf8_filesystem_name_error, path_id, project_structure_messages,
};

const FIXED_SKIPPED_DIRECTORY_NAMES: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    "release",
    "dev",
    "dist",
    "build",
    ".cache",
];

/// Counts work performed by the Stage 0 source-tree traversal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceTreeDiscoveryStats {
    pub(crate) dirs_visited: usize,
    pub(crate) dirs_skipped: usize,
    pub(crate) files_seen: usize,
    pub(crate) hash_root_files_seen: usize,
    pub(crate) support_root_files_seen: usize,
    pub(crate) module_roots_found: usize,
    pub(crate) project_package_facade_found: bool,
}

/// Directory names and configured output boundaries excluded from entry-root traversal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceTreeSkipPolicy {
    configured_directories: Vec<PathBuf>,
}

impl SourceTreeSkipPolicy {
    fn from_config(project_root: &Path, entry_root: &Path, config: &Config) -> Self {
        let mut configured_directories = Vec::new();
        for configured_folder in [&config.dev_folder, &config.release_folder] {
            let configured_path = if configured_folder.is_absolute() {
                configured_folder.clone()
            } else {
                project_root.join(configured_folder)
            };

            if let Ok(canonical_path) = fs::canonicalize(configured_path)
                && canonical_path != entry_root
            {
                configured_directories.push(canonical_path);
            }
        }

        configured_directories.sort();
        configured_directories.dedup();

        Self {
            configured_directories,
        }
    }

    fn should_skip(&self, directory: &Path) -> bool {
        let fixed_name = directory
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| FIXED_SKIPPED_DIRECTORY_NAMES.contains(&name));

        fixed_name
            || self
                .configured_directories
                .binary_search(&directory.to_path_buf())
                .is_ok()
    }
}

/// One canonical root file discovered inside a traversed directory, with its structural role.
struct DiscoveredDirectoryRoot {
    root_file: PathBuf,
    role: ModuleRootRole,
}

/// Canonical module identities, root entry candidates and traversal evidence for one directory
/// build.
///
/// `module_identities` is the Stage 0 durable identity and topology table. `module_roots` is the
/// narrow frontend normal-root lookup table derived from it for current resolver consumers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceTreeIndex {
    entry_root: PathBuf,
    module_identities: ModuleIdentityTable,
    module_roots: ModuleRootTable,
    entry_candidates: Vec<PathBuf>,
    stats: SourceTreeDiscoveryStats,
}

impl SourceTreeIndex {
    /// Build the index with one deterministic traversal of the configured entry root.
    ///
    /// The traversal also owns entry-root sibling `.bst` file/folder import-name collisions and
    /// entry-root folder/source-backed package-prefix collisions, using the same sorted directory
    /// entries it already reads. Skipped directories neither contribute collision facts nor get
    /// recursively scanned. Source-backed package-tree collision validation remains separate because
    /// registered source-backed package traversal lives outside entry-root indexing.
    ///
    /// Each source directory may contain one `#*.bst` normal root or one `+*.bst` support root.
    /// Multiple or mixed roots in one directory are rejected through the existing structured config
    /// diagnostic lane. Only normal roots become entry candidates. The optional project-root
    /// `+*.bst` facade beside `config.bst` is discovered as a separate `ProjectPackageFacade`
    /// node outside the entry-root containment tree and is never an entry candidate.
    pub(super) fn discover(
        entry_root: PathBuf,
        project_root: &Path,
        config: &Config,
        source_packages: &SourcePackageRegistry,
        string_table: &mut StringTable,
    ) -> Result<Self, CompilerMessages> {
        let discovery_start = crate::timing::start_pipeline_timing();
        let skip_policy = SourceTreeSkipPolicy::from_config(project_root, &entry_root, config);

        // One stable project package identity shared by every node in the project graph: normal
        // roots, support roots and the optional project package facade. It is derived from the
        // configured project name, never from the checkout directory or an absolute path.
        let project_package = StablePackageIdentity::project_local(&config.project_name);

        let mut stats = SourceTreeDiscoveryStats::default();
        let mut queue = VecDeque::from([entry_root.clone()]);
        let mut records = Vec::new();
        let mut entry_candidates = Vec::new();

        let facade_root_file =
            discover_project_package_facade(project_root, &mut stats, string_table)?;

        while let Some(directory) = queue.pop_front() {
            stats.dirs_visited += 1;

            let mut entries = fs::read_dir(&directory)
                .map_err(|error| Self::directory_read_error(&directory, error, string_table))?
                .collect::<Result<Vec<_>, _>>()
                .map_err(|error| {
                    CompilerError::file_error(
                        &directory,
                        format!(
                            "Failed to read directory entry while indexing source tree: {error}"
                        ),
                        string_table,
                    )
                })
                .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;
            entries.sort_by_key(|entry| entry.path());

            let mut subdirectories = Vec::new();
            let mut bst_file_stems: BTreeSet<String> = BTreeSet::new();
            let mut importable_folder_names: BTreeSet<String> = BTreeSet::new();
            let mut directory_roots: Vec<DiscoveredDirectoryRoot> = Vec::new();

            for entry in entries {
                let path = entry.path();

                if path.is_dir() {
                    if skip_policy.should_skip(&path) {
                        stats.dirs_skipped += 1;
                    } else {
                        let folder_name = path
                            .file_name()
                            .and_then(|name| name.to_str())
                            .ok_or_else(|| {
                                non_utf8_filesystem_name_error(
                                    &path,
                                    "source tree folder name",
                                    string_table,
                                )
                            })?;
                        importable_folder_names.insert(folder_name.to_owned());
                        subdirectories.push(path);
                    }
                    continue;
                }

                if !path.is_file() {
                    continue;
                }

                stats.files_seen += 1;

                let file_name =
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .ok_or_else(|| {
                            non_utf8_filesystem_name_error(
                                &path,
                                "source tree file name",
                                string_table,
                            )
                        })?;

                if let Some(stem) = bst_stem_from_file_name(file_name) {
                    bst_file_stems.insert(stem.to_owned());
                }

                if !file_name_is_module_root_file(file_name) {
                    continue;
                }

                let canonical_path = fs::canonicalize(&path)
                    .map_err(|error| {
                        CompilerError::file_error(
                            &path,
                            format!("Failed to canonicalize module root path: {error}"),
                            string_table,
                        )
                    })
                    .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

                // The project package facade is discovered beside config.bst at the project root
                // and classified as a separate node. Skip it here so a directory shared with the
                // facade does not also classify it as a support root or trigger mixed-root
                // rejection.
                if Some(&canonical_path) == facade_root_file.as_ref() {
                    continue;
                }

                let role = module_root_role_for_file_name(file_name)
                    .expect("a module root file name has a role after is_module_root_file");

                if role == ModuleRootRole::Normal {
                    stats.hash_root_files_seen += 1;
                } else if role == ModuleRootRole::Support {
                    stats.support_root_files_seen += 1;
                }

                directory_roots.push(DiscoveredDirectoryRoot {
                    root_file: canonical_path,
                    role,
                });
            }

            // Check sibling .bst file/folder import-name collisions from the same sorted
            // entries. Skipped folders are absent from importable_folder_names so they cannot
            // create false collisions.
            for stem in &bst_file_stems {
                if importable_folder_names.contains(stem) {
                    return Err(project_structure_messages(
                        &directory,
                        InvalidConfigReason::BstFileFolderCollision {
                            file_name: string_table.intern(&format!("{stem}.bst")),
                            folder_name: string_table.intern(stem),
                            directory: path_id(&directory, string_table),
                        },
                        string_table,
                    ));
                }
            }

            // On the root pass, reject entry-root folders whose names collide with
            // source-backed package import prefixes.
            if directory == entry_root {
                for folder_name in &importable_folder_names {
                    if source_packages.has_prefix(folder_name) {
                        let colliding_folder = directory.join(folder_name);
                        return Err(project_structure_messages(
                            &colliding_folder,
                            InvalidConfigReason::EntryRootPackagePrefixCollision {
                                prefix: string_table.intern(folder_name),
                                entry_folder: path_id(&colliding_folder, string_table),
                            },
                            string_table,
                        ));
                    }
                }
            }

            if let Some(root) =
                classify_directory_root(&directory, &mut directory_roots, string_table)?
            {
                let canonical_root_directory = fs::canonicalize(&directory)
                    .map_err(|error| {
                        CompilerError::file_error(
                            &directory,
                            format!("Failed to canonicalize module root directory: {error}"),
                            string_table,
                        )
                    })
                    .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

                let logical_module_path =
                    logical_module_path_from(&canonical_root_directory, &entry_root, string_table)?;

                if root.role == ModuleRootRole::Normal {
                    entry_candidates.push(root.root_file.clone());
                }

                stats.module_roots_found += 1;
                records.push(
                    ModuleIdentityRecord::new(
                        canonical_root_directory,
                        root.root_file,
                        root.role,
                        logical_module_path,
                        &project_package,
                    )
                    .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?,
                );
            }

            subdirectories.sort();
            queue.extend(subdirectories);
        }

        if let Some(facade_file) = facade_root_file {
            let facade_directory = fs::canonicalize(project_root)
                .map_err(|error| {
                    CompilerError::file_error(
                        project_root,
                        format!("Failed to canonicalize project root directory: {error}"),
                        string_table,
                    )
                })
                .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

            records.push(
                ModuleIdentityRecord::new(
                    facade_directory.clone(),
                    facade_file,
                    ModuleRootRole::ProjectPackageFacade,
                    logical_module_path_from(&facade_directory, &facade_directory, string_table)?,
                    &project_package,
                )
                .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?,
            );
        }

        record_discovery_metrics(&stats, discovery_start);
        entry_candidates.sort();

        let module_identities = ModuleIdentityTable::from_records(records);
        let module_roots = module_identities.derive_module_root_table();

        Ok(Self {
            entry_root,
            module_identities,
            module_roots,
            entry_candidates,
            stats,
        })
    }

    /// Prepare bounded root data for a directly compiled special entry file.
    ///
    /// The source tree uses the same index owner as directory compilation. The caller consumes
    /// only the prepared root table because its entry file is already explicit.
    pub(super) fn bounded_module_roots_for_single_file(
        entry_file: &Path,
        config: &Config,
        source_packages: &SourcePackageRegistry,
        string_table: &mut StringTable,
    ) -> Result<ModuleRootTable, CompilerMessages> {
        let file_name = entry_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                non_utf8_filesystem_name_error(entry_file, "single-file entry name", string_table)
            })?;
        if !file_name_is_hash_root_file(file_name) {
            return Ok(ModuleRootTable::empty());
        }

        let Some(root_directory) = entry_file.parent() else {
            return Ok(ModuleRootTable::empty());
        };
        let canonical_root = fs::canonicalize(root_directory)
            .map_err(|error| {
                CompilerError::file_error(
                    root_directory,
                    format!("Failed to canonicalize single-file source root: {error}"),
                    string_table,
                )
            })
            .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

        Self::discover(
            canonical_root.clone(),
            &canonical_root,
            config,
            source_packages,
            string_table,
        )
        .map(|index| index.module_roots)
    }

    #[cfg(test)]
    pub(crate) fn entry_root(&self) -> &Path {
        &self.entry_root
    }

    pub(crate) fn module_roots(&self) -> &ModuleRootTable {
        &self.module_roots
    }

    /// The Stage 0 durable module identity and topology table.
    ///
    /// Consumed by later graph-construction phases (Phase 5) and by focused tests; the narrow
    /// frontend lookup table is available through [`SourceTreeIndex::module_roots`].
    #[allow(dead_code)]
    pub(crate) fn module_identities(&self) -> &ModuleIdentityTable {
        &self.module_identities
    }

    pub(crate) fn entry_candidates(&self) -> &[PathBuf] {
        &self.entry_candidates
    }

    #[cfg(test)]
    pub(crate) fn stats(&self) -> &SourceTreeDiscoveryStats {
        &self.stats
    }

    fn directory_read_error(
        directory: &Path,
        error: std::io::Error,
        string_table: &mut StringTable,
    ) -> CompilerMessages {
        CompilerMessages::from_error_ref(
            CompilerError::file_error(
                directory,
                format!("Failed to read directory while indexing source tree: {error}"),
                string_table,
            ),
            string_table,
        )
    }
}

/// Discover the optional project package facade beside `config.bst` at the project root.
///
/// WHAT: scans the project root for one direct-child `+*.bst` file.
/// WHY: the facade is a node outside the entry-root containment tree. Discovering it here keeps it
///      out of the per-directory root classification so a shared directory does not classify it as
///      a support root or trigger mixed-root rejection.
fn discover_project_package_facade(
    project_root: &Path,
    stats: &mut SourceTreeDiscoveryStats,
    string_table: &mut StringTable,
) -> Result<Option<PathBuf>, CompilerMessages> {
    // A project-root read failure is an infrastructure error, not the absence of a facade.
    // Preserve it through the file-error lane with the project-root path so the build boundary
    // can render it instead of silently treating the facade as missing.
    let entries = fs::read_dir(project_root).map_err(|error| {
        CompilerMessages::from_error_ref(
            CompilerError::file_error(
                project_root,
                format!("Failed to read project root while discovering package facade: {error}"),
                string_table,
            ),
            string_table,
        )
    })?;

    let mut support_roots = Vec::new();
    for entry in entries {
        let path = entry
            .map_err(|error| {
                CompilerError::file_error(
                    project_root,
                    format!(
                        "Failed to read project root entry while discovering package facade: {error}"
                    ),
                    string_table,
                )
            })
            .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?
            .path();

        if !path.is_file() {
            continue;
        }

        // A non-UTF-8 direct-child filename cannot be classified as a support-root candidate and
        // must not be silently skipped. Use the same typed filesystem-name error owner as the
        // source-tree traversal so the offending path is preserved for rendering.
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                non_utf8_filesystem_name_error(
                    &path,
                    "project package facade candidate name",
                    string_table,
                )
            })?;

        if file_name_is_support_root_file(file_name) {
            let canonical = fs::canonicalize(&path)
                .map_err(|error| {
                    CompilerError::file_error(
                        &path,
                        format!("Failed to canonicalize project package facade path: {error}"),
                        string_table,
                    )
                })
                .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;
            support_roots.push(canonical);
        }
    }

    support_roots.sort();

    if support_roots.len() > 1 {
        let candidates = support_roots
            .iter()
            .map(|path| path_id(path, string_table))
            .collect();
        return Err(project_structure_messages(
            project_root,
            InvalidConfigReason::MultipleModuleRootFiles {
                directory: path_id(project_root, string_table),
                candidates,
            },
            string_table,
        ));
    }

    if support_roots.len() == 1 {
        stats.project_package_facade_found = true;
        Ok(support_roots.pop())
    } else {
        Ok(None)
    }
}

/// Reject multiple or mixed roots in one directory and return the single allowed root.
fn classify_directory_root(
    directory: &Path,
    directory_roots: &mut Vec<DiscoveredDirectoryRoot>,
    string_table: &mut StringTable,
) -> Result<Option<DiscoveredDirectoryRoot>, CompilerMessages> {
    if directory_roots.is_empty() {
        return Ok(None);
    }

    if directory_roots.len() > 1 {
        directory_roots.sort_by(|left, right| left.root_file.cmp(&right.root_file));
        let candidates = directory_roots
            .iter()
            .map(|root| path_id(&root.root_file, string_table))
            .collect();
        return Err(project_structure_messages(
            directory,
            InvalidConfigReason::MultipleModuleRootFiles {
                directory: path_id(directory, string_table),
                candidates,
            },
            string_table,
        ));
    }

    Ok(Some(directory_roots.pop().expect(
        "non-empty directory root list has one root after validation",
    )))
}

/// Compute the source-relative logical module path for a canonical root directory.
///
/// `base` is the canonical entry root (or, for the facade, the project root). A canonical root
/// directory discovered under the entry root always shares that prefix, so a `strip_prefix`
/// failure is a proven internal invariant: it means the entry root was not canonicalized before
/// indexing or the directory escaped the entry-root tree. Rather than silently falling back to an
/// absolute machine-local path (which would make `ModuleId` non-deterministic across machines),
/// surface it as an internal compiler error so the failure is never hidden.
fn logical_module_path_from(
    root_directory: &Path,
    base: &Path,
    string_table: &mut StringTable,
) -> Result<PathBuf, CompilerMessages> {
    root_directory
        .strip_prefix(base)
        .map(PathBuf::from)
        .map_err(|_| {
            CompilerMessages::from_error_ref(
                CompilerError::compiler_error(format!(
                    "Module root directory {root_directory:?} is not under the canonical base \
                 {base:?}; logical module path cannot fall back to an absolute path"
                )),
                string_table,
            )
        })
}

/// Extract the import-name stem from a validated `.bst` file name, or `None` for other extensions.
///
/// The caller must have already validated `file_name` as UTF-8 so that extension and stem
/// extraction can never silently skip a non-UTF-8 component.
fn bst_stem_from_file_name(file_name: &str) -> Option<&str> {
    let path = Path::new(file_name);
    let extension = path.extension().and_then(|extension| extension.to_str())?;
    if extension != BEANSTALK_FILE_EXTENSION {
        return None;
    }
    path.file_stem().and_then(|stem| stem.to_str())
}

fn record_discovery_metrics(
    stats: &SourceTreeDiscoveryStats,
    discovery_start: crate::timing::PipelineTimingStart,
) {
    crate::timing::record_started_pipeline_timing(
        "stage0.source_tree_index.discovery",
        discovery_start,
    );
    crate::timing::record_counter("source_tree_index.discovery_runs", 1.0);
    crate::timing::record_counter("source_tree_index.dirs_visited", stats.dirs_visited as f64);
    crate::timing::record_counter("source_tree_index.dirs_skipped", stats.dirs_skipped as f64);
    crate::timing::record_counter("source_tree_index.files_seen", stats.files_seen as f64);
    crate::timing::record_counter(
        "source_tree_index.hash_root_files_seen",
        stats.hash_root_files_seen as f64,
    );
    crate::timing::record_counter(
        "source_tree_index.support_root_files_seen",
        stats.support_root_files_seen as f64,
    );
    crate::timing::record_counter(
        "source_tree_index.module_roots_found",
        stats.module_roots_found as f64,
    );
    crate::timing::record_counter(
        "source_tree_index.project_package_facade_found",
        if stats.project_package_facade_found {
            1.0
        } else {
            0.0
        },
    );
}
