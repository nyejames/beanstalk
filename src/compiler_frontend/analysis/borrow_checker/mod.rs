//! Borrow Checker Driver
//!
//! This module orchestrates borrow checking for a complete HIR module.
//! It builds function metadata, runs a forward fixed-point dataflow analysis
//! per function, and stores snapshots/facts for downstream phases.

mod diagnostics;
mod metadata;
mod state;
mod transfer;
mod types;

pub(crate) use types::{
    BorrowAnalysis, BorrowCheckReport, BorrowCheckStats, BorrowDropSite, BorrowDropSiteKind,
    LocalMode,
};
#[cfg(test)]
pub(crate) use types::{BorrowStateSnapshot, LocalBorrowSnapshot};
pub(crate) type BorrowFacts = BorrowAnalysis;

use crate::borrow_log;
use crate::compiler_frontend::analysis::borrow_checker::diagnostics::BorrowDiagnostics;
use crate::compiler_frontend::analysis::borrow_checker::state::{BorrowState, FunctionLayout};
use crate::compiler_frontend::analysis::borrow_checker::transfer::{
    BlockTransferStats, BorrowTransferContext, transfer_block,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{
    FunctionBorrowSummary, FunctionReturnAliasSummary,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirFunction, HirModule, HirTerminator, LocalId, RegionId,
};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::host_functions::HostRegistry;
use crate::compiler_frontend::string_interning::StringTable;
use crate::return_borrow_checker_error;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub(crate) fn check_borrows(
    module: &HirModule,
    host_registry: &HostRegistry,
    string_table: &StringTable,
) -> Result<BorrowCheckReport, CompilerError> {
    BorrowChecker::new(module, host_registry, string_table).run()
}

struct BorrowChecker<'a> {
    module: &'a HirModule,
    host_registry: &'a HostRegistry,
    string_table: &'a StringTable,
    diagnostics: BorrowDiagnostics<'a>,
    // Fast ID lookups used throughout analysis.
    block_index_by_id: FxHashMap<BlockId, usize>,
    region_parent_by_id: FxHashMap<RegionId, Option<RegionId>>,
    // Call/signature metadata caches used by transfer for O(1) access.
    function_param_mutability: FxHashMap<FunctionId, Vec<bool>>,
    function_return_alias: FxHashMap<FunctionId, FunctionReturnAliasSummary>,
}

impl<'a> BorrowChecker<'a> {
    fn new(
        module: &'a HirModule,
        host_registry: &'a HostRegistry,
        string_table: &'a StringTable,
    ) -> Self {
        let block_index_by_id = module
            .blocks
            .iter()
            .enumerate()
            .map(|(index, block)| (block.id, index))
            .collect::<FxHashMap<_, _>>();

        let region_parent_by_id = module
            .regions
            .iter()
            .map(|region| (region.id(), region.parent()))
            .collect::<FxHashMap<_, _>>();

        Self {
            module,
            host_registry,
            string_table,
            diagnostics: BorrowDiagnostics::new(module, string_table),
            block_index_by_id,
            region_parent_by_id,
            function_param_mutability: FxHashMap::default(),
            function_return_alias: FxHashMap::default(),
        }
    }

    fn run(mut self) -> Result<BorrowCheckReport, CompilerError> {
        // WHAT: run the module-level borrow-analysis driver in three phases:
        // metadata precomputation, per-function fixed-point transfer, then fact/report assembly.
        // WHY: transfer needs allocation-light O(1) metadata lookups, while downstream tooling
        //      expects one consolidated report containing both facts and summary snapshots.
        self.build_function_param_mutability()?;
        self.build_function_return_alias_summaries()?;

        let mut report = BorrowCheckReport::default();

        for function in &self.module.functions {
            let function_stats = self.analyze_function(function, &mut report)?;
            report.stats.functions_analyzed += 1;
            report.stats.blocks_analyzed += function_stats.reachable_blocks;
            report.stats.worklist_iterations += function_stats.worklist_iterations;
        }

        borrow_log!(format!(
            "[Borrow] Completed borrow checking: functions={} blocks={} states={} facts={{stmt:{} term:{} value:{}}}",
            report.stats.functions_analyzed,
            report.stats.blocks_analyzed,
            report.analysis.total_state_snapshots(),
            report.analysis.statement_facts.len(),
            report.analysis.terminator_facts.len(),
            report.analysis.value_facts.len()
        ));

        Ok(report)
    }

    fn analyze_function(
        &self,
        function: &HirFunction,
        report: &mut BorrowCheckReport,
    ) -> Result<FunctionBorrowSummary, CompilerError> {
        // Function analysis pipeline:
        // 1) Build per-function local layout and visibility masks.
        // 2) Run forward worklist transfer to fixed point.
        // 3) Persist facts + entry/exit snapshots for reachable blocks.
        let reachable_blocks = self.collect_reachable_blocks(function)?;
        let reachable_block_set = reachable_blocks.iter().copied().collect::<FxHashSet<_>>();
        let layout = self.build_function_layout(function, &reachable_blocks)?;
        let visible_locals_by_block =
            self.build_visibility_masks(function.id, &layout, &reachable_blocks)?;

        let transfer_context = BorrowTransferContext {
            string_table: self.string_table,
            host_registry: self.host_registry,
            function_param_mutability: &self.function_param_mutability,
            function_return_alias: &self.function_return_alias,
            diagnostics: BorrowDiagnostics::new(self.module, self.string_table),
        };

        let mut in_states: FxHashMap<BlockId, BorrowState> = FxHashMap::default();
        let mut out_states: FxHashMap<BlockId, BorrowState> = FxHashMap::default();

        // Entry state starts as UNINIT, with parameters immediately initialized.
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

        if let Some(mask) = visible_locals_by_block.get(&function.entry) {
            initial_state.kill_invisible(mask);
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
            param_mutability: self
                .function_param_mutability
                .get(&function.id)
                .cloned()
                .unwrap_or_default(),
            return_alias: self
                .function_return_alias
                .get(&function.id)
                .cloned()
                .unwrap_or(FunctionReturnAliasSummary::Unknown),
        };

        let mut alias_heavy = FxHashSet::default();

        borrow_log!(format!(
            "[Borrow] Analyzing function '{}' (entry={} blocks={})",
            self.diagnostics.function_name(function.id),
            function.entry,
            reachable_blocks.len()
        ));

        // Standard forward fixed-point iteration over reachable blocks.
        while let Some(block_id) = worklist.pop_front() {
            summary.worklist_iterations += 1;

            let Some(mut input_state) = in_states.get(&block_id).cloned() else {
                continue;
            };

            if let Some(mask) = visible_locals_by_block.get(&block_id) {
                input_state.kill_invisible(mask);
            }
            in_states.insert(block_id, input_state.clone());

            let block = self.block_by_id_or_error(block_id, function.id)?;
            let mut output_state = input_state.clone();

            // Transfer the block once and collect facts while the state is hot.
            let block_stats = transfer_block(&transfer_context, &layout, block, &mut output_state)?;
            self.merge_block_stats(&mut report.stats, &block_stats);
            summary.mutable_call_sites += block_stats.mutable_call_sites;

            for (statement_id, snapshot) in block_stats.statement_entry_states {
                report
                    .analysis
                    .statement_entry_states
                    .insert(statement_id, snapshot);
            }
            for (statement_id, fact) in block_stats.statement_facts {
                report.analysis.statement_facts.insert(statement_id, fact);
            }
            if let Some((terminator_block, fact)) = block_stats.terminator_fact {
                report
                    .analysis
                    .terminator_facts
                    .insert(terminator_block, fact);
            }
            for (value_id, fact) in block_stats.value_facts {
                report.analysis.value_facts.insert(value_id, fact);
            }

            if output_state.has_any_alias_conflict() {
                alias_heavy.insert(block_id);
            }

            let changed_out = match out_states.get(&block_id) {
                Some(existing) => existing != &output_state,
                None => true,
            };

            // If output state is unchanged, successor joins cannot change either.
            if !changed_out {
                continue;
            }

            out_states.insert(block_id, output_state.clone());

            for successor in successors(&block.terminator) {
                if !reachable_block_set.contains(&successor) {
                    continue;
                }

                // Apply lexical visibility kills before join to prevent
                // branch-local aliases from leaking into outer regions.
                let mut successor_input = output_state.clone();
                if let Some(mask) = visible_locals_by_block.get(&successor) {
                    successor_input.kill_invisible(mask);
                }

                if let Some(existing) = in_states.get(&successor) {
                    self.check_inconsistent_move_join(
                        function.id,
                        successor,
                        &layout,
                        existing,
                        &successor_input,
                    )?;
                }

                let next_state = match in_states.get(&successor) {
                    Some(existing) => existing.join(&successor_input),
                    None => successor_input,
                };

                let changed_in = match in_states.get(&successor) {
                    Some(existing) => existing != &next_state,
                    None => true,
                };

                // Revisit successors only when their input state grows.
                if changed_in {
                    in_states.insert(successor, next_state);
                    worklist.push_back(successor);
                }
            }
        }

        let mut alias_heavy_blocks = alias_heavy.into_iter().collect::<Vec<_>>();
        alias_heavy_blocks.sort_by_key(|id| id.0);
        summary.alias_heavy_blocks = alias_heavy_blocks;

        // Persist snapshots for debug tooling and downstream analyses.
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

        self.record_advisory_drop_sites(function, &reachable_blocks, &layout, &out_states, report)?;

        report
            .analysis
            .function_summaries
            .insert(function.id, summary.clone());

        Ok(summary)
    }

    fn record_advisory_drop_sites(
        &self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
        layout: &FunctionLayout,
        out_states: &FxHashMap<BlockId, BorrowState>,
        report: &mut BorrowCheckReport,
    ) -> Result<(), CompilerError> {
        for block_id in reachable_blocks {
            let Some(exit_state) = out_states.get(block_id) else {
                continue;
            };

            let candidate_locals = self.drop_candidate_locals_from_state(exit_state, layout);
            if candidate_locals.is_empty() {
                continue;
            }

            let block = self.block_by_id_or_error(*block_id, function.id)?;
            let mut block_sites = Vec::new();

            match &block.terminator {
                HirTerminator::Return(_) => {
                    block_sites.push(BorrowDropSite {
                        kind: BorrowDropSiteKind::Return,
                        locals: candidate_locals.clone(),
                    });
                }
                HirTerminator::Break { .. } => {
                    block_sites.push(BorrowDropSite {
                        kind: BorrowDropSiteKind::Break,
                        locals: candidate_locals.clone(),
                    });
                }
                _ => {}
            }

            if self.block_has_region_exit_edge(block.id, block.region, function.id)? {
                block_sites.push(BorrowDropSite {
                    kind: BorrowDropSiteKind::BlockExit,
                    locals: candidate_locals,
                });
            }

            if block_sites.is_empty() {
                continue;
            }

            report
                .analysis
                .advisory_drop_sites
                .entry(*block_id)
                .or_default()
                .extend(block_sites);
        }

        for sites in report.analysis.advisory_drop_sites.values_mut() {
            sites.sort_by_key(|site| match site.kind {
                BorrowDropSiteKind::BlockExit => 0u8,
                BorrowDropSiteKind::Return => 1u8,
                BorrowDropSiteKind::Break => 2u8,
            });
        }

        Ok(())
    }

    fn drop_candidate_locals_from_state(
        &self,
        state: &BorrowState,
        layout: &FunctionLayout,
    ) -> Vec<LocalId> {
        let mut locals = Vec::new();

        for (index, local_id) in layout.local_ids.iter().enumerate() {
            let local_state = state.local_state(index);
            if local_state.mode.contains(LocalMode::SLOT) {
                locals.push(*local_id);
            }
        }

        locals.sort_by_key(|local| local.0);
        locals.dedup_by_key(|local| local.0);
        locals
    }

    fn block_has_region_exit_edge(
        &self,
        block_id: BlockId,
        block_region: RegionId,
        function_id: FunctionId,
    ) -> Result<bool, CompilerError> {
        let block = self.block_by_id_or_error(block_id, function_id)?;

        for successor in successors(&block.terminator) {
            let successor_block = self.block_by_id_or_error(successor, function_id)?;
            if !self.is_same_or_descendant_region(successor_block.region, block_region) {
                return Ok(true);
            }
        }

        Ok(false)
    }

    fn is_same_or_descendant_region(&self, region: RegionId, ancestor: RegionId) -> bool {
        let mut current = Some(region);
        while let Some(region_id) = current {
            if region_id == ancestor {
                return true;
            }

            current = self.region_parent_by_id.get(&region_id).copied().flatten();
        }

        false
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

    fn check_inconsistent_move_join(
        &self,
        function_id: FunctionId,
        successor: BlockId,
        layout: &FunctionLayout,
        existing: &BorrowState,
        incoming: &BorrowState,
    ) -> Result<(), CompilerError> {
        for local_index in 0..layout.local_count() {
            let existing_uninit = existing
                .local_state(local_index)
                .mode
                .is_definitely_uninit();
            let incoming_uninit = incoming
                .local_state(local_index)
                .mode
                .is_definitely_uninit();

            if existing_uninit == incoming_uninit {
                continue;
            }

            let location = self
                .module
                .side_table
                .hir_source_location_for_hir(HirLocation::Block(successor))
                .or_else(|| {
                    self.module
                        .side_table
                        .ast_location_for_hir(HirLocation::Block(successor))
                })
                .cloned()
                .unwrap_or_else(|| self.diagnostics.function_error_location(function_id));

            return_borrow_checker_error!(
                format!(
                    "Inconsistent ownership outcome for '{}' across control-flow paths",
                    self.diagnostics.local_name(layout.local_ids[local_index])
                ),
                location.clone(),
                {
                    CompilationStage => "Borrow Checking",
                    LifetimeHint => "A value cannot be moved on one path and borrowed on another",
                    PrimarySuggestion => "Make ownership outcomes consistent across all branches",
                }
            );
        }

        Ok(())
    }
}

fn successors(terminator: &HirTerminator) -> Vec<BlockId> {
    // Successor extraction for CFG traversal and propagation.
    match terminator {
        HirTerminator::Jump { target, .. } => vec![*target],

        HirTerminator::If {
            then_block,
            else_block,
            ..
        } => vec![*then_block, *else_block],

        HirTerminator::Match { arms, .. } => arms.iter().map(|arm| arm.body).collect::<Vec<_>>(),

        HirTerminator::Break { target } | HirTerminator::Continue { target } => vec![*target],

        HirTerminator::Return(_) | HirTerminator::Panic { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
