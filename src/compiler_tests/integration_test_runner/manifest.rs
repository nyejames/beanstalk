//! Manifest file parsing for the integration test suite.
//!
//! WHAT: reads and validates `manifest.toml`, which declares the canonical case order and tags.
//! WHY: the manifest is the authoritative source for test execution order, so parsing it is
//!      isolated here to keep the fixture loader free of TOML deserialization details.

use super::ManifestCaseSpec;
use serde::Deserialize;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestToml {
    #[serde(default)]
    pub case: Vec<ManifestCaseToml>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ManifestCaseToml {
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub tags: Vec<String>,
}

pub(crate) fn parse_manifest_file(path: &Path) -> Result<Vec<ManifestCaseSpec>, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("Failed to read manifest '{}': {error}", path.display()))?;

    let parsed: ManifestToml = toml::from_str(&source).map_err(|error| {
        format!(
            "Failed to parse manifest '{}' as TOML: {error}",
            path.display()
        )
    })?;

    let mut seen_ids = HashSet::new();
    let mut seen_paths = HashSet::new();
    let mut cases = Vec::with_capacity(parsed.case.len());
    for case in parsed.case {
        if case.id.trim().is_empty() {
            return Err(format!(
                "Manifest '{}' has a case with an empty id",
                path.display()
            ));
        }
        if case.path.trim().is_empty() {
            return Err(format!(
                "Manifest '{}' has a case with an empty path",
                path.display()
            ));
        }
        if case.tags.is_empty() {
            return Err(format!(
                "Manifest '{}' case '{}' is missing required tags.",
                path.display(),
                case.id
            ));
        }
        if case.tags.iter().any(|tag| tag.trim().is_empty()) {
            return Err(format!(
                "Manifest '{}' case '{}' has an empty tag value.",
                path.display(),
                case.id
            ));
        }
        if !seen_ids.insert(case.id.clone()) {
            return Err(format!(
                "Manifest '{}' has duplicate case id '{}'.",
                path.display(),
                case.id
            ));
        }
        if !seen_paths.insert(case.path.clone()) {
            return Err(format!(
                "Manifest '{}' has duplicate case path '{}'.",
                path.display(),
                case.path
            ));
        }

        cases.push(ManifestCaseSpec {
            id: case.id,
            path: PathBuf::from(case.path),
        });
    }

    Ok(cases)
}
