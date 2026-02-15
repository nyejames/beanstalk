//! Control Flow Manager
//!
//! This module generates structured WASM control flow from LIR constructs,
//! managing block nesting and branch targets. It handles:
//! - If/else block generation with proper BlockType
//! - Loop construct generation with break targets
//! - Branch depth management and validation
//! - Proper block nesting and stack type consistency

// Many methods are prepared for later implementation phases
// (full integration with instruction lowering)
#![allow(dead_code)]

use crate::backends::lir::nodes::LirInst;
use crate::backends::wasm::error::WasmGenerationError;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use wasm_encoder::{BlockType, Function, Instruction, ValType};

/// Manages WASM structured control flow generation.
///
/// The ControlFlowManager tracks block nesting, manages branch targets,
/// and ensures proper WASM structured control flow semantics.
///
/// WASM control flow is structured - all branches must target enclosing blocks,
/// and blocks must be properly nested. This manager enforces these constraints.
pub struct ControlFlowManager {
    /// Stack of currently active blocks (innermost at the end)
    block_stack: Vec<BlockInfo>,
    /// Counter for generating unique block IDs
    next_block_id: u32,
}

/// The kind of control flow block
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlockKind {
    /// A regular block (br jumps to end)
    Block,
    /// A loop block (br jumps to start)
    Loop,
    /// An if block (with optional else)
    If,
}

/// Information about a control flow block
#[derive(Debug, Clone)]
pub struct BlockInfo {
    /// The kind of block (block, loop, if)
    pub kind: BlockKind,
    /// The WASM BlockType for this block
    pub block_type: BlockType,
    /// The nesting depth (0 = outermost)
    pub depth: u32,
    /// Optional result type for the block
    pub result_type: Option<ValType>,
    /// Unique identifier for this block
    pub block_id: u32,
}

impl ControlFlowManager {
    /// Create a new control flow manager
    pub fn new() -> Self {
        ControlFlowManager {
            block_stack: Vec::new(),
            next_block_id: 0,
        }
    }

    // =========================================================================
    // Block Management
    // =========================================================================

    /// Enter a new control flow block.
    ///
    /// This should be called before emitting the block instruction.
    /// Returns the block ID for reference.
    pub fn enter_block(&mut self, kind: BlockKind, result_type: Option<ValType>) -> u32 {
        let block_id = self.next_block_id;
        self.next_block_id += 1;

        let block_type = result_type
            .map(BlockType::Result)
            .unwrap_or(BlockType::Empty);

        let depth = self.block_stack.len() as u32;
        self.block_stack.push(BlockInfo {
            kind,
            block_type,
            depth,
            result_type,
            block_id,
        });

        block_id
    }

    /// Exit the current control flow block.
    ///
    /// This should be called after emitting the End instruction.
    /// Returns the BlockInfo of the exited block.
    pub fn exit_block(&mut self) -> Result<BlockInfo, CompilerError> {
        self.block_stack.pop().ok_or_else(|| {
            WasmGenerationError::control_flow(
                "unknown",
                0,
                None,
                Some("Attempted to exit block when no blocks are active".to_string()),
            )
            .to_compiler_error(ErrorLocation::default())
        })
    }

    /// Get the current nesting depth (number of active blocks)
    pub fn current_depth(&self) -> u32 {
        self.block_stack.len() as u32
    }

    /// Get information about the current (innermost) block
    pub fn current_block(&self) -> Option<&BlockInfo> {
        self.block_stack.last()
    }

    /// Get information about a block at a specific depth from current.
    /// depth=0 is the innermost block, depth=1 is the next outer, etc.
    pub fn block_at_depth(&self, depth: u32) -> Option<&BlockInfo> {
        if depth as usize >= self.block_stack.len() {
            return None;
        }
        let index = self.block_stack.len() - 1 - depth as usize;
        self.block_stack.get(index)
    }

    /// Check if a branch target depth is valid
    pub fn is_valid_branch_target(&self, target_depth: u32) -> bool {
        (target_depth as usize) < self.block_stack.len()
    }

    // =========================================================================
    // If/Else Block Generation
    // =========================================================================

    /// Generate an if block.
    ///
    /// The condition should already be on the stack (i32, 0 = false, non-zero = true).
    /// Returns the block ID for the if block.
    ///
    /// Stack: [i32 (condition)] -> []
    pub fn generate_if(
        &mut self,
        result_type: Option<ValType>,
        function: &mut Function,
    ) -> Result<u32, CompilerError> {
        let block_type = result_type
            .map(BlockType::Result)
            .unwrap_or(BlockType::Empty);

        function.instruction(&Instruction::If(block_type));
        let block_id = self.enter_block(BlockKind::If, result_type);

        Ok(block_id)
    }

    /// Generate an else clause for the current if block.
    ///
    /// Must be called while inside an if block.
    pub fn generate_else(&mut self, function: &mut Function) -> Result<(), CompilerError> {
        // Verify we're in an if block
        let current = self.current_block().ok_or_else(|| {
            WasmGenerationError::control_flow(
                "else",
                0,
                None,
                Some("Attempted to generate else clause when no blocks are active".to_string()),
            )
            .to_compiler_error(ErrorLocation::default())
        })?;

        if current.kind != BlockKind::If {
            return Err(WasmGenerationError::control_flow(
                "else",
                self.current_depth(),
                None,
                Some("Else clause can only be used inside an if block".to_string()),
            )
            .to_compiler_error(ErrorLocation::default()));
        }

        function.instruction(&Instruction::Else);
        Ok(())
    }

    /// Generate an if/else block with then and else branches.
    ///
    /// This is a convenience method that handles the complete if/else structure.
    /// The condition should already be on the stack.
    ///
    /// The `lower_instructions` callback is used to lower LIR instructions to WASM.
    pub fn generate_if_else_with_callback<F>(
        &mut self,
        result_type: Option<ValType>,
        then_instructions: &[LirInst],
        else_instructions: Option<&[LirInst]>,
        function: &mut Function,
        mut lower_instructions: F,
    ) -> Result<(), CompilerError>
    where
        F: FnMut(&[LirInst], &mut Function, &mut ControlFlowManager) -> Result<(), CompilerError>,
    {
        // Generate if instruction and enter block
        self.generate_if(result_type, function)?;

        // Generate then branch
        lower_instructions(then_instructions, function, self)?;

        // Generate else branch if present
        if let Some(else_insts) = else_instructions {
            self.generate_else(function)?;
            lower_instructions(else_insts, function, self)?;
        }

        // End the if block
        self.generate_end(function)?;

        Ok(())
    }

    // =========================================================================
    // Loop Block Generation
    // =========================================================================

    /// Generate a loop block.
    ///
    /// In WASM, a br instruction targeting a loop jumps to the start of the loop.
    /// Returns the block ID for the loop.
    pub fn generate_loop(
        &mut self,
        result_type: Option<ValType>,
        function: &mut Function,
    ) -> Result<u32, CompilerError> {
        let block_type = result_type
            .map(BlockType::Result)
            .unwrap_or(BlockType::Empty);

        function.instruction(&Instruction::Loop(block_type));
        let block_id = self.enter_block(BlockKind::Loop, result_type);

        Ok(block_id)
    }

    /// Generate a loop with body instructions.
    ///
    /// The `lower_instructions` callback is used to lower LIR instructions to WASM.
    pub fn generate_loop_with_callback<F>(
        &mut self,
        result_type: Option<ValType>,
        body_instructions: &[LirInst],
        function: &mut Function,
        mut lower_instructions: F,
    ) -> Result<(), CompilerError>
    where
        F: FnMut(&[LirInst], &mut Function, &mut ControlFlowManager) -> Result<(), CompilerError>,
    {
        // Generate loop instruction and enter block
        self.generate_loop(result_type, function)?;

        // Generate loop body
        lower_instructions(body_instructions, function, self)?;

        // End the loop block
        self.generate_end(function)?;

        Ok(())
    }

    // =========================================================================
    // Regular Block Generation
    // =========================================================================

    /// Generate a regular block.
    ///
    /// In WASM, a br instruction targeting a block jumps to the end of the block.
    /// Returns the block ID for the block.
    pub fn generate_block(
        &mut self,
        result_type: Option<ValType>,
        function: &mut Function,
    ) -> Result<u32, CompilerError> {
        let block_type = result_type
            .map(BlockType::Result)
            .unwrap_or(BlockType::Empty);

        function.instruction(&Instruction::Block(block_type));
        let block_id = self.enter_block(BlockKind::Block, result_type);

        Ok(block_id)
    }

    /// Generate a block with body instructions.
    ///
    /// The `lower_instructions` callback is used to lower LIR instructions to WASM.
    pub fn generate_block_with_callback<F>(
        &mut self,
        result_type: Option<ValType>,
        body_instructions: &[LirInst],
        function: &mut Function,
        mut lower_instructions: F,
    ) -> Result<(), CompilerError>
    where
        F: FnMut(&[LirInst], &mut Function, &mut ControlFlowManager) -> Result<(), CompilerError>,
    {
        // Generate block instruction and enter block
        self.generate_block(result_type, function)?;

        // Generate block body
        lower_instructions(body_instructions, function, self)?;

        // End the block
        self.generate_end(function)?;

        Ok(())
    }

    // =========================================================================
    // Branch Instructions
    // =========================================================================

    /// Generate an unconditional branch instruction.
    ///
    /// The target_depth is relative to the current block:
    /// - 0 = branch to innermost block
    /// - 1 = branch to next outer block
    /// - etc.
    ///
    /// For loops, this jumps to the start. For blocks/if, this jumps to the end.
    pub fn generate_branch(
        &mut self,
        target_depth: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        if !self.is_valid_branch_target(target_depth) {
            return Err(WasmGenerationError::control_flow(
                "br",
                self.current_depth(),
                Some(target_depth),
                Some(format!(
                    "Branch target depth {} exceeds current nesting depth {}",
                    target_depth,
                    self.current_depth()
                )),
            )
            .to_compiler_error(ErrorLocation::default()));
        }

        function.instruction(&Instruction::Br(target_depth));
        Ok(())
    }

    /// Generate a conditional branch instruction.
    ///
    /// The condition should already be on the stack (i32).
    /// If the condition is non-zero, the branch is taken.
    ///
    /// Stack: [i32 (condition)] -> []
    pub fn generate_branch_if(
        &mut self,
        target_depth: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        if !self.is_valid_branch_target(target_depth) {
            return Err(WasmGenerationError::control_flow(
                "br_if",
                self.current_depth(),
                Some(target_depth),
                Some(format!(
                    "Conditional branch target depth {} exceeds current nesting depth {}",
                    target_depth,
                    self.current_depth()
                )),
            )
            .to_compiler_error(ErrorLocation::default()));
        }

        function.instruction(&Instruction::BrIf(target_depth));
        Ok(())
    }

    // =========================================================================
    // End Instruction
    // =========================================================================

    /// Generate an end instruction and exit the current block.
    pub fn generate_end(&mut self, function: &mut Function) -> Result<BlockInfo, CompilerError> {
        function.instruction(&Instruction::End);
        self.exit_block()
    }

    // =========================================================================
    // Validation
    // =========================================================================

    /// Validate that all blocks have been properly closed.
    ///
    /// This should be called at the end of function generation.
    pub fn validate_all_blocks_closed(&self) -> Result<(), CompilerError> {
        if !self.block_stack.is_empty() {
            return Err(WasmGenerationError::control_flow(
                "unclosed blocks",
                self.current_depth(),
                None,
                Some(format!(
                    "{} block(s) were not properly closed",
                    self.block_stack.len()
                )),
            )
            .to_compiler_error(ErrorLocation::default()));
        }
        Ok(())
    }

    /// Validate stack consistency across control flow.
    ///
    /// This checks that all branches have consistent stack types.
    /// Currently a placeholder for more sophisticated validation.
    pub fn validate_stack_consistency(&self) -> Result<(), CompilerError> {
        // Basic validation: ensure we're not in an invalid state
        // More sophisticated validation would track stack types through branches
        Ok(())
    }

    // =========================================================================
    // Utility Methods
    // =========================================================================

    /// Find the depth to the nearest enclosing loop.
    ///
    /// Returns None if there is no enclosing loop.
    /// This is useful for implementing break statements.
    pub fn find_enclosing_loop_depth(&self) -> Option<u32> {
        for (i, block) in self.block_stack.iter().rev().enumerate() {
            if block.kind == BlockKind::Loop {
                return Some(i as u32);
            }
        }
        None
    }

    /// Find the depth to the nearest enclosing block (non-loop).
    ///
    /// Returns None if there is no enclosing block.
    /// This is useful for implementing break-from-block statements.
    pub fn find_enclosing_block_depth(&self) -> Option<u32> {
        for (i, block) in self.block_stack.iter().rev().enumerate() {
            if block.kind == BlockKind::Block {
                return Some(i as u32);
            }
        }
        None
    }

    /// Get the BlockType for a given result type
    pub fn block_type_for_result(result_type: Option<ValType>) -> BlockType {
        result_type
            .map(BlockType::Result)
            .unwrap_or(BlockType::Empty)
    }

    /// Reset the control flow manager to initial state.
    ///
    /// This is useful when starting to generate a new function.
    pub fn reset(&mut self) {
        self.block_stack.clear();
        self.next_block_id = 0;
    }
}

impl Default for ControlFlowManager {
    fn default() -> Self {
        Self::new()
    }
}
