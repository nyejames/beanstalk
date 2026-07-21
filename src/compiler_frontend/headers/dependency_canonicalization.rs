//! Header dependency canonicalization.
//!
//! WHAT: rewrites raw import-spelled dependency edges into canonical resolved symbol paths.
//! WHY: dependency sorting compares exact header graph keys, so header dependencies must use the
//! same canonical paths that import preparation exposes through file visibility.

use crate::compiler_frontend::compiler_errors::compiler_error_to_diagnostic;
use crate::compiler_frontend::compiler_messages::DiagnosticBag;
use crate::compiler_frontend::headers::import_environment::HeaderImportEnvironment;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::headers::types::Header;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use rustc_hash::FxHashMap;

use std::collections::HashSet;

/// Rewrite raw import-path dependency edges into canonical resolved symbol paths.
pub(super) fn canonicalize_header_dependencies(
    headers: &mut [Header],
    import_environment: &HeaderImportEnvironment,
    file_imports_by_source: &FxHashMap<InternedPath, Vec<FileImport>>,
) -> Result<(), DiagnosticBag> {
    let mut diagnostic_bag = DiagnosticBag::new();

    for header in headers.iter_mut() {
        let visibility = match import_environment.visibility_for(&header.source_file) {
            Ok(visibility) => visibility,
            Err(error) => {
                diagnostic_bag.push(compiler_error_to_diagnostic(&error));
                continue;
            }
        };

        let file_imports = file_imports_by_source
            .get(&header.source_file)
            .map(|imports| imports.as_slice())
            .unwrap_or(&[]);

        let mut canonical: HashSet<InternedPath> =
            HashSet::with_capacity(header.dependencies.len());

        for dependency in header.dependencies.drain() {
            let matching_import = file_imports
                .iter()
                .find(|import| import.provider.path == dependency);

            if let Some(import) = matching_import {
                let local_name = match import.alias {
                    Some(alias) => alias,
                    None => match import.provider.path.name() {
                        Some(name) => name,
                        None => {
                            canonical.insert(dependency);
                            continue;
                        }
                    },
                };

                if let Some(resolved_path) = visibility
                    .visible_source_names
                    .get(&local_name)
                    .or_else(|| visibility.visible_type_alias_names.get(&local_name))
                {
                    canonical.insert(resolved_path.clone());
                }
                // External symbols and virtual packages have no header graph participant.
            } else {
                // Same-file or already-canonical edge: preserve it.
                canonical.insert(dependency);
            }
        }

        header.dependencies = canonical;
    }

    if diagnostic_bag.has_errors() {
        Err(diagnostic_bag)
    } else {
        Ok(())
    }
}
