# Beanstalk Pre-Alpha Checklist

This is a working execution plan for getting the compiler to a credible first alpha.

Use the language surface integration matrix as a reference for what is currently implemented: `docs/language-surface-integration-matrix.md`

## Release gates

These are the non-negotiable conditions for starting Alpha.

- All claimed Alpha features compile, type check, and run through the full supported pipeline.
- Unsupported syntax or incomplete features fail with structured compiler diagnostics, not panics.
- The integration suite covers the supported language surface, not just recent feature areas.
- The JS backend and HTML builder are stable enough for real small projects and docs-style sites.
- Compiler diagnostics are useful, accurate, consistently formatted, and visually moving toward the Nushell-style goal.
- Cross-platform output is stable enough that Windows and macOS do not produce avoidable golden drift.

## Phase 0: Tidy Up Tasks

### PR - Review and reorganize runtime start-fragment synthesis and capture analysis

`src/compiler_frontend/ast/templates/top_level_templates.rs` has grown into a substantial piece of frontend behavior.

It now handles:
- entry start-function scanning
- top-level template extraction
- const vs runtime fragment partitioning
- fragment ordering
- generated runtime fragment functions
- capture dependency analysis
- mutation-sensitive capture rejection
- pruning of template-only declarations
- doc fragment collection and stripping

This is exactly the kind of file that can keep working while gradually becoming too broad to reason about safely.

This PR turns the existing roadmap note about runtime template review into a fuller code-organization and semantic-audit pass.

**Why now**
- Runtime template/start-fragment behavior is now a real compiler surface, not just a temporary implementation detail.
- The file mixes orchestration, dependency analysis, mutation checks, fragment synthesis, and doc extraction.
- Later HTML-builder stabilization will be easier if this frontend/template-fragment behavior is cleaned up first.

**Primary repo targets**
- `src/compiler_frontend/ast/templates/top_level_templates.rs`
- any nearby template folding / template AST helpers it depends on
- tests for top-level template extraction and runtime fragment ordering/capture behavior

**Primary goals**
- Split orchestration from analysis logic.
- Re-check capture semantics and mutation restrictions against the intended language/runtime model.
- Make generated runtime fragment behavior easier to audit and test.
- Keep doc-fragment collection from being too entangled with runtime start-fragment synthesis.

**Checklist**
- Split `top_level_templates.rs` into smaller focused units, likely around:
  - fragment extraction/order planning
  - runtime capture/dependency analysis
  - fragment function synthesis
  - doc fragment collection/stripping
- Re-check the runtime fragment capture model for:
  - declaration dependency inclusion
  - mutation-sensitive capture rejection
  - declaration pruning in the rewritten entry start body
- Re-check whether capture analysis helpers are duplicating AST-walking logic that should be shared elsewhere.
- Ensure comments clearly explain:
  - why runtime fragments are synthesized as generated functions
  - why mutable reassignment before fragment evaluation is currently rejected
  - how pruned declarations are determined
- Add or tighten tests for fragment ordering, capture dependencies, and mutation rejection behavior.

**Testing checklist**
- Add targeted tests for:
  - mixed const/runtime top-level templates
  - source-order preservation
  - runtime fragment capture of required declarations only
  - rejection of mutable reassignment before fragment evaluation
  - pruning of declarations that are template-only captures
  - doc fragment extraction remaining stable after refactor
- Re-run:
  - `cargo clippy`
  - `cargo test`
  - `cargo run tests`

**Done when**
- Runtime start-fragment synthesis is easier to understand and maintain.
- Capture analysis is isolated enough to be audited independently.
- Template/doc extraction behavior is cleaner ahead of the final HTML-builder stabilization pass.

## Phase 4 - diagnostics and compiler UX hardening

### PR - Improve formatting/rendering of compiler errors

Move the displayed output closer to the desired final feel.

**Checklist**
- Refine rendered formatting for file path, span, label ordering, suggestions, and grouped messages.
- Make CLI `check` and normal build output feel consistent.
- Keep the data model stable while improving presentation.
- Add snapshot/golden-style tests for formatter output if practical.

**Done when**
- Errors look intentional and readable, not merely structurally correct.

### PR - Eliminate syntax-adjacent invariant panics and unreachable parser assumptions

Harden parser and AST-construction paths so malformed or unsupported user input reliably becomes structured compiler diagnostics instead of depending on nearby invariant-only assumptions.

The release gates already require unsupported or incomplete features to fail cleanly rather than through accidental panic-like behavior.
Most of the compiler is already moving in that direction.
This PR is a focused pass over syntax-adjacent `expect`, `unwrap`, and `unreachable!` style assumptions so the remaining rough edges are removed before Alpha.

This is not a blanket ban on all internal invariants.
Truly unreachable internal compiler corruption paths can stay panic-only where appropriate.
The goal is to eliminate those assumptions where malformed user syntax, reserved syntax, or parser drift could still plausibly reach them.

**Scope**
- Parser and AST-construction code
- Syntax-adjacent invariant assumptions
- Reserved/deferred syntax rejection paths
- Diagnostics for malformed syntax that currently depends on nearby internal assumptions
- Keep true internal-corruption invariants separate from user-input validation

**Primary goals**
- Ensure malformed user input produces compiler diagnostics rather than relying on panic-ish invariant paths
- Distinguish true compiler-internal invariants from syntax/user-input assumptions
- Make reserved/deferred syntax handling look intentional and structured everywhere
- Improve alpha readiness by reducing avoidable panic risk near parser surfaces

**Checklist**
- Audit parser and AST-construction code for:
  - `expect(...)`
  - `unwrap(...)`
  - `unreachable!(...)`
  - similar invariant-only assumptions
- For each occurrence, decide whether it is:
  - a true internal compiler invariant that should remain panic-only
  - a user-input-adjacent path that should become a structured diagnostic
- Replace syntax-adjacent invariant assumptions with structured compiler errors where the precondition can be violated by user-authored code, malformed syntax, reserved syntax, or parser drift.
- Re-check reserved/deferred syntax paths so they fail through one clean diagnostic pattern rather than a mix of fallback behavior and internal assumptions.
- Re-check named-handler, postfix/member parsing, and other syntax-heavy areas where preconditions may currently be enforced indirectly.
- Keep diagnostics specific:
  - name the syntax context
  - point at the relevant source location
  - suggest the direct fix where practical
- Add or tighten WHAT/WHY comments where a remaining panic-only path is preserved as a deliberate internal invariant.
- Do not hide compiler bugs behind vague diagnostics; keep genuine internal-compiler-failure paths distinguishable from user syntax errors.

**Suggested implementation order**
1. Inventory syntax-adjacent invariant assumptions in parser/AST code.
2. Convert clearly user-reachable ones to structured diagnostics first.
3. Re-check reserved/deferred syntax rejection paths second.
4. Re-check syntax-heavy helper areas such as postfix/member parsing and result-handling parsing third.
5. Leave only clearly justified internal compiler invariants as panic-only paths.

**Testing checklist**
- Add regression tests for malformed or unsupported inputs that previously depended on invariant-only assumptions.
- Add targeted failure fixtures for any reserved/deferred syntax path normalized during this pass.
- Ensure diagnostics remain specific enough to prove the correct failure reason.
- Re-run:
  - `cargo clippy`
  - `cargo test`
  - `cargo run tests`

**Done when**
- Syntax-adjacent parser/AST paths no longer rely on avoidable panic-ish invariant assumptions for user-authored bad input.
- Reserved and deferred syntax fails through deliberate structured diagnostics.
- Remaining panic-only paths are clearly internal compiler invariants rather than user-input validation shortcuts.
- The compiler is closer to Alpha release-gate expectations for clean unsupported-syntax handling.

### PR - Dev server hardening
The dev server current can hang when clicking links and sometimes takes a very long time to respond to file changes and perform a rebuild.

A review of the code should also take place to make sure the code is well orgsanised, following the codebase style guide and has helpful comments.

**Done when**
- Dev server no longer hangs when a page is refreshed multiple times
- Dev server is always snappy and responsive to source file changes and performs fast rebuilds
- Dev server code is well organised, commented and concise

---

## Phase 5 - cross-platform consistency and test stability

### PR - Finish CRLF normalization in strings and templates

Remove avoidable Windows/macOS golden drift from source normalization and emitted outputs.

**Checklist**
- Audit remaining CRLF behavior in strings, templates, and emitted output.
- Make sure normalized newline handling is consistent through the frontend and builder outputs.
- Add regression tests specifically for Windows-shaped input.

**Done when**
- Golden outputs are stable across normal Windows/macOS workflows.

### PR - Add normalized artifact assertion mode to reduce non-semantic golden churn

Full `index.html` snapshots currently include generated JS details such as line-number-derived symbols
and temporary names (`bst___hir_tmp_*`) that change even when runtime behavior does not.
That makes integration goldens brittle and increases PR noise/risk when frontend lowering shape changes.

This PR should make integration assertions more robust without weakening semantic checks.

**Fits with other PRs**
- Extends the Phase 3 integration-checks goal to use stronger assertions where byte-for-byte goldens are too brittle.
- Should land before Phase 6 backend/html stabilization so those audits can use less brittle assertions.

**Checklist**
- Extend the integration runner expectation model with an explicit normalized-assertion mode for text artifacts.
- Implement deterministic normalization for generated HTML/JS assertion comparisons, focused on unstable compiler-generated identifiers and irrelevant formatting drift.
- Keep strict byte-for-byte golden checks available for cases where exact output shape is intentionally contractual.
- Add fixture-level documentation/examples showing when to choose strict goldens vs normalized assertions.
- Migrate the known brittle runtime-fragment cases (including the recent function/collection/char/receiver-method drift set) to the appropriate assertion style.
- Add runner tests proving normalization is stable and does not mask real semantic regressions.

**Done when**
- Non-semantic generator-shape churn no longer causes broad golden failures.
- Semantic changes still fail with clear, targeted integration diffs.

### PR - Add rendered-output assertions for runtime-fragment semantics

Some integration behaviors are fundamentally about rendered output, not emitted JS text layout.
For runtime-fragment-heavy cases, asserting rendered slot output provides stronger semantic confidence
than snapshotting compiler-generated temporary symbols.

**Fits with other PRs**
- Builds on the normalized-assertion work above.
- Supports the Phase 6 JS backend semantic audit with behavior-first checks.

**Checklist**
- Add an optional integration assertion mode that executes generated HTML+JS in a deterministic test harness and compares rendered runtime-slot output.
- Keep this mode focused on semantic surfaces (runtime fragments, call/lowering paths, collection/read flows) where emitted-text snapshots are noisy.
- Ensure harness failures distinguish:
  - test harness limitations/infrastructure errors
  - actual rendered-output mismatches
- Add targeted cases that currently rely on brittle full-file goldens but are really asserting rendered text behavior.
- Document expectation-writing guidance so new cases choose rendered assertions when appropriate.

**Done when**
- Runtime-fragment semantics are asserted directly at rendered-output level where needed.
- Integration failures are lower-noise and more actionable during backend/lowering changes.

### PR - Fix remaining Windows test-runner stability issues

Remove test-runner and lock-poisoning rough edges that still make Windows less reliable.

**Checklist**
- Audit known lock poisoning paths and test-runner failure behavior.
- Ensure failed tests/builds do not leave the runner in a poisoned or misleading state.
- Add targeted tests where possible.

**Done when**
- Windows failures look like normal compiler/test failures, not infrastructure weirdness.

---

## Phase 6 - JS backend and HTML builder hardening pass

### PR - JS backend semantic audit for Alpha surface

Verify that the JS backend behavior matches the intended Alpha language rules for the supported feature set.

**Checklist**
- Audit runtime helpers involved in aliasing, copying, arrays, result propagation, casts, and builtin helpers.
- Add or expand integration tests where behavior depends on emitted JS runtime logic.
- Fix any semantics that are currently “working by accident”.
- Re-check collection builtin lowering in `src/compiler_frontend/ast/field_access/collection_builtin.rs` and remove any compatibility-only branches that drift from current frontend semantics.
- Confirm builtins using synthetic/fake parameter declarations are either removed or intentionally retained with clear justification
- Add backend-facing tests for:
  - collection get/set/push/remove/length
  - error helper builtin methods
  - mutable receiver method place validation

**Done when**
- The JS backend is trustworthy enough for real Alpha examples.

### PR - HTML builder final stabilization pass

Treat the HTML project builder as a real Alpha product surface.

**Checklist**
- Re-audit route derivation, homepage rules, duplicate path diagnostics, tracked assets, cleanup, and output layout.
- Add any remaining config and artifact assertions needed for confidence.
- Ensure docs site and small static-site projects remain a valid proving ground.

**Done when**
- The HTML project builder can be presented as a stable Alpha capability.

---

## Final pre-alpha sweep

### PR - Alpha checklist audit

Verify that the Alpha gates are genuinely met.

**Checklist**
- Re-run the feature matrix and mark all supported areas as covered.
- Re-check that unsupported/deferred features fail cleanly.
- Re-check that docs and examples match actual support.
- Re-check diagnostics quality on a representative set of failures.
- Re-check cross-platform golden stability.

**Done when**
- There is a credible yes/no answer to “is Alpha ready?”

### PR - Alpha cleanup PR

Land final small consistency and hygiene fixes before the release branch/tag.

**Checklist**
- Remove obsolete rejection fixtures for features that are now supported.
- Tighten comments, TODOs, and dead-code justifications.
- Prune stale scaffolding where the current design has clearly replaced it.
- Update release-facing docs and contribution notes if needed.

**Done when**
- The repo feels intentional at the point Alpha begins.

---

## Deferred until after Alpha
These are intentionally not Alpha blockers unless they become necessary for one of the supported slices.

This is a collection of notes and findings for future roadmaps once the roadmap above is complete.

- builtin `Error` enrichment beyond what is already required for the current compiler/runtime surface
- full tagged unions
- full pattern-matching design
- full interfaces implementation
- richer numeric redesign work not required by Alpha

**Wasm**

Broader Wasm maturity beyond the current experimental path.

Notes and limitations from previous investigations:
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
