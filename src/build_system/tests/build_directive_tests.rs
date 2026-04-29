//! Tests for the core build orchestration and output writer APIs.
// NOTE: temp file creation processes have to be explicitly dropped
// Or these tests will fail on Windows due to attempts to delete non-empty temp directories while files are still open.

use super::*;
use crate::build_system::build::{FileKind, ProjectBuilder, build_project};
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use std::fs;
use std::path::PathBuf;

#[test]
fn html_project_directives_fail_when_builder_does_not_register_them() {
    let root = temp_dir("directive_boundary_missing");
    fs::create_dir_all(&root).expect("should create temp root");

    for (directive_name, source) in [
        ("html", "[$html:\n<div>Hello</div>\n]"),
        ("css", "[$css:\n.button { color: red; }\n]"),
        ("escape_html", "[$escape_html:\n<b>Hello</b>\n]"),
    ] {
        let entry_file = root.join(format!("{directive_name}.bst"));
        fs::write(&entry_file, source).expect("should write source file");

        let builder = ProjectBuilder::new(Box::new(NoDirectiveBuilder));
        let result = build_project(
            &builder,
            entry_file
                .to_str()
                .expect("temp file path should be valid UTF-8 for this test"),
            &[],
        );

        let Err(messages) = result else {
            panic!("project-owned directives should fail when not registered");
        };
        assert!(
            messages
                .errors
                .iter()
                .any(|error| error.msg.contains(&format!(
                    "Style directive '${directive_name}' is unsupported here"
                ))),
            "expected unsupported directive error for '${directive_name}', got: {:?}",
            messages
                .errors
                .iter()
                .map(|error| error.msg.as_str())
                .collect::<Vec<_>>()
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn frontend_builtin_directives_work_without_builder_registered_project_directives() {
    let root = temp_dir("frontend_builtin_boundary");
    fs::create_dir_all(&root).expect("should create temp root");

    let entry_file = root.join("builtins.bst");
    fs::write(
        &entry_file,
        "[$children([:<li>[$slot]</li>]):\n<ul>\n  [$markdown:\n# Docs\n]\n  [$raw:\n  keep\n]\n  [$fresh:\n    [: plain ]\n  ]\n</ul>\n]",
    )
    .expect("should write source file");

    let builder = ProjectBuilder::new(Box::new(NoDirectiveBuilder));
    let result = build_project(
        &builder,
        entry_file
            .to_str()
            .expect("temp file path should be valid UTF-8 for this test"),
        &[],
    )
    .expect("frontend built-ins should compile without project-owned style registrations");

    assert_eq!(result.project.output_files.len(), 1);
    assert_eq!(
        result.project.output_files[0].relative_output_path(),
        PathBuf::from("index.html")
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn html_project_directives_are_available_under_html_builder() {
    let root = temp_dir("directive_boundary_registered");

    fs::create_dir_all(&root).expect("should create temp root");

    for (directive_name, source, expected_html_fragment) in [
        ("html", "[$html:\n<div>Hello</div>\n]", "<div>Hello</div>"),
        (
            "css",
            "[$css:\n.button { color: red; }\n]",
            ".button { color: red; }",
        ),
        (
            "escape_html",
            "[$escape_html:\n<b>Hello</b>\n]",
            "&lt;b&gt;Hello&lt;/b&gt;",
        ),
    ] {
        let entry_file = root.join(format!("{directive_name}.bst"));
        fs::write(&entry_file, source).expect("should write source file");

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(
            &builder,
            entry_file
                .to_str()
                .expect("temp file path should be valid UTF-8 for this test"),
            &[],
        )
        .expect("html builder should register HTML-project directives");

        let rendered_html = result
            .project
            .output_files
            .iter()
            .find_map(|file| match file.file_kind() {
                FileKind::Html(content) => Some(content.as_str()),
                _ => None,
            })
            .expect("expected an HTML output file");
        let escaped_runtime_fragment = expected_html_fragment.replace("</", "<\\/");

        assert!(
            rendered_html.contains("bst-slot-0"),
            "expected rendered HTML for '${directive_name}' to include a runtime slot placeholder, got: {rendered_html}"
        );
        assert!(
            rendered_html.contains(expected_html_fragment)
                || rendered_html.contains(&escaped_runtime_fragment),
            "expected rendered HTML for '${directive_name}' to contain '{expected_html_fragment}', got: {rendered_html}"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
