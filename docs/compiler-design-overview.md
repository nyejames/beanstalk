# Compilation
## What is this compiler?
- A high-level language with templates as first-class citizens 
- Near-term target is a stable JS backend/build system for static pages and JS output. Wasm remains the long-term primary target.
- Build systems can use the compiler up through HIR (and borrow checking) and then apply their own codegen for any backend, including potential Rust-interpreter-backed builds.
- A modular compiler exposed as a library, plus a build system, dev server and CLI that assemble single-file and multi-file projects into runnable bundles.
- Ownership treated as an optimisation (GC is the fallback).

### Frontend structure at a glance
- `src/compiler_frontend/mod.rs` wires stages together
- `src/compiler_frontend/ast/` builds the typed AST
- `src/compiler_frontend/type_coercion/` owns type compatibility and contextual coercion rules
- `src/compiler_frontend/hir/` lowers AST into HIR
- `src/compiler_frontend/analysis/borrow_checker/` validates borrow/exclusivity rules

## Overview
Build systems create a `BackendBuilder` implementation and wrap it in a `ProjectBuilder` struct.
The backend builder also exposes any project-specific frontend style directives it wants to
register.
The frontend compiles modules up to HIR first, then the backend builder consumes those modules.

```rust
pub trait BackendBuilder {
    /// Build the project with the given configuration
    fn build_backend(
        &self,
        modules: Vec<Module>, // Each collection of files the frontend has compiled into modules
        config: &Config,      // Persistent settings across the whole project
        flags: &[Flag],       // Settings only relevant to this build
        string_table: &mut StringTable, // Shared path interning table for the whole build
    ) -> Result<Project, CompilerMessages>;
    
    /// Validate the project configuration
    fn validate_project_config(
        &self,
        config: &Config,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError>;

    /// Project-specific frontend style directives.
    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec>;
}

pub struct ProjectBuilder {
    pub backend: Box<dyn BackendBuilder + Send>,
}
```

Build and diagnostic path handling:
- Each top-level build/config-parse lifecycle creates one shared mutable `StringTable`.
- Stage 0 config loading, frontend compilation, backend validation/build, and final diagnostic rendering all reuse that same table.
- `SourceLocation` stores an interned scope/path, not an owned diagnostic `PathBuf`.
- Rendering and filesystem-adjacent code resolve `SourceLocation.scope` through the shared `StringTable`.
- `BuildResult` and failed `CompilerMessages` own that table at the boundary so later output writing, terminal rendering, and dev-server reporting can still resolve paths consistently.

Project builders:
- Decide how modules are interpreted
- Decide how output files are structured
- Select and run backend code generation
- Emit artefacts (HTML, JS, Wasm, tooling output, etc.)
- Optionally register additional frontend style directives

Frontend style directives:
- Compiler built-ins are always available.
- Build systems can provide additional project-specific directives via `frontend_style_directives`.
- Build systems cannot override frontend-owned directive names.
- Tokenizer and template parsing use the same merged registry and reject unknown directives strictly.

Project builders do **not**:
- Parse files
- Discover modules
- Read configuration files directly
- Perform semantic compilation

Since compile speed is a goal of the compiler, complex optimisations are left to external tools for release builds only.

**Entry Point Semantics**:
- Each file has an **implicit start function** containing its top-level code
- Header parsing represents each file's top-level code as `HeaderKind::StartFunction`
- Imported files' implicit start functions are callable but do not execute automatically
- Exactly one file is chosen as the module entry file; that file's start function is the module start function in HIR
- The entry file can optionally contain top-level const templates that are consumed by project builders

### Start fragments and the builder interface

Project builders are aware of:

- the entry start function
- an ordered `start_fragments` stream
- `module_constants` metadata in HIR
- backend output (for example JS or Wasm bundle)

`start_fragments` interleave:

* compile-time strings (`ConstString`)
* runtime fragment functions (`RuntimeStringFn`)

Builders **do not** consume arbitrary exports directly. They consume the ordered fragments and decide how to materialize output for their target.

Exported constants exist so that **templates can reference them** and remain guaranteed-foldable.
They are also useful for constant data that wants to be shared module wide.

Example:

```beanstalk
# head_defaults = [:
  <meta charset="UTF-8">
]

-- `#[...]` is a top-level const template.
-- Top-level const templates are entry-file only.
-- They must fully fold at compile time.
-- Captures must be constant-only.
-- Slots are allowed if their resolved content is constant.
#[html.head: [head_defaults]]
```

## Pipeline Stages
The Beanstalk compiler frontend and build system processes modules through these stages:

0. **Project Structure** – Parses the config file and determines the boundaries of each module in the project
1. **Tokenization** – Convert source text to tokens
2. **Header Parsing** – Extract headers and identify the entry point. Separates function definitions, structs and constants from top-level code. Processes import statements so dependencies can be sorted after.
3. **Dependency Sorting** – Order headers by import dependencies (including constant dependencies)
4. **AST Construction** – Name resolution, type checking, constant resolution/folding and template lowering
5. **HIR Generation** – Semantic lowering with explicit control flow
6. **Borrow Validation** – An analysis pass to verify memory safety

Project builders then perform:

7. **Backend Lowering**
    - JS Backend (current stabilisation target): HIR → JavaScript (GC-only)
    - Wasm Backend (long-term primary target): HIR → LIR → Wasm (GC-first, ownership-eliding over time)
    - Other build systems: reuse the shared pipeline through HIR (and borrow checking) and apply custom codegen while keeping Beanstalk semantics identical across targets

### Stage 0: Project Structure (`src/build_system/create_project_modules.rs`)
Determines the boundaries of each module in the project and the config for the project.

**key Features**:
- Provides a canonical opinionated project structure
- Discovers module entry files (`#*.bst`, excluding `#config.bst`)
- Expands each module to reachable `.bst` files via recursive import resolution
- Parses and validates project config constants
- Determines which top-level root folders are visible to imports and future path resolution
- Provides the project builder with the file name and path to each module's entry point file

**`#config.bst`**
- A project-level configuration file
- Always located at the project root
- Parsed using normal Beanstalk declaration syntax
- Stage 0 reads top-level constants from it for build settings (`#entry_root`, `#output_folder`, `#root_folders`, project metadata, and custom keys)
- Provides a unified configuration map for all build systems

**`#*` Files and Modules**
- Any file whose name starts with `#` defines a **module root**
- Any directory containing a `#*` file is treated as a separate module
- The exact name of the file (e.g. `#page`, `#layout`, `#lib`) is preserved and interpreted by the build system
- The project builder can be aware of multiple `#` files per root, but they can only exist at the root of a module

### Stage 1: Tokenization (`src/compiler_frontend/tokenizer/lexer.rs`)
Converts raw source code into structured tokens with location information.

- Precise source location tracking for error reporting
- Recognition of Beanstalk-specific syntax
- Context switching for delimiter handling templates / strings (`[]` vs `""`)

### Stage 2: Header Parsing (`src/compiler_frontend/headers/parse_file_headers.rs`)
Extracts function definitions, structs, constants, imports and identify entry points before AST construction.

- **Header Extraction**: Separates declarations from top-level code
- **Implicit Start Function**: Top-level code that does not fit other header categories is collected into a `HeaderKind::StartFunction` header.
- **Entry Path Tracking**: Entry-file status is tracked separately and used in later stages.
- **Import Resolution**: Processes import declarations
- **Dependency Analysis**: Builds import graph and detects circular dependencies
- **Collect Constants**: Collect exported constants as declaration syntax plus dependency metadata
- **Preserve Top-Level Template Order**: Entry-file top-level templates are tracked in source order as ordered template items (`ConstTemplate` / `RuntimeTemplate`) for later fragment lowering.

### Stage 3: Dependency Sorting (`src/compiler_frontend/module_dependencies.rs`)
Orders headers topologically to ensure the proper compilation sequence so the AST for the whole module can be created in one pass. 
This enables the AST to perform full type checking.

- Topological sort of import dependencies
- Constant dependency ordering across files
- Source-order stability for constants declared in the same file
- Circular dependency detection
- Missing dependency diagnostics

### Stage 4: AST Construction (`src/compiler_frontend/ast/module_ast/mod.rs`)
Transforms headers into Abstract Syntax Tree with compile-time optimizations.

- **Header Integration**: Convert headers to AST nodes
- **Entry Point Handling**: The entry file path selects which start function is exposed as the module start function.
- **Constant Resolution Pass**: Constants are resolved in dependency order before general body lowering.
- **Constant Folding**: Immediate evaluation of compile-time expressions
- **Namespace Resolution**: Makes sure that variables exist and are unique to the entire module.
Variables store their full path including their parents in their name, the last part of the path is the variable name.
- **Type Checking**: Early type resolution and validation

**Type checking and coercion**

Generic expression evaluation determines the natural type of an expression and stays strict. Contextual promotion is applied afterwards by the frontend site that owns the boundary, such as a declaration or return slot.

- `parse_expression.rs` and `eval_expression.rs` determine the natural type of expressions and enforce operator typing.
- `type_coercion::compatibility` decides whether one type is accepted in another context.
- `type_coercion::numeric` applies explicit contextual promotions such as Int -> Float.
- `type_coercion::string` owns what can become string content at template boundaries.
- Declarations and returns may apply coercion after expression parsing; generic expression evaluation itself stays strict.
- Int -> Float is supported in explicit declaration / return contexts
- function arguments and match patterns still require exact compatibility
- templates and template wrappers are accepted where string slices are expected because they lower to the same HIR/string representation
- builtin casts like Float(x) / Int(x) remain explicit frontend-owned syntax

**Constant rules enforced by AST**:
- Constant declarations share declaration syntax with normal variables
- Constants cannot be mutable
- Constants must be initialized
- Constant initializers may only reference constants
- Constants must be fully foldable at compile time
- Top-level const templates are entry-file only and must fully fold
- Slots are supported in const templates if they resolve to constant values

**Compile-Time Folding**: The AST stage performs aggressive constant folding in `src/compiler_frontend/optimizers/constant_folding.rs`:
- Pure literal expressions (e.g., `2 + 3`) are evaluated immediately
- Results in `ExpressionKind::Int(5)` rather than runtime operations
- Expressions are converted to **Reverse Polish Notation (RPN)** for evaluation

#### Templates
- Templates fully resolved at the AST stage become string literals before HIR.
- Templates requiring runtime evaluation are lowered into **explicit template functions**.
- Top-level const templates are fully folded (or throw a rule error).
- Entry-file top-level templates become ordered `start_template_items` so HIR can build canonical start fragments.
- AST owns normal template parsing and folding boundaries. HIR still keeps a narrow transitional
  constant-lowering fallback for template values that arrive in already-constant contexts.

**Runtime Expressions**: When expressions cannot be folded at compile time:
- Variables, function calls or complex operations become `ExpressionKind::Runtime(Vec<AstNode>)`
- The `Vec<AstNode>` contains the expression in **RPN order** ready for stack-based evaluation
- Example: `x + 2 * y` becomes `[x, 2, y, *, +]` in the Runtime vector

**Type System Integration**: 
- Type checking occurs during AST construction
- `DataType` information is attached to all expressions
- Type mismatches are caught early in the pipeline
- Module constants are stored in AST constant metadata, not emitted as top-level runtime declaration statements
- Builtin error types (`Error`, `ErrorKind`, `ErrorLocation`, `StackFrame`) are registered from `src/compiler_frontend/builtins/error_type.rs` and lowered as canonical frontend-owned types

## Stage 5: HIR Generation (`src/compiler_frontend/hir/`)
HIR (High-Level IR) is Beanstalk’s semantic lowering stage.
It converts the fully typed AST into the first backend-facing semantic IR.
HIR makes control flow, locals, regions, and call structure explicit while still allowing nested expression trees for normal value construction and operators.
Normal template parsing/folding belongs to AST; HIR only has a narrow transitional constant-template fallback for already-constant lowering paths.

HIR is the first stage where resource lifetime semantics are made explicit, but ownership is not fully resolved yet.

### Purpose
- Linearize control flow
- Make evaluation order explicit
- Normalize control flow (if, loop, break, return) into explicit blocks
- Preserve enough structure to reason about variable usage and exclusivity
- Prepare the program for mandatory borrow validation and final lowering
- Serve as a shared source for all backends

### Key Features
**Linear Control Flow**
- All control-flow is explicit via blocks, jumps and terminators
- Expression trees remain nested for ordinary operators and value construction

**Start Fragment Stream**
- HIR exposes `start_fragments` for project builders.
- `StartFragment::ConstString` references folded compile-time strings in `const_string_pool`.
- `StartFragment::RuntimeStringFn` references generated runtime fragment functions in source order.

**Module Constant Pool**
- HIR exposes `module_constants` as compile-time metadata.
- Module constants are not lowered as runtime top-level statements.
- Backends/builders can consume `module_constants` for tooling or codegen decisions.

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

## Stage 6: Borrow Validation (`src/compiler_frontend/analysis/borrow_checker/`)
Statically determines which values are not managed by the GC heap.
Whether a drop actually happens is decided at runtime via ownership flags OR whether the value is managed by the GC.

**Does Not**
- Compute exact lifetimes
- Track per-field or per-projection aliasing
- Require ownership to be statically resolved

Borrow validation is a mandatory frontend phase for backend semantic parity:
- Programs that violate borrow/exclusivity rules are rejected before backend lowering.
- Programs that pass borrow validation can additionally be optimized for non-GC lowering in capable backends.

The borrow checker does not mutate the HIR, it produces side-table facts keyed by node / value IDs. HIR remains a stable semantic representation.

HIR represents semantic meaning under GC. Ownership is an optimization layer, not semantics. Later stages consult these facts during lowering.

Project builders and debug builds can skip optional post-borrow analyses to avoid compile time overhead,
but mandatory borrow validation itself is not optional.

**`Drop if owned` Insertion**
- This analysis can reveal conditional drop_if_owned(x) locations:
    1. At block exits
    2. On return
    3. On break from ownership-bearing scopes

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

## Language Notes
There are **no temporaries** at the language level. Compiler-introduced locals are treated exactly like user locals.

**No Shadowing**: Beanstalk disallows variable shadowing, meaning each place name refers to exactly one memory location throughout its scope. This simplifies borrow checking significantly.