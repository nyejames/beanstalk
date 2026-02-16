// HTML/JS project builder
//
// Builds Beanstalk projects for web deployment, generating separate WASM files
// for different HTML pages and including JavaScript bindings for DOM interaction.
use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::build_system::build::{FileKind, Module, OutputFile, Project, ProjectBuilder};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::Config;
use std::path::PathBuf;

#[derive(Debug)]
pub struct HtmlProjectBuilder {}

impl HtmlProjectBuilder {
    pub fn new() -> Self {
        Self {}
    }
}

impl ProjectBuilder for HtmlProjectBuilder {
    fn build_backend(
        &self,
        modules: Vec<Module>,
        config: &Config,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        let mut compiler_messages = CompilerMessages::new();
        // Validate the config has everything needed for an HTML project
        if let Err(e) = self.validate_project_config(&config) {
            return Err(CompilerMessages {
                errors: vec![e],
                warnings: Vec::new(),
            });
        }

        let mut output_files = Vec::with_capacity(1);
        for module in modules {
            // -----------------------------
            //      BACKEND COMPILATION
            // -----------------------------
            match compile_js_module(
                &module.hir,
                &module.string_table,
                &mut output_files,
                flags.contains(&Flag::Release),
            ) {
                Ok(()) => {}
                Err(e) => {
                    compiler_messages.errors.push(e);
                    return Err(compiler_messages);
                }
            }
        }

        Ok(Project {
            output_files,
            warnings: compiler_messages.warnings,
        })
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        // Validate HTML-specific configuration

        // This used to just check that there was a dev / release folder set,
        // now we don't care
        // as not having it set means it just goes into the same directory as the entry path.

        Ok(())
    }
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

#[allow(dead_code)]
#[derive(Clone)]
pub struct HTMLMeta {
    pub site_title: String,
    pub page_description: String,
    pub site_url: String,
    pub page_url: String,
    pub page_og_title: String,
    pub page_og_description: String,
    pub page_image_url: String,
    pub page_image_alt: String,
    pub page_locale: String,
    pub page_type: String,
    pub page_twitter_large_image: String,
    pub page_canonical_url: String,
    pub page_root_url: String,
    pub image_folder_url: String,
    pub favicons_folder_url: String,
    pub theme_color_light: String,
    pub theme_color_dark: String,
    pub auto_site_title: bool,
    pub release_build: bool,
}

impl Default for HTMLMeta {
    fn default() -> Self {
        HTMLMeta {
            site_title: String::from("Website Title"),
            page_description: String::from("Website Description"),
            site_url: String::from("localhost:6969"),
            page_url: String::from(""),
            page_og_title: String::from(""),
            page_og_description: String::from(""),
            page_image_url: String::from(""),
            page_image_alt: String::from(""),
            page_locale: String::from("en_US"),
            page_type: String::from("website"),
            page_twitter_large_image: String::from(""),
            page_canonical_url: String::from(""),
            page_root_url: String::from("./"),
            image_folder_url: String::from("images"),
            favicons_folder_url: String::from("images/favicons"),
            theme_color_light: String::from("#fafafa"),
            theme_color_dark: String::from("#101010"),
            auto_site_title: true,
            release_build: false,
        }
    }
}
