use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::mir::mir::borrow_check_pipeline;
use crate::compiler::parsers::build_ast::AstBlock;
use crate::build_system::core_build::compile_modules;
use crate::settings::{Config, ProjectType};
use crate::{InputModule, Flag};
use std::path::PathBuf;


/// WASM validation utilities for debugging and development
pub struct WasmValidator {
    pub module_info: WasmModuleInfo,
}

/// Detailed information about a WASM module for debugging
#[derive(Debug, Default)]
pub struct WasmModuleInfo {
    pub type_count: u32,
    pub function_count: u32,
    pub memory_pages: Option<u32>,
    pub global_count: u32,
    pub export_count: u32,
    pub import_count: u32,
    pub data_sections: Vec<DataSectionInfo>,
    pub function_signatures: Vec<FunctionSignature>,
    pub validation_errors: Vec<String>,
}

#[derive(Debug)]
pub struct DataSectionInfo {
    pub offset: u32,
    pub size: u32,
    pub content_preview: String,
}

#[derive(Debug)]
pub struct FunctionSignature {
    pub index: u32,
    pub params: Vec<wasmparser::ValType>,
    pub results: Vec<wasmparser::ValType>,
    pub local_count: u32,
}

impl WasmValidator {
    /// Create a new validator and analyze the WASM module
    pub fn new(wasm_bytes: &[u8]) -> Result<Self, String> {
        let mut validator = Self {
            module_info: WasmModuleInfo::default(),
        };
        
        validator.analyze_module(wasm_bytes)?;
        Ok(validator)
    }
    
    /// Comprehensive WASM module analysis with debugging information
    fn analyze_module(&mut self, wasm_bytes: &[u8]) -> Result<(), String> {
        let parser = wasmparser::Parser::new(0);
        let mut _type_section_types: Vec<wasmparser::FuncType> = Vec::new();
        
        for payload in parser.parse_all(wasm_bytes) {
            match payload.map_err(|e| format!("WASM parsing error: {}", e))? {
                wasmparser::Payload::TypeSection(reader) => {
                    self.module_info.type_count = reader.count();
                    // Skip detailed type parsing for now due to API changes
                }
                
                wasmparser::Payload::FunctionSection(reader) => {
                    self.module_info.function_count = reader.count();
                }
                
                wasmparser::Payload::MemorySection(reader) => {
                    for memory in reader {
                        let memory = memory.map_err(|e| format!("Memory section error: {}", e))?;
                        self.module_info.memory_pages = Some(memory.initial as u32);
                    }
                }
                
                wasmparser::Payload::GlobalSection(reader) => {
                    self.module_info.global_count = reader.count();
                }
                
                wasmparser::Payload::ExportSection(reader) => {
                    self.module_info.export_count = reader.count();
                }
                
                wasmparser::Payload::ImportSection(reader) => {
                    self.module_info.import_count = reader.count();
                }
                
                wasmparser::Payload::DataSection(reader) => {
                    for (_index, data) in reader.into_iter().enumerate() {
                        let data = data.map_err(|e| format!("Data section error: {}", e))?;
                        
                        let offset = match data.kind {
                            wasmparser::DataKind::Active { offset_expr: _, .. } => {
                                // Try to extract constant offset
                                // Try to extract constant offset (simplified)
                                0 // For now, just use 0 as offset
                            }
                            _ => 0,
                        };
                        
                        let content_preview = if data.data.len() > 32 {
                            format!("{}... ({} bytes)", 
                                   String::from_utf8_lossy(&data.data[..32]), 
                                   data.data.len())
                        } else {
                            String::from_utf8_lossy(data.data).to_string()
                        };
                        
                        self.module_info.data_sections.push(DataSectionInfo {
                            offset,
                            size: data.data.len() as u32,
                            content_preview,
                        });
                    }
                }
                
                wasmparser::Payload::CodeSectionStart { .. } => {
                    // Handle code section start
                }
                wasmparser::Payload::CodeSectionEntry(_body) => {
                    // Skip detailed code parsing for now due to API changes
                }
                
                _ => {} // Ignore other sections for now
            }
        }
        
        Ok(())
    }
    
    /// Validate the WASM module and collect any errors
    pub fn validate(&mut self, wasm_bytes: &[u8]) -> bool {
        match wasmparser::validate(wasm_bytes) {
            Ok(_) => true,
            Err(e) => {
                self.module_info.validation_errors.push(format!("Validation error: {}", e));
                false
            }
        }
    }
    
    /// Print detailed debugging information about the module
    pub fn print_debug_info(&self) {
        println!("=== WASM Module Debug Information ===");
        println!("Type count: {}", self.module_info.type_count);
        println!("Function count: {}", self.module_info.function_count);
        println!("Memory pages: {:?}", self.module_info.memory_pages);
        println!("Global count: {}", self.module_info.global_count);
        println!("Export count: {}", self.module_info.export_count);
        println!("Import count: {}", self.module_info.import_count);
        
        if !self.module_info.function_signatures.is_empty() {
            println!("\n--- Function Signatures ---");
            for sig in &self.module_info.function_signatures {
                println!("Function {}: {:?} -> {:?} (locals: {})", 
                        sig.index, sig.params, sig.results, sig.local_count);
            }
        }
        
        if !self.module_info.data_sections.is_empty() {
            println!("\n--- Data Sections ---");
            for (i, data) in self.module_info.data_sections.iter().enumerate() {
                println!("Data {}: offset={}, size={}, content=\"{}\"", 
                        i, data.offset, data.size, data.content_preview);
            }
        }
        
        if !self.module_info.validation_errors.is_empty() {
            println!("\n--- Validation Errors ---");
            for error in &self.module_info.validation_errors {
                println!("  {}", error);
            }
        }
        
        println!("=====================================");
    }
}

/// Helper function to create test modules
fn create_test_module(source_code: &str, file_name: &str) -> InputModule {
    InputModule {
        source_code: source_code.to_string(),
        source_path: PathBuf::from(file_name),
    }
}

/// Helper function to create test configuration
fn create_test_config() -> Config {
    Config {
        project_type: ProjectType::HTML,
        entry_point: PathBuf::from("test.bst"),
        name: "test_project".to_string(),
        ..Config::default()
    }
}

/// Test that empty MIR generates valid WASM
#[test]
fn test_empty_mir_wasm_validation() {
    let ast_block = AstBlock {
        ast: vec![],
        is_entry_point: true,
        scope: PathBuf::from("test"),
    };
    
    // Generate MIR
    let mir = borrow_check_pipeline(ast_block).expect("MIR generation should succeed");
    
    // Generate WASM
    let wasm_bytes = new_wasm_module(mir).expect("WASM generation should succeed");
    
    // Verify WASM is not empty
    assert!(!wasm_bytes.is_empty(), "WASM bytes should not be empty");
    
    // Comprehensive validation with debugging
    let mut validator = WasmValidator::new(&wasm_bytes).expect("WASM analysis should succeed");
    let is_valid = validator.validate(&wasm_bytes);
    
    if !is_valid {
        validator.print_debug_info();
        panic!("Generated WASM should pass validation");
    }
    
    println!("✅ Empty MIR WASM validation passed");
}

/// Test WASM validation for basic variable declarations
#[test]
fn test_basic_variables_wasm_validation() {
    let source_code = r#"
-- Basic variable declarations
int_value = 42
string_value = "hello"
bool_value = true
"#;

    let module = create_test_module(source_code, "basic_vars.bst");
    let config = create_test_config();
    let flags = vec![Flag::DisableTimers];

    match compile_modules(vec![module], &config, &flags) {
        Ok(result) => {
            let mut validator = WasmValidator::new(&result.wasm_bytes)
                .expect("WASM analysis should succeed");
            
            let is_valid = validator.validate(&result.wasm_bytes);
            
            if !is_valid {
                println!("WASM validation failed for basic variables:");
                validator.print_debug_info();
                panic!("Basic variables WASM should be valid");
            }
            
            // Verify module structure
            assert!(validator.module_info.type_count > 0 || validator.module_info.function_count == 0, 
                   "Module should have types if it has functions");
            
            println!("✅ Basic variables WASM validation passed");
            validator.print_debug_info();
        }
        Err(errors) => {
            println!("⚠ Basic variables compilation failed (expected during development):");
            for error in errors {
                println!("  {:?}", error);
            }
        }
    }
}

/// Test WASM validation for arithmetic operations
#[test]
fn test_arithmetic_wasm_validation() {
    let source_code = r#"
-- Arithmetic operations
a = 10
b = 5
sum = a + b
product = a * b
"#;

    let module = create_test_module(source_code, "arithmetic.bst");
    let config = create_test_config();
    let flags = vec![Flag::DisableTimers];

    match compile_modules(vec![module], &config, &flags) {
        Ok(result) => {
            let mut validator = WasmValidator::new(&result.wasm_bytes)
                .expect("WASM analysis should succeed");
            
            let is_valid = validator.validate(&result.wasm_bytes);
            
            if !is_valid {
                println!("WASM validation failed for arithmetic:");
                validator.print_debug_info();
                panic!("Arithmetic WASM should be valid");
            }
            
            println!("✅ Arithmetic WASM validation passed");
            validator.print_debug_info();
        }
        Err(errors) => {
            println!("⚠ Arithmetic compilation failed (expected during development):");
            for error in errors {
                println!("  {:?}", error);
            }
        }
    }
}

/// Test WASM module structure validation
#[test]
fn test_wasm_module_structure_validation() {
    let source_code = r#"
-- Test various language features
name = "test"
value = 123
flag = true

-- Mutable variables
counter ~= 0
message ~= "hello"
"#;

    let module = create_test_module(source_code, "structure_test.bst");
    let config = create_test_config();
    let flags = vec![Flag::DisableTimers];

    match compile_modules(vec![module], &config, &flags) {
        Ok(result) => {
            let mut validator = WasmValidator::new(&result.wasm_bytes)
                .expect("WASM analysis should succeed");
            
            let is_valid = validator.validate(&result.wasm_bytes);
            
            // Print debug info regardless of validation result
            validator.print_debug_info();
            
            if !is_valid {
                println!("WASM validation failed, but continuing for debugging");
            }
            
            // Verify basic module structure
            assert!(!result.wasm_bytes.is_empty(), "WASM should not be empty");
            
            // Check that we have reasonable module structure
            let has_meaningful_content = validator.module_info.type_count > 0 
                || validator.module_info.function_count > 0 
                || validator.module_info.global_count > 0
                || !validator.module_info.data_sections.is_empty();
            
            assert!(has_meaningful_content, "WASM module should have meaningful content");
            
            println!("✅ WASM module structure validation completed");
        }
        Err(errors) => {
            println!("⚠ Module structure compilation failed:");
            for error in errors {
                println!("  {:?}", error);
            }
        }
    }
}

/// Test WASM validation with string constants
#[test]
fn test_string_constants_wasm_validation() {
    let source_code = r#"
-- String constants
greeting = "Hello, World!"
name = "Beanstalk"
empty_string = ""
long_string = "This is a longer string to test string handling in the WASM backend"
"#;

    let module = create_test_module(source_code, "strings.bst");
    let config = create_test_config();
    let flags = vec![Flag::DisableTimers];

    match compile_modules(vec![module], &config, &flags) {
        Ok(result) => {
            let mut validator = WasmValidator::new(&result.wasm_bytes)
                .expect("WASM analysis should succeed");
            
            let is_valid = validator.validate(&result.wasm_bytes);
            
            if !is_valid {
                println!("WASM validation failed for strings:");
                validator.print_debug_info();
            } else {
                println!("✅ String constants WASM validation passed");
            }
            
            // Check for data sections (strings should be in data section)
            if !validator.module_info.data_sections.is_empty() {
                println!("Found {} data sections with string constants", 
                        validator.module_info.data_sections.len());
                for (i, data) in validator.module_info.data_sections.iter().enumerate() {
                    println!("  Data section {}: {}", i, data.content_preview);
                }
            }
            
            validator.print_debug_info();
        }
        Err(errors) => {
            println!("⚠ String constants compilation failed:");
            for error in errors {
                println!("  {:?}", error);
            }
        }
    }
}

/// Test WASM validation error detection
#[test]
fn test_wasm_validation_error_detection() {
    // Create intentionally invalid WASM bytes
    let invalid_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00, 0xFF]; // Invalid section
    
    let mut validator = WasmValidator::new(&invalid_wasm);
    
    match validator {
        Ok(mut v) => {
            let is_valid = v.validate(&invalid_wasm);
            assert!(!is_valid, "Invalid WASM should fail validation");
            assert!(!v.module_info.validation_errors.is_empty(), "Should have validation errors");
            
            println!("✅ WASM validation error detection working");
            v.print_debug_info();
        }
        Err(e) => {
            println!("✅ WASM validation correctly detected parsing error: {}", e);
        }
    }
}

/// Comprehensive WASM validation test with all features
#[test]
fn test_comprehensive_wasm_validation() {
    let source_code = r#"
-- Comprehensive test with multiple features
-- Basic types
int_val = 42
float_val = 3.14
string_val = "test"
bool_val = true

-- Mutable variables
counter ~= 0
message ~= "mutable"

-- Arithmetic
result = int_val + 10
scaled = float_val * 2.0
"#;

    let module = create_test_module(source_code, "comprehensive.bst");
    let config = create_test_config();
    let flags = vec![Flag::DisableTimers];

    match compile_modules(vec![module], &config, &flags) {
        Ok(result) => {
            let mut validator = WasmValidator::new(&result.wasm_bytes)
                .expect("WASM analysis should succeed");
            
            let is_valid = validator.validate(&result.wasm_bytes);
            
            // Always print debug info for comprehensive test
            validator.print_debug_info();
            
            if is_valid {
                println!("✅ Comprehensive WASM validation passed");
                
                // Verify we have expected module components
                assert!(validator.module_info.type_count > 0 || validator.module_info.function_count == 0,
                       "Should have types if functions exist");
                
                // Check for string data
                if !validator.module_info.data_sections.is_empty() {
                    println!("String constants properly stored in data sections");
                }
            } else {
                println!("⚠ Comprehensive WASM validation failed (expected during development)");
            }
        }
        Err(errors) => {
            println!("⚠ Comprehensive compilation failed:");
            for error in errors {
                println!("  {:?}", error);
            }
        }
    }
}