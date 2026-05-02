//! Header parser entry point.
//!
//! WHAT: orchestrates parsing all tokenized files into `Headers`, gathers top-level const-fragment
//! placement metadata, and builds the header-owned `ModuleSymbols` package.
//! WHY: callers should have one obvious entry function while detailed file/header parsing lives in
//! focused helper modules.

use crate::compiler_frontend::builtins::error_type::{
    is_reserved_builtin_symbol, register_builtin_error_types,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_warnings::CompilerWarning;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::file_parser::parse_headers_in_file;
use crate::compiler_frontend::headers::module_symbols::{
    FacadeExportEntry, FacadeExportTarget, GenericDeclarationKind, GenericDeclarationMetadata,
    ModuleSymbols, register_declared_symbol,
};
pub use crate::compiler_frontend::headers::types::{
    FileImport, FileRole, Header, HeaderKind, HeaderParseOptions, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::headers::types::{FileReExport, HeaderParseContext};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::source_libraries::mod_file::path_is_mod_file;
use crate::compiler_frontend::symbols::identifier_policy::ensure_not_keyword_shadow_identifier;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use rustc_hash::{FxHashMap, FxHashSet};
use std::path::Path;

pub fn parse_headers(
    tokenized_files: Vec<FileTokens>,
    external_package_registry: &ExternalPackageRegistry,
    warnings: &mut Vec<CompilerWarning>,
    entry_file_path: &Path,
    options: HeaderParseOptions,
    string_table: &mut StringTable,
) -> Result<Headers, Vec<CompilerError>> {
    let HeaderParseOptions {
        entry_file_id,
        project_path_resolver,
        path_format_config,
        style_directives,
    } = options;

    let mut headers: Vec<Header> = Vec::new();
    let mut errors: Vec<CompilerError> = Vec::new();
    let mut const_template_count = 0;
    let mut top_level_const_fragments = Vec::new();
    let mut file_re_exports_by_source: FxHashMap<InternedPath, Vec<FileReExport>> =
        FxHashMap::default();
    // Tracks runtime fragments seen so far in the entry file, for const fragment insertion indices.
    let mut runtime_fragment_count = 0usize;

    for mut file in tokenized_files {
        let is_entry_file = match (entry_file_id, file.file_id) {
            (Some(expected_id), Some(current_id)) => expected_id == current_id,
            _ => file.src_path.to_path_buf(string_table) == entry_file_path,
        };

        let file_role = if is_entry_file {
            FileRole::Entry
        } else if path_is_mod_file(&file.src_path, string_table) {
            FileRole::ModuleFacade
        } else {
            FileRole::Normal
        };

        let mut parse_context = HeaderParseContext {
            external_package_registry,
            style_directives: &style_directives,
            warnings,
            file_role,
            project_path_resolver: project_path_resolver.clone(),
            path_format_config: path_format_config.clone(),
            string_table,
            const_template_number: &mut const_template_count,
            runtime_fragment_count: &mut runtime_fragment_count,
            top_level_const_fragments: &mut top_level_const_fragments,
            file_re_exports_by_source: &mut file_re_exports_by_source,
        };

        let headers_from_file = parse_headers_in_file(&mut file, &mut parse_context);

        match headers_from_file {
            Ok(file_headers) => {
                headers.extend(file_headers);
            }
            Err(e) => {
                errors.push(e);
            }
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    let mut module_symbols =
        build_module_symbols(&headers, string_table).map_err(|mut symbol_errors| {
            errors.append(&mut symbol_errors);
            errors
        })?;

    merge_file_re_exports(&mut module_symbols, file_re_exports_by_source);

    if let Some(resolver) = &project_path_resolver {
        build_facade_data(&mut module_symbols, &headers, resolver, string_table)
            .map_err(|e| vec![e])?;
    }

    Ok(Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count: runtime_fragment_count,
        module_symbols,
    })
}

/// Build facade export maps and file library/module membership from parsed headers and the path resolver.
///
/// WHAT: scans each source library root's and regular module root's `#mod.bst` for exported symbols,
/// and records which source files belong to which library or module root.
/// WHY: AST import binding needs this data to enforce the facade gate for cross-library and
///      cross-module imports.
fn build_facade_data(
    module_symbols: &mut ModuleSymbols,
    headers: &[Header],
    resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), CompilerError> {
    // Build facade export maps from source library #mod.bst headers.
    for (prefix, facade_file) in resolver.facade_files() {
        let mod_file_logical =
            resolver.logical_path_for_canonical_file(facade_file, string_table)?;
        let mod_file_interned = InternedPath::from_path_buf(&mod_file_logical, string_table);

        let mut exports = FxHashSet::default();
        module_symbols
            .file_library_membership
            .insert(mod_file_interned.clone(), prefix.clone());

        for header in headers {
            if header.source_file == mod_file_interned
                && header.exported
                && let Some(export_name) = header.tokens.src_path.name()
            {
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

    // Build file library membership from canonical paths.
    for header in headers {
        if let Some(canonical_path) = &header.tokens.canonical_os_path {
            for (prefix, root_path) in resolver.source_library_roots() {
                if canonical_path.starts_with(root_path) {
                    module_symbols
                        .file_library_membership
                        .insert(header.source_file.clone(), prefix.clone());
                    break;
                }
            }
        }
    }

    // Build module root facade exports and file membership for entry-root modules.
    let mut module_root_prefixes: Vec<(InternedPath, InternedPath)> = Vec::new();

    for module_root in resolver.module_roots() {
        let root_interned = InternedPath::from_path_buf(module_root, string_table);

        // Ensure every module root with a facade has an entry in the map,
        // even if the export set is empty.
        if resolver.module_root_facades().contains_key(module_root) {
            module_symbols
                .module_root_facade_exports
                .entry(root_interned.clone())
                .or_default();
        }

        // Build prefix relative to entry root for import-path interception.
        if let Ok(relative) = module_root.strip_prefix(resolver.entry_root()) {
            let prefix_interned = InternedPath::from_path_buf(relative, string_table);
            module_root_prefixes.push((prefix_interned, root_interned));
        }
    }

    // Sort longest prefix first so nested module roots match before their parents.
    module_root_prefixes.sort_by_key(|(b, _)| std::cmp::Reverse(b.len()));
    module_symbols.module_root_prefixes = module_root_prefixes;

    for header in headers {
        if let Some(canonical_path) = &header.tokens.canonical_os_path
            && let Some(module_root) = resolver.module_root_for_file(canonical_path)
        {
            let module_root_interned = InternedPath::from_path_buf(&module_root, string_table);
            let logical = header.source_file.clone();
            let canonical = header.canonical_source_file(string_table);

            module_symbols
                .file_module_membership
                .insert(logical, module_root_interned.clone());
            module_symbols
                .file_module_membership
                .insert(canonical, module_root_interned.clone());

            // If this file is a module root facade, build its export map.
            if let Some(facade_file) = resolver.module_root_facades().get(&module_root)
                && canonical_path == facade_file
                && header.exported
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

    Ok(())
}

fn merge_file_re_exports(
    module_symbols: &mut ModuleSymbols,
    re_exports_by_source: FxHashMap<InternedPath, Vec<FileReExport>>,
) {
    for (source_file, re_exports) in re_exports_by_source {
        module_symbols
            .file_re_exports_by_source
            .entry(source_file)
            .and_modify(|existing| {
                for re_export in &re_exports {
                    let already_present = existing.iter().any(|entry| {
                        entry.header_path == re_export.header_path && entry.alias == re_export.alias
                    });
                    if !already_present {
                        existing.push(re_export.clone());
                    }
                }
            })
            .or_insert(re_exports);
    }
}

/// Collect all order-independent top-level symbol metadata from parsed (unsorted) headers.
///
/// WHAT: validates symbol names, builds import/export/source maps, registers builtins.
/// WHY: all this work depends only on the per-header data available immediately after parsing;
/// it does not require dependency order. `declarations` is intentionally left empty here
/// and filled by `resolve_module_dependencies` once headers are sorted.
fn build_module_symbols(
    headers: &[Header],
    string_table: &mut StringTable,
) -> Result<ModuleSymbols, Vec<CompilerError>> {
    let mut module_symbols = ModuleSymbols::empty();
    let mut errors: Vec<CompilerError> = Vec::new();

    for header in headers {
        if let Some(symbol_name) = header.tokens.src_path.name() {
            let symbol_name_text = string_table.resolve(symbol_name).to_owned();

            if let Err(error) = ensure_not_keyword_shadow_identifier(
                &symbol_name_text,
                header.name_location.to_owned(),
                "Module Declaration Collection",
            ) {
                errors.push(error);
                continue;
            }

            if is_reserved_builtin_symbol(&symbol_name_text) {
                errors.push(CompilerError::new_rule_error(
                    format!("'{symbol_name_text}' is reserved as a builtin language type."),
                    header.name_location.to_owned(),
                ));
                continue;
            }
        }

        module_symbols
            .module_file_paths
            .insert(header.source_file.to_owned());
        module_symbols.canonical_source_by_symbol_path.insert(
            header.tokens.src_path.to_owned(),
            header.canonical_source_file(string_table),
        );
        module_symbols
            .file_imports_by_source
            .entry(header.source_file.to_owned())
            .and_modify(|existing| {
                for import in &header.file_imports {
                    let already_present = existing
                        .iter()
                        .any(|e| e.header_path == import.header_path && e.alias == import.alias);
                    if !already_present {
                        existing.push(import.clone());
                    }
                }
            })
            .or_insert_with(|| header.file_imports.to_owned());

        module_symbols
            .file_re_exports_by_source
            .entry(header.source_file.to_owned())
            .and_modify(|existing| {
                for re_export in &header.file_re_exports {
                    let already_present = existing.iter().any(|e| {
                        e.header_path == re_export.header_path && e.alias == re_export.alias
                    });
                    if !already_present {
                        existing.push(re_export.clone());
                    }
                }
            })
            .or_insert_with(|| header.file_re_exports.to_owned());

        let is_facade_symbol = path_is_mod_file(&header.source_file, string_table);
        match &header.kind {
            HeaderKind::Function {
                generic_parameters, ..
            } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported && !is_facade_symbol),
                );
                register_generic_declaration_metadata(
                    &mut module_symbols,
                    header,
                    generic_parameters,
                    GenericDeclarationKind::Function,
                );
            }
            HeaderKind::Struct {
                generic_parameters, ..
            } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported && !is_facade_symbol),
                );
                register_generic_declaration_metadata(
                    &mut module_symbols,
                    header,
                    generic_parameters,
                    GenericDeclarationKind::Struct,
                );
            }
            HeaderKind::Choice {
                generic_parameters, ..
            } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported && !is_facade_symbol),
                );
                register_generic_declaration_metadata(
                    &mut module_symbols,
                    header,
                    generic_parameters,
                    GenericDeclarationKind::Choice,
                );
            }
            HeaderKind::StartFunction => {
                let start_name = header
                    .source_file
                    .join_str(IMPLICIT_START_FUNC_NAME, string_table);
                register_declared_symbol(
                    &mut module_symbols,
                    &start_name,
                    &header.source_file,
                    None,
                );
            }
            HeaderKind::Constant { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported && !is_facade_symbol),
                );
            }
            HeaderKind::TypeAlias {
                generic_parameters, ..
            } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported && !is_facade_symbol),
                );
                module_symbols
                    .type_alias_paths
                    .insert(header.tokens.src_path.to_owned());
                register_generic_declaration_metadata(
                    &mut module_symbols,
                    header,
                    generic_parameters,
                    GenericDeclarationKind::TypeAlias,
                );
            }
            _ => {}
        }
    }

    if !errors.is_empty() {
        return Err(errors);
    }

    // Register builtin error types: visible paths, struct fields, AST nodes, and declarations.
    // WHY: builtins are merged once here so AST passes see them without a separate absorption step.
    let builtin_manifest = register_builtin_error_types(string_table);
    module_symbols
        .builtin_visible_symbol_paths
        .extend(builtin_manifest.visible_symbol_paths.iter().cloned());
    module_symbols.builtin_declarations = builtin_manifest.declarations;
    module_symbols
        .resolved_struct_fields_by_path
        .extend(builtin_manifest.resolved_struct_fields_by_path);
    module_symbols
        .struct_source_by_path
        .extend(builtin_manifest.struct_source_by_path);
    module_symbols
        .builtin_struct_ast_nodes
        .extend(builtin_manifest.ast_struct_nodes);

    Ok(module_symbols)
}

fn register_generic_declaration_metadata(
    module_symbols: &mut ModuleSymbols,
    header: &Header,
    generic_parameters: &crate::compiler_frontend::datatypes::generics::GenericParameterList,
    kind: GenericDeclarationKind,
) {
    if generic_parameters.is_empty() {
        return;
    }

    module_symbols.generic_declarations_by_path.insert(
        header.tokens.src_path.to_owned(),
        GenericDeclarationMetadata {
            kind,
            parameters: generic_parameters.to_owned(),
            declaration_location: header.name_location.to_owned(),
        },
    );
}

#[cfg(test)]
#[path = "tests/parse_file_headers_tests.rs"]
mod parse_file_headers_tests;
