//! Warning identity checks for integration results.
//!
//! WHAT: applies ignore, forbid and exact warning-code/count expectation contracts.
//! WHY: warning diagnostics remain structured through both success and failure result lanes, so
//!      identity matching never needs to parse rendered warning prose.

use super::super::WarningExpectation;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use std::collections::BTreeMap;

pub(super) fn validate_warning_expectation<'diagnostic>(
    actual_warnings: impl IntoIterator<Item = &'diagnostic CompilerDiagnostic>,
    expectation: &WarningExpectation,
) -> Option<String> {
    let actual_warnings: Vec<&CompilerDiagnostic> = actual_warnings.into_iter().collect();

    match expectation {
        WarningExpectation::Ignore => None,
        WarningExpectation::Forbid => (!actual_warnings.is_empty())
            .then(|| format!("Expected no warnings, but found {}.", actual_warnings.len())),
        WarningExpectation::Exact(exact) => {
            let Some(expected_codes) = &exact.expected_codes else {
                return (actual_warnings.len() != exact.expected_count).then(|| {
                    format!(
                        "Expected exactly {} warnings, but found {}.",
                        exact.expected_count,
                        actual_warnings.len()
                    )
                });
            };

            let actual_codes = actual_warnings
                .iter()
                .map(|warning| warning.kind.code())
                .collect::<Vec<_>>();
            compare_warning_code_multisets(expected_codes, &actual_codes)
        }
    }
}

fn compare_warning_code_multisets(
    expected_codes: &[String],
    actual_codes: &[&str],
) -> Option<String> {
    let difference = super::compare_exact_code_multisets(
        expected_codes.iter().map(String::as_str),
        actual_codes.iter().copied(),
    )?;

    let mut mismatch = String::from("Warning code multiset mismatch.");
    append_code_category(&mut mismatch, "Missing warning codes", &difference.missing);
    append_code_category(
        &mut mismatch,
        "Unexpected warning codes",
        &difference.unexpected,
    );
    append_code_category(
        &mut mismatch,
        "Count-mismatched warning codes",
        &difference.count_mismatches,
    );
    Some(mismatch)
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
