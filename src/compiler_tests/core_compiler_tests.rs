use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::datatypes::{DataType, Ownership};

/// Core compiler functionality tests focusing on the essential compilation pipeline
/// These tests validate the basic compiler operations without getting into complex
/// implementation details.
#[cfg(test)]
mod place_system_tests {
    use super::*;

    #[test]
    fn test_local_place_creation() {
        let data_type = DataType::Int(Ownership::ImmutableOwned(false));
        let place = Place::local(0, &data_type);
        
        assert!(place.is_wasm_local(), "Place should be a WASM local");
        assert_eq!(place.load_instruction_count(), 1, "Local load should be 1 instruction");
        assert_eq!(place.store_instruction_count(), 1, "Local store should be 1 instruction");
    }

    #[test]
    fn test_memory_place_creation() {
        let place = Place::memory(1024, crate::compiler::wir::place::TypeSize::Word);
        
        assert!(place.requires_memory_access(), "Memory place should require memory access");
        assert_eq!(place.load_instruction_count(), 2, "Memory load should be 2 instructions");
        assert_eq!(place.store_instruction_count(), 2, "Memory store should be 2 instructions");
    }

    #[test]
    fn test_field_projection() {
        let data_type = DataType::String(Ownership::ImmutableOwned(false));
        let base = Place::local(0, &data_type);
        let field = base.project_field(0, 8, crate::compiler::wir::place::FieldSize::WasmType(WasmType::I32));
        
        assert_eq!(field.load_instruction_count(), 3, "Field projection should be 3 instructions");
        assert!(field.load_instruction_count() <= 5, "Field projection should be WASM-efficient");
    }

    #[test]
    fn test_wasm_type_mapping() {
        assert_eq!(
            WasmType::from_data_type(&DataType::Int(Ownership::ImmutableOwned(false))),
            WasmType::I64
        );
        assert_eq!(
            WasmType::from_data_type(&DataType::Float(Ownership::ImmutableOwned(false))),
            WasmType::F64
        );
        assert_eq!(
            WasmType::from_data_type(&DataType::Bool(Ownership::ImmutableOwned(false))),
            WasmType::I32
        );
    }

    #[test]
    fn test_wasm_instruction_efficiency() {
        // Test that places generate efficient WASM instruction sequences
        let local_place = Place::local(0, &DataType::Int(Ownership::ImmutableOwned(false)));
        let memory_place = Place::memory(1024, crate::compiler::wir::place::TypeSize::Word);
        
        // Local operations should be very efficient
        assert_eq!(local_place.load_instruction_count(), 1);
        assert_eq!(local_place.store_instruction_count(), 1);
        
        // Memory operations should still be reasonable
        assert!(memory_place.load_instruction_count() <= 3);
        assert!(memory_place.store_instruction_count() <= 3);
        
        // Field projections should be WASM-efficient
        let field_place = local_place.project_field(
            0, 
            8, 
            crate::compiler::wir::place::FieldSize::WasmType(WasmType::I32)
        );
        assert!(field_place.load_instruction_count() <= 5);
    }

    #[test]
    fn test_stack_operation_balance() {
        // Test that stack operations are balanced (push/pop correctly)
        let places = vec![
            Place::local(0, &DataType::Int(Ownership::ImmutableOwned(false))),
            Place::memory(1024, crate::compiler::wir::place::TypeSize::Word),
            Place::local(0, &DataType::Int(Ownership::ImmutableOwned(false)))
                .project_field(0, 4, crate::compiler::wir::place::FieldSize::WasmType(WasmType::I32)),
        ];
        
        for place in places {
            let load_ops = place.generate_load_operations();
            let store_ops = place.generate_store_operations();
            
            // Load operations should net +1 on stack (push value)
            let load_delta: i32 = load_ops.iter().map(|op| op.stack_delta).sum();
            assert_eq!(load_delta, 1, "Load operations should net +1 on stack");
            
            // Store operations should net -1 on stack (consume value)
            let store_delta: i32 = store_ops.iter().map(|op| op.stack_delta).sum();
            assert_eq!(store_delta, -1, "Store operations should net -1 on stack");
        }
    }
}