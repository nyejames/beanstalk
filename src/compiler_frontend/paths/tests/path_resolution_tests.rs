//! Unit tests for compile-time path resolution.

use crate::compiler_frontend::compiler_messages::render::{
    DiagnosticRenderContext, terse::format_terse_diagnostic_with_context,
};
use crate::compiler_frontend::compiler_messages::{
    DiagnosticPayload, ImportDiagnosticKind, InvalidCompileTimePathReason, InvalidImportPathReason,
    RuleDiagnosticKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::compile_time_paths::{
    CompileTimePathBase, CompileTimePathKind, CompileTimePathResolutionError,
};
use crate::compiler_frontend::paths::import_resolution::ImportPathResolutionError;
use crate::compiler_frontend::paths::path_resolution::ProjectPathResolver;
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
    fn new() -> Self {
        Self::with_source_libraries(&crate::libraries::SourceLibraryRegistry::default())
    }

    fn with_source_libraries(source_libraries: &crate::libraries::SourceLibraryRegistry) -> Self {
        let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
        let project_root = temp_dir.path().to_path_buf();
        let entry_root = project_root.join("src");

        // Create entry root and some fixtures.
        fs::create_dir_all(&entry_root).unwrap();
        fs::create_dir_all(entry_root.join("assets/images")).unwrap();
        fs::create_dir_all(entry_root.join("pages")).unwrap();
        fs::create_dir_all(project_root.join("docs")).unwrap();
        fs::write(entry_root.join("assets/images/logo.png"), b"").unwrap();
        fs::write(entry_root.join("pages/about.bst"), b"").unwrap();
        fs::write(entry_root.join("index.bst"), b"").unwrap();
        fs::write(project_root.join("docs/readme.txt"), b"").unwrap();

        let resolver = ProjectPathResolver::new(project_root.clone(), entry_root, source_libraries)
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

fn rendered_error_msg(error: &ImportPathResolutionError, string_table: &StringTable) -> String {
    match error {
        ImportPathResolutionError::Diagnostic(diagnostic) => format_terse_diagnostic_with_context(
            diagnostic,
            DiagnosticRenderContext::new(string_table),
        ),
        ImportPathResolutionError::Infrastructure(error) => error.msg.clone(),
    }
}

fn import_diagnostic_payload(error: &ImportPathResolutionError) -> &DiagnosticPayload {
    let diagnostic = typed_import_diagnostic(error);

    assert_eq!(
        diagnostic.kind,
        crate::compiler_frontend::compiler_messages::DiagnosticKind::Import(
            ImportDiagnosticKind::InvalidImportPath
        )
    );

    &diagnostic.payload
}

fn typed_import_diagnostic(
    error: &ImportPathResolutionError,
) -> &crate::compiler_frontend::compiler_messages::CompilerDiagnostic {
    let ImportPathResolutionError::Diagnostic(diagnostic) = error else {
        panic!("expected typed import diagnostic, got infrastructure error");
    };

    diagnostic
}

fn compile_time_path_diagnostic_payload(
    error: &CompileTimePathResolutionError,
) -> &DiagnosticPayload {
    let CompileTimePathResolutionError::Diagnostic(diagnostic) = error else {
        panic!("expected typed compile-time path diagnostic, got infrastructure error");
    };

    assert_eq!(
        diagnostic.kind,
        crate::compiler_frontend::compiler_messages::DiagnosticKind::Rule(
            RuleDiagnosticKind::InvalidCompileTimePath
        )
    );

    &diagnostic.payload
}

// -----------------------------------------------------------------------
// Relative file resolution
// -----------------------------------------------------------------------

#[test]
fn relative_file_resolves_from_importer_directory() {
    let mut h = TestHarness::new();
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
    let mut h = TestHarness::new();
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
// Entry root fallback resolution
// -----------------------------------------------------------------------

#[test]
fn entry_root_file_resolves_through_fallback() {
    let mut h = TestHarness::new();
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
    let mut h = TestHarness::new();
    let path = h.make_path(&["pages", "does_not_exist.bst"]);
    let importer = h.importer();

    let err = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect_err("missing file should produce error");

    assert!(matches!(
        compile_time_path_diagnostic_payload(&err),
        DiagnosticPayload::InvalidCompileTimePath {
            reason: InvalidCompileTimePathReason::MissingTarget,
            ..
        }
    ));
}

// -----------------------------------------------------------------------
// Project root escape rejection
// -----------------------------------------------------------------------

#[test]
fn path_escaping_project_root_is_rejected() {
    let mut h = TestHarness::new();
    // From src/index.bst, going ../../.. escapes the project root.
    let path = h.make_path(&[".", "..", "..", "..", "escape.txt"]);
    let importer = h.importer();

    let err = h
        .resolver
        .resolve_compile_time_path(&path, &importer, &mut h.string_table)
        .expect_err("escape should produce error");

    assert!(matches!(
        compile_time_path_diagnostic_payload(&err),
        DiagnosticPayload::InvalidCompileTimePath {
            reason: InvalidCompileTimePathReason::EscapesProjectRoot,
            ..
        }
    ));
}

// -----------------------------------------------------------------------
// File vs directory classification
// -----------------------------------------------------------------------

#[test]
fn entry_root_directory_classifies_correctly() {
    let mut h = TestHarness::new();
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
    let mut h = TestHarness::new();
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
    let mut h = TestHarness::new();
    let path_a = h.make_path(&["assets", "images", "logo.png"]);
    let path_b = h.make_path(&[".", "pages", "about.bst"]);
    let importer = h.importer();

    let result = h
        .resolver
        .resolve_compile_time_paths(&[path_a, path_b], &importer, &mut h.string_table)
        .expect("multi-path resolution should succeed");

    assert_eq!(result.paths.len(), 2);
    assert_eq!(result.paths[0].base, CompileTimePathBase::EntryRoot);
    assert_eq!(result.paths[0].kind, CompileTimePathKind::File);
    assert_eq!(result.paths[1].base, CompileTimePathBase::RelativeToFile);
    assert_eq!(result.paths[1].kind, CompileTimePathKind::File);
}

#[test]
fn resolve_compile_time_paths_fails_if_any_path_missing() {
    let mut h = TestHarness::new();
    let good = h.make_path(&["assets", "images", "logo.png"]);
    let bad = h.make_path(&["pages", "nonexistent.txt"]);
    let importer = h.importer();

    let err = h
        .resolver
        .resolve_compile_time_paths(&[good, bad], &importer, &mut h.string_table)
        .expect_err("should fail when any path is missing");

    assert!(matches!(
        compile_time_path_diagnostic_payload(&err),
        DiagnosticPayload::InvalidCompileTimePath {
            reason: InvalidCompileTimePathReason::MissingTarget,
            ..
        }
    ));
}

#[test]
fn empty_path_resolves_as_entry_root_public_directory() {
    let mut h = TestHarness::new();
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

    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
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

    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
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

#[cfg(windows)]
#[test]
fn canonicalized_source_library_file_resolves_to_library_prefixed_logical_path() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");
    let library_root = project_root.join("lib/html");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(&library_root).unwrap();
    fs::write(library_root.join("#mod.bst"), b"").unwrap();
    fs::write(library_root.join("helpers.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let mut source_libraries = crate::libraries::SourceLibraryRegistry::new();
    source_libraries.register_filesystem_root("html", library_root.clone());

    let resolver = ProjectPathResolver::new(project_root, entry_root, &source_libraries)
        .expect("resolver creation should succeed");

    let canonical_root = fs::canonicalize(&library_root).expect("should canonicalize library root");
    assert_eq!(
        resolver.source_library_roots().get("html"),
        Some(&canonical_root)
    );

    let canonical_file = fs::canonicalize(library_root.join("helpers.bst"))
        .expect("should canonicalize source library file");
    let mut string_table = StringTable::new();
    let logical_path = resolver
        .logical_path_for_canonical_file(&canonical_file, &mut string_table)
        .expect("canonical source library file should resolve");

    assert_eq!(logical_path, PathBuf::from("html").join("helpers.bst"));
}

// -----------------------------------------------------------------------
// Scan-root vs import-prefix behavior
// -----------------------------------------------------------------------

#[test]
fn library_scan_root_name_is_not_import_prefix() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");
    let library_root = project_root.join("lib/helper");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(&library_root).unwrap();
    fs::write(library_root.join("utils.bst"), b"").unwrap();
    fs::create_dir_all(entry_root.join("lib")).unwrap();
    fs::write(entry_root.join("lib/thing.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let mut source_libraries = crate::libraries::SourceLibraryRegistry::new();
    source_libraries.register_filesystem_root("helper", library_root.clone());

    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("lib", &mut string_table);
    path.push_str("thing", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect("entry-root fallback import should resolve");

    assert_eq!(
        result.0.base,
        CompileTimePathBase::EntryRoot,
        "scan root name 'lib' must not be treated as an import prefix"
    );
}

#[test]
fn library_direct_child_is_import_prefix() {
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

    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("utils", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect("source library import should resolve");

    assert_eq!(
        result.0.base,
        CompileTimePathBase::SourceLibraryRoot,
        "direct child of scan root must be a valid import prefix"
    );
}

#[test]
fn entry_root_import_fallback_success() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(entry_root.join("pages")).unwrap();
    fs::write(entry_root.join("pages/about.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("pages", &mut string_table);
    path.push_str("about", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect("entry-root fallback import should resolve");

    assert_eq!(
        result.0.base,
        CompileTimePathBase::EntryRoot,
        "non-relative imports without a library prefix must fall back to entry root"
    );
}

#[test]
fn source_library_prefix_wins_consistently() {
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

    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("utils", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect("source library import should resolve");

    assert_eq!(
        result.0.base,
        CompileTimePathBase::SourceLibraryRoot,
        "source library prefix must consistently win over entry-root collision"
    );
    assert_eq!(
        result.1,
        fs::canonicalize(library_root.join("utils.bst")).unwrap()
    );
}

// -----------------------------------------------------------------------
// Phase 4 — Import path restriction and canonicalization hardening
// -----------------------------------------------------------------------

#[test]
fn import_dotdot_rejected() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("..", &mut string_table);
    path.push_str("shared", &mut string_table);
    path.push_str("math", &mut string_table);

    let importer = entry_root.join("index.bst");
    let err = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect_err("'..' in imports should be rejected");
    let rendered_msg = rendered_error_msg(&err, &string_table);

    assert!(
        rendered_msg.contains("'..' are not supported"),
        "expected '..' rejection, got: {}",
        rendered_msg
    );
}

#[test]
fn missing_import_target_is_typed_diagnostic() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("missing", &mut string_table);
    path.push_str("target", &mut string_table);

    let importer = entry_root.join("index.bst");
    let err = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect_err("missing import should be rejected");
    let diagnostic = typed_import_diagnostic(&err);

    assert_eq!(
        diagnostic.kind,
        crate::compiler_frontend::compiler_messages::DiagnosticKind::Import(
            ImportDiagnosticKind::MissingImportTarget
        )
    );
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::MissingImportTarget { .. }
    ));
}

#[test]
fn import_escape_project_root_rejected() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str(".", &mut string_table);
    path.push_str("..", &mut string_table);
    path.push_str("..", &mut string_table);
    path.push_str("escape", &mut string_table);

    let importer = entry_root.join("index.bst");
    let err = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect_err("import escaping project root should be rejected");
    assert!(matches!(
        import_diagnostic_payload(&err),
        DiagnosticPayload::InvalidImportPath {
            reason: InvalidImportPathReason::ParentDirectorySegment,
            ..
        }
    ));
    let rendered_msg = rendered_error_msg(&err, &string_table);

    assert!(
        rendered_msg.contains("'..' are not supported"),
        "expected '..' rejection, got: {}",
        rendered_msg
    );
}

#[test]
fn import_escape_library_root_rejected() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");
    let library_root = project_root.join("lib/helper");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(&library_root).unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let mut source_libraries = crate::libraries::SourceLibraryRegistry::new();
    source_libraries.register_filesystem_root("helper", library_root.clone());

    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("..", &mut string_table);
    path.push_str("escape", &mut string_table);

    let importer = entry_root.join("index.bst");
    let err = resolver
        .resolve_import_as_compile_time_path(&path, &importer, &mut string_table)
        .expect_err("import escaping library root should be rejected");
    assert!(matches!(
        import_diagnostic_payload(&err),
        DiagnosticPayload::InvalidImportPath {
            reason: InvalidImportPathReason::ParentDirectorySegment,
            ..
        }
    ));
    let rendered_msg = rendered_error_msg(&err, &string_table);

    assert!(
        rendered_msg.contains("'..' are not supported"),
        "expected '..' rejection, got: {}",
        rendered_msg
    );
}

#[test]
fn module_root_facade_fallback_resolves_plain_folder_import() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(entry_root.join("helper")).unwrap();
    fs::write(entry_root.join("helper/#mod.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result =
        resolver.resolve_import_to_file_with_facade_fallback(&path, &importer, &mut string_table);

    assert!(
        result.is_ok(),
        "plain folder import should resolve to module root facade"
    );
    assert_eq!(
        result.unwrap(),
        fs::canonicalize(entry_root.join("helper/#mod.bst")).unwrap()
    );
}

#[test]
fn plain_folder_import_to_module_root_without_facade_is_rejected() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(entry_root.join("helper")).unwrap();
    fs::write(entry_root.join("helper/#page.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);

    let importer = entry_root.join("index.bst");
    let err = resolver
        .resolve_import_to_file_with_facade_fallback(&path, &importer, &mut string_table)
        .expect_err("plain folder import to module root without facade should fail");

    let diagnostic = typed_import_diagnostic(&err);
    assert_eq!(
        diagnostic.kind,
        crate::compiler_frontend::compiler_messages::DiagnosticKind::Import(
            ImportDiagnosticKind::MissingModuleFacade
        )
    );

    let rendered_msg = rendered_error_msg(&err, &string_table);
    assert!(
        rendered_msg.contains("#mod.bst facade"),
        "expected missing facade suggestion, got: {rendered_msg}"
    );
}

#[test]
fn concrete_file_import_inside_module_root_is_accepted() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(entry_root.join("helper")).unwrap();
    fs::write(entry_root.join("helper/#page.bst"), b"").unwrap();
    fs::write(entry_root.join("helper/thing.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("helper", &mut string_table);
    path.push_str("thing", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result =
        resolver.resolve_import_to_file_with_facade_fallback(&path, &importer, &mut string_table);

    assert!(
        result.is_ok(),
        "concrete file import inside a module root should resolve at Stage 0"
    );
}

#[test]
fn import_case_sensitive_symbol_mismatch_rejected() {
    let temp_dir = tempfile::tempdir().expect("failed to create temp dir");
    let project_root = temp_dir.path().to_path_buf();
    let entry_root = project_root.join("src");

    fs::create_dir_all(&entry_root).unwrap();
    fs::create_dir_all(entry_root.join("pages")).unwrap();
    fs::write(entry_root.join("pages/about.bst"), b"").unwrap();
    fs::write(entry_root.join("index.bst"), b"").unwrap();

    let source_libraries = crate::libraries::SourceLibraryRegistry::new();
    let resolver =
        ProjectPathResolver::new(project_root.clone(), entry_root.clone(), &source_libraries)
            .expect("resolver creation should succeed");

    let mut string_table = StringTable::new();
    let mut path = InternedPath::new();
    path.push_str("pages", &mut string_table);
    path.push_str("About", &mut string_table);

    let importer = entry_root.join("index.bst");
    let result = resolver.resolve_import_as_compile_time_path(&path, &importer, &mut string_table);

    #[cfg(target_os = "macos")]
    {
        let err = result.expect_err("case mismatch should be rejected on macOS");
        assert!(matches!(
            import_diagnostic_payload(&err),
            DiagnosticPayload::InvalidImportPath {
                reason: InvalidImportPathReason::CaseMismatch { .. },
                ..
            }
        ));
        let rendered_msg = rendered_error_msg(&err, &string_table);
        assert!(
            rendered_msg.contains("case mismatch"),
            "expected case mismatch error, got: {}",
            rendered_msg
        );
    }

    #[cfg(not(target_os = "macos"))]
    {
        // On case-sensitive filesystems the file simply won't be found.
        assert!(
            result.is_err(),
            "case mismatch should fail on case-sensitive filesystems"
        );
    }
}
