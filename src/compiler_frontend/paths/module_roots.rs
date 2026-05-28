//! Module-root discovery helpers for project-aware path resolution.
//!
//! A directory is a module root/boundary when it contains one or more special `#*.bst` files
//! excluding `#config.bst`. `#mod.bst` is the only public facade file; a module root without
//! `#mod.bst` exports nothing outside itself.

use crate::compiler_frontend::source_libraries::mod_file::{CONFIG_FILE_NAME, MOD_FILE_NAME};
use std::collections::{HashMap, HashSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

/// Whether a file name is a module-boundary special file (e.g. `#mod.bst`, `#page.bst`).
fn is_module_boundary_file_name(name: &str) -> bool {
    name.starts_with('#') && name.ends_with(".bst") && name != CONFIG_FILE_NAME
}

pub(crate) struct DiscoveredModuleRoots {
    pub(crate) module_roots: Vec<PathBuf>,
    pub(crate) module_roots_set: HashSet<PathBuf>,
    pub(crate) module_root_facades: HashMap<PathBuf, PathBuf>,
}

/// WHAT: discovers all module roots under the entry root.
/// WHY: facade fallback and module membership both depend on nearest module-root lookup.
pub(crate) fn discover_module_roots(entry_root: &Path) -> DiscoveredModuleRoots {
    let mut module_roots = Vec::new();
    let mut module_roots_set = HashSet::new();
    let mut module_root_facades = HashMap::new();

    let mut queue = VecDeque::new();
    queue.push_back(entry_root.to_path_buf());

    while let Some(dir) = queue.pop_front() {
        let entries = match fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        let mut has_boundary_file = false;
        let mut mod_file = None;
        let mut subdirs = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
            } else if let Some(name) = path.file_name().and_then(|name| name.to_str())
                && is_module_boundary_file_name(name)
            {
                has_boundary_file = true;
                if name == MOD_FILE_NAME {
                    mod_file = fs::canonicalize(&path).ok();
                }
            }
        }

        if has_boundary_file {
            let canonical_dir = fs::canonicalize(&dir).unwrap_or_else(|_| dir.clone());
            module_roots.push(canonical_dir.clone());
            module_roots_set.insert(canonical_dir.clone());
            if let Some(mod_path) = mod_file {
                module_root_facades.insert(canonical_dir, mod_path);
            }
        }

        for subdir in subdirs {
            queue.push_back(subdir);
        }
    }

    // Sort deepest first so nearest-ancestor lookup works.
    module_roots.sort_by(|a, b| {
        let depth_a = a.components().count();
        let depth_b = b.components().count();
        depth_b.cmp(&depth_a)
    });

    DiscoveredModuleRoots {
        module_roots,
        module_roots_set,
        module_root_facades,
    }
}

/// WHAT: returns the nearest discovered module root that contains the given file.
/// WHY: nearest-ancestor lookup determines which module a file belongs to.
pub(crate) fn module_root_for_file(module_roots: &[PathBuf], file: &Path) -> Option<PathBuf> {
    for root in module_roots {
        if file.starts_with(root) {
            return Some(root.clone());
        }
    }

    None
}
