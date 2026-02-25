//! Filesystem polling and debounce helpers for the dev server.
//!
//! The watcher is intentionally std-only and uses fingerprints to detect file add/remove/modify
//! events while ignoring the generated dev output tree.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant, SystemTime};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FileFingerprint {
    pub modified: SystemTime,
    pub len: u64,
}

pub fn collect_fingerprints(
    watch_root: &Path,
    output_dir: &Path,
) -> io::Result<HashMap<PathBuf, FileFingerprint>> {
    let mut fingerprints = HashMap::new();
    // Manual stack avoids recursion depth issues on nested project trees.
    let mut stack = vec![watch_root.to_path_buf()];

    while let Some(dir_path) = stack.pop() {
        if should_ignore_path(&dir_path, output_dir) {
            continue;
        }

        let entries = fs::read_dir(&dir_path)?;
        for entry_result in entries {
            let entry = entry_result?;
            let path = entry.path();

            if should_ignore_path(&path, output_dir) {
                continue;
            }

            let metadata = entry.metadata()?;
            if metadata.is_dir() {
                stack.push(path);
                continue;
            }

            if metadata.is_file() {
                let modified = metadata.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                fingerprints.insert(
                    path,
                    FileFingerprint {
                        modified,
                        len: metadata.len(),
                    },
                );
            }
        }
    }

    Ok(fingerprints)
}

pub fn detect_changes(
    previous: &HashMap<PathBuf, FileFingerprint>,
    current: &HashMap<PathBuf, FileFingerprint>,
) -> bool {
    if previous.len() != current.len() {
        return true;
    }

    previous
        .iter()
        .any(|(path, previous_fingerprint)| match current.get(path) {
            Some(current_fingerprint) => current_fingerprint != previous_fingerprint,
            None => true,
        })
}

pub fn should_trigger_debounced_build(
    dirty_since: Option<Instant>,
    debounce_window: Duration,
) -> bool {
    match dirty_since {
        Some(first_dirty_at) => first_dirty_at.elapsed() >= debounce_window,
        None => false,
    }
}

pub fn should_ignore_path(path: &Path, output_dir: &Path) -> bool {
    // Ignore generated build output to prevent rebuild loops.
    if path.starts_with(output_dir) {
        return true;
    }

    path.components()
        .any(|component| component.as_os_str() == OsStr::new(".git"))
}

#[cfg(test)]
#[path = "tests/watch_tests.rs"]
mod tests;
