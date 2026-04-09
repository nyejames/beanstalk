//! Integration test runner for end-to-end Beanstalk compiler coverage.
//!
//! Supports:
//! - canonical self-contained case folders under `tests/cases/<case>/`
//! - required manifest-driven case ordering and tags
//! - backend-specific expectation matrices from a shared input fixture

mod assertions;
mod execution;
mod expectations;
mod fixture;
mod manifest;
mod reporting;

#[cfg(test)]
mod tests;

use crate::build_system::build::BuildResult;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerMessages, ErrorType};
use saying::say;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

pub(crate) const CANONICAL_TESTS_PATH: &str = "tests/cases";
pub(crate) const MANIFEST_FILE_NAME: &str = "manifest.toml";
pub(crate) const EXPECT_FILE_NAME: &str = "expect.toml";
pub(crate) const INPUT_DIR_NAME: &str = "input";
pub(crate) const GOLDEN_DIR_NAME: &str = "golden";
const FAILURE_TRIAGE_REPORT_PATH: &str = "target/test-reports/integration_failure_triage.json";
const SEPARATOR_LINE_LENGTH: usize = 37;

/// Canonical backend IDs accepted by fixture expectation files and CLI filtering.
pub(crate) const SUPPORTED_BACKEND_IDS: &[&str] = &["html", "html_wasm"];

#[derive(Clone, Copy)]
pub(crate) struct TestRunnerOptions {
    /// Enables formatted warning rendering in case output blocks.
    pub show_warnings: bool,
    /// Optional backend filter applied while loading canonical cases.
    pub backend_filter: Option<BackendId>,
}

pub(crate) struct TestSuiteSpec {
    pub cases: Vec<TestCaseSpec>,
}

#[derive(Clone)]
pub(crate) struct TestCaseSpec {
    /// Rendered case label shown in test output (e.g. `case_name [html_wasm]`).
    pub display_name: String,
    /// Backend profile selected for this case execution.
    pub backend_id: BackendId,
    /// Entry path or entry directory resolved from fixture input and `entry` configuration.
    pub entry_path: PathBuf,
    /// Golden root to validate against for this specific backend execution.
    pub golden_dir: PathBuf,
    /// Final compiler flags for this backend execution.
    pub flags: Vec<Flag>,
    /// Expected success/failure contract for this backend execution.
    pub expected: ExpectedOutcome,
}

#[derive(Clone)]
pub(crate) enum ExpectedOutcome {
    Success(SuccessExpectation),
    Failure(FailureExpectation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub(crate) enum ExpectationMode {
    Success,
    Failure,
}

#[derive(Clone)]
pub(crate) struct SuccessExpectation {
    /// Warning assertion policy for successful builds.
    pub warnings: WarningExpectation,
    /// Additional backend-specific artifact checks.
    pub artifact_assertions: Vec<ArtifactAssertion>,
}

#[derive(Clone)]
pub(crate) struct FailureExpectation {
    /// Warning assertion policy for failed builds.
    pub warnings: WarningExpectation,
    /// Expected diagnostic error type.
    pub error_type: ErrorType,
    /// Ordered message fragments to prove the failure reason.
    pub message_contains: Vec<String>,
}

#[derive(Clone, Copy)]
pub(crate) enum WarningExpectation {
    Ignore,
    Forbid,
    Exact(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ArtifactKind {
    Html,
    Js,
    Wasm,
    Binary,
}

#[derive(Clone)]
pub(crate) struct ArtifactAssertion {
    /// Relative artifact path under the build output root.
    pub path: String,
    /// Artifact type expected at `path`.
    pub kind: ArtifactKind,
    /// Required text fragments for text artifacts (`html`/`js`).
    pub must_contain: Vec<String>,
    /// Forbidden text fragments for text artifacts (`html`/`js`).
    pub must_not_contain: Vec<String>,
    /// Required text fragments in order for text artifacts (`html`/`js`).
    ///
    /// Each element must appear in the artifact after the previous one.
    pub must_contain_in_order: Vec<String>,
    /// Text fragments that must appear exactly once in text artifacts (`html`/`js`).
    pub must_contain_exactly_once: Vec<String>,
    /// Enables wasmparser validation for `wasm` artifacts.
    pub validate_wasm: bool,
    /// Required export names for `wasm` artifacts.
    pub must_export: Vec<String>,
    /// Required import names for `wasm` artifacts as `module.item`.
    pub must_import: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum BackendId {
    Html,
    HtmlWasm,
}

impl BackendId {
    /// Parses a backend ID from fixture or CLI text.
    ///
    /// WHAT: maps stable string IDs to backend profiles.
    /// WHY: keeps expectation parsing and CLI filtering deterministic.
    pub(crate) fn parse(value: &str) -> Result<Self, String> {
        match value {
            "html" => Ok(Self::Html),
            "html_wasm" => Ok(Self::HtmlWasm),
            other => Err(format!(
                "Unsupported backend '{other}'. Supported backends: {}",
                SUPPORTED_BACKEND_IDS.join(", ")
            )),
        }
    }

    /// Stable user-facing backend identifier.
    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::HtmlWasm => "html_wasm",
        }
    }

    /// Backend-default flags applied to each case execution.
    ///
    /// WHAT: ensures backend mode is enabled without repeating flags in every fixture.
    /// WHY: keeps matrix configuration concise and resilient to profile growth.
    pub(crate) fn default_flags(self) -> Vec<Flag> {
        match self {
            Self::Html => Vec::new(),
            Self::HtmlWasm => vec![Flag::HtmlWasm],
        }
    }
}

pub(crate) struct CaseExecutionResult {
    pub passed: bool,
    pub panic_message: Option<String>,
    pub build_result: Option<BuildResult>,
    pub messages: Option<CompilerMessages>,
    pub failure_reason: Option<String>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct SummaryCounts {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub expected_failures: usize,
    pub unexpected_successes: usize,
}

impl SummaryCounts {
    /// Records one case execution against summary counters.
    ///
    /// WHAT: updates global/backend stats using expected-outcome semantics.
    /// WHY: expected-failure accounting must stay consistent in one place.
    pub fn record(&mut self, case: &TestCaseSpec, result: &CaseExecutionResult) {
        self.total_tests += 1;

        // Route each execution into success/failure buckets while preserving
        // expected-failure accounting semantics.
        if result.passed {
            match case.expected {
                ExpectedOutcome::Success(_) => self.passed_tests += 1,
                ExpectedOutcome::Failure(_) => self.expected_failures += 1,
            }
        } else {
            match case.expected {
                ExpectedOutcome::Success(_) => self.failed_tests += 1,
                ExpectedOutcome::Failure(_) => self.unexpected_successes += 1,
            }
        }
    }

    pub fn correct_results(&self) -> usize {
        self.passed_tests + self.expected_failures
    }

    pub fn incorrect_results(&self) -> usize {
        self.failed_tests + self.unexpected_successes
    }
}

/// Aggregate integration-suite execution summary returned to callers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IntegrationRunSummary {
    pub total_tests: usize,
    pub passed_tests: usize,
    pub failed_tests: usize,
    pub expected_failures: usize,
    pub unexpected_successes: usize,
}

impl IntegrationRunSummary {
    pub fn incorrect_results(&self) -> usize {
        self.failed_tests + self.unexpected_successes
    }
}

impl From<SummaryCounts> for IntegrationRunSummary {
    fn from(value: SummaryCounts) -> Self {
        Self {
            total_tests: value.total_tests,
            passed_tests: value.passed_tests,
            failed_tests: value.failed_tests,
            expected_failures: value.expected_failures,
            unexpected_successes: value.unexpected_successes,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FailureTriageEntry {
    pub case: String,
    pub backend: String,
    pub expected_outcome: &'static str,
    pub failure_reason: String,
    pub panic_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FailureTriageReport {
    pub total_tests: usize,
    pub incorrect_results: usize,
    pub failures: Vec<FailureTriageEntry>,
}

/// Manifest-level case reference resolved before fixture loading.
pub(crate) struct ManifestCaseSpec {
    pub id: String,
    pub path: PathBuf,
}

/// Parsed expectation file ready for case expansion.
pub(crate) struct ParsedExpectationFile {
    /// Optional entry override shared across all backend executions.
    pub entry: Option<String>,
    /// Parsed backend expectations ready for case expansion.
    pub backend_expectations: Vec<ParsedBackendExpectation>,
}

/// Per-backend expectation block after TOML parsing.
pub(crate) struct ParsedBackendExpectation {
    /// Backend profile selected for this expectation block.
    pub backend_id: BackendId,
    /// Additional fixture flags layered on top of backend defaults.
    pub flags: Vec<Flag>,
    /// Success/failure mode for this backend run.
    pub mode: ExpectationMode,
    /// Warning expectation policy.
    pub warnings: WarningExpectation,
    /// Expected error type for failure mode.
    pub error_type: Option<ErrorType>,
    /// Ordered failure message fragments.
    pub message_contains: Vec<String>,
    /// Additional artifact assertions for success mode.
    pub artifact_assertions: Vec<ArtifactAssertion>,
}

/// Normalises a relative path string to forward slashes for cross-platform comparison.
pub(crate) fn normalize_relative_path_text(path: &str) -> String {
    path.replace('\\', "/")
}

/// Normalises a `Path` to a forward-slash string for cross-platform comparison.
pub(crate) fn normalize_relative_path(path: &std::path::Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

/// Runs all test cases from the `tests/cases` directory.
pub fn run_all_test_cases(show_warnings: bool) -> Result<IntegrationRunSummary, String> {
    run_all_test_cases_with_backend_filter(show_warnings, None)
}

/// Runs all test cases with an optional backend filter.
///
/// WHAT: narrows execution to one backend profile when requested.
/// WHY: backend-focused loops should avoid rebuilding unrelated fixture variants.
pub fn run_all_test_cases_with_backend_filter(
    show_warnings: bool,
    backend_filter: Option<&str>,
) -> Result<IntegrationRunSummary, String> {
    let backend_filter = match backend_filter {
        Some(raw_backend) => Some(BackendId::parse(raw_backend)?),
        None => None,
    };

    println!("Running all Beanstalk test cases...\n");
    let timer = std::time::Instant::now();
    let options = TestRunnerOptions {
        show_warnings,
        backend_filter,
    };

    let suite = fixture::load_test_suite(options.backend_filter)?;

    let mut total_summary = SummaryCounts::default();
    let mut backend_summaries = BTreeMap::<BackendId, SummaryCounts>::new();
    let mut failure_triage_entries = Vec::new();

    if !suite.cases.is_empty() {
        say!(Cyan "Testing integration cases:");
        say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));

        for case in &suite.cases {
            println!("  {}", case.display_name);

            let result = execution::execute_test_case(case);
            reporting::render_case_result(case, &result, options.show_warnings);

            total_summary.record(case, &result);
            backend_summaries
                .entry(case.backend_id)
                .or_default()
                .record(case, &result);

            if !result.passed {
                failure_triage_entries.push(FailureTriageEntry {
                    case: case.display_name.clone(),
                    backend: case.backend_id.as_str().to_string(),
                    expected_outcome: reporting::expected_outcome_label(&case.expected),
                    failure_reason: reporting::observed_failure_reason(&result),
                    panic_message: result.panic_message.clone(),
                });
            }

            say!(Dark White "-".repeat(SEPARATOR_LINE_LENGTH));
        }

        println!();
    }

    println!();
    say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));
    print!("Test Results Summary. Took: ");
    say!(Green #timer.elapsed());

    say!("\n  Total tests:             ", Yellow total_summary.total_tests);
    say!(
        "  Successful compilations: ",
        Blue total_summary.passed_tests
    );
    say!(
        "  Failed compilations:     ",
        Blue total_summary.failed_tests
    );
    say!(
        "  Expected failures:       ",
        Blue total_summary.expected_failures
    );
    say!(
        "  Unexpected successes:    ",
        Blue total_summary.unexpected_successes
    );

    say!();
    say!(
        "  Correct results:   ",
        Green Bold total_summary.correct_results(),
        Dark White " / ",
        total_summary.total_tests
    );
    say!(
        "  Incorrect results: ",
        Red Bold total_summary.incorrect_results(),
        Dark White " / ",
        total_summary.total_tests
    );

    reporting::render_backend_summary(&backend_summaries);

    if total_summary.incorrect_results() == 0 {
        say!("\nAll tests behaved as expected.");
    } else if total_summary.total_tests > 0 {
        let percentage = reporting::format_pass_percentage(
            total_summary.correct_results(),
            total_summary.total_tests,
        );
        say!(
            Yellow "\n",
            Bright Yellow percentage,
            " %",
            Reset " of tests behaved as expected"
        );
    }

    if let Err(error) = reporting::write_failure_triage_report(
        FAILURE_TRIAGE_REPORT_PATH,
        total_summary,
        &failure_triage_entries,
    ) {
        say!(Yellow format!(
            "Failed to write machine-readable triage report: {error}"
        ));
    }

    say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));
    Ok(total_summary.into())
}
