use super::*;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, ConstStringId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction,
    HirModule, HirRegion, HirTerminator, RegionId, StartFragment, ValueKind,
};
use crate::projects::settings::Config;
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
    }];
    module.start_function = FunctionId(0);

    module
}

fn create_test_module(entry_point: PathBuf) -> Module {
    let mut string_table = StringTable::new();
    let mut hir_module = create_test_hir_module();
    hir_module.side_table.bind_function_name(
        FunctionId(0),
        crate::compiler_frontend::interned_path::InternedPath::from_single_str(
            "main",
            &mut string_table,
        ),
    );

    Module {
        folder_name: "test".to_owned(),
        entry_point,
        hir: hir_module,
        borrow_analysis: BorrowCheckReport::default(),
        required_module_imports: vec![],
        exported_functions: vec![],
        warnings: vec![],
        string_table,
    }
}

#[test]
fn build_backend_emits_single_html_output_file() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("main.bst");
    let module = create_test_module(entry_path.clone());
    let config = Config::new(entry_path.clone());

    let project = builder
        .build_backend(vec![module], &config, &[])
        .expect("build_backend should succeed");

    assert_eq!(project.output_files.len(), 1);
    assert_eq!(
        project.output_files[0].relative_output_path(),
        PathBuf::from("main.html")
    );
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("main.html")));
    assert!(matches!(
        project.output_files[0].file_kind(),
        FileKind::Html(_)
    ));
}

#[test]
fn build_backend_respects_release_pretty_toggle() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("main.bst");

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
fn page_entry_outputs_index_html() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("#page.bst");
    let module = create_test_module(entry_path.clone());
    let config = Config::new(entry_path);

    let project = builder
        .build_backend(vec![module], &config, &[])
        .expect("build_backend should succeed");
    assert_eq!(
        project.output_files[0].relative_output_path(),
        PathBuf::from("index.html")
    );
    assert_eq!(project.entry_page_rel, Some(PathBuf::from("index.html")));
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
}

#[test]
fn emits_runtime_slots_and_bootstrap_calls_start() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("main.bst");
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
    assert!(html.contains("if (typeof main === \"function\") main();"));
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
    assert!(output_paths.contains(&PathBuf::from("about.html")));
    assert!(output_paths.contains(&PathBuf::from("docs/basics.html")));
    assert!(output_paths.contains(&PathBuf::from("blog/404.html")));
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
    assert!(output_paths.contains(&PathBuf::from("docs.html")));
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

    fs::remove_dir_all(&root).expect("should remove temp dir");
}
