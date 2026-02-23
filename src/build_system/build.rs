use crate::build_system::create_project_modules::{ExternalImport, compile_project_frontend};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::basic_utility_functions::check_if_valid_path;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::Config;
use crate::return_messages_with_err;
use saying::say;
use std::fs;
use std::path::PathBuf;
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
    pub full_file_path: PathBuf,
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
    pub fn new(full_file_path: PathBuf, file_kind: FileKind) -> Self {
        Self {
            full_file_path,
            file_kind,
        }
    }

    pub(crate) fn file_kind(&self) -> &FileKind {
        &self.file_kind
    }
}
pub struct Project {
    pub output_files: Vec<OutputFile>,
    pub warnings: Vec<CompilerWarning>,
}

/// Build a Beanstalk project with an explicit target specification
///
/// Extended version of [`build_project`] that allows overriding the automatic
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
pub fn build_project(
    project_builder: Box<dyn ProjectBuilder>,
    entry_path: &str,
    flags: &[Flag],
) -> CompilerMessages {
    let _time = Instant::now();

    // For early returns before using the compiler_frontend messages from the actual compiler_frontend pipeline later
    let mut messages = CompilerMessages::new();

    let valid_path = match check_if_valid_path(entry_path) {
        Ok(path) => path,
        Err(e) => {
            return_messages_with_err!(messages, e);
        }
    };

    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            let err =
                CompilerError::compiler_error(format!("Could not get the current directory: {e}"));
            return_messages_with_err!(messages, err);
        }
    };

    say!("\nCompiling Project");
    // --------------------------------------------
    //   PERFORM THE CORE COMPILER FRONTEND BUILD
    // --------------------------------------------
    // This discovers all the modules, parses the config
    // and compiles each module to an HirModule for the backend to use
    let mut config = Config::new(valid_path);
    let modules = match compile_project_frontend(&mut config, flags) {
        Ok(modules) => modules,
        Err(e) => return e,
    };

    // --------------------------------------------
    // BUILD PROJECT USING THE APPROPRIATE BUILDER
    // --------------------------------------------
    let start = Instant::now();
    let output_files = match project_builder.build_backend(modules, &config, flags) {
        Ok(project) => {
            let duration = start.elapsed();

            // Show build results
            say!(
                "\nBuilt ",
                Blue project.output_files.len(),
                Reset " files successfully in: ",
                Green Bold #duration
            );

            project.output_files
        }

        Err(compiler_messages) => return compiler_messages,
    };

    // If the NoOutputFiles flag is set, don't write any files
    if flags.contains(&Flag::NoOutputFiles) {
        return messages;
    }

    // Now write the output files returned from the builder
    for output_file in output_files {
        // A safety check to make sure the file name has been set
        // This is to avoid accidentally overwriting things by mistake
        if output_file.full_file_path == PathBuf::new() {
            let err = CompilerError::compiler_error("File did not have a name or path set");
            return_messages_with_err!(messages, err);
        }

        let full_file_path = current_dir.clone().join(output_file.full_file_path);

        // TODO: need to think more about guards and check for where these files are being written
        // And prevent build systems from writing things to weird places or doing unexpected things.
        // TODO: For this the full file path needs to be sanitised
        // The places a project builder can build to should be sandboxed and not allowed to write to any parent of the current as a minimum

        // Otherwise create the file and fill it with the code
        if let Err(e) = match output_file.file_kind {
            FileKind::NotBuilt => continue,
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
            let err = CompilerError::compiler_error(format!("Error writing file: {e}"));
            return_messages_with_err!(messages, err);
        };
    }

    messages
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
