//! Diagnostic-code and rendered-message checks for integration failures.
//!
//! WHAT: validates typed exact/contains code contracts and message fragments at the compiler
//!       render boundary.
//! WHY: diagnostic matching must stay separate from warning, artifact and backend validation so
//!      later matching-mode changes have one owner.

use super::super::{DiagnosticAssertion, DiagnosticMatchMode, FailureExpectation};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::render::{
    relative_display_path_from_root, resolve_source_file_path, terminal, terse,
};
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, DiagnosticLabelStyle};
use std::collections::BTreeMap;
use std::path::Path;

pub(super) fn validate_diagnostics(
    messages: &CompilerMessages,
    expectation: &FailureExpectation,
    fixture_root: &Path,
) -> Option<String> {
    let diagnostic_codes: Vec<&str> = messages
        .diagnostics()
        .map(|diagnostic| diagnostic.identity().code)
        .collect();

    if let Some(reason) = compare_diagnostic_code_multisets(
        &expectation.diagnostic_codes,
        &diagnostic_codes,
        expectation.diagnostic_match,
    ) {
        return Some(reason);
    }

    if let Some(reason) =
        validate_structured_diagnostic_assertions(messages, expectation, fixture_root)
    {
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

fn validate_structured_diagnostic_assertions(
    messages: &CompilerMessages,
    expectation: &FailureExpectation,
    fixture_root: &Path,
) -> Option<String> {
    let mut mismatches = Vec::new();

    for assertion in &expectation.diagnostic_assertions {
        let matching_diagnostics = messages
            .diagnostics()
            .filter(|diagnostic| diagnostic.identity().code == assertion.code)
            .collect::<Vec<_>>();
        let actual_count = matching_diagnostics.len();

        if let Some(expected_count) = assertion.count
            && expected_count != actual_count
        {
            append_structured_mismatch(
                &mut mismatches,
                assertion,
                "count",
                expected_count.to_string(),
                actual_count.to_string(),
            );
        }

        let Some(diagnostic) = assertion
            .occurrence
            .checked_sub(1)
            .and_then(|index| matching_diagnostics.get(index))
        else {
            append_structured_mismatch(
                &mut mismatches,
                assertion,
                "diagnostic",
                format!("occurrence {} present", assertion.occurrence),
                format!("only {actual_count} occurrence(s) present"),
            );
            continue;
        };

        let identity = diagnostic.identity();
        if let Some(expected_reason) = &assertion.reason {
            let actual_reason = identity.reason_key.unwrap_or("<none>");
            if actual_reason != expected_reason {
                append_structured_mismatch(
                    &mut mismatches,
                    assertion,
                    "reason",
                    expected_reason.clone(),
                    actual_reason.to_owned(),
                );
            }
        }

        if let Some(expected_path) = &assertion.path {
            let actual_path = diagnostic_path(diagnostic, messages, fixture_root);
            if actual_path != *expected_path {
                append_structured_mismatch(
                    &mut mismatches,
                    assertion,
                    "path",
                    expected_path.clone(),
                    actual_path,
                );
            }
        }

        if let Some(expected_line) = assertion.line {
            let actual_line = diagnostic.primary_location.start_pos.line_number;
            if !position_matches(expected_line, actual_line) {
                append_structured_mismatch(
                    &mut mismatches,
                    assertion,
                    "line",
                    expected_line.to_string(),
                    actual_line.to_string(),
                );
            }
        }

        if let Some(expected_column) = assertion.column {
            let actual_column = diagnostic.primary_location.start_pos.char_column;
            if !position_matches(expected_column, actual_column) {
                append_structured_mismatch(
                    &mut mismatches,
                    assertion,
                    "column",
                    expected_column.to_string(),
                    actual_column.to_string(),
                );
            }
        }

        validate_secondary_label_assertions(
            &mut mismatches,
            assertion,
            diagnostic,
            messages,
            fixture_root,
        );
    }

    (!mismatches.is_empty()).then(|| mismatches.join("\n"))
}

fn validate_secondary_label_assertions(
    mismatches: &mut Vec<String>,
    assertion: &DiagnosticAssertion,
    diagnostic: &CompilerDiagnostic,
    messages: &CompilerMessages,
    fixture_root: &Path,
) {
    let secondary_labels = diagnostic
        .labels
        .iter()
        .filter(|label| label.style == DiagnosticLabelStyle::Secondary)
        .collect::<Vec<_>>();

    for secondary_assertion in &assertion.secondary_labels {
        let Some(label) = secondary_assertion
            .occurrence
            .checked_sub(1)
            .and_then(|index| secondary_labels.get(index))
        else {
            append_secondary_mismatch(
                mismatches,
                assertion,
                secondary_assertion.occurrence,
                "occurrence",
                format!("occurrence {} present", secondary_assertion.occurrence),
                format!(
                    "only {} secondary label occurrence(s) present",
                    secondary_labels.len()
                ),
            );
            continue;
        };

        if let Some(expected_path) = &secondary_assertion.path {
            let actual_path =
                diagnostic_path_from_location(&label.location, messages, fixture_root);
            if actual_path != *expected_path {
                append_secondary_mismatch(
                    mismatches,
                    assertion,
                    secondary_assertion.occurrence,
                    "path",
                    expected_path.clone(),
                    actual_path,
                );
            }
        }

        if let Some(expected_line) = secondary_assertion.line {
            let actual_line = label.location.start_pos.line_number;
            if !position_matches(expected_line, actual_line) {
                append_secondary_mismatch(
                    mismatches,
                    assertion,
                    secondary_assertion.occurrence,
                    "line",
                    expected_line.to_string(),
                    actual_line.to_string(),
                );
            }
        }

        if let Some(expected_column) = secondary_assertion.column {
            let actual_column = label.location.start_pos.char_column;
            if !position_matches(expected_column, actual_column) {
                append_secondary_mismatch(
                    mismatches,
                    assertion,
                    secondary_assertion.occurrence,
                    "column",
                    expected_column.to_string(),
                    actual_column.to_string(),
                );
            }
        }
    }
}

fn append_structured_mismatch(
    mismatches: &mut Vec<String>,
    assertion: &DiagnosticAssertion,
    field: &str,
    expected: String,
    actual: String,
) {
    mismatches.push(format!(
        "Structured diagnostic mismatch: code '{}' occurrence {} field '{}' expected '{}', actual '{}'.",
        assertion.code, assertion.occurrence, field, expected, actual
    ));
}

fn position_matches(expected: usize, actual: i32) -> bool {
    usize::try_from(actual).ok() == Some(expected)
}

fn append_secondary_mismatch(
    mismatches: &mut Vec<String>,
    assertion: &DiagnosticAssertion,
    secondary_occurrence: usize,
    field: &str,
    expected: String,
    actual: String,
) {
    mismatches.push(format!(
        "Structured diagnostic mismatch: code '{}' occurrence {} secondary_labels occurrence {} field '{}' expected '{}', actual '{}'.",
        assertion.code, assertion.occurrence, secondary_occurrence, field, expected, actual
    ));
}

fn diagnostic_path(
    diagnostic: &CompilerDiagnostic,
    messages: &CompilerMessages,
    fixture_root: &Path,
) -> String {
    diagnostic_path_from_location(&diagnostic.primary_location, messages, fixture_root)
}

fn diagnostic_path_from_location(
    location: &SourceLocation,
    messages: &CompilerMessages,
    fixture_root: &Path,
) -> String {
    let source_file = resolve_source_file_path(&location.scope, &messages.string_table);
    let relative_path = relative_display_path_from_root(&source_file, fixture_root);
    super::super::normalize_relative_path_text(&relative_path)
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
