# Beanstalk Compiler Architecture Guide

This document provides a comprehensive overview of the Beanstalk compiler's internal architecture, compilation pipeline, and development patterns.

## Compilation Pipeline Overview

The Beanstalk compiler follows a multi-stage pipeline: **Source -> tokens (can run in parallel for each file) -> module dependancy order sort (import info is parsed at tokenizer stage) -> ast -> wir -> borrow checking -> wasm -> wasm runtime**

### Stage 1: Tokenization
- **Location**: `src/compiler/parsers/tokenizer.rs`
- **Input**: Raw source code strings
- **Output**: Stream of tokens with location information
- **Key Features**: 
  - Tracks precise source locations for error reporting
  - Handles Beanstalk-specific syntax (`:` for scope open, `;` for scope close)
  - Supports template syntax with `[]` delimiters

### Stage 2: AST Construction with Compile-Time Optimization
- **Location**: `src/compiler/parsers/build_ast.rs`
- **Input**: Token stream
- **Output**: Abstract Syntax Tree with optimized expressions
- **Key Features**:
  - **Immediate constant folding** during AST construction
  - **RPN conversion** for expressions that can't be folded
  - **Type inference** and basic type checking
  - **Expression optimization** before AST finalization

#### Critical: Expression Handling at AST Stage

**Compile-Time Folding**: The AST stage performs aggressive constant folding in `src/compiler/optimizers/constant_folding.rs`:
- Pure literal expressions (e.g., `2 + 3`) are evaluated immediately
- Results in `ExpressionKind::Int(5)` rather than runtime operations
- Expressions are converted to **Reverse Polish Notation (RPN)** for evaluation

**Runtime Expressions**: When expressions cannot be folded at compile time:
- Variables, function calls, or complex operations become `ExpressionKind::Runtime(Vec<AstNode>)`
- The `Vec<AstNode>` contains the expression in **RPN order** ready for stack-based evaluation
- Example: `x + 2 * y` becomes `[x, 2, y, *, +]` in the Runtime vector

**Type System Integration**: 
- Type checking occurs during AST construction
- `DataType` information is attached to all expressions
- Type mismatches are caught early in the pipeline

### Stage 3: WASM-Optimized WIR Generation (Polonius-Compatible)
- **Location**: `src/compiler/wir/build_wir.rs`
- **Input**: Optimized AST with Runtime expressions
- **Output**: WASM-targeted Mid-level IR with precise lifetime tracking
- **Key Features**:
  - **Direct WASM mapping** - each WIR statement lowers to ≤3 WASM instructions
  - **WASM-native place abstraction** for memory location tracking
  - **WASM memory model integration** with linear memory and stack awareness
  - **Polonius fact generation** optimized for WASM's simpler memory model
  - **WASM-efficient interface dispatch** using function tables

#### WASM-Optimized WIR Design Philosophy

**WASM-First Architecture**: The WIR is designed specifically for efficient WASM generation:
- **Direct Instruction Mapping**: WIR operations correspond directly to WASM instruction sequences
- **WASM Type Alignment**: All WIR operands use WASM value types (i32, i64, f32, f64)
- **Structured Control Flow**: WIR blocks map directly to WASM's structured control flow
- **Linear Memory Optimization**: Place analysis optimized for WASM's linear memory model

**Polonius Integration with WASM Constraints**: Lifetime analysis accounts for WASM execution model:
- **WASM Memory Regions**: Places distinguish WASM stack locals from linear memory locations
- **Call Stack Awareness**: Regions track WASM function call boundaries and stack frames
- **Linear Memory Borrows**: Loan tracking optimized for WASM's simple memory hierarchy
- **WASM-Specific Facts**: Polonius facts generated with WASM memory model constraints

**WASM-Efficient Statement Design**: WIR statements designed for optimal WASM lowering:
```rust
// WASM-optimized WIR statements
Statement::Assign { 
    place: Place::Local(wasm_local_index),  // Direct WASM local mapping
    rvalue: Rvalue::BinaryOp(op, lhs, rhs)  // Maps to WASM arithmetic ops
}
Statement::Call {
    func: WasmFunction(func_index),         // Direct WASM function index
    args: Vec<Place>,                       // WASM-typed arguments
    destination: Place::Local(result_idx)   // WASM result local
}
```

**Interface System with WASM Function Tables**: Dynamic dispatch optimized for WASM:
- Interface vtables become WASM function tables with typed indices
- Method calls use WASM `call_indirect` with compile-time type checking
- Vtable layout optimized for WASM memory access patterns
- Borrow checking accounts for WASM calling conventions and stack management

### Stage 4: WASM-Aware Borrow Checking and Lifetime Analysis
- **Location**: `src/compiler/borrow_check/`
- **Input**: WASM-optimized WIR with region and loan information
- **Output**: Validated WIR with WASM-compatible memory safety guarantees
- **Key Features**:
  - **WASM-optimized Polonius analysis** leveraging simpler memory model
  - **WASM calling convention awareness** for two-phase borrow checking
  - **Linear memory region inference** with WASM memory layout constraints
  - **WASM-context error reporting** with memory model explanations

#### WASM-Optimized Borrow Checker Architecture

**WASM-Aware Fact Generation**: Facts generated with WASM memory model awareness:
```
// WASM-specific fact types
wasm_local_loan_issued_at(Point, Loan, LocalIndex)
wasm_memory_loan_issued_at(Point, Loan, MemoryOffset)  
wasm_call_invalidates_loans(Point, FunctionIndex, Vec<Loan>)
wasm_memory_region_subset(MemoryRegion, MemoryRegion, Point)
```

**WASM Memory Model Constraint Solving**: Region inference optimized for WASM:
- **Stack Frame Constraints**: Regions bounded by WASM function call frames
- **Linear Memory Constraints**: Memory region relationships based on WASM layout
- **Function Call Constraints**: Loan invalidation at WASM function boundaries
- **Simplified Outlives**: Leverages WASM's structured execution for simpler relationships

**WASM-Context Error Recovery**: Diagnostics that explain WASM memory concepts:
- Lifetime errors reference WASM stack frames and linear memory regions
- Borrow suggestions account for WASM calling conventions
- Move vs. copy recommendations consider WASM value type semantics
- Memory layout suggestions for WASM linear memory efficiency

### Stage 5: Direct WIR-to-WASM Lowering
- **Location**: `src/compiler/codegen/`
- **Input**: Validated WASM-optimized WIR with lifetime information
- **Output**: Optimized WASM bytecode with memory safety guarantees
- **Key Features**:
  - **One-to-few instruction mapping** from WIR statements to WASM
  - **Direct WASM module generation** without intermediate representations
  - **Lifetime-derived memory management** with optimal ARC insertion
  - **WASM function table generation** for interface dispatch
  - **Linear memory layout optimization** from WIR place analysis

#### Direct WASM Lowering Architecture

**Statement-to-Instruction Mapping**: Each WIR statement maps directly to WASM:
```rust
// Direct lowering examples
WIR::Assign { place: Place::Local(idx), rvalue: Rvalue::Use(op) }
→ WASM: local.get src_idx; local.set dst_idx

WIR::Call { func: WasmFunction(idx), args, destination }
→ WASM: [load args]; call func_idx; local.set result_idx

WIR::Terminator::Goto(block_id)
→ WASM: br block_label
```

**WASM Module Construction**: Direct generation of WASM module components:
- **Function Section**: Generated directly from WIR function definitions
- **Memory Section**: Layout determined by WIR place analysis
- **Table Section**: Interface vtables become WASM function tables
- **Export Section**: Based on WIR visibility and interface implementations

**Lifetime-Optimized Memory Management**: WIR analysis drives WASM memory decisions:
- **ARC Elimination**: Single-ownership variables use WASM value semantics
- **Memory Layout**: Complex types laid out optimally in linear memory
- **Drop Elaboration**: Cleanup code generated based on WIR lifetime analysis
- **GC Integration**: Prepared for WASM GC proposal with WIR lifetime information

## Error Handling Patterns

The compiler uses a sophisticated error system with specific macros for different error types. The error messages returned from these should always be helpful, descriptive and sometimes have a light or almost sarcastic/joky tone:

### Error Types and When to Use Them

#### `return_syntax_error!(location, "message", args...)`
- **Use for**: Malformed syntax in user code
- **Examples**: Missing semicolons, invalid tokens, malformed expressions
- **User-facing**: Yes - indicates user needs to fix syntax
- **Requires**: `TextLocation` for precise error positioning

#### `return_rule_error!(location, "message", args...)`
- **Use for**: Semantic errors and rule violations in user code
- **Examples**: Undefined variables, function not found, scope violations
- **User-facing**: Yes - indicates user needs to fix logic/usage
- **Requires**: `TextLocation` for precise error positioning

#### `return_type_error!(location, "message", args...)`
- **Use for**: Type system violations in user code
- **Examples**: Type mismatches, invalid operations on types
- **User-facing**: Yes - indicates user needs to fix types
- **Requires**: `TextLocation` for precise error positioning

#### `return_compiler_error!("message", args...)`
- **Use for**: Internal compiler bugs and unimplemented features
- **Examples**: Unsupported AST nodes, internal state corruption
- **User-facing**: No - indicates compiler developer needs to fix
- **Note**: Automatically prefixed with "COMPILER BUG" in output
- **No location required**: These are internal errors

#### `return_file_error!(path, "message", args...)`
- **Use for**: File system errors
- **Examples**: Missing files, permission errors, I/O failures
- **User-facing**: Yes - indicates file system issues
- **Requires**: File path instead of location

### Error Handling Best Practices

```rust
// Good: User made a syntax error
return_syntax_error!(location, "Expected ';' after statement, found '{}'", token);

// Good: User referenced undefined variable  
return_rule_error!(location, "Undefined variable '{}'. Variable must be declared before use.", name);

// Good: User has type mismatch
return_type_error!(location, "Cannot add {} and {}. Both operands must be the same type.", lhs_type, rhs_type);

// Good: Compiler doesn't support something yet
return_compiler_error!("For loop IR generation not yet implemented at {}:{}", location.line, location.column);

// Bad: Using compiler error for user mistakes
return_compiler_error!("User provided invalid variable name"); // Should be rule_error!

// Bad: Using rule error for unimplemented features  
return_rule_error!(location, "Match expressions not supported"); // Should be compiler_error!
```

## Beanstalk Language Features and WIR Integration

### Interface System (Dynamic Dispatch Without Traits)

Beanstalk supports interfaces for dynamic dispatch but deliberately avoids traits to maintain compilation speed and simplicity:

**Interface Definition**: Interfaces define method signatures without implementation
**Interface Implementation**: Types can implement interfaces with concrete methods  
**Dynamic Dispatch**: Interface method calls use vtables for runtime dispatch
**No Trait Bounds**: Unlike Rust, no complex trait bound resolution or associated types

**WIR Lowering for Interfaces**:
- Interface method calls become indirect calls through vtables
- Vtable construction happens during WIR generation
- Borrow checker accounts for interface receiver types and method signatures
- Interface implementations are tracked for dispatch table generation

```rust
// WIR representation of interface calls
Rvalue::Call {
    func: Operand::Constant(InterfaceMethod { interface_id, method_id }),
    args: vec![receiver, arg1, arg2],
    destination: Some(result_place),
}
```

### WASM-Optimized Memory Management in WIR

**WASM-Native Memory Model**: WIR designed around WASM's memory architecture
- **Stack Locals**: WIR `Place::Local` maps directly to WASM local indices
- **Linear Memory**: Complex types allocated in WASM linear memory with offset tracking
- **Global Variables**: WIR `Place::Global` corresponds to WASM global indices
- **Function Tables**: Interface vtables stored in WASM function tables

**WASM-Efficient Reference Counting**: ARC optimized for WASM execution
- **Value Type Optimization**: WASM value types (i32, i64, f32, f64) passed by value
- **Linear Memory ARC**: Reference counting only for heap-allocated complex types
- **Call Boundary Optimization**: ARC operations minimized across WASM function calls
- **Cycle Detection**: Simplified for WASM's structured execution model

**WASM-Aware Place Analysis**: Memory locations optimized for WASM access patterns
```rust
// WASM-optimized place examples
Place::Local(wasm_local_index)           // Direct WASM local
Place::Global(wasm_global_index)         // Direct WASM global
Place::Memory {                          // Linear memory location
    base: MemoryBase::LinearMemory,
    offset: ByteOffset(1024),
    size: TypeSize::I32,
}
Place::Projection {                      // Optimized field access
    base: Box::new(Place::Memory { ... }),
    elem: ProjectionElem::Field {
        offset: FieldOffset(8),          // Byte offset in linear memory
        size: FieldSize::F64,
    },
}
```

### WASM-Specific Design Principles

**Single-Target Optimization**: Leveraging WASM-only compilation for better design decisions
- **No Backend Abstraction**: WIR operations chosen specifically for optimal WASM lowering
- **WASM Type System Integration**: WIR types align exactly with WASM value types
- **Structured Control Flow**: WIR control flow designed for WASM's structured execution
- **Memory Model Alignment**: Place analysis optimized for WASM's linear memory + stack model

**Direct Lowering Philosophy**: Eliminating unnecessary abstraction layers
- **Statement-to-Instruction**: Each WIR statement maps to ≤3 WASM instructions
- **Type Preservation**: WASM types maintained throughout WIR analysis
- **Optimization Preservation**: WIR optimizations directly benefit WASM output
- **Debug Information**: Source locations preserved through direct lowering

**WASM Performance Integration**: WIR analysis drives WASM optimization
- **Instruction Selection**: WIR operations chosen for optimal WASM instruction sequences
- **Memory Access Patterns**: Place projections optimized for WASM memory instructions
- **Function Call Optimization**: Interface dispatch uses WASM call_indirect efficiently
- **Control Flow Efficiency**: Terminators designed for WASM br/br_if/br_table patterns

## Key Architectural Patterns

### Expression Processing Flow

1. **Parse**: Raw text → Tokens → AST nodes
2. **Fold**: Constant expressions evaluated immediately
3. **Convert**: Complex expressions → RPN in Runtime nodes
4. **Transform**: Runtime nodes → WIR statements with places
5. **Analyze**: Polonius borrow checking and lifetime inference
6. **Generate**: WIR → WASM bytecode with safety guarantees

### Variable and Scope Management

- **AST Stage**: Variable declarations create scope entries
- **WIR Stage**: Variables become places with precise lifetime tracking
- **Scope Context**: Maintained throughout transformation pipeline
- **Memory Model**: Polonius-based borrow checking with region inference

### Type System Integration

- **Early Checking**: Types resolved during AST construction
- **WIR Preservation**: Type information carried through to WIR with place types
- **Operation Selection**: WIR operations chosen based on types and borrowing
- **WASM Mapping**: Types and lifetimes determine WASM instruction selection

## Development Guidelines

### Adding New Language Features

1. **AST Representation**: Define in `ast_nodes.rs`
2. **Parsing Logic**: Add to appropriate parser module
3. **Constant Folding**: Extend if compile-time evaluation possible
4. **WIR Transformation**: Implement in `build_wir.rs` with place-aware lowering
5. **Borrow Checking**: Add lifetime and borrowing rules if needed
6. **WASM Codegen**: Add to codegen modules with lifetime preservation
7. **Testing**: Comprehensive unit and integration tests including borrow checker tests

### WIR Development Patterns

**Place Construction**: Always use place abstraction for memory locations
```rust
// Good: Place-based assignment
let place = Place::Local(local_id);
let rvalue = Rvalue::Use(Operand::Copy(source_place));
statements.push(Statement::Assign { place, rvalue });

// Bad: Direct variable manipulation
statements.push(IRNode::SetInt(var_id, value, is_global));
```

**Lifetime Tracking**: Generate facts during WIR construction
```rust
// Track borrows with precise points
let loan_id = self.issue_loan(region, borrow_kind, borrowed_place);
self.facts.loan_issued_at.push((point, loan_id, region_live_at));
```

**Interface Lowering**: Convert interface calls to vtable dispatch
```rust
// Interface method call becomes indirect call
let vtable_place = self.get_vtable_for_type(receiver_type);
let method_ptr = self.load_method_from_vtable(vtable_place, method_id);
```

### Error Message Quality

- **Be Specific**: Include exact tokens, types, or names in errors
- **Be Helpful**: Suggest corrections when possible, especially for borrow checker errors
- **Be Precise**: Use exact source locations and WIR points for lifetime errors
- **Be Consistent**: Follow established error message patterns
- **Lifetime Hints**: Provide lifetime elision suggestions and borrow scope recommendations

### Testing Patterns

- **Unit Tests**: Individual WIR transformation functions and place construction
- **Integration Tests**: Full pipeline with `.bs` files in `tests/cases/`
- **Borrow Checker Tests**: Verify correct lifetime analysis and error detection
- **Error Tests**: Verify proper error types and messages including lifetime errors
- **Edge Cases**: Complex borrowing patterns, interface dispatch, nested structures
- **Performance Tests**: Ensure WIR compilation time remains reasonable

## Memory and Performance Considerations

### Compile-Time Optimization Priority

The compiler prioritizes compile-time work to reduce runtime overhead:
- **Aggressive folding** eliminates runtime calculations
- **Type resolution** prevents runtime type checking  
- **RPN conversion** optimizes expression evaluation
- **Lifetime inference** eliminates runtime borrow checking
- **Early error detection** prevents runtime failures

### WIR Design for WASM Efficiency

- **Place-based analysis** enables precise memory layout optimization
- **Statement-level granularity** allows fine-grained optimization
- **Lifetime-aware codegen** eliminates unnecessary reference counting
- **Interface vtable optimization** reduces dynamic dispatch overhead
- **Move semantics** minimize copying and reference counting

### Polonius Integration Performance

- **Fact generation** happens incrementally during WIR construction
- **Constraint solving** uses efficient algorithms for region inference
- **Caching** of borrow checker results for incremental compilation
- **Parallel analysis** where possible for large functions

### Compilation Speed vs. Safety Tradeoffs

Beanstalk makes deliberate tradeoffs for compilation speed:
- **Interfaces instead of traits** avoid complex trait resolution
- **Simplified generics** reduce monomorphization overhead
- **Eager lifetime inference** prevents complex constraint solving
- **Statement-level WIR** balances precision with compilation speed

This architecture ensures that the Beanstalk compiler produces efficient WASM with memory safety guarantees while maintaining reasonable compilation times and providing excellent developer experience through precise error reporting and compile-time optimization.