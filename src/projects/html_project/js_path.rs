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
use crate::build_system::build::{FileKind, OutputFile};
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirModule, StartFragment};
use crate::compiler_frontend::string_interning::StringTable;
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
    borrow_analysis: &BorrowCheckReport,
    string_table: &StringTable,
    output_path: PathBuf,
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
        &js_module.source,
        &js_module.function_name_by_id,
    )?;

    Ok(OutputFile::new(output_path, FileKind::Html(html)))
}

/// Renders entry-file start fragments into HTML markup and runtime slot mounts.
///
/// WHAT: this preserves source order and creates deterministic slot IDs.
/// WHY: runtime fragment hydration order is part of builder-visible semantics.
pub(crate) fn render_entry_fragments(
    hir_module: &HirModule,
) -> Result<(String, Vec<RuntimeSlotMount>), CompilerError> {
    let mut html = String::new();
    let mut runtime_slots = Vec::new();
    let mut runtime_index = 0usize;

    for fragment in &hir_module.start_fragments {
        match fragment {
            StartFragment::ConstString(const_string_id) => {
                let string_index = const_string_id.0 as usize;
                let Some(const_string) = hir_module.const_string_pool.get(string_index) else {
                    return Err(CompilerError::compiler_error(format!(
                        "HTML builder could not resolve const fragment {}",
                        const_string_id.0
                    )));
                };
                // The HTML builder interprets const fragment strings as raw HTML.
                html.push_str(const_string);
                html.push('\n');
            }

            StartFragment::RuntimeStringFn(function_id) => {
                let slot_id = format!("bst-slot-{runtime_index}");
                runtime_index += 1;
                html.push_str(&format!("<div id=\"{slot_id}\"></div>\n"));
                runtime_slots.push(RuntimeSlotMount {
                    slot_id,
                    function_id: *function_id,
                });
            }
        }
    }

    Ok((html, runtime_slots))
}

pub(crate) fn render_html_document(
    hir_module: &HirModule,
    js_bundle: &str,
    function_names: &HashMap<FunctionId, String>,
) -> Result<String, CompilerError> {
    // Build static HTML and runtime slot list first so hydration preserves source ordering.
    let (mut html, runtime_slots) = render_entry_fragments(hir_module)?;

    // Escape the bundle so any `</script>` sequence inside string literals or comments cannot
    // prematurely terminate the HTML script tag and corrupt the page.
    let safe_bundle = escape_inline_script(js_bundle);
    html.push_str("<script>\n");
    html.push_str(&safe_bundle);
    html.push_str("\n</script>\n");
    html.push_str("<script>\n");
    html.push_str("(function () {\n");
    html.push_str("  const slots = [\n");
    for runtime_slot in &runtime_slots {
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
) -> Result<PathBuf, CompilerError> {
    use crate::projects::html_project::output_plan::derive_logical_html_path;
    derive_logical_html_path(entry_point, entry_root)
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
