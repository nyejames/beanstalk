// Tests for wasm_encoder error handling patterns
// These tests validate our error handling research and examples

use wasm_encoder::{
    Module, TypeSection, FunctionSection, CodeSection, MemorySection,
    Function, FuncType, ValType, Instruction, BlockType, MemArg, MemoryType
};
use wasmparser::Validator;

/// Test basic validation pattern
#[test]
fn test_basic_validation_pattern() {
    let result = build_simple_valid_module();
    assert!(result.is_ok(), "Basic validation should succeed");
    
    let wasm_bytes = result.unwrap();
    assert!(validate_wasm_module(&wasm_bytes).is_ok(), "Module should be valid");
}

/// Test that type mismatches are properly detected
#[test]
fn test_type_mismatch_detection() {
    let result = build_module_with_type_mismatch();
    assert!(result.is_err(), "Type mismatch should be detected");
    
    let error = result.unwrap_err();
    let error_str = error.to_string();
    assert!(
        error_str.contains("type") || error_str.contains("mismatch") || error_str.contains("stack"),
        "Error should mention type/mismatch/stack issues: {}",
        error_str
    );
}

/// Test that invalid control flow is detected
#[test]
fn test_control_flow_validation() {
    let result = build_module_with_invalid_control_flow();
    assert!(result.is_err(), "Invalid control flow should be detected");
}

/// Test that memory operations without memory section are detected
#[test]
fn test_memory_validation() {
    let result = build_module_with_invalid_memory_access();
    assert!(result.is_err(), "Invalid memory access should be detected");
}

/// Test that missing sections are detected
#[test]
fn test_missing_section_detection() {
    let result = build_module_with_missing_sections();
    assert!(result.is_err(), "Missing sections should be detected");
}

/// Test error recovery patterns
#[test]
fn test_error_recovery_pattern() {
    // This should succeed by falling back to a simpler approach
    let result = build_module_with_fallback();
    assert!(result.is_ok(), "Error recovery should produce valid module");
}

/// Test builder pattern with validation
#[test]
fn test_safe_builder_pattern() {
    let mut builder = SafeModuleBuilder::new();
    
    // Add valid function type and function
    let type_index = builder.add_function_type(vec![], vec![ValType::I32]);
    assert!(builder.add_function(type_index).is_ok());
    
    // Build should succeed
    let result = builder.build_with_simple_functions();
    assert!(result.is_ok(), "Safe builder should produce valid module");
}

/// Test builder pattern error handling
#[test]
fn test_builder_error_handling() {
    let mut builder = SafeModuleBuilder::new();
    
    // Try to add function with invalid type index
    let result = builder.add_function(99);
    assert!(result.is_err(), "Invalid type index should be rejected");
    
    // Try to add memory twice
    assert!(builder.add_memory(1).is_ok());
    assert!(builder.add_memory(1).is_err(), "Duplicate memory should be rejected");
}

// Helper functions for building test modules

fn build_simple_valid_module() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
    validate_wasm_module(&wasm_bytes)?;
    Ok(wasm_bytes)
}

fn build_module_with_type_mismatch() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
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
    validate_wasm_module(&wasm_bytes)?;
    Ok(wasm_bytes)
}

fn build_module_with_invalid_control_flow() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut module = Module::new();
    
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // ERROR: Branch to non-existent label
    func.instruction(&Instruction::Br(5)); // No such label depth
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    validate_wasm_module(&wasm_bytes)?;
    Ok(wasm_bytes)
}

fn build_module_with_invalid_memory_access() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut module = Module::new();
    
    let mut types = TypeSection::new();
    let func_type = FuncType::new(vec![], vec![ValType::I32]);
    types.ty().func_type(&func_type);
    module.section(&types);
    
    let mut functions = FunctionSection::new();
    functions.function(0);
    module.section(&functions);
    
    let mut code = CodeSection::new();
    let mut func = Function::new(vec![]);
    
    // ERROR: Try to load from memory without declaring memory
    func.instruction(&Instruction::I32Const(0)); // Address
    func.instruction(&Instruction::I32Load(MemArg { offset: 0, align: 2, memory_index: 0 }));
    func.instruction(&Instruction::End);
    
    code.function(&func);
    module.section(&code);
    
    let wasm_bytes = module.finish();
    validate_wasm_module(&wasm_bytes)?;
    Ok(wasm_bytes)
}

fn build_module_with_missing_sections() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    let mut module = Module::new();
    
    // Create function section without corresponding type section
    let mut functions = FunctionSection::new();
    functions.function(0); // Reference type 0, but no type section exists
    module.section(&functions);
    
    let wasm_bytes = module.finish();
    validate_wasm_module(&wasm_bytes)?;
    Ok(wasm_bytes)
}

fn build_module_with_fallback() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // Try complex approach first
    match build_complex_module() {
        Ok(wasm_bytes) => return Ok(wasm_bytes),
        Err(_) => {
            // Fall back to simple approach
            return build_simple_valid_module();
        }
    }
}

fn build_complex_module() -> Result<Vec<u8>, Box<dyn std::error::Error>> {
    // This intentionally fails to test fallback
    let mut module = Module::new();
    
    let mut functions = FunctionSection::new();
    functions.function(99); // Invalid type index
    module.section(&functions);
    
    let wasm_bytes = module.finish();
    validate_wasm_module(&wasm_bytes)?;
    Ok(wasm_bytes)
}

fn validate_wasm_module(wasm_bytes: &[u8]) -> Result<(), wasmparser::BinaryReaderError> {
    let mut validator = Validator::new();
    validator.validate_all(wasm_bytes).map(|_| ())
}

// Safe module builder for testing
pub struct SafeModuleBuilder {
    module: Module,
    types: Vec<(Vec<ValType>, Vec<ValType>)>,
    functions: Vec<u32>,
    has_memory: bool,
}

impl SafeModuleBuilder {
    pub fn new() -> Self {
        Self {
            module: Module::new(),
            types: Vec::new(),
            functions: Vec::new(),
            has_memory: false,
        }
    }
    
    pub fn add_function_type(&mut self, params: Vec<ValType>, results: Vec<ValType>) -> u32 {
        let type_index = self.types.len() as u32;
        self.types.push((params, results));
        type_index
    }
    
    pub fn add_function(&mut self, type_index: u32) -> Result<(), String> {
        if type_index >= self.types.len() as u32 {
            return Err(format!("Invalid type index: {}", type_index));
        }
        self.functions.push(type_index);
        Ok(())
    }
    
    pub fn add_memory(&mut self, min_pages: u64) -> Result<(), String> {
        if self.has_memory {
            return Err("Memory already added".to_string());
        }
        
        let mut memory = MemorySection::new();
        memory.memory(MemoryType {
            minimum: min_pages,
            maximum: None,
            memory64: false,
            shared: false,
            page_size_log2: None,
        });
        self.module.section(&memory);
        self.has_memory = true;
        Ok(())
    }
    
    pub fn build_with_simple_functions(mut self) -> Result<Vec<u8>, String> {
        // Add type section
        if !self.types.is_empty() {
            let mut types_section = TypeSection::new();
            for (params, results) in &self.types {
                let func_type = FuncType::new(params.clone(), results.clone());
                types_section.ty().func_type(&func_type);
            }
            self.module.section(&types_section);
        }
        
        // Add function section
        if !self.functions.is_empty() {
            let mut functions_section = FunctionSection::new();
            for &type_index in &self.functions {
                functions_section.function(type_index);
            }
            self.module.section(&functions_section);
        }
        
        // Add code section with simple implementations
        if !self.functions.is_empty() {
            let mut code_section = CodeSection::new();
            
            for &type_index in &self.functions {
                let (params, results) = &self.types[type_index as usize];
                let func = self.create_simple_function(params, results)?;
                code_section.function(&func);
            }
            
            self.module.section(&code_section);
        }
        
        // Generate and validate
        let wasm_bytes = self.module.finish();
        validate_wasm_module(&wasm_bytes)
            .map_err(|e| format!("Module validation failed: {}", e))?;
        
        Ok(wasm_bytes)
    }
    
    fn create_simple_function(&self, params: &[ValType], results: &[ValType]) -> Result<Function, String> {
        let mut func = Function::new(vec![]); // No additional locals
        
        match (params.len(), results.len()) {
            (0, 0) => {
                // () -> ()
                func.instruction(&Instruction::End);
            }
            (0, 1) => {
                // () -> T
                match results[0] {
                    ValType::I32 => func.instruction(&Instruction::I32Const(42)),
                    ValType::I64 => func.instruction(&Instruction::I64Const(42)),
                    ValType::F32 => func.instruction(&Instruction::F32Const(3.14.into())),
                    ValType::F64 => func.instruction(&Instruction::F64Const(3.14.into())),
                    _ => return Err("Unsupported result type".to_string()),
                };
                func.instruction(&Instruction::End);
            }
            (1, 1) if params[0] == results[0] => {
                // T -> T (identity function)
                func.instruction(&Instruction::LocalGet(0));
                func.instruction(&Instruction::End);
            }
            _ => {
                // For other cases, just return a default value
                if !results.is_empty() {
                    match results[0] {
                        ValType::I32 => func.instruction(&Instruction::I32Const(0)),
                        ValType::I64 => func.instruction(&Instruction::I64Const(0)),
                        ValType::F32 => func.instruction(&Instruction::F32Const(0.0.into())),
                        ValType::F64 => func.instruction(&Instruction::F64Const(0.0.into())),
                        _ => return Err("Unsupported result type".to_string()),
                    };
                }
                func.instruction(&Instruction::End);
            }
        }
        
        Ok(func)
    }
}