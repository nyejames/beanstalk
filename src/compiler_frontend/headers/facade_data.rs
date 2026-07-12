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

use crate::compiler_frontend::builtins::casts::traits::is_core_cast_trait_name;
use crate::compiler_frontend::compiler_errors::compiler_error_to_diagnostic;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, ImportFacadeType, InvalidReceiverDeclarationReason, ReservedNameOwner,
};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::import_environment::{
    ExternalPackageSymbolLookup, ExternalPackageSymbolResolutionInput, FacadeLookupResult,
    FacadeResolutionInput, FacadeType, ImportTargetResolutionInput, ModuleBoundaryCheckInput,
    ResolvedImportTarget, SourceLibraryBoundaryCheckInput, check_module_boundary,
    check_source_library_boundary, resolve_external_package_symbol, resolve_facade_import,
    resolve_import_target,
};
use crate::compiler_frontend::headers::module_symbols::{
    FacadeExportEntry, FacadeExportTarget, ModuleRootBoundary, ModuleSymbols,
};
use crate::compiler_frontend::headers::types::{FileRole, Header, HeaderExportMode, HeaderKind};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

use rustc_hash::{FxHashMap, FxHashSet};

/// Boxed diagnostic result for facade export and membership construction.
///
/// WHAT: keeps the facade build/pass family on one small error boundary.
/// WHY: facade construction carries structured diagnostics through many successful
///      build steps without inlining the large diagnostic value at every return.
type FacadeDataResult<T> = Result<T, Box<CompilerDiagnostic>>;

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
) -> FacadeDataResult<()> {
    // Pass 1: collect public authored declarations for all facades.
    build_source_library_facade_exports(module_symbols, headers, resolver, string_table)?;
    build_module_root_facade_exports_pass1(module_symbols, headers, resolver, string_table)?;

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
) -> FacadeDataResult<()> {
    for (prefix, facade_file) in resolver.facade_files() {
        let mod_file_logical = resolver
            .logical_path_for_canonical_file(facade_file, string_table)
            .map_err(|error| Box::new(compiler_error_to_diagnostic(&error)))?;
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
                reject_source_receiver_method_export(
                    module_symbols,
                    &header.tokens.src_path,
                    header.name_location.clone(),
                )?;
                collector.insert(
                    export_name,
                    FacadeExportTarget::Source(header.tokens.src_path.clone()),
                    header.name_location.clone(),
                    string_table,
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
) -> FacadeDataResult<()> {
    for (prefix, facade_file) in resolver.facade_files() {
        let mod_file_logical = resolver
            .logical_path_for_canonical_file(facade_file, string_table)
            .map_err(|error| Box::new(compiler_error_to_diagnostic(&error)))?;
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

                reject_facade_export_target_if_source_receiver_method(
                    module_symbols,
                    &target,
                    import.location.clone(),
                )?;
                collector.insert(export_name, target, import.location.clone(), string_table)?;
            }
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
) -> FacadeDataResult<()> {
    let mut module_root_boundaries =
        build_module_root_boundaries(module_symbols, resolver, string_table)?;
    module_root_boundaries.sort_by_key(|boundary| std::cmp::Reverse(boundary.import_prefix.len()));
    module_symbols.module_root_boundaries = module_root_boundaries;

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

        if let Some(export_file) = resolver.module_root_export_files().get(&module_root)
            && canonical_path == export_file
            && is_authored_facade_export(header)
            && let Some(export_name) = header.tokens.src_path.name()
        {
            reject_source_receiver_method_export(
                module_symbols,
                &header.tokens.src_path,
                header.name_location.clone(),
            )?;
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

    Ok(())
}

fn build_module_root_facade_imports(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
) -> FacadeDataResult<()> {
    let facade_sources: Vec<_> = module_symbols
        .file_imports_by_source
        .keys()
        .filter(|source_file| {
            module_symbols.file_roles_by_source.get(*source_file) == Some(&FileRole::ModuleFacade)
        })
        .cloned()
        .collect();

    for facade_source in facade_sources {
        let Some(canonical_export_path) = module_symbols
            .canonical_os_path_by_source
            .get(&facade_source)
        else {
            continue;
        };
        let Some(module_root) = resolver.module_root_for_file(canonical_export_path) else {
            continue;
        };
        let Some(module_export_path) = resolver.module_root_export_files().get(&module_root) else {
            continue;
        };

        if module_export_path != canonical_export_path {
            continue;
        }

        let module_root_interned = InternedPath::from_path_buf(&module_root, string_table);

        let current_exports = module_symbols
            .module_root_facade_exports
            .get(&module_root_interned)
            .cloned()
            .unwrap_or_default();
        let mut collector = FacadeExportCollector::from_existing(&current_exports);
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

            reject_facade_export_target_if_source_receiver_method(
                module_symbols,
                &target,
                import.location.clone(),
            )?;
            collector.insert(export_name, target, import.location.clone(), string_table)?;
        }

        module_symbols
            .module_root_facade_exports
            .insert(module_root_interned.clone(), collector.exports);
    }

    Ok(())
}

fn reject_facade_export_target_if_source_receiver_method(
    module_symbols: &ModuleSymbols,
    target: &FacadeExportTarget,
    location: SourceLocation,
) -> FacadeDataResult<()> {
    let FacadeExportTarget::Source(method_path) = target else {
        return Ok(());
    };

    reject_source_receiver_method_export(module_symbols, method_path, location)
}

fn reject_source_receiver_method_export(
    module_symbols: &ModuleSymbols,
    method_path: &InternedPath,
    location: SourceLocation,
) -> FacadeDataResult<()> {
    if module_symbols.receiver_method_paths.contains(method_path) {
        return Err(Box::new(CompilerDiagnostic::invalid_receiver_declaration(
            InvalidReceiverDeclarationReason::ReceiverMethodImportNotAllowed,
            location,
        )));
    }

    Ok(())
}

// --------------------------
//  Public import resolution
// --------------------------

/// Derive the public export name for a facade import.
///
/// WHAT: alias wins; otherwise use the imported symbol name.
fn public_export_name(
    import: &crate::compiler_frontend::headers::parse_file_headers::FileImport,
) -> FacadeDataResult<StringId> {
    match import.alias {
        Some(alias) => Ok(alias),
        None => match import.header_path.name() {
            Some(name) => Ok(name),
            None => Err(Box::new(CompilerDiagnostic::missing_import_target(
                import.header_path.clone(),
                import.location.clone(),
            ))),
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
) -> FacadeDataResult<FacadeExportTarget> {
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
            return Err(Box::new(CompilerDiagnostic::missing_package_symbol(
                symbol_name,
                package_path,
                import.location.clone(),
            )));
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
        module_root_boundaries: &module_symbols.module_root_boundaries,
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
                    return Err(Box::new(CompilerDiagnostic::not_exported_by_facade(
                        import.header_path.clone(),
                        facade_name_id,
                        diagnostic_facade_type,
                        import.location.clone(),
                    )));
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
        string_table: &StringTable,
    ) -> FacadeDataResult<()> {
        let export_name_text = string_table.resolve(export_name);
        if is_core_cast_trait_name(export_name_text) {
            return Err(Box::new(CompilerDiagnostic::reserved_name_collision(
                export_name,
                ReservedNameOwner::CoreTrait,
                location,
            )));
        }

        if self.seen_names.contains_key(&export_name) {
            return Err(Box::new(CompilerDiagnostic::duplicate_public_export(
                export_name,
                location,
            )));
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

fn build_module_root_boundaries(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> FacadeDataResult<Vec<ModuleRootBoundary>> {
    let mut module_root_boundaries = Vec::new();

    for module_root in resolver.module_roots() {
        let root_interned = InternedPath::from_path_buf(module_root, string_table);

        let export_file = resolver
            .module_root_export_files()
            .get(module_root)
            .map(|export_file| {
                module_symbols
                    .module_root_facade_exports
                    .entry(root_interned.clone())
                    .or_default();

                resolver
                    .logical_path_for_canonical_file(export_file, string_table)
                    .map(|logical_path| InternedPath::from_path_buf(&logical_path, string_table))
                    .map_err(|error| Box::new(compiler_error_to_diagnostic(&error)))
            })
            .transpose()?;

        if let Ok(relative) = module_root.strip_prefix(resolver.entry_root()) {
            let prefix_interned = InternedPath::from_path_buf(relative, string_table);
            module_root_boundaries.push(ModuleRootBoundary {
                import_prefix: prefix_interned,
                module_root: root_interned,
                export_file,
            });
        }
    }

    Ok(module_root_boundaries)
}
