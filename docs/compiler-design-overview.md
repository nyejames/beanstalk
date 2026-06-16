# Beanstalk Compiler Design Overview

Beanstalk is a high-level language with first-class string templates. The compiler is modular, exposed as a library, and used by the built-in project tooling, dev server, and backend builders.

This document is the compiler architecture guide. It describes stage ownership, data flow, cross-stage contracts, and where important systems live. It is forward-looking, but it should stay anchored in the current implementation and clearly identify intended design where the compiler has drifted.

Use these related documents for adjacent concerns:

- [`language-overview.md`](language-overview.md) for compiler-facing language facts, syntax shape, semantic invariants, and deferred language surface
- [`memory-management-design.md`](memory-management-design.md) for GC fallback, ownership, borrow analysis strategy, and lowering implications
- [`codebase-style-guide.md`](codebase-style-guide.md) for implementation standards
- [`roadmap/roadmap.md`](roadmap/roadmap.md) for planning
- [`src/docs/progress/#page.bst`](src/docs/progress/#page.bst) for implementation status, backend coverage, and feature progress

User-facing docs-site pages contain examples and beginner-oriented explanations:

- [`src/docs/project-structure/#page.bst`](src/docs/project-structure/#page.bst) for projects, config, modules, entries, and output folders
- [`src/docs/libraries/#page.bst`](src/docs/libraries/#page.bst) for imports, libraries, source libraries, external packages, and JavaScript imports
- [`src/docs/libraries/core/#page.bst`](src/docs/libraries/core/#page.bst) for core library pages
- [`src/docs/templates/#page.bst`](src/docs/templates/#page.bst) for template syntax, directives, markdown behavior, and slots
- [`src/docs/beandown/#page.bst`](src/docs/beandown/#page.bst) for Beandown authoring and import rules

Build systems use the compiler through HIR and borrow validation, then apply their own backend lowering. They assemble one or more compiled modules into runnable artifacts such as HTML, JavaScript, Wasm, or other target outputs.

## Frontend structure at a glance

### Code navigation map

- Project/build entry:
  [`src/main.rs`](../src/main.rs),
  [`src/projects/`](../src/projects/),
  [`src/build_system/`](../src/build_system/),
  and [`src/libraries/`](../src/libraries/).
- Frontend driver and early stages:
  [`src/compiler_frontend/mod.rs`](../src/compiler_frontend/mod.rs),
  [`pipeline.rs`](../src/compiler_frontend/pipeline.rs),
  [`tokenizer/`](../src/compiler_frontend/tokenizer/),
  [`headers/`](../src/compiler_frontend/headers/),
  [`declaration_syntax/`](../src/compiler_frontend/declaration_syntax/),
  and [`module_dependencies.rs`](../src/compiler_frontend/module_dependencies.rs).
- Frontend shared surfaces:
  [`compiler_messages/`](../src/compiler_frontend/compiler_messages/),
  [`symbols/`](../src/compiler_frontend/symbols/),
  [`paths/`](../src/compiler_frontend/paths/),
  [`datatypes/`](../src/compiler_frontend/datatypes/),
  [`type_coercion/`](../src/compiler_frontend/type_coercion/),
  [`traits/`](../src/compiler_frontend/traits/),
  [`builtins/`](../src/compiler_frontend/builtins/),
  [`external_packages/`](../src/compiler_frontend/external_packages/),
  [`source_libraries/`](../src/compiler_frontend/source_libraries/),
  and [`style_directives/`](../src/compiler_frontend/style_directives/).
- AST owners:
  [`ast/mod.rs`](../src/compiler_frontend/ast/mod.rs),
  [`module_ast/environment/`](../src/compiler_frontend/ast/module_ast/environment/),
  [`module_ast/emission/`](../src/compiler_frontend/ast/module_ast/emission/),
  [`module_ast/finalization/`](../src/compiler_frontend/ast/module_ast/finalization/),
  [`type_resolution/`](../src/compiler_frontend/ast/type_resolution/),
  [`expressions/`](../src/compiler_frontend/ast/expressions/),
  [`statements/`](../src/compiler_frontend/ast/statements/),
  [`templates/`](../src/compiler_frontend/ast/templates/),
  and [`generic_functions/`](../src/compiler_frontend/ast/generic_functions/).
- HIR, analysis, and backend validation:
  [`hir/`](../src/compiler_frontend/hir/),
  [`hir/reachability.rs`](../src/compiler_frontend/hir/reachability.rs),
  [`analysis/borrow_checker/`](../src/compiler_frontend/analysis/borrow_checker/),
  [`src/backends/backend_feature_validation.rs`](../src/backends/backend_feature_validation.rs),
  and [`src/backends/external_package_validation.rs`](../src/backends/external_package_validation.rs).
- Backend/project lowerings:
  [`src/backends/js/`](../src/backends/js/),
  [`src/backends/wasm/`](../src/backends/wasm/),
  [`src/projects/html_project/`](../src/projects/html_project/),
  and [`src/projects/html_project/wasm/`](../src/projects/html_project/wasm/).
- Tests and tooling:
  [`src/compiler_tests/`](../src/compiler_tests/),
  [`tests/cases/`](../tests/cases/),
  [`xtask/`](../xtask/),
  and [`benchmarks/`](../benchmarks/).

### Stage orchestration

- [`src/compiler_frontend/mod.rs`](../src/compiler_frontend/mod.rs) is the frontend module map.
- [`src/compiler_frontend/pipeline.rs`](../src/compiler_frontend/pipeline.rs) owns the `CompilerFrontend` stage flow: source file preparation → sorted headers → AST → HIR → borrow report. Source file preparation tokenizes and header-parses each file against a worker-local string table before module-wide aggregation.

### Input, paths, diagnostics, and symbols

- [`src/compiler_frontend/tokenizer/`](../src/compiler_frontend/tokenizer/) converts source text into located tokens and handles string/template delimiter context.
- [`src/compiler_frontend/compiler_messages/`](../src/compiler_frontend/compiler_messages/) owns typed diagnostics, labels, source locations, stable diagnostic descriptors, render-boundary message aggregation, and terminal/terse/dev-server renderers. `CompilerDiagnostic` is the user-facing source/config/import/type/rule/borrow diagnostic path. `CompilerError` is reserved for internal compiler, filesystem, backend, and dev-server infrastructure failures. Type diagnostics carry semantic `TypeId`s and render source-level names through `DiagnosticRenderContext` when the relevant module `TypeEnvironment` is available.
- [`src/compiler_frontend/symbols/`](../src/compiler_frontend/symbols/), [`symbols/interned_path.rs`](../src/compiler_frontend/symbols/interned_path.rs), and [`src/compiler_frontend/paths/`](../src/compiler_frontend/paths/) own interned source identities, path formatting/resolution, and canonical symbol identity shared across diagnostics, imports, and lowering.

### Declarations, imports, and type surface

- [`src/compiler_frontend/headers/`](../src/compiler_frontend/headers/) discovers top-level declarations, imports, normalized path/reference shells, declaration shells, constant initializer dependency hints, and start-body separation.
- [`src/compiler_frontend/module_dependencies.rs`](../src/compiler_frontend/module_dependencies.rs) orders top-level declaration headers by header-provided dependency edges.
- [`src/compiler_frontend/declaration_syntax/`](../src/compiler_frontend/declaration_syntax/) owns shared declaration-shell parsing used by headers and body-local AST parsing. It keeps syntactically equivalent declaration shapes on one parser path, but it does not own semantic type resolution.
- [`src/compiler_frontend/datatypes/`](../src/compiler_frontend/datatypes/) owns `TypeEnvironment` as canonical semantic type identity and `DataType` as parse-only or diagnostic-only type syntax. Semantic identity is `TypeId` equality in the relevant `TypeEnvironment`. `DataType` must not be used for semantic decisions in executable AST or HIR.
- [`src/compiler_frontend/type_coercion/`](../src/compiler_frontend/type_coercion/) owns implicit contextual compatibility and promotion rules layered on top of type identity. Explicit `cast` resolution is AST-owned and uses compiler-owned cast policy/evidence metadata instead of the coercion path.
- [`src/compiler_frontend/value_mode.rs`](../src/compiler_frontend/value_mode.rs) tracks frontend access classification for bindings, expressions, call arguments, and receiver use. It keeps mutability/reference state separate from `DataType`. Runtime ownership is a later borrow/lowering concern.
- [`src/compiler_frontend/traits/`](../src/compiler_frontend/traits/) owns parsed trait shells, resolved trait definitions, explicit same-file nominal conformance evidence, reusable evidence visibility, static generic-bound evidence checks, and trait diagnostics. Trait metadata is compile-time frontend state, not a value type or backend-side source rediscovery path.
- [`src/compiler_frontend/source_libraries/`](../src/compiler_frontend/source_libraries/) owns shared facade-file identity and import-surface helpers used across Stage 0, header import preparation, dependency sorting, and AST visibility checks. Source-library root discovery and project-local library scanning are Stage 0 build-system responsibilities.
- [`src/compiler_frontend/external_packages/`](../src/compiler_frontend/external_packages/) stores backend-provided virtual package metadata, package-local symbol paths, and stable external symbol IDs. External package symbols are resolved by package path plus symbol path. The prelude `io` namespace alias is the only bare-name external namespace exception.
- [`src/compiler_frontend/builtins/`](../src/compiler_frontend/builtins/) owns compiler-defined language symbols and operations that are neither user source declarations nor backend-provided external packages, including builtin cast target classification, policy metadata, runtime error codes, and core cast trait definitions/evidence.
- [`src/compiler_frontend/style_directives/`](../src/compiler_frontend/style_directives/) owns the merged frontend and builder directive registry used by tokenizer and template parsing.
- Design-scope and deferred-feature diagnostics should be centralized through typed `CompilerDiagnostic` constructors. Deferred features and outside-design-scope rejections must remain distinct diagnostic reasons.

### Semantic lowering and analysis

- [`src/compiler_frontend/ast/`](../src/compiler_frontend/ast/) builds the typed AST from sorted headers, resolves semantic information, parses executable bodies, folds constants/templates, validates function terminality, and prepares HIR input.
- [`src/compiler_frontend/ast/const_eval/`](../src/compiler_frontend/ast/const_eval/) owns AST compile-time evaluation for constants and foldable template expressions.
- [`src/compiler_frontend/hir/`](../src/compiler_frontend/hir/) lowers the typed AST into the first backend-facing semantic IR.
- [`src/compiler_frontend/analysis/borrow_checker/`](../src/compiler_frontend/analysis/borrow_checker/) validates borrow/exclusivity rules and produces side-table facts for later lowering.

## Build-system and frontend boundary

Build systems provide a `BackendBuilder` implementation and wrap it in a `ProjectBuilder`. The frontend compiles modules up to HIR and borrow validation. The backend builder consumes those compiled modules and emits project artifacts.

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
    pub source_file_kinds: SourceFileKindRegistry,
}
```

Backend builders do not load source files, discover modules, load or parse `#config.bst`, or perform semantic frontend compilation.

They declare the frontend-visible build surface:

- source libraries
- external packages
- config keys
- style directives
- external import providers
- builder runtime packages
- supported source file kinds

They receive a validated `Config`, may validate or interpret backend-owned config keys, and lower compiled `Module` values into artifacts.

`ProjectConfigKeyRegistry` is declarative Stage 0 metadata. It lists allowed core and backend-owned keys plus value shapes so `#config.bst` can reject unknown declarations, shape-invalid folded values, and closed-domain string values before core fields are applied or backend settings are stored.

`#config.bst` is a build-system-owned compile-time Beanstalk source file. It is parsed through the frontend up to AST so config values can use normal compile-time constants, folded templates, core/builder support imports, and typed diagnostics. It is not compiled as a module, does not produce HIR, does not create runtime `start` semantics, and does not export language-visible declarations.

Authored config entries must be known top-level `#` constants. Plain top-level bindings are runtime Beanstalk syntax and are rejected in config. Imported constants and support types may participate in config expressions, but imported declarations are support surface only and never become config entries. Authored config may contain imports, type aliases, structs, and choices as support declarations. Traits, trait conformances, trait incompatibility declarations, functions, mutable bindings, runtime statements, local helper constants, standalone templates, project-local imports, relative imports, and `#[...]` page fragments are rejected.

External import providers are builder-declared hooks that Stage 0/import preparation uses to turn non-Beanstalk files into typed external package surfaces before AST. Provider results may also carry registered runtime imports discovered while parsing the external source.

Builder-runtime package metadata covers builder-owned virtual packages such as `@web/canvas`. These packages are registered directly in `external_packages`, then attached to module external-import metadata when reachable HIR references one of their functions.

Source file kind metadata lets builders opt in to source assets that participate in normal source import discovery without becoming Beanstalk modules. HTML registers Beandown `.bd` files this way.

Project-specific config validation remains in `BackendBuilder::validate_project_config`, which returns `ProjectConfigError` so normal user config mistakes stay as typed `CompilerDiagnostic` values while infrastructure failures remain explicit `CompilerError` values.

Complex release optimizations should remain outside the fast frontend path unless they are required for correctness.

### Diagnostic and path identity contract

A build lifecycle uses a `StringTable` across config loading, frontend compilation, backend validation/build, and diagnostic rendering.

- `SourceLocation` stores interned path/scope identity, not owned diagnostic paths.
- Rendering and filesystem-adjacent code resolve interned paths through the `StringTable`.
- Boundary types such as `BuildResult` and failed `CompilerMessages` carry the string table so later output writing, terminal rendering, and dev-server reporting can resolve paths consistently.
- Directory project compilation creates one shared string-table fork source for the module batch. Each module compiles with a local delta over that immutable base, then the build system merges local suffixes in deterministic entry-path order and remaps module payloads, diagnostics, and render type environments only when the returned remap is non-identity.
- Inside a module, source files are prepared in parallel. Each file tokenizes and header-parses against a local string-table fork, then the module frontend merges those file deltas back into the module table in deterministic input order and remaps tokens, headers, warnings, and diagnostics before module-wide header aggregation, dependency sorting, and AST construction.
- Full `StringTable` cloning and full-table merging remain available for true independent table boundaries. They should not be used for ordinary parallel module compilation.

### Style directive contract

Project builders can register style directives through `frontend_style_directives`.

- Frontend-owned directives are always available.
- Builder directives cannot override frontend-owned names.
- Tokenizer and template parsing use the same merged registry.
- Unknown directives are rejected strictly.

Individual directive syntax and behavior belong in [`src/docs/templates/#page.bst`](src/docs/templates/#page.bst) and [`language-overview.md`](language-overview.md).

### Type identity contract

The frontend owns a single `TypeEnvironment` per module. It is the canonical source of semantic type identity.

- `TypeId` equality in the active `TypeEnvironment` is the only valid way to compare types for semantic decisions.
- `DataType` is parse-only or diagnostic-only. It must not be used for semantic decisions in executable AST or HIR.
- Collection type identity is a canonical `TypeEnvironment` shape. Growable `{T}` and fixed `{N T}` collections are distinct `TypeId`s, and backends recover element type plus optional fixed capacity through collection-shape queries rather than parse syntax or backend side tables.
- Hashmap type identity is a separate canonical `TypeEnvironment` constructed shape. `{K = V}` maps store key and value `TypeId`s directly and are not represented as collection capacity variants or backend side tables.
- Type diagnostics should carry canonical `TypeId`s plus context enums. They should not store rendered type names or cloned `DataType` payloads for display.
- Diagnostic renderers resolve type names at the render boundary through `DiagnosticRenderContext`, which borrows the `StringTable` and optionally the module `TypeEnvironment`.
- `TypeEnvironment` is built during AST environment construction and populated with builtins, nominal structs, choices, and generic instances before AST body emission begins. Early nominal registration records identity and generic parameter metadata only. Canonical field and variant members are written after AST-owned constructor shells are resolved to semantic `TypeId`s.
- `TypeEnvironment` member queries expose borrowed field/variant views and direct member lookup helpers. AST, HIR, and backend-facing lowering should use those semantic views instead of cloning member lists for lookup.
- AST body emission receives `AstTypeInterner`, a narrow facade over `TypeEnvironment` that allows derived type interning and module-local compatibility caching without permitting nominal declaration mutation.
- Function signatures store canonical `TypeId`s on `ReturnSlot` and `Declaration` after resolution. The parallel `DataType` values remain available for diagnostics only.
- HIR `HirStruct` and `HirChoice` carry `frontend_type_id` to trace lowering-local layouts back to the canonical `TypeEnvironment` entry. Validation asserts these IDs resolve to real type definitions.
- External package types that have no frontend mapping use `ExpectedParameterType::UnknownExternal` instead of sentinel `TypeId`s. Call validation skips type compatibility for unknown external parameters.

### Import, library, and external package contract

Stage 0 discovers source libraries, builder-supported source assets, and provider-backed external files as normal build inputs.

Header parsing/import preparation resolves imports, aliases, facade boundaries, explicit `#mod.bst` export metadata, namespace/import records, receiver-method visibility, external package symbols, prelude symbols, builtins, and file-local visibility. It produces the visibility environment consumed by dependency sorting and AST.

Dependency sorting uses header-provided dependency edges.

AST consumes file-local visibility through `ScopeContext`. It validates semantic use of visible symbols, but it does not rebuild import bindings or rediscover import visibility.

Compiler-facing rules:

- Source libraries are normal modules behind explicit `#mod.bst` facades and participate in module-level dependency sorting.
- Builder-supported source file kinds, such as Beandown `.bd`, resolve through the same extensionless source import path as `.bst` files when the active builder declares support. Recognized but unsupported source kinds are rejected with typed import diagnostics.
- Facade public API maps are built from public authored `#mod.bst` headers and public grouped facade imports such as `export import @path { Symbol }` or `export @path { Symbol }`. Ordinary facade imports and unmarked facade declarations remain private to the facade file.
- External packages are virtual typed symbols provided by backend metadata, not `.bst` source files.
- External package membership uses stable `ExternalPackageId` values, readable package paths, structured package-local symbol paths, and origin metadata. Builtin and provider-created packages share one identity model.
- External package namespace imports may expose recursive child namespace records for package-local symbol paths such as `io.input.*`. Source and facade namespace records remain shallow and field-access-only.
- External import providers live under `LibrarySet` and resolve non-Beanstalk import sources into typed package/type/function IDs before AST consumes visibility.
- Builder-runtime package metadata lets builder-owned packages share the same backend runtime asset/glue emission path as provider-created imports without pretending they were project-local files.
- External imports resolve to stable frontend IDs such as `ExternalFunctionId`.
- Grouped imports for virtual external packages resolve through external package metadata before source/module facade enforcement. Source imports still go through facade checks before source target resolution, so virtual package lookup does not weaken source-library or module privacy.
- Header import preparation does not import source-authored receiver methods as independent symbols. Source-authored receiver methods belong to their receiver type's declaring file and become callable wherever the receiver type is visible. Namespace imports may make a receiver type visible, but methods are never namespace fields and cannot be grouped-imported or aliased independently.
- External packages expose opaque types, constants, and free functions only. They do not register receiver methods or receiver-call visibility. Use source-owned wrapper types for method-style ergonomics over external handles.
- Expression/type resolution uses the active `ScopeContext` visibility maps and import records, not global bare-name lookup.
- HIR carries stable external call IDs only. Backends map those IDs to target-specific helpers, imports, generated glue, runtime names, or target-native operations.

User-facing import syntax, facade rules, library categories, and deferred package features are detailed in [`src/docs/libraries/#page.bst`](src/docs/libraries/#page.bst) and [`language-overview.md`](language-overview.md).

### Entry start and page fragments

The module entry file has an implicit `start()` function containing top-level runtime code. Non-entry files contribute declarations only.

Header parsing captures the entry file’s top-level runtime code as a `HeaderKind::StartFunction`. The implicit `start` header is not part of dependency sorting. It is appended after sorted top-level declarations and lowered by AST.

Entry-file page fragments are split:

- Top-level runtime templates remain runtime code inside `start()`.
- `start()` returns runtime fragment strings in source order.
- Entry-file top-level const templates fold in AST into builder-facing compile-time fragments.
- Each compile-time fragment records a runtime insertion index.
- Builders merge compile-time fragments into the runtime fragment list.
- HIR does not carry compile-time page fragments or a separate ordered start-fragment stream.

### Top-level declaration shape

Header parsing owns top-level declaration discovery and declaration shell parsing. These headers participate in strict top-level dependency sorting.

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

Top-level constants, type aliases, structs, choices, function signatures, and relevant type annotations can create header-provided dependency edges. Executable body references do not.

Binding-mode syntax, constant rules, facade visibility, and top-level template syntax are specified in [`language-overview.md`](language-overview.md).

## Pipeline stages

The compiler frontend and build system process modules through these stages:

0. **Project Structure**: discovers config, module roots, reachable source files, builder-supported source assets, source libraries, and external package namespaces.
1. **Tokenization**: converts source text to located tokens. In project builds this runs per file against worker-local string tables and a source-kind-specific tokenizer entry mode.
2. **Header Parsing**: parses imports, declaration shells, top-level dependency edges, fixed-capacity reference edges, constant initializer reference edges, and captures entry start body separately. Source-kind adapters such as Beandown synthesize ordinary declaration headers here. In project builds this is fused with tokenization as per-file preparation before deterministic string-table merge/remap and module-wide aggregation.
3. **Dependency Sorting**: orders top-level declaration headers by all header-provided top-level dependency edges.
4. **AST Construction**: consumes sorted headers linearly, resolves and validates semantic information, parses executable bodies, type-checks expressions, validates terminality, and prepares templates/constants for HIR/builders.
5. **HIR Generation**: lowers the typed AST into backend-facing semantic IR with explicit control flow.
6. **Borrow Validation**: validates borrow/exclusivity rules and produces side-table facts for later lowering.
7. **Backend Lowering**: project builders lower compiled modules into backend-specific artifacts.

## Stage 0: Project Structure

Path: [`src/build_system/create_project_modules/`](../src/build_system/create_project_modules/)

Stage 0 builds the module inputs consumed by the frontend.

Implementation map:

- [`project_config.rs`](../src/build_system/project_config.rs) and [`project_config/`](../src/build_system/project_config/) own `#config.bst` parsing, AST-backed value extraction, and shape validation.
- [`compilation.rs`](../src/build_system/create_project_modules/compilation.rs), [`project_roots.rs`](../src/build_system/create_project_modules/project_roots.rs), and [`frontend_orchestration.rs`](../src/build_system/create_project_modules/frontend_orchestration.rs) own single-file/directory dispatch, root interpretation, path-resolver setup, and per-module frontend orchestration.
- [`entry_discovery.rs`](../src/build_system/create_project_modules/entry_discovery.rs), [`module_inventory.rs`](../src/build_system/create_project_modules/module_inventory.rs), [`reachable_file_discovery.rs`](../src/build_system/create_project_modules/reachable_file_discovery.rs), [`import_scanning.rs`](../src/build_system/create_project_modules/import_scanning.rs), and [`source_loading.rs`](../src/build_system/create_project_modules/source_loading.rs) own entry discovery, import-graph traversal, and source loading.
- [`source_library_discovery.rs`](../src/build_system/create_project_modules/source_library_discovery.rs), [`facade_validation.rs`](../src/build_system/create_project_modules/facade_validation.rs), [`collision_detection.rs`](../src/build_system/create_project_modules/collision_detection.rs), [`project_structure_diagnostics.rs`](../src/build_system/create_project_modules/project_structure_diagnostics.rs), and [`source_discovery_error.rs`](../src/build_system/create_project_modules/source_discovery_error.rs) own source-library scanning, facade preflight, import-name collision checks, typed Stage 0 diagnostics, and diagnostic/infrastructure error boundaries.

Stage 0 owns:

- parsing `#config.bst` and reachable core/builder source-library support files through the frontend up to AST
- extracting only authored known top-level `#` config-key constants from shared AST const facts
- enforcing each config key's registered value shape before applying core fields or storing backend settings
- allowing config imports only from core/builder libraries
- rejecting project-local and relative config imports by design
- stopping config compilation at AST because config does not need HIR
- discovering module roots from build-system entry files
- expanding each module to reachable `.bst` files and builder-supported source assets through imports
- detecting source-library roots visible to imports
- recognizing external package prefixes so virtual imports are not treated as filesystem paths
- resolving source-kind candidates through the builder-provided registry
- resolving provider-backed external file imports before AST and storing typed package metadata in the external import resolution table
- rejecting sibling `.bst` file/folder import-name collisions and special-file imports before semantic compilation
- recording source file identities for later diagnostics and path rendering

Stage 0 is build-system-owned input preparation, not semantic frontend compilation. Private inferred const facts are collected after dependency sorting and AST construction. They do not participate in header dependency sorting and do not become importable declarations.

For directory builds, Stage 0 owns the build-boundary string-table fork/merge lifecycle. Module frontend jobs receive local string-table deltas, and the build system merges those deltas back into the shared build table after compilation has completed. The module frontend owns its own internal per-file fork/merge lifecycle before dependency sorting.

User-facing project layout, config, module root, and output-folder rules are in [`src/docs/project-structure/#page.bst`](src/docs/project-structure/#page.bst).

## Stage 1: Tokenization

Path: [`src/compiler_frontend/tokenizer/lexer.rs`](../src/compiler_frontend/tokenizer/lexer.rs)

Tokenization converts source text into structured tokens with source locations. It owns:

- basic lexical recognition
- source location tracking
- string and template delimiter context
- numeric literal scanning and source-location diagnostics. The tokenizer consumes literal text, classifies attached negative literals, and reports spacing-sensitive syntax errors. [`numeric_text/`](../src/compiler_frontend/numeric_text/) owns shared numeric grammar, normalization, separator/exponent validation, and materialization helpers used by later semantic consumers.
- symbolic binary-operator spacing and unary-negation spacing diagnostics
- style directive token recognition through the merged directive registry
- syntax-level rejection of unsupported or unknown directive forms where applicable

`TokenizerEntryMode` chooses the initial lexical state for a source file kind. Normal `.bst` files start in ordinary code mode. Beandown `.bd` files start inside an implicit template body, which lets the tokenizer preserve original Beandown source locations while rejecting an unescaped outer `]`. `TokenizeMode` remains the internal lexical stack state used while scanning nested templates.

## Stage 2: Header Parsing

Path: [`src/compiler_frontend/headers/parse_file_headers.rs`](../src/compiler_frontend/headers/parse_file_headers.rs)

Header parsing is the only stage that discovers module-wide top-level declarations. It parses top-level declaration shells so later stages do not reconstruct them from raw tokens.

Header parsing owns:

- import and re-export parsing
- facade-only `export` parsing and public/private facade metadata
- import path validation and normalization
- file-local import/visibility environment construction
- declaration shell parsing for constants, functions, structs, choices, type aliases, traits, and conformance metadata
- top-level dependency edge generation
- start-body token separation
- top-level const fragment placement metadata
- source-kind preparation hooks that turn non-`.bst` inputs into ordinary headers

Header dependency edges include every top-level declaration dependency needed before AST can resolve declarations linearly:

- imported declaration references
- type alias targets
- struct and choice field type annotations
- function parameter and return type annotations
- constant explicit type annotations
- fixed collection capacity references in type annotations when the capacity is a visible compile-time constant
- constant initializer references to other constants
- structurally exposed const-template condition/control references when header parsing can identify them without parsing full template body semantics

Header parsing does not type-check executable bodies or fold expressions. It should prefer storing normalized, validated path/reference forms instead of raw import/path syntax where enough context exists for later stages to consume.

Declaration-shell parsers are shared with AST body-local declaration parsing so top-level and body-local declaration syntax stays equivalent. Header parsing records parsed type-reference shells and dependency edges. AST owns resolving those shells into canonical `TypeId`s.

Header parsing/import preparation builds the file-local import environment used by dependency sorting and AST. It validates and normalizes source imports, facade re-exports, external package imports, aliases, prelude/builtin reservations, namespace records, and collision rules where they can be checked structurally.

Constants are compile-time declarations. Header parsing records symbol-shaped references found in constant initializer tokens and resolves them far enough to create dependency edges to other constants.

Executable function/start body references do not participate in dependency sorting. Body-local declarations do not participate in dependency sorting. The implicit entry start header is always appended last.

Beandown header preparation lives in [`headers/beandown_prepare.rs`](../src/compiler_frontend/headers/beandown_prepare.rs). A `.bd` input contributes one private synthetic constant declaration, `content #String`, whose initializer is a structurally built `$markdown` template over the original `.bd` body tokens. Later dependency sorting and AST folding treat that declaration like any other compile-time constant. There is no Beandown-specific HIR path.

User-facing Beandown authoring and import rules are in [`src/docs/beandown/#page.bst`](src/docs/beandown/#page.bst).

### Declaration shells

A declaration shell is a structured top-level header payload, not a fully resolved AST node.

Examples:

- constant shell: name, export flag, explicit type annotation, initializer token span/tokens, initializer reference hints, and source order
- function shell: name, generic parameters, parsed signature, and body tokens
- struct shell: name, generic parameters, parsed field names/types, and default token data where applicable
- choice shell: name, generic parameters, variant names, and payload field type shells
- type alias shell: name and target type annotation. Parameterized generic aliases are rejected before shell creation.
- trait shell: name, requirement signature shells, and requirement type-reference dependency edges
- conformance shell: target type reference, trait references, and declaration source context
- start shell: entry-file executable token body, excluded from dependency sorting

### Header and AST ownership boundary

Header parsing owns top-level discovery and declaration shell parsing. AST must not rediscover top-level symbols or reconstruct top-level declaration shells from raw tokens.

Header parsing builds `ModuleSymbols`, the order-independent top-level symbol, import, export, builtin, type-alias, and source-file metadata package. Dependency sorting finalizes the sorted declaration list. AST consumes that package directly.

## Stage 3: Dependency Sorting

Path: [`src/compiler_frontend/module_dependencies.rs`](../src/compiler_frontend/module_dependencies.rs)

Dependency sorting operates only on top-level declaration headers and header-provided dependency edges.

It owns:

- topological sorting of parsed top-level declaration headers
- cycle detection in the strict top-level declaration graph
- missing header-provided dependency diagnostics
- source-order stability among otherwise independent declarations
- finalizing `ModuleSymbols.declarations` in sorted order and appending builtin declarations
- appending the implicit entry `start` header after sorted declarations

It does not use executable function/start body references or body-local declarations. Constant initializer references are not body references. They are top-level compile-time declaration dependencies and belong in the header dependency graph.

Dependency sorting orders constants using header-provided constant initializer dependency edges. Same-file constants keep source-order semantics. Same-file forward references are rejected. Cross-file constant cycles are dependency cycles.

Same-file symbol hints that do not materialize as headers are not always dependency-sort errors. When a hint looks like a same-file declaration reference but no graph header exists, Stage 3 defers to AST type/expression resolution so the later stage can report the more precise semantic diagnostic.

### Facades and source libraries in dependency sorting

Module-root facades (`#mod.bst` files that belong to a project module root) participate in dependency sorting like other top-level declaration providers. They remain boundary export layers for visibility, but public authored declarations, public re-exports, exported constants, and exported type surfaces must still be ordered before declarations in outside modules that import them through the facade.

Other files inside the same module should not depend on symbols declared directly inside the module-root facade. Header import visibility, not dependency sorting, enforces that boundary.

Source-library facades (`#mod.bst` files provided by builder source libraries such as `@html`) also participate in dependency sorting. Their declarations are first-class providers to the consuming module, not opaque boundaries. Because source libraries have no outgoing dependency edges to project files, the topological sort naturally places them before project files that import them.

Source-library facade export edges may use the public import path rather than the concrete `#mod.bst` header path. Stage 3 treats those as satisfied facade-export edges, not as new graph nodes. This allows public source-library API names to order consumers without leaking the facade file's filesystem identity into the dependency graph.

### Header/dependency/AST contract

Header parsing and dependency sorting are responsible for making top-level declarations linearly consumable by AST.

Declaration-shell parsing is intentionally shared through `declaration_syntax`: headers parse the top-level shells needed for dependency discovery and declaration registration, while AST uses the same shell shapes for body-local declarations and owns all semantic resolution from those shells.

After dependency sorting:

- AST receives headers in dependency order. It does not topologically sort constants, structs, choices, functions, or aliases again.
- AST must not rediscover top-level declarations from raw file tokens or rebuild file import visibility from scratch.
- AST resolves declaration shells in sorted order, then parses executable bodies against the completed environment.
- AST may register nominal identity and generic parameter metadata before constants so constructors are name-resolvable. Unresolved field and variant constructor shells stay in AST-owned side tables until semantic `TypeId`s are checked and final member definitions are written to `TypeEnvironment`.
- If AST needs a top-level declaration to be resolved before another declaration, that dependency belongs in the header dependency graph.
- If a new feature introduces a top-level dependency, add it to header parsing/dependency sorting rather than adding another AST ordering pass.
- The implicit entry `start` header is never a dependency participant and is always emitted after sorted declarations.

## Stage 4: AST Construction

Path: [`src/compiler_frontend/ast/mod.rs`](../src/compiler_frontend/ast/mod.rs)

AST consumes already-sorted declaration headers and the header-built module environment. It resolves declarations in order, folds constants/templates, parses executable bodies, type-checks expressions, validates function terminality, and emits typed AST nodes.

Internally, AST construction is organized around three phase owners:

- [`build_ast_environment`](../src/compiler_frontend/ast/module_ast/environment/) consumes header-built file visibility, then resolves declaration metadata, constants, nominal types, function signatures, receiver catalog data, trait metadata, and shared environment side channels.
- [`emit_ast_nodes`](../src/compiler_frontend/ast/module_ast/emission/) parses function/start/template bodies against the completed environment, validates function terminality, emits AST nodes, and emits const-template output.
- [`finalize_ast`](../src/compiler_frontend/ast/module_ast/finalization/) performs HIR-boundary cleanup, including doc fragment extraction, const top-level fragment assembly, reactive template metadata propagation, template normalization, module constant normalization, type-boundary validation, const-fact collection, concrete choice-definition gathering, builtin AST merging, and final `Ast` construction.

Important AST subowners:

- [`type_resolution/`](../src/compiler_frontend/ast/type_resolution/) owns parsed type-reference resolution to canonical `TypeId`, including source-visible lookup, aliases, fixed collection capacity, maps, and generic nominal instantiation.
- [`module_ast/environment/public_surface.rs`](../src/compiler_frontend/ast/module_ast/environment/public_surface.rs) owns semantic public facade API validation after type and trait identities are resolved.
- [`generic_functions/`](../src/compiler_frontend/ast/generic_functions/) owns generic free-function templates, call inference, and concrete instance emission before HIR.
- [`generic_bounds.rs`](../src/compiler_frontend/ast/generic_bounds.rs) owns static trait-bound validation for concrete nominal generic instances.
- [`templates/`](../src/compiler_frontend/ast/templates/) owns template parsing, composition, folding, slot routing, render plans, control-flow validation, and template structural metadata traversal.
- [`module_ast/finalization/const_fact_collection.rs`](../src/compiler_frontend/ast/module_ast/finalization/const_fact_collection.rs) owns explicit module, private top-level, and body-local const-fact collection after AST finalization.
- [`builtins/casts/`](../src/compiler_frontend/builtins/casts/) owns builtin cast target classification, evidence, policies, and core cast-trait metadata. [`builtins/casts/resolution.rs`](../src/compiler_frontend/builtins/casts/resolution.rs) owns AST cast resolver wiring at explicit typed boundaries.
- [`field_access/`](../src/compiler_frontend/ast/field_access/) owns source fields, receiver calls, and compiler-owned collection/map builtin member access.

AST owns:

- semantic declaration resolution from header shells into canonical `TypeId`, function signature, constant, nominal type, trait, and receiver-method metadata
- public `#mod.bst` API surface validation, including private type leakage in exported signatures/fields/aliases/constants and private trait leakage in exported trait metadata
- executable body parsing, body-local declarations, expression parsing, and type checking
- function terminality validation for non-unit success returns before HIR lowering
- contextual coercion at explicit frontend-owned boundaries: declarations, assignments, returns, template/string content, casts, and backend/prelude call contracts
- generic validation, generic free-function template storage, call inference, and concrete instance emission before HIR
- trait declaration/evidence validation, static generic-bound evidence checks, and bound-provided receiver-call resolution before HIR
- explicit cast target/evidence resolution and builtin cast folding
- constant folding, const-fact collection, and const-only validation
- template composition, compile-time folding, runtime render-plan preparation, reactivity metadata preservation, and HIR-boundary template normalization

AST should be described by this ownership and data-flow contract, not by a fixed internal pass count. The internal substeps inside each phase are implementation details and may change as the stage is simplified.

The direct HTML-project Beandown API uses the same tokenizer, synthetic-header preparation, dependency sorting, and AST folding path as compiler-integrated `.bd` imports, then extracts the folded `content` constant. It deliberately stops before HIR generation, borrow validation, backend lowering, artifact writing, and output cleanup.

### Generics contract

Generics are resolved before HIR. Header parsing records declaration-site generic parameter metadata on declaration shells, but AST owns semantic registration, validation, inference, and concrete instance emission.

- Header parsing records generic parameter lists and declaration metadata. It does not infer or substitute generic types.
- AST registers generic parameter lists in `TypeEnvironment` and resolves generic signatures to canonical `TypeId`s.
- AST stores generic free-function templates and validates generic bodies before concrete calls are emitted.
- AST infers generic function calls from immediate argument evidence and immediate expected result context only.
- AST emits concrete generic function instances before HIR generation.
- HIR must never carry unresolved generic executable types or unsolved generic function calls.
- Borrow validation receives concrete HIR and does not consume generic template state.
- Backends never solve generic type arguments or generic function instances.

### Traits contract

Trait declarations and conformances are resolved before HIR. Header parsing records trait and conformance shells. AST owns semantic trait identity, requirement type resolution, conformance evidence validation, reusable evidence visibility, and generic-bound evidence checks.

- Traits are compile-time metadata in `TraitEnvironment`, not `DataType` values.
- Trait names are valid in trait declarations, conformance declarations, and generic bounds only.
- Trait names in ordinary type position are rejected with a structured static-contract diagnostic.
- Explicit conformance evidence lives in `TraitEvidenceEnvironment` with stable evidence IDs and requirement-to-method mappings.
- Static generic bounds use visible reusable evidence during generic function calls and concrete generic nominal instantiation.
- Static trait-bound receiver calls are resolved to concrete source calls before HIR.
- HIR and backends do not carry trait-object construction, erased dispatch, or trait evidence metadata for runtime dispatch.

### Imports and visibility

AST consumes the header-built file visibility environment through `ScopeContext`. It may validate semantic use of visible symbols, but it must not rebuild import bindings or rediscover top-level visibility.

All user-visible names go through one collision policy. Same-file declarations, source imports, external imports, type aliases, prelude symbols, and builtins cannot silently shadow each other.

External expression and type resolution must go through the active `ScopeContext` visibility lookup. If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST pass.

### Type checking and coercion

Expression evaluation determines the natural type of an expression and stays strict. Contextual coercion is applied only by the frontend site that owns the boundary.

AST emission should carry canonical `TypeId`s through field access, receiver lookup, builtin receiver validation, call validation, operator result typing, and compatibility checks. `DataType` remains parse-only or diagnostic spelling once a semantic `TypeId` exists.

Examples of boundary owners:

- declarations
- assignments
- returns
- template/string content
- explicit `cast` target boundaries
- backend/prelude call contracts

Detailed numeric rules, match syntax, cast syntax, and string coercion rules belong in [`language-overview.md`](language-overview.md).

### Constants and folding

The AST consumes parsed compile-time constants directly and type-checks their initializers. Module constants are compile-time metadata, not runtime top-level declaration statements.

Constants and top-level const templates must fold at compile time. Runtime expressions in constants are rejected.

Runtime expressions that cannot fold are currently represented in AST as stack-oriented RPN node vectors before HIR lowering. Those expression vectors are expression-only structures, not broad statement fragments. `ExpressionKind::Runtime` carries `ExpressionRpn`, copy expressions carry `PlaceExpression`, and `ExpressionKind::ValueBlock` is the only expression variant allowed to carry statement bodies.

### Templates

AST owns template semantic preparation.

It owns:

- composing slots, inserts, wrappers, and child templates
- folding fully constant templates into string literals
- preserving structured template `if` and `loop` bodies for runtime lazy lowering
- preparing runtime slot source/site plans after AST-owned schema extraction and contribution routing
- validating const-required template control flow before HIR
- rejecting escaped slot/insert helper artifacts that are invalid after composition/routing
- preserving runtime templates as runtime expressions
- removing helper-only template artifacts before HIR
- emitting builder-facing const top-level fragment metadata

HIR only lowers finalized runtime templates that remain after AST folding. Runtime template control flow lowers inline as ordinary HIR branches, loops, accumulator appends, and AST-prepared runtime slot source/site plans in the enclosing function, not as backend-specific template control-flow nodes. HIR consumes AST-prepared slot source/site plans only. It does not parse directives or validate slot schemas.

Compile-time page fragments stay outside HIR.

User-facing template syntax, directives, markdown behavior, and slots are in [`src/docs/templates/#page.bst`](src/docs/templates/#page.bst).

### Reactivity V1

Reactivity V1 is frontend-owned source and template metadata that later stages preserve for backend feature validation and backend lowering. It must not become a second type system or a general closure/function-value model.

Stage ownership:

- Declaration syntax parses `$Type`, `$=`, and `$T` parameter access markers as syntax only.
- AST resolves the underlying ordinary `TypeId`, assigns reactive source identity, validates `$(source)` template subscriptions, and preserves reactive template string metadata.
- HIR carries backend-facing reactive source/template metadata and reachability facts without reparsing template directives or becoming a backend render-plan language.
- Borrow validation treats subscriptions as read-only source dependencies, not active borrow lifetimes, while ordinary mutations continue to follow existing mutable/exclusive rules.
- Backend feature validation applies the selected target contract before lowering. Runtime reactive behavior remains backend-owned artifact policy.

## Stage 5: HIR Generation

Path: [`src/compiler_frontend/hir/`](../src/compiler_frontend/hir/)

HIR generation lowers the fully typed AST into the first backend-facing semantic IR. HIR is structured enough for borrow/exclusivity analysis: control flow, locals, calls, regions, and terminators are explicit, while ordinary value construction and pure operators may remain as nested expression trees.

HIR stores compact frontend `TypeId`s but does not own a separate semantic type table. `lower_module` returns the completed `HirModule` beside the AST-built `TypeEnvironment`. Borrow validation and backends must use that paired environment for semantic type queries.

HIR owns:

- explicit control-flow structure
- block, jump, terminator, loop, branch, return, and match representation
- explicit locals, regions, and call targets
- expression side-effect linearization. Calls, checked operations, casts, map operations, and other effectful expression work become explicit statement preludes plus temporary locals before the final value expression is used.
- lowered runtime template expressions
- inline runtime template control flow as ordinary CFG
- runtime slot source/site plans lowered as ordinary string accumulators and appends
- hashmap literals and map member operations as first-class HIR operations for borrow validation and backend feature validation
- module constants as compile-time metadata
- advisory private const-fact metadata projected from AST for future optimization consumers
- function-origin metadata such as entry start versus normal functions
- doc fragments, rendered path usages, warnings, and other module metadata that survives from AST into builder-facing compiled modules
- stable external function IDs selected during AST resolution
- builtin runtime cast expressions and fallible cast operations that survive AST folding
- direct user-function calls emitted during HIR lowering for user-defined cast evidence selected by AST
- checked numeric effects represented as `HirStatementKind::NumericOp` with the selected `NumericFailureMode`
- Float formatting and external Float boundary validation represented as `FormatFloat` and `ValidateFloat` statements
- backend-neutral syntactic reachability over functions, blocks, external call IDs, maps, reactive metadata, runtime casts, checked numeric operations, and Float statements from explicit roots
- enough structure for borrow validation and later backend lowering

HIR does not:

- fold templates
- reconstruct missing template plans
- carry backend-specific template control-flow nodes
- carry compile-time top-level page fragments
- use private const facts to change semantics in this plan
- solve generic functions or carry unresolved generic parameter executable types
- decide trait conformance or generic-bound evidence
- carry user-defined cast trait evidence or generic-bound cast evidence into backend lowering
- decide final runtime ownership
- model exact lifetimes

Plain `HirBinOp` remains valid for booleans, comparisons, and string concatenation. Runtime scalar arithmetic and unary negation must lower through `HirStatementKind::NumericOp`. HIR validation rejects regressions where numeric arithmetic survives as ordinary expression ops.

HIR lowering treats user-facing source errors as already diagnosed by AST or earlier stages. HIR lowering and HIR validation use `CompilerError` for internal transformation invariants. If a non-unit function can fall through after AST terminality validation, that is a compiler invariant breach.

### HIR validation

[`src/compiler_frontend/hir/validation.rs`](../src/compiler_frontend/hir/validation.rs) and [`src/compiler_frontend/hir/validation/`](../src/compiler_frontend/hir/validation/) validate the freshly lowered module before it leaves Stage 5. Borrow validation and backend feature validation should receive already-coherent HIR, not defensively repair it.

HIR validation checks definition IDs, frontend `TypeId` links, region graph shape, start-function and function-origin metadata, CFG ownership, doc fragments, module constants, reactive metadata, side-table mappings, local/place references, terminators, patterns, and expression invariants.

Important examples:

- Plain arithmetic `HirBinOp` and `HirUnaryOp::Neg` that should have been lowered to checked `NumericOp` statements are rejected as HIR shape errors.
- `HirExpressionKind::Float` values must be finite `f64`. `NaN` and `Infinity` literals are rejected as internal invariant breaches.
- Control-flow structure, block terminators, local references, side-table mappings, and expression shapes are validated for borrow validation and backend consumption.

Validation failures are `CompilerError` with `ErrorType::HirTransformation`, not user-facing `CompilerDiagnostic`, because they represent compiler-internal lowering invariants.

### Reachable backend features

[`src/compiler_frontend/hir/reachability.rs`](../src/compiler_frontend/hir/reachability.rs) records syntactic reachability for functions, blocks, external calls, map literals/operations, reactive template-backed values, reactive sinks, runtime casts, checked numeric operations, and Float formatting/validation statements. It does not fold constants, eliminate dead branches, inspect borrow facts, or perform backend lowering.

Some target-gated checks need more than backend-neutral reachability. For example, generic runtime value validation scans reachable blocks with the module `TypeEnvironment` because generic-instance detection is semantic type analysis, not a raw HIR reachability fact.

### External calls

Calls to builder-provided package functions lower to stable external call targets such as `CallTarget::ExternalFunction(ExternalFunctionId)`.

HIR does not store package import syntax or backend runtime names. Borrow validation can resolve external IDs through the external package registry to recover access rules and return-alias metadata. Backends map the same IDs to target-specific helpers, imports, generated glue, runtime names, or target-native operations.

The HTML builder and backend validators use HIR reachability for runtime artifact planning and target-contract validation. This is syntactic CFG/function reachability, not constant-condition dead-code elimination, optimization, or ownership analysis.

### Mutable rvalues

Fresh rvalues passed to mutable (`~T`) call slots are materialized into compiler-introduced hidden locals before borrow validation. Borrow validation then sees ordinary local access, not a special temporary node kind.

## Stage 6: Borrow Validation

Path: [`src/compiler_frontend/analysis/borrow_checker/`](../src/compiler_frontend/analysis/borrow_checker/)

Borrow validation enforces borrow/exclusivity rules and produces side-table facts used by later ownership-aware lowering. It does not mutate HIR, compute exact lifetimes, or decide final runtime ownership.

GC remains the semantic fallback. Ownership and deterministic destruction are optimization layers described in [`memory-management-design.md`](memory-management-design.md).

Borrow validation is mandatory for backend semantic parity:

- invalid overlapping mutable/shared access is rejected before backend lowering
- use-after-move and invalid access patterns are rejected before backend lowering
- hashmap `get` results alias the receiver conservatively, so later map mutation is rejected while the shared result is live
- valid programs may expose additional facts for ownership-aware lowerings
- GC-only backends can ignore ownership-specific optimization facts while preserving semantics

Current borrow facts include function summaries, block and statement state snapshots, statement/terminator/value access facts, conservative reactive invalidation facts, and advisory drop sites for later ownership-aware lowering. These facts are read-only side tables. They do not rewrite HIR.

Reactive subscriptions are not active borrows. Borrow validation records conservative invalidation facts for assignments, place writes, map mutations, and mutable call arguments that may dirty reactive sources, while ordinary alias/exclusivity rules continue to apply to the underlying values.

Borrow validation does not track per-field or per-projection aliasing yet. HIR remains the stable semantic representation. Borrow facts live in side tables keyed by HIR/value IDs.

The language-level no-shadowing rule supports simpler name and borrow analysis, but the rule itself is specified in [`language-overview.md`](language-overview.md).

## Stage 7: Backend lowering

Backend lowering belongs to project builders after frontend compilation.

### Navigation

- [`src/backends/js/`](../src/backends/js/) owns HIR-to-JavaScript lowering and JS runtime helper emission.
- [`src/backends/wasm/`](../src/backends/wasm/) owns HIR-to-Wasm-LIR lowering, Wasm runtime contracts, request validation, debug output, and optional binary emission.
- [`src/projects/html_project/html_project_builder.rs`](../src/projects/html_project/html_project_builder.rs) owns the HTML `BackendBuilder` implementation and route-level artifact assembly.
- [`src/projects/html_project/js_path.rs`](../src/projects/html_project/js_path.rs) owns the HTML JS page-bundle path.
- [`src/projects/html_project/wasm/`](../src/projects/html_project/wasm/) owns HTML-Wasm export planning, bootstrap JS, and artifact assembly around the core Wasm backend.
- [`src/projects/html_project/external_js/`](../src/projects/html_project/external_js/) owns provider-backed JavaScript imports, runtime module planning, generated glue, import maps, and runtime asset emission.
- [`src/projects/html_project/tracked_assets.rs`](../src/projects/html_project/tracked_assets.rs) owns HTML tracked-asset planning and passthrough emission.

### Backend handoff

Backend builders consume `Module` values containing:

- canonical entry-point path
- validated HIR
- the paired `TypeEnvironment`
- borrow-analysis facts
- warnings
- resolved const top-level fragments
- entry runtime fragment count
- the effective external package registry for the module
- deduplicated provider/runtime external-import metadata

Backend lowering has three boundaries:

- The build system owns orchestration and output writing. It calls the selected `BackendBuilder`, receives a `Project` containing explicit `OutputFile` artifacts and a cleanup policy, then writes those artifacts through the shared output writer.
- Project builders own project-specific artifact assembly policy. The HTML builder resolves page routes, validates project-level output conflicts, chooses a backend path, merges page fragments, plans tracked assets, and returns builder-owned artifacts.
- Backend lowerers own target-specific code generation from validated HIR and companion metadata. They do not rediscover frontend imports, traits, generic evidence, source config, or template syntax.

Language-owned HIR operations such as maps, checked numeric operations, Float formatting, Float validation, builtin casts, and reactive metadata lower through backend-owned runtime helpers or target-native code selected by the backend. They must not be silently reinterpreted as unchecked target-native behavior when Beanstalk semantics require checks or helper calls.

Ownership-aware backends may use borrow facts and memory-model metadata for optimization. GC-only backends can lower through the semantic baseline without deterministic drop behavior.

### Build-system output writing

Backends and project builders produce `OutputFile` records. They should not write final project outputs directly. The build system owns output-root validation, skip-unchanged writes, manifest tracking, and stale artifact cleanup through `write_project_outputs`.

### HTML builder artifact assembly

The HTML project builder owns route-level artifact assembly. It derives canonical HTML output paths, validates duplicate outputs, selects the configured backend path, emits build-level external runtime assets, emits tracked assets from rendered path usages, selects the entry page, and returns the complete project artifact set plus cleanup policy.

### External package and backend feature validation

Backend validation runs before target lowering. External package validation checks reachable external calls against the selected target’s lowering metadata. Backend feature validation checks target-gated HIR features through explicit reachability roots and returns structured diagnostics for user-visible target-contract violations. Backend lowerers should receive only HIR features and external calls that their selected target contract can lower.

Backend feature validation does not hard-code one execution root. The project builder selects a root policy for the artifact being produced, such as entry-start reachability for page bundles or an explicit exported-function set for artifacts whose callable surface is builder-defined.

### JS lowering

The HTML JS path lowers HIR through the JS backend, renders const page fragments into the document, creates runtime fragment slots, embeds or module-loads the generated JS bundle, calls entry `start` once, and hydrates returned runtime fragments into the slots in source order. It emits only the entry-reachable function set for page bundles and uses the JS backend’s referenced external-function metadata to generate only the glue wrappers that the emitted bundle calls.

The direct JS backend path can emit a complete standalone JS bundle when configured to include every HIR function. The HTML page-bundle path selects the reachable subset needed for the route artifact.

### Wasm lowering

The core Wasm backend owns HIR-to-Wasm-LIR lowering, Wasm runtime contracts, request validation, optional binary emission, and backend debug output. The HTML-Wasm path is builder orchestration around that backend. It chooses exported functions, requests helper exports, invokes Wasm lowering, generates bootstrap JS, and assembles route HTML, JS, and Wasm files.

### External JS runtime assets and glue

Provider-backed external JS has two emission levels:

- Build-level runtime emission deduplicates JS runtime assets and required runtime module specifiers across all compiled modules.
- Module-level glue generation inspects external functions referenced by the emitted JS bundle and emits only the wrapper module, import preamble, and import-map entries needed for that page bundle.

### Tracked assets

Tracked assets are a builder policy over frontend-rendered path usages. The frontend records semantic path facts while rendering paths. The HTML builder decides which file paths become emitted assets, chooses output paths relative to the final page route, reports asset warnings/conflicts, and returns asset bytes as ordinary `OutputFile` artifacts.
