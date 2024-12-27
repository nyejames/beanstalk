use std::fs;
use std::path::{Path, PathBuf};
use wat::parse_file;
use crate::CompileError;

pub fn compile_wat_file(path: &Path) -> Result<(), CompileError> {
    let wasm = parse_file(path);
    match wasm {
        Ok(wasm) => {
            let file_stem = match path.file_stem() {
                Some(s) => PathBuf::from(s).with_extension("wasm"),
                None => PathBuf::from("wasm.wasm"),
            };
            println!("Compiling: {:?} to WASM", file_stem);

            let parent_folder = path.parent().unwrap_or_else(|| Path::new(""));

            let output_path = PathBuf::from(parent_folder).join(file_stem);
            match fs::write(output_path, wasm) {
                Ok(_) => {
                    println!("WASM compiled successfully");
                    Ok(())
                }
                Err(e) => Err(CompileError {
                    msg: format!("Error writing WASM file: {:?}", e),
                    line_number: 0,
                }),
            }
        }
        Err(e) => Err(CompileError {
            msg: format!("Error parsing WAT file: {:?}", e),
            line_number: 0,
        }),
    }
}
