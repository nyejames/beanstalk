//! Source-backed package root preflight for Stage 0.
//!
//! WHAT: verifies every discovered source-backed package root contains exactly one generic hash root.
//! WHY: source-backed package imports are consumed through the prepared root public surface during header
//! parsing, so Stage 0 should reject missing or ambiguous roots before frontend compilation.

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::InvalidConfigReason;
use crate::compiler_frontend::source_packages::root_file::HashRootFileDiscovery;
use crate::compiler_frontend::source_packages::root_file::PreparedSourcePackageRoots;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use super::project_structure_diagnostics::{path_id, project_structure_messages};

/// Validate that every source-backed package root has exactly one direct-child hash root file.
pub(crate) fn validate_source_package_roots(
    prepared_roots: &PreparedSourcePackageRoots,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    for (prefix, root) in prepared_roots.roots() {
        let discovery = prepared_roots
            .root_files()
            .get(prefix)
            .unwrap_or(&HashRootFileDiscovery::Missing);

        match discovery {
            HashRootFileDiscovery::Missing => {
                return Err(project_structure_messages(
                    root,
                    InvalidConfigReason::SourcePackageMissingRoot {
                        prefix: string_table.intern(prefix),
                        root: path_id(root, string_table),
                    },
                    string_table,
                ));
            }

            HashRootFileDiscovery::Multiple(candidates) => {
                let candidates = candidates
                    .iter()
                    .map(|candidate| path_id(candidate, string_table))
                    .collect();
                return Err(project_structure_messages(
                    root,
                    InvalidConfigReason::SourcePackageMultipleRoots {
                        prefix: string_table.intern(prefix),
                        root: path_id(root, string_table),
                        candidates,
                    },
                    string_table,
                ));
            }

            HashRootFileDiscovery::Unique(_) => {}

            HashRootFileDiscovery::Unreadable(error) => {
                return Err(CompilerMessages::file_error(
                    root,
                    format!("Failed to inspect source-backed package root: {error}"),
                    string_table,
                ));
            }
        }
    }

    Ok(())
}
