//! Tests for the direct HTML-project Beandown API.
//!
//! WHAT: covers input normalization, AST-only compilation, ordering, duplicate diagnostics, and
//! the deferred caller-supplied scope boundary.
//! WHY: this API is intentionally not wired into project builds yet, so module-local tests protect
//! the tooling-facing boundary without adding integration artifacts.

use crate::compiler_frontend::compiler_messages::{
    DiagnosticKind, DiagnosticPayload, ImportDiagnosticKind,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::beandown::{
    BeandownCompileRequest, BeandownInput, BeandownPathScope, BeandownScopeConstant,
    BeandownSource, compile_beandown,
};
use std::fs;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

fn request(input: BeandownInput) -> BeandownCompileRequest {
    BeandownCompileRequest {
        input,
        default_module_constants: Vec::new(),
        module_constants_by_path: Vec::new(),
    }
}

fn temp_project(files: &[(&str, &str)]) -> TempDir {
    let temp_dir = tempfile::tempdir().expect("temp project should be created");
    for (relative_path, source) in files {
        let path = temp_dir.path().join(relative_path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).expect("source parent should be created");
        }
        fs::write(path, source).expect("source should be written");
    }

    temp_dir
}

fn compile_ok(
    input: BeandownInput,
) -> crate::projects::html_project::beandown::BeandownCompileOutput {
    let mut string_table = StringTable::new();
    compile_beandown(request(input), &mut string_table).expect("Beandown input should compile")
}

#[test]
fn file_input_compiles_one_beandown_file() {
    let temp_dir = temp_project(&[("intro.bd", "# Intro")]);
    let source_path = temp_dir.path().join("intro.bd");

    let output = compile_ok(BeandownInput::File(source_path.clone()));

    assert_eq!(output.documents.len(), 1);
    assert_eq!(output.documents[0].content, "<h1>Intro</h1>");
    assert_eq!(output.documents[0].relative_path, None);
    assert_eq!(
        output.documents[0].source_path,
        fs::canonicalize(source_path).expect("source path should canonicalize")
    );
}

#[test]
fn direct_directory_input_compiles_direct_child_beandown_files_sorted_by_relative_path() {
    let temp_dir = temp_project(&[
        ("docs/z-last.bd", "z"),
        ("docs/a-first.bd", "a"),
        ("docs/nested/ignored.bd", "nested"),
        ("docs/readme.txt", "ignored"),
    ]);

    let output = compile_ok(BeandownInput::Directory {
        path: temp_dir.path().join("docs"),
        recursive: false,
    });

    let relative_paths = output
        .documents
        .iter()
        .map(|document| document.relative_path.as_deref())
        .collect::<Vec<_>>();

    assert_eq!(
        relative_paths,
        vec![Some(Path::new("a-first.bd")), Some(Path::new("z-last.bd"))]
    );
    assert_eq!(
        output
            .documents
            .iter()
            .map(|document| document.content.as_str())
            .collect::<Vec<_>>(),
        vec!["<p>a</p>", "<p>z</p>"]
    );
}

#[test]
fn recursive_directory_input_compiles_descendant_beandown_files() {
    let temp_dir = temp_project(&[
        ("docs/index.bd", "index"),
        ("docs/nested/detail.bd", "detail"),
    ]);

    let output = compile_ok(BeandownInput::Directory {
        path: temp_dir.path().join("docs"),
        recursive: true,
    });

    assert_eq!(
        output
            .documents
            .iter()
            .map(|document| document.relative_path.as_deref())
            .collect::<Vec<_>>(),
        vec![
            Some(Path::new("index.bd")),
            Some(Path::new("nested/detail.bd"))
        ]
    );
}

#[test]
fn explicit_file_list_preserves_caller_order() {
    let temp_dir = temp_project(&[("first.bd", "first"), ("second.bd", "second")]);

    let output = compile_ok(BeandownInput::Files(vec![
        temp_dir.path().join("second.bd"),
        temp_dir.path().join("first.bd"),
    ]));

    assert_eq!(
        output
            .documents
            .iter()
            .map(|document| document.content.as_str())
            .collect::<Vec<_>>(),
        vec!["<p>second</p>", "<p>first</p>"]
    );
    assert!(
        output
            .documents
            .iter()
            .all(|document| document.relative_path.is_none())
    );
}

#[test]
fn in_memory_sources_compile_without_filesystem_output() {
    let output = compile_ok(BeandownInput::Sources(vec![BeandownSource {
        display_path: PathBuf::from("memory/intro.bd"),
        source_text: "[:nested]".to_owned(),
    }]));

    assert_eq!(output.documents.len(), 1);
    assert_eq!(output.documents[0].content, "<p>nested</p>");
    assert_eq!(
        output.documents[0].source_path,
        PathBuf::from("memory/intro.bd")
    );
    assert_eq!(output.documents[0].relative_path, None);
}

#[test]
fn duplicate_source_paths_are_diagnostics() {
    let temp_dir = temp_project(&[("intro.bd", "intro")]);
    let path = temp_dir.path().join("intro.bd");
    let mut string_table = StringTable::new();

    let messages = compile_beandown(
        request(BeandownInput::Files(vec![path.clone(), path])),
        &mut string_table,
    )
    .expect_err("duplicate input paths should fail");

    assert_eq!(messages.error_count(), 1);
    let diagnostic = messages
        .diagnostics()
        .next()
        .expect("duplicate diagnostic should be present");
    assert!(matches!(
        diagnostic.kind,
        DiagnosticKind::Import(ImportDiagnosticKind::DuplicateBeandownInputPath)
    ));
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::DuplicateBeandownInputPath { .. }
    ));
}

#[test]
fn compile_api_does_not_write_artifacts() {
    let temp_dir = temp_project(&[("intro.bd", "intro")]);
    let before = directory_entries(temp_dir.path());

    let _output = compile_ok(BeandownInput::File(temp_dir.path().join("intro.bd")));

    assert_eq!(directory_entries(temp_dir.path()), before);
}

#[test]
fn caller_supplied_scope_constants_are_deferred_without_exposing_internals() {
    let temp_dir = temp_project(&[("intro.bd", "intro")]);
    let mut string_table = StringTable::new();
    let request = BeandownCompileRequest {
        input: BeandownInput::File(temp_dir.path().join("intro.bd")),
        default_module_constants: vec![BeandownScopeConstant::test_placeholder()],
        module_constants_by_path: vec![BeandownPathScope {
            source_path: temp_dir.path().join("intro.bd"),
            constants: Vec::new(),
        }],
    };

    let messages = compile_beandown(request, &mut string_table)
        .expect_err("caller-supplied scope constants are intentionally unsupported in this slice");

    assert_eq!(messages.error_count(), 1);
    let diagnostic = messages
        .diagnostics()
        .next()
        .expect("scope diagnostic should be present");
    assert!(matches!(
        diagnostic.kind,
        DiagnosticKind::Import(ImportDiagnosticKind::InvalidBeandownApiScopeItem)
    ));
    assert!(matches!(
        diagnostic.payload,
        DiagnosticPayload::InvalidBeandownApiScopeItem { .. }
    ));
}

fn directory_entries(path: &Path) -> Vec<PathBuf> {
    let mut entries = fs::read_dir(path)
        .expect("directory should be readable")
        .map(|entry| {
            entry
                .expect("directory entry should be readable")
                .path()
                .strip_prefix(path)
                .expect("entry should be under directory")
                .to_path_buf()
        })
        .collect::<Vec<_>>();
    entries.sort();
    entries
}

// -----------------------------------------------------------------------------
// Beandown direct API TIR-backed behavior tests
// -----------------------------------------------------------------------------

#[test]
fn file_input_compiles_bd_markdown_with_nested_template() {
    let temp_dir = temp_project(&[("intro.bd", "# [:title]")]);

    let output = compile_ok(BeandownInput::File(temp_dir.path().join("intro.bd")));

    assert_eq!(output.documents.len(), 1);
    assert_eq!(output.documents[0].content, "<h1><p>title</p></h1>");
}

#[test]
fn source_input_compiles_bd_nested_authored_template() {
    let output = compile_ok(BeandownInput::Sources(vec![BeandownSource {
        display_path: PathBuf::from("memory/nested.bd"),
        source_text: "[:# Nested]".to_owned(),
    }]));

    assert_eq!(output.documents.len(), 1);
    assert_eq!(output.documents[0].content, "<h1>Nested</h1>");
}

#[test]
fn source_input_nested_raw_directive_overrides_bd_markdown_default() {
    let output = compile_ok(BeandownInput::Sources(vec![BeandownSource {
        display_path: PathBuf::from("memory/raw-nested.bd"),
        source_text: "[$raw:# Nested]".to_owned(),
    }]));

    assert_eq!(output.documents.len(), 1);
    assert_eq!(output.documents[0].content, "# Nested");
}

#[test]
fn source_input_nested_non_formatter_directive_overrides_bd_markdown_default() {
    let output = compile_ok(BeandownInput::Sources(vec![BeandownSource {
        display_path: PathBuf::from("memory/fresh-nested.bd"),
        source_text: "[$fresh:# Nested]".to_owned(),
    }]));

    assert_eq!(output.documents.len(), 1);
    assert_eq!(output.documents[0].content, "# Nested");
}
