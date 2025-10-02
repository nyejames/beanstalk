// Tests for wasm_encoder Function builder API research
// These tests validate Function builder patterns work correctly

use wasm_encoder::{
    Module, TypeSection, FunctionSection, ExportSection, CodeSection,
    Function, FuncType, ValType, ExportKind, Instruction, BlockType, MemArg
};
use wasmparser::Validator;

#[test]
fn test_simple_function_construction() {
    let mut module = Module::new();
    
    // Type section: () -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("get_answer", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: simple function returning constant
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]); // no locals
    
    func.instruction(&Instruction::I32Const(42));
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Simple function should be valid");
}

#[test]
fn test_function_with_parameters() {
    let mut module = Module::new();
    
    // Type section: (i32, i32) -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("add", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: add two parameters
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]); // no locals beyond parameters
    
    func.instruction(&Instruction::LocalGet(0)); // first parameter
    func.instruction(&Instruction::LocalGet(1)); // second parameter
    func.instruction(&Instruction::I32Add);      // add them
    func.instruction(&Instruction::End);         // return result
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with parameters should be valid");
}

#[test]
fn test_function_with_local_variables() {
    let mut module = Module::new();
    
    // Type section: (i32, i32) -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("calculate", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: function with local variables
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![
        (1, ValType::I32), // 1 local i32 variable
        (1, ValType::F32), // 1 local f32 variable
    ]);
    
    // Use local variable for intermediate calculation
    func.instruction(&Instruction::LocalGet(0)); // parameter 0
    func.instruction(&Instruction::LocalGet(1)); // parameter 1
    func.instruction(&Instruction::I32Add);      // add parameters
    func.instruction(&Instruction::LocalSet(2)); // store in local 2 (first local)
    
    // Return the local variable
    func.instruction(&Instruction::LocalGet(2));
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with locals should be valid");
}

#[test]
fn test_function_with_control_flow() {
    let mut module = Module::new();
    
    // Type section: (i32) -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![ValType::I32], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("abs", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: absolute value function using if-else
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Check if parameter is negative
    func.instruction(&Instruction::LocalGet(0)); // parameter
    func.instruction(&Instruction::I32Const(0)); // 0
    func.instruction(&Instruction::I32LtS);      // param < 0?
    
    func.instruction(&Instruction::If(BlockType::Result(ValType::I32)));
        // Negative: return -param
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::LocalGet(0));
        func.instruction(&Instruction::I32Sub); // 0 - param
    func.instruction(&Instruction::Else);
        // Positive: return param
        func.instruction(&Instruction::LocalGet(0));
    func.instruction(&Instruction::End); // end if
    func.instruction(&Instruction::End); // end function
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with control flow should be valid");
}

#[test]
fn test_function_with_loop() {
    let mut module = Module::new();
    
    // Type section: (i32) -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![ValType::I32], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("sum_to_n", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: sum from 1 to n using loop
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![(1, ValType::I32)]); // 1 local for sum
    
    // Initialize sum to 0
    func.instruction(&Instruction::I32Const(0));
    func.instruction(&Instruction::LocalSet(1)); // sum = 0
    
    // Loop while n > 0
    func.instruction(&Instruction::Block(BlockType::Empty)); // outer block for breaking
    func.instruction(&Instruction::Loop(BlockType::Empty));
        // Check if n <= 0
        func.instruction(&Instruction::LocalGet(0)); // n
        func.instruction(&Instruction::I32Const(0));
        func.instruction(&Instruction::I32LeS); // n <= 0?
        func.instruction(&Instruction::BrIf(1)); // break out of outer block if n <= 0
        
        // sum = sum + n
        func.instruction(&Instruction::LocalGet(1)); // sum
        func.instruction(&Instruction::LocalGet(0)); // n
        func.instruction(&Instruction::I32Add);
        func.instruction(&Instruction::LocalSet(1)); // store sum
        
        // n = n - 1
        func.instruction(&Instruction::LocalGet(0)); // n
        func.instruction(&Instruction::I32Const(1));
        func.instruction(&Instruction::I32Sub);
        func.instruction(&Instruction::LocalSet(0)); // store n
        
        func.instruction(&Instruction::Br(0)); // continue loop
    func.instruction(&Instruction::End); // end loop
    func.instruction(&Instruction::End); // end outer block
    
    // Return sum
    func.instruction(&Instruction::LocalGet(1));
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with loop should be valid");
}

#[test]
fn test_function_with_multiple_return_values() {
    let mut module = Module::new();
    
    // Type section: (i32, i32) -> (i32, i32)
    let mut types = TypeSection::new();
    let func_type = FuncType::new(
        vec![ValType::I32, ValType::I32], 
        vec![ValType::I32, ValType::I32]
    );
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("swap", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: swap two values
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // Return parameters in swapped order
    func.instruction(&Instruction::LocalGet(1)); // second parameter first
    func.instruction(&Instruction::LocalGet(0)); // first parameter second
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with multiple returns should be valid");
}

#[test]
fn test_function_stack_management() {
    let mut module = Module::new();
    
    // Type section: (i32, i32, i32) -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(
        vec![ValType::I32, ValType::I32, ValType::I32], 
        vec![ValType::I32]
    );
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section
    let mut exports = ExportSection::new();
    exports.export("complex_calc", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: complex calculation with proper stack management
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![(2, ValType::I32)]); // 2 locals for intermediate results
    
    // Calculate (a + b) * c using locals to manage stack depth
    func.instruction(&Instruction::LocalGet(0)); // a
    func.instruction(&Instruction::LocalGet(1)); // b
    func.instruction(&Instruction::I32Add);      // a + b
    func.instruction(&Instruction::LocalSet(3)); // store in local 3
    
    func.instruction(&Instruction::LocalGet(3)); // a + b
    func.instruction(&Instruction::LocalGet(2)); // c
    func.instruction(&Instruction::I32Mul);      // (a + b) * c
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with stack management should be valid");
}

#[test]
fn test_function_validation_errors() {
    // Test that missing End instruction causes validation failure
    let mut module = Module::new();
    
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut exports = ExportSection::new();
    exports.export("invalid", ExportKind::Func, 0);
    module.section(&exports);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    func.instruction(&Instruction::I32Const(42));
    // Missing: func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    let result = validator.validate_all(&wasm_bytes);
    assert!(result.is_err(), "Function without End should be invalid");
}

#[test]
fn test_local_variable_indexing() {
    let mut module = Module::new();
    
    // Type section: (i32, f32) -> i32 (2 parameters)
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![ValType::I32, ValType::F32], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut exports = ExportSection::new();
    exports.export("test_locals", ExportKind::Func, 0);
    module.section(&exports);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![
        (1, ValType::I32), // 1 i32 local (index 2)
        (2, ValType::F32), // 2 f32 locals (indices 3, 4)
    ]);
    
    // Test accessing parameters and locals
    func.instruction(&Instruction::LocalGet(0)); // parameter 0 (i32)
    func.instruction(&Instruction::LocalGet(2)); // local 0 (i32, index 2)
    func.instruction(&Instruction::I32Add);      // add them
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Local indexing should be valid");
}

#[test]
fn test_function_with_tee_instruction() {
    let mut module = Module::new();
    
    // Type section: (i32) -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![ValType::I32], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut exports = ExportSection::new();
    exports.export("double_and_store", ExportKind::Func, 0);
    module.section(&exports);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![(1, ValType::I32)]); // 1 local
    
    // Double the parameter and store in local, then return it
    func.instruction(&Instruction::LocalGet(0)); // parameter
    func.instruction(&Instruction::I32Const(2)); // 2
    func.instruction(&Instruction::I32Mul);      // param * 2
    func.instruction(&Instruction::LocalTee(1)); // store in local and keep on stack
    func.instruction(&Instruction::End);         // return the value
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with tee should be valid");
}