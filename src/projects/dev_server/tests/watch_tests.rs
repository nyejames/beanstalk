//! Tests for filesystem fingerprinting and debounce helpers.

use super::{
    FileFingerprint, collect_fingerprints, detect_changes, should_ignore_path,
    should_trigger_debounced_build, should_trigger_debounced_build_at,
};
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("system time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_{prefix}_{unique}"))
}

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
fn debounce_trigger_only_after_window() {
    let now = Instant::now();
    let just_dirty = now
        .checked_sub(Duration::from_millis(50))
        .expect("checked_sub should produce a valid instant");
    let old_dirty = now
        .checked_sub(Duration::from_secs(2))
        .expect("checked_sub should produce a valid instant");

    assert!(!should_trigger_debounced_build_at(
        Some(just_dirty),
        now,
        Duration::from_millis(100)
    ));
    assert!(should_trigger_debounced_build_at(
        Some(old_dirty),
        now,
        Duration::from_millis(100)
    ));
    assert!(!should_trigger_debounced_build_at(
        None,
        now,
        Duration::from_millis(100)
    ));

    // Keep one smoke check on the public wrapper that uses Instant::now internally.
    assert!(!should_trigger_debounced_build(
        Some(Instant::now()),
        Duration::from_secs(10)
    ));
}

#[test]
fn scanner_ignores_dev_output_directory() {
    let root = temp_dir("watch_scan");
    let output_dir = root.join("dev");
    let src_dir = root.join("src");
    fs::create_dir_all(&output_dir).expect("should create output dir");
    fs::create_dir_all(&src_dir).expect("should create source dir");

    fs::write(src_dir.join("main.bst"), "main").expect("should write source file");
    fs::write(output_dir.join("bundle.js"), "js").expect("should write output file");

    let fingerprints = collect_fingerprints(&root, &output_dir).expect("scanner should complete");
    assert!(
        fingerprints
            .keys()
            .all(|path| !path.starts_with(&output_dir)),
        "scanner should ignore output directory files"
    );
    assert!(
        fingerprints.keys().any(|path| path.ends_with("main.bst")),
        "scanner should include source files"
    );

    fs::remove_dir_all(&root).expect("should remove temp test dir");
}

#[test]
fn ignore_rules_cover_git_and_output_dir() {
    let root = PathBuf::from("/tmp/project");
    let output_dir = root.join("dev");
    assert!(should_ignore_path(&root.join(".git/index"), &output_dir));
    assert!(should_ignore_path(&root.join("dev/main.js"), &output_dir));
    assert!(!should_ignore_path(&root.join("src/main.bst"), &output_dir));
}
