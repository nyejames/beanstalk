/// Debug tests for WASM compilation pipeline
/// 
/// This module contains focused tests to debug the AST → WIR → WASM compilation issues
/// identified in task 10.

use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::wir::wir_nodes::{WirFunction, WirBlock, Statement, Terminator, Rvalue, Operand, Constant};
use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::compiler_errors::CompileError;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_minimal_wasm_function_generation() {
        println!("Testing minimal WASM function generation...");
        
        // First, let's test creating a minimal WASM function directly with wasm_encoder
        // to see what the correct structure should be
        println!("Creating reference WASM function with wasm_encoder...");
        
        use wasm_encoder::{Module, TypeSection, FunctionSection, CodeSection, Function, Instruction, ExportSection, ExportKind};
        
        let mut module = Module::new();
        
        // Add type section (function signature: () -> ())
        let mut types = TypeSection::new();
        types.ty().function(vec![], vec![]);
        module.section(&types);
        
        // Add function section
        let mut functions = FunctionSection::new();
        functions.function(0); // Use type 0
        module.section(&functions);
        
        // Add export section (this might be required)
        let mut exports = ExportSection::new();
        exports.export("main", ExportKind::Func, 0);
        module.section(&exports);
        
        // Add code section
        let mut codes = CodeSection::new();
        let func = Function::new(vec![]); // No locals, no instructions - just an empty function
        codes.function(&func);
        module.section(&codes);
        
        let reference_wasm = module.finish();
        println!("Reference WASM: {} bytes", reference_wasm.len());
        
        match wasmparser::validate(&reference_wasm) {
            Ok(_) => println!("✓ Reference WASM validates successfully"),
            Err(e) => {
                println!("✗ Reference WASM validation failed: {:?}", e);
                
                // Let's also try to parse it to see what's wrong
                let parser = wasmparser::Parser::new(0);
                for payload in parser.parse_all(&reference_wasm) {
                    match payload {
                        Ok(payload) => println!("Reference WASM payload: {:?}", payload),
                        Err(e) => println!("Reference WASM parse error: {:?}", e),
                    }
                }
            }
        }
        
        // Now test our WIR compilation
        println!("\nTesting WIR compilation...");
        
        // Create a minimal WIR function that should compile to valid WASM
        let mut wir_function = WirFunction::new(
            0,
            "main".to_string(),
            vec![], // No parameters
            vec![], // No return values
        );
        
        // Create a single block with just a return terminator
        let mut block = WirBlock::new(0);
        block.set_terminator(Terminator::Return { values: vec![] });
        
        wir_function.add_block(block);
        
        // Try to compile this to WASM
        let mut wasm_module = WasmModule::new();
        
        match wasm_module.compile_function(&wir_function) {
            Ok(()) => {
                println!("✓ WIR function compiled successfully");
                
                // Try to finish without validation first to see the raw WASM
                let wasm_bytes = wasm_module.finish();
                println!("Generated {} bytes of WASM", wasm_bytes.len());
                
                // Compare with reference
                println!("Reference: {} bytes, Generated: {} bytes", reference_wasm.len(), wasm_bytes.len());
                
                // Try to validate manually to see the exact error
                match wasmparser::validate(&wasm_bytes) {
                    Ok(_) => {
                        println!("✓ WASM module validated successfully");
                    }
                    Err(e) => {
                        println!("✗ WASM validation failed: {:?}", e);
                        
                        // Let's also try to parse the WASM to see what's in it
                        let parser = wasmparser::Parser::new(0);
                        for payload in parser.parse_all(&wasm_bytes) {
                            match payload {
                                Ok(payload) => println!("WASM payload: {:?}", payload),
                                Err(e) => println!("WASM parse error: {:?}", e),
                            }
                        }
                        
                        panic!("WASM validation failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ WIR compilation failed: {:?}", e);
                panic!("WIR compilation failed: {:?}", e);
            }
        }
    }

    #[test]
    fn test_function_with_local_variable() {
        println!("Testing WASM function with local variable...");
        
        // Create a WIR function with a local variable assignment
        let mut wir_function = WirFunction::new(
            0,
            "main".to_string(),
            vec![], // No parameters
            vec![], // No return values
        );
        
        // Add a local variable
        let local_place = Place::Local { 
            index: 0, 
            wasm_type: WasmType::I32 
        };
        wir_function.add_local("x".to_string(), local_place.clone());
        
        // Create a block with variable assignment and return
        let mut block = WirBlock::new(0);
        
        // Add assignment: x = 42
        let assign_stmt = Statement::Assign {
            place: local_place,
            rvalue: Rvalue::Use(Operand::Constant(Constant::I32(42))),
        };
        block.add_statement(assign_stmt);
        
        // Add return terminator
        block.set_terminator(Terminator::Return { values: vec![] });
        
        wir_function.add_block(block);
        
        // Try to compile this to WASM
        let mut wasm_module = WasmModule::new();
        
        match wasm_module.compile_function(&wir_function) {
            Ok(()) => {
                println!("✓ WIR function with local compiled successfully");
                
                // Try to finish and validate the module
                match wasm_module.finish_with_validation() {
                    Ok(wasm_bytes) => {
                        println!("✓ WASM module with local variable generated successfully");
                        println!("  Generated {} bytes of WASM", wasm_bytes.len());
                    }
                    Err(e) => {
                        println!("✗ WASM validation failed: {:?}", e);
                        panic!("WASM validation failed: {:?}", e);
                    }
                }
            }
            Err(e) => {
                println!("✗ WIR compilation failed: {:?}", e);
                panic!("WIR compilation failed: {:?}", e);
            }
        }
    }

    #[test]
    fn test_debug_actual_test_case() {
        println!("Testing actual Beanstalk test case compilation...");
        
        // Try to compile the declarations_only.bst test case and see what WIR is generated
        use crate::build::build_project_files;
        use crate::Flag;
        use std::path::Path;
        
        let test_path = Path::new("tests/cases/success/declarations_only.bst");
        let flags = vec![Flag::DisableTimers, Flag::DisableWarnings];
        
        match build_project_files(&test_path, false, &flags) {
            Ok(project) => {
                println!("✓ Test case compiled successfully");
                println!("  Generated {} output files", project.output_files.len());
            }
            Err(errors) => {
                println!("✗ Test case compilation failed:");
                for (i, error) in errors.iter().enumerate() {
                    println!("  Error {}: {:?}", i + 1, error);
                    if i >= 2 { // Limit to first 3 errors
                        break;
                    }
                }
                // Don't panic here - we expect this to fail initially
            }
        }
    }
}

/// Helper function to create a simple WIR function for testing
pub fn create_test_wir_function(name: &str) -> WirFunction {
    let mut function = WirFunction::new(
        0,
        name.to_string(),
        vec![], // No parameters
        vec![], // No return values
    );
    
    // Create a single block with just a return terminator
    let mut block = WirBlock::new(0);
    block.set_terminator(Terminator::Return { values: vec![] });
    
    function.add_block(block);
    function
}

/// Helper function to test WASM compilation of a WIR function
pub fn test_wir_to_wasm_compilation(wir_function: &WirFunction) -> Result<Vec<u8>, CompileError> {
    let mut wasm_module = WasmModule::new();
    wasm_module.compile_function(wir_function)?;
    wasm_module.finish_with_validation()
}