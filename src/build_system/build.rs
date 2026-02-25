//! Core build orchestration and output writing for Beanstalk projects.
//!
//! This module provides the canonical project build flow (`build_project`) and a dedicated output
//! writer (`write_project_outputs`). Build tools can compile once and choose where artifacts are
//! written without reimplementing frontend/backend orchestration.

use crate::build_system::create_project_modules::{ExternalImport, compile_project_frontend};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::Config;
use saying::say;
use std::fs;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

pub struct Module {
    pub(crate) folder_name: String,
    pub(crate) entry_point: PathBuf, // The name of the main start function
    pub(crate) hir: HirModule,
    pub(crate) borrow_analysis: BorrowCheckReport,
    pub(crate) required_module_imports: Vec<ExternalImport>,
    pub(crate) exported_functions: Vec<String>,
    pub(crate) warnings: Vec<CompilerWarning>,
    pub(crate) string_table: StringTable,
}

/// Unified build interface for all project types
pub trait ProjectBuilder {
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

/// Build a Beanstalk project by running path validation, frontend compilation, and backend build.
///
/// This function intentionally does not write output files so callers can decide where artifacts
/// should be emitted.
pub fn build_project(
    project_builder: &dyn ProjectBuilder,
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
    let modules = compile_project_frontend(&mut config, flags)?;
    let mut warnings = collect_frontend_warnings(&modules);

    // --------------------------------------------
    // BUILD PROJECT USING THE APPROPRIATE BUILDER
    // --------------------------------------------
    let start = Instant::now();
    let project = match project_builder.build_backend(modules, &config, flags) {
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
