use crate::compiler::mir::extract::{BitSet, BorrowFactExtractor};
use crate::compiler::mir::mir_nodes::{MirFunction, ProgramPoint};
use std::collections::{HashMap, VecDeque};

/// Forward loan-liveness dataflow analysis with worklist algorithm
///
/// This module implements forward dataflow analysis to track which loans are live
/// at each program point. Uses efficient bitsets and worklist algorithm for
/// scalable analysis of WASM-optimized MIR.
///
/// ## Algorithm Overview
///
/// Uses standard forward dataflow equations:
/// ```
/// LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s])
/// LiveOutLoans[s] = ⋃ LiveInLoans[succ(s)]
/// ```
///
/// ## Gen/Kill Set Construction
///
/// - **Gen[s]**: Loans starting at statement s (from `events[s].start_loans`)
/// - **Kill[s]**: Loans whose owners may alias places moved/reassigned at s
///
/// ## Bitset Efficiency
///
/// Loans are represented as bitsets for fast set operations:
/// - **Union**: `LiveOut = ⋃ LiveIn[successors]` using bitwise OR
/// - **Difference**: `LiveOut - Kill` using bitwise AND NOT
/// - **Iteration**: Efficient bit scanning for conflict detection
///
/// ## Performance Characteristics
///
/// - **Time Complexity**: O(n × l × i) where n=program points, l=loans, i=iterations
/// - **Space Complexity**: O(n × l) for bitsets (compact representation)
/// - **Scalability**: Linear growth with function size and loan count
///
/// ## Example
///
/// ```rust
/// // Input MIR with loans:
/// PP0: x = 42
/// PP1: a = &x        // Start loan_0
/// PP2: b = &x        // Start loan_1  
/// PP3: use(a)        // Use loan_0
/// PP4: move x        // Kill both loans
///
/// // Dataflow results:
/// PP1: LiveOut = {loan_0}
/// PP2: LiveIn = {loan_0}, LiveOut = {loan_0, loan_1}
/// PP3: LiveIn = {loan_0, loan_1}, LiveOut = {loan_0, loan_1}
/// PP4: LiveIn = {loan_0, loan_1}, LiveOut = {} // Killed by move
/// ```
#[derive(Debug)]
pub struct LoanLivenessDataflow {
    /// Live loans entering each program point
    pub live_in_loans: HashMap<ProgramPoint, BitSet>,
    /// Live loans exiting each program point
    pub live_out_loans: HashMap<ProgramPoint, BitSet>,
    /// Gen sets: loans starting at each program point
    pub gen_sets: HashMap<ProgramPoint, BitSet>,
    /// Kill sets: loans killed at each program point
    pub kill_sets: HashMap<ProgramPoint, BitSet>,
    /// Control flow graph: program point -> successors
    pub successors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
    /// Control flow graph: program point -> predecessors
    pub predecessors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
    /// Total number of loans (for bitset sizing)
    pub loan_count: usize,
}

impl LoanLivenessDataflow {
    /// Create a new loan liveness dataflow analysis
    pub fn new(loan_count: usize) -> Self {
        Self {
            live_in_loans: HashMap::new(),
            live_out_loans: HashMap::new(),
            gen_sets: HashMap::new(),
            kill_sets: HashMap::new(),
            successors: HashMap::new(),
            predecessors: HashMap::new(),
            loan_count,
        }
    }

    /// Run forward loan-liveness dataflow analysis on a function with place interning optimization
    pub fn analyze_function(
        &mut self,
        function: &MirFunction,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), String> {
        // Use the shared CFG from the function instead of building our own
        self.copy_cfg_from_function(function)?;

        // Copy gen/kill sets from the extractor (now optimized with place interning)
        self.copy_gen_kill_sets(function, extractor)?;

        // Run forward dataflow analysis with worklist algorithm
        self.run_forward_dataflow(function)?;

        Ok(())
    }

    /// Copy CFG from the shared function CFG (eliminates redundant construction)
    fn copy_cfg_from_function(&mut self, function: &MirFunction) -> Result<(), String> {
        let cfg = function.get_cfg_immutable()?;

        // Clear existing CFG data
        self.successors.clear();
        self.predecessors.clear();

        // Copy CFG structure using optimized Vec-indexed access
        for point in cfg.iter_program_points() {
            let successors = cfg.get_successors(&point).to_vec();
            let predecessors = cfg.get_predecessors(&point).to_vec();

            self.successors.insert(point, successors);
            self.predecessors.insert(point, predecessors);
        }

        Ok(())
    }

    /// Copy gen/kill sets from the borrow fact extractor (optimized)
    fn copy_gen_kill_sets(
        &mut self,
        function: &MirFunction,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), String> {
        // Pre-allocate empty BitSet for reuse
        let empty_bitset = BitSet::new(self.loan_count);

        // Copy gen and kill sets for all program points using optimized operations
        for program_point in function.get_program_points_in_order() {
            // Copy gen set using optimized copy_from when available
            if let Some(gen_set) = extractor.get_gen_set(&program_point) {
                self.gen_sets.insert(program_point, gen_set.clone());
            } else {
                self.gen_sets.insert(program_point, empty_bitset.clone());
            }

            // Copy kill set using optimized copy_from when available
            if let Some(kill_set) = extractor.get_kill_set(&program_point) {
                self.kill_sets.insert(program_point, kill_set.clone());
            } else {
                self.kill_sets.insert(program_point, empty_bitset.clone());
            }
        }

        Ok(())
    }

    /// Run forward dataflow analysis using optimized worklist algorithm
    ///
    /// Implements the equations:
    /// - LiveOutLoans[s] = ⋃ LiveInLoans[succ(s)]
    /// - LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s])
    ///
    /// Optimizations:
    /// - Pre-allocated temporary BitSets to avoid allocations in hot loop
    /// - Fast-path checks for empty sets and single bits
    /// - In-place operations to minimize copying
    /// - Efficient worklist management with duplicate detection
    fn run_forward_dataflow(&mut self, function: &MirFunction) -> Result<(), String> {
        let program_points = function.get_program_points_in_order();

        // Pre-allocate all BitSets to avoid allocations during analysis
        for point in &program_points {
            self.live_in_loans
                .insert(*point, BitSet::new(self.loan_count));
            self.live_out_loans
                .insert(*point, BitSet::new(self.loan_count));
        }

        // Pre-allocate temporary BitSets for hot loop operations
        let mut temp_live_out = BitSet::new(self.loan_count);
        let mut temp_live_in = BitSet::new(self.loan_count);

        // Worklist algorithm with optimized duplicate detection
        let mut worklist: VecDeque<ProgramPoint> = program_points.iter().copied().collect();
        let mut in_worklist: std::collections::HashSet<ProgramPoint> =
            program_points.iter().copied().collect();
        let mut iteration_count = 0;
        const MAX_ITERATIONS: usize = 10000; // Prevent infinite loops

        while let Some(current_point) = worklist.pop_front() {
            in_worklist.remove(&current_point);

            iteration_count += 1;
            if iteration_count > MAX_ITERATIONS {
                return Err(format!(
                    "Loan liveness dataflow failed to converge after {} iterations",
                    MAX_ITERATIONS
                ));
            }

            // Get current sets (avoid cloning by using references)
            let gen_set = self
                .gen_sets
                .get(&current_point)
                .ok_or_else(|| format!("Missing gen set for program point {}", current_point))?;
            let kill_set = self
                .kill_sets
                .get(&current_point)
                .ok_or_else(|| format!("Missing kill set for program point {}", current_point))?;

            // Compute LiveOutLoans[s] = ⋃ LiveInLoans[succ(s)] using pre-allocated temp
            temp_live_out.clear_all();
            if let Some(successors) = self.successors.get(&current_point) {
                // Fast path: single successor (common case)
                if successors.len() == 1 {
                    if let Some(succ_live_in) = self.live_in_loans.get(&successors[0]) {
                        temp_live_out.copy_from(succ_live_in);
                    }
                } else {
                    // Multiple successors: union all
                    for &successor in successors {
                        if let Some(succ_live_in) = self.live_in_loans.get(&successor) {
                            temp_live_out.union_with(succ_live_in);
                        }
                    }
                }
            }

            // Check if LiveOutLoans changed using fast comparison
            let current_live_out = self.live_out_loans.get_mut(&current_point).unwrap();
            let live_out_changed = *current_live_out != temp_live_out;

            if live_out_changed {
                // Update LiveOutLoans in-place
                current_live_out.copy_from(&temp_live_out);

                // Compute LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s]) using pre-allocated temp
                temp_live_in.copy_from(&temp_live_out);
                temp_live_in.subtract(kill_set); // LiveOutLoans[s] - Kill[s]
                temp_live_in.union_with(gen_set); // Gen[s] ∪ (LiveOutLoans[s] - Kill[s])

                // Check if LiveInLoans changed using fast comparison
                let current_live_in = self.live_in_loans.get_mut(&current_point).unwrap();
                let live_in_changed = *current_live_in != temp_live_in;

                if live_in_changed {
                    // Update LiveInLoans in-place
                    current_live_in.copy_from(&temp_live_in);

                    // Add predecessors to worklist with efficient duplicate detection
                    if let Some(predecessors) = self.predecessors.get(&current_point) {
                        for &pred in predecessors {
                            if !in_worklist.contains(&pred) {
                                worklist.push_back(pred);
                                in_worklist.insert(pred);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get live loans entering a program point
    pub fn get_live_in_loans(&self, point: &ProgramPoint) -> Option<&BitSet> {
        self.live_in_loans.get(point)
    }

    /// Get live loans exiting a program point
    pub fn get_live_out_loans(&self, point: &ProgramPoint) -> Option<&BitSet> {
        self.live_out_loans.get(point)
    }

    /// Check if a specific loan is live at a program point
    pub fn is_loan_live_at(&self, loan_index: usize, point: &ProgramPoint) -> bool {
        self.live_in_loans
            .get(point)
            .map(|live_set| live_set.get(loan_index))
            .unwrap_or(false)
    }

    /// Check if a specific loan is live after a program point
    pub fn is_loan_live_after(&self, loan_index: usize, point: &ProgramPoint) -> bool {
        self.live_out_loans
            .get(point)
            .map(|live_set| live_set.get(loan_index))
            .unwrap_or(false)
    }

    /// Get all live loan indices at a program point (optimized to avoid allocation)
    pub fn for_each_live_loan_at<F>(&self, point: &ProgramPoint, mut f: F)
    where
        F: FnMut(usize),
    {
        if let Some(live_set) = self.live_in_loans.get(point) {
            live_set.for_each_set_bit(f);
        }
    }

    /// Get all live loan indices after a program point (optimized to avoid allocation)
    pub fn for_each_live_loan_after<F>(&self, point: &ProgramPoint, mut f: F)
    where
        F: FnMut(usize),
    {
        if let Some(live_set) = self.live_out_loans.get(point) {
            live_set.for_each_set_bit(f);
        }
    }

    /// Get all live loan indices at a program point (compatibility method - prefer for_each_live_loan_at)
    pub fn get_live_loan_indices_at(&self, point: &ProgramPoint) -> Vec<usize> {
        let mut indices = Vec::new();
        self.for_each_live_loan_at(point, |idx| indices.push(idx));
        indices
    }

    /// Get all live loan indices after a program point (compatibility method - prefer for_each_live_loan_after)
    pub fn get_live_loan_indices_after(&self, point: &ProgramPoint) -> Vec<usize> {
        let mut indices = Vec::new();
        self.for_each_live_loan_after(point, |idx| indices.push(idx));
        indices
    }

    /// Get statistics about the dataflow analysis
    pub fn get_statistics(&self) -> DataflowStatistics {
        let total_program_points = self.live_in_loans.len();

        // Calculate average live loans per program point
        let total_live_loans: usize = self
            .live_in_loans
            .values()
            .map(|set| set.count_ones())
            .sum();
        let avg_live_loans_per_point = if total_program_points > 0 {
            total_live_loans as f64 / total_program_points as f64
        } else {
            0.0
        };

        DataflowStatistics {
            total_program_points,
            total_loans: self.loan_count,
            avg_live_loans_per_point,
        }
    }

    /// Handle control flow merges correctly (optimized)
    ///
    /// This method ensures that when multiple control flow paths merge at a program point,
    /// the live loan sets are computed correctly by taking the union of all incoming paths.
    /// Uses optimized BitSet operations to minimize allocations.
    pub fn handle_control_flow_merge(&mut self, merge_point: &ProgramPoint) -> Result<(), String> {
        // Get all predecessors of the merge point
        let predecessors = self
            .predecessors
            .get(merge_point)
            .ok_or_else(|| format!("No predecessors found for merge point {}", merge_point))?
            .clone();

        if predecessors.len() <= 1 {
            // Not actually a merge point, nothing special to do
            return Ok(());
        }

        // Compute union of live-out sets from all predecessors using optimized operations
        let mut merged_live_in = BitSet::new(self.loan_count);

        // Fast path: if only two predecessors, use direct copy + union
        if predecessors.len() == 2 {
            if let Some(pred1_live_out) = self.live_out_loans.get(&predecessors[0]) {
                merged_live_in.copy_from(pred1_live_out);
            }
            if let Some(pred2_live_out) = self.live_out_loans.get(&predecessors[1]) {
                merged_live_in.union_with(pred2_live_out);
            }
        } else {
            // Multiple predecessors: union all using optimized operations
            for &pred in &predecessors {
                if let Some(pred_live_out) = self.live_out_loans.get(&pred) {
                    merged_live_in.union_with(pred_live_out);
                }
            }
        }

        // Update the live-in set for the merge point
        self.live_in_loans.insert(*merge_point, merged_live_in);

        Ok(())
    }

    /// Handle control flow branches correctly (optimized)
    ///
    /// This method ensures that when control flow branches from a program point,
    /// the live loan information is propagated correctly to all branch targets.
    /// Uses optimized BitSet operations to minimize allocations.
    pub fn handle_control_flow_branch(
        &mut self,
        branch_point: &ProgramPoint,
    ) -> Result<(), String> {
        // Get all successors of the branch point
        let successors = self
            .successors
            .get(branch_point)
            .ok_or_else(|| format!("No successors found for branch point {}", branch_point))?
            .clone();

        if successors.len() <= 1 {
            // Not actually a branch point, nothing special to do
            return Ok(());
        }

        // Get the live-out set from the branch point
        let branch_live_out = self
            .live_out_loans
            .get(branch_point)
            .ok_or_else(|| format!("No live-out set found for branch point {}", branch_point))?;

        // Propagate to all successors using optimized copy operations
        for &succ in &successors {
            // The live-in for each successor starts with the branch point's live-out
            // This will be refined by the dataflow equations during analysis
            if !self.live_in_loans.contains_key(&succ) {
                let mut succ_live_in = BitSet::new(self.loan_count);
                succ_live_in.copy_from(branch_live_out);
                self.live_in_loans.insert(succ, succ_live_in);
            }
        }

        Ok(())
    }

    /// Validate dataflow results for consistency
    pub fn validate_results(&self) -> Result<(), String> {
        // Check that all program points have both live-in and live-out sets
        for (point, _) in &self.live_in_loans {
            if !self.live_out_loans.contains_key(point) {
                return Err(format!(
                    "Program point {} has live-in but no live-out set",
                    point
                ));
            }
        }

        for (point, _) in &self.live_out_loans {
            if !self.live_in_loans.contains_key(point) {
                return Err(format!(
                    "Program point {} has live-out but no live-in set",
                    point
                ));
            }
        }

        // Check that bitset sizes are consistent
        for (point, live_in) in &self.live_in_loans {
            if live_in.capacity() != self.loan_count {
                return Err(format!(
                    "Program point {} live-in set has capacity {} but expected {}",
                    point,
                    live_in.capacity(),
                    self.loan_count
                ));
            }
        }

        for (point, live_out) in &self.live_out_loans {
            if live_out.capacity() != self.loan_count {
                return Err(format!(
                    "Program point {} live-out set has capacity {} but expected {}",
                    point,
                    live_out.capacity(),
                    self.loan_count
                ));
            }
        }

        Ok(())
    }
}

/// Statistics about dataflow analysis results
#[derive(Debug, Clone)]
pub struct DataflowStatistics {
    /// Total number of program points analyzed
    pub total_program_points: usize,
    /// Total number of loans in the function
    pub total_loans: usize,

    /// Average number of live loans per program point
    pub avg_live_loans_per_point: f64,
}

/// Entry point for running loan-liveness dataflow analysis with place interning optimization
pub fn run_loan_liveness_dataflow(
    function: &MirFunction,
    extractor: &BorrowFactExtractor,
) -> Result<LoanLivenessDataflow, String> {
    let loan_count = extractor.get_loan_count();
    let mut dataflow = LoanLivenessDataflow::new(loan_count);
    dataflow.analyze_function(function, extractor)?;
    dataflow.validate_results()?;
    Ok(dataflow)
}
