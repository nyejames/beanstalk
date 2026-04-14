//! JS bootstrap generator for HTML+Wasm mode.
//!
//! WHAT: emits builder-owned JS that instantiates Wasm, wires host imports, hydrates
//! runtime slots from the fragment list returned by entry start(), and runs the lifecycle.
//! WHY: HTML assembly/orchestration remains builder policy while Wasm stays backend-generic.

use crate::compiler_frontend::compiler_errors::CompilerError;

/// Emits `page.js` for HTML Wasm mode.
///
/// WHAT: appends a Wasm bootstrap around lowered JS helpers and slot hydration.
/// WHY: entry start() is exported as "bst_start"; JS calls it directly and uses the
///      returned fragment list to hydrate slots. No per-function wrapper bindings needed.
pub(crate) fn generate_wasm_bootstrap_js(
    js_bundle: &str,
    slot_ids: &[String],
    start_invocation_js: &str,
) -> Result<String, CompilerError> {
    let mut out = String::new();
    out.push_str(js_bundle);
    out.push('\n');
    out.push('\n');
    out.push_str("const __bst_decoder = new TextDecoder(\"utf-8\");\n");
    out.push_str("const __bst_dom_registry = new Map();\n");
    out.push_str("let __bst_next_dom_handle = 1;\n");
    out.push('\n');
    out.push_str("function __bst_register_dom_node(node) {\n");
    out.push_str("  const handle = __bst_next_dom_handle;\n");
    out.push_str("  __bst_next_dom_handle += 1;\n");
    out.push_str("  __bst_dom_registry.set(handle, node);\n");
    out.push_str("  return handle;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("function __bst_lookup_dom_node(handle) {\n");
    out.push_str("  const node = __bst_dom_registry.get(handle);\n");
    out.push_str(
        "  if (!node) throw new Error(\"Unknown DOM node handle from Wasm host call: \" + handle);\n",
    );
    out.push_str("  return node;\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("function __bst_read_string(instance, handle) {\n");
    out.push_str("  if (handle === 0 || handle === undefined || handle === null) return \"\";\n");
    out.push_str("  const ptr = instance.exports.bst_str_ptr(handle);\n");
    out.push_str("  const len = instance.exports.bst_str_len(handle);\n");
    out.push_str("  const bytes = new Uint8Array(instance.exports.memory.buffer, ptr, len);\n");
    out.push_str("  return __bst_decoder.decode(bytes);\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("function __bst_take_string(instance, handle) {\n");
    out.push_str("  if (handle === 0 || handle === undefined || handle === null) return \"\";\n");
    out.push_str("  try {\n");
    out.push_str("    return __bst_read_string(instance, handle);\n");
    out.push_str("  } finally {\n");
    out.push_str("    instance.exports.bst_release(handle);\n");
    out.push_str("  }\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("function __bst_build_imports(instance_ref) {\n");
    out.push_str("  return {\n");
    out.push_str("    host: {\n");
    out.push_str("      log_string(handle) {\n");
    out.push_str(
        "        const text = __bst_take_string(instance_ref.current, handle);\n        console.log(text);\n",
    );
    out.push_str("      },\n");
    out.push_str("      dom_create_text(handle) {\n");
    out.push_str(
        "        const text = __bst_take_string(instance_ref.current, handle);\n        return __bst_register_dom_node(document.createTextNode(text));\n",
    );
    out.push_str("      },\n");
    out.push_str("      dom_set_text(node_handle, text_handle) {\n");
    out.push_str(
        "        const node = __bst_lookup_dom_node(node_handle);\n        node.textContent = __bst_take_string(instance_ref.current, text_handle);\n",
    );
    out.push_str("      },\n");
    out.push_str("      dom_set_html(node_handle, html_handle) {\n");
    out.push_str(
        "        const node = __bst_lookup_dom_node(node_handle);\n        node.innerHTML = __bst_take_string(instance_ref.current, html_handle);\n",
    );
    out.push_str("      },\n");
    out.push_str("    },\n");
    out.push_str("  };\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("async function __bst_instantiate_wasm(wasm_url, imports) {\n");
    out.push_str("  if (typeof WebAssembly.instantiateStreaming === \"function\") {\n");
    out.push_str("    try {\n");
    out.push_str(
        "      return await WebAssembly.instantiateStreaming(fetch(wasm_url), imports);\n",
    );
    out.push_str("    } catch (_error) {\n");
    out.push_str(
        "      // Fall back when streaming compilation is unavailable (for example MIME setup).\n",
    );
    out.push_str("    }\n");
    out.push_str("  }\n");
    out.push_str(
        "  const bytes = await fetch(wasm_url).then((response) => response.arrayBuffer());\n",
    );
    out.push_str("  return WebAssembly.instantiate(bytes, imports);\n");
    out.push_str("}\n");
    out.push('\n');
    out.push_str("(async function () {\n");
    out.push_str("  const instance_ref = { current: null };\n");
    out.push_str("  const imports = __bst_build_imports(instance_ref);\n");
    out.push_str(
        "  const { instance } = await __bst_instantiate_wasm(\"./page.wasm\", imports);\n",
    );
    out.push_str("  instance_ref.current = instance;\n");
    out.push('\n');

    if slot_ids.is_empty() {
        // No runtime slots — call bst_start() directly for any lifecycle effects.
        out.push_str("  ");
        out.push_str(start_invocation_js);
        out.push('\n');
    } else {
        // WHAT: call bst_start() and decode the returned runtime fragment list to hydrate slots.
        // WHY: entry start() is the sole runtime fragment producer; builders call it once and
        //      use the returned Vec<String> elements to fill source-order slot placeholders.
        // TODO: Vec<String> decoding from a Wasm handle requires Wasm Vec support.
        //       See lower_push_runtime_fragment TODO in backends/wasm/hir_to_lir/stmt.rs.
        //       Until that is implemented, programs with runtime templates fail to compile
        //       in Wasm mode. The slot structure below is the correct target shape.
        out.push_str("  const bst_slot_ids = [\n");
        for slot_id in slot_ids {
            out.push_str(&format!("    \"{slot_id}\",\n"));
        }
        out.push_str("  ];\n");
        out.push_str("  ");
        out.push_str(start_invocation_js);
        out.push('\n');
        out.push_str(
            "  // TODO: decode Vec<String> from bst_start() return and hydrate bst_slot_ids.\n",
        );
    }

    out.push_str("})().catch((error) => {\n");
    out.push_str("  console.error(\"Beanstalk Wasm bootstrap failed\", error);\n");
    out.push_str("  throw error;\n");
    out.push_str("});\n");

    Ok(out)
}
