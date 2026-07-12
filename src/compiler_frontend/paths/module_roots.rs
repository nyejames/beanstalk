//! Prepared module-root lookup data for project-aware path resolution.
//!
//! WHAT: stores the canonical module-root records prepared by Stage 0 and provides nearest-root
//! lookups for the frontend.
//! WHY: filesystem traversal belongs to the build system. The frontend consumes this table
//! without discovering project structure during resolver construction.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Stable identity for one prepared module-root record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct ModuleRootId(usize);

/// One canonical hash-root file and its containing module directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModuleRootRecord {
    root_directory: PathBuf,
    root_file: PathBuf,
    export_file: Option<PathBuf>,
}

impl ModuleRootRecord {
    pub(crate) fn with_export_file(
        root_directory: PathBuf,
        root_file: PathBuf,
        export_file: Option<PathBuf>,
    ) -> Self {
        Self {
            root_directory,
            root_file,
            export_file,
        }
    }
}

/// Prepared module-root records and indexes used by path resolution.
///
/// The export-file map is an index over the same records, not an independently discovered
/// filesystem view. It remains available to the current facade/export consumers until their later
/// roadmap phase replaces the filename-specific surface.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleRootTable {
    records: Vec<ModuleRootRecord>,
    by_directory: HashMap<PathBuf, ModuleRootId>,
    by_root_file: HashMap<PathBuf, ModuleRootId>,
    export_files: HashMap<PathBuf, PathBuf>,
}

impl ModuleRootTable {
    pub(crate) fn empty() -> Self {
        Self::default()
    }

    pub(crate) fn from_records(mut records: Vec<ModuleRootRecord>) -> Self {
        records.sort_by(|left, right| {
            left.root_directory
                .cmp(&right.root_directory)
                .then_with(|| left.root_file.cmp(&right.root_file))
        });

        let mut table = Self {
            records,
            ..Self::default()
        };

        for (index, record) in table.records.iter().enumerate() {
            let id = ModuleRootId(index);
            table.by_directory.insert(record.root_directory.clone(), id);
            table.by_root_file.insert(record.root_file.clone(), id);

            if let Some(export_file) = &record.export_file {
                table
                    .export_files
                    .insert(record.root_directory.clone(), export_file.clone());
            }
        }

        table
    }

    pub(crate) fn root_directories(&self) -> impl Iterator<Item = &PathBuf> {
        self.records.iter().map(|record| &record.root_directory)
    }

    pub(crate) fn export_files(&self) -> &HashMap<PathBuf, PathBuf> {
        &self.export_files
    }

    pub(crate) fn root_file_for_directory(&self, directory: &Path) -> Option<&Path> {
        let module_root_id = self.by_directory.get(directory)?;
        Some(self.records[module_root_id.0].root_file.as_path())
    }

    pub(crate) fn is_root_file(&self, file: &Path) -> bool {
        self.by_root_file.contains_key(file)
    }

    pub(crate) fn module_root_for_file(&self, file: &Path) -> Option<PathBuf> {
        let mut current = file.parent();

        while let Some(directory) = current {
            if let Some(module_root_id) = self.by_directory.get(directory) {
                return Some(self.records[module_root_id.0].root_directory.clone());
            }

            current = directory.parent();
        }

        None
    }

    pub(crate) fn contains_directory(&self, directory: &Path) -> bool {
        self.by_directory.contains_key(directory)
    }
}
