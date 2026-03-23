use crate::compiler_frontend::ast::ast::{Ast, AstStartTemplateItem, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, TextLocation};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirFunctionOrigin};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn location(line: i32) -> TextLocation {
    crate::compiler_frontend::hir::tests::hir_expression_lowering_tests::location(line)
}

fn node(kind: NodeKind, location: TextLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn function_node(name: InternedPath, location: TextLocation) -> AstNode {
    node(
        NodeKind::Function(
            name,
            FunctionSignature {
                parameters: vec![],
                returns: vec![],
            },
            vec![node(NodeKind::Return(vec![]), location.clone())],
        ),
        location,
    )
}

fn build_ast(
    nodes: Vec<AstNode>,
    entry_path: InternedPath,
    start_template_items: Vec<AstStartTemplateItem>,
) -> Ast {
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        start_template_items,
        warnings: vec![],
    }
}

fn find_function_id_by_path(
    module: &crate::compiler_frontend::hir::hir_nodes::HirModule,
    target_path: &InternedPath,
) -> Option<FunctionId> {
    module.functions.iter().find_map(|function| {
        let path = module.side_table.function_name_path(function.id)?;
        (path == target_path).then_some(function.id)
    })
}

#[test]
fn classifies_entry_file_start_runtime_template_and_normal_functions() {
    let mut string_table = StringTable::new();

    let entry_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_start = entry_path.join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);

    let imported_path = InternedPath::from_single_str("imported.bst", &mut string_table);
    let imported_start = imported_path.join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);

    let runtime_fragment_fn = entry_path.join_str("__bst_frag_0", &mut string_table);
    let normal_fn = entry_path.join_str("helper", &mut string_table);

    let ast = build_ast(
        vec![
            function_node(entry_start, location(1)),
            function_node(imported_start.clone(), location(2)),
            function_node(runtime_fragment_fn.clone(), location(3)),
            function_node(normal_fn.clone(), location(4)),
        ],
        entry_path,
        vec![AstStartTemplateItem::RuntimeStringFunction {
            function: runtime_fragment_fn.clone(),
            location: location(5),
        }],
    );

    let module = HirBuilder::new(&mut string_table, PathStringFormatConfig::default())
        .build_hir_module(ast)
        .expect("HIR lowering should succeed");

    let imported_start_id = find_function_id_by_path(&module, &imported_start)
        .expect("imported implicit start should be present");
    let runtime_id = find_function_id_by_path(&module, &runtime_fragment_fn)
        .expect("runtime template function should be present");
    let normal_id =
        find_function_id_by_path(&module, &normal_fn).expect("normal function should be present");

    assert_eq!(
        module.function_origins.get(&module.start_function),
        Some(&HirFunctionOrigin::EntryStart)
    );
    assert_eq!(
        module.function_origins.get(&imported_start_id),
        Some(&HirFunctionOrigin::FileStart)
    );
    assert_eq!(
        module.function_origins.get(&runtime_id),
        Some(&HirFunctionOrigin::RuntimeTemplate)
    );
    assert_eq!(
        module.function_origins.get(&normal_id),
        Some(&HirFunctionOrigin::Normal)
    );
    assert_eq!(module.function_origins.len(), module.functions.len());
}
