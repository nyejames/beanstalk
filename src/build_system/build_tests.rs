//! Tests for the core build orchestration and output writer APIs.

use super::{
    FileKind, OutputFile, Project, ProjectBuilder, WriteOptions, build_project,
    write_project_outputs,
};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerError, CompilerMessages, ErrorLocation};
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_build_core_{prefix}_{unique}"))
}

struct CurrentDirGuard {
    previous: PathBuf,
}

impl CurrentDirGuard {
    fn set_to(path: &PathBuf) -> Self {
        let previous = std::env::current_dir().expect("current dir should resolve");
        std::env::set_current_dir(path).expect("should change current directory for test");
        Self { previous }
    }
}

impl Drop for CurrentDirGuard {
    fn drop(&mut self) {
        let _ = std::env::set_current_dir(&self.previous);
    }
}

struct WarningBuilder;

impl ProjectBuilder for WarningBuilder {
    fn build_backend(
        &self,
        _modules: Vec<super::Module>,
        _config: &Config,
        _flags: &[Flag],
    ) -> Result<Project, CompilerMessages> {
        Ok(Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("generated.js"),
                FileKind::Js(String::from("console.log('ok');")),
            )],
            warnings: vec![CompilerWarning::new(
                "builder warning",
                ErrorLocation::default(),
                WarningKind::UnusedVariable,
                PathBuf::from("builder"),
            )],
        })
    }

    fn validate_project_config(&self, _config: &Config) -> Result<(), CompilerError> {
        Ok(())
    }
}

#[test]
fn build_project_returns_result_without_writing_files() {
    let root = temp_dir("build_only");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("main.bst"), "value = 1\n").expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
    let result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

    assert!(!result.project.output_files.is_empty());
    assert!(
        !root.join("main.html").exists(),
        "build_project should not write files to disk"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn write_project_outputs_writes_all_supported_artifacts_and_skips_not_built() {
    let root = temp_dir("writer_success");
    fs::create_dir_all(&root).expect("should create temp root");

    let project = Project {
        output_files: vec![
            OutputFile::new(PathBuf::from("assets"), FileKind::Directory),
            OutputFile::new(
                PathBuf::from("scripts/app.js"),
                FileKind::Js(String::from("console.log('hi');")),
            ),
            OutputFile::new(
                PathBuf::from("index.html"),
                FileKind::Html(String::from("<html></html>")),
            ),
            OutputFile::new(PathBuf::from("bin/app.wasm"), FileKind::Wasm(vec![0, 1, 2])),
            OutputFile::new(PathBuf::new(), FileKind::NotBuilt),
        ],
        warnings: vec![],
    };

    write_project_outputs(
        &project,
        &WriteOptions {
            output_root: root.clone(),
        },
    )
    .expect("writer should succeed");

    assert!(root.join("assets").is_dir());
    assert_eq!(
        fs::read_to_string(root.join("scripts/app.js")).expect("should read JS file"),
        "console.log('hi');"
    );
    assert_eq!(
        fs::read_to_string(root.join("index.html")).expect("should read HTML file"),
        "<html></html>"
    );
    assert_eq!(
        fs::read(root.join("bin/app.wasm")).expect("should read wasm file"),
        vec![0, 1, 2]
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn write_project_outputs_rejects_invalid_paths() {
    let root = temp_dir("writer_invalid");
    fs::create_dir_all(&root).expect("should create temp root");

    let invalid_projects = vec![
        Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("/tmp/absolute.js"),
                FileKind::Js(String::from("x")),
            )],
            warnings: vec![],
        },
        Project {
            output_files: vec![OutputFile::new(
                PathBuf::from("../escape.js"),
                FileKind::Js(String::from("x")),
            )],
            warnings: vec![],
        },
        Project {
            output_files: vec![OutputFile::new(
                PathBuf::new(),
                FileKind::Js(String::from("x")),
            )],
            warnings: vec![],
        },
    ];

    for project in invalid_projects {
        let result = write_project_outputs(
            &project,
            &WriteOptions {
                output_root: root.clone(),
            },
        );
        assert!(result.is_err(), "invalid output path should be rejected");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_preserves_builder_warnings_in_build_result() {
    let root = temp_dir("warnings");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(root.join("main.bst"), "value = 1\n").expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let result = build_project(&WarningBuilder, "main.bst", &[]).expect("build should succeed");

    assert!(
        result
            .warnings
            .iter()
            .any(|warning| warning.msg == "builder warning"),
        "build result should include backend warnings"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_emits_runtime_fragment_with_captured_start_local() {
    let root = temp_dir("runtime_fragment_capture");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "get_name|| -> String:\n    return \"Beanstalk\"\n;\nname = get_name()\n[:Hello [name]]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
    let build_result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    assert!(html.contains("<div id=\"bst-slot-0\"></div>"));
    assert!(
        html.contains("__bst_frag_0"),
        "runtime fragment function should be emitted and bootstrapped"
    );
    assert!(
        html.contains("Beanstalk"),
        "captured start-local value should be preserved in generated fragment code"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_preserves_const_and_runtime_fragment_order() {
    let root = temp_dir("fragment_order");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#[:<meta charset=\"utf-8\">]\nname = \"Beanstalk\"\n[:<title>[name]</title>]\n",
    )
    .expect("should write source file");
    let _cwd_guard = CurrentDirGuard::set_to(&root);

    let builder = HtmlProjectBuilder::new();
    let build_result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

    let html = match build_result.project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        other => panic!(
            "expected HTML output, got {:?}",
            std::mem::discriminant(other)
        ),
    };

    let const_index = html
        .find("<meta charset=\"utf-8\">")
        .expect("const fragment should be inlined");
    let slot_index = html
        .find("<div id=\"bst-slot-0\"></div>")
        .expect("runtime fragment slot should be emitted");

    assert!(
        const_index < slot_index,
        "const fragment should appear before runtime slot in source order"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
