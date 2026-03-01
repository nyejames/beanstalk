//! Tests for build-loop state transitions and queued rebuild behavior.

use super::{
    DevBuildExecutor, dev_server_error_messages, format_error_messages, run_builds_until_stable,
    run_single_build_cycle,
};
use crate::build_system::build::{self, BuildResult, FileKind, OutputFile, Project, WriteOptions};
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorType};
use crate::projects::dev_server::state::DevServerState;
use crate::projects::dev_server::watch;
use crate::projects::settings::Config;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_dev_server_build_{prefix}_{unique}"))
}

fn html_build_result() -> BuildResult {
    BuildResult {
        project: Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::from("<html><body>Hello</body></html>")),
            )],
            entry_page_rel: Some(PathBuf::from("index.html")),
            warnings: vec![],
        },
        config: Config::new(PathBuf::from("main.bst")),
        warnings: vec![],
    }
}

fn multi_page_html_build_result() -> BuildResult {
    BuildResult {
        project: Project {
            output_files: vec![
                OutputFile::new(
                    PathBuf::from("index.html"),
                    FileKind::Html(String::from("<html><body>Home</body></html>")),
                ),
                OutputFile::new(
                    PathBuf::from("docs/basics.html"),
                    FileKind::Html(String::from("<html><body>Docs</body></html>")),
                ),
            ],
            entry_page_rel: Some(PathBuf::from("index.html")),
            warnings: vec![],
        },
        config: Config::new(PathBuf::from("project")),
        warnings: vec![],
    }
}

fn html_build_result_without_entry_page() -> BuildResult {
    BuildResult {
        project: Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::from("<html><body>Hello</body></html>")),
            )],
            entry_page_rel: None,
            warnings: vec![],
        },
        config: Config::new(PathBuf::from("main.bst")),
        warnings: vec![],
    }
}

struct FakeExecutor {
    responses: Mutex<Vec<Result<BuildResult, CompilerMessages>>>,
    call_count: AtomicUsize,
    on_call: Option<Box<dyn Fn(usize) + Send + Sync>>,
}

impl FakeExecutor {
    fn new(responses: Vec<Result<BuildResult, CompilerMessages>>) -> Self {
        Self {
            responses: Mutex::new(responses),
            call_count: AtomicUsize::new(0),
            on_call: None,
        }
    }

    fn with_on_call(
        responses: Vec<Result<BuildResult, CompilerMessages>>,
        on_call: Box<dyn Fn(usize) + Send + Sync>,
    ) -> Self {
        Self {
            responses: Mutex::new(responses),
            call_count: AtomicUsize::new(0),
            on_call: Some(on_call),
        }
    }
}

impl DevBuildExecutor for FakeExecutor {
    fn build_and_write(
        &mut self,
        _entry_file: &Path,
        _flags: &[crate::compiler_frontend::Flag],
        output_dir: &Path,
    ) -> Result<BuildResult, CompilerMessages> {
        let call_index = self.call_count.fetch_add(1, Ordering::SeqCst) + 1;
        if let Some(ref callback) = self.on_call {
            callback(call_index);
        }

        let response = self
            .responses
            .lock()
            .expect("responses mutex should not be poisoned")
            .remove(0);

        match response {
            Ok(build_result) => {
                build::write_project_outputs(
                    &build_result.project,
                    &WriteOptions {
                        output_root: output_dir.to_path_buf(),
                    },
                )?;
                Ok(build_result)
            }
            Err(messages) => Err(messages),
        }
    }
}

#[test]
fn successful_build_marks_state_ok_and_sets_entry_page() {
    let root = temp_dir("success");
    fs::create_dir_all(&root).expect("should create temp root");
    let output_dir = root.join("dev");
    let state = Arc::new(DevServerState::new(output_dir.clone()));
    let mut executor = FakeExecutor::new(vec![Ok(html_build_result())]);

    let report = run_single_build_cycle(&state, &mut executor, &root.join("main.bst"), &Vec::new());
    assert!(report.build_ok);
    assert_eq!(report.version, 1);

    let build_state = state
        .build_state
        .lock()
        .expect("build state should not be poisoned");
    assert!(build_state.last_build_ok);
    assert_eq!(
        build_state
            .entry_page_rel
            .as_ref()
            .expect("entry page should be set"),
        &PathBuf::from("index.html")
    );
    assert!(output_dir.join("index.html").exists());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn failed_build_marks_state_and_stores_error_page() {
    let root = temp_dir("failure");
    fs::create_dir_all(&root).expect("should create temp root");
    let state = Arc::new(DevServerState::new(root.join("dev")));
    let mut messages = CompilerMessages::new();
    messages
        .errors
        .push(CompilerError::compiler_error("boom").with_error_type(ErrorType::Rule));
    let mut executor = FakeExecutor::new(vec![Err(messages)]);

    let report = run_single_build_cycle(&state, &mut executor, &root.join("main.bst"), &Vec::new());
    assert!(!report.build_ok);
    assert_eq!(report.version, 1);

    let build_state = state
        .build_state
        .lock()
        .expect("build state should not be poisoned");
    assert!(!build_state.last_build_ok);
    assert!(build_state.last_error_html.is_some());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn successful_multi_page_build_uses_declared_entry_page() {
    let root = temp_dir("multi_page");
    fs::create_dir_all(&root).expect("should create temp root");
    let output_dir = root.join("dev");
    let state = Arc::new(DevServerState::new(output_dir.clone()));
    let mut executor = FakeExecutor::new(vec![Ok(multi_page_html_build_result())]);

    let report = run_single_build_cycle(&state, &mut executor, &root, &Vec::new());
    assert!(report.build_ok);

    let build_state = state
        .build_state
        .lock()
        .expect("build state should not be poisoned");
    assert_eq!(
        build_state.entry_page_rel,
        Some(PathBuf::from("index.html"))
    );
    assert!(output_dir.join("docs/basics.html").exists());

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_without_declared_entry_page_is_treated_as_failure() {
    let root = temp_dir("missing_entry_page");
    fs::create_dir_all(&root).expect("should create temp root");
    let state = Arc::new(DevServerState::new(root.join("dev")));
    let mut executor = FakeExecutor::new(vec![Ok(html_build_result_without_entry_page())]);

    let report = run_single_build_cycle(&state, &mut executor, &root, &Vec::new());
    assert!(!report.build_ok);

    let build_state = state
        .build_state
        .lock()
        .expect("build state should not be poisoned");
    assert!(!build_state.last_build_ok);
    assert!(
        build_state
            .last_build_messages_summary
            .contains("did not declare a dev entry page")
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_version_increments_on_each_attempt() {
    let root = temp_dir("version");
    fs::create_dir_all(&root).expect("should create temp root");
    let state = Arc::new(DevServerState::new(root.join("dev")));
    let mut executor = FakeExecutor::new(vec![Ok(html_build_result()), Ok(html_build_result())]);

    let first = run_single_build_cycle(&state, &mut executor, &root.join("main.bst"), &Vec::new());
    let second = run_single_build_cycle(&state, &mut executor, &root.join("main.bst"), &Vec::new());

    assert_eq!(first.version, 1);
    assert_eq!(second.version, 2);
    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn queued_rebuild_runs_when_files_change_during_build() {
    let root = temp_dir("queued");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("main.bst"), "start").expect("should write initial source file");
    let output_dir = root.join("dev");
    let state = Arc::new(DevServerState::new(output_dir.clone()));

    let watched_file = root.join("main.bst");
    let mut executor = FakeExecutor::with_on_call(
        vec![Ok(html_build_result()), Ok(html_build_result())],
        Box::new(move |call_index| {
            if call_index == 1 {
                fs::write(&watched_file, "updated")
                    .expect("should mutate watched file during first build");
            }
        }),
    );

    let mut baseline =
        watch::collect_fingerprints(&root, &output_dir).expect("should collect baseline");

    let builds = run_builds_until_stable(
        &state,
        &mut executor,
        &root.join("main.bst"),
        &Vec::new(),
        &root,
        &output_dir,
        &mut baseline,
    )
    .expect("build loop should complete");

    assert_eq!(builds, 2);
    assert_eq!(executor.call_count.load(Ordering::SeqCst), 2);
    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn dev_server_error_messages_use_dev_server_error_type() {
    let messages = dev_server_error_messages(Path::new("x.bst"), "oops");
    assert_eq!(messages.errors.len(), 1);
    assert_eq!(messages.errors[0].error_type, ErrorType::DevServer);
}

#[test]
fn format_error_messages_contains_error_text() {
    let mut messages = CompilerMessages::new();
    messages
        .errors
        .push(CompilerError::compiler_error("expected text"));
    let text = format_error_messages(&messages);
    assert!(text.contains("expected text"));
}
