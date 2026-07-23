//! Structural graph and function-origin validation for HIR.
//!
//! WHAT: checks region parents, entry function metadata, function origins, and CFG ownership.
//! WHY: borrow validation and backend lowering both assume every block belongs to exactly one
//! function and that semantic function origins are complete.

use super::HirValidator;
use crate::compiler_frontend::compiler_errors::CompilerError;
use crate::compiler_frontend::hir::functions::HirFunctionOrigin;
use crate::compiler_frontend::hir::hir_side_table::HirLocation;
use crate::compiler_frontend::hir::utils::terminator_targets;
use rustc_hash::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

impl<'a> HirValidator<'a> {
    // -------------------------
    //  Structural Validation
    // -------------------------

    pub(super) fn validate_region_graph(&self) -> Result<(), CompilerError> {
        let parent_by_region = self
            .module
            .regions
            .iter()
            .map(|region| (region.id(), region.parent()))
            .collect::<FxHashMap<_, _>>();

        for region in &self.module.regions {
            if let Some(parent) = region.parent()
                && !self.region_ids.contains(&parent)
            {
                return Err(self.error_with_hir(
                    format!(
                        "Region {} references missing parent region {}",
                        region.id().0,
                        parent.0
                    ),
                    None,
                ));
            }
        }

        for region in &self.module.regions {
            let mut chain = FxHashSet::default();
            let mut current = Some(region.id());

            while let Some(region_id) = current {
                if !chain.insert(region_id) {
                    return Err(self.error_with_hir(
                        format!(
                            "Region parent graph contains a cycle at region {}",
                            region_id.0
                        ),
                        None,
                    ));
                }

                current = parent_by_region.get(&region_id).copied().flatten();
            }
        }

        Ok(())
    }

    pub(super) fn validate_start_function(&self) -> Result<(), CompilerError> {
        if !self.function_ids.contains(&self.module.start_function) {
            return Err(self.error_with_hir(
                format!(
                    "HIR start_function {:?} is not present in module functions",
                    self.module.start_function
                ),
                Some(HirLocation::Function(self.module.start_function)),
            ));
        }

        Ok(())
    }

    pub(super) fn validate_function_origins(&self) -> Result<(), CompilerError> {
        // WHAT: enforce complete and consistent function-origin coverage.
        // WHY: backends rely on this map to preserve entry/runtime semantics.
        if self.module.function_origins.len() != self.module.functions.len() {
            return Err(self.error_with_hir(
                format!(
                    "HIR function_origins contains {} entries, but module has {} functions",
                    self.module.function_origins.len(),
                    self.module.functions.len()
                ),
                None,
            ));
        }

        for function in &self.module.functions {
            if !self.module.function_origins.contains_key(&function.id) {
                return Err(self.error_with_hir(
                    format!("HIR function {:?} is missing an origin entry", function.id),
                    Some(HirLocation::Function(function.id)),
                ));
            }
        }

        if !matches!(
            self.module
                .function_origins
                .get(&self.module.start_function),
            Some(HirFunctionOrigin::EntryStart)
        ) {
            return Err(self.error_with_hir(
                format!(
                    "HIR start function {:?} must be tagged as EntryStart",
                    self.module.start_function
                ),
                Some(HirLocation::Function(self.module.start_function)),
            ));
        }

        for (origin, function_id) in &self.module.function_ids_by_origin {
            if *function_id == self.module.start_function {
                return Err(self.error_with_hir(
                    format!(
                        "HIR implicit start function {:?} must not carry a public function origin {:?}",
                        function_id, origin
                    ),
                    Some(HirLocation::Function(*function_id)),
                ));
            }

            if !self.function_ids.contains(function_id) {
                return Err(self.error_with_hir(
                    format!(
                        "HIR public function origin {:?} references missing function {:?}",
                        origin, function_id
                    ),
                    Some(HirLocation::Function(*function_id)),
                ));
            }
        }

        Ok(())
    }

    pub(super) fn validate_function_cfg_ownership(&mut self) -> Result<(), CompilerError> {
        // WHAT: ensure every block belongs to exactly one function CFG.
        // WHY: prevents cross-function jumps and ensures clear ownership for analysis.
        self.block_owner_by_id.clear();

        for function in &self.module.functions {
            let mut queue = VecDeque::new();
            let mut visited = FxHashSet::default();
            queue.push_back(function.entry);

            while let Some(block_id) = queue.pop_front() {
                if !visited.insert(block_id) {
                    continue;
                }

                self.require_block_id(block_id, Some(HirLocation::Function(function.id)))?;

                if let Some(existing_owner) = self.block_owner_by_id.get(&block_id).copied() {
                    if existing_owner != function.id {
                        return Err(self.error_with_hir(
                            format!(
                                "Block {} is reachable from multiple functions ({:?} and {:?})",
                                block_id, existing_owner, function.id
                            ),
                            Some(HirLocation::Block(block_id)),
                        ));
                    }
                } else {
                    self.block_owner_by_id.insert(block_id, function.id);
                }

                let block = self.block_by_id(block_id)?;
                for successor in terminator_targets(&block.terminator) {
                    queue.push_back(successor);
                }
            }
        }

        for block in &self.module.blocks {
            if self.block_owner_by_id.contains_key(&block.id) {
                continue;
            }

            return Err(self.error_with_hir(
                format!(
                    "Block {} is not reachable from any function entry and has no CFG owner",
                    block.id
                ),
                Some(HirLocation::Block(block.id)),
            ));
        }

        Ok(())
    }

    pub(super) fn validate_function_provenance(&self) -> Result<(), CompilerError> {
        // WHAT: enforce exactly one direct synthetic-interface provenance fact per local function.
        // WHY: the per-function link-fact lane requires complete, in-range coverage. Missing,
        // extra or out-of-range provenance is an internal `CompilerError` because the fact is
        // compiler-owned metadata, not user-facing source state.
        if self.module.function_provenance.len() != self.module.functions.len() {
            return Err(self.error_with_hir(
                format!(
                    "HIR function_provenance contains {} entries, but module has {} functions",
                    self.module.function_provenance.len(),
                    self.module.functions.len()
                ),
                None,
            ));
        }

        for function in &self.module.functions {
            if !self.module.function_provenance.contains_key(&function.id) {
                return Err(self.error_with_hir(
                    format!(
                        "HIR function {:?} is missing a function_provenance synthetic-interface fact",
                        function.id
                    ),
                    Some(HirLocation::Function(function.id)),
                ));
            }
        }

        Ok(())
    }
}
