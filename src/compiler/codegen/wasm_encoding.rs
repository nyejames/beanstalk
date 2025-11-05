//! # WASM Encoding Module
//!
//! This module provides direct WIR-to-WASM lowering for the Beanstalk compiler.
//! It implements a simplified, efficient approach to WASM generation that maps
//! WIR constructs directly to WASM instructions with minimal overhead.
//!
//! ## Architecture
//!
//! The WASM encoding follows these key principles:
//! - **Direct Lowering**: Each WIR statement maps to ≤3 WASM instructions
//! - **Memory Safety**: Leverages WIR's borrow checking for safe memory access
//! - **String Management**: Efficient string constant deduplication and storage
//! - **Local Analysis**: Automatic WASM local variable allocation from WIR places
//!
//! ## Key Components
//!
//! - [`WasmModule`]: Main module builder that orchestrates WASM generation
//! - [`StringManager`]: Handles string constant deduplication and memory layout
//! - [`LocalAnalyzer`]: Analyzes WIR functions to determine WASM local requirements
//! - [`LocalMap`]: Maps WIR places to WASM local/global indices
//!
//! ## Usage
//!
//! ```rust
//! let wasm_module = WasmModule::from_wir(&wir)?;
//! let wasm_bytes = wasm_module.finish();
//! ```

use crate::compiler::compiler_errors::CompileError;
use crate::compiler::datatypes::DataType;
use crate::compiler::host_functions::registry::HostFunctionDef;
use crate::compiler::host_functions::wasix_registry::{
    WasixFunctionDef, WasixFunctionRegistry, create_wasix_registry,
};
use crate::compiler::wir::place::{
    FieldSize, MemoryBase, Place, ProjectionElem, TypeSize, WasmType,
};
use crate::compiler::wir::wir_nodes::{
    BinOp, BorrowKind, Constant, MemoryOpKind, Operand, Rvalue, Statement, Terminator, UnOp, WIR,
    WirFunction,
};
use crate::{
    return_compiler_error, return_unimplemented_feature_error, return_wasm_validation_error,
    return_wasm_generation_error,
};
use std::collections::{HashMap, HashSet};
use wasm_encoder::*;

/// Enhanced function builder that leverages wasm_encoder's built-in validation and control flow management
///
/// This builder provides type-safe WASM generation with automatic validation and proper control frame management.
/// It addresses the "control frames remain" error by ensuring all control structures are properly opened and closed.
#[derive(Debug)]
pub struct EnhancedFunctionBuilder {
    /// The underlying wasm_encoder Function
    function: Function,
    /// Function signature information for validation
    param_types: Vec<ValType>,
    result_types: Vec<ValType>,
    /// Function name for error reporting
    function_name: String,
    /// Control frame stack for validation
    control_stack: Vec<ControlFrame>,
    /// Whether the function has been properly terminated
    is_terminated: bool,
    /// Source location information for error reporting
    source_locations: Vec<SourceLocationInfo>,
    /// Current instruction count for error context
    instruction_count: u32,
}

/// Source location information for mapping WASM errors back to Beanstalk source
#[derive(Debug, Clone)]
pub struct SourceLocationInfo {
    /// Instruction index in the WASM function
    #[allow(dead_code)]
    pub instruction_index: u32,
    /// Beanstalk source file (if available)
    pub source_file: Option<String>,
    /// Line number in Beanstalk source
    pub line: u32,
    /// Column number in Beanstalk source
    #[allow(dead_code)]
    pub column: u32,
    /// Context description (e.g., "variable assignment", "function call")
    pub context: String,
}

/// Control frame tracking for proper WASM validation
#[derive(Debug, Clone)]
pub struct ControlFrame {
    /// Type of control frame
    pub frame_type: ControlFrameType,
    /// Block type for this frame
    #[allow(dead_code)]
    pub block_type: BlockType,
    /// Whether this frame expects a result
    #[allow(dead_code)]
    pub expects_result: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum ControlFrameType {
    Function,
    #[allow(dead_code)]
    Block,
    If,
    Else,
    #[allow(dead_code)]
    Loop,
}

impl EnhancedFunctionBuilder {
    /// Create a new enhanced function builder
    pub fn new(
        locals: Vec<(u32, ValType)>,
        param_types: Vec<ValType>,
        result_types: Vec<ValType>,
        function_name: String,
    ) -> Self {
        let function = Function::new(locals);

        // Initialize with function control frame
        let mut control_stack = Vec::new();
        let function_block_type = if result_types.is_empty() {
            BlockType::Empty
        } else if result_types.len() == 1 {
            BlockType::Result(result_types[0])
        } else {
            // For multiple results, we'll need to handle this differently
            BlockType::Empty // Simplified for now
        };

        control_stack.push(ControlFrame {
            frame_type: ControlFrameType::Function,
            block_type: function_block_type,
            expects_result: !result_types.is_empty(),
        });

        Self {
            function,
            param_types,
            result_types,
            function_name,
            control_stack,
            is_terminated: false,
            source_locations: Vec::new(),
            instruction_count: 0,
        }
    }

    /// Begin a new block with proper control frame tracking
    pub fn begin_block(&mut self, _block_id: u32) -> Result<(), CompileError> {
        // For now, we don't generate explicit WASM blocks for WIR blocks
        // WIR blocks are logical constructs that map to sequential instructions
        // Control flow is handled by terminators (if, goto, return)
        Ok(())
    }

    /// End a block with control frame validation
    pub fn end_block(&mut self) -> Result<(), CompileError> {
        // Corresponding to begin_block, no explicit action needed
        Ok(())
    }

    /// Add an instruction with type safety validation and source location tracking
    pub fn instruction(&mut self, instr: &Instruction) -> Result<(), CompileError> {
        self.instruction_with_context(instr, None)
    }

    /// Add an instruction with source location context for error reporting
    pub fn instruction_with_context(
        &mut self,
        instr: &Instruction,
        source_location: Option<SourceLocationInfo>,
    ) -> Result<(), CompileError> {
        // Check for terminating instructions
        match instr {
            Instruction::Return | Instruction::Unreachable => {
                self.is_terminated = true;
            }
            Instruction::If(_) => {
                // Push if control frame
                self.control_stack.push(ControlFrame {
                    frame_type: ControlFrameType::If,
                    block_type: BlockType::Empty,
                    expects_result: false,
                });
            }
            Instruction::Else => {
                // Validate that we're in an if frame
                if let Some(frame) = self.control_stack.last_mut() {
                    if frame.frame_type != ControlFrameType::If {
                        return_compiler_error!(
                            "WASM validation error in function '{}': else instruction without matching if",
                            self.function_name
                        );
                    }
                    frame.frame_type = ControlFrameType::Else;
                } else {
                    return_compiler_error!(
                        "WASM validation error in function '{}': else instruction with empty control stack",
                        self.function_name
                    );
                }
            }
            Instruction::End => {
                // Pop control frame
                if let Some(frame) = self.control_stack.pop() {
                    // Validate that we're not popping the function frame prematurely
                    if frame.frame_type == ControlFrameType::Function
                        && self.control_stack.is_empty()
                    {
                        return_compiler_error!(
                            "WASM validation error in function '{}': attempting to end function frame",
                            self.function_name
                        );
                    }
                } else {
                    return_compiler_error!(
                        "WASM validation error in function '{}': end instruction with empty control stack",
                        self.function_name
                    );
                }
            }
            _ => {
                // Other instructions don't affect control flow
            }
        }

        // Track source location if provided
        if let Some(location) = source_location {
            self.source_locations.push(SourceLocationInfo {
                instruction_index: self.instruction_count,
                ..location
            });
        }

        // Add instruction to function
        self.function.instruction(instr);
        self.instruction_count += 1;
        Ok(())
    }

    /// Finalize the function with proper validation and termination
    pub fn finalize(mut self) -> Result<Function, CompileError> {
        // Ensure proper function termination
        if !self.is_terminated {
            self.ensure_proper_termination()?;
        }

        // Validate control frame stack
        self.validate_control_frames()?;

        // Add final End instruction for function body
        self.function.instruction(&Instruction::End);

        // Perform additional wasm_encoder validation on the function
        self.validate_function_structure()?;

        Ok(self.function)
    }

    /// Validate the function structure using wasm_encoder principles
    fn validate_function_structure(&self) -> Result<(), CompileError> {
        // Check that we have proper function signature alignment
        if self.result_types.len() > 1 {
            // Multi-value returns require special handling in WASM
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WASM: Function '{}' has {} return values - ensuring multi-value support",
                self.function_name,
                self.result_types.len()
            );
        }

        // Validate parameter count doesn't exceed WASM limits
        if self.param_types.len() > 1000 {
            return_compiler_error!(
                "Function '{}' has too many parameters ({}). WASM functions are limited to 1000 parameters.",
                self.function_name,
                self.param_types.len()
            );
        }

        // Validate return type count doesn't exceed WASM limits
        if self.result_types.len() > 1000 {
            return_compiler_error!(
                "Function '{}' has too many return values ({}). WASM functions are limited to 1000 return values.",
                self.function_name,
                self.result_types.len()
            );
        }

        Ok(())
    }

    /// Ensure the function has proper termination
    fn ensure_proper_termination(&mut self) -> Result<(), CompileError> {
        if self.result_types.is_empty() {
            // Void function - add return
            self.function.instruction(&Instruction::Return);
        } else {
            // Function with return types - add default values and return
            for result_type in &self.result_types {
                WasmModule::emit_default_value_for_type(&mut self.function, *result_type);
            }
            self.function.instruction(&Instruction::Return);
        }

        self.is_terminated = true;
        Ok(())
    }

    /// Validate that all control frames are properly closed
    fn validate_control_frames(&self) -> Result<(), CompileError> {
        // Should only have the function frame remaining
        if self.control_stack.len() != 1 {
            return_compiler_error!(
                "WASM validation error in function '{}': {} control frames remain unclosed",
                self.function_name,
                self.control_stack.len() - 1
            );
        }

        if let Some(frame) = self.control_stack.first() {
            if frame.frame_type != ControlFrameType::Function {
                return_compiler_error!(
                    "WASM validation error in function '{}': expected function frame, found {:?}",
                    self.function_name,
                    frame.frame_type
                );
            }
        }

        Ok(())
    }

    /// Get access to the underlying function for direct instruction addition (when needed)
    pub fn get_function_mut(&mut self) -> &mut Function {
        &mut self.function
    }
}

/// String constant manager for deduplication and memory management
///
/// Handles string constants by storing them in WASM linear memory with length prefixes.
/// Provides deduplication to avoid storing identical strings multiple times.
#[derive(Debug, Clone)]
pub struct StringManager {
    /// Map from string content to offset in data section
    string_constants: HashMap<String, u32>,
    /// Raw data section bytes (length prefix + string data)
    data_section: Vec<u8>,
    /// Next available offset in data section
    next_offset: u32,
}

impl StringManager {
    /// Create a new StringManager
    pub fn new() -> Self {
        Self {
            string_constants: HashMap::new(),
            data_section: Vec::new(),
            next_offset: 0,
        }
    }

    /// Add a string slice constant and return its offset in linear memory
    ///
    /// String slices are immutable references stored with a 4-byte length prefix
    /// followed by UTF-8 data. Identical strings are deduplicated and return the same offset.
    ///
    /// ## Memory Management
    /// String slice constants have static lifetime and are stored in the WASM data section.
    /// No drop semantics are needed since they persist for the entire program execution.
    /// This is appropriate for immutable string literals created with "" syntax.
    pub fn add_string_slice_constant(&mut self, value: &str) -> u32 {
        // Check if we already have this string
        if let Some(&offset) = self.string_constants.get(value) {
            return offset;
        }

        let offset = self.next_offset;
        let bytes = value.as_bytes();

        // Store length prefix (4 bytes, little-endian) + string data
        let length = bytes.len() as u32;
        self.data_section.extend_from_slice(&length.to_le_bytes());
        self.data_section.extend_from_slice(bytes);

        // Update next offset (4 bytes for length + string length)
        self.next_offset += 4 + bytes.len() as u32;

        // Store mapping for deduplication
        self.string_constants.insert(value.to_string(), offset);

        offset
    }

    /// Add a string constant and return its offset in linear memory (legacy method)
    ///
    /// This method is kept for backward compatibility and delegates to add_string_slice_constant
    pub fn add_string_constant(&mut self, value: &str) -> u32 {
        self.add_string_slice_constant(value)
    }

    /// Allocate space for a mutable string in linear memory
    ///
    /// Mutable strings (templates) are heap-allocated with a header containing:
    /// - 4 bytes: current length
    /// - 4 bytes: capacity
    /// - N bytes: UTF-8 string data
    ///
    /// ## Memory Management
    /// Mutable strings require explicit memory management and should be freed
    /// when no longer needed. The borrow checker ensures safe access patterns.
    pub fn allocate_mutable_string(&mut self, initial_value: &str, capacity: u32) -> u32 {
        let offset = self.next_offset;
        let bytes = initial_value.as_bytes();
        let current_length = bytes.len() as u32;

        // Ensure capacity is at least as large as initial content
        let actual_capacity = capacity.max(current_length);

        // Store mutable string header: [length][capacity][data...]
        self.data_section
            .extend_from_slice(&current_length.to_le_bytes());
        self.data_section
            .extend_from_slice(&actual_capacity.to_le_bytes());
        self.data_section.extend_from_slice(bytes);

        // Pad to capacity if needed
        let padding_needed = actual_capacity - current_length;
        self.data_section.extend(vec![0; padding_needed as usize]);

        // Update next offset (8 bytes for header + capacity for data)
        self.next_offset += 8 + actual_capacity;

        offset
    }

    /// Get the raw data section bytes
    pub fn get_data_section(&self) -> &[u8] {
        &self.data_section
    }

    /// Get the total size of the data section
    pub fn get_data_size(&self) -> u32 {
        self.next_offset
    }

    /// Get the number of unique strings stored
    pub fn get_string_count(&self) -> usize {
        self.string_constants.len()
    }

    /// Get string allocation statistics for memory management analysis
    pub fn get_allocation_stats(&self) -> StringAllocationStats {
        StringAllocationStats {
            unique_strings: self.string_constants.len(),
            total_data_size: self.next_offset,
            deduplication_savings: self.calculate_deduplication_savings(),
        }
    }

    /// Calculate how much memory was saved through string deduplication
    fn calculate_deduplication_savings(&self) -> u32 {
        // This is a simplified calculation - in a real implementation,
        // we would track the number of times each string was referenced
        0 // Placeholder for now
    }

    /// Add raw data to the data section and return its offset
    /// This is used for non-string data like IOVec structures
    pub fn add_raw_data(&mut self, data: &[u8]) -> u32 {
        let offset = self.next_offset;
        self.data_section.extend_from_slice(data);
        self.next_offset += data.len() as u32;
        offset
    }
}

impl Default for StringManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Statistics for string allocation and memory management
#[derive(Debug, Clone)]
pub struct StringAllocationStats {
    /// Number of unique strings stored
    pub unique_strings: usize,
    /// Total size of string data in bytes
    pub total_data_size: u32,
    /// Estimated memory saved through deduplication
    pub deduplication_savings: u32,
}

/// Local variable mapping from WIR places to WASM local indices
///
/// This structure manages the mapping between WIR Place::Local indices and
/// WASM local variable indices, enabling proper place resolution.
#[derive(Debug, Clone)]
pub struct LocalMap {
    /// Map from WIR local index to WASM local index
    local_mapping: HashMap<u32, u32>,
    /// Map from WIR global index to WASM global index
    global_mapping: HashMap<u32, u32>,
    /// Next available WASM local index
    next_local_index: u32,
    /// Next available WASM global index
    next_global_index: u32,
}

impl LocalMap {
    /// Create a new empty local map
    pub fn new() -> Self {
        Self {
            local_mapping: HashMap::new(),
            global_mapping: HashMap::new(),
            next_local_index: 0,
            next_global_index: 0,
        }
    }

    /// Create a local map with parameter count (parameters occupy first local indices)
    pub fn with_parameters(param_count: u32) -> Self {
        Self {
            local_mapping: HashMap::new(),
            global_mapping: HashMap::new(),
            next_local_index: param_count, // Parameters occupy indices 0..param_count
            next_global_index: 0,
        }
    }

    /// Map a WIR local index to a WASM local index
    pub fn map_local(&mut self, wir_local: u32, wasm_local: u32) {
        self.local_mapping.insert(wir_local, wasm_local);
    }

    /// Map a WIR global index to a WASM global index
    pub fn map_global(&mut self, wir_global: u32, wasm_global: u32) {
        self.global_mapping.insert(wir_global, wasm_global);
    }

    /// Get WASM local index for WIR local
    pub fn get_local(&self, wir_local: u32) -> Option<u32> {
        self.local_mapping.get(&wir_local).copied()
    }

    /// Get WASM global index for WIR global
    pub fn get_global(&self, wir_global: u32) -> Option<u32> {
        self.global_mapping.get(&wir_global).copied()
    }

    /// Allocate next WASM local index for a WIR local
    pub fn allocate_local(&mut self, wir_local: u32) -> u32 {
        let wasm_local = self.next_local_index;
        self.next_local_index += 1;
        self.local_mapping.insert(wir_local, wasm_local);
        wasm_local
    }

    /// Allocate next WASM global index for a WIR global
    pub fn allocate_global(&mut self, wir_global: u32) -> u32 {
        let wasm_global = self.next_global_index;
        self.next_global_index += 1;
        self.global_mapping.insert(wir_global, wasm_global);
        wasm_global
    }

    /// Get all local mappings for debugging
    pub fn get_all_locals(&self) -> &HashMap<u32, u32> {
        &self.local_mapping
    }

    /// Get all global mappings for debugging
    pub fn get_all_globals(&self) -> &HashMap<u32, u32> {
        &self.global_mapping
    }
}

impl Default for LocalMap {
    fn default() -> Self {
        Self::new()
    }
}

/// Local variable analyzer for determining WASM local requirements from WIR
///
/// This analyzer examines a WIR function to determine what local variables are needed
/// and builds the appropriate mapping for WASM code generation.
#[derive(Debug)]
pub struct LocalAnalyzer {
    /// Map from WIR local index to WASM type
    local_types: HashMap<u32, WasmType>,
    /// Count of each WASM type needed for locals
    type_counts: HashMap<WasmType, u32>,
    /// Parameter count (these occupy first local indices)
    parameter_count: u32,
}

impl LocalAnalyzer {
    /// Create a new local analyzer
    pub fn new() -> Self {
        Self {
            local_types: HashMap::new(),
            type_counts: HashMap::new(),
            parameter_count: 0,
        }
    }

    /// Analyze a WIR function to determine local variable requirements
    pub fn analyze_function(wir_function: &WirFunction) -> Self {
        let mut analyzer = Self::new();
        analyzer.parameter_count = wir_function.parameters.len() as u32;

        // Analyze all places used in the function
        for block in &wir_function.blocks {
            for statement in &block.statements {
                analyzer.collect_from_statement(statement);
            }
            analyzer.collect_from_terminator(&block.terminator);
        }

        // Also analyze local variables declared in the function
        for (_, place) in &wir_function.locals {
            analyzer.collect_from_place(place);
        }

        analyzer
    }

    /// Collect local variable information from a statement
    fn collect_from_statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Assign { place, rvalue } => {
                self.collect_from_place(place);
                self.collect_from_rvalue(rvalue);
            }
            Statement::Call {
                func,
                args,
                destination,
            } => {
                self.collect_from_operand(func);
                for arg in args {
                    self.collect_from_operand(arg);
                }
                if let Some(dest) = destination {
                    self.collect_from_place(dest);
                }
            }
            Statement::InterfaceCall {
                receiver,
                args,
                destination,
                ..
            } => {
                self.collect_from_operand(receiver);
                for arg in args {
                    self.collect_from_operand(arg);
                }
                if let Some(dest) = destination {
                    self.collect_from_place(dest);
                }
            }
            Statement::Alloc { place, size, .. } => {
                self.collect_from_place(place);
                self.collect_from_operand(size);
            }
            Statement::Dealloc { place } => {
                self.collect_from_place(place);
            }
            Statement::Store { place, value, .. } => {
                self.collect_from_place(place);
                self.collect_from_operand(value);
            }
            Statement::Drop { place } => {
                self.collect_from_place(place);
            }
            Statement::MemoryOp {
                operand, result, ..
            } => {
                if let Some(op) = operand {
                    self.collect_from_operand(op);
                }
                if let Some(res) = result {
                    self.collect_from_place(res);
                }
            }
            Statement::HostCall {
                args, destination, ..
            } => {
                for arg in args {
                    self.collect_from_operand(arg);
                }
                if let Some(dest) = destination {
                    self.collect_from_place(dest);
                }
            }
            Statement::WasixCall {
                args, destination, ..
            } => {
                for arg in args {
                    self.collect_from_operand(arg);
                }
                if let Some(dest) = destination {
                    self.collect_from_place(dest);
                }
            }
            Statement::MarkFieldInitialized { struct_place, .. } => {
                // Field initialization tracking - collect struct place
                self.collect_from_place(struct_place);
            }
            Statement::ValidateStructInitialization { struct_place, .. } => {
                // Struct validation - collect struct place
                self.collect_from_place(struct_place);
            }
            Statement::Conditional {
                condition,
                then_statements,
                else_statements,
            } => {
                // Collect from condition operand
                self.collect_from_operand(condition);
                // Collect from all statements in both branches
                for stmt in then_statements {
                    self.collect_from_statement(stmt);
                }
                for stmt in else_statements {
                    self.collect_from_statement(stmt);
                }
            }
            Statement::Nop => {
                // No places to collect
            }
        }
    }

    /// Collect local variable information from a terminator
    fn collect_from_terminator(&mut self, terminator: &Terminator) {
        match terminator {
            Terminator::Return { values } => {
                for value in values {
                    self.collect_from_operand(value);
                }
            }
            Terminator::If { condition, .. } => {
                self.collect_from_operand(condition);
            }
            Terminator::Goto { .. } | Terminator::Unreachable => {
                // No operands to collect
            }
        }
    }

    /// Collect local variable information from an rvalue
    fn collect_from_rvalue(&mut self, rvalue: &Rvalue) {
        match rvalue {
            Rvalue::Use(operand) => {
                self.collect_from_operand(operand);
            }
            Rvalue::BinaryOp(_, left, right) => {
                self.collect_from_operand(left);
                self.collect_from_operand(right);
            }
            Rvalue::UnaryOp(_, operand) => {
                self.collect_from_operand(operand);
            }
            Rvalue::Ref { place, .. } => {
                self.collect_from_place(place);
            }
            Rvalue::StringConcat(left, right) => {
                self.collect_from_operand(left);
                self.collect_from_operand(right);
            }
        }
    }

    /// Collect local variable information from an operand
    fn collect_from_operand(&mut self, operand: &Operand) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.collect_from_place(place);
            }
            Operand::Constant(_) | Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
                // These don't require local variables
            }
        }
    }

    /// Collect local variable information from a place
    fn collect_from_place(&mut self, place: &Place) {
        match place {
            Place::Local { index, wasm_type } => {
                // Only collect locals that aren't parameters
                if *index >= self.parameter_count {
                    self.local_types.insert(*index, wasm_type.clone());
                    *self.type_counts.entry(wasm_type.clone()).or_insert(0) += 1;
                }
            }
            Place::Projection { base, elem } => {
                self.collect_from_place(base);
                if let ProjectionElem::Index { index, .. } = elem {
                    self.collect_from_place(index);
                }
            }
            Place::Global { .. } | Place::Memory { .. } => {
                // These don't require local variables
            }
        }
    }

    /// Generate WASM local variable declarations as (count, ValType) pairs
    pub fn generate_wasm_locals(&self) -> Vec<(u32, ValType)> {
        // Use BTreeMap to ensure consistent ordering with build_local_mapping()
        let mut ordered_counts: std::collections::BTreeMap<WasmType, u32> =
            std::collections::BTreeMap::new();
        for (wasm_type, count) in &self.type_counts {
            ordered_counts.insert(wasm_type.clone(), *count);
        }

        ordered_counts
            .iter()
            .map(|(wasm_type, count)| (*count, self.wasm_type_to_val_type(wasm_type)))
            .collect()
    }

    /// Build local mapping from WIR analysis
    pub fn build_local_mapping(&self, wir_function: &WirFunction) -> LocalMap {
        let mut local_map = LocalMap::with_parameters(wir_function.parameters.len() as u32);
        let mut wasm_local_index = wir_function.parameters.len() as u32;

        // Group WIR locals by type to match WASM local declaration order
        let mut locals_by_type: std::collections::BTreeMap<WasmType, Vec<u32>> =
            std::collections::BTreeMap::new();

        for (wir_local_index, wasm_type) in &self.local_types {
            locals_by_type
                .entry(wasm_type.clone())
                .or_insert_with(Vec::new)
                .push(*wir_local_index);
        }

        // Map WIR locals to WASM locals in the same order as generate_wasm_locals()
        // This ensures type consistency between declaration and usage
        for (wasm_type, wir_locals) in locals_by_type {
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WASM: Mapping {} WIR locals of type {:?} starting at WASM local {}",
                wir_locals.len(),
                wasm_type,
                wasm_local_index
            );

            for wir_local_index in wir_locals {
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: WIR local {} → WASM local {} (type {:?})",
                    wir_local_index, wasm_local_index, wasm_type
                );

                local_map.map_local(wir_local_index, wasm_local_index);
                wasm_local_index += 1;
            }
        }

        // Also map any globals that might be referenced
        // (This will be expanded when we implement global variable support)

        local_map
    }

    /// Convert WasmType to ValType for wasm_encoder - uses unified conversion
    fn wasm_type_to_val_type(&self, wasm_type: &WasmType) -> ValType {
        WasmModule::unified_wasm_type_to_val_type(wasm_type)
    }

    /// Get statistics about local variable usage
    pub fn get_local_stats(&self) -> LocalAnalysisStats {
        LocalAnalysisStats {
            total_locals: self.local_types.len(),
            i32_locals: *self.type_counts.get(&WasmType::I32).unwrap_or(&0),
            i64_locals: *self.type_counts.get(&WasmType::I64).unwrap_or(&0),
            f32_locals: *self.type_counts.get(&WasmType::F32).unwrap_or(&0),
            f64_locals: *self.type_counts.get(&WasmType::F64).unwrap_or(&0),
            ref_locals: *self.type_counts.get(&WasmType::ExternRef).unwrap_or(&0)
                + *self.type_counts.get(&WasmType::FuncRef).unwrap_or(&0),
        }
    }
}

/// Statistics for local variable analysis
#[derive(Debug, Clone)]
pub struct LocalAnalysisStats {
    /// Total number of local variables
    pub total_locals: usize,
    /// Number of i32 locals
    pub i32_locals: u32,
    /// Number of i64 locals
    pub i64_locals: u32,
    /// Number of f32 locals
    pub f32_locals: u32,
    /// Number of f64 locals
    pub f64_locals: u32,
    /// Number of reference locals
    pub ref_locals: u32,
}

/// Simplified WASM module for basic WIR-to-WASM compilation
#[derive(Clone)]
pub struct WasmModule {
    type_section: TypeSection,
    import_section: ImportSection,
    function_section: FunctionSection,
    memory_section: MemorySection,
    global_section: GlobalSection,
    export_section: ExportSection,
    code_section: CodeSection,
    data_section: DataSection,

    // String constant management
    string_manager: StringManager,

    // Function registry for name resolution
    function_registry: HashMap<String, u32>,

    // Host function index mapping
    host_function_indices: HashMap<String, u32>,

    // WASIX function registry for WASIX imports
    wasix_registry: WasixFunctionRegistry,

    // WASIX memory manager for enhanced allocation strategies
    wasix_memory_manager: crate::compiler::host_functions::wasix_registry::WasixMemoryManager,

    // Source location tracking for error reporting
    function_source_map: HashMap<u32, FunctionSourceInfo>,

    // Enhanced function metadata for named returns and references
    function_metadata: HashMap<String, FunctionMetadata>,

    // Host function registry for runtime-specific mappings
    host_registry: Option<crate::compiler::host_functions::registry::HostFunctionRegistry>,

    // Internal state
    pub function_count: u32,
    pub type_count: u32,
    global_count: u32,
    
    // Track exported names to prevent duplicates
    exported_names: std::collections::HashSet<String>,
}

/// Source information for a compiled function
#[derive(Debug, Clone)]
pub struct FunctionSourceInfo {
    /// Original function name in Beanstalk
    pub function_name: String,
    /// Source file path (if available)
    pub source_file: Option<String>,
    /// Starting line in source
    pub start_line: u32,
    /// Ending line in source
    pub end_line: u32,
    /// Source location information for instructions
    pub instruction_locations: Vec<SourceLocationInfo>,
}

/// Enhanced function metadata for named returns and references
#[derive(Debug, Clone)]
pub struct FunctionMetadata {
    /// Function name
    pub name: String,
    /// Information about return parameters
    pub return_parameters: Vec<ReturnParameterInfo>,
}

/// Information about a return parameter
#[derive(Debug, Clone)]
pub struct ReturnParameterInfo {
    /// Index in the return tuple
    pub index: usize,
    /// Parameter name (if named)
    pub name: String,
    /// Original data type
    pub data_type: DataType,
    /// Whether this is a reference type
    pub is_reference: bool,
}

impl Default for WasmModule {
    fn default() -> Self {
        Self::new()
    }
}

// Unified type conversion utilities
impl WasmModule {
    /// Unified DataType to WasmType conversion - consolidates duplicate conversion logic
    pub fn unified_datatype_to_wasm_type(data_type: &DataType) -> Result<WasmType, CompileError> {
        match data_type {
            DataType::Int => Ok(WasmType::I32),
            DataType::Float => Ok(WasmType::F64), // Use f64 for Beanstalk floats
            DataType::Bool => Ok(WasmType::I32),
            DataType::String => Ok(WasmType::I32), // String slice pointer (immutable reference to data section)
            DataType::Collection(..) => Ok(WasmType::I32), // Collection pointer
            DataType::Function(..) => Ok(WasmType::FuncRef),
            DataType::Inferred => Ok(WasmType::I32), // Default to i32 for unresolved types
            DataType::None => Ok(WasmType::I32),
            DataType::True | DataType::False => Ok(WasmType::I32), // Booleans as i32
            DataType::Decimal => Ok(WasmType::F64),                // Decimals as f64
            DataType::Template => Ok(WasmType::I32), // Mutable string pointer (heap-allocated)
            DataType::Range => Ok(WasmType::I32),    // Range pointer
            DataType::CoerceToString => Ok(WasmType::I32), // String pointer
            DataType::Parameters(_) | DataType::Struct(..) => Ok(WasmType::I32), // Struct pointer
            DataType::Choices(_) => Ok(WasmType::I32), // Union pointer
            DataType::Option(_) => Ok(WasmType::I32), // Option pointer
            DataType::Reference(inner_type, _) => {
                // References have the same WASM type as their inner type
                Self::unified_datatype_to_wasm_type(inner_type)
            }
        }
    }

    /// Unified WasmType to ValType conversion - consolidates duplicate conversion logic
    pub fn unified_wasm_type_to_val_type(wasm_type: &WasmType) -> ValType {
        match wasm_type {
            WasmType::I32 => ValType::I32,
            WasmType::I64 => ValType::I64,
            WasmType::F32 => ValType::F32,
            WasmType::F64 => ValType::F64,
            WasmType::ExternRef => ValType::Ref(RefType::EXTERNREF),
            WasmType::FuncRef => ValType::Ref(RefType::FUNCREF),
        }
    }

    /// Get WASM type size in bytes - consolidates size calculation logic
    pub fn get_wasm_type_size(wasm_type: &WasmType) -> u32 {
        match wasm_type {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::ExternRef | WasmType::FuncRef => 4, // Pointer size
        }
    }

    /// Get WASM type alignment - consolidates alignment calculation logic
    pub fn get_wasm_type_alignment(wasm_type: &WasmType) -> u32 {
        match wasm_type {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::ExternRef | WasmType::FuncRef => 4, // Pointer alignment
        }
    }

    /// Check if WASM type is a numeric type - consolidates type checking logic
    pub fn is_numeric_type(wasm_type: &WasmType) -> bool {
        matches!(
            wasm_type,
            WasmType::I32 | WasmType::I64 | WasmType::F32 | WasmType::F64
        )
    }

    /// Check if WASM type is a reference type - consolidates type checking logic
    pub fn is_reference_type(wasm_type: &WasmType) -> bool {
        matches!(wasm_type, WasmType::ExternRef | WasmType::FuncRef)
    }
}

// Helper functions for common WASM instruction generation patterns
impl WasmModule {
    /// Generate I32 constant instruction - consolidates duplicate I32Const patterns
    fn emit_i32_const(function: &mut Function, value: i32) {
        function.instruction(&Instruction::I32Const(value));
    }

    /// Generate I64 constant instruction
    fn emit_i64_const(function: &mut Function, value: i64) {
        function.instruction(&Instruction::I64Const(value));
    }

    /// Generate F32 constant instruction
    fn emit_f32_const(function: &mut Function, value: f32) {
        function.instruction(&Instruction::F32Const(value.into()));
    }

    /// Generate F64 constant instruction
    fn emit_f64_const(function: &mut Function, value: f64) {
        function.instruction(&Instruction::F64Const(value.into()));
    }

    /// Generate memory offset calculation - consolidates duplicate offset + add patterns
    fn emit_memory_offset(function: &mut Function, offset: u32) {
        if offset > 0 {
            Self::emit_i32_const(function, offset as i32);
            function.instruction(&Instruction::I32Add);
        }
    }

    /// Generate array index calculation - consolidates duplicate index * element_size patterns
    fn emit_array_index_calculation(
        &mut self,
        function: &mut Function,
        index_place: &Place,
        element_size: u32,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load index value
        self.lower_place_access(index_place, function, local_map)?;
        // Multiply by element size
        Self::emit_i32_const(function, element_size as i32);
        function.instruction(&Instruction::I32Mul);
        // Add to base address (base should already be on stack)
        function.instruction(&Instruction::I32Add);
        Ok(())
    }

    /// Generate default value for WASM type - consolidates duplicate default value patterns
    fn emit_default_value_for_type(function: &mut Function, val_type: ValType) {
        match val_type {
            ValType::I32 => Self::emit_i32_const(function, 0),
            ValType::I64 => Self::emit_i64_const(function, 0),
            ValType::F32 => Self::emit_f32_const(function, 0.0),
            ValType::F64 => Self::emit_f64_const(function, 0.0),
            _ => Self::emit_i32_const(function, 0), // Default to i32 for other types
        }
    }

    /// Generate memory load instruction with proper alignment
    fn emit_memory_load(function: &mut Function, wasm_type: &WasmType, offset: u32) {
        let mem_arg = MemArg {
            offset: offset.into(),
            align: Self::get_alignment_for_type(wasm_type),
            memory_index: 0,
        };

        match wasm_type {
            WasmType::I32 => {
                function.instruction(&Instruction::I32Load(mem_arg));
            }
            WasmType::I64 => {
                function.instruction(&Instruction::I64Load(mem_arg));
            }
            WasmType::F32 => {
                function.instruction(&Instruction::F32Load(mem_arg));
            }
            WasmType::F64 => {
                function.instruction(&Instruction::F64Load(mem_arg));
            }
            WasmType::ExternRef | WasmType::FuncRef => {
                // References are stored as i32 pointers
                function.instruction(&Instruction::I32Load(mem_arg));
            }
        }
    }

    /// Generate memory store instruction with proper alignment
    fn emit_memory_store(function: &mut Function, wasm_type: &WasmType, offset: u32) {
        let mem_arg = MemArg {
            offset: offset.into(),
            align: Self::get_alignment_for_type(wasm_type),
            memory_index: 0,
        };

        match wasm_type {
            WasmType::I32 => {
                function.instruction(&Instruction::I32Store(mem_arg));
            }
            WasmType::I64 => {
                function.instruction(&Instruction::I64Store(mem_arg));
            }
            WasmType::F32 => {
                function.instruction(&Instruction::F32Store(mem_arg));
            }
            WasmType::F64 => {
                function.instruction(&Instruction::F64Store(mem_arg));
            }
            WasmType::ExternRef | WasmType::FuncRef => {
                // References are stored as i32 pointers
                function.instruction(&Instruction::I32Store(mem_arg));
            }
        }
    }

    /// Get proper alignment for WASM type - uses unified alignment calculation
    fn get_alignment_for_type(wasm_type: &WasmType) -> u32 {
        // Convert byte alignment to power-of-2 for WASM MemArg
        match Self::get_wasm_type_alignment(wasm_type) {
            4 => 2, // 4-byte alignment (2^2)
            8 => 3, // 8-byte alignment (2^3)
            _ => 0, // 1-byte alignment (2^0)
        }
    }
}

impl WasmModule {
    pub fn new() -> Self {
        Self {
            type_section: TypeSection::new(),
            import_section: ImportSection::new(),
            function_section: FunctionSection::new(),
            memory_section: MemorySection::new(),
            global_section: GlobalSection::new(),
            export_section: ExportSection::new(),
            code_section: CodeSection::new(),
            data_section: DataSection::new(),
            string_manager: StringManager::new(),
            function_registry: HashMap::new(),
            host_function_indices: HashMap::new(),
            wasix_registry: create_wasix_registry().unwrap_or_default(),
            wasix_memory_manager:
                crate::compiler::host_functions::wasix_registry::WasixMemoryManager::new(),
            function_source_map: HashMap::new(),
            function_metadata: HashMap::new(),
            host_registry: None,
            function_count: 0,
            type_count: 0,
            global_count: 0,
            exported_names: std::collections::HashSet::new(),
        }
    }

    /// Create a new WasmModule from WIR with comprehensive error handling and validation
    pub fn from_wir(wir: &WIR) -> Result<WasmModule, CompileError> {
        Self::from_wir_with_registry(wir, None)
    }

    /// Create a new WasmModule from WIR with host function registry access
    pub fn from_wir_with_registry(
        wir: &WIR, 
        registry: Option<&crate::compiler::host_functions::registry::HostFunctionRegistry>
    ) -> Result<WasmModule, CompileError> {
        let mut module = WasmModule::new();

        // Store registry reference for use during compilation
        if let Some(reg) = registry {
            module.set_host_function_registry(reg)?;
        }

        // Initialize memory section (1 page = 64KB)
        module.memory_section.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        // Generate WASM import section for host functions with registry-aware mapping
        module.encode_host_function_imports_with_registry(&wir.host_imports, registry)?;

        // Generate WASIX import section for WASIX functions
        module.add_wasix_imports()?;

        // Save the number of imports before compiling functions
        // This is needed for correct function index calculation in exports
        // Imports come first in the WASM function index space, then defined functions
        let import_count = module.function_count;

        // Process functions with enhanced error context and validation
        for (index, function) in wir.functions.iter().enumerate() {
            module.compile_function(function).map_err(|mut error| {
                // Add context about which function failed
                error.msg = format!(
                    "Failed to compile function '{}' (index {}): {}",
                    function.name, index, error.msg
                );
                error
            })?;
        }

        // Export entry point functions correctly, passing the import count
        module.export_entry_point_functions_with_import_count(&wir, import_count)?;

        // Export memory for WASIX access
        module.add_memory_export("memory")?;

        // Always validate the generated module using wasm_encoder's validation
        module.validate_with_wasm_encoder()?;

        Ok(module)
    }

    /// Set the host function registry for runtime-specific mappings
    pub fn set_host_function_registry(
        &mut self, 
        registry: &crate::compiler::host_functions::registry::HostFunctionRegistry
    ) -> Result<(), CompileError> {
        self.host_registry = Some(registry.clone());
        Ok(())
    }

    /// Get the host function registry if available
    pub fn get_host_function_registry(&self) -> Option<&crate::compiler::host_functions::registry::HostFunctionRegistry> {
        self.host_registry.as_ref()
    }

    /// Export entry point functions correctly in WASM modules with explicit import count
    ///
    /// This method implements subtask 3.3: Fix entry point export generation
    /// It ensures entry point functions are exported correctly and validates
    /// that only one start function is exported per module.
    pub fn export_entry_point_functions_with_import_count(&mut self, wir: &WIR, import_count: u32) -> Result<(), CompileError> {
        let mut entry_point_count = 0;
        let mut start_function_index: Option<u32> = None;

        // Look for entry point functions in WIR
        for (index, wir_function) in wir.functions.iter().enumerate() {
            let mut exported = false;
            
            // Check if this function is marked as an entry point in WIR exports
            if let Some(export) = wir.exports.get(&wir_function.name) {
                if export.kind == crate::compiler::wir::wir_nodes::ExportKind::Function {
                    // Check if this is the entry point by looking for specific naming patterns
                    // Entry points are typically named "main", "_start", or marked specially in the WIR
                    let is_entry_point = wir_function.name == "main" || 
                                        wir_function.name == "_start" ||
                                        wir_function.name.contains("entry") ||
                                        export.name == "_start"; // WASM start function convention

                    if is_entry_point {
                        entry_point_count += 1;
                        // FIXED: Add import_count to get the correct WASM function index
                        // Imports come first in the function index space, then defined functions
                        let function_index = import_count + (index as u32);
                        
                        // Export the entry point function
                        self.add_function_export(&export.name, function_index)?;
                        
                        // Mark as start function for WASM module
                        start_function_index = Some(function_index);
                        exported = true;

                        #[cfg(feature = "verbose_codegen_logging")]
                        println!(
                            "WASM: Exported entry point function '{}' at index {} as '{}'",
                            wir_function.name, function_index, export.name
                        );
                    }
                }
            }
            
            // Handle implicit main/_start function export only if not already exported
            if !exported && (wir_function.name == "main" || wir_function.name == "_start") {
                entry_point_count += 1;
                // FIXED: Add import_count to get the correct WASM function index
                let function_index = import_count + (index as u32);
                
                // Use the function's actual name for the export
                self.add_function_export(&wir_function.name, function_index)?;
                start_function_index = Some(function_index);

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Exported implicit entry point function '{}' at index {}",
                    wir_function.name, function_index
                );
            }
        }

        // Validate that only one start function is exported per module
        if entry_point_count > 1 {
            return_compiler_error!(
                "Multiple entry points found in module ({}). WASM modules can only have one start function.",
                entry_point_count
            );
        }

        // Add proper function indexing for exported entry points
        if let Some(start_index) = start_function_index {
            // In WASM, the start function is automatically called when the module is instantiated
            // We don't need to explicitly set a start section since the runtime will call the exported main
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WASM: Entry point function at index {} will serve as module start function",
                start_index
            );
        } else {
            // No entry point found - this is valid for library modules
            #[cfg(feature = "verbose_codegen_logging")]
            println!("WASM: No entry point function found - generating library module");
        }

        Ok(())
    }

    /// Validate the generated WASM module using wasm_encoder's built-in validation
    pub fn validate_with_wasm_encoder(&self) -> Result<(), CompileError> {
        // Generate the WASM bytes for validation
        let wasm_bytes = self.clone().finish();

        // Use wasmparser (which wasm_encoder uses internally) to validate the module
        match wasmparser::validate(&wasm_bytes) {
            Ok(_) => {
                #[cfg(feature = "verbose_codegen_logging")]
                println!("WASM: Module validation passed successfully!");
                Ok(())
            }
            Err(wasm_error) => {
                // Map wasm_encoder validation errors to helpful Beanstalk compiler error messages
                self.map_wasm_validation_error::<()>(&wasm_error)
            }
        }
    }

    /// Map WASM validation errors to helpful Beanstalk compiler error messages with source location context
    fn map_wasm_validation_error<T>(
        &self,
        wasm_error: &wasmparser::BinaryReaderError,
    ) -> Result<T, CompileError> {
        // Try to find source location context for the error
        let source_context = self.find_source_context_for_wasm_error(wasm_error);

        let error_message = match wasm_error.message() {
            msg if msg.contains("control frames remain") => {
                let context_hint = if let Some(ref ctx) = source_context {
                    format!(
                        " This error occurred around line {} in context: {}.",
                        ctx.line, ctx.context
                    )
                } else {
                    String::new()
                };
                format!(
                    "WASM control flow error: {}.{} This indicates that a function has unclosed control structures (if/else/end blocks). \
                    Check that all Beanstalk scope delimiters (':' and ';') are properly matched.",
                    msg, context_hint
                )
            }
            msg if msg.contains("type mismatch") => {
                let context_hint = if let Some(ref ctx) = source_context {
                    format!(
                        " This error occurred around line {} in context: {}.",
                        ctx.line, ctx.context
                    )
                } else {
                    String::new()
                };
                format!(
                    "WASM type error: {}.{} This indicates a type mismatch in the generated WASM code. \
                    Check that all Beanstalk variable types are consistent and properly declared.",
                    msg, context_hint
                )
            }
            msg if msg.contains("invalid function index") => {
                let context_hint = if let Some(ref ctx) = source_context {
                    format!(
                        " This error occurred around line {} in context: {}.",
                        ctx.line, ctx.context
                    )
                } else {
                    String::new()
                };
                format!(
                    "WASM function reference error: {}.{} This indicates an invalid function call. \
                    Check that all function names are correctly spelled and functions are defined before use.",
                    msg, context_hint
                )
            }
            msg if msg.contains("invalid local index") => {
                let context_hint = if let Some(ref ctx) = source_context {
                    format!(
                        " This error occurred around line {} in context: {}.",
                        ctx.line, ctx.context
                    )
                } else {
                    String::new()
                };
                format!(
                    "WASM local variable error: {}.{} This indicates an invalid variable reference. \
                    Check that all variables are properly declared before use.",
                    msg, context_hint
                )
            }
            msg if msg.contains("unexpected end") => {
                let context_hint = if let Some(ref ctx) = source_context {
                    format!(
                        " This error occurred around line {} in context: {}.",
                        ctx.line, ctx.context
                    )
                } else {
                    String::new()
                };
                format!(
                    "WASM structure error: {}.{} This indicates incomplete WASM function structure. \
                    Check that all Beanstalk functions have proper return statements and scope closing (';').",
                    msg, context_hint
                )
            }
            msg => {
                let context_hint = if let Some(ref ctx) = source_context {
                    format!(
                        " This error occurred around line {} in context: {}.",
                        ctx.line, ctx.context
                    )
                } else {
                    String::new()
                };
                format!(
                    "WASM validation error: {}.{} This indicates an issue with the generated WASM code. \
                    Please check your Beanstalk code for syntax errors or report this as a compiler bug.",
                    msg, context_hint
                )
            }
        };

        // Include the WASM offset and source location for debugging
        let debug_info = self.format_debug_info(wasm_error, &source_context);

        return_compiler_error!("{}{}", error_message, debug_info);
    }

    /// Find source context for a WASM validation error
    fn find_source_context_for_wasm_error(
        &self,
        wasm_error: &wasmparser::BinaryReaderError,
    ) -> Option<SourceLocationInfo> {
        let _offset = wasm_error.offset();

        // Try to map WASM offset to source location
        // This is a simplified implementation - a full implementation would need
        // more sophisticated offset-to-source mapping
        for function_info in self.function_source_map.values() {
            // Look for the closest instruction location
            if let Some(_location) = function_info.instruction_locations.first() {
                return Some(SourceLocationInfo {
                    instruction_index: 0,
                    source_file: function_info.source_file.clone(),
                    line: function_info.start_line,
                    column: 1,
                    context: format!("function '{}'", function_info.function_name),
                });
            }
        }

        None
    }

    /// Format debug information for error reporting
    fn format_debug_info(
        &self,
        wasm_error: &wasmparser::BinaryReaderError,
        source_context: &Option<SourceLocationInfo>,
    ) -> String {
        let mut debug_parts = Vec::new();

        // Add WASM offset
        debug_parts.push(format!("WASM offset: 0x{:x}", wasm_error.offset()));

        // Add source file information if available
        if let Some(ctx) = source_context
            && let Some(ref file) = ctx.source_file
        {
            debug_parts.push(format!("Source: {}:{}", file, ctx.line));
        }

        if debug_parts.is_empty() {
            String::new()
        } else {
            format!(" ({})", debug_parts.join(", "))
        }
    }

    /// Internal method to recreate module after validation (for debug builds)
    #[cfg(feature = "verbose_codegen_logging")]
    fn from_wir_internal(wir: &WIR) -> Result<WasmModule, CompileError> {
        let mut module = WasmModule::new();

        // Initialize memory section (1 page = 64KB)
        module.memory_section.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        // Generate WASM import section for host functions
        module.encode_host_function_imports(&wir.host_imports)?;

        // Process functions
        for function in &wir.functions {
            module.compile_function(function)?;
        }

        Ok(module)
    }

    /// Compile a WIR function to WASM with enhanced wasm_encoder integration
    pub fn compile_function(&mut self, wir_function: &WirFunction) -> Result<(), CompileError> {
        // Register function in the function registry
        self.function_registry
            .insert(wir_function.name.clone(), self.function_count);

        // Prepare function compilation context
        let (param_types, result_types, local_map) = self.prepare_function_context(wir_function)?;

        // Create and compile function body
        let function = self.create_and_compile_function_body(
            wir_function,
            param_types,
            result_types,
            local_map,
        )?;

        // Finalize function registration
        self.finalize_function_registration(wir_function, function);

        Ok(())
    }

    /// Prepare function compilation context (types and local mapping)
    fn prepare_function_context(
        &mut self,
        wir_function: &WirFunction,
    ) -> Result<(Vec<ValType>, Vec<ValType>, LocalMap), CompileError> {
        // Analyze local variable requirements
        let analyzer = LocalAnalyzer::analyze_function(wir_function);
        let local_map = analyzer.build_local_mapping(wir_function);

        // Create function type using wasm_encoder's type-safe builders
        let param_types: Vec<ValType> = wir_function
            .parameters
            .iter()
            .map(|p| self.wasm_type_to_val_type(&p.wasm_type()))
            .collect();

        let result_types: Vec<ValType> = wir_function
            .return_types
            .iter()
            .map(|t| self.wasm_type_to_val_type(t))
            .collect();

        // Use wasm_encoder's type-safe function signatures
        self.type_section
            .ty()
            .function(param_types.clone(), result_types.clone());

        // Add function to function section
        self.function_section.function(self.type_count);

        Ok((param_types, result_types, local_map))
    }

    /// Create function builder and compile function body
    fn create_and_compile_function_body(
        &mut self,
        wir_function: &WirFunction,
        param_types: Vec<ValType>,
        result_types: Vec<ValType>,
        local_map: LocalMap,
    ) -> Result<Function, CompileError> {
        // Analyze local variable requirements for wasm_locals
        let analyzer = LocalAnalyzer::analyze_function(wir_function);
        let wasm_locals = analyzer.generate_wasm_locals();

        // Create enhanced function builder with wasm_encoder integration
        let mut function_builder = EnhancedFunctionBuilder::new(
            wasm_locals,
            param_types,
            result_types,
            wir_function.name.clone(),
        );

        // Compile function body using enhanced builder
        self.compile_function_body(wir_function, &mut function_builder, &local_map)?;

        // Finalize function with wasm_encoder validation
        function_builder.finalize()
    }

    /// Generate enhanced function metadata for named returns and references
    fn generate_function_metadata(&self, wir_function: &WirFunction) -> FunctionMetadata {
        let mut return_info = Vec::new();

        for (i, return_arg) in wir_function.return_args.iter().enumerate() {
            return_info.push(ReturnParameterInfo {
                index: i,
                name: return_arg.id.clone(),
                data_type: return_arg.value.data_type.clone(),
                is_reference: self.is_datatype_reference(&return_arg.value.data_type),
            });
        }

        FunctionMetadata {
            name: wir_function.name.clone(),
            return_parameters: return_info,
        }
    }

    /// Check if a DataType represents a reference for WASM generation
    fn is_datatype_reference(&self, data_type: &DataType) -> bool {
        // For now, return false as reference types aren't fully implemented
        // This would be enhanced when reference types are properly implemented
        match data_type {
            // Add reference type detection logic here
            _ => false,
        }
    }

    /// Finalize function registration and add to module
    fn finalize_function_registration(&mut self, wir_function: &WirFunction, function: Function) {
        // Generate enhanced metadata for named returns and references
        let metadata = self.generate_function_metadata(wir_function);

        // Store metadata for debugging and error reporting
        self.function_metadata
            .insert(wir_function.name.clone(), metadata);

        // Store source information for error reporting
        let source_info = FunctionSourceInfo {
            function_name: wir_function.name.clone(),
            source_file: None, // TODO: Extract from WIR when available
            start_line: 1,     // TODO: Extract from WIR when available
            end_line: 1,       // TODO: Extract from WIR when available
            instruction_locations: Vec::new(), // TODO: Extract from function_builder
        };
        self.function_source_map
            .insert(self.function_count, source_info);

        // Add function to code section
        self.code_section.function(&function);

        self.function_count += 1;
        self.type_count += 1;

        #[cfg(feature = "verbose_codegen_logging")]
        {
            println!(
                "WASM: Successfully compiled function '{}' with {} blocks",
                wir_function.name,
                wir_function.blocks.len()
            );
            for (i, block) in wir_function.blocks.iter().enumerate() {
                println!(
                    "  Block {}: {} statements, terminator: {:?}",
                    i,
                    block.statements.len(),
                    block.terminator
                );
                for (j, statement) in block.statements.iter().enumerate() {
                    println!("    Statement {}: {:?}", j, statement);
                }
            }
        }
    }

    /// Compile function body using enhanced wasm_encoder integration
    fn compile_function_body(
        &mut self,
        wir_function: &WirFunction,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Lower each block with enhanced control flow tracking
        for (block_index, block) in wir_function.blocks.iter().enumerate() {
            function_builder.begin_block(block_index as u32)?;

            // Lower statements
            for statement in &block.statements {
                self.lower_statement_enhanced(statement, function_builder, local_map)?;
            }

            // Lower terminator with enhanced control flow validation
            self.lower_terminator_enhanced(&block.terminator, function_builder, local_map)?;

            function_builder.end_block()?;
        }

        Ok(())
    }

    /// Lower a WIR block to WASM instructions
    fn lower_block_to_wasm(
        &mut self,
        block: &crate::compiler::wir::wir_nodes::WirBlock,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Lower each statement
        for statement in &block.statements {
            self.lower_statement(statement, function, local_map)?;
        }

        // Lower the terminator

        self.lower_terminator(&block.terminator, function, local_map)?;

        Ok(())
    }

    /// Lower a WIR statement to WASM instructions with enhanced validation (WIR-faithful implementation)
    ///
    /// This method implements direct WIR-to-WASM lowering following the WIR design principle:
    /// Each WIR statement maps to ≤3 WASM instructions for efficient compilation.
    ///
    /// ## WIR Statement Mapping
    /// - `Assign`: rvalue evaluation + place assignment (≤3 instructions)
    /// - `Call`: argument loading + call instruction + result storage (≤3 instructions)
    /// - `HostCall`: argument loading + call instruction + result storage (≤3 instructions)
    /// - `Nop`: no instructions (0 instructions)
    fn lower_statement_enhanced(
        &mut self,
        statement: &Statement,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { place, rvalue } => {
                // Beanstalk-aware assignment: handles both regular and mutable (~) assignments
                self.lower_beanstalk_aware_assignment_enhanced(
                    place,
                    rvalue,
                    function_builder,
                    local_map,
                )
            }
            Statement::Call {
                func,
                args,
                destination,
            } => {
                // WIR-faithful function call: args → call → result (≤3 instructions per arg + 1 call + 1 store)
                self.lower_wir_call_enhanced(func, args, destination, function_builder, local_map)
            }
            Statement::HostCall {
                function: host_func,
                args,
                destination,
            } => {
                // WIR-faithful host call: args → call → result (≤3 instructions per arg + 1 call + 1 store)
                self.lower_wir_host_call_enhanced(
                    host_func,
                    args,
                    destination,
                    function_builder,
                    local_map,
                )
            }
            Statement::WasixCall {
                function_name,
                args,
                destination,
            } => {
                // WASIX function call: handled by WASIX registry
                self.lower_wasix_host_call(
                    function_name,
                    args,
                    destination,
                    function_builder,
                    local_map,
                )
            }
            Statement::MarkFieldInitialized { .. } => {
                // Field initialization tracking - no WASM instructions needed
                // This is handled at compile time for validation
                Ok(())
            }
            Statement::ValidateStructInitialization {
                struct_place,
                struct_type,
            } => {
                // Struct validation - generate runtime check if needed
                self.lower_struct_validation(
                    struct_place,
                    struct_type,
                    function_builder,
                    &mut local_map.clone(),
                )
            }
            Statement::Conditional {
                condition,
                then_statements,
                else_statements,
            } => {
                // Lower the condition operand to get it on the stack
                self.lower_operand_enhanced(condition, function_builder, local_map)?;

                // Create WASM if/else block structure
                // The condition is already on the stack from lower_operand_enhanced
                
                // Determine block type based on whether branches produce values
                // For now, use empty block type (no return value)
                let block_type = wasm_encoder::BlockType::Empty;
                
                // Emit if instruction
                function_builder.instruction(&Instruction::If(block_type))?;

                // Lower then block statements
                for stmt in then_statements {
                    self.lower_statement_enhanced(stmt, function_builder, local_map)?;
                }

                // Emit else instruction if there are else statements
                if !else_statements.is_empty() {
                    function_builder.instruction(&Instruction::Else)?;
                    
                    // Lower else block statements
                    for stmt in else_statements {
                        self.lower_statement_enhanced(stmt, function_builder, local_map)?;
                    }
                }

                // End the if/else block
                function_builder.instruction(&Instruction::End)?;

                Ok(())
            }
            Statement::Nop => {
                // No-op generates no instructions (0 instructions)
                Ok(())
            }
            Statement::InterfaceCall {
                interface_id: _,
                method_id: _,
                receiver,
                args,
                destination,
            } => {
                // Interface calls are lowered to indirect calls through vtables
                // For now, treat as regular function calls until vtable system is implemented
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Interface call lowered as direct call (vtable dispatch not yet implemented)"
                );

                // Load receiver as first argument
                self.lower_operand_enhanced(receiver, function_builder, local_map)?;

                // Load remaining arguments
                for arg in args {
                    self.lower_operand_enhanced(arg, function_builder, local_map)?;
                }

                // For now, generate a placeholder call (will be replaced with vtable dispatch)
                // This maintains the WIR principle of direct lowering
                function_builder.instruction(&Instruction::Call(0))?; // Placeholder function index

                // Store result if destination exists
                if let Some(dest_place) = destination {
                    let return_type = dest_place.wasm_type();
                    self.lower_place_assignment_with_type_enhanced(
                        dest_place,
                        &return_type,
                        function_builder,
                        local_map,
                    )?;
                }

                Ok(())
            }
            Statement::Alloc { place, size, .. } => {
                // Memory allocation: size → alloc → place (≤3 instructions)
                self.lower_operand_enhanced(size, function_builder, local_map)?;

                // For now, use a simple linear memory allocation strategy
                // This will be enhanced when proper memory management is implemented
                function_builder.instruction(&Instruction::I32Const(0))?; // Base memory address
                function_builder.instruction(&Instruction::I32Add)?; // Add size to get allocation

                let alloc_type = WasmType::I32; // Allocation returns pointer (i32)
                self.lower_place_assignment_with_type_enhanced(
                    place,
                    &alloc_type,
                    function_builder,
                    local_map,
                )?;

                Ok(())
            }
            Statement::Dealloc { place: _ } => {
                // Deallocation: for now, this is a no-op in linear memory model
                // Future enhancement: integrate with proper memory management
                Ok(())
            }
            Statement::Store { place, value, .. } => {
                // Memory store: value → address → store (≤3 instructions)
                self.lower_operand_enhanced(value, function_builder, local_map)?;

                // Generate address for the place
                match place {
                    Place::Memory { base, offset, .. } => {
                        self.lower_memory_base_address_enhanced(base, function_builder)?;
                        if offset.0 > 0 {
                            function_builder
                                .instruction(&Instruction::I32Const(offset.0 as i32))?;
                            function_builder.instruction(&Instruction::I32Add)?;
                        }
                        function_builder.instruction(&Instruction::I32Store(MemArg {
                            align: 0,
                            offset: 0,
                            memory_index: 0,
                        }))?;
                    }
                    _ => {
                        return_compiler_error!(
                            "Store statement with non-memory place not yet implemented: {:?}",
                            place
                        );
                    }
                }

                Ok(())
            }
            Statement::Drop { place: _ } => {
                // Drop: for now, this is a no-op (will be enhanced with proper drop semantics)
                Ok(())
            }
            Statement::MemoryOp {
                operand, result, ..
            } => {
                // Memory operations: operand → operation → result (≤3 instructions)
                if let Some(op) = operand {
                    self.lower_operand_enhanced(op, function_builder, local_map)?;
                }

                // For now, placeholder memory operation
                function_builder.instruction(&Instruction::Nop)?;

                if let Some(res) = result {
                    let result_type = res.wasm_type();
                    self.lower_place_assignment_with_type_enhanced(
                        res,
                        &result_type,
                        function_builder,
                        local_map,
                    )?;
                }

                Ok(())
            }
        }
    }

    /// Legacy method for compatibility - delegates to enhanced version
    fn lower_statement(
        &mut self,
        statement: &Statement,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { place, rvalue } => {
                // Beanstalk-aware assignment: handles both regular and mutable (~) assignments
                self.lower_beanstalk_aware_assignment(place, rvalue, function, local_map)
            }
            Statement::Call {
                func,
                args,
                destination,
            } => {
                // WIR-faithful function call: args → call → result (≤3 instructions per arg + 1 call + 1 store)
                self.lower_wir_call(func, args, destination, function, local_map)
            }
            Statement::HostCall {
                function: host_func,
                args,
                destination,
            } => {
                // WIR-faithful host call: args → call → result (≤3 instructions per arg + 1 call + 1 store)
                self.lower_wir_host_call(host_func, args, destination, function, local_map)
            }
            Statement::WasixCall {
                function_name,
                args,
                destination,
            } => {
                // WASIX function call: handled by WASIX registry (simplified version)
                self.lower_wasix_host_call_simple(
                    function_name,
                    args,
                    destination,
                    function,
                    local_map,
                )
            }
            Statement::MarkFieldInitialized { .. } => {
                // Field initialization tracking - no WASM instructions needed
                // This is handled at compile time for validation
                Ok(())
            }
            Statement::ValidateStructInitialization {
                struct_place,
                struct_type,
            } => {
                // Struct validation - generate runtime check if needed
                self.lower_struct_validation_simple(
                    struct_place,
                    struct_type,
                    function,
                    &mut local_map.clone(),
                )
            }
            Statement::Conditional {
                condition,
                then_statements,
                else_statements,
            } => {
                // Lower the condition operand to get it on the stack
                self.lower_operand(condition, function, local_map)?;

                // Create WASM if/else block structure
                // The condition is already on the stack from lower_operand
                
                // Determine block type based on whether branches produce values
                // For now, use empty block type (no return value)
                let block_type = wasm_encoder::BlockType::Empty;
                
                // Emit if instruction
                function.instruction(&Instruction::If(block_type));

                // Lower then block statements
                for stmt in then_statements {
                    self.lower_statement(stmt, function, local_map)?;
                }

                // Emit else instruction if there are else statements
                if !else_statements.is_empty() {
                    function.instruction(&Instruction::Else);
                    
                    // Lower else block statements
                    for stmt in else_statements {
                        self.lower_statement(stmt, function, local_map)?;
                    }
                }

                // End the if/else block
                function.instruction(&Instruction::End);

                Ok(())
            }
            Statement::Nop => {
                // No-op generates no instructions (0 instructions)
                Ok(())
            }
            Statement::InterfaceCall {
                interface_id: _,
                method_id: _,
                receiver,
                args,
                destination,
            } => {
                // Interface calls are lowered to indirect calls through vtables
                // For now, treat as regular function calls until vtable system is implemented
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Interface call lowered as direct call (vtable dispatch not yet implemented)"
                );

                // Load receiver as first argument
                self.lower_operand(receiver, function, local_map)?;

                // Load remaining arguments
                for arg in args {
                    self.lower_operand(arg, function, local_map)?;
                }

                // For now, generate a placeholder call (will be replaced with vtable dispatch)
                // This maintains the WIR principle of direct lowering
                function.instruction(&Instruction::Call(0)); // Placeholder function index

                // Store result if destination exists
                if let Some(dest_place) = destination {
                    let return_type = dest_place.wasm_type();
                    self.lower_place_assignment_with_type(
                        dest_place,
                        &return_type,
                        function,
                        local_map,
                    )?;
                }

                Ok(())
            }
            Statement::Alloc { place, size, .. } => {
                // Memory allocation: size → alloc → place (≤3 instructions)
                self.lower_operand(size, function, local_map)?;

                // For now, use a simple linear memory allocation strategy
                // This will be enhanced when proper memory management is implemented
                function.instruction(&Instruction::I32Const(0)); // Base memory address
                function.instruction(&Instruction::I32Add); // Add size to get allocation

                let alloc_type = WasmType::I32; // Allocation returns pointer (i32)
                self.lower_place_assignment_with_type(place, &alloc_type, function, local_map)?;

                Ok(())
            }
            Statement::Dealloc { place: _ } => {
                // Deallocation: for now, this is a no-op in linear memory model
                // Future enhancement: integrate with proper memory management
                Ok(())
            }
            Statement::Store { place, value, .. } => {
                // Memory store: value → address → store (≤3 instructions)
                self.lower_operand(value, function, local_map)?;

                // Generate address for the place
                match place {
                    Place::Memory { base, offset, .. } => {
                        self.lower_memory_base_address(base, function)?;
                        if offset.0 > 0 {
                            function.instruction(&Instruction::I32Const(offset.0 as i32));
                            function.instruction(&Instruction::I32Add);
                        }

                        // Store with appropriate instruction based on value type
                        let value_type = self.get_operand_wasm_type(value)?;
                        match value_type {
                            WasmType::I32 => {
                                function.instruction(&Instruction::I32Store(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::I64 => {
                                function.instruction(&Instruction::I64Store(MemArg {
                                    offset: 0,
                                    align: 3,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::F32 => {
                                function.instruction(&Instruction::F32Store(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::F64 => {
                                function.instruction(&Instruction::F64Store(MemArg {
                                    offset: 0,
                                    align: 3,
                                    memory_index: 0,
                                }));
                            }
                            _ => return_compiler_error!("Unsupported store type: {:?}", value_type),
                        }
                    }
                    _ => {
                        // For other place types, use the standard assignment lowering
                        let value_type = self.get_operand_wasm_type(value)?;
                        self.lower_place_assignment_with_type(
                            place,
                            &value_type,
                            function,
                            local_map,
                        )?;
                    }
                }

                Ok(())
            }
            Statement::Drop { place: _ } => {
                // Drop operations are handled by the borrow checker
                // In WASM, this is typically a no-op for basic types
                Ok(())
            }
            Statement::MemoryOp {
                op,
                operand,
                result,
            } => {
                // WASM-specific memory operations
                match op {
                    MemoryOpKind::Size => {
                        function.instruction(&Instruction::MemorySize(0));
                        if let Some(dest) = result {
                            let result_type = WasmType::I32;
                            self.lower_place_assignment_with_type(
                                dest,
                                &result_type,
                                function,
                                local_map,
                            )?;
                        }
                    }
                    MemoryOpKind::Grow => {
                        if let Some(operand) = operand {
                            self.lower_operand(operand, function, local_map)?;
                        } else {
                            function.instruction(&Instruction::I32Const(1)); // Default: grow by 1 page
                        }
                        function.instruction(&Instruction::MemoryGrow(0));
                        if let Some(dest) = result {
                            let result_type = WasmType::I32;
                            self.lower_place_assignment_with_type(
                                dest,
                                &result_type,
                                function,
                                local_map,
                            )?;
                        }
                    }
                    _ => {
                        return_unimplemented_feature_error!(
                            &format!("Memory operation '{:?}'", op),
                            None,
                            Some("use basic memory operations for now")
                        );
                    }
                }
                Ok(())
            }
        }
    }

    /// Lower WIR assignment statement (≤3 WASM instructions)
    fn lower_wir_assign(
        &mut self,
        place: &Place,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Step 1: Evaluate rvalue and put result on stack (≤2 instructions)
        self.lower_rvalue(rvalue, function, local_map)?;

        // Step 2: Validate type consistency between place and value
        let value_type = self.get_rvalue_wasm_type(rvalue)?;
        let place_type = place.wasm_type();

        if value_type != place_type {
            return_compiler_error!(
                "Type mismatch in assignment: place expects {:?} but rvalue provides {:?}. Place: {:?}, Rvalue: {:?}",
                place_type,
                value_type,
                place,
                rvalue
            );
        }

        // Step 3: Store stack value to place (≤1 instruction for locals/globals)
        self.lower_place_assignment_with_type(place, &value_type, function, local_map)?;

        Ok(())
    }

    /// Lower WIR function call statement (direct WASM call)
    fn lower_wir_call(
        &mut self,
        func: &Operand,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Step 1: Load all arguments onto stack (1 instruction per arg)
        for arg in args {
            self.lower_operand(arg, function, local_map)?;
        }

        // Step 2: Generate call instruction (1 instruction)
        match func {
            Operand::FunctionRef(func_index) => {
                function.instruction(&Instruction::Call(*func_index));
            }
            Operand::Constant(Constant::Function(func_index)) => {
                function.instruction(&Instruction::Call(*func_index));
            }
            _ => {
                return_compiler_error!(
                    "WIR function calls must use direct function references, found: {:?}",
                    func
                );
            }
        }

        // Step 3: Store result if destination exists (1 instruction)
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type(dest_place, &return_type, function, local_map)?;
        }

        Ok(())
    }

    /// Lower WIR host function call statement (direct WASM call to imported function)
    fn lower_wir_host_call(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Check for runtime-specific mapping if registry is available
        if let Some(registry) = self.host_registry.clone() {
            return self.lower_host_call_with_registry(
                host_function,
                args,
                destination,
                function,
                local_map,
                &registry,
            );
        }

        // Fallback to original logic if no registry available
        // Check if this is a WASIX function (identified by module name)
        if host_function.module == "wasix_32v1" {
            // This is a WASIX function - handle it specially
            return self.lower_wasix_host_call_simple(
                &host_function.name,
                args,
                destination,
                function,
                local_map,
            );
        }

        // Step 1: Load all arguments onto stack
        // For string/template arguments, we need to push both pointer and length
        for (arg_index, arg) in args.iter().enumerate() {
            // Check if this parameter is a string/template type
            let param_type = if arg_index < host_function.parameters.len() {
                &host_function.parameters[arg_index].data_type
            } else {
                // If we don't have parameter info, infer from operand
                &DataType::Int // Default fallback
            };

            match param_type {
                DataType::String | DataType::Template => {
                    // For string/template types, push pointer and length
                    match arg {
                        Operand::Constant(Constant::String(s)) => {
                            // Add string to data section and get offset
                            let offset = self.string_manager.add_string_slice_constant(s);
                            let length = s.len() as i32;
                            
                            // Push pointer
                            function.instruction(&Instruction::I32Const(offset as i32));
                            // Push length
                            function.instruction(&Instruction::I32Const(length));
                        }
                        _ => {
                            // For other operand types, lower normally and assume it's a pointer
                            // Then we need to load the length somehow - for now, just push 0
                            self.lower_operand(arg, function, local_map)?;
                            function.instruction(&Instruction::I32Const(0)); // TODO: Get actual length
                        }
                    }
                }
                _ => {
                    // For non-string types, lower normally
                    self.lower_operand(arg, function, local_map)?;
                }
            }
        }

        // Step 2: Generate call instruction to imported host function (1 instruction)
        let func_index = self
            .host_function_indices
            .get(&host_function.name)
            .ok_or_else(|| {
                CompileError::compiler_error(&format!(
                    "Host function '{}' not found in import table",
                    host_function.name
                ))
            })?;

        function.instruction(&Instruction::Call(*func_index));

        // Step 3: Store result if destination exists (1 instruction)
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type(dest_place, &return_type, function, local_map)?;
        }

        Ok(())
    }

    /// Lower host function call with registry-aware runtime mapping
    fn lower_host_call_with_registry(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &LocalMap,
        registry: &crate::compiler::host_functions::registry::HostFunctionRegistry,
    ) -> Result<(), CompileError> {
        use crate::compiler::host_functions::registry::RuntimeFunctionMapping;

        // Get runtime-specific mapping
        match registry.get_runtime_mapping(&host_function.name) {
            Some(RuntimeFunctionMapping::Wasix(wasix_func)) => {
                // Use WASIX fd_write generation for print and template_output calls
                if host_function.name == "print" || host_function.name == "template_output" {
                    return self.generate_wasix_fd_write_call(args, function, local_map);
                }
                
                // For other WASIX functions, use standard call lowering
                self.lower_standard_host_call(host_function, args, destination, function, local_map)
            }
            Some(RuntimeFunctionMapping::JavaScript(_js_func)) => {
                // Use JavaScript binding (not implemented yet)
                return_compiler_error!(
                    "JavaScript runtime mappings not yet implemented for host function '{}'",
                    host_function.name
                );
            }
            Some(RuntimeFunctionMapping::Native(_)) | None => {
                // Use standard host function call
                self.lower_standard_host_call(host_function, args, destination, function, local_map)
            }
        }
    }

    /// Lower standard host function call (non-WASIX)
    fn lower_standard_host_call(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Step 1: Load all arguments onto stack
        // For string/template arguments, we need to push both pointer and length
        for (arg_index, arg) in args.iter().enumerate() {
            // Check if this parameter is a string/template type
            let param_type = if arg_index < host_function.parameters.len() {
                &host_function.parameters[arg_index].data_type
            } else {
                // If we don't have parameter info, infer from operand
                &DataType::Int // Default fallback
            };

            match param_type {
                DataType::String | DataType::Template => {
                    // For string/template types, push pointer and length
                    match arg {
                        Operand::Constant(Constant::String(s)) => {
                            // Add string to data section and get offset
                            let offset = self.string_manager.add_string_slice_constant(s);
                            let length = s.len() as i32;
                            
                            // Push pointer
                            function.instruction(&Instruction::I32Const(offset as i32));
                            // Push length
                            function.instruction(&Instruction::I32Const(length));
                        }
                        _ => {
                            // For other operand types, lower normally and assume it's a pointer
                            // Then we need to load the length somehow - for now, just push 0
                            self.lower_operand(arg, function, local_map)?;
                            function.instruction(&Instruction::I32Const(0)); // TODO: Get actual length
                        }
                    }
                }
                _ => {
                    // For non-string types, lower normally
                    self.lower_operand(arg, function, local_map)?;
                }
            }
        }

        // Step 2: Generate call instruction to imported host function (1 instruction)
        let func_index = self
            .host_function_indices
            .get(&host_function.name)
            .ok_or_else(|| {
                CompileError::compiler_error(&format!(
                    "Host function '{}' not found in import table",
                    host_function.name
                ))
            })?;

        function.instruction(&Instruction::Call(*func_index));

        // Step 3: Store result if destination exists (1 instruction)
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type(dest_place, &return_type, function, local_map)?;
        }

        Ok(())
    }

    /// Generate WASIX fd_write call for print statements
    ///
    /// This method implements subtask 3.2: string-to-IOVec conversion for WASIX calls
    /// It converts a Beanstalk print() call into a WASIX fd_write call by:
    /// 1. Adding string data to linear memory with proper alignment
    /// 2. Creating IOVec structure pointing to string data
    /// 3. Generating WASM instruction sequence for WASIX fd_write call
    /// 4. Handling WASIX calling conventions and return values
    fn generate_wasix_fd_write_call(
        &mut self,
        args: &[Operand],
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Validate arguments - print() should have exactly one string argument
        if args.len() != 1 {
            return_compiler_error!(
                "print() function expects exactly 1 argument, got {}. This should be caught during type checking.",
                args.len()
            );
        }

        let string_arg = &args[0];

        // Extract string content from the operand
        let string_content = match string_arg {
            Operand::Constant(Constant::String(content)) => content.clone(),
            _ => {
                // For non-constant strings, we need to handle them differently
                // For now, this is a limitation - we only support string literals
                return_compiler_error!(
                    "WASIX print() currently only supports string literals. Variable string printing not yet implemented."
                );
            }
        };

        // Step 1: Add string data to linear memory allocation
        let string_offset = self.string_manager.add_string_slice_constant(&string_content);
        let string_len = string_content.len() as u32;

        // Skip the 4-byte length prefix to get to the actual string data
        let string_ptr = string_offset + 4;

        // Step 2: Implement string-to-IOVec conversion for WASIX calls
        let iovec = crate::compiler::host_functions::wasix_registry::IOVec::new(string_ptr, string_len);

        // Add IOVec structure to data section with proper WASIX alignment
        let iovec_bytes = iovec.to_bytes();
        let iovec_offset = self.string_manager.add_raw_data(&iovec_bytes);

        // Allocate space for nwritten result with WASIX alignment
        let nwritten_bytes = [0u8; 4]; // Initialize to 0
        let nwritten_offset = self.string_manager.add_raw_data(&nwritten_bytes);

        // Get the WASIX fd_write function index
        let wasix_function = match self.wasix_registry.get_function("print") {
            Some(func) => func,
            None => {
                return_compiler_error!(
                    "WASIX function 'print' not found in registry. This should be registered during module initialization."
                );
            }
        };

        let fd_write_func_index = wasix_function.get_function_index()?;

        // Step 3: Generate WASM instruction sequence for WASIX fd_write call
        // Handle WASIX calling conventions: fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32

        // Load stdout file descriptor (constant 1) onto WASM stack
        function.instruction(&Instruction::I32Const(1));

        // Load IOVec pointer (offset in linear memory where IOVec structure is stored)
        function.instruction(&Instruction::I32Const(iovec_offset as i32));

        // Load IOVec count (1 for single string argument)
        function.instruction(&Instruction::I32Const(1));

        // Load nwritten result pointer (where fd_write will store bytes written)
        function.instruction(&Instruction::I32Const(nwritten_offset as i32));

        // Generate call instruction to imported fd_write function
        function.instruction(&Instruction::Call(fd_write_func_index));

        // Step 4: Handle WASIX return values
        // fd_write returns errno (0 for success, non-zero for error)
        // For now, we'll just drop the return value, but this provides foundation for error handling
        function.instruction(&Instruction::Drop);

        Ok(())
    }

    /// Lower WASIX host function call (simplified version for Function)
    fn lower_wasix_host_call_simple(
        &mut self,
        function_name: &str,
        args: &[Operand],
        _destination: &Option<Place>,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match function_name {
            "print" | "template_output" => self.lower_wasix_print_simple(args, function, local_map),
            _ => {
                return_compiler_error!(
                    "Unsupported WASIX function: {}. Only 'print' and 'template_output' are currently implemented.",
                    function_name
                );
            }
        }
    }

    /// Lower print() function to WASIX fd_write call (simplified version for Function)
    fn lower_wasix_print_simple(
        &mut self,
        args: &[Operand],
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Validate arguments - print() should have exactly one string argument
        if args.len() != 1 {
            return_compiler_error!(
                "print() function expects exactly 1 argument, got {}. This should be caught during type checking.",
                args.len()
            );
        }

        let string_arg = &args[0];

        // Check if this is a constant string or a variable
        match string_arg {
            Operand::Constant(Constant::String(content)) => {
                // String literal - use the existing implementation
                self.lower_wasix_print_constant(content, function)
            }
            Operand::Copy(place) | Operand::Move(place) => {
                // String variable - use runtime implementation
                self.lower_wasix_print_variable(place, function, local_map)
            }
            _ => {
                return_compiler_error!(
                    "print() argument must be a string literal or string variable, got {:?}",
                    string_arg
                );
            }
        }
    }

    /// Print a constant string literal (compile-time known)
    fn lower_wasix_print_constant(
        &mut self,
        string_content: &str,
        function: &mut Function,
    ) -> Result<(), CompileError> {

        // Get the WASIX fd_write function index
        let wasix_function = match self.wasix_registry.get_function("print") {
            Some(func) => func,
            None => {
                return_compiler_error!(
                    "WASIX function 'print' not found in registry. This should be registered during module initialization."
                );
            }
        };

        let fd_write_func_index = wasix_function.get_function_index()?;

        // Add string data to WASM data section with proper alignment
        let string_offset = self.string_manager.add_string_constant(&string_content);
        let string_len = string_content.len() as u32;

        // FIXED: Use the StringManager offset (where data actually exists) instead of WasixMemoryManager
        // The StringManager writes actual string data to the WASM data section
        // Skip the 4-byte length prefix to get to the actual string data
        let string_ptr = string_offset + 4;

        // Create IOVec structure with WASIX alignment (8-byte aligned)
        let _iovec_ptr = match self.wasix_memory_manager.allocate_iovec_array(1) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX IOVec allocation failed: {}", e);
            }
        };
        let iovec =
            crate::compiler::host_functions::wasix_registry::IOVec::new(string_ptr, string_len);

        // Add IOVec structure to data section with proper WASIX alignment
        let iovec_bytes = iovec.to_bytes();
        let iovec_offset = self.add_raw_data_to_section(&iovec_bytes);

        // Allocate space for nwritten result with WASIX alignment
        let _nwritten_ptr = match self.wasix_memory_manager.allocate(4, 4) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX nwritten allocation failed: {}", e);
            }
        };
        let nwritten_bytes = [0u8; 4]; // Initialize to 0
        let nwritten_offset = self.add_raw_data_to_section(&nwritten_bytes);

        // Generate WASM instruction sequence for fd_write call
        // Load stdout file descriptor (constant 1) onto WASM stack
        function.instruction(&Instruction::I32Const(1));

        // Load IOVec pointer (offset in linear memory where IOVec structure is stored)
        function.instruction(&Instruction::I32Const(iovec_offset as i32));

        // Load IOVec count (1 for single string argument)
        function.instruction(&Instruction::I32Const(1));

        // Load nwritten result pointer (where fd_write will store bytes written)
        function.instruction(&Instruction::I32Const(nwritten_offset as i32));

        // Generate call instruction to imported fd_write function
        function.instruction(&Instruction::Call(fd_write_func_index));

        // Handle the return value (errno) for basic error detection
        // Store errno in a local variable for potential error checking
        // For now, we'll just drop it, but this provides the foundation for error handling
        function.instruction(&Instruction::Drop);

        Ok(())
    }

    /// Print a string variable (runtime value)
    fn lower_wasix_print_variable(
        &mut self,
        place: &Place,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Get the WASIX fd_write function index
        let wasix_function = match self.wasix_registry.get_function("print") {
            Some(func) => func,
            None => {
                return_compiler_error!(
                    "WASIX function 'print' not found in registry. This should be registered during module initialization."
                );
            }
        };

        let fd_write_func_index = wasix_function.get_function_index()?;

        // Load the string pointer from the variable
        // The place contains a pointer to: [length: u32][data: bytes]
        self.lower_place_access(place, function, local_map)?;
        
        // Stack: [string_ptr]
        // Duplicate for later use
        function.instruction(&Instruction::LocalTee(0)); // Save string_ptr in local 0
        
        // Read the length (first 4 bytes)
        function.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2, // 4-byte alignment
            memory_index: 0,
        }));
        
        // Stack: [length]
        function.instruction(&Instruction::LocalSet(1)); // Save length in local 1
        
        // Calculate data pointer (string_ptr + 4)
        function.instruction(&Instruction::LocalGet(0));
        function.instruction(&Instruction::I32Const(4));
        function.instruction(&Instruction::I32Add);
        function.instruction(&Instruction::LocalSet(2)); // Save data_ptr in local 2
        
        // Now we need to create an IOVec structure at runtime
        // Allocate IOVec space (8 bytes: ptr + len)
        let iovec_ptr = match self.wasix_memory_manager.allocate_iovec_array(1) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX IOVec allocation failed: {}", e);
            }
        };
        
        // Write data_ptr to IOVec (first 4 bytes)
        function.instruction(&Instruction::I32Const(iovec_ptr as i32));
        function.instruction(&Instruction::LocalGet(2)); // data_ptr
        function.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        
        // Write length to IOVec (next 4 bytes)
        function.instruction(&Instruction::I32Const(iovec_ptr as i32 + 4));
        function.instruction(&Instruction::LocalGet(1)); // length
        function.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));
        
        // Allocate space for nwritten result
        let nwritten_ptr = match self.wasix_memory_manager.allocate(4, 4) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX nwritten allocation failed: {}", e);
            }
        };
        
        // Call fd_write(fd=1, iovs=iovec_ptr, iovs_len=1, nwritten=nwritten_ptr)
        function.instruction(&Instruction::I32Const(1)); // stdout fd
        function.instruction(&Instruction::I32Const(iovec_ptr as i32));
        function.instruction(&Instruction::I32Const(1)); // iovs_len
        function.instruction(&Instruction::I32Const(nwritten_ptr as i32));
        function.instruction(&Instruction::Call(fd_write_func_index));
        
        // Drop the return value (errno)
        function.instruction(&Instruction::Drop);
        
        Ok(())
    }

    /// Lower a WIR rvalue to WASM instructions
    fn lower_rvalue(
        &mut self,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match rvalue {
            Rvalue::Use(operand) => self.lower_operand(operand, function, local_map),
            Rvalue::BinaryOp(op, left, right) => {
                // Determine the WASM type from the operands
                let wasm_type = self.get_operand_wasm_type(left)?;
                self.lower_operand(left, function, local_map)?;
                self.lower_operand(right, function, local_map)?;
                self.lower_binary_op(op, &wasm_type, function)
            }
            Rvalue::UnaryOp(op, operand) => {
                self.lower_operand(operand, function, local_map)?;
                self.lower_unary_op(op, function)
            }
            Rvalue::Ref { place, borrow_kind } => {
                // Implement Beanstalk's implicit borrowing semantics in WASM
                self.lower_beanstalk_reference(place, borrow_kind, function, local_map)
            }
            Rvalue::StringConcat(left, right) => {
                // String concatenation: load both operands and call string concat helper
                self.lower_operand(left, function, local_map)?;
                self.lower_operand(right, function, local_map)?;
                // For now, just leave both values on the stack
                // TODO: Implement actual string concatenation via runtime helper
                Ok(())
            }
        }
    }

    /// Lower a WIR operand to WASM instructions (WIR-faithful operand lowering)
    ///
    /// This method implements direct WIR operand lowering following WIR design principles:
    /// - Operand::Copy → place access (≤3 instructions)
    /// - Operand::Move → place access + ownership semantics (≤3 instructions)
    /// - Operand::Constant → immediate value (1 instruction)
    /// - Operand::FunctionRef → function index constant (1 instruction)
    /// - Operand::GlobalRef → global access (1 instruction)
    fn lower_operand(
        &mut self,
        operand: &Operand,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match operand {
            Operand::Constant(constant) => {
                // Direct constant lowering (1 instruction)
                self.lower_wir_constant(constant, function)
            }
            Operand::Copy(place) => {
                // Copy operation: non-consuming read from place (≤3 instructions)
                self.lower_wir_copy(place, function, local_map)
            }
            Operand::Move(place) => {
                // Move operation: consuming read from place (≤3 instructions)
                self.lower_wir_move(place, function, local_map)
            }
            Operand::FunctionRef(func_index) => {
                // Function reference as WASM function index (1 instruction)
                function.instruction(&Instruction::I32Const(*func_index as i32));

                #[cfg(feature = "verbose_codegen_logging")]
                println!("WASM: function reference {} as i32.const", func_index);

                Ok(())
            }
            Operand::GlobalRef(global_index) => {
                // Global reference: load global value (1 instruction)
                let wasm_global = local_map.get_global(*global_index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "WIR global reference {} not mapped to WASM global",
                        global_index
                    ))
                })?;

                function.instruction(&Instruction::GlobalGet(wasm_global));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: global.get {} (WIR global ref {} → WASM global {})",
                    wasm_global, global_index, wasm_global
                );

                Ok(())
            }
        }
    }

    /// Lower WIR constant to WASM immediate instruction
    fn lower_wir_constant(
        &mut self,
        constant: &Constant,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match constant {
            Constant::I32(value) => {
                Self::emit_i32_const(function, *value);
                Ok(())
            }
            Constant::I64(value) => {
                Self::emit_i64_const(function, *value);
                Ok(())
            }
            Constant::F32(value) => {
                Self::emit_f32_const(function, *value);
                Ok(())
            }
            Constant::F64(value) => {
                Self::emit_f64_const(function, *value);
                Ok(())
            }
            Constant::Bool(value) => {
                // Booleans are represented as i32 in WASM (0 = false, 1 = true)
                Self::emit_i32_const(function, if *value { 1 } else { 0 });
                Ok(())
            }
            Constant::String(value) => {
                // String slice constants: immutable pointer to string data in data section
                let offset = self.string_manager.add_string_slice_constant(value);
                Self::emit_i32_const(function, offset as i32);

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: string slice constant '{}' at offset {}",
                    value, offset
                );

                Ok(())
            }
            Constant::MutableString(value) => {
                // Mutable string constants: heap-allocated with default capacity
                let default_capacity = (value.len() as u32).max(32); // At least 32 bytes capacity
                let offset = self
                    .string_manager
                    .allocate_mutable_string(value, default_capacity);
                Self::emit_i32_const(function, offset as i32);

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: mutable string '{}' allocated at offset {} with capacity {}",
                    value, offset, default_capacity
                );

                Ok(())
            }
            Constant::Null => {
                // Null pointer is 0 in linear memory
                Self::emit_i32_const(function, 0);
                Ok(())
            }
            Constant::Function(func_index) => {
                // Function constant as WASM function index
                Self::emit_i32_const(function, *func_index as i32);
                Ok(())
            }
            Constant::MemoryOffset(offset) => {
                // Memory offset constant for address calculations
                Self::emit_i32_const(function, *offset as i32);
                Ok(())
            }
            Constant::TypeSize(size) => {
                // Type size constant for memory operations
                Self::emit_i32_const(function, *size as i32);
                Ok(())
            }
        }
    }

    /// Lower WIR copy operand (non-consuming place access)
    fn lower_wir_copy(
        &mut self,
        place: &Place,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Copy operation: read value from place without consuming it
        // This maps directly to place access in WASM
        self.lower_place_access(place, function, local_map)?;

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: copy from place {:?}", place);

        Ok(())
    }

    /// Lower WIR move operand (consuming place access)
    fn lower_wir_move(
        &mut self,
        place: &Place,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Move operation: read value from place and consume it
        // For basic WASM types, this is identical to copy at the instruction level
        // The borrow checker ensures move semantics are respected at the WIR level
        self.lower_place_access(place, function, local_map)?;

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: move from place {:?} (ownership transfer)", place);

        // Future enhancement: For complex types, this could generate additional
        // instructions to invalidate the source place or update reference counts

        Ok(())
    }

    /// Lower place access to WASM instructions (WIR-faithful place resolution)
    ///
    /// This method implements the core WIR place resolution system that maps WIR places
    /// directly to WASM memory operations. Each place type maps to specific WASM instructions:
    /// - Place::Local → local.get (1 instruction)
    /// - Place::Global → global.get (1 instruction)  
    /// - Place::Memory → address calculation + memory.load (≤3 instructions)
    /// - Place::Projection → base access + projection calculation (≤3 instructions)
    fn lower_place_access(
        &mut self,
        place: &Place,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local {
                index,
                wasm_type: _,
            } => {
                // Direct WASM local access (1 instruction)
                let wasm_local = local_map.get_local(*index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "WIR local {} not mapped to WASM local. Check local variable analysis.",
                        index
                    ))
                })?;

                function.instruction(&Instruction::LocalGet(wasm_local));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: local.get {} (WIR local {} → WASM local {})",
                    wasm_local, index, wasm_local
                );

                Ok(())
            }

            Place::Global {
                index,
                wasm_type: _,
            } => {
                // Direct WASM global access (1 instruction)
                let wasm_global = local_map.get_global(*index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "WIR global {} not mapped to WASM global. Check global variable setup.",
                        index
                    ))
                })?;

                function.instruction(&Instruction::GlobalGet(wasm_global));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: global.get {} (WIR global {} → WASM global {})",
                    wasm_global, index, wasm_global
                );

                Ok(())
            }

            Place::Memory { base, offset, size } => {
                // WASM linear memory access (≤3 instructions: base + offset + load)
                self.lower_memory_base_address(base, function)?;

                // Add offset if non-zero (1 instruction)
                Self::emit_memory_offset(function, offset.0);

                // Generate type-appropriate load instruction (1 instruction)
                self.generate_memory_load_instruction(size, function)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: memory load at offset {} with size {:?}",
                    offset.0, size
                );

                Ok(())
            }

            Place::Projection { base, elem } => {
                // Projection access: base + element calculation (≤3 instructions total)
                self.lower_place_access(base, function, local_map)?;
                self.lower_projection_element_access(elem, function, local_map)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: projection access - base: {:?}, elem: {:?}",
                    base, elem
                );

                Ok(())
            }
        }
    }

    /// Generate memory load instruction based on type size
    fn generate_memory_load_instruction(
        &self,
        size: &TypeSize,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match size {
            TypeSize::Byte => {
                function.instruction(&Instruction::I32Load8U(MemArg {
                    offset: 0,
                    align: 0, // 1-byte alignment
                    memory_index: 0,
                }));
            }
            TypeSize::Short => {
                function.instruction(&Instruction::I32Load16U(MemArg {
                    offset: 0,
                    align: 1, // 2-byte alignment
                    memory_index: 0,
                }));
            }
            TypeSize::Word => {
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2, // 4-byte alignment
                    memory_index: 0,
                }));
            }
            TypeSize::DoubleWord => {
                function.instruction(&Instruction::I64Load(MemArg {
                    offset: 0,
                    align: 3, // 8-byte alignment
                    memory_index: 0,
                }));
            }
            TypeSize::Custom { bytes, alignment } => {
                // For custom sizes, choose appropriate load instruction
                if *bytes <= 4 {
                    function.instruction(&Instruction::I32Load(MemArg {
                        offset: 0,
                        align: (*alignment as f32).log2() as u32,
                        memory_index: 0,
                    }));
                } else {
                    function.instruction(&Instruction::I64Load(MemArg {
                        offset: 0,
                        align: (*alignment as f32).log2() as u32,
                        memory_index: 0,
                    }));
                }
            }
        }
        Ok(())
    }

    /// Lower memory base address calculation
    fn lower_memory_base_address(
        &mut self,
        base: &MemoryBase,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match base {
            MemoryBase::LinearMemory => {
                // Linear memory starts at offset 0
                Self::emit_i32_const(function, 0);
                Ok(())
            }
            MemoryBase::Stack => {
                // Stack-based memory should be handled as locals, not memory operations
                return_compiler_error!(
                    "Stack-based memory access should use Place::Local, not Place::Memory"
                );
            }
            MemoryBase::Heap { alloc_id, size: _ } => {
                // Heap allocation - for now, use linear memory with allocation ID as offset
                // Future enhancement: proper heap management with allocation tracking
                Self::emit_i32_const(function, *alloc_id as i32 * 1024); // Simple allocation strategy
                Ok(())
            }
        }
    }

    /// Lower projection element access for field/index operations
    fn lower_projection_element_access(
        &mut self,
        elem: &ProjectionElem,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match elem {
            ProjectionElem::Field {
                index: _,
                offset,
                size,
            } => {
                // Field access: add field offset to base address
                if offset.0 > 0 {
                    function.instruction(&Instruction::I32Const(offset.0 as i32));
                    function.instruction(&Instruction::I32Add);
                }

                // Generate appropriate load instruction based on field size
                match size {
                    FieldSize::Fixed(1) => {
                        function.instruction(&Instruction::I32Load8U(MemArg {
                            offset: 0,
                            align: 0, // 1-byte alignment
                            memory_index: 0,
                        }));
                    }
                    FieldSize::Fixed(2) => {
                        function.instruction(&Instruction::I32Load16U(MemArg {
                            offset: 0,
                            align: 1, // 2-byte alignment
                            memory_index: 0,
                        }));
                    }
                    FieldSize::Fixed(4) => {
                        function.instruction(&Instruction::I32Load(MemArg {
                            offset: 0,
                            align: 2, // 4-byte alignment
                            memory_index: 0,
                        }));
                    }
                    FieldSize::Fixed(8) => {
                        function.instruction(&Instruction::I64Load(MemArg {
                            offset: 0,
                            align: 3, // 8-byte alignment
                            memory_index: 0,
                        }));
                    }
                    FieldSize::WasmType(wasm_type) => {
                        match wasm_type {
                            WasmType::I32 => {
                                function.instruction(&Instruction::I32Load(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::I64 => {
                                function.instruction(&Instruction::I64Load(MemArg {
                                    offset: 0,
                                    align: 3,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::F32 => {
                                function.instruction(&Instruction::F32Load(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::F64 => {
                                function.instruction(&Instruction::F64Load(MemArg {
                                    offset: 0,
                                    align: 3,
                                    memory_index: 0,
                                }));
                            }
                            WasmType::ExternRef | WasmType::FuncRef => {
                                // References are stored as i32 pointers
                                function.instruction(&Instruction::I32Load(MemArg {
                                    offset: 0,
                                    align: 2,
                                    memory_index: 0,
                                }));
                            }
                        }
                    }
                    FieldSize::Variable => {
                        // Variable size fields are typically pointers to the actual data
                        function.instruction(&Instruction::I32Load(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    FieldSize::Fixed(size) => {
                        // Handle other fixed sizes by defaulting to appropriate instruction
                        if *size <= 4 {
                            function.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        } else {
                            function.instruction(&Instruction::I64Load(MemArg {
                                offset: 0,
                                align: 3,
                                memory_index: 0,
                            }));
                        }
                    }
                }
                Ok(())
            }
            ProjectionElem::Index {
                index,
                element_size,
            } => {
                // Array indexing: base + (index * element_size)
                self.emit_array_index_calculation(function, index, *element_size, local_map)?;
                Ok(())
            }
            ProjectionElem::Length => {
                // Length field access - typically at offset 0 for arrays/strings
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
            ProjectionElem::Data => {
                // Data pointer access - typically after length field
                Self::emit_memory_offset(function, 4); // Skip length field (4 bytes)
                Ok(())
            }
            ProjectionElem::Deref => {
                // Dereference: load value from address on stack
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
        }
    }

    /// Lower place assignment to WASM instructions (WIR-faithful place assignment)
    ///
    /// This method implements WIR place assignment that maps directly to WASM store operations:
    /// - Place::Local → local.set (1 instruction)
    /// - Place::Global → global.set (1 instruction)
    /// - Place::Memory → address calculation + memory.store (≤3 instructions)
    /// - Place::Projection → base + projection + store (≤3 instructions)
    fn lower_place_assignment_with_type(
        &mut self,
        place: &Place,
        value_type: &WasmType,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local {
                index,
                wasm_type: _,
            } => {
                // Direct WASM local assignment (1 instruction)
                let wasm_local = local_map.get_local(*index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "WIR local {} not mapped to WASM local for assignment",
                        index
                    ))
                })?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: About to generate local.set {} (WIR local {} → WASM local {}, value_type: {:?}, place_type: {:?})",
                    wasm_local,
                    index,
                    wasm_local,
                    value_type,
                    place.wasm_type()
                );

                function.instruction(&Instruction::LocalSet(wasm_local));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Generated local.set {} (WIR local {} → WASM local {})",
                    wasm_local, index, wasm_local
                );

                Ok(())
            }

            Place::Global {
                index,
                wasm_type: _,
            } => {
                // Direct WASM global assignment (1 instruction)
                let wasm_global = local_map.get_global(*index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "WIR global {} not mapped to WASM global for assignment",
                        index
                    ))
                })?;

                function.instruction(&Instruction::GlobalSet(wasm_global));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: global.set {} (WIR global {} → WASM global {})",
                    wasm_global, index, wasm_global
                );

                Ok(())
            }

            Place::Memory { base, offset, size } => {
                // WASM linear memory assignment (≤3 instructions: base + offset + store)
                self.lower_memory_base_address(base, function)?;

                // Add offset if non-zero (1 instruction)
                Self::emit_memory_offset(function, offset.0);

                // Generate type-appropriate store instruction (1 instruction)
                self.generate_memory_store_instruction(value_type, size, function)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: memory store at offset {} with type {:?} and size {:?}",
                    offset.0, value_type, size
                );

                Ok(())
            }

            Place::Projection { base, elem } => {
                // Projection assignment: calculate final address then store
                self.lower_place_access(base, function, local_map)?;
                self.lower_projection_element_access(elem, function, local_map)?;

                // Generate store instruction based on projection element type
                let elem_type = elem.wasm_type();
                self.generate_memory_store_instruction(&elem_type, &TypeSize::Word, function)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: projection assignment - base: {:?}, elem: {:?}",
                    base, elem
                );

                Ok(())
            }
        }
    }

    /// Generate memory store instruction based on value type and size
    fn generate_memory_store_instruction(
        &self,
        value_type: &WasmType,
        size: &TypeSize,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match (value_type, size) {
            // Float stores
            (WasmType::F32, TypeSize::Word) | (WasmType::F32, TypeSize::Custom { .. }) => {
                function.instruction(&Instruction::F32Store(MemArg {
                    offset: 0,
                    align: 2, // 4-byte alignment
                    memory_index: 0,
                }));
            }
            (WasmType::F64, TypeSize::DoubleWord) | (WasmType::F64, TypeSize::Custom { .. }) => {
                function.instruction(&Instruction::F64Store(MemArg {
                    offset: 0,
                    align: 3, // 8-byte alignment
                    memory_index: 0,
                }));
            }
            // Integer stores
            (WasmType::I32, TypeSize::Byte) => {
                function.instruction(&Instruction::I32Store8(MemArg {
                    offset: 0,
                    align: 0, // 1-byte alignment
                    memory_index: 0,
                }));
            }
            (WasmType::I32, TypeSize::Short) => {
                function.instruction(&Instruction::I32Store16(MemArg {
                    offset: 0,
                    align: 1, // 2-byte alignment
                    memory_index: 0,
                }));
            }
            (WasmType::I32, TypeSize::Word) => {
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2, // 4-byte alignment
                    memory_index: 0,
                }));
            }
            (WasmType::I64, TypeSize::DoubleWord) => {
                function.instruction(&Instruction::I64Store(MemArg {
                    offset: 0,
                    align: 3, // 8-byte alignment
                    memory_index: 0,
                }));
            }
            // Custom sizes with type-based instruction selection
            (WasmType::I32, TypeSize::Custom { bytes, alignment }) => {
                if *bytes <= 4 {
                    function.instruction(&Instruction::I32Store(MemArg {
                        offset: 0,
                        align: (*alignment as f32).log2() as u32,
                        memory_index: 0,
                    }));
                } else {
                    return_compiler_error!(
                        "I32 value cannot be stored in {} bytes. Use I64 for larger values.",
                        bytes
                    );
                }
            }
            (WasmType::I64, TypeSize::Custom { alignment, .. }) => {
                function.instruction(&Instruction::I64Store(MemArg {
                    offset: 0,
                    align: (*alignment as f32).log2() as u32,
                    memory_index: 0,
                }));
            }
            // Reference types (stored as i32 pointers)
            (WasmType::ExternRef | WasmType::FuncRef, _) => {
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2, // 4-byte alignment for pointers
                    memory_index: 0,
                }));
            }
            // Unsupported combinations
            (value_type, size) => {
                return_compiler_error!(
                    "Unsupported store combination: {:?} value to {:?} size. Check WIR type consistency.",
                    value_type,
                    size
                );
            }
        }
        Ok(())
    }

    /// Get the WASM type of an rvalue for type-aware instruction generation
    fn get_rvalue_wasm_type(&self, rvalue: &Rvalue) -> Result<WasmType, CompileError> {
        match rvalue {
            Rvalue::Use(operand) => self.get_operand_wasm_type(operand),
            Rvalue::BinaryOp(_, left, _) => {
                // Binary operations preserve the type of their operands
                self.get_operand_wasm_type(left)
            }
            Rvalue::UnaryOp(_, operand) => {
                // Unary operations preserve the type of their operand
                self.get_operand_wasm_type(operand)
            }
            Rvalue::Ref { .. } => {
                // References are pointers (i32)
                Ok(WasmType::I32)
            }
            Rvalue::StringConcat(_, _) => {
                // String concatenation results in a string pointer (i32)
                Ok(WasmType::I32)
            }
        }
    }

    /// Get the WASM type of an operand for type-aware instruction generation
    fn get_operand_wasm_type(&self, operand: &Operand) -> Result<WasmType, CompileError> {
        match operand {
            Operand::Constant(constant) => {
                match constant {
                    Constant::I32(_) => Ok(WasmType::I32),
                    Constant::I64(_) => Ok(WasmType::I64),
                    Constant::F32(_) => Ok(WasmType::F32),
                    Constant::F64(_) => Ok(WasmType::F64),
                    Constant::Bool(_) => Ok(WasmType::I32), // Booleans are i32 in WASM
                    Constant::String(_) => Ok(WasmType::I32), // String pointers are i32
                    _ => return_compiler_error!(
                        "Unsupported constant type for WASM type determination: {:?}",
                        constant
                    ),
                }
            }
            Operand::Copy(place) | Operand::Move(place) => Ok(place.wasm_type()),
            Operand::FunctionRef(_) => Ok(WasmType::I32), // Function references are i32 indices
            Operand::GlobalRef(_) => Ok(WasmType::I32),   // Global references are i32 indices
        }
    }

    /// Lower binary operations to WASM instructions with type awareness
    fn lower_binary_op(
        &self,
        op: &BinOp,
        wasm_type: &WasmType,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match (op, wasm_type) {
            // Integer arithmetic operations
            (BinOp::Add, WasmType::I32) => {
                function.instruction(&Instruction::I32Add);
                Ok(())
            }
            (BinOp::Add, WasmType::I64) => {
                function.instruction(&Instruction::I64Add);
                Ok(())
            }
            (BinOp::Sub, WasmType::I32) => {
                function.instruction(&Instruction::I32Sub);
                Ok(())
            }
            (BinOp::Sub, WasmType::I64) => {
                function.instruction(&Instruction::I64Sub);
                Ok(())
            }
            (BinOp::Mul, WasmType::I32) => {
                function.instruction(&Instruction::I32Mul);
                Ok(())
            }
            (BinOp::Mul, WasmType::I64) => {
                function.instruction(&Instruction::I64Mul);
                Ok(())
            }
            (BinOp::Div, WasmType::I32) => {
                function.instruction(&Instruction::I32DivS); // Signed division
                Ok(())
            }
            (BinOp::Div, WasmType::I64) => {
                function.instruction(&Instruction::I64DivS); // Signed division
                Ok(())
            }
            (BinOp::Rem, WasmType::I32) => {
                function.instruction(&Instruction::I32RemS); // Signed remainder
                Ok(())
            }
            (BinOp::Rem, WasmType::I64) => {
                function.instruction(&Instruction::I64RemS); // Signed remainder
                Ok(())
            }

            // Float arithmetic operations
            (BinOp::Add, WasmType::F32) => {
                function.instruction(&Instruction::F32Add);
                Ok(())
            }
            (BinOp::Add, WasmType::F64) => {
                function.instruction(&Instruction::F64Add);
                Ok(())
            }
            (BinOp::Sub, WasmType::F32) => {
                function.instruction(&Instruction::F32Sub);
                Ok(())
            }
            (BinOp::Sub, WasmType::F64) => {
                function.instruction(&Instruction::F64Sub);
                Ok(())
            }
            (BinOp::Mul, WasmType::F32) => {
                function.instruction(&Instruction::F32Mul);
                Ok(())
            }
            (BinOp::Mul, WasmType::F64) => {
                function.instruction(&Instruction::F64Mul);
                Ok(())
            }
            (BinOp::Div, WasmType::F32) => {
                function.instruction(&Instruction::F32Div);
                Ok(())
            }
            (BinOp::Div, WasmType::F64) => {
                function.instruction(&Instruction::F64Div);
                Ok(())
            }

            // Comparison operations (return i32)
            (BinOp::Eq, WasmType::I32) => {
                function.instruction(&Instruction::I32Eq);
                Ok(())
            }
            (BinOp::Eq, WasmType::I64) => {
                function.instruction(&Instruction::I64Eq);
                Ok(())
            }
            (BinOp::Eq, WasmType::F32) => {
                function.instruction(&Instruction::F32Eq);
                Ok(())
            }
            (BinOp::Eq, WasmType::F64) => {
                function.instruction(&Instruction::F64Eq);
                Ok(())
            }
            (BinOp::Ne, WasmType::I32) => {
                function.instruction(&Instruction::I32Ne);
                Ok(())
            }
            (BinOp::Ne, WasmType::I64) => {
                function.instruction(&Instruction::I64Ne);
                Ok(())
            }
            (BinOp::Ne, WasmType::F32) => {
                function.instruction(&Instruction::F32Ne);
                Ok(())
            }
            (BinOp::Ne, WasmType::F64) => {
                function.instruction(&Instruction::F64Ne);
                Ok(())
            }
            (BinOp::Lt, WasmType::I32) => {
                function.instruction(&Instruction::I32LtS); // Signed less than
                Ok(())
            }
            (BinOp::Lt, WasmType::I64) => {
                function.instruction(&Instruction::I64LtS); // Signed less than
                Ok(())
            }
            (BinOp::Lt, WasmType::F32) => {
                function.instruction(&Instruction::F32Lt);
                Ok(())
            }
            (BinOp::Lt, WasmType::F64) => {
                function.instruction(&Instruction::F64Lt);
                Ok(())
            }
            (BinOp::Le, WasmType::I32) => {
                function.instruction(&Instruction::I32LeS); // Signed less than or equal
                Ok(())
            }
            (BinOp::Le, WasmType::I64) => {
                function.instruction(&Instruction::I64LeS); // Signed less than or equal
                Ok(())
            }
            (BinOp::Le, WasmType::F32) => {
                function.instruction(&Instruction::F32Le);
                Ok(())
            }
            (BinOp::Le, WasmType::F64) => {
                function.instruction(&Instruction::F64Le);
                Ok(())
            }
            (BinOp::Gt, WasmType::I32) => {
                function.instruction(&Instruction::I32GtS); // Signed greater than
                Ok(())
            }
            (BinOp::Gt, WasmType::I64) => {
                function.instruction(&Instruction::I64GtS); // Signed greater than
                Ok(())
            }
            (BinOp::Gt, WasmType::F32) => {
                function.instruction(&Instruction::F32Gt);
                Ok(())
            }
            (BinOp::Gt, WasmType::F64) => {
                function.instruction(&Instruction::F64Gt);
                Ok(())
            }
            (BinOp::Ge, WasmType::I32) => {
                function.instruction(&Instruction::I32GeS); // Signed greater than or equal
                Ok(())
            }
            (BinOp::Ge, WasmType::I64) => {
                function.instruction(&Instruction::I64GeS); // Signed greater than or equal
                Ok(())
            }
            (BinOp::Ge, WasmType::F32) => {
                function.instruction(&Instruction::F32Ge);
                Ok(())
            }
            (BinOp::Ge, WasmType::F64) => {
                function.instruction(&Instruction::F64Ge);
                Ok(())
            }

            // Bitwise operations (integers only)
            (BinOp::BitAnd, WasmType::I32) => {
                function.instruction(&Instruction::I32And);
                Ok(())
            }
            (BinOp::BitAnd, WasmType::I64) => {
                function.instruction(&Instruction::I64And);
                Ok(())
            }
            (BinOp::BitOr, WasmType::I32) => {
                function.instruction(&Instruction::I32Or);
                Ok(())
            }
            (BinOp::BitOr, WasmType::I64) => {
                function.instruction(&Instruction::I64Or);
                Ok(())
            }
            (BinOp::BitXor, WasmType::I32) => {
                function.instruction(&Instruction::I32Xor);
                Ok(())
            }
            (BinOp::BitXor, WasmType::I64) => {
                function.instruction(&Instruction::I64Xor);
                Ok(())
            }
            (BinOp::Shl, WasmType::I32) => {
                function.instruction(&Instruction::I32Shl);
                Ok(())
            }
            (BinOp::Shl, WasmType::I64) => {
                function.instruction(&Instruction::I64Shl);
                Ok(())
            }
            (BinOp::Shr, WasmType::I32) => {
                function.instruction(&Instruction::I32ShrS); // Signed right shift
                Ok(())
            }
            (BinOp::Shr, WasmType::I64) => {
                function.instruction(&Instruction::I64ShrS); // Signed right shift
                Ok(())
            }

            // Logical operations (implemented as short-circuiting control flow)
            (BinOp::And, _) | (BinOp::Or, _) => {
                return_compiler_error!(
                    "Logical operations (and/or) should be implemented as control flow, not binary operations. Use if/else statements for short-circuiting behavior."
                );
            }

            // Unsupported combinations
            (op, wasm_type) => {
                return_compiler_error!(
                    "Binary operation {:?} not supported for WASM type {:?}. Check that the operation is valid for the given type.",
                    op,
                    wasm_type
                );
            }
        }
    }

    /// Lower unary operations to WASM instructions
    fn lower_unary_op(&self, op: &UnOp, function: &mut Function) -> Result<(), CompileError> {
        match op {
            UnOp::Neg => {
                // Negate by subtracting from 0
                function.instruction(&Instruction::I32Const(0));
                function.instruction(&Instruction::I32Sub);
                Ok(())
            }
            _ => {
                return_compiler_error!(
                    "Unary operation not yet implemented in simplified WASM backend: {:?}",
                    op
                );
            }
        }
    }

    /// Lower if terminator to WASM structured control flow
    ///
    /// This method implements WASM structured control flow for if/else statements.
    /// It generates proper WASM if/else/end instruction sequences with correct
    /// block types and stack discipline.

    /// Lower a WIR terminator to WASM control flow instructions with enhanced validation
    fn lower_terminator_enhanced(
        &mut self,
        terminator: &Terminator,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::Goto { target } => {
                // Beanstalk scope semantics: goto represents control flow between scopes
                self.lower_beanstalk_goto_enhanced(*target, function_builder, local_map)
            }
            Terminator::If {
                condition,
                then_block,
                else_block,
            } => {
                // Beanstalk conditional with scope semantics (: and ;)
                self.lower_beanstalk_if_terminator_enhanced(
                    condition,
                    *then_block,
                    *else_block,
                    function_builder,
                    local_map,
                )
            }
            Terminator::Return { values } => {
                // Beanstalk return with proper scope closing
                self.lower_beanstalk_return_enhanced(values, function_builder, local_map)
            }
            Terminator::Unreachable => {
                function_builder.instruction(&Instruction::Unreachable)?;
                Ok(())
            }
        }
    }

    /// Legacy method for compatibility - delegates to enhanced version
    pub fn lower_terminator(
        &mut self,
        terminator: &Terminator,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Create a temporary enhanced builder for legacy compatibility
        let mut temp_builder = EnhancedFunctionBuilder::new(
            Vec::new(),
            Vec::new(),
            Vec::new(),
            "legacy_function".to_string(),
        );

        // Lower using enhanced method
        self.lower_terminator_enhanced(terminator, &mut temp_builder, local_map)?;

        // Extract instructions from temp builder (this is a simplified approach)
        // In practice, the legacy method should be phased out
        match terminator {
            Terminator::Goto { .. } => {
                function.instruction(&Instruction::Nop); // Placeholder
            }
            Terminator::If { condition, .. } => {
                self.lower_operand(condition, function, local_map)?;
                function.instruction(&Instruction::If(BlockType::Empty));
                function.instruction(&Instruction::Nop); // Then block placeholder
                function.instruction(&Instruction::Else);
                function.instruction(&Instruction::Nop); // Else block placeholder
                function.instruction(&Instruction::End);
            }
            Terminator::Return { values } => {
                for value in values {
                    self.lower_operand(value, function, local_map)?;
                }
                function.instruction(&Instruction::Return);
            }
            Terminator::Unreachable => {
                function.instruction(&Instruction::Unreachable);
            }
        }

        Ok(())
    }

    /// Ensure function has proper termination
    ///
    /// WASM functions must end with a terminating instruction (return, unreachable, etc.)
    /// This method ensures every function has proper termination by adding a return instruction.
    /// For void functions, this adds an empty return. For functions with return types,
    /// this provides default values and return.
    fn ensure_function_termination(
        &self,
        function: &mut Function,
        result_types: &[ValType],
    ) -> Result<(), CompileError> {
        // Always add proper termination to ensure WASM validation passes
        // This is essential for fixing the "control frames remain" error

        if result_types.is_empty() {
            // For void functions, add an explicit return instruction
            // This ensures the function properly terminates
            function.instruction(&Instruction::Return);
        } else {
            // For functions with return types, provide default values
            // This handles cases where WIR doesn't have explicit returns
            for result_type in result_types {
                match result_type {
                    ValType::I32 => {
                        function.instruction(&Instruction::I32Const(0));
                    }
                    ValType::I64 => {
                        function.instruction(&Instruction::I64Const(0));
                    }
                    ValType::F32 => {
                        function.instruction(&Instruction::F32Const(0.0.into()));
                    }
                    ValType::F64 => {
                        function.instruction(&Instruction::F64Const(0.0.into()));
                    }
                    _ => {
                        return_compiler_error!(
                            "Unsupported return type for automatic function termination: {:?}",
                            result_type
                        );
                    }
                }
            }
            function.instruction(&Instruction::Return);
        }
        Ok(())
    }

    /// Generate string constant WASM instructions
    ///
    /// Returns an i32.const instruction with the offset to the string data in linear memory
    fn generate_string_constant(&mut self, value: &str) -> Result<(), CompileError> {
        let _offset = self.string_manager.add_string_constant(value);
        // Return pointer to string data in linear memory as i32
        Ok(())
    }

    /// Convert WasmType to wasm_encoder ValType - uses unified conversion
    fn wasm_type_to_val_type(&self, wasm_type: &WasmType) -> ValType {
        Self::unified_wasm_type_to_val_type(wasm_type)
    }

    /// Compile a WIR function (alias for compile_function)
    pub fn compile_wir_function(
        &mut self,
        wir_function: &WirFunction,
    ) -> Result<u32, CompileError> {
        let function_index = self.function_count;
        self.compile_function(wir_function)?;
        Ok(function_index)
    }

    /// Add function export to the WASM module
    pub fn add_function_export(
        &mut self,
        name: &str,
        function_index: u32,
    ) -> Result<u32, CompileError> {
        // Check if this export name has already been added
        if self.exported_names.contains(name) {
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WASM: Skipping duplicate export '{}' at index {}",
                name, function_index
            );
            return Ok(function_index);
        }
        
        // Add export entry to export section
        self.export_section
            .export(name, ExportKind::Func, function_index);
        
        // Track this export name
        self.exported_names.insert(name.to_string());

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: Exported function '{}' at index {}",
            name, function_index
        );

        Ok(function_index)
    }

    /// Add global export to the WASM module
    pub fn add_global_export(&mut self, name: &str, global_index: u32) -> Result<(), CompileError> {
        // Add export entry to export section
        self.export_section
            .export(name, ExportKind::Global, global_index);

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Exported global '{}' at index {}", name, global_index);

        Ok(())
    }

    /// Add memory export to the WASM module
    pub fn add_memory_export(&mut self, name: &str) -> Result<(), CompileError> {
        // Add export entry to export section (memory index is always 0 for single memory)
        self.export_section.export(name, ExportKind::Memory, 0);

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Exported memory '{}'", name);

        Ok(())
    }

    /// Generate WASM import section entries for host functions used in WIR
    ///
    /// This method creates WASM function type signatures from host function definitions
    /// and adds import entries with correct module and function names.
    pub fn encode_host_function_imports(
        &mut self,
        host_imports: &HashSet<HostFunctionDef>,
    ) -> Result<(), CompileError> {
        self.encode_host_function_imports_with_registry(host_imports, None)
    }

    /// Generate WASM import section entries for host functions with registry-aware mapping
    ///
    /// This method creates WASM function type signatures from host function definitions
    /// and adds import entries with correct module and function names, using runtime-specific
    /// mappings from the host function registry when available.
    pub fn encode_host_function_imports_with_registry(
        &mut self,
        host_imports: &HashSet<HostFunctionDef>,
        registry: Option<&crate::compiler::host_functions::registry::HostFunctionRegistry>,
    ) -> Result<(), CompileError> {
        for host_func in host_imports {
            // Check for runtime-specific mapping if registry is available
            let (module_name, import_name) = if let Some(reg) = registry {
                self.get_runtime_specific_mapping(host_func, reg)?
            } else {
                (host_func.module.clone(), host_func.import_name.clone())
            };

            // Create WASM function type signature from host function definition
            let param_types =
                self.create_wasm_param_types(&host_func.params_to_signature().parameters)?;
            let result_types = self.create_wasm_result_types(&host_func.return_types)?;

            // Add function type to type section
            self.type_section.ty().function(param_types, result_types);

            // Add import entry to import section with runtime-specific mapping
            self.import_section.import(
                &module_name,
                &import_name,
                EntityType::Function(self.type_count),
            );

            // Register function index for calls - host functions come before regular functions
            self.host_function_indices
                .insert(host_func.name.clone(), self.function_count);

            // Also register in the main function registry for unified lookup
            self.function_registry
                .insert(host_func.name.clone(), self.function_count);

            // Increment counters
            self.function_count += 1;
            self.type_count += 1;
        }

        Ok(())
    }

    /// Get runtime-specific mapping for a host function
    fn get_runtime_specific_mapping(
        &self,
        host_func: &HostFunctionDef,
        registry: &crate::compiler::host_functions::registry::HostFunctionRegistry,
    ) -> Result<(String, String), CompileError> {
        use crate::compiler::host_functions::registry::{RuntimeBackend, RuntimeFunctionMapping};

        // Get the runtime mapping based on current backend
        match registry.get_runtime_mapping(&host_func.name) {
            Some(RuntimeFunctionMapping::Wasix(wasix_func)) => {
                // Use WASIX mapping for native execution
                Ok((wasix_func.module.clone(), wasix_func.name.clone()))
            }
            Some(RuntimeFunctionMapping::JavaScript(js_func)) => {
                // Use JavaScript mapping for web execution
                Ok((js_func.module.clone(), js_func.name.clone()))
            }
            Some(RuntimeFunctionMapping::Native(_)) | None => {
                // Use original host function mapping as fallback
                Ok((host_func.module.clone(), host_func.import_name.clone()))
            }
        }
    }

    /// Create WASM parameter types from host function parameters
    fn create_wasm_param_types(
        &self,
        parameters: &[crate::compiler::parsers::ast_nodes::Arg],
    ) -> Result<Vec<ValType>, CompileError> {
        let mut param_types = Vec::new();

        for param in parameters {
            // String and Template types need two parameters: pointer and length
            match &param.value.data_type {
                DataType::String | DataType::Template => {
                    param_types.push(ValType::I32); // pointer
                    param_types.push(ValType::I32); // length
                }
                _ => {
                    let wasm_type = Self::unified_datatype_to_wasm_type(&param.value.data_type)?;
                    param_types.push(self.wasm_type_to_val_type(&wasm_type));
                }
            }
        }

        Ok(param_types)
    }

    /// Create WASM result types from host function return types
    fn create_wasm_result_types(
        &self,
        return_types: &[crate::compiler::datatypes::DataType],
    ) -> Result<Vec<ValType>, CompileError> {
        let mut result_types = Vec::new();

        for return_type in return_types {
            let wasm_type = Self::unified_datatype_to_wasm_type(return_type)?;
            result_types.push(self.wasm_type_to_val_type(&wasm_type));
        }

        Ok(result_types)
    }

    /// Get the function index for a host function by name
    pub fn get_host_function_index(&self, name: &str) -> Option<u32> {
        self.host_function_indices.get(name).copied()
    }

    /// Generate WASIX import section entries for functions that need WASIX support
    ///
    /// This method adds WASIX function imports (like fd_write) to the WASM module
    /// based on the functions used in the Beanstalk program. It supports both
    /// native function implementations and import-based function resolution.
    pub fn add_wasix_imports(&mut self) -> Result<(), CompileError> {
        // Validate WASIX registry before processing imports
        self.validate_wasix_registry()?;

        // Collect information about WASIX functions to import
        let mut wasix_imports = Vec::new();

        for (beanstalk_name, wasix_func) in self.wasix_registry.list_functions() {
            // Validate each WASIX function definition
            self.validate_wasix_function_def(beanstalk_name, wasix_func)?;

            wasix_imports.push((
                beanstalk_name.clone(),
                wasix_func.module.clone(),
                wasix_func.name.clone(),
                wasix_func.parameters.clone(),
                wasix_func.returns.clone(),
            ));
        }

        if wasix_imports.is_empty() {
            #[cfg(feature = "verbose_codegen_logging")]
            println!("WASM: No WASIX imports needed for this module");
            return Ok(());
        }

        // Process each WASIX import with comprehensive error handling
        for (beanstalk_name, module, name, parameters, returns) in &wasix_imports {
            // Validate import parameters
            self.validate_wasix_import_params(beanstalk_name, module, name, parameters, returns)?;

            // Check if this function has a native implementation available
            let has_native_impl = self
                .wasix_registry
                .get_function(&beanstalk_name)
                .map(|func| func.has_native_impl())
                .unwrap_or(false);

            if has_native_impl {
                // For native implementations, we still need to add the import for fallback
                // but mark it as having native support available
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: WASIX function '{}' has native implementation, adding import as fallback",
                    beanstalk_name
                );
            }

            // Add function type to type section with validation
            if parameters.len() > 20 || returns.len() > 5 {
                return_compiler_error!(
                    "WASIX function '{}' has excessive parameters ({}) or returns ({}). This may indicate a configuration error.",
                    beanstalk_name,
                    parameters.len(),
                    returns.len()
                );
            }

            self.type_section
                .ty()
                .function(parameters.clone(), returns.clone());

            // Add import entry to import section from WASIX module
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WASM: Adding import: module='{}', name='{}', function_index={}",
                module, name, self.function_count
            );

            self.import_section.import(
                &module,
                &name,
                EntityType::Function(self.type_count),
            );

            // Update the WASIX registry with the function index
            if let Some(mut_wasix_func) = self.wasix_registry.get_function_mut(&beanstalk_name) {
                mut_wasix_func.set_function_index(self.function_count);
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Assigned function index {} to WASIX function '{}'",
                    self.function_count, beanstalk_name
                );
            }

            // Register function index for calls - WASIX functions come after host functions
            self.host_function_indices
                .insert(beanstalk_name.clone(), self.function_count);

            // Also register in the main function registry for unified lookup
            self.function_registry
                .insert(beanstalk_name.clone(), self.function_count);

            // Increment counters
            self.function_count += 1;
            self.type_count += 1;
        }

        Ok(())
    }

    /// Validate the WASIX registry configuration
    fn validate_wasix_registry(&self) -> Result<(), CompileError> {
        let function_count = self.wasix_registry.count();

        if function_count == 0 {
            // This is not an error - just means no WASIX functions are used
            return Ok(());
        }

        if function_count > 100 {
            return_compiler_error!(
                "WASIX registry contains {} functions, which exceeds reasonable limit of 100. This may indicate a configuration error.",
                function_count
            );
        }

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: WASIX registry validated with {} functions",
            function_count
        );

        Ok(())
    }

    /// Validate a single WASIX function definition
    fn validate_wasix_function_def(
        &self,
        beanstalk_name: &str,
        wasix_func: &WasixFunctionDef,
    ) -> Result<(), CompileError> {
        // Validate function names
        if beanstalk_name.is_empty() {
            return_compiler_error!("WASIX function has empty Beanstalk name");
        }

        if wasix_func.name.is_empty() {
            return_compiler_error!("WASIX function '{}' has empty WASIX name", beanstalk_name);
        }

        if wasix_func.module.is_empty() {
            return_compiler_error!("WASIX function '{}' has empty module name", beanstalk_name);
        }

        // Validate module name follows WASIX conventions
        if !is_valid_wasix_module(&wasix_func.module) {
            return_compiler_error!(
                "WASIX function '{}' uses invalid module '{}'. Valid WASIX modules: wasix_32v1, wasix_64v1, wasix_snapshot_preview1, wasi_snapshot_preview1",
                beanstalk_name,
                wasix_func.module
            );
        }

        Ok(())
    }

    /// Validate WASIX import parameters
    fn validate_wasix_import_params(
        &self,
        beanstalk_name: &str,
        _module: &str,
        name: &str,
        parameters: &[ValType],
        returns: &[ValType],
    ) -> Result<(), CompileError> {
        // Validate parameter types are supported
        for (i, param_type) in parameters.iter().enumerate() {
            if !is_supported_wasix_type(param_type) {
                return_compiler_error!(
                    "WASIX function '{}' parameter {} has unsupported type {:?}. WASIX supports i32, i64, f32, f64",
                    beanstalk_name,
                    i,
                    param_type
                );
            }
        }

        // Validate return types are supported
        for (i, return_type) in returns.iter().enumerate() {
            if !is_supported_wasix_type(return_type) {
                return_compiler_error!(
                    "WASIX function '{}' return value {} has unsupported type {:?}. WASIX supports i32, i64, f32, f64",
                    beanstalk_name,
                    i,
                    return_type
                );
            }
        }

        // Validate specific WASIX function signatures
        match name {
            "fd_write" => {
                if parameters.len() != 4 || returns.len() != 1 {
                    return_compiler_error!(
                        "WASIX fd_write function '{}' has incorrect signature: expected (i32, i32, i32, i32) -> i32, got ({:?}) -> {:?}",
                        beanstalk_name,
                        parameters
                            .iter()
                            .map(|t| format!("{:?}", t))
                            .collect::<Vec<_>>()
                            .join(", "),
                        returns
                            .iter()
                            .map(|t| format!("{:?}", t))
                            .collect::<Vec<_>>()
                            .join(", ")
                    );
                }

                // Validate fd_write parameter types
                let expected_params = [ValType::I32, ValType::I32, ValType::I32, ValType::I32];
                let expected_returns = [ValType::I32];

                if parameters != expected_params || returns != expected_returns {
                    return_compiler_error!(
                        "WASIX fd_write function '{}' has incorrect types: expected (i32, i32, i32, i32) -> i32",
                        beanstalk_name
                    );
                }
            }
            _ => {
                // For other WASIX functions, just validate they have reasonable signatures
                if parameters.len() > 10 {
                    return_compiler_error!(
                        "WASIX function '{}' has {} parameters, which exceeds reasonable limit of 10",
                        beanstalk_name,
                        parameters.len()
                    );
                }

                if returns.len() > 2 {
                    return_compiler_error!(
                        "WASIX function '{}' has {} return values, which exceeds reasonable limit of 2",
                        beanstalk_name,
                        returns.len()
                    );
                }
            }
        }

        Ok(())
    }

    /// Get the function index for a regular function by name
    pub fn get_function_index(&self, name: &str) -> Option<u32> {
        self.function_registry.get(name).copied()
    }

    /// Get the total number of functions (host + regular)
    pub fn get_total_function_count(&self) -> u32 {
        self.function_count
    }

    /// Get the number of host function imports
    pub fn get_host_function_count(&self) -> usize {
        self.host_function_indices.len()
    }

    /// Convert Beanstalk DataType to WasmType for host function signatures - uses unified conversion
    fn datatype_to_wasm_type(
        &self,
        data_type: &crate::compiler::datatypes::DataType,
    ) -> Result<WasmType, CompileError> {
        use crate::compiler::datatypes::DataType;

        // Use unified conversion for most types
        match data_type {
            DataType::None => {
                // None types don't contribute to WASM signature
                return_compiler_error!(
                    "None type should not appear in host function signatures. This indicates a problem with host function definition."
                );
            }
            DataType::Int | DataType::Float | DataType::Bool | DataType::String => {
                Self::unified_datatype_to_wasm_type(data_type)
            }
            _ => {
                return_unimplemented_feature_error!(
                    &format!("DataType '{:?}' in host function signatures", data_type),
                    None,
                    Some(
                        "use basic types (Int, Float, Bool, String) for host function parameters and return values"
                    )
                );
            }
        }
    }

    /// Lower Beanstalk reference semantics to WASM operations
    ///
    /// Implements Beanstalk's implicit borrowing semantics:
    /// - BorrowKind::Shared: Multiple shared references allowed (read-only access)
    /// - BorrowKind::Mut: Exclusive mutable access (read-write access)
    ///
    /// In WASM, both shared and mutable references are implemented as direct value access
    /// since WASM doesn't have explicit reference types for local variables.
    /// The borrow checking is handled at the WIR level, and WASM generation assumes
    /// the borrowing rules have been validated.
    fn lower_beanstalk_reference(
        &mut self,
        place: &Place,
        borrow_kind: &crate::compiler::wir::wir_nodes::BorrowKind,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        use crate::compiler::wir::wir_nodes::BorrowKind;

        match borrow_kind {
            BorrowKind::Shared => {
                // Shared reference: read-only access to the place
                // In WASM, this is just a regular value load since WASM doesn't have
                // explicit shared reference types for locals/globals
                self.lower_place_access(place, function, local_map)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Beanstalk shared reference (implicit borrow) to place: {:?}",
                    place
                );

                Ok(())
            }
            BorrowKind::Mut => {
                // Mutable reference: exclusive access to the place
                // In WASM, this is also a regular value load, but the borrow checker
                // ensures exclusive access at the WIR level
                self.lower_place_access(place, function, local_map)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Beanstalk mutable reference (explicit ~) to place: {:?}",
                    place
                );

                Ok(())
            }
        }
    }

    /// Handle Beanstalk mutability syntax in WASM generation
    ///
    /// Processes Beanstalk's `~` syntax for mutable operations:
    /// - `x ~= y` creates mutable assignment with proper WASM local.set/global.set
    /// - Ensures mutable assignments generate correct WASM operations
    /// - Implements proper WASM memory access for mutable vs immutable data
    fn lower_beanstalk_mutable_assignment(
        &mut self,
        place: &Place,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Step 1: Evaluate the rvalue (what we're assigning)
        self.lower_rvalue(rvalue, function, local_map)?;

        // Step 2: Store to the mutable place with appropriate WASM instruction
        match place {
            Place::Local {
                index,
                wasm_type: _,
            } => {
                let wasm_local = local_map.get_local(*index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "Beanstalk mutable assignment: WIR local {} not mapped to WASM local",
                        index
                    ))
                })?;

                // Generate mutable local assignment
                function.instruction(&Instruction::LocalSet(wasm_local));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Beanstalk mutable assignment (~=) to local {} (WASM local {})",
                    index, wasm_local
                );

                Ok(())
            }
            Place::Global {
                index,
                wasm_type: _,
            } => {
                let wasm_global = local_map.get_global(*index).ok_or_else(|| {
                    CompileError::compiler_error(&format!(
                        "Beanstalk mutable assignment: WIR global {} not mapped to WASM global",
                        index
                    ))
                })?;

                // Generate mutable global assignment
                function.instruction(&Instruction::GlobalSet(wasm_global));

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Beanstalk mutable assignment (~=) to global {} (WASM global {})",
                    index, wasm_global
                );

                Ok(())
            }
            Place::Memory { base, offset, size } => {
                // For memory locations, we need to calculate the address and store
                self.lower_memory_base_address(base, function)?;

                // Add offset if non-zero
                if offset.0 > 0 {
                    function.instruction(&Instruction::I32Const(offset.0 as i32));
                    function.instruction(&Instruction::I32Add);
                }

                // Generate appropriate store instruction based on size
                // We need to determine the WASM type from the place
                let wasm_type = place.wasm_type();
                self.generate_memory_store_instruction(&wasm_type, size, function)?;

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Beanstalk mutable assignment (~=) to memory at offset {} with size {:?}",
                    offset.0, size
                );

                Ok(())
            }
            Place::Projection { base, elem } => {
                // Handle mutable assignment to projected places (struct fields, array elements)
                return_compiler_error!(
                    "Beanstalk mutable assignment to projections not yet implemented: {:?}.{:?}",
                    base,
                    elem
                );
            }
        }
    }

    /// Detect if an assignment involves Beanstalk mutability syntax
    ///
    /// This method analyzes the rvalue to determine if it represents a mutable operation
    /// that should use Beanstalk's `~` syntax semantics in WASM generation.
    fn is_beanstalk_mutable_assignment(&self, rvalue: &Rvalue) -> bool {
        match rvalue {
            Rvalue::Ref { borrow_kind, .. } => {
                // Mutable references indicate `~` syntax was used
                matches!(borrow_kind, BorrowKind::Mut)
            }
            Rvalue::Use(Operand::Move(_)) => {
                // Move operations indicate `~` syntax for ownership transfer
                true
            }
            _ => false,
        }
    }

    /// Enhanced assignment lowering with Beanstalk mutability awareness
    ///
    /// This method extends the basic assignment lowering to properly handle
    /// Beanstalk's `~` mutability syntax and generate appropriate WASM instructions.
    fn lower_beanstalk_aware_assignment(
        &mut self,
        place: &Place,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        if self.is_beanstalk_mutable_assignment(rvalue) {
            // Use specialized mutable assignment handling
            self.lower_beanstalk_mutable_assignment(place, rvalue, function, local_map)
        } else {
            // Use standard assignment handling
            self.lower_wir_assign(place, rvalue, function, local_map)
        }
    }

    /// Lower Beanstalk goto with proper scope semantics
    ///
    /// Beanstalk's goto represents control flow between scopes defined by `:` and `;`.
    /// In WASM, this maps to branch instructions with proper block structure.
    fn lower_beanstalk_goto(
        &mut self,
        _target: u32,
        _function: &mut Function,
        _local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // For single-block functions, goto at the end should fall through
        // For multi-block functions, generate proper branch instruction

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: Beanstalk goto to block {} (scope transition)",
            _target
        );

        // TODO: When multi-block support is added, generate proper br instruction
        // function.instruction(&Instruction::Br(target));

        Ok(())
    }

    /// Lower Beanstalk if terminator with scope semantics
    ///
    /// Beanstalk conditionals use `:` to open scope and `;` to close scope.
    /// This maps to WASM's structured control flow with proper block nesting.
    fn lower_beanstalk_if_terminator(
        &mut self,
        condition: &Operand,
        _then_block: u32,
        _else_block: u32,
        _function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Validate condition type (must be boolean/i32)
        let condition_type = self.get_operand_wasm_type(condition)?;
        if !matches!(condition_type, WasmType::I32) {
            return_compiler_error!(
                "Beanstalk if condition must be boolean (i32 in WASM), found {:?}",
                condition_type
            );
        }

        // Load condition onto stack
        self.lower_operand(condition, _function, local_map)?;

        // Generate Beanstalk-style structured control flow
        // `:` opens scope → WASM if instruction
        _function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: Beanstalk if scope opened (:) - then block {}",
            _then_block
        );

        // Then block content (placeholder for now)
        _function.instruction(&Instruction::Nop); // Placeholder for then block

        _function.instruction(&Instruction::Else);

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Beanstalk else scope - else block {}", _else_block);

        // Else block content (placeholder for now)
        _function.instruction(&Instruction::Nop); // Placeholder for else block

        // `;` closes scope → WASM end instruction
        _function.instruction(&Instruction::End);

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Beanstalk if scope closed (;)");

        Ok(())
    }

    /// Lower Beanstalk return with proper scope closing
    ///
    /// Beanstalk returns must properly close any open scopes before returning.
    /// This ensures proper WASM control flow and scope management.
    fn lower_beanstalk_return(
        &mut self,
        values: &[Operand],
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load return values onto the stack
        for value in values {
            self.lower_operand(value, function, local_map)?;
        }

        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: Beanstalk return with {} values (closing all scopes)",
            values.len()
        );

        // Generate return instruction (automatically closes all scopes in WASM)
        function.instruction(&Instruction::Return);

        Ok(())
    }

    /// Handle Beanstalk error handling syntax (!err:)
    ///
    /// Beanstalk's `!err:` syntax creates error handling scopes that map to
    /// WASM's structured exception handling or control flow patterns.
    fn lower_beanstalk_error_handling(
        &mut self,
        error_condition: &Operand,
        _error_handler_block: u32,
        _success_block: u32,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load error condition (typically a result or error flag)
        self.lower_operand(error_condition, function, local_map)?;

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Beanstalk error handling (!err:) - checking error condition");

        // Generate conditional branch based on error condition
        // If error condition is true (non-zero), branch to error handler
        function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));

        // Error handler block (!err: scope)
        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: Beanstalk error handler scope - block {}",
            _error_handler_block
        );

        function.instruction(&Instruction::Nop); // Placeholder for error handler

        function.instruction(&Instruction::Else);

        // Success block (normal execution path)
        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Beanstalk success path - block {}", _success_block);

        function.instruction(&Instruction::Nop); // Placeholder for success path

        // Close error handling scope
        function.instruction(&Instruction::End);

        #[cfg(feature = "verbose_codegen_logging")]
        println!("WASM: Beanstalk error handling scope closed");

        Ok(())
    }

    /// Generate the final WASM module bytes
    pub fn finish(mut self) -> Vec<u8> {
        // Populate data section with string constants
        if self.string_manager.get_data_size() > 0 {
            let string_data = self.string_manager.get_data_section();
            self.data_section.active(
                0,                        // Memory index 0
                &ConstExpr::i32_const(0), // Start at offset 0 in linear memory
                string_data.iter().copied(),
            );
        }

        let mut module = Module::new();

        module.section(&self.type_section);
        module.section(&self.import_section);
        module.section(&self.function_section);
        module.section(&self.memory_section);
        module.section(&self.global_section);
        module.section(&self.export_section);
        module.section(&self.code_section);
        module.section(&self.data_section);

        module.finish()
    }

    /// Validate and finish building the WASM module with comprehensive error handling
    pub fn finish_with_validation(self) -> Result<Vec<u8>, CompileError> {
        // Validate before generating final bytes
        self.validate_with_wasm_encoder()?;

        // Generate the final WASM bytes
        let wasm_bytes = self.finish();

        // Double-check validation on final bytes
        match wasmparser::validate(&wasm_bytes) {
            Ok(_) => {
                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Final module validation passed! Generated {} bytes",
                    wasm_bytes.len()
                );
                Ok(wasm_bytes)
            }
            Err(wasm_error) => {
                // Create a temporary module for error mapping since self was consumed
                let temp_module = WasmModule::new();
                temp_module.map_wasm_validation_error(&wasm_error)
            }
        }
    }

    /// Register a function name with its index
    pub fn register_function(&mut self, name: String, index: u32) {
        self.function_registry.insert(name, index);
    }

    /// Get all registered functions
    pub fn get_all_functions(&self) -> &HashMap<String, u32> {
        &self.function_registry
    }

    /// Provide actionable suggestions for common WASM validation errors
    pub fn get_error_suggestions(&self, error_message: &str) -> Vec<String> {
        let mut suggestions = Vec::new();

        if error_message.contains("control frames remain") {
            suggestions.push(
                "Check that every ':' (scope open) has a matching ';' (scope close)".to_string(),
            );
            suggestions.push(
                "Ensure all if statements have proper else clauses or end with ';'".to_string(),
            );
            suggestions.push(
                "Verify that function definitions end with proper return statements".to_string(),
            );
        }

        if error_message.contains("type mismatch") {
            suggestions.push("Check that variable assignments use compatible types".to_string());
            suggestions
                .push("Ensure function return types match the declared signature".to_string());
            suggestions.push("Verify that arithmetic operations use numeric types".to_string());
        }

        if error_message.contains("invalid function index") {
            suggestions
                .push("Check that all function calls use correct function names".to_string());
            suggestions.push("Ensure functions are defined before they are called".to_string());
            suggestions.push("Verify that imported functions are properly declared".to_string());
        }

        if error_message.contains("invalid local index") {
            suggestions.push("Check that all variables are declared before use".to_string());
            suggestions.push("Ensure variable names are spelled correctly".to_string());
            suggestions.push("Verify that variables are in the correct scope".to_string());
        }

        if suggestions.is_empty() {
            suggestions.push("Review your Beanstalk code for syntax errors".to_string());
            suggestions.push("Check the compiler documentation for language features".to_string());
            suggestions.push(
                "Consider reporting this as a compiler bug if the code looks correct".to_string(),
            );
        }

        suggestions
    }

    /// Generate a comprehensive error report with context and suggestions
    pub fn generate_error_report(&self, wasm_error: &wasmparser::BinaryReaderError) -> String {
        let source_context = self.find_source_context_for_wasm_error(wasm_error);
        let suggestions = self.get_error_suggestions(wasm_error.message());

        let mut report = String::new();

        // Error header
        report.push_str(&format!(
            "WASM Validation Error: {}\n",
            wasm_error.message()
        ));

        // Source context
        if let Some(ctx) = source_context {
            report.push_str(&format!(
                "Location: {} around line {}\n",
                ctx.context, ctx.line
            ));
            if let Some(ref file) = ctx.source_file {
                report.push_str(&format!("File: {}\n", file));
            }
            if !ctx.context.is_empty() {
                report.push_str(&format!("Context: {}\n", ctx.context));
            }
        }

        // Debug information
        report.push_str(&format!("WASM Offset: 0x{:x}\n", wasm_error.offset()));

        // Suggestions
        report.push_str("\nSuggestions:\n");
        for (i, suggestion) in suggestions.iter().enumerate() {
            report.push_str(&format!("  {}. {}\n", i + 1, suggestion));
        }

        report
    }

    // Enhanced helper methods for wasm_encoder integration

    /// Enhanced Beanstalk-aware assignment with validation
    fn lower_beanstalk_aware_assignment_enhanced(
        &mut self,
        place: &Place,
        rvalue: &Rvalue,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Lower rvalue to stack
        self.lower_rvalue_enhanced(rvalue, function_builder, local_map)?;

        // Assign to place
        let place_type = place.wasm_type();
        self.lower_place_assignment_with_type_enhanced(
            place,
            &place_type,
            function_builder,
            local_map,
        )?;

        Ok(())
    }

    /// Enhanced rvalue lowering with validation
    fn lower_rvalue_enhanced(
        &mut self,
        rvalue: &Rvalue,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match rvalue {
            Rvalue::Use(operand) => {
                self.lower_operand_enhanced(operand, function_builder, local_map)
            }
            Rvalue::BinaryOp(op, left, right) => {
                self.lower_operand_enhanced(left, function_builder, local_map)?;
                self.lower_operand_enhanced(right, function_builder, local_map)?;

                // Determine the correct WASM instruction based on operand types
                let left_type = self.get_operand_wasm_type_enhanced(left, local_map)?;
                let right_type = self.get_operand_wasm_type_enhanced(right, local_map)?;

                // For now, assume both operands have the same type (type checking should ensure this)
                let operand_type = if left_type == right_type {
                    left_type
                } else {
                    // Type mismatch - this should be caught by type checking
                    return_compiler_error!(
                        "Type mismatch in binary operation: left operand is {:?}, right operand is {:?}. Both operands must have the same type.",
                        left_type,
                        right_type
                    );
                };

                let instruction = match (op, &operand_type) {
                    // Integer operations
                    (BinOp::Add, &WasmType::I32) => Instruction::I32Add,
                    (BinOp::Sub, &WasmType::I32) => Instruction::I32Sub,
                    (BinOp::Mul, &WasmType::I32) => Instruction::I32Mul,
                    (BinOp::Div, &WasmType::I32) => Instruction::I32DivS,
                    (BinOp::Rem, &WasmType::I32) => Instruction::I32RemS,
                    (BinOp::BitAnd, &WasmType::I32) => Instruction::I32And,
                    (BinOp::BitOr, &WasmType::I32) => Instruction::I32Or,
                    (BinOp::BitXor, &WasmType::I32) => Instruction::I32Xor,
                    (BinOp::Shl, &WasmType::I32) => Instruction::I32Shl,
                    (BinOp::Shr, &WasmType::I32) => Instruction::I32ShrS,
                    (BinOp::Eq, &WasmType::I32) => Instruction::I32Eq,
                    (BinOp::Ne, &WasmType::I32) => Instruction::I32Ne,
                    (BinOp::Lt, &WasmType::I32) => Instruction::I32LtS,
                    (BinOp::Le, &WasmType::I32) => Instruction::I32LeS,
                    (BinOp::Gt, &WasmType::I32) => Instruction::I32GtS,
                    (BinOp::Ge, &WasmType::I32) => Instruction::I32GeS,

                    // Float operations
                    (BinOp::Add, &WasmType::F64) => Instruction::F64Add,
                    (BinOp::Sub, &WasmType::F64) => Instruction::F64Sub,
                    (BinOp::Mul, &WasmType::F64) => Instruction::F64Mul,
                    (BinOp::Div, &WasmType::F64) => Instruction::F64Div,
                    (BinOp::Eq, &WasmType::F64) => Instruction::F64Eq,
                    (BinOp::Ne, &WasmType::F64) => Instruction::F64Ne,
                    (BinOp::Lt, &WasmType::F64) => Instruction::F64Lt,
                    (BinOp::Le, &WasmType::F64) => Instruction::F64Le,
                    (BinOp::Gt, &WasmType::F64) => Instruction::F64Gt,
                    (BinOp::Ge, &WasmType::F64) => Instruction::F64Ge,

                    // I64 operations
                    (BinOp::Add, &WasmType::I64) => Instruction::I64Add,
                    (BinOp::Sub, &WasmType::I64) => Instruction::I64Sub,
                    (BinOp::Mul, &WasmType::I64) => Instruction::I64Mul,
                    (BinOp::Div, &WasmType::I64) => Instruction::I64DivS,
                    (BinOp::Rem, &WasmType::I64) => Instruction::I64RemS,
                    (BinOp::BitAnd, &WasmType::I64) => Instruction::I64And,
                    (BinOp::BitOr, &WasmType::I64) => Instruction::I64Or,
                    (BinOp::BitXor, &WasmType::I64) => Instruction::I64Xor,
                    (BinOp::Shl, &WasmType::I64) => Instruction::I64Shl,
                    (BinOp::Shr, &WasmType::I64) => Instruction::I64ShrS,
                    (BinOp::Eq, &WasmType::I64) => Instruction::I64Eq,
                    (BinOp::Ne, &WasmType::I64) => Instruction::I64Ne,
                    (BinOp::Lt, &WasmType::I64) => Instruction::I64LtS,
                    (BinOp::Le, &WasmType::I64) => Instruction::I64LeS,
                    (BinOp::Gt, &WasmType::I64) => Instruction::I64GtS,
                    (BinOp::Ge, &WasmType::I64) => Instruction::I64GeS,

                    // F32 operations
                    (BinOp::Add, &WasmType::F32) => Instruction::F32Add,
                    (BinOp::Sub, &WasmType::F32) => Instruction::F32Sub,
                    (BinOp::Mul, &WasmType::F32) => Instruction::F32Mul,
                    (BinOp::Div, &WasmType::F32) => Instruction::F32Div,
                    (BinOp::Eq, &WasmType::F32) => Instruction::F32Eq,
                    (BinOp::Ne, &WasmType::F32) => Instruction::F32Ne,
                    (BinOp::Lt, &WasmType::F32) => Instruction::F32Lt,
                    (BinOp::Le, &WasmType::F32) => Instruction::F32Le,
                    (BinOp::Gt, &WasmType::F32) => Instruction::F32Gt,
                    (BinOp::Ge, &WasmType::F32) => Instruction::F32Ge,

                    // Unsupported operations
                    (BinOp::Rem, &WasmType::F64) | (BinOp::Rem, &WasmType::F32) => {
                        return_compiler_error!(
                            "Remainder operation is not supported for floating-point types"
                        );
                    }
                    (
                        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr,
                        &WasmType::F64,
                    )
                    | (
                        BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr,
                        &WasmType::F32,
                    ) => {
                        return_compiler_error!(
                            "Bitwise operations are not supported for floating-point types"
                        );
                    }
                    (BinOp::And | BinOp::Or, _) => {
                        // Logical operations require special handling with control flow
                        return_compiler_error!(
                            "Logical operations (And/Or) require control flow and are not yet implemented in enhanced mode"
                        );
                    }
                    (_, wasm_type) => {
                        return_compiler_error!(
                            "Unsupported binary operation {:?} for WASM type {:?}",
                            op,
                            wasm_type
                        );
                    }
                };

                function_builder.instruction(&instruction)?;
                Ok(())
            }
            Rvalue::UnaryOp(op, operand) => {
                self.lower_operand_enhanced(operand, function_builder, local_map)?;

                // Determine the correct WASM instruction based on operand type
                let operand_type = self.get_operand_wasm_type_enhanced(operand, local_map)?;

                match (op, &operand_type) {
                    (UnOp::Not, &WasmType::I32) => {
                        function_builder.instruction(&Instruction::I32Eqz)?;
                    }
                    (UnOp::Neg, &WasmType::I32) => {
                        // Negate by subtracting from 0: 0 - x
                        function_builder.instruction(&Instruction::I32Const(0))?;
                        function_builder.instruction(&Instruction::I32Sub)?;
                    }
                    (UnOp::Neg, &WasmType::F64) => {
                        // Negate float using F64Neg instruction
                        function_builder.instruction(&Instruction::F64Neg)?;
                    }
                    (UnOp::Neg, &WasmType::F32) => {
                        // Negate float using F32Neg instruction
                        function_builder.instruction(&Instruction::F32Neg)?;
                    }
                    (UnOp::Neg, &WasmType::I64) => {
                        // Negate by subtracting from 0: 0 - x
                        function_builder.instruction(&Instruction::I64Const(0))?;
                        function_builder.instruction(&Instruction::I64Sub)?;
                    }
                    (UnOp::Not, wasm_type) => {
                        return_compiler_error!(
                            "Logical NOT operation is only supported for boolean (i32) types, found {:?}",
                            wasm_type
                        );
                    }
                    (UnOp::Neg, &WasmType::ExternRef) | (UnOp::Neg, &WasmType::FuncRef) => {
                        return_compiler_error!(
                            "Negation operation is not supported for reference types ({:?})",
                            operand_type
                        );
                    }
                }

                Ok(())
            }
            Rvalue::Ref { place, .. } => {
                // For references, load the place value
                self.lower_place_access_enhanced(place, function_builder, local_map)
            }
            Rvalue::StringConcat(left, right) => {
                // String concatenation: load both operands
                self.lower_operand_enhanced(left, function_builder, local_map)?;
                self.lower_operand_enhanced(right, function_builder, local_map)?;
                // For now, just leave both values on the stack
                // TODO: Implement actual string concatenation via runtime helper
                Ok(())
            }
        }
    }

    /// Enhanced operand lowering with validation
    fn lower_operand_enhanced(
        &mut self,
        operand: &Operand,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.lower_place_access_enhanced(place, function_builder, local_map)
            }
            Operand::Constant(constant) => self.lower_constant_enhanced(constant, function_builder),
            Operand::FunctionRef(func_index) => {
                function_builder.instruction(&Instruction::I32Const(*func_index as i32))?;
                Ok(())
            }
            Operand::GlobalRef(global_index) => {
                if let Some(wasm_global) = local_map.get_global(*global_index) {
                    function_builder.instruction(&Instruction::GlobalGet(wasm_global))?;
                    Ok(())
                } else {
                    return_compiler_error!("Global reference not found: {}", global_index);
                }
            }
        }
    }

    /// Enhanced place access with validation
    fn lower_place_access_enhanced(
        &mut self,
        place: &Place,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local { index, .. } => {
                if let Some(wasm_local) = local_map.get_local(*index) {
                    function_builder.instruction(&Instruction::LocalGet(wasm_local))?;
                    Ok(())
                } else {
                    return_compiler_error!("Local variable not found: {}", index);
                }
            }
            Place::Global { index, .. } => {
                if let Some(wasm_global) = local_map.get_global(*index) {
                    function_builder.instruction(&Instruction::GlobalGet(wasm_global))?;
                    Ok(())
                } else {
                    return_compiler_error!("Global variable not found: {}", index);
                }
            }
            Place::Memory { base, offset, .. } => {
                self.lower_memory_base_address_enhanced(base, function_builder)?;
                if offset.0 > 0 {
                    function_builder.instruction(&Instruction::I32Const(offset.0 as i32))?;
                    function_builder.instruction(&Instruction::I32Add)?;
                }
                function_builder.instruction(&Instruction::I32Load(MemArg {
                    align: 0,
                    offset: 0,
                    memory_index: 0,
                }))?;
                Ok(())
            }
            Place::Projection { base, elem } => {
                self.lower_place_access_enhanced(base, function_builder, local_map)?;

                match elem {
                    ProjectionElem::Field { offset, .. } => {
                        if offset.0 > 0 {
                            function_builder
                                .instruction(&Instruction::I32Const(offset.0 as i32))?;
                            function_builder.instruction(&Instruction::I32Add)?;
                        }
                        function_builder.instruction(&Instruction::I32Load(MemArg {
                            align: 0,
                            offset: 0,
                            memory_index: 0,
                        }))?;
                    }
                    ProjectionElem::Index { index, .. } => {
                        self.lower_place_access_enhanced(index, function_builder, local_map)?;
                        function_builder.instruction(&Instruction::I32Const(4))?; // Assume 4-byte elements
                        function_builder.instruction(&Instruction::I32Mul)?;
                        function_builder.instruction(&Instruction::I32Add)?;
                        function_builder.instruction(&Instruction::I32Load(MemArg {
                            align: 0,
                            offset: 0,
                            memory_index: 0,
                        }))?;
                    }
                    ProjectionElem::Length => {
                        // For length access, load the length field (typically at offset 0)
                        function_builder.instruction(&Instruction::I32Load(MemArg {
                            align: 0,
                            offset: 0,
                            memory_index: 0,
                        }))?;
                    }
                    ProjectionElem::Data => {
                        // For data access, load the data pointer (typically at offset 4)
                        function_builder.instruction(&Instruction::I32Load(MemArg {
                            align: 0,
                            offset: 4,
                            memory_index: 0,
                        }))?;
                    }
                    ProjectionElem::Deref => {
                        // Dereference: load the value at the address
                        function_builder.instruction(&Instruction::I32Load(MemArg {
                            align: 0,
                            offset: 0,
                            memory_index: 0,
                        }))?;
                    }
                }
                Ok(())
            }
        }
    }

    /// Enhanced place assignment with validation
    fn lower_place_assignment_with_type_enhanced(
        &mut self,
        place: &Place,
        _wasm_type: &WasmType,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local { index, .. } => {
                if let Some(wasm_local) = local_map.get_local(*index) {
                    function_builder.instruction(&Instruction::LocalSet(wasm_local))?;
                    Ok(())
                } else {
                    return_compiler_error!("Local variable not found for assignment: {}", index);
                }
            }
            Place::Global { index, .. } => {
                if let Some(wasm_global) = local_map.get_global(*index) {
                    function_builder.instruction(&Instruction::GlobalSet(wasm_global))?;
                    Ok(())
                } else {
                    return_compiler_error!("Global variable not found for assignment: {}", index);
                }
            }
            Place::Memory { base, offset, .. } => {
                // For memory stores, we need the address on the stack first
                self.lower_memory_base_address_enhanced(base, function_builder)?;
                if offset.0 > 0 {
                    function_builder.instruction(&Instruction::I32Const(offset.0 as i32))?;
                    function_builder.instruction(&Instruction::I32Add)?;
                }
                // Value should already be on stack from rvalue evaluation
                // We need to swap stack order: [value, address] -> [address, value]
                // For now, this is simplified - proper implementation would handle stack management
                function_builder.instruction(&Instruction::I32Store(MemArg {
                    align: 0,
                    offset: 0,
                    memory_index: 0,
                }))?;
                Ok(())
            }
            Place::Projection { .. } => {
                return_compiler_error!(
                    "Projection assignment not yet implemented in enhanced mode"
                );
            }
        }
    }

    /// Get the WASM type of an operand for enhanced type checking
    fn get_operand_wasm_type_enhanced(
        &self,
        operand: &Operand,
        local_map: &LocalMap,
    ) -> Result<WasmType, CompileError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => Ok(place.wasm_type()),
            Operand::Constant(constant) => {
                match constant {
                    Constant::I32(_) => Ok(WasmType::I32),
                    Constant::I64(_) => Ok(WasmType::I64),
                    Constant::F32(_) => Ok(WasmType::F32),
                    Constant::F64(_) => Ok(WasmType::F64),
                    Constant::Bool(_) => Ok(WasmType::I32), // Booleans are i32 in WASM
                    Constant::String(_) | Constant::MutableString(_) => Ok(WasmType::I32), // String pointers are i32
                    Constant::Function(_) => Ok(WasmType::I32), // Function indices are i32
                    Constant::Null => Ok(WasmType::I32),        // Null pointer is i32
                    Constant::MemoryOffset(_) => Ok(WasmType::I32), // Memory offsets are i32
                    Constant::TypeSize(_) => Ok(WasmType::I32), // Type sizes are i32
                }
            }
            Operand::FunctionRef(_) => Ok(WasmType::I32), // Function references are i32 indices
            Operand::GlobalRef(global_index) => {
                // Look up the global's type from the local map or place manager
                if let Some(_wasm_global) = local_map.get_global(*global_index) {
                    // For now, assume globals are i32 - this could be enhanced to track actual types
                    Ok(WasmType::I32)
                } else {
                    return_compiler_error!("Global reference not found: {}", global_index);
                }
            }
        }
    }

    /// Enhanced constant lowering with validation
    fn lower_constant_enhanced(
        &mut self,
        constant: &Constant,
        function_builder: &mut EnhancedFunctionBuilder,
    ) -> Result<(), CompileError> {
        match constant {
            Constant::I32(value) => {
                function_builder.instruction(&Instruction::I32Const(*value))?;
                Ok(())
            }
            Constant::I64(value) => {
                function_builder.instruction(&Instruction::I64Const(*value))?;
                Ok(())
            }
            Constant::F32(value) => {
                function_builder.instruction(&Instruction::F32Const((*value).into()))?;
                Ok(())
            }
            Constant::F64(value) => {
                function_builder.instruction(&Instruction::F64Const((*value).into()))?;
                Ok(())
            }
            Constant::Bool(value) => {
                function_builder.instruction(&Instruction::I32Const(if *value { 1 } else { 0 }))?;
                Ok(())
            }
            Constant::String(value) => {
                let offset = self.string_manager.add_string_slice_constant(value);
                function_builder.instruction(&Instruction::I32Const(offset as i32))?;
                Ok(())
            }
            Constant::MutableString(value) => {
                let default_capacity = (value.len() as u32).max(32);
                let offset = self
                    .string_manager
                    .allocate_mutable_string(value, default_capacity);
                function_builder.instruction(&Instruction::I32Const(offset as i32))?;
                Ok(())
            }
            Constant::Function(func_index) => {
                function_builder.instruction(&Instruction::I32Const(*func_index as i32))?;
                Ok(())
            }
            Constant::Null => {
                function_builder.instruction(&Instruction::I32Const(0))?;
                Ok(())
            }
            _ => {
                return_compiler_error!("Unsupported constant type: {:?}", constant);
            }
        }
    }

    /// Enhanced memory base address lowering
    fn lower_memory_base_address_enhanced(
        &mut self,
        base: &MemoryBase,
        function_builder: &mut EnhancedFunctionBuilder,
    ) -> Result<(), CompileError> {
        match base {
            MemoryBase::LinearMemory => {
                function_builder.instruction(&Instruction::I32Const(0))?; // Base of linear memory
                Ok(())
            }
            MemoryBase::Stack => {
                return_compiler_error!("Stack memory base not yet implemented in enhanced mode");
            }
            MemoryBase::Heap { alloc_id, .. } => {
                // For heap allocations, we would need to track allocation addresses
                // For now, use a placeholder based on allocation ID
                function_builder.instruction(&Instruction::I32Const(*alloc_id as i32 * 1024))?; // Placeholder
                Ok(())
            }
        }
    }

    /// Enhanced function call lowering
    fn lower_wir_call_enhanced(
        &mut self,
        func: &Operand,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load arguments
        for arg in args {
            self.lower_operand_enhanced(arg, function_builder, local_map)?;
        }

        // Generate call instruction
        match func {
            Operand::FunctionRef(func_index) => {
                function_builder.instruction(&Instruction::Call(*func_index))?;
            }
            _ => {
                return_compiler_error!("Unsupported function operand type: {:?}", func);
            }
        }

        // Store result if destination exists
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type_enhanced(
                dest_place,
                &return_type,
                function_builder,
                local_map,
            )?;
        }

        Ok(())
    }

    /// Enhanced host function call lowering
    fn lower_wir_host_call_enhanced(
        &mut self,
        host_func: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Check for runtime-specific mapping if registry is available
        if let Some(registry) = self.host_registry.clone() {
            return self.lower_host_call_with_registry_enhanced(
                host_func,
                args,
                destination,
                function_builder,
                local_map,
                &registry,
            );
        }

        // Fallback to original logic
        // Check if this is a WASIX function first
        if self.wasix_registry.has_function(&host_func.name) {
            return self.lower_wasix_host_call(
                &host_func.name,
                args,
                destination,
                function_builder,
                local_map,
            );
        }

        // Fall back to regular host function handling
        // Load arguments
        for arg in args {
            self.lower_operand_enhanced(arg, function_builder, local_map)?;
        }

        // Generate host call instruction
        if let Some(&func_index) = self.host_function_indices.get(&host_func.name) {
            function_builder.instruction(&Instruction::Call(func_index))?;
        } else {
            return_compiler_error!("Host function not found: {}", host_func.name);
        }

        // Store result if destination exists
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type_enhanced(
                dest_place,
                &return_type,
                function_builder,
                local_map,
            )?;
        }

        Ok(())
    }

    /// Enhanced host function call lowering with registry-aware runtime mapping
    fn lower_host_call_with_registry_enhanced(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
        registry: &crate::compiler::host_functions::registry::HostFunctionRegistry,
    ) -> Result<(), CompileError> {
        use crate::compiler::host_functions::registry::RuntimeFunctionMapping;

        // Get runtime-specific mapping
        match registry.get_runtime_mapping(&host_function.name) {
            Some(RuntimeFunctionMapping::Wasix(wasix_func)) => {
                // Use WASIX fd_write generation for print calls
                if host_function.name == "print" {
                    return self.generate_wasix_fd_write_call_enhanced(args, function_builder, local_map);
                }
                
                // For other WASIX functions, use standard call lowering
                self.lower_standard_host_call_enhanced(host_function, args, destination, function_builder, local_map)
            }
            Some(RuntimeFunctionMapping::JavaScript(_js_func)) => {
                // Use JavaScript binding (not implemented yet)
                return_compiler_error!(
                    "JavaScript runtime mappings not yet implemented for host function '{}'",
                    host_function.name
                );
            }
            Some(RuntimeFunctionMapping::Native(_)) | None => {
                // Use standard host function call
                self.lower_standard_host_call_enhanced(host_function, args, destination, function_builder, local_map)
            }
        }
    }

    /// Enhanced standard host function call lowering
    fn lower_standard_host_call_enhanced(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load arguments
        for arg in args {
            self.lower_operand_enhanced(arg, function_builder, local_map)?;
        }

        // Generate host call instruction
        if let Some(&func_index) = self.host_function_indices.get(&host_function.name) {
            function_builder.instruction(&Instruction::Call(func_index))?;
        } else {
            return_compiler_error!("Host function not found: {}", host_function.name);
        }

        // Store result if destination exists
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type_enhanced(
                dest_place,
                &return_type,
                function_builder,
                local_map,
            )?;
        }

        Ok(())
    }

    /// Generate WASIX fd_write call for print statements (enhanced version)
    fn generate_wasix_fd_write_call_enhanced(
        &mut self,
        args: &[Operand],
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Validate arguments - print() should have exactly one string argument
        if args.len() != 1 {
            return_compiler_error!(
                "print() function expects exactly 1 argument, got {}. This should be caught during type checking.",
                args.len()
            );
        }

        let string_arg = &args[0];

        // Extract string content from the operand
        let string_content = match string_arg {
            Operand::Constant(Constant::String(content)) => content.clone(),
            _ => {
                // For non-constant strings, we need to handle them differently
                // For now, this is a limitation - we only support string literals
                return_compiler_error!(
                    "WASIX print() currently only supports string literals. Variable string printing not yet implemented."
                );
            }
        };

        // Add string data to linear memory allocation
        let string_offset = self.string_manager.add_string_slice_constant(&string_content);
        let string_len = string_content.len() as u32;

        // Skip the 4-byte length prefix to get to the actual string data
        let string_ptr = string_offset + 4;

        // Implement string-to-IOVec conversion for WASIX calls
        let iovec = crate::compiler::host_functions::wasix_registry::IOVec::new(string_ptr, string_len);

        // Add IOVec structure to data section with proper WASIX alignment
        let iovec_bytes = iovec.to_bytes();
        let iovec_offset = self.string_manager.add_raw_data(&iovec_bytes);

        // Allocate space for nwritten result with WASIX alignment
        let nwritten_bytes = [0u8; 4]; // Initialize to 0
        let nwritten_offset = self.string_manager.add_raw_data(&nwritten_bytes);

        // Get the WASIX fd_write function index
        let wasix_function = match self.wasix_registry.get_function("print") {
            Some(func) => func,
            None => {
                return_compiler_error!(
                    "WASIX function 'print' not found in registry. This should be registered during module initialization."
                );
            }
        };

        let fd_write_func_index = wasix_function.get_function_index()?;

        // Generate WASM instruction sequence for WASIX fd_write call
        // Handle WASIX calling conventions: fd_write(fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32

        // Load stdout file descriptor (constant 1) onto WASM stack
        function_builder.instruction(&Instruction::I32Const(1))?;

        // Load IOVec pointer (offset in linear memory where IOVec structure is stored)
        function_builder.instruction(&Instruction::I32Const(iovec_offset as i32))?;

        // Load IOVec count (1 for single string argument)
        function_builder.instruction(&Instruction::I32Const(1))?;

        // Load nwritten result pointer (where fd_write will store bytes written)
        function_builder.instruction(&Instruction::I32Const(nwritten_offset as i32))?;

        // Generate call instruction to imported fd_write function
        function_builder.instruction(&Instruction::Call(fd_write_func_index))?;

        // Handle WASIX return values
        // fd_write returns errno (0 for success, non-zero for error)
        // For now, we'll just drop the return value, but this provides foundation for error handling
        function_builder.instruction(&Instruction::Drop)?;

        Ok(())
    }

    /// Lower WASIX-specific host function calls with native function support
    ///
    /// This method handles WASIX function calls by routing them to appropriate
    /// lowering methods. It supports both native implementations and import-based
    /// function calls, with the choice determined at runtime.
    fn lower_wasix_host_call(
        &mut self,
        function_name: &str,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match function_name {
            "print" | "template_output" => self.lower_wasix_print(args, destination, function_builder, local_map),
            _ => {
                return_compiler_error!(
                    "Unsupported WASIX function: {}. Only 'print' and 'template_output' are currently implemented.",
                    function_name
                );
            }
        }
    }

    /// Lower print() function to WASIX fd_write call
    ///
    /// This method converts a Beanstalk print() call into a WASIX fd_write call by:
    /// 1. Adding string data to the WASM data section using StringManager
    /// 2. Creating IOVec structure in the data section pointing to string data
    /// 3. Generating WASM instruction sequence for WASIX fd_write call
    /// 4. Supporting both native and import-based function calls
    fn lower_wasix_print(
        &mut self,
        args: &[Operand],
        _destination: &Option<Place>,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Validate arguments - print() should have exactly one string argument
        if args.len() != 1 {
            return_compiler_error!(
                "print() function expects exactly 1 argument, got {}. This should be caught during type checking.",
                args.len()
            );
        }

        let string_arg = &args[0];

        // Check if this is a constant string or a variable
        match string_arg {
            Operand::Constant(Constant::String(content)) => {
                // String literal - use the existing implementation
                self.lower_wasix_print_constant_enhanced(content, function_builder)
            }
            Operand::Copy(place) | Operand::Move(place) => {
                // String variable - use runtime implementation
                self.lower_wasix_print_variable_enhanced(place, function_builder, local_map)
            }
            _ => {
                return_compiler_error!(
                    "print() argument must be a string literal or string variable, got {:?}",
                    string_arg
                );
            }
        }
    }

    /// Print a constant string literal using EnhancedFunctionBuilder
    fn lower_wasix_print_constant_enhanced(
        &mut self,
        string_content: &str,
        function_builder: &mut EnhancedFunctionBuilder,
    ) -> Result<(), CompileError> {

        // Add string data to WASM data section with proper alignment
        let string_offset = self.string_manager.add_string_constant(&string_content);
        let string_len = string_content.len() as u32;

        // FIXED: Use the StringManager offset (where data actually exists) instead of WasixMemoryManager
        // The StringManager writes actual string data to the WASM data section
        // Skip the 4-byte length prefix to get to the actual string data
        let string_ptr = string_offset + 4;

        // Create IOVec structure with WASIX alignment (8-byte aligned)
        let iovec =
            crate::compiler::host_functions::wasix_registry::IOVec::new(string_ptr, string_len);

        // Add IOVec structure to data section with proper WASIX alignment
        let iovec_bytes = iovec.to_bytes();
        let iovec_offset = self.add_raw_data_to_section(&iovec_bytes);

        // Allocate space for nwritten result with WASIX alignment
        let _nwritten_ptr = match self.wasix_memory_manager.allocate(4, 4) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX nwritten allocation failed: {}", e);
            }
        };
        let nwritten_bytes = [0u8; 4]; // Initialize to 0
        let nwritten_offset = self.add_raw_data_to_section(&nwritten_bytes);

        // Get the WASIX fd_write function index
        let wasix_function = match self.wasix_registry.get_function("print") {
            Some(func) => func,
            None => {
                return_compiler_error!(
                    "WASIX function 'print' not found in registry. This should be registered during module initialization."
                );
            }
        };

        let fd_write_func_index = wasix_function.get_function_index()?;

        // Generate WASM instruction sequence for fd_write call
        self.generate_fd_write_instructions_with_offsets(
            iovec_offset,
            nwritten_offset,
            fd_write_func_index,
            function_builder,
        )?;

        Ok(())
    }

    /// Print a string variable using EnhancedFunctionBuilder
    fn lower_wasix_print_variable_enhanced(
        &mut self,
        place: &Place,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Get the WASIX fd_write function index
        let wasix_function = match self.wasix_registry.get_function("print") {
            Some(func) => func,
            None => {
                return_compiler_error!(
                    "WASIX function 'print' not found in registry. This should be registered during module initialization."
                );
            }
        };

        let fd_write_func_index = wasix_function.get_function_index()?;

        // Load the string pointer from the variable
        // The place contains a pointer to: [length: u32][data: bytes]
        self.lower_place_access_enhanced(place, function_builder, local_map)?;
        
        // Stack: [string_ptr]
        // Duplicate for later use
        function_builder.instruction(&Instruction::LocalTee(0))?; // Save string_ptr in local 0
        
        // Read the length (first 4 bytes)
        function_builder.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2, // 4-byte alignment
            memory_index: 0,
        }))?;
        
        // Stack: [length]
        function_builder.instruction(&Instruction::LocalSet(1))?; // Save length in local 1
        
        // Calculate data pointer (string_ptr + 4)
        function_builder.instruction(&Instruction::LocalGet(0))?;
        function_builder.instruction(&Instruction::I32Const(4))?;
        function_builder.instruction(&Instruction::I32Add)?;
        function_builder.instruction(&Instruction::LocalSet(2))?; // Save data_ptr in local 2
        
        // Now we need to create an IOVec structure at runtime
        // Allocate IOVec space (8 bytes: ptr + len)
        let iovec_ptr = match self.wasix_memory_manager.allocate_iovec_array(1) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX IOVec allocation failed: {}", e);
            }
        };
        
        // Write data_ptr to IOVec (first 4 bytes)
        function_builder.instruction(&Instruction::I32Const(iovec_ptr as i32))?;
        function_builder.instruction(&Instruction::LocalGet(2))?; // data_ptr
        function_builder.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }))?;
        
        // Write length to IOVec (next 4 bytes)
        function_builder.instruction(&Instruction::I32Const(iovec_ptr as i32 + 4))?;
        function_builder.instruction(&Instruction::LocalGet(1))?; // length
        function_builder.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }))?;
        
        // Allocate space for nwritten result
        let nwritten_ptr = match self.wasix_memory_manager.allocate(4, 4) {
            Ok(ptr) => ptr,
            Err(e) => {
                return_compiler_error!("WASIX nwritten allocation failed: {}", e);
            }
        };
        
        // Call fd_write(fd=1, iovs=iovec_ptr, iovs_len=1, nwritten=nwritten_ptr)
        function_builder.instruction(&Instruction::I32Const(1))?; // stdout fd
        function_builder.instruction(&Instruction::I32Const(iovec_ptr as i32))?;
        function_builder.instruction(&Instruction::I32Const(1))?; // iovs_len
        function_builder.instruction(&Instruction::I32Const(nwritten_ptr as i32))?;
        function_builder.instruction(&Instruction::Call(fd_write_func_index))?;
        
        // Drop the return value (errno)
        function_builder.instruction(&Instruction::Drop)?;
        
        Ok(())
    }

    /// Generate native WASIX function call for JIT runtime integration
    ///
    /// This method generates appropriate WASM instructions for native function calls
    /// and provides fallback to import-based calls when native functions are unavailable.
    /// The choice between native and import-based calls is determined at runtime.
    fn generate_native_wasix_call(
        &mut self,
        function_name: &str,
        args: &[Operand],
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Check if this function has a native implementation available
        let has_native_impl = self
            .wasix_registry
            .get_function(function_name)
            .map(|func| func.has_native_impl())
            .unwrap_or(false);

        if has_native_impl {
            // Generate native function call
            #[cfg(feature = "verbose_codegen_logging")]
            println!("WASM: Generating native WASIX call for '{}'", function_name);

            self.generate_native_function_call(function_name, args, function_builder, local_map)
        } else {
            // Fallback to import-based call
            #[cfg(feature = "verbose_codegen_logging")]
            println!(
                "WASM: Falling back to import-based WASIX call for '{}'",
                function_name
            );

            self.generate_import_based_call(function_name, args, function_builder, local_map)
        }
    }

    /// Generate native function call instructions for JIT runtime
    ///
    /// This generates WASM instructions that will be handled by the JIT runtime's
    /// native function dispatch system. The JIT will intercept these calls and
    /// route them to native WASIX implementations.
    fn generate_native_function_call(
        &mut self,
        function_name: &str,
        args: &[Operand],
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load arguments onto the WASM stack
        for arg in args {
            self.lower_operand_enhanced(arg, function_builder, local_map)?;
        }

        // Get the function index for the native call
        // Native functions are still imported but marked for native handling
        let func_index = match self.host_function_indices.get(function_name) {
            Some(&index) => index,
            None => {
                return_compiler_error!(
                    "Native WASIX function '{}' not found in function indices. This should be registered during import generation.",
                    function_name
                );
            }
        };

        // Generate call instruction - the JIT runtime will intercept this
        // and route it to the native implementation
        function_builder.instruction(&Instruction::Call(func_index))?;

        // The native implementation handles the result directly
        // No additional result processing needed here

        Ok(())
    }

    /// Generate import-based function call as fallback
    ///
    /// This generates standard WASM import calls when native implementations
    /// are not available. This ensures compatibility with all WASIX runtimes.
    fn generate_import_based_call(
        &mut self,
        function_name: &str,
        args: &[Operand],
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load arguments onto the WASM stack
        for arg in args {
            self.lower_operand_enhanced(arg, function_builder, local_map)?;
        }

        // Get the function index for the import-based call
        let func_index = match self.host_function_indices.get(function_name) {
            Some(&index) => index,
            None => {
                return_compiler_error!(
                    "WASIX function '{}' not found in function indices. This should be registered during import generation.",
                    function_name
                );
            }
        };

        // Generate standard call instruction to imported function
        function_builder.instruction(&Instruction::Call(func_index))?;

        Ok(())
    }

    /// Add raw data to the string manager's data section
    /// This is a helper method for adding non-string data like IOVec structures
    fn add_raw_data_to_section(&mut self, data: &[u8]) -> u32 {
        self.string_manager.add_raw_data(data)
    }

    /// Generate WASM instruction sequence for WASIX fd_write call with specific memory offsets
    fn generate_fd_write_instructions_with_offsets(
        &mut self,
        iovec_offset: u32,
        nwritten_offset: u32,
        fd_write_func_index: u32,
        function_builder: &mut EnhancedFunctionBuilder,
    ) -> Result<(), CompileError> {
        // Load stdout file descriptor (constant 1) onto WASM stack
        function_builder.instruction(&Instruction::I32Const(1))?;

        // Load IOVec pointer (offset in linear memory where IOVec structure is stored)
        function_builder.instruction(&Instruction::I32Const(iovec_offset as i32))?;

        // Load IOVec count (1 for single string argument)
        function_builder.instruction(&Instruction::I32Const(1))?;

        // Load nwritten result pointer (where fd_write will store bytes written)
        function_builder.instruction(&Instruction::I32Const(nwritten_offset as i32))?;

        // Generate call instruction to imported fd_write function
        #[cfg(feature = "verbose_codegen_logging")]
        println!(
            "WASM: Generating call instruction to fd_write with function index: {}",
            fd_write_func_index
        );
        function_builder.instruction(&Instruction::Call(fd_write_func_index))?;

        // Handle the return value (errno) for basic error detection
        // Store errno in a local variable for potential error checking
        // For now, we'll just drop it, but this provides the foundation for error handling
        function_builder.instruction(&Instruction::Drop)?;

        // TODO: In the future, we could:
        // 1. Check if errno != 0 (error occurred)
        // 2. Read nwritten value to verify bytes written
        // 3. Generate appropriate error handling code

        Ok(())
    }

    /// Generate WASM instruction sequence for WASIX fd_write call
    ///
    /// WASIX fd_write signature: (fd: i32, iovs: i32, iovs_len: i32, nwritten: i32) -> i32
    /// - fd: File descriptor (1 for stdout)
    /// - iovs: Pointer to IOVec array in linear memory
    /// - iovs_len: Number of IOVec structures (1 for single string)
    /// - nwritten: Pointer to write the number of bytes written
    /// - Returns: Error code (0 for success)
    fn generate_fd_write_instructions(
        &mut self,
        call_context: &crate::compiler::host_functions::wasix_registry::WasixCallContext,
        fd_write_func_index: u32,
        function_builder: &mut EnhancedFunctionBuilder,
    ) -> Result<(), CompileError> {
        // Load stdout file descriptor (constant 1) onto WASM stack
        function_builder.instruction(&Instruction::I32Const(1))?;

        // Load IOVec pointer (pointer to IOVec structure in linear memory)
        function_builder
            .instruction(&Instruction::I32Const(call_context.iovec_region.ptr as i32))?;

        // Load IOVec count (1 for single string argument)
        function_builder.instruction(&Instruction::I32Const(1))?;

        // Load nwritten result pointer (where fd_write will store bytes written)
        function_builder.instruction(&Instruction::I32Const(
            call_context.result_region.ptr as i32,
        ))?;

        // Generate call instruction to imported fd_write function
        function_builder.instruction(&Instruction::Call(fd_write_func_index))?;

        // Handle the return value (errno) for basic error detection
        // Store errno in a local variable for potential error checking
        // For now, we'll just drop it, but this provides the foundation for error handling
        function_builder.instruction(&Instruction::Drop)?;

        // TODO: In the future, we could:
        // 1. Check if errno != 0 (error occurred)
        // 2. Read nwritten value to verify bytes written
        // 3. Generate appropriate error handling code

        Ok(())
    }

    /// Enhanced Beanstalk goto lowering
    fn lower_beanstalk_goto_enhanced(
        &mut self,
        _target: u32,
        function_builder: &mut EnhancedFunctionBuilder,
        _local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // For now, goto is implemented as a no-op
        // Proper implementation would require block label management
        function_builder.instruction(&Instruction::Nop)?;
        Ok(())
    }

    /// Enhanced Beanstalk if terminator lowering with proper control flow validation
    fn lower_beanstalk_if_terminator_enhanced(
        &mut self,
        condition: &Operand,
        _then_block: u32,
        _else_block: u32,
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load condition
        self.lower_operand_enhanced(condition, function_builder, local_map)?;

        // Generate if instruction with proper control frame tracking
        function_builder.instruction(&Instruction::If(BlockType::Empty))?;

        // Then block (placeholder)
        function_builder.instruction(&Instruction::Nop)?;

        // Else block
        function_builder.instruction(&Instruction::Else)?;
        function_builder.instruction(&Instruction::Nop)?;

        // End if block
        function_builder.instruction(&Instruction::End)?;

        Ok(())
    }

    /// Enhanced Beanstalk return lowering
    fn lower_beanstalk_return_enhanced(
        &mut self,
        values: &[Operand],
        function_builder: &mut EnhancedFunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load return values
        for value in values {
            self.lower_operand_enhanced(value, function_builder, local_map)?;
        }

        // Generate return instruction
        function_builder.instruction(&Instruction::Return)?;

        Ok(())
    }

    /// Lower struct validation to WASM instructions
    fn lower_struct_validation(
        &mut self,
        _struct_place: &Place,
        _struct_type: &DataType,
        _function_builder: &mut EnhancedFunctionBuilder,
        _local_map: &mut LocalMap,
    ) -> Result<(), CompileError> {
        // For now, struct validation is handled at compile time
        // In a more complete implementation, this could generate runtime checks
        // for uninitialized fields if needed
        Ok(())
    }

    /// Lower struct validation to WASM instructions (simple version)
    fn lower_struct_validation_simple(
        &mut self,
        _struct_place: &Place,
        _struct_type: &DataType,
        _function: &mut Function,
        _local_map: &mut LocalMap,
    ) -> Result<(), CompileError> {
        // For now, struct validation is handled at compile time
        // In a more complete implementation, this could generate runtime checks
        // for uninitialized fields if needed
        Ok(())
    }

    /// Generate WASM instructions for struct field access with proper memory layout
    fn lower_struct_field_access(
        &mut self,
        base_place: &Place,
        field_offset: u32,
        field_size: &FieldSize,
        function: &mut Function,
        local_map: &mut LocalMap,
    ) -> Result<(), CompileError> {
        // Load base address of the struct
        self.lower_place_access(base_place, function, local_map)?;

        // Add field offset if non-zero
        if field_offset > 0 {
            function.instruction(&Instruction::I32Const(field_offset as i32));
            function.instruction(&Instruction::I32Add);
        }

        // Generate appropriate load instruction based on field size
        match field_size {
            FieldSize::Fixed(1) => {
                function.instruction(&Instruction::I32Load8U(MemArg {
                    offset: 0,
                    align: 0, // 1-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(2) => {
                function.instruction(&Instruction::I32Load16U(MemArg {
                    offset: 0,
                    align: 1, // 2-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(4) => {
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2, // 4-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(8) => {
                function.instruction(&Instruction::I64Load(MemArg {
                    offset: 0,
                    align: 3, // 8-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::WasmType(wasm_type) => {
                match wasm_type {
                    WasmType::I32 => {
                        function.instruction(&Instruction::I32Load(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    WasmType::I64 => {
                        function.instruction(&Instruction::I64Load(MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    WasmType::F32 => {
                        function.instruction(&Instruction::F32Load(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    WasmType::F64 => {
                        function.instruction(&Instruction::F64Load(MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    WasmType::ExternRef | WasmType::FuncRef => {
                        // References are stored as i32 pointers
                        function.instruction(&Instruction::I32Load(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                }
            }
            FieldSize::Variable => {
                // Variable size fields are typically pointers to the actual data
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(size) => {
                // Handle other fixed sizes by defaulting to appropriate instruction
                if *size <= 4 {
                    function.instruction(&Instruction::I32Load(MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                } else {
                    function.instruction(&Instruction::I64Load(MemArg {
                        offset: 0,
                        align: 3,
                        memory_index: 0,
                    }));
                }
            }
        }

        Ok(())
    }

    /// Generate WASM instructions for struct field assignment with proper memory layout
    fn lower_struct_field_assignment(
        &mut self,
        base_place: &Place,
        field_offset: u32,
        field_size: &FieldSize,
        value_operand: &Operand,
        function: &mut Function,
        local_map: &mut LocalMap,
    ) -> Result<(), CompileError> {
        // Load base address of the struct
        self.lower_place_access(base_place, function, local_map)?;

        // Add field offset if non-zero
        if field_offset > 0 {
            function.instruction(&Instruction::I32Const(field_offset as i32));
            function.instruction(&Instruction::I32Add);
        }

        // Load the value to store
        self.lower_operand(value_operand, function, local_map)?;

        // Generate appropriate store instruction based on field size
        match field_size {
            FieldSize::Fixed(1) => {
                function.instruction(&Instruction::I32Store8(MemArg {
                    offset: 0,
                    align: 0, // 1-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(2) => {
                function.instruction(&Instruction::I32Store16(MemArg {
                    offset: 0,
                    align: 1, // 2-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(4) => {
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2, // 4-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(8) => {
                function.instruction(&Instruction::I64Store(MemArg {
                    offset: 0,
                    align: 3, // 8-byte alignment
                    memory_index: 0,
                }));
            }
            FieldSize::WasmType(wasm_type) => {
                match wasm_type {
                    WasmType::I32 => {
                        function.instruction(&Instruction::I32Store(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    WasmType::I64 => {
                        function.instruction(&Instruction::I64Store(MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    WasmType::F32 => {
                        function.instruction(&Instruction::F32Store(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    WasmType::F64 => {
                        function.instruction(&Instruction::F64Store(MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    WasmType::ExternRef | WasmType::FuncRef => {
                        // References are stored as i32 pointers
                        function.instruction(&Instruction::I32Store(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                }
            }
            FieldSize::Variable => {
                // Variable size fields are typically pointers to the actual data
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
            }
            FieldSize::Fixed(size) => {
                // Handle other fixed sizes by defaulting to appropriate instruction
                if *size <= 4 {
                    function.instruction(&Instruction::I32Store(MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                } else {
                    function.instruction(&Instruction::I64Store(MemArg {
                        offset: 0,
                        align: 3,
                        memory_index: 0,
                    }));
                }
            }
        }

        Ok(())
    }
}

/// Check if a module name is a valid WASIX module
fn is_valid_wasix_module(module: &str) -> bool {
    matches!(
        module,
        "wasix_32v1"
            | "wasix_64v1"
            | "wasix_snapshot_preview1"
            | "wasi_snapshot_preview1"
            | "wasi_unstable"
    )
}

/// Check if a WASM type is supported by WASIX
fn is_supported_wasix_type(val_type: &ValType) -> bool {
    matches!(
        val_type,
        ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64
    )
}
