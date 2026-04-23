# AST Prealpha Frontend Review Plan

This plan covers concrete frontend refactor work identified during a prealpha review of the compiler frontend, with a focus on the AST stage and template-heavy workloads.

It is intentionally scoped to **high-value simplification and performance cleanup** that fits prealpha constraints. It does **not** attempt a broad redesign of template semantics or a post-alpha optimisation pass.

This plan is anchored in the current repository shape and current frontend contract.

---

## Why this plan exists

The roadmap already calls out two relevant facts:

- the AST stage is currently a major frontend bottleneck
- `parse_function_body_statements()` is currently the dominant hot path
- optimised template folding is explicitly deferred until after Alpha

This plan does **not** replace that roadmap note. It complements it.

The purpose of this plan is to remove **structural frontend cost** that is already visible in the current AST and template pipeline without introducing large prealpha complexity.

In the current repo, the main extra cost appears to come from four places:

1. repeated cloning of AST scope state during body parsing
2. local-scope lookup that still scales linearly with local declaration count
3. repeated template composition / reconstruction / render-plan rebuilding
4. a broad AST-wide template normalization sweep that likely does more work than necessary

---

## In scope

This plan targets these current frontend areas:

- `src/compiler_frontend/ast/mod.rs`
- `src/compiler_frontend/ast/module_ast/scope_context.rs`
- `src/compiler_frontend/ast/statements/body_dispatch.rs`
- `src/compiler_frontend/ast/templates/create_template_node.rs`
- `src/compiler_frontend/ast/templates/template_types.rs`
- `src/compiler_frontend/ast/templates/template_composition.rs`
- `src/compiler_frontend/ast/templates/template_slots.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/template_folding.rs`
- `src/compiler_frontend/ast/module_ast/finalization/normalize_ast.rs`
- related AST tests and integration fixtures

---

## Explicit non-goals

These are deliberately out of scope for this plan:

- redesigning template semantics
- changing slot / `$children(..)` behaviour
- replacing the current AST -> HIR template contract
- introducing a new long-term template IR
- broad post-alpha template-folding optimisation work
- changing language surface or user-visible syntax
- large arena-allocation refactors across the whole frontend

If any step below starts to push toward one of those, stop and split it into a post-alpha plan.

---

## Success criteria

This plan is complete when the following are true:

- AST body parsing uses cheaper local lookup than the current reverse linear scan model
- nested scope creation no longer clones more AST state than necessary
- template creation no longer rebuilds composition / render metadata more than needed for correctness
- AST finalization does not re-normalize obviously template-free subtrees
- template-heavy frontend builds are measurably faster in dev profiling
- the frontend contract remains the same and existing supported language behaviour stays stable
- docs and module comments accurately describe the real template pipeline

---

## Workstream 1: tighten `ScopeContext` and local lookup

### Problem

`TopLevelDeclarationIndex` is already indexed, but local lookup in `ScopeContext::get_reference()` still walks `local_declarations` backwards linearly.

At the same time, child contexts such as:

- `new_child_control_flow()`
- `new_template_parsing_context()`
- `new_constant()`
- `new_child_function()`

still duplicate more state than necessary, especially `local_declarations` and visibility data.

This is exactly the sort of cost that compounds inside `parse_function_body_statements()` and nested expression/template parsing.

### Files

- `src/compiler_frontend/ast/module_ast/scope_context.rs`
- `src/compiler_frontend/ast/statements/body_dispatch.rs`
- any statement / expression helper that depends on local declaration growth

### Concrete refactor steps

#### 1. Introduce indexed local lookup

Add a local declaration index alongside ordered local storage.

Target shape:

- keep ordered declaration storage for source-order semantics
- add local name lookup for O(1) or close-to-O(1) access
- preserve "latest visible local wins" semantics without reverse scanning the full vector

A reasonable prealpha shape is:

```rust
local_declarations: Vec<Declaration>
local_declarations_by_name: FxHashMap<StringId, Vec<u32>>
```

or an equivalent bucketed layout like the current `TopLevelDeclarationIndex`.

#### 2. Route `add_var()` through the local index

Whenever a local is added:

- append to ordered storage
- append index into the name bucket
- preserve the current visibility-gate insertion behaviour

#### 3. Change `get_reference()` to use indexed local lookup first

Current resolution order should remain:

1. nearest visible local
2. visible top-level declaration

Only the local retrieval mechanism should change.

#### 4. Stop cloning full local state in child contexts unless the child actually forks scope state

Review each constructor separately.

- `new_child_control_flow()` should not blindly clone everything if it only needs a new scope id, loop depth update, and shared lookup state
- `new_template_parsing_context()` should share as much immutable state as possible
- `new_constant()` should do the same
- `new_child_function()` should remain the point that creates a fresh body-local environment from parameters

The key rule is:

> child contexts should share read-mostly state and only own the pieces that genuinely differ

#### 5. Consider splitting shared state from mutable-local state

If this simplifies clone pressure, introduce an internal split such as:

- shared scope environment
- local declaration layer

Do not add abstraction for its own sake. Only do this if it materially reduces duplication and keeps `ScopeContext` clearer.

### Acceptance checks

- local lookup no longer scans `local_declarations` linearly on every lookup
- nested loops / branches / templates do not clone full local state unnecessarily
- no frontend semantic change
- parser and integration tests still pass

---

## Workstream 2: reduce duplicate template finalization work

### Problem

`Template::new()` currently performs a multi-stage pipeline that includes:

- parse head
- parse body
- pre-format composition
- format body
- rebuild content from render plan
- post-format recomposition
- render-plan rebuild

Then AST finalization later walks the AST again and normalizes templates for HIR, including further metadata / render-plan refresh work.

That means template-heavy code can pay multiple times for closely related work.

### Files

- `src/compiler_frontend/ast/templates/create_template_node.rs`
- `src/compiler_frontend/ast/templates/template_types.rs`
- `src/compiler_frontend/ast/module_ast/finalization/normalize_ast.rs`
- `src/compiler_frontend/ast/mod.rs`

### Concrete refactor steps

#### 1. Define one authoritative owner for final runtime template metadata

Choose one of these models and make it explicit in code comments:

**Option A**
- `Template::new()` finalizes content shape only
- AST finalization owns final runtime metadata materialization

**Option B**
- `Template::new()` produces final runtime-ready metadata
- AST finalization trusts that and only handles cases changed by later AST rewrites

For prealpha, prefer the simpler option with fewer duplicate passes.

#### 2. Remove unconditional metadata rebuilding where the template has not changed

After the ownership point above is decided:

- do not call metadata rebuild helpers on templates whose content/kind did not change
- avoid rebuilding a render plan just because a later pass touched the surrounding AST node

#### 3. Separate "content changed" from "metadata inspected"

Add a small explicit rule in code:

- if content changed, resync metadata
- if content did not change, do not resync

That avoids broad defensive recomputation.

#### 4. Update comments in `ast/mod.rs` and template modules

The code currently has enough moving parts that ownership of template finalization can drift in docs. Tighten the module comments once the actual authority point is chosen.

### Acceptance checks

- final runtime template metadata is built in one clearly owned place
- render plans are not rebuilt redundantly for unchanged templates
- code comments explain the ownership boundary clearly

---

## Workstream 3: add cheap fast paths for simple templates

### Problem

The current template pipeline is clean but expensive because even simple templates move through the general composition / formatting / reconstruction machinery.

Many templates in docs and page-generation workloads are much simpler than the full general case.

### Files

- `src/compiler_frontend/ast/templates/create_template_node.rs`
- `src/compiler_frontend/ast/templates/template_types.rs`
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/template_folding.rs`

### Concrete refactor steps

#### 1. Add a trivial-template fast path in `Template::new()`

Fast-path templates with all of the following:

- no head atoms
- no child wrappers
- no unresolved slots
- no formatter
- no nested template expressions that require composition

These should skip unnecessary composition / recomposition work.

#### 2. Add an already-folded-content fast path

If a template body is already all compile-time string slices and does not need structural work:

- do not rebuild content through a render plan unless required
- do not create extra intermediate template wrappers

#### 3. Avoid render-plan materialization when no formatter or runtime planning needs it yet

If the template remains a compile-time-only value and no formatter requires a plan:

- keep the cheaper representation until a later stage actually needs the plan

#### 4. Keep all fast paths narrow and obvious

Do not add clever speculative optimisations.

Every fast path should be guarded by clear predicates and fall back immediately to the full pipeline when the template is not trivially simple.

### Acceptance checks

- simple templates no longer take the full heavy path
- fast-path logic is easy to read and test
- no template semantic drift

---

## Workstream 4: reduce representation bouncing in template handling

### Problem

The current template pipeline moves between:

- `TemplateContent`
- `TemplateRenderPlan`
- rebuilt `TemplateContent`

more often than is likely necessary.

That is especially costly because expressions and nested templates are cloned into render pieces and then reconstructed later.

### Files

- `src/compiler_frontend/ast/templates/template_render_plan.rs`
- `src/compiler_frontend/ast/templates/create_template_node.rs`
- `src/compiler_frontend/ast/templates/template_folding.rs`

### Concrete refactor steps

#### 1. Make `TemplateContent` the default source of truth during parsing and composition

Until formatting or HIR preparation requires a render plan, avoid unnecessary conversion into `TemplateRenderPlan`.

#### 2. Only rebuild `TemplateContent` from a render plan when formatting actually changed the content stream

If formatting produces a structurally equivalent plan or is not needed, do not bounce back through reconstruction.

#### 3. Audit render-plan cloning points

Review:

- `TemplateRenderPlan::from_content()`
- `TemplateRenderPlan::rebuild_content()`
- `Template::fold_into_stringid()`

Look for places where the full plan or full expressions are cloned defensively but are only read once.

Reduce those clones only when it keeps ownership clear.

#### 4. Keep the render plan authoritative only where it must be

The render plan exists for formatter-safe processing and final runtime planning.
It should not become an always-on intermediate representation for every simple template path if that is avoidable.

### Acceptance checks

- fewer conversions between content and render plan in simple/common cases
- template folding still behaves identically
- no readability collapse in template code

---

## Workstream 5: narrow AST template normalization in finalization

### Problem

`normalize_ast_templates_for_hir()` currently performs a broad recursive traversal over the AST to normalize embedded templates before HIR lowering.

This is correct, but likely too broad for the common case.

### Files

- `src/compiler_frontend/ast/module_ast/finalization/normalize_ast.rs`
- `src/compiler_frontend/ast/ast_nodes.rs`
- any expression helpers that could cheaply expose whether template work is present

### Concrete refactor steps

#### 1. Add a cheap way to identify template-free nodes / expressions

Potential options:

- a helper on expressions
- a helper on node kinds
- a small boolean carried during construction

Prefer the lightest approach that avoids infecting the whole AST with new bookkeeping.

#### 2. Skip recursion into obviously template-free subtrees

Examples likely worth skipping quickly:

- literal-only expressions
- operator-only runtime nodes with no template-bearing children
- control-flow branches whose bodies contain no templates

#### 3. Keep correctness over aggressiveness

Do not attempt clever partial normalization if it risks missing legal template-bearing descendants.

The goal is to cheaply avoid obviously pointless work, not to redesign finalization.

#### 4. Re-evaluate whether some normalization should happen earlier

If a small amount of normalization can safely move to node-emission time and reduce finalization cost, do that only when it simplifies the pipeline rather than scattering responsibilities.

### Acceptance checks

- finalization skips obviously template-free branches
- HIR still receives fully normalized runtime templates
- finalization ownership remains understandable

---

## Workstream 6: tighten expensive wrapper / slot composition paths

### Problem

The current wrapper and slot composition code is semantically careful, but template-heavy nested composition still appears structurally expensive because it performs a lot of cloning and wrapper reconstruction.

This is not a justification for changing semantics, but there are likely some cheap structural wins.

### Files

- `src/compiler_frontend/ast/templates/template_composition.rs`
- `src/compiler_frontend/ast/templates/template_slots.rs`
- `src/compiler_frontend/ast/templates/template_types.rs`

### Concrete refactor steps

#### 1. Identify the most common trivial composition cases and fast-path them

Examples:

- one wrapper, one child, no named slot routing
- no unresolved slots and no child wrappers
- no head-chain layers at all

#### 2. Avoid cloning wrapper templates when a read-only borrow is enough

Audit uses of:

- `clone_for_composition()`
- wrapper reconstruction in slot expansion
- layer-resolution cache outputs

Do not remove clones blindly. Only reduce them when ownership stays obvious.

#### 3. Keep pooled/layered composition structures only where they are actually paying their way

If any helper or intermediate structure exists only for an older path that no longer needs it, prune it.

#### 4. Document the cost boundaries in comments

These files are dense. Add short comments explaining where the expensive but necessary composition boundaries are, so future work does not reintroduce hidden churn.

### Acceptance checks

- composition code remains semantically identical
- trivial wrapper/slot cases take less work
- clone-heavy paths are reduced where safe

---

## Workstream 7: documentation and contract cleanup

### Problem

Parts of the template-module comments still describe older top-level runtime-template behaviour that no longer matches the current frontend contract.

That makes optimisation work harder because the intended ownership boundary is less obvious than it should be.

### Files

- `src/compiler_frontend/ast/templates/mod.rs`
- `src/compiler_frontend/ast/mod.rs`
- `docs/compiler-design-overview.md`
- `docs/roadmap/roadmap.md` if a roadmap link or note should be updated

### Concrete refactor steps

#### 1. Update the template module overview to match the current entry-fragment model

The docs should match the current behaviour where top-level runtime templates remain in the entry `start()` body rather than being described through older synthetic-fragment-function language.

#### 2. Tighten comments around template finalization ownership

Wherever final metadata/render-plan ownership lands after Workstream 2, document it in:

- the template constructor path
- AST finalization
- the AST -> HIR boundary comments

#### 3. Keep comments short and structural

These files need comments that explain ownership and pipeline shape, not long narrative comments that drift.

### Acceptance checks

- module comments match current repo behaviour
- future frontend work can identify the template ownership boundary quickly

---

## Suggested implementation order

This order is chosen to maximize value while keeping risk low.

### Phase 1 — cheapest AST hot-path win

1. Workstream 1: `ScopeContext` and local lookup

This is the best first step because it improves the known AST hot path directly and does not depend on template redesign.

### Phase 2 — remove duplicate template work

2. Workstream 2: authoritative template finalization ownership
3. Workstream 3: simple template fast paths

This should cut cost without changing semantics.

### Phase 3 — narrow broad passes

4. Workstream 5: narrower AST template normalization
5. Workstream 4: reduce representation bouncing where still useful

### Phase 4 — composition cleanup and docs

6. Workstream 6: tighten wrapper / slot composition paths
7. Workstream 7: docs and contract cleanup

---

## Validation plan

After each phase:

```bash
cargo clippy
cargo test
cargo run -- tests
```

For profiling comparisons, use the existing detailed timing path and template-heavy fixtures.

Suggested checks:

```bash
cargo run --features "detailed_timers" -- build <heavy-template-entry>
cargo run --features "detailed_timers" -- build tests/cases/<relevant-case>/input/main.bst
```

If there is a dedicated heavy benchmark file or docs-site page that stresses templates, include it in before/after timing notes for every phase.

---

## Recommended test additions

Add or extend tests only where they pin down behaviour that these refactors are likely to disturb.

### AST / scope tests

- deep nested branch / loop scopes with many locals
- local shadowing rejection still behaving correctly
- nearest-local resolution still winning over visible top-level declarations
- import visibility gates still respected

### Template tests

- trivial plain templates
- templates with head atoms but no wrappers
- templates with one wrapper and one child
- templates with named slots and loose content routing
- nested child template output that must still preserve wrapper behaviour
- templates that remain runtime due to one dynamic expression among otherwise constant content

### Finalization tests

- template-free AST nodes remain untouched
- runtime templates still reach HIR with valid final metadata
- compile-time templates still fold identically

Prefer integration tests where possible for the externally visible behaviour, and focused unit tests only for dense template helpers that are hard to pin down through end-to-end cases.

---

## Risks and guardrails

### Risk 1: over-abstracting `ScopeContext`

Avoid introducing multiple new context layers unless they clearly reduce copying and make the code easier to read.

### Risk 2: accidental template semantic drift

Do not change slot or wrapper behaviour while chasing performance.
Keep refactors mechanical and strongly covered by tests.

### Risk 3: splitting template ownership across even more places

If a refactor makes it less clear where template metadata becomes authoritative, back it out and simplify.

### Risk 4: micro-optimising uncommon paths first

Use timing output and template-heavy fixtures to confirm the work is hitting real costs.

---

## Done when

This plan is done when:

- AST scope handling is cheaper and clearer
- local lookup is indexed
- template creation and finalization do not rebuild the same structural data more than necessary
- AST finalization skips obviously irrelevant work
- docs describe the real template pipeline
- the repo remains simpler, not more layered

The intended result is a **faster, cleaner prealpha frontend** without pulling large post-alpha optimisation work into scope.
