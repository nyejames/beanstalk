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
