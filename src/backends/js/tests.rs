use crate::backends::function_registry::CallTarget;
use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction, HirLocal,
    HirModule, HirRegion, HirStatement, HirStatementKind, HirTerminator, LocalId, OptionVariant,
    RegionId, ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::TextLocation;

#[derive(Clone, Copy)]
struct TypeIds {
    unit: TypeId,
    int: TypeId,
    boolean: TypeId,
    string: TypeId,
    option_int: TypeId,
}

fn loc(line: i32) -> TextLocation {
    TextLocation::new_just_line(line)
}

fn build_type_context() -> (TypeContext, TypeIds) {
    let mut type_context = TypeContext::default();

    let unit = type_context.insert(HirType {
        kind: HirTypeKind::Unit,
    });
    let int = type_context.insert(HirType {
        kind: HirTypeKind::Int,
    });
    let boolean = type_context.insert(HirType {
        kind: HirTypeKind::Bool,
    });
    let string = type_context.insert(HirType {
        kind: HirTypeKind::String,
    });
    let option_int = type_context.insert(HirType {
        kind: HirTypeKind::Option { inner: int },
    });

    (
        type_context,
        TypeIds {
            unit,
            int,
            boolean,
            string,
            option_int,
        },
    )
}

fn expression(
    id: u32,
    kind: HirExpressionKind,
    ty: TypeId,
    region: RegionId,
    value_kind: ValueKind,
) -> HirExpression {
    HirExpression {
        id: crate::compiler_frontend::hir::hir_nodes::HirValueId(id),
        kind,
        ty,
        value_kind,
        region,
    }
}

fn unit_expression(id: u32, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::TupleConstruct { elements: vec![] },
        ty,
        region,
        ValueKind::Const,
    )
}

fn int_expression(id: u32, value: i64, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Int(value),
        ty,
        region,
        ValueKind::Const,
    )
}

fn bool_expression(id: u32, value: bool, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Bool(value),
        ty,
        region,
        ValueKind::Const,
    )
}

fn string_expression(id: u32, value: &str, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::StringLiteral(value.to_owned()),
        ty,
        region,
        ValueKind::Const,
    )
}

fn statement(id: u32, kind: HirStatementKind, line: i32) -> HirStatement {
    HirStatement {
        id: crate::compiler_frontend::hir::hir_nodes::HirNodeId(id),
        kind,
        location: loc(line),
    }
}

fn local(local_id: u32, ty: TypeId, region: RegionId) -> HirLocal {
    HirLocal {
        id: LocalId(local_id),
        ty,
        mutable: true,
        region,
        source_info: Some(loc(1)),
    }
}

fn build_module(
    string_table: &mut StringTable,
    function_name: &str,
    blocks: Vec<HirBlock>,
    function: HirFunction,
    local_names: &[(LocalId, &str)],
    type_context: TypeContext,
) -> HirModule {
    let mut module = HirModule::new();
    let function_id = function.id;
    module.blocks = blocks;
    module.start_function = function_id;
    module.functions = vec![function];
    module.type_context = type_context;
    module.regions = vec![HirRegion::lexical(RegionId(0), None)];

    let function_path = InternedPath::from_single_str(function_name, string_table);
    module
        .side_table
        .bind_function_name(function_id, function_path.clone());

    for (local_id, local_name) in local_names {
        let local_path = InternedPath::from_single_str(local_name, string_table);
        module.side_table.bind_local_name(*local_id, local_path);
    }

    module
}

#[test]
fn lower_hir_smoke_test() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(0, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
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
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("function main()"));
    assert!(output.source.contains("return;"));
    assert_eq!(
        output
            .function_name_by_id
            .get(&FunctionId(0))
            .map(String::as_str),
        Some("main")
    );
}

#[test]
fn emits_structured_if_without_dispatcher() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let assign_then = statement(
        1,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(0)),
            value: int_expression(1, 2, types.int, RegionId(0)),
        },
        2,
    );

    let assign_else = statement(
        2,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(0)),
            value: int_expression(2, 3, types.int, RegionId(0)),
        },
        3,
    );

    let blocks = vec![
        HirBlock {
            id: BlockId(0),
            region: RegionId(0),
            locals: vec![local(0, types.int, RegionId(0))],
            statements: vec![],
            terminator: HirTerminator::If {
                condition: bool_expression(3, true, types.boolean, RegionId(0)),
                then_block: BlockId(1),
                else_block: BlockId(2),
            },
        },
        HirBlock {
            id: BlockId(1),
            region: RegionId(0),
            locals: vec![],
            statements: vec![assign_then],
            terminator: HirTerminator::Jump {
                target: BlockId(3),
                args: vec![],
            },
        },
        HirBlock {
            id: BlockId(2),
            region: RegionId(0),
            locals: vec![],
            statements: vec![assign_else],
            terminator: HirTerminator::Jump {
                target: BlockId(3),
                args: vec![],
            },
        },
        HirBlock {
            id: BlockId(3),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Return(unit_expression(4, types.unit, RegionId(0))),
        },
    ];

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
    };

    let module = build_module(
        &mut string_table,
        "main",
        blocks,
        function,
        &[(LocalId(0), "x")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("if (true)"));
    assert!(!output.source.contains("switch (__bb"));
}

#[test]
fn falls_back_to_dispatcher_for_cfg_cycle() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let loop_assign = statement(
        1,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(0)),
            value: int_expression(1, 1, types.int, RegionId(0)),
        },
        2,
    );

    let blocks = vec![
        HirBlock {
            id: BlockId(0),
            region: RegionId(0),
            locals: vec![local(0, types.int, RegionId(0))],
            statements: vec![],
            terminator: HirTerminator::Jump {
                target: BlockId(1),
                args: vec![],
            },
        },
        HirBlock {
            id: BlockId(1),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::If {
                condition: bool_expression(2, true, types.boolean, RegionId(0)),
                then_block: BlockId(2),
                else_block: BlockId(3),
            },
        },
        HirBlock {
            id: BlockId(2),
            region: RegionId(0),
            locals: vec![],
            statements: vec![loop_assign],
            terminator: HirTerminator::Jump {
                target: BlockId(1),
                args: vec![],
            },
        },
        HirBlock {
            id: BlockId(3),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
        },
    ];

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
    };

    let module = build_module(
        &mut string_table,
        "main",
        blocks,
        function,
        &[(LocalId(0), "counter")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("switch (__bb"));
}

#[test]
fn lowers_break_and_continue_terminators_with_dispatcher() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let blocks = vec![
        HirBlock {
            id: BlockId(0),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Jump {
                target: BlockId(1),
                args: vec![],
            },
        },
        HirBlock {
            id: BlockId(1),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::If {
                condition: bool_expression(1, true, types.boolean, RegionId(0)),
                then_block: BlockId(2),
                else_block: BlockId(4),
            },
        },
        HirBlock {
            id: BlockId(2),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Continue { target: BlockId(3) },
        },
        HirBlock {
            id: BlockId(3),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Break { target: BlockId(4) },
        },
        HirBlock {
            id: BlockId(4),
            region: RegionId(0),
            locals: vec![],
            statements: vec![],
            terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
        },
    ];

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
    };

    let module = build_module(
        &mut string_table,
        "main",
        blocks,
        function,
        &[],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("switch (__bb"));
    assert!(output.source.contains("= 3;"));
    assert!(output.source.contains("= 4;"));
}

#[test]
fn lowers_host_io_call_to_console_log() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let io_path = InternedPath::from_single_str("io", &mut string_table);

    let call_statement = statement(
        1,
        HirStatementKind::Call {
            target: CallTarget::HostFunction(io_path),
            args: vec![string_expression(1, "hello", types.string, RegionId(0))],
            result: None,
        },
        2,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![call_statement],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
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
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("console.log"));
}

#[test]
fn auto_invokes_start_function_when_enabled() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(0, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
    };

    let module = build_module(
        &mut string_table,
        "start_main",
        vec![block],
        function,
        &[],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: true,
        },
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("start_main();"));
}

#[test]
fn exposes_function_name_map_for_runtime_fragments() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let block0 = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(0, types.unit, RegionId(0))),
    };

    let block1 = HirBlock {
        id: BlockId(1),
        region: RegionId(0),
        locals: vec![],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(1, types.unit, RegionId(0))),
    };

    let mut module = HirModule::new();
    module.blocks = vec![block0, block1];
    module.start_function = FunctionId(0);
    module.functions = vec![
        HirFunction {
            id: FunctionId(0),
            entry: BlockId(0),
            params: vec![],
            return_type: types.unit,
        },
        HirFunction {
            id: FunctionId(1),
            entry: BlockId(1),
            params: vec![],
            return_type: types.unit,
        },
    ];
    module.type_context = type_context;
    module.regions = vec![HirRegion::lexical(RegionId(0), None)];
    module.side_table.bind_function_name(
        FunctionId(0),
        InternedPath::from_single_str("start", &mut string_table),
    );
    module.side_table.bind_function_name(
        FunctionId(1),
        InternedPath::from_single_str("__bst_frag_0", &mut string_table),
    );

    let output = lower_hir_to_js(
        &module,
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed");

    assert_eq!(
        output
            .function_name_by_id
            .get(&FunctionId(0))
            .map(String::as_str),
        Some("start")
    );
    assert_eq!(
        output
            .function_name_by_id
            .get(&FunctionId(1))
            .map(String::as_str),
        Some("__bst_frag_0")
    );
}

#[test]
fn returns_error_for_unsupported_option_construct() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let option_value = expression(
        1,
        HirExpressionKind::OptionConstruct {
            variant: OptionVariant::Some,
            value: Some(Box::new(int_expression(2, 10, types.int, RegionId(0)))),
        },
        types.option_int,
        RegionId(0),
        ValueKind::RValue,
    );

    let option_statement = statement(1, HirStatementKind::Expr(option_value), 1);

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![],
        statements: vec![option_statement],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
    };

    let module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[],
        type_context,
    );

    let error = lower_hir_to_js(
        &module,
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect_err("OptionConstruct should not be supported yet");

    assert!(error.msg.contains("OptionConstruct"));
}
