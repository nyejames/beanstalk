// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.

use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::runtime::io::js_bindings::JsBindingsGenerator;
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
        _modules: Vec<InputModule>,
        config: &Config,
        _release_build: bool,
        _flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        // Validate configuration
        if let Err(e) = self.validate_config(config) {
            return Err(CompilerMessages {
                errors: vec![e],
                warnings: Vec::new(),
            });
        }

        // TODO
        // An HTML project has a directory-as-namespace structure.
        // So each directory becomes a separate HTML page.
        // Any .bst files in that directory are combined into a single WASM module.

        // Each directory becomes a separate Wasm module and has a specified index page.
        // Any other files (JS / CSS / HTML) would be copied over and have to be referenced from the index page for use.

        let output_files = Vec::new();

        Ok(Project {
            config: config.clone(),
            output_files,
            warnings: Vec::new(),
        })
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, config: &Config) -> Result<(), CompileError> {
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
