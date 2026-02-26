// Core build functionality shared across all project types
//
// Contains the common compilation pipeline steps that are used by all project builders
// This now only compiles the HIR and runs the borrow checker.
// This is because both a Wasm and JS backend must be supported, so it is agnostic about what happens after that.

use crate::build_system::build::{InputFile, Module};
use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenizeMode};
use crate::compiler_frontend::{CompilerFrontend, Flag};
use crate::projects::settings;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::{borrow_log, return_err_as_messages, return_file_error, timer_log};
use std::ffi::OsStr;
use std::fs;
use std::path::Path;
use std::time::Instant;

/// External function import required by the compiled WASM
#[derive(Debug, Clone)]
pub struct ExternalImport {
    /// Module name (e.g., "env", "beanstalk_io", "host")
    pub module: String,
    /// Function name
    pub function: String,
    /// Function signature for validation
    pub signature: FunctionSignature,
    /// Whether this is a built-in compiler_frontend function or user-defined import
    pub import_type: ImportType,
}

/// Function signature for external imports
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types
    pub params: Vec<WasmType>,
    /// Return types
    pub returns: Vec<WasmType>,
}

/// Type of external import
#[derive(Debug, Clone)]
pub enum ImportType {
    /// Built-in compiler_frontend library function (IO, memory management, etc.)
    BuiltIn(BuiltInFunction),
    /// User-defined external function from the host environment
    External,
}

/// Built-in compiler_frontend functions that the runtime must provide
#[derive(Debug, Clone)]
pub enum BuiltInFunction {
    /// IO operations
    Print,
    ReadInput,
    WriteFile,
    ReadFile,
    /// Memory management
    Malloc,
    Free,
    /// Environment access
    GetEnv,
    SetEnv,
    /// System operations
    Exit,
}

/// WASM value types for function signatures
#[derive(Debug, Clone)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

/// Find and compile all modules in the project.
/// This function is agnostic for all projects,
/// every builder will use it. It defines the structure of all Beanstalk projects
pub fn compile_project_frontend(
    config: &mut Config,
    flags: &[Flag],
) -> Result<Vec<Module>, CompilerMessages> {
    // For collecting warnings on the error path
    let messages = CompilerMessages::new();

    // -----------------------------
    //    SINGLE FILE COMPILATION
    // -----------------------------
    // If the entry is a file (not a directory),
    // Just compile and output that single file
    // Will just use the default config
    let result = if let Some(extension) = config.entry_dir.extension() {
        match extension.to_str().unwrap() {
            BEANSTALK_FILE_EXTENSION => {
                // Single BST file
                let code = match extract_source_code(&config.entry_dir) {
                    Ok(code) => code,
                    Err(e) => {
                        return_err_as_messages!(e);
                    }
                };

                let input_file = InputFile {
                    source_code: code,
                    source_path: config.entry_dir.clone(),
                };

                let module = compile_module(vec![input_file], config)?;

                vec![module]
            }
            _ => {
                let err = CompilerError::file_error(
                    &config.entry_dir,
                    format!(
                        "Unsupported file extension for compilation. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
                    ),
                );

                return_err_as_messages!(err);
            }
        }
    } else {
        // Guard clause to make sure the entry is a directory
        // Could be a file without an extension, which would be weird
        if !config.entry_dir.is_dir() {
            let err = CompilerError::file_error(
                &config.entry_dir,
                format!(
                    "Found a file without an extension set. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"
                ),
            );

            return_err_as_messages!(err);
        }

        // --------------------
        //   PARSE THE CONFIG
        // --------------------
        let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);
        match fs::exists(&config_path) {
            Ok(true) => {
                let source_code = fs::read_to_string(&config_path);
                let config_code = match source_code {
                    Ok(content) => content,
                    Err(e) => {
                        let err = CompilerError::file_error(&config_path, e.to_string());
                        return_err_as_messages!(err);
                    }
                };

                // TODO: Mutate the current config with any additional user-specified config settings in the file
                // Add things like all library paths specified by the config to the list of modules to compile
                // Then the dependency resolution stage can deal with tree shaking and things like that.
                // Parser for config file is not sorted out yet, but it should be based on top level constants
                todo!();
            }
            Err(e) => {
                let err = CompilerError::file_error(&config_path, e.to_string());
                return_err_as_messages!(err);
            }

            // No config
            // TODO: Decide whether all projects MUST have a config and error OR they just have default settings
            Ok(false) => {}
        };

        // -------------------------------------
        //  DISCOVER ALL MODULES IN THE PROJECT
        // -------------------------------------
        // Modules are folders that contain a '#' file
        // This is any file that starts with a '#' and becomes an entry point for the module
        // The #config file is a special '#' file that should only live in the entry path
        let modules = match discover_all_modules_in_project(config) {
            Ok(modules) => modules,
            Err(e) => {
                let err = CompilerError::file_error(&config.entry_dir, e);
                return_err_as_messages!(err);
            }
        };

        // TODO: discover and then compile modules in two separate stages
        modules
    };

    Ok(result)
}

/// Perform the core compilation pipeline shared by all project types
pub fn compile_module(module: Vec<InputFile>, config: &Config) -> Result<Module, CompilerMessages> {
    // Module capacity heuristic
    // Just a guess of how many strings we might need to intern per file
    const FILE_MIN_UNIQUE_SYMBOLS_CAPACITY: usize = 16;

    // Create a new string table for interning strings
    let string_table = StringTable::with_capacity(module.len() * FILE_MIN_UNIQUE_SYMBOLS_CAPACITY);

    // Create the compiler_frontend instance
    let mut compiler = CompilerFrontend::new(config, string_table);

    let time = Instant::now();

    // ----------------------------------
    //         Token generation
    // ----------------------------------
    let tokenizer_result: Vec<Result<FileTokens, CompilerError>> = module
        .iter()
        .map(|module| {
            compiler.source_to_tokens(
                &module.source_code,
                &module.source_path,
                TokenizeMode::Normal,
            )
        })
        .collect();

    // Check for any errors first
    let mut project_tokens = Vec::with_capacity(tokenizer_result.len());
    let mut compiler_messages = CompilerMessages::new();
    for file in tokenizer_result {
        match file {
            Ok(tokens) => {
                project_tokens.push(tokens);
            }
            Err(e) => {
                compiler_messages.errors.push(e);
            }
        }
    }

    if !compiler_messages.errors.is_empty() {
        return Err(compiler_messages);
    }

    timer_log!(time, "Tokenized in: ");

    // ----------------------------------
    //           Parse Headers
    // ----------------------------------
    // This will parse all the top level declarations across the token_stream
    // This is to split up the AST generation into discreet blocks and make all the public declarations known during AST generation.
    // All imports are figured out at this stage, so each header can be ordered depending on their dependencies.
    let time = Instant::now();

    let module_headers =
        match compiler.tokens_to_headers(project_tokens, &mut compiler_messages.warnings) {
            Ok(headers) => headers,
            Err(e) => {
                compiler_messages.errors.extend(e);
                return Err(compiler_messages);
            }
        };

    timer_log!(time, "Headers Parsed in: ");

    // ----------------------------------
    //       Dependency resolution
    // ----------------------------------
    let time = Instant::now();
    let sorted_modules = match compiler.sort_headers(module_headers.headers) {
        Ok(modules) => modules,
        Err(error) => {
            compiler_messages.errors.extend(error);
            return Err(compiler_messages);
        }
    };

    timer_log!(time, "Dependency graph created in: ");

    // ----------------------------------
    //          AST generation
    // ----------------------------------
    let time = Instant::now();
    //let mut exported_declarations: Vec<Arg> = Vec::with_capacity(EXPORTS_CAPACITY);
    let mut module_ast = Ast {
        nodes: Vec::with_capacity(sorted_modules.len()),
        entry_path: InternedPath::from_path_buf(
            &compiler.project_config.entry_dir,
            &mut compiler.string_table,
        ),
        external_exports: Vec::new(),
        start_template_items: Vec::new(),
        warnings: Vec::new(),
    };

    // Combine all the headers into one AST
    match compiler.headers_to_ast(sorted_modules, module_headers.top_level_template_items) {
        Ok(parser_output) => {
            module_ast.nodes.extend(parser_output.nodes);
            module_ast
                .external_exports
                .extend(parser_output.external_exports);
            module_ast
                .start_template_items
                .extend(parser_output.start_template_items);
            // Extends the compiler_frontend messages with warnings and errors from the parser
            compiler_messages.warnings.extend(parser_output.warnings);
        }
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            return Err(compiler_messages);
        }
    }

    timer_log!(time, "AST created in: ");

    // ----------------------------------
    //          HIR generation
    // ----------------------------------
    let time = Instant::now();

    let hir_module = match compiler.generate_hir(module_ast) {
        Ok(nodes) => nodes,
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            compiler_messages.warnings.extend(e.warnings);
            return Err(compiler_messages);
        }
    };

    timer_log!(time, "HIR generated in: ");

    // ----------------------------------
    //          BORROW CHECKING
    // ----------------------------------
    let time = Instant::now();

    let borrow_analysis = match compiler.check_borrows(&hir_module) {
        Ok(outcome) => outcome,
        Err(e) => {
            compiler_messages.errors.extend(e.errors);
            compiler_messages.warnings.extend(e.warnings);
            return Err(compiler_messages);
        }
    };

    timer_log!(time, "Borrow checking completed in: ");

    // Debug output for the borrow checker (macro-gated by `show_borrow_checker`)
    borrow_log!("=== BORROW CHECKER OUTPUT ===");
    borrow_log!(format!(
        "Borrow checking completed successfully (states={} functions={} blocks={} conflicts_checked={} stmt_facts={} term_facts={} value_facts={})",
        borrow_analysis.analysis.total_state_snapshots(),
        borrow_analysis.stats.functions_analyzed,
        borrow_analysis.stats.blocks_analyzed,
        borrow_analysis.stats.conflicts_checked,
        borrow_analysis.analysis.statement_facts.len(),
        borrow_analysis.analysis.terminator_facts.len(),
        borrow_analysis.analysis.value_facts.len()
    ));
    borrow_log!("=== END BORROW CHECKER OUTPUT ===");

    Ok(Module {
        folder_name: config
            .entry_dir
            .file_name()
            .unwrap_or(OsStr::new(""))
            .to_str()
            .unwrap_or("")
            .to_string(),
        entry_point: config.entry_dir.clone(), // The name of the main start function
        hir: hir_module,
        borrow_analysis,
        required_module_imports: Vec::new(), //TODO: parse imports for external modules and add to requirements list
        exported_functions: Vec::new(), //TODO: Get the list of exported functions from the AST (with their signatures)
        warnings: compiler_messages.warnings,
        string_table: compiler.string_table,
    })
}

pub(crate) fn discover_all_modules_in_project(config: &Config) -> Result<Vec<Module>, String> {
    let modules = Vec::with_capacity(1);

    // TODO:
    // HTML project builder uses directory based routing for the HTML pages.
    // Each page has a special name "#page" that can import any resources
    // and acts as the index page served from the path to its directory.
    // So "/info/specific_page" is a directory,
    // inside specific_page a #page can be added to serve this as a route.
    // Directories that don't have a #page are not served as routes.
    // Although currently this is a basic static site builder,
    // so this is more framework level stuff for the future.

    Ok(modules)
}

/// Recursively adds Beanstalk files to the list of input modules.
/// It scans through all subdirectories of the provided dir and adds them to the list
pub fn add_all_bst_files_from_dir(
    beanstalk_modules: &mut Vec<InputFile>,
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
                    let code = extract_source_code(&file_path)?;

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

                    let final_output_file = InputFile {
                        source_code: code,
                        source_path: file_path,
                    };

                    if global {
                        beanstalk_modules.insert(0, final_output_file);
                    } else {
                        beanstalk_modules.push(final_output_file);
                    }

                // If directory, recursively call add_bs_files_to_parse
                } else if file_path.is_dir() {
                    // Recursively call add_bst_files_to_parse on the new directory
                    add_all_bst_files_from_dir(beanstalk_modules, &file_path)?;

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

pub fn extract_source_code(file_path: &Path) -> Result<String, CompilerError> {
    match fs::read_to_string(file_path) {
        Ok(content) => Ok(content),
        Err(e) => {
            let suggestion: &'static str = if e.kind() == std::io::ErrorKind::NotFound {
                "Check that the file exists at the specified path"
            } else if e.kind() == std::io::ErrorKind::PermissionDenied {
                "Check that you have permission to read this file"
            } else {
                "Verify the file is accessible and not corrupted"
            };

            return_file_error!(
                &file_path,
                format!("Error reading file when adding new bst files to parse: {:?}", e), {
                    CompilationStage => "File System",
                    PrimarySuggestion => suggestion,
                }
            )
        }
    }
}
