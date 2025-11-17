use crate::compiler::string_interning::StringTable;
use std::fs;
use std::path::{Path, PathBuf};
use wat::parse_file;

pub fn compile_wat_file(src_path: &Path) -> Result<(), String> {
    // Create a temporary string table for error reporting
    let string_table = StringTable::new();

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
                Err(e) => Err(format!("Issue while writing a wat file to disk: {}", e.to_string())),
            }
        }

        Err(e) => Err(format!("Issue with parsing wat file: {}", e.to_string())),
    }
}
