//! Pre-lowering validation that a HIR module only references external functions supported by the
//! target backend.
//!
//! WHAT: validates external calls reachable from the module entry point and checks whether the
//! referenced functions have backend-specific lowering metadata.
//! WHY: backends should fail early with a structured user-facing diagnostic rather than
//! panicking or emitting a vague lowering error deep in backend code.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalPackageRegistry};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::reachability::{
    ReachableExternalCall, collect_reachability_from_start,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Backend target for external-package support validation.
pub enum BackendTarget {
    Js,
    Wasm,
}

pub enum ExternalPackageValidationError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl BackendTarget {
    fn as_str(&self) -> &'static str {
        match self {
            BackendTarget::Js => "JavaScript",
            BackendTarget::Wasm => "Wasm",
        }
    }
}

/// Validates that every reachable external function call in `hir` has lowering metadata for
/// `target`.
///
/// WHAT: consumes backend-neutral HIR reachability, then checks each reachable external call
/// against backend-specific lowering support.
/// WHY: moving this check before backend lowering lets us report a clear `Rule` error at the
/// reachable call site instead of a backend-internal `LirTransformation` or `WasmGeneration`
/// error. Unused source-library wrappers stay type-checked HIR, but they are not executable page
/// code and must not fail backend support validation.
pub fn validate_hir_external_package_support(
    hir: &HirModule,
    registry: &ExternalPackageRegistry,
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), ExternalPackageValidationError> {
    let reachability = collect_reachability_from_start(hir)
        .map_err(|error| ExternalPackageValidationError::Infrastructure(Box::new(error)))?;

    for call in &reachability.reachable_external_calls {
        if !has_backend_lowering(registry, call.function_id, &target) {
            let diagnostic =
                unsupported_external_function_diagnostic(registry, call, &target, string_table);

            return Err(ExternalPackageValidationError::Diagnostic(Box::new(
                diagnostic,
            )));
        }
    }

    Ok(())
}

fn has_backend_lowering(
    registry: &ExternalPackageRegistry,
    id: ExternalFunctionId,
    target: &BackendTarget,
) -> bool {
    match target {
        BackendTarget::Js => registry
            .get_function_by_id(id)
            .and_then(|def| def.lowerings.js.as_ref())
            .is_some_and(|lowering| {
                matches!(
                    lowering,
                    crate::compiler_frontend::external_packages::ExternalJsLowering::RuntimeFunction(_)
                        | crate::compiler_frontend::external_packages::ExternalJsLowering::InlineExpression(_)
                        | crate::compiler_frontend::external_packages::ExternalJsLowering::ExternalModuleExport { .. }
                )
            }),
        BackendTarget::Wasm => {
            // The Wasm backend hardcodes support for `Io` even though the registry lists
            // `lowerings.wasm` as `None`. Keep that parity here.
            if matches!(id, ExternalFunctionId::Io) {
                return true;
            }
            registry
                .get_function_by_id(id)
                .is_some_and(|def| def.lowerings.wasm.is_some())
        }
    }
}

fn unsupported_external_function_diagnostic(
    registry: &ExternalPackageRegistry,
    call: &ReachableExternalCall,
    target: &BackendTarget,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let function_name = registry
        .get_function_by_id(call.function_id)
        .map(|def| def.name.clone())
        .unwrap_or_else(|| call.function_id.name().to_owned());

    let package_path = registry.resolve_function_package(call.function_id);

    CompilerDiagnostic::unsupported_external_function(
        string_table.intern(&function_name),
        package_path.map(|path| string_table.intern(path)),
        string_table.intern(target.as_str()),
        call.location.clone(),
    )
}
