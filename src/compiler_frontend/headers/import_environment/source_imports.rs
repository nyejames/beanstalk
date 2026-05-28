//! Source import registration and source receiver auto-imports.
//!
//! WHAT: registers imports that resolve to source file declarations (same-module, cross-module,
//! source library) and auto-imports receiver methods when a struct type is imported.
//! WHY: source imports follow facade and export rules that differ from external package imports,
//! so they deserve their own focused registration path.
//! MUST NOT: register external package symbols or build namespace records.

use super::{
    ExportRequirement, FileVisibility, ImportEnvironmentBuilder, ReceiverMethodImportTarget,
    VisibleNameBinding, VisibleNameRegistry,
};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl<'a> ImportEnvironmentBuilder<'a> {
    /// Auto-import receiver methods for a struct type from the file where it is declared.
    ///
    /// WHAT: when a struct type is imported, all receiver methods declared in the same file
    ///       whose receiver matches that struct become visible through the receiver catalog.
    /// WHY: receiver methods travel with their receiver type on the same import surface.
    fn auto_import_receiver_methods_for_type(
        &self,
        file_visibility: &mut FileVisibility,
        struct_path: &InternedPath,
        target_file: &InternedPath,
    ) {
        let Some(struct_name) = struct_path.name() else {
            return;
        };

        // Walk receiver_method_paths directly and match by canonical source file.
        // WHY: canonical_source_by_symbol_path uses canonical OS paths while
        //      declared_paths_by_file uses header.source_file (logical/relative).
        //      Comparing against canonical_source_by_symbol_path avoids the mismatch.
        for path in &self.module_symbols.receiver_method_paths {
            if self.module_symbols.receiver_method_receiver_names.get(path) != Some(&struct_name) {
                continue;
            }

            if self
                .module_symbols
                .canonical_source_by_symbol_path
                .get(path)
                .is_some_and(|source_file| source_file == target_file)
                && let Some(name) = path.name()
            {
                Self::add_visible_receiver_method(
                    file_visibility,
                    name,
                    path,
                    SourceLocation::default(),
                );
            }
        }
    }

    /// Whether the importer and the target of an import are in the same module or source library.
    ///
    /// WHAT: same-module and same-library imports see all authored declarations by default;
    /// cross-module/cross-library imports must go through facade exports.
    /// WHY: this replaces the old `exported` boolean as the gate for same-module visibility.
    pub(super) fn is_internal_import(
        &self,
        importer_file: &InternedPath,
        symbol_path: &InternedPath,
    ) -> bool {
        let Some(target_file) = self
            .module_symbols
            .canonical_source_by_symbol_path
            .get(symbol_path)
        else {
            return false;
        };

        // Same source library?
        let importer_library = self
            .module_symbols
            .file_library_membership
            .get(importer_file);
        let target_library = self.module_symbols.file_library_membership.get(target_file);
        if importer_library == target_library && importer_library.is_some() {
            return true;
        }

        // Same module root?
        let importer_module = self
            .module_symbols
            .file_module_membership
            .get(importer_file);
        let target_module = self.module_symbols.file_module_membership.get(target_file);
        if importer_module == target_module && importer_module.is_some() {
            return true;
        }

        // If neither file belongs to an explicit library or module root, they are both in the
        // default entry-root module and can see each other's declarations.
        let importer_has_explicit_module = importer_library.is_some() || importer_module.is_some();
        let target_has_explicit_module = target_library.is_some() || target_module.is_some();
        if !importer_has_explicit_module && !target_has_explicit_module {
            return true;
        }

        false
    }

    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn register_source_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        symbol_path: &InternedPath,
        import: &FileImport,
        export_requirement: ExportRequirement,
    ) -> Result<(), CompilerDiagnostic> {
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
                return Err(diagnostics::not_exported_by_source_file(
                    symbol_path,
                    import.location.clone(),
                ));
            }
        }

        let local_name = self.derive_import_local_name(import)?;

        if let Some(symbol_name) = symbol_path.name() {
            self.emit_alias_case_warning_if_needed(import, symbol_name);
        }

        file_visibility
            .visible_declaration_paths
            .insert(symbol_path.clone());

        let is_type_alias = self.module_symbols.type_alias_paths.contains(symbol_path);
        let is_receiver_method = self
            .module_symbols
            .receiver_method_paths
            .contains(symbol_path);

        if is_receiver_method {
            // Receiver methods are not ordinary value imports. Reserve the local name
            // in the visible-name registry so aliases participate in collision checks,
            // then add receiver-call visibility.
            registry.register(
                local_name,
                VisibleNameBinding::ReceiverMethodImport {
                    target: ReceiverMethodImportTarget::SourceMethod {
                        canonical_path: symbol_path.clone(),
                    },
                },
                import.location.clone(),
            )?;
            Self::add_visible_receiver_method(
                file_visibility,
                local_name,
                symbol_path,
                import.location.clone(),
            );

            // Explicit grouped receiver-method imports are validated for receiver-type
            // visibility after all imports in the file are processed.
            if import.from_grouped {
                self.pending_receiver_validations
                    .push(super::PendingReceiverMethodValidation {
                        local_name,
                        source_path: Some(symbol_path.clone()),
                        external_function_id: None,
                        location: import.location.clone(),
                    });
            }
            return Ok(());
        }

        let binding = if is_type_alias {
            VisibleNameBinding::TypeAlias {
                canonical_path: symbol_path.clone(),
            }
        } else {
            VisibleNameBinding::SourceImport {
                canonical_path: symbol_path.clone(),
            }
        };

        registry.register(local_name, binding, import.location.clone())?;

        if is_type_alias {
            file_visibility
                .visible_type_alias_names
                .insert(local_name, symbol_path.clone());
        } else {
            file_visibility
                .visible_source_names
                .insert(local_name, symbol_path.clone());
        }

        // Importing a struct type auto-imports visible receiver methods for that
        // type from the same declaration surface.
        if self.module_symbols.nominal_type_paths.contains(symbol_path)
            && let Some(target_file) = self
                .module_symbols
                .canonical_source_by_symbol_path
                .get(symbol_path)
        {
            self.auto_import_receiver_methods_for_type(file_visibility, symbol_path, target_file);
        }

        Ok(())
    }
}
