use crate::compiler_frontend::analysis::borrow_checker::types::{
    BorrowStateSnapshot, LocalBorrowSnapshot, LocalMode,
};
use crate::compiler_frontend::hir::hir_nodes::LocalId;
use rustc_hash::FxHashMap;

#[derive(Debug, Clone)]
pub(super) struct FunctionLayout {
    pub local_ids: Vec<LocalId>,
    pub local_index_by_id: FxHashMap<LocalId, usize>,
    pub local_mutable: Vec<bool>,
}

impl FunctionLayout {
    pub(super) fn new(local_ids: Vec<LocalId>, local_mutable: Vec<bool>) -> Self {
        let mut local_index_by_id =
            FxHashMap::with_capacity_and_hasher(local_ids.len(), Default::default());

        for (index, local_id) in local_ids.iter().enumerate() {
            local_index_by_id.insert(*local_id, index);
        }

        Self {
            local_ids,
            local_index_by_id,
            local_mutable,
        }
    }

    pub(super) fn local_count(&self) -> usize {
        self.local_ids.len()
    }

    pub(super) fn index_of(&self, local_id: LocalId) -> Option<usize> {
        self.local_index_by_id.get(&local_id).copied()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct BorrowState {
    locals: Vec<LocalState>,
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

    pub(super) fn local_state_mut(&mut self, local_index: usize) -> &mut LocalState {
        &mut self.locals[local_index]
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

            joined_locals.push(LocalState {
                mode: left.mode.union(right.mode),
                alias_roots,
            });
        }

        let mut joined = Self {
            locals: joined_locals,
            root_ref_counts: vec![0; local_count],
        };
        joined.recompute_root_ref_counts();
        joined
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
}

impl LocalState {
    pub(super) fn uninit(local_count: usize) -> Self {
        Self {
            mode: LocalMode::UNINIT,
            alias_roots: RootSet::empty(local_count),
        }
    }

    pub(super) fn slot(local_count: usize) -> Self {
        Self {
            mode: LocalMode::SLOT,
            alias_roots: RootSet::empty(local_count),
        }
    }

    pub(super) fn alias(alias_roots: RootSet) -> Self {
        Self {
            mode: LocalMode::ALIAS,
            alias_roots,
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

    pub(super) fn singleton(bit_len: usize, bit_index: usize) -> Self {
        let mut set = Self::empty(bit_len);
        set.insert(bit_index);
        set
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

    pub(super) fn union_with(&mut self, other: &Self) {
        for (left, right) in self.words.iter_mut().zip(other.words.iter()) {
            *left |= *right;
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
