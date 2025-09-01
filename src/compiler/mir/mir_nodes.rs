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
        }
    }

    /// Add a function to the MIR
    pub fn add_function(&mut self, function: MirFunction) {
        self.functions.push(function);
    }

    /// Get all program points from all functions
    pub fn get_all_program_points(&self) -> Vec<ProgramPoint> {
        let mut all_points = Vec::new();
        for function in &self.functions {
            all_points.extend(&function.program_points);
        }
        all_points.sort();
        all_points
    }

    /// Get program points for a specific function
    pub fn get_function_program_points(&self, function_id: u32) -> Option<&Vec<ProgramPoint>> {
        self.functions
            .iter()
            .find(|f| f.id == function_id)
            .map(|f| &f.program_points)
    }

    /// Iterate over all program points
    pub fn iter_program_points(&self) -> impl Iterator<Item = ProgramPoint> + '_ {
        self.functions
            .iter()
            .flat_map(|f| f.program_points.iter())
            .copied()
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
            .find(|f| f.program_points.contains(point))
    }

    /// Get a mutable reference to a function by ID
    pub fn get_function_mut(&mut self, function_id: u32) -> Option<&mut MirFunction> {
        self.functions.iter_mut().find(|f| f.id == function_id)
    }

    /// Build control flow graph (placeholder for now)
    pub fn build_control_flow_graph(&mut self) {
        // This will be implemented in later tasks
    }

    /// Validate WASM constraints (placeholder for now)
    pub fn validate_wasm_constraints(&self) -> Result<(), String> {
        // This will be implemented in later tasks
        Ok(())
    }
}

/// WASM-optimized function representation with simplified borrow checking
#[derive(Debug, Clone)]
pub struct MirFunction {
    /// Function ID
    pub id: u32,
    /// Function name
    pub name: String,
    /// Parameter places (WASM locals 0..n)
    pub parameters: Vec<Place>,
    /// Return type information
    pub return_types: Vec<WasmType>,
    /// Basic blocks with WASM-structured control flow
    pub blocks: Vec<MirBlock>,
    /// Local variable places
    pub locals: HashMap<String, Place>,
    /// WASM function signature
    pub signature: FunctionSignature,
    /// All program points in this function (sequential order)
    pub program_points: Vec<ProgramPoint>,
    /// Mapping from program point to block ID
    pub program_point_to_block: HashMap<ProgramPoint, u32>,
    /// Mapping from program point to statement index within block
    pub program_point_to_statement: HashMap<ProgramPoint, usize>,
    /// Events per program point for dataflow analysis
    pub events: HashMap<ProgramPoint, Events>,
    /// All loans in this function for borrow checking
    pub loans: Vec<Loan>,
}

impl MirFunction {
    /// Create a new MIR function
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
            program_points: Vec::new(),
            program_point_to_block: HashMap::new(),
            program_point_to_statement: HashMap::new(),
            events: HashMap::new(),
            loans: Vec::new(),
        }
    }

    /// Add a program point to this function
    pub fn add_program_point(&mut self, point: ProgramPoint, block_id: u32, statement_index: usize) {
        self.program_points.push(point);
        self.program_point_to_block.insert(point, block_id);
        if statement_index != usize::MAX {
            self.program_point_to_statement.insert(point, statement_index);
        }
    }

    /// Add a block to this function
    pub fn add_block(&mut self, block: MirBlock) {
        self.blocks.push(block);
    }

    /// Get the block ID for a given program point
    pub fn get_block_for_program_point(&self, point: &ProgramPoint) -> Option<u32> {
        self.program_point_to_block.get(point).copied()
    }

    /// Get the statement index for a given program point
    pub fn get_statement_index_for_program_point(&self, point: &ProgramPoint) -> Option<usize> {
        self.program_point_to_statement.get(point).copied()
    }

    /// Get all program points in execution order for dataflow analysis
    pub fn get_program_points_in_order(&self) -> &Vec<ProgramPoint> {
        &self.program_points
    }

    /// Iterate over program points for worklist algorithm
    pub fn iter_program_points(&self) -> impl Iterator<Item = &ProgramPoint> {
        self.program_points.iter()
    }

    /// Get program point successors for dataflow analysis (placeholder for CFG construction)
    pub fn get_program_point_successors(&self, _point: &ProgramPoint) -> Vec<ProgramPoint> {
        // This will be implemented when CFG construction is added in later tasks
        vec![]
    }

    /// Get program point predecessors for dataflow analysis (placeholder for CFG construction)
    pub fn get_program_point_predecessors(&self, _point: &ProgramPoint) -> Vec<ProgramPoint> {
        // This will be implemented when CFG construction is added in later tasks
        vec![]
    }

    /// Store events for a program point
    pub fn store_events(&mut self, program_point: ProgramPoint, events: Events) {
        self.events.insert(program_point, events);
    }

    /// Get events for a program point
    pub fn get_events(&self, program_point: &ProgramPoint) -> Option<&Events> {
        self.events.get(program_point)
    }

    /// Get all events for this function
    pub fn get_all_events(&self) -> &HashMap<ProgramPoint, Events> {
        &self.events
    }

    /// Add a loan to this function
    pub fn add_loan(&mut self, loan: Loan) {
        self.loans.push(loan);
    }

    /// Get all loans in this function
    pub fn get_loans(&self) -> &[Loan] {
        &self.loans
    }

    /// Get mutable reference to loans
    pub fn get_loans_mut(&mut self) -> &mut Vec<Loan> {
        &mut self.loans
    }
}

/// WASM function signature information
#[derive(Debug, Clone)]
pub struct FunctionSignature {
    /// Parameter types in WASM order
    pub param_types: Vec<WasmType>,
    /// Return types in WASM order
    pub result_types: Vec<WasmType>,
}

/// Basic block with WASM-structured control flow and simplified borrow tracking
#[derive(Debug, Clone)]
pub struct MirBlock {
    /// Block ID for control flow
    pub id: u32,
    /// MIR statements (map to ≤3 WASM instructions each)
    pub statements: Vec<Statement>,
    /// Block terminator
    pub terminator: Terminator,
    /// Program points for statements in this block (one per statement)
    pub statement_program_points: Vec<ProgramPoint>,
    /// Program point for the terminator
    pub terminator_program_point: Option<ProgramPoint>,
    /// WASM control flow structure information
    pub control_flow_info: ControlFlowInfo,
    /// Parent block for nested structures
    pub parent_block: Option<u32>,
    /// Child blocks for nested structures
    pub child_blocks: Vec<u32>,
    /// WASM nesting level (for validation)
    pub nesting_level: u32,
}

impl MirBlock {
    /// Create a new MIR block
    pub fn new(id: u32) -> Self {
        Self {
            id,
            statements: Vec::new(),
            terminator: Terminator::Unreachable,
            statement_program_points: Vec::new(),
            terminator_program_point: None,
            control_flow_info: ControlFlowInfo {
                structure_type: WasmStructureType::Linear,
                nesting_depth: 0,
                has_fallthrough: false,
                wasm_label: None,
            },
            parent_block: None,
            child_blocks: Vec::new(),
            nesting_level: 0,
        }
    }

    /// Set the terminator for this block
    pub fn set_terminator(&mut self, terminator: Terminator) {
        self.terminator = terminator;
    }

    /// Add a statement with program point
    pub fn add_statement_with_program_point(&mut self, statement: Statement, point: ProgramPoint) {
        self.statements.push(statement);
        self.statement_program_points.push(point);
    }

    /// Set terminator with program point
    pub fn set_terminator_with_program_point(&mut self, terminator: Terminator, point: ProgramPoint) {
        self.terminator = terminator;
        self.terminator_program_point = Some(point);
    }

    /// Convert to owned (for compatibility)
    pub fn into(self) -> Self {
        self
    }

    /// Get all program points in this block (statements + terminator)
    pub fn get_all_program_points(&self) -> Vec<ProgramPoint> {
        let mut points = self.statement_program_points.clone();
        if let Some(term_point) = self.terminator_program_point {
            points.push(term_point);
        }
        points
    }

    /// Get the program point for a specific statement index
    pub fn get_statement_program_point(&self, statement_index: usize) -> Option<ProgramPoint> {
        self.statement_program_points.get(statement_index).copied()
    }

    /// Get the terminator program point
    pub fn get_terminator_program_point(&self) -> Option<ProgramPoint> {
        self.terminator_program_point
    }

    /// Check if this block contains a given program point
    pub fn contains_program_point(&self, point: &ProgramPoint) -> bool {
        self.statement_program_points.contains(point) || 
        self.terminator_program_point == Some(*point)
    }
}

/// MIR statement that maps efficiently to WASM instructions
#[derive(Debug, Clone, PartialEq)]
pub enum Statement {
    /// Assign an rvalue to a place
    Assign { place: Place, rvalue: Rvalue },

    /// Function call with WASM calling convention
    Call {
        func: Operand,
        args: Vec<Operand>,
        destination: Option<Place>,
    },

    /// Interface method call (vtable dispatch)
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
}

/// Right-hand side values with WASM operation semantics
#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
    /// Use a place or constant (maps to WASM load/const)
    Use(Operand),

    /// Binary operation (maps to WASM arithmetic/comparison)
    BinaryOp {
        op: BinOp,
        left: Operand,
        right: Operand,
    },

    /// Unary operation (maps to WASM unary ops)
    UnaryOp {
        op: UnOp,
        operand: Operand,
    },

    /// Cast operation (WASM type conversion)
    Cast {
        source: Operand,
        target_type: WasmType,
    },

    /// Reference to a place (borrow)
    Ref {
        place: Place,
        borrow_kind: BorrowKind,
    },

    /// Dereference operation
    Deref {
        place: Place,
    },

    /// Array/collection creation
    Array {
        elements: Vec<Operand>,
        element_type: WasmType,
    },

    /// Struct/object creation
    Struct {
        fields: Vec<(u32, Operand)>, // field_id, value
        struct_type: u32,
    },

    /// Load from memory with WASM-specific alignment
    Load {
        place: Place,
        alignment: u32,
        offset: u32,
    },

    /// WASM-specific memory operations
    MemorySize,
    MemoryGrow {
        pages: Operand,
    },

    /// Interface method call through vtable (maps to call_indirect)
    InterfaceCall {
        interface_id: u32,
        method_id: u32,
        receiver: Operand,
        args: Vec<Operand>,
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

/// Block terminators with WASM control flow mapping
#[derive(Debug, Clone, PartialEq)]
pub enum Terminator {
    /// Unconditional jump (WASM br)
    Goto {
        target: u32,
        /// WASM label depth for br instruction
        label_depth: u32,
    },

    /// Unconditional jump (simplified for compatibility)
    UnconditionalJump(u32),

    /// Conditional jump (WASM br_if)
    If {
        condition: Operand,
        then_block: u32,
        else_block: u32,
        /// WASM if/else structure info
        wasm_if_info: WasmIfInfo,
    },

    /// Conditional jump (simplified for compatibility)
    ConditionalJump(u32, u32),

    /// Switch/match (WASM br_table)
    Switch {
        discriminant: Operand,
        targets: Vec<u32>,
        default: u32,
        /// WASM br_table optimization info
        br_table_info: BrTableInfo,
    },

    /// Function return (WASM return)
    Return { values: Vec<Operand> },

    /// Simple return (for compatibility)
    Returns,

    /// Unreachable code (WASM unreachable)
    Unreachable,

    /// Loop back-edge (WASM br to loop start)
    Loop {
        target: u32,
        /// Loop header block ID
        loop_header: u32,
        /// WASM loop structure info
        loop_info: WasmLoopInfo,
    },

    /// WASM block structure (for nested control flow)
    Block {
        /// Inner blocks in this WASM block
        inner_blocks: Vec<u32>,
        /// Block result type
        result_type: Option<WasmType>,
        /// Exit target after block
        exit_target: u32,
    },
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

/// Simple events for dataflow analysis (one per statement)
///
/// Events replace complex Polonius facts with straightforward borrow tracking.
/// Each program point has associated events that describe what happens at that
/// statement in terms of borrows, uses, moves, and assignments.
///
/// ## Event Types
///
/// - `start_loans`: New borrows beginning at this program point
/// - `uses`: Places being read (non-consuming access)
/// - `moves`: Places being moved (consuming access)  
/// - `reassigns`: Places being written/assigned
/// - `candidate_last_uses`: Potential last uses from AST analysis
///
/// ## Dataflow Integration
///
/// Events are converted to gen/kill sets for dataflow analysis:
/// - **Gen sets**: `start_loans` become generated loans
/// - **Kill sets**: `moves` and `reassigns` kill loans of aliasing places
/// - **Use/Def sets**: `uses`/`reassigns` for liveness analysis
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
///     candidate_last_uses: vec![],            // No last uses
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
    /// Places that are candidates for last use (from AST analysis)
    pub candidate_last_uses: Vec<Place>,
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
    /// Place being borrowed
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



/// WASM control flow structure information
#[derive(Debug, Clone, PartialEq)]
pub struct ControlFlowInfo {
    /// Type of WASM control structure
    pub structure_type: WasmStructureType,
    /// Nesting depth in WASM structured control flow
    pub nesting_depth: u32,
    /// Whether this block can be reached by fallthrough
    pub has_fallthrough: bool,
    /// WASM label for br/br_if instructions
    pub wasm_label: Option<u32>,
}

/// Types of WASM control structures
#[derive(Debug, Clone, PartialEq)]
pub enum WasmStructureType {
    /// Linear sequence of instructions
    Linear,
    /// WASM if/else structure
    If,
    /// WASM loop structure
    Loop,
    /// WASM block structure
    Block,
    /// Function body
    Function,
}

/// WASM if/else structure information
#[derive(Debug, Clone, PartialEq)]
pub struct WasmIfInfo {
    /// Whether this has an else branch
    pub has_else: bool,
    /// Result type of the if expression
    pub result_type: Option<WasmType>,
    /// Nesting level within function
    pub nesting_level: u32,
}

/// WASM br_table optimization information
#[derive(Debug, Clone, PartialEq)]
pub struct BrTableInfo {
    /// Number of targets in the table
    pub target_count: u32,
    /// Whether targets are densely packed (good for br_table)
    pub is_dense: bool,
    /// Minimum target value
    pub min_target: u32,
    /// Maximum target value
    pub max_target: u32,
}

/// WASM loop structure information
#[derive(Debug, Clone, PartialEq)]
pub struct WasmLoopInfo {
    /// Loop type (while, for, etc.)
    pub loop_type: LoopType,
    /// Whether loop has break statements
    pub has_breaks: bool,
    /// Whether loop has continue statements
    pub has_continues: bool,
    /// Result type of the loop
    pub result_type: Option<WasmType>,
}

/// Types of loops for WASM optimization
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LoopType {
    /// While loop with condition at top
    While,
    /// For loop with iterator
    For,
    /// Do-while loop with condition at bottom
    DoWhile,
    /// Infinite loop
    Infinite,
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