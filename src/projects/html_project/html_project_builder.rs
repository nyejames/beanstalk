// HTML project builder orchestration.
//
// WHAT: coordinates module output-path resolution, homepage checks, and backend selection.
// WHY: project builders own artifact assembly policy while compiler backends stay generic.
use crate::backends::external_package_validation::{
    BackendTarget, validate_hir_external_package_support,
};
use crate::build_system::build::{BackendBuilder, CleanupPolicy, Module, OutputFile, Project};
use crate::build_system::utils::file_error_messages;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::LibrarySet;
use crate::projects::html_project::compile_input::HtmlModuleCompileInput;
use crate::projects::html_project::document_config::parse_html_document_config;
use crate::projects::html_project::js_path::{compile_html_module_js, html_output_path};
use crate::projects::html_project::path_policy::HtmlEntryPathPlan;
use crate::projects::html_project::style_directives::html_project_style_directives;
use crate::projects::html_project::tracked_assets::{
    emit_tracked_assets, plan_module_tracked_assets,
};
use crate::projects::html_project::wasm::artifacts::{
    CompiledHtmlWasmModule, compile_html_module_wasm,
};
use crate::projects::routing::parse_html_site_config;
use crate::projects::settings::Config;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub struct HtmlProjectBuilder {
    include_test_packages: bool,
}

impl HtmlProjectBuilder {
    /// Constructs the HTML project builder.
    ///
    /// WHAT: initializes a stateless builder implementation.
    /// WHY: builder policy is encoded in methods rather than runtime state.
    pub fn new() -> Self {
        Self {
            include_test_packages: false,
        }
    }

    /// Constructs a builder that includes integration-test external packages.
    ///
    /// WHAT: used by the integration test runner so test fixtures can import
    ///       `@test/pkg-a` and `@test/pkg-b` symbols.
    pub fn for_integration_tests() -> Self {
        Self {
            include_test_packages: true,
        }
    }
}

impl BackendBuilder for HtmlProjectBuilder {
    fn libraries(&self) -> LibrarySet {
        let mut libraries = LibrarySet::with_mandatory_core();
        libraries.expose_html_core_libraries();

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let html_lib_root =
            manifest_dir.join("src/projects/html_project/template_libraries/lib/html");
        libraries
            .source_libraries
            .register_filesystem_root("html", html_lib_root);

        if self.include_test_packages {
            libraries.external_packages = libraries
                .external_packages
                .with_test_packages_for_integration();
        }
        libraries
    }

    fn build_backend(
        &self,
        modules: Vec<Module>,
        config: &Config,
        flags: &[Flag],
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        parse_html_site_config(config, string_table)
            .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;
        let document_config = parse_html_document_config(config, string_table)
            .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

        if modules.is_empty() {
            return Err(CompilerMessages::from_error(
                CompilerError::compiler_error(
                    "HTML builder expected at least one compiled module but got 0.",
                ),
                string_table.clone(),
            ));
        }

        let release_build = flags.contains(&Flag::Release);
        let wasm_enabled = flags.contains(&Flag::HtmlWasm);
        let entry_paths = HtmlEntryPathPlan::from_config(config, string_table)?;

        let mut output_files = Vec::new();
        let mut output_paths = HashSet::new();
        let mut output_path_owners: HashMap<PathBuf, PathBuf> = HashMap::new();
        let mut entry_page_rel = None;
        let mut has_directory_homepage = false;
        let mut compiled_html_output_paths = Vec::with_capacity(modules.len());
        let mut warnings = Vec::new();

        for (module_index, module) in modules.iter().enumerate() {
            // Derive the canonical page route once. Both JS-only and HTML+Wasm output modes
            // consume this same path — downstream code must not re-derive route semantics.
            let logical_html_output_path = html_output_path(
                &module.entry_point,
                entry_paths.resolved_entry_root.as_deref(),
                string_table,
            )
            .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

            let compiled_artifacts = self.compile_one_module(
                module,
                &logical_html_output_path,
                config.project_name.as_str(),
                &document_config,
                release_build,
                wasm_enabled,
                string_table,
            )?;

            let html_output_path = compiled_artifacts.html_output_path.clone();
            for output_file in compiled_artifacts.output_files {
                let output_path = output_file.relative_output_path().to_path_buf();
                if let Some(existing_entry_point) = output_path_owners.get(&output_path) {
                    return Err(duplicate_output_path_error(
                        &module.entry_point,
                        existing_entry_point,
                        &output_path,
                        string_table,
                    ));
                }
                output_paths.insert(output_path.clone());
                output_path_owners.insert(output_path.clone(), module.entry_point.clone());
                output_files.push(output_file);
            }
            compiled_html_output_paths.push((module_index, html_output_path.clone()));

            if let Some(homepage_entry) = entry_paths.expected_homepage_entry.as_ref() {
                if module.entry_point == *homepage_entry {
                    has_directory_homepage = true;
                    entry_page_rel = Some(html_output_path.clone());
                }
            } else if entry_page_rel.is_none() {
                entry_page_rel = Some(html_output_path);
            }
        }

        entry_paths.require_homepage_if_directory_build(
            config,
            has_directory_homepage,
            string_table,
        )?;

        let mut tracked_assets = Vec::new();
        let mut tracked_asset_sources_by_output: HashMap<PathBuf, PathBuf> = HashMap::new();
        for (module_index, html_output_path) in &compiled_html_output_paths {
            let module = &modules[*module_index];
            let planned_assets =
                plan_module_tracked_assets(module, html_output_path, string_table)?;
            warnings.extend(planned_assets.warnings);

            for asset in planned_assets.assets {
                let output_path = asset.emitted_output_path.clone();

                if let Some(existing_source) = tracked_asset_sources_by_output.get(&output_path) {
                    if *existing_source == asset.source_filesystem_path {
                        continue;
                    }

                    return Err(conflicting_tracked_asset_output_error(
                        &asset.source_filesystem_path,
                        existing_source,
                        &output_path,
                        string_table,
                    ));
                }

                if !output_paths.insert(output_path.clone()) {
                    return Err(tracked_asset_conflicts_with_existing_output_error(
                        &asset.source_filesystem_path,
                        &output_path,
                        string_table,
                    ));
                }

                tracked_asset_sources_by_output
                    .insert(output_path.clone(), asset.source_filesystem_path.clone());
                tracked_assets.push(asset);
            }
        }
        output_files.extend(emit_tracked_assets(&tracked_assets, string_table)?);

        Ok(Project {
            output_files,
            entry_page_rel,
            cleanup_policy: CleanupPolicy::html(),
            warnings,
        })
    }

    fn validate_project_config(
        &self,
        config: &Config,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        // Validate HTML-specific configuration up front so build/dev runtime behavior stays
        // deterministic and all routing-policy mistakes are surfaced as config errors.
        parse_html_site_config(config, string_table)?;
        parse_html_document_config(config, string_table)?;

        // Empty dev/release folders are allowed and resolved by core build output logic.
        Ok(())
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        html_project_style_directives()
    }
}

impl HtmlProjectBuilder {
    /// Compile one module through the appropriate builder path (JS-only or HTML+Wasm).
    #[allow(clippy::too_many_arguments)]
    fn compile_one_module(
        &self,
        module: &Module,
        logical_html_output_path: &Path,
        project_name: &str,
        document_config: &crate::projects::html_project::document_config::HtmlDocumentConfig,
        release_build: bool,
        wasm_enabled: bool,
        string_table: &mut StringTable,
    ) -> Result<CompiledHtmlModuleArtifacts, CompilerMessages> {
        let libraries = self.libraries();

        // Validate that every external function call in the HIR has lowering metadata for the
        // target backend. WHY: fail early with a structured Rule error at the call site rather
        // than a vague backend-internal error during lowering.
        let backend_target = if wasm_enabled {
            BackendTarget::Wasm
        } else {
            BackendTarget::Js
        };
        validate_hir_external_package_support(
            &module.hir,
            &libraries.external_packages,
            backend_target,
        )
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

        let compile_input = HtmlModuleCompileInput {
            hir_module: &module.hir,
            const_fragments: &module.const_top_level_fragments,
            borrow_analysis: &module.borrow_analysis,
            project_name,
            document_config,
            release_build,
            entry_runtime_fragment_count: module.entry_runtime_fragment_count,
            external_package_registry: libraries.external_packages,
        };
        if wasm_enabled {
            let compiled_wasm =
                compile_html_module_wasm(&compile_input, string_table, logical_html_output_path)?;
            Ok(CompiledHtmlModuleArtifacts::from_wasm(compiled_wasm))
        } else {
            let output_file = compile_html_module_js(
                &compile_input,
                string_table,
                logical_html_output_path.to_path_buf(),
            )
            .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;
            Ok(CompiledHtmlModuleArtifacts::from_js(
                logical_html_output_path.to_path_buf(),
                output_file,
            ))
        }
    }
}

fn duplicate_output_path_error(
    duplicate_entry_point: &Path,
    existing_entry_point: &Path,
    output_path: &Path,
    string_table: &StringTable,
) -> CompilerMessages {
    let mut error_string_table = string_table.clone();
    let mut error = CompilerError::file_error(
        duplicate_entry_point,
        format!(
            "HTML builder produced duplicate output path '{}'. Entry '{}' conflicts with already-mapped entry '{}'. Ensure each '#*.bst' entry maps to a unique page output.",
            output_path.display(),
            duplicate_entry_point.display(),
            existing_entry_point.display(),
        ),
        &mut error_string_table,
    )
    .with_error_type(ErrorType::Config);
    error.metadata.insert(
        crate::compiler_frontend::compiler_errors::ErrorMetaDataKey::PrimarySuggestion,
        "Check your page routing configuration to ensure unique output paths".to_string(),
    );
    CompilerMessages::from_error(error, error_string_table)
}

fn conflicting_tracked_asset_output_error(
    source_path: &Path,
    existing_source_path: &Path,
    output_path: &Path,
    string_table: &StringTable,
) -> CompilerMessages {
    file_error_messages(
        source_path,
        format!(
            "Tracked asset '{}' would emit to '{}', but that output path is already claimed by '{}'.",
            source_path.display(),
            output_path.display(),
            existing_source_path.display(),
        ),
        string_table,
    )
}

fn tracked_asset_conflicts_with_existing_output_error(
    source_path: &Path,
    output_path: &Path,
    string_table: &StringTable,
) -> CompilerMessages {
    file_error_messages(
        source_path,
        format!(
            "Tracked asset '{}' would emit to '{}', but that output path is already claimed by another emitted HTML builder artifact.",
            source_path.display(),
            output_path.display(),
        ),
        string_table,
    )
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
#[path = "tests/html_project_builder_tests.rs"]
mod tests;
