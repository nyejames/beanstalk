# Beanstalk AST Pipeline Restructure and Optimisation Plan

## Purpose

The AST stage has drifted from a compact, mostly linear semantic construction stage into a multi-pass mutable accumulator with duplicated metadata, repeated cloning, fixed-point constant retries, and late transformational finalization. This plan refactors the AST architecture and optimises the major known churn points together.

The goal is not to reduce the pass count cosmetically. The goal is to make the AST stage structurally clear, cheaper to run, easier to profile, and harder to accidentally bloat again.

Target public AST architecture:

```text
build_ast_environment
emit_ast_nodes
finalize_ast
```

The internal substeps remain implementation details. The compiler design overview should describe AST by ownership and data flow, not by a fixed pass count.

## Primary goals

1. Replace `AstBuildState` immediately with explicit phase-owned structs.
2. Build one shared semantic environment before body emission.
3. Stop append-only declaration mutation and replace it with stable declaration slots.
4. Remove fixed-point constant resolution and replace it with explicit constant dependency ordering.
5. Redesign `ScopeContext` around shared environment/file state instead of cloning large maps.
6. Remove avoidable parser/expression copying.
7. Keep template finalization conservative but better isolated and measured.
8. Add benchmark tracking and detailed AST timers/counters so each phase is measured objectively.
9. Update roadmap/design documentation as the architecture changes.
10. Defer the deeper `DataType` / `TypeEnvironment` redesign into its own plan.

## Non-goals

These are deliberately out of scope for this first AST refactor:

- Full `DataType` / `TypeEnvironment` redesign.
- Generic semantics changes.
- Moving import binding ownership into header/dependency stages.
- Rewriting expression parsing away from the current shunting-yard/RPN model.
- Full template pipeline redesign.
- Preserving old AST APIs through compatibility wrappers or parallel legacy paths.
- Large new benchmark suites unless a real uncovered bottleneck is found.

## Reference constraints

The implementation must follow the current project rules:

- Keep compiler stage boundaries clear.
- Prefer one current implementation path, not transitional wrappers.
- Avoid user-input panics.
- Use structured diagnostics.
- Keep modules focused and readable.
- Use `mod.rs` as the module map/orchestration point.
- Add concise WHAT/WHY comments for non-obvious invariants and data flow.
- Run the required validation and benchmark gates at the end of every code-changing phase.

## Benchmark policy

### Commands

Use the new `xtask` benchmark workflow.

Before implementing each code-changing phase:

```bash
just bench
```

During implementation, when checking optimisation direction:

```bash
just bench-quick
```

After implementing each code-changing phase:

```bash
just bench
just validate
```

`just validate` already runs formatting, clippy, unit tests, integration tests, docs check, and `bench-quick`.

### Benchmark outputs

Benchmark outputs are generated under:

```text
benchmarks/results/
```

Do not commit generated benchmark result directories. Commit only summarized results in the refactor benchmark log.

Benchmark log path:

```text
docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
```

Each code-changing phase must update the benchmark log in the same commit as the phase.

### Benchmark log requirements

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
AST timer notes:
Regression classification:
Audit notes:
```

Do not paste the entire generated benchmark summary. Copy only relevant rows and observations.

### Regression thresholds

Use the mean duration unless the summary clearly shows outlier noise, in which case record both mean and median.

```text
Improved:          >= 3% faster
Neutral:           within ±3%
Regression:        >= 3% slower
Major regression:  >= 10% slower
```

A small regression may be accepted only when it removes major architectural debt and a later phase is expected to recover it. A major regression blocks continuation unless it is clearly benchmark noise or a documented temporary transition.

## Detailed timer and churn counters

Add or update detailed timers for the new architecture:

```text
AST/build environment
AST/emit nodes
AST/finalize
```

Add environment sub-timers:

```text
AST/environment/import bindings
AST/environment/type aliases
AST/environment/constants
AST/environment/nominal types
AST/environment/function signatures
AST/environment/receiver catalog
```

Add temporary or dev-only churn counters where useful:

```text
AST/scope contexts created
AST/scope local declarations cloned total
AST/scope visible map clones avoided
AST/constant dependency edges
AST/constant topo-sort count
AST/bounded expression token windows
AST/bounded expression token copies avoided
AST/runtime RPN unchanged folds
AST/template normalization nodes visited
AST/module constant normalization expressions visited
```

These counters should be compiled or printed only through existing detailed-timer/debug infrastructure. Do not pollute normal compiler output.

## Target module structure

Keep the directory name:

```text
src/compiler_frontend/ast/module_ast/
```

Restructure around the new phase ownership:

```text
module_ast/
  mod.rs
  build_context.rs

  environment/
    mod.rs
    builder.rs
    declaration_table.rs
    constant_graph.rs
    import_environment.rs

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

`module_ast/mod.rs` should be a readable structural map and orchestration entry point. Core implementation belongs in focused submodules.

## Target internal architecture

### `AstBuildContext`

Immutable inputs and services shared by the phase builders.

Owns or references:

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

Resolved semantic environment used by body emission.

Owns:

```text
file_import_bindings
declaration_table
resolved_type_aliases
resolved_struct_fields_by_path
resolved_choice_variants_by_path
resolved_function_signatures_by_path
receiver_catalog
generic_declarations_by_path
generic_nominal_instantiations
builtin data
```

Important rule: this environment is shared by reference/`Rc` from body parsing contexts. It is not cloned per body or per child expression.

### `AstEmitter`

Consumes sorted headers and the shared environment, emits body AST and module metadata.

Owns:

```text
ast nodes
module_constants
const_templates_by_path
warnings
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

Finalization should become as thin as possible, but this plan does not redesign template construction unless benchmark evidence forces it.

## Phase 1 — Benchmark log, roadmap hooks, and AST instrumentation

### Context

This phase establishes measurement before structural churn starts. It also adds roadmap tracking so follow-up work discovered during the refactor is not lost.

### Implementation steps

1. Create:

   ```text
   docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
   ```

2. Run `just bench` before touching compiler code and record baseline results in the log.

3. Add new high-level AST timers:

   ```text
   AST/build environment
   AST/emit nodes
   AST/finalize
   ```

4. Add environment sub-timers around current equivalent work, even if the old pass structure still exists temporarily.

5. Add initial churn counters for:

   ```text
   scope contexts created
   declaration snapshot rebuilds
   constant resolution rounds
   bounded expression token copies
   runtime RPN clone count
   template normalization nodes visited
   ```

6. Create the follow-up plan file:

   ```text
   docs/roadmap/plans/type-environment-redesign-plan.md
   ```

   Keep it short. Include goals:
   - compact type IDs
   - nominal type definition table
   - generic instance interning
   - reduced `DataType` cloning
   - better separation of type identity from layout

7. Update:

   ```text
   docs/roadmap/roadmap.md
   ```

   Add links/notes for:
   - this AST pipeline optimisation/refactor
   - the later type-environment redesign
   - template optimisation/restructure follow-up notes as issues are discovered

8. Run `just bench` after changes and update the benchmark log.

9. Run `just validate`.

### Audit checklist

- Are timers named around the new intended architecture, not the old fixed pass count?
- Are counters detailed-timer/dev-only, not normal output?
- Is the benchmark log concise and useful?
- Is `CONTRIBUTING.md` left alone? It already documents the benchmark workflow.
- Did roadmap updates avoid bloating user-facing docs?

### Commit contents

- Benchmark log with baseline/after results.
- Timer/counter instrumentation.
- Roadmap update.
- Short type-environment follow-up plan.
- Audit notes.

## Phase 2 — Replace `AstBuildState` with explicit phase structs

### Context

`AstBuildState` currently acts as a large mutable accumulator whose fields are valid only after certain passes. That is the core structural drift. This phase replaces it directly rather than slowly evolving it.

### Implementation steps

1. Introduce:

   ```text
   AstBuildContext
   AstModuleEnvironmentBuilder
   AstEmitter
   AstFinalizer
   ```

2. Move immutable build inputs into `AstBuildContext`.

3. Move import/type/constant/signature/receiver environment construction into `AstModuleEnvironmentBuilder`.

4. Move body/template emission state into `AstEmitter`.

5. Move final assembly into `AstFinalizer`.

6. Rewrite `Ast::new` to orchestrate only:

   ```rust
   let context = AstBuildContext::new(...);
   let environment = AstModuleEnvironmentBuilder::new(&context, module_symbols)
       .build(sorted_headers, string_table)?;
   let emitted = AstEmitter::new(&context, &environment)
       .emit(sorted_headers, string_table)?;
   AstFinalizer::new(&context, &environment)
       .finalize(emitted, string_table)
   ```

   The exact API can differ, but this should be the visible shape.

7. Delete `AstBuildState`.

8. Move old pass functions into the new modules. Do not leave compatibility wrappers.

9. Update `src/compiler_frontend/ast/mod.rs` and `module_ast/mod.rs` docs to describe the three architectural phases.

10. Update `docs/compiler-design-overview.md` AST section to describe:

    ```text
    build_ast_environment
    emit_ast_nodes
    finalize_ast
    ```

    Remove stale fixed-pass language.

11. Run before/after `just bench`, update benchmark log, run `just validate`.

### Audit checklist

- Is there one clear AST orchestration path?
- Is `AstBuildState` fully gone?
- Does `mod.rs` explain structure and data flow?
- Are old pass-count comments removed?
- Did behavior remain unchanged?
- Are timers now aligned with the new phase names?

### Commit contents

- New phase structs.
- Deleted `AstBuildState`.
- Reorganized `module_ast/`.
- Compiler design overview update.
- Benchmark log update.
- Audit notes.

## Phase 3 — Stable declaration table and environment-owned top-level metadata

### Context

Top-level declarations are currently duplicated: placeholder declarations are built from headers, then resolved declarations are appended later. Lookup compensates with reverse scans and “latest wins” behavior. This creates duplicate metadata, larger snapshots, slower lookup, and finalization cleanup.

### Implementation steps

1. Add:

   ```text
   environment/declaration_table.rs
   ```

2. Introduce stable declaration IDs:

   ```rust
   struct DeclarationId(u32);
   ```

3. Implement a table shape equivalent to:

   ```rust
   struct TopLevelDeclarationTable {
       declarations: Vec<Declaration>,
       by_path: FxHashMap<InternedPath, DeclarationId>,
       by_name: FxHashMap<StringId, Vec<DeclarationId>>,
   }
   ```

4. Build the table from sorted header placeholders once.

5. Replace append-on-resolution with update-in-place by `DeclarationId`.

6. Remove reverse latest-wins lookup paths.

7. Remove final reverse choice-definition dedupe caused by duplicate declarations.

8. Make type resolution and body emission use the same indexed table.

9. Preserve source-order and visibility behavior.

10. Add tests or strengthen existing tests around:
    - duplicate top-level name rejection
    - constants visible across files
    - choices collected once
    - struct field resolution
    - imported alias lookup

11. Run before/after `just bench`, update benchmark log, run `just validate`.

### Audit checklist

- Are declaration placeholders and resolved declarations represented once?
- Are all old append paths deleted?
- Is lookup indexed by path/name rather than reverse-scanning a Vec?
- Did finalization get simpler?
- Are diagnostics still source-located and specific?

### Commit contents

- Stable declaration table.
- Removed append-only declaration mutation.
- Removed latest-wins reverse lookup.
- Tests/fixtures as needed.
- Benchmark log update.
- Audit notes.

## Phase 4 — AST constant dependency graph and single-pass constant resolution

### Context

Constant resolution currently retries unresolved constants until a fixed point. This is expensive and diagnostically indirect. The header parsing/declaration shell already owns constant declaration shape, so this phase should reuse or extend that existing output rather than inventing a parallel full expression parser.

Same-file constant semantics remain source-order based. Cross-file constant dependencies follow sorted module/header dependency behavior. Cross-file cycles remain invalid.

### Implementation steps

1. Add:

   ```text
   environment/constant_graph.rs
   ```

2. Audit the existing constant declaration shell/header output.

3. Reuse existing initializer/reference metadata where available.

4. If the existing shell lacks just enough metadata, extend the existing declaration/header owner. Do not create a parallel parser.

5. Build constant dependency edges:
   - constant -> imported constant dependency
   - constant -> earlier same-file constant dependency
   - reject same-file forward reference
   - reject visible non-constant top-level symbol in const initializer
   - reject unknown constant reference with precise diagnostic

6. Topologically order cross-file constant dependencies.

7. Resolve/fold each constant once in dependency order.

8. Delete fixed-point retry logic:
   - pending headers
   - defer-on-name-error behavior
   - declaration snapshot rebuilds
   - retry counters that no longer apply

9. Add diagnostics for:
   - unknown constant reference
   - non-constant reference in constant initializer
   - same-file forward constant reference
   - constant cycle
   - not visible/imported constant reference

10. Add integration tests for:
    - same-file source-order constant success
    - same-file forward reference failure
    - imported constant success
    - imported non-constant failure
    - constant cycle failure
    - const template using constants

11. Run before/after `just bench`, update benchmark log, run `just validate`.

### Audit checklist

- Is constant resolution no longer retry-based?
- Does dependency extraction reuse existing declaration/header syntax ownership?
- Are diagnostics better than before?
- Did same-file source-order semantics remain unchanged?
- Are constants still compile-time-only and foldability-enforced?

### Commit contents

- Constant graph.
- Single-pass ordered constant resolution.
- Removed fixed-point retry machinery.
- Tests.
- Benchmark log update.
- Audit notes.

## Phase 5 — Redesign `ScopeContext` around shared AST environment

### Context

`ScopeContext` currently clones large maps into child contexts. Function bodies, expressions, control-flow children, and template contexts can repeatedly clone visibility maps, type alias maps, generic metadata, resolved struct fields, style directives, and external package registry data.

This phase makes `ScopeContext` match the new AST architecture.

### Target shape

```rust
struct ScopeShared {
    environment: Rc<AstModuleEnvironment>,
    file_imports: Rc<FileImportBindings>,
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

Exact names can change, but the ownership split should not.

### Implementation steps

1. Move file-local visibility maps into a compact `FileImportBindings` type if not already cleanly represented.

2. Store file import bindings in `AstModuleEnvironment`.

3. Add a `ScopeFactory` under:

   ```text
   emission/scope_factory.rs
   ```

4. Construct function/start/const-template scopes through `ScopeFactory`.

5. Remove owned map fields from `ScopeContext` where they can be reached through shared environment/file bindings.

6. Keep local declarations owned for now. Do not introduce persistent local frames unless benchmarks still show local clone pressure after shared state is removed.

7. Ensure all lookups still enforce:
   - local declarations first
   - file-local source/import visibility
   - type alias visibility
   - external package visibility through active scope
   - receiver method visibility

8. Add or update counters:
   - scope contexts created
   - shared map clones avoided
   - local declaration clone totals

9. Add tests or targeted assertions for:
   - import alias hiding original name
   - external imports through active scope only
   - receiver method visibility
   - local declaration lookup
   - no shadowing behavior

10. Run before/after `just bench`, update benchmark log, run `just validate`.

### Audit checklist

- Does `ScopeContext` no longer clone environment-wide maps?
- Are local declarations the only substantial per-scope mutable state?
- Are visibility rules still enforced only through active scope?
- Are generics metadata shared without semantic changes?
- Is lookup code simpler, not more magical?

### Commit contents

- Redesigned `ScopeContext`.
- `ScopeFactory`.
- Shared environment/file bindings.
- Tests as needed.
- Benchmark log update.
- Audit notes.

## Phase 6 — Expression/parser churn cleanup

### Context

Expression parsing should remain RPN-based. The current flow already orders infix fragments with shunting-yard before constant folding. This phase only removes avoidable allocation/copying.

### Implementation steps

1. Add a bounded token stream/window mechanism for delimiter-bounded expressions.

2. Replace expression parsing paths that currently:
   - scan to an end index
   - copy token slices with `to_vec`
   - append synthetic EOF
   - create temporary `FileTokens`

3. Keep the bounded parser API local to token stream parsing. Do not introduce a broad parser abstraction.

4. Change constant folding from:

   ```rust
   fn constant_fold(...) -> Result<Vec<AstNode>, CompilerError>
   ```

   to a return shape equivalent to:

   ```rust
   enum ConstantFoldResult {
       Unchanged,
       Folded(Vec<AstNode>),
   }
   ```

5. Update `evaluate_expression` so unchanged runtime RPN reuses the already-owned RPN vector instead of cloning.

6. Preserve the current shunting-yard/RPN representation.

7. Add counters:
   - bounded token windows used
   - bounded token copies avoided
   - unchanged runtime RPN folds

8. Add focused unit tests for bounded parsing edge cases:
   - delimiter at current position
   - nested parentheses
   - nested collections
   - templates inside bounded expressions
   - missing delimiter diagnostics

9. Run before/after `just bench`, update benchmark log, run `just validate`.

### Audit checklist

- Are bounded expressions parsed without copying token slices?
- Is the RPN pipeline preserved?
- Does constant folding avoid cloning unchanged runtime RPN?
- Are diagnostics unchanged or improved?
- Is the new token-window API narrow and readable?

### Commit contents

- Bounded token stream/window.
- Constant fold result API.
- Expression parser updates.
- Tests.
- Benchmark log update.
- Audit notes.

## Phase 7 — Conservative finalization/template cleanup

### Context

Finalization currently performs several necessary HIR-boundary tasks and recursively normalizes templates. This plan does not redesign the template pipeline, but it should isolate finalization better and measure template cost.

### Implementation steps

1. Keep finalization as the HIR-boundary normalization step.

2. Ensure `AstFinalizer` owns:
   - doc fragment stripping
   - top-level const fragment assembly
   - module constant normalization
   - template normalization
   - choice definition collection
   - final `Ast` construction

3. Consolidate duplicate traversal helpers only when the behavior is genuinely shared.

4. Add/keep counters:
   - AST nodes visited during template normalization
   - module constant expressions visited
   - templates folded during finalization
   - runtime render plans rebuilt

5. Remove obsolete finalization code made unnecessary by stable declaration slots.

6. If template normalization remains a major benchmark contributor, add a concise note to `docs/roadmap/roadmap.md` describing the discovered follow-up optimization opportunity. Do not create a separate template plan unless later requested.

7. Run before/after `just bench`, update benchmark log, run `just validate`.

### Audit checklist

- Is finalization thinner and better organized?
- Are template semantics unchanged?
- Is duplicate traversal removed only where safe?
- Did stable declaration slots remove old final dedupe work?
- Are template optimization follow-up notes added if warranted by benchmarks?

### Commit contents

- Finalization cleanup.
- Template/module constant traversal consolidation where safe.
- Roadmap notes if needed.
- Benchmark log update.
- Audit notes.

## Phase 8 — Final documentation, roadmap, and stop-point audit

### Context

This phase checks that the refactor achieved its intended boundary and stops before the separate type-environment redesign.

### Implementation steps

1. Re-read and update:

   ```text
   docs/compiler-design-overview.md
   docs/roadmap/roadmap.md
   docs/roadmap/refactors/ast-pipeline-optimisation-benchmark-log.md
   docs/roadmap/plans/type-environment-redesign-plan.md
   ```

2. Ensure `docs/compiler-design-overview.md` describes:
   - AST stage ownership
   - three-phase AST construction
   - import binding remains AST-owned
   - constants are ordered explicitly by AST environment building
   - finalization is the HIR-boundary cleanup step
   - no fixed internal pass count

3. Ensure `docs/roadmap/roadmap.md` links:
   - completed/in-progress AST refactor plan
   - future type-environment redesign
   - any template optimization notes discovered

4. Check whether `docs/src/docs/progress/#page.bst` needs updates. It likely should not unless implementation uncovered feature status drift.

5. Run final `just bench`.

6. Run final `just validate`.

7. Add final benchmark summary to the benchmark log:
   - initial baseline
   - final result
   - per-core-case deltas
   - AST timer deltas
   - remaining bottlenecks

### Stop criteria

End this AST refactor when all are true:

```text
- AST uses build_ast_environment / emit_ast_nodes / finalize_ast.
- AstBuildState is gone.
- Declaration slots replace append-only declaration mutation.
- ScopeContext no longer clones environment-wide maps.
- Constant retry loop is gone.
- Bounded expression token copying is gone.
- constant_fold no longer clones unchanged runtime RPN.
- compiler-design-overview and roadmap are updated.
- benchmark log shows phase-by-phase before/after results.
- just bench and just validate pass.
```

### Audit checklist

- Is any old pass-count language still present?
- Are there compatibility wrappers or parallel old/new APIs left?
- Are generated benchmark results uncommitted?
- Are benchmark log entries enough to judge performance?
- Are template and type-environment follow-ups captured but not mixed into this refactor?
- Does the module layout match the style guide expectations?

### Commit contents

- Final documentation updates.
- Final benchmark log summary.
- Final cleanup.
- Audit notes.

## Expected implementation order summary

```text
1. Benchmark log + timers/counters + roadmap/type-env plan
2. Replace AstBuildState with explicit phase structs
3. Stable declaration table
4. Constant dependency graph
5. ScopeContext shared environment redesign
6. Expression/parser churn cleanup
7. Conservative finalization/template cleanup
8. Final docs/benchmark/roadmap audit
```

Phases 3, 4, and 5 may reveal ordering constraints during implementation. If so, prefer keeping phase commits green over preserving this exact order. Do not split into half-migrated commits.

## Risks

### Risk: Phase 2 becomes too large

Replacing `AstBuildState` immediately is intentionally disruptive. Keep the commit reviewable by moving code into new owners with minimal semantic changes first. Optimizations follow in later phases.

### Risk: Constant dependency extraction duplicates parser logic

Avoid this by reusing/extending the existing constant declaration shell/header output. Do not add a second full expression parser.

### Risk: ScopeContext redesign overcomplicates local lookup

Keep local declarations owned in the first final shape. Only introduce persistent local frames if benchmark counters prove local clone pressure remains significant.

### Risk: Template finalization dominates after other fixes

Record roadmap notes with measured evidence. Do not redesign templates inside this plan unless it blocks the AST architecture.

### Risk: Benchmarks are noisy

Use `just bench` full runs for phase before/after. Compare mean and median. Classify regressions using the defined thresholds.

## Required per-phase commit format

Each code-changing phase commit should include:

```text
- code changes
- benchmark log before/after entries
- relevant tests
- relevant docs/roadmap updates
- audit notes
```

Do not commit:

```text
benchmarks/results/
```

## Final handoff for the future TypeEnvironment plan

After this refactor, start `docs/roadmap/plans/type-environment-redesign-plan.md`.

That plan should focus on:

```text
- compact type IDs
- nominal type definition table
- generic instance interning
- separating type identity from type layout
- reducing DataType cloning in signatures, expressions, HIR, and generics
- preserving current user-facing type semantics
```

Do not start that work until this AST refactor hits the stop criteria.
