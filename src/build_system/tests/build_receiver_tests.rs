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
fn build_project_lowers_same_file_receiver_method_calls() {
    let root = temp_dir("receiver_method_lowered");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Vector2 = |\n    x Float = 0.0,\n    y Float = 0.0,\n|\n\nreset |this ~Vector2|:\n    this.x = 0.0\n    this.y = 0.0\n;\n\nvec ~= Vector2(12.0, 87.0)\n~vec.reset()\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        build_project(&builder, "main.bst", &[])
            .expect("receiver method calls should lower through the shared call pipeline");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_non_exported_receiver_methods_across_files() {
    let root = temp_dir("receiver_method_non_exported_cross_file");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @math/Counter\ncounter ~= Counter()\nio(counter.increment())\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("math.bst"),
        "#Counter = |\n    value Int = 0,\n|\n\nincrement |this Counter| -> Int:\n    return this.value + 1\n;\n",
    )
    .expect("should write math source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let Err(messages) = build_project(&builder, "main.bst", &[]) else {
            panic!("non-exported cross-file receiver method should not be visible");
        };

        assert!(
            messages.errors.iter().any(|error| error
                .msg
                .contains("Property or method 'increment' not found")),
            "expected missing receiver-method visibility error for non-exported method"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_allows_exported_receiver_methods_across_files() {
    let root = temp_dir("receiver_method_exported_cross_file");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @math/Counter\ncounter ~= Counter()\nio(counter.increment())\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("math.bst"),
        "#Counter = |\n    value Int = 0,\n|\n\n#increment |this Counter| -> Int:\n    return this.value + 1\n;\n",
    )
    .expect("should write math source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        build_project(&builder, "main.bst", &[])
            .expect("exported cross-file receiver method should be visible");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_lowers_builtin_scalar_receiver_methods() {
    let root = temp_dir("builtin_scalar_receiver_method_lowered");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "double |this Int| -> Int:\n    return this + this\n;\n\nvalue = 21\nio(value.double())\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        build_project(&builder, "main.bst", &[])
            .expect("builtin scalar receiver method calls should lower end-to-end");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_free_function_receiver_diagnostic_is_deterministic_across_modules() {
    let root = temp_dir("receiver_method_free_function_diagnostic_deterministic");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @beta/Beta\nimport @alpha/Alpha\n\nping(0)\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("alpha.bst"),
        "#Alpha = |\n    value Int = 0,\n|\n\n#ping |this Alpha| -> Int:\n    return this.value\n;\n",
    )
    .expect("should write alpha source file");
    fs::write(
        root.join("beta.bst"),
        "#Beta = |\n    value Int = 0,\n|\n\n#ping |this Beta| -> Int:\n    return this.value\n;\n",
    )
    .expect("should write beta source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let Err(messages) = build_project(&builder, "main.bst", &[]) else {
            panic!("free-function receiver misuse should fail");
        };

        let misuse_error = messages
            .errors
            .iter()
            .find(|error| error.msg.contains("cannot be called as a free function"))
            .expect("expected free-function receiver misuse diagnostic");

        assert!(
            misuse_error.msg.contains("'ping' is a receiver method"),
            "{}",
            misuse_error.msg
        );
        assert!(
            misuse_error.msg.contains("for 'alpha.bst/Alpha'"),
            "{}",
            misuse_error.msg
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
