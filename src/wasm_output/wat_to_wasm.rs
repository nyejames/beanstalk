use crate::tokenizer::TokenPosition;
use crate::{CompileError, ErrorType};
use std::fs;
use std::path::{Path, PathBuf};
use wat::parse_file;

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
                    start_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    end_pos: TokenPosition {
                        line_number: 0,
                        char_column: 0,
                    },
                    error_type: ErrorType::File,
                }),
            }
        }

        Err(e) => Err(CompileError {
            msg: format!("Error parsing WAT file: {:?}", e),
            start_pos: TokenPosition {
                line_number: 0,
                char_column: 0,
            },
            end_pos: TokenPosition {
                line_number: 0,
                char_column: 0,
            },
            error_type: ErrorType::File,
        }),
    }
}
