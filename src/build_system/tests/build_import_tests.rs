//! Tests for the core build orchestration and output writer APIs.
// NOTE: temp file creation processes have to be explicitly dropped
// Or these tests will fail on Windows due to attempts to delete non-empty temp directories while files are still open.

use super::*;
use crate::build_system::build::{ProjectBuilder, build_project};
use crate::compiler_frontend::basic_utility_functions::normalize_path;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use std::fs;

#[test]
fn build_single_file_project_includes_reachable_import_files() {
    let root = temp_dir("single_file_reachable_imports");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::create_dir_all(root.join("utils")).expect("should create utils directory");
    fs::write(
        root.join("main.bst"),
        "import @utils/helper/greet\ngreet()\n",
    )
    .expect("should write main file");
    fs::write(
        root.join("utils/helper.bst"),
        "#greet||:\n    io(\"hello\")\n;\n",
    )
    .expect("should write helper file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

        assert!(
            !result.project.output_files.is_empty(),
            "single-file build should compile reachable imported files"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_keeps_one_shared_string_table_for_multi_module_diagnostics() {
    let root = temp_dir("multi_module_diagnostics");
    let src_dir = root.join("src");
    let docs_dir = src_dir.join("docs");
    fs::create_dir_all(&docs_dir).expect("should create docs directory");
    fs::write(root.join("#config.bst"), "#entry_root = \"src\"\n").expect("should write config");
    fs::write(src_dir.join("#page.bst"), "value = 1\n").expect("should write homepage");
    fs::write(docs_dir.join("#page.bst"), "value = 2\n").expect("should write docs page");

    let builder = ProjectBuilder::new(Box::new(MultiModuleDiagnosticBuilder));
    let Err(messages) = build_project(
        &builder,
        root.to_str().expect("temp dir path should be valid UTF-8"),
        &[],
    ) else {
        panic!("builder diagnostics should fail the build");
    };

    assert_eq!(messages.errors.len(), 1);
    assert_eq!(messages.warnings.len(), 1);

    assert_eq!(
        normalize_path(
            &messages.errors[0]
                .location
                .scope
                .to_path_buf(&messages.string_table)
        ),
        normalize_path(
            &fs::canonicalize(src_dir.join("#page.bst")).expect("homepage should canonicalize")
        )
    );
    assert_eq!(
        normalize_path(
            &messages.warnings[0]
                .location
                .scope
                .to_path_buf(&messages.string_table)
        ),
        normalize_path(
            &fs::canonicalize(docs_dir.join("#page.bst")).expect("docs page should canonicalize")
        )
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
