//! Tests for memory layout management functionality in WASM backend
//! 
//! This module tests the memory layout management methods added to WasmModule
//! for task 9 of the WASM backend implementation.

use crate::compiler::codegen::wasm_encoding::{WasmModule, MemoryLayoutManager, WASM_PAGE_SIZE};
use crate::compiler::wir::wir_nodes::{WIR, MemoryInfo, TypeInfo, InterfaceInfo};
use crate::compiler::wir::place::{Place, WasmType};
use std::collections::HashMap;

#[cfg(test)]
mod tests {
    use super::*;

    /// Test creating a memory layout manager from WIR
    #[test]
    fn test_create_memory_layout_manager() {
        let mut wasm_module = WasmModule::new();
        let wir = create_test_wir();
        
        let result = wasm_module.create_memory_layout_manager(&wir);
        assert!(result.is_ok(), "Should create memory layout manager successfully");
        
        let layout_manager = result.unwrap();
        // The layout manager may add some overhead for globals, so check it's at least the base size
        assert!(layout_manager.get_total_static_size() >= wir.type_info.memory_info.static_data_size);
    }

    /// Test struct field layout calculation
    #[test]
    fn test_calculate_struct_field_layout() {
        let wasm_module = WasmModule::new();
        let field_types = vec![WasmType::I32, WasmType::F64, WasmType::I32];
        
        let result = wasm_module.calculate_struct_field_layout(&field_types, 1);
        assert!(result.is_ok(), "Should calculate struct layout successfully");
        
        let layout = result.unwrap();
        assert_eq!(layout.struct_id, 1);
        assert_eq!(layout.fields.len(), 3);
        
        // Check field alignments
        assert_eq!(layout.fields[0].offset, 0);  // First i32 at offset 0
        assert_eq!(layout.fields[1].offset, 8);  // f64 aligned to 8 bytes
        assert_eq!(layout.fields[2].offset, 16); // Second i32 after f64
        
        // Total size should be aligned to largest alignment (8 bytes for f64)
        assert_eq!(layout.total_size, 24); // 20 bytes rounded up to 8-byte alignment
        assert_eq!(layout.alignment, 8);
    }

    /// Test memory section enhancement
    #[test]
    fn test_enhance_memory_section() {
        let mut wasm_module = WasmModule::new();
        let layout_manager = create_test_layout_manager();
        
        let result = wasm_module.enhance_memory_section(&layout_manager);
        assert!(result.is_ok(), "Should enhance memory section successfully");
    }

    /// Test static data section population
    #[test]
    fn test_populate_static_data_section() {
        let mut wasm_module = WasmModule::new();
        
        // Add some string constants
        wasm_module.add_string_constant_for_test("hello".to_string(), 0);
        wasm_module.add_string_constant_for_test("world".to_string(), 16);
        
        let layout_manager = create_test_layout_manager();
        
        let result = wasm_module.populate_static_data_section(&layout_manager);
        assert!(result.is_ok(), "Should populate static data section successfully");
    }

    /// Test heap allocation support setup
    #[test]
    fn test_setup_heap_allocation_support() {
        let mut wasm_module = WasmModule::new();
        let layout_manager = create_test_layout_manager();
        
        let result = wasm_module.setup_heap_allocation_support(&layout_manager);
        assert!(result.is_ok(), "Should setup heap allocation support successfully");
        
        let heap_allocator = result.unwrap();
        assert!(heap_allocator.start_offset > 0);
        assert!(heap_allocator.size > 0);
    }

    /// Test memory layout statistics
    #[test]
    fn test_get_memory_layout_stats() {
        let mut wasm_module = WasmModule::new();
        
        // Add some test data
        wasm_module.add_string_constant_for_test("test".to_string(), 0);
        
        let layout_manager = create_test_layout_manager();
        let stats = wasm_module.get_memory_layout_stats(&layout_manager);
        
        assert!(stats.total_static_size > 0);
        assert!(stats.string_constants_size > 0);
        assert!(stats.total_memory_pages > 0);
    }

    /// Test WASM type size and alignment functions
    #[test]
    fn test_wasm_type_properties() {
        let wasm_module = WasmModule::new();
        
        // Test i32
        assert_eq!(wasm_module.get_wasm_type_size(&WasmType::I32), 4);
        assert_eq!(wasm_module.get_wasm_type_alignment(&WasmType::I32), 4);
        
        // Test i64
        assert_eq!(wasm_module.get_wasm_type_size(&WasmType::I64), 8);
        assert_eq!(wasm_module.get_wasm_type_alignment(&WasmType::I64), 8);
        
        // Test f32
        assert_eq!(wasm_module.get_wasm_type_size(&WasmType::F32), 4);
        assert_eq!(wasm_module.get_wasm_type_alignment(&WasmType::F32), 4);
        
        // Test f64
        assert_eq!(wasm_module.get_wasm_type_size(&WasmType::F64), 8);
        assert_eq!(wasm_module.get_wasm_type_alignment(&WasmType::F64), 8);
        
        // Test pointer types
        assert_eq!(wasm_module.get_wasm_type_size(&WasmType::ExternRef), 4);
        assert_eq!(wasm_module.get_wasm_type_alignment(&WasmType::ExternRef), 4);
    }

    // Helper functions

    fn create_test_wir() -> WIR {
        let mut wir = WIR::new();
        wir.type_info = TypeInfo {
            function_types: Vec::new(),
            global_types: Vec::new(),
            memory_info: MemoryInfo {
                initial_pages: 1,
                max_pages: Some(16),
                static_data_size: 1024,
            },
            interface_info: InterfaceInfo {
                interfaces: HashMap::new(),
                vtables: HashMap::new(),
                function_table: Vec::new(),
            },
        };
        
        // Add some test globals
        wir.globals.insert(0, Place::Global { 
            index: 0, 
            wasm_type: WasmType::I32 
        });
        wir.globals.insert(1, Place::Global { 
            index: 1, 
            wasm_type: WasmType::F64 
        });
        
        wir
    }

    fn create_test_layout_manager() -> MemoryLayoutManager {
        let mut layout_manager = MemoryLayoutManager::new();
        let wir = create_test_wir();
        
        layout_manager.initialize_from_wir(&wir).unwrap();
        layout_manager.setup_heap_region().unwrap();
        
        layout_manager
    }
}