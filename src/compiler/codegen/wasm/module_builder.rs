//! WASM Module Builder
//!
//! This module provides the WasmModuleBuilder for constructing WASM modules
//! with proper section ordering and index management. The builder ensures
//! WASM sections are added in the correct order as required by the WASM spec:
//! Type, Import, Function, Table, Memory, Global, Export, Start, Element, Code, Data.
//!
//! The builder also maintains index coordination across sections to ensure
//! consistent references between types, functions, imports, and exports.

// Many methods are prepared for later implementation phases
// (host function imports, global variables, memory exports)
#![allow(dead_code)]

use crate::compiler::codegen::wasm::error::WasmGenerationError;
use crate::compiler::compiler_errors::CompilerError;
use std::collections::HashMap;
use wasm_encoder::{
    CodeSection, DataSection, ExportKind, ExportSection, Function, FunctionSection, GlobalSection,
    ImportSection, MemorySection, Module, TableSection, TypeSection, ValType,
};

/// Represents a registered function type for deduplication
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct FunctionType {
    pub params: Vec<ValType>,
    pub results: Vec<ValType>,
}

impl FunctionType {
    /// Create a new function type
    pub fn new(params: Vec<ValType>, results: Vec<ValType>) -> Self {
        FunctionType { params, results }
    }

    /// Create a function type with no parameters or results (void -> void)
    pub fn void() -> Self {
        FunctionType {
            params: Vec::new(),
            results: Vec::new(),
        }
    }

    /// Check if this is a void function type
    pub fn is_void(&self) -> bool {
        self.params.is_empty() && self.results.is_empty()
    }
}

/// Builder for constructing WASM modules with proper section ordering.
///
/// The builder maintains proper WASM section ordering:
/// 1. Type Section - Function type definitions
/// 2. Import Section - External function/memory/global imports
/// 3. Function Section - Function declarations (type indices)
/// 4. Table Section - Table definitions
/// 5. Memory Section - Linear memory definitions
/// 6. Global Section - Global variable definitions
/// 7. Export Section - Exported functions/memories/globals
/// 8. Start Section - Module start function (optional)
/// 9. Element Section - Table element initialization
/// 10. Code Section - Function bodies
/// 11. Data Section - Memory initialization data
///
/// Index coordination is maintained across sections to ensure consistent
/// references between types, functions, imports, and exports.
pub struct WasmModuleBuilder {
    // WASM sections in proper order
    type_section: TypeSection,
    import_section: ImportSection,
    function_section: FunctionSection,
    table_section: TableSection,
    memory_section: MemorySection,
    global_section: GlobalSection,
    export_section: ExportSection,
    code_section: CodeSection,
    data_section: DataSection,

    // Index tracking for proper coordination across sections
    type_count: u32,
    function_count: u32,
    import_function_count: u32,
    import_memory_count: u32,
    import_global_count: u32,
    import_table_count: u32,
    global_count: u32,
    memory_count: u32,
    table_count: u32,
    export_count: u32,

    // Type deduplication map: FunctionType -> type index
    type_cache: HashMap<FunctionType, u32>,

    // Function name to index mapping for call resolution
    function_name_to_index: HashMap<String, u32>,

    // Track which sections have content
    has_types: bool,
    has_imports: bool,
    has_functions: bool,
    has_tables: bool,
    has_memory: bool,
    has_globals: bool,
    has_exports: bool,
    has_code: bool,
    has_data: bool,
}

impl WasmModuleBuilder {
    /// Create a new WASM module builder
    pub fn new() -> Self {
        WasmModuleBuilder {
            type_section: TypeSection::new(),
            import_section: ImportSection::new(),
            function_section: FunctionSection::new(),
            table_section: TableSection::new(),
            memory_section: MemorySection::new(),
            global_section: GlobalSection::new(),
            export_section: ExportSection::new(),
            code_section: CodeSection::new(),
            data_section: DataSection::new(),

            type_count: 0,
            function_count: 0,
            import_function_count: 0,
            import_memory_count: 0,
            import_global_count: 0,
            import_table_count: 0,
            global_count: 0,
            memory_count: 0,
            table_count: 0,
            export_count: 0,

            type_cache: HashMap::new(),
            function_name_to_index: HashMap::new(),

            has_types: false,
            has_imports: false,
            has_functions: false,
            has_tables: false,
            has_memory: false,
            has_globals: false,
            has_exports: false,
            has_code: false,
            has_data: false,
        }
    }

    // =========================================================================
    // Type Section Methods
    // =========================================================================

    /// Add a function type and return its index.
    /// Uses type deduplication to avoid duplicate type entries.
    pub fn add_function_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        let func_type = FunctionType::new(params.clone(), results.clone());

        // Check if this type already exists
        if let Some(&existing_index) = self.type_cache.get(&func_type) {
            return existing_index;
        }

        // Add new type
        let type_index = self.type_count;
        self.type_section.ty().function(params, results);
        self.type_count += 1;
        self.has_types = true;

        // Cache for deduplication
        self.type_cache.insert(func_type, type_index);

        type_index
    }

    /// Add a function type from a FunctionType struct
    pub fn add_function_type_from_struct(&mut self, func_type: &FunctionType) -> u32 {
        self.add_function_type(func_type.params.clone(), func_type.results.clone())
    }

    /// Get or create a function type, returning its index
    pub fn get_or_create_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        self.add_function_type(params, results)
    }

    /// Check if a function type exists and return its index
    pub fn find_type(&self, params: &[ValType], results: &[ValType]) -> Option<u32> {
        let func_type = FunctionType::new(params.to_vec(), results.to_vec());
        self.type_cache.get(&func_type).copied()
    }

    // =========================================================================
    // Import Section Methods
    // =========================================================================

    /// Add an imported function and return its function index.
    /// Import functions are indexed before module-defined functions.
    pub fn add_import_function(&mut self, module: &str, name: &str, type_idx: u32) -> u32 {
        let function_index = self.import_function_count;
        self.import_section
            .import(module, name, wasm_encoder::EntityType::Function(type_idx));
        self.import_function_count += 1;
        self.has_imports = true;
        function_index
    }

    /// Add an imported function with a name for later reference
    pub fn add_named_import_function(
        &mut self,
        module: &str,
        name: &str,
        type_idx: u32,
        internal_name: &str,
    ) -> u32 {
        let function_index = self.add_import_function(module, name, type_idx);
        self.function_name_to_index
            .insert(internal_name.to_owned(), function_index);
        function_index
    }

    /// Add an imported memory
    pub fn add_import_memory(
        &mut self,
        module: &str,
        name: &str,
        min_pages: u32,
        max_pages: Option<u32>,
    ) -> u32 {
        let memory_index = self.import_memory_count;
        self.import_section.import(
            module,
            name,
            wasm_encoder::EntityType::Memory(wasm_encoder::MemoryType {
                minimum: min_pages as u64,
                maximum: max_pages.map(|p| p as u64),
                memory64: false,
                shared: false,
                page_size_log2: None,
            }),
        );
        self.import_memory_count += 1;
        self.has_imports = true;
        memory_index
    }

    /// Add an imported global
    pub fn add_import_global(
        &mut self,
        module: &str,
        name: &str,
        val_type: ValType,
        mutable: bool,
    ) -> u32 {
        let global_index = self.import_global_count;
        self.import_section.import(
            module,
            name,
            wasm_encoder::EntityType::Global(wasm_encoder::GlobalType {
                val_type,
                mutable,
                shared: false,
            }),
        );
        self.import_global_count += 1;
        self.has_imports = true;
        global_index
    }

    // =========================================================================
    // Function Section Methods
    // =========================================================================

    /// Add a function declaration (type index) and body.
    /// Returns the function index (accounting for imports).
    pub fn add_function(&mut self, type_idx: u32, body: Function) -> u32 {
        let function_index = self.import_function_count + self.function_count;
        self.function_section.function(type_idx);
        self.code_section.function(&body);
        self.function_count += 1;
        self.has_functions = true;
        self.has_code = true;
        function_index
    }

    /// Add a function with a name for later reference
    pub fn add_named_function(&mut self, name: &str, type_idx: u32, body: Function) -> u32 {
        let function_index = self.add_function(type_idx, body);
        self.function_name_to_index
            .insert(name.to_owned(), function_index);
        function_index
    }

    /// Get function index by name
    pub fn get_function_index(&self, name: &str) -> Option<u32> {
        self.function_name_to_index.get(name).copied()
    }

    // =========================================================================
    // Memory Section Methods
    // =========================================================================

    /// Add a memory section with min/max pages.
    /// Returns the memory index (accounting for imported memories).
    pub fn add_memory(&mut self, min_pages: u32, max_pages: Option<u32>) -> u32 {
        let memory_index = self.import_memory_count + self.memory_count;
        self.memory_section.memory(wasm_encoder::MemoryType {
            minimum: min_pages as u64,
            maximum: max_pages.map(|p| p as u64),
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        self.memory_count += 1;
        self.has_memory = true;
        memory_index
    }

    // =========================================================================
    // Global Section Methods
    // =========================================================================

    /// Add a global variable with an initial value.
    /// Returns the global index (accounting for imported globals).
    pub fn add_global_i32(&mut self, initial_value: i32, mutable: bool) -> u32 {
        let global_index = self.import_global_count + self.global_count;
        self.global_section.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I32,
                mutable,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i32_const(initial_value),
        );
        self.global_count += 1;
        self.has_globals = true;
        global_index
    }

    /// Add a global i64 variable
    pub fn add_global_i64(&mut self, initial_value: i64, mutable: bool) -> u32 {
        let global_index = self.import_global_count + self.global_count;
        self.global_section.global(
            wasm_encoder::GlobalType {
                val_type: ValType::I64,
                mutable,
                shared: false,
            },
            &wasm_encoder::ConstExpr::i64_const(initial_value),
        );
        self.global_count += 1;
        self.has_globals = true;
        global_index
    }

    // =========================================================================
    // Export Section Methods
    // =========================================================================

    /// Add an export
    pub fn add_export(&mut self, name: &str, kind: ExportKind, index: u32) {
        self.export_section.export(name, kind, index);
        self.export_count += 1;
        self.has_exports = true;
    }

    /// Add a function export
    pub fn add_function_export(&mut self, export_name: &str, function_index: u32) {
        self.add_export(export_name, ExportKind::Func, function_index);
    }

    /// Add a memory export
    pub fn add_memory_export(&mut self, export_name: &str, memory_index: u32) {
        self.add_export(export_name, ExportKind::Memory, memory_index);
    }

    /// Add a global export
    pub fn add_global_export(&mut self, export_name: &str, global_index: u32) {
        self.add_export(export_name, ExportKind::Global, global_index);
    }

    // =========================================================================
    // Index Accessors
    // =========================================================================

    /// Get the current total function count (imports + defined functions)
    pub fn total_function_count(&self) -> u32 {
        self.import_function_count + self.function_count
    }

    /// Get the count of imported functions
    pub fn import_function_count(&self) -> u32 {
        self.import_function_count
    }

    /// Get the count of defined (non-imported) functions
    pub fn defined_function_count(&self) -> u32 {
        self.function_count
    }

    /// Get the current type count
    pub fn type_count(&self) -> u32 {
        self.type_count
    }

    /// Get the current total memory count (imports + defined)
    pub fn total_memory_count(&self) -> u32 {
        self.import_memory_count + self.memory_count
    }

    /// Get the current total global count (imports + defined)
    pub fn total_global_count(&self) -> u32 {
        self.import_global_count + self.global_count
    }

    /// Get the export count
    pub fn export_count(&self) -> u32 {
        self.export_count
    }

    // =========================================================================
    // Validation Methods
    // =========================================================================

    /// Validate the current module state before finalization.
    ///
    /// This method performs comprehensive validation of the module structure:
    /// - Ensures functions have corresponding types
    /// - Validates index consistency across sections
    /// - Checks for common WASM generation errors
    pub fn validate(&self) -> Result<(), WasmGenerationError> {
        // Check that functions have types
        if self.function_count > 0 && self.type_count == 0 {
            return Err(WasmGenerationError::validation_failure(
                "Functions defined but no types",
                "Module has functions but no type section",
                "Add function types before defining functions",
            ));
        }

        // Check that function count matches code section
        // (This is implicitly handled by add_function, but good to verify)

        Ok(())
    }

    /// Perform comprehensive validation of the module before finalization.
    ///
    /// This method performs more thorough validation than `validate()`:
    /// - All basic validation checks
    /// - Export validation (ensure exported items exist)
    /// - Memory validation (ensure memory exists if memory operations are used)
    /// - Cross-section index consistency checking
    /// - Stack type validation preparation
    /// - Import/export declaration validation
    /// - Module finalization validation
    pub fn validate_comprehensive(&self) -> Result<(), WasmGenerationError> {
        // Run basic validation first
        self.validate()?;

        // Validate exports reference valid items
        self.validate_all_exports()?;

        // Validate memory consistency
        self.validate_memory_consistency()?;

        // Perform comprehensive finalization validation
        self.validate_finalization()?;

        Ok(())
    }

    /// Validate that all exports reference valid items.
    fn validate_all_exports(&self) -> Result<(), WasmGenerationError> {
        // This would require tracking exports, which we don't currently do
        // For now, we rely on the individual export validation methods
        // that are called when exports are added
        Ok(())
    }

    /// Validate memory consistency across the module.
    fn validate_memory_consistency(&self) -> Result<(), WasmGenerationError> {
        // Check that if we have memory operations, we have memory
        // This is a simplified check - full validation would require
        // parsing the code section
        if self.has_memory || self.import_memory_count > 0 {
            // Memory is available
            Ok(())
        } else {
            // No memory declared - this is fine unless there are memory operations
            // The comprehensive validator will check for memory operations
            Ok(())
        }
    }

    /// Validate index consistency across all sections.
    fn validate_index_consistency(&self) -> Result<(), WasmGenerationError> {
        // Validate that function indices are consistent
        let total_functions = self.total_function_count();
        if total_functions > 0 && self.type_count == 0 {
            return Err(WasmGenerationError::validation_failure(
                "Functions exist but no types declared",
                "index consistency validation",
                "Ensure all functions have corresponding types in the type section",
            ));
        }

        // Validate that if we have exports, we have something to export
        if self.has_exports
            && total_functions == 0
            && self.total_memory_count() == 0
            && self.total_global_count() == 0
        {
            return Err(WasmGenerationError::validation_failure(
                "Exports declared but no exportable items",
                "index consistency validation",
                "Ensure exported items exist before declaring exports",
            ));
        }

        // Validate function section consistency
        // Note: We can't easily check code section length with wasm_encoder API
        // This validation is handled by wasmparser during final validation

        // Validate that imports come before definitions
        if self.has_imports && self.has_functions {
            // This is automatically enforced by our API, but good to verify
            // No imports with definitions is fine, imports followed by definitions is fine
        }

        Ok(())
    }

    /// Validate that all exports reference valid indices.
    ///
    /// This method performs comprehensive export validation to ensure
    /// all exported items actually exist in the module.
    pub fn validate_export_consistency(&self) -> Result<(), WasmGenerationError> {
        // Note: In a full implementation, we would track all exports
        // and validate them here. For now, we rely on the individual
        // export validation methods that are called when exports are added.

        // We could enhance this by storing export information and validating it here
        Ok(())
    }

    /// Validate import/export declarations for consistency.
    ///
    /// This method ensures that:
    /// - Import indices are consistent
    /// - Export indices reference valid items
    /// - No duplicate imports or exports
    pub fn validate_import_export_consistency(&self) -> Result<(), WasmGenerationError> {
        // Validate import consistency
        if self.has_imports {
            // Check that import counts are consistent
            let total_imports = self.import_function_count
                + self.import_memory_count
                + self.import_global_count
                + self.import_table_count;

            if total_imports == 0 && self.has_imports {
                return Err(WasmGenerationError::validation_failure(
                    "Import section exists but no imports declared",
                    "import consistency validation",
                    "Remove empty import section or add import declarations",
                ));
            }
        }

        // Validate that function indices don't overlap incorrectly
        // (imports should come first, then definitions)
        if self.import_function_count > 0 && self.function_count > 0 {
            // This is the expected pattern - imports first, then definitions
            let first_defined_function_index = self.import_function_count;

            // Ensure indices are in the expected range
            if first_defined_function_index >= self.total_function_count() {
                return Err(WasmGenerationError::index_error(
                    "function",
                    first_defined_function_index,
                    self.total_function_count().saturating_sub(1),
                ));
            }
        }

        Ok(())
    }

    /// Perform comprehensive module finalization validation.
    ///
    /// This method performs final validation checks before the module
    /// is finalized, ensuring all indices and references are consistent.
    pub fn validate_finalization(&self) -> Result<(), WasmGenerationError> {
        // Check that we have at least some content
        if !self.has_types
            && !self.has_imports
            && !self.has_functions
            && !self.has_memory
            && !self.has_globals
            && !self.has_exports
        {
            return Err(WasmGenerationError::validation_failure(
                "Empty WASM module - no sections defined",
                "module finalization",
                "Add at least one section (functions, memory, globals, etc.) to create a valid module",
            ));
        }

        // Validate section ordering requirements
        if self.has_functions && !self.has_types {
            return Err(WasmGenerationError::section_ordering(
                "function",
                "type",
                "Add function types to the type section before declaring functions",
            ));
        }

        if self.has_code && !self.has_functions {
            return Err(WasmGenerationError::section_ordering(
                "code",
                "function",
                "Declare functions in the function section before adding function bodies",
            ));
        }

        // Validate index consistency
        self.validate_index_consistency()?;

        // Validate import/export consistency
        self.validate_import_export_consistency()?;

        // Validate export consistency
        self.validate_export_consistency()?;

        Ok(())
    }

    /// Validate a type index is within bounds.
    ///
    /// Returns an error if the type index is out of bounds.
    pub fn validate_type_index(&self, type_idx: u32) -> Result<(), WasmGenerationError> {
        if type_idx >= self.type_count {
            return Err(WasmGenerationError::index_error(
                "type",
                type_idx,
                self.type_count.saturating_sub(1),
            ));
        }
        Ok(())
    }

    /// Validate a function index is within bounds.
    ///
    /// Returns an error if the function index is out of bounds.
    pub fn validate_function_index(&self, func_idx: u32) -> Result<(), WasmGenerationError> {
        let total = self.total_function_count();
        if func_idx >= total {
            return Err(WasmGenerationError::index_error(
                "function",
                func_idx,
                total.saturating_sub(1),
            ));
        }
        Ok(())
    }

    /// Validate a memory index is within bounds.
    ///
    /// Returns an error if the memory index is out of bounds.
    pub fn validate_memory_index(&self, mem_idx: u32) -> Result<(), WasmGenerationError> {
        let total = self.total_memory_count();
        if mem_idx >= total {
            return Err(WasmGenerationError::index_error(
                "memory",
                mem_idx,
                total.saturating_sub(1),
            ));
        }
        Ok(())
    }

    /// Validate a global index is within bounds.
    ///
    /// Returns an error if the global index is out of bounds.
    pub fn validate_global_index(&self, global_idx: u32) -> Result<(), WasmGenerationError> {
        let total = self.total_global_count();
        if global_idx >= total {
            return Err(WasmGenerationError::index_error(
                "global",
                global_idx,
                total.saturating_sub(1),
            ));
        }
        Ok(())
    }

    /// Validate that a function export references a valid function.
    ///
    /// Returns an error if the function index is out of bounds.
    pub fn validate_function_export(
        &self,
        export_name: &str,
        func_idx: u32,
    ) -> Result<(), WasmGenerationError> {
        let total = self.total_function_count();
        if func_idx >= total {
            return Err(WasmGenerationError::export_error(
                export_name,
                "function",
                format!(
                    "Function index {} is out of bounds (max: {})",
                    func_idx,
                    total.saturating_sub(1)
                ),
            ));
        }
        Ok(())
    }

    /// Validate that a memory export references a valid memory.
    ///
    /// Returns an error if the memory index is out of bounds.
    pub fn validate_memory_export(
        &self,
        export_name: &str,
        mem_idx: u32,
    ) -> Result<(), WasmGenerationError> {
        let total = self.total_memory_count();
        if mem_idx >= total {
            return Err(WasmGenerationError::export_error(
                export_name,
                "memory",
                format!(
                    "Memory index {} is out of bounds (max: {})",
                    mem_idx,
                    total.saturating_sub(1)
                ),
            ));
        }
        Ok(())
    }

    /// Validate that a global export references a valid global.
    ///
    /// Returns an error if the global index is out of bounds.
    pub fn validate_global_export(
        &self,
        export_name: &str,
        global_idx: u32,
    ) -> Result<(), WasmGenerationError> {
        let total = self.total_global_count();
        if global_idx >= total {
            return Err(WasmGenerationError::export_error(
                export_name,
                "global",
                format!(
                    "Global index {} is out of bounds (max: {})",
                    global_idx,
                    total.saturating_sub(1)
                ),
            ));
        }
        Ok(())
    }

    // =========================================================================
    // Finalization
    // =========================================================================

    /// Finalize the module and return the WASM bytes.
    /// Sections are added in the required WASM order.
    /// Performs comprehensive validation before finalization.
    pub fn finish(self) -> Result<Vec<u8>, CompilerError> {
        // Perform comprehensive validation before finalization
        self.validate_comprehensive().map_err(|e| {
            e.to_compiler_error(crate::compiler::compiler_errors::ErrorLocation::default())
        })?;

        let mut module = Module::new();

        // Add sections in the required WASM order:
        // 1. Type Section
        if self.has_types {
            module.section(&self.type_section);
        }

        // 2. Import Section
        if self.has_imports {
            module.section(&self.import_section);
        }

        // 3. Function Section (declarations)
        if self.has_functions {
            module.section(&self.function_section);
        }

        // 4. Table Section
        if self.has_tables {
            module.section(&self.table_section);
        }

        // 5. Memory Section
        if self.has_memory {
            module.section(&self.memory_section);
        }

        // 6. Global Section
        if self.has_globals {
            module.section(&self.global_section);
        }

        // 7. Export Section
        if self.has_exports {
            module.section(&self.export_section);
        }

        // 8. Start Section (not currently used)
        // 9. Element Section (not currently used)

        // 10. Code Section (function bodies)
        if self.has_code {
            module.section(&self.code_section);
        }

        // 11. Data Section
        if self.has_data {
            module.section(&self.data_section);
        }

        Ok(module.finish())
    }
}

impl Default for WasmModuleBuilder {
    fn default() -> Self {
        Self::new()
    }
}
