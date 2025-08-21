use crate::compiler::mir::build_mir::MIR;
use crate::compiler::parsers::tokens::TextLocation;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::parsers::ast_nodes::AstNode;
use crate::return_compiler_error;

// Assumes that the AST being passed in is an entire complete module.
// All dependencies should be declared inside this module.
pub fn new_wasm_module(mir: MIR) -> Result<Vec<u8>, CompileError> {
    let module = WasmModule::new();

    // TODO: loop through the ast and use wasm_encoder and add to each section of the WasmModule

    // Build the final wasm module and validate it
    let compiled_wasm = module.finish();
    match wasmparser::validate(&compiled_wasm) {
        Ok(_) => Ok(compiled_wasm),
        Err(e) => return_compiler_error!(
            "Failed to validate final wasm output: {e}"
        ),
    }
}