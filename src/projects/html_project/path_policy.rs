//! Entry-root and homepage path policy for HTML projects.
//!
//! WHAT: resolves the configured entry root and derives homepage expectations for directory builds.
//! WHY: the HTML project builder should focus on artifact orchestration, not filesystem policy.

use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType,
};
use crate::compiler_frontend::paths::path_resolution::resolve_project_entry_root;
use crate::projects::settings::Config;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) struct HtmlEntryPathPlan {
    pub(crate) resolved_entry_root: Option<PathBuf>,
    pub(crate) expected_homepage_entry: Option<PathBuf>,
    is_directory_build: bool,
}

impl HtmlEntryPathPlan {
    /// Build the entry-path plan for one HTML project build.
    ///
    /// WHY: canonical entry-root resolution needs to happen once so page output logic and
    /// homepage validation operate on the same normalized paths.
    pub(crate) fn from_config(config: &Config) -> Result<Self, CompilerMessages> {
        let is_directory_build = config.entry_dir.is_dir();
        let resolved_entry_root = resolve_canonical_entry_root(config, is_directory_build)?;
        let expected_homepage_entry = resolved_entry_root
            .as_ref()
            .map(|entry_root| entry_root.join("#page.bst"));

        Ok(Self {
            resolved_entry_root,
            expected_homepage_entry,
            is_directory_build,
        })
    }

    /// Enforce the HTML homepage requirement for directory builds.
    ///
    /// WHY: directory routing depends on a concrete root `#page.bst`, while single-file builds do
    /// not have that contract.
    pub(crate) fn require_homepage_if_directory_build(
        &self,
        config: &Config,
        has_directory_homepage: bool,
    ) -> Result<(), CompilerMessages> {
        if self.is_directory_build && !has_directory_homepage {
            return Err(missing_homepage_error(
                config,
                self.resolved_entry_root.as_deref(),
            ));
        }

        Ok(())
    }
}

/// Resolve and canonicalize the entry root for directory builds.
///
/// Returns `None` for single-file builds and `Some(canonical_path)` for directory builds.
fn resolve_canonical_entry_root(
    config: &Config,
    is_directory_build: bool,
) -> Result<Option<PathBuf>, CompilerMessages> {
    if !is_directory_build {
        return Ok(None);
    }

    let entry_root_path = resolve_project_entry_root(config);
    let canonical = fs::canonicalize(&entry_root_path).map_err(|error| {
        CompilerMessages::from_error(CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Failed to resolve configured HTML entry root '{}': {error}",
                entry_root_path.display()
            ),
        ))
    })?;

    Ok(Some(canonical))
}

fn missing_homepage_error(config: &Config, resolved_entry_root: Option<&Path>) -> CompilerMessages {
    let entry_root = resolved_entry_root.unwrap_or_else(|| Path::new("."));
    let mut error = CompilerError::file_error(
        &config.entry_dir,
        format!(
            "HTML project builds require a '#page.bst' homepage at the root of the configured entry root '{}'.",
            entry_root.display(),
        ),
    )
    .with_error_type(ErrorType::Config);
    error.metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        format!("Create a '#page.bst' file in '{}'", entry_root.display()),
    );
    CompilerMessages::from_error(error)
}
