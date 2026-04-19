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
fn build_project_typed_constant_template_head_can_reference_prior_constant() {
    let root = temp_dir("typed_constant_template_head_reference");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "# page String = [: world]\n# test = [page: Hello ]\nio(test)\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("typed constant should remain visible to later constants");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("world Hello"),
            "typed constant reference in template head should compile and render expected output"
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_struct_template_field_can_fill_template_slots() {
    let root = temp_dir("const_struct_template_field_slots");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "Basic = |\n    page String = [:<section>[$slot]</section>],\n|\n#basic = Basic()\n#[basic.page: Hello world]\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("const struct template field should remain foldable in const template heads");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("<section>")
                && html.contains("Hello world")
                && html.contains("</section>"),
            "const struct wrapper field should compose slot content in place",
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_slot_insertion_constant_is_composed_at_use_site() {
    let root = temp_dir("const_slot_insertion_use_site");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#wrapper = [:<section>[$slot(\"content\")]</section>]\n#slot_1 = [$insert(\"content\"): Hello world]\n#[wrapper, slot_1]\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("slot insertion constants should fold when consumed by wrapper templates");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("<section>")
                && html.contains("Hello world")
                && html.contains("</section>"),
            "slot insertion constant should be resolved at the wrapper use-site",
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_top_level_header_with_unfilled_named_slots_folds_to_empty_strings() {
    let root = temp_dir("const_top_level_header_unfilled_named_slots");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        r#"Document = |
    prelude String = "<!DOCTYPE html>",
    en String = [$html:<html lang="en">],
    head String = [$html:
        <head>[$slot]</head>
    ],
    title String = [$html:<title>[$slot]</title>],
    style String = [$html:<style>[$slot]</style>],
|
#doc = Document()

# header = [:
    [doc.prelude, doc.en]
    [doc.head, $html:
        <meta charset="UTF-8">
        <link rel="icon" href="[$slot("favicon")]">
        [doc.title: Beanstalk Documentation]
        [doc.style:
            [$slot("css")]
        ]
    ]
]
#[header]
"#,
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[]);
        if let Err(ref e) = build_result {
            panic!(
                "top-level const wrappers should fold even when named slots are unfilled: {}",
                e.errors
                    .iter()
                    .map(|e| e.msg.as_str())
                    .collect::<Vec<_>>()
                    .join("; ")
            );
        }
        let build_result = build_result.unwrap();

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };
        assert!(
            html.contains("rel=\"icon\"") && html.contains("href=\"\""),
            "unfilled named slots should render as empty strings instead of failing compile-time folding",
        );
        assert!(
            html.contains("<meta charset=\"UTF-8\">"),
            "expected folded header content to remain present in output",
        );
        assert!(
            !html.contains("$slot(") && !html.contains("$insert("),
            "slot markers should not leak into folded output",
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_rejects_slot_insertion_constant_without_active_wrapper() {
    let root = temp_dir("const_slot_insertion_without_wrapper");
    fs::create_dir_all(&root).expect("should create temp root");
    fs::write(
        root.join("main.bst"),
        "#slot_1 = [$insert(\"content\"): hello]\n#[slot_1]\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let result = build_project(&builder, "main.bst", &[]);
        assert!(
            result.is_err(),
            "slot insertion constants should fail when used outside wrapper composition",
        );
        let Err(messages) = result else {
            unreachable!("assert above guarantees this is an error");
        };

        assert!(
            messages.errors.iter().any(|error| error.msg.contains(
                "'$insert(...)' can only be used while filling an immediate parent template"
            )),
            "expected a targeted slot insertion usage diagnostic",
        );
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_const_slot_children_wrap_table_rows_and_cells_without_cross_applying() {
    let root = temp_dir("const_slot_children_cells");
    fs::create_dir_all(root.join("libs")).expect("should create libs root");
    fs::write(
        root.join("libs").join("html.bst"),
        "#table = [$children([:<tr>[$slot]</tr>]):\n  <table>\n    [$children([:<td>[$slot]</td>]):[$slot]]\n  </table>\n]\n",
    )
    .expect("should write html library");
    fs::write(
        root.join("main.bst"),
        "import @libs/html {table}\n[table:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);
        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("slot child wrapper tables should build successfully");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };

        assert!(html.contains("bst-slot-0"));
        assert_eq!(html.matches("<tr>").count(), 2);
        assert!(html.contains("<td>Type"));
        assert!(html.contains("<td>Description"));
        assert!(html.contains("<td>float"));
        assert_eq!(html.matches("<td>").count(), 4);
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_markdown_page_reexported_table_keeps_rows_and_cells_inside_table() {
    let root = temp_dir("markdown_page_reexported_table");
    fs::create_dir_all(root.join("libs")).expect("should create libs root");
    fs::create_dir_all(root.join("styles")).expect("should create styles root");
    fs::write(
        root.join("libs").join("html.bst"),
        "Format = |\n  table String = [$children([:<tr>[$slot]</tr>]):\n    <table style=\"[$slot(\"style\")]\">\n      [$children([:<td>[$slot]</td>]):[$slot]]\n    </table>\n  ],\n|\n#format = Format()\n",
    )
    .expect("should write html library");
    fs::write(
        root.join("styles").join("docs.bst"),
        "import @libs/html {format}\n#page = [:\n  <body>[$slot]</body>\n]\n#table = [format.table]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("main.bst"),
        "import @styles/docs {page, table}\n[page, $markdown:\n[table:\n    [: [:Type] [:Description] ]\n    [: [:float] [:64 bit floating point number] ]\n]\n]\n",
    )
    .expect("should write source file");

    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("markdown page with re-exported table should build successfully");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };

        assert!(!html.contains('\u{FFFC}'));
        assert!(html.contains("bst-slot-0"));
        assert_eq!(html.matches("<tr>").count(), 2);
        assert!(html.contains("<td>Type"));
        assert!(html.contains("<td>Description"));
        assert_eq!(html.matches("<td>").count(), 4);
        assert!(!html.contains("<p>"));
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_docs_style_title_and_center_slot_chain_compiles() {
    let root = temp_dir("docs_title_center_slot_chain");
    fs::create_dir_all(root.join("lib")).expect("should create lib root");
    fs::create_dir_all(root.join("src/styles")).expect("should create styles root");

    fs::write(
        root.join("#config.bst"),
        "#project = \"html\"\n#entry_root = \"src\"\n#output_folder = \"release\"\n#root_folders = {\n    @lib,\n}\n",
    )
    .expect("should write config file");
    fs::write(
        root.join("lib").join("html.bst"),
        "#center String = [$insert(\"style\"):text-align: center;]\n",
    )
    .expect("should write html helper library");
    fs::write(
        root.join("src/styles").join("docs.bst"),
        "#title = [$html: <h1 style=\"font-size: 2em;[$slot(\"style\")]\">[$slot]</h1>]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("src").join("#page.bst"),
        "import @lib/html {center}\nimport @styles/docs {title}\n#[title, center: LANGUAGE BASICS]\n",
    )
    .expect("should write source file");

    let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
    let build_result = build_project(&builder, &root.to_string_lossy(), &[])
        .expect("docs-style title+center chain should compile successfully");

    // Find the generated route HTML so assertions stay stable even if file ordering changes.
    let html = build_result
        .project
        .output_files
        .iter()
        .find_map(|output| match output.file_kind() {
            FileKind::Html(content)
                if output.relative_output_path().to_string_lossy() == "index.html" =>
            {
                Some(content)
            }
            _ => None,
        })
        .expect("should emit index.html output");

    assert!(html.contains("text-align: center;"));
    assert!(html.contains("LANGUAGE BASICS"));
    assert!(!html.contains("$slot("));
    assert!(!html.contains("$insert("));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_project_markdown_docs_row_wrappers_render_plain_cells_and_headers() {
    let root = temp_dir("markdown_docs_row_wrappers");
    fs::create_dir_all(root.join("libs")).expect("should create libs root");
    fs::create_dir_all(root.join("styles")).expect("should create styles root");
    fs::write(
        root.join("libs").join("html.bst"),
        "Format = |\n    table String = [:\n      <table style=\"[$slot(\"style\")]\">\n        [$slot]\n      </table>\n    ],\n|\n#format = Format()\n",
    )
    .expect("should write html library");
    fs::write(
        root.join("styles").join("docs.bst"),
        "import @libs/html {format}\n#page = [:\n  <body>[$slot]</body>\n]\n#table = [format.table:\n    [$insert(\"style\"):border-collapse: collapse; border: 1px solid; padding: 0.5em;]\n    [$slot]\n]\n#row = [:\n    <tr>[$fresh, $children([:<td>[$slot]</td>]):[$slot]]</tr>\n]\n#header_row = [:\n    <tr>\n        [$fresh, $children([:\n            <th style=\"border: 1px solid; padding: 0.5em; text-align: left;\">[$slot]</th>\n        ]):[$slot]]\n    </tr>\n]\n",
    )
    .expect("should write docs style library");
    fs::write(
        root.join("main.bst"),
        "import @styles/docs {page, table, row, header_row}\n[page, $markdown:\n[table:\n    [header_row: [: Type] [: Description] ]\n\n    [row: [: float ] [: 64 bit floating point number] ]\n\n    [row: [: int ] [:  64 bit signed integer ] ]\n]\n]\n",
    )
    .expect("should write source file");
    {
        let _cwd_guard = CurrentDirGuard::set_to(&root);

        let builder = ProjectBuilder::new(Box::new(HtmlProjectBuilder::new()));
        let build_result = build_project(&builder, "main.bst", &[])
            .expect("markdown docs-style row wrappers should build successfully");

        let html = match build_result.project.output_files[0].file_kind() {
            FileKind::Html(content) => content,
            other => panic!(
                "expected HTML output, got {:?}",
                std::mem::discriminant(other)
            ),
        };

        assert!(!html.contains('\u{FFFC}'));
        assert!(html.contains("bst-slot-0"));
        assert!(html.contains("border-collapse: collapse; border: 1px solid; padding: 0.5em;"));
        assert_eq!(html.matches("<th style=").count(), 2);
        assert_eq!(html.matches("<td>").count(), 4);
        assert!(html.contains("Type"));
        assert!(html.contains("Description"));
        assert!(html.contains("float"));
        assert!(html.contains("64 bit floating point number"));
        assert!(!html.contains("<p>"));
    }

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

use crate::compiler_tests::test_support::temp_dir;
