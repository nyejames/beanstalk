//! Interpreter-specific backend errors.
//!
//! WHAT: groups interpreter request, lowering, runtime, and CTFE failures under one backend-local
//! enum before conversion into the compiler's shared error surface.
//! WHY: this keeps interpreter internals explicit without leaking ad-hoc strings everywhere.

use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerError, ErrorType};

#[derive(Debug, Clone)]
pub(crate) enum InterpreterBackendError {
    InvalidRequest { message: String },
    Lowering { message: String },
    Execution { message: String },
    Ctfe { message: String },
    InternalInvariant { message: String },
}

impl InterpreterBackendError {
    pub(crate) fn into_compiler_error(self) -> CompilerError {
        match self {
            Self::InvalidRequest { message } => {
                CompilerError::compiler_error(message).with_error_type(ErrorType::Compiler)
            }
            Self::Lowering { message } => {
                CompilerError::compiler_error(message).with_error_type(ErrorType::LirTransformation)
            }
            Self::Execution { message } => {
                CompilerError::compiler_error(message).with_error_type(ErrorType::Compiler)
            }
            Self::Ctfe { message } => {
                CompilerError::compiler_error(message).with_error_type(ErrorType::Compiler)
            }
            Self::InternalInvariant { message } => {
                CompilerError::compiler_error(message).with_error_type(ErrorType::Compiler)
            }
        }
    }
}
