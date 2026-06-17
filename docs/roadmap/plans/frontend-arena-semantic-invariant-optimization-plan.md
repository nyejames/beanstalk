# Frontend Arena + Semantic Invariant Optimisation Plan

## Purpose

This plan defines a cautious, evidence-based optimisation programme for the Beanstalk compiler frontend. The focus is to reduce allocation, cloning, remapping, and repeated semantic work in hot frontend paths while preserving compiler correctness, diagnostics, and the current stage ownership model.

The first implementation target is **typed `Vec` arenas with stable ID handles**, seeded by capacity heuristics gathered during tokenization/header preparation. The first structural refactor target is **scope-frame arenas**, because Beanstalk’s no-shadowing rule gives a direct opportunity to replace cloned scope contexts with parent-linked frames.

This plan is deliberately staged. Each phase must produce benchmark/profile evidence and must be willing to backtrack if an expected optimisation does not improve the targeted measurements.

---

## Final Agreed Interview Decisions

- [x] Use typed `Vec` arenas with stable ID handles first.
- [x] Defer `bumpalo`, dropless bump allocation, and lifetime-heavy arena references until profiling justifies them.
- [x] Implement scope-frame arenas before expression/template arenas.
- [x] Start with an instrumentation, benchmark, and profiling baseline phase before optimisation refactors.
- [x] Require before/after profiling and benchmark evidence for every optimisation phase.
- [x] Use repeatable performance gates with a rollback/scrutiny rule.
- [x] Commit adversarial high-churn Beanstalk benchmark fixtures under `./benchmarks`.
- [x] Include deterministic fixture generation only where useful, with committed generated fixtures as the canonical benchmark inputs.
- [x] Include an early external package registry clone-reduction phase.
- [x] Allow modest initial over-allocation for capacity heuristics.
- [x] Store optimisation reports under `benchmarks/`, not roadmap.
- [x] Treat capacity heuristics as tunable policy, never correctness logic.
- [x] Add a roadmap/progress matrix item for **Frontend Arena + Semantic Invariant Optimisation**.
- [x] Mark deeper allocator work and broader arena migrations as deliberately deferred until profiling justifies them.
- [x] Replace the existing scope-context cloning path directly; do not add compatibility wrappers.
- [x] Produce `TokenStats`, `HeaderStats`, and `FrontendArenaCapacityEstimate` unconditionally, but keep verbose reporting behind existing instrumentation/benchmark output.
- [x] Use five repeated benchmark invocations and compare medians. The existing benchmark runner’s internal warmup/measured iteration model should remain unless a later phase explicitly justifies changing it.
- [x] Use Samply for targeted profiles, not every fixture.
- [x] Require semantic/output equivalence checks for every phase.
- [x] Keep arena-specific types, capacity heuristics, counters, and optimisation helpers out of core pipeline files when they grow beyond small orchestration glue.

---

## Current Repo Shape After Interview

### Frontend orchestration

Current frontend module compilation is coordinated in:

```text
src/build_system/create_project_modules/frontend_orchestration.rs
```

The current module pipeline is:

```text
attach source files
prepare module files
sort headers
build AST
resolve const fragments
lower HIR
check borrows
collect reachability
finalize Module
```

Important current implementation facts:

- File preparation is already fused as tokenization + header parsing and runs per file before module aggregation.
- Stage timings are already wrapped around file preparation, dependency sorting, AST, HIR, and borrow checking.
- `CompilerFrontend::new(...)` currently receives cloned `style_directives`, cloned `external_packages`, and cloned `project_path_resolver`.
- The returned `Module` currently stores a cloned `external_package_registry`.
- `record_module_input_counters(...)` already records module count, source file count, and source byte count.
- `record_header_counters(...)` already records header count, import count, and top-level declaration count.

### Profiling profile

Current `Cargo.toml` has:

```toml
[profile.profiling]
inherits = "release"
debug = "line-tables-only"
strip = false
lto = "thin"
codegen-units = 1
panic = "abort"
```

This is suitable for Samply profiling while retaining useful symbols.

### Instrumentation

Current frontend instrumentation lives under:

```text
src/compiler_frontend/instrumentation/
```

The existing `FrontendCounter` surface already includes counters for:

- module/file/source/token/header/import/declaration volume;
- dependency sorting volume;
- AST construction and compile-time evaluation volume;
- HIR and borrow validation volume;
- type-environment cache/query pressure;
- string-table full clones and merge pressure;
- module string-ID remap calls.

This is the right owner for new local-only optimisation counters. Do not scatter ad hoc counters through unrelated modules.

### Header/token preparation

Current header-stage data contracts live in:

```text
src/compiler_frontend/headers/types.rs
src/compiler_frontend/headers/parse_file_headers.rs
```

`FileFrontendPrepareOutput` already stores:

- `token_count`;
- `file_imports`;
- `headers`;
- `top_level_const_fragments`;
- `const_template_count`;
- `runtime_fragment_count`;
- warnings.

This is the best early place to add `TokenStats` and aggregate `HeaderStats` without adding extra full-pipeline traversals.

### Benchmarks

Current benchmark documentation lives in:

```text
benchmarks/README.md
```

Existing commands:

```bash
just bench-check
just bench-frontend-check
just bench
just bench-frontend
just bench-report
just profile-build
```

Current benchmark case files:

```text
benchmarks/cases.txt
benchmarks/frontend-cases.txt
```

Current benchmark fixture groups include `core`, `docs`, `stress`, `module`, and `borrow`. The existing `stress` group already covers template, type, fold, pattern, collection, and environment stress. The new adversarial fixtures should extend this group unless a new group clearly improves summary readability.

Raw local history and raw profiler output must remain uncommitted.

---

## Non-Negotiable Constraints

- [ ] Optimisations must be semantics-neutral unless a phase explicitly says otherwise.
- [ ] Capacity estimates must never affect diagnostics, ordering, lowering, type identity, or emitted artifacts.
- [ ] If an estimate is too small, the arena grows normally.
- [ ] If an estimate is too large, the only acceptable effect is bounded memory overhead.
- [ ] Keep `StringTable` fork/merge/remap semantics intact.
- [ ] Keep typed diagnostics on `CompilerDiagnostic` and infrastructure failures on `CompilerError`.
- [ ] Run `just validate` before finishing every non-trivial implementation phase.
- [ ] Perform manual stage-boundary review for frontend boundary cleanup.
- [ ] Do not add compatibility wrappers for obsolete APIs.
- [ ] Do not commit raw Samply profiles or `benchmarks/local-data/`.
- [ ] Do not add failing diagnostic cases as benchmarks.

---

## Semantic Invariants To Exploit

These invariants should be documented in the optimisation report and used as review checks when evaluating new arena/scope/lookup code.

### No visible shadowing

Beanstalk forbids visible redeclaration while a name is still in scope.

Optimisation use:

- [ ] Replace cloned scope-context maps with parent-linked `ScopeFrameId`s.
- [ ] Use redeclaration checks against ancestor frames instead of maintaining shadow stacks.
- [ ] Avoid “nearest binding wins” candidate resolution machinery.
- [ ] Store local declarations in frame-local maps and ranges.

### Header parsing owns top-level discovery

Header parsing discovers top-level declarations and creates declaration shells. AST consumes sorted headers and must not rediscover top-level declarations from raw tokens.

Optimisation use:

- [ ] Use header counts as declaration arena capacity seeds.
- [ ] Avoid extra AST scans for top-level discovery.
- [ ] Keep declaration-shell parsing shared, not duplicated.

### Dependency sorting is authoritative

AST receives declarations in dependency order.

Optimisation use:

- [ ] Avoid AST fixpoint passes for constants, aliases, structs, choices, and signatures.
- [ ] Add new top-level dependency edges to header/dependency sorting instead of compensating in AST.
- [ ] Treat missing sorted-order assumptions as bugs or missing header edges.

### Header-built visibility is authoritative

Header import preparation builds file-local visibility. AST consumes it through scope lookup.

Optimisation use:

- [ ] Scope frames store body-local declarations only.
- [ ] Immutable file/module visibility tables are shared by reference/ID.
- [ ] Avoid copying import visibility into every child context.

### One entry start path

Only the module entry file owns top-level runtime code. Non-entry files contribute declarations only.

Optimisation use:

- [ ] Allocate start-specific structures only when `HeaderKind::StartFunction` exists.
- [ ] Use entry runtime/const fragment counts from file preparation instead of rescanning all files later.

### Generics resolve before HIR

HIR never carries unresolved generic executable types or unsolved generic calls.

Optimisation use:

- [ ] Keep generic templates in AST-only storage.
- [ ] Drop generic template scratch before/at HIR handoff when no longer needed.
- [ ] Avoid broad constraint graph machinery; inference is local to immediate call evidence and immediate expected result context.

### Traits are static metadata

Traits are frontend compile-time metadata, not runtime values or trait objects.

Optimisation use:

- [ ] Use compact evidence maps keyed by stable IDs.
- [ ] Avoid vtables, erased dispatch metadata, and runtime trait object lowering.
- [ ] Resolve bound-provided receiver calls before HIR.

### External packages expose free functions only

External packages expose opaque types, constants, and free functions. They do not expose receiver methods.

Optimisation use:

- [ ] Avoid external receiver-method candidate catalogs.
- [ ] Resolve external calls to stable IDs early.
- [ ] Share immutable external package metadata instead of cloning it per module where practical.

### Canonical `TypeId` semantic identity

Semantic type decisions use `TypeId` in the module `TypeEnvironment`. `DataType` is parse-only or diagnostic-only after semantic resolution.

Optimisation use:

- [ ] Arena nodes store `TypeId`s and compact IDs, not cloned semantic type trees.
- [ ] Query borrowed field/variant views instead of cloning member lists.
- [ ] Keep rendered type names at diagnostic render boundaries.

### No closures or general function values

General closures, anonymous function values, generic function values, and higher-order polymorphism are outside current language scope.

Optimisation use:

- [ ] No closure-capture environment objects.
- [ ] Function-local scope data does not escape as runtime function values.
- [ ] Arena lifetimes can stay stage/module scoped without capture promotion.

### No macro expansion language

General macros are outside current language scope.

Optimisation use:

- [ ] No macro expansion arena.
- [ ] No hygiene context.
- [ ] No repeated parse/expand/fold loop.

### Borrow validation is side-table based

Borrow validation reads HIR and produces side-table facts. It does not mutate HIR.

Optimisation use:

- [ ] Borrow facts can become dense side-table arenas keyed by HIR IDs later.
- [ ] HIR nodes should not grow borrow-state fields.
- [ ] Snapshot-heavy diagnostics can be investigated separately from semantic borrow facts.

---

## Evidence and Rollback Protocol

### Required benchmark protocol

At every phase boundary:

- [ ] Run five independent benchmark invocations and compare medians.
- [ ] Preserve the existing benchmark runner’s internal measured-iteration model unless a dedicated benchmark-system phase changes it.
- [ ] Run focused frontend benchmarks for compiler-stage refactors.
- [ ] Run end-to-end CLI benchmarks before merging substantial changes.
- [ ] Run the docs project build/check path.
- [ ] Run targeted adversarial fixtures relevant to the phase.
- [ ] Record summarized results in `benchmarks/frontend-optimization-results.md`.
- [ ] Do not commit raw local history or raw profiler files.

Suggested command shape:

```bash
# Validate semantics.
just validate

# Fast local checks while iterating.
just bench-frontend-check
just bench-check

# Phase-boundary recorded benchmark runs.
just bench-frontend
just bench

# Local drilldown.
just bench-report
```

For five independent invocations:

```bash
for i in 1 2 3 4 5; do
    just bench-frontend
    just bench
done

just bench-report
```

### Required Samply profiling protocol

Use Samply at baseline and after major refactors, not for every fixture.

```bash
just profile-build
samply record ./target/profiling/bean build docs --release
```

If `just profile-build` does not build with `detailed_timers`, use:

```bash
cargo build --profile profiling --features detailed_timers
samply record ./target/profiling/bean build docs --release
```

Targeted fixture examples:

```bash
samply record ./target/profiling/bean check benchmarks/template-stress.bst
samply record ./target/profiling/bean check benchmarks/environment-stress.bst
samply record ./target/profiling/bean check benchmarks/adversarial/one-module-kitchen-sink.bst
```

### Regression threshold

- [ ] Treat a median regression greater than 5% in `docs --release`, the focused frontend suite, or a targeted adversarial fixture as a blocker unless the phase has a documented, accepted tradeoff.
- [ ] If a change was expected to improve performance but does not move the relevant timing/counter/profile, scrutinize, revise, or revert it.
- [ ] Do not treat counter movement as a win unless timing also moves meaningfully or Samply proves the hotspot moved away.
- [ ] If results are mixed, inspect per-case local data and run targeted profiles before drawing conclusions.

---

## Organisation and Modularity Rules For This Refactor

Arena and optimisation support code must not bloat core pipeline files.

### Keep pipeline files focused

Files such as:

```text
src/build_system/create_project_modules/frontend_orchestration.rs
src/compiler_frontend/pipeline.rs
src/compiler_frontend/ast/mod.rs
```

should remain orchestration files. They may construct or pass a context, but they should not grow large capacity formulas, arena internals, or benchmark-specific helper logic.

### Recommended new module layout

Use a dedicated frontend optimisation/arena area for reusable support:

```text
src/compiler_frontend/arena/
    mod.rs
    ids.rs
    typed_arena.rs
    capacity.rs
    token_stats.rs
    header_stats.rs
    actuals.rs
```

Use AST-local modules for AST-owned arena structures:

```text
src/compiler_frontend/ast/module_ast/scope_context/
    mod.rs
    context.rs
    scope_arena.rs
    scope_frame.rs
    lookup.rs
```

If expression/template arenas are later implemented, prefer focused owners:

```text
src/compiler_frontend/ast/expressions/
    arena.rs
    scratch.rs

src/compiler_frontend/ast/templates/
    arena.rs
    render_plan_arena.rs
```

If benchmark generators are needed, prefer one clear tooling owner:

```text
benchmarks/generators/
    README.md
    generate_adversarial.rs or generate_adversarial.py
```

or extend `xtask` if that already owns repo tooling in the current implementation.

### File organisation checklist

- [ ] Add file-level docs to every new arena/stats/capacity module.
- [ ] Keep capacity formulas centralized in `capacity.rs`.
- [ ] Keep actual allocation/capacity reporting centralized in `actuals.rs` or instrumentation modules.
- [ ] Keep IDs in `ids.rs` or the module that owns the storage.
- [ ] Avoid long parameter lists by passing named context structs.
- [ ] Avoid generic helper abstractions that hide stage ownership.
- [ ] Delete old wrappers once new APIs are wired.
- [ ] Keep test-only helpers in test modules, not production files.

---

## Phase 0 — Baseline, Current-State Audit, and Report File

### Summary

This phase creates the evidence baseline and report structure before changing optimisation-sensitive code. It should capture the current compiler shape, current benchmark behaviour, and current Samply hotspot profile. No major optimisation refactor belongs in this phase.

### Implementation steps

- [ ] Create `benchmarks/frontend-optimization-results.md`.
- [ ] Add a baseline section with:
  - [ ] date;
  - [ ] commit SHA;
  - [ ] machine/OS/CPU notes;
  - [ ] Rust toolchain;
  - [ ] command list;
  - [ ] benchmark suite versions/case list;
  - [ ] raw profile filenames stored locally only.
- [ ] Run semantic validation:
  - [ ] `just validate`.
- [ ] Run benchmark checks:
  - [ ] `just bench-frontend-check`;
  - [ ] `just bench-check`.
- [ ] Run five recorded benchmark invocations:
  - [ ] `just bench-frontend` five times;
  - [ ] `just bench` five times;
  - [ ] summarize medians with `just bench-report`.
- [ ] Build a profiling binary:
  - [ ] `just profile-build`, or `cargo build --profile profiling --features detailed_timers` if needed.
- [ ] Record baseline Samply profiles:
  - [ ] `samply record ./target/profiling/bean build docs --release`;
  - [ ] one targeted profile for existing `template-stress.bst`;
  - [ ] one targeted profile for existing `environment-stress.bst`.
- [ ] Summarize top findings in `benchmarks/frontend-optimization-results.md`:
  - [ ] top stage timings;
  - [ ] top counters;
  - [ ] top sampled functions from Samply;
  - [ ] obvious clone/allocation/drop hotspots;
  - [ ] no raw profile data.
- [ ] Record the agreed semantic invariants and the intended optimisation use of each invariant.

### Audit / style guide review / validation

- [ ] Confirm no compiler semantics changed.
- [ ] Confirm no raw local data or profiles were committed.
- [ ] Confirm benchmark report stays concise and does not duplicate raw counter tables.
- [ ] Confirm `just validate` passes.

### Backtrack criteria

- [ ] If the benchmark report workflow is too noisy or hard to reproduce, simplify it before proceeding.
- [ ] If baseline profiles are too short to interpret, profile larger targeted cases or repeat workloads before optimisation phases begin.

---

## Phase 1 — TokenStats, HeaderStats, and Capacity Estimate Framework

### Summary

This phase adds cheap, unconditional statistics that piggyback on work already performed during tokenization and header aggregation. It also introduces the central capacity-estimate model used by typed arenas. This phase should not alter compiler semantics.

### Implementation steps

- [ ] Add `TokenStats` in a dedicated module, preferably:

```text
src/compiler_frontend/arena/token_stats.rs
```

or another clearly named frontend stats module if the final owner is different.

- [ ] Track cheap token counts while tokenization already creates tokens:
  - [ ] total tokens;
  - [ ] symbols;
  - [ ] literals;
  - [ ] operators;
  - [ ] template starts/body markers;
  - [ ] style directives;
  - [ ] imports;
  - [ ] hashes;
  - [ ] `if`;
  - [ ] `loop`;
  - [ ] `catch`;
  - [ ] `then`;
  - [ ] returns;
  - [ ] casts;
  - [ ] mutable markers;
  - [ ] map/collection delimiters.

- [ ] Add `TokenStats` to `FileFrontendPrepareOutput`.
- [ ] Remap handling must remain unaffected; stats contain counts only and need no string-ID remap.
- [ ] Add `HeaderStats` in a dedicated module, preferably:

```text
src/compiler_frontend/arena/header_stats.rs
```

- [ ] Compute `HeaderStats` during `parse_headers(...)` aggregation or adjacent header-counter recording:
  - [ ] functions;
  - [ ] constants;
  - [ ] structs;
  - [ ] choices;
  - [ ] type aliases;
  - [ ] traits;
  - [ ] conformances;
  - [ ] trait incompatibilities;
  - [ ] const templates;
  - [ ] start functions;
  - [ ] imports;
  - [ ] generic parameters;
  - [ ] signature members;
  - [ ] choice variants;
  - [ ] dependency edges.

- [ ] Add `FrontendArenaCapacityEstimate` in:

```text
src/compiler_frontend/arena/capacity.rs
```

- [ ] Seed initial estimates using:
  - [ ] source file count;
  - [ ] source byte count;
  - [ ] token stats;
  - [ ] header stats;
  - [ ] existing fragment counts.

- [ ] Keep formulas conservative with modest over-allocation and hard caps.
- [ ] Add a short comment above every non-obvious formula explaining what it estimates and why.
- [ ] Add initial estimate fields for:
  - [ ] scope frames;
  - [ ] declarations;
  - [ ] expressions;
  - [ ] expression items;
  - [ ] statements;
  - [ ] templates;
  - [ ] template atoms;
  - [ ] render pieces;
  - [ ] HIR blocks/statements/expressions;
  - [ ] borrow facts.

- [ ] Only wire the fields needed by early phases. Future fields may remain unused with a clear TODO and no dead-code suppression unless necessary.
- [ ] Add counters for estimate-vs-actual reporting behind `detailed_timers`:
  - [ ] estimated scope frames;
  - [ ] actual scope frames;
  - [ ] scope arena capacity;
  - [ ] estimate saturation / capped estimates;
  - [ ] future fields as they become used.

### Tests

- [ ] Add unit tests for `TokenStats` classification if tokenization APIs make this cheap.
- [ ] Add unit tests for `HeaderStats` from simple synthetic headers.
- [ ] Add unit tests for `FrontendArenaCapacityEstimate` caps and monotonic behaviour.
- [ ] Add regression tests ensuring stats do not affect diagnostics or output.

### Audit / style guide review / validation

- [ ] Confirm stats collection does not add a new full source/token/header traversal beyond work already being done.
- [ ] Confirm stats/capacity files are separate from pipeline orchestration.
- [ ] Confirm `FrontendArenaCapacityEstimate` is policy-only.
- [ ] Run `just validate`.
- [ ] Run `just bench-frontend-check` and compare to baseline.

### Backtrack criteria

- [ ] If stats collection produces measurable overhead before any arena uses it, simplify the stats set or move costly metrics behind `detailed_timers`.
- [ ] If formulas become noisy in orchestration files, move them back into `capacity.rs`.

---

## Phase 2 — Adversarial Benchmark Fixtures and Generator Support

### Summary

This phase expands benchmark coverage before the major refactors. The goal is to expose unexpected compiler weak spots and produce repeatable high-churn workloads. Fixtures should be valid Beanstalk programs, not diagnostic failures.

### Implementation steps

- [ ] Add a `benchmarks/adversarial/` directory.
- [ ] Add static committed fixtures first:
  - [ ] `one-module-kitchen-sink.bst` — many constructs in one module to stress combined AST/environment/type/template paths.
  - [ ] `deep-scope-churn.bst` — nested functions/control/value blocks/templates to stress scope frame creation and lookup.
  - [ ] `template-render-plan-churn.bst` — nested templates, slots, `$children`, `$markdown`, runtime template control flow.
  - [ ] `constant-dag-churn.bst` — many constants, cross-constant references, folded templates, arithmetic trees.
  - [ ] `expression-rpn-churn.bst` — large expressions, casts, operators, value-producing blocks at valid receiving sites.
  - [ ] `generic-trait-churn.bst` — generic functions/types, trait bounds, conformances, concrete instantiations.
  - [ ] `collection-map-borrow-churn.bst` — collections, maps, mutable access, fallible operations, borrow-valid aliases.
  - [ ] `import-external-churn/` — project fixture with import fanout and repeated external package usage.

- [ ] Keep fixtures representative; do not add many near-duplicates.
- [ ] Add fixtures to `benchmarks/cases.txt` and `benchmarks/frontend-cases.txt` under existing `stress` or `module` groups unless a new group is clearly justified.
- [ ] If generation is useful, add deterministic generator support:
  - [ ] prefer extending `xtask` if it already owns repo tooling;
  - [ ] otherwise place a small documented generator under `benchmarks/generators/`;
  - [ ] commit generated `.bst` fixtures as canonical benchmark inputs;
  - [ ] document exact regeneration command.

- [ ] Update `benchmarks/README.md` fixture list with the new adversarial fixtures.
- [ ] Add notes explaining that adversarial fixtures are for compiler churn discovery, not public language examples.

### Audit / style guide review / validation

- [ ] Ensure all fixtures are valid successful programs/projects.
- [ ] Ensure no generated output folders are committed.
- [ ] Ensure benchmark groups remain readable.
- [ ] Run `just bench-frontend-check`.
- [ ] Run `just bench-check`.
- [ ] Run targeted Samply profile for `one-module-kitchen-sink.bst` after it is added.

### Backtrack criteria

- [ ] Remove or merge fixtures that stress the same compiler path without adding insight.
- [ ] If one fixture is too large/noisy for routine checks, keep it as targeted profiling-only and document that status.

---

## Phase 3 — External Package Registry Clone Reduction

### Summary

The first Samply review showed external package metadata cloning as a surprising hotspot. This phase reduces clone pressure before the deeper AST scope rewrite. It is a targeted quick-win phase and should stay independent from arena work where possible.

### Implementation steps

- [ ] Audit clone sites for:
  - [ ] `ExternalPackageRegistry`;
  - [ ] `ExternalPackage`;
  - [ ] `ExternalFunctionDef`;
  - [ ] `ExternalSymbolPath`;
  - [ ] ABI parameter lists;
  - [ ] builder runtime package metadata.

- [ ] Add or verify counters:
  - [ ] `ExternalPackageRegistryCloneCount`;
  - [ ] `ExternalPackageDefinitionCloneCount`;
  - [ ] `ExternalFunctionDefinitionCloneCount`;
  - [ ] `ExternalSymbolPathCloneCount`;
  - [ ] `ExternalAbiParameterCloneCount`.

- [ ] Review whether `CompilerFrontend::new(...)` can borrow or share immutable external package metadata instead of cloning it.
- [ ] Prefer `Arc`/shared immutable storage only where ownership needs to survive across module/backend boundaries.
- [ ] Avoid lifetime-heavy borrow threading if it makes the pipeline harder to reason about; use a small shared registry wrapper if needed.
- [ ] Preserve backend-facing `Module` semantics:
  - [ ] backend validators still have access to reachable external function metadata;
  - [ ] diagnostics can still resolve external symbol/type names;
  - [ ] module external imports remain reachable-filtered as before.

- [ ] If final `Module` still needs an effective registry, make it cheap to clone by sharing internal immutable tables.
- [ ] Remove obsolete clone-heavy APIs after replacement.
- [ ] Do not add compatibility wrappers.

### Tests

- [ ] Run existing external package/import tests.
- [ ] Add targeted tests only if ownership/API changes create new invariants.
- [ ] Ensure external JS import fixtures still pass.

### Audit / style guide review / validation

- [ ] Confirm immutable external package metadata is not accidentally made mutable through shared ownership.
- [ ] Confirm no backend rediscovery or duplicate metadata path was introduced.
- [ ] Confirm code remains organised under `external_packages/` and pipeline files only pass the new shared handle/context.
- [ ] Run `just validate`.
- [ ] Run `just bench-frontend-check` and targeted external/import benchmarks.
- [ ] Record before/after clone counters and Samply findings.

### Backtrack criteria

- [ ] If shared ownership complicates APIs but clone counters/timings do not improve, revert or narrow the change.
- [ ] If a clone remains necessary at a true ownership boundary, document it rather than contorting APIs.

---

## Phase 4 — ScopeFrame Arena Refactor

### Summary

This is the first major arena refactor. Replace scope-context cloning with a typed `Vec` arena of parent-linked scope frames. This directly exploits Beanstalk’s no-shadowing invariant and should reduce cloned maps/vectors during AST environment, expression, template, and body parsing.

### Target design

```rust
pub struct ScopeArena {
    frames: Vec<ScopeFrame>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ScopeFrameId(u32);

pub struct ScopeFrame {
    pub parent: Option<ScopeFrameId>,
    pub kind: ScopeFrameKind,
    pub local_names: FxHashMap<StringId, DeclarationId>,
    pub local_declarations: Range<DeclarationId>,
    pub expected_result: Option<ExpectedResultId>,
    pub flags: ScopeFrameFlags,
}

pub struct ScopeContext {
    pub shared: ScopeSharedId,
    pub frame: ScopeFrameId,
}
```

Exact field names should follow the current codebase’s naming and existing types. The design principle is stable: frame IDs and parent links replace cloned visible state.

### Implementation steps

- [ ] Move/split scope code so arena-specific pieces are separate:

```text
src/compiler_frontend/ast/module_ast/scope_context/
    mod.rs
    context.rs
    scope_arena.rs
    scope_frame.rs
    lookup.rs
```

Use the current module shape if it differs, but keep arena internals out of high-level AST orchestration files.

- [ ] Add `ScopeArena::with_capacity(estimate.scope_frames)`.
- [ ] Add `ScopeFrameId` with safe index conversion helpers.
- [ ] Add root/module/file/function/template/block frame constructors.
- [ ] Replace child context clone constructors with frame allocation:
  - [ ] child expression context;
  - [ ] child template context;
  - [ ] child function/body context;
  - [ ] child constant context;
  - [ ] block/value-producing context.

- [ ] Replace visible-local cloned maps with parent-chain lookup.
- [ ] Implement redeclaration checks using no-shadowing:
  - [ ] check current frame;
  - [ ] check ancestor frames until file/module boundary as required by current visibility rules;
  - [ ] check shared header-built visibility/import/builtin records.

- [ ] Ensure import visibility remains header-owned and shared.
- [ ] Ensure diagnostics keep correct `SourceLocation` and source labels.
- [ ] Add actual arena counters:
  - [ ] frames allocated;
  - [ ] max frame depth;
  - [ ] local declarations inserted;
  - [ ] lookup ancestor steps;
  - [ ] redeclaration ancestor checks;
  - [ ] old scope-context clone counter, if still present during migration, should trend to zero and then be removed.

- [ ] Delete obsolete scope cloning helpers once callers are migrated.
- [ ] Do not keep old and new scope APIs in parallel.

### Tests

- [ ] Add focused scope-frame unit tests outside production files:
  - [ ] parent lookup;
  - [ ] no-shadowing redeclaration across frames;
  - [ ] same-frame redeclaration;
  - [ ] visibility boundary behaviour;
  - [ ] expected-result propagation where applicable.

- [ ] Run integration tests covering:
  - [ ] local declarations;
  - [ ] nested `if`/loop/catch/value-producing blocks;
  - [ ] templates capturing values;
  - [ ] same-file receiver methods;
  - [ ] imports and aliases;
  - [ ] duplicate declaration diagnostics.

### Audit / style guide review / validation

- [ ] Confirm no user-visible shadowing rule changed.
- [ ] Confirm AST does not rebuild import visibility.
- [ ] Confirm scope arena internals are not embedded in pipeline orchestration files.
- [ ] Confirm stage-local diagnostics still use `CompilerDiagnostic`.
- [ ] Confirm no compatibility wrappers remain.
- [ ] Run `just validate`.
- [ ] Run five repeated frontend benchmark invocations and compare medians.
- [ ] Run targeted Samply profiles for:
  - [ ] `environment-stress.bst`;
  - [ ] `deep-scope-churn.bst`;
  - [ ] `one-module-kitchen-sink.bst`.

### Backtrack criteria

- [ ] If scope-frame lookup increases runtime due to long parent walks, add cached nearest-name indexes or flatten only at safe boundaries, then re-profile.
- [ ] If diagnostics regress, stop and fix diagnostics before further optimisation.
- [ ] If performance regresses more than 5% without a clear offsetting win, revise or revert.

---

## Phase 5 — Capacity Heuristic Tuning For Scope Arenas

### Summary

After scope frames are arena-backed, tune capacity estimates using actual data. This phase should tighten formulas based on `docs`, existing benchmarks, and adversarial fixtures.

### Implementation steps

- [ ] Record estimate-vs-actual data for scope frames across:
  - [ ] docs build;
  - [ ] `environment-stress.bst`;
  - [ ] `template-stress.bst`;
  - [ ] `deep-scope-churn.bst`;
  - [ ] `one-module-kitchen-sink.bst`;
  - [ ] import/module fixtures.

- [ ] Add reported ratios behind detailed timers:
  - [ ] estimated / actual;
  - [ ] final capacity / actual;
  - [ ] capped estimate count.

- [ ] Tune formulas in `capacity.rs` only.
- [ ] Keep modest over-allocation acceptable.
- [ ] Add comments explaining any fixture-specific insight that influenced formulas.
- [ ] Do not add fixture-specific special cases unless they represent a real semantic category.

### Audit / style guide review / validation

- [ ] Confirm formulas remain centralized.
- [ ] Confirm capacity estimates do not affect semantics.
- [ ] Run `just validate`.
- [ ] Run five repeated benchmark invocations.
- [ ] Update `benchmarks/frontend-optimization-results.md` with estimate-vs-actual summaries.

### Backtrack criteria

- [ ] If tuned formulas overfit adversarial fixtures and hurt real docs/benchmark performance, prefer simpler formulas.
- [ ] If capacity tuning has no measurable effect, keep only the simple low-risk formulas.

---

## Phase 6 — Expression Scratch / Expression Item Arena Pilot

### Summary

This phase is gated by evidence. Implement only if profiles/counters still show expression RPN/order/fold allocation pressure after scope and external package clone work.

Start with scratch-buffer reuse and typed `Vec` arenas. Do not convert the entire expression AST to borrowed arena references.

### Entry criteria

- [ ] Samply or counters show meaningful expression allocation/clone pressure.
- [ ] `expression-rpn-churn.bst` or real docs/build profiles show expression paths as top movers.
- [ ] Phase 4 and 5 are complete and stable.

### Implementation steps

- [ ] Add expression scratch storage in a focused file:

```text
src/compiler_frontend/ast/expressions/scratch.rs
```

- [ ] Add `ExpressionScratch` with reusable vectors:
  - [ ] RPN items;
  - [ ] operator stack;
  - [ ] fold stack;
  - [ ] temporary argument lists where appropriate.

- [ ] Seed capacities from `FrontendArenaCapacityEstimate`.
- [ ] Thread scratch through expression parser/evaluator context structs, not long parameter lists.
- [ ] Avoid global mutable scratch.
- [ ] Add counters:
  - [ ] expression scratch clears;
  - [ ] expression item pushes;
  - [ ] expression item realloc/growth events if practical;
  - [ ] expression clone count where practical.

- [ ] If scratch reuse is insufficient, consider `ExpressionArena` with `ExpressionId` as a second step in this phase.
- [ ] Keep `ExpressionId` storage internal to AST until HIR lowering is updated.

### Audit / style guide review / validation

- [ ] Confirm expression diagnostics and source locations are unchanged.
- [ ] Confirm constant folding output is unchanged.
- [ ] Confirm no expression scratch leaks across files/modules.
- [ ] Run `just validate`.
- [ ] Run targeted expression benchmark/profile.

### Backtrack criteria

- [ ] If scratch threading makes parser APIs harder to reason about without measurable improvement, revert to simpler local vectors.
- [ ] If expression arena conversion causes broad churn, stop after scratch-buffer reuse and defer full conversion.

---

## Phase 7 — Template / Render-Plan Arena Pilot

### Summary

This phase is gated by evidence. Templates are central to Beanstalk, so template arena changes must be cautious and heavily validated. The goal is to reduce cloning of template atoms, render pieces, and composed/finalized templates.

### Entry criteria

- [ ] Samply or counters show template/render-plan clone/allocation pressure after earlier phases.
- [ ] `template-stress.bst`, `template-render-plan-churn.bst`, or docs profiles show template paths as top movers.
- [ ] Existing template semantics and goldens are stable.

### Implementation steps

- [ ] Add focused arena files:

```text
src/compiler_frontend/ast/templates/arena.rs
src/compiler_frontend/ast/templates/render_plan_arena.rs
```

- [ ] Add stable IDs/ranges for:
  - [ ] template atoms;
  - [ ] render pieces;
  - [ ] slot contribution lists;
  - [ ] child template lists.

- [ ] Keep the state model explicit:

```text
ParsedTemplate -> ComposedTemplate -> FinalizedTemplate
```

or equivalent current-project names.

- [ ] Avoid re-walking finalized templates except where required for HIR lowering or diagnostics.
- [ ] Replace obvious `to_vec()` / clone-for-composition paths only after tests show equivalent output.
- [ ] Add counters:
  - [ ] template atoms allocated;
  - [ ] render pieces allocated;
  - [ ] template clone count;
  - [ ] render-plan clone count;
  - [ ] finalized-template reuse count.

### Tests

- [ ] Run all template integration tests.
- [ ] Add targeted cases if not already covered:
  - [ ] nested `$children`;
  - [ ] `$fresh`;
  - [ ] named/positional/default slots;
  - [ ] runtime template `if`/`loop`;
  - [ ] const template folding;
  - [ ] markdown formatting;
  - [ ] reactive subscriptions.

### Audit / style guide review / validation

- [ ] Confirm compile-time and runtime template output is unchanged.
- [ ] Confirm builder-facing const fragments are unchanged.
- [ ] Confirm HIR still receives finalized runtime template plans only.
- [ ] Run `just validate`.
- [ ] Run targeted template profiles and benchmark medians.

### Backtrack criteria

- [ ] If template arena conversion introduces semantic fragility, revert and retain only measured low-risk clone reductions.
- [ ] If output equivalence is hard to prove, pause and add stronger goldens/tests before continuing.

---

## Phase 8 — HIR Dense Storage / Borrow Fact Compaction Investigation

### Summary

This is deliberately later. The original profile did not show HIR, borrow checking, or JS lowering as the primary bottleneck. Do not start here unless earlier phases shift the bottleneck or targeted fixtures expose HIR/borrow pressure.

### Entry criteria

- [ ] Benchmarks show HIR or borrow validation as a meaningful remaining stage mover.
- [ ] Samply identifies dense ID hash maps, borrow snapshots, or fact storage as hot.
- [ ] AST/scope/template/external clone work is complete or intentionally deferred.

### Investigation steps

- [ ] Audit HIR ID allocation and ID-to-index maps.
- [ ] Replace dense compiler-owned ID hash maps with `Vec<Option<T>>` / dense maps only where IDs are actually dense and stable.
- [ ] Investigate borrow fact side tables keyed by HIR IDs.
- [ ] Investigate snapshot storage size and diagnostic-only snapshot detail.
- [ ] Add counters for dense map hits/misses and borrow fact storage volume.

### Audit / style guide review / validation

- [ ] Confirm HIR validation still catches transformation invariants.
- [ ] Confirm borrow facts remain side tables and do not mutate HIR.
- [ ] Confirm GC-only backend semantics are unchanged.
- [ ] Run `just validate`.
- [ ] Run borrow stress benchmarks and profiles.

### Backtrack criteria

- [ ] If HIR/borrow changes do not improve stage timing or memory pressure, defer them.
- [ ] If fact compaction hurts diagnostic quality, preserve diagnostic fidelity and defer compaction.

---

## Phase 9 — Roadmap, Progress Matrix, and Documentation Updates

### Summary

This phase records the optimisation programme in project planning docs and documents what is intentionally deferred. Do this once the initial implementation direction is stable, and update it again after major phase outcomes.

### Roadmap/progress updates

- [ ] Add a roadmap/progress matrix item named:

```text
Frontend Arena + Semantic Invariant Optimisation
```

- [ ] Status suggestion:
  - [ ] `Planned` before implementation starts;
  - [ ] `In Progress` once baseline/instrumentation and first arena work begins;
  - [ ] `Partial` or equivalent when scope arenas and capacity heuristics land;
  - [ ] keep deeper arena migrations separate/deferred unless implemented.

- [ ] Describe scope:
  - [ ] typed `Vec` arenas with stable IDs;
  - [ ] capacity heuristics from token/header stats;
  - [ ] scope-frame arena refactor;
  - [ ] clone reduction in external package metadata;
  - [ ] benchmark/adversarial fixture expansion;
  - [ ] evidence-based rollback rules.

### Deliberately deferred items

Add these as deferred until profiling justifies them:

- [ ] `bumpalo`/dropless bump allocation.
- [ ] Full AST node arena conversion.
- [ ] Full expression arena conversion beyond scratch-buffer reuse.
- [ ] Full template/render-plan arena migration.
- [ ] HIR arena conversion.
- [ ] Borrow fact compaction and snapshot reduction.
- [ ] Source-library HIR caching.
- [ ] Incremental compiler caching.
- [ ] Whole-project persistent semantic cache.

### Documentation updates

- [ ] Update `docs/compiler-design-overview.md` only after implementation changes are real.
- [ ] Add a short section explaining:
  - [ ] frontend arenas are stage/module-owned implementation details;
  - [ ] capacity heuristics are policy-only;
  - [ ] `StringTable` remains the path/string identity system;
  - [ ] AST/HIR stage ownership remains unchanged.

- [ ] Update `benchmarks/README.md`:
  - [ ] list new adversarial fixtures;
  - [ ] explain the optimisation campaign’s five-invocation median protocol;
  - [ ] reaffirm raw local data/profiles are not committed.

- [ ] Keep `benchmarks/frontend-optimization-results.md` current:
  - [ ] baseline;
  - [ ] per-phase result summaries;
  - [ ] regressions and reversions;
  - [ ] heuristic tuning notes;
  - [ ] final decisions.

### Audit / style guide review / validation

- [ ] Confirm docs do not promise deferred work as implemented.
- [ ] Confirm progress matrix wording is concise and editor-friendly.
- [ ] Confirm no stale design notes conflict with the new arena direction.
- [ ] Run doc/build validation if the docs site is affected.
- [ ] Run `just validate`.

---

## Suggested Phase Order For Coding Agents

Use these as independent implementation chunks. Do not merge phases unless the change is trivial.

1. [ ] Phase 0: Baseline and report file.
2. [ ] Phase 1: Stats and capacity estimate framework.
3. [ ] Phase 2: Adversarial benchmark fixtures.
4. [ ] Phase 3: External package registry clone reduction.
5. [ ] Phase 4: Scope-frame arena refactor.
6. [ ] Phase 5: Scope arena capacity tuning.
7. [ ] Phase 6: Expression scratch/arena pilot, only if evidence supports it.
8. [ ] Phase 7: Template/render-plan arena pilot, only if evidence supports it.
9. [ ] Phase 8: HIR/borrow dense storage investigation, only if evidence supports it.
10. [ ] Phase 9: Roadmap/progress/docs finalization.

---

## Final Completion Criteria

The initial optimisation programme is complete when:

- [ ] `TokenStats`, `HeaderStats`, and `FrontendArenaCapacityEstimate` exist and are used by the first arena implementation.
- [ ] Scope context cloning has been replaced by parent-linked scope frames in a typed `Vec` arena.
- [ ] External package registry clone pressure has been reduced or documented as unavoidable at true ownership boundaries.
- [ ] Benchmark/adversarial fixtures exist and are documented.
- [ ] `benchmarks/frontend-optimization-results.md` records baseline and final phase summaries.
- [ ] Roadmap/progress matrix explicitly tracks the optimisation work and deferred deeper arena features.
- [ ] `just validate` passes.
- [ ] Five-run median benchmarks show no unjustified regressions.
- [ ] Samply profiles show the targeted clone/allocation hotspot was reduced or moved.
- [ ] Core pipeline files remain readable and are not bloated with arena/capacity helper internals.

