//! Project-aware path resolution facade.
//!
//! `ProjectPathResolver` keeps the public resolution surface for Stage 0, headers, AST folding,
//! and builder-facing path tracking. The data contracts, module-root scanning, and path
//! normalization helpers live in sibling modules so this file can focus on orchestration and
//! diagnostic boundaries.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::compiler_messages::{
    CompilerDiagnostic, DiagnosticKind, ImportDiagnosticKind, InvalidCompileTimePathReason,
    InvalidConfigReason, InvalidImportPathReason,
};
use crate::compiler_frontend::paths::compile_time_paths::{
    CompileTimePath, CompileTimePathBase, CompileTimePathKind, CompileTimePathResolutionError,
    CompileTimePaths, classify_existing_target,
};
use crate::compiler_frontend::paths::import_resolution::{
    ImportPathResolutionError, validate_import_boundary, validate_import_case_sensitivity,
};
use crate::compiler_frontend::paths::module_roots::ModuleRootTable;
use crate::compiler_frontend::paths::path_normalization::{
    ImportCandidate, ImportCandidateSupport, build_public_path,
    candidate_import_files_for_source_kinds, canonicalize_best_effort, import_contains_dotdot,
    is_relative_import_path, join_and_normalize_path,
};
use crate::compiler_frontend::source_libraries::root_file::MOD_FILE_NAME;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::{SourceFileKind, SourceFileKindRegistry, SourceLibraryRegistry};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Controls which import roots are acceptable for a given compilation context.
///
/// WHAT: determines whether relative, entry-root fallback, and project-local imports are allowed.
/// WHY: config files may only import from builder/core source libraries and external packages,
///      while normal modules can use all import roots.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ImportRootPolicy {
    /// All import roots are allowed (normal project mode).
    Normal,
    /// Only source-library roots and external package imports are allowed (config mode).
    SourceLibrariesAndExternalPackagesOnly,
}

/// Concrete source-file import selected by path resolution.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedImportFile {
    pub(crate) path: PathBuf,
    pub(crate) kind: SourceFileKind,
}

/// WHAT: resolves project-aware import paths using the configured entry root and source libraries.
/// WHY: Stage 0 discovery and later frontend import normalization must use identical path rules.
#[derive(Clone, Debug)]
pub(crate) struct ProjectPathResolver {
    project_root: PathBuf,
    entry_root: PathBuf,
    source_library_roots: HashMap<String, PathBuf>,
    /// Maps library prefix to the canonical path of its `#mod.bst` facade file, if present.
    facade_files: HashMap<String, PathBuf>,
    /// Module roots prepared by Stage 0. Resolver construction never discovers them.
    module_roots: ModuleRootTable,
    /// Import root policy enforced during import resolution.
    import_root_policy: ImportRootPolicy,
    /// Builder-supported source file kinds available for this project.
    source_file_kinds: SourceFileKindRegistry,
}

impl ProjectPathResolver {
    /// WHAT: creates a resolver from canonical project and entry roots.
    /// WHY: import normalization depends on a stable filesystem view of the project layout.
    pub(crate) fn new(
        project_root: PathBuf,
        entry_root: PathBuf,
        source_libraries: &SourceLibraryRegistry,
        source_file_kinds: &SourceFileKindRegistry,
    ) -> Result<Self, CompilerError> {
        Self::new_with_module_roots(
            project_root,
            entry_root,
            source_libraries,
            source_file_kinds,
            ModuleRootTable::empty(),
        )
    }

    /// WHAT: creates a resolver from canonical roots and Stage 0 module-root data.
    /// WHY: path resolution may query prepared module boundaries, but it must not perform
    /// filesystem discovery during normal directory construction.
    pub(crate) fn new_with_module_roots(
        project_root: PathBuf,
        entry_root: PathBuf,
        source_libraries: &SourceLibraryRegistry,
        source_file_kinds: &SourceFileKindRegistry,
        module_roots: ModuleRootTable,
    ) -> Result<Self, CompilerError> {
        let total_start = crate::timing::start_pipeline_timing();

        let source_library_roots_start = crate::timing::start_pipeline_timing();
        let mut source_library_roots = HashMap::new();
        for root in source_libraries.iter() {
            // Currently only filesystem roots are supported; embedded roots will be added later.
            let crate::libraries::ProvidedSourceRoot::Filesystem(path) = &root.root;
            // Keep a non-canonical fallback so Stage 0 can still report typed source-library
            // facade diagnostics for configured roots that exist logically but fail early
            // canonicalization in test or project setup contexts.
            let canonical_root = fs::canonicalize(path).unwrap_or_else(|_| path.clone());
            source_library_roots.insert(root.import_prefix.clone(), canonical_root);
        }
        log_path_resolver_timing(
            "stage0.path_resolver.source_library_roots",
            source_library_roots_start,
        );

        // Discover facade files (`#mod.bst`) in each source library root.
        let source_library_facades_start = crate::timing::start_pipeline_timing();
        let mut facade_files = HashMap::new();
        for (prefix, root) in &source_library_roots {
            let mod_file = root.join(MOD_FILE_NAME);
            if mod_file.is_file()
                && let Ok(canonical) = fs::canonicalize(&mod_file)
            {
                facade_files.insert(prefix.clone(), canonical);
            }
        }
        log_path_resolver_timing(
            "stage0.path_resolver.source_library_facades",
            source_library_facades_start,
        );

        let resolver = Self {
            project_root,
            entry_root,
            source_library_roots,
            facade_files,
            module_roots,
            import_root_policy: ImportRootPolicy::Normal,
            source_file_kinds: source_file_kinds.clone(),
        };

        log_path_resolver_timing("stage0.path_resolver.total", total_start);

        Ok(resolver)
    }

    /// Set the import root policy for this resolver.
    ///
    /// WHY: config files restrict imports to source libraries and external packages only.
    pub(crate) fn with_import_root_policy(mut self, policy: ImportRootPolicy) -> Self {
        self.import_root_policy = policy;
        self
    }

    /// WHAT: exposes the canonical entry root for module discovery and diagnostics.
    /// WHY: callers need one canonical source of truth after config parsing.
    pub(crate) fn entry_root(&self) -> &Path {
        &self.entry_root
    }

    /// WHAT: returns the map of source library roots.
    pub(crate) fn source_library_roots(&self) -> &HashMap<String, PathBuf> {
        &self.source_library_roots
    }

    /// WHAT: returns the map of discovered facade files.
    pub(crate) fn facade_files(&self) -> &HashMap<String, PathBuf> {
        &self.facade_files
    }

    pub(crate) fn module_root_facades(&self) -> &HashMap<PathBuf, PathBuf> {
        self.module_roots.facade_files()
    }

    pub(crate) fn module_roots(&self) -> impl Iterator<Item = &PathBuf> {
        self.module_roots.root_directories()
    }

    /// WHAT: returns the builder-supported source file kinds for this project.
    /// WHY: Stage 0 and import resolution need to know which non-`.bst` extensions are valid.
    pub(crate) fn source_file_kinds(&self) -> &SourceFileKindRegistry {
        &self.source_file_kinds
    }

    /// WHAT: returns the module root that contains the given file.
    /// WHY: nearest-ancestor lookup determines which module a file belongs to.
    pub(crate) fn module_root_for_file(&self, file: &Path) -> Option<PathBuf> {
        self.module_roots.module_root_for_file(file)
    }

    /// WHAT: derive a portable logical source path from a canonical filesystem file path.
    /// WHY: frontend identity should preserve import semantics without leaking machine-local paths.
    ///
    /// NOTE: `string_table` is only used on error paths to intern diagnostic file paths.
    pub(crate) fn logical_path_for_canonical_file(
        &self,
        canonical_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<PathBuf, CompilerError> {
        if let Ok(relative_to_entry_root) = canonical_file.strip_prefix(&self.entry_root) {
            return Ok(relative_to_entry_root.to_path_buf());
        }

        if let Ok(relative_to_project_root) = canonical_file.strip_prefix(&self.project_root) {
            return Ok(relative_to_project_root.to_path_buf());
        }

        // Source library files may live outside the project root (builder-provided).
        // Derive a logical path relative to the library root, prefixed with the library name.
        let mut sorted_library_prefixes: Vec<_> = self.source_library_roots.iter().collect();
        sorted_library_prefixes.sort_by_key(|(prefix, _)| *prefix);
        for (prefix, root) in sorted_library_prefixes {
            if let Ok(relative_to_library_root) = canonical_file.strip_prefix(root) {
                let mut logical = PathBuf::from(prefix);
                logical.push(relative_to_library_root);
                return Ok(logical);
            }
        }

        Err(CompilerError::file_error(
            canonical_file,
            format!(
                "Source file '{}' is outside both entry root '{}' and project root '{}'",
                canonical_file.display(),
                self.entry_root.display(),
                self.project_root.display()
            ),
            string_table,
        ))
    }

    /// WHAT: resolves an import path to a concrete source file and its source kind.
    /// WHY: Stage 0 must preserve the source kind so `.bd` files can be discovered without being
    ///      scanned or prepared as normal Beanstalk source.
    pub(crate) fn resolve_import_to_source_file(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<ResolvedImportFile, ImportPathResolutionError> {
        let (_, canonical) =
            self.resolve_import_as_compile_time_path(import_path, importer_file, string_table)?;
        let source_kind = self
            .source_kind_for_canonical_path(&canonical)
            .ok_or_else(|| {
                CompilerError::file_error(
                    importer_file,
                    format!(
                        "Resolved import '{}' to '{}' but could not determine its source kind.",
                        import_path.to_portable_string(string_table),
                        canonical.display()
                    ),
                    string_table,
                )
            })?;

        Ok(ResolvedImportFile {
            path: canonical,
            kind: source_kind,
        })
    }

    /// WHAT: resolves an import path with facade fallback while preserving source kind.
    /// WHY: Stage 0 needs source kind for implementation files and Beanstalk kind for facade files.
    pub(crate) fn resolve_import_to_source_file_with_facade_fallback(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<ResolvedImportFile, ImportPathResolutionError> {
        match self.resolve_import_to_source_file(import_path, importer_file, string_table) {
            Ok(resolved) => Ok(resolved),
            Err(original_error) => {
                if !is_missing_import_target_error(&original_error) {
                    return Err(original_error);
                }

                if let Some(facade_file) = self.resolve_facade_fallback(import_path, string_table) {
                    Ok(ResolvedImportFile {
                        path: facade_file,
                        kind: SourceFileKind::Beanstalk,
                    })
                } else {
                    if self.import_root_policy
                        == ImportRootPolicy::SourceLibrariesAndExternalPackagesOnly
                    {
                        return Err(original_error);
                    }

                    match self.resolve_module_root_facade_fallback(
                        import_path,
                        importer_file,
                        string_table,
                    ) {
                        Ok(Some(facade_file)) => Ok(ResolvedImportFile {
                            path: facade_file,
                            kind: SourceFileKind::Beanstalk,
                        }),
                        Ok(None) => Err(original_error),
                        Err(diagnostic_error) => Err(diagnostic_error),
                    }
                }
            }
        }
    }

    /// WHAT: checks whether an import path targets a library with a facade, and if so,
    /// returns the facade file path.
    fn resolve_facade_fallback(
        &self,
        import_path: &InternedPath,
        string_table: &StringTable,
    ) -> Option<PathBuf> {
        let first_component = import_path.as_components().first()?;
        let prefix = string_table.resolve(*first_component);
        self.facade_files.get(prefix).cloned()
    }

    /// WHAT: checks whether an import path targets a regular module root with a facade,
    /// and if so, returns the facade file path. If the target is a module root without a facade,
    /// returns a diagnostic so the caller can report a clear missing-facade error.
    /// WHY: regular module roots (under the entry root) use `#mod.bst` as their outward-facing
    ///      export surface. Plain folder imports must resolve to the facade only when it exists.
    fn resolve_module_root_facade_fallback(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<Option<PathBuf>, ImportPathResolutionError> {
        let (_, filesystem_base) = self
            .resolve_path_base(import_path, importer_file, string_table)
            .map_err(ImportPathResolutionError::from)?;

        let normalized = join_and_normalize_path(&filesystem_base, import_path, string_table);

        // Walk up from the normalized path itself to find the nearest module root.
        // WHY: a plain folder import like `@helper` normalizes to `.../helper`; we must check
        //      `helper/` itself as a module root before walking to its parents.
        let mut current = normalized.clone();
        loop {
            // Canonicalize before lookup because Stage 0 stores canonical module-root paths.
            // On macOS, temp directories are under /var which symlinks to /private/var,
            // so non-canonical paths won't match canonicalized module roots.
            let lookup_current = fs::canonicalize(&current).unwrap_or_else(|_| current.clone());

            if self.module_roots.contains_directory(&lookup_current) {
                let canonical_importer =
                    fs::canonicalize(importer_file).unwrap_or_else(|_| importer_file.to_path_buf());
                let importer_root = self.module_root_for_file(&canonical_importer);

                // Same-module imports do not need facade fallback.
                if importer_root.as_ref() == Some(&lookup_current) {
                    return Ok(None);
                }

                if let Some(facade_path) = self.module_root_facades().get(&lookup_current) {
                    return Ok(Some(facade_path.clone()));
                }

                // Target module root has no facade.
                let location = SourceLocation::from_path(importer_file, string_table);
                return Err(ImportPathResolutionError::Diagnostic(Box::new(
                    CompilerDiagnostic::missing_module_facade(import_path.clone(), location),
                )));
            }
            if !current.pop() {
                break;
            }
        }

        Ok(None)
    }

    /// WHAT: resolves one import path to both a typed compile-time path and a canonical file path.
    /// WHY: imports use the same resolution model as general path literals, but additionally
    ///      apply `.bst` extension fallback logic. Returns both representations so callers
    ///      can choose what they need.
    ///
    /// NOTE: `string_table` is used for diagnostic path interning and case-mismatch strings.
    pub(crate) fn resolve_import_as_compile_time_path(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<(CompileTimePath, PathBuf), ImportPathResolutionError> {
        if let Some(extension) = explicit_source_extension(import_path, string_table) {
            let location = SourceLocation::from_path(importer_file, string_table);
            let diagnostic = if extension == SourceFileKind::Beanstalk.extension() {
                CompilerDiagnostic::explicit_bst_extension(import_path.to_owned(), location)
            } else {
                let extension_id = string_table.intern(&extension);
                CompilerDiagnostic::explicit_source_extension(
                    import_path.to_owned(),
                    extension_id,
                    location,
                )
            };
            return Err(ImportPathResolutionError::Diagnostic(Box::new(diagnostic)));
        }

        if import_contains_dotdot(import_path, string_table) {
            let location = SourceLocation::from_path(importer_file, string_table);
            let diagnostic = CompilerDiagnostic::invalid_import_path(
                import_path.to_owned(),
                InvalidImportPathReason::ParentDirectorySegment,
                location,
            );
            return Err(ImportPathResolutionError::Diagnostic(Box::new(diagnostic)));
        }

        let (base_kind, filesystem_base) =
            self.resolve_path_base(import_path, importer_file, string_table)?;

        // Enforce import root policy for config-mode restrictions.
        if self.import_root_policy == ImportRootPolicy::SourceLibrariesAndExternalPackagesOnly {
            match base_kind {
                CompileTimePathBase::RelativeToFile
                    if self.importer_is_inside_source_library(importer_file) => {}
                CompileTimePathBase::RelativeToFile | CompileTimePathBase::EntryRoot => {
                    let location = SourceLocation::from_path(importer_file, string_table);
                    return Err(ImportPathResolutionError::Diagnostic(Box::new(
                        CompilerDiagnostic::invalid_config_reason(
                            None,
                            InvalidConfigReason::ConfigImportRootViolation,
                            location,
                        ),
                    )));
                }
                CompileTimePathBase::SourceLibraryRoot => {}
            }
        }

        // Source library roots already include the prefix directory, so skip the first
        // component when joining to avoid double-prefixing (e.g. `lib/helper/helper/...`).
        let normalized = if matches!(base_kind, CompileTimePathBase::SourceLibraryRoot) {
            let components = import_path.as_components();
            let suffix = if components.len() <= 1 {
                InternedPath::new()
            } else {
                InternedPath::from_components(components[1..].to_vec())
            };
            join_and_normalize_path(&filesystem_base, &suffix, string_table)
        } else {
            join_and_normalize_path(&filesystem_base, import_path, string_table)
        };

        let candidates = candidate_import_files_for_source_kinds(
            &normalized,
            import_path.len(),
            self.source_file_kinds(),
        );
        let existing_candidates = existing_import_candidates(&candidates);
        let folder_exists = normalized.is_dir();

        if existing_candidates.len() + usize::from(folder_exists) > 1 {
            let location = SourceLocation::from_path(importer_file, string_table);
            let diagnostic =
                CompilerDiagnostic::ambiguous_import_target(import_path.to_owned(), location);
            return Err(ImportPathResolutionError::Diagnostic(Box::new(diagnostic)));
        }

        let Some(candidate) = existing_candidates.first() else {
            let location = SourceLocation::from_path(importer_file, string_table);
            return Err(ImportPathResolutionError::Diagnostic(Box::new(
                CompilerDiagnostic::missing_import_target(import_path.clone(), location),
            )));
        };

        if candidate.support == ImportCandidateSupport::RecognizedButUnsupported {
            let location = SourceLocation::from_path(importer_file, string_table);
            let extension_id = string_table.intern(candidate.kind.extension());
            let diagnostic = CompilerDiagnostic::unsupported_source_file_kind(
                import_path.to_owned(),
                extension_id,
                location,
            );
            return Err(ImportPathResolutionError::Diagnostic(Box::new(diagnostic)));
        }

        let canonical = fs::canonicalize(&candidate.path).map_err(|error| {
            CompilerError::file_error(
                importer_file,
                format!(
                    "Failed to canonicalize resolved import '{}': {error}",
                    import_path.to_portable_string(string_table)
                ),
                string_table,
            )
        })?;

        validate_import_boundary(
            &canonical,
            &base_kind,
            &filesystem_base,
            import_path,
            importer_file,
            string_table,
        )?;
        validate_import_case_sensitivity(
            import_path,
            &base_kind,
            &filesystem_base,
            &canonical,
            candidate.is_parent_fallback,
            importer_file,
            string_table,
        )?;

        let public_path = build_public_path(import_path, &base_kind, string_table);
        let ct_path = CompileTimePath {
            source_path: import_path.clone(),
            filesystem_path: canonical.clone(),
            public_path,
            base: base_kind,
            kind: CompileTimePathKind::File,
        };
        Ok((ct_path, canonical))
    }

    /// WHAT: returns whether the import path starts with a registered source library prefix.
    /// WHY: source library imports should resolve to the library root, not fall through to entry root.
    fn matches_source_library_prefix(
        &self,
        import_path: &InternedPath,
        string_table: &StringTable,
    ) -> Option<PathBuf> {
        let first_component = import_path.as_components().first()?;
        let segment = string_table.resolve(*first_component);
        self.source_library_roots.get(segment).cloned()
    }

    /// WHAT: checks whether a file already admitted to config parsing belongs to a source library.
    /// WHY: `config.bst` cannot use relative imports, but builder/core source-library facades
    /// often re-export support declarations through relative imports inside the library root.
    fn importer_is_inside_source_library(&self, importer_file: &Path) -> bool {
        let canonical_importer =
            fs::canonicalize(importer_file).unwrap_or_else(|_| importer_file.to_path_buf());

        self.source_library_roots
            .values()
            .any(|library_root| canonical_importer.starts_with(library_root))
    }

    // -----------------------------------------------------------------------
    // Compile-time path literal resolution (non-import general paths)
    // -----------------------------------------------------------------------

    /// WHAT: resolves a general path literal to a typed compile-time path value.
    /// WHY: all Beanstalk path literals must use the same resolution rules as
    ///       imports, but additionally classify file vs directory, reject
    ///       escapes outside the project root, and carry public-path metadata.
    pub(crate) fn resolve_compile_time_path(
        &self,
        path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<CompileTimePath, CompileTimePathResolutionError> {
        let (base_kind, filesystem_base) =
            self.resolve_path_base(path, importer_file, string_table)?;

        let filesystem_path = join_and_normalize_path(&filesystem_base, path, string_table);

        self.validate_inside_project_root(&filesystem_path, path, importer_file, string_table)?;

        let kind = classify_existing_target(&filesystem_path, path, importer_file, string_table)?;

        let public_path = build_public_path(path, &base_kind, string_table);

        Ok(CompileTimePath {
            source_path: path.clone(),
            filesystem_path,
            public_path,
            base: base_kind,
            kind,
        })
    }

    /// WHAT: resolves all paths in a `Vec<InternedPath>` to typed compile-time values.
    /// WHY: grouped path syntax produces multiple `InternedPath`s from one token;
    ///      each must be resolved independently through the same rules.
    pub(crate) fn resolve_compile_time_paths(
        &self,
        paths: &[InternedPath],
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<CompileTimePaths, CompileTimePathResolutionError> {
        let mut resolved = Vec::with_capacity(paths.len());
        for path in paths {
            resolved.push(self.resolve_compile_time_path(path, importer_file, string_table)?);
        }
        Ok(CompileTimePaths { paths: resolved })
    }

    // -----------------------------------------------------------------------
    // Shared resolution helpers
    // -----------------------------------------------------------------------

    /// WHAT: exposes the normal path base calculation for provider-backed external files.
    /// WHY: Stage 0 external providers need the same relative/library/module boundary base as
    /// Beanstalk imports, but they must not append `.bst` or use facade fallback.
    ///
    /// NOTE: `string_table` is only used on error paths to intern diagnostic file paths.
    pub(crate) fn resolve_path_base_for_provider(
        &self,
        path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<(CompileTimePathBase, PathBuf), CompilerError> {
        self.resolve_path_base(path, importer_file, string_table)
    }

    fn resolve_path_base(
        &self,
        path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<(CompileTimePathBase, PathBuf), CompilerError> {
        let importer_dir = importer_file.parent().ok_or_else(|| {
            CompilerError::file_error(
                importer_file,
                "Could not determine parent directory for importing file.",
                string_table,
            )
        })?;

        if is_relative_import_path(path, string_table) {
            Ok((
                CompileTimePathBase::RelativeToFile,
                importer_dir.to_path_buf(),
            ))
        } else if let Some(library_root) = self.matches_source_library_prefix(path, string_table) {
            Ok((CompileTimePathBase::SourceLibraryRoot, library_root))
        } else {
            Ok((CompileTimePathBase::EntryRoot, self.entry_root.clone()))
        }
    }

    fn source_kind_for_canonical_path(&self, path: &Path) -> Option<SourceFileKind> {
        let extension = path.extension().and_then(|extension| extension.to_str())?;
        SourceFileKind::from_extension(extension)
    }

    /// WHAT: rejects paths that would escape the project root after normalization.
    /// WHY: paths outside the project root are a semantic error in Beanstalk.
    ///
    /// NOTE: `string_table` is only used on error paths to intern diagnostic file paths.
    fn validate_inside_project_root(
        &self,
        resolved: &Path,
        source_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<(), CompileTimePathResolutionError> {
        // Canonicalize the project root once (it must exist).
        let canonical_root = fs::canonicalize(&self.project_root).map_err(|error| {
            CompilerError::file_error(
                &self.project_root,
                format!(
                    "Failed to canonicalize project root '{}': {error}",
                    self.project_root.display()
                ),
                string_table,
            )
        })?;

        // The resolved path may not exist yet (that check comes next), so we
        // walk up to the nearest existing ancestor and canonicalize from there,
        // then re-append the remaining tail.
        let canonical_resolved = canonicalize_best_effort(resolved);

        if !canonical_resolved.starts_with(&canonical_root) {
            let location = SourceLocation::from_path(importer_file, string_table);
            let diagnostic = CompilerDiagnostic::invalid_compile_time_path(
                source_path.clone(),
                InvalidCompileTimePathReason::EscapesProjectRoot,
                location,
            );

            return Err(CompileTimePathResolutionError::Diagnostic(Box::new(
                diagnostic,
            )));
        }

        Ok(())
    }
}

fn explicit_source_extension(
    import_path: &InternedPath,
    string_table: &StringTable,
) -> Option<String> {
    for component in import_path.as_components() {
        let segment = string_table.resolve(*component);
        let Some(extension) = Path::new(segment)
            .extension()
            .and_then(|extension| extension.to_str())
        else {
            continue;
        };

        if SourceFileKind::from_extension(extension).is_some() {
            return Some(extension.to_owned());
        }
    }

    None
}

fn is_missing_import_target_error(error: &ImportPathResolutionError) -> bool {
    matches!(
        error,
        ImportPathResolutionError::Diagnostic(diagnostic)
            if matches!(
                diagnostic.kind,
                DiagnosticKind::Import(ImportDiagnosticKind::MissingImportTarget)
            )
    )
}

fn existing_import_candidates(candidates: &[ImportCandidate]) -> Vec<&ImportCandidate> {
    candidates
        .iter()
        .filter(|candidate| candidate.path.is_file())
        .collect()
}

#[cfg(test)]
#[path = "tests/path_resolution_tests.rs"]
mod tests;

/// Record a path-resolver stage timing through the central `timers` substrate.
///
/// WHAT: delegates to `timing::record_started_pipeline_timing`, which stores the
///      observation in the active collection scope and emits the stable
///      `BST_BENCH timing` line when the output mode permits.
/// WHY:  path-resolver construction uses dotted `stage0.path_resolver.*` metric
///      names. The start token is zero-sized when `timers` is off, so regular
///      builds do not read clocks for instrumentation-only measurements.
fn log_path_resolver_timing(metric: &str, start: crate::timing::PipelineTimingStart) {
    crate::timing::record_started_pipeline_timing(metric, start);
}
