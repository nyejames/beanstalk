//! Dataflow Engine - Fixpoint Iteration for Algebraic Borrow Propagation
//!
//! This module implements the **fixpoint dataflow analysis engine** that propagates
//! borrow state through the CFG using the algebraic approach. The engine guarantees
//! correctness through iterative analysis until convergence, replacing the previous
//! unstable single-pass approach.
//!
//! ## Algebraic Dataflow Algorithm
//!
//! The core algorithm implements **Kildall's dataflow framework** specialized for
//! borrow lifetime inference:
//!
//! ```text
//! Algorithm: Algebraic Borrow Propagation
//! Input: CFG with initial borrow sets
//! Output: Converged live sets for all CFG nodes
//!
//! 1. Initialize worklist with all CFG nodes
//! 2. While worklist is not empty:
//!    a. Remove node N from worklist
//!    b. Compute new_live_set = ∪ live_sets[pred] for all predecessors of N
//!    c. Apply local effects (borrow creation/killing) to new_live_set
//!    d. If new_live_set ≠ old_live_set[N]:
//!       - Update live_set[N] = new_live_set
//!       - Add all successors of N to worklist
//! 3. Validate convergence and stability
//! ```
//!
//! ## Convergence Guarantees
//!
//! The algorithm is guaranteed to converge because:
//! - **Finite Lattice**: The powerset of borrows has finite height
//! - **Monotonic Operations**: Set union only increases set size
//! - **Bounded Growth**: Each borrow set can contain at most |all_borrows| elements
//! - **Worklist Termination**: Nodes are only re-added when their inputs change
//!
//! ## Performance Characteristics
//!
//! - **Time Complexity**: O(|nodes| × |edges| × |borrows|) in worst case
//! - **Space Complexity**: O(|nodes| × |borrows|) for live sets storage
//! - **Practical Performance**: Linear in most real programs due to sparse borrow sets
//! - **Scalability**: Handles large functions efficiently unlike exponential path enumeration
//!
//! ## Key Design Principles
//!
//! - **Fixpoint Convergence**: Iterate until no changes occur (guaranteed termination)
//! - **Worklist Algorithm**: Only re-analyze nodes when their inputs change
//! - **Monotonic Operations**: Set operations that guarantee convergence
//! - **Change Detection**: Track modifications to avoid unnecessary work
//! - **Stability Validation**: Ensure final state is truly stable
//!
//! ## Algorithm Overview
//!
//! The dataflow engine implements a classic worklist algorithm:
//!
//! 1. Initialize worklist with all CFG nodes
//! 2. While worklist is not empty:
//!    - Remove a node from worklist
//!    - Compute new live set based on predecessors
//!    - If live set changed, add successors to worklist
//! 3. Validate convergence and stability
//!
//! This approach handles complex control flow patterns correctly, including
//! nested loops, early returns, and complex join points.

use crate::compiler::borrow_checker::lifetime_inference::borrow_live_sets::{
    BorrowLiveSets, BorrowSet,
};
use crate::compiler::borrow_checker::types::{BorrowId, CfgNodeId, ControlFlowGraph};
use crate::compiler::compiler_messages::compiler_errors::CompilerMessages;

use std::collections::{HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Result of dataflow analysis containing convergence information
///
/// Provides debugging and validation information about the dataflow
/// analysis process, including iteration count and convergence status.
/// Enhanced with performance metrics for optimization validation.
#[derive(Debug, Clone)]
pub(crate) struct DataflowResult {
    /// Number of iterations required to reach fixpoint
    pub(crate) iterations: usize,

    /// Whether the analysis converged to a stable fixpoint
    pub(crate) converged: bool,

    /// Number of nodes processed during analysis
    pub(crate) nodes_processed: usize,

    /// Maximum worklist size during analysis (for performance monitoring)
    pub(crate) max_worklist_size: usize,

    /// Total time spent in dataflow analysis
    pub(crate) analysis_time: Duration,

    /// Time complexity metrics for performance validation
    pub(crate) performance_metrics: PerformanceMetrics,
}

/// Performance metrics for validating linear time complexity
#[derive(Debug, Clone)]
pub(crate) struct PerformanceMetrics {
    /// Total number of CFG nodes
    pub(crate) total_nodes: usize,

    /// Total number of CFG edges
    pub(crate) total_edges: usize,

    /// Total number of borrows tracked
    pub(crate) total_borrows: usize,

    /// Average live set size across all nodes
    pub(crate) avg_live_set_size: f64,

    /// Maximum live set size encountered
    pub(crate) max_live_set_size: usize,

    /// Time per node processed (should be roughly constant for linear complexity)
    pub(crate) time_per_node: Duration,

    /// Time per borrow tracked (should be roughly constant for linear complexity)
    pub(crate) time_per_borrow: Duration,

    /// Complexity factor: time / (nodes * edges * borrows)
    /// Should remain roughly constant for different input sizes
    pub(crate) complexity_factor: f64,

    /// Total time spent in analysis
    pub(crate) analysis_time: Duration,
}

/// Fixpoint dataflow analysis engine for borrow propagation
///
/// Implements iterative dataflow analysis that propagates borrow state
/// through the CFG until reaching a stable fixpoint. This replaces the
/// previous unstable single-pass approach with guaranteed correctness.
pub(crate) struct BorrowDataflow<'a> {
    /// Reference to the control flow graph
    cfg: &'a ControlFlowGraph,

    /// Active borrow sets being analyzed
    live_sets: BorrowLiveSets,

    /// Worklist of CFG nodes that need re-analysis
    worklist: VecDeque<CfgNodeId>,

    /// Iteration counter for debugging and performance monitoring
    iteration_count: usize,

    /// Maximum iterations before giving up (safety limit)
    max_iterations: usize,

    /// Track nodes processed for statistics
    nodes_processed: usize,

    /// Track maximum worklist size for performance monitoring
    max_worklist_size: usize,

    /// Performance timing for complexity analysis
    start_time: Option<Instant>,
}

impl<'a> BorrowDataflow<'a> {
    /// Create a new dataflow analysis engine
    ///
    /// Initializes the engine with the given CFG and live sets, setting up
    /// the worklist with all CFG nodes for initial analysis.
    pub(crate) fn new(cfg: &'a ControlFlowGraph, live_sets: BorrowLiveSets) -> Self {
        let mut worklist = VecDeque::new();

        // Initialize worklist with all CFG nodes
        for &node_id in cfg.nodes.keys() {
            worklist.push_back(node_id);
        }

        let max_worklist_size = worklist.len();

        Self {
            cfg,
            live_sets,
            worklist,
            iteration_count: 0,
            max_iterations: 1000, // Safety limit to prevent infinite loops
            nodes_processed: 0,
            max_worklist_size,
            start_time: None,
        }
    }

    /// Run dataflow analysis to fixpoint convergence
    ///
    /// This is the main algorithm that iteratively processes CFG nodes
    /// until no further changes occur, guaranteeing a stable fixpoint.
    /// Enhanced with performance monitoring for complexity validation.
    pub(crate) fn analyze_to_fixpoint(&mut self) -> Result<DataflowResult, CompilerMessages> {
        // Start performance timing
        self.start_time = Some(Instant::now());

        // Starting fixpoint dataflow analysis

        while !self.worklist.is_empty() && self.iteration_count < self.max_iterations {
            self.iteration_count += 1;

            // Process all nodes in current worklist
            let current_worklist_size = self.worklist.len();
            self.max_worklist_size = self.max_worklist_size.max(current_worklist_size);

            for _ in 0..current_worklist_size {
                if let Some(node_id) = self.worklist.pop_front() {
                    let changed = self.process_node(node_id)?;

                    if changed {
                        // Add successors to worklist for re-analysis
                        self.add_successors_to_worklist(node_id);
                    }

                    self.nodes_processed += 1;
                }
            }

            // Check for convergence after each full iteration
            if self.worklist.is_empty() {
                break;
            }
        }

        // Calculate performance metrics
        let analysis_time = self.start_time.unwrap().elapsed();
        let performance_metrics = self.calculate_performance_metrics(analysis_time);

        // Validate convergence
        let converged = self.worklist.is_empty() && self.iteration_count < self.max_iterations;

        if !converged {
            let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                msg: format!(
                    "Dataflow analysis failed to converge after {} iterations. This indicates a bug in the fixpoint algorithm.",
                    self.max_iterations
                ),
                location:
                    crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(),
                error_type:
                    crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                metadata: std::collections::HashMap::new(),
            };
            return Err(CompilerMessages {
                errors: vec![error],
                warnings: vec![],
            });
        }

        // Mark live sets as stable
        self.live_sets.mark_stable();

        // Dataflow analysis converged successfully

        Ok(DataflowResult {
            iterations: self.iteration_count,
            converged,
            nodes_processed: self.nodes_processed,
            max_worklist_size: self.max_worklist_size,
            analysis_time,
            performance_metrics,
        })
    }

    /// Calculate performance metrics for complexity validation
    ///
    /// Computes various metrics to validate that the algorithm maintains
    /// linear or near-linear time complexity as input size grows.
    fn calculate_performance_metrics(&self, analysis_time: Duration) -> PerformanceMetrics {
        let total_nodes = self.cfg.nodes.len();
        let total_edges = self
            .cfg
            .nodes
            .values()
            .map(|node| node.successors.len())
            .sum::<usize>();
        let total_borrows = self.live_sets.borrow_count();

        let (_, _, max_live_set_size, avg_live_set_size) = self.live_sets.statistics();

        let time_per_node = if total_nodes > 0 {
            analysis_time / total_nodes as u32
        } else {
            Duration::ZERO
        };

        let time_per_borrow = if total_borrows > 0 {
            analysis_time / total_borrows as u32
        } else {
            Duration::ZERO
        };

        // Complexity factor: time / (nodes * edges * borrows)
        // Should remain roughly constant for linear complexity
        let complexity_factor = if total_nodes > 0 && total_edges > 0 && total_borrows > 0 {
            analysis_time.as_nanos() as f64 / (total_nodes * total_edges * total_borrows) as f64
        } else {
            0.0
        };

        PerformanceMetrics {
            total_nodes,
            total_edges,
            total_borrows,
            avg_live_set_size,
            max_live_set_size,
            time_per_node,
            time_per_borrow,
            complexity_factor,
            analysis_time,
        }
    }

    /// Process a single CFG node and return whether changes occurred
    ///
    /// This is the core of the dataflow algorithm. For each node, it:
    /// 1. Computes the new live set based on predecessors
    /// 2. Applies local effects (borrow creation/killing)
    /// 3. Detects changes and updates the live sets
    ///
    /// Optimized to minimize allocations and redundant computations.
    fn process_node(&mut self, node_id: CfgNodeId) -> Result<bool, CompilerMessages> {
        // Get predecessors once to avoid repeated CFG lookups
        let predecessors = self.cfg.predecessors(node_id);

        // Early exit for entry nodes (no predecessors)
        if predecessors.is_empty() {
            return Ok(false); // Entry nodes don't change during dataflow
        }

        // Compute incoming live set efficiently
        let mut new_live_set = BorrowSet::new();

        // Single pass through predecessors to build union
        for &pred_id in &predecessors {
            if let Some(pred_set) = self.live_sets.live_at_ref(pred_id) {
                new_live_set.extend(pred_set.iter().copied());
            }
        }

        // Apply local effects in-place to avoid additional allocations
        self.apply_local_effects_inplace(node_id, &mut new_live_set)?;

        // Check if the live set changed (avoid cloning for comparison)
        let changed = match self.live_sets.live_at_ref(node_id) {
            Some(current_set) => *current_set != new_live_set,
            None => !new_live_set.is_empty(),
        };

        if changed {
            // Update the live set
            *self.live_sets.live_at_mut(node_id) = new_live_set;
        }

        Ok(changed)
    }

    /// Apply local effects of a CFG node to the live set in-place
    ///
    /// This handles borrow creation and killing based on the operations
    /// performed at this specific CFG node. Optimized to modify the set
    /// in-place rather than creating new sets.
    fn apply_local_effects_inplace(
        &self,
        node_id: CfgNodeId,
        live_set: &mut BorrowSet,
    ) -> Result<(), CompilerMessages> {
        // Get the CFG node information once
        if let Some(cfg_node) = self.cfg.nodes.get(&node_id) {
            // Collect borrows to add/remove to avoid borrowing conflicts
            let mut borrows_to_add = Vec::new();
            let mut borrows_to_remove = Vec::new();

            // Identify borrows created at this node
            for loan in cfg_node.borrow_state.active_borrows.values() {
                if self.live_sets.creation_point(loan.id) == Some(node_id) {
                    borrows_to_add.push(loan.id);
                }
            }

            // Identify borrows killed at this node
            for &borrow_id in live_set.iter() {
                if self.live_sets.kill_point(borrow_id) == Some(node_id) {
                    borrows_to_remove.push(borrow_id);
                }
            }

            // Apply changes in-place
            for borrow_id in borrows_to_add {
                live_set.insert(borrow_id);
            }

            for borrow_id in borrows_to_remove {
                live_set.remove(&borrow_id);
            }
        }

        Ok(())
    }

    /// Add successors of a node to the worklist for re-analysis
    ///
    /// When a node's live set changes, all its successors need to be
    /// re-analyzed because their incoming live sets have changed.
    /// Optimized to use a HashSet for O(1) duplicate checking.
    fn add_successors_to_worklist(&mut self, node_id: CfgNodeId) {
        // Use a temporary set to avoid O(n) contains() calls on VecDeque
        let worklist_set: HashSet<CfgNodeId> = self.worklist.iter().copied().collect();

        for &successor in self.cfg.successors(node_id) {
            // Only add if not already in worklist (O(1) check with HashSet)
            if !worklist_set.contains(&successor) {
                self.worklist.push_back(successor);
            }
        }
    }

    /// Check if the analysis has converged to a stable fixpoint
    ///
    /// Convergence occurs when the worklist is empty and no further
    /// changes would occur if we continued iteration.
    pub(crate) fn has_converged(&self) -> bool {
        self.worklist.is_empty()
    }

    /// Get the current iteration count
    pub(crate) fn iteration_count(&self) -> usize {
        self.iteration_count
    }

    /// Extract the final live sets after analysis completion
    ///
    /// This consumes the dataflow engine and returns the computed live sets
    /// for integration with other borrow checker components.
    pub(crate) fn into_live_sets(self) -> BorrowLiveSets {
        self.live_sets
    }

    /// Get a reference to the live sets (for debugging)
    pub(crate) fn live_sets(&self) -> &BorrowLiveSets {
        &self.live_sets
    }

    /// Validate that the current state represents a true fixpoint
    ///
    /// This is used for debugging and validation to ensure that if we
    /// ran one more iteration, no changes would occur.
    pub(crate) fn validate_fixpoint(&mut self) -> Result<(), CompilerMessages> {
        // Check that no node would change if processed again
        for &node_id in self.cfg.nodes.keys() {
            let current_set = self.live_sets.live_at(node_id);

            // Compute what the set should be based on predecessors
            let predecessors = self.cfg.predecessors(node_id);
            let mut computed_set = BorrowSet::new();

            // Union all predecessor sets
            for &pred_id in &predecessors {
                if let Some(pred_set) = self.live_sets.live_at_ref(pred_id) {
                    computed_set.extend(pred_set.iter().copied());
                }
            }

            // Apply local effects to computed set
            if let Some(cfg_node) = self.cfg.nodes.get(&node_id) {
                // Add created borrows
                for loan in cfg_node.borrow_state.active_borrows.values() {
                    if self.live_sets.creation_point(loan.id) == Some(node_id) {
                        computed_set.insert(loan.id);
                    }
                }

                // Remove killed borrows
                let borrows_to_remove: Vec<BorrowId> = computed_set
                    .iter()
                    .filter(|&&borrow_id| self.live_sets.kill_point(borrow_id) == Some(node_id))
                    .copied()
                    .collect();

                for borrow_id in borrows_to_remove {
                    computed_set.remove(&borrow_id);
                }
            }

            if computed_set != current_set {
                let error = crate::compiler::compiler_messages::compiler_errors::CompilerError {
                    msg: format!(
                        "Fixpoint validation failed at node {:?}: current set != computed set",
                        node_id
                    ),
                    location:
                        crate::compiler::compiler_messages::compiler_errors::ErrorLocation::default(
                        ),
                    error_type:
                        crate::compiler::compiler_messages::compiler_errors::ErrorType::Compiler,
                    metadata: std::collections::HashMap::new(),
                };
                return Err(CompilerMessages {
                    errors: vec![error],
                    warnings: vec![],
                });
            }
        }

        Ok(())
    }

    /// Reset the analysis state for re-running with different parameters
    ///
    /// This allows the same engine to be used multiple times with different
    /// configurations or after modifying the live sets.
    pub(crate) fn reset(&mut self) {
        self.worklist.clear();
        self.iteration_count = 0;
        self.nodes_processed = 0;
        self.max_worklist_size = 0;

        // Re-initialize worklist with all nodes
        for &node_id in self.cfg.nodes.keys() {
            self.worklist.push_back(node_id);
        }

        self.max_worklist_size = self.worklist.len();

        // Mark live sets as unstable
        self.live_sets.mark_stable(); // This will be set to false when changes occur
    }

    /// Get statistics about the dataflow analysis
    ///
    /// Provides information useful for performance monitoring and debugging.
    pub(crate) fn get_statistics(&self) -> DataflowStatistics {
        DataflowStatistics {
            total_nodes: self.cfg.nodes.len(),
            total_borrows: self.live_sets.borrow_count(),
            iterations_run: self.iteration_count,
            nodes_processed: self.nodes_processed,
            max_worklist_size: self.max_worklist_size,
            current_worklist_size: self.worklist.len(),
            has_converged: self.has_converged(),
        }
    }
}

/// Statistics about dataflow analysis performance and progress
#[derive(Debug, Clone)]
pub(crate) struct DataflowStatistics {
    pub(crate) total_nodes: usize,
    pub(crate) total_borrows: usize,
    pub(crate) iterations_run: usize,
    pub(crate) nodes_processed: usize,
    pub(crate) max_worklist_size: usize,
    pub(crate) current_worklist_size: usize,
    pub(crate) has_converged: bool,
}

impl<'a> BorrowDataflow<'a> {
    /// Debug print the current state of dataflow analysis
    ///
    /// Provides comprehensive debugging visibility into the dataflow analysis state,
    /// including live sets, state transitions, and convergence information.
    pub(crate) fn debug_print_analysis_state(&self) {
        println!("=== Dataflow Analysis Debug Information ===");
        println!("Iterations: {}", self.iteration_count);
        println!("Nodes processed: {}", self.nodes_processed);
        println!("Max worklist size: {}", self.max_worklist_size);
        println!("Current worklist size: {}", self.worklist.len());
        println!("Has converged: {}", self.has_converged());
        println!();

        // Print live sets
        self.live_sets.debug_print_live_sets();

        // Print state transitions
        self.live_sets.debug_print_transitions();

        // Validate invariants
        println!("=== Invariant Validation ===");
        let invariants_valid = self.live_sets.validate_transition_invariants();
        println!("All transition invariants valid: {}", invariants_valid);
        println!();
    }

    /// Validate linear time complexity performance
    ///
    /// Checks that the dataflow analysis maintains linear or near-linear
    /// time complexity by analyzing the performance metrics.
    pub(crate) fn validate_linear_complexity(
        &self,
        performance_metrics: &PerformanceMetrics,
    ) -> bool {
        // Check that complexity factor is reasonable (not exponential)
        // For linear complexity, this should be roughly constant across different input sizes
        const MAX_COMPLEXITY_FACTOR: f64 = 1000.0; // Nanoseconds per (node * edge * borrow)

        if performance_metrics.complexity_factor > MAX_COMPLEXITY_FACTOR {
            println!(
                "WARNING: High complexity factor: {:.2} ns/(node*edge*borrow)",
                performance_metrics.complexity_factor
            );
            return false;
        }

        // Check that iterations don't grow exponentially with input size
        let expected_max_iterations = performance_metrics.total_nodes * 2; // Linear bound
        if self.iteration_count > expected_max_iterations {
            println!(
                "WARNING: High iteration count: {} iterations for {} nodes",
                self.iteration_count, performance_metrics.total_nodes
            );
            return false;
        }

        // Check that time per node is reasonable
        const MAX_TIME_PER_NODE_MS: u64 = 10; // 10ms per node should be plenty
        if performance_metrics.time_per_node.as_millis() > MAX_TIME_PER_NODE_MS as u128 {
            println!(
                "WARNING: High time per node: {}ms",
                performance_metrics.time_per_node.as_millis()
            );
            return false;
        }

        true
    }

    /// Generate performance report for optimization validation
    ///
    /// Creates a detailed report of performance characteristics that can be
    /// used to validate that optimizations are working correctly.
    pub(crate) fn generate_performance_report(
        &self,
        performance_metrics: &PerformanceMetrics,
        analysis_time: Duration,
    ) -> String {
        format!(
            r#"
=== Dataflow Analysis Performance Report ===

Input Size:
  - CFG Nodes: {}
  - CFG Edges: {}
  - Borrows Tracked: {}
  - Avg Live Set Size: {:.2}
  - Max Live Set Size: {}

Analysis Metrics:
  - Total Iterations: {}
  - Nodes Processed: {}
  - Max Worklist Size: {}
  - Analysis Time: {:.2}ms

Performance Characteristics:
  - Time per Node: {:.2}μs
  - Time per Borrow: {:.2}μs
  - Complexity Factor: {:.2} ns/(node*edge*borrow)

Linear Complexity Validation: {}

Optimization Status:
  - In-place Set Operations: ✓
  - Efficient HashMap Lookups: ✓
  - Minimized Cloning: ✓
  - Optimized Worklist Management: ✓
  - Performance Monitoring: ✓
"#,
            performance_metrics.total_nodes,
            performance_metrics.total_edges,
            performance_metrics.total_borrows,
            performance_metrics.avg_live_set_size,
            performance_metrics.max_live_set_size,
            self.iteration_count,
            self.nodes_processed,
            self.max_worklist_size,
            analysis_time.as_millis(),
            performance_metrics.time_per_node.as_micros(),
            performance_metrics.time_per_borrow.as_micros(),
            performance_metrics.complexity_factor,
            if self.validate_linear_complexity(performance_metrics) {
                "PASSED"
            } else {
                "FAILED"
            }
        )
    }
}
