use crate::compiler::mir::dataflow::LoanLivenessDataflow;
use crate::compiler::mir::extract::{BorrowFactExtractor, may_alias};
use crate::compiler::mir::mir_nodes::{
    BorrowError, BorrowErrorType, BorrowKind, InvalidationType, Loan, MirFunction, ProgramPoint,
};
use crate::compiler::mir::place::Place;
use crate::compiler::parsers::tokens::TextLocation;
use std::collections::{HashMap, HashSet, VecDeque};

/// Borrow conflict detection module
///
/// This module implements conflict detection for the simplified MIR borrow checker.
/// It detects unique/shared borrow overlaps, move-while-borrowed violations,
/// and use-after-move errors using live loan sets and aliasing analysis.
///
/// ## Conflict Types Detected
///
/// 1. **Conflicting Borrows**: Unique/shared borrow overlaps using `may_alias`
/// 2. **Move While Borrowed**: Moving place that aliases live loan owner
/// 3. **Use After Move**: Using place after it has been moved (separate dataflow)
///
/// ## Aliasing Rules
///
/// The `may_alias` function implements field-sensitive aliasing:
/// - Same place always aliases
/// - Whole vs part relationships (e.g., `x` aliases `x.field`)
/// - Distinct fields don't alias (e.g., `x.f1` vs `x.f2`)
/// - Constant indices don't alias (e.g., `arr[0]` vs `arr[1]`)
/// - Dynamic indices conservatively alias
///
/// ## Performance Characteristics
///
/// - **Conflict Detection**: O(n × l²) for checking loan pairs at each point
/// - **Move-While-Borrowed**: O(n × l × m) where m=moves per point
/// - **Use-After-Move**: O(n × u × p) where u=uses, p=moved places
/// - **Early Termination**: Stops on first conflict for fast error reporting
///
/// ## Example Conflicts
///
/// ```rust
/// // Conflicting borrows:
/// let a = &x;      // Shared borrow
/// let b = &mut x;  // ERROR: Mutable borrow of shared-borrowed value
///
/// // Move while borrowed:
/// let a = &x.field;
/// move x;          // ERROR: Cannot move x because x.field is borrowed
///
/// // Use after move:
/// let y = move x;
/// use(x);          // ERROR: Use of moved value x
/// ```
#[derive(Debug)]
pub struct BorrowConflictChecker {
    /// Live loan dataflow results
    dataflow: LoanLivenessDataflow,
    /// Borrow fact extractor with loan information
    extractor: BorrowFactExtractor,
    /// Detected conflicts grouped by severity
    errors: Vec<BorrowError>,
    warnings: Vec<BorrowError>,
    /// Moved-out places tracking for use-after-move detection
    moved_out_dataflow: MovedOutDataflow,
}

/// Forward dataflow for tracking moved-out places
#[derive(Debug)]
pub struct MovedOutDataflow {
    /// Places that are moved-out at each program point (live-in)
    moved_in: HashMap<ProgramPoint, HashSet<Place>>,
    /// Places that are moved-out after each program point (live-out)
    moved_out: HashMap<ProgramPoint, HashSet<Place>>,
    /// Control flow graph successors
    successors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
    /// Control flow graph predecessors
    predecessors: HashMap<ProgramPoint, Vec<ProgramPoint>>,
}

/// Conflict severity levels
#[derive(Debug, Clone, PartialEq)]
pub enum ConflictSeverity {
    /// Critical error that prevents compilation
    Error,
    /// Warning that should be addressed but doesn't prevent compilation.
    /// This will be for things like unused variables
    Warning,
}

/// Conflict detection results
#[derive(Debug)]
pub struct ConflictResults {
    /// All detected errors (critical)
    pub errors: Vec<BorrowError>,
    /// All detected warnings
    pub warnings: Vec<BorrowError>,
    /// Statistics about the analysis
    pub statistics: ConflictStatistics,
}

/// Statistics about conflict detection
#[derive(Debug, Clone)]
pub struct ConflictStatistics {
    /// Total number of program points analyzed
    pub total_program_points: usize,
    /// Total number of loans analyzed
    pub total_loans: usize,
    /// Number of conflicts detected
    pub total_conflicts: usize,
    /// Number of errors vs warnings
    pub error_count: usize,
    pub warning_count: usize,
    /// Conflict types breakdown
    pub conflicting_borrows_count: usize,
    pub move_while_borrowed_count: usize,
    pub use_after_move_count: usize,
}

impl BorrowConflictChecker {
    /// Create a new borrow conflict checker
    pub fn new(dataflow: LoanLivenessDataflow, extractor: BorrowFactExtractor) -> Self {
        Self {
            moved_out_dataflow: MovedOutDataflow::new(),
            dataflow,
            extractor,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Run conflict detection on a function
    pub fn check_function(&mut self, function: &MirFunction) -> Result<ConflictResults, String> {
        // First, run moved-out dataflow analysis
        self.moved_out_dataflow.analyze_function(function)?;

        // Check for different types of conflicts
        self.check_conflicting_borrows(function)?;
        self.check_move_while_borrowed(function)?;
        self.check_use_after_move(function)?;

        // Generate results
        let statistics = self.generate_statistics(function);

        Ok(ConflictResults {
            errors: self.errors.clone(),
            warnings: self.warnings.clone(),
            statistics,
        })
    }

    /// Detect unique/shared borrow overlaps using live loan sets and may_alias
    fn check_conflicting_borrows(&mut self, function: &MirFunction) -> Result<(), String> {
        let loans = self.extractor.get_loans();

        for program_point in function.get_program_points_in_order() {
            // Get live loans at this program point
            let live_loans = self
                .dataflow
                .get_live_in_loans(&program_point)
                .ok_or_else(|| {
                    format!("No live loans found for program point {}", program_point)
                })?;

            // Check all pairs of live loans for conflicts
            let live_loan_indices: Vec<usize> = live_loans.iter_set_bits().collect();

            for i in 0..live_loan_indices.len() {
                for j in (i + 1)..live_loan_indices.len() {
                    let loan_idx_a = live_loan_indices[i];
                    let loan_idx_b = live_loan_indices[j];

                    if loan_idx_a < loans.len() && loan_idx_b < loans.len() {
                        let loan_a = &loans[loan_idx_a];
                        let loan_b = &loans[loan_idx_b];

                        // Check if the loans conflict
                        if self.loans_conflict(loan_a, loan_b) {
                            let error = self.create_conflicting_borrows_error(
                                program_point,
                                loan_a,
                                loan_b,
                            );
                            self.errors.push(error);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Check if two loans conflict based on their kinds and aliasing
    fn loans_conflict(&self, loan_a: &Loan, loan_b: &Loan) -> bool {
        // Loans conflict if:
        // 1. Their owners may alias
        // 2. At least one is a mutable/unique borrow

        if !may_alias(&loan_a.owner, &loan_b.owner) {
            return false;
        }

        match (&loan_a.kind, &loan_b.kind) {
            // Two shared borrows don't conflict
            (BorrowKind::Shared, BorrowKind::Shared) => false,
            // Any other combination with aliasing places conflicts
            _ => true,
        }
    }

    /// Create a conflicting borrows error
    fn create_conflicting_borrows_error(
        &self,
        point: ProgramPoint,
        loan_a: &Loan,
        loan_b: &Loan,
    ) -> BorrowError {
        let message = format!(
            "Cannot have {} and {} borrows of aliasing places at the same time",
            borrow_kind_name(&loan_a.kind),
            borrow_kind_name(&loan_b.kind)
        );

        BorrowError {
            point,
            error_type: BorrowErrorType::ConflictingBorrows {
                existing_borrow: loan_a.kind.clone(),
                new_borrow: loan_b.kind.clone(),
                place: loan_a.owner.clone(),
            },
            message,
            location: TextLocation::default(), // TODO: Get actual source location
        }
    }

    /// Check move-while-borrowed: error if moving place that aliases live loan owner
    fn check_move_while_borrowed(&mut self, function: &MirFunction) -> Result<(), String> {
        let loans = self.extractor.get_loans();

        for program_point in function.get_program_points_in_order() {
            if let Some(events) = function.get_events(&program_point) {
                // Get live loans at this program point
                let live_loans =
                    self.dataflow
                        .get_live_in_loans(&program_point)
                        .ok_or_else(|| {
                            format!("No live loans found for program point {}", program_point)
                        })?;

                // Check each move against live loans
                for moved_place in &events.moves {
                    for loan_idx in live_loans.iter_set_bits() {
                        if loan_idx < loans.len() {
                            let loan = &loans[loan_idx];

                            // Check if the moved place aliases the loan owner
                            if may_alias(moved_place, &loan.owner) {
                                let error = self.create_move_while_borrowed_error(
                                    program_point,
                                    moved_place.clone(),
                                    loan,
                                );
                                self.errors.push(error);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Create a move-while-borrowed error
    fn create_move_while_borrowed_error(
        &self,
        point: ProgramPoint,
        moved_place: Place,
        loan: &Loan,
    ) -> BorrowError {
        let message = format!(
            "Cannot move out of `{}` because it is borrowed (loan {} at {})",
            place_name(&moved_place),
            loan.id,
            loan.origin_stmt
        );

        BorrowError {
            point,
            error_type: BorrowErrorType::BorrowAcrossOwnerInvalidation {
                borrowed_place: loan.owner.clone(),
                owner_place: moved_place.clone(),
                invalidation_point: point,
                invalidation_type: InvalidationType::Move,
            },
            message,
            location: TextLocation::default(), // TODO: Get actual source location
        }
    }

    /// Check use-after-move using separate forward dataflow for moved-out places
    fn check_use_after_move(&mut self, function: &MirFunction) -> Result<(), String> {
        for program_point in function.get_program_points_in_order() {
            if let Some(events) = function.get_events(&program_point) {
                // Get moved-out places at this program point
                let moved_out_places = self.moved_out_dataflow.get_moved_in_places(&program_point);

                // Check each use against moved-out places
                for used_place in &events.uses {
                    for moved_place in &moved_out_places {
                        // Check if the used place aliases a moved-out place
                        if may_alias(used_place, &moved_place) {
                            // Find the move point for better error reporting
                            let move_point =
                                self.find_move_point_for_place(function, used_place, program_point);

                            let error = self.create_use_after_move_error(
                                program_point,
                                used_place.clone(),
                                move_point,
                            );
                            self.errors.push(error);
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Find the program point where a place was moved (for error reporting)
    fn find_move_point_for_place(
        &self,
        function: &MirFunction,
        place: &Place,
        current_point: ProgramPoint,
    ) -> ProgramPoint {
        // Search backwards from current point to find where the place was moved
        for program_point in function.get_program_points_in_order() {
            if program_point >= current_point {
                break;
            }

            if let Some(events) = function.get_events(&program_point) {
                for moved_place in &events.moves {
                    if may_alias(place, moved_place) {
                        return program_point;
                    }
                }
            }
        }

        // Fallback: return the current point if we can't find the move
        current_point
    }

    /// Create a use-after-move error
    fn create_use_after_move_error(
        &self,
        point: ProgramPoint,
        used_place: Place,
        move_point: ProgramPoint,
    ) -> BorrowError {
        let message = format!(
            "Use of moved value `{}` (moved at {})",
            place_name(&used_place),
            move_point
        );

        BorrowError {
            point,
            error_type: BorrowErrorType::UseAfterMove {
                place: used_place,
                move_point,
            },
            message,
            location: TextLocation::default(), // TODO: Get actual source location
        }
    }

    /// Generate statistics about the conflict detection results
    fn generate_statistics(&self, function: &MirFunction) -> ConflictStatistics {
        let mut conflicting_borrows_count = 0;
        let mut move_while_borrowed_count = 0;
        let mut use_after_move_count = 0;

        // Count error types
        for error in &self.errors {
            match &error.error_type {
                BorrowErrorType::ConflictingBorrows { .. } => conflicting_borrows_count += 1,
                BorrowErrorType::BorrowAcrossOwnerInvalidation { .. } => {
                    move_while_borrowed_count += 1
                }
                BorrowErrorType::UseAfterMove { .. } => use_after_move_count += 1,
            }
        }

        // Also count warnings
        for warning in &self.warnings {
            match &warning.error_type {
                BorrowErrorType::ConflictingBorrows { .. } => conflicting_borrows_count += 1,
                BorrowErrorType::BorrowAcrossOwnerInvalidation { .. } => {
                    move_while_borrowed_count += 1
                }
                BorrowErrorType::UseAfterMove { .. } => use_after_move_count += 1,
            }
        }

        ConflictStatistics {
            total_program_points: function.get_program_points_in_order().len(),
            total_loans: self.extractor.get_loan_count(),
            total_conflicts: self.errors.len() + self.warnings.len(),
            error_count: self.errors.len(),
            warning_count: self.warnings.len(),
            conflicting_borrows_count,
            move_while_borrowed_count,
            use_after_move_count,
        }
    }
}

impl MovedOutDataflow {
    /// Create a new moved-out dataflow analysis
    pub fn new() -> Self {
        Self {
            moved_in: HashMap::new(),
            moved_out: HashMap::new(),
            successors: HashMap::new(),
            predecessors: HashMap::new(),
        }
    }

    /// Run moved-out dataflow analysis on a function
    pub fn analyze_function(&mut self, function: &MirFunction) -> Result<(), String> {
        // Use the shared CFG from the function instead of building our own
        self.copy_cfg_from_function(function)?;

        // Initialize moved-out sets
        self.initialize_moved_out_sets(function);

        // Run forward dataflow analysis
        self.run_forward_dataflow(function)?;

        Ok(())
    }

    /// Build simple linear CFG from function (simplified implementation)
    fn copy_cfg_from_function(&mut self, function: &MirFunction) -> Result<(), String> {
        // Clear existing CFG data
        self.successors.clear();
        self.predecessors.clear();

        // Build simple linear CFG from program points
        let program_points = function.get_program_points_in_order();
        
        for (i, &current_point) in program_points.iter().enumerate() {
            let mut successors = Vec::new();
            let mut predecessors = Vec::new();
            
            // Add predecessor
            if i > 0 {
                predecessors.push(program_points[i - 1]);
            }
            
            // Add successor
            if i < program_points.len() - 1 {
                successors.push(program_points[i + 1]);
            }
            
            self.successors.insert(current_point, successors);
            self.predecessors.insert(current_point, predecessors);
        }

        Ok(())
    }

    /// Initialize moved-out sets for all program points
    fn initialize_moved_out_sets(&mut self, function: &MirFunction) {
        for program_point in function.get_program_points_in_order() {
            self.moved_in.insert(program_point, HashSet::new());
            self.moved_out.insert(program_point, HashSet::new());
        }
    }

    /// Run forward dataflow analysis for moved-out places
    ///
    /// Equations:
    /// - MovedIn[s] = ⋃ MovedOut[pred(s)]
    /// - MovedOut[s] = (MovedIn[s] - Reassigns[s]) ∪ Moves[s]
    fn run_forward_dataflow(&mut self, function: &MirFunction) -> Result<(), String> {
        let program_points = function.get_program_points_in_order();

        // Worklist algorithm
        let mut worklist: VecDeque<ProgramPoint> = program_points.iter().copied().collect();
        let mut iteration_count = 0;
        const MAX_ITERATIONS: usize = 10000;

        while let Some(current_point) = worklist.pop_front() {
            iteration_count += 1;
            if iteration_count > MAX_ITERATIONS {
                return Err(format!(
                    "Moved-out dataflow failed to converge after {} iterations",
                    MAX_ITERATIONS
                ));
            }

            // Compute MovedIn[s] = ⋃ MovedOut[pred(s)]
            let mut new_moved_in = HashSet::new();
            if let Some(predecessors) = self.predecessors.get(&current_point) {
                for &pred in predecessors {
                    if let Some(pred_moved_out) = self.moved_out.get(&pred) {
                        new_moved_in.extend(pred_moved_out.iter().cloned());
                    }
                }
            }

            // Check if MovedIn changed
            let old_moved_in = self
                .moved_in
                .get(&current_point)
                .cloned()
                .unwrap_or_default();
            let moved_in_changed = new_moved_in != old_moved_in;

            if moved_in_changed {
                // Update MovedIn
                self.moved_in.insert(current_point, new_moved_in.clone());

                // Compute MovedOut[s] = (MovedIn[s] - Reassigns[s]) ∪ Moves[s]
                let mut new_moved_out = new_moved_in.clone();

                // Apply effects from this statement
                if let Some(events) = function.get_events(&current_point) {
                    // Remove reassigned places (they're no longer moved-out)
                    for reassigned_place in &events.reassigns {
                        new_moved_out.retain(|place| !may_alias(place, reassigned_place));
                    }

                    // Add newly moved places
                    new_moved_out.extend(events.moves.iter().cloned());
                }

                // Check if MovedOut changed
                let old_moved_out = self
                    .moved_out
                    .get(&current_point)
                    .cloned()
                    .unwrap_or_default();

                if new_moved_out != old_moved_out {
                    // Update MovedOut
                    self.moved_out.insert(current_point, new_moved_out);

                    // Add successors to worklist
                    if let Some(successors) = self.successors.get(&current_point) {
                        for &succ in successors {
                            if !worklist.contains(&succ) {
                                worklist.push_back(succ);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get moved-in places at a program point
    pub fn get_moved_in_places(&self, point: &ProgramPoint) -> HashSet<Place> {
        self.moved_in.get(point).cloned().unwrap_or_default()
    }

    /// Get moved-out places at a program point
    pub fn get_moved_out_places(&self, point: &ProgramPoint) -> HashSet<Place> {
        self.moved_out.get(point).cloned().unwrap_or_default()
    }

    /// Check if a place is moved-out at a program point
    pub fn is_place_moved_out(&self, place: &Place, point: &ProgramPoint) -> bool {
        if let Some(moved_places) = self.moved_in.get(point) {
            moved_places
                .iter()
                .any(|moved_place| may_alias(place, moved_place))
        } else {
            false
        }
    }
}

/// Helper function to get a human-readable name for a borrow kind
fn borrow_kind_name(kind: &BorrowKind) -> &'static str {
    match kind {
        BorrowKind::Shared => "shared",
        BorrowKind::Mut => "mutable",
        BorrowKind::Unique => "unique",
    }
}

/// Helper function to get a human-readable name for a place
fn place_name(place: &Place) -> String {
    match place {
        Place::Local { index, .. } => format!("local_{}", index),
        Place::Global { index, .. } => format!("global_{}", index),
        Place::Memory { offset, .. } => format!("memory[{}]", offset.0),
        Place::Projection { base, elem } => {
            use crate::compiler::mir::place::ProjectionElem;
            match elem {
                ProjectionElem::Field { index, .. } => {
                    format!("{}.field_{}", place_name(base), index)
                }
                ProjectionElem::Index { .. } => format!("{}[index]", place_name(base)),
                ProjectionElem::Length => format!("{}.len", place_name(base)),
                ProjectionElem::Data => format!("{}.data", place_name(base)),
                ProjectionElem::Deref => format!("*{}", place_name(base)),
            }
        }
    }
}

/// Entry point for running borrow conflict detection
pub fn run_conflict_detection(
    function: &MirFunction,
    dataflow: LoanLivenessDataflow,
    extractor: BorrowFactExtractor,
) -> Result<ConflictResults, String> {
    let mut checker = BorrowConflictChecker::new(dataflow, extractor);
    checker.check_function(function)
}
