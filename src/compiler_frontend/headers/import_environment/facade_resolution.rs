//! Header-stage facade import resolution.
//!
//! WHAT: resolves cross-library and cross-module-root imports through the target module's
//! facade exports.
//! WHY: source-library modules and regular module roots expose symbols only through their
//! `#mod.bst` facade; external importers cannot bypass it to import internal symbols.
//! MUST NOT: perform general import target resolution (that belongs in `target_resolution.rs`).

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::headers::module_symbols::FacadeExportEntry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::{FxHashMap, FxHashSet};

/// Result of looking up an import path against a module facade.
pub(crate) enum FacadeLookupResult {
    /// This import does not target a facade; use normal file-based resolution.
    NotAFacadeImport,
    /// Facade exports a source symbol with this canonical path.
    ExportedSource(InternedPath),
    /// Facade exports an external package symbol.
    ExportedExternal(ExternalSymbolId),
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

/// Attempt to resolve an import through a source-library facade only.
///
/// WHAT: checks whether the import path starts with a known library prefix and the importer
/// is outside that library. If so, looks up the symbol name in the library's facade exports.
/// WHY: re-export resolution only needs library facade checks; module-root facade checks
/// are handled separately by normal target resolution and `check_module_boundary`.
pub(crate) fn resolve_library_facade_import(
    input: &FacadeResolutionInput<'_>,
) -> Option<FacadeLookupResult> {
    try_resolve_library_facade_import(input)
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

    // External import — look up the symbol name in the facade exports.
    let symbol_name = input.header_path.name()?;
    let exports = input.facade_exports.get(library_prefix)?;
    for entry in exports {
        if entry.export_name == symbol_name {
            match &entry.target {
                crate::compiler_frontend::headers::module_symbols::FacadeExportTarget::Source(
                    path,
                ) => {
                    return Some(FacadeLookupResult::ExportedSource(path.clone()));
                }
                crate::compiler_frontend::headers::module_symbols::FacadeExportTarget::External(
                    id,
                ) => {
                    return Some(FacadeLookupResult::ExportedExternal(*id));
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

            // External import — look up the symbol name in the facade exports.
            let symbol_name = input.header_path.name()?;
            let exports = input.module_root_facade_exports.get(module_root)?;
            for entry in exports {
                if entry.export_name == symbol_name {
                    match &entry.target {
                        crate::compiler_frontend::headers::module_symbols::FacadeExportTarget::Source(path) => {
                            return Some(FacadeLookupResult::ExportedSource(path.clone()));
                        }
                        crate::compiler_frontend::headers::module_symbols::FacadeExportTarget::External(id) => {
                            return Some(FacadeLookupResult::ExportedExternal(*id));
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
    pub(crate) string_table: &'a StringTable,
}

/// Enforces module-private boundaries for cross-module-root imports.
///
/// WHAT: after an import resolves to a concrete source file, if the importer and target are in
/// different module roots, the symbol must be exported by the target module's facade.
/// WHY: `#` exports are visible across files in the same module, but not automatically visible
/// to files in other modules.
pub(crate) fn check_module_boundary(
    input: ModuleBoundaryCheckInput<'_>,
) -> Result<(), CompilerError> {
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

    // Different module roots: must go through facade.
    if let Some(facade_exports) = input.module_root_facade_exports.get(target_root) {
        if let Some(symbol_name) = input.symbol_path.name() {
            let exported = facade_exports.iter().any(|e| e.export_name == symbol_name);
            if exported {
                return Ok(());
            }
        }

        return Err(diagnostics::cross_module_import_not_exported(
            input.symbol_path,
            input.location,
            input.string_table,
        ));
    }

    // Target module has no facade.
    Err(diagnostics::missing_module_facade(
        input.symbol_path,
        input.location,
        input.string_table,
    ))
}
