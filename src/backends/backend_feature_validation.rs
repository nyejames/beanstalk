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
    ReachableDynamicTraitOperation, ReachableDynamicTraitOperationKind,
    collect_reachability_from_start,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;

pub enum BackendFeatureValidationError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

/// Validates HIR runtime features that are target-specific after frontend semantics are complete.
///
/// WHAT: dynamic trait values are legal HIR, but only the JS backend lowers their wrappers and
/// dispatch tables for Alpha.
/// WHY: HTML-Wasm must reject only reachable dynamic operations; unused functions stay type
/// checked but do not block the experimental Wasm build path.
pub fn validate_hir_backend_feature_support(
    hir: &HirModule,
    target: BackendTarget,
    string_table: &mut StringTable,
) -> Result<(), BackendFeatureValidationError> {
    let reachability = collect_reachability_from_start(hir)
        .map_err(|error| BackendFeatureValidationError::Infrastructure(Box::new(error)))?;

    if target == BackendTarget::Wasm {
        validate_wasm_dynamic_traits(
            &reachability.reachable_dynamic_trait_operations,
            target,
            string_table,
        )?;
    }

    Ok(())
}

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

    Err(BackendFeatureValidationError::Diagnostic(Box::new(
        CompilerDiagnostic::unsupported_backend_feature(
            string_table.intern(target.as_str()),
            string_table.intern(feature),
            operation.location.clone(),
        ),
    )))
}
