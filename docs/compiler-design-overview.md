# Beanstalk Compiler Design Overview

Beanstalk is a high-level language with first-class string templates.
The compiler is modular, exposed as a library, and used by the built-in project tooling, dev server, and backend builders.

This document describes compiler stage ownership, data flow, and cross-stage contracts. Use:

- `docs/language-overview.md` for language syntax and user-facing semantics
- `docs/memory-management-design.md` for ownership, GC fallback, borrow analysis strategy, and lowering implications
- `docs/codebase-style-guide.md` for implementation standards
- `docs/roadmap/roadmap.md` for current planning
- `docs/src/docs/progress/#page.bst` for current implementation status

Build systems use the compiler through HIR and borrow validation, then apply their own backend lowering.
They assemble one or more compiled modules into runnable artifacts such as HTML, JS, Wasm, or other target outputs.

## Frontend structure at a glance

### Stage orchestration

- `src/compiler_frontend/mod.rs` is the frontend module map
- `src/compiler_frontend/pipeline.rs` owns the `CompilerFrontend` stage flow: source → tokens → headers → sorted headers → AST → HIR → borrow report

### Input, paths, diagnostics, and symbols

- `src/compiler_frontend/tokenizer/` converts source text into located tokens and handles string/template delimiter context
- `src/compiler_frontend/compiler_messages/` owns structured errors, warnings, source locations, metadata, and render-boundary message aggregation
- `src/compiler_frontend/symbols/`, `interned_path`, and `paths/` own interned source identities, path formatting/resolution, and canonical symbol identity shared across diagnostics, imports, and lowering

### Declarations, imports, and type surface

- `src/compiler_frontend/headers/` discovers top-level declarations, imports, normalized path/reference shells, declaration shells, constant initializer dependency hints, and start-body separation
- `src/compiler_frontend/module_dependencies.rs` orders top-level declaration headers by header-provided dependency edges, including constant initializer dependencies
- `src/compiler_frontend/declaration_syntax/` owns shared declaration parsing used by headers and body-local AST parsing
- `src/compiler_frontend/datatypes/` owns frontend type representations used across declarations, AST validation, HIR lowering, and backend-facing metadata
- `src/compiler_frontend/type_coercion/` owns contextual compatibility and promotion rules layered on top of type identity
- `src/compiler_frontend/value_mode.rs` tracks frontend access classification for bindings, expressions, call arguments, and receiver use. It keeps mutability/reference state separate from `DataType`; runtime ownership is a later borrow/lowering concern
- `src/compiler_frontend/source_libraries/` resolves builder/project source library roots into normal module inputs
- `src/compiler_frontend/external_packages/` stores backend-provided virtual package metadata and stable external symbol IDs
- `src/compiler_frontend/builtins/` owns compiler-defined language symbols and operations that are neither user source declarations nor backend-provided external packages
- `src/compiler_frontend/style_directives/` owns the merged frontend + builder directive registry used by tokenizer and template parsing
- `src/compiler_frontend/deferred_feature_diagnostics.rs` centralizes consistent diagnostics for documented or reserved language surface that is not implemented yet

### Semantic lowering and analysis

- `src/compiler_frontend/ast/` builds the typed AST from sorted headers, resolves semantic information, parses executable bodies, folds constants/templates, and prepares HIR input
- `src/compiler_frontend/optimizers/constant_folding.rs` supports AST compile-time evaluation for constants and foldable template expressions
- `src/compiler_frontend/hir/` lowers the typed AST into the first backend-facing semantic IR
- `src/compiler_frontend/analysis/borrow_checker/` validates borrow/exclusivity rules and produces side-table facts for later lowering

## Build-system and frontend boundary

Build systems provide a `BackendBuilder` implementation and wrap it in a `ProjectBuilder`.
The frontend compiles modules up to HIR and borrow validation. The backend builder then consumes those compiled modules and emits project artifacts.

```rust
pub trait BackendBuilder {
    fn build_backend(
        &self,
        modules: Vec<Module>,
        config: &Config,
        flags: &[Flag],
        string_table: &mut StringTable,
    ) -> Result<Project, CompilerMessages>;

    fn validate_project_config(
        &self,
        config: &Config,
        string_table: &mut StringTable,
    ) -> Result<(), CompilerError>;

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec>;

    fn libraries(&self) -> LibrarySet;
}

pub struct ProjectBuilder {
    pub backend: Box<dyn BackendBuilder + Send>,
}

pub struct LibrarySet {
    pub external_packages: ExternalPackageRegistry,
    pub source_libraries: SourceLibraryRegistry,
}
```

Backend builders do not parse source files, discover modules, read project config directly, or perform semantic frontend compilation.
They declare frontend-visible libraries/directives, validate config, and lower compiled modules into artifacts.

Complex release optimizations should remain outside the fast frontend path unless they are required for correctness.

### Diagnostic and path identity contract

A build lifecycle uses a `StringTable` across config loading, frontend compilation, backend validation/build, and diagnostic rendering.

- `SourceLocation` stores interned path/scope identity, not owned diagnostic paths

- Rendering and filesystem-adjacent code resolve interned paths through the `StringTable`

- Boundary types such as `BuildResult` and failed `CompilerMessages` carry the string table so later output writing, terminal rendering, and dev-server reporting can resolve paths consistently

### Style directive contract

Project builders can register style directives through `frontend_style_directives`.

- Frontend-owned directives are always available
- Builder directives cannot override frontend-owned names
- Tokenizer and template parsing use the same merged registry
- Unknown directives are rejected strictly

Individual directive syntax and behavior belong in `docs/language-overview.md`.

### Import, library, and external package contract
Stage 0 discovers source libraries as normal module inputs.

Header parsing/import preparation resolves imports, re-exports, aliases, facade boundaries, external package symbols, prelude symbols, builtins, and file-local visibility. It produces the visibility environment consumed by dependency sorting and AST.

Dependency sorting uses header-provided dependency edges.

AST consumes file-local visibility through `ScopeContext`. It validates semantic use of visible symbols, but it does not rebuild import bindings or rediscover import visibility.

Compiler-facing rules:

- Source libraries are normal modules behind `#mod.bst` facades
- External packages are virtual typed symbols provided by backend metadata, not `.bst` source files
- External imports resolve to stable frontend IDs such as `ExternalFunctionId`
- Expression/type resolution uses the active `ScopeContext` visibility maps, not global bare-name lookup
- Backends map stable external IDs to target-specific runtime names, imports, or helper calls

User-facing import syntax, facade rules, library categories, and deferred package features are detailed in `docs/language-overview.md`.

### Entry start and page fragments

The module entry file has an implicit `start()` function containing top-level runtime code.
Non-entry files contribute declarations only.

Header parsing captures the entry file’s top-level runtime code as a `HeaderKind::StartFunction`.
The implicit `start` header is not part of dependency sorting. It is appended after sorted top-level declarations and lowered by the AST.

Entry-file page fragments are split:

- Top-level runtime templates remain runtime code inside `start()`
- `start()` returns runtime fragment strings in source order
- Entry-file top-level const templates fold in AST into builder-facing compile-time fragments
- Each compile-time fragment records a runtime insertion index
- Builders merge compile-time fragments into the runtime fragment list
- HIR does not carry compile-time page fragments or a separate ordered start-fragment stream

### Top-level declaration shape

Header parsing owns top-level declaration discovery and declaration shell parsing.
These headers participate in strict top-level dependency sorting.

```beanstalk
# site_name = "Beanstalk"

# head_defaults = [:
  <meta charset="UTF-8">
]

UserId as Int

# Card = |
    title String,
|

# render_card |title String| -> String:
    return [: <article>[title]</article>]
;
```

Top-level constants, type aliases, structs, choices, function signatures, and relevant type annotations can create header-provided dependency edges.
Executable body references do not.

`#` syntax, constant rules, export behavior, and top-level template syntax are specified in `docs/language-overview.md`.

## Pipeline stages

The compiler frontend and build system process modules through these stages:

0. **Project Structure**: discovers config, module roots, reachable source files, source libraries, and external package namespaces.
1. **Tokenization**: converts source text to located tokens.
2. **Header Parsing**: parses imports, declaration shells, top-level dependency edges, constant initializer reference edges, and captures entry start body separately.
3. **Dependency Sorting**: orders top-level declaration headers by all header-provided top-level dependency edges.
4. **AST Construction**: consumes sorted headers linearly, resolves and validates semantic information, parses executable bodies, type-checks expressions, and prepares templates/constants for HIR/builders.
5. **HIR Generation**: lowers the typed AST into backend-facing semantic IR with explicit control flow.
6. **Borrow Validation**: validates borrow/exclusivity rules and produces side-table facts for later lowering.
7. **Backend Lowering**: project builders lower compiled modules into backend-specific artifacts.

## Stage 0: Project Structure

Path: `src/build_system/create_project_modules.rs`

Stage 0 builds the module inputs consumed by the frontend. It:

* loads project config constants into `Config`
* discovers module roots from build-system entry files
* expands each module to reachable `.bst` files through imports
* detects source-library roots visible to imports
* recognizes external package prefixes so virtual imports are not treated as filesystem paths
* records source file identities for later diagnostics and path rendering

Stage 0 is build-system-owned input preparation, not semantic frontend compilation.

Detailed `#config.bst`, module-root, `#page.bst`, and `#mod.bst` user rules belong in `docs/language-overview.md`.

## Stage 1: Tokenization

Path: `src/compiler_frontend/tokenizer/lexer.rs`

Tokenization converts source text into structured tokens with source locations. It owns:

* basic lexical recognition
* source location tracking
* string and template delimiter context
* style directive token recognition through the merged directive registry
* syntax-level rejection of unsupported or unknown directive forms where applicable

## Stage 2: Header Parsing

Path: `src/compiler_frontend/headers/parse_file_headers.rs`

Header parsing is the only stage that discovers module-wide top-level declarations.
It parses top-level declaration shells so later stages do not reconstruct them from raw tokens. It owns:

- import and re-export parsing
- import path validation and normalization
- file-local import/visibility environment construction
- declaration shell parsing for constants/functions/structs/choices/type aliases
- top-level dependency edge generation
- start-body token separation
- top-level const fragment placement metadata

Header dependency edges include every top-level declaration dependency needed before AST can resolve declarations linearly:
- imported declaration references
- type alias targets
- struct and choice field type annotations
- function parameter and return type annotations
- constant explicit type annotations
- constant initializer references to other constants
- top-level const-template references where structurally detectable

Header parsing does not type-check executable bodies or fold expressions. It should prefer storing normalized, validated path/reference forms instead of raw import/path syntax where enough context exists for later stages to consume.

Header parsing/import preparation builds the file-local import environment used by dependency sorting and AST. It validates and normalizes source imports, re-exports, external package imports, aliases, prelude/builtin reservations, and collision rules where they can be checked structurally.

Constants are compile-time declarations. Header parsing records symbol-shaped references found in constant initializer tokens and resolves them far enough to create dependency edges to other constants.

Header parsing does not type-check executable bodies.
Function bodies and other executable tokens are captured for AST.

Executable function/start body references do not participate in dependency sorting.
Body-local declarations do not participate in dependency sorting.
The implicit entry start header is always appended last.

### Declaration shells
A declaration shell is a structured top-level header payload, not a fully resolved AST node.

Examples:
- constant shell: name, export flag, explicit type annotation, initializer token span/tokens, initializer reference hints, source order
- function shell: name, generic parameters, parsed signature, body tokens
- struct shell: name, generic parameters, parsed field names/types/default token data where applicable
- choice shell: name, generic parameters, variant names and payload field type shells
- type alias shell: name, generic parameters, target type annotation
- start shell: entry-file executable token body, excluded from dependency sorting

### Header and AST ownership boundary
Header parsing owns top-level discovery and declaration shell parsing.
AST must not rediscover top-level symbols or reconstruct top-level declaration shells from raw tokens.

Header parsing builds `ModuleSymbols`, the order-independent top-level symbol, import, export, builtin, type-alias, and source-file metadata package.
Dependency sorting finalizes the sorted declaration list.
AST consumes that package directly.

## Stage 3: Dependency Sorting

Path: `src/compiler_frontend/module_dependencies.rs`

Dependency sorting operates only on top-level declaration headers and header-provided dependency edges. It owns:

- topological sorting of parsed top-level declaration headers
- cycle detection in the strict top-level declaration graph
- missing header-provided dependency diagnostics
- source-order stability among otherwise independent declarations
- appending the implicit entry `start` header after sorted declarations

It does not use executable function/start body references or body-local declarations. Constant initializer references are not body references, they are top-level compile-time declaration dependencies and belong in the header dependency graph.
Dependency sorting exists only to order top-level declarations before AST construction.

Dependency sorting orders constants using header-provided constant initializer dependency edges. Same-file constants keep source-order semantics. Same-file forward references are rejected. Cross-file constant cycles are dependency cycles.

### Header/dependency/AST contract

Header parsing and dependency sorting are responsible for making top-level declarations linearly consumable by AST.

After dependency sorting:

- AST receives headers in dependency order (it does not topologically sort constants, structs, choices, functions, or aliases again)
- AST must not rediscover top-level declarations from raw file tokens or rebuild file import visibility from scratch
- AST resolves declaration shells in sorted order, then parses executable bodies against the completed environment
- If AST needs a top-level declaration to be resolved before another declaration, that dependency belongs in the header dependency graph
- If a new feature introduces a top-level dependency, add it to header parsing/dependency sorting rather than adding another AST ordering pass
- The implicit entry `start` header is never a dependency participant and is always emitted after sorted declarations

## Stage 4: AST Construction

Path: `src/compiler_frontend/ast/mod.rs`

AST consumes already-sorted declaration headers and the header-built module environment. It resolves declarations in order, folds constants/templates, parses executable bodies, type-checks expressions, and emits typed AST nodes.

Internally, AST construction is organized around three phase owners:

* `build_ast_environment`: consumes header-built file visibility, then resolves declaration metadata, constants, nominal types, function signatures, receiver catalog data, and shared environment side channels
* `emit_ast_nodes`: parses function/start/template bodies against the completed environment and emits AST nodes plus const-template output
* `finalize_ast`: performs HIR-boundary cleanup, including doc fragment extraction, const top-level fragment assembly, module constant normalization, template normalization, type-boundary validation, builtin AST merging, and final `Ast` construction

AST owns:

- type alias, constant, struct field, choice variant, and function signature validation
- expression parsing and type checking
- contextual coercion at declaration, return, and template/string boundaries
- match guard validation and exhaustiveness checks
- body-local declarations in source order
- multi-bind validation for explicit multi-return calls
- receiver-method cataloging
- generic declaration/type validation at the frontend level
- constant folding and const-only validation
- template composition, compile-time folding, helper elimination, and runtime render-plan preparation

AST should be described by this ownership and data-flow contract, not by a fixed internal pass count.
The internal substeps inside each phase are implementation details and may change as the stage is simplified.

### Imports and visibility

AST consumes the header-built file visibility environment through ScopeContext. It may validate semantic use of visible symbols, but it must not rebuild import bindings or rediscover top-level visibility.

All user-visible names go through one collision policy.
Same-file declarations, source imports, external imports, type aliases, prelude symbols, and builtins cannot silently shadow each other.

External expression and type resolution must go through the active `ScopeContext` visibility lookup. If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST pass.

### Type checking and coercion

Expression evaluation determines the natural type of an expression and stays strict.
Contextual coercion is applied only by the frontend site that owns the boundary.

Examples of boundary owners:
- declarations
- returns
- template/string content
- explicit builtin casts
- backend/prelude call contracts

Detailed numeric rules, match syntax, cast syntax, and string coercion rules belong in `docs/language-overview.md`.

### Constants and folding

The AST consumes parsed exported constants directly and type-checks their initializers.
Module constants are compile-time metadata, not runtime top-level declaration statements.

Constants and top-level const templates must fold at compile time.
Runtime expressions in constants are rejected.

Runtime expressions that cannot fold are currently represented in AST as stack-oriented RPN node vectors before HIR lowering.

### Templates

AST owns template semantic preparation.

It:

* composes slots, inserts, wrappers, and child templates
* folds fully constant templates into string literals
* preserves runtime templates as runtime expressions
* removes helper-only template artifacts before HIR
* emits builder-facing const top-level fragment metadata

HIR only lowers finalized runtime templates that remain after AST folding.
Compile-time page fragments stay outside HIR.

## Stage 5: HIR Generation

Path: `src/compiler_frontend/hir/`

HIR generation lowers the fully typed AST into the first backend-facing semantic IR.
HIR is structured enough for borrow/exclusivity analysis: control flow, locals, calls, regions, and terminators are explicit, while ordinary value construction and operators may remain as nested expression trees.

HIR owns:

* explicit control-flow structure
* block, jump, terminator, loop, branch, return, and match representation
* explicit locals and call targets
* lowered runtime template expressions
* module constants as compile-time metadata
* stable external function IDs selected during AST resolution
* enough structure for borrow validation and later backend lowering

HIR does not:

* fold templates
* reconstruct missing template plans
* carry compile-time top-level page fragments
* decide final runtime ownership
* model exact lifetimes

### External calls

Calls to builder-provided package functions lower to stable external call targets such as `CallTarget::ExternalFunction(ExternalFunctionId)`.

HIR does not store package import syntax or backend runtime names.
Borrow validation can resolve those IDs through the external package registry to recover access rules and return-alias metadata.
Backends map the same IDs to target-specific helpers, imports, or runtime names.

### Mutable rvalues

Fresh rvalues passed to mutable (`~T`) call slots are materialized into compiler-introduced hidden locals before borrow validation.
Borrow validation then sees ordinary local access, not a special temporary node kind.

## Stage 6: Borrow Validation

Path: `src/compiler_frontend/analysis/borrow_checker/`

Borrow validation enforces borrow/exclusivity rules and produces side-table facts used by later ownership-aware lowering.
It does not mutate HIR, compute exact lifetimes, or decide final runtime ownership.

GC remains the semantic fallback.
Ownership and deterministic destruction are optimization layers described in `docs/memory-management-design.md`.

Borrow validation is mandatory for backend semantic parity:

* invalid overlapping mutable/shared access is rejected before backend lowering
* use-after-move and invalid access patterns are rejected before backend lowering
* valid programs may expose additional facts for ownership-aware lowerings
* GC-only backends can ignore ownership-specific optimization facts while preserving semantics

Borrow validation does not track per-field or per-projection aliasing yet.
HIR remains the stable semantic representation; borrow facts live in side tables keyed by HIR/value IDs.

The language-level no-shadowing rule supports simpler name and borrow analysis, but the rule itself is specified in `docs/language-overview.md`.

## Backend lowering

Backend lowering belongs to project builders after frontend compilation.

Backends consume compiled modules containing:

* HIR
* borrow-analysis facts
* warnings
* module constants
* entry metadata
* const top-level fragments for builders that need page-fragment merging

Backends own target-specific output generation.
They must preserve Beanstalk semantics even when backend representations differ.

Ownership-aware backends may use borrow facts and memory-model metadata for optimization.
GC-only backends can lower through the semantic baseline without deterministic drop behavior.
