//! Result validation and artifact assertions for the integration test suite.
//!
//! WHAT: validates compilation results against success/failure expectations and checks
//!       produced artifacts for content, wasm validity, and golden file matches.
//! WHY: separating assertion logic from case execution keeps each concern independently
//!      testable and prevents the execution path from growing into a second monolith.

use super::{
    ArtifactAssertion, ArtifactKind, BackendId, CaseExecutionResult, FailureExpectation,
    FailureKind, GoldenMode, SuccessExpectation, TestCaseSpec, WarningExpectation,
    normalize_relative_path, normalize_relative_path_text,
};
use crate::build_system::build::{BuildResult, FileKind, OutputFile};
use crate::compiler_frontend::compiler_messages::compiler_errors::{
    CompilerMessages, error_type_to_str,
};
use std::fs;
use std::path::{Path, PathBuf};
use wasmparser::{Imports, Parser, Payload};

pub(crate) fn validate_success_result(
    case: &TestCaseSpec,
    build_result: BuildResult,
    expectation: &SuccessExpectation,
) -> CaseExecutionResult {
    if let Some(reason) =
        validate_warning_expectation(build_result.warnings.len(), expectation.warnings)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if case.backend_id == BackendId::Html
        && let Some(reason) = validate_html_baseline_contract(&build_result)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if case.backend_id == BackendId::HtmlWasm
        && let Some(reason) = validate_html_wasm_baseline_contract(&build_result)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if let Some(reason) =
        validate_artifact_assertions(&build_result, &expectation.artifact_assertions)
    {
        return fail(build_result, reason, FailureKind::ExpectationViolation);
    }

    if let Some((reason, kind)) =
        validate_golden_outputs(&build_result, &case.golden_dir, expectation.golden_mode)
    {
        return fail(build_result, reason, kind);
    }

    if (!expectation.rendered_output_contains.is_empty()
        || !expectation.rendered_output_not_contains.is_empty())
        && let Some((reason, kind)) = validate_rendered_output(
            &build_result,
            &expectation.rendered_output_contains,
            &expectation.rendered_output_not_contains,
        )
    {
        return fail(build_result, reason, kind);
    }

    CaseExecutionResult {
        passed: true,
        panic_message: None,
        build_result: Some(build_result),
        messages: None,
        failure_reason: None,
        failure_kind: None,
    }
}

fn fail(build_result: BuildResult, reason: String, kind: FailureKind) -> CaseExecutionResult {
    CaseExecutionResult {
        passed: false,
        panic_message: None,
        build_result: Some(build_result),
        messages: None,
        failure_reason: Some(reason),
        failure_kind: Some(kind),
    }
}

pub(crate) fn validate_failure_result(
    messages: CompilerMessages,
    expectation: &FailureExpectation,
) -> CaseExecutionResult {
    if let Some(reason) =
        validate_warning_expectation(messages.warnings.len(), expectation.warnings)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(reason),
            failure_kind: Some(FailureKind::ExpectationViolation),
        };
    }

    if !messages
        .errors
        .iter()
        .any(|error| error.error_type == expectation.error_type)
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(format!(
                "Expected error type '{}', but it was not reported.",
                error_type_to_str(&expectation.error_type)
            )),
            failure_kind: Some(FailureKind::ExpectationViolation),
        };
    }

    if !expectation.message_contains.is_empty()
        && !messages
            .errors
            .iter()
            .any(|error| contains_ordered_substrings(&error.msg, &expectation.message_contains))
    {
        return CaseExecutionResult {
            passed: false,
            panic_message: None,
            build_result: None,
            messages: Some(messages),
            failure_reason: Some(
                "Expected ordered diagnostic message fragments were not found in any emitted error."
                    .to_string(),
            ),
            failure_kind: Some(FailureKind::ExpectationViolation),
        };
    }

    CaseExecutionResult {
        passed: true,
        panic_message: None,
        build_result: None,
        messages: Some(messages),
        failure_reason: None,
        failure_kind: None,
    }
}

fn validate_warning_expectation(
    actual_count: usize,
    expectation: WarningExpectation,
) -> Option<String> {
    match expectation {
        WarningExpectation::Ignore => None,
        WarningExpectation::Forbid => {
            (actual_count > 0).then(|| format!("Expected no warnings, but found {actual_count}."))
        }
        WarningExpectation::Exact(expected) => (actual_count != expected)
            .then(|| format!("Expected exactly {expected} warnings, but found {actual_count}.")),
    }
}

fn validate_expected_artifact_paths(
    build_result: &BuildResult,
    expected_paths: &[String],
) -> Option<String> {
    let actual_paths = collect_built_artifact_paths(build_result);

    let mut expected = expected_paths
        .iter()
        .map(|path| normalize_relative_path_text(path))
        .collect::<Vec<_>>();
    expected.sort();

    if actual_paths != expected {
        return Some(format!(
            "Expected output paths {expected:?}, but produced {actual_paths:?}."
        ));
    }

    None
}

fn collect_built_artifact_paths(build_result: &BuildResult) -> Vec<String> {
    let mut actual_paths = build_result
        .project
        .output_files
        .iter()
        .filter(|output| !matches!(output.file_kind(), FileKind::NotBuilt))
        .map(|output| normalize_relative_path(output.relative_output_path()))
        .collect::<Vec<_>>();
    actual_paths.sort();
    actual_paths
}

fn validate_artifact_assertions(
    build_result: &BuildResult,
    assertions: &[ArtifactAssertion],
) -> Option<String> {
    for assertion in assertions {
        let Some(output) = find_output_file(build_result, &assertion.path) else {
            return Some(format!(
                "Artifact assertion expected output '{}', but produced paths were {:?}.",
                assertion.path,
                collect_built_artifact_paths(build_result)
            ));
        };

        if let Some(reason) = validate_single_artifact_assertion(output, assertion) {
            return Some(reason);
        }
    }

    None
}

fn validate_single_artifact_assertion(
    output: &OutputFile,
    assertion: &ArtifactAssertion,
) -> Option<String> {
    match assertion.kind {
        ArtifactKind::Html | ArtifactKind::Js => {
            let Some(text) = output_text_content(output, assertion.kind) else {
                return Some(format!(
                    "Artifact '{}' expected kind '{}', but produced a different file kind.",
                    assertion.path,
                    artifact_kind_name(assertion.kind)
                ));
            };

            for required in &assertion.must_contain {
                if !text.contains(required) {
                    return Some(format!(
                        "Artifact '{}' did not contain required fragment '{}'.",
                        assertion.path, required
                    ));
                }
            }

            for forbidden in &assertion.must_not_contain {
                if text.contains(forbidden) {
                    return Some(format!(
                        "Artifact '{}' contained forbidden fragment '{}'.",
                        assertion.path, forbidden
                    ));
                }
            }

            if !assertion.must_contain_in_order.is_empty()
                && !contains_ordered_substrings(text, &assertion.must_contain_in_order)
            {
                return Some(format!(
                    "Artifact '{}' did not contain required ordered fragments {:?}.",
                    assertion.path, assertion.must_contain_in_order
                ));
            }

            for required_once in &assertion.must_contain_exactly_once {
                let count = count_occurrences(text, required_once);
                if count != 1 {
                    return Some(format!(
                        "Artifact '{}' expected fragment '{}' exactly once, but found {} time(s).",
                        assertion.path, required_once, count
                    ));
                }
            }

            if !assertion.normalized_contains.is_empty()
                || !assertion.normalized_not_contains.is_empty()
            {
                let normalized_text = normalize_text_for_comparison(text);
                for required in &assertion.normalized_contains {
                    let normalized_required = normalize_text_for_comparison(required);
                    if !normalized_text.contains(normalized_required.as_str()) {
                        return Some(format!(
                            "Artifact '{}' did not contain required normalized fragment '{}'.",
                            assertion.path, required
                        ));
                    }
                }
                for forbidden in &assertion.normalized_not_contains {
                    let normalized_forbidden = normalize_text_for_comparison(forbidden);
                    if normalized_text.contains(normalized_forbidden.as_str()) {
                        return Some(format!(
                            "Artifact '{}' contained forbidden normalized fragment '{}'.",
                            assertion.path, forbidden
                        ));
                    }
                }
            }
        }
        ArtifactKind::Wasm => {
            let Some(bytes) = output_wasm_bytes(output) else {
                return Some(format!(
                    "Artifact '{}' expected kind 'wasm', but produced a different file kind.",
                    assertion.path
                ));
            };

            if assertion.validate_wasm
                && let Err(error) = validate_wasm_bytes(bytes)
            {
                return Some(format!(
                    "Artifact '{}' failed wasm validation: {error}",
                    assertion.path
                ));
            }

            if !assertion.must_export.is_empty() {
                let exports = match collect_wasm_exports(bytes) {
                    Ok(exports) => exports,
                    Err(error) => {
                        return Some(format!(
                            "Artifact '{}' failed while reading wasm exports: {error}",
                            assertion.path
                        ));
                    }
                };

                for required_export in &assertion.must_export {
                    if !exports.contains(required_export) {
                        return Some(format!(
                            "Artifact '{}' missing required wasm export '{}'. Available exports: {:?}.",
                            assertion.path, required_export, exports
                        ));
                    }
                }
            }

            if !assertion.must_import.is_empty() {
                let imports = match collect_wasm_imports(bytes) {
                    Ok(imports) => imports,
                    Err(error) => {
                        return Some(format!(
                            "Artifact '{}' failed while reading wasm imports: {error}",
                            assertion.path
                        ));
                    }
                };

                for required_import in &assertion.must_import {
                    if !imports.contains(required_import) {
                        return Some(format!(
                            "Artifact '{}' missing required wasm import '{}'. Available imports: {:?}.",
                            assertion.path, required_import, imports
                        ));
                    }
                }
            }
        }
        ArtifactKind::Binary => {
            if output_binary_bytes(output).is_none() {
                return Some(format!(
                    "Artifact '{}' expected kind 'binary', but produced a different file kind.",
                    assertion.path
                ));
            }
        }
    }

    None
}

fn artifact_kind_name(kind: ArtifactKind) -> &'static str {
    match kind {
        ArtifactKind::Html => "html",
        ArtifactKind::Js => "js",
        ArtifactKind::Wasm => "wasm",
        ArtifactKind::Binary => "binary",
    }
}

/// Verifies the baseline HTML backend interop/output contract.
///
/// WHAT: requires a built `index.html` HTML artifact for every html backend success case.
/// WHY: replacing legacy path assertions still needs a deterministic minimum output guarantee.
fn validate_html_baseline_contract(build_result: &BuildResult) -> Option<String> {
    let Some(index_html) = find_output_file(build_result, "index.html") else {
        return Some(
            "html baseline contract expected 'index.html', but it was not produced.".to_string(),
        );
    };

    let Some(html) = output_text_content(index_html, ArtifactKind::Html) else {
        return Some(
            "html baseline contract expected 'index.html' as an HTML artifact.".to_string(),
        );
    };

    for required_fragment in [
        "<!DOCTYPE html>",
        "<html",
        "<head>",
        "<body",
        "</body>",
        "</html>",
    ] {
        if !html.contains(required_fragment) {
            return Some(format!(
                "html baseline contract expected 'index.html' to contain '{required_fragment}'."
            ));
        }
    }

    None
}

fn validate_html_wasm_baseline_contract(build_result: &BuildResult) -> Option<String> {
    let Some(index_html) = find_output_file(build_result, "index.html") else {
        return Some(
            "html_wasm baseline contract expected 'index.html', but it was not produced."
                .to_string(),
        );
    };

    let Some(html) = output_text_content(index_html, ArtifactKind::Html) else {
        return Some(
            "html_wasm baseline contract expected 'index.html' as an HTML artifact.".to_string(),
        );
    };
    for required_fragment in [
        "<!DOCTYPE html>",
        "<html",
        "<head>",
        "<body",
        "</body>",
        "</html>",
    ] {
        if !html.contains(required_fragment) {
            return Some(format!(
                "html_wasm baseline contract expected 'index.html' to contain '{required_fragment}'."
            ));
        }
    }
    if !html.contains("<script src=\"./page.js\"></script>") {
        return Some(
            "html_wasm baseline contract expected 'index.html' to include './page.js'.".to_string(),
        );
    }
    let Some(script_pos) = html.find("<script src=\"./page.js\"></script>") else {
        return Some(
            "html_wasm baseline contract expected 'index.html' to include './page.js'.".to_string(),
        );
    };
    let Some(body_close) = html.find("</body>") else {
        return Some(
            "html_wasm baseline contract expected 'index.html' to contain '</body>'.".to_string(),
        );
    };
    if script_pos > body_close {
        return Some(
            "html_wasm baseline contract expected './page.js' to appear before '</body>'."
                .to_string(),
        );
    }

    let Some(page_js) = find_output_file(build_result, "page.js") else {
        return Some(
            "html_wasm baseline contract expected 'page.js', but it was not produced.".to_string(),
        );
    };

    let Some(js) = output_text_content(page_js, ArtifactKind::Js) else {
        return Some(
            "html_wasm baseline contract expected 'page.js' as a JS artifact.".to_string(),
        );
    };

    for required_fragment in [
        "__bst_instantiate_wasm",
        "instance.exports.bst_start()",
        "\"./page.wasm\"",
    ] {
        if !js.contains(required_fragment) {
            return Some(format!(
                "html_wasm baseline contract expected 'page.js' to contain '{required_fragment}'."
            ));
        }
    }

    let Some(page_wasm) = find_output_file(build_result, "page.wasm") else {
        return Some(
            "html_wasm baseline contract expected 'page.wasm', but it was not produced."
                .to_string(),
        );
    };

    let Some(wasm_bytes) = output_wasm_bytes(page_wasm) else {
        return Some(
            "html_wasm baseline contract expected 'page.wasm' as a wasm artifact.".to_string(),
        );
    };

    if let Err(error) = validate_wasm_bytes(wasm_bytes) {
        return Some(format!(
            "html_wasm baseline contract expected valid wasm bytes: {error}"
        ));
    }

    let exports = match collect_wasm_exports(wasm_bytes) {
        Ok(exports) => exports,
        Err(error) => {
            return Some(format!(
                "html_wasm baseline contract failed while reading wasm exports: {error}"
            ));
        }
    };

    for required_export in ["memory", "bst_str_ptr", "bst_str_len", "bst_release"] {
        if !exports.contains(&required_export.to_string()) {
            return Some(format!(
                "html_wasm baseline contract missing required export '{required_export}'. Available exports: {exports:?}."
            ));
        }
    }

    None
}

fn validate_wasm_bytes(bytes: &[u8]) -> Result<(), String> {
    wasmparser::Validator::new()
        .validate_all(bytes)
        .map(|_| ())
        .map_err(|error| error.to_string())
}

fn collect_wasm_exports(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut exports = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| error.to_string())?;
        if let Payload::ExportSection(reader) = payload {
            for export in reader {
                let export = export.map_err(|error| error.to_string())?;
                exports.push(export.name.to_string());
            }
        }
    }

    Ok(exports)
}

fn collect_wasm_imports(bytes: &[u8]) -> Result<Vec<String>, String> {
    let mut imports = Vec::new();

    for payload in Parser::new(0).parse_all(bytes) {
        let payload = payload.map_err(|error| error.to_string())?;
        if let Payload::ImportSection(reader) = payload {
            for import in reader {
                let import = match import {
                    Ok(imports) => match imports {
                        Imports::Single(_, import) => import,
                        Imports::Compact1 { module, .. } | Imports::Compact2 { module, .. } => {
                            return Err(format!(
                                "collect_wasm_imports: compact import group for module '{module}' \
                                 is not supported; Beanstalk does not emit compact imports"
                            ));
                        }
                    },
                    Err(e) => return Err(e.to_string()),
                };
                imports.push(format!("{}.{}", import.module, import.name));
            }
        }
    }

    Ok(imports)
}

fn find_output_file<'a>(
    build_result: &'a BuildResult,
    relative_path: &str,
) -> Option<&'a OutputFile> {
    let normalized_target = normalize_relative_path_text(relative_path);

    build_result.project.output_files.iter().find(|output| {
        !matches!(output.file_kind(), FileKind::NotBuilt)
            && normalize_relative_path(output.relative_output_path()) == normalized_target
    })
}

fn output_text_content(output: &OutputFile, expected_kind: ArtifactKind) -> Option<&str> {
    if matches!(expected_kind, ArtifactKind::Html)
        && let FileKind::Html(content) = output.file_kind()
    {
        return Some(content.as_str());
    }

    if matches!(expected_kind, ArtifactKind::Js)
        && let FileKind::Js(content) = output.file_kind()
    {
        return Some(content.as_str());
    }

    None
}

fn output_wasm_bytes(output: &OutputFile) -> Option<&[u8]> {
    match output.file_kind() {
        FileKind::Wasm(bytes) => Some(bytes.as_slice()),
        _ => None,
    }
}

fn output_binary_bytes(output: &OutputFile) -> Option<&[u8]> {
    match output.file_kind() {
        FileKind::Bytes(bytes) => Some(bytes.as_slice()),
        _ => None,
    }
}

fn validate_golden_outputs(
    build_result: &BuildResult,
    golden_dir: &Path,
    mode: GoldenMode,
) -> Option<(String, FailureKind)> {
    if !golden_dir.is_dir() {
        return None;
    }

    let mut expected_files = collect_files_recursive(golden_dir);
    expected_files.sort();

    if expected_files.is_empty() {
        return None;
    }

    let mut expected_paths = Vec::with_capacity(expected_files.len());
    for file in &expected_files {
        let relative = file
            .strip_prefix(golden_dir)
            .unwrap_or(file)
            .to_string_lossy()
            .replace('\\', "/");
        expected_paths.push(relative);
    }

    if let Some(reason) = validate_expected_artifact_paths(build_result, &expected_paths) {
        return Some((reason, FailureKind::StrictGoldenMismatch));
    }

    for file in expected_files {
        let relative = file
            .strip_prefix(golden_dir)
            .unwrap_or(&file)
            .to_string_lossy()
            .replace('\\', "/");

        let Some(output) = find_output_file(build_result, &relative) else {
            return Some((
                format!("Golden output '{relative}' was not produced."),
                FailureKind::StrictGoldenMismatch,
            ));
        };

        let expected_bytes = match fs::read(&file) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Some((
                    format!("Failed to read golden output '{}': {error}", file.display()),
                    FailureKind::HarnessFailed,
                ));
            }
        };

        let actual_bytes = match output.file_kind() {
            FileKind::Html(content) | FileKind::Js(content) => content.as_bytes().to_vec(),
            FileKind::Wasm(bytes) | FileKind::Bytes(bytes) => bytes.clone(),
            FileKind::Directory | FileKind::NotBuilt => Vec::new(),
        };

        // Text artifacts support normalized comparison; binary/wasm always use strict.
        let is_text = matches!(output.file_kind(), FileKind::Html(_) | FileKind::Js(_));

        if is_text && mode == GoldenMode::Normalized {
            let expected_str = String::from_utf8_lossy(&expected_bytes);
            let actual_str = match output.file_kind() {
                FileKind::Html(s) | FileKind::Js(s) => s.as_str(),
                _ => unreachable!("is_text is true"),
            };
            let norm_expected = normalize_text_for_comparison(&expected_str);
            let norm_actual = normalize_text_for_comparison(actual_str);
            if norm_expected != norm_actual {
                let detail = format!("\n{}", generate_text_diff(&norm_expected, &norm_actual, 8));
                return Some((
                    format!(
                        "Golden output '{relative}' did not match after normalization.{detail}"
                    ),
                    FailureKind::NormalizedSemanticMismatch,
                ));
            }
        } else if actual_bytes != expected_bytes {
            let detail = match output.file_kind() {
                FileKind::Html(content) | FileKind::Js(content) => {
                    let expected_str = String::from_utf8_lossy(&expected_bytes);
                    format!("\n{}", generate_text_diff(&expected_str, content, 8))
                }
                _ => format!(
                    " (expected {} bytes, got {} bytes)",
                    expected_bytes.len(),
                    actual_bytes.len()
                ),
            };
            return Some((
                format!("Golden output '{relative}' did not match the produced artifact.{detail}"),
                FailureKind::StrictGoldenMismatch,
            ));
        }
    }

    None
}

/// Normalizes compiler-generated counter suffixes in JS/HTML text for comparison.
///
/// WHAT: replaces unstable numeric counters in `bst_`-prefixed identifiers with the
///       placeholder `N`, so backend naming changes do not cause spurious golden diffs.
/// WHY: the semantics of the emitted code are determined by the identifier base name and
///      structure, not by which counter value was assigned during a specific compilation.
///
/// Stable `__bs_*` runtime library names are NOT touched.
///
/// Examples:
/// - `bst_rhs_and_fn0`        → `bst_rhs_and_fnN`
/// - `bst_calls_l2`           → `bst_calls_lN`
/// - `bst___hir_tmp_3_l13`    → `bst___hir_tmp_N_lN`
/// - `bst___template_fn_1_fn4`→ `bst___template_fn_N_fnN`
/// - `bst___bst_frag_0_fn2`   → `bst___bst_frag_N_fnN`
pub(crate) fn normalize_text_for_comparison(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut token_start: Option<usize> = None;

    for (i, ch) in text.char_indices() {
        let in_ident = ch.is_ascii_alphanumeric() || ch == '_';
        match (token_start, in_ident) {
            (None, true) => token_start = Some(i),
            (Some(start), false) => {
                result.push_str(&normalize_bst_identifier(&text[start..i]));
                result.push(ch);
                token_start = None;
            }
            (Some(_), true) => {}
            (None, false) => result.push(ch),
        }
    }
    if let Some(start) = token_start {
        result.push_str(&normalize_bst_identifier(&text[start..]));
    }
    result
}

/// Normalizes counter segments in a single `bst_`-prefixed identifier token.
///
/// Two patterns are normalized:
/// 1. Segments ending in digits whose non-digit prefix is `fn` or `l`
///    (e.g., `fn0` → `fnN`, `l13` → `lN`)
/// 2. Purely-digit standalone segments immediately after a trigger segment
///    (`fn`, `tmp`, `frag`) (e.g., `tmp_3` → `tmp_N`)
fn normalize_bst_identifier(token: &str) -> String {
    if !token.starts_with("bst_") {
        return token.to_owned();
    }

    let parts: Vec<&str> = token.split('_').collect();
    let mut result: Vec<String> = Vec::with_capacity(parts.len());

    for (i, &part) in parts.iter().enumerate() {
        let prev = if i > 0 { parts[i - 1] } else { "" };

        // Pattern 1: pure digit segment after a trigger keyword
        let is_pure_digit = !part.is_empty() && part.chars().all(|c| c.is_ascii_digit());
        let prev_is_trigger = matches!(prev, "fn" | "tmp" | "frag");
        if is_pure_digit && prev_is_trigger {
            result.push("N".to_owned());
            continue;
        }

        // Pattern 2: segment ending in digits where the short prefix is `fn` or `l`
        if let Some(normalized) = normalize_counter_suffix(part) {
            result.push(normalized);
            continue;
        }

        result.push(part.to_owned());
    }

    result.join("_")
}

/// Replaces trailing digits on a `fn` or `l` prefixed segment with `N`.
///
/// Returns `None` if the segment is not a recognized counter-suffix pattern.
fn normalize_counter_suffix(segment: &str) -> Option<String> {
    // Find the index of the first character in the trailing digit run.
    let digit_start = segment
        .char_indices()
        .rev()
        .take_while(|(_, c)| c.is_ascii_digit())
        .last()
        .map(|(i, _)| i);

    let digit_start = digit_start?;

    // If the segment is entirely digits, it is a standalone counter — handled by pattern 1.
    if digit_start == 0 {
        return None;
    }

    let prefix = &segment[..digit_start];
    if matches!(prefix, "fn" | "l") {
        Some(format!("{prefix}N"))
    } else {
        None
    }
}

/// Validates rendered HTML/JS output against `contains`/`not_contains` fragments.
///
/// WHAT: extracts `<script>` blocks from `index.html`, runs them through a minimal
///       Node.js harness that stubs `document` and captures `console.log`, then
///       checks the combined runtime output against the supplied fragment lists.
/// WHY: for runtime-semantic fixtures the emitted JS structure is noise; what matters
///      is what the program actually does when executed.
fn validate_rendered_output(
    build_result: &BuildResult,
    contains: &[String],
    not_contains: &[String],
) -> Option<(String, FailureKind)> {
    let Some(index_html_file) = find_output_file(build_result, "index.html") else {
        return Some((
            "rendered_output assertion requires 'index.html', but it was not produced.".to_string(),
            FailureKind::HarnessFailed,
        ));
    };

    let Some(html) = output_text_content(index_html_file, ArtifactKind::Html) else {
        return Some((
            "rendered_output assertion requires 'index.html' to be an HTML artifact.".to_string(),
            FailureKind::HarnessFailed,
        ));
    };

    let rendered = match execute_html_in_node(html) {
        Ok(output) => output,
        Err(reason) => return Some((reason, FailureKind::HarnessFailed)),
    };

    let combined = combine_rendered_output(&rendered);

    for required in contains {
        if !combined.contains(required.as_str()) {
            return Some((
                format!(
                    "Rendered output did not contain required fragment '{required}'.\nActual output:\n{combined}"
                ),
                FailureKind::RenderedOutputMismatch,
            ));
        }
    }

    for forbidden in not_contains {
        if combined.contains(forbidden.as_str()) {
            return Some((
                format!(
                    "Rendered output contained forbidden fragment '{forbidden}'.\nActual output:\n{combined}"
                ),
                FailureKind::RenderedOutputMismatch,
            ));
        }
    }

    None
}

struct RenderedOutput {
    io_lines: Vec<String>,
    slot_outputs: Vec<SlotOutput>,
}

struct SlotOutput {
    #[allow(dead_code)]
    id: String,
    html: String,
}

fn combine_rendered_output(output: &RenderedOutput) -> String {
    let mut parts = output.io_lines.clone();
    for slot in &output.slot_outputs {
        parts.push(slot.html.clone());
    }
    parts.join("\n")
}

/// Executes the script blocks from compiled HTML through a minimal Node.js harness.
///
/// The harness stubs `document.getElementById` to capture `insertAdjacentHTML` calls,
/// intercepts `console.log`, and emits a JSON summary to stdout for parsing.
fn execute_html_in_node(html: &str) -> Result<RenderedOutput, String> {
    let scripts = extract_script_blocks(html);
    if scripts.is_empty() {
        return Err(
            "rendered_output: no <script> blocks found in 'index.html'. \
             Ensure the fixture produces runtime output."
                .to_string(),
        );
    }

    let harness = build_node_harness(&scripts);

    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0);
    let temp_path = std::env::temp_dir().join(format!("bst_render_harness_{unique}.js"));

    std::fs::write(&temp_path, &harness)
        .map_err(|e| format!("rendered_output: failed to write node harness: {e}"))?;

    let output = std::process::Command::new("node")
        .arg(&temp_path)
        .output()
        .map_err(|e| {
            let _ = std::fs::remove_file(&temp_path);
            format!(
                "rendered_output: failed to invoke node: {e}. \
                 Ensure 'node' is on PATH to use rendered_output_contains."
            )
        })?;

    let _ = std::fs::remove_file(&temp_path);

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "rendered_output: node harness execution failed:\n{stderr}"
        ));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_harness_output(stdout.trim())
}

fn build_node_harness(scripts: &[String]) -> String {
    let prefix = r#"const __bst_io = [];
const __bst_slots = [];
console.log = (...args) => __bst_io.push(args.map(String).join(' '));
const document = {
    getElementById: (id) => ({
        insertAdjacentHTML: (_, html) => __bst_slots.push({ id, html })
    })
};
"#;

    let suffix = r#"
process.stdout.write(JSON.stringify({ io: __bst_io, slots: __bst_slots }) + '\n');
"#;

    format!("{prefix}{}\n{suffix}", scripts.join("\n"))
}

/// Extracts the text content between `<script>` and `</script>` tag pairs.
fn extract_script_blocks(html: &str) -> Vec<String> {
    let mut blocks = Vec::new();
    let mut search_from = 0;

    while let Some(open_end) = find_script_open_end(html, search_from) {
        let close_tag = "</script>";
        let Some(close_start) = html[open_end..].find(close_tag) else {
            break;
        };
        let block = &html[open_end..open_end + close_start];
        if !block.trim().is_empty() {
            blocks.push(block.to_owned());
        }
        search_from = open_end + close_start + close_tag.len();
    }

    blocks
}

/// Finds the end position of a `<script>` opening tag starting from `from`.
fn find_script_open_end(html: &str, from: usize) -> Option<usize> {
    let slice = &html[from..];
    let tag_start = slice.find("<script")?;
    let tag_slice = &slice[tag_start..];
    let close_bracket = tag_slice.find('>')?;
    Some(from + tag_start + close_bracket + 1)
}

fn parse_harness_output(json: &str) -> Result<RenderedOutput, String> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        format!("rendered_output: failed to parse node harness JSON output: {e}\nRaw: {json}")
    })?;

    let io_lines = value["io"]
        .as_array()
        .ok_or("rendered_output: 'io' field missing from harness output")?
        .iter()
        .filter_map(|v| v.as_str().map(str::to_owned))
        .collect();

    let slot_outputs = value["slots"]
        .as_array()
        .ok_or("rendered_output: 'slots' field missing from harness output")?
        .iter()
        .filter_map(|v| {
            let id = v["id"].as_str()?.to_owned();
            let html = v["html"].as_str()?.to_owned();
            Some(SlotOutput { id, html })
        })
        .collect();

    Ok(RenderedOutput {
        io_lines,
        slot_outputs,
    })
}

fn collect_files_recursive(root: &Path) -> Vec<PathBuf> {
    let mut discovered = Vec::new();
    let mut queue = vec![root.to_path_buf()];

    while let Some(directory) = queue.pop() {
        let Ok(entries) = fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                queue.push(path);
                continue;
            }

            if path.is_file() {
                discovered.push(path);
            }
        }
    }

    discovered
}

fn contains_ordered_substrings(text: &str, substrings: &[String]) -> bool {
    let mut offset = 0usize;

    for substring in substrings {
        let Some(position) = text[offset..].find(substring) else {
            return false;
        };
        offset += position + substring.len();
    }

    true
}

fn count_occurrences(text: &str, needle: &str) -> usize {
    let mut count = 0;
    let mut offset = 0;

    while let Some(position) = text[offset..].find(needle) {
        count += 1;
        offset += position + needle.len();
    }

    count
}

/// Produces a compact unified-style diff between `expected` and `actual` text.
///
/// Shows at most `max_pairs` differing line pairs (- expected / + actual).
/// Truncates with a count of remaining differences if the limit is hit.
fn generate_text_diff(expected: &str, actual: &str, max_pairs: usize) -> String {
    let exp_lines: Vec<&str> = expected.lines().collect();
    let act_lines: Vec<&str> = actual.lines().collect();
    let max_len = exp_lines.len().max(act_lines.len());

    let mut diff_lines: Vec<String> = Vec::new();
    let mut extra = 0usize;

    for i in 0..max_len {
        let e = exp_lines.get(i).copied();
        let a = act_lines.get(i).copied();
        if e == a {
            continue;
        }
        if diff_lines.len() >= max_pairs * 2 {
            extra += 1;
            continue;
        }
        if let Some(line) = e {
            diff_lines.push(format!("- {line}"));
        }
        if let Some(line) = a {
            diff_lines.push(format!("+ {line}"));
        }
    }

    let mut out = format!("--- expected\n+++ actual\n{}", diff_lines.join("\n"));
    if extra > 0 {
        out.push_str(&format!("\n... ({extra} more differing lines)"));
    }
    out
}
