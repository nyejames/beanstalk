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


### Part 1.5 — Thread entry runtime fragment count through the frontend/build boundary

#### Overview

The goal is to make the entry-page fragment contract fully explicit.

Header parsing already counts entry-file top-level runtime templates while recording const-fragment `runtime_insertion_index`, but that runtime count is not yet exposed as builder-facing metadata. JS still re-discovers it by scanning HIR, and the HTML Wasm path still has fragment-return TODOs tied to the current dynamic list model.

Desired design:

* header parsing owns the authoritative `entry_runtime_fragment_count`
* builders receive that count directly alongside const fragment metadata
* JS and Wasm builder paths do not scan HIR to rediscover slot count
* HIR may still use `PushRuntimeFragment` as a lowering primitive, but it is not the builder contract

#### Why this belongs here

Part 1 already moved const top-level fragment ordering out of HIR and into builder-facing metadata. This change finishes that cleanup for the runtime side without reintroducing a builder-facing fragment stream into HIR.

#### Work to do

##### 1. Expose runtime fragment count from header parsing

Files:

* `src/compiler_frontend/headers/parse_file_headers.rs`

Header parsing already tracks `runtime_fragment_count` for the entry file while it parses top-level runtime templates and assigns const fragment insertion indices.

Promote that into explicit output metadata.

Recommended shape:

* add `entry_runtime_fragment_count: usize` to `Headers`
* document that only the entry file contributes to this count
* keep `TopLevelConstFragment.runtime_insertion_index` unchanged

This keeps header parsing as the single owner of entry fragment discovery metadata. 

##### 2. Thread the count through frontend orchestration and module build payloads

Files:

* `src/compiler_frontend/mod.rs`
* `src/build_system/create_project_modules/frontend_orchestration.rs`
* `src/build_system/build.rs`

The frontend already resolves const fragment strings before AST is consumed by HIR and stores them on the build `Module`.

Thread `entry_runtime_fragment_count` through the same boundary:

* preserve it when `Headers` is split and sorted
* add it to the frontend→builder `Module` payload next to `const_top_level_fragments`
* keep it out of HIR

This matches the existing builder-facing const fragment ownership model.  

##### 3. Remove JS-side HIR slot recounting

Files:

* `src/projects/html_project/js_path.rs`

`js_path.rs` currently derives slot count by walking `PushRuntimeFragment` statements in the entry start function.

Replace that with direct builder metadata:

* remove `count_runtime_fragment_slots(...)`
* pass `entry_runtime_fragment_count` into `render_html_document(...)` / `render_entry_fragments(...)`
* generate runtime slot placeholders directly from the explicit count

This removes the remaining builder dependency on entry-body HIR inspection. 

##### 4. Feed the Wasm HTML path with the same explicit count

Files:

* `src/projects/html_project/wasm/js_bootstrap.rs`
* relevant HTML Wasm builder wiring

The Wasm bootstrap already uses builder-owned slot hydration shape, but the fragment-return path is still TODO-shaped.

Thread `entry_runtime_fragment_count` into the Wasm HTML builder/bootstrap path so that:

* slot IDs are generated from builder metadata, not inferred later
* future runtime fragment decoding/lowering can target a fixed-count contract cleanly
* the remaining TODO is only about decoding/lowering the returned fragment values, not discovering how many there are

This keeps the export plan unchanged while giving the Wasm path the metadata it actually needs.  

##### 5. Keep HIR out of the builder contract

Files:

* `src/compiler_frontend/hir/hir_nodes.rs`
* any builder call sites that currently inspect entry-body HIR shape

Do not add a builder-facing fragment count field to HIR.

`PushRuntimeFragment` can remain as a backend-lowering primitive for entry `start()`, but builders should not rely on scanning HIR statements to recover page assembly metadata.

That preserves the Part 1 direction: builder-facing fragment metadata lives outside HIR. 

##### 6. Update docs and tests

Files:

* `docs/roadmap/plans/ast-refactor.md`
* `docs/compiler-design-overview.md`
* header parsing tests
* HTML builder tests

Update docs to say header parsing now emits:

* const fragment insertion metadata
* entry runtime fragment count

Add tests for:

* header parsing reports the correct runtime fragment count for entry files
* JS builder no longer depends on HIR slot recounting
* Wasm bootstrap receives the explicit runtime slot count

#### Done when

* `Headers` exposes `entry_runtime_fragment_count`
* frontend orchestration passes that count into the build `Module`
* JS HTML no longer scans HIR to count runtime fragment slots
* Wasm HTML receives the same explicit count metadata
* HIR does not regain a builder-facing fragment metadata role





# Part 2 — remove AST-side module declaration recollection

## Overview

The goal is to restore the original ownership split: headers and dependency sorting own top-level declaration discovery, and AST lowers sorted headers without rebuilding the module symbol manifest.

Desired design:

* top-level declaration discovery is header-owned
* AST does not recollect module-wide declarations
* AST does not rebuild per-file declared-path/name tables
* AST consumes a shared top-level symbol manifest prepared before body lowering

## Things to preserve

Do not remove or redesign the current local declaration path inside function bodies.
Keep:
- `new_declaration(...)`
- `context.add_var(...)`
- local declaration insertion in source order as statements are parsed

That part still matches the original architecture.

Done so far:

* header parsing already owns top-level declaration discovery and start-function classification in the way described by the compiler design overview  
* bare file imports and imported file starts are already gone on the language-rule side, which removes one reason AST previously had extra import/start bookkeeping 

## Work to do

### 1. Delete the declaration recollection pass

Files:

* `src/compiler_frontend/ast/module_ast/pass_declarations.rs`
* `src/compiler_frontend/ast/module_ast/orchestrate.rs`

`pass_declarations.rs` still rebuilds a module-wide declaration and visibility database inside AST, including start declaration stubs and builtin absorption 

Delete this pass. Remove `collect_declarations(...)` from `orchestrate.rs` and remove the stale pass-order comments that still present AST construction as declaration collection first 

### 2. Move symbol-manifest ownership out of AST

Files:

* `src/compiler_frontend/headers/parse_file_headers.rs`
* `src/compiler_frontend/mod.rs`
* `src/compiler_frontend/module_dependencies.rs`
* new manifest module if needed

Introduce a frontend-owned manifest that is prepared before AST construction.

Recommended shape:

* top-level declaration stubs
* export visibility
* per-file visible symbol sets
* canonical symbol-to-source mapping
* file import metadata
* builtin manifest merged once here

AST should receive this manifest and consume it. It should not reconstruct it.

### 3. Update AST construction API to consume the manifest

Files:

* `src/compiler_frontend/mod.rs`
* `src/compiler_frontend/ast/module_ast/orchestrate.rs`

`mod.rs` and `orchestrate.rs` still reflect the old ownership model in both comments and inputs  

After the manifest exists, update the AST entrypoint so it takes:

* sorted headers
* top-level const fragment metadata
* shared symbol manifest
* existing AST build context

AST construction should then begin from manifest-driven visibility/type resolution and ordered header lowering, not from symbol recollection.

## Done when

* `pass_declarations.rs` is deleted
* `collect_declarations(...)` no longer exists in AST orchestration
* the top-level symbol manifest is created before AST and passed in
* AST no longer owns module declaration discovery

# Part 3 — shrink AST state and replace cloned declaration scopes

## Overview

The goal is to finish the real AST simplification. Once recollection is removed, `AstBuildState` and `ScopeContext` can be reduced to true AST-stage responsibilities and layered scope growth.

Desired design:

* `AstBuildState` only carries AST-stage state
* `ScopeContext` does not clone a full module declaration vec into every child scope
* top-level declarations come from the shared manifest
* parameters and locals grow incrementally in source order

Done so far:

* the AST body-lowering path already has the right semantic direction for start/runtime template handling
* import semantics are already simpler because start aliasing is gone
* the remaining complexity is now concentrated in `AstBuildState`, `ScopeContext`, and the orchestration around them

## Work to do

### 1. Shrink `AstBuildState`

File:

* `src/compiler_frontend/ast/module_ast/build_state.rs`

`AstBuildState` still stores a second symbol database:

* `importable_symbol_exported`
* `file_imports_by_source`
* `declared_paths_by_file`
* `declared_names_by_file`
* `module_file_paths`
* `canonical_source_by_symbol_path`
* `register_declared_symbol(...)` 

Move that state to the shared manifest layer. Keep only true AST-stage state:

* emitted AST nodes
* warnings
* module constants
* folded const template values
* resolved type/signature tables
* rendered path usage sink
* builtin AST payloads only if still needed after manifest construction

### 2. Rewrite `ScopeContext` around layered scope data

Files:

* `src/compiler_frontend/ast/module_ast/scope_context.rs`
* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
* any body-lowering helpers that assume cloned full declaration vectors

`ScopeContext` still stores `declarations: Vec<Declaration>` and clones it into child contexts. `new_child_function`, `new_template_parsing_context`, `new_constant`, and `add_var(...)` still operate in that model 

Replace it with:

* immutable shared top-level declaration view from the manifest
* small local declaration layer for parameters and locals
* `add_var(...)` only extends the local layer
* visibility gating still applied per file as needed

This is the core fix for source-ordered local growth without carrying a cloned module declaration vec everywhere.

### 3. Rework AST emission and import visibility to use the manifest + layered scopes

Files:

* `src/compiler_frontend/ast/module_ast/pass_emit_nodes.rs`
* `src/compiler_frontend/ast/import_bindings.rs`
* constant-header/type-resolution call sites

`import_bindings.rs` already enforces the correct import rules, but it still resolves visibility against AST-owned symbol tables populated by recollection 

Rewire it so:

* visibility gates come from the shared manifest
* constant-header resolution consumes manifest data plus per-file visible symbols
* `pass_emit_nodes.rs` builds function/start contexts from the manifest + layered local scope model, not from large prebuilt declaration vecs

## Done when

* `AstBuildState` no longer stores a second symbol registration database
* `ScopeContext` no longer clones the full module declaration vec into child contexts
* file visibility is resolved from the shared manifest
* function and start bodies grow only local/parameter scope incrementally

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