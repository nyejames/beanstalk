//! Borrow checker (scaffold)
//!
//! Walks HIR and validates borrow/move rules. This placeholder only provides
//! the public entry point used by the pipeline.

use crate::borrow_log;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::hir::nodes::HirModule;

/// Perform borrow checking on the provided HIR module.
///
/// Placeholder implementation: does no analysis and always succeeds.
pub fn check_borrows(_hir: &mut HirModule) -> Result<(), CompileError> {
    borrow_log!("check_borrows(): placeholder borrow check succeeded");
    Ok(())
}
