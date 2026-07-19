//! Self-tests for integration case execution bookkeeping.
//!
//! WHAT: protects the runner's panic-to-failure result conversion.
//! WHY: compiler and harness panics must never be reported as passing cases.

use super::super::FailureKind;
use super::super::execution::panic_case_result;

#[test]
fn panic_execution_results_are_always_failures() {
    let result = panic_case_result(Box::new("boom".to_string()));
    assert!(!result.passed);
    assert_eq!(result.panic_message.as_deref(), Some("boom"));
    assert!(result.failure_reason.is_some());
}

#[test]
fn panic_execution_result_has_harness_failed_kind() {
    let result = panic_case_result(Box::new("boom".to_string()));
    assert!(!result.passed);
    assert_eq!(result.failure_kind, Some(FailureKind::HarnessFailed));
}
