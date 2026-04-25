//! Local materialization tests for Rust interpreter lowering.
//!
//! WHAT: verifies Load/Copy lowering and temporary local layout.
//! WHY: these are the Stage 0 invariants that keep shared access distinct from explicit copies.

use super::test_support::{
    build_module, build_type_context, copy_local_expression, int_expression, load_local_expression,
    local, lower_only, statement, unit_expression,
};
use crate::backends::rust_interpreter::exec_ir::{
    ExecInstruction, ExecLocalId, ExecLocalRole, ExecTerminator,
};
use crate::compiler_frontend::hir::blocks::HirBlock;
use crate::compiler_frontend::hir::functions::{HirFunction, HirFunctionOrigin};
use crate::compiler_frontend::hir::ids::{BlockId, FunctionId, LocalId, RegionId};
use crate::compiler_frontend::hir::places::HirPlace;
use crate::compiler_frontend::hir::statements::HirStatementKind;
use crate::compiler_frontend::hir::terminators::HirTerminator;
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::symbols::string_interning::StringTable;

#[test]
fn load_local_assignment_emits_read_local() {
    let exec_program = lower_assignment_from_local(false);
    let entry_block = &exec_program.module.functions[0].blocks[0];

    assert!(
        entry_block.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                ExecInstruction::ReadLocal {
                    target: ExecLocalId(1),
                    source: ExecLocalId(0),
                }
            )
        }),
        "Load(Local) assignment should emit ReadLocal"
    );
}

#[test]
fn copy_local_assignment_emits_copy_local() {
    let exec_program = lower_assignment_from_local(true);
    let entry_block = &exec_program.module.functions[0].blocks[0];

    assert!(
        entry_block.instructions.iter().any(|instruction| {
            matches!(
                instruction,
                ExecInstruction::CopyLocal {
                    target: ExecLocalId(1),
                    source: ExecLocalId(0),
                }
            )
        }),
        "Copy(Local) assignment should emit CopyLocal"
    );
}

#[test]
fn first_temporary_local_id_does_not_collide_with_user_locals() {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![local(0, types.int, region), local(1, types.int, region)],
        statements: vec![],
        terminator: HirTerminator::Return(int_expression(1, 42, types.int, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.int,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    let exec_program = lower_only(&module);
    let function = &exec_program.module.functions[0];

    assert!(
        function
            .locals
            .iter()
            .any(|local| local.id == ExecLocalId(2) && local.role == ExecLocalRole::Temp),
        "first temporary local should start after the two user locals"
    );

    let returned_local = match &function.blocks[0].terminator {
        ExecTerminator::Return {
            value: Some(local_id),
        } => *local_id,
        other => panic!("expected value return, got {other:?}"),
    };
    assert_eq!(returned_local, ExecLocalId(2));
}

#[test]
fn lowered_function_locals_do_not_include_scratch_local() {
    let exec_program = lower_assignment_from_local(false);
    let function = &exec_program.module.functions[0];

    assert!(
        function
            .locals
            .iter()
            .all(|local| local.debug_name.as_deref() != Some("__scratch")),
        "lowered functions should not contain a persistent scratch local"
    );
}

fn lower_assignment_from_local(
    use_copy: bool,
) -> crate::backends::rust_interpreter::exec_ir::ExecProgram {
    let mut string_table = StringTable::new();
    let (type_context, types) = build_type_context();
    let region = RegionId(0);
    let source_local = LocalId(0);
    let target_local = LocalId(1);

    let value = if use_copy {
        copy_local_expression(2, source_local, types.int, region)
    } else {
        load_local_expression(2, source_local, types.int, region)
    };

    let block = HirBlock {
        id: BlockId(0),
        region,
        locals: vec![local(0, types.int, region), local(1, types.int, region)],
        statements: vec![
            statement(
                1,
                HirStatementKind::Assign {
                    target: HirPlace::Local(source_local),
                    value: int_expression(1, 7, types.int, region),
                },
                1,
            ),
            statement(
                2,
                HirStatementKind::Assign {
                    target: HirPlace::Local(target_local),
                    value,
                },
                2,
            ),
        ],
        terminator: HirTerminator::Return(unit_expression(3, types.unit, region)),
    };

    let function = HirFunction {
        id: FunctionId(0),
        entry: BlockId(0),
        params: vec![],
        return_type: types.unit,
        return_aliases: vec![],
    };

    let path = InternedPath::from_single_str("start", &mut string_table);
    let module = build_module(
        &mut string_table,
        vec![(function, path, HirFunctionOrigin::EntryStart)],
        vec![block],
        type_context,
        FunctionId(0),
    );

    lower_only(&module)
}
