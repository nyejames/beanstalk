//! Diagnostic severity levels.
//!
//! WHAT: represents how a diagnostic should be treated by renderers and pipeline policy.
//! WHY: severity is independent from diagnostic category; warnings and errors share the same
//! structured data model.

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
}
