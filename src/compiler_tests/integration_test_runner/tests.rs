//! Self-tests for the integration test runner machinery.
//!
//! WHAT: validates manifest contract enforcement, expectation parsing, fixture loading,
//!       backend matrix expansion, and execution bookkeeping.
//! WHY: the test runner is load-bearing infrastructure — catching regressions here prevents
//!      silent changes in how fixtures are discovered, parsed, or executed.

use super::assertions::normalize_text_for_comparison;
use super::execution::panic_case_result;
use super::expectations::parse_expectation_file;
use super::fixture::{
    load_canonical_case_specs, load_test_suite_from_root, load_test_suite_from_root_with_filter,
};
use super::{
    BackendId, EXPECT_FILE_NAME, FailureKind, GOLDEN_DIR_NAME, INPUT_DIR_NAME, MANIFEST_FILE_NAME,
};
use std::fs;
use std::path::{Path, PathBuf};
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

// ─── Normalization unit tests ───────────────────────────────────────────────

#[test]
fn normalization_replaces_fn_counter_suffix() {
    assert_eq!(
        normalize_text_for_comparison("bst_rhs_and_fn0"),
        "bst_rhs_and_fnN"
    );
    assert_eq!(
        normalize_text_for_comparison("bst_start_fn1"),
        "bst_start_fnN"
    );
}

#[test]
fn normalization_replaces_local_counter_suffix() {
    assert_eq!(
        normalize_text_for_comparison("bst_calls_l0"),
        "bst_calls_lN"
    );
    assert_eq!(
        normalize_text_for_comparison("bst_lhs_l1 bst_value_l3"),
        "bst_lhs_lN bst_value_lN"
    );
}

#[test]
fn normalization_replaces_hir_tmp_counters() {
    assert_eq!(
        normalize_text_for_comparison("bst___hir_tmp_0_l4"),
        "bst___hir_tmp_N_lN"
    );
    assert_eq!(
        normalize_text_for_comparison("bst___hir_tmp_3_l13"),
        "bst___hir_tmp_N_lN"
    );
}

#[test]
fn normalization_replaces_template_fn_counters() {
    assert_eq!(
        normalize_text_for_comparison("bst___template_fn_0_fn3"),
        "bst___template_fn_N_fnN"
    );
    assert_eq!(
        normalize_text_for_comparison("bst___template_fn_2_fn5"),
        "bst___template_fn_N_fnN"
    );
}

#[test]
fn normalization_replaces_frag_counters() {
    assert_eq!(
        normalize_text_for_comparison("bst___bst_frag_0_fn2"),
        "bst___bst_frag_N_fnN"
    );
}

#[test]
fn normalization_preserves_runtime_library_names() {
    let input = "__bs_read __bs_write __bs_binding __bs_assign_value __bs_result_fallback";
    assert_eq!(normalize_text_for_comparison(input), input);
}

#[test]
fn normalization_is_deterministic() {
    let input = "function bst_rhs_and_fn0(bst_calls_l2) { bst___hir_tmp_3_l13; }";
    let first = normalize_text_for_comparison(input);
    let second = normalize_text_for_comparison(input);
    assert_eq!(first, second);
}

#[test]
fn normalization_does_not_mask_semantic_name_change() {
    let a = normalize_text_for_comparison("bst_rhs_and_fn0");
    let b = normalize_text_for_comparison("bst_rhs_or_fn0");
    assert_ne!(
        a, b,
        "different base names must still differ after normalization"
    );
}

#[test]
fn normalization_preserves_non_bst_identifiers() {
    let input = "function foo(x) { return x + 1; }";
    assert_eq!(normalize_text_for_comparison(input), input);
}

#[test]
fn normalization_preserves_base_name_segment() {
    let result = normalize_text_for_comparison("bst_rhs_and_fn0");
    assert!(
        result.starts_with("bst_rhs_and_fn"),
        "base name must be preserved: {result}"
    );
    assert!(
        result.ends_with('N'),
        "only the counter is replaced: {result}"
    );
}

// ─── New fixture contract tests ─────────────────────────────────────────────

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

    let cases = load_canonical_case_specs(&case_root, None, None)
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

    let Err(error) = load_canonical_case_specs(&case_root, None, None) else {
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

    let cases = load_canonical_case_specs(&case_root, None, None)
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
        "[backends.html]\nmode = \"failure\"\nwarnings = \"ignore\"\nerror_type = \"rule\"\nmessage_contains = [\"x\"]\nrendered_output_contains = [\"y\"]\n",
    )
    .expect("should write expect file");

    let Err(error) = load_canonical_case_specs(&case_root, None, None) else {
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

    let Err(error) = load_canonical_case_specs(&case_root, None, None) else {
        panic!("normalized_contains on wasm should be rejected");
    };
    assert!(error.contains("text-only"), "unexpected error: {error}");

    fs::remove_dir_all(&root).expect("should clean up");
}

#[test]
fn panic_execution_result_has_harness_failed_kind() {
    let result = panic_case_result(Box::new("boom".to_string()));
    assert!(!result.passed);
    assert_eq!(result.failure_kind, Some(FailureKind::HarnessFailed));
}
