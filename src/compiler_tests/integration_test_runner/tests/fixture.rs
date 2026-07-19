//! Self-tests for canonical fixture discovery and backend expansion.
//!
//! WHAT: protects fixture loading, manifest-backed ordering, and backend selection.
//! WHY: the loader translates repository fixtures into executable test cases.

use super::super::fixture::{load_canonical_case_specs, load_test_suite_from_root};
use super::super::runner::select_cases;
use super::super::{
    BackendId, CaseRole, EXPECT_FILE_NAME, GOLDEN_DIR_NAME, INPUT_DIR_NAME, MANIFEST_FILE_NAME,
    TestRunnerOptions,
};
use crate::compiler_tests::test_support::temp_dir;
use std::fs;

#[test]
fn rejects_failure_fixture_without_diagnostic_codes() {
    let root = temp_dir("failure_contract_missing_codes");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "x = 1\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("fixture should be rejected");
    };
    assert!(
        error.contains("diagnostic_codes"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn accepts_failure_fixture_without_message_contains() {
    let root = temp_dir("failure_contract_codes_only");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "x = 1\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\n",
    )
    .expect("should write expect file");

    load_canonical_case_specs(&case_root, None)
        .expect("diagnostic-code-only failure fixtures should be accepted");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_canonical_fixture_without_expectation_before_execution() {
    let root = temp_dir("missing_expectation");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "not valid Beanstalk source\n")
        .expect("should write fixture source");

    let expected_path = case_root.join(EXPECT_FILE_NAME);
    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("fixture without an expectation file should be rejected");
    };
    assert!(
        error.contains("Canonical case 'case'")
            && error.contains("missing required expectation file")
            && error.contains(&case_root.display().to_string())
            && error.contains(&expected_path.display().to_string()),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_acceptance_only_fixture_with_golden_artifacts() {
    let root = temp_dir("acceptance_only_golden_artifacts");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    let golden_root = case_root.join(GOLDEN_DIR_NAME).join("html");
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::create_dir_all(&golden_root).expect("should create fixture golden directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(golden_root.join("index.html"), "<h1>ok</h1>\n")
        .expect("should write fixture golden");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nsuccess_contract = \"acceptance_only\"\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("acceptance-only fixture with golden artifacts should be rejected");
    };
    assert!(
        error.contains("acceptance_only") && error.contains("golden artifacts"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn accepts_acceptance_only_without_fixture_specific_source_marker() {
    let root = temp_dir("acceptance_only_without_source_marker");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:not_a_contract_marker]\n")
        .expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nsuccess_contract = \"acceptance_only\"\n",
    )
    .expect("should write expect file");

    load_canonical_case_specs(&case_root, None)
        .expect("acceptance-only should not require a fixture-specific source marker");

    fs::remove_dir_all(&root).expect("should clean up");
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

    let cases = load_canonical_case_specs(&case_root, None).expect("matrix case should parse");
    let names = cases
        .iter()
        .map(|case| case.display_name.as_str())
        .collect::<Vec<_>>();
    assert_eq!(names, vec!["case [html]", "case [html_wasm]"]);

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
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"matrix\"]\n",
    )
    .expect("should write manifest");

    let suite = load_test_suite_from_root(&root).expect("suite should load");
    let suite_cases = select_cases(
        suite.cases,
        &TestRunnerOptions {
            backend_filter: Some(BackendId::HtmlWasm),
            ..TestRunnerOptions::default()
        },
    );
    assert_eq!(suite_cases.len(), 1);
    assert_eq!(suite_cases[0].display_name, "case [html_wasm]");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn manifest_metadata_survives_backend_expansion() {
    let root = temp_dir("manifest_metadata_expansion");
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
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"integration\", \"maps\"]\ncontract = \"language.maps.get_alias_exclusivity\"\nrole = \"primary\"\n",
    )
    .expect("should write manifest");

    let suite = load_test_suite_from_root(&root).expect("metadata matrix case should load");
    assert_eq!(suite.cases.len(), 2);
    assert_eq!(
        suite
            .cases
            .iter()
            .map(|case| case.display_name.as_str())
            .collect::<Vec<_>>(),
        vec!["case [html]", "case [html_wasm]"]
    );

    for case in &suite.cases {
        assert_eq!(case.case_id, "case");
        assert_eq!(case.manifest_relative_path, "case");
        assert_eq!(case.tags, vec!["integration", "maps"]);
        assert_eq!(
            case.contract.as_deref(),
            Some("language.maps.get_alias_exclusivity")
        );
        assert_eq!(case.role, Some(CaseRole::Primary));
    }

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

    let cases = load_canonical_case_specs(&case_root, None).expect("cases should parse");
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

#[test]
fn accepts_success_fixture_with_golden_only_assertion() {
    let root = temp_dir("success_contract_golden_assertion");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    let golden_root = case_root.join(GOLDEN_DIR_NAME).join("html");
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::create_dir_all(&golden_root).expect("should create fixture golden directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(golden_root.join("index.html"), "<h1>ok</h1>\n").expect("should write golden file");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n",
    )
    .expect("should write expect file");

    let cases = load_canonical_case_specs(&case_root, None).expect("fixture should be accepted");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].display_name, "case [html]");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}
