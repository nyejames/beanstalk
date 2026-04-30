//! HTML project scaffolding orchestration.
//!
//! WHAT: Coordinates target resolution, user prompting, and file creation for `bean new html`.
//! WHY: Keeps CLI dispatch thin and makes the scaffold flow testable without stdin.

pub mod options;
pub mod prompt;

pub(crate) mod scaffold;
pub(crate) mod target;
pub(crate) mod templates;

pub use options::NewHtmlProjectOptions;
pub use prompt::{Prompt, TerminalPrompt};

use std::path::PathBuf;

/// Report of what the scaffold operation created, updated, skipped, or replaced.
// Phase 6 will read these fields for summary output.
#[allow(dead_code)]
pub struct CreateProjectReport {
    pub project_path: PathBuf,
    pub project_name: String,
    pub created: Vec<PathBuf>,
    pub updated: Vec<PathBuf>,
    pub skipped: Vec<PathBuf>,
    pub replaced: Vec<PathBuf>,
}

/// Create a new HTML project using the terminal for prompts.
pub fn create_html_project_template(options: NewHtmlProjectOptions) -> Result<(), String> {
    let mut prompt = TerminalPrompt::new();
    let _report = create_html_project_template_with_prompt(options, &mut prompt)?;
    Ok(())
}

/// Create a new HTML project with an injected prompt implementation.
///
/// Phase 2: module shape is stable. Full target resolution, conflict detection,
/// and template rendering arrive in later phases.
pub fn create_html_project_template_with_prompt(
    options: NewHtmlProjectOptions,
    prompt: &mut impl Prompt,
) -> Result<CreateProjectReport, String> {
    let project_path = match options.raw_path {
        Some(path) => path,
        None => {
            let current = std::env::current_dir()
                .map_err(|e| format!("Failed to resolve current directory: {e}"))?;
            let message = format!(
                "No project path specified. Current directory: {}\n\
                 Create the new HTML project in this directory? [y/N]: ",
                current.display()
            );
            if !prompt.confirm(&message, false)? {
                return Err("Cancelled project creation.".to_string());
            }
            String::new()
        }
    };

    let name_input = prompt.ask("Project name: ")?;
    let project_name = if name_input.trim().is_empty() {
        String::from("Beanstalk Project")
    } else {
        name_input.trim().to_owned()
    };

    let full_path = scaffold::write_legacy_scaffold(project_path, &project_name)?;

    Ok(CreateProjectReport {
        project_path: full_path,
        project_name,
        created: Vec::new(),
        updated: Vec::new(),
        skipped: Vec::new(),
        replaced: Vec::new(),
    })
}

#[cfg(test)]
#[path = "tests/prompt_tests.rs"]
pub mod prompt_tests;
