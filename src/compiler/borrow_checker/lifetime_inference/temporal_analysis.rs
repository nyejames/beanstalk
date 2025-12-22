//! Temporal Analysis - CFG-Based Temporal Ordering for Algebraic Lifetime Inference
//!
//! This module implements **proper temporal analysis** using CFG dominance and
//! reachability instead of the previous incorrect approach of using HirNodeId
//! as temporal ordering. This provides sound temporal relationships that respect
//! actual program execution order, which is essential for the algebraic approach.
//!
//! ## The Temporal Ordering Problem
//!
//! Correct lifetime inference requires understanding the **temporal relationships**
//! between program points. The previous implementation incorrectly assumed that
//! HirNodeId ordering corresponds to execution time, which is false:
//!
//! ```text
//! // Example: Node IDs don't reflect execution order
//! if condition:           // Node 10 (executes first)
//!     borrow = &x         // Node 5  (lower ID, executes after condition!)
//! else:
//!     use(x)              // Node 15 (higher ID, may execute before borrow!)
//! ```
//!
//! ## CFG-Based Solution
//!
//! The algebraic approach solves this using **control flow graph analysis**:
//!
//! ### Dominance Relationships
//! - **Definition**: Node A dominates node B if every path from entry to B passes through A
//! - **Usage**: Ensures borrow creation points dominate all usage points
//! - **Algorithm**: Iterative dominance computation until fixpoint
//! - **Complexity**: O(|nodes|² × |edges|) worst case, O(|nodes| × |edges|) typical
//!
//! ### Reachability Analysis  
//! - **Definition**: Node A can reach node B if there exists a path from A to B
//! - **Usage**: Determines lifetime spans and last-use points
//! - **Algorithm**: Transitive closure using Floyd-Warshall approach
//! - **Complexity**: O(|nodes|³) but with sparse CFGs performs much better
//!
//! ## Integration with Algebraic Approach
//!
//! Temporal analysis provides the **foundation** for algebraic lifetime inference:
//! - **Soundness**: Dominance ensures creation-before-use invariants
//! - **Precision**: Reachability enables accurate last-use computation
//! - **Correctness**: CFG structure guides set operation ordering
//! - **Validation**: Temporal relationships validate computed lifetimes
//!
//! ## Key Design Principles
//!
//! - **CFG-Based Ordering**: Use dominance and reachability, not node ID comparison
//! - **Execution-Time Semantics**: Temporal relationships reflect actual program execution
//! - **Soundness Validation**: Creation points must dominate all usage points
//! - **Reachability Analysis**: Determine which nodes can reach which other nodes
//! - **Performance Optimization**: Precompute dominance trees and reachability matrices
//!
//! ## Temporal Correctness
//!
//! The previous implementation incorrectly used HirNodeId ordering as temporal
//! ordering, which doesn't correspond to execution time. This module fixes that
//! by using proper CFG analysis:
//!
//! ```text
//! // WRONG (old approach): node_id_1 < node_id_2 means temporal ordering
//! // RIGHT (new approach): dominance and reachability determine temporal ordering
//!
//! if condition:           // Node 10
//!     borrow = &x         // Node 5  (lower ID but executes after condition!)
//! else:
//!     use(x)              // Node 15 (higher ID but may execute before borrow!)
//! ```
//!
//! The new approach correctly handles such cases using CFG structure.

use crate::compiler::borrow_checker::types::{CfgNodeId, ControlFlowGraph};
use crate::compiler::compiler_messages::compiler_errors::{CompilerError, CompilerMessages};
use std::collections::{HashMap, HashSet};

/// Dominance and reachability information for temporal analysis
///
/// Precomputed information about CFG relationships that enables efficient
/// temporal queries during lifetime inference.
#[derive(Debug, Clone)]
pub(crate) struct DominanceInfo {
    /// Dominance relationships: dominators[n] = set of nodes that dominate n
    dominators: HashMap<CfgNodeId, HashSet<CfgNodeId>>,

    /// Immediate dominators: idom[n] = immediate dominator of n
    immediate_dominators: HashMap<CfgNodeId, Option<CfgNodeId>>,

    /// Reachability matrix: reachable[a][b] = true if a can reach b
    reachability: HashMap<CfgNodeId, HashSet<CfgNodeId>>,

    /// Post-dominance relationships for backward analysis
    post_dominators: HashMap<CfgNodeId, HashSet<CfgNodeId>>,

    /// CFG entry and exit points for boundary analysis
    entry_points: Vec<CfgNodeId>,
    exit_points: Vec<CfgNodeId>,
}

impl DominanceInfo {
    /// Create a new empty DominanceInfo
    pub(crate) fn new() -> Self {
        Self {
            dominators: HashMap::new(),
            immediate_dominators: HashMap::new(),
            reachability: HashMap::new(),
            post_dominators: HashMap::new(),
            entry_points: Vec::new(),
            exit_points: Vec::new(),
        }
    }

    /// Check if node A dominates node B
    ///
    /// A dominates B if every path from the entry to B must pass through A.
    /// This is the correct way to determine temporal ordering in CFG analysis.
    pub(crate) fn dominates(&self, dominator: CfgNodeId, dominated: CfgNodeId) -> bool {
        if dominator == dominated {
            return true; // A node dominates itself
        }

        self.dominators
            .get(&dominated)
            .map(|doms| doms.contains(&dominator))
            .unwrap_or(false)
    }

    /// Check if node A can reach node B through CFG edges
    ///
    /// This determines if there exists any execution path from A to B,
    /// which is essential for lifetime span computation.
    pub(crate) fn can_reach(&self, from: CfgNodeId, to: CfgNodeId) -> bool {
        if from == to {
            return true; // A node can reach itself
        }

        self.reachability
            .get(&from)
            .map(|reachable| reachable.contains(&to))
            .unwrap_or(false)
    }

    /// Get the immediate dominator of a node
    ///
    /// The immediate dominator is the closest dominator in the dominance tree.
    /// This is useful for constructing dominance-based lifetime relationships.
    pub(crate) fn immediate_dominator(&self, node: CfgNodeId) -> Option<CfgNodeId> {
        self.immediate_dominators.get(&node).copied().flatten()
    }

    /// Get all nodes that dominate the given node
    pub(crate) fn all_dominators(&self, node: CfgNodeId) -> Vec<CfgNodeId> {
        self.dominators
            .get(&node)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Get all nodes reachable from the given node
    pub(crate) fn all_reachable(&self, from: CfgNodeId) -> Vec<CfgNodeId> {
        self.reachability
            .get(&from)
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .collect()
    }

    /// Check if a node post-dominates another (for backward analysis)
    ///
    /// A post-dominates B if every path from B to the exit must pass through A.
    /// This is useful for determining where borrows must end.
    pub(crate) fn post_dominates(
        &self,
        post_dominator: CfgNodeId,
        post_dominated: CfgNodeId,
    ) -> bool {
        if post_dominator == post_dominated {
            return true;
        }

        self.post_dominators
            .get(&post_dominated)
            .map(|post_doms| post_doms.contains(&post_dominator))
            .unwrap_or(false)
    }

    /// Get CFG entry points
    pub(crate) fn entry_points(&self) -> &[CfgNodeId] {
        &self.entry_points
    }

    /// Get CFG exit points
    pub(crate) fn exit_points(&self) -> &[CfgNodeId] {
        &self.exit_points
    }
}

/// CFG-based temporal analysis engine
///
/// Computes dominance and reachability information for proper temporal
/// ordering in lifetime inference. This replaces the incorrect node ID
/// based approach with sound CFG analysis.
///
/// ## Key Features
///
/// - **Dominance Analysis**: Computes which nodes dominate others for temporal ordering
/// - **Reachability Matrix**: Determines which nodes can reach which others
/// - **Soundness Validation**: Enforces hard errors for invalid temporal relationships
/// - **CFG-Based Ordering**: Uses proper control flow analysis instead of node ID comparison
pub(crate) struct TemporalAnalysis<'a> {
    /// Reference to the control flow graph
    cfg: &'a ControlFlowGraph,
}

impl<'a> TemporalAnalysis<'a> {
    /// Create a new temporal analysis engine
    pub(crate) fn new(cfg: &'a ControlFlowGraph) -> Result<Self, CompilerMessages> {
        // Validate that the CFG is well-formed
        if cfg.nodes.is_empty() {
            let error = CompilerError {
                msg: "Cannot perform temporal analysis on empty CFG".to_string(),
                location:
                    crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                metadata: std::collections::HashMap::new(),
            };
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: Vec::new(),
            });
        }

        Ok(Self { cfg })
    }

    /// Check if creation dominates usage using CFG dominance instead of node ID comparison
    ///
    /// This is the core method that replaces the incorrect node ID comparison approach
    /// with proper CFG-based temporal analysis. Creation points must dominate all usage
    /// points for sound lifetime inference.
    pub(crate) fn creation_dominates_usage(
        &self,
        dominance_info: &DominanceInfo,
        creation: CfgNodeId,
        usage: CfgNodeId,
    ) -> bool {
        dominance_info.dominates(creation, usage)
    }

    /// Get the CFG reference for integration with other components
    pub(crate) fn cfg(&self) -> &ControlFlowGraph {
        self.cfg
    }

    /// Compute complete dominance and reachability information
    ///
    /// This is the main entry point that computes all temporal relationships
    /// needed for lifetime inference.
    pub(crate) fn compute_dominance_info(&self) -> Result<DominanceInfo, CompilerMessages> {
        // Computing dominance and reachability for CFG nodes

        // Identify entry and exit points
        let entry_points = self.find_entry_points();
        let exit_points = self.find_exit_points();

        // Compute dominance relationships
        let dominators = self.compute_dominators(&entry_points)?;
        let immediate_dominators = self.compute_immediate_dominators(&dominators);

        // Compute reachability relationships
        let reachability = self.compute_reachability()?;

        // Compute post-dominance relationships
        let post_dominators = self.compute_post_dominators(&exit_points)?;

        // Temporal analysis complete

        Ok(DominanceInfo {
            dominators,
            immediate_dominators,
            reachability,
            post_dominators,
            entry_points,
            exit_points,
        })
    }

    /// Find CFG entry points (nodes with no predecessors)
    fn find_entry_points(&self) -> Vec<CfgNodeId> {
        self.cfg
            .nodes
            .keys()
            .filter(|&&node| self.cfg.predecessors(node).is_empty())
            .copied()
            .collect()
    }

    /// Find CFG exit points (nodes with no successors)
    fn find_exit_points(&self) -> Vec<CfgNodeId> {
        self.cfg
            .nodes
            .keys()
            .filter(|&&node| self.cfg.successors(node).is_empty())
            .copied()
            .collect()
    }

    /// Compute dominance relationships using iterative algorithm
    ///
    /// Implements the classic dominance algorithm that iteratively computes
    /// the set of dominators for each node until convergence.
    fn compute_dominators(
        &self,
        entry_points: &[CfgNodeId],
    ) -> Result<HashMap<CfgNodeId, HashSet<CfgNodeId>>, CompilerMessages> {
        let mut dominators: HashMap<CfgNodeId, HashSet<CfgNodeId>> = HashMap::new();
        let all_nodes: HashSet<CfgNodeId> = self.cfg.nodes.keys().copied().collect();

        // Initialize: entry points dominate only themselves, others dominate all nodes
        for &node in self.cfg.nodes.keys() {
            if entry_points.contains(&node) {
                let mut entry_doms = HashSet::new();
                entry_doms.insert(node);
                dominators.insert(node, entry_doms);
            } else {
                dominators.insert(node, all_nodes.clone());
            }
        }

        // Iterate until convergence
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 1000;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            for &node in self.cfg.nodes.keys() {
                if entry_points.contains(&node) {
                    continue; // Entry points don't change
                }

                // New dominators = {node} ∪ (∩ dominators of all predecessors)
                let predecessors = self.cfg.predecessors(node);
                if predecessors.is_empty() {
                    continue; // Should not happen for non-entry nodes
                }

                let mut new_dominators = all_nodes.clone();
                for &pred in &predecessors {
                    if let Some(pred_doms) = dominators.get(&pred) {
                        new_dominators = new_dominators.intersection(pred_doms).copied().collect();
                    }
                }
                new_dominators.insert(node); // A node always dominates itself

                if dominators.get(&node) != Some(&new_dominators) {
                    dominators.insert(node, new_dominators);
                    changed = true;
                }
            }
        }

        if iterations >= MAX_ITERATIONS {
            let error = CompilerError {
                msg: format!(
                    "Dominance computation failed to converge after {} iterations",
                    MAX_ITERATIONS
                ),
                location:
                    crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                metadata: std::collections::HashMap::new(),
            };
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: Vec::new(),
            });
        }

        // Dominance computation converged
        Ok(dominators)
    }

    /// Compute immediate dominators from dominance relationships
    ///
    /// The immediate dominator is the closest dominator in the dominance tree.
    fn compute_immediate_dominators(
        &self,
        dominators: &HashMap<CfgNodeId, HashSet<CfgNodeId>>,
    ) -> HashMap<CfgNodeId, Option<CfgNodeId>> {
        let mut immediate_dominators = HashMap::new();

        for (&node, node_dominators) in dominators {
            // Find the immediate dominator (closest in dominance tree)
            let mut candidates: Vec<CfgNodeId> = node_dominators.iter().copied().collect();
            candidates.retain(|&dom| dom != node); // Remove self

            if candidates.is_empty() {
                immediate_dominators.insert(node, None); // Entry node
                continue;
            }

            // The immediate dominator is the one that is not dominated by any other candidate
            let mut idom = None;
            for &candidate in &candidates {
                let is_immediate = candidates.iter().all(|&other| {
                    other == candidate
                        || !dominators
                            .get(&candidate)
                            .unwrap_or(&HashSet::new())
                            .contains(&other)
                });

                if is_immediate {
                    idom = Some(candidate);
                    break;
                }
            }

            immediate_dominators.insert(node, idom);
        }

        immediate_dominators
    }

    /// Compute reachability relationships using transitive closure
    ///
    /// Determines which nodes can reach which other nodes through CFG edges.
    /// This is essential for lifetime span computation.
    /// Optimized to minimize redundant computations and memory allocations.
    fn compute_reachability(
        &self,
    ) -> Result<HashMap<CfgNodeId, HashSet<CfgNodeId>>, CompilerMessages> {
        let mut reachability: HashMap<CfgNodeId, HashSet<CfgNodeId>> = HashMap::new();
        let nodes: Vec<CfgNodeId> = self.cfg.nodes.keys().copied().collect();

        // Initialize: each node can reach itself and its immediate successors
        for &node in &nodes {
            let mut reachable = HashSet::new();
            reachable.insert(node); // Self-reachable

            // Add immediate successors
            for &successor in self.cfg.successors(node) {
                reachable.insert(successor);
            }

            reachability.insert(node, reachable);
        }

        // Compute transitive closure using optimized Floyd-Warshall
        // Process nodes in a more cache-friendly order
        for &k in &nodes {
            // Get k's reachable set once to avoid repeated HashMap lookups
            let k_reachable = reachability.get(&k).cloned().unwrap_or_default();

            for &i in &nodes {
                // Skip if i cannot reach k
                if !reachability
                    .get(&i)
                    .map(|r| r.contains(&k))
                    .unwrap_or(false)
                {
                    continue;
                }

                // i can reach k, so i can reach everything k can reach
                let i_reachable = reachability.entry(i).or_default();
                let old_size = i_reachable.len();

                // Extend with k's reachable nodes (in-place union)
                i_reachable.extend(k_reachable.iter().copied());

                // Early termination if no changes (optimization)
                if i_reachable.len() == old_size {
                    continue;
                }
            }
        }

        // Reachability computation complete
        Ok(reachability)
    }

    /// Compute post-dominance relationships for backward analysis
    ///
    /// Post-dominance is useful for determining where borrows must end
    /// and for backward dataflow analysis.
    fn compute_post_dominators(
        &self,
        exit_points: &[CfgNodeId],
    ) -> Result<HashMap<CfgNodeId, HashSet<CfgNodeId>>, CompilerMessages> {
        let mut post_dominators: HashMap<CfgNodeId, HashSet<CfgNodeId>> = HashMap::new();
        let all_nodes: HashSet<CfgNodeId> = self.cfg.nodes.keys().copied().collect();

        // Initialize: exit points post-dominate only themselves, others post-dominate all nodes
        for &node in self.cfg.nodes.keys() {
            if exit_points.contains(&node) {
                let mut exit_post_doms = HashSet::new();
                exit_post_doms.insert(node);
                post_dominators.insert(node, exit_post_doms);
            } else {
                post_dominators.insert(node, all_nodes.clone());
            }
        }

        // Iterate until convergence (similar to dominance but in reverse)
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 1000;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;

            for &node in self.cfg.nodes.keys() {
                if exit_points.contains(&node) {
                    continue; // Exit points don't change
                }

                // New post-dominators = {node} ∪ (∩ post-dominators of all successors)
                let successors = self.cfg.successors(node);
                if successors.is_empty() {
                    continue; // Should not happen for non-exit nodes
                }

                let mut new_post_dominators = all_nodes.clone();
                for &succ in successors {
                    if let Some(succ_post_doms) = post_dominators.get(&succ) {
                        new_post_dominators = new_post_dominators
                            .intersection(succ_post_doms)
                            .copied()
                            .collect();
                    }
                }
                new_post_dominators.insert(node); // A node always post-dominates itself

                if post_dominators.get(&node) != Some(&new_post_dominators) {
                    post_dominators.insert(node, new_post_dominators);
                    changed = true;
                }
            }
        }

        if iterations >= MAX_ITERATIONS {
            let error = CompilerError {
                msg: format!(
                    "Post-dominance computation failed to converge after {} iterations",
                    MAX_ITERATIONS
                ),
                location:
                    crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                metadata: std::collections::HashMap::new(),
            };
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: Vec::new(),
            });
        }

        // Post-dominance computation converged
        Ok(post_dominators)
    }

    /// Compute last-use points using CFG reachability analysis
    ///
    /// This replaces the previous incorrect approach of using node ID comparison
    /// with proper CFG-based analysis to find actual last-use points.
    pub(crate) fn compute_last_uses(
        &self,
        place_usages: &HashMap<crate::compiler::hir::place::Place, Vec<CfgNodeId>>,
    ) -> Result<HashMap<crate::compiler::hir::place::Place, CfgNodeId>, CompilerMessages> {
        let mut last_uses = HashMap::new();
        let reachability = self.compute_reachability()?;

        for (place, usage_nodes) in place_usages {
            // For each usage, check if any later usage is reachable
            for &usage_node in usage_nodes {
                let is_last_use = usage_nodes
                    .iter()
                    .filter(|&&other| other != usage_node)
                    .all(|&other| {
                        !reachability
                            .get(&usage_node)
                            .map(|reachable| reachable.contains(&other))
                            .unwrap_or(false)
                    });

                if is_last_use {
                    last_uses.insert(place.clone(), usage_node);
                    break; // Found the last use for this place
                }
            }
        }

        Ok(last_uses)
    }

    /// Validate dominance relationships for soundness checking with hard error enforcement
    ///
    /// Ensures that all borrow creation points properly dominate their usage points,
    /// which is a fundamental soundness requirement. This method enforces soundness
    /// by halting compilation with fatal errors instead of just logging warnings.
    pub(crate) fn validate_dominance(
        &self,
        dominance_info: &DominanceInfo,
        borrows: &[(
            crate::compiler::borrow_checker::types::BorrowId,
            CfgNodeId,
            Vec<CfgNodeId>,
        )],
    ) -> Result<(), CompilerMessages> {
        for (borrow_id, creation_point, usage_points) in borrows {
            for &usage_point in usage_points {
                if !dominance_info.dominates(*creation_point, usage_point) {
                    let error = CompilerError {
                        msg: format!("Soundness violation: Borrow {:?} used before creation. Created at {:?}, used at {:?}", borrow_id, creation_point, usage_point),
                        location: crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                        error_type: crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                        metadata: std::collections::HashMap::new(),
                    };
                    return Err(CompilerMessages {
                        errors: vec![error],
                        warnings: Vec::new(),
                    });
                }
            }
        }

        Ok(())
    }
}
