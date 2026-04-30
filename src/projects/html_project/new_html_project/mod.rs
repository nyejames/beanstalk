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

use std::path::{Path, PathBuf};

/// Report of what the scaffold operation created, updated, skipped, or replaced.
#[derive(Debug)]
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
    let report = create_html_project_template_with_prompt(options, &mut prompt)?;
    println!(
        "{}",
        render_summary(&report, std::env::current_dir().ok().as_deref())
    );
    Ok(())
}

/// Create a new HTML project with an injected prompt implementation.
///
/// Phase 3: target resolution and interactive placement are implemented.
/// Phase 4: preflight conflict detection and `--force` handling are implemented.
/// Phase 5: full template rendering and file creation are implemented.
/// Phase 6: summary output and error polishing are implemented.
pub fn create_html_project_template_with_prompt(
    options: NewHtmlProjectOptions,
    prompt: &mut impl Prompt,
) -> Result<CreateProjectReport, String> {
    let current_dir =
        std::env::current_dir().map_err(|e| format!("Failed to resolve current directory: {e}"))?;

    let resolved = target::resolve_project_target(options.raw_path, &current_dir, prompt)?;

    scaffold::run_preflight_checks(&resolved, options.force, prompt)?;

    scaffold::write_scaffold(&resolved, options.force, prompt)
}

/// Render the CLI summary for a successful scaffold.
///
/// WHAT: builds the user-facing text that explains exactly what was created,
/// updated, skipped, or replaced, plus the next steps.
/// WHY: `new` is often a user's first interaction with `bean`; the output must
/// be explicit, calm, and actionable.
pub fn render_summary(report: &CreateProjectReport, current_dir: Option<&Path>) -> String {
    let mut lines = Vec::new();

    lines.push(String::from("Created Beanstalk HTML project:"));
    lines.push(format!("  Project path: {}", report.project_path.display()));
    lines.push(format!("  Project name: {}", report.project_name));

    if !report.created.is_empty() {
        lines.push(String::new());
        lines.push(String::from("Created:"));
        for path in &report.created {
            lines.push(format!("  {}", path.display()));
        }
    }

    if !report.updated.is_empty() {
        lines.push(String::new());
        lines.push(String::from("Updated:"));
        for path in &report.updated {
            lines.push(format!("  {}", path.display()));
        }
    }

    if !report.replaced.is_empty() {
        lines.push(String::new());
        lines.push(String::from("Replaced:"));
        for path in &report.replaced {
            lines.push(format!("  {}", path.display()));
        }
    }

    if !report.skipped.is_empty() {
        lines.push(String::new());
        lines.push(String::from("Skipped:"));
        for path in &report.skipped {
            lines.push(format!("  {}", path.display()));
        }
    }

    lines.push(String::new());
    lines.push(String::from("Next:"));

    let is_current_dir = match current_dir {
        Some(cd) => match (
            std::fs::canonicalize(cd),
            std::fs::canonicalize(&report.project_path),
        ) {
            (Ok(cd_canon), Ok(proj_canon)) => cd_canon == proj_canon,
            _ => false,
        },
        None => false,
    };

    if !is_current_dir {
        lines.push(format!("  cd {}", report.project_path.display()));
    }
    lines.push(String::from("  bean check ."));
    lines.push(String::from("  bean dev ."));

    lines.join("\n")
}

#[cfg(test)]
#[path = "tests/prompt_tests.rs"]
pub mod prompt_tests;

#[cfg(test)]
#[path = "tests/target_tests.rs"]
pub mod target_tests;

#[cfg(test)]
mod tests {
    use super::{CreateProjectReport, render_summary};
    use std::path::{Path, PathBuf};

    fn dummy_report() -> CreateProjectReport {
        CreateProjectReport {
            project_path: PathBuf::from("/path/to/site"),
            project_name: String::from("site"),
            created: vec![PathBuf::from("#config.bst"), PathBuf::from("src/#page.bst")],
            updated: Vec::new(),
            skipped: Vec::new(),
            replaced: Vec::new(),
        }
    }

    #[test]
    fn summary_shows_project_path_and_name() {
        let report = dummy_report();
        let summary = render_summary(&report, None);
        assert!(summary.contains("Created Beanstalk HTML project:"));
        assert!(summary.contains("Project path: /path/to/site"));
        assert!(summary.contains("Project name: site"));
    }

    #[test]
    fn summary_shows_created_section() {
        let report = dummy_report();
        let summary = render_summary(&report, None);
        assert!(summary.contains("Created:"));
        assert!(summary.contains("  #config.bst"));
        assert!(summary.contains("  src/#page.bst"));
    }

    #[test]
    fn summary_omits_empty_sections() {
        let report = dummy_report();
        let summary = render_summary(&report, None);
        assert!(!summary.contains("Updated:"));
        assert!(!summary.contains("Replaced:"));
        assert!(!summary.contains("Skipped:"));
    }

    #[test]
    fn summary_shows_updated_section_when_present() {
        let mut report = dummy_report();
        report.updated.push(PathBuf::from(".gitignore"));
        let summary = render_summary(&report, None);
        assert!(summary.contains("Updated:"));
        assert!(summary.contains("  .gitignore"));
    }

    #[test]
    fn summary_shows_replaced_section_when_present() {
        let mut report = dummy_report();
        report.replaced.push(PathBuf::from("#config.bst"));
        let summary = render_summary(&report, None);
        assert!(summary.contains("Replaced:"));
        assert!(summary.contains("  #config.bst"));
    }

    #[test]
    fn summary_shows_skipped_section_when_present() {
        let mut report = dummy_report();
        report.skipped.push(PathBuf::from(".gitignore"));
        let summary = render_summary(&report, None);
        assert!(summary.contains("Skipped:"));
        assert!(summary.contains("  .gitignore"));
    }

    #[test]
    fn summary_includes_cd_when_not_current_dir() {
        let report = dummy_report();
        let summary = render_summary(&report, Some(Path::new("/other")));
        assert!(summary.contains("cd /path/to/site"));
        assert!(summary.contains("bean check ."));
        assert!(summary.contains("bean dev ."));
    }

    #[test]
    fn summary_omits_cd_when_current_dir() {
        let temp = tempfile::tempdir().unwrap();
        let current = temp.path();
        let report = CreateProjectReport {
            project_path: current.to_path_buf(),
            project_name: String::from("site"),
            created: vec![PathBuf::from("#config.bst")],
            updated: Vec::new(),
            skipped: Vec::new(),
            replaced: Vec::new(),
        };
        let summary = render_summary(&report, Some(current));
        assert!(!summary.contains("cd "));
        assert!(summary.contains("bean check ."));
        assert!(summary.contains("bean dev ."));
    }

    #[test]
    fn summary_always_shows_next_steps() {
        let report = dummy_report();
        let summary = render_summary(&report, None);
        assert!(summary.contains("Next:"));
        assert!(summary.contains("bean check ."));
        assert!(summary.contains("bean dev ."));
    }
}
