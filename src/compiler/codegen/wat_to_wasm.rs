use crate::compiler::compiler_errors::CompileError;
use crate::compiler::string_interning::StringTable;
use crate::return_wat_err;
use std::fs;
use std::path::{Path, PathBuf};
use wat::parse_file;

pub fn compile_wat_file(src_path: &Path) -> Result<(), CompileError> {
    // Create a temporary string table for error reporting
    let mut string_table = StringTable::new();
    
    let wasm = parse_file(src_path);
    match wasm {
        Ok(wasm) => {
            let file_stem = match src_path.file_stem() {
                Some(s) => PathBuf::from(s).with_extension("wasm"),
                None => PathBuf::from("wasm.wasm"),
            };
            println!("Compiling: {:?} to WASM", file_stem);

            let parent_folder = src_path.parent().unwrap_or_else(|| Path::new(""));

            let output_path = PathBuf::from(parent_folder).join(file_stem);
            match fs::write(&output_path, wasm) {
                Ok(_) => {
                    println!("WASM compiled successfully");
                    Ok(())
                }
                Err(e) => return_wat_err!(&mut string_table, e.to_string(), {
                    CompilationStage => "WASM File Writing",
                    PrimarySuggestion => "Check file permissions and disk space",
                }),
            }
        }

        Err(e) => return_wat_err!(&mut string_table, e.to_string(), {
            CompilationStage => "WAT Parsing",
            PrimarySuggestion => "Check WAT syntax - ensure all instructions are valid and properly formatted",
        }),
    }
}
