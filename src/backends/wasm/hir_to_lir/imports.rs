//! Host import planning for HIR -> LIR lowering.

use crate::backends::wasm::hir_to_lir::context::WasmLirLoweringContext;
use crate::backends::wasm::lir::linkage::{WasmImport, WasmImportKind};
use crate::backends::wasm::lir::types::{WasmImportId, WasmLirSignature};
use crate::backends::wasm::runtime::imports::WasmHostFunction;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::HirStatementKind;
use crate::compiler_frontend::host_functions::CallTarget;

pub(crate) fn register_required_host_imports(
    context: &mut WasmLirLoweringContext<'_>,
) -> Result<(), CompilerError> {
    // WHAT: scan HIR for host calls and pre-register required imports.
    // WHY: deterministic import-id assignment for the whole module.
    //
    // Phase-1 note:
    // host import mapping is intentionally minimal and will be expanded.
    let mut needs_log = false;

    for block in &context.hir_module.blocks {
        for statement in &block.statements {
            if let HirStatementKind::Call { target, .. } = &statement.kind
                && matches!(target, CallTarget::HostFunction(_))
            {
                needs_log = true;
            }
        }
    }

    if needs_log {
        ensure_host_import(context, WasmHostFunction::LogString);
    }

    Ok(())
}

pub(crate) fn resolve_host_call_import(
    context: &mut WasmLirLoweringContext<'_>,
    _target: &CallTarget,
) -> Result<WasmImportId, CompilerError> {
    // Phase-1 TODO:
    // Map specific host call targets to distinct imports once host ABI lowering expands.
    Ok(ensure_host_import(context, WasmHostFunction::LogString))
}

fn ensure_host_import(
    context: &mut WasmLirLoweringContext<'_>,
    function: WasmHostFunction,
) -> WasmImportId {
    // Idempotent insert: same host function always maps to same import id.
    if let Some(import_id) = context.host_imports.get(&function).copied() {
        return import_id;
    }

    let import_id = WasmImportId(context.lir_module.imports.len() as u32);
    context.host_imports.insert(function, import_id);

    context.lir_module.imports.push(WasmImport {
        id: import_id,
        module_name: function.module_name().to_owned(),
        item_name: function.item_name().to_owned(),
        kind: WasmImportKind::Function(WasmLirSignature {
            params: vec![crate::backends::wasm::lir::types::WasmAbiType::Handle],
            results: vec![],
        }),
    });

    import_id
}
