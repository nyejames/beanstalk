//! Source library registry.
//!
//! WHAT: tracks builder-provided and project-local source library roots.
//! WHY: source library imports resolve to actual `.bst` files, not virtual packages,
//!      so the path resolver needs to know where each library prefix lives.

use std::collections::HashMap;
use std::path::PathBuf;

/// Registry of source library roots indexed by their import prefix.
///
/// WHAT: maps `@`-stripped import prefixes like `"html"` to filesystem roots.
/// WHY: the path resolver checks these prefixes before falling back to entry-root resolution.
#[derive(Clone, Debug, Default)]
pub struct SourceLibraryRegistry {
    roots: HashMap<String, SourceLibraryRoot>,
}

impl SourceLibraryRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a filesystem source library root.
    pub fn register_filesystem_root(&mut self, import_prefix: impl Into<String>, root: PathBuf) {
        let prefix = import_prefix.into();
        self.roots.insert(
            prefix.clone(),
            SourceLibraryRoot {
                import_prefix: prefix,
                root: ProvidedSourceRoot::Filesystem(root),
            },
        );
    }

    /// Returns the root for a given import prefix, if any.
    pub fn get_root(&self, prefix: &str) -> Option<&SourceLibraryRoot> {
        self.roots.get(prefix)
    }

    /// Returns true if the registry contains a root with the given prefix.
    pub fn has_prefix(&self, prefix: &str) -> bool {
        self.roots.contains_key(prefix)
    }

    /// Iterate over all registered roots.
    pub fn iter(&self) -> impl Iterator<Item = &SourceLibraryRoot> {
        self.roots.values()
    }

    /// Merge another registry into this one, returning collision errors.
    pub fn merge(&mut self, other: &SourceLibraryRegistry) -> Result<(), Vec<String>> {
        let mut collisions = Vec::new();
        let mut incoming_roots = other.roots.iter().collect::<Vec<_>>();
        // Keep multi-collision diagnostics stable across HashMap iteration order.
        incoming_roots.sort_by_key(|(prefix, _)| *prefix);

        for (prefix, root) in incoming_roots {
            if self.roots.contains_key(prefix) {
                collisions.push(prefix.clone());
            } else {
                self.roots.insert(prefix.clone(), root.clone());
            }
        }
        if collisions.is_empty() {
            Ok(())
        } else {
            Err(collisions)
        }
    }
}

/// One source library root.
#[derive(Clone, Debug)]
pub struct SourceLibraryRoot {
    pub import_prefix: String,
    pub root: ProvidedSourceRoot,
}

/// Where a source library's files live.
#[derive(Clone, Debug)]
pub enum ProvidedSourceRoot {
    /// Files on the local filesystem.
    Filesystem(PathBuf),
}
