//! LIR Analyzer
//!
//! This module analyzes LIR modules to extract type information and build
//! mapping tables needed for WASM generation. The analyzer performs:
//! - Local variable analysis and type extraction
//! - Function signature analysis and consistency checking
//! - Struct layout calculation
//! - Import/export identification
//! - Type mapping between LIR and WASM types

// Many methods and fields are prepared for later implementation phases
// (ownership system, host functions, memory model integration)
#![allow(dead_code)]

use crate::backends::lir::nodes::{LirFunction, LirModule, LirStruct, LirType};
use crate::backends::wasm::error::WasmGenerationError;
use crate::compiler_frontend::compiler_errors::CompilerError;
use std::collections::HashMap;
use wasm_encoder::ValType;

/// Analyzes LIR modules and extracts information needed for WASM generation.
///
/// The analyzer builds mapping tables for:
/// - Local variables (LIR local ID -> WASM local index)
/// - Function signatures (parameters and return types)
/// - Struct layouts (field offsets and alignment)
/// - Import/export declarations
pub struct LirAnalyzer {
    /// Maps function name -> (local_id -> WasmType)
    local_types: HashMap<String, HashMap<u32, WasmType>>,
    /// Maps function name -> FunctionSignature
    function_signatures: HashMap<String, FunctionSignature>,
    /// Maps struct name -> StructLayout
    struct_layouts: HashMap<String, StructLayout>,
    /// List of imported functions from host environment
    import_functions: Vec<ImportFunction>,
    /// List of exported functions
    export_functions: Vec<ExportFunction>,
    /// Maps function name -> function index in WASM module
    function_indices: HashMap<String, u32>,
    /// Type index cache for deduplication (signature hash -> type index)
    type_index_cache: HashMap<SignatureKey, u32>,
    /// Next available type index
    next_type_index: u32,
}

/// WASM-compatible type representation
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WasmType {
    I32,
    I64,
    F32,
    F64,
}

impl WasmType {
    /// Convert to wasm_encoder ValType
    pub fn to_val_type(&self) -> ValType {
        match self {
            WasmType::I32 => ValType::I32,
            WasmType::I64 => ValType::I64,
            WasmType::F32 => ValType::F32,
            WasmType::F64 => ValType::F64,
        }
    }

    /// Get alignment requirement in bytes
    pub fn alignment(&self) -> u32 {
        match self {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
        }
    }

    /// Get size in bytes
    pub fn size_bytes(&self) -> u32 {
        match self {
            WasmType::I32 | WasmType::F32 => 4,
            WasmType::I64 | WasmType::F64 => 8,
        }
    }

    /// Convert from LirType to WasmType
    pub fn from_lir_type(lir_type: LirType) -> Self {
        match lir_type {
            LirType::I32 => WasmType::I32,
            LirType::I64 => WasmType::I64,
            LirType::F32 => WasmType::F32,
            LirType::F64 => WasmType::F64,
        }
    }

    /// Convert from ValType to WasmType
    pub fn from_val_type(val_type: ValType) -> Option<Self> {
        match val_type {
            ValType::I32 => Some(WasmType::I32),
            ValType::I64 => Some(WasmType::I64),
            ValType::F32 => Some(WasmType::F32),
            ValType::F64 => Some(WasmType::F64),
            _ => None, // Reference types not supported yet
        }
    }

    /// Get a human-readable name for the type
    pub fn type_name(&self) -> &'static str {
        match self {
            WasmType::I32 => "i32",
            WasmType::I64 => "i64",
            WasmType::F32 => "f32",
            WasmType::F64 => "f64",
        }
    }
}

impl From<LirType> for WasmType {
    fn from(lir_type: LirType) -> Self {
        WasmType::from_lir_type(lir_type)
    }
}

impl std::fmt::Display for WasmType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.type_name())
    }
}

/// Key for signature deduplication in type section
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SignatureKey {
    pub params: Vec<WasmType>,
    pub results: Vec<WasmType>,
}

impl SignatureKey {
    /// Create a new signature key
    pub fn new(params: Vec<WasmType>, results: Vec<WasmType>) -> Self {
        SignatureKey { params, results }
    }

    /// Create from a FunctionSignature
    pub fn from_signature(sig: &FunctionSignature) -> Self {
        SignatureKey {
            params: sig.parameters.clone(),
            results: sig.returns.clone(),
        }
    }

    /// Convert to ValType vectors for wasm_encoder
    pub fn to_val_types(&self) -> (Vec<ValType>, Vec<ValType>) {
        let params: Vec<ValType> = self.params.iter().map(|t| t.to_val_type()).collect();
        let results: Vec<ValType> = self.results.iter().map(|t| t.to_val_type()).collect();
        (params, results)
    }
}

/// Function signature information for WASM generation
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types in order
    pub parameters: Vec<WasmType>,
    /// Return types (WASM supports multi-value returns)
    pub returns: Vec<WasmType>,
    /// Whether this is the main entry point function
    pub is_main: bool,
    /// Whether this function is imported from host
    pub is_host_import: bool,
    /// Import module name (if is_host_import is true)
    pub import_module: Option<String>,
    /// Import function name (if is_host_import is true)
    pub import_name: Option<String>,
}

impl FunctionSignature {
    /// Create a new function signature
    pub fn new(parameters: Vec<WasmType>, returns: Vec<WasmType>) -> Self {
        FunctionSignature {
            parameters,
            returns,
            is_main: false,
            is_host_import: false,
            import_module: None,
            import_name: None,
        }
    }

    /// Create a signature for a main function
    pub fn main_function(parameters: Vec<WasmType>, returns: Vec<WasmType>) -> Self {
        FunctionSignature {
            parameters,
            returns,
            is_main: true,
            is_host_import: false,
            import_module: None,
            import_name: None,
        }
    }

    /// Create a signature for an imported host function
    pub fn host_import(
        parameters: Vec<WasmType>,
        returns: Vec<WasmType>,
        module: String,
        name: String,
    ) -> Self {
        FunctionSignature {
            parameters,
            returns,
            is_main: false,
            is_host_import: true,
            import_module: Some(module),
            import_name: Some(name),
        }
    }

    /// Check if this signature is compatible with another signature.
    /// Two signatures are compatible if they have the same parameter and return types.
    pub fn is_compatible_with(&self, other: &FunctionSignature) -> bool {
        self.parameters == other.parameters && self.returns == other.returns
    }

    /// Check if this signature matches the given parameter and return types
    pub fn matches(&self, params: &[WasmType], returns: &[WasmType]) -> bool {
        self.parameters.as_slice() == params && self.returns.as_slice() == returns
    }

    /// Get the signature key for type deduplication
    pub fn to_signature_key(&self) -> SignatureKey {
        SignatureKey::from_signature(self)
    }

    /// Convert to ValType vectors for wasm_encoder
    pub fn to_val_types(&self) -> (Vec<ValType>, Vec<ValType>) {
        let params: Vec<ValType> = self.parameters.iter().map(|t| t.to_val_type()).collect();
        let results: Vec<ValType> = self.returns.iter().map(|t| t.to_val_type()).collect();
        (params, results)
    }

    /// Get a human-readable representation of the signature
    pub fn signature_string(&self) -> String {
        let params: Vec<&str> = self.parameters.iter().map(|t| t.type_name()).collect();
        let returns: Vec<&str> = self.returns.iter().map(|t| t.type_name()).collect();
        format!("({}) -> ({})", params.join(", "), returns.join(", "))
    }

    /// Check if this is a void function (no parameters, no returns)
    pub fn is_void(&self) -> bool {
        self.parameters.is_empty() && self.returns.is_empty()
    }

    /// Get the number of parameters
    pub fn param_count(&self) -> usize {
        self.parameters.len()
    }

    /// Get the number of return values
    pub fn return_count(&self) -> usize {
        self.returns.len()
    }
}

impl PartialEq for FunctionSignature {
    fn eq(&self, other: &Self) -> bool {
        // Two signatures are equal if they have the same types
        // (ignoring metadata like is_main, is_host_import, etc.)
        self.parameters == other.parameters && self.returns == other.returns
    }
}

impl std::fmt::Display for FunctionSignature {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.signature_string())
    }
}

/// Struct layout information for memory allocation
#[derive(Debug, Clone)]
pub struct StructLayout {
    /// Total size of the struct in bytes (aligned)
    pub total_size: u32,
    /// Maximum alignment requirement of any field
    pub alignment: u32,
    /// Layout information for each field
    pub fields: Vec<FieldLayout>,
    /// Name of the struct
    pub name: String,
}

impl StructLayout {
    /// Get field offset by index
    pub fn get_field_offset(&self, field_index: usize) -> Option<u32> {
        self.fields.get(field_index).map(|f| f.offset)
    }

    /// Get field by index
    pub fn get_field(&self, field_index: usize) -> Option<&FieldLayout> {
        self.fields.get(field_index)
    }
}

/// Field layout information within a struct
#[derive(Debug, Clone)]
pub struct FieldLayout {
    /// Byte offset from struct start
    pub offset: u32,
    /// Size of the field in bytes
    pub size: u32,
    /// Alignment requirement of the field
    pub alignment: u32,
    /// WASM type of the field
    pub wasm_type: WasmType,
    /// Field name (for debugging)
    pub name: String,
}

/// Import function information for WASM import section
#[derive(Debug, Clone)]
pub struct ImportFunction {
    /// Module name for the import (e.g., "env", "wasi")
    pub module_name: String,
    /// Function name within the module
    pub function_name: String,
    /// Function signature
    pub signature: FunctionSignature,
    /// Assigned WASM function index
    pub wasm_index: u32,
}

/// Export function information for WASM export section
#[derive(Debug, Clone)]
pub struct ExportFunction {
    /// Name to export as
    pub export_name: String,
    /// Internal function name
    pub function_name: String,
    /// Function signature
    pub signature: FunctionSignature,
}

/// Local variable mapping information for a function
#[derive(Debug, Clone)]
pub struct LocalMap {
    /// Maps LIR local ID to WASM local index
    pub lir_to_wasm: HashMap<u32, u32>,
    /// Number of function parameters (parameters come first in WASM local space)
    pub parameter_count: u32,
    /// WASM locals format: (count, type) pairs for the locals section
    pub local_types: Vec<(u32, ValType)>,
}

impl LocalMap {
    /// Get WASM local index for a LIR local ID
    pub fn get_wasm_index(&self, lir_local: u32) -> Option<u32> {
        self.lir_to_wasm.get(&lir_local).copied()
    }

    /// Check if a local index is a parameter
    pub fn is_parameter(&self, wasm_index: u32) -> bool {
        wasm_index < self.parameter_count
    }

    /// Get total number of locals (parameters + locals)
    pub fn total_locals(&self) -> u32 {
        self.lir_to_wasm.len() as u32
    }
}

impl LirAnalyzer {
    /// Analyze a LIR module and extract all necessary information for WASM generation.
    ///
    /// This method performs:
    /// 1. Struct analysis - calculates memory layouts for all structs
    /// 2. Function analysis - extracts signatures and local variable mappings
    /// 3. Export identification - identifies main and exported functions
    pub fn analyze_module(lir: &LirModule) -> Result<Self, CompilerError> {
        let mut analyzer = LirAnalyzer {
            local_types: HashMap::new(),
            function_signatures: HashMap::new(),
            struct_layouts: HashMap::new(),
            import_functions: Vec::new(),
            export_functions: Vec::new(),
            function_indices: HashMap::new(),
            type_index_cache: HashMap::new(),
            next_type_index: 0,
        };

        // Analyze structs first (needed for memory layout calculations)
        for lir_struct in &lir.structs {
            analyzer.analyze_struct(lir_struct)?;
        }

        // Analyze functions and build indices
        for (index, lir_function) in lir.functions.iter().enumerate() {
            analyzer
                .function_indices
                .insert(lir_function.name.clone(), index as u32);
            analyzer.analyze_function(lir_function)?;
        }

        Ok(analyzer)
    }

    /// Analyze a single function to extract signature and local variable information
    fn analyze_function(&mut self, lir_func: &LirFunction) -> Result<(), CompilerError> {
        // Build function signature from LIR types
        let parameters: Vec<WasmType> = lir_func
            .params
            .iter()
            .map(|t| WasmType::from_lir_type(*t))
            .collect();
        let returns: Vec<WasmType> = lir_func
            .returns
            .iter()
            .map(|t| WasmType::from_lir_type(*t))
            .collect();

        let signature = if lir_func.is_main {
            FunctionSignature::main_function(parameters, returns)
        } else {
            FunctionSignature::new(parameters, returns)
        };

        self.function_signatures
            .insert(lir_func.name.clone(), signature.clone());

        // Analyze local variables and build type mapping
        let mut local_map = HashMap::new();

        // Parameters are indexed first (0..param_count)
        for (index, param_type) in lir_func.params.iter().enumerate() {
            local_map.insert(index as u32, WasmType::from_lir_type(*param_type));
        }

        // Then local variables (param_count..param_count + local_count)
        let param_count = lir_func.params.len();
        for (index, local_type) in lir_func.locals.iter().enumerate() {
            local_map.insert(
                (param_count + index) as u32,
                WasmType::from_lir_type(*local_type),
            );
        }

        self.local_types.insert(lir_func.name.clone(), local_map);

        // Add to exports if it's the main function
        if lir_func.is_main {
            self.export_functions.push(ExportFunction {
                export_name: "main".to_owned(),
                function_name: lir_func.name.clone(),
                signature,
            });
        }

        Ok(())
    }

    /// Analyze a struct to calculate its memory layout
    fn analyze_struct(&mut self, lir_struct: &LirStruct) -> Result<(), CompilerError> {
        let mut fields = Vec::new();
        let mut current_offset = 0u32;
        let mut max_alignment = 1u32;

        for field in &lir_struct.fields {
            let wasm_type = WasmType::from_lir_type(field.ty);
            let field_alignment = wasm_type.alignment();
            let field_size = wasm_type.size_bytes();

            // Update maximum alignment for the struct
            max_alignment = max_alignment.max(field_alignment);

            // Align current offset to field's alignment requirement
            current_offset = align_to(current_offset, field_alignment);

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

        // Align total size to maximum alignment (ensures arrays of structs are aligned)
        // Also ensure minimum 2-byte alignment for tagged pointer support
        let final_alignment = max_alignment.max(2);
        let total_size = align_to(current_offset, final_alignment);

        // Get struct name as string
        let struct_name = format!("{:?}", lir_struct.name);

        let layout = StructLayout {
            total_size,
            alignment: max_alignment,
            fields,
            name: struct_name.clone(),
        };

        self.struct_layouts.insert(struct_name, layout);

        Ok(())
    }

    /// Get local mapping for a specific function.
    ///
    /// Returns a LocalMap that maps LIR local IDs to WASM local indices,
    /// with locals grouped by type for efficient WASM representation.
    pub fn get_local_mapping(&self, function_name: &str) -> Option<LocalMap> {
        let local_types = self.local_types.get(function_name)?;
        let signature = self.function_signatures.get(function_name)?;

        let parameter_count = signature.parameters.len() as u32;
        let mut lir_to_wasm = HashMap::new();
        let mut wasm_locals = Vec::new();

        // Parameters come first in WASM local space (indices 0..param_count)
        for i in 0..parameter_count {
            lir_to_wasm.insert(i, i);
        }

        // Group non-parameter locals by type for efficient WASM representation
        // WASM locals section uses (count, type) pairs
        let mut type_groups: HashMap<ValType, Vec<u32>> = HashMap::new();
        for (&lir_local, &wasm_type) in local_types {
            if lir_local >= parameter_count {
                type_groups
                    .entry(wasm_type.to_val_type())
                    .or_default()
                    .push(lir_local);
            }
        }

        // Assign WASM local indices and build local declarations
        // Order: I32, I64, F32, F64 for consistency
        let type_order = [ValType::I32, ValType::I64, ValType::F32, ValType::F64];
        let mut next_wasm_index = parameter_count;

        for val_type in type_order {
            if let Some(lir_locals) = type_groups.get(&val_type) {
                let count = lir_locals.len() as u32;
                if count > 0 {
                    wasm_locals.push((count, val_type));

                    for &lir_local in lir_locals {
                        lir_to_wasm.insert(lir_local, next_wasm_index);
                        next_wasm_index += 1;
                    }
                }
            }
        }

        Some(LocalMap {
            lir_to_wasm,
            parameter_count,
            local_types: wasm_locals,
        })
    }

    /// Get WASM locals for a function in v0.243.0 API format.
    ///
    /// Returns Vec<(u32, ValType)> where each tuple is (count, type).
    /// This format is used directly by wasm_encoder's Function::new().
    pub fn get_wasm_locals(&self, function_name: &str) -> Vec<(u32, ValType)> {
        self.get_local_mapping(function_name)
            .map(|mapping| mapping.local_types)
            .unwrap_or_default()
    }

    /// Get struct layout by name
    pub fn get_struct_layout(&self, struct_name: &str) -> Option<&StructLayout> {
        self.struct_layouts.get(struct_name)
    }

    /// Get function signature by name
    pub fn get_function_signature(&self, function_name: &str) -> Option<&FunctionSignature> {
        self.function_signatures.get(function_name)
    }

    /// Get function index by name
    pub fn get_function_index(&self, function_name: &str) -> Option<u32> {
        self.function_indices.get(function_name).copied()
    }

    /// Get all export functions
    pub fn get_export_functions(&self) -> &[ExportFunction] {
        &self.export_functions
    }

    /// Get all import functions
    pub fn get_import_functions(&self) -> &[ImportFunction] {
        &self.import_functions
    }

    /// Get all function signatures
    pub fn get_all_signatures(&self) -> &HashMap<String, FunctionSignature> {
        &self.function_signatures
    }

    /// Get all struct layouts
    pub fn get_all_struct_layouts(&self) -> &HashMap<String, StructLayout> {
        &self.struct_layouts
    }

    /// Add an import function to the analyzer
    pub fn add_import_function(&mut self, import: ImportFunction) {
        self.import_functions.push(import);
    }

    /// Add an export function to the analyzer
    pub fn add_export_function(&mut self, export: ExportFunction) {
        self.export_functions.push(export);
    }

    // =========================================================================
    // Type Index Management and Signature Consistency
    // =========================================================================

    /// Get or create a type index for a function signature.
    /// Uses caching to deduplicate identical signatures.
    pub fn get_or_create_type_index(&mut self, signature: &FunctionSignature) -> u32 {
        let key = signature.to_signature_key();

        if let Some(&index) = self.type_index_cache.get(&key) {
            return index;
        }

        let index = self.next_type_index;
        self.type_index_cache.insert(key, index);
        self.next_type_index += 1;
        index
    }

    /// Get the type index for a signature key if it exists
    pub fn get_type_index(&self, key: &SignatureKey) -> Option<u32> {
        self.type_index_cache.get(key).copied()
    }

    /// Check if two functions have compatible signatures
    pub fn signatures_compatible(
        &self,
        func1: &str,
        func2: &str,
    ) -> Result<bool, WasmGenerationError> {
        let sig1 = self.function_signatures.get(func1).ok_or_else(|| {
            WasmGenerationError::lir_analysis(
                format!("Function '{}' not found", func1),
                "signature_compatible",
            )
        })?;

        let sig2 = self.function_signatures.get(func2).ok_or_else(|| {
            WasmGenerationError::lir_analysis(
                format!("Function '{}' not found", func2),
                "signature_compatible",
            )
        })?;

        Ok(sig1.is_compatible_with(sig2))
    }

    /// Validate that a function call has the correct argument types.
    /// Returns an error if the argument types don't match the function signature.
    pub fn validate_call_arguments(
        &self,
        function_name: &str,
        arg_types: &[WasmType],
    ) -> Result<(), WasmGenerationError> {
        let signature = self.function_signatures.get(function_name).ok_or_else(|| {
            WasmGenerationError::lir_analysis(
                format!("Function '{}' not found", function_name),
                "validate_call_arguments",
            )
        })?;

        if arg_types.len() != signature.parameters.len() {
            return Err(WasmGenerationError::SignatureMismatch {
                expected: format!(
                    "{} parameters ({})",
                    signature.parameters.len(),
                    signature.signature_string()
                ),
                found: format!("{} arguments", arg_types.len()),
                function_name: function_name.to_owned(),
            });
        }

        for (i, (expected, actual)) in signature
            .parameters
            .iter()
            .zip(arg_types.iter())
            .enumerate()
        {
            if expected != actual {
                return Err(WasmGenerationError::SignatureMismatch {
                    expected: format!("parameter {} to be {}", i, expected),
                    found: format!("{}", actual),
                    function_name: function_name.to_owned(),
                });
            }
        }

        Ok(())
    }

    /// Get the expected return types for a function
    pub fn get_return_types(&self, function_name: &str) -> Option<&[WasmType]> {
        self.function_signatures
            .get(function_name)
            .map(|sig| sig.returns.as_slice())
    }

    /// Get the expected parameter types for a function
    pub fn get_param_types(&self, function_name: &str) -> Option<&[WasmType]> {
        self.function_signatures
            .get(function_name)
            .map(|sig| sig.parameters.as_slice())
    }

    /// Get all unique signature keys (for building the type section)
    pub fn get_unique_signatures(&self) -> Vec<SignatureKey> {
        let mut signatures: Vec<SignatureKey> = self
            .function_signatures
            .values()
            .map(|sig| sig.to_signature_key())
            .collect();

        // Deduplicate
        signatures.sort_by(|a, b| {
            // Sort by param count, then result count, then types
            match a.params.len().cmp(&b.params.len()) {
                std::cmp::Ordering::Equal => match a.results.len().cmp(&b.results.len()) {
                    std::cmp::Ordering::Equal => {
                        // Compare types lexicographically
                        let a_str = format!("{:?}{:?}", a.params, a.results);
                        let b_str = format!("{:?}{:?}", b.params, b.results);
                        a_str.cmp(&b_str)
                    }
                    other => other,
                },
                other => other,
            }
        });
        signatures.dedup();
        signatures
    }

    /// Build a mapping from function names to their type indices.
    /// This should be called after all signatures have been registered.
    pub fn build_type_index_map(&mut self) -> HashMap<String, u32> {
        let mut result = HashMap::new();

        // Collect the data we need first to avoid borrow issues
        let signatures: Vec<(String, SignatureKey)> = self
            .function_signatures
            .iter()
            .map(|(name, sig)| (name.clone(), sig.to_signature_key()))
            .collect();

        for (name, key) in signatures {
            // Check if we already have this type
            let index = if let Some(&existing) = self.type_index_cache.get(&key) {
                existing
            } else {
                let new_index = self.next_type_index;
                self.type_index_cache.insert(key, new_index);
                self.next_type_index += 1;
                new_index
            };
            result.insert(name, index);
        }

        result
    }
}

/// Align a value to the specified alignment
fn align_to(value: u32, alignment: u32) -> u32 {
    (value + alignment - 1) & !(alignment - 1)
}
