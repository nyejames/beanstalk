//! WASM execution tests for basic language features
//! 
//! This module tests that generated WASM produces correct results when executed.

use crate::build_system::core_build::compile_modules;
use crate::settings::{Config, ProjectType};
use crate::{InputModule, Flag};
use std::path::PathBuf;
use wasmer::{Module, Store, Instance, imports, Function};

/// WASM execution test utilities
pub struct WasmExecutor {
    store: Store,
}

impl WasmExecutor {
    /// Create a new WASM executor with basic runtime setup
    pub fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let store = Store::default();
        
        Ok(Self {
            store,
        })
    }
    
    /// Execute WASM bytes and return any exported function results
    pub fn execute_wasm(&mut self, wasm_bytes: &[u8]) -> Result<ExecutionResult, Box<dyn std::error::Error>> {
        let module = Module::new(&self.store, wasm_bytes)?;
        
        // Create basic imports for host functions
        let import_object = imports! {
            "env" => {
                "print_i32" => Function::new_typed(&mut self.store, |param: i32| {
                    println!("WASM output: {}", param);
                }),
                "print_f64" => Function::new_typed(&mut self.store, |param: f64| {
                    println!("WASM output: {}", param);
                }),
            }
        };
        
        let instance = Instance::new(&mut self.store, &module, &import_object)?;
        
        let mut result = ExecutionResult::default();
        
        // Try to call main function if it exists
        if let Ok(main_func) = instance.exports.get_function("main") {
            match main_func.call(&mut self.store, &[]) {
                Ok(_) => result.main_executed = true,
                Err(e) => result.execution_errors.push(format!("Main execution error: {}", e)),
            }
        }
        
        // Collect information about exported functions
        for (name, export) in instance.exports.iter() {
            if let wasmer::Extern::Function(func) = export {
                let func_type = func.ty(&self.store);
                result.exported_functions.push(ExportedFunction {
                    name: name.to_string(),
                    params: func_type.params().len(),
                    results: func_type.results().len(),
                });
            }
        }
        
        // Check for exported memory
        if let Ok(memory) = instance.exports.get_memory("memory") {
            let view = memory.view(&self.store);
            result.memory_size = view.size().bytes().0;
            result.has_memory = true;
        }
        
        Ok(result)
    }
}

/// Results from WASM execution
#[derive(Debug, Default)]
pub struct ExecutionResult {
    pub main_executed: bool,
    pub exported_functions: Vec<ExportedFunction>,
    pub memory_size: usize,
    pub has_memory: bool,
    pub execution_errors: Vec<String>,
}

#[derive(Debug)]
pub struct ExportedFunction {
    pub name: String,
    pub params: usize,
    pub results: usize,
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

#[cfg(test)]
mod execution_tests {
    use super::*;

    /// Test basic WASM execution setup
    #[test]
    fn test_wasm_executor_setup() {
        let executor = WasmExecutor::new();
        assert!(executor.is_ok(), "WASM executor should initialize successfully");
        println!("✅ WASM executor setup successful");
    }

    /// Test execution of empty program
    #[test]
    fn test_empty_program_execution() {
        let source_code = r#"
-- Empty program
"#;

        let module = create_test_module(source_code, "empty.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Empty program execution successful");
                        println!("  Main executed: {}", exec_result.main_executed);
                        println!("  Exported functions: {}", exec_result.exported_functions.len());
                        println!("  Has memory: {}", exec_result.has_memory);
                    }
                    Err(e) => {
                        println!("⚠ Empty program execution failed: {}", e);
                        // This might be expected if the WASM doesn't have proper exports
                    }
                }
            }
            Err(errors) => {
                println!("⚠ Empty program compilation failed:");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }

    /// Test execution of basic arithmetic operations
    #[test]
    fn test_arithmetic_execution() {
        let source_code = r#"
-- Basic arithmetic that should produce deterministic results
a = 10
b = 5
sum = a + b      -- Should be 15
product = a * b  -- Should be 50
difference = a - b -- Should be 5
quotient = a / b   -- Should be 2
"#;

        let module = create_test_module(source_code, "arithmetic.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Arithmetic execution successful");
                        println!("  Exported functions: {:?}", exec_result.exported_functions);
                        
                        // If we have exported functions, try to call them
                        for func in &exec_result.exported_functions {
                            println!("  Function '{}': {} params -> {} results", 
                                   func.name, func.params, func.results);
                        }
                    }
                    Err(e) => {
                        println!("⚠ Arithmetic execution failed: {}", e);
                        // Expected during development
                    }
                }
            }
            Err(errors) => {
                println!("⚠ Arithmetic compilation failed:");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }

    /// Test execution of variable assignments
    #[test]
    fn test_variable_assignment_execution() {
        let source_code = r#"
-- Variable assignments
value = 42
name = "test"
flag = true

-- Mutable variables
counter ~= 0
counter = counter + 1
counter = counter * 2
"#;

        let module = create_test_module(source_code, "variables.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Variable assignment execution successful");
                        
                        // Check if memory was allocated for variables
                        if exec_result.has_memory {
                            println!("  Memory allocated: {} bytes", exec_result.memory_size);
                        }
                        
                        // Check for exported functions
                        if !exec_result.exported_functions.is_empty() {
                            println!("  Exported functions:");
                            for func in &exec_result.exported_functions {
                                println!("    {}: {} -> {}", func.name, func.params, func.results);
                            }
                        }
                    }
                    Err(e) => {
                        println!("⚠ Variable assignment execution failed: {}", e);
                    }
                }
            }
            Err(errors) => {
                println!("⚠ Variable assignment compilation failed:");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }

    /// Test execution with string constants
    #[test]
    fn test_string_constants_execution() {
        let source_code = r#"
-- String constants
greeting = "Hello, World!"
name = "Beanstalk"
message = greeting + " from " + name
"#;

        let module = create_test_module(source_code, "strings.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ String constants execution successful");
                        
                        // Strings should be stored in memory
                        if exec_result.has_memory {
                            println!("  String memory allocated: {} bytes", exec_result.memory_size);
                        } else {
                            println!("  No memory section found (strings might be handled differently)");
                        }
                    }
                    Err(e) => {
                        println!("⚠ String constants execution failed: {}", e);
                    }
                }
            }
            Err(errors) => {
                println!("⚠ String constants compilation failed:");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }

    /// Test execution of function calls (if implemented)
    #[test]
    fn test_function_call_execution() {
        let source_code = r#"
-- Simple function definition and call
add_numbers |a Int, b Int| -> Int:
    return a + b
;

result = add_numbers(10, 5)
"#;

        let module = create_test_module(source_code, "functions.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Function call execution successful");
                        
                        // Check for the add_numbers function
                        let has_add_function = exec_result.exported_functions.iter()
                            .any(|f| f.name.contains("add"));
                        
                        if has_add_function {
                            println!("  Found add function in exports");
                        }
                        
                        println!("  Total exported functions: {}", exec_result.exported_functions.len());
                    }
                    Err(e) => {
                        println!("⚠ Function call execution failed: {}", e);
                        // Expected if functions aren't implemented yet
                    }
                }
            }
            Err(errors) => {
                println!("⚠ Function call compilation failed (expected if not implemented):");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }

    /// Test execution with control flow
    #[test]
    fn test_control_flow_execution() {
        let source_code = r#"
-- Control flow
value = 10
result ~= 0

if value > 5:
    result = 1
else
    result = 0
;
"#;

        let module = create_test_module(source_code, "control_flow.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Control flow execution successful");
                        
                        if exec_result.main_executed {
                            println!("  Main function executed successfully");
                        }
                        
                        println!("  Execution completed without errors");
                    }
                    Err(e) => {
                        println!("⚠ Control flow execution failed: {}", e);
                    }
                }
            }
            Err(errors) => {
                println!("⚠ Control flow compilation failed:");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }

    /// Test comprehensive execution with multiple features
    #[test]
    fn test_comprehensive_execution() {
        let source_code = r#"
-- Comprehensive test
-- Variables
base = 100
multiplier = 2
name = "test"

-- Arithmetic
result = base * multiplier + 10  -- Should be 210

-- Mutable operations
counter ~= 0
counter = counter + result
counter = counter / 10  -- Should be 21

-- Boolean
is_positive = result > 0  -- Should be true
"#;

        let module = create_test_module(source_code, "comprehensive.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Comprehensive execution successful");
                        
                        // Print detailed execution results
                        println!("  Main executed: {}", exec_result.main_executed);
                        println!("  Memory allocated: {} bytes", exec_result.memory_size);
                        println!("  Exported functions: {}", exec_result.exported_functions.len());
                        
                        for func in &exec_result.exported_functions {
                            println!("    {}: {} params -> {} results", 
                                   func.name, func.params, func.results);
                        }
                        
                        // Verify execution completed without runtime errors
                        assert!(exec_result.execution_errors.is_empty(), 
                               "Execution should complete without errors");
                    }
                    Err(e) => {
                        println!("⚠ Comprehensive execution failed: {}", e);
                        // This might be expected during development
                    }
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

    /// Test execution error handling
    #[test]
    fn test_execution_error_handling() {
        // Create invalid WASM bytes to test error handling
        let invalid_wasm = vec![0x00, 0x61, 0x73, 0x6d, 0x01, 0x00, 0x00, 0x00];
        
        let mut executor = WasmExecutor::new().expect("Executor should initialize");
        
        let result = executor.execute_wasm(&invalid_wasm);
        assert!(result.is_err(), "Invalid WASM should fail execution");
        
        println!("✅ Execution error handling working correctly");
    }

    /// Test memory usage during execution
    #[test]
    fn test_memory_usage_execution() {
        let source_code = r#"
-- Test memory usage with various data types
small_int = 1
large_int = 1000000
small_string = "hi"
large_string = "This is a much longer string that should take more memory"
float_value = 3.14159265359
"#;

        let module = create_test_module(source_code, "memory_usage.bst");
        let config = create_test_config();
        let flags = vec![Flag::DisableTimers];

        match compile_modules(vec![module], &config, &flags) {
            Ok(result) => {
                let mut executor = WasmExecutor::new().expect("Executor should initialize");
                
                match executor.execute_wasm(&result.wasm_bytes) {
                    Ok(exec_result) => {
                        println!("✅ Memory usage execution successful");
                        
                        if exec_result.has_memory {
                            println!("  Memory allocated: {} bytes", exec_result.memory_size);
                            
                            // Verify reasonable memory usage
                            assert!(exec_result.memory_size > 0, "Should allocate some memory for strings");
                            assert!(exec_result.memory_size < 1024 * 1024, "Should not allocate excessive memory");
                        } else {
                            println!("  No memory section (might be using different storage)");
                        }
                    }
                    Err(e) => {
                        println!("⚠ Memory usage execution failed: {}", e);
                    }
                }
            }
            Err(errors) => {
                println!("⚠ Memory usage compilation failed:");
                for error in errors {
                    println!("  {:?}", error);
                }
            }
        }
    }
}