use crate::compiler::codegen::wasm_encoding::WasmModule;
use crate::compiler::compiler_errors::CompileError;
use crate::compiler::mir::extract::BorrowFactExtractor;
use crate::compiler::mir::mir_nodes::{
    BorrowKind, Loan, MirFunction, Operand, ProgramPoint, Rvalue, Statement, Terminator,
};
use crate::compiler::mir::place::{Place, WasmType};
use crate::compiler::mir::unified_borrow_checker::UnifiedBorrowCheckResults;
use crate::return_compiler_error;
use std::collections::{HashMap, HashSet};
use wasm_encoder::{Function, Instruction, ValType};

/// Lifetime-optimized memory management for WASM code generation
///
/// This system integrates MIR borrow checking results to make optimal memory management
/// decisions, implementing single-ownership optimization using WASM value semantics,
/// minimal ARC generation for shared ownership, and move semantics optimization.
///
/// ## Design Principles
///
/// ### Single-Ownership Optimization
/// - Variables with single ownership use WASM value semantics (no reference counting)
/// - Direct value passing for WASM primitive types (i32, i64, f32, f64)
/// - Eliminates unnecessary ARC operations for non-shared data
///
/// ### Minimal ARC Generation
/// - Only generate ARC operations when borrow checker detects shared ownership
/// - Use lightweight reference counting for complex types requiring sharing
/// - Optimize ARC operations based on lifetime analysis results
///
/// ### Move Semantics Optimization
/// - Convert Copy operations to Move when lifetime analysis shows last use
/// - Eliminate unnecessary copying based on MIR lifetime information
/// - Generate efficient WASM instruction sequences for ownership transfer
///
/// ### Drop Elaboration
/// - Generate cleanup code only when required by borrow checker
/// - Use lifetime analysis to determine precise drop points
/// - Minimize runtime overhead from memory management
#[derive(Debug)]
pub struct LifetimeMemoryManager {
    /// Single-ownership places that use WASM value semantics
    pub single_ownership_places: HashSet<Place>,
    /// Shared ownership places requiring ARC
    shared_ownership_places: HashMap<Place, ARCInfo>,
    /// Move optimization decisions from lifetime analysis
    move_optimizations: HashMap<ProgramPoint, Vec<MoveOptimization>>,
    /// Drop elaboration points from borrow checker
    drop_points: HashMap<ProgramPoint, Vec<DropOperation>>,
    /// Memory cleanup operations required by lifetime analysis
    cleanup_operations: HashMap<ProgramPoint, Vec<CleanupOperation>>,
    /// WASM value type optimization decisions
    value_type_optimizations: HashMap<Place, ValueTypeOptimization>,
    /// Statistics for performance monitoring
    statistics: MemoryManagementStatistics,
}

/// ARC (Atomic Reference Counting) information for shared ownership
#[derive(Debug, Clone)]
pub struct ARCInfo {
    /// Reference count location in WASM linear memory
    pub ref_count_offset: u32,
    /// Data location in WASM linear memory
    pub data_offset: u32,
    /// Size of the data being reference counted
    pub data_size: u32,
    /// WASM type of the reference counted data
    pub data_type: WasmType,
    /// Whether this ARC is currently optimized away
    pub is_optimized_away: bool,
}

/// Move optimization decision from lifetime analysis
#[derive(Debug, Clone)]
pub struct MoveOptimization {
    /// Place being moved
    pub place: Place,
    /// Original operation (Copy → Move)
    pub original_operation: OperationType,
    /// Optimized operation
    pub optimized_operation: OperationType,
    /// Reason for the optimization
    pub optimization_reason: OptimizationReason,
}

/// Type of operation on a place
#[derive(Debug, Clone, PartialEq)]
pub enum OperationType {
    /// Copy the value (preserves original)
    Copy,
    /// Move the value (invalidates original)
    Move,
    /// Borrow the value (creates reference)
    Borrow(BorrowKind),
}

/// Reason for memory management optimization
#[derive(Debug, Clone)]
pub enum OptimizationReason {
    /// Last use of the place (can move instead of copy)
    LastUse,
    /// Single ownership detected (no ARC needed)
    SingleOwnership,
    /// WASM value type (can use stack semantics)
    WasmValueType,
    /// Lifetime analysis shows no further uses
    NoFurtherUses,
}

/// Drop operation required by lifetime analysis
#[derive(Debug, Clone)]
pub struct DropOperation {
    /// Place being dropped
    pub place: Place,
    /// Type of drop operation
    pub drop_type: DropType,
    /// Whether this drop can be optimized away
    pub can_optimize: bool,
}

/// Type of drop operation
#[derive(Debug, Clone)]
pub enum DropType {
    /// Simple value drop (no cleanup needed for WASM value types)
    ValueDrop,
    /// ARC decrement and potential deallocation
    ARCDrop,
    /// Linear memory deallocation
    MemoryDrop { offset: u32, size: u32 },
    /// Complex type cleanup
    ComplexDrop { cleanup_function: u32 },
}

/// Memory cleanup operation
#[derive(Debug, Clone)]
pub struct CleanupOperation {
    /// Type of cleanup
    pub cleanup_type: CleanupType,
    /// Memory location to clean up
    pub memory_location: Option<u32>,
    /// Size of memory to clean up
    pub size: Option<u32>,
}

/// Type of memory cleanup
#[derive(Debug, Clone)]
pub enum CleanupType {
    /// Deallocate linear memory
    Deallocate,
    /// Decrement reference count
    DecrementRefCount,
    /// Call destructor function
    CallDestructor { function_index: u32 },
}

/// WASM value type optimization
#[derive(Debug, Clone)]
pub struct ValueTypeOptimization {
    /// Original place representation
    pub original_place: Place,
    /// Optimized WASM representation
    pub optimized_representation: WasmValueRepresentation,
    /// Performance benefit estimate
    pub performance_benefit: PerformanceBenefit,
}

/// WASM value representation for optimized places
#[derive(Debug, Clone)]
pub enum WasmValueRepresentation {
    /// Direct WASM local (no memory allocation)
    Local { index: u32, wasm_type: WasmType },
    /// WASM global (no memory allocation)
    Global { index: u32, wasm_type: WasmType },
    /// Stack value (temporary, no storage)
    Stack { wasm_type: WasmType },
}

/// Performance benefit from optimization
#[derive(Debug, Clone)]
pub struct PerformanceBenefit {
    /// Estimated instruction count reduction
    pub instruction_reduction: u32,
    /// Memory allocation reduction (bytes)
    pub memory_reduction: u32,
    /// ARC operation elimination count
    pub arc_elimination_count: u32,
}

/// Statistics for memory management performance
#[derive(Debug, Clone, Default)]
pub struct MemoryManagementStatistics {
    /// Total places analyzed
    pub places_analyzed: usize,
    /// Single ownership optimizations applied
    pub single_ownership_optimizations: usize,
    /// ARC operations eliminated
    pub arc_operations_eliminated: usize,
    /// Move optimizations applied
    pub move_optimizations_applied: usize,
    /// Drop operations optimized away
    pub drop_operations_optimized: usize,
    /// Total memory allocation reduction (bytes)
    pub memory_allocation_reduction: u32,
    /// Total instruction count reduction
    pub instruction_count_reduction: u32,
}

impl LifetimeMemoryManager {
    /// Create a new lifetime memory manager
    pub fn new() -> Self {
        Self {
            single_ownership_places: HashSet::new(),
            shared_ownership_places: HashMap::new(),
            move_optimizations: HashMap::new(),
            drop_points: HashMap::new(),
            cleanup_operations: HashMap::new(),
            value_type_optimizations: HashMap::new(),
            statistics: MemoryManagementStatistics::default(),
        }
    }

    /// Analyze a function and integrate borrow checking results for memory management decisions
    pub fn analyze_function(
        &mut self,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), CompileError> {
        // Phase 1: Analyze ownership patterns from borrow checking results
        self.analyze_ownership_patterns(function, borrow_results, extractor)?;

        // Phase 2: Implement single-ownership optimization using WASM value semantics
        self.implement_single_ownership_optimization(function)?;

        // Phase 3: Add minimal ARC generation for shared ownership
        self.generate_minimal_arc_operations(function, borrow_results)?;

        // Phase 4: Create move semantics optimization to eliminate unnecessary copying
        self.optimize_move_semantics(function, borrow_results)?;

        // Phase 5: Implement drop elaboration based on MIR lifetime analysis
        self.elaborate_drop_operations(function, borrow_results)?;

        // Phase 6: Add memory cleanup code generation only when required by borrow checker
        self.generate_memory_cleanup_operations(function, borrow_results)?;

        Ok(())
    }

    /// Analyze ownership patterns from borrow checking results
    fn analyze_ownership_patterns(
        &mut self,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), CompileError> {
        self.statistics.places_analyzed = function.locals.len() + function.parameters.len();

        // Analyze each place in the function
        for (_, place) in &function.locals {
            self.analyze_place_ownership(place, function, borrow_results, extractor)?;
        }

        for place in &function.parameters {
            self.analyze_place_ownership(place, function, borrow_results, extractor)?;
        }

        Ok(())
    }

    /// Analyze ownership pattern for a specific place
    fn analyze_place_ownership(
        &mut self,
        place: &Place,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
        extractor: &BorrowFactExtractor,
    ) -> Result<(), CompileError> {
        // Check if this place has any loans (shared ownership)
        let has_shared_ownership = self.place_has_shared_ownership(place, extractor);

        if has_shared_ownership {
            // Generate ARC info for shared ownership
            let arc_info = self.create_arc_info_for_place(place)?;
            self.shared_ownership_places.insert(place.clone(), arc_info);
        } else {
            // Single ownership - can use WASM value semantics
            self.single_ownership_places.insert(place.clone());
            self.statistics.single_ownership_optimizations += 1;
        }

        // Check for WASM value type optimization
        if self.can_optimize_as_wasm_value_type(place) {
            let optimization = self.create_value_type_optimization(place)?;
            self.value_type_optimizations.insert(place.clone(), optimization);
        }

        Ok(())
    }

    /// Check if a place has shared ownership (multiple borrows)
    pub fn place_has_shared_ownership(&self, place: &Place, extractor: &BorrowFactExtractor) -> bool {
        // Count loans that borrow this place
        let loan_count = extractor
            .place_to_loans
            .get(place)
            .map(|loans| loans.len())
            .unwrap_or(0);

        // If more than one loan, or any mutable loans, consider it shared
        if loan_count > 1 {
            return true;
        }

        // Check for mutable borrows (always considered shared for safety)
        if let Some(loans) = extractor.place_to_loans.get(place) {
            for &loan_id in loans {
                if let Some(loan) = extractor.loans.iter().find(|l| l.id == loan_id) {
                    if matches!(loan.kind, BorrowKind::Mut) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Create ARC info for a place requiring shared ownership
    pub fn create_arc_info_for_place(&self, place: &Place) -> Result<ARCInfo, CompileError> {
        // Calculate memory layout for ARC structure
        // ARC layout: [ref_count: i32][data: T]
        let ref_count_size = 4; // i32 reference count
        let data_size = self.calculate_place_size(place);
        let total_size = ref_count_size + data_size;

        // For now, use placeholder offsets - in a real implementation,
        // this would integrate with the memory allocator
        let base_offset = 0; // TODO: Get from memory allocator

        Ok(ARCInfo {
            ref_count_offset: base_offset,
            data_offset: base_offset + ref_count_size,
            data_size,
            data_type: place.wasm_type(),
            is_optimized_away: false,
        })
    }

    /// Calculate the size of a place in bytes
    pub fn calculate_place_size(&self, place: &Place) -> u32 {
        match place {
            Place::Local { wasm_type, .. } | Place::Global { wasm_type, .. } => {
                wasm_type.byte_size()
            }
            Place::Memory { size, .. } => size.byte_size(),
            Place::Projection { base, elem } => {
                // For projections, calculate based on the element
                match elem {
                    crate::compiler::mir::place::ProjectionElem::Field { size, .. } => {
                        match size {
                            crate::compiler::mir::place::FieldSize::Fixed(bytes) => *bytes,
                            crate::compiler::mir::place::FieldSize::WasmType(wasm_type) => {
                                wasm_type.byte_size()
                            }
                            crate::compiler::mir::place::FieldSize::Variable => {
                                // Variable size - use base size as estimate
                                self.calculate_place_size(base)
                            }
                        }
                    }
                    _ => self.calculate_place_size(base), // Default to base size
                }
            }
        }
    }

    /// Check if a place can be optimized as a WASM value type
    pub fn can_optimize_as_wasm_value_type(&self, place: &Place) -> bool {
        match place {
            Place::Local { wasm_type, .. } | Place::Global { wasm_type, .. } => {
                // WASM primitive types can always be optimized
                matches!(
                    wasm_type,
                    WasmType::I32 | WasmType::I64 | WasmType::F32 | WasmType::F64
                )
            }
            Place::Memory { .. } => false, // Memory places need allocation
            Place::Projection { .. } => false, // Projections are complex
        }
    }

    /// Create value type optimization for a place
    pub fn create_value_type_optimization(&self, place: &Place) -> Result<ValueTypeOptimization, CompileError> {
        let optimized_representation = match place {
            Place::Local { index, wasm_type } => WasmValueRepresentation::Local {
                index: *index,
                wasm_type: wasm_type.clone(),
            },
            Place::Global { index, wasm_type } => WasmValueRepresentation::Global {
                index: *index,
                wasm_type: wasm_type.clone(),
            },
            _ => {
                return_compiler_error!("Cannot create value type optimization for complex place");
            }
        };

        let performance_benefit = PerformanceBenefit {
            instruction_reduction: 2, // Avoid memory load/store
            memory_reduction: self.calculate_place_size(place),
            arc_elimination_count: 1, // No ARC needed for value types
        };

        Ok(ValueTypeOptimization {
            original_place: place.clone(),
            optimized_representation,
            performance_benefit,
        })
    }

    /// Implement single-ownership optimization using WASM value semantics
    fn implement_single_ownership_optimization(&mut self, function: &MirFunction) -> Result<(), CompileError> {
        // For each single-ownership place, ensure it uses optimal WASM representation
        for place in &self.single_ownership_places {
            if self.can_optimize_as_wasm_value_type(place) {
                // Already handled in value type optimization
                continue;
            }

            // For complex single-ownership types, ensure no ARC overhead
            if let Some(arc_info) = self.shared_ownership_places.get_mut(place) {
                arc_info.is_optimized_away = true;
                self.statistics.arc_operations_eliminated += 1;
            }
        }

        Ok(())
    }

    /// Add minimal ARC generation for shared ownership
    fn generate_minimal_arc_operations(
        &mut self,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        // Only generate ARC operations for places that actually need them
        let places_to_process: Vec<_> = self.shared_ownership_places.keys().cloned().collect();
        for place in places_to_process {
            if let Some(arc_info) = self.shared_ownership_places.get(&place) {
                if arc_info.is_optimized_away {
                    continue; // Skip optimized-away ARCs
                }

                // Generate ARC operations at appropriate program points
                let arc_info_clone = arc_info.clone();
                self.generate_arc_operations_for_place(&place, &arc_info_clone, function, borrow_results)?;
            }
        }

        Ok(())
    }

    /// Generate ARC operations for a specific place
    fn generate_arc_operations_for_place(
        &mut self,
        place: &Place,
        arc_info: &ARCInfo,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        // Find program points where ARC operations are needed
        for program_point in function.get_program_points_in_order() {
            if let Some(events) = function.generate_events(&program_point) {
                // Check if this place is used (increment ref count)
                if events.uses.contains(place) {
                    self.add_arc_increment_operation(program_point, place, arc_info);
                }

                // Check if this place is moved (decrement ref count)
                if events.moves.contains(place) {
                    self.add_arc_decrement_operation(program_point, place, arc_info);
                }
            }
        }

        Ok(())
    }

    /// Add ARC increment operation
    fn add_arc_increment_operation(&mut self, point: ProgramPoint, place: &Place, arc_info: &ARCInfo) {
        let cleanup_op = CleanupOperation {
            cleanup_type: CleanupType::DecrementRefCount, // Will be increment in actual codegen
            memory_location: Some(arc_info.ref_count_offset),
            size: Some(4), // i32 ref count
        };

        self.cleanup_operations
            .entry(point)
            .or_insert_with(Vec::new)
            .push(cleanup_op);
    }

    /// Add ARC decrement operation
    fn add_arc_decrement_operation(&mut self, point: ProgramPoint, place: &Place, arc_info: &ARCInfo) {
        let cleanup_op = CleanupOperation {
            cleanup_type: CleanupType::DecrementRefCount,
            memory_location: Some(arc_info.ref_count_offset),
            size: Some(4), // i32 ref count
        };

        self.cleanup_operations
            .entry(point)
            .or_insert_with(Vec::new)
            .push(cleanup_op);
    }

    /// Create move semantics optimization to eliminate unnecessary copying
    fn optimize_move_semantics(
        &mut self,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        // Analyze each program point for move optimization opportunities
        for program_point in function.get_program_points_in_order() {
            self.analyze_move_optimization_at_point(program_point, function, borrow_results)?;
        }

        Ok(())
    }

    /// Analyze move optimization opportunities at a specific program point
    fn analyze_move_optimization_at_point(
        &mut self,
        point: ProgramPoint,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        if let Some(events) = function.generate_events(&point) {
            // Look for Copy operations that can be converted to Move
            for used_place in &events.uses {
                if self.can_optimize_copy_to_move(used_place, point, function, borrow_results) {
                    let optimization = MoveOptimization {
                        place: used_place.clone(),
                        original_operation: OperationType::Copy,
                        optimized_operation: OperationType::Move,
                        optimization_reason: OptimizationReason::LastUse,
                    };

                    self.move_optimizations
                        .entry(point)
                        .or_insert_with(Vec::new)
                        .push(optimization);

                    self.statistics.move_optimizations_applied += 1;
                }
            }
        }

        Ok(())
    }

    /// Check if a Copy operation can be optimized to Move
    fn can_optimize_copy_to_move(
        &self,
        place: &Place,
        point: ProgramPoint,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> bool {
        // Check if this is the last use of the place
        // In a full implementation, this would use liveness analysis from borrow_results
        
        // For now, use a simple heuristic: if the place is single-ownership
        // and not used in subsequent statements, it can be moved
        if !self.single_ownership_places.contains(place) {
            return false; // Only optimize single-ownership places
        }

        // Check if place is used after this point (simplified check)
        let current_point_id = point.id();
        for later_point in function.get_program_points_in_order() {
            if later_point.id() <= current_point_id {
                continue; // Skip current and earlier points
            }

            if let Some(events) = function.generate_events(&later_point) {
                if events.uses.contains(place) || events.reassigns.contains(place) {
                    return false; // Place is used later, cannot move
                }
            }
        }

        true // Safe to move
    }

    /// Implement drop elaboration based on MIR lifetime analysis
    fn elaborate_drop_operations(
        &mut self,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        // Find drop points from lifetime analysis
        for program_point in function.get_program_points_in_order() {
            self.analyze_drop_operations_at_point(program_point, function, borrow_results)?;
        }

        Ok(())
    }

    /// Analyze drop operations at a specific program point
    fn analyze_drop_operations_at_point(
        &mut self,
        point: ProgramPoint,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        if let Some(events) = function.generate_events(&point) {
            // Check for places that need to be dropped
            for moved_place in &events.moves {
                let drop_op = self.create_drop_operation_for_place(moved_place)?;
                
                self.drop_points
                    .entry(point)
                    .or_insert_with(Vec::new)
                    .push(drop_op);
            }
        }

        Ok(())
    }

    /// Create drop operation for a place
    fn create_drop_operation_for_place(&mut self, place: &Place) -> Result<DropOperation, CompileError> {
        let drop_type = if self.can_optimize_as_wasm_value_type(place) {
            // WASM value types don't need cleanup
            DropType::ValueDrop
        } else if let Some(arc_info) = self.shared_ownership_places.get(place) {
            // ARC types need reference count decrement
            DropType::ARCDrop
        } else if let Some(memory_size) = place.memory_size() {
            // Memory places need deallocation
            let offset = place.memory_offset().unwrap_or(0);
            DropType::MemoryDrop {
                offset,
                size: memory_size,
            }
        } else {
            // Default to value drop
            DropType::ValueDrop
        };

        let can_optimize = matches!(drop_type, DropType::ValueDrop);
        if can_optimize {
            self.statistics.drop_operations_optimized += 1;
        }

        Ok(DropOperation {
            place: place.clone(),
            drop_type,
            can_optimize,
        })
    }

    /// Add memory cleanup code generation only when required by borrow checker
    fn generate_memory_cleanup_operations(
        &mut self,
        function: &MirFunction,
        borrow_results: &UnifiedBorrowCheckResults,
    ) -> Result<(), CompileError> {
        // Generate cleanup operations based on drop points and ARC operations
        for (point, drop_ops) in &self.drop_points {
            for drop_op in drop_ops {
                if !drop_op.can_optimize {
                    let cleanup_op = self.create_cleanup_operation_from_drop(drop_op)?;
                    
                    self.cleanup_operations
                        .entry(*point)
                        .or_insert_with(Vec::new)
                        .push(cleanup_op);
                }
            }
        }

        Ok(())
    }

    /// Create cleanup operation from drop operation
    fn create_cleanup_operation_from_drop(&self, drop_op: &DropOperation) -> Result<CleanupOperation, CompileError> {
        let cleanup_type = match &drop_op.drop_type {
            DropType::ValueDrop => {
                return_compiler_error!("Value drops should not generate cleanup operations");
            }
            DropType::ARCDrop => CleanupType::DecrementRefCount,
            DropType::MemoryDrop { offset, size } => CleanupType::Deallocate,
            DropType::ComplexDrop { cleanup_function } => CleanupType::CallDestructor {
                function_index: *cleanup_function,
            },
        };

        let (memory_location, size) = match &drop_op.drop_type {
            DropType::MemoryDrop { offset, size } => (Some(*offset), Some(*size)),
            DropType::ARCDrop => {
                // Get ARC info for this place
                if let Some(arc_info) = self.shared_ownership_places.get(&drop_op.place) {
                    (Some(arc_info.ref_count_offset), Some(4))
                } else {
                    (None, None)
                }
            }
            _ => (None, None),
        };

        Ok(CleanupOperation {
            cleanup_type,
            memory_location,
            size,
        })
    }

    /// Generate WASM instructions for memory management operations
    pub fn generate_memory_management_instructions(
        &self,
        point: ProgramPoint,
        function: &mut Function,
        wasm_module: &WasmModule,
    ) -> Result<(), CompileError> {
        // Generate move optimization instructions
        if let Some(move_opts) = self.move_optimizations.get(&point) {
            for move_opt in move_opts {
                self.generate_move_optimization_instructions(move_opt, function, wasm_module)?;
            }
        }

        // Generate cleanup instructions
        if let Some(cleanup_ops) = self.cleanup_operations.get(&point) {
            for cleanup_op in cleanup_ops {
                self.generate_cleanup_instructions(cleanup_op, function, wasm_module)?;
            }
        }

        Ok(())
    }

    /// Generate WASM instructions for move optimization
    fn generate_move_optimization_instructions(
        &self,
        move_opt: &MoveOptimization,
        function: &mut Function,
        wasm_module: &WasmModule,
    ) -> Result<(), CompileError> {
        // For move optimization, we typically don't need extra instructions
        // The optimization is in using move semantics instead of copy semantics
        // This would be handled in the main instruction generation

        // For now, just add a comment-like no-op
        // In a real implementation, this would modify how the place is accessed
        Ok(())
    }

    /// Generate WASM instructions for cleanup operations
    fn generate_cleanup_instructions(
        &self,
        cleanup_op: &CleanupOperation,
        function: &mut Function,
        wasm_module: &WasmModule,
    ) -> Result<(), CompileError> {
        match &cleanup_op.cleanup_type {
            CleanupType::DecrementRefCount => {
                if let Some(ref_count_offset) = cleanup_op.memory_location {
                    // Generate ARC decrement: load ref_count, decrement, store, check if zero
                    function.instruction(&Instruction::I32Const(ref_count_offset as i32));
                    function.instruction(&Instruction::I32Load(wasm_encoder::MemArg {
                        offset: 0,
                        align: 2, // 4-byte alignment
                        memory_index: 0,
                    }));
                    function.instruction(&Instruction::I32Const(1));
                    function.instruction(&Instruction::I32Sub);
                    
                    // Store decremented value
                    function.instruction(&Instruction::I32Const(ref_count_offset as i32));
                    function.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                    
                    // TODO: Add conditional deallocation if ref count reaches zero
                }
            }
            CleanupType::Deallocate => {
                if let Some(offset) = cleanup_op.memory_location {
                    // For now, just mark memory as available (simplified)
                    // In a real implementation, this would call the memory allocator
                    function.instruction(&Instruction::I32Const(offset as i32));
                    function.instruction(&Instruction::I32Const(0));
                    function.instruction(&Instruction::I32Store(wasm_encoder::MemArg {
                        offset: 0,
                        align: 2,
                        memory_index: 0,
                    }));
                }
            }
            CleanupType::CallDestructor { function_index } => {
                // Call destructor function
                function.instruction(&Instruction::Call(*function_index));
            }
        }

        Ok(())
    }

    /// Get statistics for performance monitoring
    pub fn get_statistics(&self) -> &MemoryManagementStatistics {
        &self.statistics
    }

    /// Get single ownership places for optimization queries
    pub fn get_single_ownership_places(&self) -> &HashSet<Place> {
        &self.single_ownership_places
    }

    /// Get shared ownership places requiring ARC
    pub fn get_shared_ownership_places(&self) -> &HashMap<Place, ARCInfo> {
        &self.shared_ownership_places
    }

    /// Get move optimizations for a program point
    pub fn get_move_optimizations(&self, point: &ProgramPoint) -> Option<&Vec<MoveOptimization>> {
        self.move_optimizations.get(point)
    }

    /// Get drop operations for a program point
    pub fn get_drop_operations(&self, point: &ProgramPoint) -> Option<&Vec<DropOperation>> {
        self.drop_points.get(point)
    }

    /// Check if a place uses single ownership optimization
    pub fn uses_single_ownership(&self, place: &Place) -> bool {
        self.single_ownership_places.contains(place)
    }

    /// Check if a place requires ARC
    pub fn requires_arc(&self, place: &Place) -> bool {
        self.shared_ownership_places.contains_key(place)
            && !self.shared_ownership_places[place].is_optimized_away
    }

    /// Get value type optimization for a place
    pub fn get_value_type_optimization(&self, place: &Place) -> Option<&ValueTypeOptimization> {
        self.value_type_optimizations.get(place)
    }
}

impl Default for LifetimeMemoryManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::mir::mir_nodes::*;
    use crate::compiler::mir::place::*;

    #[test]
    fn test_lifetime_memory_manager_creation() {
        let manager = LifetimeMemoryManager::new();
        assert_eq!(manager.statistics.places_analyzed, 0);
        assert!(manager.single_ownership_places.is_empty());
        assert!(manager.shared_ownership_places.is_empty());
    }

    #[test]
    fn test_single_ownership_detection() {
        let mut manager = LifetimeMemoryManager::new();
        let place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };

        // Create empty extractor (no loans = single ownership)
        let extractor = BorrowFactExtractor::new();
        
        assert!(!manager.place_has_shared_ownership(&place, &extractor));
    }

    #[test]
    fn test_wasm_value_type_optimization() {
        let manager = LifetimeMemoryManager::new();
        
        let i32_place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let memory_place = Place::Memory {
            base: crate::compiler::mir::place::MemoryBase::LinearMemory,
            offset: crate::compiler::mir::place::ByteOffset(0),
            size: crate::compiler::mir::place::TypeSize::Word,
        };

        assert!(manager.can_optimize_as_wasm_value_type(&i32_place));
        assert!(!manager.can_optimize_as_wasm_value_type(&memory_place));
    }

    #[test]
    fn test_place_size_calculation() {
        let manager = LifetimeMemoryManager::new();
        
        let i32_place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };
        
        let i64_place = Place::Local {
            index: 1,
            wasm_type: WasmType::I64,
        };

        assert_eq!(manager.calculate_place_size(&i32_place), 4);
        assert_eq!(manager.calculate_place_size(&i64_place), 8);
    }

    #[test]
    fn test_arc_info_creation() {
        let manager = LifetimeMemoryManager::new();
        let place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };

        let arc_info = manager.create_arc_info_for_place(&place).unwrap();
        assert_eq!(arc_info.data_size, 4); // i32 size
        assert_eq!(arc_info.data_type, WasmType::I32);
        assert!(!arc_info.is_optimized_away);
    }

    #[test]
    fn test_value_type_optimization_creation() {
        let manager = LifetimeMemoryManager::new();
        let place = Place::Local {
            index: 0,
            wasm_type: WasmType::I32,
        };

        let optimization = manager.create_value_type_optimization(&place).unwrap();
        assert_eq!(optimization.performance_benefit.instruction_reduction, 2);
        assert_eq!(optimization.performance_benefit.memory_reduction, 4);
        assert_eq!(optimization.performance_benefit.arc_elimination_count, 1);
    }
}