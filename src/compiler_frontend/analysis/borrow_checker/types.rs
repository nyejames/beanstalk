use crate::compiler_frontend::hir::hir_nodes::{
    BlockId, FunctionId, HirNodeId, HirValueId, LocalId,
};
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
    pub statement_facts: FxHashMap<HirNodeId, StatementBorrowFact>,
    pub terminator_facts: FxHashMap<BlockId, TerminatorBorrowFact>,
    pub value_facts: FxHashMap<HirValueId, ValueBorrowFact>,
}

impl BorrowAnalysis {
    pub(crate) fn total_state_snapshots(&self) -> usize {
        self.block_entry_states.len() + self.block_exit_states.len()
    }

    pub(crate) fn statement_fact(&self, id: HirNodeId) -> Option<&StatementBorrowFact> {
        self.statement_facts.get(&id)
    }

    pub(crate) fn terminator_fact(&self, block: BlockId) -> Option<&TerminatorBorrowFact> {
        self.terminator_facts.get(&block)
    }

    pub(crate) fn value_fact(&self, id: HirValueId) -> Option<&ValueBorrowFact> {
        self.value_facts.get(&id)
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

#[derive(Debug, Clone)]
pub(crate) struct FunctionBorrowSummary {
    pub entry_block: Option<BlockId>,
    pub reachable_blocks: usize,
    pub mutable_call_sites: usize,
    pub alias_heavy_blocks: Vec<BlockId>,
    pub worklist_iterations: usize,
    pub param_mutability: Vec<bool>,
    pub return_alias: FunctionReturnAliasSummary,
}

impl Default for FunctionBorrowSummary {
    fn default() -> Self {
        Self {
            entry_block: None,
            reachable_blocks: 0,
            mutable_call_sites: 0,
            alias_heavy_blocks: Vec::new(),
            worklist_iterations: 0,
            param_mutability: Vec::new(),
            return_alias: FunctionReturnAliasSummary::Unknown,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum FunctionReturnAliasSummary {
    Fresh,
    AliasParams(Vec<usize>),
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct StatementBorrowFact {
    pub shared_roots: Vec<LocalId>,
    pub mutable_roots: Vec<LocalId>,
    pub conflicts_checked: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(crate) struct TerminatorBorrowFact {
    pub shared_roots: Vec<LocalId>,
    pub mutable_roots: Vec<LocalId>,
    pub conflicts_checked: usize,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ValueBorrowFact {
    pub classification: ValueAccessClassification,
    pub roots: Vec<LocalId>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum ValueAccessClassification {
    #[default]
    None,
    SharedRead,
    MutableArgument,
    Mixed,
}

impl ValueAccessClassification {
    pub(crate) fn merge(self, other: Self) -> Self {
        match (self, other) {
            (Self::None, rhs) => rhs,
            (lhs, Self::None) => lhs,
            (Self::SharedRead, Self::SharedRead) => Self::SharedRead,
            (Self::MutableArgument, Self::MutableArgument) => Self::MutableArgument,
            _ => Self::Mixed,
        }
    }
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
