//! File-writing scaffold logic.
//!
//! WHAT: Performs preflight conflict checks and actual directory/file creation.
//! WHY: Separates IO side effects from command parsing and user prompting.
//!
//! Note: Template content will be replaced by `templates.rs` in Phase 5.

use std::{fs, path::Path};

use crate::projects::html_project::new_html_project::prompt::Prompt;
use crate::projects::html_project::new_html_project::target::ResolvedProjectTarget;

const CONFIG_FILE: &str = "#config.bst";
const PAGE_FILE: &str = "src/#page.bst";
const DEV_MANIFEST: &str = "dev/.beanstalk_manifest";
const RELEASE_MANIFEST: &str = "release/.beanstalk_manifest";

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

/// Legacy scaffold write path.
///
/// Creates directories and writes a minimal `#config.bst` plus `dev/` and `release/`.
/// Returns the fully resolved project directory path.
///
/// Phase 3: now receives a pre-resolved path from `target.rs` instead of doing
/// its own validation.
pub(crate) fn write_legacy_scaffold(project_dir: &Path, project_name: &str) -> Result<(), String> {
    let name = if project_name.is_empty() {
        "Beanstalk Project"
    } else {
        project_name
    };

    fs::create_dir_all(project_dir).map_err(|e| e.to_string())?;

    let config_content = format!(
        "#project_name = \"{name}\"\n\
         #entry_root = \"src\"\n\
         #dev_folder = \"dev\"\n\
         #output_folder = \"release\"\n\
         #page_url_style = \"trailing_slash\"\n\
         #redirect_index_html = true\n\
         #name = \"html_project\"\n\
         #version = \"0.1.0\"\n\
         #author = \"\"\n\
         #license = \"MIT\"\n"
    );
    fs::write(project_dir.join(CONFIG_FILE), config_content).map_err(|e| e.to_string())?;

    for dir in SCAFFOLD_DIRECTORIES {
        fs::create_dir_all(project_dir.join(dir)).map_err(|e| e.to_string())?;
    }

    println!("Project created at: {:?}", &project_dir);

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{SCAFFOLD_DIRECTORIES, find_scaffold_conflicts, run_preflight_checks};
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
        let mut prompt =
            ScriptedPrompt::new(vec![String::from("1"), String::from(""), String::from("y")]);

        let result = create_html_project_template_with_prompt(options, &mut prompt);
        assert!(result.is_ok(), "expected success, got: {result:?}");

        let config_content = fs::read_to_string(project_dir.join("#config.bst")).unwrap();
        assert!(config_content.contains("project_name"));

        let user_content = fs::read_to_string(project_dir.join("user-file.txt")).unwrap();
        assert_eq!(user_content, "keep me");
    }
}
