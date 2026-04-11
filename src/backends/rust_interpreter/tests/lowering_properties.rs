//! Property-based tests for HIR to Exec IR lowering.
//!
//! WHAT: property tests that verify correctness properties across many generated inputs.
//! WHY: property tests complement unit tests by verifying universal properties hold for all inputs.

use crate::backends::rust_interpreter::exec_ir::{ExecBinaryOperator, ExecUnaryOperator};
use crate::backends::rust_interpreter::lowering::operators::{map_binary_operator, map_unary_operator};
use crate::compiler_frontend::hir::hir_nodes::{HirBinOp, HirUnaryOp};
use proptest::prelude::*;

/// **Validates: Requirements 2.1, 2.4, 2.5, 2.6**
///
/// Property 1: Binary Operator Mapping Correctness
///
/// For any HIR binary operator (Add, Sub, Mul, Div, Mod, Eq, Ne, Lt, Le, Gt, Ge, And, Or),
/// the operator mapper SHALL return the corresponding Exec IR binary operator with the same
/// semantic meaning.
#[test]
fn property_binary_operator_mapping_correctness() {
    proptest!(ProptestConfig::with_cases(100), |(hir_op in any_supported_hir_binary_operator())| {
        let result = map_binary_operator(hir_op);
        
        // All supported operators should map successfully
        prop_assert!(result.is_ok(), "Supported operator {:?} should map successfully", hir_op);
        
        let exec_op = result.unwrap();
        
        // Verify the mapping preserves semantic meaning
        match hir_op {
            HirBinOp::Add => prop_assert_eq!(exec_op, ExecBinaryOperator::Add),
            HirBinOp::Sub => prop_assert_eq!(exec_op, ExecBinaryOperator::Subtract),
            HirBinOp::Mul => prop_assert_eq!(exec_op, ExecBinaryOperator::Multiply),
            HirBinOp::Div => prop_assert_eq!(exec_op, ExecBinaryOperator::Divide),
            HirBinOp::Mod => prop_assert_eq!(exec_op, ExecBinaryOperator::Modulo),
            HirBinOp::Eq => prop_assert_eq!(exec_op, ExecBinaryOperator::Equal),
            HirBinOp::Ne => prop_assert_eq!(exec_op, ExecBinaryOperator::NotEqual),
            HirBinOp::Lt => prop_assert_eq!(exec_op, ExecBinaryOperator::LessThan),
            HirBinOp::Le => prop_assert_eq!(exec_op, ExecBinaryOperator::LessThanOrEqual),
            HirBinOp::Gt => prop_assert_eq!(exec_op, ExecBinaryOperator::GreaterThan),
            HirBinOp::Ge => prop_assert_eq!(exec_op, ExecBinaryOperator::GreaterThanOrEqual),
            HirBinOp::And => prop_assert_eq!(exec_op, ExecBinaryOperator::And),
            HirBinOp::Or => prop_assert_eq!(exec_op, ExecBinaryOperator::Or),
            _ => unreachable!("Generator should only produce supported operators"),
        }
    });
}

/// Generator for supported HIR binary operators.
///
/// This generates only the operators that are currently supported by the interpreter
/// (excludes Root and Exponent which are not yet implemented).
fn any_supported_hir_binary_operator() -> impl Strategy<Value = HirBinOp> {
    prop_oneof![
        Just(HirBinOp::Add),
        Just(HirBinOp::Sub),
        Just(HirBinOp::Mul),
        Just(HirBinOp::Div),
        Just(HirBinOp::Mod),
        Just(HirBinOp::Eq),
        Just(HirBinOp::Ne),
        Just(HirBinOp::Lt),
        Just(HirBinOp::Le),
        Just(HirBinOp::Gt),
        Just(HirBinOp::Ge),
        Just(HirBinOp::And),
        Just(HirBinOp::Or),
    ]
}

/// **Validates: Requirements 2.2, 2.7**
///
/// Property 2: Unary Operator Mapping Correctness
///
/// For any HIR unary operator (Neg, Not), the operator mapper SHALL return the corresponding
/// Exec IR unary operator with the same semantic meaning.
#[test]
fn property_unary_operator_mapping_correctness() {
    proptest!(ProptestConfig::with_cases(100), |(hir_op in any_supported_hir_unary_operator())| {
        let result = map_unary_operator(hir_op);
        
        // All supported operators should map successfully
        prop_assert!(result.is_ok(), "Supported operator {:?} should map successfully", hir_op);
        
        let exec_op = result.unwrap();
        
        // Verify the mapping preserves semantic meaning
        match hir_op {
            HirUnaryOp::Neg => prop_assert_eq!(exec_op, ExecUnaryOperator::Negate),
            HirUnaryOp::Not => prop_assert_eq!(exec_op, ExecUnaryOperator::Not),
        }
    });
}

/// Generator for supported HIR unary operators.
///
/// This generates all unary operators currently supported by the interpreter.
fn any_supported_hir_unary_operator() -> impl Strategy<Value = HirUnaryOp> {
    prop_oneof![
        Just(HirUnaryOp::Neg),
        Just(HirUnaryOp::Not),
    ]
}

/// **Validates: Requirements 3.3**
///
/// Property 5: Literal Lowering Efficiency
///
/// For any HIR literal expression (Int, Float, Bool, Char, String), lowering SHALL return
/// the literal value directly without allocating a temporary local.
#[test]
fn property_literal_lowering_efficiency() {
    use crate::backends::rust_interpreter::exec_ir::{ExecConstValue, ExecFunctionId, ExecValue};
    use crate::backends::rust_interpreter::lowering::context::{FunctionLoweringLayout, LoweringContext};
    use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirModule, HirValueId, RegionId, ValueKind};
    use rustc_hash::FxHashMap;
    
    proptest!(ProptestConfig::with_cases(100), |(literal_variant in any_literal_expression())| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        let float_type = type_context.insert(HirType { kind: HirTypeKind::Float });
        let bool_type = type_context.insert(HirType { kind: HirTypeKind::Bool });
        let char_type = type_context.insert(HirType { kind: HirTypeKind::Char });
        let string_type = type_context.insert(HirType { kind: HirTypeKind::String });
        let unit_type = type_context.insert(HirType { kind: HirTypeKind::Unit });
        
        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;
        
        let mut context = LoweringContext::new(&hir_module);
        
        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(0),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };
        
        let mut instructions = vec![];
        
        // Create the HIR expression based on the generated variant.
        let (expression, expected_const_value) = match literal_variant {
            LiteralVariant::Int(value) => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::Int(value),
                    ty: int_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, ExecConstValue::Int(value))
            }
            LiteralVariant::Float(value) => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::Float(value),
                    ty: float_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, ExecConstValue::Float(value))
            }
            LiteralVariant::Bool(value) => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::Bool(value),
                    ty: bool_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, ExecConstValue::Bool(value))
            }
            LiteralVariant::Char(value) => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::Char(value),
                    ty: char_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, ExecConstValue::Char(value))
            }
            LiteralVariant::String(value) => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::StringLiteral(value.clone()),
                    ty: string_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, ExecConstValue::String(value))
            }
            LiteralVariant::Unit => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::TupleConstruct { elements: vec![] },
                    ty: unit_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, ExecConstValue::Unit)
            }
        };
        
        // Lower the expression.
        let result = lower_expression(&mut context, &mut layout, &mut instructions, &expression);
        
        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Literal lowering should succeed");
        
        let exec_value = result.unwrap();
        
        // Property 2: Result must be ExecValue::Literal, not ExecValue::Local.
        prop_assert!(
            matches!(exec_value, ExecValue::Literal(_)),
            "Literal lowering must return ExecValue::Literal, not ExecValue::Local"
        );
        
        // Property 3: No temporary locals should be allocated.
        prop_assert_eq!(
            layout.temp_local_count,
            0,
            "Literal lowering must not allocate temporary locals"
        );
        
        // Property 4: No instructions should be emitted.
        prop_assert_eq!(
            instructions.len(),
            0,
            "Literal lowering must not emit instructions"
        );
        
        // Property 5: The literal value must match the input.
        if let ExecValue::Literal(const_value) = exec_value {
            match (const_value, expected_const_value) {
                (ExecConstValue::Int(a), ExecConstValue::Int(b)) => prop_assert_eq!(a, b),
                (ExecConstValue::Float(a), ExecConstValue::Float(b)) => {
                    // For floats, handle NaN specially
                    if b.is_nan() {
                        prop_assert!(a.is_nan(), "Expected NaN, got {}", a);
                    } else {
                        prop_assert_eq!(a, b);
                    }
                }
                (ExecConstValue::Bool(a), ExecConstValue::Bool(b)) => prop_assert_eq!(a, b),
                (ExecConstValue::Char(a), ExecConstValue::Char(b)) => prop_assert_eq!(a, b),
                (ExecConstValue::String(ref a), ExecConstValue::String(ref b)) => prop_assert_eq!(a, b),
                (ExecConstValue::Unit, ExecConstValue::Unit) => {},
                _ => prop_assert!(false, "Mismatched const value types"),
            }
        }
    });
}

/// Enum representing different literal variants for property testing.
#[derive(Debug, Clone)]
enum LiteralVariant {
    Int(i64),
    Float(f64),
    Bool(bool),
    Char(char),
    String(String),
    Unit,
}

/// Generator for literal expressions covering all supported literal types.
fn any_literal_expression() -> impl Strategy<Value = LiteralVariant> {
    prop_oneof![
        any::<i64>().prop_map(LiteralVariant::Int),
        any::<f64>().prop_map(LiteralVariant::Float),
        any::<bool>().prop_map(LiteralVariant::Bool),
        any::<char>().prop_map(LiteralVariant::Char),
        ".*".prop_map(LiteralVariant::String),
        Just(LiteralVariant::Unit),
    ]
}

/// **Validates: Requirements 3.4**
///
/// Property 6: Local Reference Lowering Efficiency
///
/// For any HIR local reference expression, lowering SHALL return the local reference directly
/// without allocating a temporary local.
#[test]
fn property_local_reference_lowering_efficiency() {
    use crate::backends::rust_interpreter::exec_ir::{ExecFunctionId, ExecLocalId, ExecValue};
    use crate::backends::rust_interpreter::lowering::context::{FunctionLoweringLayout, LoweringContext};
    use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{HirExpression, HirExpressionKind, HirModule, HirPlace, HirValueId, LocalId, RegionId, ValueKind};
    use rustc_hash::FxHashMap;
    
    proptest!(ProptestConfig::with_cases(100), |(local_index in 0u32..=50, is_load in any::<bool>())| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        
        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;
        
        let mut context = LoweringContext::new(&hir_module);
        
        // Create a HIR local ID and map it to an Exec local ID.
        let hir_local_id = LocalId(local_index);
        let exec_local_id = ExecLocalId(local_index);
        
        let mut exec_local_by_hir_local = FxHashMap::default();
        exec_local_by_hir_local.insert(hir_local_id, exec_local_id);
        
        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![hir_local_id],
            exec_local_by_hir_local,
            scratch_local_id: ExecLocalId(100),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };
        
        let mut instructions = vec![];
        
        // Create a HIR local reference expression (either Load or Copy).
        let expression = if is_load {
            HirExpression {
                id: HirValueId(1),
                kind: HirExpressionKind::Load(HirPlace::Local(hir_local_id)),
                ty: int_type,
                value_kind: ValueKind::Const,
                region: RegionId(0),
            }
        } else {
            HirExpression {
                id: HirValueId(1),
                kind: HirExpressionKind::Copy(HirPlace::Local(hir_local_id)),
                ty: int_type,
                value_kind: ValueKind::Const,
                region: RegionId(0),
            }
        };
        
        // Lower the expression.
        let result = lower_expression(&mut context, &mut layout, &mut instructions, &expression);
        
        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Local reference lowering should succeed");
        
        let exec_value = result.unwrap();
        
        // Property 2: Result must be ExecValue::Local, not ExecValue::Literal.
        prop_assert!(
            matches!(exec_value, ExecValue::Local(_)),
            "Local reference lowering must return ExecValue::Local, not ExecValue::Literal"
        );
        
        // Property 3: The returned local ID must match the mapped exec local ID.
        if let ExecValue::Local(returned_id) = exec_value {
            prop_assert_eq!(
                returned_id,
                exec_local_id,
                "Returned local ID must match the mapped exec local ID"
            );
        }
        
        // Property 4: No temporary locals should be allocated.
        prop_assert_eq!(
            layout.temp_local_count,
            0,
            "Local reference lowering must not allocate temporary locals"
        );
        
        // Property 5: No instructions should be emitted.
        prop_assert_eq!(
            instructions.len(),
            0,
            "Local reference lowering must not emit instructions"
        );
    });
}

/// **Validates: Requirements 5.2, 5.5**
///
/// Property 10: Temporary Local Uniqueness
///
/// For any function lowering that allocates multiple temporary locals, each temporary local
/// SHALL have a unique index, and the total count of temporary locals tracked SHALL equal
/// the actual number of temporaries allocated.
#[test]
fn property_temporary_local_uniqueness() {
    use crate::backends::rust_interpreter::exec_ir::{ExecFunctionId, ExecStorageType};
    use crate::backends::rust_interpreter::lowering::context::FunctionLoweringLayout;
    use rustc_hash::FxHashMap;
    
    proptest!(ProptestConfig::with_cases(100), |(allocation_count in 1u32..=50)| {
        // Create a minimal FunctionLoweringLayout for testing temporary allocation.
        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(0),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };
        
        // Allocate multiple temporary locals.
        let mut allocated_ids = vec![];
        for _ in 0..allocation_count {
            let temp_id = layout.allocate_temp_local(ExecStorageType::Int);
            allocated_ids.push(temp_id);
        }
        
        // Property 1: All allocated temporary local IDs must be unique.
        let unique_ids: std::collections::HashSet<_> = allocated_ids.iter().copied().collect();
        prop_assert_eq!(
            unique_ids.len(),
            allocation_count as usize,
            "All temporary local IDs must be unique"
        );
        
        // Property 2: temp_local_count must match the actual number of allocations.
        prop_assert_eq!(
            layout.temp_local_count,
            allocation_count,
            "temp_local_count must equal the number of allocations"
        );
        
        // Property 3: The temp_locals vector must contain exactly temp_local_count entries.
        prop_assert_eq!(
            layout.temp_locals.len(),
            allocation_count as usize,
            "temp_locals vector must contain exactly temp_local_count entries"
        );
        
        // Property 4: Each allocated ID must be registered in temp_locals.
        for (i, &allocated_id) in allocated_ids.iter().enumerate() {
            prop_assert_eq!(
                layout.temp_locals[i].id,
                allocated_id,
                "Temporary local at index {} must have the correct ID",
                i
            );
        }
    });
}

/// **Validates: Requirements 3.1**
///
/// Property 3: Binary Operation Lowering Structure
///
/// For any HIR binary operation expression, lowering SHALL recursively lower the left operand,
/// recursively lower the right operand, allocate a temporary local for the result, emit a
/// BinaryOp instruction with the correct operator and operands, and return a reference to the
/// result local.
#[test]
fn property_binary_operation_lowering_structure() {
    use crate::backends::rust_interpreter::exec_ir::{
        ExecBinaryOperator, ExecFunctionId, ExecInstruction, ExecValue,
    };
    use crate::backends::rust_interpreter::lowering::context::{
        FunctionLoweringLayout, LoweringContext,
    };
    use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{
        HirBinOp, HirExpression, HirExpressionKind, HirModule, HirValueId, RegionId, ValueKind,
    };
    use rustc_hash::FxHashMap;

    proptest!(ProptestConfig::with_cases(100), |(
        operator in any_supported_hir_binary_operator(),
        left_value in any::<i64>(),
        right_value in any::<i64>()
    )| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        let bool_type = type_context.insert(HirType { kind: HirTypeKind::Bool });

        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;

        let mut context = LoweringContext::new(&hir_module);

        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(0),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };

        let mut instructions = vec![];

        // Create a HIR binary operation expression with literal operands.
        let left_expr = HirExpression {
            id: HirValueId(1),
            kind: HirExpressionKind::Int(left_value),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let right_expr = HirExpression {
            id: HirValueId(2),
            kind: HirExpressionKind::Int(right_value),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Determine result type based on operator.
        let result_type = match operator {
            HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge => bool_type,
            _ => int_type,
        };

        let binary_expr = HirExpression {
            id: HirValueId(3),
            kind: HirExpressionKind::BinOp {
                left: Box::new(left_expr),
                op: operator,
                right: Box::new(right_expr),
            },
            ty: result_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Lower the binary operation expression.
        let result = lower_expression(&mut context, &mut layout, &mut instructions, &binary_expr);

        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Binary operation lowering should succeed");

        let exec_value = result.unwrap();

        // Property 2: Result must be ExecValue::Local (a temporary was allocated).
        prop_assert!(
            matches!(exec_value, ExecValue::Local(_)),
            "Binary operation lowering must return ExecValue::Local"
        );

        // Property 3: At least one temporary local must be allocated for the result.
        // Note: Additional temporaries may be allocated for literal operands.
        prop_assert!(
            layout.temp_local_count >= 1,
            "Binary operation lowering must allocate at least one temporary local for the result"
        );

        // Property 4: Instructions must be emitted.
        // At minimum, we expect a BinaryOp instruction, but there may also be LoadConst instructions
        // for literal operands.
        prop_assert!(
            !instructions.is_empty(),
            "Binary operation lowering must emit instructions"
        );

        // Property 5: The last instruction must be a BinaryOp instruction.
        let last_instruction = instructions.last().unwrap();
        prop_assert!(
            matches!(last_instruction, ExecInstruction::BinaryOp { .. }),
            "The last instruction must be a BinaryOp instruction"
        );

        // Property 6: The BinaryOp instruction must have the correct operator.
        if let ExecInstruction::BinaryOp { operator: exec_op, destination, .. } = last_instruction {
            // Verify operator mapping is correct.
            let expected_exec_op = match operator {
                HirBinOp::Add => ExecBinaryOperator::Add,
                HirBinOp::Sub => ExecBinaryOperator::Subtract,
                HirBinOp::Mul => ExecBinaryOperator::Multiply,
                HirBinOp::Div => ExecBinaryOperator::Divide,
                HirBinOp::Mod => ExecBinaryOperator::Modulo,
                HirBinOp::Eq => ExecBinaryOperator::Equal,
                HirBinOp::Ne => ExecBinaryOperator::NotEqual,
                HirBinOp::Lt => ExecBinaryOperator::LessThan,
                HirBinOp::Le => ExecBinaryOperator::LessThanOrEqual,
                HirBinOp::Gt => ExecBinaryOperator::GreaterThan,
                HirBinOp::Ge => ExecBinaryOperator::GreaterThanOrEqual,
                HirBinOp::And => ExecBinaryOperator::And,
                HirBinOp::Or => ExecBinaryOperator::Or,
                _ => unreachable!("Generator should only produce supported operators"),
            };

            prop_assert_eq!(
                *exec_op,
                expected_exec_op,
                "BinaryOp instruction must have the correct operator"
            );

            // Property 7: The destination local must match the returned ExecValue::Local.
            if let ExecValue::Local(result_local) = exec_value {
                prop_assert_eq!(
                    *destination,
                    result_local,
                    "BinaryOp destination must match the returned local"
                );
            }
        }

        // Property 8: The result local must be registered in temp_locals.
        if let ExecValue::Local(result_local) = exec_value {
            let found = layout.temp_locals.iter().any(|temp| temp.id == result_local);
            prop_assert!(
                found,
                "Result local must be registered in temp_locals"
            );
        }
    });
}

/// **Validates: Requirements 3.2**
///
/// Property 4: Unary Operation Lowering Structure
///
/// For any HIR unary operation expression, lowering SHALL recursively lower the operand,
/// allocate a temporary local for the result, emit a UnaryOp instruction with the correct
/// operator and operand, and return a reference to the result local.
#[test]
fn property_unary_operation_lowering_structure() {
    use crate::backends::rust_interpreter::exec_ir::{
        ExecFunctionId, ExecInstruction, ExecUnaryOperator, ExecValue,
    };
    use crate::backends::rust_interpreter::lowering::context::{
        FunctionLoweringLayout, LoweringContext,
    };
    use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{
        HirExpression, HirExpressionKind, HirModule, HirUnaryOp, HirValueId, RegionId, ValueKind,
    };
    use rustc_hash::FxHashMap;

    proptest!(ProptestConfig::with_cases(100), |(
        operator in any_supported_hir_unary_operator(),
        operand_value in any::<i64>()
    )| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        let bool_type = type_context.insert(HirType { kind: HirTypeKind::Bool });

        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;

        let mut context = LoweringContext::new(&hir_module);

        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(0),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };

        let mut instructions = vec![];

        // Create operand expression and determine types based on operator.
        // For Neg, use Int operand and Int result.
        // For Not, use Bool operand and Bool result.
        let (operand_expr, result_type) = match operator {
            HirUnaryOp::Neg => {
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::Int(operand_value),
                    ty: int_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, int_type)
            }
            HirUnaryOp::Not => {
                // For Not, use a bool operand (convert int to bool for testing).
                let bool_value = operand_value % 2 == 0;
                let expr = HirExpression {
                    id: HirValueId(1),
                    kind: HirExpressionKind::Bool(bool_value),
                    ty: bool_type,
                    value_kind: ValueKind::Const,
                    region: RegionId(0),
                };
                (expr, bool_type)
            }
        };

        // Create the unary operation expression.
        let unary_expr = HirExpression {
            id: HirValueId(2),
            kind: HirExpressionKind::UnaryOp {
                op: operator,
                operand: Box::new(operand_expr),
            },
            ty: result_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Lower the unary operation expression.
        let result = lower_expression(&mut context, &mut layout, &mut instructions, &unary_expr);

        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Unary operation lowering should succeed");

        let exec_value = result.unwrap();

        // Property 2: Result must be ExecValue::Local (a temporary was allocated).
        prop_assert!(
            matches!(exec_value, ExecValue::Local(_)),
            "Unary operation lowering must return ExecValue::Local"
        );

        // Property 3: At least one temporary local must be allocated for the result.
        // Note: Additional temporaries may be allocated for literal operands.
        prop_assert!(
            layout.temp_local_count >= 1,
            "Unary operation lowering must allocate at least one temporary local for the result"
        );

        // Property 4: Instructions must be emitted.
        // At minimum, we expect a UnaryOp instruction, but there may also be LoadConst instructions
        // for literal operands.
        prop_assert!(
            !instructions.is_empty(),
            "Unary operation lowering must emit instructions"
        );

        // Property 5: The last instruction must be a UnaryOp instruction.
        let last_instruction = instructions.last().unwrap();
        prop_assert!(
            matches!(last_instruction, ExecInstruction::UnaryOp { .. }),
            "The last instruction must be a UnaryOp instruction"
        );

        // Property 6: The UnaryOp instruction must have the correct operator.
        if let ExecInstruction::UnaryOp { operator: exec_op, destination, .. } = last_instruction {
            // Verify operator mapping is correct.
            let expected_exec_op = match operator {
                HirUnaryOp::Neg => ExecUnaryOperator::Negate,
                HirUnaryOp::Not => ExecUnaryOperator::Not,
            };

            prop_assert_eq!(
                *exec_op,
                expected_exec_op,
                "UnaryOp instruction must have the correct operator"
            );

            // Property 7: The destination local must match the returned ExecValue::Local.
            if let ExecValue::Local(result_local) = exec_value {
                prop_assert_eq!(
                    *destination,
                    result_local,
                    "UnaryOp destination must match the returned local"
                );
            }
        }

        // Property 8: The result local must be registered in temp_locals.
        if let ExecValue::Local(result_local) = exec_value {
            let found = layout.temp_locals.iter().any(|temp| temp.id == result_local);
            prop_assert!(
                found,
                "Result local must be registered in temp_locals"
            );
        }
    });
}

/// **Validates: Requirements 3.5, 3.6**
///
/// Property 7: Nested Expression Evaluation Order
///
/// For any nested binary operation expression, lowering SHALL emit instructions in left-to-right
/// evaluation order, ensuring that the left operand's instructions appear before the right
/// operand's instructions in the instruction sequence.
#[test]
fn property_nested_expression_evaluation_order() {
    use crate::backends::rust_interpreter::exec_ir::{
        ExecFunctionId, ExecInstruction, ExecValue,
    };
    use crate::backends::rust_interpreter::lowering::context::{
        FunctionLoweringLayout, LoweringContext,
    };
    use crate::backends::rust_interpreter::lowering::expressions::lower_expression;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{
        HirBinOp, HirExpression, HirExpressionKind, HirModule, HirValueId, RegionId, ValueKind,
    };
    use rustc_hash::FxHashMap;

    proptest!(ProptestConfig::with_cases(100), |(
        outer_op in any_supported_hir_binary_operator(),
        left_inner_op in any_supported_hir_binary_operator(),
        right_inner_op in any_supported_hir_binary_operator(),
        a in any::<i64>(),
        b in any::<i64>(),
        c in any::<i64>(),
        d in any::<i64>()
    )| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        let bool_type = type_context.insert(HirType { kind: HirTypeKind::Bool });

        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;

        let mut context = LoweringContext::new(&hir_module);

        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(0),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };

        let mut instructions = vec![];

        // Create nested expression: (a op b) outer_op (c op d)
        // This tests that left subtree (a op b) is fully lowered before right subtree (c op d).
        
        // Left subtree: a op b
        let a_expr = HirExpression {
            id: HirValueId(1),
            kind: HirExpressionKind::Int(a),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let b_expr = HirExpression {
            id: HirValueId(2),
            kind: HirExpressionKind::Int(b),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let left_inner_result_type = match left_inner_op {
            HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge => bool_type,
            _ => int_type,
        };

        let left_subtree = HirExpression {
            id: HirValueId(3),
            kind: HirExpressionKind::BinOp {
                left: Box::new(a_expr),
                op: left_inner_op,
                right: Box::new(b_expr),
            },
            ty: left_inner_result_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Right subtree: c op d
        let c_expr = HirExpression {
            id: HirValueId(4),
            kind: HirExpressionKind::Int(c),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let d_expr = HirExpression {
            id: HirValueId(5),
            kind: HirExpressionKind::Int(d),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let right_inner_result_type = match right_inner_op {
            HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge => bool_type,
            _ => int_type,
        };

        let right_subtree = HirExpression {
            id: HirValueId(6),
            kind: HirExpressionKind::BinOp {
                left: Box::new(c_expr),
                op: right_inner_op,
                right: Box::new(d_expr),
            },
            ty: right_inner_result_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Outer expression: left_subtree outer_op right_subtree
        let outer_result_type = match outer_op {
            HirBinOp::Eq | HirBinOp::Ne | HirBinOp::Lt | HirBinOp::Le | HirBinOp::Gt | HirBinOp::Ge => bool_type,
            _ => {
                // For arithmetic/logical operators, need compatible types
                // If both inner results are int, use int; if both are bool, use bool
                // For mixed types, we'll use int (though this may fail at runtime)
                if left_inner_result_type == bool_type && right_inner_result_type == bool_type {
                    bool_type
                } else {
                    int_type
                }
            }
        };

        let nested_expr = HirExpression {
            id: HirValueId(7),
            kind: HirExpressionKind::BinOp {
                left: Box::new(left_subtree),
                op: outer_op,
                right: Box::new(right_subtree),
            },
            ty: outer_result_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Lower the nested expression.
        let result = lower_expression(&mut context, &mut layout, &mut instructions, &nested_expr);

        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Nested expression lowering should succeed");

        let exec_value = result.unwrap();

        // Property 2: Result must be ExecValue::Local.
        prop_assert!(
            matches!(exec_value, ExecValue::Local(_)),
            "Nested expression lowering must return ExecValue::Local"
        );

        // Property 3: Instructions must be emitted.
        prop_assert!(
            !instructions.is_empty(),
            "Nested expression lowering must emit instructions"
        );

        // Property 4: Verify left-to-right evaluation order.
        // WHAT: find the BinaryOp instructions for left and right subtrees and the outer operation.
        // WHY: we need to verify that left subtree instructions appear before right subtree instructions.
        
        // Count BinaryOp instructions (should be 3: left inner, right inner, outer).
        let binary_op_count = instructions.iter().filter(|inst| matches!(inst, ExecInstruction::BinaryOp { .. })).count();
        
        // We expect at least 3 BinaryOp instructions (one for each operation).
        // There may be additional LoadConst instructions for literals.
        prop_assert!(
            binary_op_count >= 3,
            "Expected at least 3 BinaryOp instructions for nested expression, found {}",
            binary_op_count
        );

        // Find indices of BinaryOp instructions.
        let binary_op_indices: Vec<usize> = instructions
            .iter()
            .enumerate()
            .filter_map(|(i, inst)| {
                if matches!(inst, ExecInstruction::BinaryOp { .. }) {
                    Some(i)
                } else {
                    None
                }
            })
            .collect();

        // Property 5: The first BinaryOp should be for the left subtree (a op b).
        // Property 6: The second BinaryOp should be for the right subtree (c op d).
        // Property 7: The third BinaryOp should be for the outer operation.
        // This verifies left-to-right evaluation order.
        
        if binary_op_indices.len() >= 3 {
            let left_inner_idx = binary_op_indices[0];
            let right_inner_idx = binary_op_indices[1];
            let outer_idx = binary_op_indices[2];

            // Verify ordering: left inner < right inner < outer.
            prop_assert!(
                left_inner_idx < right_inner_idx,
                "Left subtree BinaryOp (index {}) must appear before right subtree BinaryOp (index {})",
                left_inner_idx,
                right_inner_idx
            );

            prop_assert!(
                right_inner_idx < outer_idx,
                "Right subtree BinaryOp (index {}) must appear before outer BinaryOp (index {})",
                right_inner_idx,
                outer_idx
            );

            // Verify the operators match what we expect.
            if let ExecInstruction::BinaryOp { operator: left_op, .. } = &instructions[left_inner_idx] {
                let expected_left_op = map_binary_operator(left_inner_op).unwrap();
                prop_assert_eq!(
                    *left_op,
                    expected_left_op,
                    "Left subtree BinaryOp must have the correct operator"
                );
            }

            if let ExecInstruction::BinaryOp { operator: right_op, .. } = &instructions[right_inner_idx] {
                let expected_right_op = map_binary_operator(right_inner_op).unwrap();
                prop_assert_eq!(
                    *right_op,
                    expected_right_op,
                    "Right subtree BinaryOp must have the correct operator"
                );
            }

            if let ExecInstruction::BinaryOp { operator: outer_op_exec, .. } = &instructions[outer_idx] {
                let expected_outer_op = map_binary_operator(outer_op).unwrap();
                prop_assert_eq!(
                    *outer_op_exec,
                    expected_outer_op,
                    "Outer BinaryOp must have the correct operator"
                );
            }
        }
    });
}
/// **Validates: Requirements 4.1, 4.3**
///
/// Property 8: Branch Condition Lowering
///
/// For any branch terminator with a computed condition expression, lowering SHALL recursively
/// lower the condition expression, emit all necessary instructions, and use the result local
/// as the branch condition without using persistent scratch locals.
#[test]
fn property_branch_condition_lowering() {
    use crate::backends::rust_interpreter::exec_ir::{
        ExecBlockId, ExecFunctionId, ExecInstruction, ExecTerminator,
    };
    use crate::backends::rust_interpreter::lowering::context::{
        FunctionLoweringLayout, LoweringContext,
    };
    use crate::backends::rust_interpreter::lowering::terminators::lower_block_terminator;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{
        BlockId, HirExpression, HirExpressionKind, HirModule, HirTerminator, HirValueId,
        RegionId, ValueKind,
    };
    use rustc_hash::FxHashMap;

    proptest!(ProptestConfig::with_cases(100), |(
        op in any_supported_hir_binary_operator(),
        left_val in any::<i64>(),
        right_val in any::<i64>()
    )| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        let bool_type = type_context.insert(HirType { kind: HirTypeKind::Bool });

        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;

        let mut context = LoweringContext::new(&hir_module);

        // Create HIR block IDs for then and else branches.
        let then_block_id = BlockId(1);
        let else_block_id = BlockId(2);

        // Create Exec block IDs for then and else branches.
        let exec_then_block = ExecBlockId(1);
        let exec_else_block = ExecBlockId(2);

        let mut exec_block_by_hir_block = FxHashMap::default();
        exec_block_by_hir_block.insert(then_block_id, exec_then_block);
        exec_block_by_hir_block.insert(else_block_id, exec_else_block);

        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block,
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(999),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };

        let mut instructions = vec![];

        // Create a computed condition: left_val op right_val
        let left_expr = HirExpression {
            id: HirValueId(1),
            kind: HirExpressionKind::Int(left_val),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let right_expr = HirExpression {
            id: HirValueId(2),
            kind: HirExpressionKind::Int(right_val),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let condition_expr = HirExpression {
            id: HirValueId(3),
            kind: HirExpressionKind::BinOp {
                left: Box::new(left_expr),
                op,
                right: Box::new(right_expr),
            },
            ty: bool_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Create an If terminator with the computed condition.
        let terminator = HirTerminator::If {
            condition: condition_expr,
            then_block: then_block_id,
            else_block: else_block_id,
        };

        // Lower the terminator.
        let result = lower_block_terminator(&mut context, &mut layout, &mut instructions, &terminator);

        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Branch terminator lowering should succeed");

        let exec_terminator = result.unwrap();

        // Property 2: Result must be a BranchBool terminator.
        prop_assert!(
            matches!(exec_terminator, ExecTerminator::BranchBool { .. }),
            "Branch terminator must lower to ExecTerminator::BranchBool"
        );

        // Property 3: Instructions must be emitted for the computed condition.
        prop_assert!(
            !instructions.is_empty(),
            "Branch terminator with computed condition must emit instructions"
        );

        // Property 4: The scratch local must NOT be used.
        // WHAT: verify that no instruction references the scratch local (ID 999).
        // WHY: Requirement 4.3 states no persistent scratch locals should be used.
        let scratch_local_id = crate::backends::rust_interpreter::exec_ir::ExecLocalId(999);
        for instruction in &instructions {
            match instruction {
                ExecInstruction::BinaryOp { left, right, destination, .. } => {
                    prop_assert_ne!(*left, scratch_local_id, "BinaryOp left operand must not use scratch local");
                    prop_assert_ne!(*right, scratch_local_id, "BinaryOp right operand must not use scratch local");
                    prop_assert_ne!(*destination, scratch_local_id, "BinaryOp destination must not use scratch local");
                }
                ExecInstruction::LoadConst { target, .. } => {
                    prop_assert_ne!(*target, scratch_local_id, "LoadConst target must not use scratch local");
                }
                _ => {}
            }
        }

        // Property 5: The condition local in the terminator must NOT be the scratch local.
        if let ExecTerminator::BranchBool { condition, .. } = exec_terminator {
            prop_assert_ne!(
                condition,
                scratch_local_id,
                "Branch condition must not use scratch local"
            );
        }

        // Property 6: At least one temporary local should be allocated for the result.
        prop_assert!(
            layout.temp_local_count > 0,
            "Branch condition lowering must allocate at least one temporary local"
        );

        // Property 7: The last instruction should be a BinaryOp (for the condition computation).
        if let Some(last_instruction) = instructions.last() {
            prop_assert!(
                matches!(last_instruction, ExecInstruction::BinaryOp { .. }),
                "Last instruction should be BinaryOp for condition computation"
            );
        }
    });
}

/// **Validates: Requirements 4.2, 4.3**
///
/// Property 9: Return Value Lowering
///
/// For any return terminator with a computed value expression, lowering SHALL recursively
/// lower the return expression, emit all necessary instructions, and use the result local
/// as the return value without using persistent scratch locals.
#[test]
fn property_return_value_lowering() {
    use crate::backends::rust_interpreter::exec_ir::{
        ExecFunctionId, ExecInstruction, ExecTerminator,
    };
    use crate::backends::rust_interpreter::lowering::context::{
        FunctionLoweringLayout, LoweringContext,
    };
    use crate::backends::rust_interpreter::lowering::terminators::lower_block_terminator;
    use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
    use crate::compiler_frontend::hir::hir_nodes::{
        HirExpression, HirExpressionKind, HirModule, HirTerminator, HirValueId, RegionId,
        ValueKind,
    };
    use rustc_hash::FxHashMap;

    proptest!(ProptestConfig::with_cases(100), |(
        op in any_supported_hir_binary_operator(),
        left_val in any::<i64>(),
        right_val in any::<i64>()
    )| {
        // Create minimal test context.
        let mut type_context = TypeContext::default();
        let int_type = type_context.insert(HirType { kind: HirTypeKind::Int });
        let bool_type = type_context.insert(HirType { kind: HirTypeKind::Bool });

        let mut hir_module = HirModule::new();
        hir_module.type_context = type_context;

        let mut context = LoweringContext::new(&hir_module);

        let mut layout = FunctionLoweringLayout {
            exec_function_id: ExecFunctionId(0),
            ordered_hir_block_ids: vec![],
            exec_block_by_hir_block: FxHashMap::default(),
            ordered_hir_local_ids: vec![],
            exec_local_by_hir_local: FxHashMap::default(),
            scratch_local_id: crate::backends::rust_interpreter::exec_ir::ExecLocalId(999),
            next_temp_local_index: 0,
            temp_local_count: 0,
            temp_locals: vec![],
        };

        let mut instructions = vec![];

        // Create a computed return value: left_val op right_val
        let left_expr = HirExpression {
            id: HirValueId(1),
            kind: HirExpressionKind::Int(left_val),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        let right_expr = HirExpression {
            id: HirValueId(2),
            kind: HirExpressionKind::Int(right_val),
            ty: int_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Determine result type based on operator
        let result_type = match op {
            crate::compiler_frontend::hir::hir_nodes::HirBinOp::Eq
            | crate::compiler_frontend::hir::hir_nodes::HirBinOp::Ne
            | crate::compiler_frontend::hir::hir_nodes::HirBinOp::Lt
            | crate::compiler_frontend::hir::hir_nodes::HirBinOp::Le
            | crate::compiler_frontend::hir::hir_nodes::HirBinOp::Gt
            | crate::compiler_frontend::hir::hir_nodes::HirBinOp::Ge => bool_type,
            _ => int_type,
        };

        let return_expr = HirExpression {
            id: HirValueId(3),
            kind: HirExpressionKind::BinOp {
                left: Box::new(left_expr),
                op,
                right: Box::new(right_expr),
            },
            ty: result_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        };

        // Create a Return terminator with the computed value.
        let terminator = HirTerminator::Return(return_expr);

        // Lower the terminator.
        let result = lower_block_terminator(&mut context, &mut layout, &mut instructions, &terminator);

        // Property 1: Lowering must succeed.
        prop_assert!(result.is_ok(), "Return terminator lowering should succeed");

        let exec_terminator = result.unwrap();

        // Property 2: Result must be a Return terminator with a value.
        prop_assert!(
            matches!(exec_terminator, ExecTerminator::Return { value: Some(_) }),
            "Return terminator must lower to ExecTerminator::Return with a value"
        );

        // Property 3: Instructions must be emitted for the computed return value.
        prop_assert!(
            !instructions.is_empty(),
            "Return terminator with computed value must emit instructions"
        );

        // Property 4: The scratch local must NOT be used.
        // WHAT: verify that no instruction references the scratch local (ID 999).
        // WHY: Requirement 4.3 states no persistent scratch locals should be used.
        let scratch_local_id = crate::backends::rust_interpreter::exec_ir::ExecLocalId(999);
        for instruction in &instructions {
            match instruction {
                ExecInstruction::BinaryOp { left, right, destination, .. } => {
                    prop_assert_ne!(*left, scratch_local_id, "BinaryOp left operand must not use scratch local");
                    prop_assert_ne!(*right, scratch_local_id, "BinaryOp right operand must not use scratch local");
                    prop_assert_ne!(*destination, scratch_local_id, "BinaryOp destination must not use scratch local");
                }
                ExecInstruction::LoadConst { target, .. } => {
                    prop_assert_ne!(*target, scratch_local_id, "LoadConst target must not use scratch local");
                }
                _ => {}
            }
        }

        // Property 5: The return value local in the terminator must NOT be the scratch local.
        if let ExecTerminator::Return { value: Some(return_local) } = exec_terminator {
            prop_assert_ne!(
                return_local,
                scratch_local_id,
                "Return value must not use scratch local"
            );
        }

        // Property 6: At least one temporary local should be allocated for the result.
        prop_assert!(
            layout.temp_local_count > 0,
            "Return value lowering must allocate at least one temporary local"
        );

        // Property 7: The last instruction should be a BinaryOp (for the return value computation).
        if let Some(last_instruction) = instructions.last() {
            prop_assert!(
                matches!(last_instruction, ExecInstruction::BinaryOp { .. }),
                "Last instruction should be BinaryOp for return value computation"
            );
        }
    });
}
