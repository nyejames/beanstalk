use crate::build_system::build_system::{
    BuildTarget, create_project_builder, determine_build_target,
};
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::compiler::host_functions::registry::create_builtin_registry;
use crate::compiler::parsers::build_ast::{ContextKind, ScopeContext, new_ast};
use crate::compiler::parsers::tokenizer;
use crate::compiler::parsers::tokens::TokenizeMode;
use crate::settings::{BEANSTALK_FILE_EXTENSION, Config, get_config_from_ast};
use crate::{Flag, settings};
use colour::{dark_cyan_ln, dark_yellow_ln, print_bold};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct InputModule {
    pub source_code: String,
    pub source_path: PathBuf,
}

pub enum OutputFile {
    Wasm(Vec<u8>),
    Html(String),
}

pub struct Project {
    pub config: Config,
    pub output_files: Vec<OutputFile>,
}

/// Build a Beanstalk project from source files
///
/// This is the main entry point for compiling Beanstalk projects. It handles both
/// single-file compilation and multi-file project builds with automatic dependency
/// resolution and target detection.
///
/// ## Parameters
///
/// - `entry_path`: Path to the main source file or project directory
/// - `release_build`: Whether to enable release optimizations
/// - `flags`: Compilation flags for debugging and feature control
///
/// ## Returns
///
/// A [`Project`] containing the compiled configuration and output files, or a vector
/// of [`CompileError`]s if compilation fails.
///
/// ## Supported Targets
///
/// - **HTML Projects**: Generate WASM + HTML with JavaScript bindings
/// - **Native Projects**: Generate standalone WASM for native execution
/// - **Single Files**: Compile individual `.bst` files with automatic target detection
pub fn build_project_files(
    entry_path: &Path,
    release_build: bool,
    flags: &[Flag],
) -> Result<Project, CompilerMessages> {
    build_project_files_with_target(entry_path, release_build, flags, None)
}

/// Build a Beanstalk project with explicit target specification
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
pub fn build_project_files_with_target(
    entry_path: &Path,
    release_build: bool,
    flags: &[Flag],
    target_override: Option<BuildTarget>,
) -> Result<Project, CompilerMessages> {
    let _time = Instant::now();

    let entry_dir = match std::env::current_dir() {
        Ok(dir) => dir.join(entry_path),
        Err(e) => {
            return Err(
                CompilerMessages {
                    errors: vec![
                        CompileError::file_error(
                            &entry_path,
                            &format!(
                            "Error finding current directory: {:?}",
                            e),
                        )
                    ],
                warnings: Vec::new(),
            })
        }
    };

    // print_ln_bold!("Project Directory: ");
    // dark_yellow_ln!("{:?}", &entry_dir);

    let mut beanstalk_modules_to_parse: Vec<InputModule> = Vec::with_capacity(1);
    let mut project_config = Config::default();

    // Determine if this is a single file or project directory
    enum CompileType {
        SingleBeanstalkFile(String), // Source Code
        MultiFile(String),           // Config Source Code
        #[allow(dead_code)]
        SingleMarkthroughFile(String), // Source Code - for future use
    }

    let project_config_type = if entry_dir.extension() == Some(BEANSTALK_FILE_EXTENSION.as_ref()) {
        // Single BST file
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleBeanstalkFile(content),
            Err(e) => return Err(CompilerMessages {
                errors: vec![CompileError::file_error(&entry_dir, &format!("Error reading file: {:?}", e))],
                warnings: Vec::new(),
            }),
        }
    } else {
        // Full project with a config file
        dark_cyan_ln!("Reading project config...");

        let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);
        let source_code = fs::read_to_string(&config_path);
        match source_code {
            Ok(content) => CompileType::MultiFile(content),
            Err(_) => return Err(CompilerMessages {
                errors: vec![CompileError::file_error(&config_path, "Error reading config file")],
                warnings: Vec::new(),
            })
        }
    };

    // Parse configuration and collect modules
    match project_config_type {
        CompileType::SingleBeanstalkFile(code) => {
            beanstalk_modules_to_parse.push(InputModule {
                source_code: code,
                source_path: entry_path.to_owned(),
            });
        }

        CompileType::SingleMarkthroughFile(_code) => {
            // TODO: Handle Markthrough files in the future
            return Err(CompilerMessages {
                errors: vec![CompileError::compiler_error("Markthrough files not yet supported")],
                warnings: Vec::new()
            });
        }

        CompileType::MultiFile(config_source_code) => {
            let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);

            // Parse the config file
            let mut tokenizer_output = match tokenizer::tokenize(
                &config_source_code,
                &config_path,
                TokenizeMode::Normal,
            ) {
                Ok(tokens) => tokens,
                Err(e) => return Err(CompilerMessages {
                    errors: vec![e.with_file_path(config_path.clone())],
                    warnings: Vec::new(),
                }),
            };

            // Create the host function registry
            let host_registry = match create_builtin_registry() {
                Ok(registry) => registry,
                Err(e) => return Err(CompilerMessages {
                    errors: vec![e.with_file_path(config_path.clone())],
                    warnings: Vec::new(),
                }),
            };

            let ast_context = ScopeContext::new_with_registry(
                ContextKind::Module,
                config_path.to_owned(),
                &[],
                host_registry,
            );

            let config_public_vars = match new_ast(&mut tokenizer_output, ast_context, true) {
                Ok(module) => module.public,
                Err(e) => return Err(CompilerMessages {
                    errors: vec![e.with_file_path(config_path.clone())],
                    warnings: Vec::new(),
                }),
            };

            // Parse configuration from AST
            if let Err(e) = get_config_from_ast(config_public_vars, &mut project_config) {
                return Err(CompilerMessages {
                    errors: vec![e.with_file_path(config_path.clone())],
                    warnings: Vec::new(),
                });
            }

            let src_dir = entry_dir.join(&project_config.src);
            let _output_dir = match release_build {
                true => entry_dir.join(&project_config.release_folder),
                false => entry_dir.join(&project_config.dev_folder),
            };

            add_bst_files_to_parse(&mut beanstalk_modules_to_parse, &src_dir)?;
        }
    }

    // ----------------------------------
    // DETERMINE BUILD TARGET AND BUILDER
    // ----------------------------------
    // If no override, read it from the config
    let build_target =
        target_override.unwrap_or_else(|| determine_build_target(&project_config, entry_path));

    let project_builder = create_project_builder(build_target);

    print_bold!("Compiling with target: ");
    dark_yellow_ln!("{:?}", project_builder.target_type());

    // ----------------------------------
    // BUILD PROJECT USING APPROPRIATE BUILDER
    // ----------------------------------
    project_builder.build_project(
        beanstalk_modules_to_parse,
        &project_config,
        release_build,
        flags,
    )
}

// Look for every subdirectory inside the dir and add all .bst files to the source_code_to_parse
fn add_bst_files_to_parse(
    source_code_to_parse: &mut Vec<InputModule>,
    project_root_dir: &Path,
) -> Result<(), CompilerMessages> {
    // Can't just use the src_dir from config, because this might be recursively called for new subdirectories

    // Read all files in the src directory
    let all_dir_entries: fs::ReadDir = match fs::read_dir(project_root_dir) {
        Ok(dir) => dir,
        Err(e) => return Err(CompilerMessages {
            errors: vec![CompileError::file_error(
                &project_root_dir,
                &format!("Can't find any files to parse inside this directory. Might be empty? \nError: {:?}", e)
            )],
            warnings: Vec::new(),
        })
    };

    for file in all_dir_entries {
        match file {
            Ok(f) => {
                let file_path = f.path();

                // If it's a .bst file, add it to the list of files to compile
                if file_path.extension() == Some(BEANSTALK_FILE_EXTENSION.as_ref()) {
                    let code = match fs::read_to_string(&file_path) {
                        Ok(content) => content,
                        Err(e) => return Err(CompilerMessages {
                            errors: vec![CompileError::file_error(
                                &file_path,
                                &format!("Error reading file when adding new bst files to parse: {:?}", e)
                            )],
                            warnings: Vec::new(),
                        })
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
                            return Err(CompilerMessages {
                                errors: vec![CompileError::file_error(
                                    &file_path,
                                    "Error getting file stem"
                                )],
                                warnings: Vec::new(),
                            })
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
                return Err(CompilerMessages {
                    errors: vec![CompileError::file_error(
                        &project_root_dir,
                        &format!("Error reading file when adding new bst files to parse: {:?}", e)
                    )],
                    warnings: Vec::new(),
                })
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
