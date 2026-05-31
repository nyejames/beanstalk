//! Namespace import registration and namespace record construction.
//!
//! WHAT: resolves bare imports into namespace records, validates facade boundaries for namespace
//! imports, and builds shallow field-access-only records from source files and external packages.
//! WHY: namespace imports are structurally different from grouped imports: they expose a record
//! surface rather than individual symbols, so their registration and record building is separate.
//! MUST NOT: register grouped imports or perform AST semantic validation.

use super::{
    FileVisibility, ImportEnvironmentBuilder, NamespaceRecord, NamespaceRecordSource,
    NamespaceTypeMember, NamespaceValueMember, ResolvedNamespaceTarget, VisibleNameBinding,
    VisibleNameRegistry,
};
use crate::compiler_frontend::compiler_messages::{CompilerDiagnostic, ImportFacadeType};
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::headers::module_symbols::GenericDeclarationKind;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::keywords::is_valid_identifier;
use crate::compiler_frontend::source_libraries::mod_file::MOD_FILE_NAME;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;
use rustc_hash::FxHashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SymbolKind {
    Value,
    Type,
}

impl<'a> ImportEnvironmentBuilder<'a> {
    /// Build and register a namespace import record.
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn register_namespace_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
        namespace_target: ResolvedNamespaceTarget,
    ) -> Result<(), CompilerDiagnostic> {
        let local_name = self.derive_namespace_name(import)?;

        let record = match namespace_target {
            ResolvedNamespaceTarget::SourceFile(ref file_path) => {
                self.validate_namespace_source_boundary(file_path, import, source_file)?;
                self.build_source_namespace_record(file_path, &import.location)?
            }
            ResolvedNamespaceTarget::ExternalPackage { package_path } => {
                self.build_external_namespace_record(package_path, &import.location)?
            }
        };

        let record_source = match &namespace_target {
            ResolvedNamespaceTarget::SourceFile(file_path) => {
                NamespaceRecordSource::SourceFile(file_path.clone())
            }
            ResolvedNamespaceTarget::ExternalPackage { package_path } => {
                NamespaceRecordSource::ExternalPackage(*package_path)
            }
        };

        registry.register(
            local_name,
            VisibleNameBinding::NamespaceRecord {
                record_source: record_source.clone(),
            },
            import.location.clone(),
        )?;

        file_visibility
            .visible_namespace_records
            .insert(local_name, record);

        // Namespace imports make receiver methods from the imported surface visible
        // through the receiver catalog under their original names.
        match &namespace_target {
            ResolvedNamespaceTarget::SourceFile(file_path) => {
                if let Some(declared_paths) =
                    self.module_symbols.declared_paths_by_file.get(file_path)
                {
                    for path in declared_paths {
                        if self.module_symbols.receiver_method_paths.contains(path)
                            && let Some(name) = path.name()
                        {
                            Self::add_visible_receiver_method(
                                file_visibility,
                                name,
                                path,
                                import.location.clone(),
                            );
                        }
                    }
                }
            }
            ResolvedNamespaceTarget::ExternalPackage { package_path } => {
                let package_path = self.string_table.resolve(*package_path).to_owned();
                self.add_external_receiver_methods_from_package(
                    file_visibility,
                    &package_path,
                    None,
                );
            }
        }

        Ok(())
    }

    /// Resolve a bare import that names a public facade namespace.
    ///
    /// WHAT: `import @library` and cross-module `import @module` expose the target `#mod.bst`
    /// surface as a namespace record.
    /// WHY: namespace imports must obey the same facade boundary as grouped imports.
    pub(super) fn resolve_facade_namespace_target(
        &mut self,
        import: &FileImport,
        source_file: &InternedPath,
    ) -> Option<ResolvedNamespaceTarget> {
        let components = import.header_path.as_components();
        if components.is_empty() {
            return None;
        }

        if let Some(target) = self.resolve_source_library_namespace_facade(components, source_file)
        {
            return Some(target);
        }

        self.resolve_module_root_namespace_facade(&import.header_path, source_file)
    }

    fn resolve_source_library_namespace_facade(
        &mut self,
        components: &[StringId],
        source_file: &InternedPath,
    ) -> Option<ResolvedNamespaceTarget> {
        if components.len() != 1 {
            return None;
        }

        let library_prefix = self.string_table.resolve(components[0]).to_owned();
        if !self
            .module_symbols
            .facade_exports
            .contains_key(&library_prefix)
        {
            return None;
        }

        let importer_library = self.module_symbols.file_library_membership.get(source_file);
        if importer_library.map(String::as_str) == Some(library_prefix.as_str()) {
            return None;
        }

        let facade_file = self
            .module_symbols
            .source_library_facade_files
            .get(&library_prefix)?
            .clone();

        self.module_symbols
            .module_file_paths
            .contains(&facade_file)
            .then_some(ResolvedNamespaceTarget::SourceFile(facade_file))
    }

    fn resolve_module_root_namespace_facade(
        &mut self,
        import_path: &InternedPath,
        source_file: &InternedPath,
    ) -> Option<ResolvedNamespaceTarget> {
        let effective_path = self.effective_module_import_path(import_path, source_file);

        for (prefix, module_root) in &self.module_symbols.module_root_prefixes {
            if &effective_path != prefix {
                continue;
            }

            let importer_root = self.module_symbols.file_module_membership.get(source_file);
            if importer_root == Some(module_root) {
                return None;
            }

            let facade_file = prefix.join_str(MOD_FILE_NAME, self.string_table);
            if self.module_symbols.module_file_paths.contains(&facade_file) {
                return Some(ResolvedNamespaceTarget::SourceFile(facade_file));
            }
        }

        None
    }

    fn effective_module_import_path(
        &self,
        import_path: &InternedPath,
        source_file: &InternedPath,
    ) -> InternedPath {
        let components = import_path.as_components();
        let Some(first) = components.first() else {
            return import_path.clone();
        };

        if self.string_table.resolve(*first) != "." {
            return import_path.clone();
        }

        let Some(importer_directory) = source_file.parent() else {
            return import_path.clone();
        };

        let mut combined = importer_directory.as_components().to_vec();
        combined.extend_from_slice(&components[1..]);
        InternedPath::from_components(combined)
    }

    /// Enforce facade privacy for concrete source-file namespace imports.
    fn validate_namespace_source_boundary(
        &mut self,
        target_file: &InternedPath,
        import: &FileImport,
        source_file: &InternedPath,
    ) -> Result<(), CompilerDiagnostic> {
        if let Some(target_library) = self
            .module_symbols
            .file_library_membership
            .get(target_file)
            .cloned()
        {
            let importer_library = self.module_symbols.file_library_membership.get(source_file);
            if importer_library.map(String::as_str) != Some(target_library.as_str()) {
                if self
                    .module_symbols
                    .source_library_facade_files
                    .get(&target_library)
                    .is_some_and(|facade_file| target_file == facade_file)
                {
                    return Ok(());
                }

                let facade_name_id = self.string_table.intern(&target_library);
                return Err(diagnostics::not_exported_by_facade(
                    &import.header_path,
                    facade_name_id,
                    ImportFacadeType::SourceLibrary,
                    import.location.clone(),
                ));
            }
        }

        let importer_root = self.module_symbols.file_module_membership.get(source_file);
        let target_root = self.module_symbols.file_module_membership.get(target_file);

        let (Some(importer_root), Some(target_root)) = (importer_root, target_root) else {
            return Ok(());
        };

        if importer_root == target_root {
            return Ok(());
        }

        if self
            .module_root_facade_logical_path(target_root)
            .is_some_and(|facade_file| target_file == &facade_file)
        {
            return Ok(());
        }

        if self
            .module_symbols
            .module_root_facade_exports
            .contains_key(target_root)
        {
            return Err(diagnostics::cross_module_import_not_exported(
                &import.header_path,
                import.location.clone(),
            ));
        }

        Err(diagnostics::missing_module_facade(
            &import.header_path,
            import.location.clone(),
        ))
    }

    fn module_root_facade_logical_path(
        &mut self,
        module_root: &InternedPath,
    ) -> Option<InternedPath> {
        self.module_symbols
            .module_root_prefixes
            .iter()
            .find_map(|(prefix, root)| {
                (root == module_root).then(|| prefix.join_str(MOD_FILE_NAME, self.string_table))
            })
    }

    /// Derive the local namespace name from an import, validating the default name.
    pub(super) fn derive_namespace_name(
        &mut self,
        import: &FileImport,
    ) -> Result<StringId, CompilerDiagnostic> {
        match import.alias {
            Some(alias) => Ok(alias),
            None => {
                let stem = import
                    .header_path
                    .name()
                    .map(|n| self.string_table.resolve(n).to_owned())
                    .unwrap_or_default();
                let stem = stem.strip_suffix(".js").unwrap_or(&stem);
                if stem.is_empty() || !is_valid_identifier(stem) {
                    return Err(CompilerDiagnostic::invalid_namespace_default_name(
                        import.header_path.clone(),
                        import.location.clone(),
                    ));
                }
                Ok(self.string_table.intern(stem))
            }
        }
    }

    /// Build a namespace record from a source file's visible declarations.
    fn build_source_namespace_record(
        &self,
        file_path: &InternedPath,
        location: &SourceLocation,
    ) -> Result<NamespaceRecord, CompilerDiagnostic> {
        let mut value_members = FxHashMap::default();
        let mut type_members = FxHashMap::default();

        let symbol_paths = self
            .module_symbols
            .declared_paths_by_file
            .get(file_path)
            .cloned()
            .unwrap_or_default();

        for symbol_path in symbol_paths {
            // Skip compiler-owned synthetic declarations (e.g. implicit start function).
            if !self
                .module_symbols
                .importable_symbol_exported
                .contains_key(&symbol_path)
            {
                continue;
            }

            // Receiver methods are not callable as namespace-record value fields.
            // They remain visible only through the receiver catalog.
            if self
                .module_symbols
                .receiver_method_paths
                .contains(&symbol_path)
            {
                continue;
            }

            let Some(name) = symbol_path.name() else {
                continue;
            };

            let kind = self.classify_symbol_kind(&symbol_path);
            match kind {
                SymbolKind::Type => {
                    type_members.insert(name, NamespaceTypeMember::SourceDeclaration(symbol_path));
                }
                SymbolKind::Value => {
                    value_members
                        .insert(name, NamespaceValueMember::SourceDeclaration(symbol_path));
                }
            }
        }

        self.check_duplicate_namespace_members(file_path, &value_members, &type_members, location)?;

        Ok(NamespaceRecord {
            value_members,
            type_members,
        })
    }

    /// Build a namespace record from an external package's symbols.
    pub(super) fn build_external_namespace_record(
        &mut self,
        package_path: StringId,
        location: &SourceLocation,
    ) -> Result<NamespaceRecord, CompilerDiagnostic> {
        let mut value_members = FxHashMap::default();
        let mut type_members = FxHashMap::default();

        let package_path_str = self.string_table.resolve(package_path).to_owned();
        let Some(package) = self
            .external_package_registry
            .get_package(&package_path_str)
        else {
            return Ok(NamespaceRecord {
                value_members,
                type_members,
            });
        };

        // Collect function names first to avoid borrowing `string_table` twice.
        let function_names: Vec<String> = package.functions.keys().cloned().collect();
        for name in function_names {
            let name_id = self.string_table.intern(&name);
            if let Some((function_id, def)) = self
                .external_package_registry
                .resolve_package_function(&package_path_str, &name)
            {
                // External receiver methods are not exposed as namespace-record value fields.
                // They are resolved through the external receiver lookup path.
                if def.receiver_type.is_some() {
                    continue;
                }
                value_members.insert(
                    name_id,
                    NamespaceValueMember::ExternalSymbol(ExternalSymbolId::Function(function_id)),
                );
            }
        }

        let type_names: Vec<String> = package.types.keys().cloned().collect();
        for name in type_names {
            let name_id = self.string_table.intern(&name);
            if let Some((type_id, _)) = self
                .external_package_registry
                .resolve_package_type(&package_path_str, &name)
            {
                type_members.insert(
                    name_id,
                    NamespaceTypeMember::ExternalSymbol(ExternalSymbolId::Type(type_id)),
                );
            }
        }

        let constant_names: Vec<String> = package.constants.keys().cloned().collect();
        for name in constant_names {
            let name_id = self.string_table.intern(&name);
            if let Some((constant_id, _)) = self
                .external_package_registry
                .resolve_package_constant(&package_path_str, &name)
            {
                value_members.insert(
                    name_id,
                    NamespaceValueMember::ExternalSymbol(ExternalSymbolId::Constant(constant_id)),
                );
            }
        }

        self.check_duplicate_namespace_members(
            &InternedPath::from_components(vec![package_path]),
            &value_members,
            &type_members,
            location,
        )?;

        Ok(NamespaceRecord {
            value_members,
            type_members,
        })
    }

    /// Classify a symbol path as a type or value member for namespace records.
    fn classify_symbol_kind(&self, symbol_path: &InternedPath) -> SymbolKind {
        if self.module_symbols.type_alias_paths.contains(symbol_path) {
            return SymbolKind::Type;
        }
        if self.module_symbols.nominal_type_paths.contains(symbol_path) {
            return SymbolKind::Type;
        }
        if let Some(metadata) = self
            .module_symbols
            .generic_declarations_by_path
            .get(symbol_path)
        {
            match metadata.kind {
                GenericDeclarationKind::Struct | GenericDeclarationKind::Choice => {
                    return SymbolKind::Type;
                }
                GenericDeclarationKind::Function => return SymbolKind::Value,
            }
        }
        // Non-generic functions and constants are value members.
        SymbolKind::Value
    }

    /// Check for same-spelling value/type collisions within one namespace surface.
    fn check_duplicate_namespace_members(
        &self,
        surface_path: &InternedPath,
        value_members: &FxHashMap<StringId, NamespaceValueMember>,
        type_members: &FxHashMap<StringId, NamespaceTypeMember>,
        location: &SourceLocation,
    ) -> Result<(), CompilerDiagnostic> {
        for name in value_members.keys() {
            if type_members.contains_key(name) {
                return Err(CompilerDiagnostic::duplicate_import_surface_member(
                    surface_path.clone(),
                    *name,
                    location.clone(),
                ));
            }
        }
        Ok(())
    }
}
