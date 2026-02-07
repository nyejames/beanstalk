// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.

use crate::build::{BuildTarget, FileKind, OutputFile, ProjectBuilder};
use crate::build_system::core_build;
use crate::build_system::core_build::{CoreCompilationResult, extract_source_code};
use crate::compiler::codegen::js::JsLoweringConfig;
use crate::compiler::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::hir::nodes::HirModule;
use crate::compiler::string_interning::StringTable;
use crate::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::{Flag, InputFile, Project, lower_hir_to_js, return_config_error, settings};
use colour::{dark_cyan_ln, e_red_ln};
use std::cmp::PartialEq;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub struct HtmlProjectBuilder {
    flags: Vec<Flag>,
}

pub struct JsHostBinding {
    pub js_path: String, // "console.log" or "Beanstalk.io"
}

struct Module {
    files: Vec<InputFile>,
    name: String,
}

impl HtmlProjectBuilder {
    pub fn new(flags: Vec<Flag>) -> Self {
        Self { flags }
    }
}

impl ProjectBuilder for HtmlProjectBuilder {
    fn build_project(
        &self,
        path: PathBuf,
        release_build: bool,
    ) -> Result<Project, CompilerMessages> {
        // Create a new project config.
        // In the future, if a different backend is possible (e.g. Wasm),
        // then flags will be used to choose something other than the default backend.
        // Currently, it is always the JS codegen backend
        let config = Config::new(path, BuildTarget::HtmlJSProject);

        // Validate the config has everything needed for an HTML project
        if let Err(e) = self.validate_project_config(&config) {
            return Err(CompilerMessages {
                errors: vec![e],
                warnings: Vec::new(),
            });
        }

        let mut project_modules: Vec<Module> = Vec::with_capacity(1);
        let mut compiler_messages = CompilerMessages::new();

        // -----------------------------
        //    SINGLE FILE COMPILATION
        // -----------------------------
        // If the entry is a file (not a directory),
        // Just compile and output that single file
        if let Some(extension) = config.entry_dir.extension() {
            match extension.to_str().unwrap() {
                BEANSTALK_FILE_EXTENSION => {
                    // Single BST file
                    let code = match extract_source_code(&config.entry_dir) {
                        Ok(code) => code,
                        Err(e) => {
                            compiler_messages.errors.push(e);
                            return Err(compiler_messages);
                        }
                    };

                    let input_file = InputFile {
                        source_code: code,
                        source_path: config.entry_dir.clone(),
                    };

                    project_modules.push(Module {
                        files: vec![input_file],

                        // TODO: probably will never happen, but should get rid of the unwrap here
                        name: config
                            .entry_dir
                            .file_name()
                            .unwrap()
                            .to_string_lossy()
                            .to_string(),
                    })
                }
                _ => {
                    compiler_messages.errors.push(CompilerError::file_error(
                        &config.entry_dir,
                        format!("Unsupported file extension for compilation. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}"),
                    ));
                    return Err(compiler_messages);
                }
            }
        } else {
            // -----------------------------
            //         WHOLE PROJECT
            // -----------------------------
            // So here the builder should check the name of the file to check for its special properties.
            // Top Level templates outside #page have to be explicitly imported from other files if they have some to use.

            // Guard clause to make sure the entry is a directory
            // Could be a file without an extension, which would be weird
            if !config.entry_dir.is_dir() {
                compiler_messages.errors.push(CompilerError::file_error(
                    &config.entry_dir,
                    "Found a file without an extension set. Beanstalk files use .{BEANSTALK_FILE_EXTENSION}",
                ));

                return Err(compiler_messages);
            }

            let modules = match create_all_modules_in_project(&config) {
                Ok(modules) => modules,
                Err(e) => {
                    compiler_messages
                        .errors
                        .push(CompilerError::file_error(&config.entry_dir, e));
                    return Err(compiler_messages);
                }
            };

            project_modules.extend(modules);
        }

        let mut output_files = Vec::with_capacity(1);
        for module in project_modules {
            // -----------------------------
            //     FRONTEND COMPILATION
            // -----------------------------
            // Use the core build pipeline to compile to HIR
            let compilation_result = core_build::compile_modules(module.files, &config)?;

            compiler_messages
                .warnings
                .extend(compilation_result.warnings);

            // -----------------------------
            //      BACKEND COMPILATION
            // -----------------------------
            // The backend will create the OutputFiles based on the BuildTarget
            match config.build_target {
                BuildTarget::HtmlJSProject => {
                    match compile_js_module(
                        &compilation_result.hir_module,
                        &compilation_result.string_table,
                        &mut output_files,
                        release_build,
                    ) {
                        Ok(()) => {}
                        Err(e) => {
                            compiler_messages.errors.push(e);
                            return Err(compiler_messages);
                        }
                    }
                }

                // TODO: Wasm backend will eventually go here

                // Unsupported backend
                _ => {
                    compiler_messages.errors.push(CompilerError::compiler_error(
                        "Backend not supported for HTML projects",
                    ));

                    return Err(compiler_messages);
                }
            }
        }

        Ok(Project {
            config: config.clone(),
            output_files,
            warnings: compiler_messages.warnings,
        })
    }

    fn validate_project_config(&self, config: &Config) -> Result<(), CompilerError> {
        // Validate HTML-specific configuration

        // This used to just check that there was a dev / release folder set,
        // now we don't care
        // as not having it set means it just goes into the same directory as the entry path.

        Ok(())
    }
}

fn create_all_modules_in_project(config: &Config) -> Result<Vec<Module>, String> {
    // ----------------------------
    //     LOOK FOR CONFIG FILE
    // ----------------------------
    let config_path = config.entry_dir.join(settings::CONFIG_FILE_NAME);

    match fs::exists(&config_path) {
        Ok(true) => {
            let source_code = fs::read_to_string(&config_path);
            let config_code = match source_code {
                Ok(content) => content,
                Err(e) => {
                    return Err(e.to_string());
                }
            };

            // TODO: Mutate the current config with any additional user-specified config settings in the file
            // Add things like all library paths specified by the config to the list of modules to compile
            // Then the dependency resolution stage can deal with tree shaking and things like that
        }
        Err(e) => {
            return Err(e.to_string());
        }

        Ok(false) => {}
    };

    let mut modules = Vec::with_capacity(1);

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

fn compile_js_module(
    hir_module: &HirModule,
    string_table: &StringTable,
    output_files: &mut Vec<OutputFile>,
    release_build: bool,
) -> Result<(), CompilerError> {
    // The project builder determines where the output files need to go
    // by provided the full path from source for each file and its content
    let js_lowering_config = JsLoweringConfig {
        pretty: !release_build,
        emit_locations: !release_build,
    };

    let js_module = lower_hir_to_js(hir_module, string_table, js_lowering_config)?;

    output_files.push(OutputFile::new(
        PathBuf::from("test".to_string()),
        FileKind::Js(js_module.source),
    ));

    Ok(())
}
