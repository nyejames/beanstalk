//! Header-stage import environment construction.
//!
//! WHAT: resolves parsed imports, re-exports, aliases, facade boundaries, and external symbols
//! into file-local visibility maps.
//! WHY: dependency sorting and AST need stable per-file visibility without rebuilding import
//! semantics in later stages.
//! MUST NOT: parse executable bodies, fold constants, or perform AST semantic validation.

mod bindings;
mod diagnostics;
mod facade_resolution;
mod re_exports;
mod target_resolution;
mod visible_names;

pub(crate) use bindings::{FileVisibility, HeaderImportEnvironment};
pub(crate) use facade_resolution::{
    FacadeLookupResult, FacadeResolutionInput, resolve_facade_import,
};
pub(crate) use re_exports::{ReExportResolutionInput, resolve_re_exports};
pub(crate) use target_resolution::{
    ExportRequirement, ImportTargetResolutionInput, ResolvedImportTarget, resolve_import_target,
};
pub(crate) use visible_names::{VisibleNameBinding, VisibleNameRegistry, check_alias_case_warning};

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::external_packages::{ExternalPackageRegistry, ExternalSymbolId};
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::source_libraries::mod_file::import_path_references_mod_file;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashSet;

/// Input bundle for preparing the module-wide import environment.
///
/// WHY: replaces the long parameter list of the old AST-side import resolver with one named struct.
pub(crate) struct ImportEnvironmentInput<'a> {
    pub(crate) module_symbols: &'a mut ModuleSymbols,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) string_table: &'a mut StringTable,
}

/// Build the header-stage import environment for all parsed source files.
///
/// WHAT: orchestrates re-export resolution, then builds per-file visibility maps by registering
/// same-file declarations, prelude/builtin names, and resolved imports.
/// WHY: this is the single entry point that AST will call to receive prepared visibility.
pub(crate) fn prepare_import_environment(
    input: ImportEnvironmentInput<'_>,
) -> Result<HeaderImportEnvironment, CompilerMessages> {
    // Resolve re-exports first so facade exports are available for cross-library imports.
    let re_export_warnings = resolve_re_exports(ReExportResolutionInput {
        module_symbols: input.module_symbols,
        external_package_registry: input.external_package_registry,
        string_table: input.string_table,
    })
    .map_err(|error| CompilerMessages::from_error(error, input.string_table.clone()))?;

    let importable_symbol_paths: FxHashSet<_> = input
        .module_symbols
        .importable_symbol_exported
        .keys()
        .cloned()
        .collect();

    let mut builder = ImportEnvironmentBuilder {
        module_symbols: input.module_symbols,
        external_package_registry: input.external_package_registry,
        string_table: input.string_table,
        environment: HeaderImportEnvironment::default(),
        warnings: re_export_warnings,
    };

    for source_file in input.module_symbols.module_file_paths.clone() {
        builder.build_file_visibility(&source_file, &importable_symbol_paths)?;
    }

    // CRITICAL: propagate collected warnings into the environment so downstream stages see them.
    builder.environment.warnings = builder.warnings;
    Ok(builder.environment)
}

struct ImportEnvironmentBuilder<'a> {
    module_symbols: &'a ModuleSymbols,
    external_package_registry: &'a ExternalPackageRegistry,
    string_table: &'a mut StringTable,
    environment: HeaderImportEnvironment,
    warnings: Vec<crate::compiler_frontend::compiler_warnings::CompilerWarning>,
}

impl<'a> ImportEnvironmentBuilder<'a> {
    fn build_file_visibility(
        &mut self,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> Result<(), CompilerMessages> {
        let mut file_visibility = FileVisibility::default();
        let mut registry = VisibleNameRegistry::new();

        // 1. Register same-file declarations.
        if let Some(declared_paths) = self.module_symbols.declared_paths_by_file.get(source_file) {
            for path in declared_paths {
                file_visibility
                    .visible_declaration_paths
                    .insert(path.clone());

                let is_type_alias = self.module_symbols.type_alias_paths.contains(path);
                let binding = if is_type_alias {
                    VisibleNameBinding::TypeAlias {
                        canonical_path: path.clone(),
                    }
                } else {
                    VisibleNameBinding::SameFileDeclaration {
                        declaration_path: path.clone(),
                    }
                };

                if let Some(name) = path.name() {
                    registry
                        .register(name, binding, SourceLocation::default(), self.string_table)
                        .map_err(|error| {
                            CompilerMessages::from_error(error, self.string_table.clone())
                        })?;

                    if is_type_alias {
                        file_visibility
                            .visible_type_alias_names
                            .insert(name, path.clone());
                    } else {
                        file_visibility
                            .visible_source_names
                            .insert(name, path.clone());
                    }
                }
            }
        }

        // 2. Register builtins.
        for path in &self.module_symbols.builtin_visible_symbol_paths {
            file_visibility
                .visible_declaration_paths
                .insert(path.clone());
            if let Some(name) = path.name() {
                registry
                    .register(
                        name,
                        VisibleNameBinding::Builtin,
                        SourceLocation::default(),
                        self.string_table,
                    )
                    .map_err(|error| {
                        CompilerMessages::from_error(error, self.string_table.clone())
                    })?;
                file_visibility
                    .visible_source_names
                    .insert(name, path.clone());
            }
        }

        // 3. Register prelude symbols in the registry so imports can detect collisions.
        for (prelude_name, symbol_id) in self.external_package_registry.prelude_symbols_by_name() {
            let prelude_name_id = self.string_table.intern(prelude_name);
            registry
                .register(
                    prelude_name_id,
                    VisibleNameBinding::Prelude {
                        symbol_id: *symbol_id,
                    },
                    SourceLocation::default(),
                    self.string_table,
                )
                .map_err(|error| CompilerMessages::from_error(error, self.string_table.clone()))?;
        }

        // 4. Resolve and register explicit imports.
        if let Some(imports) = self.module_symbols.file_imports_by_source.get(source_file) {
            for import in imports {
                // Reject direct mod file imports.
                if import_path_references_mod_file(&import.header_path, self.string_table) {
                    return Err(CompilerMessages::from_error(
                        diagnostics::direct_mod_file_import(
                            &import.header_path,
                            import.location.clone(),
                            self.string_table,
                        ),
                        self.string_table.clone(),
                    ));
                }

                // Try facade resolution first.
                let facade_input = FacadeResolutionInput {
                    importer_file: source_file,
                    header_path: &import.header_path,
                    facade_exports: &self.module_symbols.facade_exports,
                    file_library_membership: &self.module_symbols.file_library_membership,
                    module_root_facade_exports: &self.module_symbols.module_root_facade_exports,
                    file_module_membership: &self.module_symbols.file_module_membership,
                    module_root_prefixes: &self.module_symbols.module_root_prefixes,
                    string_table: self.string_table,
                };

                if let Some(facade_result) = resolve_facade_import(&facade_input) {
                    match facade_result {
                        FacadeLookupResult::ExportedSource(path) => {
                            self.register_source_import(
                                &mut file_visibility,
                                &mut registry,
                                source_file,
                                &path,
                                import,
                                ExportRequirement::AlreadyValidatedByFacade,
                            )?;
                            continue;
                        }
                        FacadeLookupResult::ExportedExternal(symbol_id) => {
                            self.register_external_import(
                                &mut file_visibility,
                                &mut registry,
                                import,
                                symbol_id,
                            )?;
                            continue;
                        }
                        FacadeLookupResult::NotExported {
                            facade_name,
                            facade_type,
                        } => {
                            return Err(CompilerMessages::from_error(
                                diagnostics::not_exported_by_facade(
                                    &import.header_path,
                                    &facade_name,
                                    facade_type,
                                    import.location.clone(),
                                    self.string_table,
                                ),
                                self.string_table.clone(),
                            ));
                        }
                        FacadeLookupResult::NotAFacadeImport => {
                            // Fall through to normal target resolution.
                        }
                    }
                }

                // Normal target resolution.
                let target = resolve_import_target(ImportTargetResolutionInput {
                    import_path: &import.header_path,
                    location: &import.location,
                    module_file_paths: &self.module_symbols.module_file_paths,
                    importable_symbol_paths,
                    external_package_registry: self.external_package_registry,
                    string_table: self.string_table,
                })
                .map_err(|error| CompilerMessages::from_error(error, self.string_table.clone()))?;

                match target {
                    ResolvedImportTarget::Source {
                        symbol_path,
                        export_requirement,
                    } => {
                        // Check module boundary for cross-module-root imports.
                        if let Some(target_file) = self
                            .module_symbols
                            .canonical_source_by_symbol_path
                            .get(&symbol_path)
                        {
                            facade_resolution::check_module_boundary(
                                facade_resolution::ModuleBoundaryCheckInput {
                                    importer_file: source_file,
                                    target_file,
                                    symbol_path: &symbol_path,
                                    location: import.location.clone(),
                                    file_module_membership: &self
                                        .module_symbols
                                        .file_module_membership,
                                    module_root_facade_exports: &self
                                        .module_symbols
                                        .module_root_facade_exports,
                                    string_table: self.string_table,
                                },
                            )
                            .map_err(|error| {
                                CompilerMessages::from_error(error, self.string_table.clone())
                            })?;
                        }

                        self.register_source_import(
                            &mut file_visibility,
                            &mut registry,
                            source_file,
                            &symbol_path,
                            import,
                            export_requirement,
                        )?;
                    }
                    ResolvedImportTarget::External { symbol_id } => {
                        self.register_external_import(
                            &mut file_visibility,
                            &mut registry,
                            import,
                            symbol_id,
                        )?;
                    }
                }
            }
        }

        // 5. Inject unshadowed prelude symbols into visible maps.
        // Prelude entries that are still registered as Prelude were not shadowed by imports
        // or declarations with different targets.
        for (prelude_name, symbol_id) in self.external_package_registry.prelude_symbols_by_name() {
            let prelude_name_id = self.string_table.intern(prelude_name);
            if let Some(VisibleNameBinding::Prelude {
                symbol_id: registered_id,
            }) = registry.get(prelude_name_id)
                && registered_id == symbol_id
            {
                file_visibility
                    .visible_external_symbols
                    .insert(prelude_name_id, *symbol_id);
            }
        }

        self.environment
            .file_visibility_by_source
            .insert(source_file.clone(), file_visibility);
        Ok(())
    }

    fn register_source_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        _source_file: &InternedPath,
        symbol_path: &InternedPath,
        import: &FileImport,
        export_requirement: ExportRequirement,
    ) -> Result<(), CompilerMessages> {
        // Check export requirement.
        if matches!(
            export_requirement,
            ExportRequirement::MustBeExportedFromSourceFile
        ) {
            let is_exported = self
                .module_symbols
                .importable_symbol_exported
                .get(symbol_path)
                .copied()
                .unwrap_or(false);
            if !is_exported {
                return Err(CompilerMessages::from_error(
                    diagnostics::not_exported_by_source_file(
                        symbol_path,
                        import.location.clone(),
                        self.string_table,
                    ),
                    self.string_table.clone(),
                ));
            }
        }

        let local_name = match import.alias {
            Some(alias) => alias,
            None => match import.header_path.name() {
                Some(name) => name,
                None => {
                    return Err(CompilerMessages::from_error(
                        CompilerError::compiler_error("Import path is missing a symbol name."),
                        self.string_table.clone(),
                    ));
                }
            },
        };

        // Alias case warning — only for explicit aliases.
        if import.alias.is_some()
            && let Some(symbol_name) = symbol_path.name()
            && let Some(warning) = check_alias_case_warning(
                &import.alias_location,
                &import.path_location,
                local_name,
                symbol_name,
                self.string_table,
            )
        {
            self.warnings.push(warning);
        }

        file_visibility
            .visible_declaration_paths
            .insert(symbol_path.clone());

        let is_type_alias = self.module_symbols.type_alias_paths.contains(symbol_path);
        let binding = if is_type_alias {
            VisibleNameBinding::TypeAlias {
                canonical_path: symbol_path.clone(),
            }
        } else {
            VisibleNameBinding::SourceImport {
                canonical_path: symbol_path.clone(),
            }
        };

        registry
            .register(
                local_name,
                binding,
                import.location.clone(),
                self.string_table,
            )
            .map_err(|error| CompilerMessages::from_error(error, self.string_table.clone()))?;

        if is_type_alias {
            file_visibility
                .visible_type_alias_names
                .insert(local_name, symbol_path.clone());
        } else {
            file_visibility
                .visible_source_names
                .insert(local_name, symbol_path.clone());
        }

        Ok(())
    }

    fn register_external_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        symbol_id: ExternalSymbolId,
    ) -> Result<(), CompilerMessages> {
        let local_name = match import.alias {
            Some(alias) => alias,
            None => match import.header_path.name() {
                Some(name) => name,
                None => {
                    return Err(CompilerMessages::from_error(
                        CompilerError::compiler_error("Import path is missing a symbol name."),
                        self.string_table.clone(),
                    ));
                }
            },
        };

        // Alias case warning — only for explicit aliases.
        if import.alias.is_some()
            && let Some(symbol_name) = import.header_path.name()
            && let Some(warning) = check_alias_case_warning(
                &import.alias_location,
                &import.path_location,
                local_name,
                symbol_name,
                self.string_table,
            )
        {
            self.warnings.push(warning);
        }

        registry
            .register(
                local_name,
                VisibleNameBinding::ExternalImport { symbol_id },
                import.location.clone(),
                self.string_table,
            )
            .map_err(|error| CompilerMessages::from_error(error, self.string_table.clone()))?;

        file_visibility
            .visible_external_symbols
            .insert(local_name, symbol_id);

        Ok(())
    }
}
