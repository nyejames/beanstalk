// Discover the correct wasm_encoder API

#[test]
fn test_discover_type_section_api() {
    use wasm_encoder::{Module, TypeSection, FuncType, ValType};
    
    let mut module = Module::new();
    let mut types = TypeSection::new();
    
    // Try the correct API based on wasm_encoder 0.238.1
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    
    module.section(&types);
    let _wasm_bytes = module.finish();
}

#[test] 
fn test_discover_function_section_api() {
    use wasm_encoder::{Module, TypeSection, FunctionSection, FuncType, ValType};
    
    let mut module = Module::new();
    
    // Type section
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    // Function section
    let mut functions = FunctionSection::new();
    functions.function(0); // This should work
    module.section(&functions);
    
    let _wasm_bytes = module.finish();
}