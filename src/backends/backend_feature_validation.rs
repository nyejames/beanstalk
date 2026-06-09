//! Pre-lowering validation for backend-specific HIR feature support.
//!
//! WHAT: rejects reachable HIR operations that are valid language semantics but unsupported by
//! a selected backend target.
//! WHY: backend lowerers should receive only features they can lower, and users should see a
//! structured source diagnostic instead of a backend-internal lowering error.

use crate::backends::external_package_validation::BackendTarget;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::module::HirModule;
use crate::compiler_frontend::hir::reachability::{
    ReachableDynamicTraitOperation, ReachableDynamicTraitOperationKind, ReachableMapUse,
    ReachableMapUseKind, collect_reachability_from_start,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

/// Failure mode for backend feature validation.
///
/// WHAT: either a user-facing diagnostic for an unsupported reachable operation, or an
///       infrastructure error if reachability collection itself fails.
pub enum BackendFeatureValidationError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

/// Validates HIR runtime features that are target-specific after frontend semantics are complete.
///
/// WHAT: dynamic trait values and hashmap construction/use are legal HIR, but only the JS
/// backend lowers them for Alpha.
/// WHY: HTML-Wasm must reject only reachable unsupported operations; unused functions stay
/// type checked but do not block the experimental Wasm build path.
pub fn validate_hir_backend_feature_support(
    hir: &HirModule,
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let reachability = collect_reachability_from_start(hir)
        .map_err(|error| BackendFeatureValidationError::Infrastructure(Box::new(error)))?;

    // JS supports all current runtime features; Wasm requires additional validation.
    if target == BackendTarget::Wasm {
        validate_wasm_dynamic_traits(
            &reachability.reachable_dynamic_trait_operations,
            target,
            string_table,
        )?;

        validate_wasm_maps(&reachability.reachable_map_uses, target, string_table)?;
    }

    Ok(())
}

/// Reports the first reachable unsupported dynamic-trait operation for the Wasm target.
///
/// WHAT: dynamic trait construction and dispatch are valid HIR, but Wasm lowering does not
/// yet support them.
/// WHY: reject early with a structured diagnostic at the source location instead of a
/// backend-internal lowering failure.
fn validate_wasm_dynamic_traits(
    operations: &[ReachableDynamicTraitOperation],
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let Some(operation) = operations.first() else {
        return Ok(());
    };

    let feature = match &operation.kind {
        ReachableDynamicTraitOperationKind::Construct { .. } => "dynamic trait value construction",
        ReachableDynamicTraitOperationKind::Dispatch { .. } => "dynamic trait method dispatch",
    };

    // Only the first reachable unsupported operation is reported. Unreachable helpers remain
    // valid typed HIR and do not block the build.
    let diagnostic = CompilerDiagnostic::unsupported_backend_feature(
        string_table.intern(target.as_str()),
        string_table.intern(feature),
        operation.location.clone(),
    );

    Err(BackendFeatureValidationError::Diagnostic(Box::new(
        diagnostic,
    )))
}

/// Reports the first reachable unsupported hashmap operation for the Wasm target.
///
/// WHAT: hashmap literals and operations are valid HIR, but Wasm lowering does not yet
/// support them.
/// WHY: reject early with a structured diagnostic at the source location instead of a
/// backend-internal lowering failure.
fn validate_wasm_maps(
    map_uses: &[ReachableMapUse],
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let Some(map_use) = map_uses.first() else {
        return Ok(());
    };

    let feature = match &map_use.kind {
        ReachableMapUseKind::Literal => "hashmap construction",
        ReachableMapUseKind::Operation(_) => "hashmap operation",
    };

    // Only the first reachable unsupported operation is reported. Unreachable helpers remain
    // valid typed HIR and do not block the build.
    let diagnostic = CompilerDiagnostic::unsupported_backend_feature(
        string_table.intern(target.as_str()),
        string_table.intern(feature),
        map_use.location.clone(),
    );

    Err(BackendFeatureValidationError::Diagnostic(Box::new(
        diagnostic,
    )))
}
