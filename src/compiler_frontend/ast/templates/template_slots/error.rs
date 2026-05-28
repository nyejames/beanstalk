//! Error boundary for template slot composition.
//!
//! WHAT: keeps slot-validation failures as typed diagnostics while preserving a narrow path for
//! genuine infrastructure failures from child-template composition.
//! WHY: slot schema and composition own user-facing template diagnostics; only their current
//! callers still require the older `CompilerError` boundary shape.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;

#[derive(Debug)]
pub(in crate::compiler_frontend::ast::templates) enum TemplateSlotError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl From<CompilerDiagnostic> for TemplateSlotError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        TemplateSlotError::Diagnostic(Box::new(diagnostic))
    }
}

impl From<CompilerError> for TemplateSlotError {
    fn from(error: CompilerError) -> Self {
        TemplateSlotError::Infrastructure(Box::new(error))
    }
}
