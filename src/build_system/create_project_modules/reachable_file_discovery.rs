//! BFS traversal over Beanstalk import graphs to find all reachable source files.
//!
//! Given an entry `.bst` file, walks its import declarations transitively to build the complete
//! set of source files that belong to a module. Also assembles `InputFile` payloads from those
//! paths for downstream compilation stages.
#![allow(clippy::result_large_err)]
// Stage 0 deliberately returns full diagnostic/infrastructure payloads in `SourceDiscoveryError`
// so import discovery does not erase source locations or downgrade filesystem failures.

use crate::build_system::build::InputFile;

use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::external_packages::ExternalPackageRegistry;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_normalization::join_and_normalize_path;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::external_import_providers::cache::ExternalImportCacheKey;
use crate::libraries::external_import_providers::cache::ExternalImportProviderCache;
use crate::libraries::external_import_providers::provider::{
    ExternalImportProvider, ExternalImportProviderContext, ExternalImportRequest,
};
use crate::libraries::external_import_providers::registry::ExternalImportProviderRegistry;
use crate::libraries::external_import_providers::resolution_table::ExternalImportResolutionTable;

use std::collections::{BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use super::import_scanning::extract_import_paths;
use super::source_discovery_error::SourceDiscoveryError;
use super::source_loading::extract_source_code;

/// Mutable external-import state shared across Stage 0 reachable-file discovery.
///
/// WHAT: groups provider metadata, the external package registry, and build-scoped provider
/// cache/table state.
/// WHY: Stage 0 needs to mutate provider results while walking imports, but callers should not
/// thread four closely related provider arguments through every discovery function.
pub(crate) struct ExternalImportDiscoveryState<'a> {
    pub(super) external_packages: &'a mut ExternalPackageRegistry,
    pub(super) providers: &'a ExternalImportProviderRegistry,
    pub(super) cache: &'a mut ExternalImportProviderCache,
    pub(super) resolution_table: &'a mut ExternalImportResolutionTable,
}

// -------------------------
//  Public API
// -------------------------

/// Collect all reachable source files for a given entry point and load their content.
pub(super) fn collect_reachable_input_files(
    entry_path: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<InputFile>, CompilerMessages> {
    // 1. Traverse the import graph to find all paths.
    let reachable_files = match discover_reachable_files(
        entry_path,
        project_path_resolver,
        style_directives,
        external_imports,
        string_table,
    ) {
        Ok(files) => files,
        Err(error) => return Err(error.into_messages(string_table)),
    };

    // 2. Load the content of each discovered file.
    let mut input_files = Vec::with_capacity(reachable_files.len());

    for source_path in reachable_files {
        let source_code = match extract_source_code(&source_path, string_table) {
            Ok(code) => code,
            Err(error) => return Err(SourceDiscoveryError::from(error).into_messages(string_table)),
        };

        input_files.push(InputFile {
            source_code,
            source_path,
        });
    }

    Ok(input_files)
}

// -------------------------
//  Reachable Discovery
// -------------------------

/// BFS over import declarations starting from `entry_point`.
///
/// WHAT: follows each file's declared imports, resolves them to canonical paths, and returns the
/// full ordered set of files reachable from the entry point.
/// WHY: the set is built with a `BTreeSet` so the output order is deterministic, and each path is
/// only visited once to handle import cycles safely.
pub(super) fn discover_reachable_files(
    entry_point: &Path,
    project_path_resolver: &ProjectPathResolver,
    style_directives: &StyleDirectiveRegistry,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<Vec<PathBuf>, SourceDiscoveryError> {
    let mut reachable = BTreeSet::new();
    let mut queue = VecDeque::new();

    // 1. Seed with entry point.
    queue.push_back(entry_point.to_path_buf());

    // 2. Seed all source library facade files so authored facade declarations are available.
    // WHY: imports may directly resolve to a target file after Stage 0 path scanning, but the
    // facade still needs to be compiled so its public declaration surface can be checked later.
    for facade_path in project_path_resolver.facade_files().values() {
        queue.push_back(facade_path.clone());
    }

    // 3. Process the queue.
    while let Some(next_file) = queue.pop_front() {
        let canonical_file = fs::canonicalize(&next_file).map_err(|error| {
            CompilerError::file_error(
                &next_file,
                format!("Failed to canonicalize module file path: {error}"),
                string_table,
            )
        })?;

        if !reachable.insert(canonical_file.clone()) {
            continue;
        }

        // 4. Extract imports from the current file.
        let import_paths = extract_import_paths(&canonical_file, style_directives, string_table)?;

        for import_path in &import_paths {
            // 5. Skip virtual package imports — AST resolution handles those.
            if external_imports
                .external_packages
                .is_virtual_package_import(import_path, string_table)
            {
                continue;
            }

            // 6. Check for unsupported builder-specific core packages.
            if let Some(package_path) = external_imports
                .external_packages
                .unsupported_known_package_import(import_path, string_table)
            {
                return Err(SourceDiscoveryError::from(
                    unsupported_builder_package_error(&canonical_file, package_path, string_table),
                ));
            }

            // 7. Detect provider-backed import prefixes (e.g. `./drawing.js` from
            //    `@./drawing.js/draw` or `@./drawing.js`).
            //    If a provider supports the extension, resolve the prefix, call the provider,
            //    and register the result. Do not add external files to the Beanstalk input list.
            if let Some((prefix_path, prefix_str, extension)) =
                provider_backed_import_prefix(import_path, string_table)
            {
                if let Some(provider) = external_imports.providers.find_by_extension(extension) {
                    resolve_provider_backed_import(
                        ProviderBackedImportRequest {
                            importer_canonical_path: &canonical_file,
                            import_path,
                            prefix_path: &prefix_path,
                            raw_prefix: &prefix_str,
                            provider,
                            project_path_resolver,
                        },
                        external_imports,
                        string_table,
                    )?;
                    continue;
                }

                // No provider registered for this extension — report unsupported extension.
                let extension_owned = extension.to_owned();
                return Err(SourceDiscoveryError::from(
                    unsupported_external_extension_error(
                        &canonical_file,
                        import_path,
                        &extension_owned,
                        string_table,
                    ),
                ));
            }

            // 8. Resolve the import to a filesystem path.
            let resolved = project_path_resolver
                .resolve_import_to_file_with_facade_fallback(
                    import_path,
                    &canonical_file,
                    string_table,
                )
                .map_err(SourceDiscoveryError::from)?;

            // 9. Ensure target module root facades are compiled for cross-module imports.
            // WHY: when an import resolves to an implementation file in another module root,
            //      the facade must be available so AST can validate boundary enforcement.
            if let Some(importer_root) = project_path_resolver.module_root_for_file(&canonical_file)
                && let Some(target_root) = project_path_resolver.module_root_for_file(&resolved)
                && importer_root != target_root
                && let Some(facade_path) = project_path_resolver
                    .module_root_facades()
                    .get(&target_root)
                && !reachable.contains(facade_path)
            {
                queue.push_back(facade_path.clone());
            }

            // 10. Queue the resolved implementation file if not already visited.
            if !reachable.contains(&resolved) {
                queue.push_back(resolved);
            }
        }
    }

    Ok(reachable.into_iter().collect())
}

// -------------------------
//  Provider-backed import resolution
// -------------------------

/// Scans the components of an import path and returns the first file prefix whose final component
/// has an explicit non-`.bst` extension.
///
/// WHAT: for grouped syntax such as `import @./drawing.js { draw }` the tokenized path is
/// `@./drawing.js/draw`; this helper extracts the prefix `./drawing.js` and the extension `js`.
/// For a bare namespace import such as `import @./helper.js` the path is `@./helper.js`; the
/// prefix is `./helper.js`.
/// WHY: provider resolution must happen for the file prefix, while any remaining components are
/// symbol names to be resolved inside the provider-created package.
fn provider_backed_import_prefix<'a>(
    import_path: &InternedPath,
    string_table: &'a StringTable,
) -> Option<(InternedPath, String, &'a str)> {
    let components = import_path.as_components();
    if components.is_empty() {
        return None;
    }

    // Walk components to find the provider-owned file segment. Any later path components are
    // grouped-import symbol names, not filesystem path segments.
    for (index, component) in components.iter().enumerate() {
        let segment = string_table.resolve(*component);
        let path = Path::new(segment);
        let Some(extension) = path.extension().and_then(|ext| ext.to_str()) else {
            continue;
        };

        if extension == "bst" {
            continue;
        }

        let prefix_components = components[..=index].to_vec();
        let prefix_path = InternedPath::from_components(prefix_components);
        let prefix_str = prefix_path.to_portable_string(string_table);
        return Some((prefix_path, prefix_str, extension));
    }

    None
}

struct ProviderBackedImportRequest<'a> {
    importer_canonical_path: &'a Path,
    import_path: &'a InternedPath,
    prefix_path: &'a InternedPath,
    raw_prefix: &'a str,
    provider: &'a std::sync::Arc<dyn ExternalImportProvider>,
    project_path_resolver: &'a ProjectPathResolver,
}

/// Resolves a provider-backed import prefix to a canonical filesystem path, checks the build cache,
/// calls the provider if needed, and records the result in the resolution table and package registry.
fn resolve_provider_backed_import(
    request: ProviderBackedImportRequest<'_>,
    external_imports: &mut ExternalImportDiscoveryState<'_>,
    string_table: &mut StringTable,
) -> Result<(), SourceDiscoveryError> {
    // Resolve the prefix to a canonical filesystem path without .bst extension or facade fallback.
    let canonical_source_path = resolve_provider_prefix_to_canonical_path(
        request.prefix_path,
        request.importer_canonical_path,
        request.project_path_resolver,
        string_table,
    )?;

    // Enforce module/library boundaries for provider-backed imports.
    // A file may only directly import a .js file that lives in the same module,
    // source library, or default entry-root area.
    check_provider_import_module_boundary(
        request.importer_canonical_path,
        &canonical_source_path,
        request.import_path,
        request.project_path_resolver,
        string_table,
    )?;

    // Build the cache key.
    let cache_key = ExternalImportCacheKey {
        canonical_source_path: canonical_source_path.clone(),
        provider_kind: request.provider.kind(),
    };

    // Use cached result when available.
    if let Some(cached) = external_imports.cache.get(&cache_key) {
        let source_file_logical = source_file_logical_path(
            request.importer_canonical_path,
            request.project_path_resolver,
            string_table,
        )?;
        let import_prefix_logical = source_file_logical_path(
            &canonical_source_path,
            request.project_path_resolver,
            string_table,
        )?;
        insert_external_import_resolution(
            external_imports.resolution_table,
            source_file_logical,
            request.raw_prefix,
            import_prefix_logical,
            cached.clone(),
        );
        return Ok(());
    }

    // Build the provider request.
    let provider_request = ExternalImportRequest {
        import_path: request.import_path.to_portable_string(string_table),
        canonical_source_path: canonical_source_path.clone(),
        source_location:
            crate::compiler_frontend::compiler_messages::source_location::SourceLocation::from_path(
                request.importer_canonical_path,
                string_table,
            ),
    };

    let result = {
        let mut context = ExternalImportProviderContext {
            package_registry: external_imports.external_packages,
            cache: external_imports.cache,
            string_table,
        };

        request
            .provider
            .resolve_external_import(provider_request, &mut context)
            .map_err(SourceDiscoveryError::from)?
    };

    if let Some(resolved) = result {
        external_imports.cache.insert(cache_key, resolved.clone());

        let source_file_logical = source_file_logical_path(
            request.importer_canonical_path,
            request.project_path_resolver,
            string_table,
        )?;
        let import_prefix_logical = source_file_logical_path(
            &canonical_source_path,
            request.project_path_resolver,
            string_table,
        )?;
        insert_external_import_resolution(
            external_imports.resolution_table,
            source_file_logical,
            request.raw_prefix,
            import_prefix_logical,
            resolved,
        );
    }

    Ok(())
}

fn insert_external_import_resolution(
    external_import_resolution_table: &mut ExternalImportResolutionTable,
    source_file_logical: String,
    raw_import_prefix: &str,
    logical_import_prefix: String,
    resolved: crate::libraries::external_import_providers::provider::ResolvedExternalImport,
) {
    external_import_resolution_table.insert(
        source_file_logical.clone(),
        logical_import_prefix.clone(),
        resolved.clone(),
    );

    if raw_import_prefix != logical_import_prefix {
        external_import_resolution_table.insert(source_file_logical, raw_import_prefix, resolved);
    }
}

/// Resolves a provider import prefix to a canonical filesystem path without appending `.bst` or
/// using facade fallback.
///
/// WHAT: reuses the normal base/boundary/case rules from `ProjectPathResolver` but skips the
/// `.bst` extension logic and facade fallback used for Beanstalk source imports.
fn resolve_provider_prefix_to_canonical_path(
    prefix_path: &InternedPath,
    importer_file: &Path,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<PathBuf, SourceDiscoveryError> {
    let (base_kind, filesystem_base) = project_path_resolver
        .resolve_path_base_for_provider(prefix_path, importer_file, string_table)
        .map_err(SourceDiscoveryError::from)?;

    let normalized = join_and_normalize_path(&filesystem_base, prefix_path, string_table);

    let canonical = fs::canonicalize(&normalized)
        .map_err(|error| {
            CompilerError::file_error(
                importer_file,
                format!(
                    "Failed to canonicalize external import prefix '{}': {error}",
                    normalized.display()
                ),
                string_table,
            )
        })
        .map_err(SourceDiscoveryError::from)?;

    crate::compiler_frontend::paths::import_resolution::validate_import_boundary(
        &canonical,
        &base_kind,
        &filesystem_base,
        prefix_path,
        importer_file,
        string_table,
    )
    .map_err(SourceDiscoveryError::from)?;

    Ok(canonical)
}

/// Derives the portable logical path for a canonical source file.
fn source_file_logical_path(
    canonical_file: &Path,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<String, SourceDiscoveryError> {
    let logical = project_path_resolver
        .logical_path_for_canonical_file(canonical_file, string_table)
        .map_err(SourceDiscoveryError::from)?;
    Ok(logical.to_string_lossy().replace('\\', "/"))
}

// -------------------------
//  Provider import boundary check
// -------------------------

/// Enforce that a provider-backed import does not cross a module or source-library boundary.
///
/// WHAT: .js files are private implementation details of the module or library that owns them.
///       Cross-module or cross-library .js imports bypass the facade and are rejected.
/// WHY: provider-backed imports must obey the same visibility boundaries as .bst source imports.
fn check_provider_import_module_boundary(
    importer_file: &Path,
    target_file: &Path,
    import_path: &InternedPath,
    project_path_resolver: &ProjectPathResolver,
    string_table: &mut StringTable,
) -> Result<(), SourceDiscoveryError> {
    let importer_container = provider_import_container(project_path_resolver, importer_file);
    let target_container = provider_import_container(project_path_resolver, target_file);

    if importer_container != target_container {
        let location = SourceLocation::from_path(importer_file, string_table);
        return Err(SourceDiscoveryError::from(
            CompilerDiagnostic::cross_module_import_not_exported(import_path.clone(), location),
        ));
    }

    Ok(())
}

/// Determine the boundary "container" of a file for provider import checks.
///
/// WHAT: returns the module root, source library root, or entry root that contains the file.
/// WHY: two files in the same container may freely import each other's .js files.
fn provider_import_container(
    project_path_resolver: &ProjectPathResolver,
    file: &Path,
) -> Option<PathBuf> {
    // Module roots are the most specific boundaries.
    if let Some(root) = project_path_resolver.module_root_for_file(file) {
        return Some(root);
    }

    // Source libraries are the next boundary.
    for root in project_path_resolver.source_library_roots().values() {
        if file.starts_with(root) {
            return Some(root.clone());
        }
    }

    // Everything under the entry root belongs to the default module.
    if file.starts_with(project_path_resolver.entry_root()) {
        return Some(project_path_resolver.entry_root().to_path_buf());
    }

    None
}

// -------------------------
//  Diagnostic Helpers
// -------------------------

fn unsupported_builder_package_error(
    importer: &Path,
    package_path: &str,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let package_path_id = string_table.intern(package_path);
    let location =
        crate::compiler_frontend::compiler_messages::source_location::SourceLocation::from_path(
            importer,
            string_table,
        );
    CompilerDiagnostic::unsupported_builder_package(package_path_id, location)
}

fn unsupported_external_extension_error(
    importer: &Path,
    import_path: &InternedPath,
    extension: &str,
    string_table: &mut StringTable,
) -> CompilerDiagnostic {
    let extension_id = string_table.intern(extension);
    let location =
        crate::compiler_frontend::compiler_messages::source_location::SourceLocation::from_path(
            importer,
            string_table,
        );
    CompilerDiagnostic::unsupported_external_extension(import_path.clone(), extension_id, location)
}
