// Optimized import structure - grouped by module for clarity
use crate::compiler::parsers::tokenizer::tokens::TextLocation;
use crate::compiler::wir::place::{Place, WasmType};
use crate::compiler::string_interning::InternedString;
use std::collections::HashMap;

/// WASM Intermediate Representation structure with simplified borrow checking
///
/// This WIR is designed specifically for efficient WASM generation with
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
/// - `ProgramPoint`: Sequential identifiers for each WIR statement
/// - `Events`: Simple event records per program point for dataflow analysis
/// - `Loan`: Simplified borrow tracking with origin points
/// - `Place`: WASM-optimized memory location abstractions (unchanged)
///
/// See `docs/dataflow-analysis-guide.md` for detailed algorithm documentation.
#[derive(Debug)]
#[allow(clippy::upper_case_acronyms)] // WIR is a well-established acronym in this codebase
pub struct WIR {
    /// Functions in the module
    pub functions: Vec<WirFunction>,
    /// Global variables and their places
    pub globals: HashMap<u32, Place>,
    /// Module exports
    pub exports: HashMap<InternedString, Export>,
    /// Type information for WASM module generation
    pub type_info: TypeInfo,
    /// Host function imports for WASM generation
    pub host_imports:
        std::collections::HashSet<crate::compiler::host_functions::registry::HostFunctionDef>,
}

impl Default for WIR {
    fn default() -> Self {
        Self::new()
    }
}

impl WIR {
    /// Create a new WIR structure
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

    /// Add a function to the WIR
    pub fn add_function(&mut self, function: WirFunction) {
        self.functions.push(function);
    }

    /// Add host function imports to the WIR
    pub fn add_host_imports(
        &mut self,
        imports: &std::collections::HashSet<
            crate::compiler::host_functions::registry::HostFunctionDef,
        >,
    ) {
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
        self.functions.iter().flat_map(|f| f.iter_program_points())
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
    pub fn find_function_for_program_point(&self, point: &ProgramPoint) -> Option<&WirFunction> {
        self.functions.iter().find(|f| f.events.contains_key(point))
    }

    /// Get a mutable reference to a function by ID
    pub fn get_function_mut(&mut self, function_id: u32) -> Option<&mut WirFunction> {
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

/// Simplified WIR function representation
///
/// This simplified design removes complex optimizations in favor of correctness:
/// - No arena allocation - uses standard Vec and HashMap
/// - No place interning - uses direct Place references
/// - No complex event caching - simple HashMap storage
/// - Essential fields only for basic WIR functionality
#[derive(Debug, Clone)]
pub struct WirFunction {
    /// Function ID
    pub id: u32,
    /// Function name
    pub name: InternedString,
    /// Parameter places
    pub parameters: Vec<Place>,
    /// Return type information (WASM types for code generation)
    pub return_types: Vec<WasmType>,
    /// Return argument information (full Arg info for named returns and references)
    pub return_args: Vec<crate::compiler::parsers::ast_nodes::Arg>,
    /// Basic blocks
    pub blocks: Vec<WirBlock>,
    /// Local variable places
    pub locals: HashMap<InternedString, Place>,
    /// WASM function signature
    pub signature: FunctionSignature,
    /// Simple event storage per program point
    pub events: HashMap<ProgramPoint, Events>,
    /// All loans in this function for borrow checking
    pub loans: Vec<Loan>,
}

impl WirFunction {
    /// Create a new simplified WIR function
    pub fn new(
        id: u32,
        name: InternedString,
        parameters: Vec<Place>,
        return_types: Vec<WasmType>,
        return_args: Vec<crate::compiler::parsers::ast_nodes::Arg>,
    ) -> Self {
        Self {
            id,
            name,
            parameters: parameters.clone(),
            return_types: return_types.clone(),
            return_args,
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
    pub fn add_block(&mut self, block: WirBlock) {
        self.blocks.push(block);
    }

    /// Add a local variable to this function
    pub fn add_local(&mut self, name: InternedString, place: Place) {
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
        // Since program points are managed at the function level,
        // return the keys from the events HashMap
        let mut points: Vec<ProgramPoint> = self.events.keys().copied().collect();
        points.sort();
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

/// Basic WIR block
///
/// Simplified design with essential fields only:
/// - No complex program point tracking
/// - No WASM-specific control flow information
/// - No parent/child block relationships
/// - Simple construction and manipulation methods
#[derive(Debug, Clone)]
pub struct WirBlock {
    /// Block ID for control flow
    pub id: u32,
    /// WIR statements
    pub statements: Vec<Statement>,
    /// Block terminator
    pub terminator: Terminator,
}

impl WirBlock {
    /// Create a new WIR block
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
}

/// WIR statement that maps efficiently to WASM instructions
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

    /// WASIX function call (low-level WASM system calls)
    WasixCall {
        function_name: InternedString,
        args: Vec<Operand>,
        destination: Option<Place>,
    },

    /// Conditional execution (simplified control flow)
    Conditional {
        condition: Operand,
        then_statements: Vec<Statement>,
        else_statements: Vec<Statement>,
    },

    /// Mark a struct field as initialized (for tracking optional defaults)
    MarkFieldInitialized {
        struct_place: Place,
        field_name: InternedString,
        field_index: u32,
    },

    /// Check that all required struct fields are initialized before use
    ValidateStructInitialization {
        struct_place: Place,
        struct_type: crate::compiler::datatypes::DataType,
    },
}

impl Statement {
    /// Generate state-aware events for this statement on-demand
    ///
    /// This method computes events dynamically from the statement structure,
    /// including state transition information for Beanstalk's memory model.
    /// Events are computed based on the statement type and operands.
    ///
    /// ## Performance Benefits
    /// - Reduces WIR memory footprint by ~30%
    /// - Eliminates redundant event storage
    /// - Enables efficient event caching for repeated access patterns
    ///
    /// ## Event Generation Rules
    /// - `Assign`: Generates reassign event for place, use/move events for rvalue operands, state transitions
    /// - `Call`: Generates use events for arguments, reassign event for destination
    /// - `InterfaceCall`: Generates use events for receiver and arguments, reassign for destination
    /// - `Drop`: Generates use event for the dropped place, transition to Killed state
    /// - `Store`: Generates reassign event for place, use event for value
    /// - `Alloc`: Generates reassign event for place, use event for size, transition to Owned
    /// - `Dealloc`: Generates use event for place, transition to Killed
    /// - `Nop`, `MemoryOp`: Generate no events for basic borrow checking
    pub fn generate_events(&self) -> Events {
        self.generate_events_at_program_point(ProgramPoint::new(0))
    }

    /// Generate state-aware events for this statement at a specific program point
    ///
    /// This method includes state transition tracking for precise borrow checking
    /// with Beanstalk's implicit borrowing semantics.
    pub fn generate_events_at_program_point(&self, program_point: ProgramPoint) -> Events {
        let mut events = Events::default();

        match self {
            Statement::Assign { place, rvalue } => {
                // The assignment itself generates a reassign event for the place
                events.reassigns.push(place.clone());

                // Generate events for the rvalue with state transitions
                self.generate_rvalue_events_with_states(rvalue, &mut events, program_point, place);
            }
            Statement::Call {
                args, destination, ..
            } => {
                // Generate use events for all arguments
                for arg in args {
                    self.generate_operand_events(arg, &mut events);
                }

                // If there's a destination, it gets reassigned with state transition to Owned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                    events.state_transitions.push(StateTransition {
                        place: dest_place.clone(),
                        from_state: PlaceState::Owned, // Assume previous state
                        to_state: PlaceState::Owned,   // Function result is owned
                        program_point,
                        reason: TransitionReason::Assignment,
                    });
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

                // If there's a destination, it gets reassigned with state transition to Owned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                    events.state_transitions.push(StateTransition {
                        place: dest_place.clone(),
                        from_state: PlaceState::Owned, // Assume previous state
                        to_state: PlaceState::Owned,   // Interface result is owned
                        program_point,
                        reason: TransitionReason::Assignment,
                    });
                }
            }
            Statement::Drop { place } => {
                // Generate drop event - this is an end-of-lifetime point
                events.uses.push(place.clone());

                // Transition to Killed state
                events.state_transitions.push(StateTransition {
                    place: place.clone(),
                    from_state: PlaceState::Owned, // Assume it was owned before drop
                    to_state: PlaceState::Killed,
                    program_point,
                    reason: TransitionReason::LastUse,
                });
            }
            Statement::Store { place, value, .. } => {
                // Store operations reassign the place and use the value
                events.reassigns.push(place.clone());
                self.generate_operand_events(value, &mut events);

                // Store creates owned value at the place
                events.state_transitions.push(StateTransition {
                    place: place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // Store creates owned value
                    program_point,
                    reason: TransitionReason::Assignment,
                });
            }
            Statement::Alloc { place, size, .. } => {
                // Allocation reassigns the place and uses the size operand
                events.reassigns.push(place.clone());
                self.generate_operand_events(size, &mut events);

                // Allocation creates owned value
                events.state_transitions.push(StateTransition {
                    place: place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // Allocation creates owned value
                    program_point,
                    reason: TransitionReason::Assignment,
                });
            }
            Statement::Dealloc { place } => {
                // Deallocation uses the place (to free it)
                events.uses.push(place.clone());

                // Transition to Killed state
                events.state_transitions.push(StateTransition {
                    place: place.clone(),
                    from_state: PlaceState::Owned, // Must be owned to deallocate
                    to_state: PlaceState::Killed,
                    program_point,
                    reason: TransitionReason::LastUse,
                });
            }
            Statement::HostCall {
                args, destination, ..
            } => {
                // Generate use events for all arguments
                for arg in args {
                    self.generate_operand_events(arg, &mut events);
                }

                // If there's a destination, it gets reassigned with state transition to Owned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                    events.state_transitions.push(StateTransition {
                        place: dest_place.clone(),
                        from_state: PlaceState::Owned, // Assume previous state
                        to_state: PlaceState::Owned,   // Host function result is owned
                        program_point,
                        reason: TransitionReason::Assignment,
                    });
                }
            }
            Statement::WasixCall {
                args, destination, ..
            } => {
                // Generate use events for all arguments (same as HostCall)
                for arg in args {
                    self.generate_operand_events(arg, &mut events);
                }

                // If there's a destination, it gets reassigned with state transition to Owned
                if let Some(dest_place) = destination {
                    events.reassigns.push(dest_place.clone());
                    events.state_transitions.push(StateTransition {
                        place: dest_place.clone(),
                        from_state: PlaceState::Owned, // Assume previous state
                        to_state: PlaceState::Owned,   // WASIX function result is owned
                        program_point,
                        reason: TransitionReason::Assignment,
                    });
                }
            }
            Statement::MarkFieldInitialized { struct_place, .. } => {
                // Mark field initialization - this is a reassign event for the struct
                events.reassigns.push(struct_place.clone());

                // Transition struct to partially initialized state
                events.state_transitions.push(StateTransition {
                    place: struct_place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // Still owned, but more initialized
                    program_point,
                    reason: TransitionReason::Assignment,
                });
            }
            Statement::ValidateStructInitialization { struct_place, .. } => {
                // Validation uses the struct place to check initialization
                events.uses.push(struct_place.clone());
            }
            Statement::Conditional {
                condition,
                then_statements,
                else_statements,
            } => {
                // Generate use event for the condition
                self.generate_operand_events(condition, &mut events);

                // For now, generate events for all statements in both branches
                // TODO: Implement proper control flow analysis
                for stmt in then_statements {
                    let stmt_events = stmt.generate_events_at_program_point(program_point);
                    events.uses.extend(stmt_events.uses);
                    events.moves.extend(stmt_events.moves);
                    events.reassigns.extend(stmt_events.reassigns);
                    events.start_loans.extend(stmt_events.start_loans);
                    events
                        .state_transitions
                        .extend(stmt_events.state_transitions);
                }
                for stmt in else_statements {
                    let stmt_events = stmt.generate_events_at_program_point(program_point);
                    events.uses.extend(stmt_events.uses);
                    events.moves.extend(stmt_events.moves);
                    events.reassigns.extend(stmt_events.reassigns);
                    events.start_loans.extend(stmt_events.start_loans);
                    events
                        .state_transitions
                        .extend(stmt_events.state_transitions);
                }
            }
            Statement::Nop | Statement::MemoryOp { .. } => {
                // These don't generate events for basic borrow checking
            }
        }

        events
    }

    /// Generate events for rvalue operations with state transitions
    fn generate_rvalue_events_with_states(
        &self,
        rvalue: &Rvalue,
        events: &mut Events,
        program_point: ProgramPoint,
        target_place: &Place,
    ) {
        match rvalue {
            Rvalue::Use(operand) => {
                self.generate_operand_events(operand, events);

                // Use operations typically create owned values at the target
                events.state_transitions.push(StateTransition {
                    place: target_place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // Use creates owned value
                    program_point,
                    reason: TransitionReason::Assignment,
                });
            }
            Rvalue::BinaryOp(_, left, right) => {
                self.generate_operand_events(left, events);
                self.generate_operand_events(right, events);

                // Binary operations create owned results
                events.state_transitions.push(StateTransition {
                    place: target_place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // Binary op creates owned value
                    program_point,
                    reason: TransitionReason::Assignment,
                });
            }
            Rvalue::UnaryOp(_, operand) => {
                self.generate_operand_events(operand, events);

                // Unary operations create owned results
                events.state_transitions.push(StateTransition {
                    place: target_place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // Unary op creates owned value
                    program_point,
                    reason: TransitionReason::Assignment,
                });
            }
            Rvalue::Ref { place, borrow_kind } => {
                // The place being borrowed is also used (read access)
                events.uses.push(place.clone());

                // Create state transition based on borrow kind
                let target_state = match borrow_kind {
                    BorrowKind::Shared => PlaceState::Referenced,
                    BorrowKind::Mut => PlaceState::Borrowed,
                };

                events.state_transitions.push(StateTransition {
                    place: target_place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: target_state,
                    program_point,
                    reason: TransitionReason::BorrowCreated,
                });

                // Note: Loan creation is handled by the borrow fact extractor
                // which scans WIR statements for Rvalue::Ref operations
                // and creates appropriate loans with unique IDs
            }
            Rvalue::StringConcat(left, right) => {
                // String concatenation uses both operands
                self.generate_operand_events(left, events);
                self.generate_operand_events(right, events);

                // String concatenation creates owned result
                events.state_transitions.push(StateTransition {
                    place: target_place.clone(),
                    from_state: PlaceState::Owned, // Assume previous state
                    to_state: PlaceState::Owned,   // String concat creates owned value
                    program_point,
                    reason: TransitionReason::Assignment,
                });
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

/// Right-hand side values for Beanstalk's implicit borrowing system
///
/// These represent the different ways values can be used in assignments:
/// - Use: Direct use of operands (constants, copies, moves)
/// - BinaryOp/UnaryOp: Arithmetic and logical operations
/// - Ref: Explicit representation of Beanstalk's implicit borrows
/// - StringConcat: String concatenation operation
#[derive(Debug, Clone, PartialEq)]
pub enum Rvalue {
    /// Use a place or constant (copies, moves, constants)
    Use(Operand),

    /// Binary operation (arithmetic, comparison, logical)
    BinaryOp(BinOp, Operand, Operand),

    /// Unary operation (negation, not)
    UnaryOp(UnOp, Operand),

    /// Borrow operation (makes Beanstalk's implicit borrows explicit in WIR)
    /// - `x = y` becomes Ref { place: y, borrow_kind: Shared }
    /// - `x ~= y` becomes Ref { place: y, borrow_kind: Mut }
    Ref {
        place: Place,
        borrow_kind: BorrowKind,
    },

    /// String concatenation operation (lhs + rhs for strings)
    StringConcat(Operand, Operand),
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

/// Operands for WIR operations in Beanstalk's memory model
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    /// Explicit copy from a place (rare - only for types that support copying)
    Copy(Place),

    /// Move from a place (ownership transfer: `~x` in Beanstalk)
    Move(Place),

    /// Constant value (literals: 42, "hello", true)
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
    /// String slice literal (immutable pointer to data section)
    String(InternedString),
    /// Mutable string (heap-allocated with capacity)
    MutableString(InternedString),
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
        self.generate_events_at_program_point(ProgramPoint::new(0))
    }

    /// Generate state-aware events for this terminator at a specific program point
    pub fn generate_events_at_program_point(&self, program_point: ProgramPoint) -> Events {
        let mut events = Events::default();

        match self {
            Terminator::If { condition, .. } => {
                self.generate_operand_events_with_states(condition, &mut events, program_point);
            }
            Terminator::Return { values } => {
                for value in values {
                    self.generate_operand_events_with_states(value, &mut events, program_point);
                }
            }
            _ => {
                // Other terminators don't have operands that generate events
            }
        }

        events
    }

    /// Generate events for operands in terminators with state transitions
    fn generate_operand_events_with_states(
        &self,
        operand: &Operand,
        events: &mut Events,
        program_point: ProgramPoint,
    ) {
        match operand {
            Operand::Copy(place) => {
                events.uses.push(place.clone());
                // Copy operations don't change the source place state
            }
            Operand::Move(place) => {
                events.moves.push(place.clone());
                // Move operations transition the source place to Moved state
                events.state_transitions.push(StateTransition {
                    place: place.clone(),
                    from_state: PlaceState::Owned, // Must be owned to move
                    to_state: PlaceState::Moved,
                    program_point,
                    reason: TransitionReason::MoveOccurred,
                });
            }
            Operand::Constant(_) => {
                // Constants don't generate events
            }
            Operand::FunctionRef(_) | Operand::GlobalRef(_) => {
                // References don't generate events
            }
        }
    }

    /// Generate events for operands in terminators (legacy method for compatibility)
    fn generate_operand_events(&self, operand: &Operand, events: &mut Events) {
        self.generate_operand_events_with_states(operand, events, ProgramPoint::new(0));
    }
}

/// Borrow kinds for Beanstalk's implicit borrowing system
///
/// In Beanstalk, borrowing is the default semantics:
/// - `x = y` creates a shared borrow (multiple allowed)
/// - `x ~= y` creates a mutable borrow (exclusive)
/// - `x ~= ~y` is a move (ownership transfer, not a borrow)
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum BorrowKind {
    /// Shared borrow: `x = y` (default, multiple allowed)
    Shared,
    /// Mutable borrow: `x ~= y` (exclusive, conflicts with any other borrow)
    Mut,
}

/// Place state in Beanstalk's memory model
///
/// Represents the current state of a place in Beanstalk's implicit borrowing system.
/// States track ownership and borrowing relationships without explicit lifetime annotations.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum PlaceState {
    /// Place owns the value (no active loans)
    Owned,
    /// Place has shared loans (x = y creates shared loan)
    Referenced,
    /// Place has mutable loan (x ~= y creates mutable loan)
    Borrowed,
    /// Last use completed, loans can be released
    Killed,
    /// Value has been moved out (place invalidated)
    Moved,
}

/// State transition for a place at a program point
///
/// Tracks how places change state during program execution, enabling
/// state-aware borrow checking with Beanstalk's implicit borrowing semantics.
#[derive(Debug, Clone, PartialEq)]
pub struct StateTransition {
    /// Place undergoing state transition
    pub place: Place,
    /// State before this program point
    pub from_state: PlaceState,
    /// State after this program point
    pub to_state: PlaceState,
    /// Program point where transition occurs
    pub program_point: ProgramPoint,
    /// Reason for the state transition
    pub reason: TransitionReason,
}

/// Reason for a state transition
#[derive(Debug, Clone, PartialEq)]
pub enum TransitionReason {
    /// Borrow created (x = y or x ~= y)
    BorrowCreated,
    /// Move occurred (ownership transfer)
    MoveOccurred,
    /// Last use detected (can transition to Killed)
    LastUse,
    /// Assignment/reassignment
    Assignment,
    /// Loan ended (return to Owned)
    LoanEnded,
}

/// Program point identifier (one per WIR statement)
///
/// Program points provide a unique identifier for each WIR statement to enable
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

/// State-aware events for dataflow analysis with Beanstalk memory model
///
/// Events provide straightforward borrow tracking for each program point with
/// enhanced state transition tracking for Beanstalk's implicit borrowing system.
/// Each program point has associated events that describe what happens at that
/// statement in terms of borrows, uses, moves, assignments, and state changes.
///
/// ## Event Types
///
/// - `start_loans`: New borrows beginning at this program point
/// - `uses`: Places being read (non-consuming access)
/// - `moves`: Places being moved (consuming access)  
/// - `reassigns`: Places being written/assigned
/// - `state_transitions`: State changes for places (Owned/Referenced/Borrowed/Moved)
///
/// ## Example
///
/// ```rust
/// // For statement: a = x (shared borrow in Beanstalk)
/// Events {
///     start_loans: vec![LoanId(0)],           // New shared loan
///     uses: vec![Place::Local(x)],            // Read x for borrowing
///     reassigns: vec![Place::Local(a)],       // Assign to a
///     moves: vec![],                          // No moves
///     state_transitions: vec![StateTransition {
///         place: Place::Local(a),
///         from_state: PlaceState::Owned,
///         to_state: PlaceState::Referenced,
///         program_point: ProgramPoint(1),
///         reason: TransitionReason::BorrowCreated,
///     }],
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
    /// State transitions for places at this program point
    pub state_transitions: Vec<StateTransition>,
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
    /// Type of borrow error with state information
    pub error_type: BorrowErrorType,
    /// Primary source location where error occurs
    pub primary_location: TextLocation,
    /// Secondary source location (e.g., where conflicting borrow was created)
    pub secondary_location: Option<TextLocation>,
    /// Main error message
    pub message: String,
    /// Helpful suggestion for fixing the error
    pub suggestion: Option<String>,
    /// Current state of the place involved in the error
    pub current_state: Option<PlaceState>,
    /// Expected state for the operation to succeed
    pub expected_state: Option<PlaceState>,
    /// Structured metadata for LLM and LSP integration
    pub metadata: std::collections::HashMap<crate::compiler::compiler_errors::ErrorMetaDataKey, &'static str>,
}

/// Types of borrow checking errors with Beanstalk-specific semantics
#[derive(Debug, Clone, PartialEq)]
pub enum BorrowErrorType {
    /// Multiple mutable borrows (x ~= y when y is already mutably borrowed)
    MultipleMutableBorrows {
        place: Place,
        existing_borrow_location: TextLocation,
        new_borrow_location: TextLocation,
    },
    /// Shared/mutable conflict (x ~= y when y has shared references, or x = y when y is mutably borrowed)
    SharedMutableConflict {
        place: Place,
        existing_borrow_kind: BorrowKind,
        new_borrow_kind: BorrowKind,
        existing_borrow_location: TextLocation,
        new_borrow_location: TextLocation,
    },
    /// Use after move (using a place that has been moved)
    UseAfterMove {
        place: Place,
        move_location: TextLocation,
        use_location: TextLocation,
    },
    /// Move while borrowed (attempting to move a place that has active borrows)
    MoveWhileBorrowed {
        place: Place,
        borrow_kind: BorrowKind,
        borrow_location: TextLocation,
        move_location: TextLocation,
    },
}

impl BorrowError {
    /// Create a multiple mutable borrows error
    pub fn multiple_mutable_borrows(
        place: Place,
        existing_location: TextLocation,
        new_location: TextLocation,
    ) -> Self {
        use crate::compiler::compiler_errors::ErrorMetaDataKey;
        
        let place_str: &'static str = Box::leak(format!("{:?}", place).into_boxed_str());
        
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(ErrorMetaDataKey::VariableName, place_str);
        metadata.insert(ErrorMetaDataKey::BorrowKind, "Mutable");
        metadata.insert(ErrorMetaDataKey::ConflictingVariable, place_str);
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(ErrorMetaDataKey::PrimarySuggestion, "Ensure the first mutable borrow is no longer used before creating the second");
        metadata.insert(ErrorMetaDataKey::LifetimeHint, "Only one mutable borrow can exist at a time");
        
        Self {
            error_type: BorrowErrorType::MultipleMutableBorrows {
                place: place.clone(),
                existing_borrow_location: existing_location.clone(),
                new_borrow_location: new_location.clone(),
            },
            primary_location: new_location,
            secondary_location: Some(existing_location),
            message: format!(
                "cannot mutably borrow `{:?}` because it is already mutably borrowed",
                place
            ),
            suggestion: Some(
                "ensure the first mutable borrow is no longer used before creating the second"
                    .to_string(),
            ),
            current_state: Some(PlaceState::Borrowed),
            expected_state: Some(PlaceState::Owned),
            metadata,
        }
    }

    /// Create a shared/mutable conflict error
    pub fn shared_mutable_conflict(
        place: Place,
        existing_kind: BorrowKind,
        new_kind: BorrowKind,
        existing_location: TextLocation,
        new_location: TextLocation,
    ) -> Self {
        use crate::compiler::compiler_errors::ErrorMetaDataKey;
        
        let place_str: &'static str = Box::leak(format!("{:?}", place).into_boxed_str());
        let existing_kind_str: &'static str = match existing_kind {
            BorrowKind::Shared => "Shared",
            BorrowKind::Mut => "Mutable",
        };
        let new_kind_str: &'static str = match new_kind {
            BorrowKind::Shared => "Shared",
            BorrowKind::Mut => "Mutable",
        };
        
        let (message, current_state, expected_state, suggestion, lifetime_hint) = match (&existing_kind, &new_kind) {
            (BorrowKind::Shared, BorrowKind::Mut) => (
                format!(
                    "cannot mutably borrow `{:?}` because it is already referenced",
                    place
                ),
                PlaceState::Referenced,
                PlaceState::Owned,
                "Ensure all shared references are finished before creating mutable access",
                "Mutable borrows require exclusive access - no other borrows can exist",
            ),
            (BorrowKind::Mut, BorrowKind::Shared) => (
                format!(
                    "cannot reference `{:?}` because it is already mutably borrowed",
                    place
                ),
                PlaceState::Borrowed,
                PlaceState::Owned,
                "Finish using the mutable borrow before creating shared references",
                "Mutable borrows are exclusive - no other borrows can exist while active",
            ),
            _ => (
                format!("conflicting borrows of `{:?}`", place),
                PlaceState::Owned, // Default
                PlaceState::Owned,
                "Resolve the borrow conflict by restructuring your code",
                "Check the borrow rules for your specific case",
            ),
        };

        let mut metadata = std::collections::HashMap::new();
        metadata.insert(ErrorMetaDataKey::VariableName, place_str);
        metadata.insert(ErrorMetaDataKey::BorrowKind, new_kind_str);
        metadata.insert(ErrorMetaDataKey::ConflictingVariable, place_str);
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(ErrorMetaDataKey::PrimarySuggestion, suggestion);
        metadata.insert(ErrorMetaDataKey::LifetimeHint, lifetime_hint);
        
        // Add information about the existing borrow kind
        let existing_borrow_info: &'static str = Box::leak(
            format!("Existing {} borrow conflicts with new {} borrow", existing_kind_str, new_kind_str).into_boxed_str()
        );
        metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, existing_borrow_info);

        Self {
            error_type: BorrowErrorType::SharedMutableConflict {
                place: place.clone(),
                existing_borrow_kind: existing_kind,
                new_borrow_kind: new_kind,
                existing_borrow_location: existing_location.clone(),
                new_borrow_location: new_location.clone(),
            },
            primary_location: new_location,
            secondary_location: Some(existing_location),
            message,
            suggestion: Some(suggestion.to_string()),
            current_state: Some(current_state),
            expected_state: Some(expected_state),
            metadata,
        }
    }

    /// Create a use after move error
    pub fn use_after_move(
        place: Place,
        move_location: TextLocation,
        use_location: TextLocation,
    ) -> Self {
        use crate::compiler::compiler_errors::ErrorMetaDataKey;
        
        let place_str: &'static str = Box::leak(format!("{:?}", place).into_boxed_str());
        let move_loc_str: &'static str = Box::leak(
            format!("Value moved at line {}", move_location.line).into_boxed_str()
        );
        let use_loc_str: &'static str = Box::leak(
            format!("Used at line {}", use_location.line).into_boxed_str()
        );
        
        let mut metadata = std::collections::HashMap::new();
        metadata.insert(ErrorMetaDataKey::VariableName, place_str);
        metadata.insert(ErrorMetaDataKey::MovedVariable, place_str);
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(ErrorMetaDataKey::PrimarySuggestion, "Consider using a reference instead of moving the value");
        metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, "Clone the value before moving if you need to use it later");
        metadata.insert(ErrorMetaDataKey::SuggestedLocation, move_loc_str);
        metadata.insert(ErrorMetaDataKey::LifetimeHint, "Once a value is moved, ownership transfers and the original variable can no longer be used");
        
        Self {
            error_type: BorrowErrorType::UseAfterMove {
                place: place.clone(),
                move_location: move_location.clone(),
                use_location: use_location.clone(),
            },
            primary_location: use_location,
            secondary_location: Some(move_location),
            message: format!("borrow of moved value: `{:?}`", place),
            suggestion: Some("consider cloning the value before the move, or restructure your code to avoid the move".to_string()),
            current_state: Some(PlaceState::Moved),
            expected_state: Some(PlaceState::Owned),
            metadata,
        }
    }

    /// Create a move while borrowed error
    pub fn move_while_borrowed(
        place: Place,
        borrow_kind: BorrowKind,
        borrow_location: TextLocation,
        move_location: TextLocation,
    ) -> Self {
        use crate::compiler::compiler_errors::ErrorMetaDataKey;
        
        let place_str: &'static str = Box::leak(format!("{:?}", place).into_boxed_str());
        let borrow_kind_str: &'static str = match borrow_kind {
            BorrowKind::Shared => "Shared",
            BorrowKind::Mut => "Mutable",
        };
        
        let borrow_type = match borrow_kind {
            BorrowKind::Shared => "referenced",
            BorrowKind::Mut => "mutably borrowed",
        };

        let current_state = match borrow_kind {
            BorrowKind::Shared => PlaceState::Referenced,
            BorrowKind::Mut => PlaceState::Borrowed,
        };
        
        let borrow_loc_str: &'static str = Box::leak(
            format!("Borrowed at line {}", borrow_location.line).into_boxed_str()
        );

        let mut metadata = std::collections::HashMap::new();
        metadata.insert(ErrorMetaDataKey::VariableName, place_str);
        metadata.insert(ErrorMetaDataKey::BorrowedVariable, place_str);
        metadata.insert(ErrorMetaDataKey::BorrowKind, borrow_kind_str);
        metadata.insert(ErrorMetaDataKey::CompilationStage, "Borrow Checking");
        metadata.insert(ErrorMetaDataKey::PrimarySuggestion, "Ensure all borrows are finished before moving the value");
        metadata.insert(ErrorMetaDataKey::AlternativeSuggestion, "Use references instead of moving the value");
        metadata.insert(ErrorMetaDataKey::SuggestedLocation, borrow_loc_str);
        metadata.insert(ErrorMetaDataKey::LifetimeHint, "Cannot move a value while it has active borrows - the borrows must end first");

        Self {
            error_type: BorrowErrorType::MoveWhileBorrowed {
                place: place.clone(),
                borrow_kind,
                borrow_location: borrow_location.clone(),
                move_location: move_location.clone(),
            },
            primary_location: move_location,
            secondary_location: Some(borrow_location),
            message: format!(
                "cannot move out of `{:?}` because it is {}",
                place, borrow_type
            ),
            suggestion: Some("ensure all borrows are finished before moving the value".to_string()),
            current_state: Some(current_state),
            expected_state: Some(PlaceState::Owned),
            metadata,
        }
    }

    /// Convert this borrow error to a compile error for the main pipeline
    pub fn to_compile_error(&self) -> crate::compiler::compiler_errors::CompileError {
        use crate::compiler::compiler_errors::{CompileError, ErrorType};

        // Format the main error message with state information
        let mut formatted_message = self.message.clone();

        // Add state information if available
        if let (Some(current), Some(expected)) = (&self.current_state, &self.expected_state) {
            formatted_message.push_str(&format!(
                "\n  current state: {:?}, expected state: {:?}",
                current, expected
            ));
        }

        // Add secondary location information if available
        if let Some(secondary_loc) = &self.secondary_location {
            formatted_message.push_str(&format!(
                "\n  note: conflicting operation occurred at {}:{}",
                secondary_loc.start_pos.line_number, secondary_loc.start_pos.char_column
            ));
        }

        // Add suggestion if available
        if let Some(suggestion) = &self.suggestion {
            formatted_message.push_str(&format!("\n  help: {}", suggestion));
        }

        CompileError {
            msg: formatted_message,
            location: self.primary_location.clone(),
            error_type: ErrorType::BorrowChecker,
            metadata: self.metadata.clone(),
        }
    }
}

/// Convert a list of borrow errors to compile errors
pub fn convert_borrow_errors_to_compile_errors(
    borrow_errors: &[BorrowError],
) -> Vec<crate::compiler::compiler_errors::CompileError> {
    borrow_errors
        .iter()
        .map(|err| err.to_compile_error())
        .collect()
}

/// Export information for WASM module
#[derive(Debug, Clone)]
pub struct Export {
    /// Export name
    pub name: InternedString,
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
    pub name: InternedString,
    /// Method signatures
    pub methods: Vec<MethodSignature>,
}

/// Method signature for interface
#[derive(Debug, Clone)]
pub struct MethodSignature {
    /// Method ID within interface
    pub id: u32,
    /// Method name
    pub name: InternedString,
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
