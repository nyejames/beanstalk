//! Tests for the core build orchestration and output writer APIs.
// NOTE: temp file creation processes have to be explicitly dropped
// Or these tests will fail on Windows due to attempts to delete non-empty temp directories while files are still open.

use super::*;
use crate::build_system::build::{ProjectBuilder, build_project};
use crate::compiler_frontend::compiler_messages::{DiagnosticPayload, InvalidReceiverCallReason};
use crate::projects::html_project::html_project_builder::HtmlProjectBuilder;
use std::fs;

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
fn build_project_allows_cross_file_receiver_methods_by_default() {
    let root = temp_dir("receiver_method_cross_file_default_visible");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @math { Counter }\ncounter ~= Counter()\nio(counter.increment())\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("math.bst"),
        "Counter = |\n    value Int = 0,\n|\n\nincrement |this Counter| -> Int:\n    return this.value + 1\n;\n",
    )
    .expect("should write math source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        build_project(&builder, "main.bst", &[])
            .expect("same-module receiver methods should be importable without the old # prefix");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_allows_cross_file_receiver_methods_with_tight_signature_spacing() {
    let root = temp_dir("receiver_method_cross_file_tight_signature");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @math { Counter }\ncounter ~= Counter()\nio(counter.increment())\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("math.bst"),
        "Counter = |\n    value Int = 0,\n|\n\nincrement|this Counter| -> Int:\n    return this.value + 1\n;\n",
    )
    .expect("should write math source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        build_project(&builder, "main.bst", &[])
            .expect("receiver method signatures should parse with or without name/bracket spacing");
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
        "import @beta { Beta }\nimport @alpha { Alpha }\n\nping(0)\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("alpha.bst"),
        "Alpha = |\n    value Int = 0,\n|\n\nping|this Alpha| -> Int:\n    return this.value\n;\n",
    )
    .expect("should write alpha source file");
    fs::write(
        root.join("beta.bst"),
        "Beta = |\n    value Int = 0,\n|\n\nping|this Beta| -> Int:\n    return this.value\n;\n",
    )
    .expect("should write beta source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let Err(messages) = build_project(&builder, "main.bst", &[]) else {
            panic!("free-function receiver misuse should fail");
        };

        let misuse_diagnostic = messages
            .error_diagnostics()
            .find(|diagnostic| {
                matches!(
                    &diagnostic.payload,
                    DiagnosticPayload::InvalidReceiverCall {
                        reason: InvalidReceiverCallReason::CalledAsFreeFunction,
                        method_name: Some(method_name),
                        ..
                    } if messages.string_table.resolve(*method_name) == "ping"
                )
            })
            .expect("expected free-function receiver misuse diagnostic");

        assert!(
            matches!(
                &misuse_diagnostic.payload,
                DiagnosticPayload::InvalidReceiverCall { .. }
            ),
            "unexpected diagnostic payload: {:?}",
            misuse_diagnostic.payload
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn builds_mutable_receiver_method_across_files_by_default() {
    let root = temp_dir("receiver_method_mutable_cross_file");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "import @math { Resettable, reset }\n\nr ~= Resettable(42)\n~r.reset()\nio(r.value)\n",
    )
    .expect("should write main source file");
    fs::write(
        root.join("math.bst"),
        "Resettable = |\n    value Int = 0,\n|\n\nreset|this ~Resettable|:\n    this.value = 0\n;\n",
    )
    .expect("should write math source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        build_project(&builder, "main.bst", &[])
            .expect("mutable receiver methods should be importable without the old # prefix");
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
