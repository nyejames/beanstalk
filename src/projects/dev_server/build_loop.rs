//! Build execution and watch-triggered rebuild coordination for the dev server.
//!
//! This module delegates compilation and artifact writing to the core build APIs, then translates
//! build outcomes into dev-server state updates and SSE reload broadcasts.
use crate::build_system::build::{self, BuildResult, ProjectBuilder, WriteMode, WriteOptions};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::projects::dev_server::error_page::{
    format_compiler_messages, render_compiler_error_page, render_runtime_error_page,
};
use crate::projects::dev_server::sse;
use crate::projects::dev_server::state::DevServerState;
use crate::projects::dev_server::watch;
use crate::projects::routing::{HtmlSiteConfig, parse_html_site_config};
use saying::say;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub struct BuildCycleReport {
    pub version: u64,
    pub build_ok: bool,
    pub clients_notified: usize,
    pub watch_scope: Option<watch::WatchScope>,
}

#[derive(Debug, Clone)]
pub struct RebuildRunReport {
    pub watch_scope: Option<watch::WatchScope>,
}

struct BuildOutcome {
    build_succeeded: bool,
    entry_page_rel: Option<PathBuf>,
    html_site_config: Option<HtmlSiteConfig>,
    diagnostics_summary: String,
    failed_build: Option<BuildFailure>,
    watch_scope: Option<watch::WatchScope>,
}

enum BuildFailure {
    CompilerMessages(CompilerMessages),
    RuntimeError { title: String, details: String },
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
    builder: ProjectBuilder,
}

impl ProjectBuildExecutor {
    pub fn new(builder: ProjectBuilder) -> Self {
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

        let build_result = build::build_project(&self.builder, entry_path, flags)?;
        let project_entry_dir = entry_file
            .parent()
            .filter(|parent| parent.is_dir())
            .map(Path::to_path_buf)
            .or_else(|| Some(entry_file.to_path_buf()))
            .filter(|path| path.is_dir());
        if let Err(mut messages) = build::write_project_outputs(
            &build_result.project,
            &WriteOptions {
                output_root: output_dir.to_path_buf(),
                project_entry_dir,
                write_mode: WriteMode::SkipUnchanged,
            },
            &build_result.string_table,
        ) {
            messages
                .warnings
                .extend(build_result.warnings.iter().cloned());
            return Err(messages);
        }

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
    let project_root = dev_server_project_root(entry_file);
    let BuildOutcome {
        build_succeeded,
        entry_page_rel,
        html_site_config,
        diagnostics_summary,
        failed_build,
        watch_scope,
    } = build_outcome;

    let version = {
        // If a previous dev-server task panicked while holding the lock, keep the latest state and
        // continue serving rebuild results instead of crashing the entire watcher loop.
        let mut build_state = match state.build_state.lock() {
            Ok(guard) => guard,
            Err(poisoned) => {
                say!(
                    Yellow "Dev server state warning: recovering from a poisoned build-state lock after a previous panic."
                );
                poisoned.into_inner()
            }
        };
        build_state.last_build_version = build_state.last_build_version.saturating_add(1);
        build_state.last_build_ok = build_succeeded;
        build_state.last_build_messages_summary = diagnostics_summary;

        if build_succeeded {
            build_state.last_error_html = None;
            build_state.entry_page_rel = entry_page_rel;
            if let Some(html_site_config) = html_site_config {
                build_state.html_site_config = html_site_config;
            }
        } else {
            // Render compiler diagnostics only after the version increments so the error page and
            // the SSE reload event always point at the same build number.
            build_state.last_error_html = Some(match failed_build {
                Some(BuildFailure::CompilerMessages(messages)) => render_compiler_error_page(
                    &messages,
                    &project_root,
                    &build_state.html_site_config.origin,
                    build_state.last_build_version,
                ),
                Some(BuildFailure::RuntimeError { title, details }) => render_runtime_error_page(
                    &title,
                    &details,
                    &build_state.html_site_config.origin,
                    build_state.last_build_version,
                ),
                None => render_runtime_error_page(
                    "Build Failed",
                    "The latest build failed, but no diagnostics were stored.",
                    &build_state.html_site_config.origin,
                    build_state.last_build_version,
                ),
            });
        }

        build_state.last_build_version
    };

    let clients_notified = sse::broadcast_reload(state, version);
    BuildCycleReport {
        version,
        build_ok: build_succeeded,
        clients_notified,
        watch_scope,
    }
}

/// Maximum consecutive rebuilds before the loop stops to prevent infinite rebuild cycles.
/// If the build itself modifies watched files (e.g. through file-system side effects),
/// the fingerprint check would trigger indefinitely without this limit.
const MAX_CONSECUTIVE_REBUILDS: usize = 5;

pub fn run_builds_until_stable(
    state: &Arc<DevServerState>,
    executor: &mut dyn DevBuildExecutor,
    entry_file: &Path,
    flags: &[Flag],
    watch_session: &watch::WatchSession,
) -> io::Result<RebuildRunReport> {
    let mut build_count = 0usize;
    let latest_watch_scope = loop {
        let build_start_revision = watch_session.current_revision();
        let report = run_single_build_cycle(state, executor, entry_file, flags);
        build_count += 1;
        let report_watch_scope = report.watch_scope.clone();

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

        // Queue one immediate follow-up build when the watch revision advances during a build.
        if watch_session.current_revision() == build_start_revision {
            break report_watch_scope;
        }

        if build_count >= MAX_CONSECUTIVE_REBUILDS {
            say!(
                Yellow "Dev server reached ",
                Yellow MAX_CONSECUTIVE_REBUILDS,
                Yellow " consecutive rebuilds without stabilising — pausing rebuild loop. ",
                Yellow "This usually means the build is modifying watched source files."
            );
            break report_watch_scope;
        }
    };

    Ok(RebuildRunReport {
        watch_scope: latest_watch_scope,
    })
}

pub fn run_watch_build_loop(
    state: Arc<DevServerState>,
    mut executor: Box<dyn DevBuildExecutor>,
    entry_file: PathBuf,
    flags: Vec<Flag>,
    initial_watch_scope: watch::WatchScope,
    poll_interval: Duration,
) {
    let mut watch_session = watch::WatchSession::start(initial_watch_scope, poll_interval);
    let mut last_seen_revision = watch_session.current_revision();

    loop {
        let seen_revision = match watch_session.wait_for_stable_change(last_seen_revision) {
            Ok(seen_revision) => seen_revision,
            Err(error) => {
                say!(
                    Yellow "Dev server watch warning: failed while waiting for file changes: ",
                    Yellow error.to_string()
                );
                return;
            }
        };

        let rebuild_report = match run_builds_until_stable(
            &state,
            executor.as_mut(),
            &entry_file,
            &flags,
            &watch_session,
        ) {
            Ok(report) => report,
            Err(error) => {
                say!(
                    Yellow "Dev server watch warning: rebuild cycle failed: ",
                    Yellow error.to_string()
                );
                last_seen_revision = watch_session.current_revision().max(seen_revision);
                continue;
            }
        };

        last_seen_revision = watch_session.current_revision().max(seen_revision);

        if let Some(next_watch_scope) = rebuild_report.watch_scope
            && next_watch_scope != *watch_session.scope()
        {
            watch_session = watch::WatchSession::start(next_watch_scope, poll_interval);
            last_seen_revision = watch_session.current_revision();
        }
    }
}

fn build_once(
    executor: &mut dyn DevBuildExecutor,
    entry_file: &Path,
    flags: &[Flag],
    output_dir: &Path,
) -> BuildOutcome {
    let mut build_result = match executor.build_and_write(entry_file, flags, output_dir) {
        Ok(build_result) => build_result,
        Err(messages) => {
            return BuildOutcome {
                build_succeeded: false,
                entry_page_rel: None,
                html_site_config: None,
                diagnostics_summary: format_compiler_messages(&messages),
                failed_build: Some(BuildFailure::CompilerMessages(messages)),
                watch_scope: None,
            };
        }
    };
    let watch_scope = watch::WatchScope::derive(
        &build_result.config.entry_dir,
        Some(&build_result.config),
        output_dir,
    );

    let html_site_config =
        match parse_html_site_config(&build_result.config, &mut build_result.string_table) {
            Ok(config) => config,
            Err(error) => {
                let messages = CompilerMessages {
                    errors: vec![error],
                    warnings: Vec::new(),
                    string_table: build_result.string_table.clone(),
                };
                return BuildOutcome {
                    build_succeeded: false,
                    entry_page_rel: None,
                    html_site_config: None,
                    diagnostics_summary: format_compiler_messages(&messages),
                    failed_build: Some(BuildFailure::CompilerMessages(messages)),
                    watch_scope: Some(watch_scope),
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
            html_site_config: Some(html_site_config),
            diagnostics_summary,
            failed_build: None,
            watch_scope: Some(watch_scope),
        }
    } else {
        BuildOutcome {
            build_succeeded: false,
            entry_page_rel: None,
            html_site_config: None,
            diagnostics_summary: String::from(
                "Build completed, but the project builder did not declare a dev entry page.",
            ),
            failed_build: Some(BuildFailure::RuntimeError {
                title: String::from("Missing Dev Entry"),
                details: String::from(
                    "Build completed, but the project builder did not declare a dev entry page.",
                ),
            }),
            watch_scope: Some(watch_scope),
        }
    }
}

fn dev_server_project_root(entry_file: &Path) -> PathBuf {
    if entry_file.is_dir() {
        return entry_file.to_path_buf();
    }

    match entry_file.parent() {
        Some(parent) => parent.to_path_buf(),
        None => PathBuf::from("."),
    }
}

pub fn dev_server_error_messages(path: &Path, msg: impl Into<String>) -> CompilerMessages {
    let mut string_table = Default::default();
    let error = CompilerError::file_error(path, msg.into(), &mut string_table)
        .with_error_type(ErrorType::DevServer);
    CompilerMessages {
        errors: vec![error],
        warnings: Vec::new(),
        string_table,
    }
}

#[cfg(test)]
#[path = "tests/build_loop_tests.rs"]
mod tests;
