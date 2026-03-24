// JS backend semantic correctness matrix.
//
// This module pins the observable contract between Beanstalk HIR semantics and emitted JS output.
// Every test maps to one or more checklist items below. Tests should be kept at the level of
// emitted JS structure — they do not execute JS, only inspect emitted text.
//
// Checklist:
//
//   [binding]        local slot bindings read via __bs_read / written via __bs_assign_value
//   [binding]        alias bindings resolved through __bs_resolve before read/write
//   [binding]        __bs_param_binding normalises plain JS values, slot bindings, and alias refs
//   [alias]          __bs_assign_borrow transitions binding to alias mode; write-through if alias
//   [alias]          __bs_assign_value collapses alias and writes to slot
//   [computed]       field read/write routed through __bs_field computed-place record
//   [computed]       index read/write routed through __bs_index computed-place record
//   [computed]       computed-place refs compose with __bs_read/__bs_write via __bs_get/__bs_set
//   [clone]          explicit copy of arrays uses __bs_clone_value (recursive per-element)
//   [clone]          explicit copy of objects uses __bs_clone_value (recursive per-key)
//   [clone]          Copy expression in HIR emits __bs_clone_value(__bs_read(...))
//   [prelude-order]  binding helpers precede alias helpers in emitted output
//   [prelude-order]  alias helpers precede computed-place helpers in emitted output
//   [prelude-order]  computed-place helpers precede clone helper in emitted output
//   [cfg]            acyclic if-then-else lowers to structured `if` without dispatcher
//   [cfg]            cycles and back-edges fall back to switch-based block dispatcher
//   [cfg]            break/continue terminators emit correct block-number assignments
//   [host]           host io(...) calls read the binding value with __bs_read before logging
//   [start]          auto_invoke_start emits the start function name followed by ()
//   [names]          function_name_by_id exposes stable JS names for runtime-fragment lookup
//   [names]          reserved JS identifiers are prefixed with _ to avoid collisions
//   [names]          identifier characters invalid for JS are replaced with _
//   [error]          unsupported HIR constructs (OptionConstruct) return a compiler error

use crate::backends::js::{JsLoweringConfig, lower_hir_to_js};
use crate::compiler_frontend::analysis::borrow_checker::{
    BorrowCheckReport, BorrowStateSnapshot, LocalBorrowSnapshot, LocalMode,
};
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FieldId, FunctionId, HirBlock, HirExpression, HirExpressionKind, HirField,
    HirFunction, HirLocal, HirModule, HirNodeId, HirPlace, HirRegion, HirStatement,
    HirStatementKind, HirStruct, HirTerminator, LocalId, OptionVariant, RegionId, StructId,
    ValueKind,
};
use crate::compiler_frontend::host_functions::CallTarget;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::compiler_frontend::tokenizer::tokens::{CharPosition, TextLocation};

// ---------------------------------------------------------------------------
// Shared test helpers
// ---------------------------------------------------------------------------

#[derive(Clone, Copy)]
struct TypeIds {
    unit: TypeId,
    int: TypeId,
    boolean: TypeId,
    string: TypeId,
    option_int: TypeId,
}

fn loc(start: i32) -> TextLocation {
    TextLocation {
        scope: InternedPath::new(),
        start_pos: CharPosition {
            line_number: start,
            char_column: 0,
        },
        end_pos: CharPosition {
            line_number: start,
            char_column: 120, // Arbitrary number
        },
    }
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

/// Builds and lowers a minimal single-function module with an empty body.
///
/// WHY: most prelude and identifier tests only need a module to exist so the prelude is emitted;
/// they do not care about the function body.
fn lower_minimal_module(function_name: &str) -> String {
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
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        function_name,
        vec![block],
        function,
        &[],
        type_context,
    );

    lower_hir_to_js(
        &module,
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: false,
        },
    )
    .expect("JS lowering should succeed")
    .source
}

fn default_config() -> JsLoweringConfig {
    JsLoweringConfig {
        pretty: true,
        emit_locations: false,
        auto_invoke_start: false,
    }
}

use super::test_symbol_helpers::{
    expected_dev_field_name, expected_dev_function_name, expected_dev_local_name,
};

// ---------------------------------------------------------------------------
// Prelude helper presence tests [binding] [alias] [computed] [clone]
// ---------------------------------------------------------------------------

/// Verifies that all six binding helpers are present in the emitted prelude. [binding]
#[test]
fn runtime_prelude_contains_all_binding_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_is_ref("),
        "prelude must contain __bs_is_ref"
    );
    assert!(
        source.contains("function __bs_binding("),
        "prelude must contain __bs_binding"
    );
    assert!(
        source.contains("function __bs_param_binding("),
        "prelude must contain __bs_param_binding"
    );
    assert!(
        source.contains("function __bs_resolve("),
        "prelude must contain __bs_resolve"
    );
    assert!(
        source.contains("function __bs_read("),
        "prelude must contain __bs_read"
    );
    assert!(
        source.contains("function __bs_write("),
        "prelude must contain __bs_write"
    );
}

/// Verifies that both alias helpers are present in the emitted prelude. [alias]
#[test]
fn runtime_prelude_contains_alias_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_assign_borrow("),
        "prelude must contain __bs_assign_borrow"
    );
    assert!(
        source.contains("function __bs_assign_value("),
        "prelude must contain __bs_assign_value"
    );
}

/// Verifies that both computed-place helpers are present in the emitted prelude. [computed]
#[test]
fn runtime_prelude_contains_computed_place_helpers() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_field("),
        "prelude must contain __bs_field"
    );
    assert!(
        source.contains("function __bs_index("),
        "prelude must contain __bs_index"
    );
}

/// Verifies that the deep-copy helper is present in the emitted prelude. [clone]
#[test]
fn runtime_prelude_contains_clone_helper() {
    let source = lower_minimal_module("main");

    assert!(
        source.contains("function __bs_clone_value("),
        "prelude must contain __bs_clone_value"
    );
}

// ---------------------------------------------------------------------------
// Prelude ordering tests [prelude-order]
// ---------------------------------------------------------------------------

/// Verifies that binding helpers precede alias helpers in emitted output. [prelude-order]
#[test]
fn binding_helpers_appear_before_alias_helpers() {
    let source = lower_minimal_module("main");

    let binding_pos = source
        .find("function __bs_binding(")
        .expect("__bs_binding must be present");
    let alias_pos = source
        .find("function __bs_assign_borrow(")
        .expect("__bs_assign_borrow must be present");

    assert!(
        binding_pos < alias_pos,
        "binding helpers must appear before alias helpers in emitted JS"
    );
}

/// Verifies that alias helpers precede computed-place helpers in emitted output. [prelude-order]
#[test]
fn alias_helpers_appear_before_computed_place_helpers() {
    let source = lower_minimal_module("main");

    let alias_pos = source
        .find("function __bs_assign_value(")
        .expect("__bs_assign_value must be present");
    let computed_pos = source
        .find("function __bs_field(")
        .expect("__bs_field must be present");

    assert!(
        alias_pos < computed_pos,
        "alias helpers must appear before computed-place helpers in emitted JS"
    );
}

/// Verifies that computed-place helpers precede the clone helper in emitted output. [prelude-order]
#[test]
fn computed_place_helpers_appear_before_clone_helper() {
    let source = lower_minimal_module("main");

    let computed_pos = source
        .find("function __bs_index(")
        .expect("__bs_index must be present");
    let clone_pos = source
        .find("function __bs_clone_value(")
        .expect("__bs_clone_value must be present");

    assert!(
        computed_pos < clone_pos,
        "computed-place helpers must appear before the clone helper in emitted JS"
    );
}

// ---------------------------------------------------------------------------
// Identifier sanitisation tests [names]
// ---------------------------------------------------------------------------

/// Verifies that a function whose name is a JS reserved word gets an underscore prefix. [names]
#[test]
fn reserved_word_function_name_gets_underscore_prefix() {
    // `for` is a reserved JS identifier; the emitter must prefix it to avoid a syntax error.
    let source = lower_minimal_module("for");
    let expected_name = expected_dev_function_name("for", 0);

    assert!(
        source.contains(&format!("function {}(", expected_name)),
        "reserved identifiers should still lower to deterministic id-based names"
    );
}

/// Verifies that invalid JS identifier characters are replaced with underscores. [names]
#[test]
fn invalid_identifier_chars_are_replaced_with_underscore() {
    // Hyphens are not valid in JS identifiers; they must be replaced.
    let source = lower_minimal_module("foo-bar");
    let expected_name = expected_dev_function_name("foo-bar", 0);

    assert!(
        source.contains(&format!("function {}(", expected_name)),
        "hyphens in identifiers must be replaced with underscores"
    );
}

// ---------------------------------------------------------------------------
// Local binding and assignment tests [binding] [alias]
// ---------------------------------------------------------------------------

/// Verifies that assigning an integer value to a local emits __bs_assign_value. [binding]
#[test]
fn local_slot_assignment_emits_assign_value() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let assign = statement(
        1,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(0)),
            value: int_expression(1, 42, types.int, RegionId(0)),
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![assign],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
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
        &[(LocalId(0), "count")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let count_name = expected_dev_local_name("count", 0);

    assert!(
        output
            .source
            .contains(&format!("__bs_assign_value({}, 42);", count_name)),
        "assigning an integer to a local must emit __bs_assign_value"
    );
}

// ---------------------------------------------------------------------------
// Clone / explicit copy tests [clone]
// ---------------------------------------------------------------------------

/// Verifies that a HIR Copy expression emits __bs_clone_value(__bs_read(...)). [clone]
#[test]
fn explicit_copy_emits_clone_value_wrapped_read() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    // Assign a source local, then assign a copy of it to a target local.
    let assign_source = statement(
        1,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(0)),
            value: string_expression(1, "hello", types.string, RegionId(0)),
        },
        1,
    );

    let copy_expr = expression(
        2,
        HirExpressionKind::Copy(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
            LocalId(0),
        )),
        types.string,
        RegionId(0),
        ValueKind::RValue,
    );

    let assign_copy = statement(
        2,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(1)),
            value: copy_expr,
        },
        2,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![
            local(0, types.string, RegionId(0)),
            local(1, types.string, RegionId(0)),
        ],
        statements: vec![assign_source, assign_copy],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
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
        &[(LocalId(0), "src"), (LocalId(1), "dst")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let src_name = expected_dev_local_name("src", 0);

    assert!(
        output
            .source
            .contains(&format!("__bs_clone_value(__bs_read({}))", src_name)),
        "Copy expression must emit __bs_clone_value(__bs_read(src))"
    );
}

// ---------------------------------------------------------------------------
// CFG lowering tests [cfg]
// ---------------------------------------------------------------------------

/// Verifies that a simple acyclic if-then-else lowers to structured JS without a dispatcher. [cfg]
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
        return_aliases: vec![],
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
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("if (true)"));
    assert!(!output.source.contains("switch (__bb"));
}

/// Verifies that a CFG cycle falls back to a switch-based block dispatcher. [cfg]
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
        return_aliases: vec![],
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
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("switch (__bb"));
}

/// Verifies that break and continue terminators emit the expected block-number assignments. [cfg]
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
        return_aliases: vec![],
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
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");

    assert!(output.source.contains("switch (__bb"));
    assert!(output.source.contains("= 3;"));
    assert!(output.source.contains("= 4;"));
}

// ---------------------------------------------------------------------------
// Host function and start-invocation tests [host] [start]
// ---------------------------------------------------------------------------

/// Verifies that host io(...) reads the binding value before logging. [host]
#[test]
fn host_io_reads_the_underlying_value_before_logging() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let io_path = InternedPath::from_single_str("io", &mut string_table);

    let assign_message = statement(
        1,
        HirStatementKind::Assign {
            target: crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(LocalId(0)),
            value: string_expression(1, "hello", types.string, RegionId(0)),
        },
        1,
    );

    let call_statement = statement(
        2,
        HirStatementKind::Call {
            target: CallTarget::HostFunction(io_path),
            args: vec![expression(
                2,
                HirExpressionKind::Load(crate::compiler_frontend::hir::hir_nodes::HirPlace::Local(
                    LocalId(0),
                )),
                types.string,
                RegionId(0),
                ValueKind::RValue,
            )],
            result: None,
        },
        2,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.string, RegionId(0))],
        statements: vec![assign_message, call_statement],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
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
        "entry_start",
        vec![block],
        function,
        &[(LocalId(0), "message")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: true,
        },
    )
    .expect("JS lowering should succeed");
    let message_name = expected_dev_local_name("message", 0);

    let assign_index = output
        .source
        .find(&format!("__bs_assign_value({}, \"hello\");", message_name))
        .expect("expected local assignment to store the string value");
    let log_index = output
        .source
        .find(&format!("console.log(__bs_read({}));", message_name))
        .expect("expected host io call to read from the local binding");

    assert!(
        assign_index < log_index,
        "host logging should occur after assigning the local value"
    );
}

/// Verifies that auto_invoke_start emits a call to the start function. [start]
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
        return_aliases: vec![],
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
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        JsLoweringConfig {
            pretty: true,
            emit_locations: false,
            auto_invoke_start: true,
        },
    )
    .expect("JS lowering should succeed");
    let start_name = expected_dev_function_name("start_main", 0);

    assert!(output.source.contains(&format!("{}();", start_name)));
}

// ---------------------------------------------------------------------------
// Function name map test [names]
// ---------------------------------------------------------------------------

/// Verifies that function_name_by_id exposes stable JS names for runtime-fragment lookup. [names]
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
            return_aliases: vec![],
        },
        HirFunction {
            id: FunctionId(1),
            entry: BlockId(1),
            params: vec![],
            return_type: types.unit,
            return_aliases: vec![],
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
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let expected_start = expected_dev_function_name("start", 0);
    let expected_fragment = expected_dev_function_name("__bst_frag_0", 1);

    assert_eq!(
        output
            .function_name_by_id
            .get(&FunctionId(0))
            .map(String::as_str),
        Some(expected_start.as_str())
    );
    assert_eq!(
        output
            .function_name_by_id
            .get(&FunctionId(1))
            .map(String::as_str),
        Some(expected_fragment.as_str())
    );
}

// ---------------------------------------------------------------------------
// Error handling test [error]
// ---------------------------------------------------------------------------

/// Verifies that an unsupported OptionConstruct expression returns a compiler error. [error]
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

    let error = lower_hir_to_js(
        &module,
        &crate::compiler_frontend::analysis::borrow_checker::BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect_err("OptionConstruct should not be supported yet");

    assert!(error.msg.contains("OptionConstruct"));
}

// ---------------------------------------------------------------------------
// Parameter normalization tests [binding]
// ---------------------------------------------------------------------------

/// Verifies that function parameters emit __bs_param_binding to normalize call arguments. [binding]
#[test]
fn function_parameters_emit_param_binding() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![],
        terminator: HirTerminator::Return(unit_expression(0, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![LocalId(0)],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let module = build_module(
        &mut string_table,
        "takes_arg",
        vec![block],
        function,
        &[(LocalId(0), "arg")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let arg_name = expected_dev_local_name("arg", 0);

    assert!(
        output
            .source
            .contains(&format!("{} = __bs_param_binding({});", arg_name, arg_name)),
        "function parameters must be normalised through __bs_param_binding"
    );
}

// ---------------------------------------------------------------------------
// Borrow-assignment and alias behavior tests [alias]
// ---------------------------------------------------------------------------

/// Verifies that assigning a Load (borrow) to a local emits __bs_assign_borrow. [alias]
#[test]
fn borrow_assignment_emits_assign_borrow() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let assign_source = statement(
        1,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(0)),
            value: int_expression(1, 42, types.int, RegionId(0)),
        },
        1,
    );

    let assign_alias = statement(
        2,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(1)),
            value: expression(
                2,
                HirExpressionKind::Load(HirPlace::Local(LocalId(0))),
                types.int,
                RegionId(0),
                ValueKind::RValue,
            ),
        },
        2,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![
            local(0, types.int, RegionId(0)),
            local(1, types.int, RegionId(0)),
        ],
        statements: vec![assign_source, assign_alias],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, RegionId(0))),
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
        &[(LocalId(0), "source"), (LocalId(1), "alias")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let alias_name = expected_dev_local_name("alias", 1);
    let source_name = expected_dev_local_name("source", 0);

    assert!(
        output.source.contains(&format!(
            "__bs_assign_borrow({}, {})",
            alias_name, source_name
        )),
        "Load assignment to a fresh local must emit __bs_assign_borrow"
    );
}

/// Verifies that an alias local is read through __bs_read in a host io call. [binding]
#[test]
fn alias_local_read_emits_bs_read() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let io_path = InternedPath::from_single_str("io", &mut string_table);

    let assign_source = statement(
        1,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(0)),
            value: int_expression(1, 99, types.int, RegionId(0)),
        },
        1,
    );

    let assign_alias = statement(
        2,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(1)),
            value: expression(
                2,
                HirExpressionKind::Load(HirPlace::Local(LocalId(0))),
                types.int,
                RegionId(0),
                ValueKind::RValue,
            ),
        },
        2,
    );

    let log_alias = statement(
        3,
        HirStatementKind::Call {
            target: CallTarget::HostFunction(io_path),
            args: vec![expression(
                3,
                HirExpressionKind::Load(HirPlace::Local(LocalId(1))),
                types.int,
                RegionId(0),
                ValueKind::RValue,
            )],
            result: None,
        },
        3,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![
            local(0, types.int, RegionId(0)),
            local(1, types.int, RegionId(0)),
        ],
        statements: vec![assign_source, assign_alias, log_alias],
        terminator: HirTerminator::Return(unit_expression(4, types.unit, RegionId(0))),
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
        &[(LocalId(0), "source"), (LocalId(1), "alias")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let alias_name = expected_dev_local_name("alias", 1);

    assert!(
        output
            .source
            .contains(&format!("console.log(__bs_read({}))", alias_name)),
        "reading an alias local in a host call must go through __bs_read"
    );
}

/// Verifies that assigning to an alias-only local emits __bs_write instead of __bs_assign_value. [alias]
#[test]
fn alias_only_local_assignment_emits_write() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let assign = statement(
        1,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(0)),
            value: int_expression(1, 42, types.int, RegionId(0)),
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![assign],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
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
        &[(LocalId(0), "target")],
        type_context,
    );

    // Mark the local as alias-only at the assignment statement so the emitter takes the
    // __bs_write path instead of __bs_assign_value.
    let mut report = BorrowCheckReport::default();
    report.analysis.statement_entry_states.insert(
        HirNodeId(1),
        BorrowStateSnapshot {
            locals: vec![LocalBorrowSnapshot {
                local: LocalId(0),
                mode: LocalMode::ALIAS,
                alias_roots: vec![],
            }],
        },
    );

    let output = lower_hir_to_js(&module, &report, &string_table, default_config())
        .expect("JS lowering should succeed");
    let target_name = expected_dev_local_name("target", 0);

    assert!(
        output
            .source
            .contains(&format!("__bs_write({}, 42)", target_name)),
        "alias-only local assignment must emit __bs_write, not __bs_assign_value"
    );
    assert!(
        !output
            .source
            .contains(&format!("__bs_assign_value({}", target_name)),
        "alias-only local must not use __bs_assign_value"
    );
}

// ---------------------------------------------------------------------------
// Computed-place tests [computed]
// ---------------------------------------------------------------------------

/// Verifies that assigning to a struct field emits __bs_write(__bs_field(...)). [computed]
#[test]
fn field_place_emits_bs_field() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let assign_to_field = statement(
        1,
        HirStatementKind::Assign {
            target: HirPlace::Field {
                base: Box::new(HirPlace::Local(LocalId(0))),
                field: FieldId(0),
            },
            value: int_expression(1, 42, types.int, RegionId(0)),
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![assign_to_field],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let mut module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[(LocalId(0), "my_struct")],
        type_context,
    );

    // Register the struct and field so the field symbol map is populated.
    module.structs = vec![HirStruct {
        id: StructId(0),
        fields: vec![HirField {
            id: FieldId(0),
            ty: types.int,
        }],
    }];
    module.side_table.bind_field_name(
        FieldId(0),
        InternedPath::from_single_str("x", &mut string_table),
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let struct_name = expected_dev_local_name("my_struct", 0);
    let field_name = expected_dev_field_name("x", 0);

    assert!(
        output.source.contains(&format!(
            "__bs_write(__bs_field({}, \"{}\"), 42)",
            struct_name, field_name
        )),
        "field assignment must route through __bs_field and __bs_write"
    );
}

/// Verifies that assigning to a collection index emits __bs_write(__bs_index(...)). [computed]
#[test]
fn index_place_emits_bs_index() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let assign_to_index = statement(
        1,
        HirStatementKind::Assign {
            target: HirPlace::Index {
                base: Box::new(HirPlace::Local(LocalId(0))),
                index: Box::new(int_expression(10, 0, types.int, RegionId(0))),
            },
            value: int_expression(1, 42, types.int, RegionId(0)),
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![assign_to_index],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
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
        &[(LocalId(0), "arr")],
        type_context,
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let array_name = expected_dev_local_name("arr", 0);

    assert!(
        output
            .source
            .contains(&format!("__bs_write(__bs_index({}, 0), 42)", array_name)),
        "index assignment must route through __bs_index and __bs_write"
    );
}

/// Verifies that reading a field place composes __bs_read with __bs_field. [computed]
#[test]
fn computed_place_read_composes_with_bs_read() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let io_path = InternedPath::from_single_str("io", &mut string_table);

    let log_field = statement(
        1,
        HirStatementKind::Call {
            target: CallTarget::HostFunction(io_path),
            args: vec![expression(
                1,
                HirExpressionKind::Load(HirPlace::Field {
                    base: Box::new(HirPlace::Local(LocalId(0))),
                    field: FieldId(0),
                }),
                types.int,
                RegionId(0),
                ValueKind::RValue,
            )],
            result: None,
        },
        1,
    );

    let block = HirBlock {
        id: BlockId(0),
        region: RegionId(0),
        locals: vec![local(0, types.int, RegionId(0))],
        statements: vec![log_field],
        terminator: HirTerminator::Return(unit_expression(2, types.unit, RegionId(0))),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let mut module = build_module(
        &mut string_table,
        "main",
        vec![block],
        function,
        &[(LocalId(0), "my_struct")],
        type_context,
    );

    module.structs = vec![HirStruct {
        id: StructId(0),
        fields: vec![HirField {
            id: FieldId(0),
            ty: types.int,
        }],
    }];
    module.side_table.bind_field_name(
        FieldId(0),
        InternedPath::from_single_str("x", &mut string_table),
    );

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("JS lowering should succeed");
    let struct_name = expected_dev_local_name("my_struct", 0);
    let field_name = expected_dev_field_name("x", 0);

    assert!(
        output.source.contains(&format!(
            "__bs_read(__bs_field({}, \"{}\"))",
            struct_name, field_name
        )),
        "field Load must compose __bs_read around __bs_field"
    );
}
