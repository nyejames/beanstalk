# Frontend Optimisation Results

This file records concise evidence for the frontend arena and semantic-invariant optimisation
programme. Raw benchmark history and profiler output stay local-only under
`benchmarks/local-data/`.

## Phase 0 Baseline - 2026-06-18

### Baseline Environment

- Commit: `c263aa6cd7b3703fd6f97dfac92e42012e233585`
- Branch: `main`
- Machine: macOS Apple Silicon benchmark host `6D851D`
- OS: macOS `14.6.1` build `23G93`; Darwin `23.6.0` ARM64
- CPU/memory: Apple M1 Pro, 10 physical CPU cores, 16 GiB memory
- Rust: `rustc 1.95.0 (59807616e 2026-04-14)`, host `aarch64-apple-darwin`, LLVM `22.1.2`
- Cargo: `cargo 1.95.0 (f2d3ce0bd 2026-03-21)`
- Just: `just 1.50.0`
- Samply: `0.13.1`

### Commands Run

- `just validate`
- `just bench-frontend-check`
- `just bench-check`
- `just bench-frontend` five recorded invocations
- `just bench` five recorded invocations, then five refreshed recorded invocations after the
  `template-stress.bst` fixture repair
- `just bench-report`
- `just profile-build`
- `samply record --save-only --output benchmarks/local-data/profiles/2026-06-18-docs-build.json.gz ./target/profiling/bean build docs --release`
- `samply record --save-only --iteration-count 5 --output benchmarks/local-data/profiles/2026-06-18-docs-build.json.gz ./target/profiling/bean build docs --release`
- `samply record --save-only --output benchmarks/local-data/profiles/2026-06-18-template-stress.json.gz ./target/profiling/bean check benchmarks/template-stress.bst`
- `samply record --save-only --output benchmarks/local-data/profiles/2026-06-18-environment-stress.json.gz ./target/profiling/bean check benchmarks/environment-stress.bst`

### Benchmark Suites

- End-to-end CLI suite: `benchmarks/cases.txt`, 16 cases across `core`, `docs`, `stress`,
  `module`, and `borrow`.
- Focused frontend suite: `benchmarks/frontend-cases.txt`, 8 cases across `core`, `docs`,
  `stress`, `module`, and `borrow`.
- Both suites use one warmup iteration and ten measured iterations per case.

The baseline repaired stale benchmark fixtures so the suites contain valid successful programs
under the current language rules: fallible collection `push` calls now use `catch`, the external
JS fixture uses free-function metadata instead of receiver-style JS annotations, the module-graph
fixture avoids a borrow alias, and the template-stress fixture uses the current default/named slot
routing shape.

Because some previous local records measured stale invalid fixtures, the first valid frontend
recorded run showed an apparent `+50ms` average movement. Treat that as a case-input correction,
not a compiler regression.

### Validation Status

- `just validate`: passed after the report and final fixture repairs. Clippy passed on native,
  Linux, and Windows targets; unit tests passed `2653/2653`; integration tests passed
  `1707/1707`; docs check passed; embedded `bench-check` passed with `+2ms avg`.
- `just bench-frontend-check`: passed on the final fixture set with `no measurable change:
  avg +3ms; 8/8 cases`.
- `just bench-check`: passed on the final fixture set with `0ms avg; 0 faster, 1 slower; 16/16 cases`.

### Latest Drilldown

`just bench-report` after the refreshed end-to-end runs:

- End-to-end CLI latest: `2026-06-18T10:08`, `no measurable change: avg -1ms; 16/16 cases`.
- Slowest end-to-end cases:
  - `check_docs`: about `185ms`; `ast_ms` about `902ms`, `ast_build_environment_ms` about `500ms`.
  - `check_benchmarks_type-stress_bst`: about `65ms`; `ast_ms` about `54ms`, `ast_build_environment_ms` about `42ms`.
  - `check_benchmarks_environment-stress_bst`: about `61ms`; `ast_ms` about `51ms`, `ast_build_environment_ms` about `39ms`.
- End-to-end stage movement: `file_prepare_ms -5ms`, `ast_ms +3ms`, `ast_finalize_ms +2ms`,
  `ast_emit_nodes_ms -1ms`; no counter movement.
- End-to-end top ratios: `fold-stress` has high `file_prepare_ms/source_file_count`;
  `type-stress` and `environment-stress` have high `ast_ms/ast_header_count`.

Focused frontend latest recorded drilldown:

- Frontend latest: `2026-06-18T01:22`, `no measurable change: avg 0ms; 8/8 cases`.
- Slowest focused frontend cases:
  - `frontend_docs`: about `437ms`; `ast_ms` about `58ms`, `ast_build_environment_ms` about `30ms`.
  - `frontend_benchmarks_type-stress_bst`: about `134ms`; `ast_ms` about `108ms`, `ast_build_environment_ms` about `83ms`.
  - `frontend_benchmarks_environment-stress_bst`: about `123ms`; `ast_ms` about `104ms`, `ast_build_environment_ms` about `80ms`.
- Frontend stage movement: `ast_ms +1ms`, `ast_build_environment_ms +1ms`; no counter movement.
- Frontend top ratios point at `type-stress`, `environment-stress`, and `collection-stress` for
  tokenization/header parsing/string-table merge-remap investigation.

Useful volume counters from the final end-to-end docs check:

- `source_file_count`: `125`
- `source_byte_count`: `527551`
- `token_count`: `41331`
- `header_count`: `1640`
- `import_count`: `850`
- `top_level_declaration_count`: `1365`
- `template_count`: `4776`
- `module_remap_string_ids_calls`: `31`
- `string_table_merge_source_entries_scanned`: `4918`

### Samply Result

The three local Samply profile files were produced, but each contains zero samples:

- `benchmarks/local-data/profiles/2026-06-18-docs-build.json.gz`
- `benchmarks/local-data/profiles/2026-06-18-template-stress.json.gz`
- `benchmarks/local-data/profiles/2026-06-18-environment-stress.json.gz`

These files are not useful for function-level hotspot attribution. Phase 1 should use the
benchmark stage/counter evidence as the baseline. A repeated docs profile with five command
iterations still produced zero samples, so future profiling should use a longer workload or a
different profiler setup only if function-level attribution is needed before a specific refactor.

### Baseline Findings

- AST construction, especially AST environment building, is the dominant compiler-stage signal in
  docs, `type-stress`, and `environment-stress`.
- File preparation remains worth watching in `fold-stress`, `type-stress`, `environment-stress`,
  and `collection-stress`, especially tokenization/header parsing/string-table merge-remap ratios.
- Current counter comparisons show no movement after the fixture repairs; the baseline is stable
  enough to start Phase 1 stats and capacity-estimate work.
- No compiler semantics changed in Phase 0. The committed fixture changes only make benchmark
  inputs conform to current documented language rules.
- Documentation drift noted: `benchmarks/README.md` still describes the external JS benchmark as
  covering external receiver methods. Current language rules and the repaired fixture expose
  external JS functions as free functions only.

### Semantic Invariants For Optimisation Review

- No visible shadowing: scope-frame arenas can use parent-linked frames and ancestor
  redeclaration checks instead of cloned shadow stacks.
- Header parsing owns top-level discovery: token/header counts are valid capacity seeds, and AST
  must not rediscover top-level declarations.
- Dependency sorting is authoritative: AST should not grow fixpoint ordering passes for constants,
  aliases, structs, choices, or signatures.
- Header-built visibility is authoritative: body-local scope frames should reference immutable
  header/import visibility instead of copying it into children.
- One entry start path: start-specific structures should be allocated only when a start header
  exists.
- Generics resolve before HIR: generic template storage can stay AST-local and should not leak
  unresolved generic calls into HIR.
- Traits are static metadata: evidence maps can be compact stable-ID tables; no runtime trait
  object metadata is needed.
- External packages expose free functions only: avoid external receiver-method catalogs and share
  immutable external metadata where ownership permits.
- Canonical `TypeId` is semantic identity: arena nodes should carry `TypeId`s and compact IDs, not
  cloned semantic type trees.
- No closures or general function values: function-local scope arenas do not need capture
  promotion for runtime function values.
- No macro expansion language: no hygiene or repeated parse/expand/fold arena is needed.
- Borrow validation is side-table based: future borrow-fact compaction should keep facts outside
  HIR nodes.

## Phase 1 Stats And Capacity Estimates - 2026-06-18

### Scope

Phase 1 added `src/compiler_frontend/arena/` as the frontend-local owner for cheap token/header
statistics and capacity-estimate policy:

- `TokenStats` is accumulated during the existing tokenizer loop and travels with `FileTokens` and
  `FileFrontendPrepareOutput`; it carries counts only, so string-ID remapping is unaffected.
- `HeaderStats` is computed from the already-aggregated module header list and module symbol
  package, including functions, constants, structs, choices, type aliases, traits, conformances,
  trait incompatibilities, const templates, start functions, imports, generic parameters,
  signature members, choice variants, and dependency edges.
- `FrontendArenaCapacityEstimate` centralizes conservative, capped estimates for scope frames,
  declarations, expressions, expression items, statements, templates, template atoms, render
  pieces, HIR blocks/statements/expressions, and borrow facts.
- Detailed-timer counters now report the estimated scope-frame count and capped-estimate count.
  Actual scope-frame and scope-arena-capacity counters intentionally remain zero until Phase 4
  creates real scope-frame arena storage.

Parent review corrected two estimate-quality details before acceptance: map/collection delimiters
now count curly braces and commas only, and trait requirement signatures contribute to
`HeaderStats.signature_members`.

### Validation Status

- Focused unit tests:
  - `cargo test --quiet token_stats`: passed, `4/4`.
  - `cargo test --quiet header_stats`: passed, `4/4`.
  - `cargo test --quiet capacity`: passed, `49/49`.
- `just bench-frontend-check`: passed at `2026-06-18T10:59`, `no measurable change: avg 0ms`;
  `8/8` cases, stage movement `ast +9ms`, `ast env +4ms`, `ast emit +3ms`.
- `just bench-check`: passed at `2026-06-18T11:01`, `+2ms avg`; `0 faster`, `2 slower`,
  `16/16` cases, stage movement `ast env +21ms`, `ast +21ms`, `file prep +11ms`.
- `just validate`: passed after Phase 1 corrections and plan/report updates. Clippy passed on
  native, Linux, and Windows targets; unit tests passed `2667/2667`; integration tests passed
  `1707/1707`; docs check passed; embedded `bench-check` passed with `+3ms avg`.

The benchmark movement is below the plan's rollback threshold and is consistent with noise for an
instrumentation-only slice. Stats and estimates remain policy-only; they do not affect diagnostics,
ordering, lowering, type identity, or emitted artifacts.

### Audit Notes

- Stage boundaries remain intact: token stats belong to tokenization output, header stats belong to
  header aggregation output, and capacity formulas live in `arena/capacity.rs` rather than pipeline
  orchestration.
- There is no new source/token traversal. Token stats are collected in the lexer loop; header stats
  are a cheap pass over already-aggregated headers after header parsing has completed.
- No dedicated output fixture was added because stats are not semantically consumed. Existing
  integration/golden validation is the regression owner for diagnostics and output equivalence.
- The progress matrix was not updated in this slice because no language feature support changed;
  the active plan still reserves roadmap/progress documentation work for Phase 9.

## Phase 2 Adversarial Benchmark Fixtures - 2026-06-18

### Scope

Phase 2 added `benchmarks/adversarial/` with seven single-file compiler-churn fixtures and one
small HTML project fixture:

- `one-module-kitchen-sink.bst` combines imports, constants, aliases, nominal types, choices,
  traits, generics, templates, collections, maps, receivers, and external package calls.
- `deep-scope-churn.bst` targets nested function/block/loop scope-frame pressure.
- `template-render-plan-churn.bst` targets slot routing, `$children` wrappers, repeated slot
  replay, and runtime template rebuilding.
- `constant-dag-churn.bst` targets compile-time dependency sorting and constant/template folding.
- `expression-rpn-churn.bst` targets expression parsing/lowering, choice matching, mutable stack
  operations, and checked arithmetic.
- `generic-trait-churn.bst` targets generic instantiation, trait evidence, and bound-provided
  receiver calls.
- `collection-map-borrow-churn.bst` targets valid collection/map mutation, fallible operations,
  receiver calls, and borrow-checker facts.
- `import-external-churn/` targets project import fanout, core package calls, and external
  JavaScript free-function metadata.

No generator was added. The initial adversarial set is clearer as hand-authored static source, and
the committed `.bst`/`.js` files are the canonical benchmark inputs.

### Validation Status

- `just bench-frontend-check`: passed at `2026-06-18T18:16`, expanding the focused frontend suite
  to `16` cases. The expected case-set change showed `avg +4ms` on the `8/16` shared cases, with
  stage movement `ast +10ms`, `ast env +7ms`, and `ast emit +3ms`.
- `just bench-check`: passed at `2026-06-18T18:16`, expanding the end-to-end suite to `25` cases.
  The expected case-set change showed `avg +2ms` on the `16/25` shared cases, with stage movement
  `ast -23ms`, `ast env -10ms`, and `file prep +10ms`.
- `just profile-case check_benchmarks_adversarial_one-module-kitchen-sink_bst terse`: the first
  sandboxed run reached Samply but failed with `Unknown(1100)`. Rerunning the same command with
  approved escalation passed and wrote local-only artifacts under
  `benchmarks/local-data/profiles/2026-06-18T18-22-55-d82ffd27/`.
- `just validate`: passed after Phase 2 docs and plan updates. Clippy passed on native, Linux, and
  Windows targets; unit tests passed `2667/2667`; integration tests passed `1707/1707`; docs check
  passed; embedded `bench-check` passed with the expected case-set change at `avg +3ms` on the
  `16/25` shared cases.

The targeted profile observed `one-module-kitchen-sink` at about `32ms` wall time. Stage attribution
pointed to `ast_ms` at about `24ms`, with `ast_build_environment_ms` about `17ms`,
`ast_emit_nodes_ms` about `6ms`, `borrow_ms` about `2ms`, and `file_prepare_ms` about `1ms`. The
profile captured only `50` samples and remained unsymbolicated, so the stack addresses are not
useful function-level evidence. The stage/counter observations are still useful for Phase 3 and
Phase 4 targeting.

Useful counters from the profile:

- `source_file_count`: `2`
- `source_byte_count`: `7280`
- `token_count`: `1665`
- `header_count`: `61`
- `top_level_declaration_count`: `58`
- `template_count`: `92`
- `const_template_count`: `55`
- `runtime_template_count`: `37`
- `ast_function_count`: `16`
- `ast_struct_count`: `7`
- `ast_choice_count`: `2`
- `ast_generic_template_count`: `4`
- `ast_generic_instance_count`: `2`
- `borrow_statement_fact_count`: `231`
- `borrow_value_fact_count`: `525`
- `estimated_scope_frames`: `107`

### Audit Notes

- The adversarial fixtures are successful programs/projects, not diagnostic cases.
- No `dev/` or `release/` generated project output folders were added.
- The new single-file fixtures live in the existing `stress` group; the multi-file external import
  project lives in the existing `module` group.
- `benchmarks/README.md` now records the adversarial fixture purpose and corrects the older
  external JS fixture description from receiver methods to external free functions.

## Phase 3 External Package Registry Clone Reduction - 2026-06-18

### Scope

Phase 3 replaced deep external-package registry clones through the frontend, AST environment,
`ScopeContext`, `Module`, and backend consumers with a shared immutable
`Arc<ExternalPackageRegistry>` handle. The registry remains mutable only during library setup,
project config parsing, and Stage 0 external import discovery; after discovery, compiled modules
and backend lowerers share the frozen registry snapshot.

This phase also added detailed-timer clone-pressure counters for:

- `external_package_registry_clone_count`
- `external_package_definition_clone_count`
- `external_function_definition_clone_count`
- `external_symbol_path_clone_count`
- `external_abi_parameter_clone_count`

Parent review adjusted the worker patch so ownership-carrying contexts own an `Arc`, while
read-only token/header preparation and backend validation call sites borrow the underlying
`ExternalPackageRegistry` through `.as_ref()`.

### Clone Counter Movement

The worker captured baseline counter values after adding counters and before the ownership
reduction, then reran the same import-heavy cases after the reduction. Parent targeted profiles
confirmed the reduced counts after review cleanup.

| Case | Registry | Package | Function | Symbol path | ABI parameter |
|---|---:|---:|---:|---:|---:|
| `external-js-imports` before | 75 | 675 | 14,625 | 31,531 | 32,850 |
| `external-js-imports` after | 1 | 9 | 195 | 451 | 438 |
| `import-external-churn` before | 161 | 1,288 | 31,073 | 67,014 | 69,874 |
| `import-external-churn` after | 1 | 8 | 193 | 454 | 434 |

The remaining registry clone is the config/build ownership boundary where the mutable builder
library registry is frozen into an immutable frontend snapshot. Definition/path/ABI clones that
remain are registration-time ownership inside registry maps, plus owned builder-runtime metadata
that still belongs to each module's backend handoff.

### Validation Status

- `cargo check`: passed after parent cleanup.
- `cargo test --quiet external_packages`: passed, `46/46`.
- `cargo test --quiet provider_registry_tests`: passed, `15/15`.
- `cargo run -- check benchmarks/external-js-imports`: passed.
- `cargo run -- build benchmarks/external-js-imports`: passed.
- `cargo run -- check benchmarks/adversarial/import-external-churn`: passed.
- `cargo run -- build benchmarks/adversarial/import-external-churn`: passed.
- `just bench-frontend-check`: passed at `2026-06-18T18:55`, `avg -47ms` on the `8/16`
  shared cases, with stage movement `ast -296ms`, `ast env -200ms`, and `ast emit -62ms`.
- `just bench-check`: passed at `2026-06-18T18:57`, `avg -14ms` on the `16/25` shared cases,
  with stage movement `ast -721ms`, `ast env -547ms`, and `ast emit -166ms`.
- `just profile-case check_benchmarks_external-js-imports terse`: passed with approved profiler
  escalation and wrote local-only artifacts under
  `benchmarks/local-data/profiles/2026-06-18T18-58-36-8e594d03/`.
- `just profile-case check_benchmarks_adversarial_import-external-churn terse`: passed with
  approved profiler escalation and wrote local-only artifacts under
  `benchmarks/local-data/profiles/2026-06-18T18-58-45-8e594d03/`.
- `just validate`: passed after Phase 3 parent cleanup and report updates. Clippy passed on
  native, Linux, and Windows targets; unit tests passed `2667/2667`; integration tests passed
  `1707/1707`; docs check passed; embedded `bench-check` passed with the expected case-set change
  at `avg -14ms` on the `16/25` shared cases.

The targeted profiles captured only `30` and `45` samples and remained unsymbolicated, so their
raw stack addresses are not useful function-level evidence. Their stage/counter observations are
useful: `external-js-imports` now checks at about `13ms` with `ast_ms` about `3ms`, and
`import-external-churn` checks at about `20ms` with `ast_ms` about `5ms`.

### Audit Notes

- External package metadata remains immutable after Stage 0 discovery and is not exposed through a
  mutable shared handle.
- Backends still validate and lower against the exact registry used by frontend resolution.
- No backend rediscovery path or duplicate package metadata path was introduced.
- The progress matrix was not updated in this slice because no language feature support changed;
  the active plan still reserves roadmap/progress documentation work for Phase 9.

## Phase 4 ScopeFrame Arena Refactor - 2026-06-18

### Scope

Phase 4 replaced the cloned flat local-declaration state in `ScopeContext` with a typed `Vec`
arena of parent-linked scope frames:

- `ScopeArena` owns `ScopeFrame` storage and creates stable `ScopeFrameId` handles.
- Child expression, template, constant, block, branch, and loop contexts allocate child frames with
  parent IDs instead of cloning all visible locals.
- Body-local functions allocate fresh root frames and receive parameters only, preserving the
  no-closures/no-implicit-capture language invariant.
- Local lookup now returns `ScopeDeclarationRef`, which hides whether the declaration is a local
  arena-owned `Rc<Declaration>` or a borrowed top-level declaration from immutable module lookups.
- `ScopeContext::clone()` creates a shallow copy of the current frame so match arms, value arms,
  and catch-helper contexts can add captures without mutating the original frame.
- Detailed counters now report actual scope frames, scope arena capacity growth, maximum frame
  depth, local declaration insertions, lookup ancestor steps, and redeclaration ancestor checks.

The old scope-local clone counter was removed because there is no remaining flat local-declaration
clone path. Capacity preallocation from `FrontendArenaCapacityEstimate` is intentionally deferred
to Phase 5. The new actual counters show the Phase 1 estimate formulas undercount scope-heavy
fixtures, so Phase 5 should tune formulas before using them as arena capacity seeds.

### Validation Status

- Worker validation before parent review:
  - `cargo fmt`: passed.
  - `cargo check`: passed.
  - `cargo clippy`: passed.
  - `cargo test --quiet scope_context`: passed.
  - `cargo test --quiet`: passed, `2677/2677`.
  - `cargo run -- check benchmarks/environment-stress.bst`: passed.
  - `cargo run -- check benchmarks/adversarial/deep-scope-churn.bst`: passed.
  - `just validate`: passed.
- Parent validation after review corrections:
  - `cargo fmt`: passed.
  - `cargo check`: passed.
  - `cargo test --quiet scope_context`: passed, `17/17`.
  - `cargo run -- check benchmarks/environment-stress.bst`: passed.
  - `cargo run -- check benchmarks/adversarial/deep-scope-churn.bst`: passed.
  - `git diff --check`: passed.
  - `just validate`: passed. Clippy passed on native, Linux, and Windows targets; unit tests
    passed `2677/2677`; integration tests passed `1707/1707`; docs check passed; embedded
    `bench-check` passed with `avg -14ms` on `16/25` shared cases and stage movement
    `ast -725ms`, `ast env -551ms`, `ast emit -167ms`.

### Benchmark Results

- `just bench-frontend-check`: passed at `2026-06-18T20:08`, `case set changed: avg -46ms`
  on the `8/16` shared cases, `0 slower`, `8 faster`. Stage movement:
  `ast -296ms`, `ast env -199ms`, `ast emit -62ms`.
- Five recorded `just bench-frontend` invocations:
  - first recorded Phase 4 run showed `case set changed: avg -46ms` on `8/16` shared cases,
    `0 slower`, `8 faster`, with stage movement `ast -297ms`, `ast env -199ms`,
    `ast emit -63ms`;
  - the next four runs reported `no measurable change` against the accepted Phase 4 run,
    confirming stable medians within the benchmark system's rough threshold.
- Five recorded `just bench` invocations:
  - first recorded Phase 4 end-to-end run showed `case set changed: avg -14ms` on `16/25`
    shared cases, `2 slower`, `12 faster`, with stage movement `ast -724ms`,
    `ast env -552ms`, `ast emit -166ms`;
  - the next four runs reported `no measurable change` against the accepted Phase 4 run.
- Latest `just bench-report` after the five-run sequence shows:
  - End-to-end latest: `no measurable change: avg 0ms; 25/25 cases`.
  - Frontend latest: `no measurable change: avg 0ms; 16/16 cases`.
  - Remaining next-investigation ratios point at file preparation for `fold-stress`, docs,
    `type-stress`, `constant-dag-churn`, and `environment-stress`, not at scope lookup.

### Targeted Profiles And Counters

Targeted profile artifacts are local-only:

- `just profile-case check_benchmarks_environment-stress_bst terse`:
  `benchmarks/local-data/profiles/2026-06-18T20-12-00-d0b0e10e/`.
- `just profile-case check_benchmarks_adversarial_deep-scope-churn_bst terse`:
  `benchmarks/local-data/profiles/2026-06-18T20-12-08-d0b0e10e/`.
- `just profile-case check_benchmarks_adversarial_one-module-kitchen-sink_bst terse`:
  `benchmarks/local-data/profiles/2026-06-18T20-12-15-d0b0e10e/`.

The profiles captured low sample counts and still reported raw addresses rather than useful
function names. Stage and counter observations were useful:

| Case | Wall | AST | Actual frames | Estimated frames | Arena capacity |
|---|---:|---:|---:|---:|---:|
| `environment-stress` | `~28ms` | `~13ms` | `254` | `165` | `768` |
| `deep-scope-churn` | `~15ms` | `~5ms` | `210` | `108` | `376` |
| `one-module-kitchen-sink` | `~17ms` | `~7ms` | `221` | `107` | `448` |

The estimate-vs-actual gap is the main Phase 5 input. It also confirms that real scope-frame
actuals are now observable instead of staying at zero.

### Audit Notes

- No user-visible shadowing rule changed. Focused tests cover parent lookup, same-frame duplicate
  lookup, ancestor redeclaration detection, visibility-gate inheritance, function child isolation,
  and clone-frame isolation.
- AST still consumes header-built visibility through `FileVisibility`; the refactor did not add
  import rediscovery.
- Scope arena internals remain local to `ast/module_ast/scope_context/`; pipeline and build
  orchestration only record capacity-policy estimates.
- Stage-local diagnostics still use existing `CompilerDiagnostic` paths. Source locations and
  labels remain owned by existing parser/type-resolution call sites.
- No compatibility wrapper for the old flat local-declaration fields remains.
- `Rc<RefCell<ScopeArena>>` is internal to the scope-context subsystem. Lookups return
  `ScopeDeclarationRef` so no borrow guard escapes into recursive parser code.

## Phase 5 Scope Arena Capacity Tuning - 2026-06-18

### Scope

Phase 5 tuned the scope-frame capacity estimate formula, added detailed estimate/actual ratio
counters, added `ScopeArena::with_capacity`, and seeded production AST scope arenas from the
module-level `FrontendArenaCapacityEstimate`. The seeding policy is AST-owned and spends the
module scope-frame estimate once across known root function, start, generic-template-validation,
and const-template parse contexts. Dynamic generic instances and direct AST helper callers remain
unseeded and grow normally.

### Benchmark Results

- Five recorded `just bench-frontend` invocations after production seeding reported no measurable
  regression. The latest run showed `no measurable change: avg -1ms; 16/16 cases`.
- Five recorded `just bench` invocations after production seeding reported no measurable
  regression. The latest run showed `no measurable change: avg 0ms; 25/25 cases`, with small stage
  movement of `ast +7ms`, `file prep +6ms`, and `ast finalize +3ms`.
- The tracked monthly summary was updated by the recorded benchmark runs. Raw benchmark history and
  profile artifacts remain local-only under `benchmarks/local-data/`.

### Targeted Profiles And Counters

Targeted `just profile-case ... terse` runs produced local-only profile directories for docs,
template stress, and import/module fixtures. Stack samples remain mostly unsymbolicated, so the
useful evidence is the observation-pass stage and counter data:

| Case | Wall | AST / Env / Emit | Estimated frames | Actual frames | Arena capacity | Estimate / actual | Capacity / actual |
|---|---:|---:|---:|---:|---:|---:|---:|
| `docs` | `~125ms` | `363 / 107 / 211ms` | `10630` | `4363` | `16590` | `2.44x` | `3.80x` |
| `template-stress` | `~27ms` | `10 / 5 / 4ms` | `467` | `279` | `811` | `1.67x` | `2.91x` |
| `module-graph` | `~15ms` | `4 / 2 / 2ms` | `221` | `129` | `413` | `1.71x` | `3.20x` |
| `import-fanout` | `~19ms` | `5 / 3 / 2ms` | `308` | `173` | `580` | `1.78x` | `3.35x` |
| `external-js-imports` | `~13ms` | `3 / 2 / 1ms` | `144` | `88` | `260` | `1.64x` | `2.95x` |
| `import-external-churn` | `~20ms` | `5 / 2 / 2ms` | `338` | `195` | `591` | `1.73x` | `3.03x` |

No under-estimates were observed in the Phase 5 evidence set. The current formula intentionally
lands on modest over-estimation for normal and adversarial fixtures. Capacity/actual ratios range
from about `2.9x` to `3.8x`, which is acceptable for the current policy because capacity remains
bounded and semantics-neutral, but future tuning should keep an eye on the docs path before
increasing scope-frame estimates further.

### Audit Notes

- Capacity formulas remain centralized in `src/compiler_frontend/arena/capacity.rs`.
- Capacity estimates remain policy-only. If estimates are too small, the arena grows normally; if
  they are too large, the effect is bounded extra `Vec` capacity.
- The Phase 5 evidence does not satisfy the broad Phase 6 entry criteria by itself. The latest
  reports point remaining investigation toward file preparation and docs AST emission rather than
  HIR dense storage or a clear expression scratch hotspot.

## Phase 6/7 Gate Evidence - 2026-06-18

### Scope

An Ollama worker ran the Phase 6/7 gate pass after Phase 5. The worker confirmed `bench-report`
still points at file preparation and docs/type/constant-DAG paths rather than expression or
template arenas, but nested `samply record` failed inside the Ollama/Codex process with
`Unknown(1100)`. Parent-side reruns of the same `profile-case` commands succeeded, so the failure
appears isolated to the nested worker environment rather than the benchmark fixtures.

### Targeted Profiles And Counters

Targeted profile artifacts are local-only:

- `just profile-case check_benchmarks_adversarial_expression-rpn-churn_bst terse`:
  `benchmarks/local-data/profiles/2026-06-18T22-00-34-67a55dd5/`.
- `just profile-case check_benchmarks_adversarial_template-render-plan-churn_bst terse`:
  `benchmarks/local-data/profiles/2026-06-18T22-00-43-67a55dd5/`.
- `just profile-case check_docs terse`:
  `benchmarks/local-data/profiles/2026-06-18T22-00-52-67a55dd5/`.

The profiles still emitted raw-address hotspots, so stage/counter observations remain the useful
signal:

| Case | Wall | AST / Env / Emit / Finalize | HIR | Borrow | Key counters |
|---|---:|---:|---:|---:|---|
| `expression-rpn-churn` | `~16ms` | `5.0 / 2.2 / 2.1 / 0.6ms` | `0.9ms` | `3.3ms` | `template_count=57`, `hir_statement_count=177`, `borrow_state_snapshot_count=493` |
| `template-render-plan-churn` | `~15ms` | `7.0 / 3.6 / 2.4 / 1.0ms` | `0.7ms` | `1.3ms` | `template_count=128`, `runtime_template_count=16`, `hir_statement_count=178` |
| `docs` | `~149ms` | `354.8 / 98.2 / 215.2 / 38.7ms` | `8.3ms` | `4.9ms` | `template_count=4776`, `const_template_count=4771`, `module_remap_string_ids_calls=31` |

### Gate Decision

- Phase 6 remains deferred. The expression churn fixture does not show expression parsing/RPN work
  as a dominant remaining cost, and there are no dedicated expression allocation or clone counters
  showing pressure.
- Phase 7 remains deferred as a broad arena migration. The template churn fixture is small, while
  docs shows meaningful AST emit/finalize work and large template counts. That supports narrower
  docs/template attribution before any render-plan arena conversion.
- Phase 8 remains gated by the same evidence posture: HIR and borrow timings are small in the
  targeted profiles, so dense HIR storage or borrow fact compaction is not the next optimization
  target.

The next optimization evidence should focus on the repeated `bench-report` signal: file
preparation, tokenization/header parsing, string-table merge/remap, and docs AST emit attribution.

## Phase 9 Documentation And Final Decisions - 2026-06-18

- `docs/compiler-design-overview.md` now records frontend arenas as stage/module-owned
  implementation details, with capacity estimates explicitly policy-only.
- `docs/src/docs/progress/#page.bst` tracks "Frontend Arena + Semantic Invariant Optimisation" as
  `Partial`: scope-frame arenas, capacity estimates, external package clone reduction, and
  adversarial fixtures are implemented; deeper expression/template/HIR arenas remain deferred.
- `benchmarks/README.md` records the five independent invocation protocol for optimization phase
  boundaries and keeps raw history/profile rules unchanged.
- Final decision for this optimization pass: keep the implemented scope/external clone work, keep
  the conservative scope capacity formulas, and defer broader arena migrations until a future
  profile shows a specific hotspot.
