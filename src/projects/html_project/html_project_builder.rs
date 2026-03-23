// HTML project builder orchestration.
//
// WHAT: coordinates module output-path resolution, homepage checks, and backend selection.
// WHY: project builders own artifact assembly policy while compiler backends stay generic.
use crate::build_system::build::{BackendBuilder, Module, OutputFile, Project};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::compiler_frontend::paths::path_resolution::resolve_project_entry_root;
use crate::projects::html_project::js_path::{compile_html_module_js, html_output_path};
use crate::projects::html_project::wasm::artifacts::{
    CompiledHtmlWasmModule, compile_html_module_wasm,
};
use crate::projects::routing::parse_html_site_config;
use crate::projects::settings::Config;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct HtmlProjectBuilder {}

impl HtmlProjectBuilder {
    /// Constructs the HTML project builder.
    ///
    /// WHAT: initializes a stateless builder implementation.
    /// WHY: builder policy is encoded in methods rather than runtime state.
    pub fn new() -> Self {
        Self {}
    }
}

impl BackendBuilder for HtmlProjectBuilder {
    fn build_backend(
        &self,
        modules: Vec<Module>,
        config: &Config,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        parse_html_site_config(config).map_err(CompilerMessages::from_error)?;

        if modules.is_empty() {
            return Err(CompilerMessages::from_error(CompilerError::compiler_error(
                "HTML builder expected at least one compiled module but got 0.",
            )));
        }

        let release_build = flags.contains(&Flag::Release);
        let wasm_enabled = flags.contains(&Flag::HtmlWasm);
        let is_directory_build = config.entry_dir.is_dir();
        let resolved_entry_root = resolve_canonical_entry_root(config, is_directory_build)?;

        let expected_homepage_entry = resolved_entry_root
            .as_ref()
            .map(|entry_root| entry_root.join("#page.bst"));

        let mut output_files = Vec::new();
        let mut output_paths = HashSet::new();
        let mut entry_page_rel = None;
        let mut has_directory_homepage = false;

        for module in modules {
            let logical_html_output_path =
                html_output_path(&module.entry_point, resolved_entry_root.as_deref())
                    .map_err(CompilerMessages::from_error)?;

            let compiled_artifacts = compile_one_module(
                &module,
                &logical_html_output_path,
                release_build,
                wasm_enabled,
            )?;

            for output_file in compiled_artifacts.output_files {
                let output_path = output_file.relative_output_path().to_path_buf();
                if !output_paths.insert(output_path.clone()) {
                    return Err(duplicate_output_path_error(
                        &module.entry_point,
                        &output_path,
                    ));
                }
                output_files.push(output_file);
            }

            if let Some(homepage_entry) = expected_homepage_entry.as_ref() {
                if module.entry_point == *homepage_entry {
                    has_directory_homepage = true;
                    entry_page_rel = Some(compiled_artifacts.html_output_path.clone());
                }
            } else if entry_page_rel.is_none() {
                entry_page_rel = Some(compiled_artifacts.html_output_path.clone());
            }
        }

        if is_directory_build && !has_directory_homepage {
            return Err(missing_homepage_error(
                config,
                resolved_entry_root.as_deref(),
            ));
        }

        Ok(Project {
            output_files,
            entry_page_rel,
            warnings: Vec::new(),
        })
    }

    fn validate_project_config(&self, config: &Config) -> Result<(), CompilerError> {
        // Validate HTML-specific configuration up front so build/dev runtime behavior stays
        // deterministic and all routing-policy mistakes are surfaced as config errors.
        parse_html_site_config(config)?;

        // Empty dev/release folders are allowed and resolved by core build output logic.
        Ok(())
    }
}

/// Resolve and canonicalize the entry root for directory builds.
///
/// Returns `None` for single-file builds; `Some(canonical_path)` for directory builds.
fn resolve_canonical_entry_root(
    config: &Config,
    is_directory_build: bool,
) -> Result<Option<PathBuf>, CompilerMessages> {
    if !is_directory_build {
        return Ok(None);
    }
    let entry_root_path = resolve_project_entry_root(config);
    let canonical = fs::canonicalize(&entry_root_path).map_err(|error| {
        CompilerMessages::from_error(CompilerError::file_error(
            &config.entry_dir,
            format!(
                "Failed to resolve configured HTML entry root '{}': {error}",
                entry_root_path.display()
            ),
        ))
    })?;
    Ok(Some(canonical))
}

/// Compile one module through the appropriate builder path (JS-only or HTML+Wasm).
fn compile_one_module(
    module: &Module,
    logical_html_output_path: &PathBuf,
    release_build: bool,
    wasm_enabled: bool,
) -> Result<CompiledHtmlModuleArtifacts, CompilerMessages> {
    if wasm_enabled {
        let compiled_wasm = compile_html_module_wasm(
            &module.hir,
            &module.borrow_analysis,
            &module.string_table,
            logical_html_output_path,
            release_build,
        )?;
        Ok(CompiledHtmlModuleArtifacts::from_wasm(compiled_wasm))
    } else {
        let output_file = compile_html_module_js(
            &module.hir,
            &module.borrow_analysis,
            &module.string_table,
            logical_html_output_path.clone(),
            release_build,
        )
        .map_err(CompilerMessages::from_error)?;
        Ok(CompiledHtmlModuleArtifacts::from_js(
            logical_html_output_path.clone(),
            output_file,
        ))
    }
}

fn duplicate_output_path_error(entry_point: &Path, output_path: &Path) -> CompilerMessages {
    let mut error = CompilerError::file_error(
        entry_point,
        format!(
            "HTML builder produced duplicate output path '{}'. Ensure each '#*.bst' entry maps to a unique page output.",
            output_path.display(),
        ),
    )
    .with_error_type(ErrorType::Config);
    error.metadata.insert(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        "Check your page routing configuration to ensure unique output paths".to_string(),
    );
    CompilerMessages::from_error(error)
}

fn missing_homepage_error(config: &Config, resolved_entry_root: Option<&Path>) -> CompilerMessages {
    let entry_root = resolved_entry_root.unwrap_or_else(|| Path::new("."));
    let mut error = CompilerError::file_error(
        &config.entry_dir,
        format!(
            "HTML project builds require a '#page.bst' homepage at the root of the configured entry root '{}'.",
            entry_root.display(),
        ),
    )
    .with_error_type(ErrorType::Config);
    error.metadata.insert(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        format!("Create a '#page.bst' file in '{}'", entry_root.display()),
    );
    CompilerMessages::from_error(error)
}

struct CompiledHtmlModuleArtifacts {
    /// Full emitted output set for one module (HTML only or HTML+Wasm trio).
    output_files: Vec<OutputFile>,
    /// HTML entry path used for homepage selection and serving/open behavior.
    html_output_path: PathBuf,
}

impl CompiledHtmlModuleArtifacts {
    /// Wraps JS-only output into the builder's common artifact shape.
    fn from_js(html_output_path: PathBuf, output_file: OutputFile) -> Self {
        Self {
            output_files: vec![output_file],
            html_output_path,
        }
    }

    /// Wraps Wasm-mode output into the builder's common artifact shape.
    fn from_wasm(compiled_wasm: CompiledHtmlWasmModule) -> Self {
        // Keep the debug struct alive through compilation so toggles can expose it without
        // changing external interfaces.
        let _debug = compiled_wasm.debug;
        Self {
            output_files: compiled_wasm.output_files,
            html_output_path: compiled_wasm.html_output_path,
        }
    }
}

#[cfg(test)]
#[path = "html_project_builder_tests.rs"]
mod tests;
