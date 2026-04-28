//! Deferred-pattern diagnostics.
//!
//! WHAT: shared rejection logic for pattern syntax that is not yet supported.
//! WHY: centralising deferred-pattern checks ensures every parser entry point
//! rejects unsupported lead tokens with identical wording.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::deferred_feature_diagnostics::deferred_feature_rule_error;
use crate::compiler_frontend::tokenizer::tokens::{FileTokens, TokenKind};
use crate::return_rule_error;

pub fn reject_deferred_pattern_lead_token(token_stream: &FileTokens) -> Result<(), CompilerError> {
    // These forms intentionally fail fast so unsupported syntax never drifts silently.
    match token_stream.current_token_kind() {
        TokenKind::Wildcard => {
            return Err(deferred_feature_rule_error(
                "Wildcard patterns in 'case' arms are not supported. Use 'else =>' for a catch-all arm.",
                token_stream.current_location(),
                "Match Statement Parsing",
                "Replace 'case _ =>' with 'else =>'.",
            ));
        }
        TokenKind::Not => {
            return Err(deferred_feature_rule_error(
                "Negated match patterns (for example 'case not ... =>') are deferred.",
                token_stream.current_location(),
                "Match Statement Parsing",
                "Use explicit positive case arms and an 'else =>' fallback in this phase.",
            ));
        }
        TokenKind::TypeParameterBracket => {
            return Err(deferred_feature_rule_error(
                "Capture/tagged patterns using '|...|' are deferred.",
                token_stream.current_location(),
                "Match Statement Parsing",
                "Use simple literal or choice-variant patterns only.",
            ));
        }
        TokenKind::As => {
            return_rule_error!(
                "`as` is not valid in match patterns. It is only supported in choice payload captures.",
                token_stream.current_location(),
                {
                    CompilationStage => "Match Statement Parsing",
                    PrimarySuggestion => "Use 'case Variant(field as local_name)' for choice payload aliases only",
                }
            );
        }
        _ => {}
    }

    Ok(())
}
