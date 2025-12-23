//! Borrow Live Sets - Core Data Structure for Algebraic Lifetime Inference
//!
//! Implements the core data structure for algebraic lifetime inference using
//! set operations on active borrows rather than explicit path enumeration.

use crate::compiler::borrow_checker::types::{BorrowChecker, BorrowId, CfgNodeId};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;
use crate::compiler::hir::place::Place;
use std::collections::{HashMap, HashSet};

/// Efficient set of borrow IDs optimized for algebraic operations
pub(crate) type BorrowSet = HashSet<BorrowId>;

/// State transition record for debugging and analysis
///
/// Records all borrow state transitions to provide clear visibility into
/// how borrows enter and exit live sets during dataflow analysis.
#[derive(Debug, Clone)]
pub(crate) struct StateTransition {
    /// CFG node where the transition occurred
    pub(crate) node_id: CfgNodeId,

    /// Type of state transition
    pub(crate) transition_type: TransitionType,

    /// Borrow ID involved in the transition
    pub(crate) borrow_id: BorrowId,

    /// Place associated with the borrow
    pub(crate) place: Place,

    /// Live set size before the transition
    pub(crate) live_set_size_before: usize,

    /// Live set size after the transition
    pub(crate) live_set_size_after: usize,
}

/// Types of borrow state transitions
///
/// These represent the fundamental operations that can occur to borrow
/// state during dataflow analysis.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum TransitionType {
    /// Borrow enters the live set (creation or propagation)
    Enter,

    /// Borrow exits the live set (kill or end of lifetime)
    Exit,

    /// Borrow propagated from predecessor (no change in liveness)
    Propagate,

    /// Borrow merged at join point
    Merge,
}

/// Core data structure for algebraic lifetime inference
#[derive(Debug, Clone)]
pub(crate) struct BorrowLiveSets {
    /// Active borrow sets per CFG node - the heart of algebraic analysis
    /// Maps each CFG node to the set of borrows that are live at that point
    live_sets: HashMap<CfgNodeId, BorrowSet>,

    /// Borrow creation points for dominance checking and soundness validation
    creation_points: HashMap<BorrowId, CfgNodeId>,

    /// Borrow kill points from last-use analysis
    /// None indicates the borrow extends to function exit (conservative)
    kill_points: HashMap<BorrowId, Option<CfgNodeId>>,

    /// Place information for each borrow (for conflict detection integration)
    borrow_places: HashMap<BorrowId, Place>,

    /// Borrow kind tracking for identity-based conflict detection
    borrow_kinds: HashMap<BorrowId, crate::compiler::borrow_checker::types::BorrowKind>,

    /// Stability flag for fixpoint convergence validation
    is_stable: bool,

    /// State transition log for debugging visibility
    /// Records all borrow state transitions for analysis and debugging
    state_transitions: Vec<StateTransition>,
}

impl BorrowLiveSets {
    /// Create a new empty borrow live sets structure
    pub(crate) fn new() -> Self {
        Self {
            live_sets: HashMap::new(),
            creation_points: HashMap::new(),
            kill_points: HashMap::new(),
            borrow_places: HashMap::new(),
            borrow_kinds: HashMap::new(),
            is_stable: false,
            state_transitions: Vec::new(),
        }
    }

    /// Initialize live sets from the current borrow checker state
    pub(crate) fn initialize_from_cfg(
        &mut self,
        checker: &BorrowChecker,
    ) -> Result<(), CompilerMessages> {
        // Clear any existing state
        self.live_sets.clear();
        self.creation_points.clear();
        self.kill_points.clear();
        self.borrow_places.clear();
        self.borrow_kinds.clear();
        self.is_stable = false;
        self.state_transitions.clear();

        // Extract borrows from each CFG node
        for (node_id, cfg_node) in &checker.cfg.nodes {
            let mut node_live_set = BorrowSet::new();

            // Add all active borrows at this node
            for loan in cfg_node.borrow_state.active_borrows.values() {
                node_live_set.insert(loan.id);

                // Record creation point (first time we see this borrow)
                self.creation_points.entry(loan.id).or_insert(*node_id);

                // Record place information for conflict detection
                self.borrow_places.insert(loan.id, loan.place.clone());

                // Record borrow kind for identity-based conflict detection
                self.borrow_kinds.insert(loan.id, loan.kind);

                // Initialize kill point as None (will be determined by last-use analysis)
                self.kill_points
                    .entry(loan.id)
                    .or_insert(loan.last_use_point);
            }

            self.live_sets.insert(*node_id, node_live_set);
        }

        // Successfully initialized live sets from CFG
        // Debug info: {} nodes, {} borrows tracked

        Ok(())
    }

    /// Add a borrow to the live set at a specific CFG node
    pub(crate) fn create_borrow(&mut self, node: CfgNodeId, borrow: BorrowId) {
        let live_set_size_before = self.live_at(node).len();

        let was_inserted = self.live_sets.entry(node).or_default().insert(borrow);

        // Only record transition if borrow was actually added
        if was_inserted {
            let live_set_size_after = self.live_at(node).len();

            // Record state transition for debugging visibility
            if let Some(place) = self.borrow_places.get(&borrow) {
                self.record_state_transition(StateTransition {
                    node_id: node,
                    transition_type: TransitionType::Enter,
                    borrow_id: borrow,
                    place: place.clone(),
                    live_set_size_before,
                    live_set_size_after,
                });
            }
        }

        // Mark as unstable since we modified the sets
        self.is_stable = false;
    }

    /// Remove a borrow from live sets starting at a specific CFG node
    pub(crate) fn kill_borrow(&mut self, node: CfgNodeId, borrow: BorrowId) {
        let live_set_size_before = self.live_at(node).len();
        let was_removed = if let Some(live_set) = self.live_sets.get_mut(&node) {
            live_set.remove(&borrow)
        } else {
            false
        };

        // Only record transition if borrow was actually removed
        if was_removed {
            let live_set_size_after = self.live_at(node).len();

            // Record state transition for debugging visibility
            if let Some(place) = self.borrow_places.get(&borrow) {
                self.record_state_transition(StateTransition {
                    node_id: node,
                    transition_type: TransitionType::Exit,
                    borrow_id: borrow,
                    place: place.clone(),
                    live_set_size_before,
                    live_set_size_after,
                });
            }
        }

        // Record the kill point
        self.kill_points.insert(borrow, Some(node));

        // Mark as unstable since we modified the sets
        self.is_stable = false;
    }

    /// Get the active borrow set at a specific CFG node
    pub(crate) fn live_at_ref(&self, node: CfgNodeId) -> Option<&BorrowSet> {
        self.live_sets.get(&node)
    }

    /// Get the active borrow set at a specific CFG node
    pub(crate) fn live_at(&self, node: CfgNodeId) -> BorrowSet {
        self.live_sets.get(&node).cloned().unwrap_or_default()
    }

    /// Get a mutable reference to the live set at a specific CFG node
    pub(crate) fn live_at_mut(&mut self, node: CfgNodeId) -> &mut BorrowSet {
        self.is_stable = false; // Any mutation marks as unstable
        self.live_sets.entry(node).or_default()
    }

    /// Merge borrow sets at a CFG join point using set union
    pub(crate) fn merge_at_join(&mut self, join_node: CfgNodeId, incoming: &[CfgNodeId]) {
        // Get the current live set size before merging
        let live_set_size_before = self.live_sets.get(&join_node).map(|s| s.len()).unwrap_or(0);

        // Collect all borrows from incoming nodes first
        let mut merged_set = BorrowSet::new();
        for &pred_node in incoming {
            if let Some(pred_set) = self.live_sets.get(&pred_node) {
                merged_set.extend(pred_set.iter().copied());
            }
        }

        // Check if anything changed
        let changed = match self.live_sets.get(&join_node) {
            Some(existing_set) => *existing_set != merged_set,
            None => !merged_set.is_empty(),
        };

        if changed {
            let live_set_size_after = merged_set.len();

            // Collect borrows that are new at this join point
            let new_borrows: Vec<BorrowId> = merged_set
                .iter()
                .filter(|&&borrow_id| !self.is_live_at(join_node, borrow_id))
                .copied()
                .collect();

            // Update the live set first
            self.live_sets.insert(join_node, merged_set);
            self.is_stable = false;

            // Record merge transitions for new borrows
            for borrow_id in new_borrows {
                if let Some(place) = self.borrow_places.get(&borrow_id) {
                    self.record_state_transition(StateTransition {
                        node_id: join_node,
                        transition_type: TransitionType::Merge,
                        borrow_id,
                        place: place.clone(),
                        live_set_size_before,
                        live_set_size_after,
                    });
                }
            }
        }
    }

    /// Check if a specific borrow is live at a given CFG node
    pub(crate) fn is_live_at(&self, node: CfgNodeId, borrow: BorrowId) -> bool {
        self.live_sets
            .get(&node)
            .map(|set| set.contains(&borrow))
            .unwrap_or(false)
    }

    /// Get all borrows that are live at any point in the analysis
    pub(crate) fn all_borrows(&self) -> impl Iterator<Item = BorrowId> + '_ {
        self.creation_points.keys().copied()
    }

    /// Get the creation point for a specific borrow
    pub(crate) fn creation_point(&self, borrow: BorrowId) -> Option<CfgNodeId> {
        self.creation_points.get(&borrow).copied()
    }

    /// Get all creation points for soundness validation
    pub(crate) fn creation_points(&self) -> impl Iterator<Item = (BorrowId, CfgNodeId)> + '_ {
        self.creation_points
            .iter()
            .map(|(&borrow, &node)| (borrow, node))
    }

    /// Get the kill point for a specific borrow
    pub(crate) fn kill_point(&self, borrow: BorrowId) -> Option<CfgNodeId> {
        self.kill_points.get(&borrow).and_then(|&opt_node| opt_node)
    }

    /// Get all kill points for move refinement integration
    pub(crate) fn all_kill_points(&self) -> impl Iterator<Item = (BorrowId, CfgNodeId)> + '_ {
        self.kill_points
            .iter()
            .filter_map(|(&borrow, &opt_node)| opt_node.map(|node| (borrow, node)))
    }

    /// Get usage points for a specific borrow (nodes where it's in the live set)
    pub(crate) fn usage_points(&self, borrow: BorrowId) -> Vec<CfgNodeId> {
        self.live_sets
            .iter()
            .filter_map(|(&node, set)| {
                if set.contains(&borrow) {
                    Some(node)
                } else {
                    None
                }
            })
            .collect()
    }

    /// Get the place associated with a specific borrow
    pub(crate) fn borrow_place(&self, borrow: BorrowId) -> Option<&Place> {
        self.borrow_places.get(&borrow)
    }

    /// Get the borrow kind for identity-based conflict detection
    pub(crate) fn borrow_kind(
        &self,
        borrow: BorrowId,
    ) -> Option<crate::compiler::borrow_checker::types::BorrowKind> {
        self.borrow_kinds.get(&borrow).copied()
    }

    /// Check if two borrows exist on disjoint execution paths
    pub(crate) fn borrows_on_disjoint_paths(&self, borrow1: BorrowId, borrow2: BorrowId) -> bool {
        // Get the usage points for both borrows
        let usage1 = self.usage_points(borrow1);
        let usage2 = self.usage_points(borrow2);

        // If either borrow has no usage points, assume they are disjoint
        if usage1.is_empty() || usage2.is_empty() {
            return true;
        }

        // Check if any usage points overlap - if they do, borrows are not disjoint
        for &node1 in &usage1 {
            for &node2 in &usage2 {
                if node1 == node2 {
                    return false; // Found overlapping usage, not disjoint
                }
            }
        }

        // No overlapping usage points found, borrows are on disjoint paths
        true
    }

    /// Detect identity-based conflicts between borrows at a CFG node
    pub(crate) fn detect_identity_conflicts(&self, node: CfgNodeId) -> Vec<(BorrowId, BorrowId)> {
        let mut conflicts = Vec::new();
        let live_set = self.live_at(node);

        // Convert to vector for pairwise comparison
        let active_borrows: Vec<BorrowId> = live_set.iter().copied().collect();

        // Check all pairs of active borrows for conflicts
        for (i, &borrow1) in active_borrows.iter().enumerate() {
            for &borrow2 in active_borrows.iter().skip(i + 1) {
                // Skip if borrows are on disjoint paths (path-sensitive analysis)
                if self.borrows_on_disjoint_paths(borrow1, borrow2) {
                    continue;
                }

                // Check for identity-based conflicts using CFG-based analysis
                if self.borrows_conflict_by_identity(borrow1, borrow2) {
                    conflicts.push((borrow1, borrow2));
                }
            }
        }

        conflicts
    }

    /// Check if two borrows conflict based on their individual identities
    fn borrows_conflict_by_identity(&self, borrow1: BorrowId, borrow2: BorrowId) -> bool {
        // Get borrow information
        let place1 = match self.borrow_place(borrow1) {
            Some(place) => place,
            None => return false, // Unknown borrow, no conflict
        };

        let place2 = match self.borrow_place(borrow2) {
            Some(place) => place,
            None => return false, // Unknown borrow, no conflict
        };

        let kind1 = match self.borrow_kind(borrow1) {
            Some(kind) => kind,
            None => return false, // Unknown borrow kind, no conflict
        };

        let kind2 = match self.borrow_kind(borrow2) {
            Some(kind) => kind,
            None => return false, // Unknown borrow kind, no conflict
        };

        // Use the enhanced place conflict detection with borrow kinds
        place1.conflicts_with(place2, kind1, kind2)
    }

    /// Preserve borrow identity during control flow merging
    pub(crate) fn preserve_identity_during_merge(
        &mut self,
        incoming_nodes: &[CfgNodeId],
        target_node: CfgNodeId,
    ) {
        // Collect all unique borrows from incoming nodes
        let mut all_borrows = HashSet::new();

        for &incoming_node in incoming_nodes {
            let incoming_set = self.live_at(incoming_node);
            all_borrows.extend(incoming_set.iter().copied());
        }

        // For each borrow, determine if it should be preserved at the target
        let mut preserved_borrows = BorrowSet::new();

        for borrow_id in all_borrows {
            // Check how many incoming paths have this borrow
            let mut paths_with_borrow = 0;

            for &incoming_node in incoming_nodes {
                if self.is_live_at(incoming_node, borrow_id) {
                    paths_with_borrow += 1;
                }
            }

            // Preserve borrow identity: keep borrows that exist on ANY path
            // This maintains path-sensitive precision while preserving identity
            if paths_with_borrow > 0 {
                preserved_borrows.insert(borrow_id);
            }
        }

        // Update the target node's live set with preserved borrows
        self.live_sets.insert(target_node, preserved_borrows);
        self.is_stable = false;
    }

    /// Check if the live sets have reached a stable fixpoint
    pub(crate) fn is_stable(&self) -> bool {
        self.is_stable
    }

    /// Mark the live sets as stable (called by dataflow engine)
    pub(crate) fn mark_stable(&mut self) {
        self.is_stable = true;
    }

    /// Get all live sets for integration with other components
    pub(crate) fn all_live_sets(&self) -> impl Iterator<Item = (CfgNodeId, &BorrowSet)> + '_ {
        self.live_sets.iter().map(|(&node, set)| (node, set))
    }

    /// Get the number of CFG nodes with live sets
    pub(crate) fn node_count(&self) -> usize {
        self.live_sets.len()
    }

    /// Get the total number of borrows being tracked
    pub(crate) fn borrow_count(&self) -> usize {
        self.creation_points.len()
    }

    /// Update kill points based on last-use analysis results
    pub(crate) fn update_kill_points(&mut self, last_uses: &HashMap<Place, CfgNodeId>) {
        // Collect updates first to avoid borrowing conflicts
        let mut updates = Vec::new();

        for (&borrow_id, place) in &self.borrow_places {
            if let Some(&last_use_node) = last_uses.get(place) {
                updates.push((borrow_id, last_use_node));
            }
        }

        // Apply updates
        for (borrow_id, last_use_node) in updates {
            self.kill_points.insert(borrow_id, Some(last_use_node));

            // Remove the borrow from live sets after its last use
            self.kill_borrow_after_node(borrow_id, last_use_node);
        }

        self.is_stable = false; // Mark as unstable after updates
    }

    /// Remove a borrow from all live sets after a specific node
    fn kill_borrow_after_node(&mut self, borrow: BorrowId, after_node: CfgNodeId) {
        // Remove the borrow from all nodes that come after the last use
        // This is a simplified implementation - a full implementation would
        // use CFG successor relationships to be more precise
        for (&node_id, live_set) in self.live_sets.iter_mut() {
            if node_id > after_node {
                live_set.remove(&borrow);
            }
        }
    }

    /// Compute the set difference between two live sets
    pub(crate) fn set_difference(&self, node_a: CfgNodeId, node_b: CfgNodeId) -> BorrowSet {
        match (self.live_sets.get(&node_a), self.live_sets.get(&node_b)) {
            (Some(set_a), Some(set_b)) => set_a.difference(set_b).copied().collect(),
            (Some(set_a), None) => set_a.clone(),
            _ => BorrowSet::new(),
        }
    }

    /// Compute the set union of multiple live sets
    pub(crate) fn set_union(&self, nodes: &[CfgNodeId]) -> BorrowSet {
        let mut union_set = BorrowSet::new();

        for &node in nodes {
            if let Some(node_set) = self.live_sets.get(&node) {
                union_set.extend(node_set.iter().copied());
            }
        }

        union_set
    }

    /// Check if two live sets are identical
    ///
    /// Check if two live sets are identical
    pub(crate) fn sets_equal(&self, node_a: CfgNodeId, node_b: CfgNodeId) -> bool {
        match (self.live_sets.get(&node_a), self.live_sets.get(&node_b)) {
            (Some(set_a), Some(set_b)) => set_a == set_b,
            (None, None) => true,
            _ => false,
        }
    }

    /// Perform in-place union of a borrow set with the live set at a node
    pub(crate) fn union_with_set(&mut self, node: CfgNodeId, other_set: &BorrowSet) -> bool {
        let live_set = self.live_sets.entry(node).or_default();
        let old_len = live_set.len();

        // Extend with borrows from other set (in-place union)
        live_set.extend(other_set.iter().copied());

        // Mark as unstable if changed
        let changed = live_set.len() > old_len;
        if changed {
            self.is_stable = false;
        }

        changed
    }

    /// Perform in-place intersection of a borrow set with the live set at a node
    pub(crate) fn intersect_with_set(&mut self, node: CfgNodeId, other_set: &BorrowSet) -> bool {
        let live_set = self.live_sets.entry(node).or_default();
        let old_set = live_set.clone();

        // Keep only borrows that exist in both sets (in-place intersection)
        live_set.retain(|borrow_id| other_set.contains(borrow_id));

        // Mark as unstable if changed
        let changed = *live_set != old_set;
        if changed {
            self.is_stable = false;
        }

        changed
    }

    /// Remove a set of borrows from the live set at a node
    pub(crate) fn remove_set(&mut self, node: CfgNodeId, borrows_to_remove: &BorrowSet) -> bool {
        let live_set = self.live_at_mut(node);
        let old_len = live_set.len();

        // Remove all specified borrows
        for &borrow_id in borrows_to_remove {
            live_set.remove(&borrow_id);
        }

        // Return true if the set changed (shrank)
        live_set.len() < old_len
    }

    /// Get the size of the live set at a specific node
    pub(crate) fn live_set_size(&self, node: CfgNodeId) -> usize {
        self.live_at(node).len()
    }

    /// Check if the live set at a node is empty
    pub(crate) fn is_empty_at(&self, node: CfgNodeId) -> bool {
        self.live_at(node).is_empty()
    }

    /// Clear all live sets (used for reinitialization)
    pub(crate) fn clear(&mut self) {
        self.live_sets.clear();
        self.creation_points.clear();
        self.kill_points.clear();
        self.borrow_places.clear();
        self.borrow_kinds.clear();
        self.is_stable = false;
        self.state_transitions.clear();
    }

    /// Get statistics about the live sets for performance analysis
    pub(crate) fn statistics(&self) -> (usize, usize, usize, f64) {
        let total_nodes = self.live_sets.len();
        let total_borrows = self.creation_points.len();

        let max_live_set_size = self
            .live_sets
            .values()
            .map(|set| set.len())
            .max()
            .unwrap_or(0);

        let avg_live_set_size = if total_nodes > 0 {
            self.live_sets.values().map(|set| set.len()).sum::<usize>() as f64 / total_nodes as f64
        } else {
            0.0
        };

        (
            total_nodes,
            total_borrows,
            max_live_set_size,
            avg_live_set_size,
        )
    }

    /// Record a state transition for debugging visibility
    fn record_state_transition(&mut self, transition: StateTransition) {
        self.state_transitions.push(transition);
    }

    /// Get all recorded state transitions for debugging
    ///
    /// Returns a reference to all state transitions that occurred during analysis.
    /// This provides complete visibility into borrow state changes.
    pub(crate) fn get_state_transitions(&self) -> &[StateTransition] {
        &self.state_transitions
    }

    /// Get state transitions for a specific CFG node
    ///
    /// Returns all state transitions that occurred at the given CFG node,
    /// useful for debugging specific control flow points.
    pub(crate) fn get_transitions_for_node(&self, node_id: CfgNodeId) -> Vec<&StateTransition> {
        self.state_transitions
            .iter()
            .filter(|t| t.node_id == node_id)
            .collect()
    }

    /// Get state transitions for a specific borrow
    ///
    /// Returns all state transitions involving the given borrow ID,
    /// useful for tracking a borrow's lifetime through the analysis.
    pub(crate) fn get_transitions_for_borrow(&self, borrow_id: BorrowId) -> Vec<&StateTransition> {
        self.state_transitions
            .iter()
            .filter(|t| t.borrow_id == borrow_id)
            .collect()
    }

    /// Clear all recorded state transitions
    ///
    /// This can be used to reset the transition log for a fresh analysis run.
    pub(crate) fn clear_transitions(&mut self) {
        self.state_transitions.clear();
    }

    /// Apply simple state transition rules for borrow propagation
    ///
    /// This implements the core state transition rules for clean borrow state management:
    /// 1. Borrows enter live sets at creation points
    /// 2. Borrows exit live sets at kill points  
    /// 3. Borrows propagate through CFG edges using straightforward set operations
    /// 4. Join points use simple set union without complex merging logic
    ///
    /// These rules ensure predictable and debuggable borrow state transitions.
    pub(crate) fn apply_transition_rules(
        &mut self,
        node_id: CfgNodeId,
        predecessors: &[CfgNodeId],
    ) {
        let live_set_size_before = self.live_at(node_id).len();

        // Rule 1: Start with empty set for entry nodes
        if predecessors.is_empty() {
            // Entry node - no incoming borrows
            if !self.live_at(node_id).is_empty() {
                self.live_sets.insert(node_id, BorrowSet::new());
            }
            return;
        }

        // Rule 2: Single predecessor - direct propagation
        if predecessors.len() == 1 {
            let pred_set = self.live_at(predecessors[0]);
            let current_set = self.live_at(node_id);

            if pred_set != current_set {
                // Record propagation transitions for new borrows
                for &borrow_id in &pred_set {
                    if !current_set.contains(&borrow_id)
                        && let Some(place) = self.borrow_places.get(&borrow_id)
                    {
                        self.record_state_transition(StateTransition {
                            node_id,
                            transition_type: TransitionType::Propagate,
                            borrow_id,
                            place: place.clone(),
                            live_set_size_before,
                            live_set_size_after: pred_set.len(),
                        });
                    }
                }

                self.live_sets.insert(node_id, pred_set);
                self.is_stable = false;
            }
            return;
        }

        // Rule 3: Multiple predecessors - simple union merge
        self.merge_at_join(node_id, predecessors);
    }

    /// Debug print live sets at all CFG nodes
    ///
    /// Provides comprehensive debugging visibility into the current state
    /// of all live sets across the entire CFG.
    pub(crate) fn debug_print_live_sets(&self) {
        println!("=== Live Sets Debug Information ===");
        println!("Total nodes: {}", self.live_sets.len());
        println!("Total borrows: {}", self.creation_points.len());
        println!("Total transitions: {}", self.state_transitions.len());
        println!();

        for (&node_id, live_set) in &self.live_sets {
            println!("Node {:?}: {} borrows live", node_id, live_set.len());
            for &borrow_id in live_set {
                if let Some(place) = self.borrow_places.get(&borrow_id)
                    && let Some(kind) = self.borrow_kinds.get(&borrow_id)
                {
                    println!("  - Borrow {} ({:?}): {:?}", borrow_id, kind, place);
                }
            }
            println!();
        }
    }

    /// Debug print state transitions
    ///
    /// Provides detailed visibility into all state transitions that occurred
    /// during analysis, useful for understanding complex control flow behavior.
    pub(crate) fn debug_print_transitions(&self) {
        println!("=== State Transitions Debug Information ===");
        println!("Total transitions: {}", self.state_transitions.len());
        println!();

        for (i, transition) in self.state_transitions.iter().enumerate() {
            println!(
                "Transition {}: {:?} at node {:?}",
                i, transition.transition_type, transition.node_id
            );
            println!("  Borrow {}: {:?}", transition.borrow_id, transition.place);
            println!(
                "  Live set size: {} -> {}",
                transition.live_set_size_before, transition.live_set_size_after
            );
            println!();
        }
    }

    /// Validate state transition invariants
    ///
    /// Checks that all recorded state transitions follow the expected rules:
    /// - Enter transitions increase live set size
    /// - Exit transitions decrease live set size  
    /// - Merge transitions may increase live set size
    /// - Propagate transitions maintain or increase live set size
    ///
    /// Returns true if all invariants hold, false otherwise.
    pub(crate) fn validate_transition_invariants(&self) -> bool {
        for transition in &self.state_transitions {
            match transition.transition_type {
                TransitionType::Enter | TransitionType::Merge => {
                    // Enter and merge should increase or maintain live set size
                    if transition.live_set_size_after < transition.live_set_size_before {
                        println!(
                            "INVARIANT VIOLATION: {:?} transition decreased live set size at node {:?}",
                            transition.transition_type, transition.node_id
                        );
                        return false;
                    }
                }
                TransitionType::Exit => {
                    // Exit should decrease live set size
                    if transition.live_set_size_after >= transition.live_set_size_before {
                        println!(
                            "INVARIANT VIOLATION: Exit transition did not decrease live set size at node {:?}",
                            transition.node_id
                        );
                        return false;
                    }
                }
                TransitionType::Propagate => {
                    // Propagate should maintain or increase live set size
                    if transition.live_set_size_after < transition.live_set_size_before {
                        println!(
                            "INVARIANT VIOLATION: Propagate transition decreased live set size at node {:?}",
                            transition.node_id
                        );
                        return false;
                    }
                }
            }
        }

        true
    }
}
