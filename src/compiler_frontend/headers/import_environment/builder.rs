//! Import environment builder implementation.
//!
//! WHAT: constructs per-file visibility maps by registering same-file declarations,
//!        prelude/builtin names, and resolved imports.
//! WHY: the builder holds mutable state across all files and performs the heavy lifting
//!      of import resolution; keeping it separate from the entry-point orchestration
//!      makes the module structure easier to navigate.

use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, ImportFacadeType};
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::module_symbols::{
    FacadeExportEntry, FacadeExportTarget, ModuleSymbols,
};
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::source_libraries::root_file::{
    import_path_references_config_file, import_path_references_hash_root_file,
};
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use crate::libraries::SourceFileKind;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use rustc_hash::{FxHashMap, FxHashSet};

use super::{
    ExternalPackageSymbolLookup, ExternalPackageSymbolResolutionInput, FacadeLookupResult,
    FacadeResolutionInput, FileVisibility, HeaderImportEnvironment, ImportTargetResolutionInput,
    ModuleBoundaryCheckInput, NamespaceRecordSource, NamespaceTargetResolutionInput,
    ResolvedImportTarget, SourceImportAccess, SourceLibraryBoundaryCheckInput, VisibleNameBinding,
    VisibleNameRegistry, check_alias_case_warning, check_module_boundary,
    check_source_library_boundary, has_explicit_bst_extension, resolve_external_package_symbol,
    resolve_facade_import, resolve_import_target, resolve_namespace_target,
};

/// Boxed diagnostic result for the import-environment builder family.
///
/// WHAT: gives visibility construction and its local resolution helpers one small error boundary.
/// WHY: import resolution passes structured diagnostics through several recursive helpers
///      without carrying the large value inline at every return.
type BuilderResult<T> = Result<T, Box<CompilerDiagnostic>>;

pub(crate) struct ImportEnvironmentBuilder<'a> {
    pub(super) module_symbols: &'a ModuleSymbols,
    pub(super) external_package_registry: &'a ExternalPackageRegistry,
    pub(super) external_import_resolution_table: &'a ExternalImportResolutionTable,
    pub(super) string_table: &'a mut StringTable,
    pub(super) environment: HeaderImportEnvironment,
    pub(super) warnings: Vec<crate::compiler_frontend::compiler_messages::CompilerDiagnostic>,
}

impl<'a> ImportEnvironmentBuilder<'a> {
    // ------------------------------
    //  Import helper methods
    // ------------------------------

    /// Derive the local binding name for an import.
    pub(super) fn derive_import_local_name(&self, import: &FileImport) -> BuilderResult<StringId> {
        match import.alias {
            Some(alias) => Ok(alias),
            None => match import.header_path.name() {
                Some(name) => Ok(name),
                None => Err(Box::new(super::diagnostics::missing_import_target_no_path(
                    import.location.clone(),
                ))),
            },
        }
    }

    /// Emit an alias-case warning when an explicit alias changes leading case.
    pub(super) fn emit_alias_case_warning_if_needed(
        &mut self,
        import: &FileImport,
        symbol_name: StringId,
    ) {
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

    /// Whether two source files share the same non-facade import boundary.
    ///
    /// WHAT: source-library members, same module-root members, and files in the implicit entry
    /// module can see each other's ordinary source declarations directly.
    /// WHY: grouped source imports and namespace imports both need the same boundary answer
    /// before deciding whether receiver methods may travel with the imported surface.
    pub(super) fn source_files_share_import_boundary(
        &self,
        importer_file: &InternedPath,
        target_file: &InternedPath,
    ) -> bool {
        let importer_library = self
            .module_symbols
            .file_library_membership
            .get(importer_file);
        let target_library = self.module_symbols.file_library_membership.get(target_file);
        if importer_library == target_library && importer_library.is_some() {
            return true;
        }

        let importer_module = self
            .module_symbols
            .file_module_membership
            .get(importer_file);
        let target_module = self.module_symbols.file_module_membership.get(target_file);
        if importer_module == target_module && importer_module.is_some() {
            return true;
        }

        let importer_has_explicit_module = importer_library.is_some() || importer_module.is_some();
        let target_has_explicit_module = target_library.is_some() || target_module.is_some();

        !importer_has_explicit_module && !target_has_explicit_module
    }

    pub(super) fn build_file_visibility(
        &mut self,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> BuilderResult<()> {
        let mut file_visibility = FileVisibility::default();
        let mut registry = VisibleNameRegistry::new();

        // Reserve compiler-owned core cast trait names before any source
        // declarations or imports can claim them. This lets the normal visible-
        // name collision path reject aliases, namespace names, and imported
        // source/export names that would shadow a core cast trait spelling.
        registry.reserve_core_cast_trait_names(self.string_table);

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
                    // Source receiver methods are receiver-call-only declarations. They do not
                    // reserve ordinary value/import names, because dispatch includes the receiver
                    // type and `method(value)` is diagnosed from the receiver catalog instead.
                    Self::add_visible_receiver_method(
                        &mut file_visibility,
                        name,
                        path,
                        SourceLocation::default(),
                    );
                    continue;
                }

                let is_type_alias = self.module_symbols.type_alias_paths.contains(path);
                let is_trait = self.module_symbols.trait_paths.contains(path);
                let binding = if is_type_alias {
                    VisibleNameBinding::TypeAlias {
                        canonical_path: path.clone(),
                    }
                } else if is_trait {
                    VisibleNameBinding::Trait {
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
                } else if is_trait {
                    file_visibility
                        .visible_trait_names
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

        // 4. Register prelude namespace aliases so they participate in collision detection
        // before explicit imports. The alias name points at an external package path, and the
        // resulting visible namespace record is built from the same path as an explicit
        // `import @package`.
        for (prelude_name, package_path) in self
            .external_package_registry
            .prelude_namespace_aliases_by_name()
        {
            let prelude_name_id = self.string_table.intern(prelude_name);
            let package_path_id = self.string_table.intern(package_path);
            registry.register(
                prelude_name_id,
                VisibleNameBinding::NamespaceRecord {
                    record_source: NamespaceRecordSource::ExternalPackage(package_path_id),
                },
                SourceLocation::default(),
            )?;
        }

        // 5. Resolve and register explicit imports.
        if let Some(imports) = self.module_symbols.file_imports_by_source.get(source_file) {
            for import in imports {
                // Reject direct imports of hash roots and canonical config files.
                if import_path_references_hash_root_file(
                    &import.header_path,
                    import.from_grouped,
                    self.string_table,
                ) || import_path_references_config_file(
                    &import.header_path,
                    import.from_grouped,
                    self.string_table,
                ) {
                    return Err(Box::new(super::diagnostics::direct_special_file_import(
                        &import.header_path,
                        import.location.clone(),
                    )));
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

        // 6. Inject unshadowed prelude symbols into visible maps.
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

                // Prelude symbols have no authored source location, so we record
                // a default location as a documented fallback.
                file_visibility
                    .visible_external_symbol_locations
                    .insert(prelude_name_id, SourceLocation::default());
            }
        }

        // 7. Inject unshadowed prelude namespace aliases into visible namespace records.
        // Aliases that are still registered as a namespace record with the same external
        // package target were not shadowed by same-file declarations, builtins, or imports
        // of a different target. Explicit imports of the same package already insert an
        // equivalent record, so we skip when the local name is already present.
        for (prelude_name, package_path) in self
            .external_package_registry
            .prelude_namespace_aliases_by_name()
        {
            let prelude_name_id = self.string_table.intern(prelude_name);
            let package_path_id = self.string_table.intern(package_path);
            if let Some(VisibleNameBinding::NamespaceRecord {
                record_source: NamespaceRecordSource::ExternalPackage(registered_package_path_id),
            }) = registry.get(prelude_name_id)
                && registered_package_path_id == &package_path_id
                && !file_visibility
                    .visible_namespace_records
                    .contains_key(&prelude_name_id)
            {
                let record = self
                    .build_external_namespace_record(package_path_id, &SourceLocation::default())?;
                file_visibility
                    .visible_namespace_records
                    .insert(prelude_name_id, record);
            }
        }

        // 8. Add Beandown's compiler-integrated implicit constant scope.
        // WHY: `.bd` bodies are synthetic constant initializers, so they need the same
        // file-local visibility maps as authored constants without a user-visible import record.
        self.register_implicit_beandown_constant_scope(&mut file_visibility, source_file);

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

        self.environment
            .file_visibility_by_source
            .insert(source_file.clone(), file_visibility);
        Ok(())
    }

    fn register_implicit_beandown_constant_scope(
        &self,
        file_visibility: &mut FileVisibility,
        source_file: &InternedPath,
    ) {
        if !self.is_beandown_source_file(source_file) {
            return;
        }

        let mut implicit_constants = FxHashMap::default();
        self.remove_beandown_generated_self_constants(file_visibility, source_file);

        // Layer 1: exported constants from the HTML source-library facade.
        self.collect_html_facade_constants(&mut implicit_constants);

        // Layer 2: exported constants from the exact same-directory facade. Later inserts
        // intentionally replace HTML names so local facade constants win on collisions.
        self.collect_same_directory_facade_constants(source_file, &mut implicit_constants);

        for (name, path) in implicit_constants {
            file_visibility
                .visible_declaration_paths
                .insert(path.clone());
            file_visibility.visible_source_names.insert(name, path);
        }
    }

    fn remove_beandown_generated_self_constants(
        &self,
        file_visibility: &mut FileVisibility,
        source_file: &InternedPath,
    ) {
        let Some((content_name, content_path)) = file_visibility
            .visible_source_names
            .iter()
            .find_map(|(name, path)| {
                if self.string_table.resolve(*name) != "content" {
                    return None;
                }

                if !self.module_symbols.constant_paths.contains(path) {
                    return None;
                }

                if self.symbol_origin_matches_source(path, source_file) {
                    Some((*name, path.clone()))
                } else {
                    None
                }
            })
        else {
            return;
        };

        file_visibility
            .visible_declaration_paths
            .remove(&content_path);
        if file_visibility.visible_source_names.get(&content_name) == Some(&content_path) {
            file_visibility.visible_source_names.remove(&content_name);
        }
    }

    fn collect_html_facade_constants(
        &self,
        implicit_constants: &mut FxHashMap<StringId, InternedPath>,
    ) {
        let Some(entries) = self.module_symbols.facade_exports.get("html") else {
            return;
        };

        self.collect_constant_exports(entries, implicit_constants, None);
    }

    fn collect_same_directory_facade_constants(
        &self,
        source_file: &InternedPath,
        implicit_constants: &mut FxHashMap<StringId, InternedPath>,
    ) {
        let Some(facade_file) = self.same_directory_facade_file(source_file) else {
            return;
        };

        if let Some(entries) = self.source_library_facade_exports_for_file(&facade_file) {
            self.collect_constant_exports(entries, implicit_constants, Some(source_file));
        }

        if let Some(entries) = self.module_root_facade_exports_for_file(&facade_file) {
            self.collect_constant_exports(entries, implicit_constants, Some(source_file));
        }
    }

    fn collect_constant_exports(
        &self,
        entries: &FxHashSet<FacadeExportEntry>,
        implicit_constants: &mut FxHashMap<StringId, InternedPath>,
        excluded_source_file: Option<&InternedPath>,
    ) {
        for entry in entries {
            let FacadeExportTarget::Source(path) = &entry.target else {
                continue;
            };

            if !self.module_symbols.constant_paths.contains(path) {
                continue;
            }

            if excluded_source_file
                .is_some_and(|source_file| self.symbol_origin_matches_source(path, source_file))
            {
                continue;
            }

            implicit_constants.insert(entry.export_name, path.clone());
        }
    }

    fn symbol_origin_matches_source(
        &self,
        symbol_path: &InternedPath,
        source_file: &InternedPath,
    ) -> bool {
        let Some(origin) = self
            .module_symbols
            .canonical_source_by_symbol_path
            .get(symbol_path)
        else {
            return false;
        };

        if origin == source_file {
            return true;
        }

        let Some(canonical_source_path) = self
            .module_symbols
            .canonical_os_path_by_source
            .get(source_file)
        else {
            return false;
        };

        origin.to_path_buf(self.string_table) == *canonical_source_path
    }

    fn is_beandown_source_file(&self, source_file: &InternedPath) -> bool {
        let Some(path) = self
            .module_symbols
            .canonical_os_path_by_source
            .get(source_file)
        else {
            return source_file
                .to_path_buf(self.string_table)
                .extension()
                .and_then(|extension| extension.to_str())
                .and_then(SourceFileKind::from_extension)
                == Some(SourceFileKind::Beandown);
        };

        path.extension()
            .and_then(|extension| extension.to_str())
            .and_then(SourceFileKind::from_extension)
            == Some(SourceFileKind::Beandown)
    }

    fn same_directory_facade_file(&self, source_file: &InternedPath) -> Option<InternedPath> {
        let beandown_directory = self.source_directory(source_file)?;

        self.module_symbols
            .file_roles_by_source
            .iter()
            .find_map(|(candidate_source, role)| {
                if !role.is_export_capable() {
                    return None;
                }

                let candidate_directory = self.source_directory(candidate_source)?;
                if candidate_directory == beandown_directory {
                    Some(candidate_source.clone())
                } else {
                    None
                }
            })
    }

    fn source_directory(&self, source_file: &InternedPath) -> Option<std::path::PathBuf> {
        if let Some(path) = self
            .module_symbols
            .canonical_os_path_by_source
            .get(source_file)
        {
            return path.parent().map(|parent| parent.to_path_buf());
        }

        source_file
            .parent()
            .map(|parent| parent.to_path_buf(self.string_table))
    }

    fn source_library_facade_exports_for_file(
        &self,
        facade_file: &InternedPath,
    ) -> Option<&FxHashSet<FacadeExportEntry>> {
        let prefix = self
            .module_symbols
            .source_library_facade_files
            .iter()
            .find_map(|(prefix, source)| {
                if source == facade_file {
                    Some(prefix)
                } else {
                    None
                }
            })?;

        self.module_symbols.facade_exports.get(prefix)
    }

    fn module_root_facade_exports_for_file(
        &self,
        facade_file: &InternedPath,
    ) -> Option<&FxHashSet<FacadeExportEntry>> {
        let module_root = self
            .module_symbols
            .file_module_membership
            .get(facade_file)?;

        self.module_symbols
            .module_root_facade_exports
            .get(module_root)
    }

    fn resolve_and_register_grouped_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> BuilderResult<()> {
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
            module_root_boundaries: &self.module_symbols.module_root_boundaries,
            string_table: self.string_table,
        };

        if let Some(facade_result) = resolve_facade_import(&facade_input) {
            match facade_result {
                FacadeLookupResult::ExportedSource {
                    path,
                    exported_entries,
                } => {
                    return self.register_source_import(
                        file_visibility,
                        registry,
                        &path,
                        import,
                        SourceImportAccess::Facade { exported_entries },
                    );
                }
                FacadeLookupResult::ExportedExternal { symbol_id } => {
                    return self.register_external_import(
                        file_visibility,
                        registry,
                        import,
                        symbol_id,
                    );
                }
                FacadeLookupResult::NotExported {
                    facade_name,
                    facade_type,
                } => {
                    let facade_name_id = self.string_table.intern(&facade_name);
                    let diagnostic_facade_type = match facade_type {
                        super::facade_resolution::FacadeType::SourceLibrary => {
                            ImportFacadeType::SourceLibrary
                        }
                        super::facade_resolution::FacadeType::ModuleRoot => {
                            ImportFacadeType::ModuleRoot
                        }
                    };
                    return Err(Box::new(super::diagnostics::not_exported_by_facade(
                        &import.header_path,
                        facade_name_id,
                        diagnostic_facade_type,
                        import.location.clone(),
                    )));
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
                access,
            } => {
                if let Some(target_file) = self
                    .module_symbols
                    .canonical_source_by_symbol_path
                    .get(&symbol_path)
                {
                    check_source_library_boundary(SourceLibraryBoundaryCheckInput {
                        importer_file: source_file,
                        target_file,
                        requested_path: &import.header_path,
                        location: import.location.clone(),
                        file_library_membership: &self.module_symbols.file_library_membership,
                        source_library_facade_files: &self
                            .module_symbols
                            .source_library_facade_files,
                        string_table: self.string_table,
                    })?;
                    check_module_boundary(ModuleBoundaryCheckInput {
                        importer_file: source_file,
                        target_file,
                        symbol_path: &symbol_path,
                        location: import.location.clone(),
                        file_module_membership: &self.module_symbols.file_module_membership,
                        module_root_facade_exports: &self.module_symbols.module_root_facade_exports,
                    })?;
                }

                let effective_requirement = if self.is_internal_import(source_file, &symbol_path) {
                    SourceImportAccess::Internal
                } else {
                    access
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
    fn resolve_and_register_external_package_grouped_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
    ) -> BuilderResult<Option<()>> {
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
            } => Err(Box::new(super::diagnostics::missing_package_symbol(
                symbol_name,
                package_path,
                import.location.clone(),
            ))),
            ExternalPackageSymbolLookup::NoMatch => Ok(None),
        }
    }

    fn resolve_and_register_bare_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
        importable_symbol_paths: &FxHashSet<InternedPath>,
    ) -> BuilderResult<()> {
        // Reject explicit `.bst` extension in import paths.
        if has_explicit_bst_extension(&import.header_path, self.string_table) {
            return Err(Box::new(CompilerDiagnostic::explicit_bst_extension(
                import.header_path.clone(),
                import.location.clone(),
            )));
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
            ResolvedImportTarget::Source { symbol_path, .. } => Err(Box::new(
                CompilerDiagnostic::direct_symbol_path_import(symbol_path, import.location.clone()),
            )),
            ResolvedImportTarget::External { .. } => {
                Err(Box::new(CompilerDiagnostic::direct_symbol_path_import(
                    import.header_path.clone(),
                    import.location.clone(),
                )))
            }
        }
    }
}
