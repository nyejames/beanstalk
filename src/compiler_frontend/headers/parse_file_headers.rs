//! Header parser entry point.
//!
//! WHAT: parses individual token streams into per-file header outputs, then aggregates prepared
//! files into module-wide `Headers`.
//! WHY: per-file parsing and module aggregation are separate boundaries so callers can merge and
//! remap local string-table outputs before dependency sorting and AST construction.

use crate::compiler_frontend::arena::{HeaderStats, TokenStats};
use crate::compiler_frontend::compiler_messages::DiagnosticBag;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::headers::constant_dependencies::{
    ConstantDependencyInput, add_constant_initializer_dependencies,
};
use crate::compiler_frontend::headers::dependency_canonicalization::canonicalize_header_dependencies;
use crate::compiler_frontend::headers::file_parser::parse_headers_in_file;
use crate::compiler_frontend::headers::import_environment::{
    ImportEnvironmentInput, prepare_import_environment,
};
use crate::compiler_frontend::headers::public_exports::build_public_exports;
use crate::compiler_frontend::headers::symbol_collection::build_module_symbols;
use crate::compiler_frontend::headers::types::HeaderParseContext;
pub use crate::compiler_frontend::headers::types::{
    FileFrontendPrepareError, FileFrontendPrepareOutput, FileImport, FileRole, Header, HeaderKind,
    HeaderParseOptions, Headers, TopLevelConstFragment,
};
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::source_libraries::root_file::{
    file_name_is_config_file, file_name_is_hash_root_file,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::FileTokens;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;
use std::path::Path;

/// Parse one tokenized file using the supplied string table.
///
/// WHAT: computes the file role, builds the header parse context, and delegates to the file parser.
/// WHY: fused frontend preparation owns local-table creation and merging in the pipeline layer,
/// while the header stage owns only header parsing against whichever table the caller provides.
pub fn parse_file_headers_with_table(
    file_tokens: &mut FileTokens,
    entry_file_path: &Path,
    options: &HeaderParseOptions,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
    const_template_offset: usize,
    runtime_fragment_offset: usize,
) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
    let HeaderParseOptions { entry_file_id, .. } = options;

    let is_entry_file = match (*entry_file_id, file_tokens.file_id) {
        (Some(expected_id), Some(current_id)) => expected_id == current_id,
        _ => file_tokens.src_path.to_path_buf(string_table) == entry_file_path,
    };

    let source_path = file_tokens
        .canonical_os_path
        .as_deref()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| file_tokens.src_path.to_path_buf(string_table));
    let is_hash_root_file = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(file_name_is_hash_root_file);
    let is_config_file = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(file_name_is_config_file);
    let is_prepared_module_root = options
        .project_path_resolver
        .as_ref()
        .is_some_and(|resolver| resolver.is_module_root_file(&source_path));

    let file_role = if is_entry_file {
        FileRole::ActiveModuleRoot
    } else if is_prepared_module_root || is_hash_root_file {
        FileRole::ImportedModuleRoot
    } else {
        FileRole::Normal
    };

    let mut parse_context = HeaderParseContext {
        external_package_registry,
        file_role,
        is_config_file,
        string_table,
        const_template_offset,
        runtime_fragment_offset,
    };

    parse_headers_in_file(file_tokens, &mut parse_context)
}

/// Parse headers from an already-tokenized file against a local string-table fork, then merge
/// the local delta back into the module/global table and remap all StringIds in the output.
///
/// WHAT: this is the per-file header-parsing half of preparation for callers that already have
///       a `FileTokens` stream, such as config parsing that runs token-level validation first.
/// WHY: callers that need the raw token stream before header parsing still get the same local-fork
///      merge/remap behavior without repeating tokenization.
pub fn prepare_file_from_tokens(
    mut file_tokens: FileTokens,
    entry_file_path: &Path,
    options: &HeaderParseOptions,
    external_package_registry: &ExternalPackageRegistry,
    string_table: &mut StringTable,
    const_template_offset: usize,
    runtime_fragment_offset: usize,
) -> Result<FileFrontendPrepareOutput, FileFrontendPrepareError> {
    let fork_source = string_table.fork_source();
    let (mut local_string_table, base_len) = fork_source.fork_for_module().into_parts();

    let file_output = parse_file_headers_with_table(
        &mut file_tokens,
        entry_file_path,
        options,
        external_package_registry,
        &mut local_string_table,
        const_template_offset,
        runtime_fragment_offset,
    );

    let remap = string_table.merge_delta_from(&local_string_table, base_len);

    match file_output {
        Ok(mut output) => {
            output.remap_string_ids(&remap);
            Ok(output)
        }
        Err(mut error) => {
            error.remap_string_ids(&remap);
            Err(error)
        }
    }
}

/// Aggregate per-file frontend preparation outputs into module-wide `Headers`.
///
/// WHAT: consumes already-remapped `FileFrontendPrepareOutput` values and builds the module-wide
/// symbol package, import environment, dependency graph, and public export data.
/// WHY: module-wide aggregation must happen after all per-file outputs have been remapped into
/// the global string table so that symbol paths and dependency edges resolve consistently.
pub fn parse_headers(
    prepared_files: Vec<FileFrontendPrepareOutput>,
    external_package_registry: &ExternalPackageRegistry,
    external_import_resolution_table: &ExternalImportResolutionTable,
    project_path_resolver: Option<&ProjectPathResolver>,
    string_table: &mut StringTable,
) -> Result<Headers, DiagnosticBag> {
    let mut module_symbols = build_module_symbols(&prepared_files, string_table)?;

    let mut headers: Vec<Header> = Vec::new();
    let mut top_level_const_fragments = Vec::new();
    let mut runtime_fragment_count = 0usize;
    let mut has_non_trivial_root_body = false;
    let mut token_stats = TokenStats::default();

    for output in &prepared_files {
        token_stats.add(&output.token_stats);
    }

    for output in prepared_files {
        headers.extend(output.headers);
        top_level_const_fragments.extend(output.top_level_const_fragments);
        runtime_fragment_count += output.runtime_fragment_count;
        has_non_trivial_root_body |= output.has_non_trivial_root_body;
    }

    if let Some(resolver) = project_path_resolver {
        build_public_exports(
            &mut module_symbols,
            &headers,
            resolver,
            external_package_registry,
            string_table,
        )
        .map_err(|boxed_diagnostic| {
            let mut bag = DiagnosticBag::new();
            bag.push(*boxed_diagnostic);
            bag
        })?;
    }

    let import_environment = prepare_import_environment(ImportEnvironmentInput {
        module_symbols: &mut module_symbols,
        external_package_registry,
        external_import_resolution_table,
        string_table,
    })
    .map_err(|messages| DiagnosticBag::from_diagnostics(messages.into_diagnostics()))?;

    canonicalize_header_dependencies(
        &mut headers,
        &import_environment,
        &module_symbols.file_imports_by_source,
    )?;

    let _constant_report = add_constant_initializer_dependencies(ConstantDependencyInput {
        headers: &mut headers,
        module_symbols: &module_symbols,
        import_environment: &import_environment,
        string_table,
    })?;

    let header_stats = HeaderStats::from_headers_and_symbols(&headers, &module_symbols);
    let const_fragment_count = top_level_const_fragments.len();

    Ok(Headers {
        headers,
        top_level_const_fragments,
        entry_runtime_fragment_count: runtime_fragment_count,
        const_fragment_count,
        has_non_trivial_root_body,
        token_stats,
        header_stats,
        module_symbols,
        import_environment,
    })
}

#[cfg(test)]
#[path = "tests/parse_file_headers_tests.rs"]
pub(crate) mod parse_file_headers_tests;
