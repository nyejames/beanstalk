use crate::compiler::mir::mir_nodes::{MIR, MirFunction, Operand, ProgramPoint, Rvalue, Statement};
use crate::compiler::mir::place::Place;
use std::collections::{HashMap, HashSet};

/// Backward liveness analysis for MIR with precise last-use refinement
///
/// This module implements standard backward dataflow analysis to compute live variables
/// at each program point, then refines candidate last uses from AST analysis to convert
/// Copy(place) operations to Move(place) when the place is not live after the statement.
///
/// ## Algorithm Overview
///
/// Uses standard backward dataflow equations:
/// ```
/// LiveOut[s] = ⋃ LiveIn[succ(s)]
/// LiveIn[s] = Uses[s] ∪ (LiveOut[s] - Defs[s])
/// ```
///
/// ## Last-Use Refinement
///
/// The analysis refines candidate last uses identified during AST analysis:
/// 1. **AST Phase**: Count variable uses and mark potential last uses
/// 2. **MIR Phase**: Generate `Copy(place)` for all candidate last uses
/// 3. **Liveness Phase**: Convert `Copy(place)` to `Move(place)` when `place ∉ LiveOut[s]`
///
/// This provides NLL-like precision for borrow lifetimes without complex lifetime tracking.
///
/// ## Performance Characteristics
///
/// - **Time Complexity**: O(n × p × i) where n=program points, p=places, i=iterations
/// - **Space Complexity**: O(n × p) for live sets
/// - **Convergence**: Typically 2-5 iterations due to monotonic lattice
///
/// ## Example
///
/// ```rust
/// // Input MIR:
/// PP0: x = 42
/// PP1: y = Copy(x)  // Candidate last use
/// PP2: z = Copy(x)  // Candidate last use
///
/// // After liveness analysis:
/// PP0: x = 42
/// PP1: y = Copy(x)  // x still live (used at PP2)
/// PP2: z = Move(x)  // x not live after this point
/// ```
#[derive(Debug)]
pub struct LivenessAnalysis {
    /// Live variables entering each program point
    pub live_in: HashMap<ProgramPoint, HashSet<Place>>,
    /// Live variables exiting each program point  
    pub live_out: HashMap<ProgramPoint, HashSet<Place>>,
    /// Use sets per program point (from events)
    pub uses: HashMap<ProgramPoint, HashSet<Place>>,
    /// Definition sets per program point (from events)
    pub defs: HashMap<ProgramPoint, HashSet<Place>>,
    /// Control flow graph: program point -> successors
    pub successors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
    /// Control flow graph: program point -> predecessors
    pub predecessors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
}

impl LivenessAnalysis {
    /// Create a new liveness analysis
    pub fn new() -> Self {
        Self {
            live_in: HashMap::new(),
            live_out: HashMap::new(),
            uses: HashMap::new(),
            defs: HashMap::new(),
            successors: HashMap::new(),
            predecessors: HashMap::new(),
        }
    }

    /// Run backward liveness analysis on the entire MIR
    pub fn analyze_mir(mir: &mut MIR) -> Result<LivenessAnalysis, String> {
        let mut analysis = LivenessAnalysis::new();

        // Analyze each function separately
        for function in &mut mir.functions {
            analysis.analyze_function(function)?;
        }

        Ok(analysis)
    }

    /// Analyze liveness for a single function
    pub fn analyze_function(&mut self, function: &mut MirFunction) -> Result<(), String> {
        // Use the shared CFG from the function instead of building our own
        self.copy_cfg_from_function(function)?;

        // Extract use/def sets from events
        self.extract_use_def_sets(function)?;

        // Run backward dataflow analysis
        self.run_backward_dataflow(function)?;

        // Refine candidate last uses based on liveness
        self.refine_last_uses(function)?;

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

    /// Extract use and def sets from events at each program point
    fn extract_use_def_sets(&mut self, function: &MirFunction) -> Result<(), String> {
        for program_point in function.get_program_points_in_order() {
            // Get events for this program point from the function
            if let Some(events) = function.get_events(&program_point) {
                // Convert events to use/def sets
                let uses: HashSet<Place> = events.uses.iter().cloned().collect();
                let defs: HashSet<Place> = events.reassigns.iter().cloned().collect();

                // Store the sets
                self.uses.insert(program_point, uses);
                self.defs.insert(program_point, defs);
            } else {
                // No events for this program point - initialize empty sets
                self.uses.insert(program_point, HashSet::new());
                self.defs.insert(program_point, HashSet::new());
            }
        }

        Ok(())
    }

    /// Extract uses and defs from a single statement
    fn extract_statement_uses_defs(
        &self,
        statement: &Statement,
        uses: &mut HashSet<Place>,
        defs: &mut HashSet<Place>,
    ) {
        match statement {
            Statement::Assign { place, rvalue } => {
                // The place being assigned is a def
                defs.insert(place.clone());

                // Extract uses from the rvalue
                self.extract_rvalue_uses(rvalue, uses);
            }
            Statement::Call {
                args, destination, ..
            } => {
                // All arguments are uses
                for arg in args {
                    self.extract_operand_uses(arg, uses);
                }

                // Destination is a def if present
                if let Some(dest) = destination {
                    defs.insert(dest.clone());
                }
            }
            Statement::InterfaceCall {
                receiver,
                args,
                destination,
                ..
            } => {
                // Receiver and all arguments are uses
                self.extract_operand_uses(receiver, uses);
                for arg in args {
                    self.extract_operand_uses(arg, uses);
                }

                // Destination is a def if present
                if let Some(dest) = destination {
                    defs.insert(dest.clone());
                }
            }
            Statement::Drop { place } => {
                // Drop uses the place
                uses.insert(place.clone());
            }
            Statement::Store { place, value, .. } => {
                // Store defs the place and uses the value
                defs.insert(place.clone());
                self.extract_operand_uses(value, uses);
            }
            Statement::Alloc { place, size, .. } => {
                // Alloc defs the place and uses the size
                defs.insert(place.clone());
                self.extract_operand_uses(size, uses);
            }
            Statement::Dealloc { place } => {
                // Dealloc uses the place
                uses.insert(place.clone());
            }
            Statement::Nop | Statement::MemoryOp { .. } => {
                // These don't have clear use/def patterns for basic analysis
            }
        }
    }

    /// Extract uses from rvalue operations
    fn extract_rvalue_uses(&self, rvalue: &Rvalue, uses: &mut HashSet<Place>) {
        match rvalue {
            Rvalue::Use(operand) => {
                self.extract_operand_uses(operand, uses);
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.extract_operand_uses(left, uses);
                self.extract_operand_uses(right, uses);
            }
            Rvalue::UnaryOp { operand, .. } => {
                self.extract_operand_uses(operand, uses);
            }
            Rvalue::Cast { source, .. } => {
                self.extract_operand_uses(source, uses);
            }
            Rvalue::Ref { place, .. } => {
                // Borrowing uses the place
                uses.insert(place.clone());
            }
            Rvalue::Deref { place } => {
                // Dereferencing uses the place
                uses.insert(place.clone());
            }
            Rvalue::Array { elements, .. } => {
                for element in elements {
                    self.extract_operand_uses(element, uses);
                }
            }
            Rvalue::Struct { fields, .. } => {
                for (_, operand) in fields {
                    self.extract_operand_uses(operand, uses);
                }
            }
            Rvalue::Load { place, .. } => {
                // Loading uses the place
                uses.insert(place.clone());
            }
            Rvalue::InterfaceCall { receiver, args, .. } => {
                self.extract_operand_uses(receiver, uses);
                for arg in args {
                    self.extract_operand_uses(arg, uses);
                }
            }
            Rvalue::MemoryGrow { pages } => {
                self.extract_operand_uses(pages, uses);
            }
            Rvalue::MemorySize => {
                // No uses
            }
        }
    }

    /// Extract uses from operands
    fn extract_operand_uses(&self, operand: &Operand, uses: &mut HashSet<Place>) {
        match operand {
            Operand::Copy(place) | Operand::Move(place) => {
                uses.insert(place.clone());
            }
            Operand::Constant(_) | Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
                // These don't use places
            }
        }
    }

    /// Extract uses from terminator operations
    fn extract_terminator_uses(
        &self,
        terminator: &crate::compiler::mir::mir_nodes::Terminator,
        uses: &mut HashSet<Place>,
    ) {
        match terminator {
            crate::compiler::mir::mir_nodes::Terminator::If { condition, .. } => {
                self.extract_operand_uses(condition, uses);
            }
            crate::compiler::mir::mir_nodes::Terminator::Switch { discriminant, .. } => {
                self.extract_operand_uses(discriminant, uses);
            }
            crate::compiler::mir::mir_nodes::Terminator::Return { values } => {
                for value in values {
                    self.extract_operand_uses(value, uses);
                }
            }
            _ => {
                // Other terminators don't have operands that use places
            }
        }
    }

    /// Run backward dataflow analysis using worklist algorithm
    fn run_backward_dataflow(&mut self, function: &MirFunction) -> Result<(), String> {
        let program_points = function.get_program_points_in_order();

        // Initialize all live sets to empty
        for point in &program_points {
            self.live_in.insert(*point, HashSet::new());
            self.live_out.insert(*point, HashSet::new());
        }

        // Worklist algorithm for backward dataflow
        let mut worklist: Vec<ProgramPoint> = program_points.clone();
        let mut iteration_count = 0;
        const MAX_ITERATIONS: usize = 1000; // Prevent infinite loops

        while let Some(current_point) = worklist.pop() {
            iteration_count += 1;
            if iteration_count > MAX_ITERATIONS {
                return Err(format!(
                    "Liveness analysis failed to converge after {} iterations",
                    MAX_ITERATIONS
                ));
            }

            // Compute LiveOut[s] = ⋃ LiveIn[succ(s)]
            let mut new_live_out = HashSet::new();
            if let Some(successors) = self.successors.get(&current_point) {
                for &successor in successors {
                    if let Some(succ_live_in) = self.live_in.get(&successor) {
                        new_live_out.extend(succ_live_in.iter().cloned());
                    }
                }
            }

            // Check if LiveOut changed
            let old_live_out = self
                .live_out
                .get(&current_point)
                .cloned()
                .unwrap_or_default();
            let live_out_changed = new_live_out != old_live_out;

            if live_out_changed {
                self.live_out.insert(current_point, new_live_out.clone());

                // Compute LiveIn[s] = Uses[s] ∪ (LiveOut[s] - Defs[s])
                let uses = self.uses.get(&current_point).cloned().unwrap_or_default();
                let defs = self.defs.get(&current_point).cloned().unwrap_or_default();

                let mut new_live_in = uses;
                for place in &new_live_out {
                    if !defs.contains(place) {
                        new_live_in.insert(place.clone());
                    }
                }

                // Check if LiveIn changed
                let old_live_in = self
                    .live_in
                    .get(&current_point)
                    .cloned()
                    .unwrap_or_default();
                if new_live_in != old_live_in {
                    self.live_in.insert(current_point, new_live_in);

                    // Add predecessors to worklist
                    if let Some(predecessors) = self.predecessors.get(&current_point) {
                        for &pred in predecessors {
                            if !worklist.contains(&pred) {
                                worklist.push(pred);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Refine candidate last uses based on liveness analysis
    fn refine_last_uses(&mut self, function: &mut MirFunction) -> Result<(), String> {
        // This function will refine Copy operations to Move operations when
        // the place is not live after the statement (confirmed last use)

        for block in &mut function.blocks {
            // Collect statement program points first to avoid borrowing issues
            let statement_points: Vec<(usize, ProgramPoint)> = (0..block.statements.len())
                .filter_map(|i| block.get_statement_program_point(i).map(|pp| (i, pp)))
                .collect();

            for (stmt_index, program_point) in statement_points {
                // Get LiveOut for this program point
                let live_out = self
                    .live_out
                    .get(&program_point)
                    .cloned()
                    .unwrap_or_default();

                // Refine operands in this statement
                if let Some(statement) = block.statements.get_mut(stmt_index) {
                    self.refine_statement_operands(statement, &live_out);
                }
            }

            // Also refine terminator operands
            if let Some(terminator_point) = block.get_terminator_program_point() {
                let live_out = self
                    .live_out
                    .get(&terminator_point)
                    .cloned()
                    .unwrap_or_default();
                self.refine_terminator_operands(&mut block.terminator, &live_out);
            }
        }

        Ok(())
    }

    /// Refine operands in a statement based on liveness
    fn refine_statement_operands(&self, statement: &mut Statement, live_out: &HashSet<Place>) {
        match statement {
            Statement::Assign { rvalue, .. } => {
                self.refine_rvalue_operands(rvalue, live_out);
            }
            Statement::Call { args, .. } => {
                for arg in args {
                    self.refine_operand(arg, live_out);
                }
            }
            Statement::InterfaceCall { receiver, args, .. } => {
                self.refine_operand(receiver, live_out);
                for arg in args {
                    self.refine_operand(arg, live_out);
                }
            }
            Statement::Store { value, .. } => {
                self.refine_operand(value, live_out);
            }
            Statement::Alloc { size, .. } => {
                self.refine_operand(size, live_out);
            }
            _ => {
                // Other statements don't have operands that can be refined
            }
        }
    }

    /// Refine operands in rvalue operations
    fn refine_rvalue_operands(&self, rvalue: &mut Rvalue, live_out: &HashSet<Place>) {
        match rvalue {
            Rvalue::Use(operand) => {
                self.refine_operand(operand, live_out);
            }
            Rvalue::BinaryOp { left, right, .. } => {
                self.refine_operand(left, live_out);
                self.refine_operand(right, live_out);
            }
            Rvalue::UnaryOp { operand, .. } => {
                self.refine_operand(operand, live_out);
            }
            Rvalue::Cast { source, .. } => {
                self.refine_operand(source, live_out);
            }
            Rvalue::Array { elements, .. } => {
                for element in elements {
                    self.refine_operand(element, live_out);
                }
            }
            Rvalue::Struct { fields, .. } => {
                for (_, operand) in fields {
                    self.refine_operand(operand, live_out);
                }
            }
            Rvalue::InterfaceCall { receiver, args, .. } => {
                self.refine_operand(receiver, live_out);
                for arg in args {
                    self.refine_operand(arg, live_out);
                }
            }
            Rvalue::MemoryGrow { pages } => {
                self.refine_operand(pages, live_out);
            }
            _ => {
                // Other rvalues don't have operands that can be refined
            }
        }
    }

    /// Refine operands in terminator operations
    fn refine_terminator_operands(
        &self,
        terminator: &mut crate::compiler::mir::mir_nodes::Terminator,
        live_out: &HashSet<Place>,
    ) {
        match terminator {
            crate::compiler::mir::mir_nodes::Terminator::If { condition, .. } => {
                self.refine_operand(condition, live_out);
            }
            crate::compiler::mir::mir_nodes::Terminator::Switch { discriminant, .. } => {
                self.refine_operand(discriminant, live_out);
            }
            crate::compiler::mir::mir_nodes::Terminator::Return { values } => {
                for value in values {
                    self.refine_operand(value, live_out);
                }
            }
            _ => {
                // Other terminators don't have operands that can be refined
            }
        }
    }

    /// Refine a single operand: convert Copy(place) to Move(place) if place ∉ LiveOut
    fn refine_operand(&self, operand: &mut Operand, live_out: &HashSet<Place>) {
        match operand {
            Operand::Copy(place) => {
                // If the place is not live after this statement, convert to Move
                if !live_out.contains(place) {
                    *operand = Operand::Move(place.clone());
                }
            }
            _ => {
                // Other operand types don't need refinement
            }
        }
    }

    /// Get live variables at a program point (for debugging/testing)
    pub fn get_live_in(&self, point: &ProgramPoint) -> Option<&HashSet<Place>> {
        self.live_in.get(point)
    }

    /// Get live variables after a program point (for debugging/testing)
    pub fn get_live_out(&self, point: &ProgramPoint) -> Option<&HashSet<Place>> {
        self.live_out.get(point)
    }

    /// Check if a place is live at a program point
    pub fn is_live_at(&self, place: &Place, point: &ProgramPoint) -> bool {
        self.live_in
            .get(point)
            .map(|live_set| live_set.contains(place))
            .unwrap_or(false)
    }

    /// Check if a place is live after a program point
    pub fn is_live_after(&self, place: &Place, point: &ProgramPoint) -> bool {
        self.live_out
            .get(point)
            .map(|live_set| live_set.contains(place))
            .unwrap_or(false)
    }

    /// Get statistics about the analysis (for debugging)
    pub fn get_statistics(&self) -> LivenessStatistics {
        LivenessStatistics {
            total_program_points: self.live_in.len(),
            max_live_vars_at_point: self
                .live_in
                .values()
                .map(|set| set.len())
                .max()
                .unwrap_or(0),
            total_refinements: 0, // This would be tracked during refinement
        }
    }
}

/// Statistics about liveness analysis results
#[derive(Debug, Clone)]
pub struct LivenessStatistics {
    /// Total number of program points analyzed
    pub total_program_points: usize,
    /// Maximum number of live variables at any single program point
    pub max_live_vars_at_point: usize,
    /// Total number of Copy->Move refinements made
    pub total_refinements: usize,
}

/// Entry point for running liveness analysis on MIR
pub fn run_liveness_analysis(mir: &mut MIR) -> Result<LivenessAnalysis, String> {
    LivenessAnalysis::analyze_mir(mir)
}
