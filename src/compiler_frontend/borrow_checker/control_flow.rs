//! # Control Flow Graph
//!
//! Represents the control flow structure of the program for dataflow analysis.
//! Provides utilities for traversal, merge point identification, and path analysis.

use crate::compiler_frontend::hir::nodes::{HirKind, HirModule, HirTerminator};
use std::collections::{HashMap, HashSet, VecDeque};

/// Unique identifier for a basic block
pub type BlockId = usize;

/// A point in the program execution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgramPoint {
    pub block: BlockId,
    pub statement: usize,
}

/// A basic block in the control flow graph
#[derive(Debug, Clone)]
pub struct BasicBlock {
    pub id: BlockId,
    pub statements: Vec<ProgramPoint>,
    pub terminator: ProgramPoint,
}

/// Control flow graph representation
pub struct ControlFlowGraph {
    /// All basic blocks
    blocks: HashMap<BlockId, BasicBlock>,

    /// Predecessor relationships: Block -> Predecessors
    predecessors: HashMap<BlockId, Vec<BlockId>>,

    /// Successor relationships: Block -> Successors
    successors: HashMap<BlockId, Vec<BlockId>>,

    /// Entry block of the function/module
    entry_block: Option<BlockId>,

    /// Exit blocks (blocks that end with return/panic)
    exit_blocks: HashSet<BlockId>,
}

impl ControlFlowGraph {
    /// Create a new empty control flow graph
    pub fn new() -> Self {
        Self {
            blocks: HashMap::new(),
            predecessors: HashMap::new(),
            successors: HashMap::new(),
            entry_block: None,
            exit_blocks: HashSet::new(),
        }
    }

    /// Add a basic block to the graph
    pub fn add_block(&mut self, block: BasicBlock) {
        let id = block.id;
        self.blocks.insert(id, block);
        self.predecessors.entry(id).or_default();
        self.successors.entry(id).or_default();
    }

    /// Add an edge between two blocks
    pub fn add_edge(&mut self, from: BlockId, to: BlockId) {
        self.successors.entry(from).or_default().push(to);
        self.predecessors.entry(to).or_default().push(from);
    }

    /// Set the entry block
    pub fn set_entry_block(&mut self, block: BlockId) {
        self.entry_block = Some(block);
    }

    /// Mark a block as an exit block
    pub fn add_exit_block(&mut self, block: BlockId) {
        self.exit_blocks.insert(block);
    }

    /// Build a control flow graph from a HIR module
    pub fn from_hir_module(hir_module: &HirModule) -> Self {
        let mut cfg = ControlFlowGraph::new();

        for block in &hir_module.blocks {
            let statements: Vec<ProgramPoint> = (0..block.nodes.len())
                .map(|statement| ProgramPoint {
                    block: block.id,
                    statement,
                })
                .collect();

            let terminator = block
                .nodes
                .iter()
                .enumerate()
                .rev()
                .find_map(|(statement, node)| {
                    if matches!(node.kind, HirKind::Terminator(_)) {
                        Some(ProgramPoint {
                            block: block.id,
                            statement,
                        })
                    } else {
                        None
                    }
                })
                .unwrap_or_else(|| ProgramPoint {
                    block: block.id,
                    statement: 0,
                });

            cfg.add_block(BasicBlock {
                id: block.id,
                statements,
                terminator,
            });
        }

        cfg.set_entry_block(hir_module.entry_block);

        for block in &hir_module.blocks {
            let terminator_node = block.nodes.iter().rev().find_map(|node| match &node.kind {
                HirKind::Terminator(term) => Some(term),
                _ => None,
            });

            if let Some(terminator) = terminator_node {
                match terminator {
                    HirTerminator::If {
                        then_block,
                        else_block,
                        ..
                    } => {
                        cfg.add_edge(block.id, *then_block);
                        if let Some(else_block) = else_block {
                            cfg.add_edge(block.id, *else_block);
                        }
                    }
                    HirTerminator::Match {
                        arms,
                        default_block,
                        ..
                    } => {
                        for arm in arms {
                            cfg.add_edge(block.id, arm.body);
                        }

                        if let Some(default_block) = default_block {
                            cfg.add_edge(block.id, *default_block);
                        }
                    }
                    HirTerminator::Loop { body, .. } => {
                        cfg.add_edge(block.id, *body);
                    }
                    HirTerminator::Break { target } | HirTerminator::Continue { target } => {
                        cfg.add_edge(block.id, *target);
                    }
                    HirTerminator::Return(_)
                    | HirTerminator::ReturnError(_)
                    | HirTerminator::Panic { .. } => {
                        cfg.add_exit_block(block.id);
                    }
                }
            }
        }

        cfg
    }

    /// Get a block by ID
    pub fn get_block(&self, id: BlockId) -> Option<&BasicBlock> {
        self.blocks.get(&id)
    }

    /// Get predecessors of a block
    pub fn get_predecessors(&self, block: BlockId) -> &[BlockId] {
        self.predecessors
            .get(&block)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get successors of a block
    pub fn get_successors(&self, block: BlockId) -> &[BlockId] {
        self.successors
            .get(&block)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get the entry block
    pub fn get_entry_block(&self) -> Option<BlockId> {
        self.entry_block
    }

    /// Get all exit blocks
    pub fn get_exit_blocks(&self) -> &HashSet<BlockId> {
        &self.exit_blocks
    }

    /// Traverse the graph in post-order (children before parents)
    /// This is useful for backward dataflow analysis
    pub fn traverse_postorder<F>(&self, mut visitor: F)
    where
        F: FnMut(BlockId),
    {
        let mut visited = HashSet::new();

        if let Some(entry) = self.entry_block {
            self.postorder_dfs(entry, &mut visited, &mut visitor);
        }
    }

    /// Traverse the graph in reverse post-order (parents before children)
    /// This is useful for forward dataflow analysis
    pub fn traverse_reverse_postorder<F>(&self, mut visitor: F)
    where
        F: FnMut(BlockId),
    {
        let mut post_order = Vec::new();
        self.traverse_postorder(|block| post_order.push(block));

        // Reverse the post-order to get reverse post-order
        for block in post_order.into_iter().rev() {
            visitor(block);
        }
    }

    /// Perform breadth-first traversal from entry block
    pub fn traverse_breadth_first<F>(&self, mut visitor: F)
    where
        F: FnMut(BlockId),
    {
        if let Some(entry) = self.entry_block {
            let mut visited = HashSet::new();
            let mut queue = VecDeque::new();

            queue.push_back(entry);
            visited.insert(entry);

            while let Some(block) = queue.pop_front() {
                visitor(block);

                for &successor in self.get_successors(block) {
                    if !visited.contains(&successor) {
                        visited.insert(successor);
                        queue.push_back(successor);
                    }
                }
            }
        }
    }

    /// Find all merge points (blocks with multiple predecessors)
    pub fn find_merge_points(&self) -> Vec<BlockId> {
        self.predecessors
            .iter()
            .filter_map(
                |(&block, preds)| {
                    if preds.len() > 1 { Some(block) } else { None }
                },
            )
            .collect()
    }

    /// Find all branch points (blocks with multiple successors)
    pub fn find_branch_points(&self) -> Vec<BlockId> {
        self.successors
            .iter()
            .filter_map(
                |(&block, succs)| {
                    if succs.len() > 1 { Some(block) } else { None }
                },
            )
            .collect()
    }

    /// Check if there's a path from one block to another
    pub fn has_path(&self, from: BlockId, to: BlockId) -> bool {
        if from == to {
            return true;
        }

        let mut visited = HashSet::new();
        let mut stack = vec![from];

        while let Some(current) = stack.pop() {
            if visited.contains(&current) {
                continue;
            }
            visited.insert(current);

            for &successor in self.get_successors(current) {
                if successor == to {
                    return true;
                }
                if !visited.contains(&successor) {
                    stack.push(successor);
                }
            }
        }

        false
    }

    /// Find all blocks reachable from a given block
    pub fn reachable_from(&self, start: BlockId) -> HashSet<BlockId> {
        let mut reachable = HashSet::new();
        let mut stack = vec![start];

        while let Some(current) = stack.pop() {
            if reachable.contains(&current) {
                continue;
            }
            reachable.insert(current);

            for &successor in self.get_successors(current) {
                if !reachable.contains(&successor) {
                    stack.push(successor);
                }
            }
        }

        reachable
    }

    /// Get all blocks in the graph
    pub fn all_blocks(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.blocks.keys().copied()
    }

    /// Get the number of blocks in the graph
    pub fn block_count(&self) -> usize {
        self.blocks.len()
    }

    /// Check if the graph is empty
    pub fn is_empty(&self) -> bool {
        self.blocks.is_empty()
    }

    /// Helper for post-order DFS traversal
    fn postorder_dfs<F>(&self, block: BlockId, visited: &mut HashSet<BlockId>, visitor: &mut F)
    where
        F: FnMut(BlockId),
    {
        if visited.contains(&block) {
            return;
        }

        visited.insert(block);

        // Visit all successors first
        for &successor in self.get_successors(block) {
            self.postorder_dfs(successor, visited, visitor);
        }

        // Then visit this block
        visitor(block);
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}
