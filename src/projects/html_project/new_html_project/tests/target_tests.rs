//! Tests for target path resolution and interactive project placement.

use crate::projects::html_project::new_html_project::prompt_tests::ScriptedPrompt;
use crate::projects::html_project::new_html_project::target::resolve_project_target;
use std::fs;
use std::path::PathBuf;

#[test]
fn omitted_path_uses_current_directory_after_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved = resolve_project_target(None, &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, current);
    assert!(resolved.target_existed);
    assert!(!resolved.target_was_non_empty);
    assert!(prompt.messages[0].contains("No project path specified"));
    assert!(prompt.messages[0].contains(&current.to_string_lossy().to_string()));
}

#[test]
fn omitted_path_cancels_when_declined() {
    let current = PathBuf::from("/tmp");
    let mut prompt = ScriptedPrompt::new(vec![String::from("n")]);

    let error = resolve_project_target(None, &current, &mut prompt).unwrap_err();

    assert_eq!(error, "Cancelled project creation.");
}

#[test]
fn dot_resolves_to_current_directory() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("")]);

    let resolved = resolve_project_target(Some(String::from(".")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, current);
    assert!(resolved.target_existed);
    assert_eq!(
        resolved.project_name,
        current.file_name().unwrap().to_str().unwrap()
    );
}

#[test]
fn relative_child_path_resolves_under_current_directory() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved =
        resolve_project_target(Some(String::from("site")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, current.join("site"));
    assert!(!resolved.target_existed);
    assert_eq!(resolved.missing_directories, vec![current.join("site")]);
}

#[test]
fn absolute_path_is_accepted() {
    let temp = tempfile::tempdir().unwrap();
    let target = temp.path().join("absolute-site");
    let current = PathBuf::from("/tmp");
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved = resolve_project_target(
        Some(target.to_string_lossy().to_string()),
        &current,
        &mut prompt,
    )
    .unwrap();

    assert_eq!(resolved.project_dir, target);
    assert!(!resolved.target_existed);
}

#[test]
fn tilde_expands_to_home_directory() {
    let original_home = std::env::var("HOME").ok();
    let temp = tempfile::tempdir().unwrap();
    unsafe { std::env::set_var("HOME", temp.path().to_string_lossy().to_string()) };

    let current = PathBuf::from("/tmp");
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved =
        resolve_project_target(Some(String::from("~/site")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, temp.path().join("site"));
    assert!(!resolved.target_existed);

    match original_home {
        Some(value) => unsafe { std::env::set_var("HOME", value) },
        None => unsafe { std::env::remove_var("HOME") },
    }
}

#[test]
fn existing_directory_option_1_uses_directory_directly() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("1"), String::from("")]);

    let resolved =
        resolve_project_target(Some(String::from("existing")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, existing);
    assert!(resolved.target_existed);
}

#[test]
fn existing_directory_option_2_creates_child_folder() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![
        String::from("2"),
        String::from("child"),
        String::from(""),
    ]);

    let resolved =
        resolve_project_target(Some(String::from("existing")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, existing.join("child"));
    assert!(!resolved.target_existed);
}

#[test]
fn option_3_cancels_for_existing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("existing");
    fs::create_dir(&existing).unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("3")]);

    let error =
        resolve_project_target(Some(String::from("existing")), &current, &mut prompt).unwrap_err();

    assert_eq!(error, "Cancelled project creation.");
}

#[test]
fn missing_directory_prompt_confirms() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved =
        resolve_project_target(Some(String::from("new/nested/site")), &current, &mut prompt)
            .unwrap();

    assert_eq!(resolved.project_dir, current.join("new/nested/site"));
    assert!(!resolved.target_existed);
    assert_eq!(
        resolved.missing_directories,
        vec![
            current.join("new"),
            current.join("new/nested"),
            current.join("new/nested/site")
        ]
    );
    assert!(prompt.messages[0].contains("directories that do not exist"));
}

#[test]
fn missing_directory_prompt_declines() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("n")]);

    let error =
        resolve_project_target(Some(String::from("new/site")), &current, &mut prompt).unwrap_err();

    assert_eq!(error, "Cancelled project creation.");
}

#[test]
fn skipped_project_name_uses_directory_basename() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved =
        resolve_project_target(Some(String::from("my-site")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_name, "my-site");
}

#[test]
fn explicit_project_name_overrides_basename() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("My Custom Site")]);

    let resolved =
        resolve_project_target(Some(String::from("my-site")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_name, "My Custom Site");
}

#[test]
fn project_name_trims_whitespace() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("  Padded Name  ")]);

    let resolved =
        resolve_project_target(Some(String::from("dir")), &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_name, "Padded Name");
}

#[test]
fn detects_non_empty_existing_directory() {
    let temp = tempfile::tempdir().unwrap();
    let existing = temp.path().join("non-empty");
    fs::create_dir(&existing).unwrap();
    fs::write(existing.join("file.txt"), b"content").unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("1"), String::from("")]);

    let resolved =
        resolve_project_target(Some(String::from("non-empty")), &current, &mut prompt).unwrap();

    assert!(resolved.target_existed);
    assert!(resolved.target_was_non_empty);
}
