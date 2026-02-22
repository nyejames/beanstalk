#![cfg(test)]

use crate::backends::function_registry::{HostAbiType, HostAccessKind, HostReturnAlias};
use crate::compiler_frontend::analysis::borrow_checker::tests::test_support::{
    assignment_target, build_ast, default_host_registry, entry_and_start, function_node, location,
    lower_hir, node, param, reference_expr, register_host_function, run_borrow_checker, symbol,
    var,
};
use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::expression::Expression;
use crate::compiler_frontend::ast::statements::functions::FunctionSignature;
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::{DataType, Ownership};
use crate::compiler_frontend::string_interning::StringTable;

#[test]
fn user_function_returning_param_aliases_caller_root() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

    let alias_fn = symbol("alias_fn", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        alias_fn.clone(),
        FunctionSignature {
            parameters: vec![param(p.clone(), DataType::Int, false, location(1))],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::Return(vec![reference_expr(p, DataType::Int, location(2))]),
            location(2),
        )],
        location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(10), Ownership::MutableOwned),
                )),
                location(10),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y,
                    Expression::function_call(
                        alias_fn,
                        vec![reference_expr(x.clone(), DataType::Int, location(11))],
                        vec![DataType::Int],
                        location(11),
                    ),
                )),
                location(11),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(12))),
                    value: Expression::int(2, location(12), Ownership::ImmutableOwned),
                },
                location(12),
            ),
        ],
        location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    let error = run_borrow_checker(&hir, &host_registry, &string_table)
        .expect_err("callee return alias should keep caller root aliased");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("may alias"));
}

#[test]
fn fresh_user_return_does_not_alias_caller_roots() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

    let fresh_fn = symbol("fresh_fn", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        fresh_fn.clone(),
        FunctionSignature {
            parameters: vec![param(p, DataType::Int, false, location(1))],
            returns: vec![DataType::Int],
        },
        vec![node(
            NodeKind::Return(vec![Expression::int(
                42,
                location(2),
                Ownership::ImmutableOwned,
            )]),
            location(2),
        )],
        location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(10), Ownership::MutableOwned),
                )),
                location(10),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y,
                    Expression::function_call(
                        fresh_fn,
                        vec![reference_expr(x.clone(), DataType::Int, location(11))],
                        vec![DataType::Int],
                        location(11),
                    ),
                )),
                location(11),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(12))),
                    value: Expression::int(2, location(12), Ownership::ImmutableOwned),
                },
                location(12),
            ),
        ],
        location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &host_registry, &string_table)
        .expect("fresh callee returns should not alias caller roots");
}

#[test]
fn unknown_user_return_is_conservatively_aliased() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let host_registry = default_host_registry(&mut string_table);

    let unknown_fn = symbol("unknown_fn", &mut string_table);
    let p = symbol("p", &mut string_table);
    let q = symbol("q", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        unknown_fn.clone(),
        FunctionSignature {
            parameters: vec![param(p.clone(), DataType::Int, false, location(1))],
            returns: vec![DataType::Int],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    q.clone(),
                    reference_expr(p, DataType::Int, location(2)),
                )),
                location(2),
            ),
            node(
                NodeKind::Return(vec![reference_expr(q, DataType::Int, location(3))]),
                location(3),
            ),
        ],
        location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(10), Ownership::MutableOwned),
                )),
                location(10),
            ),
            node(
                NodeKind::VariableDeclaration(var(
                    y,
                    Expression::function_call(
                        unknown_fn,
                        vec![reference_expr(x.clone(), DataType::Int, location(11))],
                        vec![DataType::Int],
                        location(11),
                    ),
                )),
                location(11),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, location(12))),
                    value: Expression::int(2, location(12), Ownership::ImmutableOwned),
                },
                location(12),
            ),
        ],
        location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    let error = run_borrow_checker(&hir, &host_registry, &string_table)
        .expect_err("unknown return alias should conservatively alias call args");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("may alias"));
}

#[test]
fn host_mutable_parameter_requires_mutable_access() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut host_registry = default_host_registry(&mut string_table);
    register_host_function(
        &mut host_registry,
        "host_mut",
        vec![HostAccessKind::Mutable],
        HostReturnAlias::Fresh,
        HostAbiType::Void,
    );

    let host_fn = symbol("host_mut", &mut string_table);
    let x = symbol("x", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::ImmutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: host_fn,
                    args: vec![reference_expr(x, DataType::Int, location(2))],
                    returns: vec![],
                    location: location(2),
                },
                location(2),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start], entry_path), &mut string_table);
    let error = run_borrow_checker(&hir, &host_registry, &string_table)
        .expect_err("mutable host parameter should enforce mutable access");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("mutably access"));
}

#[test]
fn host_shared_parameter_is_shared_only() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut host_registry = default_host_registry(&mut string_table);
    register_host_function(
        &mut host_registry,
        "host_shared",
        vec![HostAccessKind::Shared],
        HostReturnAlias::Fresh,
        HostAbiType::Void,
    );

    let host_fn = symbol("host_shared", &mut string_table);
    let x = symbol("x", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(var(
                    x.clone(),
                    Expression::int(1, location(1), Ownership::ImmutableOwned),
                )),
                location(1),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: host_fn,
                    args: vec![reference_expr(x, DataType::Int, location(2))],
                    returns: vec![],
                    location: location(2),
                },
                location(2),
            ),
        ],
        location(1),
    );

    let hir = lower_hir(build_ast(vec![start], entry_path), &mut string_table);
    run_borrow_checker(&hir, &host_registry, &string_table)
        .expect("shared host parameter should not force mutable access");
}

#[test]
fn unresolved_or_mismatched_host_signature_errors() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut host_registry = default_host_registry(&mut string_table);
    register_host_function(
        &mut host_registry,
        "one_arg",
        vec![HostAccessKind::Shared],
        HostReturnAlias::Fresh,
        HostAbiType::Void,
    );

    let missing_host = symbol("missing_host", &mut string_table);
    let one_arg = symbol("one_arg", &mut string_table);

    let start_missing = function_node(
        symbol("start_missing", &mut string_table),
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::HostFunctionCall {
                name: missing_host,
                args: vec![],
                returns: vec![],
                location: location(1),
            },
            location(1),
        )],
        location(1),
    );

    let start_mismatch = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![node(
            NodeKind::HostFunctionCall {
                name: one_arg,
                args: vec![],
                returns: vec![],
                location: location(2),
            },
            location(2),
        )],
        location(2),
    );

    let hir = lower_hir(
        build_ast(vec![start_missing, start_mismatch], entry_path),
        &mut string_table,
    );

    let error = run_borrow_checker(&hir, &host_registry, &string_table)
        .expect_err("missing host or mismatched signature should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(
        error.msg.contains("host call target") || error.msg.contains("argument count mismatch")
    );
}
