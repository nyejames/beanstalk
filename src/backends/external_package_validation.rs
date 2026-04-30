//! Pre-lowering validation that a HIR module only references external functions supported by the
//! target backend.
//!
//! WHAT: scans every HIR block for `CallTarget::ExternalFunction` and checks whether the
//! referenced function has backend-specific lowering metadata.
//! WHY: backends should fail early with a structured user-facing diagnostic rather than
//! panicking or emitting a vague lowering error deep in backend code.

use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorMetaDataKey, ErrorType};
use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalPackageRegistry};
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::statements::{HirStatement, HirStatementKind};

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
) -> Result<(), CompilerError> {
    for block in &hir.blocks {
        for statement in &block.statements {
            if let HirStatementKind::Call {
                target:
                    crate::compiler_frontend::external_packages::CallTarget::ExternalFunction(id),
                ..
            } = &statement.kind
                && !has_backend_lowering(registry, *id, &target)
            {
                return Err(unsupported_external_function_error(
                    registry, *id, &target, statement,
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
            .is_some_and(|def| def.lowerings.js.is_some()),
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

fn unsupported_external_function_error(
    registry: &ExternalPackageRegistry,
    id: ExternalFunctionId,
    target: &BackendTarget,
    statement: &HirStatement,
) -> CompilerError {
    let function_name = registry
        .get_function_by_id(id)
        .map(|def| def.name)
        .unwrap_or_else(|| id.name());

    let package = registry.resolve_function_package(id);

    let message = if let Some(package_path) = package {
        format!(
            "External function '{function_name}' from package '{package_path}' is not supported by the {} backend.",
            target.as_str()
        )
    } else {
        format!(
            "External function '{function_name}' is not supported by the {} backend.",
            target.as_str()
        )
    };

    let mut error = CompilerError::new_rule_error(message, statement.location.clone());
    error.metadata.insert(
        ErrorMetaDataKey::CompilationStage,
        "Backend Validation".to_owned(),
    );
    error.metadata.insert(
        ErrorMetaDataKey::PrimarySuggestion,
        "Use a builder that supports this external function, or switch to a backend that provides it (e.g., JavaScript).".to_owned(),
    );
    error.with_error_type(ErrorType::Rule)
}
