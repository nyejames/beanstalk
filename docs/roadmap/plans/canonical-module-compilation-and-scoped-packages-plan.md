# Canonical module compilation and scoped packages implementation plan

## Purpose

Replace entry-closure compilation with canonical project and package graphs, immutable module artefacts, stable public interfaces, generated sidecars and explicit entry or package assemblies. Each physical module is semantically compiled once inside its project or package boundary.

## Current state

```text
ACTIVE_PLAN: docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md
STATUS: active
CURRENT_SLICE: Phase 7c2b2 complete; accepted checkpoint ready to commit
LAST_ACCEPTED_COMMIT: e45d743a2 (Phase 7c2b1 transient AST-owned resolved public type roots)
WORKTREE: main at a76770e1b; unrelated docs-only commit above e45d743a2 preserved; accepted Phase 7c2b2 code/tests plus this plan checkpoint are unstaged
REQUIRED_RELOADS: startup files, this plan, semantic identity and direct-export origins, TypeEnvironment and AST lookup owners, retained header binding, graph scheduling and current module-result owners
RELEVANT_CONTEXT_NOW:
- docs: compiler-design-overview.md makes AST the owner of public-interface validation and canonical export projection; donor-local TypeIds must not cross the module result boundary
- code: Ast now carries required ResolvedPublicTypeRootTable plus TypeEnvironment before HIR; DefinedPublicExportOriginDraft carries stable declaration origins and receiver origins finalize from the same resolved receiver catalog
ACCEPTANCE_CRITERIA:
- met: one production type-only public-surface projector consumes retained AST roots, stable export origins and canonical type projection before HIR
- met: nominal and generic-parameter origin joins are total; missing, duplicate, ambiguous and category-mismatched facts fail through CompilerError
- met: free functions, nominal fields/variants, aliases, constants and receiver methods project deterministically to owned canonical values
- met: retained public export targets admit direct, imported and public-alias-target nominals without exposing private alias-target receiver methods
- met: donor-local public roots are taken before HIR and only the stable type surface is retained beside a successful module result
VALIDATION_STATE:
- Phase 4d just validate: passed; cross-target Clippy, 3419 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases
- Phase 4e focused validation: passed; 2 import-scanning, 5 reachability, 178 module-discovery, 19 orchestration, 5 token-remap and 116 header tests plus cargo check --tests and git diff --check
- Phase 4e just validate: passed; cross-target Clippy, 3423 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (-1ms average)
- Phase 4f focused validation: passed; 20 orchestration, 25 frontend-compilation, 179 module-discovery and 116 header tests plus cargo check --tests, cargo clippy --tests -D warnings and git diff --check
- Phase 4f just validate: passed; cross-target Clippy, 3424 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (-1ms average)
- Phase 5a focused validation: passed; 9 graph and 188 create-project-modules tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 5a just validate: passed; cross-target Clippy, 3433 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (-1ms average)
- Phase 5b focused validation: passed; 9 graph and 192 create-project-modules tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 5b just validate: passed; cross-target Clippy, 3437 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (-1ms average)
- Phase 6a focused validation: passed; 210 HIR, 20 frontend-orchestration and 226 HTML-project tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 6a just validate: passed; cross-target Clippy, 3437 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (+1ms average)
- Phase 6b focused validation: passed; 1 lane invariant, 20 frontend-orchestration, 25 project-frontend, 226 HTML-project and 227 backend tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 6b just validate: passed; cross-target Clippy, 3438 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (+1ms average)
- Phase 6c focused validation: passed; 104 compiler-message, 20 frontend-orchestration and 25 project-frontend tests plus full 3444-test unit suite, cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 6c just validate: passed; cross-target Clippy, 3447 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (0ms average)
- Phase 7a/7b focused validation: passed; 8 defined-export-origin, 18 semantic-identity, 116 header, 30 compiler-frontend, 20 frontend-orchestration, 100 module-discovery and 50 Stage 0 identity tests plus full 3458-test unit suite, all 8 receiver regressions and the full 1793-case integration suite, cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 7a/7b just validate: passed after resolved-receiver correction; cross-target Clippy, 3458 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (-1ms average)
- Phase 7c1 focused validation: passed after atomic reverse-identity and generic-base corrections; 78 canonical-type/external-registry tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 7c1 just validate: passed after clearing generated Cargo artifacts following a no-space environmental failure; cross-target Clippy, 3497 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (0ms average)
- Phase 7c2a focused validation: passed after closing receiver-method owner construction; 42 canonical identity tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 7c2a just validate: passed; cross-target Clippy, 3507 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (-1ms average)
- Phase 5c focused validation: passed after removing flat test-only compatibility APIs; 101 create-project-modules/frontend-orchestration tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 5c just validate: passed; cross-target Clippy, 3508 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (+1ms average)
- Phase 5d1 focused validation: passed after active-origin correction; 121 defined-export-origin, source-module-origin, frontend-orchestration and create-project-modules tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 5d1 just validate: passed; cross-target Clippy, 3521 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (0ms average)
- Phase 7c2b1 focused validation: passed after receiver-selection, alias-owner and required-handoff corrections; 69 public-type-root/public-surface/canonical-identity tests plus cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 7c2b1 just validate: passed; cross-target Clippy, 3530 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (+1ms average)
- Phase 7c2b2 focused validation: passed; 17 defined-export-origin, 28 defined-public-type-surface, 7 public-surface, 9 resolved-root, 42 canonical-identity and 21 frontend-orchestration tests; alias/private-receiver regression 1/1; cargo check --tests, cargo clippy --tests -D warnings, formatting and git diff --check
- Phase 7c2b2 just validate: passed; cross-target Clippy, 3563 Rust tests, 1793 integration executions, docs check and 28/28 benchmark cases (+1ms average)
DOCS_IMPACT: index.md names the canonical cross-module type identity and projection owner; progress matrix unchanged for internal interface groundwork
BLOCKERS_OR_OPEN_DECISIONS: trait/evidence identities, folded constant values, access/effect summaries, provenance and provider interface binding remain later PublicSemanticInterface facts; no blocker for this checkpoint
DELEGATION_DECISION: ollama - user requires Ollama for every worker slice
NEXT_WORKER_ORDER: ollama only; no provider substitution for this run
STOP_REASON: none
NEXT_RESUME_ACTION: commit the accepted Phase 7c2b2 checkpoint, reload startup/plan/source and select the next bounded Phase 7 PublicSemanticInterface slice through Ollama
```

## Hard prerequisites

- final TIR completion and its one-store, exact-view folding/handoff architecture are accepted at `1298da468`
- the mandatory post-TIR roadmap review checkpoint is complete and recorded against `1298da468`
- compiler test-suite hardening completed its final audit corrections, documentation build and full validation at `0e6b1cf13`
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
- per-function project-context provenance sufficient to reject every package export whose semantic facts or reachable implementation depend on private `@project`
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

- final TIR owners are fixed for this plan: consume the accepted preparation, fold and owned-handoff boundaries without reopening TIR architecture
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

### Phase 1 refreshed owner map

The accepted post-TIR anchor `1298da468` and hardened-suite anchor `0e6b1cf13` are ancestors of
`3be652bd230dd5c64d90d63fa2348651ceea4b4b`. No later accepted compiler or build-system authority
change supersedes them.

- **Replace** `DiscoveredModule`, `module_inventory.rs` and the per-entry closure compilation in
  `compilation.rs` with graph nodes, owned/semantic source sets and dependency-ordered jobs.
- **Extend then consolidate** `source_tree_index.rs` as the single Stage 0 traversal. Its current
  index records only `#*.bst` roots and entry candidates, so it must add `+*.bst` roles, canonical
  module identities, file ownership, structural ancestry, project-facade discovery and namespace
  facts without adding another filesystem scan.
- **Replace** the current `ModuleRootTable` identity model in
  `compiler_frontend/paths/module_roots.rs`. Keep its prepared nearest-root lookup behaviour, but
  move durable project/module identity and topology ownership to Stage 0.
- **Replace** `ProjectPathResolver` fallback semantics while reusing narrow path normalisation,
  source-kind candidate and diagnostic helpers. The current resolver still selects importing-file
  relative paths for `@./`, falls back to `entry_root` and discovers module public surfaces by
  walking resolved filesystem paths.
- **Delete** configured project-local package scanning in `source_package_discovery.rs`, the
  `package_folders` config surface and default package-folder assumptions. **Extend**
  `SourcePackageRegistry` only for builder/Core/dependency source package capability definitions;
  scoped project support packages belong to the project graph.
- **Split and extend** `frontend_orchestration.rs` and `headers/`: retain the one token/header
  preparation path, deterministic string-table merge and local declaration parsing, but separate
  provider-independent prepared syntax from later interface binding. Remove broad `Config` and
  mutable project resolver ownership from `CompilerFrontend` inputs as the new boundaries land.
- **Extend** `module_dependencies.rs` as the Stage 3 local-order owner. Imported provider symbols
  must leave its graph once bound interfaces replace the current combined header environment.
- **Replace** `build.rs::Module`, `CompiledModuleResult` and
  `BackendBuilder::build_backend(Vec<Module>, ...)` with explicit module outcomes, immutable
  artefact lanes, graph outcomes and `ProjectCompilation`.
- **Reshape** AST/HIR/borrow handoff behind immutable artefacts. `HirModule` now retains only its
  mandatory `start_function` among the previously mixed concerns; warnings, documentation
  fragments and rendered-path usages have moved to `ModuleCompilerMetadata`, while API-only roots
  still need the sentinel start removed.
- **Replace** consumer-local generic instance emission in
  `ast/generic_functions/` and `module_ast/emission/emitter.rs` with stable requests, project-owned
  generated sidecars and a fixed-point worklist.
- **Replace** `projects/check.rs`'s all-or-error `Vec<Module>` path with graph outcome inspection and
  check-only orphan units. **Extend** output writing as the central manifest owner.
- **Replace** HTML's flat module loop and whole-module reachability assumptions with entry
  assemblies and graph link plans. **Replace** dev's compile-everything rebuild path with successful
  artefact retention after fingerprints exist.

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

Context: this plan was authored while TIR was active and was refreshed against `1298da468` after
the mandatory review. Phase 1 still rechecks current paths before code starts, but it must consume
the accepted owners rather than redesign them. The hardened suite at `0e6b1cf13` has 1,647
canonical cases, 1,793 backend executions, explicit role ownership, zero hard policy findings and
3,359 focused Rust tests. New coverage must extend those owners rather than recreate whole-source
unit substitutes.

- Confirm the `1298da468` post-TIR review remains the current accepted anchor and no later accepted architecture change supersedes it.
- Record `git rev-parse HEAD`, branch and `git status --short` in the context capsule.
- Re-read every path in the current implementation inventory above and replace stale files or symbols.
- Preserve the final TIR handoff: exported const templates cross module interfaces only as folded owned facts or neutral owned runtime payloads, never TIR identities, views, overlays or preparation state.
- Search current source for `DiscoveredModule`, reachable entry closures, `Vec<Module>` backend handoff, `package_folders`, `@./` and `@` import resolution.
- Classify each old owner as replace, extend or delete. Record the result before code starts.
- Reuse the canonical integration owners for check/build parity, imported-root suppression, facade effect summaries, diagnostic remapping and target reachability. Add a case only for a distinct graph or module-boundary contract.
- Run `cargo run --quiet -- tests --audit` after fixture metadata changes and preserve the single role/contract/suite-policy owner.

### Phase 2: Introduce stable project, package, module and source identities

Context: separate compilation is unsafe until every consumer-facing fact can survive outside the declaring module's local arenas.

See `docs/compiler-design-overview.md` "Stable semantic identities" and "Type identity" for the full identity contracts.

- Introduce `ModuleId` values assigned in deterministic canonical path order.
- Introduce stable declaration identities for exported functions, nominal types, constants, traits and reusable evidence (conceptual `OriginDeclarationId`, `OriginFunctionId`, etc, exact names may change).
- Record root kind (Normal, Support, ProjectPackageFacade), root directory, root file and source-relative logical path.
- Record the nearest ancestor module as structural parent and direct child modules by nearest-module ancestry.
- Represent the optional project facade as a special node outside the entry-root containment tree.
- Keep module identity independent of root suffix text (the suffix after `#` or `+` is cosmetic).

Accepted Phase 2a checkpoint:

- `module_identity.rs` is the Stage 0 owner of deterministic `ModuleId`, root roles, logical
  module paths and nearest-module ancestry.
- `SourceTreeIndex` builds the durable identity table and derives the existing normal-root-only
  frontend resolver table from it, so support and facade discovery does not change current import
  behaviour.
- normal, support and project-facade roles, cosmetic suffix stability, ancestry, entry-candidate
  exclusion and facade filesystem failures have focused subsystem coverage.
- cross-build package/module origins and exported declaration identities remained after Phase 2a.

Accepted Phase 2b checkpoint:

- `StablePackageIdentity` owns package origin plus the configured canonical project/package name.
- `StableModuleOriginIdentity` owns that package identity, a portable forward-slash logical module
  path and root role. It excludes absolute paths, ordinary source-file paths, string-table IDs and
  dense build-local `ModuleId` values.
- invalid, absolute, parent and non-UTF-8 logical paths return internal compiler errors rather than
  panicking or collapsing into another identity.
- stable exported declaration identities remain for Phase 2c.

Accepted Phase 2c checkpoint:

- `compiler_frontend::semantic_identity` is the single compiler-semantic owner of portable package,
  module and exported declaration origin values; Stage 0 imports those values while retaining dense
  `ModuleId` assignment, discovery and topology ownership.
- stable exported origin IDs cover free functions, receiver methods, structs, choices, transparent
  aliases, constants and traits without source files, declaration order, export aliases or local IDs.
- receiver methods embed their stable receiver type in one `FunctionOriginKind` state, so invalid
  free/receiver combinations are unrepresentable.
- reusable evidence identity is intentionally deferred to Phase 7, where canonical target types and
  trait/evidence semantics exist; Phase 2 does not introduce a string or placeholder identity.

### Phase 3: Build canonical source indexes and owned or semantic source sets

Context: visibility and compilation reuse depend on a complete structural model prepared once.

See `docs/build-system-design.md` "Source indexing and source sets" for the full contracts.

- Build one canonical source index after config supplies `entry_root`.
- Assign every recognised source file to its nearest containing module root.
- Treat unrooted subdirectories as internal directories of the same module.
- Build `OwnedSourceSet` (every recognised source file whose nearest root is that module) and `SemanticSourceSet` (root file, reachable `.bst` files, builder-supported assets like `.bd` or `.md`).
- Build check-only orphan source units for `check` (owned `.bst` files not in the semantic source set).
- Reject `package_folders` and default `/lib` scanning. Project-local source packages are structural `+*.bst` packages or the optional project-root facade.

Accepted Phase 3a checkpoint:

- the existing `SourceTreeIndex::discover` walk inventories `.bst` plus selected builder-supported
  `.bd`/`.md` candidates; it does not add a second filesystem traversal.
- `StableOwnedSourceIdentity` combines stable module origin with a validated, non-empty portable
  module-relative source path while leaving final dense `SourceId`/`SourceDatabase` ownership to the
  later source-data-layout plan.
- one post-discovery classifier builds deterministic `OwnedSourceSet` values by nearest normal or
  support root, transfers nested-module files to the nested owner and assigns the project facade root
  exactly once, including the temporary project-root-equals-entry-root compatibility case.
- supported candidates without a containing module remain explicit portable, deterministically
  ordered facts. Phase 3a adds no orphan diagnostic and changes no resolver, import, reachability or
  compilation behavior.

### Phase 4: Split syntax preparation from interface binding

Context: header work has two explicit phases so syntax is parsed once without pretending provider interfaces already exist.

See `docs/compiler-design-overview.md` "Stage 2: header syntax and interface binding" for the full contract.

- `PreparedHeaderSyntax` is produced before the provider graph compiles: declaration shells, import shells, structural provider references, local ordering hints, root-activity metadata, source `#Import` contract shells.
- `BoundModuleHeaders` is produced after required providers compile: stable imported identities, canonical types, final visibility, collision results.
- Keep structural provider references (Stage 0), imported symbol bindings (visibility and AST) and local declaration-ordering edges (Stage 3) as separate data classes.
- Binding does not retokenize source or reparse declaration syntax.

Accepted Phase 4a checkpoint:

- `prepare_header_syntax` consumes remapped per-file outputs and produces provider-independent
  `PreparedHeaderSyntax` with retained header/import shells, order-independent symbol facts,
  root-activity metadata and frontend statistics.
- `bind_module_headers` consumes that retained value and produces `BoundModuleHeaders` with public
  export resolution, bound file visibility and completed dependency facts. It has no source text or
  tokenization input.
- production module compilation, config compilation and the direct Beandown service use the two
  explicit calls; the old combined `Headers` type and `parse_headers` entry point were removed
  without aliases or wrappers.
- the current production calls remain adjacent until Stage 0 retains prepared syntax across source
  provider scheduling. Structural source-provider references and their separation from local
  ordering hints remain the next prerequisite.

Accepted Phase 4b checkpoint:

- `StructuralProviderReference` is the shared import-clause syntax value for one normalized
  provider path and its exact source location; both Stage 0 scanning and retained `FileImport`
  shells consume it directly.
- `FileImport` embeds the structural reference while keeping alias, clause, grouping and export
  metadata separate for later binding; the parallel `header_path` and `path_location` fields and
  the old path-only scan APIs were removed without wrappers.
- reachable discovery currently resolves `provider.path` while retaining the location for the
  graph boundary. The config source-set caller mechanically migrated to the same scan and projects
  the path locally because preserving its former path-only API would duplicate the obsolete path.
- exact path-location retention and nested string-ID remapping have focused coverage. Import
  behavior, diagnostics, visibility and deterministic order are unchanged.

Accepted Phase 4c checkpoint:

- `LocalDeclarationOrderingHint` is the retained declaration-shell vocabulary for conservative
  type-surface and constant-initializer paths; `Header.local_ordering_hints` no longer presents
  these facts as already-proven dependency edges.
- `headers/ordering_hints.rs` records import or same-file spellings without consulting provider
  availability. `StructuralProviderReference`, `FileImport` binding metadata and local ordering
  hints are type-distinct production data.
- interface binding canonicalizes source import spellings through bound visibility and drops
  external or binding-only hints. Stage 3 is the sole owner that resolves the remaining hints into
  sortable dependency edges.
- the former `dependency_edges.rs` collection owner and generic `Header.dependencies` field were
  removed; `index.md` names the retained-syntax, hint and binding responsibilities.
- typed remapping, external-hint removal and existing local ordering have focused coverage.
  Diagnostics, declaration order and current language behavior are unchanged.

Accepted Phase 4d checkpoint:

- per-file token and declaration-shell preparation no longer receives `ExternalPackageRegistry`;
  `FrontendFilePrepareContext`, `HeaderParseContext`, `HeaderBuildContext` and the token-input
  preparation APIs contain no provider-interface plumbing.
- syntax preparation retains declarations and generic parameters uniformly. `bind_module_headers`
  validates prelude-function declaration names and prelude-type generic parameters from those
  retained shells once the provider interface exists.
- the binding-owned checks preserve `BST-RULE-0027` for reserved prelude-function declarations and
  `BST-RULE-0043` with the generic-parameter collision reason, including authored source locations.
- import-alias generic collisions remain syntax-owned, while same-file and imported visible-type
  generic collisions remain AST-owned. No duplicate provider-aware syntax path or compatibility
  argument remains.

Accepted Phase 4e checkpoint:

- Stage 0 retains each discovered Beanstalk file's exact `FileTokens` beside its source text and
  structural provider references. `PreparedSourceInput` represents Beanstalk, Beandown and plain
  Markdown as type-distinct states, so only Beanstalk can carry retained tokens.
- frontend file preparation rebinds retained token source identity to the module `SourceFileTable`
  and parses it without another lexical pass. Beandown still tokenizes its template body exactly
  once and plain Markdown remains non-tokenized.
- provider-free classification completes the reachable local graph and retains one shared scan
  cache. Provider-free workers and provider-required serial replay consume that cache without
  rereading or retokenizing Beanstalk source while preserving deterministic string-table identity.
- the discarded-token scan path, discovered-Beanstalk frontend retokenization and obsolete raw
  `InputFile` payload were removed. Focused tests cover source reads, retained-token state and
  nested path-location rebinding without changing import or diagnostic behavior.

Accepted Phase 4f checkpoint:

- `ModulePreparationContext` owns provider-independent source identity, per-file preparation and
  module-wide `prepare_header_syntax`. Its type contains no package registry, provider resolution
  table or builder runtime interface, and it does not construct a semantic `CompilerFrontend`.
- `PreparedModule` retains `PreparedHeaderSyntax`, its deterministic module string table, source
  identity table, preparation warnings and capacity facts. It carries no source text or tokens.
- `FrontendModuleBuildContext::compile_module_semantic` consumes the retained payload and begins
  with provider-dependent `bind_module_headers`, followed by local ordering, AST, HIR and borrow
  validation. The obsolete combined `compile_module` path was removed.
- single-file and directory orchestration call the two contexts explicitly, leaving a typed
  scheduling boundary for Phase 5. Source logical paths now come from the retained
  `SourceFileTable`, and focused coverage guards the provider-free preparation API.

### Phase 5: Build deterministic project, package and provider graphs

Context: the compiler cannot schedule canonical modules until Stage 0 can distinguish normal modules, support packages and the optional project package facade.

See `docs/build-system-design.md` "Project and package topology" and "Deterministic scheduling and graph outcomes" for the full contracts.

- Build `ProjectModuleGraph` with module nodes, compile order, entry modules and optional project package facade.
- Compute support-package scope visibility: visible to owner, normal siblings and sibling descendants, not visible above owner or outside owner's subtree.
- Validate structural dependency edges and add a defensive cycle validator.
- Compile support private subtrees before their facade, normal child and support dependencies before consumers, and the project-root facade last.
- Keep independent branches available for parallel scheduling with deterministic ID assignment, string-table delta merge and diagnostic ordering.

Accepted Phase 5a checkpoint:

- `ProjectModuleGraph` is the canonical Stage 0 structural owner built once from
  `SourceTreeIndex`; it retains deterministic `ModuleId` nodes, stable origins, root roles and
  files, nearest ancestry, direct children and owned source sets without another traversal or
  identity table.
- normal entry classification, optional facade identity, scoped-support visibility, validated
  provider-before-consumer edge insertion and deterministic compile waves live on that graph.
  Strict support scope permits imports from a strictly outer support scope while rejecting
  private descendants, same-scope support siblings and modules outside the owning normal subtree.
- module inventory consumes graph entry and wave order, and the old `SourceTreeIndex`
  `entry_candidates` path was removed. Import-derived edges and dependency-ordered semantic jobs
  remain Phase 5b.
- focused graph tests own hidden topology, visibility, ordering and defensive-failure invariants;
  existing discovery tests consume the production graph entry owner.

Accepted Phase 5b checkpoint:

- reachable discovery retains a `LocalStructuralDependencyFact` at the existing local import
  resolution join when an authored `StructuralProviderReference` crosses normal project module
  roots. The fact carries canonical consumer/provider roots plus the exact authored location; no
  import reparse, alternate resolver or location-based edge identity was added.
- serial and provider-free discovery return the same fact shape. Module inventory merges facts in
  deterministic root-pair order, and `ProjectModuleGraph` maps canonical roots, inserts
  idempotent provider-before-consumer edges and retains deterministic edge provenance.
- current normal-entry inventories are returned in populated compile-wave order. The directory
  frontend still submits them to its existing Rayon batch, so true dependency-wave semantic
  scheduling and canonical per-node compilation remain the next Phase 5 slice.
- graph/inventory mismatch and absent fact roots fail through release-safe internal
  `CompilerError` boundaries. Focused tests cover edge direction, same-module exclusion,
  duplicate fan-in, deterministic ordering and exact source-location retention.

Accepted Phase 5c checkpoint:

- `ModuleEntryCompileWaves` is the one wave-preserving inventory contract for the temporary normal
  entry jobs. It filters graph waves with no current entry job without adding a flattened adapter,
  compatibility iterator or second wave computation.
- directory semantic compilation exhausts each retained graph wave before starting the next,
  using Rayon only when one ready wave contains multiple jobs. Frontend counters now distinguish
  singleton serial jobs from actual intra-wave parallel tasks.
- current entry-closure payload semantics, single-file compilation and deterministic entry-path
  result, string-table and diagnostic aggregation remain unchanged. No immutable provider
  interface is claimed or consumed before canonical per-node jobs land.
- focused inventory tests protect provider-before-consumer waves, same-wave independent jobs,
  deterministic graph `ModuleId` order and fan-in consumers sharing a ready wave.

Accepted Phase 5d1 checkpoint:

- `SourceModuleOriginTable` is the compiler-owned, immutable and remap-free side table from each
  prepared `FileId` to its graph-owned stable module origin. Directory construction projects the
  existing `ProjectModuleGraph` owned-source authority once; single-file construction assigns the
  one synthetic normal-module origin.
- `PreparedModule` retains the active root `FileId` and source-origin table instead of a loose
  trusted module origin. Preparation validates the graph-declared active origin before discarding
  it, and semantic direct-export construction resolves the active origin from the retained table.
- directly-defined public headers must carry a retained file identity whose table origin matches
  the active root. Missing, unowned, out-of-range and mismatched identities fail through internal
  `CompilerError` boundaries, including for a module with zero public exports.
- current entry-closure semantic source selection remains unchanged. Registered source-package
  files outside the project graph retain an explicit `None` until separate package graphs supply
  their stable origins; owned orphan files are not injected into semantic compilation.

### Phase 6: Add graph outcomes and immutable module artefact lanes

Context: successful and failed module results must be explicit data classes so tooling can inspect independent branches while builders receive only success.

See `docs/compiler-design-overview.md` "Module compilation outcomes" and "Compiled module artefact" for the full contracts.

- Introduce `ModuleCompilationOutcome` with `Success(CompiledModuleArtifact)` and `Diagnosed(ModuleDiagnostics)`.
- A diagnosed module exposes no partial public interface.
- Build `CompiledModuleArtifact` with four data lanes plus fingerprints: `PublicSemanticInterface`, `ModuleExecutable`, `ModuleLinkFacts`, `ModuleCompilerMetadata`, `ModuleFingerprints`.
- Record five base-module fingerprints: public-interface, implementation, dormant root-activity, runtime-dependency, documentation.
- Build `GraphCompilationOutcome` with successful, diagnosed and blocked lanes.
- Use `Result<GraphCompilationOutcome, CompilerError>` for the graph boundary.

Accepted Phase 6a checkpoint:

- `HirModule` now owns executable and semantic HIR only. Successful warnings, resolved
  documentation fragments and rendered-path usages leave AST/HIR lowering through the named
  `HirLoweringResult` and never occupy HIR fields or HIR validation.
- `ModuleCompilerMetadata` is the current module payload's single non-HIR metadata lane. It owns
  successful warnings, resolved const fragments, root activity, documentation fragments and
  rendered-path usages, including their one post-merge string-ID remap path.
- documentation metadata uses non-HIR names and an internal `CompilerError` validation boundary
  before successful module construction. HTML warning, fragment, root-activity and tracked-asset
  consumers read the metadata lane without changing output behavior.
- explicit executable/link lanes, immutable interfaces, fingerprints and module/graph outcomes
  remain later Phase 6 slices; mandatory start and the flat backend handoff are unchanged here.

Accepted Phase 6b checkpoint:

- the current `Module` payload contains exactly `ModuleExecutable`, `ModuleLinkFacts` and
  `ModuleCompilerMetadata`; the prior flat entry, HIR, type, borrow, registry and import fields were
  removed without compatibility accessors or deref behavior.
- `ModuleExecutable` owns the validated HIR, paired type environment and borrow facts, including
  the sole executable string-ID remap path. Root-local entry identity now lives in compiler
  metadata.
- `ModuleLinkFacts` owns provider-resolved external imports and the current effective external
  registry. The complete registry is explicitly a temporary dependency until Phase 7 supplies
  immutable binding interfaces and per-function link facts.
- frontend construction, backend validation/lowering, runtime glue and test fixtures consume the
  owning lanes without changing the flat `BackendBuilder` API, mandatory start or output behavior.

Accepted Phase 6c checkpoint:

- `FrontendModuleBuildContext::compile_module_semantic` now returns
  `Result<ModuleCompilationOutcome, CompilerError>`. Success carries the current unmerged module
  payload and local string table, while `Diagnosed(ModuleDiagnostics)` carries no partial module.
- `ModuleDiagnostics` is the self-contained owner for one module's user-facing failure. It retains
  ordered diagnostics, its string table and type-render contexts, requires a user-facing error and
  cannot contain an infrastructure diagnostic.
- one structured normalization point temporarily classifies deeper mixed `CompilerMessages`
  results. User errors may retain warning/note companions; one infrastructure failure may discard
  only warning/note companions; user-error/infrastructure blends, multiple infrastructure errors
  and empty or warning-only failures are internal invariants.
- a recovered `CompilerError` retains an optional private render-identity table. The ordinary
  `CompilerMessages` conversion merges and remaps that context once, preserving module-local
  source paths through deterministic directory aggregation without another render-side
  classification path.
- graph outcomes, blocked scheduling, final compiled artefacts, public interfaces and fingerprints
  remain later slices; single-file and directory build/render behavior is unchanged.

### Phase 7: Add stable public interfaces, cross-module calls and effect summaries

Context: a public interface contains only facts a semantic consumer may observe.

See `docs/compiler-design-overview.md` "Public semantic interfaces" and "Stable semantic identities" for the full contracts.

- Build `PublicSemanticInterface` with exported origin identities, `ExportBinding`, canonical type shapes, folded constants, generic templates, trait evidence, receiver surfaces, access and effect summaries, project-context provenance.
- Record project-context provenance on public semantic facts and per-function link facts.
- Propagate executable provenance through source and generated call edges so an exported function cannot hide a private `@project` dependency behind another helper.
- Backend planning facts do not belong in this interface. Per-function calls, helper requirements and runtime assets live in `ModuleLinkFacts`.
- Replace donor-local `TypeId` with canonical cross-module type identities. Each consumer may intern compact local `TypeId` handles for imported canonical types.
- HIR represents cross-module calls with explicit stable module-function targets. The callee body is never copied into the caller.
- Borrow validation runs once per canonical module and once per generated function. Public function interfaces carry parameter access modes, mutation, consumption, return aliasing and reactive effects.
- Cross-module call transfer consumes summaries without opening the callee's HIR.

Accepted Phase 7c1 checkpoint:

- `canonical_type_identity.rs` owns hashable cross-module identities for closed builtin, source
  nominal, binding-backed opaque, collection, map, option, fallible and concrete generic nominal
  types. The vocabulary embeds no donor-local `TypeId`, `NominalTypeId`, `InternedPath`,
  `StringId`, external numeric ID, source location or absolute path.
- one total projector consumes a module-local `TypeEnvironment`, an explicit nominal-origin
  resolver and the external registry. Missing origins, unregistered external identities,
  unresolved generic parameters, function/tuple shapes, malformed constructor arity and invalid
  generic-instance bases fail through `CompilerError` rather than omission or sentinels.
- `ExternalPackageRegistry` owns one atomic reverse identity from external type ID to package and
  structured symbol path, rejects ID reuse before mutation and preserves the record across clone.
- the existing `TypeIdentityKey` remains the module-local HIR/diagnostic bridge; it is not reused
  as a cross-module identity because it deliberately carries local paths and IDs.
- exported generic-parameter ownership, complete public type surfaces and production provider
  interface wiring remain Phase 7c2; the current vocabulary has focused dead-code allowances that
  name that immediate consumer.

Accepted Phase 7c2a checkpoint:

- `ExportedGenericParameterIdentity` derives from an opaque validated generic-declaration origin,
  the declaration-local parameter position and the owned authored name. Only free functions and
  struct/choice origins can construct the owner; receiver methods and transparent aliases fail
  through `CompilerError` and donor-local generic/type/string IDs never enter the value.
- `CanonicalTypeProjectionContext` now receives an explicit generic-parameter origin resolver.
  The existing `TypeId` projector uses it for open exported parameters, so nested collections,
  options and other constructed public shapes recurse through the same canonical path as closed
  types. Missing and synthetic parameters fail instead of falling back to name identity.
- the module-local `TypeIdentityKey` bridge remains separate and every closed projection retains
  its prior behavior. Production direct public type-surface projection remains Phase 7c2b.

Accepted Phase 7c2b1 checkpoint:

- AST public-surface validation now retains one required, transient `ResolvedPublicTypeRootTable`
  in `Ast`. It contains active-root public free-function signatures, struct/choice nominal
  TypeIds, transparent-alias targets, constant TypeIds and private receiver-method signatures
  attached to directly-defined public nominal receivers.
- roots preserve dependency-sorted header order. Receiver methods use a separate deterministic
  header pass after the complete public nominal set is known, so their ordinary private header
  mode and declaration order cannot hide them or turn them into free export bindings.
- the existing AST public-surface owner materializes and writes back a public alias target TypeId
  once; table construction consumes the retained fact and treats every missing resolved root as
  an internal CompilerError rather than reparsing, guessing or silently omitting it.
- donor-local TypeIds remain inside the required Ast handoff and do not enter HIR, Module or
  CompiledModuleResult. Canonical projection consumes this table before HIR in Phase 7c2b2.

Accepted Phase 7c2b2 checkpoint:

- `defined_public_type_surface.rs` is the single production owner that joins transient AST public
  type roots to stable export origins and the existing canonical type projector before HIR. Its
  explicit type-only output covers free-function signatures, nominal fields and variants,
  transparent aliases, constant types and receiver-method signatures without donor-local IDs.
- transient nominal and exported-generic-parameter resolvers are total. Root/binding and receiver
  joins consume exact stable keys and reject missing, duplicate, unmatched, ambiguous or
  category-mismatched facts through `CompilerError` rather than omission or fallback identity.
- the header-owned public-export target predicate is shared by AST nameability and source-nominal
  origin indexing. Direct roots, imported project-graph roots and private normal-file nominals
  exposed through a public alias receive their graph-derived stable origins, while unexported
  private nominals remain absent and alias-target private receiver methods remain unavailable.
- semantic orchestration takes the transient root table before HIR and retains only
  `DefinedPublicExportOrigins` plus `DefinedPublicTypeSurface` on a successful module result.
  Folded values, generic template bodies and bounds, trait evidence, access/effect summaries,
  provenance, provider interfaces and cross-module calls remain later Phase 7 slices.

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
- Before assembly succeeds, reject every selected declaration whose public facts or reachable source or generated implementation directly or transitively depend on private `@project`.
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
- dependency facade rejects an exported constant derived from private `@project`
- dependency facade rejects an exported function whose body reads private `@project`
- dependency facade rejects an exported function that transitively calls a private project-dependent helper
- dependency facade rejects a generated function reachable from an export when its template or concrete body depends on private `@project`
- private unreachable dependency implementation may use its own `@project`
- a consuming project's CLI or programmatic input does not satisfy a dependency `#Import` contract
- dependency contracts resolve only from the dependency's own config, defaults and compatible builder globals

The canonical integration suite owns user-visible source, project and backend behavior. Focused
Rust units remain appropriate for stable graph identities, deterministic scheduling, immutable
artefact lanes, blocked-result policy and other hidden facts that integration output cannot expose.
Do not duplicate an existing primary contract to exercise a new internal implementation path.

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
```

`just validate` already includes the non-recording benchmark sanity check. Run the documentation
release build when source docs change.

## Final architecture audit

Before marking this plan complete, verify:

- one canonical graph per project or package boundary is the only production path
- no entry-closure, flat payload or fallback import path remains
- public interfaces use stable identities, not donor-local IDs
- generated functions use sidecars, not base module mutation
- entry and package assembly never trigger semantic compilation
- the Wasm plan can start without redesigning frontend modules
- no declaration exposed through a dependency facade depends on private `@project` in either public facts or reachable implementation
- dependency input namespaces remain isolated from consuming-project inputs
- source, tests, docs, progress matrix and roadmap agree

## Deliberately deferred work

- persistent module and package artefact serialisation
- persistent artefact hash encoding, on-disk cache layout, eviction and migration policy
- cross-project dependency declarations and local path dependencies
- package registries, remote fetching, versions and lockfiles
- precompiled dependency package caches
- direct normal-sibling imports with cycle detection
- cross-entry JavaScript chunking and shared browser bundles
- physical Wasm module partition and Component Model integration
- cross-build generic instance caches

The module graph, stable identities and dependency records implemented here must make those features possible without another frontend ownership rewrite.
