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
mod runner;
mod types;

#[cfg(test)]
mod tests;

pub(crate) use runner::{normalize_relative_path, normalize_relative_path_text};
pub use runner::{run_all_test_cases, run_all_test_cases_with_backend_filter};
pub use types::IntegrationRunSummary;

pub(crate) use types::{
    ArtifactAssertion, ArtifactKind, BackendId, CaseExecutionResult, ExpectationMode,
    ExpectedOutcome, FailureExpectation, FailureKind, FailureTriageEntry, FailureTriageReport,
    GoldenMode, ManifestCaseSpec, ParsedBackendExpectation, ParsedExpectationFile,
    SuccessExpectation, SummaryCounts, TestCaseSpec, TestRunnerOptions, TestSuiteSpec,
    WarningExpectation,
};

pub(crate) const CANONICAL_TESTS_PATH: &str = "tests/cases";
pub(crate) const DEFAULT_EXPECT_STUB_PATH: &str = "tests/fixtures/stubs/expect.toml";
pub(crate) const MANIFEST_FILE_NAME: &str = "manifest.toml";
pub(crate) const EXPECT_FILE_NAME: &str = "expect.toml";
pub(crate) const INPUT_DIR_NAME: &str = "input";
pub(crate) const GOLDEN_DIR_NAME: &str = "golden";
pub(crate) const FAILURE_TRIAGE_REPORT_PATH: &str =
    "target/test-reports/integration_failure_triage.json";
pub(crate) const SEPARATOR_LINE_LENGTH: usize = 37;
