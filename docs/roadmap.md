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

### REVERT MISTAKEN AST DRIFT





# Plan: restore the original AST architecture

## Diagnosis

The regression began when AST was changed from:
- a consumer of header-owned top-level declaration knowledge

into:
- a pass-driven stage that rebuilds module-wide declaration/index state from sorted headers before lowering bodies.

The root misunderstanding was introduced in commit `9786f4f41cbec596fe9d2f5b3f7cd7a9594654fc`.
Later commits `8cd0572a8d01747975438a9c7b76757f5beb15fe` and `87df41a78d37a60bec59a740d1603bc9703e815a` expanded and entrenched AST-side normalization/finalization responsibilities.
`bdf78deb5ca8f0cb1bb673add470a3e366502cd9` is surrounding orchestration churn, not the root cause.

## Correct architecture to restore

Pipeline:

1. Tokenize files
2. Parse file headers
3. Dependency sort headers
4. Lower sorted headers directly into AST
   - top-level declarations are already known from headers
   - function/local declarations are added in order as encountered during body parsing

Principles:

- Header parsing owns top-level declaration discovery
- Dependency sorting owns inter-header order
- AST owns:
  - type resolution
  - constant folding
  - body lowering
  - template lowering only where genuinely required by the AST→HIR boundary
- AST does **not** rebuild the module declaration universe from headers
- Function/local declarations remain incremental and ordered

## Things to preserve

Do not remove or redesign the current local declaration path inside function bodies.

Keep:
- `new_declaration(...)`
- `context.add_var(...)`
- local declaration insertion in source order as statements are parsed

That part still matches the original architecture.

## Phase 1: restore the stage contract in code comments and types

### Goals
- Make the intended ownership boundaries explicit before changing logic
- Stop the code from documenting the wrong model

### Changes
- Update `src/compiler_frontend/mod.rs`
  - rewrite the `headers_to_ast()` docs so they explicitly say:
    - headers already provide top-level declaration structure
    - AST lowers sorted headers and only adds in-body declarations incrementally
- Update `src/compiler_frontend/ast/module_ast/mod.rs`
  - remove wording that frames pass 1 as “register all symbols module-wide”
- Add a top-level comment in the AST entrypoint stating:
  - headers are the source of truth for top-level declarations
  - AST must not reconstruct them

### Done when
- comments and docs match the intended old architecture exactly

## Phase 2: move top-level declaration ownership back to headers

### Goals
- make header parsing / sorted headers the authoritative source of top-level declaration metadata
- eliminate AST-side reconstruction of top-level declaration indexes

### Changes
- Introduce a header-owned manifest or index structure, for example:
  - declared symbol paths by file
  - declared names by file
  - export visibility
  - file imports
  - start-function symbol path / alias metadata
- Build this during header parsing and/or dependency sorting
- Pass it into AST as an input instead of deriving it again inside `AstBuildState`

### Files
- `src/compiler_frontend/headers/parse_file_headers.rs`
- `src/compiler_frontend/module_dependencies.rs`
- `src/compiler_frontend/mod.rs`

### Done when
- AST no longer needs to scan sorted headers just to reconstruct declaration tables

## Phase 3: delete AST pass 1 declaration collection

### Goals
- remove the direct source of duplicated work
- make AST start from real semantic work instead

### Changes
- Delete `src/compiler_frontend/ast/module_ast/pass_declarations.rs`
- Remove `collect_declarations(...)` from `Ast::new(...)`
- Remove `register_declared_symbol(...)` and any AST-owned top-level declaration/index state that only exists to support pass 1
- Rewrite import-binding resolution to consume the header-owned manifest instead

### Files
- `src/compiler_frontend/ast/module_ast/pass_declarations.rs`
- `src/compiler_frontend/ast/module_ast/orchestrate.rs`
- `src/compiler_frontend/ast/module_ast/build_state.rs`
- `src/compiler_frontend/ast/module_ast/pass_import_bindings.rs`

### Done when
- `Ast::new(...)` begins from real lowering/resolution work, not declaration recollection

## Phase 4: shrink `AstBuildState` to real AST responsibilities

### Goals
- remove state that exists only because AST was rebuilding header-owned data
- reduce conceptual and compile-time overhead

### Remove or relocate if only used for top-level recollection
- `importable_symbol_exported`
- `file_imports_by_source`
- `declared_paths_by_file`
- `declared_names_by_file`
- `module_file_paths`
- any helper methods that only populate those tables

### Keep only if still required for true AST work
- resolved type/signature tables
- module constants
- const template results
- AST nodes
- warnings
- template metadata genuinely needed before HIR

### Files
- `src/compiler_frontend/ast/module_ast/build_state.rs`

### Done when
- `AstBuildState` looks like AST state, not a second header-index database

## Phase 5: stop cloning full declaration vectors through scope contexts

### Goals
- preserve ordered local declarations
- avoid copying the whole module declaration table into every scope

### Changes
- Replace `ScopeContext.declarations: Vec<Declaration>` with a layered shape:
  - shared immutable module declaration view
  - small mutable local declaration list for the active scope
- Child control-flow scopes should inherit local visibility cheaply
- Function scopes should extend with parameters without cloning the full module vec
- `add_var(...)` should continue to append only local declarations

### Files
- `src/compiler_frontend/ast/module_ast/scope_context.rs`
- `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
- any expression/reference lookup helpers that currently assume one flat cloned vec

### Done when
- function emission no longer does:
  - `self.declarations.to_owned()`
- `ScopeContext::new(...)` no longer eagerly clones the whole declaration table

## Phase 6: trim AST finalization back to the minimum boundary needed

### Goals
- keep AST focused
- prevent AST from becoming a generic whole-tree normalization machine

### Review introduced drift from
- `8cd0572a8d01747975438a9c7b76757f5beb15fe`
- `87df41a78d37a60bec59a740d1603bc9703e815a`

### Questions to answer per finalization helper
- Is this genuinely AST semantic work?
- Is this actually template parsing/composition work that should happen earlier?
- Is this HIR-shaping work that should happen at the AST→HIR lowering boundary instead?
- Is this only compensating for another abstraction leak?

### Likely keep
- top-level const template folding
- start fragment synthesis if AST is still the canonical source for it

### Likely reduce or move
- broad recursive AST-wide normalization sweeps that exist mainly to “clean up” before HIR
- duplicated template/materialization helpers that can happen during template construction instead

### Files
- `src/compiler_frontend/ast/module_ast/finalization/*`
- `src/compiler_frontend/ast/templates/*`
- `src/compiler_frontend/hir/*` boundary code where appropriate

### Done when
- AST finalization is small, explicit, and only contains logic that truly belongs before HIR

## Phase 7: restore the tests around the original contract

### Add regression coverage for

1. Header-owned top-level knowledge
   - imported declarations are available without AST recollecting them
   - sorted headers are sufficient for top-level lowering

2. Ordered in-body declarations
   - locals become visible only after declaration
   - no full pre-scan of function locals is required

3. Import visibility
   - file-scoped imports remain enforced
   - start-function aliases still work

4. Performance-shape regression checks
   - no AST declaration recollection pass
   - no full declaration vec cloning per function scope

### Done when
- the restored architecture is enforced by tests, not just comments

## Expected end state

`Ast::new(...)` should look conceptually like this:

1. consume sorted headers plus header-owned module manifest
2. resolve types/signatures using that manifest
3. lower headers/bodies directly
4. add locals as encountered inside body parsing
5. perform only minimal AST finalization actually required before HIR

No AST-side top-level declaration recollection.
No repeated rebuilding of symbol/index tables from headers.
No cloning of full module declaration vecs into every scope.
 
### PR - Refactor collection builtins into explicit compiler-owned operations and remove compatibility-shaped dispatch

Collection builtins should lower through an explicit compiler-owned representation instead of leaning on method-call-shaped compatibility scaffolding. This removes fake dispatch surface, simplifies backend contracts, and makes collection semantics easier to audit for Alpha.

**Why this PR exists**

The language rules are already clear: collection operations are compiler-owned builtins, not ordinary user-defined receiver methods. The current implementation still carries method-call-shaped indirection, including synthetic builtin paths and compatibility behavior that blurs the semantic boundary. That is workable in pre-alpha, but it is exactly the kind of representation drift that makes backend audits noisy and future maintenance harder.

**Goals**

* Represent collection builtin operations explicitly as compiler-owned operations.
* Remove synthetic “pretend method” compatibility paths where they no longer carry semantic value.
* Keep call-site mutability rules strict and explicit.
* Make collection lowering easier to audit in JS and HTML/Wasm runtime-heavy tests.

**Non-goals**

* No change to user-facing collection syntax in this PR.
* No redesign of collection semantics or error-return behavior.
* No broad container-type redesign.

**Implementation guidance**

#### 1. Replace method-shaped collection builtin representation

Audit how collection builtins currently move through AST/HIR/backend lowering.

The target shape should make it obvious that these are not normal receiver methods. Choose one current representation and thread it through:

**Preferred direction**

* add a dedicated compiler-owned builtin operation representation for collection operations

Possible shapes:

* dedicated AST node variants such as:

  * `CollectionGet`
  * `CollectionSet`
  * `CollectionPush`
  * `CollectionRemove`
  * `CollectionLength`
* or a smaller shared builtin-op enum if that keeps lowering cleaner

Avoid keeping synthetic method paths just to preserve the old AST shape.

#### 2. Remove compatibility-only dispatch artifacts

Clean up compatibility-shaped pieces such as:

* synthetic builtin method path for `set`
* collection-op lowering that depends on pretending there is a normal method symbol behind the syntax
* any compatibility branch retained only because older AST/HIR/backend shapes expected methods everywhere

Keep only what is still semantically justified.

#### 3. Re-audit mutability and place validation at the builtin boundary

Use this PR to make collection builtin validation visibly consistent with the language guide:

* mutating collection operations require explicit mutable/exclusive access at the receiver site
* non-mutating operations reject unnecessary `~`
* mutating operations require a mutable place receiver
* indexed-write / `get(index) = value` behavior remains explicit and compiler-owned

The parser/frontend diagnostics for these cases should stay clear and specific.

#### 4. Simplify HIR/backend lowering contracts

Once AST stops pretending these are methods, lower them through a smaller explicit contract.

Target result:

* HIR and JS lowering do not need to infer “is this really a collection builtin disguised as a method call?”
* lowering logic can switch on a dedicated builtin-op kind
* collection get/set/remove/push/length semantics become easier to test directly

#### 5. Re-check JS runtime helper usage against frontend semantics

Audit the emitted JS/runtime behavior for:

* `get`
* `set`
* `push`
* `remove`
* `length`

Specifically check for “working by accident” behavior and for any mismatch between current frontend validation and runtime helper semantics.

#### 6. Strengthen backend-facing coverage

Expand tests so collection behavior is not only parser/frontend-covered but also backend-contract-covered.

Add or improve cases for:

* successful `get/set/push/remove/length`
* out-of-bounds `get`
* explicit mutable receiver requirement for mutating ops
* indexed write forms
* result propagation/fallback after `get`
* HTML-Wasm runtime-sensitive collection paths where emitted runtime behavior matters

**Primary files to audit**

* `src/compiler_frontend/ast/field_access/collection_builtin.rs`
* `src/compiler_frontend/ast/field_access/mod.rs`
* relevant AST/HIR lowering files for method/builtin calls
* JS runtime helper emission and expression/statement lowering
* integration fixtures covering collection operations

**Checklist**

* Introduce one explicit representation for collection builtins.
* Remove synthetic method-path compatibility scaffolding where it is no longer needed.
* Keep parser/frontend mutability/place validation aligned with the language rules.
* Thread the new builtin-op shape through HIR/backend lowering.
* Re-audit JS runtime semantics for all collection builtins.
* Add backend-facing and HTML-Wasm-sensitive regression coverage.
* Remove stale compatibility branches and comments once the new shape lands.

**Done when**

* Collection builtins no longer depend on fake method-dispatch representation.
* AST/HIR/backend code treats collection ops as compiler-owned operations explicitly.
* Mutability/place diagnostics remain clear and correct.
* JS/backend tests prove collection behavior directly rather than indirectly through compatibility shape.

**Implementation notes for the later execution plan**

* Keep the representation change central and mechanical: choose one shape and thread it through.
* Avoid adding a second abstraction layer just to preserve old code.
* Land this before or alongside the JS backend semantic audit so the audit sees the final builtin representation.


### PR - Split the JS runtime prelude by concern and harden backend helper contracts

The JS backend runtime prelude currently centralizes too many unrelated helper groups in one file. Split it into focused modules, keep one small orchestration layer, and add stronger tests around the helper contracts that define Alpha runtime semantics.

**Why this PR exists**

The JS backend is the near-term stable backend and one of the main Alpha product surfaces. The runtime prelude is readable and well commented, but it is still too broad in one file: bindings, aliasing, computed places, cloning, errors, results, collections, strings, and casts all live together. That makes semantic auditing, targeted refactors, and regression testing harder than they need to be.

**Goals**

* Split the JS runtime helper emission into small focused modules.
* Preserve the current runtime semantics exactly unless a bug is being intentionally fixed.
* Make helper-group ownership obvious.
* Strengthen targeted tests for each helper surface.

**Non-goals**

* No wholesale JS backend redesign.
* No formatting/style churn unrelated to helper extraction.
* No user-facing language changes.

**Implementation guidance**

#### 1. Split `prelude.rs` into focused runtime helper modules

Refactor the current prelude into a small orchestration module plus focused helper emitters.

**Suggested structure**

* `src/backends/js/runtime/mod.rs`
* `src/backends/js/runtime/bindings.rs`
* `src/backends/js/runtime/aliasing.rs`
* `src/backends/js/runtime/places.rs`
* `src/backends/js/runtime/cloning.rs`
* `src/backends/js/runtime/errors.rs`
* `src/backends/js/runtime/results.rs`
* `src/backends/js/runtime/collections.rs`
* `src/backends/js/runtime/strings.rs`
* `src/backends/js/runtime/casts.rs`

The top-level emitter should only own:

* helper emission order
* high-level comments about why these groups exist
* any tiny shared glue that genuinely belongs at orchestration level

#### 2. Keep helper boundaries semantically intentional

Use the split to make helper responsibilities clearer:

* binding helpers: reference record construction, parameter normalization, read/write resolution
* alias helpers: borrow/value assignment semantics
* computed-place helpers: field/index place access
* clone helpers: explicit `copy` semantics
* error helpers: canonical runtime `Error` construction and context helpers
* result helpers: propagation and fallback behavior
* collection helpers: runtime contracts for ordered collections
* string helpers: string coercion and IO
* cast helpers: numeric/string cast behavior and result-carrier error paths

Avoid “misc” modules. Keep each file narrow.

#### 3. Re-check helper APIs for accidental overlap or leakage

During extraction, audit whether helper groups expose duplicated or cross-cutting behavior that should be simplified.

Examples to watch for:

* collection helpers depending on unrelated error-helper details without a clean boundary
* result helpers assuming too much about caller lowering shape
* alias/binding helpers carrying responsibilities that belong in computed-place helpers

Do not redesign aggressively; just remove obvious leakage.

#### 4. Strengthen JS backend tests around runtime contracts

Add targeted tests for helper-backed semantics, not just broad output snapshots.

Focus on:

* aliasing and assignment semantics
* explicit copy behavior
* result propagation/fallback helpers
* builtin error helper lowering
* collection runtime helpers
* cast success/failure behavior
* mutable receiver / place validation paths where JS runtime behavior depends on correct lowering

Prefer targeted artifact assertions or rendered-output assertions where full JS snapshots are noisy.

#### 5. Keep comments strong while reducing file breadth

The current prelude comments are useful. Preserve that quality after the split:

* each runtime helper file gets a short module doc comment
* each emitter function explains WHAT/WHY at the group level
* avoid repeating a giant duplicated overview in every file

**Primary files to touch**

* `src/backends/js/prelude.rs`
* `src/backends/js/mod.rs`
* JS backend tests and integration fixtures with runtime-heavy behavior

**Checklist**

* Split the JS runtime prelude into focused helper-group modules.
* Keep one small orchestration layer responsible for emission order.
* Preserve current helper semantics unless fixing an identified bug.
* Audit for duplicated or leaked helper responsibilities during extraction.
* Add or expand targeted tests for helper-backed runtime semantics.
* Prefer targeted assertions over brittle full-file snapshots where code shape is not the contract.

**Done when**

* No single JS runtime helper file owns most of the backend runtime surface.
* Helper-group ownership is obvious from file layout.
* Existing JS semantics remain stable.
* Runtime-heavy test coverage is stronger and lower-noise than before.

**Implementation notes for the later execution plan**

* Keep the first pass mostly structural.
* Only fix helper semantics in the same PR when the bug is obvious and covered.
* This PR should make the later “JS backend semantic audit for Alpha surface” materially easier.

### PR - Migrate remaining brittle fixtures, prune redundant coverage, and close the Alpha test matrix gaps

Now that the integration runner supports strict goldens, normalized goldens, rendered-output assertions, and targeted artifact assertions, finish migrating brittle fixtures to the right assertion surface, remove redundant cases that no longer add value, and fill the most visible Alpha-surface coverage gaps.

**Why this PR exists**

The integration runner is already capable of lower-noise assertion modes. The remaining work is fixture migration and coverage curation: some fixtures are still too brittle for what they actually test, some gaps remain visible in the language surface matrix, and some older coverage is now redundant or weaker than newer canonical cases.

**Goals**

* Migrate remaining brittle fixtures to normalized, rendered-output, or targeted artifact assertions where appropriate.
* Keep strict byte-for-byte goldens only where exact output shape is actually the contract.
* Fill the clearest remaining Alpha-surface gaps.
* Remove or rewrite redundant tests that duplicate stronger canonical coverage.
* Keep the matrix and manifest aligned with the real supported surface.

**Non-goals**

* No broad feature expansion.
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

* update `docs/language-surface-integration-matrix.md`
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


## Phase 5 - cross-platform consistency and test stability

### PR - Finish CRLF normalization in strings and templates

Remove avoidable Windows/macOS golden drift from source normalization and emitted outputs.

**Checklist**
- Audit remaining CRLF behavior in strings, templates, and emitted output.
- Make sure normalized newline handling is consistent through the frontend and builder outputs.
- Add regression tests specifically for Windows-shaped input.

**Done when**
- Golden outputs are stable across normal Windows/macOS workflows.

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

## Notes and limitations from previous investigations
- The WASM backend can't handle Choice/Union types yet (maps to Handle but produces i32/i64 mismatches). 
- rt_string_from_i64 Wasm helper: Explicitly noted in the 1ac2613 commit message as an "incremental bridge implementation". It produces valid output but is not a complete runtime implementation. This is scoped for a dedicated follow-up and does not cause panics.
