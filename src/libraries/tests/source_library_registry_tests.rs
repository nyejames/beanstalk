//! Unit tests for the source-library registry.

use crate::libraries::{ProvidedSourceRoot, SourceLibraryRegistry};

#[test]
fn iter_returns_roots_in_canonical_prefix_order() {
    let mut registry = SourceLibraryRegistry::new();
    registry.register_filesystem_root("zeta", std::path::PathBuf::from("/lib/zeta"));
    registry.register_filesystem_root("alpha", std::path::PathBuf::from("/lib/alpha"));
    registry.register_filesystem_root("middle", std::path::PathBuf::from("/lib/middle"));

    let prefixes: Vec<&str> = registry
        .iter()
        .map(|root| root.import_prefix.as_str())
        .collect();
    assert_eq!(prefixes, vec!["alpha", "middle", "zeta"]);
}

#[test]
fn merge_reports_collisions_in_canonical_prefix_order() {
    let mut builder = SourceLibraryRegistry::new();
    builder.register_filesystem_root("zeta", std::path::PathBuf::from("/builder/zeta"));
    builder.register_filesystem_root("alpha", std::path::PathBuf::from("/builder/alpha"));

    let mut project_local = SourceLibraryRegistry::new();
    project_local.register_filesystem_root("zeta", std::path::PathBuf::from("/local/zeta"));
    project_local.register_filesystem_root("beta", std::path::PathBuf::from("/local/beta"));
    project_local.register_filesystem_root("alpha", std::path::PathBuf::from("/local/alpha"));

    let mut merged = builder.clone();
    let collisions = merged
        .merge(&project_local)
        .expect_err("overlapping prefixes should collide");

    assert_eq!(collisions, vec!["alpha", "zeta"]);
}

#[test]
fn merge_adds_non_overlapping_roots_in_canonical_order() {
    let mut builder = SourceLibraryRegistry::new();
    builder.register_filesystem_root("html", std::path::PathBuf::from("/builder/html"));

    let mut project_local = SourceLibraryRegistry::new();
    project_local.register_filesystem_root("widgets", std::path::PathBuf::from("/lib/widgets"));
    project_local.register_filesystem_root("alpha", std::path::PathBuf::from("/lib/alpha"));

    let mut merged = builder.clone();
    merged
        .merge(&project_local)
        .expect("non-overlapping prefixes should merge cleanly");

    let prefixes: Vec<&str> = merged
        .iter()
        .map(|root| root.import_prefix.as_str())
        .collect();
    assert_eq!(prefixes, vec!["alpha", "html", "widgets"]);
}

#[test]
fn get_root_and_has_prefix_work_after_merge() {
    let mut registry = SourceLibraryRegistry::new();
    registry.register_filesystem_root("html", std::path::PathBuf::from("/builder/html"));

    assert!(registry.has_prefix("html"));
    assert!(!registry.has_prefix("missing"));

    let root = registry.get_root("html").expect("html root should exist");
    assert!(
        matches!(&root.root, ProvidedSourceRoot::Filesystem(path) if path == std::path::Path::new("/builder/html"))
    );
}
