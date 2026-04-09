//! Loop lowering regression tests.
//!
//! WHAT: validates range/collection loop lowering into explicit HIR CFG blocks.
//! WHY: loop header refactors must preserve control-flow semantics and loop-target routing.

use crate::compiler_frontend::ast::ast::Ast;
use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, Declaration, LoopBindings, NodeKind, RangeEndKind, RangeLoopSpec, SourceLocation,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::CompilerMessages;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirBinOp, HirExpressionKind, HirModule, HirPlace, HirStatementKind, HirTerminator,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::paths::path_format::PathStringFormatConfig;
use crate::compiler_frontend::string_interning::StringTable;

fn test_location(line: i32) -> SourceLocation {
    super::hir_expression_lowering_tests::location(line)
}

fn node(kind: NodeKind, location: SourceLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: SourceLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

fn loop_binding(name: &str, data_type: DataType, string_table: &mut StringTable) -> Declaration {
    let location = test_location(1);
    Declaration {
        id: super::symbol(name, string_table),
        value: Expression::new(
            ExpressionKind::NoValue,
            location,
            data_type,
            Ownership::ImmutableOwned,
        ),
    }
}

fn range_loop_spec(
    start: Expression,
    end: Expression,
    end_kind: RangeEndKind,
    step: Option<Expression>,
) -> RangeLoopSpec {
    RangeLoopSpec {
        start,
        end,
        end_kind,
        step,
    }
}

fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        module_constants: vec![],
        doc_fragments: vec![],
        entry_path,
        start_template_items: vec![],
        rendered_path_usages: vec![],
        warnings: vec![],
    }
}

fn lower_ast(ast: Ast, string_table: &mut StringTable) -> Result<HirModule, CompilerMessages> {
    HirBuilder::new(
        string_table,
        PathStringFormatConfig::default(),
        super::test_project_path_resolver(),
    )
    .build_hir_module(ast)
}

fn assert_no_placeholder_terminators(module: &HirModule) {
    assert!(
        module
            .blocks
            .iter()
            .all(|block| !matches!(block.terminator, HirTerminator::Panic { message: None })),
        "expected no placeholder Panic(None) terminators in lowered HIR"
    );
}

fn range_loop_cfg_blocks(module: &HirModule) -> (BlockId, BlockId, BlockId, BlockId, BlockId) {
    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let step_zero_check_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected entry jump to range step-zero-check"),
    };

    let step_abs_check_block = match module.blocks[step_zero_check_block.0 as usize].terminator {
        HirTerminator::If { else_block, .. } => else_block,
        _ => panic!("expected zero-check branch"),
    };

    let direction_check_block = match module.blocks[step_abs_check_block.0 as usize].terminator {
        HirTerminator::If { else_block, .. } => else_block,
        _ => panic!("expected abs-check branch"),
    };

    let header_selector_block = match module.blocks[direction_check_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected direction branch"),
    };

    let header_ascending_block = match module.blocks[header_selector_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected header selector branch"),
    };

    (
        step_zero_check_block,
        header_selector_block,
        header_ascending_block,
        step_abs_check_block,
        direction_check_block,
    )
}

fn collection_literal(location: SourceLocation) -> Expression {
    Expression::collection(
        vec![
            Expression::int(1, location.clone(), Ownership::ImmutableOwned),
            Expression::int(2, location.clone(), Ownership::ImmutableOwned),
            Expression::int(3, location.clone(), Ownership::ImmutableOwned),
        ],
        location,
        Ownership::ImmutableOwned,
    )
}

#[test]
fn lowers_range_loop_with_new_syntax() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(2);

    let range_loop = node(
        NodeKind::RangeLoop {
            bindings: LoopBindings {
                item: loop_binding("value", DataType::Int, &mut string_table),
                index: None,
            },
            range: range_loop_spec(
                Expression::int(0, location.clone(), Ownership::ImmutableOwned),
                Expression::int(3, location.clone(), Ownership::ImmutableOwned),
                RangeEndKind::Exclusive,
                None,
            ),
            body: vec![],
        },
        location.clone(),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![range_loop],
        test_location(1),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("range loop lowering should succeed");

    let (_, header_selector_block, header_ascending_block, _, _) = range_loop_cfg_blocks(&module);

    let (body_block, exit_block) = match module.blocks[header_ascending_block.0 as usize].terminator
    {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected ascending header branch"),
    };

    let step_block = match module.blocks[body_block.0 as usize].terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected body jump to step block"),
    };

    assert!(matches!(
        module.blocks[step_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_selector_block
    ));
    assert!(matches!(
        module.blocks[exit_block.0 as usize].terminator,
        HirTerminator::Return(_)
    ));
    assert_no_placeholder_terminators(&module);
}

#[test]
fn lowers_range_loop_with_index_binding() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(10);

    let range_loop = node(
        NodeKind::RangeLoop {
            bindings: LoopBindings {
                item: loop_binding("value", DataType::Int, &mut string_table),
                index: Some(loop_binding("index", DataType::Int, &mut string_table)),
            },
            range: range_loop_spec(
                Expression::int(0, location.clone(), Ownership::ImmutableOwned),
                Expression::int(4, location.clone(), Ownership::ImmutableOwned),
                RangeEndKind::Exclusive,
                None,
            ),
            body: vec![],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![range_loop],
        test_location(9),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("range loop lowering with index should succeed");

    let (_, _, header_ascending_block, _, _) = range_loop_cfg_blocks(&module);

    let body_block = match module.blocks[header_ascending_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected ascending header branch"),
    };

    let body_statements = &module.blocks[body_block.0 as usize].statements;
    let assign_count = body_statements
        .iter()
        .filter(|statement| matches!(statement.kind, HirStatementKind::Assign { .. }))
        .count();
    assert_eq!(
        assign_count, 2,
        "expected value + index binding assignments"
    );

    let step_block = match module.blocks[body_block.0 as usize].terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected body jump to step block"),
    };

    let has_index_increment =
        module.blocks[step_block.0 as usize]
            .statements
            .iter()
            .any(|statement| match &statement.kind {
                HirStatementKind::Assign { value, .. } => matches!(
                    &value.kind,
                    HirExpressionKind::BinOp {
                        op: HirBinOp::Add,
                        right,
                        ..
                    } if matches!(right.kind, HirExpressionKind::Int(1))
                ),
                _ => false,
            });

    assert!(
        has_index_increment,
        "expected explicit zero-based index increment in range step block"
    );
}

#[test]
fn preserves_runtime_zero_step_guard_for_dynamic_step() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(20);

    let step_symbol = super::symbol("step", &mut string_table);
    let step_decl = node(
        NodeKind::VariableDeclaration(Declaration {
            id: step_symbol.clone(),
            value: Expression::int(2, location.clone(), Ownership::ImmutableOwned),
        }),
        location.clone(),
    );

    let range_loop = node(
        NodeKind::RangeLoop {
            bindings: LoopBindings {
                item: loop_binding("value", DataType::Int, &mut string_table),
                index: None,
            },
            range: range_loop_spec(
                Expression::int(0, location.clone(), Ownership::ImmutableOwned),
                Expression::int(10, location.clone(), Ownership::ImmutableOwned),
                RangeEndKind::Exclusive,
                Some(Expression::reference(
                    step_symbol,
                    DataType::Int,
                    location.clone(),
                    Ownership::ImmutableReference,
                )),
            ),
            body: vec![],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![step_decl, range_loop],
        test_location(19),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("dynamic-step range loop lowering should succeed");

    let (step_zero_check_block, _, _, _, _) = range_loop_cfg_blocks(&module);

    let panic_block = match module.blocks[step_zero_check_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected runtime zero-check branch"),
    };

    assert!(matches!(
        module.blocks[panic_block.0 as usize].terminator,
        HirTerminator::Panic { message: Some(_) }
    ));
}

#[test]
fn lowers_collection_loop_to_explicit_cfg() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(30);

    let collection_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("item", DataType::Int, &mut string_table),
                index: None,
            },
            iterable: collection_literal(location.clone()),
            body: vec![],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![collection_loop],
        test_location(29),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("collection loop lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to collection header"),
    };

    let (body_block, exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected collection loop header conditional"),
    };

    let step_block = match module.blocks[body_block.0 as usize].terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected collection body jump to step block"),
    };

    assert!(matches!(
        module.blocks[step_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_block
    ));
    assert!(matches!(
        module.blocks[exit_block.0 as usize].terminator,
        HirTerminator::Return(_)
    ));
}

#[test]
fn lowers_collection_loop_item_binding_from_indexed_place() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(40);

    let collection_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("item", DataType::Int, &mut string_table),
                index: None,
            },
            iterable: collection_literal(location.clone()),
            body: vec![],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![collection_loop],
        test_location(39),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("collection loop lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to collection header"),
    };
    let body_block = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected collection loop header conditional"),
    };

    let has_indexed_item_assign =
        module.blocks[body_block.0 as usize]
            .statements
            .iter()
            .any(|statement| {
                matches!(
                    statement.kind,
                    HirStatementKind::Assign {
                        value: crate::compiler_frontend::hir::hir_nodes::HirExpression {
                            kind: HirExpressionKind::Load(HirPlace::Index { .. }),
                            ..
                        },
                        ..
                    }
                )
            });

    assert!(
        has_indexed_item_assign,
        "expected collection item binding to load from indexed place"
    );
}

#[test]
fn lowers_collection_loop_optional_index_binding() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(50);

    let collection_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("item", DataType::Int, &mut string_table),
                index: Some(loop_binding("index", DataType::Int, &mut string_table)),
            },
            iterable: collection_literal(location.clone()),
            body: vec![],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![collection_loop],
        test_location(49),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("collection loop lowering with index should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to collection header"),
    };
    let body_block = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected collection loop header conditional"),
    };

    let statements = &module.blocks[body_block.0 as usize].statements;
    let has_item_assign = statements.iter().any(|statement| {
        matches!(
            statement.kind,
            HirStatementKind::Assign {
                value: crate::compiler_frontend::hir::hir_nodes::HirExpression {
                    kind: HirExpressionKind::Load(HirPlace::Index { .. }),
                    ..
                },
                ..
            }
        )
    });
    let has_index_assign = statements.iter().any(|statement| {
        matches!(
            statement.kind,
            HirStatementKind::Assign {
                value: crate::compiler_frontend::hir::hir_nodes::HirExpression {
                    kind: HirExpressionKind::Load(HirPlace::Local(_)),
                    ..
                },
                ..
            }
        )
    });

    assert!(has_item_assign, "expected indexed item assignment");
    assert!(has_index_assign, "expected explicit user index assignment");
}

#[test]
fn break_targets_exit_block_in_collection_loop() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(60);

    let collection_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("item", DataType::Int, &mut string_table),
                index: None,
            },
            iterable: collection_literal(location.clone()),
            body: vec![node(
                NodeKind::If(
                    Expression::bool(true, location.clone(), Ownership::ImmutableOwned),
                    vec![node(NodeKind::Break, location.clone())],
                    None,
                ),
                location.clone(),
            )],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![collection_loop],
        test_location(59),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("collection loop lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to collection header"),
    };

    let (_, exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected collection loop header conditional"),
    };

    let break_targets_exit = module.blocks.iter().any(|block| {
        matches!(
            block.terminator,
            HirTerminator::Break { target } if target == exit_block
        )
    });

    assert!(
        break_targets_exit,
        "expected break terminator to target collection loop exit block"
    );
}

#[test]
fn continue_targets_step_block_in_collection_loop() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(70);

    let collection_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("item", DataType::Int, &mut string_table),
                index: None,
            },
            iterable: collection_literal(location.clone()),
            body: vec![node(NodeKind::Continue, location.clone())],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![collection_loop],
        test_location(69),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("collection loop lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to collection header"),
    };

    let body_block = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If { then_block, .. } => then_block,
        _ => panic!("expected collection loop header conditional"),
    };

    let step_block = match module.blocks[body_block.0 as usize].terminator {
        HirTerminator::Continue { target } => target,
        _ => panic!("expected continue terminator in collection body"),
    };

    assert!(matches!(
        module.blocks[step_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_block
    ));
}

#[test]
fn nested_loop_targets_remain_correct() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let location = test_location(80);

    let inner_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("inner_item", DataType::Int, &mut string_table),
                index: None,
            },
            iterable: collection_literal(location.clone()),
            body: vec![node(NodeKind::Continue, location.clone())],
        },
        location.clone(),
    );

    let outer_loop = node(
        NodeKind::CollectionLoop {
            bindings: LoopBindings {
                item: loop_binding("outer_item", DataType::Int, &mut string_table),
                index: None,
            },
            iterable: collection_literal(location.clone()),
            body: vec![inner_loop, node(NodeKind::Continue, location.clone())],
        },
        location,
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![outer_loop],
        test_location(79),
    );

    let module = lower_ast(build_ast(vec![start_fn], entry_path), &mut string_table)
        .expect("nested collection loop lowering should succeed");

    let continue_targets = module
        .blocks
        .iter()
        .filter_map(|block| match block.terminator {
            HirTerminator::Continue { target } => Some(target),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert!(
        continue_targets.len() >= 2,
        "expected at least one continue for each nested loop"
    );

    let unique_targets = continue_targets
        .iter()
        .map(|target| target.0)
        .collect::<std::collections::BTreeSet<_>>();
    assert!(
        unique_targets.len() >= 2,
        "nested loops should continue to distinct step blocks"
    );

    for target in continue_targets {
        assert!(matches!(
            module.blocks[target.0 as usize].terminator,
            HirTerminator::Jump { .. }
        ));
    }
}
