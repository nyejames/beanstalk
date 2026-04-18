//! ABI and Wasm value-type conversion helpers.

use crate::backends::error_types::BackendErrorType;
use crate::backends::wasm::lir::types::WasmAbiType;
use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerError, ErrorType};
use wasm_encoder::ValType;

pub(crate) fn abi_to_val_type(abi: WasmAbiType) -> Result<ValType, CompilerError> {
    // WHAT: map backend ABI types onto concrete core-Wasm value types.
    // WHY: phase-2 uses an `i32` handle ABI for linear-memory objects to keep interop simple.
    match abi {
        WasmAbiType::I32 | WasmAbiType::Handle => Ok(ValType::I32),
        WasmAbiType::I64 => Ok(ValType::I64),
        WasmAbiType::F32 => Ok(ValType::F32),
        WasmAbiType::F64 => Ok(ValType::F64),
        WasmAbiType::Void => Err(CompilerError::compiler_error(
            "Wasm emission cannot lower `Void` to a concrete Wasm value type",
        )
        .with_error_type(ErrorType::Backend(BackendErrorType::WasmGeneration))),
    }
}
