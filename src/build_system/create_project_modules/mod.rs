//! Frontend compilation coordinator for Beanstalk projects.
//!
//! Dispatches to single-file or directory-project flows, then delegates to focused submodules:
//! - `frontend_orchestration`   — per-module pipeline (tokenization through borrow checking)
//! - `module_discovery`         — project-level entry-file and reachable-file discovery
//! - `reachable_file_discovery` — BFS traversal over import graphs
//! - `import_scanning`          — per-file import path extraction
//! - `source_loading`           — raw file I/O
//!
//! Stage 0 config loading lives in `project_config`. This module begins after config has been
//! applied to `Config`.

mod compilation;
mod frontend_orchestration;
mod import_scanning;
mod module_discovery;
mod reachable_file_discovery;
mod source_loading;

#[cfg(test)]
pub(super) use module_discovery::{DiscoveredModule, discover_all_modules_in_project};
pub use source_loading::extract_source_code;

#[cfg(test)]
pub(crate) use crate::projects::settings;
#[cfg(test)]
pub(crate) use std::fs;

use crate::build_system::build::Module;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::{Flag, FrontendBuildProfile};
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};

/// Compile all project modules through the frontend pipeline.
///
/// WHAT: dispatches to single-file or directory-project flow depending on the entry path.
/// WHY: separating the two flows keeps each path readable as orchestration over named steps.
pub fn compile_project_frontend(
    config: &mut Config,
    flags: &[Flag],
    style_directives: &StyleDirectiveRegistry,
    string_table: &mut StringTable,
) -> Result<Vec<Module>, CompilerMessages> {
    let build_profile = if flags.contains(&Flag::Release) {
        FrontendBuildProfile::Release
    } else {
        FrontendBuildProfile::Dev
    };

    // Dispatch: single-file entry vs. directory project.
    if let Some(extension) = config.entry_dir.extension() {
        return compilation::compile_single_file_frontend(
            config,
            build_profile,
            style_directives,
            extension,
            string_table,
        );
    }

    if !config.entry_dir.is_dir() {
        use crate::compiler_frontend::compiler_errors::CompilerError;
        let err = CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Found a file without an extension set. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
            ),
            string_table,
        );
        return Err(CompilerMessages::from_error_ref(err, string_table));
    }

    compilation::compile_directory_frontend(config, build_profile, style_directives, string_table)
}

#[cfg(test)]
#[path = "../tests/create_project_modules_tests.rs"]
mod create_project_modules_tests;

#[cfg(test)]
#[path = "../tests/compile_project_frontend_tests.rs"]
mod compile_project_frontend_tests;
