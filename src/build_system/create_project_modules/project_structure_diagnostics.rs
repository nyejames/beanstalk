//! Stage 0 project-structure diagnostic helpers.
//!
//! WHAT: builds typed config/project-structure diagnostics at the input-preparation boundary.
//! WHY: resolver setup, library discovery, and module inventory all report the same
//! `InvalidConfigReason` payloads, so the location and string-table rules belong in one small
//! Stage 0 owner instead of being duplicated across those modules.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, InvalidConfigReason};
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::projects::settings::Config;

use std::path::Path;

/// Build an invalid-config diagnostic tied to a specific config key.
pub(super) fn config_diagnostic_messages(
    config: &Config,
    key: &str,
    reason: InvalidConfigReason,
    string_table: &mut StringTable,
) -> CompilerMessages {
    // Stage 0 can run after config parsing with a boundary-owned StringTable. Use a fresh
    // file-level location here so diagnostics never carry SourceLocation IDs from another table.
    let key_id = string_table.intern(key);
    let location = SourceLocation::from_path(&config.config_file_path(), string_table);
    let diagnostic = CompilerDiagnostic::invalid_config_reason(Some(key_id), reason, location);

    CompilerMessages::from_diagnostic_ref(diagnostic, string_table)
}

/// Build an invalid-project-structure diagnostic tied to the offending filesystem path.
pub(super) fn project_structure_messages(
    location_path: &Path,
    reason: InvalidConfigReason,
    string_table: &mut StringTable,
) -> CompilerMessages {
    let location = SourceLocation::from_path(location_path, string_table);
    let diagnostic = CompilerDiagnostic::invalid_config_reason(None, reason, location);

    CompilerMessages::from_diagnostic_ref(diagnostic, string_table)
}

/// Intern a path spelling for diagnostic payloads.
pub(super) fn path_id(path: &Path, string_table: &mut StringTable) -> StringId {
    string_table.get_or_intern(path.display().to_string())
}
