//! Borrow-checker metadata construction helpers.
//!
//! This module builds the per-function layout and signature-derived metadata that the fixed-point
//! driver needs before it can run transfer over reachable blocks.

use super::state::{FunctionLayout, FunctionLayoutInputs, RootSet};
use super::types::FunctionReturnAliasSummary;
use super::engine::{BorrowChecker, successors};
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, HirExpression, HirExpressionKind, HirFunction, HirPattern, HirPlace, HirStatement,
    HirStatementKind, HirTerminator, LocalId,
};
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::return_borrow_checker_error;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

impl<'a> BorrowChecker<'a> {
    pub(super) fn build_function_param_mutability(&mut self) -> Result<(), CompilerError> {
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

    pub(super) fn build_function_return_alias_summaries(&mut self) -> Result<(), CompilerError> {
        // Signature metadata is authoritative for user-function return aliasing.
        // The classifier is used as a validator so contradictory lowering states
        // are rejected early instead of silently degrading call-site behavior.
        for function in &self.module.functions {
            let mut alias_indices = function
                .return_aliases
                .iter()
                .filter_map(|candidates| candidates.as_ref())
                .flatten()
                .copied()
                .collect::<Vec<_>>();
            alias_indices.sort_unstable();
            alias_indices.dedup();

            let summary = if alias_indices.is_empty() {
                FunctionReturnAliasSummary::Fresh
            } else {
                FunctionReturnAliasSummary::AliasParams(alias_indices)
            };

            let classified = self.classify_function_return_alias(function)?;
            self.validate_return_alias_consistency(function, &summary, &classified)?;
            self.function_return_alias.insert(function.id, summary);
        }

        Ok(())
    }

    fn validate_return_alias_consistency(
        &self,
        function: &HirFunction,
        declared: &FunctionReturnAliasSummary,
        classified: &FunctionReturnAliasSummary,
    ) -> Result<(), CompilerError> {
        let function_location = self.diagnostics.function_error_location(function.id);
        let function_name = self.diagnostics.function_name(function.id);

        match (declared, classified) {
            (
                FunctionReturnAliasSummary::Fresh,
                FunctionReturnAliasSummary::AliasParams(indices),
            ) => {
                return_borrow_checker_error!(
                    format!(
                        "Return alias metadata for function '{}' declares Fresh, but analysis found aliases to parameter index/indices {:?}",
                        function_name, indices
                    ),
                    function_location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Ensure non-alias returns copy values instead of returning parameter-backed places",
                    }
                );
            }
            (
                FunctionReturnAliasSummary::AliasParams(expected_indices),
                FunctionReturnAliasSummary::AliasParams(observed_indices),
            ) => {
                for observed in observed_indices {
                    if expected_indices.contains(observed) {
                        continue;
                    }

                    return_borrow_checker_error!(
                        format!(
                            "Return alias metadata for function '{}' does not include observed alias parameter index {}",
                            function_name, observed
                        ),
                        function_location,
                        {
                            CompilationStage => "Borrow Checking",
                            PrimarySuggestion => "Update return alias metadata so it matches actual returned aliases",
                        }
                    );
                }
            }
            (FunctionReturnAliasSummary::AliasParams(_), FunctionReturnAliasSummary::Fresh)
            | (FunctionReturnAliasSummary::AliasParams(_), FunctionReturnAliasSummary::Unknown) => {
                return_borrow_checker_error!(
                    format!(
                        "Return alias metadata for function '{}' declares aliased return values, but analysis could not validate that aliasing shape",
                        function_name
                    ),
                    function_location,
                    {
                        CompilationStage => "Borrow Checking",
                        PrimarySuggestion => "Return the declared aliased parameter place directly on all paths",
                    }
                );
            }
            _ => {}
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

        for &block_id in &reachable_blocks {
            let block = self.block_by_id_or_error(block_id, function.id)?;
            let HirTerminator::Return(value) = &block.terminator else {
                continue;
            };

            saw_return = true;
            let return_summary = self.classify_return_expression(
                function,
                &reachable_blocks,
                value,
                &param_index_by_local,
            )?;
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

    fn classify_return_expression(
        &self,
        function: &HirFunction,
        reachable_blocks: &[BlockId],
        expression: &HirExpression,
        param_index_by_local: &FxHashMap<LocalId, usize>,
    ) -> Result<FunctionReturnAliasSummary, CompilerError> {
        // Direct `return load(param_root)` is a precise alias.
        // For non-parameter roots, inspect local assignment shape:
        // - direct-load chains from params => AliasParams
        // - pure expression writes => Fresh
        // - mixed/unknown writes (for example call results) => Unknown
        if let HirExpressionKind::Load(place) = &expression.kind {
            let Some(root_local) = root_local_for_place(place) else {
                return Ok(FunctionReturnAliasSummary::Unknown);
            };

            if let Some(param_index) = param_index_by_local.get(&root_local).copied() {
                return Ok(FunctionReturnAliasSummary::AliasParams(vec![param_index]));
            }

            let mut alias_param_indices = Vec::new();
            let mut saw_fresh_write = false;
            let mut saw_unknown_write = false;
            let mut saw_any_write = false;

            let mut queue = VecDeque::new();
            let mut visited = FxHashSet::default();
            queue.push_back(root_local);
            visited.insert(root_local);

            while let Some(local_to_scan) = queue.pop_front() {
                for block_id in reachable_blocks {
                    let block = self.block_by_id_or_error(*block_id, function.id)?;

                    for statement in &block.statements {
                        match &statement.kind {
                            HirStatementKind::Assign { target, value } => {
                                let HirPlace::Local(target_local) = target else {
                                    continue;
                                };
                                if *target_local != local_to_scan {
                                    continue;
                                }

                                saw_any_write = true;

                                if let HirExpressionKind::Load(source_place) = &value.kind {
                                    let Some(source_root_local) =
                                        root_local_for_place(source_place)
                                    else {
                                        saw_unknown_write = true;
                                        continue;
                                    };

                                    if let Some(param_index) =
                                        param_index_by_local.get(&source_root_local).copied()
                                    {
                                        alias_param_indices.push(param_index);
                                        continue;
                                    }

                                    if visited.insert(source_root_local) {
                                        queue.push_back(source_root_local);
                                    }
                                } else {
                                    saw_fresh_write = true;
                                }
                            }
                            HirStatementKind::Call {
                                result: Some(result_local),
                                ..
                            } if *result_local == local_to_scan => {
                                saw_any_write = true;
                                saw_fresh_write = true;
                            }
                            _ => {}
                        }
                    }
                }
            }

            if !alias_param_indices.is_empty() {
                alias_param_indices.sort_unstable();
                alias_param_indices.dedup();

                return Ok(if saw_fresh_write || saw_unknown_write {
                    FunctionReturnAliasSummary::Unknown
                } else {
                    FunctionReturnAliasSummary::AliasParams(alias_param_indices)
                });
            }

            if saw_unknown_write {
                return Ok(FunctionReturnAliasSummary::Unknown);
            }

            if saw_fresh_write || saw_any_write {
                return Ok(FunctionReturnAliasSummary::Fresh);
            }

            return Ok(FunctionReturnAliasSummary::Unknown);
        }

        Ok(FunctionReturnAliasSummary::Fresh)
    }

    pub(super) fn build_function_layout(
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
        let mut local_first_write_order = vec![-1; local_ids.len()];
        let mut local_last_use_order = vec![-1; local_ids.len()];
        let mut statement_order_by_id = FxHashMap::default();
        let mut terminator_order_by_block = FxHashMap::default();
        let mut block_successors = FxHashMap::default();
        let mut block_local_max_use_order = FxHashMap::default();
        let mut next_order_key = 0i32;

        for block_id in reachable_blocks {
            let block = self.block_by_id_or_error(*block_id, function.id)?;
            let mut max_use_order = vec![-1; local_ids.len()];

            for statement in &block.statements {
                // WHAT: assign a deterministic ordinal key for this statement.
                // WHY: same-line source locations are not sufficient for precise move decisions.
                let order_key = next_order_key;
                next_order_key += 1;
                statement_order_by_id.insert(statement.id, order_key);

                collect_statement_written_locals(statement, &mut |local_id| {
                    if let Some(index) = local_index_by_id.get(&local_id).copied() {
                        let first_write = &mut local_first_write_order[index];
                        if *first_write < 0 || order_key < *first_write {
                            *first_write = order_key;
                        }

                        local_last_use_order[index] = local_last_use_order[index].max(order_key);
                        max_use_order[index] = max_use_order[index].max(order_key);
                    }
                });
                collect_statement_loaded_locals(statement, &mut |local_id| {
                    if let Some(index) = local_index_by_id.get(&local_id).copied() {
                        local_last_use_order[index] = local_last_use_order[index].max(order_key);
                        max_use_order[index] = max_use_order[index].max(order_key);
                    }
                });
            }

            // WHAT: terminators also participate in future-use classification.
            // WHY: return/branch conditions can be the last read of a root in the block.
            let terminator_order = next_order_key;
            next_order_key += 1;
            terminator_order_by_block.insert(*block_id, terminator_order);

            collect_terminator_loaded_locals(&block.terminator, &mut |local_id| {
                if let Some(index) = local_index_by_id.get(&local_id).copied() {
                    local_last_use_order[index] = local_last_use_order[index].max(terminator_order);
                    max_use_order[index] = max_use_order[index].max(terminator_order);
                }
            });

            block_local_max_use_order.insert(*block_id, max_use_order);
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
            &block_local_max_use_order,
        );

        Ok(FunctionLayout::new(FunctionLayoutInputs {
            local_ids,
            local_mutable,
            local_regions,
            local_first_write_order,
            local_last_use_order,
            statement_order_by_id,
            terminator_order_by_block,
            block_local_max_use_order,
            block_successors,
            may_use_from_block,
            must_use_from_block,
        }))
    }

    pub(super) fn build_visibility_masks(
        &self,
        function_id: crate::compiler_frontend::hir::hir_nodes::FunctionId,
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
        ancestor: crate::compiler_frontend::hir::hir_nodes::RegionId,
        mut region: crate::compiler_frontend::hir::hir_nodes::RegionId,
        function_id: crate::compiler_frontend::hir::hir_nodes::FunctionId,
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
                    .cloned()
                    .unwrap_or_else(|| self.diagnostics.function_error_location(function_id));

                return_borrow_checker_error!(
                    format!(
                        "Borrow checker could not resolve region '{}' while analyzing block '{}'",
                        region.0, block_id
                    ),
                    location.clone(),
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

    pub(super) fn collect_reachable_blocks(
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
}

fn compute_future_use_sets(
    local_count: usize,
    reachable_blocks: &[BlockId],
    block_successors: &FxHashMap<BlockId, Vec<BlockId>>,
    block_local_max_use_order: &FxHashMap<BlockId, Vec<i32>>,
) -> (FxHashMap<BlockId, RootSet>, FxHashMap<BlockId, RootSet>) {
    // WHAT: derives per-block MAY/MUST future-use summaries by fixed-point propagation.
    // WHY: transfer needs O(1) future-use classification when deciding borrow vs move.
    let mut block_use_sets = FxHashMap::default();
    for block_id in reachable_blocks {
        let mut uses = RootSet::empty(local_count);
        if let Some(max_use_order) = block_local_max_use_order.get(block_id) {
            for (local_index, order_key) in max_use_order.iter().enumerate() {
                if *order_key >= 0 {
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
        HirStatementKind::PushRuntimeFragment { vec_local, value } => {
            visitor(*vec_local);
            collect_expression_loaded_locals(value, visitor);
        }
    }
}

fn collect_statement_written_locals(statement: &HirStatement, visitor: &mut impl FnMut(LocalId)) {
    match &statement.kind {
        HirStatementKind::Assign { target, .. } => collect_place_written_local(target, visitor),
        HirStatementKind::Call {
            result: Some(local),
            ..
        } => visitor(*local),
        HirStatementKind::Call { result: None, .. }
        | HirStatementKind::Expr(_)
        | HirStatementKind::Drop(_)
        | HirStatementKind::PushRuntimeFragment { .. } => {}
    }
}

fn collect_place_written_local(place: &HirPlace, visitor: &mut impl FnMut(LocalId)) {
    match place {
        HirPlace::Local(local) => visitor(*local),
        HirPlace::Field { base, .. } | HirPlace::Index { base, .. } => {
            collect_place_written_local(base, visitor)
        }
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
        HirTerminator::Break { .. } | HirTerminator::Continue { .. } => {}
    }
}

fn collect_expression_loaded_locals(expression: &HirExpression, visitor: &mut impl FnMut(LocalId)) {
    match &expression.kind {
        HirExpressionKind::Load(place) => collect_place_loaded_locals(place, visitor),
        HirExpressionKind::Copy(place) => collect_place_loaded_locals(place, visitor),
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
        HirExpressionKind::TupleGet { tuple, .. } => {
            collect_expression_loaded_locals(tuple, visitor);
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
        HirExpressionKind::ResultPropagate { result } => {
            collect_expression_loaded_locals(result, visitor);
        }
        HirExpressionKind::ResultIsOk { result }
        | HirExpressionKind::ResultUnwrapOk { result }
        | HirExpressionKind::ResultUnwrapErr { result }
        | HirExpressionKind::BuiltinCast { value: result, .. } => {
            collect_expression_loaded_locals(result, visitor);
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
