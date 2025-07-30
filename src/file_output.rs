use crate::compiler::compiler_errors::CompileError;
use crate::compiler::compiler_errors::ErrorType;
use crate::compiler::parsers::tokens::TextLocation;
use crate::{OutputModule, return_file_errors, return_file_error};
use std::fs;
use std::path::Path;
use wasmparser::validate;

pub fn write_wasm_module(module: OutputModule, file_path: &Path) -> Result<(), CompileError> {
    // If the output directory does not exist, create it
    let parent_dir = match file_path.parent() {
        Some(dir) => dir,
        None => return_file_error!(
            file_path,
            "Error getting parent directory of output file when writing: {:?}",
            file_path
        ),
    };

    // Create the necessary directory if it doesn't exist
    if fs::metadata(parent_dir).is_err() {
        match fs::create_dir_all(parent_dir) {
            Ok(_) => {}
            Err(e) => return_file_error!(file_path, "Error creating directory: {:?}", e),
        }
    }

    // match fs::write(&file_path, &module.html) {
    //     Ok(_) => {}
    //     Err(e) => return_file_error!(module.output_path, "Error writing HTML file: {:?}", e),
    // }

    // Write the JS file to the same directory
    // match fs::write(file_path.with_extension("js"), &module.js) {
    //     Ok(_) => {}
    //     Err(e) => return_file_error!(module.output_path, "Error writing JS file: {:?}", e),
    // };

    let wasm = module.wasm.finish()?;

    if let Err(e) = validate(&wasm) {
        return_file_error!(file_path, "Error validating WASM module: {:?}", e)
    }

    // Write the wasm file to the same directory
    match fs::write(file_path.with_extension("wasm"), wasm) {
        Ok(_) => {}
        Err(e) => return_file_error!(file_path, "Error writing WASM file: {:?}", e),
    }

    Ok(())
}
