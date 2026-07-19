//! Manifest file parsing for the integration test suite.
//!
//! WHAT: reads and validates `manifest.toml`, which declares canonical case order and metadata.
//! WHY: the manifest is the authoritative source for test execution order, so parsing it is
//!      isolated here to keep the fixture loader free of TOML deserialization details.

use super::{CaseRole, ManifestCaseSpec};
use serde::Deserialize;
use std::collections::{HashMap, HashSet};
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
    #[serde(default)]
    pub contract: Option<String>,
    #[serde(default)]
    pub role: Option<String>,
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
    let mut seen_primary_contracts = HashMap::<String, String>::new();
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

        if case
            .contract
            .as_deref()
            .is_some_and(|contract| contract.trim().is_empty())
        {
            return Err(format!(
                "Manifest '{}' case '{}' has an empty contract value.",
                path.display(),
                case.id
            ));
        }

        let role = case
            .role
            .as_deref()
            .map(CaseRole::parse)
            .transpose()
            .map_err(|error| {
                format!(
                    "Manifest '{}' case '{}' has an invalid role: {error}.",
                    path.display(),
                    case.id
                )
            })?;

        if role == Some(CaseRole::Primary) && case.contract.is_none() {
            return Err(format!(
                "Manifest '{}' case '{}' has role 'primary' but is missing a contract.",
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

        if let (Some(CaseRole::Primary), Some(contract)) = (role, case.contract.as_ref())
            && let Some(previous_case_id) =
                seen_primary_contracts.insert(contract.clone(), case.id.clone())
        {
            return Err(format!(
                "Manifest '{}' has duplicate primary contract '{}' on cases '{}' and '{}'.",
                path.display(),
                contract,
                previous_case_id,
                case.id
            ));
        }

        cases.push(ManifestCaseSpec {
            id: case.id,
            path: PathBuf::from(case.path),
            tags: case.tags,
            contract: case.contract,
            role,
        });
    }

    Ok(cases)
}
