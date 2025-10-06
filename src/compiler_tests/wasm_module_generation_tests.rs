use crate::compiler::wir::wir_nodes::{WIR, WirFunction, WirBlock, Statement, Terminator, Operand, Constant};
use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::codegen::build_wasm::new_wasm_module;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_wasm_module_generation() {
        println!("Testing empty WASM module generation...");
        
        // Create an empty WIR structure
        let wir = WIR::new();
        
        // Generate WASM module
        match new_wasm_module(wir) {
            Ok(wasm_bytes) => {
                println!("✅ Successfully generated empty WASM module!");
                println!("   Module size: {} bytes", wasm_bytes.len());
                
                // Basic validation - check if it starts with WASM magic number
                assert!(wasm_bytes.len() >= 4, "WASM module too small");
                assert_eq!(&wasm_bytes[0..4], b"\0asm", "WASM module missing magic number");
                
                // Check version (should be 0x01 0x00 0x00 0x00)
                assert!(wasm_bytes.len() >= 8, "WASM module missing version");
                assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00], "WASM module has incorrect version");
                
                println!("✅ Empty WASM module generation test passed!");
            }
            Err(e) => {
                panic!("❌ Failed to generate empty WASM module: {:?}", e);
            }
        }
    }

    #[test]
    fn test_simple_function_wasm_module_generation() {
        println!("Testing simple function WASM module generation...");
        
        // Create a minimal WIR structure
        let mut wir = WIR::new();
        
        // Create a simple function that returns 42 directly
        let mut function = WirFunction::new(
            0,
            "test_function".to_string(),
            vec![], // No parameters
            vec![WasmType::I32], // Returns i32
        );
        
        // Create a simple block that returns 42 directly (no local variables)
        let mut block = WirBlock::new(0);
        
        // Just return the constant directly
        block.set_terminator(Terminator::Return {
            values: vec![Operand::Constant(Constant::I32(42))],
        });
        
        function.add_block(block);
        wir.add_function(function);
        
        // Generate WASM module
        match new_wasm_module(wir) {
            Ok(wasm_bytes) => {
                println!("✅ Successfully generated WASM module with function!");
                println!("   Module size: {} bytes", wasm_bytes.len());
                
                // Basic validation - check if it starts with WASM magic number
                assert!(wasm_bytes.len() >= 4, "WASM module too small");
                assert_eq!(&wasm_bytes[0..4], b"\0asm", "WASM module missing magic number");
                
                // Check version (should be 0x01 0x00 0x00 0x00)
                assert!(wasm_bytes.len() >= 8, "WASM module missing version");
                assert_eq!(&wasm_bytes[4..8], &[0x01, 0x00, 0x00, 0x00], "WASM module has incorrect version");
                
                // The module should be larger than an empty module since it contains a function
                assert!(wasm_bytes.len() > 20, "WASM module with function should be larger");
                
                println!("✅ Simple function WASM module generation test passed!");
            }
            Err(e) => {
                panic!("❌ Failed to generate WASM module with function: {:?}", e);
            }
        }
    }

    #[test]
    fn test_wasm_module_validation() {
        println!("Testing WASM module validation...");
        
        // Create a WIR with a simple function
        let mut wir = WIR::new();
        
        let mut function = WirFunction::new(
            0,
            "add_function".to_string(),
            vec![
                Place::Local { index: 0, wasm_type: WasmType::I32 },
                Place::Local { index: 1, wasm_type: WasmType::I32 },
            ], // Two i32 parameters
            vec![WasmType::I32], // Returns i32
        );
        
        // Create a block that adds the two parameters and returns the result
        let mut block = WirBlock::new(0);
        
        // Add statement: result = param0 + param1
        block.add_statement(Statement::Assign {
            place: Place::Local { index: 2, wasm_type: WasmType::I32 },
            rvalue: crate::compiler::wir::wir_nodes::Rvalue::BinaryOp(
                crate::compiler::wir::wir_nodes::BinOp::Add,
                Operand::Copy(Place::Local { index: 0, wasm_type: WasmType::I32 }),
                Operand::Copy(Place::Local { index: 1, wasm_type: WasmType::I32 }),
            ),
        });
        
        // Return the result
        block.set_terminator(Terminator::Return {
            values: vec![Operand::Copy(Place::Local { index: 2, wasm_type: WasmType::I32 })],
        });
        
        function.add_block(block);
        wir.add_function(function);
        
        // Generate and validate WASM module
        match new_wasm_module(wir) {
            Ok(wasm_bytes) => {
                println!("✅ Successfully generated WASM module with arithmetic!");
                println!("   Module size: {} bytes", wasm_bytes.len());
                
                // Validate using wasmparser
                match wasmparser::validate(&wasm_bytes) {
                    Ok(_) => {
                        println!("✅ WASM module validation passed!");
                    }
                    Err(e) => {
                        panic!("❌ WASM module validation failed: {}", e);
                    }
                }
            }
            Err(e) => {
                panic!("❌ Failed to generate WASM module with arithmetic: {:?}", e);
            }
        }
    }
}