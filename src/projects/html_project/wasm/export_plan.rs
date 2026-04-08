//! Export-plan construction for HTML builder Wasm mode.
//!
//! WHAT: selects which functions must be callable from builder-owned JS orchestration.
//! WHY: the backend must stay generic and only lower exports explicitly requested by builders.

use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirModule, HirStatementKind, HirTerminator, StartFragment,
};
use crate::compiler_frontend::host_functions::CallTarget;
use rustc_hash::FxHashSet;
use std::collections::VecDeque;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlWasmExportPlan {
    /// Deterministic function export assignments used by JS wrapper generation.
    pub function_exports: Vec<HtmlWasmFunctionExport>,
    /// Helper exports required for string interop and memory access from JS.
    pub helper_exports: HtmlWasmHelperExports,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct HtmlWasmFunctionExport {
    /// Function selected from HIR as callable from builder-owned JS orchestration.
    pub function_id: FunctionId,
    /// Stable export symbol name exposed by Wasm (`bst_call_N`).
    pub export_name: String,
    /// Reason the function is exported, used for debug readability and test intent.
    pub purpose: HtmlWasmExportPurpose,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum HtmlWasmExportPurpose {
    JsStartCall,
    RuntimeTemplateCall,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct HtmlWasmHelperExports {
    /// Export linear memory so JS can decode backend-managed UTF-8 buffers.
    pub export_memory: bool,
    /// Export helper that returns string buffer pointer for a string handle.
    pub export_str_ptr: bool,
    /// Export helper that returns string byte length for a string handle.
    pub export_str_len: bool,
    /// Export helper that releases a moved string handle after JS consumption.
    pub export_release: bool,
}

impl HtmlWasmHelperExports {
    /// Enables all currently required helpers for HTML Wasm mode.
    ///
    /// WHAT: turns on the full string interop helper surface.
    /// WHY: phase-1 HTML Wasm mode always depends on these helpers.
    pub(crate) fn all_enabled() -> Self {
        Self {
            export_memory: true,
            export_str_ptr: true,
            export_str_len: true,
            export_release: true,
        }
    }
}

/// Builds the full HTML->Wasm export plan from builder-visible HIR semantics.
///
/// WHAT: walks entry-start reachable blocks and start-fragment runtime functions.
/// WHY: HTML builder keeps entry orchestration in JS and only exports what JS needs to call.
pub(crate) fn build_html_wasm_export_plan(
    hir_module: &HirModule,
) -> Result<HtmlWasmExportPlan, CompilerError> {
    let runtime_template_functions = collect_runtime_template_fragment_functions(hir_module);
    let mut requested_function_ids = FxHashSet::default();

    for function_id in &runtime_template_functions {
        requested_function_ids.insert(*function_id);
    }

    for block_id in collect_reachable_entry_blocks(hir_module)? {
        let block = block_by_id_or_error(hir_module, block_id)?;
        for statement in &block.statements {
            if let HirStatementKind::Call { target, .. } = &statement.kind
                && let CallTarget::UserFunction(function_id) = target
            {
                requested_function_ids.insert(*function_id);
            }
        }
    }

    let mut function_ids = requested_function_ids.into_iter().collect::<Vec<_>>();
    function_ids.sort_by_key(|function_id| function_id.0);

    let mut function_exports = Vec::with_capacity(function_ids.len());
    for (index, function_id) in function_ids.iter().enumerate() {
        let purpose = if runtime_template_functions.contains(function_id) {
            HtmlWasmExportPurpose::RuntimeTemplateCall
        } else {
            HtmlWasmExportPurpose::JsStartCall
        };
        function_exports.push(HtmlWasmFunctionExport {
            function_id: *function_id,
            export_name: format!("bst_call_{index}"),
            purpose,
        });
    }

    Ok(HtmlWasmExportPlan {
        function_exports,
        helper_exports: HtmlWasmHelperExports::all_enabled(),
    })
}

fn collect_runtime_template_fragment_functions(hir_module: &HirModule) -> FxHashSet<FunctionId> {
    // Runtime template fragments must always be exported so slot hydration can invoke them.
    let mut runtime_functions = FxHashSet::default();
    for fragment in &hir_module.start_fragments {
        if let StartFragment::RuntimeStringFn(function_id) = fragment {
            runtime_functions.insert(*function_id);
        }
    }
    runtime_functions
}

fn collect_reachable_entry_blocks(hir_module: &HirModule) -> Result<Vec<BlockId>, CompilerError> {
    // Traverse control flow from the entry block so we only export reachable direct call targets.
    let start_function = hir_module
        .functions
        .iter()
        .find(|function| function.id == hir_module.start_function)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HTML Wasm export planning could not find start function {:?}",
                hir_module.start_function
            ))
        })?;

    let mut queue = VecDeque::new();
    let mut visited = FxHashSet::default();
    queue.push_back(start_function.entry);

    while let Some(block_id) = queue.pop_front() {
        if !visited.insert(block_id) {
            continue;
        }

        let block = block_by_id_or_error(hir_module, block_id)?;
        for successor in terminator_successors(&block.terminator) {
            queue.push_back(successor);
        }
    }

    let mut block_ids = visited.into_iter().collect::<Vec<_>>();
    block_ids.sort_by_key(|block_id| block_id.0);
    Ok(block_ids)
}

fn terminator_successors(terminator: &HirTerminator) -> Vec<BlockId> {
    // Keep CFG successor expansion in one place so traversal rules stay consistent.
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

fn block_by_id_or_error(
    hir_module: &HirModule,
    block_id: BlockId,
) -> Result<&crate::compiler_frontend::hir::hir_nodes::HirBlock, CompilerError> {
    // Convert unexpected missing-block states into deterministic compiler diagnostics.
    hir_module
        .blocks
        .iter()
        .find(|block| block.id == block_id)
        .ok_or_else(|| {
            CompilerError::compiler_error(format!(
                "HTML Wasm export planning could not resolve block {block_id:?}",
            ))
        })
}
