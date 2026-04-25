//! Tests for build-loop state transitions and queued rebuild behavior.

use super::{
    DevBuildExecutor, ProjectBuildExecutor, dev_server_error_messages, run_builds_until_stable,
    run_single_build_cycle,
};
use crate::build_system::build::{
    self, BackendBuilder, BuildResult, CleanupPolicy, FileKind, OutputFile, Project,
    ProjectBuilder, WriteMode, WriteOptions,
};
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorMetaDataKey, ErrorType, SourceLocation,
};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_tests::test_support::temp_dir;
use crate::projects::dev_server::error_page::format_compiler_messages;
use crate::projects::dev_server::state::DevServerState;
use crate::projects::dev_server::watch;
use crate::projects::settings::Config;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

fn html_build_result() -> BuildResult {
    BuildResult {
        project: Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::from("<html><body>Hello</body></html>")),
            )],
            entry_page_rel: Some(PathBuf::from("index.html")),
            cleanup_policy: CleanupPolicy::html(),
            warnings: vec![],
        },
        config: Config::new(PathBuf::from("main.bst")),
        warnings: vec![],
        string_table: StringTable::new(),
    }
}

fn watch_scope(root: &Path, output_dir: &Path) -> watch::WatchScope {
    watch::WatchScope {
        output_dir: output_dir.to_path_buf(),
        targets: vec![watch::WatchTarget {
            watch_path: root.to_path_buf(),
            interest_path: None,
            recursive: true,
        }],
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
                    PathBuf::from("docs/basics/index.html"),
                    FileKind::Html(String::from("<html><body>Docs</body></html>")),
                ),
            ],
            entry_page_rel: Some(PathBuf::from("index.html")),
            cleanup_policy: CleanupPolicy::html(),
            warnings: vec![],
        },
        config: Config::new(PathBuf::from("project")),
        warnings: vec![],
        string_table: StringTable::new(),
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
            cleanup_policy: CleanupPolicy::html(),
            warnings: vec![],
        },
        config: Config::new(PathBuf::from("main.bst")),
        warnings: vec![],
        string_table: StringTable::new(),
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
                        project_entry_dir: None,
                        write_mode: WriteMode::AlwaysWrite,
                    },
                    &build_result.string_table,
                )?;
                Ok(build_result)
            }
            Err(messages) => Err(messages),
        }
    }
}

struct InvalidOutputWarningBuilder;

impl BackendBuilder for InvalidOutputWarningBuilder {
    fn build_backend(
        &self,
        _modules: Vec<crate::build_system::build::Module>,
        config: &Config,
        _flags: &[crate::compiler_frontend::Flag],
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        Ok(Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("../escape.js"),
                FileKind::Js(String::from("console.log('broken');")),
            )],
            entry_page_rel: None,
            cleanup_policy: CleanupPolicy::generic([".js"]),
            warnings: vec![CompilerWarning::new(
                "builder warning",
                SourceLocation::from_path(&config.entry_dir, string_table),
                WarningKind::UnusedVariable,
            )],
        })
    }

    fn validate_project_config(
        &self,
        _config: &Config,
        _string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        Ok(())
    }

    fn external_packages(
        &self,
    ) -> crate::compiler_frontend::external_packages::ExternalPackageRegistry {
        crate::compiler_frontend::external_packages::ExternalPackageRegistry::new()
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
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
    let mut messages = CompilerMessages::empty(StringTable::new());
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
    assert!(output_dir.join("docs/basics/index.html").exists());

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
    let (watch_session, watch_trigger) =
        watch::WatchSession::manual(watch_scope(&root, &output_dir));

    let watched_file = root.join("main.bst");
    let mut executor = FakeExecutor::with_on_call(
        vec![Ok(html_build_result()), Ok(html_build_result())],
        Box::new(move |call_index| {
            if call_index == 1 {
                fs::write(&watched_file, "updated")
                    .expect("should mutate watched file during first build");
                watch_trigger.notify_change();
            }
        }),
    );

    let builds = run_builds_until_stable(
        &state,
        &mut executor,
        &root.join("main.bst"),
        &Vec::new(),
        &watch_session,
    )
    .expect("build loop should complete");

    assert!(builds.watch_scope.is_some());
    assert_eq!(executor.call_count.load(Ordering::SeqCst), 2);
    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn rebuild_loop_stops_at_max_consecutive_rebuilds() {
    use super::MAX_CONSECUTIVE_REBUILDS;

    let root = temp_dir("max_rebuilds");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("main.bst"), "start").expect("should write initial source file");
    let output_dir = root.join("dev");
    let state = Arc::new(DevServerState::new(output_dir.clone()));
    let (watch_session, watch_trigger) =
        watch::WatchSession::manual(watch_scope(&root, &output_dir));

    // Build enough responses for every possible rebuild cycle.
    let responses: Vec<_> = (0..MAX_CONSECUTIVE_REBUILDS + 2)
        .map(|_| Ok(html_build_result()))
        .collect();

    let watched_file = root.join("main.bst");
    let counter = Arc::new(AtomicUsize::new(0));
    let counter_clone = counter.clone();

    // Mutate the watched file on every call so fingerprints always change.
    let mut executor = FakeExecutor::with_on_call(
        responses,
        Box::new(move |call_index| {
            counter_clone.store(call_index, Ordering::SeqCst);
            let content = format!("version_{call_index}");
            fs::write(&watched_file, content).expect("should mutate watched file during build");
            watch_trigger.notify_change();
        }),
    );

    let _builds = run_builds_until_stable(
        &state,
        &mut executor,
        &root.join("main.bst"),
        &Vec::new(),
        &watch_session,
    )
    .expect("build loop should complete despite instability");

    // The loop must stop at the safety limit rather than running forever.
    assert_eq!(
        executor.call_count.load(Ordering::SeqCst),
        MAX_CONSECUTIVE_REBUILDS
    );
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
    let mut messages = CompilerMessages::empty(StringTable::new());
    let mut error = CompilerError::compiler_error("expected text");
    error.new_metadata_entry(
        ErrorMetaDataKey::CompilationStage,
        String::from("AST Construction"),
    );
    error.new_metadata_entry(
        ErrorMetaDataKey::PrimarySuggestion,
        String::from("Declare/import the function before calling it"),
    );
    messages.errors.push(error);
    let text = format_compiler_messages(&messages);
    assert!(text.contains("expected text"));
    assert!(text.contains("stage: AST Construction"));
    assert!(text.contains("help: Declare/import the function before calling it"));
}

#[test]
fn project_build_executor_preserves_warnings_when_output_write_fails() {
    let root = temp_dir("write_failure_preserves_warnings");
    fs::create_dir_all(&root).expect("should create temp root");
    let entry_file = root.join("main.bst");
    let output_dir = root.join("dev");
    fs::write(&entry_file, "value = 1\n").expect("should write source file");

    let mut executor =
        ProjectBuildExecutor::new(ProjectBuilder::new(Box::new(InvalidOutputWarningBuilder)));
    let messages = match executor.build_and_write(&entry_file, &[], &output_dir) {
        Ok(_) => panic!("invalid output path should fail writing"),
        Err(messages) => messages,
    };

    assert_eq!(messages.errors.len(), 1);
    assert_eq!(messages.warnings.len(), 1);
    assert_eq!(
        messages.errors[0]
            .location
            .scope
            .to_path_buf(&messages.string_table),
        PathBuf::from("../escape.js")
    );
    assert_eq!(
        messages.warnings[0]
            .location
            .scope
            .to_path_buf(&messages.string_table),
        entry_file
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
