//! Self-tests for integration expectation schema parsing.
//!
//! WHAT: protects backend matrix contracts and expectation-only validation.
//! WHY: malformed expectations must fail before fixture execution begins.

use super::super::expectations::parse_expectation_file;
use super::super::fixture::load_canonical_case_specs;
use super::super::{EXPECT_FILE_NAME, ExpectedOutcome, GOLDEN_DIR_NAME, INPUT_DIR_NAME};
use crate::compiler_tests::test_support::temp_dir;
use std::fs;

#[test]
fn rejects_error_type_expectation_key() {
    let root = temp_dir("reject_error_type_key");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "x = 1\n").expect("should write fixture source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\nerror_type = \"rule\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = parse_expectation_file(&case_root.join(EXPECT_FILE_NAME)) else {
        panic!("error_type key should be rejected");
    };
    assert!(error.contains("unknown field"), "unexpected error: {error}");

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

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
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
        "[backends.html]\nmode = \"failure\"\npanic = true\nwarnings = \"forbid\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\nmessage_contains = [\"x\"]\n",
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
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("fixture should be rejected");
    };
    assert!(
        error.contains("Unsupported backend 'wasm'"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up temp fixture root");
}

#[test]
fn accepts_normalized_golden_mode() {
    let root = temp_dir("normalized_golden_mode");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    let golden_root = case_root.join(GOLDEN_DIR_NAME).join("html");
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::create_dir_all(&golden_root).expect("should create golden directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(golden_root.join("index.html"), "<h1>ok</h1>\n").expect("should write golden");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\ngolden_mode = \"normalized\"\n",
    )
    .expect("should write expect file");

    let cases = load_canonical_case_specs(&case_root, None)
        .expect("normalized golden_mode fixture should be accepted");
    assert_eq!(cases.len(), 1);

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_unknown_golden_mode() {
    let root = temp_dir("unknown_golden_mode");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\ngolden_mode = \"fuzzy\"\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("unknown golden_mode should be rejected");
    };
    assert!(
        error.contains("golden_mode") && error.contains("fuzzy"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn accepts_success_fixture_with_rendered_output_only() {
    let root = temp_dir("rendered_output_only");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nrendered_output_contains = [\"ok\"]\n",
    )
    .expect("should write expect file");

    let cases = load_canonical_case_specs(&case_root, None)
        .expect("rendered_output_contains-only fixture should be accepted");
    assert_eq!(cases.len(), 1);

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_rendered_output_in_failure_mode() {
    let root = temp_dir("rendered_output_failure_mode");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"failure\"\nwarnings = \"ignore\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\nmessage_contains = [\"x\"]\nrendered_output_contains = [\"y\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("rendered_output_contains in failure mode should be rejected");
    };
    assert!(
        error.contains("rendered_output_contains"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_normalized_contains_on_wasm_artifact() {
    let root = temp_dir("normalized_contains_wasm");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\n\n[[backends.html.artifact_assertions]]\npath = \"page.wasm\"\nkind = \"wasm\"\nnormalized_contains = [\"something\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("normalized_contains on wasm should be rejected");
    };
    assert!(error.contains("text-only"), "unexpected error: {error}");

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn accepts_artifacts_must_not_exist_in_success_mode() {
    let root = temp_dir("absence_contract_success");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nartifacts_must_not_exist = [\"api\\\\index.html\"]\n",
    )
    .expect("should write expect file");

    let cases = load_canonical_case_specs(&case_root, None)
        .expect("artifacts_must_not_exist in success mode should be accepted");
    assert_eq!(cases.len(), 1);

    let ExpectedOutcome::Success(expectation) = &cases[0].expected else {
        panic!("case should have a success expectation");
    };
    assert_eq!(
        expectation.artifacts_must_not_exist,
        vec!["api/index.html".to_string()],
        "non-canonical backslash separator should be normalised to forward slashes"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_artifacts_must_not_exist_in_failure_mode() {
    let root = temp_dir("absence_contract_failure");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"failure\"\nwarnings = \"ignore\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\nartifacts_must_not_exist = [\"api/index.html\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("artifacts_must_not_exist in failure mode should be rejected");
    };
    assert!(
        error.contains("artifacts_must_not_exist"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_empty_artifacts_must_not_exist_entry() {
    let root = temp_dir("absence_contract_empty");
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write source");
    fs::write(
        case_root.join(EXPECT_FILE_NAME),
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nartifacts_must_not_exist = [\"\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("empty artifacts_must_not_exist entry should be rejected");
    };
    assert!(
        error.contains("empty") && error.contains("artifacts_must_not_exist"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}
