use crate::backends::wasm::backend::lower_hir_to_wasm_lir;
use crate::backends::wasm::lir::function::WasmLirFunctionOrigin;
use crate::backends::wasm::lir::instructions::{WasmCalleeRef, WasmLirStmt, WasmLirTerminator};
use crate::backends::wasm::lir::linkage::{WasmExportKind, WasmFunctionLinkage};
use crate::backends::wasm::lir::types::{WasmLirFunctionId, WasmLirLocalId};
use crate::backends::wasm::request::{WasmBackendRequest, WasmDebugFlags, WasmExportPolicy};
use crate::backends::wasm::tests::lowering::test_support::{
    bool_expression, borrow_facts_with_drop_site, build_module, build_type_context,
    default_borrow_facts, expression, int_expression, load_local, local, statement,
    string_expression, unit_expression,
};
use crate::compiler_frontend::analysis::borrow_checker::BorrowDropSiteKind;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBinOp, HirBlock, HirExpressionKind, HirFunction, HirFunctionOrigin,
    HirPlace, HirStatementKind, HirTerminator, LocalId, RegionId, StartFragment, ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use rustc_hash::FxHashMap;

#[test]
fn lowers_calls_and_cfg_with_deterministic_block_mapping() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let callee_path = InternedPath::from_single_str("callee", &mut string_table);
    let main_path = InternedPath::from_single_str("main", &mut string_table);

    let callee_block = HirBlock {
        id: BlockId(10),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(100, 7, types.int, RegionId(0))),
    };

    let main_entry = HirBlock {
        id: BlockId(30),
        region: RegionId(0),
        locals: vec![
            local(0, types.boolean, RegionId(0)),
            local(1, types.int, RegionId(0)),
        ],
        statements: vec![
            statement(
                1,
                HirStatementKind::Assign {
                    target: HirPlace::Local(LocalId(0)),
                    value: bool_expression(101, true, types.boolean, RegionId(0)),
                },
                1,
            ),
            statement(
                2,
                HirStatementKind::Call {
                    target: CallTarget::UserFunction(callee_path.clone()),
                    args: vec![],
                    result: Some(LocalId(1)),
                },
                2,
            ),
        ],
        terminator: HirTerminator::If {
            condition: load_local(102, LocalId(0), types.boolean, RegionId(0)),
            then_block: BlockId(40),
            else_block: BlockId(50),
        },
    };

    let then_block = HirBlock {
        id: BlockId(40),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(load_local(103, LocalId(1), types.int, RegionId(0))),
    };

    let else_block = HirBlock {
        id: BlockId(50),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(104, 0, types.int, RegionId(0))),
    };

    let callee = HirFunction {
        id: FunctionId(0),
        entry: BlockId(10),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };
    let main = HirFunction {
        id: FunctionId(1),
        entry: BlockId(30),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        vec![
            (callee, callee_path, HirFunctionOrigin::Normal),
            (main, main_path, HirFunctionOrigin::EntryStart),
        ],
        vec![callee_block, main_entry, then_block, else_block],
        type_context,
        FunctionId(1),
    );

    let result = lower_hir_to_wasm_lir(
        &module,
        &default_borrow_facts(),
        &WasmBackendRequest::default(),
    )
    .expect("Wasm lowering should succeed");

    let main_lir = result
        .lir_module
        .functions
        .iter()
        .find(|function| function.id == WasmLirFunctionId(1))
        .expect("lowered main function should be present");
    assert_eq!(main_lir.blocks.len(), 3);
    assert_eq!(
        main_lir
            .blocks
            .iter()
            .map(|block| block.id.0)
            .collect::<Vec<_>>(),
        vec![0, 1, 2]
    );

    assert!(
        main_lir.blocks[0]
            .statements
            .iter()
            .any(|statement| matches!(
                statement,
                WasmLirStmt::Call {
                    callee: WasmCalleeRef::Function(WasmLirFunctionId(0)),
                    ..
                }
            ))
    );

    assert!(matches!(
        main_lir.blocks[0].terminator,
        WasmLirTerminator::Branch {
            then_block,
            else_block,
            ..
        } if then_block.0 == 1 && else_block.0 == 2
    ));
}

#[test]
fn lowers_runtime_template_with_literal_and_handle_chunks_in_order() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let runtime_path = InternedPath::from_single_str("__bst_frag_0", &mut string_table);

    let concat = expression(
        202,
        HirExpressionKind::BinOp {
            left: Box::new(expression(
                201,
                HirExpressionKind::BinOp {
                    left: Box::new(string_expression(200, "a", types.string, RegionId(0))),
                    op: HirBinOp::Add,
                    right: Box::new(load_local(203, LocalId(0), types.string, RegionId(0))),
                },
                types.string,
                RegionId(0),
                ValueKind::RValue,
            )),
            op: HirBinOp::Add,
            right: Box::new(string_expression(204, "b", types.string, RegionId(0))),
        },
        types.string,
        RegionId(0),
        ValueKind::RValue,
    );

    let runtime_block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.string, RegionId(0))],
        statements: vec![],
        terminator: HirTerminator::Return(concat),
    };

    let runtime_function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![LocalId(0)],
        return_type: types.string,
        return_aliases: vec![],
    };

    let mut module = build_module(
        &mut string_table,
        vec![(
            runtime_function,
            runtime_path,
            HirFunctionOrigin::RuntimeTemplate,
        )],
        vec![runtime_block],
        type_context,
        FunctionId(0),
    );
    module
        .start_fragments
        .push(StartFragment::RuntimeStringFn(FunctionId(0)));

    let result = lower_hir_to_wasm_lir(
        &module,
        &default_borrow_facts(),
        &WasmBackendRequest::default(),
    )
    .expect("Wasm lowering should succeed");

    let runtime_lir = result
        .lir_module
        .functions
        .iter()
        .find(|function| function.id == WasmLirFunctionId(0))
        .expect("lowered runtime function should be present");
    let statements = &runtime_lir.blocks[0].statements;

    assert!(matches!(statements[0], WasmLirStmt::StringNewBuffer { .. }));
    assert!(matches!(
        statements[1],
        WasmLirStmt::StringPushLiteral { .. }
    ));
    assert!(matches!(
        statements[2],
        WasmLirStmt::StringPushHandle { .. }
    ));
    assert!(matches!(
        statements[3],
        WasmLirStmt::StringPushLiteral { .. }
    ));
    assert!(matches!(statements[4], WasmLirStmt::StringFinish { .. }));
}

#[test]
fn deduplicates_static_utf8_segments() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let start_path = InternedPath::from_single_str("main", &mut string_table);

    let start_block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![
            local(0, types.string, RegionId(0)),
            local(1, types.string, RegionId(0)),
        ],
        statements: vec![
            statement(
                1,
                HirStatementKind::Assign {
                    target: HirPlace::Local(LocalId(0)),
                    value: string_expression(300, "same", types.string, RegionId(0)),
                },
                1,
            ),
            statement(
                2,
                HirStatementKind::Assign {
                    target: HirPlace::Local(LocalId(1)),
                    value: string_expression(301, "same", types.string, RegionId(0)),
                },
                2,
            ),
        ],
        terminator: HirTerminator::Return(unit_expression(302, types.unit, RegionId(0))),
    };

    let start_function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        vec![(start_function, start_path, HirFunctionOrigin::EntryStart)],
        vec![start_block],
        type_context,
        FunctionId(0),
    );

    let result = lower_hir_to_wasm_lir(
        &module,
        &default_borrow_facts(),
        &WasmBackendRequest::default(),
    )
    .expect("Wasm lowering should succeed");
    assert_eq!(result.lir_module.static_data.len(), 1);
}

#[test]
fn maps_advisory_drop_sites_to_drop_if_owned_statements() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let start_path = InternedPath::from_single_str("main", &mut string_table);

    let start_block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.string, RegionId(0))],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(401, types.unit, RegionId(0))),
    };
    let start_function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };
    let module = build_module(
        &mut string_table,
        vec![(start_function, start_path, HirFunctionOrigin::EntryStart)],
        vec![start_block],
        type_context,
        FunctionId(0),
    );

    let borrow_facts =
        borrow_facts_with_drop_site(BlockId(0), BorrowDropSiteKind::Return, vec![LocalId(0)]);

    let result = lower_hir_to_wasm_lir(&module, &borrow_facts, &WasmBackendRequest::default())
        .expect("Wasm lowering should succeed");
    let lowered_start = result
        .lir_module
        .functions
        .iter()
        .find(|function| function.id == WasmLirFunctionId(0))
        .expect("lowered start function should be present");
    assert!(lowered_start.blocks[0].statements.iter().any(
        |statement| matches!(statement, WasmLirStmt::DropIfOwned { value } if *value == WasmLirLocalId(0))
    ));
}

#[test]
fn synthesizes_export_wrappers_with_stable_names() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let start_path = InternedPath::from_single_str("main", &mut string_table);

    let start_block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(500, 123, types.int, RegionId(0))),
    };
    let start_function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };
    let module = build_module(
        &mut string_table,
        vec![(start_function, start_path, HirFunctionOrigin::EntryStart)],
        vec![start_block],
        type_context,
        FunctionId(0),
    );

    let mut export_names = FxHashMap::default();
    export_names.insert(FunctionId(0), "main".to_owned());

    let request = WasmBackendRequest {
        export_policy: WasmExportPolicy {
            exported_functions: vec![FunctionId(0)],
            export_names,
            helper_exports: Default::default(),
        },
        target_features: Default::default(),
        emit_options: Default::default(),
        debug_flags: WasmDebugFlags {
            show_wasm_exports: true,
            ..Default::default()
        },
    };

    let result = lower_hir_to_wasm_lir(&module, &default_borrow_facts(), &request)
        .expect("Wasm lowering should succeed");

    assert_eq!(result.lir_module.exports.len(), 1);
    let wrapper_id = match result.lir_module.exports[0].kind {
        WasmExportKind::Function(function_id) => function_id,
        _ => panic!("expected function export"),
    };
    assert_eq!(result.lir_module.exports[0].export_name, "main");

    let wrapper = result
        .lir_module
        .functions
        .iter()
        .find(|function| function.id == wrapper_id)
        .expect("wrapper function should be present");
    assert!(matches!(
        wrapper.origin,
        WasmLirFunctionOrigin::ExportWrapper
    ));
    assert!(matches!(
        wrapper.linkage,
        WasmFunctionLinkage::ExportedWrapper
    ));
    assert!(
        wrapper.blocks[0]
            .statements
            .iter()
            .any(|statement| matches!(
                statement,
                WasmLirStmt::Call {
                    callee: WasmCalleeRef::Function(WasmLirFunctionId(0)),
                    ..
                }
            ))
    );
}

#[test]
fn rejects_invalid_export_request_with_structured_diagnostic() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let start_path = InternedPath::from_single_str("main", &mut string_table);

    let start_block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(600, 1, types.int, RegionId(0))),
    };
    let start_function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };
    let module = build_module(
        &mut string_table,
        vec![(start_function, start_path, HirFunctionOrigin::EntryStart)],
        vec![start_block],
        type_context,
        FunctionId(0),
    );

    let invalid_request = WasmBackendRequest {
        export_policy: WasmExportPolicy {
            exported_functions: vec![FunctionId(0)],
            export_names: FxHashMap::default(),
            helper_exports: Default::default(),
        },
        ..Default::default()
    };

    let error = lower_hir_to_wasm_lir(&module, &default_borrow_facts(), &invalid_request)
        .expect_err("invalid request should produce a lowering diagnostic");
    assert!(
        error.errors[0]
            .msg
            .contains("missing stable export name for FunctionId(0)")
    );
}
