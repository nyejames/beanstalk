//! Type alias target resolution.
//!
//! WHAT: resolves `NamedType` placeholders inside type alias targets to concrete `DataType`s.
//! WHY: type aliases are compile-time-only type metadata; their targets must be fully resolved
//! before function signatures and struct fields are resolved.
//!
//! ## Cycle handling
//!
//! Type alias cycles (e.g. `A as B` + `B as A`) are detected by dependency sorting, because
//! `create_header` collects named-type dependency edges from alias targets just like from struct
//! fields and constant type annotations. Self-reference (`A as A`) also creates a self-loop edge.

use crate::compiler_frontend::ast::import_bindings::FileImportBindings;
use crate::compiler_frontend::ast::module_ast::build_state::AstBuildState;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::declaration_syntax::type_syntax::resolve_named_types_in_data_type;
use crate::compiler_frontend::external_packages::ExternalSymbolId;
use crate::compiler_frontend::headers::parse_file_headers::{Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::{StringId, StringTable};
use rustc_hash::FxHashMap;

impl<'a> AstBuildState<'a> {
    /// Resolve all type alias targets in sorted-header order.
    ///
    /// WHAT: iterates sorted headers, resolving each `TypeAlias` target against already-resolved
    /// aliases and visible declarations.
    /// WHY: dependency sorting guarantees that when we reach an alias, all its dependencies have
    /// already been processed.
    pub(in crate::compiler_frontend::ast) fn resolve_type_aliases(
        &mut self,
        sorted_headers: &[Header],
        file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        let mut alias_paths_by_name: FxHashMap<StringId, InternedPath> = FxHashMap::default();

        // Pre-scan to build name → path map for all type aliases.
        for header in sorted_headers {
            if let HeaderKind::TypeAlias { .. } = &header.kind
                && let Some(name) = header.tokens.src_path.name()
            {
                alias_paths_by_name.insert(name, header.tokens.src_path.to_owned());
            }
        }

        for header in sorted_headers {
            let HeaderKind::TypeAlias { target } = &header.kind else {
                continue;
            };
            let alias_path = &header.tokens.src_path;

            let file_bindings = file_import_bindings.get(&header.source_file);

            let resolved_target = resolve_named_types_in_data_type(
                target,
                &header.name_location,
                &mut |type_name| {
                    // 1. Check already-resolved type aliases (including same-file).
                    if let Some(alias_path) = alias_paths_by_name.get(&type_name)
                        && let Some(resolved_dt) =
                            self.resolved_type_aliases_by_path.get(alias_path)
                    {
                        return Some(resolved_dt.to_owned());
                    }
                    // 2. Check imported type aliases by local name.
                    if let Some(bindings) = file_bindings
                        && let Some(alias_path) = bindings.visible_type_aliases.get(&type_name)
                        && let Some(resolved_dt) =
                            self.resolved_type_aliases_by_path.get(alias_path)
                    {
                        return Some(resolved_dt.to_owned());
                    }
                    // 3. Check visible declarations (structs, choices, builtins).
                    if let Some(dt) = self
                        .declarations
                        .iter()
                        .rfind(|d| {
                            d.id.name() == Some(type_name)
                                && !d.is_unresolved_constant_placeholder()
                        })
                        .map(|d| d.value.data_type.to_owned())
                    {
                        return Some(dt);
                    }
                    // 4. Check visible external types.
                    if let Some(bindings) = file_bindings
                        && let Some(ExternalSymbolId::Type(type_id)) =
                            bindings.visible_external_symbols.get(&type_name)
                    {
                        return Some(DataType::External { type_id: *type_id });
                    }
                    None
                },
                string_table,
            )
            .map_err(|error| self.error_messages(error, string_table))?;

            self.resolved_type_aliases_by_path
                .insert(alias_path.to_owned(), resolved_target);
        }

        Ok(())
    }
}
