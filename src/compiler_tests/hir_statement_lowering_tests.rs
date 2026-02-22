#![cfg(test)]

use std::path::Path;

use crate::backends::function_registry::HostFunctionId;
use crate::compiler_frontend::ast::ast::{Ast, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, NodeKind, TextLocation, Var};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::{CompilerMessages, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpressionKind, HirPattern, HirStatementKind, HirTerminator,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::{StringId, StringTable};
use crate::projects::settings::IMPLICIT_START_FUNC_NAME;

fn test_location(line: i32) -> TextLocation {
    TextLocation::new_just_line(line)
}

fn node(kind: NodeKind, location: TextLocation) -> AstNode {
    AstNode {
        kind,
        location,
        scope: InternedPath::new(),
    }
}

fn var(name: StringId, value: Expression) -> Var {
    Var { id: name, value }
}

fn param(name: StringId, data_type: DataType, mutable: bool, location: TextLocation) -> Var {
    let ownership = if mutable {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Var {
        id: name,
        value: Expression::new(ExpressionKind::None, location, data_type, ownership),
    }
}

fn function_node(
    name: StringId,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: TextLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        warnings: vec![],
    }
}

fn entry_path_and_start_name(string_table: &mut StringTable) -> (InternedPath, StringId) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path
        .join_str(IMPLICIT_START_FUNC_NAME, string_table)
        .extract_header_name(string_table);

    (entry_path, start_name)
}

fn lower_ast(
    ast: Ast,
    string_table: &mut StringTable,
) -> Result<crate::compiler_frontend::hir::hir_nodes::HirModule, CompilerMessages> {
    HirBuilder::new(string_table).build_hir_module(ast)
}

#[test]
fn registers_declarations_and_resolves_start_function() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let field_name = string_table.intern("field");
    let struct_name = string_table.intern("MyStruct");

    let struct_node = node(
        NodeKind::StructDefinition(
            struct_name,
            vec![var(
                field_name,
                Expression::new(
                    ExpressionKind::None,
                    test_location(1),
                    DataType::Int,
                    Ownership::ImmutableOwned,
                ),
            )],
        ),
        test_location(1),
    );

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(2),
    );

    let ast = build_ast(vec![struct_node, start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    assert_eq!(module.structs.len(), 1);
    assert_eq!(module.functions.len(), 1);
    assert_eq!(
        module.side_table.function_name_id(module.start_function),
        Some(start_name)
    );
}

#[test]
fn allocates_parameter_locals_and_binds_names() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = string_table.intern("x");

    let body = vec![node(
        NodeKind::Return(vec![Expression::reference(
            x,
            DataType::Int,
            test_location(3),
            Ownership::ImmutableReference,
        )]),
        test_location(3),
    )];

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x, DataType::Int, false, test_location(2))],
            returns: vec![DataType::Int],
        },
        body,
        test_location(2),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_fn = &module.functions[module.start_function.0 as usize];
    assert_eq!(start_fn.params.len(), 1);

    let entry_block = &module.blocks[start_fn.entry.0 as usize];
    assert_eq!(entry_block.locals.len(), 1);
    assert_eq!(
        module
            .side_table
            .resolve_local_name(start_fn.params[0], &string_table),
        Some("x")
    );
}

#[test]
fn variable_declaration_emits_local_and_assign_statement() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = string_table.intern("x");

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::VariableDeclaration(var(
                x,
                Expression::int(42, test_location(4), Ownership::ImmutableOwned),
            )),
            test_location(4),
        )],
        test_location(3),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_fn = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start_fn.entry.0 as usize];

    assert_eq!(entry_block.locals.len(), 1);
    assert!(
        entry_block
            .statements
            .iter()
            .any(|statement| matches!(statement.kind, HirStatementKind::Assign { .. }))
    );
}

#[test]
fn assignment_lowers_value_prelude_before_assign() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = string_table.intern("x");
    let helper = string_table.intern("helper");

    let helper_fn = function_node(
        helper,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::Return(vec![Expression::int(
                1,
                test_location(1),
                Ownership::ImmutableOwned,
            )]),
            test_location(1),
        )],
        test_location(1),
    );

    let target_node = node(
        NodeKind::Rvalue(Expression::reference(
            x,
            DataType::Int,
            test_location(5),
            Ownership::MutableReference,
        )),
        test_location(5),
    );

    let assignment = node(
        NodeKind::Assignment {
            target: Box::new(target_node),
            value: Expression::function_call(helper, vec![], vec![DataType::Int], test_location(5)),
        },
        test_location(5),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x, DataType::Int, true, test_location(4))],
            returns: vec![],
        },
        vec![assignment],
        test_location(4),
    );

    let ast = build_ast(vec![helper_fn, start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let block = &module.blocks[start.entry.0 as usize];

    assert!(matches!(
        block.statements.get(0).map(|statement| &statement.kind),
        Some(HirStatementKind::Call {
            result: Some(_),
            ..
        })
    ));
    assert!(matches!(
        block.statements.get(1).map(|statement| &statement.kind),
        Some(HirStatementKind::Assign { .. })
    ));
}

#[test]
fn call_statements_emit_without_result_binding() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let callee = string_table.intern("callee");

    let callee_fn = function_node(
        callee,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::Return(vec![Expression::int(
                9,
                test_location(1),
                Ownership::ImmutableOwned,
            )]),
            test_location(1),
        )],
        test_location(1),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::FunctionCall {
                    name: callee,
                    args: vec![],
                    returns: vec![DataType::Int],
                    location: test_location(2),
                },
                test_location(2),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: HostFunctionId::Alloc,
                    args: vec![Expression::int(
                        1,
                        test_location(3),
                        Ownership::ImmutableOwned,
                    )],
                    returns: vec![DataType::Int],
                    location: test_location(3),
                },
                test_location(3),
            ),
        ],
        test_location(2),
    );

    let ast = build_ast(vec![callee_fn, start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let block = &module.blocks[start.entry.0 as usize];

    let call_results = block
        .statements
        .iter()
        .filter_map(|statement| match statement.kind {
            HirStatementKind::Call { result, .. } => Some(result),
            _ => None,
        })
        .collect::<Vec<_>>();

    assert_eq!(call_results, vec![None, None]);
}

#[test]
fn return_lowering_handles_zero_one_and_many_values() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let one_name = string_table.intern("one");
    let many_name = string_table.intern("many");

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Return(vec![]), test_location(1))],
        test_location(1),
    );

    let one_fn = function_node(
        one_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::Return(vec![Expression::int(
                8,
                test_location(2),
                Ownership::ImmutableOwned,
            )]),
            test_location(2),
        )],
        test_location(2),
    );

    let many_fn = function_node(
        many_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::Int, DataType::Bool],
        },
        vec![node(
            NodeKind::Return(vec![
                Expression::int(1, test_location(3), Ownership::ImmutableOwned),
                Expression::bool(true, test_location(3), Ownership::ImmutableOwned),
            ]),
            test_location(3),
        )],
        test_location(3),
    );

    let ast = build_ast(vec![start_fn, one_fn, many_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_block =
        &module.blocks[module.functions[module.start_function.0 as usize].entry.0 as usize];
    assert!(matches!(
        &start_block.terminator,
        HirTerminator::Return(value)
            if matches!(
                &value.kind,
                HirExpressionKind::TupleConstruct { elements } if elements.is_empty()
            )
    ));

    let one_block = &module.blocks[module.functions[1].entry.0 as usize];
    assert!(matches!(
        &one_block.terminator,
        HirTerminator::Return(value)
            if matches!(&value.kind, HirExpressionKind::Int(8))
    ));

    let many_block = &module.blocks[module.functions[2].entry.0 as usize];
    assert!(matches!(
        &many_block.terminator,
        HirTerminator::Return(value)
            if matches!(
                &value.kind,
                HirExpressionKind::TupleConstruct { elements } if elements.len() == 2
            )
    ));
}

#[test]
fn lowers_if_to_then_else_merge_blocks() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = string_table.intern("x");
    let y = string_table.intern("y");

    let if_node = node(
        NodeKind::If(
            Expression::bool(true, test_location(2), Ownership::ImmutableOwned),
            vec![node(
                NodeKind::VariableDeclaration(var(
                    x,
                    Expression::int(1, test_location(2), Ownership::ImmutableOwned),
                )),
                test_location(2),
            )],
            Some(vec![node(
                NodeKind::VariableDeclaration(var(
                    y,
                    Expression::int(2, test_location(3), Ownership::ImmutableOwned),
                )),
                test_location(3),
            )]),
        ),
        test_location(2),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![if_node],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let (then_block, else_block) = match entry_block.terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected if terminator in entry block"),
    };

    assert!(matches!(
        module.blocks[then_block.0 as usize].terminator,
        HirTerminator::Jump { .. }
    ));
    assert!(matches!(
        module.blocks[else_block.0 as usize].terminator,
        HirTerminator::Jump { .. }
    ));
}

#[test]
fn non_unit_function_with_terminal_if_does_not_report_fallthrough() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let chooser = string_table.intern("chooser");

    let chooser_fn = function_node(
        chooser,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::If(
                Expression::bool(true, test_location(8), Ownership::ImmutableOwned),
                vec![node(
                    NodeKind::Return(vec![Expression::int(
                        1,
                        test_location(8),
                        Ownership::ImmutableOwned,
                    )]),
                    test_location(8),
                )],
                Some(vec![node(
                    NodeKind::Return(vec![Expression::int(
                        2,
                        test_location(9),
                        Ownership::ImmutableOwned,
                    )]),
                    test_location(9),
                )]),
            ),
            test_location(8),
        )],
        test_location(7),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn, chooser_fn], entry_path);
    let module =
        lower_ast(ast, &mut string_table).expect("all-terminal if should not trigger fallthrough");

    let chooser_block = &module.blocks[module.functions[1].entry.0 as usize];
    assert!(matches!(chooser_block.terminator, HirTerminator::If { .. }));
}

#[test]
fn lowers_while_to_header_body_exit_shape() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let while_node = node(
        NodeKind::WhileLoop(
            Expression::bool(false, test_location(2), Ownership::ImmutableOwned),
            vec![node(
                NodeKind::Rvalue(Expression::int(
                    10,
                    test_location(2),
                    Ownership::ImmutableOwned,
                )),
                test_location(2),
            )],
        ),
        test_location(2),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![while_node],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to while header"),
    };

    let (body_block, _exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected if in while header"),
    };

    assert!(matches!(
        module.blocks[body_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_block
    ));
}

#[test]
fn non_unit_function_with_terminal_match_default_does_not_report_fallthrough() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let chooser = string_table.intern("choose_match");
    let x = string_table.intern("x");

    let chooser_fn = function_node(
        chooser,
        FunctionSignature {
            parameters: vec![param(x, DataType::Int, false, test_location(10))],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::Match(
                Expression::reference(
                    x,
                    DataType::Int,
                    test_location(11),
                    Ownership::ImmutableReference,
                ),
                vec![MatchArm {
                    condition: Expression::int(1, test_location(11), Ownership::ImmutableOwned),
                    body: vec![node(
                        NodeKind::Return(vec![Expression::int(
                            1,
                            test_location(11),
                            Ownership::ImmutableOwned,
                        )]),
                        test_location(11),
                    )],
                }],
                Some(vec![node(
                    NodeKind::Return(vec![Expression::int(
                        2,
                        test_location(12),
                        Ownership::ImmutableOwned,
                    )]),
                    test_location(12),
                )]),
            ),
            test_location(11),
        )],
        test_location(10),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn, chooser_fn], entry_path);
    let module = lower_ast(ast, &mut string_table)
        .expect("all-terminal match arms should not trigger fallthrough");

    let chooser_block = &module.blocks[module.functions[1].entry.0 as usize];
    assert!(matches!(
        chooser_block.terminator,
        HirTerminator::Match { .. }
    ));
}

#[test]
fn lowers_match_with_literal_arms_and_synthesized_wildcard_default() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = string_table.intern("x");

    let match_node = node(
        NodeKind::Match(
            Expression::reference(
                x,
                DataType::Int,
                test_location(3),
                Ownership::ImmutableReference,
            ),
            vec![
                MatchArm {
                    condition: Expression::int(1, test_location(3), Ownership::ImmutableOwned),
                    body: vec![node(
                        NodeKind::Rvalue(Expression::int(
                            9,
                            test_location(3),
                            Ownership::ImmutableOwned,
                        )),
                        test_location(3),
                    )],
                },
                MatchArm {
                    condition: Expression::int(2, test_location(3), Ownership::ImmutableOwned),
                    body: vec![node(
                        NodeKind::Rvalue(Expression::int(
                            8,
                            test_location(3),
                            Ownership::ImmutableOwned,
                        )),
                        test_location(3),
                    )],
                },
            ],
            None,
        ),
        test_location(3),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x, DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![match_node],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];

    let arms = match &entry_block.terminator {
        HirTerminator::Match { arms, .. } => arms,
        _ => panic!("expected match terminator"),
    };

    assert_eq!(arms.len(), 3);
    assert!(matches!(arms[0].pattern, HirPattern::Literal(_)));
    assert!(matches!(arms[1].pattern, HirPattern::Literal(_)));
    assert!(matches!(arms[2].pattern, HirPattern::Wildcard));
}

#[test]
fn for_loop_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let i = string_table.intern("i");

    let for_loop = node(
        NodeKind::ForLoop(
            Box::new(var(
                i,
                Expression::new(
                    ExpressionKind::None,
                    test_location(2),
                    DataType::Int,
                    Ownership::ImmutableOwned,
                ),
            )),
            Expression::range(
                Expression::int(0, test_location(2), Ownership::ImmutableOwned),
                Expression::int(3, test_location(2), Ownership::ImmutableOwned),
                test_location(2),
                Ownership::ImmutableOwned,
            ),
            vec![],
        ),
        test_location(2),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![for_loop],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("for-loop should fail in this phase");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("For-loop lowering"));
}

#[test]
fn top_level_return_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let top_level_return = node(NodeKind::Return(vec![]), test_location(2));

    let ast = build_ast(vec![start_fn, top_level_return], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("top-level return should fail");

    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("Top-level return"));
}

#[test]
fn enforces_non_unit_fallthrough_and_unit_implicit_return() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let non_unit_name = string_table.intern("non_unit");

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let non_unit_fn = function_node(
        non_unit_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::Int],
        },
        vec![],
        test_location(2),
    );

    let ast_err = build_ast(vec![start_fn.clone(), non_unit_fn], entry_path.clone());
    let err = lower_ast(ast_err, &mut string_table).expect_err("non-unit fallthrough should fail");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("fall through"));

    let ast_ok = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast_ok, &mut string_table).expect("unit fallthrough should succeed");
    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    assert!(matches!(entry_block.terminator, HirTerminator::Return(_)));
}

#[test]
fn side_table_maps_statement_and_terminator_locations() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = string_table.intern("x");

    let decl_loc = test_location(4);
    let ret_loc = test_location(5);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x,
                    Expression::int(1, decl_loc.clone(), Ownership::ImmutableOwned),
                )),
                decl_loc.clone(),
            ),
            node(NodeKind::Return(vec![]), ret_loc.clone()),
        ],
        test_location(3),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let decl_mappings = module.side_table.hir_locations_for_ast(&decl_loc);
    assert!(!decl_mappings.is_empty());

    let ret_mappings = module.side_table.hir_locations_for_ast(&ret_loc);
    assert!(!ret_mappings.is_empty());
}
