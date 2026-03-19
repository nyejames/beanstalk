//! In-process validation for emitted Wasm bytes.

use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerError, ErrorType};
use std::fmt::Write as _;

pub(crate) fn validate_emitted_module(wasm_bytes: &[u8]) -> Result<String, CompilerError> {
    // WHAT: run an in-process validator pass in debug/test workflows.
    // WHY: this provides immediate structural/type feedback without requiring external tools.
    let mut validator = wasmparser::Validator::new();

    validator.validate_all(wasm_bytes).map_err(|error| {
        CompilerError::compiler_error(format!("Wasm validation failed: {error}"))
            .with_error_type(ErrorType::WasmGeneration)
    })?;

    let mut out = String::new();
    let _ = writeln!(
        out,
        "Wasm validation succeeded ({} bytes).",
        wasm_bytes.len()
    );
    Ok(out)
}
