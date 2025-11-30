//! HIR → LIR lowering (scaffold)
//!
//! Transforms annotated HIR into LIR suitable for Wasm codegen. This is a
//! minimal placeholder to establish module structure.

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::hir::nodes::HirModule;
use crate::compiler::lir::nodes::LirModule;

/// Lower HIR into LIR.
///
/// Placeholder implementation returns an empty LIR module.
pub fn lower_to_lir(_hir: &HirModule) -> Result<LirModule, CompileError> {
    Ok(LirModule::default())
}
