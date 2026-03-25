//! Core build orchestration and output writing for Beanstalk projects.
//!
//! This module provides the canonical project build flow (`build_project`) and a dedicated output
//! writer (`write_project_outputs`). Build tools can compile once and choose where artifacts are
//! written without reimplementing frontend/backend orchestration.

use crate::build_system::create_project_modules::compile_project_frontend;
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
use std::io;
use std::path::{Component, Path, PathBuf};
use std::time::Instant;

/// Manifest file written to the output root to track which files the build system created.
/// Used to identify stale artifacts from previous builds that should be cleaned up.
const BUILD_MANIFEST_FILENAME: &str = ".beanstalk_manifest";

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

    /// Frontend style directives provided by this backend.
    ///
    /// Core directives are always present in frontend registry construction.
    /// This hook supplies non-core directive behavior for tokenization/template parsing.
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
    let valid_path = check_if_valid_path(entry_path).map_err(CompilerMessages::from_error)?;

    say!("\nCompiling Project");

    // --------------------------------------------
    //   PERFORM THE CORE COMPILER FRONTEND BUILD
    // --------------------------------------------
    // This discovers all the modules, parses the config,
    // and compiles each module to HIR for backend lowering.
    let mut config = Config::new(valid_path);
    let frontend_style_directives = project_builder.backend.frontend_style_directives();
    let style_directives = StyleDirectiveRegistry::merged(&frontend_style_directives)
        .map_err(CompilerMessages::from_error)?;

    // WHAT: Load and validate project config before compilation begins (Stage 0)
    // WHY: Config must be validated early so backends can reject invalid settings before any work
    load_project_config(&mut config, &style_directives)?;

    // WHAT: Validate backend-specific config requirements before compilation
    // WHY: Backend validation must occur after Stage 0 loading but before any compilation work
    project_builder
        .backend
        .validate_project_config(&config)
        .map_err(CompilerMessages::from_error)?;

    let modules = compile_project_frontend(&mut config, flags, &style_directives)?;
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
/// When `options.project_entry_dir` is set, stale artifacts from previous builds are cleaned up
/// using a manifest file to track which files the build system owns.
pub fn write_project_outputs(
    project: &Project,
    options: &WriteOptions,
) -> Result<(), CompilerMessages> {
    // WHAT: Validate output root safety and read previous manifest when cleanup is enabled
    // WHY: Must happen before any writes so we can compare old vs new artifact sets
    let previous_manifest = if let Some(project_entry_dir) = &options.project_entry_dir {
        validate_output_root_is_safe(&options.output_root, project_entry_dir)?;
        read_build_manifest(&options.output_root)
    } else {
        Vec::new()
    };

    fs::create_dir_all(&options.output_root).map_err(|error| {
        CompilerMessages::from_error(CompilerError::file_error(
            &options.output_root,
            format!(
                "Failed to create output root '{}': {error}",
                options.output_root.display()
            ),
        ))
    })?;

    let mut current_output_paths: HashSet<PathBuf> = HashSet::new();

    for output_file in &project.output_files {
        if matches!(output_file.file_kind(), FileKind::NotBuilt) {
            continue;
        }

        validate_relative_output_path(output_file.relative_output_path())?;
        current_output_paths.insert(output_file.relative_output_path().to_path_buf());

        let destination = options.output_root.join(output_file.relative_output_path());

        match output_file.file_kind() {
            FileKind::NotBuilt => {}
            FileKind::Directory => {
                fs::create_dir_all(&destination).map_err(|error| {
                    CompilerMessages::from_error(CompilerError::file_error(
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
                    CompilerMessages::from_error(CompilerError::file_error(
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
                    CompilerMessages::from_error(CompilerError::file_error(
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

    // WHAT: Clean up stale artifacts and write updated manifest when cleanup is enabled
    // WHY: Artifacts from removed pages must not persist in the output folder between builds
    if options.project_entry_dir.is_some() {
        remove_stale_artifacts(
            &options.output_root,
            &current_output_paths,
            &previous_manifest,
        );
        write_build_manifest(&options.output_root, &current_output_paths)?;
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
        CompilerMessages::from_error(CompilerError::file_error(
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
        return Err(CompilerMessages::from_error(CompilerError::file_error(
            relative_output_path,
            "Output path cannot be empty for built artifacts.",
        )));
    }

    if relative_output_path.is_absolute() {
        return Err(CompilerMessages::from_error(CompilerError::file_error(
            relative_output_path,
            "Output path must be relative, not absolute.",
        )));
    }

    for component in relative_output_path.components() {
        match component {
            Component::Normal(_) => {}
            Component::ParentDir => {
                return Err(CompilerMessages::from_error(CompilerError::file_error(
                    relative_output_path,
                    "Output path cannot contain '..' traversal components.",
                )));
            }
            Component::CurDir | Component::RootDir | Component::Prefix(_) => {
                return Err(CompilerMessages::from_error(CompilerError::file_error(
                    relative_output_path,
                    "Output path must only contain normal path components.",
                )));
            }
        }
    }

    Ok(())
}

/// Reject output roots that are dangerous system paths or suspiciously far from the project.
///
/// WHY: Stale artifact cleanup deletes files, so the output root must be validated before any
/// removal to prevent accidental deletion on system-critical or unrelated paths.
fn validate_output_root_is_safe(
    output_root: &Path,
    project_entry_dir: &Path,
) -> Result<(), CompilerMessages> {
    // WHAT: Canonicalize the output root, falling back to the nearest existing ancestor
    // WHY: Symlinks or relative segments could disguise a dangerous target path
    let canonical_root = canonicalize_or_nearest_ancestor(output_root);

    if is_dangerous_system_path(&canonical_root) {
        return Err(CompilerMessages::from_error(CompilerError::file_error(
            output_root,
            format!(
                "Refusing to use '{}' as the build output root because it is a protected system path. \
                 Configure a project-relative output folder in #config.bst.",
                output_root.display()
            ),
        )));
    }

    // WHAT: Verify the output root is near the project directory
    // WHY: An output root in a completely unrelated location is likely a misconfiguration
    let canonical_project = canonicalize_or_nearest_ancestor(project_entry_dir);
    let project_parent = canonical_project.parent().unwrap_or(&canonical_project);

    let is_inside_project = canonical_root.starts_with(&canonical_project);
    let is_sibling_of_project = canonical_root.starts_with(project_parent);

    if !is_inside_project && !is_sibling_of_project {
        return Err(CompilerMessages::from_error(CompilerError::file_error(
            output_root,
            format!(
                "Build output root '{}' is not inside or adjacent to the project directory '{}'. \
                 Stale artifact cleanup requires the output root to be near the project to prevent \
                 accidental file deletion.",
                output_root.display(),
                project_entry_dir.display()
            ),
        )));
    }

    Ok(())
}

/// Canonicalize a path, falling back to the nearest existing ancestor if the path does not exist.
fn canonicalize_or_nearest_ancestor(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }

    let mut ancestor = path.to_path_buf();
    while let Some(parent) = ancestor.parent() {
        if let Ok(canonical) = fs::canonicalize(parent) {
            // Re-append the non-existent suffix to the canonical ancestor
            let suffix = path.strip_prefix(parent).unwrap_or(Path::new(""));
            return canonical.join(suffix);
        }
        ancestor = parent.to_path_buf();
    }

    path.to_path_buf()
}

/// Check whether a path matches a known dangerous system directory.
///
/// WHY: The cleanup process removes files, so it must never operate on OS-critical directories
/// like /, /usr, /etc, or their Windows equivalents.
fn is_dangerous_system_path(path: &Path) -> bool {
    let component_count = path.components().count();

    // Reject filesystem root and paths with very few components (e.g. /foo on Unix)
    if component_count < 2 {
        return true;
    }

    #[cfg(unix)]
    {
        let path_str = path.to_string_lossy();
        let dangerous_unix_paths: &[&str] = &[
            "/usr", "/bin", "/sbin", "/etc", "/var", "/lib", "/boot", "/sys", "/proc", "/dev",
            "/home", "/tmp", "/opt", "/root", "/run", "/snap", "/srv",
        ];
        for dangerous in dangerous_unix_paths {
            if path_str == *dangerous || path_str.as_ref() == format!("{dangerous}/") {
                return true;
            }
        }
    }

    #[cfg(windows)]
    {
        let path_str = path.to_string_lossy().to_lowercase();
        let dangerous_windows_paths: &[&str] = &[
            r"c:\",
            r"c:\windows",
            r"c:\program files",
            r"c:\program files (x86)",
            r"c:\users",
            r"c:\system32",
        ];
        for dangerous in dangerous_windows_paths {
            if path_str == *dangerous || path_str == dangerous.trim_end_matches('\\') {
                return true;
            }
        }
    }

    false
}

/// Read the build manifest from the output root, returning the list of previously written paths.
///
/// Returns an empty list if the manifest does not exist, is unreadable, or contains corrupt data.
/// Invalid lines (path traversal, absolute paths) are silently skipped for safety.
fn read_build_manifest(output_root: &Path) -> Vec<PathBuf> {
    let manifest_path = output_root.join(BUILD_MANIFEST_FILENAME);
    let content = match fs::read_to_string(&manifest_path) {
        Ok(content) => content,
        Err(_) => return Vec::new(),
    };

    let mut paths = Vec::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let path = PathBuf::from(line);
        if validate_relative_output_path(&path).is_ok() {
            paths.push(path);
        }
    }
    paths
}

/// Write the build manifest listing all current output artifact paths.
///
/// The manifest is a sorted, deduplicated plain text file with one relative path per line.
fn write_build_manifest(
    output_root: &Path,
    current_paths: &HashSet<PathBuf>,
) -> Result<(), CompilerMessages> {
    let manifest_path = output_root.join(BUILD_MANIFEST_FILENAME);

    let mut sorted_paths: Vec<String> = current_paths
        .iter()
        .map(|path| path.to_string_lossy().into_owned())
        .collect();
    sorted_paths.sort();

    let content = sorted_paths.join("\n");
    fs::write(&manifest_path, content).map_err(|error| {
        CompilerMessages::from_error(CompilerError::file_error(
            &manifest_path,
            format!(
                "Failed to write build manifest '{}': {error}",
                manifest_path.display()
            ),
        ))
    })
}

/// Remove files from previous builds that are no longer in the current output set.
///
/// Only removes files that appear in the previous manifest but not in the current output paths.
/// Each path is re-validated before removal. After removing a file, empty parent directories
/// are cleaned up toward (but never including) the output root.
fn remove_stale_artifacts(
    output_root: &Path,
    current_output_paths: &HashSet<PathBuf>,
    previous_manifest_paths: &[PathBuf],
) {
    let canonical_output_root = canonicalize_or_nearest_ancestor(output_root);

    for stale_relative in previous_manifest_paths {
        if current_output_paths.contains(stale_relative) {
            continue;
        }

        // Re-validate each manifest entry before deletion as defense against corrupted manifests
        if validate_relative_output_path(stale_relative).is_err() {
            continue;
        }

        let absolute_path = output_root.join(stale_relative);

        // Verify the resolved path stays within the output root after symlink resolution
        let canonical_target = canonicalize_or_nearest_ancestor(&absolute_path);
        if !canonical_target.starts_with(&canonical_output_root) {
            continue;
        }

        if absolute_path.is_file() {
            if let Err(error) = fs::remove_file(&absolute_path) {
                say!(
                    Yellow "Warning: failed to remove stale artifact '",
                    Yellow absolute_path.display(),
                    Yellow "': ",
                    Yellow error.to_string()
                );
                continue;
            }
            remove_empty_parent_dirs(output_root, &absolute_path);
        } else if absolute_path.is_dir() {
            // Only remove empty directories — never recursively delete
            let _ = remove_empty_dir_if_safe(&absolute_path);
        }
    }
}

/// Walk from a removed file's parent directory upward toward the output root, removing each
/// directory if it is empty. Stops as soon as a removal fails (directory not empty) or the
/// output root is reached.
fn remove_empty_parent_dirs(output_root: &Path, removed_file: &Path) {
    let mut current = match removed_file.parent() {
        Some(parent) => parent.to_path_buf(),
        None => return,
    };

    let output_root_canonical = canonicalize_or_nearest_ancestor(output_root);

    while current != output_root
        && canonicalize_or_nearest_ancestor(&current) != output_root_canonical
    {
        if remove_empty_dir_if_safe(&current).is_err() {
            break;
        }
        current = match current.parent() {
            Some(parent) => parent.to_path_buf(),
            None => break,
        };
    }
}

/// Attempt to remove a directory only if it is empty. Returns Ok(()) if removed, Err otherwise.
fn remove_empty_dir_if_safe(path: &Path) -> io::Result<()> {
    fs::remove_dir(path)
}

#[cfg(test)]
#[path = "build_tests.rs"]
mod tests;
