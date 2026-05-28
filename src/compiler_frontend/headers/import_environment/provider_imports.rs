//! Provider-backed import resolution helpers.
//!
//! WHAT: resolves grouped and bare imports against external files handled by registered import
//! providers (e.g., `.js` files parsed into typed external packages).
//! WHY: provider-backed imports bridge the Stage 0 external-file discovery path with the header
//! import environment, turning provider results into ordinary external-package registrations.
//! MUST NOT: perform provider parsing or AST-level semantic validation.

use super::{FileVisibility, ImportEnvironmentBuilder, VisibleNameRegistry};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringId;
use crate::libraries::external_import_providers::provider::ResolvedExternalImport;

impl<'a> ImportEnvironmentBuilder<'a> {
    /// Try to resolve a grouped import against a provider-backed external file.
    ///
    /// WHAT: Grouped items like `import @./drawing.js { draw }` are expanded into
    /// individual `FileImport`s with a normalized provider prefix such as
    /// `drawing.js` or `widgets/drawing.js`, followed by the requested symbol name.
    /// The symbol component is looked up in the provider-created external package.
    ///
    /// Returns `Ok(Some(()))` if resolved, `Ok(None)` if this import is not
    /// provider-backed, or `Err` for a diagnostic.
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn resolve_provider_backed_grouped_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
    ) -> Result<Option<()>, CompilerDiagnostic> {
        let Some((resolved, remaining)) =
            self.find_provider_resolution_with_remaining(source_file, &import.header_path)
        else {
            return Ok(None);
        };

        // Grouped imports must name exactly one symbol within the provider package.
        if remaining.len() != 1 {
            return Err(CompilerDiagnostic::direct_symbol_path_import(
                import.header_path.clone(),
                import.location.clone(),
            ));
        }

        let symbol_name = remaining[0];
        let package = self
            .external_package_registry
            .get_package_by_id(resolved.package_id);
        let Some(package) = package else {
            return Err(super::diagnostics::missing_import_target(
                &import.header_path,
                import.location.clone(),
            ));
        };

        let symbol_id = self
            .lookup_external_symbol_id_by_name(&package.path, symbol_name)
            .ok_or_else(|| {
                super::diagnostics::missing_import_target(
                    &import.header_path,
                    import.location.clone(),
                )
            })?;

        self.register_external_import(file_visibility, registry, import, symbol_id)?;

        Ok(Some(()))
    }

    /// Try to resolve a bare (namespace) import against a provider-backed external file.
    ///
    /// WHAT: `import @./helper.js` where `helper.js` has a registered provider.
    /// The import exposes the provider's package as a namespace record.
    ///
    /// Returns `Ok(Some(()))` if resolved, `Ok(None)` if not provider-backed,
    /// or `Err` for a diagnostic.
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn resolve_provider_backed_bare_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        source_file: &InternedPath,
    ) -> Result<Option<()>, CompilerDiagnostic> {
        let Some((resolved, remaining)) =
            self.find_provider_resolution_with_remaining(source_file, &import.header_path)
        else {
            return Ok(None);
        };

        // If there are remaining components after the provider-backed prefix, this is a
        // direct symbol-path import, which is invalid for bare imports.
        if !remaining.is_empty() {
            return Err(CompilerDiagnostic::direct_symbol_path_import(
                import.header_path.clone(),
                import.location.clone(),
            ));
        }

        let package = self
            .external_package_registry
            .get_package_by_id(resolved.package_id);
        let Some(package) = package else {
            return Err(super::diagnostics::missing_import_target(
                &import.header_path,
                import.location.clone(),
            ));
        };

        let package_path_id = self.string_table.intern(&package.path);
        let namespace_record =
            self.build_external_namespace_record(package_path_id, &import.location)?;

        let local_name = self.derive_namespace_name(import)?;

        registry.register(
            local_name,
            super::VisibleNameBinding::NamespaceRecord {
                record_source: super::NamespaceRecordSource::ExternalPackage(package_path_id),
            },
            import.location.clone(),
        )?;

        file_visibility
            .visible_namespace_records
            .insert(local_name, namespace_record);

        Ok(Some(()))
    }

    /// Look up a provider resolution and return the result plus any remaining
    /// path components after the provider-backed prefix.
    ///
    /// Walks the import path components building progressively longer prefixes.
    /// The longest matching provider-backed prefix wins. Remaining components
    /// (if any) are returned so the caller can decide whether to treat them as
    /// a symbol name or as an invalid direct import.
    fn find_provider_resolution_with_remaining(
        &self,
        source_file: &InternedPath,
        import_path: &InternedPath,
    ) -> Option<(ResolvedExternalImport, Vec<StringId>)> {
        let source_str = source_file.to_portable_string(self.string_table);
        let components = import_path.as_components();

        for prefix_len in (1..=components.len()).rev() {
            let prefix = InternedPath::from_components(components[..prefix_len].to_vec());
            let prefix_str = prefix.to_portable_string(self.string_table);
            if let Some(entry) = self
                .external_import_resolution_table
                .get(&source_str, &prefix_str)
            {
                let remaining = components[prefix_len..].to_vec();
                return Some((entry.clone(), remaining));
            }
        }

        None
    }

    /// Look up an external symbol ID by name within a provider-created package.
    fn lookup_external_symbol_id_by_name(
        &self,
        package_path: &str,
        name: StringId,
    ) -> Option<ExternalSymbolId> {
        let name_str = self.string_table.resolve(name);
        self.external_package_registry
            .resolve_package_symbol(package_path, name_str)
    }
}
