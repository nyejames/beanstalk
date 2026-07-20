//! Diagnostic-code and rendered-message checks for integration failures.
//!
//! WHAT: validates typed exact/contains code contracts and message fragments at the compiler
//!       render boundary.
//! WHY: diagnostic matching must stay separate from warning, artifact and backend validation so
//!      later matching-mode changes have one owner.

use super::super::{DiagnosticMatchMode, FailureExpectation};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::render::{terminal, terse};
use std::collections::BTreeMap;

pub(super) fn validate_diagnostics(
    messages: &CompilerMessages,
    expectation: &FailureExpectation,
) -> Option<String> {
    let diagnostic_codes: Vec<&str> = messages
        .diagnostics()
        .map(|diagnostic| diagnostic.kind.code())
        .collect();

    if let Some(reason) = compare_diagnostic_code_multisets(
        &expectation.diagnostic_codes,
        &diagnostic_codes,
        expectation.diagnostic_match,
    ) {
        return Some(reason);
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

fn compare_diagnostic_code_multisets(
    expected_codes: &[String],
    actual_codes: &[&str],
    match_mode: DiagnosticMatchMode,
) -> Option<String> {
    let expected_counts = count_codes(expected_codes.iter().map(String::as_str));
    let actual_counts = count_codes(actual_codes.iter().copied());

    let mut missing_codes = BTreeMap::new();
    let mut unexpected_codes = BTreeMap::new();
    let mut count_mismatches = BTreeMap::new();

    for (code, expected_count) in &expected_counts {
        match actual_counts.get(code) {
            None => {
                missing_codes.insert(*code, (*expected_count, 0));
            }
            Some(actual_count) => {
                let count_is_invalid = match match_mode {
                    DiagnosticMatchMode::Exact => actual_count != expected_count,
                    DiagnosticMatchMode::Contains => actual_count < expected_count,
                };

                if count_is_invalid {
                    count_mismatches.insert(*code, (*expected_count, *actual_count));
                }
            }
        }
    }

    if match_mode == DiagnosticMatchMode::Exact {
        for (code, actual_count) in &actual_counts {
            if !expected_counts.contains_key(code) {
                unexpected_codes.insert(*code, (0, *actual_count));
            }
        }
    }

    if missing_codes.is_empty() && unexpected_codes.is_empty() && count_mismatches.is_empty() {
        return None;
    }

    let mut mismatch = format!(
        "Diagnostic code multiset mismatch in {} mode.",
        match_mode.as_str()
    );
    append_code_category(&mut mismatch, "Missing codes", &missing_codes);
    append_code_category(&mut mismatch, "Unexpected codes", &unexpected_codes);
    append_code_category(&mut mismatch, "Count-mismatched codes", &count_mismatches);
    Some(mismatch)
}

fn count_codes<'code>(codes: impl IntoIterator<Item = &'code str>) -> BTreeMap<&'code str, usize> {
    let mut counts = BTreeMap::new();
    for code in codes {
        *counts.entry(code).or_insert(0) += 1;
    }
    counts
}

fn append_code_category(
    mismatch: &mut String,
    category: &str,
    codes: &BTreeMap<&str, (usize, usize)>,
) {
    if codes.is_empty() {
        return;
    }

    mismatch.push(' ');
    mismatch.push_str(category);
    mismatch.push_str(": ");

    let mut first = true;
    for (code, (expected_count, actual_count)) in codes {
        if !first {
            mismatch.push_str(", ");
        }
        first = false;
        mismatch.push_str(code);
        mismatch.push_str(" (expected ");
        mismatch.push_str(&expected_count.to_string());
        mismatch.push_str(", actual ");
        mismatch.push_str(&actual_count.to_string());
        mismatch.push(')');
    }
    mismatch.push('.');
}
