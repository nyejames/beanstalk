use crate::compiler_frontend::hir::hir_nodes::{BlockId, FunctionId, LocalId};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone, Default)]
pub(crate) struct BorrowCheckReport {
    pub analysis: BorrowAnalysis,
    pub stats: BorrowCheckStats,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BorrowAnalysis {
    pub function_summaries: FxHashMap<FunctionId, FunctionBorrowSummary>,
    pub block_entry_states: FxHashMap<BlockId, BorrowStateSnapshot>,
    pub block_exit_states: FxHashMap<BlockId, BorrowStateSnapshot>,
}

impl BorrowAnalysis {
    pub(crate) fn total_state_snapshots(&self) -> usize {
        self.block_entry_states.len() + self.block_exit_states.len()
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BorrowCheckStats {
    pub functions_analyzed: usize,
    pub blocks_analyzed: usize,
    pub statements_analyzed: usize,
    pub terminators_analyzed: usize,
    pub worklist_iterations: usize,
    pub conflicts_checked: usize,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct FunctionBorrowSummary {
    pub entry_block: Option<BlockId>,
    pub reachable_blocks: usize,
    pub mutable_call_sites: usize,
    pub alias_heavy_blocks: Vec<BlockId>,
    pub worklist_iterations: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct BorrowStateSnapshot {
    pub locals: Vec<LocalBorrowSnapshot>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LocalBorrowSnapshot {
    pub local: LocalId,
    pub mode: LocalMode,
    pub alias_roots: Vec<LocalId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) struct LocalMode(u8);

impl LocalMode {
    pub(crate) const UNINIT: Self = Self(0b001);
    pub(crate) const SLOT: Self = Self(0b010);
    pub(crate) const ALIAS: Self = Self(0b100);

    pub(crate) const fn bits(self) -> u8 {
        self.0
    }

    pub(crate) fn contains(self, other: Self) -> bool {
        (self.0 & other.0) == other.0
    }

    pub(crate) fn union(self, other: Self) -> Self {
        Self(self.0 | other.0)
    }

    pub(crate) fn is_definitely_uninit(self) -> bool {
        self.0 == Self::UNINIT.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AccessKind {
    Shared,
    Mutable,
}
