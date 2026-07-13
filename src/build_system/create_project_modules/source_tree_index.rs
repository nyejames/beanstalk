//! Stage 0 source-tree indexing for directory projects.
//!
//! WHAT: performs the one deterministic entry-root traversal that prepares module roots, root
//! entry candidates, sibling import-name collision facts, and entry-root source-library prefix
//! collision facts for the rest of the build.
//! WHY: filesystem discovery belongs to Stage 0. Keeping it here prevents the frontend resolver,
//! module inventory, and collision validators from repeating the same expensive walk.

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::module_roots::{ModuleRootRecord, ModuleRootTable};
use crate::compiler_frontend::source_libraries::root_file::file_name_is_hash_root_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::SourceLibraryRegistry;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::project_structure_diagnostics::{path_id, project_structure_messages};

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

/// Canonical module roots, root entry candidates and traversal evidence for one directory build.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct SourceTreeIndex {
    entry_root: PathBuf,
    module_roots: ModuleRootTable,
    entry_candidates: Vec<PathBuf>,
    stats: SourceTreeDiscoveryStats,
}

impl SourceTreeIndex {
    /// Build the index with one deterministic traversal of the configured entry root.
    ///
    /// The traversal also owns entry-root sibling `.bst` file/folder import-name collisions and
    /// entry-root folder/source-library-prefix collisions, using the same sorted directory
    /// entries it already reads. Skipped directories neither contribute collision facts nor get
    /// recursively scanned. Source-library-tree collision validation remains separate because
    /// registered source-library traversal lives outside entry-root indexing.
    pub(super) fn discover(
        entry_root: PathBuf,
        project_root: &Path,
        config: &Config,
        source_libraries: &SourceLibraryRegistry,
        string_table: &mut StringTable,
    ) -> Result<Self, CompilerMessages> {
        let discovery_start = crate::timing::start_pipeline_timing();
        let skip_policy = SourceTreeSkipPolicy::from_config(project_root, &entry_root, config);
        let mut stats = SourceTreeDiscoveryStats::default();
        let mut queue = VecDeque::from([entry_root.clone()]);
        let mut records = Vec::new();
        let mut entry_candidates = Vec::new();

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
            let mut bst_file_stems: BTreeSet<String> = BTreeSet::new();
            let mut importable_folder_names: BTreeSet<String> = BTreeSet::new();

            for entry in entries {
                let path = entry.path();

                if path.is_dir() {
                    if skip_policy.should_skip(&path) {
                        stats.dirs_skipped += 1;
                    } else {
                        if let Some(folder_name) = path.file_name().and_then(|name| name.to_str()) {
                            importable_folder_names.insert(folder_name.to_owned());
                        }
                        subdirectories.push(path);
                    }
                    continue;
                }

                if !path.is_file() {
                    continue;
                }

                stats.files_seen += 1;

                if let Some(stem) = bst_file_stem(&path) {
                    bst_file_stems.insert(stem.to_owned());
                }

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
            // source-library import prefixes.
            if directory == entry_root {
                for folder_name in &importable_folder_names {
                    if source_libraries.has_prefix(folder_name) {
                        let colliding_folder = directory.join(folder_name);
                        return Err(project_structure_messages(
                            &colliding_folder,
                            InvalidConfigReason::EntryRootLibraryPrefixCollision {
                                prefix: string_table.intern(folder_name),
                                entry_folder: path_id(&colliding_folder, string_table),
                            },
                            string_table,
                        ));
                    }
                }
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
                    let candidates = hash_root_files
                        .iter()
                        .map(|path| path_id(path, string_table))
                        .collect();
                    return Err(project_structure_messages(
                        &root_directory,
                        InvalidConfigReason::MultipleModuleRootFiles {
                            directory: path_id(&root_directory, string_table),
                            candidates,
                        },
                        string_table,
                    ));
                }

                stats.module_roots_found += 1;
                let root_file = hash_root_files
                    .pop()
                    .expect("non-empty hash-root list has one root after duplicate validation");

                entry_candidates.push(root_file.clone());

                records.push(ModuleRootRecord::new(root_directory, root_file));
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
        source_libraries: &SourceLibraryRegistry,
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
            source_libraries,
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

/// Extract the import-name stem from a `.bst` file path, or `None` for other extensions.
fn bst_file_stem(path: &Path) -> Option<&str> {
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
        "source_tree_index.module_roots_found",
        stats.module_roots_found as f64,
    );
}
