### REVERT MISTAKEN AST DRIFT

## Updated plan

### 2. `html_wasm` direct `bst_start() -> Vec<String>` runtime ABI

**Status:** mostly completed

This is aligned with the current compiler docs: the entry `start()` returns runtime fragment strings in source order, and builders hydrate slots from that result rather than using a separate wrapper pipeline .

ONLY the entry file has a top level start function. There should be an error for files that are using the top-level scope for regular code or top level templates outside of the entry file. Other headers like functions, choices, structs and constants are fine though.

#### Completed

* Wasm helper export policy now includes:

  * `bst_vec_new`
  * `bst_vec_push`
  * `bst_vec_len`
  * `bst_vec_get`
* Wasm helper emission and export sections were extended for the vec-handle runtime.
* LIR gained `VecNew` and `VecPushHandle`.
* HIR→LIR lowering now supports:

  * empty `Vec<String>` collection literal construction
  * runtime fragment push lowering
* HTML Wasm bootstrap now:

  * calls `instance.exports.bst_start()`
  * reads vec length/items
  * decodes string handles through `bst_str_ptr` / `bst_str_len`
  * hydrates slots directly
  * releases the vec handle afterwards
* HTML Wasm export-plan/request plumbing was updated accordingly.

#### Correction for drift

* The original plan described this as a narrow ABI addition. That is still true.
* But the implementation is **not** generic collection lowering. It only handles the runtime-fragment accumulator path plus empty string-vec construction. Keep that scope explicit.

#### Remaining

* Validate full backend behavior through the actual `html_wasm` suite.
* Add canonical integration coverage for the Wasm runtime fragment collection path, which the roadmap still calls out as a gap .
* Consider splitting vec-helper emission out of the giant helper emitter before more runtime helpers land. That file is trending toward “does too much” per the style guide’s single-responsibility and function-size guidance .

---

### 3. Unit / frontend contract test rewrites

**Status:** started, not completed

The ast-refactor plan says tests should assert the restored architecture directly: strict top-level sorting, `start` excluded from the graph, AST consuming headers directly, and no top-level reparse assumptions .

#### Completed

* Some `build_system::build` tests were updated to reflect runtime slot placeholders and the new HTML/Wasm output shape.
* A few assertions were loosened to avoid brittle exact-shape failures.

#### Still missing

* Dedicated dependency-sorting tests for:

  * strict-edge sort behavior
  * cycle rejection
  * `start` excluded from graph
  * `start` appended last
  * no soft-edge behavior
* AST contract tests for:

  * consuming parsed header payloads directly
  * no top-level reparse
  * entry `start` body seeing resolved top-level declarations
* Focused tests for constant-header deferral and struct-default resolution.

---

### 4. Integration fixture migration and curation

**Status:** not done in these two commits

This remains one of the main unfinished parts of Part 4 of the roadmap plan .

#### Still needed

* Audit failing fixtures by semantic intent.
* Rewrite stale fixtures that assumed old top-level execution behavior.
* Move brittle output checks to:

  * normalized golden
  * rendered-output assertions
  * targeted artifact assertions
* Add stronger canonical cases for remaining alpha gaps.

---

### 5. Harness hardening

**Status:** not done in these two commits

#### Still needed

* Remove remaining `todo!` / panic-shaped assertion paths in the integration runner.
* Add runner-level regression tests for normalization and wasm import/export assertion handling.
* Keep harness failure vs semantic mismatch classification clean.

This is explicitly still part of the plan and also required by the no-user-input-panics rule in the style guide .

---

### 6. Manifest / matrix ownership

**Status:** not done in these two commits

#### Still needed

* Update `tests/cases/manifest.toml` alongside fixture migrations.
* Update `docs/roadmap/language-surface-integration-matrix.md` alongside canonical coverage changes.
* Keep those in the same commits as fixture work.

---

### 7. Part 5 cleanup after the functional fixes

**Status:** still pending

The broader AST cleanup from the roadmap remains separate and still active:

* remove remaining transitional structure
* keep `ast/mod.rs` as the strict entry point
* tighten context ownership
* trim public surface
* clean lints/dead code/comments/tests 

Nothing in these two commits closes that work.

---

## Drift / excessive code corrections

### `visible_scope.rs`

You were right to be suspicious.

That file is **not needed** in the final shape. The second commit removed it, and that was the correct move. The information needed for deferral is already present through:

* file-local `visible_symbol_paths`
* `declaration_stubs_by_path`

Reintroducing a separate visibility helper would push the code back toward duplication and stage-overlap, which the refactor is trying to remove  .

### `type_resolution.rs`

This is the main place where the implementation now looks heavier than ideal.

The new `inline_visible_constant_references*` machinery is doing a lot:

* rewriting expressions
* inlining constants
* partially re-evaluating runtime-node shapes
* supporting struct default folding

That is probably justified functionally, but it is now the clearest candidate for follow-up extraction into a dedicated AST constant-resolution helper/module. As it stands, it is drifting toward mixed responsibility.

### `parse_file_headers.rs`

The new placeholder discovery scan is defensible because header parsing owns top-level discovery and AST should not re-scan top-level syntax later . But it should stay there and not spawn another parallel visibility/discovery path.

### Wasm helper emission

`emit/helpers.rs` is getting bulky. Before more helpers are added, split vec helper emission into dedicated helper functions or a vec-runtime helper file. That would better match the style guide’s organization rules .

### Test softening

Some unit-test assertion loosening is acceptable for this transition, but this should not become the permanent solution. The roadmap already points toward fixture-level assertion migration instead of broad, softer unit tests .

---

## Cleaned-up replacement for the plan

### Part 4 — Rebuild tests around the restored frontend contract

#### 1. Header-stage directive visibility + deferred constant/default resolution

**Completed**

* Thread build-system style directives into header parsing early.
* Seed declaration stubs on `ModuleSymbols`.
* Allow header-owned constant/default shaping to defer unresolved constant references.
* Resolve deferred constant headers in AST using visible symbol paths plus declaration stubs.
* Remove redundant header-stage visibility helper file.

**Remaining**

* Verify docs build is green.
* Add focused regression tests for `$html`, constant deferral, and struct defaults.

#### 2. `html_wasm` direct `bst_start()` fragment vec ABI

**Completed**

* Add vec runtime helper exports and lowering support.
* Return vec handle directly from `bst_start()`.
* Decode runtime fragment vec in JS bootstrap and hydrate slots.

**Remaining**

* Validate full `html_wasm` suite.
* Add canonical integration coverage for this path.
* Keep generic collection lowering out of scope for now.

#### 3. Rebuild unit/frontend tests around the final contract

**Started**

* Some build tests were updated for slot-placeholder/runtime-fragment behavior.

**Remaining**

* Add dependency-sort contract tests.
* Add AST contract tests.
* Add regression tests for deferred constant resolution.

#### 4. Curate and migrate integration fixtures

**Not done**

* Rewrite stale fixtures.
* Move noisy cases to intent-aligned assertion modes.
* Add stronger canonical fixtures for remaining matrix gaps.

#### 5. Harden the integration harness

**Not done**

* Remove panic/todo paths.
* Add runner regression tests.
* Keep failure classes distinct.

#### 6. Keep manifest and matrix in sync

**Not done**

* Update manifest and matrix with fixture batches.




# Overview
The goal is to restore the intended frontend architecture and remove the AST drift that duplicated header-stage work. 
This refactor has been gradually taking place and is focused on removing redunancy and simplifying the frontnend wherever possible.

Beanstalk’s frontend is intentionally eager. Earlier stages should do the declaration-level work needed so later stages can stay focused and avoid reparsing or rebuilding the same top-level information again.

The correct contract is:

* header parsing owns top-level declaration discovery
* header parsing parses the declaration of top-level items
* header parsing collects strict top-level dependency edges from those declarations
* dependency sorting orders top-level headers before AST begins
* AST consumes the already-shaped, already-sorted headers directly
* AST resolves, validates, and lowers those headers
* AST parses executable bodies and other body-local declarations
* AST does **not** reparse top-level declarations

The tests are currnently broken and now need to be fixed or updated after this large frontend refactor.

There are also some unexpected errors when trying to build the docs `cargo run build docs` involving no longer finding the $html directive.
This should also be investigated as part of this pass. "ERROR: Style directive '$html' is unsupported here."

# Part 4 - Rebuild tests around the restored contract

The ast refactor has broken most of the integration tests and 9 unit tests. This will require work to investigate why the tests are failing and whether the test should be updated, or the code fixed to pass the test.

Tests should now assert the restored architecture directly.

#### Dependency sorting tests should verify

* top-level declaration headers are sorted by strict edges
* cycles in strict top-level dependencies are rejected
* `start` does not participate in graph sorting
* `start` is appended last
* no soft-edge behavior remains in this phase

#### AST tests should verify

* AST consumes parsed header payloads directly
* AST no longer reparses top-level declarations
* final type resolution and semantic validation still work correctly
* body parsing still behaves correctly in sorted order

#### Integration tests should verify

* representative modules with functions, constants, structs, and choices still compile correctly
* top-level dependencies resolve in the intended order
* entry `start` sees all resolved top-level declarations before its body is lowered

## Migrate remaining brittle fixtures, prune redundant coverage, and close the Alpha test matrix gaps

Now that the integration runner supports strict goldens, normalized goldens, rendered-output assertions, and targeted artifact assertions, finish migrating brittle fixtures to the right assertion surface, remove redundant cases that no longer add value, and fill the most visible Alpha-surface coverage gaps.

This will also be part of fixing the current failing tests and making sure they are correctly refactored to be less brittle and more complete with the new AST architecture.

The integration runner is already capable of lower-noise assertion modes. The remaining work is fixture migration and coverage curation: some fixtures are still too brittle for what they actually test, some gaps remain visible in the language surface matrix, and some older coverage is now redundant or weaker than newer canonical cases.

Focus areas:

* Existing test failure investigation / fixing
* Rewrite tests for the final architecture
* manifest-driven import visibility
* layered local scope growth
* no AST-side declaration recollection assumptions
* entry `start()` as the runtime fragment producer
* builder merge behavior for const fragments + runtime fragments
* HTML Wasm export plan behavior after direct entry `start()` export

**Goals**
Delete or rewrite tests that are only validating the old recollection model.
Fix / update remaining tests so they are both less brittle and pass after the current refactor.

* Migrate remaining brittle fixtures to normalized, rendered-output, or targeted artifact assertions where appropriate.
* Keep strict byte-for-byte goldens only where exact output shape is actually the contract.
* Fill the clearest remaining Alpha-surface gaps.
* Remove or rewrite redundant tests that duplicate stronger canonical coverage.
* Keep the matrix and manifest aligned with the real supported surface.

**Non-goals**

* No weakening of semantic checks just to reduce failures.
* No mass deletion of tests without replacing lost confidence.

**Implementation guidance**

#### 1. Audit all remaining brittle fixtures by assertion intent

For each currently noisy fixture, decide what it is really testing:

* **Strict golden** when exact HTML/JS/Wasm shape is the contract
* **Normalized golden** when emitted code structure matters but counter-name drift is noise
* **Rendered output** when runtime behavior is the contract
* **Artifact assertions** when only a few targeted output properties matter

Document the migration reason in the PR notes so future fixture authors can follow the pattern.

#### 2. Migrate the remaining runtime-fragment-heavy brittle cases

Prioritize fixtures where full generated-output snapshots are still too noisy compared with the semantic intent.

Common candidates:

* runtime fragment ordering / interleave behavior
* result propagation/fallback through generated output
* runtime collection read/write flows
* call/lowering paths where helper/counter drift is noisy
* short-circuit/runtime behavior cases where rendered output is the real contract

#### 3. Fill the explicit matrix gaps

Add or strengthen canonical cases for the most visible remaining gaps:

* choice / match backend-runtime coverage
* char failure diagnostics
* HTML-Wasm collection runtime coverage
* cross-platform newline / rendering drift-sensitive surfaces
* any remaining receiver-method runtime-sensitive cases outside plain JS coverage

Where possible, prefer one strong canonical fixture over several narrow redundant fixtures.

#### 4. Prune or rewrite redundant coverage

Audit tests that are now redundant because newer canonical cases cover the same behavior more clearly.

Candidates to prune or rewrite:

* older fixtures that assert emitted-shape noise rather than semantics
* overlapping frontend-only tests that add little beyond stronger integration cases
* repeated narrow cases that can be merged into one clearer canonical scenario

Do not delete coverage blindly. Replace weak/redundant tests with stronger intent-aligned tests.

#### 5. Harden the integration harness itself where needed

Use this PR to remove remaining obvious harness rough edges that affect trust in the suite.

In particular:

* remove any remaining `todo!`/panic-shaped paths in integration assertion code that can still be exercised during normal test workflows
* add small runner-level tests around normalization / rendered-output behavior where confidence is still thin
* keep harness failures clearly distinct from semantic mismatches

#### 6. Keep matrix and manifest ownership disciplined

For every test migration or new canonical fixture:

* update `docs/roadmap/language-surface-integration-matrix.md`
* update `tests/cases/manifest.toml`
* remove vague “temporary” coverage where the new canonical case supersedes it

The goal is that the matrix describes the real supported Alpha surface and the canonical fixtures that prove it.

**Suggested migration heuristic**

Use this decision rule consistently:

* exact emitted shape matters → strict golden
* emitted structure matters but generated counters do not → normalized golden
* runtime behavior matters → rendered output
* only a few output facts matter → artifact assertions

**Checklist**

* Audit remaining brittle fixtures by semantic intent.
* Migrate noisy full-file goldens to normalized/rendered/artifact modes where appropriate.
* Add missing canonical cases for the visible Alpha matrix gaps.
* Rewrite or remove redundant weaker tests that no longer add confidence.
* Remove remaining avoidable `todo!`/panic-shaped harness paths in active test code.
* Update the language surface matrix and test manifest alongside fixture changes.
* Add small runner-level regression tests where the assertion infrastructure itself needs confidence.

**Done when**

* Remaining broad golden failures mostly indicate real semantic regressions, not generator noise.
* The visible Alpha matrix gaps are materially reduced.
* The suite has fewer redundant fixtures and stronger canonical cases.
* Harness failures are clearly infrastructure failures, not mixed with semantic mismatches.
* The matrix and manifest accurately reflect the current supported surface.

**Implementation notes for the later execution plan**

* Treat this as a curation PR, not a random grab-bag.
* Migrate fixtures in small themed batches so failures stay interpretable.
* Prefer behavior-first assertions for runtime semantics.
* Keep strict goldens only where exact emitted shape is intentionally contractual.



# Part 5 — finalize, audit, document, and clean tests

## Overview

The goal is to finish the refactor cleanly and lock in the final AST shape.

This phase is about removing transitional structure, making the module layout obvious, tightening ownership boundaries inside the AST, and cleaning up tests/docs so they reflect the final architecture rather than the refactor process.

This phase is **not** the broader header/AST responsibility cleanup pass. Larger ownership cleanups such as `import_bindings.rs` and wider stage-boundary review will happen in a separate task. This phase should first finish the current refactor properly.

Desired design:

* `orchestrate.rs` is removed
* `ast/mod.rs` is the strict entry point and overview of the AST module
* `ast/mod.rs` clearly shows the AST pipeline flow and where the important parts live
* `ast/mod.rs` does not act as a transitional forwarding layer
* aliasing and old path shims are removed
* the public surface of `ast/mod.rs` is intentionally small and reflects the final architecture only
* `ScopeContext` and the main AST build context have clear responsibilities and are passed through the AST in a disciplined way
* touched code follows the style guide and compiler design docs
* tests reflect the current AST ownership model and pipeline, not the previous transitional structure

Done so far:

* the top-level template rewrite already removed a large amount of fragment-specific complexity
* the remaining work is now mostly structural cleanup, module-surface cleanup, documentation alignment, and test/lint follow-through

## Work to do

### 0. Remove `orchestrate.rs` and move its logic into `ast/mod.rs`

Files:

* `src/compiler_frontend/ast/module_ast/orchestrate.rs`
* `src/compiler_frontend/ast/mod.rs`

Finish phase 0 of this plan fully.

`orchestrate.rs` should be removed entirely once its logic has been moved into `ast/mod.rs`.

The final `ast/mod.rs` should be the first file someone reads to understand:

* the AST stage entry point
* the real pass order
* what the AST owns
* what it consumes from header parsing
* which internal files implement the important parts of the pipeline

Do not leave behind a split entrypoint.

### 1. Make `ast/mod.rs` the strict entry point and module overview

Files:

* `src/compiler_frontend/ast/mod.rs`
* `src/compiler_frontend/mod.rs`

Rewrite `ast/mod.rs` so it reflects the final architecture directly.

It should:

1. expose the AST stage entry point
2. show the real pipeline in order
3. act as the overview/orchestration file for the module
4. point clearly to the important internal files and types
5. contain concise comments describing the stage flow and responsibilities

It should not:

* preserve old forwarding structure
* reintroduce `orchestrate.rs` in another form
* expose internal details unnecessarily
* carry stale wording from the transitional refactor shape

### 2. Trim the public surface of `ast/mod.rs` after the refactor

Files:

* `src/compiler_frontend/ast/mod.rs`
* `src/compiler_frontend/ast/module_ast/mod.rs`
* any touched AST submodules

Once the final flow is in place, do a cleanup pass over visibility.

This should include:

* removing aliasing and old path shapes
* removing transitional re-exports
* reducing `pub` visibility for internal-only helpers, types, and modules
* keeping only the API surface needed by the rest of the compiler
* making the module root reflect the final architecture rather than the history of the refactor

The AST module should have one obvious public entry surface and minimal internal leakage.

### 3. Tighten context ownership and data-passing discipline inside the AST

Files:

* `src/compiler_frontend/ast/module_ast/build_state.rs`
* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* any main AST build context type used by the refactor

Review the responsibilities and passing discipline of `ScopeContext` and the main AST build context.

The goal is to avoid context drift where state becomes split across overlapping structs without a clear reason.

This pass should check:

* what data belongs in long-lived AST build state
* what data belongs in local scope-tracking only
* which data should be passed explicitly instead of being stored broadly
* whether any fields are duplicated, partially overlapping, or only exist because of the transition
* whether naming still reflects the final role of each context clearly

Prefer one clear owner for each category of data.

Avoid multiple context structs carrying near-duplicate state or becoming generic “bags of stuff”.

### 4. Review the touched areas against the style guide and compiler design docs

Files:

* all files touched by this plan
* `docs/compiler-design-overview.md`
* `docs/codebase-style-guide.md`

Do a deliberate alignment pass:

* stage ownership matches the design overview
* `mod.rs` files reflect the intended organisation rules
* module boundaries remain clear
* no transitional wrappers or compatibility shims remain
* comments explain behavior and rationale, not syntax
* error paths use structured diagnostics and avoid user-input panics
* naming stays explicit and full
* files and functions still have one clear responsibility

### 5. Add strong comments to the new AST pipeline key parts

Files:

* `src/compiler_frontend/ast/mod.rs`
* `src/compiler_frontend/ast/module_ast/build_state.rs`
* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* `src/compiler_frontend/ast/import_bindings.rs`
* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`

The key AST pipeline parts should be clearly commented.

Comments should explain:

* what this stage owns
* what it no longer owns
* why `ast/mod.rs` is the strict entry point
* how header parsing and AST responsibilities are currently divided
* how AST uses the shared top-level manifest
* how local scope growth works during body lowering
* how the surviving build/scope contexts relate to each other
* how entry `start()` and const fragment finalization relate to the big picture

Use concise WHAT/WHY comments and file-level docs.

### 6. Clean remaining lints, dead code, and stale tests

This final phase must explicitly include:

* cleaning up any remaining `clippy` lints in touched areas
* reviewing dead code, stale `#[allow(dead_code)]`, unused helpers, and leftover unused paths
* removing stale comments and docs from the old architecture
* removing leftover legacy codepaths or aliases kept only during transition
* updating or deleting tests that still assert transitional AST structure instead of the final model
* running the required checks from the style guide:

  * `cargo clippy`
  * `cargo test`
  * `cargo run tests`

## Done when

* `orchestrate.rs` is gone
* `ast/mod.rs` is the strict AST entry point and overview file
* aliasing, transitional re-exports, and old path shims are removed
* the public surface of the AST module is intentionally small and reflects the final architecture only
* `ScopeContext` and the main AST build context have clear, non-overlapping responsibilities
* touched code follows the style guide and compiler design docs
* key AST pipeline files are well commented with clear WHAT/WHY and stage ownership
* tests validate the final architecture rather than transitional internals
* remaining clippy lints in touched areas are cleaned up
* no stale architectural comments from the old model remain