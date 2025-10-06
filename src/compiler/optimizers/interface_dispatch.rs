use crate::compiler::compiler_errors::CompileError;
use crate::compiler::wir::wir_nodes::{InterfaceInfo, InterfaceDefinition, VTable};
use crate::return_compiler_error;
use std::collections::HashMap;
use wasm_encoder::{TableType, RefType, ConstExpr};

/// Interface dispatch system for dynamic method calls
///
/// This system handles vtable generation and function table management
/// for Beanstalk's interface system. It's moved to optimizers as it's
/// a complex feature that can be added after basic functionality works.
///
/// ## Design
/// - Interfaces define method signatures without implementation
/// - Types implement interfaces with concrete methods
/// - Dynamic dispatch uses vtables and call_indirect
/// - Function tables store method implementations
#[derive(Debug)]
pub struct InterfaceDispatchSystem {
    /// Method type mappings for call_indirect
    pub method_type_mappings: HashMap<(u32, u32), u32>, // (interface_id, method_id) -> type_index
    /// VTable offset calculations
    pub vtable_offsets: HashMap<(u32, u32), u32>, // (interface_id, type_id) -> offset
    /// Interface method mapping for efficient dispatch
    pub method_mapping: InterfaceMethodMapping,
}

/// Interface method mapping for efficient dispatch
#[derive(Debug, Clone)]
pub struct InterfaceMethodMapping {
    /// Maps (interface_id, method_id, type_id) to function_index
    pub implementations: HashMap<(u32, u32, u32), u32>,
}

impl InterfaceMethodMapping {
    pub fn new() -> Self {
        Self {
            implementations: HashMap::new(),
        }
    }

    pub fn add_method_implementation(
        &mut self,
        interface_id: u32,
        method_id: u32,
        type_id: u32,
        func_index: u32,
    ) {
        self.implementations
            .insert((interface_id, method_id, type_id), func_index);
    }
}

impl InterfaceDispatchSystem {
    pub fn new() -> Self {
        Self {
            method_type_mappings: HashMap::new(),
            vtable_offsets: HashMap::new(),
            method_mapping: InterfaceMethodMapping::new(),
        }
    }

    /// Generate type signatures for all interface methods
    pub fn generate_interface_method_types(
        &mut self,
        interface_info: &InterfaceInfo,
        type_count: &mut u32,
    ) -> Result<Vec<wasm_encoder::FuncType>, CompileError> {
        let mut func_types = Vec::new();

        for (_interface_id, interface_def) in &interface_info.interfaces {
            for method in &interface_def.methods {
                // Convert method signature to WASM function type
                let param_types = self.wasm_types_to_val_types(&method.param_types);
                let result_types = self.wasm_types_to_val_types(&method.return_types);

                let func_type = wasm_encoder::FuncType::new(param_types, result_types);
                func_types.push(func_type);

                // Store mapping for later lookup
                self.method_type_mappings
                    .insert((interface_def.id, method.id), *type_count);
                *type_count += 1;
            }
        }

        Ok(func_types)
    }

    /// Create WASM function table for interface dispatch
    pub fn create_interface_function_table(
        &self,
        interface_info: &InterfaceInfo,
    ) -> Result<(TableType, Vec<u32>), CompileError> {
        let table_type = TableType {
            element_type: RefType::FUNCREF,
            minimum: interface_info.function_table.len() as u64,
            maximum: Some(interface_info.function_table.len() as u64),
            table64: false,
            shared: false,
        };

        let func_indices: Vec<u32> = interface_info.function_table.clone();
        Ok((table_type, func_indices))
    }

    /// Generate vtable data structures in linear memory
    pub fn generate_vtable_data_structures(
        &mut self,
        interface_info: &InterfaceInfo,
    ) -> Result<Vec<(u32, Vec<u8>)>, CompileError> {
        let mut vtable_data = Vec::new();
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

            // Store vtable offset for later lookup
            self.vtable_offsets
                .insert((vtable.interface_id, vtable.type_id), current_vtable_offset);

            vtable_data.push((current_vtable_offset, vtable_bytes));
            current_vtable_offset += vtable_size as u32;
        }

        Ok(vtable_data)
    }

    /// Get interface method type index for call_indirect
    pub fn get_interface_method_type_index(
        &self,
        interface_id: u32,
        method_id: u32,
    ) -> Result<u32, CompileError> {
        self.method_type_mappings
            .get(&(interface_id, method_id))
            .copied()
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!(
                    "Method type index not found for interface {} method {}",
                    interface_id, method_id
                ))
            })
    }

    /// Calculate vtable offset for a given interface and implementing type
    pub fn calculate_vtable_offset(
        &self,
        interface_id: u32,
        type_id: u32,
    ) -> Result<u32, CompileError> {
        self.vtable_offsets
            .get(&(interface_id, type_id))
            .copied()
            .ok_or_else(|| {
                CompileError::new_thread_panic(format!(
                    "VTable offset not found for interface {} and type {}",
                    interface_id, type_id
                ))
            })
    }

    /// Generate interface method index mapping for efficient dispatch
    pub fn create_interface_method_mapping(
        &mut self,
        interface_info: &InterfaceInfo,
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

        self.method_mapping = mapping.clone();
        mapping
    }

    /// Convert WASM types to wasm_encoder ValType
    fn wasm_types_to_val_types(
        &self,
        wasm_types: &[crate::compiler::wir::place::WasmType],
    ) -> Vec<wasm_encoder::ValType> {
        wasm_types
            .iter()
            .map(|wt| match wt {
                crate::compiler::wir::place::WasmType::I32 => wasm_encoder::ValType::I32,
                crate::compiler::wir::place::WasmType::I64 => wasm_encoder::ValType::I64,
                crate::compiler::wir::place::WasmType::F32 => wasm_encoder::ValType::F32,
                crate::compiler::wir::place::WasmType::F64 => wasm_encoder::ValType::F64,
                crate::compiler::wir::place::WasmType::ExternRef => wasm_encoder::ValType::EXTERNREF,
                crate::compiler::wir::place::WasmType::FuncRef => wasm_encoder::ValType::FUNCREF,
            })
            .collect()
    }
}

impl Default for InterfaceDispatchSystem {
    fn default() -> Self {
        Self::new()
    }
}