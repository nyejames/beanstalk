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
//!   4. A second inline `<script>` calls entry `start()` once. start() returns the
//!      runtime fragment array and each element is hydrated into its slot in source order.

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
use std::path::{Path, PathBuf};

/// Returns the number of runtime fragment slots owned by entry start().
///
/// WHAT: walks PushRuntimeFragment statements in the entry start() function body.
/// WHY: entry start() is the canonical source of runtime slot ordering; builders derive
///      slot counts from the HIR statement sequence rather than a parallel metadata field.
pub(crate) fn count_runtime_fragment_slots(hir_module: &HirModule) -> usize {
    use crate::compiler_frontend::hir::hir_nodes::HirStatementKind;
    use crate::compiler_frontend::hir::utils::terminator_targets;
    use rustc_hash::FxHashSet;
    use std::collections::VecDeque;

    let Some(start_fn) = hir_module
        .functions
        .iter()
        .find(|f| f.id == hir_module.start_function)
    else {
        return 0;
    };

    let mut count = 0;
    let mut visited: FxHashSet<_> = FxHashSet::default();
    let mut queue = VecDeque::new();
    queue.push_back(start_fn.entry);

    while let Some(block_id) = queue.pop_front() {
        if !visited.insert(block_id) {
            continue;
        }
        if let Some(block) = hir_module.blocks.iter().find(|b| b.id == block_id) {
            for stmt in &block.statements {
                if matches!(stmt.kind, HirStatementKind::PushRuntimeFragment { .. }) {
                    count += 1;
                }
            }
            for succ in terminator_targets(&block.terminator) {
                queue.push_back(succ);
            }
        }
    }
    count
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

/// Renders entry-file start fragments into static HTML and an ordered list of slot IDs.
///
/// WHAT: merges const fragments (with runtime insertion indices) and runtime slot placeholders
/// into source-order HTML. Returns slot IDs so the bootstrap script can hydrate them in order.
/// WHY: source order requires interleaving const strings at their indexed positions
///      relative to runtime slots. Slot count is supplied by the caller from HIR.
pub(crate) fn render_entry_fragments(
    const_fragments: &[ResolvedConstFragment],
    slot_count: usize,
) -> (String, Vec<String>) {
    let mut html = String::new();
    let mut slot_ids: Vec<String> = Vec::new();
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
    for _ in 0..slot_count {
        let slot_id = format!("bst-slot-{runtime_index}");
        html.push_str(&format!("<div id=\"{slot_id}\"></div>\n"));
        slot_ids.push(slot_id);
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

    (html, slot_ids)
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
    let slot_count = count_runtime_fragment_slots(hir_module);
    let (body_html, slot_ids) = render_entry_fragments(const_fragments, slot_count);
    let page_metadata = extract_html_page_metadata(hir_module, string_table)?;

    let Some(start_function_name) = function_names.get(&hir_module.start_function) else {
        return Err(CompilerError::compiler_error(format!(
            "HTML builder could not resolve start function {:?}",
            hir_module.start_function
        )));
    };

    let script_html =
        render_runtime_bootstrap_script_html(start_function_name, js_bundle, &slot_ids);

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
    start_function_name: &str,
    js_bundle: &str,
    slot_ids: &[String],
) -> String {
    // Escape the bundle so any `</script>` sequence inside string literals or comments cannot
    // prematurely terminate the HTML script tag and corrupt the page.
    let safe_bundle = escape_inline_script(js_bundle);
    let mut html = String::new();
    html.push_str("<script>\n");
    html.push_str(&safe_bundle);
    html.push_str("\n</script>\n");
    html.push_str("<script>\n");
    html.push_str("(function () {\n");

    if slot_ids.is_empty() {
        // No runtime fragments — call start() directly for any user-defined lifecycle effects.
        html.push_str(&format!(
            "  if (typeof {start_function_name} === \"function\") {start_function_name}();\n"
        ));
    } else {
        // WHAT: call entry start() once; it returns the runtime fragment array in source order.
        // WHY: start() accumulates fragments via PushRuntimeFragment and returns them as a JS
        //      array. Calling start() here both produces the fragments and runs the lifecycle.
        html.push_str(&format!(
            "  var bst_frags = {start_function_name}();\n"
        ));
        html.push_str("  var bst_slots = [\n");
        for slot_id in slot_ids {
            html.push_str(&format!("    \"{slot_id}\",\n"));
        }
        html.push_str("  ];\n");
        html.push_str("  for (var i = 0; i < bst_slots.length; i++) {\n");
        html.push_str("    var el = document.getElementById(bst_slots[i]);\n");
        html.push_str(
            "    if (!el) throw new Error(\"Missing runtime mount slot: \" + bst_slots[i]);\n",
        );
        html.push_str("    el.insertAdjacentHTML(\"beforeend\", bst_frags[i] || \"\");\n");
        html.push_str("  }\n");
    }

    html.push_str("})();\n");
    html.push_str("</script>\n");
    html
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
