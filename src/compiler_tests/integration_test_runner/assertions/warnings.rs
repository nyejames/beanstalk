//! Warning-count checks for integration results.
//!
//! WHAT: applies the existing ignore, forbid and exact warning-count expectation contract.
//! WHY: warning validation stays independent from diagnostic-code and artifact assertions until
//!      the later multiplicity phase changes that contract.

use super::super::WarningExpectation;

pub(super) fn validate_warning_expectation(
    actual_count: usize,
    expectation: WarningExpectation,
) -> Option<String> {
    match expectation {
        WarningExpectation::Ignore => None,
        WarningExpectation::Forbid => {
            (actual_count > 0).then(|| format!("Expected no warnings, but found {actual_count}."))
        }
        WarningExpectation::Exact(expected) => (actual_count != expected)
            .then(|| format!("Expected exactly {expected} warnings, but found {actual_count}.")),
    }
}
