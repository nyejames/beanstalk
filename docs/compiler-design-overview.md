# Compilation
## What is this compiler?
- A high-level language with templates as first-class citizens 
- Near-term target is a stable JS backend/build system for static pages and JS output. Wasm remains the long-term primary target.
- Build systems can use the compiler up through HIR (and borrow checking) and then apply their own codegen for any backend, including potential Rust-interpreter-backed builds.
- A modular compiler exposed as a library, plus a build system, dev server and CLI that assemble single-file and multi-file projects into runnable bundles.
- Ownership treated as an optimisation (GC is the fallback).

### Frontend structure at a glance
- `src/compiler_frontend/mod.rs` wires stages together
- `src/compiler_frontend/headers/` parses top-level declarations, defering parsing the bodies of functions or type checking until AST stage.
- `src/compiler_frontend/ast/` builds the typed AST
- `src/compiler_frontend/declaration_syntax/` stores the shared syntax parsing for both header and AST stage
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
- The module entry file has an implicit `start` function containing its top-level runtime code.
- Non-entry files do not participate in entry execution and cannot contain top-level executable code.
- Header parsing represents the entry file’s top-level runtime code as `HeaderKind::StartFunction`.
- The implicit entry `start` header is not part of dependency sorting.
- After top-level declaration headers are dependency-sorted, the entry `start` header is appended last and lowered by AST.
- Entry-file top-level runtime templates are evaluated inside that entry `start` function in source order.
- Entry-file top-level const templates are folded separately and exposed to project builders as compile-time fragment metadata.
- The entry `start` function returns the runtime fragment strings (`Vec<String>`) for the page in source order.

### Entry-page fragment interface

Project builders are aware of:

- the entry start function
- compile-time top-level fragment metadata produced by AST
- `module_constants` metadata in HIR
- backend output (for example JS or Wasm bundle)

Builders do **not** consume a HIR-level ordered start-fragment stream. Instead:

- AST folds entry-file top-level const templates into compile-time fragment strings
- each compile-time fragment records a **runtime insertion index**
- the entry `start()` function evaluates entry-file runtime top-level templates in source order
- the entry `start()` function returns `Vec<String>` containing those runtime fragment strings
- the builder merges compile-time fragments into the returned runtime fragment list using the recorded insertion indices

This keeps compile-time page fragments out of HIR and avoids a separate runtime fragment wrapper-function pipeline.

Exported constants exist so that **templates can reference them** and remain guaranteed-foldable.
They are also useful for constant data that wants to be shared module wide.

```beanstalk
# head_defaults = [:
  <meta charset="UTF-8">
]
```

-- `#[...]` is a top-level const template.
-- Top-level const templates are entry-file only.
-- They must fully fold at compile time.
-- Captures must be constant-only.
-- They become compile-time builder fragments, not HIR runtime fragments.
`#[html.head: [head_defaults]]`

## Pipeline Stages
The Beanstalk compiler frontend and build system processes modules through these stages:

0. **Project Structure** – Parses the config file and determines the boundaries of each module in the project
1. **Tokenization** – Convert source text to tokens
2. **Header Parsing** – Discover top-level declarations, collect strict top-level dependency edges, and build the implicit entry `start` body separately from top-level declarations
3. **Dependency Sorting** – Order parsed top-level declaration headers by strict dependency edges, detect cycles in the top-level graph, and append the implicit entry `start` header last
4. **AST Construction** – Lower the already-shaped, already-sorted top-level headers, resolve and validate them, and parse executable bodies and body-local declarations. All type checking happens here.
5. **HIR Generation** – Semantic lowering with explicit control flow
6. **Borrow Validation** – An analysis pass to verify memory safety

Project builders then perform:

7. **Backend Lowering**
    - JS Backend (current stabilisation target): HIR → JavaScript (GC-only)
    - Wasm Backend (long-term primary target): HIR → LIR → Wasm (GC-first, ownership-eliding over time)
    - Other build systems: reuse the shared pipeline through HIR (and borrow checking) and apply custom codegen while keeping Beanstalk semantics identical across targets

### Stage 0: Project Structure (`src/build_system/create_project_modules.rs`)
Determines boundaries for each module in the project and the config for the project.

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
Top-level declaration discovery and parsing of top-level structs, choices and constants.
Parses the top-level shape of each declaration kind so later stages do not need to reconstruct it.
Does not parse anything in function bodies beyond capturing their token streams and tracking their dependencies.

- **Top-Level Declaration Discovery**: Header parsing is the only stage that discovers module-wide top-level declarations.
- **Declaration Parsing**: Top-level Function signatures, exported constant declarations and structs/choices are parsed here.
- **Strict Dependency Edge Collection**: Header parsing collects strict top-level dependency edges.
- **Implicit Entry Start Capture**: Entry-file top-level executable code is collected into a `HeaderKind::StartFunction` header for later AST lowering.
- **Import Collection**: Imports needed by top-level declarations are collected here for top-level dependency analysis.
- **Top-Level Const Fragments**: Entry-file top-level const templates are recorded as ordered compile-time fragment headers.
- **Runtime Fragment Counting**: Entry-file top-level runtime templates remain in the entry `start` body, while header parsing tracks how many runtime fragments precede each const fragment so builders can merge outputs correctly.

Exported constants are parsed as top-level declarations. Their declared type shape is header-owned, the AST later resolves and validates the initializer. Top-level struct field shapes and choice variant shapes are fully parsed in the header stage, but validated and type checked at the AST stage.

Header parsing also builds the header-owned `ModuleSymbols` package: the order-independent top-level symbol, import, export, builtin, and source-file metadata needed by dependency sorting and AST construction. Dependency sorting later finalizes only the sorted `declarations` list inside that package; AST consumes the package directly rather than rediscovering top-level symbols.

### Stage 3: Dependency Sorting (`src/compiler_frontend/module_dependencies.rs`)
Operates only on top-level declaration headers and only on strict dependency edges.
Allows the AST to lower the whole module in declaration order without rebuilding module-wide top-level symbol knowledge.
Does not use executable body references or soft expression-derived edges.
This enables full-module type checking while keeping top-level declaration ownership in the header stage.

- Topological sort of parsed top-level declaration headers
- Strict dependency edges only
- Cycle detection in the strict top-level declaration graph
- Missing strict dependency diagnostics
- Source-order stability where possible among otherwise-independent declarations
- The implicit entry `start` header is not part of the dependency graph and is appended after sorting

### Stage 4: AST Construction (`src/compiler_frontend/ast/mod.rs`)
Consumes the already-shaped, already-sorted top-level headers and the header-owned `ModuleSymbols` package from the header and dependency stages.
AST resolves and validates those headers, enforces file-local import visibility, lowers executable bodies, and prepares templates for HIR.
It does not rediscover top-level symbols or reparse top-level declaration shells.

- **Import Visibility**: AST resolves per-file import visibility while still using the shared module-wide top-level symbol package.
- **Top-Level Resolution**: AST resolves and validates constants, struct field types, and function signatures from the parsed header payloads.
- **Body Parsing**: Function bodies and the entry `start` body are parsed and lowered here.
- **Local Scope Growth**: Executable bodies register local declarations incrementally in source order. Body-local declarations reuse shared declaration syntax, but top-level declaration shells remain header-owned.
- **Namespace Resolution**: Variables keep full scoped paths, and uniqueness is enforced by scope rules rather than post-hoc recollection.
- **Template Preparation**: AST performs template composition, compile-time folding, helper elimination, and runtime render-plan preparation before HIR.

**Top-level vs body parsing**

- Top-level declaration parsing belongs to header parsing.
- Executable body parsing belongs to AST construction.
- Body-local declarations are parsed in source order during AST lowering of executable code.
- Dependency sorting exists only to order top-level declarations before AST begins; it does not apply inside executable bodies.

Internally, AST runs in this order: resolve file import bindings, resolve constants and struct field types, resolve function signatures, build the receiver-method catalog, emit AST nodes for executable bodies, then finalize template and constant metadata for HIR and builders.

**Type checking and coercion**

Generic expression evaluation determines the natural type of an expression and stays strict. Contextual promotion is applied afterwards by the frontend site that owns the boundary, such as a declaration or return slot.

- `parse_expression.rs` and `eval_expression.rs` determine the natural type of expressions and enforce operator typing.
- `type_coercion::compatibility` decides whether one type is accepted in another context.
- `type_coercion::numeric` applies explicit contextual promotions such as Int -> Float.
- `type_coercion::string` owns what can become string content at template boundaries.
- Declarations and returns may apply coercion after expression parsing; generic expression evaluation itself stays strict.
- Int -> Float is supported in explicit declaration / return contexts
- function arguments and match patterns still require exact compatibility
- Templates and template wrappers are accepted where string slices are expected because they lower to the same HIR/string representation
- builtin casts like Float(x) / Int(x) remain explicit frontend-owned syntax

**Constants**

The AST consumes the parsed exported-constant directly and type-checks the initializer without rebuilding the declaration from raw top-level syntax.

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
- Templates requiring runtime evaluation remain runtime template expressions and are lowered normally inside function bodies.
- AST owns template composition, compile-time folding, helper elimination, and render-plan construction.
- HIR only lowers finalized runtime templates that remain after AST folding.

**Top-level templates**
- Entry-file top-level const templates are folded in AST.
- Folded top-level const templates are exposed as builder-facing compile-time fragment metadata.
- Each compile-time top-level fragment stores a **runtime insertion index** describing where it should be merged into the final runtime fragment list.
- Entry-file top-level runtime templates remain ordinary runtime code inside the entry `start()` function.
- AST does not synthesize standalone runtime fragment wrapper functions for top-level templates.
- AST does not perform top-level runtime template capture replay or start-body pruning as part of template generation.
- The entry `start()` function returns `Vec<String>` containing runtime top-level fragment results in source order.
- Builders merge compile-time fragments into that returned runtime list using the recorded insertion indices.

**General template rules**
- Partial compile-time folding inside a runtime template is normal and expected.
- Wrapper/slot composition is AST-time machinery only.
- Wrapper-shaped final templates are not automatically compile-time constants.
- The deciding rule is whether the final template value still depends on runtime expressions.
- Raw slot-insert/helper artifacts are not stable program values and must not survive past AST composition.

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
The Semantic lowering stage, converting the fully typed AST into the first backend-facing semantic IR.
Makes control flow, locals, regions, and call structure explicit while still allowing nested expression trees for normal value construction and operators.
Assumes AST has already completed template folding, composition, and runtime render-plan preparation.
Does not carry compile-time top-level page fragment metadata and does not reconstruct missing template plans.

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

**Template Boundary**
- HIR assumes template inputs are already semantically complete.
- HIR does not fold templates or reconstruct missing template plans.
- HIR lowers remaining runtime templates as ordinary runtime expressions inside function bodies.
- Top-level const page fragments do not pass through HIR.

**Entry Start Runtime Output**
- HIR lowers the entry `start()` function normally.
- The entry `start()` function returns the runtime fragment strings for the page in source order.
- HIR does not carry compile-time top-level fragment placement metadata.
- Builder-facing compile-time fragment ordering is resolved before HIR and stays outside the HIR data model.

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
