//! Shared test support for Rust interpreter backend unit tests.
//!
//! WHAT: provides canonical HIR fixture builders, type context helpers, and backend
//!       execution wrappers reused across all interpreter test files.
//! WHY: keeping fixture construction in one place mirrors the JS/Wasm backend test pattern
//!      and prevents each test file from inventing its own slightly-different builders.

use crate::backends::rust_interpreter::exec_ir::{
    ExecBlock, ExecBlockId, ExecConst, ExecConstId, ExecConstValue, ExecFunction,
    ExecFunctionFlags, ExecFunctionId, ExecInstruction, ExecLocal, ExecLocalId, ExecLocalRole,
    ExecModule, ExecModuleId, ExecProgram, ExecStorageType, ExecTerminator,
};
use crate::backends::rust_interpreter::lowering::lower_hir_module_to_exec_program;
use crate::backends::rust_interpreter::request::InterpreterExecutionPolicy;
use crate::backends::rust_interpreter::runtime::RuntimeEngine;
use crate::backends::rust_interpreter::value::Value;
use crate::compiler_frontend::analysis::borrow_checker::BorrowFacts;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_datatypes::{HirType, HirTypeKind, TypeContext, TypeId};
use crate::compiler_frontend::hir::hir_nodes::{
    FunctionId, HirBlock, HirExpression, HirExpressionKind, HirFunction, HirFunctionOrigin,
    HirLocal, HirModule, HirNodeId, HirPlace, HirRegion, HirStatement, HirStatementKind,
    HirValueId, LocalId, RegionId, ValueKind,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;
use crate::compiler_frontend::tests::test_support::test_location;

// ============================================================
// Type context helpers
// ============================================================

/// Canonical stable type IDs for test fixtures.
#[derive(Clone, Copy)]
pub(crate) struct TypeIds {
    pub unit: TypeId,
    pub boolean: TypeId,
    pub int: TypeId,
    pub float: TypeId,
    pub char_type: TypeId,
    pub string: TypeId,
}

pub(crate) fn build_type_context() -> (TypeContext, TypeIds) {
    let mut type_context = TypeContext::default();

    let unit = type_context.insert(HirType {
        kind: HirTypeKind::Unit,
    });
    let boolean = type_context.insert(HirType {
        kind: HirTypeKind::Bool,
    });
    let int = type_context.insert(HirType {
        kind: HirTypeKind::Int,
    });
    let float = type_context.insert(HirType {
        kind: HirTypeKind::Float,
    });
    let char_type = type_context.insert(HirType {
        kind: HirTypeKind::Char,
    });
    let string = type_context.insert(HirType {
        kind: HirTypeKind::String,
    });

    (
        type_context,
        TypeIds {
            unit,
            boolean,
            int,
            float,
            char_type,
            string,
        },
    )
}

// ============================================================
// HIR expression builders
// ============================================================

pub(crate) fn expression(
    id: u32,
    kind: HirExpressionKind,
    ty: TypeId,
    region: RegionId,
    value_kind: ValueKind,
) -> HirExpression {
    HirExpression {
        id: HirValueId(id),
        kind,
        ty,
        value_kind,
        region,
    }
}

pub(crate) fn int_expression(id: u32, value: i64, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Int(value),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(crate) fn bool_expression(id: u32, value: bool, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Bool(value),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(crate) fn float_expression(id: u32, value: f64, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Float(value),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(crate) fn char_expression(id: u32, value: char, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Char(value),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(crate) fn string_expression(
    id: u32,
    value: &str,
    ty: TypeId,
    region: RegionId,
) -> HirExpression {
    expression(
        id,
        HirExpressionKind::StringLiteral(value.to_owned()),
        ty,
        region,
        ValueKind::Const,
    )
}

pub(crate) fn unit_expression(id: u32, ty: TypeId, region: RegionId) -> HirExpression {
    expression(
        id,
        HirExpressionKind::TupleConstruct { elements: vec![] },
        ty,
        region,
        ValueKind::Const,
    )
}

pub(crate) fn load_local_expression(
    id: u32,
    local_id: LocalId,
    ty: TypeId,
    region: RegionId,
) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Load(HirPlace::Local(local_id)),
        ty,
        region,
        ValueKind::Place,
    )
}

pub(crate) fn copy_local_expression(
    id: u32,
    local_id: LocalId,
    ty: TypeId,
    region: RegionId,
) -> HirExpression {
    expression(
        id,
        HirExpressionKind::Copy(HirPlace::Local(local_id)),
        ty,
        region,
        ValueKind::RValue,
    )
}

// ============================================================
// HIR statement / local builders
// ============================================================

pub(crate) fn statement(id: u32, kind: HirStatementKind, line: i32) -> HirStatement {
    HirStatement {
        id: HirNodeId(id),
        kind,
        location: test_location(line),
    }
}

pub(crate) fn local(local_id: u32, ty: TypeId, region: RegionId) -> HirLocal {
    HirLocal {
        id: LocalId(local_id),
        ty,
        mutable: true,
        region,
        source_info: Some(test_location(1)),
    }
}

// ============================================================
// HIR module builder
// ============================================================

pub(crate) fn build_module(
    string_table: &mut StringTable,
    functions: Vec<(HirFunction, InternedPath, HirFunctionOrigin)>,
    blocks: Vec<HirBlock>,
    type_context: TypeContext,
    start_function: FunctionId,
) -> HirModule {
    let mut module = HirModule::new();
    module.functions = functions.iter().map(|(f, _, _)| f.clone()).collect();
    module.blocks = blocks;
    module.start_function = start_function;
    module.type_context = type_context;

    let mut max_region_id = 0u32;
    for block in &module.blocks {
        max_region_id = max_region_id.max(block.region.0);
    }

    module.regions = (0..=max_region_id)
        .map(|region_id| {
            let parent = (region_id != 0).then_some(RegionId(0));
            HirRegion::lexical(RegionId(region_id), parent)
        })
        .collect();

    for (function, path, origin) in functions {
        module.side_table.bind_function_name(function.id, path);
        module.function_origins.insert(function.id, origin);
    }

    for block in &module.blocks {
        for local in &block.locals {
            let local_path =
                InternedPath::from_single_str(&format!("local_{}", local.id.0), string_table);
            module.side_table.bind_local_name(local.id, local_path);
        }
    }

    module
}

pub(crate) fn default_borrow_facts() -> BorrowFacts {
    BorrowFacts::default()
}

// ============================================================
// Backend execution helpers
// ============================================================

/// Lower a HIR module to Exec IR, panicking on error.
pub(crate) fn lower_only(module: &HirModule) -> ExecProgram {
    let string_table = StringTable::new();
    lower_hir_module_to_exec_program(module, &default_borrow_facts(), &string_table)
        .expect("expected lowering to succeed")
}

/// Lower a HIR module to Exec IR, expecting an error.
pub(crate) fn lower_only_expect_error(module: &HirModule) -> CompilerError {
    let string_table = StringTable::new();
    lower_hir_module_to_exec_program(module, &default_borrow_facts(), &string_table)
        .expect_err("expected lowering to fail")
}

/// Lower a HIR module and execute the start function, returning the runtime value.
pub(crate) fn lower_and_execute_start(module: &HirModule) -> Value {
    let exec_program = lower_only(module);
    let mut runtime = RuntimeEngine::new(exec_program, InterpreterExecutionPolicy::NormalHeadless);
    runtime
        .execute_start()
        .expect("expected execution to succeed")
}

/// Lower a HIR module and execute the start function, returning both the value and the
/// runtime engine so callers can inspect heap state.
pub(crate) fn lower_and_execute_start_with_runtime(module: &HirModule) -> (Value, RuntimeEngine) {
    let exec_program = lower_only(module);
    let mut runtime = RuntimeEngine::new(exec_program, InterpreterExecutionPolicy::NormalHeadless);
    let value = runtime
        .execute_start()
        .expect("expected execution to succeed");
    (value, runtime)
}

// ============================================================
// Minimal manual Exec IR builders for runtime-specific tests
// ============================================================

/// Build a single-function ExecProgram for use in runtime and lookup tests.
pub(crate) fn build_simple_exec_program(
    blocks: Vec<ExecBlock>,
    locals: Vec<ExecLocal>,
    constants: Vec<ExecConst>,
    parameter_slots: Vec<ExecLocalId>,
) -> ExecProgram {
    let function_id = ExecFunctionId(0);
    let entry_block = blocks.first().map(|b| b.id).unwrap_or(ExecBlockId(0));

    ExecProgram {
        module: ExecModule {
            id: ExecModuleId(0),
            functions: vec![ExecFunction {
                id: function_id,
                debug_name: "test_fn".to_owned(),
                entry_block,
                parameter_slots,
                locals,
                blocks,
                result_type: ExecStorageType::Unknown,
                flags: ExecFunctionFlags {
                    is_start: true,
                    is_ctfe_allowed: false,
                },
            }],
            constants,
            entry_function: Some(function_id),
        },
    }
}

/// Build an ExecProgram that returns a string heap object when executed.
pub(crate) fn build_manual_exec_program_returning_string(text: &str) -> ExecProgram {
    let const_id = ExecConstId(0);
    let scratch_id = ExecLocalId(0);

    build_simple_exec_program(
        vec![ExecBlock {
            id: ExecBlockId(0),
            instructions: vec![ExecInstruction::LoadConst {
                target: scratch_id,
                const_id,
            }],
            terminator: ExecTerminator::Return {
                value: Some(scratch_id),
            },
        }],
        vec![ExecLocal {
            id: scratch_id,
            debug_name: Some("__scratch".to_owned()),
            storage_type: ExecStorageType::HeapHandle,
            role: ExecLocalRole::InternalScratch,
        }],
        vec![ExecConst {
            id: const_id,
            value: ExecConstValue::String(text.to_owned()),
        }],
        vec![],
    )
}

pub(crate) fn exec_block(
    id: u32,
    instructions: Vec<ExecInstruction>,
    terminator: ExecTerminator,
) -> ExecBlock {
    ExecBlock {
        id: ExecBlockId(id),
        instructions,
        terminator,
    }
}

pub(crate) fn exec_const_int(id: u32, value: i64) -> ExecConst {
    ExecConst {
        id: ExecConstId(id),
        value: ExecConstValue::Int(value),
    }
}

pub(crate) fn exec_const_string(id: u32, value: &str) -> ExecConst {
    ExecConst {
        id: ExecConstId(id),
        value: ExecConstValue::String(value.to_owned()),
    }
}

pub(crate) fn scratch_local(id: u32) -> ExecLocal {
    ExecLocal {
        id: ExecLocalId(id),
        debug_name: Some("__scratch".to_owned()),
        storage_type: ExecStorageType::Unknown,
        role: ExecLocalRole::InternalScratch,
    }
}

pub(crate) fn user_local(id: u32, storage_type: ExecStorageType) -> ExecLocal {
    ExecLocal {
        id: ExecLocalId(id),
        debug_name: Some(format!("local_{id}")),
        storage_type,
        role: ExecLocalRole::UserLocal,
    }
}

// ============================================================
// Value assertion helpers
// ============================================================

pub(crate) fn assert_value_is_int(value: &Value, expected: i64) {
    match value {
        Value::Int(v) => assert_eq!(*v, expected, "expected Int({expected}), got Int({v})"),
        other => panic!("expected Value::Int({expected}), got {other:?}"),
    }
}

pub(crate) fn assert_value_is_bool(value: &Value, expected: bool) {
    match value {
        Value::Bool(v) => assert_eq!(*v, expected, "expected Bool({expected}), got Bool({v})"),
        other => panic!("expected Value::Bool({expected}), got {other:?}"),
    }
}

pub(crate) fn assert_value_is_unit(value: &Value) {
    assert!(
        matches!(value, Value::Unit),
        "expected Value::Unit, got {value:?}"
    );
}

pub(crate) fn assert_value_is_char(value: &Value, expected: char) {
    match value {
        Value::Char(v) => assert_eq!(*v, expected, "expected Char({expected:?}), got Char({v:?})"),
        other => panic!("expected Value::Char({expected:?}), got {other:?}"),
    }
}

pub(crate) fn assert_value_is_float(value: &Value, expected: f64) {
    match value {
        Value::Float(v) => {
            assert!(
                (v - expected).abs() < f64::EPSILON,
                "expected Float({expected}), got Float({v})"
            )
        }
        other => panic!("expected Value::Float({expected}), got {other:?}"),
    }
}
