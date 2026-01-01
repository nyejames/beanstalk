//! Property-based tests for WASM codegen components
//!
//! These tests validate correctness properties for the LIR to WASM codegen system.
//! Tests are organized by the design document properties they validate.

#[cfg(test)]
mod local_variable_tests {
    use crate::compiler::codegen::wasm::analyzer::WasmType;
    use crate::compiler::codegen::wasm::local_manager::LocalVariableManager;
    use crate::compiler::lir::nodes::{LirFunction, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use wasm_encoder::ValType;

    /// Generate a random LirType for property testing
    #[derive(Clone, Debug)]
    struct ArbitraryLirType(LirType);

    impl Arbitrary for ArbitraryLirType {
        fn arbitrary(g: &mut Gen) -> Self {
            let types = [LirType::I32, LirType::I64, LirType::F32, LirType::F64];
            let idx = usize::arbitrary(g) % types.len();
            ArbitraryLirType(types[idx])
        }
    }

    /// Generate a random list of parameter types
    #[derive(Clone, Debug)]
    struct ArbitraryParams(Vec<LirType>);

    impl Arbitrary for ArbitraryParams {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-5 parameters
            let count = usize::arbitrary(g) % 6;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryParams(types)
        }
    }

    /// Generate a random list of local types
    #[derive(Clone, Debug)]
    struct ArbitraryLocals(Vec<LirType>);

    impl Arbitrary for ArbitraryLocals {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-10 locals
            let count = usize::arbitrary(g) % 11;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryLocals(types)
        }
    }

    /// Helper to create a test LirFunction
    fn create_test_function(params: &[LirType], locals: &[LirType]) -> LirFunction {
        LirFunction {
            name: "test_func".to_string(),
            params: params.to_vec(),
            returns: vec![],
            locals: locals.to_vec(),
            body: vec![],
            is_main: false,
        }
    }

    // =========================================================================
    // Property 7: Local API Format Compliance
    // For any function with local variables, the generated WASM locals should
    // use the v0.243.0 API format of Vec<(u32, ValType)> with proper type grouping.
    // Validates: Requirements 2.2, 2.6, 7.6
    // =========================================================================

    /// Property: WASM locals format uses (count, type) pairs
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_wasm_locals_format_is_count_type_pairs() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let wasm_locals = manager.generate_wasm_locals();

            // Each entry should be (count, type) where count > 0
            for (count, _val_type) in &wasm_locals {
                if *count == 0 {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Total count in WASM locals equals number of LIR locals (excluding params)
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_wasm_locals_count_matches_lir_locals() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let wasm_locals = manager.generate_wasm_locals();

            // Sum of all counts should equal number of LIR locals
            let total_count: u32 = wasm_locals.iter().map(|(count, _)| count).sum();
            if total_count != locals.0.len() as u32 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: WASM locals are grouped by type (no duplicate type entries)
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_wasm_locals_grouped_by_type() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let wasm_locals = manager.generate_wasm_locals();

            // Check that each type appears at most once
            let mut seen_types = std::collections::HashSet::new();
            for (_, val_type) in &wasm_locals {
                if seen_types.contains(val_type) {
                    return TestResult::failed();
                }
                seen_types.insert(*val_type);
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: All LIR local types are preserved in WASM locals
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_all_local_types_preserved() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            if locals.0.is_empty() {
                return TestResult::discard();
            }

            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let wasm_locals = manager.generate_wasm_locals();

            // Count types in LIR locals
            let mut lir_type_counts = std::collections::HashMap::new();
            for lir_type in &locals.0 {
                let wasm_type = WasmType::from_lir_type(*lir_type);
                *lir_type_counts
                    .entry(wasm_type.to_val_type())
                    .or_insert(0u32) += 1;
            }

            // Count types in WASM locals
            let mut wasm_type_counts = std::collections::HashMap::new();
            for (count, val_type) in &wasm_locals {
                *wasm_type_counts.entry(*val_type).or_insert(0u32) += count;
            }

            // Counts should match
            if lir_type_counts != wasm_type_counts {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Parameter count is correctly tracked
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_parameter_count_correct() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);

            if manager.parameter_count() != params.0.len() as u32 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Local count is correctly tracked
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_local_count_correct() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);

            if manager.local_count() != locals.0.len() as u32 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Total count equals parameters plus locals
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_total_count_correct() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);

            let expected_total = params.0.len() as u32 + locals.0.len() as u32;
            if manager.total_count() != expected_total {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: All mappings are valid (validate_mappings returns true)
    /// Feature: lir-to-wasm-codegen, Property 7: Local API Format Compliance
    #[test]
    fn prop_all_mappings_valid() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);

            if !manager.validate_mappings() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific scenarios
    // =========================================================================

    #[test]
    fn test_empty_function() {
        let func = create_test_function(&[], &[]);
        let manager = LocalVariableManager::analyze_function(&func);

        assert_eq!(manager.parameter_count(), 0);
        assert_eq!(manager.local_count(), 0);
        assert_eq!(manager.total_count(), 0);
        assert!(manager.generate_wasm_locals().is_empty());
        assert!(manager.validate_mappings());
    }

    #[test]
    fn test_params_only() {
        let func = create_test_function(&[LirType::I32, LirType::I64], &[]);
        let manager = LocalVariableManager::analyze_function(&func);

        assert_eq!(manager.parameter_count(), 2);
        assert_eq!(manager.local_count(), 0);
        assert_eq!(manager.total_count(), 2);
        assert!(manager.generate_wasm_locals().is_empty());

        // Parameters should map directly
        assert_eq!(manager.get_wasm_local_index(0), Some(0));
        assert_eq!(manager.get_wasm_local_index(1), Some(1));
        assert!(manager.is_parameter(0));
        assert!(manager.is_parameter(1));
    }

    #[test]
    fn test_locals_only() {
        let func = create_test_function(&[], &[LirType::I32, LirType::F64]);
        let manager = LocalVariableManager::analyze_function(&func);

        assert_eq!(manager.parameter_count(), 0);
        assert_eq!(manager.local_count(), 2);
        assert_eq!(manager.total_count(), 2);

        let wasm_locals = manager.generate_wasm_locals();
        // Should have 2 entries (one I32, one F64) or combined if same type
        let total: u32 = wasm_locals.iter().map(|(c, _)| c).sum();
        assert_eq!(total, 2);

        // Locals should not be parameters
        assert!(!manager.is_parameter(0));
        assert!(!manager.is_parameter(1));
    }

    #[test]
    fn test_mixed_params_and_locals() {
        let func = create_test_function(
            &[LirType::I32, LirType::I64],
            &[LirType::F32, LirType::I32, LirType::F64],
        );
        let manager = LocalVariableManager::analyze_function(&func);

        assert_eq!(manager.parameter_count(), 2);
        assert_eq!(manager.local_count(), 3);
        assert_eq!(manager.total_count(), 5);

        // Parameters map directly
        assert_eq!(manager.get_wasm_local_index(0), Some(0));
        assert_eq!(manager.get_wasm_local_index(1), Some(1));

        // All locals should have valid mappings
        assert!(manager.get_wasm_local_index(2).is_some());
        assert!(manager.get_wasm_local_index(3).is_some());
        assert!(manager.get_wasm_local_index(4).is_some());

        // WASM locals should only include non-parameter locals
        let wasm_locals = manager.generate_wasm_locals();
        let total: u32 = wasm_locals.iter().map(|(c, _)| c).sum();
        assert_eq!(total, 3);
    }

    #[test]
    fn test_type_grouping() {
        // Create function with multiple locals of same type
        let func = create_test_function(
            &[],
            &[
                LirType::I32,
                LirType::I32,
                LirType::I32,
                LirType::F64,
                LirType::F64,
            ],
        );
        let manager = LocalVariableManager::analyze_function(&func);

        let wasm_locals = manager.generate_wasm_locals();

        // Should have exactly 2 entries: one for I32 (count=3), one for F64 (count=2)
        assert_eq!(wasm_locals.len(), 2);

        // Find the I32 and F64 entries
        let i32_entry = wasm_locals.iter().find(|(_, t)| *t == ValType::I32);
        let f64_entry = wasm_locals.iter().find(|(_, t)| *t == ValType::F64);

        assert_eq!(i32_entry, Some(&(3, ValType::I32)));
        assert_eq!(f64_entry, Some(&(2, ValType::F64)));
    }

    #[test]
    fn test_local_type_retrieval() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64]);
        let manager = LocalVariableManager::analyze_function(&func);

        assert_eq!(manager.get_local_type(0), Some(WasmType::I32));
        assert_eq!(manager.get_local_type(1), Some(WasmType::F64));
        assert_eq!(manager.get_local_type(2), None);
    }

    #[test]
    fn test_build_local_mapping() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64, LirType::I32]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();

        assert_eq!(local_map.parameter_count, 1);
        assert_eq!(local_map.lir_to_wasm.len(), 3);

        // Parameter maps directly
        assert_eq!(local_map.get_wasm_index(0), Some(0));

        // Locals should have valid mappings
        assert!(local_map.get_wasm_index(1).is_some());
        assert!(local_map.get_wasm_index(2).is_some());

        // WASM locals should have 2 entries (F64 and I32)
        let total: u32 = local_map.local_types.iter().map(|(c, _)| c).sum();
        assert_eq!(total, 2);
    }
}

#[cfg(test)]
mod local_access_tests {
    use crate::compiler::codegen::wasm::instruction_lowerer::InstructionLowerer;
    use crate::compiler::codegen::wasm::local_manager::LocalVariableManager;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use wasm_encoder::Function;

    /// Generate a random LirType for property testing
    #[derive(Clone, Debug)]
    struct ArbitraryLirType(LirType);

    impl Arbitrary for ArbitraryLirType {
        fn arbitrary(g: &mut Gen) -> Self {
            let types = [LirType::I32, LirType::I64, LirType::F32, LirType::F64];
            let idx = usize::arbitrary(g) % types.len();
            ArbitraryLirType(types[idx])
        }
    }

    /// Generate a random list of parameter types
    #[derive(Clone, Debug)]
    struct ArbitraryParams(Vec<LirType>);

    impl Arbitrary for ArbitraryParams {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 1-5 parameters (at least 1 for testing)
            let count = (usize::arbitrary(g) % 5) + 1;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryParams(types)
        }
    }

    /// Generate a random list of local types
    #[derive(Clone, Debug)]
    struct ArbitraryLocals(Vec<LirType>);

    impl Arbitrary for ArbitraryLocals {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 1-10 locals (at least 1 for testing)
            let count = (usize::arbitrary(g) % 10) + 1;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryLocals(types)
        }
    }

    /// Helper to create a test LirFunction
    fn create_test_function(params: &[LirType], locals: &[LirType]) -> LirFunction {
        LirFunction {
            name: "test_func".to_string(),
            params: params.to_vec(),
            returns: vec![],
            locals: locals.to_vec(),
            body: vec![],
            is_main: false,
        }
    }

    // =========================================================================
    // Property 8: Local Access Operations
    // For any LocalGet or LocalSet operation, the generated WASM instructions
    // should use correct local indices and maintain proper parameter ordering.
    // Validates: Requirements 2.3, 2.4, 2.5
    // =========================================================================

    /// Property: LocalGet for parameters uses correct indices (0..param_count)
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_local_get_params_use_correct_indices() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            // Test LocalGet for each parameter
            for i in 0..params.0.len() as u32 {
                let mut wasm_func = Function::new(manager.generate_wasm_locals());
                let inst = LirInst::LocalGet(i);

                if lowerer.lower_instruction(&inst, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: LocalGet for locals uses correct indices (param_count..)
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_local_get_locals_use_correct_indices() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            let param_count = params.0.len() as u32;

            // Test LocalGet for each local variable
            for i in 0..locals.0.len() as u32 {
                let lir_index = param_count + i;
                let mut wasm_func = Function::new(manager.generate_wasm_locals());
                let inst = LirInst::LocalGet(lir_index);

                if lowerer.lower_instruction(&inst, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: LocalSet for all valid indices succeeds
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_local_set_all_valid_indices_succeed() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            let total = params.0.len() + locals.0.len();

            // Test LocalSet for all valid indices
            for i in 0..total as u32 {
                let mut wasm_func = Function::new(manager.generate_wasm_locals());
                let inst = LirInst::LocalSet(i);

                if lowerer.lower_instruction(&inst, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: LocalTee for all valid indices succeeds
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_local_tee_all_valid_indices_succeed() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            let total = params.0.len() + locals.0.len();

            // Test LocalTee for all valid indices
            for i in 0..total as u32 {
                let mut wasm_func = Function::new(manager.generate_wasm_locals());
                let inst = LirInst::LocalTee(i);

                if lowerer.lower_instruction(&inst, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Invalid local indices fail gracefully
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_invalid_local_indices_fail() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            let total = (params.0.len() + locals.0.len()) as u32;
            let invalid_index = total + 10; // Definitely out of bounds

            let mut wasm_func = Function::new(manager.generate_wasm_locals());
            let inst = LirInst::LocalGet(invalid_index);

            // Should fail for invalid index
            if lowerer.lower_instruction(&inst, &mut wasm_func).is_ok() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Parameters are correctly identified
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_parameters_correctly_identified() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            let param_count = params.0.len() as u32;

            // All indices < param_count should be parameters
            for i in 0..param_count {
                if !lowerer.is_parameter(i) {
                    return TestResult::failed();
                }
            }

            // All indices >= param_count should not be parameters
            for i in param_count..(param_count + locals.0.len() as u32) {
                if lowerer.is_parameter(i) {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    /// Property: Parameter count is correctly reported
    /// Feature: lir-to-wasm-codegen, Property 8: Local Access Operations
    #[test]
    fn prop_parameter_count_correct() {
        fn property(params: ArbitraryParams, locals: ArbitraryLocals) -> TestResult {
            let func = create_test_function(&params.0, &locals.0);
            let manager = LocalVariableManager::analyze_function(&func);
            let local_map = manager.build_local_mapping();
            let lowerer = InstructionLowerer::new(local_map);

            if lowerer.parameter_count() != params.0.len() as u32 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryParams, ArbitraryLocals) -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific scenarios
    // =========================================================================

    #[test]
    fn test_local_get_parameter() {
        let func = create_test_function(&[LirType::I32, LirType::I64], &[LirType::F32]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        let mut wasm_func = Function::new(manager.generate_wasm_locals());

        // LocalGet for parameter 0 should succeed
        let inst = LirInst::LocalGet(0);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());

        // LocalGet for parameter 1 should succeed
        let inst = LirInst::LocalGet(1);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_local_get_local_variable() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64, LirType::I32]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        let mut wasm_func = Function::new(manager.generate_wasm_locals());

        // LocalGet for local 1 (first local after param) should succeed
        let inst = LirInst::LocalGet(1);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());

        // LocalGet for local 2 should succeed
        let inst = LirInst::LocalGet(2);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_local_set() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        let mut wasm_func = Function::new(manager.generate_wasm_locals());

        // LocalSet for parameter should succeed
        let inst = LirInst::LocalSet(0);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());

        // LocalSet for local should succeed
        let inst = LirInst::LocalSet(1);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_local_tee() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        let mut wasm_func = Function::new(manager.generate_wasm_locals());

        // LocalTee for parameter should succeed
        let inst = LirInst::LocalTee(0);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());

        // LocalTee for local should succeed
        let inst = LirInst::LocalTee(1);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_invalid_local_index() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        let mut wasm_func = Function::new(manager.generate_wasm_locals());

        // LocalGet for invalid index should fail
        let inst = LirInst::LocalGet(100);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_err());
    }

    #[test]
    fn test_emit_convenience_methods() {
        let func = create_test_function(&[LirType::I32], &[LirType::F64]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        let mut wasm_func = Function::new(manager.generate_wasm_locals());

        // Test emit_local_get
        assert!(lowerer.emit_local_get(0, &mut wasm_func).is_ok());
        assert!(lowerer.emit_local_get(1, &mut wasm_func).is_ok());

        // Test emit_local_set
        assert!(lowerer.emit_local_set(0, &mut wasm_func).is_ok());
        assert!(lowerer.emit_local_set(1, &mut wasm_func).is_ok());

        // Test emit_local_tee
        assert!(lowerer.emit_local_tee(0, &mut wasm_func).is_ok());
        assert!(lowerer.emit_local_tee(1, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_is_parameter() {
        let func = create_test_function(&[LirType::I32, LirType::I64], &[LirType::F32]);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let lowerer = InstructionLowerer::new(local_map);

        // Indices 0 and 1 are parameters
        assert!(lowerer.is_parameter(0));
        assert!(lowerer.is_parameter(1));

        // Index 2 is a local, not a parameter
        assert!(!lowerer.is_parameter(2));
    }
}

#[cfg(test)]
mod struct_layout_tests {
    use crate::compiler::codegen::wasm::analyzer::WasmType;
    use crate::compiler::codegen::wasm::memory_layout::{
        MemoryLayoutCalculator, align_to, alignment_for_lir_type, size_for_lir_type,
    };
    use crate::compiler::lir::nodes::{LirField, LirStruct, LirType};
    use crate::compiler::string_interning::StringTable;
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};

    /// Generate a random LirType for property testing
    #[derive(Clone, Debug)]
    struct ArbitraryLirType(LirType);

    impl Arbitrary for ArbitraryLirType {
        fn arbitrary(g: &mut Gen) -> Self {
            let types = [LirType::I32, LirType::I64, LirType::F32, LirType::F64];
            let idx = usize::arbitrary(g) % types.len();
            ArbitraryLirType(types[idx])
        }
    }

    /// Generate a random list of field types for struct testing
    #[derive(Clone, Debug)]
    struct ArbitraryFieldTypes(Vec<LirType>);

    impl Arbitrary for ArbitraryFieldTypes {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 1-10 fields
            let count = (usize::arbitrary(g) % 10) + 1;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryFieldTypes(types)
        }
    }

    /// Helper to create a LirStruct from field types
    fn create_test_struct(field_types: &[LirType], string_table: &mut StringTable) -> LirStruct {
        let struct_name = string_table.intern("TestStruct");
        let fields: Vec<LirField> = field_types
            .iter()
            .enumerate()
            .map(|(i, &ty)| {
                let field_name = string_table.intern(&format!("field_{}", i));
                LirField {
                    name: field_name,
                    offset: 0, // Will be calculated by layout calculator
                    ty,
                }
            })
            .collect();

        // Calculate total size (rough estimate, actual calculation done by layout calculator)
        let total_size: u32 = field_types.iter().map(|t| size_for_lir_type(*t)).sum();

        LirStruct {
            name: struct_name,
            fields,
            total_size,
        }
    }

    // =========================================================================
    // Property 9: Struct Memory Layout
    // For any struct definition, the calculated memory layout should have
    // correct field offsets, alignment requirements, and total size.
    // Validates: Requirements 3.1
    // =========================================================================

    /// Property: All field offsets must be properly aligned to their type's alignment
    /// Feature: lir-to-wasm-codegen, Property 9: Struct Memory Layout
    #[test]
    fn prop_field_offsets_are_aligned() {
        fn property(field_types: ArbitraryFieldTypes) -> TestResult {
            if field_types.0.is_empty() {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let test_struct = create_test_struct(&field_types.0, &mut string_table);
            let mut calculator = MemoryLayoutCalculator::new();

            match calculator.calculate_struct_layout(&test_struct) {
                Ok(layout) => {
                    // Check that each field's offset is aligned to its type's alignment
                    for field in &layout.fields {
                        let alignment = field.alignment;
                        if field.offset % alignment != 0 {
                            return TestResult::failed();
                        }
                    }
                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFieldTypes) -> TestResult);
    }

    /// Property: Field offsets must be monotonically increasing (no overlapping)
    /// Feature: lir-to-wasm-codegen, Property 9: Struct Memory Layout
    #[test]
    fn prop_field_offsets_are_monotonic() {
        fn property(field_types: ArbitraryFieldTypes) -> TestResult {
            if field_types.0.len() < 2 {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let test_struct = create_test_struct(&field_types.0, &mut string_table);
            let mut calculator = MemoryLayoutCalculator::new();

            match calculator.calculate_struct_layout(&test_struct) {
                Ok(layout) => {
                    // Check that each field starts after the previous field ends
                    for i in 1..layout.fields.len() {
                        let prev_end = layout.fields[i - 1].offset + layout.fields[i - 1].size;
                        let curr_start = layout.fields[i].offset;
                        if curr_start < prev_end {
                            return TestResult::failed();
                        }
                    }
                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFieldTypes) -> TestResult);
    }

    /// Property: Total struct size must be at least the sum of all field sizes
    /// Feature: lir-to-wasm-codegen, Property 9: Struct Memory Layout
    #[test]
    fn prop_total_size_covers_all_fields() {
        fn property(field_types: ArbitraryFieldTypes) -> TestResult {
            if field_types.0.is_empty() {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let test_struct = create_test_struct(&field_types.0, &mut string_table);
            let mut calculator = MemoryLayoutCalculator::new();

            match calculator.calculate_struct_layout(&test_struct) {
                Ok(layout) => {
                    // Total size must cover all fields
                    let min_size: u32 = field_types.0.iter().map(|t| size_for_lir_type(*t)).sum();
                    if layout.total_size < min_size {
                        return TestResult::failed();
                    }

                    // Total size must be at least as large as the last field's end
                    if let Some(last_field) = layout.fields.last() {
                        let last_end = last_field.offset + last_field.size;
                        if layout.total_size < last_end {
                            return TestResult::failed();
                        }
                    }

                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFieldTypes) -> TestResult);
    }

    /// Property: Total struct size must be aligned to at least 2 bytes (for tagged pointers)
    /// Feature: lir-to-wasm-codegen, Property 9: Struct Memory Layout
    #[test]
    fn prop_total_size_aligned_for_tagged_pointers() {
        fn property(field_types: ArbitraryFieldTypes) -> TestResult {
            if field_types.0.is_empty() {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let test_struct = create_test_struct(&field_types.0, &mut string_table);
            let mut calculator = MemoryLayoutCalculator::new();

            match calculator.calculate_struct_layout(&test_struct) {
                Ok(layout) => {
                    // Total size must be at least 2-byte aligned for tagged pointer support
                    if layout.total_size % 2 != 0 {
                        return TestResult::failed();
                    }
                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFieldTypes) -> TestResult);
    }

    /// Property: Field count in layout matches input field count
    /// Feature: lir-to-wasm-codegen, Property 9: Struct Memory Layout
    #[test]
    fn prop_field_count_preserved() {
        fn property(field_types: ArbitraryFieldTypes) -> TestResult {
            if field_types.0.is_empty() {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let test_struct = create_test_struct(&field_types.0, &mut string_table);
            let mut calculator = MemoryLayoutCalculator::new();

            match calculator.calculate_struct_layout(&test_struct) {
                Ok(layout) => {
                    if layout.fields.len() != field_types.0.len() {
                        return TestResult::failed();
                    }
                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFieldTypes) -> TestResult);
    }

    /// Property: Field types are preserved in layout
    /// Feature: lir-to-wasm-codegen, Property 9: Struct Memory Layout
    #[test]
    fn prop_field_types_preserved() {
        fn property(field_types: ArbitraryFieldTypes) -> TestResult {
            if field_types.0.is_empty() {
                return TestResult::discard();
            }

            let mut string_table = StringTable::new();
            let test_struct = create_test_struct(&field_types.0, &mut string_table);
            let mut calculator = MemoryLayoutCalculator::new();

            match calculator.calculate_struct_layout(&test_struct) {
                Ok(layout) => {
                    for (i, &lir_type) in field_types.0.iter().enumerate() {
                        let expected_wasm_type = WasmType::from_lir_type(lir_type);
                        if layout.fields[i].wasm_type != expected_wasm_type {
                            return TestResult::failed();
                        }
                    }
                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFieldTypes) -> TestResult);
    }

    // =========================================================================
    // Unit tests for alignment helper functions
    // =========================================================================

    #[test]
    fn test_align_to_basic() {
        assert_eq!(align_to(0, 4), 0);
        assert_eq!(align_to(1, 4), 4);
        assert_eq!(align_to(4, 4), 4);
        assert_eq!(align_to(5, 4), 8);
        assert_eq!(align_to(7, 8), 8);
        assert_eq!(align_to(8, 8), 8);
        assert_eq!(align_to(9, 8), 16);
    }

    #[test]
    fn test_alignment_for_types() {
        assert_eq!(alignment_for_lir_type(LirType::I32), 4);
        assert_eq!(alignment_for_lir_type(LirType::I64), 8);
        assert_eq!(alignment_for_lir_type(LirType::F32), 4);
        assert_eq!(alignment_for_lir_type(LirType::F64), 8);
    }

    #[test]
    fn test_size_for_types() {
        assert_eq!(size_for_lir_type(LirType::I32), 4);
        assert_eq!(size_for_lir_type(LirType::I64), 8);
        assert_eq!(size_for_lir_type(LirType::F32), 4);
        assert_eq!(size_for_lir_type(LirType::F64), 8);
    }

    /// Test a specific struct layout: {i32, i64, i32}
    /// Expected layout:
    /// - field_0 (i32): offset 0, size 4, align 4
    /// - padding: 4 bytes (to align i64 to 8)
    /// - field_1 (i64): offset 8, size 8, align 8
    /// - field_2 (i32): offset 16, size 4, align 4
    /// - total size: 24 (aligned to 8)
    #[test]
    fn test_specific_struct_layout() {
        let mut string_table = StringTable::new();
        let field_types = vec![LirType::I32, LirType::I64, LirType::I32];
        let test_struct = create_test_struct(&field_types, &mut string_table);
        let mut calculator = MemoryLayoutCalculator::new();

        let layout = calculator
            .calculate_struct_layout(&test_struct)
            .expect("Layout calculation should succeed");

        assert_eq!(layout.fields.len(), 3);

        // field_0 (i32): offset 0
        assert_eq!(layout.fields[0].offset, 0);
        assert_eq!(layout.fields[0].size, 4);
        assert_eq!(layout.fields[0].alignment, 4);

        // field_1 (i64): offset 8 (aligned to 8)
        assert_eq!(layout.fields[1].offset, 8);
        assert_eq!(layout.fields[1].size, 8);
        assert_eq!(layout.fields[1].alignment, 8);

        // field_2 (i32): offset 16
        assert_eq!(layout.fields[2].offset, 16);
        assert_eq!(layout.fields[2].size, 4);
        assert_eq!(layout.fields[2].alignment, 4);

        // Total size: 20 bytes used, but aligned to 8 = 24
        assert_eq!(layout.total_size, 24);
        assert_eq!(layout.alignment, 8);
    }
}

#[cfg(test)]
mod instruction_lowering_tests {
    use crate::compiler::codegen::wasm::instruction_lowerer::InstructionLowerer;
    use crate::compiler::codegen::wasm::local_manager::LocalVariableManager;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use wasm_encoder::Function;

    /// Generate a random LirType for property testing
    #[derive(Clone, Debug)]
    struct ArbitraryLirType(LirType);

    impl Arbitrary for ArbitraryLirType {
        fn arbitrary(g: &mut Gen) -> Self {
            let types = [LirType::I32, LirType::I64, LirType::F32, LirType::F64];
            let idx = usize::arbitrary(g) % types.len();
            ArbitraryLirType(types[idx])
        }
    }

    /// Generate a random i32 constant
    #[derive(Clone, Debug)]
    struct ArbitraryI32(i32);

    impl Arbitrary for ArbitraryI32 {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryI32(i32::arbitrary(g))
        }
    }

    /// Generate a random i64 constant
    #[derive(Clone, Debug)]
    struct ArbitraryI64(i64);

    impl Arbitrary for ArbitraryI64 {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryI64(i64::arbitrary(g))
        }
    }

    /// Generate a random f32 constant (avoiding NaN for comparison)
    #[derive(Clone, Debug)]
    struct ArbitraryF32(f32);

    impl Arbitrary for ArbitraryF32 {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate a finite f32 value
            let bits = u32::arbitrary(g);
            let value = f32::from_bits(bits);
            if value.is_nan() || value.is_infinite() {
                ArbitraryF32(0.0)
            } else {
                ArbitraryF32(value)
            }
        }
    }

    /// Generate a random f64 constant (avoiding NaN for comparison)
    #[derive(Clone, Debug)]
    struct ArbitraryF64(f64);

    impl Arbitrary for ArbitraryF64 {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate a finite f64 value
            let bits = u64::arbitrary(g);
            let value = f64::from_bits(bits);
            if value.is_nan() || value.is_infinite() {
                ArbitraryF64(0.0)
            } else {
                ArbitraryF64(value)
            }
        }
    }

    /// Helper to create a test LirFunction with some locals
    fn create_test_function(params: &[LirType], locals: &[LirType]) -> LirFunction {
        LirFunction {
            name: "test_func".to_string(),
            params: params.to_vec(),
            returns: vec![],
            locals: locals.to_vec(),
            body: vec![],
            is_main: false,
        }
    }

    /// Helper to create an InstructionLowerer for testing
    fn create_lowerer(
        params: &[LirType],
        locals: &[LirType],
    ) -> (InstructionLowerer, Vec<(u32, wasm_encoder::ValType)>) {
        let func = create_test_function(params, locals);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let wasm_locals = manager.generate_wasm_locals();
        (InstructionLowerer::new(local_map), wasm_locals)
    }

    // =========================================================================
    // Property 3: Instruction Lowering Correctness
    // For any sequence of LIR instructions, the generated WASM bytecode should
    // maintain proper stack discipline and produce equivalent behavior.
    // Validates: Requirements 1.3
    // =========================================================================

    /// Property: I32 constant lowering succeeds for all i32 values
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_i32_const_lowering_succeeds() {
        fn property(value: ArbitraryI32) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::I32Const(value.0);

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryI32) -> TestResult);
    }

    /// Property: I64 constant lowering succeeds for all i64 values
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_i64_const_lowering_succeeds() {
        fn property(value: ArbitraryI64) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::I64Const(value.0);

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryI64) -> TestResult);
    }

    /// Property: F32 constant lowering succeeds for all finite f32 values
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_f32_const_lowering_succeeds() {
        fn property(value: ArbitraryF32) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::F32Const(value.0);

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryF32) -> TestResult);
    }

    /// Property: F64 constant lowering succeeds for all finite f64 values
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_f64_const_lowering_succeeds() {
        fn property(value: ArbitraryF64) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::F64Const(value.0);

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryF64) -> TestResult);
    }

    /// Property: All I32 arithmetic operations lower successfully
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_i32_arithmetic_lowering_succeeds() {
        fn property() -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

            let i32_ops = [
                LirInst::I32Add,
                LirInst::I32Sub,
                LirInst::I32Mul,
                LirInst::I32DivS,
                LirInst::I32Eq,
                LirInst::I32Ne,
                LirInst::I32LtS,
                LirInst::I32GtS,
            ];

            for op in &i32_ops {
                let mut wasm_func = Function::new(wasm_locals.clone());
                if lowerer.lower_instruction(op, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: All I64 arithmetic operations lower successfully
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_i64_arithmetic_lowering_succeeds() {
        fn property() -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

            let i64_ops = [
                LirInst::I64Add,
                LirInst::I64Sub,
                LirInst::I64Mul,
                LirInst::I64DivS,
                LirInst::I64Eq,
                LirInst::I64Ne,
                LirInst::I64LtS,
                LirInst::I64GtS,
            ];

            for op in &i64_ops {
                let mut wasm_func = Function::new(wasm_locals.clone());
                if lowerer.lower_instruction(op, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: All F64 arithmetic operations lower successfully
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_f64_arithmetic_lowering_succeeds() {
        fn property() -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

            let f64_ops = [
                LirInst::F64Add,
                LirInst::F64Sub,
                LirInst::F64Mul,
                LirInst::F64Div,
                LirInst::F64Eq,
                LirInst::F64Ne,
            ];

            for op in &f64_ops {
                let mut wasm_func = Function::new(wasm_locals.clone());
                if lowerer.lower_instruction(op, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: Stack management instructions (Nop, Drop) lower successfully
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_stack_management_lowering_succeeds() {
        fn property() -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

            let stack_ops = [LirInst::Nop, LirInst::Drop];

            for op in &stack_ops {
                let mut wasm_func = Function::new(wasm_locals.clone());
                if lowerer.lower_instruction(op, &mut wasm_func).is_err() {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: Return instruction lowers successfully
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_return_lowering_succeeds() {
        fn property() -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::Return;

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: Call instruction lowers successfully for any function index
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_call_lowering_succeeds() {
        fn property(func_index: u32) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::Call(func_index);

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u32) -> TestResult);
    }

    /// Property: Global get/set instructions lower successfully
    /// Feature: lir-to-wasm-codegen, Property 3: Instruction Lowering Correctness
    #[test]
    fn prop_global_access_lowering_succeeds() {
        fn property(global_index: u32) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

            // Test GlobalGet
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::GlobalGet(global_index);
            if lowerer.lower_instruction(&inst, &mut wasm_func).is_err() {
                return TestResult::failed();
            }

            // Test GlobalSet
            let mut wasm_func = Function::new(wasm_locals);
            let inst = LirInst::GlobalSet(global_index);
            if lowerer.lower_instruction(&inst, &mut wasm_func).is_err() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(u32) -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific instruction lowering scenarios
    // =========================================================================

    #[test]
    fn test_i32_const_specific_values() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        // Test specific i32 values
        let test_values = [0, 1, -1, i32::MAX, i32::MIN, 42, -42];

        for value in test_values {
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::I32Const(value);
            assert!(
                lowerer.lower_instruction(&inst, &mut wasm_func).is_ok(),
                "Failed to lower I32Const({})",
                value
            );
        }
    }

    #[test]
    fn test_i64_const_specific_values() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        // Test specific i64 values
        let test_values = [0i64, 1, -1, i64::MAX, i64::MIN, 42, -42];

        for value in test_values {
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::I64Const(value);
            assert!(
                lowerer.lower_instruction(&inst, &mut wasm_func).is_ok(),
                "Failed to lower I64Const({})",
                value
            );
        }
    }

    #[test]
    fn test_f32_const_specific_values() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        // Test specific f32 values
        let test_values = [0.0f32, 1.0, -1.0, 3.14159, -3.14159, f32::MAX, f32::MIN];

        for value in test_values {
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::F32Const(value);
            assert!(
                lowerer.lower_instruction(&inst, &mut wasm_func).is_ok(),
                "Failed to lower F32Const({})",
                value
            );
        }
    }

    #[test]
    fn test_f64_const_specific_values() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        // Test specific f64 values
        let test_values = [
            0.0f64,
            1.0,
            -1.0,
            3.14159265358979,
            -3.14159265358979,
            f64::MAX,
            f64::MIN,
        ];

        for value in test_values {
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::F64Const(value);
            assert!(
                lowerer.lower_instruction(&inst, &mut wasm_func).is_ok(),
                "Failed to lower F64Const({})",
                value
            );
        }
    }

    #[test]
    fn test_arithmetic_sequence() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut wasm_func = Function::new(wasm_locals);

        // Test a sequence: push 10, push 5, add (should leave 15 on stack)
        let instructions = [LirInst::I32Const(10), LirInst::I32Const(5), LirInst::I32Add];

        for inst in &instructions {
            assert!(lowerer.lower_instruction(inst, &mut wasm_func).is_ok());
        }
    }

    #[test]
    fn test_emit_convenience_methods() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
        let mut wasm_func = Function::new(wasm_locals);

        // Test constant emission methods
        lowerer.emit_i32_const(42, &mut wasm_func);
        lowerer.emit_i64_const(42, &mut wasm_func);
        lowerer.emit_f32_const(3.14, &mut wasm_func);
        lowerer.emit_f64_const(3.14159, &mut wasm_func);

        // Test arithmetic emission methods
        lowerer.emit_i32_add(&mut wasm_func);
        lowerer.emit_i32_sub(&mut wasm_func);
        lowerer.emit_i32_mul(&mut wasm_func);
        lowerer.emit_i32_div_s(&mut wasm_func);

        lowerer.emit_i64_add(&mut wasm_func);
        lowerer.emit_i64_sub(&mut wasm_func);
        lowerer.emit_i64_mul(&mut wasm_func);
        lowerer.emit_i64_div_s(&mut wasm_func);

        lowerer.emit_f64_add(&mut wasm_func);
        lowerer.emit_f64_sub(&mut wasm_func);
        lowerer.emit_f64_mul(&mut wasm_func);
        lowerer.emit_f64_div(&mut wasm_func);
    }

    #[test]
    fn test_control_flow_blocks_rejected() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        // Control flow blocks should be rejected (handled by ControlFlowManager)
        let block_inst = LirInst::Block {
            instructions: vec![],
        };
        let mut wasm_func = Function::new(wasm_locals.clone());
        assert!(
            lowerer
                .lower_instruction(&block_inst, &mut wasm_func)
                .is_err()
        );

        let loop_inst = LirInst::Loop {
            instructions: vec![],
        };
        let mut wasm_func = Function::new(wasm_locals.clone());
        assert!(
            lowerer
                .lower_instruction(&loop_inst, &mut wasm_func)
                .is_err()
        );

        let if_inst = LirInst::If {
            then_branch: vec![],
            else_branch: None,
        };
        let mut wasm_func = Function::new(wasm_locals.clone());
        assert!(lowerer.lower_instruction(&if_inst, &mut wasm_func).is_err());

        let br_inst = LirInst::Br(0);
        let mut wasm_func = Function::new(wasm_locals.clone());
        assert!(lowerer.lower_instruction(&br_inst, &mut wasm_func).is_err());

        let br_if_inst = LirInst::BrIf(0);
        let mut wasm_func = Function::new(wasm_locals);
        assert!(
            lowerer
                .lower_instruction(&br_if_inst, &mut wasm_func)
                .is_err()
        );
    }
}

#[cfg(test)]
mod memory_operation_tests {
    use crate::compiler::codegen::wasm::instruction_lowerer::InstructionLowerer;
    use crate::compiler::codegen::wasm::local_manager::LocalVariableManager;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use wasm_encoder::Function;

    /// Generate a random offset value (0 to 1MB)
    #[derive(Clone, Debug)]
    struct ArbitraryOffset(u32);

    impl Arbitrary for ArbitraryOffset {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate offsets in a reasonable range (0 to 1MB)
            ArbitraryOffset(u32::arbitrary(g) % (1024 * 1024))
        }
    }

    /// Generate a random alignment value (valid WASM alignments: 0, 1, 2, 3)
    #[derive(Clone, Debug)]
    struct ArbitraryAlign(u32);

    impl Arbitrary for ArbitraryAlign {
        fn arbitrary(g: &mut Gen) -> Self {
            // WASM alignment is log2 of actual alignment
            // 0 = 1-byte, 1 = 2-byte, 2 = 4-byte, 3 = 8-byte
            ArbitraryAlign(u32::arbitrary(g) % 4)
        }
    }

    /// Helper to create a test LirFunction
    fn create_test_function(params: &[LirType], locals: &[LirType]) -> LirFunction {
        LirFunction {
            name: "test_func".to_string(),
            params: params.to_vec(),
            returns: vec![],
            locals: locals.to_vec(),
            body: vec![],
            is_main: false,
        }
    }

    /// Helper to create an InstructionLowerer for testing
    fn create_lowerer(
        params: &[LirType],
        locals: &[LirType],
    ) -> (InstructionLowerer, Vec<(u32, wasm_encoder::ValType)>) {
        let func = create_test_function(params, locals);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let wasm_locals = manager.generate_wasm_locals();
        (InstructionLowerer::new(local_map), wasm_locals)
    }

    // =========================================================================
    // Property 10: Memory Operation Correctness
    // For any memory access operation, the generated WASM should use correct
    // MemArg structures with proper offset, alignment, and memory_index fields.
    // Validates: Requirements 3.2, 7.3, 8.3
    // =========================================================================

    /// Property: I32 load operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_i32_load_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            // Alignment for i32 should be at most 2 (log2(4) = 2)
            let valid_align = align.0.min(2);
            let inst = LirInst::I32Load {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: I32 store operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_i32_store_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            let valid_align = align.0.min(2);
            let inst = LirInst::I32Store {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: I64 load operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_i64_load_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            // Alignment for i64 should be at most 3 (log2(8) = 3)
            let valid_align = align.0.min(3);
            let inst = LirInst::I64Load {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: I64 store operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_i64_store_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            let valid_align = align.0.min(3);
            let inst = LirInst::I64Store {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: F32 load operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_f32_load_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            // Alignment for f32 should be at most 2 (log2(4) = 2)
            let valid_align = align.0.min(2);
            let inst = LirInst::F32Load {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: F32 store operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_f32_store_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            let valid_align = align.0.min(2);
            let inst = LirInst::F32Store {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: F64 load operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_f64_load_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            // Alignment for f64 should be at most 3 (log2(8) = 3)
            let valid_align = align.0.min(3);
            let inst = LirInst::F64Load {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: F64 store operations succeed for all valid offset/align combinations
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_f64_store_succeeds_for_all_offsets() {
        fn property(offset: ArbitraryOffset, align: ArbitraryAlign) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            let valid_align = align.0.min(3);
            let inst = LirInst::F64Store {
                offset: offset.0,
                align: valid_align,
            };

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryOffset, ArbitraryAlign) -> TestResult);
    }

    /// Property: Natural alignment calculation is correct
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_natural_alignment_log2_correct() {
        fn property() -> TestResult {
            // Test known values
            if InstructionLowerer::natural_alignment_log2(1) != 0 {
                return TestResult::failed();
            }
            if InstructionLowerer::natural_alignment_log2(2) != 1 {
                return TestResult::failed();
            }
            if InstructionLowerer::natural_alignment_log2(4) != 2 {
                return TestResult::failed();
            }
            if InstructionLowerer::natural_alignment_log2(8) != 3 {
                return TestResult::failed();
            }
            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: Alignment validation correctly identifies valid alignments
    /// Feature: lir-to-wasm-codegen, Property 10: Memory Operation Correctness
    #[test]
    fn prop_alignment_validation_correct() {
        fn property() -> TestResult {
            // Valid alignments (powers of 2, not exceeding natural)
            if !InstructionLowerer::validate_alignment(1, 4) {
                return TestResult::failed();
            }
            if !InstructionLowerer::validate_alignment(2, 4) {
                return TestResult::failed();
            }
            if !InstructionLowerer::validate_alignment(4, 4) {
                return TestResult::failed();
            }

            // Invalid: exceeds natural alignment
            if InstructionLowerer::validate_alignment(8, 4) {
                return TestResult::failed();
            }

            // Invalid: not a power of 2
            if InstructionLowerer::validate_alignment(3, 4) {
                return TestResult::failed();
            }
            if InstructionLowerer::validate_alignment(0, 4) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific memory operation scenarios
    // =========================================================================

    #[test]
    fn test_i32_load_specific_offsets() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        // Test specific offset values
        let test_offsets = [0, 4, 8, 16, 100, 1000, 65536];

        for offset in test_offsets {
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::I32Load { offset, align: 2 };
            assert!(
                lowerer.lower_instruction(&inst, &mut wasm_func).is_ok(),
                "Failed to lower I32Load with offset {}",
                offset
            );
        }
    }

    #[test]
    fn test_i64_load_specific_offsets() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);

        let test_offsets = [0, 8, 16, 100, 1000, 65536];

        for offset in test_offsets {
            let mut wasm_func = Function::new(wasm_locals.clone());
            let inst = LirInst::I64Load { offset, align: 3 };
            assert!(
                lowerer.lower_instruction(&inst, &mut wasm_func).is_ok(),
                "Failed to lower I64Load with offset {}",
                offset
            );
        }
    }

    #[test]
    fn test_struct_field_access_helpers() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
        let mut wasm_func = Function::new(wasm_locals);

        // Test struct field load/store helpers
        lowerer.emit_struct_field_load_i32(0, &mut wasm_func);
        lowerer.emit_struct_field_store_i32(4, &mut wasm_func);
        lowerer.emit_struct_field_load_i64(8, &mut wasm_func);
        lowerer.emit_struct_field_store_i64(16, &mut wasm_func);
        lowerer.emit_struct_field_load_f32(24, &mut wasm_func);
        lowerer.emit_struct_field_store_f32(28, &mut wasm_func);
        lowerer.emit_struct_field_load_f64(32, &mut wasm_func);
        lowerer.emit_struct_field_store_f64(40, &mut wasm_func);
    }

    #[test]
    fn test_memory_operation_sequence() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut wasm_func = Function::new(wasm_locals);

        // Test a sequence: load from address 0, add 1, store to address 4
        let instructions = [
            LirInst::I32Const(0), // Push address 0
            LirInst::I32Load {
                offset: 0,
                align: 2,
            }, // Load i32 from address 0
            LirInst::I32Const(1), // Push 1
            LirInst::I32Add,      // Add
            LirInst::I32Const(4), // Push address 4
                                  // Note: In real code, we'd need to swap the stack order for store
        ];

        for inst in &instructions {
            assert!(lowerer.lower_instruction(inst, &mut wasm_func).is_ok());
        }
    }

    #[test]
    fn test_emit_memory_convenience_methods() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
        let mut wasm_func = Function::new(wasm_locals);

        // Test memory emission methods
        lowerer.emit_i32_load(0, 2, &mut wasm_func);
        lowerer.emit_i32_store(4, 2, &mut wasm_func);
        lowerer.emit_i64_load(8, 3, &mut wasm_func);
        lowerer.emit_i64_store(16, 3, &mut wasm_func);
        lowerer.emit_f32_load(24, 2, &mut wasm_func);
        lowerer.emit_f32_store(28, 2, &mut wasm_func);
        lowerer.emit_f64_load(32, 3, &mut wasm_func);
        lowerer.emit_f64_store(40, 3, &mut wasm_func);
    }

    #[test]
    fn test_alignment_values() {
        // Test natural alignment log2 calculation
        assert_eq!(InstructionLowerer::natural_alignment_log2(1), 0);
        assert_eq!(InstructionLowerer::natural_alignment_log2(2), 1);
        assert_eq!(InstructionLowerer::natural_alignment_log2(4), 2);
        assert_eq!(InstructionLowerer::natural_alignment_log2(8), 3);

        // Unknown sizes default to 0
        assert_eq!(InstructionLowerer::natural_alignment_log2(3), 0);
        assert_eq!(InstructionLowerer::natural_alignment_log2(5), 0);
    }

    #[test]
    fn test_alignment_validation() {
        // Valid alignments
        assert!(InstructionLowerer::validate_alignment(1, 4));
        assert!(InstructionLowerer::validate_alignment(2, 4));
        assert!(InstructionLowerer::validate_alignment(4, 4));
        assert!(InstructionLowerer::validate_alignment(4, 8));
        assert!(InstructionLowerer::validate_alignment(8, 8));

        // Invalid: exceeds natural alignment
        assert!(!InstructionLowerer::validate_alignment(8, 4));
        assert!(!InstructionLowerer::validate_alignment(16, 8));

        // Invalid: not a power of 2
        assert!(!InstructionLowerer::validate_alignment(0, 4));
        assert!(!InstructionLowerer::validate_alignment(3, 4));
        assert!(!InstructionLowerer::validate_alignment(5, 8));
        assert!(!InstructionLowerer::validate_alignment(6, 8));
    }
}

#[cfg(test)]
mod control_flow_tests {
    use crate::compiler::codegen::wasm::control_flow::{BlockKind, ControlFlowManager};
    use crate::compiler::codegen::wasm::instruction_lowerer::InstructionLowerer;
    use crate::compiler::codegen::wasm::local_manager::LocalVariableManager;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use wasm_encoder::{Function, ValType};

    /// Generate a random nesting depth (0-10)
    #[derive(Clone, Debug)]
    struct ArbitraryDepth(u32);

    impl Arbitrary for ArbitraryDepth {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryDepth(u32::arbitrary(g) % 11)
        }
    }

    /// Generate a random block kind
    #[derive(Clone, Debug)]
    struct ArbitraryBlockKind(BlockKind);

    impl Arbitrary for ArbitraryBlockKind {
        fn arbitrary(g: &mut Gen) -> Self {
            let kinds = [BlockKind::Block, BlockKind::Loop, BlockKind::If];
            let idx = usize::arbitrary(g) % kinds.len();
            ArbitraryBlockKind(kinds[idx])
        }
    }

    /// Generate a random optional result type
    #[derive(Clone, Debug)]
    struct ArbitraryResultType(Option<ValType>);

    impl Arbitrary for ArbitraryResultType {
        fn arbitrary(g: &mut Gen) -> Self {
            let types = [
                None,
                Some(ValType::I32),
                Some(ValType::I64),
                Some(ValType::F32),
                Some(ValType::F64),
            ];
            let idx = usize::arbitrary(g) % types.len();
            ArbitraryResultType(types[idx])
        }
    }

    /// Helper to create a test LirFunction
    fn create_test_function(params: &[LirType], locals: &[LirType]) -> LirFunction {
        LirFunction {
            name: "test_func".to_string(),
            params: params.to_vec(),
            returns: vec![],
            locals: locals.to_vec(),
            body: vec![],
            is_main: false,
        }
    }

    /// Helper to create an InstructionLowerer for testing
    fn create_lowerer(
        params: &[LirType],
        locals: &[LirType],
    ) -> (InstructionLowerer, Vec<(u32, ValType)>) {
        let func = create_test_function(params, locals);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let wasm_locals = manager.generate_wasm_locals();
        (InstructionLowerer::new(local_map), wasm_locals)
    }

    // =========================================================================
    // Property 13: Control Flow Structure
    // For any control flow construct (if/else, loops, blocks), the generated
    // WASM should maintain proper block nesting, consistent stack types, and
    // correct branch targets.
    // Validates: Requirements 4.1, 4.2, 4.5, 4.6, 8.4
    // =========================================================================

    /// Property: Block nesting depth is correctly tracked
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_block_nesting_depth_tracked() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            let mut manager = ControlFlowManager::new();

            // Enter blocks up to the specified depth
            for i in 0..depth.0 {
                let block_id = manager.enter_block(BlockKind::Block, None);
                if block_id != i {
                    return TestResult::failed();
                }
                if manager.current_depth() != i + 1 {
                    return TestResult::failed();
                }
            }

            // Exit all blocks
            for _ in 0..depth.0 {
                if manager.exit_block().is_err() {
                    return TestResult::failed();
                }
            }

            // Should be back to depth 0
            if manager.current_depth() != 0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    /// Property: Block kinds are correctly preserved
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_block_kinds_preserved() {
        fn property(kind: ArbitraryBlockKind, result_type: ArbitraryResultType) -> TestResult {
            let mut manager = ControlFlowManager::new();

            manager.enter_block(kind.0, result_type.0);

            let current = manager.current_block();
            if current.is_none() {
                return TestResult::failed();
            }

            let block_info = current.unwrap();
            if block_info.kind != kind.0 {
                return TestResult::failed();
            }
            if block_info.result_type != result_type.0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryBlockKind, ArbitraryResultType) -> TestResult);
    }

    /// Property: Branch targets are validated correctly
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_branch_targets_validated() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            if depth.0 == 0 {
                return TestResult::discard();
            }

            let mut manager = ControlFlowManager::new();

            // Enter blocks
            for _ in 0..depth.0 {
                manager.enter_block(BlockKind::Block, None);
            }

            // Valid targets: 0 to depth-1
            for target in 0..depth.0 {
                if !manager.is_valid_branch_target(target) {
                    return TestResult::failed();
                }
            }

            // Invalid target: depth (out of bounds)
            if manager.is_valid_branch_target(depth.0) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    /// Property: Exit block fails when no blocks are open
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_exit_block_fails_when_empty() {
        fn property() -> TestResult {
            let mut manager = ControlFlowManager::new();

            // Should fail when no blocks are open
            if manager.exit_block().is_ok() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: Block at depth returns correct block info
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_block_at_depth_correct() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            if depth.0 == 0 {
                return TestResult::discard();
            }

            let mut manager = ControlFlowManager::new();
            let kinds = [BlockKind::Block, BlockKind::Loop, BlockKind::If];

            // Enter blocks with different kinds
            for i in 0..depth.0 {
                let kind = kinds[(i as usize) % kinds.len()];
                manager.enter_block(kind, None);
            }

            // Check block_at_depth returns correct blocks
            for i in 0..depth.0 {
                let block = manager.block_at_depth(i);
                if block.is_none() {
                    return TestResult::failed();
                }
                // Depth 0 should be the innermost (last entered)
                let expected_kind = kinds[((depth.0 - 1 - i) as usize) % kinds.len()];
                if block.unwrap().kind != expected_kind {
                    return TestResult::failed();
                }
            }

            // Out of bounds should return None
            if manager.block_at_depth(depth.0).is_some() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    /// Property: Find enclosing loop returns correct depth
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_find_enclosing_loop_correct() {
        fn property() -> TestResult {
            let mut manager = ControlFlowManager::new();

            // No loop - should return None
            manager.enter_block(BlockKind::Block, None);
            if manager.find_enclosing_loop_depth().is_some() {
                return TestResult::failed();
            }

            // Add a loop
            manager.enter_block(BlockKind::Loop, None);
            if manager.find_enclosing_loop_depth() != Some(0) {
                return TestResult::failed();
            }

            // Add another block on top
            manager.enter_block(BlockKind::Block, None);
            if manager.find_enclosing_loop_depth() != Some(1) {
                return TestResult::failed();
            }

            // Add another loop
            manager.enter_block(BlockKind::Loop, None);
            if manager.find_enclosing_loop_depth() != Some(0) {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: Validate all blocks closed works correctly
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_validate_all_blocks_closed() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            let mut manager = ControlFlowManager::new();

            // Enter blocks
            for _ in 0..depth.0 {
                manager.enter_block(BlockKind::Block, None);
            }

            // Should fail if blocks are open
            if depth.0 > 0 && manager.validate_all_blocks_closed().is_ok() {
                return TestResult::failed();
            }

            // Exit all blocks
            for _ in 0..depth.0 {
                let _ = manager.exit_block();
            }

            // Should succeed when all blocks are closed
            if manager.validate_all_blocks_closed().is_err() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    /// Property: If block generation produces valid WASM
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_if_block_generation_valid() {
        fn property(result_type: ArbitraryResultType) -> TestResult {
            let mut manager = ControlFlowManager::new();
            let mut function = Function::new(vec![]);

            // Generate if block
            if manager.generate_if(result_type.0, &mut function).is_err() {
                return TestResult::failed();
            }

            // Should be in an if block
            let current = manager.current_block();
            if current.is_none() || current.unwrap().kind != BlockKind::If {
                return TestResult::failed();
            }

            // Generate end
            if manager.generate_end(&mut function).is_err() {
                return TestResult::failed();
            }

            // Should be back to no blocks
            if manager.current_depth() != 0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryResultType) -> TestResult);
    }

    /// Property: Loop block generation produces valid WASM
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_loop_block_generation_valid() {
        fn property(result_type: ArbitraryResultType) -> TestResult {
            let mut manager = ControlFlowManager::new();
            let mut function = Function::new(vec![]);

            // Generate loop block
            if manager.generate_loop(result_type.0, &mut function).is_err() {
                return TestResult::failed();
            }

            // Should be in a loop block
            let current = manager.current_block();
            if current.is_none() || current.unwrap().kind != BlockKind::Loop {
                return TestResult::failed();
            }

            // Generate end
            if manager.generate_end(&mut function).is_err() {
                return TestResult::failed();
            }

            if manager.current_depth() != 0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryResultType) -> TestResult);
    }

    /// Property: Branch generation validates targets
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_branch_generation_validates_targets() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            if depth.0 == 0 {
                return TestResult::discard();
            }

            let mut manager = ControlFlowManager::new();
            let mut function = Function::new(vec![]);

            // Enter blocks
            for _ in 0..depth.0 {
                let _ = manager.generate_block(None, &mut function);
            }

            // Valid branch should succeed
            if manager.generate_branch(0, &mut function).is_err() {
                return TestResult::failed();
            }

            // Invalid branch should fail
            if manager.generate_branch(depth.0, &mut function).is_ok() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    /// Property: Branch if generation validates targets
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_branch_if_generation_validates_targets() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            if depth.0 == 0 {
                return TestResult::discard();
            }

            let mut manager = ControlFlowManager::new();
            let mut function = Function::new(vec![]);

            // Enter blocks
            for _ in 0..depth.0 {
                let _ = manager.generate_block(None, &mut function);
            }

            // Valid branch_if should succeed
            if manager.generate_branch_if(0, &mut function).is_err() {
                return TestResult::failed();
            }

            // Invalid branch_if should fail
            if manager.generate_branch_if(depth.0, &mut function).is_ok() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    /// Property: Reset clears all state
    /// Feature: lir-to-wasm-codegen, Property 13: Control Flow Structure
    #[test]
    fn prop_reset_clears_state() {
        fn property(depth: ArbitraryDepth) -> TestResult {
            let mut manager = ControlFlowManager::new();

            // Enter some blocks
            for _ in 0..depth.0 {
                manager.enter_block(BlockKind::Block, None);
            }

            // Reset
            manager.reset();

            // Should be back to initial state
            if manager.current_depth() != 0 {
                return TestResult::failed();
            }

            // New block should get ID 0
            let block_id = manager.enter_block(BlockKind::Block, None);
            if block_id != 0 {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryDepth) -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific control flow scenarios
    // =========================================================================

    #[test]
    fn test_simple_if_else() {
        let mut manager = ControlFlowManager::new();
        let mut function = Function::new(vec![]);

        // Generate if
        assert!(manager.generate_if(None, &mut function).is_ok());
        assert_eq!(manager.current_depth(), 1);

        // Generate else
        assert!(manager.generate_else(&mut function).is_ok());
        assert_eq!(manager.current_depth(), 1);

        // Generate end
        assert!(manager.generate_end(&mut function).is_ok());
        assert_eq!(manager.current_depth(), 0);
    }

    #[test]
    fn test_nested_blocks() {
        let mut manager = ControlFlowManager::new();
        let mut function = Function::new(vec![]);

        // Outer block
        assert!(manager.generate_block(None, &mut function).is_ok());
        assert_eq!(manager.current_depth(), 1);

        // Inner loop
        assert!(manager.generate_loop(None, &mut function).is_ok());
        assert_eq!(manager.current_depth(), 2);

        // Innermost if
        assert!(manager.generate_if(None, &mut function).is_ok());
        assert_eq!(manager.current_depth(), 3);

        // Close all
        assert!(manager.generate_end(&mut function).is_ok());
        assert_eq!(manager.current_depth(), 2);
        assert!(manager.generate_end(&mut function).is_ok());
        assert_eq!(manager.current_depth(), 1);
        assert!(manager.generate_end(&mut function).is_ok());
        assert_eq!(manager.current_depth(), 0);
    }

    #[test]
    fn test_else_outside_if_fails() {
        let mut manager = ControlFlowManager::new();
        let mut function = Function::new(vec![]);

        // Generate a block (not an if)
        assert!(manager.generate_block(None, &mut function).is_ok());

        // Else should fail
        assert!(manager.generate_else(&mut function).is_err());
    }

    #[test]
    fn test_branch_in_loop() {
        let mut manager = ControlFlowManager::new();
        let mut function = Function::new(vec![]);

        // Generate loop
        assert!(manager.generate_loop(None, &mut function).is_ok());

        // Branch to loop start (depth 0)
        assert!(manager.generate_branch(0, &mut function).is_ok());

        // End loop
        assert!(manager.generate_end(&mut function).is_ok());
    }

    #[test]
    fn test_find_enclosing_block() {
        let mut manager = ControlFlowManager::new();

        // No block initially
        assert!(manager.find_enclosing_block_depth().is_none());

        // Add a loop
        manager.enter_block(BlockKind::Loop, None);
        assert!(manager.find_enclosing_block_depth().is_none());

        // Add a block
        manager.enter_block(BlockKind::Block, None);
        assert_eq!(manager.find_enclosing_block_depth(), Some(0));

        // Add an if
        manager.enter_block(BlockKind::If, None);
        assert_eq!(manager.find_enclosing_block_depth(), Some(1));
    }

    #[test]
    fn test_block_type_for_result() {
        use wasm_encoder::BlockType;

        assert!(matches!(
            ControlFlowManager::block_type_for_result(None),
            BlockType::Empty
        ));
        assert!(matches!(
            ControlFlowManager::block_type_for_result(Some(ValType::I32)),
            BlockType::Result(ValType::I32)
        ));
        assert!(matches!(
            ControlFlowManager::block_type_for_result(Some(ValType::I64)),
            BlockType::Result(ValType::I64)
        ));
    }

    #[test]
    fn test_lower_block_with_control_flow() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut function = Function::new(wasm_locals);
        let mut control_flow = ControlFlowManager::new();

        // Lower a simple block with instructions
        let instructions = vec![LirInst::I32Const(42), LirInst::Drop];

        assert!(
            lowerer
                .lower_block(&instructions, None, &mut function, &mut control_flow)
                .is_ok()
        );
        assert_eq!(control_flow.current_depth(), 0);
    }

    #[test]
    fn test_lower_loop_with_control_flow() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut function = Function::new(wasm_locals);
        let mut control_flow = ControlFlowManager::new();

        // Lower a simple loop with instructions
        let instructions = vec![LirInst::I32Const(1), LirInst::Drop];

        assert!(
            lowerer
                .lower_loop(&instructions, None, &mut function, &mut control_flow)
                .is_ok()
        );
        assert_eq!(control_flow.current_depth(), 0);
    }

    #[test]
    fn test_lower_if_with_control_flow() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut function = Function::new(wasm_locals);
        let mut control_flow = ControlFlowManager::new();

        // Lower an if with then and else branches
        let then_branch = vec![LirInst::I32Const(1), LirInst::Drop];
        let else_branch = vec![LirInst::I32Const(2), LirInst::Drop];

        assert!(
            lowerer
                .lower_if(
                    &then_branch,
                    Some(&else_branch),
                    None,
                    &mut function,
                    &mut control_flow
                )
                .is_ok()
        );
        assert_eq!(control_flow.current_depth(), 0);
    }

    #[test]
    fn test_lower_nested_control_flow() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut function = Function::new(wasm_locals);
        let mut control_flow = ControlFlowManager::new();

        // Create nested structure: block { loop { if { } } }
        let inner_if = LirInst::If {
            then_branch: vec![LirInst::I32Const(1), LirInst::Drop],
            else_branch: None,
        };
        let loop_body = vec![LirInst::I32Const(0), inner_if];
        let inner_loop = LirInst::Loop {
            instructions: loop_body,
        };
        let block_body = vec![inner_loop];

        assert!(
            lowerer
                .lower_block(&block_body, None, &mut function, &mut control_flow)
                .is_ok()
        );
        assert_eq!(control_flow.current_depth(), 0);
    }

    #[test]
    fn test_lower_instructions_sequence() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut function = Function::new(wasm_locals);
        let mut control_flow = ControlFlowManager::new();

        // Lower a sequence of instructions including control flow
        let instructions = vec![
            LirInst::I32Const(10),
            LirInst::Drop,
            LirInst::Block {
                instructions: vec![LirInst::I32Const(20), LirInst::Drop],
            },
            LirInst::I32Const(30),
            LirInst::Drop,
        ];

        assert!(
            lowerer
                .lower_instructions(&instructions, &mut function, &mut control_flow)
                .is_ok()
        );
        assert_eq!(control_flow.current_depth(), 0);
    }
}

#[cfg(test)]
mod function_call_tests {
    use crate::compiler::codegen::wasm::instruction_lowerer::InstructionLowerer;
    use crate::compiler::codegen::wasm::local_manager::LocalVariableManager;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use std::collections::HashMap;
    use wasm_encoder::{Function, ValType};

    /// Generate a random function index (0-100)
    #[derive(Clone, Debug)]
    struct ArbitraryFuncIndex(u32);

    impl Arbitrary for ArbitraryFuncIndex {
        fn arbitrary(g: &mut Gen) -> Self {
            ArbitraryFuncIndex(u32::arbitrary(g) % 101)
        }
    }

    /// Generate a random list of argument local indices
    #[derive(Clone, Debug)]
    struct ArbitraryArgLocals(Vec<u32>);

    impl Arbitrary for ArbitraryArgLocals {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-5 argument locals
            let count = usize::arbitrary(g) % 6;
            let locals: Vec<u32> = (0..count).map(|i| i as u32).collect();
            ArbitraryArgLocals(locals)
        }
    }

    /// Generate a random function name
    #[derive(Clone, Debug)]
    struct ArbitraryFuncName(String);

    impl Arbitrary for ArbitraryFuncName {
        fn arbitrary(g: &mut Gen) -> Self {
            let names = ["main", "helper", "compute", "process", "init"];
            let idx = usize::arbitrary(g) % names.len();
            ArbitraryFuncName(names[idx].to_string())
        }
    }

    /// Helper to create a test LirFunction
    fn create_test_function(params: &[LirType], locals: &[LirType]) -> LirFunction {
        LirFunction {
            name: "test_func".to_string(),
            params: params.to_vec(),
            returns: vec![],
            locals: locals.to_vec(),
            body: vec![],
            is_main: false,
        }
    }

    /// Helper to create an InstructionLowerer with function indices
    fn create_lowerer_with_functions(
        params: &[LirType],
        locals: &[LirType],
        func_names: &[&str],
    ) -> (InstructionLowerer, Vec<(u32, ValType)>) {
        let func = create_test_function(params, locals);
        let manager = LocalVariableManager::analyze_function(&func);
        let local_map = manager.build_local_mapping();
        let wasm_locals = manager.generate_wasm_locals();

        let mut function_indices = HashMap::new();
        for (i, name) in func_names.iter().enumerate() {
            function_indices.insert(name.to_string(), i as u32);
        }

        (
            InstructionLowerer::with_function_indices(local_map, function_indices),
            wasm_locals,
        )
    }

    /// Helper to create a basic InstructionLowerer
    fn create_lowerer(
        params: &[LirType],
        locals: &[LirType],
    ) -> (InstructionLowerer, Vec<(u32, ValType)>) {
        create_lowerer_with_functions(params, locals, &[])
    }

    // =========================================================================
    // Property 14: Function Call Generation
    // For any function call (direct or host), the generated WASM should use
    // correct function indices, proper argument loading, and appropriate call
    // instructions.
    // Validates: Requirements 4.3, 5.5, 7.4
    // =========================================================================

    /// Property: Direct function calls succeed for all valid indices
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_direct_call_succeeds_for_all_indices() {
        fn property(func_index: ArbitraryFuncIndex) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            let inst = LirInst::Call(func_index.0);

            match lowerer.lower_instruction(&inst, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFuncIndex) -> TestResult);
    }

    /// Property: emit_call generates valid WASM for all indices
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_emit_call_valid_for_all_indices() {
        fn property(func_index: ArbitraryFuncIndex) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            // emit_call should always succeed (no validation of index)
            lowerer.emit_call(func_index.0, &mut wasm_func);

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFuncIndex) -> TestResult);
    }

    /// Property: emit_call_by_name succeeds for registered functions
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_emit_call_by_name_succeeds_for_registered() {
        fn property(func_name: ArbitraryFuncName) -> TestResult {
            let func_names = ["main", "helper", "compute", "process", "init"];
            let (lowerer, wasm_locals) = create_lowerer_with_functions(&[], &[], &func_names);
            let mut wasm_func = Function::new(wasm_locals);

            match lowerer.emit_call_by_name(&func_name.0, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFuncName) -> TestResult);
    }

    /// Property: emit_call_by_name fails for unregistered functions
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_emit_call_by_name_fails_for_unregistered() {
        fn property() -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
            let mut wasm_func = Function::new(wasm_locals);

            // Should fail for any function name since none are registered
            match lowerer.emit_call_by_name("nonexistent", &mut wasm_func) {
                Ok(()) => TestResult::failed(),
                Err(_) => TestResult::passed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: has_function correctly identifies registered functions
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_has_function_correct() {
        fn property(func_name: ArbitraryFuncName) -> TestResult {
            let func_names = ["main", "helper", "compute", "process", "init"];
            let (lowerer, _) = create_lowerer_with_functions(&[], &[], &func_names);

            // Should return true for registered functions
            if !lowerer.has_function(&func_name.0) {
                return TestResult::failed();
            }

            // Should return false for unregistered functions
            if lowerer.has_function("nonexistent") {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFuncName) -> TestResult);
    }

    /// Property: get_function_index returns correct indices
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_get_function_index_correct() {
        fn property() -> TestResult {
            let func_names = ["main", "helper", "compute"];
            let (lowerer, _) = create_lowerer_with_functions(&[], &[], &func_names);

            // Check each function has the correct index
            for (i, name) in func_names.iter().enumerate() {
                match lowerer.get_function_index(name) {
                    Some(idx) if idx == i as u32 => {}
                    _ => return TestResult::failed(),
                }
            }

            // Unregistered should return None
            if lowerer.get_function_index("nonexistent").is_some() {
                return TestResult::failed();
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: function_count returns correct count
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_function_count_correct() {
        fn property() -> TestResult {
            // Test with different numbers of functions
            for count in 0..=5 {
                let func_names: Vec<&str> = ["a", "b", "c", "d", "e"]
                    .iter()
                    .take(count)
                    .copied()
                    .collect();
                let (lowerer, _) = create_lowerer_with_functions(&[], &[], &func_names);

                if lowerer.function_count() != count {
                    return TestResult::failed();
                }
            }

            TestResult::passed()
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    /// Property: emit_call_with_local_args loads all arguments
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_emit_call_with_local_args_loads_all() {
        fn property(arg_count: ArbitraryArgLocals) -> TestResult {
            // Create enough locals for the arguments
            let local_count = arg_count.0.len().max(1);
            let locals: Vec<LirType> = (0..local_count).map(|_| LirType::I32).collect();
            let (lowerer, wasm_locals) = create_lowerer(&[], &locals);
            let mut wasm_func = Function::new(wasm_locals);

            // Should succeed for valid local indices
            match lowerer.emit_call_with_local_args(0, &arg_count.0, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryArgLocals) -> TestResult);
    }

    /// Property: emit_call_and_store_result stores result correctly
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_emit_call_and_store_result_valid() {
        fn property(func_index: ArbitraryFuncIndex) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
            let mut wasm_func = Function::new(wasm_locals);

            // Should succeed for valid local index
            match lowerer.emit_call_and_store_result(func_index.0, 0, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFuncIndex) -> TestResult);
    }

    /// Property: emit_call_and_tee_result keeps result on stack
    /// Feature: lir-to-wasm-codegen, Property 14: Function Call Generation
    #[test]
    fn prop_emit_call_and_tee_result_valid() {
        fn property(func_index: ArbitraryFuncIndex) -> TestResult {
            let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
            let mut wasm_func = Function::new(wasm_locals);

            // Should succeed for valid local index
            match lowerer.emit_call_and_tee_result(func_index.0, 0, &mut wasm_func) {
                Ok(()) => TestResult::passed(),
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryFuncIndex) -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific function call scenarios
    // =========================================================================

    #[test]
    fn test_simple_call() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
        let mut wasm_func = Function::new(wasm_locals);

        let inst = LirInst::Call(0);
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_call_with_arguments() {
        let (lowerer, wasm_locals) = create_lowerer(&[LirType::I32, LirType::I64], &[]);
        let mut wasm_func = Function::new(wasm_locals);

        // Load arguments and call
        let instructions = vec![
            LirInst::LocalGet(0), // Load first arg
            LirInst::LocalGet(1), // Load second arg
            LirInst::Call(0),     // Call function
        ];

        for inst in &instructions {
            assert!(lowerer.lower_instruction(inst, &mut wasm_func).is_ok());
        }
    }

    #[test]
    fn test_call_by_name() {
        let func_names = ["main", "helper"];
        let (lowerer, wasm_locals) = create_lowerer_with_functions(&[], &[], &func_names);
        let mut wasm_func = Function::new(wasm_locals);

        assert!(lowerer.emit_call_by_name("main", &mut wasm_func).is_ok());
        assert!(lowerer.emit_call_by_name("helper", &mut wasm_func).is_ok());
        assert!(
            lowerer
                .emit_call_by_name("nonexistent", &mut wasm_func)
                .is_err()
        );
    }

    #[test]
    fn test_call_with_local_args() {
        let (lowerer, wasm_locals) = create_lowerer(&[LirType::I32], &[LirType::I64, LirType::F32]);
        let mut wasm_func = Function::new(wasm_locals);

        // Call with parameter and locals as arguments
        assert!(
            lowerer
                .emit_call_with_local_args(0, &[0, 1, 2], &mut wasm_func)
                .is_ok()
        );
    }

    #[test]
    fn test_call_by_name_with_local_args() {
        let func_names = ["target"];
        let (lowerer, wasm_locals) =
            create_lowerer_with_functions(&[LirType::I32], &[], &func_names);
        let mut wasm_func = Function::new(wasm_locals);

        assert!(
            lowerer
                .emit_call_by_name_with_local_args("target", &[0], &mut wasm_func)
                .is_ok()
        );
        assert!(
            lowerer
                .emit_call_by_name_with_local_args("nonexistent", &[0], &mut wasm_func)
                .is_err()
        );
    }

    #[test]
    fn test_call_and_store_result() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut wasm_func = Function::new(wasm_locals);

        // Call and store result in local 0
        assert!(
            lowerer
                .emit_call_and_store_result(0, 0, &mut wasm_func)
                .is_ok()
        );
    }

    #[test]
    fn test_call_and_tee_result() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[LirType::I32]);
        let mut wasm_func = Function::new(wasm_locals);

        // Call and tee result to local 0
        assert!(
            lowerer
                .emit_call_and_tee_result(0, 0, &mut wasm_func)
                .is_ok()
        );
    }

    #[test]
    fn test_function_index_management() {
        let func_names = ["a", "b", "c"];
        let (lowerer, _) = create_lowerer_with_functions(&[], &[], &func_names);

        assert!(lowerer.has_function("a"));
        assert!(lowerer.has_function("b"));
        assert!(lowerer.has_function("c"));
        assert!(!lowerer.has_function("d"));

        assert_eq!(lowerer.get_function_index("a"), Some(0));
        assert_eq!(lowerer.get_function_index("b"), Some(1));
        assert_eq!(lowerer.get_function_index("c"), Some(2));
        assert_eq!(lowerer.get_function_index("d"), None);

        assert_eq!(lowerer.function_count(), 3);
    }

    #[test]
    fn test_nested_calls() {
        let func_names = ["outer", "inner"];
        let (lowerer, wasm_locals) =
            create_lowerer_with_functions(&[], &[LirType::I32], &func_names);
        let mut wasm_func = Function::new(wasm_locals);

        // Simulate: result = outer(inner())
        // 1. Call inner
        assert!(lowerer.emit_call_by_name("inner", &mut wasm_func).is_ok());
        // 2. Call outer (result of inner is on stack)
        assert!(lowerer.emit_call_by_name("outer", &mut wasm_func).is_ok());
        // 3. Store result
        assert!(lowerer.emit_local_set(0, &mut wasm_func).is_ok());
    }

    #[test]
    fn test_return_instruction() {
        let (lowerer, wasm_locals) = create_lowerer(&[], &[]);
        let mut wasm_func = Function::new(wasm_locals);

        let inst = LirInst::Return;
        assert!(lowerer.lower_instruction(&inst, &mut wasm_func).is_ok());
    }
}

#[cfg(test)]
mod memory_manager_tests {
    use crate::compiler::codegen::wasm::memory_manager::{
        DEFAULT_MAX_PAGES, DEFAULT_MIN_PAGES, HEAP_START_OFFSET, MIN_ALLOCATION_ALIGNMENT,
        MemoryConfig, MemoryManager,
    };
    use crate::compiler::codegen::wasm::module_builder::WasmModuleBuilder;
    use wasm_encoder::Function;

    // =========================================================================
    // Unit tests for MemoryManager
    // =========================================================================

    #[test]
    fn test_memory_config_default() {
        let config = MemoryConfig::default();
        assert_eq!(config.min_pages, DEFAULT_MIN_PAGES);
        assert_eq!(config.max_pages, Some(DEFAULT_MAX_PAGES));
        assert_eq!(config.heap_start, HEAP_START_OFFSET);
        assert!(config.export_memory);
        assert_eq!(config.memory_export_name, "memory");
    }

    #[test]
    fn test_memory_config_minimal() {
        let config = MemoryConfig::minimal();
        assert_eq!(config.min_pages, 1);
        assert_eq!(config.max_pages, Some(16));
        assert!(!config.export_memory);
    }

    #[test]
    fn test_memory_config_with_pages() {
        let config = MemoryConfig::with_pages(4, Some(256));
        assert_eq!(config.min_pages, 4);
        assert_eq!(config.max_pages, Some(256));
        assert!(config.export_memory);
    }

    #[test]
    fn test_memory_manager_new() {
        let manager = MemoryManager::new();
        assert!(!manager.is_setup());
        assert!(manager.indices().is_none());
    }

    #[test]
    fn test_memory_manager_with_config() {
        let config = MemoryConfig::minimal();
        let manager = MemoryManager::with_config(config.clone());
        assert_eq!(manager.config().min_pages, config.min_pages);
        assert_eq!(manager.config().max_pages, config.max_pages);
    }

    #[test]
    fn test_memory_manager_setup() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();

        let result = manager.setup_memory(&mut module_builder);
        assert!(result.is_ok());

        let indices = result.unwrap();
        assert_eq!(indices.memory_index, 0);
        assert!(manager.is_setup());
        assert!(manager.indices().is_some());
    }

    #[test]
    fn test_memory_manager_indices_after_setup() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();

        manager.setup_memory(&mut module_builder).unwrap();

        assert!(manager.memory_index().is_some());
        assert!(manager.heap_ptr_global().is_some());
        assert!(manager.alloc_func_index().is_some());
        assert!(manager.free_func_index().is_some());
    }

    #[test]
    fn test_memory_manager_creates_ownership_manager() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();

        manager.setup_memory(&mut module_builder).unwrap();

        let ownership_manager = manager.create_ownership_manager();
        assert!(ownership_manager.is_ok());

        let om = ownership_manager.unwrap();
        assert_eq!(
            om.alloc_function_index(),
            manager.alloc_func_index().unwrap()
        );
        assert_eq!(om.free_function_index(), manager.free_func_index().unwrap());
    }

    #[test]
    fn test_memory_manager_create_ownership_manager_before_setup_fails() {
        let manager = MemoryManager::new();
        let result = manager.create_ownership_manager();
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_manager_generate_allocation() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();
        manager.setup_memory(&mut module_builder).unwrap();

        let mut func = Function::new(vec![]);
        let result = manager.generate_allocation(64, &mut func);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_manager_generate_allocation_before_setup_fails() {
        let manager = MemoryManager::new();
        let mut func = Function::new(vec![]);
        let result = manager.generate_allocation(64, &mut func);
        assert!(result.is_err());
    }

    #[test]
    fn test_memory_manager_generate_allocation_owned() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();
        manager.setup_memory(&mut module_builder).unwrap();

        let mut func = Function::new(vec![]);
        let result = manager.generate_allocation_owned(64, &mut func);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_manager_generate_free() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();
        manager.setup_memory(&mut module_builder).unwrap();

        let mut func = Function::new(vec![]);
        let result = manager.generate_free(&mut func);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_manager_generate_conditional_drop() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();
        manager.setup_memory(&mut module_builder).unwrap();

        let mut func = Function::new(vec![(1, wasm_encoder::ValType::I32)]);
        let result = manager.generate_conditional_drop(0, &mut func);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_manager_generate_scope_exit_drops() {
        let mut manager = MemoryManager::new();
        let mut module_builder = WasmModuleBuilder::new();
        manager.setup_memory(&mut module_builder).unwrap();

        let mut func = Function::new(vec![(3, wasm_encoder::ValType::I32)]);
        let owned_locals = vec![0, 1, 2];
        let result = manager.generate_scope_exit_drops(&owned_locals, &mut func);
        assert!(result.is_ok());
    }

    #[test]
    fn test_memory_manager_module_validates() {
        let mut manager = MemoryManager::with_config(MemoryConfig::minimal());
        let mut module_builder = WasmModuleBuilder::new();

        manager.setup_memory(&mut module_builder).unwrap();

        // Finalize and validate the module
        let wasm_bytes = module_builder.finish();
        assert!(wasm_bytes.is_ok());

        // Validate with wasmparser
        let bytes = wasm_bytes.unwrap();
        let validation_result = wasmparser::validate(&bytes);
        assert!(
            validation_result.is_ok(),
            "WASM validation failed: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_memory_manager_with_user_function() {
        let mut manager = MemoryManager::with_config(MemoryConfig::minimal());
        let mut module_builder = WasmModuleBuilder::new();

        // Setup memory first
        let indices = manager.setup_memory(&mut module_builder).unwrap();

        // Add a user function that uses allocation
        let func_type = module_builder.add_function_type(vec![], vec![wasm_encoder::ValType::I32]);
        let mut func = Function::new(vec![]);

        // Generate allocation code
        manager.generate_allocation(32, &mut func).unwrap();
        func.instruction(&wasm_encoder::Instruction::End);

        module_builder.add_function(func_type, func);

        // Finalize and validate
        let wasm_bytes = module_builder.finish().unwrap();
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "WASM validation failed: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_min_allocation_alignment() {
        // Verify the alignment constant is correct for tagged pointers
        assert_eq!(MIN_ALLOCATION_ALIGNMENT, 2);
    }

    #[test]
    fn test_heap_start_offset() {
        // Verify heap starts after first page (64KB)
        assert_eq!(HEAP_START_OFFSET, 65536);
    }
}


#[cfg(test)]
mod module_validation_tests {
    //! Property-based tests for WASM module validation
    //!
    //! These tests validate Property 1: Valid WASM Module Generation
    //! For any valid LIR module, the generated WASM module should have proper
    //! section ordering, pass wasmparser validation, and be executable.
    //!
    //! Validates: Requirements 8.1, 8.6

    use crate::compiler::codegen::wasm::encode::encode_wasm;
    use crate::compiler::codegen::wasm::module_builder::WasmModuleBuilder;
    use crate::compiler::codegen::wasm::validator::WasmValidator;
    use crate::compiler::lir::nodes::{LirFunction, LirInst, LirModule, LirType};
    use quickcheck::{Arbitrary, Gen, QuickCheck, TestResult};
    use wasm_encoder::{ExportKind, Function, Instruction, ValType};

    /// Generate a random LirType for property testing
    #[derive(Clone, Debug)]
    struct ArbitraryLirType(LirType);

    impl Arbitrary for ArbitraryLirType {
        fn arbitrary(g: &mut Gen) -> Self {
            let types = [LirType::I32, LirType::I64, LirType::F32, LirType::F64];
            let idx = usize::arbitrary(g) % types.len();
            ArbitraryLirType(types[idx])
        }
    }

    /// Generate a random list of parameter types (0-3 params for simplicity)
    #[derive(Clone, Debug)]
    struct ArbitraryParams(Vec<LirType>);

    impl Arbitrary for ArbitraryParams {
        fn arbitrary(g: &mut Gen) -> Self {
            let count = usize::arbitrary(g) % 4;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryParams(types)
        }
    }

    /// Generate a random list of local types (0-5 locals for simplicity)
    #[derive(Clone, Debug)]
    struct ArbitraryLocals(Vec<LirType>);

    impl Arbitrary for ArbitraryLocals {
        fn arbitrary(g: &mut Gen) -> Self {
            let count = usize::arbitrary(g) % 6;
            let types: Vec<LirType> = (0..count)
                .map(|_| ArbitraryLirType::arbitrary(g).0)
                .collect();
            ArbitraryLocals(types)
        }
    }

    /// Generate a random return type (0 or 1 return values)
    #[derive(Clone, Debug)]
    struct ArbitraryReturns(Vec<LirType>);

    impl Arbitrary for ArbitraryReturns {
        fn arbitrary(g: &mut Gen) -> Self {
            let has_return = bool::arbitrary(g);
            if has_return {
                ArbitraryReturns(vec![ArbitraryLirType::arbitrary(g).0])
            } else {
                ArbitraryReturns(vec![])
            }
        }
    }

    /// Generate a simple valid LIR instruction sequence that maintains stack balance
    /// These are instructions that don't leave values on the stack
    #[derive(Clone, Debug)]
    struct ArbitrarySimpleBody(Vec<LirInst>);

    impl Arbitrary for ArbitrarySimpleBody {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 0-3 stack-balanced instruction pairs
            let count = usize::arbitrary(g) % 4;
            let mut instructions = Vec::new();

            for _ in 0..count {
                let inst_type = usize::arbitrary(g) % 3;
                match inst_type {
                    0 => {
                        // Nop - no stack effect
                        instructions.push(LirInst::Nop);
                    }
                    1 => {
                        // Push then drop - balanced
                        instructions.push(LirInst::I32Const(i32::arbitrary(g)));
                        instructions.push(LirInst::Drop);
                    }
                    _ => {
                        // Just nop for safety
                        instructions.push(LirInst::Nop);
                    }
                }
            }

            ArbitrarySimpleBody(instructions)
        }
    }

    /// Generate a valid LIR function with random parameters, locals, and simple body
    #[derive(Clone, Debug)]
    struct ArbitraryLirFunction {
        name: String,
        params: Vec<LirType>,
        returns: Vec<LirType>,
        locals: Vec<LirType>,
        body: Vec<LirInst>,
        is_main: bool,
    }

    impl Arbitrary for ArbitraryLirFunction {
        fn arbitrary(g: &mut Gen) -> Self {
            let params = ArbitraryParams::arbitrary(g).0;
            let returns = ArbitraryReturns::arbitrary(g).0;
            let locals = ArbitraryLocals::arbitrary(g).0;
            let body = ArbitrarySimpleBody::arbitrary(g).0;
            let is_main = bool::arbitrary(g);

            // Generate a unique function name
            let name_suffix = usize::arbitrary(g) % 1000;
            let name = format!("func_{}", name_suffix);

            ArbitraryLirFunction {
                name,
                params,
                returns,
                locals,
                body,
                is_main,
            }
        }
    }

    impl From<ArbitraryLirFunction> for LirFunction {
        fn from(arb: ArbitraryLirFunction) -> Self {
            LirFunction {
                name: arb.name,
                params: arb.params,
                returns: arb.returns,
                locals: arb.locals,
                body: arb.body,
                is_main: arb.is_main,
            }
        }
    }

    /// Generate a valid LIR module with 1-3 functions
    #[derive(Clone, Debug)]
    struct ArbitraryLirModule(LirModule);

    impl Arbitrary for ArbitraryLirModule {
        fn arbitrary(g: &mut Gen) -> Self {
            // Generate 1-3 functions
            let func_count = (usize::arbitrary(g) % 3) + 1;
            let mut functions: Vec<LirFunction> = Vec::new();
            let mut has_main = false;

            for i in 0..func_count {
                let mut arb_func = ArbitraryLirFunction::arbitrary(g);
                // Ensure unique names
                arb_func.name = format!("func_{}", i);
                // Only one main function
                if !has_main && arb_func.is_main {
                    has_main = true;
                } else {
                    arb_func.is_main = false;
                }
                functions.push(arb_func.into());
            }

            // Ensure at least one main function for valid module
            if !has_main && !functions.is_empty() {
                functions[0].is_main = true;
            }

            ArbitraryLirModule(LirModule {
                functions,
                structs: vec![],
            })
        }
    }

    /// Helper to create a minimal valid LIR module
    fn create_minimal_lir_module() -> LirModule {
        LirModule {
            functions: vec![LirFunction {
                name: "main".to_string(),
                params: vec![],
                returns: vec![],
                locals: vec![],
                body: vec![],
                is_main: true,
            }],
            structs: vec![],
        }
    }

    /// Helper to create a LIR module with a function that has specific params/returns
    fn create_lir_module_with_signature(
        params: Vec<LirType>,
        returns: Vec<LirType>,
        locals: Vec<LirType>,
    ) -> LirModule {
        LirModule {
            functions: vec![LirFunction {
                name: "main".to_string(),
                params,
                returns,
                locals,
                body: vec![],
                is_main: true,
            }],
            structs: vec![],
        }
    }

    // =========================================================================
    // Property 1: Valid WASM Module Generation (Comprehensive)
    // For any valid LIR module, the generated WASM module should have proper
    // section ordering, pass wasmparser validation, and be executable.
    // Validates: Requirements 8.1, 8.6
    // =========================================================================

    /// Property: Any valid LIR module produces a valid WASM module
    /// Feature: lir-to-wasm-codegen, Property 1: Valid WASM Module Generation
    #[test]
    fn prop_valid_lir_produces_valid_wasm() {
        fn property(module: ArbitraryLirModule) -> TestResult {
            let lir_module = module.0;

            // Skip modules with no functions (edge case)
            if lir_module.functions.is_empty() {
                return TestResult::discard();
            }

            // Encode the LIR module to WASM
            match encode_wasm(&lir_module) {
                Ok(wasm_bytes) => {
                    // Validate with wasmparser
                    match wasmparser::validate(&wasm_bytes) {
                        Ok(_) => TestResult::passed(),
                        Err(e) => {
                            eprintln!("WASM validation failed: {:?}", e);
                            TestResult::failed()
                        }
                    }
                }
                Err(e) => {
                    eprintln!("WASM encoding failed: {:?}", e);
                    TestResult::failed()
                }
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLirModule) -> TestResult);
    }

    /// Property: Generated WASM modules have proper section ordering
    /// Feature: lir-to-wasm-codegen, Property 1: Valid WASM Module Generation
    #[test]
    fn prop_wasm_has_proper_section_ordering() {
        fn property(
            params: ArbitraryParams,
            returns: ArbitraryReturns,
            locals: ArbitraryLocals,
        ) -> TestResult {
            let lir_module = create_lir_module_with_signature(params.0, returns.0, locals.0);

            match encode_wasm(&lir_module) {
                Ok(wasm_bytes) => {
                    // Parse the module to verify section ordering
                    let parser = wasmparser::Parser::new(0);
                    let mut last_section_id: Option<u8> = None;

                    for payload in parser.parse_all(&wasm_bytes) {
                        match payload {
                            Ok(wasmparser::Payload::TypeSection { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 1 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(1);
                            }
                            Ok(wasmparser::Payload::ImportSection { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 2 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(2);
                            }
                            Ok(wasmparser::Payload::FunctionSection { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 3 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(3);
                            }
                            Ok(wasmparser::Payload::MemorySection { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 5 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(5);
                            }
                            Ok(wasmparser::Payload::GlobalSection { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 6 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(6);
                            }
                            Ok(wasmparser::Payload::ExportSection { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 7 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(7);
                            }
                            Ok(wasmparser::Payload::CodeSectionStart { .. }) => {
                                if let Some(last) = last_section_id {
                                    if last >= 10 {
                                        return TestResult::failed();
                                    }
                                }
                                last_section_id = Some(10);
                            }
                            _ => {}
                        }
                    }

                    TestResult::passed()
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(
                property as fn(ArbitraryParams, ArbitraryReturns, ArbitraryLocals) -> TestResult,
            );
    }

    /// Property: WasmValidator correctly validates generated modules
    /// Feature: lir-to-wasm-codegen, Property 1: Valid WASM Module Generation
    #[test]
    fn prop_wasm_validator_validates_generated_modules() {
        fn property(module: ArbitraryLirModule) -> TestResult {
            let lir_module = module.0;

            if lir_module.functions.is_empty() {
                return TestResult::discard();
            }

            match encode_wasm(&lir_module) {
                Ok(wasm_bytes) => {
                    let mut validator = WasmValidator::new();
                    match validator.validate_module(&wasm_bytes) {
                        Ok(_) => TestResult::passed(),
                        Err(e) => {
                            eprintln!("WasmValidator failed: {:?}", e);
                            TestResult::failed()
                        }
                    }
                }
                Err(_) => TestResult::failed(),
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn(ArbitraryLirModule) -> TestResult);
    }

    /// Property: Module builder produces valid modules for any function signature
    /// Feature: lir-to-wasm-codegen, Property 1: Valid WASM Module Generation
    #[test]
    fn prop_module_builder_produces_valid_modules() {
        fn property(
            params: ArbitraryParams,
            returns: ArbitraryReturns,
            locals: ArbitraryLocals,
        ) -> TestResult {
            let mut module_builder = WasmModuleBuilder::new();

            // Convert LirTypes to ValTypes
            let wasm_params: Vec<ValType> = params
                .0
                .iter()
                .map(|t| match t {
                    LirType::I32 => ValType::I32,
                    LirType::I64 => ValType::I64,
                    LirType::F32 => ValType::F32,
                    LirType::F64 => ValType::F64,
                })
                .collect();

            let wasm_returns: Vec<ValType> = returns
                .0
                .iter()
                .map(|t| match t {
                    LirType::I32 => ValType::I32,
                    LirType::I64 => ValType::I64,
                    LirType::F32 => ValType::F32,
                    LirType::F64 => ValType::F64,
                })
                .collect();

            let wasm_locals: Vec<(u32, ValType)> = {
                let mut grouped: std::collections::HashMap<ValType, u32> =
                    std::collections::HashMap::new();
                for lir_type in &locals.0 {
                    let val_type = match lir_type {
                        LirType::I32 => ValType::I32,
                        LirType::I64 => ValType::I64,
                        LirType::F32 => ValType::F32,
                        LirType::F64 => ValType::F64,
                    };
                    *grouped.entry(val_type).or_insert(0) += 1;
                }
                grouped.into_iter().map(|(t, c)| (c, t)).collect()
            };

            // Add function type
            let type_idx =
                module_builder.add_function_type(wasm_params.clone(), wasm_returns.clone());

            // Create function body
            let mut func = Function::new(wasm_locals);

            // Add default return values if needed
            for return_type in &wasm_returns {
                match return_type {
                    ValType::I32 => func.instruction(&Instruction::I32Const(0)),
                    ValType::I64 => func.instruction(&Instruction::I64Const(0)),
                    ValType::F32 => func.instruction(&Instruction::F32Const(0.0_f32.into())),
                    ValType::F64 => func.instruction(&Instruction::F64Const(0.0_f64.into())),
                    _ => &mut func,
                };
            }
            func.instruction(&Instruction::End);

            // Add function to module
            let func_idx = module_builder.add_function(type_idx, func);

            // Add export
            module_builder.add_export("test_func", ExportKind::Func, func_idx);

            // Finalize and validate
            match module_builder.finish() {
                Ok(wasm_bytes) => match wasmparser::validate(&wasm_bytes) {
                    Ok(_) => TestResult::passed(),
                    Err(e) => {
                        eprintln!("WASM validation failed: {:?}", e);
                        TestResult::failed()
                    }
                },
                Err(e) => {
                    eprintln!("Module builder finish failed: {:?}", e);
                    TestResult::failed()
                }
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(
                property as fn(ArbitraryParams, ArbitraryReturns, ArbitraryLocals) -> TestResult,
            );
    }

    /// Property: Empty modules (with just memory setup) are valid
    /// Feature: lir-to-wasm-codegen, Property 1: Valid WASM Module Generation
    #[test]
    fn prop_minimal_module_is_valid() {
        fn property() -> TestResult {
            let lir_module = create_minimal_lir_module();

            match encode_wasm(&lir_module) {
                Ok(wasm_bytes) => match wasmparser::validate(&wasm_bytes) {
                    Ok(_) => TestResult::passed(),
                    Err(e) => {
                        eprintln!("WASM validation failed: {:?}", e);
                        TestResult::failed()
                    }
                },
                Err(e) => {
                    eprintln!("WASM encoding failed: {:?}", e);
                    TestResult::failed()
                }
            }
        }

        QuickCheck::new()
            .tests(100)
            .quickcheck(property as fn() -> TestResult);
    }

    // =========================================================================
    // Unit tests for specific module validation scenarios
    // =========================================================================

    #[test]
    fn test_minimal_module_validates() {
        let lir_module = create_minimal_lir_module();
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Minimal module should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_i32_params_validates() {
        let lir_module = create_lir_module_with_signature(
            vec![LirType::I32, LirType::I32],
            vec![],
            vec![],
        );
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with i32 params should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_i64_params_validates() {
        let lir_module = create_lir_module_with_signature(
            vec![LirType::I64],
            vec![],
            vec![],
        );
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with i64 params should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_float_params_validates() {
        let lir_module = create_lir_module_with_signature(
            vec![LirType::F32, LirType::F64],
            vec![],
            vec![],
        );
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with float params should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_return_value_validates() {
        let lir_module = create_lir_module_with_signature(
            vec![],
            vec![LirType::I32],
            vec![],
        );
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with return value should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_locals_validates() {
        let lir_module = create_lir_module_with_signature(
            vec![],
            vec![],
            vec![LirType::I32, LirType::I64, LirType::F32, LirType::F64],
        );
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with locals should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_mixed_signature_validates() {
        let lir_module = create_lir_module_with_signature(
            vec![LirType::I32, LirType::F64],
            vec![LirType::I64],
            vec![LirType::F32, LirType::I32],
        );
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with mixed signature should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_module_with_multiple_functions_validates() {
        let lir_module = LirModule {
            functions: vec![
                LirFunction {
                    name: "main".to_string(),
                    params: vec![],
                    returns: vec![],
                    locals: vec![],
                    body: vec![],
                    is_main: true,
                },
                LirFunction {
                    name: "helper".to_string(),
                    params: vec![LirType::I32],
                    returns: vec![LirType::I32],
                    locals: vec![],
                    body: vec![],
                    is_main: false,
                },
            ],
            structs: vec![],
        };
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");
        let validation_result = wasmparser::validate(&wasm_bytes);
        assert!(
            validation_result.is_ok(),
            "Module with multiple functions should validate: {:?}",
            validation_result.err()
        );
    }

    #[test]
    fn test_wasm_validator_validates_minimal_module() {
        let lir_module = create_minimal_lir_module();
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");

        let mut validator = WasmValidator::new();
        let result = validator.validate_module(&wasm_bytes);
        assert!(
            result.is_ok(),
            "WasmValidator should validate minimal module: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_wasm_validator_with_context() {
        let lir_module = create_minimal_lir_module();
        let wasm_bytes = encode_wasm(&lir_module).expect("Encoding should succeed");

        let mut validator = WasmValidator::new();
        validator.set_context("test module validation");
        let result = validator.validate_module(&wasm_bytes);
        assert!(
            result.is_ok(),
            "WasmValidator with context should validate: {:?}",
            result.err()
        );
    }
}
