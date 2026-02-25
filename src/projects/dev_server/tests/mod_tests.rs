//! Tests for dev-server orchestration and entry-path validation.

use super::{DevServerOptions, validate_dev_entry_path};
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_mod_{prefix}_{unique}"))
}

#[test]
fn defaults_match_dev_server_contract() {
    let defaults = DevServerOptions::default();
    assert_eq!(defaults.host, "127.0.0.1");
    assert_eq!(defaults.port, 6342);
    assert_eq!(defaults.poll_interval_ms, 300);
}

#[test]
fn entry_path_validation_accepts_bst_files() {
    let root = temp_dir("entry_file");
    fs::create_dir_all(&root).expect("should create temp root");
    let file = root.join("main.bst");
    fs::write(&file, "x = 1").expect("should write test file");

    let validated = validate_dev_entry_path(
        file.to_str()
            .expect("temp path should be valid utf-8 for this test"),
    )
    .expect("valid bst path should pass validation");

    assert!(validated.ends_with("main.bst"));
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn entry_path_validation_rejects_directories() {
    let root = temp_dir("entry_dir");
    fs::create_dir_all(&root).expect("should create temp root");
    let error = validate_dev_entry_path(
        root.to_str()
            .expect("temp path should be valid utf-8 for this test"),
    )
    .expect_err("directories should be rejected in phase 1");

    assert_eq!(error.errors.len(), 1);
    assert!(error.errors[0].msg.contains("deferred"));
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}
