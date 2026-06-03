//! Source import registration and source receiver auto-imports.
//!
//! WHAT: registers imports that resolve to source file declarations (same-module, cross-module,
//! source library) and auto-imports receiver methods when a nominal receiver type is imported.
//! WHY: source imports follow facade and export rules that differ from external package imports,
//! so they deserve their own focused registration path.
//! MUST NOT: register external package symbols or build namespace records.

use super::{
    FileVisibility, ImportEnvironmentBuilder, ReceiverMethodImportTarget, SourceImportAccess,
    VisibleNameBinding, VisibleNameRegistry,
};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::import_environment::diagnostics;
use crate::compiler_frontend::headers::module_symbols::FacadeExportTarget;
use crate::compiler_frontend::headers::parse_file_headers::FileImport;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::tokenizer::tokens::SourceLocation;

impl<'a> ImportEnvironmentBuilder<'a> {
    /// Auto-import receiver methods for a nominal type from the file where it is declared.
    ///
    /// WHAT: when a struct or choice type is imported, all receiver methods declared in the same
    ///       file whose receiver matches that type become visible through the receiver catalog.
    /// WHY: receiver methods travel with their receiver type on the same import surface.
    fn auto_import_receiver_methods_for_type(
        &self,
        file_visibility: &mut FileVisibility,
        nominal_type_path: &InternedPath,
        target_file: &InternedPath,
        access: &SourceImportAccess,
    ) {
        let Some(receiver_type_name) = nominal_type_path.name() else {
            return;
        };

        // Walk receiver_method_paths directly and match by canonical source file.
        // WHY: canonical_source_by_symbol_path uses canonical OS paths while
        //      declared_paths_by_file uses header.source_file (logical/relative).
        //      Comparing against canonical_source_by_symbol_path avoids the mismatch.
        for path in &self.module_symbols.receiver_method_paths {
            if self.module_symbols.receiver_method_receiver_names.get(path)
                != Some(&receiver_type_name)
            {
                continue;
            }

            if self
                .module_symbols
                .canonical_source_by_symbol_path
                .get(path)
                .is_some_and(|source_file| source_file == target_file)
                && self.receiver_method_visible_for_source_access(path, access)
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

        self.source_files_share_import_boundary(importer_file, target_file)
    }

    /// Whether a receiver method can travel with a source type imported through this access path.
    ///
    /// WHAT: internal imports preserve the existing file/module-local receiver behavior; direct
    /// source imports require the method itself to be importable from its source file; facade
    /// imports use the exact explicit facade surface that exposed the receiver type.
    pub(super) fn receiver_method_visible_for_source_access(
        &self,
        method_path: &InternedPath,
        access: &SourceImportAccess,
    ) -> bool {
        match access {
            SourceImportAccess::Internal => true,
            SourceImportAccess::DirectSourceExport => self
                .module_symbols
                .importable_source_symbol_paths
                .contains(method_path),
            SourceImportAccess::Facade { exported_entries } => exported_entries.iter().any(
                |entry| matches!(&entry.target, FacadeExportTarget::Source(path) if path == method_path),
            ),
        }
    }

    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn register_source_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        symbol_path: &InternedPath,
        import: &FileImport,
        access: SourceImportAccess,
    ) -> Result<(), CompilerDiagnostic> {
        // Check export requirement.
        if matches!(&access, SourceImportAccess::DirectSourceExport) {
            let is_importable = self
                .module_symbols
                .importable_source_symbol_paths
                .contains(symbol_path);
            if !is_importable {
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
        let is_trait = self.module_symbols.trait_paths.contains(symbol_path);
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
        } else if is_trait {
            VisibleNameBinding::Trait {
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
        } else if is_trait {
            file_visibility
                .visible_trait_names
                .insert(local_name, symbol_path.clone());
        } else {
            file_visibility
                .visible_source_names
                .insert(local_name, symbol_path.clone());
        }

        // Importing a nominal receiver type auto-imports visible receiver methods
        // for that type from the same declaration surface.
        if self.module_symbols.nominal_type_paths.contains(symbol_path)
            && let Some(target_file) = self
                .module_symbols
                .canonical_source_by_symbol_path
                .get(symbol_path)
        {
            self.auto_import_receiver_methods_for_type(
                file_visibility,
                symbol_path,
                target_file,
                &access,
            );
        }

        Ok(())
    }
}
