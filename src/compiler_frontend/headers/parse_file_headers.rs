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
use crate::compiler_frontend::headers::file_parser::parse_headers_in_file;
use crate::compiler_frontend::headers::module_symbols::{ModuleSymbols, register_declared_symbol};
use crate::compiler_frontend::headers::types::HeaderParseContext;
pub use crate::compiler_frontend::headers::types::{
    FileImport, Header, HeaderKind, HeaderParseOptions, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::symbols::identifier_policy::ensure_not_keyword_shadow_identifier;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;
use std::path::Path;

pub fn parse_headers(
    tokenized_files: Vec<FileTokens>,
    host_registry: &HostRegistry,
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
    // Tracks runtime fragments seen so far in the entry file, for const fragment insertion indices.
    let mut runtime_fragment_count = 0usize;

    for mut file in tokenized_files {
        let is_entry_file = match (entry_file_id, file.file_id) {
            (Some(expected_id), Some(current_id)) => expected_id == current_id,
            _ => file.src_path.to_path_buf(string_table) == entry_file_path,
        };

        let mut parse_context = HeaderParseContext {
            host_function_registry: host_registry,
            style_directives: &style_directives,
            warnings,
            is_entry_file,
            project_path_resolver: project_path_resolver.clone(),
            path_format_config: path_format_config.clone(),
            string_table,
            const_template_number: &mut const_template_count,
            runtime_fragment_count: &mut runtime_fragment_count,
            top_level_const_fragments: &mut top_level_const_fragments,
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

    let module_symbols =
        build_module_symbols(&headers, string_table).map_err(|mut symbol_errors| {
            errors.append(&mut symbol_errors);
            errors
        })?;

    Ok(Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count: runtime_fragment_count,
        module_symbols,
    })
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
            .or_insert_with(|| header.file_imports.to_owned());

        match &header.kind {
            HeaderKind::Function { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::Struct { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
                );
            }
            HeaderKind::Choice { .. } => {
                register_declared_symbol(
                    &mut module_symbols,
                    &header.tokens.src_path,
                    &header.source_file,
                    Some(header.exported),
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
                    Some(header.exported),
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

#[cfg(test)]
#[path = "tests/parse_file_headers_tests.rs"]
mod parse_file_headers_tests;
