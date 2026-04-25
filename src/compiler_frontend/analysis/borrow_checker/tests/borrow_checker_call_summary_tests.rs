//! Borrow-checker call-summary regression tests.
//!
//! WHAT: verifies how call return aliases and host-call access summaries affect borrow facts.
//! WHY: call boundaries are where alias metadata is easiest to get wrong and hardest to debug.

use crate::compiler_frontend::ast::ast_nodes::NodeKind;
use crate::compiler_frontend::ast::expressions::call_argument::{
    CallAccessMode, CallArgument, CallPassingMode,
};
use crate::compiler_frontend::ast::expressions::expression::{Expression, Operator};
use crate::compiler_frontend::ast::statements::functions::{
    FunctionReturn, FunctionSignature, ReturnSlot,
};
use crate::compiler_frontend::compiler_errors::ErrorType;
use crate::compiler_frontend::datatypes::DataType;
use crate::compiler_frontend::external_packages::test_support::{
    TestExternalAbiType as ExternalAbiType, TestExternalAccessKind as ExternalAccessKind,
    TestExternalReturnAlias as ExternalReturnAlias,
};
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::{
    assignment_target, build_ast, default_external_package_registry, entry_and_start,
    fresh_returns, function_node, lower_hir, make_test_variable, node, param, reference_expr,
    register_external_function, run_borrow_checker, symbol, test_location,
};
use crate::compiler_frontend::value_mode::ValueMode;

#[test]
fn user_function_returning_param_aliases_caller_root() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let alias_fn = symbol("alias_fn", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        alias_fn.clone(),
        FunctionSignature {
            parameters: vec![param(p.clone(), DataType::Int, false, test_location(1))],
            returns: vec![ReturnSlot::success(FunctionReturn::AliasCandidates {
                parameter_indices: vec![0],
                data_type: DataType::Int,
            })],
        },
        vec![node(
            NodeKind::Return(vec![reference_expr(p, DataType::Int, test_location(2))]),
            test_location(2),
        )],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    Expression::function_call(
                        alias_fn,
                        vec![reference_expr(x.clone(), DataType::Int, test_location(11))],
                        vec![DataType::Int],
                        test_location(11),
                    ),
                )),
                test_location(11),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, test_location(12))),
                    value: Expression::int(2, test_location(12), ValueMode::ImmutableOwned),
                },
                test_location(12),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("callee return alias should keep caller root aliased");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("may alias"));
}

#[test]
fn fresh_user_return_does_not_alias_caller_roots() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let fresh_fn = symbol("fresh_fn", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        fresh_fn.clone(),
        FunctionSignature {
            parameters: vec![param(p, DataType::Int, false, test_location(1))],
            returns: fresh_returns(vec![DataType::Int]),
        },
        vec![node(
            NodeKind::Return(vec![Expression::int(
                42,
                test_location(2),
                ValueMode::ImmutableOwned,
            )]),
            test_location(2),
        )],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    Expression::function_call(
                        fresh_fn,
                        vec![reference_expr(x.clone(), DataType::Int, test_location(11))],
                        vec![DataType::Int],
                        test_location(11),
                    ),
                )),
                test_location(11),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, test_location(12))),
                    value: Expression::int(2, test_location(12), ValueMode::ImmutableOwned),
                },
                test_location(12),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("fresh callee returns should not alias caller roots");
}

#[test]
fn default_user_returning_param_is_fresh_by_default() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let unknown_fn = symbol("unknown_fn", &mut string_table);
    let p = symbol("p", &mut string_table);
    let q = symbol("q", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        unknown_fn.clone(),
        FunctionSignature {
            parameters: vec![param(p.clone(), DataType::Int, false, test_location(1))],
            returns: fresh_returns(vec![DataType::Int]),
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    q.clone(),
                    reference_expr(p, DataType::Int, test_location(2)),
                )),
                test_location(2),
            ),
            node(
                NodeKind::Return(vec![reference_expr(q, DataType::Int, test_location(3))]),
                test_location(3),
            ),
        ],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    Expression::function_call(
                        unknown_fn,
                        vec![reference_expr(x.clone(), DataType::Int, test_location(11))],
                        vec![DataType::Int],
                        test_location(11),
                    ),
                )),
                test_location(11),
            ),
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(x, DataType::Int, test_location(12))),
                    value: Expression::int(2, test_location(12), ValueMode::ImmutableOwned),
                },
                test_location(12),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("default user returns should be fresh unless explicitly declared as aliasing");
}

#[test]
fn mutable_user_argument_is_accepted_without_false_shared_conflict() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let mut_sink = symbol("mut_sink", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        mut_sink.clone(),
        FunctionSignature {
            parameters: vec![param(p, DataType::Int, true, test_location(1))],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mut_sink,
                    args: vec![CallArgument::positional(
                        reference_expr(x, DataType::Int, test_location(11)),
                        CallAccessMode::Shared,
                        test_location(11),
                    )],
                    result_types: vec![],
                    location: test_location(11),
                },
                test_location(11),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("single mutable argument call should be accepted");
}

#[test]
fn mutable_user_call_with_fresh_mutable_arg_does_not_alias_existing_place_argument() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let mut2 = symbol("mut2", &mut string_table);
    let a = symbol("a", &mut string_table);
    let b = symbol("b", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        mut2.clone(),
        FunctionSignature {
            parameters: vec![
                param(a, DataType::Int, true, test_location(1)),
                param(b, DataType::Int, true, test_location(1)),
            ],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mut2,
                    args: vec![
                        CallArgument::positional(
                            reference_expr(x, DataType::Int, test_location(11)),
                            CallAccessMode::Shared,
                            test_location(11),
                        ),
                        CallArgument::positional(
                            Expression::int(2, test_location(11), ValueMode::ImmutableOwned),
                            CallAccessMode::Shared,
                            test_location(11),
                        )
                        .with_passing_mode(CallPassingMode::FreshMutableValue),
                    ],
                    result_types: vec![],
                    location: test_location(11),
                },
                test_location(11),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &external_package_registry, &string_table).expect(
        "fresh mutable call args should be treated as independent locals, not aliases of other args",
    );
}

#[test]
fn host_mutable_parameter_requires_mutable_access() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut external_package_registry = default_external_package_registry(&mut string_table);
    register_external_function(
        &mut external_package_registry,
        "host_mut",
        vec![ExternalAccessKind::Mutable],
        ExternalReturnAlias::Fresh,
        ExternalAbiType::Void,
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
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(1), ValueMode::ImmutableOwned),
                )),
                test_location(1),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: host_fn,
                    args: vec![CallArgument::positional(
                        reference_expr(x, DataType::Int, test_location(2)),
                        CallAccessMode::Shared,
                        test_location(2),
                    )],
                    result_types: vec![],
                    location: test_location(2),
                },
                test_location(2),
            ),
        ],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start], entry_path), &mut string_table);
    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("mutable host parameter should enforce mutable access");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("mutably access"));
}

#[test]
fn host_mutable_parameter_accepts_mutable_local_argument() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut external_package_registry = default_external_package_registry(&mut string_table);
    register_external_function(
        &mut external_package_registry,
        "host_mut_ok",
        vec![ExternalAccessKind::Mutable],
        ExternalReturnAlias::Fresh,
        ExternalAbiType::Void,
    );

    let host_fn = symbol("host_mut_ok", &mut string_table);
    let x = symbol("x", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(1), ValueMode::MutableOwned),
                )),
                test_location(1),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: host_fn,
                    args: vec![CallArgument::positional(
                        reference_expr(x, DataType::Int, test_location(2)),
                        CallAccessMode::Shared,
                        test_location(2),
                    )],
                    result_types: vec![],
                    location: test_location(2),
                },
                test_location(2),
            ),
        ],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start], entry_path), &mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("mutable host parameter should accept mutable local argument");
}

#[test]
fn host_shared_parameter_is_shared_only() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut external_package_registry = default_external_package_registry(&mut string_table);
    register_external_function(
        &mut external_package_registry,
        "host_shared",
        vec![ExternalAccessKind::Shared],
        ExternalReturnAlias::Fresh,
        ExternalAbiType::Void,
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
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(1), ValueMode::ImmutableOwned),
                )),
                test_location(1),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: host_fn,
                    args: vec![CallArgument::positional(
                        reference_expr(x, DataType::Int, test_location(2)),
                        CallAccessMode::Shared,
                        test_location(2),
                    )],
                    result_types: vec![],
                    location: test_location(2),
                },
                test_location(2),
            ),
        ],
        test_location(1),
    );

    let hir = lower_hir(build_ast(vec![start], entry_path), &mut string_table);
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("shared host parameter should not force mutable access");
}

#[test]
fn two_mutable_args_to_same_root_are_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let mut2 = symbol("mut2", &mut string_table);
    let a = symbol("a", &mut string_table);
    let b = symbol("b", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        mut2.clone(),
        FunctionSignature {
            parameters: vec![
                param(a, DataType::Int, true, test_location(1)),
                param(b, DataType::Int, true, test_location(1)),
            ],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mut2,
                    args: vec![
                        CallArgument::positional(
                            reference_expr(x.clone(), DataType::Int, test_location(11)),
                            CallAccessMode::Shared,
                            test_location(11),
                        ),
                        CallArgument::positional(
                            reference_expr(x, DataType::Int, test_location(11)),
                            CallAccessMode::Shared,
                            test_location(11),
                        ),
                    ],
                    result_types: vec![],
                    location: test_location(11),
                },
                test_location(11),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("two mutable args to the same root should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
}

#[test]
fn shared_then_mutable_args_to_same_root_are_rejected() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let read_then_mut = symbol("read_then_mut", &mut string_table);
    let read = symbol("read", &mut string_table);
    let mutate = symbol("mutate", &mut string_table);
    let x = symbol("x", &mut string_table);

    let callee = function_node(
        read_then_mut.clone(),
        FunctionSignature {
            parameters: vec![
                param(read, DataType::Int, false, test_location(1)),
                param(mutate, DataType::Int, true, test_location(1)),
            ],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::FunctionCall {
                    name: read_then_mut,
                    args: vec![
                        CallArgument::positional(
                            reference_expr(x.clone(), DataType::Int, test_location(11)),
                            CallAccessMode::Shared,
                            test_location(11),
                        ),
                        CallArgument::positional(
                            reference_expr(x, DataType::Int, test_location(11)),
                            CallAccessMode::Shared,
                            test_location(11),
                        ),
                    ],
                    result_types: vec![],
                    location: test_location(11),
                },
                test_location(11),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("shared then mutable access to same root in a call must fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("overlapping"));
}

#[test]
fn unresolved_or_mismatched_host_signature_errors() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut external_package_registry = default_external_package_registry(&mut string_table);
    register_external_function(
        &mut external_package_registry,
        "one_arg",
        vec![ExternalAccessKind::Shared],
        ExternalReturnAlias::Fresh,
        ExternalAbiType::Void,
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
                result_types: vec![],
                location: test_location(1),
            },
            test_location(1),
        )],
        test_location(1),
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
                result_types: vec![],
                location: test_location(2),
            },
            test_location(2),
        )],
        test_location(2),
    );

    let hir = lower_hir(
        build_ast(vec![start_missing, start_mismatch], entry_path),
        &mut string_table,
    );

    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("missing host or mismatched signature should fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(
        error.msg.contains("host call target") || error.msg.contains("argument count mismatch")
    );
}

#[test]
fn mutable_user_parameter_rejects_immutable_argument_reused_after_call() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let mut_user = symbol("mut_user", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        mut_user.clone(),
        FunctionSignature {
            parameters: vec![param(p, DataType::Int, true, test_location(1))],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::ImmutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mut_user,
                    args: vec![CallArgument::positional(
                        reference_expr(x.clone(), DataType::Int, test_location(11)),
                        CallAccessMode::Shared,
                        test_location(11),
                    )],
                    result_types: vec![],
                    location: test_location(11),
                },
                test_location(11),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    reference_expr(x, DataType::Int, test_location(12)),
                )),
                test_location(12),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("immutable argument passed to mutable user param and reused must fail");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("immutable local"));
}

#[test]
fn out_of_range_return_alias_metadata_is_reported_at_call_site() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let mut external_package_registry = default_external_package_registry(&mut string_table);
    register_external_function(
        &mut external_package_registry,
        "bad_alias_host",
        vec![ExternalAccessKind::Shared],
        ExternalReturnAlias::AliasArgs(vec![1]),
        ExternalAbiType::I32,
    );

    let bad_alias_host = symbol("bad_alias_host", &mut string_table);
    let x = symbol("x", &mut string_table);

    let start = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::HostFunctionCall {
                    name: bad_alias_host,
                    args: vec![CallArgument::positional(
                        reference_expr(x, DataType::Int, test_location(11)),
                        CallAccessMode::Shared,
                        test_location(11),
                    )],
                    result_types: vec![DataType::Int],
                    location: test_location(11),
                },
                test_location(11),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(build_ast(vec![start], entry_path), &mut string_table);

    let error = run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect_err("out-of-range return alias metadata should fail at call site");
    assert_eq!(error.error_type, ErrorType::BorrowChecker);
    assert!(error.msg.contains("out-of-range return-alias index"));
}

#[test]
fn same_line_mutable_call_then_reuse_uses_order_keys() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let mut_user = symbol("mut_user", &mut string_table);
    let p = symbol("p", &mut string_table);
    let x = symbol("x", &mut string_table);
    let y = symbol("y", &mut string_table);

    let callee = function_node(
        mut_user.clone(),
        FunctionSignature {
            parameters: vec![param(p, DataType::Int, true, test_location(1))],
            returns: vec![],
        },
        vec![],
        test_location(1),
    );

    // WHAT: both statements intentionally share one source line.
    // WHY: validates that borrow/move classification uses statement order keys, not line numbers.
    let same_line = test_location(20);
    let caller = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    x.clone(),
                    Expression::int(1, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::FunctionCall {
                    name: mut_user,
                    args: vec![CallArgument::positional(
                        reference_expr(x.clone(), DataType::Int, same_line.clone()),
                        CallAccessMode::Shared,
                        same_line.clone(),
                    )],
                    result_types: vec![],
                    location: same_line.clone(),
                },
                same_line.clone(),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    y,
                    reference_expr(x, DataType::Int, same_line.clone()),
                )),
                same_line,
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![callee, caller], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &external_package_registry, &string_table).expect(
        "same-line mutable call + later reuse should borrow (not move) when ordered by statement",
    );
}

#[test]
fn short_circuit_rhs_mutable_call_with_later_merge_use_borrows_instead_of_moving() {
    let mut string_table = StringTable::new();
    let (entry_path, start_name) = entry_and_start(&mut string_table);
    let external_package_registry = default_external_package_registry(&mut string_table);

    let rhs_name = symbol("rhs_short", &mut string_table);
    let calls = symbol("calls", &mut string_table);
    let lhs = symbol("lhs", &mut string_table);
    let value = symbol("value", &mut string_table);
    let sink = symbol("sink", &mut string_table);
    let param_calls = symbol("param_calls", &mut string_table);

    let rhs_function = function_node(
        rhs_name.clone(),
        FunctionSignature {
            parameters: vec![param(
                param_calls.clone(),
                DataType::Int,
                true,
                test_location(1),
            )],
            returns: fresh_returns(vec![DataType::Bool]),
        },
        vec![
            node(
                NodeKind::Assignment {
                    target: Box::new(assignment_target(
                        param_calls.clone(),
                        DataType::Int,
                        test_location(2),
                    )),
                    value: Expression::runtime(
                        vec![
                            node(
                                NodeKind::Rvalue(Expression::reference(
                                    param_calls,
                                    DataType::Int,
                                    test_location(2),
                                    ValueMode::MutableOwned,
                                )),
                                test_location(2),
                            ),
                            node(
                                NodeKind::Rvalue(Expression::int(
                                    1,
                                    test_location(2),
                                    ValueMode::ImmutableOwned,
                                )),
                                test_location(2),
                            ),
                            node(NodeKind::Operator(Operator::Add), test_location(2)),
                        ],
                        DataType::Int,
                        test_location(2),
                        ValueMode::MutableOwned,
                    ),
                },
                test_location(2),
            ),
            node(
                NodeKind::Return(vec![Expression::bool(
                    true,
                    test_location(3),
                    ValueMode::ImmutableOwned,
                )]),
                test_location(3),
            ),
        ],
        test_location(1),
    );

    let short_circuit_value = Expression::runtime(
        vec![
            node(
                NodeKind::Rvalue(Expression::reference(
                    lhs.clone(),
                    DataType::Bool,
                    test_location(11),
                    ValueMode::ImmutableOwned,
                )),
                test_location(11),
            ),
            node(
                NodeKind::Rvalue(Expression::function_call(
                    rhs_name,
                    vec![Expression::reference(
                        calls.clone(),
                        DataType::Int,
                        test_location(11),
                        ValueMode::MutableOwned,
                    )],
                    vec![DataType::Bool],
                    test_location(11),
                )),
                test_location(11),
            ),
            node(NodeKind::Operator(Operator::And), test_location(11)),
        ],
        DataType::Bool,
        test_location(11),
        ValueMode::ImmutableOwned,
    );

    let start_function = function_node(
        start_name,
        FunctionSignature {
            parameters: vec![],
            returns: vec![],
        },
        vec![
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    lhs,
                    Expression::bool(false, test_location(10), ValueMode::ImmutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    calls.clone(),
                    Expression::int(0, test_location(10), ValueMode::MutableOwned),
                )),
                test_location(10),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(value, short_circuit_value)),
                test_location(11),
            ),
            node(
                NodeKind::VariableDeclaration(make_test_variable(
                    sink,
                    Expression::reference(
                        calls,
                        DataType::Int,
                        test_location(12),
                        ValueMode::ImmutableReference,
                    ),
                )),
                test_location(12),
            ),
        ],
        test_location(10),
    );

    let hir = lower_hir(
        build_ast(vec![rhs_function, start_function], entry_path),
        &mut string_table,
    );
    run_borrow_checker(&hir, &external_package_registry, &string_table)
        .expect("rhs-only mutable short-circuit call with later merge use should stay borrowed");
}
