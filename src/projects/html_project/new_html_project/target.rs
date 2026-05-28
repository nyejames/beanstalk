//! Target path resolution for `bean new html`.
//!
//! WHAT: Resolves user-provided paths into canonical project directories with
//! interactive prompts for ambiguous or destructive placements.
//! WHY: Path semantics (omitted, relative, absolute, home expansion) and user
//! confirmation belong in one place, separate from file writing.

use std::path::{Path, PathBuf};

use crate::projects::html_project::new_html_project::prompt::Prompt;

/// Fully resolved project placement after all user interaction.
#[derive(Debug)]
pub struct ResolvedProjectTarget {
    pub project_dir: PathBuf,
    pub project_name: String,
    pub target_was_non_empty: bool,
}

/// Resolve the final project directory and name from CLI input and user prompts.
pub fn resolve_project_target(
    raw_path: Option<String>,
    current_dir: &Path,
    prompt: &mut impl Prompt,
) -> Result<ResolvedProjectTarget, String> {
    let resolved_dir = resolve_project_dir(raw_path, current_dir, prompt)?;

    let target_exists = resolved_dir.exists();
    let target_was_non_empty = target_exists && is_directory_non_empty(&resolved_dir);

    let default_name = resolved_dir
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Beanstalk Project")
        .to_owned();

    let name_input = prompt.ask(&format!(
        "Project name (press Enter to use {default_name}): "
    ))?;
    let project_name = if name_input.trim().is_empty() {
        default_name
    } else {
        name_input.trim().to_owned()
    };

    Ok(ResolvedProjectTarget {
        project_dir: resolved_dir,
        project_name,
        target_was_non_empty,
    })
}

fn resolve_project_dir(
    raw_path: Option<String>,
    current_dir: &Path,
    prompt: &mut impl Prompt,
) -> Result<PathBuf, String> {
    match raw_path {
        None => {
            let message = format!(
                "No project path specified. Current directory: {}\n\
                 Create the new HTML project in this directory? [y/N]: ",
                current_dir.display()
            );
            if !prompt.confirm(&message, false)? {
                return Err("Cancelled project creation.".to_string());
            }
            Ok(current_dir.to_path_buf())
        }
        Some(path) if path == "." => Ok(current_dir.to_path_buf()),
        Some(path) => {
            let expanded = expand_tilde(&path)?;
            let resolved = if expanded.is_absolute() {
                normalize_path(&expanded)
            } else {
                normalize_path(&current_dir.join(expanded))
            };

            if resolved.exists() {
                handle_existing_directory(&resolved, prompt)
            } else {
                handle_missing_directory(&resolved, prompt)
            }
        }
    }
}

fn handle_existing_directory(path: &Path, prompt: &mut impl Prompt) -> Result<PathBuf, String> {
    let message = format!(
        "Target directory already exists:\n\
         {}\n\n\
         What do you want to do?\n\
           1. Create the project inside this directory\n\
           2. Create a new child folder for the project inside this directory\n\
           3. Cancel\n\n\
         Choose [1/2/3]: ",
        path.display()
    );

    loop {
        let choice = prompt.ask(&message)?;
        match choice.trim() {
            "1" => return Ok(path.to_path_buf()),
            "2" => {
                let folder_name = prompt.ask("Project folder name: ")?;
                let trimmed = folder_name.trim();
                if trimmed.is_empty() {
                    continue;
                }
                return Ok(path.join(trimmed));
            }
            "3" => return Err("Cancelled project creation.".to_string()),
            _ => continue,
        }
    }
}

fn handle_missing_directory(path: &Path, prompt: &mut impl Prompt) -> Result<PathBuf, String> {
    let message = format!(
        "The project target contains directories that do not exist:\n\
         {}\n\n\
         Create the missing directories and scaffold the project there? [y/N]: ",
        path.display()
    );

    if !prompt.confirm(&message, false)? {
        return Err("Cancelled project creation.".to_string());
    }

    Ok(path.to_path_buf())
}

pub(super) fn expand_tilde(path: &str) -> Result<PathBuf, String> {
    if let Some(rest) = path.strip_prefix('~') {
        let home = std::env::var("HOME")
            .map_err(|_| "Could not determine home directory for '~' expansion.".to_string())?;
        let home_path = PathBuf::from(home);
        let rest_trimmed = rest.trim_start_matches('/');
        if rest_trimmed.is_empty() {
            Ok(home_path)
        } else {
            Ok(home_path.join(rest_trimmed))
        }
    } else {
        Ok(PathBuf::from(path))
    }
}

pub(super) fn normalize_path(path: &Path) -> PathBuf {
    let mut result = PathBuf::new();

    for component in path.components() {
        match component {
            std::path::Component::Prefix(prefix) => result.push(prefix.as_os_str()),
            std::path::Component::RootDir => result.push("/"),
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if result.file_name().is_some() {
                    result.pop();
                }
            }
            std::path::Component::Normal(name) => result.push(name),
        }
    }

    result
}

fn is_directory_non_empty(path: &Path) -> bool {
    if let Ok(mut entries) = std::fs::read_dir(path) {
        entries.next().is_some()
    } else {
        false
    }
}
