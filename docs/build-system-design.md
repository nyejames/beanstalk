# Beanstalk Build System Design

Beanstalk is a high-level language with first-class string templates. Its build system discovers projects, bootstraps config, constructs module and package graphs, schedules compilation, plans links and owns outputs.

This document is the single source of truth for accepted build-system, project graph, builder, tooling, link and output architecture. It describes the end state the build system implements, including contracts the progress matrix still reports as incomplete. It is not an implementation-status report.

`docs/compiler-design-overview.md` is mandatory prerequisite reading. It owns core compiler invariants, semantic representations, frontend stage contracts, stable identities, public interfaces, semantic fingerprints and the generic backend handoff. This document references those definitions rather than restating them.

Companion authorities:

- `docs/compiler-design-overview.md` for core compiler architecture and cross-stage compiler contracts
- `docs/language-overview.md` for source syntax, language semantics and language-scope decisions
- `docs/src/docs/codebase/design-scope/overview.bd` for design bias and scope boundaries
- `docs/src/docs/codebase/memory-management/overview.bd` for access, borrow, GC, ownership and destruction semantics
- `docs/src/docs/codebase/style-guide/style-guide.bd` for implementation standards
- `docs/src/docs/progress/#page.bst` for current support and backend coverage
- `docs/roadmap/roadmap.md` and `docs/roadmap/plans/` for sequencing, active work and genuinely deferred design

## Architectural invariants

- One directory-scoped `#*.bst` or `+*.bst` module is the canonical semantic compilation unit. A physical module is compiled once per project or package build.
- Stage 0 owns one canonical graph, file ownership, legal topology and deterministic scheduling.
- Tokenization and header parsing produce reusable source metadata once. Later stages do not reparse it. Stage 0 orchestrates that preparation rather than implementing a second import parser or scanner.
- `config.bst` is one self-contained compile-time source file. It cannot contain source imports or depend on another file, package or binding.
- Structural module and package graph edges are distinct from module-local declaration-ordering edges.
- Successful compiled module artefacts are immutable. A failed module compilation produces diagnostics and no partial artefact or public interface.
- Module interfaces use stable semantic identities, not donor-local indexes.
- Backends consume compiled graphs and explicit link plans. They do not rediscover source structure.
- The build system owns output writing, manifests and stale cleanup. Backends and project builders produce output records rather than writing final project outputs directly.
- Parallelism, reuse and caching must preserve deterministic identities, diagnostics and output order.

## Project bootstrap

### Self-contained config.bst

`config.bst` is build-system-owned compile-time Beanstalk source, not a module. It emits no HIR, start function or runtime artefact. It contains one required open `project` const record, private top-level helper constants declared before values that use them and top-level builder and tooling section records.

`config.bst` is one self-contained compile-time source file. It cannot contain source imports or depend on another file, package or binding. Direct `#Import` fields inside `project` remain build-input contracts and do not perform source resolution. An authored `import` declaration is rejected before path resolution with a structured diagnostic. Config parsing operates on exactly one authored source identity. Config bootstrap does not construct a package resolver, config import graph or config source set. Config uses ordinary tokenization, local declaration ordering, semantic checking and constant folding for its one file.

Authored config entries are top-level compile-time constants declared with `name #= value` or `name #Type = value`. Config permits the accepted constant and anonymous const-record surface only. It contains no source imports, runtime declarations, mutable bindings, functions, traits, conformances, standalone templates, page fragments or module exports.

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

`config.bst` does not select the project builder. Final builder-selection syntax and a possible Beanstalk-native build script system remain deferred. The current CLI selects HTML implicitly. One artefact builder runs per `build` or `dev` invocation.

### project and @project

- `project.name` is required and provides stable project identity.
- Compiler-owned project fields are strictly validated. Additional fully folded project metadata is allowed: public project values may contain folded scalar values, optionals, nested anonymous const records, collections of supported folded values and folded templates represented as strings.
- Project fields do not gain implicit sibling scope. A field initializer follows ordinary anonymous-record rules.
- Private helper constants provide reusable derived values used by later config values.
- The `project` record must be available before a builder or tooling section references it.

The folded project record produces a specialised immutable `ProjectGlobalsInterface` under the permanently reserved `@project` import root:

- The interface contains stable field identities, folded backend-neutral values, source locations, field-level fingerprints and project-context provenance. It contains no AST, HIR or runtime body.
- It is classified as project-local and Beanstalk-source-backed but is not discovered as a normal source package.
- `@project` exposes direct project fields as namespace members. It does not export another value named `project`.
- Normal modules and project-owned support packages may explicitly import `@project`. It is never implicitly injected into modules.
- No child module, support package, dependency alias, Core package or Builder package may claim `@project`. `@project` cannot be directly re-exported.
- Internal module or support-package exports may expose project-derived constants, but provenance is retained so the project package facade rejects any transitive dependency on `@project`.
- Project field dependencies are tracked at field granularity. Project-field changes invalidate only modules that use the changed `@project` fields.

### Imported build values

- Direct primitive or optional fields of `project` may declare `#Import` contracts. V1 `#Import` field types are `String`, `Int`, `Float`, `Bool`, `Char` and optional forms. Nested project fields do not declare `#Import` in V1. Nested project fields do not provide unqualified source input values.
- `#Import` is constant-source syntax, not a source import and not a semantic wrapper type. Project-level `#Import` contracts are collected and validated before module AST construction.
- Direct imported fields inside `project` resolve from explicit build input, builder-provided primitive globals, the declaration default or a diagnostic. They resolve before project settings are applied and before Stage 0 applies `entry_root`.
- A direct imported project field and every reachable same-name source `#Import` declaration form one strict contract. Matching requires the same semantic type, optionality, required or default state and folded default value. Different defaults are conflicting contracts.
- A fixed same-name project field is an authoritative provider for compatible source `#Import` declarations and blocks CLI override.
- CLI inputs use repeated `--input name=value` only. Unknown inputs are diagnosed after reachable config and source contracts are known.
- The project-wide barrier validates all reachable source contracts before affected modules compile.

The resolution order is fixed:

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

### Builder and tooling sections

- Top-level records other than `project` are potential builder or tooling config sections.
- The active artefact builder section is required, even when empty. The `project` record does not select that builder.
- The active builder section is recursively schema-validated through declarative metadata: accepted fields, nested shapes, required or defaulted values, closed domains, project or entry scope and stable identities where useful. Unknown fields inside the active section are diagnostics.
- Inactive or unavailable builder sections are parsed, name-resolved and folded as ordinary compile-time records but are not schema-validated or retained in `ProjectCompilation`. Unknown top-level record names are therefore allowed as inactive builder or tooling sections.
- Duplicate section names and collisions with primitive constants are rejected.
- Builder sections cannot declare `#Import` fields. They consume already folded values from `project` and use backend-neutral folded values rather than builder-specific nominal types.
- Builder project settings and builder entry settings use strict, non-overlapping schemas. There is no `ProjectAndEntry` or equivalent shared-scope escape hatch. Project and entry values do not implicitly inherit, merge or override one another.

Project-specific config validation remains in the active builder's schema validation. User config mistakes remain `CompilerDiagnostic` values while infrastructure failures remain `CompilerError` values. Complex release optimisation stays outside the fast frontend path unless correctness requires it.

Entry-local `config:` blocks:

- An entry `config:` block is root-only builder metadata, not an embedded independent `config.bst` compilation unit.
- It is valid only at the top level of a normal module root and at most once per root. It is invalid in normal files, support roots, project package facades, `export:`, executable bodies and `config.bst`.
- The block contains config section records only. Imports, aliases, support types, helper constants and `#Import` declarations live outside the block in the normal root file.
- The block uses the root file's ordinary compile-time visibility. It may reference imported constants, `@project`, same-file constants declared before it, source `#Import` constants, foldable local const-record types and selected-builder compile-time values available through normal module imports. Same-file forward references remain invalid.
- Its references participate in the module's ordinary header dependency metadata and AST constant folding. It creates no ordinary module symbol and no HIR representation.
- It cannot contain a `project` section or change project-level builder behaviour. It may contain active artefact-builder and tooling-overlay sections.
- Active builder entry fields are strictly schema-validated. Inactive sections are parsed and folded but not schema-validated. An entry block is optional. The active artefact-builder subsection inside it is also optional so tooling-only metadata remains possible.
- Every normal module's entry block is validated during canonical compilation, whether or not an entry assembly activates it. Only resolved settings for the active artefact builder contribute entry activity. Imported normal modules never apply their entry metadata to an importer.

## Project and package graphs

Terminology is strict:

- Module: one directory-scoped compilation and visibility unit rooted by `#*.bst` or `+*.bst`
- Package: a named reusable `@...` import root and future dependency or distribution unit
- Binding: a typed bridge to an implementation outside Beanstalk source
- Prelude: implicit import policy, not a package kind
- Library: informal wording only

### Source indexing and prepared-source orchestration

Stage 0 owns source indexing, module ownership, root roles, legal topology, namespace identities, graph construction and deterministic scheduling. It orchestrates reusable file preparation rather than implementing a second import parser or scanner.

The bootstrap and graph flow is the fixed order recorded under imported build values. Within it:

- Stage 0 may schedule source preparation before the graph is complete. Tokenizer and header owners parse once and return retained metadata.
- Prepared tokens, source-kind payloads, headers, imports, declaration shells, root-activity shells, diagnostics and deterministic string-table delta or remap information are retained and reused by graph construction and module compilation.
- Import preparation emits two distinct edge classes: structural provider references consumed by Stage 0 graph construction and module-local symbol references consumed by AST visibility. Structural module and package edges are distinct from module-local declaration-ordering edges.
- Stage 0 uses structural import results to finalise graph edges. The same prepared headers later enter local aggregation and Stage 3.
- Stage 0 never owns a competing import grammar. Tokenization and header parsing remain the only syntax owners for their source surfaces.
- Later stages do not reparse or rescan information an earlier owner already produced.

Stage 0 owns: parsing `config.bst` through AST and extracting known folded config constants, validating `entry_root` as a relative directory strictly below the project root, building one canonical source-tree index, discovering normal and support roots and the optional project package facade, assigning every source file to its nearest module owner, classifying root roles once, discovering builder-supported source assets and provider imports, establishing extensionless import namespace identities, validating file, directory, module and package collisions, computing direct child-module relationships by nearest-module ancestry, computing support-package visibility scopes, rejecting illegal structural dependencies before semantic compilation, building the acyclic project module graph, assigning deterministic module and semantic identities, producing dependency-order compile waves and recording source identities for diagnostics.

`entry_root` rejects an empty path, `.`, parent components, absolute paths, paths outside the project root and symlink-resolved equality with the project root. Single-file compilation remains an explicit synthetic-module mode.

Builder-supported `.bd` and `.md` assets participate in the same extensionless namespace as `.bst` source. Provider-backed explicit-extension imports retain their registered provider contract and explicit owner. Builder source packages use the same canonical public-interface and compiled-artefact model as project source modules. Binding-backed packages remain registry metadata. Stage 0 produces structure and inputs. It does not type-check executable bodies, generate HIR or perform borrow validation.

### Package classification

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

### Normal modules and dormant roots

Module roots:

- `#*.bst` defines a normal module. `+*.bst` defines an API-only support module that exposes a scoped package. One optional project-root `+*.bst` beside `config.bst` defines the external project package facade. The suffix after `#` or `+` is cosmetic. `config.bst` is not a module root.
- Every project source file belongs to its nearest containing module root.
- A normal module may own dormant top-level runtime work and page fragments. A support module and project package facade are API-only: they have no implicit `start`, top-level runtime statements, page fragments, route or builder artefact. Functions and ordinary runtime code inside functions remain valid.
- `export:` is the only public visibility marker for every root kind.

Dormant normal-root code is fully parsed, type-checked, lowered to HIR and borrow-validated during canonical module compilation before it can be stored for later activation. Compiling the module doesn't decide whether its root is active. Entry assembly activates already-compiled dormant root work. It never triggers deferred semantic compilation.

Normal modules import owned files and unrooted directories, direct child normal modules, visible support packages and registered packages. They do not import parents, ancestors, normal siblings, grandchildren directly, sibling descendants or another module's private file path. A child module re-exports anything its parent should see from deeper descendants.

### Scoped support packages

A `+*.bst` support root exposes a package named by its containing directory. For support package `S` whose nearest ancestor normal module is `P`:

- `S` is visible to `P`, visible to normal sibling modules and their descendants, not visible above `P` or outside `P`'s subtree, not imported from its own private implementation descendants and another support package in the same owner scope cannot import `S`
- The support facade may import ordinary files it owns, any descendant module in its private subtree, support packages from a strictly outer scope and registered packages. It may not import its parent, normal sibling consumers or same-scope support siblings.
- Consumers see only the support facade's `export:` surface. They cannot address its private descendant modules.

Scoped support packages are injected by package name.

### Project package facade

The project-root `+*.bst` facade is a canonical compiled API-only module. It may define and export its own functions, types, constants, traits and other legal API-only declarations, and it receives a normal immutable compiled module artefact and public interface. Its package identity comes from the canonical project config `name`. It is not visible to internal project modules. The compiler overview owns the legal API-only semantic compilation contract.

The facade has a special project-wide assembly privilege: `ProjectPackageAssembly` is a separate assembly and link plan that references the already compiled facade artefact and the public interfaces of selected descendant modules below `entry_root`, regardless of ordinary lexical module visibility. Assembly never recompiles or mutates the facade module and never bypasses an `export:` boundary. The facade emits no route or runtime entry. A project can be both an application and a package. Without this facade the project has no externally consumable Beanstalk package surface. It cannot import `@project` or expose any declaration transitively dependent on it.

### Dependency package graphs

A source dependency compiles as a separate package graph rather than being merged into the consumer's module graph. Each dependency owns its config, private `@project` interface and immutable module artefacts. A dependency never sees the consuming project's `@project` values. Dependencies compile against the active target builder's frontend capability surface. Artefact compatibility records the capability interfaces actually used, not merely a builder class name. A pure dependency may be reused across builders when required Core and Builder capability fingerprints are compatible. Consumers use the dependency package facade and immutable package artefacts. Persistent or precompiled artefacts may later replace source compilation without changing the semantic interface model. Public-interface provenance is retained on exported constants so the facade can reject `@project`-dependent exports.

### External providers and binding-backed packages

Builders declare the frontend-visible surface: source-backed packages, binding-backed packages, config keys, style directives, external import providers, builder runtime packages and supported source file kinds. Source and binding registries remain separate because their compiler and runtime needs differ. Shared `PackageMetadata` classifies both through the independent `PackageOrigin` and `PackageBacking` axes.

External import providers convert supported non-Beanstalk files into typed binding-backed package surfaces before AST consumes visibility. Provider results may also record runtime imports and assets for later link planning. Builder-runtime packages such as `@web/canvas` use the same binding identity and runtime asset path as provider-created packages.

Builder-supported source file kinds participate in extensionless source discovery without becoming modules. Beandown `.bd` and Markdown `.md` inputs become ordinary synthetic compile-time declarations during header preparation.

Binding-backed packages are virtual typed symbols, not Beanstalk modules. They expose opaque types, constants and free functions only. All binding-backed packages use stable package and symbol IDs. Direct builder packages and provider-created packages share one identity model. External package namespace imports may expose recursive package-local paths such as `io.input.*`. Source-module namespace records remain shallow and field-access-only. Binding-backed packages don't expose source receiver methods. Use source-owned wrapper types for method-style APIs. The bare `io` name is prelude policy for `@core/io`, not a package category.

### Import roots and collision policy

Source imports resolve from the importing file's owning module root, never the physical file directory. `@./...` and parent components are invalid. Paths may traverse ordinary unrooted directories owned by the same module. Reaching a child module or support package ends filesystem traversal and exposes only its facade. Import paths cannot bypass a facade with forms such as `@child/internal`. The project-root package facade is the sole assembly exception and resolves project paths from `entry_root`. Provider imports use an explicit owner and don't silently reintroduce file-relative resolution. Header import preparation consumes the Stage 0 namespace and doesn't probe ordered fallback candidates.

Namespace and collision policy: no import uses precedence, nearest-match shadowing or ordered fallback. Reject overlapping visible identities between support packages, direct child modules, extensionless source files, internal directory path segments, the project package name, Core or Builder package roots, dependency aliases and case-only variants. Recognised extensionless source kinds share one namespace: `docs.bst`, `docs.bd`, `docs.md` and `docs/` cannot coexist where each would mean `@docs`. Explicit-extension provider files may coexist with a same-stem directory only when syntax remains unambiguous. The same support-package name may appear in disjoint scopes. Overlapping scopes are rejected with diagnostics pointing to both declarations.

### Deterministic scheduling and module results

Valid project structure is acyclic by construction, with a defensive cycle validator retained for malformed internal state and future extensions.

Stage 0 produces dependency-ordered compile waves. Consumers blocked by a failed required interface are not semantically compiled. Independent graph branches continue. Diagnostics owned by a shared module are emitted once at project level rather than repeated for every blocked dependant. Parallel scheduling preserves the deterministic string-table merge and diagnostic ordering contract owned by the compiler overview.

Module compilation success and failure shapes, public interface contents and borrow fact ownership are compiler contracts. See `docs/compiler-design-overview.md`.

## Project compilation orchestration

The build system produces one immutable project payload from the canonical module graph. Conceptual shape only. Exact names may change.

```rust
pub struct ProjectCompilation {
    pub structure: ProjectModuleGraph,
    pub project_globals: ProjectGlobalsInterface,
    pub modules: Vec<CompiledModuleArtifact>,
    pub generated: Vec<ModuleGeneratedArtifacts>,
    pub entries: Vec<EntryAssembly>,
    pub package_facade: Option<ProjectPackageAssembly>,
}
```

Boundaries that may not change:

- successful artefacts are immutable
- failed results expose no partial interface
- generated sidecars are separate from base module artefacts
- entry assemblies are many-to-one with modules
- the project package facade compiles as an ordinary API-only module, and `ProjectPackageAssembly` is a separate assembly plan over its compiled artefact
- backends receive the graph and link plans

### Builder capability surfaces

Builders declare the frontend-visible surface listed under external providers and binding-backed packages. One artefact builder runs per `build` or `dev` invocation. The current CLI selects HTML implicitly. Final builder-selection syntax and a possible Beanstalk-native build script system remain deferred.

Builder-relevant root activity selects normal modules as entries. For HTML, root runtime work, page fragments and resolved active HTML entry config make a module an entry. Tooling-only entry config never creates an artefact entry.

### ProjectCompilation and generated sidecars

Generated concrete generic functions live in generated-artefact sidecars. They never mutate base module artefacts.

- The compiler owns generic inference, template validation, request shape, concrete HIR and borrow validation. See `docs/compiler-design-overview.md`.
- The build system owns a deterministic project-wide or package-wide worklist that deduplicates requests, materialises concrete functions into sidecars and continues until no generated function requests another instance.
- Requests are keyed by stable generic declaration identity, canonical concrete type identities and required evidence identities.
- Generated instances are reused across entries.
- Cross-package instances belong to the consuming compilation while dependency base artefacts remain immutable.
- Generated instances are invalidated when template semantics, concrete types or required evidence change.

### Command and tooling policies

- `build` and `dev` compile the union reachable from builder-selected artefact entries and the project package facade.
- `check` compiles every discovered module below `entry_root`.
- `check` also applies selected-target validation to actual linkable roots without performing backend lowering or writing outputs.
- Unsupported target features in unreachable private functions do not fail a build or check.
- `check` and future LSP are tooling overlays over the selected target builder surface, not independent copies of target packages, directives and source kinds.

### Entry assemblies and package link plans

A normal module stores dormant root activity in its canonical compiled artefact. An `EntryAssembly` selects one normal module as the active entry and activates only that module's implicit `start()`, top-level runtime work, runtime page fragments, compile-time page fragments and entry-owned runtime dependencies. Imported normal modules expose their public interfaces without executing root work. Support modules and project package facades never have root runtime activity.

One canonical normal module may produce several `EntryAssembly` values. The HTML builder initially produces at most one route entry per normal module.

Entry and package link plans compute exact reachable unions from the per-function runtime facts the compiler records. Backends do not repeatedly scan source or reconstruct imports.

`ProjectPackageAssembly` is the package link plan over the compiled facade artefact and selected descendant interfaces defined under the project package facade.

### Target-validation roots

- Target-validation roots are builder-selected entries, reachable generated functions, project package exports and any additional callable roots the selected builder declares.
- `check` invokes the same validation roots without code generation.
- Target validation runs before lowering.
- The project builder supplies roots from the entry or package link plan. Validation does not hard-code one execution root.

The compiler owns target-validation semantics over those roots. See `docs/compiler-design-overview.md`.

### Runtime dependency unions

The compiler records runtime dependency facts per executable function: external calls, helper families, reactive features, numeric and cast operations, maps, target-gated features, runtime assets and cross-module calls. Entry and package link plans compute exact reachable unions from those facts. Backends do not repeatedly scan source or reconstruct imports.

## HTML project builder

### Entry and fragment assembly

For each artefact-producing entry, the HTML builder selects the active normal module, activates its dormant `start` and root fragment metadata, merges compile-time fragments at their recorded runtime insertion indexes, creates runtime fragment slots, executes active `start` once through the selected runtime path, hydrates runtime fragments in source order and assembles route HTML and companion artefacts.

Imported normal modules, support packages and the project package facade never execute root work. Modules without HTML artefact activity remain available to the graph but are excluded from route, runtime-glue and tracked-asset planning. HIR carries runtime code only. Entry assemblies and the HTML builder own document and route semantics.

### JavaScript and Wasm partitioning

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

### External JavaScript and tracked assets

Provider-backed external JS has two emission levels: build-level runtime emission deduplicates JS runtime assets and required runtime module specifiers across the linked project compilation. Module-level glue generation inspects external functions referenced by the emitted JS bundle and emits only the wrapper module, import preamble and import-map entries needed for that page bundle.

Tracked assets are a builder policy over frontend-rendered path usages. The frontend records semantic path facts while rendering paths. The HTML builder decides which file paths become emitted assets, chooses output paths relative to the final page route, reports asset warnings or conflicts and returns asset bytes as ordinary output artefacts.

## Output ownership

- Artefact builders own output-path settings and defaults inside their private project config section. Builders that emit no artefacts register no output settings. HTML defaults remain `dev` and `release` unless its selected config overrides them.
- Every output root is a validated relative path outside `entry_root`. The build system owns path validation, output writing, skip-unchanged writes, manifests and stale cleanup.
- Output ownership is keyed by stable builder identity and build profile. Development and release cannot silently claim the same root.
- An existing foreign manifest causes a structured conflict before writing. One builder never deletes files owned by another manifest. Independent builders have no force-overwrite escape hatch.
- Backends and project builders produce `OutputFile` records. They do not write final project outputs directly. Output writing is central.
- Future minification, obfuscation or other output transformations require an explicit ordered pipeline. A transformer receives the prior manifest and artefacts through a declared contract. The final manifest records the complete pipeline identity. Pipeline implementation is deferred.

## Incremental and persistent artefacts

The compiler owns the semantic contents of the five module fingerprints. The build system owns invalidation, relinking and cache compatibility over them. See `docs/compiler-design-overview.md`.

- Private or exported body changes do not recompile semantic consumers unless an exported semantic fact or effect changes.
- Implementation changes can relink artefacts without recompiling semantic dependants.
- Root-activity changes relink entries that activate the module.
- Runtime-dependency changes update capability, glue and asset plans.
- Documentation-only changes regenerate documentation or editor indexes without invalidating semantic consumers or generated executable instances.
- Project-field dependencies invalidate only modules that use the changed `@project` fields.

Build and rebuild policy:

- The first development build compiles the complete required graph. Later builds reuse successful in-memory module artefacts.
- Changed modules rebuild. Semantic dependants rebuild only when the provider's public-interface or exported-effect fingerprint changes.
- Affected entries relink when implementation, root activity, runtime dependencies or generated instances change.
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

## Build-system implementation map

- Build system, project builder and Stage 0: `src/build_system/`, `src/builder_surface/`, `src/projects/`, `src/compiler_frontend/module_dependencies.rs`
- Builder surface: `src/builder_surface/`
- HTML builder: `src/projects/html_project/`
- JS and Wasm backends: `src/backends/js/`, `src/backends/wasm/`
- Tests, validation and roadmap: `tests/cases/`, `justfile`, `docs/roadmap/`

Compiler frontend, AST, HIR and borrow stage locations are mapped in `docs/compiler-design-overview.md`.
