//! Tests for dev-server orchestration and entry-path validation.

use super::{DevServerOptions, resolve_dev_runtime_paths, validate_dev_entry_path};
use crate::build_system::build::{BackendBuilder, Project, ProjectBuilder};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::style_directives::{
    StyleDirectiveHandlerSpec, StyleDirectiveSpec, TemplateHeadCompatibility,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TemplateBodyMode;
use crate::compiler_tests::test_support::temp_dir;
use crate::projects::settings::Config;
use std::fs;

struct NoopBuilder;

impl BackendBuilder for NoopBuilder {
    fn build_backend(
        &self,
        _modules: Vec<crate::build_system::build::Module>,
        _config: &Config,
        _flags: &[Flag],
        _string_table: &mut StringTable,
    ) -> Result<Project, crate::compiler_frontend::compiler_errors::CompilerMessages> {
        panic!("build_backend should not run in dev-server output-dir tests");
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

struct ConflictingDirectiveBuilder;

impl BackendBuilder for ConflictingDirectiveBuilder {
    fn build_backend(
        &self,
        _modules: Vec<crate::build_system::build::Module>,
        _config: &Config,
        _flags: &[Flag],
        _string_table: &mut StringTable,
    ) -> Result<Project, crate::compiler_frontend::compiler_errors::CompilerMessages> {
        panic!("build_backend should not run in dev-server output-dir tests");
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
        vec![StyleDirectiveSpec::handler(
            "markdown",
            TemplateBodyMode::Normal,
            TemplateHeadCompatibility::fully_compatible_meaningful(),
            StyleDirectiveHandlerSpec::no_op(),
        )]
    }
}

#[test]
fn defaults_match_dev_server_contract() {
    let defaults = DevServerOptions::default();
    assert_eq!(defaults.host, "127.0.0.1");
    assert_eq!(defaults.port, 6342);
    assert_eq!(defaults.poll_interval_ms, 300);
}

#[test]
fn entry_path_validation_accepts_bst_files() {
    let root = temp_dir("entry_file");
    fs::create_dir_all(&root).expect("should create temp root");
    let file = root.join("main.bst");
    fs::write(&file, "x = 1").expect("should write test file");

    let validated = validate_dev_entry_path(
        file.to_str()
            .expect("temp path should be valid utf-8 for this test"),
    )
    .expect("valid bst path should pass validation");

    assert!(validated.ends_with("main.bst"));
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn entry_path_validation_accepts_directories() {
    let root = temp_dir("entry_dir");
    fs::create_dir_all(&root).expect("should create temp root");
    let validated = validate_dev_entry_path(
        root.to_str()
            .expect("temp path should be valid utf-8 for this test"),
    )
    .expect("directories should be accepted");

    assert_eq!(
        validated,
        root.canonicalize().expect("temp dir should canonicalize")
    );
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn empty_entry_path_uses_current_directory() {
    let expected = std::env::current_dir()
        .expect("current directory should resolve")
        .canonicalize()
        .expect("current directory should canonicalize");
    let validated = validate_dev_entry_path("").expect("empty path should use current directory");
    assert_eq!(validated, expected);
}

#[test]
fn resolve_dev_runtime_paths_use_configured_dev_folder_for_directory_projects() {
    let root = temp_dir("configured_dev_folder");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("#config.bst"), "#dev_folder = \"preview\"\n")
        .expect("should write config");

    let builder = ProjectBuilder::new(Box::new(NoopBuilder));
    let resolved = resolve_dev_runtime_paths(&builder, &root, &[])
        .expect("directory output dir should resolve");

    assert_eq!(resolved.output_dir, root.join("preview"));
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn resolve_dev_runtime_paths_fall_back_to_project_root_for_empty_dev_folder() {
    let root = temp_dir("empty_dev_folder");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("#config.bst"), "#dev_folder = \"\"\n").expect("should write config");

    let builder = ProjectBuilder::new(Box::new(NoopBuilder));
    let resolved = resolve_dev_runtime_paths(&builder, &root, &[])
        .expect("directory output dir should resolve");

    assert_eq!(
        resolved.output_dir,
        root.canonicalize().expect("temp dir should canonicalize")
    );
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn resolve_dev_runtime_paths_return_config_load_failures() {
    let root = temp_dir("bad_config");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("#config.bst"), "import\n").expect("should write bad config");

    let builder = ProjectBuilder::new(Box::new(NoopBuilder));
    let messages = resolve_dev_runtime_paths(&builder, &root, &[])
        .expect_err("bad config should fail directory bootstrap");

    assert_eq!(messages.errors.len(), 1);
    assert!(
        messages.errors[0]
            .msg
            .contains("Expected a path after the 'import' keyword")
    );
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}

#[test]
fn resolve_dev_runtime_paths_return_style_directive_merge_failures() {
    let root = temp_dir("style_directive_conflict");
    fs::create_dir_all(&root).expect("should create temp root");

    let builder = ProjectBuilder::new(Box::new(ConflictingDirectiveBuilder));
    let messages = resolve_dev_runtime_paths(&builder, &root, &[])
        .expect_err("conflicting directives should fail bootstrap");

    assert_eq!(messages.errors.len(), 1);
    assert!(
        messages.errors[0].msg.contains("cannot override")
            || messages.errors[0].msg.contains("already exists")
    );
    fs::remove_dir_all(&root).expect("should clean up temp dir");
}
