// Simple validation test for wasm_encoder error handling
use wasm_encoder::{
    Module, TypeSection, FunctionSection, CodeSection,
    Function, FuncType, ValType, Instruction
};
use wasmparser::Validator;

#[test]
fn test_wasm_encoder_basic_validation() {
    let mut module = Module::new();
    
    // Add a simple function: () -> i32
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    func.instruction(&Instruction::I32Const(42));
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let mut validator = Validator::new();
    let result = validator.validate_all(&wasm_bytes);
    assert!(result.is_ok(), "Basic WASM module should be valid");
}

#[test]
fn test_wasm_encoder_type_mismatch_detection() {
    let mut module = Module::new();
    
    // Function type: () -> i32 (expects i32 return)
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // ERROR: Function should return i32, but we're producing f32
    func.instruction(&Instruction::F32Const(3.14.into()));
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // Validation should fail due to type mismatch
    let mut validator = Validator::new();
    let result = validator.validate_all(&wasm_bytes);
    assert!(result.is_err(), "Type mismatch should be detected by validation");
}