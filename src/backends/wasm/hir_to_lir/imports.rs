//! Host import planning for HIR -> LIR lowering.

use crate::backends::wasm::hir_to_lir::context::WasmLirLoweringContext;
use crate::backends::wasm::lir::linkage::{WasmImport, WasmImportKind};
use crate::backends::wasm::lir::types::{WasmAbiType, WasmImportId, WasmLirSignature};
use crate::backends::wasm::runtime::imports::WasmHostFunction;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::HirStatementKind;
use crate::compiler_frontend::host_functions::CallTarget;

pub(crate) fn register_required_host_imports(
    context: &mut WasmLirLoweringContext<'_>,
) -> Result<(), CompilerError> {
    // WHAT: scan HIR for host calls and pre-register required imports.
    // WHY: deterministic import-id assignment for the whole module.
    for block in &context.hir_module.blocks {
        for statement in &block.statements {
            if let HirStatementKind::Call {
                target: CallTarget::HostFunction(path),
                ..
            } = &statement.kind
            {
                let host_function = resolve_host_function_name(context, path)?;
                ensure_host_import(context, host_function);
            }
        }
    }

    Ok(())
}

pub(crate) fn resolve_host_call_import(
    context: &mut WasmLirLoweringContext<'_>,
    target: &CallTarget,
) -> Result<WasmImportId, CompilerError> {
    // WHAT: resolve a host call target to its pre-registered import id.
    // WHY: each distinct host function maps to exactly one import; unsupported targets
    // must fail with a structured diagnostic instead of silently mapping to the wrong import.
    let CallTarget::HostFunction(path) = target else {
        return Err(CompilerError::lir_transformation(
            "Wasm lowering expected a HostFunction call target in resolve_host_call_import",
        ));
    };

    let host_function = resolve_host_function_name(context, path)?;
    Ok(ensure_host_import(context, host_function))
}

fn resolve_host_function_name(
    context: &WasmLirLoweringContext<'_>,
    path: &crate::compiler_frontend::interned_path::InternedPath,
) -> Result<WasmHostFunction, CompilerError> {
    // WHAT: map a host function path to its Wasm backend import identity.
    // WHY: ensures only explicitly supported host calls are lowered.
    let Some(name) = path.name_str(context.string_table) else {
        return Err(CompilerError::lir_transformation(
            "Wasm lowering could not resolve host function path to a name",
        ));
    };

    match name {
        "io" => Ok(WasmHostFunction::LogString),
        _ => Err(CompilerError::lir_transformation(format!(
            "Wasm backend does not yet support host function '{name}'"
        ))),
    }
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

    // WHAT: import signature is determined by the host function identity.
    // WHY: each host function has a fixed ABI contract.
    let signature = host_function_signature(function);
    context.lir_module.imports.push(WasmImport {
        id: import_id,
        module_name: function.module_name().to_owned(),
        item_name: function.item_name().to_owned(),
        kind: WasmImportKind::Function(signature),
    });

    import_id
}

fn host_function_signature(function: WasmHostFunction) -> WasmLirSignature {
    // WHAT: canonical ABI signature for each supported host function.
    // WHY: keeps import registration and signature assignment in one explicit place.
    match function {
        WasmHostFunction::LogString => WasmLirSignature {
            params: vec![WasmAbiType::Handle],
            results: vec![],
        },
        WasmHostFunction::DomCreateText
        | WasmHostFunction::DomSetText
        | WasmHostFunction::DomSetHtml => WasmLirSignature {
            // Placeholder signatures for upcoming DOM integration.
            params: vec![WasmAbiType::Handle],
            results: vec![],
        },
    }
}
