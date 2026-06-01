// HTML project builder orchestration.
//
// WHAT: coordinates module output-path resolution, homepage checks, and backend selection.
// WHY: project builders own artifact assembly policy while compiler backends stay generic.
use crate::backends::backend_feature_validation::{
    BackendFeatureValidationError, validate_hir_backend_feature_support,
};
use crate::backends::external_package_validation::{
    BackendTarget, ExternalPackageValidationError, validate_hir_external_package_support,
};
use crate::build_system::build::{BackendBuilder, CleanupPolicy, Module, OutputFile, Project};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::LibrarySet;
use crate::projects::html_project::compile_input::HtmlModuleCompileInput;
use crate::projects::html_project::diagnostics::{
    duplicate_html_output_path_messages, tracked_asset_builder_output_conflict_messages,
    tracked_asset_output_conflict_messages,
};
use crate::projects::html_project::document_config::parse_html_document_config;
use crate::projects::html_project::external_js::js_import_provider::JsExternalImportProvider;
use crate::projects::html_project::external_js::runtime_assets::emit_external_js_runtime_assets;
use crate::projects::html_project::external_js::runtime_emission_plan::HtmlExternalRuntimeEmissionPlan;
use crate::projects::html_project::external_js::runtime_glue::emit_build_runtime_modules;
use crate::projects::html_project::external_libraries::web::canvas::register_web_canvas_package;
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
use crate::projects::settings::{Config, ProjectConfigError};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

const HTML_SOURCE_LIBRARY_PREFIX: &str = "html";

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
    fn build_backend(
        &self,
        modules: Vec<Module>,
        config: &Config,
        flags: &[Flag],
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        parse_html_site_config(config, string_table)
            .map_err(|error| error.into_messages(string_table.clone()))?;
        let document_config = parse_html_document_config(config, string_table)
            .map_err(|error| error.into_messages(string_table.clone()))?;

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

        let runtime_emission_plan = HtmlExternalRuntimeEmissionPlan::from_modules(&modules);

        output_files.extend(emit_external_js_runtime_assets(
            &runtime_emission_plan,
            &mut output_paths,
            string_table,
        )?);

        output_files.extend(emit_build_runtime_modules(
            &runtime_emission_plan,
            &mut output_paths,
            string_table,
        )?);

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
    ) -> Result<(), ProjectConfigError> {
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

    fn libraries(&self) -> LibrarySet {
        let mut libraries = LibrarySet::with_mandatory_core();
        libraries.source_libraries.register_filesystem_root(
            HTML_SOURCE_LIBRARY_PREFIX,
            LibrarySet::builtin_source_library_root(HTML_SOURCE_LIBRARY_PREFIX),
        );

        libraries.expose_html_core_libraries();

        let canvas_metadata = register_web_canvas_package(&mut libraries.external_packages);
        libraries.builder_runtime_packages.push(canvas_metadata);

        Self::register_html_config_keys(&mut libraries);

        libraries
            .external_import_providers
            .register(std::sync::Arc::new(JsExternalImportProvider::new()));

        if self.include_test_packages {
            libraries.external_packages = libraries
                .external_packages
                .with_test_packages_for_integration();
        }

        libraries
    }
}

impl HtmlProjectBuilder {
    /// Register HTML-backend-specific config keys into the library set's key registry.
    ///
    /// WHY: Stage 0 config loading must know which keys are valid before backend semantic
    /// validation runs. Keeping registration here keeps HTML-specific meaning out of the core.
    fn register_html_config_keys(libraries: &mut LibrarySet) {
        let registry = &mut libraries.config_keys;

        // Routing / site keys
        registry.register_backend_string("origin");
        registry.register_backend_string("page_url_style");
        registry.register_backend_bool("redirect_index_html");

        // HTML document shell keys
        registry.register_backend_string("html_lang");
        registry.register_backend_string("html_title_prefix");
        registry.register_backend_string("html_title_postfix");
        registry.register_backend_string("html_favicon");
        registry.register_backend_bool("html_inject_charset");
        registry.register_backend_bool("html_inject_viewport");
        registry.register_backend_bool("html_inject_color_scheme");
        registry.register_backend_bool("html_inject_core_css");
        registry.register_backend_string("html_body_style");
    }

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
            &module.external_package_registry,
            backend_target,
            string_table,
        )
        .map_err(|error| match error {
            ExternalPackageValidationError::Diagnostic(diagnostic) => {
                CompilerMessages::from_diagnostic_ref(*diagnostic, string_table)
            }
            ExternalPackageValidationError::Infrastructure(error) => {
                CompilerMessages::from_error_ref(*error, string_table)
            }
        })?;

        validate_hir_backend_feature_support(&module.hir, backend_target, string_table).map_err(
            |error| match error {
                BackendFeatureValidationError::Diagnostic(diagnostic) => {
                    CompilerMessages::from_diagnostic_ref(*diagnostic, string_table)
                }
                BackendFeatureValidationError::Infrastructure(error) => {
                    CompilerMessages::from_error_ref(*error, string_table)
                }
            },
        )?;

        let compile_input = HtmlModuleCompileInput {
            hir_module: &module.hir,
            type_environment: &module.type_environment,
            const_fragments: &module.const_top_level_fragments,
            borrow_analysis: &module.borrow_analysis,
            project_name,
            document_config,
            release_build,
            entry_runtime_fragment_count: module.entry_runtime_fragment_count,
            external_package_registry: module.external_package_registry.clone(),
        };
        if wasm_enabled {
            let compiled_wasm =
                compile_html_module_wasm(&compile_input, string_table, logical_html_output_path)?;
            Ok(CompiledHtmlModuleArtifacts::from_wasm(compiled_wasm))
        } else {
            let compiled_js = compile_html_module_js(
                module,
                &compile_input,
                string_table,
                logical_html_output_path.to_path_buf(),
            )?;
            Ok(CompiledHtmlModuleArtifacts {
                output_files: compiled_js.output_files,
                html_output_path: compiled_js.html_output_path,
            })
        }
    }
}

fn duplicate_output_path_error(
    duplicate_entry_point: &Path,
    existing_entry_point: &Path,
    output_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    duplicate_html_output_path_messages(
        duplicate_entry_point,
        existing_entry_point,
        output_path,
        string_table,
    )
}

fn conflicting_tracked_asset_output_error(
    source_path: &Path,
    existing_source_path: &Path,
    output_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    tracked_asset_output_conflict_messages(
        source_path,
        existing_source_path,
        output_path,
        string_table,
    )
}

fn tracked_asset_conflicts_with_existing_output_error(
    source_path: &Path,
    output_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    tracked_asset_builder_output_conflict_messages(source_path, output_path, string_table)
}

struct CompiledHtmlModuleArtifacts {
    /// Full emitted output set for one module (HTML only or HTML+Wasm trio).
    output_files: Vec<OutputFile>,
    /// HTML entry path used for homepage selection and serving/open behavior.
    html_output_path: PathBuf,
}

impl CompiledHtmlModuleArtifacts {
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
