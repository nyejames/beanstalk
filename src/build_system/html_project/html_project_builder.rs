// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.

use crate::build::{BuildTarget, FileKind, OutputFile, ProjectBuilder};
use crate::build_system::core_build;
use crate::compiler::codegen::js::JsLoweringConfig;
use crate::compiler::compiler_errors::{CompilerError, CompilerMessages};
use crate::settings::Config;
use crate::{Flag, InputModule, Project, lower_hir_to_js, return_config_error};
use std::path::PathBuf;

pub struct HtmlProjectBuilder {
    target: BuildTarget,
}

pub struct JsHostBinding {
    pub js_path: String, // "console.log" or "Beanstalk.io"
}

impl HtmlProjectBuilder {
    pub fn new(target: BuildTarget) -> Self {
        Self { target }
    }
}

impl ProjectBuilder for HtmlProjectBuilder {
    fn build_project(
        &self,
        modules: Vec<InputModule>,
        config: &Config,
        release_build: bool,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        // Validate configuration
        if let Err(e) = self.validate_config(config) {
            return Err(CompilerMessages {
                errors: vec![e],
                warnings: Vec::new(),
            });
        }

        // Use the core build pipeline to compile to HIR
        let compilation_result =
            core_build::compile_modules(modules, config, release_build, flags)?;

        let mut compiler_messages = CompilerMessages {
            errors: Vec::new(),
            warnings: compilation_result.warnings,
        };

        let js_lowering_config = JsLoweringConfig {
            pretty: !release_build,
            emit_locations: !release_build,
        };

        let js_module = match lower_hir_to_js(
            &compilation_result.hir_module,
            &compilation_result.string_table,
            js_lowering_config,
        ) {
            Ok(js_module) => js_module,
            Err(e) => {
                compiler_messages.errors.push(e);
                return Err(compiler_messages);
            }
        };

        // The project builder determines where the output files need to go
        // by provided the full path from source for each file and its content

        // TODO
        // Create the full structure of the builder output
        // Currently just outputs a single HTML file for testing
        let output_files = vec![OutputFile::new(
            PathBuf::from("test".to_string()),
            FileKind::Js(js_module.source),
        )];

        Ok(Project {
            config: config.clone(),
            output_files,
            warnings: compiler_messages.warnings,
        })
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, config: &Config) -> Result<(), CompilerError> {
        // Validate HTML-specific configuration
        if config.dev_folder.as_os_str().is_empty() {
            return_config_error!(
                "HTML projects require a dev_folder to be specified",
                crate::compiler::compiler_errors::ErrorLocation::default(),
                {
                    CompilationStage => "Configuration",
                    PrimarySuggestion => "Add 'dev_folder' field to your project configuration",
                    SuggestedInsertion => "dev_folder = \"dev\"",
                }
            );
        }

        if config.release_folder.as_os_str().is_empty() {
            return_config_error!(
                "HTML projects require a release_folder to be specified",
                crate::compiler::compiler_errors::ErrorLocation::default(),
                {
                    CompilationStage => "Configuration",
                    PrimarySuggestion => "Add 'release_folder' field to your project configuration",
                    SuggestedInsertion => "release_folder = \"release\"",
                }
            );
        }

        // Check for web-specific features in config
        // TODO: Add validation for HTML-specific configuration options

        Ok(())
    }
}
