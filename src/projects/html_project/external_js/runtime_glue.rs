//! Generated HTML JS glue for provider-created external module exports.
//!
//! WHAT: produces small ES modules that import from emitted JS runtime assets and re-export
//!       wrapper functions with stable names the JS backend can call.
//! WHY: keeps user-authored JS separate from generated glue, and gives the HTML builder control
//!      over module resolution, wrapper semantics, and dev/debug vs release validation.

use crate::backends::js::external_module_export_glue_function_name;
use crate::build_system::build::{FileKind, Module, OutputFile};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::external_packages::{
    ExternalFunctionId, ExternalJsLowering, ExternalPackageId, ExternalPackageRegistry,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::external_js::path_identity::stable_path_hash_hex;
use crate::projects::html_project::external_js::runtime_assets::js_runtime_asset_output_path;
use crate::projects::html_project::external_js::runtime_emission_plan::HtmlExternalRuntimeEmissionPlan;
use crate::projects::html_project::external_js::runtime_module_registry::RuntimeModuleRegistry;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Result of generating glue for a single compiled module.
///
/// WHAT: carries any emitted glue files and the import preamble that must be prepended to the
///       JS bundle so it can call the generated wrapper functions.
/// WHY: the caller (JS-only HTML path) decides how to embed the bundle and imports together.
pub(crate) struct ModuleGlueResult {
    /// Additional JS output files to emit (glue modules).
    pub glue_output_files: Vec<OutputFile>,
    /// Optional `import { ... } from "...";` statement to prepend to the bundle.
    pub bundle_import_preamble: Option<String>,
    /// Optional import-map HTML to inject before module scripts.
    pub import_map_html: Option<String>,
}

/// Generates glue and import-map HTML for a single compiled module.
///
/// WHAT: inspects the set of external functions referenced by the JS backend and produces:
///       - a glue ES module if any `ExternalModuleExport` functions were referenced;
///       - the import statement to prepend to the JS bundle;
///       - optional import-map HTML for bare runtime specifiers.
/// WHY: caller (JS-only HTML path) decides whether to inline the bundle as a module script
///      or emit it separately.
pub(crate) fn generate_module_glue(
    module: &Module,
    referenced_external_functions: &HashSet<ExternalFunctionId>,
    registry: &ExternalPackageRegistry,
    html_output_path: &Path,
    release_build: bool,
) -> Result<ModuleGlueResult, CompilerError> {
    let referenced_exports = collect_referenced_exports(referenced_external_functions, registry)?;
    if referenced_exports.is_empty() {
        return Ok(ModuleGlueResult {
            glue_output_files: Vec::new(),
            bundle_import_preamble: None,
            import_map_html: None,
        });
    }

    // Build a map from package ID to its emitted asset path so glue can import it.
    // WHY: paths must be relative to the glue module, not the HTML document, because
    //      the browser resolves ES module imports relative to the importing module's URL.
    let package_asset_paths = build_package_asset_path_map(module);

    // Generate the glue module source.
    let glue_source =
        generate_glue_module_source(&referenced_exports, &package_asset_paths, release_build)?;

    let glue_output_path = glue_module_output_path(module);

    let glue_output_file = OutputFile::new(glue_output_path.clone(), FileKind::Js(glue_source));

    let glue_import_path = relative_url_path(html_output_path, &glue_output_path);
    let glue_names: Vec<String> = referenced_exports
        .iter()
        .map(|exp| external_module_export_glue_function_name(exp.function_id))
        .collect();

    let import_preamble = format!(
        "import {{ {} }} from \"{}\";\n",
        glue_names.join(", "),
        glue_import_path
    );

    let import_map_html = build_import_map_html(module, html_output_path);

    Ok(ModuleGlueResult {
        glue_output_files: vec![glue_output_file],
        bundle_import_preamble: Some(import_preamble),
        import_map_html,
    })
}

/// Build-level emission of runtime modules from a pre-built emission plan.
///
/// WHAT: emits each unique runtime module once per build.
/// WHY: the plan already collected and deduplicated required specifiers, so this function
///      only handles registry lookup, output-path conflict checks, and file creation.
pub(crate) fn emit_build_runtime_modules(
    plan: &HtmlExternalRuntimeEmissionPlan,
    occupied_output_paths: &mut HashSet<PathBuf>,
    string_table: &StringTable,
) -> Result<Vec<OutputFile>, CompilerMessages> {
    let registry = RuntimeModuleRegistry::v1();
    let mut files = Vec::with_capacity(plan.runtime_module_specifiers().len());

    for specifier in plan.runtime_module_specifiers() {
        let Some(module_source) = registry.module_source(specifier) else {
            let message = format!(
                "Generated JS runtime module '{}' is required but is not registered.",
                specifier
            );
            return Err(CompilerMessages::from_error(
                CompilerError::compiler_error(message),
                string_table.clone(),
            ));
        };

        let runtime_path = runtime_module_output_path(specifier);
        if !occupied_output_paths.insert(runtime_path.clone()) {
            let message = format!(
                "Generated JS runtime module output path '{}' conflicts with an existing output.",
                runtime_path.display()
            );
            return Err(CompilerMessages::from_error(
                CompilerError::compiler_error(message),
                string_table.clone(),
            ));
        }

        files.push(OutputFile::new(
            runtime_path,
            FileKind::Js(module_source.to_owned()),
        ));
    }

    Ok(files)
}

/// Collects the subset of referenced external functions that use `ExternalModuleExport`.
fn collect_referenced_exports(
    referenced_external_functions: &HashSet<ExternalFunctionId>,
    registry: &ExternalPackageRegistry,
) -> Result<Vec<ReferencedExport>, CompilerError> {
    let mut exports = Vec::new();

    for function_id in referenced_external_functions {
        let Some(function_def) = registry.get_function_by_id(*function_id) else {
            continue;
        };
        let Some(lowering) = function_def.lowerings.js.as_ref() else {
            continue;
        };
        let ExternalJsLowering::ExternalModuleExport { export_name } = lowering else {
            continue;
        };
        let package_id = registry
            .resolve_function_package_id(*function_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "HTML JS glue could not resolve a package for external function '{}'.",
                    function_id.name()
                ))
            })?;

        exports.push(ReferencedExport {
            function_id: *function_id,
            package_id,
            export_name: export_name.clone(),
            raw_import_name: raw_export_import_name(*function_id),
            is_fallible: function_def.is_fallible(),
        });
    }

    exports.sort_by(|left, right| {
        external_function_sort_key(left.function_id)
            .cmp(&external_function_sort_key(right.function_id))
    });

    Ok(exports)
}

/// Metadata about one referenced external module export.
struct ReferencedExport {
    function_id: ExternalFunctionId,
    package_id: ExternalPackageId,
    export_name: String,
    raw_import_name: String,
    is_fallible: bool,
}

/// Build a map from package ID to the relative URL path of its emitted JS asset.
///
/// WHAT: computes paths relative to the glue module so ES module imports resolve correctly
///       when the glue module imports from emitted JS runtime assets.
fn build_package_asset_path_map(
    module: &Module,
) -> HashMap<crate::compiler_frontend::external_packages::ExternalPackageId, String> {
    let mut map = HashMap::new();
    let glue_output_path = glue_module_output_path(module);

    for external_import in &module.module_external_imports {
        let Some(asset) = &external_import.runtime_asset else {
            continue;
        };
        if asset.asset_kind != "js" {
            continue;
        }
        let output_path = js_runtime_asset_output_path(&asset.canonical_source_path);
        let relative = relative_url_path(&glue_output_path, &output_path);
        map.insert(external_import.package_id, relative);
    }

    map
}

/// Generate the glue module ES module source.
fn generate_glue_module_source(
    exports: &[ReferencedExport],
    package_asset_paths: &HashMap<ExternalPackageId, String>,
    release_build: bool,
) -> Result<String, CompilerError> {
    let mut source = String::new();

    // Group imports by asset path so we emit one import statement per asset.
    let mut imports_by_path: HashMap<String, Vec<(String, String)>> = HashMap::new();
    for export in exports {
        let path = package_asset_paths.get(&export.package_id).ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HTML JS glue could not find a runtime asset for external package {:?}.",
                export.package_id
            ))
        })?;
        imports_by_path
            .entry(path.clone())
            .or_default()
            .push((export.export_name.clone(), export.raw_import_name.clone()));
    }

    // Emit import statements.
    let mut sorted_paths: Vec<_> = imports_by_path.keys().cloned().collect();
    sorted_paths.sort();
    for path in sorted_paths {
        let mut names = imports_by_path.get(&path).cloned().unwrap_or_default();
        names.sort();
        names.dedup();
        let import_names = names
            .iter()
            .map(|(export_name, local_name)| format!("{export_name} as {local_name}"))
            .collect::<Vec<_>>();
        source.push_str(&format!(
            "import {{ {} }} from \"{}\";\n",
            import_names.join(", "),
            path
        ));
    }

    // Emit wrapper functions.
    for export in exports {
        let wrapper_name = external_module_export_glue_function_name(export.function_id);
        source.push('\n');

        if export.is_fallible {
            source.push_str(&generate_fallible_wrapper(
                &wrapper_name,
                &export.raw_import_name,
                release_build,
            ));
        } else {
            source.push_str(&generate_infallible_wrapper(
                &wrapper_name,
                &export.raw_import_name,
            ));
        }
    }

    Ok(source)
}

/// Generates a non-fallible wrapper that forwards all arguments and returns the raw result.
fn generate_infallible_wrapper(wrapper_name: &str, export_name: &str) -> String {
    format!(
        "export function {wrapper_name}(...args) {{
    return {export_name}(...args);
}}
"
    )
}

/// Generates a fallible wrapper that validates the external result shape and converts it to
/// Beanstalk's internal fallible carrier.
///
/// WHAT: calls the raw JS export, expects `{ ok: boolean, value? }` or `{ ok: false, error }`,
///       and returns `{ tag: "ok", value: ... }` or `{ tag: "err", value: { message, code } }`.
/// WHY: the JS backend HIR lowering assumes all fallible calls return this carrier shape.
fn generate_fallible_wrapper(wrapper_name: &str, export_name: &str, release_build: bool) -> String {
    let invalid_wrapper_handling = if release_build {
        String::from(
            "        return { tag: \"err\", value: { message: \"Invalid result wrapper from external JavaScript function\", code: 0 } };",
        )
    } else {
        format!(
            "        throw new Error(
            \"Invalid result wrapper from external function '{wrapper_name}': \" +
            \"expected {{ ok: boolean, value? }} or {{ ok: false, error: {{ code, message }} }}\"
        );"
        )
    };

    format!(
        "export function {wrapper_name}(...args) {{
    let result;
    try {{
        result = {export_name}(...args);
    }} catch (e) {{
        return {{ tag: \"err\", value: {{ message: String(e.message || e), code: 0 }} }};
    }}

    if (result && typeof result.ok === \"boolean\") {{
        if (result.ok === true) {{
            return {{ tag: \"ok\", value: result.value }};
        }}
        if (result.ok === false) {{
            const error = result.error || {{ message: \"Unknown error\", code: 0 }};
            return {{ tag: \"err\", value: {{ message: error.message || \"Unknown error\", code: typeof error.code === \"number\" ? error.code : 0 }} }};
        }}
    }}

{invalid_wrapper_handling}
}}
"
    )
}

/// Build import-map HTML for bare runtime specifiers.
///
/// WHAT: produces a `<script type=\"importmap\">` that maps registered core module specifiers
///       to their emitted relative paths.
/// WHY: provider-created JS assets use bare imports like `import {{ bstOk }} from "@beanstalk/runtime";`;
///      the import map lets the browser resolve those without rewriting user files.
fn build_import_map_html(module: &Module, html_output_path: &Path) -> Option<String> {
    let mut entries: Vec<(String, String)> = Vec::new();

    for external_import in &module.module_external_imports {
        for runtime_import in &external_import.required_runtime_imports {
            let runtime_path = runtime_module_output_path(&runtime_import.module_name);
            let relative = relative_url_path(html_output_path, &runtime_path);
            entries.push((runtime_import.module_name.clone(), relative));
        }
    }

    if entries.is_empty() {
        return None;
    }

    // Deduplicate by specifier.
    entries.sort_by(|a, b| a.0.cmp(&b.0));
    entries.dedup_by(|a, b| a.0 == b.0);

    let mut map_json = String::from("{\n  \"imports\": {\n");
    for (index, (specifier, path)) in entries.iter().enumerate() {
        if index > 0 {
            map_json.push_str(",\n");
        }
        map_json.push_str(&format!("    \"{specifier}\": \"{path}\""));
    }
    map_json.push_str("\n  }\n}");

    Some(format!(
        "<script type=\"importmap\">\n{map_json}\n</script>\n"
    ))
}

fn raw_export_import_name(id: ExternalFunctionId) -> String {
    match id {
        ExternalFunctionId::Synthetic(n) => format!("__bs_external_fn{n}"),
        other => format!("__bs_external_{}", other.name()),
    }
}

fn external_function_sort_key(id: ExternalFunctionId) -> String {
    match id {
        ExternalFunctionId::Synthetic(n) => format!("synthetic:{n:010}"),
        other => format!("builtin:{}", other.name()),
    }
}

/// Deterministic output path for a module's glue ES module.
fn glue_module_output_path(module: &Module) -> PathBuf {
    let entry_hash = stable_path_hash_hex(&module.entry_point);
    PathBuf::from("_beanstalk/js/glue").join(format!("module-{entry_hash}.js"))
}

/// Deterministic output path for a core runtime module.
fn runtime_module_output_path(specifier: &str) -> PathBuf {
    let safe_name = runtime_module_safe_name(specifier);
    PathBuf::from("_beanstalk/js/runtime").join(format!("{safe_name}.js"))
}

fn runtime_module_safe_name(specifier: &str) -> String {
    let trimmed = specifier.trim_start_matches('@');
    let mut safe_name = String::new();

    for ch in trimmed.chars() {
        if ch.is_ascii_alphanumeric() {
            safe_name.push(ch.to_ascii_lowercase());
        } else if !safe_name.ends_with('-') {
            safe_name.push('-');
        }
    }

    let safe_name = safe_name.trim_matches('-');
    if safe_name.is_empty() {
        "runtime-module".to_owned()
    } else {
        safe_name.to_owned()
    }
}

/// Compute a relative URL path from an HTML document to an asset.
///
/// WHAT: returns a relative path string using `/` separators suitable for use in an HTML
///       `src`, `href`, or module import specifier.
/// WHY: both the HTML and assets are emitted with relative paths from the project root;
///      the browser resolves relative module specifiers against the document URL.
fn relative_url_path(from_html: &Path, to_asset: &Path) -> String {
    let from_components: Vec<_> = from_html.components().collect();
    let to_components: Vec<_> = to_asset.components().collect();

    // Find common prefix length, excluding the HTML file name itself.
    let mut common = 0;
    let from_dir_len = from_components.len().saturating_sub(1);
    while common < from_dir_len
        && common < to_components.len()
        && from_components[common] == to_components[common]
    {
        common += 1;
    }

    let mut path = String::new();

    // Go up one level for each remaining directory in the HTML path.
    for _ in common..from_dir_len {
        path.push_str("../");
    }

    // Go down through the remaining asset components.
    for component in to_components.iter().skip(common) {
        if !path.is_empty() && !path.ends_with('/') {
            path.push('/');
        }
        path.push_str(&component.as_os_str().to_string_lossy());
    }

    if path.is_empty() {
        path.push_str(
            &to_asset
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
        );
    }

    if !path.starts_with("./") && !path.starts_with("../") && !path.starts_with('/') {
        path.insert_str(0, "./");
    }

    path
}

#[cfg(test)]
#[path = "tests/runtime_glue_tests.rs"]
mod tests;
