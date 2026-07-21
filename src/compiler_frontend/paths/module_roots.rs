//! Prepared module-root lookup data for project-aware path resolution.
//!
//! WHAT: stores the canonical normal module-root records prepared by Stage 0 and provides
//! nearest-root lookups for the frontend resolver.
//! WHY: filesystem discovery and durable module identity belong to Stage 0. The frontend
//! consumes this narrow lookup table without discovering project structure or owning module
//! identity, roles or ancestry.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Internal index for one prepared normal module-root record.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct ModuleRootId(usize);

/// One canonical normal module root and its containing directory.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ModuleRootRecord {
    root_directory: PathBuf,
    root_file: PathBuf,
}

impl ModuleRootRecord {
    pub(crate) fn new(root_directory: PathBuf, root_file: PathBuf) -> Self {
        Self {
            root_directory,
            root_file,
        }
    }
}

/// Prepared normal module-root records and indexes used by path resolution.
///
/// Stage 0 builds this table from the normal roots of its durable module identity table. Support
/// and facade roots stay out of this table so import resolution and header-role lookup are
/// unchanged by Phase 2 identity work.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModuleRootTable {
    records: Vec<ModuleRootRecord>,
    by_directory: HashMap<PathBuf, ModuleRootId>,
    by_root_file: HashMap<PathBuf, ModuleRootId>,
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
        }

        table
    }

    pub(crate) fn root_directories(&self) -> impl Iterator<Item = &PathBuf> {
        self.records.iter().map(|record| &record.root_directory)
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
}
