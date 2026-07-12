//! Stage 0 source-tree indexing for directory projects.
//!
//! WHAT: performs the one deterministic entry-root traversal that prepares module roots and root
//! entry candidates for the rest of the build.
//! WHY: filesystem discovery belongs to Stage 0. Keeping it here prevents the frontend resolver
//! and module inventory from repeating the same expensive walk.

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::paths::module_roots::{ModuleRootRecord, ModuleRootTable};
use crate::compiler_frontend::source_libraries::root_file::file_name_is_hash_root_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use std::collections::VecDeque;
use std::fs;
use std::path::{Path, PathBuf};

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
    pub(crate) module_roots_found: usize,
    pub(crate) duplicate_hash_root_dirs: usize,
}

/// Directory names and configured output boundaries excluded from entry-root traversal.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct SourceTreeSkipPolicy {
    configured_directories: Vec<PathBuf>,
}

/// One directory that contains multiple current hash-root files.
///
/// Phase 2 records this evidence without rejecting it because the live temporary `#mod.bst`
/// export-file selection and multi-entry page model are removed only when root roles are unified
/// in later phases.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct DuplicateHashRootDirectory {
    pub(crate) directory: PathBuf,
    pub(crate) files: Vec<PathBuf>,
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

/// Canonical module roots, root entry candidates and traversal evidence for one directory build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceTreeIndex {
    entry_root: PathBuf,
    module_roots: ModuleRootTable,
    entry_candidates: Vec<PathBuf>,
    duplicate_hash_root_directories: Vec<DuplicateHashRootDirectory>,
    stats: SourceTreeDiscoveryStats,
}

impl SourceTreeIndex {
    /// Build the index with one deterministic traversal of the configured entry root.
    pub(super) fn discover(
        entry_root: PathBuf,
        project_root: &Path,
        config: &Config,
        string_table: &mut StringTable,
    ) -> Result<Self, CompilerMessages> {
        let discovery_start = crate::timing::start_pipeline_timing();
        let skip_policy = SourceTreeSkipPolicy::from_config(project_root, &entry_root, config);
        let mut stats = SourceTreeDiscoveryStats::default();
        let mut queue = VecDeque::from([entry_root.clone()]);
        let mut records = Vec::new();
        let mut entry_candidates = Vec::new();
        let mut duplicate_directories = Vec::new();

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
            let mut hash_root_files = Vec::new();

            for entry in entries {
                let path = entry.path();

                if path.is_dir() {
                    if skip_policy.should_skip(&path) {
                        stats.dirs_skipped += 1;
                    } else {
                        subdirectories.push(path);
                    }
                    continue;
                }

                if !path.is_file() {
                    continue;
                }

                stats.files_seen += 1;
                if !path_is_hash_root_file(&path) {
                    continue;
                }

                stats.hash_root_files_seen += 1;
                hash_root_files.push(
                    fs::canonicalize(&path)
                        .map_err(|error| {
                            CompilerError::file_error(
                                &path,
                                format!("Failed to canonicalize module root path: {error}"),
                                string_table,
                            )
                        })
                        .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?,
                );
            }

            if !hash_root_files.is_empty() {
                hash_root_files.sort();
                let root_directory = fs::canonicalize(&directory)
                    .map_err(|error| {
                        CompilerError::file_error(
                            &directory,
                            format!("Failed to canonicalize module root directory: {error}"),
                            string_table,
                        )
                    })
                    .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

                if hash_root_files.len() > 1 {
                    stats.duplicate_hash_root_dirs += 1;
                    duplicate_directories.push(DuplicateHashRootDirectory {
                        directory: root_directory.clone(),
                        files: hash_root_files.clone(),
                    });
                }

                stats.module_roots_found += 1;
                // Keep the current `#mod.bst` choice isolated until the later root-role
                // transition. The selected path is carried as prepared export identity below.
                let export_file = hash_root_files
                    .iter()
                    .find(|file| root_file_name_is_mod(file))
                    .cloned();
                let Some(root_file) = hash_root_files
                    .iter()
                    .find(|file| !root_file_name_is_mod(file))
                    .cloned()
                    .or_else(|| export_file.clone())
                else {
                    continue;
                };

                entry_candidates.extend(
                    hash_root_files
                        .iter()
                        .filter(|file| !root_file_name_is_mod(file))
                        .cloned(),
                );

                records.push(ModuleRootRecord::with_export_file(
                    root_directory,
                    root_file,
                    export_file,
                ));
            }

            subdirectories.sort();
            queue.extend(subdirectories);
        }

        record_discovery_metrics(&stats, discovery_start);
        entry_candidates.sort();

        Ok(Self {
            entry_root,
            module_roots: ModuleRootTable::from_records(records),
            entry_candidates,
            duplicate_hash_root_directories: duplicate_directories,
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
        string_table: &mut StringTable,
    ) -> Result<ModuleRootTable, CompilerMessages> {
        if !path_is_hash_root_file(entry_file) {
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

    pub(crate) fn entry_candidates(&self) -> &[PathBuf] {
        &self.entry_candidates
    }

    #[cfg(test)]
    pub(crate) fn duplicate_hash_root_directories(&self) -> &[DuplicateHashRootDirectory] {
        &self.duplicate_hash_root_directories
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

fn path_is_hash_root_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(file_name_is_hash_root_file)
}

// Temporary Phase 3 selector: `#mod.bst` remains the prepared export file when present until
// the later root-role transition removes filename-specific identity.
fn root_file_name_is_mod(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "#mod.bst")
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
        "source_tree_index.module_roots_found",
        stats.module_roots_found as f64,
    );
    crate::timing::record_counter(
        "source_tree_index.duplicate_hash_root_dirs",
        stats.duplicate_hash_root_dirs as f64,
    );
}
