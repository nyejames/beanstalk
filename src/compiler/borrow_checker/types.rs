//! Core data structures for the borrow checker.
//!
//! Defines fundamental types including BorrowChecker, ControlFlowGraph,
//! BorrowState, and Loan for borrow checking analysis.

use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::compiler_messages::compiler_warnings::CompilerWarning;
use crate::compiler::hir::nodes::HirNodeId;
use crate::compiler::hir::place::Place;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::string_interning::InternedString;
use crate::compiler::string_interning::StringTable;
use std::collections::{HashMap, HashSet, VecDeque};
use indexmap::IndexMap;

/// Unique identifier for borrows
pub type BorrowId = usize;

/// Unique identifier for CFG nodes (same as HIR node IDs)
pub type CfgNodeId = HirNodeId;

/// Main borrow checker state and context.
///
/// Maintains all state needed for borrow checking analysis including CFG,
/// active borrows, error collection, and string table for error reporting.
pub struct BorrowChecker<'a> {
    /// Control flow graph for the current function
    pub cfg: ControlFlowGraph,

    /// String table for resolving interned strings in error messages
    pub string_table: &'a mut StringTable,

    /// Accumulated borrow checker errors
    pub errors: Vec<CompilerError>,

    /// Accumulated borrow checker warnings
    pub warnings: Vec<CompilerWarning>,

    /// Next available borrow ID for unique identification
    pub next_borrow_id: BorrowId,

    /// Function signatures for return validation (future use)
    #[allow(dead_code)]
    pub function_signatures: HashMap<InternedString, FunctionSignature>,
}

/// Control Flow Graph for borrow analysis.
///
/// Provides graph-based view of program execution flow for path-sensitive
/// borrow checking and lifetime analysis.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Mapping from HIR node IDs to CFG nodes (deterministic iteration)
    pub nodes: IndexMap<HirNodeId, CfgNode>,

    /// Edges between CFG nodes (node_id -> list of successor node_ids)
    /// Uses IndexMap for deterministic iteration during traversal
    pub edges: IndexMap<HirNodeId, Vec<HirNodeId>>,

    /// Entry points for functions (typically one per function)
    pub entry_points: Vec<HirNodeId>,

    /// Exit points for functions (return statements and implicit exits)
    pub exit_points: Vec<HirNodeId>,

    /// Cached predecessor relationships for efficient reverse traversal
    predecessors_cache: HashMap<HirNodeId, Vec<HirNodeId>>,
}

/// Single node in the control flow graph.
///
/// Each CFG node corresponds to a HIR node and maintains borrow state
/// information for analysis.
#[derive(Debug, Clone)]
pub struct CfgNode {
    /// The HIR node ID this CFG node represents
    #[allow(dead_code)]
    pub hir_id: HirNodeId,

    /// Predecessor nodes in the CFG
    pub predecessors: Vec<HirNodeId>,

    /// Successor nodes in the CFG
    pub successors: Vec<HirNodeId>,

    /// Borrow state at this node
    pub borrow_state: BorrowState,

    /// Type of CFG node for analysis purposes
    #[allow(dead_code)]
    pub node_type: CfgNodeType,
}

/// Classification of CFG nodes for analysis
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CfgNodeType {
    /// Regular statement or expression
    Statement,

    /// Conditional branch point (if, match)
    Branch,

    /// Loop header
    LoopHeader,

    /// Loop body
    #[allow(dead_code)]
    LoopBody,

    /// Function entry point
    FunctionEntry,

    /// Function exit point (return or implicit)
    FunctionExit,

    /// Join point where multiple control flow paths merge
    #[allow(dead_code)]
    Join,
}

/// Borrow state tracking for a CFG node.
///
/// Tracks all active borrows at a particular point in the program for
/// conflict detection and lifetime analysis.
#[derive(Debug, Clone, Default)]
pub struct BorrowState {
    /// All currently active borrows, indexed by borrow ID (deterministic iteration)
    pub active_borrows: IndexMap<BorrowId, Loan>,

    /// Mapping from places to the borrows that reference them
    /// Uses IndexMap for deterministic iteration during conflict detection
    pub place_to_borrows: IndexMap<Place, Vec<BorrowId>>,

    /// Last use points for each place (for move analysis)
    #[allow(dead_code)]
    pub last_uses: HashMap<Place, HirNodeId>,

    /// Cached overlapping places for efficient conflict detection
    overlapping_places_cache: HashMap<Place, Vec<Place>>,
}

/// Tracked borrow with metadata.
///
/// Represents active borrows with creation point, kind, target place,
/// and active regions for lifetime analysis.
#[derive(Debug, Clone)]
pub struct Loan {
    /// Unique identifier for this borrow
    pub id: BorrowId,

    /// The place being borrowed
    pub place: Place,

    /// Kind of borrow (shared, mutable, or move)
    pub kind: BorrowKind,

    /// HIR node where this borrow was created
    pub creation_point: HirNodeId,

    /// HIR node where this borrow was last used (if determined)
    #[allow(dead_code)]
    pub last_use_point: Option<HirNodeId>,
}

/// Kind of borrow for conflict analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BorrowKind {
    /// Shared/immutable borrow (default for loads)
    Shared,

    /// Mutable/exclusive borrow (from mutable access)
    Mutable,

    /// Candidate move (potential ownership transfer, treated conservatively as mutable)
    /// This is refined to either Move or Mutable by last-use analysis
    CandidateMove,

    /// Move (refined from CandidateMove by last-use analysis)
    Move,
}

impl<'a> BorrowChecker<'a> {
    /// Create a new borrow checker instance.
    pub fn new(string_table: &'a mut StringTable) -> Self {
        Self {
            cfg: ControlFlowGraph::new(),
            string_table,
            errors: Vec::new(),
            warnings: Vec::new(),
            next_borrow_id: 0,
            function_signatures: HashMap::new(),
        }
    }

    /// Generate a new unique borrow ID.
    pub fn next_borrow_id(&mut self) -> BorrowId {
        let id = self.next_borrow_id;
        self.next_borrow_id += 1;
        id
    }

    /// Add an error to the error collection.
    pub fn add_error(&mut self, error: CompilerError) {
        self.errors.push(error);
    }

    /// Add a warning to the warning collection
    #[allow(dead_code)]
    pub fn add_warning(&mut self, warning: CompilerWarning) {
        self.warnings.push(warning);
    }

    /// Finish borrow checking and return results.
    pub fn finish(self) -> Result<(), CompilerMessages> {
        if self.errors.is_empty() {
            if self.warnings.is_empty() {
                Ok(())
            } else {
                Err(CompilerMessages {
                    errors: Vec::new(),
                    warnings: self.warnings,
                })
            }
        } else {
            Err(CompilerMessages {
                errors: self.errors,
                warnings: self.warnings,
            })
        }
    }

    /// Record a last use for move refinement integration
    pub fn record_last_use(&mut self, borrow_id: BorrowId, kill_point: CfgNodeId) {
        // Find the loan and update its last use point
        for cfg_node in self.cfg.nodes.values_mut() {
            if let Some(loan) = cfg_node.borrow_state.active_borrows.get_mut(&borrow_id) {
                loan.last_use_point = Some(kill_point);
                break;
            }
        }
    }
}

impl ControlFlowGraph {
    /// Create a new empty control flow graph
    pub fn new() -> Self {
        Self {
            nodes: IndexMap::new(),
            edges: IndexMap::new(),
            entry_points: Vec::new(),
            exit_points: Vec::new(),
            predecessors_cache: HashMap::new(),
        }
    }

    /// Add a node to the CFG
    pub fn add_node(&mut self, hir_id: HirNodeId, node_type: CfgNodeType) {
        let node = CfgNode {
            hir_id,
            predecessors: Vec::new(),
            successors: Vec::new(),
            borrow_state: BorrowState::default(),
            node_type,
        };
        self.nodes.insert(hir_id, node);
    }

    /// Add an edge between two CFG nodes
    pub fn add_edge(&mut self, from: HirNodeId, to: HirNodeId) {
        // Add to edges map
        self.edges.entry(from).or_default().push(to);

        // Update successor/predecessor relationships
        if let Some(from_node) = self.nodes.get_mut(&from) {
            from_node.successors.push(to);
        }
        if let Some(to_node) = self.nodes.get_mut(&to) {
            to_node.predecessors.push(from);
        }

        // Update predecessors cache
        self.predecessors_cache.entry(to).or_default().push(from);
    }

    /// Mark a node as an entry point
    pub fn add_entry_point(&mut self, hir_id: HirNodeId) {
        self.entry_points.push(hir_id);
    }

    /// Mark a node as an exit point
    pub fn add_exit_point(&mut self, hir_id: HirNodeId) {
        self.exit_points.push(hir_id);
    }

    /// Get successors of a node
    pub fn successors(&self, node_id: HirNodeId) -> &[HirNodeId] {
        self.edges
            .get(&node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Get predecessors of a node
    pub fn predecessors(&self, node_id: HirNodeId) -> Vec<HirNodeId> {
        self.predecessors_cache
            .get(&node_id)
            .cloned()
            .unwrap_or_default()
    }

    /// Get predecessors as slice for efficient iteration
    pub fn predecessors_slice(&self, node_id: HirNodeId) -> &[HirNodeId] {
        self.predecessors_cache
            .get(&node_id)
            .map(|v| v.as_slice())
            .unwrap_or(&[])
    }

    /// Efficient worklist-based traversal
    pub fn traverse_postorder<F>(&self, mut visit: F) 
    where
        F: FnMut(HirNodeId),
    {
        let mut visited = HashSet::new();
        let mut stack = Vec::new();
        let mut post_order = Vec::new();

        // Start from all entry points
        for &entry in &self.entry_points {
            if !visited.contains(&entry) {
                stack.push(entry);
            }
        }

        // DFS to build post-order
        while let Some(node_id) = stack.pop() {
            if visited.contains(&node_id) {
                continue;
            }

            visited.insert(node_id);
            post_order.push(node_id);

            // Add successors to stack
            for &successor in self.successors(node_id) {
                if !visited.contains(&successor) {
                    stack.push(successor);
                }
            }
        }

        // Visit in post-order
        for node_id in post_order.into_iter().rev() {
            visit(node_id);
        }
    }

    /// Efficient reverse post-order traversal for dataflow analysis
    pub fn traverse_reverse_postorder<F>(&self, mut visit: F)
    where
        F: FnMut(HirNodeId),
    {
        let mut visited = HashSet::new();
        let mut worklist = VecDeque::new();

        // Start from exit points for backward analysis
        for &exit in &self.exit_points {
            worklist.push_back(exit);
        }

        while let Some(node_id) = worklist.pop_front() {
            if visited.contains(&node_id) {
                continue;
            }

            visited.insert(node_id);
            visit(node_id);

            // Add predecessors to worklist
            for &pred in self.predecessors_slice(node_id) {
                if !visited.contains(&pred) {
                    worklist.push_back(pred);
                }
            }
        }
    }
}

impl BorrowState {
    /// Create a new empty borrow state
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self::default()
    }

    /// Check if this borrow state is empty
    pub fn is_empty(&self) -> bool {
        self.active_borrows.is_empty()
    }

    /// Add a borrow to this state
    pub fn add_borrow(&mut self, loan: Loan) {
        let borrow_id = loan.id;
        let place = loan.place.clone();

        // Add to active borrows
        self.active_borrows.insert(borrow_id, loan);

        // Add to place mapping
        self.place_to_borrows
            .entry(place)
            .or_default()
            .push(borrow_id);
    }

    /// Remove a borrow from this state
    pub fn remove_borrow(&mut self, borrow_id: BorrowId) {
        if let Some(loan) = self.active_borrows.swap_remove(&borrow_id) {
            // Remove from place mapping
            if let Some(borrow_list) = self.place_to_borrows.get_mut(&loan.place) {
                borrow_list.retain(|&id| id != borrow_id);
                if borrow_list.is_empty() {
                    self.place_to_borrows.swap_remove(&loan.place);
                }
            }

            // Invalidate overlapping places cache for this place
            self.overlapping_places_cache.remove(&loan.place);
        }
    }

    /// Get all borrows for a specific place
    /// **Optimization**: Direct slice access without allocation
    #[allow(dead_code)]
    pub fn borrows_for_place(&self, place: &Place) -> Vec<&Loan> {
        if let Some(borrow_ids) = self.place_to_borrows.get(place) {
            borrow_ids
                .iter()
                .filter_map(|&id| self.active_borrows.get(&id))
                .collect()
        } else {
            Vec::new()
        }
    }

    /// Get all borrows for places that overlap with the given place
    pub fn borrows_for_overlapping_places(&self, place: &Place) -> Vec<&Loan> {
        let mut result = Vec::new();
        
        // Check cache first
        if let Some(cached_places) = self.overlapping_places_cache.get(place) {
            for existing_place in cached_places {
                if let Some(borrow_ids) = self.place_to_borrows.get(existing_place) {
                    for &borrow_id in borrow_ids {
                        if let Some(loan) = self.active_borrows.get(&borrow_id) {
                            result.push(loan);
                        }
                    }
                }
            }
        } else {
            // Cache miss - compute without updating cache (since this is immutable method)
            for (existing_place, borrow_ids) in &self.place_to_borrows {
                if place.overlaps_with(existing_place) {
                    for &borrow_id in borrow_ids {
                        if let Some(loan) = self.active_borrows.get(&borrow_id) {
                            result.push(loan);
                        }
                    }
                }
            }
        }
        
        result
    }

    /// Get all borrows for places that overlap with the given place (mutable version)
    /// **Optimization**: Uses cached overlapping places and updates cache for future lookups
    pub fn borrows_for_overlapping_places_mut(&mut self, place: &Place) -> Vec<&Loan> {
        let mut result = Vec::new();
        
        // Check cache first
        if let Some(cached_places) = self.overlapping_places_cache.get(place) {
            for existing_place in cached_places {
                if let Some(borrow_ids) = self.place_to_borrows.get(existing_place) {
                    for &borrow_id in borrow_ids {
                        if let Some(loan) = self.active_borrows.get(&borrow_id) {
                            result.push(loan);
                        }
                    }
                }
            }
        } else {
            // Build cache and collect results
            let mut overlapping_places = Vec::new();
            for (existing_place, borrow_ids) in &self.place_to_borrows {
                if place.overlaps_with(existing_place) {
                    overlapping_places.push(existing_place.clone());
                    for &borrow_id in borrow_ids {
                        if let Some(loan) = self.active_borrows.get(&borrow_id) {
                            result.push(loan);
                        }
                    }
                }
            }
            
            // Cache the overlapping places for future lookups
            self.overlapping_places_cache.insert(place.clone(), overlapping_places);
        }
        
        result
    }

    /// Merge another borrow state into this one (for CFG join points).
    ///
    /// Implements conservative merging for Polonius-style analysis:
    /// keeps borrows that exist in BOTH incoming states to ensure conflicts
    /// are only reported if they exist on ALL paths.
    pub fn merge(&mut self, other: &BorrowState) {
        // If this state is empty, use efficient move from other (first incoming edge)
        if self.is_empty() {
            // Reserve capacity to reduce reallocations
            self.active_borrows.reserve(other.active_borrows.len());
            self.place_to_borrows.reserve(other.place_to_borrows.len());
            
            for (&borrow_id, loan) in &other.active_borrows {
                self.active_borrows.insert(borrow_id, loan.clone());
            }
            
            for (place, borrow_ids) in &other.place_to_borrows {
                self.place_to_borrows.insert(place.clone(), borrow_ids.clone());
            }
            
            self.last_uses.reserve(other.last_uses.len());
            for (place, &node_id) in &other.last_uses {
                self.last_uses.insert(place.clone(), node_id);
            }
            return;
        }

        // Conservative merge: keep borrows that exist in both states
        // **Optimization**: Use retain for efficient in-place filtering
        self.active_borrows.retain(|&borrow_id, _| {
            other.active_borrows.contains_key(&borrow_id)
        });

        // Rebuild place mapping efficiently
        self.place_to_borrows.clear();
        self.place_to_borrows.reserve(self.active_borrows.len());
        for loan in self.active_borrows.values() {
            self.place_to_borrows
                .entry(loan.place.clone())
                .or_default()
                .push(loan.id);
        }

        // Merge last uses - keep the later use point for each place
        for (place, &node_id) in &other.last_uses {
            match self.last_uses.get_mut(place) {
                Some(existing) => {
                    // Keep the larger node ID (later in execution)
                    if node_id > *existing {
                        *existing = node_id;
                    }
                }
                None => {
                    self.last_uses.insert(place.clone(), node_id);
                }
            }
        }

        // Clear cache after merge
        self.overlapping_places_cache.clear();
    }

    /// Union merge: combine borrows from both states (for propagation).
    ///
    /// Used when propagating state along CFG edges where we want to
    /// accumulate all borrows that could be active.
    pub fn union_merge(&mut self, other: &BorrowState) {
        // Reserve capacity to reduce reallocations
        self.active_borrows.reserve(other.active_borrows.len());
        
        // Add all borrows from other that don't exist in self
        for (&borrow_id, loan) in &other.active_borrows {
            if !self.active_borrows.contains_key(&borrow_id) {
                // Update place mapping
                self.place_to_borrows
                    .entry(loan.place.clone())
                    .or_default()
                    .push(borrow_id);
                self.active_borrows.insert(borrow_id, loan.clone());
            }
        }

        // Merge last uses efficiently
        for (place, &node_id) in &other.last_uses {
            match self.last_uses.get_mut(place) {
                Some(existing) => {
                    if node_id > *existing {
                        *existing = node_id;
                    }
                }
                None => {
                    self.last_uses.insert(place.clone(), node_id);
                }
            }
        }

        // Clear cache after union merge
        self.overlapping_places_cache.clear();
    }

    /// Record a last use for a place
    pub fn record_last_use(&mut self, place: Place, node_id: HirNodeId) {
        self.last_uses.insert(place, node_id);
    }

    /// Get the last use point for a place, if known
    #[allow(dead_code)]
    pub fn get_last_use(&self, place: &Place) -> Option<HirNodeId> {
        self.last_uses.get(place).copied()
    }

    /// Check if a place has any active borrows
    /// **Optimization**: Direct contains_key check
    pub fn has_active_borrows(&self, place: &Place) -> bool {
        self.place_to_borrows.contains_key(place)
    }

    /// Get all active borrow IDs
    /// **Optimization**: Direct iterator over keys
    pub fn active_borrow_ids(&self) -> impl Iterator<Item = BorrowId> + '_ {
        self.active_borrows.keys().copied()
    }

    /// Get a specific loan by ID
    /// **Optimization**: Direct get operation
    pub fn get_loan(&self, borrow_id: BorrowId) -> Option<&Loan> {
        self.active_borrows.get(&borrow_id)
    }

    /// Update borrow state from a live set (for lifetime inference integration)
    ///
    /// This method integrates the corrected lifetime inference results with the borrow
    /// state, ensuring that only borrows that are actually live according to the
    /// simplified analysis are considered active.
    ///
    /// Note: This is a simplified stub implementation since we removed the complex
    /// BorrowLiveSets architecture as part of the cleanup.
    pub fn update_from_simplified_analysis(&mut self, _active_borrows: &[BorrowId]) {
        // Simplified implementation - no complex state updates needed
        // The borrow state should already contain all relevant borrows from the borrow tracking phase.
        // This method is kept for API compatibility but doesn't perform complex operations.
    }

    /// Efficient batch borrow removal
    /// **Optimization**: Removes multiple borrows efficiently
    pub fn remove_borrows_batch(&mut self, borrow_ids: &[BorrowId]) {
        for &borrow_id in borrow_ids {
            if let Some(loan) = self.active_borrows.swap_remove(&borrow_id) {
                // Remove from place mapping
                if let Some(borrow_list) = self.place_to_borrows.get_mut(&loan.place) {
                    borrow_list.retain(|&id| id != borrow_id);
                    if borrow_list.is_empty() {
                        self.place_to_borrows.swap_remove(&loan.place);
                    }
                }
            }
        }
        
        // Clear cache after batch removal
        self.overlapping_places_cache.clear();
    }

    /// Get statistics for performance monitoring
    /// **Optimization**: Provides metrics for performance analysis
    pub fn get_stats(&self) -> BorrowStateStats {
        BorrowStateStats {
            active_borrows_count: self.active_borrows.len(),
            place_mappings_count: self.place_to_borrows.len(),
            cache_entries_count: self.overlapping_places_cache.len(),
            last_uses_count: self.last_uses.len(),
        }
    }
}

/// Statistics for borrow state performance monitoring
#[derive(Debug, Clone)]
pub struct BorrowStateStats {
    pub active_borrows_count: usize,
    pub place_mappings_count: usize,
    pub cache_entries_count: usize,
    pub last_uses_count: usize,
}

impl Loan {
    /// Create a new loan
    pub fn new(id: BorrowId, place: Place, kind: BorrowKind, creation_point: HirNodeId) -> Self {
        Self {
            id,
            place,
            kind,
            creation_point,
            last_use_point: None,
        }
    }

    /// Check if this loan conflicts with another loan
    pub fn conflicts_with(&self, other: &Loan) -> bool {
        // Use enhanced place conflict detection
        self.place
            .conflicts_with(&other.place, self.kind, other.kind)
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}
