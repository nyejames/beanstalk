use crate::compiler::compiler_errors::{CompileError, ErrorType};
use crate::compiler::module_dependencies::resolve_module_dependencies;
use crate::compiler::parsers::build_ast::{AstBlock, ContextKind, ScopeContext, new_ast};
use crate::compiler::parsers::tokenizer;
use crate::compiler::parsers::tokens::{TextLocation, TokenContext};
use crate::settings::{Config, get_config_from_ast};
use crate::{Compiler, Flag, return_file_errors, settings};
use colour::{dark_cyan_ln, dark_yellow_ln, green_ln, print_bold, print_ln_bold};
use rayon::iter::Either;
use rayon::prelude::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

pub struct InputModule {
    pub source_code: String,
    pub source_path: PathBuf,
}

pub fn build_project(
    entry_path: &Path,
    release_build: bool,
    flags: &[Flag],
) -> Result<Config, Vec<CompileError>> {
    // Create a new PathBuf from the entry_path
    let entry_dir = match std::env::current_dir() {
        Ok(dir) => dir.join(entry_path),
        Err(e) => return_file_errors!(entry_path, "Error getting current directory: {:?}", e),
    };

    // Read content from a test file
    print_ln_bold!("Project Directory: ");
    dark_yellow_ln!("{:?}", &entry_dir);

    let mut modules_to_parse: Vec<InputModule> = Vec::new();
    let mut project_config = Config::default();

    // check to see if there is a config.bs file in this directory
    // if there is, read it and set the config settings
    // and check where the project entry points are
    enum CompileType {
        SingleFile(String), // Source Code
        MultiFile(String),  // Config Source Code
    }

    // Single BS file
    let config = if entry_dir.extension() == Some("bs".as_ref()) {
        let source_code = fs::read_to_string(&entry_dir);
        match source_code {
            Ok(content) => CompileType::SingleFile(content),
            Err(e) => return_file_errors!(entry_dir, "Error reading file: {:?}", e),
        }

    // Full project with a config file
    } else {
        dark_cyan_ln!("Reading project config...");

        let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);
        let source_code = fs::read_to_string(&config_path);
        match source_code {
            Ok(content) => CompileType::MultiFile(content),
            Err(_) => return_file_errors!(config_path, "No config file found in project directory"),
        }
    };

    // TODO: project global imports
    // (config file imports that are available to the entire project without the need for importing explicitly)
    let mut _global_imports: Vec<String> = Vec::new();

    match config {
        CompileType::SingleFile(code) => {
            modules_to_parse.push(InputModule {
                source_code: code,
                source_path: entry_path.to_owned(),
            });
        }

        CompileType::MultiFile(config_source_code) => {
            let config_path = entry_dir.join(settings::CONFIG_FILE_NAME);

            // Parse the config file
            let mut tokenizer_output = match tokenizer::tokenize(&config_source_code, &config_path)
            {
                Ok(tokens) => tokens,
                Err(e) => {
                    return Err(vec![e.with_file_path(config_path)]);
                }
            };

            // let config_args = Vec::from([
            //     "project",
            //     "src",
            //     "dev_folder",
            //     "release_folder",
            //     "name",
            //     "version",
            //     "author",
            //     "license",
            //     "html_meta",
            // ]);

            let ast_context =
                ScopeContext::new(ContextKind::Config, config_path.to_owned(), Vec::new());

            let config_module = match new_ast(&mut tokenizer_output, ast_context, true) {
                Ok(expr) => expr,
                Err(e) => return Err(vec![e.with_file_path(config_path)]),
            };

            // If reading the config threw and error, get out of here.
            if let Err(e) = get_config_from_ast(config_module, &mut project_config) {
                return Err(vec![e.with_file_path(config_path)]);
            }

            let src_dir = entry_dir.join(&project_config.src);
            let output_dir = match release_build {
                true => entry_dir.join(&project_config.release_folder),
                false => entry_dir.join(&project_config.dev_folder),
            };

            add_bs_files_to_parse(&mut modules_to_parse, &output_dir, &src_dir)?;
        }
    }

    // ----------------------------------
    // BUILD REST OF PROJECT AFTER CONFIG
    // ----------------------------------
    print_bold!("\nCompiling: ");
    dark_yellow_ln!("{:?}", project_config.src);
    let compiler = Compiler::new(&project_config, flags);

    // Compile each module to tokens and collect them all
    let time = Instant::now();
    let project_tokens: Vec<Result<TokenContext, CompileError>> = modules_to_parse
        .par_iter()
        .map(|module| compiler.source_to_tokens(&module.source_code, &module.source_path))
        .collect();

    // Return any compilation errors and sort modules into dependency order
    // Once the compiler has created a dependency graph,
    // each AST creation can also export it's public variables for type checking,
    // and successive ast blocks can type check properly.
    // Circular dependencies are disallowed
    let sorted_modules = resolve_module_dependencies(project_tokens)?;

    // AST generation
    let (_project_asts, errors): (Vec<AstBlock>, Vec<CompileError>) = sorted_modules
        .into_par_iter()
        .map(|module_tokens| compiler.tokens_to_ast(module_tokens, &[]))
        .partition_map(|result| match result {
            Ok(ast) => Either::Left(ast),
            Err(e) => Either::Right(e),
        });

    // Return any errors that have been found so far
    if !errors.is_empty() {
        return Err(errors);
    }

    if !flags.contains(&Flag::DisableTimers) {
        print!("AST created in: ");
        green_ln!("{:?}", time.elapsed());
    }

    // Now we can combine the ASTs into one monolithic Wasm file.

    Ok(project_config)
}

// Look for every subdirectory inside the dir and add all .bs files to the source_code_to_parse
pub fn add_bs_files_to_parse(
    source_code_to_parse: &mut Vec<InputModule>,
    output_dir: &Path,
    project_root_dir: &Path,
) -> Result<(), Vec<CompileError>> {
    // Can't just use the src_dir from config, because this might be recursively called for new subdirectories

    // Read all files in the src directory
    let all_dir_entries: fs::ReadDir = match fs::read_dir(project_root_dir) {
        Ok(dir) => dir,
        Err(e) => return_file_errors!(
            project_root_dir,
            "Error reading directory when adding new bs files to parse: {:?}",
            e
        ),
    };

    for file in all_dir_entries {
        match file {
            Ok(f) => {
                let file_path = f.path();

                // If it's a .bs file, add it to the list of files to compile
                if file_path.extension() == Some("bs".as_ref()) {
                    let code = match fs::read_to_string(&file_path) {
                        Ok(content) => content,
                        Err(e) => return_file_errors!(
                            file_path,
                            "Error reading file when adding new bs files to parse: {:?}",
                            e
                        ),
                    };

                    // If code is empty, skip compiling this module
                    if code.is_empty() {
                        continue;
                    }

                    let mut global = false;

                    let file_name = match file_path.file_stem().unwrap().to_str() {
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
                            return_file_errors!(file_path, "Error getting file stem")
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
                    // Add the new directory folder to the output directory
                    let new_output_dir = output_dir.join(file_path.file_stem().unwrap());

                    // Recursively call add_bs_files_to_parse on the new directory
                    add_bs_files_to_parse(source_code_to_parse, &new_output_dir, &file_path)?;

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
                return_file_errors!(
                    project_root_dir,
                    "Error reading file when adding new bs files to parse: {:?}",
                    e
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
