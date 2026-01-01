//! WASM Instruction Optimizer
//!
//! This module provides optimization passes for WASM code generation.
//! It handles:
//! - Instruction optimization (numeric instruction selection, constant folding)
//! - Local access optimization (minimizing stack operations)
//! - Local type grouping for minimal section size
//!
//! Requirements: 7.1, 7.2, 7.5, 7.6

// Many methods are prepared for later implementation phases
#![allow(dead_code)]

use crate::compiler::lir::nodes::{LirInst, LirType};
use std::collections::HashMap;
use wasm_encoder::ValType;

/// Instruction optimizer for WASM code generation.
///
/// Provides optimization passes that can be applied to LIR instructions
/// before lowering to WASM bytecode. Optimizations include:
/// - Constant folding for arithmetic operations
/// - Immediate value usage for small constants
/// - Instruction count minimization
/// - Redundant operation elimination
pub struct InstructionOptimizer {
    /// Track constant values for locals (for constant propagation)
    constant_locals: HashMap<u32, ConstantValue>,
    /// Statistics for optimization passes
    stats: OptimizationStats,
}

/// Represents a constant value that can be tracked during optimization
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ConstantValue {
    I32(i32),
    I64(i64),
    F32(f32),
    F64(f64),
}

impl ConstantValue {
    /// Create a constant value from a LIR instruction if it's a constant
    pub fn from_lir_inst(inst: &LirInst) -> Option<Self> {
        match inst {
            LirInst::I32Const(v) => Some(ConstantValue::I32(*v)),
            LirInst::I64Const(v) => Some(ConstantValue::I64(*v)),
            LirInst::F32Const(v) => Some(ConstantValue::F32(*v)),
            LirInst::F64Const(v) => Some(ConstantValue::F64(*v)),
            _ => None,
        }
    }

    /// Convert to LIR instruction
    pub fn to_lir_inst(&self) -> LirInst {
        match self {
            ConstantValue::I32(v) => LirInst::I32Const(*v),
            ConstantValue::I64(v) => LirInst::I64Const(*v),
            ConstantValue::F32(v) => LirInst::F32Const(*v),
            ConstantValue::F64(v) => LirInst::F64Const(*v),
        }
    }

    /// Check if this is a zero value (useful for identity optimizations)
    pub fn is_zero(&self) -> bool {
        match self {
            ConstantValue::I32(v) => *v == 0,
            ConstantValue::I64(v) => *v == 0,
            ConstantValue::F32(v) => *v == 0.0,
            ConstantValue::F64(v) => *v == 0.0,
        }
    }

    /// Check if this is a one value (useful for identity optimizations)
    pub fn is_one(&self) -> bool {
        match self {
            ConstantValue::I32(v) => *v == 1,
            ConstantValue::I64(v) => *v == 1,
            ConstantValue::F32(v) => *v == 1.0,
            ConstantValue::F64(v) => *v == 1.0,
        }
    }

    /// Check if this constant can use a smaller immediate encoding
    /// WASM uses LEB128 encoding, so small values are more efficient
    pub fn is_small_immediate(&self) -> bool {
        match self {
            // Values that fit in 7 bits (single byte LEB128)
            ConstantValue::I32(v) => *v >= -64 && *v <= 63,
            ConstantValue::I64(v) => *v >= -64 && *v <= 63,
            // Floats don't have small immediate optimization
            ConstantValue::F32(_) | ConstantValue::F64(_) => false,
        }
    }
}

/// Statistics for tracking optimization effectiveness
#[derive(Debug, Default, Clone)]
pub struct OptimizationStats {
    /// Number of constant folding operations performed
    pub constants_folded: u32,
    /// Number of identity operations removed (x + 0, x * 1, etc.)
    pub identity_ops_removed: u32,
    /// Number of redundant loads eliminated
    pub redundant_loads_eliminated: u32,
    /// Number of redundant stores eliminated
    pub redundant_stores_eliminated: u32,
    /// Total instructions before optimization
    pub instructions_before: u32,
    /// Total instructions after optimization
    pub instructions_after: u32,
}

impl OptimizationStats {
    /// Calculate the reduction percentage
    pub fn reduction_percentage(&self) -> f64 {
        if self.instructions_before == 0 {
            return 0.0;
        }
        let reduced = self
            .instructions_before
            .saturating_sub(self.instructions_after);
        (reduced as f64 / self.instructions_before as f64) * 100.0
    }
}

impl InstructionOptimizer {
    /// Create a new instruction optimizer
    pub fn new() -> Self {
        InstructionOptimizer {
            constant_locals: HashMap::new(),
            stats: OptimizationStats::default(),
        }
    }

    /// Get optimization statistics
    pub fn stats(&self) -> &OptimizationStats {
        &self.stats
    }

    /// Reset the optimizer state (call between functions)
    pub fn reset(&mut self) {
        self.constant_locals.clear();
    }

    /// Optimize a sequence of LIR instructions.
    ///
    /// This is the main entry point for instruction optimization.
    /// It applies multiple optimization passes:
    /// 1. Constant folding
    /// 2. Identity operation removal
    /// 3. Redundant operation elimination
    pub fn optimize_instructions(&mut self, instructions: Vec<LirInst>) -> Vec<LirInst> {
        self.stats.instructions_before += instructions.len() as u32;

        let mut result = instructions;

        // Pass 1: Constant folding and propagation
        result = self.constant_folding_pass(result);

        // Pass 2: Identity operation removal
        result = self.identity_removal_pass(result);

        // Pass 3: Peephole optimizations
        result = self.peephole_pass(result);

        self.stats.instructions_after += result.len() as u32;
        result
    }

    /// Constant folding pass - evaluate constant expressions at compile time
    fn constant_folding_pass(&mut self, instructions: Vec<LirInst>) -> Vec<LirInst> {
        let mut result = Vec::with_capacity(instructions.len());
        let mut pending_constants: Vec<ConstantValue> = Vec::new();

        for inst in instructions {
            match &inst {
                // Track constants pushed onto the stack
                LirInst::I32Const(_)
                | LirInst::I64Const(_)
                | LirInst::F32Const(_)
                | LirInst::F64Const(_) => {
                    if let Some(cv) = ConstantValue::from_lir_inst(&inst) {
                        pending_constants.push(cv);
                    }
                    result.push(inst);
                }

                // Try to fold binary I32 operations
                LirInst::I32Add | LirInst::I32Sub | LirInst::I32Mul | LirInst::I32DivS => {
                    if pending_constants.len() >= 2 {
                        let b = pending_constants.pop().unwrap();
                        let a = pending_constants.pop().unwrap();

                        if let (ConstantValue::I32(av), ConstantValue::I32(bv)) = (a, b) {
                            let folded = match &inst {
                                LirInst::I32Add => Some(av.wrapping_add(bv)),
                                LirInst::I32Sub => Some(av.wrapping_sub(bv)),
                                LirInst::I32Mul => Some(av.wrapping_mul(bv)),
                                LirInst::I32DivS => {
                                    if bv != 0 {
                                        Some(av.wrapping_div(bv))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some(value) = folded {
                                // Remove the two constant instructions and replace with folded result
                                result.pop(); // Remove second constant
                                result.pop(); // Remove first constant
                                let folded_inst = LirInst::I32Const(value);
                                pending_constants.push(ConstantValue::I32(value));
                                result.push(folded_inst);
                                self.stats.constants_folded += 1;
                                continue;
                            }
                        }
                        // Couldn't fold, restore constants
                        pending_constants.push(a);
                        pending_constants.push(b);
                    }
                    pending_constants.clear(); // Operation consumes stack values
                    result.push(inst);
                }

                // Try to fold binary I64 operations
                LirInst::I64Add | LirInst::I64Sub | LirInst::I64Mul | LirInst::I64DivS => {
                    if pending_constants.len() >= 2 {
                        let b = pending_constants.pop().unwrap();
                        let a = pending_constants.pop().unwrap();

                        if let (ConstantValue::I64(av), ConstantValue::I64(bv)) = (a, b) {
                            let folded = match &inst {
                                LirInst::I64Add => Some(av.wrapping_add(bv)),
                                LirInst::I64Sub => Some(av.wrapping_sub(bv)),
                                LirInst::I64Mul => Some(av.wrapping_mul(bv)),
                                LirInst::I64DivS => {
                                    if bv != 0 {
                                        Some(av.wrapping_div(bv))
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some(value) = folded {
                                result.pop();
                                result.pop();
                                let folded_inst = LirInst::I64Const(value);
                                pending_constants.push(ConstantValue::I64(value));
                                result.push(folded_inst);
                                self.stats.constants_folded += 1;
                                continue;
                            }
                        }
                        pending_constants.push(a);
                        pending_constants.push(b);
                    }
                    pending_constants.clear();
                    result.push(inst);
                }

                // Try to fold binary F64 operations
                LirInst::F64Add | LirInst::F64Sub | LirInst::F64Mul | LirInst::F64Div => {
                    if pending_constants.len() >= 2 {
                        let b = pending_constants.pop().unwrap();
                        let a = pending_constants.pop().unwrap();

                        if let (ConstantValue::F64(av), ConstantValue::F64(bv)) = (a, b) {
                            let folded = match &inst {
                                LirInst::F64Add => Some(av + bv),
                                LirInst::F64Sub => Some(av - bv),
                                LirInst::F64Mul => Some(av * bv),
                                LirInst::F64Div => {
                                    if bv != 0.0 {
                                        Some(av / bv)
                                    } else {
                                        None
                                    }
                                }
                                _ => None,
                            };

                            if let Some(value) = folded {
                                result.pop();
                                result.pop();
                                let folded_inst = LirInst::F64Const(value);
                                pending_constants.push(ConstantValue::F64(value));
                                result.push(folded_inst);
                                self.stats.constants_folded += 1;
                                continue;
                            }
                        }
                        pending_constants.push(a);
                        pending_constants.push(b);
                    }
                    pending_constants.clear();
                    result.push(inst);
                }

                // Track constant assignments to locals
                LirInst::LocalSet(local) => {
                    if let Some(cv) = pending_constants.pop() {
                        self.constant_locals.insert(*local, cv);
                    } else {
                        // Non-constant assignment invalidates tracking
                        self.constant_locals.remove(local);
                    }
                    pending_constants.clear();
                    result.push(inst);
                }

                // LocalTee also tracks constants
                LirInst::LocalTee(local) => {
                    if let Some(&cv) = pending_constants.last() {
                        self.constant_locals.insert(*local, cv);
                    } else {
                        self.constant_locals.remove(local);
                    }
                    result.push(inst);
                }

                // Other instructions clear pending constants
                _ => {
                    pending_constants.clear();
                    result.push(inst);
                }
            }
        }

        result
    }

    /// Identity operation removal pass
    /// Removes operations like x + 0, x * 1, x - 0, x / 1
    fn identity_removal_pass(&mut self, instructions: Vec<LirInst>) -> Vec<LirInst> {
        let mut result = Vec::with_capacity(instructions.len());
        let mut i = 0;

        while i < instructions.len() {
            // Look for patterns: const 0/1 followed by add/sub/mul/div
            if i + 1 < instructions.len() {
                let current = &instructions[i];
                let next = &instructions[i + 1];

                // Check for identity patterns
                if let Some(cv) = ConstantValue::from_lir_inst(current) {
                    let is_identity = match next {
                        // x + 0 = x, 0 + x = x
                        LirInst::I32Add | LirInst::I64Add | LirInst::F64Add => cv.is_zero(),
                        // x - 0 = x (but not 0 - x)
                        LirInst::I32Sub | LirInst::I64Sub | LirInst::F64Sub => cv.is_zero(),
                        // x * 1 = x, 1 * x = x
                        LirInst::I32Mul | LirInst::I64Mul | LirInst::F64Mul => cv.is_one(),
                        // x / 1 = x
                        LirInst::I32DivS | LirInst::I64DivS | LirInst::F64Div => cv.is_one(),
                        _ => false,
                    };

                    if is_identity {
                        // Skip both the constant and the operation
                        self.stats.identity_ops_removed += 1;
                        i += 2;
                        continue;
                    }
                }
            }

            result.push(instructions[i].clone());
            i += 1;
        }

        result
    }

    /// Peephole optimization pass
    /// Looks for small patterns that can be optimized
    fn peephole_pass(&mut self, instructions: Vec<LirInst>) -> Vec<LirInst> {
        let mut result = Vec::with_capacity(instructions.len());
        let mut i = 0;

        while i < instructions.len() {
            // Pattern: LocalGet(x) followed by LocalSet(x) - redundant store
            if i + 1 < instructions.len() {
                if let (LirInst::LocalGet(get_local), LirInst::LocalSet(set_local)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    if get_local == set_local {
                        // Skip both - this is a no-op
                        self.stats.redundant_stores_eliminated += 1;
                        i += 2;
                        continue;
                    }
                }
            }

            // Pattern: LocalSet(x) followed immediately by LocalGet(x) - use LocalTee
            if i + 1 < instructions.len() {
                if let (LirInst::LocalSet(set_local), LirInst::LocalGet(get_local)) =
                    (&instructions[i], &instructions[i + 1])
                {
                    if set_local == get_local {
                        // Replace with LocalTee
                        result.push(LirInst::LocalTee(*set_local));
                        self.stats.redundant_loads_eliminated += 1;
                        i += 2;
                        continue;
                    }
                }
            }

            // Pattern: Drop followed by constant - the constant was unused
            // (This is a simple dead code elimination)
            if i + 1 < instructions.len() {
                if let LirInst::Drop = &instructions[i] {
                    // Check if previous instruction was a constant
                    if !result.is_empty() {
                        if let Some(prev) = result.last() {
                            if ConstantValue::from_lir_inst(prev).is_some() {
                                // Remove the constant and skip the drop
                                result.pop();
                                i += 1;
                                continue;
                            }
                        }
                    }
                }
            }

            result.push(instructions[i].clone());
            i += 1;
        }

        result
    }

    /// Check if an instruction is a simple constant load
    pub fn is_constant_instruction(inst: &LirInst) -> bool {
        matches!(
            inst,
            LirInst::I32Const(_)
                | LirInst::I64Const(_)
                | LirInst::F32Const(_)
                | LirInst::F64Const(_)
        )
    }

    /// Check if an instruction is a local access
    pub fn is_local_access(inst: &LirInst) -> bool {
        matches!(
            inst,
            LirInst::LocalGet(_) | LirInst::LocalSet(_) | LirInst::LocalTee(_)
        )
    }
}

impl Default for InstructionOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

// =========================================================================
// Local Access Optimizer
// =========================================================================

/// Optimizer for local variable access patterns.
///
/// This optimizer focuses on:
/// - Minimizing unnecessary stack operations
/// - Optimizing local access patterns
/// - Local type grouping for minimal WASM section size
pub struct LocalAccessOptimizer {
    /// Track last access type for each local (for access pattern optimization)
    last_access: HashMap<u32, LocalAccessType>,
    /// Statistics for local access optimization
    stats: LocalAccessStats,
}

/// Type of local access
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum LocalAccessType {
    Get,
    Set,
    Tee,
}

/// Statistics for local access optimization
#[derive(Debug, Default, Clone)]
pub struct LocalAccessStats {
    /// Number of get-after-set patterns converted to tee
    pub get_after_set_optimized: u32,
    /// Number of redundant gets eliminated
    pub redundant_gets_eliminated: u32,
    /// Number of redundant sets eliminated
    pub redundant_sets_eliminated: u32,
}

impl LocalAccessOptimizer {
    /// Create a new local access optimizer
    pub fn new() -> Self {
        LocalAccessOptimizer {
            last_access: HashMap::new(),
            stats: LocalAccessStats::default(),
        }
    }

    /// Get optimization statistics
    pub fn stats(&self) -> &LocalAccessStats {
        &self.stats
    }

    /// Reset the optimizer state
    pub fn reset(&mut self) {
        self.last_access.clear();
    }

    /// Optimize local access patterns in a sequence of instructions
    pub fn optimize_local_access(&mut self, instructions: Vec<LirInst>) -> Vec<LirInst> {
        let mut result = Vec::with_capacity(instructions.len());
        let mut i = 0;

        while i < instructions.len() {
            let inst = &instructions[i];

            // Pattern: LocalSet(x) followed by LocalGet(x) -> LocalTee(x)
            if i + 1 < instructions.len() {
                if let LirInst::LocalSet(set_local) = inst {
                    if let LirInst::LocalGet(get_local) = &instructions[i + 1] {
                        if set_local == get_local {
                            result.push(LirInst::LocalTee(*set_local));
                            self.stats.get_after_set_optimized += 1;
                            i += 2;
                            continue;
                        }
                    }
                }
            }

            // Pattern: LocalGet(x) followed by LocalGet(x) - could use LocalTee earlier
            // This is informational - we track it but don't change it here
            if let LirInst::LocalGet(local) = inst {
                self.last_access.insert(*local, LocalAccessType::Get);
            } else if let LirInst::LocalSet(local) = inst {
                self.last_access.insert(*local, LocalAccessType::Set);
            } else if let LirInst::LocalTee(local) = inst {
                self.last_access.insert(*local, LocalAccessType::Tee);
            }

            result.push(inst.clone());
            i += 1;
        }

        result
    }
}

impl Default for LocalAccessOptimizer {
    fn default() -> Self {
        Self::new()
    }
}

// =========================================================================
// Local Type Grouping
// =========================================================================

/// Groups local variables by type for minimal WASM section size.
///
/// WASM's local section uses (count, type) pairs, so grouping locals
/// by type reduces the section size.
pub struct LocalTypeGrouper;

impl LocalTypeGrouper {
    /// Group local types for optimal WASM encoding.
    ///
    /// Takes a list of local types and returns grouped (count, type) pairs.
    /// Types are ordered consistently: I32, I64, F32, F64
    pub fn group_locals(locals: &[LirType]) -> Vec<(u32, ValType)> {
        let mut type_counts: HashMap<LirType, u32> = HashMap::new();

        for local_type in locals {
            *type_counts.entry(*local_type).or_insert(0) += 1;
        }

        // Order types consistently for deterministic output
        let type_order = [LirType::I32, LirType::I64, LirType::F32, LirType::F64];
        let mut result = Vec::new();

        for lir_type in type_order {
            if let Some(&count) = type_counts.get(&lir_type) {
                if count > 0 {
                    let val_type = match lir_type {
                        LirType::I32 => ValType::I32,
                        LirType::I64 => ValType::I64,
                        LirType::F32 => ValType::F32,
                        LirType::F64 => ValType::F64,
                    };
                    result.push((count, val_type));
                }
            }
        }

        result
    }

    /// Calculate the WASM section size for grouped locals.
    ///
    /// Returns the approximate byte size of the locals section.
    pub fn calculate_section_size(grouped: &[(u32, ValType)]) -> u32 {
        // Each entry is: LEB128(count) + 1 byte for type
        // LEB128 for small counts is typically 1 byte
        let mut size = 0u32;

        // Vector length (LEB128)
        size += Self::leb128_size(grouped.len() as u32);

        for (count, _) in grouped {
            // Count (LEB128)
            size += Self::leb128_size(*count);
            // Type (1 byte)
            size += 1;
        }

        size
    }

    /// Calculate LEB128 encoded size for a u32 value
    fn leb128_size(value: u32) -> u32 {
        if value < 128 {
            1
        } else if value < 16384 {
            2
        } else if value < 2097152 {
            3
        } else if value < 268435456 {
            4
        } else {
            5
        }
    }

    /// Compare two groupings and return which is more efficient
    pub fn compare_groupings(a: &[(u32, ValType)], b: &[(u32, ValType)]) -> std::cmp::Ordering {
        let size_a = Self::calculate_section_size(a);
        let size_b = Self::calculate_section_size(b);
        size_a.cmp(&size_b)
    }
}
