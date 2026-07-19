//! Recursive golden-file discovery for integration expectations.
//!
//! WHAT: owns the filesystem inventory that determines whether a backend has a golden contract.
//! WHY: fixture validation and comparison must consume the same deterministic file set.

use super::super::FailureKind;
use super::super::types::{GoldenExpectation, GoldenFile, GoldenFileInventory, GoldenMode};
use crate::build_system::build::{BuildResult, FileKind};
use std::fs;
use std::path::Path;

/// Discovers one backend's golden files and resolves its effective comparison mode.
pub(crate) fn discover_golden_expectation(
    golden_dir: &Path,
    authored_mode: Option<GoldenMode>,
) -> Result<GoldenExpectation, String> {
    let inventory = discover_golden_files(golden_dir)?;

    if inventory.is_empty() {
        if let Some(mode) = authored_mode {
            return Err(format!(
                "Golden directory '{}' has golden_mode = \"{}\" but contains no golden files.",
                golden_dir.display(),
                golden_mode_label(mode)
            ));
        }

        return Ok(GoldenExpectation {
            inventory,
            mode: None,
        });
    }

    Ok(GoldenExpectation {
        inventory,
        mode: Some(authored_mode.unwrap_or(GoldenMode::Strict)),
    })
}

/// Recursively inventories actual files under a backend golden directory.
///
/// WHAT: returns relative paths in deterministic order and preserves filesystem failures.
/// WHY: directories alone are not golden contracts, while unreadable golden state must not be
///      silently treated as absent.
fn discover_golden_files(root: &Path) -> Result<GoldenFileInventory, String> {
    match fs::metadata(root) {
        Ok(metadata) if !metadata.is_dir() => {
            return Err(format!(
                "Golden path '{}' exists but is not a directory.",
                root.display()
            ));
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(GoldenFileInventory::default());
        }
        Err(error) => {
            return Err(format!(
                "Failed to inspect golden directory '{}': {error}",
                root.display()
            ));
        }
    }

    let mut files = Vec::new();
    visit_golden_directory(root, root, &mut files)?;
    files.sort_by(|left, right| left.relative_path.cmp(&right.relative_path));

    Ok(GoldenFileInventory { files })
}

fn visit_golden_directory(
    directory: &Path,
    root: &Path,
    files: &mut Vec<GoldenFile>,
) -> Result<(), String> {
    let entries = fs::read_dir(directory).map_err(|error| {
        format!(
            "Failed to read golden directory '{}': {error}",
            directory.display()
        )
    })?;

    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| {
            format!(
                "Failed to read an entry in golden directory '{}': {error}",
                directory.display()
            )
        })?;
        paths.push(entry.path());
    }
    paths.sort();

    for path in paths {
        let metadata = fs::metadata(&path).map_err(|error| {
            format!(
                "Failed to inspect golden entry '{}': {error}",
                path.display()
            )
        })?;

        if metadata.is_dir() {
            visit_golden_directory(&path, root, files)?;
            continue;
        }

        if !metadata.is_file() {
            continue;
        }

        let relative_path = path.strip_prefix(root).map_err(|error| {
            format!(
                "Failed to make golden entry '{}' relative to '{}': {error}",
                path.display(),
                root.display()
            )
        })?;
        files.push(GoldenFile {
            relative_path: relative_path.to_string_lossy().replace('\\', "/"),
            absolute_path: path,
        });
    }

    Ok(())
}

fn golden_mode_label(mode: GoldenMode) -> &'static str {
    match mode {
        GoldenMode::Strict => "strict",
        GoldenMode::Normalized => "normalized",
    }
}

pub(super) fn validate_golden_outputs(
    build_result: &BuildResult,
    golden: &GoldenExpectation,
) -> Option<(String, FailureKind)> {
    if golden.inventory.is_empty() {
        return None;
    }

    let Some(mode) = golden.mode else {
        return Some((
            "Golden files were discovered without an effective golden mode.".to_owned(),
            FailureKind::HarnessFailed,
        ));
    };

    let expected_paths = golden
        .inventory
        .files
        .iter()
        .map(|file| file.relative_path.clone())
        .collect::<Vec<_>>();

    if let Some(reason) = validate_expected_artifact_paths(build_result, &expected_paths) {
        return Some((reason, FailureKind::StrictGoldenMismatch));
    }

    for file in &golden.inventory.files {
        let relative = &file.relative_path;

        let Some(output) = super::find_output_file(build_result, relative) else {
            return Some((
                format!("Golden output '{relative}' was not produced."),
                FailureKind::StrictGoldenMismatch,
            ));
        };

        let expected_bytes = match fs::read(&file.absolute_path) {
            Ok(bytes) => bytes,
            Err(error) => {
                return Some((
                    format!(
                        "Failed to read golden output '{}': {error}",
                        file.absolute_path.display()
                    ),
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
        if is_text {
            let expected_str = String::from_utf8_lossy(&expected_bytes);
            let actual_str = match output.file_kind() {
                FileKind::Html(s) | FileKind::Js(s) => s.as_str(),
                _ => unreachable!("is_text is true"),
            };

            if let Some(detail) = compare_text_golden(expected_str.as_ref(), actual_str, mode) {
                let failure_kind = if mode == GoldenMode::Normalized {
                    FailureKind::NormalizedSemanticMismatch
                } else {
                    FailureKind::StrictGoldenMismatch
                };
                let context = if mode == GoldenMode::Normalized {
                    "did not match after normalization"
                } else {
                    "did not match the produced artifact"
                };
                return Some((
                    format!("Golden output '{relative}' {context}.\n{detail}"),
                    failure_kind,
                ));
            }
            continue;
        }

        if actual_bytes != expected_bytes {
            let detail = format!(
                "expected {} bytes, got {} bytes",
                expected_bytes.len(),
                actual_bytes.len()
            );
            return Some((
                format!(
                    "Golden output '{relative}' did not match the produced artifact ({detail})."
                ),
                FailureKind::StrictGoldenMismatch,
            ));
        }
    }

    None
}

fn validate_expected_artifact_paths(
    build_result: &BuildResult,
    expected_paths: &[String],
) -> Option<String> {
    let actual_paths = super::collect_built_artifact_paths(build_result);

    let mut expected = expected_paths
        .iter()
        .map(|path| super::normalize_relative_path_text(path))
        .collect::<Vec<_>>();
    expected.sort();

    if actual_paths != expected {
        return Some(format!(
            "Expected output paths {expected:?}, but produced {actual_paths:?}."
        ));
    }

    None
}

pub(super) fn compare_text_golden(
    expected: &str,
    actual: &str,
    mode: GoldenMode,
) -> Option<String> {
    let normalized_expected = super::normalize_text_line_endings(expected);
    let normalized_actual = super::normalize_text_line_endings(actual);

    match mode {
        GoldenMode::Strict => {
            if normalized_expected == normalized_actual {
                return None;
            }
            Some(generate_text_diff(
                &normalized_expected,
                &normalized_actual,
                8,
            ))
        }
        GoldenMode::Normalized => {
            let semantic_expected = super::normalize_text_for_comparison(&normalized_expected);
            let semantic_actual = super::normalize_text_for_comparison(&normalized_actual);
            if semantic_expected == semantic_actual {
                return None;
            }
            Some(generate_text_diff(&semantic_expected, &semantic_actual, 8))
        }
    }
}

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
