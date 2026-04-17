//! Frontend file identity types and source-file tables.
//!
//! WHAT: defines explicit file identifiers plus canonical/logical path metadata.
//! WHY: semantic identity must not be reconstructed from filesystem/path strings.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use rustc_hash::FxHashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct FileId(pub u32);

#[derive(Debug, Clone)]
pub struct SourceFileIdentity {
    pub file_id: FileId,
    pub canonical_os_path: PathBuf,
    pub logical_path: InternedPath,
}

#[derive(Debug, Clone, Default)]
pub struct SourceFileTable {
    files: Vec<SourceFileIdentity>,
    canonical_to_id: FxHashMap<PathBuf, FileId>,
}

impl SourceFileTable {
    pub fn empty() -> Self {
        Self::default()
    }

    /// Builds deterministic file identities for one module.
    ///
    /// WHAT: canonical files are sorted by logical path before IDs are assigned.
    /// WHY: deterministic ID assignment must not depend on host filesystem iteration order.
    pub fn build(
        canonical_files: &[PathBuf],
        entry_file_path: &Path,
        project_path_resolver: Option<&ProjectPathResolver>,
        string_table: &mut StringTable,
    ) -> Result<Self, CompilerError> {
        let fallback_root = entry_file_path
            .parent()
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));

        let mut rows = Vec::with_capacity(canonical_files.len());
        for canonical in canonical_files {
            let logical = if let Some(resolver) = project_path_resolver {
                resolver.logical_path_for_canonical_file(canonical, string_table)?
            } else {
                logical_path_for_single_file_mode(canonical, &fallback_root)
            };

            rows.push((canonical.clone(), logical));
        }

        rows.sort_by(|(_, left), (_, right)| {
            left.to_string_lossy()
                .replace('\\', "/")
                .cmp(&right.to_string_lossy().replace('\\', "/"))
        });

        let mut files = Vec::with_capacity(rows.len());
        let mut canonical_to_id = FxHashMap::default();
        for (index, (canonical, logical)) in rows.into_iter().enumerate() {
            let file_id = FileId(index as u32);
            let logical_path = InternedPath::from_path_buf(&logical, string_table);
            let identity = SourceFileIdentity {
                file_id,
                canonical_os_path: canonical.clone(),
                logical_path,
            };
            canonical_to_id.insert(canonical, file_id);
            files.push(identity);
        }

        Ok(Self {
            files,
            canonical_to_id,
        })
    }

    pub fn get_by_canonical_path(&self, canonical_path: &Path) -> Option<&SourceFileIdentity> {
        let file_id = self.canonical_to_id.get(canonical_path)?;
        self.get(*file_id)
    }

    pub fn get(&self, file_id: FileId) -> Option<&SourceFileIdentity> {
        self.files.get(file_id.0 as usize)
    }
}

fn logical_path_for_single_file_mode(canonical_file: &Path, source_root: &Path) -> PathBuf {
    if let Ok(relative) = canonical_file.strip_prefix(source_root) {
        return relative.to_path_buf();
    }

    canonical_file
        .file_name()
        .map(PathBuf::from)
        .unwrap_or_else(|| canonical_file.to_path_buf())
}
