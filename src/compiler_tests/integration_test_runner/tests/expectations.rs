//! Self-tests for integration expectation schema parsing.
//!
//! WHAT: protects backend matrix contracts and expectation-only validation.
//! WHY: malformed expectations must fail before fixture execution begins.

use super::super::expectations::parse_expectation_file;
use super::super::fixture::load_canonical_case_specs;
use super::super::types::{DiagnosticMatchMode, SuccessContract};
use super::super::{EXPECT_FILE_NAME, ExpectedOutcome, GOLDEN_DIR_NAME, INPUT_DIR_NAME};
use crate::compiler_tests::test_support::temp_dir;
use std::fs;
use std::path::PathBuf;

fn write_fixture(name: &str, expectation_source: &str) -> (PathBuf, PathBuf) {
    let root = temp_dir(name);
    let case_root = root.join("case");
    let input_root = case_root.join(INPUT_DIR_NAME);
    fs::create_dir_all(&input_root).expect("should create fixture input directory");
    fs::write(input_root.join("#page.bst"), "#[:ok]\n").expect("should write fixture source");
    fs::write(case_root.join(EXPECT_FILE_NAME), expectation_source)
        .expect("should write expect file");
    (root, case_root)
}

#[test]
fn accepts_explicit_acceptance_only_and_retains_typed_intent() {
    let (root, case_root) = write_fixture(
        "explicit_acceptance_only",
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nsuccess_contract = \"acceptance_only\"\n",
    );

    let cases = load_canonical_case_specs(&case_root, None)
        .expect("explicit acceptance-only fixture should be accepted");
    let ExpectedOutcome::Success(expectation) = &cases[0].expected else {
        panic!("case should have a success expectation");
    };
    assert_eq!(
        expectation.success_contract,
        Some(SuccessContract::AcceptanceOnly)
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn failure_diagnostic_match_defaults_to_exact_and_is_retained() {
    let (root, case_root) = write_fixture(
        "default_diagnostic_match",
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\n",
    );

    let cases = load_canonical_case_specs(&case_root, None)
        .expect("failure fixture should use exact diagnostic matching by default");
    let super::super::types::ExpectedOutcome::Failure(expectation) = &cases[0].expected else {
        panic!("case should have a failure expectation");
    };
    assert_eq!(expectation.diagnostic_match, DiagnosticMatchMode::Exact);
    assert_eq!(expectation.diagnostic_match_reason, None);

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn justified_contains_diagnostic_match_is_retained() {
    let (root, case_root) = write_fixture(
        "contains_diagnostic_match",
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\ndiagnostic_match = \"contains\"\ndiagnostic_match_reason = \"independent recovery\"\n",
    );

    let cases = load_canonical_case_specs(&case_root, None)
        .expect("justified contains matching should be accepted");
    let super::super::types::ExpectedOutcome::Failure(expectation) = &cases[0].expected else {
        panic!("case should have a failure expectation");
    };
    assert_eq!(expectation.diagnostic_match, DiagnosticMatchMode::Contains);
    assert_eq!(
        expectation.diagnostic_match_reason.as_deref(),
        Some("independent recovery")
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn contains_diagnostic_match_requires_non_blank_reason() {
    for (name, reason) in [("missing", None), ("blank", Some("   "))] {
        let reason_line = reason.map_or_else(String::new, |value| {
            format!("diagnostic_match_reason = \"{value}\"\n")
        });
        let (root, case_root) = write_fixture(
            &format!("contains_without_reason_{name}"),
            &format!(
                "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\ndiagnostic_match = \"contains\"\n{reason_line}"
            ),
        );

        let Err(error) = load_canonical_case_specs(&case_root, None) else {
            panic!("contains matching without a non-blank reason should be rejected");
        };
        assert!(
            error.contains("diagnostic_match_reason") && error.contains("contains"),
            "unexpected error: {error}"
        );

        fs::remove_dir_all(&root).expect("should clean up");
    }
}

#[test]
fn exact_diagnostic_match_rejects_authored_reason() {
    let (root, case_root) = write_fixture(
        "exact_diagnostic_match_reason",
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\ndiagnostic_match = \"exact\"\ndiagnostic_match_reason = \"not allowed\"\n",
    );

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("exact matching with a reason should be rejected");
    };
    assert!(
        error.contains("diagnostic_match_reason") && error.contains("exact"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn diagnostic_match_fields_are_failure_only() {
    let (root, case_root) = write_fixture(
        "success_diagnostic_match_field",
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\ndiagnostic_match = \"exact\"\nrendered_output_contains = [\"ok\"]\n",
    );

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("diagnostic_match should be rejected on success expectations");
    };
    assert!(
        error.contains("failure-only") && error.contains("diagnostic_match"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_unknown_success_contract_value_with_backend_context() {
    let (root, case_root) = write_fixture(
        "unknown_success_contract",
        "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nsuccess_contract = \"typecheck_only\"\n",
    );

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("unknown success_contract should be rejected");
    };
    assert!(
        error.contains("success_contract")
            && error.contains("typecheck_only")
            && error.contains("[backends.html]"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_acceptance_only_on_failure_backend() {
    let (root, case_root) = write_fixture(
        "acceptance_only_failure_mode",
        "[backends.html]\nmode = \"failure\"\nwarnings = \"forbid\"\nsuccess_contract = \"acceptance_only\"\ndiagnostic_codes = [\"BST-RULE-0001\"]\n",
    );

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("acceptance_only on a failure backend should be rejected");
    };
    assert!(
        error.contains("mode = \"failure\"") && error.contains("success_contract"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_acceptance_only_mixed_with_success_assertions() {
    let mixed_contracts = [
        (
            "artifact",
            "\n[[backends.html.artifact_assertions]]\npath = \"index.html\"\nkind = \"html\"\nmust_contain = [\"ok\"]\n",
        ),
        ("golden_mode", "\ngolden_mode = \"normalized\"\n"),
        ("rendered_output", "\nrendered_output_contains = [\"ok\"]\n"),
        (
            "artifact_absence",
            "\nartifacts_must_not_exist = [\"unexpected.html\"]\n",
        ),
    ];

    for (name, extra_contract) in mixed_contracts {
        let (root, case_root) = write_fixture(
            &format!("acceptance_only_mixed_{name}"),
            &format!(
                "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nsuccess_contract = \"acceptance_only\"\n{extra_contract}"
            ),
        );

        let Err(error) = load_canonical_case_specs(&case_root, None) else {
            panic!("acceptance-only mixed with {name} should be rejected");
        };
        assert!(
            error.contains("acceptance_only") && error.contains("must not combine"),
            "unexpected error for {name}: {error}"
        );

        fs::remove_dir_all(&root).expect("should clean up");
    }
}

#[test]
fn rejects_removed_success_contract_spelling() {
    let removed_contract = ["compile", "_only"].concat();
    let (root, case_root) = write_fixture(
        "removed_success_contract_spelling",
        &format!(
            "[backends.html]\nmode = \"success\"\nwarnings = \"forbid\"\nsuccess_contract = \"{removed_contract}\"\n"
        ),
    );

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("the removed success contract spelling should be rejected");
    };
    assert!(
        error.contains(&removed_contract) && error.contains("acceptance_only"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn rejects_acceptance_only_with_authored_expected_warning() {
    let (root, case_root) = write_fixture(
        "acceptance_only_expected_warning",
        "[backends.html]\nmode = \"success\"\nwarnings = \"exact\"\nwarning_count = 1\nsuccess_contract = \"acceptance_only\"\n",
    );

    let Err(error) = load_canonical_case_specs(&case_root, None) else {
        panic!("acceptance-only with an authored expected warning should be rejected");
    };
    assert!(
        error.contains("acceptance_only") && error.contains("expected-warning"),
        "unexpected error: {error}"
    );

    fs::remove_dir_all(&root).expect("should clean up");
}

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
