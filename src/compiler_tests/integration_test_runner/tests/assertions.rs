//! Self-tests for integration result and artifact assertions.
//!
//! WHAT: protects diagnostic rendering, text normalization, and artifact absence contracts.
//! WHY: assertion regressions can silently weaken the suite without changing compilation.

use super::super::assertions::{
    compare_text_golden, discover_golden_expectation, normalize_text_for_comparison,
    validate_failure_result, validate_golden_outputs, validate_rendered_output_fragments,
    validate_success_result,
};
use super::super::types::{ExactWarningExpectation, GoldenExpectation, SuccessContract};
use super::super::{
    BackendId, DiagnosticMatchMode, ExpectedOutcome, FailureExpectation, FailureKind, GoldenMode,
    SuccessExpectation, TestCaseSpec, WarningExpectation,
};
use crate::build_system::build::{BuildResult, CleanupPolicy, FileKind, OutputFile, Project};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::source_location::{CharPosition, SourceLocation};
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticLabel, DiagnosticLabelMessage, InvalidAssignmentTargetReason,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_tests::test_support::temp_dir;
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;

const DIAGNOSTICS_SOURCE: &str = include_str!("../assertions/diagnostics.rs");

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

fn diagnostic_messages(codes: &[&str]) -> CompilerMessages {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let diagnostics = codes
        .iter()
        .map(|code| match *code {
            "BST-RULE-0044" => CompilerDiagnostic::invalid_assignment_target(
                InvalidAssignmentTargetReason::ImmutableBinding,
                None,
                None,
                None,
                None,
                None,
                test_location(source_path.clone()),
            ),
            "BST-SYNTAX-0003" => {
                CompilerDiagnostic::unexpected_trailing_comma(test_location(source_path.clone()))
            }
            other => panic!("test diagnostic code is not constructed: {other}"),
        })
        .collect();

    CompilerMessages::from_diagnostics(diagnostics, string_table)
}

fn diagnostic_expectation(
    expected_codes: &[&str],
    diagnostic_match: DiagnosticMatchMode,
    diagnostic_match_reason: Option<&str>,
) -> FailureExpectation {
    FailureExpectation {
        warnings: WarningExpectation::Ignore,
        message_contains: Vec::new(),
        diagnostic_codes: expected_codes
            .iter()
            .map(|code| (*code).to_owned())
            .collect(),
        diagnostic_match,
        diagnostic_match_reason: diagnostic_match_reason.map(str::to_owned),
    }
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
        diagnostic_match: DiagnosticMatchMode::Exact,
        diagnostic_match_reason: None,
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
        diagnostic_match: DiagnosticMatchMode::Exact,
        diagnostic_match_reason: None,
        message_contains: vec!["secondary context lives here".to_string()],
    };

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn failure_message_contains_stays_on_typed_render_output() {
    let removed_conversion_name = ["to", "_", "legacy", "_", "error"].concat();

    assert!(
        !DIAGNOSTICS_SOURCE.contains(&removed_conversion_name),
        "failure message assertions must stay on typed render-boundary output",
    );
}

#[test]
fn exact_diagnostic_matching_ignores_order() {
    let messages = diagnostic_messages(&["BST-SYNTAX-0003", "BST-RULE-0044"]);
    let expectation = diagnostic_expectation(
        &["BST-RULE-0044", "BST-SYNTAX-0003"],
        DiagnosticMatchMode::Exact,
        None,
    );

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn exact_diagnostic_matching_reports_unexpected_extra() {
    let messages = diagnostic_messages(&["BST-RULE-0044", "BST-SYNTAX-0003"]);
    let expectation = diagnostic_expectation(&["BST-RULE-0044"], DiagnosticMatchMode::Exact, None);

    let result = validate_failure_result(messages, &expectation);
    let reason = result
        .failure_reason
        .expect("unexpected diagnostic should fail matching");

    assert!(
        reason.contains("Unexpected codes: BST-SYNTAX-0003"),
        "{reason}"
    );
    assert!(!reason.contains("Missing codes"), "{reason}");
}

#[test]
fn exact_diagnostic_matching_reports_duplicate_count_mismatch() {
    let messages = diagnostic_messages(&["BST-RULE-0044", "BST-RULE-0044"]);
    let expectation = diagnostic_expectation(&["BST-RULE-0044"], DiagnosticMatchMode::Exact, None);

    let result = validate_failure_result(messages, &expectation);
    let reason = result
        .failure_reason
        .expect("duplicate diagnostic should fail matching");

    assert!(reason.contains("Count-mismatched codes"), "{reason}");
    assert!(reason.contains("expected 1, actual 2"), "{reason}");
    assert!(!reason.contains("Unexpected codes"), "{reason}");
}

#[test]
fn exact_diagnostic_matching_keeps_missing_and_unexpected_categories_distinct() {
    let messages = diagnostic_messages(&["BST-SYNTAX-0003"]);
    let expectation = diagnostic_expectation(&["BST-RULE-0044"], DiagnosticMatchMode::Exact, None);

    let result = validate_failure_result(messages, &expectation);
    let reason = result
        .failure_reason
        .expect("different diagnostic should fail matching");

    assert!(reason.contains("Missing codes: BST-RULE-0044"), "{reason}");
    assert!(
        reason.contains("Unexpected codes: BST-SYNTAX-0003"),
        "{reason}"
    );
    assert!(!reason.contains("Count-mismatched codes"), "{reason}");
}

#[test]
fn justified_contains_matching_accepts_extra_diagnostics() {
    let messages = diagnostic_messages(&["BST-RULE-0044", "BST-SYNTAX-0003"]);
    let expectation = diagnostic_expectation(
        &["BST-RULE-0044"],
        DiagnosticMatchMode::Contains,
        Some("independent parser recovery can emit a second diagnostic"),
    );

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn justified_contains_matching_accepts_extra_expected_code_occurrences() {
    let messages = diagnostic_messages(&["BST-RULE-0044", "BST-RULE-0044"]);
    let expectation = diagnostic_expectation(
        &["BST-RULE-0044"],
        DiagnosticMatchMode::Contains,
        Some("independent recovery may repeat this diagnostic"),
    );

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn contains_matching_requires_every_expected_occurrence() {
    let messages = diagnostic_messages(&["BST-RULE-0044"]);
    let expectation = diagnostic_expectation(
        &["BST-RULE-0044", "BST-RULE-0044"],
        DiagnosticMatchMode::Contains,
        Some("two independent sites must report the same diagnostic"),
    );

    let result = validate_failure_result(messages, &expectation);
    let reason = result
        .failure_reason
        .expect("a missing expected occurrence should fail matching");

    assert!(reason.contains("Count-mismatched codes"), "{reason}");
    assert!(reason.contains("expected 2, actual 1"), "{reason}");
    assert!(!reason.contains("Unexpected codes"), "{reason}");
}

fn exact_warning_expectation(codes: &[&str]) -> WarningExpectation {
    WarningExpectation::Exact(ExactWarningExpectation {
        expected_codes: codes.iter().map(|code| (*code).to_owned()).collect(),
    })
}

fn warning_build_result(codes: &[&str]) -> BuildResult {
    let mut result = build_result_with_index_html(VALID_HTML);
    let mut string_table = StringTable::new();
    let alias = string_table.intern("Alias");
    let symbol = string_table.intern("symbol");
    let warnings = codes
        .iter()
        .map(|code| match *code {
            "BST-RULE-0022" => CompilerDiagnostic::unreachable_match_arm(SourceLocation::default()),
            "BST-IMPORT-0003" => CompilerDiagnostic::import_alias_case_mismatch(
                alias,
                symbol,
                SourceLocation::default(),
            ),
            other => panic!("test warning code is not constructed: {other}"),
        })
        .collect();
    result.string_table = string_table;
    result.warnings = warnings;
    result
}

#[test]
fn exact_warning_codes_match_success_warnings_independent_of_order() {
    let expectation = SuccessExpectation {
        warnings: exact_warning_expectation(&["BST-IMPORT-0003", "BST-RULE-0022"]),
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden: GoldenExpectation::default(),
        rendered_output_contains: Vec::new(),
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: Vec::new(),
    };
    let case = success_test_case(BackendId::Html, expectation.clone());
    let result = validate_success_result(
        &case,
        warning_build_result(&["BST-RULE-0022", "BST-IMPORT-0003"]),
        &expectation,
    );

    assert!(result.passed, "{:?}", result.failure_reason);
}

#[test]
fn exact_warning_codes_report_missing_and_unexpected_codes() {
    let expectation = SuccessExpectation {
        warnings: exact_warning_expectation(&["BST-RULE-0022"]),
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden: GoldenExpectation::default(),
        rendered_output_contains: Vec::new(),
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: Vec::new(),
    };
    let case = success_test_case(BackendId::Html, expectation.clone());
    let result = validate_success_result(
        &case,
        warning_build_result(&["BST-IMPORT-0003"]),
        &expectation,
    );
    let reason = result
        .failure_reason
        .expect("different warning code should fail matching");

    assert!(reason.contains("Missing warning codes"), "{reason}");
    assert!(reason.contains("Unexpected warning codes"), "{reason}");
    assert!(
        !reason.contains("Count-mismatched warning codes"),
        "{reason}"
    );
}

#[test]
fn exact_warning_codes_report_duplicate_count_mismatch() {
    let expectation = SuccessExpectation {
        warnings: exact_warning_expectation(&["BST-RULE-0022", "BST-RULE-0022"]),
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden: GoldenExpectation::default(),
        rendered_output_contains: Vec::new(),
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: Vec::new(),
    };
    let case = success_test_case(BackendId::Html, expectation.clone());
    let result = validate_success_result(
        &case,
        warning_build_result(&["BST-RULE-0022"]),
        &expectation,
    );
    let reason = result
        .failure_reason
        .expect("duplicate warning count should fail matching");

    assert!(
        reason.contains("Count-mismatched warning codes"),
        "{reason}"
    );
    assert!(reason.contains("expected 2, actual 1"), "{reason}");
    assert!(!reason.contains("Unexpected warning codes"), "{reason}");
}

#[test]
fn ignore_and_forbid_keep_their_structured_warning_behaviour() {
    let ignored = SuccessExpectation {
        warnings: WarningExpectation::Ignore,
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden: GoldenExpectation::default(),
        rendered_output_contains: Vec::new(),
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: Vec::new(),
    };
    let ignored_case = success_test_case(BackendId::Html, ignored.clone());
    let ignored_result = validate_success_result(
        &ignored_case,
        warning_build_result(&["BST-RULE-0022"]),
        &ignored,
    );
    assert!(ignored_result.passed, "{:?}", ignored_result.failure_reason);

    let forbidden = SuccessExpectation {
        warnings: WarningExpectation::Forbid,
        ..ignored
    };
    let forbidden_case = success_test_case(BackendId::Html, forbidden.clone());
    let forbidden_result = validate_success_result(
        &forbidden_case,
        warning_build_result(&["BST-RULE-0022"]),
        &forbidden,
    );
    assert!(!forbidden_result.passed);
    assert!(
        forbidden_result
            .failure_reason
            .as_deref()
            .is_some_and(|reason| reason.contains("Expected no warnings"))
    );
}

#[test]
fn exact_warning_codes_match_warnings_retained_in_failed_compilation_messages() {
    let mut string_table = StringTable::new();
    let source_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let warning = CompilerDiagnostic::unreachable_match_arm(test_location(source_path.clone()));
    let error = CompilerDiagnostic::unexpected_trailing_comma(test_location(source_path));
    let messages = CompilerMessages::from_diagnostics(vec![error, warning], string_table);
    let expectation = FailureExpectation {
        warnings: exact_warning_expectation(&["BST-RULE-0022"]),
        message_contains: Vec::new(),
        diagnostic_codes: vec!["BST-SYNTAX-0003".to_owned(), "BST-RULE-0022".to_owned()],
        diagnostic_match: DiagnosticMatchMode::Exact,
        diagnostic_match_reason: None,
    };

    let result = validate_failure_result(messages, &expectation);

    assert!(result.passed, "{:?}", result.failure_reason);
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

fn build_result_with_index_html(html: &str) -> BuildResult {
    build_result_with_output_files(vec![(
        PathBuf::from("index.html"),
        FileKind::Html(html.to_owned()),
    )])
}

fn absence_expectation(forbidden: Vec<String>) -> SuccessExpectation {
    SuccessExpectation {
        warnings: WarningExpectation::Forbid,
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden: GoldenExpectation::default(),
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
        golden: GoldenExpectation::default(),
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

#[test]
fn strict_text_goldens_ignore_lf_vs_crlf_differences() {
    assert!(
        compare_text_golden("<p>a\r\nb</p>\r\n", "<p>a\nb</p>\n", GoldenMode::Strict).is_none()
    );
}

#[test]
fn normalized_text_goldens_ignore_lf_vs_crlf_differences() {
    assert!(
        compare_text_golden(
            "<p>bst_rhs_and_fn0\r\n</p>\r\n",
            "<p>bst_rhs_and_fn7\n</p>\n",
            GoldenMode::Normalized,
        )
        .is_none()
    );
}

#[test]
fn normalized_comparison_strips_core_css_after_crlf_normalization() {
    let normalized =
        normalize_text_for_comparison("<style>\r\nbody { color: red; }\r\n</style>\r\nok");
    assert!(normalized.contains("<style>/* CORE_CSS */</style>"));
    assert!(!normalized.contains("body { color: red; }"));
}

#[test]
fn rendered_output_fragment_validation_reports_semantic_mismatch_kind() {
    let contains = vec!["missing-fragment".to_string()];
    let result = validate_rendered_output_fragments("rendered text", &contains, &[])
        .expect("missing required fragment should fail");
    assert_eq!(result.1, FailureKind::RenderedOutputMismatch);
}

#[test]
fn rendered_output_validation_reports_harness_failure_without_script_blocks() {
    let expectation = SuccessExpectation {
        warnings: WarningExpectation::Forbid,
        success_contract: None,
        artifact_assertions: Vec::new(),
        golden: GoldenExpectation::default(),
        rendered_output_contains: vec!["anything".to_string()],
        rendered_output_not_contains: Vec::new(),
        artifacts_must_not_exist: Vec::new(),
    };
    let case = success_test_case(BackendId::Html, expectation.clone());
    let build_result = build_result_with_index_html(VALID_HTML);

    let result = validate_success_result(&case, build_result, &expectation);

    assert_eq!(result.failure_kind, Some(FailureKind::HarnessFailed));
}

#[test]
fn strict_golden_validation_treats_crlf_and_lf_as_equivalent_for_text() {
    let root = temp_dir("strict_golden_line_endings");
    let golden_dir = root.join("golden");
    fs::create_dir_all(&golden_dir).expect("should create golden dir");
    fs::write(golden_dir.join("index.html"), "<p>a\r\nb</p>\r\n")
        .expect("should write CRLF golden");

    let build_result = build_result_with_index_html("<p>a\nb</p>\n");
    let golden = discover_golden_expectation(&golden_dir, None)
        .expect("golden inventory should be discovered");
    let mismatch = validate_golden_outputs(&build_result, &golden);
    assert!(
        mismatch.is_none(),
        "strict text golden checks should ignore line-ending-only differences"
    );

    fs::remove_dir_all(&root).expect("should clean temp directory");
}

#[test]
fn normalized_golden_validation_treats_crlf_and_lf_as_equivalent_for_text() {
    let root = temp_dir("normalized_golden_line_endings");
    let golden_dir = root.join("golden");
    fs::create_dir_all(&golden_dir).expect("should create golden dir");
    fs::write(golden_dir.join("index.html"), "bst_rhs_and_fn0\r\n")
        .expect("should write CRLF golden");

    let build_result = build_result_with_index_html("bst_rhs_and_fn8\n");
    let golden = discover_golden_expectation(&golden_dir, Some(GoldenMode::Normalized))
        .expect("golden inventory should be discovered");
    let mismatch = validate_golden_outputs(&build_result, &golden);
    assert!(
        mismatch.is_none(),
        "normalized golden checks should ignore counter and line-ending drift"
    );

    fs::remove_dir_all(&root).expect("should clean temp directory");
}

#[test]
fn nested_golden_validation_compares_relative_paths() {
    let root = temp_dir("nested_golden_comparison");
    let golden_dir = root.join("golden");
    let golden_file = golden_dir.join("nested").join("page.html");
    fs::create_dir_all(golden_file.parent().expect("nested parent should exist"))
        .expect("should create nested golden directory");
    fs::write(&golden_file, "<p>nested</p>\n").expect("should write nested golden");

    let build_result = BuildResult {
        project: Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("nested/page.html"),
                FileKind::Html("<p>nested</p>\n".to_owned()),
            )],
            entry_page_rel: None,
            cleanup_policy: CleanupPolicy::html(),
            warnings: Vec::new(),
        },
        config: Config::new(PathBuf::from("main.bst")),
        warnings: Vec::new(),
        string_table: StringTable::new(),
    };
    let golden = discover_golden_expectation(&golden_dir, None)
        .expect("golden inventory should be discovered");

    assert!(validate_golden_outputs(&build_result, &golden).is_none());

    fs::remove_dir_all(&root).expect("should clean temp directory");
}
