//! Header-stage re-export resolution.
//!
//! WHAT: resolves `#import @...` clauses in `#mod.bst` files to concrete targets and updates
//! the module's facade export metadata.
//! WHY: re-exports must be resolved before per-file import binding so that cross-library
//! imports can see symbols exposed through the facade.
//! MUST NOT: register per-file visible names or build file-local visibility maps.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::headers::import_environment::facade_resolution::{
    FacadeLookupResult, FacadeResolutionInput, ModuleBoundaryCheckInput, check_module_boundary,
    resolve_library_facade_import,
};
use crate::compiler_frontend::headers::import_environment::target_resolution::{
    ImportTargetResolutionInput, ResolvedImportTarget, resolve_import_target,
};
use crate::compiler_frontend::headers::import_environment::visible_names::check_alias_case_warning;
use crate::compiler_frontend::headers::module_symbols::{
    FacadeExportEntry, FacadeExportTarget, ModuleSymbols,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::source_libraries::mod_file::import_path_references_mod_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashSet;

/// Input bundle for re-export resolution.
///
/// WHY: re-export resolution mutates `module_symbols` facade export maps and needs access to
/// the external package registry for virtual-package re-exports.
pub(crate) struct ReExportResolutionInput<'a> {
    pub(crate) module_symbols: &'a mut ModuleSymbols,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) string_table: &'a mut StringTable,
}

/// Resolve re-export targets collected during header parsing and augment the facade export maps.
///
/// WHAT: for each `#import @...` clause in `#mod.bst` files, resolves the target path to a
/// concrete source symbol or external package symbol, then adds it to the appropriate facade.
pub(crate) fn resolve_re_exports(
    input: ReExportResolutionInput<'_>,
) -> Result<Vec<CompilerWarning>, CompilerError> {
    let re_exports_by_source = input.module_symbols.file_re_exports_by_source.clone();
    let mut warnings: Vec<CompilerWarning> = Vec::new();

    let importable_symbol_paths: FxHashSet<_> = input
        .module_symbols
        .importable_symbol_exported
        .keys()
        .cloned()
        .collect();

    for (source_file, re_exports) in re_exports_by_source {
        // Determine which facade this source file belongs to.
        let facade_key = determine_facade_key(&source_file, input.module_symbols);

        let Some(facade_key) = facade_key else {
            // This file is not part of any facade; skip its re-exports.
            continue;
        };

        for re_export in re_exports {
            // Reject direct mod file imports.
            if import_path_references_mod_file(&re_export.header_path, input.string_table) {
                return Err(diagnostics::direct_mod_file_import(
                    &re_export.header_path,
                    re_export.location.clone(),
                    input.string_table,
                ));
            }

            let target = resolve_re_export_target(
                &source_file,
                &re_export,
                input.module_symbols,
                input.external_package_registry,
                &importable_symbol_paths,
                input.string_table,
                &mut warnings,
            )?;

            let Some(symbol_name) = re_export.header_path.name() else {
                return Err(diagnostics::missing_reexport_symbol_name(
                    re_export.location.clone(),
                ));
            };
            let export_name = re_export.alias.unwrap_or(symbol_name);

            if re_export.alias.is_some()
                && let Some(warning) = check_alias_case_warning(
                    &re_export.alias_location,
                    &re_export.path_location,
                    export_name,
                    symbol_name,
                    input.string_table,
                )
            {
                warnings.push(warning);
            }

            let entry = FacadeExportEntry {
                export_name,
                target,
            };

            let exports = match &facade_key {
                FacadeKey::Library(prefix) => input
                    .module_symbols
                    .facade_exports
                    .entry(prefix.clone())
                    .or_default(),
                FacadeKey::ModuleRoot(root) => input
                    .module_symbols
                    .module_root_facade_exports
                    .entry(root.clone())
                    .or_default(),
            };

            if exports.iter().any(|e| e.export_name == export_name) {
                return Err(diagnostics::duplicate_facade_export_name(
                    input.string_table.resolve(export_name),
                    re_export.location.clone(),
                ));
            }
            exports.insert(entry);
        }
    }

    Ok(warnings)
}

/// Local classification of which facade a source file belongs to.
enum FacadeKey {
    Library(String),
    ModuleRoot(InternedPath),
}

/// Determine the facade key for a source file, if any.
fn determine_facade_key(
    source_file: &InternedPath,
    module_symbols: &ModuleSymbols,
) -> Option<FacadeKey> {
    if let Some(library_prefix) = module_symbols.file_library_membership.get(source_file) {
        return Some(FacadeKey::Library(library_prefix.clone()));
    }

    if let Some(module_root) = module_symbols.file_module_membership.get(source_file)
        && module_symbols
            .module_root_facade_exports
            .contains_key(module_root)
    {
        return Some(FacadeKey::ModuleRoot(module_root.clone()));
    }

    None
}

/// Resolve one re-export target to its concrete export form.
fn resolve_re_export_target(
    source_file: &InternedPath,
    re_export: &crate::compiler_frontend::headers::types::FileReExport,
    module_symbols: &ModuleSymbols,
    external_package_registry: &ExternalPackageRegistry,
    importable_symbol_paths: &FxHashSet<InternedPath>,
    string_table: &StringTable,
    _warnings: &mut Vec<CompilerWarning>,
) -> Result<FacadeExportTarget, CompilerError> {
    // Try facade resolution first.
    let facade_input = FacadeResolutionInput {
        importer_file: source_file,
        header_path: &re_export.header_path,
        facade_exports: &module_symbols.facade_exports,
        file_library_membership: &module_symbols.file_library_membership,
        module_root_facade_exports: &module_symbols.module_root_facade_exports,
        file_module_membership: &module_symbols.file_module_membership,
        module_root_prefixes: &module_symbols.module_root_prefixes,
        string_table,
    };

    // Re-exports only need library facade resolution; module-root boundaries are
    // checked later by `check_module_boundary` after normal target resolution.
    if let Some(facade_result) = resolve_library_facade_import(&facade_input) {
        match facade_result {
            FacadeLookupResult::ExportedSource(path) => {
                return Ok(FacadeExportTarget::Source(path));
            }
            FacadeLookupResult::ExportedExternal(id) => {
                return Ok(FacadeExportTarget::External(id));
            }
            FacadeLookupResult::NotExported {
                facade_name,
                facade_type,
            } => {
                return Err(diagnostics::not_exported_by_facade(
                    &re_export.header_path,
                    &facade_name,
                    facade_type,
                    re_export.location.clone(),
                    string_table,
                ));
            }
            FacadeLookupResult::NotAFacadeImport => {
                // Fall through to normal target resolution.
            }
        }
    }

    // Normal target resolution for non-facade re-exports.
    let target = resolve_import_target(ImportTargetResolutionInput {
        import_path: &re_export.header_path,
        location: &re_export.location,
        module_file_paths: &module_symbols.module_file_paths,
        importable_symbol_paths,
        external_package_registry,
        string_table,
    })?;

    // Re-export targets must be exported from their source file.
    if let ResolvedImportTarget::Source {
        ref symbol_path, ..
    } = target
    {
        let is_exported = module_symbols
            .importable_symbol_exported
            .get(symbol_path)
            .copied()
            .unwrap_or(false);
        if !is_exported {
            return Err(diagnostics::not_exported_by_source_file(
                symbol_path,
                re_export.location.clone(),
                string_table,
            ));
        }
    }

    // Re-exports that target another module root must still respect that module's facade surface.
    if let ResolvedImportTarget::Source {
        ref symbol_path, ..
    } = target
        && let Some(target_file) = module_symbols
            .canonical_source_by_symbol_path
            .get(symbol_path)
    {
        check_module_boundary(ModuleBoundaryCheckInput {
            importer_file: source_file,
            target_file,
            symbol_path,
            location: re_export.location.clone(),
            file_module_membership: &module_symbols.file_module_membership,
            module_root_facade_exports: &module_symbols.module_root_facade_exports,
            string_table,
        })?;
    }

    match target {
        ResolvedImportTarget::Source { symbol_path, .. } => {
            Ok(FacadeExportTarget::Source(symbol_path))
        }
        ResolvedImportTarget::External { symbol_id } => Ok(FacadeExportTarget::External(symbol_id)),
    }
}
