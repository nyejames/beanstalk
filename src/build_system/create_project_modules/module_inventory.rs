//! Directory-project module inventory assembly.
//!
//! WHAT: turns discovered root entry files into `DiscoveredModule` records containing all
//! transitively reachable input files.
//! WHY: module inventory is the Stage 0 bridge between filesystem discovery and parallel
//! frontend compilation; root setup and source-library validation live in sibling modules.

use crate::build_system::build::InputFile;

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;

use std::path::PathBuf;

use super::entry_discovery::discover_root_entry_files;
use super::project_roots::resolve_project_entry_root;
use super::project_structure_diagnostics::{config_diagnostic_messages, path_id};
use super::reachable_file_discovery::{
    ExternalImportDiscoveryState, collect_reachable_input_files,
};

/// Entry point and all collected source files for one discovered module.
pub(crate) struct DiscoveredModule {
    pub(crate) entry_point: PathBuf,
    pub(crate) input_files: Vec<InputFile>,
}

/// Scans the directory project for all root entry files and their reachable dependencies.
pub(crate) fn discover_all_modules_in_project(
    config: &Config,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<DiscoveredModule>, CompilerMessages> {
    let source_root = resolve_project_entry_root(config);

    if !source_root.exists() {
        return Err(config_diagnostic_messages(
            config,
            "entry_root",
            InvalidConfigReason::ConfiguredEntryRootMissing {
                entry_root: path_id(&source_root, string_table),
            },
            string_table,
        ));
    }

    let entry_points = discover_root_entry_files(project_path_resolver.entry_root(), string_table)
        .map_err(|error| CompilerMessages::from_error_ref(error, string_table))?;

    if entry_points.is_empty() {
        return Err(config_diagnostic_messages(
            config,
            "entry_root",
            InvalidConfigReason::NoRootModuleEntries {
                entry_root: path_id(project_path_resolver.entry_root(), string_table),
            },
            string_table,
        ));
    }

    let mut modules = Vec::with_capacity(entry_points.len());

    for entry_point in entry_points {
        let input_files = collect_reachable_input_files(
            &entry_point,
            project_path_resolver,
            style_directives,
            external_imports,
            string_table,
        )?;

        modules.push(DiscoveredModule {
            entry_point,
            input_files,
        });
    }

    Ok(modules)
}
