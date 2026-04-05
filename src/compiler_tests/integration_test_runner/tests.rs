//! Self-tests for the integration test runner machinery.
//!
//! WHAT: validates manifest contract enforcement, expectation parsing, fixture loading,
//!       backend matrix expansion, and execution bookkeeping.
//! WHY: the test runner is load-bearing infrastructure — catching regressions here prevents
//!      silent changes in how fixtures are discovered, parsed, or executed.

use super::execution::panic_case_result;
use super::expectations::parse_expectation_file;
use super::fixture::{
    load_canonical_case_specs, load_test_suite_from_root, load_test_suite_from_root_with_filter,
};
use super::{BackendId, EXPECT_FILE_NAME, GOLDEN_DIR_NAME, INPUT_DIR_NAME, MANIFEST_FILE_NAME};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_integration_runner_{prefix}_{unique}"))
}

fn write_success_fixture(root: &PathBuf, case_name: &str) {
    let case_root = root.join(case_name);
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n",
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
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\nerror_type = \"rule\"\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None, None) else {
        panic!("fixture should be rejected");
    };
    assert!(
        error.contains("message_contains"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_legacy_top_level_expectation_contract() {
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

    let Err(error) = load_canonical_case_specs(&case_root, None, None) else {
        panic!("legacy fixture should be rejected");
    };
    assert!(
        error.contains("[backends.<id>]"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_backend_panic_expectation_key() {
    let root = temp_dir("reject_backend_panic_key");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"failure\"\npanic = true\nwarnings = \"forbid\"\nerror_type = \"rule\"\nmessage_contains = [\"x\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = parse_expectation_file(&case_root.join(EXPECT_FILE_NAME)) else {
        panic!("panic key should be rejected");
    };
    assert!(error.contains("unknown field"), "unexpected error: {error}");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_top_level_panic_expectation_key() {
    let root = temp_dir("reject_top_level_panic_key");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "panic = true\n\n[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n",
    )
    .expect("should write expect file");

    let Err(error) = parse_expectation_file(&case_root.join(EXPECT_FILE_NAME)) else {
        panic!("panic key should be rejected");
    };
    assert!(error.contains("unknown field"), "unexpected error: {error}");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn rejects_manifest_case_without_tags() {
    let root = temp_dir("manifest_missing_tags");
    fs::create_dir_all(&root).expect("should create root");
    write_success_fixture(&root, "case");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case\"\npath = \"case\"\n",
    )
    .expect("should write manifest");

    let Err(error) = load_test_suite_from_root(&root) else {
        panic!("manifest missing tags should be rejected");
    };
    assert!(
        error.contains("missing required tags"),
        "unexpected: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn panic_execution_results_are_always_failures() {
    let result = panic_case_result(Box::new("boom".to_string()));
    assert!(!result.passed);
    assert_eq!(result.panic_message.as_deref(), Some("boom"));
    assert!(result.failure_reason.is_some());
}

#[test]
fn accepts_success_fixture_without_explicit_artifact_assertions() {
    let root = temp_dir("success_contract_golden_assertion");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n",
    )
    .expect("should write expect file");

    let cases =
        load_canonical_case_specs(&case_root, None, None).expect("fixture should be accepted");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].display_name, "case [html]");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn manifest_order_is_preserved() {
    let root = temp_dir("manifest_order");
    fs::create_dir_all(&root).expect("should create root");

    write_success_fixture(&root, "case_a");
    write_success_fixture(&root, "case_b");
    write_success_fixture(&root, "case_c");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case_b\"\npath = \"case_b\"\ntags = [\"ordered\"]\n\n[[case]]\nid = \"case_a\"\npath = \"case_a\"\ntags = [\"ordered\"]\n\n[[case]]\nid = \"case_c\"\npath = \"case_c\"\ntags = [\"ordered\"]\n",
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
fn manifest_must_declare_every_fixture_directory() {
    let root = temp_dir("manifest_authoritative");
    fs::create_dir_all(&root).expect("should create root");

    write_success_fixture(&root, "case_a");
    write_success_fixture(&root, "case_b");

    fs::write(
        root.join(MANIFEST_FILE_NAME),
        "[[case]]\nid = \"case_a\"\npath = \"case_a\"\ntags = [\"coverage\"]\n",
    )
    .expect("should write manifest");

    let Err(error) = load_test_suite_from_root(&root) else {
        panic!("manifest should reject undeclared fixtures");
    };
    assert!(
        error.contains("undeclared fixtures"),
        "unexpected error: {error}"
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

    let Err(error) = load_canonical_case_specs(&case_root, None, None) else {
        panic!("fixture should be rejected");
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
        "[[case]]\nid = \"case\"\npath = \"case\"\ntags = [\"matrix\"]\n",
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

    let cases =
        load_canonical_case_specs(&case_root, None, None).expect("fixture should be accepted");
    assert_eq!(cases.len(), 1);
    assert_eq!(cases[0].display_name, "case [html]");

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}
