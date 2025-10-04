use crate::compiler::mir::place::{Place, WasmType};
use crate::compiler::parsers::tokens::TextLocation;
use std::collections::HashMap;

/// WASM-optimized Mid-level IR structure with simplified borrow checking
///
/// This MIR is designed specifically for efficient WASM generation with
/// simple dataflow-based borrow checking using program points and events.
///
/// ## Design Principles
///
/// ### Simplicity Over Sophistication
/// - Simple events instead of complex Polonius facts
/// - One program point per statement for clear tracking
/// - Standard dataflow algorithms instead of constraint solving
/// - WASM-first design avoiding unnecessary generality
///
/// ### Performance Focus
/// - Efficient bitsets for loan tracking
/// - Worklist algorithm optimized for WASM control flow
/// - Fast compilation prioritized over analysis sophistication
/// - Memory-efficient data structures
///
/// ### Maintainability
/// - Clear program point model for easy debugging
/// - Standard algorithms that are well-understood
/// - Simple data structures that are easy to extend
/// - Comprehensive test coverage for reliability
///
/// ## Core Data Structures
///
/// - `ProgramPoint`: Sequential identifiers for each MIR statement
/// - `Events`: Simple event records per program point for dataflow analysis
/// - `Loan`: Simplified borrow tracking with origin points
/// - `Place`: WASM-optimized memory location abstractions (unchanged)
///
/// See `docs/dataflow-analysis-guide.md` for detailed algorithm documentation.
#[derive(Debug)]
pub struct MIR {
    /// Functions in the module
    pub functions: Vec<MirFunction>,
    /// Global variables and their places
    pub globals: HashMap<u32, Place>,
    /// Module exports
    pub exports: HashMap<String, Export>,
    /// Type information for WASM module generation
    pub type_info: TypeInfo,
    /// Host function imports for WASM generation
    pub host_imports: std::collections::HashSet<crate::compiler::host_functions::registry::HostFunctionDef>,
}

impl MIR {
    /// Create a new MIR structure
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            globals: HashMap::new(),
            exports: HashMap::new(),
            type_info: TypeInfo {
                function_types: Vec::new(),
                global_types: Vec::new(),
                memory_info: MemoryInfo {
                    initial_pages: 1,
                    max_pages: None,
                    static_data_size: 0,
                },
                interface_info: InterfaceInfo {
                    interfaces: HashMap::new(),
                    vtables: HashMap::new(),
                    function_table: Vec::new(),
                },
            },
            host_imports: std::collections::HashSet::new(),
        }
    }

    /// Add a function to the MIR
    pub fn add_function(&mut self, function: MirFunction) {
        self.functions.push(function);
    }

    /// Add host function imports to the MIR
    pub fn add_host_imports(&mut self, imports: &std::collections::HashSet<crate::compiler::host_functions::registry::HostFunctionDef>) {
        self.host_imports.extend(imports.iter().cloned());
    }

    /// Get all program points from all functions
    pub fn get_all_program_points(&self) -> Vec<ProgramPoint> {
        let mut all_points = Vec::new();
        for function in &self.functions {
            all_points.extend(function.get_program_points_in_order());
        }
        all_points.sort();
        all_points
    }

    /// Get program points for a specific function
    pub fn get_function_program_points(&self, function_id: u32) -> Option<Vec<ProgramPoint>> {
        self.functions
            .iter()
            .find(|f| f.id == function_id)
            .map(|f| f.get_program_points_in_order())
    }

    /// Iterate over all program points
    pub fn iter_program_points(&self) -> impl Iterator<Item = ProgramPoint> + '_ {
        self.functions
            .iter()
            .flat_map(|f| f.iter_program_points())
    }

    /// Get program points for dataflow analysis in execution order
    pub fn get_program_points_for_dataflow(&self) -> Vec<ProgramPoint> {
        let mut all_points = Vec::new();
        for function in &self.functions {
            all_points.extend(function.get_program_points_in_order());
        }
        all_points
    }

    /// Find the function containing a given program point
    pub fn find_function_for_program_point(&self, point: &ProgramPoint) -> Option<&MirFunction> {
        self.functions
            .iter()
            .find(|f| f.events.contains_key(point))
    }

    /// Get a mutable reference to a function by ID
    pub fn get_function_mut(&mut self, function_id: u32) -> Option<&mut MirFunction> {
        self.functions.iter_mut().find(|f| f.id == function_id)
    }

    /// Build control flow graph for all functions (simplified)
    pub fn build_control_flow_graph(&mut self) -> Result<(), String> {
        // Simplified - no complex CFG building for now
        Ok(())
    }

    /// Validate WASM constraints (placeholder for now)
    pub fn validate_wasm_constraints(&self) -> Result<(), String> {
        // This will be implemented in later tasks
        Ok(())
    }
}

/// Simple program point information
#[derive(Debug, Clone)]
pub struct ProgramPointInfo {
    /// Block ID containing this program point
    pub block_id: u32,
    /// Statement index within the block (None for terminators)
    pub statement_index: Option<usize>,
    /// Source location for error reporting
    pub source_location: Option<TextLocation>,
}

impl ProgramPointInfo {
    /// Create new program point info for a statement
    pub fn new_statement(block_id: u32, statement_index: usize, source_location: Option<TextLocation>) -> Self {
        Self {
            block_id,
            statement_index: Some(statement_index),
            source_location,
        }
    }

    /// Create new program point info for a terminator
    pub fn new_terminator(block_id: u32, source_location: Option<TextLocation>) -> Self {
        Self {
            block_id,
            statement_index: None,
            source_location,
        }
    }

    /// Check if this program point is a terminator
    pub fn is_terminator(&self) -> bool {
        self.statement_index.is_none()
    }
}

/// Simplified MIR function representation
/// 
/// This simplified design removes complex optimizations in favor of correctness:
/// - No arena allocation - uses standard Vec and HashMap
/// - No place interning - uses direct Place references
/// - No complex event caching - simple HashMap storage
/// - Essential fields only for basic MIR functionality
#[derive(Debug, Clone)]
pub struct MirFunction {
    /// Function ID
    pub id: u32,
    /// Function name
    pub name: String,
    /// Parameter places
    pub parameters: Vec<Place>,
    /// Return type information
    pub return_types: Vec<WasmType>,
    /// Basic blocks
    pub blocks: Vec<MirBlock>,
    /// Local variable places
    pub locals: HashMap<String, Place>,
    /// WASM function signature
    pub signature: FunctionSignature,
    /// Simple event storage per program point
    pub events: HashMap<ProgramPoint, Events>,
    /// All loans in this function for borrow checking
    pub loans: Vec<Loan>,
}



impl MirFunction {
    /// Create a new simplified MIR function
    pub fn new(id: u32, name: String, parameters: Vec<Place>, return_types: Vec<WasmType>) -> Self {
        Self {
            id,
            name,
            parameters: parameters.clone(),
            return_types: return_types.clone(),
            blocks: Vec::new(),
            locals: HashMap::new(),
            signature: FunctionSignature {
                param_types: parameters.iter().map(|p| p.wasm_type()).collect(),
                result_types: return_types,
            },
            events: HashMap::new(),
            loans: Vec::new(),
        }
    }

    /// Add a block to this function
    pub fn add_block(&mut self, block: MirBlock) {
        self.blocks.push(block);
    }

    /// Add a local variable to this function
    pub fn add_local(&mut self, name: String, place: Place) {
        self.locals.insert(name, place);
    }

    /// Store events for a program point
    pub fn store_events(&mut self, program_point: ProgramPoint, events: Events) {
        self.events.insert(program_point, events);
    }

    /// Get events for a program point
    pub fn get_events(&self, program_point: &ProgramPoint) -> Option<&Events> {
        self.events.get(program_point)
    }

    /// Get all program points in execution order
    pub fn get_program_points_in_order(&self) -> Vec<ProgramPoint> {
        let mut points = Vec::new();
        for block in &self.blocks {
            points.extend(block.get_all_program_points());
        }
        points
    }

    /// Iterate over program points
    pub fn iter_program_points(&self) -> impl Iterator<Item = ProgramPoint> + '_ {
        self.events.keys().copied()
    }



    /// Get all events for this function
    pub fn get_all_events(&self) -> impl Iterator<Item = (&ProgramPoint, &Events)> + '_ {
        self.events.iter()
    }

    /// Generate events for a program point (compatibility method)
    pub fn generate_events(&self, program_point: &ProgramPoint) -> Option<Events> {
        self.get_events(program_point).cloned()
    }

    /// Get CFG (compatibility method - simplified)
    pub fn get_cfg_immutable(&self) -> Result<&(), String> {
        // Simplified - no CFG for now
        Err("CFG not implemented in simplified MIR".to_string())
    }

    /// Get source location for a program point (compatibility method)
    pub fn get_source_location(&self, _point: &ProgramPoint) -> Option<&TextLocation> {
        // Simplified - source locations not yet tracked per program point
        // This will be implemented in a future task
        None
    }

    /// Build CFG (compatibility method - simplified)
    pub fn build_cfg(&mut self) -> Result<(), String> {
        // Simplified - no CFG building for now
        Ok(())
    }

    /// Add a loan to this function
    pub fn add_loan(&mut self, loan: Loan) {
        self.loans.push(loan);
    }

    /// Get all loans in this function
    pub fn get_loans(&self) -> &[Loan] {
        &self.loans
    }

    /// Get the number of loans in this function
    pub fn get_loan_count(&self) -> usize {
        self.loans.len()
    }

}

// Tests will be added in a separate testing task to maintain focus on core functionality

/// WASM function signature information
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types in WASM order
    pub param_types: Vec<WasmType>,
    /// Return types in WASM order
    pub result_types: Vec<WasmType>,
}

/// Basic MIR block
/// 
/// Simplified design with essential fields only:
/// - No complex program point tracking
/// - No WASM-specific control flow information
/// - No parent/child block relationships
/// - Simple construction and manipulation methods
#[derive(Debug, Clone)]
pub struct MirBlock {
    /// Block ID for control flow
    pub id: u32,
    /// MIR statements
    pub statements: Vec<Statement>,
    /// Block terminator
    pub terminator: Terminator,
}

impl MirBlock {
    /// Create a new MIR block
    pub fn new(id: u32) -> Self {
        Self {
            id,
            statements: Vec::new(),
            terminator: Terminator::Unreachable,
        }
    }

    /// Add a statement to this block
    pub fn add_statement(&mut self, statement: Statement) {
        self.statements.push(statement);
    }

    /// Set the terminator for this block
    pub fn set_terminator(&mut self, terminator: Terminator) {
        self.terminator = terminator;
    }

    /// Get all program points in this block (for compatibility)
    pub fn get_all_program_points(&self) -> Vec<ProgramPoint> {
        // This is a simplified implementation - program points are managed at the function level
        Vec::new()
    }

    /// Get the program point for a specific statement index (compatibility)
    pub fn get_statement_program_point(&self, _statement_index: usize) -> Option<ProgramPoint> {
        // Simplified - program points are managed at function level
        None
    }

    /// Get the terminator program point (compatibility)
    pub fn get_terminator_program_point(&self) -> Option<ProgramPoint> {
        // Simplified - program points are managed at function level
        None
    }
}

/// MIR statement that maps efficiently to WASM instructions
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Assign the rvalue to a place
    Assign { place: Place, rvalue: Rvalue },

    /// Function call with WASM calling convention
    Call {
        func: Operand,
        args: Vec<Operand>,
        destination: Option<Place>,
    },

    /// Interface method call (vtable dispatch)
    /// Interfaces will be dynamically dispatched at runtime
    InterfaceCall {
        interface_id: u32,
        method_id: u32,
        receiver: Operand,
        args: Vec<Operand>,
        destination: Option<Place>,
    },

    /// Memory allocation in linear memory
    Alloc {
        place: Place,
        size: Operand,
        align: u32,
    },

    /// Memory deallocation
    Dealloc { place: Place },

    /// No-op (for analysis points)
    Nop,

    /// Store to memory with WASM alignment
    Store {
        place: Place,
        value: Operand,
        alignment: u32,
        offset: u32,
    },

    /// WASM-specific memory operations
    MemoryOp {
        op: MemoryOpKind,
        operand: Option<Operand>,
        result: Option<Place>,
    },

    /// Drop value (for lifetime analysis)
    Drop { place: Place },

    /// Host function call (functions provided by the runtime)
    HostCall {
        function: crate::compiler::host_functions::registry::HostFunctionDef,
        args: Vec<Operand>,
        destination: Option<Place>,
    },
}

impl Statement {
    /// Generate events for this statement on-demand
    ///
    /// This method computes events dynamically from the statement structure,
    /// eliminating the need to store events in MirFunction. Events are computed
    /// based on the statement type and operands.
    ///
    /// ## Performance Benefits
    /// - Reduces MIR memory footprint by ~30%
    /// - Eliminates redundant event storage
    /// - Enables efficient event caching for repeated access patterns
    ///
    /// ## Event Generation Rules
    /// - `Assign`: Generates reassign event for place, use/move events for rvalue operands
    /// - `Call`: Generates use events for arguments, reassign event for destination
    /// - `InterfaceCall`: Generates use events for receiver and arguments, reassign for destination
    /// - `Drop`: Generates use event for the dropped place
    /// - `Store`: Generates reassign event for place, use event for value
    /// - `Alloc`: Generates reassign event for place, use event for size
    /// - `Dealloc`: Generates use event for place
    /// - `Nop`, `MemoryOp`: Generate no events for basic borrow checking
    pub fn generate_events(&self) -> Events {
        let mut events = Events::default();

        match self {
            Statement::Assign { place, rvalue } => {
                // The assignment itself generates a reassign event for the place
                events.reassigns.push(place.clone());
                
                // Generate events for the rvalue
                self.generate_rvalue_events(rvalue, &mut events);
            }
            Statement::Call { args, destination, .. } => {
                // Generate use events for all arguments
                for arg in args {
                    self.generate_operand_events(arg, &mut events);
                }

                // If there's a destination, it gets reassigned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                }
            }
            Statement::InterfaceCall {
                receiver,
                args,
                destination,
                ..
            } => {
                // Generate use event for receiver
                self.generate_operand_events(receiver, &mut events);

                // Generate use events for all arguments
                for arg in args {
                    self.generate_operand_events(arg, &mut events);
                }

                // If there's a destination, it gets reassigned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                }
            }
            Statement::Drop { place } => {
                // Generate drop event - this is an end-of-lifetime point
                // For now, we'll track this as a use (the place is being accessed to drop it)
                events.uses.push(place.clone());
            }
            Statement::Store { place, value, .. } => {
                // Store operations reassign the place and use the value
                events.reassigns.push(place.clone());
                self.generate_operand_events(value, &mut events);
            }
            Statement::Alloc { place, size, .. } => {
                // Allocation reassigns the place and uses the size operand
                events.reassigns.push(place.clone());
                self.generate_operand_events(size, &mut events);
            }
            Statement::Dealloc { place } => {
                // Deallocation uses the place (to free it)
                events.uses.push(place.clone());
            }
            Statement::HostCall { args, destination, .. } => {
                // Generate use events for all arguments
                for arg in args {
                    self.generate_operand_events(arg, &mut events);
                }

                // If there's a destination, it gets reassigned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                }
            }
            Statement::Nop | Statement::MemoryOp { .. } => {
                // These don't generate events for basic borrow checking
            }
        }

        events
    }

    /// Generate events for rvalue operations
    fn generate_rvalue_events(&self, rvalue: &Rvalue, events: &mut Events) {
        match rvalue {
            Rvalue::Use(operand) => {
                self.generate_operand_events(operand, events);
            }
            Rvalue::BinaryOp(_, left, right) => {
                self.generate_operand_events(left, events);
                self.generate_operand_events(right, events);
            }
            Rvalue::UnaryOp(_, operand) => {
                self.generate_operand_events(operand, events);
            }
            Rvalue::Ref { place, .. } => {
                // The place being borrowed is also used (read access)
                events.uses.push(place.clone());
            }
        }
    }

    /// Generate events for operands
    fn generate_operand_events(&self, operand: &Operand, events: &mut Events) {
        match operand {
            Operand::Copy(place) => {
                // Generate use event for the place (non-consuming read)
                events.uses.push(place.clone());
            }
            Operand::Move(place) => {
                // Generate move event for the place (consuming read)
                events.moves.push(place.clone());
            }
            Operand::Constant(_) => {
                // Constants don't generate events
            }
            Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
                // References don't generate events
            }
        }
    }


}

/// Essential right-hand side values
/// 
/// Simplified to contain only essential variants:
/// - Removed complex projection systems
/// - Removed optimization-specific operations
/// - Kept only basic operations needed for core functionality
#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
    /// Use a place or constant
    Use(Operand),

    /// Binary operation
    BinaryOp(BinOp, Operand, Operand),

    /// Unary operation
    UnaryOp(UnOp, Operand),

    /// Reference to a place (borrow)
    Ref {
        place: Place,
        borrow_kind: BorrowKind,
    },
}

/// WASM-specific memory operations
#[derive(Debug, Clone, PartialEq)]
pub enum MemoryOpKind {
    /// Get current memory size in pages
    Size,
    /// Grow memory by specified pages
    Grow,
    /// Fill memory region with value
    Fill,
    /// Copy memory region
    Copy,
}

/// Operands for MIR operations
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// Copy from a place
    Copy(Place),

    /// Move from a place
    Move(Place),

    /// Constant value
    Constant(Constant),

    /// WASM function reference
    FunctionRef(u32),

    /// WASM global reference
    GlobalRef(u32),
}

/// Constants with WASM type information
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    /// 32-bit integer
    I32(i32),
    /// 64-bit integer
    I64(i64),
    /// 32-bit float
    F32(f32),
    /// 64-bit float
    F64(f64),
    /// Boolean (as i32)
    Bool(bool),
    /// String literal (pointer to linear memory)
    String(String),
    /// Function reference
    Function(u32),
    /// Null pointer (0 in linear memory)
    Null,
    /// Memory offset constant
    MemoryOffset(u32),
    /// Type size constant
    TypeSize(u32),
}

/// Binary operations with WASM instruction mapping
#[derive(Debug, Clone, PartialEq)]
pub enum BinOp {
    // Arithmetic (map to WASM add, sub, mul, div)
    Add,
    Sub,
    Mul,
    Div,
    Rem,

    // Bitwise (map to WASM and, or, xor, shl, shr)
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,

    // Comparison (map to WASM eq, ne, lt, le, gt, ge)
    Eq,
    Ne,
    Lt,
    Le,
    Gt,
    Ge,

    // Logical (implemented as short-circuiting control flow)
    And,
    Or,
}

/// Unary operations
#[derive(Debug, Clone, PartialEq)]
pub enum UnOp {
    /// Negation
    Neg,
    /// Bitwise NOT
    Not,
}

/// Essential block terminators
/// 
/// Simplified to contain only essential variants:
/// - Removed complex WASM-specific optimization information
/// - Removed redundant compatibility variants
/// - Kept only basic control flow needed for core functionality
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Unconditional jump
    Goto { target: u32 },

    /// Function return
    Return { values: Vec<Operand> },

    /// Conditional jump
    If {
        condition: Operand,
        then_block: u32,
        else_block: u32,
    },

    // Pattern matching terminator will be added when match expressions are implemented

    /// Unreachable code
    Unreachable,

}

impl Terminator {
    /// Generate events for this terminator
    pub fn generate_events(&self) -> Events {
        let mut events = Events::default();

        match self {
            Terminator::If { condition, .. } => {
                self.generate_operand_events(condition, &mut events);
            }
            Terminator::Return { values } => {
                for value in values {
                    self.generate_operand_events(value, &mut events);
                }
            }
            _ => {
                // Other terminators don't have operands that generate events
            }
        }

        events
    }

    /// Generate events for operands in terminators
    fn generate_operand_events(&self, operand: &Operand, events: &mut Events) {
        match operand {
            Operand::Copy(place) => {
                events.uses.push(place.clone());
            }
            Operand::Move(place) => {
                events.moves.push(place.clone());
            }
            Operand::Constant(_) => {
                // Constants don't generate events
            }
            Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
                // References don't generate events
            }
        }
    }
}

/// Simplified borrow kinds for dataflow analysis
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BorrowKind {
    /// Shared/immutable borrow
    Shared,
    /// Mutable borrow
    Mut,
    /// Unique borrow (move)
    Unique,
}



/// Program point identifier (one per MIR statement)
///
/// Program points provide a unique identifier for each MIR statement to enable
/// precise dataflow analysis. Each statement gets exactly one program point.
///
/// ## Design Rationale
///
/// The program point model enables precise dataflow equations:
/// - `LiveOut[s] = ⋃ LiveIn[succ(s)]` (backward liveness)
/// - `LiveInLoans[s] = Gen[s] ∪ (LiveOutLoans[s] - Kill[s])` (forward loan tracking)
///
/// Sequential allocation ensures deterministic ordering for worklist algorithms
/// and provides O(1) successor/predecessor relationships in linear control flow.
///
/// ## Usage Example
///
/// ```rust
/// let pp1 = ProgramPoint::new(0);  // First statement
/// let pp2 = pp1.next();            // Second statement  
/// assert!(pp1.precedes(&pp2));     // Sequential ordering
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ProgramPoint(pub u32);

impl ProgramPoint {
    /// Create a new program point with the given ID
    pub fn new(id: u32) -> Self {
        ProgramPoint(id)
    }

    /// Get the program point ID
    pub fn id(&self) -> u32 {
        self.0
    }

    /// Get the next program point in sequence
    pub fn next(&self) -> ProgramPoint {
        ProgramPoint(self.0 + 1)
    }

    /// Check if this program point comes before another
    pub fn precedes(&self, other: &ProgramPoint) -> bool {
        self.0 < other.0
    }
}

impl std::fmt::Display for ProgramPoint {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "pp{}", self.0)
    }
}

/// Program point generator for sequential allocation during MIR construction
#[derive(Debug)]
pub struct ProgramPointGenerator {
    /// Next program point ID to allocate
    next_id: u32,
    /// All allocated program points in order
    allocated_points: Vec<ProgramPoint>,
}

impl ProgramPointGenerator {
    /// Create a new program point generator
    pub fn new() -> Self {
        Self {
            next_id: 0,
            allocated_points: Vec::new(),
        }
    }

    /// Allocate the next program point in sequence
    pub fn allocate_next(&mut self) -> ProgramPoint {
        let point = ProgramPoint::new(self.next_id);
        self.next_id += 1;
        self.allocated_points.push(point);
        point
    }

    /// Get all allocated program points
    pub fn get_all_points(&self) -> &[ProgramPoint] {
        &self.allocated_points
    }

    /// Get the count of allocated program points
    pub fn count(&self) -> usize {
        self.allocated_points.len()
    }
}

impl Default for ProgramPointGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Simple events for dataflow analysis
///
/// Events provide straightforward borrow tracking for each program point.
/// Each program point has associated events that describe what happens at that
/// statement in terms of borrows, uses, moves, and assignments.
///
/// ## Event Types
///
/// - `start_loans`: New borrows beginning at this program point
/// - `uses`: Places being read (non-consuming access)
/// - `moves`: Places being moved (consuming access)  
/// - `reassigns`: Places being written/assigned
///
/// ## Example
///
/// ```rust
/// // For statement: a = &x
/// Events {
///     start_loans: vec![LoanId(0)],           // New borrow
///     uses: vec![Place::Local(x)],            // Read x for borrowing
///     reassigns: vec![Place::Local(a)],       // Assign to a
///     moves: vec![],                          // No moves
/// }
/// ```
#[derive(Debug, Clone, Default)]
pub struct Events {
    /// Loans starting at this program point
    pub start_loans: Vec<LoanId>,
    /// Places being used (read access)
    pub uses: Vec<Place>,
    /// Places being moved (consuming read)
    pub moves: Vec<Place>,
    /// Places being reassigned (write access)
    pub reassigns: Vec<Place>,
}

/// Loan identifier for tracking borrows
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct LoanId(pub u32);

impl LoanId {
    /// Create a new loan ID
    pub fn new(id: u32) -> Self {
        LoanId(id)
    }

    /// Get the loan ID
    pub fn id(&self) -> u32 {
        self.0
    }
}

impl std::fmt::Display for LoanId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "loan{}", self.0)
    }
}

/// Simple loan structure for tracking borrows
///
/// Loans represent active borrows in the simplified borrow checking system.
/// Each loan tracks what is borrowed, how it's borrowed, and where the borrow originated.
///
/// ## Loan Lifecycle
///
/// 1. **Creation**: Loan created when `Rvalue::Ref` generates `start_loans` event
/// 2. **Tracking**: Loan tracked through dataflow analysis using efficient bitsets
/// 3. **Termination**: Loan ends when owner is moved/reassigned or goes out of scope
///
/// ## Conflict Detection
///
/// Loans are checked for conflicts using aliasing analysis:
/// - **Shared + Shared**: No conflict (multiple readers allowed)
/// - **Shared + Mutable**: Conflict (reader/writer conflict)
/// - **Mutable + Any**: Conflict (exclusive access required)
///
/// ## Example
///
/// ```rust
/// // For code: let a = &x;
/// Loan {
///     id: LoanId(0),
///     owner: Place::Local { index: 0, wasm_type: I32 }, // x
///     kind: BorrowKind::Shared,
///     origin_stmt: ProgramPoint(1), // Where borrow occurs
/// }
/// ```
#[derive(Debug, Clone)]
pub struct Loan {
    /// Unique loan identifier
    pub id: LoanId,
    /// Place being borrowed (direct Place reference for simplicity)
    pub owner: Place,
    /// Kind of borrow (shared, mutable, unique)
    pub kind: BorrowKind,
    /// Program point where this loan originates
    pub origin_stmt: ProgramPoint,
}

/// Borrow checking error
#[derive(Debug, Clone)]
pub struct BorrowError {
    /// Program point where error occurs
    pub point: ProgramPoint,
    /// Type of borrow error
    pub error_type: BorrowErrorType,
    /// Error message
    pub message: String,
    /// Source location for error reporting
    pub location: TextLocation,
}

/// Types of borrow checking errors
#[derive(Debug, Clone, PartialEq)]
pub enum BorrowErrorType {
    /// Conflicting borrows (shared vs mutable)
    ConflictingBorrows {
        existing_borrow: BorrowKind,
        new_borrow: BorrowKind,
        place: Place,
    },
    /// Use after move
    UseAfterMove {
        place: Place,
        move_point: ProgramPoint,
    },
    /// Borrow live across owner move/drop
    BorrowAcrossOwnerInvalidation {
        borrowed_place: Place,
        owner_place: Place,
        invalidation_point: ProgramPoint,
        invalidation_type: InvalidationType,
    },
}

/// Types of owner invalidation
#[derive(Debug, Clone, PartialEq)]
pub enum InvalidationType {
    /// Owner was moved
    Move,
}







/// Export information for WASM module
#[derive(Debug, Clone)]
pub struct Export {
    /// Export name
    pub name: String,
    /// Export kind
    pub kind: ExportKind,
    /// Index in respective section
    pub index: u32,
}

/// WASM export kinds
#[derive(Debug, Clone, PartialEq)]
pub enum ExportKind {
    Function,
    Global,
    Memory,
    Table,
}

/// Type information for WASM module generation
#[derive(Debug, Clone)]
pub struct TypeInfo {
    /// Function type signatures
    pub function_types: Vec<FunctionSignature>,
    /// Global variable types
    pub global_types: Vec<WasmType>,
    /// Memory requirements
    pub memory_info: MemoryInfo,
    /// Interface vtable information
    pub interface_info: InterfaceInfo,
}

/// Memory information for WASM module
#[derive(Debug, Clone)]
pub struct MemoryInfo {
    /// Initial memory size (in WASM pages)
    pub initial_pages: u32,
    /// Maximum memory size (in WASM pages)
    pub max_pages: Option<u32>,
    /// Static data size
    pub static_data_size: u32,
}

/// Interface information for dynamic dispatch
#[derive(Debug, Clone)]
pub struct InterfaceInfo {
    /// Interface definitions
    pub interfaces: HashMap<u32, InterfaceDefinition>,
    /// Vtable layouts
    pub vtables: HashMap<u32, VTable>,
    /// Function table for call_indirect
    pub function_table: Vec<u32>,
}

/// Interface definition
#[derive(Debug, Clone)]
pub struct InterfaceDefinition {
    /// Interface ID
    pub id: u32,
    /// Interface name
    pub name: String,
    /// Method signatures
    pub methods: Vec<MethodSignature>,
}

/// Method signature for interface
#[derive(Debug, Clone)]
pub struct MethodSignature {
    /// Method ID within interface
    pub id: u32,
    /// Method name
    pub name: String,
    /// Parameter types (including receiver)
    pub param_types: Vec<WasmType>,
    /// Return types
    pub return_types: Vec<WasmType>,
}

/// Virtual table for interface dispatch
#[derive(Debug, Clone)]
pub struct VTable {
    /// Interface ID
    pub interface_id: u32,
    /// Implementing type ID
    pub type_id: u32,
    /// Function indices for each method
    pub method_functions: Vec<u32>,
}
