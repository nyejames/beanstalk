//! External import registration and external receiver auto-imports.
//!
//! WHAT: registers imports that resolve to external package symbols and auto-imports receiver
//! methods attached to explicitly imported external types.
//! WHY: external package imports use stable IDs rather than source paths, and their receiver
//! methods are discovered from package metadata rather than source file scans.
//! MUST NOT: register source declarations or build source namespace records.

use super::{
    FileVisibility, ImportEnvironmentBuilder, ReceiverMethodImportTarget, VisibleNameBinding,
    VisibleNameRegistry,
};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::{
    ExternalSignatureType, ExternalSymbolId, ExternalTypeId,
};
use crate::compiler_frontend::headers::parse_file_headers::FileImport;

impl<'a> ImportEnvironmentBuilder<'a> {
    // The typed diagnostic payload is still large enough to trigger clippy::result_large_err here.
    #[allow(clippy::result_large_err)]
    pub(super) fn register_external_import(
        &mut self,
        file_visibility: &mut FileVisibility,
        registry: &mut VisibleNameRegistry,
        import: &FileImport,
        symbol_id: ExternalSymbolId,
    ) -> Result<(), CompilerDiagnostic> {
        let local_name = self.derive_import_local_name(import)?;

        if let Some(symbol_name) = import.header_path.name() {
            self.emit_alias_case_warning_if_needed(import, symbol_name);
        }

        if let ExternalSymbolId::Function(function_id) = symbol_id
            && self
                .external_package_registry
                .get_function_by_id(function_id)
                .is_some_and(|function| function.receiver_type.is_some())
        {
            // External receiver methods follow the same user-facing rule as source receiver
            // methods: grouped imports may rename the method for receiver-call lookup, but they
            // must not make `method(...)` a valid free-function call.
            registry.register(
                local_name,
                VisibleNameBinding::ReceiverMethodImport {
                    target: ReceiverMethodImportTarget::ExternalMethod { function_id },
                },
                import.location.clone(),
            )?;
            Self::add_visible_external_receiver_method(file_visibility, local_name, function_id);

            // Explicit grouped receiver-method imports are validated for receiver-type
            // visibility after all imports in the file are processed.
            if import.from_grouped {
                self.pending_receiver_validations
                    .push(super::PendingReceiverMethodValidation {
                        local_name,
                        source_path: None,
                        external_function_id: Some(function_id),
                        location: import.location.clone(),
                    });
            }
            return Ok(());
        }

        registry.register(
            local_name,
            VisibleNameBinding::ExternalImport { symbol_id },
            import.location.clone(),
        )?;

        file_visibility
            .visible_external_symbols
            .insert(local_name, symbol_id);

        file_visibility
            .visible_external_symbol_locations
            .insert(local_name, import.location.clone());

        if let ExternalSymbolId::Type(type_id) = symbol_id {
            self.auto_import_external_receiver_methods_for_type(file_visibility, type_id);
        }

        Ok(())
    }

    /// Auto-imports receiver methods attached to an explicitly imported external type.
    ///
    /// WHAT: scans the type's owning package for receiver methods whose receiver is the
    ///       exact package-scoped opaque type ID and adds them to receiver-call visibility.
    /// WHY: external JS opaque types follow the same import rule as source structs:
    ///      importing the receiver type makes same-surface receiver methods callable without
    ///      exposing those methods as free functions or namespace fields.
    fn auto_import_external_receiver_methods_for_type(
        &mut self,
        file_visibility: &mut FileVisibility,
        type_id: ExternalTypeId,
    ) {
        let Some(type_def) = self.external_package_registry.get_type_by_id(type_id) else {
            return;
        };

        let Some(package_path) = self
            .external_package_registry
            .get_package_by_id(type_def.package_id)
            .map(|package| package.path.clone())
        else {
            return;
        };

        self.add_external_receiver_methods_from_package(
            file_visibility,
            &package_path,
            Some(type_id),
        );
    }

    /// Adds receiver-call visibility for external receiver methods exposed by a package.
    ///
    /// `receiver_type_filter` narrows the scan for grouped type imports. Namespace imports pass
    /// `None` because the whole package surface is visible through the namespace import.
    pub(super) fn add_external_receiver_methods_from_package(
        &mut self,
        file_visibility: &mut FileVisibility,
        package_path: &str,
        receiver_type_filter: Option<ExternalTypeId>,
    ) {
        let Some(package) = self.external_package_registry.get_package(package_path) else {
            return;
        };

        let function_names = package.functions.keys().cloned().collect::<Vec<_>>();

        for function_name in function_names {
            let Some((function_id, function_def)) = self
                .external_package_registry
                .resolve_package_function(package_path, &function_name)
            else {
                continue;
            };

            let Some(receiver_type) = function_def.receiver_type.as_ref() else {
                continue;
            };

            if let Some(type_id) = receiver_type_filter
                && receiver_type != &ExternalSignatureType::External(type_id)
            {
                continue;
            }

            let method_name = self.string_table.intern(&function_name);
            Self::add_visible_external_receiver_method(file_visibility, method_name, function_id);
        }
    }
}
