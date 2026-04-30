//! Pass 2: import binding resolution.
//!
//! WHAT: builds per-source-file import visibility maps and start-function aliases.
//! WHY: imports are file-scoped rules, but declarations are module-scoped identities;
//! separating them keeps the declaration table stable while gates vary per file.

use super::build_state::AstBuildState;
use crate::compiler_frontend::ast::import_bindings::{
    FileImportBindings, resolve_file_import_bindings, resolve_re_exports,
};
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashMap;

impl<'a> AstBuildState<'a> {
    /// Build per-source-file import visibility and start-function aliases.
    pub(in crate::compiler_frontend::ast) fn resolve_import_bindings(
        &mut self,
        string_table: &mut StringTable,
    ) -> Result<FxHashMap<InternedPath, FileImportBindings>, CompilerMessages> {
        let reexport_warnings = resolve_re_exports(
            &mut self.module_symbols,
            self.external_package_registry,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;
        self.warnings.extend(reexport_warnings);

        let (mut bindings, import_warnings) = resolve_file_import_bindings(
            &self.module_symbols.file_imports_by_source,
            &self.module_symbols.module_file_paths,
            &self.module_symbols.importable_symbol_exported,
            &self.module_symbols.declared_paths_by_file,
            &self.module_symbols.type_alias_paths,
            &self.module_symbols.builtin_visible_symbol_paths,
            self.external_package_registry,
            &self.module_symbols.facade_exports,
            &self.module_symbols.file_library_membership,
            &self.module_symbols.file_module_membership,
            &self.module_symbols.module_root_facade_exports,
            &self.module_symbols.module_root_prefixes,
            &self.module_symbols.canonical_source_by_symbol_path,
            string_table,
        )
        .map_err(|error| self.error_messages(error, string_table))?;

        self.warnings.extend(import_warnings);

        for (source_file, binding) in bindings.iter_mut() {
            binding.visible_symbol_paths.extend(
                self.module_symbols
                    .builtin_visible_symbol_paths
                    .iter()
                    .cloned(),
            );

            // Add builtins to visible_source_bindings so name lookup finds them.
            // Same-file declarations take precedence; do not overwrite.
            for path in &self.module_symbols.builtin_visible_symbol_paths {
                if let Some(name) = path.name() {
                    binding
                        .visible_source_bindings
                        .entry(name)
                        .or_insert_with(|| path.to_owned());
                }
            }

            // Add local type aliases to visible_type_aliases so they resolve in type
            // annotations within the same file.
            if let Some(declared_paths) =
                self.module_symbols.declared_paths_by_file.get(source_file)
            {
                for path in declared_paths {
                    if self.module_symbols.type_alias_paths.contains(path)
                        && let Some(name) = path.name()
                    {
                        binding.visible_type_aliases.insert(name, path.to_owned());
                    }
                }
            }
        }

        Ok(bindings)
    }
}
