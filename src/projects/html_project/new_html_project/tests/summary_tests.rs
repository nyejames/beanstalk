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
