//! Tests for dev-server watch scope derivation and change detection helpers.

use super::{
    FileFingerprint, WatchScope, WatchSession, WatchTarget, collect_fingerprints, detect_changes,
    should_ignore_path,
};
use crate::compiler_tests::test_support::temp_dir;
use crate::projects::settings::Config;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

#[test]
fn detects_added_and_removed_files() {
    let mut previous = HashMap::new();
    let mut current = HashMap::new();
    let path_a = PathBuf::from("a.bst");
    let path_b = PathBuf::from("b.bst");

    previous.insert(
        path_a.clone(),
        FileFingerprint {
            modified: SystemTime::UNIX_EPOCH,
            len: 10,
        },
    );
    current.insert(
        path_a,
        FileFingerprint {
            modified: SystemTime::UNIX_EPOCH,
            len: 10,
        },
    );
    current.insert(
        path_b,
        FileFingerprint {
            modified: SystemTime::UNIX_EPOCH,
            len: 1,
        },
    );

    assert!(detect_changes(&previous, &current));
}

#[test]
fn detects_modified_file_fingerprints() {
    let path = PathBuf::from("test.bst");
    let mut previous = HashMap::new();
    let mut current = HashMap::new();

    previous.insert(
        path.clone(),
        FileFingerprint {
            modified: SystemTime::UNIX_EPOCH,
            len: 5,
        },
    );
    current.insert(
        path,
        FileFingerprint {
            modified: SystemTime::UNIX_EPOCH + Duration::from_secs(1),
            len: 5,
        },
    );

    assert!(detect_changes(&previous, &current));
}

#[test]
fn directory_scope_watches_config_entry_root_and_root_folders() {
    let root = temp_dir("directory_scope");
    let output_dir = root.join("dev");
    fs::create_dir_all(root.join("src")).expect("should create src dir");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    let canonical_root = root.canonicalize().expect("root should canonicalize");

    let mut config = Config::new(root.clone());
    config.entry_root = PathBuf::from("src");
    config.root_folders = vec![PathBuf::from("assets")];

    let scope = WatchScope::derive(&root, Some(&config), &output_dir);

    assert!(scope.watches_path(&canonical_root.join("#config.bst")));
    assert!(scope.watches_path(&canonical_root.join("src/main.bst")));
    assert!(scope.watches_path(&canonical_root.join("assets/logo.png")));
    assert!(!scope.watches_path(&canonical_root.join("target/debug/app")));

    fs::remove_dir_all(&root).expect("should remove temp test dir");
}

#[test]
fn scanner_only_scans_declared_watch_targets() {
    let root = temp_dir("watch_scan");
    let output_dir = root.join("dev");
    let src_dir = root.join("src");
    let unrelated_dir = root.join("target");
    fs::create_dir_all(&output_dir).expect("should create output dir");
    fs::create_dir_all(&src_dir).expect("should create source dir");
    fs::create_dir_all(&unrelated_dir).expect("should create unrelated dir");

    fs::write(src_dir.join("main.bst"), "main").expect("should write source file");
    fs::write(output_dir.join("bundle.js"), "js").expect("should write output file");
    fs::write(unrelated_dir.join("debug.txt"), "ignore me").expect("should write unrelated file");

    let scope = WatchScope {
        output_dir: output_dir.clone(),
        targets: vec![WatchTarget {
            watch_path: src_dir.clone(),
            interest_path: None,
            recursive: true,
        }],
    };

    let fingerprints = collect_fingerprints(&scope).expect("scanner should complete");
    assert!(fingerprints.keys().all(|path| path.starts_with(&src_dir)));
    assert!(fingerprints.keys().any(|path| path.ends_with("main.bst")));
    assert!(
        fingerprints
            .keys()
            .all(|path| !path.starts_with(&output_dir)),
        "scanner should ignore output directory files"
    );

    fs::remove_dir_all(&root).expect("should remove temp test dir");
}

#[test]
fn manual_watch_session_coalesces_bursty_changes() {
    let scope = WatchScope {
        output_dir: PathBuf::from("dev"),
        targets: vec![WatchTarget {
            watch_path: PathBuf::from("src"),
            interest_path: None,
            recursive: true,
        }],
    };
    let (session, trigger) = WatchSession::manual(scope);

    trigger.notify_change();
    trigger.notify_change();
    trigger.notify_change();

    let seen_revision = session
        .wait_for_stable_change(0)
        .expect("manual watch session should settle");
    assert_eq!(seen_revision, 3);
}

#[test]
fn ignore_rules_cover_git_output_and_editor_temp_files() {
    let root = PathBuf::from("/tmp/project");
    let output_dir = root.join("dev");
    assert!(should_ignore_path(&root.join(".git/index"), &output_dir));
    assert!(should_ignore_path(&root.join("dev/main.js"), &output_dir));
    assert!(should_ignore_path(
        &root.join("src/main.bst.swp"),
        &output_dir
    ));
    assert!(should_ignore_path(
        &root.join("src/#main.bst#"),
        &output_dir
    ));
    assert!(!should_ignore_path(&root.join("src/main.bst"), &output_dir));
}
