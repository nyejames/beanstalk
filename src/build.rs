use crate::build_system::{embedded_project, html_project, jit_wasm};
use crate::compiler::compiler_errors::ErrorMetaDataKey::CompilationStage;
use crate::compiler::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::compiler_warnings::CompilerWarning;
use crate::settings::{BEANSTALK_FILE_EXTENSION, Config, ProjectType};
use crate::{Flag, return_compiler_error, return_file_error, settings};
use colour::{dark_cyan_ln, dark_yellow_ln, green_ln_bold, print_bold};
use std::fs::FileType;
use std::path::{Path, PathBuf};
use std::time::Instant;
use std::{fs, path};

pub struct InputModule {
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

/// Build configuration that determines how WASM files are generated and organized
#[derive(Debug, Clone)]
pub enum BuildTarget {
    /// HTML/JS project - generates separate WASM files for different HTML imports
    /// First implementation will just generate JS,
    /// then eventually core parts will move to Wasm when that part of the backend as that part of the compiler is developed
    HtmlProject,

    // TODO: Separate JS jit and Wasm jit backends for this?
    /// Just runs as wasm and doesn't generate any output files
    Jit,

    /// Embedded project - Beanstalk embedding in Rust applications
    Embedded {
        /// Whether to enable hot reloading support
        hot_reload: bool,
        /// Custom IO interface configuration
        io_config: Option<String>,
    },
}

/// Unified build interface for all project types
pub trait ProjectBuilder {
    /// Build the project with the given configuration
    fn build_project(
        &self,
        modules: Vec<InputModule>,
        config: &Config,
        release_build: bool,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages>;

    /// Get the build target type
    fn target_type(&self) -> &BuildTarget;

    /// Validate the project configuration
    fn validate_config(&self, config: &Config) -> Result<(), CompilerError>;
}

/// Create the appropriate project builder based on configuration
pub fn create_project_builder(target: BuildTarget) -> Box<dyn ProjectBuilder> {
    match target {
        BuildTarget::HtmlProject => Box::new(
            html_project::html_project_builder::HtmlProjectBuilder::new(target),
        ),
        BuildTarget::Embedded { .. } => {
            Box::new(embedded_project::EmbeddedProjectBuilder::new(target))
        }
        BuildTarget::Jit => Box::new(jit_wasm::JitProjectBuilder::new(target)),
    }
}

/// Determine a build target from project configuration
pub fn determine_build_target(config: &Config) -> BuildTarget {
    // Check if this is a single file or project
    if config.entry_point.extension().is_some() {
        // Single file - default to HTML project
        BuildTarget::HtmlProject
    } else {
        // Project directory - check config for the target type
        match &config.project_type {
            ProjectType::HTML => BuildTarget::HtmlProject,

            ProjectType::Embedded => BuildTarget::Embedded {
                hot_reload: false, // Default to false for embedded
                io_config: None,
            },

            ProjectType::Jit => BuildTarget::Jit,

            // Currently not using JIT, just parsing a string template
            ProjectType::Repl => BuildTarget::Jit,
        }
    }
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
    entry_dir: &Path,
    release_build: bool,
    flags: &[Flag],
    target_override: Option<BuildTarget>,
) -> CompilerMessages {
    let _time = Instant::now();

    // For early returns before using the compiler messages from the actual compiler pipeline later
    let mut messages = CompilerMessages::new();

    let current_dir = match std::env::current_dir() {
        Ok(dir) => dir,
        Err(e) => {
            messages.errors.push(CompilerError::file_error(
                &PathBuf::new(),
                format!("Error finding current directory: {:?}", e),
            ));
            return messages;
        }
    };

    let mut project_config = Config::new(current_dir.join(entry_dir));

    //println!("Project Directory: ");
    //dark_yellow_ln!("{:?}", &entry_dir);

    let mut beanstalk_modules_to_parse: Vec<InputModule> = Vec::with_capacity(1);

    // Determine if this is a single file or project directory
    enum CompileType {
        SingleBeanstalkFile(String), // Source Code
        MultiFile(String),           // Config Source Code
    }

    let project_config_type = if entry_dir.extension() == Some(BEANSTALK_FILE_EXTENSION.as_ref()) {
        // Single BST file
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleBeanstalkFile(content),
            Err(e) => {
                messages.errors.push(CompilerError::file_error(
                    &PathBuf::new(),
                    format!("Error reading file: {:?}", e),
                ));
                return messages;
            }
        }
    } else {
        // Full project with a config file
        dark_cyan_ln!("Reading project config...");

        let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);
        let source_code = fs::read_to_string(&config_path);
        match source_code {
            Ok(content) => CompileType::MultiFile(content),
            Err(e) => {
                messages.errors.push(CompilerError::file_error(
                    &PathBuf::new(),
                    format!("Error reading config file: {:?}", e),
                ));
                return messages;
            }
        }
    };

    // Parse configuration and collect modules
    match project_config_type {
        CompileType::SingleBeanstalkFile(code) => {
            beanstalk_modules_to_parse.push(InputModule {
                source_code: code,
                source_path: entry_dir.to_owned(),
            });
        }

        // TODO: No longer have config files working,
        // this needs to be picked up later when more complex projects are needed
        CompileType::MultiFile(config_source_code) => {
            let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);

            // Parse the config file
            // let mut tokenizer_output = match tokenize(
            //     &config_source_code,
            //     &config_path,
            //     TokenizeMode::Normal,
            // ) {
            //     Ok(tokens) => tokens,
            //     Err(e) => {
            //         return Err(CompilerMessages {
            //             errors: vec![e.with_file_path(config_path.clone())],
            //             warnings: Vec::new(),
            //         });
            //     }
            // };
            //
            // // Create the host function registry
            // let host_registry = match create_builtin_registry() {
            //     Ok(registry) => registry,
            //     Err(e) => {
            //         return Err(CompilerMessages {
            //             errors: vec![e.with_file_path(config_path.clone())],
            //             warnings: Vec::new(),
            //         });
            //     }
            // };
            //
            // let config_tokens = vec![tokenizer_output];
            // let config_ast = match Ast::new(config_tokens, &HostFunctionRegistry::new()) {
            //     Ok(config_ast) => config_ast,
            //     Err(e) => {
            //         return Err(e);
            //     }
            // };
            //
            // // Parse configuration from AST
            // if let Err(e) = get_config_from_ast(config_ast, &mut project_config) {
            //     return Err(CompilerMessages {
            //         errors: vec![e.with_file_path(config_path.clone())],
            //         warnings: Vec::new(),
            //     });
            // }
            //
            // Just use default for now

            match add_bst_files_to_parse(&mut beanstalk_modules_to_parse, &entry_dir) {
                // Currently doesn't emit warnings
                Ok(_) => {}
                Err(e) => {
                    messages.errors.push(e);
                    return messages;
                }
            }
        }
    }

    // ----------------------------------
    // DETERMINE BUILD TARGET AND BUILDER
    // ----------------------------------
    // If no override, read it from the config
    let build_target = target_override.unwrap_or_else(|| determine_build_target(&project_config));

    let project_builder = create_project_builder(build_target);

    print_bold!("Compiling with target: ");
    dark_yellow_ln!("{:?}", project_builder.target_type());

    // -------------------------------------------
    // BUILD PROJECT USING THE APPROPRIATE BUILDER
    // -------------------------------------------
    let start = Instant::now();
    let output_files = match project_builder.build_project(
        beanstalk_modules_to_parse,
        &project_config,
        release_build,
        flags,
    ) {
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

        Err(compiler_messages) => return compiler_messages,
    };

    // TODO: Now write the output files returned from the builder
    for output_file in output_files {
        // A safety check to make sure the file name as been set
        // This is to avoid accidentally overwriting things by mistake
        if output_file.full_file_path == PathBuf::new() {
            messages.errors.push(CompilerError::file_error(
                &current_dir,
                "File did not have a name or path set",
            ));

            return messages;
        }

        // Otherwise create the file and fill it with the code
        if let Err(e) = match output_file.file_kind {
            FileKind::Js(content) => {
                let file = current_dir
                    .join(output_file.full_file_path)
                    .with_extension("js");
                fs::write(file, content)
            },
            FileKind::Wasm(content) => {
                let file = current_dir
                    .join(output_file.full_file_path)
                    .with_extension("wasm");
                fs::write(file, content)
            },
            FileKind::Directory => fs::create_dir_all(
                current_dir.join(output_file.full_file_path
            )),
            FileKind::Html(content) => {
                let file = current_dir
                    .join(output_file.full_file_path)
                    .with_extension("html");
                fs::write(file, content)
            },
        } {
            messages.errors.push(CompilerError::file_error(
                &current_dir,
                format!("Error writing file: {:?}", e),
            ));
            return messages;
        };
    }

    messages
}

// Look for every subdirectory inside the dir and add all .bst files to the source_code_to_parse
fn add_bst_files_to_parse(
    source_code_to_parse: &mut Vec<InputModule>,
    project_root_dir: &Path,
) -> Result<(), CompilerError> {
    // Can't just use the src_dir from config, because this might be recursively called for new subdirectories

    // Read all files in the src directory
    let all_dir_entries: fs::ReadDir = match fs::read_dir(project_root_dir) {
        Ok(dir) => dir,
        Err(e) => {
            let error_msg: &'static str = Box::leak(
                format!(
                    "Can't find any files to parse inside this directory. Might be empty? \nError: {:?}",
                    e
                ).into_boxed_str()
            );

            let suggestion: &'static str = if e.kind() == std::io::ErrorKind::NotFound {
                "Check that the directory exists at the specified path"
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Check that you have permission to read this directory"
            } else {
                "Verify the directory is accessible"
            };

            return_file_error!(project_root_dir, error_msg, {
                CompilationStage => "File System",
                PrimarySuggestion => suggestion,
            });
        }
    };

    for file in all_dir_entries {
        match file {
            Ok(f) => {
                let file_path = f.path();

                // If it's a .bst file, add it to the list of files to compile
                if file_path.extension() == Some(BEANSTALK_FILE_EXTENSION.as_ref()) {
                    let code = match fs::read_to_string(&file_path) {
                        Ok(content) => content,
                        Err(e) => {
                            let error_msg: &'static str = Box::leak(
                                format!(
                                    "Error reading file when adding new bst files to parse: {:?}",
                                    e
                                )
                                .into_boxed_str(),
                            );

                            let suggestion: &'static str =
                                if e.kind() == std::io::ErrorKind::NotFound {
                                    "Check that the file exists at the specified path"
                                } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                                    "Check that you have permission to read this file"
                                } else {
                                    "Verify the file is accessible and not corrupted"
                                };

                            return_file_error!(
                                &file_path, error_msg, {
                                    CompilationStage => "File System",
                                    PrimarySuggestion => suggestion,
                                }
                            )
                        }
                    };

                    // If code is empty, skip compiling this module
                    if code.is_empty() {
                        continue;
                    }

                    let mut global = false;

                    let _file_name = match file_path.file_stem().unwrap().to_str() {
                        Some(stem_str) => {
                            if stem_str.contains(settings::GLOBAL_PAGE_KEYWORD) {
                                global = true;
                                settings::GLOBAL_PAGE_KEYWORD.to_string()
                            } else if stem_str.contains(settings::COMP_PAGE_KEYWORD) {
                                settings::INDEX_PAGE_NAME.to_string()
                            } else {
                                stem_str.to_string()
                            }
                        }
                        None => {
                            return_file_error!(
                                &file_path,
                                "Error getting file stem - file name contains invalid characters", {
                                    CompilationStage => "File System",
                                    PrimarySuggestion => "Ensure the file name contains only valid UTF-8 characters"
                                }
                            )
                        }
                    };

                    let final_output_file = InputModule {
                        source_code: code,
                        source_path: file_path,
                    };

                    if global {
                        source_code_to_parse.insert(0, final_output_file);
                    } else {
                        source_code_to_parse.push(final_output_file);
                    }

                // If directory, recursively call add_bs_files_to_parse
                } else if file_path.is_dir() {
                    // Recursively call add_bst_files_to_parse on the new directory
                    add_bst_files_to_parse(source_code_to_parse, &file_path)?;

                    // HANDLE USING JS / HTML / CSS MIXED INTO THE PROJECT
                }

                // else if let Some(ext) = file_path.extension() {
                //     // TEMPORARY: PUT THEM DIRECTLY INTO THE OUTPUT DIRECTORY
                //     if ext == "js" || ext == "html" || ext == "css" {
                //         let file_name = file_path.file_name().unwrap().to_str().unwrap();
                //
                //         source_code_to_parse.push(TemplateModule::new(
                //             "",
                //             &file_path,
                //             &output_dir.join(file_name),
                //         ));
                //     }
                // }
            }

            Err(e) => {
                let error_msg: &'static str = Box::leak(
                    format!(
                        "Error reading directory entry when adding new bst files to parse: {:?}",
                        e
                    )
                    .into_boxed_str(),
                );

                return_file_error!(
                    project_root_dir,
                    error_msg, {
                        CompilationStage => "File System",
                        PrimarySuggestion => "Check directory permissions and file system integrity"
                    }
                )
            }
        }
    }

    Ok(())
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
