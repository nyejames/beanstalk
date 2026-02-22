mod diagnostics;
mod state;
mod transfer;
mod types;

#[allow(unused_imports)]
pub(crate) use types::{BorrowAnalysis, BorrowCheckReport, BorrowCheckStats};

use crate::borrow_log;
use crate::compiler_frontend::analysis::borrow_checker::diagnostics::BorrowDiagnostics;
use crate::compiler_frontend::analysis::borrow_checker::state::{BorrowState, FunctionLayout};
use crate::compiler_frontend::analysis::borrow_checker::transfer::{
    BlockTransferStats, BorrowTransferContext, transfer_block,
};
use crate::compiler_frontend::analysis::borrow_checker::types::FunctionBorrowSummary;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirFunction, HirModule, HirTerminator,
};
use crate::compiler_frontend::interned_path::InternedPath;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_borrow_checker_error;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub(crate) fn check_borrows(
    module: &HirModule,
    string_table: &StringTable,
) -> Result<BorrowCheckReport, CompilerError> {
    BorrowChecker::new(module, string_table).run()
}

struct BorrowChecker<'a> {
    module: &'a HirModule,
    string_table: &'a StringTable,
    diagnostics: BorrowDiagnostics<'a>,
    block_index_by_id: FxHashMap<BlockId, usize>,
    function_by_path: FxHashMap<InternedPath, FunctionId>,
    function_param_mutability: FxHashMap<FunctionId, Vec<bool>>,
}

impl<'a> BorrowChecker<'a> {
    fn new(module: &'a HirModule, string_table: &'a StringTable) -> Self {
        let block_index_by_id = module
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id, index))
            .collect::<FxHashMap<_, _>>();

        Self {
            module,
            string_table,
            diagnostics: BorrowDiagnostics::new(module, string_table),
            block_index_by_id,
            function_by_path: FxHashMap::default(),
            function_param_mutability: FxHashMap::default(),
        }
    }

    fn run(mut self) -> Result<BorrowCheckReport, CompilerError> {
        self.build_function_lookup()?;
        self.build_function_param_mutability()?;

        let mut report = BorrowCheckReport::default();

        for function in &self.module.functions {
            let function_stats = self.analyze_function(function, &mut report)?;
            report.stats.functions_analyzed += 1;
            report.stats.blocks_analyzed += function_stats.reachable_blocks;
            report.stats.worklist_iterations += function_stats.worklist_iterations;
        }

        borrow_log!(format!(
            "[Borrow] Completed borrow checking: functions={} blocks={} states={}",
            report.stats.functions_analyzed,
            report.stats.blocks_analyzed,
            report.analysis.total_state_snapshots()
        ));

        Ok(report)
    }

    fn build_function_lookup(&mut self) -> Result<(), CompilerError> {
        for function in &self.module.functions {
            let Some(path) = self
                .module
                .side_table
                .function_name_path(function.id)
                .cloned()
            else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker is missing function path binding for '{}'",
                        function.id
                    ),
                    self.diagnostics.function_error_location(function.id),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            self.function_by_path.insert(path, function.id);
        }

        Ok(())
    }

    fn build_function_param_mutability(&mut self) -> Result<(), CompilerError> {
        let mut local_mutability_by_id = FxHashMap::default();
        for block in &self.module.blocks {
            for local in &block.locals {
                local_mutability_by_id.insert(local.id, local.mutable);
            }
        }

        for function in &self.module.functions {
            let mut param_mutability = Vec::with_capacity(function.params.len());

            for param in &function.params {
                let Some(is_mutable) = local_mutability_by_id.get(param).copied() else {
                    return_borrow_checker_error!(
                        format!(
                            "Borrow checker could not resolve mutability for parameter local '{}' in function '{}'",
                            self.diagnostics.local_name(*param),
                            self.diagnostics.function_name(function.id)
                        ),
                        self.diagnostics.function_error_location(function.id),
                        {
                            CompilationStage => "Borrow Checking",
                        }
                    );
                };

                param_mutability.push(is_mutable);
            }

            self.function_param_mutability
                .insert(function.id, param_mutability);
        }

        Ok(())
    }

    fn analyze_function(
        &self,
        function: &HirFunction,
        report: &mut BorrowCheckReport,
    ) -> Result<FunctionBorrowSummary, CompilerError> {
        let reachable_blocks = self.collect_reachable_blocks(function)?;
        let layout = self.build_function_layout(function, &reachable_blocks)?;

        let transfer_context = BorrowTransferContext {
            module: self.module,
            string_table: self.string_table,
            function_by_path: &self.function_by_path,
            function_param_mutability: &self.function_param_mutability,
            diagnostics: BorrowDiagnostics::new(self.module, self.string_table),
        };

        let mut in_states: FxHashMap<BlockId, BorrowState> = FxHashMap::default();
        let mut out_states: FxHashMap<BlockId, BorrowState> = FxHashMap::default();

        let mut initial_state = BorrowState::new_uninitialized(layout.local_count());
        for param in &function.params {
            let Some(param_index) = layout.index_of(*param) else {
                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not map parameter '{}' into function state layout",
                        self.diagnostics.local_name(*param)
                    ),
                    self.diagnostics.function_error_location(function.id),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            initial_state.initialize_parameter(param_index);
        }

        in_states.insert(function.entry, initial_state);

        let mut worklist = VecDeque::new();
        worklist.push_back(function.entry);

        let mut summary = FunctionBorrowSummary {
            entry_block: Some(function.entry),
            reachable_blocks: reachable_blocks.len(),
            mutable_call_sites: 0,
            alias_heavy_blocks: Vec::new(),
            worklist_iterations: 0,
        };

        let mut alias_heavy = FxHashSet::default();

        borrow_log!(format!(
            "[Borrow] Analyzing function '{}' (entry={} blocks={})",
            self.diagnostics.function_name(function.id),
            function.entry,
            reachable_blocks.len()
        ));

        while let Some(block_id) = worklist.pop_front() {
            summary.worklist_iterations += 1;

            let Some(input_state) = in_states.get(&block_id).cloned() else {
                continue;
            };

            let block = self.block_by_id_or_error(block_id, function.id)?;
            let mut output_state = input_state.clone();

            let block_stats = transfer_block(&transfer_context, &layout, block, &mut output_state)?;
            self.merge_block_stats(&mut report.stats, &block_stats);
            summary.mutable_call_sites += block_stats.mutable_call_sites;

            if output_state.has_any_alias_conflict() {
                alias_heavy.insert(block_id);
            }

            let changed_out = match out_states.get(&block_id) {
                Some(existing) => existing != &output_state,
                None => true,
            };

            if !changed_out {
                continue;
            }

            out_states.insert(block_id, output_state.clone());

            for successor in successors(&block.terminator) {
                if !reachable_blocks.contains(&successor) {
                    continue;
                }

                let next_state = match in_states.get(&successor) {
                    Some(existing) => existing.join(&output_state),
                    None => output_state.clone(),
                };

                let changed_in = match in_states.get(&successor) {
                    Some(existing) => existing != &next_state,
                    None => true,
                };

                if changed_in {
                    in_states.insert(successor, next_state);
                    worklist.push_back(successor);
                }
            }
        }

        let mut alias_heavy_blocks = alias_heavy.into_iter().collect::<Vec<_>>();
        alias_heavy_blocks.sort_by_key(|id| id.0);
        summary.alias_heavy_blocks = alias_heavy_blocks;

        for block_id in &reachable_blocks {
            if let Some(state) = in_states.get(block_id) {
                report
                    .analysis
                    .block_entry_states
                    .insert(*block_id, state.to_snapshot(&layout.local_ids));
            }

            if let Some(state) = out_states.get(block_id) {
                report
                    .analysis
                    .block_exit_states
                    .insert(*block_id, state.to_snapshot(&layout.local_ids));
            }
        }

        report
            .analysis
            .function_summaries
            .insert(function.id, summary.clone());

        Ok(summary)
    }

    fn build_function_layout(
        &self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
    ) -> Result<FunctionLayout, CompilerError> {
        let mut local_mutability_by_id = FxHashMap::default();

        for block_id in reachable_blocks {
            let block = self.block_by_id_or_error(*block_id, function.id)?;
            for local in &block.locals {
                local_mutability_by_id.insert(local.id, local.mutable);
            }
        }

        for param in &function.params {
            if !local_mutability_by_id.contains_key(param) {
                return_borrow_checker_error!(
                    format!(
                        "Function '{}' parameter '{}' is missing from reachable local layout",
                        self.diagnostics.function_name(function.id),
                        self.diagnostics.local_name(*param)
                    ),
                    self.diagnostics.function_error_location(function.id),
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            }
        }

        let mut local_ids = local_mutability_by_id.keys().copied().collect::<Vec<_>>();
        local_ids.sort_by_key(|local_id| local_id.0);

        let local_mutable = local_ids
            .iter()
            .map(|local_id| local_mutability_by_id[local_id])
            .collect::<Vec<_>>();

        Ok(FunctionLayout::new(local_ids, local_mutable))
    }

    fn collect_reachable_blocks(
        &self,
        function: &HirFunction,
    ) -> Result<Vec<BlockId>, CompilerError> {
        let mut visited = FxHashSet::default();
        let mut order = Vec::new();
        let mut queue = VecDeque::new();

        queue.push_back(function.entry);

        while let Some(block_id) = queue.pop_front() {
            if !visited.insert(block_id) {
                continue;
            }

            order.push(block_id);

            let block = self.block_by_id_or_error(block_id, function.id)?;
            for next in successors(&block.terminator) {
                queue.push_back(next);
            }
        }

        Ok(order)
    }

    fn block_by_id_or_error(
        &self,
        block_id: BlockId,
        function_id: FunctionId,
    ) -> Result<&crate::compiler_frontend::hir::hir_nodes::HirBlock, CompilerError> {
        let Some(index) = self.block_index_by_id.get(&block_id).copied() else {
            return_borrow_checker_error!(
                format!(
                    "Borrow checker could not resolve block '{}' while analyzing function '{}'",
                    block_id,
                    self.diagnostics.function_name(function_id)
                ),
                self.diagnostics.function_error_location(function_id),
                {
                    CompilationStage => "Borrow Checking",
                }
            );
        };

        Ok(&self.module.blocks[index])
    }

    fn merge_block_stats(&self, total: &mut BorrowCheckStats, block: &BlockTransferStats) {
        total.statements_analyzed += block.statements_analyzed;
        total.terminators_analyzed += block.terminators_analyzed;
        total.conflicts_checked += block.conflicts_checked;
    }
}

fn successors(terminator: &HirTerminator) -> Vec<BlockId> {
    match terminator {
        HirTerminator::Jump { target, .. } => vec![*target],

        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],

        HirTerminator::Match { arms, .. } => arms.iter().map(|arm| arm.body).collect::<Vec<_>>(),

        HirTerminator::Loop { body, break_target } => vec![*body, *break_target],

        HirTerminator::Break { target } | HirTerminator::Continue { target } => vec![*target],

        HirTerminator::Return(_) | HirTerminator::Panic { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
