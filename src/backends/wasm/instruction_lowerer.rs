//! Instruction Lowering Engine
//!
//! This module converts LIR instructions to WASM bytecode with proper
//! stack discipline and type handling. It handles:
//! - Local variable access (LocalGet, LocalSet, LocalTee)
//! - Constant loading (I32Const, I64Const, F32Const, F64Const)
//! - Arithmetic operations (add, sub, mul, div)
//! - Comparison operations (eq, ne, lt, gt)
//! - Memory operations (load, store)
//! - Proper parameter vs local variable ordering
//! - Index translation from LIR to WASM local space

// Many methods are prepared for later implementation phases
// (full instruction lowering integration with encode.rs)
#![allow(dead_code)]

use crate::backends::lir::nodes::LirInst;
use crate::backends::wasm::analyzer::LocalMap;
use crate::backends::wasm::constants::{ALIGNMENT_MASK, OWNERSHIP_BIT};
use crate::backends::wasm::error::WasmGenerationError;
use crate::compiler_frontend::compiler_errors::{CompilerError, ErrorLocation};
use std::collections::HashMap;
use wasm_encoder::{Function, Ieee32, Ieee64, Instruction, MemArg};

/// Converts LIR instructions to WASM bytecode.
///
/// The InstructionLowerer handles the translation of LIR instructions to WASM,
/// including proper local index mapping, stack discipline management, and
/// efficient instruction selection.
///
/// Stack discipline is maintained by ensuring that each instruction consumes
/// and produces the correct number of stack values according to WASM semantics.
pub struct InstructionLowerer {
    /// Mapping from LIR local IDs to WASM local indices
    local_map: LocalMap,
    /// Function indices for call instructions
    function_indices: HashMap<String, u32>,
}

impl InstructionLowerer {
    /// Create a new instruction lowerer with the given local mapping.
    pub fn new(local_map: LocalMap) -> Self {
        InstructionLowerer {
            local_map,
            function_indices: HashMap::new(),
        }
    }

    /// Create a new instruction lowerer with local mapping and function indices.
    pub fn with_function_indices(
        local_map: LocalMap,
        function_indices: HashMap<String, u32>,
    ) -> Self {
        InstructionLowerer {
            local_map,
            function_indices,
        }
    }

    /// Get the WASM local index for a LIR local ID.
    ///
    /// Returns an error if the LIR local ID is not found in the mapping.
    fn get_wasm_local(&self, lir_local: u32) -> Result<u32, WasmGenerationError> {
        self.local_map.get_wasm_index(lir_local).ok_or_else(|| {
            WasmGenerationError::lir_analysis(
                format!("Local index {} not found in mapping", lir_local),
                "LocalGet/LocalSet",
            )
        })
    }

    /// Lower a single LIR instruction to WASM.
    ///
    /// This method handles the translation of individual LIR instructions,
    /// emitting the appropriate WASM instructions to the function body.
    /// Stack discipline is maintained by following WASM's operand stack semantics.
    pub fn lower_instruction(
        &self,
        inst: &LirInst,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        match inst {
            // =========================================================================
            // Local Variable Access Operations
            // Stack: [] -> [value] for LocalGet
            // Stack: [value] -> [] for LocalSet
            // Stack: [value] -> [value] for LocalTee
            // =========================================================================
            LirInst::LocalGet(lir_local) => {
                let wasm_local = self
                    .get_wasm_local(*lir_local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalGet(wasm_local));
            }

            LirInst::LocalSet(lir_local) => {
                let wasm_local = self
                    .get_wasm_local(*lir_local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalSet(wasm_local));
            }

            LirInst::LocalTee(lir_local) => {
                let wasm_local = self
                    .get_wasm_local(*lir_local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalTee(wasm_local));
            }

            // =========================================================================
            // Global Variable Access Operations
            // Stack: [] -> [value] for GlobalGet
            // Stack: [value] -> [] for GlobalSet
            // =========================================================================
            LirInst::GlobalGet(global_index) => {
                function.instruction(&Instruction::GlobalGet(*global_index));
            }

            LirInst::GlobalSet(global_index) => {
                function.instruction(&Instruction::GlobalSet(*global_index));
            }

            // =========================================================================
            // Constant Loading Operations
            // Stack: [] -> [value]
            // Uses immediate values for efficient code generation
            // =========================================================================
            LirInst::I32Const(value) => {
                function.instruction(&Instruction::I32Const(*value));
            }

            LirInst::I64Const(value) => {
                function.instruction(&Instruction::I64Const(*value));
            }

            LirInst::F32Const(value) => {
                function.instruction(&Instruction::F32Const(Ieee32::from(*value)));
            }

            LirInst::F64Const(value) => {
                function.instruction(&Instruction::F64Const(Ieee64::from(*value)));
            }

            // =========================================================================
            // I32 Arithmetic Operations
            // Stack: [i32, i32] -> [i32]
            // =========================================================================
            LirInst::I32Add => {
                function.instruction(&Instruction::I32Add);
            }

            LirInst::I32Sub => {
                function.instruction(&Instruction::I32Sub);
            }

            LirInst::I32Mul => {
                function.instruction(&Instruction::I32Mul);
            }

            LirInst::I32DivS => {
                function.instruction(&Instruction::I32DivS);
            }

            // =========================================================================
            // I32 Comparison Operations
            // Stack: [i32, i32] -> [i32] (0 or 1)
            // =========================================================================
            LirInst::I32Eq => {
                function.instruction(&Instruction::I32Eq);
            }

            LirInst::I32Ne => {
                function.instruction(&Instruction::I32Ne);
            }

            LirInst::I32LtS => {
                function.instruction(&Instruction::I32LtS);
            }

            LirInst::I32GtS => {
                function.instruction(&Instruction::I32GtS);
            }

            // =========================================================================
            // I64 Arithmetic Operations
            // Stack: [i64, i64] -> [i64]
            // =========================================================================
            LirInst::I64Add => {
                function.instruction(&Instruction::I64Add);
            }

            LirInst::I64Sub => {
                function.instruction(&Instruction::I64Sub);
            }

            LirInst::I64Mul => {
                function.instruction(&Instruction::I64Mul);
            }

            LirInst::I64DivS => {
                function.instruction(&Instruction::I64DivS);
            }

            // =========================================================================
            // I64 Comparison Operations
            // Stack: [i64, i64] -> [i32] (0 or 1)
            // =========================================================================
            LirInst::I64Eq => {
                function.instruction(&Instruction::I64Eq);
            }

            LirInst::I64Ne => {
                function.instruction(&Instruction::I64Ne);
            }

            LirInst::I64LtS => {
                function.instruction(&Instruction::I64LtS);
            }

            LirInst::I64GtS => {
                function.instruction(&Instruction::I64GtS);
            }

            // =========================================================================
            // F64 Arithmetic Operations
            // Stack: [f64, f64] -> [f64]
            // =========================================================================
            LirInst::F64Add => {
                function.instruction(&Instruction::F64Add);
            }

            LirInst::F64Sub => {
                function.instruction(&Instruction::F64Sub);
            }

            LirInst::F64Mul => {
                function.instruction(&Instruction::F64Mul);
            }

            LirInst::F64Div => {
                function.instruction(&Instruction::F64Div);
            }

            // =========================================================================
            // F64 Comparison Operations
            // Stack: [f64, f64] -> [i32] (0 or 1)
            // =========================================================================
            LirInst::F64Eq => {
                function.instruction(&Instruction::F64Eq);
            }

            LirInst::F64Ne => {
                function.instruction(&Instruction::F64Ne);
            }

            // =========================================================================
            // Memory Load Operations
            // Stack: [i32 (address)] -> [value]
            // Uses MemArg with offset, alignment, and memory index
            // =========================================================================
            LirInst::I32Load { offset, align } => {
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            LirInst::I64Load { offset, align } => {
                function.instruction(&Instruction::I64Load(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            LirInst::F32Load { offset, align } => {
                function.instruction(&Instruction::F32Load(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            LirInst::F64Load { offset, align } => {
                function.instruction(&Instruction::F64Load(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            // =========================================================================
            // Memory Store Operations
            // Stack: [i32 (address), value] -> []
            // Uses MemArg with offset, alignment, and memory index
            // =========================================================================
            LirInst::I32Store { offset, align } => {
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            LirInst::I64Store { offset, align } => {
                function.instruction(&Instruction::I64Store(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            LirInst::F32Store { offset, align } => {
                function.instruction(&Instruction::F32Store(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            LirInst::F64Store { offset, align } => {
                function.instruction(&Instruction::F64Store(MemArg {
                    offset: *offset as u64,
                    align: *align,
                    memory_index: 0,
                }));
            }

            // =========================================================================
            // Control Flow Operations
            // =========================================================================
            LirInst::Return => {
                function.instruction(&Instruction::Return);
            }

            LirInst::Call(func_index) => {
                function.instruction(&Instruction::Call(*func_index));
            }

            // =========================================================================
            // Stack Management
            // =========================================================================
            LirInst::Nop => {
                function.instruction(&Instruction::Nop);
            }

            LirInst::Drop => {
                function.instruction(&Instruction::Drop);
            }

            // =========================================================================
            // Control Flow Blocks - Delegated to ControlFlowManager
            // These instructions require the ControlFlowManager for proper handling
            // =========================================================================
            LirInst::Block { instructions: _ } => {
                // Block instructions should be handled via lower_block method
                // which has access to ControlFlowManager
                return Err(WasmGenerationError::instruction_lowering(
                    "Block",
                    "Use lower_block() method with ControlFlowManager for block instructions",
                )
                .to_compiler_error(ErrorLocation::default()));
            }

            LirInst::Loop { instructions: _ } => {
                // Loop instructions should be handled via lower_loop method
                return Err(WasmGenerationError::instruction_lowering(
                    "Loop",
                    "Use lower_loop() method with ControlFlowManager for loop instructions",
                )
                .to_compiler_error(ErrorLocation::default()));
            }

            LirInst::If {
                then_branch: _,
                else_branch: _,
            } => {
                // If instructions should be handled via lower_if method
                return Err(WasmGenerationError::instruction_lowering(
                    "If",
                    "Use lower_if() method with ControlFlowManager for if instructions",
                )
                .to_compiler_error(ErrorLocation::default()));
            }

            LirInst::Br(target_depth) => {
                // Branch instructions should be handled via lower_branch method
                return Err(WasmGenerationError::instruction_lowering(
                    format!("Br({})", target_depth),
                    "Use lower_branch() method with ControlFlowManager for branch instructions",
                )
                .to_compiler_error(ErrorLocation::default()));
            }

            LirInst::BrIf(target_depth) => {
                // Conditional branch instructions should be handled via lower_branch_if method
                return Err(WasmGenerationError::instruction_lowering(
                    format!("BrIf({})", target_depth),
                    "Use lower_branch_if() method with ControlFlowManager for conditional branch instructions",
                )
                .to_compiler_error(ErrorLocation::default()));
            }

            // =========================================================================
            // Ownership Operations
            // These instructions implement Beanstalk's tagged pointer ownership system
            // =========================================================================
            LirInst::TagAsOwned(local) => {
                // Tag a local as owned: local = local | OWNERSHIP_BIT
                let wasm_local = self
                    .get_wasm_local(*local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalGet(wasm_local));
                function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
                function.instruction(&Instruction::I32Or);
                function.instruction(&Instruction::LocalSet(wasm_local));
            }

            LirInst::TagAsBorrowed(local) => {
                // Tag a local as borrowed: local = local & ALIGNMENT_MASK
                let wasm_local = self
                    .get_wasm_local(*local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalGet(wasm_local));
                function.instruction(&Instruction::I32Const(ALIGNMENT_MASK));
                function.instruction(&Instruction::I32And);
                function.instruction(&Instruction::LocalSet(wasm_local));
            }

            LirInst::MaskPointer => {
                // Extract real pointer from tagged pointer: stack_top & ALIGNMENT_MASK
                function.instruction(&Instruction::I32Const(ALIGNMENT_MASK));
                function.instruction(&Instruction::I32And);
            }

            LirInst::TestOwnership => {
                // Test ownership bit: stack_top & OWNERSHIP_BIT
                function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
                function.instruction(&Instruction::I32And);
            }

            LirInst::PossibleDrop(local) => {
                // Conditional drop based on ownership flag
                // This requires the OwnershipManager for proper handling
                return Err(WasmGenerationError::instruction_lowering(
                    format!("PossibleDrop({})", local),
                    "Use lower_possible_drop() method with OwnershipManager for possible_drop instructions",
                )
                .to_compiler_error(ErrorLocation::default()));
            }

            LirInst::PrepareOwnedArg(local) => {
                // Load local and set ownership bit: local | OWNERSHIP_BIT
                let wasm_local = self
                    .get_wasm_local(*local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalGet(wasm_local));
                function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
                function.instruction(&Instruction::I32Or);
            }

            LirInst::PrepareBorrowedArg(local) => {
                // Load local and clear ownership bit: local & ALIGNMENT_MASK
                let wasm_local = self
                    .get_wasm_local(*local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalGet(wasm_local));
                function.instruction(&Instruction::I32Const(ALIGNMENT_MASK));
                function.instruction(&Instruction::I32And);
            }

            LirInst::HandleOwnedParam {
                param_local,
                real_ptr_local,
            } => {
                // Extract real pointer and store in separate local
                let wasm_param = self
                    .get_wasm_local(*param_local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                let wasm_real_ptr = self
                    .get_wasm_local(*real_ptr_local)
                    .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
                function.instruction(&Instruction::LocalGet(wasm_param));
                function.instruction(&Instruction::I32Const(ALIGNMENT_MASK));
                function.instruction(&Instruction::I32And);
                function.instruction(&Instruction::LocalSet(wasm_real_ptr));
            }
        }

        Ok(())
    }

    // =========================================================================
    // Convenience Methods for Instruction Emission
    // =========================================================================

    /// Emit a LocalGet instruction for the given LIR local.
    pub fn emit_local_get(
        &self,
        lir_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let wasm_local = self
            .get_wasm_local(lir_local)
            .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
        function.instruction(&Instruction::LocalGet(wasm_local));
        Ok(())
    }

    /// Emit a LocalSet instruction for the given LIR local.
    pub fn emit_local_set(
        &self,
        lir_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let wasm_local = self
            .get_wasm_local(lir_local)
            .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
        function.instruction(&Instruction::LocalSet(wasm_local));
        Ok(())
    }

    /// Emit a LocalTee instruction for the given LIR local.
    /// LocalTee sets the local but also leaves the value on the stack.
    pub fn emit_local_tee(
        &self,
        lir_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let wasm_local = self
            .get_wasm_local(lir_local)
            .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
        function.instruction(&Instruction::LocalTee(wasm_local));
        Ok(())
    }

    // =========================================================================
    // Constant Emission Methods
    // =========================================================================

    /// Emit an I32 constant instruction.
    pub fn emit_i32_const(&self, value: i32, function: &mut Function) {
        function.instruction(&Instruction::I32Const(value));
    }

    /// Emit an I64 constant instruction.
    pub fn emit_i64_const(&self, value: i64, function: &mut Function) {
        function.instruction(&Instruction::I64Const(value));
    }

    /// Emit an F32 constant instruction.
    pub fn emit_f32_const(&self, value: f32, function: &mut Function) {
        function.instruction(&Instruction::F32Const(Ieee32::from(value)));
    }

    /// Emit an F64 constant instruction.
    pub fn emit_f64_const(&self, value: f64, function: &mut Function) {
        function.instruction(&Instruction::F64Const(Ieee64::from(value)));
    }

    // =========================================================================
    // Arithmetic Emission Methods
    // =========================================================================

    /// Emit an I32 addition instruction.
    pub fn emit_i32_add(&self, function: &mut Function) {
        function.instruction(&Instruction::I32Add);
    }

    /// Emit an I32 subtraction instruction.
    pub fn emit_i32_sub(&self, function: &mut Function) {
        function.instruction(&Instruction::I32Sub);
    }

    /// Emit an I32 multiplication instruction.
    pub fn emit_i32_mul(&self, function: &mut Function) {
        function.instruction(&Instruction::I32Mul);
    }

    /// Emit an I32 signed division instruction.
    pub fn emit_i32_div_s(&self, function: &mut Function) {
        function.instruction(&Instruction::I32DivS);
    }

    /// Emit an I64 addition instruction.
    pub fn emit_i64_add(&self, function: &mut Function) {
        function.instruction(&Instruction::I64Add);
    }

    /// Emit an I64 subtraction instruction.
    pub fn emit_i64_sub(&self, function: &mut Function) {
        function.instruction(&Instruction::I64Sub);
    }

    /// Emit an I64 multiplication instruction.
    pub fn emit_i64_mul(&self, function: &mut Function) {
        function.instruction(&Instruction::I64Mul);
    }

    /// Emit an I64 signed division instruction.
    pub fn emit_i64_div_s(&self, function: &mut Function) {
        function.instruction(&Instruction::I64DivS);
    }

    /// Emit an F64 addition instruction.
    pub fn emit_f64_add(&self, function: &mut Function) {
        function.instruction(&Instruction::F64Add);
    }

    /// Emit an F64 subtraction instruction.
    pub fn emit_f64_sub(&self, function: &mut Function) {
        function.instruction(&Instruction::F64Sub);
    }

    /// Emit an F64 multiplication instruction.
    pub fn emit_f64_mul(&self, function: &mut Function) {
        function.instruction(&Instruction::F64Mul);
    }

    /// Emit an F64 division instruction.
    pub fn emit_f64_div(&self, function: &mut Function) {
        function.instruction(&Instruction::F64Div);
    }

    // =========================================================================
    // Memory Operation Emission Methods
    // =========================================================================

    /// Emit an I32 load instruction with the given offset and alignment.
    pub fn emit_i32_load(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::I32Load(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an I32 store instruction with the given offset and alignment.
    pub fn emit_i32_store(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::I32Store(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an I64 load instruction with the given offset and alignment.
    pub fn emit_i64_load(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::I64Load(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an I64 store instruction with the given offset and alignment.
    pub fn emit_i64_store(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::I64Store(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an F32 load instruction with the given offset and alignment.
    pub fn emit_f32_load(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::F32Load(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an F32 store instruction with the given offset and alignment.
    pub fn emit_f32_store(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::F32Store(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an F64 load instruction with the given offset and alignment.
    pub fn emit_f64_load(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::F64Load(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    /// Emit an F64 store instruction with the given offset and alignment.
    pub fn emit_f64_store(&self, offset: u32, align: u32, function: &mut Function) {
        function.instruction(&Instruction::F64Store(MemArg {
            offset: offset as u64,
            align,
            memory_index: 0,
        }));
    }

    // =========================================================================
    // Struct Field Access Methods
    // These methods help with loading/storing struct fields using proper offsets
    // =========================================================================

    /// Load an I32 field from a struct at the given field offset.
    /// Assumes the struct base address is already on the stack.
    /// Stack: [i32 (base_addr)] -> [i32 (field_value)]
    pub fn emit_struct_field_load_i32(&self, field_offset: u32, function: &mut Function) {
        // Natural alignment for i32 is 2 (log2(4) = 2)
        function.instruction(&Instruction::I32Load(MemArg {
            offset: field_offset as u64,
            align: 2,
            memory_index: 0,
        }));
    }

    /// Store an I32 value to a struct field at the given field offset.
    /// Assumes the struct base address and value are on the stack.
    /// Stack: [i32 (base_addr), i32 (value)] -> []
    pub fn emit_struct_field_store_i32(&self, field_offset: u32, function: &mut Function) {
        function.instruction(&Instruction::I32Store(MemArg {
            offset: field_offset as u64,
            align: 2,
            memory_index: 0,
        }));
    }

    /// Load an I64 field from a struct at the given field offset.
    /// Assumes the struct base address is already on the stack.
    /// Stack: [i32 (base_addr)] -> [i64 (field_value)]
    pub fn emit_struct_field_load_i64(&self, field_offset: u32, function: &mut Function) {
        // Natural alignment for i64 is 3 (log2(8) = 3)
        function.instruction(&Instruction::I64Load(MemArg {
            offset: field_offset as u64,
            align: 3,
            memory_index: 0,
        }));
    }

    /// Store an I64 value to a struct field at the given field offset.
    /// Assumes the struct base address and value are on the stack.
    /// Stack: [i32 (base_addr), i64 (value)] -> []
    pub fn emit_struct_field_store_i64(&self, field_offset: u32, function: &mut Function) {
        function.instruction(&Instruction::I64Store(MemArg {
            offset: field_offset as u64,
            align: 3,
            memory_index: 0,
        }));
    }

    /// Load an F32 field from a struct at the given field offset.
    /// Assumes the struct base address is already on the stack.
    /// Stack: [i32 (base_addr)] -> [f32 (field_value)]
    pub fn emit_struct_field_load_f32(&self, field_offset: u32, function: &mut Function) {
        // Natural alignment for f32 is 2 (log2(4) = 2)
        function.instruction(&Instruction::F32Load(MemArg {
            offset: field_offset as u64,
            align: 2,
            memory_index: 0,
        }));
    }

    /// Store an F32 value to a struct field at the given field offset.
    /// Assumes the struct base address and value are on the stack.
    /// Stack: [i32 (base_addr), f32 (value)] -> []
    pub fn emit_struct_field_store_f32(&self, field_offset: u32, function: &mut Function) {
        function.instruction(&Instruction::F32Store(MemArg {
            offset: field_offset as u64,
            align: 2,
            memory_index: 0,
        }));
    }

    /// Load an F64 field from a struct at the given field offset.
    /// Assumes the struct base address is already on the stack.
    /// Stack: [i32 (base_addr)] -> [f64 (field_value)]
    pub fn emit_struct_field_load_f64(&self, field_offset: u32, function: &mut Function) {
        // Natural alignment for f64 is 3 (log2(8) = 3)
        function.instruction(&Instruction::F64Load(MemArg {
            offset: field_offset as u64,
            align: 3,
            memory_index: 0,
        }));
    }

    /// Store an F64 value to a struct field at the given field offset.
    /// Assumes the struct base address and value are on the stack.
    /// Stack: [i32 (base_addr), f64 (value)] -> []
    pub fn emit_struct_field_store_f64(&self, field_offset: u32, function: &mut Function) {
        function.instruction(&Instruction::F64Store(MemArg {
            offset: field_offset as u64,
            align: 3,
            memory_index: 0,
        }));
    }

    /// Get the natural alignment (as log2) for a given type size.
    /// Used for MemArg alignment field.
    pub fn natural_alignment_log2(type_size: u32) -> u32 {
        match type_size {
            1 => 0, // 2^0 = 1
            2 => 1, // 2^1 = 2
            4 => 2, // 2^2 = 4
            8 => 3, // 2^3 = 8
            _ => 0, // Default to byte alignment
        }
    }

    /// Validate that an alignment value is valid for WASM.
    /// Alignment must be a power of 2 and not exceed the natural alignment.
    pub fn validate_alignment(align: u32, natural_align: u32) -> bool {
        // Alignment must be a power of 2
        if align == 0 || (align & (align - 1)) != 0 {
            return false;
        }
        // Alignment must not exceed natural alignment
        align <= natural_align
    }

    // =========================================================================
    // Query Methods
    // =========================================================================

    /// Check if a LIR local is a parameter.
    pub fn is_parameter(&self, lir_local: u32) -> bool {
        lir_local < self.local_map.parameter_count
    }

    /// Get the parameter count from the local map.
    pub fn parameter_count(&self) -> u32 {
        self.local_map.parameter_count
    }

    /// Get a reference to the local map.
    pub fn local_map(&self) -> &LocalMap {
        &self.local_map
    }

    /// Get a reference to the function indices map.
    pub fn function_indices(&self) -> &HashMap<String, u32> {
        &self.function_indices
    }

    // =========================================================================
    // Control Flow Lowering Methods
    // These methods integrate with ControlFlowManager for proper block handling
    // =========================================================================

    /// Lower a block instruction with its body.
    ///
    /// This method handles the complete block structure including nested instructions.
    pub fn lower_block(
        &self,
        instructions: &[LirInst],
        result_type: Option<wasm_encoder::ValType>,
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        // Generate block instruction and enter block
        control_flow.generate_block(result_type, function)?;

        // Lower all instructions in the block body
        for inst in instructions {
            self.lower_instruction_with_control_flow(inst, function, control_flow)?;
        }

        // End the block
        control_flow.generate_end(function)?;

        Ok(())
    }

    /// Lower a loop instruction with its body.
    ///
    /// In WASM, a br instruction targeting a loop jumps to the start of the loop.
    pub fn lower_loop(
        &self,
        instructions: &[LirInst],
        result_type: Option<wasm_encoder::ValType>,
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        // Generate loop instruction and enter block
        control_flow.generate_loop(result_type, function)?;

        // Lower all instructions in the loop body
        for inst in instructions {
            self.lower_instruction_with_control_flow(inst, function, control_flow)?;
        }

        // End the loop
        control_flow.generate_end(function)?;

        Ok(())
    }

    /// Lower an if instruction with optional else branch.
    ///
    /// The condition should already be on the stack (i32).
    pub fn lower_if(
        &self,
        then_branch: &[LirInst],
        else_branch: Option<&[LirInst]>,
        result_type: Option<wasm_encoder::ValType>,
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        // Generate if instruction and enter block
        control_flow.generate_if(result_type, function)?;

        // Lower then branch instructions
        for inst in then_branch {
            self.lower_instruction_with_control_flow(inst, function, control_flow)?;
        }

        // Generate else branch if present
        if let Some(else_insts) = else_branch {
            control_flow.generate_else(function)?;
            for inst in else_insts {
                self.lower_instruction_with_control_flow(inst, function, control_flow)?;
            }
        }

        // End the if block
        control_flow.generate_end(function)?;

        Ok(())
    }

    /// Lower a branch instruction.
    ///
    /// The target_depth is relative to the current block.
    pub fn lower_branch(
        &self,
        target_depth: u32,
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        control_flow.generate_branch(target_depth, function)
    }

    /// Lower a conditional branch instruction.
    ///
    /// The condition should already be on the stack (i32).
    pub fn lower_branch_if(
        &self,
        target_depth: u32,
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        control_flow.generate_branch_if(target_depth, function)
    }

    /// Lower a single LIR instruction with control flow support.
    ///
    /// This method handles both regular instructions and control flow instructions.
    pub fn lower_instruction_with_control_flow(
        &self,
        inst: &LirInst,
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        match inst {
            // Control flow instructions - delegate to specialized methods
            LirInst::Block { instructions } => {
                self.lower_block(instructions, None, function, control_flow)
            }
            LirInst::Loop { instructions } => {
                self.lower_loop(instructions, None, function, control_flow)
            }
            LirInst::If {
                then_branch,
                else_branch,
            } => self.lower_if(
                then_branch,
                else_branch.as_deref(),
                None,
                function,
                control_flow,
            ),
            LirInst::Br(target_depth) => self.lower_branch(*target_depth, function, control_flow),
            LirInst::BrIf(target_depth) => {
                self.lower_branch_if(*target_depth, function, control_flow)
            }
            // All other instructions - use the regular lowering method
            _ => self.lower_instruction(inst, function),
        }
    }

    /// Lower a sequence of LIR instructions with control flow support.
    ///
    /// This is the main entry point for lowering a function body.
    pub fn lower_instructions(
        &self,
        instructions: &[LirInst],
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
    ) -> Result<(), CompilerError> {
        for inst in instructions {
            self.lower_instruction_with_control_flow(inst, function, control_flow)?;
        }
        Ok(())
    }

    // =========================================================================
    // Function Call Generation Methods
    // These methods handle direct function calls with proper argument loading
    // and result handling.
    // =========================================================================

    /// Emit a direct function call instruction.
    ///
    /// Arguments should already be on the stack in the correct order.
    /// Stack: [arg0, arg1, ..., argN] -> [result0, result1, ..., resultM]
    pub fn emit_call(&self, func_index: u32, function: &mut Function) {
        function.instruction(&Instruction::Call(func_index));
    }

    /// Emit a function call by name using the function indices map.
    ///
    /// Returns an error if the function name is not found.
    pub fn emit_call_by_name(
        &self,
        func_name: &str,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let func_index = self.function_indices.get(func_name).ok_or_else(|| {
            WasmGenerationError::lir_analysis(
                format!("Function '{}' not found in function indices", func_name),
                "emit_call_by_name",
            )
            .to_compiler_error(ErrorLocation::default())
        })?;

        function.instruction(&Instruction::Call(*func_index));
        Ok(())
    }

    /// Load arguments from locals and emit a function call.
    ///
    /// This is a convenience method that loads the specified locals onto the stack
    /// and then emits a call instruction.
    ///
    /// Stack: [] -> [result0, result1, ..., resultM]
    pub fn emit_call_with_local_args(
        &self,
        func_index: u32,
        arg_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Load all arguments onto the stack
        for &local in arg_locals {
            self.emit_local_get(local, function)?;
        }

        // Emit the call
        function.instruction(&Instruction::Call(func_index));
        Ok(())
    }

    /// Load arguments from locals by name and emit a function call.
    ///
    /// This combines local loading with function lookup by name.
    pub fn emit_call_by_name_with_local_args(
        &self,
        func_name: &str,
        arg_locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        let func_index = self.function_indices.get(func_name).ok_or_else(|| {
            WasmGenerationError::lir_analysis(
                format!("Function '{}' not found in function indices", func_name),
                "emit_call_by_name_with_local_args",
            )
            .to_compiler_error(ErrorLocation::default())
        })?;

        self.emit_call_with_local_args(*func_index, arg_locals, function)
    }

    /// Emit a function call and store the result in a local.
    ///
    /// This is useful for single-return functions where you want to
    /// immediately store the result.
    ///
    /// Stack: [arg0, arg1, ..., argN] -> []
    pub fn emit_call_and_store_result(
        &self,
        func_index: u32,
        result_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Emit the call (arguments should already be on stack)
        function.instruction(&Instruction::Call(func_index));

        // Store the result
        self.emit_local_set(result_local, function)?;
        Ok(())
    }

    /// Emit a function call, keeping the result on the stack (using tee).
    ///
    /// This stores the result in a local while also leaving it on the stack.
    ///
    /// Stack: [arg0, arg1, ..., argN] -> [result]
    pub fn emit_call_and_tee_result(
        &self,
        func_index: u32,
        result_local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Emit the call (arguments should already be on stack)
        function.instruction(&Instruction::Call(func_index));

        // Tee the result (store and keep on stack)
        self.emit_local_tee(result_local, function)?;
        Ok(())
    }

    /// Check if a function exists in the function indices map.
    pub fn has_function(&self, func_name: &str) -> bool {
        self.function_indices.contains_key(func_name)
    }

    /// Get the function index for a function name.
    pub fn get_function_index(&self, func_name: &str) -> Option<u32> {
        self.function_indices.get(func_name).copied()
    }

    /// Get the number of registered functions.
    pub fn function_count(&self) -> usize {
        self.function_indices.len()
    }

    // =========================================================================
    // Ownership Operation Methods
    // These methods handle Beanstalk's tagged pointer ownership system
    // =========================================================================

    /// Lower a possible_drop instruction with the OwnershipManager.
    ///
    /// This generates code that conditionally frees memory based on ownership.
    pub fn lower_possible_drop(
        &self,
        local: u32,
        function: &mut Function,
        ownership_manager: &crate::backends::wasm::ownership_manager::OwnershipManager,
    ) -> Result<(), CompilerError> {
        let wasm_local = self
            .get_wasm_local(local)
            .map_err(|e| e.to_compiler_error(ErrorLocation::default()))?;
        ownership_manager.generate_possible_drop(wasm_local, function)
    }

    /// Lower a single LIR instruction with ownership support.
    ///
    /// This method handles ownership instructions that require the OwnershipManager.
    pub fn lower_instruction_with_ownership(
        &self,
        inst: &LirInst,
        function: &mut Function,
        ownership_manager: &crate::backends::wasm::ownership_manager::OwnershipManager,
    ) -> Result<(), CompilerError> {
        match inst {
            LirInst::PossibleDrop(local) => {
                self.lower_possible_drop(*local, function, ownership_manager)
            }
            // All other instructions - use the regular lowering method
            _ => self.lower_instruction(inst, function),
        }
    }

    /// Lower a sequence of LIR instructions with full support (control flow + ownership).
    ///
    /// This is the most complete entry point for lowering a function body.
    pub fn lower_instructions_full(
        &self,
        instructions: &[LirInst],
        function: &mut Function,
        control_flow: &mut crate::backends::wasm::control_flow::ControlFlowManager,
        ownership_manager: &crate::backends::wasm::ownership_manager::OwnershipManager,
    ) -> Result<(), CompilerError> {
        for inst in instructions {
            match inst {
                // Control flow instructions - delegate to specialized methods
                LirInst::Block { instructions } => {
                    self.lower_block(instructions, None, function, control_flow)?;
                }
                LirInst::Loop { instructions } => {
                    self.lower_loop(instructions, None, function, control_flow)?;
                }
                LirInst::If {
                    then_branch,
                    else_branch,
                } => {
                    self.lower_if(
                        then_branch,
                        else_branch.as_deref(),
                        None,
                        function,
                        control_flow,
                    )?;
                }
                LirInst::Br(target_depth) => {
                    self.lower_branch(*target_depth, function, control_flow)?;
                }
                LirInst::BrIf(target_depth) => {
                    self.lower_branch_if(*target_depth, function, control_flow)?;
                }
                // Ownership instructions - delegate to ownership manager
                LirInst::PossibleDrop(local) => {
                    self.lower_possible_drop(*local, function, ownership_manager)?;
                }
                // All other instructions - use the regular lowering method
                _ => {
                    self.lower_instruction(inst, function)?;
                }
            }
        }
        Ok(())
    }

    // =========================================================================
    // Return Handling Methods
    // These methods handle proper return statement generation with stack management
    // =========================================================================

    /// Emit a return instruction.
    ///
    /// The return values should already be on the stack in the correct order.
    /// Stack: [return_value0, return_value1, ...] -> (function exits)
    pub fn emit_return(&self, function: &mut Function) {
        function.instruction(&Instruction::Return);
    }

    /// Emit a return with a single i32 value from a local.
    ///
    /// This loads the local onto the stack and then returns.
    /// Stack: [] -> (function exits with i32 value)
    pub fn emit_return_i32_local(
        &self,
        local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        self.emit_local_get(local, function)?;
        function.instruction(&Instruction::Return);
        Ok(())
    }

    /// Emit a return with a single i64 value from a local.
    ///
    /// This loads the local onto the stack and then returns.
    /// Stack: [] -> (function exits with i64 value)
    pub fn emit_return_i64_local(
        &self,
        local: u32,
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        self.emit_local_get(local, function)?;
        function.instruction(&Instruction::Return);
        Ok(())
    }

    /// Emit a return with a constant i32 value.
    ///
    /// Stack: [] -> (function exits with i32 value)
    pub fn emit_return_i32_const(&self, value: i32, function: &mut Function) {
        function.instruction(&Instruction::I32Const(value));
        function.instruction(&Instruction::Return);
    }

    /// Emit a return with a constant i64 value.
    ///
    /// Stack: [] -> (function exits with i64 value)
    pub fn emit_return_i64_const(&self, value: i64, function: &mut Function) {
        function.instruction(&Instruction::I64Const(value));
        function.instruction(&Instruction::Return);
    }

    /// Emit a return with a constant f32 value.
    ///
    /// Stack: [] -> (function exits with f32 value)
    pub fn emit_return_f32_const(&self, value: f32, function: &mut Function) {
        function.instruction(&Instruction::F32Const(Ieee32::from(value)));
        function.instruction(&Instruction::Return);
    }

    /// Emit a return with a constant f64 value.
    ///
    /// Stack: [] -> (function exits with f64 value)
    pub fn emit_return_f64_const(&self, value: f64, function: &mut Function) {
        function.instruction(&Instruction::F64Const(Ieee64::from(value)));
        function.instruction(&Instruction::Return);
    }

    /// Emit return values from multiple locals.
    ///
    /// This loads all specified locals onto the stack and then returns.
    /// Useful for multi-value returns.
    /// Stack: [] -> (function exits with multiple values)
    pub fn emit_return_from_locals(
        &self,
        locals: &[u32],
        function: &mut Function,
    ) -> Result<(), CompilerError> {
        for &local in locals {
            self.emit_local_get(local, function)?;
        }
        function.instruction(&Instruction::Return);
        Ok(())
    }

    /// Emit default return values for the given types.
    ///
    /// This is useful when a function needs to return but doesn't have
    /// explicit return values (e.g., at the end of a function body).
    /// Stack: [] -> [default_values...]
    pub fn emit_default_return_values(
        &self,
        return_types: &[wasm_encoder::ValType],
        function: &mut Function,
    ) {
        for return_type in return_types {
            match return_type {
                wasm_encoder::ValType::I32 => {
                    function.instruction(&Instruction::I32Const(0));
                }
                wasm_encoder::ValType::I64 => {
                    function.instruction(&Instruction::I64Const(0));
                }
                wasm_encoder::ValType::F32 => {
                    function.instruction(&Instruction::F32Const(Ieee32::from(0.0_f32)));
                }
                wasm_encoder::ValType::F64 => {
                    function.instruction(&Instruction::F64Const(Ieee64::from(0.0_f64)));
                }
                _ => {
                    // For reference types, we'd need to handle differently
                    // For now, just emit i32 0 as a placeholder
                    function.instruction(&Instruction::I32Const(0));
                }
            }
        }
    }

    /// Check if a sequence of instructions ends with a return.
    ///
    /// This is useful for determining if we need to add implicit return handling.
    pub fn ends_with_return(instructions: &[LirInst]) -> bool {
        instructions
            .last()
            .map_or(false, |inst| matches!(inst, LirInst::Return))
    }

    /// Check if a sequence of instructions ends with an unconditional branch or return.
    ///
    /// This is useful for determining if control flow can fall through.
    pub fn ends_with_terminator(instructions: &[LirInst]) -> bool {
        instructions.last().map_or(false, |inst| {
            matches!(inst, LirInst::Return | LirInst::Br(_))
        })
    }
}
