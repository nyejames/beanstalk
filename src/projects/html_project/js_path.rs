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
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::ids::FunctionId;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::compile_input::HtmlModuleCompileInput;
use crate::projects::html_project::document_config::HtmlDocumentConfig;
use crate::projects::html_project::document_shell::render_html_document_shell;
use crate::projects::html_project::page_metadata::extract_html_page_metadata;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Inputs for rendering a JS-backed HTML document.
///
/// WHAT: groups all data needed to produce the final HTML document from a lowered JS module.
/// WHY: `render_html_document` previously took 9 separate parameters; this struct keeps the
///      call sites readable and the parameter list stable as fields are added or renamed.
pub(crate) struct HtmlDocumentRenderInput<'a> {
    pub hir_module: &'a HirModule,
    pub const_fragments: &'a [ResolvedConstFragment],
    pub string_table: &'a StringTable,
    pub document_config: &'a HtmlDocumentConfig,
    pub logical_html_path: &'a Path,
    pub project_name: &'a str,
    pub js_bundle: &'a str,
    pub function_names: &'a HashMap<FunctionId, String>,
    pub entry_runtime_fragment_count: usize,
}

/// Compiles one module through the JS-only HTML builder path.
///
/// WHAT: lowers HIR to JS and embeds the JS with runtime slot hydration into HTML.
/// WHY: this preserves existing builder behavior when `--html-wasm` is not enabled.
pub(crate) fn compile_html_module_js(
    input: &HtmlModuleCompileInput<'_>,
    string_table: &StringTable,
    output_path: PathBuf,
) -> Result<OutputFile, CompilerError> {
    let mut js_lowering_config = JsLoweringConfig::standard_html(input.release_build);
    js_lowering_config.external_package_registry = input.external_package_registry.clone();

    let js_module = lower_hir_to_js(
        input.hir_module,
        input.borrow_analysis,
        string_table,
        js_lowering_config,
    )?;
    let html = render_html_document(&HtmlDocumentRenderInput {
        hir_module: input.hir_module,
        const_fragments: input.const_fragments,
        string_table,
        document_config: input.document_config,
        logical_html_path: &output_path,
        project_name: input.project_name,
        js_bundle: &js_module.source,
        function_names: &js_module.function_name_by_id,
        entry_runtime_fragment_count: input.entry_runtime_fragment_count,
    })?;

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
        .map(|f| (f.runtime_insertion_index, f.rendered_text.as_str()))
        .collect();
    sorted_const.sort_by_key(|(idx, _)| *idx);

    let mut const_iter = sorted_const.iter().peekable();

    // Emit const fragments with insertion_index == 0 (before any runtime slots).
    while let Some((_, html_str)) = const_iter.next_if(|(idx, _)| *idx == runtime_index) {
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
        while let Some((_, html_str)) = const_iter.next_if(|(idx, _)| *idx == runtime_index) {
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
    input: &HtmlDocumentRenderInput<'_>,
) -> Result<String, CompilerError> {
    let (body_html, slot_ids) =
        render_entry_fragments(input.const_fragments, input.entry_runtime_fragment_count);
    let page_metadata = extract_html_page_metadata(input.hir_module, input.string_table)?;

    let Some(start_function_name) = input.function_names.get(&input.hir_module.start_function)
    else {
        return Err(CompilerError::compiler_error(format!(
            "HTML builder could not resolve start function {:?}",
            input.hir_module.start_function
        )));
    };

    let script_html =
        render_runtime_bootstrap_script_html(start_function_name, input.js_bundle, &slot_ids);

    Ok(render_html_document_shell(
        input.document_config,
        &page_metadata,
        input.logical_html_path,
        input.project_name,
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
        html.push_str(&format!("  var bst_frags = {start_function_name}();\n"));
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
