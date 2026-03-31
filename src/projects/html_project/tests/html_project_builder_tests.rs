//! Tests for HTML project builder orchestration.

use super::*;
use crate::backends::js::test_symbol_helpers::expected_dev_function_name;
use crate::build_system::build::{FileKind, Project};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::compiler_errors::{CompilerMessages, ErrorType};
use crate::compiler_frontend::hir::hir_nodes::{ConstStringId, FunctionId, StartFragment};
use crate::compiler_frontend::paths::path_resolution::{CompileTimePathBase, CompileTimePathKind};
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::html_project::tests::test_support::{
    assert_fragment_before_body_close, assert_has_basic_shell, collect_output_paths,
    create_test_module, expect_bytes_output, expect_html_output, expect_js_output,
    rendered_path_usage, temp_dir,
};
use crate::projects::settings::Config;
use std::fs;
use std::path::PathBuf;

fn build_with_test_modules(
    builder: &HtmlProjectBuilder,
    entry_points: Vec<PathBuf>,
    config: &Config,
    flags: &[Flag],
) -> Result<Project, CompilerMessages> {
    let mut string_table = StringTable::new();
    let modules = entry_points
        .into_iter()
        .map(|entry_point| create_test_module(entry_point, &mut string_table))
        .collect();
    builder.build_backend(modules, config, flags, &mut string_table)
}

#[test]
fn build_backend_emits_single_html_output_file() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let config = Config::new(entry_path.clone());

    let project = build_with_test_modules(&builder, vec![entry_path], &config, &[])
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

    let dev_project = build_with_test_modules(
        &builder,
        vec![entry_path.clone()],
        &Config::new(entry_path.clone()),
        &[],
    )
    .expect("dev build should succeed");
    let release_project = build_with_test_modules(
        &builder,
        vec![entry_path.clone()],
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

    let project = build_with_test_modules(&builder, vec![entry_path], &config, &[])
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

    let project = build_with_test_modules(
        &builder,
        vec![PathBuf::from("#page.bst"), PathBuf::from("#404.bst")],
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

    let result = build_with_test_modules(
        &builder,
        vec![PathBuf::from("#page.bst"), PathBuf::from("index.bst")],
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
    let mut string_table = StringTable::new();
    let mut module = create_test_module(entry_path.clone(), &mut string_table);
    module.hir.start_fragments = vec![
        StartFragment::ConstString(ConstStringId(0)),
        StartFragment::RuntimeStringFn(FunctionId(0)),
    ];
    module.hir.const_string_pool = vec![String::from("<meta charset=\"utf-8\">")];

    let project = builder
        .build_backend(
            vec![module],
            &Config::new(entry_path),
            &[],
            &mut string_table,
        )
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

    let project = build_with_test_modules(
        &builder,
        vec![
            entry_root.join("#page.bst"),
            entry_root.join("about").join("#page.bst"),
            entry_root.join("docs").join("basics").join("#page.bst"),
            entry_root.join("blog").join("#404.bst"),
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

    let project = build_with_test_modules(
        &builder,
        vec![
            entry_root.join("#page.bst"),
            entry_root.join("docs").join("#page.bst"),
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

    let result = build_with_test_modules(
        &builder,
        vec![entry_root.join("about").join("#page.bst")],
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

    let project = build_with_test_modules(
        &builder,
        vec![entry_path.clone()],
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

    let project = build_with_test_modules(
        &builder,
        vec![PathBuf::from("#page.bst"), PathBuf::from("#404.bst")],
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
fn wasm_directory_build_preserves_nested_routes() {
    let root = temp_dir("wasm_directory_routes");
    fs::create_dir_all(root.join("src/docs")).expect("should create docs dir");
    fs::create_dir_all(root.join("src/blog")).expect("should create blog dir");
    let entry_root = fs::canonicalize(root.join("src")).expect("entry root should resolve");

    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(root.clone());
    config.entry_root = PathBuf::from("src");

    let project = build_with_test_modules(
        &builder,
        vec![
            entry_root.join("#page.bst"),
            entry_root.join("docs").join("#page.bst"),
            entry_root.join("blog").join("#404.bst"),
        ],
        &config,
        &[Flag::HtmlWasm],
    )
    .expect("wasm directory build should succeed without duplicate output paths");

    let output_paths = collect_output_paths(&project.output_files);
    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("page.js")));
    assert!(output_paths.contains(&PathBuf::from("page.wasm")));
    assert!(output_paths.contains(&PathBuf::from("docs/index.html")));
    assert!(output_paths.contains(&PathBuf::from("docs/page.js")));
    assert!(output_paths.contains(&PathBuf::from("docs/page.wasm")));
    assert!(output_paths.contains(&PathBuf::from("blog/404/index.html")));
    assert!(output_paths.contains(&PathBuf::from("blog/404/page.js")));
    assert!(output_paths.contains(&PathBuf::from("blog/404/page.wasm")));
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn builder_rejects_invalid_origin_config() {
    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(PathBuf::from("."));
    config
        .settings
        .insert(String::from("origin"), String::from("not-a-slash"));

    let result = build_with_test_modules(&builder, vec![PathBuf::from("#page.bst")], &config, &[]);
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

#[test]
fn build_backend_emits_tracked_assets_and_dedupes_same_source_output() {
    let root = temp_dir("builder_tracked_asset_dedupe");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    fs::create_dir_all(root.join("docs")).expect("should create docs dir");
    fs::write(root.join("assets/logo.png"), [1_u8, 2, 3]).expect("should write asset");
    let canonical_root = fs::canonicalize(&root).expect("root should resolve");

    let builder = HtmlProjectBuilder::new();
    let config = Config::new(root.clone());
    let mut string_table = StringTable::new();

    let mut homepage = create_test_module(canonical_root.join("#page.bst"), &mut string_table);
    homepage.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        &["assets", "logo.png"],
        &["assets", "logo.png"],
        canonical_root.join("assets/logo.png"),
        CompileTimePathBase::ProjectRootFolder,
        CompileTimePathKind::File,
        &["#page.bst"],
        1,
    ));

    let mut docs_page =
        create_test_module(canonical_root.join("docs/#page.bst"), &mut string_table);
    docs_page.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        &["assets", "logo.png"],
        &["assets", "logo.png"],
        canonical_root.join("assets/logo.png"),
        CompileTimePathBase::ProjectRootFolder,
        CompileTimePathKind::File,
        &["docs", "#page.bst"],
        1,
    ));

    let project = builder
        .build_backend(vec![homepage, docs_page], &config, &[], &mut string_table)
        .expect("tracked-asset build should succeed");

    let output_paths = collect_output_paths(&project.output_files);
    assert!(output_paths.contains(&PathBuf::from("assets/logo.png")));
    assert_eq!(
        expect_bytes_output(&project.output_files, "assets/logo.png"),
        [1_u8, 2, 3]
    );
    assert_eq!(
        project
            .output_files
            .iter()
            .filter(|file| matches!(file.file_kind(), FileKind::Bytes(_)))
            .count(),
        1,
        "same source/same emitted path should dedupe"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_backend_allows_same_source_file_to_emit_multiple_relative_outputs() {
    let root = temp_dir("builder_tracked_asset_relative_copies");
    fs::create_dir_all(root.join("blog/post")).expect("should create blog dir");
    fs::create_dir_all(root.join("shared")).expect("should create shared dir");
    fs::write(root.join("shared/logo.png"), [4_u8, 5, 6]).expect("should write asset");
    let canonical_root = fs::canonicalize(&root).expect("root should resolve");

    let builder = HtmlProjectBuilder::new();
    let config = Config::new(root.clone());
    let mut string_table = StringTable::new();

    let mut homepage = create_test_module(canonical_root.join("#page.bst"), &mut string_table);
    homepage.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        &[".", "logo.png"],
        &[".", "logo.png"],
        canonical_root.join("shared/logo.png"),
        CompileTimePathBase::RelativeToFile,
        CompileTimePathKind::File,
        &["#page.bst"],
        1,
    ));

    let mut blog_page = create_test_module(
        canonical_root.join("blog/post/#page.bst"),
        &mut string_table,
    );
    blog_page.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        &["..", "shared", "logo.png"],
        &["..", "shared", "logo.png"],
        canonical_root.join("shared/logo.png"),
        CompileTimePathBase::RelativeToFile,
        CompileTimePathKind::File,
        &["blog", "post", "#page.bst"],
        1,
    ));

    let project = builder
        .build_backend(vec![homepage, blog_page], &config, &[], &mut string_table)
        .expect("tracked-asset build should succeed");

    assert_eq!(
        expect_bytes_output(&project.output_files, "logo.png"),
        [4_u8, 5, 6]
    );
    assert_eq!(
        expect_bytes_output(&project.output_files, "blog/shared/logo.png"),
        [4_u8, 5, 6]
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}

#[test]
fn build_backend_rejects_conflicting_tracked_asset_output_paths() {
    let root = temp_dir("builder_tracked_asset_conflict");
    fs::create_dir_all(root.join("assets")).expect("should create assets dir");
    fs::create_dir_all(root.join("docs")).expect("should create docs dir");
    fs::write(root.join("assets/logo-a.png"), [1_u8]).expect("should write first asset");
    fs::write(root.join("assets/logo-b.png"), [2_u8]).expect("should write second asset");
    let canonical_root = fs::canonicalize(&root).expect("root should resolve");

    let builder = HtmlProjectBuilder::new();
    let config = Config::new(root.clone());
    let mut string_table = StringTable::new();

    let mut homepage = create_test_module(canonical_root.join("#page.bst"), &mut string_table);
    homepage.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        &["assets", "logo.png"],
        &["assets", "logo.png"],
        canonical_root.join("assets/logo-a.png"),
        CompileTimePathBase::ProjectRootFolder,
        CompileTimePathKind::File,
        &["#page.bst"],
        1,
    ));

    let mut docs_page =
        create_test_module(canonical_root.join("docs/#page.bst"), &mut string_table);
    docs_page.hir.rendered_path_usages.push(rendered_path_usage(
        &mut string_table,
        &["assets", "logo.png"],
        &["assets", "logo.png"],
        canonical_root.join("assets/logo-b.png"),
        CompileTimePathBase::ProjectRootFolder,
        CompileTimePathKind::File,
        &["docs", "#page.bst"],
        1,
    ));

    let error =
        match builder.build_backend(vec![homepage, docs_page], &config, &[], &mut string_table) {
            Err(messages) => messages,
            Ok(_) => panic!("conflicting tracked assets should fail"),
        };

    assert!(
        error.errors[0].msg.contains("already claimed"),
        "expected conflicting tracked-asset output error"
    );

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
