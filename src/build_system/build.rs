//! Core build orchestration and output writing for Beanstalk projects.
//!
//! This module provides the canonical project build flow (`build_project`) and a dedicated output
//! writer (`write_project_outputs`). Build tools can compile once and choose where artifacts are
//! written without reimplementing frontend/backend orchestration.

use crate::build_system::create_project_modules::{compile_project_frontend, load_project_config};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::projects::settings::Config;
use saying::say;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

pub struct Module {
    pub(crate) entry_point: PathBuf, // The name of the main start function
    pub(crate) hir: HirModule,
    pub(crate) borrow_analysis: BorrowCheckReport,
    pub(crate) warnings: Vec<CompilerWarning>,
    pub(crate) string_table: StringTable,
}

/// Unified build interface for all project types
pub trait BackendBuilder {
    /// Build the project with the given configuration
    fn build_backend(
        &self,
        modules: Vec<Module>, // Each collection of files the frontend has compiled into modules
        config: &Config,      // Persistent settings across the whole project
        flags: &[Flag],       // Settings only relevant to this build
    ) -> Result<Project, CompilerMessages>;

    /// Validate the project configuration
    fn validate_project_config(&self, config: &Config) -> Result<(), CompilerError>;
}

pub struct ProjectBuilder {
    pub backend: Box<dyn BackendBuilder + Send>,
    pub frontend_style_directives: Vec<StyleDirectiveSpec>,
}

impl ProjectBuilder {
    pub fn new(backend: Box<dyn BackendBuilder + Send>) -> Self {
        Self {
            backend,
            frontend_style_directives: Vec::new(),
        }
    }

    pub fn with_frontend_style_directives(
        mut self,
        frontend_style_directives: Vec<StyleDirectiveSpec>,
    ) -> Self {
        self.frontend_style_directives = frontend_style_directives;
        self
    }
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
    pub warnings: Vec<CompilerWarning>,
}

/// Result of a successful core build orchestration run.
pub struct BuildResult {
    pub project: Project,
    pub config: Config,
    pub warnings: Vec<CompilerWarning>,
}

/// Options for writing a compiled project to disk.
pub struct WriteOptions {
    pub output_root: PathBuf,
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
    let valid_path = check_if_valid_path(entry_path).map_err(compiler_messages_from_error)?;

    say!("\nCompiling Project");

    // --------------------------------------------
    //   PERFORM THE CORE COMPILER FRONTEND BUILD
    // --------------------------------------------
    // This discovers all the modules, parses the config,
    // and compiles each module to HIR for backend lowering.
    let mut config = Config::new(valid_path);
    
    // WHAT: Load and validate project config before compilation begins (Stage 0)
    // WHY: Config must be validated early so backends can reject invalid settings before any work
    load_project_config(&mut config)?;
    
    // WHAT: Validate backend-specific config requirements before compilation
    // WHY: Backend validation must occur after Stage 0 loading but before any compilation work
    project_builder
        .backend
        .validate_project_config(&config)
        .map_err(compiler_messages_from_error)?;
    
    let modules = compile_project_frontend(
        &mut config,
        flags,
        &project_builder.frontend_style_directives,
    )?;
    let mut warnings = collect_frontend_warnings(&modules);

    // --------------------------------------------
    // BUILD PROJECT USING THE APPROPRIATE BUILDER
    // --------------------------------------------
    let start = Instant::now();
    let project = match project_builder
        .backend
        .build_backend(modules, &config, flags)
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
        Err(compiler_messages) => return Err(compiler_messages),
    };

    warnings.extend(project.warnings.iter().cloned());

    Ok(BuildResult {
        project,
        config,
        warnings,
    })
}

/// Write built project artifacts to the provided output root.
///
/// Artifact paths are explicit and must already include any desired extension.
pub fn write_project_outputs(
    project: &Project,
    options: &WriteOptions,
) -> Result<(), CompilerMessages> {
    fs::create_dir_all(&options.output_root).map_err(|error| {
        compiler_messages_from_error(CompilerError::file_error(
            &options.output_root,
            format!(
                "Failed to create output root '{}': {error}",
                options.output_root.display()
            ),
        ))
    })?;

    for output_file in &project.output_files {
        if matches!(output_file.file_kind(), FileKind::NotBuilt) {
            continue;
        }

        validate_relative_output_path(output_file.relative_output_path())?;

        let destination = options.output_root.join(output_file.relative_output_path());

        match output_file.file_kind() {
            FileKind::NotBuilt => {}
            FileKind::Directory => {
                fs::create_dir_all(&destination).map_err(|error| {
                    compiler_messages_from_error(CompilerError::file_error(
                        &destination,
                        format!(
                            "Failed to create output directory '{}': {error}",
                            destination.display()
                        ),
                    ))
                })?;
            }
            FileKind::Js(content) | FileKind::Html(content) => {
                create_parent_dir_if_needed(&destination)?;
                fs::write(&destination, content).map_err(|error| {
                    compiler_messages_from_error(CompilerError::file_error(
                        &destination,
                        format!(
                            "Failed to write output file '{}': {error}",
                            destination.display()
                        ),
                    ))
                })?;
            }
            FileKind::Wasm(bytes) => {
                create_parent_dir_if_needed(&destination)?;
                fs::write(&destination, bytes).map_err(|error| {
                    compiler_messages_from_error(CompilerError::file_error(
                        &destination,
                        format!(
                            "Failed to write output file '{}': {error}",
                            destination.display()
                        ),
                    ))
                })?;
            }
        }
    }

    Ok(())
}

fn collect_frontend_warnings(modules: &[Module]) -> Vec<CompilerWarning> {
    let mut warnings = Vec::new();
    for module in modules {
        warnings.extend(module.warnings.iter().cloned());
    }
    warnings
}

fn create_parent_dir_if_needed(path: &Path) -> Result<(), CompilerMessages> {
    let Some(parent) = path.parent() else {
        return Ok(());
    };

    fs::create_dir_all(parent).map_err(|error| {
        compiler_messages_from_error(CompilerError::file_error(
            parent,
            format!(
                "Failed to create parent directory '{}': {error}",
                parent.display()
            ),
        ))
    })
}

fn validate_relative_output_path(relative_output_path: &Path) -> Result<(), CompilerMessages> {
    if relative_output_path.as_os_str().is_empty() {
        return Err(compiler_messages_from_error(CompilerError::file_error(
            relative_output_path,
            "Output path cannot be empty for built artifacts.",
        )));
    }

    if relative_output_path.is_absolute() {
        return Err(compiler_messages_from_error(CompilerError::file_error(
            relative_output_path,
            "Output path must be relative, not absolute.",
        )));
    }

    for component in relative_output_path.components() {
        match component {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(compiler_messages_from_error(CompilerError::file_error(
                    relative_output_path,
                    "Output path cannot contain '..' traversal components.",
                )));
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {
                return Err(compiler_messages_from_error(CompilerError::file_error(
                    relative_output_path,
                    "Output path must only contain normal path components.",
                )));
            }
        }
    }

    Ok(())
}

fn compiler_messages_from_error(error: CompilerError) -> CompilerMessages {
    CompilerMessages {
        errors: vec![error],
        warnings: Vec::new(),
    }
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
