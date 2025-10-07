# Beanstalk WIR Implementation Guide

This guide explains how Beanstalk's WASM Intermediate Representation (WIR) works and how each component contributes to efficient WASM generation with memory safety. The WIR is designed as a dual-purpose system that enables both precise borrow checking and direct WASM lowering.

## What is WIR?

WIR (WASM Intermediate Representation) is Beanstalk's intermediate language that sits between the AST and WASM bytecode. It serves two critical purposes:

1. **Borrow Checking**: Provides precise memory safety analysis using dataflow techniques
2. **WASM Generation**: Maps directly to efficient WASM instruction sequences

Think of WIR as a "WASM-aware" intermediate representation that understands both Beanstalk's memory model and WASM's execution model.

## Core Design Philosophy

### Beanstalk's Unique Memory Model

Beanstalk has a distinctive approach to memory management that WIR must handle correctly:

- **Borrowing is implicit**: `x = y` automatically creates a shared borrow (no `&` needed)
- **Mutability is explicit**: `x ~= y` creates a mutable borrow using the `~` operator
- **Moves are compiler-determined**: The compiler decides when to move vs borrow based on usage analysis
- **No implicit copying**: All copying must be explicit in the source code

### WASM-First Design

Every WIR operation is chosen to map efficiently to WASM:
- **Direct instruction mapping**: Each WIR statement becomes ≤3 WASM instructions
- **WASM type alignment**: All operands use WASM value types (i32, i64, f32, f64)
- **Linear memory integration**: Memory operations map directly to WASM's linear memory model
- **Stack-based evaluation**: Operations designed for WASM's stack machine

## Module Structure and Responsibilities

The WIR system is organized into focused modules, each with a specific responsibility:

```
src/compiler/wir/
├── wir.rs                    // Pipeline orchestration and entry points
├── wir_nodes.rs             // Core data structures (WIR, Statement, Place, etc.)
├── place.rs                 // Memory location abstraction for WASM
├── build_wir.rs             // AST → WIR transformation logic
├── extract.rs               // Borrow fact extraction for dataflow analysis
└── unified_borrow_checker.rs // Combined borrow checking and conflict detection
```

Let's explore each module and understand what it does:

### `wir.rs` - The Pipeline Orchestrator

This is the main entry point that coordinates the entire WIR pipeline. Think of it as the conductor of an orchestra:

**Key Function**: `borrow_check_pipeline(ast: AstBlock) -> Result<WIR, Vec<CompileError>>`

This function orchestrates the complete process:
1. Calls `ast_to_wir()` to transform AST into WIR
2. Runs borrow checking on all functions
3. Returns either a validated WIR or compilation errors

**Why it exists**: Provides a clean interface for the rest of the compiler while hiding the complexity of the multi-stage WIR pipeline.

### `wir_nodes.rs` - The Core Data Structures

This module defines all the fundamental types that make up the WIR. It's like the vocabulary of the WIR language.

#### `WIR` - The Complete Module Representation

```rust
pub struct WIR {
    pub functions: Vec<WirFunction>,           // All functions in the module
    pub globals: HashMap<u32, Place>,         // Global variables
    pub exports: HashMap<String, Export>,     // WASM exports
    pub type_info: TypeInfo,                  // Type information for WASM
    pub host_imports: HashSet<HostFunctionDef>, // Runtime function imports
}
```

**Purpose**: Represents a complete Beanstalk module ready for WASM generation. Contains everything needed to produce a valid WASM module.

#### `WirFunction` - Individual Function Representation

```rust
pub struct WirFunction {
    pub id: u32,                              // Unique function identifier
    pub name: String,                         // Function name
    pub parameters: Vec<Place>,               // Parameter locations
    pub return_types: Vec<WasmType>,          // Return value types
    pub blocks: Vec<WirBlock>,                // Function body as basic blocks
    pub locals: HashMap<String, Place>,       // Local variable mapping
    pub signature: FunctionSignature,         // WASM function signature
    pub events: HashMap<ProgramPoint, Events>, // Borrow checking events
    pub loans: Vec<Loan>,                     // Active borrows in this function
}
```

**Purpose**: Represents a single function with all information needed for both borrow checking and WASM generation.

**Key Methods**:
- `get_program_points_in_order()`: Returns all program points for dataflow analysis
- `generate_events()`: Creates borrow checking events for a program point
- `add_loan()`: Tracks a new borrow for conflict detection

#### `Statement` - Individual WIR Operations

```rust
pub enum Statement {
    Assign { place: Place, rvalue: Rvalue },  // Basic assignment
    Call { func: Operand, args: Vec<Operand>, destination: Option<Place> }, // Function calls
    InterfaceCall { ... },                    // Dynamic dispatch calls
    Store { place: Place, value: Operand, alignment: u32, offset: u32 }, // Memory operations
    Drop { place: Place },                    // Explicit cleanup
    // ... other variants
}
```

**Purpose**: Represents individual operations that map directly to WASM instruction sequences.

**Key Feature**: Each statement can generate its own borrow checking events via `generate_events()`, eliminating the need to store events separately.

#### `Rvalue` - Right-Hand Side Values

```rust
pub enum Rvalue {
    Use(Operand),                            // Direct use of a value
    BinaryOp(BinOp, Operand, Operand),      // Arithmetic/logical operations
    UnaryOp(UnOp, Operand),                 // Unary operations
    Ref { place: Place, borrow_kind: BorrowKind }, // Beanstalk's implicit borrows
}
```

**Purpose**: Represents how values are computed or obtained. The `Ref` variant is crucial - it makes Beanstalk's implicit borrowing explicit in the WIR.

**Beanstalk Mapping**:
- `x = y` becomes `Ref { place: y, borrow_kind: Shared }`
- `x ~= y` becomes `Ref { place: y, borrow_kind: Mut }`

#### `ProgramPoint` - Borrow Checking Locations

```rust
pub struct ProgramPoint(pub u32);
```

**Purpose**: Provides unique identifiers for each WIR statement to enable precise dataflow analysis. Each statement gets exactly one program point.

**Why needed**: Borrow checking requires tracking exactly where borrows start, end, and conflict. Program points provide the precision needed for accurate analysis.

#### `Events` - Borrow Checking Information

```rust
pub struct Events {
    pub start_loans: Vec<LoanId>,    // New borrows starting here
    pub uses: Vec<Place>,            // Places being read
    pub moves: Vec<Place>,           // Places being moved (consumed)
    pub reassigns: Vec<Place>,       // Places being written/assigned
}
```

**Purpose**: Describes what happens at each program point in terms of memory access patterns. This information drives the borrow checking dataflow analysis.

#### `Loan` - Active Borrow Tracking

```rust
pub struct Loan {
    pub id: LoanId,                  // Unique loan identifier
    pub owner: Place,                // What's being borrowed
    pub kind: BorrowKind,            // Shared or mutable borrow
    pub origin_stmt: ProgramPoint,   // Where the borrow started
}
```

**Purpose**: Represents an active borrow in the system. Loans are tracked through dataflow analysis to detect conflicts.

### `place.rs` - WASM-Optimized Memory Locations

This module defines how WIR represents memory locations in a way that maps efficiently to WASM.

#### `Place` - Memory Location Abstraction

```rust
pub enum Place {
    Local { index: u32, wasm_type: WasmType },     // WASM local variables
    Global { index: u32, wasm_type: WasmType },    // WASM global variables
    Memory { base: MemoryBase, offset: ByteOffset, size: TypeSize }, // Linear memory
    Projection { base: Box<Place>, elem: ProjectionElem }, // Field/index access
}
```

**Purpose**: Represents memory locations in a way that maps directly to WASM memory operations.

**WASM Mapping**:
- `Local` → `local.get`/`local.set` instructions
- `Global` → `global.get`/`global.set` instructions  
- `Memory` → `memory.load`/`memory.store` instructions
- `Projection` → Computed memory addresses

**Key Methods**:
- `wasm_type()`: Returns the WASM type for this location
- `load_instruction_count()`: How many WASM instructions needed to load
- `generate_load_operations()`: Produces the actual WASM instruction sequence

#### `WasmType` - WASM Value Types

```rust
pub enum WasmType {
    I32, I64, F32, F64,              // WASM primitive types
    ExternRef, FuncRef,              // WASM reference types
}
```

**Purpose**: Ensures all WIR operations use types that WASM can handle directly.

#### `PlaceManager` - Memory Layout Coordination

```rust
pub struct PlaceManager {
    next_local_index: u32,           // Next WASM local to allocate
    next_global_index: u32,          // Next WASM global to allocate
    memory_layout: MemoryLayout,     // Linear memory organization
    local_types: HashMap<u32, WasmType>, // Type tracking for locals
    // ...
}
```

**Purpose**: Coordinates memory allocation across the entire module, ensuring efficient WASM memory layout.

### `build_wir.rs` - AST to WIR Transformation

This is where the magic happens - converting Beanstalk's high-level AST into the low-level WIR.

#### `WirTransformContext` - Transformation State

```rust
pub struct WirTransformContext {
    place_manager: PlaceManager,     // Memory allocation
    variable_scopes: Vec<HashMap<String, Place>>, // Variable tracking
    function_names: HashMap<String, u32>, // Function registry
    // ...
}
```

**Purpose**: Maintains all the state needed during AST→WIR transformation, including variable scoping and memory allocation.

#### Key Transformation Functions

**`ast_to_wir(ast: AstBlock) -> Result<WIR, CompileError>`**
- Main entry point for transformation
- Handles both function definitions and main program logic
- Orchestrates the two-pass algorithm (function collection, then statement transformation)

**`transform_ast_node_to_wir(node: &AstNode, context: &mut WirTransformContext) -> Result<Vec<Statement>, CompileError>`**
- Converts individual AST nodes into WIR statements
- Handles all Beanstalk language constructs (variables, functions, control flow, etc.)
- Generates appropriate borrow checking events

**Critical Beanstalk Transformations**:
- Variable declarations: `let x = y` → `Assign { place: x_place, rvalue: Ref { place: y_place, borrow_kind: Shared } }`
- Mutable assignments: `x ~= y` → `Assign { place: x_place, rvalue: Ref { place: y_place, borrow_kind: Mut } }`
- Function calls: Proper argument lowering and return value handling

### `extract.rs` - Borrow Fact Extraction

This module builds the foundation for borrow checking by extracting dataflow facts from WIR.

#### `BorrowFactExtractor` - Dataflow Fact Builder

```rust
pub struct BorrowFactExtractor {
    pub gen_sets: HashMap<ProgramPoint, BitSet>,    // Loans starting at each point
    pub kill_sets: HashMap<ProgramPoint, BitSet>,   // Loans ending at each point
    pub loans: Vec<Loan>,                           // All loans in the function
    pub place_to_loans: HashMap<Place, Vec<LoanId>>, // Efficient lookup
    // ...
}
```

**Purpose**: Builds the gen/kill sets needed for forward dataflow analysis of loan liveness.

**Key Process**:
1. `collect_loans_from_events()`: Finds all borrow operations in the WIR
2. `build_gen_sets()`: Determines which loans start at each program point
3. `build_kill_sets()`: Determines which loans end due to moves/reassignments
4. Uses efficient bitsets for performance in large functions

#### `BitSet` - Efficient Dataflow Sets

```rust
pub struct BitSet {
    words: Vec<u64>,                 // Packed bits for efficiency
    capacity: usize,                 // Total bit capacity
    word_count: usize,               // Number of 64-bit words
}
```

**Purpose**: Provides highly optimized bitset operations for dataflow analysis. Uses SIMD-friendly operations and bulk processing for performance.

**Key Operations**:
- `union_with()`: Combines two bitsets (for dataflow join operations)
- `subtract()`: Removes bits (for kill set application)
- `for_each_set_bit()`: Efficient iteration over active loans

#### `may_alias()` - Aliasing Analysis

```rust
pub fn may_alias(place_a: &Place, place_b: &Place) -> bool
```

**Purpose**: Determines if two memory locations might refer to the same data. Critical for determining when borrows conflict.

**Aliasing Rules**:
- Same place always aliases
- Variable aliases its fields: `x` aliases `x.field`
- Different fields don't alias: `x.field1` vs `x.field2`
- Constant array indices: `arr[0]` vs `arr[1]` don't alias
- Dynamic indices are conservative: `arr[i]` might alias anything in `arr`

### `unified_borrow_checker.rs` - Conflict Detection

This module combines multiple analyses into a single efficient pass for ~40% performance improvement.

#### `UnifiedBorrowChecker` - Combined Analysis Engine

```rust
pub struct UnifiedBorrowChecker {
    live_vars_in: HashMap<ProgramPoint, HashSet<Place>>,    // Variable liveness
    live_loans_in: HashMap<ProgramPoint, BitSet>,           // Loan liveness
    moved_places_in: HashMap<ProgramPoint, HashSet<Place>>, // Move tracking
    // ... corresponding _out sets
    errors: Vec<BorrowError>,                               // Detected violations
    warnings: Vec<BorrowError>,                             // Potential issues
}
```

**Purpose**: Performs all borrow checking analyses in a single forward traversal, detecting conflicts immediately.

**Unified Algorithm**:
1. **Liveness Analysis**: Computes which variables are live (backward analysis, cached)
2. **Loan Tracking**: Tracks which borrows are active (forward dataflow)
3. **Move Tracking**: Tracks which places have been moved out (forward dataflow)
4. **Conflict Detection**: Immediately detects violations using current state
5. **Refinement**: Optimizes Copy→Move operations based on liveness

#### Key Conflict Checks

**`check_conflicting_borrows_at_point()`**
- Detects when multiple incompatible borrows are active
- Shared + Shared = OK (multiple readers allowed)
- Shared + Mutable = ERROR (reader/writer conflict)
- Mutable + Mutable = ERROR (exclusive access required)

**`check_move_while_borrowed_at_point()`**
- Detects attempts to move a value that's currently borrowed
- Uses aliasing analysis to check if moved place conflicts with active loans

**`check_use_after_move_at_point()`**
- Detects attempts to use a value after it's been moved
- Tracks moved-out places and checks for subsequent usage

## The Complete Pipeline Flow

Here's how everything works together:

1. **AST Input**: Beanstalk source code parsed into AST
2. **WIR Generation** (`build_wir.rs`): 
   - Transform AST nodes into WIR statements
   - Allocate places for all memory locations
   - Generate events for each statement
3. **Fact Extraction** (`extract.rs`):
   - Scan WIR for borrow operations
   - Build gen/kill sets for dataflow analysis
   - Create loan objects for all borrows
4. **Borrow Checking** (`unified_borrow_checker.rs`):
   - Run unified forward analysis
   - Detect all types of borrow conflicts
   - Generate helpful error messages
5. **WASM Generation** (separate module):
   - Use validated WIR to generate WASM bytecode
   - Places map directly to WASM memory operations
   - Statements map to WASM instruction sequences

## Key Design Benefits

### For Borrow Checking
- **Precise tracking**: Program points enable exact conflict detection
- **Field sensitivity**: Can borrow different struct fields simultaneously
- **Performance**: Bitset dataflow scales to large functions
- **Helpful errors**: Source locations and suggestions for fixes

### For WASM Generation
- **Direct mapping**: Each WIR operation becomes ≤3 WASM instructions
- **Efficient memory**: Places map directly to WASM locals/globals/memory
- **Type safety**: WASM types preserved throughout the pipeline
- **Optimization ready**: Clean IR suitable for external WASM optimizers

### For Maintainability
- **Clear separation**: Each module has a focused responsibility
- **Testable components**: Each analysis can be tested independently
- **Extensible design**: New language features can be added incrementally
- **Performance monitoring**: Built-in statistics for optimization

## Working with the WIR System

### Adding New Language Features

1. **Extend AST**: Add new node types for the feature
2. **Update WIR**: Add new statement/rvalue types if needed
3. **Transform Logic**: Add transformation in `build_wir.rs`
4. **Borrow Rules**: Update conflict detection if memory semantics change
5. **WASM Mapping**: Ensure new constructs map to efficient WASM

### Debugging Borrow Errors

1. **Check Events**: Verify events are generated correctly for statements
2. **Trace Dataflow**: Follow gen/kill sets through the analysis
3. **Aliasing Analysis**: Confirm `may_alias()` returns expected results
4. **Program Points**: Ensure proper mapping from WIR to source locations

### Performance Optimization

1. **Bitset Operations**: Profile bitset performance in large functions
2. **Aliasing Cache**: Cache `may_alias()` results for repeated queries
3. **Event Generation**: Consider caching events for hot program points
4. **Memory Layout**: Optimize place allocation for WASM efficiency

This WIR system provides the foundation for Beanstalk's unique combination of memory safety and WASM performance. Each component is designed to work efficiently both independently and as part of the larger compilation pipeline.