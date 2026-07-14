//! Input normalization for the direct Beandown API.
//!
//! WHAT: turns file, directory, file-list, and in-memory requests into ordered source units.
//! WHY: compile orchestration should receive deterministic, duplicate-checked inputs without
//! owning filesystem traversal policy.

use crate::builder_surface::SourceFileKind;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::compiler_messages::CompilerDiagnostic;
use crate::compiler_frontend::compiler_messages::source_location::SourceLocation;
use crate::compiler_frontend::symbols::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::beandown::scope::{BeandownPathScope, BeandownScopeConstant};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BeandownCompileRequest {
    pub(crate) input: BeandownInput,
    pub(crate) default_module_constants: Vec<BeandownScopeConstant>,
    pub(crate) module_constants_by_path: Vec<BeandownPathScope>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BeandownInput {
    File(PathBuf),
    Directory { path: PathBuf, recursive: bool },
    Files(Vec<PathBuf>),
    Sources(Vec<BeandownSource>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct BeandownSource {
    pub(crate) display_path: PathBuf,
    pub(crate) source_text: String,
}

pub(super) struct BeandownSourceUnit {
    pub(super) source_path: PathBuf,
    pub(super) relative_path: Option<PathBuf>,
    pub(super) source_text: String,
}

impl BeandownCompileRequest {
    pub(super) fn collect_sources(
        self,
        string_table: &mut StringTable,
    ) -> Result<Vec<BeandownSourceUnit>, CompilerMessages> {
        self.validate_no_caller_scope_constants(string_table)?;

        let units = match self.input {
            BeandownInput::File(path) => vec![read_file_unit(path, None, string_table)?],

            BeandownInput::Directory { path, recursive } => {
                collect_directory_units(path, recursive, string_table)?
            }

            BeandownInput::Files(paths) => {
                let mut units = Vec::with_capacity(paths.len());
                for path in paths {
                    units.push(read_file_unit(path, None, string_table)?);
                }
                units
            }

            BeandownInput::Sources(sources) => sources
                .into_iter()
                .map(|source| BeandownSourceUnit {
                    source_path: normalize_path_for_identity(&source.display_path),
                    relative_path: None,
                    source_text: source.source_text,
                })
                .collect(),
        };

        reject_duplicate_source_paths(&units, string_table)?;

        Ok(units)
    }

    fn validate_no_caller_scope_constants(
        &self,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerMessages> {
        if let Some(scope) = self
            .module_constants_by_path
            .iter()
            .find(|scope| !scope.constants.is_empty())
        {
            let messages = unsupported_scope_constant_messages(&scope.source_path, string_table);
            return Err(messages);
        }

        if !self.default_module_constants.is_empty() {
            let location_path = match &self.input {
                BeandownInput::File(path) => path,
                BeandownInput::Directory { path, .. } => path,
                BeandownInput::Files(paths) => paths
                    .first()
                    .map(PathBuf::as_path)
                    .unwrap_or_else(|| Path::new("<beandown>")),
                BeandownInput::Sources(sources) => sources
                    .first()
                    .map(|source| source.display_path.as_path())
                    .unwrap_or_else(|| Path::new("<beandown>")),
            };
            let messages = unsupported_scope_constant_messages(location_path, string_table);
            return Err(messages);
        }

        Ok(())
    }
}

fn collect_directory_units(
    directory: PathBuf,
    recursive: bool,
    string_table: &mut StringTable,
) -> Result<Vec<BeandownSourceUnit>, CompilerMessages> {
    let root = canonicalize_path(&directory, string_table)?;
    let mut paths = Vec::new();
    collect_beandown_paths_in_directory(&root, recursive, &mut paths, string_table)?;

    paths.sort_by(|left, right| {
        normalized_relative_path(&root, left).cmp(&normalized_relative_path(&root, right))
    });

    let mut units = Vec::with_capacity(paths.len());
    for path in paths {
        let relative_path = normalized_relative_path(&root, &path);
        units.push(read_file_unit(path, Some(relative_path), string_table)?);
    }

    Ok(units)
}

fn collect_beandown_paths_in_directory(
    directory: &Path,
    recursive: bool,
    paths: &mut Vec<PathBuf>,
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let entries = read_directory_sorted(directory, string_table)?;

    for entry in entries {
        let path = entry.path();
        if path.is_dir() && recursive {
            collect_beandown_paths_in_directory(&path, recursive, paths, string_table)?;
        } else if path.is_file() && has_beandown_extension(&path) {
            paths.push(canonicalize_path(&path, string_table)?);
        }
    }

    Ok(())
}

fn read_directory_sorted(
    directory: &Path,
    string_table: &mut StringTable,
) -> Result<Vec<fs::DirEntry>, CompilerMessages> {
    let read_dir = fs::read_dir(directory).map_err(|error| {
        CompilerMessages::file_error(
            directory,
            format!(
                "Failed to read Beandown directory '{}': {error}",
                directory.display()
            ),
            &string_table.clone(),
        )
    })?;

    let mut entries = Vec::new();
    for entry in read_dir {
        let entry = entry.map_err(|error| {
            CompilerMessages::file_error(
                directory,
                format!(
                    "Failed to inspect Beandown directory entry in '{}': {error}",
                    directory.display()
                ),
                &string_table.clone(),
            )
        })?;
        entries.push(entry);
    }

    entries.sort_by_key(|entry| normalize_path_for_identity(&entry.path()));
    Ok(entries)
}

fn read_file_unit(
    path: PathBuf,
    relative_path: Option<PathBuf>,
    string_table: &mut StringTable,
) -> Result<BeandownSourceUnit, CompilerMessages> {
    let source_path = canonicalize_path(&path, string_table)?;
    let source_text = fs::read_to_string(&source_path).map_err(|error| {
        CompilerMessages::file_error(
            &source_path,
            format!(
                "Failed to read Beandown source '{}': {error}",
                source_path.display()
            ),
            &string_table.clone(),
        )
    })?;

    Ok(BeandownSourceUnit {
        source_path,
        relative_path,
        source_text,
    })
}

fn canonicalize_path(
    path: &Path,
    string_table: &mut StringTable,
) -> Result<PathBuf, CompilerMessages> {
    fs::canonicalize(path).map_err(|error| {
        CompilerMessages::file_error(
            path,
            format!(
                "Failed to resolve Beandown path '{}': {error}",
                path.display()
            ),
            &string_table.clone(),
        )
    })
}

fn reject_duplicate_source_paths(
    units: &[BeandownSourceUnit],
    string_table: &mut StringTable,
) -> Result<(), CompilerMessages> {
    let mut first_locations: HashMap<PathBuf, SourceLocation> = HashMap::new();
    let mut diagnostics = Vec::new();

    for unit in units {
        let normalized = normalize_path_for_identity(&unit.source_path);
        let location = SourceLocation::from_path(&unit.source_path, string_table);

        if let Some(first_location) = first_locations.get(&normalized) {
            let path = InternedPath::from_path_buf(&normalized, string_table);
            diagnostics.push(CompilerDiagnostic::duplicate_beandown_input_path(
                path,
                first_location.clone(),
                location,
            ));
        } else {
            first_locations.insert(normalized, location);
        }
    }

    if diagnostics.is_empty() {
        Ok(())
    } else {
        Err(CompilerMessages::from_diagnostics(
            diagnostics,
            string_table.clone(),
        ))
    }
}

fn unsupported_scope_constant_messages(
    location_path: &Path,
    string_table: &mut StringTable,
) -> CompilerMessages {
    let location = SourceLocation::from_path(location_path, string_table);
    let path = InternedPath::from_path_buf(location_path, string_table);
    let diagnostic = CompilerDiagnostic::invalid_beandown_api_scope_item(path, location);

    CompilerMessages::from_diagnostics(vec![diagnostic], string_table.clone())
}

fn normalized_relative_path(root: &Path, path: &Path) -> PathBuf {
    path.strip_prefix(root)
        .map(normalize_path_for_identity)
        .unwrap_or_else(|_| normalize_path_for_identity(path))
}

fn normalize_path_for_identity(path: &Path) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        let component_text = component.as_os_str().to_string_lossy().replace('\\', "/");
        normalized.push(component_text);
    }

    normalized
}

fn has_beandown_extension(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .and_then(SourceFileKind::from_extension)
        == Some(SourceFileKind::Beandown)
}
