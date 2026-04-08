//! Per-function lowering for HIR -> Wasm LIR.

use crate::backends::wasm::hir_to_lir::context::{
    WasmFunctionLoweringContext, WasmLirLoweringContext, lower_type_to_abi,
};
use crate::backends::wasm::hir_to_lir::ownership::insert_advisory_drops;
use crate::backends::wasm::hir_to_lir::stmt::lower_statement;
use crate::backends::wasm::hir_to_lir::templates::lower_runtime_template_function;
use crate::backends::wasm::hir_to_lir::terminator::lower_terminator;
use crate::backends::wasm::lir::function::{WasmLirFunction, WasmLirFunctionOrigin};
use crate::backends::wasm::lir::types::{WasmAbiType, WasmLirSignature, WasmLocalRole};
use crate::compiler_frontend::compiler_messages::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_datatypes::TypeId;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirFunction, HirFunctionOrigin, HirTerminator, LocalId,
};
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub(crate) fn lower_function(
    module_context: &mut WasmLirLoweringContext<'_>,
    hir_function: &HirFunction,
) -> Result<WasmLirFunction, CompilerError> {
    // WHAT: resolve stable function id assigned during module pre-pass.
    // WHY: preserves deterministic cross-function references.
    let Some(lir_id) = module_context.function_map.get(&hir_function.id).copied() else {
        return Err(CompilerError::lir_transformation(format!(
            "Wasm lowering missing stable function id for {:?}",
            hir_function.id
        )));
    };

    // WHAT: map HIR classification to LIR function origin metadata.
    // WHY: builders/backends need explicit semantic role information.
    let origin = map_function_origin(
        module_context
            .hir_module
            .function_origins
            .get(&hir_function.id)
            .copied()
            .ok_or_else(|| {
                CompilerError::lir_transformation(format!(
                    "Wasm lowering missing function origin for {:?}",
                    hir_function.id
                ))
            })?,
    );
    let signature = lower_function_signature(module_context, hir_function)?;
    let mut function_context = WasmFunctionLoweringContext::new(
        module_context,
        hir_function,
        lir_id,
        build_debug_name(module_context, hir_function),
        origin,
        signature,
    );

    // WHAT: discover and pre-allocate every reachable block/local map entry.
    // WHY: terminator lowering needs block ids available up-front.
    let reachable_blocks = collect_reachable_blocks(function_context.module_context, hir_function)?;
    let local_type_map = collect_local_type_map(
        function_context.module_context,
        &reachable_blocks,
        hir_function.id,
    )?;
    alloc_function_locals(&mut function_context, &local_type_map)?;
    for block_id in &reachable_blocks {
        function_context.alloc_block(*block_id);
    }

    // Runtime template bodies use dedicated string-fragment lowering path.
    if matches!(
        function_context.lir_function.origin,
        WasmLirFunctionOrigin::RuntimeTemplate
    ) {
        lower_runtime_template_function(&mut function_context)?;
        return Ok(function_context.lir_function);
    }

    for block_id in &reachable_blocks {
        // Clone block snapshot so we can mutably borrow function context while lowering.
        let hir_block =
            block_by_id_or_error(function_context.module_context, *block_id, hir_function.id)?
                .clone();

        let mut lowered_statements = Vec::new();
        for statement in &hir_block.statements {
            lower_statement(&mut function_context, statement, &mut lowered_statements)?;
        }

        // WHAT: materialize borrow checker advisory drop points.
        // WHY: ownership remains optimization-only, but phase-1 still records insertion sites.
        insert_advisory_drops(&function_context, *block_id, &mut lowered_statements);
        let lowered_terminator = lower_terminator(
            &mut function_context,
            &hir_block.terminator,
            &mut lowered_statements,
        )?;

        let Some(lir_block) = function_context.block_mut(*block_id) else {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm lowering could not resolve lowered block mapping for {block_id:?}",
            )));
        };
        lir_block.statements = lowered_statements;
        lir_block.terminator = lowered_terminator;
    }

    Ok(function_context.lir_function)
}

fn lower_function_signature(
    context: &WasmLirLoweringContext<'_>,
    function: &HirFunction,
) -> Result<WasmLirSignature, CompilerError> {
    // Phase-1 note:
    // multi-value lowering is intentionally incomplete. We only support zero/one
    // result here and keep the explicit error paths for unsupported shapes.
    let local_type_map = collect_local_type_map(
        context,
        &collect_reachable_blocks(context, function)?,
        function.id,
    )?;

    let mut params = Vec::with_capacity(function.params.len());
    for local_id in &function.params {
        let Some(type_id) = local_type_map.get(local_id).copied() else {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm lowering could not resolve parameter type for local {:?} in function {:?}",
                local_id, function.id
            )));
        };
        params.push(lower_type_to_abi(context, type_id));
    }

    let result_abi = lower_type_to_abi(context, function.return_type);
    let results = if matches!(result_abi, WasmAbiType::Void) {
        Vec::new()
    } else {
        vec![result_abi]
    };

    Ok(WasmLirSignature { params, results })
}

fn alloc_function_locals(
    context: &mut WasmFunctionLoweringContext<'_, '_>,
    local_type_map: &FxHashMap<LocalId, TypeId>,
) -> Result<(), CompilerError> {
    // WHAT: allocate parameter locals first.
    // WHY: stable argument ordering is required for call ABI correctness.
    let param_set = context
        .hir_function
        .params
        .iter()
        .copied()
        .collect::<FxHashSet<_>>();

    for param_local in &context.hir_function.params {
        let Some(type_id) = local_type_map.get(param_local).copied() else {
            return Err(CompilerError::lir_transformation(format!(
                "Wasm lowering missing type for parameter local {param_local:?}",
            )));
        };

        let abi_type = lower_type_to_abi(context.module_context, type_id);
        let lir_local = context.alloc_local(
            Some(format!("arg_{}", param_local.0)),
            abi_type,
            WasmLocalRole::Param,
        );
        context.local_map.insert(*param_local, lir_local);
    }

    // WHAT: allocate remaining user locals in sorted local-id order.
    // WHY: deterministic id assignment helps testing and future debug mapping.
    let mut remaining_locals = local_type_map.keys().copied().collect::<Vec<_>>();
    remaining_locals.sort_by_key(|local_id| local_id.0);

    for local_id in remaining_locals {
        if param_set.contains(&local_id) {
            continue;
        }

        let type_id = local_type_map[&local_id];
        let abi_type = lower_type_to_abi(context.module_context, type_id);
        let lir_local = context.alloc_local(
            Some(format!("local_{}", local_id.0)),
            abi_type,
            WasmLocalRole::UserLocal,
        );
        context.local_map.insert(local_id, lir_local);
    }

    Ok(())
}

fn collect_local_type_map(
    context: &WasmLirLoweringContext<'_>,
    block_ids: &[BlockId],
    function_id: FunctionId,
) -> Result<FxHashMap<LocalId, TypeId>, CompilerError> {
    let mut map = FxHashMap::default();

    for block_id in block_ids {
        let block = block_by_id_or_error(context, *block_id, function_id)?;
        for local in &block.locals {
            map.entry(local.id).or_insert(local.ty);
        }
    }

    Ok(map)
}

fn collect_reachable_blocks(
    context: &WasmLirLoweringContext<'_>,
    function: &HirFunction,
) -> Result<Vec<BlockId>, CompilerError> {
    // WHAT: explicit CFG reachability walk.
    // WHY: avoids lowering dead blocks and keeps block-id mapping canonical.
    let mut visited = FxHashSet::default();
    let mut queue = VecDeque::new();
    queue.push_back(function.entry);
    visited.insert(function.entry);

    while let Some(block_id) = queue.pop_front() {
        let block = block_by_id_or_error(context, block_id, function.id)?;
        let mut successors = block_successors(&block.terminator);
        successors.sort_by_key(|id| id.0);

        for successor in successors {
            if visited.insert(successor) {
                queue.push_back(successor);
            }
        }
    }

    let mut block_ids = visited.into_iter().collect::<Vec<_>>();
    block_ids.sort_by_key(|id| id.0);
    Ok(block_ids)
}

fn block_successors(terminator: &HirTerminator) -> Vec<BlockId> {
    match terminator {
        HirTerminator::Jump { target, .. }
        | HirTerminator::Break { target }
        | HirTerminator::Continue { target } => vec![*target],
        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],
        HirTerminator::Match { arms, .. } => arms.iter().map(|arm| arm.body).collect(),
        HirTerminator::Return(_) | HirTerminator::Panic { .. } => Vec::new(),
    }
}

fn block_by_id_or_error<'a>(
    context: &'a WasmLirLoweringContext<'_>,
    block_id: BlockId,
    function_id: FunctionId,
) -> Result<&'a crate::compiler_frontend::hir::hir_nodes::HirBlock, CompilerError> {
    context
        .hir_module
        .blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or_else(|| {
            CompilerError::lir_transformation(format!(
                "Wasm lowering could not resolve block {block_id:?} for function {function_id:?}",
            ))
        })
}

fn build_debug_name(
    module_context: &WasmLirLoweringContext<'_>,
    hir_function: &HirFunction,
) -> String {
    // WHAT: derive debug name from source path when available.
    // WHY: improves readability in debug output and name sections.
    if let Some(path) = module_context
        .hir_module
        .side_table
        .function_name_path(hir_function.id)
        && let Some(name) = path.name_str(module_context.string_table)
    {
        return format!("fn_{}_{}", name, hir_function.id.0);
    }
    format!("fn_{}", hir_function.id.0)
}

fn map_function_origin(origin: HirFunctionOrigin) -> WasmLirFunctionOrigin {
    // Intentional 1:1 mapping in phase-1 so origin semantics stay backend-stable.
    match origin {
        HirFunctionOrigin::Normal => WasmLirFunctionOrigin::Normal,
        HirFunctionOrigin::EntryStart => WasmLirFunctionOrigin::EntryStart,
        HirFunctionOrigin::FileStart => WasmLirFunctionOrigin::FileStart,
        HirFunctionOrigin::RuntimeTemplate => WasmLirFunctionOrigin::RuntimeTemplate,
    }
}
