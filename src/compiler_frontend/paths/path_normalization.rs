//! Low-level Beanstalk path normalization helpers.
//!
//! These helpers translate already-tokenized `InternedPath` components into filesystem candidate
//! paths and public path values. They do not own import visibility, facade policy, or diagnostic
//! construction.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::compile_time_paths::CompileTimePathBase;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::settings::BEANSTALK_FILE_EXTENSION;
use std::ffi::OsStr;
use std::fs;
use std::path::{Path, PathBuf};

/// WHAT: checks whether an import path contains any `..` components.
/// WHY: parent-directory traversal is not supported in Beanstalk imports.
pub(crate) fn import_contains_dotdot(
    import_path: &InternedPath,
    string_table: &StringTable,
) -> bool {
    import_path
        .as_components()
        .iter()
        .any(|component| string_table.resolve(*component) == "..")
}

pub(crate) fn is_relative_import_path(
    import_path: &InternedPath,
    string_table: &StringTable,
) -> bool {
    matches!(
        import_path
            .as_components()
            .first()
            .map(|component| string_table.resolve(*component)),
        Some(".") | Some("..")
    )
}

pub(crate) fn join_and_normalize_path(
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

pub(crate) fn candidate_import_files(
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

/// WHAT: builds the project-visible public path from a resolved path literal.
/// WHY: the public path is what string coercion renders; it differs from the filesystem path by
/// stripping the base and keeping the user-visible segments.
pub(crate) fn build_public_path(
    source_path: &InternedPath,
    base_kind: &CompileTimePathBase,
    string_table: &StringTable,
) -> InternedPath {
    // An empty source/public path under a rooted base represents the Beanstalk public-root
    // literal (`@/`). This is site-root semantics, not OS-root semantics.
    match base_kind {
        // Relative paths keep their original form as the public path.
        CompileTimePathBase::RelativeToFile => source_path.clone(),

        // Source-library and entry-root paths keep the visible segments. For source-library paths
        // the first segment is the library prefix, which must be preserved. For entry-root paths,
        // all segments are visible. In both cases the source path already contains the correct
        // visible segments, so we can reuse it directly.
        CompileTimePathBase::SourceLibraryRoot | CompileTimePathBase::EntryRoot => {
            // Strip leading `.` or `..` defensively; these should not be present for non-relative
            // paths.
            let components = source_path.as_components();
            let skip = components
                .iter()
                .take_while(|component| {
                    let segment = string_table.resolve(**component);
                    segment == "." || segment == ".."
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

/// WHAT: best-effort canonicalization that works even when the leaf doesn't exist yet.
/// WHY: project-root validation needs a canonical path for prefix comparison, but the target file
/// may not exist. Missing target diagnostics are reported separately.
pub(crate) fn canonicalize_best_effort(path: &Path) -> PathBuf {
    if let Ok(canonical) = fs::canonicalize(path) {
        return canonical;
    }

    let mut existing = path.to_path_buf();
    let mut tail_components: Vec<String> = Vec::new();

    while !existing.exists() {
        if let Some(name) = existing.file_name().and_then(|name| name.to_str()) {
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
