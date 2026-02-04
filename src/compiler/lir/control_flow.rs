//! Control Flow Lowering
//!
//! This module handles lowering control flow constructs (if, match, loop,
//! break, continue, return) to LIR instructions.

use crate::compiler::compiler_messages::compiler_errors::CompilerError;
use crate::compiler::hir::nodes::{BlockId, HirBlock, HirExpr, HirMatchArm, HirPattern};
use crate::compiler::lir::nodes::{LirInst, LirType};

use super::context::{LoopContext, LoweringContext};
use super::types::{datatype_to_lir_type, hir_expr_to_lir_type};

impl LoweringContext {
    // ========================================================================
    // If-Statement Lowering
    // ========================================================================

    /// Lowers a HIR if-statement to LIR instructions.
    pub fn lower_if(
        &mut self,
        condition: &HirExpr,
        then_block: BlockId,
        else_block: Option<BlockId>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the condition expression
        insts.extend(self.lower_expr(condition)?);

        // Lower the then block
        let then_insts = self.lower_block(then_block, blocks)?;

        // Lower the else block if present
        let else_insts = if let Some(else_id) = else_block {
            Some(self.lower_block(else_id, blocks)?)
        } else {
            None
        };

        // Emit LIR if instruction
        insts.push(LirInst::If {
            then_branch: then_insts,
            else_branch: else_insts,
        });

        Ok(insts)
    }

    // ========================================================================
    // Match Expression Lowering
    // ========================================================================

    /// Lowers a HIR match expression to LIR instructions.
    pub fn lower_match(
        &mut self,
        scrutinee: &HirExpr,
        arms: &[HirMatchArm],
        default_block: Option<BlockId>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the scrutinee expression and store in a temporary
        insts.extend(self.lower_expr(scrutinee)?);

        let scrutinee_type = hir_expr_to_lir_type(scrutinee);
        let scrutinee_local = self.local_allocator.allocate(scrutinee_type);
        insts.push(LirInst::LocalSet(scrutinee_local));

        // Build the nested if-else structure for match arms
        let match_insts =
            self.lower_match_arms(scrutinee_local, scrutinee_type, arms, default_block, blocks)?;
        insts.extend(match_insts);

        // Free the scrutinee local
        self.local_allocator.free(scrutinee_local);

        Ok(insts)
    }

    /// Lowers match arms to nested if-else instructions.
    fn lower_match_arms(
        &mut self,
        scrutinee_local: u32,
        scrutinee_type: LirType,
        arms: &[HirMatchArm],
        default_block: Option<BlockId>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        if arms.is_empty() {
            if let Some(default_id) = default_block {
                return self.lower_block(default_id, blocks);
            } else {
                return Ok(vec![LirInst::Nop]);
            }
        }

        let arm = &arms[0];
        let remaining_arms = &arms[1..];

        // Lower the pattern comparison
        let pattern_insts =
            self.lower_pattern_comparison(scrutinee_local, scrutinee_type, &arm.pattern)?;

        // Lower the guard if present
        let condition_insts = if let Some(guard) = &arm.guard {
            let mut guard_insts = pattern_insts;
            let pattern_result_local = self.local_allocator.allocate(LirType::I32);
            guard_insts.push(LirInst::LocalSet(pattern_result_local));
            guard_insts.extend(self.lower_expr(guard)?);
            guard_insts.push(LirInst::LocalGet(pattern_result_local));
            guard_insts.push(LirInst::I32Mul); // AND operation for booleans
            self.local_allocator.free(pattern_result_local);
            guard_insts
        } else {
            pattern_insts
        };

        // Lower the arm body
        let body_insts = self.lower_block(arm.body, blocks)?;

        // Recursively lower remaining arms as the else branch
        let else_insts = self.lower_match_arms(
            scrutinee_local,
            scrutinee_type,
            remaining_arms,
            default_block,
            blocks,
        )?;

        // Build the if-else structure
        let mut insts = condition_insts;
        insts.push(LirInst::If {
            then_branch: body_insts,
            else_branch: if else_insts.is_empty() {
                None
            } else {
                Some(else_insts)
            },
        });

        Ok(insts)
    }

    /// Lowers a pattern comparison to LIR instructions.
    fn lower_pattern_comparison(
        &mut self,
        scrutinee_local: u32,
        scrutinee_type: LirType,
        pattern: &HirPattern,
    ) -> Result<Vec<LirInst>, CompilerError> {
        match pattern {
            HirPattern::Literal(lit_expr) => {
                let mut insts = Vec::new();

                // Load scrutinee
                insts.push(LirInst::LocalGet(scrutinee_local));

                // Lower the literal expression
                insts.extend(self.lower_expr(lit_expr)?);

                // Emit equality comparison based on type
                let eq_inst = match scrutinee_type {
                    LirType::I32 => LirInst::I32Eq,
                    LirType::I64 => LirInst::I64Eq,
                    LirType::F64 => LirInst::F64Eq,
                    LirType::F32 => {
                        return Err(CompilerError::lir_transformation(
                            "F32 pattern matching not yet supported",
                        ));
                    }
                };
                insts.push(eq_inst);

                Ok(insts)
            }
            HirPattern::Range { start, end } => {
                let mut insts = Vec::new();

                // Check scrutinee >= start
                insts.push(LirInst::LocalGet(scrutinee_local));
                insts.extend(self.lower_expr(start)?);
                let ge_insts = self.emit_greater_or_equal(scrutinee_type)?;
                insts.extend(ge_insts);

                // Store result in temporary
                let ge_result_local = self.local_allocator.allocate(LirType::I32);
                insts.push(LirInst::LocalSet(ge_result_local));

                // Check scrutinee <= end
                insts.push(LirInst::LocalGet(scrutinee_local));
                insts.extend(self.lower_expr(end)?);
                let le_insts = self.emit_less_or_equal(scrutinee_type)?;
                insts.extend(le_insts);

                // AND the two results
                insts.push(LirInst::LocalGet(ge_result_local));
                insts.push(LirInst::I32Mul);

                self.local_allocator.free(ge_result_local);

                Ok(insts)
            }
            HirPattern::Wildcard => Ok(vec![LirInst::I32Const(1)]),
        }
    }

    /// Emits instructions for greater-or-equal comparison.
    pub fn emit_greater_or_equal(&self, ty: LirType) -> Result<Vec<LirInst>, CompilerError> {
        let lt_inst = match ty {
            LirType::I32 => LirInst::I32LtS,
            LirType::I64 => LirInst::I64LtS,
            LirType::F64 | LirType::F32 => {
                return Err(CompilerError::lir_transformation(
                    "Float comparison >= not yet supported",
                ));
            }
        };

        Ok(vec![lt_inst, LirInst::I32Const(0), LirInst::I32Eq])
    }

    /// Emits instructions for less-or-equal comparison.
    pub fn emit_less_or_equal(&self, ty: LirType) -> Result<Vec<LirInst>, CompilerError> {
        let gt_inst = match ty {
            LirType::I32 => LirInst::I32GtS,
            LirType::I64 => LirInst::I64GtS,
            LirType::F64 | LirType::F32 => {
                return Err(CompilerError::lir_transformation(
                    "Float comparison <= not yet supported",
                ));
            }
        };

        Ok(vec![gt_inst, LirInst::I32Const(0), LirInst::I32Eq])
    }

    // ========================================================================
    // Loop Lowering
    // ========================================================================

    /// Lowers a HIR loop to LIR instructions.
    pub fn lower_loop(
        &mut self,
        label: BlockId,
        binding: Option<(
            crate::compiler::string_interning::InternedString,
            crate::compiler::datatypes::DataType,
        )>,
        iterator: Option<&HirExpr>,
        body: BlockId,
        index_binding: Option<crate::compiler::string_interning::InternedString>,
        blocks: &[HirBlock],
    ) -> Result<Vec<LirInst>, CompilerError> {
        // Push loop context for break/continue handling
        let loop_depth = self.loop_stack.len() as u32;
        self.loop_stack.push(LoopContext::new(label, loop_depth));

        let mut insts = Vec::new();

        // Handle iterator setup if present (for-in loops)
        if let Some(iter_expr) = iterator {
            insts.extend(self.lower_iterator_setup(&binding, &index_binding, iter_expr)?);
        }

        // Lower the loop body
        let body_insts = self.lower_block(body, blocks)?;

        // Emit LIR loop instruction wrapped in a block for break handling
        insts.push(LirInst::Block {
            instructions: vec![LirInst::Loop {
                instructions: body_insts,
            }],
        });

        // Pop loop context
        self.loop_stack.pop();

        Ok(insts)
    }

    /// Lowers iterator setup for for-in loops.
    fn lower_iterator_setup(
        &mut self,
        binding: &Option<(
            crate::compiler::string_interning::InternedString,
            crate::compiler::datatypes::DataType,
        )>,
        index_binding: &Option<crate::compiler::string_interning::InternedString>,
        iterator: &HirExpr,
    ) -> Result<Vec<LirInst>, CompilerError> {
        use crate::compiler::hir::nodes::HirExprKind;

        let mut insts = Vec::new();

        // Allocate locals for loop variables
        if let Some((var_name, var_type)) = binding {
            let lir_type = datatype_to_lir_type(var_type);
            let local_idx = self.local_allocator.allocate(lir_type);
            self.var_to_local.insert(*var_name, local_idx);

            // Initialize the loop variable based on the iterator type
            match &iterator.kind {
                HirExprKind::Range { start, .. } => {
                    insts.extend(self.lower_expr(start)?);
                    insts.push(LirInst::LocalSet(local_idx));
                }
                _ => {
                    // Initialize to default value
                    match lir_type {
                        LirType::I32 => insts.push(LirInst::I32Const(0)),
                        LirType::I64 => insts.push(LirInst::I64Const(0)),
                        LirType::F32 => insts.push(LirInst::F32Const(0.0)),
                        LirType::F64 => insts.push(LirInst::F64Const(0.0)),
                    }
                    insts.push(LirInst::LocalSet(local_idx));
                }
            }
        }

        // Allocate index variable if present
        if let Some(idx_name) = index_binding {
            let idx_local = self.local_allocator.allocate(LirType::I64);
            self.var_to_local.insert(*idx_name, idx_local);
            insts.push(LirInst::I64Const(0));
            insts.push(LirInst::LocalSet(idx_local));
        }

        Ok(insts)
    }

    // ========================================================================
    // Break and Continue Lowering
    // ========================================================================

    /// Lowers a HIR break statement to LIR instructions.
    pub fn lower_break(&self, target: BlockId) -> Result<Vec<LirInst>, CompilerError> {
        let loop_ctx = self.find_loop_context(target)?;

        let current_depth = self.loop_stack.len() as u32;
        let target_depth = loop_ctx.depth;
        let nesting_diff = current_depth - target_depth - 1;

        // Branch depth: 1 (to exit loop) + 2 * nesting_diff (for nested loops)
        let branch_depth = 1 + nesting_diff * 2;

        Ok(vec![LirInst::Br(branch_depth)])
    }

    /// Lowers a HIR continue statement to LIR instructions.
    pub fn lower_continue(&self, target: BlockId) -> Result<Vec<LirInst>, CompilerError> {
        let loop_ctx = self.find_loop_context(target)?;

        let current_depth = self.loop_stack.len() as u32;
        let target_depth = loop_ctx.depth;
        let nesting_diff = current_depth - target_depth - 1;

        // Branch depth: 0 (to loop start) + 2 * nesting_diff (for nested loops)
        let branch_depth = nesting_diff * 2;

        Ok(vec![LirInst::Br(branch_depth)])
    }

    /// Finds the loop context for a given target block ID.
    fn find_loop_context(&self, target: BlockId) -> Result<&LoopContext, CompilerError> {
        self.loop_stack
            .iter()
            .rev()
            .find(|ctx| ctx.label == target)
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Break/continue target not found in loop stack: block {}",
                    target
                ))
            })
    }

    // ========================================================================
    // Return Lowering
    // ========================================================================

    /// Lowers a HIR return statement to LIR instructions.
    pub fn lower_return(&mut self, values: &[HirExpr]) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower each return value expression
        for value in values {
            insts.extend(self.lower_expr(value)?);
        }

        // Emit return instruction
        insts.push(LirInst::Return);

        Ok(insts)
    }

    /// Lowers a HIR error return statement to LIR instructions.
    pub fn lower_return_error(&mut self, error: &HirExpr) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        // Lower the error expression
        insts.extend(self.lower_expr(error)?);

        // Emit return instruction
        insts.push(LirInst::Return);

        Ok(insts)
    }

    /// Lowers a HIR panic statement to LIR instructions.
    pub fn lower_panic(
        &mut self,
        message: Option<&HirExpr>,
    ) -> Result<Vec<LirInst>, CompilerError> {
        let mut insts = Vec::new();

        if let Some(msg) = message {
            insts.extend(self.lower_expr(msg)?);
            insts.push(LirInst::Drop);
        }

        // Emit unreachable (trap) - using a special call index
        insts.push(LirInst::Call(0xFFFFFFFF));

        Ok(insts)
    }
}
