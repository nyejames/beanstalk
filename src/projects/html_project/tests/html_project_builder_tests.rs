//! Tests for HTML project builder orchestration.

use super::*;
use crate::backends::js::test_symbol_helpers::expected_dev_function_name;
use crate::build_system::build::FileKind;
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::hir::hir_nodes::{ConstStringId, FunctionId, StartFragment};
use crate::projects::html_project::tests::test_support::{
    assert_fragment_before_body_close, assert_has_basic_shell, collect_output_paths,
    create_test_module, expect_html_output, expect_js_output, temp_dir,
};
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;

#[test]
fn build_backend_emits_single_html_output_file() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let config = Config::new(entry_path.clone());

    let project = builder
        .build_backend(vec![create_test_module(entry_path)], &config, &[])
        .expect("build_backend should succeed");

    assert_eq!(project.output_files.len(), 1);
    assert_eq!(
        project.output_files[0].relative_output_path(),
        PathBuf::from("index.html")
    );
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));
    assert!(matches!(
        project.output_files[0].file_kind(),
        FileKind::Html(_)
    ));
}

#[test]
fn build_backend_respects_release_pretty_toggle() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");

    let dev_project = builder
        .build_backend(
            vec![create_test_module(entry_path.clone())],
            &Config::new(entry_path.clone()),
            &[],
        )
        .expect("dev build should succeed");
    let release_project = builder
        .build_backend(
            vec![create_test_module(entry_path.clone())],
            &Config::new(entry_path),
            &[Flag::Release],
        )
        .expect("release build should succeed");

    let dev_html = expect_html_output(&dev_project.output_files, "index.html");
    let release_html = expect_html_output(&release_project.output_files, "index.html");

    assert!(
        dev_html.contains("\n        return;\n"),
        "dev build should include pretty indentation for statements"
    );
    assert!(
        release_html.contains("return;"),
        "release build should still emit valid JS statements"
    );
    assert!(
        !release_html.contains("\n        return;\n"),
        "release build should avoid pretty indentation"
    );
}

#[test]
fn hash_prefixed_route_name_strips_hash_from_output() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#404.bst");
    let config = Config::new(entry_path.clone());

    let project = builder
        .build_backend(vec![create_test_module(entry_path)], &config, &[])
        .expect("build_backend should succeed");

    assert_eq!(
        project.output_files[0].relative_output_path(),
        PathBuf::from("404.html")
    );
}

#[test]
fn build_backend_emits_html_for_multiple_modules() {
    let builder = HtmlProjectBuilder::new();
    let config = Config::new(PathBuf::from("docs.bst"));

    let project = builder
        .build_backend(
            vec![
                create_test_module(PathBuf::from("#page.bst")),
                create_test_module(PathBuf::from("#404.bst")),
            ],
            &config,
            &[],
        )
        .expect("build_backend should succeed");

    let output_paths = collect_output_paths(&project.output_files);
    assert_eq!(project.output_files.len(), 2);
    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("404.html")));
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));
}

#[test]
fn duplicate_output_paths_are_rejected() {
    let builder = HtmlProjectBuilder::new();
    let config = Config::new(PathBuf::from("docs.bst"));

    let result = builder.build_backend(
        vec![
            create_test_module(PathBuf::from("#page.bst")),
            create_test_module(PathBuf::from("index.bst")),
        ],
        &config,
        &[],
    );

    let err = match result {
        Err(messages) => messages,
        Ok(_) => panic!("duplicate output paths should fail"),
    };
    assert!(
        err.errors
            .iter()
            .any(|error| error.msg.contains("duplicate output path")),
        "expected duplicate output path error message"
    );
    assert!(
        err.errors
            .iter()
            .any(|error| error.error_type == ErrorType::Config),
        "expected duplicate output path to be classified as a config error"
    );
}

#[test]
fn emits_runtime_slots_and_bootstrap_calls_start() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let mut module = create_test_module(entry_path.clone());
    module.hir.start_fragments = vec![
        StartFragment::ConstString(ConstStringId(0)),
        StartFragment::RuntimeStringFn(FunctionId(0)),
    ];
    module.hir.const_string_pool = vec![String::from("<meta charset=\"utf-8\">")];

    let project = builder
        .build_backend(vec![module], &Config::new(entry_path), &[])
        .expect("build_backend should succeed");

    let html = expect_html_output(&project.output_files, "index.html");
    let start_name = expected_dev_function_name("start_entry", 0);

    assert_has_basic_shell(html);
    assert!(html.contains("<meta charset=\"utf-8\">"));
    assert!(html.contains("<div id=\"bst-slot-0\"></div>"));
    assert!(html.contains("insertAdjacentHTML(\"beforeend\", fn());"));
    assert!(html.contains(&format!(
        "if (typeof {} === \"function\") {}();",
        start_name, start_name
    )));
}

#[test]
fn directory_build_maps_routes_relative_to_entry_root() {
    let root = temp_dir("directory_routes");
    fs::create_dir_all(root.join("src/about")).expect("should create about dir");
    fs::create_dir_all(root.join("src/docs/basics")).expect("should create docs dir");
    fs::create_dir_all(root.join("src/blog")).expect("should create blog dir");
    let entry_root = fs::canonicalize(root.join("src")).expect("entry root should resolve");

    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(root.clone());
    config.entry_root = PathBuf::from("src");

    let project = builder
        .build_backend(
            vec![
                create_test_module(entry_root.join("#page.bst")),
                create_test_module(entry_root.join("about").join("#page.bst")),
                create_test_module(entry_root.join("docs").join("basics").join("#page.bst")),
                create_test_module(entry_root.join("blog").join("#404.bst")),
            ],
            &config,
            &[],
        )
        .expect("directory build should succeed");

    let output_paths = collect_output_paths(&project.output_files);
    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("about/index.html")));
    assert!(output_paths.contains(&PathBuf::from("docs/basics/index.html")));
    assert!(output_paths.contains(&PathBuf::from("blog/404/index.html")));
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn directory_build_supports_custom_entry_root_names() {
    let root = temp_dir("custom_entry_root");
    fs::create_dir_all(root.join("pages/docs")).expect("should create pages dir");
    let entry_root = fs::canonicalize(root.join("pages")).expect("entry root should resolve");

    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(root.clone());
    config.entry_root = PathBuf::from("pages");

    let project = builder
        .build_backend(
            vec![
                create_test_module(entry_root.join("#page.bst")),
                create_test_module(entry_root.join("docs").join("#page.bst")),
            ],
            &config,
            &[],
        )
        .expect("directory build should succeed");

    let output_paths = collect_output_paths(&project.output_files);
    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("docs/index.html")));
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn directory_build_requires_homepage_at_entry_root() {
    let root = temp_dir("missing_homepage");
    fs::create_dir_all(root.join("src/about")).expect("should create about dir");
    let entry_root = fs::canonicalize(root.join("src")).expect("entry root should resolve");

    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(root.clone());
    config.entry_root = PathBuf::from("src");

    let result = builder.build_backend(
        vec![create_test_module(
            entry_root.join("about").join("#page.bst"),
        )],
        &config,
        &[],
    );

    let err = match result {
        Err(messages) => messages,
        Ok(_) => panic!("missing homepage should fail"),
    };
    assert!(
        err.errors
            .iter()
            .any(|error| error.msg.contains("require a '#page.bst' homepage")),
        "expected homepage error message"
    );
    assert!(
        err.errors
            .iter()
            .any(|error| error.error_type == ErrorType::Config),
        "expected missing homepage to be classified as a config error"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn wasm_flag_emits_html_js_and_wasm_artifacts() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");

    let project = builder
        .build_backend(
            vec![create_test_module(entry_path.clone())],
            &Config::new(entry_path),
            &[Flag::HtmlWasm],
        )
        .expect("wasm mode build should succeed");

    let output_paths = collect_output_paths(&project.output_files);
    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("page.js")));
    assert!(output_paths.contains(&PathBuf::from("page.wasm")));
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));

    let html = expect_html_output(&project.output_files, "index.html");
    assert_has_basic_shell(html);
    assert!(html.contains("<script src=\"./page.js\"></script>"));
    assert_fragment_before_body_close(html, "<script src=\"./page.js\"></script>");

    let js = expect_js_output(&project.output_files, "page.js");
    assert!(js.contains("WebAssembly.instantiateStreaming"));
    assert!(js.contains("bst_str_ptr"));
    assert!(
        project
            .output_files
            .iter()
            .any(|file| matches!(file.file_kind(), FileKind::Wasm(_))),
        "expected one wasm artifact in wasm mode"
    );
}

#[test]
fn wasm_mode_uses_per_page_folder_layout() {
    let builder = HtmlProjectBuilder::new();
    let config = Config::new(PathBuf::from("docs.bst"));

    let project = builder
        .build_backend(
            vec![
                create_test_module(PathBuf::from("#page.bst")),
                create_test_module(PathBuf::from("#404.bst")),
            ],
            &config,
            &[Flag::HtmlWasm],
        )
        .expect("wasm mode build should succeed");

    let output_paths = collect_output_paths(&project.output_files);
    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("page.js")));
    assert!(output_paths.contains(&PathBuf::from("page.wasm")));
    assert!(output_paths.contains(&PathBuf::from("404/index.html")));
    assert!(output_paths.contains(&PathBuf::from("404/page.js")));
    assert!(output_paths.contains(&PathBuf::from("404/page.wasm")));
}

#[test]
fn builder_rejects_invalid_origin_config() {
    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(PathBuf::from("."));
    config
        .settings
        .insert(String::from("origin"), String::from("not-a-slash"));

    let result = builder.build_backend(
        vec![create_test_module(PathBuf::from("#page.bst"))],
        &config,
        &[],
    );
    let messages = match result {
        Err(messages) => messages,
        Ok(_) => panic!("invalid origin should fail"),
    };
    assert!(
        messages.errors[0]
            .msg
            .contains("'#origin' must start with '/'")
    );
}
