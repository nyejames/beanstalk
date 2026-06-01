//! Header-stage import environment construction.
//!
//! WHAT: resolves parsed imports, aliases, facade boundaries, and external symbols into
//! file-local visibility maps.
//! WHY: dependency sorting and AST need stable per-file visibility without rebuilding import
//! semantics in later stages.
//! MUST NOT: parse executable bodies, fold constants, or perform AST semantic validation.

mod bindings;
mod diagnostics;
mod external_imports;
mod facade_resolution;
mod namespace_imports;
mod provider_imports;
mod receiver_imports;
mod source_imports;
mod target_resolution;
mod visible_names;

pub(crate) use bindings::{
    FileVisibility, HeaderImportEnvironment, NamespaceRecord, NamespaceRecordSource,
    NamespaceTypeMember, NamespaceValueMember, ReceiverMethodVisibility,
};
pub(crate) use facade_resolution::{
    FacadeLookupResult, FacadeResolutionInput, resolve_facade_import,
};

pub(crate) use target_resolution::{
    ExportRequirement, ExternalPackageSymbolLookup, ExternalPackageSymbolResolutionInput,
    ImportTargetResolutionInput, NamespaceTargetResolutionInput, ResolvedImportTarget,
    ResolvedNamespaceTarget, has_explicit_bst_extension, resolve_external_package_symbol,
    resolve_import_target, resolve_namespace_target,
};
pub(crate) use visible_names::{
    ReceiverMethodImportTarget, VisibleNameBinding, VisibleNameRegistry, check_alias_case_warning,
};

use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, ImportFacadeType};
use crate::compiler_frontend::external_packages::{ExternalFunctionId, ExternalPackageRegistry};
use crate::compiler_frontend::headers::module_symbols::ModuleSymbols;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::source_libraries::mod_file::import_path_references_special_file;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use rustc_hash::FxHashSet;

/// Input bundle for preparing the module-wide import environment.
///
/// WHY: replaces the long parameter list of the old AST-side import resolver with one named struct.
pub(crate) struct ImportEnvironmentInput<'a> {
    pub(crate) module_symbols: &'a mut ModuleSymbols,
    pub(crate) external_package_registry: &'a ExternalPackageRegistry,
    pub(crate) external_import_resolution_table: &'a ExternalImportResolutionTable,
    pub(crate) string_table: &'a mut StringTable,
}

/// Build the header-stage import environment for all parsed source files.
///
/// WHAT: builds per-file visibility maps by registering same-file declarations, prelude/builtin
/// names, and resolved imports.
/// WHY: this is the single entry point that AST will call to receive prepared visibility.
/// BOUNDARY: returns `CompilerMessages` because this is a true build boundary that carries the
/// shared `StringTable` needed for rendering and downstream transport. Inner helpers use
/// `Result<..., CompilerDiagnostic>` to avoid repeated `StringTable` cloning; conversion happens
/// only at this top-level boundary.
pub(crate) fn prepare_import_environment(
    input: ImportEnvironmentInput<'_>,
) -> Result<HeaderImportEnvironment, CompilerMessages> {
    let importable_symbol_paths: FxHashSet<_> = input
        .module_symbols
        .importable_symbol_exported
        .keys()
        .cloned()
        .collect();

    let mut builder = ImportEnvironmentBuilder {
        module_symbols: input.module_symbols,
        external_package_registry: input.external_package_registry,
        external_import_resolution_table: input.external_import_resolution_table,
        string_table: input.string_table,
        environment: HeaderImportEnvironment::default(),
        warnings: Vec::new(),
        pending_receiver_validations: Vec::new(),
    };

    for source_file in input.module_symbols.module_file_paths.clone() {
        if let Err(diag) = builder.build_file_visibility(&source_file, &importable_symbol_paths) {
            return Err(CompilerMessages::from_diagnostic(
                diag,
                builder.string_table.clone(),
            ));
        }
    }

    // CRITICAL: propagate collected warnings into the environment so downstream stages see them.
    builder.environment.warnings = builder.warnings;
    Ok(builder.environment)
}

/// One explicit grouped receiver-method import that needs receiver-type visibility validation.
///
/// WHY: header import preparation validates receiver-type visibility after all imports in the
///      file are processed, so that `import @surface { method, Type }` succeeds regardless of
///      entry order within the grouped block.
#[derive(Clone, Debug)]
struct PendingReceiverMethodValidation {
    local_name: StringId,
    source_path: Option<InternedPath>,
    external_function_id: Option<ExternalFunctionId>,
    location: SourceLocation,
}

struct ImportEnvironmentBuilder<'a> {
    pub(super) module_symbols: &'a ModuleSymbols,
    pub(super) external_package_registry: &'a ExternalPackageRegistry,
    pub(super) external_import_resolution_table: &'a ExternalImportResolutionTable,
    pub(super) string_table: &'a mut StringTable,
    pub(super) environment: HeaderImportEnvironment,
    pub(super) warnings: Vec<crate::compiler_frontend::compiler_messages::CompilerDiagnostic>,
    pub(super) pending_receiver_validations: Vec<PendingReceiverMethodValidation>,
}

impl<'a> ImportEnvironmentBuilder<'a> {
    // ------------------------------
    //  Import helper methods
    // ------------------------------

    /// Derive the local binding name for an import.
    fn derive_import_local_name(
        &self,
        import: &FileImport,
    ) -> Result<StringId, CompilerDiagnostic> {
        match import.alias {
            Some(alias) => Ok(alias),
            None => match import.header_path.name() {
                Some(name) => Ok(name),
                None => Err(diagnostics::missing_import_target_no_path(
                    import.location.clone(),
                )),
            },
        }
    }

    /// Emit an alias-case warning when an explicit alias changes leading case.
    fn emit_alias_case_warning_if_needed(&mut self, import: &FileImport, symbol_name: StringId) {
        let Some(alias) = import.alias else {
            return;
        };
        if let Some(warning) = check_alias_case_warning(
            &import.alias_location,
            &import.path_location,
            alias,
            symbol_name,
            self.string_table,
        ) {
            self.warnings.push(warning);
        }
    }

    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    fn build_file_visibility(
        &mut self,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> Result<(), CompilerDiagnostic> {
        let mut file_visibility = FileVisibility::default();
        let mut registry = VisibleNameRegistry::new();
        self.pending_receiver_validations.clear();

        // 1. Register same-file declarations.
        if let Some(declared_paths) = self.module_symbols.declared_paths_by_file.get(source_file) {
            for path in declared_paths {
                file_visibility
                    .visible_declaration_paths
                    .insert(path.clone());

                let Some(name) = path.name() else {
                    continue;
                };

                if self.module_symbols.receiver_method_paths.contains(path) {
                    // Same-file receiver methods reserve their spelling for collision
                    // checks, but they are not ordinary source names. AST resolves them
                    // through the receiver catalog so `method(value)` remains invalid
                    // while `value.method()` can dispatch by receiver type.
                    registry.register(
                        name,
                        VisibleNameBinding::ReceiverMethodImport {
                            target: ReceiverMethodImportTarget::SourceMethod {
                                canonical_path: path.clone(),
                            },
                        },
                        SourceLocation::default(),
                    )?;
                    Self::add_visible_receiver_method(
                        &mut file_visibility,
                        name,
                        path,
                        SourceLocation::default(),
                    );
                    continue;
                }

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

                registry.register(name, binding, SourceLocation::default())?;

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

        // 2. Register builtins.
        for path in &self.module_symbols.builtin_visible_symbol_paths {
            file_visibility
                .visible_declaration_paths
                .insert(path.clone());
            if let Some(name) = path.name() {
                registry.register(name, VisibleNameBinding::Builtin, SourceLocation::default())?;
                file_visibility
                    .visible_source_names
                    .insert(name, path.clone());
            }
        }

        // 3. Register prelude symbols in the registry so imports can detect collisions.
        // Mutation: prelude names are compiler-owned fixed symbols interned for name comparison.
        for (prelude_name, symbol_id) in self.external_package_registry.prelude_symbols_by_name() {
            let prelude_name_id = self.string_table.intern(prelude_name);
            registry.register(
                prelude_name_id,
                VisibleNameBinding::Prelude {
                    symbol_id: *symbol_id,
                },
                SourceLocation::default(),
            )?;
        }

        // 4. Resolve and register explicit imports.
        if let Some(imports) = self.module_symbols.file_imports_by_source.get(source_file) {
            for import in imports {
                // Reject direct imports of special files (#mod, #page, #config).
                if import_path_references_special_file(&import.header_path, self.string_table) {
                    return Err(diagnostics::direct_special_file_import(
                        &import.header_path,
                        import.location.clone(),
                    ));
                }

                if import.from_grouped {
                    // Grouped imports keep the existing facade → target resolution flow.
                    self.resolve_and_register_grouped_import(
                        &mut file_visibility,
                        &mut registry,
                        import,
                        source_file,
                        importable_symbol_paths,
                    )?;
                } else {
                    // Bare imports are namespace imports or direct symbol-path imports.
                    self.resolve_and_register_bare_import(
                        &mut file_visibility,
                        &mut registry,
                        import,
                        source_file,
                        importable_symbol_paths,
                    )?;
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

        // Sort receiver method paths for deterministic lookup ordering.
        // WHY: same method name from different sources must resolve consistently
        //      across compilations; lexicographic order by function path is stable.
        for paths in file_visibility.visible_receiver_methods.values_mut() {
            paths.sort_by(|a, b| {
                let a_str = a.function_path.to_string(self.string_table);
                let b_str = b.function_path.to_string(self.string_table);
                a_str.cmp(&b_str)
            });
        }

        for methods in file_visibility
            .visible_external_receiver_methods
            .values_mut()
        {
            methods.sort_by_key(|function_id| format!("{function_id:?}"));
        }

        self.validate_pending_receiver_methods(&file_visibility)?;
        self.pending_receiver_validations.clear();

        self.environment
            .file_visibility_by_source
            .insert(source_file.clone(), file_visibility);
        Ok(())
    }

    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    fn resolve_and_register_grouped_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> Result<(), CompilerDiagnostic> {
        // Check for provider-backed grouped import first.
        if let Some(resolved) = self.resolve_provider_backed_grouped_import(
            file_visibility,
            registry,
            import,
            source_file,
        )? {
            return Ok(resolved);
        }

        if let Some(resolved) = self.resolve_and_register_external_package_grouped_import(
            file_visibility,
            registry,
            import,
        )? {
            return Ok(resolved);
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
                    return self.register_source_import(
                        file_visibility,
                        registry,
                        &path,
                        import,
                        ExportRequirement::AlreadyValidatedByFacade,
                    );
                }
                FacadeLookupResult::NotExported {
                    facade_name,
                    facade_type,
                } => {
                    let facade_name_id = self.string_table.intern(&facade_name);
                    let diagnostic_facade_type = match facade_type {
                        facade_resolution::FacadeType::SourceLibrary => {
                            ImportFacadeType::SourceLibrary
                        }
                        facade_resolution::FacadeType::ModuleRoot => ImportFacadeType::ModuleRoot,
                    };
                    return Err(diagnostics::not_exported_by_facade(
                        &import.header_path,
                        facade_name_id,
                        diagnostic_facade_type,
                        import.location.clone(),
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
        })?;

        match target {
            ResolvedImportTarget::Source {
                symbol_path,
                export_requirement,
            } => {
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
                            file_module_membership: &self.module_symbols.file_module_membership,
                            module_root_facade_exports: &self
                                .module_symbols
                                .module_root_facade_exports,
                        },
                    )?;
                }

                let effective_requirement = if self.is_internal_import(source_file, &symbol_path) {
                    ExportRequirement::AlreadyValidatedByFacade
                } else {
                    export_requirement
                };

                self.register_source_import(
                    file_visibility,
                    registry,
                    &symbol_path,
                    import,
                    effective_requirement,
                )
            }
            ResolvedImportTarget::External { symbol_id } => {
                self.register_external_import(file_visibility, registry, import, symbol_id)
            }
        }
    }

    /// Resolve grouped virtual-package imports before source facade enforcement.
    ///
    /// WHAT: `import @web/canvas { get_canvas }` is parsed as a grouped import whose
    /// individual entry path is `web/canvas/get_canvas`. That path may also look like a
    /// module-root facade import if the project has a `web/canvas/#mod.bst` shape. Checking
    /// external metadata here keeps virtual packages out of source facade privacy rules while
    /// leaving all source imports on the normal facade-first path.
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    fn resolve_and_register_external_package_grouped_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
    ) -> Result<Option<()>, CompilerDiagnostic> {
        if !import.from_grouped {
            return Ok(None);
        }

        match resolve_external_package_symbol(ExternalPackageSymbolResolutionInput {
            import_path: &import.header_path,
            external_package_registry: self.external_package_registry,
            string_table: self.string_table,
        }) {
            ExternalPackageSymbolLookup::Found { symbol_id } => {
                self.register_external_import(file_visibility, registry, import, symbol_id)?;
                Ok(Some(()))
            }
            ExternalPackageSymbolLookup::PackageFoundSymbolMissing {
                package_path,
                symbol_name,
            } => Err(diagnostics::missing_package_symbol(
                symbol_name,
                package_path,
                import.location.clone(),
            )),
            ExternalPackageSymbolLookup::NoMatch => Ok(None),
        }
    }

    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    fn resolve_and_register_bare_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> Result<(), CompilerDiagnostic> {
        // Reject explicit `.bst` extension in import paths.
        if has_explicit_bst_extension(&import.header_path, self.string_table) {
            return Err(CompilerDiagnostic::explicit_bst_extension(
                import.header_path.clone(),
                import.location.clone(),
            ));
        }

        // Check for provider-backed bare import.
        if let Some(resolved) = self.resolve_provider_backed_bare_import(
            file_visibility,
            registry,
            import,
            source_file,
        )? {
            return Ok(resolved);
        }

        // Try namespace resolution first. Facade namespaces must be checked before concrete
        // file/package resolution so `import @module` exposes `module/#mod.bst`, not a private
        // implementation path or a missing direct symbol.
        let namespace_target = self
            .resolve_facade_namespace_target(import, source_file)
            .or_else(|| {
                resolve_namespace_target(NamespaceTargetResolutionInput {
                    import_path: &import.header_path,
                    module_file_paths: &self.module_symbols.module_file_paths,
                    external_package_registry: self.external_package_registry,
                    string_table: self.string_table,
                })
            });

        if let Some(target) = namespace_target {
            return self.register_namespace_import(
                file_visibility,
                registry,
                import,
                source_file,
                target,
            );
        }

        // Namespace resolution failed. Try normal target resolution to detect
        // direct symbol-path imports that are now invalid.
        let target = resolve_import_target(ImportTargetResolutionInput {
            import_path: &import.header_path,
            location: &import.location,
            module_file_paths: &self.module_symbols.module_file_paths,
            importable_symbol_paths,
            external_package_registry: self.external_package_registry,
            string_table: self.string_table,
        })?;

        // If normal resolution succeeds for a bare import, it's a direct symbol-path import.
        match target {
            ResolvedImportTarget::Source { symbol_path, .. } => Err(
                CompilerDiagnostic::direct_symbol_path_import(symbol_path, import.location.clone()),
            ),
            ResolvedImportTarget::External { .. } => {
                Err(CompilerDiagnostic::direct_symbol_path_import(
                    import.header_path.clone(),
                    import.location.clone(),
                ))
            }
        }
    }
}
