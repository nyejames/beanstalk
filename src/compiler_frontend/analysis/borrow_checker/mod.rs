//! Borrow Checker Driver
//!
//! This module orchestrates borrow checking for a complete HIR module.
//! It builds function metadata, runs a forward fixed-point dataflow analysis
//! per function, and stores snapshots/facts for downstream phases.

mod diagnostics;
mod state;
mod transfer;
mod types;

#[allow(unused_imports)]
pub(crate) use types::{BorrowAnalysis, BorrowCheckReport, BorrowCheckStats};

use crate::backends::function_registry::HostRegistry;
use crate::borrow_log;
use crate::compiler_frontend::analysis::borrow_checker::diagnostics::BorrowDiagnostics;
use crate::compiler_frontend::analysis::borrow_checker::state::{
    BorrowState, FunctionLayout, RootSet,
};
use crate::compiler_frontend::analysis::borrow_checker::transfer::{
    BlockTransferStats, BorrowTransferContext, transfer_block,
};
use crate::compiler_frontend::analysis::borrow_checker::types::{
    FunctionBorrowSummary, FunctionReturnAliasSummary,
};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_display::HirLocation;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirExpression, HirExpressionKind, HirFunction, HirModule, HirPattern,
    HirPlace, HirStatement, HirStatementKind, HirTerminator, LocalId, RegionId,
};
use crate::compiler_frontend::interned_path::InternedPath;
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
    function_by_path: FxHashMap<InternedPath, FunctionId>,
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
            function_by_path: FxHashMap::default(),
            function_param_mutability: FxHashMap::default(),
            function_return_alias: FxHashMap::default(),
        }
    }

    fn run(mut self) -> Result<BorrowCheckReport, CompilerError> {
        // Build all metadata once so block transfer stays allocation-light
        // and does not repeat module scans.
        self.build_function_lookup()?;
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

    fn build_function_lookup(&mut self) -> Result<(), CompilerError> {
        // Function call targets are path-based, so the checker builds one
        // canonical path -> FunctionId map from side-table bindings.
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
        // Parameter mutability is stored on locals. Gather it once globally,
        // then materialize each function's param mutability vector in order.
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

    fn build_function_return_alias_summaries(&mut self) -> Result<(), CompilerError> {
        // Conservative callee summaries are computed once and reused at each call site.
        for function in &self.module.functions {
            let summary = self.classify_function_return_alias(function)?;
            self.function_return_alias.insert(function.id, summary);
        }

        Ok(())
    }

    fn classify_function_return_alias(
        &self,
        function: &HirFunction,
    ) -> Result<FunctionReturnAliasSummary, CompilerError> {
        // Summary lattice:
        // Fresh < AliasParams(bitset) < Unknown
        // If any reachable return shape is ambiguous, we escalate to Unknown.
        let reachable_blocks = self.collect_reachable_blocks(function)?;
        let mut param_index_by_local = FxHashMap::default();
        for (param_index, param_local) in function.params.iter().enumerate() {
            param_index_by_local.insert(*param_local, param_index);
        }

        let mut summary = FunctionReturnAliasSummary::Fresh;
        let mut saw_return = false;

        for block_id in reachable_blocks {
            let block = self.block_by_id_or_error(block_id, function.id)?;
            let HirTerminator::Return(value) = &block.terminator else {
                continue;
            };

            saw_return = true;
            let return_summary = classify_return_expression(value, &param_index_by_local);
            summary = merge_return_alias(summary, return_summary);

            if matches!(summary, FunctionReturnAliasSummary::Unknown) {
                // Unknown is the lattice top; no additional scanning can improve it.
                break;
            }
        }

        if !saw_return {
            return Ok(FunctionReturnAliasSummary::Unknown);
        }

        Ok(summary)
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
            module: self.module,
            string_table: self.string_table,
            host_registry: self.host_registry,
            function_by_path: &self.function_by_path,
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
        // Borrow state uses dense indices for speed.
        // Build one stable LocalId -> dense index layout from reachable blocks.
        let mut local_info_by_id = FxHashMap::default();

        for block_id in reachable_blocks {
            let block = self.block_by_id_or_error(*block_id, function.id)?;
            for local in &block.locals {
                local_info_by_id.insert(local.id, (local.mutable, local.region));
            }
        }

        for param in &function.params {
            if !local_info_by_id.contains_key(param) {
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

        let mut local_ids = local_info_by_id.keys().copied().collect::<Vec<_>>();
        local_ids.sort_by_key(|local_id| local_id.0);

        let local_mutable = local_ids
            .iter()
            .map(|local_id| local_info_by_id[local_id].0)
            .collect::<Vec<_>>();
        let local_regions = local_ids
            .iter()
            .map(|local_id| local_info_by_id[local_id].1)
            .collect::<Vec<_>>();
        let mut local_index_by_id = FxHashMap::default();
        for (index, local_id) in local_ids.iter().enumerate() {
            local_index_by_id.insert(*local_id, index);
        }

        let reachable_block_set = reachable_blocks.iter().copied().collect::<FxHashSet<_>>();
        let mut local_last_use_line = vec![-1; local_ids.len()];
        let mut block_successors = FxHashMap::default();
        let mut block_local_max_use_line = FxHashMap::default();

        for block_id in reachable_blocks {
            let block = self.block_by_id_or_error(*block_id, function.id)?;
            let mut max_use_line = vec![-1; local_ids.len()];

            for statement in &block.statements {
                let line = statement.location.start_pos.line_number;
                collect_statement_loaded_locals(statement, &mut |local_id| {
                    if let Some(index) = local_index_by_id.get(&local_id).copied() {
                        local_last_use_line[index] = local_last_use_line[index].max(line);
                        max_use_line[index] = max_use_line[index].max(line);
                    }
                });
            }

            let terminator_line = self
                .module
                .side_table
                .hir_source_location_for_hir(HirLocation::Terminator(*block_id))
                .or_else(|| {
                    self.module
                        .side_table
                        .ast_location_for_hir(HirLocation::Terminator(*block_id))
                })
                .map(|location| location.start_pos.line_number)
                .unwrap_or_default();

            collect_terminator_loaded_locals(&block.terminator, &mut |local_id| {
                if let Some(index) = local_index_by_id.get(&local_id).copied() {
                    local_last_use_line[index] = local_last_use_line[index].max(terminator_line);
                    max_use_line[index] = max_use_line[index].max(terminator_line);
                }
            });

            block_local_max_use_line.insert(*block_id, max_use_line);
            block_successors.insert(
                *block_id,
                successors(&block.terminator)
                    .into_iter()
                    .filter(|successor| reachable_block_set.contains(successor))
                    .collect(),
            );
        }

        let (may_use_from_block, must_use_from_block) = compute_future_use_sets(
            local_ids.len(),
            reachable_blocks,
            &block_successors,
            &block_local_max_use_line,
        );

        Ok(FunctionLayout::new(
            local_ids,
            local_mutable,
            local_regions,
            local_last_use_line,
            block_successors,
            block_local_max_use_line,
            may_use_from_block,
            must_use_from_block,
        ))
    }

    fn build_visibility_masks(
        &self,
        function_id: FunctionId,
        layout: &FunctionLayout,
        reachable_blocks: &[BlockId],
    ) -> Result<FxHashMap<BlockId, RootSet>, CompilerError> {
        // A local is visible in a block when local.region is an ancestor
        // of block.region in the lexical region tree.
        let mut masks = FxHashMap::default();

        for block_id in reachable_blocks {
            let block = self.block_by_id_or_error(*block_id, function_id)?;
            let mut mask = RootSet::empty(layout.local_count());

            for (local_index, local_region) in layout.local_regions.iter().enumerate() {
                if self.is_region_ancestor_of(
                    *local_region,
                    block.region,
                    function_id,
                    *block_id,
                )? {
                    mask.insert(local_index);
                }
            }

            masks.insert(*block_id, mask);
        }

        Ok(masks)
    }

    fn is_region_ancestor_of(
        &self,
        ancestor: RegionId,
        mut region: RegionId,
        function_id: FunctionId,
        block_id: BlockId,
    ) -> Result<bool, CompilerError> {
        // Walk parent links from `region` to root.
        loop {
            if region == ancestor {
                return Ok(true);
            }

            let Some(parent) = self.region_parent_by_id.get(&region).copied() else {
                let location = self
                    .module
                    .side_table
                    .hir_source_location_for_hir(HirLocation::Block(block_id))
                    .or_else(|| {
                        self.module
                            .side_table
                            .ast_location_for_hir(HirLocation::Block(block_id))
                    })
                    .map(|text| text.to_error_location(self.string_table))
                    .unwrap_or_else(|| self.diagnostics.function_error_location(function_id));

                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve region '{}' while analyzing block '{}'",
                        region.0, block_id
                    ),
                    location,
                    {
                        CompilationStage => "Borrow Checking",
                    }
                );
            };

            let Some(parent) = parent else {
                return Ok(false);
            };
            region = parent;
        }
    }

    fn collect_reachable_blocks(
        &self,
        function: &HirFunction,
    ) -> Result<Vec<BlockId>, CompilerError> {
        // Breadth-first traversal over explicit terminator successors.
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
                .map(|source| source.to_error_location(self.string_table))
                .unwrap_or_else(|| self.diagnostics.function_error_location(function_id));

            return_borrow_checker_error!(
                format!(
                    "Inconsistent ownership outcome for '{}' across control-flow paths",
                    self.diagnostics.local_name(layout.local_ids[local_index])
                ),
                location,
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

fn compute_future_use_sets(
    local_count: usize,
    reachable_blocks: &[BlockId],
    block_successors: &FxHashMap<BlockId, Vec<BlockId>>,
    block_local_max_use_line: &FxHashMap<BlockId, Vec<i32>>,
) -> (FxHashMap<BlockId, RootSet>, FxHashMap<BlockId, RootSet>) {
    let mut block_use_sets = FxHashMap::default();
    for block_id in reachable_blocks {
        let mut uses = RootSet::empty(local_count);
        if let Some(max_use_line) = block_local_max_use_line.get(block_id) {
            for (local_index, line) in max_use_line.iter().enumerate() {
                if *line >= 0 {
                    uses.insert(local_index);
                }
            }
        }
        block_use_sets.insert(*block_id, uses);
    }

    let mut may_use_from_block = FxHashMap::default();
    for block_id in reachable_blocks {
        may_use_from_block.insert(*block_id, RootSet::empty(local_count));
    }

    let mut changed = true;
    while changed {
        changed = false;
        for block_id in reachable_blocks.iter().rev() {
            let mut next = block_use_sets
                .get(block_id)
                .cloned()
                .unwrap_or_else(|| RootSet::empty(local_count));

            if let Some(successors) = block_successors.get(block_id) {
                for successor in successors {
                    if let Some(successor_may) = may_use_from_block.get(successor) {
                        next.union_with(successor_may);
                    }
                }
            }

            let should_update = may_use_from_block
                .get(block_id)
                .map(|existing| existing != &next)
                .unwrap_or(true);

            if should_update {
                may_use_from_block.insert(*block_id, next);
                changed = true;
            }
        }
    }

    let mut must_use_from_block = FxHashMap::default();
    for block_id in reachable_blocks {
        must_use_from_block.insert(*block_id, RootSet::full(local_count));
    }

    changed = true;
    while changed {
        changed = false;
        for block_id in reachable_blocks.iter().rev() {
            let mut next = block_use_sets
                .get(block_id)
                .cloned()
                .unwrap_or_else(|| RootSet::empty(local_count));

            let successors = block_successors
                .get(block_id)
                .map(Vec::as_slice)
                .unwrap_or(&[]);

            let successor_must = if successors.is_empty() {
                RootSet::empty(local_count)
            } else {
                let mut intersection = RootSet::full(local_count);
                for successor in successors {
                    if let Some(must_set) = must_use_from_block.get(successor) {
                        intersection.intersect_with(must_set);
                    } else {
                        intersection = RootSet::empty(local_count);
                        break;
                    }
                }
                intersection
            };

            next.union_with(&successor_must);

            let should_update = must_use_from_block
                .get(block_id)
                .map(|existing| existing != &next)
                .unwrap_or(true);

            if should_update {
                must_use_from_block.insert(*block_id, next);
                changed = true;
            }
        }
    }

    (may_use_from_block, must_use_from_block)
}

fn merge_return_alias(
    left: FunctionReturnAliasSummary,
    right: FunctionReturnAliasSummary,
) -> FunctionReturnAliasSummary {
    // Conservative join:
    // Unknown dominates; Fresh is neutral; AliasParams unions indices.
    match (left, right) {
        (FunctionReturnAliasSummary::Unknown, _) | (_, FunctionReturnAliasSummary::Unknown) => {
            FunctionReturnAliasSummary::Unknown
        }

        (FunctionReturnAliasSummary::Fresh, other) | (other, FunctionReturnAliasSummary::Fresh) => {
            other
        }

        (
            FunctionReturnAliasSummary::AliasParams(mut left),
            FunctionReturnAliasSummary::AliasParams(right),
        ) => {
            left.extend(right);
            left.sort_unstable();
            left.dedup();
            FunctionReturnAliasSummary::AliasParams(left)
        }
    }
}

fn classify_return_expression(
    expression: &HirExpression,
    param_index_by_local: &FxHashMap<LocalId, usize>,
) -> FunctionReturnAliasSummary {
    // Direct `return load(param_root)` is a precise alias.
    // Loads of non-parameter locals are considered fresh values from the callee frame.
    if let HirExpressionKind::Load(place) = &expression.kind {
        if let Some(root_local) = root_local_for_place(place) {
            return match param_index_by_local.get(&root_local).copied() {
                Some(param_index) => FunctionReturnAliasSummary::AliasParams(vec![param_index]),
                None => FunctionReturnAliasSummary::Fresh,
            };
        }

        return FunctionReturnAliasSummary::Fresh;
    }

    if expression_has_param_load(expression, param_index_by_local) {
        FunctionReturnAliasSummary::Unknown
    } else {
        FunctionReturnAliasSummary::Fresh
    }
}

fn expression_has_param_load(
    expression: &HirExpression,
    param_index_by_local: &FxHashMap<LocalId, usize>,
) -> bool {
    match &expression.kind {
        HirExpressionKind::Load(place) => root_local_for_place(place)
            .map(|local| param_index_by_local.contains_key(&local))
            .unwrap_or(false),
        HirExpressionKind::BinOp { left, right, .. } => {
            expression_has_param_load(left, param_index_by_local)
                || expression_has_param_load(right, param_index_by_local)
        }
        HirExpressionKind::UnaryOp { operand, .. } => {
            expression_has_param_load(operand, param_index_by_local)
        }
        HirExpressionKind::StructConstruct { fields, .. } => fields
            .iter()
            .any(|(_, value)| expression_has_param_load(value, param_index_by_local)),
        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => elements
            .iter()
            .any(|element| expression_has_param_load(element, param_index_by_local)),
        HirExpressionKind::Range { start, end } => {
            expression_has_param_load(start, param_index_by_local)
                || expression_has_param_load(end, param_index_by_local)
        }
        HirExpressionKind::OptionConstruct { value, .. } => value
            .as_ref()
            .map(|value| expression_has_param_load(value, param_index_by_local))
            .unwrap_or(false),
        HirExpressionKind::ResultConstruct { value, .. } => {
            expression_has_param_load(value, param_index_by_local)
        }
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => false,
    }
}

fn root_local_for_place(place: &HirPlace) -> Option<LocalId> {
    match place {
        HirPlace::Local(local) => Some(*local),
        HirPlace::Field { base, .. } | HirPlace::Index { base, .. } => root_local_for_place(base),
    }
}

fn collect_statement_loaded_locals(statement: &HirStatement, visitor: &mut impl FnMut(LocalId)) {
    match &statement.kind {
        HirStatementKind::Assign { target, value } => {
            collect_place_index_loaded_locals(target, visitor);
            collect_expression_loaded_locals(value, visitor);
        }
        HirStatementKind::Call { args, .. } => {
            for arg in args {
                collect_expression_loaded_locals(arg, visitor);
            }
        }
        HirStatementKind::Expr(expression) => {
            collect_expression_loaded_locals(expression, visitor);
        }
        HirStatementKind::Drop(local) => visitor(*local),
    }
}

fn collect_place_index_loaded_locals(place: &HirPlace, visitor: &mut impl FnMut(LocalId)) {
    match place {
        HirPlace::Local(_) => {}
        HirPlace::Field { base, .. } => collect_place_index_loaded_locals(base, visitor),
        HirPlace::Index { base, index } => {
            collect_place_index_loaded_locals(base, visitor);
            collect_expression_loaded_locals(index, visitor);
        }
    }
}

fn collect_terminator_loaded_locals(terminator: &HirTerminator, visitor: &mut impl FnMut(LocalId)) {
    match terminator {
        // Jump argument passing is CFG plumbing, not a semantic value use.
        HirTerminator::Jump { .. } => {}
        HirTerminator::If { condition, .. } => {
            collect_expression_loaded_locals(condition, visitor);
        }
        HirTerminator::Match { scrutinee, arms } => {
            collect_expression_loaded_locals(scrutinee, visitor);
            for arm in arms {
                if let HirPattern::Literal(expression) = &arm.pattern {
                    collect_expression_loaded_locals(expression, visitor);
                }
                if let Some(guard) = &arm.guard {
                    collect_expression_loaded_locals(guard, visitor);
                }
            }
        }
        HirTerminator::Return(value) => {
            collect_expression_loaded_locals(value, visitor);
        }
        HirTerminator::Panic { message } => {
            if let Some(message) = message {
                collect_expression_loaded_locals(message, visitor);
            }
        }
        HirTerminator::Loop { .. }
        | HirTerminator::Break { .. }
        | HirTerminator::Continue { .. } => {}
    }
}

fn collect_expression_loaded_locals(expression: &HirExpression, visitor: &mut impl FnMut(LocalId)) {
    match &expression.kind {
        HirExpressionKind::Load(place) => collect_place_loaded_locals(place, visitor),
        HirExpressionKind::BinOp { left, right, .. } => {
            collect_expression_loaded_locals(left, visitor);
            collect_expression_loaded_locals(right, visitor);
        }
        HirExpressionKind::UnaryOp { operand, .. } => {
            collect_expression_loaded_locals(operand, visitor);
        }
        HirExpressionKind::StructConstruct { fields, .. } => {
            for (_, value) in fields {
                collect_expression_loaded_locals(value, visitor);
            }
        }
        HirExpressionKind::Collection(elements)
        | HirExpressionKind::TupleConstruct { elements } => {
            for element in elements {
                collect_expression_loaded_locals(element, visitor);
            }
        }
        HirExpressionKind::Range { start, end } => {
            collect_expression_loaded_locals(start, visitor);
            collect_expression_loaded_locals(end, visitor);
        }
        HirExpressionKind::OptionConstruct { value, .. } => {
            if let Some(value) = value {
                collect_expression_loaded_locals(value, visitor);
            }
        }
        HirExpressionKind::ResultConstruct { value, .. } => {
            collect_expression_loaded_locals(value, visitor);
        }
        HirExpressionKind::Int(_)
        | HirExpressionKind::Float(_)
        | HirExpressionKind::Bool(_)
        | HirExpressionKind::Char(_)
        | HirExpressionKind::StringLiteral(_) => {}
    }
}

fn collect_place_loaded_locals(place: &HirPlace, visitor: &mut impl FnMut(LocalId)) {
    match place {
        HirPlace::Local(local) => visitor(*local),
        HirPlace::Field { base, .. } => collect_place_loaded_locals(base, visitor),
        HirPlace::Index { base, index } => {
            collect_place_loaded_locals(base, visitor);
            collect_expression_loaded_locals(index, visitor);
        }
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

        HirTerminator::Loop { body, break_target } => vec![*body, *break_target],

        HirTerminator::Break { target } | HirTerminator::Continue { target } => vec![*target],

        HirTerminator::Return(_) | HirTerminator::Panic { .. } => Vec::new(),
    }
}

#[cfg(test)]
mod tests;
