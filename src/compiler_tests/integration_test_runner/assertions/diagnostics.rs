//! Diagnostic-code and rendered-message checks for integration failures.
//!
//! WHAT: validates the current containment contract for expected diagnostic codes and message
//!       fragments at the compiler render boundary.
//! WHY: diagnostic matching must stay separate from warning, artifact and backend validation so
//!      later matching-mode changes have one owner.

use super::super::FailureExpectation;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::render::{terminal, terse};

pub(super) fn validate_diagnostics(
    messages: &CompilerMessages,
    expectation: &FailureExpectation,
) -> Option<String> {
    let diagnostic_codes: Vec<&str> = messages
        .diagnostics()
        .map(|diagnostic| diagnostic.kind.code())
        .collect();

    for expected_code in &expectation.diagnostic_codes {
        if !diagnostic_codes
            .iter()
            .any(|actual| actual == expected_code)
        {
            return Some(format!(
                "Expected diagnostic code '{}', but it was not reported. \
                 Actual codes: {:?}.",
                expected_code, diagnostic_codes
            ));
        }
    }

    // Message fragments are checked from typed render output rather than diagnostic debug text.
    if !expectation.message_contains.is_empty() {
        let mut rendered_messages: Vec<String> = messages
            .diagnostics()
            .enumerate()
            .map(|(diagnostic_index, diagnostic)| {
                terminal::format_payload_guidance(
                    &diagnostic.payload,
                    messages.diagnostic_render_context(diagnostic_index),
                )
                .join("\n")
            })
            .collect();

        rendered_messages.extend(messages.diagnostics().flat_map(|diagnostic| {
            terminal::format_label_messages(diagnostic, &messages.string_table)
        }));

        rendered_messages.extend(terse::format_terse_compiler_messages(messages));

        if !rendered_messages.iter().any(|message| {
            super::contains_ordered_substrings(message, &expectation.message_contains)
        }) {
            return Some(
                "Expected ordered diagnostic message fragments were not found in any emitted error."
                    .to_string(),
            );
        }
    }

    None
}
