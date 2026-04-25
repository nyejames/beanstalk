//! Integration test runner type definitions.
//!
//! WHAT: defines the data types used by the integration test harness.

use crate::build_system::build::BuildResult;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_messages::compiler_errors::{CompilerMessages, ErrorType};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

pub(crate) const SUPPORTED_BACKEND_IDS: &[&str] = &["html", "html_wasm"];

#[derive(Clone, Copy)]
pub(crate) struct TestRunnerOptions {
    pub show_warnings: bool,
    pub backend_filter: Option<BackendId>,
}

pub(crate) struct TestSuiteSpec {
    pub cases: Vec<TestCaseSpec>,
}

#[derive(Clone)]
pub(crate) struct TestCaseSpec {
    pub display_name: String,
    pub backend_id: BackendId,
    pub entry_path: PathBuf,
    pub golden_dir: PathBuf,
    pub flags: Vec<Flag>,
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
    pub warnings: WarningExpectation,
    pub artifact_assertions: Vec<ArtifactAssertion>,
    pub golden_mode: GoldenMode,
    pub rendered_output_contains: Vec<String>,
    pub rendered_output_not_contains: Vec<String>,
}

#[derive(Clone)]
pub(crate) struct FailureExpectation {
    pub warnings: WarningExpectation,
    pub error_type: ErrorType,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub(crate) enum GoldenMode {
    #[default]
    Strict,
    Normalized,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum FailureKind {
    StrictGoldenMismatch,
    NormalizedSemanticMismatch,
    RenderedOutputMismatch,
    HarnessFailed,
    ExpectationViolation,
}

#[derive(Clone)]
pub(crate) struct ArtifactAssertion {
    pub path: String,
    pub kind: ArtifactKind,
    pub must_contain: Vec<String>,
    pub must_not_contain: Vec<String>,
    pub must_contain_in_order: Vec<String>,
    pub must_contain_exactly_once: Vec<String>,
    pub normalized_contains: Vec<String>,
    pub normalized_not_contains: Vec<String>,
    pub validate_wasm: bool,
    pub must_export: Vec<String>,
    pub must_import: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub(crate) enum BackendId {
    Html,
    HtmlWasm,
}

impl BackendId {
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

    pub(crate) fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::HtmlWasm => "html_wasm",
        }
    }

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
    pub failure_kind: Option<FailureKind>,
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
    pub fn record(&mut self, case: &TestCaseSpec, result: &CaseExecutionResult) {
        self.total_tests += 1;

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
    pub failure_kind: Option<FailureKind>,
    pub panic_message: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct FailureTriageReport {
    pub total_tests: usize,
    pub incorrect_results: usize,
    pub failures: Vec<FailureTriageEntry>,
}

pub(crate) struct ManifestCaseSpec {
    pub id: String,
    pub path: PathBuf,
}

pub(crate) struct ParsedExpectationFile {
    pub entry: Option<String>,
    pub backend_expectations: Vec<ParsedBackendExpectation>,
}

pub(crate) struct ParsedBackendExpectation {
    pub backend_id: BackendId,
    pub flags: Vec<Flag>,
    pub mode: ExpectationMode,
    pub warnings: WarningExpectation,
    pub error_type: Option<ErrorType>,
    pub message_contains: Vec<String>,
    pub artifact_assertions: Vec<ArtifactAssertion>,
    pub golden_mode: GoldenMode,
    pub rendered_output_contains: Vec<String>,
    pub rendered_output_not_contains: Vec<String>,
}
