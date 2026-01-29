// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.

use crate::build::{BuildTarget, OutputFile, ProjectBuilder};
use crate::build_system::core_build;
use crate::compiler::codegen::js::lower_hir_to_js;
use crate::compiler::compiler_errors::{CompilerError, CompilerMessages};
use crate::settings::Config;
use crate::{Flag, InputModule, Project, return_config_error};

pub struct HtmlProjectBuilder {
    target: BuildTarget,
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

        let js_module = match lower_hir_to_js(
            &compilation_result.hir_module,
            &compilation_result.string_table,
        ) {
            Ok(module) => module,
            Err(error) => {
                compiler_messages.errors.push(error);
                return Err(compiler_messages);
            }
        };

        // TODO
        // An HTML project has a directory-as-namespace structure.
        // So each directory becomes a separate HTML page.
        // Any .bst files in that directory are combined into a single WASM module.

        // Each directory becomes a separate Wasm module and has a specified index page.
        // Any other files (JS / CSS / HTML) would be copied over and have to be referenced from the index page for use.

        let output_files = vec![OutputFile::Js(js_module.source)];

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
