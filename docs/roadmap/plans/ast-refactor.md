### REVERT MISTAKEN AST DRIFT

The regression began when AST was changed from:
- a consumer of header-owned top-level declaration knowledge

into:
- a pass-driven stage that rebuilds module-wide declaration/index state from sorted headers before lowering bodies.

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

# MERGE TOP-LEVEL SYMBOL COLLECTION BACK INTO HEADER PARSING

The current `symbol_manifest` stage fixes AST ownership drift, but it does so by introducing an extra packaging pass between dependency sorting and AST construction.

That is cleaner than AST recollecting declarations itself, but it is still one pass too many.

The intended architecture is simpler:

1. Tokenize files
2. Parse file headers
   - collect all top-level declaration knowledge here
   - collect file/import/export/source metadata here
   - merge builtin/reserved top-level symbol visibility here
3. Dependency sort headers
4. Lower sorted headers directly into AST
   - AST consumes header-owned top-level knowledge
   - AST still performs module-wide type/signature/catalog preparation before body emission
   - AST does not rebuild or repackage the module symbol universe as a separate stage

## Correct architecture to restore

Header parsing should own the full top-level symbol collection package for the module.

That package should include:

- top-level declaration stubs
- file import metadata
- export visibility metadata
- declared symbol paths by file
- declared symbol names by file
- canonical source-file mapping for declarations
- module file path set
- builtin-visible symbol paths
- builtin struct/type payloads needed by AST
- any other top-level symbol facts AST/import visibility need later

Dependency sorting should then reorder headers without forcing a second top-level recollection or manifest-construction step.

AST should receive:

- sorted headers
- header-owned top-level symbol data already prepared for consumption

AST still needs module-wide pre-body semantic preparation where the language requires it:

- import visibility resolution
- constant/type resolution
- function signature resolution
- receiver-method catalog construction

But this is AST semantic work, not top-level declaration recollection.

## Why this change exists

The current `symbol_manifest` stage is mostly packaging work, not new language work.

It exists because AST had drifted into rebuilding module-wide declaration/index state from sorted headers before lowering bodies.

That drift should be fixed at the ownership boundary, not preserved as a first-class compiler stage.

The simpler model is:

- header parsing discovers top-level declarations once
- dependency sorting orders them
- AST consumes them

This reduces extra pass churn, keeps stage ownership sharper, and makes the frontend easier to reason about.

## Goals

- Remove `symbol_manifest` as a distinct frontend stage
- Move top-level symbol collection fully into header parsing ownership
- Preserve the current cleaner AST ownership split
- Keep AST focused on semantic lowering work, not declaration packaging
- Avoid reintroducing AST-side recollection or duplicate symbol databases
- Keep the pipeline aligned with the compiler design docs

## Non-goals

- Do not force AST into a purely streaming one-header-at-a-time model if current language semantics still require module-wide pre-body semantic preparation
- Do not move function-signature resolution, receiver catalog construction, or similar AST semantic work into header parsing
- Do not add compatibility wrappers or parallel transitional APIs
- Do not preserve `symbol_manifest` under a different name if it is still just a separate packaging stage

## Target frontend shape

Desired flow:

1. `parse_headers(...)`
   - parses headers
   - collects top-level declarations and metadata
   - merges builtin/reserved top-level symbol knowledge
   - returns a header-owned module symbol package together with headers / fragment metadata

2. `resolve_module_dependencies(...)`
   - sorts headers
   - preserves or reorders the header-owned declaration package as needed
   - returns one sorted-header result object

3. `Ast::new(...)`
   - consumes sorted headers plus header-owned symbol data
   - resolves import visibility
   - resolves types/constants/signatures
   - builds receiver catalog
   - emits AST nodes
   - finalizes templates / const fragments / module constants

## Work to do

### 1. Move symbol-manifest construction logic into header parsing ownership

Files:

- `src/compiler_frontend/headers/parse_file_headers.rs`
- helper files extracted from header parsing if needed
- `src/compiler_frontend/symbol_manifest.rs` (remove after migration)

The logic currently in `build_symbol_manifest(...)` should be relocated into header parsing ownership.

Header parsing should directly collect:

- `declarations`
- `canonical_source_by_symbol_path`
- `module_file_paths`
- `file_imports_by_source`
- `importable_symbol_exported`
- `declared_paths_by_file`
- `declared_names_by_file`
- builtin-visible symbol paths
- builtin struct/type metadata needed later by AST

This does not mean everything must stay in one giant file.
Helpers can and should be extracted if they improve readability.

But ownership should be header-stage ownership, not a later manifest-building stage.

### 2. Introduce a header-owned module symbol data type

Files:

- `src/compiler_frontend/headers/parse_file_headers.rs`
- possibly a new header-adjacent helper file if needed

Create a single header-owned struct returned by header parsing.

Possible shape:

- `Headers`
  - `headers`
  - `top_level_const_fragments`
  - `entry_runtime_fragment_count`
  - `module_symbols` or similar

The important part is not the exact name.
The important part is that the data is clearly owned by the header stage.

This struct should replace the need for a later standalone `SymbolManifest`.

### 3. Merge builtin/reserved top-level symbol registration into the header-owned package

Files:

- `src/compiler_frontend/headers/parse_file_headers.rs`
- builtin registration helpers currently used by `symbol_manifest`
- any identifier/reserved-symbol helpers that fit better near header collection

Builtin error types and reserved builtin symbol validation should be merged into the same top-level symbol collection flow.

That means:

- reserved builtin symbol rejection still happens before illegal declarations are accepted
- builtin-visible symbols are registered once
- builtin struct/type payloads are prepared once
- AST no longer needs a later “absorb builtins into manifest” step

### 4. Rework dependency sorting to preserve header-owned symbol data

Files:

- `src/compiler_frontend/module_dependencies.rs`
- `src/compiler_frontend/mod.rs`

Dependency sorting should operate on the header-owned package, not force a later reconstruction step.

Two acceptable shapes:

- sort only `headers` while carrying the already-collected symbol package alongside them
- or return a new sorted wrapper object that includes both sorted headers and the already-prepared top-level symbol data

The result should be that nothing after dependency sorting needs to “rebuild the manifest.”

### 5. Remove `symbol_manifest` from the public frontend pipeline

Files:

- `src/compiler_frontend/mod.rs`
- `src/compiler_frontend/symbol_manifest.rs`
- any tests or helper code referencing the separate stage

Delete:

- `pub fn build_symbol_manifest(...)`
- the `symbol_manifest` module from the visible pipeline
- the extra call site between `sort_headers()` and `headers_to_ast()`

The frontend flow should go directly from:

- header parsing
- dependency sorting
- AST construction

with no standalone manifest step in between.

### 6. Update AST construction to consume header-owned symbol data directly

Files:

- `src/compiler_frontend/ast/module_ast/build_state.rs`
- `src/compiler_frontend/ast/module_ast/orchestrate.rs`
- `src/compiler_frontend/mod.rs`

`AstBuildState::new(...)` should take the header-owned symbol package directly.

This should preserve the good part of the current refactor:

- AST starts with the top-level symbol data already known
- AST does not recollect declarations from headers

But the source of that data is now header-owned, not manifest-stage-owned.

### 7. Keep AST semantic pre-body passes, but make their ownership explicit

Files:

- `src/compiler_frontend/ast/module_ast/pass_import_bindings.rs`
- `src/compiler_frontend/ast/module_ast/pass_type_resolution.rs`
- `src/compiler_frontend/ast/module_ast/pass_function_signatures.rs`
- `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
- `src/compiler_frontend/ast/module_ast/orchestrate.rs`

Do not collapse these semantic passes into header parsing.

Instead, clarify the boundary:

Header parsing owns:
- top-level symbol discovery
- top-level symbol metadata collection
- builtin/reserved top-level symbol package assembly

AST owns:
- import visibility resolution from that package
- semantic type/signature resolution
- receiver-method catalog construction
- body lowering
- template normalization/finalization

This is the clean split.

### 8. Remove stale terminology that still describes a separate manifest stage

Files:

- `src/compiler_frontend/mod.rs`
- `src/compiler_frontend/ast/module_ast/orchestrate.rs`
- `src/compiler_frontend/headers/parse_file_headers.rs`
- `docs/compiler-design-overview.md`
- `docs/roadmap/plans/...` files touched by the refactor

Update comments/docs so they no longer describe:

- `symbol_manifest` as a real pipeline stage
- AST as consuming a separately-built manifest stage
- header parsing as discovering headers but not owning the full top-level symbol package

The docs should consistently describe the restored ownership model.

## Suggested implementation order

### Phase 1 — move ownership without changing semantics

- create the header-owned module symbol package type
- move `symbol_manifest` data-building logic under header parsing ownership
- keep AST call sites semantically identical
- keep sorting behavior identical
- do not attempt broader AST simplification in the same step

### Phase 2 — remove the standalone stage

- thread the header-owned symbol package through dependency sorting
- remove `build_symbol_manifest(...)`
- remove the extra frontend stage wiring
- update AST to consume the header-owned package directly

### Phase 3 — cleanup and documentation alignment

- delete `symbol_manifest.rs`
- simplify comments and stage docs
- remove stale wording about manifest construction
- clean up dead fields / unused helpers / transitional names

## Checklist

- Move top-level symbol-package construction under header parsing ownership
- Merge builtin/reserved top-level symbol registration into that ownership boundary
- Return header-owned symbol data alongside parsed headers
- Thread that data through dependency sorting without rebuilding it
- Remove `symbol_manifest` as a standalone frontend stage
- Update AST to consume header-owned top-level symbol data directly
- Keep AST semantic pre-body passes intact and clearly documented
- Delete stale manifest-stage docs/comments/code
- Run:
  - `cargo clippy`
  - `cargo test`
  - `cargo run tests`

## Done when

- `symbol_manifest` no longer exists as a distinct compiler/frontend stage
- header parsing owns the full top-level symbol collection package
- dependency sorting preserves that package instead of forcing a later rebuild
- AST consumes header-owned top-level symbol data directly
- AST no longer rebuilds or repackages module-wide top-level symbol knowledge
- docs/comments consistently describe the restored ownership split
- touched code passes linting and tests

## Notes for the later implementation PR

- Keep this change narrowly about stage ownership and pass removal
- Do not mix it with unrelated AST simplifications unless they are required by the threading change
- Prefer one clear data flow over parallel temporary structs
- Prefer deleting the manifest stage cleanly over renaming it and keeping the same architecture
- If helper extraction is needed, extract helpers from header parsing, but keep ownership there


# Part 4 - Migrate remaining brittle fixtures, prune redundant coverage, and close the Alpha test matrix gaps

Now that the integration runner supports strict goldens, normalized goldens, rendered-output assertions, and targeted artifact assertions, finish migrating brittle fixtures to the right assertion surface, remove redundant cases that no longer add value, and fill the most visible Alpha-surface coverage gaps.

This will also be part of fixing the current failing tests and making sure they are correctly refactored to be less brittle and more complete with the new AST architecture.

**Why this PR exists**

The integration runner is already capable of lower-noise assertion modes. The remaining work is fixture migration and coverage curation: some fixtures are still too brittle for what they actually test, some gaps remain visible in the language surface matrix, and some older coverage is now redundant or weaker than newer canonical cases.

Focus areas:

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

The goal is to finish the refactor cleanly. This phase makes the new AST pipeline readable, aligned with the docs, and free of leftover lint/style drift.

Desired design:

* the AST pipeline is easy to follow in `orchestrate.rs`
* comments explain what each stage is doing and why it exists in the overall compiler pipeline
* touched code follows the style guide and compiler design docs
* tests reflect the new ownership model, not the old recollection model

Done so far:

* the top-level template rewrite already removed a large amount of fragment-specific complexity
* the remaining work is now mostly structural cleanup, documentation alignment, and test/lint follow-through

## Work to do

### 1. Simplify `orchestrate.rs` to match the final architecture 

Files:

* `src/compiler_frontend/ast/module_ast/orchestrate.rs`
* `src/compiler_frontend/mod.rs`

After Parts 1–3, rewrite `orchestrate.rs` so the pass sequence reflects the real pipeline:

1. consume shared top-level symbol manifest
2. resolve import visibility and type/signature tables
3. lower sorted headers directly
4. finalize const fragments, doc fragments, template normalization, and module constants

Remove all stale wording that still describes AST as reconstructing declarations first  

### 2. Review the touched areas against the style guide and compiler design docs

Files:

* all files touched by this plan
* `docs/compiler-design-overview.md`
* `docs/codebase-style-guide.md`

Do a deliberate alignment pass:

* stage ownership matches the design overview
* module boundaries remain clear
* no transitional wrappers or compatibility shims remain
* comments explain behavior and rationale, not syntax
* error paths use structured diagnostics and avoid user-input panics
* naming stays explicit and full
* files and functions still have one clear responsibility  

### 3. Add strong comments to the new AST pipeline key parts

Files:

* `src/compiler_frontend/ast/module_ast/orchestrate.rs`
* `src/compiler_frontend/ast/module_ast/build_state.rs`
* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* `src/compiler_frontend/ast/import_bindings.rs`
* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`

The newly refactored AST pipeline key parts should be clearly commented.

Comments should explain:

* what this stage owns
* what it no longer owns
* why header parsing now owns top-level declaration discovery
* how AST uses the shared manifest
* how local scope growth works during body lowering
* how entry `start()` and const fragment finalization relate to the big picture

Use concise WHAT/WHY comments and file-level docs. The style guide requires this level of explanation for complex stage logic 

### 5. Clean remaining lints and dead code

This final phase must explicitly include:

* cleaning up any remaining `clippy` lints
* reviewing dead code, stale `#[allow(dead_code)]`, and leftover unused paths
* removing stale comments and docs from the old architecture
* running the required checks from the style guide:

  * `cargo clippy`
  * `cargo test`
  * `cargo run tests` 

## Done when

* `orchestrate.rs` reflects the final AST pipeline clearly
* touched code follows the style guide and compiler design docs
* key AST pipeline files are well commented with clear WHAT/WHY and stage ownership
* tests validate the new architecture
* remaining clippy lints in touched areas are cleaned up
* no stale architectural comments from the old model remain