# Canonical module compilation and scoped packages implementation plan

## Purpose

Replace entry-closure compilation with canonical project and package graphs, immutable module artefacts, stable public interfaces, generated sidecars and explicit entry or package assemblies. Each physical module is semantically compiled once inside its project or package boundary.

## Current-state capsule

```text
ACTIVE_PLAN: docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md
STATUS: queued
CURRENT_SLICE: Phase 0 - refresh repository and freeze current owner maps
LAST_GOOD_COMMIT: none until the first implementation slice is accepted
BRANCH: main
IMPLEMENTATION_SCOPE: compiler frontend, build system, backends
```

## Hard prerequisites

- final TIR completion must be accepted so template folding and handoff are stable
- this plan must land before the HTML Wasm backend plan so backend work consumes a stable graph

## Required authority documents

- `docs/compiler-design-overview.md` for compiler stages, module artefact contents, semantic identities, fingerprints and target validation
- `docs/build-system-design.md` for Stage 0 graph construction, source sets, package topology, command policy, builders, link planning and output ownership
- `docs/language-overview.md` and relevant language module and import references for source syntax
- `docs/src/docs/codebase/memory-management/borrow-validation/borrow-validation.bd` for exported call summaries and cross-module borrow behaviour
- `docs/src/docs/codebase/style-guide/style-guide.bd`, `testing.bd` and `validation.bd`
- `docs/src/docs/progress/#page.bst` for current support
- the downstream config, entry-config and Wasm plans

## Accepted design to encode

This plan binds the following plan-local implementation decisions. Complete contracts live in the two architecture authorities. Each bullet references where the full contract lives.

Module and graph structure (see `docs/build-system-design.md` "Architectural invariants" and "Project and package topology"):

- one canonical graph per project or package boundary
- `OwnedSourceSet`, `SemanticSourceSet` and check-only orphan units
- one source index with canonical logical source identities
- strict module and package topology with `#*.bst` normal roots and `+*.bst` support roots
- scoped support packages with parent, sibling and descendant visibility rules
- a compiled project facade plus separate `ProjectPackageAssembly`
- separate Core, Builder and dependency source package graphs
- module-root-relative source imports with no `@./` or parent traversal

Stage 0 and config (see `docs/build-system-design.md` "Selected command and capability surface" and "Self-contained config.bst"):

- command and capability selection before config schema validation
- one self-contained `config.bst` result consumed by Stage 0
- `package_folders` and default `/lib` scanning do not exist

Compiler stages (see `docs/compiler-design-overview.md` "Frontend stages"):

- one tokenizer and header syntax-preparation pass
- later interface binding against completed provider interfaces
- structural provider references, imported symbol bindings and local ordering edges as separate types
- no deferred semantic compilation during linking

Module artefacts (see `docs/compiler-design-overview.md` "Compiled module artefact" and "Fingerprints and reuse facts"):

- `ModuleExecutable`, `ModuleLinkFacts` and `ModuleCompilerMetadata` as separate data lanes
- five base-module fingerprints: public-interface, implementation, dormant root-activity, runtime-dependency, documentation
- per-function link facts as the compiler's linking authority
- immutable binding-backed semantic interfaces
- stable origin identities and `ExportBinding`

Graph outcomes (see `docs/compiler-design-overview.md` "Module compilation outcomes" and `docs/build-system-design.md` "Graph compilation outcome"):

- `GraphCompilationOutcome` with successful, diagnosed and blocked lanes
- `Result<GraphCompilationOutcome, CompilerError>` for the graph boundary
- no partial interface on a diagnosed module
- canonical cross-module types
- public semantic interfaces without backend planning facts

Generated functions (see `docs/compiler-design-overview.md` "Generated concrete functions"):

- generated sidecars and fixed-point worklists
- generated instances live outside immutable base module artefacts

Project compilation (see `docs/build-system-design.md` "Success-only ProjectCompilation" and "Entry and package link planning"):

- success-only `ProjectCompilation`
- command-specific source and validation roots
- entry assemblies and package assemblies
- current implementation paths removed rather than wrapped

## Non-goals

- persistent artefact serialisation and precompiled caches
- package declaration syntax, registries, remote fetching or version solving
- final builder selection syntax or build-script design
- cross-page browser chunk sharing beyond physical variant reuse
- direct normal-sibling imports unless real project evidence justifies a later proposal
- physical Wasm module layout, which the Wasm plan owns

## Risks and blockers

- the active TIR plan may move template folding and handoff owners before this plan begins
- cross-module type identity, generic instances, trait evidence and receiver-method visibility are tightly coupled, so one boundary must not leak donor-local IDs through another
- current backend metadata is filtered from entry `start` reachability, but canonical reusable modules need per-function link facts
- a repository-wide import migration is unavoidable because current code and docs use `@./` and global entry-root paths
- strict support-package scopes may reveal real project-structure friction; record evidence for the deferred fallback rather than loosening inside this plan

## Current implementation inventory

Phase 0 must inspect and update these current owners. The existing path list is navigation only. Replace removed paths instead of preserving them through aliases.

Current owners to audit:

- source indexing: `src/build_system/create_project_modules/source_tree_index.rs`
- module root discovery: `src/compiler_frontend/paths/module_roots.rs`
- entry-closure inventory: `src/build_system/create_project_modules/module_inventory.rs::DiscoveredModule`
- import scanning: `src/compiler_frontend/paths/path_resolution.rs`
- path resolution: `src/compiler_frontend/paths/path_resolution.rs::ProjectPathResolver`
- source-package discovery: `src/build_system/create_project_modules/source_package_discovery.rs`
- provider resolution: `src/builder_surface/source_package_registry.rs`
- file preparation: `src/build_system/create_project_modules/frontend_orchestration.rs`
- interface binding: `src/compiler_frontend/headers/`
- local declaration sorting: `src/compiler_frontend/module_dependencies.rs`
- AST, HIR and borrow orchestration: `src/compiler_frontend/ast/`, `src/compiler_frontend/hir/`, `src/compiler_frontend/analysis/borrow_checker/`
- module payload construction: `src/build_system/build.rs::Module`
- backend handoff: `src/build_system/build.rs::BackendBuilder::build_backend`
- generic request collection: `src/compiler_frontend/ast/generic_functions/`
- check-only diagnostics: `src/projects/check.rs`
- output writing: HTML project builder output owners
- dev rebuild retention: `src/projects/dev_server/`

## Known implementation migrations

The plan must explicitly own these migrations. Each migration names the current state, the accepted replacement and the phase that removes it.

Module payload and backend handoff:
- replace `Vec<Module>` backend handoff with `ProjectCompilation` (Phase 9 and 10)
- replace per-entry source closures with canonical module compilation (Phase 5 and 11)
- remove repeated semantic compilation of shared modules (Phase 5 and 11)
- remove duplicate diagnostic production from shared module failures (Phase 6 and 11)

Config and packages:
- remove `package_folders` config fields and scanning (Phase 3 and 11)
- remove default `/lib` discovery (Phase 3 and 11)
- remove entry-root fallback import resolution (Phase 3 and 11)
- remove source `@./` importing-file-relative resolution (Phase 3 and 11)
- remove importing-file-relative provider fallback where not owned by a provider contract (Phase 3 and 11)

Builder state:
- split mutable provider build state from reusable builder capability definitions (Phase 5)

HIR and artefact lanes (see `docs/compiler-design-overview.md` "Compiled module artefact"):
- replace unconditional `start_function` sentinels with API-only roots that have no implicit start (Phase 6)
- move warnings out of HIR into `ModuleCompilerMetadata` (Phase 6)
- move documentation fragments out of HIR into `ModuleCompilerMetadata` (Phase 6)
- move runtime path and asset facts to `ModuleLinkFacts` (Phase 6)
- keep compile-time page fragments outside HIR, in `ModuleCompilerMetadata` (Phase 6)
- stop carrying a complete mutable external registry inside each compiled module (Phase 6 and 7)

Frontend and tooling:
- stop `CompilerFrontend` from reading broad builder config maps (Phase 4)
- remove `FileKind::NotBuilt` if check no longer needs a fake backend result (Phase 11)
- update `index.md` as owners move (Phase 12)

## Implementation phases

Each phase must leave one coherent path and include focused tests. Reference the named sections in `docs/compiler-design-overview.md` and `docs/build-system-design.md` for full contracts rather than restating them here.

### Phase 1: Refresh the repository and freeze current owner maps

Context: this plan was authored while the TIR plan was active. Starting from stale paths would create bad APIs around code that no longer exists.

- Confirm TIR finalisation is accepted and template folding and handoff are stable.
- Record `git rev-parse HEAD`, branch and `git status --short` in the context capsule.
- Re-read every path in the current implementation inventory above and replace stale files or symbols.
- Reconcile the final TIR handoff: exported const templates cross module interfaces only as folded owned facts, never TIR identities.
- Search current source for `DiscoveredModule`, reachable entry closures, `Vec<Module>` backend handoff, `package_folders`, `@./` and `@` import resolution.
- Classify each old owner as replace, extend or delete. Record the result before code starts.

### Phase 2: Introduce stable project, package, module and source identities

Context: separate compilation is unsafe until every consumer-facing fact can survive outside the declaring module's local arenas.

See `docs/compiler-design-overview.md` "Stable semantic identities" and "Type identity" for the full identity contracts.

- Introduce `ModuleId` values assigned in deterministic canonical path order.
- Introduce stable declaration identities for exported functions, nominal types, constants, traits and reusable evidence (conceptual `OriginDeclarationId`, `OriginFunctionId`, etc, exact names may change).
- Record root kind (Normal, Support, ProjectPackageFacade), root directory, root file and source-relative logical path.
- Record the nearest ancestor module as structural parent and direct child modules by nearest-module ancestry.
- Represent the optional project facade as a special node outside the entry-root containment tree.
- Keep module identity independent of root suffix text (the suffix after `#` or `+` is cosmetic).

### Phase 3: Build canonical source indexes and owned or semantic source sets

Context: visibility and compilation reuse depend on a complete structural model prepared once.

See `docs/build-system-design.md` "Source indexing and source sets" for the full contracts.

- Build one canonical source index after config supplies `entry_root`.
- Assign every recognised source file to its nearest containing module root.
- Treat unrooted subdirectories as internal directories of the same module.
- Build `OwnedSourceSet` (every recognised source file whose nearest root is that module) and `SemanticSourceSet` (root file, reachable `.bst` files, builder-supported assets like `.bd` or `.md`).
- Build check-only orphan source units for `check` (owned `.bst` files not in the semantic source set).
- Reject `package_folders` and default `/lib` scanning. Project-local source packages are structural `+*.bst` packages or the optional project-root facade.

### Phase 4: Split syntax preparation from interface binding

Context: header work has two explicit phases so syntax is parsed once without pretending provider interfaces already exist.

See `docs/compiler-design-overview.md` "Stage 2: header syntax and interface binding" for the full contract.

- `PreparedHeaderSyntax` is produced before the provider graph compiles: declaration shells, import shells, structural provider references, local ordering hints, root-activity metadata, source `#Import` contract shells.
- `BoundModuleHeaders` is produced after required providers compile: stable imported identities, canonical types, final visibility, collision results.
- Keep structural provider references (Stage 0), imported symbol bindings (visibility and AST) and local declaration-ordering edges (Stage 3) as separate data classes.
- Binding does not retokenize source or reparse declaration syntax.

### Phase 5: Build deterministic project, package and provider graphs

Context: the compiler cannot schedule canonical modules until Stage 0 can distinguish normal modules, support packages and the optional project package facade.

See `docs/build-system-design.md` "Project and package topology" and "Deterministic scheduling and graph outcomes" for the full contracts.

- Build `ProjectModuleGraph` with module nodes, compile order, entry modules and optional project package facade.
- Compute support-package scope visibility: visible to owner, normal siblings and sibling descendants, not visible above owner or outside owner's subtree.
- Validate structural dependency edges and add a defensive cycle validator.
- Compile support private subtrees before their facade, normal child and support dependencies before consumers, and the project-root facade last.
- Keep independent branches available for parallel scheduling with deterministic ID assignment, string-table delta merge and diagnostic ordering.

### Phase 6: Add graph outcomes and immutable module artefact lanes

Context: successful and failed module results must be explicit data classes so tooling can inspect independent branches while builders receive only success.

See `docs/compiler-design-overview.md` "Module compilation outcomes" and "Compiled module artefact" for the full contracts.

- Introduce `ModuleCompilationOutcome` with `Success(CompiledModuleArtifact)` and `Diagnosed(ModuleDiagnostics)`.
- A diagnosed module exposes no partial public interface.
- Build `CompiledModuleArtifact` with four data lanes plus fingerprints: `PublicSemanticInterface`, `ModuleExecutable`, `ModuleLinkFacts`, `ModuleCompilerMetadata`, `ModuleFingerprints`.
- Record five base-module fingerprints: public-interface, implementation, dormant root-activity, runtime-dependency, documentation.
- Build `GraphCompilationOutcome` with successful, diagnosed and blocked lanes.
- Use `Result<GraphCompilationOutcome, CompilerError>` for the graph boundary.

### Phase 7: Add stable public interfaces, cross-module calls and effect summaries

Context: a public interface contains only facts a semantic consumer may observe.

See `docs/compiler-design-overview.md` "Public semantic interfaces" and "Stable semantic identities" for the full contracts.

- Build `PublicSemanticInterface` with exported origin identities, `ExportBinding`, canonical type shapes, folded constants, generic templates, trait evidence, receiver surfaces, access and effect summaries, project-context provenance.
- Backend planning facts do not belong in this interface. Per-function calls, helper requirements and runtime assets live in `ModuleLinkFacts`.
- Replace donor-local `TypeId` with canonical cross-module type identities. Each consumer may intern compact local `TypeId` handles for imported canonical types.
- HIR represents cross-module calls with explicit stable module-function targets. The callee body is never copied into the caller.
- Borrow validation runs once per canonical module and once per generated function. Public function interfaces carry parameter access modes, mutation, consumption, return aliasing and reactive effects.
- Cross-module call transfer consumes summaries without opening the callee's HIR.

### Phase 8: Add generated sidecars and the fixed-point worklist

Context: canonical module compilation removes repeated provider work, but consumer-local generic instance emission would still duplicate concrete functions.

See `docs/compiler-design-overview.md` "Generated concrete functions" for the full contract.

- Key generated requests by stable generic declaration identity, canonical concrete type identities and required evidence identities.
- Build project-owned generated sidecar storage associated with the declaring module. Keep the base compiled module artefact immutable.
- Materialise concrete AST and HIR bodies using the declaring template and canonical substitution. Borrow-check each generated function.
- Process requests through a deduplicating worklist because generated bodies may request more instances.
- Continue until the worklist reaches a fixed point.
- A diagnosed generated request blocks only entries or package surfaces that require it.

### Phase 9: Build success-only ProjectCompilation, entry assemblies and package assemblies

Context: a project builder never receives diagnosed or blocked required modules.

See `docs/build-system-design.md` "Success-only ProjectCompilation" and "Entry and package link planning" for the full contracts.

- Assemble `ProjectCompilation` only when every artefact required by selected entries or package surface succeeded.
- Build `EntryAssembly` for each active normal module: activate only that module's dormant `start`, runtime fragments, compile-time fragments and resolved entry settings.
- Build `ProjectPackageAssembly` over the compiled facade artefact, selected descendant public interfaces, reachable generated functions and permitted runtime requirements.
- Entry and package assembly never trigger parsing, type checking, HIR generation, generic inference or borrow validation.
- The implicit `start` is non-exported, non-importable and infallible.

### Phase 10: Migrate build, dev, check, HTML and backend consumers

Context: backends must consume canonical artefacts without rediscovering module relationships.

- Replace `BackendBuilder::build_backend(Vec<Module>, ...)` with the graph-aware `ProjectCompilation` payload.
- Update HTML builder to consume entry assemblies and linked module calls.
- Update `check` to compile every discovered project module, check-only orphan units, the facade and required source package graphs.
- Update `dev` to reuse successful in-memory artefacts according to fingerprint and invalidation rules.
- Preserve current page routing, fragment interleaving and JS behaviour during migration.

### Phase 11: Delete entry-closure, flat payload and fallback import paths

Context: the refactor is not complete while old entry closures, configured source-package discovery, path fallbacks or consumer-local generic emission remain available. This phase is deletion-first.

- Delete `DiscoveredModule` and entry-closure compile loops.
- Delete flat `Vec<Module>` backend interfaces.
- Delete `ProjectPathResolver` importing-file-relative and entry-root fallback.
- Delete `package_folders` config fields, scanning and tests.
- Delete default `/lib` discovery.
- Delete imported declaration and body copying.
- Delete donor-local cross-module type transport.
- Delete consumer-local generic materialisation.
- Delete borrow paths that inspect foreign HIR.
- Delete backend paths that infer call or package identity from rendered names.
- Delete `FileKind::NotBuilt` if check no longer needs a fake backend result.
- Delete adapters and compatibility aliases introduced during migration.

### Phase 12: Migrate fixtures, scaffolding, docs and the progress matrix

Context: documentation must teach the accepted import model and project structure.

- Update `docs/language-overview.md` with the final `#` and `+` root model.
- Update project-structure, package and import source pages.
- Add a complete project tree showing normal modules, support packages, private package subtrees and a root package facade.
- Add legal and illegal dependency examples.
- Explicitly state that imports are module-root-relative, not file-relative.
- Explicitly reject `@./` and `..` examples.
- Update `bean new` output to remove `lib` and `package_folders` assumptions.
- Update progress matrix rows for implemented and deferred features.
- Rebuild generated documentation through the compiler.

## Old owners and paths to remove

- `DiscoveredModule` and entry-closure compile loops
- `BackendBuilder::build_backend(Vec<Module>, ...)` flat handoff
- `ProjectPathResolver` importing-file-relative and entry-root fallback
- `package_folders` config fields, scanning and tests
- default `/lib` discovery
- `source_package_discovery.rs` project-local package scanning
- root public-surface fallback and path-base variants
- imported declaration and body copying
- donor-local cross-module type transport
- consumer-local generic materialisation
- borrow paths that inspect foreign HIR
- backend paths that infer call or package identity from rendered names
- `FileKind::NotBuilt` if check no longer needs a fake backend result

## Required tests

Cover:

- one shared module compiled once for multiple entries
- one diagnostic set for a shared failure
- independent branches continuing after one diagnosis
- blocked consumers receiving no secondary name or type cascades
- check-only orphan source units not entering module artefacts
- stable identities under source-file moves and declaration reordering
- module-root-relative imports
- strict support-package scopes
- project-facade assembly
- dependency and Builder package graph separation
- generated instance reuse
- cross-module borrow effects
- deterministic ordering under parallel scheduling
- no source or provider fallback to `@./`
- no API-only sentinel start
- artefact-lane validation

## Documentation and progress-matrix impact

- update `docs/language-overview.md` with the final `#` and `+` root model
- update project-structure, package and import source pages
- update `docs/compiler-design-overview.md` stage ownership only where code ownership moves
- update `index.md` as owners move
- progress matrix rows: normal modules, support packages, project facade, module-root-relative imports, compile-once artefacts, generic reuse, graph backend handoff, deferred persistent caches and deferred sibling imports

## Validation requirements

Each code-bearing phase runs:

```bash
cargo fmt
just validate
just bench-check
```

Run the documentation release build when source docs change.

## Final architecture audit

Before marking this plan complete, verify:

- one canonical graph per project or package boundary is the only production path
- no entry-closure, flat payload or fallback import path remains
- public interfaces use stable identities, not donor-local IDs
- generated functions use sidecars, not base module mutation
- entry and package assembly never trigger semantic compilation
- the Wasm plan can start without redesigning frontend modules
- source, tests, docs, progress matrix and roadmap agree

## Deliberately deferred work

- persistent module and package artefact serialisation
- source-hash and public-interface-hash invalidation policy
- retained dev-server module artefacts across rebuilds
- cross-project dependency declarations and local path dependencies
- package registries, remote fetching, versions and lockfiles
- precompiled dependency package caches
- direct normal-sibling imports with cycle detection
- cross-entry JavaScript chunking and shared browser bundles
- physical Wasm module partition and Component Model integration
- cross-build generic instance caches

The module graph, stable identities and dependency records implemented here must make those features possible without another frontend ownership rewrite.
