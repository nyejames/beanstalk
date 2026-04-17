//! HTML+Wasm artifact planning and emission helpers.
//!
//! WHAT: coordinates HTML-builder-specific planning around the generic Wasm backend.
//! WHY: this keeps orchestration concerns local to the builder and avoids leaking HTML policy
//! into backend lowering/emission modules.

use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::backends::wasm::backend::lower_hir_to_wasm_module;
use crate::backends::wasm::request::WasmBackendRequest;
use crate::build_system::build::{FileKind, OutputFile, ResolvedConstFragment};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::hir::hir_nodes::HirModule;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::document_shell::render_html_document_shell;
use crate::projects::html_project::js_path::render_entry_fragments;
use crate::projects::html_project::output_plan::plan_wasm_output_from_logical_html_path;
use crate::projects::html_project::page_metadata::extract_html_page_metadata;
use crate::projects::html_project::wasm::export_plan::{
    HtmlWasmExportPlan, build_html_wasm_export_plan,
};
use crate::projects::html_project::wasm::js_bootstrap::generate_wasm_bootstrap_js;
use crate::projects::html_project::wasm::request::build_wasm_backend_request;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

const SHOW_HTML_WASM_PLAN: bool = false;
const SHOW_HTML_WASM_JS: bool = false;
const SHOW_HTML_WASM_EXPORTS: bool = false;

#[derive(Debug, Clone)]
pub(crate) struct HtmlWasmBuildPlan {
    /// Deterministic export selection and wrapper naming policy for this module.
    pub export_plan: HtmlWasmExportPlan,
    /// Ordered runtime slot IDs derived from entry start() PushRuntimeFragment sequence.
    pub js_entry_slot_ids: Vec<String>,
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

/// Inputs required to emit final route artifacts for HTML+Wasm mode.
///
/// WHAT: groups backend outputs plus route/document metadata used during final emission.
/// WHY: emission is called from one orchestration site and should avoid a long argument list.
pub(crate) struct HtmlWasmArtifactEmitInput<'a> {
    pub entry_fragment_html: &'a str,
    pub string_table: &'a StringTable,
    pub logical_html_output_path: &'a Path,
    pub project_name: &'a str,
    pub document_config: &'a HtmlDocumentConfig,
    pub hir_module: &'a HirModule,
    pub js_bundle: &'a str,
    pub wasm_bytes: Vec<u8>,
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

/// Compiles a single module through the HTML+Wasm builder path.
///
/// WHAT: lowers JS and Wasm artifacts, generates bootstrap JS, and emits route-indexed outputs.
/// WHY: keeps the HTML builder in charge of artifact layout while delegating Wasm lowering.
pub(crate) fn compile_html_module_wasm(
    hir_module: &HirModule,
    const_fragments: &[ResolvedConstFragment],
    borrow_analysis: &BorrowCheckReport,
    string_table: &mut StringTable,
    logical_html_output_path: &Path,
    project_name: &str,
    document_config: &HtmlDocumentConfig,
    release_build: bool,
    entry_runtime_fragment_count: usize,
) -> Result<CompiledHtmlWasmModule, CompilerMessages> {
    // Derive per-route artifact paths from the already-derived logical HTML path.
    // WHY: the builder has already computed the canonical route via derive_logical_html_path.
    //      This planner only colocates JS/Wasm artifacts beside the HTML output — no re-derivation.
    let output_plan = plan_wasm_output_from_logical_html_path(logical_html_output_path)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

    let js_lowering_config = JsLoweringConfig::standard_html(release_build);
    let js_module = lower_hir_to_js(
        hir_module,
        borrow_analysis,
        string_table,
        js_lowering_config,
    )
    .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

    let (entry_fragment_html, slot_ids) =
        render_entry_fragments(const_fragments, entry_runtime_fragment_count);

    let build_plan = build_html_wasm_plan(hir_module, slot_ids)
        .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;

    let wasm_result = lower_hir_to_wasm_module(
        hir_module,
        borrow_analysis.borrow_facts(),
        &build_plan.wasm_request,
        string_table,
    )?;
    let wasm_bytes = wasm_result.wasm_bytes.ok_or_else(|| {
        CompilerMessages::from_error(
            CompilerError::compiler_error(
                "HTML Wasm mode expected emitted wasm bytes, but the backend returned none",
            ),
            string_table.clone(),
        )
    })?;

    let artifacts = emit_html_wasm_artifacts(
        &build_plan,
        HtmlWasmArtifactEmitInput {
            entry_fragment_html: &entry_fragment_html,
            string_table,
            logical_html_output_path,
            project_name,
            document_config,
            hir_module,
            js_bundle: &js_module.source,
            wasm_bytes,
        },
    )
    .map_err(|error| CompilerMessages::from_error(error, string_table.clone()))?;
    let debug_outputs = build_debug_outputs(
        &build_plan,
        &artifacts,
        wasm_result.debug_outputs.plan_text,
        wasm_result.debug_outputs.exports_text,
    );
    emit_debug_outputs_if_enabled(&debug_outputs);

    let js_path = output_plan.js_path.expect("Wasm plan always has a js_path");
    let wasm_path = output_plan
        .wasm_path
        .expect("Wasm plan always has a wasm_path");
    Ok(CompiledHtmlWasmModule {
        output_files: vec![
            OutputFile::new(
                output_plan.html_path.clone(),
                FileKind::Html(artifacts.html),
            ),
            OutputFile::new(js_path, FileKind::Js(artifacts.bootstrap_js)),
            OutputFile::new(wasm_path, FileKind::Wasm(artifacts.wasm_bytes)),
        ],
        html_output_path: output_plan.html_path,
        debug: debug_outputs,
    })
}

/// Builds builder-local Wasm planning state before invoking the backend.
///
/// WHAT: keeps request construction deterministic and debuggable.
/// WHY: HTML orchestration must remain explicit and stable while backend internals evolve.
pub(crate) fn build_html_wasm_plan(
    hir_module: &HirModule,
    js_entry_slot_ids: Vec<String>,
) -> Result<HtmlWasmBuildPlan, CompilerError> {
    let export_plan = build_html_wasm_export_plan(hir_module)?;
    let wasm_request = build_wasm_backend_request(&export_plan);
    // WHY: entry start() is exported as "bst_start"; JS evaluates it directly and consumes the
    //      returned fragment Vec handle. No JS-side wrapper installation is part of the contract.
    let js_start_invocation = String::from("instance.exports.bst_start()");

    Ok(HtmlWasmBuildPlan {
        export_plan,
        js_entry_slot_ids,
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
    input: HtmlWasmArtifactEmitInput<'_>,
) -> Result<HtmlWasmArtifacts, CompilerError> {
    let HtmlWasmArtifactEmitInput {
        entry_fragment_html,
        string_table,
        logical_html_output_path,
        project_name,
        document_config,
        hir_module,
        js_bundle,
        wasm_bytes,
    } = input;

    let bootstrap_js = generate_wasm_bootstrap_js(
        js_bundle,
        &plan.js_entry_slot_ids,
        &plan.js_start_invocation,
    )?;
    let page_metadata = extract_html_page_metadata(hir_module, string_table)?;
    let html = render_wasm_html_document(
        document_config,
        &page_metadata,
        logical_html_output_path,
        project_name,
        entry_fragment_html,
    );

    Ok(HtmlWasmArtifacts {
        wasm_bytes,
        bootstrap_js,
        html,
    })
}

fn render_wasm_html_document(
    document_config: &HtmlDocumentConfig,
    page_metadata: &crate::projects::html_project::page_metadata::HtmlPageMetadata,
    logical_html_output_path: &Path,
    project_name: &str,
    entry_fragment_html: &str,
) -> String {
    render_html_document_shell(
        document_config,
        page_metadata,
        logical_html_output_path,
        project_name,
        entry_fragment_html.to_string(),
        String::from("<script src=\"./page.js\"></script>\n"),
    )
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
        plan.js_entry_slot_ids.len(),
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
        "helper_exports: memory={} bst_str_ptr={} bst_str_len={} bst_vec_new={} bst_vec_push={} bst_vec_len={} bst_vec_get={} bst_release={}",
        helper.export_memory,
        helper.export_str_ptr,
        helper.export_str_len,
        helper.export_vec_new,
        helper.export_vec_push,
        helper.export_vec_len,
        helper.export_vec_get,
        helper.export_release
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

#[cfg(test)]
#[path = "tests/artifacts_tests.rs"]
mod tests;
