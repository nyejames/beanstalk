//! External import registration.
//!
//! WHAT: registers imports that resolve to external package symbols.
//! WHY: external package imports use stable IDs rather than source paths, while receiver method
//! syntax remains source-owned and compiler-owned rather than external-package metadata.
//! MUST NOT: register source declarations or build source namespace records.

use super::{FileVisibility, ImportEnvironmentBuilder, VisibleNameBinding, VisibleNameRegistry};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
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

        Ok(())
    }
}
