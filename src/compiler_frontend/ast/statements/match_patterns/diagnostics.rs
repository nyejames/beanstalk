//! Deferred-pattern diagnostics.
//!
//! WHAT: shared rejection logic for pattern syntax that is not yet supported.
//! WHY: centralising deferred-pattern checks ensures every parser entry point
//! rejects unsupported lead tokens with identical wording.

use crate::compiler_frontend::compiler_messages::deferred_feature_diagnostics::deferred_feature_reason_diagnostic;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DeferredFeatureReason, InvalidMatchPatternReason,
};
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};

/// Reject match-pattern lead tokens that are unsupported or deferred.
///
/// WHAT: checks the current token against wildcard, negation, capture-tagged,
/// and `as` patterns and returns a structured diagnostic for each.
/// WHY: every parser entry point that begins a pattern should call this so
/// unsupported syntax is rejected with consistent wording and stable codes.
#[allow(clippy::result_large_err)]
pub fn reject_deferred_pattern_lead_token(
    token_stream: &FileTokens,
) -> Result<(), CompilerDiagnostic> {
    // These forms intentionally fail fast so unsupported syntax never drifts silently.
    match token_stream.current_token_kind() {
        TokenKind::Wildcard => {
            return Err(CompilerDiagnostic::invalid_match_pattern(
                InvalidMatchPatternReason::WildcardNotSupported,
                None,
                None,
                token_stream.current_location(),
            ));
        }

        TokenKind::Not => {
            return Err(deferred_feature_reason_diagnostic(
                DeferredFeatureReason::NegatedMatchPattern,
                token_stream.current_location(),
            ));
        }

        TokenKind::TypeParameterBracket => {
            return Err(deferred_feature_reason_diagnostic(
                DeferredFeatureReason::CaptureTaggedPattern,
                token_stream.current_location(),
            ));
        }

        TokenKind::As => {
            return Err(CompilerDiagnostic::invalid_match_pattern(
                InvalidMatchPatternReason::AsNotValid,
                None,
                None,
                token_stream.current_location(),
            ));
        }

        _ => {}
    }

    Ok(())
}
