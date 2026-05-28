//! Facade export and file-membership data for header imports.
//!
//! WHAT: derives source-library and entry-root module facade export maps from parsed headers.
//! WHY: import environment preparation needs a single header-owned view of which declarations are
//! exposed across facade boundaries and which source files belong to each boundary.

use crate::compiler_frontend::compiler_errors::compiler_error_to_diagnostic;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::headers::module_symbols::{
    FacadeExportEntry, FacadeExportTarget, ModuleSymbols,
};
use crate::compiler_frontend::headers::types::{FileRole, Header, HeaderKind};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;

use rustc_hash::FxHashSet;

/// Whether a header kind represents a real authored declaration that should be exported by a
/// module facade.
///
/// WHAT: functions, structs, choices, type aliases, and compile-time constants are authored
/// declarations. Const templates and the implicit start function are not.
/// WHY: `#mod.bst` has no private authored declaration surface; every valid authored declaration
/// is automatically exported. Imported names and runtime bindings are not headers of these kinds.
fn is_authored_facade_declaration(kind: &HeaderKind) -> bool {
    matches!(
        kind,
        HeaderKind::Function { .. }
            | HeaderKind::Struct { .. }
            | HeaderKind::Choice { .. }
            | HeaderKind::TypeAlias { .. }
            | HeaderKind::Constant { .. }
    )
}

fn is_authored_facade_export(header: &Header) -> bool {
    header.file_role == FileRole::ModuleFacade && is_authored_facade_declaration(&header.kind)
}

/// Build facade export maps and file library/module membership from parsed headers and the path
/// resolver.
pub(super) fn build_facade_data(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    build_source_library_facade_exports(module_symbols, headers, resolver, string_table)?;
    build_source_library_membership(module_symbols, headers, resolver, string_table);
    build_module_root_facade_data(module_symbols, headers, resolver, string_table);

    Ok(())
}

fn build_source_library_facade_exports(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerDiagnostic> {
    for (prefix, facade_file) in resolver.facade_files() {
        // Mutation: logical path resolution converts canonical OS paths into interned components.
        let mod_file_logical = resolver
            .logical_path_for_canonical_file(facade_file, string_table)
            .map_err(|error| compiler_error_to_diagnostic(&error))?;
        let mod_file_interned = InternedPath::from_path_buf(&mod_file_logical, string_table);

        let mut exports = FxHashSet::default();
        module_symbols
            .file_library_membership
            .insert(mod_file_interned.clone(), prefix.clone());
        module_symbols
            .source_library_facade_files
            .insert(prefix.clone(), mod_file_interned.clone());

        for header in headers {
            if header.source_file != mod_file_interned {
                continue;
            }

            if !is_authored_facade_export(header) {
                continue;
            }

            if let Some(export_name) = header.tokens.src_path.name() {
                exports.insert(FacadeExportEntry {
                    export_name,
                    target: FacadeExportTarget::Source(header.tokens.src_path.clone()),
                });
            }
        }

        module_symbols
            .facade_exports
            .insert(prefix.clone(), exports);
    }

    Ok(())
}

fn build_source_library_membership(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) {
    for header in headers {
        let Some(canonical_path) = &header.tokens.canonical_os_path else {
            continue;
        };

        for (prefix, root_path) in resolver.source_library_roots() {
            if canonical_path.starts_with(root_path) {
                let canonical_source = header.canonical_source_file(string_table);
                module_symbols
                    .file_library_membership
                    .insert(header.source_file.clone(), prefix.clone());
                module_symbols
                    .file_library_membership
                    .insert(canonical_source, prefix.clone());
                break;
            }
        }
    }
}

fn build_module_root_facade_data(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) {
    let mut module_root_prefixes =
        build_module_root_prefixes(module_symbols, resolver, string_table);
    module_root_prefixes.sort_by_key(|(prefix, _)| std::cmp::Reverse(prefix.len()));
    module_symbols.module_root_prefixes = module_root_prefixes;

    for header in headers {
        let Some(canonical_path) = &header.tokens.canonical_os_path else {
            continue;
        };
        let Some(module_root) = resolver.module_root_for_file(canonical_path) else {
            continue;
        };

        let module_root_interned = InternedPath::from_path_buf(&module_root, string_table);
        let logical = header.source_file.clone();
        let canonical = header.canonical_source_file(string_table);

        // Insert both logical and canonical identities because import checks may arrive from
        // source spelling while dependency sorting and path resolution work with canonical paths.
        module_symbols
            .file_module_membership
            .insert(logical, module_root_interned.clone());
        module_symbols
            .file_module_membership
            .insert(canonical, module_root_interned.clone());

        if let Some(facade_file) = resolver.module_root_facades().get(&module_root)
            && canonical_path == facade_file
            && is_authored_facade_export(header)
            && let Some(export_name) = header.tokens.src_path.name()
        {
            let exports = module_symbols
                .module_root_facade_exports
                .entry(module_root_interned)
                .or_default();
            exports.insert(FacadeExportEntry {
                export_name,
                target: FacadeExportTarget::Source(header.tokens.src_path.clone()),
            });
        }
    }
}

fn build_module_root_prefixes(
    module_symbols: &mut ModuleSymbols,
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Vec<(InternedPath, InternedPath)> {
    let mut module_root_prefixes = Vec::new();

    for module_root in resolver.module_roots() {
        let root_interned = InternedPath::from_path_buf(module_root, string_table);

        if resolver.module_root_facades().contains_key(module_root) {
            module_symbols
                .module_root_facade_exports
                .entry(root_interned.clone())
                .or_default();
        }

        if let Ok(relative) = module_root.strip_prefix(resolver.entry_root()) {
            let prefix_interned = InternedPath::from_path_buf(relative, string_table);
            module_root_prefixes.push((prefix_interned, root_interned));
        }
    }

    module_root_prefixes
}
