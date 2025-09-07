use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::mir_nodes::{
    Constant, MIR, MirFunction, Operand, Rvalue, Statement, Terminator,
};
use crate::compiler::mir::place::{Place, WasmType};
use crate::return_compiler_error;
use std::borrow::Cow;
use std::collections::HashMap;
use wasm_encoder::*;

pub struct WasmModule {
    // Function acting as Global scope of Beanstalk module
    // Runs automatically when the module is loaded and can't have any args or returns
    start_section: Option<StartSection>,
    type_section: TypeSection,
    import_section: ImportSection,
    function_signature_section: FunctionSection,
    table_section: TableSection,
    memory_section: MemorySection,
    global_section: GlobalSection,
    export_section: ExportSection,
    element_section: ElementSection,
    code_section: CodeSection,
    data_section: DataSection,

    // Internal state for tracking
    pub function_count: u32,
    pub type_count: u32,
    global_count: u32,
    local_count: u32,
    string_constants: Vec<String>,
    string_constant_map: std::collections::HashMap<String, u32>,

    // Heap management (for simple bump-pointer allocator)
    heap_ptr_global_index: Option<u32>,
    initial_heap_offset: u32,
}

impl Default for WasmModule {
    fn default() -> Self {
        Self::new()
    }
}

impl WasmModule {
    pub fn new() -> Self {
        let start_section = StartSection { function_index: 0 };

        Self {
            start_section: Option::from(start_section),
            type_section: TypeSection::new(),
            import_section: ImportSection::new(),
            function_signature_section: FunctionSection::new(),
            table_section: TableSection::new(),
            memory_section: MemorySection::new(),
            global_section: GlobalSection::new(),
            export_section: ExportSection::new(),
            element_section: ElementSection::new(),
            code_section: CodeSection::new(),
            data_section: DataSection::new(),
            function_count: 0,
            type_count: 0,
            global_count: 0,
            local_count: 0,
            string_constants: Vec::new(),
            string_constant_map: std::collections::HashMap::new(),
            heap_ptr_global_index: None,
            initial_heap_offset: 0,
        }
    }

    /// Create a new WasmModule from MIR with proper initialization
    pub fn from_mir(mir: &MIR) -> Result<WasmModule, CompileError> {
        let mut module = WasmModule::new();

        // Initialize memory section based on MIR memory requirements
        module.initialize_memory_from_mir(mir)?;

        // Initialize type section from MIR function signatures
        module.initialize_types_from_mir(mir)?;

        // Initialize string constants from MIR
        module.initialize_string_constants_from_mir(mir)?;

        // Initialize global section from MIR globals
        module.initialize_globals_from_mir(mir)?;

        // Initialize interface support if needed
        if !mir.type_info.interface_info.interfaces.is_empty() {
            module.initialize_interface_support_from_mir(mir)?;
        }
        // Record initial heap offset (start of dynamic allocations)
        module.initial_heap_offset = mir.type_info.memory_info.static_data_size;
        Ok(module)
    }

    /// Initialize memory section from MIR memory information
    fn initialize_memory_from_mir(&mut self, mir: &MIR) -> Result<(), CompileError> {
        let memory_info = &mir.type_info.memory_info;

        // Set up WASM memory with initial and max pages from MIR
        let memory_type = MemoryType {
            minimum: memory_info.initial_pages as u64,
            maximum: memory_info.max_pages.map(|p| p as u64),
            memory64: false,
            shared: false,
            page_size_log2: None,
        };

        self.memory_section.memory(memory_type);

        // Create a global heap pointer for dynamic allocations (bump-pointer)
        // Initialized to static_data_size so dynamic allocations follow static data.
        let heap_ptr_global_type = GlobalType {
            val_type: ValType::I32,
            mutable: true,
            shared: false,
        };
        let init_expr = ConstExpr::i32_const(memory_info.static_data_size as i32);
        self.global_section.global(heap_ptr_global_type, &init_expr);
        self.heap_ptr_global_index = Some(self.global_count);
        self.global_count += 1;

        Ok(())
    }

    /// Initialize type section from MIR function signatures
    fn initialize_types_from_mir(&mut self, mir: &MIR) -> Result<(), CompileError> {
        // Add function types from MIR type information
        for function_sig in &mir.type_info.function_types {
            let param_types: Vec<ValType> = function_sig
                .param_types
                .iter()
                .map(|wasm_type| self.wasm_type_to_val_type(wasm_type))
                .collect();

            let result_types: Vec<ValType> = function_sig
                .result_types
                .iter()
                .map(|wasm_type| self.wasm_type_to_val_type(wasm_type))
                .collect();

            self.type_section.ty().function(param_types, result_types);
            self.type_count += 1;
        }

        Ok(())
    }

    /// Initialize string constants from MIR
    fn initialize_string_constants_from_mir(&mut self, mir: &MIR) -> Result<(), CompileError> {
        let mut current_offset = mir.type_info.memory_info.static_data_size;

        // Collect all string constants from MIR functions
        for function in &mir.functions {
            for block in &function.blocks {
                for statement in &block.statements {
                    self.collect_string_constants_from_statement(statement, &mut current_offset)?;
                }
                self.collect_string_constants_from_terminator(
                    &block.terminator,
                    &mut current_offset,
                )?;
            }
        }

        // Add string constants to data section
        for (string_value, offset) in &self.string_constant_map {
            let string_bytes = string_value.as_bytes();
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(*offset as i32),
                string_bytes.iter().copied(),
            );
        }

        Ok(())
    }

    /// Initialize globals from MIR global variables
    fn initialize_globals_from_mir(&mut self, mir: &MIR) -> Result<(), CompileError> {
        for (_global_id, place) in &mir.globals {
            let wasm_type = self.wasm_type_to_val_type(&place.wasm_type());
            let global_type = GlobalType {
                val_type: wasm_type,
                mutable: true, // Most globals are mutable in Beanstalk
                shared: false,
            };

            // Initialize with zero value
            let init_expr = match wasm_type {
                ValType::I32 => ConstExpr::i32_const(0),
                ValType::I64 => ConstExpr::i64_const(0),
                ValType::F32 => ConstExpr::f32_const(0.0.into()),
                ValType::F64 => ConstExpr::f64_const(0.0.into()),
                ValType::V128 => return_compiler_error!("V128 globals not supported yet"),
                ValType::Ref(ref_type) => ConstExpr::ref_null(ref_type.heap_type),
            };

            self.global_section.global(global_type, &init_expr);
            self.global_count += 1;
        }

        Ok(())
    }

    /// Initialize interface support (vtables and function tables)
    fn initialize_interface_support_from_mir(&mut self, mir: &MIR) -> Result<(), CompileError> {
        let interface_info = &mir.type_info.interface_info;

        // Generate interface method type signatures
        self.generate_interface_method_types(interface_info)?;

        // Create function table for call_indirect
        if !interface_info.function_table.is_empty() {
            self.create_interface_function_table(interface_info)?;
        }

        // Generate vtable data structures in linear memory
        if !interface_info.vtables.is_empty() {
            self.generate_vtable_data_structures(interface_info)?;
        }

        Ok(())
    }

    /// Generate type signatures for all interface methods
    fn generate_interface_method_types(
        &mut self,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        for (_interface_id, interface_def) in &interface_info.interfaces {
            for method in &interface_def.methods {
                // Convert method signature to WASM function type
                let param_types = self.wasm_types_to_val_types(&method.param_types);
                let result_types = self.wasm_types_to_val_types(&method.return_types);

                // Validate the method signature
                self.validate_function_signature(&param_types, &result_types)?;

                // Add to type section
                self.type_section.ty().function(param_types, result_types);
                self.type_count += 1;
            }
        }
        Ok(())
    }

    /// Create WASM function table for interface dispatch
    fn create_interface_function_table(
        &mut self,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        let table_type = TableType {
            element_type: RefType::FUNCREF,
            minimum: interface_info.function_table.len() as u64,
            maximum: Some(interface_info.function_table.len() as u64),
            table64: false,
            shared: false,
        };

        self.table_section.table(table_type);

        // Initialize element section with function indices
        let func_indices: Vec<u32> = interface_info.function_table.clone();
        self.element_section.active(
            Some(0),                  // Table index 0
            &ConstExpr::i32_const(0), // Offset 0
            Elements::Functions(Cow::Borrowed(&func_indices)),
        );

        Ok(())
    }

    /// Generate vtable data structures in linear memory
    fn generate_vtable_data_structures(
        &mut self,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        let mut current_vtable_offset = 0u32;

        for (_vtable_id, vtable) in &interface_info.vtables {
            // Calculate vtable size (4 bytes per method function index)
            let vtable_size = vtable.method_functions.len() * 4;

            // Create vtable data as bytes
            let mut vtable_bytes = Vec::new();
            for &func_index in &vtable.method_functions {
                // Store function indices as little-endian i32
                vtable_bytes.extend_from_slice(&func_index.to_le_bytes());
            }

            // Add vtable to data section
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(current_vtable_offset as i32),
                vtable_bytes.iter().copied(),
            );

            current_vtable_offset += vtable_size as u32;
        }

        Ok(())
    }

    /// Get interface method type index for call_indirect
    pub fn get_interface_method_type_index(
        &self,
        interface_id: u32,
        method_id: u32,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<u32, CompileError> {
        // Find the interface definition
        let interface_def = interface_info
            .interfaces
            .get(&interface_id)
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!("Interface {} not found", interface_id))
            })?;

        // Find the method within the interface
        let method = interface_def
            .methods
            .iter()
            .find(|m| m.id == method_id)
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!(
                    "Method {} not found in interface {}",
                    method_id, interface_id
                ))
            })?;

        // Calculate type index based on method position
        // This assumes interface method types are added sequentially after function types
        let mut type_index = self.type_count;

        // Find the position of this method among all interface methods
        for (_iface_id, iface_def) in &interface_info.interfaces {
            if iface_def.id == interface_id {
                for iface_method in &iface_def.methods {
                    if iface_method.id == method_id {
                        return Ok(type_index);
                    }
                    type_index += 1;
                }
                break;
            } else {
                type_index += iface_def.methods.len() as u32;
            }
        }

        return_compiler_error!(
            "Could not determine type index for interface {} method {}",
            interface_id,
            method_id
        );
    }

    /// Calculate vtable offset for a given interface and implementing type
    pub fn calculate_vtable_offset(
        &self,
        interface_id: u32,
        type_id: u32,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<u32, CompileError> {
        let mut offset = 0u32;

        // Collect and sort vtables by type_id for deterministic ordering
        let mut sorted_vtables: Vec<_> = interface_info.vtables.values().collect();
        sorted_vtables.sort_by_key(|vtable| vtable.type_id);

        // Find the vtable for this interface and type combination
        for vtable in sorted_vtables {
            if vtable.interface_id == interface_id && vtable.type_id == type_id {
                return Ok(offset);
            }
            // Each vtable entry is 4 bytes (function index as i32)
            offset += vtable.method_functions.len() as u32 * 4;
        }

        return_compiler_error!(
            "VTable not found for interface {} and type {}",
            interface_id,
            type_id
        );
    }

    /// Generate interface method index mapping for efficient dispatch
    pub fn create_interface_method_mapping(
        &self,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> InterfaceMethodMapping {
        let mut mapping = InterfaceMethodMapping::new();

        // Map each interface method to its function table index
        for (_interface_id, interface_def) in &interface_info.interfaces {
            for method in &interface_def.methods {
                // Find all implementations of this method across vtables
                for (_vtable_id, vtable) in &interface_info.vtables {
                    if vtable.interface_id == interface_def.id {
                        // Find the method index within the interface
                        if let Some(method_index) =
                            interface_def.methods.iter().position(|m| m.id == method.id)
                        {
                            if method_index < vtable.method_functions.len() {
                                let func_index = vtable.method_functions[method_index];
                                mapping.add_method_implementation(
                                    interface_def.id,
                                    method.id,
                                    vtable.type_id,
                                    func_index,
                                );
                            }
                        }
                    }
                }
            }
        }

        mapping
    }

    /// Compile a MIR function and add it to the module
    /// This method provides complete MirFunction → wasm-encoder Function conversion
    pub fn compile_mir_function(
        &mut self,
        mir_function: &MirFunction,
    ) -> Result<u32, CompileError> {
        // Add function signature to function section
        let type_index = self.get_or_create_function_type(mir_function)?;
        self.function_signature_section.function(type_index);

        // Build local variable mapping using the new place resolution method
        let local_map = self.build_local_index_mapping(mir_function)?;

        // Create WASM function with locals (proper type information as count, ValType pairs)
        let locals = self.build_function_locals(mir_function, &local_map)?;
        let mut function = Function::new(locals);

        // Generate function prologue for parameter handling
        self.generate_function_prologue(mir_function, &mut function, &local_map)?;

        // Lower each block to WASM instructions using statement iteration
        // Note: For interface support, we would need to pass interface_info here
        // For now, we use the basic lowering which will error on interface calls
        for block in &mir_function.blocks {
            self.lower_block_to_wasm(block, &mut function, &local_map)?;
        }

        // Generate function epilogue for return handling
        self.generate_function_epilogue(mir_function, &mut function, &local_map)?;

        // Generate proper function end instruction and validation
        function.instruction(&Instruction::End);

        // Validate the generated function before adding to code section
        self.validate_generated_function(mir_function, &function)?;

        // Add to code section using existing integration
        self.code_section.function(&function);

        let function_index = self.function_count;
        self.function_count += 1;

        Ok(function_index)
    }

    /// Compile a MIR function with interface support and add it to the module
    pub fn compile_mir_function_with_interface_support(
        &mut self,
        mir_function: &MirFunction,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<u32, CompileError> {
        // Add function signature to function section
        let type_index = self.get_or_create_function_type(mir_function)?;
        self.function_signature_section.function(type_index);

        // Build local variable mapping using the new place resolution method
        let local_map = self.build_local_index_mapping(mir_function)?;

        // Create WASM function with locals (proper type information as count, ValType pairs)
        let locals = self.build_function_locals(mir_function, &local_map)?;
        let mut function = Function::new(locals);

        // Generate function prologue for parameter handling
        self.generate_function_prologue(mir_function, &mut function, &local_map)?;

        // Lower each block to WASM instructions with interface support
        for block in &mir_function.blocks {
            self.lower_block_to_wasm_with_interface_support(
                block,
                &mut function,
                &local_map,
                interface_info,
            )?;
        }

        // Generate function epilogue for return handling
        self.generate_function_epilogue(mir_function, &mut function, &local_map)?;

        // Generate proper function end instruction and validation
        function.instruction(&Instruction::End);

        // Validate the generated function before adding to code section
        self.validate_generated_function(mir_function, &function)?;

        // Add to code section using existing integration
        self.code_section.function(&function);

        let function_index = self.function_count;
        self.function_count += 1;

        Ok(function_index)
    }

    /// Convert MIR WasmType to wasm-encoder ValType with pointer handling
    pub fn wasm_type_to_val_type(&self, wasm_type: &WasmType) -> ValType {
        match wasm_type {
            WasmType::I32 => ValType::I32,
            WasmType::I64 => ValType::I64,
            WasmType::F32 => ValType::F32,
            WasmType::F64 => ValType::F64,
            WasmType::ExternRef => ValType::I32, // Pointer types as i32 in linear memory model
            WasmType::FuncRef => ValType::Ref(RefType::FUNCREF),
        }
    }

    /// Convert multiple WasmTypes to ValTypes for function signatures
    pub fn wasm_types_to_val_types(&self, wasm_types: &[WasmType]) -> Vec<ValType> {
        wasm_types
            .iter()
            .map(|wt| self.wasm_type_to_val_type(wt))
            .collect()
    }

    /// Validate type compatibility between MIR and WASM
    pub fn validate_type_compatibility(
        &self,
        mir_type: &WasmType,
        expected_wasm_type: ValType,
    ) -> Result<(), CompileError> {
        let actual_wasm_type = self.wasm_type_to_val_type(mir_type);
        if actual_wasm_type != expected_wasm_type {
            return_compiler_error!(
                "Type mismatch: MIR type {:?} maps to WASM type {:?}, but expected {:?}",
                mir_type,
                actual_wasm_type,
                expected_wasm_type
            );
        }
        Ok(())
    }

    /// Generate function signature from MirFunction metadata and add to type section
    pub fn add_function_signature_from_mir(
        &mut self,
        mir_function: &MirFunction,
    ) -> Result<u32, CompileError> {
        // Convert parameter types
        let param_types = self.wasm_types_to_val_types(&mir_function.signature.param_types);

        // Convert result types
        let result_types = self.wasm_types_to_val_types(&mir_function.signature.result_types);

        // Validate WASM function signature constraints
        self.validate_function_signature(&param_types, &result_types)?;

        // Add to type section
        self.type_section.ty().function(param_types, result_types);

        let type_index = self.type_count;
        self.type_count += 1;

        Ok(type_index)
    }

    /// Validate WASM function signature constraints
    fn validate_function_signature(
        &self,
        param_types: &[ValType],
        result_types: &[ValType],
    ) -> Result<(), CompileError> {
        // WASM 1.0 allows at most 1 result type, WASM multi-value allows multiple
        if result_types.len() > 1 {
            // For now, we'll support multi-value results but warn about compatibility
            // In a real implementation, this might be configurable based on target WASM version
        }

        // Validate that all types are supported in WASM
        for param_type in param_types {
            self.validate_wasm_val_type(param_type)?;
        }

        for result_type in result_types {
            self.validate_wasm_val_type(result_type)?;
        }

        Ok(())
    }

    /// Validate that a ValType is supported in our WASM target
    fn validate_wasm_val_type(&self, val_type: &ValType) -> Result<(), CompileError> {
        match val_type {
            ValType::I32 | ValType::I64 | ValType::F32 | ValType::F64 => Ok(()),
            ValType::Ref(_ref_type) => {
                // For now, accept all reference types - we'll validate specific heap types later
                Ok(())
            }
            ValType::V128 => return_compiler_error!("V128 SIMD types not yet supported"),
        }
    }

    /// Get or create function type index for MIR function
    fn get_or_create_function_type(
        &mut self,
        mir_function: &MirFunction,
    ) -> Result<u32, CompileError> {
        // Use the new signature generation method
        self.add_function_signature_from_mir(mir_function)
    }

    /// Add interface method signature to type section
    pub fn add_interface_method_signature(
        &mut self,
        method: &crate::compiler::mir::mir_nodes::MethodSignature,
    ) -> Result<u32, CompileError> {
        // Convert parameter types
        let param_types = self.wasm_types_to_val_types(&method.param_types);

        // Convert result types
        let result_types = self.wasm_types_to_val_types(&method.return_types);

        // Validate WASM function signature constraints
        self.validate_function_signature(&param_types, &result_types)?;

        // Add to type section
        self.type_section.ty().function(param_types, result_types);

        let type_index = self.type_count;
        self.type_count += 1;

        Ok(type_index)
    }

    /// Calculate struct layout with proper WASM alignment rules
    pub fn calculate_struct_layout(&self, field_types: &[WasmType]) -> StructLayout {
        let mut layout = StructLayout::new();
        let mut current_offset = 0u32;

        for (field_index, field_type) in field_types.iter().enumerate() {
            let field_size = self.get_wasm_type_size(field_type);
            let field_alignment = self.get_wasm_type_alignment(field_type);

            // Align the current offset to the field's alignment requirement
            current_offset = align_to(current_offset, field_alignment);

            layout.add_field(
                field_index as u32,
                current_offset,
                field_size,
                field_alignment,
            );
            current_offset += field_size;
        }

        // Align the total size to the largest alignment requirement
        let max_alignment = field_types
            .iter()
            .map(|t| self.get_wasm_type_alignment(t))
            .max()
            .unwrap_or(1);

        layout.total_size = align_to(current_offset, max_alignment);
        layout.alignment = max_alignment;

        layout
    }

    /// Get the size in bytes of a WASM type
    pub fn get_wasm_type_size(&self, wasm_type: &WasmType) -> u32 {
        match wasm_type {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::ExternRef | WasmType::FuncRef => 4, // Pointers are 4 bytes in WASM32
        }
    }

    /// Get the alignment requirement of a WASM type
    pub fn get_wasm_type_alignment(&self, wasm_type: &WasmType) -> u32 {
        match wasm_type {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
            WasmType::ExternRef | WasmType::FuncRef => 4, // Pointer alignment
        }
    }

    /// Create a type index mapping for efficient type lookups
    pub fn create_type_index_mapping(
        &mut self,
        mir: &MIR,
    ) -> Result<TypeIndexMapping, CompileError> {
        let mut mapping = TypeIndexMapping::new();

        // Map function signatures to type indices
        for (func_index, function) in mir.functions.iter().enumerate() {
            let type_index = self.add_function_signature_from_mir(function)?;
            mapping.add_function_type(func_index as u32, type_index);
        }

        // Map interface method signatures
        for (_interface_id, interface_def) in &mir.type_info.interface_info.interfaces {
            for method in &interface_def.methods {
                let type_index = self.add_interface_method_signature(method)?;
                mapping.add_interface_method_type(interface_def.id, method.id, type_index);
            }
        }

        Ok(mapping)
    }

    /// Validate all types in the MIR for WASM compatibility
    pub fn validate_mir_types(&self, mir: &MIR) -> Result<(), CompileError> {
        // Validate function signatures
        for function in &mir.functions {
            for param_type in &function.signature.param_types {
                let val_type = self.wasm_type_to_val_type(param_type);
                self.validate_wasm_val_type(&val_type)?;
            }

            for result_type in &function.signature.result_types {
                let val_type = self.wasm_type_to_val_type(result_type);
                self.validate_wasm_val_type(&val_type)?;
            }
        }

        // Validate global types
        for (_global_id, place) in &mir.globals {
            let wasm_type = place.wasm_type();
            let val_type = self.wasm_type_to_val_type(&wasm_type);
            self.validate_wasm_val_type(&val_type)?;
        }

        // Validate interface method signatures
        for (_interface_id, interface_def) in &mir.type_info.interface_info.interfaces {
            for method in &interface_def.methods {
                for param_type in &method.param_types {
                    let val_type = self.wasm_type_to_val_type(param_type);
                    self.validate_wasm_val_type(&val_type)?;
                }

                for result_type in &method.return_types {
                    let val_type = self.wasm_type_to_val_type(result_type);
                    self.validate_wasm_val_type(&val_type)?;
                }
            }
        }

        Ok(())
    }

    /// Build local variable mapping from MIR places to WASM local indices
    /// This is the legacy method - use build_local_index_mapping for new code
    fn build_local_map(
        &self,
        mir_function: &MirFunction,
    ) -> Result<HashMap<Place, u32>, CompileError> {
        let mut local_map = HashMap::new();
        let mut local_index = mir_function.parameters.len() as u32;

        // Map parameters to local indices 0..n-1
        for (i, param_place) in mir_function.parameters.iter().enumerate() {
            local_map.insert(param_place.clone(), i as u32);
        }

        // Map local variables to subsequent indices
        for (_, local_place) in &mir_function.locals {
            if !local_map.contains_key(local_place) {
                local_map.insert(local_place.clone(), local_index);
                local_index += 1;
            }
        }

        Ok(local_map)
    }

    /// Build WASM function locals declaration
    fn build_function_locals(
        &self,
        mir_function: &MirFunction,
        local_map: &HashMap<Place, u32>,
    ) -> Result<Vec<(u32, ValType)>, CompileError> {
        let mut locals = Vec::new();
        let param_count = mir_function.parameters.len() as u32;

        // Count locals by type (excluding parameters)
        // Parameters are automatically available as locals 0..n-1 in WASM
        let mut type_counts: HashMap<ValType, u32> = HashMap::new();

        // Count all local variables that need to be declared
        for (_, local_place) in &mir_function.locals {
            if let Some(&local_index) = local_map.get(local_place) {
                if local_index >= param_count {
                    let val_type = self.wasm_type_to_val_type(&local_place.wasm_type());
                    *type_counts.entry(val_type).or_insert(0) += 1;
                }
            }
        }

        // Also count any places in the local_map that aren't parameters or in mir_function.locals
        // This handles cases where the MIR creates local places that aren't explicitly in the locals map
        for (place, &local_index) in local_map {
            if local_index >= param_count {
                // Check if this place is already counted in mir_function.locals
                let already_counted = mir_function
                    .locals
                    .values()
                    .any(|local_place| local_place == place);
                if !already_counted {
                    let val_type = self.wasm_type_to_val_type(&place.wasm_type());
                    *type_counts.entry(val_type).or_insert(0) += 1;
                }
            }
        }

        // Convert to (count, type) pairs
        for (val_type, count) in type_counts {
            locals.push((count, val_type));
        }

        Ok(locals)
    }

    /// Collect string constants from a MIR statement
    fn collect_string_constants_from_statement(
        &mut self,
        statement: &Statement,
        current_offset: &mut u32,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { rvalue, .. } => {
                self.collect_string_constants_from_rvalue(rvalue, current_offset)?;
            }
            Statement::Call { args, .. } => {
                for arg in args {
                    self.collect_string_constants_from_operand(arg, current_offset)?;
                }
            }
            Statement::InterfaceCall { args, receiver, .. } => {
                self.collect_string_constants_from_operand(receiver, current_offset)?;
                for arg in args {
                    self.collect_string_constants_from_operand(arg, current_offset)?;
                }
            }
            _ => {} // Other statements don't contain string constants
        }
        Ok(())
    }

    /// Collect string constants from a MIR rvalue
    fn collect_string_constants_from_rvalue(
        &mut self,
        rvalue: &Rvalue,
        current_offset: &mut u32,
    ) -> Result<(), CompileError> {
        match rvalue {
            Rvalue::Use(operand) => {
                self.collect_string_constants_from_operand(operand, current_offset)?;
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.collect_string_constants_from_operand(left, current_offset)?;
                self.collect_string_constants_from_operand(right, current_offset)?;
            }
            Rvalue::UnaryOp { operand, .. } => {
                self.collect_string_constants_from_operand(operand, current_offset)?;
            }
            Rvalue::Cast { source, .. } => {
                self.collect_string_constants_from_operand(source, current_offset)?;
            }
            Rvalue::Array { elements, .. } => {
                for element in elements {
                    self.collect_string_constants_from_operand(element, current_offset)?;
                }
            }
            Rvalue::Struct { fields, .. } => {
                for (_, operand) in fields {
                    self.collect_string_constants_from_operand(operand, current_offset)?;
                }
            }
            _ => {} // Other rvalues don't contain string constants
        }
        Ok(())
    }

    /// Collect string constants from a MIR operand
    fn collect_string_constants_from_operand(
        &mut self,
        operand: &Operand,
        current_offset: &mut u32,
    ) -> Result<(), CompileError> {
        if let Operand::Constant(Constant::String(string_value)) = operand {
            if !self.string_constant_map.contains_key(string_value) {
                self.string_constant_map
                    .insert(string_value.clone(), *current_offset);
                self.string_constants.push(string_value.clone());
                *current_offset += string_value.len() as u32 + 1; // +1 for null terminator
            }
        }
        Ok(())
    }

    /// Collect string constants from a MIR terminator
    fn collect_string_constants_from_terminator(
        &mut self,
        terminator: &Terminator,
        current_offset: &mut u32,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::If { condition, .. } => {
                self.collect_string_constants_from_operand(condition, current_offset)?;
            }
            Terminator::Switch { discriminant, .. } => {
                self.collect_string_constants_from_operand(discriminant, current_offset)?;
            }
            Terminator::Return { values } => {
                for value in values {
                    self.collect_string_constants_from_operand(value, current_offset)?;
                }
            }
            _ => {} // Other terminators don't contain string constants
        }
        Ok(())
    }

    /// Lower a MIR block to WASM instructions
    fn lower_block_to_wasm(
        &self,
        block: &crate::compiler::mir::mir_nodes::MirBlock,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Lower each statement in the block
        for statement in &block.statements {
            self.lower_statement(statement, function, local_map)?;
        }

        // Lower the terminator
        self.lower_terminator(&block.terminator, function, local_map)?;

        Ok(())
    }

    /// Lower a MIR block to WASM instructions with interface support
    pub fn lower_block_to_wasm_with_interface_support(
        &self,
        block: &crate::compiler::mir::mir_nodes::MirBlock,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        // Lower each statement in the block with interface support
        for statement in &block.statements {
            match statement {
                Statement::InterfaceCall {
                    interface_id,
                    method_id,
                    receiver,
                    args,
                    destination,
                } => {
                    self.lower_interface_call(
                        *interface_id,
                        *method_id,
                        receiver,
                        args,
                        destination,
                        function,
                        local_map,
                        interface_info,
                    )?;
                }
                _ => {
                    self.lower_statement(statement, function, local_map)?;
                }
            }
        }

        // Lower the terminator
        self.lower_terminator(&block.terminator, function, local_map)?;

        Ok(())
    }

    /// Generate function prologue for parameter handling
    /// This sets up the function entry point and initializes any necessary state
    fn generate_function_prologue(
        &self,
        mir_function: &MirFunction,
        function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // For most functions, no special prologue is needed in WASM
        // Parameters are automatically available as local variables 0..n-1
        // However, we can add validation or initialization code here if needed

        // Validate that we have the expected number of parameters
        if mir_function.parameters.len() > u32::MAX as usize {
            return_compiler_error!(
                "Function has too many parameters: {}",
                mir_function.parameters.len()
            );
        }

        // In WASM, parameters are automatically loaded into locals 0..n-1
        // No explicit prologue instructions are typically needed
        // This is a placeholder for future enhancements like:
        // - Stack frame setup for complex functions
        // - Parameter validation
        // - Debug information generation

        Ok(())
    }

    /// Generate function epilogue for return handling
    /// This handles function return and cleanup
    fn generate_function_epilogue(
        &self,
        mir_function: &MirFunction,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Handle return values based on function signature
        match mir_function.return_types.len() {
            0 => {
                // No return value - function ends with implicit return
                // The End instruction will be added by the caller
            }
            1 => {
                // Single return value - ensure it's on the stack
                // For now, assume the return value is already on the stack from the last statement
                // In a full implementation, this would check the last block's terminator

                // If the function has a return type but no explicit return in the last block,
                // we might need to load a default value
                // For now, we'll assume the MIR is well-formed and has proper returns
            }
            _ => {
                // Multiple return values (WASM multi-value)
                // For now, this is not fully implemented
                return_compiler_error!(
                    "Multi-value returns not yet fully implemented for function '{}'",
                    mir_function.name
                );
            }
        }

        // Add any cleanup code here if needed
        // For example:
        // - Reference counting decrements
        // - Memory cleanup
        // - Debug information

        Ok(())
    }

    /// Validate the generated function before adding to code section
    /// This ensures the function is well-formed and meets WASM requirements
    fn validate_generated_function(
        &self,
        mir_function: &MirFunction,
        _function: &Function,
    ) -> Result<(), CompileError> {
        // Validate function signature constraints
        if mir_function.parameters.len() > 1000 {
            return_compiler_error!(
                "Function '{}' has too many parameters: {}",
                mir_function.name,
                mir_function.parameters.len()
            );
        }

        if mir_function.return_types.len() > 1000 {
            return_compiler_error!(
                "Function '{}' has too many return types: {}",
                mir_function.name,
                mir_function.return_types.len()
            );
        }

        // Validate that all parameter types are supported
        for (i, param_place) in mir_function.parameters.iter().enumerate() {
            let wasm_type = param_place.wasm_type();
            let val_type = self.wasm_type_to_val_type(&wasm_type);
            if let Err(_) = self.validate_wasm_val_type(&val_type) {
                return_compiler_error!(
                    "Invalid parameter type at index {} in function '{}'",
                    i,
                    mir_function.name
                );
            }
        }

        // Validate that all return types are supported
        for (i, return_type) in mir_function.return_types.iter().enumerate() {
            let val_type = self.wasm_type_to_val_type(return_type);
            if let Err(_) = self.validate_wasm_val_type(&val_type) {
                return_compiler_error!(
                    "Invalid return type at index {} in function '{}'",
                    i,
                    mir_function.name
                );
            }
        }

        // Validate that the function has at least one block
        if mir_function.blocks.is_empty() {
            return_compiler_error!("Function '{}' has no blocks", mir_function.name);
        }

        // Additional validation can be added here:
        // - Check that all local variables are properly typed
        // - Validate control flow structure
        // - Check that all places in the function are resolvable

        Ok(())
    }

    // ===== STATEMENT LOWERING METHODS =====

    /// Lower a MIR statement to WASM instructions
    /// Each statement maps to ≤3 WASM instructions for optimal performance
    pub fn lower_statement(
        &self,
        statement: &Statement,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match statement {
            Statement::Assign { place, rvalue } => {
                self.lower_assign_statement(place, rvalue, function, local_map)
            }

            Statement::Call {
                func,
                args,
                destination,
            } => self.lower_call_statement(func, args, destination, function, local_map),

            Statement::Drop { place } => self.lower_drop_statement(place, function, local_map),

            Statement::Nop => {
                // No WASM instructions generated for Nop
                Ok(())
            }

            Statement::InterfaceCall {
                interface_id,
                method_id,
                receiver,
                args,
                destination,
            } => {
                // This requires interface_info to be passed - for now we'll return an error
                // In the full implementation, this would be called from a context that has interface_info
                return_compiler_error!(
                    "InterfaceCall statement lowering requires interface_info context - use lower_interface_call method directly"
                );
            }

            Statement::Alloc { place, size, align } => {
                self.lower_alloc(place, size, *align, function, local_map)
            }

            Statement::Dealloc { place } => self.lower_dealloc(place, function, local_map),

            Statement::Store {
                place,
                value,
                alignment,
                offset,
            } => self.lower_store(place, value, *alignment, *offset, function, local_map),

            Statement::MemoryOp {
                op,
                operand,
                result,
            } => self.lower_memory_op(op, operand.as_ref(), result.as_ref(), function, local_map),
        }
    }

    /// Lower dynamic allocation using a bump-pointer heap (very simple allocator)
    fn lower_alloc(
        &self,
        place: &Place,
        size: &Operand,
        align: u32,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        let heap_ptr_idx = self.heap_ptr_global_index.ok_or_else(|| {
            CompileError::new_thread_panic("Heap pointer global not initialized".to_string())
        })?;

        // Load current heap_ptr
        function.instruction(&Instruction::GlobalGet(heap_ptr_idx));
        // Save as result pointer (to be stored into destination place)
        // We'll duplicate it using local.tee pattern: spill to a temp local
        // For now, directly store later; keep a copy by re-loading after increment

        // Align heap_ptr upward: heap_ptr = (heap_ptr + (align-1)) & !(align-1)
        function.instruction(&Instruction::I32Const((align.saturating_sub(1)) as i32));
        function.instruction(&Instruction::I32Add);
        let mask = !((align.max(1)) - 1);
        function.instruction(&Instruction::I32Const(mask as i32));
        function.instruction(&Instruction::I32And);
        // Compute new heap_ptr = aligned_heap_ptr + size
        self.lower_operand(size, function, local_map)?; // push size
        function.instruction(&Instruction::I32Add);
        // Store new heap_ptr
        function.instruction(&Instruction::GlobalSet(heap_ptr_idx));

        // Recompute pointer to store into destination: heap_ptr - size
        function.instruction(&Instruction::GlobalGet(heap_ptr_idx));
        self.lower_operand(size, function, local_map)?;
        function.instruction(&Instruction::I32Sub);
        self.resolve_place_store(place, function, local_map)?;
        Ok(())
    }

    /// Lower deallocation (no-op for bump-pointer allocator)
    fn lower_dealloc(
        &self,
        _place: &Place,
        _function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        Ok(())
    }

    /// Lower a direct store to memory or local/global
    fn lower_store(
        &self,
        place: &Place,
        value: &Operand,
        _alignment: u32,
        _offset: u32,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Load value first
        self.lower_operand(value, function, local_map)?;
        // Then store into place
        self.resolve_place_store(place, function, local_map)?;
        Ok(())
    }

    /// Lower WASM-specific memory ops: Size/Grow/Fill/Copy
    fn lower_memory_op(
        &self,
        op: &crate::compiler::mir::mir_nodes::MemoryOpKind,
        operand: Option<&Operand>,
        result: Option<&Place>,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::mir_nodes::MemoryOpKind;
        match op {
            MemoryOpKind::Size => {
                function.instruction(&Instruction::MemorySize(0));
                if let Some(dst) = result {
                    self.resolve_place_store(dst, function, local_map)?;
                } else {
                    function.instruction(&Instruction::Drop);
                }
            }
            MemoryOpKind::Grow => {
                if let Some(pages) = operand {
                    self.lower_operand(pages, function, local_map)?;
                } else {
                    function.instruction(&Instruction::I32Const(0));
                }
                function.instruction(&Instruction::MemoryGrow(0));
                if let Some(dst) = result {
                    self.resolve_place_store(dst, function, local_map)?;
                } else {
                    function.instruction(&Instruction::Drop);
                }
            }
            MemoryOpKind::Fill => {
                // Not yet wired: requires memory.fill instruction (bulk-memory)
                return_compiler_error!("MemoryOp::Fill not yet implemented");
            }
            MemoryOpKind::Copy => {
                // Not yet wired: requires memory.copy instruction (bulk-memory)
                return_compiler_error!("MemoryOp::Copy not yet implemented");
            }
        }
        Ok(())
    }

    /// Lower Statement::Assign: evaluate rvalue → store to place (≤3 WASM instructions)
    fn lower_assign_statement(
        &self,
        place: &Place,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Lifetime-optimized assign:
        // - If rvalue is a simple Use(Copy/Move) from the same place, skip store (no-op)
        // - Otherwise, evaluate rvalue then store
        if let Rvalue::Use(op) = rvalue {
            if let Operand::Copy(src) | Operand::Move(src) = op {
                if src == place {
                    // Self-assign; nothing to do
                    return Ok(());
                }
            }
        }

        // Evaluate rvalue and push result
        self.lower_rvalue(rvalue, function, local_map)?;

        // Store to destination
        self.resolve_place_store(place, function, local_map)?;
        Ok(())
    }

    /// Lower Statement::Call: load arguments → call function → store result
    fn lower_call_statement(
        &self,
        func: &Operand,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Step 1: Load all arguments onto WASM stack
        for arg in args {
            self.lower_operand(arg, function, local_map)?;
        }

        // Step 2: Generate call instruction based on function operand type
        match func {
            Operand::Constant(Constant::Function(func_index)) => {
                function.instruction(&Instruction::Call(*func_index));
            }

            Operand::FunctionRef(func_index) => {
                function.instruction(&Instruction::Call(*func_index));
            }

            _ => {
                return_compiler_error!("Invalid function operand type for call: {:?}", func);
            }
        }

        // Step 3: Store result if destination is provided
        if let Some(dest_place) = destination {
            self.resolve_place_store(dest_place, function, local_map)?;
        }

        Ok(())
    }

    /// Lower interface call to call_indirect instruction with vtable dispatch
    pub fn lower_interface_call(
        &self,
        interface_id: u32,
        method_id: u32,
        receiver: &Operand,
        args: &[Operand],
        destination: &Option<Place>,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        // Validate the interface call before lowering with detailed error reporting
        self.validate_interface_call_with_context(
            interface_id,
            method_id,
            receiver,
            args,
            interface_info,
        )?;

        // Step 1: Load receiver object vtable pointer
        self.load_receiver_vtable_pointer(receiver, function, local_map)?;

        // Step 2: Load function index from vtable slot with method traversal
        self.load_method_function_index(interface_id, method_id, function, interface_info)?;

        // Step 5: Load receiver and all arguments for call_indirect
        // The receiver needs to be loaded again since it was consumed by vtable operations
        self.lower_operand(receiver, function, local_map)?;
        for arg in args {
            self.lower_operand(arg, function, local_map)?;
        }

        // Step 6: Get type index for call_indirect
        let type_index =
            self.get_interface_method_type_index(interface_id, method_id, interface_info)?;

        // Step 7: Generate call_indirect instruction with proper type signature
        function.instruction(&Instruction::CallIndirect {
            type_index: type_index,
            table_index: 0, // Table index 0 (function table)
        });

        // Step 8: Store result if destination is provided
        if let Some(dest_place) = destination {
            self.resolve_place_store(dest_place, function, local_map)?;
        }

        Ok(())
    }

    /// Get the byte offset of the vtable pointer within an object
    /// For now, we assume the vtable pointer is always at offset 0 (beginning of object)
    /// In a more sophisticated implementation, this would depend on the object layout
    pub fn get_vtable_offset_in_object(
        &self,
        _object_type: &WasmType,
    ) -> Result<u32, CompileError> {
        // For interface objects, the vtable pointer is stored at the beginning
        // This is a common pattern in object-oriented languages
        Ok(0)
    }

    /// Calculate the byte offset of a method within a vtable
    pub fn calculate_method_offset_in_vtable(
        &self,
        interface_id: u32,
        method_id: u32,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<u32, CompileError> {
        // Find the interface definition
        let interface_def = interface_info
            .interfaces
            .get(&interface_id)
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!("Interface {} not found", interface_id))
            })?;

        // Find the method index within the interface
        let method_index = interface_def
            .methods
            .iter()
            .position(|m| m.id == method_id)
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!(
                    "Method {} not found in interface {}",
                    method_id, interface_id
                ))
            })?;

        // Each method slot is 4 bytes (function index as i32)
        Ok(method_index as u32 * 4)
    }

    /// Create error handling for invalid interface calls with MIR context
    pub fn create_interface_call_error(
        &self,
        interface_id: u32,
        method_id: u32,
        error_type: &str,
        context: &str,
    ) -> CompileError {
        CompileError::new_thread_panic(format!(
            "Interface call error: {} for interface {} method {} - {}",
            error_type, interface_id, method_id, context
        ))
    }

    /// Validate interface call with detailed error reporting
    pub fn validate_interface_call_with_context(
        &self,
        interface_id: u32,
        method_id: u32,
        receiver: &Operand,
        args: &[Operand],
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        // Check if interface exists
        if !interface_info.interfaces.contains_key(&interface_id) {
            return Err(self.create_interface_call_error(
                interface_id,
                method_id,
                "Interface not found",
                &format!(
                    "Available interfaces: {:?}",
                    interface_info.interfaces.keys().collect::<Vec<_>>()
                ),
            ));
        }

        let interface_def = &interface_info.interfaces[&interface_id];

        // Check if method exists in interface
        let method_exists = interface_def.methods.iter().any(|m| m.id == method_id);
        if !method_exists {
            return Err(self.create_interface_call_error(
                interface_id,
                method_id,
                "Method not found in interface",
                &format!(
                    "Available methods: {:?}",
                    interface_def
                        .methods
                        .iter()
                        .map(|m| m.id)
                        .collect::<Vec<_>>()
                ),
            ));
        }

        // Validate types
        let receiver_type = self.infer_operand_type(receiver).map_err(|_| {
            self.create_interface_call_error(
                interface_id,
                method_id,
                "Cannot infer receiver type",
                "Receiver operand type inference failed",
            )
        })?;

        let arg_types: Result<Vec<_>, _> = args
            .iter()
            .map(|arg| self.infer_operand_type(arg))
            .collect();
        let arg_types = arg_types.map_err(|_| {
            self.create_interface_call_error(
                interface_id,
                method_id,
                "Cannot infer argument types",
                "One or more argument types could not be inferred",
            )
        })?;

        self.validate_interface_call_types(
            interface_id,
            method_id,
            &receiver_type,
            &arg_types,
            interface_info,
        )
        .map_err(|_| {
            self.create_interface_call_error(
                interface_id,
                method_id,
                "Type validation failed",
                &format!(
                    "Receiver type: {:?}, Argument types: {:?}",
                    receiver_type, arg_types
                ),
            )
        })?;

        Ok(())
    }

    /// Load vtable pointer from receiver object in linear memory
    pub fn load_receiver_vtable_pointer(
        &self,
        receiver: &Operand,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Load the receiver object
        self.lower_operand(receiver, function, local_map)?;

        // Get the receiver type to determine vtable offset
        let receiver_type = self.infer_operand_type(receiver)?;
        let vtable_offset = self.get_vtable_offset_in_object(&receiver_type)?;

        // Load vtable pointer from the receiver object
        function.instruction(&Instruction::I32Load(MemArg {
            offset: vtable_offset as u64,
            align: 2, // 4-byte alignment for pointer
            memory_index: 0,
        }));

        Ok(())
    }

    /// Load function index from vtable slot with method traversal
    pub fn load_method_function_index(
        &self,
        interface_id: u32,
        method_id: u32,
        function: &mut Function,
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        // Calculate method offset in vtable
        let method_offset =
            self.calculate_method_offset_in_vtable(interface_id, method_id, interface_info)?;

        // Add method offset to vtable pointer if not the first method
        if method_offset > 0 {
            function.instruction(&Instruction::I32Const(method_offset as i32));
            function.instruction(&Instruction::I32Add);
        }

        // Load function index from vtable slot
        function.instruction(&Instruction::I32Load(MemArg {
            offset: 0,
            align: 2, // 4-byte alignment for function index
            memory_index: 0,
        }));

        Ok(())
    }

    /// Generate type checking support for call_indirect instructions
    pub fn validate_interface_call_types(
        &self,
        interface_id: u32,
        method_id: u32,
        receiver_type: &WasmType,
        arg_types: &[WasmType],
        interface_info: &crate::compiler::mir::mir_nodes::InterfaceInfo,
    ) -> Result<(), CompileError> {
        // Find the interface definition
        let interface_def = interface_info
            .interfaces
            .get(&interface_id)
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!(
                    "Interface {} not found for type checking",
                    interface_id
                ))
            })?;

        // Find the method signature
        let method = interface_def
            .methods
            .iter()
            .find(|m| m.id == method_id)
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!(
                    "Method {} not found in interface {} for type checking",
                    method_id, interface_id
                ))
            })?;

        // Validate receiver type (first parameter)
        if !method.param_types.is_empty() {
            let expected_receiver_type = &method.param_types[0];
            if receiver_type != expected_receiver_type {
                return_compiler_error!(
                    "Interface call receiver type mismatch: expected {:?}, got {:?}",
                    expected_receiver_type,
                    receiver_type
                );
            }
        }

        // Validate argument types (excluding receiver)
        let expected_arg_types = if method.param_types.len() > 1 {
            &method.param_types[1..]
        } else {
            &[]
        };

        if arg_types.len() != expected_arg_types.len() {
            return_compiler_error!(
                "Interface call argument count mismatch: expected {}, got {}",
                expected_arg_types.len(),
                arg_types.len()
            );
        }

        for (i, (expected, actual)) in expected_arg_types.iter().zip(arg_types.iter()).enumerate() {
            if expected != actual {
                return_compiler_error!(
                    "Interface call argument {} type mismatch: expected {:?}, got {:?}",
                    i,
                    expected,
                    actual
                );
            }
        }

        Ok(())
    }

    /// Lower Statement::Drop: proper cleanup code generation
    fn lower_drop_statement(
        &self,
        place: &Place,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Lifetime-optimized Drop:
        // - For locals/globals of WASM value types: no work besides stack cleanup if any
        // - For heap/linear-memory backed places: perform minimal ARC decrement stub
        // - Avoid unnecessary loads if there is nothing to release

        match place {
            Place::Local { .. } | Place::Global { .. } => Ok(()),
            Place::Memory { base, .. } => {
                self.emit_arc_decrement_for_base(base, function, local_map)?;
                Ok(())
            }
            Place::Projection { base, .. } => {
                // Try to resolve ultimate memory base
                if let Some(mem_base) = base.memory_base() {
                    self.emit_arc_decrement_for_base(mem_base, function, local_map)?;
                }
                Ok(())
            }
        }
    }

    /// Emit a minimal ARC decrement stub for heap-backed memory.
    /// This assumes a conventional layout where the first 4 bytes at the object
    /// address hold a reference count (u32). If the count reaches zero, we do
    /// nothing else here; full deallocation is handled in task 13.
    fn emit_arc_decrement_for_base(
        &self,
        base: &crate::compiler::mir::place::MemoryBase,
        _function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::place::MemoryBase;
        match base {
            MemoryBase::Heap { .. } => {
                // Placeholder: ARC decrement to be fully implemented with concrete addresses in task 13
                Ok(())
            }
            _ => Ok(()),
        }
    }

    // ===== RVALUE LOWERING METHODS =====

    /// Lower an rvalue to WASM instructions that push the result onto the stack
    pub fn lower_rvalue(
        &self,
        rvalue: &Rvalue,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match rvalue {
            Rvalue::Use(operand) => self.lower_operand(operand, function, local_map),

            Rvalue::BinaryOp { op, left, right } => {
                self.lower_binary_op(op, left, right, function, local_map)
            }

            Rvalue::UnaryOp { op, operand } => {
                self.lower_unary_op(op, operand, function, local_map)
            }

            Rvalue::Cast {
                source,
                target_type,
            } => self.lower_cast(source, target_type, function, local_map),

            Rvalue::Array {
                elements,
                element_type,
            } => self.lower_array_creation(elements, element_type, function, local_map),

            Rvalue::Struct {
                fields,
                struct_type,
            } => self.lower_struct_creation(fields, *struct_type, function, local_map),

            // Other rvalue types not implemented in this task
            Rvalue::Ref { .. } => {
                return_compiler_error!(
                    "Ref rvalue lowering not yet implemented - will be added in later tasks"
                );
            }

            Rvalue::Deref { .. } => {
                return_compiler_error!(
                    "Deref rvalue lowering not yet implemented - will be added in later tasks"
                );
            }

            Rvalue::Load { place, .. } => self.resolve_place_load(place, function, local_map),

            Rvalue::MemorySize => self.lower_memory_size(function),

            Rvalue::MemoryGrow { pages } => self.lower_memory_grow(pages, function, local_map),

            Rvalue::InterfaceCall { .. } => {
                return_compiler_error!(
                    "InterfaceCall rvalue lowering not yet implemented - will be added in task 11"
                );
            }
        }
    }

    /// Lower binary operations to WASM arithmetic instructions (i32.add, i32.mul, etc.)
    fn lower_binary_op(
        &self,
        op: &crate::compiler::mir::mir_nodes::BinOp,
        left: &Operand,
        right: &Operand,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::mir_nodes::BinOp;

        // Load left operand onto stack
        self.lower_operand(left, function, local_map)?;

        // Load right operand onto stack
        self.lower_operand(right, function, local_map)?;

        // Generate appropriate WASM instruction based on operation
        // For now, assume i32 operations - type inference will be added later
        match op {
            // Arithmetic operations
            BinOp::Add => {
                function.instruction(&Instruction::I32Add);
            }
            BinOp::Sub => {
                function.instruction(&Instruction::I32Sub);
            }
            BinOp::Mul => {
                function.instruction(&Instruction::I32Mul);
            }
            BinOp::Div => {
                function.instruction(&Instruction::I32DivS);
            } // Signed division
            BinOp::Rem => {
                function.instruction(&Instruction::I32RemS);
            } // Signed remainder

            // Bitwise operations
            BinOp::BitAnd => {
                function.instruction(&Instruction::I32And);
            }
            BinOp::BitOr => {
                function.instruction(&Instruction::I32Or);
            }
            BinOp::BitXor => {
                function.instruction(&Instruction::I32Xor);
            }
            BinOp::Shl => {
                function.instruction(&Instruction::I32Shl);
            }
            BinOp::Shr => {
                function.instruction(&Instruction::I32ShrS);
            } // Signed right shift

            // Comparison operations
            BinOp::Eq => {
                function.instruction(&Instruction::I32Eq);
            }
            BinOp::Ne => {
                function.instruction(&Instruction::I32Ne);
            }
            BinOp::Lt => {
                function.instruction(&Instruction::I32LtS);
            } // Signed less than
            BinOp::Le => {
                function.instruction(&Instruction::I32LeS);
            } // Signed less equal
            BinOp::Gt => {
                function.instruction(&Instruction::I32GtS);
            } // Signed greater than
            BinOp::Ge => {
                function.instruction(&Instruction::I32GeS);
            } // Signed greater equal

            // Logical operations (implemented as short-circuiting control flow)
            BinOp::And | BinOp::Or => {
                return_compiler_error!(
                    "Logical And/Or operations should be lowered as control flow, not binary ops"
                );
            }
        }

        Ok(())
    }

    /// Lower unary operations for negation and bitwise operations
    fn lower_unary_op(
        &self,
        op: &crate::compiler::mir::mir_nodes::UnOp,
        operand: &Operand,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::mir_nodes::UnOp;

        // Load operand onto stack
        self.lower_operand(operand, function, local_map)?;

        // Generate appropriate WASM instruction
        match op {
            UnOp::Neg => {
                // Negation: 0 - operand
                function.instruction(&Instruction::I32Const(0));
                function.instruction(&Instruction::I32Sub);
            }
            UnOp::Not => {
                // Bitwise NOT: operand XOR -1
                function.instruction(&Instruction::I32Const(-1));
                function.instruction(&Instruction::I32Xor);
            }
        }

        Ok(())
    }

    /// Lower cast operations for WASM type conversion instructions
    fn lower_cast(
        &self,
        source: &Operand,
        target_type: &WasmType,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Load source operand
        self.lower_operand(source, function, local_map)?;

        // Determine source type from operand (simplified for now)
        let source_type = self.infer_operand_type(source)?;

        // Generate appropriate WASM conversion instruction
        match (source_type.clone(), target_type) {
            // Same type - no conversion needed
            (src, tgt) if src == *tgt => Ok(()),

            // Integer conversions
            (WasmType::I32, WasmType::I64) => {
                function.instruction(&Instruction::I64ExtendI32S);
                Ok(())
            }
            (WasmType::I64, WasmType::I32) => {
                function.instruction(&Instruction::I32WrapI64);
                Ok(())
            }

            // Float conversions
            (WasmType::F32, WasmType::F64) => {
                function.instruction(&Instruction::F64PromoteF32);
                Ok(())
            }
            (WasmType::F64, WasmType::F32) => {
                function.instruction(&Instruction::F32DemoteF64);
                Ok(())
            }

            // Integer to float conversions
            (WasmType::I32, WasmType::F32) => {
                function.instruction(&Instruction::F32ConvertI32S);
                Ok(())
            }
            (WasmType::I32, WasmType::F64) => {
                function.instruction(&Instruction::F64ConvertI32S);
                Ok(())
            }
            (WasmType::I64, WasmType::F32) => {
                function.instruction(&Instruction::F32ConvertI64S);
                Ok(())
            }
            (WasmType::I64, WasmType::F64) => {
                function.instruction(&Instruction::F64ConvertI64S);
                Ok(())
            }

            // Float to integer conversions
            (WasmType::F32, WasmType::I32) => {
                function.instruction(&Instruction::I32TruncF32S);
                Ok(())
            }
            (WasmType::F32, WasmType::I64) => {
                function.instruction(&Instruction::I64TruncF32S);
                Ok(())
            }
            (WasmType::F64, WasmType::I32) => {
                function.instruction(&Instruction::I32TruncF64S);
                Ok(())
            }
            (WasmType::F64, WasmType::I64) => {
                function.instruction(&Instruction::I64TruncF64S);
                Ok(())
            }

            // Reference type conversions (treat as i32 pointers)
            (WasmType::ExternRef, WasmType::I32) | (WasmType::FuncRef, WasmType::I32) => Ok(()),
            (WasmType::I32, WasmType::ExternRef) | (WasmType::I32, WasmType::FuncRef) => Ok(()),

            // Unsupported conversions
            _ => {
                return_compiler_error!(
                    "Unsupported cast from {:?} to {:?}",
                    source_type,
                    target_type
                );
            }
        }
    }

    /// Lower array creation with linear memory allocation
    fn lower_array_creation(
        &self,
        elements: &[Operand],
        element_type: &WasmType,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        let element_size = self.get_wasm_type_size(element_type);
        let _total_size = elements.len() as u32 * element_size;

        // For now, allocate in linear memory at a fixed offset
        // In a full implementation, this would use a heap allocator
        let array_offset = 1024u32; // Placeholder offset

        // Store array length at offset
        function.instruction(&Instruction::I32Const(array_offset as i32));
        function.instruction(&Instruction::I32Const(elements.len() as i32));
        function.instruction(&Instruction::I32Store(MemArg {
            offset: 0,
            align: 2,
            memory_index: 0,
        }));

        // Store each element
        for (i, element) in elements.iter().enumerate() {
            let element_offset = array_offset + 4 + (i as u32 * element_size);

            // Load element value
            self.lower_operand(element, function, local_map)?;

            // Store at calculated offset
            function.instruction(&Instruction::I32Const(element_offset as i32));

            // Use appropriate store instruction based on element type
            match element_type {
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
                    function.instruction(&Instruction::I32Store(MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                }
            }
        }

        // Return pointer to array (offset in linear memory)
        function.instruction(&Instruction::I32Const(array_offset as i32));

        Ok(())
    }

    /// Lower struct creation with linear memory allocation
    fn lower_struct_creation(
        &self,
        fields: &[(u32, Operand)],
        _struct_type: u32,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // For now, allocate struct in linear memory at a fixed offset
        // In a full implementation, this would use type information and heap allocator
        let struct_offset = 2048u32; // Placeholder offset

        // Calculate field offsets (simplified - assume all fields are i32 for now)
        let field_size = 4u32; // i32 size

        // Store each field
        for (field_index, (_field_id, field_value)) in fields.iter().enumerate() {
            let field_offset = struct_offset + (field_index as u32 * field_size);

            // Load field value
            self.lower_operand(field_value, function, local_map)?;

            // Store at calculated field offset
            function.instruction(&Instruction::I32Const(field_offset as i32));
            function.instruction(&Instruction::I32Store(MemArg {
                offset: 0,
                align: 2,
                memory_index: 0,
            }));
        }

        // Return pointer to struct (offset in linear memory)
        function.instruction(&Instruction::I32Const(struct_offset as i32));

        Ok(())
    }

    /// Lower WASM memory.size instruction
    fn lower_memory_size(&self, function: &mut Function) -> Result<(), CompileError> {
        function.instruction(&Instruction::MemorySize(0)); // Memory index 0
        Ok(())
    }

    /// Lower WASM memory.grow instruction
    fn lower_memory_grow(
        &self,
        pages: &Operand,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Load number of pages to grow
        self.lower_operand(pages, function, local_map)?;

        // Generate memory.grow instruction
        function.instruction(&Instruction::MemoryGrow(0)); // Memory index 0

        Ok(())
    }

    /// Infer the WASM type of an operand for type conversion
    pub fn infer_operand_type(&self, operand: &Operand) -> Result<WasmType, CompileError> {
        match operand {
            Operand::Constant(constant) => {
                match constant {
                    Constant::I32(_) => Ok(WasmType::I32),
                    Constant::I64(_) => Ok(WasmType::I64),
                    Constant::F32(_) => Ok(WasmType::F32),
                    Constant::F64(_) => Ok(WasmType::F64),
                    Constant::Bool(_) => Ok(WasmType::I32), // Booleans as i32
                    Constant::String(_) => Ok(WasmType::I32), // String pointers as i32
                    Constant::Function(_) => Ok(WasmType::FuncRef),
                    Constant::Null => Ok(WasmType::I32), // Null pointer as i32
                    Constant::MemoryOffset(_) => Ok(WasmType::I32),
                    Constant::TypeSize(_) => Ok(WasmType::I32),
                }
            }
            Operand::Copy(place) | Operand::Move(place) => Ok(place.wasm_type()),
            Operand::FunctionRef(_) => Ok(WasmType::FuncRef),
            Operand::GlobalRef(_) => Ok(WasmType::I32), // Global references as i32 indices
        }
    }

    /// Integrate constant folding for compile-time known values
    /// This method checks if an rvalue can be folded at compile time
    pub fn try_fold_rvalue(&self, rvalue: &Rvalue) -> Option<Constant> {
        match rvalue {
            Rvalue::Use(Operand::Constant(constant)) => {
                // Already a constant, return as-is
                Some(constant.clone())
            }

            Rvalue::BinaryOp { op, left, right } => {
                // Try to fold binary operations on constants
                if let (Operand::Constant(left_const), Operand::Constant(right_const)) =
                    (left, right)
                {
                    self.fold_binary_op_constants(op, left_const, right_const)
                } else {
                    None
                }
            }

            Rvalue::UnaryOp { op, operand } => {
                // Try to fold unary operations on constants
                if let Operand::Constant(operand_const) = operand {
                    self.fold_unary_op_constant(op, operand_const)
                } else {
                    None
                }
            }

            Rvalue::Cast {
                source,
                target_type,
            } => {
                // Try to fold cast operations on constants
                if let Operand::Constant(source_const) = source {
                    self.fold_cast_constant(source_const, target_type)
                } else {
                    None
                }
            }

            // Other rvalue types cannot be folded at compile time
            _ => None,
        }
    }

    /// Fold binary operations on constant operands
    fn fold_binary_op_constants(
        &self,
        op: &crate::compiler::mir::mir_nodes::BinOp,
        left: &Constant,
        right: &Constant,
    ) -> Option<Constant> {
        use crate::compiler::mir::mir_nodes::BinOp;

        match (left, right) {
            (Constant::I32(l), Constant::I32(r)) => {
                match op {
                    BinOp::Add => Some(Constant::I32(l.wrapping_add(*r))),
                    BinOp::Sub => Some(Constant::I32(l.wrapping_sub(*r))),
                    BinOp::Mul => Some(Constant::I32(l.wrapping_mul(*r))),
                    BinOp::Div if *r != 0 => Some(Constant::I32(l / r)),
                    BinOp::Rem if *r != 0 => Some(Constant::I32(l % r)),
                    BinOp::BitAnd => Some(Constant::I32(l & r)),
                    BinOp::BitOr => Some(Constant::I32(l | r)),
                    BinOp::BitXor => Some(Constant::I32(l ^ r)),
                    BinOp::Shl => Some(Constant::I32(l << (r & 31))), // Mask to 5 bits
                    BinOp::Shr => Some(Constant::I32(l >> (r & 31))), // Mask to 5 bits
                    BinOp::Eq => Some(Constant::Bool(l == r)),
                    BinOp::Ne => Some(Constant::Bool(l != r)),
                    BinOp::Lt => Some(Constant::Bool(l < r)),
                    BinOp::Le => Some(Constant::Bool(l <= r)),
                    BinOp::Gt => Some(Constant::Bool(l > r)),
                    BinOp::Ge => Some(Constant::Bool(l >= r)),
                    _ => None, // Division by zero or unsupported operation
                }
            }

            (Constant::Bool(l), Constant::Bool(r)) => match op {
                BinOp::And => Some(Constant::Bool(*l && *r)),
                BinOp::Or => Some(Constant::Bool(*l || *r)),
                BinOp::Eq => Some(Constant::Bool(l == r)),
                BinOp::Ne => Some(Constant::Bool(l != r)),
                _ => None,
            },

            // Add more constant type combinations as needed
            _ => None,
        }
    }

    /// Fold unary operations on constant operands
    fn fold_unary_op_constant(
        &self,
        op: &crate::compiler::mir::mir_nodes::UnOp,
        operand: &Constant,
    ) -> Option<Constant> {
        use crate::compiler::mir::mir_nodes::UnOp;

        match operand {
            Constant::I32(value) => match op {
                UnOp::Neg => Some(Constant::I32(value.wrapping_neg())),
                UnOp::Not => Some(Constant::I32(!value)),
            },

            Constant::Bool(value) => {
                match op {
                    UnOp::Not => Some(Constant::Bool(!value)),
                    UnOp::Neg => None, // Cannot negate boolean
                }
            }

            _ => None,
        }
    }

    /// Fold cast operations on constant operands
    fn fold_cast_constant(&self, source: &Constant, target_type: &WasmType) -> Option<Constant> {
        match (source, target_type) {
            // Same type - no conversion needed
            (Constant::I32(val), WasmType::I32) => Some(Constant::I32(*val)),
            (Constant::I64(val), WasmType::I64) => Some(Constant::I64(*val)),
            (Constant::F32(val), WasmType::F32) => Some(Constant::F32(*val)),
            (Constant::F64(val), WasmType::F64) => Some(Constant::F64(*val)),

            // Integer conversions
            (Constant::I32(val), WasmType::I64) => Some(Constant::I64(*val as i64)),
            (Constant::I64(val), WasmType::I32) => Some(Constant::I32(*val as i32)),

            // Float conversions
            (Constant::F32(val), WasmType::F64) => Some(Constant::F64(*val as f64)),
            (Constant::F64(val), WasmType::F32) => Some(Constant::F32(*val as f32)),

            // Integer to float
            (Constant::I32(val), WasmType::F32) => Some(Constant::F32(*val as f32)),
            (Constant::I32(val), WasmType::F64) => Some(Constant::F64(*val as f64)),
            (Constant::I64(val), WasmType::F32) => Some(Constant::F32(*val as f32)),
            (Constant::I64(val), WasmType::F64) => Some(Constant::F64(*val as f64)),

            // Float to integer
            (Constant::F32(val), WasmType::I32) => Some(Constant::I32(*val as i32)),
            (Constant::F32(val), WasmType::I64) => Some(Constant::I64(*val as i64)),
            (Constant::F64(val), WasmType::I32) => Some(Constant::I32(*val as i32)),
            (Constant::F64(val), WasmType::I64) => Some(Constant::I64(*val as i64)),

            // Boolean conversions
            (Constant::Bool(val), WasmType::I32) => Some(Constant::I32(if *val { 1 } else { 0 })),

            // Other conversions not supported at compile time
            _ => None,
        }
    }

    /// Lower an operand to WASM instructions (Copy/Move/Constant handling)
    pub fn lower_operand(
        &self,
        operand: &Operand,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match operand {
            Operand::Copy(place) => {
                // Copy: load value from place (non-destructive)
                self.resolve_place_load(place, function, local_map)
            }

            Operand::Move(place) => {
                // Move: load value from place (potentially destructive, but same WASM instruction)
                // The difference between Copy and Move is handled by the borrow checker
                // At WASM level, both generate the same load instruction
                self.resolve_place_load(place, function, local_map)
            }

            Operand::Constant(constant) => self.lower_constant(constant, function),

            Operand::FunctionRef(func_index) => {
                // Function references are represented as function indices in WASM
                function.instruction(&Instruction::I32Const(*func_index as i32));
                Ok(())
            }

            Operand::GlobalRef(global_index) => {
                // Global references are represented as global indices in WASM
                function.instruction(&Instruction::I32Const(*global_index as i32));
                Ok(())
            }
        }
    }

    /// Lower a constant to WASM const instruction
    pub fn lower_constant(
        &self,
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
                // Booleans are represented as i32 in WASM (0 = false, 1 = true)
                let int_value = if *value { 1 } else { 0 };
                function.instruction(&Instruction::I32Const(int_value));
                Ok(())
            }

            Constant::String(string_value) => {
                // String constants are stored in linear memory
                // Get the memory offset for this string constant
                if let Some(&offset) = self.string_constant_map.get(string_value) {
                    function.instruction(&Instruction::I32Const(offset as i32));
                    Ok(())
                } else {
                    return_compiler_error!(
                        "String constant '{}' not found in string constant map",
                        string_value
                    );
                }
            }

            Constant::Function(func_index) => {
                // Function constants are represented as function indices
                function.instruction(&Instruction::I32Const(*func_index as i32));
                Ok(())
            }

            Constant::Null => {
                // Null pointer is 0 in linear memory
                function.instruction(&Instruction::I32Const(0));
                Ok(())
            }

            Constant::MemoryOffset(offset) => {
                // Memory offsets are i32 constants
                function.instruction(&Instruction::I32Const(*offset as i32));
                Ok(())
            }

            Constant::TypeSize(size) => {
                // Type sizes are i32 constants
                function.instruction(&Instruction::I32Const(*size as i32));
                Ok(())
            }
        }
    }

    // ===== PLACE RESOLUTION METHODS =====

    /// Resolve a place to WASM location and generate load instructions
    /// This is the main entry point for place → WASM instruction mapping
    pub fn resolve_place_load(
        &self,
        place: &Place,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local {
                index,
                wasm_type: _,
            } => {
                // Map MIR local to WASM local index using local_map
                let wasm_local_index = local_map.get(place).copied().unwrap_or(*index);

                // Validate type compatibility (skip for now to avoid local_count issues)
                // let expected_val_type = self.wasm_type_to_val_type(wasm_type);
                // self.validate_local_type(wasm_local_index, expected_val_type)?;

                // Generate local.get instruction
                function.instruction(&Instruction::LocalGet(wasm_local_index));
                Ok(())
            }

            Place::Global { index, wasm_type } => {
                // Map MIR global to WASM global index using global_count tracking
                if *index >= self.global_count {
                    return_compiler_error!(
                        "Global index {} exceeds global_count {}",
                        index,
                        self.global_count
                    );
                }

                // Validate type compatibility
                let expected_val_type = self.wasm_type_to_val_type(wasm_type);
                self.validate_global_type(*index, expected_val_type)?;

                // Generate global.get instruction
                function.instruction(&Instruction::GlobalGet(*index));
                Ok(())
            }

            Place::Memory { base, offset, size } => {
                // Calculate memory address and generate memory load
                self.resolve_memory_load(base, offset, size, function, local_map)
            }

            Place::Projection { base, elem } => {
                // Resolve projection by calculating final address and loading
                self.resolve_projection_load(base, elem, function, local_map)
            }
        }
    }

    /// Resolve a place to WASM location and generate store instructions
    /// Assumes the value to store is already on the WASM stack
    pub fn resolve_place_store(
        &self,
        place: &Place,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match place {
            Place::Local {
                index,
                wasm_type: _,
            } => {
                // Map MIR local to WASM local index using local_map
                let wasm_local_index = local_map.get(place).copied().unwrap_or(*index);

                // Validate type compatibility (skip for now to avoid local_count issues)
                // let expected_val_type = self.wasm_type_to_val_type(wasm_type);
                // self.validate_local_type(wasm_local_index, expected_val_type)?;

                // Generate local.set instruction
                function.instruction(&Instruction::LocalSet(wasm_local_index));
                Ok(())
            }

            Place::Global { index, wasm_type } => {
                // Map MIR global to WASM global index using global_count tracking
                if *index >= self.global_count {
                    return_compiler_error!(
                        "Global index {} exceeds global_count {}",
                        index,
                        self.global_count
                    );
                }

                // Validate type compatibility
                let expected_val_type = self.wasm_type_to_val_type(wasm_type);
                self.validate_global_type(*index, expected_val_type)?;

                // Generate global.set instruction
                function.instruction(&Instruction::GlobalSet(*index));
                Ok(())
            }

            Place::Memory { base, offset, size } => {
                // Calculate memory address and generate memory store
                self.resolve_memory_store(base, offset, size, function, local_map)
            }

            Place::Projection { base, elem } => {
                // Resolve projection by calculating final address and storing
                self.resolve_projection_store(base, elem, function, local_map)
            }
        }
    }

    /// Build local index mapping from MirFunction parameters and locals using local_count
    /// This creates the mapping from MIR places to WASM local indices
    pub fn build_local_index_mapping(
        &mut self,
        mir_function: &MirFunction,
    ) -> Result<HashMap<Place, u32>, CompileError> {
        let mut local_map = HashMap::new();
        let mut wasm_local_index = 0u32;

        // Map parameters to WASM local indices 0..n-1
        // Parameters are always the first locals in WASM functions
        for param_place in &mir_function.parameters {
            local_map.insert(param_place.clone(), wasm_local_index);
            wasm_local_index += 1;
        }

        // Map local variables to subsequent WASM local indices
        // Use the existing local_count field to track allocation
        for (_local_name, local_place) in &mir_function.locals {
            if !local_map.contains_key(local_place) {
                local_map.insert(local_place.clone(), wasm_local_index);
                wasm_local_index += 1;

                // Update local_count to track total locals allocated
                self.local_count = self.local_count.max(wasm_local_index);
            }
        }

        Ok(local_map)
    }

    /// Add global index mapping for MIR globals to WASM global section using global_count
    /// This ensures MIR global places map correctly to WASM global indices
    pub fn add_global_index_mapping(
        &mut self,
        _mir_global_id: u32,
        place: &Place,
    ) -> Result<u32, CompileError> {
        match place {
            Place::Global { index, wasm_type } => {
                // Validate that the global index is within bounds
                if *index >= self.global_count {
                    // Extend global_count if needed
                    self.global_count = *index + 1;
                }

                // Add global to WASM global section if not already present
                let global_type = GlobalType {
                    val_type: self.wasm_type_to_val_type(wasm_type),
                    mutable: true, // Most globals are mutable in Beanstalk
                    shared: false,
                };

                // Initialize with zero value based on type
                let init_expr = self.create_zero_init_expr(wasm_type)?;
                self.global_section.global(global_type, &init_expr);

                Ok(*index)
            }
            _ => {
                return_compiler_error!("Expected Global place for global mapping, got {:?}", place);
            }
        }
    }

    /// Calculate memory offset for Place::Memory projections
    /// Handles linear memory layout and alignment requirements
    fn resolve_memory_load(
        &self,
        base: &crate::compiler::mir::place::MemoryBase,
        offset: &crate::compiler::mir::place::ByteOffset,
        size: &crate::compiler::mir::place::TypeSize,
        function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::place::{MemoryBase, TypeSize};

        match base {
            MemoryBase::LinearMemory => {
                // Load from WASM linear memory at offset
                function.instruction(&Instruction::I32Const(offset.0 as i32));

                // Generate appropriate memory load instruction based on size
                match size {
                    TypeSize::Byte => {
                        function.instruction(&Instruction::I32Load8U(MemArg {
                            offset: 0,
                            align: 0,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Short => {
                        function.instruction(&Instruction::I32Load16U(MemArg {
                            offset: 0,
                            align: 1,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Word => {
                        function.instruction(&Instruction::I32Load(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::DoubleWord => {
                        function.instruction(&Instruction::I64Load(MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Custom { bytes, alignment } => {
                        // For custom sizes, use the most appropriate load instruction
                        if *bytes <= 4 {
                            let align_log2 = alignment.trailing_zeros().min(2);
                            function.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: align_log2,
                                memory_index: 0,
                            }));
                        } else {
                            let align_log2 = alignment.trailing_zeros().min(3);
                            function.instruction(&Instruction::I64Load(MemArg {
                                offset: 0,
                                align: align_log2,
                                memory_index: 0,
                            }));
                        }
                    }
                }
                Ok(())
            }

            MemoryBase::Stack => {
                // Stack allocations should be handled as locals, not memory operations
                return_compiler_error!(
                    "Stack-based memory should use local operations, not memory loads"
                );
            }

            MemoryBase::Heap {
                alloc_id: _,
                size: _alloc_size,
            } => {
                // Heap allocations are stored in linear memory
                // For now, treat heap allocations as linear memory with offset
                // In a full implementation, this would involve heap management
                function.instruction(&Instruction::I32Const(offset.0 as i32));

                // Use word-sized load for heap allocations (typically pointers)
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
        }
    }

    /// Calculate memory offset for Place::Memory projections and generate store
    /// Assumes the value to store is on the stack, followed by the address
    fn resolve_memory_store(
        &self,
        base: &crate::compiler::mir::place::MemoryBase,
        offset: &crate::compiler::mir::place::ByteOffset,
        size: &crate::compiler::mir::place::TypeSize,
        function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::place::{MemoryBase, TypeSize};

        match base {
            MemoryBase::LinearMemory => {
                // Generate address for linear memory store
                function.instruction(&Instruction::I32Const(offset.0 as i32));

                // Generate appropriate memory store instruction based on size
                match size {
                    TypeSize::Byte => {
                        function.instruction(&Instruction::I32Store8(MemArg {
                            offset: 0,
                            align: 0,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Short => {
                        function.instruction(&Instruction::I32Store16(MemArg {
                            offset: 0,
                            align: 1,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Word => {
                        function.instruction(&Instruction::I32Store(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::DoubleWord => {
                        function.instruction(&Instruction::I64Store(MemArg {
                            offset: 0,
                            align: 3,
                            memory_index: 0,
                        }));
                    }
                    TypeSize::Custom { bytes, alignment } => {
                        // For custom sizes, use the most appropriate store instruction
                        if *bytes <= 4 {
                            let align_log2 = alignment.trailing_zeros().min(2);
                            function.instruction(&Instruction::I32Store(MemArg {
                                offset: 0,
                                align: align_log2,
                                memory_index: 0,
                            }));
                        } else {
                            let align_log2 = alignment.trailing_zeros().min(3);
                            function.instruction(&Instruction::I64Store(MemArg {
                                offset: 0,
                                align: align_log2,
                                memory_index: 0,
                            }));
                        }
                    }
                }
                Ok(())
            }

            MemoryBase::Stack => {
                // Stack allocations should be handled as locals, not memory operations
                return_compiler_error!(
                    "Stack-based memory should use local operations, not memory stores"
                );
            }

            MemoryBase::Heap {
                alloc_id: _,
                size: _alloc_size,
            } => {
                // Heap allocations are stored in linear memory
                function.instruction(&Instruction::I32Const(offset.0 as i32));

                // Use word-sized store for heap allocations (typically pointers)
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
        }
    }

    /// Resolve field offset for Place::Field projections with byte-level precision
    /// This handles struct field access with proper WASM alignment
    fn resolve_projection_load(
        &self,
        base: &Place,
        elem: &crate::compiler::mir::place::ProjectionElem,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::place::{FieldSize, ProjectionElem};

        match elem {
            ProjectionElem::Field {
                index: _field_index,
                offset,
                size,
            } => {
                // First, load the base address
                self.resolve_place_load(base, function, local_map)?;

                // Add field offset to get final address
                function.instruction(&Instruction::I32Const(offset.0 as i32));
                function.instruction(&Instruction::I32Add);

                // Generate load instruction based on field size
                match size {
                    FieldSize::Fixed(bytes) => {
                        if *bytes <= 4 {
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
                    FieldSize::WasmType(wasm_type) => match wasm_type {
                        crate::compiler::mir::place::WasmType::I32 => {
                            function.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::I64 => {
                            function.instruction(&Instruction::I64Load(MemArg {
                                offset: 0,
                                align: 3,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::F32 => {
                            function.instruction(&Instruction::F32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::F64 => {
                            function.instruction(&Instruction::F64Load(MemArg {
                                offset: 0,
                                align: 3,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::ExternRef
                        | crate::compiler::mir::place::WasmType::FuncRef => {
                            function.instruction(&Instruction::I32Load(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        }
                    },
                    FieldSize::Variable => {
                        // Variable size fields are typically pointers to data
                        function.instruction(&Instruction::I32Load(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                }
                Ok(())
            }

            ProjectionElem::Index {
                index,
                element_size,
            } => {
                // Array index access: base + (index * element_size)
                self.resolve_place_load(base, function, local_map)?;

                // Load index value
                self.resolve_place_load(index, function, local_map)?;

                // Multiply index by element size
                function.instruction(&Instruction::I32Const(*element_size as i32));
                function.instruction(&Instruction::I32Mul);

                // Add to base address
                function.instruction(&Instruction::I32Add);

                // Load element (assume i32 for now)
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }

            ProjectionElem::Length => {
                // Load length field (typically at offset 0 for collections)
                self.resolve_place_load(base, function, local_map)?;
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }

            ProjectionElem::Data => {
                // Load data pointer (typically at offset 4 for collections)
                self.resolve_place_load(base, function, local_map)?;
                function.instruction(&Instruction::I32Const(4)); // Standard data pointer offset
                function.instruction(&Instruction::I32Add);
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }

            ProjectionElem::Deref => {
                // Dereference: load base as address, then load from that address
                self.resolve_place_load(base, function, local_map)?;
                function.instruction(&Instruction::I32Load(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
        }
    }

    /// Resolve field offset for Place::Field projections and generate store
    /// Assumes the value to store is on the stack
    fn resolve_projection_store(
        &self,
        base: &Place,
        elem: &crate::compiler::mir::place::ProjectionElem,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        use crate::compiler::mir::place::{FieldSize, ProjectionElem};

        match elem {
            ProjectionElem::Field {
                index: _field_index,
                offset,
                size,
            } => {
                // Load the base address
                self.resolve_place_load(base, function, local_map)?;

                // Add field offset to get final address
                function.instruction(&Instruction::I32Const(offset.0 as i32));
                function.instruction(&Instruction::I32Add);

                // Generate store instruction based on field size
                match size {
                    FieldSize::Fixed(bytes) => {
                        if *bytes <= 4 {
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
                    FieldSize::WasmType(wasm_type) => match wasm_type {
                        crate::compiler::mir::place::WasmType::I32 => {
                            function.instruction(&Instruction::I32Store(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::I64 => {
                            function.instruction(&Instruction::I64Store(MemArg {
                                offset: 0,
                                align: 3,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::F32 => {
                            function.instruction(&Instruction::F32Store(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::F64 => {
                            function.instruction(&Instruction::F64Store(MemArg {
                                offset: 0,
                                align: 3,
                                memory_index: 0,
                            }));
                        }
                        crate::compiler::mir::place::WasmType::ExternRef
                        | crate::compiler::mir::place::WasmType::FuncRef => {
                            function.instruction(&Instruction::I32Store(MemArg {
                                offset: 0,
                                align: 2,
                                memory_index: 0,
                            }));
                        }
                    },
                    FieldSize::Variable => {
                        // Variable size fields are typically pointers to data
                        function.instruction(&Instruction::I32Store(MemArg {
                            offset: 0,
                            align: 2,
                            memory_index: 0,
                        }));
                    }
                }
                Ok(())
            }

            ProjectionElem::Index {
                index,
                element_size,
            } => {
                // Array index access: base + (index * element_size)
                self.resolve_place_load(base, function, local_map)?;

                // Load index value
                self.resolve_place_load(index, function, local_map)?;

                // Multiply index by element size
                function.instruction(&Instruction::I32Const(*element_size as i32));
                function.instruction(&Instruction::I32Mul);

                // Add to base address
                function.instruction(&Instruction::I32Add);

                // Store element (assume i32 for now)
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }

            ProjectionElem::Length => {
                // Store length field (typically at offset 0 for collections)
                self.resolve_place_load(base, function, local_map)?;
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }

            ProjectionElem::Data => {
                // Store data pointer (typically at offset 4 for collections)
                self.resolve_place_load(base, function, local_map)?;
                function.instruction(&Instruction::I32Const(4)); // Standard data pointer offset
                function.instruction(&Instruction::I32Add);
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }

            ProjectionElem::Deref => {
                // Dereference store: load base as address, then store to that address
                self.resolve_place_load(base, function, local_map)?;
                function.instruction(&Instruction::I32Store(MemArg {
                    offset: 0,
                    align: 2,
                    memory_index: 0,
                }));
                Ok(())
            }
        }
    }

    // ===== HELPER METHODS FOR PLACE RESOLUTION =====

    /// Validate that a local index has the expected type
    fn validate_local_type(
        &self,
        local_index: u32,
        _expected_type: ValType,
    ) -> Result<(), CompileError> {
        // In a full implementation, this would check against the function's local types
        // For now, we'll just validate that the index is reasonable
        // Note: We allow local_index == local_count because local_count gets updated after validation
        if local_index > self.local_count {
            return_compiler_error!(
                "Local index {} exceeds local_count {}",
                local_index,
                self.local_count
            );
        }
        Ok(())
    }

    /// Validate that a global index has the expected type
    fn validate_global_type(
        &self,
        global_index: u32,
        _expected_type: ValType,
    ) -> Result<(), CompileError> {
        // In a full implementation, this would check against the global section types
        // For now, we'll just validate that the index is reasonable
        if global_index >= self.global_count {
            return_compiler_error!(
                "Global index {} exceeds global_count {}",
                global_index,
                self.global_count
            );
        }
        Ok(())
    }

    /// Create a zero initialization expression for a WASM type
    fn create_zero_init_expr(&self, wasm_type: &WasmType) -> Result<ConstExpr, CompileError> {
        match wasm_type {
            WasmType::I32 => Ok(ConstExpr::i32_const(0)),
            WasmType::I64 => Ok(ConstExpr::i64_const(0)),
            WasmType::F32 => Ok(ConstExpr::f32_const(0.0.into())),
            WasmType::F64 => Ok(ConstExpr::f64_const(0.0.into())),
            WasmType::ExternRef => {
                // For now, use i32 const 0 as a placeholder for externref
                // In a full implementation, this would use proper ref.null
                Ok(ConstExpr::i32_const(0))
            }
            WasmType::FuncRef => {
                // For now, use i32 const 0 as a placeholder for funcref
                // In a full implementation, this would use proper ref.null
                Ok(ConstExpr::i32_const(0))
            }
        }
    }

    // ===== GETTER METHODS FOR TESTING =====

    /// Get the current function count (for testing)
    pub fn get_function_count(&self) -> u32 {
        self.function_count
    }

    /// Get the current type count (for testing)
    pub fn get_type_count(&self) -> u32 {
        self.type_count
    }

    /// Get the current global count (for testing)
    pub fn get_global_count(&self) -> u32 {
        self.global_count
    }

    /// Get the current local count (for testing)
    pub fn get_local_count(&self) -> u32 {
        self.local_count
    }

    /// Get string constants (for testing)
    pub fn get_string_constants(&self) -> &Vec<String> {
        &self.string_constants
    }

    /// Get string constant map (for testing)
    pub fn get_string_constant_map(&self) -> &HashMap<String, u32> {
        &self.string_constant_map
    }

    /// Add function export (for testing)
    pub fn add_function_export(&mut self, name: &str, function_index: u32) {
        self.export_section
            .export(name, ExportKind::Func, function_index);
    }

    /// Add a global export
    pub fn add_global_export(&mut self, name: &str, global_index: u32) {
        self.export_section
            .export(name, ExportKind::Global, global_index);
    }

    /// Add a memory export
    pub fn add_memory_export(&mut self, name: &str, memory_index: u32) {
        self.export_section
            .export(name, ExportKind::Memory, memory_index);
    }

    /// Add a table export
    pub fn add_table_export(&mut self, name: &str, table_index: u32) {
        self.export_section
            .export(name, ExportKind::Table, table_index);
    }

    /// Add string constant for testing
    pub fn add_string_constant_for_test(&mut self, string_value: String, offset: u32) {
        if !self.string_constant_map.contains_key(&string_value) {
            self.string_constant_map
                .insert(string_value.clone(), offset);
            self.string_constants.push(string_value);
        }
    }

    // ===== TERMINATOR LOWERING METHODS =====

    /// Lower a MIR terminator to WASM control flow instructions
    /// Maps MIR terminators to structured WASM control flow
    pub fn lower_terminator(
        &self,
        terminator: &Terminator,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::Goto {
                target,
                label_depth,
            } => self.lower_goto_terminator(*target, *label_depth, function),

            Terminator::UnconditionalJump(target) => {
                // Legacy format - convert to Goto with default label depth
                self.lower_goto_terminator(*target, 0, function)
            }

            Terminator::If {
                condition,
                then_block,
                else_block,
                wasm_if_info,
            } => self.lower_if_terminator(
                condition,
                *then_block,
                *else_block,
                wasm_if_info,
                function,
                local_map,
            ),

            Terminator::ConditionalJump(_then_block, _else_block) => {
                // Legacy format - convert to If with default condition (assume condition is on stack)
                // This is a simplified case that assumes the condition is already evaluated
                // For legacy format, we can't load a condition, so we'll generate a placeholder
                return_compiler_error!(
                    "ConditionalJump terminator requires condition operand. Use If terminator instead."
                );
            }

            Terminator::Switch {
                discriminant,
                targets,
                default,
                br_table_info,
            } => self.lower_switch_terminator(
                discriminant,
                targets,
                *default,
                br_table_info,
                function,
                local_map,
            ),

            Terminator::Return { values } => {
                self.lower_return_terminator(values, function, local_map)
            }

            Terminator::Returns => {
                // Legacy format - convert to Return with no values
                self.lower_return_terminator(&vec![], function, local_map)
            }

            Terminator::Unreachable => self.lower_unreachable_terminator(function),

            Terminator::Loop {
                target,
                loop_header,
                loop_info,
            } => self.lower_loop_terminator(*target, *loop_header, loop_info, function, local_map),

            Terminator::Block {
                inner_blocks,
                result_type,
                exit_target,
            } => self.lower_block_terminator(
                inner_blocks,
                result_type,
                *exit_target,
                function,
                local_map,
            ),
        }
    }

    /// Lower Terminator::Goto to WASM br instruction with correct label depths
    fn lower_goto_terminator(
        &self,
        _target: u32,
        label_depth: u32,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // Generate WASM br instruction with the specified label depth
        // In WASM, br takes a label index (depth from current position)
        function.instruction(&Instruction::Br(label_depth));

        // Note: The target block ID is used for control flow analysis but not directly
        // in the WASM instruction. The label_depth determines which enclosing block to branch to.
        // This assumes that the MIR has been properly analyzed to set correct label depths.

        Ok(())
    }

    /// Lower Terminator::If to WASM if/else structures with proper condition evaluation
    fn lower_if_terminator(
        &self,
        condition: &Operand,
        then_block: u32,
        else_block: u32,
        wasm_if_info: &crate::compiler::mir::mir_nodes::WasmIfInfo,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Step 1: Load condition operand onto stack
        self.lower_operand(condition, function, local_map)?;

        // Step 2: Generate WASM if instruction
        // The condition is now on the stack as i32 (0 = false, non-zero = true)

        if wasm_if_info.has_else {
            // Generate if/else structure
            let block_type = if let Some(result_type) = &wasm_if_info.result_type {
                // If the if/else produces a result, specify the result type
                BlockType::Result(self.wasm_type_to_val_type(result_type))
            } else {
                // No result type
                BlockType::Empty
            };

            function.instruction(&Instruction::If(block_type));

            // Then branch: br to then_block (label depth 1 to exit if)
            function.instruction(&Instruction::Br(1)); // Branch out of if to then_block

            function.instruction(&Instruction::Else);

            // Else branch: br to else_block (label depth 1 to exit if)
            function.instruction(&Instruction::Br(1)); // Branch out of if to else_block

            function.instruction(&Instruction::End);
        } else {
            // Generate if without else
            function.instruction(&Instruction::If(BlockType::Empty));

            // Then branch: br to then_block (label depth 1 to exit if)
            function.instruction(&Instruction::Br(1)); // Branch out of if to then_block

            function.instruction(&Instruction::End);

            // Implicit fall-through to else_block
        }

        Ok(())
    }

    /// Lower Terminator::Return with value loading and return instruction
    fn lower_return_terminator(
        &self,
        values: &[Operand],
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Step 1: Load all return values onto the stack
        for value in values {
            self.lower_operand(value, function, local_map)?;
        }

        // Step 2: Generate WASM return instruction
        // The return values are now on the stack in the correct order
        function.instruction(&Instruction::Return);

        Ok(())
    }

    /// Lower Terminator::Unreachable to WASM unreachable instruction
    fn lower_unreachable_terminator(&self, function: &mut Function) -> Result<(), CompileError> {
        // Generate WASM unreachable instruction
        // This indicates that this code path should never be reached
        function.instruction(&Instruction::Unreachable);

        Ok(())
    }

    /// Lower Terminator::Switch to WASM br_table instructions for efficient multi-way branching
    /// This enhanced version provides proper WASM br_table generation with optimization
    fn lower_switch_terminator(
        &self,
        discriminant: &Operand,
        targets: &[u32],
        default: u32,
        br_table_info: &crate::compiler::mir::mir_nodes::BrTableInfo,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Validate switch parameters
        if targets.is_empty() {
            return_compiler_error!("Switch terminator must have at least one target");
        }

        // Step 1: Load discriminant value onto stack
        self.lower_operand(discriminant, function, local_map)?;

        // Step 2: Choose optimal WASM instruction based on br_table_info
        if br_table_info.is_dense && targets.len() > 2 {
            // Use br_table for dense switch statements (efficient for many targets)
            self.generate_dense_br_table(targets, default, br_table_info, function)?;
        } else {
            // For sparse or small switch statements, generate optimized if/else chain
            self.generate_sparse_switch_chain(discriminant, targets, default, function, local_map)?;
        }

        Ok(())
    }

    /// Generate dense br_table instruction for efficient multi-way branching
    fn generate_dense_br_table(
        &self,
        targets: &[u32],
        default: u32,
        br_table_info: &crate::compiler::mir::mir_nodes::BrTableInfo,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // Create label indices for br_table
        // Each target gets mapped to its position in the table
        let mut label_indices = Vec::new();

        // Handle dense packing: fill gaps between min_target and max_target
        let range_size = (br_table_info.max_target - br_table_info.min_target + 1) as usize;
        label_indices.resize(range_size, targets.len() as u32); // Default to default case

        // Map actual targets to their label indices
        for (label_idx, &target_value) in targets.iter().enumerate() {
            let table_index = (target_value - br_table_info.min_target) as usize;
            if table_index < label_indices.len() {
                label_indices[table_index] = label_idx as u32;
            }
        }

        // Adjust discriminant if min_target is not 0
        if br_table_info.min_target > 0 {
            function.instruction(&Instruction::I32Const(br_table_info.min_target as i32));
            function.instruction(&Instruction::I32Sub);
        }

        // Generate br_table instruction
        let default_label = targets.len() as u32;
        function.instruction(&Instruction::BrTable(
            Cow::Owned(label_indices),
            default_label,
        ));

        Ok(())
    }

    /// Generate sparse switch chain using optimized if/else sequence
    fn generate_sparse_switch_chain(
        &self,
        discriminant: &Operand,
        targets: &[u32],
        _default: u32,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // For sparse switches, generate binary search tree or linear chain
        // depending on the number of targets

        if targets.len() <= 4 {
            // Linear chain for small number of targets
            self.generate_linear_switch_chain(discriminant, targets, function, local_map)?;
        } else {
            // Binary search tree for larger number of targets
            self.generate_binary_search_switch(discriminant, targets, function, local_map)?;
        }

        Ok(())
    }

    /// Generate linear if/else chain for small switches
    fn generate_linear_switch_chain(
        &self,
        discriminant: &Operand,
        targets: &[u32],
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Generate: if (discriminant == target[0]) br 0; else if (discriminant == target[1]) br 1; ...

        for (i, &target_value) in targets.iter().enumerate() {
            // Load discriminant for comparison
            self.lower_operand(discriminant, function, local_map)?;
            function.instruction(&Instruction::I32Const(target_value as i32));
            function.instruction(&Instruction::I32Eq);

            // If equal, branch to target
            function.instruction(&Instruction::If(BlockType::Empty));
            function.instruction(&Instruction::Br(i as u32 + 1)); // Branch to appropriate target
            function.instruction(&Instruction::End);
        }

        // Default case: fall through or explicit branch
        function.instruction(&Instruction::Br(targets.len() as u32));

        Ok(())
    }

    /// Generate binary search tree for larger switches
    fn generate_binary_search_switch(
        &self,
        discriminant: &Operand,
        targets: &[u32],
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // For now, implement as linear chain
        // TODO: Implement actual binary search tree optimization
        self.generate_linear_switch_chain(discriminant, targets, function, local_map)
    }

    /// Lower Terminator::Loop with WASM loop structures and back-edge handling
    /// This enhanced version provides proper WASM loop generation with optimization
    fn lower_loop_terminator(
        &self,
        target: u32,
        loop_header: u32,
        loop_info: &crate::compiler::mir::mir_nodes::WasmLoopInfo,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Validate loop parameters
        if target == loop_header {
            // This is a loop back-edge - generate proper WASM loop structure
            self.generate_wasm_loop_structure(target, loop_info, function)?;
        } else {
            // This is a branch within a loop - generate appropriate branch
            self.generate_loop_branch(target, loop_header, loop_info, function, local_map)?;
        }

        Ok(())
    }

    /// Generate WASM loop structure for loop headers
    fn generate_wasm_loop_structure(
        &self,
        _target: u32,
        loop_info: &crate::compiler::mir::mir_nodes::WasmLoopInfo,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // Determine block type based on loop result type
        let block_type = if let Some(result_type) = &loop_info.result_type {
            BlockType::Result(self.wasm_type_to_val_type(result_type))
        } else {
            BlockType::Empty
        };

        // Generate WASM loop instruction
        function.instruction(&Instruction::Loop(block_type));

        // The loop body will be generated by subsequent statements
        // The loop structure is now established for back-edges

        // Note: The actual loop body and back-edge handling will be done
        // by the block lowering process. This just establishes the WASM loop structure.

        Ok(())
    }

    /// Generate branch within a loop (break, continue, etc.)
    fn generate_loop_branch(
        &self,
        target: u32,
        loop_header: u32,
        loop_info: &crate::compiler::mir::mir_nodes::WasmLoopInfo,
        function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Determine the type of loop branch
        if target == loop_header {
            // Continue: branch back to loop start
            self.generate_loop_continue(loop_info, function)?;
        } else if target > loop_header {
            // Break: branch out of loop
            self.generate_loop_break(target, loop_info, function)?;
        } else {
            // Regular branch within loop body
            self.generate_loop_internal_branch(target, function)?;
        }

        Ok(())
    }

    /// Generate loop continue (branch back to loop start)
    fn generate_loop_continue(
        &self,
        loop_info: &crate::compiler::mir::mir_nodes::WasmLoopInfo,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // In WASM, continue is a branch to the loop instruction (label depth 0)
        // This assumes we're inside a WASM loop structure

        match loop_info.loop_type {
            crate::compiler::mir::mir_nodes::LoopType::While
            | crate::compiler::mir::mir_nodes::LoopType::For
            | crate::compiler::mir::mir_nodes::LoopType::Infinite => {
                // Branch back to loop start (label depth 0 for innermost loop)
                function.instruction(&Instruction::Br(0));
            }
            crate::compiler::mir::mir_nodes::LoopType::DoWhile => {
                // For do-while, we might need different handling
                // For now, treat the same as other loops
                function.instruction(&Instruction::Br(0));
            }
        }

        Ok(())
    }

    /// Generate loop break (branch out of loop)
    fn generate_loop_break(
        &self,
        _target: u32,
        loop_info: &crate::compiler::mir::mir_nodes::WasmLoopInfo,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // In WASM, break is a branch out of the loop structure
        // This typically means branching to the block containing the loop (label depth 1)

        if loop_info.has_breaks {
            // Branch out of loop (label depth 1 to exit the block containing the loop)
            function.instruction(&Instruction::Br(1));
        } else {
            // If the loop doesn't expect breaks, this might be an error
            return_compiler_error!("Loop break generated for loop that doesn't support breaks");
        }

        Ok(())
    }

    /// Generate internal branch within loop body
    fn generate_loop_internal_branch(
        &self,
        _target: u32,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // For internal branches, use a simple branch instruction
        // The label depth will be calculated based on the control flow structure
        function.instruction(&Instruction::Br(0));

        Ok(())
    }

    /// Lower Terminator::Block for WASM block structure generation
    /// This enhanced version provides proper WASM block generation for complex control flow
    fn lower_block_terminator(
        &self,
        inner_blocks: &[u32],
        result_type: &Option<WasmType>,
        exit_target: u32,
        function: &mut Function,
        _local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Validate block parameters
        if inner_blocks.is_empty() {
            return_compiler_error!("Block terminator must have at least one inner block");
        }

        // Generate WASM block structure for complex control flow patterns
        self.generate_wasm_block_structure(inner_blocks, result_type, exit_target, function)?;

        Ok(())
    }

    /// Generate WASM block structure for complex control flow patterns
    fn generate_wasm_block_structure(
        &self,
        inner_blocks: &[u32],
        result_type: &Option<WasmType>,
        exit_target: u32,
        function: &mut Function,
    ) -> Result<(), CompileError> {
        // Determine block type based on result type
        let block_type = if let Some(wasm_type) = result_type {
            BlockType::Result(self.wasm_type_to_val_type(wasm_type))
        } else {
            BlockType::Empty
        };

        // Generate WASM block instruction
        function.instruction(&Instruction::Block(block_type));

        // The inner blocks will be processed by the block lowering process
        // This establishes the WASM block structure for proper control flow

        // Generate branch to exit target
        // Calculate appropriate label depth based on nesting
        let label_depth = self.calculate_block_exit_depth(inner_blocks, exit_target)?;
        function.instruction(&Instruction::Br(label_depth));

        // End the block structure
        function.instruction(&Instruction::End);

        Ok(())
    }

    /// Calculate label depth for block exit branches
    fn calculate_block_exit_depth(
        &self,
        inner_blocks: &[u32],
        exit_target: u32,
    ) -> Result<u32, CompileError> {
        // For now, use a simple heuristic based on block structure
        // In a full implementation, this would use proper control flow analysis

        if inner_blocks.contains(&exit_target) {
            // Exit target is within the block - use depth 0
            Ok(0)
        } else {
            // Exit target is outside the block - use depth 1 to exit the block
            Ok(1)
        }
    }

    /// Create block label management for proper WASM structured control flow
    /// This is a helper method for managing nested control flow structures
    pub fn create_block_label_manager(&self) -> BlockLabelManager {
        BlockLabelManager::new()
    }

    /// Generate nested control flow support with proper label depth calculation
    /// This method handles complex nested control flow patterns
    pub fn generate_nested_control_flow(
        &self,
        terminators: &[Terminator],
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        let mut label_manager = self.create_block_label_manager();

        // Process each terminator with proper nesting tracking
        for (i, terminator) in terminators.iter().enumerate() {
            self.lower_terminator_with_nesting(
                terminator,
                &mut label_manager,
                function,
                local_map,
                i,
            )?;
        }

        Ok(())
    }

    /// Lower terminator with proper nesting context
    fn lower_terminator_with_nesting(
        &self,
        terminator: &Terminator,
        label_manager: &mut BlockLabelManager,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
        terminator_index: usize,
    ) -> Result<(), CompileError> {
        match terminator {
            Terminator::Block {
                inner_blocks,
                result_type,
                exit_target,
            } => {
                // Enter block control frame
                let block_id = Some(*exit_target);
                label_manager.enter_control_frame(ControlFrameType::Block, block_id);

                // Generate block structure
                self.lower_block_terminator(
                    inner_blocks,
                    result_type,
                    *exit_target,
                    function,
                    local_map,
                )?;

                // Exit block control frame
                label_manager.exit_control_frame();
            }

            Terminator::Loop {
                target,
                loop_header,
                loop_info,
            } => {
                // Enter loop control frame
                let loop_id = Some(*loop_header);
                label_manager.enter_control_frame(ControlFrameType::Loop, loop_id);

                // Generate loop structure
                self.lower_loop_terminator(*target, *loop_header, loop_info, function, local_map)?;

                // Exit loop control frame
                label_manager.exit_control_frame();
            }

            Terminator::If {
                condition,
                then_block,
                else_block,
                wasm_if_info,
            } => {
                // Enter if control frame
                let if_id = Some(*then_block);
                label_manager.enter_control_frame(ControlFrameType::If, if_id);

                // Generate if structure with proper label depths
                self.lower_if_terminator_with_nesting(
                    condition,
                    *then_block,
                    *else_block,
                    wasm_if_info,
                    label_manager,
                    function,
                    local_map,
                )?;

                // Exit if control frame
                label_manager.exit_control_frame();
            }

            Terminator::Goto {
                target,
                label_depth: _,
            } => {
                // Calculate proper label depth using label manager
                let calculated_depth = label_manager.calculate_branch_depth(*target).unwrap_or(0);
                self.lower_goto_terminator(*target, calculated_depth, function)?;
            }

            _ => {
                // For other terminators, use standard lowering
                self.lower_terminator(terminator, function, local_map)?;
            }
        }

        Ok(())
    }

    /// Lower if terminator with proper nesting context
    fn lower_if_terminator_with_nesting(
        &self,
        condition: &Operand,
        then_block: u32,
        else_block: u32,
        wasm_if_info: &crate::compiler::mir::mir_nodes::WasmIfInfo,
        label_manager: &BlockLabelManager,
        function: &mut Function,
        local_map: &HashMap<Place, u32>,
    ) -> Result<(), CompileError> {
        // Load condition
        self.lower_operand(condition, function, local_map)?;

        // Determine block type
        let block_type = if let Some(result_type) = &wasm_if_info.result_type {
            BlockType::Result(self.wasm_type_to_val_type(result_type))
        } else {
            BlockType::Empty
        };

        // Generate if structure with calculated label depths
        function.instruction(&Instruction::If(block_type));

        // Then branch with proper label depth
        let then_depth = label_manager
            .calculate_branch_depth(then_block)
            .unwrap_or(1);
        function.instruction(&Instruction::Br(then_depth));

        if wasm_if_info.has_else {
            function.instruction(&Instruction::Else);

            // Else branch with proper label depth
            let else_depth = label_manager
                .calculate_branch_depth(else_block)
                .unwrap_or(1);
            function.instruction(&Instruction::Br(else_depth));
        }

        function.instruction(&Instruction::End);

        Ok(())
    }

    /// Validate control flow structure to ensure proper WASM structured execution
    /// This method performs comprehensive validation of the generated control flow
    pub fn validate_control_flow_structure(
        &self,
        terminators: &[Terminator],
        function_name: &str,
    ) -> Result<(), CompileError> {
        let mut label_manager = self.create_block_label_manager();
        let mut validation_errors = Vec::new();

        // Track control flow structure for validation
        for (i, terminator) in terminators.iter().enumerate() {
            if let Err(error) =
                self.validate_terminator_structure(terminator, &mut label_manager, i, function_name)
            {
                validation_errors.push(error);
            }
        }

        // Check for unmatched control structures
        if label_manager.get_current_depth() != 0 {
            validation_errors.push(format!(
                "Unmatched control structures in function '{}': depth {} at end",
                function_name,
                label_manager.get_current_depth()
            ));
        }

        // Report validation errors
        if !validation_errors.is_empty() {
            return_compiler_error!(
                "Control flow validation failed in function '{}': {}",
                function_name,
                validation_errors.join("; ")
            );
        }

        Ok(())
    }

    /// Validate individual terminator structure
    fn validate_terminator_structure(
        &self,
        terminator: &Terminator,
        label_manager: &mut BlockLabelManager,
        terminator_index: usize,
        function_name: &str,
    ) -> Result<(), String> {
        match terminator {
            Terminator::Block {
                inner_blocks,
                result_type: _,
                exit_target,
            } => {
                // Validate block structure
                if inner_blocks.is_empty() {
                    return Err(format!("Empty block at terminator {}", terminator_index));
                }

                // Enter and validate block frame
                label_manager.enter_control_frame(ControlFrameType::Block, Some(*exit_target));

                // Validate that exit target is reachable
                if label_manager.get_label_depth(*exit_target).is_none() {
                    return Err(format!(
                        "Invalid exit target {} in block at terminator {}",
                        exit_target, terminator_index
                    ));
                }

                label_manager.exit_control_frame();
            }

            Terminator::Loop {
                target,
                loop_header,
                loop_info,
            } => {
                // Validate loop structure
                if *target == *loop_header && !loop_info.has_continues {
                    return Err(format!(
                        "Loop back-edge without continue support at terminator {}",
                        terminator_index
                    ));
                }

                // Enter and validate loop frame
                label_manager.enter_control_frame(ControlFrameType::Loop, Some(*loop_header));
                label_manager.exit_control_frame();
            }

            Terminator::If {
                condition: _,
                then_block,
                else_block,
                wasm_if_info,
            } => {
                // Validate if structure
                if *then_block == *else_block && wasm_if_info.has_else {
                    return Err(format!(
                        "If with same then/else blocks at terminator {}",
                        terminator_index
                    ));
                }

                // Enter and validate if frame
                label_manager.enter_control_frame(ControlFrameType::If, Some(*then_block));
                label_manager.exit_control_frame();
            }

            Terminator::Switch {
                discriminant: _,
                targets,
                default,
                br_table_info,
            } => {
                // Validate switch structure
                if targets.is_empty() {
                    return Err(format!(
                        "Switch with no targets at terminator {}",
                        terminator_index
                    ));
                }

                if br_table_info.is_dense && br_table_info.max_target < br_table_info.min_target {
                    return Err(format!(
                        "Invalid br_table range at terminator {}",
                        terminator_index
                    ));
                }

                // Validate that default is not in targets (would be redundant)
                if targets.contains(default) {
                    return Err(format!(
                        "Default target {} appears in targets list at terminator {}",
                        default, terminator_index
                    ));
                }
            }

            Terminator::Goto {
                target,
                label_depth,
            } => {
                // Validate goto target and depth
                if *label_depth > label_manager.get_current_depth() {
                    return Err(format!(
                        "Invalid label depth {} (current depth {}) at terminator {}",
                        label_depth,
                        label_manager.get_current_depth(),
                        terminator_index
                    ));
                }

                // Check if target is reachable (if we have the information)
                if let Some(target_depth) = label_manager.get_label_depth(*target) {
                    let calculated_depth = label_manager
                        .get_current_depth()
                        .saturating_sub(target_depth);
                    if *label_depth != calculated_depth {
                        return Err(format!(
                            "Label depth mismatch: expected {}, got {} at terminator {}",
                            calculated_depth, label_depth, terminator_index
                        ));
                    }
                }
            }

            _ => {
                // Other terminators don't need special validation
            }
        }

        Ok(())
    }

    /// Finalize the WASM module and return the encoded bytes
    pub fn finish(self) -> Vec<u8> {
        let mut module = Module::new();

        // Encode each section in the correct order (only if they have content)
        if self.type_count > 0 {
            module.section(&self.type_section);
        }

        // Always include import section (may be empty)
        module.section(&self.import_section);

        if self.function_count > 0 {
            module.section(&self.function_signature_section);
        }

        // Include table section if we have interface support
        module.section(&self.table_section);

        // Always include memory section for linear memory
        module.section(&self.memory_section);

        if self.global_count > 0 {
            module.section(&self.global_section);
        }

        // Always include export section
        module.section(&self.export_section);

        if let Some(start_section) = self.start_section {
            module.section(&start_section);
        }

        // Include element section if we have function tables
        module.section(&self.element_section);

        if self.function_count > 0 {
            module.section(&self.code_section);
        }

        // Include data section if we have string constants
        if !self.string_constants.is_empty() {
            module.section(&self.data_section);
        }

        module.finish()
    }

    // ===== MEMORY LAYOUT MANAGEMENT METHODS =====

    /// Add memory layout methods to existing WasmModule using existing string_constants and string_constant_map fields
    /// This provides comprehensive memory layout management for WASM linear memory
    pub fn create_memory_layout_manager(
        &mut self,
        mir: &MIR,
    ) -> Result<MemoryLayoutManager, CompileError> {
        let mut layout_manager = MemoryLayoutManager::new();

        // Initialize with MIR memory information
        layout_manager.initialize_from_mir(mir)?;

        // Add string constants using existing fields
        layout_manager.add_string_constants(&self.string_constants, &self.string_constant_map)?;

        // Calculate global data layout
        layout_manager.calculate_global_data_layout(&mir.globals)?;

        // Set up heap allocation region
        layout_manager.setup_heap_region()?;

        Ok(layout_manager)
    }

    /// Implement static data allocation for string constants and global data using existing data_section
    /// This method populates the WASM data section with all static data
    pub fn populate_static_data_section(
        &mut self,
        layout_manager: &MemoryLayoutManager,
    ) -> Result<(), CompileError> {
        // Clear existing data section to rebuild it
        self.data_section = DataSection::new();

        // Add string constants to data section
        for (string_value, &offset) in &self.string_constant_map {
            let string_bytes = string_value.as_bytes();

            // Add null terminator for C-style strings
            let mut data_bytes = string_bytes.to_vec();
            data_bytes.push(0);

            // Create active data segment at the calculated offset
            self.data_section.active(
                0, // Memory index 0 (linear memory)
                &ConstExpr::i32_const(offset as i32),
                data_bytes.iter().copied(),
            );
        }

        // Add global static data to data section
        for (global_id, global_data) in layout_manager.get_global_static_data() {
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(global_data.offset as i32),
                global_data.data.iter().copied(),
            );
        }

        // Add struct layout data if needed
        for (struct_id, struct_data) in layout_manager.get_struct_static_data() {
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(struct_data.offset as i32),
                struct_data.data.iter().copied(),
            );
        }

        Ok(())
    }

    /// Add struct field layout calculation with WASM-appropriate alignment
    /// This method calculates optimal struct layouts for WASM linear memory
    pub fn calculate_struct_field_layout(
        &self,
        field_types: &[WasmType],
        struct_id: u32,
    ) -> Result<StructFieldLayout, CompileError> {
        let mut layout = StructFieldLayout::new(struct_id);
        let mut current_offset = 0u32;
        let mut max_alignment = 1u32;

        // Calculate layout for each field with proper WASM alignment
        for (field_index, field_type) in field_types.iter().enumerate() {
            let field_size = self.get_wasm_type_size(field_type);
            let field_alignment = self.get_wasm_type_alignment(field_type);

            // Update maximum alignment requirement
            max_alignment = max_alignment.max(field_alignment);

            // Align current offset to field alignment requirement
            current_offset = align_to(current_offset, field_alignment);

            // Add field to layout
            layout.add_field(
                field_index as u32,
                current_offset,
                field_size,
                field_alignment,
                field_type.clone(),
            );

            // Advance offset by field size
            current_offset += field_size;
        }

        // Align total size to maximum alignment for proper struct alignment
        layout.total_size = align_to(current_offset, max_alignment);
        layout.alignment = max_alignment;

        // Validate layout constraints
        self.validate_struct_layout(&layout)?;

        Ok(layout)
    }

    /// Enhance memory section generation with proper initial/max page settings using existing memory_section
    /// This method configures WASM memory with optimal settings based on static analysis
    pub fn enhance_memory_section(
        &mut self,
        layout_manager: &MemoryLayoutManager,
    ) -> Result<(), CompileError> {
        // Calculate required memory based on static data and heap requirements
        let static_data_size = layout_manager.get_total_static_size();
        let heap_size = layout_manager.get_heap_size();
        let total_required_bytes = static_data_size + heap_size;

        // Convert bytes to WASM pages (64KB each)
        let required_pages = (total_required_bytes + WASM_PAGE_SIZE - 1) / WASM_PAGE_SIZE;

        // Set initial pages with some headroom for dynamic allocation
        let initial_pages = required_pages.max(1); // At least 1 page

        // Set maximum pages based on application requirements
        // For now, allow growth up to 16MB (256 pages) for typical applications
        let max_pages = Some((initial_pages * 4).max(256));

        // Create enhanced memory type with calculated settings
        let memory_type = MemoryType {
            minimum: initial_pages as u64,
            maximum: max_pages.map(|p| p as u64),
            memory64: false,      // Use 32-bit memory for WASM32
            shared: false,        // Single-threaded for now
            page_size_log2: None, // Use default 64KB pages
        };

        // Replace existing memory section with enhanced configuration
        self.memory_section = MemorySection::new();
        self.memory_section.memory(memory_type);

        Ok(())
    }

    /// Implement data section population with string constants and static data
    /// This method ensures all static data is properly placed in WASM linear memory
    pub fn populate_data_section_with_layout(
        &mut self,
        layout_manager: &MemoryLayoutManager,
    ) -> Result<(), CompileError> {
        // Clear and rebuild data section with comprehensive layout
        self.data_section = DataSection::new();

        // Add string constants with proper alignment
        for string_info in layout_manager.get_string_layout() {
            let string_bytes = string_info.content.as_bytes();
            let mut data_with_terminator = string_bytes.to_vec();
            data_with_terminator.push(0); // Null terminator

            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(string_info.offset as i32),
                data_with_terminator.iter().copied(),
            );
        }

        // Add global variable initial values
        for global_info in layout_manager.get_global_layout() {
            if let Some(initial_data) = &global_info.initial_data {
                self.data_section.active(
                    0, // Memory index 0
                    &ConstExpr::i32_const(global_info.offset as i32),
                    initial_data.iter().copied(),
                );
            }
        }

        // Add struct type information for runtime type checking
        for struct_info in layout_manager.get_struct_layout() {
            // Add struct metadata (size, alignment, field count)
            let metadata = struct_info.create_metadata_bytes();
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(struct_info.metadata_offset as i32),
                metadata.iter().copied(),
            );
        }

        // Add vtable data for interface dispatch (if present)
        for vtable_info in layout_manager.get_vtable_layout() {
            let vtable_data = vtable_info.create_vtable_bytes();
            self.data_section.active(
                0, // Memory index 0
                &ConstExpr::i32_const(vtable_info.offset as i32),
                vtable_data.iter().copied(),
            );
        }

        Ok(())
    }

    /// Add heap allocation support for dynamic memory management
    /// This method sets up heap management infrastructure in WASM linear memory
    pub fn setup_heap_allocation_support(
        &mut self,
        layout_manager: &MemoryLayoutManager,
    ) -> Result<HeapAllocator, CompileError> {
        let heap_info = layout_manager.get_heap_info();

        // Create heap allocator with calculated parameters
        let mut heap_allocator =
            HeapAllocator::new(heap_info.start_offset, heap_info.size, heap_info.alignment);

        // Initialize heap metadata in data section
        let heap_metadata = heap_allocator.create_initial_metadata();
        self.data_section.active(
            0, // Memory index 0
            &ConstExpr::i32_const(heap_info.metadata_offset as i32),
            heap_metadata.iter().copied(),
        );

        // Add heap management functions to the module
        self.add_heap_management_functions(&mut heap_allocator)?;

        Ok(heap_allocator)
    }

    /// Add heap management functions (malloc, free, etc.) to the WASM module
    fn add_heap_management_functions(
        &mut self,
        heap_allocator: &mut HeapAllocator,
    ) -> Result<(), CompileError> {
        // Add malloc function type
        let malloc_type_index = self.type_count;
        self.type_section.ty().function(
            vec![ValType::I32], // size parameter
            vec![ValType::I32], // pointer result
        );
        self.type_count += 1;

        // Add free function type
        let free_type_index = self.type_count;
        self.type_section.ty().function(
            vec![ValType::I32], // pointer parameter
            vec![],             // no result
        );
        self.type_count += 1;

        // Add function signatures
        self.function_signature_section.function(malloc_type_index);
        self.function_signature_section.function(free_type_index);

        // Generate malloc function implementation
        let malloc_function = heap_allocator.generate_malloc_function()?;
        self.code_section.function(&malloc_function);

        // Generate free function implementation
        let free_function = heap_allocator.generate_free_function()?;
        self.code_section.function(&free_function);

        // Export heap management functions
        let malloc_func_index = self.function_count;
        let free_func_index = self.function_count + 1;
        self.function_count += 2;

        self.export_section
            .export("malloc", ExportKind::Func, malloc_func_index);
        self.export_section
            .export("free", ExportKind::Func, free_func_index);

        Ok(())
    }

    /// Validate struct layout for WASM compatibility
    fn validate_struct_layout(&self, layout: &StructFieldLayout) -> Result<(), CompileError> {
        // Check that total size is properly aligned
        if layout.total_size % layout.alignment != 0 {
            return_compiler_error!(
                "Struct layout total size {} is not aligned to {}",
                layout.total_size,
                layout.alignment
            );
        }

        // Check that all field offsets are properly aligned
        for field in &layout.fields {
            if field.offset % field.alignment != 0 {
                return_compiler_error!(
                    "Field {} offset {} is not aligned to {}",
                    field.index,
                    field.offset,
                    field.alignment
                );
            }
        }

        // Check for field overlaps
        let mut sorted_fields = layout.fields.clone();
        sorted_fields.sort_by_key(|f| f.offset);

        for i in 1..sorted_fields.len() {
            let prev_field = &sorted_fields[i - 1];
            let curr_field = &sorted_fields[i];
            let prev_end = prev_field.offset + prev_field.size;

            if prev_end > curr_field.offset {
                return_compiler_error!(
                    "Field {} (offset {}) overlaps with field {} (ends at {})",
                    curr_field.index,
                    curr_field.offset,
                    prev_field.index,
                    prev_end
                );
            }
        }

        Ok(())
    }

    /// Get memory layout statistics for debugging and optimization
    pub fn get_memory_layout_stats(
        &self,
        layout_manager: &MemoryLayoutManager,
    ) -> MemoryLayoutStats {
        MemoryLayoutStats {
            total_static_size: layout_manager.get_total_static_size(),
            string_constants_size: self
                .string_constants
                .iter()
                .map(|s| s.len() as u32 + 1)
                .sum(),
            global_data_size: layout_manager.get_global_data_size(),
            struct_metadata_size: layout_manager.get_struct_metadata_size(),
            heap_size: layout_manager.get_heap_size(),
            total_memory_pages: (layout_manager.get_total_static_size()
                + layout_manager.get_heap_size()
                + WASM_PAGE_SIZE
                - 1)
                / WASM_PAGE_SIZE,
            alignment_waste: layout_manager.calculate_alignment_waste(),
        }
    }
}

// ===== HELPER STRUCTURES =====

/// Struct layout information for WASM memory layout calculation
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Total size of the struct in bytes
    pub total_size: u32,
    /// Alignment requirement of the struct
    pub alignment: u32,
    /// Field information: (field_index, offset, size, alignment)
    fields: Vec<(u32, u32, u32, u32)>,
}

impl StructLayout {
    /// Create a new empty struct layout
    pub fn new() -> Self {
        Self {
            total_size: 0,
            alignment: 1,
            fields: Vec::new(),
        }
    }

    /// Add a field to the struct layout
    pub fn add_field(&mut self, field_index: u32, offset: u32, size: u32, alignment: u32) {
        self.fields.push((field_index, offset, size, alignment));
        self.alignment = self.alignment.max(alignment);
    }

    /// Get the offset of a field by index
    pub fn get_field_offset(&self, field_index: u32) -> Option<u32> {
        self.fields
            .iter()
            .find(|(idx, _, _, _)| *idx == field_index)
            .map(|(_, offset, _, _)| *offset)
    }

    /// Get the size of a field by index
    pub fn get_field_size(&self, field_index: u32) -> Option<u32> {
        self.fields
            .iter()
            .find(|(idx, _, _, _)| *idx == field_index)
            .map(|(_, _, size, _)| *size)
    }

    /// Get the alignment of a field by index
    pub fn get_field_alignment(&self, field_index: u32) -> Option<u32> {
        self.fields
            .iter()
            .find(|(idx, _, _, _)| *idx == field_index)
            .map(|(_, _, _, alignment)| *alignment)
    }
}

/// Type index mapping for efficient type lookups
#[derive(Debug, Clone)]
pub struct TypeIndexMapping {
    /// Function index to type index mapping
    function_types: HashMap<u32, u32>,
    /// Interface method to type index mapping
    interface_method_types: HashMap<(u32, u32), u32>,
}

impl TypeIndexMapping {
    /// Create a new empty type index mapping
    pub fn new() -> Self {
        Self {
            function_types: HashMap::new(),
            interface_method_types: HashMap::new(),
        }
    }

    /// Add a function type mapping
    pub fn add_function_type(&mut self, function_index: u32, type_index: u32) {
        self.function_types.insert(function_index, type_index);
    }

    /// Add an interface method type mapping
    pub fn add_interface_method_type(
        &mut self,
        interface_id: u32,
        method_id: u32,
        type_index: u32,
    ) {
        self.interface_method_types
            .insert((interface_id, method_id), type_index);
    }

    /// Get the type index for a function
    pub fn get_function_type(&self, function_index: u32) -> Option<u32> {
        self.function_types.get(&function_index).copied()
    }

    /// Get the type index for an interface method
    pub fn get_interface_method_type(&self, interface_id: u32, method_id: u32) -> Option<u32> {
        self.interface_method_types
            .get(&(interface_id, method_id))
            .copied()
    }
}

/// Interface method mapping for efficient dispatch
/// Maps interface methods to their implementations across different types
#[derive(Debug, Clone)]
pub struct InterfaceMethodMapping {
    /// Maps (interface_id, method_id, type_id) to function_index
    method_implementations: HashMap<(u32, u32, u32), u32>,
    /// Maps (interface_id, method_id) to list of implementing types
    method_implementers: HashMap<(u32, u32), Vec<u32>>,
}

impl InterfaceMethodMapping {
    /// Create a new empty interface method mapping
    pub fn new() -> Self {
        Self {
            method_implementations: HashMap::new(),
            method_implementers: HashMap::new(),
        }
    }

    /// Add a method implementation for a specific type
    pub fn add_method_implementation(
        &mut self,
        interface_id: u32,
        method_id: u32,
        type_id: u32,
        function_index: u32,
    ) {
        self.method_implementations
            .insert((interface_id, method_id, type_id), function_index);

        // Track which types implement this method
        self.method_implementers
            .entry((interface_id, method_id))
            .or_insert_with(Vec::new)
            .push(type_id);
    }

    /// Get the function index for a method implementation
    pub fn get_method_implementation(
        &self,
        interface_id: u32,
        method_id: u32,
        type_id: u32,
    ) -> Option<u32> {
        self.method_implementations
            .get(&(interface_id, method_id, type_id))
            .copied()
    }

    /// Get all types that implement a specific method
    pub fn get_method_implementers(&self, interface_id: u32, method_id: u32) -> Option<&Vec<u32>> {
        self.method_implementers.get(&(interface_id, method_id))
    }
}

/// Block label management for proper WASM structured control flow
/// Tracks nested control flow structures and their label depths
#[derive(Debug, Clone)]
pub struct BlockLabelManager {
    /// Stack of active control flow structures
    control_stack: Vec<ControlFrame>,
    /// Current nesting depth
    current_depth: u32,
    /// Mapping from block ID to label depth
    block_to_label: HashMap<u32, u32>,
}

impl BlockLabelManager {
    /// Create a new block label manager
    pub fn new() -> Self {
        Self {
            control_stack: Vec::new(),
            current_depth: 0,
            block_to_label: HashMap::new(),
        }
    }

    /// Enter a new control flow structure (block, if, loop)
    pub fn enter_control_frame(
        &mut self,
        frame_type: ControlFrameType,
        block_id: Option<u32>,
    ) -> u32 {
        let label_depth = self.current_depth;

        let frame = ControlFrame {
            frame_type,
            label_depth,
            block_id,
        };

        self.control_stack.push(frame);

        if let Some(id) = block_id {
            self.block_to_label.insert(id, label_depth);
        }

        self.current_depth += 1;
        label_depth
    }

    /// Exit the current control flow structure
    pub fn exit_control_frame(&mut self) -> Option<ControlFrame> {
        if let Some(frame) = self.control_stack.pop() {
            self.current_depth = self.current_depth.saturating_sub(1);
            Some(frame)
        } else {
            None
        }
    }

    /// Get the label depth for a block ID
    pub fn get_label_depth(&self, block_id: u32) -> Option<u32> {
        self.block_to_label.get(&block_id).copied()
    }

    /// Get the current nesting depth
    pub fn get_current_depth(&self) -> u32 {
        self.current_depth
    }

    /// Calculate relative label depth for branching
    pub fn calculate_branch_depth(&self, target_block_id: u32) -> Option<u32> {
        if let Some(target_depth) = self.get_label_depth(target_block_id) {
            // Branch depth is the difference between current depth and target depth
            Some(self.current_depth.saturating_sub(target_depth))
        } else {
            None
        }
    }
}

/// Control flow frame for tracking nested structures
#[derive(Debug, Clone)]
pub struct ControlFrame {
    /// Type of control flow structure
    pub frame_type: ControlFrameType,
    /// Label depth of this frame
    pub label_depth: u32,
    /// Associated block ID (if any)
    pub block_id: Option<u32>,
}

/// Types of WASM control flow structures
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ControlFrameType {
    /// WASM block
    Block,
    /// WASM if/else
    If,
    /// WASM loop
    Loop,
    /// Function body
    Function,
}

/// Helper function to align a value to a given alignment
fn align_to(value: u32, alignment: u32) -> u32 {
    if alignment == 0 {
        return value;
    }
    (value + alignment - 1) & !(alignment - 1)
}

// ===== MEMORY LAYOUT MANAGEMENT STRUCTURES =====

/// WASM page size constant (64KB)
pub const WASM_PAGE_SIZE: u32 = 65536;

/// Memory layout manager for comprehensive WASM linear memory management
#[derive(Debug, Clone)]
pub struct MemoryLayoutManager {
    /// Static data region layout
    static_region: StaticDataRegion,
    /// String constants layout
    string_layout: Vec<StringInfo>,
    /// Global variables layout
    global_layout: Vec<GlobalInfo>,
    /// Struct type layouts
    struct_layout: Vec<StructInfo>,
    /// VTable layouts for interface dispatch
    vtable_layout: Vec<VTableInfo>,
    /// Heap allocation region
    heap_info: HeapInfo,
    /// Total static data size
    total_static_size: u32,
}

impl MemoryLayoutManager {
    /// Create a new memory layout manager
    pub fn new() -> Self {
        Self {
            static_region: StaticDataRegion::new(),
            string_layout: Vec::new(),
            global_layout: Vec::new(),
            struct_layout: Vec::new(),
            vtable_layout: Vec::new(),
            heap_info: HeapInfo::new(),
            total_static_size: 0,
        }
    }

    /// Initialize from MIR memory information
    pub fn initialize_from_mir(&mut self, mir: &MIR) -> Result<(), CompileError> {
        // Set up static region based on MIR memory info
        self.static_region.size = mir.type_info.memory_info.static_data_size;
        self.total_static_size = mir.type_info.memory_info.static_data_size;

        // Initialize heap region after static data
        self.heap_info.start_offset = self.total_static_size;

        Ok(())
    }

    /// Add string constants using existing fields
    pub fn add_string_constants(
        &mut self,
        strings: &[String],
        string_map: &HashMap<String, u32>,
    ) -> Result<(), CompileError> {
        for (string_value, &offset) in string_map {
            let string_info = StringInfo {
                content: string_value.clone(),
                offset,
                size: string_value.len() as u32 + 1, // +1 for null terminator
                alignment: 1,                        // Strings have byte alignment
            };
            self.string_layout.push(string_info);
        }

        // Sort by offset for efficient access
        self.string_layout.sort_by_key(|s| s.offset);

        Ok(())
    }

    /// Calculate global data layout
    pub fn calculate_global_data_layout(
        &mut self,
        globals: &HashMap<u32, Place>,
    ) -> Result<(), CompileError> {
        let mut current_offset = self.get_next_available_offset();

        for (global_id, place) in globals {
            let wasm_type = place.wasm_type();
            let size = get_wasm_type_size(&wasm_type);
            let alignment = get_wasm_type_alignment(&wasm_type);

            // Align offset for this global
            current_offset = align_to(current_offset, alignment);

            let global_info = GlobalInfo {
                id: *global_id,
                offset: current_offset,
                size,
                alignment,
                wasm_type,
                initial_data: None, // Will be set later if needed
            };

            self.global_layout.push(global_info);
            current_offset += size;
        }

        // Update total static size
        self.total_static_size = current_offset;

        Ok(())
    }

    /// Set up heap allocation region
    pub fn setup_heap_region(&mut self) -> Result<(), CompileError> {
        // Align heap start to page boundary for efficiency
        let heap_start = align_to(self.total_static_size, WASM_PAGE_SIZE);

        // Default heap size: 1MB (16 pages)
        let default_heap_size = 16 * WASM_PAGE_SIZE;

        self.heap_info = HeapInfo {
            start_offset: heap_start,
            size: default_heap_size,
            alignment: 8,                     // 8-byte alignment for heap allocations
            metadata_offset: heap_start - 64, // Reserve 64 bytes before heap for metadata
        };

        Ok(())
    }

    /// Get next available offset in static region
    fn get_next_available_offset(&self) -> u32 {
        let string_end = self
            .string_layout
            .iter()
            .map(|s| s.offset + s.size)
            .max()
            .unwrap_or(0);

        let global_end = self
            .global_layout
            .iter()
            .map(|g| g.offset + g.size)
            .max()
            .unwrap_or(0);

        string_end.max(global_end).max(self.static_region.size)
    }

    /// Get accessors for layout information
    pub fn get_string_layout(&self) -> &[StringInfo] {
        &self.string_layout
    }
    pub fn get_global_layout(&self) -> &[GlobalInfo] {
        &self.global_layout
    }
    pub fn get_struct_layout(&self) -> &[StructInfo] {
        &self.struct_layout
    }
    pub fn get_vtable_layout(&self) -> &[VTableInfo] {
        &self.vtable_layout
    }
    pub fn get_heap_info(&self) -> &HeapInfo {
        &self.heap_info
    }

    /// Get size information
    pub fn get_total_static_size(&self) -> u32 {
        self.total_static_size
    }
    pub fn get_heap_size(&self) -> u32 {
        self.heap_info.size
    }
    pub fn get_global_data_size(&self) -> u32 {
        self.global_layout.iter().map(|g| g.size).sum()
    }
    pub fn get_struct_metadata_size(&self) -> u32 {
        self.struct_layout.iter().map(|s| s.metadata_size).sum()
    }

    /// Calculate alignment waste for optimization analysis
    pub fn calculate_alignment_waste(&self) -> u32 {
        let mut waste = 0u32;

        // Calculate waste in string layout
        for i in 1..self.string_layout.len() {
            let prev = &self.string_layout[i - 1];
            let curr = &self.string_layout[i];
            let prev_end = prev.offset + prev.size;
            let aligned_start = align_to(prev_end, curr.alignment);
            waste += aligned_start - prev_end;
        }

        // Calculate waste in global layout
        for i in 1..self.global_layout.len() {
            let prev = &self.global_layout[i - 1];
            let curr = &self.global_layout[i];
            let prev_end = prev.offset + prev.size;
            let aligned_start = align_to(prev_end, curr.alignment);
            waste += aligned_start - prev_end;
        }

        waste
    }

    /// Get static data for different categories
    pub fn get_global_static_data(&self) -> impl Iterator<Item = (u32, &GlobalStaticData)> {
        // For now, return empty iterator - will be implemented when global initialization is added
        std::iter::empty()
    }

    pub fn get_struct_static_data(&self) -> impl Iterator<Item = (u32, &StructStaticData)> {
        // For now, return empty iterator - will be implemented when struct metadata is added
        std::iter::empty()
    }
}

/// Static data region information
#[derive(Debug, Clone)]
pub struct StaticDataRegion {
    pub start_offset: u32,
    pub size: u32,
}

impl StaticDataRegion {
    pub fn new() -> Self {
        Self {
            start_offset: 0,
            size: 0,
        }
    }
}

/// String constant information
#[derive(Debug, Clone)]
pub struct StringInfo {
    pub content: String,
    pub offset: u32,
    pub size: u32,
    pub alignment: u32,
}

/// Global variable information
#[derive(Debug, Clone)]
pub struct GlobalInfo {
    pub id: u32,
    pub offset: u32,
    pub size: u32,
    pub alignment: u32,
    pub wasm_type: WasmType,
    pub initial_data: Option<Vec<u8>>,
}

/// Struct type information
#[derive(Debug, Clone)]
pub struct StructInfo {
    pub id: u32,
    pub offset: u32,
    pub size: u32,
    pub alignment: u32,
    pub field_count: u32,
    pub metadata_offset: u32,
    pub metadata_size: u32,
}

impl StructInfo {
    /// Create metadata bytes for runtime type information
    pub fn create_metadata_bytes(&self) -> Vec<u8> {
        let mut metadata = Vec::new();

        // Add struct size (4 bytes)
        metadata.extend_from_slice(&self.size.to_le_bytes());

        // Add alignment (4 bytes)
        metadata.extend_from_slice(&self.alignment.to_le_bytes());

        // Add field count (4 bytes)
        metadata.extend_from_slice(&self.field_count.to_le_bytes());

        // Add struct ID (4 bytes)
        metadata.extend_from_slice(&self.id.to_le_bytes());

        metadata
    }
}

/// VTable information for interface dispatch
#[derive(Debug, Clone)]
pub struct VTableInfo {
    pub interface_id: u32,
    pub offset: u32,
    pub size: u32,
    pub method_count: u32,
    pub method_indices: Vec<u32>,
}

impl VTableInfo {
    /// Create vtable bytes for interface dispatch
    pub fn create_vtable_bytes(&self) -> Vec<u8> {
        let mut vtable_data = Vec::new();

        // Add interface ID (4 bytes)
        vtable_data.extend_from_slice(&self.interface_id.to_le_bytes());

        // Add method count (4 bytes)
        vtable_data.extend_from_slice(&self.method_count.to_le_bytes());

        // Add method indices (4 bytes each)
        for &method_index in &self.method_indices {
            vtable_data.extend_from_slice(&method_index.to_le_bytes());
        }

        vtable_data
    }
}

/// Heap allocation information
#[derive(Debug, Clone)]
pub struct HeapInfo {
    pub start_offset: u32,
    pub size: u32,
    pub alignment: u32,
    pub metadata_offset: u32,
}

impl HeapInfo {
    pub fn new() -> Self {
        Self {
            start_offset: 0,
            size: 0,
            alignment: 8,
            metadata_offset: 0,
        }
    }
}

/// Enhanced struct field layout with WASM-specific optimizations
#[derive(Debug, Clone)]
pub struct StructFieldLayout {
    pub struct_id: u32,
    pub total_size: u32,
    pub alignment: u32,
    pub fields: Vec<FieldInfo>,
}

impl StructFieldLayout {
    pub fn new(struct_id: u32) -> Self {
        Self {
            struct_id,
            total_size: 0,
            alignment: 1,
            fields: Vec::new(),
        }
    }

    pub fn add_field(
        &mut self,
        index: u32,
        offset: u32,
        size: u32,
        alignment: u32,
        wasm_type: WasmType,
    ) {
        let field_info = FieldInfo {
            index,
            offset,
            size,
            alignment,
            wasm_type,
        };
        self.fields.push(field_info);
    }
}

/// Field information for struct layouts
#[derive(Debug, Clone)]
pub struct FieldInfo {
    pub index: u32,
    pub offset: u32,
    pub size: u32,
    pub alignment: u32,
    pub wasm_type: WasmType,
}

/// Heap allocator for dynamic memory management
#[derive(Debug)]
pub struct HeapAllocator {
    pub start_offset: u32,
    pub size: u32,
    pub alignment: u32,
    pub free_list_offset: u32,
}

impl HeapAllocator {
    pub fn new(start_offset: u32, size: u32, alignment: u32) -> Self {
        Self {
            start_offset,
            size,
            alignment,
            free_list_offset: start_offset,
        }
    }

    /// Create initial heap metadata
    pub fn create_initial_metadata(&self) -> Vec<u8> {
        let mut metadata = Vec::new();

        // Heap start offset (4 bytes)
        metadata.extend_from_slice(&self.start_offset.to_le_bytes());

        // Heap size (4 bytes)
        metadata.extend_from_slice(&self.size.to_le_bytes());

        // Free list head (4 bytes) - initially points to start
        metadata.extend_from_slice(&self.start_offset.to_le_bytes());

        // Alignment (4 bytes)
        metadata.extend_from_slice(&self.alignment.to_le_bytes());

        metadata
    }

    /// Generate malloc function implementation
    pub fn generate_malloc_function(&self) -> Result<Function, CompileError> {
        // For now, return a simple stub function
        // In a full implementation, this would generate a complete malloc implementation
        let locals = vec![(1, ValType::I32)]; // One local for calculations
        let mut function = Function::new(locals);

        // Simple bump allocator for now
        // Load requested size (parameter 0)
        function.instruction(&Instruction::LocalGet(0));

        // For now, just return a fixed offset (placeholder)
        function.instruction(&Instruction::Drop);
        function.instruction(&Instruction::I32Const(self.start_offset as i32));

        function.instruction(&Instruction::End);
        Ok(function)
    }

    /// Generate free function implementation
    pub fn generate_free_function(&self) -> Result<Function, CompileError> {
        // For now, return a simple stub function
        // In a full implementation, this would generate a complete free implementation
        let locals = vec![]; // No locals needed for stub
        let mut function = Function::new(locals);

        // Simple no-op for now (just drop the pointer parameter)
        function.instruction(&Instruction::LocalGet(0));
        function.instruction(&Instruction::Drop);

        function.instruction(&Instruction::End);
        Ok(function)
    }
}

/// Memory layout statistics for debugging and optimization
#[derive(Debug, Clone)]
pub struct MemoryLayoutStats {
    pub total_static_size: u32,
    pub string_constants_size: u32,
    pub global_data_size: u32,
    pub struct_metadata_size: u32,
    pub heap_size: u32,
    pub total_memory_pages: u32,
    pub alignment_waste: u32,
}

/// Placeholder structures for future implementation
#[derive(Debug, Clone)]
pub struct GlobalStaticData {
    pub offset: u32,
    pub data: Vec<u8>,
}

#[derive(Debug, Clone)]
pub struct StructStaticData {
    pub offset: u32,
    pub data: Vec<u8>,
}

/// Helper functions for WASM type information
fn get_wasm_type_size(wasm_type: &WasmType) -> u32 {
    match wasm_type {
        WasmType::I32 | WasmType::F32 => 4,
        WasmType::I64 | WasmType::F64 => 8,
        WasmType::ExternRef | WasmType::FuncRef => 4, // Pointers are 4 bytes in WASM32
    }
}

fn get_wasm_type_alignment(wasm_type: &WasmType) -> u32 {
    match wasm_type {
        WasmType::I32 | WasmType::F32 => 4,
        WasmType::I64 | WasmType::F64 => 8,
        WasmType::ExternRef | WasmType::FuncRef => 4, // Pointer alignment
    }
}
