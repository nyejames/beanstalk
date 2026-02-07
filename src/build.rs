use crate::build_system::html_project;
use crate::build_system::html_project::html_project_builder::HtmlProjectBuilder;
use crate::compiler::basic_utility_functions::check_if_valid_file_path;
use crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage;
use crate::compiler::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::{Flag, return_compiler_error, return_file_error, settings};
use colour::{dark_cyan_ln, dark_yellow_ln, green_ln_bold, print_bold};
use std::fmt::format;
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fs, path};

/// Unified build interface for all project types
pub trait ProjectBuilder {
    /// Build the project with the given configuration
    fn build_project(
        &self,
        path: PathBuf,
        release_build: bool,
    ) -> Result<Project, CompilerMessages>;

    /// Validate the project configuration
    fn validate_project_config(&self, config: &Config) -> Result<(), CompilerError>;
}

pub struct InputFile {
    pub source_code: String,
    pub source_path: PathBuf,
}

pub struct OutputFile {
    pub full_file_path: PathBuf,
    file_kind: FileKind,
}

pub enum FileKind {
    Wasm(Vec<u8>),
    Js(String), // Either just glue code for web or pure JS backend
    Html(String),
    Directory, // So the build system can create empty folders if needed
}

impl OutputFile {
    pub fn new(full_file_path: PathBuf, file_kind: FileKind) -> Self {
        Self {
            full_file_path,
            file_kind,
        }
    }
}
pub struct Project {
    pub config: Config,
    pub output_files: Vec<OutputFile>,
    pub warnings: Vec<CompilerWarning>,
}

/// Build configuration that determines how files are generated and organized
#[derive(Debug, Clone)]
pub enum BuildTarget {
    /// HTML/JS project - generates separate WASM files for different HTML imports
    /// First implementation will just generate JS,
    /// then eventually core parts will move to Wasm when that part of the backend as that part of the compiler is developed
    HtmlJSProject,

    // HTMLWasmProject,
    /// Embedded project - Beanstalk embedding in Rust applications
    Interpreter,
}

/// Build a Beanstalk project with an explicit target specification
///
/// Extended version of [`build_project_files`] that allows overriding the automatic
/// target detection with a specific build target. This is useful for:
/// - Cross-compilation scenarios
/// - Testing different target configurations
/// - Build system integration with explicit target control
///
/// ## Parameters
///
/// - `entry_path`: Path to the main source file or project directory
/// - `release_build`: Whether to enable release optimizations
/// - `flags`: Compilation flags for debugging and feature control
/// - `target_override`: Optional explicit target specification
///
/// ## Target Override
///
/// When `target_override` is provided, it bypasses automatic target detection:
/// - `Some(BuildTarget::HtmlProject)`: Force HTML/WASM output
/// - `Some(BuildTarget::Native { .. })`: Force native WASM output
/// - `None`: Use automatic target detection based on project structure
pub fn build_project_files(
    project_builder: Box<dyn ProjectBuilder>,
    path: &str,
    release_build: bool,
) -> Result<CompilerMessages, CompilerError> {
    let _time = Instant::now();

    // For early returns before using the compiler messages from the actual compiler pipeline later
    let mut messages = CompilerMessages::new();

    let valid_path = check_if_valid_file_path(path)?;

    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            return_compiler_error!(format!("Could not get the current directory: {e}"));
        }
    };

    print_bold!("\nCompiling Project");

    // --------------------------------------------
    // BUILD PROJECT USING THE APPROPRIATE BUILDER
    // --------------------------------------------
    let start = Instant::now();
    let output_files = match project_builder.build_project(valid_path, release_build) {
        Ok(project) => {
            let duration = start.elapsed();

            // Show build results
            print!(
                "\nBuilt {} files successfully in: ",
                project.output_files.len()
            );
            green_ln_bold!("{:?}", duration);

            messages.warnings.extend(project.warnings);
            project.output_files
        }

        Err(compiler_messages) => return Ok(compiler_messages),
    };

    // TODO: Now write the output files returned from the builder
    for output_file in output_files {
        // A safety check to make sure the file name has been set
        // This is to avoid accidentally overwriting things by mistake
        if output_file.full_file_path == PathBuf::new() {
            return_compiler_error!("File did not have a name or path set");
        }

        let full_file_path = current_dir.clone().join(output_file.full_file_path);

        // Otherwise create the file and fill it with the code
        if let Err(e) = match output_file.file_kind {
            FileKind::Js(content) => {
                let file = full_file_path.with_extension("js");
                fs::write(file, content)
            }
            FileKind::Wasm(content) => {
                let file = full_file_path.with_extension("wasm");
                fs::write(file, content)
            }
            FileKind::Directory => fs::create_dir_all(&full_file_path),
            FileKind::Html(content) => {
                let file = full_file_path.with_extension("html");
                fs::write(file, content)
            }
        } {
            return_compiler_error!(format!("Error writing file: {:?}", e))
        };
    }

    Ok(messages)
}

// fn remove_old_files(output_dir: &Path) -> Result<(), Vec<CompileError>> {
//     // Any HTML files in the output dir not on the list of files to compile should be deleted
//     let output_dir = match release_build {
//         true => PathBuf::from(&entry_dir).join(&project_config.release_folder),
//         false => PathBuf::from(&entry_dir).join(&project_config.dev_folder),
//     };
//
//     let dir_files = match fs::read_dir(&output_dir) {
//         Ok(dir) => dir,
//         Err(e) => return_file_error!(output_dir, "Error reading output_dir directory: {:?}", e),
//     };
//
//     for file in dir_files {
//         let file = match file {
//             Ok(f) => f,
//             Err(e) => return_file_error!(
//                 output_dir,
//                 "Error reading file in when trying to delete old files: {:?}",
//                 e
//             ),
//         };
//
//         let file_path = file.path();
//
//         if (
//             // These checks are mostly here for safety to avoid accidentally deleting files
//             (  file_path.extension() == Some("html".as_ref())
//                 || file_path.extension() == Some("wasm".as_ref()))
//                 || file_path.extension() == Some("js".as_ref())  )
//
//             // If the file is not in the source code to parse, it's unnecessary
//             && !tokenised_modules.iter().any(|f| f.output_path.with_extension("") == file_path.with_extension(""))
//         {
//             match fs::remove_file(&file_path) {
//                 Ok(_) => {
//                     blue_ln!("Deleted unused file: {:?}", file_path.file_name());
//                 }
//                 Err(e) => return_file_error!(file_path, "Error deleting file: {:?}", e),
//             }
//         }
//     }
// }
