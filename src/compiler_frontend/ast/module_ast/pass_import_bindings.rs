//! Pass 2: import binding resolution.
//!
//! WHAT: builds per-source-file import visibility maps and start-function aliases.
//! WHY: imports are file-scoped rules, but declarations are module-scoped identities;
//! separating them keeps the declaration table stable while gates vary per file.

use super::build_state::AstBuildState;
use crate::compiler_frontend::ast::import_bindings::{
    FileImportBindings, resolve_file_import_bindings,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashMap;

impl<'a> AstBuildState<'a> {
    /// Build per-source-file import visibility and start-function aliases.
    pub(in crate::compiler_frontend::ast) fn resolve_import_bindings(
        &self,
        string_table: &mut StringTable,
    ) -> Result<FxHashMap<InternedPath, FileImportBindings>, CompilerMessages> {
        let mut bindings = resolve_file_import_bindings(
            &self.module_symbols.file_imports_by_source,
            &self.module_symbols.module_file_paths,
            &self.module_symbols.importable_symbol_exported,
            &self.module_symbols.declared_paths_by_file,
            &self.module_symbols.declared_names_by_file,
            self.external_package_registry,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        for binding in bindings.values_mut() {
            binding.visible_symbol_paths.extend(
                self.module_symbols
                    .builtin_visible_symbol_paths
                    .iter()
                    .cloned(),
            );
        }

        Ok(bindings)
    }
}
