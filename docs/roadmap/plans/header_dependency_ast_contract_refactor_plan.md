# Beanstalk Header / Dependency Sorting / AST Contract Refactor Plan

## Purpose

This plan corrects the frontend stage contract before continuing the wider AST restructure and optimisation work.

The goal is to make the frontend pipeline behave like this:

```text
Tokenization
  -> Header parsing / import preparation / declaration shell creation
  -> Dependency sorting of all top-level declaration dependencies
  -> AST linear declaration resolution and body emission
```

After dependency sorting, AST should be able to walk sorted headers in order. It should not build another top-level ordering graph, rediscover imports, or reconstruct top-level declaration shells from raw tokens.

This plan supersedes the parts of the current AST refactor that tried to compensate inside AST for missing header/dependency data, especially the newly added AST constant graph.

## Design rule

> If AST cannot resolve a top-level declaration by walking sorted headers in order, the missing dependency belongs in header parsing or dependency sorting, not in a new AST ordering pass.

## Immediate problem to fix

The current repo has drifted back into duplicate work:

- `src/compiler_frontend/module_dependencies.rs` performs topological dependency sorting.
- `src/compiler_frontend/ast/module_ast/environment/constant_graph.rs` performs a second topological sort for constants.
- `src/compiler_frontend/ast/import_bindings.rs` still owns import binding and file-local visibility construction, even though the desired contract puts this in the header/import preparation stage.
- `src/compiler_frontend/headers/header_dispatch.rs` already parses constant declaration shells and collects `DeclarationSyntax.initializer_references`, but currently only adds dependency edges from explicit constant type annotations.
- `src/compiler_frontend/headers/file_parser.rs` already separates imports, top-level declarations, top-level const fragments, and entry `start` body tokens, but the parsed import data is not yet fully resolved into the stage contract AST should consume.

This plan moves the missing work to the correct stage and deletes the AST-side compensating systems.

## Non-goals

This plan does **not** redesign the full `DataType` / `TypeEnvironment` representation. That remains a separate follow-up plan.

This plan does **not** rewrite expression parsing or constant folding. AST still parses and folds constant initializer expressions semantically. Header parsing only creates declaration shells and dependency/reference data.

This plan does **not** move function/start executable body parsing into headers. Function bodies and entry `start` body tokens remain AST-owned.

This plan does **not** add compatibility shims for old AST import binding or constant graph flows. Beanstalk is pre-alpha; when ownership moves, old paths should be removed.

## Required benchmark and validation gate

Every code-changing phase must update the benchmark log and end green.

Before implementing the phase:

```bash
just bench
```

During implementation, optional directional checks:

```bash
just bench-quick
```

After implementing the phase:

```bash
just bench
just validate
```

Each phase commit must include:

- code changes
- benchmark log update with before/after `just bench` summary paths and key values
- relevant documentation/comment updates
- audit notes

Generated files in `benchmarks/results/` must not be committed. Commit only summarized results in the benchmark log.

## Benchmark log

Use the existing AST benchmark log:
`docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md`

If the file does not exist yet, create it in Phase 0.

For this plan, add a section named:

```text
Header / Dependency Sorting / AST Contract Refactor
```

Each phase entry should include:
- phase name
- before benchmark run directory / summary path
- after benchmark run directory / summary path
- key benchmark rows: check/build speed-test, check docs, relevant stress cases
- relevant detailed timer changes: headers, dependency sorting, AST/build environment, AST/constants
- regression classification
- notes / follow-up optimization observations

Regression classification:
Improved:          >= 3% faster
Neutral:           within ±3%
Regression:        >= 3% slower
Major regression:  >= 10% slower

A major regression blocks continuation unless the cause is identified and explicitly accepted.

---

# Target stage contract

## Header parsing / import preparation owns

Header parsing and header preparation should do one cheap structural pass over each file, then a module-level import/symbol preparation step.

It owns:

- import and re-export syntax parsing
- import path validation and normalization where enough context exists
- file-local import/visibility environment construction
- top-level declaration discovery
- declaration shell parsing for constants, functions, structs, choices, type aliases, const templates, and entry start headers
- generic parameter syntax attached to declarations and type surfaces
- constant initializer token capture
- constant initializer reference hint collection
- dependency edge generation for all top-level declaration dependencies required before AST can resolve linearly
- entry `start` body capture and separation from dependency-sorted headers
- top-level const fragment placement metadata
- `ModuleSymbols` construction and final header-owned metadata needed by dependency sorting and AST

## Dependency sorting owns

Dependency sorting consumes header-provided dependency edges and produces sorted headers.

It owns:

- topological sorting of top-level declaration headers
- cycle detection for top-level declaration dependencies
- missing dependency diagnostics
- source-order stability among independent declarations
- same-file constant source-order enforcement
- appending entry `StartFunction` headers after sorted declarations
- finalizing `ModuleSymbols.declarations` in sorted order

Constant initializer references to other constants are top-level declaration dependencies. They are not executable body references.

## AST owns

AST consumes sorted headers and header-built visibility/declaration metadata.

It owns:

- linear declaration resolution from sorted shells
- type alias, constant, struct, choice, and function signature semantic validation
- constant folding and const-only validation
- expression parsing and type checking
- body-local declaration shell creation and full resolution through shared declaration syntax code
- function/start/template body parsing
- receiver method cataloging once signatures are resolved
- template semantic preparation and finalization
- HIR-facing AST assembly

AST must not:

- topologically sort constants or other top-level declarations
- rebuild file import bindings from scratch
- rediscover top-level symbols from raw file tokens
- reconstruct top-level declaration shells from raw tokens
- add an ordering pass to compensate for missing header dependency edges

---

# Current code anchors

The plan is anchored in these current areas:

```text
src/compiler_frontend/headers/file_parser.rs
src/compiler_frontend/headers/header_dispatch.rs
src/compiler_frontend/headers/types.rs
src/compiler_frontend/headers/module_symbols.rs
src/compiler_frontend/module_dependencies.rs
src/compiler_frontend/ast/import_bindings.rs
src/compiler_frontend/ast/module_ast/environment/builder.rs
src/compiler_frontend/ast/module_ast/environment/type_resolution.rs
src/compiler_frontend/ast/module_ast/environment/constant_graph.rs
src/compiler_frontend/declaration_syntax/declaration_shell.rs
src/compiler_frontend/ast/statements/declarations.rs
```

Important current facts:

- `DeclarationSyntax` already stores `initializer_tokens` and `initializer_references`.
- `HeaderKind::Constant` already stores `DeclarationSyntax` and `source_order`.
- `create_constant_header_payload` currently collects dependency edges only from the explicit type annotation.
- `resolve_constant_headers` currently asks AST `constant_graph.rs` for ordered constant headers.
- `resolve_file_import_bindings` and `resolve_re_exports` currently live under `ast/import_bindings.rs` even though the desired contract makes them header/import-preparation responsibilities.
- `module_dependencies.rs` currently documents “strict-edges-only” and explicitly excludes initializer-expression symbols. This must change.
- `rayon` already exists as a root dependency, so the later parallel header phase does not require adding a new crate.

---

# Phase 0 — Contract reconciliation, benchmark baseline, and plan anchoring

## Summary

Make the intended stage contract explicit in docs and benchmark tracking before touching behavior. This prevents agents from adding another AST compensating pass later.

## Implementation steps

1. Run baseline:

   ```bash
   just bench
   ```

2. Create or update:

   ```text
   docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
   ```

   Add the baseline result under the `Header / Dependency Sorting / AST Contract Refactor` section.

3. Update `docs/compiler-design-overview.md` if the local changes are not already present.

   Required wording changes:

   - Header parsing is structural module preparation, not shallow declaration discovery.
   - Header parsing/import preparation owns import normalization and file-local visibility environment construction.
   - Constant initializer references to constants are top-level dependency edges.
   - Dependency sorting consumes all top-level declaration edges, not only “strict” type edges.
   - AST consumes sorted headers linearly and must not do top-level sorting.
   - Add a clear Header / Dependency Sorting / AST contract section.

4. Update `docs/roadmap/roadmap.md` to link this plan if it is committed into the repo, for example:

   ```text
   - Header/dependency/AST stage contract refactor: docs/roadmap/plans/header-dependency-ast-contract-refactor-plan.md
   ```

5. If the wider AST refactor plan is already committed, add a note that this plan must land before the remaining AST phases continue.

## Audit checklist

- No code behavior changes in this phase unless unavoidable.
- Documentation states that AST must not add another top-level sorter.
- Benchmark log includes the initial baseline.

## Validation

Documentation-only changes do not require `just validate`, but run it if any code or generated docs are touched.

---

# Phase 1 — Split import environment ownership out of AST

## Summary

Move import binding and visibility construction from AST into the header/import-preparation layer. AST should consume file-local visibility data, not build it.

This phase should be mostly ownership relocation and API reshaping, not behavior change.

## Implementation steps

1. Split `src/compiler_frontend/ast/import_bindings.rs`.

   Move import-related types and functions into a header-owned module, likely:

   ```text
   src/compiler_frontend/headers/import_environment.rs
   ```

   Move these concepts out of AST:

   ```text
   FileImportBindings
   resolve_file_import_bindings
   resolve_re_exports
   visible-name collision registry/helpers
   import target resolution helpers
   facade import resolution helpers
   module boundary checks
   alias case warnings
   ```

2. Keep AST constant declaration parsing separate.

   `parse_constant_header_declaration` and `ConstantHeaderParseContext` should not stay in a file named `import_bindings.rs`.

   Move them to a clearer AST module, for example:

   ```text
   src/compiler_frontend/ast/module_ast/environment/constant_resolution.rs
   ```

   This keeps the semantic fold/type-check step AST-owned while making import binding header-owned.

3. Update header exports.

   `headers/mod.rs` or equivalent should expose the header import environment types needed by dependency sorting and AST.

4. Extend `Headers` and/or `SortedHeaders` to carry the resolved file import environment.

   Target shape:

   ```rust
   pub struct Headers {
       pub headers: Vec<Header>,
       pub top_level_const_fragments: Vec<TopLevelConstFragment>,
       pub entry_runtime_fragment_count: usize,
       pub module_symbols: ModuleSymbols,
       pub file_import_bindings: FxHashMap<InternedPath, FileImportBindings>,
       pub warnings: Vec<CompilerWarning>,
   }
   ```

   Exact placement may vary, but AST must receive `FileImportBindings` from the header/dependency output, not construct it.

5. Wire `parse_headers` so that after all file headers and `ModuleSymbols` are built, it resolves re-exports and import bindings before returning `Headers`.

   This is still Stage 2/header preparation because it depends on all parsed file shells and module symbols.

6. Update `AstModuleEnvironmentBuilder::build`.

   Remove:

   ```text
   self.resolve_import_bindings(...)
   ```

   Replace it with consuming precomputed `file_import_bindings` from the sorted header bundle.

7. Update file-level doc comments.

   Required comment updates:

   - `headers/import_environment.rs`: explain it owns file-local visibility and import normalization.
   - `ast/module_ast/environment/builder.rs`: explain AST consumes header-built visibility.
   - Remove or rewrite the old `ast/import_bindings.rs` responsibility comments.

## Tests

Add or strengthen tests for:

- import alias collision still fails
- prelude/builtin alias collision still fails
- source-library facade import still works
- cross-module facade boundary still fails when bypassed
- external package imports still resolve to stable external IDs
- import warnings are preserved after moving ownership

Prefer existing integration fixtures if available. Add focused unit tests only for import-environment helper behavior that is difficult to cover end-to-end.

## Audit checklist

- No duplicate `FileImportBindings` type remains.
- No AST-owned import resolver remains under an AST module.
- AST builder does not call an import-binding construction pass.
- Import diagnostics still use structured errors and preserve locations.
- The move did not introduce compatibility wrappers.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 2 — Make constant initializer dependencies first-class header dependency edges

## Summary

Use the existing `DeclarationSyntax.initializer_references` data to generate real dependency edges for constants during header/import preparation. This replaces the reason `constant_graph.rs` exists.

The important distinction:

- Header parsing records dependency edges and ordering requirements.
- AST still parses and folds the initializer expression semantically.

## Implementation steps

1. Add a header/import-preparation function for constant initializer dependencies.

   Suggested owner:

   ```text
   src/compiler_frontend/headers/constant_dependencies.rs
   ```

   Suggested API:

   ```rust
   pub(crate) fn add_constant_initializer_dependencies(
       headers: &mut [Header],
       file_import_bindings: &FxHashMap<InternedPath, FileImportBindings>,
       module_symbols: &ModuleSymbols,
       string_table: &mut StringTable,
   ) -> Result<(), Vec<CompilerError>>
   ```

2. For each `HeaderKind::Constant`, inspect `declaration.initializer_references`.

3. Resolve each reference using the file’s `FileImportBindings`.

   Cases:

   - visible external constant: no header dependency edge
   - visible external non-constant: record a non-constant constant-initializer diagnostic, or leave to AST only if current semantics require delayed validation
   - visible type alias: no value dependency edge
   - visible source binding to another constant: add dependency edge
   - visible source binding to struct/choice constructor-like use: no constant edge; AST will validate constructor use
   - visible source binding to non-constant value: diagnostic
   - unknown but same module constant name exists and is not visible: not-visible diagnostic
   - unknown name: keep current unknown-reference diagnostic behavior, but prefer header-stage diagnostic if it can be precise

4. Preserve same-file constant source-order semantics.

   If a constant references another constant in the same source file:

   - referencing an earlier same-file constant is valid
   - referencing itself is a cycle/self-reference error
   - referencing a later same-file constant is a same-file forward-reference error

   Do not use dependency sorting to silently allow same-file forward references.

5. For cross-file constant references, add dependencies into `Header.dependencies`.

   Cross-file cycles should be detected by `module_dependencies.rs` as normal top-level dependency cycles.

6. Remove the “initializer-expression symbols are soft hints” model from code comments.

   Update `headers/header_dispatch.rs`, `declaration_shell.rs`, and any helper comments to state that initializer reference hints are used by header dependency preparation, while expression parsing remains AST-owned.

7. Add counters/timers if useful under `detailed_timers`:

   ```text
   Header/constant initializer dependencies
   Header/constant initializer dependency edges
   ```

## Tests

Add or strengthen integration cases:

### Cross-file constant dependency sorts before AST

```beanstalk
-- a.bst
# a = 1

-- b.bst
import @a/a
# b = a + 1
```

Expected: success. No AST constant graph needed.

### Same-file forward reference rejected

```beanstalk
# b = a + 1
# a = 1
```

Expected: rule error with a clear same-file forward-reference message.

### Cross-file constant cycle rejected by dependency sorting

```beanstalk
-- a.bst
import @b/b
# a = b + 1

-- b.bst
import @a/a
# b = a + 1
```

Expected: dependency-cycle diagnostic.

### Non-constant top-level reference rejected

```beanstalk
# make || -> Int:
    return 1
;

# value = make
```

Expected: constant initializer cannot reference non-constant value.

### Constructor-like references remain valid

Struct and choice constructors in constants should not be misclassified as invalid constant references if the existing language supports const records/choice construction in constants.

## Audit checklist

- Constant dependencies are added to `Header.dependencies` before `resolve_module_dependencies`.
- No AST-specific ordering is required to resolve constants.
- Same-file source-order rule is preserved.
- Diagnostics distinguish unknown, not visible, non-constant, same-file forward reference, and cycle cases where practical.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 3 — Rewrite dependency sorting around complete header-provided edges

## Summary

Dependency sorting should become the single top-level ordering authority. Its comments, diagnostics, and implementation should reflect that it now sorts all top-level declaration dependencies, including constant initializer dependencies.

## Implementation steps

1. Update the module-level doc comment in:

   ```text
   src/compiler_frontend/module_dependencies.rs
   ```

   Remove “strict-edges-only” and “initializer-expression symbols are NOT edges.”

   Replace with:

   ```text
   Dependency edges are header-provided top-level declaration dependencies. They include type-surface dependencies and constant initializer dependencies. Executable function/start body references remain excluded.
   ```

2. Audit `resolve_graph_path`.

   Because imports and paths should now be normalized earlier, dependency sorting should need less fuzzy matching over suffixes and optional `.bst` extensions.

   Do not remove fallback behavior blindly in this phase if tests rely on it. Instead:

   - add comments identifying fallback matching as legacy tolerance if it remains
   - prefer normalized canonical paths for new dependency edges
   - add follow-up cleanup notes if fuzzy matching remains

3. Ensure `Header.dependencies` contains canonical paths where possible.

   The header/import-preparation stage should normalize paths before dependency sorting.

4. Keep `StartFunction` excluded from the graph and appended last.

5. Audit facade header handling.

   Current behavior excludes `#mod.bst` facade headers and appends them near the end. Do not redesign facade semantics unless required for correctness. Document why facade headers are excluded or update the sorting model if this turns out stale.

6. Improve dependency-cycle diagnostics if constant cycles now flow through this sorter.

   The diagnostic should not imply only imports are involved when the cycle is a constant initializer cycle.

7. Ensure `ModuleSymbols.build_sorted_declarations` receives the final sorted order and no later stage mutates ordering.

## Tests

- Existing dependency sorting tests should pass.
- Add unit tests for constant initializer dependency edges if they are easier to inspect at this level.
- Add integration tests from Phase 2 if not already added there.

## Audit checklist

- There is one topological sort for top-level declarations.
- Code comments match the new stage contract.
- Dependency sorting does not mention “soft initializer hints.”
- AST is not mentioned as the owner of fixing declaration order.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 4 — Delete AST constant graph and make AST constant resolution linear

## Summary

Once constant initializer dependencies are part of header dependency sorting, `src/compiler_frontend/ast/module_ast/environment/constant_graph.rs` is duplicate work and must be removed.

AST should parse/fold constants by walking `sorted_headers` in order.

## Implementation steps

1. Delete:

   ```text
   src/compiler_frontend/ast/module_ast/environment/constant_graph.rs
   ```

2. Remove it from `environment/mod.rs` or any module declarations.

3. Remove `ordered_constant_headers` from `AstModuleEnvironmentBuilder`.

4. Update `resolve_constant_headers` in:

   ```text
   src/compiler_frontend/ast/module_ast/environment/type_resolution.rs
   ```

   Replace:

   ```rust
   let ordered_headers = self.ordered_constant_headers(...)?;
   for header in ordered_headers { ... }
   ```

   with a direct walk:

   ```rust
   for header in sorted_headers {
       let HeaderKind::Constant { .. } = &header.kind else { continue; };
       ... parse/fold constant ...
   }
   ```

5. Add an internal invariant check if useful:

   If constant parsing sees an unresolved top-level constant dependency that should have been sorted earlier, report a compiler error pointing to missing header dependency extraction.

   Do not recover by sorting again.

6. Remove AST counters related to constant graph sorting, for example:

   ```text
   ConstantTopologicalSortCount
   ConstantDependencyEdges
   ```

   Or move edge counters into the header/dependency stage if they remain useful.

7. Update timers:

   Keep:

   ```text
   AST/environment/constants resolved in
   ```

   Remove any timer suggesting AST ordered constants.

8. Update docs/comments:

   - `type_resolution.rs` should say constants are resolved in dependency-sorted header order.
   - `builder.rs` should say environment building consumes header-prepared import bindings and sorted headers.

## Tests

Run the constant dependency integration cases from Phase 2.

Add a regression test that would have failed without correct header sorting, proving AST no longer needs a constant graph.

## Audit checklist

- `constant_graph.rs` is gone.
- No AST function performs topological sorting of constants.
- No AST counter/timer refers to constant topological sorting.
- Constant resolution is a simple loop over sorted headers.
- Any missing-order problem is treated as a header/dependency bug.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 5 — Enforce shared declaration shell ownership

## Summary

Top-level header parsing and AST body-local declaration parsing must share the same shell parser. The header stage can hand AST already-created shells for top-level declarations. AST may create shells for body-local declarations, but only through the same shared `declaration_syntax` code.

This prevents duplicated syntax enforcement and avoids reparsing top-level declaration structure.

## Implementation steps

1. Audit all declaration parsing call sites.

   Search for parsing logic that duplicates `parse_declaration_syntax`, struct shell parsing, choice shell parsing, function signature parsing, type alias parsing, or multi-bind declaration parsing.

2. Clarify module ownership in `src/compiler_frontend/declaration_syntax/`.

   Add or update top-level module docs:

   ```text
   declaration_syntax owns reusable declaration shell parsing for both header top-level declarations and AST body-local declarations. Header parsing stores shells; AST resolves shells.
   ```

3. Ensure top-level AST resolution consumes shell data from `HeaderKind`.

   It should not inspect raw top-level header tokens to reconstruct:

   - constant declaration syntax
   - function signature syntax
   - struct fields
   - choice variants
   - type alias target syntax

4. Ensure body-local AST declaration parsing still creates a shell first.

   The AST body parser should call the same shell parser, then resolve the shell into a typed declaration.

5. Split or rename misleading modules.

   If a file suggests it owns “full declaration parsing” but actually owns shell parsing, rename or update comments.

6. Remove obsolete helper functions and tests that assert old duplicated behavior.

## Tests

- Existing declaration syntax tests must still pass.
- Add tests only where a duplicated parser is removed and behavior could drift.
- Prefer integration tests for user-visible syntax diagnostics.

## Audit checklist

- One shell parser owns declaration syntax.
- Header parsing stores top-level shells.
- AST resolves top-level shells but does not recreate them.
- Body-local declarations go through the same shell parser.
- Comments use “shell” consistently for structured-but-unresolved declaration payloads.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 6 — Remove remaining AST import/order assumptions and revise the AST refactor plan

## Summary

With import bindings and constant ordering moved out of AST, update the remaining AST architecture work so it does not preserve obsolete assumptions.

## Implementation steps

1. Audit `src/compiler_frontend/ast/module_ast/environment/`.

   Confirm the environment builder now receives:

   - sorted headers
   - `ModuleSymbols`
   - header-built `FileImportBindings`
   - declaration shells

   and does not build/import/sort those things itself.

2. Update `AstModuleEnvironment` fields.

   It may still store file import bindings for `ScopeContext`, but they should be input data from headers, not builder-produced data.

3. Update `ScopeContext` construction.

   `ScopeContext` should consume the header-built file visibility environment.

4. Update or revise the remaining AST refactor plan.

   If the plan file is committed, edit it. If it is not yet in repo, create a note in the roadmap or benchmark log.

   Required revisions:

   - remove any phase that keeps AST import binding as an AST phase
   - remove any mention of AST constant graph sorting
   - make `build_ast_environment` a linear semantic-resolution phase over sorted headers
   - keep later AST optimizations focused on context cloning, declaration table shape, expression token windows, and finalization/template churn

5. Update roadmap notes.

   Add follow-up notes for any discovered template or type-environment optimization opportunities.

## Tests

No new behavior tests should be necessary unless code paths changed. Run full validation.

## Audit checklist

- No AST doc comment claims AST owns import binding.
- No AST doc comment claims AST owns constant ordering.
- Remaining AST plan reflects the new contract.
- Roadmap has current follow-up notes.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Phase 7 — Header-stage parallel parsing with Rayon

## Summary

Once header parsing owns more cheap structural work, parallelizing per-file header parsing becomes more valuable. This phase uses Rayon to parse headers across files in parallel before the module-level combine/import/dependency steps.

Rayon already exists in the root `Cargo.toml`, so this phase should not need a new dependency.

## Important constraint

Parallel parsing must not corrupt or race shared compiler state.

The main risk is `StringTable`, warnings, and any path normalization that currently requires mutable shared access. Do not add a coarse global lock that makes the parallel version slower unless benchmarking proves it is still worthwhile.

## Implementation steps

1. Audit `parse_headers` and `parse_headers_in_file` shared mutable state.

   Current shared/aggregated state includes:

   - `StringTable`
   - warnings
   - `const_template_count`
   - `top_level_const_fragments`
   - `runtime_fragment_count`
   - `file_re_exports_by_source`
   - project path resolver/config/style directives

2. Introduce a per-file parse output type.

   Suggested shape:

   ```rust
   struct ParsedFileHeaders {
       headers: Vec<Header>,
       warnings: Vec<CompilerWarning>,
       file_re_exports: Vec<FileReExport>,
       top_level_const_fragments: Vec<TopLevelConstFragment>,
       runtime_fragment_count: usize,
       const_template_count: usize,
   }
   ```

   Exact shape may differ. The goal is to remove shared mutation during per-file parsing.

3. Make const-template numbering deterministic.

   Since parallel files finish in nondeterministic order, any numbering that affects paths/output must be deterministic.

   Options:

   - preassign file order and per-file const-template base offsets before parallel parsing
   - collect per-file const template counts first, compute offsets, then assign stable IDs
   - keep const-template numbering sequential for entry file only if only the entry file can contain top-level const templates

   Do not accept nondeterministic generated paths.

4. Handle `runtime_fragment_count` deterministically.

   Runtime fragment count is entry-file-specific. Keep it tied to the entry file parse result, not global parallel mutation.

5. Handle `StringTable` safely.

   Preferred direction:

   - minimize new interning during parallel header parsing
   - use already-interned token symbols and source paths where possible
   - if interning is required, isolate it behind a small deterministic pre-pass or a carefully scoped synchronization point

   Avoid a broad `Arc<Mutex<StringTable>>` hot path unless benchmarks show it is acceptable.

6. Use Rayon for file-level parsing.

   Suggested pattern:

   ```rust
   tokenized_files
       .into_par_iter()
       .map(parse_one_file_headers)
       .collect::<Result<Vec<_>, _>>()
   ```

   Exact implementation depends on `StringTable` strategy.

7. Combine parsed file results in stable source order.

   The final `headers` vector must be deterministic before dependency sorting.

8. Keep module-level work sequential unless separately justified.

   These can stay sequential:

   - `build_module_symbols`
   - re-export resolution
   - import environment construction
   - constant initializer dependency edge augmentation
   - dependency sorting

9. Add detailed timers:

   ```text
   Header parsing/file structural parse
   Header parsing/import preparation
   Header parsing/constant dependency edges
   Header parsing/parallel combine
   ```

10. If parallel parsing regresses performance, keep the sequential path as the default and record why.

    Do not leave two long-term parallel implementations. If both paths remain temporarily, gate the experimental one clearly and create a cleanup follow-up.

## Tests

- Run the full integration suite.
- Add a determinism test if feasible: parse/build the same multi-file project multiple times and assert stable sorted header output or stable final emitted artifact.
- Ensure diagnostics remain deterministic enough for tests.

## Audit checklist

- Per-file parsing has no unsafe shared mutable state.
- Header outputs are deterministic.
- Const-template IDs and runtime insertion indices are stable.
- Parallelization does not introduce broad lock contention without measurement.
- Sequential fallback, if retained, is documented as temporary.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log. Pay special attention to docs build, template stress, and speed-test results.

---

# Phase 8 — Final cleanup, documentation pass, and enforcement audit

## Summary

Remove stale comments, old assumptions, and transitional code. The final state should make it hard for an agent to reintroduce an AST-side sort/import binding pass.

## Implementation steps

1. Search for stale phrases and update/delete them:

   ```text
   strict dependency edges
   soft initializer hints
   AST import binding
   constant graph
   initializer-expression symbols are NOT edges
   AST ordering
   ordered_constant_headers
   ```

2. Update top-of-file doc comments in:

   ```text
   src/compiler_frontend/headers/*.rs
   src/compiler_frontend/module_dependencies.rs
   src/compiler_frontend/ast/module_ast/environment/*.rs
   src/compiler_frontend/ast/mod.rs
   src/compiler_frontend/declaration_syntax/*.rs
   ```

3. Ensure `docs/compiler-design-overview.md` matches the actual code.

4. Ensure roadmap and benchmark log are updated.

5. If any temporary sequential/parallel fallback remains, document when it should be removed.

6. Prune obsolete tests that only asserted old implementation structure.

7. Run a final duplication audit.

   Confirm there is one owner for each responsibility:

   ```text
   imports / visibility      -> headers/import preparation
   declaration shells        -> declaration_syntax + headers for top-level
   top-level dependency sort -> module_dependencies.rs
   constant semantic folding -> AST
   executable body parsing   -> AST
   HIR lowering              -> HIR
   borrow validation         -> borrow checker
   ```

## Audit checklist

- No AST top-level sorter remains.
- No AST import-binding builder remains.
- No duplicate declaration shell parser remains.
- Header/dependency/AST contract is documented in docs and code comments.
- Benchmarks show no unexplained regression.
- Follow-up AST refactor plan is revised and ready to continue.

## Validation

Before implementation:

```bash
just bench
```

After implementation:

```bash
just bench
just validate
```

Update the benchmark log.

---

# Expected final architecture

After this plan lands:

## Header parsing / preparation output

The sorted-input bundle should include enough data for AST to operate linearly:

```rust
pub(crate) struct SortedHeaders {
    pub(crate) headers: Vec<Header>,
    pub(crate) top_level_const_fragments: Vec<TopLevelConstFragment>,
    pub(crate) entry_runtime_fragment_count: usize,
    pub(crate) module_symbols: ModuleSymbols,
    pub(crate) file_import_bindings: FxHashMap<InternedPath, FileImportBindings>,
    pub(crate) warnings: Vec<CompilerWarning>,
}
```

Exact names can differ, but the ownership should not.

## AST constant resolution

AST constant resolution should look structurally like:

```rust
for header in sorted_headers {
    let HeaderKind::Constant { .. } = &header.kind else {
        continue;
    };

    let declaration = parse_and_fold_constant_header(header, header_built_file_bindings, ...)?;
    declaration_table.replace_by_path(declaration.clone())?;
    module_constants.push(declaration);
}
```

No AST graph. No retry loop. No topo-sort.

## Dependency sorting

Dependency sorting should operate on all header-provided top-level declaration edges:

```text
type alias targets
struct field types
choice payload types
function signature types
constant explicit type annotations
constant initializer references to constants
other future top-level compile-time declaration dependencies
```

It should not inspect executable function/start body references.

## Declaration shell parsing

Top-level declarations:

```text
Header parsing creates shells once.
AST consumes those shells.
```

Body-local declarations:

```text
AST creates shells using the same shared declaration_syntax parser, then resolves them.
```

---

# Risks and mitigations

## Risk: Header parsing becomes too semantic

This is intentional up to the structural boundary. Header parsing may parse imports, paths, declaration shells, and dependency references. It must not type-check executable bodies or fold expressions.

## Risk: Import environment depends on all module symbols

That is acceptable. Header parsing has a per-file structural parse step and a module-level header preparation step. Both are Stage 2 responsibilities.

## Risk: Parallel header parsing fights `StringTable`

Mitigate by first removing shared mutable state from per-file parsing. If string interning cannot be made efficient safely, keep parallelization limited or delayed with a benchmark-backed note.

## Risk: Fuzzy dependency path matching hides bad normalized paths

During migration, keep fallback behavior if needed. Add debug assertions or tests that new dependency edges are canonical. Later remove legacy fuzzy matching once coverage is strong.

## Risk: Same-file constant ordering changes accidentally

Same-file constants must continue to follow source order. Forward references must be rejected, not silently allowed by topo-sort.

---

# Stop criteria

This plan is complete when:

- header/import preparation builds file-local visibility environments
- constant initializer dependencies are represented as header dependency edges
- dependency sorting is the only top-level declaration sorter
- `constant_graph.rs` is deleted
- AST constant resolution is linear over sorted headers
- AST does not build import bindings
- top-level declaration shells are created only by header/shared declaration syntax code
- body-local AST declarations use the same declaration shell parser
- docs and file comments enforce the new stage contract
- benchmark log shows phase-by-phase results
- `just validate` passes
- `just bench` shows no unexplained major regression

After this, continue the remaining AST restructure/optimization plan with the corrected stage contract.
