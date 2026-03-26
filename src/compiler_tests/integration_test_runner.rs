//! Integration test runner for end-to-end Beanstalk compiler coverage.
//!
//! Supports:
//! - canonical self-contained case folders under `tests/cases/<case>/`
//! - optional manifest-driven case ordering
//! - backend-specific expectation matrices from a shared input fixture

use crate::build_system::build::{
    BuildResult, FileKind, OutputFile, ProjectBuilder, build_project,
};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerMessages, ErrorType, error_type_to_str,
};
use crate::compiler_frontend::compiler_messages::compiler_warnings::print_formatted_warning;
use crate::compiler_frontend::display_messages::print_formatted_error;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use saying::say;
use serde::Deserialize;
use std::any::Any;
use std::collections::{BTreeMap, HashSet};
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};
use wasmparser::{Parser, Payload};

const CANONICAL_TESTS_PATH: &str = "tests/cases";
const MANIFEST_FILE_NAME: &str = "manifest.toml";
const EXPECT_FILE_NAME: &str = "expect.toml";
const INPUT_DIR_NAME: &str = "input";
const GOLDEN_DIR_NAME: &str = "golden";
const SEPARATOR_LINE_LENGTH: usize = 37;

/// Canonical backend IDs accepted by fixture expectation files and CLI filtering.
const SUPPORTED_BACKEND_IDS: &[&str] = &["html", "html_wasm"];

#[derive(Clone, Copy)]
struct TestRunnerOptions {
    /// Enables formatted warning rendering in case output blocks.
    show_warnings: bool,
    /// Optional backend filter applied while loading canonical cases.
    backend_filter: Option<BackendId>,
}

struct TestSuiteSpec {
    cases: Vec<TestCaseSpec>,
}

#[derive(Clone)]
struct TestCaseSpec {
    /// Rendered case label shown in test output (e.g. `case_name [html_wasm]`).
    display_name: String,
    /// Backend profile selected for this case execution.
    backend_id: BackendId,
    /// Entry path or entry directory resolved from fixture input and `entry` configuration.
    entry_path: PathBuf,
    /// Golden root to validate against for this specific backend execution.
    golden_dir: PathBuf,
    /// Final compiler flags for this backend execution.
    flags: Vec<Flag>,
    /// Expected success/failure contract for this backend execution.
    expected: ExpectedOutcome,
}

#[derive(Clone)]
enum ExpectedOutcome {
    Success(SuccessExpectation),
    Failure(FailureExpectation),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExpectationMode {
    Success,
    Failure,
}

#[derive(Clone)]
struct SuccessExpectation {
    /// Warning assertion policy for successful builds.
    warnings: WarningExpectation,
    /// Additional backend-specific artifact checks.
    artifact_assertions: Vec<ArtifactAssertion>,
}

#[derive(Clone)]
struct FailureExpectation {
    /// Allows panic passthrough for known in-progress failure cases.
    allow_panic: bool,
    /// Warning assertion policy for failed builds.
    warnings: WarningExpectation,
    /// Expected diagnostic error type.
    error_type: ErrorType,
    /// Ordered message fragments to prove the failure reason.
    message_contains: Vec<String>,
}

#[derive(Clone, Copy)]
enum WarningExpectation {
    Ignore,
    Forbid,
    Exact(usize),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ArtifactKind {
    Html,
    Js,
    Wasm,
}

#[derive(Clone)]
struct ArtifactAssertion {
    /// Relative artifact path under the build output root.
    path: String,
    /// Artifact type expected at `path`.
    kind: ArtifactKind,
    /// Required text fragments for text artifacts (`html`/`js`).
    must_contain: Vec<String>,
    /// Forbidden text fragments for text artifacts (`html`/`js`).
    must_not_contain: Vec<String>,
    /// Enables wasmparser validation for `wasm` artifacts.
    validate_wasm: bool,
    /// Required export names for `wasm` artifacts.
    must_export: Vec<String>,
    /// Required import names for `wasm` artifacts as `module.item`.
    must_import: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Ord, PartialOrd)]
enum BackendId {
    Html,
    HtmlWasm,
}

impl BackendId {
    /// Parses a backend ID from fixture or CLI text.
    ///
    /// WHAT: maps stable string IDs to backend profiles.
    /// WHY: keeps expectation parsing and CLI filtering deterministic.
    fn parse(value: &str) -> Result<Self, String> {
        match value {
            "html" => Ok(Self::Html),
            "html_wasm" => Ok(Self::HtmlWasm),
            other => Err(format!(
                "Unsupported backend '{other}'. Supported backends: {}",
                supported_backend_ids_text()
            )),
        }
    }

    /// Stable user-facing backend identifier.
    fn as_str(self) -> &'static str {
        match self {
            Self::Html => "html",
            Self::HtmlWasm => "html_wasm",
        }
    }

    /// Backend-default flags applied to each case execution.
    ///
    /// WHAT: ensures backend mode is enabled without repeating flags in every fixture.
    /// WHY: keeps matrix configuration concise and resilient to profile growth.
    fn default_flags(self) -> Vec<Flag> {
        match self {
            Self::Html => Vec::new(),
            Self::HtmlWasm => vec![Flag::HtmlWasm],
        }
    }
}

struct CaseExecutionResult {
    passed: bool,
    panic_message: Option<String>,
    build_result: Option<BuildResult>,
    messages: Option<CompilerMessages>,
    failure_reason: Option<String>,
}

#[derive(Default)]
struct SummaryCounts {
    total_tests: usize,
    passed_tests: usize,
    failed_tests: usize,
    expected_failures: usize,
    unexpected_successes: usize,
}

impl SummaryCounts {
    /// Records one case execution against summary counters.
    ///
    /// WHAT: updates global/backend stats using expected-outcome semantics.
    /// WHY: expected-failure accounting must stay consistent in one place.
    fn record(&mut self, case: &TestCaseSpec, result: &CaseExecutionResult) {
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

    fn correct_results(&self) -> usize {
        self.passed_tests + self.expected_failures
    }

    fn incorrect_results(&self) -> usize {
        self.failed_tests + self.unexpected_successes
    }
}

struct ManifestCaseSpec {
    id: String,
    path: PathBuf,
    _tags: Vec<String>,
}

struct ParsedExpectationFile {
    /// Optional entry override shared across all backend executions.
    entry: Option<String>,
    /// Parsed backend expectations ready for case expansion.
    backend_expectations: Vec<ParsedBackendExpectation>,
}

struct ParsedBackendExpectation {
    /// Backend profile selected for this expectation block.
    backend_id: BackendId,
    /// Additional fixture flags layered on top of backend defaults.
    flags: Vec<Flag>,
    /// Success/failure mode for this backend run.
    mode: ExpectationMode,
    /// Panic allowance toggle for failure-mode cases.
    allow_panic: bool,
    /// Warning expectation policy.
    warnings: WarningExpectation,
    /// Expected error type for failure mode.
    error_type: Option<ErrorType>,
    /// Ordered failure message fragments.
    message_contains: Vec<String>,
    /// Additional artifact assertions for success mode.
    artifact_assertions: Vec<ArtifactAssertion>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestToml {
    #[serde(default)]
    case: Vec<ManifestCaseToml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestCaseToml {
    id: String,
    path: String,
    #[serde(default)]
    tags: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectationToml {
    mode: Option<ExpectationMode>,
    entry: Option<String>,
    #[serde(default)]
    flags: Vec<String>,
    builder: Option<String>,
    #[serde(rename = "panic", default)]
    allow_panic: bool,
    warnings: Option<String>,
    warning_count: Option<usize>,
    error_type: Option<String>,
    #[serde(default)]
    message_contains: Vec<String>,
    #[serde(default)]
    artifact_assertions: Vec<ArtifactAssertionToml>,
    #[serde(default)]
    backends: BTreeMap<String, BackendExpectationToml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackendExpectationToml {
    mode: ExpectationMode,
    #[serde(default)]
    flags: Vec<String>,
    #[serde(rename = "panic", default)]
    allow_panic: bool,
    warnings: Option<String>,
    warning_count: Option<usize>,
    error_type: Option<String>,
    #[serde(default)]
    message_contains: Vec<String>,
    #[serde(default)]
    artifact_assertions: Vec<ArtifactAssertionToml>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactAssertionToml {
    path: String,
    kind: String,
    #[serde(default)]
    must_contain: Vec<String>,
    #[serde(default)]
    must_not_contain: Vec<String>,
    #[serde(default)]
    validate_wasm: bool,
    #[serde(default)]
    must_export: Vec<String>,
    #[serde(default)]
    must_import: Vec<String>,
}

/// Runs all test cases from the `tests/cases` directory.
pub fn run_all_test_cases(show_warnings: bool) {
    run_all_test_cases_with_backend_filter(show_warnings, None);
}

/// Runs all test cases with an optional backend filter.
///
/// WHAT: narrows execution to one backend profile when requested.
/// WHY: backend-focused loops should avoid rebuilding unrelated fixture variants.
pub fn run_all_test_cases_with_backend_filter(show_warnings: bool, backend_filter: Option<&str>) {
    let backend_filter = match backend_filter {
        Some(raw_backend) => match BackendId::parse(raw_backend) {
            Ok(backend) => Some(backend),
            Err(error) => {
                say!(Red "Failed to parse backend filter:");
                println!("  {error}");
                return;
            }
        },
        None => None,
    };

    println!("Running all Beanstalk test cases...\n");
    let timer = std::time::Instant::now();
    let options = TestRunnerOptions {
        show_warnings,
        backend_filter,
    };

    let suite = match load_test_suite(options.backend_filter) {
        Ok(spec) => spec,
        Err(error) => {
            say!(Red "Failed to load integration test suite:");
            println!("  {error}");
            return;
        }
    };

    let mut total_summary = SummaryCounts::default();
    let mut backend_summaries = BTreeMap::<BackendId, SummaryCounts>::new();

    if !suite.cases.is_empty() {
        say!(Cyan "Testing integration cases:");
        say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));

        for case in &suite.cases {
            println!("  {}", case.display_name);

            let result = execute_test_case(case);
            render_case_result(case, &result, options.show_warnings);

            total_summary.record(case, &result);
            backend_summaries
                .entry(case.backend_id)
                .or_default()
                .record(case, &result);

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

    render_backend_summary(&backend_summaries);

    if total_summary.incorrect_results() == 0 {
        say!("\nAll tests behaved as expected.");
    } else if total_summary.total_tests > 0 {
        let percentage =
            (total_summary.correct_results() as f64 / total_summary.total_tests as f64) * 100.0;
        say!(
            Yellow "\n",
            Bright Yellow format!("{percentage:.1}"),
            " %",
            Reset " of tests behaved as expected"
        );
    }

    say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));
}

fn render_backend_summary(backend_summaries: &BTreeMap<BackendId, SummaryCounts>) {
    if backend_summaries.is_empty() {
        return;
    }

    say!("\n  Backend breakdown:");
    for (backend_id, summary) in backend_summaries {
        say!(format!(
            "    {:<9} total={} pass={} fail={} expected_failures={} unexpected_successes={}",
            backend_id.as_str(),
            summary.total_tests,
            summary.passed_tests,
            summary.failed_tests,
            summary.expected_failures,
            summary.unexpected_successes
        ));
    }
}

fn load_test_suite(backend_filter: Option<BackendId>) -> Result<TestSuiteSpec, String> {
    load_test_suite_from_root_with_filter(Path::new(CANONICAL_TESTS_PATH), backend_filter)
}

#[cfg(test)]
fn load_test_suite_from_root(root: &Path) -> Result<TestSuiteSpec, String> {
    load_test_suite_from_root_with_filter(root, None)
}

fn load_test_suite_from_root_with_filter(
    root: &Path,
    backend_filter: Option<BackendId>,
) -> Result<TestSuiteSpec, String> {
    let mut cases = Vec::new();
    let mut loaded_canonical_paths = HashSet::new();

    let manifest_path = root.join(MANIFEST_FILE_NAME);
    if manifest_path.is_file() {
        for manifest_case in parse_manifest_file(&manifest_path)? {
            let fixture_root = root.join(&manifest_case.path);
            let case_specs =
                load_canonical_case_specs(&fixture_root, Some(manifest_case.id), backend_filter)?;
            loaded_canonical_paths.insert(fs::canonicalize(&fixture_root).unwrap_or(fixture_root));
            cases.extend(case_specs);
        }
    }

    if root.is_dir() {
        let entries = fs::read_dir(root).map_err(|error| {
            format!(
                "Failed to read canonical test root '{}': {error}",
                root.display()
            )
        })?;

        let mut discovered_dirs = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|error| format!("Failed to read test entry: {error}"))?;
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };

            if matches!(name, "success" | "failure") {
                continue;
            }

            if !(path.join(INPUT_DIR_NAME).is_dir() && path.join(EXPECT_FILE_NAME).is_file()) {
                continue;
            }

            discovered_dirs.push(path);
        }

        discovered_dirs.sort();

        for fixture_root in discovered_dirs {
            let canonical_path =
                fs::canonicalize(&fixture_root).unwrap_or_else(|_| fixture_root.clone());
            if loaded_canonical_paths.contains(&canonical_path) {
                continue;
            }

            cases.extend(load_canonical_case_specs(
                &fixture_root,
                None,
                backend_filter,
            )?);
            loaded_canonical_paths.insert(canonical_path);
        }
    }

    Ok(TestSuiteSpec { cases })
}

fn parse_manifest_file(path: &Path) -> Result<Vec<ManifestCaseSpec>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read manifest '{}': {error}", path.display()))?;

    let parsed: ManifestToml = toml::from_str(&source).map_err(|error| {
        format!(
            "Failed to parse manifest '{}' as TOML: {error}",
            path.display()
        )
    })?;

    let mut cases = Vec::with_capacity(parsed.case.len());
    for case in parsed.case {
        if case.id.trim().is_empty() {
            return Err(format!(
                "Manifest '{}' has a case with an empty id",
                path.display()
            ));
        }
        if case.path.trim().is_empty() {
            return Err(format!(
                "Manifest '{}' has a case with an empty path",
                path.display()
            ));
        }

        cases.push(ManifestCaseSpec {
            id: case.id,
            path: PathBuf::from(case.path),
            _tags: case.tags,
        });
    }

    Ok(cases)
}

fn load_canonical_case_specs(
    fixture_root: &Path,
    explicit_id: Option<String>,
    backend_filter: Option<BackendId>,
) -> Result<Vec<TestCaseSpec>, String> {
    let input_root = fixture_root.join(INPUT_DIR_NAME);
    let expect_path = fixture_root.join(EXPECT_FILE_NAME);

    if !input_root.is_dir() {
        return Err(format!(
            "Canonical fixture '{}' is missing '{}'",
            fixture_root.display(),
            INPUT_DIR_NAME
        ));
    }

    let parsed_expectation = parse_expectation_file(&expect_path)?;
    validate_fixture_contract(fixture_root, &parsed_expectation)?;
    let entry_path = resolve_case_entry_path(&input_root, parsed_expectation.entry.as_deref())?;
    let case_id = explicit_id.unwrap_or_else(|| {
        fixture_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unnamed_case")
            .to_string()
    });

    let mut case_specs = Vec::new();
    for backend_expectation in parsed_expectation.backend_expectations {
        if let Some(selected_backend) = backend_filter
            && selected_backend != backend_expectation.backend_id
        {
            continue;
        }

        let expected = match backend_expectation.mode {
            ExpectationMode::Success => ExpectedOutcome::Success(SuccessExpectation {
                warnings: backend_expectation.warnings,
                artifact_assertions: backend_expectation.artifact_assertions,
            }),
            ExpectationMode::Failure => ExpectedOutcome::Failure(FailureExpectation {
                allow_panic: backend_expectation.allow_panic,
                warnings: backend_expectation.warnings,
                error_type: backend_expectation.error_type.ok_or_else(|| {
                    format!(
                        "Canonical fixture '{}' backend '{}' is missing required 'error_type' for failure mode.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    )
                })?,
                message_contains: backend_expectation.message_contains,
            }),
        };

        let flags = merge_flags(
            backend_expectation.backend_id.default_flags(),
            backend_expectation.flags,
        );
        let backend_name = backend_expectation.backend_id.as_str();
        let golden_dir = golden_dir_for_backend(fixture_root, backend_expectation.backend_id);

        case_specs.push(TestCaseSpec {
            display_name: format!("{case_id} [{backend_name}]"),
            backend_id: backend_expectation.backend_id,
            entry_path: entry_path.clone(),
            golden_dir,
            flags,
            expected,
        });
    }

    Ok(case_specs)
}

fn merge_flags(default_flags: Vec<Flag>, extra_flags: Vec<Flag>) -> Vec<Flag> {
    // Default backend flags establish the runtime mode, while fixture flags
    // can layer additional toggles without duplicating the same flag value.
    let mut merged = default_flags;
    for flag in extra_flags {
        if !merged.contains(&flag) {
            merged.push(flag);
        }
    }

    merged
}

fn parse_expectation_file(path: &Path) -> Result<ParsedExpectationFile, String> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "Failed to read expectation file '{}': {error}",
            path.display()
        )
    })?;

    let parsed: ExpectationToml = toml::from_str(&source).map_err(|error| {
        format!(
            "Failed to parse expectation file '{}' as TOML: {error}",
            path.display()
        )
    })?;

    if let Some(builder) = &parsed.builder
        && builder != "html"
    {
        return Err(format!(
            "Expectation file '{}' only supports builder = \"html\" right now",
            path.display()
        ));
    }

    let matrix_mode = !parsed.backends.is_empty();
    if matrix_mode {
        return parse_matrix_expectation_file(path, parsed);
    }

    parse_legacy_expectation_file(path, parsed)
}

fn parse_legacy_expectation_file(
    path: &Path,
    parsed: ExpectationToml,
) -> Result<ParsedExpectationFile, String> {
    let mode = parsed.mode.ok_or_else(|| {
        format!(
            "Expectation file '{}' is missing required key 'mode'.",
            path.display()
        )
    })?;

    let warnings =
        parse_warning_expectation(parsed.warnings.as_deref(), parsed.warning_count, path, "")?;
    let flags = parse_case_flags(&parsed.flags, path, "")?;
    let backend_id = if flags.contains(&Flag::HtmlWasm) {
        BackendId::HtmlWasm
    } else {
        BackendId::Html
    };
    let error_type = parsed
        .error_type
        .as_deref()
        .map(parse_error_type)
        .transpose()?;

    let artifact_assertions = parse_artifact_assertions(path, "", &parsed.artifact_assertions)?;

    Ok(ParsedExpectationFile {
        entry: parsed.entry,
        backend_expectations: vec![ParsedBackendExpectation {
            backend_id,
            flags,
            mode,
            allow_panic: parsed.allow_panic,
            warnings,
            error_type,
            message_contains: parsed.message_contains,
            artifact_assertions,
        }],
    })
}

fn parse_matrix_expectation_file(
    path: &Path,
    parsed: ExpectationToml,
) -> Result<ParsedExpectationFile, String> {
    // In matrix mode, all mode/outcome keys must be declared inside explicit
    // backend sections so each backend can evolve independently.
    if parsed.mode.is_some()
        || !parsed.flags.is_empty()
        || parsed.allow_panic
        || parsed.warnings.is_some()
        || parsed.warning_count.is_some()
        || parsed.error_type.is_some()
        || !parsed.message_contains.is_empty()
        || !parsed.artifact_assertions.is_empty()
    {
        return Err(format!(
            "Expectation file '{}' uses backend matrix mode and must keep mode/warnings/flags/error/artifact keys inside '[backends.<id>]'.",
            path.display()
        ));
    }

    let mut backend_expectations = Vec::new();
    for (backend_key, backend_expectation) in parsed.backends {
        let backend_id = BackendId::parse(&backend_key).map_err(|error| {
            format!(
                "Expectation file '{}' has invalid backend key '{}': {error}",
                path.display(),
                backend_key
            )
        })?;
        let context = format!("[backends.{}]", backend_id.as_str());
        let warnings = parse_warning_expectation(
            backend_expectation.warnings.as_deref(),
            backend_expectation.warning_count,
            path,
            &context,
        )?;
        let flags = parse_case_flags(&backend_expectation.flags, path, &context)?;
        let error_type = backend_expectation
            .error_type
            .as_deref()
            .map(parse_error_type)
            .transpose()?;

        let artifact_assertions =
            parse_artifact_assertions(path, &context, &backend_expectation.artifact_assertions)?;

        backend_expectations.push(ParsedBackendExpectation {
            backend_id,
            flags,
            mode: backend_expectation.mode,
            allow_panic: backend_expectation.allow_panic,
            warnings,
            error_type,
            message_contains: backend_expectation.message_contains,
            artifact_assertions,
        });
    }

    Ok(ParsedExpectationFile {
        entry: parsed.entry,
        backend_expectations,
    })
}

fn parse_artifact_assertions(
    path: &Path,
    context: &str,
    assertions: &[ArtifactAssertionToml],
) -> Result<Vec<ArtifactAssertion>, String> {
    let mut parsed_assertions = Vec::with_capacity(assertions.len());

    for (index, assertion) in assertions.iter().enumerate() {
        let assertion_label = if context.is_empty() {
            format!("artifact_assertions[{index}]")
        } else {
            format!("{context}.artifact_assertions[{index}]")
        };

        if assertion.path.trim().is_empty() {
            return Err(format!(
                "Expectation file '{}' {} requires a non-empty 'path'.",
                path.display(),
                assertion_label
            ));
        }

        let kind = parse_artifact_kind(path, &assertion.kind, &assertion_label)?;
        validate_artifact_strings(
            path,
            &assertion_label,
            "must_contain",
            &assertion.must_contain,
        )?;
        validate_artifact_strings(
            path,
            &assertion_label,
            "must_not_contain",
            &assertion.must_not_contain,
        )?;
        validate_artifact_strings(
            path,
            &assertion_label,
            "must_export",
            &assertion.must_export,
        )?;
        validate_artifact_strings(
            path,
            &assertion_label,
            "must_import",
            &assertion.must_import,
        )?;

        match kind {
            ArtifactKind::Html | ArtifactKind::Js => {
                if assertion.validate_wasm
                    || !assertion.must_export.is_empty()
                    || !assertion.must_import.is_empty()
                {
                    return Err(format!(
                        "Expectation file '{}' {} uses wasm-only fields on a text artifact assertion.",
                        path.display(),
                        assertion_label
                    ));
                }
                if assertion.must_contain.is_empty() && assertion.must_not_contain.is_empty() {
                    return Err(format!(
                        "Expectation file '{}' {} must define 'must_contain' and/or 'must_not_contain' for text artifacts.",
                        path.display(),
                        assertion_label
                    ));
                }
            }
            ArtifactKind::Wasm => {
                if !assertion.must_contain.is_empty() || !assertion.must_not_contain.is_empty() {
                    return Err(format!(
                        "Expectation file '{}' {} uses text-only fields on a wasm artifact assertion.",
                        path.display(),
                        assertion_label
                    ));
                }
                if !assertion.validate_wasm
                    && assertion.must_export.is_empty()
                    && assertion.must_import.is_empty()
                {
                    return Err(format!(
                        "Expectation file '{}' {} must enable 'validate_wasm' or require imports/exports for wasm assertions.",
                        path.display(),
                        assertion_label
                    ));
                }
            }
        }

        parsed_assertions.push(ArtifactAssertion {
            path: normalize_relative_path_text(&assertion.path),
            kind,
            must_contain: assertion.must_contain.clone(),
            must_not_contain: assertion.must_not_contain.clone(),
            validate_wasm: assertion.validate_wasm,
            must_export: assertion.must_export.clone(),
            must_import: assertion.must_import.clone(),
        });
    }

    Ok(parsed_assertions)
}

fn validate_artifact_strings(
    path: &Path,
    assertion_label: &str,
    field_name: &str,
    values: &[String],
) -> Result<(), String> {
    for value in values {
        if value.is_empty() {
            return Err(format!(
                "Expectation file '{}' {} contains an empty '{}' value.",
                path.display(),
                assertion_label,
                field_name
            ));
        }
    }

    Ok(())
}

fn parse_artifact_kind(
    path: &Path,
    raw_kind: &str,
    assertion_label: &str,
) -> Result<ArtifactKind, String> {
    match raw_kind {
        "html" => Ok(ArtifactKind::Html),
        "js" => Ok(ArtifactKind::Js),
        "wasm" => Ok(ArtifactKind::Wasm),
        other => Err(format!(
            "Expectation file '{}' {} has unsupported artifact kind '{}'.",
            path.display(),
            assertion_label,
            other
        )),
    }
}

fn validate_fixture_contract(
    fixture_root: &Path,
    expectation: &ParsedExpectationFile,
) -> Result<(), String> {
    if expectation.backend_expectations.is_empty() {
        return Err(format!(
            "Fixture '{}' does not define any backend expectations.",
            fixture_root.display()
        ));
    }

    for backend_expectation in &expectation.backend_expectations {
        let golden_dir = golden_dir_for_backend(fixture_root, backend_expectation.backend_id);
        let has_golden_dir = golden_dir.is_dir();
        let has_artifact_assertions = !backend_expectation.artifact_assertions.is_empty();
        let has_backend_baseline_contract =
            backend_has_builtin_success_contract(backend_expectation.backend_id);

        match backend_expectation.mode {
            ExpectationMode::Failure => {
                if backend_expectation.error_type.is_none() {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"failure\" but is missing required 'error_type'.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
                if backend_expectation.message_contains.is_empty() {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"failure\" but is missing required 'message_contains'.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
                if has_artifact_assertions {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"failure\" and must not define artifact assertions.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
            }
            ExpectationMode::Success => {
                if !has_golden_dir && !has_artifact_assertions && !has_backend_baseline_contract {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"success\" and must provide artifact assertions and/or a '{}' directory.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str(),
                        golden_dir.display()
                    ));
                }
                if backend_expectation.allow_panic {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"success\" and cannot set panic = true.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
                if backend_expectation.error_type.is_some()
                    || !backend_expectation.message_contains.is_empty()
                {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"success\" and must not set failure-only keys ('error_type'/'message_contains').",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
            }
        }
    }

    Ok(())
}

fn parse_warning_expectation(
    warnings_mode: Option<&str>,
    warning_count: Option<usize>,
    path: &Path,
    context: &str,
) -> Result<WarningExpectation, String> {
    let context_prefix = if context.is_empty() {
        String::new()
    } else {
        format!("{context} ")
    };

    let Some(mode) = warnings_mode else {
        return Err(format!(
            "Expectation file '{}' {}is missing required key 'warnings'.",
            path.display(),
            context_prefix
        ));
    };

    match mode {
        "ignore" => {
            if warning_count.is_some() {
                return Err(format!(
                    "Expectation file '{}' {}sets 'warning_count' but warnings != \"exact\".",
                    path.display(),
                    context_prefix
                ));
            }
            Ok(WarningExpectation::Ignore)
        }
        "forbid" => {
            if warning_count.is_some() {
                return Err(format!(
                    "Expectation file '{}' {}sets 'warning_count' but warnings != \"exact\".",
                    path.display(),
                    context_prefix
                ));
            }
            Ok(WarningExpectation::Forbid)
        }
        "exact" => {
            let expected_count = warning_count.ok_or_else(|| {
                format!(
                    "Expectation file '{}' {}uses warnings = \"exact\" but is missing 'warning_count'.",
                    path.display(),
                    context_prefix
                )
            })?;
            Ok(WarningExpectation::Exact(expected_count))
        }
        other => Err(format!(
            "Expectation file '{}' {}has unsupported warnings mode '{other}'.",
            path.display(),
            context_prefix
        )),
    }
}

fn parse_case_flags(
    flag_names: &[String],
    path: &Path,
    context: &str,
) -> Result<Vec<Flag>, String> {
    let mut flags = Vec::with_capacity(flag_names.len());
    for flag_name in flag_names {
        let parsed = match flag_name.as_str() {
            "release" => Flag::Release,
            "hide_warnings" => Flag::DisableWarnings,
            "hide_timers" => Flag::DisableTimers,
            "html_wasm" => Flag::HtmlWasm,
            other => {
                if context.is_empty() {
                    return Err(format!(
                        "Expectation file '{}' has unsupported flag '{}'.",
                        path.display(),
                        other
                    ));
                }
                return Err(format!(
                    "Expectation file '{}' {} has unsupported flag '{}'.",
                    path.display(),
                    context,
                    other
                ));
            }
        };
        flags.push(parsed);
    }
    Ok(flags)
}

fn resolve_case_entry_path(
    input_root: &Path,
    configured_entry: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(entry) = configured_entry {
        if entry == "." {
            return Ok(input_root.to_path_buf());
        }

        return Ok(input_root.join(entry));
    }

    let default_entry = input_root.join("#page.bst");
    if default_entry.is_file() {
        return Ok(default_entry);
    }

    Err(format!(
        "Could not determine canonical test entry for '{}'. Add 'entry = ...' to '{}' or provide #page.bst.",
        input_root.display(),
        EXPECT_FILE_NAME
    ))
}

fn execute_test_case(case: &TestCaseSpec) -> CaseExecutionResult {
    let builder = backend_builder_for_case(case.backend_id);
    let mut flags = vec![Flag::DisableTimers];
    flags.extend(case.flags.iter().cloned());
    let entry_path = case.entry_path.to_string_lossy().to_string();

    let execution = catch_unwind(AssertUnwindSafe(|| {
        build_project(&builder, &entry_path, &flags)
    }));

    match execution {
        Ok(build_result) => match &case.expected {
            ExpectedOutcome::Success(expectation) => match build_result {
                Ok(build_result) => validate_success_result(case, build_result, expectation),
                Err(messages) => CaseExecutionResult {
                    passed: false,
                    panic_message: None,
                    build_result: None,
                    messages: Some(messages),
                    failure_reason: Some(
                        "Expected a successful build, but compilation failed.".to_string(),
                    ),
                },
            },
            ExpectedOutcome::Failure(expectation) => match build_result {
                Ok(build_result) => CaseExecutionResult {
                    passed: false,
                    panic_message: None,
                    build_result: Some(build_result),
                    messages: None,
                    failure_reason: Some(
                        "Expected a compilation failure, but the case built successfully."
                            .to_string(),
                    ),
                },
                Err(messages) => validate_failure_result(messages, expectation),
            },
        },
        Err(payload) => match &case.expected {
            ExpectedOutcome::Failure(expectation) if expectation.allow_panic => {
                CaseExecutionResult {
                    passed: true,
                    panic_message: Some(format_panic_payload(payload)),
                    build_result: None,
                    messages: None,
                    failure_reason: None,
                }
            }
            _ => CaseExecutionResult {
                passed: false,
                panic_message: Some(format_panic_payload(payload)),
                build_result: None,
                messages: None,
                failure_reason: Some("The compiler panicked while running this case.".to_string()),
            },
        },
    }
}

fn backend_builder_for_case(_backend_id: BackendId) -> ProjectBuilder {
    // This backend-builder seam is explicit even though both current backends
    // route through the HTML builder, so future non-HTML backends can slot in cleanly.
    ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()))
}

/// Declares whether a backend always has an implicit success contract.
///
/// WHAT: marks backends that always enforce baseline artifact checks.
/// WHY: keeps fixture validation permissive while still guaranteeing minimum output checks.
fn backend_has_builtin_success_contract(backend_id: BackendId) -> bool {
    matches!(backend_id, BackendId::Html | BackendId::HtmlWasm)
}

fn validate_success_result(
    case: &TestCaseSpec,
    build_result: BuildResult,
    expectation: &SuccessExpectation,
) -> CaseExecutionResult {
    if let Some(reason) =
        validate_warning_expectation(build_result.warnings.len(), expectation.warnings)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: Some(build_result),
            messages: None,
            failure_reason: Some(reason),
        };
    }

    if case.backend_id == BackendId::Html
        && let Some(reason) = validate_html_baseline_contract(&build_result)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: Some(build_result),
            messages: None,
            failure_reason: Some(reason),
        };
    }

    if case.backend_id == BackendId::HtmlWasm
        && let Some(reason) = validate_html_wasm_baseline_contract(&build_result)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: Some(build_result),
            messages: None,
            failure_reason: Some(reason),
        };
    }

    if let Some(reason) =
        validate_artifact_assertions(&build_result, &expectation.artifact_assertions)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: Some(build_result),
            messages: None,
            failure_reason: Some(reason),
        };
    }

    if let Some(reason) = validate_golden_outputs(&build_result, &case.golden_dir) {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: Some(build_result),
            messages: None,
            failure_reason: Some(reason),
        };
    }

    CaseExecutionResult {
        passed: true,
        panic_message: None,
        build_result: Some(build_result),
        messages: None,
        failure_reason: None,
    }
}

fn validate_failure_result(
    messages: CompilerMessages,
    expectation: &FailureExpectation,
) -> CaseExecutionResult {
    if let Some(reason) =
        validate_warning_expectation(messages.warnings.len(), expectation.warnings)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(reason),
        };
    }

    if !messages
        .errors
        .iter()
        .any(|error| error.error_type == expectation.error_type)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(format!(
                "Expected error type '{}', but it was not reported.",
                error_type_to_str(&expectation.error_type)
            )),
        };
    }

    if !expectation.message_contains.is_empty()
        && !messages
            .errors
            .iter()
            .any(|error| contains_ordered_substrings(&error.msg, &expectation.message_contains))
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(
                "Expected ordered diagnostic message fragments were not found in any emitted error."
                    .to_string(),
            ),
        };
    }

    CaseExecutionResult {
        passed: true,
        panic_message: None,
        build_result: None,
        messages: Some(messages),
        failure_reason: None,
    }
}

fn validate_warning_expectation(
    actual_count: usize,
    expectation: WarningExpectation,
) -> Option<String> {
    match expectation {
        WarningExpectation::Ignore => None,
        WarningExpectation::Forbid if actual_count > 0 => {
            Some(format!("Expected no warnings, but found {actual_count}."))
        }
        WarningExpectation::Forbid => None,
        WarningExpectation::Exact(expected) if actual_count != expected => Some(format!(
            "Expected exactly {expected} warnings, but found {actual_count}."
        )),
        WarningExpectation::Exact(_) => None,
    }
}

fn validate_expected_artifact_paths(
    build_result: &BuildResult,
    expected_paths: &[String],
) -> Option<String> {
    let actual_paths = collect_built_artifact_paths(build_result);

    let mut expected = expected_paths
        .iter()
        .map(|path| normalize_relative_path_text(path))
        .collect::<Vec<_>>();
    expected.sort();

    if actual_paths != expected {
        return Some(format!(
            "Expected output paths {:?}, but produced {:?}.",
            expected, actual_paths
        ));
    }

    None
}

fn collect_built_artifact_paths(build_result: &BuildResult) -> Vec<String> {
    let mut actual_paths = build_result
        .project
        .output_files
        .iter()
        .filter(|output| !matches!(output.file_kind(), FileKind::NotBuilt))
        .map(|output| normalize_relative_path(output.relative_output_path()))
        .collect::<Vec<_>>();
    actual_paths.sort();
    actual_paths
}

fn validate_artifact_assertions(
    build_result: &BuildResult,
    assertions: &[ArtifactAssertion],
) -> Option<String> {
    for assertion in assertions {
        let Some(output) = find_output_file(build_result, &assertion.path) else {
            return Some(format!(
                "Artifact assertion expected output '{}', but produced paths were {:?}.",
                assertion.path,
                collect_built_artifact_paths(build_result)
            ));
        };

        if let Some(reason) = validate_single_artifact_assertion(output, assertion) {
            return Some(reason);
        }
    }

    None
}

fn validate_single_artifact_assertion(
    output: &OutputFile,
    assertion: &ArtifactAssertion,
) -> Option<String> {
    match assertion.kind {
        ArtifactKind::Html | ArtifactKind::Js => {
            let Some(text) = output_text_content(output, assertion.kind) else {
                return Some(format!(
                    "Artifact '{}' expected kind '{}', but produced a different file kind.",
                    assertion.path,
                    artifact_kind_name(assertion.kind)
                ));
            };

            for required in &assertion.must_contain {
                if !text.contains(required) {
                    return Some(format!(
                        "Artifact '{}' did not contain required fragment '{}'.",
                        assertion.path, required
                    ));
                }
            }

            for forbidden in &assertion.must_not_contain {
                if text.contains(forbidden) {
                    return Some(format!(
                        "Artifact '{}' contained forbidden fragment '{}'.",
                        assertion.path, forbidden
                    ));
                }
            }
        }
        ArtifactKind::Wasm => {
            let Some(bytes) = output_wasm_bytes(output) else {
                return Some(format!(
                    "Artifact '{}' expected kind 'wasm', but produced a different file kind.",
                    assertion.path
                ));
            };

            if assertion.validate_wasm
                && let Err(error) = validate_wasm_bytes(bytes)
            {
                return Some(format!(
                    "Artifact '{}' failed wasm validation: {error}",
                    assertion.path
                ));
            }

            if !assertion.must_export.is_empty() {
                let exports = match collect_wasm_exports(bytes) {
                    Ok(exports) => exports,
                    Err(error) => {
                        return Some(format!(
                            "Artifact '{}' failed while reading wasm exports: {error}",
                            assertion.path
                        ));
                    }
                };

                for required_export in &assertion.must_export {
                    if !exports.contains(required_export) {
                        return Some(format!(
                            "Artifact '{}' missing required wasm export '{}'. Available exports: {:?}.",
                            assertion.path, required_export, exports
                        ));
                    }
                }
            }

            if !assertion.must_import.is_empty() {
                let imports = match collect_wasm_imports(bytes) {
                    Ok(imports) => imports,
                    Err(error) => {
                        return Some(format!(
                            "Artifact '{}' failed while reading wasm imports: {error}",
                            assertion.path
                        ));
                    }
                };

                for required_import in &assertion.must_import {
                    if !imports.contains(required_import) {
                        return Some(format!(
                            "Artifact '{}' missing required wasm import '{}'. Available imports: {:?}.",
                            assertion.path, required_import, imports
                        ));
                    }
                }
            }
        }
    }

    None
}

fn artifact_kind_name(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Html => "html",
        ArtifactKind::Js => "js",
        ArtifactKind::Wasm => "wasm",
    }
}

/// Verifies the baseline HTML backend interop/output contract.
///
/// WHAT: requires a built `index.html` HTML artifact for every html backend success case.
/// WHY: replacing legacy path assertions still needs a deterministic minimum output guarantee.
fn validate_html_baseline_contract(build_result: &BuildResult) -> Option<String> {
    let index_html = match find_output_file(build_result, "index.html") {
        Some(output) => output,
        None => {
            return Some(
                "html baseline contract expected 'index.html', but it was not produced."
                    .to_string(),
            );
        }
    };

    let Some(html) = output_text_content(index_html, ArtifactKind::Html) else {
        return Some(
            "html baseline contract expected 'index.html' as an HTML artifact.".to_string(),
        );
    };

    for required_fragment in [
        "<!DOCTYPE html>",
        "<html",
        "<head>",
        "<body",
        "</body>",
        "</html>",
    ] {
        if !html.contains(required_fragment) {
            return Some(format!(
                "html baseline contract expected 'index.html' to contain '{}'.",
                required_fragment
            ));
        }
    }

    None
}

fn validate_html_wasm_baseline_contract(build_result: &BuildResult) -> Option<String> {
    let index_html = match find_output_file(build_result, "index.html") {
        Some(output) => output,
        None => {
            return Some(
                "html_wasm baseline contract expected 'index.html', but it was not produced."
                    .to_string(),
            );
        }
    };

    let Some(html) = output_text_content(index_html, ArtifactKind::Html) else {
        return Some(
            "html_wasm baseline contract expected 'index.html' as an HTML artifact.".to_string(),
        );
    };
    for required_fragment in [
        "<!DOCTYPE html>",
        "<html",
        "<head>",
        "<body",
        "</body>",
        "</html>",
    ] {
        if !html.contains(required_fragment) {
            return Some(format!(
                "html_wasm baseline contract expected 'index.html' to contain '{}'.",
                required_fragment
            ));
        }
    }
    if !html.contains("<script src=\"./page.js\"></script>") {
        return Some(
            "html_wasm baseline contract expected 'index.html' to include './page.js'.".to_string(),
        );
    }
    let Some(script_pos) = html.find("<script src=\"./page.js\"></script>") else {
        return Some(
            "html_wasm baseline contract expected 'index.html' to include './page.js'.".to_string(),
        );
    };
    let Some(body_close) = html.find("</body>") else {
        return Some(
            "html_wasm baseline contract expected 'index.html' to contain '</body>'.".to_string(),
        );
    };
    if script_pos > body_close {
        return Some(
            "html_wasm baseline contract expected './page.js' to appear before '</body>'."
                .to_string(),
        );
    }

    let page_js = match find_output_file(build_result, "page.js") {
        Some(output) => output,
        None => {
            return Some(
                "html_wasm baseline contract expected 'page.js', but it was not produced."
                    .to_string(),
            );
        }
    };

    let Some(js) = output_text_content(page_js, ArtifactKind::Js) else {
        return Some(
            "html_wasm baseline contract expected 'page.js' as a JS artifact.".to_string(),
        );
    };

    for required_fragment in [
        "__bst_instantiate_wasm",
        "__bst_install_wasm_wrappers",
        "\"./page.wasm\"",
    ] {
        if !js.contains(required_fragment) {
            return Some(format!(
                "html_wasm baseline contract expected 'page.js' to contain '{}'.",
                required_fragment
            ));
        }
    }

    let page_wasm = match find_output_file(build_result, "page.wasm") {
        Some(output) => output,
        None => {
            return Some(
                "html_wasm baseline contract expected 'page.wasm', but it was not produced."
                    .to_string(),
            );
        }
    };

    let Some(wasm_bytes) = output_wasm_bytes(page_wasm) else {
        return Some(
            "html_wasm baseline contract expected 'page.wasm' as a wasm artifact.".to_string(),
        );
    };

    if let Err(error) = validate_wasm_bytes(wasm_bytes) {
        return Some(format!(
            "html_wasm baseline contract expected valid wasm bytes: {error}"
        ));
    }

    let exports = match collect_wasm_exports(wasm_bytes) {
        Ok(exports) => exports,
        Err(error) => {
            return Some(format!(
                "html_wasm baseline contract failed while reading wasm exports: {error}"
            ));
        }
    };

    for required_export in ["memory", "bst_str_ptr", "bst_str_len", "bst_release"] {
        if !exports.contains(&required_export.to_string()) {
            return Some(format!(
                "html_wasm baseline contract missing required export '{}'. Available exports: {:?}.",
                required_export, exports
            ));
        }
    }

    None
}

fn validate_wasm_bytes(bytes: &[u8]) -> Result<(), String> {
    wasmparser::Validator::new()
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn collect_wasm_exports(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut exports = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| error.to_string())?;
        if let Payload::ExportSection(reader) = payload {
            for export in reader {
                let export = export.map_err(|error| error.to_string())?;
                exports.push(export.name.to_string());
            }
        }
    }

    Ok(exports)
}

fn collect_wasm_imports(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut imports = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| error.to_string())?;
        if let Payload::ImportSection(reader) = payload {
            for import in reader {
                let import = import.map_err(|error| error.to_string())?;
                imports.push(format!("{}.{}", import.module, import.name));
            }
        }
    }

    Ok(imports)
}

fn find_output_file<'a>(
    build_result: &'a BuildResult,
    relative_path: &str,
) -> Option<&'a OutputFile> {
    let normalized_target = normalize_relative_path_text(relative_path);

    build_result.project.output_files.iter().find(|output| {
        !matches!(output.file_kind(), FileKind::NotBuilt)
            && normalize_relative_path(output.relative_output_path()) == normalized_target
    })
}

fn output_text_content(output: &OutputFile, expected_kind: ArtifactKind) -> Option<&str> {
    match (expected_kind, output.file_kind()) {
        (ArtifactKind::Html, FileKind::Html(content)) => Some(content.as_str()),
        (ArtifactKind::Js, FileKind::Js(content)) => Some(content.as_str()),
        _ => None,
    }
}

fn output_wasm_bytes(output: &OutputFile) -> Option<&[u8]> {
    match output.file_kind() {
        FileKind::Wasm(bytes) => Some(bytes.as_slice()),
        _ => None,
    }
}

fn normalize_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn normalize_relative_path_text(path: &str) -> String {
    path.replace('\\', "/")
}

fn validate_golden_outputs(build_result: &BuildResult, golden_dir: &Path) -> Option<String> {
    if !golden_dir.is_dir() {
        return None;
    }

    let mut expected_files = collect_files_recursive(golden_dir);
    expected_files.sort();

    let mut expected_paths = Vec::with_capacity(expected_files.len());
    for file in &expected_files {
        let relative = file
            .strip_prefix(golden_dir)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        expected_paths.push(relative);
    }

    if let Some(reason) = validate_expected_artifact_paths(build_result, &expected_paths) {
        return Some(reason);
    }

    for file in expected_files {
        let relative = file
            .strip_prefix(golden_dir)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");

        let Some(output) = find_output_file(build_result, &relative) else {
            return Some(format!("Golden output '{relative}' was not produced."));
        };

        let expected_bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Some(format!(
                    "Failed to read golden output '{}': {error}",
                    file.display()
                ));
            }
        };

        let actual_bytes = match output.file_kind() {
            FileKind::Html(content) | FileKind::Js(content) => content.as_bytes().to_vec(),
            FileKind::Wasm(bytes) => bytes.clone(),
            FileKind::Directory => Vec::new(),
            FileKind::NotBuilt => Vec::new(),
        };

        if actual_bytes != expected_bytes {
            return Some(format!(
                "Golden output '{relative}' did not match the produced artifact."
            ));
        }
    }

    None
}

fn render_case_result(case: &TestCaseSpec, result: &CaseExecutionResult, show_warnings: bool) {
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

            print_formatted_error(error.to_owned());
        }

        if show_warnings {
            for warning in &messages.warnings {
                print_formatted_warning(warning.to_owned());
            }
        }
    } else if let Some(build_result) = &result.build_result
        && show_warnings
    {
        for warning in &build_result.warnings {
            print_formatted_warning(warning.to_owned());
        }
    }
}

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

fn format_panic_payload(payload: Box<dyn Any + Send>) -> String {
    match payload.downcast::<String>() {
        Ok(message) => *message,
        Err(payload) => match payload.downcast::<&'static str>() {
            Ok(message) => (*message).to_string(),
            Err(_) => "non-string panic payload".to_string(),
        },
    }
}

fn parse_error_type(value: &str) -> Result<ErrorType, String> {
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "syntax" => Ok(ErrorType::Syntax),
        "type" => Ok(ErrorType::Type),
        "rule" => Ok(ErrorType::Rule),
        "file" => Ok(ErrorType::File),
        "config" => Ok(ErrorType::Config),
        "compiler" => Ok(ErrorType::Compiler),
        "devserver" | "dev_server" => Ok(ErrorType::DevServer),
        "borrowchecker" | "borrow_checker" => Ok(ErrorType::BorrowChecker),
        "hirtransformation" | "hir_transformation" => Ok(ErrorType::HirTransformation),
        "lirtransformation" | "lir_transformation" => Ok(ErrorType::LirTransformation),
        "wasmgeneration" | "wasm_generation" => Ok(ErrorType::WasmGeneration),
        other => Err(format!("Unsupported error type '{other}'")),
    }
}

fn collect_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut discovered = Vec::new();
    let mut queue = vec![root.to_path_buf()];

    while let Some(directory) = queue.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
                continue;
            }

            if path.is_file() {
                discovered.push(path);
            }
        }
    }

    discovered
}

/// Resolves backend-scoped golden directories for fixture assertions.
///
/// WHAT: maps each backend execution to `golden/<backend>/...`.
/// WHY: keeps artifact snapshots backend-specific even for non-matrix fixtures.
fn golden_dir_for_backend(fixture_root: &Path, backend_id: BackendId) -> PathBuf {
    fixture_root.join(GOLDEN_DIR_NAME).join(backend_id.as_str())
}

fn supported_backend_ids_text() -> String {
    SUPPORTED_BACKEND_IDS.join(", ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::SystemTime;

    fn temp_dir(prefix: &str) -> PathBuf {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("time should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("beanstalk_integration_runner_{prefix}_{unique}"))
    }

    fn write_success_fixture(root: &Path, case_name: &str) {
        let case_root = root.join(case_name);
        let input_root = case_root.join(INPUT_DIR_NAME);
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "mode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write expect file");
    }

    #[test]
    fn rejects_failure_fixture_without_message_contains() {
        let root = temp_dir("failure_contract_missing_message");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::write(input_root.join("#page.bst"), "x = 1\n").expect("should write fixture source");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "mode = \"failure\"\nwarnings = \"forbid\"\nerror_type = \"rule\"\n",
        )
        .expect("should write expect file");

        let error = match load_canonical_case_specs(&case_root, None, None) {
            Ok(_) => panic!("fixture should be rejected"),
            Err(error) => error,
        };
        assert!(
            error.contains("message_contains"),
            "unexpected error: {error}"
        );

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn accepts_success_fixture_without_explicit_artifact_assertions() {
        let root = temp_dir("success_contract_backend_baseline");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "mode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write expect file");

        let cases =
            load_canonical_case_specs(&case_root, None, None).expect("fixture should be accepted");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].display_name, "case [html]");

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn accepts_success_fixture_with_golden_only_assertion() {
        let root = temp_dir("success_contract_golden_assertion");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        let golden_root = case_root.join(GOLDEN_DIR_NAME).join("html");
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::create_dir_all(&golden_root).expect("should create fixture golden directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(golden_root.join("index.html"), "<h1>ok</h1>\n")
            .expect("should write golden file");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "mode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write expect file");

        let cases =
            load_canonical_case_specs(&case_root, None, None).expect("fixture should be accepted");
        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].display_name, "case [html]");

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn manifest_order_is_preserved_before_discovery_fallback() {
        let root = temp_dir("manifest_order");
        fs::create_dir_all(&root).expect("should create root");

        write_success_fixture(&root, "case_a");
        write_success_fixture(&root, "case_b");
        write_success_fixture(&root, "case_c");

        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "[[case]]\nid = \"case_b\"\npath = \"case_b\"\n\n[[case]]\nid = \"case_a\"\npath = \"case_a\"\n",
        )
        .expect("should write manifest");

        let suite = load_test_suite_from_root(&root).expect("suite should load");
        let names = suite
            .cases
            .iter()
            .map(|case| case.display_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(
            names,
            vec!["case_b [html]", "case_a [html]", "case_c [html]"]
        );

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn accepts_backend_matrix_and_expands_case_variants() {
        let root = temp_dir("backend_matrix");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "entry = \".\"\n\n[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n\n[backends.html_wasm]\nmode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write matrix expect file");

        let cases =
            load_canonical_case_specs(&case_root, None, None).expect("matrix case should parse");
        let names = cases
            .iter()
            .map(|case| case.display_name.as_str())
            .collect::<Vec<_>>();
        assert_eq!(names, vec!["case [html]", "case [html_wasm]"]);

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn rejects_unknown_backend_matrix_key() {
        let root = temp_dir("backend_matrix_unknown");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "entry = \".\"\n\n[backends.wasm]\nmode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write matrix expect file");

        let error = match load_canonical_case_specs(&case_root, None, None) {
            Ok(_) => panic!("fixture should be rejected"),
            Err(error) => error,
        };
        assert!(
            error.contains("Unsupported backend 'wasm'"),
            "unexpected error: {error}"
        );

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn backend_filter_limits_loaded_case_variants() {
        let root = temp_dir("backend_filter");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "entry = \".\"\n\n[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n\n[backends.html_wasm]\nmode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write matrix expect file");

        fs::write(
            root.join(MANIFEST_FILE_NAME),
            "[[case]]\nid = \"case\"\npath = \"case\"\n",
        )
        .expect("should write manifest");

        let suite = load_test_suite_from_root_with_filter(&root, Some(BackendId::HtmlWasm))
            .expect("suite should load");
        assert_eq!(suite.cases.len(), 1);
        assert_eq!(suite.cases[0].display_name, "case [html_wasm]");

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }

    #[test]
    fn matrix_cases_resolve_backend_specific_golden_directories() {
        let root = temp_dir("matrix_backend_goldens");
        let case_root = root.join("case");
        let input_root = case_root.join(INPUT_DIR_NAME);
        let golden_html_root = case_root.join(GOLDEN_DIR_NAME).join("html");
        let golden_wasm_root = case_root.join(GOLDEN_DIR_NAME).join("html_wasm");

        fs::create_dir_all(&input_root).expect("should create fixture input directory");
        fs::create_dir_all(&golden_html_root).expect("should create html golden directory");
        fs::create_dir_all(&golden_wasm_root).expect("should create wasm golden directory");
        fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
        fs::write(golden_html_root.join("index.html"), "<h1>html</h1>\n")
            .expect("should write html golden");
        fs::write(golden_wasm_root.join("index.html"), "<h1>wasm</h1>\n")
            .expect("should write wasm golden");
        fs::write(
            case_root.join(EXPECT_FILE_NAME),
            "entry = \".\"\n\n[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n\n[backends.html_wasm]\nmode = \"success\"\nwarnings = \"forbid\"\n",
        )
        .expect("should write matrix expect file");

        let cases = load_canonical_case_specs(&case_root, None, None).expect("cases should parse");
        assert_eq!(cases.len(), 2);

        let html_case = cases
            .iter()
            .find(|case| case.backend_id == BackendId::Html)
            .expect("html case should exist");
        let wasm_case = cases
            .iter()
            .find(|case| case.backend_id == BackendId::HtmlWasm)
            .expect("html_wasm case should exist");

        assert_eq!(html_case.golden_dir, golden_html_root);
        assert_eq!(wasm_case.golden_dir, golden_wasm_root);

        fs::remove_dir_all(&root).expect("should clean up temp fixture root");
    }
}
