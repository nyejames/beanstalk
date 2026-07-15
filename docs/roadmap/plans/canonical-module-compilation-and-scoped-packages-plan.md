# Canonical module compilation and scoped packages implementation plan

## Purpose

Replace Beanstalk's entry-closure frontend with a canonical project module graph. Every physical module must be prepared, type-checked, lowered and borrow-validated once per build, then reused by every page entry and package facade that depends on it.

The same refactor establishes the final project structure model:

- `#*.bst` roots define normal composition or entry modules
- `+*.bst` roots define scoped, API-only support packages
- one optional project-root `+*.bst` beside `config.bst` defines the project's external package facade
- source imports resolve from the importing file's owning module root, never from the importing file's physical directory
- public visibility remains controlled by one `export:` block
- project structure restricts which module dependencies are legal before semantic compilation begins

This plan is a prerequisite for the HTML Wasm backend plan. It must land first so later backend work consumes a stable module graph, explicit link targets and reusable module artifacts instead of reconstructing module relationships from a flat `Vec<Module>`.

## Active context capsule

ACTIVE_PLAN:
- `docs/roadmap/plans/canonical-module-compilation-and-scoped-packages-plan.md`

CURRENT_SLICE:
- Phase: Phase 0, queued and not started
- Checklist item: Refresh `main`, predecessor-plan outcomes and all implementation anchors before changing code
- Goal: Establish the current repository baseline and replace stale plan anchors with current owners
- Non-goals: Do not begin implementation while the active TIR plan or an earlier roadmap dependency is incomplete

LAST_GOOD_COMMIT:
- `751442111a8686e6447bbdd2fe3b5a2dcb52615c` - audited code anchor before this plan was inserted

CURRENT_WORKTREE_STATE:
- Clean / known changes: Plan and roadmap insertion only. Implementation has not started. The first worker must inspect its own local worktree and preserve unrelated changes.
- Branch: `main`
- Dedicated worker worktrees: none recorded

RELEVANT_DOCS_THIS_SLICE:
- `AGENTS.md`
- `docs/src/docs/codebase/style-guide/style-guide.bd`
- `docs/src/docs/codebase/style-guide/testing.bd`
- `docs/src/docs/codebase/style-guide/validation.bd`
- `docs/src/docs/codebase/compiler-design/overview.bd`
- `docs/src/docs/codebase/compiler-design/stages/project-structure/project-structure.bd`
- `docs/src/docs/codebase/compiler-design/imports-packages-and-bindings/imports-packages-and-bindings.bd`
- `docs/src/docs/codebase/compiler-design/build-system-and-frontend-boundary/build-system-and-frontend-boundary.bd`
- `docs/src/docs/codebase/compiler-design/parallelism-and-determinism/parallelism-and-determinism.bd`
- `docs/src/docs/codebase/compiler-design/stages/hir-generation/hir-generation.bd`
- `docs/src/docs/codebase/compiler-design/stages/borrow-validation/borrow-validation.bd`
- `docs/language-overview.md` because syntax, module semantics, imports, diagnostics and tests are changed
- `docs/src/docs/codebase/memory-management/borrow-validation/borrow-validation.bd` because exported call summaries and cross-module borrow behavior are changed
- `docs/src/docs/codebase/memory-management/overview.bd`

RELEVANT_CODE:
- `src/build_system/create_project_modules/source_tree_index.rs`: current one-pass entry-root scan, root discovery and extensionless source-name collision owner
- `src/build_system/create_project_modules/module_inventory.rs::DiscoveredModule`: current entry plus transitive source-closure compilation unit that must be deleted
- `src/build_system/create_project_modules/compilation.rs`: current per-entry directory compile orchestration, result aggregation and duplicated failure path
- `src/build_system/create_project_modules/frontend_orchestration.rs::FrontendModuleBuildContext`: current full frontend invocation for one flattened source closure
- `src/build_system/create_project_modules/project_roots.rs`: current project root, entry root and configured source-package setup
- `src/build_system/create_project_modules/source_package_discovery.rs`: current `package_folders` scanning and project-local source-package registration to remove
- `src/compiler_frontend/paths/path_resolution.rs`: current file-relative, package-prefix and entry-root fallback resolver to replace
- `src/compiler_frontend/paths/module_roots.rs`: current nearest hash-root table, the starting point for canonical module ownership
- `src/builder_surface/source_package_registry.rs`: current builder/project source-package registry to narrow to supplied packages and migrate onto compiled package interfaces
- `src/builder_surface/package_metadata.rs`: `PackageOrigin` and `PackageBacking` contracts to preserve
- `src/build_system/build.rs::Module`: current flat backend payload to replace with a graph-aware project compilation payload
- `src/compiler_frontend/ast/generic_functions/`: current consumer-local generic request and materialisation owner
- `src/compiler_frontend/hir/hir_expression.rs`: current HIR call representation that needs explicit cross-module targets
- `src/compiler_frontend/analysis/borrow_checker/`: current function summary and call-transfer owner
- `src/projects/html_project/`: first backend consumer of the project graph and entry link plans
- `docs/roadmap/plans/html_project_backend_wasm_final_implementation_plan.md`: downstream plan whose current-state assumptions must be refreshed when this plan completes

ACCEPTANCE_CRITERIA:
- Every canonical `#` or `+` module is prepared and semantically compiled exactly once per project build.
- A diagnostic owned by a shared module is emitted once, with canonical counts and render context.
- Imports are rooted at the importing file's owning module root and never use `@./` or `..`.
- Normal modules can import only their own source tree, direct child modules, visible support packages and registered packages.
- Scoped `+` packages obey the accepted parent/sibling-descendant visibility rules without allowing structural cycles.
- The optional project-root `+` facade exposes a package API without becoming internally importable.
- `package_folders` and default `/lib` scanning are removed.
- Public module interfaces contain stable cross-module identities, not donor-local `TypeId`, AST or HIR identities.
- Generic function instances are deduplicated project-wide.
- Borrow checking is per canonical module and cross-module calls consume exported effect summaries.
- Backends consume a project graph and explicit entry link plans.
- All affected documentation, progress-matrix rows, fixtures, scaffolds and deferred roadmap notes are aligned.

DECISIONS_ALREADY_MADE:
- decision: A directory-scoped module is the canonical semantic compilation unit.
  - reason: Shared source must not be repeatedly prepared and compiled for every active entry.
  - source/user/date: user agreement, 2026-07-15
- decision: Keep module-local `TypeEnvironment` values and use stable cross-module semantic identities.
  - reason: A project-global type arena would blur ownership and weaken future incremental and package boundaries.
  - source/user/date: user agreement, 2026-07-15
- decision: `+*.bst` defines a scoped support package, with the containing directory as its name.
  - reason: It provides strict, cycle-free lateral reuse without a second declaration-visibility keyword.
  - source/user/date: user agreement, 2026-07-15
- decision: Source imports are module-root-relative, not importing-file-relative.
  - reason: Files can move within a module without rewriting imports, while `..` remains unnecessary and invalid.
  - source/user/date: user agreement, 2026-07-15
- decision: Package and import namespaces never shadow or use precedence.
  - reason: Every `@name` must have one stable meaning before semantic compilation.
  - source/user/date: user agreement, 2026-07-15
- decision: Implement within-build reuse now and deliberately defer persistent incremental compilation.
  - reason: This fixes the current architecture without coupling it to cache serialization and invalidation policy.
  - source/user/date: user agreement, 2026-07-15

BLOCKERS / RISKS:
- The active TIR plan and earlier roadmap plans may move source, config, import or template owners before this plan begins. Phase 0 must refresh every named path and delete stale assumptions.
- Cross-module type identity, generic instances, trait evidence and receiver-method visibility are tightly coupled. Do not cut one boundary with donor-local IDs still leaking through another.
- Current backend metadata is filtered from entry `start` reachability. Canonical reusable modules require per-function or link-plan-owned runtime dependency facts.
- A large repository-wide import migration is unavoidable because current code and docs use `@./` and global entry-root paths.
- Strict support-package scopes may reveal real project-structure friction. Do not loosen to normal sibling imports inside this plan. Record evidence for the deferred fallback instead.
- Error collection must avoid cascades from modules blocked by a failed dependency while still compiling independent branches.

VALIDATION_STATE:
- last command: none for this queued plan
- result: plan-only artifact. No implementation validation is claimed.
- known unrelated failures: none recorded. Phase 0 must run the current required baseline and record any unrelated failure before implementation.

DOCS_IMPACT:
- progress matrix needed: yes, for module roots, support packages, import semantics, compile-once artifacts, package facades, generics and deferred incremental/output sharing
- other docs stale: `docs/language-overview.md`, project-structure pages, package/import pages, getting-started examples, codebase compiler-design pages and the Wasm plan's current-state section
- authorized docs updates: yes. The user explicitly requested complete documentation alignment as part of this plan.

NEXT_ACTION:
- After preceding roadmap work is complete, refresh `main`, update this capsule, run Phase 0 and commit the accepted baseline before Phase 1 begins.

---

## Executive summary

The current compiler treats each active `#*.bst` entry as an independent compilation universe. Each entry discovers its reachable source closure and invokes the full frontend over that closure. When two entries import the same module, the shared files are tokenized, header-parsed, type-checked, lowered and borrow-checked more than once. Identical diagnostics are then appended during failure aggregation.

The target pipeline is:

```text
project/config inputs
    -> one canonical source index
    -> module ownership tree and scoped package namespaces
    -> validated acyclic module dependency graph
    -> compile each canonical module once in dependency order
    -> immutable public interfaces + module-local HIR/type/borrow artifacts
    -> project-wide generic instance worklist
    -> entry and package-facade link plans
    -> backend artifacts
```

The target build payload is a graph, not a flat list of entry-local modules:

```text
ProjectCompilation
├── source/module structure
├── CompiledModuleArtifact[module_id]
├── ModuleGeneratedArtifacts[module_id]
├── entry assemblies
├── optional project package facade
└── package and external-runtime metadata
```

No phase may solve duplicated work by hiding repeated diagnostics in the renderer. The architecture must stop producing repeated module diagnostics in the first place.

## Current repository state at plan creation

This plan was written against `main` at `751442111a8686e6447bbdd2fe3b5a2dcb52615c`.

Current facts to re-check when Phase 0 begins:

- The TIR finalisation plan is still active.
- `module_inventory.rs::DiscoveredModule` stores one entry path and every transitively reachable input file.
- Directory compilation invokes `FrontendModuleBuildContext::compile_module` independently for every discovered entry closure.
- Failed module messages are remapped and appended during directory failure aggregation.
- `build.rs::Module` owns one HIR module, one `TypeEnvironment`, one borrow report and entry-root metadata. `BackendBuilder::build_backend` receives `Vec<Module>`.
- `ProjectPathResolver` resolves registered source-package prefixes before falling back to importing-file-relative `@./` or project-global `entry_root` paths.
- `Config` and Stage 0 currently carry `package_folders` and default `/lib` discovery for project-local source-backed packages.
- `SourcePackageRegistry` and `ExternalPackageRegistry` already separate Beanstalk-source and binding-backed packages. `PackageOrigin` and `PackageBacking` are sound metadata axes.
- Generic function call parsing emits instantiation requests that are materialised within the consuming module.
- Borrow validation already has function-summary concepts that can become module-interface facts.
- The HTML Wasm plan already expects a compiled module graph and link plan, but the current backend handoff does not provide them.
- User-facing docs currently describe `@./x` as importing-file-relative and most non-relative project imports as `entry_root`-relative. That is not the accepted final design.

Phase 0 must replace this snapshot with current facts before implementation.

## Terminology

### Normal module

A directory-scoped module rooted by one cosmetic `#*.bst` file. It may be an active entry, own dormant top-level runtime work and expose declarations through `export:`.

### Support module

A directory-scoped module rooted by one cosmetic `+*.bst` file. It is API-only and backs a scoped package name derived from its containing directory.

### Project package facade

The optional `+*.bst` beside `config.bst`. It defines the externally consumable package surface of the project and is not visible to internal project modules.

### Module owner

The nearest containing `#` or `+` root for a source file. Every project source file has exactly one owner.

### Direct child module

A module whose nearest ancestor module is the importer. It may be several filesystem directories below the importer when the intermediate directories have no module root.

### Public module interface

An immutable semantic record of one module's exported declarations, canonical type identities, folded constant facts, generic templates, trait/evidence facts and call-effect summaries.

### Compiled module artifact

The module-local semantic result: public interface, HIR, local `TypeEnvironment`, borrow facts, diagnostics, root activity and backend-neutral dependency metadata.

### Entry assembly

A link plan that selects one normal module as active, activates its dormant start/fragments and records the reachable compiled modules, generated instances and runtime dependencies required by that entry.

## Binding design decisions

These are accepted design, not implementation suggestions. A worker may improve type or file names after inspecting current `main`, but may not change these semantics without a new user decision.

### 1. Canonical compilation unit

- One directory-scoped module is compiled once per build.
- Shared source is not absorbed into every entry's AST, HIR or type environment.
- Each module keeps a local `TypeEnvironment`, local HIR identities and local borrow facts.
- Cross-module references use stable project semantic identities.
- A project-global `TypeEnvironment` is explicitly rejected.

### 2. Stable cross-module identities

Every exported source declaration needs an identity rooted in its declaring module. The conceptual forms are:

```rust
ModuleId
ModuleDeclarationId { module: ModuleId, declaration: DeclarationId }
ModuleFunctionId { module: ModuleId, function: DeclarationId }
ModuleTypeId { module: ModuleId, declaration: DeclarationId }
ModuleConstantId { module: ModuleId, declaration: DeclarationId }
```

The exact Rust names can change, but these rules cannot:

- donor-local `TypeId`, AST node indexes, HIR function indexes and import aliases do not cross module boundaries
- aliases affect source spelling, not semantic identity
- private declarations never receive a consumer-visible identity
- identity assignment is deterministic across thread scheduling within one build

### 3. Cross-module type identity

Public interfaces cannot store donor-local `TypeId` values. Introduce one canonical cross-module type representation that can describe:

- builtins
- module-owned nominal structs and choices
- transparent aliases
- constructed options, collections, maps and fallible carriers
- concrete generic nominal instances
- generic parameters inside exported generic templates
- external package types

Each consumer module may intern compact local `TypeId` handles for imported canonical types. The local environment must retain an origin map back to canonical identity. Semantic equality across module boundaries compares canonical identity, never rendered names or unrelated local handles.

### 4. Ordinary cross-module calls

- HIR represents a source call to another module with an explicit stable module-function target.
- The callee body is not copied into the caller.
- Backend linking resolves module-function targets through the project graph.
- Private functions remain addressable only within their declaring artifact.

### 5. Generic function ownership

- The declaring module owns and validates the immutable generic template.
- Consumers infer concrete type arguments and emit concrete requests.
- A project-wide worklist deduplicates by stable generic declaration identity plus canonical concrete type identities.
- Generated instances live in a generated-artifact sidecar, not by mutating the immutable base module artifact.
- A generated instance may request further generic instances. Materialisation therefore uses a deterministic worklist, not one pass.
- Invalid generic templates are diagnosed at declaration compilation.
- Inference, missing evidence and invalid concrete substitutions are diagnosed at the requesting call site, with declaration context where useful.
- HIR and backends receive only concrete executable targets.

### 6. Constants and const templates

- A module folds its constants and const templates once.
- Exported folded facts are copied into the public interface as owned backend-neutral values.
- Consumers do not parse, compose or fold the provider's template again.
- TIR references never cross module interfaces. TIR remains AST-local according to the final TIR architecture.
- Private constants remain local.

### 7. Traits, conformances and receiver methods

- Exported traits use stable declaration identities.
- Reusable conformance evidence is exported as stable semantic evidence, not reconstructed structurally in consumers.
- Receiver methods remain tied to their receiver type's source surface.
- A consumer receives only methods that the exported receiver surface makes visible.
- Methods are not independently importable, aliased or re-exported as free namespace entries.

### 8. Borrow validation and call effects

- Borrow validation runs once for each canonical module and once for generated concrete functions.
- Public function interfaces carry the effect facts needed by consumers: parameter access modes, mutation, possible ownership consumption, return aliasing and relevant reactive effects.
- Cross-module call transfer consumes those summaries and never opens the callee's HIR as if it were local.
- Missing or internally inconsistent exported summaries are compiler invariant failures.

### 9. Normal `#*.bst` modules

A normal module may import:

- ordinary files and unrooted directories owned by itself
- direct child normal modules
- support packages visible in its lexical module scope
- registered Core, Builder and future dependency packages
- project-local provider files allowed by the active builder

A normal module may not import:

- its parent or any ancestor
- a normal sibling
- a grandchild module directly
- a sibling module's descendant
- a cousin, uncle or unrelated branch
- another module's private file path

A direct child must re-export anything its parent should see from deeper descendants.

### 10. Scoped `+*.bst` support packages

- The containing directory supplies the package name.
- The suffix after `+` is cosmetic.
- A directory contains at most one root, either `#*.bst` or `+*.bst`.
- A support root is API-only: no implicit start, top-level runtime statements, page fragments, route or builder artifact.
- Functions, types, constants, const templates, traits and ordinary runtime code inside functions remain valid.
- `export:` is the only declaration visibility marker.

For a support package `S` whose nearest ancestor module is `P`:

- `S` is visible to `P`
- `S` is visible to normal sibling modules of `S`
- `S` is visible to descendants of those sibling modules
- `S` is not visible above `P`
- `S` is not visible from outside `P`'s subtree
- `S` is not visible inside `S`'s own private implementation subtree
- another support package with the same owner scope cannot import `S`

A support facade may import:

- ordinary files it owns
- any descendant module in its own private subtree
- support packages supplied by a strictly outer scope
- registered packages

It may not import its parent, normal sibling consumers or same-scope support siblings. These rules make valid project module graphs acyclic by construction. Keep a defensive cycle validator for malformed internal state and future feature changes.

### 11. Private support-package subtrees

```text
markdown/
├── +package.bst
├── parser/
│   └── #parser.bst
├── model/
│   └── +support.bst
└── rendering/
    └── #rendering.bst
```

- The `markdown` facade may import every descendant module under its directory.
- Descendants cannot import the `markdown` facade.
- Consumers import only `@markdown` and cannot address `parser`, `model` or `rendering` through the package.
- The facade explicitly re-exports intended API items.
- Nested `+` modules can provide scoped internal sharing inside the private subtree.

### 12. Project-root package facade

```text
project/
├── config.bst
├── +package.bst
└── src/
    ├── #site.bst
    ├── parser/
    │   └── #parser.bst
    └── model/
        └── +support.bst
```

- At most one `+*.bst` may sit beside `config.bst`.
- No root facade means the project has no externally consumable Beanstalk package surface.
- The root facade is not visible to internal modules.
- It may import any descendant module below `entry_root` through an entry-root-anchored assembly namespace.
- It still receives only each descendant's `export:` surface.
- It is API-only and emits no route or runtime entry.
- A project may be both an application and a library.
- One canonical config `name` supplies external package identity. Remove the `project_name` alias. If the preceding config-block plan changes the storage shape, apply this semantic rule to the then-current canonical project metadata owner.

### 13. Entry-root invariant

For directory projects, `entry_root` must be a relative directory strictly below the project root.

Reject:

- an empty path
- `.`
- `..` or any parent component
- an absolute path
- a path resolving outside the project root
- a symlink-resolved path equal to the project root

Single-file compilation remains a separate synthetic-module mode and is not forced through the directory-project config invariant.

### 14. Module-root-relative imports

All project source imports are resolved from the importing file's owning module root, not its physical directory.

```text
src/
├── #site.bst
├── accounts.bst
└── internal/
    └── deep/
        └── renderer.bst
```

Inside `renderer.bst`:

```beanstalk
import @accounts { Account }
```

This resolves to `src/accounts.bst`, not `src/internal/deep/accounts.bst`.

Rules:

- `@./accounts` is unsupported and has no compatibility meaning.
- `..` is always invalid.
- Paths may traverse ordinary unrooted directories owned by the same module.
- Reaching a `#` child module or `+` support package terminates filesystem traversal and exposes only its facade.
- `@child/internal` and `@support/internal` are invalid boundary bypasses.
- Scoped support packages are injected by their package name, so consumers do not encode ancestor paths.
- The project-root facade is the sole assembly exception and resolves project paths from `entry_root`.
- Project-local explicit-extension provider imports must have an explicit owner. Prefer the same module-root anchor so deep files do not reintroduce importing-file-relative behavior. Preserve a different base only where a registered package provider contract intentionally owns it and document that exception.

### 15. Strict namespace and collision policy

No source or package import uses precedence, nearest-match shadowing or ordered fallback.

Reject overlapping identities between:

- visible support packages
- direct child normal modules
- extensionless ordinary source files
- internal directory path segments
- the project package name
- Core or Builder package roots
- future dependency aliases
- case-only variants

Recognized extensionless source kinds such as `.bst`, `.bd` and `.md` share one import namespace. `docs.bst`, `docs.bd`, `docs.md` and `docs/` cannot coexist where they would all mean `@docs`.

Explicit-extension files such as `drawing.js` may coexist with `drawing/` only when the import syntax remains unambiguous.

The same support-package name may appear in disjoint visibility scopes. It is invalid when those scopes overlap. Diagnostics must point to both declarations and explain the overlap scope.

### 16. Package-system boundary

This plan implements:

- scoped project support packages
- optional project-root package facades
- canonical within-build source package compilation
- Builder source packages on the same interface/artifact model
- a unified collision and import namespace
- package metadata compatible with future dependency packages

This plan removes:

- `package_folders`
- default `/lib` project-package scanning
- project-local source packages as a separately configured discovery system
- direct consumer access to package implementation trees

This plan preserves:

- `PackageOrigin`
- `PackageBacking`
- separate Beanstalk-source and external-binding implementations where their compiler needs differ
- Builder registration of package names and filesystem roots

This plan deliberately defers dependency declarations, path dependencies, registries, remote fetching, semantic version solving, lockfiles, overrides, precompiled artifact serialization and persistent package caches.

### 17. Entry assembly and physical output

- Normal modules retain dormant start and page-fragment metadata in their canonical artifact.
- Entry assembly activates only the selected normal module.
- Imported modules and support packages never execute start work.
- HTML-JS may continue producing one self-contained output bundle per entry.
- Repeated final JS or Wasm bytes across separate page artifacts are not proof of repeated frontend compilation.
- Cross-page JS chunks, shared browser bundles and physical Wasm module layout remain deferred.

### 18. Incremental compilation boundary

This plan guarantees reuse within one build. It must also record enough stable facts for a later incremental system:

- canonical module identity
- owned source files
- direct semantic dependencies
- public interface fingerprint inputs
- generated instance dependencies
- entry link dependencies

Persistent source hashes, serialized artifacts, dev-server retention, public-interface hashing policy and selective invalidation remain deferred.

### 19. Deferred sibling-import fallback

Do not allow normal sibling modules to import each other in this plan.

Record this explicit fallback for later evaluation:

> Direct normal-sibling imports with cycle detection may be introduced if real projects show that scoped support packages create unacceptable factoring friction.

Any future proposal must include project evidence, cycle diagnostics, invalidation consequences and a reason the shared behavior cannot live in a scoped `+` package.

## Target architecture contracts

The names below are illustrative. Phase 0 may rename them to match current repository conventions, but each responsibility needs one owner.

```rust
enum ModuleRootKind {
    Normal,
    Support,
    ProjectPackageFacade,
}

struct ProjectModuleGraph {
    modules: Vec<ProjectModuleNode>,
    compile_order: Vec<ModuleId>,
    entry_modules: Vec<ModuleId>,
    project_package_facade: Option<ModuleId>,
}

struct ProjectModuleNode {
    id: ModuleId,
    kind: ModuleRootKind,
    root_directory: PathBuf,
    root_file: PathBuf,
    owner_parent: Option<ModuleId>,
    owned_files: Vec<SourceFileId>,
    dependencies: Vec<ModuleId>,
    namespace: ModuleNamespace,
}

struct CompiledModuleInterface {
    module_id: ModuleId,
    exports: ModuleExports,
    canonical_types: CanonicalTypeTable,
    folded_constants: ExportedConstFacts,
    generic_templates: ExportedGenericTemplates,
    trait_evidence: ExportedTraitEvidence,
    function_effects: ExportedFunctionEffects,
}

struct CompiledModuleArtifact {
    interface: CompiledModuleInterface,
    hir: HirModule,
    type_environment: TypeEnvironment,
    borrow_analysis: BorrowCheckReport,
    diagnostics: Vec<CompilerDiagnostic>,
    root_activity: ModuleRootActivity,
    runtime_dependencies: ModuleRuntimeDependencyFacts,
}

struct ModuleGeneratedArtifacts {
    generic_instances: Vec<GeneratedFunctionArtifact>,
}

struct ProjectCompilation {
    structure: ProjectModuleGraph,
    modules: Vec<CompiledModuleArtifact>,
    generated: Vec<ModuleGeneratedArtifacts>,
    entries: Vec<EntryAssembly>,
    package_facade: Option<ProjectPackageAssembly>,
}
```

Required ownership boundaries:

- Stage 0 owns filesystem discovery, file ownership, root roles, legal module topology and import namespace identities.
- Header/import preparation owns source import binding and public export maps.
- AST owns semantic declaration resolution, canonical type mapping, generic requests and constant/template folding.
- HIR owns explicit cross-module executable call targets.
- Borrow validation owns function effect summaries and generated-function validation.
- The build system owns project compilation scheduling, deterministic parallelism, generic worklist orchestration and entry link plans.
- Backends consume the graph and link plans. They do not rediscover source structure.

## Diagnostic requirements

Add structured reasons where current diagnostics cannot express the new facts. Do not store pre-rendered prose in payloads.

Required categories include:

- invalid `entry_root` relationship to project root
- mixed or duplicate `#` and `+` roots in one directory
- duplicate project-root package facades
- support root containing forbidden runtime or fragment activity
- illegal module dependency with importer, target and relationship
- `@./` source import removed
- parent-path segment rejected
- import path attempting to traverse through a child or package facade
- ambiguous extensionless source identity
- overlapping support-package scopes
- package/import namespace collision with both source locations
- project package facade missing canonical `name`
- project modules attempting to import the project facade
- dependency module unavailable because its canonical compilation failed
- malformed canonical interface or missing effect summary as internal `CompilerError`

Error collection policy:

- Compile independent branches even when another branch fails.
- Do not compile a dependent module when a required interface failed.
- Avoid cascades of unknown-name/type errors caused only by the unavailable dependency.
- Report the dependency's real diagnostics once.
- A compact blocked-module note is allowed only if it materially helps locate skipped work and does not inflate error counts as if it were an independent source failure.

## Execution and slice rules

- Refresh this plan's active context capsule after every accepted slice and before any context compaction.
- Every implementation slice must fit one coding-agent context and have one clear primary owner.
- Commit accepted slices independently with the plan update included.
- Preserve unrelated user changes and never assume a clean worktree without checking.
- Do not add compatibility wrappers, fallback resolver modes, dual module artifacts or legacy aliases.
- When a new owner is wired, delete the old owner in the same phase before acceptance.
- User-visible behavior belongs primarily in `tests/cases/`.
- Unit tests protect hidden identities, topology, summaries, remapping and scheduling invariants.
- Benchmark fixtures are performance evidence, not correctness tests.
- Do not edit `docs/release/**` manually.
- Code-bearing phases end with `cargo fmt`, focused tests and `just validate`.
- Run `just bench-check` when Stage 0, import resolution, module preparation, AST/HIR orchestration, generic materialisation or scheduling changes.
- Run targeted profiling only for an observed regression or to attribute the existing duplicated-work baseline.

---

# Implementation phases

## Phase 0 - Activation, current-main audit and measured baseline

### Context and reason

This plan was authored while the TIR plan was active and before several earlier roadmap plans. Starting from stale paths would create bad APIs around code that no longer exists. Phase 0 is a hard activation gate, not optional administration.

### Slice 0A - Reload repository authority

- [ ] Confirm every earlier roadmap plan that this plan depends on is complete or explicitly compatible.
- [ ] Record `git rev-parse HEAD`, branch and `git status --short --branch` in the context capsule.
- [ ] Re-read `AGENTS.md` and its current task-reading matrix.
- [ ] Re-read current project-structure, import/package, type-identity, HIR, borrow, parallelism and backend-handoff docs.
- [ ] Re-open every path in `RELEVANT_CODE` and replace stale files or symbols in the capsule.
- [ ] Reconcile the final TIR handoff. Confirm exported const templates can cross module interfaces only as folded owned facts, never TIR identities.
- [ ] Reconcile the completed config-block and import-value plans. Preserve their final owners and syntax.
- [ ] Reconcile the current Wasm plan and record the exact graph contract it expects.

### Slice 0B - Inventory obsolete architecture

- [ ] Search current source for `DiscoveredModule`, reachable entry closures, `Vec<Module>` backend handoff and message append aggregation.
- [ ] Inventory every use of `package_folders`, source-package project scanning and default `/lib` behavior.
- [ ] Inventory `@./`, `RelativeToFile`, `EntryRoot`, public-surface fallback and root-file path walking.
- [ ] Inventory donor-local `TypeId`, function IDs, trait IDs and generic instance keys that currently cross file/module boundaries.
- [ ] Inventory current function-summary production and call-transfer consumption.
- [ ] Inventory current external-runtime metadata and every place that assumes reachability begins at entry `start`.
- [ ] Inventory documentation, examples, tests, benchmarks and scaffolds requiring source-import migration.
- [ ] Classify each old owner as replace, extend or delete. Record the result in this plan before code starts.

### Slice 0C - Baseline correctness and performance evidence

- [ ] Add or identify one canonical multi-entry fixture where many entries share a substantial module subtree.
- [ ] Add or identify one shared-module diagnostic fixture that produces one unknown-name failure in the shared module.
- [ ] Capture counters for unique module roots, entry closures, source preparations, module frontend invocations and emitted diagnostics.
- [ ] Run the current focused Stage 0/module tests.
- [ ] Run `just validate` and record exact totals and unrelated failures.
- [ ] Run `just bench-frontend-check` and `just bench-check`.
- [ ] Profile only the shared-graph fixture if the existing counter/timing evidence does not clearly attribute repeated work.
- [ ] Store concise baseline evidence in the plan. Do not commit raw profiles.

### Phase 0 acceptance

- [ ] The capsule names current owners and a current commit.
- [ ] All predecessor-plan effects are understood.
- [ ] The existing duplicated-work path is proven by counters or focused profiling.
- [ ] Required migration scope is enumerated.
- [ ] No implementation code changed beyond bounded baseline instrumentation and fixtures.

### Phase 0 audit / style guide / validation

- [ ] Review ownership, duplication, stale paths and test gaps.
- [ ] Review all new instrumentation names and comments against the style guide.
- [ ] Confirm benchmarks are not used as correctness coverage.
- [ ] Run `cargo fmt` if Rust changed.
- [ ] Run focused tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the accepted baseline.

---

## Phase 1 - Project layout, root roles and config invariants

### Context and reason

The compiler cannot schedule canonical modules until Stage 0 can distinguish normal modules, support packages and the optional project package facade without rediscovering the tree later.

### Slice 1A - Root identity model

- [ ] Introduce one root-kind enum for normal `#`, support `+` and project package facade roles.
- [ ] Extend root filename classification to recognize `+*.bst` with the same UTF-8 and representability rigor as hash roots.
- [ ] Keep suffixes after `#` and `+` cosmetic.
- [ ] Reject more than one root of either kind in one module directory.
- [ ] Reject a directory containing both `#*.bst` and `+*.bst`.
- [ ] Preserve `config.bst` as non-module build-system input.
- [ ] Discover at most one project-root `+*.bst` beside `config.bst`.
- [ ] Keep single-file compilation behavior separate and explicit.

### Slice 1B - Strict entry-root validation

- [ ] Move entry-root relationship validation into the Stage 0 project-root owner.
- [ ] Reject empty, `.`, parent-containing and absolute values.
- [ ] Canonicalize project and entry roots before relationship checks.
- [ ] Reject an entry root equal to or outside the project root after symlink resolution.
- [ ] Require an existing directory.
- [ ] Preserve useful config source locations and structured reasons.
- [ ] Add Windows, Linux and macOS path-shape coverage where platform behavior differs.

### Slice 1C - Root semantic roles

- [ ] Add header/parser role information for normal, support and project-facade roots.
- [ ] Preserve dormant start/fragments for normal roots.
- [ ] Reject top-level runtime statements and page fragments in support and project-facade roots.
- [ ] Allow ordinary declarations, folded constants, const templates and function bodies in support/facade modules.
- [ ] Ensure the implicit `start` symbol is never exported or importable.
- [ ] Add API-only root activity tests.

### Slice 1D - Canonical project package name

- [ ] Inspect the final config representation produced by the earlier config plan.
- [ ] Establish one canonical `name` field or equivalent typed metadata owner.
- [ ] Remove `project_name` alias handling and tests.
- [ ] Require a valid package name only when a project-root `+` facade exists.
- [ ] Validate the name against import-root identifier and case rules.
- [ ] Keep application-only projects valid without an external package surface.

### Phase 1 acceptance

- [ ] Stage 0 has one root-role authority.
- [ ] Directory projects cannot use project root as `entry_root`.
- [ ] Support/facade roots are API-only.
- [ ] Project package identity is config-owned and filename-independent.
- [ ] No builder or frontend path guesses root roles from filename strings later.

### Phase 1 audit / style guide / validation

- [ ] Review root discovery for duplicate scanners and lossy filename handling.
- [ ] Review diagnostics for correct config/import/rule lanes.
- [ ] Review comments and type names for clear WHAT/WHY ownership.
- [ ] Run `cargo fmt`.
- [ ] Run focused root, config and single-file tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check` because Stage 0 discovery changed.
- [ ] Update the capsule and commit the phase.

---

## Phase 2 - Canonical module tree, file ownership and support scopes

### Context and reason

Visibility and compilation reuse depend on a complete structural model prepared once. The path resolver must consume this model rather than scanning parents or trying fallback candidates at import time.

### Slice 2A - Canonical module records

- [ ] Replace the root-only table with or extend it into canonical module records carrying stable `ModuleId` values.
- [ ] Assign IDs in deterministic canonical path order.
- [ ] Record root kind, root directory, root file and source-relative logical path.
- [ ] Record the nearest ancestor module as structural parent.
- [ ] Record direct child modules by nearest-module ancestry, not immediate filesystem depth.
- [ ] Represent the optional project facade as a special node outside the entry-root containment tree.
- [ ] Keep module identity independent of root suffix text.

### Slice 2B - File ownership

- [ ] Assign every recognized project source file to its nearest containing module root.
- [ ] Treat unrooted subdirectories as internal directories of the same module.
- [ ] Exclude config, output folders, generated folders and project infrastructure.
- [ ] Reject source files below `entry_root` that have no owning module if the current project rules require ownership.
- [ ] Preserve source-kind information for `.bst`, `.bd`, `.md` and provider-backed explicit-extension inputs.
- [ ] Expose owned-file iteration without rescanning the filesystem.

### Slice 2C - Support-package scope computation

- [ ] Derive a support package's name from its containing directory basename.
- [ ] Record its owner scope as the nearest ancestor module outside its private subtree.
- [ ] Compute visibility to the owner, normal siblings and sibling descendants.
- [ ] Exclude the support module and its private descendants from consumer visibility.
- [ ] Allow support modules to see support packages from strictly outer scopes.
- [ ] Reject same-scope support-to-support dependencies.
- [ ] Add a narrow query API such as `visible_support_packages(importer_module)`.

### Slice 2D - Structural dependency validator

- [ ] Classify module relationships: own source, direct child, visible support, private descendant, parent, sibling, cousin and unrelated.
- [ ] Encode legal dependency rules in one Stage 0 owner.
- [ ] Add a defensive cycle detector and make any cycle an internal/project-structure failure with a readable chain.
- [ ] Ensure root/project facade dependencies point only into `entry_root` descendants.
- [ ] Keep package private descendants hidden from ordinary consumers.

### Phase 2 acceptance

- [ ] Every source file has exactly one canonical module owner.
- [ ] The module tree is deterministic.
- [ ] Support visibility is queryable without path walking.
- [ ] Legal module dependencies are structurally acyclic.
- [ ] No later compiler stage performs nearest-root discovery.

### Phase 2 audit / style guide / validation

- [ ] Review data structure ownership and avoid a broad generic graph utility.
- [ ] Review deterministic ordering and non-UTF-8 failure handling.
- [ ] Review support-scope tests for parent, sibling, descendant, private-subtree and outer-scope cases.
- [ ] Run `cargo fmt`.
- [ ] Run focused Stage 0 tree and scope tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the phase.

---

## Phase 3 - Strict module namespaces and module-root-relative imports

### Context and reason

This is a deliberate source-language hard break. Import paths must reveal structural ownership rather than depending on the importing file's directory or a global entry-root fallback.

### Slice 3A - Module namespace index

- [ ] Build one validated namespace per module from its owned files, internal directories, direct child modules and visible support packages.
- [ ] Register Core, Builder and binding-backed package roots into the same collision review without pretending they are filesystem entries.
- [ ] Represent namespace entries with an enum such as internal file, internal directory, child module, support package, registered package and provider file.
- [ ] Reject extensionless source-kind collisions and case-only collisions during Stage 0.
- [ ] Reject support-package overlap with files, directories, child modules or registered packages.
- [ ] Do not implement precedence or shadowing.

### Slice 3B - Resolution semantics

- [ ] Replace importing-file-relative and global entry-root fallback with module-root-relative lookup.
- [ ] Resolve the first component through the importing module's validated namespace.
- [ ] Continue path traversal only through internal directories owned by the same module.
- [ ] Terminate traversal at child-module and support-package facades.
- [ ] Resolve support packages by scoped package name without physical ancestor paths.
- [ ] Give the project-root facade a distinct entry-root-anchored assembly resolver.
- [ ] Keep config imports restricted to allowed Core or Builder packages.
- [ ] Make project-local explicit-extension provider paths use an explicit owner and document any registered-package exception.

### Slice 3C - Hard rejection and diagnostics

- [ ] Reject `@./...` with a targeted migration diagnostic.
- [ ] Keep `..` rejected.
- [ ] Reject direct imports of `#*.bst`, `+*.bst` and `config.bst`.
- [ ] Reject path traversal beyond a module or package facade.
- [ ] Reject parent, normal-sibling, grandchild and unrelated module dependencies with relationship-specific diagnostics.
- [ ] Label both declarations for namespace collisions.
- [ ] Ensure diagnostics use module-root-relative logical paths in messages.

### Slice 3D - Repository source migration

- [ ] Migrate compiler tests, integration fixtures, benchmarks, packages and documentation source away from `@./`.
- [ ] Replace global entry-root imports with paths valid from each owning module root.
- [ ] Split or rename `name.ext` / `name/` collisions exposed by the stricter namespace.
- [ ] Preserve import aliases only where they are semantically useful.
- [ ] Remove tests that protect old fallback order.
- [ ] Add end-to-end deep-file fixtures proving module-root anchoring.
- [ ] Add negative fixtures for every illegal topology relationship.

### Slice 3E - Delete old resolver paths

- [ ] Remove `RelativeToFile` and generic project-global source fallback where no longer used.
- [ ] Remove public-surface fallback that discovers a root by walking up from a candidate path.
- [ ] Remove duplicate extension candidate logic made obsolete by the namespace index.
- [ ] Keep compile-time non-import path literals separate if they still need filesystem-relative semantics.
- [ ] Ensure providers and source imports do not share an abstraction that blurs their different outputs.

### Phase 3 acceptance

- [ ] Every project source import is resolved from a module or package namespace prepared by Stage 0.
- [ ] `@./` has no accepted path.
- [ ] Moving a file within its owning module does not change bare import meaning.
- [ ] Facade boundaries cannot be bypassed.
- [ ] The repository builds using only the new import model.

### Phase 3 audit / style guide / validation

- [ ] Audit for residual `@./`, `RelativeToFile`, `EntryRoot` source fallback and root-walking helpers.
- [ ] Review import diagnostics and label quality.
- [ ] Review namespace types for explicit enums rather than boolean-heavy state.
- [ ] Run `cargo fmt`.
- [ ] Run focused resolver, header and integration tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check` because a hot resolver path changed.
- [ ] Update the capsule and commit the phase.

---

## Phase 4 - Package/config consolidation and `+` package migration

### Context and reason

Scoped `+` packages replace configured project-local source-package folders. Builder-provided source packages remain registered inputs, but they must expose the same facade and compiled-interface model.

### Slice 4A - Remove configured project-local package folders

- [ ] Remove `package_folders` from the canonical config registry and `Config`.
- [ ] Remove explicit/default folder flags and `/lib` default behavior.
- [ ] Delete folder-path validation and project-local package scanning.
- [ ] Delete replaced-key diagnostics that only exist for `libraries`, `library_folders`, `root_folders` or `package_folders` compatibility.
- [ ] Remove source-package prefix merge paths that existed solely for configured project packages.
- [ ] Keep clear diagnostics for stale `package_folders` source through the normal unknown/removed-config path chosen by the final config design.

### Slice 4B - Local scoped packages

- [ ] Register nested `+` roots directly from the canonical project module tree.
- [ ] Expose their package names only through computed scopes, not a global registry.
- [ ] Ensure same-named packages in disjoint scopes are valid.
- [ ] Ensure overlapping scopes are rejected before header preparation.
- [ ] Keep support-package implementation children private.

### Slice 4C - Builder source packages

- [ ] Preserve `SourcePackageRegistry` only for Builder and future Dependency supplied roots, or replace it with a narrower supplied-package registry.
- [ ] Require supplied Beanstalk-source packages to expose one `+*.bst` facade.
- [ ] Let Builder metadata supply package identity, so supplied packages do not require project `config.bst`.
- [ ] Migrate `@html` and any other Builder source package to `+` facade semantics.
- [ ] Compile supplied package modules once per build through the same canonical artifact pipeline.
- [ ] Preserve `PackageOrigin` and `PackageBacking` in interfaces and diagnostics.

### Slice 4D - Project-root package facade

- [ ] Compile the optional project-root `+` facade as an API-only assembly root.
- [ ] Give it entry-root-anchored access to descendant module facades.
- [ ] Prevent all internal modules from importing it.
- [ ] Store the final external surface as a project package interface.
- [ ] Do not emit a backend route or start artifact for it.
- [ ] Add hybrid application/library fixtures.

### Slice 4E - Scaffolds and examples

- [ ] Update `bean new` output to remove `lib`/`package_folders` assumptions.
- [ ] Add a focused library-project scaffold or documented shape with root `+` facade.
- [ ] Add a scoped support-package example at the narrowest common ancestor of its consumers.
- [ ] Keep generic `common`/`utils` dumping-ground guidance out of generated scaffolds.

### Phase 4 acceptance

- [ ] Project-local packages are discovered structurally through `+` roots.
- [ ] `package_folders` and default `/lib` scanning are gone.
- [ ] Builder source packages use the same facade contract.
- [ ] A root `+` facade produces a valid package interface and no artifact.
- [ ] Package names are unambiguous and metadata-backed.

### Phase 4 audit / style guide / validation

- [ ] Audit for all old package-folder fields, validators, diagnostics and tests.
- [ ] Audit Builder package registration for duplicate source preparation paths.
- [ ] Review package terminology in comments and type names.
- [ ] Run `cargo fmt`.
- [ ] Run focused config, package, scaffold and project-build tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the phase.

---

## Phase 5 - Stable semantic identities and public module interfaces

### Context and reason

Separate compilation is unsafe until every consumer-facing fact can survive outside the declaring module's local arenas. This phase establishes the reusable semantic boundary before changing project scheduling.

### Slice 5A - Stable declaration identities

- [ ] Introduce stable module-qualified identities for exported functions, nominal types, constants, traits and reusable evidence.
- [ ] Assign declaration IDs deterministically from canonical header/declaration order.
- [ ] Keep source aliases and public aliases as names mapping to identities.
- [ ] Keep private declarations out of public identity maps.
- [ ] Add remapping support only for interned strings actually stored in interface facts.

### Slice 5B - Canonical type identity

- [ ] Define the canonical cross-module type representation.
- [ ] Convert exported function signatures, fields, variants, aliases, bounds and constant types into canonical identities.
- [ ] Add a module-local map from canonical type identities to local `TypeId` handles.
- [ ] Represent imported nominals as local proxies that retain their declaring module identity.
- [ ] Preserve transparent aliases without creating new nominal identity.
- [ ] Ensure diagnostics can render imported canonical types through the consumer or declaring context without storing formatted names.

### Slice 5C - Public interface model

- [ ] Add one immutable `CompiledModuleInterface` owner.
- [ ] Include exported declaration maps, canonical type facts, folded constants, generic template descriptors, trait/evidence facts, receiver-method surfaces and function effect summaries.
- [ ] Keep root runtime activity, private HIR and private diagnostics out of the interface.
- [ ] Add explicit validation that no private declaration leaks through an exported signature, field, alias, bound or folded constant type.
- [ ] Add an in-memory interface fingerprint input model for future incremental work without implementing persistent hashes.

### Slice 5D - Import preparation consumes interfaces

- [ ] Replace imported-file declaration copying with interface lookup records.
- [ ] Make `ScopeContext` and type resolution distinguish local declarations, module declarations and external package declarations explicitly.
- [ ] Preserve source-level grouped and namespace imports without turning module paths into first-class values.
- [ ] Resolve receiver methods and trait evidence through canonical exported surfaces.
- [ ] Delete any importer-side reconstruction of a dependency's public API.

### Phase 5 acceptance

- [ ] No public interface stores donor-local `TypeId`, AST node or HIR index identity.
- [ ] A consumer can type-check imported signatures using local handles mapped to canonical types.
- [ ] Export privacy is validated once at the declaring module boundary.
- [ ] Import aliases never alter semantic identity.

### Phase 5 audit / style guide / validation

- [ ] Audit every interface field for ownership and stable identity.
- [ ] Audit type comparison for rendered-name or cross-environment local-ID mistakes.
- [ ] Audit exported traits, generic bounds and receiver methods.
- [ ] Run `cargo fmt`.
- [ ] Run focused type, import, trait, method and diagnostic tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check` if interface construction affects hot AST paths.
- [ ] Update the capsule and commit the phase.

---

## Phase 6 - One-module frontend contract, explicit HIR targets and effect summaries

### Context and reason

The frontend must accept only a module's owned files plus immutable dependency interfaces. This is the semantic cutover that makes global compile-once scheduling possible.

### Slice 6A - Prepared canonical module input

- [ ] Replace flattened `InputFile` closures with a `PreparedModule` input containing only owned files and resolved dependency interfaces.
- [ ] Keep source assets owned by the module that imports them.
- [ ] Build the module source table from owned files only.
- [ ] Pass root kind and dormant activity role explicitly.
- [ ] Preserve per-file parallel preparation and deterministic string-table merge inside one module.
- [ ] Ensure dependency source text is never attached to the consumer compiler.

### Slice 6B - AST cross-module values and types

- [ ] Resolve imported functions, constants, types, traits and evidence through interface records.
- [ ] Fold module-owned constants and const templates once.
- [ ] Consume exported folded constants without re-running provider AST folding.
- [ ] Preserve useful call-site and declaration-site diagnostic context.
- [ ] Ensure imported declaration dependency ordering does not enter the consumer's local header sort.

### Slice 6C - Explicit HIR call targets

- [ ] Replace path-only or local-only source call targets with an enum that distinguishes local, module, generated-generic and external calls.
- [ ] Lower ordinary cross-module calls to stable `ModuleFunctionId` targets.
- [ ] Keep private local calls compact.
- [ ] Validate that backends can recover the callee module and exported signature without source paths.
- [ ] Add HIR invariant tests that avoid incidental vector indexes.

### Slice 6D - Borrow/effect summaries

- [ ] Extend current function summary production to the complete public effect contract.
- [ ] Store exported summaries in the public interface.
- [ ] Make call transfer consume module-function summaries exactly like resolved external metadata, without reading foreign HIR.
- [ ] Borrow-check each canonical module function and dormant start once.
- [ ] Distinguish fresh return, alias-of-parameter, multi-parameter alias and unknown/imprecise alias results.
- [ ] Include reactive invalidation facts only where cross-module consumers require them.

### Slice 6E - Temporary orchestration convergence

- [ ] Route the existing directory orchestration through the new one-module compiler rather than maintaining two semantic frontend implementations.
- [ ] If entry-oriented scheduling still repeats a canonical module temporarily, ensure it calls the same compiler and returns the same artifact shape.
- [ ] Do not retain an adapter after Phase 7.
- [ ] Add an internal assertion that a module artifact contains only owner-local source identities.

### Phase 6 acceptance

- [ ] One module can be compiled from owned files and dependency interfaces alone.
- [ ] Cross-module HIR targets are explicit.
- [ ] Constants and templates are folded once per declaring module invocation.
- [ ] Borrow validation does not inspect foreign HIR.
- [ ] There is one semantic module compiler implementation.

### Phase 6 audit / style guide / validation

- [ ] Audit source tables and AST/HIR for foreign source leakage.
- [ ] Audit HIR target enums and backend-facing identity.
- [ ] Audit borrow summaries against current memory design.
- [ ] Run `cargo fmt`.
- [ ] Run focused AST, HIR, borrow and cross-module integration tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the phase.

---

## Phase 7 - Project-wide canonical compilation scheduler

### Context and reason

The semantic boundary now exists. This phase removes the root cause of repeated work by replacing entry-closure discovery and compilation with one project-wide dependency schedule.

### Slice 7A - Project compilation plan

- [ ] Replace `DiscoveredModule` with canonical module nodes and entry selections.
- [ ] Resolve module dependency edges from prepared import identities.
- [ ] Compute a deterministic dependency-first schedule.
- [ ] Compile support private subtrees before their facade.
- [ ] Compile normal child/support dependencies before consumers.
- [ ] Compile the project-root package facade last.
- [ ] Keep independent branches available for parallel scheduling.

### Slice 7B - Compile-once artifact store

- [ ] Allocate one artifact slot per `ModuleId`.
- [ ] Invoke the module compiler at most once for each module.
- [ ] Pass completed dependency interfaces to the consumer.
- [ ] Store failures by canonical module identity.
- [ ] Mark dependents blocked without generating cascaded semantic failures.
- [ ] Continue compiling independent branches for multi-error feedback.
- [ ] Merge string-table deltas and diagnostics in deterministic module order.

### Slice 7C - Deterministic parallel execution

- [ ] Begin with a clear serial dependency scheduler if needed for correctness.
- [ ] Add parallel execution only across ready independent modules.
- [ ] Do not use shared mutable semantic registries from workers without deterministic delta/merge ownership.
- [ ] Preserve deterministic IDs, diagnostics, interface order and artifact order across thread counts.
- [ ] Add tests under multiple `RAYON_NUM_THREADS` values.

### Slice 7D - Delete entry-closure compilation

- [ ] Delete reachable BFS per entry as the module compilation-unit owner.
- [ ] Retain only graph traversal needed to determine entry reachability and artifact linking.
- [ ] Delete entry-local copies of shared source text, tokens, headers, AST, HIR and borrow reports.
- [ ] Delete failure aggregation that appends repeated module diagnostics.
- [ ] Update counters to distinguish unique modules, entries and link operations.

### Phase 7 acceptance

- [ ] Module frontend invocation count equals canonical module count, not entry count times shared closure size.
- [ ] A broken shared module emits one diagnostic set.
- [ ] Independent failing modules still report independently.
- [ ] Results are deterministic across thread counts.
- [ ] No production path constructs `DiscoveredModule` entry closures.

### Phase 7 audit / style guide / validation

- [ ] Audit scheduling state for clear ownership and no broad graph utility leakage.
- [ ] Audit failure blocking and diagnostic counts.
- [ ] Audit string-table and ID merge determinism.
- [ ] Run `cargo fmt`.
- [ ] Run focused scheduler, multi-error and determinism tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-frontend-check` and `just bench-check`.
- [ ] Update the capsule with before/after counters and commit the phase.

---

## Phase 8 - Project-wide generic instance materialisation

### Context and reason

Canonical module compilation removes repeated provider work, but consumer-local generic instance emission would still duplicate concrete functions across entries and modules. This phase completes the reusable executable model.

### Slice 8A - Stable generic keys and requests

- [ ] Replace donor-local `TypeId` generic keys with canonical concrete type identities.
- [ ] Key requests by stable generic declaration identity plus ordered concrete arguments.
- [ ] Give every unique key a deterministic `GenericInstanceId`.
- [ ] Record requesting call locations without making them part of semantic identity.
- [ ] Preserve recursion detection using stable instance keys.

### Slice 8B - Generated artifact sidecars

- [ ] Add project-owned generated artifact storage associated with the declaring module.
- [ ] Keep the base compiled module artifact immutable.
- [ ] Materialise concrete AST/HIR bodies using the declaring template and canonical substitution.
- [ ] Resolve imported types and calls through the same dependency interfaces as base module compilation.
- [ ] Borrow-check each generated concrete function.
- [ ] Export generated effect summaries for callers.

### Slice 8C - Deterministic worklist

- [ ] Process requests through a deduplicating worklist because generated bodies may request more instances.
- [ ] Detect recursive or cyclic instantiation with useful call/declaration context.
- [ ] Ensure one invalid instance produces one diagnostic set even when requested by multiple entries.
- [ ] Choose and document which requesting site is primary when several equivalent calls exist.
- [ ] Preserve all distinct call-site inference diagnostics before a request is accepted.

### Slice 8D - HIR and backend targets

- [ ] Lower generic calls to `GenericInstanceId` targets.
- [ ] Remove consumer-local generated function names as semantic identity.
- [ ] Make entry linking include only reachable generated instances.
- [ ] Delete old per-consuming-module materialisation and tests that depend on duplicated names.

### Phase 8 acceptance

- [ ] Equivalent generic requests across modules and entries produce one concrete instance.
- [ ] Generated instances are outside immutable base artifacts.
- [ ] HIR and backends see concrete stable targets.
- [ ] Generic diagnostics retain correct call and declaration context.

### Phase 8 audit / style guide / validation

- [ ] Audit canonical type keys and deterministic ordering.
- [ ] Audit generated artifact ownership and recursion handling.
- [ ] Audit borrow validation of generated functions.
- [ ] Run `cargo fmt`.
- [ ] Run focused generic, HIR, borrow and multi-entry tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the phase.

---

## Phase 9 - Project graph backend handoff and entry link plans

### Context and reason

Backends must consume canonical artifacts without rediscovering module relationships or assuming all runtime dependencies are reachable from one module-local start function.

### Slice 9A - Replace flat backend payload

- [ ] Replace `BackendBuilder::build_backend(Vec<Module>, ...)` with a graph-aware `ProjectCompilation` or equivalent payload.
- [ ] Keep backend-facing access narrow: module artifacts, generated artifacts, entry assemblies, package facade and package/runtime metadata.
- [ ] Remove source-path graph reconstruction from builders.
- [ ] Preserve project output and cleanup ownership.

### Slice 9B - Entry assembly

- [ ] Build one `EntryAssembly` for every active normal module with builder-relevant root activity.
- [ ] Activate only that module's dormant start and fragments.
- [ ] Compute reachable local, module, generic and external functions over the linked graph.
- [ ] Keep API-only normal modules importable but artifact-free.
- [ ] Exclude support and project-facade roots from entry selection.

### Slice 9C - Runtime dependency facts

- [ ] Replace start-filtered module-flat external import metadata with per-function or reachability-indexed facts.
- [ ] Aggregate provider assets, required runtime imports and Builder runtime packages through the entry link plan.
- [ ] Deduplicate by stable package/runtime identity.
- [ ] Preserve useful module attribution for diagnostics.
- [ ] Ensure package facade checking does not trigger runtime asset output.

### Slice 9D - HTML-JS migration

- [ ] Update the HTML builder to consume entry assemblies and linked module calls.
- [ ] Preserve current page routing, fragment interleaving and JS behavior.
- [ ] Allow self-contained JS per entry as the initial physical output policy.
- [ ] Emit linked module/generic functions once within each entry bundle.
- [ ] Add artifact assertions for API-only modules, support packages and shared-module calls.

### Slice 9E - Wasm plan handoff

- [ ] Document the stable graph, call-target, type-identity, effect-summary and entry-link contracts the Wasm plan receives.
- [ ] Do not decide physical Wasm module layout in this phase.
- [ ] Update the Wasm plan's stale current-state section only after this phase is accepted.

### Phase 9 acceptance

- [ ] Backends receive one graph-aware payload.
- [ ] Entry activation is explicit and separate from module compilation.
- [ ] Runtime assets follow linked reachability, not module-local start assumptions.
- [ ] HTML-JS output remains behaviorally compatible under the new graph.
- [ ] The Wasm plan no longer needs to invent the frontend module graph.

### Phase 9 audit / style guide / validation

- [ ] Audit backend boundaries for source rediscovery.
- [ ] Audit reachability and external asset ownership.
- [ ] Audit dormant start behavior for imported modules and support packages.
- [ ] Run `cargo fmt`.
- [ ] Run focused HTML builder, external provider and artifact tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the phase.

---

## Phase 10 - Obsolete path deletion and diagnostic hardening

### Context and reason

The refactor is not complete while old entry closures, configured source-package discovery, path fallbacks or consumer-local generic emission remain available. This phase is deletion-first.

### Slice 10A - Delete old project compilation owners

- [ ] Delete `DiscoveredModule` and entry-closure compile loops.
- [ ] Delete old reachable-source APIs used only to flatten dependencies into entries.
- [ ] Delete old directory failure append behavior.
- [ ] Delete flat `Vec<Module>` backend interfaces.
- [ ] Delete adapters and compatibility aliases introduced during migration.

### Slice 10B - Delete old package and resolver owners

- [ ] Delete project-local `package_folders` discovery files or reduce them to supplied Builder package preparation if still correctly named.
- [ ] Delete old root public-surface fallback and path-base variants.
- [ ] Delete importer-file-relative source resolution.
- [ ] Delete package-prefix precedence and shadowing tests.
- [ ] Rename files/modules whose old names no longer describe their owner.

### Slice 10C - Delete old semantic duplication

- [ ] Delete imported declaration/body copying.
- [ ] Delete donor-local cross-module type transport.
- [ ] Delete consumer-local generic materialisation.
- [ ] Delete borrow paths that inspect foreign HIR.
- [ ] Delete backend paths that infer call or package identity from rendered names.

### Slice 10D - Diagnostic review

- [ ] Review all new project-structure and import reasons for typed payloads.
- [ ] Ensure shared-module errors render once with the correct `TypeEnvironment`.
- [ ] Ensure blocked dependents do not emit cascades.
- [ ] Ensure terse counts, terminal output and dev-server payloads agree.
- [ ] Ensure internal malformed graph/interface states use `CompilerError`, not fabricated source diagnostics.

### Phase 10 acceptance

- [ ] One production path remains for module discovery, import resolution, module compilation, generic materialisation and backend handoff.
- [ ] Greps find no obsolete config keys, resolver modes, entry closures or compatibility shims.
- [ ] Diagnostic counts reflect canonical source failures.

### Phase 10 audit / style guide / validation

- [ ] Perform a repository-wide stale API and comment audit.
- [ ] Review module/file sizes and split mixed owners rather than hiding them behind helpers.
- [ ] Review panic, `todo!`, `.unwrap()` and ignored-result paths touched by the refactor.
- [ ] Run `cargo fmt`.
- [ ] Run focused negative diagnostics and internal invariant tests.
- [ ] Run `just validate`.
- [ ] Run `just bench-check`.
- [ ] Update the capsule and commit the phase.

---

## Phase 11 - Performance, determinism and scale proof

### Context and reason

The original bug exposed duplicated semantic work. Completion requires measured proof that the new architecture scales with unique modules rather than entries times shared closure size.

### Slice 11A - Stable counters

- [ ] Record canonical modules discovered.
- [ ] Record owned files prepared.
- [ ] Record module frontend invocations.
- [ ] Record dependency-interface imports.
- [ ] Record generic requests, unique instances and dedup hits.
- [ ] Record entry link operations separately from module compilation.
- [ ] Record blocked dependents and independent failures where useful.
- [ ] Keep counters concise and avoid default output noise.

### Slice 11B - Canonical benchmark shapes

- [ ] Add or update a many-entry, heavily shared module graph benchmark.
- [ ] Add a deep normal-module tree with scoped support packages.
- [ ] Add a package-facade/private-subtree benchmark.
- [ ] Add generic requests repeated across several entries.
- [ ] Keep correctness assertions in integration tests, not benchmark code.

### Slice 11C - Acceptance measurements

- [ ] Prove module frontend invocation count equals unique module count.
- [ ] Prove each shared source file is prepared once.
- [ ] Prove one shared unknown-name error appears once.
- [ ] Compare Stage 0, AST, HIR and borrow timings against Phase 0 baseline.
- [ ] Confirm no broad single-entry regression beyond accepted noise.
- [ ] Confirm deterministic outputs and diagnostics under several thread counts.
- [ ] Profile any material regression and either fix it or record an explicit accepted tradeoff before completion.

### Phase 11 acceptance

- [ ] Multi-entry shared projects show substantial reduction in total frontend work.
- [ ] Single-entry builds remain competitive.
- [ ] No duplicate module diagnostics remain.
- [ ] Parallel scheduling is deterministic.
- [ ] Measurements distinguish frontend reuse from physical output duplication.

### Phase 11 audit / style guide / validation

- [ ] Review instrumentation for stable names and clear ownership.
- [ ] Review benchmark fixtures for realistic structure and no correctness role.
- [ ] Run `cargo fmt`.
- [ ] Run focused counter tests.
- [ ] Run `just bench-frontend-check`.
- [ ] Run `just bench-check`.
- [ ] Run `just validate`.
- [ ] Update the capsule with final evidence and commit the phase.

---

## Phase 12 - Documentation, progress matrix, scaffolds and migration guidance

### Context and reason

The current documentation teaches the wrong import base and permits dependency shapes that the accepted design rejects. This phase must make project structure obvious, not merely technically accurate.

### Slice 12A - Canonical language and project docs

- [ ] Update `docs/language-overview.md` with the final `#` and `+` root model.
- [ ] Update project-structure source pages and their basic variants.
- [ ] Update package/import source pages and their basic variants.
- [ ] Add a complete project tree showing normal modules, support packages, private package subtrees and a root package facade.
- [ ] Add legal/illegal dependency tables.
- [ ] Explain that `export:` controls visibility while topology controls whether an import is legal.
- [ ] Explain that root filenames are cosmetic and directory names define module/package paths.

### Slice 12B - Make module-root-relative imports unmistakable

- [ ] Include a deep importing file and show the same bare import resolving to the owning module root.
- [ ] Explicitly state that imports are not relative to the importing file.
- [ ] Explicitly reject `@./` and `..` examples.
- [ ] Show internal directory traversal, direct child module imports and support-package imports side by side.
- [ ] Show facade boundary termination and invalid `@child/internal` examples.
- [ ] Show the project-root facade's special entry-root-anchored assembly imports.
- [ ] Ensure getting-started examples use only the new model.

### Slice 12C - Compiler-design docs

- [ ] Update Stage 0 ownership and module-tree contracts.
- [ ] Update imports/packages/bindings with scoped support namespaces and no-precedence resolution.
- [ ] Update type identity docs with canonical cross-module identity and local handle mapping.
- [ ] Update HIR docs with module and generic call targets.
- [ ] Update borrow docs with exported effect summaries.
- [ ] Update parallelism docs with dependency-ready module scheduling and deterministic merge.
- [ ] Update backend handoff docs with `ProjectCompilation` and entry assemblies.
- [ ] Update source comments and `index.md` locators where owners moved.

### Slice 12D - Progress matrix

Review existing rows first and consolidate rather than adding duplicates. The matrix must clearly record:

- [ ] normal `#` modules and dormant starts: supported
- [ ] scoped `+` support packages: supported
- [ ] optional project package facade: supported as a source API surface
- [ ] module-root-relative imports and no ancestor paths: supported
- [ ] strict module topology and namespace collision enforcement: supported
- [ ] canonical compile-once module frontend: supported
- [ ] project-wide concrete generic instance reuse: supported
- [ ] module effect summaries and graph backend handoff: supported
- [ ] consuming separate third-party Beanstalk projects: deferred
- [ ] persistent incremental compilation and serialized module/package caches: deferred
- [ ] direct normal-sibling imports: deferred design fallback
- [ ] cross-entry JS chunks and shared physical output: deferred
- [ ] physical Wasm module layout: owned by the following Wasm plan

### Slice 12E - Roadmap and future-plan alignment

- [ ] Keep this plan immediately before the HTML Wasm plan while active.
- [ ] Replace the old generic incremental-build note with the precise persistent-cache deferral.
- [ ] Replace `source-backed package HIR caching` wording with canonical artifact persistence and dependency-package cache wording.
- [ ] Record direct normal-sibling imports as a deliberate fallback only if support packages prove too restrictive.
- [ ] Keep package manager, versioning, fetching and lockfiles deferred.
- [ ] Update the Wasm plan's current-state section to consume the landed graph and remove obsolete `Vec<Module>` migration assumptions.
- [ ] Remove or rewrite the roadmap note claiming source-backed packages are compiled into each consumer.

### Slice 12F - Generated docs and migration review

- [ ] Rebuild generated documentation through the compiler.
- [ ] Inspect every changed route, tree diagram, import example and table.
- [ ] Search generated and source docs for `@./`, `package_folders`, old library-folder language and unrestricted cross-module imports.
- [ ] Verify package and module terminology is consistent.
- [ ] Add concise migration guidance for existing pre-release projects without compatibility syntax.

### Phase 12 acceptance

- [ ] A new reader can predict legal imports from the directory tree.
- [ ] The docs explicitly distinguish module-root-relative imports from file-relative imports.
- [ ] Progress and roadmap distinguish landed architecture from deliberately deferred package/incremental/output work.
- [ ] Generated docs are compiler-built and reviewed.

### Phase 12 audit / style guide / validation

- [ ] Review every changed documentation source and generated route.
- [ ] Review examples against real integration fixtures.
- [ ] Review the progress matrix for status accuracy rather than aspirational design.
- [ ] Run `cargo fmt` if code or fixtures changed.
- [ ] Run `just validate` because this phase includes source fixtures, scaffolds and plan alignment.
- [ ] Run the documentation release build and inspect the generated diff.
- [ ] Update the capsule and commit the phase.

---

## Phase 13 - Final independent audit and Wasm handoff

### Context and reason

This refactor changes project structure, frontend identity, HIR, borrow checking, package semantics and backend contracts. Completion requires a whole-plan audit, not only passing tests.

### Slice 13A - Design acceptance audit

- [ ] Verify every binding design decision in this plan against source, tests and docs.
- [ ] Verify normal modules cannot import parents, normal siblings, grandchildren, sibling descendants, cousins or private files.
- [ ] Verify support visibility, private subtrees and outer-support layering exactly match the accepted model.
- [ ] Verify the root project facade is external-only and config-named.
- [ ] Verify import names never shadow or depend on fallback order.
- [ ] Verify root filenames remain cosmetic.

### Slice 13B - Compiler-boundary audit

- [ ] Stage 0 owns structure and namespaces, not semantic type checking.
- [ ] Header/import preparation owns import binding and exported surfaces.
- [ ] AST owns semantic resolution, generic requests and folding.
- [ ] HIR carries explicit stable call targets.
- [ ] Borrow validation consumes HIR and exports summaries without mutating HIR.
- [ ] Build orchestration owns scheduling, generated instances and entry link plans.
- [ ] Backends consume compiled graph facts without reading source syntax.

### Slice 13C - Duplication and stale-path audit

- [ ] Search for old entry closures, flat backend payloads, package-folder config, importer-file-relative source paths and public-surface fallback.
- [ ] Search for duplicated module AST/HIR/type environments in entry outputs.
- [ ] Search for donor-local IDs crossing interfaces.
- [ ] Search for consumer-local duplicate generic instances.
- [ ] Search for foreign-HIR borrow inspection.
- [ ] Search for stale docs and comments.

### Slice 13D - Diagnostics and tests audit

- [ ] Verify user failures use structured diagnostics with useful labels.
- [ ] Verify malformed graph/interface states use `CompilerError`.
- [ ] Verify shared failures appear once in terminal, terse and dev-server forms.
- [ ] Verify tests have one primary owner per behavior.
- [ ] Verify benchmarks are not used as correctness coverage.
- [ ] Verify backend artifact tests consume graph behavior rather than incidental order.

### Slice 13E - Final validation and completion

- [ ] Run `cargo fmt`.
- [ ] Run all focused suites named by the final capsule.
- [ ] Run `just validate`.
- [ ] Run `just bench-frontend-check`.
- [ ] Run `just bench-check`.
- [ ] Run the documentation release build if not already included in the final validation state.
- [ ] Record exact results, totals and any unrelated failures.
- [ ] Refresh the Wasm plan one final time against the landed contracts.
- [ ] Mark this plan complete and remove its active roadmap link only after user acceptance.

### Final acceptance

- [ ] Unique module count, not entry closure count, determines frontend work.
- [ ] All source and package imports obey the final structural namespace.
- [ ] Scoped support packages provide practical project-local reuse without normal sibling dependencies.
- [ ] Public interfaces and generated artifacts use stable identities.
- [ ] Constants, templates, generics and borrow facts are not redundantly recomputed.
- [ ] Entry and package facade assembly are explicit.
- [ ] The HTML Wasm plan can start without redesigning frontend modules.
- [ ] Source, tests, docs, progress matrix and roadmap agree.

---

## Deliberately deferred work after this plan

These items must be explicit in the roadmap and progress matrix where applicable:

- persistent module and package artifact serialization
- source-hash and public-interface-hash invalidation policy
- retained dev-server module artifacts across rebuilds
- selective downstream invalidation
- cross-project dependency declarations and local path dependencies
- package registries, remote fetching, versions, lockfiles and overrides
- precompiled dependency package caches
- direct normal-sibling imports with cycle detection
- cross-entry JavaScript chunking and shared browser bundles
- general JavaScript tree shaking or minification
- physical Wasm module partition and Component Model integration
- cross-build generic instance caches

The module graph, stable identities and dependency records implemented here must make those features possible without requiring another frontend ownership rewrite.

## Plan completion record

When the plan is accepted, replace this section with a concise final record containing:

- final accepted commit
- final architecture summary
- validation commands and results
- benchmark/counter evidence
- documentation and progress-matrix changes
- deferred follow-up links
- exact Wasm plan handoff

Do not turn this plan into a chronological diary. Keep the active context capsule current and retain only concise accepted-phase notes needed for future reloads.
