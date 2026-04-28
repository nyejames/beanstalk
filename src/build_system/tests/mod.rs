//! Tests for the core build orchestration and output writer APIs.
// NOTE: temp file creation processes have to be explicitly dropped
// Or these tests will fail on Windows due to attempts to delete non-empty temp directories while files are still open.

use super::{WriteOptions, write_project_outputs as write_project_outputs_with_table};
use crate::build_system::build::{
    BackendBuilder, CleanupPolicy, FileKind, OutputFile, Project, WriteMode,
};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorType, SourceLocation,
};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::libraries::LibrarySet;
use crate::projects::settings::Config;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};

struct CurrentDirGuard {
    _lock: MutexGuard<'static, ()>,
    previous: PathBuf,
}

impl CurrentDirGuard {
    fn set_to(path: &PathBuf) -> Self {
        // Intentionally recover from a poisoned mutex. This lock only serializes cwd-mutating
        // tests — it does not protect shared semantic state. Recovering here prevents one
        // panicking test from cascading PoisonError into every subsequent cwd-mutating test.
        let lock = current_dir_test_lock()
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let previous = std::env::current_dir().expect("current dir should resolve");
        std::env::set_current_dir(path).expect("should change current directory for test");
        Self {
            _lock: lock,
            previous,
        }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

fn current_dir_test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn html_cleanup_policy() -> CleanupPolicy {
    CleanupPolicy::html()
}

fn generic_cleanup_policy() -> CleanupPolicy {
    CleanupPolicy::generic([".html", ".js", ".wasm"])
}

fn write_project_outputs(
    project: &Project,
    options: &WriteOptions,
) -> Result<(), CompilerMessages> {
    write_project_outputs_with_table(project, options, &StringTable::default())
}

fn always_write_options(output_root: PathBuf, project_entry_dir: Option<PathBuf>) -> WriteOptions {
    WriteOptions {
        output_root,
        project_entry_dir,
        write_mode: WriteMode::AlwaysWrite,
    }
}

fn skip_unchanged_options(
    output_root: PathBuf,
    project_entry_dir: Option<PathBuf>,
) -> WriteOptions {
    WriteOptions {
        output_root,
        project_entry_dir,
        write_mode: WriteMode::SkipUnchanged,
    }
}

fn html_project(output_files: Vec<OutputFile>, entry_page_rel: Option<PathBuf>) -> Project {
    Project {
        output_files,
        entry_page_rel,
        cleanup_policy: html_cleanup_policy(),
        warnings: vec![],
    }
}

struct WarningBuilder;

impl BackendBuilder for WarningBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
        _string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        Ok(Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("generated.js"),
                FileKind::Js(String::from("console.log('ok');")),
            )],
            entry_page_rel: None,
            cleanup_policy: CleanupPolicy::generic([".js"]),
            warnings: vec![CompilerWarning::new(
                "builder warning",
                SourceLocation::default(),
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

    fn libraries(&self) -> LibrarySet {
        LibrarySet::with_core_packages()
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct ValidationTrackingBuilder {
    validated: std::sync::Arc<std::sync::atomic::AtomicBool>,
    built: std::sync::Arc<std::sync::atomic::AtomicBool>,
}

impl BackendBuilder for ValidationTrackingBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
        _string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        self.built.store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(Project {
            output_files: vec![],
            entry_page_rel: None,
            cleanup_policy: CleanupPolicy::generic(Vec::<&str>::new()),
            warnings: vec![],
        })
    }

    fn validate_project_config(
        &self,
        _config: &Config,
        _string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        self.validated
            .store(true, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    }

    fn libraries(&self) -> LibrarySet {
        LibrarySet::with_core_packages()
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct FailingValidationBuilder;

impl BackendBuilder for FailingValidationBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
        _string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        panic!("should not call build_backend if validation fails");
    }

    fn validate_project_config(
        &self,
        _config: &Config,
        _string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        Err(CompilerError::new(
            "Fake config error",
            SourceLocation::default(),
            ErrorType::Config,
        ))
    }

    fn libraries(&self) -> LibrarySet {
        LibrarySet::with_core_packages()
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct NoDirectiveBuilder;

impl BackendBuilder for NoDirectiveBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
        _string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        Ok(Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::new()),
            )],
            entry_page_rel: Some(PathBuf::from("index.html")),
            cleanup_policy: CleanupPolicy::generic([".html"]),
            warnings: vec![],
        })
    }

    fn validate_project_config(
        &self,
        _config: &Config,
        _string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        Ok(())
    }

    fn libraries(&self) -> LibrarySet {
        LibrarySet::with_core_packages()
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

struct MultiModuleDiagnosticBuilder;

impl BackendBuilder for MultiModuleDiagnosticBuilder {
    fn build_backend(
        &self,
        modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages> {
        let homepage = modules
            .iter()
            .find(|module| module.entry_point.ends_with("src/#page.bst"))
            .expect("directory build should discover homepage module");
        let docs_page = modules
            .iter()
            .find(|module| module.entry_point.ends_with("src/docs/#page.bst"))
            .expect("directory build should discover docs module");

        Err(CompilerMessages {
            errors: vec![CompilerError::new_rule_error(
                "homepage diagnostic",
                SourceLocation::from_path(&homepage.entry_point, string_table),
            )],
            warnings: vec![CompilerWarning::new(
                "docs warning",
                SourceLocation::from_path(&docs_page.entry_point, string_table),
                WarningKind::UnusedVariable,
            )],
            string_table: string_table.clone(),
        })
    }

    fn validate_project_config(
        &self,
        _config: &Config,
        _string_table: &mut StringTable,
    ) -> Result<(), CompilerError> {
        Ok(())
    }

    fn libraries(&self) -> LibrarySet {
        LibrarySet::with_core_packages()
    }

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec> {
        Vec::new()
    }
}

mod build_cleanup_tests;
mod build_directive_tests;
mod build_import_tests;
mod build_infrastructure_tests;
mod build_orchestration_tests;
mod build_receiver_tests;
mod build_runtime_tests;
mod build_struct_tests;
mod build_template_tests;
