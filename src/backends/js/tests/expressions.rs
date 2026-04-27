//! Expression lowering tests for JavaScript output.

use super::support::*;
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::expressions::{
    HirExpressionKind, HirVariantCarrier, HirVariantField, ValueKind,
};
use crate::compiler_frontend::hir::functions::HirFunction;
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, LocalId, RegionId};
use crate::compiler_frontend::hir::operators::HirBinOp;
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;

#[test]
fn integer_division_binop_emits_zero_checked_truncation_path() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let assign_lhs = statement(
        1,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(0)),
            value: int_expression(1, 10, types.int, region),
        },
        1,
    );
    let assign_rhs = statement(
        2,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(1)),
            value: int_expression(2, 3, types.int, region),
        },
        2,
    );
    let int_div_expr = expression(
        3,
        HirExpressionKind::BinOp {
            left: Box::new(expression(
                4,
                HirExpressionKind::Load(HirPlace::Local(LocalId(0))),
                types.int,
                region,
                ValueKind::Place,
            )),
            op: HirBinOp::IntDiv,
            right: Box::new(expression(
                5,
                HirExpressionKind::Load(HirPlace::Local(LocalId(1))),
                types.int,
                region,
                ValueKind::Place,
            )),
        },
        types.int,
        region,
        ValueKind::RValue,
    );
    let assign_result = statement(
        3,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(2)),
            value: int_div_expr,
        },
        3,
    );

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![
            local(0, types.int, region),
            local(1, types.int, region),
            local(2, types.int, region),
        ],
        statements: vec![assign_lhs, assign_rhs, assign_result],
        terminator: HirTerminator::Return(unit_expression(6, types.unit, region)),
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
        &[
            (LocalId(0), "lhs"),
            (LocalId(1), "rhs"),
            (LocalId(2), "result"),
        ],
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
        output.source.contains("Math.trunc("),
        "integer division should use truncation path"
    );
    assert!(
        output.source.contains("Integer division by zero"),
        "integer division path should include explicit zero trap"
    );
    assert!(
        output.source.contains("__rhs === 0"),
        "integer division path should branch on zero divisor"
    );
}

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
            target: HirPlace::Local(LocalId(0)),
            value: string_expression(1, "hello", types.string, RegionId(0)),
        },
        1,
    );

    let copy_expr = expression(
        2,
        HirExpressionKind::Copy(HirPlace::Local(LocalId(0))),
        types.string,
        RegionId(0),
        ValueKind::RValue,
    );

    let assign_copy = statement(
        2,
        HirStatementKind::Assign {
            target: HirPlace::Local(LocalId(1)),
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
        &BorrowCheckReport::default(),
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

// Error handling test [error]
// ---------------------------------------------------------------------------

/// Verifies that VariantConstruct with Option carrier lowers to tagged JS objects. [option]
#[test]
fn lowers_option_construct_expression() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();

    let value_name = string_table.intern("value");
    let option_value = expression(
        1,
        HirExpressionKind::VariantConstruct {
            carrier: HirVariantCarrier::Option,
            variant_index: 1,
            fields: vec![HirVariantField {
                name: Some(value_name),
                value: int_expression(2, 10, types.int, RegionId(0)),
            }],
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

    let output = lower_hir_to_js(
        &module,
        &BorrowCheckReport::default(),
        &string_table,
        default_config(),
    )
    .expect("VariantConstruct(Option) lowering should succeed");

    assert!(
        output.source.contains("{ tag: \"some\", value: 10 }"),
        "expected VariantConstruct(Option) to lower into a tagged JS object"
    );
}

// ---------------------------------------------------------------------------
