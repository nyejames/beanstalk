//! Memory Layout Calculator
//!
//! This module handles Beanstalk's memory model in WASM, including:
//! - Struct layouts with field offset calculation
//! - Alignment requirement handling
//! - Struct size and padding calculation
//! - Tagged pointer operations for ownership system

// Many methods are prepared for later implementation phases
// (ownership system, memory model integration)
#![allow(dead_code)]

use crate::backends::lir::nodes::{LirStruct, LirType};
use crate::compiler_frontend::codegen::wasm::analyzer::{FieldLayout, StructLayout, WasmType};
use crate::compiler_frontend::compiler_errors::CompilerError;
use std::collections::HashMap;
use wasm_encoder::Function;

/// Calculates memory layouts for structs and handles alignment requirements.
///
/// The calculator ensures:
/// - Proper field alignment for efficient memory access
/// - Minimum 2-byte alignment for tagged pointer support
/// - Correct padding between fields and at struct end
pub struct MemoryLayoutCalculator {
    /// Cached struct layouts by name
    struct_layouts: HashMap<String, StructLayout>,
    /// Alignment requirements for each WASM type
    alignment_requirements: HashMap<WasmType, u32>,
}

impl MemoryLayoutCalculator {
    /// Create a new memory layout calculator with default alignment requirements
    pub fn new() -> Self {
        let mut alignment_requirements = HashMap::new();
        alignment_requirements.insert(WasmType::I32, 4);
        alignment_requirements.insert(WasmType::I64, 8);
        alignment_requirements.insert(WasmType::F32, 4);
        alignment_requirements.insert(WasmType::F64, 8);

        MemoryLayoutCalculator {
            struct_layouts: HashMap::new(),
            alignment_requirements,
        }
    }

    /// Calculate struct layout with proper alignment and padding.
    ///
    /// This method:
    /// 1. Iterates through fields in order
    /// 2. Aligns each field to its natural alignment
    /// 3. Tracks maximum alignment for struct-level alignment
    /// 4. Ensures minimum 2-byte alignment for tagged pointer support
    /// 5. Pads total size to struct alignment
    pub fn calculate_struct_layout(
        &mut self,
        struct_def: &LirStruct,
    ) -> Result<StructLayout, CompilerError> {
        let mut fields = Vec::new();
        let mut current_offset = 0u32;
        let mut max_alignment = 1u32;

        for field in &struct_def.fields {
            let wasm_type = WasmType::from_lir_type(field.ty);
            let field_alignment = self.calculate_alignment(wasm_type);
            let field_size = wasm_type.size_bytes();

            // Update maximum alignment for the struct
            max_alignment = max_alignment.max(field_alignment);

            // Align current offset to field's alignment requirement
            current_offset = self.align_to(current_offset, field_alignment);

            // Get field name as string
            let field_name = format!("{:?}", field.name);

            fields.push(FieldLayout {
                offset: current_offset,
                size: field_size,
                alignment: field_alignment,
                wasm_type,
                name: field_name,
            });

            current_offset += field_size;
        }

        // Ensure minimum 2-byte alignment for tagged pointer support
        // This allows the lowest bit to be used for ownership flags
        let final_alignment = max_alignment.max(2);

        // Align total size to struct alignment (ensures arrays of structs are aligned)
        let total_size = self.align_to(current_offset, final_alignment);

        // Get struct name as string
        let struct_name = format!("{:?}", struct_def.name);

        let layout = StructLayout {
            total_size,
            alignment: max_alignment,
            fields,
            name: struct_name.clone(),
        };

        // Cache the layout
        self.struct_layouts.insert(struct_name, layout.clone());

        Ok(layout)
    }

    /// Get field offset for a struct field by index
    pub fn get_field_offset(&self, struct_name: &str, field_index: usize) -> Option<u32> {
        self.struct_layouts
            .get(struct_name)?
            .fields
            .get(field_index)
            .map(|field| field.offset)
    }

    /// Get field offset by field name
    pub fn get_field_offset_by_name(&self, struct_name: &str, field_name: &str) -> Option<u32> {
        let layout = self.struct_layouts.get(struct_name)?;
        layout
            .fields
            .iter()
            .find(|f| f.name == field_name)
            .map(|f| f.offset)
    }

    /// Calculate alignment requirement for a WASM type
    pub fn calculate_alignment(&self, wasm_type: WasmType) -> u32 {
        *self.alignment_requirements.get(&wasm_type).unwrap_or(&1)
    }

    /// Get cached struct layout by name
    pub fn get_struct_layout(&self, struct_name: &str) -> Option<&StructLayout> {
        self.struct_layouts.get(struct_name)
    }

    /// Check if a struct layout is cached
    pub fn has_struct_layout(&self, struct_name: &str) -> bool {
        self.struct_layouts.contains_key(struct_name)
    }

    /// Align a value to the specified alignment.
    ///
    /// Uses the formula: (value + alignment - 1) & !(alignment - 1)
    /// This rounds up to the next multiple of alignment.
    fn align_to(&self, value: u32, alignment: u32) -> u32 {
        if alignment == 0 {
            return value;
        }
        (value + alignment - 1) & !(alignment - 1)
    }

    /// Calculate aligned size ensuring minimum 2-byte alignment for tagged pointers.
    ///
    /// All heap allocations must be at least 2-byte aligned to allow
    /// the lowest bit to be used for ownership flags.
    pub fn calculate_aligned_size(&self, base_size: u32) -> u32 {
        self.align_to(base_size, 2)
    }

    /// Calculate the size needed for an array of structs
    pub fn calculate_array_size(&self, struct_name: &str, count: u32) -> Option<u32> {
        let layout = self.struct_layouts.get(struct_name)?;
        Some(layout.total_size * count)
    }

    /// Implement tagged pointer operations for Beanstalk's ownership system.
    ///
    /// Tagged pointers use the lowest alignment-safe bit for ownership:
    /// - 0 = borrowed (callee must not drop)
    /// - 1 = owned (callee must drop before returning)
    ///
    /// This is a placeholder that will be fully implemented in the ownership_manager module.
    pub fn implement_tagged_pointer_ops(
        &self,
        _function: &mut Function,
    ) -> Result<(), CompilerError> {
        // Tagged pointer operations are implemented in ownership_manager.rs
        // This method is kept for API compatibility
        Ok(())
    }
}

impl Default for MemoryLayoutCalculator {
    fn default() -> Self {
        Self::new()
    }
}

/// Standalone function to align a value to the specified alignment.
/// Useful for one-off calculations without creating a MemoryLayoutCalculator.
pub fn align_to(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

/// Calculate the natural alignment for a LIR type
pub fn alignment_for_lir_type(lir_type: LirType) -> u32 {
    match lir_type {
        LirType::I32 | LirType::F32 => 4,
        LirType::I64 | LirType::F64 => 8,
    }
}

/// Calculate the size in bytes for a LIR type
pub fn size_for_lir_type(lir_type: LirType) -> u32 {
    match lir_type {
        LirType::I32 | LirType::F32 => 4,
        LirType::I64 | LirType::F64 => 8,
    }
}
