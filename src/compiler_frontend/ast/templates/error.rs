//! Local template error boundary.
//!
//! WHAT: keeps formatter and template-owned source diagnostics typed while template helpers still
//! expose a mix of `CompilerDiagnostic` and older `CompilerError` entrypoints.
//! WHY: template construction and folding sit between AST source diagnostics and project-aware
//! formatting/folding infrastructure. This boundary makes that distinction explicit locally.

use crate::compiler_frontend::ast::templates::template_slots::TemplateSlotError;
use crate::compiler_frontend::compiler_errors::{CompilerError, compiler_error_to_diagnostic};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;

#[derive(Debug)]
pub(crate) enum TemplateError {
    Diagnostic(Box<CompilerDiagnostic>),
    Infrastructure(Box<CompilerError>),
}

impl TemplateError {
    pub(crate) fn into_diagnostic(self) -> CompilerDiagnostic {
        match self {
            TemplateError::Diagnostic(diagnostic) => *diagnostic,
            TemplateError::Infrastructure(error) => compiler_error_to_diagnostic(error.as_ref()),
        }
    }
}

impl From<CompilerDiagnostic> for TemplateError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        TemplateError::Diagnostic(Box::new(diagnostic))
    }
}

impl From<CompilerError> for TemplateError {
    fn from(error: CompilerError) -> Self {
        TemplateError::Infrastructure(Box::new(error))
    }
}

impl From<TemplateSlotError> for TemplateError {
    fn from(error: TemplateSlotError) -> Self {
        match error {
            TemplateSlotError::Diagnostic(diagnostic) => TemplateError::Diagnostic(diagnostic),
            TemplateSlotError::Infrastructure(error) => TemplateError::Infrastructure(error),
        }
    }
}
