//! Result and error propagation emission tests for JavaScript output.

use super::support::*;

// Result/error emission contract tests beyond helper-level [error] [result]
// ---------------------------------------------------------------------------

/// Verifies that `__bs_error_bubble` is emitted at a call site with synthesized
/// location arguments (file, line, column, functionName). [error]
#[test]
fn error_bubble_emitted_at_call_site_with_location_args() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let bubble_path = InternedPath::from_single_str("__bs_error_bubble", &mut string_table);

    let call_statement = statement(
        1,
        HirStatementKind::Call {
            target: CallTarget::HostFunction(bubble_path),
            args: vec![
                string_expression(1, "error_value", types.string, RegionId(0)),
                string_expression(2, "test.bst", types.string, RegionId(0)),
                int_expression(3, 42, types.int, RegionId(0)),
                int_expression(4, 7, types.int, RegionId(0)),
                string_expression(5, "test_function", types.string, RegionId(0)),
            ],
            result: None,
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![call_statement],
        terminator: HirTerminator::Return(unit_expression(6, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(
        output.source.contains("__bs_error_bubble("),
        "error bubble call site must emit __bs_error_bubble invocation"
    );
    assert!(
        output.source.contains("\"test.bst\"")
            && output.source.contains("42")
            && output.source.contains("7")
            && output.source.contains("\"test_function\""),
        "error bubble call site must include synthesized location arguments"
    );
}

/// Verifies that nested Result-returning functions each get a try/catch wrapper
/// and that result propagation calls are emitted in the function bodies. [result]
#[test]
fn result_propagate_emitted_in_nested_function_calls() {
    let mut string_table = StringTable::new();
    let (mut type_context, types) = build_type_context();
    let region = RegionId(0);

    let result_type = type_context.insert(HirType {
        kind: HirTypeKind::Result {
            ok: types.string,
            err: types.string,
        },
    });

    // Function C (id 0): innermost, returns Result directly
    let c_return = expression(
        1,
        HirExpressionKind::ResultConstruct {
            variant: ResultVariant::Ok,
            value: Box::new(string_expression(2, "inner", types.string, region)),
        },
        result_type,
        region,
        ValueKind::RValue,
    );
    let c_block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(c_return),
    };
    let c_function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: result_type,
        return_aliases: vec![],
    };

    // Function B (id 1): calls C, propagates result
    let b_call = statement(
        3,
        HirStatementKind::Call {
            target: CallTarget::UserFunction(FunctionId(0)),
            args: vec![],
            result: Some(LocalId(0)),
        },
        1,
    );
    let b_return = expression(
        4,
        HirExpressionKind::ResultConstruct {
            variant: ResultVariant::Ok,
            value: Box::new(expression(
                5,
                HirExpressionKind::ResultPropagate {
                    result: Box::new(expression(
                        6,
                        HirExpressionKind::Load(HirPlace::Local(LocalId(0))),
                        result_type,
                        region,
                        ValueKind::RValue,
                    )),
                },
                types.string,
                region,
                ValueKind::RValue,
            )),
        },
        result_type,
        region,
        ValueKind::RValue,
    );
    let b_block = HirBlock {
        id: BlockId(1),
        region,
        locals: vec![local(0, result_type, region)],
        statements: vec![b_call],
        terminator: HirTerminator::Return(b_return),
    };
    let b_function = HirFunction {
        id: FunctionId(1),
        entry: BlockId(1),
        params: vec![],
        return_type: result_type,
        return_aliases: vec![],
    };

    // Function A (id 2): calls B, propagates result
    let a_call = statement(
        7,
        HirStatementKind::Call {
            target: CallTarget::UserFunction(FunctionId(1)),
            args: vec![],
            result: Some(LocalId(0)),
        },
        1,
    );
    let a_return = expression(
        8,
        HirExpressionKind::ResultConstruct {
            variant: ResultVariant::Ok,
            value: Box::new(expression(
                9,
                HirExpressionKind::ResultPropagate {
                    result: Box::new(expression(
                        10,
                        HirExpressionKind::Load(HirPlace::Local(LocalId(0))),
                        result_type,
                        region,
                        ValueKind::RValue,
                    )),
                },
                types.string,
                region,
                ValueKind::RValue,
            )),
        },
        result_type,
        region,
        ValueKind::RValue,
    );
    let a_block = HirBlock {
        id: BlockId(2),
        region,
        locals: vec![local(0, result_type, region)],
        statements: vec![a_call],
        terminator: HirTerminator::Return(a_return),
    };
    let a_function = HirFunction {
        id: FunctionId(2),
        entry: BlockId(2),
        params: vec![],
        return_type: result_type,
        return_aliases: vec![],
    };

    let mut module = HirModule::new();
    module.blocks = vec![c_block, b_block, a_block];
    module.functions = vec![c_function, b_function, a_function];
    module.start_function = FunctionId(2);
    module.type_context = type_context;
    module.regions = vec![HirRegion::lexical(RegionId(0), None)];

    module.side_table.bind_function_name(
        FunctionId(0),
        InternedPath::from_single_str("inner", &mut string_table),
    );
    module.side_table.bind_function_name(
        FunctionId(1),
        InternedPath::from_single_str("middle", &mut string_table),
    );
    module.side_table.bind_function_name(
        FunctionId(2),
        InternedPath::from_single_str("outer", &mut string_table),
    );

    module.function_origins.insert(
        FunctionId(0),
        crate::compiler_frontend::hir::hir_nodes::HirFunctionOrigin::Normal,
    );
    module.function_origins.insert(
        FunctionId(1),
        crate::compiler_frontend::hir::hir_nodes::HirFunctionOrigin::Normal,
    );
    module.function_origins.insert(
        FunctionId(2),
        crate::compiler_frontend::hir::hir_nodes::HirFunctionOrigin::Normal,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    let try_count = output.source.matches("try {").count();
    assert!(
        try_count >= 3,
        "expected at least 3 try/catch wrappers for Result-returning functions, found {try_count}"
    );

    let propagate_return_count = output
        .source
        .matches("return { tag: \"ok\", value: __bs_result_propagate(")
        .count();
    assert!(
        propagate_return_count >= 2,
        "expected at least 2 return statements with result propagation, found {propagate_return_count}"
    );
}

// ---------------------------------------------------------------------------
