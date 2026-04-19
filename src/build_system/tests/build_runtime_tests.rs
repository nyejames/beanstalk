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
fn build_project_emits_runtime_fragment_with_captured_start_local() {
    let root = temp_dir("runtime_fragment_capture");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "get_name|| -> String:\n    return \"Beanstalk\"\n;\nname = get_name()\n[:Hello [name]]\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[]).expect("build should succeed");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };

        assert!(html.contains("<div id=\"bst-slot-0\"></div>"));
        // WHY: new architecture calls start() once and hydrates slots from the returned array.
        //      No per-fragment wrapper functions (__bst_frag_N) are emitted.
        assert!(
            html.contains("bst_frags"),
            "bootstrap must use start() result array to hydrate slots"
        );
        assert!(
            html.contains("insertAdjacentHTML"),
            "bootstrap must hydrate runtime slots via insertAdjacentHTML"
        );
        assert!(
            html.contains("Beanstalk"),
            "captured start-local value should be preserved in generated fragment code"
        );
    }

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

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
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
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
