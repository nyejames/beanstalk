use super::*;
use crate::build_system::build::{FileKind, Module};
use crate::compiler_frontend::Flag;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, ConstStringId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction,
    HirFunctionOrigin, HirModule, HirNodeId, HirRegion, HirStatement, HirStatementKind,
    HirTerminator, RegionId, StartFragment, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;
use crate::projects::html_project::js_path::{
    RuntimeSlotMount, escape_inline_script, render_entry_fragments, render_html_document,
};
use crate::projects::html_project::wasm::artifacts::build_html_wasm_plan;
use crate::projects::settings::Config;
use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

fn temp_dir(prefix: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .expect("time should be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("beanstalk_html_builder_{prefix}_{unique}"))
}

fn create_test_hir_module() -> HirModule {
    let mut module = HirModule::new();
    let mut type_context = TypeContext::default();
    let unit_type = type_context.insert(HirType {
        kind: HirTypeKind::Unit,
    });

    module.type_context = type_context;
    module.regions = vec![HirRegion::lexical(RegionId(0), None)];

    module.blocks = vec![HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(HirExpression {
            id: crate::compiler_frontend::hir::hir_nodes::HirValueId(0),
            kind: HirExpressionKind::TupleConstruct { elements: vec![] },
            ty: unit_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        }),
    }];

    module.functions = vec![HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: unit_type,
        return_aliases: vec![],
    }];
    module.start_function = FunctionId(0);
    module
        .function_origins
        .insert(FunctionId(0), HirFunctionOrigin::EntryStart);

    module
}

fn create_test_module(entry_point: PathBuf) -> Module {
    let mut string_table = StringTable::new();
    let mut hir_module = create_test_hir_module();
    hir_module.side_table.bind_function_name(
        FunctionId(0),
        crate::compiler_frontend::interned_path::InternedPath::from_single_str(
            "start_entry",
            &mut string_table,
        ),
    );

    Module {
        entry_point,
        hir: hir_module,
        borrow_analysis: BorrowCheckReport::default(),
        warnings: vec![],
        string_table,
    }
}

fn add_callable_function(module: &mut Module, function_id: FunctionId, name: &str) {
    let unit_type = module.hir.functions[0].return_type;
    let block_id = BlockId(module.hir.blocks.len() as u32);
    let value_id =
        crate::compiler_frontend::hir::hir_nodes::HirValueId(module.hir.blocks.len() as u32 + 10);

    module.hir.blocks.push(HirBlock {
        id: block_id,
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(HirExpression {
            id: value_id,
            kind: HirExpressionKind::TupleConstruct { elements: vec![] },
            ty: unit_type,
            value_kind: ValueKind::Const,
            region: RegionId(0),
        }),
    });
    module.hir.functions.push(HirFunction {
        id: function_id,
        entry: block_id,
        params: vec![],
        return_type: unit_type,
        return_aliases: vec![],
    });
    module
        .hir
        .function_origins
        .insert(function_id, HirFunctionOrigin::Normal);
    module.hir.side_table.bind_function_name(
        function_id,
        InternedPath::from_single_str(name, &mut module.string_table),
    );
}

fn add_start_call(module: &mut Module, target_name: &str, statement_id: u32) {
    let target_path = InternedPath::from_single_str(target_name, &mut module.string_table);
    let start_block = module
        .hir
        .blocks
        .iter_mut()
        .find(|block| block.id == BlockId(0))
        .expect("start block should exist");
    start_block.statements.push(HirStatement {
        id: HirNodeId(statement_id),
        kind: HirStatementKind::Call {
            target: CallTarget::UserFunction(target_path),
            args: vec![],
            result: None,
        },
        location: TextLocation::default(),
    });
}

#[test]
fn build_backend_emits_single_html_output_file() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let module = create_test_module(entry_path.clone());
    let config = Config::new(entry_path.clone());

    let project = builder
        .build_backend(vec![module], &config, &[])
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

    let dev_html = match dev_project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        _ => panic!("expected HTML output for dev build"),
    };

    let release_html = match release_project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        _ => panic!("expected HTML output for release build"),
    };

    assert!(
        dev_html.contains("    return;"),
        "dev build should include pretty indentation for statements"
    );
    assert!(
        release_html.contains("return;"),
        "release build should still emit valid JS statements"
    );
    assert!(
        !release_html.contains("    return;"),
        "release build should avoid pretty indentation"
    );
}

#[test]
fn hash_prefixed_route_name_strips_hash_from_output() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#404.bst");
    let module = create_test_module(entry_path.clone());
    let config = Config::new(entry_path);

    let project = builder
        .build_backend(vec![module], &config, &[])
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

    let output_paths = project
        .output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect::<Vec<_>>();

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

    assert!(result.is_err(), "duplicate output paths should fail");
    let err = result.err().expect("expected duplicate output path error");
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

    let html = match project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        _ => panic!("expected HTML output"),
    };

    assert!(html.contains("<meta charset=\"utf-8\">"));
    assert!(html.contains("<div id=\"bst-slot-0\"></div>"));
    assert!(html.contains("insertAdjacentHTML(\"beforeend\", fn());"));
    assert!(html.contains("if (typeof start_entry === \"function\") start_entry();"));
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

    let output_paths = project
        .output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect::<Vec<_>>();

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

    let output_paths = project
        .output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect::<Vec<_>>();

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

    assert!(result.is_err(), "missing homepage should fail");
    let err = result.err().expect("expected missing homepage error");
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

    let output_paths = project
        .output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect::<Vec<_>>();

    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("page.js")));
    assert!(output_paths.contains(&PathBuf::from("page.wasm")));
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));

    let html = project
        .output_files
        .iter()
        .find_map(|file| match file.file_kind() {
            FileKind::Html(content) if file.relative_output_path() == "index.html" => Some(content),
            _ => None,
        })
        .expect("index html should exist");
    assert!(html.contains("<script src=\"./page.js\"></script>"));

    let js = project
        .output_files
        .iter()
        .find_map(|file| match file.file_kind() {
            FileKind::Js(content) if file.relative_output_path() == "page.js" => Some(content),
            _ => None,
        })
        .expect("page.js should exist");
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

    let output_paths = project
        .output_files
        .iter()
        .map(|file| file.relative_output_path().to_path_buf())
        .collect::<Vec<_>>();

    assert!(output_paths.contains(&PathBuf::from("index.html")));
    assert!(output_paths.contains(&PathBuf::from("page.js")));
    assert!(output_paths.contains(&PathBuf::from("page.wasm")));
    assert!(output_paths.contains(&PathBuf::from("404/index.html")));
    assert!(output_paths.contains(&PathBuf::from("404/page.js")));
    assert!(output_paths.contains(&PathBuf::from("404/page.wasm")));
}

#[test]
fn wasm_mode_bootstrap_calls_wrapper_exports_not_internal_names() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let mut module = create_test_module(entry_path.clone());

    add_callable_function(&mut module, FunctionId(1), "helper_fn");
    add_start_call(&mut module, "helper_fn", 11);

    let project = builder
        .build_backend(vec![module], &Config::new(entry_path), &[Flag::HtmlWasm])
        .expect("wasm mode build should succeed");
    let js = project
        .output_files
        .iter()
        .find_map(|file| match file.file_kind() {
            FileKind::Js(content) if file.relative_output_path() == "page.js" => Some(content),
            _ => None,
        })
        .expect("page.js should be emitted");

    assert!(js.contains("helper_fn = (...args) =>"));
    assert!(js.contains("bst_call_0"));
    assert!(js.contains("const slots = ["));
}

#[test]
fn wasm_export_plan_is_deterministic_with_stable_wrapper_names() {
    let mut module = create_test_module(PathBuf::from("#page.bst"));
    add_callable_function(&mut module, FunctionId(2), "helper_b");
    add_callable_function(&mut module, FunctionId(1), "helper_a");
    add_start_call(&mut module, "helper_b", 41);
    add_start_call(&mut module, "helper_a", 42);
    module.hir.start_fragments = vec![StartFragment::RuntimeStringFn(FunctionId(2))];

    let function_name_by_id = HashMap::from([
        (FunctionId(0), String::from("start_entry")),
        (FunctionId(1), String::from("helper_a")),
        (FunctionId(2), String::from("helper_b")),
    ]);

    let plan_a = build_html_wasm_plan(
        &module.hir,
        &function_name_by_id,
        Vec::<RuntimeSlotMount>::new(),
    )
    .expect("wasm plan should build");
    let plan_b = build_html_wasm_plan(
        &module.hir,
        &function_name_by_id,
        Vec::<RuntimeSlotMount>::new(),
    )
    .expect("wasm plan should build");

    assert_eq!(
        plan_a
            .export_plan
            .function_exports
            .iter()
            .map(|item| (item.function_id.0, item.export_name.clone()))
            .collect::<Vec<_>>(),
        vec![
            (FunctionId(1).0, String::from("bst_call_0")),
            (FunctionId(2).0, String::from("bst_call_1")),
        ]
    );
    assert_eq!(
        plan_a
            .export_plan
            .function_exports
            .iter()
            .map(|item| item.export_name.clone())
            .collect::<Vec<_>>(),
        plan_b
            .export_plan
            .function_exports
            .iter()
            .map(|item| item.export_name.clone())
            .collect::<Vec<_>>()
    );
}

#[test]
fn wasm_export_plan_wires_required_helper_exports() {
    let module = create_test_module(PathBuf::from("#page.bst"));
    let function_name_by_id = HashMap::from([(FunctionId(0), String::from("start_entry"))]);

    let plan = build_html_wasm_plan(
        &module.hir,
        &function_name_by_id,
        Vec::<RuntimeSlotMount>::new(),
    )
    .expect("wasm plan should build");
    let helper = plan.wasm_request.export_policy.helper_exports;

    assert!(helper.export_memory);
    assert!(helper.export_str_ptr);
    assert!(helper.export_str_len);
    assert!(helper.export_release);
}

#[test]
fn builder_rejects_invalid_origin_config() {
    let builder = HtmlProjectBuilder::new();
    let mut config = Config::new(PathBuf::from("."));
    config
        .settings
        .insert(String::from("origin"), String::from("not-a-slash"));

    let module = create_test_module(PathBuf::from("#page.bst"));
    let result = builder.build_backend(vec![module], &config, &[]);

    assert!(result.is_err());
    let messages = match result {
        Err(m) => m,
        Ok(_) => unreachable!(),
    };
    assert!(
        messages.errors[0]
            .msg
            .contains("'#origin' must start with '/'")
    );
}

// ---------------------------------------------------------------------------
// JS-only HTML lifecycle contract tests
// ---------------------------------------------------------------------------

/// Verifies that the HTML lifecycle order is: static fragments → JS bundle → hydration → start().
///
/// This pins the observable contract described in js_path.rs so regressions are caught without
/// manually reading emitted HTML.
#[test]
fn js_lifecycle_order_is_static_then_bundle_then_hydration_then_start() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let mut module = create_test_module(entry_path.clone());

    module.hir.start_fragments = vec![
        StartFragment::ConstString(ConstStringId(0)),
        StartFragment::RuntimeStringFn(FunctionId(0)),
    ];
    module.hir.const_string_pool = vec![String::from("<h1>Hello</h1>")];

    let project = builder
        .build_backend(vec![module], &Config::new(entry_path), &[])
        .expect("build_backend should succeed");

    let html = match project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        _ => panic!("expected HTML output"),
    };

    // Locate each lifecycle phase in the emitted HTML and verify their relative ordering.
    let static_pos = html
        .find("<h1>Hello</h1>")
        .expect("static fragment must be present");
    let slot_pos = html
        .find("<div id=\"bst-slot-0\">")
        .expect("runtime slot must be present");
    let bundle_pos = html
        .find("<script>")
        .expect("JS bundle script block must be present");
    let hydration_pos = html
        .find("insertAdjacentHTML")
        .expect("slot hydration must be present");
    let start_pos = html
        .find("if (typeof start_entry")
        .expect("start() invocation must be present");

    assert!(
        static_pos < slot_pos,
        "const fragment must appear before runtime slot"
    );
    assert!(
        slot_pos < bundle_pos,
        "runtime slot must appear before the JS bundle script tag"
    );
    assert!(
        bundle_pos < hydration_pos,
        "JS bundle must be loaded before slot hydration"
    );
    assert!(
        hydration_pos < start_pos,
        "slot hydration must complete before start() is called"
    );
}

/// Verifies that multiple runtime slots are mounted in the same order as the source fragments.
#[test]
fn multiple_runtime_slots_are_mounted_in_source_order() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let mut module = create_test_module(entry_path.clone());

    add_callable_function(&mut module, FunctionId(1), "frag_b");

    // Two runtime fragments in explicit source order.
    module.hir.start_fragments = vec![
        StartFragment::RuntimeStringFn(FunctionId(0)),
        StartFragment::RuntimeStringFn(FunctionId(1)),
    ];

    let project = builder
        .build_backend(vec![module], &Config::new(entry_path), &[])
        .expect("build_backend should succeed");

    let html = match project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        _ => panic!("expected HTML output"),
    };

    let slot0_pos = html.find("bst-slot-0").expect("bst-slot-0 must be present");
    let slot1_pos = html.find("bst-slot-1").expect("bst-slot-1 must be present");

    assert!(
        slot0_pos < slot1_pos,
        "runtime slots must appear in source fragment order"
    );
}

/// Verifies that a module with no runtime fragments still emits a valid start() call.
#[test]
fn no_runtime_fragments_still_emits_start_call() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let module = create_test_module(entry_path.clone());

    // No start_fragments — only the start function exists.
    let project = builder
        .build_backend(vec![module], &Config::new(entry_path), &[])
        .expect("build_backend should succeed");

    let html = match project.output_files[0].file_kind() {
        FileKind::Html(content) => content,
        _ => panic!("expected HTML output"),
    };

    assert!(
        !html.contains("bst-slot-"),
        "no runtime slots should be present when there are no runtime fragments"
    );
    assert!(
        html.contains("if (typeof start_entry === \"function\") start_entry();"),
        "start() must still be called when there are no runtime fragments"
    );
}

// ---------------------------------------------------------------------------
// Inline-script safety tests
// ---------------------------------------------------------------------------

/// Verifies that escape_inline_script replaces `</` with `<\/` to prevent script-tag breakout.
#[test]
fn escape_inline_script_replaces_closing_tag_sequence() {
    let js = "const x = \"</script>\";";
    let escaped = escape_inline_script(js);

    assert_eq!(
        escaped, "const x = \"<\\/script>\";",
        "escape_inline_script must replace '</' with '<\\/'"
    );
    assert!(
        !escaped.contains("</"),
        "escaped JS must not contain any '</' sequence"
    );
}

/// Verifies that a JS bundle containing `</script>` is safely embedded in the HTML output.
///
/// Uses render_html_document directly with a pre-built js_bundle that contains the hazardous
/// sequence, bypassing HIR lowering. This is the most direct regression test for the hazard.
#[test]
fn inline_js_bundle_with_closing_script_tag_is_escaped_in_html() {
    let mut hir_module = create_test_hir_module();
    hir_module.start_fragments = vec![];

    // Simulate a JS bundle that happens to contain the closing-tag sequence inside a string.
    let js_bundle = "const msg = \"</script>\";\n";

    let mut function_names = HashMap::new();
    function_names.insert(FunctionId(0), String::from("start_entry"));

    let html = render_html_document(&hir_module, js_bundle, &function_names)
        .expect("render_html_document should succeed");

    assert!(
        !html.contains("</script>\";"),
        "raw </script> inside a JS string must not appear unescaped in HTML output"
    );
    assert!(
        html.contains("<\\/script>"),
        "the closing-tag sequence must be escaped as <\\/script> in the output"
    );
}

// ---------------------------------------------------------------------------
// Error path tests
// ---------------------------------------------------------------------------

/// Verifies that render_entry_fragments returns an error when a ConstStringId is out of bounds.
#[test]
fn render_entry_fragments_errors_on_missing_const_string() {
    let mut hir_module = create_test_hir_module();
    hir_module.start_fragments = vec![StartFragment::ConstString(ConstStringId(99))];
    // const_string_pool is empty by default, so index 99 is out of bounds.

    let result = render_entry_fragments(&hir_module);
    let error = result.expect_err("should fail when const fragment ID is out of bounds");
    assert!(
        error.msg.contains("const fragment"),
        "error message must mention the missing const fragment"
    );
}

/// Verifies that render_html_document returns an error when a runtime fragment function name
/// is missing from the function-name map.
#[test]
fn render_html_document_errors_on_missing_function_name() {
    let mut hir_module = create_test_hir_module();
    hir_module.start_fragments = vec![StartFragment::RuntimeStringFn(FunctionId(99))];

    let function_names = HashMap::new();
    let result = render_html_document(&hir_module, "// bundle", &function_names);
    let error = result.expect_err("should fail when runtime fragment function name is missing");
    assert!(
        error.msg.contains("runtime fragment function"),
        "error message must mention the missing runtime fragment function"
    );
}
