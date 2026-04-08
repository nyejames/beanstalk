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

## Phase 1 - Code review checkpoint

This phase is a deliberate cleanup and consolidation checkpoint before pushing further on language surface.
The goal is to reduce structural risk now, remove stale paths while the compiler is still prealpha,
and make later feature work land into a tighter codebase.

### PR - Split postfix/member parsing into field access, receiver calls, and builtin member handlers

Break up the current postfix/member parsing path before more method and call-surface work lands on top of it.

Right now one frontend area is carrying too many distinct responsibilities:
- chained postfix parsing
- field lookup
- mutable-place checks
- receiver method lookup/call parsing
- collection builtin member handling
- builtin error-helper member handling
- some removed/legacy member diagnostics

This is readable enough today, but it is the kind of file shape that quietly turns into a catch-all as more language surface lands.
This PR should split those concerns into clearer homes while keeping current language behavior stable unless a real inconsistency is discovered during the refactor.

This is not meant to redesign member syntax.
The goal is to make the existing behavior come from smaller, more intentional modules so later method and builtin work lands into cleaner boundaries.

**Scope**
- Frontend AST parsing only
- Postfix/member parsing and helper structure
- Collection builtin member handling
- Builtin error-helper member handling
- Mutable-place helper logic used by member calls
- Keep current user-visible semantics stable unless a clear bug or inconsistency is found

**Primary goals**
- Stop one file from owning all postfix/member parsing concerns
- Separate user-defined receiver methods from compiler-owned builtin member behavior
- Make mutable-place analysis come from one small deliberate helper path
- Make builtin member parsing read like policy, not like a side-effect of general field parsing
- Remove stale member-handling logic if it is clearly deprecated and no longer justified

**Checklist**
- Audit the current postfix/member parsing path and list the responsibilities it currently owns before moving code.
- Extract the shared postfix-chain driver into a clearly named module or keep it as the thin coordinating layer after the split.
- Move field/member name parsing and field-access-specific helpers into a field-access-focused home.
- Move mutable-place / place-shape helpers into a small shared helper module used by:
  - receiver method calls
  - builtin member calls
  - assignment/mutation paths where relevant
- Move collection builtin member parsing into its own focused module.
- Move builtin error-helper member parsing into its own focused module.
- Keep receiver-method lookup and receiver-method call validation in a dedicated receiver-method area rather than mixing it with builtin member behavior.
- Re-check whether any builtin member argument parsing is currently relying on fake or synthetic declaration shapes and simplify that path if doing so improves clarity.
- Remove any now-redundant local helpers or duplicated checks after the split.
- Re-check diagnostics so they still name the correct member, receiver type, and expected usage.
- Re-check that const-record restrictions and mutable-receiver restrictions still come from one deliberate validation path.
- Add WHAT/WHY comments around the new module boundaries explaining:
  - what responsibility each module owns
  - why that responsibility is separate
  - what should not be added there
- Keep the final shape aligned with the style guide instead of simply moving code into more files.

**Suggested implementation order**
1. Audit and list the current responsibilities in the existing postfix/member parsing file.
2. Extract mutable-place helper logic first.
3. Extract collection builtin member parsing second.
4. Extract builtin error-helper member parsing third.
5. Re-home receiver-method lookup/call parsing into a clearer dedicated area.
6. Leave one thin postfix-chain coordinator that dispatches to field access, builtin members, or receiver methods.
7. Delete redundant helpers and run a readability pass over the final shape.

**Testing checklist**
- Re-run existing parser/unit coverage for:
  - plain field access
  - chained field access
  - receiver method calls
  - mutable receiver rejection
  - const-record method rejection
  - collection builtin methods
  - builtin error helper methods
- Add regression tests for any inconsistency or bug found during the split.
- Ensure diagnostics remain stable or improve only intentionally.
- Re-run:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- Postfix/member parsing no longer reads like one catch-all subsystem.
- Field access, receiver methods, and compiler-owned builtin members are clearly separated.
- Mutable-place checks come from one small deliberate helper path.
- The frontend is in a cleaner shape for later method-surface and builtin-surface work.

### PR - Prune dead scaffolding and pre-alpha placeholder surfaces

Use a deliberate cleanup pass to remove placeholder scaffolding that is not earning its keep in the current pre-alpha codebase.

At this stage the compiler should bias toward a tight, current-design codebase rather than carrying around speculative helpers, dead modules, or broad dead-code allowances just because they might be useful later.
Some placeholders are justified.
Some are not.
This PR is about auditing that boundary and pruning what no longer belongs in the active compiler path.

This is not about deleting future ideas from docs or design notes.
It is about removing dead or premature implementation surface from the production codebase when it is not part of the current alpha path.

**Scope**
- Frontend, build-system, project, and backend code where dead scaffolding or placeholder code is present
- `#[allow(dead_code)]` / similar allowances
- Not-yet-wired modules and helpers
- Clearly deferred enum variants or utility paths that are not part of the current supported surface
- Keep code that is genuinely part of the active alpha path even if a small amount of future-proofing remains necessary

**Primary goals**
- Reduce mental overhead from dead or speculative implementation surface
- Stop central types and modules from becoming storage for deferred ideas
- Make dead-code allowances rare and justified rather than ambient
- Keep the main compiler path focused on the current supported design

**Checklist**
- Audit `#[allow(dead_code)]` usage across the codebase and categorize each one as:
  - justified current placeholder
  - near-term planned and necessary
  - removable now
- Remove not-yet-wired modules or helpers that are not on the active alpha path and are not providing current value.
- Re-check crate/module exports so removed placeholder code is not still being carried by public or crate-level module structure.
- Audit central enums and core data-model types for deferred variants that are not part of the current supported alpha surface.
- For each deferred variant or placeholder type helper, choose one of:
  - remove it now
  - move it behind a more appropriate boundary
  - keep it with a clear WHY comment explaining why it must exist before the feature itself lands
- Remove utility methods that only exist for hypothetical future refactors if they are not actually used.
- Tighten or remove stale comments that describe old transitional behavior rather than the current design.
- Re-check tests and fixtures for coverage that only exists to preserve dead compatibility or removed scaffolding paths.
- Do not keep transitional wrappers just to preserve an older internal API shape.
- Add WHAT/WHY comments only where a remaining placeholder is genuinely justified and should stay for the current roadmap.

**Suggested implementation order**
1. Inventory dead-code allowances and placeholder modules first.
2. Remove obviously dead or not-wired project/helpers second.
3. Audit central enums/types for speculative variants third.
4. Remove stale helper methods and transitional wrappers fourth.
5. Run a readability pass over touched files so the final shape is tighter, not just smaller.

**Testing checklist**
- Re-run focused unit/integration tests around every touched subsystem.
- Remove or update tests that were only protecting dead scaffolding rather than supported behavior.
- Add regression coverage if pruning reveals a real dependency that should remain deliberate.
- Re-run:
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- Dead-code allowances are exceptional rather than routine.
- The active compiler path is carrying less speculative ballast.
- Central enums/types/modules read more like the current compiler and less like a storage area for future ideas.
- The codebase is tighter and easier to reason about before Alpha.

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
  - named arguments (`parameter = value`, with call-site `~` on the value expression)
- Mark gaps explicitly.

For every feature, mark:
- implementation status
- parser/unit coverage
- integration coverage
- backend/runtime coverage if relevant

Distinguish “reserved but deferred” from “implemented but incomplete”

Include compiler-owned builtins and method-like surfaces in the matrix:
- collection methods
- error helper methods
- receiver methods
- result suffix handling

Include cross-platform coverage flags for golden-sensitive features

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
  - `cargo check`
  - `cargo test`
  - `cargo run tests`

**Done when**
- Syntax-adjacent parser/AST paths no longer rely on avoidable panic-ish invariant assumptions for user-authored bad input.
- Reserved and deferred syntax fails through deliberate structured diagnostics.
- Remaining panic-only paths are clearly internal compiler invariants rather than user-input validation shortcuts.
- The compiler is closer to Alpha release-gate expectations for clean unsupported-syntax handling.

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
- Audit collection builtin lowering from AST member syntax through backend-visible call semantics
- Remove deprecated collection builtin compatibility paths instead of preserving them behind unreachable guards
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