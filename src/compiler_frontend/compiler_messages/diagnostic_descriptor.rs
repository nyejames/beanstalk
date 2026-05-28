//! Stable diagnostic descriptors.
//!
//! WHAT: binds a diagnostic kind to its stable code, short title, and default severity.
//! WHY: enum variants are internal implementation details, while diagnostic codes are the
//! contract for users, tests, and future tooling.

use crate::compiler_frontend::compiler_messages::DiagnosticSeverity;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticDescriptor {
    pub code: &'static str,
    pub title: &'static str,
    pub default_severity: DiagnosticSeverity,
}

impl DiagnosticDescriptor {
    pub(crate) const fn new(
        code: &'static str,
        title: &'static str,
        default_severity: DiagnosticSeverity,
    ) -> Self {
        Self {
            code,
            title,
            default_severity,
        }
    }
}
