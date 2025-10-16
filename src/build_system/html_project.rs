// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.

use crate::build_system::build_system::{BuildTarget, ProjectBuilder};
use crate::compiler::compiler_errors::{CompileError, CompilerMessages};
use crate::runtime::io::js_bindings::JsBindingsGenerator;
use crate::settings::Config;
use crate::{Flag, InputModule, Project};

pub struct HtmlProjectBuilder {
    target: BuildTarget,
}

impl HtmlProjectBuilder {
    pub fn new(target: BuildTarget) -> Self {
        Self { target }
    }

    /// Generate JavaScript bindings for WASM module using the new comprehensive generator
    #[allow(dead_code)] // Will be used when HTML build system is fully integrated
    fn generate_js_bindings(&self, wasm_name: &str, release_build: bool) -> String {
        let generator = JsBindingsGenerator::new(wasm_name.to_string())
            .with_dom_functions(true)
            .with_dev_features(!release_build);

        generator.generate_js_bindings()
    }

    /// Generate HTML template with WASM integration
    #[allow(dead_code)] // Will be used when HTML build system is fully integrated
    fn generate_html_template(&self, title: &str, js_filename: &str) -> String {
        format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>{}</title>
    <style>
        body {{
            font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', Roboto, sans-serif;
            margin: 0;
            padding: 20px;
            background-color: #f5f5f5;
        }}
        #app {{
            max-width: 800px;
            margin: 0 auto;
            background: white;
            padding: 20px;
            border-radius: 8px;
            box-shadow: 0 2px 10px rgba(0,0,0,0.1);
        }}
        .loading {{
            text-align: center;
            color: #666;
            font-style: italic;
        }}
        .error {{
            color: #d32f2f;
            background: #ffebee;
            padding: 10px;
            border-radius: 4px;
            margin: 10px 0;
        }}
    </style>
</head>
<body>
    <div id="app">
        <div class="loading">Loading Beanstalk application...</div>
    </div>
    
    <script>
        // Inline the JS bindings for better performance and integration
        {}
    </script>
</body>
</html>"#,
            title, js_filename
        )
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
        })
    }

    fn target_type(&self) -> &BuildTarget {
        &self.target
    }

    fn validate_config(&self, config: &Config) -> Result<(), CompileError> {
        // Validate HTML-specific configuration
        if config.dev_folder.as_os_str().is_empty() {
            return Err(CompileError::compiler_error(
                "HTML projects require a dev_folder to be specified",
            ));
        }

        if config.release_folder.as_os_str().is_empty() {
            return Err(CompileError::compiler_error(
                "HTML projects require a release_folder to be specified",
            ));
        }

        // Check for web-specific features in config
        // TODO: Add validation for HTML-specific configuration options

        Ok(())
    }
}
