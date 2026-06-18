# Beanstalk Benchmarks

## Purpose

The benchmark system is a rough compiler-development sanity check. It is meant to answer whether a change obviously helped, hurt, or did nothing measurable.

It is not a Criterion-style statistical benchmark suite, a CI timing gate, or a public performance report. Public summaries stay terse so they can be tracked in the repo without becoming noisy.

## Commands

```bash
just bench-check
just bench-frontend-check
just bench
just bench-frontend
```

`just bench-check` runs the end-to-end CLI suite without writing local history or tracked summaries. Use it for validation.

`just bench-frontend-check` runs the focused in-process frontend suite without writing local history or tracked summaries. Use it when compiler-stage changes are too small to read through subprocess noise.

`just bench` records an end-to-end CLI run, updates local raw history under `benchmarks/local-data/`, and updates the current monthly summary under `benchmarks/summaries/`.

`just bench-frontend` records the focused frontend suite through the same local history and monthly summary flow, but under a separate suite kind.

Each suite uses one warmup iteration and ten measured iterations per case.

### Profiling commands

```bash
just profile                  # default terse filter across all cases
just profile <filter>         # named filter: terse, normal, deep, raw-index
just profile-case <case-name> [filter]   # profile one specific case
just profile-build            # build the profiling binary (target/profiling/bean)
```

Run `just bench-report` first to identify which case and stage are worth profiling.

## Measurement Model

CLI wall-clock time is the public rough regression signal. It measures the built `bean` binary as a subprocess, so it includes command startup, project loading, frontend compilation, backend work where relevant, and output handling.

Compiler stage timings are attribution and debugging evidence. They help explain whether obvious movement likely came from file preparation, dependency sorting, AST, HIR, borrow validation, or another instrumented stage.

Stage observations are emitted as stable `BST_BENCH timing <metric>=<ms>ms` lines when the compiler is built with `detailed_timers`. Human timer prose is kept for developer readability, but benchmark parsing should prefer the stable metric lines.

Counter observations are local diagnostic evidence, not public benchmark results. Stable counter metric names use snake_case and are emitted as `BST_BENCH counter <metric>=<value>` lines. Counters are stored in local JSONL and used by local report tooling; raw counter tables must not be added to tracked summaries.

The current `file_prepare_ms` metric is the combined parallel file-preparation aggregate: per-file tokenization, header parsing, local string-table work, and deterministic merge/remap into the module table. Older local records may still contain separate legacy `tokenize_ms` or `headers_ms` observations.

In-process frontend timings call production compiler paths directly and stop at the documented frontend/backend boundary after HIR and borrow validation. They are useful for compiler refactors, but they are still rough development signals rather than precise measurements.

`no measurable change` means no overlapping benchmark case exceeded the deliberately rough comparison threshold.

## Suite Kinds

`end_to_end_cli` is the normal CLI benchmark suite. Its primary metric is subprocess wall-clock time.

`frontend_phases` is the focused in-process frontend suite. Its primary metric is total frontend time, with stage timings used for attribution.

Local history records the suite kind and primary metric so CLI and frontend runs are never compared against each other.

## Case Groups

End-to-end benchmark cases live in `benchmarks/cases.txt`. Focused frontend cases live in `benchmarks/frontend-cases.txt`.

Both files use group directives:

```text
# group: core
check benchmarks/speed-test.bst
build benchmarks/speed-test.bst
```

Groups are public summary labels, not compiler architecture boundaries:

- `core`: baseline check/build cases.
- `docs`: documentation project checking.
- `stress`: targeted template, type, fold, pattern, collection, and environment stress fixtures.
- `module`: module/import/dependency graph and import fanout coverage.
- `borrow`: valid borrow and exclusivity coverage.

## Summary Interpretation

Monthly summaries show absolute average times for `all` cases and for each group. Group averages provide context without adding long per-case tables.

`Case spread latest` is spread across different benchmark cases. It is not timing uncertainty.

`**-18ms avg**; 5 faster, 0 slower` means an obvious improvement across shared cases.

`no measurable change` means no overlapping benchmark case exceeded the rough per-case threshold.

`mixed` means at least one case improved and at least one case regressed. Inspect local JSONL or rerun before drawing broad conclusions.

`case set changed` means cases were added or removed, so only shared cases are directly comparable.

## Optimization Phase Protocol

For compiler optimization phases, run both focused frontend and end-to-end suites five independent
times and compare the benchmark-system medians. Keep the suite's normal warmup/measured iteration
model; repeat the whole recorded command rather than changing per-case iteration counts.

Use `just bench-report` and targeted `just profile-case <case-name>` runs for attribution. Record
only concise conclusions in `benchmarks/frontend-optimization-results.md` and the tracked monthly
summary. Raw benchmark history, raw profiles, and expanded counter tables stay local-only.

## Stage Movement Interpretation

`Stage movement: ast +22ms` suggests the change likely affected AST construction, but the benchmark is still rough. Confirm with frontend benchmarks or targeted profiling if the change matters.

Only the top meaningful stage movers are shown. Full per-case stage data stays local-only.

Stage movement should explain a benchmark result, not replace it. Treat it as a clue for where to investigate.

## Raw Local History

Detailed run data is local-only in `benchmarks/local-data/runs.jsonl`. Do not commit raw local history.

Raw records include per-case means, medians, standard deviations, stage timings, counters, suite kind, primary metric name, system identity, and commit metadata when available. Counters include work-volume counters and implementation-pressure counters.

The tracked Markdown summaries under `benchmarks/summaries/` are the public record. They must stay concise.

## Local Drilldown Reports

`just bench-report` reads local JSONL only. It does not update tracked summaries or append local history.

Use it for compact per-case, stage, counter, and ratio detail during active optimization work.

## Local Profiling

Use `just bench-report` to choose a case and stage before profiling. Then run `just profile` or `just profile-case <case-name>` to collect Samply-backed stack samples alongside `detailed_timers` stage and counter observations.

### Two-run model

Each profiling case runs twice:

1. **Observation pass** — a non-profiled run that collects `detailed_timers` stage timings and counter data.
2. **Samply pass** — records stack samples into a raw profile.

The observation pass provides reliable stage/counter attribution without profiler overhead. The Samply pass provides call-stack evidence.

### Profiling binary

The profiling binary is built to `target/profiling/bean` using `just profile-build`. It uses release settings with frame-pointer debug info and `detailed_timers`. Do not commit the binary.

### Filter modes

Filter modes control how much detail appears in summaries:

| Mode | Purpose | Keeps |
|---|---|---|
| `terse` | agent-first default | top 8 Beanstalk-owned functions per case, top 3 cases in root summary |
| `normal` | human + agent investigation | top 20 functions per case, top 8 cases in root summary |
| `deep` | pre-refactor investigation | top 50 functions per case, all profiled cases, caller/callee context |
| `raw-index` | artifact generation only | raw profile and observation logs, no parsed hotspots |

`terse` is the default when no filter is specified.

### Output layout

```text
benchmarks/local-data/
├── profile-runs.jsonl              # derived local history (not raw profiles)
└── profiles/
    └── <run-id>/
        ├── agent-summary.md        # start here
        ├── profile-drift.md        # drift report when comparable history exists
        ├── profile-hotspots.json   # aggregated hotspot metadata
        └── cases/
            └── <case-name>/
                ├── summary.md
                ├── detailed-observations.json
                └── profile.json.gz
```

### Drift thresholds

When comparable profiling history exists, drift reports flag significant changes:

- **Function drift**: at least 300 samples, at least 1.0% inclusive share, at least 2.0 percentage-point delta, and at least 20ms estimated delta.
- **Stage drift**: at least 5% change and at least 10ms absolute delta.
- **Counter drift**: at least 3% change with a meaningful absolute delta.

Drift is attribution evidence. It does not prove an optimization or regression.

### Rules

- Do not commit raw profiles, `profile-runs.jsonl`, or anything under `benchmarks/local-data/`.
- Profile evidence is attribution, not proof. Use benchmarks to validate or reject changes.
- Public summary rules under `benchmarks/summaries/` are unchanged by profiling.

## Adding Cases

Benchmark cases are end-to-end CLI measurements such as `check path` or `build path`. Frontend benchmark cases are in-process frontend measurements such as `frontend path`.

New cases should be valid programs or projects that exercise a distinct compiler or build-system path. Prefer one representative fixture over many near-duplicates.

Do not add negative diagnostic tests as benchmarks. Failure cases belong in `tests/cases/` where diagnostics can be asserted directly.

Project fixtures should commit only source inputs. Generated `dev/` and `release/` output directories are ignored and must not be committed.

Keep the public group list short. Use existing groups unless a new group gives clearly better summary readability.

Adversarial fixtures under `benchmarks/adversarial/` are compiler churn discovery workloads, not
public language examples. They should remain valid successful programs or projects, but they may
combine many surfaces in ways that are intentionally dense so profiling can expose frontend
allocation, lookup, folding, import, and lowering pressure.

## Fixture List

- `speed-test.bst`: broad baseline language and compiler exercise covering constant folding, templates, structs, receivers, collections, and control flow.
- `template-stress.bst`: deeply nested template composition, slot usage, `$children` wrappers, and formatter directive stress.
- `type-stress.bst`: type and method-heavy source with structs, choices, aliases, receivers, and constructor patterns.
- `fold-stress.bst`: constant folding coverage with large arithmetic trees, chained dependencies, and const record creation.
- `pattern-stress.bst`: pattern and match coverage including exhaustive choice arms, guards, payload capture, and relational patterns.
- `collection-stress.bst`: collection operations and loop coverage with mutations, range loops, nested iteration, and fallible fallback patterns.
- `environment-stress.bst`: AST environment building, type alias expansion, nominal structs and choices, receiver catalog construction, generic declarations and instantiations, and body validation/type resolution.
- `module-graph/`: small multi-file project with imports, facade exports, cross-file constants, and templates.
- `import-fanout/`: multi-file project with repeated imports, aliases, facade wrapper declarations, and cross-file constants for string-table interning and module-graph resolution.
- `external-js-imports/`: HTML project with annotated JavaScript imports, runtime helper imports, opaque external types, namespace imports, and external free functions.
- `borrow-stress.bst`: valid mutable/exclusive access and borrow-validation coverage.
- `adversarial/one-module-kitchen-sink.bst`: dense single-module churn across imports, constants,
  aliases, nominal types, choices, traits, generics, templates, collections, maps, receivers, and
  external package calls.
- `adversarial/deep-scope-churn.bst`: nested functions, control blocks, loop scopes, and local
  declaration pressure for scope-frame creation and ancestor lookup.
- `adversarial/template-render-plan-churn.bst`: nested template composition, slots, inserts,
  `$children` wrappers, repeated slot replay, and runtime template rebuilding.
- `adversarial/constant-dag-churn.bst`: large compile-time constant dependency DAGs, arithmetic
  folding, const records, and folded templates.
- `adversarial/expression-rpn-churn.bst`: expression parsing and RPN lowering pressure through
  choice matching, mutable stacks, checked operators, and value recovery.
- `adversarial/generic-trait-churn.bst`: generic structs/functions, trait declarations, explicit
  conformances, bound-provided receiver calls, and concrete instantiations.
- `adversarial/collection-map-borrow-churn.bst`: valid collection/map mutation, fallible
  operations, mutable receiver calls, and borrow-checker side-table pressure.
- `adversarial/import-external-churn/`: HTML project fixture with import fanout, cross-file
  constants/types/helpers, core package calls, and repeated external JavaScript free-function
  usage.

## What Not To Do

- Do not treat small timing changes as precise performance measurements.
- Do not add per-case tables to tracked summaries.
- Do not add raw counter dumps to tracked summaries.
- Do not add expensive counters that require new full-pipeline traversals without a targeted investigation.
- Do not treat counter movement as an optimization result unless timing moved meaningfully too.
- Do not compare CLI and frontend suite results manually as if they were the same metric.
- Do not commit `benchmarks/local-data/`, generated benchmark outputs, or old benchmark result folders.
- Do not add failing diagnostic cases to benchmark suites.
- Do not add many fixtures that stress the same path in slightly different ways.
