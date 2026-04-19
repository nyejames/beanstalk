//! Tests for the core build orchestration and output writer APIs.
// NOTE: temp file creation processes have to be explicitly dropped
// Or these tests will fail on Windows due to attempts to delete non-empty temp directories while files are still open.

use super::*;
use crate::build_system::build::{
    BackendBuilder, CleanupPolicy, FileKind, OutputFile, Project, ProjectBuilder, WriteMode,
    WriteOptions, build_project, resolve_project_output_root,
    write_project_outputs as write_project_outputs_with_table,
};
use crate::build_system::output_cleanup::{
    BuilderKind, ManifestLimitedSafeModeReason, ManifestLoadResult,
};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::basic_utility_functions::normalize_path;
use crate::compiler_frontend::compiler_errors::{
    CompilerError, CompilerMessages, ErrorType, SourceLocation,
};
use crate::compiler_frontend::compiler_messages::display_messages::resolve_source_file_path;
use crate::compiler_frontend::compiler_warnings::{CompilerWarning, WarningKind};
use crate::compiler_frontend::style_directives::StyleDirectiveSpec;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;
use std::sync::{Mutex, MutexGuard, OnceLock};
use std::thread;
use std::time::{Duration, SystemTime};

#[test]
fn build_project_allows_const_record_coercion_with_all_defaults() {
    let root = temp_dir("const_record_all_defaults");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"ok\",\n|\n#basic = Basic()\n",
    )
    .expect("should write source file");

    // Explictly dropping the build result here to ensure all references to builder are released before we remove the temp dir
    // This fixed a mutex poisoning bug specifically on Windows
    {
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));

        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let result = build_project(&builder, "main.bst", &[]);
        assert!(
            result.is_ok(),
            "const struct coercion with defaults should compile"
        );
    } // everything dropped here

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_allows_const_record_coercion_with_constant_arguments() {
    let root = temp_dir("const_record_constant_args");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"default\",\n    color String = \"red\",\n|\n#label = \"Docs\"\n#basic = Basic(label, \"green\")\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(&builder, "main.bst", &[]);
        assert!(
            result.is_ok(),
            "const struct coercion with constant arguments should compile"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_runtime_struct_constructor_supports_partial_defaults() {
    let root = temp_dir("runtime_struct_partial_defaults");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Point = |\n    x Int,\n    y Int = 99,\n|\npoint = Point(5)\nio([: point.y])\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("runtime struct constructor with defaults should compile");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("99"),
            "runtime constructor should include the struct default value in emitted output"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_struct_default_uses_same_file_constant_declared_later() {
    let root = temp_dir("struct_default_forward_constant");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Card = |\n    color String = base + \"!\",\n|\n#base = \"red\"\ncard = Card()\nio([: card.color])\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("struct default should resolve same-file constants declared later");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("red!"),
            "forward constant dependency should be sorted before struct parsing and fold into one value",
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_constant_can_reference_same_file_struct_declared_later() {
    let root = temp_dir("const_depends_on_forward_struct");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#basic = Basic()\nBasic = |\n    body String = \"ok\",\n|\nio([: basic.body])\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("constant should resolve same-file struct declared later");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            !html.is_empty(),
            "build output should still be produced when constant references forward-declared struct"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_struct_default_uses_imported_constant() {
    let root = temp_dir("struct_default_imported_constant");
    fs::create_dir_all(root.join("styles")).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @styles/theme/base\nCard = |\n    color String = base,\n|\ncard = Card()\nio([: card.color])\n",
    )
    .expect("should write main source file");
    fs::write(root.join("styles/theme.bst"), "#base = \"green\"\n")
        .expect("should write imported constant source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("struct default should resolve imported constants");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("green"),
            "imported constant should be available in struct default value resolution",
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_const_record_with_non_constant_argument() {
    let root = temp_dir("const_record_non_constant_arg");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String = \"ok\",\n|\nget_value || -> String:\n    return \"dynamic\"\n;\n#basic = Basic(get_value())\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(&builder, "main.bst", &[]);
        assert!(
            result.is_err(),
            "non-constant struct constructor argument in '#'-constant should fail"
        );
        let Err(messages) = result else {
            unreachable!("assert above guarantees this is an error");
        };

        assert!(
            messages.errors.iter().any(|error| {
                error.msg.contains("get_value") && error.msg.contains("non-constant value")
            }),
            "expected a targeted error describing the non-constant argument"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_const_record_when_required_fields_are_missing() {
    let root = temp_dir("const_record_missing_required");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    body String,\n    color String = \"blue\",\n|\n#basic = Basic()\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(&builder, "main.bst", &[]);
        assert!(
            result.is_err(),
            "missing required fields in const record constructor should fail"
        );
        let Err(messages) = result else {
            unreachable!("assert above guarantees this is an error");
        };

        assert!(
            messages
                .errors
                .iter()
                .any(|error| error.msg.contains("Missing required argument for field")),
            "expected a missing-required-fields constructor diagnostic"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_struct_constructor_with_too_many_arguments() {
    let root = temp_dir("struct_constructor_too_many_args");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Point = |\n    x Int,\n    y Int = 1,\n|\n#point = Point(1, 2, 3)\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(&builder, "main.bst", &[]);
        assert!(
            result.is_err(),
            "too many struct constructor arguments should fail"
        );
        let Err(messages) = result else {
            unreachable!("assert above guarantees this is an error");
        };

        assert!(
            messages.errors.iter().any(|error| error
                .msg
                .contains("extra positional arguments were provided")),
            "expected a too-many-arguments constructor diagnostic"
        );
    }
    fs::remove_dir_all(&root).expect("should remove temp dir");
}

// ---------------------------------------------------------------------------

use crate::compiler_tests::test_support::temp_dir;
