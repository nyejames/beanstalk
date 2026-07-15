//! Tests for target path resolution and interactive project placement.

use crate::projects::html_project::new_html_project::prompt_tests::ScriptedPrompt;
use crate::projects::html_project::new_html_project::target::{
    HomeEnv, expand_tilde, normalize_path, resolve_project_target,
};
use std::fs;
use std::path::PathBuf;

use std::collections::HashMap;

/// In-process mock environment for platform-independent home-resolution tests.
///
/// WHAT: Holds a fixed set of environment-variable values so tests can exercise
/// the `HOME`, `USERPROFILE` and `HOMEDRIVE`/`HOMEPATH` fallback chain without
/// mutating the process-global environment.
/// WHY: Direct `std::env::set_var` races under parallel test execution and
/// cannot simulate Windows variables on a Unix host.
struct MockHomeEnv {
    values: HashMap<String, String>,
    windows: bool,
}

impl MockHomeEnv {
    fn empty() -> Self {
        Self {
            values: HashMap::new(),
            windows: false,
        }
    }

    fn with(key: &str, value: &str) -> Self {
        let mut values = HashMap::new();
        values.insert(key.to_owned(), value.to_owned());
        Self {
            values,
            windows: false,
        }
    }

    fn and(mut self, key: &str, value: &str) -> Self {
        self.values.insert(key.to_owned(), value.to_owned());
        self
    }

    fn windows(mut self) -> Self {
        self.windows = true;
        self
    }
}

impl HomeEnv for MockHomeEnv {
    fn get(&self, key: &str) -> Option<String> {
        self.values.get(key).cloned()
    }

    fn is_windows(&self) -> bool {
        self.windows
    }
}

#[test]
fn normalize_path_collapses_dot_and_dotdot() {
    assert_eq!(
        normalize_path(&PathBuf::from("/a/b/../c")),
        PathBuf::from("/a/c")
    );
    assert_eq!(
        normalize_path(&PathBuf::from("./site")),
        PathBuf::from("site")
    );
    assert_eq!(
        normalize_path(&PathBuf::from("a/./b")),
        PathBuf::from("a/b")
    );
}

#[test]
fn normalize_path_does_not_escape_root() {
    assert_eq!(
        normalize_path(&PathBuf::from("/a/../../b")),
        PathBuf::from("/b")
    );
}

#[test]
fn expand_tilde_bare_and_slash_separated_expand_to_home() {
    let env = MockHomeEnv::with("HOME", "/mock/home");

    assert_eq!(
        expand_tilde("~", &env).unwrap(),
        PathBuf::from("/mock/home")
    );
    assert_eq!(
        expand_tilde("~/site", &env).unwrap(),
        PathBuf::from("/mock/home/site")
    );
    assert_eq!(
        expand_tilde("~/nested/deep", &env).unwrap(),
        PathBuf::from("/mock/home/nested/deep")
    );
}

#[test]
fn expand_tilde_windows_backslash_separated_expands() {
    let env = MockHomeEnv::with("HOME", "/mock/home");

    assert_eq!(
        expand_tilde("~\\site", &env).unwrap(),
        PathBuf::from("/mock/home/site")
    );
    assert_eq!(
        expand_tilde("~\\nested\\deep", &env).unwrap(),
        PathBuf::from("/mock/home/nested/deep")
    );
}

#[test]
fn expand_tilde_named_user_forms_are_unchanged() {
    let env = MockHomeEnv::with("HOME", "/mock/home");

    assert_eq!(
        expand_tilde("~other", &env).unwrap(),
        PathBuf::from("~other")
    );
    assert_eq!(
        expand_tilde("~other/site", &env).unwrap(),
        PathBuf::from("~other/site")
    );
    assert_eq!(
        expand_tilde("~other\\site", &env).unwrap(),
        PathBuf::from("~other\\site")
    );
}

#[test]
fn expand_tilde_passes_through_non_tilde_paths() {
    let env = MockHomeEnv::with("HOME", "/mock/home");

    assert_eq!(
        expand_tilde("/absolute/path", &env).unwrap(),
        PathBuf::from("/absolute/path")
    );
    assert_eq!(
        expand_tilde("relative/path", &env).unwrap(),
        PathBuf::from("relative/path")
    );
}

#[test]
fn expand_tilde_home_success_and_missing_home_failure() {
    let present = MockHomeEnv::with("HOME", "/mock/home");
    assert_eq!(
        expand_tilde("~/site", &present).unwrap(),
        PathBuf::from("/mock/home/site")
    );

    let absent = MockHomeEnv::empty();
    let error = expand_tilde("~/site", &absent).unwrap_err();
    assert_eq!(
        error,
        "Could not determine home directory for '~' expansion."
    );
}

#[test]
fn expand_tilde_userprofile_fallback_when_home_absent() {
    let env = MockHomeEnv::with("USERPROFILE", "C:\\Users\\test").windows();

    assert_eq!(
        expand_tilde("~/site", &env).unwrap(),
        PathBuf::from("C:\\Users\\test/site")
    );
}

#[test]
fn expand_tilde_homedrive_plus_homepath_fallback() {
    let env = MockHomeEnv::with("HOMEDRIVE", "C:")
        .and("HOMEPATH", "\\Users\\test")
        .windows();

    assert_eq!(
        expand_tilde("~/site", &env).unwrap(),
        PathBuf::from("C:\\Users\\test/site")
    );
}

#[test]
fn expand_tilde_incomplete_homedrive_pair_is_rejected() {
    let drive_only = MockHomeEnv::with("HOMEDRIVE", "C:").windows();
    let error = expand_tilde("~/site", &drive_only).unwrap_err();
    assert_eq!(
        error,
        "Could not determine home directory for '~' expansion."
    );

    let path_only = MockHomeEnv::with("HOMEPATH", "\\Users\\test").windows();
    let error = expand_tilde("~/site", &path_only).unwrap_err();
    assert_eq!(
        error,
        "Could not determine home directory for '~' expansion."
    );
}

#[test]
fn expand_tilde_empty_home_falls_through_to_userprofile() {
    let env = MockHomeEnv::with("HOME", "")
        .and("USERPROFILE", "C:\\Users\\test")
        .windows();

    assert_eq!(
        expand_tilde("~/site", &env).unwrap(),
        PathBuf::from("C:\\Users\\test/site")
    );
}

#[test]
fn expand_tilde_non_windows_does_not_use_windows_home_variables() {
    let env = MockHomeEnv::with("USERPROFILE", "C:\\Users\\test")
        .and("HOMEDRIVE", "C:")
        .and("HOMEPATH", "\\Users\\test");

    let error = expand_tilde("~/site", &env).unwrap_err();
    assert_eq!(
        error,
        "Could not determine home directory for '~' expansion."
    );
}

#[test]
fn omitted_path_uses_current_directory_after_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let current = temp.path().to_path_buf();
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("")]);

    let resolved = resolve_project_target(None, &current, &mut prompt).unwrap();

    assert_eq!(resolved.project_dir, current);
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

    assert!(resolved.target_was_non_empty);
}
