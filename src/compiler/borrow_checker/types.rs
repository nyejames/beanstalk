//! Core data structures for the borrow checker
//!
//! This module defines the fundamental types used throughout the borrow checking
//! process, including the main BorrowChecker struct, control flow graph representation,
//! borrow state tracking, and loan management.

use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use crate::compiler::compiler_messages::compiler_warnings::CompilerWarning;
use crate::compiler::hir::nodes::HirNodeId;
use crate::compiler::hir::place::Place;
use crate::compiler::parsers::statements::functions::FunctionSignature;
use crate::compiler::string_interning::InternedString;
use crate::compiler::string_interning::StringTable;
use std::collections::HashMap;

/// Unique identifier for borrows
pub type BorrowId = usize;

/// The main borrow checker state and context
///
/// This struct maintains all the state needed for borrow checking analysis,
/// including the control flow graph, active borrows, error collection, and
/// string table for error reporting.
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

/// Control Flow Graph representation for borrow analysis
///
/// The CFG provides a graph-based view of program execution flow, enabling
/// path-sensitive borrow checking and lifetime analysis.
#[derive(Debug, Clone)]
pub struct ControlFlowGraph {
    /// Mapping from HIR node IDs to CFG nodes
    pub nodes: HashMap<HirNodeId, CfgNode>,

    /// Edges between CFG nodes (node_id -> list of successor node_ids)
    pub edges: HashMap<HirNodeId, Vec<HirNodeId>>,

    /// Entry points for functions (typically one per function)
    pub entry_points: Vec<HirNodeId>,

    /// Exit points for functions (return statements and implicit exits)
    pub exit_points: Vec<HirNodeId>,
}

/// A single node in the control flow graph
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

/// Borrow state tracking for a single CFG node
///
/// This tracks all active borrows at a particular point in the program,
/// enabling conflict detection and lifetime analysis.
#[derive(Debug, Clone, Default)]
pub struct BorrowState {
    /// All currently active borrows, indexed by borrow ID
    pub active_borrows: HashMap<BorrowId, Loan>,

    /// Mapping from places to the borrows that reference them
    pub place_to_borrows: HashMap<Place, Vec<BorrowId>>,

    /// Last use points for each place (for move analysis)
    #[allow(dead_code)]
    pub last_uses: HashMap<Place, HirNodeId>,
}

/// A tracked borrow with its metadata
///
/// Loans represent active borrows in the system, tracking their creation point,
/// kind, target place, and active regions for lifetime analysis.
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

    /// CFG regions where this borrow is active
    #[allow(dead_code)]
    pub active_regions: Vec<CfgRegion>,
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

/// A region in the control flow graph where a borrow is active
///
/// CFG regions represent spans of execution where a particular borrow
/// remains valid, used for lifetime inference and conflict detection.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct CfgRegion {
    /// Starting CFG node for this region
    pub start: HirNodeId,

    /// Ending CFG node for this region
    pub end: HirNodeId,

    /// All execution paths through this region
    pub paths: Vec<Vec<HirNodeId>>,
}

impl<'a> BorrowChecker<'a> {
    /// Create a new borrow checker instance
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

    /// Generate a new unique borrow ID
    pub fn next_borrow_id(&mut self) -> BorrowId {
        let id = self.next_borrow_id;
        self.next_borrow_id += 1;
        id
    }

    /// Add an error to the error collection
    pub fn add_error(&mut self, error: CompilerError) {
        self.errors.push(error);
    }

    /// Add a warning to the warning collection
    #[allow(dead_code)]
    pub fn add_warning(&mut self, warning: CompilerWarning) {
        self.warnings.push(warning);
    }

    /// Finish borrow checking and return results
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
}

impl ControlFlowGraph {
    /// Create a new empty control flow graph
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            entry_points: Vec::new(),
            exit_points: Vec::new(),
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
        if let Some(node) = self.nodes.get(&node_id) {
            node.predecessors.clone()
        } else {
            Vec::new()
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
    #[allow(dead_code)]
    pub fn remove_borrow(&mut self, borrow_id: BorrowId) {
        if let Some(loan) = self.active_borrows.remove(&borrow_id) {
            // Remove from place mapping
            if let Some(borrow_list) = self.place_to_borrows.get_mut(&loan.place) {
                borrow_list.retain(|&id| id != borrow_id);
                if borrow_list.is_empty() {
                    self.place_to_borrows.remove(&loan.place);
                }
            }
        }
    }

    /// Get all borrows for a specific place
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
        for (existing_place, borrow_ids) in &self.place_to_borrows {
            if place.overlaps_with(existing_place) {
                for &borrow_id in borrow_ids {
                    if let Some(loan) = self.active_borrows.get(&borrow_id) {
                        result.push(loan);
                    }
                }
            }
        }
        result
    }

    /// Merge another borrow state into this one (for CFG join points)
    /// 
    /// This implements conservative merging for Polonius-style analysis:
    /// - At join points, we keep borrows that exist in BOTH incoming states
    /// - This ensures that conflicts are only reported if they exist on ALL paths
    pub fn merge(&mut self, other: &BorrowState) {
        // If this state is empty, copy from other (first incoming edge)
        if self.is_empty() {
            self.active_borrows = other.active_borrows.clone();
            self.place_to_borrows = other.place_to_borrows.clone();
            self.last_uses = other.last_uses.clone();
            return;
        }

        // Conservative merge: keep borrows that exist in both states
        // This is correct for Polonius-style analysis where conflicts
        // are only errors if they exist on ALL incoming paths
        let mut borrows_to_keep = HashMap::new();

        for (&borrow_id, loan) in &self.active_borrows {
            if other.active_borrows.contains_key(&borrow_id) {
                borrows_to_keep.insert(borrow_id, loan.clone());
            }
        }

        // Replace with merged state
        self.active_borrows = borrows_to_keep;

        // Rebuild place mapping
        self.place_to_borrows.clear();
        for loan in self.active_borrows.values() {
            self.place_to_borrows
                .entry(loan.place.clone())
                .or_default()
                .push(loan.id);
        }

        // Merge last uses - keep the later use point for each place
        for (place, &node_id) in &other.last_uses {
            self.last_uses
                .entry(place.clone())
                .and_modify(|existing| {
                    // Keep the larger node ID (later in execution)
                    if node_id > *existing {
                        *existing = node_id;
                    }
                })
                .or_insert(node_id);
        }
    }

    /// Union merge: combine borrows from both states (for propagation)
    /// 
    /// This is used when propagating state along CFG edges where we want
    /// to accumulate all borrows that could be active.
    pub fn union_merge(&mut self, other: &BorrowState) {
        // Add all borrows from other that don't exist in self
        for (&borrow_id, loan) in &other.active_borrows {
            if !self.active_borrows.contains_key(&borrow_id) {
                self.active_borrows.insert(borrow_id, loan.clone());
                self.place_to_borrows
                    .entry(loan.place.clone())
                    .or_default()
                    .push(borrow_id);
            }
        }

        // Merge last uses
        for (place, &node_id) in &other.last_uses {
            self.last_uses
                .entry(place.clone())
                .and_modify(|existing| {
                    if node_id > *existing {
                        *existing = node_id;
                    }
                })
                .or_insert(node_id);
        }
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
    pub fn has_active_borrows(&self, place: &Place) -> bool {
        self.place_to_borrows.contains_key(place)
    }

    /// Get all active borrow IDs
    pub fn active_borrow_ids(&self) -> impl Iterator<Item = BorrowId> + '_ {
        self.active_borrows.keys().copied()
    }

    /// Get a specific loan by ID
    pub fn get_loan(&self, borrow_id: BorrowId) -> Option<&Loan> {
        self.active_borrows.get(&borrow_id)
    }
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
            active_regions: Vec::new(),
        }
    }

    /// Check if this loan conflicts with another loan
    pub fn conflicts_with(&self, other: &Loan) -> bool {
        // Use enhanced place conflict detection
        self.place.conflicts_with(&other.place, self.kind, other.kind)
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}
