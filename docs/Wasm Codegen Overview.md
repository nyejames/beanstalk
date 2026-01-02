# WASM Codegen Overview

This document explains the structure and functionality of the WASM codegen stage in the Beanstalk compiler. The codegen transforms Beanstalk's Low-Level Intermediate Representation (LIR) into valid WebAssembly bytecode.

## What is WebAssembly?

WebAssembly (WASM) is a binary instruction format designed as a portable compilation target. A valid WASM module consists of several sections that must appear in a specific order:

1. **Type Section** - Function signatures (parameter and return types)
2. **Import Section** - External functions, memories, tables, globals
3. **Function Section** - Function type indices (links functions to their signatures)
4. **Table Section** - Function tables for indirect calls
5. **Memory Section** - Linear memory declarations
6. **Global Section** - Global variable declarations
7. **Export Section** - Functions/memories/tables exposed to host
8. **Start Section** - Optional entry point function
9. **Element Section** - Table initialization data
10. **Code Section** - Function bodies (actual instructions)
11. **Data Section** - Memory initialization data

The codegen must produce these sections in the correct order with consistent indices across sections.

## Architecture Overview

```
LIR Module → Analysis → WASM Generation → Validation → WASM Bytes
     ↓           ↓            ↓             ↓           ↓
  Functions   Local Maps   Section Build  Validation  Output
  Structs     Type Maps    Instruction    Error       Module
  Globals     Index Maps   Generation     Handling
```

## File Structure

```
src/compiler/codegen/wasm/
├── mod.rs              # Module entry point and documentation
├── constants.rs        # Shared constants (ownership bits, memory config)
├── encode.rs           # Main entry point: encode_wasm()
├── analyzer.rs         # LIR analysis and type extraction
├── module_builder.rs   # WASM section construction
├── instruction_lowerer.rs  # LIR → WASM instruction translation
├── control_flow.rs     # Block/loop/if structure management
├── local_manager.rs    # Local variable index mapping
├── memory_layout.rs    # Struct field offset calculation
├── memory_manager.rs   # Linear memory and bump allocator
├── ownership_manager.rs # Tagged pointer ownership system
├── host_functions.rs   # Import/export handling
├── validator.rs        # wasmparser validation integration
├── optimizer.rs        # Basic instruction optimization
└── error.rs            # WASM-specific error types
```

## Component Responsibilities

### encode.rs - Main Entry Point
The `encode_wasm()` function orchestrates the entire codegen pipeline:
1. Analyzes the LIR module
2. Builds WASM sections
3. Lowers instructions
4. Validates the output
5. Returns WASM bytes

### analyzer.rs - LIR Analysis
Extracts information from LIR needed for WASM generation:
- Function signatures and types
- Local variable types and counts
- Struct layouts and field offsets
- Import/export requirements

### module_builder.rs - Section Construction
Manages WASM module construction using the `wasm_encoder` library:
- Maintains proper section ordering
- Coordinates indices across sections
- Handles type deduplication
- Manages exports and imports

### instruction_lowerer.rs - Instruction Translation
Converts LIR instructions to WASM bytecode:
- Arithmetic operations (add, sub, mul, div)
- Comparison operations (eq, ne, lt, gt)
- Memory operations (load, store)
- Local variable access (get, set, tee)
- Control flow delegation

### control_flow.rs - Control Flow Management
Handles WASM's structured control flow:
- Block nesting and depth tracking
- Loop constructs with break targets
- If/else with proper BlockType
- Branch instruction generation

### local_manager.rs - Local Variable Mapping
Maps LIR locals to WASM local indices:
- Parameters come first (indices 0..param_count)
- Locals follow (indices param_count..)
- Groups locals by type for efficient encoding

### memory_layout.rs - Struct Layout
Calculates memory layouts for structs:
- Field offsets with proper alignment
- Total size calculation
- Alignment requirements for tagged pointers

### memory_manager.rs - Memory Management
Sets up WASM linear memory:
- Memory section with min/max pages
- Bump allocator using globals
- Allocation and free functions

### ownership_manager.rs - Ownership System
Implements Beanstalk's tagged pointer system:
- Ownership bit manipulation (1=owned, 0=borrowed)
- Pointer masking operations
- Possible_drop generation
- Unified ABI for function calls

## LIR to WASM Pipeline Example

Here's how a simple function flows through the pipeline:

### Input: LIR Function
```rust
LirFunction {
    name: "add_one",
    params: [LirType::I32],
    returns: [LirType::I32],
    locals: [],
    body: [
        LirInst::LocalGet(0),    // Get parameter
        LirInst::I32Const(1),    // Push constant 1
        LirInst::I32Add,         // Add them
        LirInst::Return,         // Return result
    ],
}
```

### Step 1: Analysis
The analyzer extracts:
- Function signature: `(i32) -> i32`
- Parameter count: 1
- Local count: 0
- Local mapping: `{0 -> 0}` (param 0 maps to WASM local 0)

### Step 2: Module Building
The module builder creates:
- Type section entry: `(func (param i32) (result i32))`
- Function section entry: type index 0
- Export entry: "add_one" -> function 0

### Step 3: Instruction Lowering
Each LIR instruction becomes WASM:
```
LocalGet(0)  → local.get 0
I32Const(1)  → i32.const 1
I32Add       → i32.add
Return       → return
```

### Step 4: Code Section
The function body is encoded:
```
(func $add_one (param i32) (result i32)
  local.get 0
  i32.const 1
  i32.add
  return
)
```

### Step 5: Validation
wasmparser validates:
- Stack types are consistent
- Local indices are valid
- Control flow is properly structured

### Output: WASM Bytes
A valid WASM module ready for execution.

## Beanstalk's Ownership System in WASM

Beanstalk uses tagged pointers for runtime ownership resolution:

### Tagged Pointer Format
```
Pointer value: 0x12345678
                       ^
                       └── Ownership bit (1=owned, 0=borrowed)
```

### Operations
```rust
// Tag as owned: ptr | 1
function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
function.instruction(&Instruction::I32Or);

// Tag as borrowed: ptr & ~1
function.instruction(&Instruction::I32Const(ALIGNMENT_MASK));
function.instruction(&Instruction::I32And);

// Test ownership: ptr & 1
function.instruction(&Instruction::I32Const(OWNERSHIP_BIT));
function.instruction(&Instruction::I32And);
```

### Possible Drop
At control flow boundaries, the compiler inserts conditional drops:
```wasm
;; possible_drop(ptr_local)
local.get $ptr_local
i32.const 1          ;; OWNERSHIP_BIT
i32.and
if
  local.get $ptr_local
  i32.const -2       ;; ALIGNMENT_MASK
  i32.and
  call $__bst_free
end
```

## Design Rationale

### Why This Structure?

1. **Separation of Concerns**: Each component has a single responsibility, making the code easier to understand and maintain.

2. **Testability**: Components can be tested independently with property-based tests.

3. **Extensibility**: New features (like new instruction types) can be added without modifying unrelated code.

4. **Error Handling**: Comprehensive error types with context make debugging easier.

### Why wasm_encoder?

The `wasm_encoder` library provides:
- Type-safe WASM construction
- Automatic section ordering
- Fluent API for instruction emission
- No manual byte manipulation

### Why Bump Allocator?

For the initial implementation:
- Simple and fast
- No fragmentation concerns
- Suitable for short-lived programs
- Can be replaced with more sophisticated allocators later

## Current Limitations

1. **HIR→LIR Missing**: The LIR module is a scaffold; actual HIR lowering doesn't exist yet.

2. **F32 Operations**: Only F64 floating-point operations are implemented.

3. **Unsigned Operations**: Missing unsigned integer division and comparison.

4. **Bitwise Operations**: Missing AND, OR, XOR, shifts.

5. **String Handling**: No string data section or constant handling.

6. **Indirect Calls**: Function pointers not supported.

## Future Work

### Priority 1: Complete LIR Stage
- Implement HIR→LIR transformation
- Add ownership resolution during lowering
- Insert possible_drop at control flow boundaries

### Priority 2: Complete Instruction Set
- Add F32 operations
- Add unsigned integer operations
- Add bitwise operations
- Add type conversion operations

### Priority 3: Memory Model
- Implement string data section
- Add static data initialization
- Implement memory.copy/fill

### Priority 4: Advanced Features
- Indirect calls and function tables
- Multi-value return handling
- Exception handling

## Testing

Tests are located in `src/compiler_tests/`:
- `wasm_codegen_tests.rs` - Property-based tests for components
- `wasm_integration_tests.rs` - End-to-end pipeline tests

Run tests with:
```bash
cargo test --lib wasm_codegen_tests
cargo test --lib wasm_integration_tests
```

## References

- [WebAssembly Specification](https://webassembly.github.io/spec/)
- [wasm_encoder Documentation](https://docs.rs/wasm-encoder/)
- [Beanstalk Memory Management](./Beanstalk%20Memory%20Management.md)
- [Beanstalk Compiler Design Overview](./Beanstalk%20Compiler%20Design%20Overview.md)
