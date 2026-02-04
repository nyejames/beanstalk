# Compilation 
Beanstalk’s compiler enforces memory safety through a hybrid strategy: A fallback garbage collector combined with increasingly strong static analysis that incrementally removes the need for runtime memory management.

All programs are correct under GC. Programs that satisfy stronger static rules run faster. Beanstalk treats ownership as an optimization target.

If static guarantees are missing or incomplete, the value falls back to GC.

Current state: early development with a JS backend/build system as the first milestone for generating static pages and JS output. Syntax and constructs (e.g., closures, interfaces) are still being shaped before full pipeline support. Wasm remains the long-term primary target.

In early compiler iterations and in the JavaScript backend, all heap values are managed by a garbage collector. As the compiler matures, static analyses (last-use analysis, borrow validation, region reasoning) are layered on top to eliminate GC participation where possible, especially for the Wasm backend.

At runtime, ownership is resolved via tagged pointers, allowing a single calling convention for borrowed and owned values.

This style of memory management can be incrementally strengthened with region analysis, stricter static lifetimes or place-based tracking in future iterations of the compiler without having to change the language semantics.

## Overview
Build systems can drive the compiler through header parsing, AST, HIR and borrow checking, then run their own codegen for any backend (including Rust-interpreter-backed flows). For the Wasm target, the build system groups files for a single Wasm module; for JS and other targets, the same pipeline is reused before custom emission.

JS backend: AST or HIR is lowered directly to JavaScript with GC-only semantics.

Wasm backend: HIR is progressively enriched with ownership information and lowered to Wasm, initially relying on Wasm GC and later reducing it. All files compile to one WASM module with proper function exports.

Since compile speed is a goal of the compiler, complex optimizations are left to external tools for release builds only.

**Entry Point Semantics**:
- Each file has an **implicit start function** containing its top-level code
- Every file in the module's implicit start function becomes `HeaderKind::StartFunction`
- Imported files' implicit start functions are callable but don't execute automatically
- Only one entry point is allowed per module, 
this is the `HeaderKind::Main` and is implicit start function of the entry file

## Pipeline Stages
The Beanstalk compiler processes modules through these stages:
1. **Tokenization** – Convert source text to tokens
2. **Header Parsing** – Extract headers and identify the entry point. Separates function definitions, structs and constants from top-level code. Processes `#import "path"` statements so dependencies can be sorted after.
3. **Dependency Sorting** – Order headers by import dependencies
4. **AST Construction** – Name resolution, type checking and constant folding
5. **HIR Generation** – Semantic lowering with explicit control flow and possible drop points inserted
6. (Optional) **Borrow Validation** – Verify memory safety
7. **Backend Lowering**
    - JS Backend (current stabilisation target): HIR → JavaScript (GC-only)
    - Wasm Backend (long-term primary target): HIR → LIR → Wasm (GC-first, ownership-eliding over time)
    - Other build systems: reuse the shared pipeline through HIR (and borrow checking) and apply custom codegen while keeping Beanstalk semantics identical across targets

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
- Linearize control flow
- Make evaluation order explicit
- Normalize control flow (if, loop, break, return) into explicit blocks
- Insert possible drop points based on control flow
- Preserve enough structure to reason about variable usage and exclusivity
- Prepare the program for optional borrow validation and final lowering
- Serve as a shared source for all backends

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

- Whether a drop actually happens is decided at runtime via ownership flags OR whether the value is managed by the GC

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
Use the `show_hir` flag to see the output.

---

## Stage 6: Borrow Validation (`src/compiler/borrow_checker/`)
Borrow validation is not required for correctness, it's for optimization:
- If a value passes borrow validation it becomes eligible for non-GC lowering.
- If it fails or is unanalyzed it remains GC-managed.

The borrow checker does not mutate the HIR, it produces side-table facts keyed by node / value IDs. HIR remains a stable semantic representation.

HIR represents semantic meaning under GC. Ownership is a provable optimization layer, not semantics. Later stages consult these facts during lowering.

### Purpose
Statically determine which values are not managed by the GC heap.

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
Ensures all values that might own data eventually reach a drop site.

---

## Wasm Backend (HIR → LIR → Wasm)

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

## Stage 8: Codegen (`src/compiler/codegen/`)
Transforms LIR directly into Wasm bytecode.

### Purpose
- Encode LIR instructions into valid WebAssembly.
- Produce linear memory layout, data segments, function tables and exports.

### Key Features
- **Direct Encoding**: LIR nodes correspond 1:1 (or close) to Wasm bytecode

---

# Beanstalk Memory Model and Borrow Semantics
Beanstalk treats ownership like vectorization or inlining. Programmers who follow the rules get faster code.

Ownership is not a type-level distinction, it's a runtime property constrained by static rules.
If those static rules are not followed, then GC is used as a fallback.

### Rules
Rather than forcing ownership correctness:
- Beanstalk treats ownership like vectorization or inlining
- Programmers who follow the rules get faster code
- Everyone else still gets correct code

### 1. Shared References (Default)
- Borrowing is the Default
- Multiple shared references to the same data are allowed
- Shared references are read-only access
- Created by default assignment: `x = y`
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
- Moves are identified via last-use analysis but finalized at runtime using ownership flags
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

Beanstalk inserts conditional drops and proves that *all paths are safe* for values that follow the borrow checker rules. If not, then they are managed by the fallback GC.

## Language Notes
There are **no temporaries** at the language level. Compiler-introduced locals are treated exactly like user locals.

**No Shadowing**: Beanstalk disallows variable shadowing, meaning each place name refers to exactly one memory location throughout its scope. This simplifies borrow checking significantly.

# Future

## Region-Based Memory Management
Regions can be introduced as compile-time scopes that guarantee collective drop behavior, allowing the compiler to:
- Elide runtime ownership checks
- Bulk-free memory
- Strengthen static guarantees incrementally
