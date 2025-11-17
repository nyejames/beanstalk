// Consolidated imports for better organization and maintainability
use crate::compiler::wir::{
    place::Place,
    wir_nodes::{BorrowKind, Loan, LoanId, PlaceState, ProgramPoint, StateTransition, WirFunction},
};
use crate::borrow_log;
use std::collections::HashMap;

/// State mapping for loan-to-state relationships in Beanstalk's memory model
///
/// This structure tracks the mapping between loans and place states, enabling
/// state-aware borrow checking that understands Beanstalk's implicit borrowing semantics.
#[derive(Debug, Clone)]
pub struct StateMapping {
    /// Mapping from loan ID to the state it represents
    pub loan_to_state: HashMap<LoanId, PlaceState>,
    /// Mapping from place to all loans that affect it
    pub place_to_loans: HashMap<Place, Vec<LoanId>>,
    /// History of state transitions for debugging and error reporting
    pub state_history: Vec<StateTransition>,
}

impl StateMapping {
    /// Create a new empty state mapping
    pub fn new() -> Self {
        Self {
            loan_to_state: HashMap::new(),
            place_to_loans: HashMap::new(),
            state_history: Vec::new(),
        }
    }

    /// Map a loan to a specific state
    pub fn map_loan_to_state(&mut self, loan_id: LoanId, place: Place, state: PlaceState) {
        self.loan_to_state.insert(loan_id, state);
        self.place_to_loans.entry(place).or_default().push(loan_id);
    }

    /// Get the current state of a place based on its active loans
    pub fn get_place_state(&self, place: &Place) -> PlaceState {
        // Get all loans affecting this place
        let loans = match self.place_to_loans.get(place) {
            Some(loans) => loans,
            None => return PlaceState::Owned,
        };

        if loans.is_empty() {
            return PlaceState::Owned;
        }

        // Check for mutable loans first (exclusive)
        for loan_id in loans {
            if let Some(state) = self.loan_to_state.get(loan_id)
                && *state == PlaceState::Borrowed
            {
                return PlaceState::Borrowed;
            }
        }

        // If no mutable loans, check for shared loans
        for loan_id in loans {
            if let Some(state) = self.loan_to_state.get(loan_id)
                && *state == PlaceState::Referenced
            {
                return PlaceState::Referenced;
            }
        }

        // Default to owned if no active loans
        PlaceState::Owned
    }

    /// Record a state transition
    pub fn record_state_transition(&mut self, transition: StateTransition) {
        self.state_history.push(transition);
    }

    /// Get all state transitions for a place
    pub fn get_state_transitions_for_place(&self, place: &Place) -> Vec<&StateTransition> {
        self.state_history
            .iter()
            .filter(|t| &t.place == place)
            .collect()
    }

    /// Get loans affecting a specific place
    pub fn get_loans_for_place(&self, place: &Place) -> Option<&[LoanId]> {
        self.place_to_loans.get(place).map(|v| v.as_slice())
    }

    /// Check if a place has any mutable loans
    pub fn has_mutable_loans(&self, place: &Place) -> bool {
        if let Some(loan_ids) = self.place_to_loans.get(place) {
            loan_ids
                .iter()
                .any(|loan_id| self.loan_to_state.get(loan_id) == Some(&PlaceState::Borrowed))
        } else {
            false
        }
    }

    /// Check if a place has any shared loans
    pub fn has_shared_loans(&self, place: &Place) -> bool {
        if let Some(loan_ids) = self.place_to_loans.get(place) {
            loan_ids
                .iter()
                .any(|loan_id| self.loan_to_state.get(loan_id) == Some(&PlaceState::Referenced))
        } else {
            false
        }
    }

    /// Get all places that are currently in a specific state
    pub fn get_places_in_state(&self, target_state: &PlaceState) -> Vec<Place> {
        let mut places = Vec::new();
        for place in self.place_to_loans.keys() {
            if self.get_place_state(place) == *target_state {
                places.push(place.clone());
            }
        }
        places
    }

    /// Update the state mapping when a loan ends
    pub fn end_loan(&mut self, loan_id: LoanId) {
        self.loan_to_state.remove(&loan_id);

        // Remove the loan from place-to-loans mapping
        self.place_to_loans.retain(|_, loan_ids| {
            loan_ids.retain(|id| *id != loan_id);
            !loan_ids.is_empty()
        });
    }

    /// Clear all loans for a place (when it's reassigned)
    pub fn clear_loans_for_place(&mut self, place: &Place) {
        if let Some(loan_ids) = self.place_to_loans.remove(place) {
            for loan_id in loan_ids {
                self.loan_to_state.remove(&loan_id);
            }
        }
    }
}

/// Borrow fact extraction for gen/kill set construction with state mapping
///
/// This module builds gen/kill sets for forward loan-liveness dataflow analysis
/// and maps loans to Beanstalk states for state-aware borrow checking.
/// Gen sets contain loans starting at each statement, kill sets contain loans
/// whose owners may alias places that are moved or reassigned.
///
/// Enhanced with state mapping to support Beanstalk's implicit borrowing semantics.
#[derive(Debug)]
pub struct BorrowFactExtractor {
    /// Gen sets: loans starting at each program point
    pub gen_sets: HashMap<ProgramPoint, BitSet>,
    /// Kill sets: loans killed at each program point
    pub kill_sets: HashMap<ProgramPoint, BitSet>,
    /// All loans in the function
    pub loans: Vec<Loan>,
    /// Mapping from places to loans that borrow them
    pub place_to_loans: HashMap<Place, Vec<LoanId>>,
    /// Total number of loans (for bitset sizing)
    pub loan_count: usize,
}

/// Simple bitset for dataflow analysis
///
/// This is a straightforward bitset implementation focused on correctness.
/// Performance optimizations can be added later if needed.
#[derive(Debug, Clone, PartialEq)]
pub struct BitSet {
    /// Bits packed into u64 words for efficient SIMD operations
    words: Vec<u64>,
    /// Number of bits this set can hold (cached for fast access)
    capacity: usize,
    /// Cached word count to avoid repeated division
    word_count: usize,
}

impl BitSet {
    /// Create a new bitset with the given capacity
    pub fn new(capacity: usize) -> Self {
        let word_count = capacity.div_ceil(64); // Round up to nearest 64
        Self {
            words: vec![0; word_count],
            capacity,
            word_count,
        }
    }

    /// Set a bit to true (optimized with cached bounds check)
    #[inline]
    pub fn set(&mut self, bit: usize) {
        debug_assert!(
            bit < self.capacity,
            "Bit index {} out of bounds (capacity: {})",
            bit,
            self.capacity
        );
        let word_index = bit >> 6; // Faster than bit / 64
        let bit_index = bit & 63; // Faster than bit % 64
        unsafe {
            // Safe because we've checked bounds above
            *self.words.get_unchecked_mut(word_index) |= 1u64 << bit_index;
        }
    }

    /// Check if a bit is set (optimized with cached bounds check)
    #[inline]
    pub fn get(&self, bit: usize) -> bool {
        if bit >= self.capacity {
            return false;
        }
        let word_index = bit >> 6; // Faster than bit / 64
        let bit_index = bit & 63; // Faster than bit % 64
        unsafe {
            // Safe because we've checked bounds above
            (*self.words.get_unchecked(word_index) & (1u64 << bit_index)) != 0
        }
    }

    /// Clear a specific bit (set to 0) - for tests
    #[allow(dead_code)] // Utility method for future testing
    #[inline]
    pub fn clear(&mut self, bit: usize) {
        if bit >= self.capacity {
            return;
        }
        let word_index = bit >> 6; // Faster than bit / 64
        let bit_index = bit & 63; // Faster than bit % 64
        unsafe {
            // Safe because we've checked bounds above
            *self.words.get_unchecked_mut(word_index) &= !(1u64 << bit_index);
        }
    }

    /// Fast intersection with another bitset (self &= other) - for tests
    #[allow(dead_code)] // Utility method for future testing
    #[inline]
    pub fn intersect_with(&mut self, other: &BitSet) {
        // Fast path: if other is empty, result is empty
        if other.is_empty_fast() {
            self.clear_all_fast();
            return;
        }

        // Fast path: if self is empty, nothing to do
        if self.is_empty_fast() {
            return;
        }

        // Use the smaller word count
        let min_words = self.word_count.min(other.word_count);

        // Bulk intersection operation
        for i in 0..min_words {
            unsafe {
                *self.words.get_unchecked_mut(i) &= *other.words.get_unchecked(i);
            }
        }

        // Clear any remaining words in self if other is smaller
        if other.word_count < self.word_count {
            for word in &mut self.words[other.word_count..] {
                *word = 0;
            }
        }
    }

    /// Fast union with another bitset (self |= other) - optimized for hot paths
    #[inline]
    pub fn union_with(&mut self, other: &BitSet) {
        // Fast path: if other is empty, nothing to do
        if other.is_empty_fast() {
            return;
        }

        // Fast path: if self is empty, just copy other
        if self.is_empty_fast() {
            self.copy_from(other);
            return;
        }

        // Use the smaller word count to avoid bounds checking
        let min_words = self.word_count.min(other.word_count);

        // Bulk operation with potential for SIMD optimization
        self.union_with_bulk(&other.words[..min_words]);
    }

    /// Bulk union operation optimized for SIMD
    #[inline]
    fn union_with_bulk(&mut self, other_words: &[u64]) {
        // Process in chunks for better cache locality and potential SIMD
        const CHUNK_SIZE: usize = 8; // Process 8 u64s at a time for cache efficiency

        let chunks = other_words.chunks_exact(CHUNK_SIZE);
        let remainder = chunks.remainder();

        // Process full chunks
        for (chunk_idx, chunk) in chunks.enumerate() {
            let start_idx = chunk_idx * CHUNK_SIZE;
            for (i, &word) in chunk.iter().enumerate() {
                unsafe {
                    // Safe because we're within bounds
                    *self.words.get_unchecked_mut(start_idx + i) |= word;
                }
            }
        }

        // Process remainder
        let remainder_start = (other_words.len() / CHUNK_SIZE) * CHUNK_SIZE;
        for (i, &word) in remainder.iter().enumerate() {
            unsafe {
                // Safe because we're within bounds
                *self.words.get_unchecked_mut(remainder_start + i) |= word;
            }
        }
    }

    /// Fast subtraction of another bitset (self &= !other) - optimized for hot paths
    #[inline]
    pub fn subtract(&mut self, other: &BitSet) {
        // Fast path: if other is empty, nothing to subtract
        if other.is_empty_fast() {
            return;
        }

        // Fast path: if self is empty, nothing to do
        if self.is_empty_fast() {
            return;
        }

        // Use the smaller word count
        let min_words = self.word_count.min(other.word_count);

        // Bulk subtraction operation
        self.subtract_bulk(&other.words[..min_words]);
    }

    /// Bulk subtraction operation optimized for SIMD
    #[inline]
    fn subtract_bulk(&mut self, other_words: &[u64]) {
        const CHUNK_SIZE: usize = 8;

        let chunks = other_words.chunks_exact(CHUNK_SIZE);
        let remainder = chunks.remainder();

        // Process full chunks
        for (chunk_idx, chunk) in chunks.enumerate() {
            let start_idx = chunk_idx * CHUNK_SIZE;
            for (i, &word) in chunk.iter().enumerate() {
                unsafe {
                    *self.words.get_unchecked_mut(start_idx + i) &= !word;
                }
            }
        }

        // Process remainder
        let remainder_start = (other_words.len() / CHUNK_SIZE) * CHUNK_SIZE;
        for (i, &word) in remainder.iter().enumerate() {
            unsafe {
                *self.words.get_unchecked_mut(remainder_start + i) &= !word;
            }
        }
    }

    /// Optimized emptiness check using direct word scanning
    #[inline]
    fn is_empty_fast(&self) -> bool {
        // Fast path for small bitsets (single word)
        if self.word_count == 1 {
            return self.words[0] == 0;
        }

        // Use chunks for better cache performance
        const CHUNK_SIZE: usize = 8;
        let chunks = self.words.chunks_exact(CHUNK_SIZE);
        let remainder = chunks.remainder();

        // Check full chunks
        for chunk in chunks {
            let mut combined = 0u64;
            for &word in chunk {
                combined |= word;
            }
            if combined != 0 {
                return false;
            }
        }

        // Check remainder
        for &word in remainder {
            if word != 0 {
                return false;
            }
        }

        true
    }

    /// Fast bit count without iterator allocation
    #[inline]
    pub fn count_ones(&self) -> usize {
        // Fast path for empty sets
        if self.is_empty_fast() {
            return 0;
        }

        // Fast path for single word
        if self.word_count == 1 {
            return self.words[0].count_ones() as usize;
        }

        // Bulk count using hardware popcount
        let mut total = 0;
        for &word in &self.words {
            total += word.count_ones() as usize;
        }
        total
    }

    /// Iterate over set bit indices without allocation - optimized for hot paths
    pub fn for_each_set_bit<F>(&self, mut f: F)
    where
        F: FnMut(usize),
    {
        // Fast path for empty sets
        if self.is_empty_fast() {
            return;
        }

        // Fast path for single bit (common in sparse sets)
        if self.count_ones() == 1 {
            for (word_idx, &word) in self.words.iter().enumerate() {
                if word != 0 {
                    let bit_idx = word.trailing_zeros() as usize;
                    let global_bit = word_idx * 64 + bit_idx;
                    if global_bit < self.capacity {
                        f(global_bit);
                    }
                    return;
                }
            }
        }

        // General case with optimized bit scanning
        for (word_idx, &word) in self.words.iter().enumerate() {
            if word != 0 {
                let mut remaining_word = word;
                let base_bit = word_idx * 64;

                // Use trailing_zeros for efficient bit scanning
                while remaining_word != 0 {
                    let bit_offset = remaining_word.trailing_zeros() as usize;
                    let global_bit = base_bit + bit_offset;

                    if global_bit >= self.capacity {
                        break;
                    }

                    f(global_bit);

                    // Clear the lowest set bit
                    remaining_word &= remaining_word - 1;
                }
            }
        }
    }

    /// Collect set bit indices into a vector (for compatibility with existing code)
    #[allow(dead_code)] // Utility method for future compatibility
    pub fn iter_set_bits(&self) -> impl Iterator<Item = usize> + '_ {
        // For compatibility, but prefer for_each_set_bit for performance
        let mut bits = Vec::new();
        self.for_each_set_bit(|bit| bits.push(bit));
        bits.into_iter()
    }

    /// Fast clear all bits without allocation
    #[allow(dead_code)] // Utility method for future use
    #[inline]
    pub fn clear_all(&mut self) {
        self.clear_all_fast();
    }

    /// Optimized clear all implementation
    #[inline]
    fn clear_all_fast(&mut self) {
        // Fast path for small bitsets
        if self.word_count <= 4 {
            for word in &mut self.words {
                *word = 0;
            }
            return;
        }

        // Use fill for larger bitsets (potentially SIMD optimized)
        self.words.fill(0);
    }

    /// Fast copy from another bitset
    #[inline]
    pub fn copy_from(&mut self, other: &BitSet) {
        debug_assert_eq!(self.capacity, other.capacity, "BitSet capacity mismatch");
        debug_assert_eq!(
            self.word_count, other.word_count,
            "BitSet word count mismatch"
        );

        // Fast path for empty source
        if other.is_empty_fast() {
            self.clear_all_fast();
            return;
        }

        // Bulk copy (potentially SIMD optimized)
        self.words.copy_from_slice(&other.words);
    }

    /// Create a copy of this bitset (optimized)
    pub fn clone(&self) -> Self {
        Self {
            words: self.words.clone(),
            capacity: self.capacity,
            word_count: self.word_count,
        }
    }

    /// Get the capacity of this bitset (cached for fast access)
    #[allow(dead_code)] // Utility method for future use
    #[inline]
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl BorrowFactExtractor {
    /// Create a new borrow fact extractor with place interning
    pub fn new() -> Self {
        Self {
            gen_sets: HashMap::new(),
            kill_sets: HashMap::new(),
            loans: Vec::new(),
            place_to_loans: HashMap::new(),
            loan_count: 0,
        }
    }

    /// Extract borrow facts from a function with state mapping
    ///
    /// This method processes events from all program points and maps loans to states,
    /// building both gen/kill sets (existing) and state mappings (new) for hybrid
    /// state-loan borrow checking.
    pub fn from_function_with_states(
        function: &WirFunction,
    ) -> Result<(Self, StateMapping), String> {
        let mut extractor = Self::new();
        let mut state_mapping = StateMapping::new();

        // Step 1: Extract loans and build gen/kill sets using existing infrastructure
        extractor.extract_function(function)?;

        // Step 2: Map each loan to its corresponding state
        for loan in &extractor.loans {
            let state = match loan.kind {
                BorrowKind::Shared => PlaceState::Referenced,
                BorrowKind::Mut => PlaceState::Borrowed,
            };
            state_mapping.map_loan_to_state(loan.id, loan.owner.clone(), state);
        }

        // Step 3: Process events from all program points to build state transitions
        for program_point in function.get_program_points_in_order() {
            if let Some(events) = function.get_events(&program_point) {
                // Record state transitions from events
                for transition in &events.state_transitions {
                    state_mapping.record_state_transition(transition.clone());
                }
            }
        }

        Ok((extractor, state_mapping))
    }

    /// Extract gen/kill sets for a function with place interning optimization
    pub fn extract_function(&mut self, function: &WirFunction) -> Result<(), String> {
        // First, collect all loans from events
        self.collect_loans_from_events(function)?;

        // Build place-to-loans index for efficient kill set construction
        self.build_place_to_loans_index();

        // Build gen sets from the loans we created
        self.build_gen_sets(function)?;

        // Build kill sets from moves and reassigns using fast aliasing
        self.build_kill_sets(function)?;

        Ok(())
    }

    /// Collect all loans from events in the function using place interning
    fn collect_loans_from_events(&mut self, function: &WirFunction) -> Result<(), String> {
        let mut loan_id_counter = 0u32;

        // Copy the existing loans from the function
        self.loans = function.get_loans().to_vec();
        self.loan_count = self.loans.len();

        // If no loans exist, generate them from borrow operations in the WIR
        if self.loans.is_empty() {
            self.generate_loans_from_wir(function, &mut loan_id_counter)?;
        }

        Ok(())
    }

    /// Generate loans from WIR statements that create borrows
    fn generate_loans_from_wir(
        &mut self,
        function: &WirFunction,
        loan_id_counter: &mut u32,
    ) -> Result<(), String> {
        // Scan all blocks and statements for borrow operations
        for block in &function.blocks {
            for (stmt_index, statement) in block.statements.iter().enumerate() {
                // Create program point for this statement
                let program_point = ProgramPoint::new(block.id * 1000 + stmt_index as u32);

                // Check if this statement creates a borrow
                if let Some(loan) =
                    self.extract_loan_from_statement(statement, program_point, loan_id_counter)
                {
                    self.loans.push(loan);
                }
            }

            // Check terminator for borrows
            let terminator_point = ProgramPoint::new(block.id * 1000 + 999);
            if let Some(loan) = self.extract_loan_from_terminator(
                &block.terminator,
                terminator_point,
                loan_id_counter,
            ) {
                self.loans.push(loan);
            }
        }

        self.loan_count = self.loans.len();

        // Log loan generation for debugging
        if self.loan_count > 0 {
            borrow_log!(
                "Generated {} loans for function '{}'",
                self.loan_count, function.name
            );
            for (compiler::borrow_checker::extract::BitSet::is_empty_fast::CHUNK_SIZE, loan) in self.loans.iter().enumerate() {
                borrow_log!(
                    "  Loan {}: {:?} borrow of {:?} at {:?}",
                    i, loan.kind, loan.owner, loan.origin_stmt
                );
            }
        }

        Ok(())
    }

    /// Extract loan from a statement if it creates a borrow
    fn extract_loan_from_statement(
        &self,
        statement: &crate::compiler::wir::wir_nodes::Statement,
        program_point: ProgramPoint,
        loan_id_counter: &mut u32,
    ) -> Option<Loan> {
        use crate::compiler::wir::wir_nodes::{Rvalue, Statement};

        match statement {
            Statement::Assign {
                rvalue: Rvalue::Ref { place, borrow_kind },
                ..
            } => {
                let loan_id = LoanId::new(*loan_id_counter);
                *loan_id_counter += 1;

                Some(Loan {
                    id: loan_id,
                    owner: place.clone(),
                    kind: borrow_kind.clone(),
                    origin_stmt: program_point,
                })
            }
            _ => None,
        }
    }

    /// Extract loan from a terminator if it creates a borrow
    fn extract_loan_from_terminator(
        &self,
        _terminator: &crate::compiler::wir::wir_nodes::Terminator,
        _program_point: ProgramPoint,
        _loan_id_counter: &mut u32,
    ) -> Option<Loan> {
        // Terminators don't typically create borrows in the simplified WIR
        None
    }

    /// Build index from places to loans that borrow them
    pub fn build_place_to_loans_index(&mut self) {
        self.place_to_loans.clear();

        for loan in &self.loans {
            let entry = self.place_to_loans.entry(loan.owner.clone()).or_default();
            entry.push(loan.id);
        }
    }

    /// Build gen sets containing loans starting at each statement
    fn build_gen_sets(&mut self, function: &WirFunction) -> Result<(), String> {
        // Pre-allocate empty BitSet for reuse
        let empty_bitset = BitSet::new(self.loan_count);

        // Initialize gen sets for all program points
        for program_point in function.get_program_points_in_order() {
            self.gen_sets.insert(program_point, empty_bitset.clone());
        }

        // Build gen sets based on loans we created, not from events
        // Since we generate loans from WIR statements, we know exactly where they start
        for loan in &self.loans {
            let gen_set = self.gen_sets.get_mut(&loan.origin_stmt).ok_or_else(|| {
                format!("Gen set not found for program point {:?}", loan.origin_stmt)
            })?;

            // Find the loan index and set it in the gen set
            if let Some(loan_index) = self.loans.iter().position(|l| l.id == loan.id) {
                gen_set.set(loan_index);
            }
        }

        // Log gen set construction for debugging
        let mut total_gen_bits = 0;
        for (program_point, gen_set) in &self.gen_sets {
            let count = gen_set.count_ones();
            if count > 0 {
                total_gen_bits += count;
                borrow_log!("Gen set at {}: {} loans starting", program_point, count);
            }
        }

        if total_gen_bits > 0 {
            borrow_log!("Total gen set bits: {}", total_gen_bits);
        }

        Ok(())
    }

    /// Build kill sets for loans whose owners may alias moved/reassigned places (optimized with fast aliasing)
    fn build_kill_sets(&mut self, function: &WirFunction) -> Result<(), String> {
        // Pre-allocate empty BitSet for reuse
        let empty_bitset = BitSet::new(self.loan_count);

        // Initialize kill sets for all program points using optimized allocation
        for program_point in function.get_program_points_in_order() {
            self.kill_sets.insert(program_point, empty_bitset.clone());
        }

        // Populate kill sets from moves and reassigns using legacy event generation (TODO: optimize)
        for program_point in function.get_program_points_in_order() {
            if let Some(events) = function.generate_events(&program_point) {
                // Fast path: if no moves or reassigns, skip processing
                if events.moves.is_empty() && events.reassigns.is_empty() {
                    continue;
                }

                // Collect places that need to be processed (legacy approach using may_alias)
                let mut places_to_kill =
                    Vec::with_capacity(events.moves.len() + events.reassigns.len());
                places_to_kill.extend(events.moves.iter());
                places_to_kill.extend(events.reassigns.iter());

                // Process all places that need to be killed
                let kill_set = self.kill_sets.get_mut(&program_point).ok_or_else(|| {
                    format!("Kill set not found for program point {:?}", program_point)
                })?;

                // Fast path: single place (common case)
                if places_to_kill.len() == 1 {
                    // Fast path: if no loans, nothing to kill
                    if self.loans.is_empty() {
                        continue;
                    }

                    let place = places_to_kill[0];

                    // Fast path: single loan (common in small functions)
                    if self.loans.len() == 1 {
                        if may_alias(&self.loans[0].owner, place) {
                            kill_set.set(0);
                        }
                    } else {
                        // General case: find all loans whose owners may alias this place
                        for (loan_index, loan) in self.loans.iter().enumerate() {
                            if may_alias(&loan.owner, place) {
                                kill_set.set(loan_index);
                            }
                        }
                    }
                } else {
                    // Multiple places: batch processing
                    for place in places_to_kill {
                        for (loan_index, loan) in self.loans.iter().enumerate() {
                            if may_alias(&loan.owner, place) {
                                kill_set.set(loan_index);
                            }
                        }
                    }
                }
            }
        }

        Ok(())
    }

    /// Get the gen set for a program point
    pub fn get_gen_set(&self, point: &ProgramPoint) -> Option<&BitSet> {
        self.gen_sets.get(point)
    }

    /// Get the kill set for a program point
    pub fn get_kill_set(&self, point: &ProgramPoint) -> Option<&BitSet> {
        self.kill_sets.get(point)
    }

    /// Get all loans in the function
    pub fn get_loans(&self) -> &[Loan] {
        &self.loans
    }

    /// Get the loan count (for bitset sizing)
    pub fn get_loan_count(&self) -> usize {
        self.loan_count
    }

    /// Get loans that borrow a specific place - for tests
    pub fn get_loans_for_place(&self, place: &Place) -> Option<&[LoanId]> {
        self.place_to_loans.get(place).map(|v| v.as_slice())
    }

    /// Get the state of a place using hybrid loan-state analysis
    ///
    /// This method provides efficient state queries on top of loan tracking,
    /// combining the performance of bitset operations with the semantics of
    /// state-based analysis for Beanstalk's memory model.
    pub fn get_place_state_with_loans(
        &self,
        place: &Place,
        live_loans: &BitSet,
        state_mapping: &StateMapping,
    ) -> PlaceState {
        // Fast path: check state mapping first
        let base_state = state_mapping.get_place_state(place);

        // If no loans are affecting this place, return the base state
        if let Some(loan_ids) = self.place_to_loans.get(place) {
            // Check if any of the loans affecting this place are currently live
            let mut has_live_mutable = false;
            let mut has_live_shared = false;

            for loan_id in loan_ids {
                // Find the loan index in our loans vector
                if let Some(loan_index) = self.loans.iter().position(|l| l.id == *loan_id) {
                    // Check if this loan is currently live
                    if live_loans.get(loan_index) {
                        // Check the loan kind to determine state
                        match self.loans[loan_index].kind {
                            BorrowKind::Mut => has_live_mutable = true,
                            BorrowKind::Shared => has_live_shared = true,
                        }
                    }
                }
            }

            // Determine state based on live loans
            if has_live_mutable {
                PlaceState::Borrowed
            } else if has_live_shared {
                PlaceState::Referenced
            } else {
                PlaceState::Owned
            }
        } else {
            base_state
        }
    }

    /// Efficient state query method using loan information
    ///
    /// This method implements the hybrid approach: use existing bitset operations
    /// for loan liveness tracking (performance) and add state-based queries on top
    /// of loan tracking (semantics).
    pub fn get_place_state_efficient(&self, place: &Place, live_loans: &BitSet) -> PlaceState {
        // Fast path: if no loans affect this place, it's owned
        if let Some(loan_ids) = self.place_to_loans.get(place) {
            // Use bitset operations for efficient loan liveness checking
            let mut has_live_mutable = false;
            let mut has_live_shared = false;

            // Iterate through loans affecting this place
            for loan_id in loan_ids {
                // Find the loan index for bitset lookup
                if let Some(loan_index) = self.loans.iter().position(|l| l.id == *loan_id) {
                    // Use efficient bitset get operation
                    if live_loans.get(loan_index) {
                        // Determine state based on loan kind
                        match self.loans[loan_index].kind {
                            BorrowKind::Mut => {
                                has_live_mutable = true;
                                break; // Mutable is exclusive, no need to check further
                            }
                            BorrowKind::Shared => has_live_shared = true,
                        }
                    }
                }
            }

            // Return state based on live loans
            if has_live_mutable {
                PlaceState::Borrowed
            } else if has_live_shared {
                PlaceState::Referenced
            } else {
                PlaceState::Owned
            }
        } else {
            PlaceState::Owned
        }
    }

    /// Check for state-based conflicts using hybrid analysis
    ///
    /// This method combines bitset operations for performance with state-based
    /// conflict detection for Beanstalk's memory model semantics.
    pub fn check_state_conflicts(
        &self,
        place: &Place,
        new_borrow_kind: &BorrowKind,
        live_loans: &BitSet,
    ) -> Option<PlaceState> {
        let current_state = self.get_place_state_efficient(place, live_loans);

        // Apply Beanstalk conflict rules
        match (current_state, new_borrow_kind) {
            // Multiple shared borrows are allowed
            (PlaceState::Referenced, BorrowKind::Shared) => None,

            // Mutable borrows conflict with any existing borrows
            (PlaceState::Referenced, BorrowKind::Mut) => Some(PlaceState::Referenced),
            (PlaceState::Borrowed, BorrowKind::Shared) => Some(PlaceState::Borrowed),
            (PlaceState::Borrowed, BorrowKind::Mut) => Some(PlaceState::Borrowed),

            // No conflict with owned places
            (PlaceState::Owned, _) => None,

            // Can't borrow moved or killed places
            (PlaceState::Moved, _) => Some(PlaceState::Moved),
            (PlaceState::Killed, _) => Some(PlaceState::Killed),
        }
    }

    /// Get all places with live loans using efficient bitset operations
    ///
    /// This method demonstrates the hybrid approach by using bitset operations
    /// for performance while providing state-based results.
    pub fn get_places_with_live_loans(&self, live_loans: &BitSet) -> Vec<(Place, PlaceState)> {
        let mut result = Vec::new();

        // Iterate through all places that have loans
        for (place, loan_ids) in &self.place_to_loans {
            // Check if any loans for this place are live
            let mut has_live_loans = false;
            for loan_id in loan_ids {
                if let Some(loan_index) = self.loans.iter().position(|l| l.id == *loan_id)
                    && live_loans.get(loan_index)
                {
                    has_live_loans = true;
                    break;
                }
            }

            if has_live_loans {
                let state = self.get_place_state_efficient(place, live_loans);
                result.push((place.clone(), state));
            }
        }

        result
    }

    /// Compute state transitions using hybrid analysis
    ///
    /// This method uses bitset operations to efficiently track which loans
    /// are starting/ending and maps them to state transitions.
    pub fn compute_state_transitions(
        &self,
        gen_set: &BitSet,
        kill_set: &BitSet,
        state_mapping: &StateMapping,
    ) -> Vec<StateTransition> {
        let mut transitions = Vec::new();

        // Process loans being generated (starting)
        gen_set.for_each_set_bit(|loan_index| {
            if loan_index < self.loans.len() {
                let loan = &self.loans[loan_index];
                let new_state = match loan.kind {
                    BorrowKind::Shared => PlaceState::Referenced,
                    BorrowKind::Mut => PlaceState::Borrowed,
                };

                transitions.push(StateTransition {
                    place: loan.owner.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: new_state,
                    program_point: loan.origin_stmt,
                    reason: crate::compiler::wir::wir_nodes::TransitionReason::BorrowCreated,
                });
            }
        });

        // Process loans being killed (ending)
        kill_set.for_each_set_bit(|loan_index| {
            if loan_index < self.loans.len() {
                let loan = &self.loans[loan_index];
                let current_state = state_mapping.get_place_state(&loan.owner);

                transitions.push(StateTransition {
                    place: loan.owner.clone(),
                    from_state: current_state,
                    to_state: PlaceState::Owned, // Return to owned when loan ends
                    program_point: loan.origin_stmt,
                    reason: crate::compiler::wir::wir_nodes::TransitionReason::LoanEnded,
                });
            }
        });

        transitions
    }

    /// Update function events with the loans that were created
    /// This ensures that the events contain the correct loan IDs for borrow checking
    pub fn update_function_events(
        &self,
        function: &mut crate::compiler::wir::wir_nodes::WirFunction,
    ) {
        // Update events to include the loan IDs we created
        for loan in &self.loans {
            if let Some(events) = function.events.get_mut(&loan.origin_stmt) {
                // Add the loan ID to the start_loans if it's not already there
                if !events.start_loans.contains(&loan.id) {
                    events.start_loans.push(loan.id);
                }
            }
        }

        // Also store the loans in the function for reference
        function.loans = self.loans.clone();
    }
}

/// Field-sensitive aliasing analysis for places
///
/// This function implements the may_alias(a, b) rules from the design:
/// - Same place → alias
/// - Var(x) aliases Field(Var(x), _) and Index(Var(x), _)
/// - Distinct fields don't alias: x.f1 vs x.f2
/// - Constant indices: arr[0] vs arr[1] don't alias
/// - Dynamic indices: Unknown(_) conservatively aliases everything
pub fn may_alias(place_a: &Place, place_b: &Place) -> bool {
    use crate::compiler::wir::place::{Place, ProjectionElem};

    // Same place always aliases
    if place_a == place_b {
        return true;
    }

    match (place_a, place_b) {
        // Same local/global variables alias
        (Place::Local { index: i1, .. }, Place::Local { index: i2, .. }) => i1 == i2,
        (Place::Global { index: i1, .. }, Place::Global { index: i2, .. }) => i1 == i2,

        // Memory locations alias if they overlap
        (
            Place::Memory {
                base: b1,
                offset: o1,
                size: s1,
            },
            Place::Memory {
                base: b2,
                offset: o2,
                size: s2,
            },
        ) => {
            // Same memory base and overlapping ranges
            if b1 == b2 {
                let start1 = o1.0;
                let end1 = start1 + s1.byte_size();
                let start2 = o2.0;
                let end2 = start2 + s2.byte_size();

                // Check for overlap: [start1, end1) overlaps [start2, end2)
                start1 < end2 && start2 < end1
            } else {
                false
            }
        }

        // Projection aliasing rules
        (
            Place::Projection {
                base: base_a,
                elem: elem_a,
            },
            Place::Projection {
                base: base_b,
                elem: elem_b,
            },
        ) => {
            // If bases don't alias, projections don't alias
            if !may_alias(base_a, base_b) {
                return false;
            }

            // If bases alias, check projection elements
            match (elem_a, elem_b) {
                // Same field projections alias
                (
                    ProjectionElem::Field { index: i1, .. },
                    ProjectionElem::Field { index: i2, .. },
                ) => i1 == i2,

                // Array index projections
                (
                    ProjectionElem::Index { index: idx1, .. },
                    ProjectionElem::Index { index: idx2, .. },
                ) => {
                    // If both are constant indices, check if they're the same
                    match (idx1.as_ref(), idx2.as_ref()) {
                        (Place::Local { index: i1, .. }, Place::Local { index: i2, .. })
                            if i1 == i2 =>
                        {
                            true
                        }
                        (Place::Memory { offset: o1, .. }, Place::Memory { offset: o2, .. })
                            if o1 == o2 =>
                        {
                            true
                        }
                        _ => true, // Conservative: assume dynamic indices may alias
                    }
                }

                // Different projection types: conservative aliasing
                _ => true,
            }
        }

        // Variable aliases its projections
        (
            base,
            Place::Projection {
                base: proj_base, ..
            },
        )
        | (
            Place::Projection {
                base: proj_base, ..
            },
            base,
        ) => may_alias(base, proj_base),

        // Different types don't alias
        _ => false,
    }
}

/// Entry point for extracting gen/kill sets from a function with place interning optimization
pub fn extract_gen_kill_sets(function: &WirFunction) -> Result<BorrowFactExtractor, String> {
    let mut extractor = BorrowFactExtractor::new();
    extractor.extract_function(function)?;
    Ok(extractor)
}
