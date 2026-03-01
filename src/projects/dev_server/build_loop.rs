//! Build execution and watch-triggered rebuild coordination for the dev server.
//!
//! This module delegates compilation and artifact writing to the core build APIs, then translates
//! build outcomes into dev-server state updates and SSE reload broadcasts.

use crate::build_system::build::{self, BuildResult, ProjectBuilder, WriteOptions};
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
use saying::say;
use std::collections::HashMap;
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

/// Adapter for build execution used by the dev loop.
///
/// Keeping this contract small makes the watch/build coordination testable while still delegating
/// real work to the core build APIs.
pub trait DevBuildExecutor: Send {
    fn build_and_write(
        &mut self,
        entry_file: &Path,
        flags: &[Flag],
        output_dir: &Path,
    ) -> Result<BuildResult, CompilerMessages>;
}

pub struct ProjectBuildExecutor {
    builder: Box<dyn ProjectBuilder + Send>,
}

impl ProjectBuildExecutor {
    pub fn new(builder: Box<dyn ProjectBuilder + Send>) -> Self {
        Self { builder }
    }
}

impl DevBuildExecutor for ProjectBuildExecutor {
    fn build_and_write(
        &mut self,
        entry_file: &Path,
        flags: &[Flag],
        output_dir: &Path,
    ) -> Result<BuildResult, CompilerMessages> {
        let entry_path = entry_file.to_str().ok_or_else(|| {
            dev_server_error_messages(
                entry_file,
                "Dev server entry path contains invalid UTF-8 and cannot be compiled.",
            )
        })?;

        let build_result = build::build_project(self.builder.as_ref(), entry_path, flags)?;
        build::write_project_outputs(
            &build_result.project,
            &WriteOptions {
                output_root: output_dir.to_path_buf(),
            },
        )?;

        Ok(build_result)
    }
}

pub fn run_single_build_cycle(
    state: &Arc<DevServerState>,
    executor: &mut dyn DevBuildExecutor,
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
    executor: &mut dyn DevBuildExecutor,
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
    mut executor: Box<dyn DevBuildExecutor>,
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
            executor.as_mut(),
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
    executor: &mut dyn DevBuildExecutor,
    entry_file: &Path,
    flags: &[Flag],
    output_dir: &Path,
) -> BuildOutcome {
    let build_result = match executor.build_and_write(entry_file, flags, output_dir) {
        Ok(build_result) => build_result,
        Err(messages) => {
            return BuildOutcome {
                build_succeeded: false,
                entry_page_rel: None,
                diagnostics_summary: format_compiler_messages(&messages),
                error_title: Some(String::from("Build Failed")),
            };
        }
    };

    let warnings_summary = build_result
        .warnings
        .iter()
        .map(|warning| warning.msg.clone())
        .collect::<Vec<String>>()
        .join("\n");

    if let Some(entry_page_rel) = build_result.project.entry_page_rel.clone() {
        let diagnostics_summary = if warnings_summary.is_empty() {
            String::from("Build succeeded.")
        } else {
            format!("Build succeeded with warnings:\n{warnings_summary}")
        };

        BuildOutcome {
            build_succeeded: true,
            entry_page_rel: Some(entry_page_rel),
            diagnostics_summary,
            error_title: None,
        }
    } else {
        BuildOutcome {
            build_succeeded: false,
            entry_page_rel: None,
            diagnostics_summary: String::from(
                "Build completed, but the project builder did not declare a dev entry page.",
            ),
            error_title: Some(String::from("Missing Dev Entry")),
        }
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
