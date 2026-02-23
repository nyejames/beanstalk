use super::*;
use crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction, HirModule,
    HirRegion, HirTerminator, RegionId, ValueKind,
};
use crate::projects::settings::Config;
use std::path::PathBuf;

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
fn build_backend_emits_single_js_output_file() {
    let builder = HtmlProjectBuilder::new();
    let entry_path = PathBuf::from("main.bst");
    let module = create_test_module(entry_path.clone());
    let config = Config::new(entry_path.clone());

    let project = builder
        .build_backend(vec![module], &config, &[])
        .expect("build_backend should succeed");

    assert_eq!(project.output_files.len(), 1);
    assert_eq!(project.output_files[0].full_file_path, entry_path);
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

    let dev_js = match dev_project.output_files[0].file_kind() {
        FileKind::Js(content) => content,
        _ => panic!("expected JS output for dev build"),
    };

    let release_js = match release_project.output_files[0].file_kind() {
        FileKind::Js(content) => content,
        _ => panic!("expected JS output for release build"),
    };

    assert!(
        dev_js.contains("    return;"),
        "dev build should include pretty indentation for statements"
    );
    assert!(
        release_js.contains("return;"),
        "release build should still emit valid JS statements"
    );
    assert!(
        !release_js.contains("    return;"),
        "release build should avoid pretty indentation"
    );
}

#[test]
fn auto_invoke_start_policy_matches_entrypoint_rules() {
    let single_file_config = Config::new(PathBuf::from("main.bst"));
    let single_file_entry = PathBuf::from("main.bst");
    assert!(should_auto_invoke_start(
        &single_file_config,
        &single_file_entry
    ));

    let page_config = Config::new(PathBuf::from("website"));
    let page_entry = PathBuf::from("website/#page.bst");
    assert!(should_auto_invoke_start(&page_config, &page_entry));

    let imported_entry = PathBuf::from("website/helper.bst");
    assert!(!should_auto_invoke_start(&page_config, &imported_entry));
}
