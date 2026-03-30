//! Core build orchestration and output writing for Beanstalk projects.
//!
//! This module provides the canonical project build flow (`build_project`) and a dedicated output
//! writer (`write_project_outputs`). Build tools can compile once and choose where artifacts are
//! written without reimplementing frontend/backend orchestration.

use crate::build_system::create_project_modules::compile_project_frontend;
pub use crate::build_system::output_cleanup::CleanupPolicy;
use crate::build_system::output_cleanup::{
    finalize_output_cleanup, prepare_output_cleanup, validate_relative_output_path,
};
use crate::build_system::project_config::load_project_config;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::{StyleDirectiveRegistry, StyleDirectiveSpec};
use crate::projects::settings::Config;
use saying::say;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

const FILE_MIN_UNIQUE_SYMBOLS_CAPACITY: usize = 32;

pub struct Module {
    pub(crate) entry_point: PathBuf, // Canonical entry file for the compiled module
    pub(crate) hir: HirModule,
    pub(crate) borrow_analysis: BorrowCheckReport,
    pub(crate) warnings: Vec<CompilerWarning>,
}

/// Unified build interface for all project types
pub trait BackendBuilder {
    /// Build the project with the given configuration
    fn build_backend(
        &self,
        modules: Vec<Module>, // Each collection of files the frontend has compiled into modules
        config: &Config,      // Persistent settings across the whole project
        flags: &[Flag],       // Settings only relevant to this build
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages>;

    /// Validate the project configuration
    fn validate_project_config(
        &self,
        config: &Config,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError>;

    /// Project-specific frontend style directives provided by this backend.
    ///
    /// Frontend-owned directives are always present in registry construction and cannot be
    /// overridden by project builders. This hook supplies only project-owned additions for
    /// tokenization/template parsing.
    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec>;
}

pub struct ProjectBuilder {
    pub backend: Box<dyn BackendBuilder + Send>,
}

impl ProjectBuilder {
    pub fn new(backend: Box<dyn BackendBuilder + Send>) -> Self {
        Self { backend }
    }
}

pub(crate) struct BuildBootstrap {
    pub(crate) config: Config,
    pub(crate) style_directives: StyleDirectiveRegistry,
    pub(crate) string_table: StringTable,
}

pub struct InputFile {
    pub source_code: String,
    pub source_path: PathBuf,
}

pub struct OutputFile {
    relative_output_path: PathBuf,
    file_kind: FileKind,
}

pub enum FileKind {
    // This signals for the build system to not create this file.
    // Good for error checking / LSPs etc.
    NotBuilt,

    Wasm(Vec<u8>),
    Bytes(Vec<u8>),
    Js(String), // Either just glue code for web or pure JS backend
    Html(String),
    Directory, // So the build system can create empty folders if needed
}

impl OutputFile {
    /// Create an output artifact with an explicit relative path under the chosen output root.
    pub fn new(relative_output_path: PathBuf, file_kind: FileKind) -> Self {
        Self {
            relative_output_path,
            file_kind,
        }
    }

    /// Relative output path including any desired extension.
    pub fn relative_output_path(&self) -> &Path {
        &self.relative_output_path
    }

    pub(crate) fn file_kind(&self) -> &FileKind {
        &self.file_kind
    }
}

pub struct Project {
    pub output_files: Vec<OutputFile>,
    pub entry_page_rel: Option<PathBuf>,
    /// Builder-owned cleanup contract for manifest tracking and stale artifact removal.
    pub cleanup_policy: CleanupPolicy,
    pub warnings: Vec<CompilerWarning>,
}

/// Result of a successful core build orchestration run.
pub struct BuildResult {
    pub project: Project,
    pub config: Config,
    pub warnings: Vec<CompilerWarning>,
    pub string_table: StringTable,
}

/// Options for writing a compiled project to disk.
pub struct WriteOptions {
    pub output_root: PathBuf,
    /// When set, enables stale artifact cleanup via manifest tracking and output root safety
    /// validation. Should be the project's entry directory so safety checks can verify the output
    /// root is in a sensible location relative to the project.
    pub project_entry_dir: Option<PathBuf>,
}

/// Resolve the output root for a directory project based on the build profile.
///
/// The config owns the default folder names. If a config explicitly clears a folder path, outputs
/// fall back to the project root.
pub fn resolve_project_output_root(config: &Config, flags: &[Flag]) -> PathBuf {
    let release_build = flags.contains(&Flag::Release);
    let configured_folder = if release_build {
        &config.release_folder
    } else {
        &config.dev_folder
    };

    if configured_folder.is_absolute() {
        return configured_folder.clone();
    }

    if configured_folder.as_os_str().is_empty() {
        return config.entry_dir.clone();
    }

    config.entry_dir.join(configured_folder)
}

/// Build a Beanstalk project by running path validation, frontend compilation, and backend build.
///
/// This function intentionally does not write output files so callers can decide where artifacts
/// should be emitted.
pub fn build_project(
    project_builder: &ProjectBuilder,
    entry_path: &str,
    flags: &[Flag],
) -> Result<BuildResult, CompilerMessages> {
    let mut path_string_table = StringTable::new();
    let valid_path = check_if_valid_path(entry_path, &mut path_string_table)
        .map_err(|error| CompilerMessages::from_error(error, path_string_table))?;

    say!("\nCompiling Project");

    // --------------------------------------------
    //   PERFORM THE CORE COMPILER FRONTEND BUILD
    // --------------------------------------------
    // This discovers all the modules, parses the config,
    // and compiles each module to HIR for backend lowering.
    let BuildBootstrap {
        mut config,
        style_directives,
        mut string_table,
    } = bootstrap_project_build(project_builder, valid_path)?;

    let modules =
        compile_project_frontend(&mut config, flags, &style_directives, &mut string_table)?;
    let mut warnings = collect_frontend_warnings(&modules);

    // --------------------------------------------
    // BUILD PROJECT USING THE APPROPRIATE BUILDER
    // --------------------------------------------
    let start = Instant::now();
    let project =
        match project_builder
            .backend
            .build_backend(modules, &config, flags, &mut string_table)
        {
            Ok(project) => {
                let duration = start.elapsed();
                say!(
                    "\nBuilt ",
                    Blue project.output_files.len(),
                    Reset " files successfully in: ",
                    Green Bold #duration
                );
                project
            }
            Err(mut compiler_messages) => {
                compiler_messages.string_table = string_table;
                return Err(compiler_messages);
            }
        };

    warnings.extend(project.warnings.iter().cloned());

    Ok(BuildResult {
        project,
        config,
        warnings,
        string_table,
    })
}

/// Build the shared Stage 0/bootstrap state used by both CLI builds and the dev server.
///
/// WHAT: merges frontend/project directives, loads `#config.bst`, and runs backend-specific
/// config validation into one reusable setup step.
/// WHY: directory builds and the dev server must share one bootstrap path so config/output
/// behavior does not drift between "build" and "serve" flows.
pub(crate) fn bootstrap_project_build(
    project_builder: &ProjectBuilder,
    entry_path: PathBuf,
) -> Result<BuildBootstrap, CompilerMessages> {
    let mut config = Config::new(entry_path);

    // Create a new string table for interning strings
    let mut string_table = StringTable::with_capacity(FILE_MIN_UNIQUE_SYMBOLS_CAPACITY);

    let frontend_style_directives = project_builder.backend.frontend_style_directives();
    let style_directives = StyleDirectiveRegistry::merged(&frontend_style_directives)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

    // WHAT: Load and validate project config before compilation begins (Stage 0).
    // WHY: Backends and serving code both depend on the same validated config surface.
    load_project_config(&mut config, &style_directives, &mut string_table)?;

    // WHAT: Validate backend-specific config requirements before compilation.
    // WHY: Backends should reject unsupported settings before frontend compilation does work.
    project_builder
        .backend
        .validate_project_config(&config, &mut string_table)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

    Ok(BuildBootstrap {
        config,
        style_directives,
        string_table,
    })
}

/// Write built project artifacts to the provided output root.
///
/// Artifact paths are explicit and must already include any desired extension.
/// When `options.project_entry_dir` is set, stale artifacts from previous builds are cleaned up
/// using a manifest file to track which files the build system owns.
pub fn write_project_outputs(
    project: &Project,
    options: &WriteOptions,
    string_table: &StringTable,
) -> Result<(), CompilerMessages> {
    let cleanup_state = prepare_output_cleanup(
        &options.output_root,
        options.project_entry_dir.as_deref(),
        &project.cleanup_policy,
        string_table,
    )?;

    fs::create_dir_all(&options.output_root).map_err(|error| {
        file_error_messages(
            &options.output_root,
            format!(
                "Failed to create output root '{}': {error}",
                options.output_root.display()
            ),
            string_table,
        )
    })?;

    let mut current_managed_artifact_paths: HashSet<PathBuf> = HashSet::new();

    for output_file in &project.output_files {
        if matches!(output_file.file_kind(), FileKind::NotBuilt) {
            continue;
        }

        let relative_output_path = output_file.relative_output_path();
        validate_relative_output_path(relative_output_path, string_table)?;
        if !matches!(output_file.file_kind(), FileKind::Directory)
            && (project.cleanup_policy.manages_path(relative_output_path)
                || matches!(output_file.file_kind(), FileKind::Bytes(_)))
        {
            current_managed_artifact_paths.insert(relative_output_path.to_path_buf());
        }

        let destination = options.output_root.join(relative_output_path);

        match output_file.file_kind() {
            FileKind::NotBuilt => {}
            FileKind::Directory => {
                fs::create_dir_all(&destination).map_err(|error| {
                    file_error_messages(
                        &destination,
                        format!(
                            "Failed to create output directory '{}': {error}",
                            destination.display()
                        ),
                        string_table,
                    )
                })?;
            }
            FileKind::Js(content) | FileKind::Html(content) => {
                create_parent_dir_if_needed(&destination, string_table)?;
                fs::write(&destination, content).map_err(|error| {
                    file_error_messages(
                        &destination,
                        format!(
                            "Failed to write output file '{}': {error}",
                            destination.display()
                        ),
                        string_table,
                    )
                })?;
            }
            FileKind::Wasm(bytes) | FileKind::Bytes(bytes) => {
                create_parent_dir_if_needed(&destination, string_table)?;
                fs::write(&destination, bytes).map_err(|error| {
                    file_error_messages(
                        &destination,
                        format!(
                            "Failed to write output file '{}': {error}",
                            destination.display()
                        ),
                        string_table,
                    )
                })?;
            }
        }
    }

    // WHAT: Clean up stale artifacts and write updated manifest when cleanup is enabled
    // WHY: Artifacts from removed pages must not persist in the output folder between builds
    finalize_output_cleanup(
        &cleanup_state,
        &options.output_root,
        &current_managed_artifact_paths,
        &project.cleanup_policy,
        string_table,
    )?;

    Ok(())
}

fn collect_frontend_warnings(modules: &[Module]) -> Vec<CompilerWarning> {
    let mut warnings = Vec::new();
    for module in modules {
        warnings.extend(module.warnings.iter().cloned());
    }
    warnings
}

fn create_parent_dir_if_needed(
    path: &Path,
    string_table: &StringTable,
) -> Result<(), CompilerMessages> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|error| {
        file_error_messages(
            parent,
            format!(
                "Failed to create parent directory '{}': {error}",
                parent.display()
            ),
            string_table,
        )
    })
}

fn file_error_messages(
    path: &Path,
    msg: impl Into<String>,
    string_table: &StringTable,
) -> CompilerMessages {
    CompilerMessages::file_error(path, msg, string_table)
}

#[cfg(test)]
#[path = "tests/build_tests.rs"]
mod tests;
