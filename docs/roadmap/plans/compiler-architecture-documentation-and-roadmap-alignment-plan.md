# Compiler and build-system architecture documentation alignment plan

## Purpose

Establish two durable architecture authorities without losing any accepted design:

- `docs/compiler-design-overview.md` for the core compiler, semantic representations and frontend stage contracts
- `docs/build-system-design.md` for project bootstrap, graph orchestration, project builders, tooling, linking and output ownership

The compiler design overview remains mandatory for every implementation task. The build-system design document is mandatory whenever work touches project discovery, config, modules, packages, imports, builders, tooling overlays, link planning, backend assembly, output writing or incremental build artefacts.

This plan updates documentation and roadmap artefacts only. It does not implement the compiler or build-system behaviour it describes.

The work must preserve every accepted architecture decision. Compression is never an independent goal. Remove navigation, history and repetition only after the destination contract has been verified section by section.

## Current state

```text
ACTIVE_PLAN: docs/roadmap/plans/compiler-architecture-documentation-and-roadmap-alignment-plan.md
STATUS: in execution
CURRENT_SLICE: Phase 2 and 3 - decision inventory, then create docs/build-system-design.md by preservation first
BRANCH: main
WORKTREE: clean at phase start
BASELINE_COMMIT: 121187989
BASELINE_CHANGE: Phase 1 factual repair of the compiler overview accepted at 121187989. Phase 0 baseline refresh accepted at c6c79ece4. Original replacement plan and overview rewrite at c31ad8b558c2b4c84c39c11a10e698fabe945e17 and d9659079151bbe36229e09913db7c7ffe6b6ad48
PRE_REWRITE_OVERVIEW: 6c513f02555f5d63e886d0047852673a1f2fab97 (lost-contract audit reference)
CURRENT_COMPILER_AUTHORITY: docs/compiler-design-overview.md
NEW_BUILD_AUTHORITY: docs/build-system-design.md
BLOCKER: do not resume roadmap-plan alignment until both architecture documents pass the lost-contract audit
VALIDATION_RECORDED_AT_BASELINE: cargo run --quiet -- check docs passed with no errors or warnings at Phase 1 completion
NEXT_ACTION: build the section-level decision inventory, then create the build-system document without compressing moved contracts
```

Before editing, refresh:

- `git branch --show-current`
- `git rev-parse HEAD`
- `git status --short`
- the current state in `docs/roadmap/plans/final-tir-completion-plan.md`
- the current contents and diff of `docs/compiler-design-overview.md`
- every active roadmap plan named below
- current implementation owners named by those plans

Preserve unrelated user changes. Update this current-state block whenever a phase is accepted.

## Authority hierarchy

Keep these authorities separate:

- `docs/compiler-design-overview.md`: accepted core compiler architecture, semantic ownership and cross-stage compiler contracts
- `docs/build-system-design.md`: accepted build-system, project graph, builder, tooling, link and output architecture
- `docs/language-overview.md`: exact source syntax, language semantics and language-scope decisions that have not migrated to the codebase language docs
- `docs/src/docs/codebase/language/**`: migrated compiler-facing language semantics
- `docs/src/docs/codebase/memory-management/**`: access, borrow, GC, ownership and destruction semantics
- `docs/src/docs/codebase/design-scope/**`: design bias and scope boundaries
- `docs/src/docs/progress/#page.bst`: current implementation status and backend coverage
- `docs/roadmap/roadmap.md` and `docs/roadmap/plans/**`: implementation order, active work and genuinely deferred design
- `AGENTS.md` and codebase style documentation: repository workflow, implementation and validation rules

A roadmap plan may describe current implementation state and phased work. It must not override accepted architecture in either architecture document.

The progress matrix records what works today. It does not override accepted end-state design.

## Documentation writing rules

Every edited architecture or roadmap document must follow `docs/src/docs/codebase/style-guide/style-guide.bd`.

Required rules:

- Use British English.
- Use no Markdown tables.
- Avoid semicolons in prose.
- Avoid Oxford commas.
- Do not use em dashes.
- Use straight `'` apostrophes.
- Use contractions naturally unless a normative rule needs `must` or `must not`.
- Mix short sentences with longer explanatory sentences.
- Skip generic transitions such as `however`, `therefore` and `consequently`.
- Prefer tight headings, short paragraphs and compact lists.
- Keep code syntax exact even when it contains punctuation avoided in prose.
- Describe accepted architecture in present tense even when implementation is incomplete.
- Keep current implementation snapshots in roadmap plans, not architecture authorities.
- Use conceptual Rust shapes only when they clarify ownership. State that exact names may change.
- Do not freeze current source paths as permanent architecture.
- Avoid headings that contain only navigation links.
- Do not explain the document structure unless the reader needs that explanation.

## Protection rules

These rules apply to every phase:

- Do not replace either architecture document wholesale in one worker pass.
- Do not target a line count, percentage reduction or token quota.
- Do not compress and move content in the same step.
- Move or copy a contract first, verify it, then improve wording in a later pass.
- Keep a section-level decision inventory while content moves between documents.
- Do not delete source wording until its destination contains an equivalent or clearer contract.
- Compare every accepted slice against both the current restored overview and the pre-rewrite overview at commit `6c513f02555f5d63e886d0047852673a1f2fab97`.
- Review each diff for lost ownership rules, negative contracts, exceptional cases and deferred boundaries.
- Preserve exact language rules in language authorities rather than moving them into architecture documents.
- Preserve memory semantics in memory authorities rather than summarising them into weaker compiler prose.
- Do not create a third architecture document to avoid deciding ownership between the two accepted authorities.
- Do not preserve rejected architecture through compatibility wording or parallel descriptions.
- Do not continue to roadmap-plan alignment until the two architecture documents are accepted together.

## Non-goals

- Do not implement compiler or build-system behaviour.
- Do not choose final CLI or source syntax for selecting a project builder.
- Do not implement a Beanstalk-native build script system.
- Do not implement persistent cache serialisation.
- Do not design package declaration, registry or version-solving syntax.
- Do not implement an output transformation pipeline.
- Do not alter memory-management semantics.
- Do not delete the existing split compiler-design website pages until migration completeness is confirmed separately.
- Do not edit generated files under `docs/release/**` directly.
- Do not preserve old config imports, flat config keys, `project = "html"`, `package_folders`, global output folders or legacy HTML page metadata constants.

## Accepted architecture ledger

Every item in this section must be encoded in one architecture authority or the named roadmap plan. No item may remain only in conversation history.

### Canonical compilation and preparation

- One directory-scoped `#*.bst` or `+*.bst` module is the canonical semantic compilation unit.
- A physical module is compiled once per project or package build.
- Every normal module's dormant root code is parsed, type-checked, lowered to HIR and borrow-validated even when no current entry activates it.
- Stage 0 owns project discovery, source indexing, module ownership, root roles, legal topology, namespace identities, graph construction and deterministic scheduling.
- Tokenization and header parsing remain the only syntax owners for their source surfaces.
- Stage 0 orchestrates reusable source preparation rather than implementing a second import parser or scanner.
- Prepared tokens, source-kind payloads, headers, imports, declaration shells, root-activity shells, diagnostics and deterministic remap data are retained and reused.
- Later stages do not reparse or rescan information an earlier owner already produced.
- Stage 3 orders declarations inside one module. It does not order project modules or copy provider declarations into a consumer graph.
- Structural module and package edges are distinct from module-local declaration-ordering edges.

### Module compilation results

- Successful compiled module artefacts are immutable.
- A failed module compilation produces diagnostics and no partial `CompiledModuleArtifact` or public interface.
- Consumers blocked by a failed required interface are not semantically compiled.
- Independent graph branches continue.
- Successful artefacts may retain structured warnings for deterministic replay.
- Errors never live inside a successful compiled artefact.
- A shared module's diagnostics are emitted once rather than repeated for every blocked dependant.

### Stable identities and public interfaces

- Public identities remain stable across builds.
- A public declaration identity derives from stable package or project identity, canonical module path, module root role, exported declaration name, declaration category and receiver identity where relevant.
- Identity does not depend on a cosmetic root filename, ordinary source filename, source position, declaration order or thread scheduling.
- Moving an exported declaration between files in the same module preserves identity.
- Renaming it or moving it to another module changes identity.
- Module-local `TypeId`, AST and HIR IDs remain local and replaceable.
- Public interfaces use canonical cross-module type identities rather than donor-local handles.
- Public interfaces contain exported declarations, canonical types, folded constants, generic templates, trait and conformance evidence, receiver surfaces, function access summaries, effect summaries, runtime facts needed by consumers and provenance needed by package facades.
- Private declarations never receive consumer-visible identities.
- Aliases change source spelling, not semantic identity.
- Receiver methods remain attached to the receiver type's exported surface and are not independent namespace entries.

### Fingerprints and reuse

Each successful module records separate invalidation classes:

- Semantic public-interface fingerprint: exported identities, canonical type shapes, exported folded values, generic semantics and bounds, trait and conformance evidence, receiver surfaces, access and effect summaries and project-context provenance.
- Implementation fingerprint: every executable body and non-interface implementation fact that can change generated code, including bodies of exported functions.
- Dormant root-activity fingerprint: dormant start work, page fragments and entry metadata that affect entry activation.
- Runtime-dependency fingerprint: helpers, capabilities, external calls, target-gated features, glue and tracked runtime assets.
- Documentation fingerprint: public documentation and editor or API-index metadata.

Additional rules:

- Private or exported body changes do not recompile semantic consumers unless an exported semantic fact or effect changes.
- Implementation changes can relink artefacts without recompiling semantic dependants.
- Root-activity changes relink entries that activate the module.
- Runtime-dependency changes update capability, glue and asset plans.
- Documentation-only changes do not invalidate semantic consumers or generated executable instances.
- Project field dependencies invalidate only modules that use the changed `@project` fields.

### Generics

- The declaring module owns and validates each immutable generic template.
- AST infers concrete arguments and emits module-local concrete requests.
- The build system owns project-wide or package-wide aggregation, deduplication, scheduling and sidecar placement.
- Requests are keyed by stable generic declaration identity, canonical concrete type identities and required evidence identities.
- One deterministic worklist continues until no generated function requests another instance.
- Generated functions live in sidecars and never mutate base module artefacts.
- Generated instances are reused across entries.
- Cross-package instances belong to the consuming compilation while dependency base artefacts remain immutable.
- Generated functions lower to concrete HIR and are borrow-validated independently.
- HIR and backends never solve generic arguments or consume unresolved template state.

### TIR

- One AST module build owns one `TemplateIrStore`.
- Parser emission writes directly into that store.
- TIR IDs are module-local typed IDs.
- `Template` is a thin handle carrying the durable TIR reference and source location. It is not a registry handle.
- The phase sequence is `Parsed -> Composed -> Formatted -> Finalized`.
- Folding requires `Composed` or later.
- AST-to-HIR handoff requires `Finalized`.
- An exact `TirView` is the structural read authority.
- One semantic preparation owner classifies a value as foldable, runtime or helper while validating all required authority.
- Missing roots, phases, overlays or exact-view authority are internal errors.
- There is no reconstruction fallback from legacy content.
- No TIR store, ID, view, overlay or preparation type crosses into a completed module, public interface, HIR or backend.
- HIR receives folded strings or neutral owned runtime handoff data only.

### HIR, borrow and target facts

- HIR is the first backend-facing semantic IR.
- Each module retains local HIR IDs and its paired local `TypeEnvironment`.
- Cross-module calls use stable module-function targets.
- A backend-neutral structured HIR view is derived and validated from canonical HIR when a structured lowerer needs it.
- The structured view is not a second semantic authority and may be cached only as derived data.
- HIR validation completes before borrow validation or target validation.
- Borrow validation runs for every canonical module and generated concrete function.
- Borrow validation reads HIR and writes side tables without rewriting HIR.
- Public function interfaces export parameter access, mutation, possible consumption, return aliasing and relevant reactive effects.
- Runtime dependency facts are recorded per executable function.
- Per-function facts cover external calls, helper families, reactive features, numeric and cast operations, maps, target-gated features, runtime assets and cross-module calls.
- Entry and package link plans compute exact reachable unions.
- Backends do not reconstruct imports or repeatedly rescan source to rediscover these facts.

### Project config bootstrap

`config.bst` is deliberately self-contained.

- `config.bst` is build-system-owned compile-time Beanstalk source, not a module.
- It emits no HIR, start function or runtime artefact.
- It contains one required open `project` const record.
- It may contain private top-level helper constants declared before values that use them.
- It may contain top-level builder and tooling section records.
- It cannot contain source imports of any kind.
- Project-local, relative, Core, Builder, dependency and binding-backed imports are all rejected.
- An authored `import` declaration is rejected before path resolution with a structured diagnostic.
- Config parsing operates on exactly one authored source identity.
- Config bootstrap does not construct a package resolver, config import graph or config source set.
- Config uses ordinary tokenization, local declaration ordering, semantic checking and constant folding for its one file.
- Config permits the accepted constant and anonymous const-record surface only. It contains no runtime declarations, mutable bindings, functions, traits, conformances, page fragments or module exports.
- Project fields follow ordinary anonymous-record initializer rules and do not gain sibling-field scope.
- Private helper constants provide reusable derived values.
- The `project` record must be available before a builder or tooling section references it.
- `config.bst` does not select a project builder.
- The current commands select HTML implicitly.
- Final builder-selection design and a possible Beanstalk-native build script system remain deferred.
- One artefact builder runs per `build` or `dev` invocation.

### `project`, `@project` and imported build values

- `project.name` is required and provides stable project identity.
- Compiler-owned project fields are strictly validated.
- Additional fully folded project metadata and nested folded records are allowed.
- Public project values may contain folded scalar values, optionals, nested anonymous const records, collections of supported folded values and folded templates represented as strings.
- Direct primitive or optional fields of `project` may declare `#Import` contracts.
- V1 imported types are `String`, `Int`, `Float`, `Bool`, `Char` and optional forms.
- Nested project fields do not declare `#Import` in V1.
- `#Import` is constant-source syntax, not a source import and not a semantic wrapper type.
- Direct project `#Import` values resolve from explicit build input, builder-provided primitive globals, the declaration default or a diagnostic.
- Project input resolution happens before Stage 0 applies `entry_root`.
- The folded project record produces a specialised immutable `ProjectGlobalsInterface` under the permanently reserved `@project` root.
- The interface contains stable field identities, folded backend-neutral values, source locations, field-level fingerprints and project-context provenance.
- It contains no AST, HIR or runtime body.
- It is classified as project-local and Beanstalk-source-backed but is not discovered as a normal source package.
- `@project` exposes direct project fields as namespace members. It does not export another value named `project`.
- Normal modules and project-owned support packages may explicitly import `@project`.
- `@project` is never implicitly injected.
- No child module, support package, dependency alias, Core package or Builder package may claim `@project`.
- `@project` cannot be directly re-exported.
- Internal exports may expose project-derived constants, while provenance lets the external project package facade reject any transitive dependency on `@project`.
- Project field dependencies are tracked at field granularity.
- A direct imported project field and every reachable same-name source `#Import` form one strict contract.
- Matching requires the same semantic type, optionality, required or default state and folded default value.
- Different defaults are conflicting contracts.
- A fixed same-name project field is an authoritative provider for compatible source declarations and blocks CLI override.
- Nested project fields do not provide unqualified source input values.
- CLI inputs use repeated `--input name=value` only.
- Unknown CLI inputs are diagnosed after reachable source contracts are known.

### Builder and tooling config sections

- Top-level records other than `project` are potential builder or tooling sections.
- The active artefact builder section is required in project config even when empty.
- The active builder section is recursively schema-validated.
- Schema metadata covers accepted fields, nested shapes, required or defaulted values, closed domains, project or entry scope and stable identities where useful.
- Unknown fields inside the active section are diagnostics.
- Inactive or unavailable sections are parsed, name-resolved and folded as ordinary const records but are not schema-validated.
- Inactive sections are not retained in `ProjectCompilation`.
- Unknown top-level record names are allowed as potential inactive sections.
- Duplicate section names and collisions with primitive constants are rejected.
- Builder sections cannot declare `#Import` fields.
- Builder sections may consume already folded values from `project`.
- Builder sections use backend-neutral folded values rather than builder-specific nominal types.
- Builder project settings and builder entry settings use strict non-overlapping schemas.
- There is no `ProjectAndEntry` or equivalent shared-scope escape hatch.
- Project and entry settings do not implicitly inherit, merge or override each other.

### Entry-local config

- An entry `config:` block is root-only builder metadata, not an embedded `config.bst` unit.
- It is valid only at the top level of a normal module root and at most once per root.
- It is invalid in normal files, support roots, the project package facade, `export:`, executable bodies and `config.bst`.
- The block contains section records only.
- Imports, aliases, helper constants, support types and source `#Import` declarations live outside the block.
- The block uses the containing root file's ordinary compile-time visibility.
- It may reference imported constants, `@project`, same-file constants declared before it, source `#Import` constants, foldable local const-record types and selected-builder compile-time values available through normal module imports.
- Same-file forward references remain invalid.
- Entry config references participate in ordinary header dependency metadata and AST constant folding.
- The block creates no ordinary module symbol and no HIR representation.
- It cannot contain a `project` section or change project-level builder behaviour.
- It may contain active artefact-builder and tooling-overlay sections.
- Active builder entry fields are strictly schema-validated.
- Inactive sections are parsed and folded but not schema-validated.
- An entry block is optional.
- Its active artefact-builder subsection is optional so tooling-only entry metadata remains possible.
- Every normal module's entry block is validated during canonical compilation, whether or not an entry activates it.
- Only active artefact-builder settings contribute entry activity.
- Imported normal modules never apply their entry metadata to an importer.

### Modules, support packages and facades

- `#*.bst` defines a normal module.
- `+*.bst` inside the project source tree defines an API-only scoped support module.
- One optional project-root `+*.bst` beside `config.bst` defines the external project package facade.
- Every project source file belongs to its nearest module root.
- Normal modules may own dormant root runtime work and page fragments.
- Support modules and the project package facade are API-only and have no implicit start, top-level runtime work, page fragments, route or builder artefact.
- Functions and runtime code inside functions remain valid in API-only roots.
- `export:` is the only public visibility marker.
- Source imports resolve from the importing file's owning module root, never its physical file directory.
- `@./...` and parent components are invalid.
- Reaching a child module or support package ends filesystem traversal and exposes only its facade.
- Normal modules may import owned files, unrooted owned directories, direct child normal modules, visible support packages and registered packages.
- Normal modules may not import parents, ancestors, normal siblings, grandchildren directly, sibling descendants, unrelated branches or another module's private file path.
- Support packages follow the accepted owner-scope visibility rules and keep private implementation subtrees inaccessible to consumers.
- Valid project topology is acyclic by construction, with a defensive cycle validator retained.
- No import uses precedence, nearest-match shadowing or ordered fallback.
- Extensionless recognised source kinds share one import namespace.

### Project package facade

- The project-root `+*.bst` is a canonical compiled API-only module.
- It may define and export its own functions, types, constants, traits and other legal API-only declarations.
- It receives a normal immutable compiled module artefact and public interface.
- It has a special project-wide assembly privilege that may reference the public interfaces of any descendant module below `entry_root`.
- It never bypasses an `export:` boundary.
- It is not visible to internal project modules.
- `ProjectPackageAssembly` is a separate assembly and link plan that references the already compiled facade artefact and selected descendant interfaces.
- Assembly never recompiles or mutates the facade module.
- The facade emits no route or runtime entry.
- A project can be both an application and a package.
- Without the facade the project has no externally consumable Beanstalk package surface.
- The facade cannot import `@project` or expose a declaration transitively dependent on it.

### Dependency packages

- A source dependency compiles as a separate package graph rather than being merged into the consuming project graph.
- Each dependency owns its config, private `@project` interface and immutable module artefacts.
- A dependency never sees the consuming project's `@project` values.
- Dependencies compile against the active target builder's frontend capability surface.
- Artefact compatibility records the Core and Builder capability interfaces actually used rather than only a builder class name.
- A pure dependency may be reused across builders when required capability fingerprints are compatible.
- Consumers use the dependency package facade and immutable package artefacts.
- Persistent or precompiled artefacts may later replace source compilation without changing the semantic interface model.

### Commands, tooling and entries

- `build` and `dev` compile the union reachable from builder-selected artefact entries and the project package facade.
- `check` compiles every discovered module below `entry_root`.
- `check` applies selected-target validation to actual linkable roots without backend code generation or output writing.
- Target-validation roots are builder-selected entries, reachable generated functions, project package exports and additional callable roots declared by the selected builder.
- Unsupported target features in unreachable private functions do not fail a build or check.
- `check` and future LSP support are tooling overlays over the selected target builder surface.
- Tooling overlays do not duplicate target packages, directives or source kinds.
- Builder-relevant root activity selects normal modules as entries.
- For HTML, runtime root work, page fragments and resolved active HTML entry config can make a module an entry.
- Tooling-only entry config never creates an artefact entry.
- One canonical normal module may produce several entry assemblies.
- The HTML builder initially creates at most one route entry per normal module.
- Entry assembly activates dormant root work. Compilation does not decide activation.

### Mixed HTML JavaScript and Wasm

- Target validation runs before lowering.
- The HTML builder partitions concrete reachable functions per entry link plan.
- `start` is JavaScript-owned.
- DOM, browser, project JavaScript and other JS-required dependencies force the containing function and transitive callers to JavaScript.
- Neutral console IO does not force JavaScript ownership.
- Remaining supported functions default to Wasm.
- No Wasm-owned Beanstalk function may call a JS-owned Beanstalk function after propagation.
- JavaScript-owned functions may call Wasm-owned functions through generated wrappers.
- Partition decisions record explicit reasons and are independent of debug or release mode.
- Canonical HIR and module artefacts remain shared.
- Physical variants are keyed by module identity, selected concrete function set, target assignments, ABI and layout identities, runtime capability requirements and relevant backend config fingerprint.
- Entries with the same key reuse one variant.
- Different keys produce different companion or Wasm variants.
- One source function may be JavaScript in one entry variant and Wasm in another.
- Each module has a generated JavaScript companion facade for an entry variant.
- Wasm is emitted per selected module variant.
- Each page owns one runtime instance and memory shared by its linked Wasm modules.
- Wasm lowering consumes an explicit selected-function and import plan.
- Structured Wasm LIR is backend-owned.
- Dispatcher-loop, `bst_start`, per-module memory, helper-export booleans and `i64` Int bridge architecture are removed rather than retained through adapters.

### Output ownership and incremental artefacts

- Artefact builders own output-path settings and defaults in their private project config section.
- Builders that produce no artefacts register no output settings.
- HTML defaults remain `dev` and `release` unless its active section overrides them.
- Every output root is a validated relative path outside `entry_root`.
- The build system owns path validation, output writing, skip-unchanged writes, manifests and stale cleanup.
- Output ownership is keyed by stable builder identity and build profile.
- Development and release builds cannot silently claim the same root.
- An existing foreign manifest causes a structured conflict before writing.
- One builder never deletes files owned by another manifest.
- Independent builders have no force-overwrite escape hatch.
- Future minification, obfuscation or output transformation requires an explicit ordered pipeline.
- A transformer receives the prior stage's manifest and artefacts through a declared contract.
- The final manifest records the complete pipeline identity.
- The first development build compiles the complete required graph.
- Later builds reuse successful in-memory module artefacts.
- Changed modules rebuild.
- Semantic dependants rebuild only when the provider's public-interface or exported-effect fingerprint changes.
- Affected entries relink when implementation, root activity, runtime dependencies or generated instances change.
- Persistent caching is a later implementation of the same boundaries.
- A serialised artefact is reusable only when compatible with compiler artefact format, language semantics version, stable package or project identity, source and config fingerprints, imported interfaces, required capability interfaces, frontend feature configuration and embedded ABI or layout policy.
- Incompatible artefacts are discarded and rebuilt.
- Normal builds do not attempt best-effort migration or compatibility repair.

## Target architecture split

### `docs/compiler-design-overview.md`

This remains mandatory for all Beanstalk work.

It owns:

- core compiler invariants
- the prepared project and module input boundary
- module compilation success and failure shapes
- diagnostic lanes and deterministic identity remapping
- stable semantic identities
- module-local and canonical cross-module type identity
- immutable public semantic interfaces
- tokenizer ownership
- header parsing ownership
- structural edge output versus local declaration edges
- local declaration ordering
- AST semantics
- constants, coercion, casts, generics, traits and reactivity
- one-store TIR architecture
- HIR, HIR validation and derived structured views
- borrow validation and exported effect summaries
- per-function runtime facts produced by the compiler
- generic target-contract validation over explicit roots
- semantic definitions of the five fingerprints
- a compact compiler implementation map

It retains only a compact build-system handoff:

- the compiler receives stable project and module identities, prepared source metadata, dependency-ordered provider interfaces and selected validation roots
- the compiler emits immutable module results, generated requests, public interfaces, HIR, borrow facts, runtime facts and semantic fingerprints
- full discovery, config, topology, command and artefact policy lives in the build-system authority

It does not own:

- full project config design
- the `@project` bootstrap process
- complete module and package visibility topology
- CLI command policy
- project-wide generic worklist orchestration
- entry route assembly
- concrete HTML partition and output policy
- output manifests
- incremental build scheduling

### `docs/build-system-design.md`

This is mandatory for work involving Stage 0 or later project orchestration.

It owns:

- self-contained `config.bst` bootstrap
- `project`, `@project` and project-wide `#Import` resolution
- builder and tooling sections
- entry-local config integration
- source indexing and prepared-source orchestration
- structural module and package graph edges
- normal module, support-package and facade topology
- import-root resolution and collision policy
- dependency package graphs
- builder capability surfaces
- external providers and builder-supported source kinds
- deterministic project and package scheduling
- project-wide generic worklists and generated sidecars
- `ProjectCompilation`
- command policies for build, dev, check and future LSP
- entry and package link plans
- target-validation root selection
- HTML entry and fragment assembly
- concrete JavaScript and Wasm partitioning
- external JavaScript glue and tracked assets
- output settings, manifests and future pipeline boundaries
- in-memory reuse and persistent compatibility
- a compact build-system implementation map

The build-system document must state near the top that the compiler overview is a prerequisite authority.

### Primary-owner rule for crossing contracts

Do not duplicate full contracts in both documents.

- Module graph discovery and topology: full owner is the build-system document. The compiler document states the stable graph input it consumes.
- Public semantic interfaces: full owner is the compiler document. The build-system document states how graphs schedule and link them.
- Generic functions: compiler owns inference, template validation, request shape, concrete HIR and borrow validation. Build system owns worklists, deduplication, sidecars and entry reuse.
- Fingerprints: compiler owns semantic contents. Build system owns invalidation, relinking and cache compatibility.
- Runtime facts: compiler owns per-function fact production. Build system owns reachable-union link planning.
- Target validation: compiler owns validation semantics. Build system owns root selection and command invocation.
- Project package facade: compiler owns legal API-only semantic compilation. Build system owns project-wide assembly privilege and link plan.
- Backends: compiler owns the generic validated-HIR handoff. Build system owns project-builder policy, concrete HTML partitioning, assets and outputs.
- Diagnostics: compiler owns structured diagnostic data and semantic error lanes. Build system owns project aggregation, command boundaries and output-conflict diagnostics.

## Required correction pass before the split

Repair the current compiler overview while it is still the single architecture authority. Do not move content until these corrections are accepted.

### TIR wording

- Replace every `TemplateIrRegistry`, registry handle or plural-store statement with the one-store architecture.
- State `Parsed -> Composed -> Formatted -> Finalized`.
- State the folding and handoff phase requirements.
- State exact `TirView` authority and one preparation owner.
- Remove any implication that old content may reconstruct missing TIR state.

### Project package facade

- State that the root facade is a compiled API-only module.
- State that `ProjectPackageAssembly` references its compiled artefact and selected descendant public interfaces.
- Replace the statement that the facade is only an assembly plan.
- Preserve the rule that assembly does not trigger another compilation.

### Dependency edge classes

- Separate structural module or package dependencies from module-local declaration edges.
- Remove imported declarations from the local Stage 3 graph.
- State that import preparation emits structural provider references for Stage 0 and local symbol references for AST visibility.

### Imported build-value barriers

Use this build-system flow:

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

State that project config itself has no source imports and creates no config import graph.

### Dormant root validation

Restore an explicit rule that dormant normal-root code is fully parsed, checked, lowered and borrow-validated before it can be stored for later activation.

### Generic request ownership

Replace any AST ownership wording that says AST owns project-wide requests. AST emits module-local requests. The build system owns project-wide aggregation and materialisation.

### Implementation fingerprint

State that the implementation fingerprint covers all executable body semantics, including exported function bodies, when those changes do not alter the public semantic interface.

### `ProjectGlobalsInterface`

Add its stable fields, values, source locations, fingerprints, provenance and explicit absence of AST, HIR and runtime state.

### Project config import prohibition

Replace every statement that permits imported constants, support types, Core packages or Builder packages in `config.bst`.

State:

> `config.bst` is one self-contained compile-time source file. It cannot contain source imports or depend on another file, package or binding. Direct `#Import` fields inside `project` remain build-input contracts and do not perform source resolution.

## Target outline for `docs/compiler-design-overview.md`

Use this outline. Subheadings may be adjusted when readability improves, but ownership may not move back into this file.

```text
# Beanstalk Compiler Design Overview

Authority and companion documents
Architectural invariants

## Compiler input and result boundary
### Prepared project input
### Module compilation results
### Diagnostics and deterministic remapping
### Stable identities and public interfaces
### Type identity
### Fingerprints produced by the compiler

## Frontend stages
### Tokenization
### Header parsing and prepared source metadata
### Structural edges and local declaration edges
### Local declaration ordering
### AST semantics
#### Constants and coercion
#### Generics and traits
#### Templates and one-store TIR
#### Reactivity
### HIR and validation
### Derived structured HIR views
### Borrow validation
### Per-function runtime facts
### Target-contract validation

## Backend-facing compiler handoff

## Compiler implementation map
```

Content requirements:

- Keep one concise conceptual result shape only when it clarifies success versus failure.
- Define `CompiledModuleArtifact` ownership without listing project-builder fields.
- Keep stable identity and public-interface details complete.
- Keep explicit positive and negative stage ownership.
- Keep detailed TIR, HIR and borrow contracts that prevent reconstruction or ownership drift.
- Keep language syntax examples out unless one small example clarifies a compiler-specific distinction.
- Keep all concrete HTML, output and command behaviour out.
- End with a short implementation locator, not a directory catalogue.

## Target outline for `docs/build-system-design.md`

```text
# Beanstalk Build System Design

Authority and compiler prerequisite
Architectural invariants

## Project bootstrap
### Self-contained config.bst
### project and @project
### Imported build values
### Builder and tooling sections

## Project and package graphs
### Source indexing and prepared-source orchestration
### Normal modules and dormant roots
### Scoped support packages
### Project package facade
### Dependency package graphs
### Import roots and collision policy
### Deterministic scheduling and module results

## Project compilation orchestration
### ProjectCompilation and generated sidecars
### Command and tooling policies
### Entry assemblies and package link plans
### Target-validation roots
### Runtime dependency unions

## HTML project builder
### Entry and fragment assembly
### JavaScript and Wasm partitioning
### External JavaScript and tracked assets

## Output ownership

## Incremental and persistent artefacts

## Build-system implementation map
```

Content requirements:

- State that the compiler overview is mandatory prerequisite reading.
- Keep the config bootstrap single-file and import-free.
- Keep the two-phase project and source `#Import` resolution order explicit.
- Keep the full module, support-package, facade and dependency topology.
- Keep structural graph edges separate from local declaration edges.
- Keep the compiled root facade separate from its package assembly.
- Keep command graph policies and target-validation roots explicit.
- Keep generic worklist ownership in the build system without repeating compiler inference rules.
- Keep concrete HTML partition and output rules complete.
- Keep persistent caching as deferred implementation over accepted compatibility boundaries.

## Required roadmap and reference changes

### `docs/roadmap/plans/compiler-architecture-documentation-and-roadmap-alignment-plan.md`

Replace the current file with this plan at the same path.

Do not preserve the old phase status that claims the monolith rewrite is complete. The current rewrite remains the protected source for the split but requires the correction pass above.

### `docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md`

Update after both architecture documents are accepted.

Required changes:

- require the compiler and build-system architecture documents
- add parse-once prepared-source orchestration
- separate structural graph edges from local declaration edges
- add full dormant root validation
- add successful artefact versus failed result boundaries
- compile the project root facade as an API-only module and build its package assembly separately
- add stable cross-build identities
- add exported provenance and five fingerprints
- add per-function runtime facts
- add dependency package graph boundaries
- add reserved `@project`
- add command graph policies and selected-target validation
- record accepted incremental facts while deferring persistence implementation
- remove physical-output policy that belongs to the HTML plan
- remove `package_folders` and consumer-merged source packages

### `docs/roadmap/plans/import_values_anonymous_records_plan.md`

Keep the file path and replace its title with:

```text
Project config, imported build values and anonymous records implementation plan
```

Replace the config model rather than patching isolated bullets.

Required project-config changes:

- one self-contained `config.bst` file
- no source imports of any kind
- no config path resolver, config source set or imported config declarations
- required open `project` record
- private earlier helper constants
- project fields with no sibling scope
- direct project field `#Import`
- specialised `ProjectGlobalsInterface`
- reserved `@project`
- field-level project dependencies
- section-aware recursive builder schemas
- inactive section tolerance
- strict project and entry schema separation
- builder-owned output paths
- removal of `project = "html"`
- current implicit HTML builder selection and deferred final selection design

Required `#Import` phases:

1. Parse the one config file and collect direct project contracts.
2. Resolve project values before applying `entry_root`.
3. Discover and prepare project source once.
4. Collect reachable source contracts from prepared headers.
5. Validate strict same-name contracts.
6. Resolve remaining inputs and diagnose unknown inputs.
7. Supply resolved values to module AST compilation.

Required anonymous-record rules:

- config sections and `project` use const anonymous records
- direct project fields may carry import-source metadata
- runtime anonymous records remain hidden nominal types
- runtime anonymous records cannot escape public interfaces
- exported anonymous const records remain field-access-only
- avoid anonymous-specific HIR nodes when existing nominal lowering can represent them

Implementation deletion requirements:

- remove config import scanning
- remove config source-package root preparation
- remove config package path resolution
- remove multi-file config preparation
- remove imported config authored-scope classification
- remove config-only multi-file remap paths
- replace flat config key ownership rather than layering recursive schemas beside it

### `docs/roadmap/plans/entry-config-blocks-runtime-title-plan.md`

Rewrite its accepted design and phase structure.

Remove:

- entry config as an isolated embedded config file
- shared project-config import policy
- imports or support declarations inside the block
- inability to see surrounding module declarations
- `ProjectAndEntry` scope
- active-root-only validation
- inactive-section diagnostics
- global output folders
- any builder selector in config

Require:

- settings-only root block
- ordinary root visibility
- earlier same-file constants only
- normal header dependency metadata
- AST folding through the module path
- no HIR representation
- no `project` section
- strict active builder entry schema
- inactive section folding without schema validation
- validation for every normal module
- active builder metadata only for artefact activity
- no metadata application from imported normal modules
- removal of reserved `page_*` behaviour
- shared JavaScript and Wasm initial document metadata
- HTML-JS `io.set_title`
- pre-lowering target rejection elsewhere

State explicitly that project config has no imports while entry config may consume ordinary module imports because project discovery has already completed.

### `docs/roadmap/plans/html_project_backend_wasm_final_implementation_plan.md`

Update after canonical graph planning.

Require:

- `ProjectCompilation`, stable module IDs, generated sidecars and entry link plans
- per-function runtime facts rather than repeated source rediscovery
- partition per entry
- JavaScript-owned start
- backward JavaScript requirement propagation
- no Wasm-to-JavaScript Beanstalk calls
- JavaScript-to-Wasm wrappers
- explicit partition reasons
- debug and release independence
- selected-target validation shared with check
- physical variant keys and deduplication
- derived structured HIR and backend-owned structured Wasm LIR
- page-local runtime instance and memory
- builder-owned output roots
- foreign-manifest rejection
- future explicit transformation pipelines

Remove dispatcher-loop and bridge architecture as final fallback.

### `docs/roadmap/plans/number_type_numeric_plan.md`

Refresh:

- current one-store TIR terminology
- shared value-to-string handoff
- no Number-specific TIR nodes
- stable module and generated-function HIR integration
- numeric side tables local to JS until another consumer exists
- implementation and interface fingerprint effects
- shared selected-target validation used by check and build
- no numeric project-config fields without a registered folded schema need

### `docs/roadmap/plans/final-tir-completion-plan.md`

Do not rewrite its active design from this plan.

At completion:

- ensure both architecture documents use the final one-store terminology
- remove all registry, foreign-store and reconstruction wording from active authorities
- update TIR links to the compiler overview, not the build-system document
- hand off only measured deferred performance work

### `docs/roadmap/plans/compiler-diagnostics-improvement-plan.md`

Keep diagnostic content and align cross-cutting references:

- stage-local `DiagnosticBag`
- no partial artefacts on module failure
- one diagnostic set per canonical module
- replayable warnings only on successful artefacts
- target-contract diagnostics shared by check and build
- project config import rejection
- builder section, `@project`, output manifest and package facade provenance diagnostics
- no suggestions for removed config selectors, flat keys, `package_folders`, `@./` or legacy page metadata

### `docs/roadmap/roadmap.md`

Keep this alignment plan immediately after TIR finalisation until it is complete.

After alignment, order active plans by hard dependency:

1. Final TIR completion
2. Compiler and build-system architecture documentation alignment
3. Diagnostics improvements, if incomplete
4. Canonical module compilation and scoped packages
5. Project config, imported build values and anonymous records
6. Entry-local config blocks and runtime title
7. Number and numeric semantics
8. HTML mixed JavaScript and Wasm backend

Remove or rewrite:

- source packages compiling into every consumer
- dispatcher-loop CFG as a possible final design
- `Project::Html(...)` or `project = "html"`
- project-global output folders
- config import support
- vague incremental wording that conflicts with accepted fingerprints
- Wasm notes already owned by the final Wasm plan
- package caching notes that merge dependency internals into consumers
- deleted predecessor plans and stale branch assumptions

Keep genuinely deferred:

- final builder-selection and build-script design
- package declaration, registry and version-solving design
- persistent serialisation implementation
- explicit output transformation pipeline syntax
- cross-page shared browser chunks beyond variant deduplication
- sibling normal-module imports only if project evidence justifies them
- future non-HTML builders and their capabilities

### `docs/language-overview.md`

Update source-facing rules without copying compiler internals.

Required changes:

- self-contained import-free `config.bst`
- required open `project` record
- direct project-field `#Import`
- private helper constants
- private builder and tooling section records
- `import @project` and grouped imports
- reserved project-local non-re-exportable `@project`
- entry `config:` as a settings-only block using surrounding visibility
- no declarations or imports inside entry blocks
- same-file earlier-constant rule
- current implicit HTML builder as tooling behaviour, with final selection deferred
- canonical module-root-relative imports
- removal of `package_folders` and `/lib` scanning

Keep fingerprints, cache compatibility and backend partition details out of the language reference.

### `AGENTS.md`

Update mandatory reading:

- keep `docs/compiler-design-overview.md` mandatory for every task
- require `docs/build-system-design.md` for Stage 0, config, imports, modules, packages, builders, tooling, link planning, backend project assembly, outputs, incremental builds and the dev server
- state that compiler semantic architecture belongs in the compiler overview
- state that project and build orchestration belongs in the build-system document
- require both when a task crosses the boundary
- retain memory, language, progress and roadmap routing

Update the documentation policy so implementation plans modify the correct primary authority when an accepted cross-stage contract changes.

### Other references

After both documents are stable, update:

- `README.md`
- `CONTRIBUTING.md`
- `index.md`
- `docs/src/docs/codebase/overview.bd`
- any codebase navigation pages
- active roadmap plan reading lists
- user-facing docs that link directly to the old monolith for build-system concepts

Do not change current-support statements unless implementation status changed. Do not edit generated release docs directly.

## Execution phases

### Phase 0: refresh and protect the baseline

- Capture branch, commit and worktree state.
- Read `AGENTS.md`, both style-guide writing and validation docs, the current compiler overview, the current alignment plan and all named roadmap plans.
- Fetch or inspect the pre-rewrite overview at `6c513f02555f5d63e886d0047852673a1f2fab97`.
- Save a local section inventory of the pre-rewrite and restored documents.
- Search for stale config import, TIR registry, facade assembly and fingerprint wording.
- Record the current docs validation result.
- Update this plan's current-state block.

Exit when the baseline can be reproduced and every target document has a named owner.

### Phase 1: factual repair of the current compiler overview

Work section by section. Do not split content yet.

- Correct TIR to the final one-store architecture.
- Correct the project-root facade to compiled API-only module plus separate assembly.
- Separate structural dependency edges from local declaration edges.
- Add both imported-value barriers.
- State full dormant root validation.
- Correct generic request ownership.
- Expand implementation fingerprint ownership.
- define `ProjectGlobalsInterface` precisely.
- Make project config strictly import-free.
- Remove any remaining factual conflict found by comparison with the accepted ledger.

After each section:

- compare the changed text with the restored and pre-rewrite versions
- list every contract intentionally removed and its destination or reason
- stop if a contract has no clear destination

Exit when the single current compiler overview is factually coherent and includes every accepted contract before the split.

### Phase 2: create the decision inventory

Create a temporary local inventory, not a new repository authority.

For every architecture paragraph or bullet, record:

- source section
- primary destination document
- destination section
- whether wording moves unchanged, moves then edits, remains as a boundary summary or is removed as repetition
- linked roadmap plan when implementation detail belongs there

The inventory must cover the accepted architecture ledger in this plan.

Exit when every accepted decision has exactly one primary authority and any secondary mention is explicitly a short boundary reference.

### Phase 3: create `docs/build-system-design.md` by preservation first

- Create the new document with its target outline.
- Move or copy build-system-owned contracts from the repaired compiler overview without compressing them.
- Add the project config no-import rule and corrected bootstrap flow.
- Add the compiled facade plus separate assembly model.
- Add complete Stage 0, graph, command, builder, link, backend project and output ownership.
- Keep current code paths only in the compact implementation map.
- Link back to the compiler overview for semantic interface, TIR, HIR, borrow and target-validation definitions.

Do not remove the source content from the compiler overview during this phase.

Exit when the build-system document independently answers every question in its ownership list and contains no weaker restatement than the repaired source.

### Phase 4: slim the compiler overview section by section

For each build-owned section now present in the new document:

1. Verify the destination contract.
2. Replace the source section with the required compiler-facing boundary summary.
3. Add a precise link to the build-system document.
4. Compare the diff for lost semantic contracts.
5. Continue only after the section passes review.

Then reorganise the remaining compiler content into the target outline.

Allowed compression:

- navigation lists
- historical explanations
- duplicated negative lists when one primary list remains
- implementation-current file paths outside the final locator
- repeated backend rediscovery prohibitions

Forbidden compression:

- stage ownership
- data-shape boundaries
- negative ownership rules that prevent duplicated work
- exact TIR lifecycle and authority
- local versus canonical identity
- public interface contents
- failure and diagnostic boundaries
- semantic fingerprint contents
- generic, trait, cast, constant and terminality contracts
- HIR and borrow invariants

Exit when the compiler overview is focused on compiler semantics and remains sufficient for every implementation task.

### Phase 5: cross-document authority audit

Read both documents from the start as a pair.

Audit:

- no accepted decision is missing
- no full contract has two owners
- cross-links point to the primary owner
- terminology is identical
- project facade compilation and assembly are not conflated
- structural and local dependency edges are not conflated
- config and entry config import rules are distinct
- compiler and build-system generic ownership is distinct
- semantic fingerprints and build invalidation are distinct
- compiler target validation and build root selection are distinct
- no old TIR registry wording remains
- no config import path remains accepted

Perform a second audit against the pre-rewrite overview to identify details that are absent from both new documents. Restore any durable architecture contract or explicitly assign it to a roadmap plan.

Exit only after user review accepts both authorities together.

### Phase 6: align roadmap plans

Update plans in dependency order:

1. canonical module compilation and scoped packages
2. project config, imported values and anonymous records
3. entry-local config and runtime title
4. Number and numeric semantics
5. HTML mixed JavaScript and Wasm
6. diagnostics cross-references
7. TIR completion references
8. roadmap-wide stale-plan review

For each plan:

- refresh current paths and implementation state
- read both architecture authorities
- remove architecture copied from the wrong owner
- preserve plan-specific implementation detail
- name old owners and paths that must be deleted
- forbid compatibility bridges
- split phases into coherent valid compiler states
- record focused tests and validation
- update its current-state capsule

Exit when no active plan contains a stronger conflicting architecture statement.

### Phase 7: update reading routes and references

- Update `AGENTS.md`.
- Update README, CONTRIBUTING, index and codebase navigation.
- Update plan reading lists.
- Update language references.
- Rebuild generated docs through the compiler when required.
- Do not edit `docs/release/**` directly.

Exit when repository search finds no stale statement that the compiler overview alone owns build-system architecture.

### Phase 8: final audit

Perform the final audit below. Do not mark the plan complete based only on docs compilation.

## Validation

Use the current documentation validation gate from `docs/src/docs/codebase/style-guide/validation.bd`.

At minimum:

- run `cargo run --quiet -- check docs`
- run the repository's current documentation link or build validation when distinct
- rebuild generated docs through normal tooling when source docs changed
- inspect `git diff --check`
- inspect `git status --short`

Style scans:

- no Markdown table rows in edited architecture or roadmap files
- no em dash character
- no curly apostrophe character
- no semicolons in prose outside code blocks
- no obvious Oxford comma constructions
- no stale `TemplateIrRegistry` or multi-store architecture in active docs
- no accepted config source imports
- no `project = "html"`
- no `package_folders`
- no global project output-folder ownership
- no statement that dependency source is merged into consumer modules

Repository searches must include:

```text
TemplateIrRegistry
foreign store
project = "html"
Project::Html
package_folders
config import
imported config
SourceAndBindingPackagesOnly
ProjectAndEntry
page_title
page_head
@./
dispatcher loop
bst_start
```

A search hit is not automatically wrong. Classify each hit as active design, current implementation to remove, historical test data or generated output.

## Lost-contract audit

Before accepting each architecture document:

- compare it with the repaired pre-split overview
- compare it with the pre-rewrite overview at the baseline parent
- compare it with every accepted decision in this plan
- inspect removed lines, not only added lines
- verify each removed contract has a destination or a written reason for deletion
- verify negative rules remain visible enough to prevent an agent from inventing a duplicate path
- verify special cases remain explicit
- verify implementation maps are locators rather than architecture owners

Do not use line count or token count as proof of quality.

## Final audit

The plan is complete only when all statements are true:

- `docs/compiler-design-overview.md` is the complete mandatory core compiler authority.
- `docs/build-system-design.md` is the complete build and project orchestration authority.
- The compiler overview links to the build-system document without depending on it for core compiler semantics.
- The build-system document names the compiler overview as prerequisite reading.
- The project config bootstrap is single-file and import-free.
- Entry config still consumes normal module visibility.
- The project-root facade is a compiled API-only module and its assembly is separate.
- Structural graph edges and local declaration edges have different owners.
- Dormant roots are fully validated before activation.
- TIR wording matches the active final TIR plan exactly.
- AST and build-system generic ownership are not conflated.
- Implementation fingerprints cover exported and private executable bodies.
- `ProjectGlobalsInterface` is precise and contains no semantic runtime state.
- No accepted interview decision exists only in this plan or conversation history.
- Every active roadmap plan cites the correct architecture owner.
- No active roadmap plan retains rejected config, package, backend or incremental architecture.
- `AGENTS.md` routes readers correctly.
- Documentation follows the style guide.
- Validation commands and manual audits are recorded honestly.
- No repository file was deleted or heavily compressed before its destination contract was accepted.

## Completion record

When complete, update this section with:

```text
FINAL_COMMIT:
COMPILER_DOCUMENT_ACCEPTED:
BUILD_SYSTEM_DOCUMENT_ACCEPTED:
ROADMAP_PLANS_ALIGNED:
REFERENCE_AUDIT:
DOCS_VALIDATION:
LOST_CONTRACT_AUDIT:
KNOWN_UNRELATED_FAILURES:
```
