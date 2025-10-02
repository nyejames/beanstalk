// Tests for wasm_encoder Type System Integration
// These tests validate type system patterns work correctly

use wasm_encoder::{
    Module, TypeSection, FunctionSection, ExportSection, CodeSection,
    Function, ValType, ExportKind, Instruction, BlockType
};
use wasmparser::Validator;

#[test]
fn test_basic_type_definitions() {
    let mut module = Module::new();
    
    // Type section with various function signatures
    let mut types = TypeSection::new();
    
    // Type 0: () -> i32
    types.ty().function(vec![], vec![ValType::I32]);
    
    // Type 1: (i32, i32) -> i32
    types.ty().function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    
    // Type 2: (f64) -> f64
    types.ty().function(vec![ValType::F64], vec![ValType::F64]);
    
    module.section(&types);
    
    // Function section using type indices
    let mut functions = FunctionSection::new();
    functions.function(0); // uses type 0
    functions.function(1); // uses type 1
    functions.function(2); // uses type 2
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("get_const", ExportKind::Func, 0);
    exports.export("add", ExportKind::Func, 1);
    exports.export("sqrt", ExportKind::Func, 2);
    module.section(&exports);
    
    // Code section with type-matching implementations
    let mut code = CodeSection::new();
    
    // Function 0: () -> i32
    let mut func0 = Function::new(vec![]);
    func0.instruction(&Instruction::I32Const(42));
    func0.instruction(&Instruction::End);
    code.function(&func0);
    
    // Function 1: (i32, i32) -> i32
    let mut func1 = Function::new(vec![]);
    func1.instruction(&Instruction::LocalGet(0));
    func1.instruction(&Instruction::LocalGet(1));
    func1.instruction(&Instruction::I32Add);
    func1.instruction(&Instruction::End);
    code.function(&func1);
    
    // Function 2: (f64) -> f64
    let mut func2 = Function::new(vec![]);
    func2.instruction(&Instruction::LocalGet(0));
    func2.instruction(&Instruction::F64Sqrt);
    func2.instruction(&Instruction::End);
    code.function(&func2);
    
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Type definitions should be valid");
}

#[test]
fn test_type_safe_control_flow() {
    let mut module = Module::new();
    
    // Type section: (i32) -> i32
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("abs", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: type-safe if-else
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Absolute value with type-safe control flow
    func.instruction(&Instruction::LocalGet(0)); // parameter
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::I32LtS); // param < 0?
    
    // Both branches must produce i32
    func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        // Then: return -param
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalGet(0));
        func.instruction(&Instruction::I32Sub);
    func.instruction(&Instruction::Else);
        // Else: return param
        func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::End);
    
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Type-safe control flow should be valid");
}

#[test]
fn test_local_variable_types() {
    let mut module = Module::new();
    
    // Type section: (i32, f32) -> f64
    let mut types = TypeSection::new();
    types.ty().function(
        vec![ValType::I32, ValType::F32],
        vec![ValType::F64]
    );
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("convert_and_add", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: function with typed locals
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![
        (1, ValType::F64), // 1 f64 local
        (1, ValType::I32), // 1 i32 local
    ]);
    
    // Local indices:
    // 0: i32 parameter
    // 1: f32 parameter
    // 2: f64 local
    // 3: i32 local
    
    // Convert i32 to f64 and store
    func.instruction(&Instruction::LocalGet(0)); // i32 parameter
    func.instruction(&Instruction::F64ConvertI32S); // i32 -> f64
    func.instruction(&Instruction::LocalSet(2)); // store in f64 local
    
    // Convert f32 to f64 and add
    func.instruction(&Instruction::LocalGet(1)); // f32 parameter
    func.instruction(&Instruction::F64PromoteF32); // f32 -> f64
    func.instruction(&Instruction::LocalGet(2)); // f64 local
    func.instruction(&Instruction::F64Add); // f64 + f64
    
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Typed locals should be valid");
}

#[test]
fn test_type_conversions() {
    let mut module = Module::new();
    
    // Type section: (i32, f32, f64) -> (f32, i32, f64)
    let mut types = TypeSection::new();
    types.ty().function(
        vec![ValType::I32, ValType::F32, ValType::F64],
        vec![ValType::F32, ValType::I32, ValType::F64]
    );
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("type_conversions", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: various type conversions
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Convert i32 -> f32
    func.instruction(&Instruction::LocalGet(0)); // i32 parameter
    func.instruction(&Instruction::F32ConvertI32S); // i32 -> f32
    
    // Convert f32 -> i32
    func.instruction(&Instruction::LocalGet(1)); // f32 parameter
    func.instruction(&Instruction::I32TruncF32S); // f32 -> i32
    
    // Keep f64 as-is
    func.instruction(&Instruction::LocalGet(2)); // f64 parameter
    
    func.instruction(&Instruction::End); // returns (f32, i32, f64)
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Type conversions should be valid");
}

#[test]
fn test_multi_value_returns() {
    let mut module = Module::new();
    
    // Type section: (i32, i32) -> (i32, i32)
    let mut types = TypeSection::new();
    types.ty().function(
        vec![ValType::I32, ValType::I32],
        vec![ValType::I32, ValType::I32]
    );
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("swap", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: function returning multiple values
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Swap the two parameters
    func.instruction(&Instruction::LocalGet(1)); // second parameter
    func.instruction(&Instruction::LocalGet(0)); // first parameter
    func.instruction(&Instruction::End); // returns (param1, param0)
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Multi-value returns should be valid");
}

#[test]
fn test_type_mismatch_validation() {
    let mut module = Module::new();
    
    // Type section: function should return i32
    let mut types = TypeSection::new();
    types.ty().function(vec![], vec![ValType::I32]);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Wrong: return f32 instead of i32
    func.instruction(&Instruction::F32Const(wasm_encoder::Ieee32::new(3.14_f32.to_bits())));
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    let result = validator.validate_all(&wasm_bytes);
    assert!(result.is_err(), "Type mismatch should cause validation error");
}

#[test]
fn test_stack_type_consistency() {
    let mut module = Module::new();
    
    // Type section: (i32, i32) -> i32
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Correct: consistent stack operations
    func.instruction(&Instruction::LocalGet(0)); // stack: [i32]
    func.instruction(&Instruction::LocalGet(1)); // stack: [i32, i32]
    func.instruction(&Instruction::I32Add);      // stack: [i32]
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Consistent stack types should be valid");
}

#[test]
fn test_block_type_consistency() {
    let mut module = Module::new();
    
    // Type section: (i32) -> i32
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Block with consistent type
    func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::I32GtS); // param > 0?
    
    func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        func.instruction(&Instruction::LocalGet(0)); // i32
        func.instruction(&Instruction::I32Const(2));
        func.instruction(&Instruction::I32Mul); // i32 * i32 = i32
    func.instruction(&Instruction::Else);
        func.instruction(&Instruction::I32Const(0)); // i32
    func.instruction(&Instruction::End); // block produces i32
    
    func.instruction(&Instruction::End); // function returns i32
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Block type consistency should be valid");
}

#[test]
fn test_invalid_local_index() {
    let mut module = Module::new();
    
    // Type section: (i32) -> i32
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]); // no locals beyond parameter
    
    // Valid: access parameter 0
    func.instruction(&Instruction::LocalGet(0));
    
    // Invalid: access non-existent local 1
    func.instruction(&Instruction::LocalGet(1)); // This should cause validation error
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    let result = validator.validate_all(&wasm_bytes);
    assert!(result.is_err(), "Invalid local index should cause validation error");
}