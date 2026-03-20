use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::{BEANSTALK_FILE_EXTENSION, Config};
use crate::return_file_error;
use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs;
use std::path::{Component, Path, PathBuf};

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

    /// WHAT: rewrites an import into its canonical project-aware path form.
    /// WHY: later compiler stages should resolve imports deterministically instead of relying on suffix matches.
    pub(crate) fn normalize_import_path(
        &self,
        import_path: &InternedPath,
        importer_file: &InternedPath,
        string_table: &mut StringTable,
    ) -> Result<InternedPath, CompilerError> {
        let importer_path = importer_file.to_path_buf(string_table);
        let normalized =
            self.normalize_import_path_buf(import_path, &importer_path, string_table)?;
        Ok(InternedPath::from_path_buf(&normalized, string_table))
    }

    /// WHAT: resolves one import path to a concrete `.bst` source file on disk.
    /// WHY: Stage 0 must follow the same root-folder rules the frontend uses later.
    pub(crate) fn resolve_import_to_file(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &StringTable,
    ) -> Result<PathBuf, CompilerError> {
        let normalized =
            self.normalize_import_path_buf(import_path, importer_file, string_table)?;

        for candidate in candidate_import_files(&normalized, import_path.len()) {
            if candidate.is_file() {
                return fs::canonicalize(&candidate).map_err(|error| {
                    CompilerError::file_error(
                        importer_file,
                        format!(
                            "Failed to canonicalize resolved import '{}': {error}",
                            import_path.to_portable_string(string_table)
                        ),
                    )
                });
            }
        }

        Err(CompilerError::file_error(
            importer_file,
            format!(
                "Could not resolve import '{}'. Non-relative imports first match configured '#root_folders' from the project root and otherwise fall back to the entry root '{}'.",
                import_path.to_portable_string(string_table),
                self.entry_root.display()
            ),
        ))
    }

    /// WHAT: rejects entry-root folders that can never be reached through non-relative imports.
    /// WHY: configured root folders win before the entry-root fallback, so matching source folder names become dead paths.
    pub(crate) fn validate_entry_root_collisions(&self) -> Result<(), CompilerError> {
        let entries = fs::read_dir(&self.entry_root).map_err(|error| {
            CompilerError::file_error(
                &self.entry_root,
                format!(
                    "Failed to read configured entry root '{}' while validating '#root_folders': {error}",
                    self.entry_root.display()
                ),
            )
        })?;

        for entry in entries {
            let entry = entry.map_err(|error| {
                CompilerError::file_error(
                    &self.entry_root,
                    format!("Failed to read entry-root directory entry: {error}"),
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
                &unreachable_path,
                format!(
                    "The source folder '{}' is unreachable because '{}' is also configured in '#root_folders'.",
                    unreachable_path.display(),
                    name
                ),
                {
                    CompilationStage => "Project Structure",
                    PrimarySuggestion => "Rename the folder inside '#entry_root' to a different name so imports can reach it through the entry-root fallback.",
                    AlternativeSuggestion => "If that folder should be the explicit project-root import target instead, rename the '#root_folders' entry to a different top-level name.",
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

    fn normalize_import_path_buf(
        &self,
        import_path: &InternedPath,
        importer_file: &Path,
        string_table: &StringTable,
    ) -> Result<PathBuf, CompilerError> {
        let importer_dir = importer_file.parent().ok_or_else(|| {
            CompilerError::file_error(
                importer_file,
                "Could not determine parent directory for importing file.",
            )
        })?;

        let base = if is_relative_import_path(import_path, string_table) {
            importer_dir.to_path_buf()
        } else if self.matches_root_folder(import_path, string_table) {
            self.project_root.clone()
        } else {
            self.entry_root.clone()
        };

        Ok(join_and_normalize_path(&base, import_path, string_table))
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
