//! Self-tests for cross-case integration-suite policy.
//!
//! WHAT: protects ownership, assertion-strength and classification evaluation.
//! WHY: these rules must be emitted once by the policy owner before reporting or execution.

use super::super::policy::evaluate_suite;
use super::super::types::GoldenExpectation;
use super::super::{
    BackendId, CaseRole, DiagnosticMatchMode, ExpectedOutcome, FailureExpectation, SuccessContract,
    SuccessExpectation, TestCaseSpec, TestSuiteSpec, WarningExpectation,
};
use std::path::PathBuf;

fn suite(cases: Vec<TestCaseSpec>) -> TestSuiteSpec {
    TestSuiteSpec { cases }
}

fn success_case(
    case_id: &str,
    backend_id: BackendId,
    contract: Option<&str>,
    role: Option<CaseRole>,
    success_contract: Option<SuccessContract>,
    rendered_output: Option<&str>,
) -> TestCaseSpec {
    TestCaseSpec {
        display_name: format!("{case_id} [{}]", backend_id.as_str()),
        case_id: case_id.to_owned(),
        manifest_relative_path: case_id.to_owned(),
        tags: vec!["integration".to_owned()],
        contract: contract.map(str::to_owned),
        role,
        backend_id,
        entry_path: PathBuf::from("input/#page.bst"),
        flags: Vec::new(),
        expected: ExpectedOutcome::Success(SuccessExpectation {
            warnings: WarningExpectation::Forbid,
            success_contract,
            artifact_assertions: Vec::new(),
            golden: GoldenExpectation::default(),
            rendered_output_contains: rendered_output.map(str::to_owned).into_iter().collect(),
            rendered_output_not_contains: Vec::new(),
            artifacts_must_not_exist: Vec::new(),
        }),
    }
}

fn failure_case(
    case_id: &str,
    backend_id: BackendId,
    contract: Option<&str>,
    role: Option<CaseRole>,
    diagnostic_match: DiagnosticMatchMode,
    diagnostic_match_reason: Option<&str>,
) -> TestCaseSpec {
    TestCaseSpec {
        display_name: format!("{case_id} [{}]", backend_id.as_str()),
        case_id: case_id.to_owned(),
        manifest_relative_path: case_id.to_owned(),
        tags: vec!["integration".to_owned()],
        contract: contract.map(str::to_owned),
        role,
        backend_id,
        entry_path: PathBuf::from("input/#page.bst"),
        flags: Vec::new(),
        expected: ExpectedOutcome::Failure(FailureExpectation {
            warnings: WarningExpectation::Forbid,
            message_contains: Vec::new(),
            diagnostic_codes: vec!["BST-RULE-0001".to_owned()],
            diagnostic_match,
            diagnostic_match_reason: diagnostic_match_reason.map(str::to_owned),
        }),
    }
}

#[test]
fn duplicate_primary_contract_is_emitted_once_by_policy() {
    let suite = suite(vec![
        success_case(
            "case_a",
            BackendId::Html,
            Some("language.shared"),
            Some(CaseRole::Primary),
            None,
            Some("case-a"),
        ),
        success_case(
            "case_b",
            BackendId::Html,
            Some("language.shared"),
            Some(CaseRole::Primary),
            None,
            Some("case-b"),
        ),
    ]);

    let evaluation = evaluate_suite(&suite);

    assert_eq!(evaluation.hard_findings.len(), 1);
    assert_eq!(
        evaluation.hard_findings[0].code,
        "duplicate_primary_contract"
    );
}

#[test]
fn primary_without_contract_is_emitted_once_by_policy() {
    let suite = suite(vec![success_case(
        "primary_without_contract",
        BackendId::Html,
        None,
        Some(CaseRole::Primary),
        None,
        Some("primary"),
    )]);

    let evaluation = evaluate_suite(&suite);

    assert_eq!(evaluation.hard_findings.len(), 1);
    assert_eq!(evaluation.hard_findings[0].code, "primary_missing_contract");
    assert!(evaluation.advisories.is_empty());
}

#[test]
fn whole_case_acceptance_only_requires_smoke_role() {
    let suite = suite(vec![success_case(
        "acceptance_only",
        BackendId::Html,
        None,
        Some(CaseRole::Backend),
        Some(SuccessContract::AcceptanceOnly),
        None,
    )]);

    let evaluation = evaluate_suite(&suite);

    assert_eq!(evaluation.hard_findings.len(), 1);
    assert_eq!(
        evaluation.hard_findings[0].code,
        "acceptance_only_requires_smoke_role"
    );
}

#[test]
fn missing_contains_reason_is_a_hard_policy_finding() {
    let evaluation = evaluate_suite(&suite(vec![failure_case(
        "contains_missing_reason",
        BackendId::Html,
        Some("diagnostics.contains_reason"),
        Some(CaseRole::Boundary),
        DiagnosticMatchMode::Contains,
        None,
    )]));

    assert_eq!(evaluation.hard_findings.len(), 1);
    assert_eq!(
        evaluation.hard_findings[0].code,
        "diagnostic_contains_requires_reason"
    );
    assert!(
        evaluation.hard_findings[0]
            .message
            .contains("contains_missing_reason")
    );
    assert!(
        evaluation.hard_findings[0]
            .message
            .contains("backend 'html'")
    );
}

#[test]
fn blank_contains_reason_is_a_hard_policy_finding() {
    let evaluation = evaluate_suite(&suite(vec![failure_case(
        "contains_blank_reason",
        BackendId::Html,
        Some("diagnostics.contains_reason"),
        Some(CaseRole::Boundary),
        DiagnosticMatchMode::Contains,
        Some(" \t"),
    )]));

    assert_eq!(evaluation.hard_findings.len(), 1);
    assert_eq!(
        evaluation.hard_findings[0].code,
        "diagnostic_contains_requires_reason"
    );
}

#[test]
fn justified_contains_reason_has_no_hard_policy_finding() {
    let evaluation = evaluate_suite(&suite(vec![failure_case(
        "contains_justified_reason",
        BackendId::Html,
        Some("diagnostics.contains_reason"),
        Some(CaseRole::Boundary),
        DiagnosticMatchMode::Contains,
        Some("independent recovery"),
    )]));

    assert!(evaluation.hard_findings.is_empty());
    assert!(evaluation.advisories.is_empty());
}

#[test]
fn contains_findings_are_backend_local_and_deterministic() {
    let evaluation = evaluate_suite(&suite(vec![
        failure_case(
            "contains_backend_matrix",
            BackendId::Html,
            Some("diagnostics.contains_reason"),
            Some(CaseRole::Boundary),
            DiagnosticMatchMode::Contains,
            None,
        ),
        failure_case(
            "contains_backend_matrix",
            BackendId::HtmlWasm,
            Some("diagnostics.contains_reason"),
            Some(CaseRole::Boundary),
            DiagnosticMatchMode::Contains,
            Some("  "),
        ),
    ]));

    assert_eq!(evaluation.hard_findings.len(), 2);
    assert_eq!(
        evaluation
            .hard_findings
            .iter()
            .map(|finding| finding.message.as_str())
            .collect::<Vec<_>>(),
        vec![
            "Case 'contains_backend_matrix' backend 'html' uses diagnostic_match = \"contains\" without a non-blank authored diagnostic_match_reason.",
            "Case 'contains_backend_matrix' backend 'html_wasm' uses diagnostic_match = \"contains\" without a non-blank authored diagnostic_match_reason.",
        ]
    );
}

#[test]
fn contains_findings_preserve_existing_hard_and_advisory_ordering() {
    let evaluation = evaluate_suite(&suite(vec![
        success_case(
            "primary_without_contract",
            BackendId::Html,
            None,
            Some(CaseRole::Primary),
            None,
            Some("primary"),
        ),
        failure_case(
            "contains_without_reason",
            BackendId::Html,
            None,
            None,
            DiagnosticMatchMode::Contains,
            None,
        ),
        success_case(
            "unclassified_case",
            BackendId::Html,
            None,
            None,
            None,
            Some("marker"),
        ),
    ]));

    assert_eq!(
        evaluation
            .hard_findings
            .iter()
            .map(|finding| finding.code.as_str())
            .collect::<Vec<_>>(),
        vec![
            "primary_missing_contract",
            "diagnostic_contains_requires_reason",
        ]
    );
    assert_eq!(
        evaluation
            .advisories
            .iter()
            .map(|finding| (finding.case_id.as_deref(), finding.code.as_str()))
            .collect::<Vec<_>>(),
        vec![
            (
                Some("contains_without_reason"),
                "missing_contract_classification"
            ),
            (
                Some("contains_without_reason"),
                "missing_role_classification"
            ),
            (Some("unclassified_case"), "missing_contract_classification"),
            (Some("unclassified_case"), "missing_role_classification"),
        ]
    );
}

#[test]
fn stronger_mixed_backend_contract_does_not_force_smoke_role() {
    let suite = suite(vec![
        success_case(
            "mixed",
            BackendId::Html,
            None,
            Some(CaseRole::Backend),
            Some(SuccessContract::AcceptanceOnly),
            None,
        ),
        success_case(
            "mixed",
            BackendId::HtmlWasm,
            None,
            Some(CaseRole::Backend),
            None,
            Some("marker"),
        ),
    ]);

    let evaluation = evaluate_suite(&suite);

    assert!(evaluation.hard_findings.is_empty());
}

#[test]
fn missing_classification_findings_are_advisories() {
    let suite = suite(vec![success_case(
        "unclassified_case",
        BackendId::Html,
        None,
        None,
        None,
        Some("marker"),
    )]);

    let evaluation = evaluate_suite(&suite);
    let codes = evaluation
        .advisories
        .iter()
        .map(|finding| finding.code.as_str())
        .collect::<Vec<_>>();

    assert!(evaluation.hard_findings.is_empty());
    assert_eq!(
        codes,
        vec![
            "missing_contract_classification",
            "missing_role_classification"
        ]
    );
}

#[test]
fn policy_findings_are_deterministic_in_manifest_order() {
    let suite = suite(vec![
        success_case("case_b", BackendId::Html, None, None, None, Some("case-b")),
        success_case("case_a", BackendId::Html, None, None, None, Some("case-a")),
    ]);

    let evaluation = evaluate_suite(&suite);
    let findings = evaluation
        .advisories
        .iter()
        .map(|finding| {
            (
                finding.case_id.as_deref().unwrap_or("<none>"),
                finding.code.as_str(),
            )
        })
        .collect::<Vec<_>>();

    assert_eq!(
        findings,
        vec![
            ("case_b", "missing_contract_classification"),
            ("case_b", "missing_role_classification"),
            ("case_a", "missing_contract_classification"),
            ("case_a", "missing_role_classification"),
        ]
    );
}
