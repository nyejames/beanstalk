//! Expectation file parsing for integration test cases.
//!
//! WHAT: reads and validates `expect.toml`, building typed expectation contracts per backend.
//! WHY: isolating TOML parsing here keeps fixture loading free of deserialization details and
//!      makes expectation format changes easy to find and update.

use super::{
    ArtifactAssertion, ArtifactKind, BackendId, ExpectationMode, GoldenMode,
    ParsedBackendExpectation, ParsedExpectationFile, WarningExpectation,
    normalize_relative_path_text,
};
use crate::backends::error_types::BackendErrorType;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_messages::compiler_errors::ErrorType;
use serde::Deserialize;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ExpectationToml {
    mode: Option<ExpectationMode>,
    entry: Option<String>,
    #[serde(default)]
    flags: Vec<String>,
    builder: Option<String>,
    warnings: Option<String>,
    warning_count: Option<usize>,
    error_type: Option<String>,
    #[serde(default)]
    message_contains: Vec<String>,
    #[serde(default)]
    artifact_assertions: Vec<ArtifactAssertionToml>,
    #[serde(default)]
    backends: BTreeMap<String, BackendExpectationToml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct BackendExpectationToml {
    mode: ExpectationMode,
    #[serde(default)]
    flags: Vec<String>,
    warnings: Option<String>,
    warning_count: Option<usize>,
    error_type: Option<String>,
    #[serde(default)]
    message_contains: Vec<String>,
    #[serde(default)]
    artifact_assertions: Vec<ArtifactAssertionToml>,
    golden_mode: Option<String>,
    #[serde(default)]
    rendered_output_contains: Vec<String>,
    #[serde(default)]
    rendered_output_not_contains: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactAssertionToml {
    path: String,
    kind: String,
    #[serde(default)]
    must_contain: Vec<String>,
    #[serde(default)]
    must_not_contain: Vec<String>,
    #[serde(default)]
    must_contain_in_order: Vec<String>,
    #[serde(default)]
    must_contain_exactly_once: Vec<String>,
    #[serde(default)]
    normalized_contains: Vec<String>,
    #[serde(default)]
    normalized_not_contains: Vec<String>,
    #[serde(default)]
    validate_wasm: bool,
    #[serde(default)]
    must_export: Vec<String>,
    #[serde(default)]
    must_import: Vec<String>,
}

pub(crate) fn parse_expectation_file(path: &Path) -> Result<ParsedExpectationFile, String> {
    let source = fs::read_to_string(path).map_err(|error| {
        format!(
            "Failed to read expectation file '{}': {error}",
            path.display()
        )
    })?;

    parse_expectation_source(&source, path)
}

/// Parse expectation TOML from an in-memory string.
///
/// WHAT: separates parsing from file I/O so the runner can supply a default stub
///       while still reporting errors against the canonical case path.
/// WHY: avoids duplicating boilerplate expect.toml files across cases.
pub(crate) fn parse_expectation_source(
    source: &str,
    display_path: &Path,
) -> Result<ParsedExpectationFile, String> {
    let parsed: ExpectationToml = toml::from_str(source).map_err(|error| {
        format!(
            "Failed to parse expectation file '{}' as TOML: {error}",
            display_path.display()
        )
    })?;

    if let Some(builder) = &parsed.builder
        && builder != "html"
    {
        return Err(format!(
            "Expectation file '{}' only supports builder = \"html\" right now",
            display_path.display()
        ));
    }

    if parsed.backends.is_empty() {
        return Err(format!(
            "Expectation file '{}' must declare at least one '[backends.<id>]' section. Legacy top-level mode/flags/error fields are no longer supported.",
            display_path.display()
        ));
    }

    parse_matrix_expectation_file(display_path, parsed)
}

fn parse_matrix_expectation_file(
    path: &Path,
    parsed: ExpectationToml,
) -> Result<ParsedExpectationFile, String> {
    // In matrix mode, all mode/outcome keys must be declared inside explicit
    // backend sections so each backend can evolve independently.
    if parsed.mode.is_some()
        || !parsed.flags.is_empty()
        || parsed.warnings.is_some()
        || parsed.warning_count.is_some()
        || parsed.error_type.is_some()
        || !parsed.message_contains.is_empty()
        || !parsed.artifact_assertions.is_empty()
    {
        return Err(format!(
            "Expectation file '{}' uses backend matrix mode and must keep mode/warnings/flags/error/artifact keys inside '[backends.<id>]'.",
            path.display()
        ));
    }

    let mut backend_expectations = Vec::new();
    for (backend_key, backend_expectation) in parsed.backends {
        let backend_id = BackendId::parse(&backend_key).map_err(|error| {
            format!(
                "Expectation file '{}' has invalid backend key '{}': {error}",
                path.display(),
                backend_key
            )
        })?;
        let context = format!("[backends.{}]", backend_id.as_str());
        let warnings = parse_warning_expectation(
            backend_expectation.warnings.as_deref(),
            backend_expectation.warning_count,
            path,
            &context,
        )?;
        let flags = parse_case_flags(&backend_expectation.flags, path, &context)?;
        let error_type = backend_expectation
            .error_type
            .as_deref()
            .map(parse_error_type)
            .transpose()?;

        let artifact_assertions =
            parse_artifact_assertions(path, &context, &backend_expectation.artifact_assertions)?;

        let golden_mode =
            parse_golden_mode(path, &context, backend_expectation.golden_mode.as_deref())?;

        // rendered_output_* is only valid for success mode; validate here so the
        // error message can reference the backend context.
        if backend_expectation.mode == ExpectationMode::Failure
            && (!backend_expectation.rendered_output_contains.is_empty()
                || !backend_expectation.rendered_output_not_contains.is_empty())
        {
            return Err(format!(
                "Expectation file '{}' {} uses mode = \"failure\" and must not set \
                 'rendered_output_contains' or 'rendered_output_not_contains'.",
                path.display(),
                context
            ));
        }

        validate_rendered_output_strings(
            path,
            &context,
            "rendered_output_contains",
            &backend_expectation.rendered_output_contains,
        )?;
        validate_rendered_output_strings(
            path,
            &context,
            "rendered_output_not_contains",
            &backend_expectation.rendered_output_not_contains,
        )?;

        backend_expectations.push(ParsedBackendExpectation {
            backend_id,
            flags,
            mode: backend_expectation.mode,
            warnings,
            error_type,
            message_contains: backend_expectation.message_contains,
            artifact_assertions,
            golden_mode,
            rendered_output_contains: backend_expectation.rendered_output_contains,
            rendered_output_not_contains: backend_expectation.rendered_output_not_contains,
        });
    }

    Ok(ParsedExpectationFile {
        entry: parsed.entry,
        backend_expectations,
    })
}

fn parse_artifact_assertions(
    path: &Path,
    context: &str,
    assertions: &[ArtifactAssertionToml],
) -> Result<Vec<ArtifactAssertion>, String> {
    let mut parsed_assertions = Vec::with_capacity(assertions.len());

    for (index, assertion) in assertions.iter().enumerate() {
        let assertion_label = artifact_assertion_label(context, index);

        if assertion.path.trim().is_empty() {
            return Err(format!(
                "Expectation file '{}' {} requires a non-empty 'path'.",
                path.display(),
                assertion_label
            ));
        }

        let kind = parse_artifact_kind(path, &assertion.kind, &assertion_label)?;
        validate_artifact_assertion_fields(path, &assertion_label, assertion)?;
        validate_artifact_assertion_shape(path, &assertion_label, kind, assertion)?;

        parsed_assertions.push(ArtifactAssertion {
            path: normalize_relative_path_text(&assertion.path),
            kind,
            must_contain: assertion.must_contain.clone(),
            must_not_contain: assertion.must_not_contain.clone(),
            must_contain_in_order: assertion.must_contain_in_order.clone(),
            must_contain_exactly_once: assertion.must_contain_exactly_once.clone(),
            normalized_contains: assertion.normalized_contains.clone(),
            normalized_not_contains: assertion.normalized_not_contains.clone(),
            validate_wasm: assertion.validate_wasm,
            must_export: assertion.must_export.clone(),
            must_import: assertion.must_import.clone(),
        });
    }

    Ok(parsed_assertions)
}

fn artifact_assertion_label(context: &str, index: usize) -> String {
    if context.is_empty() {
        format!("artifact_assertions[{index}]")
    } else {
        format!("{context}.artifact_assertions[{index}]")
    }
}

fn validate_artifact_assertion_fields(
    path: &Path,
    assertion_label: &str,
    assertion: &ArtifactAssertionToml,
) -> Result<(), String> {
    for (field_name, values) in [
        ("must_contain", &assertion.must_contain),
        ("must_not_contain", &assertion.must_not_contain),
        ("must_contain_in_order", &assertion.must_contain_in_order),
        (
            "must_contain_exactly_once",
            &assertion.must_contain_exactly_once,
        ),
        ("normalized_contains", &assertion.normalized_contains),
        (
            "normalized_not_contains",
            &assertion.normalized_not_contains,
        ),
        ("must_export", &assertion.must_export),
        ("must_import", &assertion.must_import),
    ] {
        validate_artifact_strings(path, assertion_label, field_name, values)?;
    }

    Ok(())
}

fn validate_artifact_assertion_shape(
    path: &Path,
    assertion_label: &str,
    kind: ArtifactKind,
    assertion: &ArtifactAssertionToml,
) -> Result<(), String> {
    match kind {
        ArtifactKind::Html | ArtifactKind::Js => {
            if assertion.validate_wasm
                || !assertion.must_export.is_empty()
                || !assertion.must_import.is_empty()
            {
                return Err(format!(
                    "Expectation file '{}' {} uses wasm-only fields on a text artifact assertion.",
                    path.display(),
                    assertion_label
                ));
            }
            if assertion.must_contain.is_empty()
                && assertion.must_not_contain.is_empty()
                && assertion.must_contain_in_order.is_empty()
                && assertion.must_contain_exactly_once.is_empty()
                && assertion.normalized_contains.is_empty()
                && assertion.normalized_not_contains.is_empty()
            {
                return Err(format!(
                    "Expectation file '{}' {} must define at least one of 'must_contain', \
                     'must_not_contain', 'must_contain_in_order', 'must_contain_exactly_once', \
                     'normalized_contains', or 'normalized_not_contains' for text artifacts.",
                    path.display(),
                    assertion_label
                ));
            }
        }
        ArtifactKind::Wasm => {
            if !assertion.must_contain.is_empty()
                || !assertion.must_not_contain.is_empty()
                || !assertion.must_contain_in_order.is_empty()
                || !assertion.must_contain_exactly_once.is_empty()
                || !assertion.normalized_contains.is_empty()
                || !assertion.normalized_not_contains.is_empty()
            {
                return Err(format!(
                    "Expectation file '{}' {} uses text-only fields on a wasm artifact assertion.",
                    path.display(),
                    assertion_label
                ));
            }
            if !assertion.validate_wasm
                && assertion.must_export.is_empty()
                && assertion.must_import.is_empty()
            {
                return Err(format!(
                    "Expectation file '{}' {} must enable 'validate_wasm' or require imports/exports for wasm assertions.",
                    path.display(),
                    assertion_label
                ));
            }
        }
        ArtifactKind::Binary => {
            if !assertion.must_contain.is_empty()
                || !assertion.must_not_contain.is_empty()
                || !assertion.must_contain_in_order.is_empty()
                || !assertion.must_contain_exactly_once.is_empty()
                || !assertion.normalized_contains.is_empty()
                || !assertion.normalized_not_contains.is_empty()
                || assertion.validate_wasm
                || !assertion.must_export.is_empty()
                || !assertion.must_import.is_empty()
            {
                return Err(format!(
                    "Expectation file '{}' {} uses text-only or wasm-only fields on a binary artifact assertion.",
                    path.display(),
                    assertion_label
                ));
            }
        }
    }

    Ok(())
}

fn validate_artifact_strings(
    path: &Path,
    assertion_label: &str,
    field_name: &str,
    values: &[String],
) -> Result<(), String> {
    for value in values {
        if value.is_empty() {
            return Err(format!(
                "Expectation file '{}' {} contains an empty '{}' value.",
                path.display(),
                assertion_label,
                field_name
            ));
        }
    }

    Ok(())
}

fn parse_golden_mode(path: &Path, context: &str, raw: Option<&str>) -> Result<GoldenMode, String> {
    match raw {
        None | Some("strict") => Ok(GoldenMode::Strict),
        Some("normalized") => Ok(GoldenMode::Normalized),
        Some(other) => Err(format!(
            "Expectation file '{}' {} has unsupported golden_mode '{other}'. \
             Supported values: \"strict\", \"normalized\".",
            path.display(),
            context
        )),
    }
}

fn validate_rendered_output_strings(
    path: &Path,
    context: &str,
    field_name: &str,
    values: &[String],
) -> Result<(), String> {
    for value in values {
        if value.is_empty() {
            return Err(format!(
                "Expectation file '{}' {} contains an empty '{field_name}' value.",
                path.display(),
                context
            ));
        }
    }
    Ok(())
}

fn parse_artifact_kind(
    path: &Path,
    raw_kind: &str,
    assertion_label: &str,
) -> Result<ArtifactKind, String> {
    match raw_kind {
        "html" => Ok(ArtifactKind::Html),
        "js" => Ok(ArtifactKind::Js),
        "wasm" => Ok(ArtifactKind::Wasm),
        "binary" => Ok(ArtifactKind::Binary),
        other => Err(format!(
            "Expectation file '{}' {} has unsupported artifact kind '{}'.",
            path.display(),
            assertion_label,
            other
        )),
    }
}

pub(crate) fn parse_warning_expectation(
    warnings_mode: Option<&str>,
    warning_count: Option<usize>,
    path: &Path,
    context: &str,
) -> Result<WarningExpectation, String> {
    let context_prefix = if context.is_empty() {
        String::new()
    } else {
        format!("{context} ")
    };

    let Some(mode) = warnings_mode else {
        return Err(format!(
            "Expectation file '{}' {}is missing required key 'warnings'.",
            path.display(),
            context_prefix
        ));
    };

    match mode {
        "ignore" => {
            if warning_count.is_some() {
                return Err(format!(
                    "Expectation file '{}' {}sets 'warning_count' but warnings != \"exact\".",
                    path.display(),
                    context_prefix
                ));
            }
            Ok(WarningExpectation::Ignore)
        }
        "forbid" => {
            if warning_count.is_some() {
                return Err(format!(
                    "Expectation file '{}' {}sets 'warning_count' but warnings != \"exact\".",
                    path.display(),
                    context_prefix
                ));
            }
            Ok(WarningExpectation::Forbid)
        }
        "exact" => {
            let expected_count = warning_count.ok_or_else(|| {
                format!(
                    "Expectation file '{}' {}uses warnings = \"exact\" but is missing 'warning_count'.",
                    path.display(),
                    context_prefix
                )
            })?;
            Ok(WarningExpectation::Exact(expected_count))
        }
        other => Err(format!(
            "Expectation file '{}' {}has unsupported warnings mode '{other}'.",
            path.display(),
            context_prefix
        )),
    }
}

pub(crate) fn parse_case_flags(
    flag_names: &[String],
    path: &Path,
    context: &str,
) -> Result<Vec<Flag>, String> {
    let mut flags = Vec::with_capacity(flag_names.len());
    for flag_name in flag_names {
        let parsed = match flag_name.as_str() {
            "release" => Flag::Release,
            "hide_warnings" => Flag::DisableWarnings,
            "hide_timers" => Flag::DisableTimers,
            "html_wasm" => Flag::HtmlWasm,
            other => {
                if context.is_empty() {
                    return Err(format!(
                        "Expectation file '{}' has unsupported flag '{}'.",
                        path.display(),
                        other
                    ));
                }
                return Err(format!(
                    "Expectation file '{}' {} has unsupported flag '{}'.",
                    path.display(),
                    context,
                    other
                ));
            }
        };
        flags.push(parsed);
    }
    Ok(flags)
}

pub(crate) fn parse_error_type(value: &str) -> Result<ErrorType, String> {
    let normalized = value.to_ascii_lowercase();
    match normalized.as_str() {
        "syntax" => Ok(ErrorType::Syntax),
        "type" => Ok(ErrorType::Type),
        "rule" => Ok(ErrorType::Rule),
        "file" => Ok(ErrorType::File),
        "config" => Ok(ErrorType::Config),
        "compiler" => Ok(ErrorType::Compiler),
        "devserver" | "dev_server" => Ok(ErrorType::DevServer),
        "borrowchecker" | "borrow_checker" => Ok(ErrorType::BorrowChecker),
        "hirtransformation" | "hir_transformation" => Ok(ErrorType::HirTransformation),
        "lirtransformation" | "lir_transformation" => {
            Ok(ErrorType::Backend(BackendErrorType::LirTransformation))
        }
        "wasmgeneration" | "wasm_generation" => {
            Ok(ErrorType::Backend(BackendErrorType::WasmGeneration))
        }
        other => Err(format!("Unsupported error type '{other}'")),
    }
}
