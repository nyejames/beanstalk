//! Integration test runner for end-to-end Beanstalk compiler coverage.
//!
//! Supports:
//! - canonical self-contained case folders under `tests/cases/<case>/`
//! - optional manifest-driven case ordering

use crate::build_system::build::{BuildResult, FileKind, build_project};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerMessages, ErrorType, error_type_to_str,
};
use crate::compiler_frontend::compiler_messages::compiler_warnings::print_formatted_warning;
use crate::compiler_frontend::display_messages::print_formatted_error;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use saying::say;
use std::any::Any;
use std::collections::HashSet;
use std::fs;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

const CANONICAL_TESTS_PATH: &str = "tests/cases";
const MANIFEST_FILE_NAME: &str = "manifest.toml";
const EXPECT_FILE_NAME: &str = "expect.toml";
const INPUT_DIR_NAME: &str = "input";
const GOLDEN_DIR_NAME: &str = "golden";
const SEPARATOR_LINE_LENGTH: usize = 37;

#[derive(Clone, Copy)]
struct TestRunnerOptions {
    show_warnings: bool,
}

struct TestSuiteSpec {
    cases: Vec<TestCaseSpec>,
}

#[derive(Clone)]
struct TestCaseSpec {
    display_name: String,
    entry_path: PathBuf,
    fixture_root: PathBuf,
    expected: ExpectedOutcome,
}

#[derive(Clone)]
enum ExpectedOutcome {
    Success(SuccessExpectation),
    Failure(FailureExpectation),
}

#[derive(Clone)]
struct SuccessExpectation {
    warnings: WarningExpectation,
    output_paths: Option<Vec<String>>,
}

#[derive(Clone)]
struct FailureExpectation {
    allow_panic: bool,
    warnings: WarningExpectation,
    error_type: Option<ErrorType>,
    message_contains: Vec<String>,
}

#[derive(Clone, Copy)]
enum WarningExpectation {
    Ignore,
    Forbid,
    Exact(usize),
}

struct CaseExecutionResult {
    passed: bool,
    panic_message: Option<String>,
    build_result: Option<BuildResult>,
    messages: Option<CompilerMessages>,
    failure_reason: Option<String>,
}

struct ManifestCaseSpec {
    id: String,
    path: PathBuf,
}

struct ParsedExpectationFile {
    mode: String,
    entry: Option<String>,
    allow_panic: bool,
    warnings: WarningExpectation,
    error_type: Option<ErrorType>,
    message_contains: Vec<String>,
    output_paths: Option<Vec<String>>,
}

/// Runs all test cases from the `tests/cases` directory.
pub fn run_all_test_cases(show_warnings: bool) {
    println!("Running all Beanstalk test cases...\n");
    let timer = std::time::Instant::now();
    let options = TestRunnerOptions { show_warnings };

    let suite = match load_test_suite() {
        Ok(spec) => spec,
        Err(error) => {
            say!(Red "Failed to load integration test suite:");
            println!("  {error}");
            return;
        }
    };

    let mut total_tests = 0usize;
    let mut passed_tests = 0usize;
    let mut failed_tests = 0usize;
    let mut expected_failures = 0usize;
    let mut unexpected_successes = 0usize;

    if !suite.cases.is_empty() {
        say!(Cyan "Testing integration cases:");
        say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));

        for case in &suite.cases {
            total_tests += 1;
            println!("  {}", case.display_name);

            let result = execute_test_case(case);
            render_case_result(case, &result, options.show_warnings);

            if result.passed {
                match case.expected {
                    ExpectedOutcome::Success(_) => passed_tests += 1,
                    ExpectedOutcome::Failure(_) => expected_failures += 1,
                }
            } else {
                match case.expected {
                    ExpectedOutcome::Success(_) => failed_tests += 1,
                    ExpectedOutcome::Failure(_) => unexpected_successes += 1,
                }
            }

            say!(Dark White "-".repeat(SEPARATOR_LINE_LENGTH));
        }

        println!();
    }

    println!();
    say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));
    print!("Test Results Summary. Took: ");
    say!(Green #timer.elapsed());

    say!("\n  Total tests:             ", Yellow total_tests);
    say!("  Successful compilations: ", Blue passed_tests);
    say!("  Failed compilations:     ", Blue failed_tests);
    say!("  Expected failures:       ", Blue expected_failures);
    say!("  Unexpected successes:    ", Blue unexpected_successes);

    let correct_results = passed_tests + expected_failures;
    let incorrect_results = failed_tests + unexpected_successes;

    say!();
    say!("  Correct results:   ", Green Bold correct_results, Dark White " / ", total_tests);
    say!("  Incorrect results: ", Red Bold incorrect_results, Dark White " / ", total_tests);

    if incorrect_results == 0 {
        say!("\nAll tests behaved as expected.");
    } else if total_tests > 0 {
        let percentage = (correct_results as f64 / total_tests as f64) * 100.0;
        say!(
            Yellow "\n",
            Bright Yellow format!("{percentage:.1}"),
            " %",
            Reset " of tests behaved as expected"
        );
    }

    say!(Dark White "=".repeat(SEPARATOR_LINE_LENGTH));
}

fn load_test_suite() -> Result<TestSuiteSpec, String> {
    let root = Path::new(CANONICAL_TESTS_PATH);
    let mut cases = Vec::new();
    let mut loaded_canonical_paths = HashSet::new();

    let manifest_path = root.join(MANIFEST_FILE_NAME);
    if manifest_path.is_file() {
        for manifest_case in parse_manifest_file(&manifest_path)? {
            let fixture_root = root.join(&manifest_case.path);
            let case = load_canonical_case(&fixture_root, Some(manifest_case.id))?;
            loaded_canonical_paths.insert(fs::canonicalize(&fixture_root).unwrap_or(fixture_root));
            cases.push(case);
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

            cases.push(load_canonical_case(&fixture_root, None)?);
            loaded_canonical_paths.insert(canonical_path);
        }
    }

    cases.sort_by(|lhs, rhs| lhs.display_name.cmp(&rhs.display_name));

    Ok(TestSuiteSpec { cases })
}

fn parse_manifest_file(path: &Path) -> Result<Vec<ManifestCaseSpec>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read manifest '{}': {error}", path.display()))?;

    let mut cases = Vec::new();
    let mut current_id: Option<String> = None;
    let mut current_path: Option<PathBuf> = None;

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if line == "[[case]]" {
            if let (Some(id), Some(case_path)) = (current_id.take(), current_path.take()) {
                cases.push(ManifestCaseSpec {
                    id,
                    path: case_path,
                });
            }
            continue;
        }

        if let Some((key, value)) = parse_key_value_line(line) {
            match key {
                "id" => current_id = Some(parse_required_string(value)?),
                "path" => current_path = Some(PathBuf::from(parse_required_string(value)?)),
                _ => {}
            }
        }
    }

    if let (Some(id), Some(case_path)) = (current_id.take(), current_path.take()) {
        cases.push(ManifestCaseSpec {
            id,
            path: case_path,
        });
    }

    Ok(cases)
}

fn load_canonical_case(
    fixture_root: &Path,
    explicit_id: Option<String>,
) -> Result<TestCaseSpec, String> {
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
    let entry_path = resolve_case_entry_path(&input_root, parsed_expectation.entry.as_deref())?;
    let case_id = explicit_id.unwrap_or_else(|| {
        fixture_root
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("unnamed_case")
            .to_string()
    });

    let expected = match parsed_expectation.mode.as_str() {
        "success" => ExpectedOutcome::Success(SuccessExpectation {
            warnings: parsed_expectation.warnings,
            output_paths: parsed_expectation.output_paths,
        }),
        "failure" => ExpectedOutcome::Failure(FailureExpectation {
            allow_panic: parsed_expectation.allow_panic,
            warnings: parsed_expectation.warnings,
            error_type: parsed_expectation.error_type,
            message_contains: parsed_expectation.message_contains,
        }),
        other => {
            return Err(format!(
                "Canonical fixture '{}' has unsupported mode '{other}'",
                fixture_root.display()
            ));
        }
    };

    Ok(TestCaseSpec {
        display_name: case_id,
        entry_path,
        fixture_root: fixture_root.to_path_buf(),
        expected,
    })
}

fn parse_expectation_file(path: &Path) -> Result<ParsedExpectationFile, String> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "Failed to read expectation file '{}': {error}",
            path.display()
        )
    })?;

    let mut mode = None;
    let mut entry = None;
    let mut allow_panic = false;
    let mut warnings = WarningExpectation::Ignore;
    let mut warning_count = None;
    let mut error_type = None;
    let mut message_contains = Vec::new();
    let mut output_paths = None;

    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        let Some((key, value)) = parse_key_value_line(line) else {
            continue;
        };

        match key {
            "mode" => mode = Some(parse_required_string(value)?),
            "entry" => entry = Some(parse_required_string(value)?),
            "builder" => {
                let builder = parse_required_string(value)?;
                if builder != "html" {
                    return Err(format!(
                        "Expectation file '{}' only supports builder = \"html\" right now",
                        path.display()
                    ));
                }
            }
            "panic" => allow_panic = parse_bool_value(value)?,
            "warnings" => {
                warnings = match parse_required_string(value)?.as_str() {
                    "ignore" => WarningExpectation::Ignore,
                    "forbid" => WarningExpectation::Forbid,
                    "exact" => WarningExpectation::Exact(0),
                    other => {
                        return Err(format!(
                            "Expectation file '{}' has unsupported warnings mode '{other}'",
                            path.display()
                        ));
                    }
                };
            }
            "warning_count" => warning_count = Some(parse_usize_value(value)?),
            "error_type" => error_type = Some(parse_error_type(value)?),
            "message_contains" => message_contains = parse_string_array_value(value)?,
            "output_paths" => output_paths = Some(parse_string_array_value(value)?),
            _ => {}
        }
    }

    let mode = mode.ok_or_else(|| {
        format!(
            "Expectation file '{}' is missing required key 'mode'",
            path.display()
        )
    })?;

    if let WarningExpectation::Exact(_) = warnings {
        let expected_count = warning_count.ok_or_else(|| {
            format!(
                "Expectation file '{}' uses warnings = \"exact\" but is missing 'warning_count'",
                path.display()
            )
        })?;
        warnings = WarningExpectation::Exact(expected_count);
    }

    Ok(ParsedExpectationFile {
        mode,
        entry,
        allow_panic,
        warnings,
        error_type,
        message_contains,
        output_paths,
    })
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

    let preferred_entries = [
        PathBuf::from("main.bst"),
        PathBuf::from("entry.bst"),
        PathBuf::from("#page.bst"),
    ];

    for relative in preferred_entries {
        let candidate = input_root.join(&relative);
        if candidate.is_file() {
            return Ok(candidate);
        }
    }

    let root_entries = collect_root_hash_entries(input_root)?;
    if root_entries.len() == 1 {
        return Ok(root_entries
            .into_iter()
            .next()
            .unwrap_or_else(|| input_root.to_path_buf()));
    }

    Err(format!(
        "Could not determine canonical test entry for '{}'. Add 'entry = ...' to '{}' or provide main.bst / entry.bst / #page.bst.",
        input_root.display(),
        EXPECT_FILE_NAME
    ))
}

fn collect_root_hash_entries(input_root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut entries = Vec::new();
    let directory = fs::read_dir(input_root).map_err(|error| {
        format!(
            "Failed to scan canonical test input root '{}': {error}",
            input_root.display()
        )
    })?;

    for entry in directory {
        let entry =
            entry.map_err(|error| format!("Failed to read canonical test input entry: {error}"))?;
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if name.starts_with('#') && path.extension().is_some_and(|ext| ext == "bst") {
            entries.push(path);
        }
    }

    entries.sort();
    Ok(entries)
}

fn execute_test_case(case: &TestCaseSpec) -> CaseExecutionResult {
    let builder = HtmlProjectBuilder::new();
    let flags = vec![Flag::DisableTimers];
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

    if let Some(expected_paths) = &expectation.output_paths
        && let Some(reason) = validate_output_paths(&build_result, expected_paths)
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
        validate_golden_outputs(&build_result, &case.fixture_root.join(GOLDEN_DIR_NAME))
    {
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

    if let Some(expected_type) = &expectation.error_type
        && !messages
            .errors
            .iter()
            .any(|error| &error.error_type == expected_type)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(format!(
                "Expected error type '{}', but it was not reported.",
                error_type_to_str(expected_type)
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

fn validate_output_paths(build_result: &BuildResult, expected_paths: &[String]) -> Option<String> {
    let mut actual_paths = build_result
        .project
        .output_files
        .iter()
        .filter(|output| !matches!(output.file_kind(), FileKind::NotBuilt))
        .map(|output| {
            output
                .relative_output_path()
                .to_string_lossy()
                .replace('\\', "/")
        })
        .collect::<Vec<_>>();
    actual_paths.sort();

    let mut expected = expected_paths.to_vec();
    expected.sort();

    if actual_paths != expected {
        return Some(format!(
            "Expected output paths {:?}, but produced {:?}.",
            expected, actual_paths
        ));
    }

    None
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

    if let Some(reason) = validate_output_paths(build_result, &expected_paths) {
        return Some(reason);
    }

    for file in expected_files {
        let relative = file
            .strip_prefix(golden_dir)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");

        let Some(output) = build_result.project.output_files.iter().find(|output| {
            output
                .relative_output_path()
                .to_string_lossy()
                .replace('\\', "/")
                == relative
        }) else {
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

fn parse_key_value_line(line: &str) -> Option<(&str, &str)> {
    let (key, value) = line.split_once('=')?;
    Some((key.trim(), value.trim()))
}

fn parse_required_string(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    let Some(stripped) = trimmed
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
    else {
        return Err(format!("Expected a quoted string value, got '{trimmed}'"));
    };

    Ok(stripped.to_string())
}

fn parse_bool_value(value: &str) -> Result<bool, String> {
    match value.trim() {
        "true" => Ok(true),
        "false" => Ok(false),
        other => Err(format!("Expected a boolean value, got '{other}'")),
    }
}

fn parse_usize_value(value: &str) -> Result<usize, String> {
    value
        .trim()
        .parse::<usize>()
        .map_err(|error| format!("Expected an unsigned integer value: {error}"))
}

fn parse_string_array_value(value: &str) -> Result<Vec<String>, String> {
    let trimmed = value.trim();
    let Some(inner) = trimmed
        .strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
    else {
        return Err(format!("Expected a string array, got '{trimmed}'"));
    };

    let inner = inner.trim();
    if inner.is_empty() {
        return Ok(Vec::new());
    }

    inner
        .split(',')
        .map(|item| parse_required_string(item.trim()))
        .collect()
}

fn parse_error_type(value: &str) -> Result<ErrorType, String> {
    let normalized = parse_required_string(value)?.to_ascii_lowercase();
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
