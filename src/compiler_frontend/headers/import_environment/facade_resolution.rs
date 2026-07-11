//! Header-stage facade import resolution.
//!
//! WHAT: resolves cross-library and cross-module-root imports through the target module's
//! facade exports.
//! WHY: source-library modules and regular module roots expose symbols only through their
//! `#mod.bst` facade; external importers cannot bypass it to import internal symbols.
//! MUST NOT: perform general import target resolution (that belongs in `target_resolution.rs`).

use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, ImportFacadeType};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::headers::import_environment::target_resolution::suffix_matches_with_optional_source_extension;
use crate::compiler_frontend::headers::module_symbols::{FacadeExportEntry, FacadeExportTarget};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::{FxHashMap, FxHashSet};

/// Boxed diagnostic result for facade boundary checks.
///
/// WHAT: gives source-library and module privacy checks one small error boundary.
/// WHY: both callers already propagate boxed diagnostics, so the checks can preserve
///      structured errors without unboxing and reboxing them.
type BoundaryCheckResult<T> = Result<T, Box<CompilerDiagnostic>>;

/// Result of looking up an import path against a module facade.
pub(crate) enum FacadeLookupResult {
    /// This import does not target a facade; use normal file-based resolution.
    NotAFacadeImport,
    /// Facade exports a source symbol with this canonical path and public surface.
    ExportedSource {
        path: InternedPath,
        exported_entries: FxHashSet<FacadeExportEntry>,
    },
    /// Facade exports an external package symbol.
    ExportedExternal { symbol_id: ExternalSymbolId },
    /// Import targets a facade but the requested symbol is not exported.
    NotExported {
        facade_name: String,
        facade_type: FacadeType,
    },
}

/// Classification of facade for diagnostic messages.
pub(crate) enum FacadeType {
    SourceLibrary,
    ModuleRoot,
}

/// Input bundle for facade resolution.
///
/// WHY: facade resolution needs both the import path and the module's facade metadata.
pub(crate) struct FacadeResolutionInput<'a> {
    pub(crate) importer_file: &'a InternedPath,
    pub(crate) header_path: &'a InternedPath,
    pub(crate) facade_exports: &'a FxHashMap<String, FxHashSet<FacadeExportEntry>>,
    pub(crate) file_library_membership: &'a FxHashMap<InternedPath, String>,
    pub(crate) module_root_facade_exports:
        &'a FxHashMap<InternedPath, FxHashSet<FacadeExportEntry>>,
    pub(crate) file_module_membership: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) module_root_prefixes: &'a [(InternedPath, InternedPath)],
    pub(crate) string_table: &'a StringTable,
}

/// Attempt to resolve an import through a source-library or module-root facade.
///
/// WHAT: checks whether the import path starts with a known library prefix or module root,
/// and whether the importer is outside that module. If so, looks up the symbol name in the
/// facade's exported entries.
pub(crate) fn resolve_facade_import(
    input: &FacadeResolutionInput<'_>,
) -> Option<FacadeLookupResult> {
    // Try cross-library facade resolution first.
    if let Some(result) = try_resolve_library_facade_import(input) {
        return Some(result);
    }

    // Try cross-module-root facade resolution.
    try_resolve_module_root_facade_import(input)
}

/// Cross-library facade lookup.
///
/// WHAT: when an import path starts with a library prefix and the importer is outside that
/// library, the symbol must be exported by the module facade.
fn try_resolve_library_facade_import(
    input: &FacadeResolutionInput<'_>,
) -> Option<FacadeLookupResult> {
    let components = input.header_path.as_components();
    if components.is_empty() {
        return None;
    }

    let first = input.string_table.resolve(components[0]);
    let library_prefix = input.facade_exports.keys().find(|p| *p == first)?;

    // Internal imports within the same library use normal file-based resolution.
    let importer_library = input.file_library_membership.get(input.importer_file);
    if importer_library.map(|s| s.as_str()) == Some(library_prefix) {
        return Some(FacadeLookupResult::NotAFacadeImport);
    }

    // Imports from outside the source library must request exactly one public symbol from the
    // facade root. Extra path components are implementation details, not part of the facade API.
    if components.len() != 2 {
        return Some(FacadeLookupResult::NotExported {
            facade_name: library_prefix.clone(),
            facade_type: FacadeType::SourceLibrary,
        });
    }

    let symbol_name = components[1];
    let exports = input.facade_exports.get(library_prefix)?;
    for entry in exports {
        if entry.export_name == symbol_name {
            match &entry.target {
                FacadeExportTarget::Source(path) => {
                    return Some(FacadeLookupResult::ExportedSource {
                        path: path.clone(),
                        exported_entries: exports.clone(),
                    });
                }
                FacadeExportTarget::External(symbol_id) => {
                    return Some(FacadeLookupResult::ExportedExternal {
                        symbol_id: *symbol_id,
                    });
                }
            }
        }
    }

    Some(FacadeLookupResult::NotExported {
        facade_name: library_prefix.clone(),
        facade_type: FacadeType::SourceLibrary,
    })
}

/// Cross-module-root facade lookup.
///
/// WHAT: when an import path targets a regular module root under the entry root and the
/// importer is outside that module, the symbol must be exported by the module facade.
fn try_resolve_module_root_facade_import(
    input: &FacadeResolutionInput<'_>,
) -> Option<FacadeLookupResult> {
    if input.module_root_prefixes.is_empty() {
        return None;
    }

    // Build the effective path to match against module root prefixes.
    // For entry-root imports, this is just the header path.
    // For relative imports, prepend the importer's parent directory.
    let components = input.header_path.as_components();
    if components.is_empty() {
        return None;
    }

    let is_relative = input.string_table.resolve(components[0]) == ".";
    let effective_path = if is_relative {
        if let Some(importer_dir) = input.importer_file.parent() {
            let mut combined = importer_dir.as_components().to_vec();
            // Skip the leading "." component.
            combined.extend_from_slice(&components[1..]);
            InternedPath::from_components(combined)
        } else {
            input.header_path.clone()
        }
    } else {
        input.header_path.clone()
    };

    // Find the longest matching module root prefix.
    for (prefix, module_root) in input.module_root_prefixes {
        if effective_path.starts_with(prefix) {
            // Internal imports within the same module root use normal resolution.
            let importer_root = input.file_module_membership.get(input.importer_file);
            if importer_root == Some(module_root) {
                return Some(FacadeLookupResult::NotAFacadeImport);
            }

            // Named module roots use the root path as their public API prefix. The entry-root
            // facade has no prefix, so its public re-exports stay addressable at their real
            // source paths, but arbitrary paths with the same final name must not match.
            let prefix_len = prefix.as_components().len();
            let effective_components = effective_path.as_components();
            let public_suffix = &effective_components[prefix_len..];
            let exports = input.module_root_facade_exports.get(module_root)?;

            if prefix_len == 0 {
                for entry in exports {
                    if let FacadeExportTarget::Source(path) = &entry.target
                        && suffix_matches_with_optional_source_extension(
                            path,
                            &effective_path,
                            input.string_table,
                        )
                    {
                        return Some(FacadeLookupResult::ExportedSource {
                            path: path.clone(),
                            exported_entries: exports.clone(),
                        });
                    }
                }

                return Some(FacadeLookupResult::NotExported {
                    facade_name: prefix.to_portable_string(input.string_table),
                    facade_type: FacadeType::ModuleRoot,
                });
            }

            let symbol_name = if public_suffix.len() == 1 {
                Some(public_suffix[0])
            } else {
                None
            };
            let Some(symbol_name) = symbol_name else {
                return Some(FacadeLookupResult::NotExported {
                    facade_name: prefix.to_portable_string(input.string_table),
                    facade_type: FacadeType::ModuleRoot,
                });
            };

            for entry in exports {
                if entry.export_name == symbol_name {
                    match &entry.target {
                        FacadeExportTarget::Source(path) => {
                            return Some(FacadeLookupResult::ExportedSource {
                                path: path.clone(),
                                exported_entries: exports.clone(),
                            });
                        }
                        FacadeExportTarget::External(symbol_id) => {
                            return Some(FacadeLookupResult::ExportedExternal {
                                symbol_id: *symbol_id,
                            });
                        }
                    }
                }
            }
            return Some(FacadeLookupResult::NotExported {
                facade_name: prefix.to_portable_string(input.string_table),
                facade_type: FacadeType::ModuleRoot,
            });
        }
    }

    None
}

/// Input bundle for source-library boundary checking.
pub(crate) struct SourceLibraryBoundaryCheckInput<'a> {
    pub(crate) importer_file: &'a InternedPath,
    pub(crate) target_file: &'a InternedPath,
    pub(crate) requested_path: &'a InternedPath,
    pub(crate) location: SourceLocation,
    pub(crate) file_library_membership: &'a FxHashMap<InternedPath, String>,
    pub(crate) source_library_facade_files: &'a FxHashMap<String, InternedPath>,
    pub(crate) string_table: &'a mut StringTable,
}

/// Enforces source-library facade privacy for concrete source-file imports.
///
/// WHAT: after normal source resolution reaches a file inside a source library, an importer
/// outside that library may only import the library's facade file. Grouped public symbol imports
/// should already have resolved through `resolve_facade_import`.
pub(crate) fn check_source_library_boundary(
    input: SourceLibraryBoundaryCheckInput<'_>,
) -> BoundaryCheckResult<()> {
    let Some(target_library) = input.file_library_membership.get(input.target_file) else {
        return Ok(());
    };

    let importer_library = input.file_library_membership.get(input.importer_file);
    if importer_library.map(String::as_str) == Some(target_library.as_str()) {
        return Ok(());
    }

    if input
        .source_library_facade_files
        .get(target_library)
        .is_some_and(|facade_file| input.target_file == facade_file)
    {
        return Ok(());
    }

    let facade_name_id = input.string_table.intern(target_library);
    Err(Box::new(CompilerDiagnostic::not_exported_by_facade(
        input.requested_path.clone(),
        facade_name_id,
        ImportFacadeType::SourceLibrary,
        input.location,
    )))
}

/// Input bundle for module boundary checking.
///
/// WHY: cross-module-root imports must respect the target module's facade even when they
/// resolved through normal file-based path matching.
pub(crate) struct ModuleBoundaryCheckInput<'a> {
    pub(crate) importer_file: &'a InternedPath,
    pub(crate) target_file: &'a InternedPath,
    pub(crate) symbol_path: &'a InternedPath,
    pub(crate) location: SourceLocation,
    pub(crate) file_module_membership: &'a FxHashMap<InternedPath, InternedPath>,
    pub(crate) module_root_facade_exports:
        &'a FxHashMap<InternedPath, FxHashSet<FacadeExportEntry>>,
}

/// Enforces module-private boundaries for cross-module-root imports.
///
/// WHAT: after an import resolves to a concrete source file, if the importer and target are in
/// different module roots, the symbol must be exported by the target module's facade.
/// WHY: ordinary source declarations are importable inside one module, but cross-module imports
/// must use the target module's explicit facade surface.
pub(crate) fn check_module_boundary(
    input: ModuleBoundaryCheckInput<'_>,
) -> BoundaryCheckResult<()> {
    let importer_root = input.file_module_membership.get(input.importer_file);
    let target_root = input.file_module_membership.get(input.target_file);

    // Skip if either file has no module root membership (e.g., source libraries handled separately).
    let (Some(importer_root), Some(target_root)) = (importer_root, target_root) else {
        return Ok(());
    };

    // Same module root: no boundary.
    if importer_root == target_root {
        return Ok(());
    }

    // Different module roots: grouped public imports should already have resolved through the
    // target facade. Direct source-path resolution here is therefore a boundary violation.
    if input.module_root_facade_exports.contains_key(target_root) {
        return Err(Box::new(diagnostics::cross_module_import_not_exported(
            input.symbol_path,
            input.location,
        )));
    }

    // Target module has no facade.
    Err(Box::new(diagnostics::missing_module_facade(
        input.symbol_path,
        input.location,
    )))
}
