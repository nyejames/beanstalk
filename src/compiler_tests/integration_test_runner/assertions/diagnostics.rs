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
    let difference = match match_mode {
        DiagnosticMatchMode::Exact => super::compare_exact_code_multisets(
            expected_codes.iter().map(String::as_str),
            actual_codes.iter().copied(),
        ),
        DiagnosticMatchMode::Contains => {
            compare_contained_code_multisets(expected_codes, actual_codes)
        }
    }?;

    let mut mismatch = format!(
        "Diagnostic code multiset mismatch in {} mode.",
        match_mode.as_str()
    );
    append_code_category(&mut mismatch, "Missing codes", &difference.missing);
    append_code_category(&mut mismatch, "Unexpected codes", &difference.unexpected);
    append_code_category(
        &mut mismatch,
        "Count-mismatched codes",
        &difference.count_mismatches,
    );
    Some(mismatch)
}

fn compare_contained_code_multisets(
    expected_codes: &[String],
    actual_codes: &[&str],
) -> Option<super::CodeMultisetDifference> {
    let mut expected_counts = BTreeMap::new();
    for code in expected_codes {
        *expected_counts.entry(code.as_str()).or_insert(0) += 1;
    }

    let mut actual_counts = BTreeMap::new();
    for code in actual_codes {
        *actual_counts.entry(*code).or_insert(0) += 1;
    }

    let mut missing = BTreeMap::new();
    let mut count_mismatches = BTreeMap::new();

    for (code, expected_count) in expected_counts {
        match actual_counts.get(code) {
            None => {
                missing.insert(code.to_owned(), (expected_count, 0));
            }
            Some(actual_count) if *actual_count < expected_count => {
                count_mismatches.insert(code.to_owned(), (expected_count, *actual_count));
            }
            Some(_) => {}
        }
    }

    if missing.is_empty() && count_mismatches.is_empty() {
        return None;
    }

    Some(super::CodeMultisetDifference {
        missing,
        unexpected: BTreeMap::new(),
        count_mismatches,
    })
}

fn append_code_category(
    mismatch: &mut String,
    category: &str,
    codes: &BTreeMap<String, (usize, usize)>,
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
