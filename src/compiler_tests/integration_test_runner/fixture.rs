//! Fixture discovery and loading for the integration test suite.
//!
//! WHAT: locates canonical case directories, validates the manifest, and builds typed
//!       `TestCaseSpec` values ready for execution.
//! WHY: keeping fixture loading separate from expectation parsing and case execution gives
//!      each piece a single clear responsibility.

use super::types::SuccessContract;
use super::{
    BackendId, CANONICAL_TESTS_PATH, DEFAULT_EXPECT_STUB_PATH, EXPECT_FILE_NAME, ExpectationMode,
    ExpectedOutcome, FailureExpectation, GOLDEN_DIR_NAME, INPUT_DIR_NAME, MANIFEST_FILE_NAME,
    ManifestCaseSpec, ParsedExpectationFile, SuccessExpectation, TestCaseSpec, TestSuiteSpec,
};
use crate::compiler_frontend::Flag;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn load_test_suite() -> Result<TestSuiteSpec, String> {
    load_test_suite_from_root(Path::new(CANONICAL_TESTS_PATH))
}

pub(crate) fn load_test_suite_from_root(root: &Path) -> Result<TestSuiteSpec, String> {
    let mut cases = Vec::new();
    let manifest_path = root.join(MANIFEST_FILE_NAME);
    if !manifest_path.is_file() {
        return Err(format!(
            "Canonical integration root '{}' must define '{}'.",
            root.display(),
            MANIFEST_FILE_NAME
        ));
    }

    let manifest_cases = super::manifest::parse_manifest_file(&manifest_path)?;
    validate_manifest_authoritativeness(root, &manifest_cases)?;

    for manifest_case in manifest_cases {
        let fixture_root = root.join(&manifest_case.path);
        let case_specs = load_canonical_case_specs(&fixture_root, Some(manifest_case))?;
        cases.extend(case_specs);
    }

    Ok(TestSuiteSpec { cases })
}

fn validate_manifest_authoritativeness(
    root: &Path,
    manifest_cases: &[ManifestCaseSpec],
) -> Result<(), String> {
    let declared_paths = manifest_cases
        .iter()
        .map(|case| {
            fs::canonicalize(root.join(&case.path)).unwrap_or_else(|_| root.join(&case.path))
        })
        .collect::<HashSet<_>>();

    let discovered_roots = discover_canonical_fixture_roots(root)?;
    let mut undeclared_fixtures = Vec::new();
    for discovered_root in discovered_roots {
        let canonical_discovered =
            fs::canonicalize(&discovered_root).unwrap_or_else(|_| discovered_root.clone());
        if !declared_paths.contains(&canonical_discovered) {
            undeclared_fixtures.push(discovered_root);
        }
    }

    if !undeclared_fixtures.is_empty() {
        undeclared_fixtures.sort();
        let preview = undeclared_fixtures
            .iter()
            .take(6)
            .map(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("unknown_case")
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join(", ");
        return Err(format!(
            "Manifest '{}' must list every canonical case; found undeclared fixtures: {preview}.",
            root.join(MANIFEST_FILE_NAME).display()
        ));
    }

    Ok(())
}

fn discover_canonical_fixture_roots(root: &Path) -> Result<Vec<PathBuf>, String> {
    let entries = fs::read_dir(root).map_err(|error| {
        format!(
            "Failed to read canonical test root '{}': {error}",
            root.display()
        )
    })?;

    let mut discovered_dirs = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| format!("Failed to read test entry: {error}"))?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
            continue;
        };

        if matches!(name, "success" | "failure") {
            continue;
        }

        if !path.join(INPUT_DIR_NAME).is_dir() {
            continue;
        }

        discovered_dirs.push(path);
    }

    discovered_dirs.sort();
    Ok(discovered_dirs)
}

pub(crate) fn load_canonical_case_specs(
    fixture_root: &Path,
    manifest_case: Option<ManifestCaseSpec>,
) -> Result<Vec<TestCaseSpec>, String> {
    let input_root = fixture_root.join(INPUT_DIR_NAME);
    let expect_path = fixture_root.join(EXPECT_FILE_NAME);

    if !input_root.is_dir() {
        return Err(format!(
            "Canonical fixture '{}' is missing '{}'",
            fixture_root.display(),
            INPUT_DIR_NAME
        ));
    }

    let parsed_expectation = if expect_path.is_file() {
        super::expectations::parse_expectation_file(&expect_path)?
    } else {
        let default_stub_path = Path::new(DEFAULT_EXPECT_STUB_PATH);
        let source = fs::read_to_string(default_stub_path).map_err(|error| {
            format!(
                "Failed to read default expectation stub '{}': {error}",
                default_stub_path.display()
            )
        })?;
        super::expectations::parse_expectation_source(&source, &expect_path)?
    };
    validate_fixture_contract(fixture_root, &parsed_expectation)?;
    let entry_path = resolve_case_entry_path(&input_root, parsed_expectation.entry.as_deref())?;
    let manifest_relative_path = manifest_case
        .as_ref()
        .map(|case| normalize_manifest_relative_path(&case.path))
        .unwrap_or_else(|| {
            fixture_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unnamed_case")
                .to_owned()
        });
    let (case_id, tags, contract, role) = match manifest_case {
        Some(manifest_case) => (
            manifest_case.id,
            manifest_case.tags,
            manifest_case.contract,
            manifest_case.role,
        ),
        None => (
            fixture_root
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("unnamed_case")
                .to_string(),
            Vec::new(),
            None,
            None,
        ),
    };

    let mut case_specs = Vec::new();
    for backend_expectation in parsed_expectation.backend_expectations {
        let golden_dir = golden_dir_for_backend(fixture_root, backend_expectation.backend_id);
        let expected = match backend_expectation.mode {
            ExpectationMode::Success => ExpectedOutcome::Success(SuccessExpectation {
                warnings: backend_expectation.warnings,
                success_contract: backend_expectation.success_contract,
                artifact_assertions: backend_expectation.artifact_assertions,
                golden_mode: backend_expectation.golden_mode,
                has_golden: golden_dir_has_files(&golden_dir),
                rendered_output_contains: backend_expectation.rendered_output_contains,
                rendered_output_not_contains: backend_expectation.rendered_output_not_contains,
                artifacts_must_not_exist: backend_expectation.artifacts_must_not_exist,
            }),
            ExpectationMode::Failure => ExpectedOutcome::Failure(FailureExpectation {
                warnings: backend_expectation.warnings,
                message_contains: backend_expectation.message_contains,
                diagnostic_codes: backend_expectation.diagnostic_codes,
            }),
        };

        let flags = merge_flags(
            backend_expectation.backend_id.default_flags(),
            backend_expectation.flags,
        );
        let backend_name = backend_expectation.backend_id.as_str();

        case_specs.push(TestCaseSpec {
            display_name: format!("{case_id} [{backend_name}]"),
            case_id: case_id.clone(),
            manifest_relative_path: manifest_relative_path.clone(),
            tags: tags.clone(),
            contract: contract.clone(),
            role,
            backend_id: backend_expectation.backend_id,
            entry_path: entry_path.clone(),
            golden_dir,
            flags,
            expected,
        });
    }

    Ok(case_specs)
}

fn normalize_manifest_relative_path(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
}

fn merge_flags(default_flags: Vec<Flag>, extra_flags: Vec<Flag>) -> Vec<Flag> {
    // Default backend flags establish the runtime mode, while fixture flags
    // can layer additional toggles without duplicating the same flag value.
    let mut merged = default_flags;
    for flag in extra_flags {
        if !merged.contains(&flag) {
            merged.push(flag);
        }
    }

    merged
}

fn validate_fixture_contract(
    fixture_root: &Path,
    expectation: &ParsedExpectationFile,
) -> Result<(), String> {
    if expectation.backend_expectations.is_empty() {
        return Err(format!(
            "Fixture '{}' does not define any backend expectations.",
            fixture_root.display()
        ));
    }

    for backend_expectation in &expectation.backend_expectations {
        let golden_dir = golden_dir_for_backend(fixture_root, backend_expectation.backend_id);
        let has_golden_dir = golden_dir_has_files(&golden_dir);
        let has_artifact_assertions = !backend_expectation.artifact_assertions.is_empty();
        let has_backend_baseline_contract =
            backend_has_builtin_success_contract(backend_expectation.backend_id);

        match backend_expectation.mode {
            ExpectationMode::Failure => {
                if backend_expectation.diagnostic_codes.is_empty() {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"failure\" but is missing required 'diagnostic_codes'.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
                if has_artifact_assertions {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"failure\" and must not define artifact assertions.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
            }
            ExpectationMode::Success => {
                if backend_expectation.success_contract == Some(SuccessContract::CompileOnly)
                    && has_golden_dir
                {
                    return Err(format!(
                        "Fixture '{}' backend '{}' declares success_contract = \"compile_only\" but has golden artifacts in '{}'.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str(),
                        golden_dir.display()
                    ));
                }

                let has_rendered_output = !backend_expectation.rendered_output_contains.is_empty()
                    || !backend_expectation.rendered_output_not_contains.is_empty();
                if !has_golden_dir
                    && !has_artifact_assertions
                    && !has_backend_baseline_contract
                    && !has_rendered_output
                {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"success\" and must provide \
                         artifact assertions, a '{}' directory, or 'rendered_output_contains'.",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str(),
                        golden_dir.display()
                    ));
                }
                if !backend_expectation.message_contains.is_empty()
                    || !backend_expectation.diagnostic_codes.is_empty()
                {
                    return Err(format!(
                        "Fixture '{}' backend '{}' uses mode = \"success\" and must not set failure-only keys ('diagnostic_codes'/'message_contains').",
                        fixture_root.display(),
                        backend_expectation.backend_id.as_str()
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Declares whether a backend always has an implicit success contract.
///
/// WHAT: marks backends that always enforce baseline artifact checks.
/// WHY: keeps fixture validation permissive while still guaranteeing minimum output checks.
fn backend_has_builtin_success_contract(backend_id: BackendId) -> bool {
    matches!(backend_id, BackendId::Html | BackendId::HtmlWasm)
}

fn resolve_case_entry_path(
    input_root: &Path,
    configured_entry: Option<&str>,
) -> Result<PathBuf, String> {
    if let Some(entry) = configured_entry {
        if entry == "." {
            return Ok(input_root.to_path_buf());
        }

        return Ok(input_root.join(entry));
    }

    let default_entry = input_root.join("#page.bst");
    if default_entry.is_file() {
        return Ok(default_entry);
    }

    Err(format!(
        "Could not determine canonical test entry for '{}'. Add 'entry = ...' to '{}' or provide #page.bst.",
        input_root.display(),
        EXPECT_FILE_NAME
    ))
}

/// Resolves backend-scoped golden directories for fixture assertions.
///
/// WHAT: maps each backend execution to `golden/<backend>/...`.
/// WHY: keeps artifact snapshots backend-specific even for non-matrix fixtures.
pub(crate) fn golden_dir_for_backend(fixture_root: &Path, backend_id: BackendId) -> PathBuf {
    fixture_root.join(GOLDEN_DIR_NAME).join(backend_id.as_str())
}

fn golden_dir_has_files(golden_dir: &Path) -> bool {
    std::fs::read_dir(golden_dir)
        .ok()
        .is_some_and(|mut entries| entries.next().is_some())
}
