//! Phase 1 Stage 0 filesystem-identity tests.
//!
//! WHAT: exercises the strict UTF-8 filesystem-identity contract added by the codebase
//!      integrity cleanup plan: non-UTF-8 module roots, source names, folder names,
//!      extensions, source-package prefixes and single-file entries must surface as File
//!      infrastructure errors, and source-package canonicalization must be mandatory.
//! WHY: these invariants are Stage 0 subsystem-local facts that integration output cannot
//!      inspect directly, so they own a focused test file beside the create-project-modules
//!      module rather than living in the oversized Stage 0 orchestration test file.

use super::*;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_tests::test_support::temp_dir;

#[cfg(target_os = "linux")]
mod non_utf8_filesystem_identity {
    use super::*;
    use crate::compiler_frontend::compiler_errors::ErrorType;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    fn assert_file_infrastructure_error(messages: &CompilerMessages) {
        let (error_type, message, _location) = messages
            .first_infrastructure_error_for_tests()
            .expect("expected an infrastructure file error");
        assert_eq!(
            *error_type,
            ErrorType::File,
            "non-UTF-8 filesystem name should be a File infrastructure error"
        );
        assert!(
            message.contains("Non-UTF-8"),
            "error message should mention non-UTF-8: {message}"
        );
    }

    #[test]
    fn source_tree_rejects_non_utf8_file_name() {
        let root = temp_dir("source_tree_non_utf8_file");
        let entry_root = root.join("src");
        fs::create_dir_all(&entry_root).expect("should create entry root");
        fs::write(entry_root.join("#home.bst"), "").expect("should write entry root");

        let bad_name = OsString::from_vec(vec![0xC3, 0x28]);
        let bad_file = entry_root.join(bad_name);
        fs::write(&bad_file, "x ~= 1\n").expect("should write non-UTF-8 named file");

        let mut config = Config::new(root.clone());
        config.entry_root = PathBuf::from("src");
        let canonical_root = fs::canonicalize(&root).expect("project root should canonicalize");
        let canonical_entry_root =
            fs::canonicalize(&entry_root).expect("entry root should canonicalize");
        let mut string_table = StringTable::new();

        let messages = super::source_tree_index::SourceTreeIndex::discover(
            canonical_entry_root,
            &canonical_root,
            &config,
            &crate::builder_surface::SourcePackageRegistry::default(),
            &mut string_table,
        )
        .expect_err("non-UTF-8 file name should be rejected");

        assert_file_infrastructure_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn source_tree_rejects_non_utf8_folder_name() {
        let root = temp_dir("source_tree_non_utf8_folder");
        let entry_root = root.join("src");
        fs::create_dir_all(&entry_root).expect("should create entry root");
        fs::write(entry_root.join("#home.bst"), "").expect("should write entry root");

        let bad_name = OsString::from_vec(vec![0xC3, 0x28]);
        let bad_folder = entry_root.join(bad_name);
        fs::create_dir_all(&bad_folder).expect("should create non-UTF-8 named folder");

        let mut config = Config::new(root.clone());
        config.entry_root = PathBuf::from("src");
        let canonical_root = fs::canonicalize(&root).expect("project root should canonicalize");
        let canonical_entry_root =
            fs::canonicalize(&entry_root).expect("entry root should canonicalize");
        let mut string_table = StringTable::new();

        let messages = super::source_tree_index::SourceTreeIndex::discover(
            canonical_entry_root,
            &canonical_root,
            &config,
            &crate::builder_surface::SourcePackageRegistry::default(),
            &mut string_table,
        )
        .expect_err("non-UTF-8 folder name should be rejected");

        assert_file_infrastructure_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn collision_detection_rejects_non_utf8_name() {
        let root = temp_dir("collision_non_utf8_name");
        let package_root = root.join("pkg");
        fs::create_dir_all(&package_root).expect("should create package root");

        let bad_name = OsString::from_vec(vec![0xC3, 0x28]);
        let bad_file = package_root.join(bad_name);
        fs::write(&bad_file, "x ~= 1\n").expect("should write non-UTF-8 named file");

        let mut source_packages = crate::builder_surface::SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let messages = super::collision_detection::validate_source_package_tree_collisions(
            &source_packages,
            &mut string_table,
        )
        .expect_err("non-UTF-8 name in collision check should be rejected");

        assert_file_infrastructure_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn project_local_package_prefix_rejects_non_utf8_name() {
        let root = temp_dir("package_prefix_non_utf8");
        let packages_folder = root.join("packages");
        fs::create_dir_all(&packages_folder).expect("should create packages folder");

        let bad_name = OsString::from_vec(vec![0xC3, 0x28]);
        let bad_package = packages_folder.join(bad_name);
        fs::create_dir_all(&bad_package).expect("should create non-UTF-8 named package directory");

        let mut config = Config::new(root.clone());
        config.package_folders = vec![PathBuf::from("packages")];
        config.has_explicit_package_folders = true;

        let mut string_table = StringTable::new();
        let messages = super::source_package_discovery::discover_project_local_source_packages(
            &config,
            &root,
            &mut string_table,
        )
        .expect_err("non-UTF-8 package prefix should be rejected");

        assert_file_infrastructure_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }
}

#[cfg(target_os = "linux")]
mod non_utf8_single_file_identity {
    use super::*;
    use crate::compiler_frontend::compiler_errors::ErrorType;
    use crate::compiler_frontend::style_directives::StyleDirectiveRegistry;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    fn assert_file_infrastructure_error(messages: &CompilerMessages) {
        let (error_type, message, _location) = messages
            .first_infrastructure_error_for_tests()
            .expect("expected an infrastructure file error");
        assert_eq!(
            *error_type,
            ErrorType::File,
            "non-UTF-8 single-file input should be a File infrastructure error"
        );
        assert!(
            message.contains("UTF-8"),
            "error message should mention UTF-8: {message}"
        );
    }

    #[test]
    fn single_file_rejects_non_utf8_extension() {
        let root = temp_dir("single_file_non_utf8_ext");
        let entry = root.join("main.");
        let bad_ext = OsString::from_vec(vec![0xC3, 0x28]);
        let entry_with_bad_ext = entry.with_extension(bad_ext);
        fs::write(&entry_with_bad_ext, "x ~= 1\n").expect("should write entry file");

        let config = Config::new(entry_with_bad_ext.clone());
        let mut builder_surface = crate::builder_surface::BuilderSurface::with_mandatory_core();
        let mut string_table = StringTable::new();

        let extension = entry_with_bad_ext
            .extension()
            .expect("entry should have an extension");
        let messages = super::compilation::compile_single_file_frontend(
            &config,
            crate::compiler_frontend::FrontendBuildProfile::Dev,
            &StyleDirectiveRegistry::default(),
            &mut builder_surface,
            extension,
            &mut string_table,
        );
        let Err(messages) = messages else {
            panic!("non-UTF-8 extension should be rejected");
        };

        assert_file_infrastructure_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn single_file_rejects_non_utf8_entry_name() {
        let root = temp_dir("single_file_non_utf8_name");
        let bad_name = OsString::from_vec(vec![0xC3, 0x28]);
        let bad_file = root.join(bad_name).with_extension("bst");
        fs::write(&bad_file, "x ~= 1\n").expect("should write entry file");

        let config = Config::new(bad_file.clone());
        let mut builder_surface = crate::builder_surface::BuilderSurface::with_mandatory_core();
        let mut string_table = StringTable::new();

        let extension = bad_file
            .extension()
            .expect("entry should have a .bst extension");
        let messages = super::compilation::compile_single_file_frontend(
            &config,
            crate::compiler_frontend::FrontendBuildProfile::Dev,
            &StyleDirectiveRegistry::default(),
            &mut builder_surface,
            extension,
            &mut string_table,
        );
        let Err(messages) = messages else {
            panic!("non-UTF-8 entry file name should be rejected");
        };

        assert_file_infrastructure_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }
}

mod prepare_source_package_roots_tests {
    use super::*;
    use crate::builder_surface::SourcePackageRegistry;
    use crate::compiler_frontend::compiler_errors::ErrorType;
    use crate::compiler_frontend::source_packages::root_file::HashRootFileDiscovery;

    fn assert_canonicalization_error(messages: &CompilerMessages) {
        let (error_type, message, _location) = messages
            .first_infrastructure_error_for_tests()
            .expect("expected an infrastructure file error");
        assert_eq!(
            *error_type,
            ErrorType::File,
            "canonicalization failure should be a File infrastructure error"
        );
        assert!(
            message.contains("canonicalize"),
            "error message should mention canonicalization: {message}"
        );
    }

    #[test]
    fn canonical_root_with_single_hash_file_succeeds() {
        let root = temp_dir("prepare_roots_canonical_success");
        let package_root = root.join("pkg");
        fs::create_dir_all(&package_root).expect("should create package root");
        fs::write(package_root.join("#home.bst"), "").expect("should write hash root");

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root.clone(),
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let prepared = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect("canonical root should prepare successfully");

        let roots = prepared.roots();
        assert_eq!(roots.len(), 1, "one root should be prepared");
        let canonical = roots.get("pkg").expect("pkg root should exist");
        assert_eq!(
            *canonical,
            fs::canonicalize(&package_root).expect("root should canonicalize"),
            "prepared root should be canonical"
        );

        let root_files = prepared.root_files();
        let discovery = root_files.get("pkg").expect("pkg discovery should exist");
        match discovery {
            HashRootFileDiscovery::Unique(file) => {
                assert_eq!(
                    *file,
                    fs::canonicalize(package_root.join("#home.bst"))
                        .expect("hash root file should canonicalize"),
                    "discovered root file should be canonical"
                );
            }
            other => panic!("expected Unique hash root, got {other:?}"),
        }

        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn canonicalization_failure_returns_file_error() {
        let root = temp_dir("prepare_roots_canonical_failure");
        fs::create_dir_all(&root).expect("should create temp root");
        let nonexistent = root.join("does_not_exist");

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            nonexistent,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let messages = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect_err("nonexistent root should fail canonicalization");

        assert_canonicalization_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn missing_hash_root_preserves_missing_outcome() {
        let root = temp_dir("prepare_roots_missing");
        let package_root = root.join("pkg");
        fs::create_dir_all(&package_root).expect("should create package root");

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let prepared = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect("root without hash file should still prepare");

        let discovery = prepared
            .root_files()
            .get("pkg")
            .expect("pkg discovery should exist");
        assert_eq!(
            *discovery,
            HashRootFileDiscovery::Missing,
            "root with no hash file should be Missing"
        );

        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn multiple_hash_roots_preserves_multiple_outcome() {
        let root = temp_dir("prepare_roots_multiple");
        let package_root = root.join("pkg");
        fs::create_dir_all(&package_root).expect("should create package root");
        fs::write(package_root.join("#home.bst"), "").expect("should write first hash root");
        fs::write(package_root.join("#page.bst"), "").expect("should write second hash root");

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let prepared = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect("root with multiple hash files should still prepare");

        let discovery = prepared
            .root_files()
            .get("pkg")
            .expect("pkg discovery should exist");
        assert!(
            matches!(discovery, HashRootFileDiscovery::Multiple(_)),
            "root with two hash files should be Multiple"
        );

        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[cfg(unix)]
    #[test]
    fn unreadable_hash_root_preserves_unreadable_outcome() {
        use std::os::unix::fs::PermissionsExt;

        let root = temp_dir("prepare_roots_unreadable");
        let package_root = root.join("pkg");
        fs::create_dir_all(&package_root).expect("should create package root");
        fs::write(package_root.join("#home.bst"), "").expect("should write hash root");

        // Remove read permission so discover_hash_root_file cannot read the directory.
        // Canonicalization still succeeds because it only traverses the parent.
        fs::set_permissions(&package_root, fs::Permissions::from_mode(0o000))
            .expect("should remove read permissions");

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root.clone(),
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let prepared = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect("canonicalization should succeed even without read permission");

        let discovery = prepared
            .root_files()
            .get("pkg")
            .expect("pkg discovery should exist");
        assert!(
            matches!(discovery, HashRootFileDiscovery::Unreadable(_)),
            "unreadable root should be Unreadable, got {discovery:?}"
        );

        // Restore permissions so cleanup can remove the directory.
        fs::set_permissions(&package_root, fs::Permissions::from_mode(0o755))
            .expect("should restore permissions");
        fs::remove_dir_all(&root).expect("should remove temp root");
    }
}

/// Linux preparation tests for non-UTF-8 direct-child hash-root candidates.
///
/// Directory, single-file and config compilation all delegate source-package preparation to
/// `prepare_source_package_roots`, so these tests cover the shared owner instead of duplicating the
/// same assertion at each orchestration boundary. macOS rejects the invalid-byte fixture before
/// discovery can inspect it.
#[cfg(target_os = "linux")]
mod non_utf8_hash_root_candidate_tests {
    use super::*;
    use crate::builder_surface::SourcePackageRegistry;
    use crate::compiler_frontend::compiler_errors::ErrorType;
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;
    use std::path::PathBuf;

    fn assert_non_utf8_file_error(messages: &CompilerMessages) {
        let (error_type, message, _location) = messages
            .first_infrastructure_error_for_tests()
            .expect("expected an infrastructure file error");
        assert_eq!(
            *error_type,
            ErrorType::File,
            "non-UTF-8 hash-root candidate should be a File infrastructure error"
        );
        assert!(
            message.contains("Non-UTF-8"),
            "error message should mention non-UTF-8: {message}"
        );
        assert!(
            message.contains("hash-root public-surface candidate"),
            "error message should name the hash-root context: {message}"
        );
    }

    fn package_with_non_utf8_child() -> (PathBuf, PathBuf) {
        let root = temp_dir("prepare_roots_non_utf8_candidate");
        let package_root = root.join("pkg");
        fs::create_dir_all(&package_root).expect("should create package root");

        let bad_name = OsString::from_vec(vec![0xC3, 0x28]);
        let bad_file = package_root.join(bad_name);
        fs::write(&bad_file, b"").expect("should write non-UTF-8 named file");

        (root, package_root)
    }

    #[test]
    fn invalid_candidate_without_valid_hash_root_returns_file_error() {
        let (root, package_root) = package_with_non_utf8_child();

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let messages = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect_err("non-UTF-8 candidate should fail preparation");

        assert_non_utf8_file_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }

    #[test]
    fn valid_hash_root_plus_invalid_candidate_still_returns_file_error() {
        let (root, package_root) = package_with_non_utf8_child();
        fs::write(package_root.join("#home.bst"), b"").expect("should write valid hash root");

        let mut source_packages = SourcePackageRegistry::new();
        source_packages.register_filesystem_root(
            "pkg",
            package_root,
            crate::builder_surface::PackageOrigin::ProjectLocal,
        );

        let mut string_table = StringTable::new();
        let messages = super::source_package_discovery::prepare_source_package_roots(
            &source_packages,
            &mut string_table,
        )
        .expect_err("valid root plus invalid candidate should still fail");

        assert_non_utf8_file_error(&messages);
        fs::remove_dir_all(&root).expect("should remove temp root");
    }
}
