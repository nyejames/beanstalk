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
- `src/compiler_frontend/pipeline.rs` owns the `CompilerFrontend` stage flow: source file preparation → sorted headers → AST → HIR → borrow report. Source file preparation tokenizes and header-parses each file against a worker-local string table before module-wide aggregation.

### Input, paths, diagnostics, and symbols

- `src/compiler_frontend/tokenizer/` converts source text into located tokens and handles string/template delimiter context
- `src/compiler_frontend/compiler_messages/` owns typed diagnostics, labels, source locations, stable diagnostic descriptors, render-boundary message aggregation, and terminal/terse/dev-server renderers. `CompilerDiagnostic` is the user-facing source/config/import/type/rule/borrow diagnostic path. `CompilerError` is reserved for internal compiler, filesystem, backend, and dev-server infrastructure failures. Type diagnostics carry semantic `TypeId`s and render source-level names through `DiagnosticRenderContext` when the relevant module `TypeEnvironment` is available.
- `src/compiler_frontend/symbols/`, `interned_path`, and `paths/` own interned source identities, path formatting/resolution, and canonical symbol identity shared across diagnostics, imports, and lowering

### Declarations, imports, and type surface

- `src/compiler_frontend/headers/` discovers top-level declarations, imports, normalized path/reference shells, declaration shells, constant initializer dependency hints, and start-body separation
- `src/compiler_frontend/module_dependencies.rs` orders top-level declaration headers by header-provided dependency edges, including constant initializer dependencies
- `src/compiler_frontend/declaration_syntax/` owns shared declaration-shell parsing used by headers and body-local AST parsing. It keeps syntactically equivalent declaration shapes on one parser path, but it does not own semantic type resolution.
- `src/compiler_frontend/datatypes/` owns `TypeEnvironment` (canonical semantic type identity) and `DataType` (parse-only / diagnostic-only type syntax). Semantic identity is `TypeId` equality in the relevant `TypeEnvironment`; `DataType` must not be used for semantic decisions in executable AST or HIR.
- `src/compiler_frontend/type_coercion/` owns contextual compatibility and promotion rules layered on top of type identity
- `src/compiler_frontend/value_mode.rs` tracks frontend access classification for bindings, expressions, call arguments, and receiver use. It keeps mutability/reference state separate from `DataType`; runtime ownership is a later borrow/lowering concern
- `src/compiler_frontend/traits/` owns parsed trait shells, resolved trait definitions, explicit conformance evidence, evidence visibility, dynamic-safety classification, and trait diagnostics. Trait metadata is compile-time frontend state, not a backend-side source rediscovery path
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
    ) -> Result<(), ProjectConfigError>;

    fn frontend_style_directives(&self) -> Vec<StyleDirectiveSpec>;

    fn libraries(&self) -> LibrarySet;
}

pub struct ProjectBuilder {
    pub backend: Box<dyn BackendBuilder + Send>,
}

pub struct LibrarySet {
    pub external_packages: ExternalPackageRegistry,
    pub source_libraries: SourceLibraryRegistry,
    pub config_keys: ProjectConfigKeyRegistry,
    pub external_import_providers: ExternalImportProviderRegistry,
    pub external_import_cache: ExternalImportProviderCache,
    pub external_import_resolution_table: ExternalImportResolutionTable,
    pub builder_runtime_packages: Vec<BuilderRuntimePackageMetadata>,
}
```

Backend builders do not parse source files, discover modules, read project config directly, or perform semantic frontend compilation.
They declare frontend-visible libraries/directives/config keys/external import providers, builder-runtime packages, validate config, and lower compiled modules into artifacts. `ProjectConfigKeyRegistry` is declarative Stage 0 metadata: it lists allowed core and backend-owned keys plus value shapes so `#config.bst` can reject unknown declarations, shape-invalid folded values, and closed-domain string values before core fields are applied or backend settings are stored. Config extraction consumes shared AST const facts for declarations authored in `#config.bst`; imported core/builder constants and types are support surface, not config entries. External import providers are builder-declared hooks that Stage 0/import preparation uses to turn non-Beanstalk files into typed external package surfaces before AST; for JS-backed providers, that provider result also carries registered runtime imports discovered while parsing the external source. Builder-runtime package metadata covers builder-owned JS-backed virtual packages such as `@web/canvas`; these packages are registered directly in `external_packages`, then attached to module external-import metadata only when entry-reachable HIR references one of their functions. Project-specific config validation remains in `BackendBuilder::validate_project_config`, which returns `ProjectConfigError` so normal user config mistakes stay as typed `CompilerDiagnostic` values while infrastructure failures remain explicit `CompilerError` values.

Complex release optimizations should remain outside the fast frontend path unless they are required for correctness.

### Diagnostic and path identity contract

A build lifecycle uses a `StringTable` across config loading, frontend compilation, backend validation/build, and diagnostic rendering.

- `SourceLocation` stores interned path/scope identity, not owned diagnostic paths

- Rendering and filesystem-adjacent code resolve interned paths through the `StringTable`

- Boundary types such as `BuildResult` and failed `CompilerMessages` carry the string table so later output writing, terminal rendering, and dev-server reporting can resolve paths consistently

- Directory project compilation creates one shared string-table fork source for the module batch. Each module compiles with a local delta over that immutable base, then the build system merges local suffixes in deterministic entry-path order and remaps module payloads, diagnostics, and render type environments only when the returned remap is non-identity.

- Inside a module, source files are prepared in parallel. Each file tokenizes and header-parses against a local string-table fork, then the module frontend merges those file deltas back into the module table in deterministic input order and remaps tokens, headers, warnings, and diagnostics before module-wide header aggregation, dependency sorting, and AST construction.

- Full `StringTable` cloning and full-table merging remain available for true independent table boundaries. They should not be used for ordinary parallel module compilation.

### Style directive contract

Project builders can register style directives through `frontend_style_directives`.

- Frontend-owned directives are always available
- Builder directives cannot override frontend-owned names
- Tokenizer and template parsing use the same merged registry
- Unknown directives are rejected strictly

Individual directive syntax and behavior belong in `docs/language-overview.md`.

### Type identity contract

The frontend owns a single `TypeEnvironment` per module. It is the canonical source of semantic type identity.

- `TypeId` equality in the active `TypeEnvironment` is the only valid way to compare types for semantic decisions
- `DataType` is parse-only / diagnostic-only. It must not be used for semantic decisions in executable AST or HIR
- Type diagnostics should carry canonical `TypeId`s plus context enums. They should not store rendered type names or cloned `DataType` payloads for display.
- Diagnostic renderers resolve type names at the render boundary through `DiagnosticRenderContext`, which borrows the `StringTable` and optionally the module `TypeEnvironment`.
- `TypeEnvironment` is built during AST environment construction and populated with builtins, nominal structs, choices, and generic instances before AST body emission begins. Early nominal registration records identity and generic parameter metadata only; canonical field and variant members are written after AST-owned constructor shells are resolved to semantic `TypeId`s.
- `TypeEnvironment` member queries expose borrowed field/variant views and direct member lookup helpers. AST, HIR, and backend-facing lowering should use those semantic views instead of cloning member lists for lookup.
- AST body emission receives `AstTypeInterner`, a narrow façade over `TypeEnvironment` that allows derived type interning (tuples, function types) and module-local compatibility caching without permitting nominal declaration mutation
- Function signatures store canonical `TypeId`s on `ReturnSlot` and `Declaration` after resolution. The parallel `DataType` vectors remain available for diagnostics only
- HIR `HirStruct` and `HirChoice` carry `frontend_type_id` to trace lowering-local layouts back to the canonical `TypeEnvironment` entry. Validation asserts these IDs resolve to real type definitions
- External package types that have no frontend mapping use `ExpectedParameterType::UnknownExternal` instead of sentinel `TypeId`s. Call validation skips type compatibility for unknown external parameters

### Import, library, and external package contract
Stage 0 discovers source libraries and provider-backed external files as normal build inputs.

Header parsing/import preparation resolves imports, aliases, facade boundaries, namespace/import records, receiver-method visibility, external package symbols, prelude symbols, builtins, and file-local visibility. It produces the visibility environment consumed by dependency sorting and AST.

Dependency sorting uses header-provided dependency edges.

AST consumes file-local visibility through `ScopeContext`. It validates semantic use of visible symbols, but it does not rebuild import bindings or rediscover import visibility.

Compiler-facing rules:

- Source libraries are normal modules behind `#mod.bst` facades and participate in module-level dependency sorting
- External packages are virtual typed symbols provided by backend metadata, not `.bst` source files
- External package membership uses stable `ExternalPackageId` values plus readable package paths and origin metadata, so built-in and provider-created packages share one identity model
- External import providers live under `LibrarySet` and resolve non-Beanstalk import sources into typed package/type/function/method IDs before AST consumes visibility
- Builder-runtime package metadata lets builder-owned packages share the same backend runtime asset/glue emission path as provider-created imports without pretending they were project-local files
- External imports resolve to stable frontend IDs such as `ExternalFunctionId`
- Grouped imports for virtual external packages resolve through external package metadata before source/module facade enforcement. Source imports still go through facade checks before source target resolution, so virtual package lookup does not weaken source-library or module privacy.
- Explicit grouped receiver-method imports reserve their local spelling in the header-stage visible-name registry before adding receiver-call visibility, so aliases collide consistently with same-file declarations, imports, prelude symbols, builtins, and namespace records. Auto-imported receiver methods from type imports and namespace imports stay receiver-call-only and do not create ordinary visible value fields.
- Grouped receiver-method imports require the receiver type to be visible in the importing file. Header import preparation validates source nominal types, external opaque type IDs, namespace records, grouped type imports, and language-visible scalar receivers before AST consumes the receiver-call visibility; AST retains the exact transparent source type-alias guard because type-alias targets are resolved there.
- Expression/type resolution uses the active `ScopeContext` visibility maps and import records, not global bare-name lookup
- HIR carries stable external call IDs only; backends map those IDs to target-specific runtime names, emitted JS assets, generated glue, imports, or helper calls

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
site_name #= "Beanstalk"

head_defaults #= [:
  <meta charset="UTF-8">
]

UserId as Int

Card = |
    title String,
|

render_card |title String| -> String:
    return [: <article>[title]</article>]
;
```

Top-level constants, type aliases, structs, choices, function signatures, and relevant type annotations can create header-provided dependency edges.
Executable body references do not.

Binding-mode syntax, constant rules, facade visibility, and top-level template syntax are specified in `docs/language-overview.md`.

## Pipeline stages

The compiler frontend and build system process modules through these stages:

0. **Project Structure**: discovers config, module roots, reachable source files, source libraries, and external package namespaces
1. **Tokenization**: converts source text to located tokens. In project builds this runs per file against worker-local string tables.
2. **Header Parsing**: parses imports, declaration shells, top-level dependency edges, constant initializer reference edges, and captures entry start body separately. In project builds this is fused with tokenization as per-file preparation before deterministic string-table merge/remap and module-wide aggregation.
3. **Dependency Sorting**: orders top-level declaration headers by all header-provided top-level dependency edges
4. **AST Construction**: consumes sorted headers linearly, resolves and validates semantic information, parses executable bodies, type-checks expressions, and prepares templates/constants for HIR/builders
5. **HIR Generation**: lowers the typed AST into backend-facing semantic IR with explicit control flow
6. **Borrow Validation**: validates borrow/exclusivity rules and produces side-table facts for later lowering
7. **Backend Lowering**: project builders lower compiled modules into backend-specific artifacts

## Stage 0: Project Structure

Path: `src/build_system/create_project_modules/`

Stage 0 builds the module inputs consumed by the frontend. It:

- compiles `#config.bst` and reachable core/builder source-library support files through the frontend up to AST, then extracts folded immutable known-key declarations authored in `#config.bst` into `Config` from shared AST const facts after enforcing each key's registered value shape
- allows config imports only from core/builder libraries and keeps project-local config imports rejected by design
- stops config compilation at AST; config does not need HIR
- discovers module roots from build-system entry files
- expands each module to reachable `.bst` files through imports
- detects source-library roots visible to imports
- recognizes external package prefixes so virtual imports are not treated as filesystem paths
- resolves provider-backed external file imports before AST and stores their typed package metadata in the external import resolution table
- rejects sibling `.bst` file/folder import-name collisions and special-file imports before semantic compilation
- records source file identities for later diagnostics and path rendering

Stage 0 is build-system-owned input preparation, not semantic frontend compilation.
Private inferred const facts are collected after dependency sorting and AST construction; they do not participate in header dependency sorting and do not become importable declarations.

For directory builds, Stage 0 also owns the build-boundary string-table fork/merge lifecycle: module frontend jobs receive local string-table deltas, and the build system merges those deltas back into the shared build table after compilation has completed. The module frontend owns its own internal per-file fork/merge lifecycle before dependency sorting.

Detailed `#config.bst`, module-root, `#page.bst`, and `#mod.bst` user rules belong in `docs/language-overview.md`.

## Stage 1: Tokenization

Path: `src/compiler_frontend/tokenizer/lexer.rs`

Tokenization converts source text into structured tokens with source locations. It owns:

- basic lexical recognition
- source location tracking
- string and template delimiter context
- style directive token recognition through the merged directive registry
- syntax-level rejection of unsupported or unknown directive forms where applicable

## Stage 2: Header Parsing

Path: `src/compiler_frontend/headers/parse_file_headers.rs`

Header parsing is the only stage that discovers module-wide top-level declarations.
It parses top-level declaration shells so later stages do not reconstruct them from raw tokens. It owns:

- import and re-export parsing
- import path validation and normalization
- file-local import/visibility environment construction
- declaration shell parsing for constants/functions/structs/choices/type aliases/traits/conformances
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
Declaration-shell parsers are shared with AST body-local declaration parsing so top-level and body-local declaration syntax stays equivalent. Header parsing records parsed type-reference shells and dependency edges; AST owns resolving those shells into canonical `TypeId`s.

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
- type alias shell: name, target type annotation. Parameterized generic aliases are rejected before shell creation.
- trait shell: name, requirement signature shells, and requirement type-reference dependency edges
- conformance shell: target type reference, trait references, and declaration source context
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

### Facades and source libraries in dependency sorting

Module-root facades (`#mod.bst` files that belong to a project module root) participate in dependency sorting like other top-level declaration providers. They remain boundary export layers for visibility, but exported constants and type surfaces must still be ordered before declarations in outside modules that import them through the facade. Other files inside the same module should not depend on symbols declared directly inside the module-root facade; header import visibility, not dependency sorting, enforces that boundary.

Source-library facades (`#mod.bst` files provided by builder source libraries such as `@html`) also participate in dependency sorting. Their declarations are first-class providers to the consuming module, not opaque boundaries. Because source libraries have no outgoing dependency edges to project files, the topological sort naturally places them before project files that import them.

### Header/dependency/AST contract

Header parsing and dependency sorting are responsible for making top-level declarations linearly consumable by AST.
Declaration-shell parsing is intentionally shared through `declaration_syntax`: headers parse the top-level shells needed for dependency discovery and declaration registration, while AST uses the same shell shapes for body-local declarations and owns all semantic resolution from those shells.

After dependency sorting:

- AST receives headers in dependency order (it does not topologically sort constants, structs, choices, functions, or aliases again)
- AST must not rediscover top-level declarations from raw file tokens or rebuild file import visibility from scratch
- AST resolves declaration shells in sorted order, then parses executable bodies against the completed environment
- AST may register nominal identity and generic parameter metadata before constants so constructors are name-resolvable, but unresolved field and variant constructor shells stay in AST-owned side tables until semantic `TypeId`s are checked and final member definitions are written to `TypeEnvironment`
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

- semantic type resolution from parsed declaration/type shells to canonical `TypeId`s
- type alias, constant, struct field, choice variant, and function signature validation
- expression parsing and type checking
- contextual coercion at declaration, return, and template/string boundaries
- match guard validation and exhaustiveness checks
- body-local declarations in source order
- multi-bind validation for explicit multi-return calls
- receiver-method cataloging
- generic declaration/type validation at the frontend level
- generic free-function template storage, body validation, immediate local call inference, and concrete instance emission before HIR
- trait declaration resolution, trait visibility, conformance evidence validation, static generic-bound evidence checks, evidence-backed receiver fallback, dynamic-safety checks, and dynamic trait coercion insertion
- constant folding and const-only validation
- template composition, compile-time folding, control-flow validation, helper elimination, and runtime render-plan preparation

AST should be described by this ownership and data-flow contract, not by a fixed internal pass count.
The internal substeps inside each phase are implementation details and may change as the stage is simplified.

### Generics contract

Generics are resolved before HIR. Header parsing records declaration-site
generic parameter metadata on declaration shells, but AST owns semantic
registration, validation, inference, and concrete instance emission.

- Header parsing records generic parameter lists and declaration metadata; it does not infer or substitute generic types
- AST registers generic parameter lists in `TypeEnvironment` and resolves generic signatures to canonical `TypeId`s
- AST stores generic free-function templates and validates generic bodies before concrete calls are emitted
- AST infers generic function calls from immediate argument evidence and immediate expected result context only
- AST emits concrete generic function instances before HIR generation
- HIR must never carry unresolved generic executable types or unsolved generic function calls
- Borrow validation receives concrete HIR and does not consume generic template state
- Backends never solve generic type arguments or generic function instances

### Traits contract

Trait declarations and conformances are resolved before HIR. Header parsing records trait and
conformance shells; AST owns semantic trait identity, requirement type resolution, conformance
evidence validation, evidence visibility, dynamic-safety classification, and generic-bound
evidence checks.

- Traits are compile-time metadata in `TraitEnvironment`, not `DataType` values
- Dynamic trait value annotations become semantic `TypeId`s in `TypeEnvironment`
- Explicit conformance evidence lives in `TraitEvidenceEnvironment` with stable evidence IDs and requirement-to-method mappings
- Static generic bounds use visible reusable evidence during generic function calls and concrete generic nominal instantiation
- Dynamic coercion decisions are inserted by AST at explicit typed boundaries, then lowered as explicit HIR operations
- Backends must consume selected trait/evidence IDs from HIR and must not resolve trait declarations or scan source headers for method shapes

### Imports and visibility

AST consumes the header-built file visibility environment through ScopeContext. It may validate semantic use of visible symbols, but it must not rebuild import bindings or rediscover top-level visibility.

All user-visible names go through one collision policy.
Same-file declarations, source imports, external imports, type aliases, prelude symbols, and builtins cannot silently shadow each other.

External expression and type resolution must go through the active `ScopeContext` visibility lookup. If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST pass.

### Type checking and coercion

Expression evaluation determines the natural type of an expression and stays strict.
Contextual coercion is applied only by the frontend site that owns the boundary.

AST emission should carry canonical `TypeId`s through field access, receiver lookup, builtin receiver validation, call validation, operator result typing, and compatibility checks. `DataType` remains parse-only or diagnostic spelling once a semantic `TypeId` exists.

Examples of boundary owners:
- declarations
- returns
- template/string content
- explicit builtin casts
- backend/prelude call contracts

Detailed numeric rules, match syntax, cast syntax, and string coercion rules belong in `docs/language-overview.md`.

### Constants and folding

The AST consumes parsed compile-time constants directly and type-checks their initializers.
Module constants are compile-time metadata, not runtime top-level declaration statements.

Constants and top-level const templates must fold at compile time.
Runtime expressions in constants are rejected.

Runtime expressions that cannot fold are currently represented in AST as stack-oriented RPN node vectors before HIR lowering.

### Templates

AST owns template semantic preparation.

It:

* composes slots, inserts, wrappers, and child templates
* folds fully constant templates into string literals
* preserves structured template `if` / `loop` bodies for runtime lazy lowering
* prepares runtime slot source/site plans after AST-owned schema extraction and contribution routing
* validates const-required template control flow before HIR
* rejects escaped slot/insert helper artifacts that are invalid after composition/routing
* preserves runtime templates as runtime expressions
* removes helper-only template artifacts before HIR
* emits builder-facing const top-level fragment metadata

HIR only lowers finalized runtime templates that remain after AST folding. Runtime
template control flow lowers inline as ordinary HIR branches, loops, accumulator
appends, and AST-prepared runtime slot source/site plans in the enclosing
function, not as backend-specific template control-flow nodes. HIR consumes
AST-prepared slot source/site plans only; it does not parse directives or
validate slot schemas.
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
* inline runtime template control flow as ordinary CFG
* runtime slot source/site plans lowered as ordinary string accumulators and appends
* module constants as compile-time metadata
* advisory private const-fact metadata projected from AST for future optimization consumers
* stable external function IDs selected during AST resolution
* resolved trait declarations and conformance evidence projected across the backend boundary
* explicit dynamic trait construction and dynamic trait method-dispatch operations
* backend-neutral syntactic reachability over functions, blocks, external call IDs, and dynamic trait runtime operations from explicit roots
* enough structure for borrow validation and later backend lowering

HIR does not:

* fold templates
* reconstruct missing template plans
* carry backend-specific template control-flow nodes
* carry compile-time top-level page fragments
* use private const facts to change semantics in this plan
* solve generic functions or carry unresolved generic parameter executable types
* decide trait conformance, generic-bound evidence, or dynamic coercion
* decide final runtime ownership
* model exact lifetimes

### Dynamic trait operations

Dynamic trait value construction lowers to explicit HIR wrapper-construction expressions carrying
the target trait ID and selected evidence ID. Dynamic trait method calls lower to explicit HIR
statements carrying the erased trait ID, requirement ID, receiver expression, argument expressions,
and access effects.

HIR reachability records reachable dynamic trait runtime operations alongside reachable functions
and external calls. JS lowering uses those facts to emit only reachable dynamic method tables and
the concrete method functions they reference. HTML-Wasm uses the same reachable facts to reject
reachable dynamic construction or dispatch with structured unsupported-backend diagnostics before
Wasm lowering. Unreachable dynamic-only helper functions remain valid typed HIR and do not block
backend builds.

### External calls

Calls to builder-provided package functions lower to stable external call targets such as `CallTarget::ExternalFunction(ExternalFunctionId)`.

HIR does not store package import syntax or backend runtime names.
Borrow validation can resolve those IDs through the external package registry to recover access rules and return-alias metadata.
Backends map the same IDs to target-specific helpers, imports, or runtime names.
The HTML builder uses HIR reachability from the entry `start` function for runtime artifact planning and unsupported-backend validation. This is syntactic CFG/function reachability, not constant-condition dead-code elimination, optimization, or ownership analysis.

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

## Stage 7: Backend lowering

Backend lowering belongs to project builders after frontend compilation.

Backends consume compiled modules containing:

* HIR
* borrow-analysis facts
* warnings
* module constants
* entry metadata
* const top-level fragments for builders that need page-fragment merging
* module external-import metadata for provider-created and builder-runtime external packages

Backends own target-specific output generation.
They must preserve Beanstalk semantics even when backend representations differ.
For the HTML JS path, backend lowering owns emitted JS assets, registered runtime modules, generated external-call glue, import-map HTML, and the final mapping from stable external function IDs to runtime wrapper names. Runtime module emission and import-map entries are driven by module external-import metadata recorded from accepted JS runtime imports, not by function fallibility.
HTML JS page bundles emit the function set reachable from entry `start`; this keeps unused source-library wrappers from requesting glue or runtime assets. Reachable dynamic trait wrapper evidence can add concrete trait method implementations to that emitted JS function set without re-solving trait semantics in the backend. HTML-Wasm uses the same entry/export reachability for companion JS, Wasm function lowering, and backend feature validation, while reachable unsupported JS-backed external calls and reachable dynamic trait runtime operations fail with structured diagnostics before lowering.

Ownership-aware backends may use borrow facts and memory-model metadata for optimization.
GC-only backends can lower through the semantic baseline without deterministic drop behavior.
