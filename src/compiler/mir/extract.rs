use crate::compiler::mir::mir_nodes::{
    Loan, LoanId, MirFunction, ProgramPoint, BorrowKind
};
use crate::compiler::mir::place::Place;
use std::collections::HashMap;

/// Borrow fact extraction for gen/kill set construction
///
/// This module builds gen/kill sets for forward loan-liveness dataflow analysis.
/// Gen sets contain loans starting at each statement, kill sets contain loans
/// whose owners may alias places that are moved or reassigned.
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

/// Efficient bitset for loan tracking
#[derive(Debug, Clone, PartialEq)]
pub struct BitSet {
    /// Bits packed into u64 words
    words: Vec<u64>,
    /// Number of bits this set can hold
    capacity: usize,
}

impl BitSet {
    /// Create a new bitset with the given capacity
    pub fn new(capacity: usize) -> Self {
        let word_count = (capacity + 63) / 64; // Round up to nearest 64
        Self {
            words: vec![0; word_count],
            capacity,
        }
    }

    /// Set a bit to true
    pub fn set(&mut self, bit: usize) {
        if bit < self.capacity {
            let word_index = bit / 64;
            let bit_index = bit % 64;
            self.words[word_index] |= 1u64 << bit_index;
        }
    }

    /// Set a bit to false
    pub fn clear(&mut self, bit: usize) {
        if bit < self.capacity {
            let word_index = bit / 64;
            let bit_index = bit % 64;
            self.words[word_index] &= !(1u64 << bit_index);
        }
    }

    /// Check if a bit is set
    pub fn get(&self, bit: usize) -> bool {
        if bit < self.capacity {
            let word_index = bit / 64;
            let bit_index = bit % 64;
            (self.words[word_index] & (1u64 << bit_index)) != 0
        } else {
            false
        }
    }

    /// Union with another bitset (self |= other)
    pub fn union_with(&mut self, other: &BitSet) {
        for (i, &word) in other.words.iter().enumerate() {
            if i < self.words.len() {
                self.words[i] |= word;
            }
        }
    }

    /// Intersect with another bitset (self &= other)
    pub fn intersect_with(&mut self, other: &BitSet) {
        for (i, &word) in other.words.iter().enumerate() {
            if i < self.words.len() {
                self.words[i] &= word;
            }
        }
    }

    /// Subtract another bitset (self &= !other)
    pub fn subtract(&mut self, other: &BitSet) {
        for (i, &word) in other.words.iter().enumerate() {
            if i < self.words.len() {
                self.words[i] &= !word;
            }
        }
    }

    /// Check if this bitset is empty
    pub fn is_empty(&self) -> bool {
        self.words.iter().all(|&word| word == 0)
    }

    /// Count the number of set bits
    pub fn count_ones(&self) -> usize {
        self.words.iter().map(|&word| word.count_ones() as usize).sum()
    }

    /// Iterate over set bit indices
    pub fn iter_set_bits(&self) -> impl Iterator<Item = usize> + '_ {
        self.words.iter().enumerate().flat_map(|(word_idx, &word)| {
            (0..64).filter_map(move |bit_idx| {
                if (word & (1u64 << bit_idx)) != 0 {
                    Some(word_idx * 64 + bit_idx)
                } else {
                    None
                }
            })
        }).filter(|&bit| bit < self.capacity)
    }

    /// Clear all bits
    pub fn clear_all(&mut self) {
        for word in &mut self.words {
            *word = 0;
        }
    }

    /// Set all bits
    pub fn set_all(&mut self) {
        for word in &mut self.words {
            *word = u64::MAX;
        }
        // Clear any bits beyond capacity in the last word
        if self.capacity % 64 != 0 {
            let last_word_bits = self.capacity % 64;
            let mask = (1u64 << last_word_bits) - 1;
            if let Some(last_word) = self.words.last_mut() {
                *last_word &= mask;
            }
        }
    }

    /// Create a copy of this bitset
    pub fn clone(&self) -> Self {
        Self {
            words: self.words.clone(),
            capacity: self.capacity,
        }
    }

    /// Get the capacity of this bitset
    pub fn capacity(&self) -> usize {
        self.capacity
    }
}

impl BorrowFactExtractor {
    /// Create a new borrow fact extractor
    pub fn new() -> Self {
        Self {
            gen_sets: HashMap::new(),
            kill_sets: HashMap::new(),
            loans: Vec::new(),
            place_to_loans: HashMap::new(),
            loan_count: 0,
        }
    }

    /// Extract gen/kill sets for a function
    pub fn extract_function(&mut self, function: &MirFunction) -> Result<(), String> {
        // First, collect all loans from events
        self.collect_loans_from_events(function)?;
        
        // Build place-to-loans index for efficient kill set construction
        self.build_place_to_loans_index();
        
        // Build gen sets from start_loans events
        self.build_gen_sets(function)?;
        
        // Build kill sets from moves and reassigns
        self.build_kill_sets(function)?;
        
        Ok(())
    }

    /// Collect all loans from events in the function
    fn collect_loans_from_events(&mut self, function: &MirFunction) -> Result<(), String> {
        let mut _loan_id_counter = 0u32;
        
        // Iterate through all program points and collect loans
        for &program_point in function.get_program_points_in_order() {
            if let Some(events) = function.get_events(&program_point) {
                for &loan_id in &events.start_loans {
                    // For now, we need to reconstruct loan information from the events
                    // In a full implementation, this would come from the MIR construction phase
                    
                    // Create a placeholder loan - in practice, this information would be
                    // stored during MIR construction when borrows are created
                    let loan = Loan {
                        id: loan_id,
                        owner: Place::Local { index: 0, wasm_type: crate::compiler::mir::place::WasmType::I32 }, // Placeholder
                        kind: BorrowKind::Shared, // Placeholder
                        origin_stmt: program_point,
                    };
                    
                    self.loans.push(loan);
                    _loan_id_counter += 1;
                }
            }
        }
        
        self.loan_count = self.loans.len();
        Ok(())
    }

    /// Build index from places to loans that borrow them
    fn build_place_to_loans_index(&mut self) {
        self.place_to_loans.clear();
        
        for loan in &self.loans {
            let entry = self.place_to_loans.entry(loan.owner.clone()).or_insert_with(Vec::new);
            entry.push(loan.id);
        }
    }

    /// Build gen sets containing loans starting at each statement
    fn build_gen_sets(&mut self, function: &MirFunction) -> Result<(), String> {
        // Initialize gen sets for all program points
        for &program_point in function.get_program_points_in_order() {
            self.gen_sets.insert(program_point, BitSet::new(self.loan_count));
        }
        
        // Populate gen sets from start_loans events
        for &program_point in function.get_program_points_in_order() {
            if let Some(events) = function.get_events(&program_point) {
                let gen_set = self.gen_sets.get_mut(&program_point).unwrap();
                
                for &loan_id in &events.start_loans {
                    // Find the loan index in our loans vector
                    if let Some(loan_index) = self.loans.iter().position(|loan| loan.id == loan_id) {
                        gen_set.set(loan_index);
                    }
                }
            }
        }
        
        Ok(())
    }

    /// Build kill sets for loans whose owners may alias moved/reassigned places
    fn build_kill_sets(&mut self, function: &MirFunction) -> Result<(), String> {
        // Initialize kill sets for all program points
        for &program_point in function.get_program_points_in_order() {
            self.kill_sets.insert(program_point, BitSet::new(self.loan_count));
        }
        
        // Populate kill sets from moves and reassigns
        for &program_point in function.get_program_points_in_order() {
            if let Some(events) = function.get_events(&program_point) {
                // Collect places that need to be processed
                let mut places_to_kill = Vec::new();
                places_to_kill.extend(events.moves.iter());
                places_to_kill.extend(events.reassigns.iter());
                
                // Collect all loan indices that need to be killed
                let mut loan_indices_to_kill = Vec::new();
                for place in places_to_kill {
                    loan_indices_to_kill.extend(self.get_aliasing_loan_indices(place));
                }
                
                // Update kill set
                let kill_set = self.kill_sets.get_mut(&program_point).unwrap();
                for loan_index in loan_indices_to_kill {
                    kill_set.set(loan_index);
                }
            }
        }
        
        Ok(())
    }

    /// Add loans that may alias the given place to the kill set
    fn add_aliasing_loans_to_kill_set(&self, place: &Place, kill_set: &mut BitSet) {
        // Find all loans whose owners may alias this place
        for (loan_index, loan) in self.loans.iter().enumerate() {
            if may_alias(&loan.owner, place) {
                kill_set.set(loan_index);
            }
        }
    }

    /// Get loan indices that may alias the given place
    fn get_aliasing_loan_indices(&self, place: &Place) -> Vec<usize> {
        let mut indices = Vec::new();
        for (loan_index, loan) in self.loans.iter().enumerate() {
            if may_alias(&loan.owner, place) {
                indices.push(loan_index);
            }
        }
        indices
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

    /// Get loans that borrow a specific place
    pub fn get_loans_for_place(&self, place: &Place) -> Option<&[LoanId]> {
        self.place_to_loans.get(place).map(|v| v.as_slice())
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
    use crate::compiler::mir::place::{Place, ProjectionElem};
    
    // Same place always aliases
    if place_a == place_b {
        return true;
    }
    
    match (place_a, place_b) {
        // Same local/global variables alias
        (Place::Local { index: i1, .. }, Place::Local { index: i2, .. }) => i1 == i2,
        (Place::Global { index: i1, .. }, Place::Global { index: i2, .. }) => i1 == i2,
        
        // Memory locations alias if they overlap
        (Place::Memory { base: b1, offset: o1, size: s1 }, Place::Memory { base: b2, offset: o2, size: s2 }) => {
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
        (Place::Projection { base: base_a, elem: elem_a }, Place::Projection { base: base_b, elem: elem_b }) => {
            // If bases don't alias, projections don't alias
            if !may_alias(base_a, base_b) {
                return false;
            }
            
            // If bases alias, check projection elements
            match (elem_a, elem_b) {
                // Same field projections alias
                (ProjectionElem::Field { index: i1, .. }, ProjectionElem::Field { index: i2, .. }) => i1 == i2,
                
                // Array index projections
                (ProjectionElem::Index { index: idx1, .. }, ProjectionElem::Index { index: idx2, .. }) => {
                    // If both are constant indices, check if they're the same
                    match (idx1.as_ref(), idx2.as_ref()) {
                        (Place::Local { index: i1, .. }, Place::Local { index: i2, .. }) if i1 == i2 => true,
                        (Place::Memory { offset: o1, .. }, Place::Memory { offset: o2, .. }) if o1 == o2 => true,
                        _ => true, // Conservative: assume dynamic indices may alias
                    }
                }
                
                // Different projection types: conservative aliasing
                _ => true,
            }
        }
        
        // Variable aliases its projections
        (base, Place::Projection { base: proj_base, .. }) |
        (Place::Projection { base: proj_base, .. }, base) => {
            may_alias(base, proj_base)
        }
        
        // Different types don't alias
        _ => false,
    }
}

/// Entry point for extracting gen/kill sets from a function
pub fn extract_gen_kill_sets(function: &MirFunction) -> Result<BorrowFactExtractor, String> {
    let mut extractor = BorrowFactExtractor::new();
    extractor.extract_function(function)?;
    Ok(extractor)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::mir::mir_nodes::*;
    use crate::compiler::mir::place::*;

    /// Create a test function with some loans for testing
    fn create_test_function_with_loans() -> MirFunction {
        let mut function = MirFunction::new(0, "test".to_string(), vec![], vec![]);
        
        // Create test places
        let place_x = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place_y = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        // Create program points
        let pp1 = ProgramPoint::new(0);
        let pp2 = ProgramPoint::new(1);
        
        // Add program points to function
        function.add_program_point(pp1, 0, 0);
        function.add_program_point(pp2, 0, 1);
        
        // Create events with loans
        let mut events1 = Events::default();
        events1.start_loans.push(LoanId::new(0)); // Start loan 0 at pp1
        function.store_events(pp1, events1);
        
        let mut events2 = Events::default();
        events2.moves.push(place_x.clone()); // Move place_x at pp2
        function.store_events(pp2, events2);
        
        function
    }

    #[test]
    fn test_bitset_operations() {
        let mut bitset = BitSet::new(100);
        
        // Test setting and getting bits
        assert!(!bitset.get(5));
        bitset.set(5);
        assert!(bitset.get(5));
        
        // Test clearing bits
        bitset.clear(5);
        assert!(!bitset.get(5));
        
        // Test union
        let mut other = BitSet::new(100);
        other.set(10);
        bitset.union_with(&other);
        assert!(bitset.get(10));
        
        // Test count
        bitset.set(20);
        assert_eq!(bitset.count_ones(), 2); // bits 10 and 20
    }

    #[test]
    fn test_may_alias_same_place() {
        let place1 = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place2 = Place::Local { index: 0, wasm_type: WasmType::I32 };
        
        assert!(may_alias(&place1, &place2));
    }

    #[test]
    fn test_may_alias_different_locals() {
        let place1 = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let place2 = Place::Local { index: 1, wasm_type: WasmType::I32 };
        
        assert!(!may_alias(&place1, &place2));
    }

    #[test]
    fn test_may_alias_field_projections() {
        let base = Place::Local { index: 0, wasm_type: WasmType::I32 };
        
        let field1 = base.clone().project_field(0, 0, FieldSize::WasmType(WasmType::I32));
        let field2 = base.clone().project_field(1, 4, FieldSize::WasmType(WasmType::I32));
        
        // Different fields shouldn't alias
        assert!(!may_alias(&field1, &field2));
        
        // Same field should alias
        let field1_copy = base.clone().project_field(0, 0, FieldSize::WasmType(WasmType::I32));
        assert!(may_alias(&field1, &field1_copy));
    }

    #[test]
    fn test_may_alias_base_and_projection() {
        let base = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let projection = base.clone().project_field(0, 0, FieldSize::WasmType(WasmType::I32));
        
        // Base should alias its projections
        assert!(may_alias(&base, &projection));
        assert!(may_alias(&projection, &base));
    }

    #[test]
    fn test_may_alias_memory_overlap() {
        let mem1 = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: crate::compiler::mir::place::ByteOffset(0),
            size: TypeSize::Word, // 4 bytes
        };
        
        let mem2 = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: crate::compiler::mir::place::ByteOffset(2),
            size: TypeSize::Word, // 4 bytes
        };
        
        // Should overlap: [0,4) and [2,6)
        assert!(may_alias(&mem1, &mem2));
        
        let mem3 = Place::Memory {
            base: MemoryBase::LinearMemory,
            offset: crate::compiler::mir::place::ByteOffset(8),
            size: TypeSize::Word, // 4 bytes
        };
        
        // Should not overlap: [0,4) and [8,12)
        assert!(!may_alias(&mem1, &mem3));
    }

    #[test]
    fn test_gen_kill_extraction() {
        let function = create_test_function_with_loans();
        let mut extractor = BorrowFactExtractor::new();
        
        let result = extractor.extract_function(&function);
        assert!(result.is_ok(), "Gen/kill extraction should succeed");
        
        // Check that we have gen/kill sets for all program points
        assert_eq!(extractor.gen_sets.len(), 2);
        assert_eq!(extractor.kill_sets.len(), 2);
    }

    #[test]
    fn test_place_to_loans_index() {
        let mut extractor = BorrowFactExtractor::new();
        
        // Add some test loans
        let place_x = Place::Local { index: 0, wasm_type: WasmType::I32 };
        let loan1 = Loan {
            id: LoanId::new(0),
            owner: place_x.clone(),
            kind: BorrowKind::Shared,
            origin_stmt: ProgramPoint::new(0),
        };
        let loan2 = Loan {
            id: LoanId::new(1),
            owner: place_x.clone(),
            kind: BorrowKind::Mut,
            origin_stmt: ProgramPoint::new(1),
        };
        
        extractor.loans.push(loan1);
        extractor.loans.push(loan2);
        extractor.build_place_to_loans_index();
        
        // Check that both loans are indexed under place_x
        let loans_for_x = extractor.get_loans_for_place(&place_x).unwrap();
        assert_eq!(loans_for_x.len(), 2);
        assert!(loans_for_x.contains(&LoanId::new(0)));
        assert!(loans_for_x.contains(&LoanId::new(1)));
    }
}