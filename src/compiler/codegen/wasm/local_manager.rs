//! Local Variable Manager
//!
//! This module manages the mapping between LIR locals and WASM locals,
//! including optimization and type grouping. It handles:
//! - Local variable analysis for functions
//! - Building local mapping between LIR and WASM indices
//! - Support for v0.243.0 API format IntoIterator<Item = (u32, ValType)>
//! - Proper parameter vs local variable ordering

// Many methods are prepared for later implementation phases
// (full integration with encode.rs)
#![allow(dead_code)]

use crate::compiler::codegen::wasm::analyzer::{LocalMap, WasmType};
use crate::compiler::lir::nodes::LirFunction;
use std::collections::HashMap;
use wasm_encoder::{Function, ValType};

/// Manages local variable mapping and optimization for a single function.
///
/// The LocalVariableManager handles the translation between LIR local indices
/// and WASM local indices, ensuring proper ordering (parameters first, then locals)
/// and type grouping for efficient WASM representation.
///
/// WASM local space layout:
/// - Indices 0..param_count: Function parameters
/// - Indices param_count..: Local variables (grouped by type)
pub struct LocalVariableManager {
    /// Maps LIR local ID to its WasmType
    local_types: HashMap<u32, WasmType>,
    /// Maps LIR local ID to WASM local index
    lir_to_wasm: HashMap<u32, u32>,
    /// Number of function parameters
    parameter_count: u32,
    /// Total number of locals (excluding parameters)
    local_count: u32,
    /// WASM locals in v0.243.0 format: (count, type) pairs
    wasm_locals: Vec<(u32, ValType)>,
}

impl LocalVariableManager {
    /// Analyze a LIR function and create a local variable manager.
    ///
    /// This method:
    /// 1. Extracts parameter types and assigns them indices 0..param_count
    /// 2. Extracts local variable types
    /// 3. Groups locals by type for efficient WASM representation
    /// 4. Builds the LIR to WASM index mapping
    pub fn analyze_function(lir_func: &LirFunction) -> Self {
        let parameter_count = lir_func.params.len() as u32;
        let local_count = lir_func.locals.len() as u32;

        let mut local_types = HashMap::new();
        let mut lir_to_wasm = HashMap::new();

        // Parameters are indexed first (0..param_count)
        // Parameters map directly: LIR index == WASM index
        for (index, param_type) in lir_func.params.iter().enumerate() {
            let wasm_type = WasmType::from_lir_type(*param_type);
            local_types.insert(index as u32, wasm_type);
            lir_to_wasm.insert(index as u32, index as u32);
        }

        // Collect local variables by type for grouping
        // WASM locals section uses (count, type) pairs, so grouping by type is more efficient
        let mut type_groups: HashMap<WasmType, Vec<u32>> = HashMap::new();

        for (index, local_type) in lir_func.locals.iter().enumerate() {
            let lir_index = parameter_count + index as u32;
            let wasm_type = WasmType::from_lir_type(*local_type);
            local_types.insert(lir_index, wasm_type);
            type_groups.entry(wasm_type).or_default().push(lir_index);
        }

        // Build WASM locals and assign indices
        // Order: I32, I64, F32, F64 for consistency
        let type_order = [WasmType::I32, WasmType::I64, WasmType::F32, WasmType::F64];
        let mut wasm_locals = Vec::new();
        let mut next_wasm_index = parameter_count;

        for wasm_type in type_order {
            if let Some(lir_locals) = type_groups.get(&wasm_type) {
                let count = lir_locals.len() as u32;
                if count > 0 {
                    wasm_locals.push((count, wasm_type.to_val_type()));

                    // Assign WASM indices to each LIR local in this type group
                    for &lir_index in lir_locals {
                        lir_to_wasm.insert(lir_index, next_wasm_index);
                        next_wasm_index += 1;
                    }
                }
            }
        }

        LocalVariableManager {
            local_types,
            lir_to_wasm,
            parameter_count,
            local_count,
            wasm_locals,
        }
    }

    /// Generate WASM locals in the v0.243.0 API format.
    ///
    /// Returns Vec<(u32, ValType)> where each tuple is (count, type).
    /// This format is used directly by wasm_encoder's Function::new().
    ///
    /// Note: This only includes non-parameter locals. Parameters are part
    /// of the function signature, not the locals section.
    pub fn generate_wasm_locals(&self) -> Vec<(u32, ValType)> {
        self.wasm_locals.clone()
    }

    /// Build a LocalMap for use by other codegen components.
    ///
    /// The LocalMap provides:
    /// - LIR to WASM index mapping
    /// - Parameter count for distinguishing params from locals
    /// - WASM locals format for function creation
    pub fn build_local_mapping(&self) -> LocalMap {
        LocalMap {
            lir_to_wasm: self.lir_to_wasm.clone(),
            parameter_count: self.parameter_count,
            local_types: self.wasm_locals.clone(),
        }
    }

    /// Get WASM local index for a LIR local ID.
    ///
    /// Returns None if the LIR local ID is not found in the mapping.
    pub fn get_wasm_local_index(&self, lir_local: u32) -> Option<u32> {
        self.lir_to_wasm.get(&lir_local).copied()
    }

    /// Check if a LIR local ID is a parameter.
    pub fn is_parameter(&self, lir_local: u32) -> bool {
        lir_local < self.parameter_count
    }

    /// Get the type of a LIR local.
    pub fn get_local_type(&self, lir_local: u32) -> Option<WasmType> {
        self.local_types.get(&lir_local).copied()
    }

    /// Get the number of function parameters.
    pub fn parameter_count(&self) -> u32 {
        self.parameter_count
    }

    /// Get the number of local variables (excluding parameters).
    pub fn local_count(&self) -> u32 {
        self.local_count
    }

    /// Get the total number of locals (parameters + local variables).
    pub fn total_count(&self) -> u32 {
        self.parameter_count + self.local_count
    }

    /// Create a WASM function with proper locals.
    ///
    /// This creates a new wasm_encoder::Function with the locals section
    /// properly initialized using the v0.243.0 API format.
    pub fn create_function_with_locals(&self) -> Function {
        Function::new(self.wasm_locals.clone())
    }

    /// Validate that all LIR locals have valid WASM mappings.
    ///
    /// Returns true if all locals from 0 to (parameter_count + local_count - 1)
    /// have valid mappings.
    pub fn validate_mappings(&self) -> bool {
        let total = self.parameter_count + self.local_count;
        for i in 0..total {
            if !self.lir_to_wasm.contains_key(&i) {
                return false;
            }
        }
        true
    }
}
