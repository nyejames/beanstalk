//! Build execution and watch-triggered rebuild coordination for the dev server.
//!
//! This module connects the shared compiler pipeline to the dev server state, writes build
//! artifacts into the dev output directory, and broadcasts reload events after each build attempt.

use crate::build_system::build::{FileKind, OutputFile, Project, ProjectBuilder};
use crate::build_system::create_project_modules::compile_project_frontend;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorType, error_type_to_str,
};
use crate::projects::dev_server::error_page::{
    format_compiler_messages, render_runtime_error_page,
};
use crate::projects::dev_server::sse;
use crate::projects::dev_server::state::DevServerState;
use crate::projects::dev_server::watch;
use crate::projects::settings::Config;
use saying::say;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

pub struct BuildCycleReport {
    pub version: u64,
    pub build_ok: bool,
    pub clients_notified: usize,
}

struct BuildOutcome {
    build_succeeded: bool,
    entry_page_rel: Option<PathBuf>,
    diagnostics_summary: String,
    error_title: Option<String>,
}

pub trait DevBuildExecutor: Send + Sync {
    fn build_project(&self, entry_file: &Path, flags: &[Flag])
    -> Result<Project, CompilerMessages>;
}

pub struct ProjectBuildExecutor {
    builder: Box<dyn ProjectBuilder + Send + Sync>,
}

impl ProjectBuildExecutor {
    pub fn new(builder: Box<dyn ProjectBuilder + Send + Sync>) -> Self {
        Self { builder }
    }
}

impl DevBuildExecutor for ProjectBuildExecutor {
    fn build_project(
        &self,
        entry_file: &Path,
        flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        let mut config = Config::new(entry_file.to_path_buf());
        let modules = compile_project_frontend(&mut config, flags)?;
        self.builder.build_backend(modules, &config, flags)
    }
}

struct WriteOutputResult {
    html_entries: Vec<PathBuf>,
}

pub fn run_single_build_cycle(
    state: &Arc<DevServerState>,
    executor: &dyn DevBuildExecutor,
    entry_file: &Path,
    flags: &[Flag],
) -> BuildCycleReport {
    let output_dir = match state.build_state.lock() {
        Ok(guard) => guard.output_dir.clone(),
        Err(_) => PathBuf::from("dev"),
    };

    let build_outcome = build_once(executor, entry_file, flags, &output_dir);

    let version = {
        let mut build_state = state
            .build_state
            .lock()
            .expect("build state should not be poisoned");
        build_state.last_build_version = build_state.last_build_version.saturating_add(1);
        build_state.last_build_ok = build_outcome.build_succeeded;
        build_state.last_build_messages_summary = build_outcome.diagnostics_summary.clone();

        if build_outcome.build_succeeded {
            build_state.last_error_html = None;
            build_state.entry_page_rel = build_outcome.entry_page_rel.clone();
        } else {
            let title = build_outcome
                .error_title
                .clone()
                .unwrap_or_else(|| String::from("Build Failed"));
            build_state.last_error_html = Some(render_runtime_error_page(
                &title,
                &build_outcome.diagnostics_summary,
                build_state.last_build_version,
            ));
        }

        build_state.last_build_version
    };

    let clients_notified = sse::broadcast_reload(state, version);
    BuildCycleReport {
        version,
        build_ok: build_outcome.build_succeeded,
        clients_notified,
    }
}

pub fn run_builds_until_stable(
    state: &Arc<DevServerState>,
    executor: &dyn DevBuildExecutor,
    entry_file: &Path,
    flags: &[Flag],
    watch_root: &Path,
    output_dir: &Path,
    baseline_fingerprints: &mut HashMap<PathBuf, watch::FileFingerprint>,
) -> io::Result<usize> {
    let mut build_count = 0usize;

    loop {
        // Capture source fingerprints before building so we can detect edits made during build.
        let before_build = watch::collect_fingerprints(watch_root, output_dir)?;
        let report = run_single_build_cycle(state, executor, entry_file, flags);
        build_count += 1;

        if report.build_ok {
            say!(
                Green "Dev build #",
                Green report.version,
                Green " finished successfully. Reload broadcast to ",
                Green report.clients_notified,
                Green " clients."
            );
        } else {
            say!(
                Yellow "Dev build #",
                Yellow report.version,
                Yellow " failed. Reload broadcast to ",
                Yellow report.clients_notified,
                Yellow " clients."
            );
        }

        let after_build = watch::collect_fingerprints(watch_root, output_dir)?;
        *baseline_fingerprints = after_build.clone();

        // Queue one immediate follow-up build if files changed while the previous build was running.
        if !watch::detect_changes(&before_build, &after_build) {
            break;
        }
    }

    Ok(build_count)
}

pub fn run_watch_build_loop(
    state: Arc<DevServerState>,
    executor: Arc<dyn DevBuildExecutor>,
    entry_file: PathBuf,
    flags: Vec<Flag>,
    watch_root: PathBuf,
    poll_interval: Duration,
) {
    let debounce_window = poll_interval;
    let output_dir = match state.build_state.lock() {
        Ok(guard) => guard.output_dir.clone(),
        Err(_) => return,
    };

    let mut known_fingerprints = match watch::collect_fingerprints(&watch_root, &output_dir) {
        Ok(fingerprints) => fingerprints,
        Err(error) => {
            say!(
                Yellow "Dev server watch warning: failed to collect initial fingerprints: ",
                Yellow error.to_string()
            );
            HashMap::new()
        }
    };

    let mut dirty_since: Option<Instant> = None;

    loop {
        thread::sleep(poll_interval);

        let current_fingerprints = match watch::collect_fingerprints(&watch_root, &output_dir) {
            Ok(fingerprints) => fingerprints,
            Err(error) => {
                say!(
                    Yellow "Dev server watch warning: scan failed: ",
                    Yellow error.to_string()
                );
                continue;
            }
        };

        if watch::detect_changes(&known_fingerprints, &current_fingerprints) {
            known_fingerprints = current_fingerprints;
            dirty_since = Some(Instant::now());
        }

        if !watch::should_trigger_debounced_build(dirty_since, debounce_window) {
            continue;
        }

        if let Err(error) = run_builds_until_stable(
            &state,
            executor.as_ref(),
            &entry_file,
            &flags,
            &watch_root,
            &output_dir,
            &mut known_fingerprints,
        ) {
            say!(
                Yellow "Dev server watch warning: rebuild cycle failed: ",
                Yellow error.to_string()
            );
        }

        dirty_since = None;
    }
}

fn build_once(
    executor: &dyn DevBuildExecutor,
    entry_file: &Path,
    flags: &[Flag],
    output_dir: &Path,
) -> BuildOutcome {
    let project = match executor.build_project(entry_file, flags) {
        Ok(project) => project,
        Err(messages) => {
            let summary = format_compiler_messages(&messages);
            return BuildOutcome {
                build_succeeded: false,
                entry_page_rel: None,
                diagnostics_summary: summary,
                error_title: Some(String::from("Build Failed")),
            };
        }
    };

    let warnings_summary = project
        .warnings
        .iter()
        .map(|warning| warning.msg.clone())
        .collect::<Vec<String>>()
        .join("\n");

    match write_output_files(&project.output_files, output_dir) {
        Ok(write_result) => {
            if write_result.html_entries.len() == 1 {
                let diagnostics_summary = if warnings_summary.is_empty() {
                    String::from("Build succeeded.")
                } else {
                    format!("Build succeeded with warnings:\n{warnings_summary}")
                };

                BuildOutcome {
                    build_succeeded: true,
                    entry_page_rel: write_result.html_entries.first().cloned(),
                    diagnostics_summary,
                    error_title: None,
                }
            } else if write_result.html_entries.is_empty() {
                // Phase-1 serves a single HTML page at `/`, so this is a runtime dev-server failure.
                let details = "Build completed, but no HTML entry was emitted. \
                               Single-file dev mode currently requires exactly one HTML output."
                    .to_string();
                BuildOutcome {
                    build_succeeded: false,
                    entry_page_rel: None,
                    diagnostics_summary: details,
                    error_title: Some(String::from("Missing HTML Entry")),
                }
            } else {
                let details = format!(
                    "Build emitted {} HTML files, but single-file dev mode requires exactly one HTML entry.",
                    write_result.html_entries.len()
                );
                BuildOutcome {
                    build_succeeded: false,
                    entry_page_rel: None,
                    diagnostics_summary: details,
                    error_title: Some(String::from("Multiple HTML Entries")),
                }
            }
        }
        Err(error) => {
            let details = error.to_string();
            BuildOutcome {
                build_succeeded: false,
                entry_page_rel: None,
                diagnostics_summary: details,
                error_title: Some(String::from("Output Write Failed")),
            }
        }
    }
}

fn write_output_files(
    output_files: &[OutputFile],
    output_dir: &Path,
) -> io::Result<WriteOutputResult> {
    fs::create_dir_all(output_dir)?;
    let mut html_entries = Vec::new();

    for output_file in output_files {
        let stem = safe_output_stem(&output_file.full_file_path);

        match output_file.file_kind() {
            FileKind::NotBuilt => {}
            FileKind::Directory => {
                fs::create_dir_all(output_dir.join(stem))?;
            }
            FileKind::Js(source) => {
                fs::write(output_dir.join(format!("{stem}.js")), source)?;
            }
            FileKind::Wasm(bytes) => {
                fs::write(output_dir.join(format!("{stem}.wasm")), bytes)?;
            }
            FileKind::Html(markup) => {
                let relative_path = PathBuf::from(format!("{stem}.html"));
                fs::write(output_dir.join(&relative_path), markup)?;
                html_entries.push(relative_path);
            }
        }
    }

    Ok(WriteOutputResult { html_entries })
}

fn safe_output_stem(path: &Path) -> String {
    let raw_stem = path
        .file_stem()
        .and_then(OsStr::to_str)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("output");

    // Keep generated filenames predictable and filesystem-safe across platforms.
    let sanitized: String = raw_stem
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' {
                ch
            } else {
                '_'
            }
        })
        .collect();

    if sanitized.is_empty() {
        String::from("output")
    } else {
        sanitized
    }
}

pub fn dev_server_error_messages(path: &Path, msg: impl Into<String>) -> CompilerMessages {
    let error = CompilerError::file_error(path, msg.into()).with_error_type(ErrorType::DevServer);
    CompilerMessages {
        errors: vec![error],
        warnings: Vec::new(),
    }
}

pub fn format_error_messages(messages: &CompilerMessages) -> String {
    let mut formatted = String::new();
    for error in &messages.errors {
        let line = format!("[{}] {}\n", error_type_to_str(&error.error_type), error.msg);
        formatted.push_str(&line);
    }
    formatted
}

#[cfg(test)]
#[path = "tests/build_loop_tests.rs"]
mod tests;
