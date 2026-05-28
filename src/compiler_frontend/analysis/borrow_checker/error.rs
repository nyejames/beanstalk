//! Borrow-checker boundary error.
//!
//! WHAT: separates user-facing borrow diagnostics from internal borrow-checker infrastructure
//! failures.
//! WHY: borrow-rule failures should remain typed `CompilerDiagnostic` values, while invalid HIR
//! metadata or missing compiler side-table facts should stay on the internal `CompilerError` path.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
#[cfg(test)]
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[derive(Debug, Clone)]
pub(crate) enum BorrowCheckError {
    Diagnostic(CompilerDiagnostic),
    Infrastructure(CompilerError),
}

impl BorrowCheckError {
    pub(crate) fn into_diagnostic_or_infrastructure(
        self,
    ) -> Result<CompilerDiagnostic, CompilerError> {
        match self {
            BorrowCheckError::Diagnostic(diagnostic) => Ok(diagnostic),
            BorrowCheckError::Infrastructure(error) => Err(error),
        }
    }

    #[cfg(test)]
    pub(crate) fn diagnostic(&self) -> Option<&CompilerDiagnostic> {
        match self {
            BorrowCheckError::Diagnostic(diagnostic) => Some(diagnostic),
            BorrowCheckError::Infrastructure(_) => None,
        }
    }

    #[cfg(test)]
    pub(crate) fn infrastructure(&self) -> Option<&CompilerError> {
        match self {
            BorrowCheckError::Diagnostic(_) => None,
            BorrowCheckError::Infrastructure(error) => Some(error),
        }
    }

    #[cfg(test)]
    pub(crate) fn rendered_message_for_tests(&self, string_table: &StringTable) -> String {
        match self {
            BorrowCheckError::Diagnostic(diagnostic) => {
                crate::compiler_frontend::compiler_messages::render::terse::format_terse_diagnostics(
                    std::slice::from_ref(diagnostic),
                    string_table,
                )
                .join("\n")
            }
            BorrowCheckError::Infrastructure(error) => error.msg.clone(),
        }
    }
}

impl From<CompilerError> for BorrowCheckError {
    fn from(error: CompilerError) -> Self {
        BorrowCheckError::Infrastructure(error)
    }
}

impl From<CompilerDiagnostic> for BorrowCheckError {
    fn from(diagnostic: CompilerDiagnostic) -> Self {
        BorrowCheckError::Diagnostic(diagnostic)
    }
}
