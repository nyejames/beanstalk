use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::mir_nodes::{
    BinOp, Constant, MIR, MirFunction, Operand, Rvalue, Statement, Terminator, UnOp,
};
use crate::compiler::mir::place::{Place, WasmType};
use crate::return_compiler_error;
use std::collections::HashMap;
use wasm_encoder::*;

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

    /// Add a string constant and return its offset in linear memory
    /// 
    /// Strings are stored with a 4-byte length prefix followed by UTF-8 data.
    /// Identical strings are deduplicated and return the same offset.
    /// 
    /// ## Memory Management
    /// String constants have static lifetime and are stored in the WASM data section.
    /// No drop semantics are needed since they persist for the entire program execution.
    /// This is appropriate for basic string literals - dynamic string allocation
    /// would require more complex lifetime tracking.
    pub fn add_string_constant(&mut self, value: &str) -> u32 {
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

/// Simplified WASM module for basic MIR-to-WASM compilation
pub struct WasmModule {
    type_section: TypeSection,
    function_section: FunctionSection,
    memory_section: MemorySection,
    global_section: GlobalSection,
    export_section: ExportSection,
    code_section: CodeSection,
    data_section: DataSection,

    // String constant management
    string_manager: StringManager,

    // Internal state
    pub function_count: u32,
    pub type_count: u32,
    global_count: u32,
}

impl Default for WasmModule {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmModule {
    pub fn new() -> Self {
        Self {
            type_section: TypeSection::new(),
            function_section: FunctionSection::new(),
            memory_section: MemorySection::new(),
            global_section: GlobalSection::new(),
            export_section: ExportSection::new(),
            code_section: CodeSection::new(),
            data_section: DataSection::new(),
            string_manager: StringManager::new(),
            function_count: 0,
            type_count: 0,
            global_count: 0,
        }
    }

    /// Create a new WasmModule from MIR
    pub fn from_mir(mir: &MIR) -> Result<WasmModule, CompileError> {
        let mut module = WasmModule::new();

        // Initialize memory section (1 page = 64KB)
        module.memory_section.memory(MemoryType {
            minimum: 1,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });

        // Process functions
        for function in &mir.functions {
            module.compile_function(function)?;
        }

        Ok(module)
    }

    /// Compile a MIR function to WASM
    pub fn compile_function(&mut self, mir_function: &MirFunction) -> Result<(), CompileError> {
        // Create function type
        let param_types: Vec<ValType> = mir_function
            .parameters
            .iter()
            .map(|p| self.wasm_type_to_val_type(&p.wasm_type()))
            .collect();

        let result_types: Vec<ValType> = mir_function
            .return_types
            .iter()
            .map(|t| self.wasm_type_to_val_type(t))
            .collect();

        self.type_section.ty().function(param_types, result_types);

        // Add function to function section
        self.function_section.function(self.type_count);

        // Create function body
        let mut function = Function::new(vec![]); // No locals for now
        let local_map = HashMap::new(); // Empty local map for now

        // Lower each block
        for block in &mir_function.blocks {
            self.lower_block_to_wasm(block, &mut function, &local_map)?;
        }

        // Add function to code section
        self.code_section.function(&function);

        self.function_count += 1;
        self.type_count += 1;

        Ok(())
    }

    /// Lower a MIR block to WASM instructions
    fn lower_block_to_wasm(
        &mut self,
        block: &crate::compiler::mir::mir_nodes::MirBlock,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Lower each statement
        for statement in &block.statements {
            self.lower_statement(statement, function, local_map)?;
        }

        // Lower the terminator
        self.lower_terminator(&block.terminator, function, local_map)?;

        Ok(())
    }

    /// Lower a MIR statement to WASM instructions
    fn lower_statement(
        &mut self,
        statement: &Statement,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { place: _, rvalue } => {
                // For now, just lower the rvalue and drop the result
                self.lower_rvalue(rvalue, function, local_map)?;
                function.instruction(&Instruction::Drop);
                Ok(())
            }
            Statement::Call {
                func: _,
                args: _,
                destination: _,
            } => {
                // Function calls not yet implemented
                return_compiler_error!(
                    "Function calls not yet implemented in simplified WASM backend"
                );
            }
            Statement::Nop => {
                // No-op - generate no instructions
                Ok(())
            }
            _ => {
                return_compiler_error!(
                    "Statement type not yet implemented in simplified WASM backend"
                );
            }
        }
    }

    /// Lower a MIR rvalue to WASM instructions
    fn lower_rvalue(
        &mut self,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match rvalue {
            Rvalue::Use(operand) => self.lower_operand(operand, function, local_map),
            Rvalue::BinaryOp(op, left, right) => {
                self.lower_operand(left, function, local_map)?;
                self.lower_operand(right, function, local_map)?;
                self.lower_binary_op(op, function)
            }
            Rvalue::UnaryOp(op, operand) => {
                self.lower_operand(operand, function, local_map)?;
                self.lower_unary_op(op, function)
            }
            Rvalue::Ref {
                place: _,
                borrow_kind: _,
            } => {
                // References not yet implemented
                return_compiler_error!("References not yet implemented in simplified WASM backend");
            }
        }
    }

    /// Lower a MIR operand to WASM instructions
    fn lower_operand(
        &mut self,
        operand: &Operand,
        function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match operand {
            Operand::Constant(constant) => self.lower_constant(constant, function),
            Operand::Copy(_place) | Operand::Move(_place) => {
                // Place operations not yet implemented
                return_compiler_error!(
                    "Place operations not yet implemented in simplified WASM backend"
                );
            }
            Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
                // References not yet implemented
                return_compiler_error!("References not yet implemented in simplified WASM backend");
            }
        }
    }

    /// Lower a constant to WASM instructions
    fn lower_constant(
        &mut self,
        constant: &Constant,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match constant {
            Constant::I32(value) => {
                function.instruction(&Instruction::I32Const(*value));
                Ok(())
            }
            Constant::I64(value) => {
                function.instruction(&Instruction::I64Const(*value));
                Ok(())
            }
            Constant::F32(value) => {
                function.instruction(&Instruction::F32Const((*value).into()));
                Ok(())
            }
            Constant::F64(value) => {
                function.instruction(&Instruction::F64Const((*value).into()));
                Ok(())
            }
            Constant::Bool(value) => {
                function.instruction(&Instruction::I32Const(if *value { 1 } else { 0 }));
                Ok(())
            }
            Constant::String(value) => {
                // Add string to string manager and get offset
                let offset = self.string_manager.add_string_constant(value);
                // Generate i32.const with pointer to string data in linear memory
                function.instruction(&Instruction::I32Const(offset as i32));
                Ok(())
            }
            Constant::Null => {
                // Null pointer is 0 in linear memory
                function.instruction(&Instruction::I32Const(0));
                Ok(())
            }
            Constant::Function(func_index) => {
                // Function reference as index
                function.instruction(&Instruction::I32Const(*func_index as i32));
                Ok(())
            }
            Constant::MemoryOffset(offset) => {
                // Memory offset constant
                function.instruction(&Instruction::I32Const(*offset as i32));
                Ok(())
            }
            Constant::TypeSize(size) => {
                // Type size constant
                function.instruction(&Instruction::I32Const(*size as i32));
                Ok(())
            }
        }
    }

    /// Lower binary operations to WASM instructions
    fn lower_binary_op(&self, op: &BinOp, function: &mut Function) -> Result<(), CompileError> {
        match op {
            BinOp::Add => {
                function.instruction(&Instruction::I32Add);
                Ok(())
            }
            BinOp::Sub => {
                function.instruction(&Instruction::I32Sub);
                Ok(())
            }
            BinOp::Mul => {
                function.instruction(&Instruction::I32Mul);
                Ok(())
            }
            _ => {
                return_compiler_error!(
                    "Binary operation not yet implemented in simplified WASM backend: {:?}",
                    op
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

    /// Lower a MIR terminator to WASM control flow instructions
    pub fn lower_terminator(
        &mut self,
        terminator: &Terminator,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::Goto { target: _ } => {
                // Simple goto - for now just continue
                Ok(())
            }
            Terminator::If {
                condition,
                then_block: _,
                else_block: _,
            } => {
                // Load condition and generate if
                self.lower_operand(condition, function, local_map)?;
                function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
                function.instruction(&Instruction::End);
                Ok(())
            }
            Terminator::Return { values } => {
                // Load return values
                for value in values {
                    self.lower_operand(value, function, local_map)?;
                }
                function.instruction(&Instruction::Return);
                Ok(())
            }
            Terminator::Unreachable => {
                function.instruction(&Instruction::Unreachable);
                Ok(())
            }
        }
    }

    /// Generate string constant WASM instructions
    /// 
    /// Returns an i32.const instruction with the offset to the string data in linear memory
    fn generate_string_constant(&mut self, value: &str) -> Result<(), CompileError> {
        let _offset = self.string_manager.add_string_constant(value);
        // Return pointer to string data in linear memory as i32
        Ok(())
    }

    /// Convert WasmType to wasm_encoder ValType
    fn wasm_type_to_val_type(&self, wasm_type: &WasmType) -> ValType {
        match wasm_type {
            WasmType::I32 => ValType::I32,
            WasmType::I64 => ValType::I64,
            WasmType::F32 => ValType::F32,
            WasmType::F64 => ValType::F64,
            WasmType::ExternRef => ValType::Ref(RefType::EXTERNREF),
            WasmType::FuncRef => ValType::Ref(RefType::FUNCREF),
        }
    }

    /// Compile a MIR function (alias for compile_function)
    pub fn compile_mir_function(
        &mut self,
        mir_function: &MirFunction,
    ) -> Result<u32, CompileError> {
        let function_index = self.function_count;
        self.compile_function(mir_function)?;
        Ok(function_index)
    }

    /// Add function export (placeholder)
    pub fn add_function_export(
        &mut self,
        _name: &str,
        _function_index: u32,
    ) -> Result<u32, CompileError> {
        // Export functionality will be added when needed
        Ok(_function_index)
    }

    /// Add global export (placeholder)
    pub fn add_global_export(
        &mut self,
        _name: &str,
        _global_index: u32,
    ) -> Result<(), CompileError> {
        // Export functionality will be added when needed
        Ok(())
    }

    /// Add memory export (placeholder)
    pub fn add_memory_export(&mut self, _name: &str) -> Result<(), CompileError> {
        // Export functionality will be added when needed
        Ok(())
    }

    /// Get lifetime memory statistics
    pub fn get_lifetime_memory_statistics(&self) -> LifetimeMemoryStatistics {
        LifetimeMemoryStatistics::default()
    }

    /// Generate the final WASM module bytes
    pub fn finish(mut self) -> Vec<u8> {
        // Populate data section with string constants
        if self.string_manager.get_data_size() > 0 {
            let string_data = self.string_manager.get_data_section();
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(0), // Start at offset 0 in linear memory
                string_data.iter().copied(),
            );
        }

        let mut module = Module::new();

        module.section(&self.type_section);
        module.section(&self.function_section);
        module.section(&self.memory_section);
        module.section(&self.global_section);
        module.section(&self.export_section);
        module.section(&self.code_section);
        module.section(&self.data_section);

        module.finish()
    }
}

/// Placeholder for lifetime memory statistics
#[derive(Debug, Clone)]
pub struct LifetimeMemoryStatistics {
    pub single_ownership_optimizations: usize,
    pub arc_operations_eliminated: usize,
    pub move_optimizations_applied: usize,
    pub drop_operations_optimized: usize,
    pub memory_allocation_reduction: usize,
    pub instruction_count_reduction: usize,
}

impl Default for LifetimeMemoryStatistics {
    fn default() -> Self {
        Self {
            single_ownership_optimizations: 0,
            arc_operations_eliminated: 0,
            move_optimizations_applied: 0,
            drop_operations_optimized: 0,
            memory_allocation_reduction: 0,
            instruction_count_reduction: 0,
        }
    }
}
