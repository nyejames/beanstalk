# Compiler architecture documentation and roadmap alignment plan

## Purpose

Make `docs/compiler-design-overview.md` the concise, complete source of truth for Beanstalk compiler architecture, then align every active roadmap plan and compiler-facing reference with that architecture.

This is the mandatory documentation and planning pass immediately after the final TIR plan completes. It must happen before implementation begins on the canonical module graph, imported build values, entry config, Number or mixed HTML JS/Wasm plans.

The result must let an implementation agent answer these questions without inventing a temporary bridge:

- What is the canonical compilation unit?
- Which stage owns each parse, semantic and linking decision?
- Which metadata is reused rather than reconstructed?
- How are project config, `@project`, modules, packages and entries represented?
- What is immutable, generated, entry-specific or backend-specific?
- Which changes invalidate consumers, linked artefacts or documentation?
- How do `build`, `dev`, `check` and future tooling use the same compiler graph?
- How do JavaScript and Wasm share the frontend while producing entry-specific artefacts?
- Which deferred choices remain genuinely undecided?

This plan updates documentation and roadmap artefacts only. It does not implement compiler behaviour.

## Baseline

Audit against the repository state at the start of execution, not against the paths or implementation snapshots recorded here.

The plan was finalised from the design interview against `main` at:

```text
4dc4dec5423d46105e69fbfebc199089d6761c64
```

Before editing, refresh:

- `git branch --show-current`
- `git rev-parse HEAD`
- `git status --short`
- the current state block in `docs/roadmap/plans/final-tir-completion-plan.md`
- all active roadmap plan paths and status blocks
- implementation anchors named by the affected plans

Preserve unrelated user changes. Do not start this plan until TIR completion is accepted and its final documentation obligations are known.

## Authority hierarchy

Keep the documentation authorities distinct:

- `docs/compiler-design-overview.md`: accepted compiler architecture, stage ownership and cross-stage contracts
- `docs/language-overview.md`: exact source syntax, language semantics and language-scope decisions
- `docs/src/docs/codebase/memory-management/**`: access, borrow, GC, ownership and destruction semantics
- `docs/src/docs/codebase/design-scope/**`: concise design bias and scope boundaries
- `docs/src/docs/progress/#page.bst`: current implementation status and backend coverage
- `docs/roadmap/roadmap.md` and `docs/roadmap/plans/**`: implementation order, active work and genuinely deferred design
- `AGENTS.md` and codebase style documentation: repository workflow, implementation and validation rules

Do not move memory semantics, full language syntax or current support status into the compiler monolith.

## Writing rules

Apply these rules to the monolith and every revised plan:

- Use British English.
- Use no Markdown tables.
- Prefer tight headings, short paragraphs and compact lists.
- Avoid repeating a contract in multiple sections unless a short boundary reminder prevents a likely architectural mistake.
- Remove historical change logs, superseded alternatives and compatibility language.
- Describe accepted end-state architecture in present tense even when implementation is incomplete.
- Keep current-state implementation snapshots inside roadmap plans, not the monolith.
- Use conceptual Rust shapes only where ownership is clearer than prose. State that exact names may change.
- Do not freeze current file paths as permanent architecture.
- Do not use em dashes or curly apostrophes.
- Avoid headings that contain only navigation links.
- Do not retain text solely because it existed in the split compiler-design pages.

## Non-goals

- Do not delete `docs/src/docs/codebase/compiler-design/**` in this plan. The user will remove it after confirming migration completeness.
- Do not rename `docs/src/docs/codebase/design-scope/` in this plan.
- Do not modify memory-management semantics.
- Do not select a final CLI or source syntax for choosing project builders.
- Do not implement persistent caches, a package manager or a build-pipeline language.
- Do not implement the compiler changes described by the revised plans.
- Do not preserve old API shapes through compatibility wrappers.

## Final accepted architecture

Every item below must be encoded in the monolith or the named roadmap plan. No item may remain only in interview notes.

### Canonical graph and stage ownership

- One directory-scoped `#*.bst` or `+*.bst` module is the canonical semantic compilation unit.
- A physical module is compiled once per build.
- Normal module dormant root code is fully parsed, type-checked, lowered and borrow-validated even when no current entry activates it.
- Stage 0 owns source indexing, module ownership, root roles, legal topology, namespace identities, graph construction and deterministic scheduling.
- Tokenization and header parsing remain the only syntax owners for their source surfaces.
- Stage 0 orchestrates reusable file preparation rather than implementing a second import parser or scanner.
- Prepared tokens, headers, imports, declaration shells and source-kind metadata are retained and reused by graph construction and module compilation.
- Later stages do not reparse or rescan information an earlier owner already produced.
- Stage 3 orders declarations inside one module. It does not order project modules or copy provider declarations into a consumer graph.
- A failed module compilation returns diagnostics and no partial `CompiledModuleArtifact` or public interface.
- Consumers blocked by a failed required interface are not semantically compiled. Independent graph branches continue.

### Commands and entries

- `build` and `dev` compile the union reachable from builder-selected artefact entries and the project package facade.
- `check` compiles every discovered module below `entry_root`.
- `check` also applies selected-target validation to actual linkable roots without performing backend lowering or writing outputs.
- Target-validation roots include builder-selected entries, reachable generated functions, project package exports and any additional callable roots explicitly declared by the selected builder.
- Unsupported target features in unreachable private functions do not fail a build or check.
- Builder-relevant root activity selects normal modules as entries.
- For HTML, root runtime work, page fragments and resolved active HTML entry config can make a module an entry.
- Tooling-only `check` or `lsp` config never creates an artefact entry.
- One canonical normal module may produce several `EntryAssembly` values. The HTML builder initially produces at most one route entry per normal module.
- Entry assembly activates dormant root work. Compilation does not decide activation.

### Module, support-package and facade topology

- `#*.bst` defines a normal module.
- `+*.bst` defines an API-only scoped support module.
- One optional project-root `+*.bst` beside `config.bst` defines the external project package facade.
- Source imports resolve from the importing file's owning module root, never the physical file directory.
- `@./...` and parent components are invalid.
- Normal modules import owned files and unrooted directories, direct child normal modules, visible support packages and registered packages.
- Normal modules do not import parents, ancestors, normal siblings, grandchildren directly, sibling descendants or another module's private file path.
- Support packages follow the accepted owner-scope visibility rules and keep private implementation subtrees inaccessible to consumers.
- The project package facade can assemble the public interfaces of any descendant module below `entry_root`, regardless of ordinary lexical module visibility.
- The facade never bypasses an `export:` boundary.
- The facade cannot import `@project` or expose any declaration transitively dependent on it.
- Valid project structure is acyclic by construction, with a defensive cycle validator retained for malformed internal state and future extensions.

### Dependency packages

- A source dependency compiles as a separate package graph rather than being merged into the consumer's module graph.
- Each dependency owns its config, private `@project` interface and immutable module artefacts.
- A dependency never sees the consuming project's `@project` values.
- Dependencies compile against the active target builder's frontend capability surface.
- Artefact compatibility records the capability interfaces actually used, not merely a builder class name.
- A pure dependency may be reused across builders when required Core and Builder capability fingerprints are compatible.
- Consumers use the dependency package facade and immutable package artefacts.
- Persistent or precompiled artefacts may later replace source compilation without changing the semantic interface model.

### Project config and `@project`

- `config.bst` does not select the project builder.
- The current CLI continues to select HTML implicitly. Final builder-selection syntax and a possible Beanstalk-native build script system remain deferred.
- One artefact builder runs per `build` or `dev` invocation.
- `check` and future LSP support are tooling overlays over the selected target builder surface, not independent copies of target packages, directives and source kinds.
- `config.bst` remains build-system-owned compile-time Beanstalk source, not a module. It emits no HIR, start function or runtime artefact.
- `config.bst` contains one required open `project` const record and private top-level helper constants where useful.
- `project.name` is required and provides stable project identity.
- Compiler-owned project fields are strictly validated.
- Additional fully folded project metadata and nested folded records are allowed.
- Direct primitive or optional fields of `project` may declare `#Import` contracts.
- V1 `#Import` field types remain `String`, `Int`, `Float`, `Bool`, `Char` and optional forms.
- Nested project fields do not declare `#Import` contracts in V1.
- Project fields do not gain implicit sibling scope. A field initializer follows ordinary anonymous-record rules.
- Private helper constants can derive values used by several project fields.
- The folded project record produces a specialised immutable `ProjectGlobalsInterface` under the permanently reserved `@project` import root.
- `@project` exposes direct record fields as namespace members. It does not export another value named `project`.
- Normal modules and project-owned support packages may explicitly import `@project`.
- `@project` is not implicitly injected into modules.
- No child module, support package, dependency alias, Core package or Builder package may claim `@project`.
- `@project` cannot be directly re-exported.
- Internal module or support-package exports may expose project-derived constants, but provenance must be retained so the external project package facade rejects any transitive dependency on `@project`.
- Project dependencies are recorded at field granularity.

### Project-wide imported build values

- `#Import` is constant-source syntax, not a semantic wrapper type.
- Project-level `#Import` contracts are collected and validated before module AST construction.
- Direct imported fields inside `project` resolve before project settings are applied and before Stage 0 uses `entry_root`.
- The project-wide barrier validates all reachable source contracts before affected modules compile.
- A direct imported project field and every reachable same-name source `#Import` declaration form one strict contract when the project field is imported.
- Matching requires the same semantic type, optionality, required/default state and folded default value.
- Different defaults are conflicting contracts.
- A fixed same-name project field is an authoritative provider for compatible source `#Import` declarations and blocks CLI override.
- Nested project fields do not provide unqualified source input values.
- CLI inputs use repeated `--input name=value` only.
- Unknown inputs are diagnosed after reachable config and source contracts are known.

### Builder and tooling config sections

- Top-level records other than `project` are potential Builder or tooling config sections.
- The active artefact builder section is required in `config.bst`, even when empty.
- The `project` record does not select that builder.
- The active builder section is recursively schema-validated through declarative metadata.
- Schema metadata includes accepted fields, nested shapes, required/defaulted values, closed domains, project or entry scope and stable identities where useful.
- Unknown fields inside the active section are diagnostics.
- Inactive or unavailable builder sections are parsed, name-resolved and folded as ordinary compile-time records but are not schema-validated or retained in `ProjectCompilation`.
- Unknown top-level record names are therefore allowed as inactive builder or tooling sections.
- Duplicate section names and collisions with primitive constants are rejected.
- Builder sections cannot declare `#Import` fields. They consume values from `project`.
- Project config may use compiler and Core compile-time imports only. Builder sections use backend-neutral folded values rather than builder-specific nominal types.
- Builder project settings and builder entry settings use strict, non-overlapping schemas.
- Remove `ProjectAndEntry` or any equivalent shared-scope escape hatch.
- Project and entry values do not implicitly inherit, merge or override one another.

### Entry-local `config:` blocks

- An entry `config:` block is root-only builder metadata, not an embedded independent `config.bst` compilation unit.
- It is valid only at the top level of a normal module root and at most once per root.
- It is invalid in normal files, support roots, project package facades, `export:`, executable bodies and `config.bst`.
- The block contains config section records only.
- Imports, aliases, support types, helper constants and `#Import` declarations live outside the block in the normal root file.
- The block uses the root file's ordinary compile-time visibility.
- It may reference imported constants, project values, same-file constants, `#Import` constants, types used for foldable const records and compiler or selected-builder compile-time constants visible through ordinary imports.
- Same-file constants must be visible before the block. Same-file forward references remain invalid.
- Its references participate in the module's ordinary header dependency metadata and AST constant folding.
- It creates no ordinary module symbol and no HIR representation.
- It cannot contain a `project` section or change project-level builder behaviour.
- It may contain active artefact-builder and tooling-overlay sections.
- Active builder entry fields are strictly schema-validated.
- Inactive sections are parsed and folded but not schema-validated.
- An entry block is optional. The active artefact-builder subsection inside it is also optional so tooling-only metadata remains possible.
- Every normal module's entry block is validated during canonical compilation, whether or not an entry assembly activates it.
- Only resolved settings for the active artefact builder contribute entry activity.
- Imported normal modules never apply their entry metadata to an importer.

### Diagnostics and module results

- `CompilerDiagnostic` owns source, syntax, config, import, type, rule, borrow and target-contract failures.
- `CompilerError` owns internal invariants, filesystem failures, backend failures and tooling infrastructure failures.
- `DiagnosticBag` owns stage-local accumulation.
- `CompilerMessages` is used only at build and rendering boundaries.
- A successful artefact may retain structured warnings for deterministic replay.
- Warning payloads do not affect semantic fingerprints.
- Replayed warnings are remapped into the current build's source and rendering context.
- Errors do not live in `CompiledModuleArtifact`.
- A failed `ModuleCompilationResult` contains diagnostics and no partial semantic interface.
- Diagnostics owned by a shared module are emitted once.
- Dependants blocked by a failed interface do not emit one redundant blocked-module diagnostic each.

### Stable identities and fingerprints

- Public identities remain stable across builds.
- A public declaration identity derives from stable package or project identity, canonical module path, module root role, exported declaration name, declaration category and receiver identity where relevant.
- Public identity does not depend on cosmetic root filenames, the ordinary source file containing the declaration, source position, declaration order or thread scheduling.
- Moving an exported declaration between files in the same module preserves identity.
- Renaming it or moving it to another module changes identity.
- Module-local `TypeId`, AST and HIR IDs remain local and replaceable.
- Each successful module records separate invalidation classes:
  - semantic public-interface fingerprint
  - implementation fingerprint
  - dormant root-activity fingerprint
  - runtime-dependency fingerprint
  - documentation fingerprint
- Public-interface fingerprints include exported names and identities, canonical type shapes, exported folded values, generic template semantics and bounds, trait and conformance evidence, receiver surfaces, function access and effect summaries and project-context provenance.
- They exclude private bodies, source locations, comments, warnings, formatting-only metadata and dormant root code that is not public API.
- Private body changes do not recompile semantic consumers unless an exported effect or public fact changes.
- Implementation changes can relink artefacts without recompiling consumers.
- Root-activity changes relink entries that activate the module.
- Runtime-dependency changes update capability, glue and asset plans.
- Documentation-only changes regenerate documentation or editor indexes without invalidating semantic consumers or generated executable instances.

### Generic instances

- The declaring module owns and validates an immutable generic template.
- Consumers emit requests keyed by stable generic declaration identity, canonical concrete types and required evidence identities.
- One deterministic project or package worklist deduplicates requests and continues until no generated function requests another instance.
- Generated functions live in sidecars and do not mutate base module artefacts.
- Generated instances are reused across entries.
- Cross-package instances belong to the consuming compilation while dependency base artefacts remain immutable.
- Generated instances are invalidated when template semantics, concrete types or required evidence change.
- HIR and backends see concrete executable targets only.

### TIR

- Follow the final accepted TIR plan at execution time.
- The current final direction is one module-scoped `TemplateIrStore`, direct parser emission, module-local typed IDs and an exact `TirView`.
- TIR remains the only AST structural authority for templates after parser emission.
- Folding requires the accepted prepared phase and HIR handoff requires finalised authority.
- Missing required roots, overlays, phases or exact-view authority are internal errors, never permission to reconstruct template meaning from legacy content.
- No TIR store, ID, view, overlay or preparation type crosses into a completed compiler module, public interface, HIR or backend.
- HIR receives folded strings or neutral owned runtime handoff data only.
- Do not copy superseded multi-store or store-qualified identity wording from earlier plans into the monolith.

### HIR, borrow and runtime metadata

- HIR is the first backend-facing semantic IR.
- Each module retains local HIR IDs and its paired local `TypeEnvironment`.
- Cross-module calls use stable module-function targets.
- A backend-neutral structured HIR view is derived and validated from canonical HIR when a structured lowerer needs it.
- The structured view is not a second semantic IR and may be cached only as derived data.
- HIR validation completes before borrow or backend feature validation.
- Borrow validation runs for each canonical module and generated concrete function.
- Borrow validation reads HIR and writes side tables without rewriting it.
- Public function interfaces export parameter access, mutation, possible consumption, return aliasing and relevant reactive effects.
- Runtime dependency metadata is recorded per executable function, including external calls, helper families, reactive features, numeric and cast operations, maps, target-gated features, runtime assets and cross-module calls.
- Entry and package link plans compute exact reachable unions from those facts.
- Backends do not repeatedly scan source or reconstruct imports.

### Number and numeric ownership

- `numeric_text` owns lexical numeric grammar and materialisation helpers.
- Exact `Number` values use one frontend semantic owner.
- HIR records numeric domain, operator and failure mode rather than backend helper names or domain-specific duplicated variants.
- Rounding happens at source and HIR operation result boundaries according to the Number plan.
- Compile-time and runtime numeric behaviour must agree.
- Numeric optimisation facts remain side tables and do not mutate HIR.
- JS-only check elision stays in the JS path until a second backend needs a shared analysis owner.
- Unsupported runtime numeric domains are rejected before target lowering.
- Number formatting uses the common frontend value-to-string path consumed by template folding and runtime lowering. It does not add Number-specific TIR nodes.

### Mixed HTML JavaScript and Wasm backend

- The HTML builder consumes entry link plans and performs deterministic function-level partitioning per entry.
- `start` is JavaScript-owned.
- DOM, browser, project JavaScript and other JS-required dependencies force the containing function and transitive callers to JavaScript.
- Neutral console IO does not force JavaScript ownership.
- Remaining supported functions default to Wasm.
- No Wasm-owned Beanstalk function may call a JS-owned Beanstalk function after propagation.
- JavaScript-owned functions may call Wasm-owned functions through generated wrappers.
- Partition decisions record explicit reasons and are independent of debug or release mode.
- Canonical HIR and module artefacts remain shared.
- Physical output variants are keyed by module identity, selected function set, target assignments, ABI and layout identities and runtime-capability requirements.
- Entries with the same key reuse one variant. Different keys produce different companion or Wasm variants.
- One source function may be JavaScript in one entry variant and Wasm in another.
- Each module has a generated JavaScript companion facade for an entry variant.
- Wasm is emitted per selected module variant.
- Each page owns one runtime instance and memory shared by its linked Wasm modules.
- Wasm lowering consumes an explicit selected-function and import plan.
- Wasm LIR is structured and builder-owned. Dispatcher-loop, `bst_start`, per-module memory, helper-export booleans and `i64` Int bridge architecture are removed rather than preserved through adapters.

### Output ownership and deliberate pipelines

- Artefact builders own output-path settings and defaults inside their private project config section.
- Builders that emit no artefacts register no output settings.
- HTML defaults remain `dev` and `release` unless its selected config overrides them.
- Every output root is a validated relative path outside `entry_root`.
- The build system owns path validation, output writing, skip-unchanged writes, manifests and stale cleanup.
- Output ownership is keyed by stable builder identity and build profile.
- Development and release cannot silently claim the same root.
- An existing foreign manifest causes a structured conflict before writing.
- One builder never deletes files owned by another manifest.
- Independent builders have no force-overwrite escape hatch.
- Future minification, obfuscation or other output transformations require an explicit ordered pipeline.
- A transformer receives the prior manifest and artefacts through a declared contract.
- The final manifest records the complete pipeline identity.

### Incremental and persistent artefacts

- The first development build compiles the complete required graph.
- Later builds reuse successful in-memory module artefacts.
- Changed modules rebuild.
- Semantic dependants rebuild only when the provider's public-interface or exported-effect fingerprint changes.
- Affected entries relink when implementation, root activity, runtime dependencies or generated instances change.
- Project-field dependencies invalidate only modules that use the changed `@project` fields.
- Persistent caching is a later implementation of the same boundaries.
- A serialised artefact is reusable only when compatible with:
  - compiler semantic artefact format version
  - relevant language semantics version
  - stable package or project identity
  - source and config fingerprints
  - imported public-interface fingerprints
  - required Core and Builder capability-interface fingerprints
  - target-independent frontend feature configuration
  - any ABI or layout policy embedded in the artefact
- Incompatible artefacts are discarded and rebuilt.
- Normal builds do not attempt best-effort deserialisation, partial migration or compatibility repair.

## Required document changes

### 1. `docs/compiler-design-overview.md`

Replace the current organisation with the following compact outline:

```text
# Beanstalk Compiler Design Overview

Introduction and authority
Architectural invariants

## Project compilation model
### Builder and tooling surfaces
### Project config and @project
### Module graph, packages and imports
### Compilation results and deterministic diagnostics
### Identities, fingerprints and reuse

## Frontend stages
### Stage 0: project preparation and graph construction
### Stages 1 and 2: source preparation
### Stage 3: local declaration ordering
### Stage 4: AST semantics
### Stage 5: HIR and derived views
### Stage 6: borrow validation

## Project assembly and backend lowering
### Entry assemblies and command policies
### Per-function runtime facts and link planning
### HTML JavaScript and Wasm partitioning
### Output ownership

## Incremental and persistent artefacts

## Implementation map
```

Use this structure as the final architecture. Do not retain the current document simply by appending new sections.

#### Delete or compress

- Delete the large `Code navigation map` near the top.
- Delete the separate backend `Navigation` section.
- Move the few durable path references into the relevant ownership paragraphs or the final compact `Implementation map`.
- Delete repeated lists that restate HIR exclusions, TIR exclusions or backend source-rediscovery prohibitions without adding a new boundary.
- Compress arena capacity policy and exact Rayon threshold details into roadmap or implementation documentation. Keep only deterministic ownership and merge contracts in the monolith.
- Remove examples that teach language syntax rather than compiler ownership.
- Replace Markdown package tables with compact lists.
- Replace implementation-current `BackendBuilder`, `BuilderSurface` and payload listings when they obscure the accepted end state. Keep one conceptual `ProjectCompilation` block only.

#### Replace the opening invariants

Use concise wording equivalent to:

```markdown
## Architectural invariants

- A directory-scoped module is compiled once and owns local type, HIR and borrow identity.
- Stage 0 owns one canonical graph, file ownership, legal topology and deterministic scheduling.
- Tokenization and header parsing produce reusable source metadata once. Later stages do not reparse it.
- Module interfaces use stable semantic identities, not donor-local indexes.
- AST resolves constants, generics, traits, casts and templates before executable HIR reaches a backend.
- TIR is AST-local. HIR receives only folded strings or neutral owned runtime data.
- HIR is the first backend-facing semantic IR. Borrow validation reads it and writes side tables.
- Backends consume compiled graphs and explicit link plans. They do not rediscover source structure.
- GC is the semantic baseline. Ownership-aware lowering preserves the same source behaviour.
- Parallelism, reuse and caching must preserve deterministic identities, diagnostics and output order.
```

#### Replace the build-system boundary

Describe one successful immutable compilation payload and a separate failed result:

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

State that names may change but these boundaries may not:

- successful artefacts are immutable
- failed results expose no partial interface
- generated sidecars are separate
- entry assemblies are many-to-one with modules
- the package facade is an assembly plan, not another module compilation
- backends receive the graph and link plans

#### Add a compact project-config subsection

Encode the complete `project`, `@project`, builder-section and entry-config contracts from this plan. Do not reproduce user-facing syntax in depth. Include one short source example only if it makes the distinction clear:

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

Explicitly state:

- config does not select a builder
- `@project` is synthetic, immutable and project-local
- builder records are private
- inactive sections are folded but not schema-validated
- entry blocks consume normal root visibility and produce dormant metadata
- output settings belong to artefact builders

#### Rewrite Stage 0

Stage 0 must be presented as an orchestrator over reusable preparation:

```text
config and raw inputs
-> project #Import resolution
-> folded project record and @project interface
-> canonical source index
-> token/header preparation needed to resolve imports
-> module and package graph finalisation
-> dependency-ordered compile waves
```

Clarify the apparent ordering loop:

- Stage 0 may schedule source preparation before the graph is complete.
- Tokenizer and header owners parse once and return retained metadata.
- Stage 0 uses structural import results to finalise graph edges.
- The same prepared headers later enter local aggregation and Stage 3.
- Stage 0 never owns a competing import grammar.

#### Rewrite module and import contracts

Retain the accepted normal-module, support-package and project-facade rules in compact lists. Add:

- reserved `@project`
- separate dependency package graphs
- public-interface provenance
- project facade rejection of `@project`-dependent exports
- stable identity rules independent of source files and declaration order

#### Add fingerprints and incremental boundaries

Add one concise section defining the five fingerprints, their invalidation roles, generated sidecar reuse and persistent compatibility. This replaces vague future-only incremental wording.

#### Rewrite AST and TIR wording

Align with the TIR plan at execution time. At the current baseline, document:

- one module-scoped store
- direct parser emission
- exact `TirView`
- one semantic preparation path
- folded or owned HIR handoff
- no TIR state crossing the stage
- no reconstruction fallback

Do not retain old multi-store, foreign-store or overlay-set architecture merely because earlier versions of the monolith described it.

Add concise AST ownership for:

- entry config folding through ordinary module visibility
- project-wide generic requests
- imported canonical types
- const anonymous records
- Number value-to-string integration

#### Rewrite HIR and borrow wording

Add:

- stable cross-module calls
- derived structured HIR view
- per-function runtime dependency facts
- generated function HIR
- exported borrow effects

Keep the existing rule that user errors should have been diagnosed before HIR and internal invalid shapes use `CompilerError`.

#### Replace backend sections

Replace the parallel JS-only and HTML-Wasm descriptions with the accepted mixed partition design. Keep direct standalone JS and core standalone Wasm as separate lowerer use cases, but make HTML artefact planning the primary builder contract.

State that target validation happens before lowering and `check` invokes the same validation roots without code generation.

#### Add output ownership

Document builder-owned output config, central output writing, manifest ownership and explicit future pipelines. Remove any implication that all builders share global `dev_folder` or `output_folder` fields.

#### Final compact implementation map

End with a short locator list only:

- build and Stage 0 owners
- frontend stage roots
- HIR and borrow roots
- builder surface
- HTML builder
- JS and Wasm backends
- tests, validation and roadmap

Do not list every subdirectory.

### 2. `docs/roadmap/plans/entry-config-blocks-runtime-title-plan.md`

Replace the plan's accepted design and phase structure rather than patching isolated bullets.

#### Replace rejected assumptions

Remove:

- entry config as an isolated embedded `config.bst`
- imports or support declarations inside the block
- inability to see surrounding root constants and imports
- `ProjectAndEntry` field scope
- active-root-only compilation of the block
- unknown entry section diagnostics for inactive builders
- global output folders as project config
- any `project = "html"` bootstrap dependency

#### New purpose

Use this purpose:

```markdown
Implement root-local builder metadata through one `config:` block that participates in ordinary module visibility and constant folding, then replace reserved HTML metadata constants with resolved entry settings and add runtime `io.set_title` support.
```

#### New hard prerequisites

- final TIR completion
- post-TIR roadmap alignment
- canonical module graph and root roles
- anonymous const records and project config sections
- project-wide `#Import` resolution and `@project`

The plan must move after the imported-values/project-config plan in roadmap order.

#### New acceptance criteria

Include all of these:

- one block at most in a normal root
- no block declarations, imports or helpers inside
- ordinary root visibility and same-file source order
- header dependency metadata for referenced constants
- AST folding through the module path
- no HIR representation
- no `project` section
- strict active builder entry schema
- inactive section folding without schema validation
- project and entry field non-overlap
- validation for every normal module
- active builder settings only contribute artefact activity
- imported modules never apply metadata
- HTML keys `title`, `description`, `lang`, `favicon`, `body_style` and `head`
- removal of `page_*` compatibility behaviour
- shared JS/Wasm initial document metadata
- HTML-JS `io.set_title`
- pre-lowering target rejection elsewhere

#### Rewrite implementation phases

1. Refresh current module, project config, builder schema and HTML metadata owners.
2. Add root-block shell parsing and placement diagnostics without a second parser.
3. Extend header dependency metadata and ordinary AST folding for entry section records.
4. Add builder project/entry schema separation and dormant entry metadata on successful module artefacts.
5. Replace HTML reserved-constant scanning and document-shell inputs.
6. Add `io.set_title` metadata, reachability, validation and HTML-JS lowering.
7. Migrate fixtures, scaffolding and docs. Delete old paths.

Each phase must identify old owners to delete. Do not introduce an isolated config AST, a duplicate config registry or compatibility metadata scanner.

### 3. `docs/roadmap/plans/import_values_anonymous_records_plan.md`

Keep the existing file path and replace its title with:

```text
Project config, imported build values and anonymous records implementation plan
```

Do not rename the file. Retaining the path avoids link churn while the new title states its full responsibility.

#### Replace the config model wholesale

Remove:

- `project #= "html"`
- public top-level primitive config globals
- hidden nested project records that are never importable
- `package_folders`
- default `/lib` scanning
- flat builder keys
- global `dev_folder` and `output_folder`
- unknown top-level record rejection
- builder config `#Import`

#### Add final project config syntax

Use a canonical example like:

```beanstalk
config_helper #= "alpha"

project #= |
    name = "beanstalk_docs",
    version #Import of String = "0.1.0",
    entry_root = "src",
    metadata = |
        channel = config_helper,
    |,
|

html #= |
    dev_output = "dev",
    release_output = "release",
|
```

Do not imply sibling fields can refer to one another.

#### Add `ProjectGlobalsInterface`

The plan must implement a specialised synthetic interface with:

- reserved `@project` identity
- direct namespace members
- stable field IDs
- folded const values
- source locations
- field-level fingerprints
- provenance
- no AST, HIR or runtime body in the completed interface

Classify it as ProjectLocal and BeanstalkSource for package metadata, while making clear that ordinary source-package discovery does not create it.

#### Rewrite `#Import` phases

Separate resolution into:

1. parse raw CLI inputs
2. prepare config declarations and direct project import contracts
3. resolve project input fields
4. fold and validate `project`
5. establish Stage 0 structural settings and publish `@project`
6. prepare reachable module headers and collect source contracts
7. validate project-wide contract equality and unknown inputs
8. compile affected modules with resolved values

Do not reconcile different module values after AST compilation.

#### Anonymous records

Retain the accepted hidden nominal runtime model and const-record folding. Add:

- config sections and `project` use const anonymous records
- direct project fields may carry `#Import` source metadata
- runtime anonymous records still cannot escape public interfaces
- exported anonymous const records remain field-access-only
- no anonymous-specific HIR nodes unless existing nominal lowering cannot represent them

#### Builder schema redesign

Replace `ProjectConfigKeyRegistry` with a section-aware recursive schema owner, or rename it accordingly. The plan must require:

- compiler-owned `project` schema
- separate active builder project schema
- separate entry schema
- tooling overlay schemas
- no shared project/entry fields
- recursive field validation and defaults
- inactive section tolerance
- stable section and field identities where useful

#### Output settings

Move output paths into the active artefact builder section. The plan must migrate global config fields and CLI/build output-root lookups to builder-resolved output settings while preserving central path validation and output writing.

#### Builder selection

Remove the `project` selector. Record only:

- current commands default to HTML
- final builder-selection design is deferred
- one artefact builder per invocation
- possible future CLI or Beanstalk-native build orchestration requires a separate design

### 4. `docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md`

Keep the plan but replace its final binding decisions and deferred boundaries with the complete accepted graph model.

#### Add parse-once preparation

Specify that Stage 0 obtains graph edges from retained tokenizer/header preparation outputs. Delete any phase that performs a lightweight import scan and later reparses the same import grammar unless it is limited to provider-free source discovery that does not duplicate syntax ownership.

The plan must produce reusable prepared-file artefacts or equivalent stage-local outputs containing:

- tokens or source-kind prepared payload
- imports and normalised paths
- declaration shells
- dependency hints
- root activity shells
- diagnostics and warnings
- deterministic string-table delta/remap information

#### Add command graph policies

- build/dev required graph union
- check all discovered modules
- selected-target validation without lowering
- many entry assemblies per module
- full dormant root validation

#### Add result boundary

Implement successful immutable artefacts separately from failed diagnostic results. Remove diagnostics as an owned field of a supposedly successful `CompiledModuleArtifact`, except replayable warnings.

#### Extend module interfaces

Add:

- stable cross-build semantic-path identities
- exported constant provenance
- exported generic templates and evidence
- function effect summaries
- per-function runtime dependency facts
- separate fingerprints
- documentation fingerprint

#### Add dependency graph boundary

Record separate dependency package compilation, capability fingerprints and consumer-owned generated instances.

#### Add `@project`

Reserve the namespace, inject the synthetic interface into project-owned graph visibility, allow support-package imports, reject facade use and preserve provenance through internal exports.

#### Replace incremental deferral

The implementation may still defer persistent caching, but the plan must implement or record the accepted invalidation facts:

- interface
- implementation
- root activity
- runtime dependencies
- documentation
- generated requests
- entry links
- project-field dependencies

The first canonical implementation must expose stable data structures for these facts even if the dev server does not consume all of them immediately.

#### Replace physical-output deferral

The canonical module plan should not prescribe browser chunking, but it must guarantee that link plans can select subsets and reference deduplicated physical variants. Move final JS/Wasm variant policy to the Wasm plan.

### 5. `docs/roadmap/plans/html_project_backend_wasm_final_implementation_plan.md`

Retain the final mixed-backend direction but refresh it after canonical modules land.

#### Replace stale current-state scaffolding

- Consume `ProjectCompilation`, canonical module IDs, generated sidecars and entry link plans.
- Do not introduce an interim graph wrapper around `Vec<Module>` as a durable API.
- Remove compatibility adapters as soon as the canonical payload is wired.
- Use per-function runtime facts from compiler artefacts rather than repeated broad HIR scans where the information is already available.

#### Lock partition semantics

Add explicit accepted rules:

- partition per entry link plan
- JavaScript-owned start
- JS requirement backward propagation
- no Wasm-to-JS Beanstalk calls
- JS-to-Wasm wrappers
- explicit decision reasons
- debug/release-independent partition
- selected-target validation shared with check

#### Add physical variant keys

Define a conceptual key containing:

- module identity
- selected concrete function set
- target assignment
- ABI/layout identities
- runtime capability requirements
- relevant backend configuration fingerprint

Entries reuse identical variants. Different keys produce separate companion or Wasm outputs.

#### Structured HIR and LIR

- Keep structured HIR as a derived validated view, not persisted semantic authority.
- Keep structured Wasm LIR as the backend-owned emission IR.
- Remove dispatcher-loop and bridge paths rather than documenting them as permanent fallbacks.

#### Runtime and output

- one page-local runtime instance and memory
- linked module variants import the runtime
- project-level runtime artefact bytes may be emitted once while instances remain page-local
- builder-owned output roots and manifest identity
- no foreign-root overwrite
- future transformation pipeline deferred

### 6. `docs/roadmap/plans/number_type_numeric_plan.md`

Refresh this plan after final TIR and canonical module docs are aligned.

Required changes:

- remove references to legacy template fallback paths
- use the current one-store TIR and shared value-to-string handoff
- keep Number-specific nodes out of TIR
- align HIR numeric target shapes with stable module and generated-function artefacts
- keep numeric optimisation side tables local to JS until a second consumer exists
- include numeric semantic changes in implementation fingerprints and public type/signature changes in interface fingerprints
- treat target support through the shared selected-target validation path used by `check` and `build`
- do not add Number fields to config schemas without an explicit need and registered folded value shape

### 7. `docs/roadmap/plans/final-tir-completion-plan.md`

Do not rewrite its active implementation design from this plan.

At TIR completion:

- make its final docs phase update the monolith to the actual accepted one-store architecture
- remove stale split compiler-design references
- ensure no old multi-store, foreign-store, content fallback or registry terminology survives in active docs
- hand off only measured, genuinely deferred performance work to the roadmap
- mark this alignment plan as the next mandatory task

### 8. `docs/roadmap/plans/compiler-diagnostics-improvement-plan.md`

Keep its diagnostic content. Update cross-cutting architecture references only:

- `DiagnosticBag` for stage-local work
- no partial artefacts on module failure
- one diagnostic set per canonical module
- cached successful warnings only
- target-contract diagnostics shared by check and build
- project config, builder section, `@project`, output-manifest and package-facade provenance diagnostics
- no suggestions that teach removed `project = "html"`, flat config keys, `package_folders`, `@./` imports or old entry metadata constants

Do not expand the diagnostics plan into the owner of config or module architecture.

### 9. `docs/roadmap/roadmap.md`

Insert this plan immediately after TIR finalisation:

```markdown
- [Compiler architecture documentation and roadmap alignment](docs/roadmap/plans/compiler-architecture-documentation-and-roadmap-alignment-plan.md)
```

Use the final downloaded file at that path when adding it to the repository.

During this plan, refresh the entire roadmap rather than patching only linked plans.

#### Required roadmap order

Set this roadmap order after the alignment pass:

1. Final TIR completion
2. Compiler architecture documentation and roadmap alignment
3. Diagnostics improvements, if still incomplete
4. Canonical module compilation and scoped packages
5. Project config, imported build values and anonymous records
6. Entry-local config blocks and runtime title
7. Number and numeric semantics
8. HTML mixed JavaScript/Wasm backend

Keep this order. The Number plan remains before HTML Wasm so the backend plan consumes the final numeric HIR and target-validation surface.

#### Remove stale roadmap notes

Delete or rewrite:

- source-backed packages compile into every consumer
- dispatcher-loop CFG may be the permanent final shape
- `Project::Html(...)` or `project = "html"` as future config direction
- global output-folder assumptions
- vague incremental build wording that conflicts with accepted fingerprints
- Wasm TODO bullets already owned by the final Wasm plan
- package caching notes that imply dependency internals merge into consumer modules
- references to deleted predecessor plans or old branches

#### Add genuine deferred items

Keep only undecided surfaces:

- final builder-selection and build-script design
- package manager, dependency declaration and version solving syntax
- persistent artefact serialisation implementation
- explicit output transformation pipeline syntax and contracts
- cross-page shared browser chunks beyond variant deduplication
- sibling normal-module import fallback, only if project evidence justifies it
- future non-HTML target builders and their package capabilities

### 10. `docs/language-overview.md`

Update source-facing semantics without duplicating compiler internals.

Required changes:

- replace old `config.bst` flat-key and `project = "html"` examples
- document required open `project` record and private builder records
- document direct project-field `#Import`
- document `import @project` and grouped imports
- state that `@project` is project-local, reserved and not re-exportable
- document entry `config:` as a settings-only block using surrounding compile-time visibility
- document no declarations or imports inside an entry block
- document same-file earlier-constant rule
- document builder selection as deferred and current HTML default as tooling behaviour, not language semantics
- remove `package_folders` and `/lib` scanning
- update module/package import rules to canonical module-root-relative resolution
- keep compiler fingerprint, cache and backend partition details out of the language reference

### 11. User-facing docs, scaffolding and README

Update the following after their owning implementation plans land, not during this docs-only alignment unless examples are already presented as final design:

- project structure and config pages
- packages/import pages
- imported build values page
- HTML entry metadata pages
- Core IO page for `io.set_title`
- generated HTML project scaffolding
- README snippets
- progress matrix

The alignment plan should update links and final-design explanations now, but current-support claims must remain in the progress matrix.

### 12. `AGENTS.md` and codebase documentation

Ensure maintenance guidance says:

- compiler architecture and stage ownership belong in the monolith
- language, memory, progress and roadmap remain separate authorities
- implementation plans must update the monolith when they change an accepted cross-stage contract
- plans should not create replacement architecture pages under the soon-to-be-removed compiler-design split

Do not add references to coding agents in user-facing documentation.

## Execution phases

### Phase 0: post-TIR repository refresh

- Confirm TIR completion and final architecture.
- Capture current branch, commit and worktree state.
- Read all target files and implementation owners.
- Inventory active references to split compiler-design pages.
- Inventory stale terminology listed in validation below.
- Update this plan's baseline and any moved paths.
- Run the current docs validation baseline and record unrelated failures.

Exit when every target plan has a current owner map and no planned replacement depends on a deleted predecessor.

### Phase 1: rewrite the compiler monolith

- Apply the new compact outline.
- Insert the accepted architecture.
- Delete navigation and repeated implementation detail.
- Preserve relevant source links only in the final implementation map.
- Review for contradiction, repetition and token cost.
- Confirm every cross-stage contract is in one primary location.

Exit when an implementation agent can derive the entire pipeline and no active plan contains a stronger conflicting architecture statement.

### Phase 2: align canonical module and package planning

- Update canonical graph plan with parse-once preparation, results, identities, fingerprints, dependency graphs, `@project`, command policies and incremental boundaries.
- Update roadmap dependencies.
- Update the project facade and support-package wording in the language overview.

### Phase 3: replace config and imported-value planning

- Rewrite the imported-values plan around `project`, `@project`, builder sections and the project-wide resolution barrier.
- Move builder output settings.
- Rewrite entry config around ordinary module visibility and non-overlapping entry schemas.
- Reorder the plans so anonymous records and project config precede entry blocks.

### Phase 4: align backend and numeric plans

- Refresh Number ownership and TIR references.
- Refresh the Wasm plan to consume canonical artefacts and entry plans.
- Add variant deduplication, check validation and output ownership.
- Remove transitional backend architecture from final-design sections.

### Phase 5: roadmap-wide quality pass

For every remaining plan under `docs/roadmap/plans/`:

- refresh branch and commit assumptions
- remove deleted paths and predecessor-plan dependencies
- classify each design statement as accepted, current implementation or deferred
- remove completed work or compress it into a short current-state capsule
- remove repeated rationale already owned by the monolith
- ensure phases are agent-sized and leave the compiler valid
- ensure each plan names old owners to delete and forbids compatibility bridges
- ensure current validation gates match the style guide
- ensure roadmap order matches hard dependencies

### Phase 6: references, docs build and final audit

- Update AGENTS and canonical cross-links.
- Update language and high-level codebase references.
- Rebuild generated docs normally. Never edit `docs/release/**` directly.
- Run validation.
- Perform a final decision-coverage audit against this plan.

## Validation

Run the current documentation-only gate from the style guide. At minimum:

```bash
cargo run --quiet -- check docs
```

Run focused repository audits:

```bash
rg -n 'docs/src/docs/codebase/compiler-design' \
  AGENTS.md README.md docs --glob '!docs/release/**'

rg -n 'project\s*#?=\s*"html"|Project::Html|SUPPORTED_PROJECT_CONFIG_VALUES' \
  docs AGENTS.md README.md

rg -n 'package_folders|library_folders|default /lib|/lib scanning' \
  docs AGENTS.md README.md

rg -n '@\./|importing-file-relative|file-relative import' \
  docs AGENTS.md README.md

rg -n 'ProjectAndEntry|project-and-entry|shared project and entry' \
  docs

rg -n 'Vec<Module>|modules: Vec<Module>|flat backend payload' \
  docs/compiler-design-overview.md docs/roadmap/plans

rg -n 'TemplateIrRegistry|TemplateStoreId|foreign-store|legacy template fallback' \
  docs/compiler-design-overview.md docs/roadmap

rg -n 'dispatcher-loop.*permanent|bst_start|StringFromI64|per-module memory' \
  docs/compiler-design-overview.md docs/roadmap

rg -n '^\|' \
  docs/compiler-design-overview.md \
  docs/roadmap/plans/compiler-architecture-documentation-and-roadmap-alignment-plan.md
```

Interpret search results rather than requiring every query to be empty. Current-state sections may mention a removed term only when they clearly identify it as a deletion target. Final-design sections must not teach it.

Check manually:

- no Markdown tables in the revised monolith or this plan
- no em dashes or curly apostrophes
- no contradictory owner for config, graph, generic materialisation, target validation or output writing
- no decision from the accepted architecture section is absent from all target documents
- no implementation-status statement was moved into the monolith
- no user-facing language rule exists only in a roadmap plan
- no active plan instructs an agent to preserve a transitional bridge

## Acceptance criteria

- `docs/compiler-design-overview.md` is the single concise compiler architecture authority.
- The monolith is shorter or materially denser than the current version despite covering more accepted architecture.
- The split compiler-design pages contain no unique accepted contract needed before deletion.
- Every accepted interview decision is present in the monolith or the roadmap plan that owns its future implementation.
- TIR wording matches the completed TIR plan rather than an earlier representation.
- Config plans agree on `project`, `@project`, builder sections, entry blocks and imported values.
- Canonical module and Wasm plans agree on graph, link and physical variant ownership.
- Dependency packages remain separate compilation graph boundaries.
- `check` has a documented full-module and selected-target contract.
- Stable identities, fingerprints and incremental boundaries are explicit.
- Failed modules expose no partial artefacts.
- Output roots, manifests and future pipelines have one owner model.
- Roadmap order matches hard prerequisites.
- Deferred builder selection is explicitly recorded and not accidentally solved by config syntax.
- All revised Markdown uses compact lists rather than tables.
- Documentation validation passes, or unrelated failures are recorded precisely.

## Final decision-coverage audit

Before accepting the plan implementation, verify each group below against the actual updated files.

### Monolith-owned

- canonical compile-once modules
- parse-once reusable source preparation
- successful versus failed module results
- stable cross-module identities
- separate fingerprints
- generic sidecars
- TIR stage boundary
- HIR structured view
- per-function runtime facts
- borrow effect summaries
- dependency graph boundaries
- entry assemblies and command policies
- mixed HTML partitioning
- output ownership
- incremental and persistent compatibility

### Config and imported-values plan-owned

- required open `project` record
- direct project-field `#Import`
- private config helpers
- synthetic reserved `@project`
- field-level dependencies and provenance
- project-wide contract barrier
- active and inactive builder sections
- tooling overlays
- builder selection deferral
- builder-owned output paths
- anonymous record implementation

### Entry-config plan-owned

- settings-only root block
- ordinary root visibility
- same-file earlier constants
- no declarations inside
- no project settings
- strict active entry schema
- inactive section tolerance
- dormant validation
- HTML metadata and runtime title

### Canonical module plan-owned

- module topology and support scopes
- project facade assembly exception
- full dormant root validation
- command graph selection
- dependency package graphs
- result and interface data
- stable identities and invalidation facts
- multi-entry assembly support

### Wasm plan-owned

- entry-specific partition
- JavaScript-owned start
- no Wasm-to-JS calls
- selected function plans
- companion modules
- per-module variants
- page-local runtime memory
- structured HIR and LIR
- variant deduplication
- target validation roots

### Roadmap-owned deferrals

- final builder-selection or build-script system
- dependency declaration and package-manager syntax
- persistent cache implementation
- output transformation pipeline syntax
- broader browser chunk sharing
- sibling normal-module imports

No group may be marked complete by pointing back to the interview or this plan alone. Its final owner document must contain the accepted contract.
