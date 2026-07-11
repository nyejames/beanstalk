//! Tests for the frontend-only `check` command flow.

use super::{execute_check, format_terse_summary_line};
use crate::compiler_tests::test_support::temp_dir;
use std::fs;
use std::time::Duration;

#[test]
fn check_compiles_single_file_without_writing_artifacts() {
    let root = temp_dir("single_file");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("main.bst");
    fs::write(&entry_file, "value = 1\n").expect("should write source file");

    let outcome = execute_check(
        entry_file
            .to_str()
            .expect("temp file path should be valid UTF-8 for this test"),
    );
    assert!(
        !outcome.messages.has_errors(),
        "single-file check should compile without errors"
    );
    assert_eq!(
        fs::read_dir(&root).expect("should read temp root").count(),
        1,
        "check should not write output artifacts to the source folder"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn check_compiles_directory_project_without_writing_artifacts() {
    let root = temp_dir("directory_project");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("#page.bst");
    fs::write(&entry_file, "value = 1\n").expect("should write source file");

    let outcome = execute_check(
        root.to_str()
            .expect("temp directory path should be valid UTF-8 for this test"),
    );
    assert!(
        !outcome.messages.has_errors(),
        "directory check should compile without errors"
    );
    assert!(
        !root.join("dev").exists(),
        "check should not create dev output artifacts"
    );
    assert!(
        !root.join("release").exists(),
        "check should not create release output artifacts"
    );
    assert!(
        !root.join("index.html").exists(),
        "check should not emit backend output artifacts"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn terse_summary_line_matches_clean_success_contract() {
    let summary = format_terse_summary_line(Duration::from_millis(5), 0, 0);
    assert_eq!(summary, "Done in 5ms. No errors or warnings.");
}

#[test]
fn check_collects_frontend_warnings_on_success() {
    let root = temp_dir("check_warnings");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("main.bst");
    // A capture pattern before an else arm makes the else arm unreachable.
    fs::write(
        &entry_file,
        "value ~= \"hello\"\n\
         if value is:\n\
             captured => result = captured\n\
             else => result = \"fallback\"\n\
         ;\n",
    )
    .expect("should write source file");

    let outcome = execute_check(
        entry_file
            .to_str()
            .expect("temp file path should be valid UTF-8 for this test"),
    );

    assert!(
        !outcome.messages.has_errors(),
        "check should succeed even when warnings are present"
    );
    assert!(
        outcome.messages.warning_count() > 0,
        "an unreachable match arm should produce at least one warning"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn check_reports_empty_messages_for_warning_free_success() {
    let root = temp_dir("check_clean");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("main.bst");
    // Use the variable in an io.line call so the frontend has no unused bindings or unreachable arms.
    fs::write(
        &entry_file,
        "value = 1\n\
         io.line([: [value]])\n",
    )
    .expect("should write source file");

    let outcome = execute_check(
        entry_file
            .to_str()
            .expect("temp file path should be valid UTF-8 for this test"),
    );

    assert!(
        !outcome.messages.has_errors(),
        "clean check should not produce errors"
    );
    assert_eq!(
        outcome.messages.warning_count(),
        0,
        "clean check should not produce warnings"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
