//! Tests for dev-server orchestration and entry-path validation.

use super::{DevServerOptions, validate_dev_entry_path};
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_mod_{prefix}_{unique}"))
}

struct CurrentDirGuard {
    _lock: MutexGuard<'static, ()>,
    previous: PathBuf,
}

impl CurrentDirGuard {
    fn set_to(path: &PathBuf) -> Self {
        let lock = current_dir_test_lock()
            .lock()
            .expect("current-dir test lock should not be poisoned");
        let previous = std::env::current_dir().expect("current dir should resolve");
        std::env::set_current_dir(path).expect("should change current directory for test");
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

fn current_dir_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
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
fn entry_path_validation_accepts_directories() {
    let root = temp_dir("entry_dir");
    fs::create_dir_all(&root).expect("should create temp root");
    let validated = validate_dev_entry_path(
        root.to_str()
            .expect("temp path should be valid utf-8 for this test"),
    )
    .expect("directories should be accepted");

    assert_eq!(
        validated,
        root.canonicalize().expect("temp dir should canonicalize")
    );
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn empty_entry_path_uses_current_directory() {
    let root = temp_dir("current_dir");
    fs::create_dir_all(&root).expect("should create temp root");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let validated = validate_dev_entry_path("").expect("empty path should use current directory");

    assert_eq!(
        validated,
        root.canonicalize().expect("temp dir should canonicalize")
    );
    drop(_cwd_guard);
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}
