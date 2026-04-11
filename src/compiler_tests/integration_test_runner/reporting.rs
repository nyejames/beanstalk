//! Terminal output and triage report writing for the integration test suite.
//!
//! WHAT: renders per-case results, the final summary, and the machine-readable triage report.
//! WHY: keeping all formatted output here means the orchestrator only knows about counts and
//!      outcomes — not how to render them — so the output format can evolve independently.

use super::{
    BackendId, CaseExecutionResult, ExpectedOutcome, FailureTriageEntry, FailureTriageReport,
    SEPARATOR_LINE_LENGTH, SummaryCounts, TestCaseSpec,
};
use crate::compiler_frontend::compiler_messages::compiler_errors::error_type_to_str;
use crate::compiler_frontend::display_messages::{print_formatted_error, print_formatted_warning};
use saying::say;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(crate) fn render_case_result(
    case: &TestCaseSpec,
    result: &CaseExecutionResult,
    show_warnings: bool,
) {
    match (&case.expected, result.passed) {
        (ExpectedOutcome::Success(_), true) => say!(Green "✓ PASS"),
        (ExpectedOutcome::Failure(_), true) => say!(Green "✓ EXPECTED FAILURE"),
        (ExpectedOutcome::Success(_), false) => say!(Red "✗ FAIL"),
        (ExpectedOutcome::Failure(_), false) => say!(Yellow "✗ UNEXPECTED SUCCESS"),
    }

    if let Some(reason) = &result.failure_reason {
        say!(Red reason);
    }

    if let Some(panic_message) = &result.panic_message {
        say!(Red format!("panic: {panic_message}"));
    }

    if let Some(messages) = &result.messages {
        for error in &messages.errors {
            if result.passed && matches!(case.expected, ExpectedOutcome::Failure(_)) {
                say!(Yellow error_type_to_str(&error.error_type));
                continue;
            }

            print_formatted_error(error.to_owned(), &messages.string_table);
        }

        if show_warnings {
            for warning in &messages.warnings {
                print_formatted_warning(warning.to_owned(), &messages.string_table);
            }
        }
    } else if let Some(build_result) = &result.build_result
        && show_warnings
    {
        for warning in &build_result.warnings {
            print_formatted_warning(warning.to_owned(), &build_result.string_table);
        }
    }
}

pub(crate) fn render_backend_summary(backend_summaries: &BTreeMap<BackendId, SummaryCounts>) {
    if backend_summaries.is_empty() {
        return;
    }

    say!("\n  Backend breakdown:");
    let rule = format!("  {}", "─".repeat(SEPARATOR_LINE_LENGTH - 2));
    say!(Dark White rule);
    for (backend_id, summary) in backend_summaries {
        let incorrect = summary.incorrect_results();
        if incorrect > 0 {
            say!(
                "    ", Cyan format!("{:<9}", backend_id.as_str()),
                Reset "  total: ", Yellow summary.total_tests,
                Reset "  passed: ", Blue summary.correct_results(),
                Reset "  failed: ", Red Bold incorrect
            );
        } else {
            say!(
                "    ", Cyan format!("{:<9}", backend_id.as_str()),
                Reset "  total: ", Yellow summary.total_tests,
                Reset "  passed: ", Green Bold summary.correct_results()
            );
        }
    }
}

pub(crate) fn format_pass_percentage(correct_results: usize, total_tests: usize) -> String {
    let correct_results =
        u128::try_from(correct_results).expect("usize values always fit into u128");
    let total_tests = u128::try_from(total_tests).expect("usize values always fit into u128");
    let scaled_tenths = (correct_results * 1_000) / total_tests;

    format!("{}.{}", scaled_tenths / 10, scaled_tenths % 10)
}

pub(crate) fn expected_outcome_label(expected: &ExpectedOutcome) -> &'static str {
    match expected {
        ExpectedOutcome::Success(_) => "success",
        ExpectedOutcome::Failure(_) => "failure",
    }
}

pub(crate) fn observed_failure_reason(result: &CaseExecutionResult) -> String {
    if let Some(messages) = &result.messages
        && let Some(first_error) = messages.errors.first()
    {
        let base = result
            .failure_reason
            .as_deref()
            .unwrap_or("Compilation failed.");
        return format!(
            "{base} First diagnostic [{}]: {}",
            error_type_to_str(&first_error.error_type),
            first_error.msg
        );
    }

    if let Some(reason) = &result.failure_reason {
        return reason.to_owned();
    }

    if let Some(panic_message) = &result.panic_message {
        return format!("Compiler panic: {panic_message}");
    }

    "No failure reason was recorded.".to_string()
}

pub(crate) fn write_failure_triage_report(
    report_path_str: &str,
    summary: SummaryCounts,
    failures: &[FailureTriageEntry],
) -> Result<(), String> {
    let report = FailureTriageReport {
        total_tests: summary.total_tests,
        incorrect_results: summary.incorrect_results(),
        failures: failures.to_vec(),
    };

    let report_path = Path::new(report_path_str);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create triage report directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let report_json =
        serde_json::to_string_pretty(&report).map_err(|error| format!("JSON error: {error}"))?;
    fs::write(report_path, report_json).map_err(|error| {
        format!(
            "Failed to write triage report '{}': {error}",
            report_path.display()
        )
    })?;
    Ok(())
}
