//! Integration-test result validation and assertion-family wiring.
//!
//! WHAT: sequences success and failure checks against the canonical expectation model.
//! WHY: each assertion family owns its checks while this module preserves their established
//!      order and converts failures into runner results.

mod artifacts;
mod diagnostics;
mod goldens;
mod rendered_output;
mod warnings;
mod wasm;

pub(crate) use goldens::discover_golden_expectation;

#[cfg(test)]
use super::GoldenMode;
#[cfg(test)]
use super::types::GoldenExpectation;
use super::{
    BackendId, CaseExecutionResult, FailureExpectation, FailureKind, SuccessExpectation,
    TestCaseSpec,
};
use crate::build_system::build::BuildResult;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;

#[cfg(test)]
pub(crate) fn normalize_text_for_comparison(text: &str) -> String {
    goldens::normalize_text_for_comparison(text)
}

#[cfg(test)]
pub(crate) fn compare_text_golden(
    expected: &str,
    actual: &str,
    mode: GoldenMode,
) -> Option<String> {
    goldens::compare_text_golden(expected, actual, mode)
}

#[cfg(test)]
pub(crate) fn validate_golden_outputs(
    build_result: &BuildResult,
    golden: &GoldenExpectation,
) -> Option<(String, FailureKind)> {
    goldens::validate_golden_outputs(build_result, golden)
}

#[cfg(test)]
pub(crate) fn validate_rendered_output_fragments(
    rendered_output: &str,
    contains: &[String],
    not_contains: &[String],
) -> Option<(String, FailureKind)> {
    rendered_output::validate_rendered_output_fragments(rendered_output, contains, not_contains)
}

pub(crate) fn validate_success_result(
    case: &TestCaseSpec,
    build_result: BuildResult,
    expectation: &SuccessExpectation,
) -> CaseExecutionResult {
    if let Some(reason) =
        warnings::validate_warning_expectation(build_result.warnings.len(), expectation.warnings)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if case.backend_id == BackendId::Html
        && let Some(reason) = artifacts::validate_html_baseline_contract(&build_result)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if case.backend_id == BackendId::HtmlWasm
        && let Some(reason) = wasm::validate_html_wasm_baseline_contract(&build_result)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if let Some(reason) =
        artifacts::validate_artifact_assertions(&build_result, &expectation.artifact_assertions)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if let Some(reason) = artifacts::validate_artifacts_must_not_exist(
        &build_result,
        &expectation.artifacts_must_not_exist,
    ) {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if let Some((reason, kind)) =
        goldens::validate_golden_outputs(&build_result, &expectation.golden)
    {
        return fail(build_result, reason, kind);
    }

    if (!expectation.rendered_output_contains.is_empty()
        || !expectation.rendered_output_not_contains.is_empty())
        && let Some((reason, kind)) = rendered_output::validate_rendered_output(
            &build_result,
            &expectation.rendered_output_contains,
            &expectation.rendered_output_not_contains,
        )
    {
        return fail(build_result, reason, kind);
    }

    CaseExecutionResult {
        passed: true,
        panic_message: None,
        build_result: Some(build_result),
        messages: None,
        failure_reason: None,
        failure_kind: None,
    }
}

fn fail(build_result: BuildResult, reason: String, kind: FailureKind) -> CaseExecutionResult {
    CaseExecutionResult {
        passed: false,
        panic_message: None,
        build_result: Some(build_result),
        messages: None,
        failure_reason: Some(reason),
        failure_kind: Some(kind),
    }
}

pub(crate) fn validate_failure_result(
    messages: CompilerMessages,
    expectation: &FailureExpectation,
) -> CaseExecutionResult {
    if let Some(reason) =
        warnings::validate_warning_expectation(messages.warnings().count(), expectation.warnings)
    {
        return failure_messages(messages, reason);
    }

    if let Some(reason) = diagnostics::validate_diagnostics(&messages, expectation) {
        return failure_messages(messages, reason);
    }

    CaseExecutionResult {
        passed: true,
        panic_message: None,
        build_result: None,
        messages: Some(messages),
        failure_reason: None,
        failure_kind: None,
    }
}

fn failure_messages(messages: CompilerMessages, reason: String) -> CaseExecutionResult {
    CaseExecutionResult {
        passed: false,
        panic_message: None,
        build_result: None,
        messages: Some(messages),
        failure_reason: Some(reason),
        failure_kind: Some(FailureKind::ExpectationViolation),
    }
}

/// Checks whether every fragment appears in order without requiring adjacency.
fn contains_ordered_substrings(text: &str, substrings: &[String]) -> bool {
    let mut offset = 0usize;

    for substring in substrings {
        let Some(position) = text[offset..].find(substring) else {
            return false;
        };
        offset += position + substring.len();
    }

    true
}
