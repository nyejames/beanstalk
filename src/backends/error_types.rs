//! Backend-specific error taxonomy.
//!
//! WHAT: groups backend lowering and codegen error categories that do not belong in the frontend
//! error enum, keeping stage boundaries clean.
//! WHY: Wasm generation and LIR transformation are backend concerns; the frontend should not own
//! their error variants.

use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerError, ErrorType};

#[derive(PartialEq, Debug, Clone)]
pub enum BackendErrorType {
    LirTransformation,
    WasmGeneration,
}

impl BackendErrorType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LirTransformation => "LIR Transformation",
            Self::WasmGeneration => "WASM Generation",
        }
    }
}

/// Helper: constructs a `CompilerError` tagged with [`BackendErrorType::LirTransformation`].
pub fn lir_transformation_error(msg: impl Into<String>) -> CompilerError {
    CompilerError::compiler_error(msg)
        .with_error_type(ErrorType::Backend(BackendErrorType::LirTransformation))
}
