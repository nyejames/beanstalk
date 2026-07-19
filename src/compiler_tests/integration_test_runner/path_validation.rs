//! Lexical validation for paths authored in integration-test metadata.
//!
//! WHAT: rejects path syntax that can escape the owner selected by the loader.
//! WHY: the manifest and expectation loaders share the same relative-path safety contract while
//!      retaining their own contextual validation and filesystem ownership.

use std::path::{Component, Path};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum CurrentDirectoryRule {
    Forbid,
    AllowExactSentinel,
}

pub(super) fn validate_relative_path(
    raw_path: &str,
    field_name: &str,
    current_directory_rule: CurrentDirectoryRule,
) -> Result<(), String> {
    if raw_path.trim().is_empty() {
        return Err(format!("{field_name} must not be empty"));
    }
    if raw_path != raw_path.trim() {
        return Err(format!(
            "{field_name} '{raw_path}' must not have leading or trailing whitespace"
        ));
    }
    if current_directory_rule == CurrentDirectoryRule::AllowExactSentinel && raw_path == "." {
        return Ok(());
    }

    if has_portable_absolute_or_prefix(raw_path) {
        return Err(format!(
            "{field_name} '{raw_path}' must be a relative path without an absolute or platform-prefix component"
        ));
    }

    let path = Path::new(raw_path);
    for component in path.components() {
        let invalid_component = match component {
            Component::Prefix(_) => Some("platform-prefix"),
            Component::RootDir => Some("root"),
            Component::ParentDir => Some("parent"),
            Component::CurDir => Some("current-directory"),
            Component::Normal(_) => None,
        };
        if let Some(invalid_component) = invalid_component {
            return Err(format!(
                "{field_name} '{raw_path}' contains an authored {invalid_component} component"
            ));
        }
    }

    // On Unix, `Path` treats backslashes as ordinary filename characters. Inspect both common
    // separators so Windows-authored parent/current-directory components cannot become ordinary
    // filenames when a manifest is checked on another platform.
    for component in raw_path.split(['/', '\\']) {
        if component == ".." {
            return Err(format!(
                "{field_name} '{raw_path}' contains an authored parent component"
            ));
        }
        if component == "." {
            return Err(format!(
                "{field_name} '{raw_path}' contains an authored current-directory component"
            ));
        }
    }

    Ok(())
}

fn has_portable_absolute_or_prefix(raw_path: &str) -> bool {
    let bytes = raw_path.as_bytes();
    let has_drive_prefix = bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':';

    raw_path.starts_with('/') || raw_path.starts_with('\\') || has_drive_prefix
}
