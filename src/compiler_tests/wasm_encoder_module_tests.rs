// Tests for wasm_encoder Module builder API research
// These tests validate our documented examples work correctly

use wasm_encoder::{
    Module, TypeSection, FunctionSection, ExportSection, CodeSection, MemorySection,
    GlobalSection, ValType, ExportKind, Function, Instruction, MemoryType, GlobalType, ConstExpr, MemArg
};
use wasmparser::Validator;

#[test]
fn test_minimal_module_construction() {
    let mut module = Module::new();
    
    // Type section: () -> i32
    let mut types = TypeSection::new();
    types.ty().function(vec![], vec![ValType::I32]);
    module.section(&types);
    
    // Function section: one function using type 0
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    // Export section: export function as "get_answer"
    let mut exports = ExportSection::new();
    exports.export("get_answer", ExportKind::Func, 0);
    module.section(&exports);
    
    // Code section: implement the function
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    func.instruction(&Instruction::I32Const(42));
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Module should be valid");
    
    // Check WASM magic number
    assert_eq!(&wasm_bytes[0..4], &[0x00, 0x61, 0x73, 0x6d]); // "\0asm"
    assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00]); // version 1
}

#[test]
fn test_math_operations_module() {
    let mut module = Module::new();
    
    // Type section: (i32, i32) -> i32 for all math operations
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32, ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    // Function section: four functions, all using type 0
    let mut functions = FunctionSection::new();
    functions.function(0); // add
    functions.function(0); // subtract  
    functions.function(0); // multiply
    functions.function(0); // divide
    module.section(&functions);
    
    // Export section: export all math functions
    let mut exports = ExportSection::new();
    exports.export("add", ExportKind::Func, 0);
    exports.export("subtract", ExportKind::Func, 1);
    exports.export("multiply", ExportKind::Func, 2);
    exports.export("divide", ExportKind::Func, 3);
    module.section(&exports);
    
    // Code section: implement all functions
    let mut code = CodeSection::new();
    
    // Add function: param0 + param1
    let mut add_func = Function::new(vec![]);
    add_func.instruction(&Instruction::LocalGet(0));
    add_func.instruction(&Instruction::LocalGet(1));
    add_func.instruction(&Instruction::I32Add);
    add_func.instruction(&Instruction::End);
    code.function(&add_func);
    
    // Subtract function: param0 - param1
    let mut sub_func = Function::new(vec![]);
    sub_func.instruction(&Instruction::LocalGet(0));
    sub_func.instruction(&Instruction::LocalGet(1));
    sub_func.instruction(&Instruction::I32Sub);
    sub_func.instruction(&Instruction::End);
    code.function(&sub_func);
    
    // Multiply function: param0 * param1
    let mut mul_func = Function::new(vec![]);
    mul_func.instruction(&Instruction::LocalGet(0));
    mul_func.instruction(&Instruction::LocalGet(1));
    mul_func.instruction(&Instruction::I32Mul);
    mul_func.instruction(&Instruction::End);
    code.function(&mul_func);
    
    // Divide function: param0 / param1 (signed)
    let mut div_func = Function::new(vec![]);
    div_func.instruction(&Instruction::LocalGet(0));
    div_func.instruction(&Instruction::LocalGet(1));
    div_func.instruction(&Instruction::I32DivS);
    div_func.instruction(&Instruction::End);
    code.function(&div_func);
    
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Math module should be valid");
}

#[test]
fn test_module_with_memory_and_globals() {
    let mut module = Module::new();
    
    // Type section: various function signatures
    let mut types = TypeSection::new();
    types.ty().function(vec![], vec![ValType::I32]);           // () -> i32 (get_counter)
    types.ty().function(vec![ValType::I32], vec![]);           // (i32) -> () (set_counter)
    types.ty().function(vec![ValType::I32, ValType::I32], vec![]); // (i32, i32) -> () (store_at)
    types.ty().function(vec![ValType::I32], vec![ValType::I32]); // (i32) -> i32 (load_from)
    module.section(&types);
    
    // Function section: four functions
    let mut functions = FunctionSection::new();
    functions.function(0); // get_counter
    functions.function(1); // set_counter
    functions.function(2); // store_at
    functions.function(3); // load_from
    module.section(&functions);
    
    // Memory section: 1 page minimum, 10 pages maximum
    let mut memories = MemorySection::new();
    memories.memory(MemoryType {
        minimum: 1,
        maximum: Some(10),
        memory64: false,
        shared: false,
        page_size_log2: None,
    });
    module.section(&memories);
    
    // Global section: mutable counter initialized to 0
    let mut globals = GlobalSection::new();
    let global_type = GlobalType {
        val_type: ValType::I32,
        mutable: true,
        shared: false,
    };
    let init_expr = ConstExpr::i32_const(0);
    globals.global(global_type, &init_expr);
    module.section(&globals);
    
    // Export section: export functions, memory, and global
    let mut exports = ExportSection::new();
    exports.export("get_counter", ExportKind::Func, 0);
    exports.export("set_counter", ExportKind::Func, 1);
    exports.export("store_at", ExportKind::Func, 2);
    exports.export("load_from", ExportKind::Func, 3);
    exports.export("memory", ExportKind::Memory, 0);
    exports.export("counter", ExportKind::Global, 0);
    module.section(&exports);
    
    // Code section: implement all functions
    let mut code = CodeSection::new();
    
    // get_counter: return global 0
    let mut get_counter = Function::new(vec![]);
    get_counter.instruction(&Instruction::GlobalGet(0));
    get_counter.instruction(&Instruction::End);
    code.function(&get_counter);
    
    // set_counter: set global 0 to parameter
    let mut set_counter = Function::new(vec![]);
    set_counter.instruction(&Instruction::LocalGet(0));
    set_counter.instruction(&Instruction::GlobalSet(0));
    set_counter.instruction(&Instruction::End);
    code.function(&set_counter);
    
    // store_at: store value at memory address
    let mut store_at = Function::new(vec![]);
    store_at.instruction(&Instruction::LocalGet(0)); // address
    store_at.instruction(&Instruction::LocalGet(1)); // value
    store_at.instruction(&Instruction::I32Store(MemArg {
        offset: 0,
        align: 2, // 4-byte alignment for i32
        memory_index: 0,
    }));
    store_at.instruction(&Instruction::End);
    code.function(&store_at);
    
    // load_from: load value from memory address
    let mut load_from = Function::new(vec![]);
    load_from.instruction(&Instruction::LocalGet(0)); // address
    load_from.instruction(&Instruction::I32Load(MemArg {
        offset: 0,
        align: 2, // 4-byte alignment for i32
        memory_index: 0,
    }));
    load_from.instruction(&Instruction::End);
    code.function(&load_from);
    
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // Validate the generated WASM
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Stateful module should be valid");
}

#[test]
fn test_section_ordering_validation() {
    // Test that sections must be added in correct order
    let mut module = Module::new();
    
    // Correct order should work
    let mut types = TypeSection::new();
    types.ty().function(vec![], vec![ValType::I32]);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut exports = ExportSection::new();
    exports.export("test", ExportKind::Func, 0);
    module.section(&exports);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    func.instruction(&Instruction::I32Const(1));
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Correctly ordered module should be valid");
}

#[test]
fn test_function_with_local_variables() {
    let mut module = Module::new();
    
    // Function type: (i32) -> i32
    let mut types = TypeSection::new();
    types.ty().function(vec![ValType::I32], vec![ValType::I32]);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut exports = ExportSection::new();
    exports.export("double_plus_one", ExportKind::Func, 0);
    module.section(&exports);
    
    // Function with one local variable
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![(1, ValType::I32)]); // 1 local i32 variable
    
    // local[1] = param[0] * 2
    func.instruction(&Instruction::LocalGet(0)); // get parameter
    func.instruction(&Instruction::I32Const(2));
    func.instruction(&Instruction::I32Mul);
    func.instruction(&Instruction::LocalSet(1)); // store in local
    
    // return local[1] + 1
    func.instruction(&Instruction::LocalGet(1));
    func.instruction(&Instruction::I32Const(1));
    func.instruction(&Instruction::I32Add);
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Function with locals should be valid");
}

#[test]
fn test_empty_module() {
    let module = Module::new();
    let wasm_bytes = module.finish();
    
    // Even empty modules should be valid WASM
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Empty module should be valid");
    
    // Should have WASM magic number and version
    assert_eq!(&wasm_bytes[0..4], &[0x00, 0x61, 0x73, 0x6d]); // "\0asm"
    assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00]); // version 1
}

#[test]
fn test_multiple_function_types() {
    let mut module = Module::new();
    
    // Multiple function types
    let mut types = TypeSection::new();
    types.ty().function(vec![], vec![ValType::I32]); // type 0: () -> i32
    types.ty().function(vec![ValType::I32, ValType::I32], vec![ValType::I32]); // type 1: (i32, i32) -> i32
    types.ty().function(vec![ValType::F32], vec![ValType::F32]); // type 2: (f32) -> f32
    module.section(&types);
    
    // Multiple functions
    let mut functions = FunctionSection::new();
    functions.function(0); // function 0 uses type 0
    functions.function(1); // function 1 uses type 1
    functions.function(2); // function 2 uses type 2
    module.section(&functions);
    
    // Export all functions
    let mut exports = ExportSection::new();
    exports.export("get_constant", ExportKind::Func, 0);
    exports.export("add_ints", ExportKind::Func, 1);
    exports.export("square_float", ExportKind::Func, 2);
    module.section(&exports);
    
    // Implement all functions
    let mut code = CodeSection::new();
    
    // Function 0: return constant
    let mut func0 = Function::new(vec![]);
    func0.instruction(&Instruction::I32Const(100));
    func0.instruction(&Instruction::End);
    code.function(&func0);
    
    // Function 1: add two integers
    let mut func1 = Function::new(vec![]);
    func1.instruction(&Instruction::LocalGet(0));
    func1.instruction(&Instruction::LocalGet(1));
    func1.instruction(&Instruction::I32Add);
    func1.instruction(&Instruction::End);
    code.function(&func1);
    
    // Function 2: square a float
    let mut func2 = Function::new(vec![]);
    func2.instruction(&Instruction::LocalGet(0));
    func2.instruction(&Instruction::LocalGet(0));
    func2.instruction(&Instruction::F32Mul);
    func2.instruction(&Instruction::End);
    code.function(&func2);
    
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    let mut validator = Validator::new();
    validator.validate_all(&wasm_bytes).expect("Multi-function module should be valid");
}

#[test]
fn test_function_code_count_mismatch_validation() {
    let mut module = Module::new();
    
    let mut types = TypeSection::new();
    types.ty().function(vec![], vec![ValType::I32]);
    module.section(&types);
    
    // Declare 2 functions
    let mut functions = FunctionSection::new();
    functions.function(0);
    functions.function(0);
    module.section(&functions);
    
    // But only implement 1 function
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    func.instruction(&Instruction::I32Const(42));
    func.instruction(&Instruction::End);
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    
    // This should fail validation
    let mut validator = Validator::new();
    let result = validator.validate_all(&wasm_bytes);
    assert!(result.is_err(), "Module with function/code mismatch should be invalid");
}