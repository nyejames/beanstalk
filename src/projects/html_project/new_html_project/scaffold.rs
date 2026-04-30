//! File-writing scaffold logic.
//!
//! WHAT: Performs preflight conflict checks and actual directory/file creation.
//! WHY: Separates IO side effects from command parsing and user prompting.

use std::fs;
use std::path::{Path, PathBuf};

use crate::projects::html_project::new_html_project::{
    CreateProjectReport, prompt::Prompt, target::ResolvedProjectTarget, templates,
};

const CONFIG_FILE: &str = "#config.bst";
const PAGE_FILE: &str = "src/#page.bst";
const DEV_MANIFEST: &str = "dev/.beanstalk_manifest";
const RELEASE_MANIFEST: &str = "release/.beanstalk_manifest";
const GITIGNORE_FILE: &str = ".gitignore";

/// Relative paths of all scaffold-owned files.
const SCAFFOLD_OWNED_FILES: &[&str] = &[CONFIG_FILE, PAGE_FILE, DEV_MANIFEST, RELEASE_MANIFEST];

/// Relative paths of all scaffold-owned directories.
const SCAFFOLD_DIRECTORIES: &[&str] = &["src", "lib", "dev", "release"];

/// Run preflight checks before writing anything.
///
/// Returns Ok(()) when it is safe to proceed, or Err when the user cancels
/// or when unresolvable conflicts exist without `--force`.
pub(crate) fn run_preflight_checks(
    target: &ResolvedProjectTarget,
    force: bool,
    prompt: &mut impl Prompt,
) -> Result<(), String> {
    let conflicts = find_scaffold_conflicts(&target.project_dir);

    if !conflicts.is_empty() && !force {
        let file_list = conflicts
            .iter()
            .map(|path| format!("  {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        return Err(format!(
            "Cannot create project because scaffold-owned files already exist:\n\
             {file_list}\n\n\
             Run with --force to replace scaffold-owned files."
        ));
    }

    if !conflicts.is_empty() && force {
        let file_list = conflicts
            .iter()
            .map(|path| format!("  {path}"))
            .collect::<Vec<_>>()
            .join("\n");
        let message = format!(
            "WARNING: --force will overwrite existing Beanstalk scaffold files in:\n\
             {}\n\n\
             Files that may be replaced:\n\
             {file_list}\n\n\
             Continue? [y/N]: ",
            target.project_dir.display()
        );
        if !prompt.confirm(&message, false)? {
            return Err("Cancelled project creation.".to_string());
        }
    }

    if target.target_was_non_empty && conflicts.is_empty() {
        let message = format!(
            "WARNING: The target directory is not empty:\n\
             {}\n\n\
             Creating a project here may mix scaffold files with existing content.\n\n\
             Continue? [y/N]: ",
            target.project_dir.display()
        );
        if !prompt.confirm(&message, false)? {
            return Err("Cancelled project creation.".to_string());
        }
    }

    Ok(())
}

fn find_scaffold_conflicts(project_dir: &Path) -> Vec<&'static str> {
    SCAFFOLD_OWNED_FILES
        .iter()
        .copied()
        .filter(|relative| project_dir.join(relative).exists())
        .collect()
}

/// Write the complete scaffold to disk.
///
/// WHAT: creates directories, writes scaffold-owned files, and handles `.gitignore`
/// creation or append after confirming with the user.
/// WHY: this is the single IO entry point for the scaffold; all file writes are
/// ordered and tracked so the caller can report exactly what happened.
pub(crate) fn write_scaffold(
    target: &ResolvedProjectTarget,
    force: bool,
    prompt: &mut impl Prompt,
) -> Result<CreateProjectReport, String> {
    let project_dir = &target.project_dir;

    // 1. Create missing directories (including the project dir itself).
    if !project_dir.exists() {
        fs::create_dir_all(project_dir).map_err(|e| {
            format!(
                "Project creation failed while creating directory {}: {e}.",
                project_dir.display()
            )
        })?;
    }

    let mut created: Vec<PathBuf> = Vec::new();
    let mut replaced: Vec<PathBuf> = Vec::new();

    for dir in SCAFFOLD_DIRECTORIES {
        let dir_path = project_dir.join(dir);
        if !dir_path.exists() {
            fs::create_dir_all(&dir_path).map_err(|e| {
                format!(
                    "Project creation failed while creating directory {}: {e}.",
                    dir_path.display()
                )
            })?;
            created.push(PathBuf::from(dir));
        }
    }

    // Helper to write a scaffold-owned file and track its disposition.
    let mut write_scaffold_file = |relative: &str, content: &str| -> Result<(), String> {
        let path = project_dir.join(relative);
        let existed = path.exists();
        fs::write(&path, content).map_err(|e| {
            format!(
                "Project creation failed while writing {relative}: {e}.\n\
                 Some scaffold directories may already have been created. \
                 No existing files were overwritten unless --force was confirmed."
            )
        })?;
        if existed && force {
            replaced.push(PathBuf::from(relative));
        } else {
            created.push(PathBuf::from(relative));
        }
        Ok(())
    };

    // 2. Config file.
    write_scaffold_file(
        CONFIG_FILE,
        &templates::config_template(&target.project_name),
    )?;

    // 3. Starter page.
    write_scaffold_file(PAGE_FILE, templates::page_template())?;

    // 4. Dev manifest.
    write_scaffold_file(DEV_MANIFEST, templates::manifest_template())?;

    // 5. Release manifest.
    write_scaffold_file(RELEASE_MANIFEST, templates::manifest_template())?;

    // 6. .gitignore (handled separately — never overwritten).
    let gitignore = handle_gitignore(project_dir, prompt)?;

    if let Some(path) = gitignore.created {
        created.push(path);
    }

    Ok(CreateProjectReport {
        project_path: project_dir.clone(),
        project_name: target.project_name.clone(),
        created,
        updated: gitignore.updated.into_iter().collect(),
        skipped: gitignore.skipped.into_iter().collect(),
        replaced,
    })
}

/// Result of handling `.gitignore` during scaffold creation.
struct GitignoreResult {
    created: Option<PathBuf>,
    updated: Option<PathBuf>,
    skipped: Option<PathBuf>,
}

/// Handle `.gitignore` creation or append.
///
/// Returns a `GitignoreResult` where at most one field is Some.
fn handle_gitignore(
    project_dir: &Path,
    prompt: &mut impl Prompt,
) -> Result<GitignoreResult, String> {
    let gitignore_path = project_dir.join(GITIGNORE_FILE);

    if !gitignore_path.exists() {
        let should_create =
            prompt.confirm("Add a .gitignore with Beanstalk defaults? [Y/n]: ", true)?;
        if should_create {
            fs::write(&gitignore_path, templates::gitignore_template()).map_err(|e| {
                format!(
                    "Project creation failed while writing .gitignore: {e}.\n\
                     Some scaffold directories may already have been created. \
                     No existing files were overwritten unless --force was confirmed."
                )
            })?;
            return Ok(GitignoreResult {
                created: Some(PathBuf::from(GITIGNORE_FILE)),
                updated: None,
                skipped: None,
            });
        }
        return Ok(GitignoreResult {
            created: None,
            updated: None,
            skipped: Some(PathBuf::from(GITIGNORE_FILE)),
        });
    }

    // Existing .gitignore — check whether it already contains the Beanstalk block.
    let existing = fs::read_to_string(&gitignore_path).map_err(|e| {
        format!(
            "Project creation failed while reading existing .gitignore: {e}.\n\
             Some scaffold directories may already have been created. \
             No existing files were overwritten unless --force was confirmed."
        )
    })?;

    if existing_contains_dev_block(&existing) {
        return Ok(GitignoreResult {
            created: None,
            updated: None,
            skipped: Some(PathBuf::from(GITIGNORE_FILE)),
        });
    }

    let should_append = prompt.confirm(
        ".gitignore already exists.\nAdd missing Beanstalk defaults to it? [Y/n]: ",
        true,
    )?;
    if should_append {
        let mut appended = existing;
        if !appended.ends_with('\n') {
            appended.push('\n');
        }
        appended.push_str(templates::gitignore_append_block());
        fs::write(&gitignore_path, appended).map_err(|e| {
            format!(
                "Project creation failed while appending to .gitignore: {e}.\n\
                 Some scaffold directories may already have been created. \
                 No existing files were overwritten unless --force was confirmed."
            )
        })?;
        return Ok(GitignoreResult {
            created: None,
            updated: Some(PathBuf::from(GITIGNORE_FILE)),
            skipped: None,
        });
    }

    Ok(GitignoreResult {
        created: None,
        updated: None,
        skipped: Some(PathBuf::from(GITIGNORE_FILE)),
    })
}

/// Check whether an existing `.gitignore` already contains the `/dev` Beanstalk rule.
fn existing_contains_dev_block(content: &str) -> bool {
    content.lines().any(|line| line.trim() == "/dev")
}

#[cfg(test)]
mod tests {
    use super::{
        SCAFFOLD_DIRECTORIES, find_scaffold_conflicts, run_preflight_checks, write_scaffold,
    };
    use crate::projects::html_project::new_html_project::prompt::ScriptedPrompt;
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
        assert_eq!(content, super::templates::config_template("Test Site"));
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
        assert_eq!(content, super::templates::page_template());
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
        assert_eq!(dev_manifest, super::templates::manifest_template());
        assert_eq!(release_manifest, super::templates::manifest_template());
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
        assert_eq!(content, super::templates::gitignore_template());
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
}
