//! Shared diagnostics for deferred and intentionally unsupported user-facing features.
//!
//! WHAT: provides one helper pattern for deferred language-surface failures.
//! WHY: parser/tokenizer callsites should not hand-roll metadata keys and wording patterns.

use crate::compiler_frontend::compiler_errors::SourceLocation;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DeferredFeatureReason};

/// Build a structured diagnostic for a known deferred language surface.
///
/// WHAT: carries a specific deferred-feature reason rather than pre-rendered prose.
/// WHY: AST and header callers should report deferred features without round-tripping through
/// legacy rule errors or string metadata maps.
pub(crate) fn deferred_feature_reason_diagnostic(
    reason: DeferredFeatureReason,
    location: SourceLocation,
) -> CompilerDiagnostic {
    CompilerDiagnostic::deferred_feature_reason(reason, location)
}
