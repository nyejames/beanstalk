//! Source-library facade preflight for Stage 0.
//!
//! WHAT: verifies every discovered source-library root exposes a `#mod.bst` facade.
//! WHY: source-library imports are consumed through facade exports during header parsing, so
//! Stage 0 should reject missing facades before frontend compilation starts.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::source_libraries::root_file::MOD_FILE_NAME;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use super::project_structure_diagnostics::{path_id, project_structure_messages};

/// Validate that every source-library root has the required facade file.
pub(super) fn validate_source_library_facades(
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    for (prefix, root) in resolver.source_library_roots() {
        let mod_file = root.join(MOD_FILE_NAME);

        if !mod_file.is_file() {
            return Err(project_structure_messages(
                root,
                InvalidConfigReason::SourceLibraryMissingFacade {
                    prefix: string_table.intern(prefix),
                    root: path_id(root, string_table),
                },
                string_table,
            ));
        }
    }

    Ok(())
}
