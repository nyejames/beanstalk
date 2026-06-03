//! Facade export and file-membership data for header imports.
//!
//! WHAT: derives source-library and entry-root module facade export maps from parsed headers and
//! public file imports.
//! WHY: import environment preparation needs a single header-owned view of which declarations are
//! exposed across facade boundaries and which source files belong to each boundary.
//!
//! ## Export map construction
//!
//! Facade exports come from two sources:
//! 1. Public authored headers (`export` declarations) in the facade file.
//! 2. Public import records (`export import` or `export @path { ... }`) in the facade file.
//!
//! Because public imports may re-export symbols from other facades, construction is two-pass:
//! - Pass 1 collects all public authored declarations for every facade.
//! - Pass 2 resolves public imports against the completed authored export maps.

use crate::compiler_frontend::compiler_errors::compiler_error_to_diagnostic;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, ImportFacadeType};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::import_environment::{
    ExternalPackageSymbolLookup, ExternalPackageSymbolResolutionInput, FacadeLookupResult,
    FacadeResolutionInput, FacadeType, ImportTargetResolutionInput, ModuleBoundaryCheckInput,
    ResolvedImportTarget, SourceLibraryBoundaryCheckInput, check_module_boundary,
    check_source_library_boundary, resolve_external_package_symbol, resolve_facade_import,
    resolve_import_target,
};
use crate::compiler_frontend::headers::module_symbols::{
    FacadeExportEntry, FacadeExportTarget, ModuleSymbols,
};
use crate::compiler_frontend::headers::types::{FileRole, Header, HeaderExportMode, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use rustc_hash::{FxHashMap, FxHashSet};

/// Whether a header kind represents a real authored declaration that can be exported by a
/// module facade.
///
/// WHAT: functions, structs, choices, type aliases, traits, and compile-time constants are
/// authored declarations. Const templates, conformance declarations, and the implicit start
/// function are not.
fn is_authored_facade_declaration(kind: &HeaderKind) -> bool {
    matches!(
        kind,
        HeaderKind::Function { .. }
            | HeaderKind::Struct { .. }
            | HeaderKind::Choice { .. }
            | HeaderKind::TypeAlias { .. }
            | HeaderKind::Trait { .. }
            | HeaderKind::Constant { .. }
    )
}

/// Whether a header is a public authored facade export.
///
/// WHAT: only explicit `export` declarations in `#mod.bst` become public facade entries.
fn is_authored_facade_export(header: &Header) -> bool {
    header.file_role == FileRole::ModuleFacade
        && header.export_mode == HeaderExportMode::Public
        && is_authored_facade_declaration(&header.kind)
}

/// Build facade export maps and file library/module membership from parsed headers and the path
/// resolver.
pub(super) fn build_facade_data(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    // Pass 1: collect public authored declarations for all facades.
    build_source_library_facade_exports(module_symbols, headers, resolver, string_table)?;
    build_module_root_facade_exports_pass1(module_symbols, headers, resolver, string_table);

    // Membership does not depend on import resolution.
    build_source_library_membership(module_symbols, resolver, string_table);
    build_module_root_membership(module_symbols, resolver, string_table);

    // Pass 2: resolve public facade imports against the completed authored export maps.
    build_source_library_facade_imports(
        module_symbols,
        resolver,
        external_package_registry,
        string_table,
    )?;
    build_module_root_facade_imports(
        module_symbols,
        resolver,
        external_package_registry,
        string_table,
    )?;

    Ok(())
}

// --------------------------
//  Source-library facades
// --------------------------

fn build_source_library_facade_exports(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    for (prefix, facade_file) in resolver.facade_files() {
        let mod_file_logical = resolver
            .logical_path_for_canonical_file(facade_file, string_table)
            .map_err(|error| compiler_error_to_diagnostic(&error))?;
        let mod_file_interned = InternedPath::from_path_buf(&mod_file_logical, string_table);

        let mut collector = FacadeExportCollector::default();

        module_symbols
            .file_library_membership
            .insert(mod_file_interned.clone(), prefix.clone());
        module_symbols
            .source_library_facade_files
            .insert(prefix.clone(), mod_file_interned.clone());

        for header in headers {
            if header.source_file != mod_file_interned {
                continue;
            }

            if !is_authored_facade_export(header) {
                continue;
            }

            if let Some(export_name) = header.tokens.src_path.name() {
                collector.insert(
                    export_name,
                    FacadeExportTarget::Source(header.tokens.src_path.clone()),
                    header.name_location.clone(),
                )?;
            }
        }

        module_symbols
            .facade_exports
            .insert(prefix.clone(), collector.exports);
    }

    Ok(())
}

fn build_source_library_facade_imports(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    for (prefix, facade_file) in resolver.facade_files() {
        let mod_file_logical = resolver
            .logical_path_for_canonical_file(facade_file, string_table)
            .map_err(|error| compiler_error_to_diagnostic(&error))?;
        let mod_file_interned = InternedPath::from_path_buf(&mod_file_logical, string_table);

        let current_exports = module_symbols
            .facade_exports
            .get(prefix)
            .cloned()
            .unwrap_or_default();
        let mut collector = FacadeExportCollector::from_existing(&current_exports);

        if let Some(imports) = module_symbols
            .file_imports_by_source
            .get(&mod_file_interned)
        {
            let mut receiver_method_validations = Vec::new();
            for import in imports {
                if import.export_mode != HeaderExportMode::Public {
                    continue;
                }

                let export_name = public_export_name(import)?;
                let target = resolve_public_facade_import(
                    module_symbols,
                    import,
                    &mod_file_interned,
                    external_package_registry,
                    string_table,
                )?;

                record_receiver_method_export_validation(
                    module_symbols,
                    export_name,
                    &target,
                    import.location.clone(),
                    &mut receiver_method_validations,
                );
                collector.insert(export_name, target, import.location.clone())?;
            }

            validate_receiver_method_exports_have_public_receivers(
                module_symbols,
                &collector.exports,
                &receiver_method_validations,
                string_table,
            )?;
        }

        module_symbols
            .facade_exports
            .insert(prefix.clone(), collector.exports);
    }

    Ok(())
}

// --------------------------
//  Module-root facades
// --------------------------

fn build_module_root_facade_exports_pass1(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) {
    let mut module_root_prefixes =
        build_module_root_prefixes(module_symbols, resolver, string_table);
    module_root_prefixes.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
    module_symbols.module_root_prefixes = module_root_prefixes;

    for header in headers {
        let Some(canonical_path) = &header.tokens.canonical_os_path else {
            continue;
        };
        let Some(module_root) = resolver.module_root_for_file(canonical_path) else {
            continue;
        };

        let module_root_interned = InternedPath::from_path_buf(&module_root, string_table);
        let logical = header.source_file.clone();
        let canonical = header.canonical_source_file(string_table);

        module_symbols
            .file_module_membership
            .insert(logical, module_root_interned.clone());
        module_symbols
            .file_module_membership
            .insert(canonical, module_root_interned.clone());

        if let Some(facade_file) = resolver.module_root_facades().get(&module_root)
            && canonical_path == facade_file
            && is_authored_facade_export(header)
            && let Some(export_name) = header.tokens.src_path.name()
        {
            let exports = module_symbols
                .module_root_facade_exports
                .entry(module_root_interned)
                .or_default();
            exports.insert(FacadeExportEntry {
                export_name,
                target: FacadeExportTarget::Source(header.tokens.src_path.clone()),
            });
        }
    }
}

fn build_module_root_facade_imports(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    let facade_sources: Vec<_> = module_symbols
        .file_imports_by_source
        .keys()
        .filter(|source_file| {
            module_symbols.file_roles_by_source.get(*source_file) == Some(&FileRole::ModuleFacade)
        })
        .cloned()
        .collect();

    for facade_source in facade_sources {
        let Some(canonical_facade_path) = module_symbols
            .canonical_os_path_by_source
            .get(&facade_source)
        else {
            continue;
        };
        let Some(module_root) = resolver.module_root_for_file(canonical_facade_path) else {
            continue;
        };
        let Some(module_facade_path) = resolver.module_root_facades().get(&module_root) else {
            continue;
        };

        if module_facade_path != canonical_facade_path {
            continue;
        }

        let module_root_interned = InternedPath::from_path_buf(&module_root, string_table);

        let current_exports = module_symbols
            .module_root_facade_exports
            .get(&module_root_interned)
            .cloned()
            .unwrap_or_default();
        let mut collector = FacadeExportCollector::from_existing(&current_exports);
        let mut receiver_method_validations = Vec::new();

        let imports = module_symbols
            .file_imports_by_source
            .get(&facade_source)
            .cloned()
            .unwrap_or_default();

        for import in imports {
            if import.export_mode != HeaderExportMode::Public {
                continue;
            }

            let export_name = public_export_name(&import)?;
            let target = resolve_public_facade_import(
                module_symbols,
                &import,
                &facade_source,
                external_package_registry,
                string_table,
            )?;

            record_receiver_method_export_validation(
                module_symbols,
                export_name,
                &target,
                import.location.clone(),
                &mut receiver_method_validations,
            );
            collector.insert(export_name, target, import.location.clone())?;
        }

        validate_receiver_method_exports_have_public_receivers(
            module_symbols,
            &collector.exports,
            &receiver_method_validations,
            string_table,
        )?;

        module_symbols
            .module_root_facade_exports
            .insert(module_root_interned.clone(), collector.exports);
    }

    Ok(())
}

#[derive(Clone, Debug)]
struct PublicReceiverMethodExportValidation {
    export_name: StringId,
    method_path: InternedPath,
    location: SourceLocation,
}

fn record_receiver_method_export_validation(
    module_symbols: &ModuleSymbols,
    export_name: StringId,
    target: &FacadeExportTarget,
    location: SourceLocation,
    validations: &mut Vec<PublicReceiverMethodExportValidation>,
) {
    let FacadeExportTarget::Source(method_path) = target else {
        return;
    };

    if !module_symbols.receiver_method_paths.contains(method_path) {
        return;
    }

    validations.push(PublicReceiverMethodExportValidation {
        export_name,
        method_path: method_path.clone(),
        location,
    });
}

fn validate_receiver_method_exports_have_public_receivers(
    module_symbols: &ModuleSymbols,
    exports: &FxHashSet<FacadeExportEntry>,
    validations: &[PublicReceiverMethodExportValidation],
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    for validation in validations {
        if public_receiver_type_is_visible_for_method(
            module_symbols,
            exports,
            &validation.method_path,
            string_table,
        ) {
            continue;
        }

        let receiver_type_name = module_symbols
            .receiver_method_receiver_names
            .get(&validation.method_path)
            .copied();
        return Err(
            CompilerDiagnostic::receiver_method_import_requires_visible_receiver_type(
                validation.export_name,
                receiver_type_name,
                validation.location.clone(),
            ),
        );
    }

    Ok(())
}

fn public_receiver_type_is_visible_for_method(
    module_symbols: &ModuleSymbols,
    exports: &FxHashSet<FacadeExportEntry>,
    method_path: &InternedPath,
    string_table: &mut StringTable,
) -> bool {
    let Some(receiver_name) = module_symbols
        .receiver_method_receiver_names
        .get(method_path)
    else {
        return false;
    };

    if receiver_name_is_builtin_scalar(*receiver_name, string_table) {
        return true;
    }

    let Some(receiver_type_path) =
        source_receiver_nominal_type_path(module_symbols, method_path, *receiver_name)
    else {
        return false;
    };

    exports
        .iter()
        .any(|entry| matches!(&entry.target, FacadeExportTarget::Source(path) if path == &receiver_type_path))
}

fn receiver_name_is_builtin_scalar(receiver_name: StringId, string_table: &StringTable) -> bool {
    matches!(
        string_table.resolve(receiver_name),
        "Int" | "Float" | "Bool" | "String" | "Char"
    )
}

fn source_receiver_nominal_type_path(
    module_symbols: &ModuleSymbols,
    receiver_method_path: &InternedPath,
    receiver_name: StringId,
) -> Option<InternedPath> {
    let method_source = module_symbols
        .canonical_source_by_symbol_path
        .get(receiver_method_path)?;

    module_symbols
        .nominal_type_paths
        .iter()
        .find(|type_path| {
            type_path.name() == Some(receiver_name)
                && module_symbols
                    .canonical_source_by_symbol_path
                    .get(*type_path)
                    .is_some_and(|type_source| type_source == method_source)
        })
        .cloned()
}

// --------------------------
//  Public import resolution
// --------------------------

/// Derive the public export name for a facade import.
///
/// WHAT: alias wins; otherwise use the imported symbol name.
fn public_export_name(
    import: &crate::compiler_frontend::headers::parse_file_headers::FileImport,
) -> Result<StringId, CompilerDiagnostic> {
    match import.alias {
        Some(alias) => Ok(alias),
        None => match import.header_path.name() {
            Some(name) => Ok(name),
            None => Err(CompilerDiagnostic::missing_import_target(
                import.header_path.clone(),
                import.location.clone(),
            )),
        },
    }
}

/// Resolve a public facade import to its concrete export target.
///
/// WHAT: tries external package resolution, then facade resolution, then direct source resolution.
/// WHY: public imports in a facade re-export the resolved symbol through the module API.
fn resolve_public_facade_import(
    module_symbols: &ModuleSymbols,
    import: &crate::compiler_frontend::headers::parse_file_headers::FileImport,
    facade_file: &InternedPath,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> Result<FacadeExportTarget, CompilerDiagnostic> {
    // 1. Try external package resolution first.
    match resolve_external_package_symbol(ExternalPackageSymbolResolutionInput {
        import_path: &import.header_path,
        external_package_registry,
        string_table,
    }) {
        ExternalPackageSymbolLookup::Found { symbol_id } => {
            return Ok(FacadeExportTarget::External(symbol_id));
        }
        ExternalPackageSymbolLookup::PackageFoundSymbolMissing {
            package_path,
            symbol_name,
        } => {
            return Err(CompilerDiagnostic::missing_package_symbol(
                symbol_name,
                package_path,
                import.location.clone(),
            ));
        }
        ExternalPackageSymbolLookup::NoMatch => {}
    }

    // 2. Try facade resolution.
    let facade_input = FacadeResolutionInput {
        importer_file: facade_file,
        header_path: &import.header_path,
        facade_exports: &module_symbols.facade_exports,
        file_library_membership: &module_symbols.file_library_membership,
        module_root_facade_exports: &module_symbols.module_root_facade_exports,
        file_module_membership: &module_symbols.file_module_membership,
        module_root_prefixes: &module_symbols.module_root_prefixes,
        string_table,
    };

    if let Some(facade_result) = resolve_facade_import(&facade_input) {
        match facade_result {
            FacadeLookupResult::ExportedSource { path, .. } => {
                return Ok(FacadeExportTarget::Source(path));
            }
            FacadeLookupResult::ExportedExternal { symbol_id } => {
                return Ok(FacadeExportTarget::External(symbol_id));
            }
            FacadeLookupResult::NotExported {
                facade_name,
                facade_type,
            } => {
                // The entry-root facade has no public path prefix. While building that facade's
                // own public imports, root-relative same-module re-exports must still be allowed
                // to fall through to direct source resolution. Normal importers keep receiving
                // `NotExported` from `prepare_import_environment`.
                if matches!(facade_type, FacadeType::ModuleRoot) && facade_name.is_empty() {
                    // Fall through to direct source resolution.
                } else {
                    // The target facade exists but does not export this symbol.
                    // Preserve the same diagnostic that a normal importer would see.
                    let facade_name_id = string_table.intern(&facade_name);
                    let diagnostic_facade_type = match facade_type {
                        FacadeType::SourceLibrary => ImportFacadeType::SourceLibrary,
                        FacadeType::ModuleRoot => ImportFacadeType::ModuleRoot,
                    };
                    return Err(CompilerDiagnostic::not_exported_by_facade(
                        import.header_path.clone(),
                        facade_name_id,
                        diagnostic_facade_type,
                        import.location.clone(),
                    ));
                }
            }
            FacadeLookupResult::NotAFacadeImport => {
                // Fall through to direct source resolution.
            }
        }
    }

    // 3. Direct source resolution.
    let target = resolve_import_target(ImportTargetResolutionInput {
        import_path: &import.header_path,
        location: &import.location,
        module_file_paths: &module_symbols.module_file_paths,
        importable_symbol_paths: &module_symbols.importable_source_symbol_paths,
        external_package_registry,
        string_table,
    })?;

    match target {
        ResolvedImportTarget::Source { symbol_path, .. } => {
            if let Some(target_file) = module_symbols
                .canonical_source_by_symbol_path
                .get(&symbol_path)
            {
                check_source_library_boundary(SourceLibraryBoundaryCheckInput {
                    importer_file: facade_file,
                    target_file,
                    requested_path: &import.header_path,
                    location: import.location.clone(),
                    file_library_membership: &module_symbols.file_library_membership,
                    source_library_facade_files: &module_symbols.source_library_facade_files,
                    string_table,
                })?;
                check_module_boundary(ModuleBoundaryCheckInput {
                    importer_file: facade_file,
                    target_file,
                    symbol_path: &symbol_path,
                    location: import.location.clone(),
                    file_module_membership: &module_symbols.file_module_membership,
                    module_root_facade_exports: &module_symbols.module_root_facade_exports,
                })?;
            }

            Ok(FacadeExportTarget::Source(symbol_path))
        }
        ResolvedImportTarget::External { symbol_id } => Ok(FacadeExportTarget::External(symbol_id)),
    }
}

// --------------------------
//  Export collection helper
// --------------------------

/// Accumulates facade export entries for one facade file and detects duplicate public names.
#[derive(Default)]
struct FacadeExportCollector {
    exports: FxHashSet<FacadeExportEntry>,
    seen_names: FxHashMap<StringId, SourceLocation>,
}

impl FacadeExportCollector {
    fn from_existing(exports: &FxHashSet<FacadeExportEntry>) -> Self {
        let mut seen_names = FxHashMap::default();
        for entry in exports {
            seen_names.insert(entry.export_name, SourceLocation::default());
        }
        Self {
            exports: exports.clone(),
            seen_names,
        }
    }

    fn insert(
        &mut self,
        export_name: StringId,
        target: FacadeExportTarget,
        location: SourceLocation,
    ) -> Result<(), CompilerDiagnostic> {
        if self.seen_names.contains_key(&export_name) {
            return Err(CompilerDiagnostic::duplicate_public_export(
                export_name,
                location,
            ));
        }
        self.seen_names.insert(export_name, location);
        self.exports.insert(FacadeExportEntry {
            export_name,
            target,
        });
        Ok(())
    }
}

// --------------------------
//  Membership helpers
// --------------------------

fn build_source_library_membership(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) {
    for (source_file, canonical_path) in module_symbols.canonical_os_path_by_source.clone() {
        for (prefix, root_path) in resolver.source_library_roots() {
            if canonical_path.starts_with(root_path) {
                let canonical_source = InternedPath::from_path_buf(&canonical_path, string_table);
                module_symbols
                    .file_library_membership
                    .insert(source_file.clone(), prefix.clone());
                module_symbols
                    .file_library_membership
                    .insert(canonical_source, prefix.clone());
                break;
            }
        }
    }
}

fn build_module_root_membership(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) {
    for (source_file, canonical_path) in module_symbols.canonical_os_path_by_source.clone() {
        let Some(module_root) = resolver.module_root_for_file(&canonical_path) else {
            continue;
        };

        let module_root_interned = InternedPath::from_path_buf(&module_root, string_table);
        let canonical_source = InternedPath::from_path_buf(&canonical_path, string_table);

        module_symbols
            .file_module_membership
            .insert(source_file, module_root_interned.clone());
        module_symbols
            .file_module_membership
            .insert(canonical_source, module_root_interned);
    }
}

fn build_module_root_prefixes(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Vec<(InternedPath, InternedPath)> {
    let mut module_root_prefixes = Vec::new();

    for module_root in resolver.module_roots() {
        let root_interned = InternedPath::from_path_buf(module_root, string_table);

        if resolver.module_root_facades().contains_key(module_root) {
            module_symbols
                .module_root_facade_exports
                .entry(root_interned.clone())
                .or_default();
        }

        if let Ok(relative) = module_root.strip_prefix(resolver.entry_root()) {
            let prefix_interned = InternedPath::from_path_buf(relative, string_table);
            module_root_prefixes.push((prefix_interned, root_interned));
        }
    }

    module_root_prefixes
}
