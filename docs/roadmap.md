# Beanstalk Pre-Alpha Checklist

This is a working execution plan for getting the compiler to a credible first alpha.

## Release gates

These are the non-negotiable conditions for starting Alpha.

- All claimed Alpha features compile, type check, and run through the full supported pipeline.
- Unsupported syntax or incomplete features fail with structured compiler diagnostics, not panics.
- The integration suite covers the supported language surface, not just recent feature areas.
- The JS backend and HTML builder are stable enough for real small projects and docs-style sites.
- Compiler diagnostics are useful, accurate, consistently formatted, and visually moving toward the Nushell-style goal.
- Cross-platform output is stable enough that Windows and macOS do not produce avoidable golden drift.

---

## Phase 1 - Code review checkpoint

This phase is a deliberate cleanup and consolidation checkpoint before pushing further on language surface.
The goal is to reduce structural risk now, remove stale paths while the compiler is still prealpha,
and make later feature work land into a tighter codebase.

### PR - Add coverage for the audit-discovered blind spots

Use the audit as a direct test-gap backlog and close the highest-value coverage holes before more features are added.

**Checklist**
- Add or expand coverage for:
  - Stage 0 config parsing and validation edge cases
  - module discovery and routing/homepage behavior
  - top-level const-template and start-fragment ordering rules
  - receiver method visibility/import/same-file constraints
  - borrow-checker CFG edge cases and merge/drop-site behavior
  - JS runtime-sensitive lowering paths
  - Wasm request/contract validation paths
  - output cleanup and stale artifact behavior
- Prefer end-to-end integration coverage where it gives better protection than narrow unit tests.
- Prune or rewrite tests that are redundant after the new coverage lands.

**Done when**
- The most important audit findings are protected by tests.
- Regressions in recently-refactored hotspots are more likely to be caught at the right layer.
- The suite becomes broader rather than just denser in a few areas.

### PR - Run a style-guide and readability sweep across the touched areas

Finish the checkpoint by making the newly-refactored code read like deliberate final code rather than churn aftermath.

**Checklist**
- Add or tighten file-level docs and WHAT/WHY comments where the refactors introduced new seams.
- Normalize naming and function boundaries to match `docs/codebase-style-guide.md`.
- Remove any remaining low-value comments that only narrate syntax or restate code.
- Re-check that touched files are not carrying avoidable inline imports, broad dead-code allowances, or mixed responsibilities.
- Run the normal verification loop:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- The refactor checkpoint leaves the codebase clearer, not just differently arranged.
- The touched subsystems read consistently with the style guide.
- This phase ends with the compiler in a tighter shape for the next language-feature work.

## Phase 2 - close the core language feature gaps

### PR - Consolidate Char across the frontend and backend surface

Stop Char being a neglected primitive with uneven support.

**Checklist**
- Audit tokenizer, parser, AST typing, HIR typing, evaluation, lowering, and backend handling for Char.
- Fill any missing type-checking or lowering gaps.
- Add parser, type, runtime/backend, and integration coverage.

**Done when**
- Char behaves like a deliberate core datatype rather than a half-kept edge type.

### PR - Add named argument passing for function calls and struct creation

Support `as`-based named argument passing as the first non-positional call path.

**Checklist**
- Finalize the parser/lowering shape for named arguments using `as`.
- Thread named call arguments through function-call checking.
- Thread named field assignment through struct construction.
- Add diagnostics for unknown names, duplicates, missing required args, mixed-order invalid cases if disallowed.
- Add integration tests for calls and struct initialization.

**Done when**
- Named argument passing works for the chosen Alpha scope.
- The compiler can explain bad named-argument usage cleanly.

### PR - Harden structs, records, and methods together

Close the loop on struct/record/method behavior as one language slice.

**Checklist**
- Audit runtime structs and const records against current docs/scope.
- Confirm methods resolve cleanly, especially receiver methods and same-file/export visibility.
- Add missing integration tests for declaration, construction, defaults, methods, field access, mutation, and diagnostics.
- Tighten any remaining semantic rough edges.

**Done when**
- Structs and records feel Alpha-ready as a practical feature, not a partially assembled one.

### PR - Harden basic if expressions and logical expressions

Make these small core expression features boring and reliable.

**Checklist**
- Audit expression parsing, type checking, constant folding, and lowering.
- Add focused integration cases for boolean combinations, nesting, precedence, and invalid type combinations.
- Improve error messages for non-boolean logic misuse.

**Done when**
- These features no longer feel like edge behavior.

---

## Phase 3 - expand integration coverage across the full Alpha surface

### PR - Create a language-surface integration matrix

Track what supported language features have canonical end-to-end coverage.

**Checklist**
- Add a simple feature-to-case mapping section or helper doc.
- Enumerate the Alpha surface:
  - control flow
  - functions/calls
  - templates/style directives
  - structs/records/methods
  - choices
  - pattern matching
  - arrays
  - results/options/multiple returns/multiple assignment
  - type checking
  - paths/imports
  - html project builds
  - logical expressions
  - if expressions
  - char
  - named arguments
- Mark gaps explicitly.

**Done when**
- Missing integration coverage is visible immediately.

### PR - Add integration coverage for the neglected language areas

Broaden the suite away from being overly concentrated on current recent work.

**Checklist**
- Add success and failure cases for basic control flow.
- Add success and failure cases for function declarations/calls.
- Add templates/style directive stability cases.
- Add structs/records/methods cases.
- Add arrays and array diagnostics.
- Add logical and if-expression cases.
- Add Char cases.

**Done when**
- The canonical integration suite represents the supported language rather than mostly paths/results/assets.

### PR - Add backend-facing integration checks for runtime-heavy features

Make sure JS/backend semantics are being checked where language behavior depends on runtime lowering.

**Checklist**
- Add cases for alias-sensitive behavior where relevant.
- Add cases for template runtime fragment insertion behavior.
- Add cases for result propagation/fallback through generated outputs.
- Add cases for arrays and casts where backend behavior matters.
- Expand artifact assertions where goldens alone are too brittle or too vague.

**Done when**
- Runtime semantics are not being trusted blindly.

---

## Phase 4 - diagnostics and compiler UX hardening

### PR - Standardize unsupported/incomplete-feature diagnostics

All incomplete or intentionally deferred features fail the same way: clearly and helpfully.

**Checklist**
- Audit current “not implemented”, “reserved”, and fallback diagnostics.
- Normalize wording, stage metadata, source locations, and suggestion style.
- Prefer one clean pattern for deferred-feature errors.

**Done when**
- Unsupported features feel deliberately handled.

### PR - Improve type-checking diagnostics across common user mistakes

Push compiler errors toward useful Nushell-style presentation and clarity.

**Checklist**
- Audit the most common type mismatch surfaces.
- Make messages name exact types and exact offending value/name where practical.
- Improve suggestions for common mistakes in calls, assignments, expressions, and struct construction.
- Add targeted failure fixtures proving the wording is specific enough.

**Done when**
- Type errors are accurate, grounded, and visibly better than generic compiler output.

### PR - Improve formatting/rendering of compiler errors

Move the displayed output closer to the desired final feel.

**Checklist**
- Refine rendered formatting for file path, span, label ordering, suggestions, and grouped messages.
- Make CLI `check` and normal build output feel consistent.
- Keep the data model stable while improving presentation.
- Add snapshot/golden-style tests for formatter output if practical.

**Done when**
- Errors look intentional and readable, not merely structurally correct.

### PR - Add variable-name ban list / reserved near-builtins

Prevent obviously stupid or misleading variable names that collide with builtin semantics.

**Checklist**
- Define a ban/reservation policy for misleading names such as `_true`, `FALSE`, and too-close builtins.
- Enforce it in parsing/name-resolution/type stages as appropriate.
- Produce good diagnostics explaining why the name is reserved.
- Add integration tests.

**Done when**
- Users cannot create confusing pseudo-builtin identifiers.

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
- PullDeprecated enum variant (src/compiler_frontend/ast/field_access.rs:176, 189, 387): A deprecated CollectionBuiltinMethod variant guarded by three unreachable!() calls.

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