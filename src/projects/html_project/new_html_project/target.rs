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
// Phase 4 will use missing_directories, target_existed, and target_was_non_empty
// for conflict detection and preflight safety checks.
#[allow(dead_code)]
pub struct ResolvedProjectTarget {
    pub project_dir: PathBuf,
    pub project_name: String,
    pub missing_directories: Vec<PathBuf>,
    pub target_existed: bool,
    pub target_was_non_empty: bool,
}

/// Resolve the final project directory and name from CLI input and user prompts.
pub fn resolve_project_target(
    raw_path: Option<String>,
    current_dir: &Path,
    prompt: &mut impl Prompt,
) -> Result<ResolvedProjectTarget, String> {
    let resolved_dir = resolve_project_dir(raw_path, current_dir, prompt)?;

    let target_existed = resolved_dir.exists();
    let target_was_non_empty = target_existed && is_directory_non_empty(&resolved_dir);

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

    let missing_directories = compute_missing_directories(&resolved_dir);

    Ok(ResolvedProjectTarget {
        project_dir: resolved_dir,
        project_name,
        missing_directories,
        target_existed,
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

fn expand_tilde(path: &str) -> Result<PathBuf, String> {
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

fn normalize_path(path: &Path) -> PathBuf {
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

fn compute_missing_directories(path: &Path) -> Vec<PathBuf> {
    let mut missing = Vec::new();
    let mut current = path;

    while !current.exists() {
        missing.push(current.to_path_buf());
        if let Some(parent) = current.parent() {
            current = parent;
        } else {
            break;
        }
    }

    missing.reverse();
    missing
}

#[cfg(test)]
mod tests {
    use super::{compute_missing_directories, expand_tilde, normalize_path};
    use std::path::PathBuf;

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
    fn expand_tilde_replaces_with_home() {
        let original = std::env::var("HOME").ok();
        unsafe { std::env::set_var("HOME", "/mock/home") };

        assert_eq!(
            expand_tilde("~/site").unwrap(),
            PathBuf::from("/mock/home/site")
        );
        assert_eq!(expand_tilde("~").unwrap(), PathBuf::from("/mock/home"));

        unsafe {
            match original {
                Some(value) => std::env::set_var("HOME", value),
                None => std::env::remove_var("HOME"),
            }
        }
    }

    #[test]
    fn expand_tilde_passes_through_non_tilde_paths() {
        assert_eq!(
            expand_tilde("/absolute/path").unwrap(),
            PathBuf::from("/absolute/path")
        );
        assert_eq!(
            expand_tilde("relative/path").unwrap(),
            PathBuf::from("relative/path")
        );
    }

    #[test]
    fn compute_missing_directories_lists_all_missing() {
        let base = std::env::temp_dir();
        let target = base.join("a").join("b").join("c");

        let missing = compute_missing_directories(&target);

        assert_eq!(
            missing,
            vec![base.join("a"), base.join("a/b"), base.join("a/b/c")]
        );
    }

    #[test]
    fn compute_missing_directories_is_empty_when_path_exists() {
        let base = std::env::temp_dir();
        let missing = compute_missing_directories(&base);
        assert!(missing.is_empty());
    }
}
