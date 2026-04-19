//! Tests for the core build orchestration and output writer APIs.
// NOTE: temp file creation processes have to be explicitly dropped
// Or these tests will fail on Windows due to attempts to delete non-empty temp directories while files are still open.

use super::*;
use std::fs;

#[test]
fn current_dir_guard_recovers_after_mutex_poisoning() {
    let root = temp_dir("poison_recovery");
    fs::create_dir_all(&root).expect("should create temp root");

    // Intentionally poison the cwd mutex by panicking while holding the guard.
    let panic_result = std::panic::catch_unwind(|| {
        let _guard = CurrentDirGuard::set_to(&root);
        panic!("deliberate panic to poison the cwd mutex");
    });
    assert!(
        panic_result.is_err(),
        "catch_unwind should capture the panic"
    );

    // A subsequent guard acquisition must succeed despite the poisoned mutex.
    let guard = CurrentDirGuard::set_to(&root);
    let current = fs::canonicalize(std::env::current_dir().expect("current dir should resolve"))
        .expect("current dir should canonicalize");
    let expected = fs::canonicalize(&root).expect("temp root should canonicalize");
    assert_eq!(current, expected);
    drop(guard);

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
