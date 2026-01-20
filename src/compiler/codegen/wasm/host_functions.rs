//! Host Function Integration for WASM Codegen
//!
//! This module handles the integration of host functions into WASM modules.
//! It provides:
//! - WASM import section generation for host functions
//! - Import module organization and naming
//! - Type compatibility checking between Beanstalk and host
//! - Export handling for main and host interface functions
//!
//! Host functions in Beanstalk are external functions provided by the runtime
//! environment (e.g., JavaScript in web contexts, native functions in CLI).
//! They are imported into the WASM module and called like regular functions.

// Many functions are prepared for later integration phases
#![allow(dead_code)]

use crate::compiler::codegen::wasm::analyzer::{FunctionSignature, WasmType};
use crate::compiler::codegen::wasm::error::WasmGenerationError;
use crate::compiler::codegen::wasm::module_builder::WasmModuleBuilder;
use crate::compiler::compiler_errors::{CompilerError, ErrorLocation};
use crate::compiler::host_functions::registry::{HostFunctionDef, HostRegistry, JsFunctionDef};
use crate::compiler::string_interning::StringTable;
use std::collections::HashMap;
use wasm_encoder::{ExportKind, ValType};

/// Represents a host function import for WASM generation
#[derive(Debug, Clone)]
pub struct HostImport {
    /// WASM import module name (e.g., "beanstalk_io")
    pub module_name: String,
    /// WASM import function name (e.g., "io")
    pub function_name: String,
    /// Beanstalk function name (for lookup)
    pub beanstalk_name: String,
    /// Parameter types in WASM format
    pub params: Vec<ValType>,
    /// Return types in WASM format
    pub returns: Vec<ValType>,
    /// Assigned WASM function index
    pub wasm_index: Option<u32>,
}

impl HostImport {
    /// Create a new host import from a host function definition
    pub fn from_host_def(def: &HostFunctionDef, string_table: &StringTable) -> Self {
        let module_name = string_table.resolve(def.module).to_string();
        let function_name = string_table.resolve(def.import_name).to_string();
        let beanstalk_name = string_table.resolve(def.name).to_string();

        // Convert Beanstalk types to WASM types
        // For now, we use the JavaScript mapping which provides the actual WASM types
        // The host function def uses Beanstalk types which need conversion
        HostImport {
            module_name,
            function_name,
            beanstalk_name,
            params: Vec::new(), // Will be filled from JS mapping
            returns: Vec::new(),
            wasm_index: None,
        }
    }

    /// Create a host import from a JavaScript function definition
    pub fn from_js_def(js_def: &JsFunctionDef, beanstalk_name: &str) -> Self {
        HostImport {
            module_name: js_def.module.clone(),
            function_name: js_def.name.clone(),
            beanstalk_name: beanstalk_name.to_string(),
            params: js_def.parameters.clone(),
            returns: js_def.returns.clone(),
            wasm_index: None,
        }
    }

    /// Get the function signature for this import
    pub fn to_signature(&self) -> FunctionSignature {
        let params: Vec<WasmType> = self
            .params
            .iter()
            .filter_map(|v| WasmType::from_val_type(*v))
            .collect();
        let returns: Vec<WasmType> = self
            .returns
            .iter()
            .filter_map(|v| WasmType::from_val_type(*v))
            .collect();

        FunctionSignature::host_import(
            params,
            returns,
            self.module_name.clone(),
            self.function_name.clone(),
        )
    }
}

/// Represents a function export for WASM generation
#[derive(Debug, Clone)]
pub struct FunctionExport {
    /// Export name (visible to host)
    pub export_name: String,
    /// Internal function name
    pub internal_name: String,
    /// WASM function index
    pub function_index: u32,
    /// Export kind (always Func for functions)
    pub kind: ExportKind,
}

impl FunctionExport {
    /// Create a new function export
    pub fn new(export_name: &str, internal_name: &str, function_index: u32) -> Self {
        FunctionExport {
            export_name: export_name.to_string(),
            internal_name: internal_name.to_string(),
            function_index,
            kind: ExportKind::Func,
        }
    }

    /// Create a main function export
    pub fn main(function_index: u32) -> Self {
        FunctionExport {
            export_name: "main".to_string(),
            internal_name: "main".to_string(),
            function_index,
            kind: ExportKind::Func,
        }
    }
}

/// Represents a memory export for WASM generation
#[derive(Debug, Clone)]
pub struct MemoryExport {
    /// Export name (visible to host)
    pub export_name: String,
    /// WASM memory index
    pub memory_index: u32,
}

impl MemoryExport {
    /// Create a new memory export
    pub fn new(export_name: &str, memory_index: u32) -> Self {
        MemoryExport {
            export_name: export_name.to_string(),
            memory_index,
        }
    }

    /// Create the standard "memory" export (index 0)
    pub fn standard() -> Self {
        MemoryExport {
            export_name: "memory".to_string(),
            memory_index: 0,
        }
    }
}

/// Represents a global export for WASM generation
#[derive(Debug, Clone)]
pub struct GlobalExport {
    /// Export name (visible to host)
    pub export_name: String,
    /// WASM global index
    pub global_index: u32,
}

impl GlobalExport {
    /// Create a new global export
    pub fn new(export_name: &str, global_index: u32) -> Self {
        GlobalExport {
            export_name: export_name.to_string(),
            global_index,
        }
    }
}

/// Manages host function imports and exports for WASM generation
pub struct HostFunctionManager {
    /// Registered host imports
    imports: Vec<HostImport>,
    /// Registered function exports
    function_exports: Vec<FunctionExport>,
    /// Registered memory exports
    memory_exports: Vec<MemoryExport>,
    /// Registered global exports
    global_exports: Vec<GlobalExport>,
    /// Map from Beanstalk function name to WASM function index
    name_to_index: HashMap<String, u32>,
    /// Next available import function index
    next_import_index: u32,
}

impl HostFunctionManager {
    /// Create a new host function manager
    pub fn new() -> Self {
        HostFunctionManager {
            imports: Vec::new(),
            function_exports: Vec::new(),
            memory_exports: Vec::new(),
            global_exports: Vec::new(),
            name_to_index: HashMap::new(),
            next_import_index: 0,
        }
    }

    /// Register host functions from a registry
    pub fn register_from_registry(
        &mut self,
        registry: &HostRegistry,
        string_table: &StringTable,
    ) -> Result<(), CompilerError> {
        // Get all JavaScript mappings (these have the actual WASM types)
        for (beanstalk_name, js_def) in registry.list_js_mappings() {
            let name_str = string_table.resolve(*beanstalk_name);
            let import = HostImport::from_js_def(js_def, name_str);
            self.register_import(import)?;
        }

        Ok(())
    }

    /// Register a single host import
    pub fn register_import(&mut self, mut import: HostImport) -> Result<u32, CompilerError> {
        // Check for duplicate registration
        if self.name_to_index.contains_key(&import.beanstalk_name) {
            return Err(WasmGenerationError::lir_analysis(
                format!(
                    "Host function '{}' is already registered",
                    import.beanstalk_name
                ),
                "register_import",
            )
            .to_compiler_error(ErrorLocation::default()));
        }

        // Assign index
        let index = self.next_import_index;
        import.wasm_index = Some(index);
        self.next_import_index += 1;

        // Store mapping
        self.name_to_index
            .insert(import.beanstalk_name.clone(), index);
        self.imports.push(import);

        Ok(index)
    }

    /// Register a function export
    pub fn register_export(&mut self, export: FunctionExport) {
        self.function_exports.push(export);
    }

    /// Register a memory export
    pub fn register_memory_export(&mut self, export: MemoryExport) {
        self.memory_exports.push(export);
    }

    /// Register a global export
    pub fn register_global_export(&mut self, export: GlobalExport) {
        self.global_exports.push(export);
    }

    /// Get the WASM function index for a host function by name
    pub fn get_function_index(&self, beanstalk_name: &str) -> Option<u32> {
        self.name_to_index.get(beanstalk_name).copied()
    }

    /// Check if a function is a host import
    pub fn is_host_import(&self, beanstalk_name: &str) -> bool {
        self.name_to_index.contains_key(beanstalk_name)
    }

    /// Get all registered imports
    pub fn get_imports(&self) -> &[HostImport] {
        &self.imports
    }

    /// Get all registered function exports
    pub fn get_exports(&self) -> &[FunctionExport] {
        &self.function_exports
    }

    /// Get all registered memory exports
    pub fn get_memory_exports(&self) -> &[MemoryExport] {
        &self.memory_exports
    }

    /// Get all registered global exports
    pub fn get_global_exports(&self) -> &[GlobalExport] {
        &self.global_exports
    }

    /// Get the number of imported functions
    pub fn import_count(&self) -> u32 {
        self.next_import_index
    }

    /// Get the signature for a host function
    pub fn get_signature(&self, beanstalk_name: &str) -> Option<FunctionSignature> {
        self.imports
            .iter()
            .find(|i| i.beanstalk_name == beanstalk_name)
            .map(|i| i.to_signature())
    }

    /// Apply all imports to a WASM module builder
    pub fn apply_imports(
        &self,
        builder: &mut WasmModuleBuilder,
    ) -> Result<HashMap<String, u32>, CompilerError> {
        let mut index_map = HashMap::new();

        for import in &self.imports {
            // First, add the function type
            let type_idx = builder.add_function_type(import.params.clone(), import.returns.clone());

            // Then add the import
            let func_idx =
                builder.add_import_function(&import.module_name, &import.function_name, type_idx);

            index_map.insert(import.beanstalk_name.clone(), func_idx);
        }

        Ok(index_map)
    }

    /// Apply all exports to a WASM module builder
    pub fn apply_exports(&self, builder: &mut WasmModuleBuilder) {
        // Apply function exports
        for export in &self.function_exports {
            builder.add_export(&export.export_name, export.kind, export.function_index);
        }

        // Apply memory exports
        for export in &self.memory_exports {
            builder.add_memory_export(&export.export_name, export.memory_index);
        }

        // Apply global exports
        for export in &self.global_exports {
            builder.add_global_export(&export.export_name, export.global_index);
        }
    }

    /// Validate that all exports reference valid indices
    pub fn validate_exports(&self, builder: &WasmModuleBuilder) -> Result<(), WasmGenerationError> {
        // Validate function exports
        for export in &self.function_exports {
            builder.validate_function_index(export.function_index)?;
        }

        // Validate memory exports
        for export in &self.memory_exports {
            builder.validate_memory_index(export.memory_index)?;
        }

        // Validate global exports
        for export in &self.global_exports {
            builder.validate_global_index(export.global_index)?;
        }

        Ok(())
    }

    /// Validate type compatibility between Beanstalk and host function
    pub fn validate_type_compatibility(
        &self,
        beanstalk_name: &str,
        expected_params: &[WasmType],
        expected_returns: &[WasmType],
    ) -> Result<(), WasmGenerationError> {
        let import = self
            .imports
            .iter()
            .find(|i| i.beanstalk_name == beanstalk_name)
            .ok_or_else(|| {
                WasmGenerationError::lir_analysis(
                    format!("Host function '{}' not found", beanstalk_name),
                    "validate_type_compatibility",
                )
            })?;

        // Convert import params to WasmType for comparison
        let import_params: Vec<WasmType> = import
            .params
            .iter()
            .filter_map(|v| WasmType::from_val_type(*v))
            .collect();
        let import_returns: Vec<WasmType> = import
            .returns
            .iter()
            .filter_map(|v| WasmType::from_val_type(*v))
            .collect();

        // Check parameter count
        if import_params.len() != expected_params.len() {
            return Err(WasmGenerationError::SignatureMismatch {
                expected: format!("{} parameters", expected_params.len()),
                found: format!("{} parameters", import_params.len()),
                function_name: beanstalk_name.to_string(),
            });
        }

        // Check return count
        if import_returns.len() != expected_returns.len() {
            return Err(WasmGenerationError::SignatureMismatch {
                expected: format!("{} returns", expected_returns.len()),
                found: format!("{} returns", import_returns.len()),
                function_name: beanstalk_name.to_string(),
            });
        }

        // Check parameter types
        for (i, (expected, actual)) in expected_params.iter().zip(import_params.iter()).enumerate()
        {
            if expected != actual {
                return Err(WasmGenerationError::SignatureMismatch {
                    expected: format!("parameter {} to be {}", i, expected),
                    found: format!("{}", actual),
                    function_name: beanstalk_name.to_string(),
                });
            }
        }

        // Check return types
        for (i, (expected, actual)) in expected_returns
            .iter()
            .zip(import_returns.iter())
            .enumerate()
        {
            if expected != actual {
                return Err(WasmGenerationError::SignatureMismatch {
                    expected: format!("return {} to be {}", i, expected),
                    found: format!("{}", actual),
                    function_name: beanstalk_name.to_string(),
                });
            }
        }

        Ok(())
    }
}

impl Default for HostFunctionManager {
    fn default() -> Self {
        Self::new()
    }
}

/// Helper function to create standard Beanstalk host imports
pub fn create_standard_host_imports() -> Vec<HostImport> {
    vec![
        // io() function - outputs to stdout
        HostImport {
            module_name: "beanstalk_io".to_string(),
            function_name: "io".to_string(),
            beanstalk_name: "io".to_string(),
            params: vec![ValType::I32, ValType::I32], // ptr, len for string
            returns: vec![],
            wasm_index: None,
        },
    ]
}

/// Validate that all required host functions are available
pub fn validate_required_host_functions(
    manager: &HostFunctionManager,
) -> Result<(), WasmGenerationError> {
    // The io() function is mandatory
    if !manager.is_host_import("io") {
        return Err(WasmGenerationError::lir_analysis(
            "Required host function 'io' is not registered. Every Beanstalk module requires the io() function.",
            "validate_required_host_functions",
        ));
    }

    Ok(())
}
