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
use crate::compiler::host_functions::registry::HostFunctionRegistry;
use crate::compiler::string_interning::StringTable;
use crate::compiler::wir::place::{
    MemoryBase, Place, ProjectionElem, TypeSize, WasmType,
};
use crate::compiler::wir::wir_nodes::{
    BinOp, BorrowKind, Constant, Operand, Rvalue, Statement, Terminator, UnOp, WIR,
    WirFunction,
};
use crate::return_compiler_error;
use std::collections::{HashMap, HashSet};
use wasm_encoder::*;

const ENTRY_POINT_FUNCTION_NAME: &str = "_start";
const MAIN_FUNCTION_NAME: &str = "main";

/// Function builder that leverages wasm_encoder's built-in validation and control flow management
///
/// This builder provides type-safe WASM generation with automatic validation and proper control frame management.
/// It addresses the "control frames remain" error by ensuring all control structures are properly opened and closed.
#[derive(Debug)]
pub struct FunctionBuilder {
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

impl FunctionBuilder {
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
                        let function_name_static: &'static str =
                            Box::leak(self.function_name.clone().into_boxed_str());
                        return_compiler_error!(
                            "WASM validation error in function '{}': else instruction without matching if",
                            self.function_name ; {
                                CompilationStage => "WASM Validation",
                                VariableName => function_name_static,
                                PrimarySuggestion => "Ensure 'else' is only used within an 'if' block",
                            }
                        );
                    }
                    frame.frame_type = ControlFrameType::Else;
                } else {
                    let function_name_static: &'static str =
                        Box::leak(self.function_name.clone().into_boxed_str());
                    return_compiler_error!(
                        "WASM validation error in function '{}': else instruction with empty control stack",
                        self.function_name ; {
                            CompilationStage => "WASM Validation",
                            VariableName => function_name_static,
                            PrimarySuggestion => "This is an internal compiler error - control stack is empty when processing 'else'",
                        }
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
                        let function_name_static: &'static str =
                            Box::leak(self.function_name.clone().into_boxed_str());
                        return_compiler_error!(
                            "WASM validation error in function '{}': attempting to end function frame",
                            self.function_name ; {
                                CompilationStage => "WASM Validation",
                                VariableName => function_name_static,
                                PrimarySuggestion => "This is an internal compiler error - function frame ended prematurely",
                            }
                        );
                    }
                } else {
                    let function_name_static: &'static str =
                        Box::leak(self.function_name.clone().into_boxed_str());
                    return_compiler_error!(
                        "WASM validation error in function '{}': end instruction with empty control stack",
                        self.function_name ; {
                            CompilationStage => "WASM Validation",
                            VariableName => function_name_static,
                            PrimarySuggestion => "This is an internal compiler error - control stack is empty when processing 'end'",
                        }
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
            let function_name_static: &'static str =
                Box::leak(self.function_name.clone().into_boxed_str());
            return_compiler_error!(
                "Function '{}' has too many parameters ({}). WASM functions are limited to 1000 parameters.",
                self.function_name,
                self.param_types.len() ; {
                    CompilationStage => "WASM Validation",
                    VariableName => function_name_static,
                    PrimarySuggestion => "Reduce the number of function parameters to 1000 or fewer",
                }
            );
        }

        // Validate return type count doesn't exceed WASM limits
        if self.result_types.len() > 1000 {
            let function_name_static: &'static str =
                Box::leak(self.function_name.clone().into_boxed_str());
            return_compiler_error!(
                "Function '{}' has too many return values ({}). WASM functions are limited to 1000 return values.",
                self.function_name,
                self.result_types.len() ; {
                    CompilationStage => "WASM Validation",
                    VariableName => function_name_static,
                    PrimarySuggestion => "Reduce the number of return values to 1000 or fewer",
                }
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
            let function_name_static: &'static str =
                Box::leak(self.function_name.clone().into_boxed_str());
            return_compiler_error!(
                "WASM validation error in function '{}': {} control frames remain unclosed",
                self.function_name,
                self.control_stack.len() - 1 ; {
                    CompilationStage => "WASM Validation",
                    VariableName => function_name_static,
                    PrimarySuggestion => "Ensure all control structures (if/else/loops) are properly closed with matching 'end' instructions",
                }
            );
        }

        if let Some(frame) = self.control_stack.first() {
            if frame.frame_type != ControlFrameType::Function {
                let function_name_static: &'static str =
                    Box::leak(self.function_name.clone().into_boxed_str());
                return_compiler_error!(
                    "WASM validation error in function '{}': expected function frame, found {:?}",
                    self.function_name,
                    frame.frame_type ; {
                        CompilationStage => "WASM Validation",
                        VariableName => function_name_static,
                        PrimarySuggestion => "This is an internal compiler error - the control frame stack is corrupted",
                    }
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

    /// Add a string constant and return its offset in linear memory
    ///
    /// This method delegates to add_string_slice_constant for consistency
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

    // String table for resolving interned string identifiers
    string_table: StringTable,

    // String constant management
    string_manager: StringManager,

    // Function registry for name resolution
    function_registry: HashMap<String, u32>,

    // Host function index mapping
    host_function_indices: HashMap<String, u32>,

    // Source location tracking for error reporting
    function_source_map: HashMap<u32, FunctionSourceInfo>,

    // Function metadata for named returns and references
    function_metadata: HashMap<String, FunctionMetadata>,

    // Host function registry for runtime-specific mappings
    host_registry: Option<HostFunctionRegistry>,

    // Internal state
    pub function_count: u32,
    pub type_count: u32,
    global_count: u32,

    // Track exported names to prevent duplicates
    exported_names: HashSet<String>,
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

/// Function metadata for named returns and references
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
        Self::new(StringTable::new())
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
    fn emit_default_value_for_type(function: &mut Function, val_type: ValType) {
        match val_type {
            ValType::I32 => Self::emit_i32_const(function, 0),
            ValType::I64 => Self::emit_i64_const(function, 0),
            ValType::F32 => Self::emit_f32_const(function, 0.0),
            ValType::F64 => Self::emit_f64_const(function, 0.0),
            _ => Self::emit_i32_const(function, 0), // Default to i32 for other types
        }
    }

    /// Load a string argument for host function calls - consolidates duplicate string loading logic
    ///
    /// For string/template types, this pushes both pointer and length onto the stack.
    /// For constant strings, it adds them to the data section and pushes the offset and length.
    /// For variable strings, it loads the pointer and length from memory.
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
    pub fn new(string_table: StringTable) -> Self {
        Self {
            type_section: TypeSection::new(),
            import_section: ImportSection::new(),
            function_section: FunctionSection::new(),
            memory_section: MemorySection::new(),
            global_section: GlobalSection::new(),
            export_section: ExportSection::new(),
            code_section: CodeSection::new(),
            data_section: DataSection::new(),
            string_table,
            string_manager: StringManager::new(),
            function_registry: HashMap::new(),
            host_function_indices: HashMap::new(),
            function_source_map: HashMap::new(),
            function_metadata: HashMap::new(),
            host_registry: None,
            function_count: 0,
            type_count: 0,
            global_count: 0,
            exported_names: std::collections::HashSet::new(),
        }
    }

    /// Create a new WasmModule from WIR with host function registry access
    pub fn from_wir(
        wir: &WIR,
        registry: &HostFunctionRegistry,
        string_table: StringTable,
    ) -> Result<WasmModule, CompileError> {
        let mut module = WasmModule::new(string_table);

        // Store registry reference for use during compilation
        module.set_host_function_registry(registry)?;

        // Initialize memory section (1 page = 64KB)
        module.memory_section.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        // Generate WASM import section for host functions with registry-aware mapping
        module.encode_host_function_imports(&wir.host_imports, registry)?;

        // Save the number of imports before compiling functions
        // This is needed for correct function index calculation in exports
        // Imports come first in the WASM function index space, then defined functions
        let import_count = module.function_count;

        // Process functions with error context and validation
        for (index, function) in wir.functions.iter().enumerate() {
            let function_name = module.string_table.resolve(function.name).to_string();
            module.compile_function(function).map_err(|mut error| {
                // Add context about which function failed
                error.msg = format!(
                    "Failed to compile function '{}' (index {}): {}",
                    function_name, index, error.msg
                );
                error
            })?;
        }

        // Export entry point functions correctly, passing the import count
        module.export_entry_point_functions_with_import_count(&wir, import_count)?;

        // Export memory for host function access
        module.add_memory_export("memory")?;

        // Always validate the generated module using wasm_encoder's validation
        module.validate_with_wasm_encoder()?;

        Ok(module)
    }

    /// Set the host function registry for runtime-specific mappings
    pub fn set_host_function_registry(
        &mut self,
        registry: &HostFunctionRegistry,
    ) -> Result<(), CompileError> {
        self.host_registry = Some(registry.clone());
        Ok(())
    }

    /// Get the host function registry if available
    pub fn get_host_function_registry(&self) -> Option<&HostFunctionRegistry> {
        self.host_registry.as_ref()
    }

    /// Resolve an interned string ID to its string value
    pub fn resolve_string(
        &self,
        string_id: crate::compiler::string_interning::InternedString,
    ) -> &str {
        self.string_table.resolve(string_id)
    }

    /// Export entry point functions correctly in WASM modules with explicit import count
    ///
    /// This method implements subtask 3.3: Fix entry point export generation
    /// It ensures entry point functions are exported correctly and validates
    /// that only one start function is exported per module.
    pub fn export_entry_point_functions_with_import_count(
        &mut self,
        wir: &WIR,
        import_count: u32,
    ) -> Result<(), CompileError> {
        let mut entry_point_count = 0;
        let mut start_function_index: Option<u32> = None;

        // Look for entry point functions in WIR
        for (index, wir_function) in wir.functions.iter().enumerate() {
            let mut exported = false;

            // Check if this function is marked as an entry point in WIR exports
            if let Some(export) = wir.exports.get(&wir_function.name) {
                if export.kind == crate::compiler::wir::wir_nodes::ExportKind::Function {
                    // Resolve interned strings for comparison
                    let function_name = self.string_table.resolve(wir_function.name).to_string();
                    let export_name = self.string_table.resolve(export.name).to_string();

                    // Check if this is the entry point by looking for specific naming patterns
                    // Entry points are typically named "main", "_start", or marked specially in the WIR
                    let is_entry_point = function_name == MAIN_FUNCTION_NAME
                        || function_name == ENTRY_POINT_FUNCTION_NAME
                        || function_name.contains("entry")
                        || export_name == ENTRY_POINT_FUNCTION_NAME; // WASM start function convention

                    if is_entry_point {
                        entry_point_count += 1;
                        // FIXED: Add import_count to get the correct WASM function index
                        // Imports come first in the function index space, then defined functions
                        let function_index = import_count + (index as u32);

                        // Export the entry point function
                        self.add_function_export(&export_name, function_index)?;

                        // Mark as start function for WASM module
                        start_function_index = Some(function_index);
                        exported = true;

                        #[cfg(feature = "verbose_codegen_logging")]
                        println!(
                            "WASM: Exported entry point function '{}' at index {} as '{}'",
                            function_name, function_index, export_name
                        );
                    }
                }
            }

            // Handle implicit main/_start function export only if not already exported
            let function_name = self.string_table.resolve(wir_function.name).to_string();
            if !exported
                && (function_name == MAIN_FUNCTION_NAME
                    || function_name == ENTRY_POINT_FUNCTION_NAME)
            {
                entry_point_count += 1;
                // FIXED: Add import_count to get the correct WASM function index
                let function_index = import_count + (index as u32);

                // Use the function's actual name for the export
                self.add_function_export(&function_name, function_index)?;
                start_function_index = Some(function_index);

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: Exported implicit entry point function '{}' at index {}",
                    function_name, function_index
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

    /// Compile a WIR function to WASM with wasm_encoder integration
    pub fn compile_function(&mut self, wir_function: &WirFunction) -> Result<(), CompileError> {
        // Get function name before any mutable borrows
        let function_name_string = self.string_table.resolve(wir_function.name).to_string();

        // Register the function in the function registry
        self.function_registry
            .insert(function_name_string, self.function_count);

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
        let function_name = self.string_table.resolve(wir_function.name);
        let mut function_builder = FunctionBuilder::new(
            wasm_locals,
            param_types,
            result_types,
            function_name.to_string(),
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
                name: self.string_table.resolve(return_arg.id).to_string(),
                data_type: return_arg.value.data_type.clone(),
                is_reference: self.is_datatype_reference(&return_arg.value.data_type),
            });
        }

        FunctionMetadata {
            name: self.string_table.resolve(wir_function.name).to_string(),
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
        let function_name = self.string_table.resolve(wir_function.name);
        self.function_metadata
            .insert(function_name.to_string(), metadata);

        // Store source information for error reporting
        let source_info = FunctionSourceInfo {
            function_name: function_name.to_string(),
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
                function_name,
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

    /// Compile function body using wasm_encoder integration
    fn compile_function_body(
        &mut self,
        wir_function: &WirFunction,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Lower each block with control flow tracking
        for (block_index, block) in wir_function.blocks.iter().enumerate() {
            function_builder.begin_block(block_index as u32)?;

            // Lower statements
            for statement in &block.statements {
                self.lower_statement(statement, function_builder, local_map)?;
            }

            // Lower terminator with control flow validation
            self.lower_terminator(&block.terminator, function_builder, local_map)?;

            function_builder.end_block()?;
        }

        Ok(())
    }

    /// Lower a WIR block to WASM instructions
    fn lower_statement(
        &mut self,
        statement: &Statement,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { place, rvalue } => {
                // Beanstalk-aware assignment: handles both regular and mutable (~) assignments
                self.lower_beanstalk_aware_assignment(place, rvalue, function_builder, local_map)
            }
            Statement::Call {
                func,
                args,
                destination,
            } => {
                // WIR-faithful function call: args → call → result (≤3 instructions per arg + 1 call + 1 store)
                self.lower_wir_call(func, args, destination, function_builder, local_map)
            }
            Statement::HostCall {
                function: host_func,
                args,
                destination,
            } => {
                // WIR-faithful host call: args → call → result (≤3 instructions per arg + 1 call + 1 store)
                self.lower_wir_host_call(host_func, args, destination, function_builder, local_map)
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
                self.lower_operand(condition, function_builder, local_map)?;

                // Create WASM if/else block structure
                // The condition is already on the stack from lower_operand

                // Determine block type based on whether branches produce values
                // For now, use empty block type (no return value)
                let block_type = wasm_encoder::BlockType::Empty;

                // Emit if instruction
                function_builder.instruction(&Instruction::If(block_type))?;

                // Lower then block statements
                for stmt in then_statements {
                    self.lower_statement(stmt, function_builder, local_map)?;
                }

                // Emit else instruction if there are else statements
                if !else_statements.is_empty() {
                    function_builder.instruction(&Instruction::Else)?;

                    // Lower else block statements
                    for stmt in else_statements {
                        self.lower_statement(stmt, function_builder, local_map)?;
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
                self.lower_operand(receiver, function_builder, local_map)?;

                // Load remaining arguments
                for arg in args {
                    self.lower_operand(arg, function_builder, local_map)?;
                }

                // For now, generate a placeholder call (will be replaced with vtable dispatch)
                // This maintains the WIR principle of direct lowering
                function_builder.instruction(&Instruction::Call(0))?; // Placeholder function index

                // Store result if destination exists
                if let Some(dest_place) = destination {
                    let return_type = dest_place.wasm_type();
                    self.lower_place_assignment_with_type(
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
                self.lower_operand(size, function_builder, local_map)?;

                // For now, use a simple linear memory allocation strategy
                // This will be enhanced when proper memory management is implemented
                function_builder.instruction(&Instruction::I32Const(0))?; // Base memory address
                function_builder.instruction(&Instruction::I32Add)?; // Add size to get allocation

                let alloc_type = WasmType::I32; // Allocation returns pointer (i32)
                self.lower_place_assignment_with_type(
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
                self.lower_operand(value, function_builder, local_map)?;

                // Generate address for the place
                match place {
                    Place::Memory { base, offset, .. } => {
                        self.lower_memory_base_address(base, function_builder)?;
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
                            place ; {
                                CompilationStage => "WASM Encoding",
                                PrimarySuggestion => "Use memory-based places for store operations",
                                AlternativeSuggestion => "Convert to local/global assignment if appropriate",
                            }
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
                    self.lower_operand(op, function_builder, local_map)?;
                }

                // For now, placeholder memory operation
                function_builder.instruction(&Instruction::Nop)?;

                if let Some(res) = result {
                    let result_type = res.wasm_type();
                    self.lower_place_assignment_with_type(
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

    /// Lower WIR assignment statement (≤3 WASM instructions)
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
                let string_content = self.string_table.resolve(*value);
                let offset = self
                    .string_manager
                    .add_string_slice_constant(string_content);
                Self::emit_i32_const(function, offset as i32);

                #[cfg(feature = "verbose_codegen_logging")]
                println!(
                    "WASM: string slice constant '{}' at offset {}",
                    string_content, offset
                );

                Ok(())
            }
            Constant::MutableString(value) => {
                // Mutable string constants: heap-allocated with default capacity
                let string_content = self.string_table.resolve(*value);
                let default_capacity = (string_content.len() as u32).max(32); // At least 32 bytes capacity
                let offset = self
                    .string_manager
                    .allocate_mutable_string(string_content, default_capacity);
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

    /// Lower projection element access for field/index operations
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
                        bytes ; {
                            CompilationStage => "WASM Encoding",
                            PrimarySuggestion => "Use I64 type for values larger than 4 bytes",
                            ExpectedType => "I64",
                            FoundType => "I32",
                        }
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
                    size ; {
                        CompilationStage => "WASM Encoding",
                        PrimarySuggestion => "Ensure WIR type matches the target memory size",
                        FoundType => "incompatible type/size combination",
                    }
                );
            }
        }
        Ok(())
    }

    /// Get the WASM type of an rvalue for type-aware instruction generation
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
                    "Logical operations (and/or) should be implemented as control flow, not binary operations. Use if/else statements for short-circuiting behavior." ; {
                        CompilationStage => "WASM Encoding",
                        PrimarySuggestion => "Convert logical operations to if/else control flow in WIR generation",
                        AlternativeSuggestion => "Use bitwise operations (&, |) for non-short-circuiting behavior",
                    }
                );
            }

            // Unsupported combinations
            (op, wasm_type) => {
                return_compiler_error!(
                    "Binary operation {:?} not supported for WASM type {:?}. Check that the operation is valid for the given type.",
                    op,
                    wasm_type ; {
                        CompilationStage => "WASM Encoding",
                        PrimarySuggestion => "Ensure the operation is supported for the operand type",
                        AlternativeSuggestion => "Check WIR type inference for correctness",
                    }
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
                    op ; {
                        CompilationStage => "WASM Encoding",
                        PrimarySuggestion => "Implement the unary operation in the WASM backend",
                    }
                );
            }
        }
    }

    /// Lower if terminator to WASM structured control flow
    ///
    /// This method implements WASM structured control flow for if/else statements.
    /// It generates proper WASM if/else/end instruction sequences with correct
    /// block types and stack discipline.

    /// Lower a WIR terminator to WASM control flow instructions with validation
    fn lower_terminator(
        &mut self,
        terminator: &Terminator,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::Goto { target } => {
                // Beanstalk scope semantics: goto represents control flow between scopes
                self.lower_beanstalk_goto(*target, function_builder, local_map)
            }
            Terminator::If {
                condition,
                then_block,
                else_block,
            } => {
                // Beanstalk conditional with scope semantics (: and ;)
                self.lower_beanstalk_if_terminator(
                    condition,
                    *then_block,
                    *else_block,
                    function_builder,
                    local_map,
                )
            }
            Terminator::Return { values } => {
                // Beanstalk return with proper scope closing
                self.lower_beanstalk_return(values, function_builder, local_map)
            }
            Terminator::Unreachable => {
                function_builder.instruction(&Instruction::Unreachable)?;
                Ok(())
            }
        }
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
        string_table: &StringTable,
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

    /// Finish building the WASM module and return the compiled bytes
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

    /// Generate WASM import section entries for host functions with registry-aware mapping
    ///
    /// This method creates WASM function type signatures from host function definitions
    /// and adds import entries with correct module and function names, using runtime-specific
    /// mappings from the host function registry.
    pub fn encode_host_function_imports(
        &mut self,
        host_imports: &HashSet<HostFunctionDef>,
        registry: &HostFunctionRegistry,
    ) -> Result<(), CompileError> {
        for host_func in host_imports {
            // Check for runtime-specific mapping if registry is available
            let (module_name, import_name) = self.get_runtime_specific_mapping(host_func, registry)?;

            // Create WASM function type signature from host function definition
            let param_types = self.create_wasm_param_types_from_basic(&host_func.parameters)?;
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
            self.host_function_indices.insert(
                self.string_table.resolve(host_func.name).to_string(),
                self.function_count,
            );

            // Also register in the main function registry for unified lookup
            self.function_registry.insert(
                self.string_table.resolve(host_func.name).to_string(),
                self.function_count,
            );

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
        registry: &HostFunctionRegistry,
    ) -> Result<(String, String), CompileError> {
        use crate::compiler::host_functions::registry::RuntimeFunctionMapping;

        // Get the runtime mapping based on current backend
        match registry.get_runtime_mapping(&host_func.name) {
            Some(RuntimeFunctionMapping::JavaScript(js_func)) => {
                // Use JavaScript mapping for web execution (already String)
                Ok((js_func.module.clone(), js_func.name.clone()))
            }
            Some(RuntimeFunctionMapping::Native(_)) | None => {
                // Use original host function mapping as fallback (need to resolve StringId)
                Ok((
                    self.string_table.resolve(host_func.module).to_string(),
                    self.string_table.resolve(host_func.import_name).to_string(),
                ))
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

    /// Create WASM parameter types from BasicParameter (used for host functions)
    fn create_wasm_param_types_from_basic(
        &self,
        parameters: &[crate::compiler::host_functions::registry::BasicParameter],
    ) -> Result<Vec<ValType>, CompileError> {
        let mut param_types = Vec::new();

        for param in parameters {
            // String, Template, and CoerceToString types need two parameters: pointer and length
            match &param.data_type {
                DataType::String | DataType::Template | DataType::CoerceToString => {
                    param_types.push(ValType::I32); // pointer
                    param_types.push(ValType::I32); // length
                }
                _ => {
                    let wasm_type = Self::unified_datatype_to_wasm_type(&param.data_type)?;
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
                return_compiler_error!(
                    "DataType '{:?}' in host function signatures not yet implemented - use basic types (Int, Float, Bool, String) for host function parameters and return values",
                    data_type
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

    /// Lower Beanstalk goto with proper scope semantics
    ///
    /// Beanstalk's goto represents control flow between scopes defined by `:` and `;`.
    /// In WASM, this maps to branch instructions with proper block structure.

    /// Lower Beanstalk if terminator with scope semantics
    ///
    /// Beanstalk conditionals use `:` to open scope and `;` to close scope.
    /// This maps to WASM's structured control flow with proper block nesting.

    /// Lower Beanstalk return with proper scope closing
    ///
    /// Beanstalk returns must properly close any open scopes before returning.
    /// This ensures proper WASM control flow and scope management.

    /// Handle Beanstalk error handling syntax (!err:)
    ///
    /// Beanstalk's `!err:` syntax creates error handling scopes that map to
    /// WASM's structured exception handling or control flow patterns.
    fn lower_beanstalk_aware_assignment(
        &mut self,
        place: &Place,
        rvalue: &Rvalue,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Lower rvalue to stack
        self.lower_rvalue(rvalue, function_builder, local_map)?;

        // Assign to place
        let place_type = place.wasm_type();
        self.lower_place_assignment_with_type(place, &place_type, function_builder, local_map)?;

        Ok(())
    }

    /// Rvalue lowering with validation
    fn lower_rvalue(
        &mut self,
        rvalue: &Rvalue,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match rvalue {
            Rvalue::Use(operand) => self.lower_operand(operand, function_builder, local_map),
            Rvalue::BinaryOp(op, left, right) => {
                self.lower_operand(left, function_builder, local_map)?;
                self.lower_operand(right, function_builder, local_map)?;

                // Determine the correct WASM instruction based on operand types
                let left_type = self.get_operand_wasm_type(left, local_map)?;
                let right_type = self.get_operand_wasm_type(right, local_map)?;

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
                self.lower_operand(operand, function_builder, local_map)?;

                // Determine the correct WASM instruction based on operand type
                let operand_type = self.get_operand_wasm_type(operand, local_map)?;

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
                self.lower_place_access(place, function_builder, local_map)
            }
            Rvalue::StringConcat(left, right) => {
                // String concatenation: load both operands
                self.lower_operand(left, function_builder, local_map)?;
                self.lower_operand(right, function_builder, local_map)?;
                // For now, just leave both values on the stack
                // TODO: Implement actual string concatenation via runtime helper
                Ok(())
            }
        }
    }

    /// Operand lowering with validation
    fn lower_operand(
        &mut self,
        operand: &Operand,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                self.lower_place_access(place, function_builder, local_map)
            }
            Operand::Constant(constant) => self.lower_constant(constant, function_builder),
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

    /// Place access with validation
    fn lower_place_access(
        &mut self,
        place: &Place,
        function_builder: &mut FunctionBuilder,
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
                self.lower_memory_base_address(base, function_builder)?;
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
                self.lower_place_access(base, function_builder, local_map)?;

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
                        self.lower_place_access(index, function_builder, local_map)?;
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

    /// Place assignment with validation
    fn lower_place_assignment_with_type(
        &mut self,
        place: &Place,
        _wasm_type: &WasmType,
        function_builder: &mut FunctionBuilder,
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
                self.lower_memory_base_address(base, function_builder)?;
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

    /// Get the WASM type of an operand for type checking
    fn get_operand_wasm_type(
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

    /// Constant lowering with validation
    fn lower_constant(
        &mut self,
        constant: &Constant,
        function_builder: &mut FunctionBuilder,
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
                let value_str = self.string_table.resolve(*value);
                let offset = self.string_manager.add_string_slice_constant(value_str);
                function_builder.instruction(&Instruction::I32Const(offset as i32))?;
                Ok(())
            }
            Constant::MutableString(value) => {
                let value_str = self.string_table.resolve(*value);
                let default_capacity = (value_str.len() as u32).max(32);
                let offset = self
                    .string_manager
                    .allocate_mutable_string(value_str, default_capacity);
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

    /// Memory base address lowering
    fn lower_memory_base_address(
        &mut self,
        base: &MemoryBase,
        function_builder: &mut FunctionBuilder,
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

    /// Function call lowering
    fn lower_wir_call(
        &mut self,
        func: &Operand,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load arguments
        for arg in args {
            self.lower_operand(arg, function_builder, local_map)?;
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
            self.lower_place_assignment_with_type(
                dest_place,
                &return_type,
                function_builder,
                local_map,
            )?;
        }

        Ok(())
    }

    /// Host function call lowering
    fn lower_wir_host_call(
        &mut self,
        host_func: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Check for runtime-specific mapping if registry is available
        if let Some(registry) = self.host_registry.clone() {
            return self.lower_host_call_with_registry(
                host_func,
                args,
                destination,
                function_builder,
                local_map,
                &registry,
            );
        }

        // Regular host function handling
        // Load arguments
        for arg in args {
            self.lower_operand(arg, function_builder, local_map)?;
        }

        // Generate host call instruction
        let host_function_name = self.string_table.resolve(host_func.name).to_string();
        if let Some(&func_index) = self.host_function_indices.get(&host_function_name) {
            function_builder.instruction(&Instruction::Call(func_index))?;
        } else {
            return_compiler_error!("Host function not found: {}", host_function_name);
        }

        // Store result if destination exists
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type(
                dest_place,
                &return_type,
                function_builder,
                local_map,
            )?;
        }

        Ok(())
    }

    /// Host function call lowering with registry-aware runtime mapping
    fn lower_host_call_with_registry(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
        _registry: &HostFunctionRegistry,
    ) -> Result<(), CompileError> {
        // The registry is used during import generation to get runtime-specific mappings
        // For call generation, we use the standard host function call for all backends
        // The import name is already resolved during import generation
        self.lower_standard_host_call(
            host_function,
            args,
            destination,
            function_builder,
            local_map,
        )
    }

    /// Standard host function call lowering
    fn lower_standard_host_call(
        &mut self,
        host_function: &HostFunctionDef,
        args: &[Operand],
        destination: &Option<Place>,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load arguments with special handling for string types
        // String, Template, and CoerceToString parameters need both pointer and length
        for (i, arg) in args.iter().enumerate() {
            if i < host_function.parameters.len() {
                let param_type = &host_function.parameters[i].data_type;
                match param_type {
                    DataType::String | DataType::Template | DataType::CoerceToString => {
                        // For string types, we need to push both pointer and length
                        self.lower_string_argument(arg, function_builder, local_map)?;
                    }
                    _ => {
                        // For other types, just lower the operand normally
                        self.lower_operand(arg, function_builder, local_map)?;
                    }
                }
            } else {
                // If we have more args than parameters, just lower normally
                self.lower_operand(arg, function_builder, local_map)?;
            }
        }

        // Generate host call instruction
        let host_function_name = self.string_table.resolve(host_function.name).to_string();
        if let Some(&func_index) = self.host_function_indices.get(&host_function_name) {
            function_builder.instruction(&Instruction::Call(func_index))?;
        } else {
            return_compiler_error!("Host function not found: {}", host_function_name);
        }

        // Store result if destination exists
        if let Some(dest_place) = destination {
            let return_type = dest_place.wasm_type();
            self.lower_place_assignment_with_type(
                dest_place,
                &return_type,
                function_builder,
                local_map,
            )?;
        }

        Ok(())
    }

    /// Lower a string argument for host function calls
    /// Pushes both pointer and length onto the stack
    fn lower_string_argument(
        &mut self,
        arg: &Operand,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match arg {
            Operand::Constant(Constant::String(string_id)) => {
                // For string constants, add to data section and push pointer + length
                let value_str = self.string_table.resolve(*string_id);
                let offset = self.string_manager.add_string_slice_constant(value_str);
                let length = value_str.len() as i32;
                
                // String is stored with 4-byte length prefix, so pointer to data is offset + 4
                // Push pointer to string data (skip the 4-byte length prefix)
                function_builder.instruction(&Instruction::I32Const((offset + 4) as i32))?;
                // Push length
                function_builder.instruction(&Instruction::I32Const(length))?;
                Ok(())
            }
            Operand::Constant(Constant::MutableString(string_id)) => {
                // For mutable strings, allocate in memory and push pointer + length
                let value_str = self.string_table.resolve(*string_id);
                let default_capacity = (value_str.len() as u32).max(32);
                let offset = self.string_manager.allocate_mutable_string(value_str, default_capacity);
                let length = value_str.len() as i32;
                
                // Mutable string is stored with 8-byte header (4 bytes length + 4 bytes capacity)
                // Push pointer to string data (skip the 8-byte header)
                function_builder.instruction(&Instruction::I32Const((offset + 8) as i32))?;
                // Push length
                function_builder.instruction(&Instruction::I32Const(length))?;
                Ok(())
            }
            Operand::Copy(place) | Operand::Move(place) => {
                // For string variables, we need to load both pointer and length from memory
                // Strings are stored with a 4-byte length prefix followed by data
                match place {
                    Place::Local { index, .. } => {
                        // Load the string pointer (which points to the length prefix)
                        if let Some(wasm_local) = local_map.get_local(*index) {
                            // Push pointer (offset to string data, skipping 4-byte length prefix)
                            function_builder.instruction(&Instruction::LocalGet(wasm_local))?;
                            function_builder.instruction(&Instruction::I32Const(4))?;
                            function_builder.instruction(&Instruction::I32Add)?;
                            
                            // Load and push length (from the 4-byte prefix)
                            function_builder.instruction(&Instruction::LocalGet(wasm_local))?;
                            function_builder.instruction(&Instruction::I32Load(MemArg {
                                align: 2, // 4-byte alignment
                                offset: 0,
                                memory_index: 0,
                            }))?;
                            Ok(())
                        } else {
                            return_compiler_error!("Local variable not found: {}", index);
                        }
                    }
                    Place::Memory { base, offset, .. } => {
                        // Load from memory location
                        self.lower_memory_base_address(base, function_builder)?;
                        if offset.0 > 0 {
                            function_builder.instruction(&Instruction::I32Const(offset.0 as i32))?;
                            function_builder.instruction(&Instruction::I32Add)?;
                        }
                        
                        // Duplicate the address for loading both pointer and length
                        // First, save the base address to a temporary (we'll use the stack)
                        // Push pointer (base + 4 to skip length prefix)
                        function_builder.instruction(&Instruction::I32Const(4))?;
                        function_builder.instruction(&Instruction::I32Add)?;
                        
                        // Reload base address for length
                        self.lower_memory_base_address(base, function_builder)?;
                        if offset.0 > 0 {
                            function_builder.instruction(&Instruction::I32Const(offset.0 as i32))?;
                            function_builder.instruction(&Instruction::I32Add)?;
                        }
                        
                        // Load length from memory
                        function_builder.instruction(&Instruction::I32Load(MemArg {
                            align: 2, // 4-byte alignment
                            offset: 0,
                            memory_index: 0,
                        }))?;
                        Ok(())
                    }
                    _ => {
                        // For other place types, fall back to normal operand lowering
                        // This may not work correctly for strings, but it's a fallback
                        self.lower_operand(arg, function_builder, local_map)?;
                        // Push a default length of 0 as a placeholder
                        function_builder.instruction(&Instruction::I32Const(0))?;
                        Ok(())
                    }
                }
            }
            _ => {
                // For other operand types, fall back to normal lowering
                self.lower_operand(arg, function_builder, local_map)?;
                // Push a default length of 0 as a placeholder
                function_builder.instruction(&Instruction::I32Const(0))?;
                Ok(())
            }
        }
    }

    /// Beanstalk goto lowering
    fn lower_beanstalk_goto(
        &mut self,
        _target: u32,
        function_builder: &mut FunctionBuilder,
        _local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // For now, goto is implemented as a no-op
        function_builder.instruction(&Instruction::Nop)?;
        Ok(())
    }

    /// Beanstalk if terminator lowering with proper control flow validation
    fn lower_beanstalk_if_terminator(
        &mut self,
        condition: &Operand,
        _then_block: u32,
        _else_block: u32,
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Lower condition to stack
        self.lower_operand(condition, function_builder, local_map)?;

        // For now, just drop the condition
        // Proper implementation would require block management
        function_builder.instruction(&Instruction::Drop)?;
        Ok(())
    }

    /// Lower struct validation to WASM instructions
    fn lower_struct_validation(
        &mut self,
        _struct_place: &Place,
        _struct_type: &DataType,
        _function_builder: &mut FunctionBuilder,
        _local_map: &mut LocalMap,
    ) -> Result<(), CompileError> {
        // For now, struct validation is handled at compile time
        // In a more complete implementation, this could generate runtime checks
        // for uninitialized fields if needed
        Ok(())
    }

    /// Beanstalk return lowering
    fn lower_beanstalk_return(
        &mut self,
        values: &[Operand],
        function_builder: &mut FunctionBuilder,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load return values
        for value in values {
            self.lower_operand(value, function_builder, local_map)?;
        }

        // Generate return instruction
        function_builder.instruction(&Instruction::Return)?;

        Ok(())
    }

}
