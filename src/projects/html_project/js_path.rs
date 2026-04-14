//! HTML builder JavaScript-only rendering path.
//!
//! WHAT: owns HIR -> JS lowering and inline HTML assembly for the JS-only build path.
//! WHY: keeping this path isolated lets the HTML builder add a Wasm mode without
//! blending two output strategies into one large module.
//!
//! JS-only HTML lifecycle contract (in emission order):
//!   1. Static entry fragments are emitted as raw HTML in source order.
//!   2. Runtime fragment slots are emitted as `<div id="bst-slot-N">` placeholders.
//!   3. The compiled JS bundle is embedded in an inline `<script>` block.
//!      The bundle content is escaped so it cannot contain a raw `</script>` sequence
//!      that would prematurely close the script tag.
//!   4. A second inline `<script>` hydrates runtime slots in source order via
//!      `insertAdjacentHTML`, then calls `start()` after all slots are filled.

use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::build_system::build::{FileKind, OutputFile, ResolvedConstFragment};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirModule};
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::document_shell::render_html_document_shell;
use crate::projects::html_project::page_metadata::extract_html_page_metadata;
use std::collections::HashMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RuntimeSlotMount {
    /// Deterministic DOM element ID used as the runtime fragment mount point.
    pub slot_id: String,
    /// HIR function used to render the runtime fragment for this slot.
    pub function_id: FunctionId,
}

/// Compiles one module through the JS-only HTML builder path.
///
/// WHAT: lowers HIR to JS and embeds the JS with runtime slot hydration into HTML.
/// WHY: this preserves existing builder behavior when `--html-wasm` is not enabled.
pub(crate) fn compile_html_module_js(
    hir_module: &HirModule,
    const_fragments: &[ResolvedConstFragment],
    borrow_analysis: &BorrowCheckReport,
    string_table: &StringTable,
    output_path: PathBuf,
    project_name: &str,
    document_config: &HtmlDocumentConfig,
    release_build: bool,
) -> Result<OutputFile, CompilerError> {
    let js_lowering_config = JsLoweringConfig::standard_html(release_build);

    let js_module = lower_hir_to_js(
        hir_module,
        borrow_analysis,
        string_table,
        js_lowering_config,
    )?;
    let html = render_html_document(
        hir_module,
        const_fragments,
        string_table,
        document_config,
        &output_path,
        project_name,
        &js_module.source,
        &js_module.function_name_by_id,
    )?;

    Ok(OutputFile::new(output_path, FileKind::Html(html)))
}

/// Renders entry-file start fragments into HTML markup and runtime slot mounts.
///
/// WHAT: merges const fragments (with runtime insertion indices) and runtime fragment functions
/// from entry start() into source-order HTML and slot mount list.
/// WHY: runtime fragment hydration order is part of builder-visible semantics.
pub(crate) fn render_entry_fragments(
    hir_module: &HirModule,
    const_fragments: &[ResolvedConstFragment],
) -> (String, Vec<RuntimeSlotMount>) {
    let mut html = String::new();
    let mut runtime_slots = Vec::new();

    // WHAT: merge const fragments (with insertion index) and runtime fragment functions.
    // WHY: source order requires interleaving const strings at their indexed positions
    //      relative to runtime slots.

    let runtime_fns = &hir_module.entry_runtime_fragment_functions;
    let mut runtime_index = 0usize;

    // Sort const fragments by runtime_insertion_index to handle them in order.
    let mut sorted_const: Vec<(usize, &str)> = const_fragments
        .iter()
        .map(|f| (f.runtime_insertion_index, f.html.as_str()))
        .collect();
    sorted_const.sort_by_key(|(idx, _)| *idx);

    let mut const_iter = sorted_const.iter().peekable();

    // Emit const fragments with insertion_index == 0 (before any runtime slots).
    while const_iter
        .peek()
        .map(|(idx, _)| *idx == runtime_index)
        .unwrap_or(false)
    {
        let (_, html_str) = const_iter.next().unwrap();
        html.push_str(html_str);
        html.push('\n');
    }

    // Interleave runtime slots and const fragments.
    for &function_id in runtime_fns {
        let slot_id = format!("bst-slot-{runtime_index}");
        html.push_str(&format!("<div id=\"{slot_id}\"></div>\n"));
        runtime_slots.push(RuntimeSlotMount {
            slot_id,
            function_id,
        });
        runtime_index += 1;

        // Emit any const fragments whose insertion_index matches this runtime slot position.
        while const_iter
            .peek()
            .map(|(idx, _)| *idx == runtime_index)
            .unwrap_or(false)
        {
            let (_, html_str) = const_iter.next().unwrap();
            html.push_str(html_str);
            html.push('\n');
        }
    }

    // Emit any remaining const fragments after all runtime slots.
    for (_, html_str) in const_iter {
        html.push_str(html_str);
        html.push('\n');
    }

    (html, runtime_slots)
}

pub(crate) fn render_html_document(
    hir_module: &HirModule,
    const_fragments: &[ResolvedConstFragment],
    string_table: &StringTable,
    document_config: &HtmlDocumentConfig,
    logical_html_path: &Path,
    project_name: &str,
    js_bundle: &str,
    function_names: &HashMap<FunctionId, String>,
) -> Result<String, CompilerError> {
    // Build static body content and runtime slot list first so hydration preserves source order.
    let (body_html, runtime_slots) = render_entry_fragments(hir_module, const_fragments);
    let page_metadata = extract_html_page_metadata(hir_module, string_table)?;
    let script_html = render_runtime_bootstrap_script_html(
        hir_module,
        js_bundle,
        function_names,
        &runtime_slots,
    )?;

    Ok(render_html_document_shell(
        document_config,
        &page_metadata,
        logical_html_path,
        project_name,
        body_html,
        script_html,
    ))
}

fn render_runtime_bootstrap_script_html(
    hir_module: &HirModule,
    js_bundle: &str,
    function_names: &HashMap<FunctionId, String>,
    runtime_slots: &[RuntimeSlotMount],
) -> Result<String, CompilerError> {
    let mut html = String::new();

    // Escape the bundle so any `</script>` sequence inside string literals or comments cannot
    // prematurely terminate the HTML script tag and corrupt the page.
    let safe_bundle = escape_inline_script(js_bundle);
    html.push_str("<script>\n");
    html.push_str(&safe_bundle);
    html.push_str("\n</script>\n");
    html.push_str("<script>\n");
    html.push_str("(function () {\n");
    html.push_str("  const slots = [\n");
    for runtime_slot in runtime_slots {
        let Some(function_name) = function_names.get(&runtime_slot.function_id) else {
            return Err(CompilerError::compiler_error(format!(
                "HTML builder could not resolve runtime fragment function {:?}",
                runtime_slot.function_id
            )));
        };
        let _ = writeln!(
            html,
            "    [\"{}\", {}],",
            runtime_slot.slot_id, function_name
        );
    }
    html.push_str("  ];\n\n");
    // Hydrate runtime fragments in source order before running start().
    html.push_str("  for (const [id, fn] of slots) {\n");
    html.push_str("    const el = document.getElementById(id);\n");
    html.push_str("    if (!el) throw new Error(\"Missing runtime mount slot: \" + id);\n");
    html.push_str("    el.insertAdjacentHTML(\"beforeend\", fn());\n");
    html.push_str("  }\n\n");

    let Some(start_function_name) = function_names.get(&hir_module.start_function) else {
        return Err(CompilerError::compiler_error(format!(
            "HTML builder could not resolve start function {:?}",
            hir_module.start_function
        )));
    };

    let _ = writeln!(
        html,
        "  // start() remains the lifecycle entrypoint and runs after fragment hydration.\n  if (typeof {start_function_name} === \"function\") {start_function_name}();"
    );
    html.push_str("})();\n");
    html.push_str("</script>\n");

    Ok(html)
}

/// Derive the logical HTML output path for this entry file.
///
/// Delegates to the canonical output planner so JS-only and Wasm paths agree on route derivation.
pub(crate) fn html_output_path(
    entry_point: &Path,
    entry_root: Option<&Path>,
    string_table: &mut StringTable,
) -> Result<PathBuf, CompilerError> {
    use crate::projects::html_project::output_plan::derive_logical_html_path;
    derive_logical_html_path(entry_point, entry_root, string_table)
}

/// Escapes JS source so it is safe to embed inside an HTML `<script>` block.
///
/// WHAT: replaces every `</` occurrence with `<\/` so the HTML parser cannot see a closing tag
/// sequence inside the script content.
/// WHY: a raw `</script>` anywhere in an inlined JS bundle — including inside string literals or
/// comments — causes the browser to terminate the script tag early and corrupt the page.
/// `<\/` is a valid JS string escape sequence equivalent to `</`, so the JS semantics are
/// preserved while the HTML parser sees a harmless non-tag sequence.
pub(crate) fn escape_inline_script(js: &str) -> String {
    js.replace("</", "<\\/")
}

#[cfg(test)]
#[path = "tests/js_path_tests.rs"]
mod tests;
