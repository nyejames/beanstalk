//! HTML+Wasm artifact planning and emission helpers.
//!
//! WHAT: coordinates HTML-builder-specific planning around the generic Wasm backend.
//! WHY: this keeps orchestration concerns local to the builder and avoids leaking HTML policy
//! into backend lowering/emission modules.

use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::backends::wasm::backend::lower_hir_to_wasm_module;
use crate::backends::wasm::request::WasmBackendRequest;
use crate::build_system::build::{FileKind, OutputFile};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirModule};
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::html_project::js_path::{RuntimeSlotMount, render_entry_fragments};
use crate::projects::html_project::wasm::export_plan::{
    HtmlWasmExportPlan, build_html_wasm_export_plan,
};
use crate::projects::html_project::wasm::js_bootstrap::generate_wasm_bootstrap_js;
use crate::projects::html_project::wasm::request::build_wasm_backend_request;
use crate::projects::routing::HtmlSiteConfig;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const SHOW_HTML_WASM_PLAN: bool = false;
const SHOW_HTML_WASM_JS: bool = false;
const SHOW_HTML_WASM_EXPORTS: bool = false;

#[derive(Debug, Clone)]
pub(crate) struct HtmlWasmBuildPlan {
    /// Deterministic export selection and wrapper naming policy for this module.
    pub export_plan: HtmlWasmExportPlan,
    /// Ordered runtime slot mounts copied from start fragments.
    pub js_entry_fragments: Vec<RuntimeSlotMount>,
    /// JS start invocation snippet reused in bootstrap emission and debug summaries.
    pub js_start_invocation: String,
    /// Generic backend request derived from builder policy.
    pub wasm_request: WasmBackendRequest,
}

#[derive(Debug, Clone)]
pub(crate) struct HtmlWasmArtifacts {
    /// Final emitted wasm binary for this route module.
    pub wasm_bytes: Vec<u8>,
    /// Generated page bootstrap JavaScript loaded by `index.html`.
    pub bootstrap_js: String,
    /// Route document shell with runtime slot mounts and script include.
    pub html: String,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct HtmlWasmDebugOutputs {
    /// Builder-local export/runtime planning summary.
    pub plan_summary: Option<String>,
    /// Helper export requirements summary for deterministic debugging.
    pub helper_exports_summary: Option<String>,
    /// Artifact paths/sizes summary for golden-style assertions.
    pub artifact_summary: Option<String>,
    /// Preview of generated JS bootstrap (plus backend export text when available).
    pub js_bootstrap_preview: Option<String>,
}

pub(crate) struct CompiledHtmlWasmModule {
    /// Final artifact list emitted for this module route.
    pub output_files: Vec<OutputFile>,
    /// Route HTML path used by homepage/entry-page tracking.
    pub html_output_path: PathBuf,
    /// Optional debug text payloads used by internal debug toggles.
    pub debug: HtmlWasmDebugOutputs,
}

#[derive(Debug, Clone)]
struct HtmlWasmOutputPaths {
    /// Relative `index.html` path for this route.
    html_path: PathBuf,
    /// Relative `page.js` path colocated with `index.html`.
    js_path: PathBuf,
    /// Relative `page.wasm` path colocated with `index.html`.
    wasm_path: PathBuf,
}

/// Compiles a single module through the HTML+Wasm builder path.
///
/// WHAT: lowers JS and Wasm artifacts, generates bootstrap JS, and emits route-indexed outputs.
/// WHY: keeps the HTML builder in charge of artifact layout while delegating Wasm lowering.
pub(crate) fn compile_html_module_wasm(
    hir_module: &HirModule,
    borrow_analysis: &BorrowCheckReport,
    string_table: &StringTable,
    logical_html_output_path: &Path,
    release_build: bool,
    _config: &HtmlSiteConfig,
) -> Result<CompiledHtmlWasmModule, CompilerMessages> {
    // Convert logical route output (`about.html`) into route-folder artifacts (`about/index.html` etc).
    let output_paths =
        wasm_output_paths_for_html_route(logical_html_output_path).map_err(single_error)?;

    let js_lowering_config = JsLoweringConfig {
        pretty: !release_build,
        emit_locations: false,
        auto_invoke_start: false,
    };
    let js_module = lower_hir_to_js(
        hir_module,
        borrow_analysis,
        string_table,
        js_lowering_config,
    )
    .map_err(single_error)?;
    let (entry_fragment_html, runtime_slots) =
        render_entry_fragments(hir_module).map_err(single_error)?;
    let build_plan =
        build_html_wasm_plan(hir_module, &js_module.function_name_by_id, runtime_slots)
            .map_err(single_error)?;

    let wasm_result = lower_hir_to_wasm_module(
        hir_module,
        borrow_analysis.borrow_facts(),
        &build_plan.wasm_request,
    )?;
    let wasm_bytes = wasm_result.wasm_bytes.ok_or_else(|| {
        single_error(CompilerError::compiler_error(
            "HTML Wasm mode expected emitted wasm bytes, but the backend returned none",
        ))
    })?;

    let artifacts = emit_html_wasm_artifacts(
        &build_plan,
        &entry_fragment_html,
        hir_module,
        &js_module.source,
        &js_module.function_name_by_id,
        wasm_bytes,
    )
    .map_err(single_error)?;
    let debug_outputs = build_debug_outputs(
        &build_plan,
        &artifacts,
        wasm_result.debug_outputs.plan_text,
        wasm_result.debug_outputs.exports_text,
    );
    emit_debug_outputs_if_enabled(&debug_outputs);

    Ok(CompiledHtmlWasmModule {
        output_files: vec![
            OutputFile::new(
                output_paths.html_path.clone(),
                FileKind::Html(artifacts.html),
            ),
            OutputFile::new(output_paths.js_path, FileKind::Js(artifacts.bootstrap_js)),
            OutputFile::new(output_paths.wasm_path, FileKind::Wasm(artifacts.wasm_bytes)),
        ],
        html_output_path: output_paths.html_path,
        debug: debug_outputs,
    })
}

/// Builds builder-local Wasm planning state before invoking the backend.
///
/// WHAT: keeps request construction deterministic and debuggable.
/// WHY: HTML orchestration must remain explicit and stable while backend internals evolve.
pub(crate) fn build_html_wasm_plan(
    hir_module: &HirModule,
    function_name_by_id: &std::collections::HashMap<FunctionId, String>,
    js_entry_fragments: Vec<RuntimeSlotMount>,
) -> Result<HtmlWasmBuildPlan, CompilerError> {
    let export_plan = build_html_wasm_export_plan(hir_module)?;
    let wasm_request = build_wasm_backend_request(&export_plan);
    let start_function_name = function_name_by_id
        .get(&hir_module.start_function)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HTML Wasm plan could not resolve JS start function name for {:?}",
                hir_module.start_function
            ))
        })?
        .clone();
    let js_start_invocation =
        format!("if (typeof {start_function_name} === \"function\") {start_function_name}();");

    Ok(HtmlWasmBuildPlan {
        export_plan,
        js_entry_fragments,
        js_start_invocation,
        wasm_request,
    })
}

/// Emits final HTML+Wasm artifacts from the builder plan and backend output.
///
/// WHAT: produces `page.js`, `page.wasm`, and route `index.html`.
/// WHY: keeping one emission function avoids path/policy drift across call sites.
pub(crate) fn emit_html_wasm_artifacts(
    plan: &HtmlWasmBuildPlan,
    entry_fragment_html: &str,
    hir_module: &HirModule,
    js_bundle: &str,
    function_name_by_id: &std::collections::HashMap<FunctionId, String>,
    wasm_bytes: Vec<u8>,
) -> Result<HtmlWasmArtifacts, CompilerError> {
    let bootstrap_js = generate_wasm_bootstrap_js(
        hir_module,
        js_bundle,
        function_name_by_id,
        &plan.export_plan,
        &plan.js_entry_fragments,
        &plan.js_start_invocation,
    )?;
    let html = render_wasm_html_document(entry_fragment_html);

    Ok(HtmlWasmArtifacts {
        wasm_bytes,
        bootstrap_js,
        html,
    })
}

fn render_wasm_html_document(entry_fragment_html: &str) -> String {
    // Keep Wasm mode HTML shell minimal and delegate runtime orchestration to `page.js`.
    let mut html = String::new();
    html.push_str(entry_fragment_html);
    html.push_str("<script src=\"./page.js\"></script>\n");
    html
}

fn wasm_output_paths_for_html_route(
    logical_html_output_path: &Path,
) -> Result<HtmlWasmOutputPaths, CompilerError> {
    // Convert route logical paths into per-page folder artifacts:
    // - `index.html` -> `index.html`, `page.js`, `page.wasm`
    // - `about/index.html` -> `about/index.html`, `about/page.js`, `about/page.wasm`
    // - legacy `about.html` -> `about/index.html`, `about/page.js`, `about/page.wasm`
    let route_base = if logical_html_output_path == Path::new("index.html") {
        PathBuf::new()
    } else if logical_html_output_path
        .file_name()
        .and_then(|name| name.to_str())
        == Some("index.html")
    {
        logical_html_output_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_default()
    } else {
        if logical_html_output_path
            .extension()
            .and_then(|ext| ext.to_str())
            != Some("html")
        {
            return Err(CompilerError::file_error(
                logical_html_output_path,
                format!(
                    "HTML Wasm output conversion expected an '.html' path, got '{}'",
                    logical_html_output_path.display()
                ),
            ));
        }
        logical_html_output_path.with_extension("")
    };

    if route_base.as_os_str().is_empty() {
        Ok(HtmlWasmOutputPaths {
            html_path: PathBuf::from("index.html"),
            js_path: PathBuf::from("page.js"),
            wasm_path: PathBuf::from("page.wasm"),
        })
    } else {
        Ok(HtmlWasmOutputPaths {
            html_path: route_base.join("index.html"),
            js_path: route_base.join("page.js"),
            wasm_path: route_base.join("page.wasm"),
        })
    }
}

fn build_debug_outputs(
    plan: &HtmlWasmBuildPlan,
    artifacts: &HtmlWasmArtifacts,
    wasm_plan_text: Option<String>,
    wasm_exports_text: Option<String>,
) -> HtmlWasmDebugOutputs {
    // Build deterministic debug text so golden-style comparisons stay stable when enabled.
    let mut debug = HtmlWasmDebugOutputs::default();

    let mut plan_summary = String::new();
    let _ = writeln!(
        plan_summary,
        "HTML Wasm build plan: runtime_slots={} requested_exports={}",
        plan.js_entry_fragments.len(),
        plan.export_plan.function_exports.len()
    );
    let _ = writeln!(
        plan_summary,
        "start_invocation: {}",
        plan.js_start_invocation
    );
    if let Some(wasm_plan_text) = wasm_plan_text {
        let _ = writeln!(plan_summary, "{wasm_plan_text}");
    }
    debug.plan_summary = Some(plan_summary);

    let helper = &plan.export_plan.helper_exports;
    debug.helper_exports_summary = Some(format!(
        "helper_exports: memory={} bst_str_ptr={} bst_str_len={} bst_release={}",
        helper.export_memory, helper.export_str_ptr, helper.export_str_len, helper.export_release
    ));

    debug.artifact_summary = Some(format!(
        "artifacts: html_bytes={} js_bytes={} wasm_bytes={}",
        artifacts.html.len(),
        artifacts.bootstrap_js.len(),
        artifacts.wasm_bytes.len()
    ));

    if let Some(wasm_exports_text) = wasm_exports_text {
        let mut preview = String::new();
        let _ = writeln!(preview, "{wasm_exports_text}");
        let lines = artifacts.bootstrap_js.lines().take(40);
        for line in lines {
            let _ = writeln!(preview, "{line}");
        }
        debug.js_bootstrap_preview = Some(preview);
    }

    debug
}

fn emit_debug_outputs_if_enabled(debug: &HtmlWasmDebugOutputs) {
    // Toggle-gated debug printing keeps normal builds deterministic and quiet.
    if SHOW_HTML_WASM_PLAN && let Some(text) = &debug.plan_summary {
        println!("{text}");
    }
    if SHOW_HTML_WASM_EXPORTS && let Some(text) = &debug.helper_exports_summary {
        println!("{text}");
    }
    if SHOW_HTML_WASM_EXPORTS && let Some(text) = &debug.artifact_summary {
        println!("{text}");
    }
    if SHOW_HTML_WASM_JS && let Some(text) = &debug.js_bootstrap_preview {
        println!("{text}");
    }
}

fn single_error(error: CompilerError) -> CompilerMessages {
    // Builder internals often return one root error; normalize into compiler message shape.
    CompilerMessages {
        errors: vec![error],
        warnings: Vec::new(),
    }
}
