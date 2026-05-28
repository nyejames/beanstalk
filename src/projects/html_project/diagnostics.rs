//! Typed diagnostics for HTML-project policy checks.
//!
//! WHAT: turns deterministic routing and output-path policy failures into structured config
//! diagnostics.
//! WHY: HTML builder mistakes are user-facing project feedback, not infrastructure failures.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidConfigReason};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use std::path::Path;

pub(crate) fn missing_homepage_messages(
    config_path: &Path,
    entry_root: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    html_config_messages(
        config_path,
        |string_table| InvalidConfigReason::MissingHtmlHomepage {
            entry_root: path_id(entry_root, string_table),
        },
        string_table,
    )
}

pub(crate) fn duplicate_html_output_path_messages(
    duplicate_entry_point: &Path,
    existing_entry_point: &Path,
    output_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    html_config_messages(
        duplicate_entry_point,
        |string_table| InvalidConfigReason::DuplicateHtmlOutputPath {
            output_path: path_id(output_path, string_table),
            entry_point: path_id(duplicate_entry_point, string_table),
            existing_entry_point: path_id(existing_entry_point, string_table),
        },
        string_table,
    )
}

pub(crate) fn tracked_asset_output_conflict_messages(
    asset_path: &Path,
    existing_owner: &Path,
    output_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    html_config_messages(
        asset_path,
        |string_table| InvalidConfigReason::TrackedAssetOutputConflict {
            asset_path: path_id(asset_path, string_table),
            output_path: path_id(output_path, string_table),
            existing_owner: path_id(existing_owner, string_table),
        },
        string_table,
    )
}

pub(crate) fn tracked_asset_builder_output_conflict_messages(
    asset_path: &Path,
    output_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    html_config_messages(
        asset_path,
        |string_table| InvalidConfigReason::TrackedAssetBuilderOutputConflict {
            asset_path: path_id(asset_path, string_table),
            output_path: path_id(output_path, string_table),
        },
        string_table,
    )
}

fn html_config_messages(
    location_path: &Path,
    reason: impl FnOnce(&mut StringTable) -> InvalidConfigReason,
    string_table: &mut StringTable,
) -> CompilerMessages {
    let location = SourceLocation::from_path(location_path, string_table);
    let diagnostic =
        CompilerDiagnostic::invalid_config_reason(None, reason(string_table), location);

    CompilerMessages::from_diagnostic_ref(diagnostic, string_table)
}

fn path_id(path: &Path, string_table: &mut StringTable) -> StringId {
    string_table.get_or_intern(path.display().to_string())
}
