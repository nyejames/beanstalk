//! Self-tests for integration result and artifact assertions.
//!
//! WHAT: protects diagnostic rendering, text normalization, and artifact absence contracts.
//! WHY: assertion regressions can silently weaken the suite without changing compilation.

use super::super::assertions::{
    normalize_text_for_comparison, validate_failure_result, validate_success_result,
};
use super::super::types::SuccessContract;
use super::super::{
    BackendId, ExpectedOutcome, FailureExpectation, GoldenMode, SuccessExpectation, TestCaseSpec,
    WarningExpectation,
};
use crate::build_system::build::{BuildResult, CleanupPolicy, FileKind, OutputFile, Project};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticLabel, DiagnosticLabelMessage, InvalidAssignmentTargetReason,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::Config;
use std::path::PathBuf;

const ASSERTIONS_SOURCE: &str = include_str!("../assertions.rs");

fn test_location(path: InternedPath) -> SourceLocation {
    SourceLocation::new(
        path,
        CharPosition {
            line_number: 1,
            char_column: 1,
        },
        CharPosition {
            line_number: 1,
            char_column: 2,
        },
    )
}

#[test]
fn failure_message_contains_uses_structured_render_output() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let variable_name = string_table.intern("value");
    let diagnostic = CompilerDiagnostic::invalid_assignment_target(
        InvalidAssignmentTargetReason::ImmutableBinding,
        Some(variable_name),
        None,
        None,
        None,
        None,
        test_location(source_path),
    );
    let messages = CompilerMessages::from_diagnostic(diagnostic, string_table);
    let expectation = FailureExpectation {
        warnings: WarningExpectation::Ignore,
        diagnostic_codes: vec!["BST-RULE-0044".to_string()],
        message_contains: vec!["Cannot reassign `value`".to_string()],
    };

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn failure_message_contains_includes_rendered_label_text() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let label_text = string_table.intern("secondary context lives here");
    let diagnostic = CompilerDiagnostic::invalid_assignment_target(
        InvalidAssignmentTargetReason::ImmutableBinding,
        None,
        None,
        None,
        None,
        None,
        test_location(source_path.clone()),
    )
    .with_labels(vec![DiagnosticLabel::secondary(
        test_location(source_path),
        Some(DiagnosticLabelMessage::RenderedText(label_text)),
    )]);
    let messages = CompilerMessages::from_diagnostic(diagnostic, string_table);
    let expectation = FailureExpectation {
        warnings: WarningExpectation::Ignore,
        diagnostic_codes: vec!["BST-RULE-0044".to_string()],
        message_contains: vec!["secondary context lives here".to_string()],
    };

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn failure_message_contains_stays_on_typed_render_output() {
    let removed_conversion_name = ["to", "_", "legacy", "_", "error"].concat();

    assert!(
        !ASSERTIONS_SOURCE.contains(&removed_conversion_name),
        "failure message assertions must stay on typed render-boundary output",
    );
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

const VALID_HTML: &str = "<!DOCTYPE html><html><head></head><body></body></html>";
const VALID_HTML_WASM: &str =
    "<!DOCTYPE html><html><head></head><body><script src=\"./page.js\"></script></body></html>";
const VALID_PAGE_JS: &str = "__bst_instantiate_wasm instance.exports.bst_start() \"./page.wasm\"";

fn build_result_with_output_files(files: Vec<(PathBuf, FileKind)>) -> BuildResult {
    let output_files = files
        .into_iter()
        .map(|(path, kind)| OutputFile::new(path, kind))
        .collect();
    BuildResult {
        project: Project {
            output_files,
            entry_page_rel: Some(PathBuf::from("index.html")),
            cleanup_policy: CleanupPolicy::html(),
            warnings: Vec::new(),
        },
        config: Config::new(PathBuf::from("main.bst")),
        warnings: Vec::new(),
        string_table: StringTable::new(),
    }
}

fn absence_expectation(forbidden: Vec<String>) -> SuccessExpectation {
    SuccessExpectation {
        warnings: WarningExpectation::Forbid,
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden_mode: GoldenMode::Strict,
        has_golden: false,
        rendered_output_contains: Vec::new(),
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: forbidden,
    }
}

fn acceptance_only_expectation() -> SuccessExpectation {
    SuccessExpectation {
        warnings: WarningExpectation::Forbid,
        success_contract: Some(SuccessContract::AcceptanceOnly),
        artifact_assertions: Vec::new(),
        golden_mode: GoldenMode::Strict,
        has_golden: false,
        rendered_output_contains: Vec::new(),
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: Vec::new(),
    }
}

fn absence_test_case(expectation: SuccessExpectation) -> TestCaseSpec {
    TestCaseSpec {
        display_name: "absence-contract".to_string(),
        case_id: "absence-contract".to_string(),
        manifest_relative_path: "absence-contract".to_string(),
        tags: Vec::new(),
        contract: None,
        role: None,
        backend_id: BackendId::Html,
        entry_path: PathBuf::from("."),
        golden_dir: PathBuf::from("nonexistent-golden"),
        flags: Vec::new(),
        expected: ExpectedOutcome::Success(expectation),
    }
}

fn success_test_case(backend_id: BackendId, expectation: SuccessExpectation) -> TestCaseSpec {
    TestCaseSpec {
        display_name: "success-contract".to_string(),
        case_id: "success-contract".to_string(),
        manifest_relative_path: "success-contract".to_string(),
        tags: Vec::new(),
        contract: None,
        role: None,
        backend_id,
        entry_path: PathBuf::from("."),
        golden_dir: PathBuf::from("nonexistent-golden"),
        flags: Vec::new(),
        expected: ExpectedOutcome::Success(expectation),
    }
}

#[test]
fn acceptance_only_html_baseline_rejects_broken_html() {
    let expectation = acceptance_only_expectation();
    let case = success_test_case(BackendId::Html, expectation.clone());
    let build_result = build_result_with_output_files(vec![(
        PathBuf::from("index.html"),
        FileKind::Html("<!DOCTYPE html><html><head></head><body></body>".to_owned()),
    )]);

    let result = validate_success_result(&case, build_result, &expectation);

    assert!(!result.passed);
    assert!(
        result
            .failure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("html baseline contract"))
    );
}

#[test]
fn acceptance_only_html_wasm_baseline_rejects_missing_output() {
    let expectation = acceptance_only_expectation();
    let case = success_test_case(BackendId::HtmlWasm, expectation.clone());
    let build_result = build_result_with_output_files(Vec::new());

    let result = validate_success_result(&case, build_result, &expectation);

    assert!(!result.passed);
    assert!(
        result
            .failure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("html_wasm baseline contract"))
    );
}

#[test]
fn acceptance_only_html_wasm_baseline_rejects_invalid_wasm() {
    let expectation = acceptance_only_expectation();
    let case = success_test_case(BackendId::HtmlWasm, expectation.clone());
    let build_result = build_result_with_output_files(vec![
        (
            PathBuf::from("index.html"),
            FileKind::Html(VALID_HTML_WASM.to_owned()),
        ),
        (
            PathBuf::from("page.js"),
            FileKind::Js(VALID_PAGE_JS.to_owned()),
        ),
        (PathBuf::from("page.wasm"), FileKind::Wasm(vec![0, 1, 2])),
    ]);

    let result = validate_success_result(&case, build_result, &expectation);

    assert!(!result.passed);
    assert!(
        result
            .failure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("valid wasm bytes"))
    );
}

#[test]
fn absence_contract_passes_when_forbidden_path_not_built() {
    let expectation = absence_expectation(vec!["api/index.html".to_string()]);
    let case = absence_test_case(expectation.clone());
    let build_result = build_result_with_output_files(vec![(
        PathBuf::from("index.html"),
        FileKind::Html(VALID_HTML.to_owned()),
    )]);

    let result = validate_success_result(&case, build_result, &expectation);

    assert!(
        result.passed,
        "absence contract should pass when the forbidden path is not among built artifacts"
    );
}

#[test]
fn absence_contract_fails_when_forbidden_path_is_built() {
    let expectation = absence_expectation(vec!["api/index.html".to_string()]);
    let case = absence_test_case(expectation.clone());
    let build_result = build_result_with_output_files(vec![
        (
            PathBuf::from("index.html"),
            FileKind::Html(VALID_HTML.to_owned()),
        ),
        (
            PathBuf::from("api/index.html"),
            FileKind::Html(VALID_HTML.to_owned()),
        ),
    ]);

    let result = validate_success_result(&case, build_result, &expectation);

    assert!(
        !result.passed,
        "absence contract should fail when the forbidden path is built"
    );
    let reason = result
        .failure_reason
        .expect("failure should carry a reason");
    assert!(
        reason.contains("api/index.html"),
        "failure reason should name the forbidden path: {reason}"
    );
}

#[test]
fn absence_contract_ignores_not_built_files() {
    let expectation = absence_expectation(vec!["api/index.html".to_string()]);
    let case = absence_test_case(expectation.clone());
    let build_result = build_result_with_output_files(vec![
        (
            PathBuf::from("index.html"),
            FileKind::Html(VALID_HTML.to_owned()),
        ),
        (PathBuf::from("api/index.html"), FileKind::NotBuilt),
    ]);

    let result = validate_success_result(&case, build_result, &expectation);

    assert!(
        result.passed,
        "NotBuilt files must not count as emitted artifacts"
    );
}
