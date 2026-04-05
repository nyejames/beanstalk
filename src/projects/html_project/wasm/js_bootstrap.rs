//! JS bootstrap generator for HTML+Wasm mode.
//!
//! WHAT: emits builder-owned JS that instantiates Wasm, wires host imports, installs
//! wrapper functions, hydrates runtime slots, and then runs the entry start function.
//! WHY: HTML assembly/orchestration remains builder policy while Wasm stays backend-generic.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_datatypes::HirTypeKind;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirModule};
use crate::projects::html_project::js_path::RuntimeSlotMount;
use crate::projects::html_project::wasm::export_plan::HtmlWasmExportPlan;
use crate::projects::html_project::wasm::request::export_name_by_function_id;
use std::collections::HashMap;
use std::fmt::Write as _;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WasmWrapperReturnBehavior {
    Unit,
    StringHandle,
    Passthrough,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct WasmWrapperBinding {
    js_function_name: String,
    export_name: String,
    return_behavior: WasmWrapperReturnBehavior,
}

/// Emits `page.js` for HTML Wasm mode.
///
/// WHAT: appends a Wasm bootstrap around lowered JS helpers, wrapper bindings, and slot hydration.
/// WHY: keeps entry/start orchestration builder-owned while Wasm backend exports stay generic.
pub(crate) fn generate_wasm_bootstrap_js(
    hir_module: &HirModule,
    js_bundle: &str,
    function_name_by_id: &HashMap<FunctionId, String>,
    export_plan: &HtmlWasmExportPlan,
    runtime_slots: &[RuntimeSlotMount],
    start_invocation_js: &str,
) -> Result<String, CompilerError> {
    // Build wrapper metadata first so codegen can emit stable wrapper declarations.
    let export_name_map = export_name_by_function_id(export_plan);
    let wrapper_bindings = build_wrapper_bindings(
        hir_module,
        function_name_by_id,
        export_plan,
        &export_name_map,
    )?;

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
    out.push_str("function __bst_lift_call_arg(arg) {\n");
    out.push_str("  const raw = __bs_is_ref(arg) ? __bs_read(arg) : arg;\n");
    out.push_str("  if (typeof raw === \"string\") {\n");
    out.push_str(
        "    throw new Error(\"Wasm wrapper calls from JS do not support string arguments yet.\");\n",
    );
    out.push_str("  }\n");
    out.push_str("  return raw;\n");
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
    out.push_str("function __bst_install_wasm_wrappers(instance) {\n");
    out.push_str("  function __bst_get_export(name) {\n");
    out.push_str("    const exported = instance.exports[name];\n");
    out.push_str(
        "    if (typeof exported !== \"function\") throw new Error(\"Missing Wasm export: \" + name);\n",
    );
    out.push_str("    return exported;\n");
    out.push_str("  }\n");
    out.push('\n');

    for wrapper_binding in &wrapper_bindings {
        let export_name_literal = escape_js_string(&wrapper_binding.export_name);
        match wrapper_binding.return_behavior {
            WasmWrapperReturnBehavior::Unit => {
                let _ = writeln!(
                    out,
                    "  {} = (...args) => {{\n    __bst_get_export({})(...args.map(__bst_lift_call_arg));\n    return;\n  }};",
                    wrapper_binding.js_function_name, export_name_literal
                );
            }
            WasmWrapperReturnBehavior::StringHandle => {
                let _ = writeln!(
                    out,
                    "  {} = (...args) => {{\n    const result = __bst_get_export({})(...args.map(__bst_lift_call_arg));\n    return __bst_take_string(instance, result);\n  }};",
                    wrapper_binding.js_function_name, export_name_literal
                );
            }
            WasmWrapperReturnBehavior::Passthrough => {
                let _ = writeln!(
                    out,
                    "  {} = (...args) => __bst_get_export({})(...args.map(__bst_lift_call_arg));",
                    wrapper_binding.js_function_name, export_name_literal
                );
            }
        }
    }

    out.push_str("}\n");
    out.push('\n');
    out.push_str("(async function () {\n");
    out.push_str("  const instance_ref = { current: null };\n");
    out.push_str("  const imports = __bst_build_imports(instance_ref);\n");
    out.push_str(
        "  const { instance } = await __bst_instantiate_wasm(\"./page.wasm\", imports);\n",
    );
    out.push_str("  instance_ref.current = instance;\n");
    out.push_str("  __bst_install_wasm_wrappers(instance);\n");
    out.push('\n');
    out.push_str("  const slots = [\n");
    for slot in runtime_slots {
        let function_name = function_name_by_id
            .get(&slot.function_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "HTML Wasm bootstrap generation could not resolve runtime fragment function {:?}",
                    slot.function_id
                ))
            })?
            .clone();
        let _ = writeln!(
            out,
            "    [{}, {}],",
            escape_js_string(&slot.slot_id),
            function_name
        );
    }
    out.push_str("  ];\n");
    out.push('\n');
    out.push_str("  for (const [id, fn] of slots) {\n");
    out.push_str("    const el = document.getElementById(id);\n");
    out.push_str("    if (!el) throw new Error(\"Missing runtime mount slot: \" + id);\n");
    out.push_str("    el.insertAdjacentHTML(\"beforeend\", fn());\n");
    out.push_str("  }\n");
    out.push('\n');
    out.push_str("  ");
    out.push_str(start_invocation_js);
    out.push('\n');
    out.push_str("})().catch((error) => {\n");
    out.push_str("  console.error(\"Beanstalk Wasm bootstrap failed\", error);\n");
    out.push_str("  throw error;\n");
    out.push_str("});\n");

    Ok(out)
}

/// Resolves JS function names, assigned Wasm export names, and return handling policy.
///
/// WHAT: prepares all wrapper metadata before final JS emission.
/// WHY: keeps mapping/validation logic separate from string-based code generation.
fn build_wrapper_bindings(
    hir_module: &HirModule,
    function_name_by_id: &HashMap<FunctionId, String>,
    export_plan: &HtmlWasmExportPlan,
    export_name_map: &rustc_hash::FxHashMap<FunctionId, String>,
) -> Result<Vec<WasmWrapperBinding>, CompilerError> {
    let mut bindings = Vec::with_capacity(export_plan.function_exports.len());
    for function_export in &export_plan.function_exports {
        let js_function_name = function_name_by_id
            .get(&function_export.function_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "HTML Wasm bootstrap generation could not resolve JS function name for {:?}",
                    function_export.function_id
                ))
            })?
            .clone();
        let export_name = export_name_map
            .get(&function_export.function_id)
            .ok_or_else(|| {
                CompilerError::compiler_error(format!(
                    "HTML Wasm bootstrap generation could not resolve export name for {:?}",
                    function_export.function_id
                ))
            })?
            .clone();
        let return_behavior =
            return_behavior_for_function(hir_module, function_export.function_id)?;

        bindings.push(WasmWrapperBinding {
            js_function_name,
            export_name,
            return_behavior,
        });
    }

    Ok(bindings)
}

/// Determines how wrapper glue should adapt Wasm return values back to JS semantics.
///
/// WHAT: inspects HIR return type for each exported callable.
/// WHY: string-returning functions need handle -> JS string conversion and release.
fn return_behavior_for_function(
    hir_module: &HirModule,
    function_id: FunctionId,
) -> Result<WasmWrapperReturnBehavior, CompilerError> {
    let function = hir_module
        .functions
        .iter()
        .find(|function| function.id == function_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HTML Wasm bootstrap generation missing function metadata for {function_id:?}",
            ))
        })?;
    let return_type = hir_module.type_context.get(function.return_type);

    Ok(match return_type.kind {
        HirTypeKind::Unit => WasmWrapperReturnBehavior::Unit,
        HirTypeKind::String => WasmWrapperReturnBehavior::StringHandle,
        _ => WasmWrapperReturnBehavior::Passthrough,
    })
}

/// Escapes a Rust string into a safe JS string literal.
///
/// WHAT: applies escaping for quotes, control characters, and backslashes.
/// WHY: generated bootstrap JS must remain syntactically valid for any route/slot/export name.
fn escape_js_string(value: &str) -> String {
    let mut escaped = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => escaped.push_str("\\\\"),
            '"' => escaped.push_str("\\\""),
            '\n' => escaped.push_str("\\n"),
            '\r' => escaped.push_str("\\r"),
            '\t' => escaped.push_str("\\t"),
            '\0' => escaped.push_str("\\0"),
            control if control.is_control() => {
                let _ = write!(escaped, "\\u{:04X}", control as u32);
            }
            normal => escaped.push(normal),
        }
    }
    escaped.push('"');
    escaped
}
