use crate::projects::html_project::new_html_project::prompt_tests::ScriptedPrompt;
use crate::projects::html_project::new_html_project::scaffold::{
    SCAFFOLD_DIRECTORIES, find_scaffold_conflicts, run_preflight_checks, write_scaffold,
};
use crate::projects::html_project::new_html_project::target::ResolvedProjectTarget;
use crate::projects::html_project::new_html_project::{
    NewHtmlProjectOptions, create_html_project_template_with_prompt,
};
use std::fs;
use std::path::PathBuf;

fn empty_target(path: PathBuf) -> ResolvedProjectTarget {
    ResolvedProjectTarget {
        project_dir: path,
        project_name: String::from("test"),
        missing_directories: Vec::new(),
        target_existed: false,
        target_was_non_empty: false,
    }
}

#[test]
fn existing_scaffold_directories_are_not_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();

    for dir in SCAFFOLD_DIRECTORIES {
        fs::create_dir_all(project_dir.join(dir)).unwrap();
    }

    let conflicts = find_scaffold_conflicts(&project_dir);
    assert!(conflicts.is_empty());
}

#[test]
fn existing_config_file_is_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("#config.bst"), b"old").unwrap();

    let conflicts = find_scaffold_conflicts(&project_dir);
    assert_eq!(conflicts, vec!["#config.bst"]);
}

#[test]
fn existing_page_file_is_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::create_dir(project_dir.join("src")).unwrap();
    fs::write(project_dir.join("src/#page.bst"), b"old").unwrap();

    let conflicts = find_scaffold_conflicts(&project_dir);
    assert_eq!(conflicts, vec!["src/#page.bst"]);
}

#[test]
fn existing_manifests_are_conflicts() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::create_dir(project_dir.join("dev")).unwrap();
    fs::create_dir(project_dir.join("release")).unwrap();
    fs::write(project_dir.join("dev/.beanstalk_manifest"), b"old").unwrap();
    fs::write(project_dir.join("release/.beanstalk_manifest"), b"old").unwrap();

    let conflicts = find_scaffold_conflicts(&project_dir);
    assert!(conflicts.contains(&"dev/.beanstalk_manifest"));
    assert!(conflicts.contains(&"release/.beanstalk_manifest"));
}

#[test]
fn gitignore_is_not_a_conflict() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join(".gitignore"), b"old").unwrap();

    let conflicts = find_scaffold_conflicts(&project_dir);
    assert!(conflicts.is_empty());
}

#[test]
fn preflight_fails_without_force_when_conflicts_exist() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("#config.bst"), b"old").unwrap();

    let target = empty_target(project_dir.clone());
    let mut prompt = ScriptedPrompt::new(Vec::new());

    let error = run_preflight_checks(&target, false, &mut prompt).unwrap_err();

    assert!(error.contains("Cannot create project"));
    assert!(error.contains("#config.bst"));
    assert!(error.contains("--force"));
}

#[test]
fn preflight_succeeds_when_no_conflicts_exist() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();

    let target = empty_target(project_dir);
    let mut prompt = ScriptedPrompt::new(Vec::new());

    assert!(run_preflight_checks(&target, false, &mut prompt).is_ok());
}

#[test]
fn preflight_with_force_asks_second_confirmation() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("#config.bst"), b"old").unwrap();

    let target = empty_target(project_dir);
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    assert!(run_preflight_checks(&target, true, &mut prompt).is_ok());
    assert!(prompt.messages[0].contains("WARNING: --force"));
    assert!(prompt.messages[0].contains("#config.bst"));
}

#[test]
fn preflight_with_force_cancels_on_decline() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("#config.bst"), b"old").unwrap();

    let target = empty_target(project_dir);
    let mut prompt = ScriptedPrompt::new(vec![String::from("n")]);

    let error = run_preflight_checks(&target, true, &mut prompt).unwrap_err();
    assert_eq!(error, "Cancelled project creation.");
}

#[test]
fn preflight_warns_about_non_empty_directory() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("other.txt"), b"content").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("test"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    assert!(run_preflight_checks(&target, false, &mut prompt).is_ok());
    assert!(prompt.messages[0].contains("not empty"));
}

#[test]
fn preflight_non_empty_cancelled_performs_no_writes() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("other.txt"), b"content").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("test"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("n")]);

    let error = run_preflight_checks(&target, false, &mut prompt).unwrap_err();
    assert_eq!(error, "Cancelled project creation.");
}

#[test]
fn end_to_end_conflict_without_force_performs_no_writes() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("#config.bst"), b"original").unwrap();

    let options = NewHtmlProjectOptions {
        raw_path: Some(project_dir.to_string_lossy().to_string()),
        force: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("1"), String::from("")]);

    let error = create_html_project_template_with_prompt(options, &mut prompt).unwrap_err();
    assert!(error.contains("Cannot create project"));

    let content = fs::read_to_string(project_dir.join("#config.bst")).unwrap();
    assert_eq!(content, "original");
}

#[test]
fn end_to_end_force_overwrites_scaffold_owned_files_only() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().to_path_buf();
    fs::write(project_dir.join("#config.bst"), b"original").unwrap();
    fs::write(project_dir.join("user-file.txt"), b"keep me").unwrap();

    let options = NewHtmlProjectOptions {
        raw_path: Some(project_dir.to_string_lossy().to_string()),
        force: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![
        String::from("1"),
        String::from(""),
        String::from("y"),
        String::from("n"),
    ]);

    let result = create_html_project_template_with_prompt(options, &mut prompt);
    assert!(result.is_ok(), "expected success, got: {result:?}");

    let config_content = fs::read_to_string(project_dir.join("#config.bst")).unwrap();
    assert!(config_content.contains("# name = "));

    let user_content = fs::read_to_string(project_dir.join("user-file.txt")).unwrap();
    assert_eq!(user_content, "keep me");
}

// Phase 5 scaffold write tests

#[test]
fn creates_full_default_scaffold_in_empty_temp_dir() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("My Project"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    let report = write_scaffold(&target, false, &mut prompt).unwrap();

    assert!(project_dir.join("#config.bst").exists());
    assert!(project_dir.join("src/#page.bst").exists());
    assert!(project_dir.join("lib").exists());
    assert!(project_dir.join("dev/.beanstalk_manifest").exists());
    assert!(project_dir.join("release/.beanstalk_manifest").exists());
    assert!(project_dir.join(".gitignore").exists());

    assert!(report.created.contains(&PathBuf::from("#config.bst")));
    assert!(report.created.contains(&PathBuf::from("src/#page.bst")));
    assert!(report.created.contains(&PathBuf::from("lib")));
    assert!(
        report
            .created
            .contains(&PathBuf::from("dev/.beanstalk_manifest"))
    );
    assert!(
        report
            .created
            .contains(&PathBuf::from("release/.beanstalk_manifest"))
    );
    assert!(report.created.contains(&PathBuf::from(".gitignore")));
}

#[test]
fn generated_config_exactly_matches_expected_content() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join("#config.bst")).unwrap();
    assert_eq!(
        content,
        super::start_page_scaffolding::config_template("Test Site")
    );
}

#[test]
fn generated_page_exactly_matches_expected_content() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join("src/#page.bst")).unwrap();
    assert_eq!(content, super::start_page_scaffolding::page_template());
}

#[test]
fn manifests_are_generated_under_dev_and_release() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    write_scaffold(&target, false, &mut prompt).unwrap();

    let dev_manifest = fs::read_to_string(project_dir.join("dev/.beanstalk_manifest")).unwrap();
    let release_manifest =
        fs::read_to_string(project_dir.join("release/.beanstalk_manifest")).unwrap();
    assert_eq!(
        dev_manifest,
        super::start_page_scaffolding::manifest_template()
    );
    assert_eq!(
        release_manifest,
        super::start_page_scaffolding::manifest_template()
    );
}

#[test]
fn lib_is_created_and_empty() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    write_scaffold(&target, false, &mut prompt).unwrap();

    let lib = project_dir.join("lib");
    assert!(lib.is_dir());
    let mut entries = fs::read_dir(&lib).unwrap();
    assert!(entries.next().is_none());
}

#[test]
fn gitignore_is_created_by_default() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    let report = write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert_eq!(content, super::start_page_scaffolding::gitignore_template());
    assert!(report.created.contains(&PathBuf::from(".gitignore")));
}

#[test]
fn existing_gitignore_gets_append_block_when_confirmed() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join(".gitignore"), "node_modules/\n").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    let report = write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert!(content.contains("node_modules/"));
    assert!(content.contains("# Beanstalk"));
    assert!(content.contains("/dev"));
    assert!(report.updated.contains(&PathBuf::from(".gitignore")));
}

#[test]
fn existing_gitignore_is_unchanged_when_declined() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join(".gitignore"), "node_modules/\n").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("n")]);

    let report = write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert_eq!(content, "node_modules/\n");
    assert!(report.skipped.contains(&PathBuf::from(".gitignore")));
}

#[test]
fn existing_gitignore_with_dev_is_not_duplicated() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join(".gitignore"), "/dev\n").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(Vec::new());

    let report = write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert_eq!(content, "/dev\n");
    assert!(report.skipped.contains(&PathBuf::from(".gitignore")));
}

#[test]
fn project_name_is_escaped_in_config() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from(r#"Say "hello"\back"#),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    write_scaffold(&target, false, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join("#config.bst")).unwrap();
    assert!(content.contains(r#"# name = "Say \"hello\"\\back""#));
}

#[test]
fn force_replaces_scaffold_owned_files_only() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    fs::create_dir(&project_dir).unwrap();
    fs::create_dir(project_dir.join("src")).unwrap();
    fs::create_dir(project_dir.join("dev")).unwrap();
    fs::create_dir(project_dir.join("release")).unwrap();
    fs::write(project_dir.join("#config.bst"), b"old config").unwrap();
    fs::write(project_dir.join("src/#page.bst"), b"old page").unwrap();
    fs::write(project_dir.join("dev/.beanstalk_manifest"), b"old manifest").unwrap();
    fs::write(
        project_dir.join("release/.beanstalk_manifest"),
        b"old manifest",
    )
    .unwrap();
    fs::write(project_dir.join("user-file.txt"), b"keep me").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y"), String::from("y")]);

    let report = write_scaffold(&target, true, &mut prompt).unwrap();

    assert!(report.replaced.contains(&PathBuf::from("#config.bst")));
    assert!(report.replaced.contains(&PathBuf::from("src/#page.bst")));
    assert!(
        report
            .replaced
            .contains(&PathBuf::from("dev/.beanstalk_manifest"))
    );
    assert!(
        report
            .replaced
            .contains(&PathBuf::from("release/.beanstalk_manifest"))
    );

    let user_content = fs::read_to_string(project_dir.join("user-file.txt")).unwrap();
    assert_eq!(user_content, "keep me");
}

#[test]
fn gitignore_never_overwritten_even_with_force() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    fs::create_dir(&project_dir).unwrap();
    fs::write(project_dir.join(".gitignore"), b"custom\n").unwrap();

    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: Vec::new(),
        target_existed: true,
        target_was_non_empty: true,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("n")]);

    let report = write_scaffold(&target, true, &mut prompt).unwrap();

    let content = fs::read_to_string(project_dir.join(".gitignore")).unwrap();
    assert_eq!(content, "custom\n");
    assert!(report.skipped.contains(&PathBuf::from(".gitignore")));
}

#[test]
fn write_failure_returns_precise_error() {
    let temp = tempfile::tempdir().unwrap();
    let project_dir = temp.path().join("project");
    let target = ResolvedProjectTarget {
        project_dir: project_dir.clone(),
        project_name: String::from("Test Site"),
        missing_directories: vec![project_dir.clone()],
        target_existed: false,
        target_was_non_empty: false,
    };
    let mut prompt = ScriptedPrompt::new(vec![String::from("y")]);

    // Create a file where the project directory should be to force create_dir_all to fail.
    fs::write(&project_dir, b"").unwrap();

    let error = write_scaffold(&target, false, &mut prompt).unwrap_err();
    assert!(error.contains("Project creation failed"));
}
