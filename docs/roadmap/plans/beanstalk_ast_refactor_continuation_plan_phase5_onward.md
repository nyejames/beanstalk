# Beanstalk AST Refactor Continuation Plan

## Purpose

This plan replaces the old AST refactor plan from Phase 5 onward.

Phases 1–4 of the earlier AST pipeline restructure have already been completed, but the compiler architecture has now been corrected around a stronger header/dependency/AST stage contract. The remaining AST refactor must continue from that new contract, not from the old assumption that AST owns import binding or top-level constant ordering.

The new rule is:

Header parsing prepares top-level declaration shells, imports, visibility data, and dependency edges.
Dependency sorting produces top-level headers in the order AST needs.
AST consumes sorted top-level shells. It does not discover, bind, or sort them.

This continuation plan starts with a contract review and cleanup phase. That phase checks the work done by the header/dependency/AST refactor, removes any remaining duplicate AST-owned ordering/import work, verifies documentation and file-level comments, and only then resumes the remaining AST performance refactor.

## Current prerequisite state

Before starting this continuation plan, the header/dependency/AST refactor should have made the following true:
- Header parsing parses imports and re-exports.
- Header parsing builds top-level declaration shells.
- Header parsing records constant initializer dependency edges.
- Header/import preparation builds file-local visibility data.
- Dependency sorting owns all top-level declaration ordering.
- Dependency sorting includes constant initializer dependencies.
- Same-file constant source-order semantics are enforced before AST.
- Cross-file constant cycles are dependency cycles.
- The implicit entry start header is always appended last.
- AST does not topologically sort constants or any other top-level declarations.
- AST does not rebuild file import visibility from scratch.

This plan begins by auditing whether that is actually true in code.

## Updated primary goals

1. Verify and enforce the new header/dependency/AST contract before continuing.
2. Remove remaining AST-owned top-level ordering/import leftovers.
3. Ensure AST resolves sorted top-level shells linearly.
4. Redesign `ScopeContext` around shared environment and header-built file visibility.
5. Remove avoidable expression/parser copying while preserving shunting-yard/RPN.
6. Keep template/finalization cleanup conservative, measured, and isolated.
7. Keep benchmarks and audit notes updated phase by phase.
8. Leave the deeper `DataType` / `TypeEnvironment` redesign to its own plan.

## Updated non-goals

These are deliberately out of scope:

```text
- Reintroducing AST-owned import binding.
- Reintroducing AST-owned constant dependency graphs.
- Adding any top-level ordering pass in AST.
- Changing generic semantics.
- Rewriting expression parsing away from the current shunting-yard/RPN model.
- Full template pipeline redesign.
- Full DataType / TypeEnvironment redesign.
- Preserving old AST APIs through compatibility wrappers or parallel legacy paths.
```

If AST discovers a missing top-level dependency, the fix belongs in header parsing or dependency sorting, not in AST.

## Benchmark policy

Use the existing `xtask` benchmark workflow.

Before implementing each code-changing phase:

```bash
just bench
```

During implementation, when checking optimization direction:

```bash
just bench-quick
```

After implementing each code-changing phase:

```bash
just bench
just validate
```

`just validate` already covers formatting, clippy, unit tests, integration tests, docs check, and `bench-quick`.

Benchmark outputs are generated under:

```text
benchmarks/results/
```

Do not commit generated benchmark result directories. Commit summarized results in:

```text
docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
```

Each code-changing phase must update the benchmark log in the same commit as the phase.

## Benchmark log requirements

For each phase, record:

```text
Phase:
Commit:
Before benchmark run directory:
Before summary path:
After benchmark run directory:
After summary path:
Key rows:
- check benchmarks/speed-test.bst
- build benchmarks/speed-test.bst
- check docs
- any stress case directly affected by this phase
Relevant timer/counter notes:
Regression classification:
Audit notes:
```

Use the mean duration unless the generated summary clearly shows outlier noise, in which case record both mean and median.

Regression thresholds:

```text
Improved:          >= 3% faster
Neutral:           within +/-3%
Regression:        >= 3% slower
Major regression:  >= 10% slower
```

A small regression may be accepted only when it removes major architectural debt and a later phase is expected to recover it. A major regression blocks continuation unless it is clearly benchmark noise or a documented temporary transition.

## Detailed timers and counters

The remaining AST refactor should keep or add timers around the current architecture:

```text
AST/build environment
AST/emit nodes
AST/finalize
```

Keep AST counters focused on AST-owned work:

```text
AST/scope contexts created
AST/scope local declarations cloned total
AST/scope visible map clones avoided
AST/bounded expression token windows
AST/bounded expression token copies avoided
AST/runtime RPN unchanged folds
AST/template normalization nodes visited
AST/module constant normalization expressions visited
```

The following counters/timers should not exist as AST-owned work after the contract refactor:

```text
AST/constant dependency edges
AST/constant topo-sort count
AST/environment/import bindings resolved
```

If equivalent measurement is useful, it belongs under header/dependency timing names, for example:

```text
Headers/import preparation
Headers/declaration shell parsing
Headers/constant initializer dependency edges
Dependency sorting/topological sort
Dependency sorting/constant dependency edges
```

## Updated target module structure

The AST module should no longer contain a constant graph or import environment builder.

Target shape:

```text
src/compiler_frontend/ast/module_ast/
  mod.rs
  build_context.rs

  environment/
    mod.rs
    builder.rs
    resolved_declarations.rs
    declaration_table_view.rs    # only if AST needs a view/wrapper around header-built data

  emission/
    mod.rs
    emitter.rs
    scope_context.rs
    scope_factory.rs

  finalization/
    mod.rs
    finalizer.rs
    normalize_ast.rs
    normalize_constants.rs
    template_helpers.rs
```

Hard rules:

```text
- No ast/module_ast/environment/constant_graph.rs
- No AST-owned import_environment.rs that rebuilds file visibility
- No ordered_constant_headers in AST
- No AST topological sort over top-level headers
```

## Updated target internal architecture

### `AstPhaseContext` / `AstBuildContext`

Immutable inputs and services shared by AST phase builders.

Expected contents:

```text
entry_path
external_package_registry
style_directives
build_profile
project_path_resolver
path_format_config
top_level_const_fragments
entry_runtime_fragment_count
```

### `AstModuleEnvironment`

A semantic environment built by walking dependency-sorted headers.

It consumes:

```text
sorted declaration headers
header-built declaration table / symbol package
header-built file visibility bindings
header-provided dependency order
```

It owns AST-resolved semantic products:

```text
resolved_type_aliases
resolved_struct_fields_by_path
resolved_choice_variants_by_path
resolved_function_signatures_by_path
receiver_catalog
generic_declarations_by_path references/handles
generic_nominal_instantiations
builtin semantic data
module constants after fold/type validation
```

It must not own:

```text
import binding construction
top-level declaration discovery
constant dependency graph construction
top-level dependency sorting
```

### `AstEmitter`

Consumes sorted headers and the shared environment, then emits body AST and module metadata.

Owns:

```text
ast nodes
const_templates_by_path
warnings generated during body/template emission
rendered_path_usages
```

### `AstFinalizer`

Performs final AST assembly and HIR-boundary cleanup.

Owns:

```text
doc fragment stripping
const top-level fragment assembly
module constant normalization
template HIR-boundary normalization
choice definition collection
final Ast construction
```

Finalization should become thinner where earlier phases make old cleanup unnecessary.

---

# Phase 5 — Contract review and AST cleanup after header/dependency refactor

## Context

This is the first phase of the updated continuation plan.

The header/dependency/AST refactor changed the compiler contract: imports, visibility preparation, constant dependency edges, and top-level ordering now belong before AST. This phase reviews that work, removes stale AST-owned responsibilities, updates comments/docs, and verifies the code is clean enough to continue.

This phase is intentionally part audit and part cleanup. Any discovered contract violations should be fixed here before ScopeContext or expression churn work begins.

## Implementation steps

1. Run `just bench` and record the starting benchmark entry for this phase.

2. Audit the current code paths for these forbidden AST responsibilities:

   ```text
   ast/module_ast/environment/constant_graph.rs
   ordered_constant_headers
   AST constant topological sorting
   AST import-binding construction
   AST rebuilding file-local visibility maps
   AST dependency-order fallback logic
   fixed-point constant retry loops
   declaration snapshot rebuild loops
   stale "soft initializer edge" comments
   stale "AST owns import binding" comments
   ```

3. Delete or refactor any AST-owned constant graph code.

   Required end state:

   ```text
   - constants are resolved by iterating sorted headers
   - AST assumes dependencies are already ordered
   - missing dependency order is treated as a header/dependency bug
   ```

4. Delete or refactor any AST-owned import binding builder code.

   Required end state:

   ```text
   - AST receives header-built file visibility data
   - ScopeContext consumes visibility data by shared reference/Rc
   - AST does not resolve imports from raw import declarations
   ```

5. Review the header/dependency refactor output for duplication.

   Check whether similar logic now exists in both:

   ```text
   src/compiler_frontend/headers/
   src/compiler_frontend/module_dependencies.rs
   src/compiler_frontend/ast/
   ```

   Consolidate into the correct owner rather than leaving parallel behavior.

6. Update file-level doc comments in affected files, especially:

   ```text
   src/compiler_frontend/headers/mod.rs
   src/compiler_frontend/headers/parse_file_headers.rs
   src/compiler_frontend/module_dependencies.rs
   src/compiler_frontend/ast/mod.rs
   src/compiler_frontend/ast/module_ast/mod.rs
   src/compiler_frontend/ast/module_ast/environment/mod.rs
   src/compiler_frontend/ast/module_ast/environment/builder.rs
   src/compiler_frontend/ast/module_ast/scope_context.rs
   ```

   Comments must clearly state:

   ```text
   - headers discover and shell-parse top-level declarations
   - headers/import preparation builds file visibility
   - dependency sorting owns top-level order
   - AST consumes sorted headers linearly
   - AST does not sort or bind top-level declarations
   ```

7. Review `docs/compiler-design-overview.md`.

   Ensure it matches the implemented code, especially:

   ```text
   - Header Parsing
   - Dependency Sorting
   - Header/dependency/AST contract
   - AST Construction
   - Imports and visibility
   - Constants and folding
   ```

8. Review `docs/roadmap/roadmap.md`.

   Add or adjust notes only if this phase discovers remaining follow-up work, such as:

   ```text
   - template optimization opportunity
   - type environment redesign note
   - header parallelization follow-up if not finished
   ```

9. Add or update integration tests proving the contract:

   ```text
   - cross-file constant dependency resolves through dependency sorting
   - AST has no separate constant ordering behavior
   - same-file forward constant reference fails before/at dependency contract boundary
   - import alias visibility is available to AST through header-built bindings
   - non-imported symbol remains invisible to AST
   - start header runs after sorted declarations and does not participate in dependency sorting
   ```

10. Remove obsolete counters/timers from AST:

    ```text
    AST/constant dependency edges
    AST/constant topo-sort count
    AST/environment/import bindings resolved
    ```

    If needed, replace them with header/dependency counters.

11. Run `just bench`, update the benchmark log, then run `just validate`.

## Audit checklist

```text
- Is there no AST constant graph?
- Is there no AST top-level sorting pass?
- Is there no AST import-binding builder?
- Does AST resolve constants by walking sorted headers?
- Are stale comments/doc comments removed?
- Does the compiler design overview match the code?
- Are diagnostics still structured and source-located?
- Did cleanup reduce duplication rather than move it?
- Did tests prove the stage contract rather than implementation details?
```

## Commit contents

```text
- Contract cleanup changes
- Removed obsolete AST ordering/import code
- Updated comments/doc comments
- Tests proving contract
- Benchmark log before/after entry
- Audit notes
```

---

# Phase 6 — Redesign `ScopeContext` around shared AST environment

## Context

After the contract cleanup, AST should be consuming header-built file visibility and sorted declaration shells. `ScopeContext` should reflect that.

The current risk is that body parsing, expression parsing, control-flow children, and templates clone large maps or carry duplicated visibility state. This phase redesigns `ScopeContext` so environment-wide data is shared and only truly local state is owned per scope.

## Target shape

Exact names can change, but the ownership split should not:

```rust
struct ScopeShared {
    environment: Rc<AstModuleEnvironment>,
    file_visibility: Rc<FileVisibility>,
    source_file_scope: InternedPath,
    path_context: PathContext,
}

struct ScopeContext {
    shared: Rc<ScopeShared>,
    kind: ContextKind,
    scope: InternedPath,
    local_declarations: Vec<Declaration>,
    local_declarations_by_name: FxHashMap<StringId, Vec<u32>>,
    expected_result_types: Vec<DataType>,
    expected_error_type: Option<DataType>,
    loop_depth: usize,
}
```

`FileVisibility` may be whatever type the header/dependency refactor introduced. The important point is that it is header-built and shared. AST must not rebuild it.

## Implementation steps

1. Run `just bench` and record the starting benchmark entry for this phase.

2. Identify every `ScopeContext` constructor and child-context constructor.

3. Classify every field in `ScopeContext` as one of:

   ```text
   - shared module environment
   - shared file visibility
   - shared frontend services/config
   - per-scope semantic state
   - body-local declaration state
   ```

4. Introduce or finalize:

   ```text
   ScopeShared
   ScopeContext
   ScopeFactory
   ```

   Suggested location:

   ```text
   src/compiler_frontend/ast/module_ast/emission/scope_factory.rs
   ```

5. Route function, start, const-template, expression, block, loop, match, and template contexts through `ScopeFactory`.

6. Remove owned environment-wide maps from `ScopeContext`.

   Examples of things that should not be cloned into every context:

   ```text
   top-level declaration table
   file visibility maps
   resolved type aliases
   generic declarations
   resolved struct fields
   receiver catalog
   style directives
   external package registry
   path format config
   project path resolver
   ```

7. Keep local declarations owned for now.

   Do not introduce persistent local frames unless counters prove local clone pressure remains significant after shared-state removal.

8. Preserve lookup semantics:

   ```text
   local declarations first
   file-local source/import visibility
   file-local type alias visibility
   external/prelude visibility through active file scope
   builtins/reserved symbols
   receiver method visibility
   no shadowing
   ```

9. Add or update counters:

   ```text
   AST/scope contexts created
   AST/scope local declarations cloned total
   AST/scope visible map clones avoided
   ```

10. Add or update tests for:

    ```text
    import alias hiding original imported name
    external imports through active scope only
    receiver method visibility
    local declaration lookup
    no shadowing
    type alias visibility through file scope
    constant use through header-built visibility
    ```

11. Run `just bench`, update the benchmark log, then run `just validate`.

## Audit checklist

```text
- Does ScopeContext no longer clone environment-wide maps?
- Are header-built visibility bindings consumed, not rebuilt?
- Are local declarations the main per-scope owned state?
- Is lookup still explicit and readable?
- Are generic metadata and receiver methods shared without semantic changes?
- Are comments clear about shared vs local state?
- Did benchmark counters confirm clone reduction?
```

## Commit contents

```text
- ScopeShared / ScopeFactory / redesigned ScopeContext
- Removed owned environment-wide map fields
- Lookup updates
- Tests as needed
- Benchmark log update
- Audit notes
```

---

# Phase 7 — Expression and parser churn cleanup

## Context

Expression parsing should remain shunting-yard/RPN based. This phase removes avoidable allocation/copying without changing that model.

The old AST plan identified bounded expression parsing and unchanged runtime RPN folding as likely churn points. Those findings still stand under the new contract.

## Implementation steps

1. Run `just bench` and record the starting benchmark entry for this phase.

2. Audit expression parsing paths that currently:

   ```text
   - scan to an end index
   - copy token slices with to_vec
   - append synthetic EOF
   - create temporary FileTokens
   ```

3. Add a bounded token stream/window mechanism.

   Requirements:

   ```text
   - no Vec<Token> allocation for normal delimiter-bounded expressions
   - no synthetic EOF token allocation for ordinary bounded parsing
   - source locations and canonical source path still preserved
   - diagnostics still point to useful spans
   - narrow API local to token stream parsing
   ```

4. Replace expression parsing paths that can use the bounded stream/window.

5. Preserve the current shunting-yard/RPN pipeline.

   Do not rewrite to Pratt parsing or nested expression trees in this phase.

6. Change constant folding from always returning a fresh `Vec<AstNode>` to a result shape equivalent to:

   ```rust
   enum ConstantFoldResult {
       Unchanged,
       Folded(Vec<AstNode>),
   }
   ```

7. Update `evaluate_expression` so unchanged runtime RPN reuses the already-owned RPN vector instead of cloning.

8. Ensure body-local declarations continue to use the shared declaration shell parser.

   Important rule:

   ```text
   Top-level declaration shells come from headers.
   Body-local declarations may create shells through the same declaration_syntax owner before full AST resolution.
   ```

9. Add counters:

   ```text
   AST/bounded expression token windows
   AST/bounded expression token copies avoided
   AST/runtime RPN unchanged folds
   ```

10. Add focused tests for bounded parsing edge cases:

    ```text
    delimiter at current position
    nested parentheses
    nested collections
    templates inside bounded expressions
    missing delimiter diagnostics
    body-local declarations using declaration shell parser
    ```

11. Run `just bench`, update the benchmark log, then run `just validate`.

## Audit checklist

```text
- Are bounded expressions parsed without copying token slices?
- Is RPN preserved?
- Does constant folding avoid cloning unchanged runtime RPN?
- Are diagnostics unchanged or improved?
- Is the token-window API narrow and readable?
- Did the change avoid creating a broad parser abstraction?
```

## Commit contents

```text
- Bounded token stream/window
- Expression parser updates
- ConstantFoldResult-style API
- Tests
- Benchmark log update
- Audit notes
```

---

# Phase 8 — Conservative finalization and template cleanup

## Context

Finalization still owns HIR-boundary cleanup and template normalization. This plan does not redesign the template pipeline, but finalization should be thinner after stable declarations, header-built visibility, and dependency-sorted constants.

This phase removes obsolete cleanup, consolidates traversal only where behavior is genuinely shared, and records measured template/finalization cost.

## Implementation steps

1. Run `just bench` and record the starting benchmark entry for this phase.

2. Review `AstFinalizer` and finalization submodules.

3. Ensure finalization owns only:

   ```text
   doc fragment stripping
   const top-level fragment assembly
   module constant normalization
   template HIR-boundary normalization
   choice definition collection
   final Ast construction
   ```

4. Remove obsolete finalization code made unnecessary by:

   ```text
   stable declaration slots
   header-built file visibility
   dependency-sorted constant resolution
   removal of AST constant graph/retry machinery
   removal of duplicate declaration metadata
   ```

5. Consolidate duplicate traversal helpers only when the behavior is genuinely shared.

   Do not create a broad template utility module unless both callers clearly own the same behavior.

6. Add or keep counters:

   ```text
   AST/template normalization nodes visited
   AST/module constant normalization expressions visited
   AST/templates folded during finalization
   AST/runtime render plans rebuilt
   ```

7. Confirm finalization does not compensate for:

   ```text
   missing top-level ordering
   duplicate declaration metadata
   missing import visibility
   stale constant resolution state
   ```

8. If template normalization remains a major benchmark contributor, add a concise note to:

   ```text
   docs/roadmap/roadmap.md
   ```

   Do not create a separate template plan unless explicitly requested later.

9. Run `just bench`, update the benchmark log, then run `just validate`.

## Audit checklist

```text
- Is finalization thinner and better organized?
- Are template semantics unchanged?
- Is duplicate traversal removed only where safe?
- Did stable declarations and header/dependency ordering remove old cleanup?
- Are template optimization follow-up notes added only if benchmark evidence supports them?
```

## Commit contents

```text
- Finalization cleanup
- Template/module constant traversal consolidation where safe
- Roadmap notes if needed
- Benchmark log update
- Audit notes
```

---

# Phase 9 — Final documentation, benchmark, and stop-point audit

## Context

This phase verifies that the AST continuation refactor is complete and aligned with the new compiler-stage contract. It also prepares the codebase for the future `TypeEnvironment` redesign.

## Implementation steps

1. Run `just bench` and record the starting benchmark entry for this phase.

2. Re-read and update as needed:

   ```text
   docs/compiler-design-overview.md
   docs/roadmap/roadmap.md
   docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
   docs/roadmap/plans/type-environment-redesign-plan.md
   docs/src/docs/progress/#page.bst
   ```

3. `docs/compiler-design-overview.md` must describe:

   ```text
   header parsing/import preparation owns file-local import visibility construction
   dependency sorting owns all top-level declaration ordering
   constant initializer dependencies are header/dependency edges
   AST consumes sorted headers linearly
   AST does not rebuild import visibility
   AST does not topologically sort constants or other top-level declarations
   AST uses build_ast_environment / emit_ast_nodes / finalize_ast internally
   finalization is HIR-boundary cleanup
   ```

4. Check file-level comments for stale ownership claims.

   Search for stale phrases like:

   ```text
   AST import binding
   strict dependency edges only
   initializer-expression symbols are soft hints
   constant graph
   ordered constant headers
   declaration snapshot rebuild
   latest wins
   fixed-point constant resolution
   ```

5. Check whether `docs/src/docs/progress/#page.bst` needs updates.

   It likely should not unless the refactor discovered or changed user-visible feature support.

6. Add final benchmark summary to the benchmark log:

   ```text
   - baseline at start of continuation plan
   - final result
   - core benchmark deltas
   - relevant AST/header/dependency timer deltas
   - remaining bottlenecks
   - next recommended plan
   ```

7. Run final:

   ```bash
   just bench
   just validate
   ```

## Stop criteria

End this continuation refactor only when all are true:

```text
- Header/dependency/AST contract is enforced in code.
- AST has no constant graph.
- AST has no top-level dependency sort.
- AST does not rebuild file import visibility.
- AST resolves top-level declarations by walking sorted headers.
- AstBuildState is gone.
- Stable declaration slots/table are used.
- ScopeContext no longer clones environment-wide maps.
- Bounded expression token copying is gone from targeted hot paths.
- constant_fold no longer clones unchanged runtime RPN.
- Finalization no longer compensates for duplicate declarations or missing top-level ordering.
- compiler-design-overview and relevant file doc comments are current.
- roadmap notes capture remaining template/type-environment follow-ups.
- benchmark log shows phase-by-phase before/after results.
- generated benchmark results are uncommitted.
- just bench and just validate pass.
```

## Audit checklist

```text
- Is any old pass-count language still present?
- Are there compatibility wrappers or parallel old/new APIs left?
- Are generated benchmark results uncommitted?
- Are benchmark log entries enough to judge performance?
- Are template and type-environment follow-ups captured but not mixed into this refactor?
- Does the module layout match the style guide expectations?
- Would a future agent know where top-level ordering belongs?
```

## Commit contents

```text
- Final documentation updates
- Final benchmark log summary
- Final cleanup
- Audit notes
```

---

# Updated implementation order summary

```text
5. Contract review and AST cleanup after header/dependency refactor
6. ScopeContext shared environment redesign
7. Expression/parser churn cleanup
8. Conservative finalization/template cleanup
9. Final docs/benchmark/roadmap audit
```

This is intentionally numbered from Phase 5 to preserve continuity with the completed Phase 1–4 AST work.

---

# Risks and mitigations

## Risk: AST-owned ordering sneaks back in

Mitigation:

```text
- Dependency sorting must be the only top-level ordering owner.
- Any missing top-level dependency discovered in AST must become a header dependency edge.
- Add tests proving AST resolves constants by sorted order without a second graph.
```

## Risk: header parsing becomes too semantic

Mitigation:

```text
Header parsing may parse imports, normalized paths, declaration shells, type surfaces, and constant reference hints.
It must not fold expressions, type-check executable bodies, or lower runtime AST nodes.
```

## Risk: ScopeContext redesign overcomplicates lookup

Mitigation:

```text
Keep shared vs local state explicit.
Keep local declarations owned at first.
Only introduce persistent local frames if benchmark counters prove local clone pressure remains significant.
```

## Risk: expression token windows blur parser ownership

Mitigation:

```text
Keep token-window APIs narrow and local to token stream parsing.
Do not introduce a broad parser abstraction.
Preserve existing diagnostics and RPN expression output.
```

## Risk: template finalization dominates after other fixes

Mitigation:

```text
Record measured evidence in the benchmark log.
Add roadmap notes for a template-specific follow-up.
Do not redesign templates inside this continuation unless it blocks the AST architecture.
```

## Risk: benchmarks are noisy

Mitigation:

```text
Use full just bench runs for before/after phase entries.
Compare mean and median.
Classify regressions with defined thresholds.
Do not overreact to one noisy case without checking the generated logs.
```

---

# Required per-phase commit format

Each code-changing phase commit should include:

```text
- code changes
- benchmark log before/after entries
- relevant tests
- relevant docs/roadmap/comment updates
- audit notes
```

Do not commit:

```text
benchmarks/results/
```

---

# Final handoff for the future TypeEnvironment plan

After this continuation plan hits its stop criteria, continue with:

```text
docs/roadmap/plans/type-environment-redesign-plan.md
```

That plan should focus on:

```text
- compact type IDs
- nominal type definition table
- generic instance interning
- separating type identity from type layout
- reducing DataType cloning in signatures, expressions, HIR, and generics
- preserving current user-facing type semantics
```

Do not start that work until this AST continuation refactor hits the stop criteria.
