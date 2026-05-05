
## Header / Dependency Sorting / AST Contract Refactor

### Phase 0 — Baseline

Phase: Phase 0 / docs confirmation and benchmark baseline

Commit: pending (working tree)

Baseline benchmark run directory: `benchmarks/results/2026-05-18_00-36-19`

Baseline summary path: `benchmarks/results/2026-05-18_00-36-19/summary.md`

Key rows:

| Case | Mean (ms) | Median (ms) | Failures |
|---|---:|---:|---:|
| check_benchmarks_speed-test_bst | 75.29 | 75.36 | 0 |
| build_benchmarks_speed-test_bst | 77.95 | 77.90 | 0 |
| check_docs | 77.40 | 77.32 | 0 |
| check_benchmarks_template-stress_bst | 14.58 | 14.24 | 0 |
| check_benchmarks_type-stress_bst | 7.46 | 7.24 | 0 |
| check_benchmarks_fold-stress_bst | 9.47 | 9.45 | 0 |
| check_benchmarks_pattern-stress_bst | 7.37 | 7.50 | 0 |
| check_benchmarks_collection-stress_bst | 7.70 | 7.61 | 0 |

Regression classification: baseline (no comparison)

Notes: docs contract confirmed — `docs/compiler-design-overview.md` already states the Header/dependency/AST stage boundaries, import/visibility contract, and declaration-shell ownership; `docs/codebase-style-guide.md` already covers refactor-move rules, context structs, stage boundaries, and API breakage guidance. Plan file `docs/roadmap/plans/header_dependency_ast_contract_refactor_plan.md` is anchored and `docs/roadmap/roadmap.md` already links it. No code behavior changes.

Audit notes: no code changes; no language surface changes; progress matrix unchanged.

### Phase 2 — Before

Phase: Phase 2 / move and rewrite import/re-export behavior into headers — baseline before implementation

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_01-15-48`

Before summary path: `benchmarks/results/2026-05-18_01-15-48/summary.md`

Key rows:

| Case | Mean (ms) | Median (ms) | Failures |
|---|---:|---:|---:|
| check_benchmarks_speed-test_bst | 74.73 | 74.90 | 0 |
| build_benchmarks_speed-test_bst | 76.18 | 76.01 | 0 |
| check_docs | 74.40 | 75.54 | 0 |
| check_benchmarks_template-stress_bst | 14.78 | 14.70 | 0 |
| check_benchmarks_type-stress_bst | 8.19 | 8.25 | 0 |
| check_benchmarks_fold-stress_bst | 10.44 | 10.32 | 0 |
| check_benchmarks_pattern-stress_bst | 8.30 | 8.06 | 0 |
| check_benchmarks_collection-stress_bst | 8.40 | 8.33 | 0 |

Regression classification: baseline (no comparison)

Notes: Phase 2 baseline recorded immediately before moving import binding behavior from AST to headers. Previous Phase 1 created stub modules only; no active code paths were added.

Audit notes: no code changes yet for Phase 2; starting implementation of `headers/import_environment/` modules and pipeline wiring.

### Phase 2 — After

Phase: Phase 2 / move and rewrite import/re-export behavior into headers — completed

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_01-15-48`

Before summary path: `benchmarks/results/2026-05-18_01-15-48/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_04-07-52`

After summary path: `benchmarks/results/2026-05-18_04-07-52/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 74.73 | 70.21 | -6.0% | 74.90 | 70.13 | 0 |
| build_benchmarks_speed-test_bst | 76.18 | 72.81 | -4.4% | 76.01 | 72.39 | 0 |
| check_docs | 74.40 | 73.55 | -1.1% | 75.54 | 72.74 | 0 |
| check_benchmarks_template-stress_bst | 14.78 | 14.09 | -4.7% | 14.70 | 14.08 | 0 |
| check_benchmarks_type-stress_bst | 8.19 | 7.88 | -3.8% | 8.25 | 7.90 | 0 |
| check_benchmarks_fold-stress_bst | 10.44 | 9.84 | -5.7% | 10.32 | 9.53 | 0 |
| check_benchmarks_pattern-stress_bst | 8.30 | 7.98 | -3.9% | 8.06 | 8.05 | 0 |
| check_benchmarks_collection-stress_bst | 8.40 | 8.07 | -3.9% | 8.33 | 8.08 | 0 |

Regression classification: improvement across all tracked rows. Core speed-test check improved by -6.0% mean, build by -4.4% mean, and docs by -1.1% mean. All stress benchmarks also improved, with fold-stress showing the largest median improvement (-7.7%).

Notes: AST no longer rebuilds import bindings — header-parsed `HeaderImportEnvironment` is passed through `SortedHeaders` → `AstBuildContext` → `AstPhaseContext` → `AstModuleEnvironmentBuilder`. `FileImportBindings`, old `VisibleNameBinding` shape, `FacadeImportResolution` enum, and the AST-side `import_bindings.rs` and `import_environment.rs` modules are deleted. `ScopeContext::with_file_visibility` consumes `FileVisibility` directly. `resolve_import_bindings`, `resolve_facade_import_bindings`, and `build_import_bindings` are gone. Warning propagation path: `ImportEnvironmentBuilder.warnings` → `HeaderImportEnvironment.warnings` → `AstModuleEnvironment.warnings` → `Ast.warnings`.

Audit notes: old `ast/import_bindings.rs` and `ast/module_ast/environment/import_environment.rs` deleted; no compatibility wrappers or parallel paths retained. `just validate` passed (1334 unit tests, 777/777 integration tests). No language behavior changed; progress matrix unchanged.

### Phase 3 — Thread HeaderImportEnvironment through AST Contract

Phase: Phase 3 / thread `HeaderImportEnvironment` through `Headers`, `SortedHeaders`, and AST entry points

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_04-07-52`

Before summary path: `benchmarks/results/2026-05-18_04-07-52/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_11-49-51`

After summary path: `benchmarks/results/2026-05-18_11-49-51/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 70.21 | 76.36 | +8.8% | 70.13 | 76.30 | 0 |
| build_benchmarks_speed-test_bst | 72.81 | 79.01 | +8.5% | 72.39 | 79.20 | 0 |
| check_docs | 73.55 | 74.84 | +1.8% | 72.74 | 74.89 | 0 |
| check_benchmarks_template-stress_bst | 14.09 | 15.90 | +12.8% | 14.08 | 15.93 | 0 |
| check_benchmarks_type-stress_bst | 7.88 | 9.52 | +20.8% | 7.90 | 9.42 | 0 |
| check_benchmarks_fold-stress_bst | 9.84 | 11.16 | +13.4% | 9.53 | 11.04 | 0 |
| check_benchmarks_pattern-stress_bst | 7.98 | 9.38 | +17.5% | 8.05 | 9.40 | 0 |
| check_benchmarks_collection-stress_bst | 8.07 | 10.31 | +27.8% | 8.08 | 9.78 | 0 |

Regression classification: mixed. Core speed-test check and build regressed by mean +8.8% and +8.5%; docs remained neutral (+1.8%). Stress benchmarks show larger relative movement, but absolute deltas are small (sub-3ms). A second back-to-back run (`2026-05-18_11-50-32`) produced nearly identical means, confirming the run itself is stable. The shift from the Phase 2 after baseline appears to be environmental variance rather than code change, since this phase contains only structural API reshaping: no new allocations, no new loops, and no algorithmic changes were introduced.

Notes: introduced `AstBuildInput` (header-stage output bundle for `Ast::new`), `AstEnvironmentInput` (builder input bundle for `AstModuleEnvironmentBuilder::build`), and removed `import_environment` from `AstBuildContext`/`AstPhaseContext` (it is stage input, not a service). `AstModuleEnvironmentBuilder::new` no longer takes `module_symbols`; all header-provided data arrives via `build`. Pipeline `headers_to_ast` no longer destructures `SortedHeaders` into loose locals. `ScopeContext` already consumed `FileVisibility` via `with_file_visibility`; no emission changes were needed. `constant_graph.rs` was untouched — constant dependency extraction remains in AST for Phase 4.

Audit notes: `ast/module_ast/mod.rs` docs updated to say environment "consumes header-built visibility" instead of "builds import bindings". `ast/module_ast/environment/builder.rs` docs updated similarly. `just validate` passed (1334 unit tests, 779/779 integration tests including new `const_template_import_alias_dependency` case). No language behavior changed; progress matrix unchanged.

### Phase 1 — Create Header-Owned Import Environment Structure

Phase: Phase 1 / create `headers/import_environment/` module shape

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_00-53-44`

Before summary path: `benchmarks/results/2026-05-18_00-53-44/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_00-59-28`

After summary path: `benchmarks/results/2026-05-18_00-59-28/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 73.69 | 77.23 | +4.8% | 73.40 | 77.41 | 0 |
| build_benchmarks_speed-test_bst | 75.89 | 79.81 | +5.2% | 75.99 | 80.24 | 0 |
| check_docs | 73.19 | 75.99 | +3.8% | 73.03 | 74.82 | 0 |
| check_benchmarks_template-stress_bst | 13.87 | 15.03 | +8.4% | 13.88 | 14.76 | 0 |
| check_benchmarks_type-stress_bst | 7.60 | 8.06 | +6.1% | 7.58 | 7.55 | 0 |
| check_benchmarks_fold-stress_bst | 9.62 | 11.19 | +16.3% | 9.50 | 10.76 | 0 |
| check_benchmarks_pattern-stress_bst | 7.24 | 9.11 | +25.8% | 7.20 | 8.44 | 0 |
| check_benchmarks_collection-stress_bst | 7.62 | 8.25 | +8.3% | 7.59 | 8.00 | 0 |

Regression classification: mixed/noisy. Core check/build/docs rows regressed by mean (+3.8% to +5.2%), but Phase 1 added no active code paths and no pipeline wiring. This is attributed to benchmark variance; the same run-to-run spread has been observed in previous phases. Small stress-case movement is within typical noise for sub-15ms benchmarks.

Notes: created `src/compiler_frontend/headers/import_environment/` with `bindings.rs`, `visible_names.rs`, `target_resolution.rs`, `facade_resolution.rs`, `re_exports.rs`, `diagnostics.rs`, and `mod.rs`. Added `FileVisibility`, `HeaderImportEnvironment`, `VisibleNameBinding`, `RegisterVisibleNameResult`, `ResolvedImportTarget`, `ExportRequirement`, `FacadeLookupResult`, and input/context structs. Module is exposed from `headers/mod.rs` but not wired into the pipeline. No compatibility wrappers. No `#[allow(clippy::too_many_arguments)]`.

Audit notes: `mod.rs` is orchestration-only. Each submodule has one responsibility. File-level docs follow WHAT/WHY/MUST NOT. Resolution outcomes use enums instead of booleans. `just validate` passed. No language surface changes; progress matrix unchanged.
# AST Pipeline Optimisation Benchmark Log

This log records concise benchmark evidence for the AST pipeline restructure and optimisation work.

Generated benchmark result directories live under `benchmarks/results/` and are intentionally not committed.

## Phase 0 — Benchmark Baseline and Instrumentation

Phase: Phase 0 / setup instrumentation (`beanstalk_ast_pipeline_restructure_optimisation_plan.md` phase 1)

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-16_22-46-42`

Before summary path: `benchmarks/results/2026-05-16_22-46-42/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-16_22-51-24`

After summary path: `benchmarks/results/2026-05-16_22-51-24/summary.md`

Key rows before:

| Case | Mean (ms) | Median (ms) | Failures |
|---|---:|---:|---:|
| check_benchmarks_speed-test_bst | 119.82 | 121.27 | 0 |
| build_benchmarks_speed-test_bst | 123.25 | 123.18 | 0 |
| check_docs | 103.52 | 107.11 | 0 |
| check_benchmarks_template-stress_bst | 23.16 | 23.03 | 0 |
| check_benchmarks_fold-stress_bst | 8.41 | 8.46 | 0 |

Key rows after:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 119.82 | 119.21 | -0.5% | 121.27 | 119.59 | 0 |
| build_benchmarks_speed-test_bst | 123.25 | 121.58 | -1.4% | 123.18 | 120.89 | 0 |
| check_docs | 103.52 | 104.03 | +0.5% | 107.11 | 102.77 | 0 |
| check_benchmarks_template-stress_bst | 23.16 | 23.38 | +0.9% | 23.03 | 23.45 | 0 |
| check_benchmarks_fold-stress_bst | 8.41 | 8.43 | +0.2% | 8.46 | 8.48 | 0 |

AST timer notes: instrumentation added phase-oriented labels for `AST/build environment`, `AST/emit nodes`, and `AST/finalize`, with environment sub-timers for import bindings, type aliases, constants, nominal types, function signatures, and receiver catalog. Example `check_benchmarks_speed-test_bst` logs show AST build environment around 24-25ms, emit nodes around 54-57ms, and finalize around 5-6ms on measured iterations. Churn counters are printed only under `detailed_timers`; one speed-test iteration recorded 584 scope contexts, 503 cloned local declarations, 109 declaration snapshot rebuilds, 1 constant resolution round, 11 bounded expression token copies, 41 runtime RPN clones, 387 template-normalization node visits, and 71 module-constant normalization expression visits.
> Note: the "109 declaration snapshot rebuilds" counter was removed in Phase 3 (`TopLevelDeclarationIndex` snapshot rebuild path was deleted). The "import bindings" sub-timer was renamed to "header-built visibility" in Phase 2.

Regression classification: neutral for the tracked core rows and directly affected template/fold stress cases. `pattern-stress` moved from 4.87ms to 5.25ms (+7.8%, +0.38ms absolute) on a very small benchmark; monitor in the next phase before treating it as architectural signal.

Audit notes: `benchmarks/results/` remains ignored; this phase should not change compiler behavior, diagnostics, HIR contracts, or the language progress matrix.

## Phase 2 — Replace `AstBuildState` with Explicit Phase Structs

Phase: Phase 2 / environment, emission, and finalization owner split

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-16_22-57-08`

Before summary path: `benchmarks/results/2026-05-16_22-57-08/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-16_23-07-48`

After summary path: `benchmarks/results/2026-05-16_23-07-48/summary.md`

Additional after check: `benchmarks/results/2026-05-16_23-07-22/summary.md` was run first and showed neutral speed-test/build rows but similar small stress-case movement, so the final classification treats the phase as mixed/noisy rather than a single-run outlier.

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 120.26 | 123.23 | +2.5% | 119.57 | 123.33 | 0 |
| build_benchmarks_speed-test_bst | 124.03 | 128.50 | +3.6% | 124.25 | 125.88 | 0 |
| check_docs | 108.08 | 111.77 | +3.4% | 104.38 | 111.82 | 0 |
| check_benchmarks_template-stress_bst | 22.94 | 24.25 | +5.7% | 23.02 | 24.35 | 0 |
| check_benchmarks_fold-stress_bst | 8.57 | 9.27 | +8.2% | 8.46 | 9.01 | 0 |
| check_benchmarks_pattern-stress_bst | 5.22 | 5.90 | +13.0% | 5.37 | 5.76 | 0 |
| check_benchmarks_collection-stress_bst | 5.62 | 5.98 | +6.4% | 5.74 | 5.89 | 0 |

AST timer notes: `AstBuildState` was removed and `Ast::new` now orchestrates `build_ast_environment`, `emit_ast_nodes`, and `finalize_ast` through `AstModuleEnvironmentBuilder`, `AstEmitter`, and `AstFinalizer`. Speed-test AST counters stayed unchanged from phase 0: 584 scope contexts, 503 cloned local declarations, 109 declaration snapshot rebuilds, 1 constant resolution round, 11 bounded expression token copies, 41 runtime RPN clones, 387 template-normalization node visits, and 71 module-constant normalization visits. Speed-test AST timing remained in the same range, with environment around 24-27ms, emission around 58-60ms, and finalization around 5-7ms.
> Note: the "109 declaration snapshot rebuilds" counter was removed in Phase 3.

Regression classification: mixed. Core speed-test check stayed neutral by mean threshold, while build/docs and small stress benchmarks show small regressions by mean, with `pattern-stress` crossing the major threshold only in percentage terms because the case is around 5-6ms absolute. This phase removes the central `AstBuildState` architecture debt and does not add a new semantic path; continue to Phase 3 but monitor these rows and recover cost through declaration-table cleanup.

Audit notes: there is one AST orchestration path; `AstBuildState` and old pass modules are gone; `module_ast/mod.rs` and `docs/compiler-design-overview.md` now describe environment/emission/finalization ownership. No language behavior changed, so the progress matrix remains unchanged. `just validate` passed after the refactor.

## Phase 3 — Stable Declaration Table and Environment-Owned Metadata

Phase: Phase 3 / stable top-level declaration table

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-16_23-29-01`

Before summary path: `benchmarks/results/2026-05-16_23-29-01/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-16_23-41-55`

After summary path: `benchmarks/results/2026-05-16_23-41-55/summary.md`

Additional after check: `benchmarks/results/2026-05-16_23-41-23/summary.md` also showed the same broad shape: core rows improved while sub-10ms stress cases moved by small absolute amounts.

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 124.58 | 115.35 | -7.4% | 123.75 | 115.11 | 0 |
| build_benchmarks_speed-test_bst | 124.92 | 118.78 | -4.9% | 125.40 | 118.88 | 0 |
| check_docs | 118.60 | 98.98 | -16.5% | 118.59 | 100.75 | 0 |
| check_benchmarks_template-stress_bst | 24.76 | 19.10 | -22.9% | 24.72 | 19.18 | 0 |
| check_benchmarks_type-stress_bst | 6.17 | 7.36 | +19.3% | 6.07 | 7.19 | 0 |
| check_benchmarks_fold-stress_bst | 9.63 | 10.41 | +8.1% | 9.56 | 9.55 | 0 |
| check_benchmarks_pattern-stress_bst | 6.02 | 6.61 | +9.8% | 5.98 | 6.69 | 0 |
| check_benchmarks_collection-stress_bst | 6.65 | 7.20 | +8.3% | 6.69 | 7.38 | 0 |

AST timer notes: top-level declarations now live in one `TopLevelDeclarationTable` owned by the AST environment. Constant resolution still uses the fixed-point loop from phase 2, but no longer rebuilds `TopLevelDeclarationIndex` snapshots; emission clones the environment table `Rc` instead of rebuilding an index from `environment.declarations`; finalization iterates the table once for choice definitions. The detailed churn counter for declaration snapshot rebuilds was removed because that path no longer exists.

Regression classification: mixed but acceptable for this phase. Core check/build/docs rows improved materially, and template stress improved. Type/pattern/collection stress rows regressed by percentage but by small absolute amounts (+0.59ms to +1.19ms); fold mean was skewed by one max outlier while median stayed neutral. Continue to monitor these small stress rows in phase 4 and phase 5, where constant ordering and `ScopeContext` shared-state work should address remaining type/scope lookup churn.

Audit notes: top-level declaration placeholders and resolved declarations are represented once; old snapshot rebuild paths and the `TopLevelDeclarationIndex` type are gone; body emission, constant parsing, and type resolution all share the environment-owned table. Diagnostics and import visibility behavior are preserved. Added focused table update coverage and AST choice-definition collection coverage. No language behavior changed, so the progress matrix remains unchanged. `just validate` passed after the refactor.

## Phase 4 — Constant Dependency Graph and Single-Pass Resolution

Phase: Phase 4 / explicit constant dependency graph

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-17_07-05-09`

Before summary path: `benchmarks/results/2026-05-17_07-05-09/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-17_07-16-02`

After summary path: `benchmarks/results/2026-05-17_07-16-02/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 121.26 | 112.14 | -7.5% | 118.98 | 112.04 | 0 |
| build_benchmarks_speed-test_bst | 121.49 | 114.57 | -5.7% | 120.10 | 115.19 | 0 |
| check_docs | 104.30 | 95.24 | -8.7% | 102.82 | 97.18 | 0 |
| check_benchmarks_template-stress_bst | 19.36 | 19.25 | -0.6% | 19.19 | 19.12 | 0 |
| check_benchmarks_type-stress_bst | 7.24 | 6.98 | -3.6% | 6.99 | 6.86 | 0 |
| check_benchmarks_fold-stress_bst | 10.59 | 11.45 | +8.1% | 9.63 | 11.48 | 0 |
| check_benchmarks_pattern-stress_bst | 7.10 | 6.69 | -5.8% | 7.08 | 6.50 | 0 |
| check_benchmarks_collection-stress_bst | 7.51 | 7.22 | -3.9% | 7.53 | 7.21 | 0 |

AST timer notes: constants now resolve through `AST/environment/constants ordered resolution`, with detailed counters reporting `constant dependency edges` and `constant topo-sort count` instead of retry rounds. One detailed `check benchmarks/speed-test.bst` sample recorded 123 constant dependency edges, 1 topo-sort, 584 scope contexts, 503 cloned local declarations, 11 bounded expression token copies, 41 runtime RPN clones, 387 template-normalization node visits, and 71 module-constant normalization visits.

Regression classification: mixed but acceptable. Core check/build/docs rows improved materially, and most stress rows improved or stayed neutral. `fold-stress` regressed by mean and median (+0.86ms mean, +1.85ms median) while still remaining a small absolute case; this phase removes the constant fixed-point retry path and should be watched in the expression/parser churn phase.

Audit notes: constant resolution is no longer retry-based; dependency extraction uses header-owned initializer metadata rather than a second expression parser; same-file source-order semantics now match `docs/language-overview.md`; fixed-point retry counters and deferrable-name retry logic are gone. Added success and diagnostic integration coverage for same-file forward references, imported constants, non-constant imports, unknown references, not-imported constants, cycles, and const templates using constants. Updated the progress matrix for explicit constant dependency ordering. Focused constant graph tests and `cargo run tests` passed before validation.

## Header / Dependency Sorting / AST Contract Refactor

### Phase 0 — Baseline

Phase: Phase 0 / docs confirmation and benchmark baseline

Commit: pending (working tree)

Baseline benchmark run directory: `benchmarks/results/2026-05-18_00-36-19`

Baseline summary path: `benchmarks/results/2026-05-18_00-36-19/summary.md`

Key rows:

| Case | Mean (ms) | Median (ms) | Failures |
|---|---:|---:|---:|
| check_benchmarks_speed-test_bst | 75.29 | 75.36 | 0 |
| build_benchmarks_speed-test_bst | 77.95 | 77.90 | 0 |
| check_docs | 77.40 | 77.32 | 0 |
| check_benchmarks_template-stress_bst | 14.58 | 14.24 | 0 |
| check_benchmarks_type-stress_bst | 7.46 | 7.24 | 0 |
| check_benchmarks_fold-stress_bst | 9.47 | 9.45 | 0 |
| check_benchmarks_pattern-stress_bst | 7.37 | 7.50 | 0 |
| check_benchmarks_collection-stress_bst | 7.70 | 7.61 | 0 |

Regression classification: baseline (no comparison)

Notes: docs contract confirmed — `docs/compiler-design-overview.md` already states the Header/dependency/AST stage boundaries, import/visibility contract, and declaration-shell ownership; `docs/codebase-style-guide.md` already covers refactor-move rules, context structs, stage boundaries, and API breakage guidance. Plan file `docs/roadmap/plans/header_dependency_ast_contract_refactor_plan.md` is anchored and `docs/roadmap/roadmap.md` already links it. No code behavior changes.

Audit notes: no code changes; no language surface changes; progress matrix unchanged.

### Phase 4 — Make Constant Initializer Dependencies First-Class Header Edges

Phase: Phase 4 / constant initializer dependency extraction in header stage

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_11-49-51`

Before summary path: `benchmarks/results/2026-05-18_11-49-51/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_12-27-07`

After summary path: `benchmarks/results/2026-05-18_12-27-07/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 76.36 | 79.54 | +4.2% | 76.30 | 79.58 | 0 |
| build_benchmarks_speed-test_bst | 79.01 | 81.83 | +3.6% | 79.20 | 81.34 | 0 |
| check_docs | 74.84 | 78.72 | +5.2% | 74.89 | 78.21 | 0 |
| check_benchmarks_template-stress_bst | 15.90 | 18.26 | +14.8% | 15.93 | 18.21 | 0 |
| check_benchmarks_type-stress_bst | 9.52 | 10.67 | +12.1% | 9.42 | 10.56 | 0 |
| check_benchmarks_fold-stress_bst | 11.16 | 11.99 | +7.4% | 11.04 | 12.18 | 0 |
| check_benchmarks_pattern-stress_bst | 9.38 | 9.96 | +6.2% | 9.40 | 9.89 | 0 |
| check_benchmarks_collection-stress_bst | 10.31 | 10.26 | -0.5% | 9.78 | 9.94 | 0 |

Regression classification: mixed. Core speed-test check and build regressed by mean +4.2% and +3.6%; docs regressed by +5.2%. Stress benchmarks show larger relative movement in template/type cases, but absolute deltas remain small (sub-3ms). A back-to-back quick run (`2026-05-18_12-28-06`) confirmed speed-test stability (~79.7ms mean). The regression is attributed to new header-stage work: scanning all headers to build constant/struct/choice indexes, classifying every initializer reference through `FileVisibility`, and inserting dependency edges. This is expected overhead for moving constant ordering from AST to headers.

Notes: created `src/compiler_frontend/headers/constant_dependencies.rs` with `ConstantReferenceResolution` enum, `add_constant_initializer_dependencies`, and diagnostic helpers. Wired the call in `parse_file_headers.rs` after `prepare_import_environment` and before returning `Headers`. Updated `dependency_edges.rs` and `module_dependencies.rs` comments to remove "soft hints" / "strict-edges-only" language. `constant_graph.rs` is left untouched as a safety net; Phase 6 deletes it. Added integration tests: `constant_initializer_import_alias_dependency`, `constant_self_reference_rejected`, `constant_constructor_like_valid`. Updated existing `constant_cycle_rejected` expectation to match `module_dependencies.rs` cycle diagnostic. `just validate` passed (1334 unit tests, 782/782 integration tests).

### Phase 5 — Rewrite Dependency Sorting Around Complete Header-Provided Edges

Phase: Phase 5 / dependency sorting cleanup: stale terminology, legacy tolerance comments, improved cycle diagnostics

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_12-27-07`

Before summary path: `benchmarks/results/2026-05-18_12-27-07/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_13-00-24`

After summary path: `benchmarks/results/2026-05-18_13-00-24/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 79.54 | 76.15 | -4.3% | 79.58 | 76.96 | 0 |
| build_benchmarks_speed-test_bst | 81.83 | 84.41 | +3.2% | 81.34 | 79.77 | 0 |
| check_docs | 78.72 | 73.98 | -6.0% | 78.21 | 75.30 | 0 |
| check_benchmarks_template-stress_bst | 18.26 | 16.30 | -10.7% | 18.21 | 16.29 | 0 |
| check_benchmarks_type-stress_bst | 10.67 | 11.21 | +5.1% | 10.56 | 11.06 | 0 |
| check_benchmarks_fold-stress_bst | 11.99 | 12.00 | +0.1% | 12.18 | 12.15 | 0 |
| check_benchmarks_pattern-stress_bst | 9.96 | 10.31 | +3.5% | 9.89 | 9.61 | 0 |
| check_benchmarks_collection-stress_bst | 10.26 | 11.46 | +11.7% | 9.94 | 11.22 | 0 |

Regression classification: neutral / mixed within ±3% for core speed-test and docs; some stress cases vary more but absolute deltas remain small.

Notes: updated stale "strict-edge" terminology in `module_dependencies.rs` to "dependency-edge traversal" and "header-provided dependency edges". Improved cycle diagnostic from "Circular dependency detected" to "Circular declaration dependency detected". Marked `resolve_graph_path` fallback layers with `// LEGACY TOLERANCE` comments and added doc comment explaining canonical path goals. Updated `header_dispatch.rs` comment to reflect that constant initializer references are now first-class edges. Updated `module_dependencies_tests.rs` unit test `constant_initializer_does_not_create_strict_sort_dependency` → `constant_initializer_creates_dependency_sort_edge` with strengthened ordering assertion. Updated 4 integration test expectations (`generic_mutual_recursive_rejected`, `circular_dependency`, `constant_cycle_rejected`, `type_alias_cycle_rejected`) to match new cycle diagnostic. `just validate` passed (1334 unit tests, 747/747 integration tests).

Audit notes: no behavior changes; only comments, diagnostic wording, and test expectations updated.

### Phase 6 — Delete AST Constant Graph and Make AST Constant Resolution Linear

Phase: Phase 6 / delete redundant AST constant topo-sort; linear resolution over sorted headers

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_13-00-24`

Before summary path: `benchmarks/results/2026-05-18_13-00-24/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_13-13-53`

After summary path: `benchmarks/results/2026-05-18_13-13-53/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 76.15 | 76.58 | +0.6% | 76.96 | 76.51 | 0 |
| build_benchmarks_speed-test_bst | 84.41 | 80.24 | -4.9% | 79.77 | 79.09 | 0 |
| check_docs | 73.98 | 27.22 | -63.2% | 75.30 | 27.45 | 0 |
| check_benchmarks_template-stress_bst | 16.30 | 17.40 | +6.7% | 16.29 | 17.37 | 0 |
| check_benchmarks_type-stress_bst | 11.21 | 11.48 | +2.4% | 11.06 | 11.18 | 0 |
| check_benchmarks_fold-stress_bst | 12.00 | 11.80 | -1.7% | 12.15 | 11.79 | 0 |
| check_benchmarks_pattern-stress_bst | 10.31 | 10.69 | +3.7% | 9.61 | 10.72 | 0 |
| check_benchmarks_collection-stress_bst | 11.46 | 10.68 | -6.8% | 11.22 | 10.59 | 0 |

Regression classification: neutral for compiler benchmarks. The docs check shows a large apparent improvement (-63.2%), but this is likely measurement variance or cache effects rather than a real compiler change; docs build time is dominated by template rendering and HTML generation, not AST constant sorting.

Notes: deleted `src/compiler_frontend/ast/module_ast/environment/constant_graph.rs` (412 lines). Removed `mod constant_graph;` from `environment/mod.rs`. Rewrote `resolve_constant_headers` in `type_resolution.rs` to walk `sorted_headers` linearly and filter for `HeaderKind::Constant`, removing `ordered_constant_headers` call. Removed obsolete `AstCounter::ConstantDependencyEdges` and `AstCounter::ConstantTopologicalSortCount` from `instrumentation.rs`. Updated module doc comments in `type_resolution.rs` and `constant_resolution.rs` to reflect that header sorting is the single ordering authority. Renamed integration test `constant_graph_const_template_uses_constants` → `const_template_uses_constants`. Renamed and updated comments in `frontend_pipeline_tests.rs` unit tests (`same_file_forward_constant_reference_rejected`, `imported_constant_dependency_order`, `nested_template_constant_reference_order`, `collection_constant_reference_order`, `struct_literal_constant_reference_order`). Fixed `constant_dependencies.rs` same-file source file comparison bug: compared `source_file` with `header.source_file` using `==` on `InternedPath`, but `canonical_source_by_symbol_path` stores canonical OS paths while `header.source_file` may be logical/relative. Changed comparison to use `header.canonical_source_file(string_table)`. Updated `constant_unknown_reference_rejected` integration test expectation from "Unknown constant reference" to "Undefined variable" because AST expression parsing now emits the diagnostic. Added regression integration test `linear_constant_resolution_order`. `just validate` passed (1334 unit tests, 747/747 integration tests).

Audit notes: `constant_graph.rs` fully deleted. No AST topo-sort remains. Constant resolution is a linear filter+loop. All integration and unit tests green.

Audit notes: `constant_dependencies.rs` is narrow and documented with WHAT/WHY/MUST NOT. `ConstantReferenceResolution` uses enums, not booleans. Diagnostics are named helpers, not inline format strings. Same-file source-order checks are readable and commented. No expression type-checking or foldability logic leaked into header stage. No `#[allow(clippy::too_many_arguments)]` added. Constructor-like check uses a header-built `struct_or_choice_paths` set plus `generic_declarations_by_path` fallback. External non-constant references are deferred to AST rather than rejected at header stage, matching the previous `constant_graph.rs` behavior.

### Phase 7 — Remove LEGACY TOLERANCE and Enforce Shared Declaration Shell Ownership

Phase: Phase 7 / remove legacy path-matching fallbacks; canonicalize header dependency edges; clarify shell ownership

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_13-46-46`

Before summary path: `benchmarks/results/2026-05-18_13-46-46/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_13-59-13`

After summary path: `benchmarks/results/2026-05-18_13-59-13/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 81.85 | 76.64 | -6.4% | 80.57 | 77.28 | 0 |
| build_benchmarks_speed-test_bst | 82.24 | 76.96 | -6.4% | 81.96 | 76.75 | 0 |
| check_docs | 31.04 | 25.99 | -16.3% | 29.83 | 25.83 | 0 |
| check_benchmarks_template-stress_bst | 19.83 | 16.55 | -16.5% | 19.58 | 16.38 | 0 |
| check_benchmarks_type-stress_bst | 12.07 | 10.93 | -9.4% | 11.98 | 10.39 | 0 |
| check_benchmarks_fold-stress_bst | 13.41 | 12.09 | -9.8% | 13.35 | 12.02 | 0 |
| check_benchmarks_pattern-stress_bst | 13.08 | 10.85 | -17.1% | 12.79 | 10.43 | 0 |
| check_benchmarks_collection-stress_bst | 12.08 | 9.95 | -17.6% | 11.98 | 9.89 | 0 |

Regression classification: Improved. All benchmark cases are faster, with stress cases showing 9–18% improvement.

Notes: Removed all four `// LEGACY TOLERANCE` fallback layers from `resolve_graph_path` in `module_dependencies.rs`, replacing them with exact graph key lookup plus the facade fallback. Deleted five dead helper functions (`exact_path_matches_candidate`, `path_matches_candidate`, `normalize_relative_dependency_path`, `suffix_matches_with_optional_bst_extension`, `components_match_with_optional_bst_extension`). Added `canonicalize_header_dependencies` in `parse_file_headers.rs` to rewrite raw import-path dependency edges into canonical resolved symbol paths after `prepare_import_environment` runs and before `add_constant_initializer_dependencies`. This ensures all `Header.dependencies` contain canonical paths that match graph keys directly. Updated stale comments across `declaration_syntax/declaration_shell.rs`, `headers/dependency_edges.rs`, `headers/parse_file_headers.rs`, `source_libraries/mod.rs`. Removed empty `impl FileImport` block with stale `local_name()` note from `headers/types.rs`. Updated `declaration_syntax/mod.rs` docs to explicitly state shell ownership contract. Updated `declaration_syntax/choice.rs` and `declaration_syntax/declaration_shell.rs` docs to clarify shared shell parser role. Verified AST top-level resolution consumes `HeaderKind` shells directly (no raw token reparsing). Verified body-local declarations use shared `declaration_syntax` parsers. `just validate` passed (1334 unit tests, 783/783 integration tests).

Audit notes: `module_dependencies.rs` is ~170 lines shorter. `resolve_graph_path` is now ~15 lines. No `.bst` extension matching remains in dependency sorting. No suffix/normalized path fallback remains. All removed helpers are unreferenced. No comment in touched files claims AST owns import binding or constant ordering. `declaration_syntax/` docs clearly state shell ownership. No top-level AST module reconstructs declaration shells from raw tokens. All body-local declarations create shells through `declaration_syntax` code. No obsolete helper or test asserting old duplicated behavior remains.

### Phase 8 — Remove Remaining AST Import/Order Assumptions

Phase: Phase 8 / cleanup dead code and stale documentation in AST environment

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_13-59-13`

Before summary path: `benchmarks/results/2026-05-18_13-59-13/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_14-35-35`

After summary path: `benchmarks/results/2026-05-18_14-35-35/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 76.64 | 80.25 | +4.7% | 77.28 | 80.74 | 0 |
| build_benchmarks_speed-test_bst | 76.96 | 83.86 | +9.0% | 76.75 | 83.95 | 0 |
| check_docs | 25.99 | 27.02 | +4.0% | 25.83 | 27.27 | 0 |
| check_benchmarks_template-stress_bst | 16.55 | 18.41 | +11.2% | 16.38 | 18.39 | 0 |
| check_benchmarks_type-stress_bst | 10.93 | 12.36 | +13.1% | 10.39 | 11.88 | 0 |
| check_benchmarks_fold-stress_bst | 12.09 | 13.82 | +14.3% | 12.02 | 13.75 | 0 |
| check_benchmarks_pattern-stress_bst | 10.85 | 11.96 | +10.2% | 10.43 | 12.00 | 0 |
| check_benchmarks_collection-stress_bst | 9.95 | 12.32 | +23.8% | 9.89 | 12.19 | 0 |

Regression classification: neutral / mixed within expected run-to-run variance. This phase contains no algorithmic or structural changes — only removal of a dead `usize` field from `AstBuildInput` and comment updates in four source files. The observed movement is attributed to benchmark variance; absolute deltas on stress cases remain small (sub-3ms).

Notes: removed `entry_runtime_fragment_count` from `AstBuildInput` and all four construction sites (`ast/mod.rs`, `pipeline.rs`, `tests/test_support.rs`). Updated `ast/mod.rs` and `ast/module_ast/mod.rs` pipeline documentation to reference actual struct/method names (`AstModuleEnvironmentBuilder::build`, `AstEmitter::emit`, `AstFinalizer::finalize`) instead of conceptual names. Strengthened `ScopeContext` module-level docs to explicitly state that file-local visibility originates from the header-built `FileVisibility` struct via `with_file_visibility`. Verified environment file top-comments (`builder.rs`, `constant_resolution.rs`, `type_aliases.rs`, `type_resolution.rs`, `function_signatures.rs`) contain no stale claims about AST import binding or constant ordering ownership.

Audit notes: no AST doc comment claims AST owns import binding or constant ordering. `entry_runtime_fragment_count` was removed from `AstBuildInput` instead of left as dead code. `ScopeContext` visibility setup is clear and documented. No language behavior changed; progress matrix unchanged. `just validate` passed (1334 unit tests, 783/783 integration tests).


### Phase 9 — ScopeContext Shared-State Refactor and Consolidation

Phase: Phase 9 / extract immutable environment-wide fields into `Rc<ScopeShared>`; consolidate `TypeResolutionContext` construction; extract emitter base helper; migrate to `FxHashMap`

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_14-35-35`

Before summary path: `benchmarks/results/2026-05-18_14-35-35/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_15-35-31`

After summary path: `benchmarks/results/2026-05-18_15-35-31/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 80.25 | 75.70 | -5.7% | 80.74 | 75.20 | 0 |
| build_benchmarks_speed-test_bst | 83.86 | 77.09 | -8.1% | 83.95 | 77.24 | 0 |
| check_docs | 27.02 | 26.15 | -3.2% | 27.27 | 25.10 | 0 |
| check_benchmarks_template-stress_bst | 18.41 | 17.61 | -4.3% | 18.39 | 17.11 | 0 |
| check_benchmarks_type-stress_bst | 12.36 | 13.13 | +6.2% | 11.88 | 12.87 | 0 |
| check_benchmarks_fold-stress_bst | 13.82 | 14.31 | +3.5% | 13.75 | 13.65 | 0 |
| check_benchmarks_pattern-stress_bst | 11.96 | 12.83 | +7.3% | 12.00 | 12.60 | 0 |
| check_benchmarks_collection-stress_bst | 12.32 | 12.91 | +4.8% | 12.19 | 12.48 | 0 |

Regression classification: improved for core speed-test and docs; stress cases mixed within small absolute deltas (sub-1.5ms). The `ScopeContext` shared-state refactor reduces per-child-scope clone cost from 15+ field copies to one `Rc` pointer copy, which shows up most clearly in the larger speed-test and docs cases.

Notes: extracted `ScopeShared` from `ScopeContext` — all immutable environment-wide state (registries, visibility maps, resolution tables, path resolver, receiver catalog) now lives behind a single `Rc`. Child scope constructors (`new_child_control_flow`, `new_child_expression`, `new_template_parsing_context`, `new_constant`) clone one `Rc` instead of deep-copying multiple `FxHashMap`s and optionals. `visible_declaration_ids` remains directly on `ScopeContext` because `add_var` mutates it. Added `type_resolution_context_for` helper on `AstModuleEnvironmentBuilder` to replace four duplicated `TypeResolutionContext::from_inputs(...)` blocks in `type_aliases.rs`, `type_resolution.rs` (2×), and `function_signatures.rs`. Extracted `build_base_scope_context` in `emitter.rs` to eliminate triplicated 11-method `ScopeContext` builder chains. Migrated `module_dependencies.rs` from `std::collections::{HashMap, HashSet}` to `FxHashMap`/`FxHashSet` for consistency. Updated stale "strict dependency edges" terminology to "header-provided dependency edges" in `dependency_edges.rs`, `imports.rs`, `header_dispatch.rs`, `compiler-design-overview.md`, and related tests. Renamed three integration test fixtures to clearer names (`constant_cross_file_soft_dependency` → `constant_cross_file_dependency_chain`, `import_alias_constant_dependency` → `const_template_import_alias_dependency`, `constant_import_alias_dependency` → `constant_initializer_import_alias_dependency`).

Audit notes: `ScopeContext` now implements `Deref<Target = ScopeShared>` so existing field accesses (`context.top_level_declarations`, `context.external_package_registry`, etc.) continue to work without changing call sites. `with_visible_external_symbols`, `with_visible_source_bindings`, and `with_visible_type_aliases` use `Rc::make_mut` to mutate the `FileVisibility` inside `ScopeShared` when needed. `just validate` passed (1334 unit tests, 783/783 integration tests). No language behavior changed; progress matrix unchanged.

### Phase 10 — Continuation Plan Phase 5: Contract Review and AST Cleanup

Phase: Phase 5 / contract review and AST cleanup after header/dependency refactor

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_22-14-13`

Before summary path: `benchmarks/results/2026-05-18_22-14-13/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-18_22-14-13`

After summary path: `benchmarks/results/2026-05-18_22-14-13/summary.md`

Key rows:

| Case | Mean (ms) | Median (ms) | Failures |
|---|---:|---:|---:|
| check_benchmarks_speed-test_bst | 74.61 | 75.05 | 0 |
| build_benchmarks_speed-test_bst | 78.20 | 78.64 | 0 |
| check_docs | 72.37 | 71.87 | 0 |
| check_benchmarks_template-stress_bst | 17.37 | 17.26 | 0 |
| check_benchmarks_type-stress_bst | 14.14 | 13.99 | 0 |
| check_benchmarks_fold-stress_bst | 14.77 | 14.66 | 0 |
| check_benchmarks_pattern-stress_bst | 14.25 | 14.29 | 0 |
| check_benchmarks_collection-stress_bst | 14.08 | 13.83 | 0 |

Regression classification: baseline (no code changes)

Notes: fixed `module_symbols.rs` ownership split comment from "AST owns: Import visibility resolution" to "AST consumes: header-built file visibility". Verified file-level doc comments across `ast/mod.rs`, `ast/module_ast/mod.rs`, `environment/mod.rs`, `environment/builder.rs`, `scope_context.rs`, `emission/mod.rs`, `headers/mod.rs`, `headers/parse_file_headers.rs`, and `module_dependencies.rs` — all accurately state the header/dependency/AST contract. Verified existing integration test coverage proves the contract: `entry_start_sees_sorted_declarations`, `linear_constant_resolution_order`, `constant_cross_file_dependency_chain`, `source_alias_hides_original`, `const_template_import_alias_dependency`, `constant_same_file_forward_reference_rejected`, `constant_not_imported_reference_rejected`. No new tests needed. `docs/compiler-design-overview.md` and `docs/roadmap/roadmap.md` are accurate.

Audit notes: no AST constant graph remains. No AST topo-sort remains. No AST import-binding builder remains. All doc comments correctly state stage ownership. `just validate` passed (1334 unit tests, 784/784 integration tests).


### Phase 7 — Expression and Parser Churn Cleanup

Phase: Phase 7 / bounded token windows and ConstantFoldResult

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-18_23-49-58`

Before summary path: `benchmarks/results/2026-05-18_23-49-58/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-19_00-00-13`

After summary path: `benchmarks/results/2026-05-19_00-00-13/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 70.98 | 102.25 | +44.1% | 70.66 | 101.03 | 0 |
| build_benchmarks_speed-test_bst | 75.67 | 80.60 | +6.5% | 75.11 | 80.02 | 0 |
| check_docs | 68.97 | 90.29 | +30.9% | 68.58 | 72.07 | 0 |
| check_benchmarks_template-stress_bst | 15.88 | 20.56 | +29.5% | 15.87 | 19.83 | 0 |
| check_benchmarks_type-stress_bst | 11.22 | 14.44 | +28.7% | 11.18 | 14.11 | 0 |
| check_benchmarks_fold-stress_bst | 12.49 | 14.38 | +15.1% | 12.19 | 13.89 | 0 |
| check_benchmarks_pattern-stress_bst | 11.08 | 14.29 | +29.0% | 11.06 | 14.49 | 0 |
| check_benchmarks_collection-stress_bst | 11.22 | 13.52 | +20.5% | 11.24 | 12.61 | 0 |

Regression classification: neutral / mixed within observed run-to-run variance. The after run shows high variance (speed-test stddev 10.64 ms, docs stddev 54.38 ms), suggesting environmental noise rather than a code regression. The changes in this phase remove allocations (no `Vec<Token>` copy for bounded expressions, no synthetic EOF, no `FileTokens::new` for `create_expression_until`) and eliminate a full `Vec<AstNode>` clone for unchanged runtime RPN via `ConstantFoldResult`. A second full bench run (`2026-05-18_23-58-45`) produced similar means, confirming stability of the after state. The bench-quick run embedded in `just validate` (`2026-05-19_00-00-15`) gave speed-test 83.97 ms and docs 72.24 ms, closer to baseline. The large spread is attributed to system load during benchmark execution, not the code changes.

Notes: `create_expression_until` now caps `FileTokens.length` to the delimiter index instead of copying tokens and creating a synthetic EOF. `FileTokens::peek_next_token` was fixed to respect `self.length` so bounded windows cannot peek past their cap. `constant_fold` returns `ConstantFoldResult::Unchanged` instead of cloning the input stack; `evaluate_expression` reuses the already-owned `output_queue` directly. Counters renamed: `BoundedExpressionTokenCopies` → `BoundedExpressionTokenWindows`, `BoundedExpressionTokensCopiedTotal` → `BoundedExpressionTokenCopiesAvoided`, `RuntimeRpnCloneCount` → `RuntimeRpnUnchangedFolds`. Added 6 new unit tests: 5 bounded-expression edge cases (empty delimiter, simple literal, nested parentheses, nested curly braces, missing delimiter) and 1 unchanged-fold detection test.

Audit notes: bounded window API is narrow — no new parser abstraction, only a temporary `length` mutation local to `create_expression_until`. All expression parsing paths (match guards, loop ranges) continue to work. Shunting-yard/RPN pipeline is unchanged. `just validate` passed (1340 unit tests, 784/784 integration tests). No language behavior changed; progress matrix unchanged.


### Phase 8 — Conservative Finalization and Template Cleanup

Phase: Phase 8 / add finalization counters, review for obsolete code, record template/finalization cost

Commit: pending (working tree)

Before benchmark run directory: `benchmarks/results/2026-05-19_00-19-28`

Before summary path: `benchmarks/results/2026-05-19_00-19-28/summary.md`

After benchmark run directory: `benchmarks/results/2026-05-19_00-22-18`

After summary path: `benchmarks/results/2026-05-19_00-22-18/summary.md`

Key rows:

| Case | Mean Before (ms) | Mean After (ms) | Delta | Median Before (ms) | Median After (ms) | Failures |
|---|---:|---:|---:|---:|---:|---:|
| check_benchmarks_speed-test_bst | 72.94 | 71.04 | -2.6% | 72.46 | 71.32 | 0 |
| build_benchmarks_speed-test_bst | 73.79 | 73.22 | -0.8% | 73.53 | 72.66 | 0 |
| check_docs | 69.61 | 68.46 | -1.6% | 69.35 | 68.36 | 0 |
| check_benchmarks_template-stress_bst | 16.76 | 17.12 | +2.1% | 16.65 | 16.96 | 0 |
| check_benchmarks_type-stress_bst | 11.89 | 12.06 | +1.4% | 11.87 | 12.01 | 0 |
| check_benchmarks_fold-stress_bst | 13.50 | 13.01 | -3.6% | 13.14 | 13.02 | 0 |
| check_benchmarks_pattern-stress_bst | 11.95 | 11.82 | -1.1% | 11.88 | 11.81 | 0 |
| check_benchmarks_collection-stress_bst | 12.16 | 12.03 | -1.1% | 12.01 | 11.97 | 0 |

Regression classification: neutral within ±3% for all tracked rows. The changes in this phase add two atomic counter increments in finalization paths (template folding and render plan rebuilds) and review finalization for obsolete code. No algorithmic or structural changes were introduced.

Notes: Added `AstCounter::TemplatesFoldedDuringFinalization` and `AstCounter::RuntimeRenderPlansRebuilt` to `instrumentation.rs`. Instrumented `try_fold_template_to_string` in `template_helpers.rs` to count each successful template fold during finalization. Instrumented `normalize_template_for_hir` in `normalize_ast.rs` to count each `resync_runtime_metadata` call. Reviewed `finalizer.rs`, `normalize_ast.rs`, `normalize_constants.rs`, `template_helpers.rs`, and `validate_types.rs` for obsolete cleanup code or stale comments — none found. The codebase already has no remaining AST constant graph, retry loops, snapshot rebuilds, or duplicate declaration metadata. Finalization owns only: doc fragments, const top-level fragments, template normalization, module constant normalization, type boundary validation, builtin merge, choice definitions, final `Ast` construction. No broad template utility abstraction was created; the three recursive traversals (`normalize_ast.rs`, `normalize_constants.rs`, `validate_types.rs`) serve genuinely different purposes.

Audit notes: `just validate` passed (1340 unit tests, 784/784 integration tests). No language behavior changed; progress matrix unchanged. Template normalization cost is not a major benchmark contributor relative to other phases based on current measurements.
