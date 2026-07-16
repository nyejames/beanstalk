# Beanstalk Compiler Design Overview

Beanstalk is a high-level language with first-class string templates. Its compiler is a staged, backend-neutral library used by the project tool, development server and backend builders.

This document is the single source of truth for accepted compiler architecture and cross-stage contracts. It describes the final design the compiler and backends implement, including contracts that the progress matrix still reports as incomplete.

This is not an implementation-status report. Use:

- [`language-overview.md`](language-overview.md) for compiler-facing language semantics and syntax
- [`src/docs/codebase/design-scope/overview.bd`](src/docs/codebase/design-scope/overview.bd) for the high-level design bias and scope boundaries
- [`src/docs/codebase/memory-management/overview.bd`](src/docs/codebase/memory-management/overview.bd) for access, borrow, GC and ownership semantics
- [`src/docs/codebase/style-guide/style-guide.bd`](src/docs/codebase/style-guide/style-guide.bd) for implementation standards
- [`src/docs/progress/#page.bst`](src/docs/progress/#page.bst) for current support and backend coverage
- [`roadmap/roadmap.md`](roadmap/roadmap.md) for sequencing and unaccepted proposals

User-facing pages under `docs/src/docs/**` contain examples and teaching material. They do not replace this compiler architecture reference.

## Architectural invariants

- A directory-scoped module is the canonical semantic compilation unit and is compiled once per build.
- Stage 0 owns the canonical module graph, file ownership, legal topology and import namespace identities.
- Header parsing discovers top-level declarations once and builds the visibility consumed by dependency sorting and AST.
- Each module owns one local `TypeEnvironment`, local HIR identities and local borrow facts.
- Cross-module references use stable project semantic identities, never donor-local indexes or `TypeId` values.
- User-facing failures use `CompilerDiagnostic`. Internal, infrastructure and impossible-state failures use `CompilerError`.
- Generics, traits, cast evidence, constants and template semantics resolve before executable HIR reaches a backend.
- TIR is AST-local. HIR receives only folded strings or neutral owned runtime handoff data.
- HIR is the first backend-facing semantic IR and carries explicit cross-module call targets.
- Borrow validation reads HIR and produces side tables and exported call-effect summaries without rewriting HIR.
- GC remains the semantic baseline. Ownership-aware lowering is an optimisation with identical source semantics.
- Backends consume compiled project graphs and entry link plans. They do not rediscover source structure.
- Parallel work must preserve deterministic identities, diagnostics and output ordering.

## Frontend structure at a glance

### Code navigation map

- Project/build entry:
  [`src/main.rs`](../src/main.rs),
  [`src/projects/`](../src/projects/),
  [`src/build_system/`](../src/build_system/),
  and [`src/builder_surface/`](../src/builder_surface/).
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
  [`source_packages/`](../src/compiler_frontend/source_packages/),
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
- HIR, analysis and backend validation:
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

### Input, paths, diagnostics and symbols

- [`src/compiler_frontend/tokenizer/`](../src/compiler_frontend/tokenizer/) converts source text into located tokens and handles string/template delimiter context.
- [`src/compiler_frontend/compiler_messages/`](../src/compiler_frontend/compiler_messages/) owns typed diagnostics, labels, source locations, stable diagnostic descriptors, render-boundary aggregation and terminal, terse and development-server renderers.
- [`src/compiler_frontend/symbols/`](../src/compiler_frontend/symbols/), [`symbols/interned_path.rs`](../src/compiler_frontend/symbols/interned_path.rs) and [`src/compiler_frontend/paths/`](../src/compiler_frontend/paths/) own interned source identities, path formatting/resolution and canonical symbol identity shared across diagnostics, imports and lowering.

### Declarations, imports and type surface

- [`src/compiler_frontend/headers/`](../src/compiler_frontend/headers/) discovers top-level declarations, imports, normalised path/reference shells, declaration shells, constant initializer dependency hints and start-body separation.
- [`src/compiler_frontend/module_dependencies.rs`](../src/compiler_frontend/module_dependencies.rs) orders top-level declaration headers by header-provided dependency edges.
- [`src/compiler_frontend/declaration_syntax/`](../src/compiler_frontend/declaration_syntax/) owns shared declaration-shell parsing used by headers and body-local AST parsing. It keeps syntactically equivalent declaration shapes on one parser path, but it does not own semantic type resolution.
- [`src/compiler_frontend/datatypes/`](../src/compiler_frontend/datatypes/) owns `TypeEnvironment` as canonical semantic type identity and `DataType` as parse-only or diagnostic-only type syntax. Semantic identity is `TypeId` equality in the relevant `TypeEnvironment`. `DataType` must not be used for semantic decisions in executable AST or HIR.
- [`src/compiler_frontend/type_coercion/`](../src/compiler_frontend/type_coercion/) owns implicit contextual compatibility and promotion rules layered on top of type identity. Explicit `cast` resolution is AST-owned and uses compiler-owned cast policy/evidence metadata instead of the coercion path.
- [`src/compiler_frontend/value_mode.rs`](../src/compiler_frontend/value_mode.rs) tracks frontend access classification for bindings, expressions, call arguments and receiver use. It keeps mutability/reference state separate from `DataType`. Runtime ownership is a later borrow/lowering concern.
- [`src/compiler_frontend/traits/`](../src/compiler_frontend/traits/) owns parsed trait shells, resolved trait definitions, explicit same-file nominal conformance evidence, reusable evidence visibility, static generic-bound evidence checks and trait diagnostics. Trait metadata is compile-time frontend state, not a value type or backend-side source rediscovery path.
- Stage 0 owns canonical module identity, file ownership, legal topology and source-package roots. Builder-supplied source packages use the same compiled-interface model as project modules.
- [`src/compiler_frontend/external_packages/`](../src/compiler_frontend/external_packages/) stores backend-provided virtual package metadata, package-local symbol paths and stable external symbol IDs. External package symbols are resolved by package path plus symbol path. The prelude `io` namespace alias is the only bare-name external namespace exception.
- [`src/compiler_frontend/builtins/`](../src/compiler_frontend/builtins/) owns compiler-defined language symbols and operations that are neither user source declarations nor backend-provided external packages, including builtin cast target classification, policy metadata, runtime error codes and core cast trait definitions/evidence.
- [`src/compiler_frontend/style_directives/`](../src/compiler_frontend/style_directives/) owns the merged frontend and builder directive registry used by tokenizer and template parsing.
- Design-scope and deferred-feature diagnostics should be centralised through typed `CompilerDiagnostic` constructors. Deferred features and outside-design-scope rejections must remain distinct diagnostic reasons.

### Semantic lowering and analysis

- [`src/compiler_frontend/ast/`](../src/compiler_frontend/ast/) builds the typed AST from sorted headers, resolves semantic information, parses executable bodies, folds constants/templates, validates function terminality and prepares HIR input.
- [`src/compiler_frontend/ast/const_eval/`](../src/compiler_frontend/ast/const_eval/) owns AST compile-time evaluation for constants and foldable template expressions.
- [`src/compiler_frontend/hir/`](../src/compiler_frontend/hir/) lowers the typed AST into the first backend-facing semantic IR.
- [`src/compiler_frontend/analysis/borrow_checker/`](../src/compiler_frontend/analysis/borrow_checker/) validates borrow/exclusivity rules and produces side-table facts for later lowering.

## Build-system and frontend boundary

The build system prepares one canonical project graph, compiles each module once and passes an immutable project compilation to the selected backend builder.

The architectural handoff is:

```rust
pub trait BackendBuilder {
    fn build_backend(
        &self,
        compilation: ProjectCompilation,
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

    fn frontend_surface(&self) -> BuilderSurface;
}

pub struct ProjectCompilation {
    pub structure: ProjectModuleGraph,
    pub modules: Vec<CompiledModuleArtifact>,
    pub generated: Vec<ModuleGeneratedArtifacts>,
    pub entries: Vec<EntryAssembly>,
    pub package_facade: Option<ProjectPackageAssembly>,
}

pub struct ProjectBuilder {
    pub backend: Box<dyn BackendBuilder + Send>,
}

pub struct BuilderSurface {
    pub binding_packages: ExternalPackageRegistry,
    pub source_packages: SourcePackageRegistry,
    pub config_keys: ProjectConfigKeyRegistry,
    pub external_import_providers: ExternalImportProviderRegistry,
    pub external_import_cache: ExternalImportProviderCache,
    pub external_import_resolution_table: ExternalImportResolutionTable,
    pub builder_runtime_packages: Vec<BuilderRuntimePackageMetadata>,
    pub source_file_kinds: SourceFileKindRegistry,
}
```

The exact Rust type names may vary. The ownership boundaries do not.

The build system owns:

- project and config discovery
- the canonical source index
- module roots, file ownership and legal dependency topology
- deterministic module scheduling
- project-wide generic instance materialisation
- entry assemblies and the optional external package facade
- output writing and stale-artifact cleanup

Each `CompiledModuleArtifact` owns:

- its immutable public interface
- validated module-local HIR
- its local `TypeEnvironment`
- borrow facts and exported function-effect summaries
- diagnostics and warnings
- dormant root activity and page-fragment metadata
- backend-neutral runtime dependency facts

Generated concrete generic functions live in generated-artifact sidecars. They do not mutate immutable base module artifacts.

Backend builders do not:

- load source files
- discover module topology
- parse `config.bst`
- perform semantic frontend compilation
- materialise generic functions from source templates
- reconstruct imports or package visibility
- write final project outputs directly

Builders declare the frontend-visible surface:

- source-backed packages
- binding-backed packages
- config keys
- style directives
- external import providers
- builder runtime packages
- supported source file kinds

Source and binding registries remain separate because their compiler and runtime needs differ. Shared `PackageMetadata` classifies both through independent `PackageOrigin` and `PackageBacking` axes.

`ProjectConfigKeyRegistry` is declarative Stage 0 metadata. It defines allowed core and builder-owned keys plus their folded value shapes.

`config.bst` is build-system-owned compile-time Beanstalk source. It is compiled through AST so it can use normal constants, folding, allowed Core or Builder imports and typed diagnostics. It is not a module, produces no HIR, has no `start` and exports no language-visible declarations.

Authored config entries are known top-level `#` constants. Imported constants and support types may contribute to expressions but never become config entries. Config rejects project-local or relative imports, runtime declarations, mutable bindings, functions, traits, conformances, standalone templates and page fragments.

External import providers convert supported non-Beanstalk files into typed binding-backed package surfaces before AST consumes visibility. Provider results may also record runtime imports and assets for later link planning.

Builder-runtime packages such as `@web/canvas` use the same binding identity and runtime asset path as provider-created packages.

Builder-supported source file kinds participate in extensionless source discovery without becoming modules. Beandown `.bd` and Markdown `.md` inputs become ordinary synthetic compile-time declarations during header preparation.

Project-specific config validation remains in `BackendBuilder::validate_project_config`. User config mistakes remain `CompilerDiagnostic` values while infrastructure failures remain `CompilerError` values.

Complex release optimisation stays outside the fast frontend path unless correctness requires it.

### Diagnostics, path identity and deterministic aggregation

Diagnostics are compiler data, not a final formatting step.

- `CompilerDiagnostic` represents source, syntax, import, type, rule, borrow, config and target-contract failures.
- `CompilerError` represents compiler invariants, filesystem failures, backend failures and tooling infrastructure failures.
- `DiagnosticBag` owns stage-local accumulation.
- `CompilerMessages` is used only at build and rendering boundaries.
- Diagnostic payloads carry structured facts, stable reasons, source locations, symbols and semantic IDs instead of pre-rendered prose.

`SourceLocation` stores interned path and scope identity. Rendering and filesystem-adjacent code resolve that identity through the build's `StringTable`. Boundary results carry the string table needed to render diagnostics after later build stages fail.

Each canonical module owns its diagnostics. A shared module is compiled once and its diagnostics are emitted once. A dependent module is not compiled when a required public interface failed, avoiding secondary unknown-name and type cascades. Independent graph branches continue compiling.

Parallel work may fork string tables only when deltas are merged and remapped in deterministic order:

- module deltas merge in canonical module order
- file-preparation deltas merge in original source-file order
- diagnostics and warnings never merge in worker-completion order
- tokens, headers, type-rendering contexts and module payloads are remapped before later stages consume them

Full table cloning remains available for genuinely independent identity boundaries. It is not the ordinary module-compilation path.

Provider-backed discovery remains serial while it mutates shared package IDs, provider caches, resolution tables or diagnostic identity. Parallel provider discovery requires deterministic provider deltas and remapping.

### Style directive contract

Project builders can register style directives through `frontend_style_directives`.

- Frontend-owned directives are always available.
- Builder directives cannot override frontend-owned names.
- Tokenizer and template parsing use the same merged registry.
- Unknown directives are rejected strictly.

Individual directive syntax and behaviour belong in [`src/docs/templates/#page.bst`](src/docs/templates/#page.bst) and [`language-overview.md`](language-overview.md).

### Type identity contract

Each compiled module owns one local `TypeEnvironment`. `TypeId` equality in that environment is the only valid comparison for module-local semantic decisions.

Cross-module interfaces use canonical project type identities rather than donor-local `TypeId` values. The canonical representation covers:

- builtins
- module-owned structs and choices
- transparent aliases
- options, collections, maps and fallible carriers
- concrete generic nominal instances
- generic parameters inside exported generic templates
- external package types

A consumer module may intern compact local `TypeId` handles for imported canonical types. Its `TypeEnvironment` retains the origin mapping back to canonical identity. Cross-module equality compares canonical identity, never rendered names or unrelated local handles.

`DataType` is parse-only or diagnostic-only after semantic resolution. It must not drive executable AST, HIR or backend semantic decisions.

Collection and map identity remain canonical constructed shapes:

- growable `{T}` and fixed `{N T}` collections are distinct
- fixed capacity is semantic identity, not an allocation hint
- `{K = V}` maps store key and value identities directly
- backends query semantic shapes rather than parse syntax or private side tables

Type diagnostics carry semantic identities plus context enums. Renderers resolve source-level names through `DiagnosticRenderContext` at the output boundary.

AST builds the local `TypeEnvironment`. Early nominal registration records identity and generic parameter metadata. Canonical fields and variants are written after AST resolves their type shells.

Member queries expose borrowed field or variant views and direct lookup helpers. Later stages do not clone member lists for semantic lookup.

AST body emission receives `AstTypeInterner`, a narrow facade over `TypeEnvironment` that allows derived type interning and module-local compatibility caching without permitting nominal declaration mutation.

Imported canonical types are interned through the same narrow AST-owned boundary. Consumer-local handles retain their canonical origin and do not mutate the exporting module's environment.

Function signatures store local semantic `TypeId` values after resolution. Exported signatures also project canonical cross-module types into the immutable public interface.

HIR structs and choices retain their local frontend type links. Cross-module call targets and public interfaces use stable module semantic identities.

External parameters with no frontend mapping use `ExpectedParameterType::UnknownExternal`, never sentinel `TypeId` values.

### Module, package, import and binding contract

Terminology is strict:

- **Module**: one directory-scoped compilation and visibility unit rooted by `#*.bst` or `+*.bst`
- **Package**: a named reusable `@...` import root and future dependency or distribution unit
- **Binding**: a typed bridge to an implementation outside Beanstalk source
- **Prelude**: implicit import policy, not a package kind
- **Library**: informal wording only

Packages are classified on independent axes:

```rust
enum PackageOrigin {
    Core,
    Standard,
    Builder,
    ProjectLocal,
    Dependency,
}

enum PackageBacking {
    BeanstalkSource,
    ExternalBinding,
}
```

Current mappings include:

| Package | Origin | Backing |
|---|---|---|
| `@html` | Builder | BeanstalkSource |
| `@core/collections`, `@core/io`, `@core/math`, `@core/text`, `@core/random`, `@core/time` | Core | ExternalBinding |
| `@web/canvas` | Builder | ExternalBinding |
| scoped `+*.bst` package | ProjectLocal | BeanstalkSource |
| project-root package facade | ProjectLocal | BeanstalkSource |
| annotated project-local `.js` import | ProjectLocal | ExternalBinding |

`Standard` and `Dependency` remain valid origins even when no current package uses them.

#### Module roots

A directory contains at most one module root:

- `#*.bst` defines a normal module
- `+*.bst` defines an API-only support module that exposes a scoped package
- one optional project-root `+*.bst` beside `config.bst` defines the project's external package facade
- the suffix after `#` or `+` is cosmetic
- `config.bst` is not a module root

Every project source file belongs to its nearest containing module root.

A normal module may own dormant top-level runtime work and page fragments. A support module and project package facade are API-only: they have no implicit `start`, top-level runtime statements, page fragments, route or builder artifact. Functions and ordinary runtime code inside functions remain valid.

`export:` is the only public visibility marker for every root kind.

#### Canonical module graph

Stage 0 builds one canonical module graph and compiles each module once in dependency order.

A normal module may import:

- ordinary files and unrooted directories it owns
- direct child normal modules
- support packages visible in its lexical module scope
- registered Core, Builder and dependency packages
- builder-supported provider files with an explicit owner

A normal module may not import:

- its parent or any ancestor
- a normal sibling
- a grandchild directly
- a sibling's descendant
- an unrelated branch
- another module's private file path

A child module re-exports anything its parent should see from deeper descendants.

Valid project structure is acyclic by construction. The compiler still validates cycles defensively.

#### Scoped support packages

A `+*.bst` support root exposes a package named by its containing directory.

For support package `S` whose nearest ancestor normal module is `P`:

- `S` is visible to `P`
- `S` is visible to normal sibling modules and their descendants
- `S` is not visible above `P` or outside `P`'s subtree
- `S` is not imported from its own private implementation descendants
- another support package in the same owner scope cannot import `S`

The support facade may import:

- ordinary files it owns
- any descendant module in its private subtree
- support packages from a strictly outer scope
- registered packages

It may not import its parent, normal sibling consumers or same-scope support siblings.

Consumers see only the support facade's `export:` surface. They cannot address its private descendant modules.

#### Project package facade

A project may contain one `+*.bst` root beside `config.bst`.

- The facade defines the project's external Beanstalk package surface.
- Its package identity comes from the canonical project config `name`.
- It is not visible to internal project modules.
- It may assemble exported surfaces from any descendant module below `entry_root`.
- It receives only each descendant's public interface.
- It emits no route or runtime entry.
- A project can be both an application and a package.
- Without this facade the project has no externally consumable Beanstalk package surface.

#### Import roots

All project source imports resolve from the importing file's owning module root, not the file's physical directory.

- `@./...` has no supported meaning.
- `..` is always invalid.
- Paths may traverse ordinary unrooted directories owned by the same module.
- Reaching a child module or support package ends filesystem traversal and exposes only its facade.
- Import paths cannot bypass a facade with forms such as `@child/internal`.
- Scoped support packages are injected by package name.
- The project-root package facade is the sole assembly exception and resolves project paths from `entry_root`.
- Provider imports use an explicit owner and do not silently reintroduce file-relative resolution.

Header import preparation consumes the Stage 0 namespace. It does not probe ordered fallback candidates.

#### Namespace and collision policy

No import uses precedence, nearest-match shadowing or ordered fallback.

Reject overlapping visible identities between:

- support packages
- direct child modules
- extensionless source files
- internal directory path segments
- the project package name
- Core or Builder package roots
- dependency aliases
- case-only variants

Recognised extensionless source kinds share one namespace. `docs.bst`, `docs.bd`, `docs.md` and `docs/` cannot coexist where each would mean `@docs`.

Explicit-extension provider files may coexist with a same-stem directory only when syntax remains unambiguous.

The same support-package name may appear in disjoint scopes. Overlapping scopes are rejected with diagnostics pointing to both declarations.

#### Public interfaces

A compiled module exports an immutable semantic interface containing:

- exported declarations
- canonical type identities
- folded constant facts
- generic templates
- trait and conformance evidence
- receiver surfaces
- function access and effect summaries

Private declarations never receive consumer-visible identities.

Aliases affect source spelling, not semantic identity.

Receiver methods remain attached to their receiver type's exported source surface. They are not independently imported, aliased or re-exported.

#### Binding-backed packages

Binding-backed packages are virtual typed symbols, not Beanstalk modules. They expose opaque types, constants and free functions only.

All binding-backed packages use stable package and symbol IDs. Direct builder packages and provider-created packages share one identity model.

External package namespace imports may expose recursive package-local paths such as `io.input.*`. Source-module namespace records remain shallow and field-access-only.

External import providers resolve supported non-Beanstalk sources before AST. HIR carries stable external symbol IDs. Backends map those IDs to target helpers, imports, generated glue or native operations.

Binding-backed packages do not expose source receiver methods. Use source-owned wrapper types for method-style APIs.

The bare `io` name is prelude policy for `@core/io`, not a package category.

### Module root activity, entry assembly and page fragments

A normal module stores dormant root activity in its canonical compiled artifact. Compiling the module does not decide whether its root is active.

An `EntryAssembly` selects one normal module as the active entry and activates only that module's:

- implicit `start()`
- top-level runtime work
- runtime page fragments
- compile-time page fragments
- entry-owned runtime dependencies

Imported normal modules expose their public interfaces without executing root work. Support modules and project package facades never have root runtime activity.

Header parsing records normal-root top-level runtime code as a dormant `HeaderKind::StartFunction`. It is excluded from local declaration dependency sorting and emitted after sorted declarations.

Page fragments split before HIR:

- runtime templates remain runtime code inside dormant `start()`
- compile-time templates fold once into owned module artifact data
- each compile-time fragment records its runtime insertion index
- entry assembly merges compile-time and runtime fragments in source order
- HIR never carries compile-time fragments or document structure

Project builders use root activity and entry assemblies to decide artifact eligibility. API-only modules remain importable but do not produce routes, runtime glue or tracked assets.

### Top-level declaration shape

Header parsing owns top-level declaration discovery and declaration shell parsing. These headers participate in strict top-level dependency sorting.

Top-level declarations are owned by the module's immutable public or private semantic state. Cross-module consumers use the public interface rather than merging provider headers into their own declaration graph.

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

Top-level constants, type aliases, structs, choices, function signatures and relevant type annotations can create header-provided dependency edges. Executable body references do not.

Binding-mode syntax, constant rules, module visibility and top-level template syntax are specified in [`language-overview.md`](language-overview.md).

## Pipeline stages

The compiler and build system process a project through these stages:

0. **Project structure**: parse config, build one canonical source index, assign file ownership, discover normal and support roots, validate package scopes, build the acyclic module graph and establish deterministic compile order.
1. **Tokenization**: convert Beanstalk source into located tokens using source-kind entry modes.
2. **Header parsing**: parse imports, root exports, declaration shells, dependency edges and dormant normal-root start metadata.
3. **Dependency sorting**: order declarations within each module from header-provided edges. Project module order is already owned by Stage 0.
4. **AST construction**: resolve module-local semantics, project imported canonical types, fold constants and templates, validate bodies and emit generic instance requests.
5. **HIR generation**: lower each module and generated function into explicit backend-facing control flow with stable cross-module call targets.
6. **Borrow validation**: validate each canonical module and generated function, producing side tables and public call-effect summaries.
7. **Project assembly and backend lowering**: build entry and package-facade link plans, then lower the compiled project graph into target artifacts.

## Stage 0: Project structure

Stage 0 converts project files and builder metadata into one canonical module and package graph.

It owns:

- parsing `config.bst` through AST and extracting known folded config constants
- validating `entry_root` as a relative directory strictly below the project root
- building one canonical source-tree index
- discovering normal `#*.bst` roots, support `+*.bst` roots and the optional project package facade
- assigning every source file to its nearest module owner
- classifying root roles once
- discovering builder-supported source assets and provider imports
- establishing extensionless import namespace identities
- validating file, directory, module and package collisions
- computing direct child-module relationships by nearest-module ancestry
- computing support-package visibility scopes
- rejecting illegal structural dependencies before semantic compilation
- building the acyclic project module graph
- assigning deterministic module and semantic identities
- producing dependency-order compile waves
- recording source identities for diagnostics

Directory-project `entry_root` rejects:

- an empty path
- `.`
- parent components
- absolute paths
- paths outside the project root
- symlink-resolved equality with the project root

Single-file compilation remains an explicit synthetic-module mode.

Stage 0 imports are resolved from the importing file's module owner. The path resolver consumes the prepared graph and namespace. It does not scan parents or try precedence-based fallback candidates.

Builder-supported `.bd` and `.md` assets participate in the same extensionless namespace as `.bst` source. Provider-backed explicit-extension imports retain their registered provider contract and explicit owner.

Project-local source packages are structural `+` packages or the project-root facade. `package_folders` and default `/lib` scanning do not exist.

Builder source packages use the same canonical public-interface and compiled-artifact model as project source modules. Binding-backed packages remain registry metadata.

Stage 0 enforces the build-boundary deterministic aggregation contract above while scheduling dependency-respecting module waves.

Stage 0 produces structure and inputs. It does not type-check executable bodies, generate HIR or perform borrow validation.

## Stage 1: Tokenization

Path: [`src/compiler_frontend/tokenizer/lexer.rs`](../src/compiler_frontend/tokenizer/lexer.rs)

Tokenization converts source text into structured tokens with source locations. It owns:

- basic lexical recognition
- source location tracking
- string and template delimiter context
- numeric literal scanning and source-location diagnostics. The tokenizer consumes literal text, classifies attached negative literals and reports spacing-sensitive syntax errors. [`numeric_text/`](../src/compiler_frontend/numeric_text/) owns shared numeric grammar, normalisation, separator/exponent validation and materialisation helpers used by later semantic consumers.
- symbolic binary-operator spacing and unary-negation spacing diagnostics
- style directive token recognition through the merged directive registry
- syntax-level rejection of unsupported or unknown directive forms where applicable

`TokenizerEntryMode` chooses the initial lexical state for a source file kind. Normal `.bst` files start in ordinary code mode. Beandown `.bd` files start inside an implicit template body, which lets the tokenizer preserve original Beandown source locations while rejecting an unescaped outer `]`. Plain Markdown `.md` has no tokenizer entry mode and is prepared before tokenization. `TokenizeMode` remains the internal lexical stack state used while scanning nested templates.

## Stage 2: Header Parsing

Path: [`src/compiler_frontend/headers/parse_file_headers.rs`](../src/compiler_frontend/headers/parse_file_headers.rs)

Header parsing is the only stage that discovers module-wide top-level declarations. It parses top-level declaration shells so later stages do not reconstruct them from raw tokens.

Header parsing owns:

- import and public re-export syntax
- root-role-aware `export:` parsing
- import binding against the Stage 0 namespace
- file-local visibility construction
- declaration shells for constants, functions, structs, choices, aliases, traits and conformances
- local declaration dependency edges
- dormant normal-root start-body separation
- compile-time fragment placement metadata
- source-kind adapters that synthesise ordinary declarations

Support roots and project package facades reject root runtime activity before AST. Normal roots retain dormant start and fragment metadata for entry assembly.

Imported module and package references resolve to stable public-interface identities. Header parsing does not copy provider declarations into the consumer or bypass a facade to reach private files.

Header dependency edges include every top-level declaration dependency needed before AST can resolve declarations linearly:

- imported declaration references
- type alias targets
- struct and choice field type annotations
- function parameter and return type annotations
- constant explicit type annotations
- fixed collection capacity references in type annotations when the capacity is a visible compile-time constant
- constant initializer references to other constants
- structurally exposed const-template condition/control references when header parsing can identify them without parsing full template body semantics

Header parsing does not type-check executable bodies or fold expressions. It should prefer storing normalised, validated path/reference forms instead of raw import/path syntax where enough context exists for later stages to consume.

Declaration-shell parsers are shared with AST body-local declaration parsing so top-level and body-local declaration syntax stays equivalent. Header parsing records parsed type-reference shells and dependency edges. AST owns resolving those shells into canonical `TypeId`s.

Header import preparation consumes the Stage 0 module graph and namespace, then builds the file-local visibility used by dependency sorting and AST. It resolves module interfaces, support packages, source assets, binding-backed packages, aliases, prelude names and collision rules without probing filesystem fallbacks.

Constants are compile-time declarations. Header parsing records symbol-shaped references found in constant initializer tokens and resolves them far enough to create dependency edges to other constants.

Executable function/start body references do not participate in dependency sorting. Body-local declarations do not participate in dependency sorting. The dormant normal-root start header is always appended last.

Beandown header preparation lives in [`headers/beandown_prepare.rs`](../src/compiler_frontend/headers/beandown_prepare.rs). A `.bd` input contributes one private synthetic constant declaration, `content #String`, whose initializer is a structurally built `$md` template over the original `.bd` body tokens. During AST template parsing, that Beandown source-kind context also defaults nested templates with no explicit directive to the Markdown formatter. Any explicit nested template directive overrides the Beandown default. Plain Markdown preparation lives in [`headers/plain_markdown_prepare.rs`](../src/compiler_frontend/headers/plain_markdown_prepare.rs). A `.md` input renders the raw Markdown to HTML and contributes the same private `content #String` declaration shape with a synthetic string-literal initializer. Later dependency sorting and AST folding treat both declarations like any other compile-time constant. There is no Beandown- or Markdown-specific AST node, HIR path, borrow-checker path or backend path.

Project builds prepare source files through one deterministic Stage 1/2 scheduling path. Tiny
modules stay serial, medium modules cross to per-file Rayon only when source bytes justify the
overhead and larger modules use chunked Rayon scheduling. Every strategy satisfies the
deterministic file-preparation merge contract above. `RAYON_NUM_THREADS` remains the external
concurrency control. The compiler does not create a custom frontend Rayon pool.

User-facing Beandown authoring and import rules are in [`src/docs/beandown/#page.bst`](src/docs/beandown/#page.bst).
User-facing plain Markdown authoring and import rules are in [`src/docs/markdown/#page.bst`](src/docs/markdown/#page.bst).

### Declaration shells

A declaration shell is a structured top-level header payload, not a fully resolved AST node.

Examples:

- constant shell: name, export flag, explicit type annotation, initializer token span/tokens, initializer reference hints and source order
- function shell: name, generic parameters, parsed signature and body tokens
- struct shell: name, generic parameters, parsed field names/types and default token data where applicable
- choice shell: name, generic parameters, variant names and payload field type shells
- type alias shell: name and target type annotation. Parameterised generic aliases are rejected before shell creation.
- trait shell: name, requirement signature shells and requirement type-reference dependency edges
- conformance shell: target type reference, trait references and declaration source context
- start shell: dormant normal-root executable token body, excluded from dependency sorting

### Header and AST ownership boundary

Header parsing owns top-level discovery and declaration shell parsing. AST must not rediscover top-level symbols or reconstruct top-level declaration shells from raw tokens.

Header parsing builds `ModuleSymbols`, the order-independent top-level symbol, import, export, builtin, type-alias and source-file metadata package. Dependency sorting finalises the sorted declaration list. AST consumes that package directly.

## Stage 3: Dependency sorting

Stage 0 orders modules in the project graph. Stage 3 orders top-level declarations inside one canonical module.

It owns:

- topological sorting of local declaration shells
- cycle detection in the local declaration graph
- source-order stability among independent declarations
- constant initializer dependency ordering
- finalising the module's declaration order
- appending builtin declarations
- appending the dormant normal-root start header after declarations

It does not:

- order project modules
- copy imported module declarations into the local graph
- inspect executable function or start-body references
- order body-local declarations
- rediscover imports

Cross-module dependencies are satisfied by compiled immutable public interfaces. A provider module is compiled before its consumers according to the Stage 0 graph.

Same-file constants retain source-order semantics. Same-file forward references are rejected. Cross-file constants inside one module use header-provided edges. Cross-module exported constants are already folded owned facts in the provider interface.

A source-backed Builder package is compiled through the same module-interface model. Consumers do not treat its private headers as local graph nodes.

After dependency sorting:

- AST consumes declarations linearly
- AST does not rebuild import visibility
- AST may register nominal identities before resolving members
- any missing local ordering edge is fixed in header parsing
- any project dependency belongs in the Stage 0 module graph
- dormant `start` is never a dependency participant

## Stage 4: AST Construction

Path: [`src/compiler_frontend/ast/mod.rs`](../src/compiler_frontend/ast/mod.rs)

AST consumes already-sorted declaration headers and the header-built module environment. It resolves declarations in order, folds constants/templates, parses executable bodies, type-checks expressions, validates function terminality and emits typed AST nodes.

Internally, AST construction is organised around three phase owners:

- [`build_ast_environment`](../src/compiler_frontend/ast/module_ast/environment/) consumes header-built file visibility, then resolves declaration metadata, constants, nominal types, function signatures, receiver catalogue data, trait metadata and shared environment side channels.
- [`emit_ast_nodes`](../src/compiler_frontend/ast/module_ast/emission/) parses function/start/template bodies against the completed environment, validates function terminality, emits AST nodes and emits const-template output.
- [`finalize_ast`](../src/compiler_frontend/ast/module_ast/finalization/) performs HIR-boundary cleanup, including doc fragment extraction, const top-level fragment assembly, reactive template metadata propagation, template normalisation, module constant normalisation, type-boundary validation, const-fact collection, concrete choice-definition gathering, builtin AST merging and final `Ast` construction.

### Frontend arenas and capacity policy

Frontend arenas are stage/module-owned implementation details. They provide stable IDs and reduce
clone/allocation pressure inside the owning stage, but they do not change source semantics,
diagnostics, declaration ordering, HIR shape or backend artifacts.

Token/header statistics and `FrontendArenaCapacityEstimate` produce conservative `Vec` capacity
seeds. These estimates are policy-only: undersized estimates grow normally, oversized estimates
only reserve bounded extra capacity and capacity formulas must remain centralised in the frontend
arena policy modules.

The scope-frame arena is AST-owned. It replaces cloned body-local scope maps with parent-linked
frames while continuing to consume header-built visibility through `ScopeContext`. `StringTable`
remains the path and string identity system and AST/HIR ownership boundaries remain unchanged.

### AST parallelism and determinism

AST body emission remains serial within one module. `AstEmitter` consumes mutable module-local semantic state, emits warnings and rendered paths and records generic instance requests.

Independent modules may compile in dependency-respecting parallel waves. Their public interfaces are immutable before consumers begin.

Generic requests are worker-local outputs. The build system merges them deterministically by stable generic declaration identity and canonical type arguments into the project-wide materialisation worklist.

Wrapping mutable AST state in locks does not make body emission safely parallel. Parallel body emission requires immutable lookup snapshots, worker-local diagnostics and requests plus deterministic merge ownership.

HIR generation, borrow validation and backend lowering are not parallelised as incidental follow-up work. Each stage needs its own ownership and deterministic merge design.

Important AST subowners:

- [`type_resolution/`](../src/compiler_frontend/ast/type_resolution/) owns parsed type-reference resolution to canonical `TypeId`, including source-visible lookup, aliases, fixed collection capacity, maps and generic nominal instantiation.
- [`module_ast/environment/public_surface.rs`](../src/compiler_frontend/ast/module_ast/environment/public_surface.rs) owns semantic module public API validation after type and trait identities are resolved.
- [`generic_functions/`](../src/compiler_frontend/ast/generic_functions/) owns generic free-function templates, call inference and concrete instance request emission before project-wide materialisation.
- [`generic_bounds.rs`](../src/compiler_frontend/ast/generic_bounds.rs) owns static trait-bound validation for concrete nominal generic instances.
- [`templates/`](../src/compiler_frontend/ast/templates/) owns template parsing, composition, folding, slot routing, runtime slot plan preparation, control-flow validation and template structural metadata traversal.
- [`templates/tir/`](../src/compiler_frontend/ast/templates/tir/) owns the AST-local Template IR store. [`runtime_handoff.rs`](../src/compiler_frontend/ast/templates/runtime_handoff.rs) owns its neutral HIR handoff payloads.
- [`module_ast/finalization/const_fact_collection.rs`](../src/compiler_frontend/ast/module_ast/finalization/const_fact_collection.rs) owns explicit module, private top-level and body-local const-fact collection after AST finalisation.
- [`builtins/casts/`](../src/compiler_frontend/builtins/casts/) owns builtin cast target classification, evidence, policies and core cast-trait metadata. [`builtins/casts/resolution.rs`](../src/compiler_frontend/builtins/casts/resolution.rs) owns AST cast resolver wiring at explicit typed boundaries.
- [`field_access/`](../src/compiler_frontend/ast/field_access/) owns source fields, receiver calls and compiler-owned collection/map builtin member access.

AST owns:

- module-local semantic declaration resolution
- imported canonical type projection into local `TypeId` handles
- public interface validation and canonical export projection
- executable body parsing and type checking
- body-local declarations
- function terminality validation
- contextual coercion at explicit receiving boundaries
- generic template validation and concrete request emission
- trait, conformance and generic-bound evidence validation
- explicit cast evidence resolution and builtin folding
- constant and const-record folding
- exported folded constant facts
- template composition, slot routing, folding and runtime handoff preparation
- reactive source and subscription metadata
- module-local TIR from parser emission through finalisation

AST should be described by this ownership and data-flow contract, not by a fixed internal pass count. The internal substeps inside each phase are implementation details and may change as the stage is simplified.

The direct HTML-project Beandown API uses the same tokenizer, synthetic-header preparation, dependency sorting and AST folding path as compiler-integrated `.bd` imports, then extracts the folded `content` constant. It deliberately stops before HIR generation, borrow validation, backend lowering, artifact writing and output cleanup.

### Generics contract

The declaring module owns and validates each immutable generic template.

Consumers infer concrete arguments from immediate call arguments and immediate expected result context, then emit requests keyed by:

- stable generic declaration identity
- canonical concrete type identities
- required visible trait evidence

The build system owns a deterministic project-wide worklist. It deduplicates requests, materialises concrete functions into generated-artifact sidecars and continues until no generated function requests another instance.

Invalid generic templates are diagnosed when the declaring module compiles. Inference failures, missing evidence and invalid concrete substitutions are diagnosed at the requesting call site with declaration context where useful.

Generated executable functions are lowered to concrete HIR and borrow-validated independently. Base module artifacts remain immutable.

HIR and backends receive only concrete executable targets. They never solve generic arguments or consume unresolved generic template state.

### Traits contract

Trait declarations and conformances are compile-time frontend metadata.

Header parsing records trait and conformance shells. AST owns semantic trait identity, requirement type resolution, explicit conformance validation, evidence visibility, generic-bound checks and bound-provided receiver-call resolution.

Exported traits and reusable conformance evidence use stable module semantic identities. Consumers do not reconstruct conformance structurally.

Receiver methods remain tied to the receiver type's exported source surface. Methods are not independently imported, aliased or re-exported.

Traits are not value types. Trait names are valid only in trait declarations, conformance declarations and generic bounds.

Static bound calls resolve to concrete executable targets before HIR. HIR and backends do not carry trait objects, erased dispatch or trait evidence for runtime dispatch.

### Imports and visibility

AST consumes the header-built file visibility environment through `ScopeContext`. It may validate semantic use of visible symbols, but it must not rebuild import bindings or rediscover top-level visibility.

All user-visible names go through one collision policy. Same-file declarations, source imports, external imports, type aliases, prelude symbols and builtins cannot silently shadow each other.

External expression and type resolution must go through the active `ScopeContext` visibility lookup. If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST pass.

### Type checking and coercion

Expression evaluation determines the natural type of an expression and stays strict. Contextual coercion is applied only by the frontend site that owns the boundary.

AST emission should carry canonical `TypeId`s through field access, receiver lookup, builtin receiver validation, call validation, operator result typing and compatibility checks. `DataType` remains parse-only or diagnostic spelling once a semantic `TypeId` exists.

Examples of boundary owners:

- declarations and assignments
- returns
- concrete function parameters
- struct and choice fields
- default values
- typed collection and map entries
- template and string content
- explicit `cast` target boundaries
- `then` arms whose enclosing value-producing block has an explicit receiver
- backend and prelude call contracts

Detailed numeric rules, match syntax, cast syntax and string coercion rules belong in [`language-overview.md`](language-overview.md).

### Value-producing blocks and terminality

Value-producing `if`, match and block-form `catch` are closed receiving constructs, not general expressions.

They are valid only where the receiver is explicit, including declarations, assignments, multi-bind, returns and nested `then`. Every producing path must satisfy the receiver arity.

AST owns user-facing receiving-context, arity and terminality diagnostics. Non-unit success returns must be terminal before HIR lowering.

If HIR receives a non-unit function that can fall through, the AST contract was violated and HIR reports an internal transformation error.

### Constants and folding

Constants are compile-time declarations and module metadata, not runtime top-level statements.

Header parsing records initializer references for dependency ordering. AST owns semantic checking and folding.

A module folds its constants and const templates once. Exported folded facts are copied into the immutable public interface as owned backend-neutral values. Consumers do not parse or fold provider templates again.

Private inferred const facts are advisory optimisation metadata. They do not affect semantics, dependency sorting or visibility.

Fully folded struct constants may become const records. Const records are compile-time field-access-only groups. They are not runtime values and cannot be passed, returned, stored or used through runtime methods.

Compile-time and runtime semantics must agree:

- checked numeric failure rules match
- cast range and non-finite checks match
- Float formatting matches
- template interpolation output does not depend on the backend

### Templates

AST owns all template semantics.

TIR is the single AST-local structural authority from parser emission through composition, formatting, folding and finalisation. `Template` is a thin handle into the module-scoped TIR registry while AST construction is active.

AST owns:

- parsing template bodies and emitting them into TIR
- composing slots, inserts, wrappers and child templates in TIR
- folding fully constant templates into string literals
- preserving structured template `if` and `loop` bodies for runtime lazy lowering
- preparing runtime slot source/site plans after AST-owned schema extraction and contribution routing
- validating const-required template control flow before HIR
- rejecting escaped slot/insert helper artifacts that are invalid after composition/routing
- preserving runtime templates as runtime expressions
- replacing runtime templates with owned runtime handoff payloads before HIR
- removing helper-only template artifacts before HIR
- emitting builder-facing const top-level fragment metadata
- exporting only folded owned const-template facts through module interfaces

AST finalisation folds const templates or replaces runtime templates with neutral owned handoff payloads. The TIR registry and stores are dropped before the completed AST leaves the stage. No TIR reference, store, view, overlay or registry value crosses a module interface or enters HIR.

HIR only lowers finalised runtime templates that remain after AST folding. Runtime template control flow lowers inline as ordinary HIR branches, loops, accumulator appends and AST-prepared runtime slot source/site plans in the enclosing function, not as backend-specific template control-flow nodes. HIR consumes AST-prepared slot source/site plans and owned runtime handoff payloads only. It does not parse directives, validate slot schemas or reconstruct TIR.

User-facing template syntax, directives, markdown behaviour and slots are in [`src/docs/templates/#page.bst`](src/docs/templates/#page.bst).

### Reactivity V1

Reactivity V1 is frontend-owned source and template metadata that later stages preserve for backend feature validation and backend lowering. It must not become a second type system or a general closure/function-value model.

Stage ownership:

- Declaration syntax parses `$Type`, `$=` and `$T` parameter access markers as syntax only.
- AST resolves the underlying ordinary `TypeId`, assigns reactive source identity, validates `$(source)` template subscriptions and preserves reactive template string metadata.
- HIR carries backend-facing reactive source/template metadata and reachability facts without reparsing template directives or becoming a backend render-plan language. HIR consumes finalised runtime template metadata from the AST stage. It does not parse template directives, slot schemas or TIR nodes.
- Borrow validation treats subscriptions as read-only source dependencies, not active borrow lifetimes, while ordinary mutations continue to follow existing mutable/exclusive rules.
- Backend feature validation applies the selected target contract before lowering. Runtime reactive behaviour remains backend-owned artifact policy.

## Stage 5: HIR generation

Path: [`src/compiler_frontend/hir/`](../src/compiler_frontend/hir/)

HIR lowers fully typed module AST and generated concrete functions into the first backend-facing semantic IR.

Each module retains local HIR IDs and its paired local `TypeEnvironment`. Cross-module executable references use stable project targets such as a module-function identity. The callee body is not copied into the caller.

HIR makes control flow, locals, regions, calls and terminators explicit. Pure value construction may remain nested while effectful work is linearised into statements and temporary locals.

HIR owns:

- explicit local control flow
- locals, places, regions and terminators
- stable local and cross-module call targets
- concrete generated-function targets
- expression side-effect linearisation
- runtime template string construction
- template control flow as ordinary CFG
- runtime slot accumulators and appends
- map operations
- checked numeric operations
- runtime casts
- Float formatting and validation
- reactive metadata
- module constants and advisory private const facts
- function-origin metadata
- backend-neutral reachability facts
- stable external package call IDs

HIR does not:

- merge provider module bodies into consumers
- carry donor-local type or function indexes across modules
- fold constants or templates
- reconstruct slot or render plans
- carry TIR
- carry compile-time page fragments
- solve generic arguments
- decide trait conformance
- carry runtime trait evidence
- decide final runtime ownership
- model exact lifetimes
- assemble routes or project artifacts

Plain `HirBinOp` remains valid for booleans, comparisons and string concatenation. Runtime scalar arithmetic and unary negation must lower through `HirStatementKind::NumericOp`. HIR validation rejects regressions where numeric arithmetic survives as ordinary expression ops.

HIR lowering treats user-facing source errors as already diagnosed by AST or earlier stages. HIR lowering and HIR validation use `CompilerError` for internal transformation invariants. If a non-unit function can fall through after AST terminality validation, that is a compiler invariant breach.

### HIR validation

[`src/compiler_frontend/hir/validation.rs`](../src/compiler_frontend/hir/validation.rs) and [`src/compiler_frontend/hir/validation/`](../src/compiler_frontend/hir/validation/) validate the freshly lowered module before it leaves Stage 5. Borrow validation and backend feature validation should receive already-coherent HIR, not defensively repair it.

HIR validation checks definition IDs, frontend `TypeId` links, region graph shape, start-function and function-origin metadata, CFG ownership, doc fragments, module constants, reactive metadata, side-table mappings, local/place references, terminators, patterns and expression invariants.

Important examples:

- Plain arithmetic `HirBinOp` and `HirUnaryOp::Neg` that should have been lowered to checked `NumericOp` statements are rejected as HIR shape errors.
- `HirExpressionKind::Float` values must be finite `f64`. `NaN` and `Infinity` literals are rejected as internal invariant breaches.
- Control-flow structure, block terminators, local references, side-table mappings and expression shapes are validated for borrow validation and backend consumption.

Validation failures are `CompilerError` with `ErrorType::HirTransformation`, not user-facing `CompilerDiagnostic`, because they represent compiler-internal lowering invariants.

### Reachable backend features

[`src/compiler_frontend/hir/reachability.rs`](../src/compiler_frontend/hir/reachability.rs) records syntactic reachability for functions, blocks, external calls, map literals/operations, reactive template-backed values, reactive sinks, runtime casts, checked numeric operations and Float formatting/validation statements. It does not fold constants, eliminate dead branches, inspect borrow facts or perform backend lowering.

Some target-gated checks need more than backend-neutral reachability. For example, generic runtime value validation scans reachable blocks with the module `TypeEnvironment` because generic-instance detection is semantic type analysis, not a raw HIR reachability fact.

### Call targets

Source calls use one of three explicit target classes:

- module-local function target
- stable cross-module function target
- stable binding-backed external function ID

Cross-module targets resolve through the compiled project graph and entry or package link plan. HIR does not store import aliases, package source syntax or backend runtime names.

Borrow validation resolves source function targets to exported access and effect summaries. Backends resolve source and external targets to generated functions, linked module functions, imports, glue or target-native operations.

The HTML builder and backend validators use HIR reachability for runtime artifact planning and target-contract validation. This is syntactic CFG/function reachability, not constant-condition dead-code elimination, optimisation or ownership analysis.

### Mutable rvalues

Fresh rvalues passed to mutable (`~T`) call slots are materialised into compiler-introduced hidden locals before borrow validation. Borrow validation then sees ordinary local access, not a special temporary node kind.

## Stage 6: Borrow validation

Borrow validation runs once for each canonical module and once for each generated concrete function.

It enforces:

- shared and exclusive access rules
- use-after-consumption safety
- conservative aliasing for collections and maps
- legal mutable call access
- control-flow joins
- inferred move safety
- reactive invalidation facts

It reads validated HIR and writes read-only side tables. It does not rewrite HIR, compute exact lifetimes or decide final runtime ownership.

Public function interfaces export the facts consumers need:

- parameter access modes
- mutation effects
- possible ownership consumption
- return aliasing
- relevant reactive effects

Cross-module call transfer consumes these summaries. It never opens the callee's HIR as local control flow.

Missing or inconsistent exported summaries are `CompilerError` invariant failures.

GC-only backends may ignore ownership optimisation facts but cannot skip borrow validation. GC and ownership-aware lowering accept and reject the same programs.

Reactive subscriptions are read-only source dependencies, not active borrow lifetimes.

Borrow facts remain keyed by module-local HIR identity. Exported summaries use stable module function identity.

## Stage 7: Backend lowering

Backend lowering belongs to project builders after frontend compilation.

### Navigation

- [`src/backends/js/`](../src/backends/js/) owns HIR-to-JavaScript lowering and JS runtime helper emission.
- [`src/backends/wasm/`](../src/backends/wasm/) owns HIR-to-Wasm-LIR lowering, Wasm runtime contracts, request validation, debug output and optional binary emission.
- [`src/projects/html_project/html_project_builder.rs`](../src/projects/html_project/html_project_builder.rs) owns the HTML `BackendBuilder` implementation and route-level artifact assembly.
- [`src/projects/html_project/js_path.rs`](../src/projects/html_project/js_path.rs) owns the HTML JS page-bundle path.
- [`src/projects/html_project/wasm/`](../src/projects/html_project/wasm/) owns HTML-Wasm export planning, bootstrap JS and artifact assembly around the core Wasm backend.
- [`src/projects/html_project/external_js/`](../src/projects/html_project/external_js/) owns provider-backed JavaScript imports, runtime module planning, generated glue, import maps and runtime asset emission.
- [`src/projects/html_project/tracked_assets.rs`](../src/projects/html_project/tracked_assets.rs) owns HTML tracked-asset planning and passthrough emission.

### Backend handoff

Backend builders consume `ProjectCompilation`, containing:

- the canonical module graph
- immutable compiled module artifacts
- generated concrete-function sidecars
- entry assemblies
- the optional project package facade assembly
- canonical public interfaces
- module-local HIR and type environments
- borrow facts and exported function-effect summaries
- diagnostics and warnings
- root activity and page-fragment metadata
- backend-neutral runtime dependency facts
- binding-backed package metadata

Backend lowering has three boundaries:

- The build system owns compilation scheduling, link-plan construction and output writing.
- Project builders own artifact policy such as routes, entry selection, fragment assembly and tracked assets.
- Backend lowerers own target code generation from validated HIR and explicit link plans.

Backends do not rediscover source imports, module topology, generic templates, trait evidence, config syntax or template syntax.

Language-owned HIR operations lower through backend helpers or target-native operations that preserve Beanstalk semantics.

Ownership-aware backends may consume borrow facts for optimisation. GC-only paths preserve the same source behaviour without deterministic destruction.

### Build-system output writing

Backends and project builders produce `OutputFile` records. They should not write final project outputs directly. The build system owns output-root validation, skip-unchanged writes, manifest tracking and stale artifact cleanup through `write_project_outputs`.

### HTML entry and fragment assembly

The HTML project builder consumes entry assemblies rather than treating every compiled module as an independent page.

For each artifact-producing entry it:

1. Selects the active normal module.
2. Activates that module's dormant `start` and root fragment metadata.
3. Merges compile-time fragments at their recorded runtime insertion indexes.
4. Creates runtime fragment slots.
5. Executes active `start` once through the selected runtime path.
6. Hydrates runtime fragments in source order.
7. Assembles route HTML and companion artifacts.

Imported normal modules, support packages and the project package facade never execute root work.

Modules without HTML artifact activity remain available to the graph but are excluded from route, runtime-glue and tracked-asset planning.

HIR carries runtime code only. Entry assemblies and the HTML builder own document and route semantics.

### External package and backend feature validation

Backend validation runs before target lowering. External package validation checks reachable external calls against the selected target's lowering metadata. Backend feature validation checks target-gated HIR features through explicit reachability roots and returns structured diagnostics for user-visible target-contract violations. Backend lowerers should receive only HIR features and external calls that their selected target contract can lower.

Backend feature validation does not hard-code one execution root. The project builder supplies roots from the entry or package link plan, including active start, reachable linked module functions, generated functions and explicit exported surfaces.

### JS lowering

The HTML-JS path lowers HIR through the JS backend, then uses the shared fragment contract: const fragments render into the document, runtime fragment slots are emitted, the generated JS bundle is embedded or module-loaded, the active entry's `start` is called once and returned runtime fragments hydrate the slots in source order. The HTML page-bundle path emits only the entry link plan's reachable concrete function set and uses the JS backend's referenced external-function metadata to generate only the glue wrappers that the emitted bundle calls.

The direct JS backend path can emit a complete standalone JS bundle when configured to include every HIR function. The HTML page-bundle path selects the reachable subset needed for the route artifact.

HTML-JS reactive runtime fragments are a separate, JS-only concern. Ordinary runtime page-fragment assembly is shared with HTML-Wasm. Reactive mounting is not.

### Wasm lowering

The core Wasm backend owns HIR-to-Wasm-LIR lowering, Wasm runtime contracts, request validation, optional binary emission and backend debug output. It consumes explicit linked module and generated-function targets. The HTML-Wasm path is builder orchestration around that backend. It chooses exported functions, requests helper exports, invokes Wasm lowering, generates bootstrap JS and assembles route HTML, JS and Wasm files using the same shared fragment contract as HTML-JS.

### External JS runtime assets and glue

Provider-backed external JS has two emission levels:

- Build-level runtime emission deduplicates JS runtime assets and required runtime module specifiers across the linked project compilation.
- Module-level glue generation inspects external functions referenced by the emitted JS bundle and emits only the wrapper module, import preamble and import-map entries needed for that page bundle.

### Tracked assets

Tracked assets are a builder policy over frontend-rendered path usages. The frontend records semantic path facts while rendering paths. The HTML builder decides which file paths become emitted assets, chooses output paths relative to the final page route, reports asset warnings/conflicts and returns asset bytes as ordinary `OutputFile` artifacts.
