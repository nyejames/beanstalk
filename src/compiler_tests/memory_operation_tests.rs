use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::mir::mir_nodes::{
    Constant, MIR, MirFunction, Operand, Rvalue, Statement, Terminator, MemoryOpKind,
    MirBlock, Events, FunctionSignature, TypeInfo, MemoryInfo, InterfaceInfo,
};
use crate::compiler::mir::place::{Place, WasmType, MemoryBase, ByteOffset, TypeSize};
use std::collections::HashMap;
use wasm_encoder::Function;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alloc_statement_lowering() {
        let mut wasm_module = WasmModule::new();
        
        // Create a simple MIR for initialization
        let mir = create_test_mir();
        let wasm_module = WasmModule::from_mir(&mir).expect("Failed to create WASM module from MIR");
        
        let place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let size_operand = Operand::Constant(Constant::I32(1024));
        let align = 8;
        
        let alloc_stmt = Statement::Alloc {
            place: place.clone(),
            size: size_operand,
            align,
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&alloc_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Alloc statement lowering should succeed");
    }

    #[test]
    fn test_dealloc_statement_lowering() {
        let wasm_module = WasmModule::new();
        
        let place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let dealloc_stmt = Statement::Dealloc { place };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&dealloc_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Dealloc statement lowering should succeed");
    }

    #[test]
    fn test_store_statement_with_alignment() {
        let wasm_module = WasmModule::new();
        
        let place = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(1024),
            size: TypeSize::Word,
        };
        
        let value = Operand::Constant(Constant::I32(42));
        let alignment = 4;
        let offset = 8;
        
        let store_stmt = Statement::Store {
            place,
            value,
            alignment,
            offset,
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&store_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Store statement lowering should succeed");
    }

    #[test]
    fn test_load_rvalue_with_alignment() {
        let wasm_module = WasmModule::new();
        
        let place = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(2048),
            size: TypeSize::Word,
        };
        
        let load_rvalue = Rvalue::Load {
            place,
            alignment: 4,
            offset: 16,
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_rvalue(&load_rvalue, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Load rvalue lowering should succeed");
    }

    #[test]
    fn test_memory_size_operation() {
        let wasm_module = WasmModule::new();
        
        let result_place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let memory_op = Statement::MemoryOp {
            op: MemoryOpKind::Size,
            operand: None,
            result: Some(result_place),
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&memory_op, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Memory size operation should succeed");
    }

    #[test]
    fn test_memory_grow_operation() {
        let wasm_module = WasmModule::new();
        
        let pages_operand = Operand::Constant(Constant::I32(1));
        let result_place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let memory_op = Statement::MemoryOp {
            op: MemoryOpKind::Grow,
            operand: Some(pages_operand),
            result: Some(result_place),
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&memory_op, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Memory grow operation should succeed");
    }

    #[test]
    fn test_memory_fill_operation() {
        let wasm_module = WasmModule::new();
        
        // For memory fill, we need dest, value, size as operand
        let fill_operand = Operand::Constant(Constant::I32(0)); // Simplified for test
        let result_place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let memory_op = Statement::MemoryOp {
            op: MemoryOpKind::Fill,
            operand: Some(fill_operand),
            result: Some(result_place),
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&memory_op, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Memory fill operation should succeed");
    }

    #[test]
    fn test_memory_copy_operation() {
        let wasm_module = WasmModule::new();
        
        // For memory copy, we need dest, src, size as operand
        let copy_operand = Operand::Constant(Constant::I32(0)); // Simplified for test
        let result_place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let memory_op = Statement::MemoryOp {
            op: MemoryOpKind::Copy,
            operand: Some(copy_operand),
            result: Some(result_place),
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        let result = wasm_module.lower_statement(&memory_op, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Memory copy operation should succeed");
    }

    #[test]
    fn test_alignment_validation() {
        let wasm_module = WasmModule::new();
        
        let place = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(1024),
            size: TypeSize::Word,
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        // Test with invalid alignment (not power of 2)
        let result = wasm_module.resolve_place_load_with_alignment(
            &place, 
            3, // Invalid alignment
            0, 
            &mut wasm_function, 
            &local_map
        );
        assert!(result.is_err(), "Invalid alignment should cause error");
        
        // Test with valid alignment
        let result = wasm_module.resolve_place_load_with_alignment(
            &place, 
            4, // Valid alignment
            0, 
            &mut wasm_function, 
            &local_map
        );
        assert!(result.is_ok(), "Valid alignment should succeed");
    }

    #[test]
    fn test_bounds_checking_integration() {
        let wasm_module = WasmModule::new();
        
        let place = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(1024),
            size: TypeSize::Word,
        };
        
        let mut wasm_function = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let local_map = HashMap::new();
        
        // Test that bounds checking is integrated into memory operations
        let result = wasm_module.resolve_place_load_with_alignment(
            &place, 
            4, 
            0, 
            &mut wasm_function, 
            &local_map
        );
        assert!(result.is_ok(), "Memory load with bounds checking should succeed");
    }

    /// Helper function to create a minimal MIR for testing
    fn create_test_mir() -> MIR {
        let mut mir = MIR::new();
        
        // Set up basic memory info
        mir.type_info.memory_info = MemoryInfo {
            initial_pages: 1,
            max_pages: Some(10),
            static_data_size: 1024,
        };
        
        // Add a simple function
        let function = MirFunction::new(
            0,
            "test_function".to_string(),
            vec![],
            vec![WasmType::I32],
        );
        
        mir.add_function(function);
        mir
    }
}