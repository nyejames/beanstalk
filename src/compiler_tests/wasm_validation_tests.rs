use crate::compiler::codegen::build_wasm::new_wasm_module;
use crate::compiler::mir::mir::borrow_check_pipeline;
use crate::compiler::parsers::build_ast::AstBlock;
use std::path::PathBuf;

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
    
    // Verify WASM passes basic validation using wasmparser
    wasmparser::validate(&wasm_bytes).expect("Generated WASM should pass validation");
}