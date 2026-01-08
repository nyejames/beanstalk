# Compilation 
Beanstalk’s compiler enforces memory safety using last-use analysis combined with a unified runtime ownership ABI, rather than a fully static, place-based borrow checker.

The compiler statically enforces exclusivity rules (no simultaneous mutable accesses), determines possible ownership transfer points and inserts conditional drop sites.

At runtime, ownership is resolved via tagged pointers, allowing a single calling convention for borrowed and owned values.

This style of memory management can be incrementally strengthened with region analysis, stricter static lifetimes or place-based tracking in future iterations of the compiler without having to change the language semantics.

## Overview
The build system will determine which files are associated with a single Wasm module.
Those files are then all tokenized, parsed into headers and have their dependencies sorted. 
After this, everything is combined into a single AST that should be able to check all types and see all declarations in the module.

- **Single WASM Output**: All files compile to one WASM module with proper function exports
**Entry Point Semantics**:
- Each file has an **implicit start function** containing its top-level code
- Every file in the module's implicit start function becomes `HeaderKind::StartFunction`
- Imported files' implicit start functions are callable but don't execute automatically
- Only one entry point is allowed per module, 
this is the `HeaderKind::Main` and is implicit start function of the entry file

## Pipeline Stages
The Beanstalk compiler processes modules through these stages:
1. **Tokenization** – Convert source text to tokens
2. **Header Parsing** – Extract headers and identify the entry point. Separates function definitions, structs, constants from top-level code
3. **Dependency Sorting** – Order headers by dependencies
4. **AST Construction** – Build an abstract syntax tree - includes name resolution and constant folding
5. **HIR Generation** – Create a high-level IR with possible drop points inserted
6. **Borrow Validation** – Verify memory safety
7. **LIR Generation** – Create an IR close to Wasm that can be lowered directly
8. **Codegen** – Produce final Wasm bytecode

**Key Pipeline Principles**:
- **Import Resolution**: Processes `#import "path"` statements at the header stage so dependencies can be sorted after
- **Early optimization**: Constant folding and type checking at AST stage
- **Module-aware compilation**: Header parsing enables multi-file modules with proper entry point designation
- **No optimization passes in IR**: Complex optimizations left to external WASM tools for release builds only

### Stage 1: Tokenization (`src/compiler/parsers/tokenizer.rs`)
**Purpose**: Convert raw source code into structured tokens with location information.

**Key Features**:
- Precise source location tracking for error reporting
- Recognition of Beanstalk-specific syntax (`:`, `;`, `~`, `#import`)
- Context switching for delimiter handling (`[]` vs `""`)

**Development Notes**:
This stage of the compiler is stable and currently can represent almost all the tokens Beanstalk will need to represent.

---

### Stage 2: Header Parsing (`src/compiler/parsers/parse_file_headers.rs`)
**Purpose**: Extract function definitions, structs, constants, imports and identify entry points before AST construction.

**Key Features**:
- **Header Extraction**: Separates declarations from top-level code
- **Implicit Start Function**: Top level code that does not fit into the other header catagories is placed into a  `HeaderKind::StartFunction` header that becomes a public "start" function.
- **Entry Point Detection**: Identifies the entry file and converts its start function to `HeaderKind::Main`
- **Import Resolution**: Processes `#import "path/function_name"` directives
- **Dependency Analysis**: Builds import graph and detects circular dependencies

**Development Notes**:
Use `show_headers` feature flag to inspect parsed headers.

```rust
pub enum HeaderKind {
    Function(FunctionSignature, Vec<Token>),
    Template(Vec<Token>), // Top level templates are used for HTML page generation
    Struct(Vec<Arg>),
    Choice(Vec<Arg>),
    Constant(Arg),

    // The top-level scope of regular files.
    // Any other logic in the top level scope implicitly becomes a "start" function.
    // This only runs when explicitly called from an import.
    // Each .bst file can see and use these like normal functions.
    // Start functions have no arguments or return values
    // and are not visible to the host from the final wasm module.
    StartFunction(Vec<Token>),

    // This is the main function that the host environment can use to run the final Wasm module.
    // The start function of the entry file.
    // It has the same rules as other start functions,
    // but it is exposed to the host from the final Wasm module.
    Main(Vec<Token>),
}
```

---

### Stage 3: Dependency Sorting (`src/lib.rs::sort_headers`)
**Purpose**: Order headers topologically to ensure the proper compilation sequence so the AST for the whole module can be created in one pass. This enables the AST to perform full type checking.

**Key Features**:
- Topological sort of import dependencies
- Circular dependency detection
- Entry point validation (single entry per module)

---

### Stage 4: AST Construction (`src/compiler/parsers/ast.rs`)
**Purpose**: Transform headers into Abstract Syntax Tree with compile-time optimizations.

**Key Features**:
- **Header Integration**: Convert headers to AST nodes
- **Entry Point Handling**: StartFunction and Main headers are parsed into normal functions and given a reserved name. Only the main function is exposed to the host.
- **Constant Folding**: Immediate evaluation of compile-time expressions
- **Namespace Resolution**: Makes sure that variables exist and are unique to the scope
- **Type Checking**: Early type resolution and validation

**Compile-Time Folding**: The AST stage performs aggressive constant folding in `src/compiler/optimizers/constant_folding.rs`:
- Pure literal expressions (e.g., `2 + 3`) are evaluated immediately
- Results in `ExpressionKind::Int(5)` rather than runtime operations
- Expressions are converted to **Reverse Polish Notation (RPN)** for evaluation
#### Templates
- Templates fully resolved at the AST stage become string literals before HIR.
- Templates requiring runtime evaluation are lowered into **explicit template functions**.

**Runtime Expressions**: When expressions cannot be folded at compile time:
- Variables, function calls or complex operations become `ExpressionKind::Runtime(Vec<AstNode>)`
- The `Vec<AstNode>` contains the expression in **RPN order** ready for stack-based evaluation
- Example: `x + 2 * y` becomes `[x, 2, y, *, +]` in the Runtime vector

**Type System Integration**: 
- Type checking occurs during AST construction
- `DataType` information is attached to all expressions
- Type mismatches are caught early in the pipeline

**Development Notes**:
- Use `show_ast` feature flag to inspect generated AST

---

## Stage 5: HIR Generation (`src/compiler/hir/`)
HIR (High-Level IR) is Beanstalk’s semantic lowering stage.
It converts the fully typed AST into a linear, control-flow-explicit representation suitable for last-use analysis and ownership reasoning. HIR never performs template parsing or folding.

HIR is the first stage where resource lifetime semantics are made explicit, but ownership is not fully resolved yet.

HIR intentionally avoids full place-based tracking in the initial implementation to reduce complexity and enable incremental evolution of the memory model.

### Purpose
- Convert structured AST nodes into a linear, analyzable form
- Insert possible drop points based on control flow
- Normalize control flow (if, loop, break, return) into explicit blocks
- Preserve enough structure to reason about variable usage and exclusivity
- Prepare the program for borrow validation and final lowering

### Key Features
**Linear Control Flow**
- HIR contains no nested expressions or implicit scopes
- All control-flow is explicit via blocks, jumps and terminators

**Last-Use–Oriented Semantics**
- HIR does not model exact lifetimes
- Instead, it enables backwards last-use analysis
- Variables are tracked by symbol identity, not by place projections

**Possible Drop Insertion**
- Conditional possible_drop(x) nodes are inserted:
    1. At block exits
    2. On return
    3. On break from ownership-bearing scopes

- Whether a drop actually happens is decided at runtime via ownership flags

**Ownership Is Not Yet Final**
- HIR does not decide whether an operation is a move or borrow
- It records where ownership could be consumed
- Final ownership resolution happens during lowering

**Desugared Semantics**
- Assignment forms, mutation syntax and control-flow sugar are normalized
- All effects are explicit statements
- Calls to runtime templates appear as normal HIR call nodes

**Not Wasm-Shaped**
- No stack discipline
- No memory offsets
- No ABI lowering

#### Host Calls
- Builtins such as `io` are preserved as explicit call nodes.
- HIR assumes required host imports exist.
- No abstraction layer exists between HIR and host calls.

### Debugging HIR
HIR should read like a resource-aware CFG, not a tree. Use show_hir to inspect:
- Inserted possible_drop points
- Linearized control flow
- Explicit ownership boundaries

---

## Stage 6: Borrow Validation (`src/compiler/borrow_checker/`)
The borrow checker operates on HIR to enforce exclusivity and usage rules, not full lifetime correctness.

It ensures that all potential ownership transfers identified by last-use analysis are sound with respect to Beanstalk’s reference rules.

This stage enforces soundness, not maximal static precision.
Programs that pass this stage are memory safe, even if some ownership decisions are deferred to runtime.

Beanstalk aims to:
- Reject *definitely unsafe* programs.
- Accept programs that are *conditionally safe*.
- Use runtime checks to resolve safe ambiguity.

This avoids complex CFG reasoning and keeps compilation fast and predictable.

### Purpose
- Enforce “at most one mutable access” at any point
- Prevent use-after-move on statically determined moves
- Ensure mutable access is exclusive across control-flow joins
- Validate that possible ownership consumption is consistent

**Does Not**
- Compute exact lifetimes
- Track per-field or per-projection aliasing
- Require ownership to be statically resolved

### Key Features
**Exclusivity Checking**
Ensures no two mutable accesses overlap and mutable access excludes shared access.

**Move Safety**
Ensures uses do not follow statically determined moves.
Runtime-resolved moves rely on inserted drop points.

**Control-Flow Awareness**
Branches are checked independently and merges enforce conservative rules.

**Drop Safety**
- Ensures all values that might own data eventually reach a drop site.

---

## Stage 7: LIR Generation (`src/compiler/lir/`)
LIR (Low-Level IR) is the *Wasm-shaped* representation of the program.  
It is a close, structural match to Wasm’s execution model, with stack effects and control blocks fully explicit.

LIR contains no remaining high-level constructs from Beanstalk: everything has been lowered into concrete Wasm-compatible operations. It is where ownership becomes concrete.

### Purpose
- Transform HIR into an instruction-level IR that can be directly emitted as Wasm.
- Make control flow, locals and memory operations explicit.
- Insert drops, compute field offsets and rewrite multi-value returns.

### Key Features
- **Ownership Resolution**: Tagged pointers are generated and ownership flags are masked and tested. `possible_drop` nodes become conditional frees.
- **Wasm-Friendly Control Flow**: Blocks, loops and branches match Wasm’s structured CFG.
- **Concrete Memory Access**: All field and array accesses lowered to explicit offsets and load/store instructions.
- **Explicit Locals**: All temporaries materialized as Wasm locals or stack values.
- **Drop Semantics**: Ownership outcomes from HIR lowering translated to explicit drop/free operations.
- **Stack Discipline**: Expressions sequenced according to Wasm’s operand stack rules.
- **Final Type Model**: All values lowered to Wasm types (`i32`, `i64`, `f32`, `f64`, plus reference types if enabled).

### Debugging LIR
- Use `show_lir` to inspect Wasm-shaped blocks.
- Verify stack height balancing.
- Confirm struct layouts and offsets.
- Inspect lowered drop instructions and ownership decisions.

---

## Stage 8: Codegen (`src/compiler/codegen/`)
Transforms LIR directly into Wasm bytecode.

### Purpose
- Encode LIR instructions into valid WebAssembly.
- Produce linear memory layout, data segments, function tables and exports.

### Key Features
- **Direct Encoding**: LIR nodes correspond 1:1 (or close) to Wasm bytecode s

# Beanstalk Memory Model and Borrow Semantics
Beanstalk uses a borrow checker. But unlike Rust, ownership is not a type-level distinction, it's a runtime property constrained by static rules.

Beanstalk treats ownership as a runtime state constrained by compile-time guarantees, rather than a purely static property.

### Rules
### 1. Shared References (Default)
- Borrowing is the Default
- Multiple shared references to the same data are allowed
- Shared references are read-only access
- Created by default assignment: `x = y`
- Last-use analysis determines when they can be "killed"
- **No explicit `&` or `&mut` operators** - these don't exist in Beanstalk
- All variable usage creates immutable references by default

### 2. Mutable Access (`~` syntax)
- Mutability is always explicit
- Use `~` to indicate mutable access (reference or ownership)
- Only one mutable access allowed at a time
- Mutable access is exclusive (no other references allowed)
- Created by mutable assignment: `x ~= y`
- The compiler guarantees exclusivity statically; whether the access consumes ownership is resolved dynamically.

### 3. Ownership Transfer (Moves)
- - Moves are identified via last-use analysis but finalized at runtime using ownership flags
- The compiler determines when the last use of a variable happens statically for any given scope
- If the variable is passed into a function call or assigned to a new variable, and it's determined to be a move at runtime, then the new owner is responsible for dropping the value
- Otherwise, the last time an owner uses a value without moving it, a possible_drop() insertion will drop the value

### 4. Copies are Explicit
- No implicit copying for any types unless they are part of an expression creating a new value out of multiple references, or when used inside a template head
- All types require explicit copy semantics when copying is needed
- Most operations use borrowing instead of copying

### 5. Unified ABI for Moves and Mutable References
Beanstalk does not generate separate function bodies for “owned” vs “borrowed” arguments. 
Function signatures make no distinction between a mutable reference or a move (owned value). Instead, all function calls use a single ABI:

- Arguments that live in linear memory are passed as tagged pointers.
- The lowest alignment-safe bit of the pointer is used as an ownership flag (1 = owned, 0 = borrowed).
- The callee masks out the tag to recover the real pointer.
- If the ownership bit is set, the callee is responsible for dropping the value before returning.
- Borrow checker rules guarantee that the caller no longer uses owned arguments after the call.

This ABI allows Beanstalk to defer ownership decisions without sacrificing safety or requiring duplicate function bodies.

This design keeps dispatch static, avoids monomorphization and prevents binary-size growth on Wasm while still allowing the compiler to freely choose between moves and mutable references based on last-use analysis.

Rust’s borrow checker reasons about control-flow paths precisely and rejects programs that are ambiguous.

Beanstalk instead:
- Allows ambiguity.
- Inserts conditional drops.
- Proves that *all paths are safe*.

## Language Notes
There are **no temporaries** at the language level. Compiler-introduced locals are treated exactly like user locals.

**No Shadowing**: Beanstalk disallows variable shadowing, meaning each place name refers to exactly one memory location throughout its scope. This simplifies borrow checking significantly.

# Future

## Region-Based Memory Management
Regions can be introduced as compile-time scopes that guarantee collective drop behavior, allowing the compiler to:
- Elide runtime ownership checks
- Bulk-free memory
- Strengthen static guarantees incrementally