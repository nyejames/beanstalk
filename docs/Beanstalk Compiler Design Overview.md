# Compilation 
## What is this compiler?
- A high-level language with templates as first-class citizens and ownership treated as an optimisation (GC is the fallback).
- Near-term target is a stable JS backend/build system for static pages and JS output. Wasm remains the long-term primary target.
- Build systems can use the compiler up through HIR (and borrow checking) and then apply their own codegen for any backend, including potential Rust-interpreter-backed builds.
- A modular compiler exposed as a library, plus a build system and CLI that assemble single-file and multi-file projects into runnable bundles.

Beanstalk's compiler enforces memory safety through a hybrid strategy: A fallback garbage collector combined with increasingly strong static analysis that incrementally removes the need for runtime memory management.
All programs are correct under GC. Programs that satisfy stronger static rules run faster. Beanstalk treats ownership as an optimization target.
If static guarantees are missing or incomplete, the value falls back to GC.

## Current status
- Early development: The primary milestone is a stable JS build system/backend for static pages and JS output.
- Syntax and semantics are still shifting. Some constructs such as closures, interfaces or async are not final or fully implemented in the pipeline.

In early compiler iterations and in the JavaScript backend, all heap values are managed by a garbage collector. As the compiler matures, static analyses (last-use analysis, borrow validation, region reasoning) are layered on top to eliminate GC participation where possible, especially for the Wasm backend.

At runtime, ownership is resolved via tagged pointers, allowing a single calling convention for borrowed and owned values.

This style of memory management can be incrementally strengthened with region analysis, stricter static lifetimes or place-based tracking in future iterations of the compiler without having to change the language semantics.

## Overview
Build systems can create a function that implements the ProjectBuilder trait and pass that, along with a list of input files to the compiler's core build system.
The project builder takes the output of the compiler's frontend (A list of Hir modules) and performs the backend compilation stages.
The compiler's core build system then creates and writes the list of output files the project builder produced.

```rust
    /// Unified build interface for all project types
    pub trait ProjectBuilder {
    /// Build the project with the given configuration
    fn build_backend(
        &self,
        modules: Vec<Module>, // Each collection of files the frontend has compiled into modules
        config: &Config,      // Persistent settings across the whole project
        flags: &[Flag],       // Settings only relevant to this build
    ) -> Result<Project, CompilerMessages>;
    
        /// Validate the project configuration
        fn validate_project_config(&self, config: &Config) -> Result<(), CompilerError>;
    }
```

Project builders:
- Decide how modules are interpreted
- Decide how output files are structured
- Select and run backend code generation
- Emit artefacts (HTML, JS, Wasm, tooling output, etc.)

Project builders do **not**:
- Parse files
- Discover modules
- Read configuration files directly
- Perform semantic compilation

Since compile speed is a goal of the compiler, complex optimisations are left to external tools for release builds only.

**Entry Point Semantics**:
- Each file has an **implicit start function** containing its top-level code
- Every file in the module's implicit start function becomes `HeaderKind::StartFunction`
- Imported files' implicit start functions are callable but don't execute automatically
- Only one entry point is allowed per module, 
this is the `HeaderKind::Main` and is implicit start function of the entry file

## Pipeline Stages
The Beanstalk compiler frontend and build system processes modules through these stages:
0. **Project Structure** – Parses the config file and determines the boundaries of each module in the project
1. **Tokenization** – Convert source text to tokens
2. **Header Parsing** – Extract headers and identify the entry point. Separates function definitions, structs and constants from top-level code. Processes `#import "path"` statements so dependencies can be sorted after.
5. **Dependency Sorting** – Order headers by import dependencies
6. **AST Construction** – Name resolution, type checking and constant folding
7. **HIR Generation** – Semantic lowering with explicit control flow and possible drop points inserted
8. **Borrow Validation** – An analysis pass to verify memory safety

Project builders then perform:
9. **Backend Lowering**
    - JS Backend (current stabilisation target): HIR → JavaScript (GC-only)
    - Wasm Backend (long-term primary target): HIR → LIR → Wasm (GC-first, ownership-eliding over time)
    - Other build systems: reuse the shared pipeline through HIR (and borrow checking) and apply custom codegen while keeping Beanstalk semantics identical across targets

The core build system then assembles the output files from the project builder's output.

### Stage 0: Project Structure (`src/build_system/build.rs`)
**Purpose**: Determine the boundaries of each module in the project and the config for the project.

**key Features**:
- Provides a canonical opinionated project structure
- Discovers all the modules in the project
- Parses and validates the config
- Determines what libraries are available for import
- Provides the project builder with the file name and path to each module's entry point file

**`#config`**
- A project-level configuration file
- Always located at the project root
- Parsed and validated by the compiler
- Provides a unified configuration map for all build systems

**`#*` Files and Modules**
- Any file whose name starts with `#` defines a **module root**
- Any directory containing a `#*` file is treated as a separate module
- The exact name of the file (e.g. `#page`, `#layout`, `#lib`) is preserved and interpreted by the build system
- The project builder can be aware of multiple `#` files per root, but they can only exist at the root of a module

### Stage 1: Tokenization (`src/compiler_frontend/tokenizer/tokenizer.rs`)
**Purpose**: Convert raw source code into structured tokens with location information.

**Key Features**:
- Precise source location tracking for error reporting
- Recognition of Beanstalk-specific syntax
- Context switching for delimiter handling (`[]` vs `""`)

**Development Notes**:
This stage of the compiler is stable and currently can represent almost all the tokens Beanstalk will need to represent.

---

### Stage 2: Header Parsing (`src/compiler_frontend/headers/parse_file_headers.rs`)
**Purpose**: Extract function definitions, structs, constants, imports and identify entry points before AST construction.

**Key Features**:
- **Header Extraction**: Separates declarations from top-level code
- **Implicit Start Function**: Top level code that does not fit into the other header catagories is placed into a  `HeaderKind::StartFunction` header that becomes a public "start" function.
- **Entry Point Detection**: Identifies the entry file and converts its start function to `HeaderKind::Main`
- **Import Resolution**: Processes import declarations
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

### Stage 3: Dependency Sorting (`src/compiler_frontend/headers/parse_file_headers.rs`)
**Purpose**: Order headers topologically to ensure the proper compilation sequence so the AST for the whole module can be created in one pass. This enables the AST to perform full type checking.

**Key Features**:
- Topological sort of import dependencies
- Circular dependency detection
- Entry point validation (single entry per module)

---

### Stage 4: AST Construction (`src/compiler_frontend/parsers/ast.rs`)
**Purpose**: Transform headers into Abstract Syntax Tree with compile-time optimizations.

**Key Features**:
- **Header Integration**: Convert headers to AST nodes
- **Entry Point Handling**: StartFunction and Main headers are parsed into normal functions and given a reserved name. Only the main function is exposed to the host.
- **Constant Folding**: Immediate evaluation of compile-time expressions
- **Namespace Resolution**: Makes sure that variables exist and are unique to the scope
- **Type Checking**: Early type resolution and validation

**Compile-Time Folding**: The AST stage performs aggressive constant folding in `src/compiler_frontend/optimizers/constant_folding.rs`:
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

## Stage 5: HIR Generation (`src/compiler_frontend/hir/`)
HIR (High-Level IR) is Beanstalk’s semantic lowering stage.
It converts the fully typed AST into a linear, control-flow-explicit representation suitable for last-use analysis and ownership reasoning. HIR never performs template parsing or folding.

HIR is the first stage where resource lifetime semantics are made explicit, but ownership is not fully resolved yet.

HIR intentionally avoids full place-based tracking in the initial implementation to reduce complexity and enable incremental evolution of the memory model.

### Purpose
- Linearize control flow
- Make evaluation order explicit
- Normalize control flow (if, loop, break, return) into explicit blocks
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

**Ownership Is Not Yet Final**  
- HIR does not decide whether an operation is a move or borrow
- It records where ownership could be consumed
- Final ownership resolution happens during lowering

**Desugared Semantics**
- Assignment forms, mutation syntax and control-flow sugar are normalized
- All effects are explicit statements
- Calls to runtime templates appear as normal HIR call nodes

#### Host Calls
- Builtins such as `io` are preserved as explicit call nodes.
- HIR assumes required host imports exist.
- No abstraction layer exists between HIR and host calls.

### Debugging HIR
Use the `show_hir` flag to see the output.

---

## Stage 6: Borrow Validation (`src/compiler_frontend/borrow_checker/`)
Borrow validation is not required for correctness, it's for optimization:
- If a value passes borrow validation, it becomes eligible for non-GC lowering.
- If it fails or is unanalyzed, it remains GC-managed.

The borrow checker does not mutate the HIR, it produces side-table facts keyed by node / value IDs. HIR remains a stable semantic representation.

HIR represents semantic meaning under GC. Ownership is an optimization layer, not semantics. Later stages consult these facts during lowering.

While the single mutable access rule is always enforced,
project builders and debug builds can skip any further analysis to avoid compile time overhead.

**Possible Drop Insertion**
- Conditional possible_drop(x) locations can be revealed by this analysis:
    1. At block exits
    2. On return
    3. On break from ownership-bearing scopes

- Whether a drop actually happens is decided at runtime via ownership flags OR whether the value is managed by the GC

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
- Only one mutable access is allowed at a time
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
