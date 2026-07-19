//! Terminal output and triage report writing for the integration test suite.
//!
//! WHAT: renders case results, writes machine-readable triage/inventory reports, and owns their
//!       stable output shapes.
//! WHY: keeping reporting here means the runner only coordinates loading, selection and execution.

use super::{
    BackendId, CaseExecutionResult, CaseRole, ExpectedOutcome, FailureExpectation, FailureKind,
    FailureTriageEntry, FailureTriageReport, SEPARATOR_LINE_LENGTH, SuccessExpectation,
    SummaryCounts, TestCaseSpec, WarningExpectation,
};
use crate::compiler_frontend::compiler_messages::render::{terminal, terse};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticCategory, DiagnosticSeverity,
};
use saying::say;
use serde::Serialize;
use std::collections::BTreeMap;
use std::fmt::Write;
use std::fs;
use std::path::Path;
use std::process::Command;

const SUITE_INVENTORY_SCHEMA_VERSION: u32 = 1;

pub(crate) fn format_case_listing(cases: &[TestCaseSpec]) -> String {
    if cases.is_empty() {
        return String::from("No test cases matched the selection filters.\n");
    }

    let mut listing = String::new();
    let mut index = 0;
    while index < cases.len() {
        let case = &cases[index];
        let case_id = &case.case_id;
        let _ = writeln!(listing, "case_id: {case_id}");
        let _ = writeln!(listing, "  backends:");

        while index < cases.len() && cases[index].case_id == *case_id {
            let backend_case = &cases[index];
            let _ = writeln!(
                listing,
                "    - {} ({})",
                backend_case.backend_id.as_str(),
                expected_outcome_label(&backend_case.expected)
            );
            index += 1;
        }

        let _ = writeln!(
            listing,
            "  tags: {}",
            if case.tags.is_empty() {
                "<none>".to_string()
            } else {
                case.tags.join(", ")
            }
        );
        let _ = writeln!(
            listing,
            "  contract: {}",
            case.contract.as_deref().unwrap_or("<none>")
        );
        let _ = writeln!(
            listing,
            "  role: {}\n",
            case.role.map_or("<none>", |role| role.as_str())
        );
    }

    listing
}

/// Stable machine-readable inventory for the canonical integration suite.
///
/// WHAT: records manifest metadata and the current typed expectation facts without executing a
///       case.
/// WHY: audit output is a review input for later policy phases, not a second test runner.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct SuiteInventoryReport {
    pub schema_version: u32,
    pub repository_commit: Option<String>,
    pub manifest_case_count: usize,
    pub expanded_backend_execution_count: usize,
    pub cases: Vec<InventoryCase>,
    pub hard_policy_violations: Vec<AuditFinding>,
    pub advisory_findings: Vec<AuditFinding>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct InventoryCase {
    pub canonical_id: String,
    pub manifest_relative_path: String,
    pub tags: Vec<String>,
    pub contract: Option<String>,
    pub role: Option<CaseRole>,
    pub backends: Vec<InventoryBackend>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct InventoryBackend {
    pub backend: String,
    pub mode: &'static str,
    pub compile_only: bool,
    pub warning_mode: &'static str,
    pub diagnostic_match: Option<&'static str>,
    pub structured_diagnostic_assertions: bool,
    pub assertion_kinds: Vec<&'static str>,
    pub golden_mode: Option<&'static str>,
    pub golden_present: bool,
    pub artifact_assertion_count: usize,
    pub rendered_output_assertion_count: usize,
    pub artifact_absence_assertion_count: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct AuditFinding {
    pub code: String,
    pub case_id: Option<String>,
    pub message: String,
}

pub(crate) fn build_suite_inventory_report(
    cases: &[TestCaseSpec],
    repository_commit: Option<String>,
) -> SuiteInventoryReport {
    let mut inventory_cases = Vec::<InventoryCase>::new();

    for case in cases {
        if let Some(inventory_case) = inventory_cases.last_mut()
            && inventory_case.canonical_id == case.case_id
        {
            inventory_case.backends.push(build_backend_inventory(case));
            continue;
        }

        inventory_cases.push(InventoryCase {
            canonical_id: case.case_id.clone(),
            manifest_relative_path: case.manifest_relative_path.clone(),
            tags: case.tags.clone(),
            contract: case.contract.clone(),
            role: case.role,
            backends: vec![build_backend_inventory(case)],
        });
    }

    let mut hard_policy_violations = Vec::new();
    let mut advisory_findings = Vec::new();
    let mut primary_contracts = BTreeMap::<String, String>::new();

    for inventory_case in &inventory_cases {
        if inventory_case.contract.is_none() {
            advisory_findings.push(AuditFinding {
                code: "missing_contract_classification".to_owned(),
                case_id: Some(inventory_case.canonical_id.clone()),
                message: "Case has no manifest contract classification.".to_owned(),
            });
        }

        if inventory_case.role.is_none() {
            advisory_findings.push(AuditFinding {
                code: "missing_role_classification".to_owned(),
                case_id: Some(inventory_case.canonical_id.clone()),
                message: "Case has no manifest role classification.".to_owned(),
            });
        }

        if inventory_case.role == Some(CaseRole::Primary) {
            if let Some(contract) = inventory_case.contract.as_ref()
                && let Some(previous_case_id) =
                    primary_contracts.insert(contract.clone(), inventory_case.canonical_id.clone())
            {
                hard_policy_violations.push(AuditFinding {
                    code: "duplicate_primary_contract".to_owned(),
                    case_id: Some(inventory_case.canonical_id.clone()),
                    message: format!(
                        "Primary contract '{contract}' is also owned by case '{previous_case_id}'."
                    ),
                });
            } else if inventory_case.contract.is_none() {
                hard_policy_violations.push(AuditFinding {
                    code: "primary_missing_contract".to_owned(),
                    case_id: Some(inventory_case.canonical_id.clone()),
                    message: "Primary case has no manifest contract classification.".to_owned(),
                });
            }
        }
    }

    SuiteInventoryReport {
        schema_version: SUITE_INVENTORY_SCHEMA_VERSION,
        repository_commit,
        manifest_case_count: inventory_cases.len(),
        expanded_backend_execution_count: cases.len(),
        cases: inventory_cases,
        hard_policy_violations,
        advisory_findings,
    }
}

fn build_backend_inventory(case: &TestCaseSpec) -> InventoryBackend {
    match &case.expected {
        ExpectedOutcome::Success(expectation) => InventoryBackend {
            backend: case.backend_id.as_str().to_owned(),
            mode: "success",
            compile_only: false,
            warning_mode: warning_mode_label(expectation.warnings),
            diagnostic_match: None,
            structured_diagnostic_assertions: false,
            assertion_kinds: success_assertion_kinds(case, expectation),
            golden_mode: Some(golden_mode_label(expectation.golden_mode)),
            golden_present: expectation.has_golden,
            artifact_assertion_count: expectation.artifact_assertions.len(),
            rendered_output_assertion_count: expectation.rendered_output_contains.len()
                + expectation.rendered_output_not_contains.len(),
            artifact_absence_assertion_count: expectation.artifacts_must_not_exist.len(),
        },
        ExpectedOutcome::Failure(expectation) => InventoryBackend {
            backend: case.backend_id.as_str().to_owned(),
            mode: "failure",
            compile_only: false,
            warning_mode: warning_mode_label(expectation.warnings),
            diagnostic_match: Some("contains"),
            structured_diagnostic_assertions: false,
            assertion_kinds: failure_assertion_kinds(expectation),
            golden_mode: None,
            golden_present: false,
            artifact_assertion_count: 0,
            rendered_output_assertion_count: 0,
            artifact_absence_assertion_count: 0,
        },
    }
}

fn success_assertion_kinds(
    case: &TestCaseSpec,
    expectation: &SuccessExpectation,
) -> Vec<&'static str> {
    let mut kinds = Vec::new();

    if matches!(case.backend_id, BackendId::Html | BackendId::HtmlWasm) {
        kinds.push("backend_baseline");
    }

    if !expectation.artifact_assertions.is_empty() {
        kinds.push("artifact_assertions");
    }
    if expectation.has_golden {
        kinds.push("golden");
    }
    if !expectation.rendered_output_contains.is_empty()
        || !expectation.rendered_output_not_contains.is_empty()
    {
        kinds.push("rendered_output");
    }
    if !expectation.artifacts_must_not_exist.is_empty() {
        kinds.push("artifact_absence");
    }
    kinds
}

fn failure_assertion_kinds(expectation: &FailureExpectation) -> Vec<&'static str> {
    let mut kinds = Vec::new();
    if !expectation.diagnostic_codes.is_empty() {
        kinds.push("diagnostic_codes");
    }
    if !expectation.message_contains.is_empty() {
        kinds.push("message_contains");
    }
    kinds
}

fn warning_mode_label(expectation: WarningExpectation) -> &'static str {
    match expectation {
        WarningExpectation::Ignore => "ignore",
        WarningExpectation::Forbid => "forbid",
        WarningExpectation::Exact(_) => "exact",
    }
}

fn golden_mode_label(mode: super::GoldenMode) -> &'static str {
    match mode {
        super::GoldenMode::Strict => "strict",
        super::GoldenMode::Normalized => "normalized",
    }
}

/// Discovers the current repository revision without making audit depend on Git.
pub(crate) fn discover_repository_commit() -> Option<String> {
    let output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()?;

    if !output.status.success() {
        return None;
    }

    let commit = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (!commit.is_empty()).then_some(commit)
}

pub(crate) fn write_suite_inventory_report(
    report_path_str: &str,
    report: &SuiteInventoryReport,
) -> Result<(), String> {
    let report_path = Path::new(report_path_str);
    if let Some(parent) = report_path.parent() {
        fs::create_dir_all(parent).map_err(|error| {
            format!(
                "Failed to create suite inventory directory '{}': {error}",
                parent.display()
            )
        })?;
    }

    let report_json =
        serde_json::to_string_pretty(report).map_err(|error| format!("JSON error: {error}"))?;
    fs::write(report_path, report_json).map_err(|error| {
        format!(
            "Failed to write suite inventory report '{}': {error}",
            report_path.display()
        )
    })?;

    Ok(())
}

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

    if let Some(kind) = result.failure_kind {
        let label = match kind {
            FailureKind::StrictGoldenMismatch => "[strict golden mismatch]",
            FailureKind::NormalizedSemanticMismatch => "[normalized mismatch]",
            FailureKind::RenderedOutputMismatch => "[rendered output mismatch]",
            FailureKind::HarnessFailed => "[harness error]",
            FailureKind::ExpectationViolation => "[expectation violation]",
        };
        say!(Dark White label);
    }

    if let Some(reason) = &result.failure_reason {
        say!(Red reason);
    }

    if let Some(panic_message) = &result.panic_message {
        say!(Red format!("panic: {panic_message}"));
    }

    if let Some(messages) = &result.messages {
        for (diagnostic_index, diagnostic) in messages
            .diagnostics()
            .enumerate()
            .filter(|(_, diagnostic)| diagnostic.severity == DiagnosticSeverity::Error)
        {
            if result.passed && matches!(case.expected, ExpectedOutcome::Failure(_)) {
                say!(Yellow diagnostic_summary_label(diagnostic));
                continue;
            }

            terminal::print_diagnostic_with_context(
                diagnostic,
                messages.diagnostic_render_context(diagnostic_index),
            );
        }

        if show_warnings {
            for (diagnostic_index, warning) in messages
                .diagnostics()
                .enumerate()
                .filter(|(_, diagnostic)| diagnostic.severity == DiagnosticSeverity::Warning)
            {
                terminal::print_diagnostic_with_context(
                    warning,
                    messages.diagnostic_render_context(diagnostic_index),
                );
            }
        }
    } else if let Some(build_result) = &result.build_result
        && show_warnings
    {
        for warning in &build_result.warnings {
            crate::compiler_frontend::compiler_messages::render::terminal::print_diagnostic(
                warning,
                &build_result.string_table,
            );
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
        && let Some(first_diagnostic) = messages
            .diagnostics()
            .find(|diagnostic| diagnostic.severity == DiagnosticSeverity::Error)
    {
        let base = result
            .failure_reason
            .as_deref()
            .unwrap_or("Compilation failed.");
        let diagnostic_index = messages
            .diagnostic_slice()
            .iter()
            .position(|diagnostic| std::ptr::eq(diagnostic, first_diagnostic))
            .unwrap_or(0);
        let terse_line = terse::format_terse_diagnostic_with_context(
            first_diagnostic,
            messages.diagnostic_render_context(diagnostic_index),
        );

        return format!("{base} First diagnostic: {terse_line}");
    }

    if let Some(reason) = &result.failure_reason {
        return reason.to_owned();
    }

    if let Some(panic_message) = &result.panic_message {
        return format!("Compiler panic: {panic_message}");
    }

    "No failure reason was recorded.".to_string()
}

fn diagnostic_summary_label(diagnostic: &CompilerDiagnostic) -> String {
    let descriptor = diagnostic.kind.descriptor();
    let category = match diagnostic.kind.category() {
        DiagnosticCategory::Syntax => "Syntax Error",
        DiagnosticCategory::Type => "Type Error",
        DiagnosticCategory::Rule
        | DiagnosticCategory::Import
        | DiagnosticCategory::DeferredFeature => "Language Rule Error",
        DiagnosticCategory::Borrow => "Borrow Checker Violation",
        DiagnosticCategory::Config => "Malformed Config",
        DiagnosticCategory::Infrastructure => "Infrastructure Failure",
    };

    format!("{category} [{}]", descriptor.code)
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
