//! # Dataflow Analysis Engine
//!
//! Performs forward and backward dataflow analysis to compute borrow states and last-use information.
//! Provides the foundation for borrow checking and ownership analysis.

use super::{
    borrow_state::{AccessKind, BorrowConflict, BorrowState, ProgramPoint},
    control_flow::{BlockId, ControlFlowGraph},
    place_registry::{PlaceId, PlaceRegistry},
};
use crate::compiler_frontend::hir::nodes::{
    HirExpr, HirExprKind, HirKind, HirModule, HirNode, HirStmt, HirTerminator,
};
use crate::compiler_frontend::parsers::tokenizer::tokens::TextLocation;
use std::collections::{HashMap, HashSet, VecDeque};

#[derive(Default)]
pub struct AnalysisResults {
    pub states: HashMap<ProgramPoint, BorrowState>,
    pub last_uses: HashMap<PlaceId, ProgramPoint>,
    pub conflicts: Vec<BorrowConflict>,
}

#[derive(Debug, Clone)]
struct Access {
    place: PlaceId,
    kind: AccessKind,
    location: TextLocation,
}

/// Dataflow analysis engine
pub struct DataflowEngine {
    cfg: ControlFlowGraph,
    place_registry: PlaceRegistry,
}

impl DataflowEngine {
    /// Create a new dataflow engine
    pub fn new(cfg: ControlFlowGraph, place_registry: PlaceRegistry) -> Self {
        Self {
            cfg,
            place_registry,
        }
    }

    /// Compute borrow states, conflicts and last-use hints for a module.
    pub fn analyze(&mut self, hir_module: &HirModule) -> AnalysisResults {
        let mut results = AnalysisResults::default();
        let mut block_out_states: HashMap<BlockId, BorrowState> = HashMap::new();
        let mut worklist = VecDeque::new();

        results.last_uses = self.compute_last_uses(hir_module);
        let block_map: HashMap<BlockId, &Vec<HirNode>> = hir_module
            .blocks
            .iter()
            .map(|block| (block.id, &block.nodes))
            .collect();

        if let Some(entry) = self.cfg.get_entry_block() {
            worklist.push_back(entry);
        }

        while let Some(block_id) = worklist.pop_front() {
            let mut in_state = self.merge_predecessor_states(block_id, &block_out_states);
            let Some(nodes) = block_map.get(&block_id) else {
                continue;
            };

            results.states.retain(|point, _| point.block != block_id);

            for (statement_idx, node) in nodes.iter().enumerate() {
                let point = ProgramPoint {
                    block: block_id,
                    statement: statement_idx,
                };

                results
                    .conflicts
                    .extend(self.apply_node(&mut in_state, node));
                results.states.insert(point, in_state.clone());

                for (place, last_use_point) in results.last_uses.iter() {
                    if *last_use_point == point {
                        in_state.end_borrows_for_place(*place);
                    }
                }
            }

            let out_state = in_state.clone();
            let previous_out = block_out_states.get(&block_id);
            let changed = previous_out
                .map(|existing| existing != &out_state)
                .unwrap_or(true);

            block_out_states.insert(block_id, out_state);

            if changed {
                for successor in self.cfg.get_successors(block_id) {
                    worklist.push_back(*successor);
                }
            }
        }

        results
    }

    fn merge_predecessor_states(
        &self,
        block_id: BlockId,
        out_states: &HashMap<BlockId, BorrowState>,
    ) -> BorrowState {
        let mut merged_states = Vec::new();

        for predecessor in self.cfg.get_predecessors(block_id) {
            if let Some(state) = out_states.get(predecessor) {
                merged_states.push(state.clone());
            }
        }

        let mut merged_iter = merged_states.into_iter();
        let Some(mut merged) = merged_iter.next() else {
            return BorrowState::new();
        };

        for state in merged_iter {
            merged = merged.merge(&state);
        }

        merged
    }

    fn apply_node(&mut self, state: &mut BorrowState, node: &HirNode) -> Vec<BorrowConflict> {
        let mut conflicts = Vec::new();
        let accesses = self.collect_accesses(node);

        for access in accesses {
            if let Err(conflict) =
                state.record_access(access.place, access.kind, access.location.clone())
            {
                conflicts.push(conflict);
            }
        }

        conflicts
    }

    fn compute_last_uses(&mut self, hir_module: &HirModule) -> HashMap<PlaceId, ProgramPoint> {
        let mut last_uses = HashMap::new();
        let mut live_in: HashMap<BlockId, HashSet<PlaceId>> = HashMap::new();
        let mut live_out: HashMap<BlockId, HashSet<PlaceId>> = HashMap::new();
        let block_map: HashMap<BlockId, &Vec<HirNode>> = hir_module
            .blocks
            .iter()
            .map(|block| (block.id, &block.nodes))
            .collect();

        let mut changed = true;
        while changed {
            changed = false;

            for block in &hir_module.blocks {
                let mut successors_live = HashSet::new();
                for successor in self.cfg.get_successors(block.id) {
                    if let Some(set) = live_in.get(successor) {
                        successors_live.extend(set.iter().copied());
                    }
                }

                let mut live: HashSet<PlaceId> = successors_live.clone();
                let Some(nodes) = block_map.get(&block.id) else {
                    continue;
                };

                for (statement_idx, node) in nodes.iter().enumerate().rev() {
                    for access in self.collect_accesses(node) {
                        if !live.contains(&access.place) {
                            last_uses.entry(access.place).or_insert(ProgramPoint {
                                block: block.id,
                                statement: statement_idx,
                            });
                        }
                        live.insert(access.place);
                    }
                }

                let live_in_entry = live_in.entry(block.id).or_default();
                if live_in_entry != &live {
                    *live_in_entry = live.clone();
                    changed = true;
                }

                let live_out_entry = live_out.entry(block.id).or_default();
                if live_out_entry != &successors_live {
                    *live_out_entry = successors_live;
                    changed = true;
                }
            }
        }

        last_uses
    }

    fn collect_accesses(&mut self, node: &HirNode) -> Vec<Access> {
        match &node.kind {
            HirKind::Stmt(stmt) => self.collect_stmt_accesses(stmt),
            HirKind::Terminator(term) => self.collect_terminator_accesses(term),
        }
    }

    fn collect_stmt_accesses(&mut self, stmt: &HirStmt) -> Vec<Access> {
        match stmt {
            HirStmt::Assign {
                target,
                value,
                is_mutable,
            } => {
                let mut accesses = self.collect_expr_accesses(value);
                let place = self.place_registry.hir_place_to_place(target);
                let kind = if *is_mutable {
                    AccessKind::Write
                } else {
                    AccessKind::Read
                };

                accesses.push(Access {
                    place,
                    kind,
                    location: value.location.clone(),
                });

                accesses
            }
            HirStmt::Call { args, .. } => args
                .iter()
                .flat_map(|expr| self.collect_expr_accesses(expr))
                .collect(),
            HirStmt::PossibleDrop(place) => {
                let place = self.place_registry.hir_place_to_place(place);
                vec![Access {
                    place,
                    kind: AccessKind::Move,
                    location: place_to_location(place, &self.place_registry),
                }]
            }
            HirStmt::RuntimeTemplateCall { captures, .. } => captures
                .iter()
                .flat_map(|expr| self.collect_expr_accesses(expr))
                .collect(),
            HirStmt::TemplateFn { .. }
            | HirStmt::FunctionDef { .. }
            | HirStmt::StructDef { .. } => Vec::new(),
            HirStmt::ExprStmt(expr) => self.collect_expr_accesses(expr),
        }
    }

    fn collect_terminator_accesses(&mut self, term: &HirTerminator) -> Vec<Access> {
        match term {
            HirTerminator::If { condition, .. } => self.collect_expr_accesses(condition),
            HirTerminator::Match {
                scrutinee, arms, ..
            } => {
                let mut accesses = self.collect_expr_accesses(scrutinee);
                for arm in arms {
                    if let Some(guard) = &arm.guard {
                        accesses.extend(self.collect_expr_accesses(guard));
                    }
                }

                accesses
            }
            HirTerminator::Loop { iterator, .. } => iterator
                .iter()
                .flat_map(|expr| self.collect_expr_accesses(expr))
                .collect(),
            HirTerminator::Break { .. }
            | HirTerminator::Continue { .. }
            | HirTerminator::Panic { .. } => Vec::new(),
            HirTerminator::Return(values) => values
                .iter()
                .flat_map(|expr| self.collect_expr_accesses(expr))
                .collect(),
            HirTerminator::ReturnError(expr) => self.collect_expr_accesses(expr),
        }
    }

    fn collect_expr_accesses(&mut self, expr: &HirExpr) -> Vec<Access> {
        let mut accesses = Vec::new();

        match &expr.kind {
            HirExprKind::Load(place) => {
                let place = self.place_registry.hir_place_to_place(place);
                accesses.push(Access {
                    place,
                    kind: AccessKind::Read,
                    location: expr.location.clone(),
                });
            }
            HirExprKind::Move(place) => {
                let place = self.place_registry.hir_place_to_place(place);
                accesses.push(Access {
                    place,
                    kind: AccessKind::Move,
                    location: expr.location.clone(),
                });
            }
            HirExprKind::Field { base, field } => {
                let base_place = self
                    .place_registry
                    .register_place(super::place_registry::Place::Variable(*base));
                let place =
                    self.place_registry
                        .register_place(super::place_registry::Place::Field {
                            base: base_place,
                            field: *field,
                        });
                accesses.push(Access {
                    place,
                    kind: AccessKind::Read,
                    location: expr.location.clone(),
                });
            }
            HirExprKind::BinOp { left, right, .. } => {
                accesses.extend(self.collect_expr_accesses(left));
                accesses.extend(self.collect_expr_accesses(right));
            }
            HirExprKind::UnaryOp { operand, .. } => {
                accesses.extend(self.collect_expr_accesses(operand));
            }
            HirExprKind::Call { args, .. } => {
                for arg in args {
                    accesses.extend(self.collect_expr_accesses(arg));
                }
            }
            HirExprKind::MethodCall { receiver, args, .. } => {
                accesses.extend(self.collect_expr_accesses(receiver));
                for arg in args {
                    accesses.extend(self.collect_expr_accesses(arg));
                }
            }
            HirExprKind::StructConstruct { fields, .. } => {
                for (_, value) in fields {
                    accesses.extend(self.collect_expr_accesses(value));
                }
            }
            HirExprKind::Collection(items) => {
                for item in items {
                    accesses.extend(self.collect_expr_accesses(item));
                }
            }
            HirExprKind::Range { start, end } => {
                accesses.extend(self.collect_expr_accesses(start));
                accesses.extend(self.collect_expr_accesses(end));
            }
            HirExprKind::StringLiteral(_)
            | HirExprKind::HeapString(_)
            | HirExprKind::Bool(_)
            | HirExprKind::Char(_)
            | HirExprKind::Float(_)
            | HirExprKind::Int(_) => {}
        }

        accesses
    }

    pub fn place_registry(&self) -> &PlaceRegistry {
        &self.place_registry
    }
}

fn place_to_location(place: PlaceId, registry: &PlaceRegistry) -> TextLocation {
    let Some(place) = registry.get_place(place) else {
        return TextLocation::default();
    };

    match place {
        super::place_registry::Place::Variable(_) => TextLocation::default(),
        super::place_registry::Place::Field { .. } => TextLocation::default(),
        super::place_registry::Place::Index { .. } => TextLocation::default(),
        super::place_registry::Place::Unknown => TextLocation::default(),
    }
}
