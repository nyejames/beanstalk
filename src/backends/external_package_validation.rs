//! Pre-lowering validation that a HIR module only references external functions supported by the
//! target backend.
//!
//! WHAT: scans every HIR block for `CallTarget::ExternalFunction` and checks whether the
//! referenced function has backend-specific lowering metadata.
//! WHY: backends should fail early with a structured user-facing diagnostic rather than
//! panicking or emitting a vague lowering error deep in backend code.

use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalPackageRegistry};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Backend target for external-package support validation.
pub enum BackendTarget {
    Js,
    Wasm,
}

impl BackendTarget {
    fn as_str(&self) -> &'static str {
        match self {
            BackendTarget::Js => "JavaScript",
            BackendTarget::Wasm => "Wasm",
        }
    }
}

/// Validates that every external function call in `hir` has lowering metadata for `target`.
///
/// WHAT: iterates all HIR blocks and statements, finds `Call` statements targeting external
/// functions, and checks the registry for backend-specific lowering support.
/// WHY: moving this check before backend lowering lets us report a clear `Rule` error at the
/// call site instead of a backend-internal `LirTransformation` or `WasmGeneration` error.
pub fn validate_hir_external_package_support(
    hir: &HirModule,
    registry: &ExternalPackageRegistry,
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    for block in &hir.blocks {
        for statement in &block.statements {
            if let HirStatementKind::Call {
                target:
                    crate::compiler_frontend::external_packages::CallTarget::ExternalFunction(id),
                ..
            } = &statement.kind
                && !has_backend_lowering(registry, *id, &target)
            {
                return Err(unsupported_external_function_diagnostic(
                    registry,
                    *id,
                    &target,
                    statement,
                    string_table,
                ));
            }
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
    id: ExternalFunctionId,
    target: &BackendTarget,
    statement: &HirStatement,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let function_name = registry
        .get_function_by_id(id)
        .map(|def| def.name.clone())
        .unwrap_or_else(|| id.name().to_owned());

    let package_path = registry.resolve_function_package(id);

    CompilerDiagnostic::unsupported_external_function(
        string_table.intern(&function_name),
        package_path.map(|path| string_table.intern(path)),
        string_table.intern(target.as_str()),
        statement.location.clone(),
    )
}
