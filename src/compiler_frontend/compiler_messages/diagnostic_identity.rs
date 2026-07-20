//! Stable identity facts for compiler diagnostics.
//!
//! WHAT: exposes the diagnostic code, actual severity, and optional typed reason key needed by
//! compiler tooling and tests.
//! WHY: reason keys are a compiler-owned contract independent of rendered wording, titles, and
//! `Debug` output, so renderers can evolve without changing structured identity.

use super::diagnostic_payload::DiagnosticPayload;
use super::diagnostic_severity::DiagnosticSeverity;

// The identity record is the compiler-facing boundary value consumed by tests and tooling.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct DiagnosticIdentity {
    pub(crate) code: &'static str,
    pub(crate) severity: DiagnosticSeverity,
    pub(crate) reason_key: Option<&'static str>,
}

impl DiagnosticIdentity {
    // Keep construction beside the identity record so callers cannot rebuild the reason bridge.
    pub(super) fn new(
        code: &'static str,
        severity: DiagnosticSeverity,
        payload: &DiagnosticPayload,
    ) -> Self {
        Self {
            code,
            severity,
            reason_key: payload.stable_reason_key(),
        }
    }
}
