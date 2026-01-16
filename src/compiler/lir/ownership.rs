//! Ownership Operations
//!
//! This module handles ownership tagging and drop operations for
//! Beanstalk's runtime ownership model.

use crate::compiler::compiler_messages::compiler_errors::CompilerError;
use crate::compiler::hir::nodes::{HirExpr, HirPlace};
use crate::compiler::lir::nodes::LirInst;

use super::context::LoweringContext;
use super::types::data_type_to_lir_type;

impl LoweringContext {
    // ========================================================================
    // Ownership Tagging Helpers
    // ========================================================================

    /// Emits a `TagAsOwned` instruction for the given local.
    ///
    /// This sets the ownership bit (bit 0) to 1 in the tagged pointer,
    /// indicating that the value is owned.
    pub fn emit_tag_as_owned(&self, local_idx: u32) -> LirInst {
        LirInst::TagAsOwned(local_idx)
    }

    /// Emits a `TagAsBorrowed` instruction for the given local.
    ///
    /// This clears the ownership bit (bit 0) to 0 in the tagged pointer,
    /// indicating that the value is borrowed.
    pub fn emit_tag_as_borrowed(&self, local_idx: u32) -> LirInst {
        LirInst::TagAsBorrowed(local_idx)
    }

    /// Emits a `MaskPointer` instruction.
    ///
    /// This extracts the real pointer address from a tagged pointer by
    /// masking out the ownership bit.
    pub fn emit_mask_pointer(&self) -> LirInst {
        LirInst::MaskPointer
    }

    /// Emits a `TestOwnership` instruction.
    ///
    /// This tests the ownership bit of a tagged pointer and leaves the
    /// result on the stack (1 = owned, 0 = borrowed).
    pub fn emit_test_ownership(&self) -> LirInst {
        LirInst::TestOwnership
    }

    // ========================================================================
    // Possible Drop Lowering
    // ========================================================================

    /// Lowers a `HirStmt::PossibleDrop` to LIR instructions.
    ///
    /// This emits a conditional drop instruction that will free the value
    /// only if it is owned at runtime.
    pub fn lower_possible_drop(&mut self, place: &HirPlace) -> Result<Vec<LirInst>, CompilerError> {
        let local_idx = self.get_local_for_place(place)?;
        Ok(vec![LirInst::PossibleDrop(local_idx)])
    }

    // ========================================================================
    // Mutable Assignment with Ownership
    // ========================================================================

    /// Lowers a mutable assignment with ownership tagging.
    ///
    /// This handles the `~=` assignment operator in Beanstalk.
    pub fn lower_mutable_assign(
        &mut self,
        target: &HirPlace,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Get or allocate the target local
        let target_local = self.get_or_allocate_local(target, &value.data_type)?;

        // Store the value in the local
        insts.push(LirInst::LocalSet(target_local));

        // Tag the local as owned
        insts.push(self.emit_tag_as_owned(target_local));

        Ok(insts)
    }

    /// Lowers a borrowed assignment (no ownership transfer).
    pub fn lower_borrowed_assign(
        &mut self,
        target: &HirPlace,
        value: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the value expression
        insts.extend(self.lower_expr(value)?);

        // Get or allocate the target local
        let target_local = self.get_or_allocate_local(target, &value.data_type)?;

        // Store the value in the local
        insts.push(LirInst::LocalSet(target_local));

        // Tag the local as borrowed
        insts.push(self.emit_tag_as_borrowed(target_local));

        Ok(insts)
    }

    /// Lowers an assignment statement based on mutability.
    pub fn lower_assign(
        &mut self,
        target: &HirPlace,
        value: &HirExpr,
        is_mutable: bool,
    ) -> Result<Vec<LirInst>, CompilerError> {
        match target {
            HirPlace::Var(_) => {
                if is_mutable {
                    self.lower_mutable_assign(target, value)
                } else {
                    self.lower_borrowed_assign(target, value)
                }
            }
            HirPlace::Field { base, field } => self.lower_field_assignment(base, *field, value),
            HirPlace::Index { base, index } => {
                self.lower_collection_element_assignment(base, index, value)
            }
        }
    }
}
