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
