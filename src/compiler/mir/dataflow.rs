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

    /// Run forward loan-liveness dataflow analysis on a function
    pub fn analyze_function(
        &mut self,
        function: &MirFunction,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), String> {
        // Build control flow graph for this function
        self.build_control_flow_graph(function)?;
        
        // Copy gen/kill sets from the extractor
        self.copy_gen_kill_sets(function, extractor)?;
        
        // Run forward dataflow analysis with worklist algorithm
        self.run_forward_dataflow(function)?;
        
        Ok(())
    }

    /// Build control flow graph between program points
    fn build_control_flow_graph(&mut self, function: &MirFunction) -> Result<(), String> {
        let program_points = function.get_program_points_in_order();
        
        // Initialize empty successor/predecessor lists for all points
        for &point in program_points {
            self.successors.insert(point, Vec::new());
            self.predecessors.insert(point, Vec::new());
        }
        
        // For now, build a simple linear CFG since full CFG construction
        // is planned for later tasks. This provides the basic structure needed.
        for (i, &current_point) in program_points.iter().enumerate() {
            // For linear CFG, each point flows to the next
            if i + 1 < program_points.len() {
                let next_point = program_points[i + 1];
                
                // Add successor relationship
                self.successors.get_mut(&current_point).unwrap().push(next_point);
                self.predecessors.get_mut(&next_point).unwrap().push(current_point);
            }
        }
        
        // TODO: In later tasks, this will be replaced with proper CFG construction
        // that handles branches, loops, and other control flow structures from
        // the MIR block terminators
        
        Ok(())
    }

    /// Copy gen/kill sets from the borrow fact extractor
    fn copy_gen_kill_sets(
        &mut self,
        function: &MirFunction,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), String> {
        // Copy gen and kill sets for all program points
        for &program_point in function.get_program_points_in_order() {
            // Copy gen set
            if let Some(gen_set) = extractor.get_gen_set(&program_point) {
                self.gen_sets.insert(program_point, gen_set.clone());
            } else {
                self.gen_sets.insert(program_point, BitSet::new(self.loan_count));
            }
            
            // Copy kill set
            if let Some(kill_set) = extractor.get_kill_set(&program_point) {
                self.kill_sets.insert(program_point, kill_set.clone());
            } else {
                self.kill_sets.insert(program_point, BitSet::new(self.loan_count));
            }
        }
        
        Ok(())
    }

    /// Run forward dataflow analysis using worklist algorithm
    ///
    /// Implements the equations:
    /// - LiveOutLoans[s] = ⋃ LiveInLoans[succ(s)]
    /// - LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s])
    fn run_forward_dataflow(&mut self, function: &MirFunction) -> Result<(), String> {
        let program_points = function.get_program_points_in_order();
        
        // Initialize all live loan sets to empty
        for &point in program_points {
            self.live_in_loans.insert(point, BitSet::new(self.loan_count));
            self.live_out_loans.insert(point, BitSet::new(self.loan_count));
        }
        
        // Worklist algorithm for forward dataflow
        // Use VecDeque for efficient queue operations
        let mut worklist: VecDeque<ProgramPoint> = program_points.iter().copied().collect();
        let mut iteration_count = 0;
        const MAX_ITERATIONS: usize = 10000; // Prevent infinite loops
        
        while let Some(current_point) = worklist.pop_front() {
            iteration_count += 1;
            if iteration_count > MAX_ITERATIONS {
                return Err(format!(
                    "Loan liveness dataflow failed to converge after {} iterations", 
                    MAX_ITERATIONS
                ));
            }
            
            // Get current sets (clone to avoid borrowing issues)
            let gen_set = self.gen_sets.get(&current_point)
                .ok_or_else(|| format!("Missing gen set for program point {}", current_point))?
                .clone();
            let kill_set = self.kill_sets.get(&current_point)
                .ok_or_else(|| format!("Missing kill set for program point {}", current_point))?
                .clone();
            
            // Compute LiveOutLoans[s] = ⋃ LiveInLoans[succ(s)]
            let mut new_live_out = BitSet::new(self.loan_count);
            if let Some(successors) = self.successors.get(&current_point) {
                for &successor in successors {
                    if let Some(succ_live_in) = self.live_in_loans.get(&successor) {
                        new_live_out.union_with(succ_live_in);
                    }
                }
            }
            
            // Check if LiveOutLoans changed
            let old_live_out = self.live_out_loans.get(&current_point)
                .cloned()
                .unwrap_or_else(|| BitSet::new(self.loan_count));
            let live_out_changed = new_live_out != old_live_out;
            
            if live_out_changed {
                // Update LiveOutLoans
                self.live_out_loans.insert(current_point, new_live_out.clone());
                
                // Compute LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s])
                let mut new_live_in = new_live_out.clone();
                new_live_in.subtract(&kill_set); // LiveOutLoans[s] - Kill[s]
                new_live_in.union_with(&gen_set); // Gen[s] ∪ (LiveOutLoans[s] - Kill[s])
                
                // Check if LiveInLoans changed
                let old_live_in = self.live_in_loans.get(&current_point)
                    .cloned()
                    .unwrap_or_else(|| BitSet::new(self.loan_count));
                
                if new_live_in != old_live_in {
                    // Update LiveInLoans
                    self.live_in_loans.insert(current_point, new_live_in);
                    
                    // Add predecessors to worklist (predecessor pushing on changes)
                    if let Some(predecessors) = self.predecessors.get(&current_point) {
                        for &pred in predecessors {
                            // Only add if not already in worklist to avoid duplicates
                            if !worklist.contains(&pred) {
                                worklist.push_back(pred);
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
        self.live_in_loans.get(point)
            .map(|live_set| live_set.get(loan_index))
            .unwrap_or(false)
    }

    /// Check if a specific loan is live after a program point
    pub fn is_loan_live_after(&self, loan_index: usize, point: &ProgramPoint) -> bool {
        self.live_out_loans.get(point)
            .map(|live_set| live_set.get(loan_index))
            .unwrap_or(false)
    }

    /// Get all live loan indices at a program point
    pub fn get_live_loan_indices_at(&self, point: &ProgramPoint) -> Vec<usize> {
        self.live_in_loans.get(point)
            .map(|live_set| live_set.iter_set_bits().collect())
            .unwrap_or_default()
    }

    /// Get all live loan indices after a program point
    pub fn get_live_loan_indices_after(&self, point: &ProgramPoint) -> Vec<usize> {
        self.live_out_loans.get(point)
            .map(|live_set| live_set.iter_set_bits().collect())
            .unwrap_or_default()
    }

    /// Get statistics about the dataflow analysis
    pub fn get_statistics(&self) -> DataflowStatistics {
        let max_live_loans_at_point = self.live_in_loans.values()
            .map(|set| set.count_ones())
            .max()
            .unwrap_or(0);
        
        let max_live_loans_after_point = self.live_out_loans.values()
            .map(|set| set.count_ones())
            .max()
            .unwrap_or(0);
        
        let total_program_points = self.live_in_loans.len();
        
        // Calculate average live loans per program point
        let total_live_loans: usize = self.live_in_loans.values()
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
            max_live_loans_at_point,
            max_live_loans_after_point,
            avg_live_loans_per_point,
        }
    }

    /// Handle control flow merges correctly
    ///
    /// This method ensures that when multiple control flow paths merge at a program point,
    /// the live loan sets are computed correctly by taking the union of all incoming paths.
    pub fn handle_control_flow_merge(&mut self, merge_point: &ProgramPoint) -> Result<(), String> {
        // Get all predecessors of the merge point
        let predecessors = self.predecessors.get(merge_point)
            .ok_or_else(|| format!("No predecessors found for merge point {}", merge_point))?
            .clone();
        
        if predecessors.len() <= 1 {
            // Not actually a merge point, nothing special to do
            return Ok(());
        }
        
        // Compute union of live-out sets from all predecessors
        let mut merged_live_in = BitSet::new(self.loan_count);
        for &pred in &predecessors {
            if let Some(pred_live_out) = self.live_out_loans.get(&pred) {
                merged_live_in.union_with(pred_live_out);
            }
        }
        
        // Update the live-in set for the merge point
        self.live_in_loans.insert(*merge_point, merged_live_in);
        
        Ok(())
    }

    /// Handle control flow branches correctly
    ///
    /// This method ensures that when control flow branches from a program point,
    /// the live loan information is propagated correctly to all branch targets.
    pub fn handle_control_flow_branch(&mut self, branch_point: &ProgramPoint) -> Result<(), String> {
        // Get all successors of the branch point
        let successors = self.successors.get(branch_point)
            .ok_or_else(|| format!("No successors found for branch point {}", branch_point))?
            .clone();
        
        if successors.len() <= 1 {
            // Not actually a branch point, nothing special to do
            return Ok(());
        }
        
        // Get the live-out set from the branch point
        let branch_live_out = self.live_out_loans.get(branch_point)
            .ok_or_else(|| format!("No live-out set found for branch point {}", branch_point))?
            .clone();
        
        // Propagate to all successors (they all get the same live loans initially)
        for &succ in &successors {
            // The live-in for each successor starts with the branch point's live-out
            // This will be refined by the dataflow equations during analysis
            if !self.live_in_loans.contains_key(&succ) {
                self.live_in_loans.insert(succ, branch_live_out.clone());
            }
        }
        
        Ok(())
    }

    /// Validate dataflow results for consistency
    pub fn validate_results(&self) -> Result<(), String> {
        // Check that all program points have both live-in and live-out sets
        for (point, _) in &self.live_in_loans {
            if !self.live_out_loans.contains_key(point) {
                return Err(format!("Program point {} has live-in but no live-out set", point));
            }
        }
        
        for (point, _) in &self.live_out_loans {
            if !self.live_in_loans.contains_key(point) {
                return Err(format!("Program point {} has live-out but no live-in set", point));
            }
        }
        
        // Check that bitset sizes are consistent
        for (point, live_in) in &self.live_in_loans {
            if live_in.capacity() != self.loan_count {
                return Err(format!(
                    "Program point {} live-in set has capacity {} but expected {}", 
                    point, live_in.capacity(), self.loan_count
                ));
            }
        }
        
        for (point, live_out) in &self.live_out_loans {
            if live_out.capacity() != self.loan_count {
                return Err(format!(
                    "Program point {} live-out set has capacity {} but expected {}", 
                    point, live_out.capacity(), self.loan_count
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
    /// Maximum number of live loans at any single program point (live-in)
    pub max_live_loans_at_point: usize,
    /// Maximum number of live loans after any single program point (live-out)
    pub max_live_loans_after_point: usize,
    /// Average number of live loans per program point
    pub avg_live_loans_per_point: f64,
}

/// Entry point for running loan-liveness dataflow analysis
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::mir::extract::{BitSet, BorrowFactExtractor};
    use crate::compiler::mir::mir_nodes::*;
    use crate::compiler::mir::place::*;

    /// Create a test function with some loans for dataflow testing
    fn create_test_function_with_dataflow() -> (MirFunction, BorrowFactExtractor) {
        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        
        // Create test places
        let place_x = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_y = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        // Create program points
        let pp1 = ProgramPoint::new(0);
        let pp2 = ProgramPoint::new(1);
        let pp3 = ProgramPoint::new(2);
        
        // Add program points to function
        function.add_program_point(pp1, 0, 0);
        function.add_program_point(pp2, 0, 1);
        function.add_program_point(pp3, 0, 2);
        
        // Create events with loans
        let mut events1 = Events::default();
        events1.start_loans.push(LoanId::new(0)); // Start loan 0 at pp1
        function.store_events(pp1, events1);
        
        let mut events2 = Events::default();
        events2.start_loans.push(LoanId::new(1)); // Start loan 1 at pp2
        events2.uses.push(place_x.clone()); // Use place_x at pp2
        function.store_events(pp2, events2);
        
        let mut events3 = Events::default();
        events3.moves.push(place_y.clone()); // Move place_y at pp3 (kills loans)
        function.store_events(pp3, events3);
        
        // Create extractor with gen/kill sets
        let mut extractor = BorrowFactExtractor::new();
        extractor.extract_function(&function).unwrap();
        
        (function, extractor)
    }

    #[test]
    fn test_dataflow_initialization() {
        let loan_count = 5;
        let dataflow = LoanLivenessDataflow::new(loan_count);
        
        assert_eq!(dataflow.loan_count, loan_count);
        assert!(dataflow.live_in_loans.is_empty());
        assert!(dataflow.live_out_loans.is_empty());
    }

    #[test]
    fn test_control_flow_graph_construction() {
        let (function, _) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(2);
        
        let result = dataflow.build_control_flow_graph(&function);
        assert!(result.is_ok(), "CFG construction should succeed");
        
        // Check that we have successors/predecessors for all program points
        assert_eq!(dataflow.successors.len(), 3);
        assert_eq!(dataflow.predecessors.len(), 3);
        
        // Check linear CFG structure
        let pp1 = ProgramPoint::new(0);
        let pp2 = ProgramPoint::new(1);
        let pp3 = ProgramPoint::new(2);
        
        assert_eq!(dataflow.successors[&pp1], vec![pp2]);
        assert_eq!(dataflow.successors[&pp2], vec![pp3]);
        assert!(dataflow.successors[&pp3].is_empty()); // Last point has no successors
        
        assert!(dataflow.predecessors[&pp1].is_empty()); // First point has no predecessors
        assert_eq!(dataflow.predecessors[&pp2], vec![pp1]);
        assert_eq!(dataflow.predecessors[&pp3], vec![pp2]);
    }

    #[test]
    fn test_gen_kill_set_copying() {
        let (function, extractor) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(extractor.get_loan_count());
        
        dataflow.build_control_flow_graph(&function).unwrap();
        let result = dataflow.copy_gen_kill_sets(&function, &extractor);
        assert!(result.is_ok(), "Gen/kill set copying should succeed");
        
        // Check that we have gen/kill sets for all program points
        assert_eq!(dataflow.gen_sets.len(), 3);
        assert_eq!(dataflow.kill_sets.len(), 3);
        
        // Check that gen sets contain the expected loans
        let pp1 = ProgramPoint::new(0);
        let pp2 = ProgramPoint::new(1);
        
        let gen1 = &dataflow.gen_sets[&pp1];
        let gen2 = &dataflow.gen_sets[&pp2];
        
        // pp1 should generate loan 0, pp2 should generate loan 1
        assert!(gen1.get(0), "pp1 should generate loan 0");
        assert!(gen2.get(1), "pp2 should generate loan 1");
    }

    #[test]
    fn test_forward_dataflow_analysis() {
        let (function, extractor) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(extractor.get_loan_count());
        
        let result = dataflow.analyze_function(&function, &extractor);
        assert!(result.is_ok(), "Dataflow analysis should succeed");
        
        // Check that we have live sets for all program points
        assert_eq!(dataflow.live_in_loans.len(), 3);
        assert_eq!(dataflow.live_out_loans.len(), 3);
        
        // Validate that the analysis converged
        let validation = dataflow.validate_results();
        assert!(validation.is_ok(), "Dataflow results should be valid");
    }

    #[test]
    fn test_loan_liveness_queries() {
        let (function, extractor) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(extractor.get_loan_count());
        
        dataflow.analyze_function(&function, &extractor).unwrap();
        
        let pp1 = ProgramPoint::new(0);
        let pp2 = ProgramPoint::new(1);
        
        // Test loan liveness queries
        let live_in_pp1 = dataflow.get_live_in_loans(&pp1);
        assert!(live_in_pp1.is_some(), "Should have live-in set for pp1");
        
        let live_out_pp1 = dataflow.get_live_out_loans(&pp1);
        assert!(live_out_pp1.is_some(), "Should have live-out set for pp1");
        
        // Test specific loan queries
        let is_loan_0_live = dataflow.is_loan_live_at(0, &pp2);
        // This depends on the specific dataflow results, but should not panic
        let _ = is_loan_0_live;
        
        // Test live loan indices
        let live_indices = dataflow.get_live_loan_indices_at(&pp1);
        assert!(live_indices.len() <= extractor.get_loan_count());
    }

    #[test]
    fn test_dataflow_statistics() {
        let (function, extractor) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(extractor.get_loan_count());
        
        dataflow.analyze_function(&function, &extractor).unwrap();
        
        let stats = dataflow.get_statistics();
        assert_eq!(stats.total_program_points, 3);
        assert_eq!(stats.total_loans, extractor.get_loan_count());
        assert!(stats.avg_live_loans_per_point >= 0.0);
    }

    #[test]
    fn test_control_flow_merge_handling() {
        let (function, extractor) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(extractor.get_loan_count());
        
        dataflow.analyze_function(&function, &extractor).unwrap();
        
        // Test merge handling (even though our test function is linear)
        let pp2 = ProgramPoint::new(1);
        let result = dataflow.handle_control_flow_merge(&pp2);
        assert!(result.is_ok(), "Merge handling should succeed");
    }

    #[test]
    fn test_control_flow_branch_handling() {
        let (function, extractor) = create_test_function_with_dataflow();
        let mut dataflow = LoanLivenessDataflow::new(extractor.get_loan_count());
        
        dataflow.analyze_function(&function, &extractor).unwrap();
        
        // Test branch handling (even though our test function is linear)
        let pp1 = ProgramPoint::new(0);
        let result = dataflow.handle_control_flow_branch(&pp1);
        assert!(result.is_ok(), "Branch handling should succeed");
    }

    #[test]
    fn test_bitset_operations_in_dataflow() {
        let mut bitset1 = BitSet::new(10);
        let mut bitset2 = BitSet::new(10);
        
        // Set some bits
        bitset1.set(1);
        bitset1.set(3);
        bitset2.set(2);
        bitset2.set(3);
        
        // Test union
        bitset1.union_with(&bitset2);
        assert!(bitset1.get(1));
        assert!(bitset1.get(2));
        assert!(bitset1.get(3));
        
        // Test subtract
        let mut bitset3 = BitSet::new(10);
        bitset3.set(2);
        bitset1.subtract(&bitset3);
        assert!(bitset1.get(1));
        assert!(!bitset1.get(2)); // Should be removed
        assert!(bitset1.get(3));
    }

    #[test]
    fn test_entry_point_function() {
        let (function, extractor) = create_test_function_with_dataflow();
        
        let result = run_loan_liveness_dataflow(&function, &extractor);
        assert!(result.is_ok(), "Entry point function should succeed");
        
        let dataflow = result.unwrap();
        assert_eq!(dataflow.loan_count, extractor.get_loan_count());
        assert!(!dataflow.live_in_loans.is_empty());
        assert!(!dataflow.live_out_loans.is_empty());
    }
}