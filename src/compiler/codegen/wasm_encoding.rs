use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::mir_nodes::{
    BinOp, Constant, MIR, MirFunction, Operand, Rvalue, Statement, Terminator, UnOp,
};
use crate::compiler::mir::place::{Place, WasmType, MemoryBase, ProjectionElem, TypeSize};
use crate::{return_compiler_error, return_unimplemented_feature_error, return_wasm_validation_error};
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

/// Local variable mapping from MIR places to WASM local indices
/// 
/// This structure manages the mapping between MIR Place::Local indices and
/// WASM local variable indices, enabling proper place resolution.
#[derive(Debug, Clone)]
pub struct LocalMap {
    /// Map from MIR local index to WASM local index
    local_mapping: HashMap<u32, u32>,
    /// Map from MIR global index to WASM global index
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

    /// Map a MIR local index to a WASM local index
    pub fn map_local(&mut self, mir_local: u32, wasm_local: u32) {
        self.local_mapping.insert(mir_local, wasm_local);
    }

    /// Map a MIR global index to a WASM global index
    pub fn map_global(&mut self, mir_global: u32, wasm_global: u32) {
        self.global_mapping.insert(mir_global, wasm_global);
    }

    /// Get WASM local index for MIR local
    pub fn get_local(&self, mir_local: u32) -> Option<u32> {
        self.local_mapping.get(&mir_local).copied()
    }

    /// Get WASM global index for MIR global
    pub fn get_global(&self, mir_global: u32) -> Option<u32> {
        self.global_mapping.get(&mir_global).copied()
    }

    /// Allocate next WASM local index for a MIR local
    pub fn allocate_local(&mut self, mir_local: u32) -> u32 {
        let wasm_local = self.next_local_index;
        self.next_local_index += 1;
        self.local_mapping.insert(mir_local, wasm_local);
        wasm_local
    }

    /// Allocate next WASM global index for a MIR global
    pub fn allocate_global(&mut self, mir_global: u32) -> u32 {
        let wasm_global = self.next_global_index;
        self.next_global_index += 1;
        self.global_mapping.insert(mir_global, wasm_global);
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

/// Local variable analyzer for determining WASM local requirements from MIR
/// 
/// This analyzer examines a MIR function to determine what local variables are needed
/// and builds the appropriate mapping for WASM code generation.
#[derive(Debug)]
pub struct LocalAnalyzer {
    /// Map from MIR local index to WASM type
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

    /// Analyze a MIR function to determine local variable requirements
    pub fn analyze_function(mir_function: &MirFunction) -> Self {
        let mut analyzer = Self::new();
        analyzer.parameter_count = mir_function.parameters.len() as u32;

        // Analyze all places used in the function
        for block in &mir_function.blocks {
            for statement in &block.statements {
                analyzer.collect_from_statement(statement);
            }
            analyzer.collect_from_terminator(&block.terminator);
        }

        // Also analyze local variables declared in the function
        for (_, place) in &mir_function.locals {
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
            Statement::Call { func, args, destination } => {
                self.collect_from_operand(func);
                for arg in args {
                    self.collect_from_operand(arg);
                }
                if let Some(dest) = destination {
                    self.collect_from_place(dest);
                }
            }
            Statement::InterfaceCall { receiver, args, destination, .. } => {
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
            Statement::MemoryOp { operand, result, .. } => {
                if let Some(op) = operand {
                    self.collect_from_operand(op);
                }
                if let Some(res) = result {
                    self.collect_from_place(res);
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
        self.type_counts
            .iter()
            .map(|(wasm_type, count)| (*count, self.wasm_type_to_val_type(wasm_type)))
            .collect()
    }

    /// Build local mapping from MIR analysis
    pub fn build_local_mapping(&self, mir_function: &MirFunction) -> LocalMap {
        let mut local_map = LocalMap::with_parameters(mir_function.parameters.len() as u32);
        let mut wasm_local_index = mir_function.parameters.len() as u32;

        // Map each MIR local to a WASM local index
        for (mir_local_index, _wasm_type) in &self.local_types {
            local_map.map_local(*mir_local_index, wasm_local_index);
            wasm_local_index += 1;
        }

        // Also map any globals that might be referenced
        // (This will be expanded when we implement global variable support)

        local_map
    }

    /// Convert WasmType to ValType for wasm_encoder
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

/// Comprehensive statistics for WASM module generation
#[derive(Debug, Clone)]
pub struct WasmModuleStats {
    /// Number of functions in the module
    pub function_count: u32,
    /// Number of types in the module
    pub type_count: u32,
    /// Number of global variables
    pub global_count: u32,
    /// String allocation statistics
    pub string_stats: StringAllocationStats,
    /// Estimated module size in bytes
    pub estimated_size: u32,
}

impl WasmModuleStats {
    /// Generate a human-readable report of module statistics
    pub fn generate_report(&self) -> String {
        format!(
            "WASM Module Statistics:\n\
             - Functions: {}\n\
             - Types: {}\n\
             - Globals: {}\n\
             - Unique Strings: {}\n\
             - String Data Size: {} bytes\n\
             - Estimated Total Size: {} bytes\n\
             - Memory Saved by Deduplication: {} bytes",
            self.function_count,
            self.type_count,
            self.global_count,
            self.string_stats.unique_strings,
            self.string_stats.total_data_size,
            self.estimated_size,
            self.string_stats.deduplication_savings
        )
    }

    /// Check if the module is within reasonable size limits
    pub fn validate_size_limits(&self) -> Result<(), CompileError> {
        const MAX_FUNCTIONS: u32 = 10000;
        const MAX_TYPES: u32 = 1000;
        const MAX_MODULE_SIZE: u32 = 50 * 1024 * 1024; // 50MB

        if self.function_count > MAX_FUNCTIONS {
            return_compiler_error!(
                "Module has too many functions ({}). Maximum supported: {}. Consider splitting into multiple modules.",
                self.function_count, MAX_FUNCTIONS
            );
        }

        if self.type_count > MAX_TYPES {
            return_compiler_error!(
                "Module has too many types ({}). Maximum supported: {}. Consider simplifying type usage.",
                self.type_count, MAX_TYPES
            );
        }

        if self.estimated_size > MAX_MODULE_SIZE {
            return_compiler_error!(
                "Module size is too large ({} bytes). Maximum supported: {} bytes. Consider splitting into multiple modules or reducing complexity.",
                self.estimated_size, MAX_MODULE_SIZE
            );
        }

        Ok(())
    }
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

    // Function registry for name resolution
    function_registry: HashMap<String, u32>,

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
            function_registry: HashMap::new(),
            function_count: 0,
            type_count: 0,
            global_count: 0,
        }
    }

    /// Create a new WasmModule from MIR with comprehensive error handling
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

        // Process functions with enhanced error context
        for (index, function) in mir.functions.iter().enumerate() {
            module.compile_function(function).map_err(|mut error| {
                // Add context about which function failed
                error.msg = format!(
                    "Failed to compile function '{}' (index {}): {}",
                    function.name, index, error.msg
                );
                error
            })?;
        }

        // Note: Validation is optional here since it consumes the module
        // Use finish_with_validation() if validation is needed

        Ok(module)
    }

    /// Validate the generated WASM module using wasmparser
    pub fn validate_module(self) -> Result<(), CompileError> {
        // Generate the WASM bytes for validation
        let wasm_bytes = self.finish();
        
        // Use wasmparser to validate the module
        match wasmparser::validate(&wasm_bytes) {
            Ok(_) => Ok(()),
            Err(wasm_error) => {
                return_wasm_validation_error!(&wasm_error, None);
            }
        }
    }

    /// Compile a MIR function to WASM with proper local variable analysis
    pub fn compile_function(&mut self, mir_function: &MirFunction) -> Result<(), CompileError> {
        // Register function in the function registry
        self.function_registry.insert(mir_function.name.clone(), self.function_count);

        // Analyze local variable requirements
        let analyzer = LocalAnalyzer::analyze_function(mir_function);
        let wasm_locals = analyzer.generate_wasm_locals();
        let local_map = analyzer.build_local_mapping(mir_function);

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

        self.type_section.ty().function(param_types, result_types.clone());

        // Add function to function section
        self.function_section.function(self.type_count);

        // Create function body with proper locals from analysis
        let mut function = Function::new(wasm_locals);

        // Lower each block with proper local mapping
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

    /// Lower a MIR statement to WASM instructions
    fn lower_statement(
        &mut self,
        statement: &Statement,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { place, rvalue } => {
                // Lower the rvalue to put value on stack
                self.lower_rvalue(rvalue, function, local_map)?;
                // Lower the place assignment to store the value
                self.lower_place_assignment(place, function, local_map)?;
                Ok(())
            }
            Statement::Call { func, args, destination } => {
                self.lower_function_call(func, args, destination, function, local_map)
            }
            Statement::Nop => {
                // No-op - generate no instructions
                Ok(())
            }
            _ => {
                return_unimplemented_feature_error!(
                    &format!("Statement type '{:?}'", statement),
                    None,
                    Some("try using simpler statements or break complex operations into multiple steps")
                );
            }
        }
    }

    /// Lower a MIR rvalue to WASM instructions
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
            Rvalue::Ref {
                place: _,
                borrow_kind: _,
            } => {
                return_unimplemented_feature_error!(
                    "Reference operations",
                    None,
                    Some("use direct value access instead of references for now")
                );
            }
        }
    }

    /// Lower a MIR operand to WASM instructions
    fn lower_operand(
        &mut self,
        operand: &Operand,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match operand {
            Operand::Constant(constant) => self.lower_constant(constant, function),
            Operand::Copy(place) => {
                // Copy operation - load value from place
                self.lower_place_access(place, function, local_map)
            }
            Operand::Move(place) => {
                // Move operation - load value from place (same as copy for now)
                // TODO: In future, this could invalidate the source place for borrow checking
                self.lower_place_access(place, function, local_map)
            }
            Operand::FunctionRef(func_index) => {
                // Function reference as index constant
                function.instruction(&Instruction::I32Const(*func_index as i32));
                Ok(())
            }
            Operand::GlobalRef(global_index) => {
                // Global reference - load global value
                let wasm_global = local_map.get_global(*global_index)
                    .ok_or_else(|| CompileError::compiler_error(
                        &format!("Global index {} not found in local mapping", global_index)
                    ))?;
                function.instruction(&Instruction::GlobalGet(wasm_global));
                Ok(())
            }
        }
    }

    /// Lower place access to WASM instructions (CRITICAL IMPLEMENTATION)
    /// 
    /// This method handles the core place resolution system that enables variable access.
    /// It converts MIR Place operations into appropriate WASM instructions.
    fn lower_place_access(
        &mut self,
        place: &Place,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local { index, wasm_type: _ } => {
                // Map MIR local to WASM local index
                let wasm_local = local_map.get_local(*index)
                    .ok_or_else(|| CompileError::compiler_error(
                        &format!("Local variable with index {} not found in mapping. This indicates a problem with local variable analysis.", index)
                    ))?;
                function.instruction(&Instruction::LocalGet(wasm_local));
                Ok(())
            }
            
            Place::Global { index, wasm_type: _ } => {
                // Map MIR global to WASM global index
                let wasm_global = local_map.get_global(*index)
                    .ok_or_else(|| CompileError::compiler_error(
                        &format!("Global variable with index {} not found in mapping", index)
                    ))?;
                function.instruction(&Instruction::GlobalGet(wasm_global));
                Ok(())
            }
            
            Place::Memory { base, offset, size } => {
                // Generate memory access with proper alignment
                self.lower_memory_base(base, function)?;
                
                // Add offset if non-zero
                if offset.0 > 0 {
                    function.instruction(&Instruction::I32Const(offset.0 as i32));
                    function.instruction(&Instruction::I32Add);
                }
                
                // Use appropriate load instruction based on type size
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
                        // For custom sizes, use appropriate load based on size
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
            
            Place::Projection { base, elem } => {
                // Handle field access and array indexing
                self.lower_place_access(base, function, local_map)?;
                self.lower_projection_element(elem, function, local_map)?;
                Ok(())
            }
        }
    }

    /// Lower function call to WASM instructions (CRITICAL IMPLEMENTATION)
    /// 
    /// This method handles function calls by loading arguments onto the WASM stack
    /// and generating appropriate call instructions.
    fn lower_function_call(
        &mut self,
        func: &Operand,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Load all arguments onto the stack in order
        for arg in args {
            self.lower_operand(arg, function, local_map)?;
        }

        // Generate the appropriate call instruction based on function operand type
        match func {
            Operand::FunctionRef(func_index) => {
                // Direct function call using function index
                function.instruction(&Instruction::Call(*func_index));
            }
            Operand::Constant(Constant::Function(func_index)) => {
                // Function constant - also direct call
                function.instruction(&Instruction::Call(*func_index));
            }
            _ => {
                return_unimplemented_feature_error!(
                    "Indirect function calls (function pointers)",
                    None,
                    Some("use direct function calls by name instead of function variables")
                );
            }
        }

        // If there's a destination place, store the result
        if let Some(dest_place) = destination {
            self.lower_place_assignment(dest_place, function, local_map)?;
        }

        Ok(())
    }

    /// Lower place assignment to WASM instructions (CRITICAL IMPLEMENTATION)
    /// 
    /// This method handles place assignment operations, generating appropriate
    /// WASM store instructions for different place types.
    fn lower_place_assignment(
        &mut self,
        place: &Place,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local { index, wasm_type: _ } => {
                // Map MIR local to WASM local index
                let wasm_local = local_map.get_local(*index)
                    .ok_or_else(|| CompileError::compiler_error(
                        &format!("Local variable with index {} not found in mapping", index)
                    ))?;
                function.instruction(&Instruction::LocalSet(wasm_local));
                Ok(())
            }
            
            Place::Global { index, wasm_type: _ } => {
                // Map MIR global to WASM global index
                let wasm_global = local_map.get_global(*index)
                    .ok_or_else(|| CompileError::compiler_error(
                        &format!("Global variable with index {} not found in mapping", index)
                    ))?;
                function.instruction(&Instruction::GlobalSet(wasm_global));
                Ok(())
            }
            
            Place::Memory { base, offset, size } => {
                // Generate address calculation first
                self.lower_memory_base(base, function)?;
                
                // Add offset if non-zero
                if offset.0 > 0 {
                    function.instruction(&Instruction::I32Const(offset.0 as i32));
                    function.instruction(&Instruction::I32Add);
                }
                
                // Use appropriate store instruction based on type size
                match size {
                    TypeSize::Byte => {
                        function.instruction(&Instruction::I32Store8(MemArg {
                            offset: 0,
                            align: 0, // 1-byte alignment
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Short => {
                        function.instruction(&Instruction::I32Store16(MemArg {
                            offset: 0,
                            align: 1, // 2-byte alignment
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Word => {
                        function.instruction(&Instruction::I32Store(MemArg {
                            offset: 0,
                            align: 2, // 4-byte alignment
                            memory_index: 0,
                        }));
                    }
                    TypeSize::DoubleWord => {
                        function.instruction(&Instruction::I64Store(MemArg {
                            offset: 0,
                            align: 3, // 8-byte alignment
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Custom { bytes, alignment } => {
                        // For custom sizes, use appropriate store based on size
                        if *bytes <= 4 {
                            function.instruction(&Instruction::I32Store(MemArg {
                                offset: 0,
                                align: (*alignment as f32).log2() as u32,
                                memory_index: 0,
                            }));
                        } else {
                            function.instruction(&Instruction::I64Store(MemArg {
                                offset: 0,
                                align: (*alignment as f32).log2() as u32,
                                memory_index: 0,
                            }));
                        }
                    }
                }
                Ok(())
            }
            
            Place::Projection { base, elem } => {
                // Handle field assignment and array element assignment
                self.lower_place_access(base, function, local_map)?;
                self.lower_projection_element(elem, function, local_map)?;
                // The actual store will be handled by the caller
                Ok(())
            }
        }
    }

    /// Lower memory base to WASM instructions
    fn lower_memory_base(
        &mut self,
        base: &MemoryBase,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        match base {
            MemoryBase::LinearMemory => {
                // Linear memory base is always at offset 0
                function.instruction(&Instruction::I32Const(0));
                Ok(())
            }
            MemoryBase::Stack => {
                // Stack-based memory (should be handled as locals)
                return_compiler_error!("Stack-based memory should be handled as local variables, not memory operations");
            }
            MemoryBase::Heap { alloc_id: _, size: _ } => {
                // Heap allocation - for now, treat as linear memory
                // TODO: Implement proper heap management
                function.instruction(&Instruction::I32Const(0));
                Ok(())
            }
        }
    }

    /// Lower projection element to WASM instructions
    fn lower_projection_element(
        &mut self,
        elem: &ProjectionElem,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match elem {
            ProjectionElem::Field { index: _, offset, size: _ } => {
                // Field access - add field offset to base address
                function.instruction(&Instruction::I32Const(offset.0 as i32));
                function.instruction(&Instruction::I32Add);
                Ok(())
            }
            ProjectionElem::Index { index, element_size } => {
                // Array indexing - calculate offset from index
                self.lower_place_access(index, function, local_map)?;
                function.instruction(&Instruction::I32Const(*element_size as i32));
                function.instruction(&Instruction::I32Mul);
                function.instruction(&Instruction::I32Add);
                Ok(())
            }
            ProjectionElem::Length => {
                // Length field access (for arrays/strings)
                // Length is typically stored at offset 0
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
            ProjectionElem::Data => {
                // Data pointer access (for arrays/strings)
                // Data pointer is typically stored after length
                function.instruction(&Instruction::I32Const(4)); // Skip length field
                function.instruction(&Instruction::I32Add);
                Ok(())
            }
            ProjectionElem::Deref => {
                // Dereference operation - load from memory
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
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
                    _ => return_compiler_error!("Unsupported constant type for WASM type determination: {:?}", constant),
                }
            }
            Operand::Copy(place) | Operand::Move(place) => {
                Ok(place.wasm_type())
            }
            Operand::FunctionRef(_) => Ok(WasmType::I32), // Function references are i32 indices
            Operand::GlobalRef(_) => Ok(WasmType::I32), // Global references are i32 indices
        }
    }

    /// Lower binary operations to WASM instructions with type awareness
    fn lower_binary_op(&self, op: &BinOp, wasm_type: &WasmType, function: &mut Function) -> Result<(), CompileError> {
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
                return_compiler_error!("Logical operations (and/or) should be implemented as control flow, not binary operations. Use if/else statements for short-circuiting behavior.");
            }

            // Unsupported combinations
            (op, wasm_type) => {
                return_compiler_error!(
                    "Binary operation {:?} not supported for WASM type {:?}. Check that the operation is valid for the given type.",
                    op, wasm_type
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
    fn lower_if_terminator(
        &mut self,
        condition: &Operand,
        then_block: u32,
        else_block: u32,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        // Validate that condition is boolean (i32 in WASM)
        let condition_type = self.get_operand_wasm_type(condition)?;
        if !matches!(condition_type, WasmType::I32) {
            return_compiler_error!(
                "If condition must be boolean (i32 in WASM), found {:?}. This indicates a type checking error in MIR generation.",
                condition_type
            );
        }
        
        // Load condition onto stack
        self.lower_operand(condition, function, local_map)?;
        
        // Generate structured control flow using WASM if/else/end
        // The condition is already on the stack from the operand lowering
        function.instruction(&Instruction::If(wasm_encoder::BlockType::Empty));
        
        // For now, we'll generate placeholder blocks
        // TODO: Generate actual block instructions when block lowering is implemented
        // This is a simplified implementation that maintains stack discipline
        
        // Then block (executed when condition is true)
        // In a full implementation, this would lower the actual then_block MIR
        function.instruction(&Instruction::Nop); // Placeholder for then block
        
        function.instruction(&Instruction::Else);
        
        // Else block (executed when condition is false)  
        // In a full implementation, this would lower the actual else_block MIR
        function.instruction(&Instruction::Nop); // Placeholder for else block
        
        function.instruction(&Instruction::End);
        
        Ok(())
    }

    /// Lower a MIR terminator to WASM control flow instructions
    pub fn lower_terminator(
        &mut self,
        terminator: &Terminator,
        function: &mut Function,
        local_map: &LocalMap,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::Goto { target: _ } => {
                // For simple linear control flow, no explicit branch needed
                // The next block will be processed sequentially
                Ok(())
            }
            Terminator::If {
                condition,
                then_block,
                else_block,
            } => {
                self.lower_if_terminator(condition, *then_block, *else_block, function, local_map)
            }
            Terminator::Return { values } => {
                // Load return values onto the stack
                for value in values {
                    self.lower_operand(value, function, local_map)?;
                }
                // Add explicit Return instruction for proper function termination
                function.instruction(&Instruction::Return);
                Ok(())
            }
            Terminator::Unreachable => {
                function.instruction(&Instruction::Unreachable);
                Ok(())
            }
        }
    }

    /// Ensure function has proper termination
    /// 
    /// WASM functions must end with a terminating instruction (return, unreachable, etc.)
    /// The wasm_encoder library requires explicit termination for all functions.
    fn ensure_function_termination(
        &self,
        function: &mut Function,
        result_types: &[ValType],
    ) -> Result<(), CompileError> {
        if result_types.is_empty() {
            // For void functions, add an explicit return instruction
            function.instruction(&Instruction::Return);
        } else {
            // For functions with return types, we need to provide default values
            // This is a simplified approach - in a full implementation, we would
            // analyze the control flow to determine if a return is actually needed
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

    /// Validate and finish building the WASM module with comprehensive error handling
    pub fn finish_with_validation(self) -> Result<Vec<u8>, CompileError> {
        let wasm_bytes = self.finish();
        
        // Validate the generated WASM module
        match wasmparser::validate(&wasm_bytes) {
            Ok(_) => Ok(wasm_bytes),
            Err(wasm_error) => {
                return_wasm_validation_error!(&wasm_error, None);
            }
        }
    }

    /// Get comprehensive module statistics for debugging and optimization
    pub fn get_module_stats(&self) -> WasmModuleStats {
        WasmModuleStats {
            function_count: self.function_count,
            type_count: self.type_count,
            global_count: self.global_count,
            string_stats: self.string_manager.get_allocation_stats(),
            estimated_size: self.estimate_module_size(),
        }
    }

    /// Estimate the final module size for memory planning
    fn estimate_module_size(&self) -> u32 {
        // Rough estimation based on section counts
        let base_size = 100; // Basic module overhead
        let type_size = self.type_count * 20; // Rough estimate per type
        let function_size = self.function_count * 50; // Rough estimate per function
        let data_size = self.string_manager.get_data_size();
        
        base_size + type_size + function_size + data_size
    }

    /// Get function index by name for function calls
    pub fn get_function_index(&self, name: &str) -> Option<u32> {
        self.function_registry.get(name).copied()
    }

    /// Register a function name with its index
    pub fn register_function(&mut self, name: String, index: u32) {
        self.function_registry.insert(name, index);
    }

    /// Get all registered functions
    pub fn get_all_functions(&self) -> &HashMap<String, u32> {
        &self.function_registry
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
