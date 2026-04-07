use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::return_file_error;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

// ---------------------------------------------------------------------------
// Compile-time path value types
// ---------------------------------------------------------------------------

/// Whether a resolved compile-time path points at a file or a directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileTimePathKind {
    File,
    Directory,
}

/// How the path was resolved relative to the project layout.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CompileTimePathBase {
    /// Resolved relative to the importing file (`./` or `../`).
    RelativeToFile,
    /// First segment matched a configured `#root_folders` entry.
    ProjectRootFolder,
    /// Fell through to the configured `#entry_root`.
    EntryRoot,
}

/// A fully resolved compile-time path value.
///
/// WHAT: carries all semantic metadata the compiler needs for validation,
/// typed representation, and later string coercion of Beanstalk path literals.
///
/// WHY: path literals must be first-class compile-time values so that
/// `#origin` application, file/directory distinction, and public-path
/// formatting can be handled consistently in one place.
#[derive(Clone, Debug)]
pub struct CompileTimePath {
    /// The original syntactic path as written in source, normalized to
    /// Beanstalk components. Preserved for diagnostics and future path
    /// manipulation.
    pub source_path: InternedPath,

    /// The canonical filesystem path used for compile-time existence
    /// validation. This is an absolute path into the development tree.
    pub filesystem_path: PathBuf,

    /// The project-visible public path after resolution but *before*
    /// `#origin` application. This is the path that string coercion
    /// should render (plus optional origin prefix).
    pub public_path: InternedPath,

    /// How the path resolved semantically – determines whether `#origin`
    /// is applied during string coercion.
    pub base: CompileTimePathBase,

    /// Whether the target is a file or a directory.
    pub kind: CompileTimePathKind,
}

/// A collection of one or more resolved compile-time path values.
///
/// WHAT: wraps multiple resolved paths from a single path expression.
/// WHY: grouped path syntax (`@dir {a, b}`) produces multiple paths
///      from one token. This type carries them as a unit so expressions
///      and string coercion can handle the 1-or-many case uniformly.
#[derive(Clone, Debug)]
pub struct CompileTimePaths {
    pub paths: Vec<CompileTimePath>,
}

/// WHAT: resolves project-aware import paths using the configured entry root and explicit root folders.
/// WHY: Stage 0 discovery and later frontend import normalization must use identical path rules.
#[derive(Clone, Debug)]
pub(crate) struct ProjectPathResolver {
    project_root: PathBuf,
    entry_root: PathBuf,
    root_folder_names: HashSet<String>,
}

impl ProjectPathResolver {
    /// WHAT: creates a resolver from canonical project and entry roots.
    /// WHY: import normalization depends on a stable filesystem view of the project layout.
    pub(crate) fn new(
        project_root: PathBuf,
        entry_root: PathBuf,
        root_folders: &[PathBuf],
    ) -> Result<Self, CompilerError> {
        Ok(Self {
            project_root,
            entry_root,
            root_folder_names: collect_root_folder_names(root_folders)?,
        })
    }

    /// WHAT: exposes the canonical entry root for module discovery and diagnostics.
    /// WHY: callers need one canonical source of truth after config parsing.
    pub(crate) fn entry_root(&self) -> &Path {
        &self.entry_root
    }

    /// WHAT: derive a portable logical source path from a canonical filesystem file path.
    /// WHY: frontend identity should preserve import semantics without leaking machine-local paths.
    pub(crate) fn logical_path_for_canonical_file(
        &self,
        canonical_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<PathBuf, CompilerError> {
        if let Ok(relative_to_entry_root) = canonical_file.strip_prefix(&self.entry_root) {
            return Ok(relative_to_entry_root.to_path_buf());
        }

        let mut sorted_root_folders = self
            .root_folder_names
            .iter()
            .map(String::as_str)
            .collect::<Vec<_>>();
        sorted_root_folders.sort_unstable();

        for root_folder_name in sorted_root_folders {
            let root_folder_path = self.project_root.join(root_folder_name);
            if canonical_file.starts_with(&root_folder_path)
                && let Ok(relative_to_project_root) =
                    canonical_file.strip_prefix(&self.project_root)
            {
                return Ok(relative_to_project_root.to_path_buf());
            }
        }

        if let Ok(relative_to_project_root) = canonical_file.strip_prefix(&self.project_root) {
            return Ok(relative_to_project_root.to_path_buf());
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

    /// WHAT: resolves one import path to a concrete `.bst` source file on disk.
    /// WHY: Stage 0 must follow the same root-folder rules the frontend uses later.
    pub(crate) fn resolve_import_to_file(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<PathBuf, CompilerError> {
        let (_, canonical) =
            self.resolve_import_as_compile_time_path(import_path, importer_file, string_table)?;
        Ok(canonical)
    }

    /// WHAT: resolves one import path to both a typed compile-time path and a canonical file path.
    /// WHY: imports use the same resolution model as general path literals, but additionally
    ///      apply `.bst` extension fallback logic. Returns both representations so callers
    ///      can choose what they need.
    pub(crate) fn resolve_import_as_compile_time_path(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<(CompileTimePath, PathBuf), CompilerError> {
        let (base_kind, filesystem_base) =
            self.resolve_path_base(import_path, importer_file, string_table)?;
        let normalized = join_and_normalize_path(&filesystem_base, import_path, string_table);

        for candidate in candidate_import_files(&normalized, import_path.len()) {
            if candidate.is_file() {
                let canonical = fs::canonicalize(&candidate).map_err(|error| {
                    CompilerError::file_error(
                        importer_file,
                        format!(
                            "Failed to canonicalize resolved import '{}': {error}",
                            import_path.to_portable_string(string_table)
                        ),
                        string_table,
                    )
                })?;
                let public_path = build_public_path(import_path, &base_kind, string_table);
                let ct_path = CompileTimePath {
                    source_path: import_path.clone(),
                    filesystem_path: canonical.clone(),
                    public_path,
                    base: base_kind,
                    kind: CompileTimePathKind::File,
                };
                return Ok((ct_path, canonical));
            }
        }

        Err(CompilerError::file_error(
            importer_file,
            format!(
                "Could not resolve import '{}'. Non-relative imports first match configured '#root_folders' from the project root and otherwise fall back to the entry root '{}'.",
                import_path.to_portable_string(string_table),
                self.entry_root.display()
            ),
            string_table,
        ))
    }

    /// WHAT: rejects entry-root folders that can never be reached through non-relative imports.
    /// WHY: configured root folders win before the entry-root fallback, so matching source folder names become dead paths.
    pub(crate) fn validate_entry_root_collisions(
        &self,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        let entries = fs::read_dir(&self.entry_root).map_err(|error| {
            CompilerError::file_error(
                &self.entry_root,
                format!(
                    "Failed to read configured entry root '{}' while validating '#root_folders': {error}",
                    self.entry_root.display()
                ),
                string_table,
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                CompilerError::file_error(
                    &self.entry_root,
                    format!("Failed to read entry-root directory entry: {error}"),
                    string_table,
                )
            })?;
            let path = entry.path();

            if !path.is_dir() {
                continue;
            }

            let Some(name) = path.file_name().and_then(|value| value.to_str()) else {
                continue;
            };

            if !self.root_folder_names.contains(name) {
                continue;
            }

            let unreachable_path = self.entry_root.join(name);
            return_file_error!(
                string_table,
                &unreachable_path,
                format!(
                    "The source folder '{}' is unreachable because '{}' is also configured in '#root_folders'.",
                    unreachable_path.display(),
                    name
                ),
                {
                    CompilationStage => String::from("Project Structure"),
                    PrimarySuggestion => String::from("Rename the folder inside '#entry_root' to a different name so imports can reach it through the entry-root fallback."),
                    AlternativeSuggestion => String::from("If that folder should be the explicit project-root import target instead, rename the '#root_folders' entry to a different top-level name."),
                }
            );
        }

        Ok(())
    }

    /// WHAT: returns whether the import path starts with a configured explicit root folder.
    /// WHY: explicit project-root imports should never fall through to the entry-root default.
    pub(crate) fn matches_root_folder(
        &self,
        import_path: &InternedPath,
        string_table: &StringTable,
    ) -> bool {
        import_path
            .as_components()
            .first()
            .map(|component| string_table.resolve(*component))
            .is_some_and(|segment| self.root_folder_names.contains(segment))
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
    ) -> Result<CompileTimePath, CompilerError> {
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
    ) -> Result<CompileTimePaths, CompilerError> {
        let mut resolved = Vec::with_capacity(paths.len());
        for path in paths {
            resolved.push(self.resolve_compile_time_path(path, importer_file, string_table)?);
        }
        Ok(CompileTimePaths { paths: resolved })
    }

    // -----------------------------------------------------------------------
    // Shared resolution helpers
    // -----------------------------------------------------------------------

    /// WHAT: determines the semantic base for a path and its filesystem root.
    /// WHY: import resolution and general path resolution share this logic.
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
        } else if self.matches_root_folder(path, string_table) {
            Ok((
                CompileTimePathBase::ProjectRootFolder,
                self.project_root.clone(),
            ))
        } else {
            Ok((CompileTimePathBase::EntryRoot, self.entry_root.clone()))
        }
    }

    /// WHAT: rejects paths that would escape the project root after normalization.
    /// WHY: paths outside the project root are a semantic error in Beanstalk.
    fn validate_inside_project_root(
        &self,
        resolved: &Path,
        source_path: &InternedPath,
        importer_file: &Path,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
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
            return_file_error!(
                string_table,
                importer_file,
                format!(
                    "Compile-time path escapes the project root and is not allowed: '{}'",
                    source_path.to_portable_string(string_table),
                ),
                {
                    CompilationStage => String::from("Project Structure"),
                    PrimarySuggestion => String::from("Use a path inside the project root or move the target into the project."),
                }
            );
        }

        Ok(())
    }
}

/// WHAT: resolves the directory configured as the project entry root.
/// WHY: several build-system stages need the same entry-root interpretation.
pub(crate) fn resolve_project_entry_root(config: &Config) -> PathBuf {
    if config.entry_root.as_os_str().is_empty() {
        return config.entry_dir.clone();
    }

    if config.entry_root.is_absolute() {
        config.entry_root.clone()
    } else {
        config.entry_dir.join(&config.entry_root)
    }
}

fn collect_root_folder_names(root_folders: &[PathBuf]) -> Result<HashSet<String>, CompilerError> {
    let mut names = HashSet::with_capacity(root_folders.len());

    for root_folder in root_folders {
        let name = extract_root_folder_name(root_folder)?;
        names.insert(name);
    }

    Ok(names)
}

fn extract_root_folder_name(root_folder: &Path) -> Result<String, CompilerError> {
    let mut components = root_folder.components();
    let Some(first) = components.next() else {
        return Err(CompilerError::compiler_error(
            "Configured '#root_folders' entry cannot be empty.",
        ));
    };

    if components.next().is_some() {
        return Err(CompilerError::compiler_error(format!(
            "Configured '#root_folders' entry '{}' must be a single top-level folder name.",
            root_folder.display()
        )));
    }

    match first {
        Component::Normal(name) => Ok(name.to_string_lossy().to_string()),
        _ => Err(CompilerError::compiler_error(format!(
            "Configured '#root_folders' entry '{}' must be a relative top-level folder name.",
            root_folder.display()
        ))),
    }
}

fn is_relative_import_path(import_path: &InternedPath, string_table: &StringTable) -> bool {
    matches!(
        import_path
            .as_components()
            .first()
            .map(|component| string_table.resolve(*component)),
        Some(".") | Some("..")
    )
}

fn join_and_normalize_path(
    base: &Path,
    import_path: &InternedPath,
    string_table: &StringTable,
) -> PathBuf {
    let mut joined = base.to_path_buf();

    for component in import_path.as_components() {
        match string_table.resolve(*component) {
            "." => {}
            ".." => {
                joined.pop();
            }
            segment => joined.push(segment),
        }
    }

    joined
}

fn candidate_import_files(
    normalized_import_path: &Path,
    import_component_len: usize,
) -> Vec<PathBuf> {
    let mut candidates = Vec::with_capacity(2);
    candidates.push(with_bst_extension(normalized_import_path.to_path_buf()));

    if import_component_len > 1
        && let Some(parent) = normalized_import_path.parent()
    {
        candidates.push(with_bst_extension(parent.to_path_buf()));
    }

    candidates
}

fn with_bst_extension(path: PathBuf) -> PathBuf {
    if path.extension() == Some(OsStr::new(BEANSTALK_FILE_EXTENSION)) {
        path
    } else {
        path.with_extension(BEANSTALK_FILE_EXTENSION)
    }
}

// ---------------------------------------------------------------------------
// Compile-time path helpers
// ---------------------------------------------------------------------------

/// WHAT: checks that the resolved filesystem target exists and classifies it.
/// WHY: compile-time path validation requires the target to exist.
fn classify_existing_target(
    filesystem_path: &Path,
    source_path: &InternedPath,
    importer_file: &Path,
    string_table: &mut StringTable,
) -> Result<CompileTimePathKind, CompilerError> {
    if filesystem_path.is_file() {
        Ok(CompileTimePathKind::File)
    } else if filesystem_path.is_dir() {
        Ok(CompileTimePathKind::Directory)
    } else {
        return_file_error!(
            string_table,
            importer_file,
            format!(
                "Compile-time path does not exist: '{}'",
                source_path.to_portable_string(string_table),
            ),
            {
                CompilationStage => String::from("Project Structure"),
                PrimarySuggestion => String::from("Check that the file or directory exists relative to the configured path base."),
            }
        );
    }
}

/// WHAT: builds the project-visible public path from a resolved path literal.
/// WHY: the public path is what string coercion renders; it differs from the
///      filesystem path by stripping the base and keeping the user-visible segments.
fn build_public_path(
    source_path: &InternedPath,
    base_kind: &CompileTimePathBase,
    string_table: &StringTable,
) -> InternedPath {
    // An empty source/public path under a rooted base represents the Beanstalk
    // public-root literal (`@/`). This is site-root semantics, not OS-root semantics.
    match base_kind {
        // Relative paths keep their original form as the public path.
        CompileTimePathBase::RelativeToFile => source_path.clone(),

        // Root-folder and entry-root paths keep the visible segments.
        // For root-folder paths the first segment is the folder name itself
        // which must be preserved. For entry-root paths, all segments are
        // visible. In both cases the source path already contains the
        // correct visible segments, so we can reuse it directly.
        CompileTimePathBase::ProjectRootFolder | CompileTimePathBase::EntryRoot => {
            // Strip leading `.` or `..` (should not be present for non-relative,
            // but guard defensively).
            let components = source_path.as_components();
            let skip = components
                .iter()
                .take_while(|c| {
                    let s = string_table.resolve(**c);
                    s == "." || s == ".."
                })
                .count();

            if skip == 0 {
                source_path.clone()
            } else {
                InternedPath::from_components(components[skip..].to_vec())
            }
        }
    }
}

/// WHAT: best-effort canonicalization that works even when the leaf doesn't
///       exist yet – walks up to the nearest existing ancestor.
/// WHY: `validate_inside_project_root` needs a canonical path for prefix
///      comparison, but the target file may not exist (we report that separately).
fn canonicalize_best_effort(path: &Path) -> PathBuf {
    // Try to canonicalize the entire path first.
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }

    // Walk up until we find an existing ancestor, collecting non-existent
    // tail segments as owned strings to avoid borrow conflicts.
    let mut existing = path.to_path_buf();
    let mut tail_components: Vec<String> = Vec::new();

    while !existing.exists() {
        if let Some(name) = existing.file_name().and_then(|n| n.to_str()) {
            tail_components.push(name.to_owned());
        }
        if !existing.pop() {
            return path.to_path_buf();
        }
    }

    let mut result = fs::canonicalize(&existing).unwrap_or(existing);
    for component in tail_components.iter().rev() {
        result.push(component);
    }
    result
}

#[cfg(test)]
#[path = "tests/path_resolution_tests.rs"]
mod tests;
