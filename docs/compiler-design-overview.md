# Beanstalk Compiler Design Overview

Beanstalk is a high-level language with first-class string templates. Its compiler is a staged, backend-neutral library used by the project tool, the development server and the backend builders.

This document is the single source of truth for accepted compiler architecture and cross-stage contracts. It describes the end state the compiler and backends implement, including contracts the progress matrix still reports as incomplete. It is not an implementation-status report.

Companion authorities:

- `docs/language-overview.md` for source syntax, language semantics and language-scope decisions
- `docs/src/docs/codebase/design-scope/overview.bd` for design bias and scope boundaries
- `docs/src/docs/codebase/memory-management/overview.bd` for access, borrow, GC, ownership and destruction semantics
- `docs/src/docs/codebase/style-guide/style-guide.bd` for implementation standards
- `docs/src/docs/progress/#page.bst` for current support and backend coverage
- `docs/roadmap/roadmap.md` and `docs/roadmap/plans/` for sequencing, active work and genuinely deferred design

User-facing pages under `docs/src/docs/**` teach the language. They do not replace this compiler architecture reference.

## Architectural invariants

- One directory-scoped `#*.bst` or `+*.bst` module is the canonical semantic compilation unit. A physical module is compiled once per project or package build and owns local type, HIR and borrow identity.
- Every normal module's dormant root code is parsed, type-checked, lowered to HIR and borrow-validated even when no current entry activates it.
- Stage 0 owns one canonical graph, file ownership, legal topology and deterministic scheduling.
- Tokenization and header parsing produce reusable source metadata once. Later stages do not reparse it.
- Module interfaces use stable semantic identities, not donor-local indexes.
- AST resolves constants, generics, traits, casts and templates before executable HIR reaches a backend.
- TIR is AST-local. HIR receives only folded strings or neutral owned runtime data.
- HIR is the first backend-facing semantic IR. Borrow validation reads it and writes side tables.
- Backends consume compiled graphs and explicit link plans. They do not rediscover source structure.
- GC is the semantic baseline. Ownership-aware lowering preserves the same source behaviour.
- Parallelism, reuse and caching must preserve deterministic identities, diagnostics and output order.

## Project compilation model

The compiler produces one immutable project payload from a canonical module graph. Conceptual shape only. Exact names may change.

```rust
pub struct ProjectCompilation {
    pub structure: ProjectModuleGraph,
    pub project_globals: ProjectGlobalsInterface,
    pub modules: Vec<CompiledModuleArtifact>,
    pub generated: Vec<ModuleGeneratedArtifacts>,
    pub entries: Vec<EntryAssembly>,
    pub package_facade: Option<ProjectPackageAssembly>,
}

pub enum ModuleCompilationResult {
    Success(CompiledModuleArtifact),
    Failed(ModuleDiagnostics),
}
```

Boundaries that may not change:

- successful artefacts are immutable
- failed results expose no partial interface
- generated sidecars are separate from base module artefacts
- entry assemblies are many-to-one with modules
- the project package facade compiles as an ordinary API-only module, and `ProjectPackageAssembly` is a separate assembly plan over its compiled artefact
- backends receive the graph and link plans

### Builder and tooling surfaces

- `build` and `dev` compile the union reachable from builder-selected artefact entries and the project package facade.
- `check` compiles every discovered module below `entry_root`.
- `check` also applies selected-target validation to actual linkable roots without performing backend lowering or writing outputs.
- Target-validation roots are builder-selected entries, reachable generated functions, project package exports and any additional callable roots the selected builder declares.
- Unsupported target features in unreachable private functions do not fail a build or check.
- `check` and future LSP are tooling overlays over the selected target builder surface, not independent copies of target packages, directives and source kinds.
- One artefact builder runs per `build` or `dev` invocation.
- Builder-relevant root activity selects normal modules as entries. For HTML, root runtime work, page fragments and resolved active HTML entry config make a module an entry. Tooling-only `check` or `lsp` config never creates an artefact entry.
- Final builder-selection syntax and a possible Beanstalk-native build script system remain deferred. The current CLI selects HTML implicitly.

Builders declare the frontend-visible surface: source-backed packages, binding-backed packages, config keys, style directives, external import providers, builder runtime packages and supported source file kinds. Source and binding registries remain separate because their compiler and runtime needs differ. Shared `PackageMetadata` classifies both through independent `PackageOrigin` and `PackageBacking` axes.

External import providers convert supported non-Beanstalk files into typed binding-backed package surfaces before AST consumes visibility. Provider results may also record runtime imports and assets for later link planning. Builder-runtime packages such as `@web/canvas` use the same binding identity and runtime asset path as provider-created packages.

Builder-supported source file kinds participate in extensionless source discovery without becoming modules. Beandown `.bd` and Markdown `.md` inputs become ordinary synthetic compile-time declarations during header preparation.

`config.bst` authored config entries are top-level compile-time constants declared with `name #= value` or `name #Type = value`. Config permits the accepted constant and anonymous const-record surface only. It contains no source imports, runtime declarations, mutable bindings, functions, traits, conformances, standalone templates, page fragments or module exports.

Style directive contract:

- Frontend-owned directives are always available. Builder directives cannot override frontend-owned names.
- Tokenizer and template parsing use the same merged registry. Unknown directives are rejected strictly.

Project-specific config validation remains in the active builder's schema validation. User config mistakes remain `CompilerDiagnostic` values while infrastructure failures remain `CompilerError` values. Complex release optimisation stays outside the fast frontend path unless correctness requires it.

### Project config and @project

`config.bst` is build-system-owned compile-time Beanstalk source, not a module. It emits no HIR, start function or runtime artefact. It contains one required open `project` const record, private top-level helper constants declared before values that use them and top-level builder and tooling section records.

`config.bst` is one self-contained compile-time source file. It cannot contain source imports or depend on another file, package or binding. Direct `#Import` fields inside `project` remain build-input contracts and do not perform source resolution. An authored `import` declaration is rejected before path resolution with a structured diagnostic. Config parsing operates on exactly one authored source identity. Config bootstrap does not construct a package resolver, config import graph or config source set. Config uses ordinary tokenization, local declaration ordering, semantic checking and constant folding for its one file.

Short shape:

```beanstalk
project #= |
    name = "beanstalk_docs",
    version #Import of String = "0.1.0",
    entry_root = "src",
|

html #= |
    dev_output = "dev",
    release_output = "release",
|
```

Contracts:

- `config.bst` does not select the project builder.
- `project.name` is required and provides stable project identity.
- Compiler-owned project fields are strictly validated. Additional fully folded project metadata is allowed: public project values may contain folded scalar values, optionals, nested anonymous const records, collections of supported folded values and folded templates represented as strings.
- Direct primitive or optional fields of `project` may declare `#Import` contracts. V1 `#Import` field types are `String`, `Int`, `Float`, `Bool`, `Char` and optional forms. Nested project fields do not declare `#Import` in V1. Nested project fields do not provide unqualified source input values.
- Project fields do not gain implicit sibling scope. A field initializer follows ordinary anonymous-record rules.
- Private helper constants provide reusable derived values used by later config values.
- The `project` record must be available before a builder or tooling section references it.
- Top-level records other than `project` are potential builder or tooling config sections.
- The active artefact builder section is required, even when empty. The `project` record does not select that builder.
- The active builder section is recursively schema-validated through declarative metadata: accepted fields, nested shapes, required or defaulted values, closed domains, project or entry scope and stable identities where useful. Unknown fields inside the active section are diagnostics.
- Inactive or unavailable builder sections are parsed, name-resolved and folded as ordinary compile-time records but are not schema-validated or retained in `ProjectCompilation`. Unknown top-level record names are therefore allowed as inactive builder or tooling sections.
- Duplicate section names and collisions with primitive constants are rejected.
- Builder sections cannot declare `#Import` fields. They consume already folded values from `project` and use backend-neutral folded values rather than builder-specific nominal types.
- Builder project settings and builder entry settings use strict, non-overlapping schemas. There is no `ProjectAndEntry` or equivalent shared-scope escape hatch. Project and entry values do not implicitly inherit, merge or override one another.
- `#Import` is constant-source syntax, not a source import and not a semantic wrapper type. Project-level `#Import` contracts are collected and validated before module AST construction.
- Direct imported fields inside `project` resolve before project settings are applied and before Stage 0 uses `entry_root`.
- The project-wide barrier validates all reachable source contracts before affected modules compile.
- A direct imported project field and every reachable same-name source `#Import` declaration form one strict contract. Matching requires the same semantic type, optionality, required or default state and folded default value. Different defaults are conflicting contracts.
- A fixed same-name project field is an authoritative provider for compatible source `#Import` declarations and blocks CLI override.
- CLI inputs use repeated `--input name=value` only. Unknown inputs are diagnosed after reachable config and source contracts are known.
- Project dependencies are recorded at field granularity.

`@project`:

- The folded project record produces a specialised immutable `ProjectGlobalsInterface` under the permanently reserved `@project` import root.
- The interface contains stable field identities, folded backend-neutral values, source locations, field-level fingerprints and project-context provenance. It contains no AST, HIR or runtime body.
- It is classified as project-local and Beanstalk-source-backed but is not discovered as a normal source package.
- `@project` exposes direct project fields as namespace members. It does not export another value named `project`.
- Normal modules and project-owned support packages may explicitly import `@project`. It is never implicitly injected into modules.
- No child module, support package, dependency alias, Core package or Builder package may claim `@project`. `@project` cannot be directly re-exported.
- Internal module or support-package exports may expose project-derived constants, but provenance is retained so the project package facade rejects any transitive dependency on `@project`.

Entry-local `config:` blocks:

- An entry `config:` block is root-only builder metadata, not an embedded independent `config.bst` compilation unit.
- It is valid only at the top level of a normal module root and at most once per root. It is invalid in normal files, support roots, project package facades, `export:`, executable bodies and `config.bst`.
- The block contains config section records only. Imports, aliases, support types, helper constants and `#Import` declarations live outside the block in the normal root file.
- The block uses the root file's ordinary compile-time visibility. It may reference imported constants, `@project`, same-file constants declared before it, source `#Import` constants, foldable local const-record types and selected-builder compile-time values available through normal module imports. Same-file forward references remain invalid.
- Its references participate in the module's ordinary header dependency metadata and AST constant folding. It creates no ordinary module symbol and no HIR representation.
- It cannot contain a `project` section or change project-level builder behaviour. It may contain active artefact-builder and tooling-overlay sections.
- Active builder entry fields are strictly schema-validated. Inactive sections are parsed and folded but not schema-validated. An entry block is optional. The active artefact-builder subsection inside it is also optional so tooling-only metadata remains possible.
- Every normal module's entry block is validated during canonical compilation, whether or not an entry assembly activates it. Only resolved settings for the active artefact builder contribute entry activity. Imported normal modules never apply their entry metadata to an importer.

### Module graph, packages and imports

Terminology is strict:

- Module: one directory-scoped compilation and visibility unit rooted by `#*.bst` or `+*.bst`
- Package: a named reusable `@...` import root and future dependency or distribution unit
- Binding: a typed bridge to an implementation outside Beanstalk source
- Prelude: implicit import policy, not a package kind
- Library: informal wording only

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

Current mappings:

- `@html`: Builder origin, BeanstalkSource backing
- `@core/collections`, `@core/io`, `@core/math`, `@core/text`, `@core/random`, `@core/time`: Core origin, ExternalBinding backing
- `@web/canvas`: Builder origin, ExternalBinding backing
- scoped `+*.bst` package: ProjectLocal origin, BeanstalkSource backing
- project-root package facade: ProjectLocal origin, BeanstalkSource backing
- annotated project-local `.js` import: ProjectLocal origin, ExternalBinding backing

`Standard` and `Dependency` remain valid origins even when no current package uses them.

Module roots:

- `#*.bst` defines a normal module. `+*.bst` defines an API-only support module that exposes a scoped package. One optional project-root `+*.bst` beside `config.bst` defines the external project package facade. The suffix after `#` or `+` is cosmetic. `config.bst` is not a module root.
- Every project source file belongs to its nearest containing module root.
- A normal module may own dormant top-level runtime work and page fragments. A support module and project package facade are API-only: they have no implicit `start`, top-level runtime statements, page fragments, route or builder artefact. Functions and ordinary runtime code inside functions remain valid.
- `export:` is the only public visibility marker for every root kind.

Source imports resolve from the importing file's owning module root, never the physical file directory. `@./...` and parent components are invalid. Paths may traverse ordinary unrooted directories owned by the same module. Reaching a child module or support package ends filesystem traversal and exposes only its facade. Import paths cannot bypass a facade with forms such as `@child/internal`. Scoped support packages are injected by package name. The project-root package facade is the sole assembly exception and resolves project paths from `entry_root`. Provider imports use an explicit owner and don't silently reintroduce file-relative resolution. Header import preparation consumes the Stage 0 namespace and doesn't probe ordered fallback candidates.

Normal modules import owned files and unrooted directories, direct child normal modules, visible support packages and registered packages. They do not import parents, ancestors, normal siblings, grandchildren directly, sibling descendants or another module's private file path. A child module re-exports anything its parent should see from deeper descendants.

Scoped support packages: a `+*.bst` support root exposes a package named by its containing directory. For support package `S` whose nearest ancestor normal module is `P`:

- `S` is visible to `P`, visible to normal sibling modules and their descendants, not visible above `P` or outside `P`'s subtree, not imported from its own private implementation descendants, and another support package in the same owner scope cannot import `S`
- The support facade may import ordinary files it owns, any descendant module in its private subtree, support packages from a strictly outer scope and registered packages. It may not import its parent, normal sibling consumers or same-scope support siblings.
- Consumers see only the support facade's `export:` surface. They cannot address its private descendant modules.

The project-root `+*.bst` facade is a canonical compiled API-only module. It may define and export its own functions, types, constants, traits and other legal API-only declarations, and it receives a normal immutable compiled module artefact and public interface. Its package identity comes from the canonical project config `name`. It is not visible to internal project modules.

The facade has a special project-wide assembly privilege: `ProjectPackageAssembly` is a separate assembly and link plan that references the already compiled facade artefact and the public interfaces of selected descendant modules below `entry_root`, regardless of ordinary lexical module visibility. Assembly never recompiles or mutates the facade module and never bypasses an `export:` boundary. The facade emits no route or runtime entry. A project can be both an application and a package. Without this facade the project has no externally consumable Beanstalk package surface. It cannot import `@project` or expose any declaration transitively dependent on it.

Valid project structure is acyclic by construction, with a defensive cycle validator retained for malformed internal state and future extensions.

Namespace and collision policy: no import uses precedence, nearest-match shadowing or ordered fallback. Reject overlapping visible identities between support packages, direct child modules, extensionless source files, internal directory path segments, the project package name, Core or Builder package roots, dependency aliases and case-only variants. Recognised extensionless source kinds share one namespace: `docs.bst`, `docs.bd`, `docs.md` and `docs/` cannot coexist where each would mean `@docs`. Explicit-extension provider files may coexist with a same-stem directory only when syntax remains unambiguous. The same support-package name may appear in disjoint scopes. Overlapping scopes are rejected with diagnostics pointing to both declarations.

Public interfaces: a compiled module exports an immutable semantic interface containing exported declarations, canonical type identities, folded constant facts, generic templates, trait and conformance evidence, receiver surfaces, function access and effect summaries, runtime facts needed by consumers and provenance needed by package facades. Public interfaces use canonical cross-module type identities rather than donor-local handles. Private declarations never receive consumer-visible identities. Aliases affect source spelling, not semantic identity. Receiver methods remain attached to their receiver type's exported source surface and aren't independently imported, aliased or re-exported.

Binding-backed packages are virtual typed symbols, not Beanstalk modules. They expose opaque types, constants and free functions only. All binding-backed packages use stable package and symbol IDs. Direct builder packages and provider-created packages share one identity model. External package namespace imports may expose recursive package-local paths such as `io.input.*`. Source-module namespace records remain shallow and field-access-only. External import providers resolve supported non-Beanstalk sources before AST. HIR carries stable external symbol IDs. Backends map those IDs to target helpers, imports, generated glue or native operations. Binding-backed packages don't expose source receiver methods. Use source-owned wrapper types for method-style APIs. The bare `io` name is prelude policy for `@core/io`, not a package category.

A source dependency compiles as a separate package graph rather than being merged into the consumer's module graph. Each dependency owns its config, private `@project` interface and immutable module artefacts. A dependency never sees the consuming project's `@project` values. Dependencies compile against the active target builder's frontend capability surface. Artefact compatibility records the capability interfaces actually used, not merely a builder class name. A pure dependency may be reused across builders when required Core and Builder capability fingerprints are compatible. Consumers use the dependency package facade and immutable package artefacts. Persistent or precompiled artefacts may later replace source compilation without changing the semantic interface model. Public-interface provenance is retained on exported constants so the facade can reject `@project`-dependent exports.

### Compilation results and deterministic diagnostics

- `CompilerDiagnostic` owns source, syntax, config, import, type, rule, borrow and target-contract failures. Diagnostic payloads carry structured facts, stable reasons, source locations, symbols and semantic IDs instead of pre-rendered prose.
- `CompilerError` owns internal invariants, filesystem failures, backend failures and tooling infrastructure failures.
- `DiagnosticBag` owns stage-local accumulation. `CompilerMessages` is used only at build and rendering boundaries.
- `SourceLocation` stores interned path and scope identity. Rendering and filesystem-adjacent code resolve that identity through the build's `StringTable`. Boundary results carry the string table needed to render diagnostics after later build stages fail.
- A successful artefact may retain structured warnings for deterministic replay. Warning payloads don't affect semantic fingerprints. Replayed warnings are remapped into the current build's source and rendering context.
- Errors do not live in `CompiledModuleArtifact`. A failed `ModuleCompilationResult` contains diagnostics and no partial semantic interface.
- A failed module compilation returns diagnostics and no partial `CompiledModuleArtifact` or public interface. Consumers blocked by a failed required interface are not semantically compiled. Independent graph branches continue.
- Diagnostics owned by a shared module are emitted once. Dependants blocked by a failed interface don't emit one redundant blocked-module diagnostic each. One diagnostic set is produced per module.

Parallel string-table aggregation: parallel work may fork string tables only when deltas are merged and remapped in deterministic order. Module deltas merge in canonical module order, file-preparation deltas merge in original source-file order, and diagnostics and warnings never merge in worker-completion order. Tokens, headers, type-rendering contexts and module payloads are remapped before later stages consume them. Full table cloning remains available for genuinely independent identity boundaries but isn't the ordinary module-compilation path. Provider-backed discovery remains serial while it mutates shared package IDs, provider caches, resolution tables or diagnostic identity. Parallel provider discovery requires deterministic provider deltas and remapping.

### Identities, fingerprints and reuse

- Public identities remain stable across builds. A public declaration identity derives from stable package or project identity, canonical module path, module root role, exported declaration name, declaration category and receiver identity where relevant.
- Public identity does not depend on cosmetic root filenames, the ordinary source file containing the declaration, source position, declaration order or thread scheduling. Moving an exported declaration between files in the same module preserves identity. Renaming it or moving it to another module changes identity.
- Module-local `TypeId`, AST and HIR IDs remain local and replaceable.

Each successful module records separate invalidation fingerprints:

- Semantic public-interface fingerprint: exported names and identities, canonical type shapes, exported folded values, generic template semantics and bounds, trait and conformance evidence, receiver surfaces, function access and effect summaries and project-context provenance. Excludes private bodies, source locations, comments, warnings, formatting-only metadata and dormant root code that is not public API.
- Implementation fingerprint: every executable body and non-interface implementation fact that can change generated code, including bodies of exported functions.
- Dormant root-activity fingerprint: dormant start work, page fragments and entry metadata that affect entry activation.
- Runtime-dependency fingerprint: helpers, capabilities, external calls, target-gated features, glue and tracked runtime assets.
- Documentation fingerprint: public documentation and editor or API-index metadata.

Private or exported body changes do not recompile semantic consumers unless an exported semantic fact or effect changes. Implementation changes can relink artefacts without recompiling semantic dependants. Root-activity changes relink entries that activate the module. Runtime-dependency changes update capability, glue and asset plans. Documentation-only changes regenerate documentation or editor indexes without invalidating semantic consumers or generated executable instances.

Generic instances:

- The declaring module owns and validates an immutable generic template.
- Consumers emit requests keyed by stable generic declaration identity, canonical concrete types and required evidence identities.
- One deterministic project or package worklist deduplicates requests and continues until no generated function requests another instance.
- Generated functions live in sidecars and do not mutate base module artefacts. Generated instances are reused across entries.
- Cross-package instances belong to the consuming compilation while dependency base artefacts remain immutable.
- Generated instances are invalidated when template semantics, concrete types or required evidence change. HIR and backends see concrete executable targets only.

## Frontend stages

### Stage 0: project preparation and graph construction

Stage 0 owns source indexing, module ownership, root roles, legal topology, namespace identities, graph construction and deterministic scheduling. It orchestrates reusable file preparation rather than implementing a second import parser or scanner.

Flow:

```text
read and parse one self-contained config.bst
-> resolve direct project #Import fields
-> fold and validate project plus active config sections
-> derive entry_root and @project
-> build the canonical source index
-> tokenize and header-prepare project source once
-> finalise module and package graphs
-> collect reachable source #Import contracts
-> resolve remaining inputs and diagnose unknown inputs
-> compile dependency-ordered waves
```

Project config itself has no source imports and creates no config import graph.

- Stage 0 may schedule source preparation before the graph is complete. Tokenizer and header owners parse once and return retained metadata.
- Prepared tokens, source-kind payloads, headers, imports, declaration shells, root-activity shells, diagnostics and deterministic string-table delta or remap information are retained and reused by graph construction and module compilation.
- Import preparation emits two distinct edge classes: structural provider references consumed by Stage 0 graph construction and module-local symbol references consumed by AST visibility. Structural module and package edges are distinct from module-local declaration-ordering edges.
- Stage 0 uses structural import results to finalise graph edges. The same prepared headers later enter local aggregation and Stage 3.
- Stage 0 never owns a competing import grammar. Tokenization and header parsing remain the only syntax owners for their source surfaces.
- Later stages do not reparse or rescan information an earlier owner already produced.

Stage 0 owns: parsing `config.bst` through AST and extracting known folded config constants, validating `entry_root` as a relative directory strictly below the project root, building one canonical source-tree index, discovering normal and support roots and the optional project package facade, assigning every source file to its nearest module owner, classifying root roles once, discovering builder-supported source assets and provider imports, establishing extensionless import namespace identities, validating file, directory, module and package collisions, computing direct child-module relationships by nearest-module ancestry, computing support-package visibility scopes, rejecting illegal structural dependencies before semantic compilation, building the acyclic project module graph, assigning deterministic module and semantic identities, producing dependency-order compile waves, and recording source identities for diagnostics.

`entry_root` rejects an empty path, `.`, parent components, absolute paths, paths outside the project root and symlink-resolved equality with the project root. Single-file compilation remains an explicit synthetic-module mode.

Builder-supported `.bd` and `.md` assets participate in the same extensionless namespace as `.bst` source. Provider-backed explicit-extension imports retain their registered provider contract and explicit owner. Builder source packages use the same canonical public-interface and compiled-artefact model as project source modules. Binding-backed packages remain registry metadata. Stage 0 produces structure and inputs. It does not type-check executable bodies, generate HIR or perform borrow validation.

### Type identity contract

Each compiled module owns one local `TypeEnvironment`. `TypeId` equality in that environment is the only valid comparison for module-local semantic decisions. Cross-module interfaces use canonical project type identities rather than donor-local `TypeId` values. The canonical representation covers builtins, module-owned structs and choices, transparent aliases, options, collections, maps and fallible carriers, concrete generic nominal instances, generic parameters inside exported generic templates and external package types.

A consumer module may intern compact local `TypeId` handles for imported canonical types. Its `TypeEnvironment` retains the origin mapping back to canonical identity. Cross-module equality compares canonical identity, never rendered names or unrelated local handles. `DataType` is parse-only or diagnostic-only after semantic resolution and must not drive executable AST, HIR or backend semantic decisions.

Collection and map identity remain canonical constructed shapes: growable `{T}` and fixed `{N T}` collections are distinct, fixed capacity is semantic identity not an allocation hint, `{K = V}` maps store key and value identities directly, and backends query semantic shapes rather than parse syntax or private side tables. Type diagnostics carry semantic identities plus context enums. Renderers resolve source-level names through `DiagnosticRenderContext` at the output boundary.

AST builds the local `TypeEnvironment`. Early nominal registration records identity and generic parameter metadata. Canonical fields and variants are written after AST resolves their type shells. Member queries expose borrowed field or variant views and direct lookup helpers. Later stages don't clone member lists for semantic lookup. AST body emission receives `AstTypeInterner`, a narrow facade over `TypeEnvironment` that allows derived type interning and module-local compatibility caching without permitting nominal declaration mutation. Imported canonical types are interned through the same narrow AST-owned boundary. Consumer-local handles retain their canonical origin and don't mutate the exporting module's environment. Function signatures store local semantic `TypeId` values after resolution. Exported signatures also project canonical cross-module types into the immutable public interface. HIR structs and choices retain their local frontend type links. Cross-module call targets and public interfaces use stable module semantic identities. External parameters with no frontend mapping use `ExpectedParameterType::UnknownExternal`, never sentinel `TypeId` values.

### Stages 1 and 2: source preparation

Tokenization converts source text into structured tokens with source locations. It owns basic lexical recognition, source location tracking, string and template delimiter context, numeric literal scanning and source-location diagnostics, symbolic operator and mutable-declaration spacing diagnostics, style directive token recognition through the merged registry and syntax-level rejection of unsupported or unknown directive forms. `numeric_text` owns shared numeric grammar, normalisation, separator and exponent validation and materialisation helpers used by later semantic consumers. `TokenizerEntryMode` chooses the initial lexical state for a source file kind: normal `.bst` files start in ordinary code mode, Beandown `.bd` files start inside an implicit template body (preserving original Beandown source locations while rejecting an unescaped outer `]`), and plain Markdown `.md` has no tokenizer entry mode and is prepared before tokenization.

Header parsing is the only stage that discovers module-wide top-level declarations. It parses top-level declaration shells so later stages don't reconstruct them from raw tokens. It owns import and public re-export syntax, root-role-aware `export:` parsing, import binding against the Stage 0 namespace, file-local visibility construction, declaration shells for constants, functions, structs, choices, aliases, traits and conformances, local declaration dependency edges, dormant normal-root start-body separation, compile-time fragment placement metadata and source-kind adapters that synthesise ordinary declarations. Support roots and project package facades reject root runtime activity before AST. Normal roots retain dormant start and fragment metadata for entry assembly.

Imported module and package references resolve to stable public-interface identities. Header parsing doesn't copy provider declarations into the consumer or bypass a facade to reach private files.

Header dependency edges include every top-level declaration dependency needed before AST can resolve declarations linearly: imported declaration references, type alias targets, struct and choice field type annotations, function parameter and return type annotations, constant explicit type annotations, fixed collection capacity references in type annotations when the capacity is a visible compile-time constant, constant initializer references to other constants and structurally exposed const-template condition or control references when header parsing can identify them without parsing full template body semantics.

Header parsing doesn't type-check executable bodies or fold expressions. It prefers storing normalised, validated path and reference forms instead of raw import or path syntax where enough context exists for later stages to consume. Declaration-shell parsers are shared with AST body-local declaration parsing so top-level and body-local declaration syntax stays equivalent. Header parsing records parsed type-reference shells and dependency edges. AST owns resolving those shells into canonical `TypeId`s. Header import preparation consumes the Stage 0 module graph and namespace, then builds the file-local visibility used by dependency sorting and AST. It resolves module interfaces, support packages, source assets, binding-backed packages, aliases, prelude names and collision rules without probing filesystem fallbacks.

Constants are compile-time declarations. Header parsing records symbol-shaped references found in constant initializer tokens and resolves them far enough to create dependency edges to other constants. Executable function and start body references don't participate in dependency sorting. Body-local declarations don't participate in dependency sorting. The dormant normal-root start header is always appended last.

Declaration shells are structured top-level header payloads, not fully resolved AST nodes:

- constant shell: name, export flag, explicit type annotation, initializer token span or tokens, initializer reference hints and source order
- function shell: name, generic parameters, parsed signature and body tokens
- struct shell: name, generic parameters, parsed field names and types and default token data where applicable
- choice shell: name, generic parameters, variant names and payload field type shells
- type alias shell: name and target type annotation. Parameterised generic aliases are rejected before shell creation.
- trait shell: name, requirement signature shells and requirement type-reference dependency edges
- conformance shell: target type reference, trait references and declaration source context
- start shell: dormant normal-root executable token body, excluded from dependency sorting

Beandown `.bd` header preparation contributes one private synthetic constant declaration, `content #String`, whose initializer is a structurally built `$md` template over the original `.bd` body tokens. During AST template parsing, that Beandown source-kind context also defaults nested templates with no explicit directive to the Markdown formatter. Any explicit nested template directive overrides the Beandown default. Plain Markdown `.md` preparation renders the raw Markdown to HTML and contributes the same private `content #String` declaration shape with a synthetic string-literal initializer. Later dependency sorting and AST folding treat both declarations like any other compile-time constant. There is no Beandown- or Markdown-specific AST node, HIR path, borrow-checker path or backend path.

Prepared outputs feed graph construction and local aggregation. Stage 3 orders declarations inside one module. It doesn't order project modules, copy provider declarations into a consumer graph or admit imported declarations as local graph nodes.

### Stage 3: local declaration ordering

Stage 0 orders modules in the project graph. Stage 3 orders top-level declarations inside one canonical module using the prepared header metadata, without rescanning earlier-prepared sources.

Stage 3 owns:

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

Cross-module dependencies are satisfied by compiled immutable public interfaces. A provider module is compiled before its consumers according to the Stage 0 graph. Same-file constants retain source-order semantics. Same-file forward references are rejected. Cross-file constants inside one module use header-provided edges. Cross-module exported constants are already folded owned facts in the provider interface. A source-backed Builder package is compiled through the same module-interface model. Consumers don't treat its private headers as local graph nodes.

After dependency sorting: AST consumes declarations linearly, AST doesn't rebuild import visibility, AST may register nominal identities before resolving members, any missing local ordering edge is fixed in header parsing, any project dependency belongs in the Stage 0 module graph and dormant `start` is never a dependency participant. Stage 3 produces the finalised visibility package that AST consumes directly.

### Stage 4: AST semantics

AST consumes already-sorted declaration headers and the header-built module environment. It resolves declarations in order, folds constants and templates, parses executable bodies, type-checks expressions, validates function terminality and emits typed AST nodes. AST resolves constants, generics, traits, casts and templates before executable HIR reaches a backend. Each module owns one local `TypeEnvironment`, local HIR identities and local borrow facts. Cross-module references use stable project semantic identities, never donor-local indexes or `TypeId` values.

AST owns:

- module-local semantic declaration resolution
- imported canonical type projection into local `TypeId` handles
- public interface validation and canonical export projection
- executable body parsing and type checking
- body-local declarations
- function terminality validation
- contextual coercion at explicit receiving boundaries
- generic template validation and module-local concrete request emission
- trait, conformance and generic-bound evidence validation
- explicit cast evidence resolution and builtin folding
- constant and const-record folding
- exported folded constant facts
- template composition, slot routing, folding and runtime handoff preparation
- reactive source and subscription metadata
- module-local TIR from parser emission through finalisation
- entry config folding through ordinary module visibility
- const anonymous records
- Number value-to-string integration through the common frontend value-to-string path consumed by template folding and runtime lowering

AST should be described by this ownership and data-flow contract, not by a fixed internal pass count. Internal substeps are implementation details and may change as the stage is simplified.

#### Generics contract

The declaring module owns and validates each immutable generic template. Consumers infer concrete arguments from immediate call arguments and immediate expected result context, then emit requests keyed by stable generic declaration identity, canonical concrete type identities and required visible trait evidence. The build system owns a deterministic project-wide worklist that deduplicates requests, materialises concrete functions into generated-artefact sidecars and continues until no generated function requests another instance. Invalid generic templates are diagnosed when the declaring module compiles. Inference failures, missing evidence and invalid concrete substitutions are diagnosed at the requesting call site with declaration context where useful. Generated executable functions are lowered to concrete HIR and borrow-validated independently. Base module artefacts remain immutable. HIR and backends receive only concrete executable targets and never solve generic arguments or consume unresolved generic template state.

#### Traits contract

Trait declarations and conformances are compile-time frontend metadata. Header parsing records trait and conformance shells. AST owns semantic trait identity, requirement type resolution, explicit conformance validation, evidence visibility, generic-bound checks and bound-provided receiver-call resolution. Exported traits and reusable conformance evidence use stable module semantic identities. Consumers don't reconstruct conformance structurally. Receiver methods remain tied to the receiver type's exported source surface and aren't independently imported, aliased or re-exported. Traits are not value types. Trait names are valid only in trait declarations, conformance declarations and generic bounds. Static bound calls resolve to concrete executable targets before HIR. HIR and backends don't carry trait objects, erased dispatch or trait evidence for runtime dispatch.

#### Imports and visibility

AST consumes the header-built file visibility environment through `ScopeContext`. It may validate semantic use of visible symbols, but it must not rebuild import bindings or rediscover top-level visibility. All user-visible names go through one collision policy: same-file declarations, source imports, external imports, type aliases, prelude symbols and builtins cannot silently shadow each other. External expression and type resolution must go through the active `ScopeContext` visibility lookup. If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST pass.

#### Type checking and coercion

Expression evaluation determines the natural type of an expression and stays strict. Contextual coercion is applied only by the frontend site that owns the boundary. AST emission carries canonical `TypeId`s through field access, receiver lookup, builtin receiver validation, call validation, operator result typing and compatibility checks. `DataType` remains parse-only or diagnostic spelling once a semantic `TypeId` exists. Boundary owners include declarations and assignments, returns, concrete function parameters, struct and choice fields, default values, typed collection and map entries, template and string content, explicit `cast` target boundaries, `then` arms whose enclosing value-producing block has an explicit receiver and backend and prelude call contracts. Detailed numeric rules, match syntax, cast syntax and string coercion rules belong in `docs/language-overview.md`.

#### Value-producing blocks and terminality

Value-producing `if`, match and block-form `catch` are closed receiving constructs, not general expressions. They're valid only where the receiver is explicit, including declarations, assignments, multi-bind, returns and nested `then`. Every producing path must satisfy the receiver arity. AST owns user-facing receiving-context, arity and terminality diagnostics. Non-unit success returns must be terminal before HIR lowering. If HIR receives a non-unit function that can fall through, the AST contract was violated and HIR reports an internal transformation error.

#### Constants and folding

Constants are compile-time declarations and module metadata, not runtime top-level statements. Header parsing records initializer references for dependency ordering. AST owns semantic checking and folding. A module folds its constants and const templates once. Exported folded facts are copied into the immutable public interface as owned backend-neutral values. Consumers don't parse or fold provider templates again. Private inferred const facts are advisory optimisation metadata and don't affect semantics, dependency sorting or visibility. Fully folded struct constants may become const records: compile-time field-access-only groups that aren't runtime values and cannot be passed, returned, stored or used through runtime methods. Compile-time and runtime semantics must agree: checked numeric failure rules match, cast range and non-finite checks match, Float formatting matches and template interpolation output doesn't depend on the backend.

#### Templates

AST owns all template semantics. TIR is the single AST-local structural authority from parser emission through composition, formatting, folding and finalisation. `Template` is a thin handle carrying the durable TIR reference and source location while AST construction is active. It is not a registry handle.

AST owns:

- parsing template bodies and emitting them into TIR
- composing slots, inserts, wrappers and child templates in TIR
- folding fully constant templates into string literals
- preserving structured template `if` and `loop` bodies for runtime lazy lowering
- preparing runtime slot source and site plans after AST-owned schema extraction and contribution routing
- validating const-required template control flow before HIR
- rejecting escaped slot or insert helper artefacts that are invalid after composition and routing
- preserving runtime templates as runtime expressions
- replacing runtime templates with owned runtime handoff payloads before HIR
- removing helper-only template artefacts before HIR
- emitting builder-facing const top-level fragment metadata
- exporting only folded owned const-template facts through module interfaces

AST finalisation folds const templates or replaces runtime templates with neutral owned handoff payloads. The one module TIR store is dropped before the completed AST leaves the stage. No TIR reference, store, view, overlay or preparation value crosses a module interface or enters HIR. HIR only lowers finalised runtime templates that remain after AST folding. Runtime template control flow lowers inline as ordinary HIR branches, loops, accumulator appends and AST-prepared runtime slot source and site plans in the enclosing function, not as backend-specific template control-flow nodes. HIR consumes AST-prepared slot source and site plans and owned runtime handoff payloads only. It doesn't parse directives, validate slot schemas or reconstruct TIR.

TIR is AST-local:

- One AST module build owns one `TemplateIrStore`. Parser emission writes directly into that store. All TIR IDs are module-local typed IDs.
- The phase sequence is `Parsed -> Composed -> Formatted -> Finalized`. Folding requires `Composed` or later. AST-to-HIR handoff requires `Finalized`.
- An exact `TirView` is the structural read authority for templates after parser emission.
- One semantic preparation owner classifies a value as foldable, runtime or helper while validating all required authority. Preparation produces a folded string or owned HIR handoff.
- No TIR store, ID, view, overlay or preparation type crosses into a completed compiler module, public interface, HIR or backend. HIR receives folded strings or neutral owned runtime handoff data only.
- Missing required roots, overlays, phases or exact-view authority are internal errors, never permission to reconstruct template meaning from legacy content. There is no reconstruction fallback.
- Number formatting doesn't add Number-specific TIR nodes.

#### Reactivity V1

Reactivity V1 is frontend-owned source and template metadata that later stages preserve for backend feature validation and backend lowering. It must not become a second type system or a general closure or function-value model. Declaration syntax parses `$Type`, `$=` and `$T` parameter access markers as syntax only. AST resolves the underlying ordinary `TypeId`, assigns reactive source identity, validates `$(source)` template subscriptions and preserves reactive template string metadata. HIR carries backend-facing reactive source and template metadata and reachability facts without reparsing template directives or becoming a backend render-plan language. Borrow validation treats subscriptions as read-only source dependencies, not active borrow lifetimes, while ordinary mutations continue to follow existing mutable and exclusive rules. Backend feature validation applies the selected target contract before lowering. Runtime reactive behaviour remains backend-owned artefact policy.

### Stage 5: HIR and derived views

HIR lowers fully typed module AST and generated concrete functions into the first backend-facing semantic IR. Each module retains local HIR IDs and its paired local `TypeEnvironment`. Cross-module executable references use stable project targets such as a module-function identity. The callee body isn't copied into the caller. HIR makes control flow, locals, regions, calls and terminators explicit. Pure value construction may remain nested while effectful work is linearised into statements and temporary locals.

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
- assemble routes or project artefacts

Plain `HirBinOp` remains valid for booleans, comparisons and string concatenation. Runtime scalar arithmetic and unary negation must lower through `HirStatementKind::NumericOp`. HIR validation rejects regressions where numeric arithmetic survives as ordinary expression ops. HIR lowering treats user-facing source errors as already diagnosed by AST or earlier stages. HIR lowering and HIR validation use `CompilerError` for internal transformation invariants. If a non-unit function can fall through after AST terminality validation, that's a compiler invariant breach.

A backend-neutral structured HIR view is derived and validated from canonical HIR when a structured lowerer needs it. The structured view isn't a second semantic authority and may be cached only as derived data. HIR validation completes before borrow validation or target validation. Generated concrete functions produce HIR in sidecars.

HIR validation checks definition IDs, frontend `TypeId` links, region graph shape, start-function and function-origin metadata, CFG ownership, doc fragments, module constants, reactive metadata, side-table mappings, local and place references, terminators, patterns and expression invariants. `HirExpressionKind::Float` values must be finite `f64`. `NaN` and `Infinity` literals are rejected as internal invariant breaches. Validation failures are `CompilerError` with `ErrorType::HirTransformation`, not user-facing `CompilerDiagnostic`, because they represent compiler-internal lowering invariants.

Reachability records syntactic reachability for functions, blocks, external calls, map literals and operations, reactive template-backed values, reactive sinks, runtime casts, checked numeric operations and Float formatting and validation statements. It doesn't fold constants, eliminate dead branches, inspect borrow facts or perform backend lowering. Some target-gated checks need more than backend-neutral reachability: generic runtime value validation scans reachable blocks with the module `TypeEnvironment` because generic-instance detection is semantic type analysis, not a raw HIR reachability fact.

Source calls use one of three explicit target classes: module-local function target, stable cross-module function target and stable binding-backed external function ID. Cross-module targets resolve through the compiled project graph and entry or package link plan. HIR doesn't store import aliases, package source syntax or backend runtime names. Borrow validation resolves source function targets to exported access and effect summaries. Backends resolve source and external targets to generated functions, linked module functions, imports, glue or target-native operations. The HTML builder and backend validators use HIR reachability for runtime artefact planning and target-contract validation. This is syntactic CFG and function reachability, not constant-condition dead-code elimination, optimisation or ownership analysis.

Fresh rvalues passed to mutable (`~T`) call slots are materialised into compiler-introduced hidden locals before borrow validation. Borrow validation then sees ordinary local access, not a special temporary node kind.

### Stage 6: borrow validation

Borrow validation runs once for each canonical module and once for each generated concrete function.

It enforces:

- shared and exclusive access rules
- use-after-consumption safety
- conservative aliasing for collections and maps
- legal mutable call access
- control-flow joins
- inferred move safety
- reactive invalidation facts

It reads validated HIR and writes read-only side tables. It doesn't rewrite HIR, compute exact lifetimes or decide final runtime ownership.

Public function interfaces export the facts consumers need: parameter access modes, mutation effects, possible ownership consumption, return aliasing and relevant reactive effects. Cross-module call transfer consumes these summaries and never opens the callee's HIR as local control flow. Missing or inconsistent exported summaries are `CompilerError` invariant failures.

GC remains the semantic baseline. Ownership-aware lowering is an optimisation with identical source semantics. Ownership-aware backends may consume borrow facts for optimisation. GC-only paths preserve the same source behaviour without deterministic destruction. GC-only backends may ignore ownership optimisation facts but cannot skip borrow validation. GC and ownership-aware lowering accept and reject the same programs.

Reactive subscriptions are read-only source dependencies, not active borrow lifetimes. Borrow facts remain keyed by module-local HIR identity. Exported summaries usage uses stable module function identity.

## Project assembly and backend lowering

### Entry assemblies and command policies

A normal module stores dormant root activity in its canonical compiled artefact. Dormant normal-root code is fully parsed, type-checked, lowered to HIR and borrow-validated during canonical module compilation before it can be stored for later activation. Compiling the module doesn't decide whether its root is active. Entry assembly activates already-compiled dormant root work. It never triggers deferred semantic compilation. An `EntryAssembly` selects one normal module as the active entry and activates only that module's implicit `start()`, top-level runtime work, runtime page fragments, compile-time page fragments and entry-owned runtime dependencies. Imported normal modules expose their public interfaces without executing root work. Support modules and project package facades never have root runtime activity.

Header parsing records normal-root top-level runtime code as a dormant `HeaderKind::StartFunction`. It's excluded from local declaration dependency sorting and emitted after sorted declarations. Page fragments split before HIR: runtime templates remain runtime code inside dormant `start()`, compile-time templates fold once into owned module artefact data, each compile-time fragment records its runtime insertion index, entry assembly merges compile-time and runtime fragments in source order, and HIR never carries compile-time fragments or document structure.

One canonical normal module may produce several `EntryAssembly` values. The HTML builder initially produces at most one route entry per normal module.

For each artefact-producing entry, the HTML builder selects the active normal module, activates its dormant `start` and root fragment metadata, merges compile-time fragments at their recorded runtime insertion indexes, creates runtime fragment slots, executes active `start` once through the selected runtime path, hydrates runtime fragments in source order and assembles route HTML and companion artefacts.

Imported normal modules, support packages and the project package facade never execute root work. Modules without HTML artefact activity remain available to the graph but are excluded from route, runtime-glue and tracked-asset planning. HIR carries runtime code only. Entry assemblies and the HTML builder own document and route semantics.

### Per-function runtime facts and link planning

- Runtime dependency metadata is recorded per executable function: external calls, helper families, reactive features, numeric and cast operations, maps, target-gated features, runtime assets and cross-module calls.
- Entry and package link plans compute exact reachable unions from those facts. Backends do not repeatedly scan source or reconstruct imports.
- Numeric ownership: `numeric_text` owns lexical numeric grammar and materialisation helpers. Exact `Number` values use one frontend semantic owner. HIR records numeric domain, operator and failure mode rather than backend helper names or domain-specific duplicated variants. Rounding happens at source and HIR operation result boundaries according to the Number plan. Compile-time and runtime numeric behaviour must agree. Numeric optimisation facts remain side tables and do not mutate HIR. Unsupported runtime numeric domains are rejected before target lowering. JS-only check elision stays in the JS path until a second backend needs a shared analysis owner.

### HTML, JavaScript and Wasm partitioning

Target validation runs before lowering. `check` invokes the same validation roots without code generation.

Backend feature validation checks target-gated HIR features through explicit reachability roots and returns structured diagnostics for user-visible target-contract violations. External package validation checks reachable external calls against the selected target's lowering metadata. Backend lowerers receive only HIR features and external calls that their selected target contract can lower. Validation does not hard-code one execution root. The project builder supplies roots from the entry or package link plan.

The HTML builder consumes entry link plans and performs deterministic function-level partitioning per entry:

- `start` is JavaScript-owned.
- DOM, browser, project JavaScript and other JS-required dependencies force the containing function and transitive callers to JavaScript. Neutral console IO does not force JavaScript ownership.
- Remaining supported functions default to Wasm.
- No Wasm-owned Beanstalk function may call a JS-owned Beanstalk function after propagation. JavaScript-owned functions may call Wasm-owned functions through generated wrappers.
- Partition decisions record explicit reasons and are independent of debug or release mode.
- Canonical HIR and module artefacts remain shared.

Physical output variants:

- Variants are keyed by module identity, selected concrete function set, target assignments, ABI and layout identities, runtime capability requirements and relevant backend config fingerprint.
- Entries with the same key reuse one variant. Different keys produce different companion or Wasm variants.
- One source function may be JavaScript in one entry variant and Wasm in another.
- Each module has a generated JavaScript companion facade for an entry variant. Wasm is emitted per selected module variant.
- Each page owns one runtime instance and memory shared by its linked Wasm modules.
- Wasm lowering consumes an explicit selected-function and import plan. Wasm LIR is structured and builder-owned. Dispatcher-loop, `bst_start`, per-module memory, helper-export booleans and `i64` Int bridge architecture are removed rather than preserved through adapters.

Lowerer use cases:

- The HTML-JS path lowers HIR through the JS backend, then uses the shared fragment contract: const fragments render into the document, runtime fragment slots are emitted, the generated JS bundle is embedded or module-loaded, the active entry's `start` is called once and returned runtime fragments hydrate the slots in source order.
- The HTML page-bundle path emits only the entry link plan's reachable concrete function set and uses the JS backend's referenced external-function metadata to generate only the glue wrappers the emitted bundle calls. HTML-JS reactive runtime fragments are a separate, JS-only concern. Ordinary runtime page-fragment assembly is shared with HTML-Wasm. Reactive mounting is not.
- The direct standalone JS backend can emit a complete standalone JS bundle when configured to include every HIR function. It is a separate lowerer use case.
- The core standalone Wasm backend owns HIR-to-Wasm-LIR lowering, Wasm runtime contracts, request validation, optional binary emission and backend debug output. It consumes explicit linked module and generated-function targets. It is a separate lowerer use case.
- Provider-backed external JS has two emission levels: build-level runtime emission deduplicates JS runtime assets and required runtime module specifiers across the linked project compilation. Module-level glue generation inspects external functions referenced by the emitted JS bundle and emits only the wrapper module, import preamble and import-map entries needed for that page bundle.
- Tracked assets are a builder policy over frontend-rendered path usages. The frontend records semantic path facts while rendering paths. The HTML builder decides which file paths become emitted assets, chooses output paths relative to the final page route, reports asset warnings or conflicts and returns asset bytes as ordinary output artefacts.

### Output ownership

- Artefact builders own output-path settings and defaults inside their private project config section. Builders that emit no artefacts register no output settings. HTML defaults remain `dev` and `release` unless its selected config overrides them.
- Every output root is a validated relative path outside `entry_root`. The build system owns path validation, output writing, skip-unchanged writes, manifests and stale cleanup.
- Output ownership is keyed by stable builder identity and build profile. Development and release cannot silently claim the same root.
- An existing foreign manifest causes a structured conflict before writing. One builder never deletes files owned by another manifest. Independent builders have no force-overwrite escape hatch.
- Backends and project builders produce `OutputFile` records. They do not write final project outputs directly. Output writing is central.
- Future minification, obfuscation or other output transformations require an explicit ordered pipeline. A transformer receives the prior manifest and artefacts through a declared contract. The final manifest records the complete pipeline identity. Pipeline implementation is deferred.

## Incremental and persistent artefacts

- The first development build compiles the complete required graph. Later builds reuse successful in-memory module artefacts.
- Changed modules rebuild. Semantic dependants rebuild only when the provider's public-interface or exported-effect fingerprint changes.
- Affected entries relink when implementation, root activity, runtime dependencies or generated instances change.
- Project-field dependencies invalidate only modules that use the changed `@project` fields.
- Persistent caching is a later implementation of the same boundaries. A serialised artefact is reusable only when compatible with:
  - compiler semantic artefact format version
  - relevant language semantics version
  - stable package or project identity
  - source and config fingerprints
  - imported public-interface fingerprints
  - required Core and Builder capability-interface fingerprints
  - target-independent frontend feature configuration
  - any ABI or layout policy embedded in the artefact
- Incompatible artefacts are discarded and rebuilt. Normal builds do not attempt best-effort deserialisation, partial migration or compatibility repair.

## Implementation map

- Build system, project builder and Stage 0: `src/build_system/`, `src/builder_surface/`, `src/projects/`, `src/compiler_frontend/module_dependencies.rs`
- Frontend stage roots: `src/compiler_frontend/pipeline.rs`, `src/compiler_frontend/tokenizer/`, `src/compiler_frontend/headers/`, `src/compiler_frontend/declaration_syntax/`
- AST and TIR roots: `src/compiler_frontend/ast/`
- HIR and borrow roots: `src/compiler_frontend/hir/`, `src/compiler_frontend/analysis/borrow_checker/`
- Builder surface: `src/builder_surface/`
- HTML builder: `src/projects/html_project/`
- JS and Wasm backends: `src/backends/js/`, `src/backends/wasm/`
- Tests, validation and roadmap: `tests/cases/`, `justfile`, `docs/roadmap/`
