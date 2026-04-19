//! HIR statement lowering regression tests.
//!
//! WHAT: checks how statement-level AST nodes become HIR blocks, statements, and terminators.
//! WHY: statement lowering owns most CFG construction and benefits from targeted regression coverage.

use crate::compiler_frontend::ast::ast_nodes::{
    AstNode, MultiBindTarget, MultiBindTargetKind, NodeKind,
};
use crate::compiler_frontend::ast::expressions::call_argument::{CallAccessMode, CallArgument};
use crate::compiler_frontend::ast::expressions::expression::{
    Expression, ExpressionKind, Operator, ResultCallHandling,
};
use crate::compiler_frontend::ast::statements::branching::MatchArm;
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::ast::templates::template_types::Template;
use crate::compiler_frontend::ast::{Ast, AstDocFragment, AstDocFragmentKind};
use crate::compiler_frontend::compiler_errors::{CompilerMessages, ErrorType};
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::hir::hir_nodes::{
    FunctionId, HirConstValue, HirDocFragmentKind, HirExpressionKind, HirModule, HirPattern,
    HirPlace, HirStatementKind, HirTerminator,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    fresh_returns, function_node, make_test_variable, node, param, runtime_function_call_node,
    runtime_operator_node, test_location,
};

use crate::compiler_frontend::hir::hir_builder::{
    assert_no_placeholder_terminators, build_ast, lower_ast,
};

#[test]
fn allocates_parameter_locals_and_binds_names() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

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
            returns: fresh_returns(vec![DataType::Int]),
        },
        body,
        test_location(2),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let module = lower_ast(ast, &mut string_table).expect("HIR lowering should succeed");

    let start_fn = &module.functions[module.start_function.0 as usize];
    assert_eq!(start_fn.params.len(), 1);

    let entry_block = &module.blocks[start_fn.entry.0 as usize];
    assert!(entry_block.locals.len() >= 1);
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
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::VariableDeclaration(make_test_variable(
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

    assert!(entry_block.locals.len() >= 1);
    assert!(
        entry_block
            .statements
            .iter()
            .any(|statement| matches!(statement.kind, HirStatementKind::Assign { .. }))
    );
}

#[test]
fn duplicate_local_declarations_in_same_scope_fail() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let var_name = super::symbol("my_var", &mut string_table);

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    var_name.clone(),
                    Expression::int(1, test_location(2), Ownership::ImmutableOwned),
                )),
                test_location(2),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    var_name.clone(),
                    Expression::int(2, test_location(3), Ownership::ImmutableOwned),
                )),
                test_location(3),
            ),
        ],
        test_location(1),
    );

    let ast = build_ast(vec![start_function], entry_path);
    let error = lower_ast(ast, &mut string_table).expect_err("duplicate symbol should fail");
    assert!(error.errors.iter().any(|item| {
        item.msg
            .contains("Local 'my_var' is already declared in this function scope")
    }));
}

#[test]
fn assignment_lowers_value_prelude_before_assign() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let x = super::symbol("x", &mut string_table);
    let helper = super::symbol("helper", &mut string_table);

    let helper_fn = function_node(
        helper.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: fresh_returns(vec![DataType::Int]),
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

    let call_pos = block
        .statements
        .iter()
        .position(|statement| {
            matches!(
                &statement.kind,
                HirStatementKind::Call {
                    result: Some(_),
                    ..
                }
            )
        })
        .expect("entry block should contain a Call statement with a result");
    let assign_pos = block
        .statements
        .iter()
        .rposition(|statement| matches!(&statement.kind, HirStatementKind::Assign { .. }))
        .expect("entry block should contain an Assign statement");
    assert!(
        call_pos < assign_pos,
        "Call prelude must precede the final Assign"
    );
}

#[test]
fn call_statements_emit_without_result_binding() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let callee = super::symbol("callee", &mut string_table);
    let alloc = super::symbol("alloc", &mut string_table);

    let callee_fn = function_node(
        callee.clone(),
        FunctionSignature {
            parameters: vec![],
            returns: fresh_returns(vec![DataType::Int]),
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
                    result_types: vec![DataType::Int],
                    location: test_location(2),
                },
                test_location(2),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: alloc,
                    args: vec![CallArgument::positional(
                        Expression::int(1, test_location(3), Ownership::ImmutableOwned),
                        CallAccessMode::Shared,
                        test_location(3),
                    )],
                    result_types: vec![DataType::Int],
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
    let (entry_path, start_name) = super::entry_path_and_start_name(&mut string_table);
    let one_name = super::symbol("one", &mut string_table);
    let many_name = super::symbol("many", &mut string_table);

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
            returns: fresh_returns(vec![DataType::Int]),
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
            returns: fresh_returns(vec![DataType::Int, DataType::Bool]),
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
