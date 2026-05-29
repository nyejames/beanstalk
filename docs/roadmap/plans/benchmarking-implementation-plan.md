# Beanstalk compiler benchmarking implementation plan

## Purpose

Extend the existing benchmark system so compiler optimization work has enough local evidence to identify useful changes without making the tracked benchmark summaries noisy.

This plan supplements the current workflow:

```bash
just bench-check
just bench-frontend-check
just bench
just bench-frontend
```

The target model is:

```text
terse tracked summaries
+ local raw JSONL history
+ stable machine-readable counters
+ compact local drilldown report
+ optional profiling build helpers
```

The default path must stay fast. Expensive profiling, allocation tracking, trace output, and microbenchmarks remain targeted tools for active investigations only.

## Current repo anchors

Use the current implementation as the starting point, not a parallel benchmark system.

```text
benchmarks/README.md
benchmarks/cases.txt
benchmarks/frontend-cases.txt
justfile
Cargo.toml
src/benchmarking/mod.rs
src/benchmarking/frontend.rs
src/compiler_frontend/compiler_messages/compiler_dev_logging.rs
src/compiler_frontend/instrumentation.rs
src/compiler_frontend/pipeline.rs
src/build_system/create_project_modules/mod.rs
src/build_system/create_project_modules/frontend_orchestration.rs
src/compiler_frontend/module_dependencies.rs
xtask/src/main.rs
xtask/src/mode.rs
xtask/src/bench.rs
xtask/src/frontend_bench.rs
xtask/src/bench_observations.rs
xtask/src/bench_history.rs
xtask/src/bench_summary.rs
xtask/src/bench_types.rs
xtask/src/compiler_binary.rs
xtask/src/process_runner.rs
docs/roadmap/roadmap.md
docs/src/docs/progress/#page.bst
docs/codebase-style-guide.md
```

Current important facts:

- `benchmarks/README.md` already defines benchmarks as rough compiler-development sanity checks, not a statistical suite, public report, or CI timing gate.
- Tracked summaries live under `benchmarks/summaries/` and must stay terse.
- Detailed run data already lives in ignored local history under `benchmarks/local-data/runs.jsonl`.
- `LocalCaseRecord` and `BenchmarkCaseObservations` already have `counters` fields.
- `FrontendBenchmarkReport` already has a `counters` field, but `run_frontend_benchmark` currently returns an empty counter list.
- `src/compiler_frontend/instrumentation.rs` already contains atomic frontend counters for type-environment, type-compatibility-cache, string-table, and remap pressure.
- `compiler_dev_logging.rs` has a timing collector and stable `BST_BENCH timing ...` lines, but no equivalent stable counter transport.
- `bench_observations.rs` already parses stable timing lines and legacy timer prose. It should parse stable counter lines instead of relying on human counter prose.
- `just validate` currently runs the CLI benchmark check. Frontend benchmark validation should be run explicitly for benchmark-tooling changes rather than being added to the default validation path.

## Implementation principles

- [ ] Extend existing benchmark infrastructure; do not create a second system.
- [ ] Keep tracked summaries compact and human-readable.
- [ ] Keep detailed evidence local-only.
- [ ] Prefer one current API shape over compatibility shims; this repo is pre-release.
- [ ] Use stable snake_case metric names for machine-readable data.
- [ ] Record counters while the compiler is already touching the data.
- [ ] Do not add full extra AST/HIR/source traversals just to count things unless a targeted investigation justifies it.
- [ ] Keep compiler-stage ownership intact: Stage 0 counters stay in build-system-owned code, dependency counters stay in dependency sorting, AST counters stay in AST, HIR counters stay in HIR or immediately after HIR output, and borrow counters stay in borrow validation/reporting.
- [ ] Treat counters as diagnostic evidence. Timing movement still decides whether an optimization is meaningful.
- [ ] Follow `docs/codebase-style-guide.md`: readable named steps, named structs, no clever pipelines, no broad generic helpers, no noisy wrappers, no user-input panics.

## Consolidation and simplification targets

These are explicit complexity-reduction goals for the implementation.

- [ ] Do not add a new global counter subsystem. Extend `src/compiler_frontend/instrumentation.rs`.
- [ ] Replace the timing-only in-memory collector with one observation snapshot containing timings and counters.
- [ ] Use one stable counter output format: `BST_BENCH counter <metric>=<value>`.
- [ ] Stop relying on human counter sections for new records. After stable counter lines are wired, remove or ignore the human counter-section parser to avoid duplicate/unstable names.
- [ ] Keep legacy timing prose parsing because current benchmark output and old local expectations may still depend on it.
- [ ] Do not bump the local JSONL schema unless a new persisted field is actually added. Existing `counters` arrays should be enough.
- [ ] Reuse `BenchmarkCaseObservations`, `BenchmarkMetric`, `BenchmarkComparison`, `BenchmarkThresholds`, and stage movement helpers instead of creating parallel report data models.
- [ ] Add `bench-report` as one exact xtask mode. Do not refactor xtask into a broad argument parser yet.
- [ ] Keep the initial report implementation in `xtask/src/bench_report.rs`. Split later only if the file becomes hard to review.
- [ ] Add a profiling build helper in `justfile` first. Do not add Rust dependencies or profiler wrappers until a concrete investigation needs them.
- [ ] Do not change monthly summary rendering in the initial implementation.

## Final target workflow

```bash
# Correctness and rough CLI regression gate.
just validate

# Focused frontend timing/counter sanity check.
just bench-frontend-check

# Record local history and terse tracked summaries.
just bench
just bench-frontend

# Inspect local-only detail without writing files.
just bench-report

# Build a profiling-friendly binary for manual CPU profiling.
just profile-build
```

---

# Phase 0 — Baseline audit

## Context

Start with a no-code audit. The repo already has benchmark orchestration, summary writing, local JSONL history, stable timing lines, and partial compiler-side counters. The first agent task is to confirm exact current behavior before changing APIs.

## Checklist

- [ ] Confirm the working tree is clean.

  ```bash
  git status --short
  ```

- [ ] Read the benchmark contract.

  ```text
  benchmarks/README.md
  ```

- [ ] Inspect command wiring.

  ```text
  justfile
  xtask/src/main.rs
  xtask/src/mode.rs
  xtask/src/bench.rs
  xtask/src/frontend_bench.rs
  xtask/src/compiler_binary.rs
  xtask/src/process_runner.rs
  ```

- [ ] Inspect observation, comparison, summary, and history types.

  ```text
  xtask/src/bench_observations.rs
  xtask/src/bench_history.rs
  xtask/src/bench_summary.rs
  xtask/src/bench_types.rs
  ```

- [ ] Inspect compiler-side collection and frontend benchmark API.

  ```text
  src/benchmarking/mod.rs
  src/benchmarking/frontend.rs
  src/compiler_frontend/compiler_messages/compiler_dev_logging.rs
  src/compiler_frontend/instrumentation.rs
  ```

- [ ] Inspect cheap counter insertion points.

  ```text
  src/build_system/create_project_modules/mod.rs
  src/build_system/create_project_modules/frontend_orchestration.rs
  src/compiler_frontend/module_dependencies.rs
  src/compiler_frontend/ast/
  src/compiler_frontend/hir/
  src/compiler_frontend/analysis/borrow_checker/
  ```

- [ ] Run current validation and frontend benchmark checks.

  ```bash
  just validate
  just bench-frontend-check
  ```

- [ ] If no local baseline exists, create one intentionally.

  ```bash
  just bench
  just bench-frontend
  tail -n 2 benchmarks/local-data/runs.jsonl
  ```

- [ ] Confirm ignored local data is not staged.

  ```bash
  git status --short benchmarks/local-data
  ```

## Exit gate

- [ ] No source changes in this phase.
- [ ] Unexpected behavior is written down before implementation begins.
- [ ] The agent knows whether local history already contains old counter records.
- [ ] `just validate` and `just bench-frontend-check` pass, or exact failures are documented.

---

# Phase 1 — Documentation contract and deferred-feature status

## Context

Document the measurement model before changing behavior. This keeps later agents from turning the benchmark system into a noisy report generator.

## Files to change

```text
benchmarks/README.md
docs/roadmap/roadmap.md
docs/src/docs/progress/#page.bst
```

Optionally add the committed version of this plan as:

```text
docs/roadmap/plans/compiler_benchmarking_capabilities_plan.md
```

## Checklist — `benchmarks/README.md`

- [ ] Add a concise `Counter policy` section:

  ```text
  Counters are local diagnostic evidence, not public benchmark results.
  Stable counter metric names use snake_case.
  Counters are stored in local JSONL and used by local report tooling.
  Raw counter tables must not be added to tracked summaries.
  ```

- [ ] Document stable machine-readable formats:

  ```text
  BST_BENCH timing <metric>=<ms>ms
  BST_BENCH counter <metric>=<value>
  ```

- [ ] Add a `Local drilldown reports` section:

  ```text
  `just bench-report` reads local JSONL only.
  It does not update tracked summaries or append local history.
  It may show per-case, stage, counter, and ratio detail for active optimization work.
  ```

- [ ] Update `Raw Local History` to state that counters include both work-volume counters and implementation-pressure counters.
- [ ] Update `What Not To Do`:

  ```text
  Do not add raw counter dumps to tracked summaries.
  Do not add expensive counters that require new full-pipeline traversals without a targeted investigation.
  Do not treat counter movement as an optimization result unless timing moved meaningfully too.
  ```

## Checklist — `docs/roadmap/roadmap.md`

- [ ] Add a plan/TODO item:

  ```markdown
  - Compiler benchmarking and profiling observability:
    `docs/roadmap/plans/compiler_benchmarking_capabilities_plan.md`
    - Stabilise benchmark counter collection for CLI and in-process frontend runs.
    - Add local-only benchmark drilldown reports.
    - Keep tracked summaries terse.
    - Add profiling build helpers for targeted optimization work.
  ```

- [ ] Add a deliberate deferral note:

  ```markdown
  - Benchmarking/profiling deferred tooling: CI performance gates, public dashboards,
    Criterion microbenchmark suites, tracing span export, allocation-profiler wrappers,
    summary counter expansion, and source-library HIR caching remain deferred until stable
    counters and local reports identify hot paths worth isolating. These tools should be
    added only when they answer a specific optimization question and should not become
    part of the default validation path.
  ```

- [ ] Explicitly state that these are outside this implementation:

  ```text
  CI timing gates
  public performance dashboards
  source-library HIR caching
  ownership/drop/ABI specialization
  JS minification/tree-shaking
  package-manager caching
  broad Criterion benchmark suite
  tracing/allocation profiler integrations
  tracked-summary counter expansion
  ```

## Checklist — `docs/src/docs/progress/#page.bst`

Add or update a `Compiler development tooling` section. Keep it factual and matrix-shaped, not a design document.

- [ ] Add `Benchmark summaries`.

  ```text
  Status: Supported
  Coverage: Targeted
  Current role: Terse tracked monthly benchmark summaries for rough compiler-development trend tracking.
  Watch points: Do not add per-case tables or raw counter dumps. Detailed analysis belongs in local JSONL and local report commands.
  ```

- [ ] Add `Local benchmark history`.

  ```text
  Status: Supported
  Coverage: Targeted
  Current role: Local-only JSONL stores per-case timings, stage timings, counters, suite kind, primary metric, system identity, and commit metadata.
  Watch points: Must remain ignored/untracked. Schema changes should be deliberate and documented.
  ```

- [ ] Add `Benchmark work counters`.

  ```text
  Status: Partial initially; update to Supported after stable counter plumbing and first cheap counters land.
  Coverage: Targeted
  Current role: Local evidence for explaining stage movement and normalized costs.
  Watch points: Counters are diagnostic evidence, not public results. Prefer cheap counters recorded during existing work.
  ```

- [ ] Add `Local benchmark drilldown report`.

  ```text
  Status: Deferred initially; update to Partial after `bench-report` lands.
  Coverage: None initially
  Current role: Planned local-only report for slowest cases, stage movement, counter movement, normalized ratios, and next investigation candidates.
  Watch points: Must not write tracked summaries. Must not become a CI timing gate.
  ```

- [ ] Add `Profiling command wrappers`.

  ```text
  Status: Deferred / Targeted
  Coverage: None initially
  Current role: Optional local helpers for CPU profiling when benchmark reports identify an active investigation area.
  Watch points: Keep out of default validation. Avoid mandatory external tools.
  ```

- [ ] Add `CI performance gates / public dashboard`.

  ```text
  Status: Deferred
  Coverage: None
  Current role: Not part of Alpha. Current benchmarks are rough local sanity checks.
  Watch points: Do not fail CI on timing noise until benchmarks are stable enough and dedicated infrastructure exists.
  ```

- [ ] Add `Criterion microbenchmarks`.

  ```text
  Status: Deferred / Targeted
  Coverage: None initially
  Current role: Only for isolated hot functions after local benchmark reports identify them.
  Watch points: Do not replace end-to-end and frontend benchmark suites with microbenchmarks.
  ```

- [ ] Keep or update `Source-library HIR caching` as deferred. State that benchmark counters may quantify its cost but do not implement caching.

## Exit gate

- [ ] Documentation stays terse and non-speculative.
- [ ] Deferred items are not implied to be Alpha commitments.
- [ ] The progress matrix remains a status matrix.
- [ ] Run:

  ```bash
  cargo fmt
  just validate
  just bench-frontend-check
  ```

- [ ] Only intended documentation files are staged.

---

# Phase 2 — Stable counter transport and observation plumbing

## Context

The missing implementation seam is reliable transport from compiler counters to both benchmark paths:

```text
CLI benchmark:
  compiler stdout -> xtask parser -> BenchmarkCaseObservations -> local JSONL

In-process frontend benchmark:
  compiler collector -> FrontendBenchmarkReport -> BenchmarkCaseObservations -> local JSONL
```

This phase wires that seam before adding more counters.

## Files to change

```text
src/compiler_frontend/compiler_messages/compiler_dev_logging.rs
src/compiler_frontend/instrumentation.rs
src/benchmarking/frontend.rs
src/benchmarking/mod.rs
xtask/src/bench_observations.rs
xtask/src/bench_observations/tests.rs
xtask/src/frontend_bench.rs
```

## Checklist — compiler collector

- [ ] Replace the timing-only collector result with a named observation snapshot.

  Suggested shape:

  ```rust
  pub(crate) struct BenchmarkObservationSnapshot {
      pub(crate) timings: Vec<BenchmarkObservationMetric>,
      pub(crate) counters: Vec<BenchmarkObservationMetric>,
  }

  pub(crate) struct BenchmarkObservationMetric {
      pub(crate) name: String,
      pub(crate) value: f64,
  }
  ```

  Keep these as narrow as possible. Publicly expose only what `src/benchmarking/frontend.rs` needs.

- [ ] Update active collection state:

  ```rust
  struct ActiveBenchmarkCollection {
      timings: Vec<BenchmarkObservationMetric>,
      counters: Vec<BenchmarkObservationMetric>,
      suppress_output: bool,
  }
  ```

- [ ] Add stable counter recording:

  ```rust
  #[cfg(feature = "detailed_timers")]
  pub fn log_benchmark_counter(metric_name: &str, value: f64)
  ```

- [ ] `log_benchmark_counter` must:

  - [ ] record into the active collector even when output is suppressed,
  - [ ] print `BST_BENCH counter <metric>=<value>` only when detailed timer output is enabled,
  - [ ] reject or ignore empty metric names at the call boundary if practical,
  - [ ] avoid human labels in the stable line.

- [ ] Replace `stop_and_collect_benchmark_timings()` with:

  ```rust
  stop_and_collect_benchmark_observations()
  ```

  Do not keep a long-term compatibility wrapper. This is internal pre-release API.

## Checklist — instrumentation module

- [ ] Keep `FrontendCounter` as the single compiler-side counter enum.
- [ ] Add `counter_metric_name(counter) -> &'static str` for every existing variant.
- [ ] Use snake_case names, for example:

  ```text
  type_environment_fields_for_queries
  type_environment_fields_returned
  type_environment_variants_for_queries
  type_environment_variants_returned
  type_environment_substitute_type_id_calls
  type_environment_substitution_cache_lookups
  type_environment_substitution_cache_hits
  type_environment_substitution_cache_misses
  type_compatibility_cache_lookups
  type_compatibility_cache_hits
  type_compatibility_cache_misses
  string_table_full_clones
  string_table_merge_source_entries_scanned
  module_remap_string_ids_calls
  ```

- [ ] Change `log_frontend_counters()` so it records counters regardless of suppressed output.
- [ ] If human counter prose remains, make it optional display only. Do not parse it for new records.
- [ ] Keep `reset_frontend_counters()` command-scoped as it is today.

## Checklist — frontend benchmark API

- [ ] Update `run_frontend_benchmark` to collect observations once after `compile_project_frontend`.
- [ ] Populate `FrontendBenchmarkReport.stages` from observation timings.
- [ ] Populate `FrontendBenchmarkReport.counters` from observation counters.
- [ ] Preserve empty vectors when compiled without `detailed_timers`.
- [ ] Keep `start_benchmark_collection(true)` for in-process frontend benchmarks so output is suppressed but data is still collected.

## Checklist — xtask stdout parser

- [ ] Add stable counter parsing:

  ```rust
  const STABLE_COUNTER_PREFIX: &str = "BST_BENCH counter";
  ```

- [ ] Parse lines shaped as:

  ```text
  BST_BENCH counter <name>=<number>
  ```

- [ ] Strip ANSI before parsing, as timing parsing already does.
- [ ] Reject empty names and non-numeric values by ignoring the malformed line.
- [ ] Remove or ignore legacy human counter-section parsing for new output. Prefer no human counter parser unless a specific current compatibility need is found in Phase 0.
- [ ] Keep stable timing parsing and legacy timing prose parsing unchanged.

## Checklist — tests

- [ ] Add parser tests for:

  ```text
  stable counter line
  multiple stable counter lines
  malformed counter line ignored
  ANSI-stripped stable counter line
  timing parsing unaffected
  counters averaged across iterations
  ```

- [ ] Add a frontend benchmark conversion test that a constructed `FrontendBenchmarkReport` with counters becomes `BenchmarkCaseObservations` with counters.
- [ ] Add a focused collector test if there is a clean test surface that does not require a full compiler run.

## Exit gate

- [ ] No new counter subsystem exists.
- [ ] No duplicate collector APIs remain.
- [ ] Stable counter names are snake_case.
- [ ] In-process frontend benchmarks can collect counters with output suppressed.
- [ ] No monthly summary behavior changes.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test --quiet --package xtask
  cargo test --quiet benchmarking
  just validate
  just bench-frontend-check
  ```

- [ ] Record one local frontend run and inspect that counters appear in JSONL.

  ```bash
  just bench-frontend
  tail -n 1 benchmarks/local-data/runs.jsonl
  git status --short benchmarks/local-data
  ```

---

# Phase 3 — First cheap work-volume counters

## Context

Existing counters mostly describe implementation pressure. Add a small first set of work-volume counters so timing movement can be interpreted without adding noisy or expensive traversals.

The first set should answer:

```text
Did the compiler get slower because it processed more input/work,
or because the same amount of work became more expensive?
```

## Files likely to change

```text
src/compiler_frontend/instrumentation.rs
src/build_system/create_project_modules/mod.rs
src/build_system/create_project_modules/frontend_orchestration.rs
src/compiler_frontend/module_dependencies.rs
src/compiler_frontend/ast/
src/compiler_frontend/hir/
src/compiler_frontend/analysis/borrow_checker/
```

## First counter set

Add only counters that are cheap from existing data or already-maintained stats.

### Input and file preparation

- [ ] `module_count`
- [ ] `source_file_count`
- [ ] `source_byte_count`
- [ ] `prepared_file_count`
- [ ] `token_count`
- [ ] `header_count`
- [ ] `import_count`
- [ ] `top_level_declaration_count`

### Dependency sorting

- [ ] `dependency_header_count`
- [ ] `dependency_edge_count`
- [ ] `dependency_visit_count`

### AST / type / template boundary

Start with phase-boundary counts that are already known. Do not add a full AST walk.

- [ ] `ast_header_count`
- [ ] `ast_function_count`
- [ ] `ast_struct_count`
- [ ] `ast_choice_count`
- [ ] `ast_constant_count`
- [ ] `ast_receiver_method_count`
- [ ] `ast_generic_template_count`
- [ ] `ast_generic_instance_count`
- [ ] `constant_fold_attempt_count`
- [ ] `constant_fold_success_count`
- [ ] `template_count`
- [ ] `const_template_count`
- [ ] `runtime_template_count`

If any of these require a new traversal or awkward ownership leak, defer that specific counter.

### HIR

- [ ] `hir_block_count`
- [ ] `hir_statement_count`
- [ ] `hir_function_count`

Defer deep expression/category counters such as calls, branches, and loops unless the lowering path already exposes them cheaply.

### Borrow validation

Reuse `BorrowCheckReport` stats and side-table lengths.

- [ ] `borrow_function_count`
- [ ] `borrow_block_count`
- [ ] `borrow_conflict_check_count`
- [ ] `borrow_state_snapshot_count`
- [ ] `borrow_statement_fact_count`
- [ ] `borrow_terminator_fact_count`
- [ ] `borrow_value_fact_count`

Defer `borrow_drop_if_owned_site_count` and last-use candidate counts unless they are already exposed without extra traversal.

## Checklist — insertion rules

- [ ] Record `source_file_count` and `source_byte_count` where the `InputFile` slice is already available in module compilation.
- [ ] Record `prepared_file_count` from the same module file count.
- [ ] Record `token_count` immediately after tokenization or add a narrow count field to `FileFrontendPrepareOutput` if token data is already available there.
- [ ] Record `header_count`, `import_count`, and `top_level_declaration_count` from header aggregation data, not by reparsing source.
- [ ] Record dependency counts inside `resolve_module_dependencies` and `visit_node` without formatting paths.
- [ ] Record HIR block/statement/function counts once after HIR generation succeeds.
- [ ] Record borrow counters immediately after borrow validation succeeds, using existing report stats.
- [ ] Do not record user source text, rendered paths, or diagnostic prose in counters.
- [ ] Do not change compiler behavior to make a counter easier.

## Deferred counter set

Keep these out of the first pass unless an active investigation needs them:

```text
source_library_root_count
external_import_provider_file_count
dependency_facade_export_edge_count
template_slot_count
template_insert_count
hir_hidden_local_count
hir_call_count
hir_branch_count
hir_loop_count
borrow_drop_if_owned_site_count
borrow_last_use_candidate_count
backend_module_count
emitted_js_byte_count
runtime_asset_count
external_glue_function_count
html_artifact_count
```

## Exit gate

- [ ] Every new counter is cheap or explicitly deferred.
- [ ] Counter names are stable snake_case.
- [ ] No extra full AST/HIR/source traversal was added without justification.
- [ ] No stage ownership boundary was crossed just for instrumentation.
- [ ] Parallel file preparation remains deterministic.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test --quiet
  just validate
  just bench-frontend-check
  ```

- [ ] Record both suites once and verify counters are present.

  ```bash
  just bench
  just bench-frontend
  tail -n 2 benchmarks/local-data/runs.jsonl
  git status --short benchmarks/local-data
  ```

- [ ] Confirm tracked summaries remain terse.

---

# Phase 4 — Local-only benchmark drilldown report

## Context

Once counters are stable, add a compact local report that reads `runs.jsonl` and helps choose the next investigation. It must not write files or change tracked summaries.

The report should answer:

```text
What are the slowest cases?
Which stages moved?
Which counters moved?
Which normalized ratios look suspicious?
Which case should be profiled next?
```

## Files to change

```text
justfile
xtask/src/main.rs
xtask/src/mode.rs
xtask/src/bench_report.rs      # new
xtask/src/bench_history.rs
xtask/src/bench_types.rs
```

Avoid adding `bench_report_model.rs` unless `bench_report.rs` becomes too large to review.

## Command design

- [ ] Add one xtask mode:

  ```bash
  cargo run --package xtask --bin xtask -- bench-report
  ```

- [ ] Add one just command:

  ```make
  bench-report:
      cargo run --package xtask --bin xtask -- bench-report
  ```

- [ ] Keep the existing simple `args.len() == 2` xtask shape.
- [ ] Do not add report flags in the first version.
- [ ] Do not append local history.
- [ ] Do not update monthly summaries.

## Report behavior

- [ ] Read `benchmarks/local-data/runs.jsonl`.
- [ ] Load current system identity in read-only mode if possible.
- [ ] For each suite kind, find the latest matching run for the current system.
- [ ] If there is no system identity, report the latest run per suite kind and print a short local-only note.
- [ ] Compare the latest run with the previous matching run for the same system and suite kind.
- [ ] Handle older records with missing counters without failing.

## Initial output shape

Keep the report plain and capped:

```text
Benchmark report: local data only

Frontend phases / macOS M1 (B7F2A9)
Latest: May 29th - 12:40, commit abc1234
Change: -8ms avg; 3 faster, 0 slower; 9/9 cases

Slowest cases:
  environment-stress        ~184ms  ast_ms ~112ms, hir_ms ~24ms
  template-stress           ~151ms  ast_ms ~89ms, file_prepare_ms ~18ms
  import-fanout             ~130ms  file_prepare_ms ~52ms, dependency_sort_ms ~12ms

Stage movement:
  ast_ms                    -14ms across 4 cases
  file_prepare_ms            +5ms across 2 cases

Counter movement:
  token_count                +9%
  type_compatibility_cache_misses -18%

Ratios:
  file_prepare_ms/source_file_count  import-fanout       3.1ms/file
  ast_ms/ast_header_count            environment-stress  1.42ms/header
  borrow_ms/borrow_conflict_check_count borrow-stress    0.004ms/check

Next investigation candidates:
  environment-stress: high ast_ms and type-compatibility pressure
  import-fanout: high file_prepare_ms/source_file_count
```

## Checklist — implementation

- [ ] Reuse `LocalRunRecord`, `to_case_results`, `BenchmarkComparison`, and existing threshold logic.
- [ ] Keep report calculation separate from formatting inside `bench_report.rs`.
- [ ] Use named structs for report sections only where they improve clarity.
- [ ] Show top 3 slowest cases per suite.
- [ ] Show top 2 current stages per slow case.
- [ ] Show top meaningful stage movers only.
- [ ] Show top 3 counter movements only.
- [ ] Display percentage movement only when the previous value is non-zero.
- [ ] Display absolute movement for zero-to-nonzero counters.
- [ ] Skip missing numerator/denominator ratios.
- [ ] Skip ratios with zero denominator.
- [ ] Cap ratios to 5.
- [ ] Cap investigation candidates to 3.
- [ ] Phrase investigation candidates as hints, not conclusions.

## Initial ratio catalog

Use a small static catalog:

```text
file_prepare_ms / source_file_count
file_prepare_ms / source_byte_count
file_prepare_ms / token_count
dependency_sort_ms / dependency_edge_count
ast_ms / ast_header_count
ast_ms / type_compatibility_cache_lookups
ast_ms / type_compatibility_cache_misses
hir_ms / hir_statement_count
borrow_ms / borrow_conflict_check_count
borrow_ms / borrow_statement_fact_count
borrow_ms / borrow_value_fact_count
```

## Initial investigation hints

- [ ] High `file_prepare_ms/source_file_count`: inspect tokenization, header parsing, string-table merge/remap.
- [ ] High `dependency_sort_ms/dependency_edge_count`: inspect duplicate edges or graph traversal.
- [ ] High `ast_ms/type_compatibility_cache_misses`: inspect compatibility caching or repeated type checks.
- [ ] High `borrow_ms/borrow_conflict_check_count`: inspect borrow state representation.
- [ ] Stable counters but slower timing: run CPU profiling before refactoring.

## Tests

- [ ] Add report calculation tests with small fake local records.
- [ ] Test no local history.
- [ ] Test only CLI history.
- [ ] Test only frontend history.
- [ ] Test missing counters from older records.
- [ ] Test zero denominator ratios are skipped.

## Exit gate

- [ ] `bench-report` writes no files.
- [ ] Existing benchmark commands still behave the same.
- [ ] Output is compact enough for routine local use.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test --quiet --package xtask
  just validate
  just bench-frontend-check
  just bench-report
  ```

---

# Phase 5 — Profiling build helper

## Context

Counters and reports identify where to investigate. CPU profilers explain why a case is expensive. This phase adds only the build support needed for manual profiling.

## Files to change

```text
Cargo.toml
justfile
benchmarks/README.md
```

## Checklist — Cargo profile

- [ ] Add a profiling profile without changing the release profile.

  ```toml
  [profile.profiling]
  inherits = "release"
  debug = "line-tables-only"
  strip = false
  lto = "thin"
  codegen-units = 1
  panic = "abort"
  ```

## Checklist — justfile

- [ ] Add:

  ```make
  profile-build:
      RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --features detailed_timers
  ```

- [ ] Optionally add a simple runner that does not assume external profilers exist:

  ```make
  profile-check path:
      RUSTFLAGS="-C force-frame-pointers=yes" cargo build --profile profiling --features detailed_timers
      target/profiling/bean check {{path}}
  ```

- [ ] Do not add commands that assume `samply`, `perf`, `heaptrack`, or `dhat` are installed unless they check for the tool and fail clearly.

## Checklist — docs

- [ ] Add a short local profiling note to `benchmarks/README.md`:

  ```text
  Use `just bench-report` to choose a case and stage first.
  Build with `just profile-build`.
  Run an external profiler against `target/profiling/bean`.
  Do not commit profiler output.
  ```

- [ ] Mention example commands as examples only, not required tooling:

  ```text
  samply record target/profiling/bean check benchmarks/template-stress.bst
  perf record --call-graph dwarf target/profiling/bean check benchmarks/template-stress.bst
  ```

## Exit gate

- [ ] Normal release builds are unchanged.
- [ ] Normal benchmark commands are unchanged.
- [ ] No external profiler dependency is added to the workspace.
- [ ] Run:

  ```bash
  cargo fmt
  cargo build --profile profiling --features detailed_timers
  just validate
  just bench-frontend-check
  ```

---

# Phase 6 — Targeted fixture expansion

## Context

Add new benchmark fixtures only after reports can explain them. The existing suite already has baseline, template, type, folding, pattern, collection, environment, module/import, and borrow coverage. New cases must add distinct signal.

## Files to change

```text
benchmarks/cases.txt
benchmarks/frontend-cases.txt
benchmarks/README.md
benchmarks/<new-fixture>/
```

## Candidate fixtures

Add one fixture at a time.

- [ ] `many-small-components/`

  ```text
  many small files, imports, facades, templates, receiver methods
  ```

- [ ] `source-library-heavy/`

  ```text
  source-library import/facade cost; useful before considering source-library HIR caching
  ```

- [ ] `external-js-imports/`

  ```text
  provider-backed JS import metadata, runtime imports, external package visibility, glue planning
  ```

- [ ] `template-site-large/`

  ```text
  top-level const/runtime fragments, nested templates, style directives, const folding
  ```

Controlled generated-style fixtures should remain rare:

```text
import_chain_100
import_fanout_100
const_chain_1000
template_depth_100
generic_instances_200
borrow_cfg_diamonds_50
```

Do not add a generator until hand-authored fixtures become hard to maintain.

## Checklist

- [ ] Add only source inputs. Do not commit generated `dev/` or `release/` output.
- [ ] Add the case to `cases.txt`, `frontend-cases.txt`, or both only when it gives distinct signal.
- [ ] Prefer existing groups unless a new group clearly improves summary readability.
- [ ] Update the fixture list in `benchmarks/README.md`.
- [ ] Run `just bench-report` before and after adding the case to confirm it adds useful signal.
- [ ] Do not add failing diagnostic fixtures to benchmarks.

## Exit gate

- [ ] The fixture is a valid program or project.
- [ ] It exercises a distinct compiler/build-system path.
- [ ] It is readable enough to maintain.
- [ ] It does not duplicate existing stress coverage.
- [ ] Run:

  ```bash
  cargo fmt
  just validate
  just bench-frontend-check
  just bench-report
  ```

---

# Explicitly deferred or targeted work

These items must remain documented as deferred/targeted in `docs/roadmap/roadmap.md` and `docs/src/docs/progress/#page.bst` until separately implemented.

## Summary counter enrichment

Do not implement in the initial benchmark tooling pass.

It may be considered only after `bench-report` has been used on real optimization work and the same small set of work-volume counters repeatedly explains timing movement.

Allowed future shape, at most one line:

```text
Work movement: token_count +9%, ast_header_count +6%
```

Rules if later implemented:

- [ ] No per-case rows.
- [ ] No raw counter dumps.
- [ ] No implementation-pressure counters by default.
- [ ] Cap to 2 counters.
- [ ] Hide on baseline and no-measurable-change runs.
- [ ] Hide when case sets changed and movement is not comparable.

## Tracing span export

Deferred/targeted.

Potential future feature:

```toml
[features]
perf_trace = []
```

Potential spans:

```text
project.discover_modules
project.prepare_source_files
frontend.prepare_file
frontend.merge_string_table_delta
frontend.sort_dependencies
ast.build_environment
ast.emit_nodes
ast.finalize
hir.generate_module
borrow.validate_module
backend_js.emit_project
```

Rules:

- [ ] Trace output is opt-in.
- [ ] Trace files go under ignored local data.
- [ ] Trace spans follow stage ownership.
- [ ] Normal benchmarks do not require tracing.

## Allocation profiling wrappers

Deferred/targeted.

Potential future feature:

```toml
[features]
perf_allocs = []
```

Rules:

- [ ] Global allocator changes are isolated behind the feature.
- [ ] Allocation output is ignored.
- [ ] No allocation profiling in `just validate`.
- [ ] No allocation-specific compiler behavior paths.

## Criterion microbenchmarks

Deferred/targeted.

Add Criterion only for isolated hot functions after project-shaped benchmarks and `bench-report` identify a stable inner loop, such as tokenizer scanning, dependency sorting, type compatibility, constant folding, template slot resolution, or borrow state transitions.

Criterion must not replace `bench` or `bench-frontend` as the source of truth.

## CI timing gates and public dashboards

Deferred.

Current benchmarks are rough local sanity checks. Do not fail CI on timing movement until dedicated infrastructure and noise handling exist.

## Source-library HIR caching

Deferred.

Benchmark counters may show source-library recompilation cost, but caching should wait until the source-library/package model is stable enough to design invalidation, identity, diagnostics, and artifact boundaries.

## Ownership/drop/ABI specialization

Deferred as compiler/backend optimization work.

Counters may inform this later, but this plan must not make ownership semantic. GC fallback remains the language baseline, and borrow validation facts remain side-table optimization evidence.

---

# Final readiness review for the future optimization skill

The benchmarking work is ready to support an optimization agent when all of the following are true:

- [ ] `just bench-frontend-check` shows useful stage and counter data without writing history.
- [ ] `just bench-report` identifies slowest cases, stage movement, counter movement, ratios, and investigation candidates.
- [ ] Local JSONL contains stable counter names.
- [ ] Old local records without counters do not break reports.
- [ ] Tracked summaries remain compact.
- [ ] Roadmap and progress matrix clearly mark CI gates, dashboards, tracing, allocation profiling, Criterion, and source-library caching as deferred/targeted.
- [ ] Profiling build helper exists for manual CPU profiler use.

The future optimization loop should be:

```text
1. Run the smallest relevant benchmark check.
2. Inspect stage movement.
3. Inspect counters and ratios.
4. Profile only when counters do not explain timing movement.
5. Make one focused change.
6. Run correctness validation.
7. Re-run the same benchmark suite.
8. Keep the change only if it gives a tangible improvement or an independent readability win.
```

Optimization changes should prefer simple free wins:

```text
remove avoidable clones
pre-size vectors when size is already known
avoid repeated path/string formatting
use borrowed TypeEnvironment views instead of cloned data
avoid full StringTable clone/merge paths in ordinary module work
avoid duplicate dependency edges
cache repeated type compatibility checks where ownership is clear
remove dead compatibility wrappers
keep hot loops straightforward and readable
```

Reject changes that add indirection, cleverness, or stage-boundary friction unless the benchmark improvement is tangible.

---

# Suggested PR breakdown

## PR 1 — Benchmark documentation contract

- [ ] Phase 1 docs changes only.

Validation:

```bash
cargo fmt
just validate
just bench-frontend-check
```

## PR 2 — Stable counter transport

- [ ] Phase 2 collector changes.
- [ ] Stable `BST_BENCH counter` parser.
- [ ] Frontend benchmark reports populated with counters.
- [ ] Parser/collector/conversion tests.

Validation:

```bash
cargo fmt
cargo test --quiet --package xtask
cargo test --quiet benchmarking
just validate
just bench-frontend-check
```

## PR 3 — First cheap work-volume counters

- [ ] Phase 3 counters.
- [ ] No report command yet unless needed for local verification.

Validation:

```bash
cargo fmt
cargo test --quiet
just validate
just bench-frontend-check
just bench
just bench-frontend
```

Do not stage `benchmarks/local-data/`.

## PR 4 — Local report command

- [ ] Phase 4 `bench-report`.
- [ ] Justfile command.
- [ ] Report calculation tests.

Validation:

```bash
cargo fmt
cargo test --quiet --package xtask
just validate
just bench-frontend-check
just bench-report
```

## PR 5 — Profiling profile helper

- [ ] Phase 5 profiling profile.
- [ ] Minimal justfile helper.
- [ ] README profiling note.

Validation:

```bash
cargo fmt
cargo build --profile profiling --features detailed_timers
just validate
just bench-frontend-check
```

## PR 6 — Targeted fixture expansion

- [ ] One fixture only unless the first fixture proves the pattern.
- [ ] Case-list update.
- [ ] README fixture-list update.

Validation:

```bash
cargo fmt
just validate
just bench-frontend-check
just bench-report
```

## Optional later PR — Summary counter enrichment

Only after local reports prove the line is worth tracking.

Validation:

```bash
cargo fmt
cargo test --quiet --package xtask
just validate
just bench-frontend-check
just bench-frontend
git diff -- benchmarks/summaries
```

Reject the PR if the summary diff feels noisy.
