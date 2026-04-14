//! HIR function-origin tracking tests.
//!
//! WHAT: verifies how HIR records entry and normal function origins.
//! WHY: backends rely on stable function-origin metadata when deciding which functions to emit.
//!
//! The old FileStart and RuntimeTemplate origins are removed.
//! Only EntryStart (for the entry-file implicit start) and Normal (for all other functions)
//! remain after Phase 1.

use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, SourceLocation};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{FunctionId, HirFunctionOrigin};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn location(line: i32) -> SourceLocation {
    crate::compiler_frontend::hir::tests::hir_expression_lowering_tests::location(line)
}

fn node(kind: NodeKind, location: SourceLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn function_node(name: InternedPath, location: SourceLocation) -> AstNode {
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

fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        const_top_level_fragments: vec![],
        rendered_path_usages: vec![],
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
fn classifies_entry_start_and_normal_functions() {
    let mut string_table = StringTable::new();

    let entry_path = InternedPath::from_single_str("main.bst", &mut string_table);
    let entry_start = entry_path.join_str(IMPLICIT_START_FUNC_NAME, &mut string_table);
    let normal_fn = entry_path.join_str("helper", &mut string_table);

    let ast = build_ast(
        vec![
            function_node(entry_start, location(1)),
            function_node(normal_fn.clone(), location(2)),
        ],
        entry_path,
    );

    let module = HirBuilder::new(&mut string_table, PathStringFormatConfig::default())
        .build_hir_module(ast)
        .expect("HIR lowering should succeed");

    let normal_id =
        find_function_id_by_path(&module, &normal_fn).expect("normal function should be present");

    assert_eq!(
        module.function_origins.get(&module.start_function),
        Some(&HirFunctionOrigin::EntryStart)
    );
    assert_eq!(
        module.function_origins.get(&normal_id),
        Some(&HirFunctionOrigin::Normal)
    );
    // Every function has exactly one origin tag.
    assert_eq!(module.function_origins.len(), module.functions.len());
}
