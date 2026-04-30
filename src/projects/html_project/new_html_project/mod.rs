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
#[derive(Debug)]
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
/// Phase 3: target resolution and interactive placement are implemented.
/// Phase 4: preflight conflict detection and `--force` handling are implemented.
/// Phase 5 will replace the legacy scaffold write with full template rendering.
pub fn create_html_project_template_with_prompt(
    options: NewHtmlProjectOptions,
    prompt: &mut impl Prompt,
) -> Result<CreateProjectReport, String> {
    let current_dir =
        std::env::current_dir().map_err(|e| format!("Failed to resolve current directory: {e}"))?;

    let resolved = target::resolve_project_target(options.raw_path, &current_dir, prompt)?;

    scaffold::run_preflight_checks(&resolved, options.force, prompt)?;

    scaffold::write_legacy_scaffold(&resolved.project_dir, &resolved.project_name)?;

    Ok(CreateProjectReport {
        project_path: resolved.project_dir,
        project_name: resolved.project_name,
        created: Vec::new(),
        updated: Vec::new(),
        skipped: Vec::new(),
        replaced: Vec::new(),
    })
}

#[cfg(test)]
#[path = "tests/prompt_tests.rs"]
pub mod prompt_tests;

#[cfg(test)]
#[path = "tests/target_tests.rs"]
pub mod target_tests;
