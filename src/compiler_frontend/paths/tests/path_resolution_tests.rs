//! Unit tests for compile-time path resolution.

use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_resolution::{
    CompileTimePathBase, CompileTimePathKind, ProjectPathResolver,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use std::fs;
use std::path::PathBuf;

/// Creates a temp directory tree and a resolver for testing.
struct TestHarness {
    project_root: PathBuf,
    resolver: ProjectPathResolver,
    string_table: StringTable,
    _temp_dir: tempfile::TempDir,
}

impl TestHarness {
    fn new(root_folders: &[&str]) -> Self {
        Self::with_source_libraries(
            root_folders,
            &crate::libraries::SourceLibraryRegistry::default(),
        )
    }

    fn with_source_libraries(
        root_folders: &[&str],
        source_libraries: &crate::libraries::SourceLibraryRegistry,
    ) -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let project_root = temp_dir.path().to_path_buf();
        let entry_root = project_root.join("src");

        // Create entry root and some fixtures.
        fs::create_dir_all(&entry_root).unwrap();
        fs::create_dir_all(project_root.join("assets/images")).unwrap();
        fs::create_dir_all(project_root.join("src/pages")).unwrap();
        fs::create_dir_all(project_root.join("docs")).unwrap();
        fs::write(project_root.join("assets/images/logo.png"), b"").unwrap();
        fs::write(project_root.join("src/pages/about.bst"), b"").unwrap();
        fs::write(project_root.join("src/index.bst"), b"").unwrap();
        fs::write(project_root.join("docs/readme.txt"), b"").unwrap();

        let root_folder_paths: Vec<PathBuf> = root_folders.iter().map(PathBuf::from).collect();

        let resolver = ProjectPathResolver::new(
            project_root.clone(),
            entry_root,
            &root_folder_paths,
            source_libraries,
        )
        .expect("resolver creation should succeed");

        TestHarness {
            project_root,
            resolver,
            string_table: StringTable::new(),
            _temp_dir: temp_dir,
        }
    }

    fn make_path(&mut self, components: &[&str]) -> InternedPath {
        let mut path = InternedPath::new();
        for c in components {
            path.push_str(c, &mut self.string_table);
        }
        path
    }

    fn importer(&self) -> PathBuf {
        self.project_root.join("src/index.bst")
    }
}

// -----------------------------------------------------------------------
// Relative file resolution
// -----------------------------------------------------------------------

#[test]
fn relative_file_resolves_from_importer_directory() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&[".", "pages", "about.bst"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("relative file should resolve");

    assert_eq!(result.base, CompileTimePathBase::RelativeToFile);
    assert_eq!(result.kind, CompileTimePathKind::File);
    assert!(result.filesystem_path.ends_with("src/pages/about.bst"));
}

// -----------------------------------------------------------------------
// Relative directory resolution
// -----------------------------------------------------------------------

#[test]
fn relative_directory_resolves_and_classifies_as_directory() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&[".", "pages"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("relative directory should resolve");

    assert_eq!(result.base, CompileTimePathBase::RelativeToFile);
    assert_eq!(result.kind, CompileTimePathKind::Directory);
}

// -----------------------------------------------------------------------
// Root folder resolution
// -----------------------------------------------------------------------

#[test]
fn root_folder_file_resolves_from_project_root() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&["assets", "images", "logo.png"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("root folder file should resolve");

    assert_eq!(result.base, CompileTimePathBase::ProjectRootFolder);
    assert_eq!(result.kind, CompileTimePathKind::File);
    assert!(result.filesystem_path.ends_with("assets/images/logo.png"));

    // Public path should preserve the root folder segment.
    let public = result.public_path.to_portable_string(&h.string_table);
    assert_eq!(public, "assets/images/logo.png");
}

// -----------------------------------------------------------------------
// Entry root fallback resolution
// -----------------------------------------------------------------------

#[test]
fn entry_root_file_resolves_through_fallback() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&["pages", "about.bst"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("entry root file should resolve");

    assert_eq!(result.base, CompileTimePathBase::EntryRoot);
    assert_eq!(result.kind, CompileTimePathKind::File);
}

// -----------------------------------------------------------------------
// Non-existent target rejection
// -----------------------------------------------------------------------

#[test]
fn non_existent_target_is_rejected() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&["assets", "does_not_exist.png"]);
    let importer = h.importer();

    let err = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect_err("missing file should produce error");

    assert!(err.msg.contains("does not exist"));
}

// -----------------------------------------------------------------------
// Project root escape rejection
// -----------------------------------------------------------------------

#[test]
fn path_escaping_project_root_is_rejected() {
    let mut h = TestHarness::new(&["assets"]);
    // From src/index.bst, going ../../.. escapes the project root.
    let path = h.make_path(&[".", "..", "..", "..", "escape.txt"]);
    let importer = h.importer();

    let err = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect_err("escape should produce error");

    assert!(err.msg.contains("escapes the project root"));
}

// -----------------------------------------------------------------------
// File vs directory classification
// -----------------------------------------------------------------------

#[test]
fn root_folder_directory_classifies_correctly() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&["assets", "images"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("directory should resolve");

    assert_eq!(result.kind, CompileTimePathKind::Directory);
}

// -----------------------------------------------------------------------
// Public path segment preservation
// -----------------------------------------------------------------------

#[test]
fn relative_path_public_path_keeps_dot_prefix() {
    let mut h = TestHarness::new(&["assets"]);
    let path = h.make_path(&[".", "pages", "about.bst"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("should resolve");

    let public = result.public_path.to_portable_string(&h.string_table);
    assert!(public.starts_with("./"));
}

// -----------------------------------------------------------------------
// Multi-path resolution (`resolve_compile_time_paths`)
// -----------------------------------------------------------------------

#[test]
fn resolve_compile_time_paths_resolves_multiple_paths() {
    let mut h = TestHarness::new(&["assets"]);
    let path_a = h.make_path(&["assets", "images", "logo.png"]);
    let path_b = h.make_path(&[".", "pages", "about.bst"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_paths(&[path_a, path_b], &importer, &mut h.string_table)
        .expect("multi-path resolution should succeed");

    assert_eq!(result.paths.len(), 2);
    assert_eq!(result.paths[0].base, CompileTimePathBase::ProjectRootFolder);
    assert_eq!(result.paths[0].kind, CompileTimePathKind::File);
    assert_eq!(result.paths[1].base, CompileTimePathBase::RelativeToFile);
    assert_eq!(result.paths[1].kind, CompileTimePathKind::File);
}

#[test]
fn resolve_compile_time_paths_fails_if_any_path_missing() {
    let mut h = TestHarness::new(&["assets"]);
    let good = h.make_path(&["assets", "images", "logo.png"]);
    let bad = h.make_path(&["assets", "nonexistent.txt"]);
    let importer = h.importer();

    let err = h
        .resolver
        .resolve_compile_time_paths(&[good, bad], &importer, &mut h.string_table)
        .expect_err("should fail when any path is missing");

    assert!(err.msg.contains("does not exist"));
}

#[test]
fn empty_path_resolves_as_entry_root_public_directory() {
    let mut h = TestHarness::new(&["assets"]);
    let path = InternedPath::new();
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect("empty path should resolve to entry root");

    assert_eq!(result.base, CompileTimePathBase::EntryRoot);
    assert_eq!(result.kind, CompileTimePathKind::Directory);
    assert_eq!(result.filesystem_path, h.project_root.join("src"));
    assert!(result.public_path.as_components().is_empty());
}

#[test]
fn source_library_import_resolves_to_library_root() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");
    let library_root = project_root.join("lib/helper");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(&library_root).unwrap();
    fs::write(library_root.join("utils.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let mut source_libraries = crate::libraries::SourceLibraryRegistry::new();
    source_libraries.register_filesystem_root("helper", library_root.clone());

    let resolver = ProjectPathResolver::new(
        project_root.clone(),
        entry_root.clone(),
        &[],
        &source_libraries,
    )
    .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("utils", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect("source library import should resolve");

    assert_eq!(result.0.base, CompileTimePathBase::SourceLibraryRoot);
    assert_eq!(
        result.1,
        fs::canonicalize(library_root.join("utils.bst")).unwrap(),
        "should resolve to source library root file"
    );
}

#[test]
fn source_library_prefix_takes_priority_over_entry_root() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");
    let library_root = project_root.join("lib/helper");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(&library_root).unwrap();
    fs::write(library_root.join("utils.bst"), b"").unwrap();
    // Also create a conflicting file under entry root.
    fs::create_dir_all(entry_root.join("helper")).unwrap();
    fs::write(entry_root.join("helper/utils.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let mut source_libraries = crate::libraries::SourceLibraryRegistry::new();
    source_libraries.register_filesystem_root("helper", library_root.clone());

    let resolver = ProjectPathResolver::new(
        project_root.clone(),
        entry_root.clone(),
        &[],
        &source_libraries,
    )
    .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("utils", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect("source library import should resolve");

    assert_eq!(result.0.base, CompileTimePathBase::SourceLibraryRoot);
    assert_eq!(
        result.1,
        fs::canonicalize(library_root.join("utils.bst")).unwrap()
    );
}
