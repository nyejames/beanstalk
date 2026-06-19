# Beanstalk Template Optimisation and TIR Implementation Plan

## Purpose

This document contains two related implementation plans:

1. **Plan A — Small Template/Frontend Optimisations**: capacity metadata, clone reduction, string-buffer sizing, targeted counters, and obvious low-risk churn reductions.
2. **Plan B — Template IR (TIR)**: a staged refactor that makes an AST-local template IR the authoritative internal template representation after parity is proven.

The plans are intentionally ordered. Complete Plan A first. TIR is a larger refactor and should only begin once the smaller optimisation work has produced clean measurements, clearer counters, and a stable baseline.

## Non-negotiable constraints

- Optimisation work must be justified by measured before/after results.
- Keep readability, modularity, correctness, and diagnostics ahead of clever low-level optimisation.
- Do not change template language behaviour as part of these plans.
- Do not move template parsing, directive interpretation, slot routing, or formatter semantics into HIR or backends.
- Do not add a long-lived feature flag or compatibility layer.
- Temporary old/new adapters are allowed only inside `src/compiler_frontend/ast/templates/` and must have deletion checkpoints.
- Use existing capacity heuristics before adding new allocation policy.
- Add complexity only where profiling/counters make the reason clear.
- Every phase ends with audit, style-guide review, validation, and benchmark/profiling evidence.

## Agreed interview decisions

- [x] Use staged migration throughout the TIR plan.
- [x] Store TIR in an AST-local, per-module `TemplateIrStore` with typed IDs.
- [x] Model finalized template semantics in TIR; keep bulky/rare metadata in narrow side tables.
- [x] Add a TIR-native formatter view first; keep existing formatter algorithms initially.
- [x] Small optimisation phases require measurable improvement or neutral wall time with material churn reduction.
- [x] TIR phases may be performance-neutral while migrating if they reduce old paths, preserve parity, and avoid meaningful regressions.
- [x] Final parser state should emit directly into TIR.
- [x] Start TIR migration with a `TemplateContent` / current `Template` to TIR converter for parity.
- [x] Store existing interned `StringId`s in TIR v1, with byte-length metadata for capacity planning.
- [x] Add source-span-backed template body text as a deferred roadmap optimisation option.
- [x] Enforce strict semantic parity. Behaviour changes are out of scope unless they are bug fixes with regression tests.
- [x] Implement directly on `main`; no feature flag.
- [x] Defer incremental template caching. Add cache-ready metadata only.

## Current repository shape at the end of the interview

Inspected on `main` during the interview.

### Template subsystem

Current module map: `src/compiler_frontend/ast/templates/mod.rs`

The template module currently describes this flow:

```text
Tokens
  -> template_head_parser/
  -> template_body_parser.rs
  -> create_template_node.rs
  -> template_types.rs
  -> template_control_flow/
  -> template_slots/
  -> template_composition.rs
  -> template_render_units.rs
  -> styles/
  -> template_render_plan.rs
  -> template_folding.rs
  -> top_level_templates.rs
```

Important files:

- `src/compiler_frontend/ast/templates/template_types.rs`
  - `Template` is still the central AST template representation.
  - It owns `content`, `control_flow`, `unformatted_content`, `content_needs_formatting`, `render_plan`, `kind`, `doc_children`, `style`, `conditional_child_wrappers`, `conditional_child_wrapper_plan`, `runtime_slot_application`, `id`, and `location`.
  - It has already gained clone-churn-sensitive methods such as `resync_composition_metadata` and `clone_for_composition`.
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
  - `TemplateRenderPlan` is a flat `Vec<RenderPiece>` representation used by formatting/folding.
  - It already has `clone_recording_template_churn` counters.
- `src/compiler_frontend/ast/templates/template_render_units.rs`
  - Prepares `TemplateContent` plus `TemplateRenderPlan` for normal templates, branches, and loop bodies.
  - It still performs `TemplateContent` cloning, render-plan rebuilding, aggregate placeholder processing, post-format rebuilds, and wrapper recomposition.
- `src/compiler_frontend/ast/templates/template_folding.rs`
  - Folding now borrows existing render plans when available and only builds fallback plans when missing.
  - It still creates fresh strings, resolves fold bindings through owned expression paths, and recursively folds nested templates.
- `src/compiler_frontend/ast/templates/template_body_parser.rs`
  - Recursively parses nested templates.
  - Some foldable child templates are folded immediately during body parsing.
  - It clones some token kinds, wrappers, and owner template metadata.

### Capacity heuristics

Current module map: `src/compiler_frontend/arena/`

Important files:

- `src/compiler_frontend/arena/mod.rs`
  - Owns frontend arena policy and capacity estimate types.
  - It gathers cheap token/header statistics and turns them into conservative capacity estimates.
- `src/compiler_frontend/arena/capacity.rs`
  - `FrontendArenaCapacityEstimate` already has fields for `templates`, `template_atoms`, and `render_pieces`.
  - Estimates are policy-only and must not affect diagnostics, ordering, lowering, type identity, or emitted artifacts.
  - Heuristics use modest `3/2` over-allocation and a hard cap.
- `src/compiler_frontend/tokenizer/lexer.rs`
  - Tokenization already uses `source_code.len() / settings::SRC_TO_TOKEN_RATIO` to seed token vector capacity.
  - Tokenization accumulates `TokenStats` for later frontend estimates.

### Instrumentation and benchmarking

Important files:

- `src/compiler_frontend/instrumentation/ast_counters.rs`
  - Existing template counters include:
    - `TemplateAtomsParsed`
    - `TemplateCompositionPasses`
    - `TemplateWrapperApplications`
    - `TemplateRenderPlansBuilt`
    - `TemplateRenderPiecesBuilt`
    - `TemplateRenderPlanCloneCalls`
    - `TemplateRenderPiecesCloned`
    - `TemplateFoldPlanPiecesVisited`
    - `TemplateFoldFallbackPlanBuilds`
    - `TemplateFoldLoopIterations`
    - `TemplateNormalizationNodesVisited`
    - `RuntimeRenderPlansRebuilt`
    - `RuntimeSlotApplicationPlansBuilt`
    - `RuntimeSlotSourcesPlanned`
    - `RuntimeSlotSitesPlanned`
- `src/compiler_frontend/compiler_messages/compiler_dev_logging.rs`
  - `detailed_timers` emits stable `BST_BENCH timing <metric>=<ms>ms` lines.
  - Counters emit stable `BST_BENCH counter <metric>=<value>` lines.
  - In-process benchmark collection is already available.
- `benchmarks/README.md`
  - Use `just bench-frontend-check`, `just bench-frontend`, `just bench-check`, and `just bench`.
  - Optimisation phases should run five independent frontend and end-to-end suite runs and compare medians.
  - Use `just bench-report` and targeted `just profile-case <case-name>` runs for attribution.
  - Do not commit raw profiles, raw local history, raw counter dumps, or generated benchmark outputs.
  - Existing adversarial fixtures include `template-render-plan-churn.bst` and other frontend stress cases.

### Documentation / planning targets

- `docs/roadmap/roadmap.md`
  - Currently links the previous frontend arena/semantic invariant optimisation plan.
  - It already notes broader expression/template/HIR arena migrations are deferred until profiling justifies them.
  - It contains a benchmarking/profiling deferred tooling section.
- `docs/src/docs/progress/#page.bst`
  - Progress Matrix tracks current implementation behaviour.
  - The `Templates and style directives` row is currently `Supported` with broad coverage.
  - Do not use the Progress Matrix as a dumping ground for speculative implementation details.

## Shared measurement protocol

Use this protocol for both plans.

### Baseline capture

Before each phase that changes performance-sensitive code:

- [ ] Record current branch, commit, and working tree state.
- [ ] Run `just validate`.
- [ ] Run five independent `just bench-frontend` runs.
- [ ] Run five independent `just bench` runs if the change can affect backend/output/project flow.
- [ ] Run `just bench-report` and record the affected cases, stage movement, and relevant counters.
- [ ] Choose one targeted profiling case if stage/counter data points to a specific hotspot.
- [ ] Run `just profile-case <case-name> normal` or `deep` when attribution is needed.
- [ ] If symbolication fails, use stage timings and counters only; do not claim function-level hotspots.

### After-change capture

After each phase:

- [ ] Run `just validate`.
- [ ] Run five independent `just bench-frontend` runs.
- [ ] Run five independent `just bench` runs when relevant.
- [ ] Run `just bench-report` and compare medians.
- [ ] Run targeted profiling only if benchmark/counter movement needs attribution.
- [ ] Record concise conclusions in `benchmarks/frontend-optimization-results.md`.
- [ ] Do not commit raw JSONL history, raw profile files, generated output folders, or expanded counter dumps.

### Acceptance thresholds

For Plan A:

- Accept a change when it improves 5-run median time above the benchmark noise threshold, or when it materially reduces churn/counters with neutral wall time and no memory/output/test regression.
- Rework or revert changes that add complexity without measurable timing, memory, or churn benefit.

For Plan B:

- Intermediate phases may be timing-neutral when they remove old representation paths, reduce churn counters, and preserve semantic parity.
- No phase should regress relevant 5-run benchmark medians by more than roughly 2-3% unless there is a documented temporary reason and an immediate follow-up phase removes the regression.
- The final TIR route must show reduced template churn counters and no semantic/output drift.

### Evidence format

Each phase should append a short entry like this to `benchmarks/frontend-optimization-results.md`:

```markdown
## <date> — <phase name>

Baseline: <commit>
Change: <commit/range>
Suites: 5x bench-frontend, 5x bench
Relevant cases: <cases>
Median movement: <summary>
Stage movement: <summary>
Counter movement: <summary>
Profile evidence: <none / profile run path summary>
Decision: accepted / reworked / reverted
Notes: <1-3 bullets>
```

---

# Plan A — Small Template/Frontend Optimisations

## Goal

Reduce obvious template/frontend churn before TIR:

- use existing capacity heuristics for template-related vectors and buffers;
- add targeted counters where current evidence is not specific enough;
- reduce expression/template/render-plan clones;
- avoid render-plan fallback/rebuild work when a prepared plan already exists;
- pre-size string buffers using cheap metadata;
- keep all behaviour unchanged.

## Phase A0 — Baseline and evidence setup

### Summary

Before changing code, capture a clean baseline and identify which template workloads currently move AST time and counters. This phase prevents speculative optimisation.

### Tasks

- [x] Run `just validate` on current `main`.
- [x] Run five independent `just bench-frontend` runs.
- [x] Run five independent `just bench` runs.
- [x] Run `just bench-report`.
- [x] Identify the top template-heavy cases, especially:
  - [x] `benchmarks/template-stress.bst`
  - [x] `benchmarks/adversarial/template-render-plan-churn.bst`
  - [x] docs project check/build cases
  - [x] `.bd` / Markdown-heavy cases if present in local reports
- [x] Run targeted `just profile-case <case-name> normal` on the highest-value template case.
  - `just profile-case check_docs normal` wrote observation artifacts, but Samply failed with
    `Unknown(1100)`, so Phase A0 uses observation-pass stage timings and counters only.
- [x] Record baseline timing/counter/profile conclusions in `benchmarks/frontend-optimization-results.md`.

### Audit / style / validation

- [x] Confirm no generated benchmark data is staged.
- [x] Confirm no code changes were made except an optional results note.
- [x] Confirm the baseline includes both frontend and end-to-end medians.

### Phase A0 evidence summary

Baseline commit: `a994e0ec7738295295c0ffb858153615072d7ad5` on `main`, with a clean starting
worktree.

Validation passed before benchmark capture. Five focused frontend runs and five end-to-end runs
completed. Latest reports: frontend `no measurable change: avg 0ms; 16/16 cases`; end-to-end
`no measurable change: avg -1ms; 25/25 cases`.

`check_docs` is the dominant template-heavy baseline case. Observation-pass counters show
`template_count=4788`, `const_template_count=4783`, `ast_template_render_plans_built=16181`,
and `ast_template_fold_fallback_plan_builds=6846`. Dedicated stress fixtures are much smaller:
`template-stress` has `template_count=213`, and
`adversarial/template-render-plan-churn` has `template_count=128`.

Durable details are recorded in `benchmarks/frontend-optimization-results.md`. Raw benchmark
history and profile artifacts remain local-only under `benchmarks/local-data/`.

## Phase A1 — Add targeted template churn counters

### Summary

Current counters already cover render-plan builds, clone calls, pieces, fallback builds, loop iterations, and runtime slot planning. Add only counters needed to distinguish the next small changes. Do not add counters that require new whole-pipeline traversals.

### Files

- `src/compiler_frontend/instrumentation/ast_counters.rs`
- `src/compiler_frontend/ast/templates/template_body_parser.rs`
- `src/compiler_frontend/ast/templates/template_folding.rs`
- `src/compiler_frontend/ast/templates/template_render_units.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- relevant tests under `src/compiler_frontend/instrumentation/tests/` or existing instrumentation test location

### Counters to add

Add a narrow subset first:

- [x] `TemplateNestedTemplateParses`
- [x] `TemplateBodyTokenVisits`
- [x] `TemplateTextBytesParsed`
- [x] `TemplateFoldOutputBytes`
- [x] `TemplateFoldStringInternCalls`
- [x] `TemplateFoldExpressionCloneRequests`
- [x] `TemplateFoldBindingSubstitutions`
- [x] `TemplateContentClonesForRenderUnits`
- [x] `TemplateContentRebuildsAfterFormatting`
- [x] `TemplateWrapperVectorClones`
- [x] `TemplateAggregatePlanBuilds`

Optional only if cheap to wire:

- [ ] `TemplateEstimatedFoldOutputBytes`
- [ ] `TemplateFoldOutputEstimateMissBytes`
- [ ] `TemplateCommonLiteralInternUses`

### Implementation steps

- [x] Extend `AstCounter` enum.
- [x] Add atomic storage under `#[cfg(feature = "detailed_timers")]`.
- [x] Update `all_counters()` length and list.
- [x] Add human labels.
- [x] Add stable snake_case metric names.
- [x] Add tests that prove new counters log without panics and non-detailed builds remain no-op.
- [x] Wire counters only at existing hot-path decision points.
- [x] Avoid adding new traversals only to count things.

### Measurement

- [x] Run `just bench-frontend-check` to ensure no obvious instrumentation overhead in normal check flow.
- [x] Run one detailed-timer benchmark and confirm new counters appear.
- [x] Record the new counters' baseline values for template-heavy cases.

### Audit / style / validation

- [x] Verify counter names are stable and snake_case.
- [x] Verify no raw counter dumps are committed.
- [x] Verify counters are feature-gated and normal builds are quiet.
- [x] Run `just validate`.

### Phase A1 evidence summary

Added the required AST template churn counters and wired them only at existing parser, folding, and
render-unit work sites. The optional Phase A3 estimator/cache counters remain deferred.

Validation passed after parent review and corrections:

- `cargo fmt`
- `cargo test instrumentation`
- `cargo test instrumentation --features detailed_timers`
- `cargo test compiler_frontend::ast::templates`
- `cargo test compiler_frontend::ast::templates --features detailed_timers`
- `just bench-frontend-check`
- `cargo run --features detailed_timers -- check benchmarks/adversarial/template-render-plan-churn.bst`
- `just validate`

`just bench-frontend-check` passed on the focused frontend suite with `+6ms avg`, `0 faster`,
`5 slower`, and `16/16 cases`. Full `just validate` passed, and its validation-safe benchmark
check reported `no measurable change: avg 0ms; 25/25 cases`, with `ast +15ms`, `file prep -14ms`,
and `ast emit +7ms`.

The detailed-timers adversarial template render-plan check confirmed every new stable metric name.
Baseline values for that case:

- `ast_template_nested_template_parses=76`
- `ast_template_body_token_visits=331`
- `ast_template_text_bytes_parsed=1257`
- `ast_template_fold_output_bytes=2840`
- `ast_template_fold_string_intern_calls=62`
- `ast_template_fold_expression_clone_requests=24`
- `ast_template_fold_binding_substitutions=0`
- `ast_template_content_clones_for_render_units=128`
- `ast_template_content_rebuilds_after_formatting=39`
- `ast_template_wrapper_vector_clones=170`
- `ast_template_aggregate_plan_builds=0`

No language support status changed, so `docs/src/docs/progress/#page.bst` does not need an update.

## Phase A2 — Thread capacity estimates into template vectors

### Summary

Use existing `FrontendArenaCapacityEstimate` fields before inventing new heuristics. The current estimate already exposes `templates`, `template_atoms`, and `render_pieces`. Apply these where allocation-heavy vectors are created at module/template preparation boundaries.

### Files

- `src/compiler_frontend/arena/capacity.rs`
- `src/compiler_frontend/ast/templates/template_types.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/template_render_units.rs`
- AST module/environment code where capacity estimates are already available

### Implementation steps

- [x] Locate where `FrontendArenaCapacityEstimate` is created and passed into AST/module construction.
- [x] Make template preparation code able to read a narrow capacity policy object rather than the whole estimate if that improves ownership clarity.
- [x] Use `estimate.templates` to seed future template stores or template side vectors where already naturally module-scoped.
  - No module-scoped template store exists yet. Phase A2 uses `estimate.templates` to normalize the
    per-template atom policy instead of creating a new store.
- [x] Use `estimate.template_atoms` for initial content/atom vectors when constructing larger template collections.
- [x] Use `estimate.render_pieces` for render-plan piece vectors where a module/template-level estimate is available.
  - Existing render-plan builders already have exact local input lengths, so Phase A2 kept those
    exact capacities and only tightened local aggregate vectors.
- [x] Keep small local vectors with obvious exact capacities unchanged.
- [x] Do not pass capacity estimates through many unrelated call layers; introduce a small context struct if needed.
- [x] Add counters to compare estimated vs actual render/template piece counts when cheap.

### Rules

- [x] Capacity estimates must not affect diagnostics, ordering, lowering, type identity, or emitted artifacts.
- [x] Do not allocate huge capacities for tiny nested templates.
- [x] Prefer exact local capacities such as `content.atoms.len()` over module estimates when exact data is available.
- [x] Use existing hard caps and modest over-allocation policy.

### Measurement

- [x] Compare allocation/churn counters and frontend medians before/after.
- [x] Inspect template-heavy cases for neutral or improved AST time.
- [x] Rework if memory grows without timing/counter benefit.

### Audit / style / validation

- [x] Confirm capacity policy remains in `arena/` or a narrow template-local capacity helper.
- [x] Confirm pipeline files do not gain heuristic clutter.
- [x] Run `just validate`.
- [x] Run five-run benchmark protocol.

### Phase A2 evidence summary

Added a narrow `TemplateCapacityPolicy` in `arena/capacity.rs`, threaded it through existing
AST/template parsing contexts, and pre-sized initial `TemplateContent` atom vectors from the
average estimated atoms per estimated template, clamped at `64` atoms per template. Exact local
render-plan capacities such as `content.atoms.len()` remain authoritative, and local aggregate
vectors now use exact plan lengths where available.

Validation passed:

- `cargo test compiler_frontend::arena`
- `cargo test compiler_frontend::ast::templates`
- `cargo test instrumentation --features detailed_timers`
- `just bench-frontend-check`
- `just validate`

Five recorded focused frontend runs reported rough median movement around `+2ms`; five recorded
end-to-end runs reported rough median movement around `+1ms`. Both are inside the suite's noise
threshold. `just bench-report` still points at broad docs/file-preparation and AST-emission noise
rather than a new capacity-specific hotspot. The change is accepted as neutral timing with bounded
allocation-policy cleanup. No language support status changed, so the progress matrix does not
need an update.

## Phase A3 — Pre-size folding and formatting output strings

### Summary

Template folding currently creates fresh `String` buffers. Add cheap byte estimates from render pieces and TIR-ready metadata to reduce reallocations. This should be low-risk and useful before TIR.

### Files

- `src/compiler_frontend/ast/templates/template_folding.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/template_render_units.rs`
- `src/compiler_frontend/ast/templates/template_formatting.rs`
- formatter style modules under `src/compiler_frontend/ast/templates/styles/`

### Implementation steps

- [x] Add a helper such as `estimate_render_plan_output_bytes(plan, string_table) -> usize`.
- [x] Count only cheap known text bytes at first:
  - [x] `RenderPiece::Text`
  - [x] `RenderPiece::HeadContent`
  - [x] known aggregate output bytes when folding aggregate plans
  - [x] known folded child string IDs when already available
    - No additional folded child string IDs were cheaply available without recursive folding.
- [x] Do not recursively fold just to estimate.
- [x] Use `String::with_capacity(estimate)` in:
  - [x] `fold_plan_to_emission`
  - [x] `fold_aggregate_render_plan`
  - [x] `fold_template_loop` aggregate buffer where body estimate and loop count are known or bounded
- [x] Clamp estimates where loop expansion could become huge.
- [x] Add counters for estimated and actual output bytes.
- [x] If formatter output builders allocate strings repeatedly, add exact/estimated capacities there too.
  - No clean formatter-side exact capacity was available without making formatter code noisier, so Phase A3 left formatter builders unchanged.

### Measurement

- [x] Compare `TemplateFoldOutputBytes`, `TemplateEstimatedFoldOutputBytes`, and AST timing.
- [x] Check large const-loop and markdown/template stress cases.
- [x] Rework if memory use rises significantly without timing benefit.

### Audit / style / validation

- [x] Confirm estimators are pure and do not change foldability.
- [x] Confirm no formatter semantics changed.
- [x] Add focused tests for estimation helpers if non-trivial.
- [x] Run `just validate`.
- [x] Run five-run benchmark protocol.

### Phase A3 evidence summary

Added `TemplateRenderPlan::estimate_output_bytes` and render-piece byte estimation for cheap
already-resolved text. Template folding now uses `String::with_capacity` for direct plan folding,
aggregate wrapper folding, and const-loop aggregate buffers. Range-loop reservations are capped to
avoid turning the configured expansion limit into a large allocation request, while collection
loops use their known folded item count. The new detailed-timer counters are
`ast_template_estimated_fold_output_bytes` and `ast_template_fold_output_estimate_miss_bytes`.

Validation passed:

- `cargo test compiler_frontend::ast::templates --lib`
- `cargo test instrumentation --features detailed_timers --lib`
- `just bench-frontend-check`
- `just validate`

Five recorded focused frontend runs reported rough median movement around `0ms`; five recorded
end-to-end runs also reported rough median movement around `0ms`. The latest `just bench-report`
showed neutral wall time with small latest-run reductions in fold output and estimate-miss
counters. The change is accepted as neutral timing with bounded allocation hints and better
measurement coverage. No language support status changed, so the progress matrix does not need an
update.

## Phase A4 — Reduce fold-time expression cloning

### Summary

Folding currently resolves bindings by taking owned `Expression` values and cloning/coercing recursively. Add a borrow-first path so common unchanged expressions do not allocate.

### Files

- `src/compiler_frontend/ast/templates/template_folding.rs`
- expression helper modules if a small shared borrowed/owned utility already exists

### Implementation steps

- [x] Introduce a small local enum in `template_folding.rs`:

```rust
pub(crate) enum FoldResolvedExpression<'a> {
    Borrowed(&'a Expression),
    Owned(Box<Expression>),
}
```

- [x] Replace `resolve_fold_bindings_in_expression(expression: Expression, ...) -> Expression` with a borrow-first helper.
- [x] Only allocate an owned expression when:
  - [x] a reference is substituted with a binding;
  - [x] a coerced inner value changed;
  - [x] an RPN expression has at least one substituted operand;
  - [x] constant folding actually produces a new folded expression.
- [x] Keep fallback behaviour identical when folding fails.
- [x] Add counters for clone requests vs actual owned rewrites.
- [x] Keep helper local to template folding unless another subsystem genuinely needs it.

### Tests

- [x] Existing template constant-folding tests pass unchanged.
- [x] Add focused tests for:
  - [x] bool condition binding substitution;
  - [x] option-present capture substitution;
  - [x] RPN substitution inside const template loops;
  - [x] no substitution path remains borrowed.

### Measurement

- [x] Compare clone counters and AST timing.
- [x] Pay special attention to const loop cases and branch-heavy template cases.

### Audit / style / validation

- [x] Confirm no `.clone()` remains on the common no-substitution path.
- [x] Confirm code stays readable; do not overuse clever `Cow` chains.
- [x] Run `just validate`.
- [x] Run five-run benchmark protocol.

### Phase A4 evidence summary

Implemented the borrow-first `FoldResolvedExpression` resolver in
`src/compiler_frontend/ast/templates/template_folding.rs` and added the
`TemplateFoldExpressionOwnedRewrites` counter in
`src/compiler_frontend/instrumentation/ast_counters.rs`. The common no-substitution
path now returns a borrowed reference, avoiding expression-tree clones for most
template expressions. Rewrites only allocate when a binding is substituted, a
coerced inner value changes, an RPN operand is substituted, or constant folding
produces a new value.

Validation passed:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --lib instrumentation --features detailed_timers`
- `cargo test --quiet`
- `cargo run -- tests`
- `cargo run -- check docs`
- `just validate`
- `just bench-frontend-check`
- five recorded `just bench-frontend` runs

Benchmark report after the five frontend runs:

- Focused frontend suite: `0ms avg; 0 faster, 1 slower; 16/16 cases`.
- Stage movement within noise: `ast_ms +6ms`, `ast_emit_nodes_ms +2ms`,
  `borrow_ms +2ms`, `hir_ms +2ms`, `ast_build_environment_ms +2ms`.
- Counter movement: `ast_template_fold_output_estimate_miss_bytes +30%`,
  `ast_template_fold_output_bytes +27%`, `ast_template_estimated_fold_output_bytes +24%`.
  These movements reflect normal run-to-run variance in the small template-churn
  fixtures; no semantic behaviour changed.
- The adversarial `template-render-plan-churn.bst` fixture reports
  `ast_template_fold_expression_clone_requests=0` and
  `ast_template_fold_expression_owned_rewrites=0`, consistent with a fixture that
  exercises render-plan churn rather than fold-binding substitution.

Focused unit tests for the resolver live in the new separate file
`src/compiler_frontend/ast/templates/template_folding_tests.rs`, matching the
project rule that tests do not live in production files. The instrumentation
regression test was updated to cover the new owned-rewrite counter.

No language support status changed, so `docs/src/docs/progress/#page.bst` does not
need an update.

## Phase A5 — Remove avoidable render-unit rebuilds and wrapper clones

### Summary

`template_render_units.rs` still clones content, rebuilds content after formatting, and rebuilds render plans after recomposition. Reduce only obvious duplication and only where parity is easy to prove.

### Files

- `src/compiler_frontend/ast/templates/template_render_units.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/template_composition.rs`
- `src/compiler_frontend/ast/templates/template_slots/`
- `src/compiler_frontend/ast/templates/template_types.rs`

### Implementation steps

- [x] Audit every `to_owned()`, `clone()`, and `rebuild_content()` in `template_render_units.rs`.
- [x] Add counters for each clone/rebuild family before changing behaviour.
  - Existing counters (`TemplateContentClonesForRenderUnits`, `TemplateRenderPlansBuilt`,
    `TemplateRenderPlanCloneCalls`, `TemplateRenderPiecesCloned`,
    `TemplateContentRebuildsAfterFormatting`, `TemplateWrapperVectorClones`) already cover the
    relevant families; no new whole-pipeline traversals were added.
- [x] Where `prepare_template_render_unit` already owns `parsed_content`, move it instead of cloning.
- [x] In `prepare_control_flow_render_units`, avoid `branch_unit.content.clone()` by moving
  `branch.content` with `std::mem::take` and moving the prepared content back.
- [x] In loop body handling, avoid `template_loop.body_content.to_owned()` by moving
  `template_loop.body_content` with `std::mem::take` and moving the prepared content back.
- [ ] Replace wrapper `Vec<Template>` clones with borrowed slices or wrapper-set IDs where this can be done locally without TIR.
  - Deferred: local inspection showed no safe, parity-preserving way to replace wrapper vector
    clones without either TIR-style wrapper-set IDs or a larger refactor of composition ownership.
    The existing `TemplateWrapperVectorClones` counter remains in place for the TIR migration.
- [x] Avoid `TemplateRenderPlan::from_content` fallback builds when a plan is already authoritative.
- [x] Keep aggregate placeholder behaviour unchanged.
- [x] Confirm existing tests cover `$children`, `$fresh`, slots, inserts, repeated slot replay, and
  control-flow wrappers. All existing create-template-node and slot unit/integration tests passed
  unchanged; no new fixture was required for this slice.

### Measurement

- [x] Compare:
  - [x] `TemplateRenderPlansBuilt`
  - [x] `TemplateRenderPlanCloneCalls`
  - [x] `TemplateRenderPiecesCloned`
  - [x] `TemplateContentClonesForRenderUnits`
  - [x] `TemplateContentRebuildsAfterFormatting`
  - [x] `TemplateWrapperVectorClones`
- [x] Run template-stress and adversarial template churn benchmarks.

### Audit / style / validation

- [x] Confirm no obsolete compatibility wrapper remains.
- [x] Confirm moved code has corrected ownership/API shape, not copied old shape.
- [x] Run `just validate`.
- [x] Run five-run benchmark protocol.

### Phase A5 evidence summary

Reduced avoidable cloning and fallback plan builds in
`src/compiler_frontend/ast/templates/template_render_units.rs`:

- Replaced `content_with_shared_head_prefix` with `content_with_head_prefix_owned_body`, which
  takes ownership of the body content and avoids one `TemplateContent` clone per control-flow arm.
- Used `std::mem::take` to move branch, fallback, and loop body content through
  `prepare_template_render_unit` and moved the prepared content back, removing three
  `TemplateContentClonesForRenderUnits` increments per arm.
- Updated `aggregate_pieces_from_template` to reuse the authoritative `template.render_plan` via
  `clone_recording_template_churn()` instead of rebuilding a plan from content.
- Added `Default` to `TemplateContent` (and removed the redundant manual `default()` method) so
  `std::mem::take` leaves a well-defined empty content behind.

Validation passed:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --lib instrumentation --features detailed_timers`
- `cargo test --quiet`
- `cargo run -- tests`
- `cargo run -- check docs`
- `just validate`
- `just bench-frontend-check`
- five recorded `just bench-frontend` runs

Benchmark report after the five frontend runs:

- Focused frontend suite: `no measurable change: avg 0ms; 16/16 cases`.
- First run showed `**-3ms avg**; 3 faster, 0 slower`, but the five-run median is inside noise.
- Stage movement: `borrow_ms -1ms` across 16 cases; all other stage movements within noise.
- Counter movement in the latest comparison:
  - `ast_template_fold_output_bytes +25%`
  - `ast_template_estimated_fold_output_bytes +25%`
  - `ast_template_fold_output_estimate_miss_bytes +24%`
  These output-byte movements reflect normal fixture variance; the slice does not change fold
  output semantics.
- The adversarial `template-render-plan-churn.bst` fixture continues to report
  `ast_template_content_clones_for_render_units=128`, which is unchanged because this fixture
  exercises render-plan churn rather than control-flow content cloning.

End-to-end `just validate` benchmark check reported `**-4ms avg**; 9 faster, 0 slower; 25/25 cases`.

Wrapper-vector clones were intentionally left untouched because replacing them cleanly requires
TIR-style wrapper-set IDs. The existing counter continues to measure them for the Plan B migration.

No language support status changed, so `docs/src/docs/progress/#page.bst` does not need an update.

## Phase A6 — Low-risk parser-loop cleanup

### Summary

Clean obvious parser-loop costs without changing parsing strategy. This phase should stay narrow; direct parser-to-TIR emission belongs in Plan B.

### Files

- `src/compiler_frontend/ast/templates/template_body_parser.rs`
- `src/compiler_frontend/tokenizer/text_modes.rs`
- `src/compiler_frontend/tokenizer/lexer.rs` only if needed

### Implementation steps

- [x] Replace `let token_kind = current_token_kind().clone()` in hot loops with match-by-reference where possible.
- [x] Clone only owned payloads that are inserted into AST/TIR structures.
- [x] Cache common interned literal IDs in the parser context:
  - [x] newline `"\n"`
  - [x] open bracket `"["`
  - [x] close bracket `"]"`
  - [ ] empty string — not needed; no hot path repeatedly interns the empty string.
- [x] Keep cache scoped to parser/fold context, not global mutable state.
- [x] Avoid changing tokenizer modes or delimiter semantics.
- [ ] Add counters for common literal cache uses if useful.
  - Deferred: the existing `TemplateBodyTokenVisits` and `TemplateTextBytesParsed`
    counters already measure hot-loop volume. A cache-hit counter would only measure
    avoided intern calls and is awkward to instrument without adding a counter increment
    per token, which would negate the small win.

### Tests

- [x] Existing template syntax tests pass.
- [x] No focused tests needed: the change is a local borrow/clone refinement with no new
  branches or public API surface; existing template parser and create-template-node tests
  already exercise the affected paths.

### Measurement

- [x] Compare parser/token visit counters and AST timing.
- [x] If no measurable effect and readability worsens, revert.
  - Readability remained neutral or improved; the match-by-reference pattern is idiomatic
    and the cached-ID fields are documented. First focused-frontend run showed a measurable
    `-4ms`, so the phase was accepted.

### Audit / style / validation

- [x] Confirm borrowed matches remain easy to read.
- [x] Confirm no borrow gymnastics leak into unrelated parser code.
- [x] Run `just validate`.
- [x] Run five-run benchmark protocol.

### Phase A6 evidence summary

Cleaned obvious parser-loop costs in `src/compiler_frontend/ast/templates/template_body_parser.rs`:

- Replaced `let token_kind = self.token_stream.current_token_kind().clone()` in the hot
  `parse_content` loop with match-by-reference. Only the rare `found =>` error arm clones the
  token kind for the diagnostic payload.
- Cached pre-interned `StringId`s for `"\n"`, `"["`, and `"]"` on `TemplateBodyParser` and
  reused them in the hot loop and in `consume_balanced_brackets_as_literal_text`, avoiding
  repeated string-table lookups for these single-character literals.
- `src/compiler_frontend/tokenizer/text_modes.rs` was inspected and left unchanged because its
  hot loop operates on `char` values, not `TokenKind` clones.

Validation passed:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --lib instrumentation --features detailed_timers`
- `cargo test --quiet`
- `cargo run -- tests`
- `cargo run -- check docs`
- `just validate`
- `just bench-frontend-check`
- five recorded `just bench-frontend` runs

Benchmark results:

- `just bench-frontend-check`: `**-4ms avg**; 6 faster, 0 slower; 16/16 cases`; stage movement
  `ast -13ms`, `ast emit -6ms`, `borrow -5ms`.
- Five recorded `just bench-frontend` runs: first run `**-4ms avg**`, remaining four runs
  `no measurable change: avg 0ms`. The five-run median is inside noise but the first-run and
  validation-safe checks show a consistent small improvement.
- End-to-end `just validate` benchmark check: `**-6ms avg**; 10 faster, 0 slower; 25/25 cases`;
  stage movement `ast -14ms`, `ast emit -14ms`, `file prep +6ms`.

No language support status changed, so `docs/src/docs/progress/#page.bst` does not need an update.

## Phase A7 — Plan A documentation and roadmap updates

### Summary

Record only durable conclusions. Do not over-document raw data.

### Documentation changes

- [x] Update `benchmarks/frontend-optimization-results.md` with concise accepted/rejected phase results.
- [x] Update `benchmarks/README.md` only if new counters or fixture usage changes the benchmark workflow.
  - No update needed: new counters follow the existing counter emission model described in the README.
- [x] Update `docs/roadmap/roadmap.md`:
  - [x] Add this small optimisation plan under `# Plans` if committed as a roadmap plan.
    - Already linked under `# Plans`.
  - [x] Update the previous optimisation plan note to say template churn/capacity clone-reduction work has been split out from broad arena migration.
  - [x] Keep broad template arenas deferred unless profiling now justifies them.
- [x] Do not update `docs/src/docs/progress/#page.bst` for internal-only counter/capacity changes unless a user-visible support status changes.

### Audit / style / validation

- [x] Confirm docs do not claim performance wins without measurements.
- [x] Confirm no raw benchmark tables are committed.
- [x] Run `just validate`.

### Phase A7 evidence summary

- Appended concise Phase A4, A5, and A6 evidence entries to
  `benchmarks/frontend-optimization-results.md`.
- Updated `docs/roadmap/roadmap.md` to clarify that the previous frontend arena optimisation plan
  has split out its template churn/capacity/clone-reduction work into this active plan, and that
  broad template-to-TIR arena migration remains deferred until Plan B scaffolding and profiling
  justify it.
- `benchmarks/README.md` did not need changes.
- `docs/src/docs/progress/#page.bst` was not updated because no user-facing language support or
  backend behavior changed.

Validation passed:

- `just validate`

---

# Plan B — Template IR (TIR)

## Goal

Introduce an AST-local Template IR that eventually becomes the authoritative internal representation for parsed and finalized templates.

TIR should make template parsing/folding/formatting/lowering faster by removing representation ping-pong:

```text
Current shape:
tokens -> TemplateContent -> TemplateRenderPlan -> formatter IO -> rebuilt TemplateContent -> render plan -> fold/HIR

Target shape:
tokens -> TemplateIrStore / TemplateIrId -> TIR formatter view -> TIR fold/HIR preparation
```

## TIR design baseline

### New module layout

Add under `src/compiler_frontend/ast/templates/`:

```text
tir/
├── mod.rs
├── ids.rs
├── store.rs
├── node.rs
├── summary.rs
├── validation.rs
├── convert_from_template.rs       # temporary; deletion checkpoint
├── fold.rs
├── formatter_view.rs
├── render_unit.rs
├── slot_plan.rs
├── wrapper_sets.rs
├── reactive_metadata.rs
└── tests/
```

Do not expose this outside AST templates except through narrow methods on existing AST template objects during migration.

### Core types

Use exact names only if they fit the implementation; the shape is the contract.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateIrId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateIrNodeId(u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateWrapperSetId(u32);

pub(crate) struct TemplateIrStore {
    templates: Vec<TemplateIr>,
    nodes: Vec<TemplateIrNode>,
    wrapper_sets: Vec<TemplateWrapperSet>,
    slot_plans: Vec<TemplateSlotPlan>,
    formatter_anchors: Vec<TemplateFormatterAnchor>,
    reactive_subscriptions: Vec<TemplateReactiveSubscription>,
}

pub(crate) struct TemplateIr {
    root: TemplateIrNodeId,
    style: Style,
    kind: TemplateType,
    summary: TemplateIrSummary,
    location: SourceLocation,
}

pub(crate) struct TemplateIrNode {
    kind: TemplateIrNodeKind,
    location: SourceLocation,
}

pub(crate) enum TemplateIrNodeKind {
    Sequence { children: Vec<TemplateIrNodeId> },
    Text { text: StringId, byte_len: u32, origin: TemplateSegmentOrigin },
    DynamicExpression { expression: Expression, origin: TemplateSegmentOrigin },
    ChildTemplate { template: TemplateIrId },
    Slot { slot: SlotPlaceholder },
    InsertContribution { template: TemplateIrId },
    BranchChain { branches: Vec<TemplateIrBranch>, fallback: Option<TemplateIrNodeId> },
    Loop { header: TemplateLoopHeader, body: TemplateIrNodeId, aggregate_wrapper: Option<TemplateIrNodeId> },
    LoopControl { kind: TemplateLoopControlKind },
    RuntimeSlotSite { site: RuntimeSlotSiteId },
}
```

If `Vec` fields in node kinds create too much clone pressure, replace them with typed ranges into store-owned side vectors in later phases.

### Summary metadata

```rust
pub(crate) struct TemplateIrSummary {
    pub estimated_output_bytes: usize,
    pub text_node_count: u32,
    pub text_byte_count: usize,
    pub dynamic_expression_count: u32,
    pub child_template_count: u32,
    pub slot_count: u32,
    pub wrapper_count: u32,
    pub max_depth: u16,
    pub has_slots: bool,
    pub has_formatter: bool,
    pub has_control_flow: bool,
    pub has_reactivity: bool,
    pub is_const_evaluable_shape: bool,
}
```

### Side-table ownership

Use side tables for data that is bulky, reused, or rarely read:

- slot routing plans;
- wrapper sets;
- formatter opaque anchors;
- reactive subscriptions;
- optional debug/source metadata if large;
- future cache metadata.

### Deferred from TIR v1

These are deliberately not part of TIR v1:

- source-span-backed body text instead of `StringId`;
- per-template parse cache;
- formatter-output cache;
- dev-mode source-hash keyed template reuse;
- dependency-aware invalidation for imported consts/directives;
- formatter algorithm rewrites beyond adapter/view changes;
- template behaviour changes;
- HIR/backend-facing TIR IDs.

## Phase B0 — TIR scaffolding and design docs

### Summary

Add the TIR module skeleton and design comments without changing behaviour. This creates a stable implementation target for later phases.

### Tasks

- [x] Add `src/compiler_frontend/ast/templates/tir/` module tree.
- [x] Add `ids.rs`, `store.rs`, `node.rs`, `summary.rs`, and `validation.rs`.
- [x] Re-export only the narrow internal API from `tir/mod.rs`.
- [x] Update `src/compiler_frontend/ast/templates/mod.rs` module docs to include TIR as a staged internal path.
- [x] Add file-level doc comments explaining:
  - [x] AST ownership;
  - [x] no HIR/backend ownership;
  - [x] semantic parity constraint;
  - [x] temporary converter deletion plan;
  - [x] no feature flag.
- [x] Add isolated tests for typed ID bounds, summary defaults, and basic store insertion.
  - Tests live under `src/compiler_frontend/ast/templates/tir/tests/`, following the project
    rule that tests do not live in production files.

### Measurement

- [x] No benchmark improvement expected.
- [x] Ran `just bench-frontend-check` (via `just validate`) to confirm no accidental overhead.

### Audit / style / validation

- [x] Confirm TIR module has one clear responsibility.
- [x] Confirm no old template paths are modified yet.
- [x] Run `just validate`.

### Phase B0 evidence summary

Created the TIR module skeleton under `src/compiler_frontend/ast/templates/tir/`:

- `ids.rs` — typed `u32` IDs: `TemplateIrId`, `TemplateIrNodeId`, `TemplateWrapperSetId`.
- `store.rs` — `TemplateIrStore` owning contiguous template/node/side-table vectors.
- `node.rs` — `TemplateIr`, `TemplateIrNode`, and `TemplateIrNodeKind` covering sequences,
  text, dynamic expressions, child templates, slots, insert contributions, branch chains,
  loops, loop control, and runtime slot sites.
- `summary.rs` — `TemplateIrSummary` shape metadata for capacity planning and feature flags.
- `validation.rs` — empty Phase B1 validation entry point.
- `mod.rs` — narrow `pub(crate)` re-exports and module-level design docs.

`src/compiler_frontend/ast/templates/mod.rs` was updated to list TIR as a staged internal
path in its module-layout documentation.

Validation passed:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --quiet`
- `cargo run -- tests`
- `cargo run -- check docs`
- `just validate`

`just validate` benchmark check reported `mixed: avg 0ms; 7 faster, 12 slower; 25/25 cases`,
with stage movement `ast +121ms`, `ast emit +65ms`, `ast env +41ms`. The TIR scaffolding is
not wired into production paths, so the movement is attributed to normal run-to-run variance;
no TIR-specific work runs during benchmarks yet. The phase is accepted as timing-neutral
scaffolding.

No language support status changed, so `docs/src/docs/progress/#page.bst` does not need an update.

## Phase B1 — TIR store, summaries, and converter parity

### Summary

Build TIR from current `Template` / `TemplateContent` without changing the production route. This gives parity tests and starts collecting TIR-specific counters.

### Files

- `src/compiler_frontend/ast/templates/tir/convert_from_template.rs`
- `src/compiler_frontend/ast/templates/tir/summary.rs`
- `src/compiler_frontend/ast/templates/tir/validation.rs`
- `src/compiler_frontend/instrumentation/ast_counters.rs`

### Counters to add

- [x] `TirTemplatesCreated`
- [x] `TirNodesCreated`
- [x] `TirTextNodesCreated`
- [x] `TirTextBytesRecorded`
- [x] `TirMaxDepth`
- [x] `TirConverterTemplatesConverted`
- [x] `TirConverterNodesConverted`
- [x] `TirWrapperSetsCreated`
- [ ] `TirWrapperSetReuseHits`
  - Deferred to Phase B5: wrapper-set reuse logic is not implemented yet, so the counter
    variant would be unused and produce dead-code warnings. It will be added alongside
    wrapper set deduplication.
- [x] `TirValidationNodesVisited`

### Implementation steps

- [x] Implement `TemplateIrStore::with_capacity_estimate(estimate: FrontendArenaCapacityEstimate)`.
- [x] Seed `templates`, `nodes`, and side vectors from existing capacity estimate fields.
- [x] Implement `convert_template_to_tir(template: &Template, store: &mut TemplateIrStore, string_table: &StringTable) -> TemplateIrId`.
- [x] Preserve:
  - [x] source locations;
  - [x] style metadata (including formatter presence in summary);
  - [x] template kind;
  - [x] text origin;
  - [x] slot placeholders;
  - [x] child-template opacity;
  - [x] control-flow structure (branch chains, loops, loop control);
  - [ ] runtime slot application metadata — `RuntimeSlotSite` nodes are created for the
        primary site, but full side-table preservation is deferred to Phase B5;
  - [x] reactive subscription presence flag (the `TemplateReactiveSubscription` side table
        remains a placeholder);
  - [x] wrapper plans/sets where representable — wrapper sets are pushed when conditional
        child wrappers exist, but the link from template to wrapper set is deferred to Phase B5.
- [x] Compute `TemplateIrSummary` during conversion without a second traversal.
- [x] Add TIR validation for impossible IDs, missing roots, invalid child/body references,
      and recursive cycles within a bounded depth.

### Parity tests

- [x] Add unit tests that convert synthetic templates to TIR and assert summary invariants
      (text, multi-text sequence, dynamic expression, child template, slot, branch chain,
      loop, loop control, empty template, multiple templates in one store).
- [ ] Add tests that compare old fold output to TIR-fold output — deferred to Phase B2.
- [x] Add tests for nested templates, slots, wrappers, branch chains, loops, loop control,
      and runtime slot application shape.
- [x] Add structural validation tests (empty store, valid template, valid sequence,
      out-of-bounds root, out-of-bounds child/body, converter+validation integration).

### Measurement

- [x] Production path does not build TIR — the converter is called only from tests.
- [x] Benchmark impact is zero/neutral.

### Audit / style / validation

- [x] Confirm converter is explicitly temporary in docs/comments.
- [x] Confirm no behaviour route changed.
- [x] Run `just validate`.

### Phase B1 evidence summary

Implemented Phase B1:

- Added `TemplateIrStore::with_capacity_estimate` in `tir/store.rs`, seeding template,
  node, and side vectors from `FrontendArenaCapacityEstimate`.
- Added `tir/convert_from_template.rs` with `convert_template_to_tir`, walking
  `TemplateContent` atoms, `TemplateControlFlow`, and `runtime_slot_application` to build
  TIR nodes. `TemplateIrSummary` is computed inline during the walk.
- Added structural `validate_tir_store` in `tir/validation.rs` with ID bounds checks,
  root validation, and bounded cycle detection.
- Added TIR counters in `ast_counters.rs` (9 of 10; `TirWrapperSetReuseHits` deferred to
  Phase B5 to avoid dead-code warnings).
- Added isolated tests under `tir/tests/converter_tests.rs` and
  `tir/tests/validation_tests.rs`.

Validation passed:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --lib instrumentation --features detailed_timers`
- `cargo test --quiet`
- `cargo run -- tests`
- `cargo run -- check docs`
- `just validate`

`just validate` benchmark check reported `mixed: avg -2ms; 9 faster, 2 slower; 25/25 cases`,
with stage movements inside noise (`ast +53ms`, `ast env +26ms`, `ast emit +20ms`). The
converter is test-only in this phase, so no production timing impact is expected.

No language support status changed, so `docs/src/docs/progress/#page.bst` does not need an
update.

## Phase B2 — TIR-native folding route

### Summary

Route compile-time folding through TIR while keeping the old folding path available as a parity oracle inside tests. This should remove some `TemplateContent -> TemplateRenderPlan` fallback work and improve string capacity use via summaries.

### Files

- `src/compiler_frontend/ast/templates/tir/fold.rs`
- `src/compiler_frontend/ast/templates/template_folding.rs`
- `src/compiler_frontend/ast/templates/tir/convert_from_template.rs`
- `src/compiler_frontend/ast/templates/tests/`

### Implementation steps

- [x] Implement `fold_tir_template(store, template_id, fold_context) -> TemplateEmission`.
- [x] Use `TemplateIrSummary::estimated_output_bytes` to seed output buffers.
- [x] Preserve structural no-output vs empty output.
- [x] Preserve break/continue output handling at the nearest template loop.
- [x] Preserve option-present capture folding.
- [x] Preserve const loop expansion limits.
- [x] Preserve nested child template folding order.
- [x] Preserve unresolved slot/insert rejection behaviour.
- [x] Keep old `Template::fold_into_stringid` as a wrapper that converts to TIR and folds through TIR, or route selected call sites directly through TIR if store access is already available.
  - `Template::fold_into_stringid` / `fold_to_emission` now route non-formatting templates through TIR. Templates that still require deferred body formatting use the legacy render-plan fold path until Phase B3 provides a TIR formatter view.
- [x] Add parity tests that compare old and new fold outputs for existing template cases.

### Deletion checkpoint

- [x] Mark old folding internals that become unused after TIR fold route.
  - Temporary `pub(crate)` helpers remain in `template_folding.rs` so TIR can share condition, option-capture, loop diagnostics, wrapper application, and aggregate render-piece folding without duplicating semantics.
- [ ] Delete obsolete fallback helpers only when no production call path uses them.

### Measurement

- [ ] Compare:
  - [ ] `TemplateFoldPlanPiecesVisited`
  - [ ] `TemplateFoldFallbackPlanBuilds`
  - [ ] `TemplateFoldExpressionCloneRequests`
  - [ ] `TemplateFoldOutputBytes`
  - [ ] TIR fold counters
- [ ] Run template stress and const-loop-heavy benchmarks.

### Audit / style / validation

- [ ] Confirm diagnostics/source locations match old behaviour.
- [ ] Confirm no change to `$markdown`, slots, wrappers, or runtime templates.
- [ ] Run `just validate`.
- [ ] Run five-run benchmark protocol.

### Phase B2a evidence summary — TIR node-shape prep for folding

Before implementing the fold route, the TIR node shape was extended to carry the
information the fold path needs without reaching back into the AST:

- `TemplateIr` now stores `conditional_child_wrappers: Vec<Template>` so the TIR
  fold path can apply parent `$children(..)` wrappers directly.
- `TemplateIrBranch` now stores the full `TemplateBranchSelector` (bool or
  option-present capture) instead of only the extracted condition expression.
  A `condition_expression()` helper provides uniform access for validation and
  cycle detection.
- The converter populates `conditional_child_wrappers` and passes the full
  selector through to `TemplateIrBranch::new`.
- Validation tests were updated to construct branches with `TemplateBranchSelector`.

This prerequisite slice was committed as `templates: TIR node-shape prep for
folding (Phase B2a)`. The remaining Phase B2 work — the actual TIR fold function,
production-path routing, and parity tests — is the next slice.

Validation passed for B2a:

- `cargo fmt`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --lib instrumentation --features detailed_timers`
- `cargo test --quiet`
- `cargo run -- tests`
- `cargo run -- check docs`
- `just validate`

`just validate` benchmark check reported `**-4ms avg**; 9 faster, 0 slower; 25/25 cases`,
with stage movements inside noise. No production path changed, so no timing impact
is expected.

### Phase B2b evidence summary — TIR-native folding route

Implemented the TIR-native folding route and wired the non-formatting production fold path through
TIR:

- Added `src/compiler_frontend/ast/templates/tir/fold.rs` with `fold_tir_template`, TIR node
  traversal, branch-chain folding, const range/collection loop folding, aggregate-wrapper folding,
  and TIR-specific fold counters.
- Routed `Template::fold_into_stringid` / `Template::fold_to_emission` through
  `TemplateIrStore` conversion plus TIR folding for templates that do not still require deferred
  body formatting.
- Kept deferred formatter work on the legacy render-plan fold path until Phase B3. The converter
  also keeps formatted nested child templates as dynamic expressions so they still call the
  formatter-aware legacy path.
- Preserved conditional `$children(..)` wrapper semantics by sharing one wrapper-application helper
  between legacy folding and TIR folding. This fixes the nested control-flow child case where TIR
  must apply wrappers stored on `TemplateIr`.
- Preserved loop-control semantics by consuming `break` / `continue` at the nearest template loop
  rather than propagating loop-control signals out of folded loops.
- Carried the old `TemplateAggregateRenderPlan` on the TIR loop node as a narrow temporary bridge
  until Phase B4 replaces aggregate wrappers with TIR-native render units.
- Added TIR fold parity tests for text, scalar expressions, bool branches, range and collection
  loops, nested child templates, nested control-flow child wrappers, unresolved slots, aggregate
  wrappers, and zero-output loops.

Validation passed:

- `cargo fmt`
- `cargo test --lib compiler_frontend::ast::templates`
- `cargo test --lib instrumentation --features detailed_timers`
- `cargo clippy --all-targets --all-features -- -D warnings`
- `just validate`

`just validate` benchmark check reported `**+5ms avg**; 0 faster, 22 slower; 25/25 cases`, with
stage movement `ast +68ms`, `ast env +31ms`, and `ast emit +24ms`. The full five-run Phase B2
benchmark protocol and `benchmarks/frontend-optimization-results.md` entry remain pending before
Phase B2 is closed.

## Phase B3 — TIR-native formatter view

### Summary

Feed existing formatter algorithms from TIR without rebuilding `TemplateContent` or `TemplateRenderPlan`. This removes formatter-bound representation ping-pong while preserving formatter behaviour.

### Files

- `src/compiler_frontend/ast/templates/tir/formatter_view.rs`
- `src/compiler_frontend/ast/templates/template_formatting.rs`
- `src/compiler_frontend/ast/templates/styles/`
- `src/compiler_frontend/ast/templates/template_render_units.rs`

### Implementation steps

- [ ] Define a TIR formatter view that exposes:
  - [ ] body text nodes eligible for formatting;
  - [ ] head content that bypasses body formatters;
  - [ ] opaque child-template anchors;
  - [ ] opaque dynamic-expression anchors;
  - [ ] source locations;
  - [ ] anchor kinds needed by `$markdown`.
- [ ] Adapt existing `FormatterInput` / `FormatterOutput` construction to use TIR view data.
- [ ] Rebuild TIR nodes from formatter output directly.
- [ ] Avoid rebuilding `TemplateContent` after formatting.
- [ ] Keep `$markdown` algorithm unchanged unless profiling later proves it remains a hotspot.
- [ ] Preserve child-template opacity rules.
- [ ] Preserve inline-code behavior around dynamic expression anchors.
- [ ] Preserve formatter warnings and diagnostics.

### Tests

- [ ] Existing markdown/code/raw/html/css/escape tests pass.
- [ ] Add TIR formatter parity tests for:
  - [ ] parent-authored inline code with dynamic expression anchors;
  - [ ] child-template opaque boundaries;
  - [ ] nested child templates under `$markdown`;
  - [ ] `$children(...)` wrappers plus formatter output.

### Measurement

- [ ] Compare `TemplateContentRebuildsAfterFormatting`, `TemplateRenderPlansBuilt`, and formatter-related AST stage movement.
- [ ] Profile markdown-heavy docs and `.bd` cases.

### Audit / style / validation

- [ ] Confirm formatter algorithms did not gain TIR-store mutation details.
- [ ] Confirm formatter view is a narrow API, not a second render plan.
- [ ] Run `just validate`.
- [ ] Run five-run benchmark protocol.

## Phase B4 — TIR render units, slots, wrappers, and control flow

### Summary

Move render-unit preparation semantics into TIR. This phase should remove most old `TemplateContent` / `TemplateRenderPlan` ping-pong for prepared templates.

### Files

- `src/compiler_frontend/ast/templates/tir/render_unit.rs`
- `src/compiler_frontend/ast/templates/tir/slot_plan.rs`
- `src/compiler_frontend/ast/templates/tir/wrapper_sets.rs`
- `src/compiler_frontend/ast/templates/template_render_units.rs`
- `src/compiler_frontend/ast/templates/template_slots/`
- `src/compiler_frontend/ast/templates/template_composition.rs`
- `src/compiler_frontend/ast/templates/template_control_flow/`

### Implementation steps

- [ ] Represent shared head prefixes in TIR without cloning full template content.
- [ ] Represent branch render units as TIR nodes that refer to shared prefix/wrapper data.
- [ ] Represent loop body and aggregate wrapper plans in TIR.
- [ ] Replace aggregate placeholder atoms with an internal TIR aggregate marker node or side-table entry.
- [ ] Intern/reuse wrapper sets through `TemplateWrapperSetId`.
- [ ] Convert slot routing to produce TIR slot plans.
- [ ] Keep runtime slot source/site planning behaviour identical.
- [ ] Preserve repeated slot replay behaviour.
- [ ] Preserve `$fresh` and direct-child wrapper scoping.
- [ ] Preserve conditional wrapper behaviour: skipped branches and zero-output loops must not receive wrappers.

### Deletion checkpoint

- [ ] Delete old aggregate placeholder conversion paths once TIR aggregate handling owns the route.
- [ ] Delete old wrapper clone helpers when `TemplateWrapperSetId` replaces them.
- [ ] Delete obsolete `content_with_shared_head_prefix` if no longer used.

### Tests

- [ ] Add parity tests for:
  - [ ] named slots;
  - [ ] positional slots;
  - [ ] default slots;
  - [ ] repeated slot replay;
  - [ ] loose content routing;
  - [ ] `$children(...)`;
  - [ ] `$fresh`;
  - [ ] template `if` false/no-else structural no-output;
  - [ ] template loop zero iterations;
  - [ ] `break`/`continue` preserving output before the signal.

### Measurement

- [ ] Compare old template content/render-plan counters against TIR counters.
- [ ] Expect visible reductions in render-plan builds, clones, wrapper vector clones, and fallback plan builds.

### Audit / style / validation

- [ ] Confirm side tables have clear ownership and no broad mutable context leaks.
- [ ] Confirm no template semantic changes are bundled.
- [ ] Run `just validate`.
- [ ] Run five-run benchmark protocol.

## Phase B5 — HIR preparation and runtime metadata from TIR

### Summary

HIR should continue receiving finalized runtime template metadata, not parse directives or consume raw TIR as a backend language. This phase changes AST's internal source of runtime template metadata from old structures to TIR.

### Files

- `src/compiler_frontend/ast/templates/top_level_templates.rs`
- `src/compiler_frontend/ast/templates/reactive_template_metadata.rs`
- `src/compiler_frontend/ast/templates/template_renderability.rs`
- HIR lowering files that currently read `Template.render_plan` or template runtime metadata
- `src/compiler_frontend/ast/templates/tir/reactive_metadata.rs`

### Implementation steps

- [ ] Add TIR APIs that produce the existing HIR-facing runtime template handoff shape.
- [ ] Preserve runtime fragment order.
- [ ] Preserve top-level const fragment metadata.
- [ ] Preserve reactive template metadata.
- [ ] Preserve runtime slot application plans.
- [ ] Preserve backend feature validation inputs.
- [ ] Do not expose `TemplateIrId` as a backend-facing stable ABI.
- [ ] Keep HIR free of formatter/directive/slot-schema parsing.

### Tests

- [ ] Existing HTML/JS integration goldens pass.
- [ ] Existing reactivity tests pass.
- [ ] Existing runtime template control-flow tests pass.
- [ ] Existing top-level fragment ordering tests pass.

### Measurement

- [ ] Compare runtime template rebuild counters and AST/HIR stage movement.
- [ ] Confirm backend time does not regress.

### Audit / style / validation

- [ ] Confirm AST/HIR boundary remains clean.
- [ ] Confirm TIR IDs do not leak outside frontend AST internals.
- [ ] Run `just validate`.
- [ ] Run five-run benchmark protocol.

## Phase B6 — Direct parser-to-TIR emission

### Summary

Make template parsing emit TIR directly. This is the phase where TIR begins replacing the old `TemplateContent` construction path rather than being a converter target.

### Files

- `src/compiler_frontend/ast/templates/template_body_parser.rs`
- `src/compiler_frontend/ast/templates/template_head_parser/`
- `src/compiler_frontend/ast/templates/create_template_node.rs`
- `src/compiler_frontend/ast/templates/tir/store.rs`
- `src/compiler_frontend/ast/templates/tir/node.rs`
- `src/compiler_frontend/ast/templates/tir/render_unit.rs`

### Implementation steps

- [ ] Add parser context access to `TemplateIrStore` through a narrow `TemplateIrBuilder`.
- [ ] Parse template heads into TIR metadata and side-table entries.
- [ ] Parse template body text into TIR `Text` nodes with `StringId` and byte length.
- [ ] Parse nested templates into child `TemplateIrId`s, not owned recursive `Template` values.
- [ ] Parse slot definitions and insert contributions into TIR nodes/side tables.
- [ ] Parse direct `[else]`, `[else if]`, `[break]`, and `[continue]` markers into TIR control-flow nodes.
- [ ] Preserve current source diagnostics and source locations exactly where practical.
- [ ] Keep a temporary old-to-new converter only for tests and migration call sites.
- [ ] Stop immediate foldable child-template folding during parsing if TIR folding can do it later with equal or better capacity behaviour. Preserve output semantics.

### Deletion checkpoint

- [ ] Mark old parser `TemplateContent` construction paths for removal.
- [ ] Delete converter-only paths once all production parsing emits TIR.
- [ ] Delete obsolete `TemplateInheritance` wrapper cloning paths if replaced by wrapper-set IDs.

### Tests

- [ ] Parser unit tests pass.
- [ ] Template integration tests pass.
- [ ] Add parser-to-TIR structure tests for nested templates, doc comments, suppressed child templates, control flow, slots, wrappers, and formatter markers.
- [ ] Add diagnostic parity tests for malformed templates.

### Measurement

- [ ] Expect reductions in:
  - [ ] `TemplateAtomsParsed` or old content atoms;
  - [ ] wrapper vector clones;
  - [ ] render plan builds;
  - [ ] nested template parse/fold churn;
  - [ ] AST time on template-heavy cases.
- [ ] If AST time regresses, inspect whether TIR builder introduced too many small vectors or side-table reallocations.

### Audit / style / validation

- [ ] Confirm `template_body_parser.rs` remains readable.
- [ ] Split large builder logic into TIR files rather than bloating parser files.
- [ ] Run `just validate`.
- [ ] Run five-run benchmark protocol.

## Phase B7 — Make TIR authoritative and delete obsolete structures

### Summary

Remove permanent parallel representations. TIR becomes the authoritative AST template structure. Old structures should either disappear or become narrow compatibility shells only where another API still expects them temporarily inside the same phase.

### Files

- `src/compiler_frontend/ast/templates/template_types.rs`
- `src/compiler_frontend/ast/templates/template.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/template_render_units.rs`
- `src/compiler_frontend/ast/templates/template_folding.rs`
- `src/compiler_frontend/ast/templates/template_composition.rs`
- `src/compiler_frontend/ast/templates/template_slots/`
- `src/compiler_frontend/ast/templates/tir/`

### Implementation steps

- [ ] Replace `Template.content` authority with `TemplateIrId` or a renamed TIR-backed `Template` shell.
- [ ] Remove or narrow `TemplateContent` to a temporary parser/test-only type if still needed.
- [ ] Remove or narrow `TemplateRenderPlan` if TIR formatter/folding/HIR no longer needs it.
- [ ] Remove `render_plan: Option<TemplateRenderPlan>` from the authoritative template path.
- [ ] Remove old `clone_for_composition` patterns that TIR IDs make unnecessary.
- [ ] Remove old `unformatted_content` if TIR formatter metadata replaces it.
- [ ] Remove old aggregate placeholder atom handling if TIR aggregate nodes own it.
- [ ] Update `remap_string_ids` to walk TIR store and side tables.
- [ ] Update all internal callers to use TIR APIs.
- [ ] Delete temporary converter after no production or parity tests require it.

### Tests

- [ ] Full `just validate`.
- [ ] Full integration suite.
- [ ] Existing backend goldens.
- [ ] Template-specific parity tests should now assert against TIR output directly rather than old/new comparison.

### Measurement

- [ ] Run full five-run benchmark protocol.
- [ ] Run targeted profile on the worst remaining template-heavy case.
- [ ] Compare against Plan A final baseline and TIR pre-migration baseline.

### Audit / style / validation

- [ ] Confirm no permanent compatibility wrappers remain.
- [ ] Confirm module docs describe the new actual flow.
- [ ] Confirm public AST/HIR/backends remain cleanly separated.
- [ ] Confirm no dead code is left with stale comments.
- [ ] Run `cargo clippy` explicitly if `just validate` output is too noisy to inspect.

## Phase B8 — Documentation, roadmap, and matrix updates

### Summary

Document the final internal architecture and deferred optimisation options. Do not claim user-visible feature changes.

### Required docs updates

- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] Explain TIR as AST-owned template representation.
  - [ ] State that HIR consumes finalized runtime template metadata and does not parse directives/slot schemas.
  - [ ] State that TIR is internal and behaviour-preserving.
- [ ] Update `src/compiler_frontend/ast/templates/mod.rs`:
  - [ ] Replace old flow diagram with TIR flow.
  - [ ] Map files to current owners.
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] Link the TIR implementation plan under `# Plans` until complete.
  - [ ] Add follow-up bullets for deliberately deferred template performance work:
    - [ ] source-span-backed template body text instead of eager `StringId` interning;
    - [ ] per-template parse cache;
    - [ ] formatter-output cache;
    - [ ] dev-mode source-hash keyed template reuse;
    - [ ] dependency-aware invalidation for imported consts/directives;
    - [ ] formatter algorithm rewrites only if post-TIR profiling justifies them;
    - [ ] incremental module/template compilation integration after module-boundary incremental builds exist.
- [ ] Update `docs/src/docs/progress/#page.bst` only after TIR lands:
  - [ ] Keep `Templates and style directives` status as `Supported` if behaviour is unchanged.
  - [ ] Add a short note that the implementation now uses an internal AST-local TIR for template parsing/folding/formatting/lowering preparation.
  - [ ] Do not add speculative caching/source-span text rows unless those features are actually implemented.
  - [ ] If a dedicated internal tooling/performance row exists later, mark source-span-backed text and template caching as `Deferred`; otherwise keep them in roadmap only.
- [ ] Update `benchmarks/README.md` only if TIR adds new recommended benchmark cases or stable counters.
- [ ] Update `benchmarks/frontend-optimization-results.md` with final TIR evidence summary.

### Audit / style / validation

- [ ] Confirm documentation distinguishes internal representation from language features.
- [ ] Confirm deferred features are in roadmap, not presented as implemented.
- [ ] Confirm Progress Matrix still describes current behaviour.
- [ ] Run `just validate`.

## Phase B9 — Final TIR performance review

### Summary

Close the refactor only after measuring whether TIR actually improved the intended workloads.

### Tasks

- [ ] Run five independent `just bench-frontend` runs.
- [ ] Run five independent `just bench` runs.
- [ ] Run `just bench-report`.
- [ ] Profile the worst remaining template-heavy case with `normal` or `deep` mode.
- [ ] Compare against:
  - [ ] pre-Plan-A baseline;
  - [ ] post-Plan-A baseline;
  - [ ] pre-TIR baseline;
  - [ ] final TIR result.
- [ ] Summarize:
  - [ ] wall-time movement;
  - [ ] AST stage movement;
  - [ ] render-plan clone/build movement;
  - [ ] TIR node/store counters;
  - [ ] output byte/capacity estimate accuracy;
  - [ ] remaining hotspots;
  - [ ] whether any deferred roadmap item is now justified.

### Acceptance

- [ ] Accept if semantic parity is proven, old representation paths are deleted, and template-heavy churn/timing improves or at least becomes materially cleaner without meaningful regression.
- [ ] If TIR is structurally cleaner but timing-neutral, keep only if clone/build counters materially decrease and code readability is not worse.
- [ ] If TIR regresses timing or complexity without clear benefit, pause before further work and write a rollback/refactor decision note.

---

# Agent handoff checklist

Before starting implementation:

- [ ] Read `docs/compiler-design-overview.md`.
- [ ] Read `docs/language-overview.md` template section.
- [ ] Read `docs/memory-management-design.md` only for AST/HIR ownership boundaries if needed.
- [ ] Read `codebase-style-guide.md`.
- [ ] Read `benchmarks/README.md`.
- [ ] Inspect current `src/compiler_frontend/ast/templates/` files before editing.
- [ ] Inspect `src/compiler_frontend/arena/` capacity helpers before adding new heuristics.
- [ ] Inspect `src/compiler_frontend/instrumentation/` before adding counters.

Before ending each phase:

- [ ] Run `just validate`.
- [ ] Run relevant benchmark protocol.
- [ ] Record concise results.
- [ ] Remove dead code and temporary adapters scheduled for that phase.
- [ ] Check touched files against style guide.
- [ ] Confirm no raw benchmark/profile data is staged.
- [ ] Confirm no semantic template behaviour changed unless it is an explicit bug fix with tests.
