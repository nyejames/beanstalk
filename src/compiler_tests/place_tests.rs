use crate::compiler::datatypes::DataType;
use crate::compiler::wir::place::{
    ArithmeticOp, ByteOffset, FieldOffset, FieldSize, MemoryBase, MemoryLayout, Place,
    PlaceManager, ProjectionElem, StackOpType, TypeSize, WasmType,
};

#[cfg(test)]
mod place_creation_tests {
    use super::*;

    #[test]
    fn test_local_place_creation() {
        let data_type = DataType::Int;
        let place = Place::local(0, &data_type);

        match &place {
            Place::Local { index, wasm_type } => {
                assert_eq!(*index, 0);
                assert_eq!(*wasm_type, WasmType::I64);
            }
            _ => assert!(false, "Expected Local place, got {:?}", place),
        }

        assert!(place.is_wasm_local());
        assert!(!place.is_wasm_global());
        assert!(!place.requires_memory_access());
    }

    #[test]
    fn test_global_place_creation() {
        let data_type = DataType::Float;
        let place = Place::global(5, &data_type);

        match &place {
            Place::Global { index, wasm_type } => {
                assert_eq!(*index, 5);
                assert_eq!(*wasm_type, WasmType::F64);
            }
            _ => assert!(false, "Expected Global place, got {:?}", place),
        }

        assert!(!place.is_wasm_local());
        assert!(place.is_wasm_global());
        assert!(!place.requires_memory_access());
    }

    #[test]
    fn test_memory_place_creation() {
        let place = Place::memory(1024, TypeSize::Word);

        match &place {
            Place::Memory { base, offset, size } => {
                assert_eq!(*base, MemoryBase::LinearMemory);
                assert_eq!(*offset, ByteOffset(1024));
                assert_eq!(*size, TypeSize::Word);
            }
            _ => assert!(false, "Expected Memory place, got {:?}", place),
        }

        assert!(!place.is_wasm_local());
        assert!(!place.is_wasm_global());
        assert!(place.requires_memory_access());
    }

    #[test]
    fn test_heap_allocation_place() {
        let place = Place::heap_alloc(
            1,
            256,
            2048,
            TypeSize::Custom {
                bytes: 256,
                alignment: 8,
            },
        );

        match place {
            Place::Memory { base, offset, size } => {
                match base {
                    MemoryBase::Heap { alloc_id, size } => {
                        assert_eq!(alloc_id, 1);
                        assert_eq!(size, 256);
                    }
                    _ => assert!(false, "Expected Heap memory base, got {:?}", base),
                }
                assert_eq!(offset, ByteOffset(2048));
                match size {
                    TypeSize::Custom { bytes, alignment } => {
                        assert_eq!(bytes, 256);
                        assert_eq!(alignment, 8);
                    }
                    _ => panic!("Expected Custom type size"),
                }
            }
            _ => assert!(false, "Expected Memory place, got {:?}", place),
        }
    }
}

#[cfg(test)]
mod place_projection_tests {
    use super::*;

    #[test]
    fn test_field_projection() {
        let base = Place::local(
            0,
            &DataType::Int,
        );
        let projected = base.project_field(1, 8, FieldSize::WasmType(WasmType::F32));

        match projected {
            Place::Projection { base, elem } => {
                assert!(base.is_wasm_local());
                match elem {
                    ProjectionElem::Field {
                        index,
                        offset,
                        size,
                    } => {
                        assert_eq!(index, 1);
                        assert_eq!(offset, FieldOffset(8));
                        match size {
                            FieldSize::WasmType(wasm_type) => {
                                assert_eq!(wasm_type, WasmType::F32);
                            }
                            _ => panic!("Expected WasmType field size"),
                        }
                    }
                    _ => panic!("Expected Field projection"),
                }
            }
            _ => panic!("Expected Projection place"),
        }
    }

    #[test]
    fn test_index_projection() {
        let base = Place::memory(1024, TypeSize::Word);
        let index = Place::local(
            1,
            &DataType::Int,
        );
        let projected = base.project_index(index, 4);

        match projected {
            Place::Projection { base, elem } => {
                assert!(base.requires_memory_access());
                match elem {
                    ProjectionElem::Index {
                        index,
                        element_size,
                    } => {
                        assert!(index.is_wasm_local());
                        assert_eq!(element_size, 4);
                    }
                    _ => panic!("Expected Index projection"),
                }
            }
            _ => panic!("Expected Projection place"),
        }
    }

    #[test]
    fn test_nested_projections() {
        let base = Place::local(
            0,
            &DataType::Int,
        );
        let field_proj = base.project_field(0, 16, FieldSize::Fixed(32));
        let index_proj = field_proj.project_index(
            Place::local(
                1,
                &DataType::Int,
            ),
            8,
        );

        // Should be able to chain projections
        assert_eq!(index_proj.load_instruction_count(), 6); // base (1) + field (2) + index (3) = 6
    }
}

#[cfg(test)]
mod wasm_type_tests {
    use super::*;

    #[test]
    fn test_wasm_type_from_data_type() {
        assert_eq!(
            WasmType::from_data_type(&DataType::Int),
            WasmType::I64
        );
        assert_eq!(
            WasmType::from_data_type(&DataType::Float),
            WasmType::F64
        );
        assert_eq!(
            WasmType::from_data_type(&DataType::Bool),
            WasmType::I32
        );
        assert_eq!(
            WasmType::from_data_type(&DataType::String),
            WasmType::I32 // Pointer to linear memory
        );
    }

    #[test]
    fn test_wasm_type_byte_sizes() {
        assert_eq!(WasmType::I32.byte_size(), 4);
        assert_eq!(WasmType::I64.byte_size(), 8);
        assert_eq!(WasmType::F32.byte_size(), 4);
        assert_eq!(WasmType::F64.byte_size(), 8);
        assert_eq!(WasmType::ExternRef.byte_size(), 4);
        assert_eq!(WasmType::FuncRef.byte_size(), 4);
    }

    #[test]
    fn test_wasm_type_compatibility() {
        assert!(WasmType::I32.is_local_compatible());
        assert!(WasmType::I64.is_local_compatible());
        assert!(WasmType::F32.is_local_compatible());
        assert!(WasmType::F64.is_local_compatible());
        assert!(WasmType::FuncRef.is_local_compatible());

        // ExternRef requires memory for complex types
        assert!(WasmType::ExternRef.requires_memory());
        assert!(!WasmType::I32.requires_memory());
    }
}

#[cfg(test)]
mod instruction_count_tests {
    use super::*;

    #[test]
    fn test_local_instruction_counts() {
        let place = Place::local(
            0,
            &DataType::Int,
        );

        assert_eq!(place.load_instruction_count(), 1); // local.get
        assert_eq!(place.store_instruction_count(), 1); // local.set
    }

    #[test]
    fn test_global_instruction_counts() {
        let place = Place::global(
            0,
            &DataType::Float,
        );

        assert_eq!(place.load_instruction_count(), 1); // global.get
        assert_eq!(place.store_instruction_count(), 1); // global.set
    }

    #[test]
    fn test_memory_instruction_counts() {
        let place = Place::memory(1024, TypeSize::Word);

        assert_eq!(place.load_instruction_count(), 2); // i32.const + memory.load
        assert_eq!(place.store_instruction_count(), 2); // i32.const + memory.store
    }

    #[test]
    fn test_projection_instruction_counts() {
        let base = Place::local(
            0,
            &DataType::Int,
        );
        let projected = base.project_field(0, 8, FieldSize::WasmType(WasmType::I32));

        // base load + field offset + add
        assert_eq!(projected.load_instruction_count(), 3);
        // base load + field offset + add + store
        assert_eq!(projected.store_instruction_count(), 4);
    }

    #[test]
    fn test_complex_projection_instruction_counts() {
        let base = Place::memory(1024, TypeSize::Word);
        let index = Place::local(
            1,
            &DataType::Int,
        );
        let projected = base.project_index(index, 4);

        // base (2) + index load (1) + mul (1) + add (1) = 5
        assert_eq!(projected.load_instruction_count(), 5);
    }
}

#[cfg(test)]
mod stack_operation_tests {
    use super::*;

    #[test]
    fn test_local_load_operations() {
        let place = Place::local(
            0,
            &DataType::Int,
        );
        let ops = place.generate_load_operations();

        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].op_type, StackOpType::Load);
        assert_eq!(ops[0].wasm_type, WasmType::I64);
        assert_eq!(ops[0].stack_delta, 1);
    }

    #[test]
    fn test_local_store_operations() {
        let place = Place::local(
            0,
            &DataType::Float,
        );
        let ops = place.generate_store_operations();

        assert_eq!(ops.len(), 1);
        assert_eq!(ops[0].op_type, StackOpType::Store);
        assert_eq!(ops[0].wasm_type, WasmType::F64);
        assert_eq!(ops[0].stack_delta, -1);
    }

    #[test]
    fn test_memory_load_operations() {
        let place = Place::memory(1024, TypeSize::Word);
        let ops = place.generate_load_operations();

        assert_eq!(ops.len(), 2);
        // First: load address
        assert_eq!(ops[0].op_type, StackOpType::Load);
        assert_eq!(ops[0].wasm_type, WasmType::I32);
        assert_eq!(ops[0].stack_delta, 1);
        // Second: load from memory
        assert_eq!(ops[1].op_type, StackOpType::Load);
        assert_eq!(ops[1].wasm_type, WasmType::I32);
        assert_eq!(ops[1].stack_delta, 0); // Replace address with value
    }

    #[test]
    fn test_projection_operations() {
        let base = Place::local(
            0,
            &DataType::Int,
        );
        let projected = base.project_field(0, 8, FieldSize::WasmType(WasmType::I32));
        let ops = projected.generate_load_operations();

        assert_eq!(ops.len(), 3);
        // Base load
        assert_eq!(ops[0].op_type, StackOpType::Load);
        assert_eq!(ops[0].stack_delta, 1);
        // Offset constant
        assert_eq!(ops[1].op_type, StackOpType::Load);
        assert_eq!(ops[1].stack_delta, 1);
        // Add operation
        assert_eq!(ops[2].op_type, StackOpType::Arithmetic(ArithmeticOp::Add));
        assert_eq!(ops[2].stack_delta, -1);
    }
}

#[cfg(test)]
mod place_manager_tests {
    use super::*;

    #[test]
    fn test_place_manager_creation() {
        let manager = PlaceManager::new();
        assert_eq!(manager.get_local_types().len(), 0);
        assert_eq!(manager.get_global_types().len(), 0);
    }

    #[test]
    fn test_local_allocation() {
        let mut manager = PlaceManager::new();

        let place1 = manager.allocate_local(&DataType::Int);
        let place2 = manager.allocate_local(&DataType::Float);

        assert!(place1.is_wasm_local());
        assert!(place2.is_wasm_local());

        let local_types = manager.get_local_types();
        assert_eq!(local_types.len(), 2);
        assert_eq!(local_types[0], WasmType::I64);
        assert_eq!(local_types[1], WasmType::F64);
    }

    #[test]
    fn test_global_allocation() {
        let mut manager = PlaceManager::new();

        let place1 = manager.allocate_global(&DataType::Bool);
        let place2 = manager.allocate_global(&DataType::String);

        assert!(place1.is_wasm_global());
        assert!(place2.is_wasm_global());

        let global_types = manager.get_global_types();
        assert_eq!(global_types.len(), 2);
        assert_eq!(global_types[0], WasmType::I32);
        assert_eq!(global_types[1], WasmType::I32); // String is pointer
    }

    #[test]
    fn test_memory_allocation() {
        let mut manager = PlaceManager::new();

        let place1 = manager.allocate_memory(16, 4);
        let place2 = manager.allocate_memory(32, 8);

        assert!(place1.requires_memory_access());
        assert!(place2.requires_memory_access());

        let layout = manager.get_memory_layout();
        assert_eq!(layout.total_size(), 48); // 16 + 32 aligned
        assert_eq!(layout.get_regions().len(), 2);
    }

    #[test]
    fn test_heap_allocation() {
        let mut manager = PlaceManager::new();

        let data_type =
            DataType::String;
        let place = manager.allocate_heap(&data_type, 64);

        assert!(place.requires_memory_access());

        match place {
            Place::Memory { base, .. } => match base {
                MemoryBase::Heap { alloc_id, size } => {
                    assert_eq!(alloc_id, 0);
                    assert_eq!(size, 64);

                    let allocation = manager.get_heap_allocation(alloc_id);
                    assert!(allocation.is_some());
                    assert_eq!(allocation.unwrap().size, 64);
                }
                _ => panic!("Expected heap allocation"),
            },
            _ => panic!("Expected memory place"),
        }
    }
}

#[cfg(test)]
mod memory_layout_tests {
    use super::*;

    #[test]
    fn test_memory_layout_creation() {
        let layout = MemoryLayout::new();
        assert_eq!(layout.total_size(), 0);
        assert_eq!(layout.get_regions().len(), 0);
    }

    #[test]
    fn test_memory_allocation() {
        let mut layout = MemoryLayout::new();

        let offset1 = layout.allocate(16, 4);
        let offset2 = layout.allocate(24, 8);

        assert_eq!(offset1, 0);
        assert_eq!(offset2, 16); // No padding needed
        assert_eq!(layout.total_size(), 40);

        let regions = layout.get_regions();
        assert_eq!(regions.len(), 2);
        assert_eq!(regions[0].start, 0);
        assert_eq!(regions[0].size, 16);
        assert_eq!(regions[1].start, 16);
        assert_eq!(regions[1].size, 24);
    }

    #[test]
    fn test_memory_alignment() {
        let mut layout = MemoryLayout::new();

        // Allocate 1 byte with 1-byte alignment
        let offset1 = layout.allocate(1, 1);
        // Allocate 8 bytes with 8-byte alignment (should be aligned)
        let offset2 = layout.allocate(8, 8);

        assert_eq!(offset1, 0);
        assert_eq!(offset2, 8); // Aligned to 8-byte boundary
        assert_eq!(layout.total_size(), 16);
    }
}

#[cfg(test)]
mod type_size_tests {
    use super::*;

    #[test]
    fn test_type_size_to_wasm_type() {
        assert_eq!(TypeSize::Byte.to_wasm_type(), WasmType::I32);
        assert_eq!(TypeSize::Short.to_wasm_type(), WasmType::I32);
        assert_eq!(TypeSize::Word.to_wasm_type(), WasmType::I32);
        assert_eq!(TypeSize::DoubleWord.to_wasm_type(), WasmType::I64);

        assert_eq!(
            TypeSize::Custom {
                bytes: 2,
                alignment: 2
            }
            .to_wasm_type(),
            WasmType::I32
        );
        assert_eq!(
            TypeSize::Custom {
                bytes: 8,
                alignment: 8
            }
            .to_wasm_type(),
            WasmType::I64
        );
    }

    #[test]
    fn test_type_size_byte_sizes() {
        assert_eq!(TypeSize::Byte.byte_size(), 1);
        assert_eq!(TypeSize::Short.byte_size(), 2);
        assert_eq!(TypeSize::Word.byte_size(), 4);
        assert_eq!(TypeSize::DoubleWord.byte_size(), 8);
        assert_eq!(
            TypeSize::Custom {
                bytes: 12,
                alignment: 4
            }
            .byte_size(),
            12
        );
    }

    #[test]
    fn test_type_size_alignment() {
        assert_eq!(TypeSize::Byte.alignment(), 1);
        assert_eq!(TypeSize::Short.alignment(), 2);
        assert_eq!(TypeSize::Word.alignment(), 4);
        assert_eq!(TypeSize::DoubleWord.alignment(), 8);
        assert_eq!(
            TypeSize::Custom {
                bytes: 12,
                alignment: 16
            }
            .alignment(),
            16
        );
    }
}

#[cfg(test)]
mod projection_elem_tests {
    use super::*;

    #[test]
    fn test_field_projection_wasm_type() {
        let field = ProjectionElem::Field {
            index: 0,
            offset: FieldOffset(8),
            size: FieldSize::WasmType(WasmType::F64),
        };

        assert_eq!(field.wasm_type(), WasmType::F64);
        assert_eq!(field.instruction_count(), 2);
    }

    #[test]
    fn test_index_projection_instruction_count() {
        let index_place = Place::local(
            1,
            &DataType::Int,
        );
        let index = ProjectionElem::Index {
            index: Box::new(index_place),
            element_size: 8,
        };

        assert_eq!(index.wasm_type(), WasmType::I32);
        assert_eq!(index.instruction_count(), 3); // load index + multiply + add
    }

    #[test]
    fn test_length_and_data_projections() {
        let length = ProjectionElem::Length;
        let data = ProjectionElem::Data;

        assert_eq!(length.wasm_type(), WasmType::I32);
        assert_eq!(data.wasm_type(), WasmType::I32);
        assert_eq!(length.instruction_count(), 1);
        assert_eq!(data.instruction_count(), 1);
    }

    #[test]
    fn test_deref_projection() {
        let deref = ProjectionElem::Deref;

        assert_eq!(deref.wasm_type(), WasmType::I32);
        assert_eq!(deref.instruction_count(), 1);
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_complex_place_operations() {
        let mut manager = PlaceManager::new();

        // Create a struct in heap memory
        let struct_place = manager.allocate_heap(
            &DataType::String,
            64,
        );

        // Project to a field within the struct
        let field_place = struct_place.project_field(1, 16, FieldSize::WasmType(WasmType::I32));

        // Project to an array element within that field
        let index_place = manager.allocate_local(&DataType::Int);
        let element_place = field_place.project_index(index_place, 4);

        // Verify the complex place works correctly
        assert!(element_place.requires_memory_access());
        assert!(element_place.load_instruction_count() > 5); // Complex addressing

        let ops = element_place.generate_load_operations();
        assert!(ops.len() > 5); // Multiple operations for complex addressing

        // Verify stack operations are balanced
        let total_delta: i32 = ops.iter().map(|op| op.stack_delta).sum();
        assert_eq!(total_delta, 1); // Should push one value onto stack
    }

    #[test]
    fn test_wasm_lowering_validation() {
        let mut manager = PlaceManager::new();

        // Test that all place types can be lowered to ≤3 WASM instructions
        let local = manager.allocate_local(&DataType::Int);
        let global = manager.allocate_global(&DataType::Float);
        let memory = manager.allocate_memory(32, 4);

        assert!(local.load_instruction_count() <= 3);
        assert!(local.store_instruction_count() <= 3);
        assert!(global.load_instruction_count() <= 3);
        assert!(global.store_instruction_count() <= 3);
        assert!(memory.load_instruction_count() <= 3);
        assert!(memory.store_instruction_count() <= 3);

        // Even simple projections should be reasonable
        let projected = local.project_field(0, 4, FieldSize::WasmType(WasmType::I32));
        assert!(projected.load_instruction_count() <= 5); // Slightly more for projections
    }
}
