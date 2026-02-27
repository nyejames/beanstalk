use crate::compiler_frontend::analysis::borrow_checker::types::{
    BorrowStateSnapshot, LocalBorrowSnapshot, LocalMode,
};
use crate::compiler_frontend::hir::hir_nodes::{BlockId, LocalId, RegionId};
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub(super) struct FunctionLayout {
    pub local_ids: Vec<LocalId>,
    pub local_index_by_id: FxHashMap<LocalId, usize>,
    pub local_mutable: Vec<bool>,
    pub local_regions: Vec<RegionId>,
    pub local_first_write_line: Vec<i32>,
    pub local_last_use_line: Vec<i32>,
    pub block_successors: FxHashMap<BlockId, Vec<BlockId>>,
    pub block_local_max_use_line: FxHashMap<BlockId, Vec<i32>>,
    pub may_use_from_block: FxHashMap<BlockId, RootSet>,
    pub must_use_from_block: FxHashMap<BlockId, RootSet>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum FutureUseKind {
    None,
    May,
    Must,
}

impl FunctionLayout {
    pub(super) fn new(
        local_ids: Vec<LocalId>,
        local_mutable: Vec<bool>,
        local_regions: Vec<RegionId>,
        local_first_write_line: Vec<i32>,
        local_last_use_line: Vec<i32>,
        block_successors: FxHashMap<BlockId, Vec<BlockId>>,
        block_local_max_use_line: FxHashMap<BlockId, Vec<i32>>,
        may_use_from_block: FxHashMap<BlockId, RootSet>,
        must_use_from_block: FxHashMap<BlockId, RootSet>,
    ) -> Self {
        let mut local_index_by_id =
            FxHashMap::with_capacity_and_hasher(local_ids.len(), Default::default());

        for (index, local_id) in local_ids.iter().enumerate() {
            local_index_by_id.insert(*local_id, index);
        }

        Self {
            local_ids,
            local_index_by_id,
            local_mutable,
            local_regions,
            local_first_write_line,
            local_last_use_line,
            block_successors,
            block_local_max_use_line,
            may_use_from_block,
            must_use_from_block,
        }
    }

    pub(super) fn local_count(&self) -> usize {
        self.local_ids.len()
    }

    pub(super) fn index_of(&self, local_id: LocalId) -> Option<usize> {
        self.local_index_by_id.get(&local_id).copied()
    }

    pub(super) fn local_is_expired(&self, local_index: usize, current_line: i32) -> bool {
        let last_use = self.local_last_use_line[local_index];
        last_use >= 0 && last_use < current_line
    }

    pub(super) fn future_use_kind(
        &self,
        block_id: BlockId,
        local_index: usize,
        current_line: i32,
    ) -> FutureUseKind {
        if self.local_has_future_use_in_block(block_id, local_index, current_line) {
            return FutureUseKind::Must;
        }

        let Some(successors) = self.block_successors.get(&block_id) else {
            return FutureUseKind::None;
        };
        if successors.is_empty() {
            return FutureUseKind::None;
        }

        let mut may = false;
        let mut must = true;

        for successor in successors {
            let successor_may = self
                .may_use_from_block
                .get(successor)
                .map(|roots| roots.contains(local_index))
                .unwrap_or(false);
            let successor_must = self
                .must_use_from_block
                .get(successor)
                .map(|roots| roots.contains(local_index))
                .unwrap_or(false);

            may |= successor_may;
            must &= successor_must;
        }

        if !may {
            FutureUseKind::None
        } else if must {
            FutureUseKind::Must
        } else {
            FutureUseKind::May
        }
    }

    fn local_has_future_use_in_block(
        &self,
        block_id: BlockId,
        local_index: usize,
        current_line: i32,
    ) -> bool {
        self.block_local_max_use_line
            .get(&block_id)
            .and_then(|max_use_lines| max_use_lines.get(local_index))
            .map(|line| *line > current_line)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BorrowState {
    // One lattice state per function-local index.
    locals: Vec<LocalState>,
    // Cached count of locals whose effective roots include each root index.
    // This keeps mutable conflict checks O(1) per root.
    root_ref_counts: Vec<u32>,
}

impl BorrowState {
    pub(super) fn new_uninitialized(local_count: usize) -> Self {
        let locals = (0..local_count)
            .map(|_| LocalState::uninit(local_count))
            .collect::<Vec<_>>();

        Self {
            locals,
            root_ref_counts: vec![0; local_count],
        }
    }

    pub(super) fn initialize_parameter(&mut self, local_index: usize) {
        let local_count = self.locals.len();
        self.update_local_state(local_index, LocalState::slot(local_count));
    }

    pub(super) fn local_state(&self, local_index: usize) -> &LocalState {
        &self.locals[local_index]
    }

    pub(super) fn alias_count_for_root(&self, root_index: usize) -> u32 {
        self.root_ref_counts[root_index]
    }

    pub(super) fn has_any_alias_conflict(&self) -> bool {
        self.root_ref_counts.iter().any(|count| *count > 1)
    }

    pub(super) fn effective_roots(&self, local_index: usize) -> RootSet {
        self.effective_roots_from_state(local_index, &self.locals[local_index])
    }

    pub(super) fn update_local_state(&mut self, local_index: usize, new_state: LocalState) {
        let old_roots = self.effective_roots(local_index);
        for root_index in old_roots.iter_ones() {
            if self.root_ref_counts[root_index] > 0 {
                self.root_ref_counts[root_index] -= 1;
            }
        }

        self.locals[local_index] = new_state;

        let new_roots = self.effective_roots(local_index);
        for root_index in new_roots.iter_ones() {
            self.root_ref_counts[root_index] += 1;
        }
    }

    pub(super) fn join(&self, other: &Self) -> Self {
        let local_count = self.locals.len();
        let mut joined_locals = Vec::with_capacity(local_count);

        for index in 0..local_count {
            let left = &self.locals[index];
            let right = &other.locals[index];

            let mut alias_roots = left.alias_roots.clone();
            alias_roots.union_with(&right.alias_roots);
            let mut direct_alias_roots = left.direct_alias_roots.clone();
            direct_alias_roots.union_with(&right.direct_alias_roots);

            joined_locals.push(LocalState {
                mode: left.mode.union(right.mode),
                alias_roots,
                direct_alias_roots,
            });
        }

        let mut joined = Self {
            locals: joined_locals,
            root_ref_counts: vec![0; local_count],
        };
        joined.recompute_root_ref_counts();
        joined
    }

    pub(super) fn kill_invisible(&mut self, visible_mask: &RootSet) {
        let local_count = self.locals.len();
        let mut changed = false;

        for local_index in 0..local_count {
            if !visible_mask.contains(local_index) {
                let replacement = LocalState::uninit(local_count);
                if self.locals[local_index] != replacement {
                    self.locals[local_index] = replacement;
                    changed = true;
                }
                continue;
            }

            let mut next = self.locals[local_index].clone();
            if next.mode.contains(LocalMode::ALIAS) {
                next.alias_roots.intersect_with(visible_mask);
                next.direct_alias_roots.intersect_with(visible_mask);
                if next.alias_roots.is_empty() {
                    next = if next.mode.contains(LocalMode::SLOT) {
                        LocalState {
                            mode: LocalMode::SLOT,
                            alias_roots: RootSet::empty(local_count),
                            direct_alias_roots: RootSet::empty(local_count),
                        }
                    } else {
                        LocalState::uninit(local_count)
                    };
                }
            }

            if next != self.locals[local_index] {
                self.locals[local_index] = next;
                changed = true;
            }
        }

        if changed {
            self.recompute_root_ref_counts();
        }
    }

    pub(super) fn to_snapshot(&self, local_ids: &[LocalId]) -> BorrowStateSnapshot {
        let mut locals = Vec::with_capacity(self.locals.len());

        for (index, local_state) in self.locals.iter().enumerate() {
            let alias_roots = local_state
                .alias_roots
                .iter_ones()
                .map(|root_index| local_ids[root_index])
                .collect::<Vec<_>>();

            locals.push(LocalBorrowSnapshot {
                local: local_ids[index],
                mode: local_state.mode,
                alias_roots,
            });
        }

        BorrowStateSnapshot { locals }
    }

    pub(super) fn invalidate_root(&mut self, root_index: usize) {
        let local_count = self.locals.len();

        for local_index in 0..local_count {
            if local_index == root_index {
                self.locals[local_index] = LocalState::uninit(local_count);
                continue;
            }

            let mut next = self.locals[local_index].clone();
            if next.mode.contains(LocalMode::ALIAS) && next.alias_roots.contains(root_index) {
                next.alias_roots.remove(root_index);
                next.direct_alias_roots.remove(root_index);
                if next.alias_roots.is_empty() {
                    next = if next.mode.contains(LocalMode::SLOT) {
                        LocalState::slot(local_count)
                    } else {
                        LocalState::uninit(local_count)
                    };
                }
            }

            self.locals[local_index] = next;
        }

        self.recompute_root_ref_counts();
    }

    fn effective_roots_from_state(&self, local_index: usize, state: &LocalState) -> RootSet {
        let local_count = self.locals.len();
        let mut roots = RootSet::empty(local_count);

        if state.mode.contains(LocalMode::SLOT) {
            roots.insert(local_index);
        }

        if state.mode.contains(LocalMode::ALIAS) {
            roots.union_with(&state.alias_roots);
        }

        roots
    }

    fn recompute_root_ref_counts(&mut self) {
        self.root_ref_counts.fill(0);

        for local_index in 0..self.locals.len() {
            let roots = self.effective_roots(local_index);
            for root_index in roots.iter_ones() {
                self.root_ref_counts[root_index] += 1;
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct LocalState {
    pub mode: LocalMode,
    pub alias_roots: RootSet,
    pub direct_alias_roots: RootSet,
}

impl LocalState {
    pub(super) fn uninit(local_count: usize) -> Self {
        Self {
            mode: LocalMode::UNINIT,
            alias_roots: RootSet::empty(local_count),
            direct_alias_roots: RootSet::empty(local_count),
        }
    }

    pub(super) fn slot(local_count: usize) -> Self {
        Self {
            mode: LocalMode::SLOT,
            alias_roots: RootSet::empty(local_count),
            direct_alias_roots: RootSet::empty(local_count),
        }
    }

    pub(super) fn alias(alias_roots: RootSet) -> Self {
        let local_count = alias_roots.bit_len;
        Self::alias_with_direct(alias_roots, RootSet::empty(local_count))
    }

    pub(super) fn alias_with_direct(alias_roots: RootSet, direct_alias_roots: RootSet) -> Self {
        Self {
            mode: LocalMode::ALIAS,
            alias_roots,
            direct_alias_roots,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct RootSet {
    words: Vec<u64>,
    bit_len: usize,
}

impl RootSet {
    pub(super) fn empty(bit_len: usize) -> Self {
        let word_len = bit_len.div_ceil(64);
        Self {
            words: vec![0; word_len],
            bit_len,
        }
    }

    pub(super) fn full(bit_len: usize) -> Self {
        let word_len = bit_len.div_ceil(64);
        let mut words = vec![u64::MAX; word_len];
        if bit_len % 64 != 0 {
            let remainder = bit_len % 64;
            let mask = (1u64 << remainder) - 1;
            if let Some(last) = words.last_mut() {
                *last = mask;
            }
        }
        Self { words, bit_len }
    }

    pub(super) fn insert(&mut self, bit_index: usize) {
        if bit_index >= self.bit_len {
            return;
        }

        let word_index = bit_index / 64;
        let bit_offset = bit_index % 64;
        self.words[word_index] |= 1u64 << bit_offset;
    }

    pub(super) fn contains(&self, bit_index: usize) -> bool {
        if bit_index >= self.bit_len {
            return false;
        }

        let word_index = bit_index / 64;
        let bit_offset = bit_index % 64;
        (self.words[word_index] & (1u64 << bit_offset)) != 0
    }

    pub(super) fn remove(&mut self, bit_index: usize) {
        if bit_index >= self.bit_len {
            return;
        }

        let word_index = bit_index / 64;
        let bit_offset = bit_index % 64;
        self.words[word_index] &= !(1u64 << bit_offset);
    }

    pub(super) fn union_with(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(other.words.iter()) {
            *left |= *right;
        }
    }

    pub(super) fn intersect_with(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(other.words.iter()) {
            *left &= *right;
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.words.iter().all(|word| *word == 0)
    }

    pub(super) fn iter_ones(&self) -> RootSetIter<'_> {
        RootSetIter {
            set: self,
            word_index: 0,
            current_word: if self.words.is_empty() {
                0
            } else {
                self.words[0]
            },
        }
    }
}

pub(super) struct RootSetIter<'a> {
    set: &'a RootSet,
    word_index: usize,
    current_word: u64,
}

impl<'a> Iterator for RootSetIter<'a> {
    type Item = usize;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if self.word_index >= self.set.words.len() {
                return None;
            }

            if self.current_word != 0 {
                let trailing = self.current_word.trailing_zeros() as usize;
                let bit_index = self.word_index * 64 + trailing;
                self.current_word &= self.current_word - 1;

                if bit_index < self.set.bit_len {
                    return Some(bit_index);
                }

                continue;
            }

            self.word_index += 1;
            if self.word_index < self.set.words.len() {
                self.current_word = self.set.words[self.word_index];
            }
        }
    }
}
