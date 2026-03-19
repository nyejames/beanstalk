//! Static UTF-8 data pooling for LIR lowering.

use crate::backends::wasm::hir_to_lir::context::WasmLirLoweringContext;
use crate::backends::wasm::lir::module::{WasmStaticData, WasmStaticDataKind};
use crate::backends::wasm::lir::types::WasmStaticDataId;

pub(crate) fn intern_static_utf8(
    context: &mut WasmLirLoweringContext<'_>,
    text: &str,
    debug_name: &str,
) -> WasmStaticDataId {
    // WHAT: deduplicate static UTF-8 segments by raw bytes.
    // WHY: keeps memory plan deterministic and avoids duplicate string payloads.
    let bytes = text.as_bytes().to_vec();

    if let Some(existing) = context.static_string_pool.get(&bytes).copied() {
        return existing;
    }

    let id = WasmStaticDataId(context.lir_module.static_data.len() as u32);
    context.static_string_pool.insert(bytes.clone(), id);
    context.lir_module.static_data.push(WasmStaticData {
        id,
        debug_name: debug_name.to_owned(),
        bytes,
        kind: WasmStaticDataKind::Utf8StringBytes,
    });

    id
}
