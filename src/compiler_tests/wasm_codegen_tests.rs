#[cfg(test)]
mod wasm_codegen_tests {
    use crate::compiler::codegen::wasm_encoding::WasmModule;
    use crate::compiler::codegen::build_wasm::new_wasm_module;
    use crate::compiler::mir::mir_nodes::{
        MIR, MirBlock, MirFunction, Statement, Terminator, Rvalue, Operand, Constant, BinOp,
        UnOp, Export, ExportKind, MemoryInfo, InterfaceInfo,
        InterfaceDefinition, MethodSignature, WasmIfInfo, BrTableInfo,
        WasmLoopInfo, LoopType,
    };
    use crate::compiler::mir::place::{Place, WasmType, ProjectionElem, FieldOffset, FieldSize, TypeSize, MemoryBase, ByteOffset};
    use std::collections::HashMap;
    use wasm_encoder::Function;

    use crate::compiler::mir::mir_nodes::VTable;

    // ===== STATEMENT LOWERING TESTS =====
    // Test all MIR statement types for proper WASM instruction generation

    #[test]
    fn test_statement_lowering_assign_constant() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        // Test: place = constant
        let target_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let assign_stmt = Statement::Assign {
            place: target_place,
            rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
        };

        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign constant statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_assign_binary_op() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        // Set up local mapping
        let place_a = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_b = Place::Local { index: 1, wasm_type: WasmType::I32 };
        let place_result = Place::Local { index: 2, wasm_type: WasmType::I32 };
        local_map.insert(place_a.clone(), 0);
        local_map.insert(place_b.clone(), 1);
        local_map.insert(place_result.clone(), 2);

        // Test: result = a + b
        let assign_stmt = Statement::Assign {
            place: place_result,
            rvalue: Rvalue::BinaryOp {
                op: BinOp::Add,
                left: Operand::Copy(place_a),
                right: Operand::Copy(place_b),
            },
        };

        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign binary operation should lower successfully");
    }

    #[test]
    fn test_statement_lowering_assign_unary_op() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let place_input = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_result = Place::Local { index: 1, wasm_type: WasmType::I32 };
        local_map.insert(place_input.clone(), 0);
        local_map.insert(place_result.clone(), 1);

        // Test: result = -input
        let assign_stmt = Statement::Assign {
            place: place_result,
            rvalue: Rvalue::UnaryOp {
                op: UnOp::Neg,
                operand: Operand::Copy(place_input),
            },
        };

        let result = wasm_module.lower_statement(&assign_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Assign unary operation should lower successfully");
    }

    #[test]
    fn test_statement_lowering_call() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let result_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(result_place.clone(), 0);

        // Test: result = call func(arg1, arg2)
        let call_stmt = Statement::Call {
            func: Operand::Constant(Constant::Function(1)),
            args: vec![
                Operand::Constant(Constant::I32(10)),
                Operand::Constant(Constant::I32(20)),
            ],
            destination: Some(result_place),
        };

        let result = wasm_module.lower_statement(&call_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Call statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_interface_call() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let receiver_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        let result_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        local_map.insert(receiver_place.clone(), 0);
        local_map.insert(result_place.clone(), 1);

        // Create interface info for testing
        let mut interface_info = InterfaceInfo {
            interfaces: HashMap::new(),
            vtables: HashMap::new(),
            function_table: vec![0, 1, 2],
        };

        let interface_def = InterfaceDefinition {
            id: 0,
            name: "TestInterface".to_string(),
            methods: vec![MethodSignature {
                id: 0,
                name: "test_method".to_string(),
                param_types: vec![WasmType::ExternRef, WasmType::I32],
                return_types: vec![WasmType::I32],
            }],
        };
        interface_info.interfaces.insert(0, interface_def);

        // Test: result = interface_call receiver.method(arg)
        let _interface_call_stmt = Statement::InterfaceCall {
            interface_id: 0,
            method_id: 0,
            receiver: Operand::Copy(receiver_place.clone()),
            args: vec![Operand::Constant(Constant::I32(42))],
            destination: Some(result_place),
        };

        let result = wasm_module.lower_interface_call(
            0, // interface_id
            0, // method_id
            &Operand::Copy(receiver_place),
            &[Operand::Constant(Constant::I32(42))],
            &Some(Place::Local { index: 1, wasm_type: WasmType::I32 }),
            &mut wasm_function,
            &local_map,
            &interface_info,
        );
        assert!(result.is_ok(), "Interface call statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_drop() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let target_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        local_map.insert(target_place.clone(), 0);

        // Test: drop place
        let drop_stmt = Statement::Drop { place: target_place };

        let result = wasm_module.lower_statement(&drop_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Drop statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_nop() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        // Test: nop (no operation)
        let nop_stmt = Statement::Nop;

        let result = wasm_module.lower_statement(&nop_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Nop statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_alloc() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let target_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        local_map.insert(target_place.clone(), 0);

        // Test: place = alloc(size, align)
        let alloc_stmt = Statement::Alloc {
            place: target_place,
            size: Operand::Constant(Constant::I32(64)),
            align: 8,
        };

        let result = wasm_module.lower_statement(&alloc_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Alloc statement should lower successfully");
    }

    #[test]
    fn test_statement_lowering_dealloc() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let target_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        local_map.insert(target_place.clone(), 0);

        // Test: dealloc place
        let dealloc_stmt = Statement::Dealloc {
            place: target_place,
        };

        let result = wasm_module.lower_statement(&dealloc_stmt, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Dealloc statement should lower successfully");
    }

    // ===== PLACE RESOLUTION TESTS =====
    // Test all place projection types for proper WASM memory access

    #[test]
    fn test_place_resolution_local() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let local_place = Place::Local { index: 5, wasm_type: WasmType::I32 };
        local_map.insert(local_place.clone(), 5);

        // Test loading from local
        let result = wasm_module.resolve_place_load(&local_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Local place load should resolve successfully");

        // Test storing to local
        let result = wasm_module.resolve_place_store(&local_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Local place store should resolve successfully");
    }

    #[test]
    fn test_place_resolution_global() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let global_place = Place::Global { index: 2, wasm_type: WasmType::F64 };

        // Test loading from global
        let result = wasm_module.resolve_place_load(&global_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Global place load should resolve successfully");

        // Test storing to global
        let result = wasm_module.resolve_place_store(&global_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Global place store should resolve successfully");
    }

    #[test]
    fn test_place_resolution_memory() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let memory_place = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(16),
            size: TypeSize::Word,
        };

        // Test loading from memory
        let result = wasm_module.resolve_place_load(&memory_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Memory place load should resolve successfully");

        // Test storing to memory
        let result = wasm_module.resolve_place_store(&memory_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Memory place store should resolve successfully");
    }

    #[test]
    fn test_place_resolution_field_projection() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let base_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        local_map.insert(base_place.clone(), 0);

        let field_place = Place::Projection {
            base: Box::new(base_place),
            elem: ProjectionElem::Field {
                index: 0,
                offset: FieldOffset(8),
                size: FieldSize::WasmType(WasmType::I32),
            },
        };

        // Test loading from field
        let result = wasm_module.resolve_place_load(&field_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Field projection load should resolve successfully");

        // Test storing to field
        let result = wasm_module.resolve_place_store(&field_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Field projection store should resolve successfully");
    }

    #[test]
    fn test_place_resolution_index_projection() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let base_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        let index_place = Place::Local { index: 1, wasm_type: WasmType::I32 };
        local_map.insert(base_place.clone(), 0);
        local_map.insert(index_place.clone(), 1);

        let indexed_place = Place::Projection {
            base: Box::new(base_place),
            elem: ProjectionElem::Index {
                index: Box::new(index_place),
                element_size: 4, // 4 bytes for Word
            },
        };

        // Test loading from indexed element
        let result = wasm_module.resolve_place_load(&indexed_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Index projection load should resolve successfully");

        // Test storing to indexed element
        let result = wasm_module.resolve_place_store(&indexed_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Index projection store should resolve successfully");
    }

    #[test]
    fn test_place_resolution_deref_projection() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let pointer_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        local_map.insert(pointer_place.clone(), 0);

        let deref_place = Place::Projection {
            base: Box::new(pointer_place),
            elem: ProjectionElem::Deref,
        };

        // Test loading from dereferenced pointer
        let result = wasm_module.resolve_place_load(&deref_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Deref projection load should resolve successfully");

        // Test storing to dereferenced pointer
        let result = wasm_module.resolve_place_store(&deref_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Deref projection store should resolve successfully");
    }

    #[test]
    fn test_place_resolution_nested_projections() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let base_place = Place::Local { index: 0, wasm_type: WasmType::ExternRef };
        local_map.insert(base_place.clone(), 0);

        // Test nested projections: base.field[index].deref
        let nested_place = Place::Projection {
            base: Box::new(Place::Projection {
                base: Box::new(Place::Projection {
                    base: Box::new(base_place),
                    elem: ProjectionElem::Field {
                        index: 0,
                        offset: FieldOffset(16),
                        size: FieldSize::WasmType(WasmType::ExternRef),
                    },
                }),
                elem: ProjectionElem::Index {
                    index: Box::new(Place::Local { index: 1, wasm_type: WasmType::I32 }),
                    element_size: 4, // 4 bytes for pointer
                },
            }),
            elem: ProjectionElem::Deref,
        };

        // Test complex nested projection
        let result = wasm_module.resolve_place_load(&nested_place, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Nested projections should resolve successfully");
    }

    // ===== CONTROL FLOW TESTS =====
    // Test all terminator types for proper WASM control flow generation

    #[test]
    fn test_terminator_lowering_goto() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let goto_terminator = Terminator::Goto {
            target: 1,
            label_depth: 0,
        };

        let result = wasm_module.lower_terminator(&goto_terminator, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Goto terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_if() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let condition_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(condition_place.clone(), 0);

        let if_terminator = Terminator::If {
            condition: Operand::Copy(condition_place),
            then_block: 1,
            else_block: 2,
            wasm_if_info: WasmIfInfo {
                has_else: true,
                result_type: None,
                nesting_level: 0,
            },
        };

        let result = wasm_module.lower_terminator(&if_terminator, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "If terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_switch() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let discriminant_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(discriminant_place.clone(), 0);

        let switch_terminator = Terminator::Switch {
            discriminant: Operand::Copy(discriminant_place),
            targets: vec![1, 2, 3],
            default: 4,
            br_table_info: BrTableInfo {
                is_dense: true,
                default_index: 4,
                target_count: 3,
                min_target: 1,
                max_target: 3,
            },
        };

        let result = wasm_module.lower_terminator(&switch_terminator, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Switch terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_loop() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let condition_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(condition_place.clone(), 0);

        let loop_terminator = Terminator::Loop {
            target: 1,
            loop_header: 0,
            loop_info: WasmLoopInfo {
                header_block: 0,
                is_infinite: false,
                nesting_level: 0,
                loop_type: LoopType::While,
                has_breaks: true,
                has_continues: false,
                result_type: None,
            },
        };

        let result = wasm_module.lower_terminator(&loop_terminator, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Loop terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_return() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let mut local_map = HashMap::new();

        let return_place = Place::Local { index: 0, wasm_type: WasmType::I32 };
        local_map.insert(return_place.clone(), 0);

        let return_terminator = Terminator::Return {
            values: vec![Operand::Copy(return_place)],
        };

        let result = wasm_module.lower_terminator(&return_terminator, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Return terminator should lower successfully");
    }

    #[test]
    fn test_terminator_lowering_unreachable() {
        let mut wasm_module = WasmModule::new();
        let mut wasm_function = Function::new(vec![]);
        let local_map = HashMap::new();

        let unreachable_terminator = Terminator::Unreachable;

        let result = wasm_module.lower_terminator(&unreachable_terminator, &mut wasm_function, &local_map);
        assert!(result.is_ok(), "Unreachable terminator should lower successfully");
    }

    // ===== INTERFACE DISPATCH TESTS =====
    // Test vtable generation and call_indirect for dynamic dispatch

    #[test]
    fn test_interface_vtable_generation() {
        // Test that we can create interface info structures
        let interface_def = InterfaceDefinition {
            id: 0,
            name: "TestInterface".to_string(),
            methods: vec![
                MethodSignature {
                    id: 0,
                    name: "method1".to_string(),
                    param_types: vec![WasmType::ExternRef, WasmType::I32],
                    return_types: vec![WasmType::I32],
                },
                MethodSignature {
                    id: 1,
                    name: "method2".to_string(),
                    param_types: vec![WasmType::ExternRef],
                    return_types: vec![WasmType::F64],
                },
            ],
        };

        // Create vtable layout
        let vtable = VTable {
            interface_id: 0,
            type_id: 1,
            method_functions: vec![10, 11], // Function indices for methods
        };

        let mut interface_info = InterfaceInfo {
            interfaces: HashMap::new(),
            vtables: HashMap::new(),
            function_table: vec![10, 11],
        };
        interface_info.interfaces.insert(0, interface_def);
        interface_info.vtables.insert(0, vtable);

        // Test that interface info is properly structured
        assert_eq!(interface_info.interfaces.len(), 1);
        assert_eq!(interface_info.vtables.len(), 1);
        assert_eq!(interface_info.function_table.len(), 2);
    }

    #[test]
    fn test_interface_method_index_lookup() {
        let wasm_module = WasmModule::new();

        // Create interface with methods
        let interface_def = InterfaceDefinition {
            id: 0,
            name: "TestInterface".to_string(),
            methods: vec![
                MethodSignature {
                    id: 0,
                    name: "method1".to_string(),
                    param_types: vec![WasmType::ExternRef],
                    return_types: vec![WasmType::I32],
                },
                MethodSignature {
                    id: 1,
                    name: "method2".to_string(),
                    param_types: vec![WasmType::ExternRef],
                    return_types: vec![WasmType::F64],
                },
            ],
        };

        let interface_info = InterfaceInfo {
            interfaces: {
                let mut map = HashMap::new();
                map.insert(0, interface_def);
                map
            },
            vtables: HashMap::new(),
            function_table: Vec::new(),
        };

        // Test method type index lookup
        let result = wasm_module.get_interface_method_type_index(0, 0, &interface_info);
        assert!(result.is_ok(), "Method type index lookup should succeed");

        let result = wasm_module.get_interface_method_type_index(0, 1, &interface_info);
        assert!(result.is_ok(), "Method type index lookup should succeed for second method");
    }

    #[test]
    fn test_vtable_offset_calculation() {
        let wasm_module = WasmModule::new();

        // Create vtables for different types implementing the same interface
        let vtable1 = VTable {
            interface_id: 0,
            type_id: 1,
            method_functions: vec![10, 11], // 2 methods
        };

        let vtable2 = VTable {
            interface_id: 0,
            type_id: 2,
            method_functions: vec![20, 21], // 2 methods
        };

        let interface_info = InterfaceInfo {
            interfaces: HashMap::new(),
            vtables: {
                let mut map = HashMap::new();
                map.insert(1, vtable1);
                map.insert(2, vtable2);
                map
            },
            function_table: Vec::new(),
        };

        // Test vtable offset calculation
        let offset1 = wasm_module.calculate_vtable_offset(0, 1, &interface_info);
        assert!(offset1.is_ok(), "VTable offset calculation should succeed for type 1");

        let offset2 = wasm_module.calculate_vtable_offset(0, 2, &interface_info);
        assert!(offset2.is_ok(), "VTable offset calculation should succeed for type 2");

        // Second vtable should be after first (2 methods * 4 bytes = 8 bytes offset)
        assert_eq!(offset2.unwrap(), 8);
    }

    // ===== MEMORY LAYOUT TESTS =====
    // Test struct fields and array access patterns

    #[test]
    fn test_struct_field_layout_calculation() {
        let wasm_module = WasmModule::new();

        // Test struct with various field types
        let field_types = vec![
            WasmType::I32,     // 4 bytes, 4-byte aligned
            WasmType::I64,     // 8 bytes, 8-byte aligned  
            WasmType::F32,     // 4 bytes, 4-byte aligned
            WasmType::F64,     // 8 bytes, 8-byte aligned
            WasmType::ExternRef, // 4 bytes, 4-byte aligned (pointer)
        ];

        let layout = wasm_module.calculate_struct_layout(&field_types);

        // Verify proper alignment
        assert_eq!(layout.get_field_offset(0), Some(0));  // I32 at offset 0
        assert_eq!(layout.get_field_offset(1), Some(8));  // I64 aligned to 8 bytes
        assert_eq!(layout.get_field_offset(2), Some(16)); // F32 after I64
        assert_eq!(layout.get_field_offset(3), Some(24)); // F64 aligned to 8 bytes
        assert_eq!(layout.get_field_offset(4), Some(32)); // ExternRef after F64

        // Verify field sizes
        assert_eq!(layout.get_field_size(0), Some(4));  // I32
        assert_eq!(layout.get_field_size(1), Some(8));  // I64
        assert_eq!(layout.get_field_size(2), Some(4));  // F32
        assert_eq!(layout.get_field_size(3), Some(8));  // F64
        assert_eq!(layout.get_field_size(4), Some(4));  // ExternRef

        // Total size should be aligned to largest alignment (8 bytes)
        assert_eq!(layout.total_size, 40); // 36 bytes rounded up to 8-byte boundary
        assert_eq!(layout.alignment, 8);
    }

    #[test]
    fn test_array_element_access_calculation() {
        let wasm_module = WasmModule::new();

        // Test different element sizes
        let element_sizes = vec![
            (WasmType::I32, 4),
            (WasmType::I64, 8),
            (WasmType::F32, 4),
            (WasmType::F64, 8),
            (WasmType::ExternRef, 4),
        ];

        for (wasm_type, expected_size) in element_sizes {
            let size = wasm_module.get_wasm_type_size(&wasm_type);
            assert_eq!(size, expected_size, "Size for {:?} should be {}", wasm_type, expected_size);

            let alignment = wasm_module.get_wasm_type_alignment(&wasm_type);
            assert_eq!(alignment, expected_size, "Alignment for {:?} should be {}", wasm_type, expected_size);
        }
    }

    #[test]
    fn test_memory_layout_with_padding() {
        let wasm_module = WasmModule::new();

        // Test struct that requires padding
        let field_types = vec![
            WasmType::I32,     // 4 bytes
            WasmType::I64,     // 8 bytes (requires 4 bytes padding after I32)
            WasmType::I32,     // 4 bytes
            // Total: 4 + 4(pad) + 8 + 4 = 20 bytes, rounded to 24 for 8-byte alignment
        ];

        let layout = wasm_module.calculate_struct_layout(&field_types);

        assert_eq!(layout.get_field_offset(0), Some(0));  // I32 at 0
        assert_eq!(layout.get_field_offset(1), Some(8));  // I64 at 8 (4 bytes padding)
        assert_eq!(layout.get_field_offset(2), Some(16)); // I32 at 16

        assert_eq!(layout.total_size, 24); // Padded to 8-byte boundary
        assert_eq!(layout.alignment, 8);   // Largest alignment requirement
    }

    #[test]
    fn test_nested_struct_layout() {
        let wasm_module = WasmModule::new();

        // Test nested struct (struct containing another struct)
        // Inner struct: { I32, I64 } = 16 bytes (8-byte aligned)
        // Outer struct: { inner_struct, F32 } = 20 bytes, padded to 24

        let inner_struct_types = vec![WasmType::I32, WasmType::I64];
        let inner_layout = wasm_module.calculate_struct_layout(&inner_struct_types);

        // Simulate outer struct with inner struct as first field
        let outer_field_types = vec![
            // Represent inner struct as a block of bytes (simplified)
            WasmType::I64, // Represents inner struct (8-byte aligned, 16 bytes)
            WasmType::F32, // Additional field
        ];

        let outer_layout = wasm_module.calculate_struct_layout(&outer_field_types);

        assert_eq!(inner_layout.total_size, 16);
        assert_eq!(inner_layout.alignment, 8);
        assert_eq!(outer_layout.total_size, 16); // I64 + F32 = 12, padded to 16
    }

    // ===== INTEGRATION TESTS =====
    // Test complete MIR → WASM module generation

    #[test]
    fn test_complete_mir_to_wasm_empty_module() {
        let mir = MIR::new();

        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Empty MIR should generate valid WASM module");

        let wasm_bytes = result.unwrap();
        assert!(!wasm_bytes.is_empty(), "Generated WASM should not be empty");
        assert_eq!(&wasm_bytes[0..4], b"\0asm", "Should have WASM magic number");
    }

    #[test]
    fn test_complete_mir_to_wasm_simple_function() {
        let mut mir = MIR::new();

        // Create simple function: fn add(a: i32, b: i32) -> i32 { return a + b; }
        let mut function = MirFunction::new(
            0,
            "add".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::I32 },
                Place::Local { index: 1, wasm_type: WasmType::I32 },
            ],
            vec![WasmType::I32],
        );

        let mut block = MirBlock::new(0);

        // result = a + b
        let add_stmt = Statement::Assign {
            place: Place::Local { index: 2, wasm_type: WasmType::I32 },
            rvalue: Rvalue::BinaryOp {
                op: BinOp::Add,
                left: Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 }),
                right: Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 }),
            },
        };
        block.statements.push(add_stmt);

        // return result
        block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 2, wasm_type: WasmType::I32 })],
        };

        function.add_block(block);
        mir.add_function(function);

        // Add export
        mir.exports.insert("add".to_string(), Export {
            name: "add".to_string(),
            kind: ExportKind::Function,
            index: 0,
        });

        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Simple function MIR should generate valid WASM module");

        let wasm_bytes = result.unwrap();
        assert!(wasm_bytes.len() > 20, "Generated WASM should be substantial for function");
    }

    #[test]
    fn test_complete_mir_to_wasm_with_globals() {
        let mut mir = MIR::new();

        // Add global variable
        mir.globals.insert(0, Place::Global { index: 0, wasm_type: WasmType::I32 });

        // Create function that uses global
        let mut function = MirFunction::new(
            0,
            "use_global".to_string(),
            vec![],
            vec![WasmType::I32],
        );

        let mut block = MirBlock::new(0);

        // Load global value
        let load_global = Statement::Assign {
            place: Place::Local { index: 0, wasm_type: WasmType::I32 },
            rvalue: Rvalue::Use(Operand::Copy(Place::Global { index: 0, wasm_type: WasmType::I32 })),
        };
        block.statements.push(load_global);

        // Return global value
        block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 })],
        };

        function.add_block(block);
        mir.add_function(function);

        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "MIR with globals should generate valid WASM module");
    }

    #[test]
    fn test_complete_mir_to_wasm_with_memory() {
        let mut mir = MIR::new();

        // Configure memory
        mir.type_info.memory_info = MemoryInfo {
            initial_pages: 2,
            max_pages: Some(10),
            static_data_size: 1024,
        };

        // Create function that uses memory
        let mut function = MirFunction::new(
            0,
            "use_memory".to_string(),
            vec![Place::Local { index: 0, wasm_type: WasmType::ExternRef }],
            vec![WasmType::I32],
        );

        let mut block = MirBlock::new(0);

        // Load from memory
        let memory_place = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: ByteOffset(0),
            size: TypeSize::Word,
        };

        let load_memory = Statement::Assign {
            place: Place::Local { index: 1, wasm_type: WasmType::I32 },
            rvalue: Rvalue::Use(Operand::Copy(memory_place)),
        };
        block.statements.push(load_memory);

        // Return loaded value
        block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 })],
        };

        function.add_block(block);
        mir.add_function(function);

        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "MIR with memory should generate valid WASM module");
    }

    #[test]
    fn test_complete_mir_to_wasm_with_interfaces() {
        let mut mir = MIR::new();

        // Set up interface system
        let interface_def = InterfaceDefinition {
            id: 0,
            name: "TestInterface".to_string(),
            methods: vec![MethodSignature {
                id: 0,
                name: "test_method".to_string(),
                param_types: vec![WasmType::ExternRef, WasmType::I32],
                return_types: vec![WasmType::I32],
            }],
        };

        let vtable = VTable {
            interface_id: 0,
            type_id: 1,
            method_functions: vec![1], // Points to implementation function
        };

        mir.type_info.interface_info.interfaces.insert(0, interface_def);
        mir.type_info.interface_info.vtables.insert(0, vtable);
        mir.type_info.interface_info.function_table = vec![1];

        // Create implementation function
        let mut impl_function = MirFunction::new(
            1,
            "test_impl".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::ExternRef },
                Place::Local { index: 1, wasm_type: WasmType::I32 },
            ],
            vec![WasmType::I32],
        );

        let mut impl_block = MirBlock::new(0);
        impl_block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 })],
        };
        impl_function.add_block(impl_block);
        mir.add_function(impl_function);

        // Create caller function
        let mut caller_function = MirFunction::new(
            0,
            "caller".to_string(),
            vec![Place::Local { index: 0, wasm_type: WasmType::ExternRef }],
            vec![WasmType::I32],
        );

        let mut caller_block = MirBlock::new(0);

        // Interface call
        let interface_call = Statement::InterfaceCall {
            interface_id: 0,
            method_id: 0,
            receiver: Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::ExternRef }),
            args: vec![Operand::Constant(Constant::I32(42))],
            destination: Some(Place::Local { index: 1, wasm_type: WasmType::I32 }),
        };
        caller_block.statements.push(interface_call);

        caller_block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 })],
        };

        caller_function.add_block(caller_block);
        mir.add_function(caller_function);

        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "MIR with interfaces should generate valid WASM module");
    }

    #[test]
    fn test_complete_mir_to_wasm_complex_control_flow() {
        let mut mir = MIR::new();

        // Create function with complex control flow
        let mut function = MirFunction::new(
            0,
            "complex_flow".to_string(),
            vec![Place::Local { index: 0, wasm_type: WasmType::I32 }],
            vec![WasmType::I32],
        );

        // Block 0: Entry - check condition
        let mut block0 = MirBlock::new(0);
        let condition_check = Statement::Assign {
            place: Place::Local { index: 1, wasm_type: WasmType::I32 },
            rvalue: Rvalue::BinaryOp {
                op: BinOp::Gt,
                left: Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 }),
                right: Operand::Constant(Constant::I32(0)),
            },
        };
        block0.statements.push(condition_check);
        block0.terminator = Terminator::If {
            condition: Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 }),
            then_block: 1,
            else_block: 2,
            wasm_if_info: WasmIfInfo {
                has_else: true,
                result_type: None,
                nesting_level: 0,
            },
        };

        // Block 1: Then branch
        let mut block1 = MirBlock::new(1);
        let then_result = Statement::Assign {
            place: Place::Local { index: 2, wasm_type: WasmType::I32 },
            rvalue: Rvalue::Use(Operand::Constant(Constant::I32(1))),
        };
        block1.statements.push(then_result);
        block1.terminator = Terminator::Goto { target: 3, label_depth: 0 };

        // Block 2: Else branch
        let mut block2 = MirBlock::new(2);
        let else_result = Statement::Assign {
            place: Place::Local { index: 2, wasm_type: WasmType::I32 },
            rvalue: Rvalue::Use(Operand::Constant(Constant::I32(-1))),
        };
        block2.statements.push(else_result);
        block2.terminator = Terminator::Goto { target: 3, label_depth: 0 };

        // Block 3: Exit
        let mut block3 = MirBlock::new(3);
        block3.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 2, wasm_type: WasmType::I32 })],
        };

        function.add_block(block0);
        function.add_block(block1);
        function.add_block(block2);
        function.add_block(block3);
        mir.add_function(function);

        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "MIR with complex control flow should generate valid WASM module");
    }

    #[test]
    fn test_wasm_module_validation_integration() {
        let mut mir = MIR::new();

        // Create a function that should pass validation
        let mut function = MirFunction::new(
            0,
            "validated_function".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::I32 },
                Place::Local { index: 1, wasm_type: WasmType::F64 },
            ],
            vec![WasmType::F64],
        );

        let mut block = MirBlock::new(0);
        
        // Convert i32 to f64 and return
        let convert_stmt = Statement::Assign {
            place: Place::Local { index: 2, wasm_type: WasmType::F64 },
            rvalue: Rvalue::Cast {
                source: Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 }),
                target_type: WasmType::F64,
            },
        };
        block.statements.push(convert_stmt);

        block.terminator = Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 2, wasm_type: WasmType::F64 })],
        };

        function.add_block(block);
        mir.add_function(function);

        // Test that the complete pipeline including validation works
        let result = new_wasm_module(mir);
        assert!(result.is_ok(), "Valid MIR should pass complete validation pipeline");

        let wasm_bytes = result.unwrap();
        
        // Additional validation using wasmparser directly
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(validation_result.is_ok(), "Generated WASM should pass wasmparser validation");
    }
}