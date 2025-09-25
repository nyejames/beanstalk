use crate::compiler::mir::place::{Place, WasmType};

/// Tests for MIR place system functionality
/// Moved from src/compiler/mir/place.rs

#[cfg(test)]
mod mir_place_tests {
    use super::*;

    #[test]
    fn test_place_creation() {
        let local_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        assert_eq!(local_place.wasm_type(), WasmType::I32);
        
        let global_place = Place::Global { index: 1, wasm_type: WasmType::F64 };
        assert_eq!(global_place.wasm_type(), WasmType::F64);
    }

    #[test]
    fn test_wasm_type_size() {
        assert_eq!(WasmType::I32.byte_size(), 4);
        assert_eq!(WasmType::I64.byte_size(), 8);
        assert_eq!(WasmType::F32.byte_size(), 4);
        assert_eq!(WasmType::F64.byte_size(), 8);
    }

    #[test]
    fn test_place_equality() {
        let place1 = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place2 = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place3 = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        assert_eq!(place1, place2);
        assert_ne!(place1, place3);
    }

    #[test]
    fn test_place_debug_format() {
        let place = Place::Local { index: 42, wasm_type: WasmType::I64 };
        let debug_str = format!("{:?}", place);
        
        assert!(debug_str.contains("Local"));
        assert!(debug_str.contains("42"));
        assert!(debug_str.contains("I64"));
    }
}