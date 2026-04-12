//! Function lowering for the Rust interpreter backend.
//!
//! WHAT: lowers per-function local layouts, block structure, and a restricted executable subset.
//! WHY: phase 1 needs a small real execution path before broader expression support lands.

use crate::backends::rust_interpreter::exec_ir::{
    ExecBlock, ExecBlockId, ExecFunction, ExecFunctionFlags, ExecLocal, ExecLocalId, ExecLocalRole,
    ExecStorageType,
};
use crate::backends::rust_interpreter::lowering::context::{
    FunctionLoweringLayout, LoweringContext,
};
use crate::backends::rust_interpreter::lowering::statements::lower_block_statements;
use crate::backends::rust_interpreter::lowering::terminators::lower_block_terminator;
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{BlockId, HirBlock, HirFunction, HirLocal, LocalId};
use crate::compiler_frontend::hir::utils::terminator_targets;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub(crate) fn lower_function_shell(
    context: &mut LoweringContext<'_>,
    function: &HirFunction,
) -> Result<(), CompilerError> {
    let mut layout = build_function_layout(context, function)?;
    let locals_by_id = collect_hir_locals_by_id(context, function, &layout)?;

    let mut exec_locals = Vec::new();
    let mut parameter_slots = Vec::with_capacity(function.params.len());

    for local_id in &layout.ordered_hir_local_ids {
        let Some(exec_local_id) = layout.exec_local_by_hir_local.get(local_id).copied() else {
            return Err(CompilerError::compiler_error(format!(
                "Rust interpreter lowering could not resolve Exec local for HIR local {local_id:?}"
            )));
        };

        let Some(hir_local) = locals_by_id.get(local_id) else {
            return Err(CompilerError::compiler_error(format!(
                "Rust interpreter lowering could not resolve HIR local {local_id:?}"
            )));
        };

        let role = if function.params.contains(local_id) {
            parameter_slots.push(exec_local_id);
            ExecLocalRole::Param
        } else {
            ExecLocalRole::UserLocal
        };

        exec_locals.push(ExecLocal {
            id: exec_local_id,
            debug_name: Some(debug_name_for_local(*local_id, hir_local, role)),
            storage_type: context.lower_storage_type(hir_local.ty),
            role,
        });
    }

    exec_locals.push(ExecLocal {
        id: layout.scratch_local_id,
        debug_name: Some("__scratch".to_owned()),
        storage_type: ExecStorageType::Unknown,
        role: ExecLocalRole::InternalScratch,
    });

    let mut exec_blocks = Vec::with_capacity(layout.ordered_hir_block_ids.len());
    let ordered_hir_block_ids = layout.ordered_hir_block_ids.clone();
    for hir_block_id in &ordered_hir_block_ids {
        let Some(exec_block_id) = layout.exec_block_by_hir_block.get(hir_block_id).copied() else {
            return Err(CompilerError::compiler_error(format!(
                "Rust interpreter lowering could not resolve Exec block for HIR block {hir_block_id:?}"
            )));
        };

        let hir_block = context.hir_block_by_id(*hir_block_id)?.clone();
        exec_blocks.push(lower_block(
            context,
            &mut layout,
            &hir_block,
            exec_block_id,
        )?);
    }

    // Add temporary locals to the exec_locals list after all blocks are lowered
    exec_locals.extend(layout.temp_locals.clone());

    let Some(entry_block) = layout.exec_block_by_hir_block.get(&function.entry).copied() else {
        return Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering could not resolve entry block for function {:?}",
            function.id
        )));
    };

    let exec_function = ExecFunction {
        id: layout.exec_function_id,
        debug_name: format!("fn_{}", function.id.0),
        entry_block,
        parameter_slots,
        locals: exec_locals,
        blocks: exec_blocks,
        result_type: context.lower_storage_type(function.return_type),
        flags: ExecFunctionFlags {
            is_start: function.id == context.hir_module.start_function,
            is_ctfe_allowed: false,
        },
    };

    context.exec_program.module.functions.push(exec_function);
    Ok(())
}

fn build_function_layout(
    context: &LoweringContext<'_>,
    function: &HirFunction,
) -> Result<FunctionLoweringLayout, CompilerError> {
    let Some(exec_function_id) = context.function_id_by_hir_id.get(&function.id).copied() else {
        return Err(CompilerError::compiler_error(format!(
            "Rust interpreter lowering could not resolve Exec function id for HIR function {:?}",
            function.id
        )));
    };

    let ordered_hir_block_ids = collect_reachable_block_ids(context, function.entry)?;
    let mut exec_block_by_hir_block = FxHashMap::default();

    for (index, block_id) in ordered_hir_block_ids.iter().enumerate() {
        exec_block_by_hir_block.insert(*block_id, ExecBlockId(index as u32));
    }

    let ordered_hir_local_ids =
        collect_function_local_ids(context, function, &ordered_hir_block_ids)?;
    let mut exec_local_by_hir_local = FxHashMap::default();

    for (index, local_id) in ordered_hir_local_ids.iter().enumerate() {
        exec_local_by_hir_local.insert(*local_id, ExecLocalId(index as u32));
    }

    let scratch_local_id = ExecLocalId(ordered_hir_local_ids.len() as u32);

    Ok(FunctionLoweringLayout {
        exec_function_id,
        ordered_hir_block_ids,
        exec_block_by_hir_block,
        ordered_hir_local_ids,
        exec_local_by_hir_local,
        scratch_local_id,
        next_temp_local_index: 0,
        temp_local_count: 0,
        temp_locals: Vec::new(),
    })
}

fn lower_block(
    context: &mut LoweringContext<'_>,
    layout: &mut FunctionLoweringLayout,
    hir_block: &HirBlock,
    exec_block_id: ExecBlockId,
) -> Result<ExecBlock, CompilerError> {
    let mut instructions = lower_block_statements(context, layout, hir_block)?;
    let terminator =
        lower_block_terminator(context, layout, &mut instructions, &hir_block.terminator)?;

    Ok(ExecBlock {
        id: exec_block_id,
        instructions,
        terminator,
    })
}

fn collect_reachable_block_ids(
    context: &LoweringContext<'_>,
    entry_block_id: BlockId,
) -> Result<Vec<BlockId>, CompilerError> {
    let mut worklist = VecDeque::new();
    let mut seen = FxHashSet::default();
    let mut ordered_blocks = Vec::new();

    worklist.push_back(entry_block_id);

    while let Some(block_id) = worklist.pop_front() {
        if !seen.insert(block_id) {
            continue;
        }

        ordered_blocks.push(block_id);

        let block = context.hir_block_by_id(block_id)?;
        for successor in terminator_targets(&block.terminator) {
            if !seen.contains(&successor) {
                worklist.push_back(successor);
            }
        }
    }

    Ok(ordered_blocks)
}

fn collect_function_local_ids(
    context: &LoweringContext<'_>,
    function: &HirFunction,
    ordered_hir_block_ids: &[BlockId],
) -> Result<Vec<LocalId>, CompilerError> {
    let mut seen = FxHashSet::default();
    let mut local_ids = Vec::new();

    for param_id in &function.params {
        if seen.insert(*param_id) {
            local_ids.push(*param_id);
        }
    }

    for block_id in ordered_hir_block_ids {
        let block = context.hir_block_by_id(*block_id)?;
        for local in &block.locals {
            if seen.insert(local.id) {
                local_ids.push(local.id);
            }
        }
    }

    Ok(local_ids)
}

fn collect_hir_locals_by_id(
    context: &LoweringContext<'_>,
    function: &HirFunction,
    layout: &FunctionLoweringLayout,
) -> Result<FxHashMap<LocalId, HirLocal>, CompilerError> {
    let mut locals_by_id = FxHashMap::default();

    for block_id in &layout.ordered_hir_block_ids {
        let block = context.hir_block_by_id(*block_id)?;
        for local in &block.locals {
            locals_by_id.insert(local.id, local.clone());
        }
    }

    for param_id in &function.params {
        if !locals_by_id.contains_key(param_id) {
            return Err(CompilerError::compiler_error(format!(
                "Rust interpreter lowering could not resolve parameter local {:?} in function {:?}",
                param_id, function.id
            )));
        }
    }

    Ok(locals_by_id)
}

fn debug_name_for_local(local_id: LocalId, hir_local: &HirLocal, role: ExecLocalRole) -> String {
    if hir_local.source_info.is_some() {
        return format!("local_{}", local_id.0);
    }

    match role {
        ExecLocalRole::Param => format!("param_{}", local_id.0),
        ExecLocalRole::UserLocal => format!("local_{}", local_id.0),
        ExecLocalRole::Temp => format!("temp_{}", local_id.0),
        ExecLocalRole::InternalScratch => "__scratch".to_owned(),
    }
}
