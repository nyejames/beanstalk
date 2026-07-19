//! Self-tests for deterministic integration-case listing output.
//!
//! WHAT: protects grouped listing and audit inventory reporting.
//! WHY: both reporting modes must expose retained metadata without invoking case execution.

use super::super::reporting::{build_suite_inventory_report, format_case_listing};
use super::super::types::{GoldenExpectation, SuccessContract};
use super::super::{
    BackendId, CaseRole, ExpectedOutcome, FailureExpectation, SuccessExpectation, TestCaseSpec,
    WarningExpectation,
};
use std::path::PathBuf;

fn case(
    case_id: &str,
    backend_id: BackendId,
    tags: &[&str],
    contract: Option<&str>,
    role: Option<CaseRole>,
    expected: ExpectedOutcome,
) -> TestCaseSpec {
    TestCaseSpec {
        display_name: format!("{case_id} [{}]", backend_id.as_str()),
        case_id: case_id.to_owned(),
        manifest_relative_path: case_id.to_owned(),
        tags: tags.iter().map(|tag| (*tag).to_owned()).collect(),
        contract: contract.map(str::to_owned),
        role,
        backend_id,
        entry_path: PathBuf::from("input/#page.bst"),
        flags: Vec::new(),
        expected,
    }
}

#[test]
fn listing_groups_selected_backends_and_retains_case_metadata() {
    let listing = format_case_listing(&[
        case(
            "case_a",
            BackendId::Html,
            &["integration", "language"],
            Some("language.case_a"),
            Some(CaseRole::Primary),
            ExpectedOutcome::Failure(FailureExpectation {
                warnings: WarningExpectation::Forbid,
                message_contains: Vec::new(),
                diagnostic_codes: vec!["BST-RULE-0001".to_owned()],
            }),
        ),
        case(
            "case_a",
            BackendId::HtmlWasm,
            &["integration", "language"],
            Some("language.case_a"),
            Some(CaseRole::Primary),
            ExpectedOutcome::Failure(FailureExpectation {
                warnings: WarningExpectation::Forbid,
                message_contains: Vec::new(),
                diagnostic_codes: vec!["BST-RULE-0001".to_owned()],
            }),
        ),
    ]);

    assert_eq!(
        listing,
        concat!(
            "case_id: case_a\n",
            "  backends:\n",
            "    - html (failure)\n",
            "    - html_wasm (failure)\n",
            "  tags: integration, language\n",
            "  contract: language.case_a\n",
            "  role: primary\n\n",
        )
    );
}

#[test]
fn empty_listing_is_explicit() {
    assert_eq!(
        format_case_listing(&[]),
        "No test cases matched the selection filters.\n"
    );
}

#[test]
fn inventory_json_groups_backend_metadata_under_one_canonical_case() {
    let html_case = case(
        "case_a",
        BackendId::Html,
        &["integration", "language"],
        Some("language.case_a"),
        Some(CaseRole::Primary),
        ExpectedOutcome::Failure(FailureExpectation {
            warnings: WarningExpectation::Forbid,
            message_contains: Vec::new(),
            diagnostic_codes: vec!["BST-RULE-0001".to_owned()],
        }),
    );
    let wasm_case = case(
        "case_a",
        BackendId::HtmlWasm,
        &["integration", "language"],
        Some("language.case_a"),
        Some(CaseRole::Primary),
        ExpectedOutcome::Success(SuccessExpectation {
            warnings: WarningExpectation::Forbid,
            success_contract: None,
            artifact_assertions: Vec::new(),
            golden: GoldenExpectation::default(),
            rendered_output_contains: vec!["ok".to_owned()],
            rendered_output_not_contains: Vec::new(),
            artifacts_must_not_exist: Vec::new(),
        }),
    );

    let report =
        build_suite_inventory_report(&[html_case, wasm_case], Some("0123456789abcdef".to_owned()));
    let json = serde_json::to_value(&report).expect("inventory should serialize");

    assert_eq!(json["schema_version"], 2);
    assert_eq!(json["repository_commit"], "0123456789abcdef");
    assert_eq!(json["manifest_case_count"], 1);
    assert_eq!(json["expanded_backend_execution_count"], 2);
    assert_eq!(json["cases"][0]["canonical_id"], "case_a");
    assert_eq!(json["cases"][0]["manifest_relative_path"], "case_a");
    assert_eq!(json["cases"][0]["role"], "primary");
    assert_eq!(
        json["cases"][0]["backends"].as_array().map(Vec::len),
        Some(2)
    );
    assert_eq!(json["cases"][0]["backends"][0]["backend"], "html");
    assert_eq!(json["cases"][0]["backends"][0]["baseline_applied"], false);
    assert_eq!(
        json["cases"][0]["backends"][0]["diagnostic_match"],
        "contains"
    );
    assert_eq!(json["cases"][0]["backends"][1]["backend"], "html_wasm");
    assert_eq!(json["cases"][0]["backends"][1]["baseline_applied"], true);
    assert_eq!(json["cases"][0]["backends"][1]["golden_present"], false);
    assert_eq!(
        json["cases"][0]["backends"][1]["golden_mode"],
        serde_json::Value::Null
    );
    assert_eq!(json["summary"]["rendered_output_backend_blocks"], 1);
    assert_eq!(
        json["hard_policy_violations"].as_array().map(Vec::len),
        Some(0)
    );
    assert_eq!(json["advisory_findings"].as_array().map(Vec::len), Some(0));
}

#[test]
fn inventory_distinguishes_acceptance_only_from_baseline_only() {
    let explicit_case = case(
        "explicit_acceptance_only",
        BackendId::Html,
        &["integration"],
        None,
        None,
        ExpectedOutcome::Success(SuccessExpectation {
            warnings: WarningExpectation::Forbid,
            success_contract: Some(SuccessContract::AcceptanceOnly),
            artifact_assertions: Vec::new(),
            golden: GoldenExpectation::default(),
            rendered_output_contains: Vec::new(),
            rendered_output_not_contains: Vec::new(),
            artifacts_must_not_exist: Vec::new(),
        }),
    );
    let baseline_only_case = case(
        "baseline_only",
        BackendId::HtmlWasm,
        &["integration"],
        None,
        None,
        ExpectedOutcome::Success(SuccessExpectation {
            warnings: WarningExpectation::Forbid,
            success_contract: None,
            artifact_assertions: Vec::new(),
            golden: GoldenExpectation::default(),
            rendered_output_contains: Vec::new(),
            rendered_output_not_contains: Vec::new(),
            artifacts_must_not_exist: Vec::new(),
        }),
    );

    let report = build_suite_inventory_report(&[explicit_case, baseline_only_case], None);
    let json = serde_json::to_value(&report).expect("inventory should serialize");

    assert_eq!(json["cases"][0]["backends"][0]["baseline_applied"], true);
    assert_eq!(json["cases"][0]["backends"][0]["acceptance_only"], true);
    assert_eq!(
        json["cases"][0]["backends"][0]["assertion_kinds"],
        serde_json::json!(["backend_baseline", "acceptance_only"])
    );
    assert_eq!(json["cases"][1]["backends"][0]["baseline_applied"], true);
    assert_eq!(json["cases"][1]["backends"][0]["acceptance_only"], false);
    assert_eq!(
        json["cases"][1]["backends"][0]["assertion_kinds"],
        serde_json::json!(["backend_baseline"])
    );
    assert_eq!(json["summary"]["acceptance_only_backend_blocks"], 1);
    assert_eq!(json["summary"]["baseline_only_backend_blocks"], 1);
}

#[test]
fn inventory_counts_authored_expected_warning_as_a_contract() {
    let report = build_suite_inventory_report(
        &[case(
            "expected_warning",
            BackendId::Html,
            &["integration"],
            None,
            Some(CaseRole::Smoke),
            ExpectedOutcome::Success(SuccessExpectation {
                warnings: WarningExpectation::Exact(1),
                success_contract: None,
                artifact_assertions: Vec::new(),
                golden: GoldenExpectation::default(),
                rendered_output_contains: Vec::new(),
                rendered_output_not_contains: Vec::new(),
                artifacts_must_not_exist: Vec::new(),
            }),
        )],
        None,
    );
    let json = serde_json::to_value(&report).expect("inventory should serialize");

    assert_eq!(json["summary"]["expected_warning_backend_blocks"], 1);
    assert_eq!(json["summary"]["baseline_only_backend_blocks"], 0);
    assert_eq!(
        json["cases"][0]["backends"][0]["assertion_kinds"],
        serde_json::json!(["backend_baseline", "expected_warning"])
    );
}

#[test]
fn whole_case_acceptance_only_requires_smoke_role() {
    let report = build_suite_inventory_report(
        &[case(
            "acceptance_only_case",
            BackendId::Html,
            &["integration"],
            None,
            Some(CaseRole::Backend),
            ExpectedOutcome::Success(SuccessExpectation {
                warnings: WarningExpectation::Forbid,
                success_contract: Some(SuccessContract::AcceptanceOnly),
                artifact_assertions: Vec::new(),
                golden: GoldenExpectation::default(),
                rendered_output_contains: Vec::new(),
                rendered_output_not_contains: Vec::new(),
                artifacts_must_not_exist: Vec::new(),
            }),
        )],
        None,
    );

    assert_eq!(report.hard_policy_violations.len(), 1);
    assert_eq!(
        report.hard_policy_violations[0].code,
        "acceptance_only_requires_smoke_role"
    );
}

#[test]
fn mixed_backend_acceptance_only_does_not_force_smoke_role() {
    let acceptance_case = case(
        "mixed_contracts",
        BackendId::Html,
        &["integration"],
        None,
        Some(CaseRole::Backend),
        ExpectedOutcome::Success(SuccessExpectation {
            warnings: WarningExpectation::Forbid,
            success_contract: Some(SuccessContract::AcceptanceOnly),
            artifact_assertions: Vec::new(),
            golden: GoldenExpectation::default(),
            rendered_output_contains: Vec::new(),
            rendered_output_not_contains: Vec::new(),
            artifacts_must_not_exist: Vec::new(),
        }),
    );
    let stronger_case = case(
        "mixed_contracts",
        BackendId::HtmlWasm,
        &["integration"],
        None,
        Some(CaseRole::Backend),
        ExpectedOutcome::Success(SuccessExpectation {
            warnings: WarningExpectation::Forbid,
            success_contract: None,
            artifact_assertions: Vec::new(),
            golden: GoldenExpectation::default(),
            rendered_output_contains: vec!["marker".to_owned()],
            rendered_output_not_contains: Vec::new(),
            artifacts_must_not_exist: Vec::new(),
        }),
    );

    let report = build_suite_inventory_report(&[acceptance_case, stronger_case], None);

    assert!(report.hard_policy_violations.is_empty());
    assert_eq!(report.summary.acceptance_only_backend_blocks, 1);
    assert_eq!(report.summary.rendered_output_backend_blocks, 1);
}

#[test]
fn inventory_reports_missing_contract_and_role_as_advisories() {
    let report = build_suite_inventory_report(
        &[case(
            "unclassified",
            BackendId::Html,
            &["integration"],
            None,
            None,
            ExpectedOutcome::Success(SuccessExpectation {
                warnings: WarningExpectation::Forbid,
                success_contract: None,
                artifact_assertions: Vec::new(),
                golden: GoldenExpectation::default(),
                rendered_output_contains: Vec::new(),
                rendered_output_not_contains: Vec::new(),
                artifacts_must_not_exist: Vec::new(),
            }),
        )],
        None,
    );

    let codes = report
        .advisory_findings
        .iter()
        .map(|finding| finding.code.as_str())
        .collect::<Vec<_>>();
    assert_eq!(
        codes,
        vec![
            "missing_contract_classification",
            "missing_role_classification"
        ]
    );
    assert!(report.hard_policy_violations.is_empty());
}

#[test]
fn inventory_keeps_duplicate_primary_contracts_in_hard_findings() {
    let cases = [
        case(
            "case_a",
            BackendId::Html,
            &["integration"],
            Some("language.shared"),
            Some(CaseRole::Primary),
            ExpectedOutcome::Success(SuccessExpectation {
                warnings: WarningExpectation::Forbid,
                success_contract: None,
                artifact_assertions: Vec::new(),
                golden: GoldenExpectation::default(),
                rendered_output_contains: Vec::new(),
                rendered_output_not_contains: Vec::new(),
                artifacts_must_not_exist: Vec::new(),
            }),
        ),
        case(
            "case_b",
            BackendId::Html,
            &["integration"],
            Some("language.shared"),
            Some(CaseRole::Primary),
            ExpectedOutcome::Success(SuccessExpectation {
                warnings: WarningExpectation::Forbid,
                success_contract: None,
                artifact_assertions: Vec::new(),
                golden: GoldenExpectation::default(),
                rendered_output_contains: Vec::new(),
                rendered_output_not_contains: Vec::new(),
                artifacts_must_not_exist: Vec::new(),
            }),
        ),
    ];

    let report = build_suite_inventory_report(&cases, None);
    assert_eq!(report.hard_policy_violations.len(), 1);
    assert_eq!(
        report.hard_policy_violations[0].code,
        "duplicate_primary_contract"
    );
}
