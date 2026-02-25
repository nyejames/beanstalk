#![cfg(test)]

use crate::compiler_frontend::ast::ast::{Ast, ModuleExport};
use crate::compiler_frontend::ast::ast_nodes::{AstNode, Declaration, NodeKind, TextLocation};
use crate::compiler_frontend::ast::expressions::expression::{Expression, ExpressionKind};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::ast::templates::create_template_node::Template;
use crate::compiler_frontend::compiler_errors::{CompilerMessages, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_builder::HirBuilder;
use crate::compiler_frontend::hir::hir_nodes::{
    HirExpressionKind, HirModule, HirPattern, HirStatementKind, HirTerminator,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::projects::settings::{IMPLICIT_START_FUNC_NAME, TOP_LEVEL_TEMPLATE_NAME};

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

fn var(name: InternedPath, value: Expression) -> Declaration {
    Declaration { id: name, value }
}

fn param(
    name: InternedPath,
    data_type: DataType,
    mutable: bool,
    location: TextLocation,
) -> Declaration {
    let ownership = if mutable {
        Ownership::MutableOwned
    } else {
        Ownership::ImmutableOwned
    };

    Declaration {
        id: name,
        value: Expression::new(ExpressionKind::None, location, data_type, ownership),
    }
}

fn function_node(
    name: InternedPath,
    signature: FunctionSignature,
    body: Vec<AstNode>,
    location: TextLocation,
) -> AstNode {
    node(NodeKind::Function(name, signature, body), location)
}

fn symbol(name: &str, string_table: &mut StringTable) -> InternedPath {
    InternedPath::from_single_str(name, string_table)
}

fn runtime_template_expression(location: TextLocation, content: Vec<Expression>) -> Expression {
    let mut template = Template::create_default(None);
    template.location = location.clone();

    for expression in content {
        template.content.add(expression, false);
    }

    Expression::template(template, Ownership::ImmutableOwned)
}

fn build_ast(nodes: Vec<AstNode>, entry_path: InternedPath) -> Ast {
    Ast {
        nodes,
        entry_path,
        external_exports: Vec::<ModuleExport>::new(),
        warnings: vec![],
    }
}

fn entry_path_and_start_name(string_table: &mut StringTable) -> (InternedPath, InternedPath) {
    let entry_path = InternedPath::from_single_str("main.bst", string_table);
    let start_name = entry_path.join_str(IMPLICIT_START_FUNC_NAME, string_table);

    (entry_path, start_name)
}

fn lower_ast(
    ast: Ast,
    string_table: &mut StringTable,
) -> Result<crate::compiler_frontend::hir::hir_nodes::HirModule, CompilerMessages> {
    HirBuilder::new(string_table).build_hir_module(ast)
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

#[test]
fn registers_declarations_and_resolves_start_function() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let struct_name = symbol("MyStruct", &mut string_table);
    let field_name = struct_name.append(string_table.intern("field"));

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
        start_name.clone(),
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
        module
            .side_table
            .function_name_path(module.start_function)
            .cloned(),
        Some(start_name)
    );
}

#[test]
fn allocates_parameter_locals_and_binds_names() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = symbol("x", &mut string_table);

    let body = vec![node(
        NodeKind::Return(vec![Expression::reference(
            x.clone(),
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
    let x = symbol("x", &mut string_table);

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
fn start_function_with_no_template_declaration_returns_empty_string() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let template_name = symbol(TOP_LEVEL_TEMPLATE_NAME, &mut string_table);

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::StringSlice],
        },
        vec![node(
            NodeKind::Return(vec![Expression::reference(
                template_name,
                DataType::Template,
                test_location(2),
                Ownership::ImmutableReference,
            )]),
            test_location(2),
        )],
        test_location(1),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_fn = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start_fn.entry.0 as usize];
    assert_eq!(entry_block.locals.len(), 1);
    assert_eq!(
        module
            .side_table
            .resolve_local_name(entry_block.locals[0].id, &string_table),
        Some(TOP_LEVEL_TEMPLATE_NAME)
    );

    assert!(matches!(
        entry_block.statements.first().map(|statement| &statement.kind),
        Some(HirStatementKind::Assign { value, .. })
            if matches!(&value.kind, HirExpressionKind::StringLiteral(value) if value.is_empty())
    ));
    assert!(matches!(
        &entry_block.terminator,
        HirTerminator::Return(value) if matches!(&value.kind, HirExpressionKind::Load(_))
    ));
}

#[test]
fn start_function_accumulates_multiple_top_level_templates_in_source_order() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let template_name = symbol(TOP_LEVEL_TEMPLATE_NAME, &mut string_table);
    let first = string_table.intern("First");
    let second = string_table.intern("Second");

    let first_template = runtime_template_expression(
        test_location(2),
        vec![Expression::string_slice(
            first,
            test_location(2),
            Ownership::ImmutableOwned,
        )],
    );

    let second_template = runtime_template_expression(
        test_location(3),
        vec![Expression::string_slice(
            second,
            test_location(3),
            Ownership::ImmutableOwned,
        )],
    );

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::StringSlice],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(template_name.clone(), first_template)),
                test_location(2),
            ),
            node(
                NodeKind::VariableDeclaration(var(template_name.clone(), second_template)),
                test_location(3),
            ),
            node(
                NodeKind::Return(vec![Expression::reference(
                    template_name,
                    DataType::Template,
                    test_location(4),
                    Ownership::ImmutableReference,
                )]),
                test_location(4),
            ),
        ],
        test_location(1),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_fn = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start_fn.entry.0 as usize];
    let template_locals = entry_block
        .locals
        .iter()
        .filter(|local| {
            module
                .side_table
                .resolve_local_name(local.id, &string_table)
                .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME)
        })
        .count();
    assert_eq!(template_locals, 1);
    assert_eq!(entry_block.statements.len(), 7);

    let template_calls = entry_block
        .statements
        .iter()
        .filter_map(|statement| match &statement.kind {
            HirStatementKind::Call { args, .. } => Some(args),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(template_calls.len(), 2);
    assert!(matches!(
        template_calls[0][0].kind,
        HirExpressionKind::StringLiteral(ref value) if value == "First"
    ));
    assert!(matches!(
        template_calls[1][0].kind,
        HirExpressionKind::StringLiteral(ref value) if value == "Second"
    ));
}

#[test]
fn top_level_template_declarations_do_not_redeclare_accumulator_local() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let template_name = symbol(TOP_LEVEL_TEMPLATE_NAME, &mut string_table);
    let one = string_table.intern("One");
    let two = string_table.intern("Two");

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![DataType::StringSlice],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    template_name.clone(),
                    runtime_template_expression(
                        test_location(2),
                        vec![Expression::string_slice(
                            one,
                            test_location(2),
                            Ownership::ImmutableOwned,
                        )],
                    ),
                )),
                test_location(2),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    template_name.clone(),
                    runtime_template_expression(
                        test_location(3),
                        vec![Expression::string_slice(
                            two,
                            test_location(3),
                            Ownership::ImmutableOwned,
                        )],
                    ),
                )),
                test_location(3),
            ),
            node(
                NodeKind::Return(vec![Expression::reference(
                    template_name,
                    DataType::Template,
                    test_location(4),
                    Ownership::ImmutableReference,
                )]),
                test_location(4),
            ),
        ],
        test_location(1),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_fn = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start_fn.entry.0 as usize];

    let template_locals = entry_block
        .locals
        .iter()
        .filter(|local| {
            module
                .side_table
                .resolve_local_name(local.id, &string_table)
                .is_some_and(|name| name == TOP_LEVEL_TEMPLATE_NAME)
        })
        .count();

    assert_eq!(template_locals, 1);
}

#[test]
fn assignment_lowers_value_prelude_before_assign() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = symbol("x", &mut string_table);
    let helper = symbol("helper", &mut string_table);

    let helper_fn = function_node(
        helper.clone(),
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
            x.clone(),
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
        block.statements.first().map(|statement| &statement.kind),
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
    let callee = symbol("callee", &mut string_table);
    let alloc = symbol("alloc", &mut string_table);

    let callee_fn = function_node(
        callee.clone(),
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
                    name: alloc,
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
    let one_name = symbol("one", &mut string_table);
    let many_name = symbol("many", &mut string_table);

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
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

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
    let chooser = symbol("chooser", &mut string_table);

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
    assert_no_placeholder_terminators(&module);
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
fn break_in_while_targets_loop_exit_block() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let while_node = node(
        NodeKind::WhileLoop(
            Expression::bool(true, test_location(20), Ownership::ImmutableOwned),
            vec![node(NodeKind::Break, test_location(21))],
        ),
        test_location(20),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![while_node],
        test_location(19),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected jump to while header"),
    };

    let (body_block, exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected while header conditional terminator"),
    };

    assert!(matches!(
        module.blocks[body_block.0 as usize].terminator,
        HirTerminator::Break { target } if target == exit_block
    ));
}

#[test]
fn continue_in_for_targets_step_block() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let i = symbol("i", &mut string_table);

    let for_node = node(
        NodeKind::ForLoop(
            Box::new(var(
                i,
                Expression::new(
                    ExpressionKind::None,
                    test_location(30),
                    DataType::Int,
                    Ownership::ImmutableOwned,
                ),
            )),
            Expression::range(
                Expression::int(0, test_location(30), Ownership::ImmutableOwned),
                Expression::int(2, test_location(30), Ownership::ImmutableOwned),
                test_location(30),
                Ownership::ImmutableOwned,
            ),
            vec![node(NodeKind::Continue, test_location(31))],
        ),
        test_location(30),
    );

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![for_node],
        test_location(29),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("for-loop lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected entry jump to for header"),
    };

    let (body_block, _exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected for header conditional terminator"),
    };

    let step_block = match module.blocks[body_block.0 as usize].terminator {
        HirTerminator::Continue { target } => target,
        _ => panic!("expected continue terminator in for body"),
    };

    assert!(matches!(
        module.blocks[step_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_block
    ));
}

#[test]
fn non_unit_function_with_terminal_match_default_does_not_report_fallthrough() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let chooser = symbol("choose_match", &mut string_table);
    let x = symbol("x", &mut string_table);

    let chooser_fn = function_node(
        chooser,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(10))],
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
    assert_no_placeholder_terminators(&module);
}

#[test]
fn lowers_match_with_literal_arms_and_synthesized_wildcard_default() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = symbol("x", &mut string_table);

    let match_node = node(
        NodeKind::Match(
            Expression::reference(
                x.clone(),
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
fn match_rejects_non_literal_pattern_expressions() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let x = symbol("x", &mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![param(x.clone(), DataType::Int, false, test_location(2))],
            returns: vec![],
        },
        vec![node(
            NodeKind::Match(
                Expression::reference(
                    x.clone(),
                    DataType::Int,
                    test_location(3),
                    Ownership::ImmutableReference,
                ),
                vec![MatchArm {
                    condition: Expression::reference(
                        x,
                        DataType::Int,
                        test_location(3),
                        Ownership::ImmutableReference,
                    ),
                    body: vec![],
                }],
                None,
            ),
            test_location(3),
        )],
        test_location(2),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table)
        .expect_err("non-literal match pattern should fail HIR lowering");

    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(
        err.errors[0]
            .msg
            .contains("Match arm patterns must be compile-time literals"),
        "unexpected error message: {}",
        err.errors[0].msg
    );
}

#[test]
fn for_loop_lowers_to_header_body_step_exit_shape() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);
    let i = symbol("i", &mut string_table);

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
    let module = lower_ast(ast, &mut string_table).expect("for-loop lowering should succeed");

    let start = &module.functions[module.start_function.0 as usize];
    let entry_block = &module.blocks[start.entry.0 as usize];
    let header_block = match entry_block.terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected entry jump to for header"),
    };

    let (body_block, exit_block) = match module.blocks[header_block.0 as usize].terminator {
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => (then_block, else_block),
        _ => panic!("expected for header to lower to conditional terminator"),
    };

    let step_block = match module.blocks[body_block.0 as usize].terminator {
        HirTerminator::Jump { target, .. } => target,
        _ => panic!("expected for body to jump to step block"),
    };

    assert!(matches!(
        module.blocks[step_block.0 as usize].terminator,
        HirTerminator::Jump { target, .. } if target == header_block
    ));
    assert!(matches!(
        module.blocks[exit_block.0 as usize].terminator,
        HirTerminator::Return(_)
    ));
    assert_no_placeholder_terminators(&module);
}

#[test]
fn break_outside_loop_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Break, test_location(2))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("break outside loop should fail");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("active loop context"));
}

#[test]
fn continue_outside_loop_reports_hir_transformation_error() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_path_and_start_name(&mut string_table);

    let start_fn = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(NodeKind::Continue, test_location(2))],
        test_location(1),
    );

    let ast = build_ast(vec![start_fn], entry_path);
    let err = lower_ast(ast, &mut string_table).expect_err("continue outside loop should fail");
    assert_eq!(err.errors[0].error_type, ErrorType::HirTransformation);
    assert!(err.errors[0].msg.contains("active loop context"));
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
    let non_unit_name = symbol("non_unit", &mut string_table);

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
    let x = symbol("x", &mut string_table);

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
