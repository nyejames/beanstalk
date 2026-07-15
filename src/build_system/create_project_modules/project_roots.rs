//! Project root and path-resolver setup for Stage 0.
//!
//! WHAT: interprets config paths, canonicalizes the project/entry roots, wires source-backed package
//! discovery, and constructs the shared `ProjectPathResolver`.
//! WHY: config path interpretation is build-system input preparation, while the frontend path
//! resolver should focus on resolving already-established project roots.

use crate::builder_surface::{SourceFileKindRegistry, SourcePackageRegistry};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use std::fs;
use std::path::PathBuf;

use super::collision_detection::validate_source_package_tree_collisions;
use super::project_structure_diagnostics::{config_diagnostic_messages, path_id};
use super::root_validation::validate_source_package_roots;
use super::source_package_discovery::{
    discover_project_local_source_packages, merge_source_packages, prepare_source_package_roots,
};
use super::source_tree_index::SourceTreeIndex;

/// Canonical roots used to construct project-aware path resolution.
pub(super) struct ProjectRootResolution {
    pub(super) project_root: PathBuf,
    pub(super) entry_root: PathBuf,
}

/// Canonical directory-project roots plus the one Stage 0 source-tree index built from them.
pub(super) struct ProjectPathResolverSetup {
    pub(super) resolver: ProjectPathResolver,
    pub(super) source_tree_index: SourceTreeIndex,
}

/// Build only the resolver for callers that don't need the directory module inventory.
#[cfg(test)]
pub(super) fn build_project_path_resolver(
    config: &Config,
    builder_source_packages: &SourcePackageRegistry,
    source_file_kinds: &SourceFileKindRegistry,
    string_table: &mut StringTable,
) -> Result<ProjectPathResolver, CompilerMessages> {
    build_project_path_resolver_with_index(
        config,
        builder_source_packages,
        source_file_kinds,
        string_table,
    )
    .map(|setup| setup.resolver)
}

/// Build the canonical path resolver for a directory project.
///
/// WHY: both `project_root` and `entry_root` must be canonicalized before path resolution; doing
/// this in one owner keeps config interpretation out of later module inventory and frontend paths.
pub(super) fn build_project_path_resolver_with_index(
    config: &Config,
    builder_source_packages: &SourcePackageRegistry,
    source_file_kinds: &SourceFileKindRegistry,
    string_table: &mut StringTable,
) -> Result<ProjectPathResolverSetup, CompilerMessages> {
    let roots = resolve_project_roots(config, string_table)?;

    let project_local_packages =
        discover_project_local_source_packages(config, &roots.project_root, string_table)?;

    let merged_packages = merge_source_packages(
        config,
        builder_source_packages,
        &project_local_packages,
        string_table,
    )?;

    let prepared_source_package_roots =
        prepare_source_package_roots(&merged_packages, string_table)?;
    validate_source_package_roots(&prepared_source_package_roots, string_table)?;

    let entry_root = roots.entry_root.clone();
    let source_tree_index = SourceTreeIndex::discover(
        entry_root.clone(),
        &roots.project_root,
        config,
        &merged_packages,
        string_table,
    )?;

    let resolver = ProjectPathResolver::new_with_module_roots(
        roots.project_root,
        entry_root.clone(),
        prepared_source_package_roots,
        source_file_kinds,
        source_tree_index.module_roots().clone(),
    )
    .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

    validate_source_package_tree_collisions(&merged_packages, string_table)?;

    Ok(ProjectPathResolverSetup {
        resolver,
        source_tree_index,
    })
}

/// Resolve the directory configured as the project entry root.
pub(crate) fn resolve_project_entry_root(config: &Config) -> PathBuf {
    if config.entry_root.as_os_str().is_empty() {
        return config.entry_dir.clone();
    }

    if config.entry_root.is_absolute() {
        config.entry_root.clone()
    } else {
        config.entry_dir.join(&config.entry_root)
    }
}

fn resolve_project_roots(
    config: &Config,
    string_table: &mut StringTable,
) -> Result<ProjectRootResolution, CompilerMessages> {
    let project_root = match fs::canonicalize(&config.entry_dir) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &config.entry_dir,
                format!("Failed to canonicalize project root: {error}"),
                string_table,
            );

            return Err(CompilerMessages::from_error_ref(file_error, string_table));
        }
    };

    let entry_root_path = resolve_project_entry_root(config);
    if !entry_root_path.exists() {
        return Err(config_diagnostic_messages(
            config,
            "entry_root",
            InvalidConfigReason::ConfiguredEntryRootMissing {
                entry_root: path_id(&entry_root_path, string_table),
            },
            string_table,
        ));
    }

    let entry_root = match fs::canonicalize(&entry_root_path) {
        Ok(path) => path,
        Err(error) => {
            let file_error = CompilerError::file_error(
                &entry_root_path,
                format!("Failed to canonicalize configured entry root: {error}"),
                string_table,
            );

            return Err(CompilerMessages::from_error_ref(file_error, string_table));
        }
    };

    Ok(ProjectRootResolution {
        project_root,
        entry_root,
    })
}
