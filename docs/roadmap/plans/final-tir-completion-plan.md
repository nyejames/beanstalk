# Beanstalk Final TIR Completion Plan

## Purpose

Complete the Template IR migration around one fast AST-local representation and one unambiguous path from parser output to HIR.

```text
template syntax
-> parser-local TIR construction
-> one module-scoped TemplateIrStore
-> exact TirView
-> one semantic preparation pass
-> folded string or owned runtime handoff
-> HIR
```

The final implementation must preserve template behaviour while deleting multi-store ownership, duplicated classification and safety walks, incomplete view identity, fallback terminology and implementation-shaped tests.

TIR remains AST-local. No TIR store, ID, view, overlay or preparation type may cross into HIR, a backend or a completed compiler module.

## Required authority documents

- `docs/compiler-design-overview.md` for AST ownership, TIR boundary and HIR handoff contracts
- `docs/build-system-design.md` for build orchestration context only; TIR ownership stays in the compiler document
- `docs/src/docs/codebase/style-guide/style-guide.bd`, `testing.bd` and `validation.bd`

## Current state

```text
ACTIVE_PLAN: docs/roadmap/plans/final-tir-completion-plan.md
STATUS: active
CURRENT_SLICE: R6D downstream roadmap handoff and dedicated post-TIR optimisation plan
LAST_ACCEPTED_COMMIT: e4aef3987 (R6C exact const-required preparation handoff and one-attempt regression coverage)
WORKTREE: main at e4aef3987; clean before this parent-owned benchmark-state refresh; concurrent user documentation remains untouched
REQUIRED_RELOADS: startup files, this plan, and current TIR source/diff
RELEVANT_CONTEXT_NOW:
- docs: roadmap and queued plans touching templates, AST finalization, HIR handoff, module compilation, diagnostics or performance
- code: final one-store/exact-view/prepared fold and owned runtime-handoff owners established by accepted R0-R6C checkpoints
ACCEPTANCE_CRITERIA:
- review every queued plan against final TIR owners and remove stale multi-store, fallback, duplicate-preparation and deleted-API assumptions
- create one dedicated post-TIR `$md` and template-parser optimisation plan owning source-slice text, formatter allocation, incremental caching, parallel folding and backend string-build work
- refresh queued dependency/current-state capsules against the accepted repository checkpoint and update roadmap sequencing without reopening final TIR architecture
VALIDATION_STATE:
- R2C just validate: passed; cross-target Clippy, 3421 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 23 faster and 0 slower
- R3 ownership map: passed through Codex CLI simple-exploration; no repeated preparation proving a cache, new preparation.rs is the required final owner, and classification/control-flow predicates remain only where they answer earlier-stage questions
- R3 targeted preparation, const-required, runtime-slot cache and wrapper tests: passed
- R3 just validate: passed; cross-target Clippy, 3434 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 22 faster and 0 slower
- TIR inventory: 19,948 production and 17,712 test lines (R2C: 19,963 and 17,094; 069a29acb: 24,274 and 27,231)
- deletion, stale-terminology, HIR/backend TIR-boundary and git diff checks: passed
- R4 ownership map: passed through Codex CLI simple-exploration; all production callers and cache/handoff/finalization owners mapped; one A-D checkpoint is the smallest state without transitional parallel APIs
- R4 Codex implementation and correction slices: accepted after parent review and final cache/handoff cleanup
- R4 just validate: passed; cross-target Clippy, 3435 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 23 faster and 0 slower
- TIR inventory: 18,851 production and 17,747 test lines (R3: 19,948 and 17,712; 069a29acb: 24,274 and 27,231)
- R5 ownership/test map: passed through Codex CLI simple-exploration; remove Template.kind first, then consolidate reactive metadata, then perform primary-owner test cleanup
- R5B Codex implementation and parent-review correction slices: accepted; Template has exactly two fields, missing kind authority remains an infrastructure error, scalar coercion is template-free and the synchronization-only test is deleted
- R5B just validate: passed; cross-target Clippy, 3434 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 22 faster and 0 slower
- TIR inventory: 18,858 production and 17,739 test lines (R4: 18,851 and 17,747; 069a29acb: 24,274 and 27,231)
- R5A Codex implementation and parent-review correction slices: accepted; one exact-view metadata reducer now owns structural, nested-value, wrapper and resolved-slot transitions while annotation and owned handoff retain their distinct outputs
- R5A focused validation: passed; formatting, cargo check, 9 reactive metadata tests, 19 flow-aware collector tests, 27 normalization tests and git diff checks
- R5A just validate: passed; cross-target Clippy, 3438 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 22 faster and 0 slower
- TIR inventory: 18,861 production and 17,739 test lines (R5B: 18,858 and 17,739; 069a29acb: 24,274 and 27,231)
- R5C Codex implementation and parent-review correction slices: accepted; the obsolete test-only linear-current-state helper, its implementation-shaped assertions and five redundant tests are deleted, while the distinct normalization-to-runtime reactive metadata boundary remains covered
- R5C just validate after the parent-review correction: passed; cross-target Clippy, 3433 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 23 faster and 0 slower
- final R5 TIR inventory: 18,854 production and 17,681 test lines (R5A: 18,861 and 17,739; 069a29acb: 24,274 and 27,231)
- R6A-R6B Codex implementation and parent-review cleanup slices: accepted; Rust module maps and compiler-design now name one store, exact identity/transitions, one preparation owner, prepared fold/runtime reducers and the neutral HIR boundary; obsolete formatter-anchor, current-state and store-clone instrumentation surfaces are deleted
- R6B TIR identifier/phrase and HIR/backend import gates: passed; the four global `fallback path` hits are exact non-TIR owners in JS map/string safety, AST/HIR fallible return-shape handling and borrow-checker registry-drift protection
- R6A-R6B just validate after the parent doctest correction: passed; cross-target Clippy, 3433 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 22 faster and 0 slower
- final R6A-R6B TIR inventory: 18,740 production and 17,681 test lines (R5: 18,854 and 17,681; 069a29acb: 24,274 and 27,231)
- R6C Codex CLI counter/caller review: every retained counter and production preparation caller mapped; no preparation cache exists; one exact same-identity, same-mode repeat was found where const-required construction prepares a top-level const template and the emitter immediately prepares it again before folding
- R6C Codex CLI preparation-handoff slice: accepted after parent review; const-required construction returns the immediate Template plus PreparedTemplate result, emitter folding consumes it without a second preparation, expression parsing preserves the intentional ConstRequired-to-Value boundary and focused benchmark-counter coverage records one preparation attempt
- R6C preparation-handoff just validate: passed; cross-target Clippy, 3433 unit tests, 1784 integration cases, docs check and 28 benchmark sanity cases; -7ms average, 22 faster and 0 slower
- R6C benchmark-tooling review: ordinary recorded runs exercise representative end-to-end workloads with counters disabled; exact raw samples for `c1ecc2c58` and `069a29acb` are unavailable locally, so historical comparison must use the tracked summaries and record that attribution limit explicitly
- R6C six recorded just bench samples: passed; suite averages 14.628, 14.662, 14.674, 14.848, 14.782 and 14.794ms, with one -6ms comparison followed by five 0ms comparisons and 28/28 cases throughout
- R6C representative current workload means/ranges: template 9.010ms (8.858-9.109), wrapper/slot 6.745ms (6.552-6.911), control-flow 4.056ms (3.991-4.187), collection/control 14.953ms (14.846-15.023), Beandown/docs 199.144ms (196.023-204.026)
- R6C historical summary comparison: both c1ecc2c58 and 069a29acb carry the same July baseline of all/core/docs/stress/module/borrow ~21/5/228/16/13/10ms; the current summary is ~15/5/201/8/8/6ms, with no raw per-commit samples available for exact individual-case attribution
- R6C just bench-report: latest comparison 0ms with 28/28 cases, no counters, ratios or investigation candidates; the six samples show no consistent stage regression, so no profiling or architectural restoration is warranted
DOCS_IMPACT: compiler-design-overview.md and Rust module maps updated for the final architecture; index.md locator already names preparation.rs; progress matrix unchanged because user-visible support did not change
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: codex-cli read-only roadmap review - user requires Codex CLI for worker slices and R6D needs a bounded cross-plan stale-assumption/dependency map before parent documentation edits
NEXT_WORKER_ORDER: codex-cli (user-required provider for the next worker slice)
STOP_REASON: none
NEXT_RESUME_ACTION: commit accepted R6C benchmark evidence, then launch the bounded Codex CLI R6D queued-plan review
```

Use `069a29acb` as the implementation and regression base. Do not continue extending `FoldAuthorityWalk`, foreign-store traversal, external expression-overlay stacks or prepared foreign-wrapper proofs.

## Downstream handoffs

Final TIR completion unblocks:

- the Number plan, which consumes the shared value-to-string path and one-store folding
- the entry config plan, which folds `config:` blocks through the ordinary module AST path
- the canonical module plan, which requires stable template folding before immutable module artefacts can carry folded constants

### Mandatory post-TIR roadmap review

After final TIR completion is accepted and before the canonical module plan becomes active:

- refresh every queued plan against the final TIR owners and deleted APIs
- remove every remaining legacy fallback, multi-store, foreign-store and duplicate-preparation assumption
- refresh current-state capsules and implementation paths
- verify the queued dependency chain
- confirm that downstream plans consume one module-scoped store, exact `TirView`, one preparation owner and the final folded-string or owned-runtime handoff
- record the reviewed repository commit in each queued plan

This checkpoint changes documentation and plan assumptions only. It does not reopen accepted TIR architecture.

## Deletion obligations

Final completion must delete:

- registry and multi-store ownership (`TemplateIrRegistry`, `RegisteredTemplateIrStore`)
- foreign-store paths (`foreign_slot_insert_proxy.rs`, foreign child, wrapper, slot-source and fold paths)
- overlay-stack reconstruction (`TemplateOverlaySet`, `TemplateOverlaySetId`, external expression-overlay stacks)
- content fallback (detached content, per-template stores, `TemplateContent` fallback)
- duplicate classification (`fold_safety.rs`, `FoldAuthorityWalk`, authority tokens)
- compatibility names (`TemplateRef`, `TemplateNodeRef`, `TemplateWrapperSetRef`, store-qualified IDs)

## Final architecture

### One module-scoped TIR store

One AST module build owns one `TemplateIrStore` containing all template arenas and overlay payloads:

```rust
pub(crate) struct TemplateIrStore {
    templates: Vec<TemplateIr>,
    nodes: Vec<TemplateIrNode>,
    wrapper_sets: Vec<TemplateWrapperSet>,
    slot_plans: Vec<TemplateSlotPlan>,
    expression_overlays: Vec<TirExpressionOverlay>,
    slot_resolution_overlays: Vec<TirSlotResolutionOverlay>,
    wrapper_context_overlays: Vec<TirWrapperContextOverlay>,
    // existing counters and side tables
}
```

Move the current overlay vectors and their allocation/look-up APIs from `TemplateIrRegistry` into `TemplateIrStore`. Do not add a second overlay registry or a new owner beside the store.

Production contexts carry one shared `Rc<RefCell<TemplateIrStore>>`. `ScopeContext::new` and other production constructors must receive this handle explicitly. They must not allocate scratch stores that are immediately replaced.

Delete:

- `TemplateIrRegistry`
- `RegisteredTemplateIrStore`
- registry store vectors and store-handle lookups
- `TemplateStoreId`
- `TemplateStringDomainId`
- `TemplateIrStoreOwner`
- store freezing and string-domain validation
- cross-registry owner checks
- foreign-store child, wrapper, slot-source and fold paths
- `foreign_slot_insert_proxy.rs`

A future second-store design requires a separate plan with a real production owner and measured benefit.

### Keep direct parser emission

`TemplateParserIrBuilderState` continues to emit text, expressions, child templates, slots, inserts and control-flow roots directly into the shared module store.

Do not restore detached content, per-template stores, routine subtree cloning, a parallel AST template tree or a second render plan.

Every template value parsed during one module build belongs to the same store. Template-valued head and body expressions must therefore become structural `ChildTemplate` or `InsertContribution` nodes immediately rather than opaque dynamic expressions that need later foreign conversion.

### Module-local references

All TIR IDs are module-local typed IDs into the one store.

Target reference shapes:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateViewContext {
    pub(crate) expression_overlay: Option<TirExpressionOverlayId>,
    pub(crate) slot_resolution: Option<TirSlotResolutionOverlayId>,
    pub(crate) wrapper_context: Option<TirWrapperContextOverlayId>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateTirReference {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateTirChildReference {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TemplateWrapperReference {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}
```

`TemplateViewContext` is carried by value. Do not replace `TemplateOverlaySetId` with another context ID or canonical context table. Back optional overlay IDs with `NonZeroU32` and index-plus-one encoding so each `Option<...Id>` stays one word without sentinel values. Record the resulting context/reference sizes as a focused performance invariant.

Delete:

- `TemplateRef`
- `TemplateNodeRef`
- `TemplateWrapperSetRef`
- `TemplateOverlaySet`
- `TemplateOverlaySetId`
- overlay-set allocation, lookup and composition
- store-qualification helpers such as `template_id_in_store`

Move durable reference types out of `parser_builder_state.rs` into `refs.rs`. Keep parser builder state limited to in-progress parser construction.

### Thin durable `Template`

Target shape:

```rust
pub(crate) struct Template {
    pub(crate) tir_reference: TemplateTirReference,
    pub(crate) location: SourceLocation,
}
```

`TemplateIr.kind` is the sole post-construction template-kind owner. Remove `Template.kind`, kind synchronization methods and foreign-boundary cache comments.

The phase sequence remains:

```text
Parsed -> Composed -> Formatted -> Finalized
```

Folding requires `Composed` or later. AST-to-HIR handoff requires `Finalized`.

### Complete `TirView` identity

`TirView` borrows the one store and carries one exact identity:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TirViewIdentity {
    pub(crate) root: TemplateIrId,
    pub(crate) phase: TemplateTirPhase,
    pub(crate) context: TemplateViewContext,
}

pub(crate) struct TirView<'a> {
    store: &'a TemplateIrStore,
    identity: TirViewIdentity,
}
```

No consumer may carry a separate expression-overlay stack, store token, authority token or active root identity that is not represented by `TirViewIdentity`.

`ExpressionSiteId`, `SlotOccurrenceId` and `ChildTemplateOccurrenceId` remain allocated from module-wide store counters. Numeric collisions between unrelated templates are impossible inside one store.

### Complete expression-overlay invariant

An expression overlay attached to a composed or finalized value root must contain the effective overrides for every structural descendant reached through:

- child-template nodes
- wrappers
- resolved slot sources
- branch and fallback bodies
- loop bodies and aggregate wrappers
- structural helper roots

When finalization creates a root expression overlay, reuse the existing site-keyed normalization collector and merge structural descendant overrides into that root overlay once. The outer, more contextual override wins when the same reused expression site appears in both maps.

Structural expressions without an override remain read directly from their TIR node.

This replaces root-first overlay stacks. Expression lookup is one overlay lookup followed by structural fallback.

### Two explicit view transitions

All recursive consumers use methods on `TirView`. They must not calculate phase or overlay transitions locally.

#### Structural transition

Used for TIR child nodes, wrappers and resolved slot sources.

- Retain the current view's complete expression overlay.
- For a `Parsed` reference, ignore the referenced slot and wrapper overlays.
- For a `Composed` or later reference, use the referenced slot and wrapper overlays.
- Preserve the referenced root and phase.

Provide named methods such as:

```rust
view.structural_child(reference)
view.wrapper(reference)
view.resolved_slot_source(root)
```

#### Nested value transition

Used when an AST expression contains an independently owned `Template` value.

- Start from the nested template's complete `TemplateTirReference.context`.
- Do not retain the containing structural root's expression overlay.

Provide one named method such as:

```rust
view.nested_template_value(reference)
```

This distinction resolves both known overlay bugs without store comparisons or stack-reset heuristics.

### One semantic preparation owner

Replace authority validation, fold safety and immediate pre-fold classification with one owner in `tir/preparation.rs`.

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TemplatePreparationMode {
    Value,
    ConstRequired,
}

pub(crate) enum PreparedTemplate {
    Foldable(PreparedFold),
    Runtime(PreparedRuntime),
    Helper(TemplateHelperKind),
}

pub(crate) struct PreparedFold {
    pub(crate) identity: TirViewIdentity,
    pub(crate) value_kind: TemplateConstValueKind,
}

pub(crate) struct PreparedRuntime {
    pub(crate) identity: TirViewIdentity,
    pub(crate) reason: RuntimeTemplateReason,
}
```

The enum must make contradictory states impossible. Do not return an optional folded value beside a second disposition enum.

Preparation:

- validates every reachable required root, node, overlay, wrapper set, slot plan and render root
- follows structural and nested-value transitions through `TirView`
- validates all authority even after discovering runtime dependence
- detects cycles by `TirViewIdentity`
- distinguishes helper values from final template values
- records one foldable or runtime disposition
- preserves lazy runtime semantics in `Value` mode
- applies const-required branch, loop and helper rules in `ConstRequired` mode
- returns `CompilerError` for missing authority
- lets the owning caller convert valid runtime dependence into the existing const-required diagnostic

Preparation is a compact semantic result, not a cloned node plan or second IR.

Use a preparation cache only when its key includes every semantic input:

```text
TirViewIdentity + TemplatePreparationMode + const-loop limit
```

Binding-dependent preparation remains uncached unless binding identity becomes explicit.

### One fold entry

```rust
pub(crate) fn fold_prepared_template(
    prepared: &PreparedFold,
    view: TirView<'_>,
    context: &mut TemplateFoldContext<'_>,
) -> Result<TemplateEmission, TemplateError>;
```

The function verifies that `prepared.identity == view.identity()` before cache lookup.

Only `PreparedTemplate::Foldable` reaches the folder. Delete:

- `FoldAuthorityWalk`
- `FoldAuthorityToken`
- `PreparedTirViewFold`
- `PreparedTirFoldDecision`
- `ViewNativeWalkContext`
- `ReadOnly` versus `Direct`
- `fold_tir_view_prepared`
- prevalidated and expression-stack fold entries
- prepared foreign wrappers
- foreign fold cycle stacks

The fold walker trusts preparation, uses `TirView` transitions and performs no recursive authority or eligibility preflight.

A runtime slot plan is never folded as `NoOutput`. `NoOutput`, `Break` and `Continue` remain structural emission states only.

### One handoff path

Owned runtime handoff materialization remains a distinct reducer because it produces different data from folding. It consumes `PreparedRuntime` and the same exact `TirView` used by preparation.

Handoff must use the same structural and nested-value transition methods. It must not classify again, reconstruct overlays or use a folded-child shortcut with different semantics.

HIR continues to receive only owned runtime handoff payloads or folded string expressions.

### One finalization decision

Replace `try_fold_template_to_string`, `TemplateFinalizationFoldDisposition` and `TemplateFinalizationFoldResult` with one finalization owner, provisionally `template_value_finalization.rs`.

Call-site policy:

- ordinary AST template expression: prepare in `Value` mode, then fold or materialise runtime handoff
- module constant: prepare in `ConstRequired` mode, then fold or emit the existing non-foldable-const diagnostic
- top-level const fragment, `$doc` and Beandown content: `ConstRequired`
- slot inserts, slot definitions and loop-control helpers: consumed or rejected by their composition owner before the final value boundary

Module-constant and AST-expression paths share preparation and folding. They differ only in whether a runtime result is valid and which established diagnostic owns rejection.

### Exact cache semantics

Adapt the existing `TirFoldCache` rather than adding another cache layer.

A fold key includes:

```text
TirViewIdentity + const-loop limit + empty-binding proof
```

Binding-sensitive folds remain uncached unless a stable binding key is introduced deliberately.

Cache lookup happens only after preparation identity has been checked. A cache hit must never hide malformed authority or a runtime disposition.

## Existing systems to reuse

Keep and simplify these systems rather than replacing them:

- `TemplateIrStore` typed arenas and module-wide occurrence counters
- direct parser TIR construction
- `TemplateIrSummary` for capacity hints, output-size estimates and conservative cheap facts
- `TemplateTirPhase`
- the `TirView` effective-read concept
- immutable expression, slot-resolution and wrapper-context overlay payloads
- existing slot schema and loose-fill routing
- wrapper-set reuse and equivalence checks
- `TirCopyState` only where a real derived tree needs copied nodes or fresh occurrence IDs
- existing owned runtime handoff types and handoff walker
- `TirFoldCache`, with the corrected key
- the expression-payload walker for nested AST expressions
- output-size reservation and fold counters

Do not keep an old owner merely because its implementation can be reused. Move useful logic into the final owner and delete the obsolete surface.

## Module ownership map

Expected final owners:

```text
ast/templates/
  template.rs                 shared template vocabulary and thin Template
  template_folding.rs         fold context, bindings and TemplateEmission
  runtime_handoff.rs          neutral owned handoff vocabulary
  reactive_template_metadata.rs

ast/templates/tir/
  mod.rs                      concise module map and contracts
  ids.rs                      module-local typed IDs
  refs.rs                     durable, child and wrapper references
  store.rs                    all TIR arenas, overlays and side tables
  overlays.rs                 overlay payloads and TemplateViewContext
  view.rs                     identity, effective reads and transitions
  preparation.rs              sole semantic preparation owner
  fold.rs                     prepared constant folding
  handoff_materialization.rs  prepared runtime materialisation
  formatter_view.rs           formatter read/write boundary
  render_unit.rs              branch and aggregate derived-root construction
  wrapper_sets.rs             wrapper-set storage policy
  slot_composition/           slot schema, routing and composition
```

Delete `registry.rs`, `fold_safety.rs` and `foreign_slot_insert_proxy.rs`.

Do not create a generic visitor framework or cosmetic one-type modules. Split a surviving file only when it remains mixed-responsibility or above the style guide's practical ~2000-line target after deletion.

## Execution rules

- Work by final owner, not by old path versus new path.
- Do not commit forwarding shims, parallel context types or optional compatibility parameters.
- Mechanical intermediate states may exist only inside one uncheckpointed slice.
- Delete tests tied only to an owner in the same slice that deletes the owner.
- Preserve the `069a29acb` semantic regressions until their final owner has equivalent coverage.
- Do not add new cross-product tests while temporary 3E3c2e coverage already proves the case.
- Keep each phase net-negative in production TIR lines. Temporary growth inside a slice must be removed before its checkpoint.
- Keep functions under roughly 200 lines and files under roughly 2000 lines where practical.
- Preserve unrelated user-owned changes.

### Common code gate

Every code-bearing checkpoint runs:

```bash
cargo fmt
# focused tests for the changed owners
just validate
```

`just validate` already includes cross-target Clippy, unit tests, integration tests, docs checking and non-recording `bench-check`. Do not record a separate final `bench-check` as additional validation.

## Remaining implementation

### Phase R0 - Lock the ownership boundary

#### R0A - Production inventory

Run focused production and test inventories for:

```text
TemplateIrRegistry
RegisteredTemplateIrStore
allocate_store
allocate_in
adopt_store
store_handle
TemplateStoreId
TemplateIrStoreOwner
TemplateStringDomainId
foreign_slot_insert_proxy
TemplateRef
TemplateOverlaySetId
expression_overlay_stack
```

Classify every hit as:

- required production owner
- production compatibility or defensive path
- test-only fixture support
- obsolete migration architecture

Confirm:

- `AstPhaseContext` allocates one module store
- every production `ScopeContext` and constant context receives that same handle
- no completed AST, HIR, backend or compiled module stores raw TIR identity
- imported constants and source-backed package declarations are rebuilt into the consuming module's AST-local store rather than sharing a foreign TIR store

Record scoped production and test line counts for `src/compiler_frontend/ast/templates/tir` at `c1ecc2c58` and `069a29acb`.

#### R0 acceptance

- No required production second-store owner exists.
- Any contradiction identifies an exact production path and stops R1 before code changes.
- The current state block records the accepted ownership decision and next slice.

R0 accepted on the current `3b17bad3f` worktree. `AstPhaseContext` allocates the one module
store and all parsed constant headers use that handle. `ScopeContext::new` still allocates and
then replaces a scratch store, while registry adoption, foreign proxying and overlay stacks are
obsolete migration architecture owned by later phases. No raw TIR identity crosses completed AST,
HIR or backend boundaries. Under the same tracked-file classification (`*/tests/*`, `*_tests.rs`
and `*tests.rs` are tests), `c1ecc2c58` contains 22,084 production and 24,180 test lines in the
TIR directory; `069a29acb` contains 24,274 production and 27,231 test lines.

### Phase R1 - Collapse ownership and references

#### R1A - Require the shared store handle

- Make production `ScopeContext` construction require the module TIR handle.
- Remove default scratch registry/store allocation from `ScopeContext::new`.
- Update constant, type, trait, function-signature and body-emission contexts to pass the same handle.
- Keep test-only isolated store construction in local test helpers, not production constructors.

R1A completed in `7fd17f4b0`. `ScopeContext::new` requires the module store, every production
constructor passes that handle directly and only test helpers allocate isolated stores.

#### R1B - Merge registry storage into the store

- Move overlay payload vectors and APIs into `TemplateIrStore`.
- Move overlay dimension allocation and lookup callers to the store.
- Replace registry/store pairs in contexts with one store handle.
- Delete `TemplateIrRegistry` and `RegisteredTemplateIrStore` in the same checkpoint.
- Remove owner-token, store-ID, freeze and string-domain fields and APIs.
- Remove `Clone` from the full store if no final production owner needs whole-store snapshots. Rewrite tests to build the exact malformed or derived shape they require.

Do not leave a thin registry that forwards to the store.

#### R1C - Make references module-local

- Replace store-qualified roots and side-table refs with typed module-local IDs.
- Move durable reference types into `refs.rs`.
- Update wrapper sets, slot-resolution sources, child nodes, caches and diagnostics.
- Keep source locations for diagnostics rather than formatting store-qualified IDs into messages.

#### R1D - Delete foreign conversion

- Delete `foreign_slot_insert_proxy.rs` and its tests.
- Delete foreign child, wrapper, slot-source, metadata and fold branches.
- Make parser template values structural immediately.
- Simplify `render_unit.rs` so branch and aggregate candidates reuse same-store nodes directly.
- Remove `runtime_template_expression` conversion from render-unit preparation when it exists only for foreign templates.

#### R1 acceptance

- One shared `TemplateIrStore` is the only TIR owner in production.
- No store ID, owner token, store vector, foreign proxy or cross-store branch remains.
- Parser output, integration output and diagnostics are unchanged.
- Production and test lines are below the `069a29acb` base.
- Common code gate passes.

R1B-R1D accepted on the pre-commit worktree. `TemplateIrStore` now owns all overlay arenas and is
the only production TIR owner. Durable references contain module-local typed IDs, the registry,
store/owner identity and foreign conversion paths are deleted and parser template values are
structural before render-unit preparation. The final tracked-file inventory is 20,368 production
and 17,224 test lines versus 24,274 and 27,231 at `069a29acb`. `just validate` passed from a clean
target with 3,441 unit tests, 1,784 integration cases, docs checking and 28 benchmark sanity cases.

### Phase R2 - Make `TirView` complete

#### R2A - Replace overlay sets with value contexts

- Add copyable `TemplateViewContext` containing the three optional overlay IDs.
- Convert overlay IDs to `NonZeroU32`-backed newtypes so optional dimensions remain compact.
- Replace `TemplateOverlaySetId` fields on durable, child and wrapper references.
- Delete overlay-set storage, canonicalization and composition.
- Make the empty context `TemplateViewContext::default()`.

R2A accepted on the pre-commit `5fa9bb32f` worktree. References and `TirView` now carry a
12-byte `TemplateViewContext` directly, each optional overlay ID remains one 32-bit word and the
three reference shapes are pinned at 20 bytes. Overlay-set storage, identity, allocation,
canonicalization and composition are deleted without a forwarding API. The final tracked-file
inventory is 20,048 production and 16,727 test lines. `just validate` passed with 3,413 unit tests,
1,784 integration cases, docs checking and all 28 benchmark sanity cases.

#### R2B - Build complete expression overlays

- Reuse the existing expression-site normalization collector.
- Merge structural descendant overrides into each effective root overlay once.
- Preserve outer-context precedence for reused sites.
- Assert that all expression overlay entries refer to valid module-global `ExpressionSiteId`s.
- Keep structural fallback for sites with no override.

R2B accepted on the pre-commit `ee30d9aef` worktree. The existing finalization collector now
revisits shared structure by active context, preserves the outer effective override, emits one
payload per module-global expression site and writes one canonical root overlay. Store allocation
rejects unallocated expression sites. `just validate` passed with 3,417 unit tests, 1,784
integration cases, docs checking and all 28 benchmark sanity cases.

#### R2C - Centralize transitions

- Add structural child, wrapper, resolved-source and nested-value methods to `TirView`.
- Encode `Parsed` versus `Composed` slot/wrapper rules once.
- Delete every external `expression_overlay_stack`, manual push/pop and current-store comparison.
- Make cycle and cache identity use `TirViewIdentity`.

Focused invariants:

- a structural child retains the outer complete expression overlay
- an independently nested template value starts its own expression overlay
- a `Parsed` structural child ignores premature slot/wrapper context
- a `Composed` child uses its slot/wrapper context
- the same root under different view contexts remains cache-distinct
- reused expression sites obey outer-context precedence

#### R2 acceptance

- `TirViewIdentity` completely determines every effective read.
- No overlay context exists outside `TirView` or a TIR reference.
- No overlay-set table or expression stack remains.
- Common code gate passes.

R2C accepted on the pre-commit `144568587` worktree. `TirViewIdentity` is now the exact cache and
view-semantic cycle identity, and `TirView` owns the structural child, wrapper, resolved slot
source, structural helper and nested-value transitions. Structural transitions retain only the
current complete expression overlay, Parsed references ignore referenced slot/wrapper dimensions
and independently nested AST values start from their durable full context. Overlay stacks, the
store stack resolver, generic `child_view` and synthetic resolved-source child transitions are
deleted. The final tracked-file inventory is 19,963 production and 17,094 test lines. `just
validate` passed with 3,421 unit tests, 1,784 integration cases, docs checking and all 28 benchmark
sanity cases.

### Phase R3 - Introduce one preparation pass

#### R3A - Add preparation vocabulary

- Add `TemplatePreparationMode`, `PreparedTemplate`, `PreparedFold`, `PreparedRuntime`, `RuntimeTemplateReason` and `TemplateHelperKind` only where existing types do not already express the state.
- Keep the result compact and identity-based.
- Add a cache keyed by exact identity, mode and const-loop limit only if repeated preparation is observed in current callers.

#### R3B - Implement exhaustive preparation

- Traverse through `TirView` transitions.
- Validate roots, nodes, overlay IDs, wrapper sets, slot plans, render pieces and helper references.
- Continue validation after runtime dependence is found.
- Detect cycles by exact view identity.
- Preserve current const control-flow, slot, wrapper, reactivity and helper semantics.
- Use existing `TemplateIrSummary` only for safe cheap hints, never as authority.
- Reuse existing expression and slot-schema helpers rather than adding another expression or slot walker.

#### R3C - Delete replaced owners

- Delete `fold_safety.rs`.
- Delete fold authority tokens and decisions.
- Delete immediate full-template classification before folding.
- Delete redundant full-tree slot, insert and loop-control scans whose facts now come from preparation.
- Retain earlier parser-stage predicates only when they answer a genuinely earlier question and cannot consume a prepared view.

#### R3 acceptance

- Every effective template produces exactly one foldable, runtime or helper result.
- Missing required authority is always an internal compiler error.
- Runtime dependence is ordinary semantics, not a fallback.
- `fold_safety.rs` and authority tokens are gone.
- Common code gate passes.

R3 accepted on the pre-commit `5823343d4` worktree. `preparation.rs` now owns one
mode-aware exhaustive `PreparationWalk` over exact structural and nested-value views, returning
only `Foldable`, `Runtime` or `Helper`. Missing authority remains internal after runtime discovery,
only prepared foldable values enter folding or caching and runtime slot plans cannot disappear as
empty output. `fold_safety.rs`, authority tokens and immediate full-template decision walks are
deleted. The final tracked-file inventory is 19,948 production and 17,712 test lines. `just
validate` passed with cross-target Clippy, 3,434 unit tests, 1,784 integration cases, docs checking
and all 28 benchmark sanity cases.

### Phase R4 - Simplify fold, handoff and finalization

#### R4A - One prepared fold path

- Add `fold_prepared_template`.
- Remove prevalidated, read-only, direct, stack-carrying and foreign fold entries.
- Remove recursive authority and eligibility checks from `fold.rs`.
- Route child, wrapper and resolved slot sources through `TirView` methods.
- Reject runtime slot plans before entering fold dispatch.
- Remove duplicate active-root cycle stacks after preparation owns cycle rejection.

#### R4B - Exact fold caching

- Adapt `TirFoldCacheKey` to `TirViewIdentity`.
- Include the const-loop limit and empty-binding proof.
- Keep binding-dependent folds uncached.
- Check prepared/view identity before cache lookup.

#### R4C - Prepared runtime handoff

- Make handoff materialization accept `PreparedRuntime` plus the exact view.
- Remove folded-child shortcuts that bypass preparation or use different transition rules.
- Keep existing owned runtime handoff types and HIR lowering contracts.

#### R4D - One finalization owner

- Replace the current prepare/classify/fold/disposition sequence with one helper.
- Migrate AST expressions, module constants, top-level const fragments, `$doc` and Beandown callers to their explicit preparation mode.
- Delete `TemplateFinalizationFoldDisposition`, `TemplateFinalizationFoldResult` and `try_fold_template_to_string`.
- Keep module-constant and runtime-expression diagnostics distinct at the final caller boundary.

#### R4 acceptance

- Finalization never reclassifies a prepared template.
- Fold and handoff consume the same exact semantics.
- Runtime slot plans cannot disappear as empty output.
- The fold cache cannot hide malformed authority.
- Common code gate passes.

R4 accepted on the pre-commit `52f806697` worktree. `fold_prepared_template` is the sole fold
entry, verifies the preparation/view identity before cache lookup and shares exact-view cache reuse
with Composed child and resolved-source folds. Runtime handoff consumes `PreparedRuntime` plus that
same view, selects the specialized slot shape from the actual root plan and no longer folds child
templates through a shortcut. `FinalizedTemplateValue` now exclusively represents folded, runtime
or helper outcomes, and all final value boundaries use an explicit preparation mode. The final
tracked-file inventory is 18,851 production and 17,747 test lines. `just validate` passed with
cross-target Clippy, 3,435 unit tests, 1,784 integration cases, docs checking and all 28 benchmark
sanity cases.

### Phase R5 - Consolidate remaining consumers and tests

#### R5A - Remaining consumers

- Move reactive metadata traversal onto `TirView` transitions.
- Use prepared facts or the same view methods for const-required control-flow validation.
- Use exact finalized views for final type and debug validation.
- Remove raw-store authority helpers and registry-era comments.
- Keep distinct reducer matches where metadata, folding and handoff genuinely produce different results.

R5A accepted on the pre-commit `a8bfdb213` worktree. Template-backed reactive metadata now has
one exact-view reducer with exact-identity cycle state and shared structural, nested-value, wrapper
and resolved-slot transitions. The flow-aware collector supplies a Composed-or-later view and
normalization supplies a Finalized view. The raw-store root selector and duplicate node, template
and runtime-slot walkers are deleted, while flow-aware overlay annotation and owned runtime-handoff
metadata remain distinct reducers because they mutate or consume different representations.
Focused validation and `just validate` passed with cross-target Clippy, 3,438 unit tests, 1,784
integration cases, docs checking and all 28 benchmark sanity cases. The tracked TIR inventory is
18,861 production and 17,739 test lines.

#### R5B - Remove duplicate durable state

- Remove `Template.kind` and synchronization methods.
- Read `TemplateIr.kind` through the shared store.
- Keep mutable kind only in parser-local state before the `TemplateIr` entry is created.

R5B accepted on the pre-commit `a16cf5a1f` worktree. `Template` now carries only its exact TIR
reference and source location, `TemplateBuildState.kind` remains parser-local and `TemplateIr.kind`
is the sole durable owner. Construction writes the final classification before returning the thin
handle, missing store authority remains an internal error and the scalar value-to-string helper no
longer interprets templates without TIR authority. Direct fixtures and semantic kind assertions now
read the owning store, while the cache-synchronization-only test is deleted. The tracked TIR
inventory is 18,858 production and 17,739 test lines. `just validate` passed with cross-target
Clippy, 3,434 unit tests, 1,784 integration cases, docs checking and all 28 benchmark sanity cases.

#### R5C - Assign test ownership

Keep one primary owner for:

- rendered output and diagnostics
- expression context at structural and nested-value boundaries
- phase transition rules
- missing-authority errors
- slot routing, missing slots and repeated replay
- `$fresh` immediate-parent suppression
- wrapper order and `IfChildEmits`
- runtime slot applications in control flow
- runtime handoff
- cache identity
- cycle rejection
- reactive subscriptions

Then:

- move user-visible cases to `tests/cases`
- keep focused unit tests only for hidden invariants
- merge tests that differ only by old store numbering, owner tokens, entry point or walker implementation
- delete cross-store, owner-collision, foreign-proxy and removed-API fixtures
- delete broad shared fixture helpers that hide the exact invariant
- record final production and test line reductions from `069a29acb`

R5C accepted on the pre-commit `c649e0ea8` worktree. The obsolete test-only
`can_reuse_as_linear_current_state` accessor and its parser assertions are deleted rather than
replaced by another compatibility surface. Parser output and phase, exact-view transitions,
prepared fold caching and cycles, wrapper and slot semantics, owned HIR handoff, normalization,
reactive metadata and finalized type validation retain distinct primary owners. Five redundant
parser/cache tests and duplicate deep payload assertions are removed; parent review restored the
normalization test that protects reactive metadata preservation across the runtime-handoff
boundary. The final tracked TIR inventory is 18,854 production and 17,681 test lines versus
24,274 and 27,231 at `069a29acb`. `just validate` passed after the correction with cross-target
Clippy, 3,433 unit tests, 1,784 integration cases, docs checking and all 28 benchmark sanity
cases. The progress matrix is unchanged because this slice changes test ownership only.

#### R5 acceptance

- Each behaviour has one primary test owner.
- Test names describe semantics rather than implementation paths.
- Production and test lines are materially lower than `069a29acb`.
- Common code gate passes.

### Phase R6 - Document, measure and close

#### R6A - Final docs and module maps

- Update `docs/compiler-design-overview.md` to the one-store architecture.
- Update `templates/mod.rs` and `tir/mod.rs` as concise structural maps.
- Remove multiple-store, foreign-reference, freezing, authority-token and fallback wording.
- Document exact view context, structural versus nested-value transitions, preparation modes and the HIR boundary.
- Update progress docs only if user-visible support changed.

#### R6B - Hard grep gates

Require zero production hits unless an exact final use is documented:

```text
TemplateIrRegistry
RegisteredTemplateIrStore
TemplateStoreId
TemplateStringDomainId
TemplateIrStoreOwner
TemplateRef
TemplateNodeRef
TemplateWrapperSetRef
TemplateOverlaySet
TemplateOverlaySetId
foreign_slot_insert_proxy
FoldAuthorityWalk
FoldAuthorityToken
PreparedTirViewFold
PreparedForeignWrapper
ViewNativeWalkContext
expression_overlay_stack
read_only_safe
current-state
compatibility mirror
fallback path
```

Require zero TIR imports in HIR and backend modules except neutral owned runtime handoff vocabulary.

R6A-R6B accepted on the pre-commit `e17b8a278` worktree. The compiler contract and Rust
module maps now name one module store, module-local identities, complete `TirViewIdentity`, shared
structural and nested-value transitions, the sole exhaustive preparation owner, the sole prepared
fold entry, prepared runtime materialisation and neutral owned AST-to-HIR payloads. Parent review
also removed the unused formatter-anchor reservation plus five obsolete zero-only current-state
and full-store-clone counters rather than preserving dead instrumentation surfaces. The word-bounded
TIR identifier and exact stale-phrase greps are empty, and HIR/backend modules import no
TIR-internal type. Four global `fallback path` phrases remain only in exact non-TIR owners: JS
map/string copy safety, AST and HIR fallible return-shape handling, and borrow-checker registry-drift
protection. The final tracked TIR inventory is 18,740 production and 17,681 test lines versus
24,274 and 27,231 at `069a29acb`. After correcting a stray module-map doctest fence, `just
validate` passed with cross-target Clippy, 3,433 unit tests, 1,784 integration cases, docs checking
and all 28 benchmark sanity cases. The progress matrix is unchanged because support did not change.

#### R6C - Performance evidence

Retain counters for:

- preparation attempts and cache hits
- preparation nodes visited
- fold attempts and cache hits
- fold nodes visited
- wrapper applications
- output reservation and estimate misses
- owned handoff materialisations

Confirm adjacent finalization callers do not prepare the same view repeatedly.

After the architecture is stable:

```bash
just bench
just bench
just bench
just bench
just bench
just bench
just bench-report
```

The R6C caller review and correction are accepted on the pre-commit `4a1b80b26` worktree.
Every retained counter and production preparation caller was mapped. The review found one exact
same-identity, same-mode repeat between const-required construction and immediate top-level folding.
Const-required construction now returns its immediate `PreparedTemplate` evidence alongside the
thin durable `Template` handle, top-level folding consumes that evidence through the exact-view
identity check, and expression parsing deliberately prepares again in `Value` mode because runtime
dependence is valid at that distinct semantic boundary. No preparation cache or durable preparation
field was added. Focused benchmark-counter coverage observes one preparation attempt across
construction and folding. `just validate` passed with cross-target Clippy, 3,433 unit tests, 1,784
integration cases, docs checking and all 28 benchmark sanity cases; the sanity result was `-7ms`
average with 22 faster and no slower cases.

R6C recorded performance evidence is accepted at `e4aef3987`. Six end-to-end samples on the same
macOS Apple Silicon system recorded suite averages of 14.628, 14.662, 14.674, 14.848, 14.782 and
14.794ms. The first was 6ms faster than the preceding local baseline and each subsequent comparison
was 0ms, with all 28 cases passing throughout. Representative means and ranges were: template
9.010ms (8.858-9.109), wrapper/slot 6.745ms (6.552-6.911), control flow 4.056ms
(3.991-4.187), collection/control flow 14.953ms (14.846-15.023), and Beandown/docs
199.144ms (196.023-204.026). `just bench-report` found no counters, ratios or investigation
candidates and no consistent stage regression.

The required historical comparison has an explicit evidence limit: local raw history contains no
samples for `c1ecc2c58` or `069a29acb`, and ordinary recorded runs intentionally disable detailed
counters. Both commits carry the same tracked July summary baseline of approximately 21/5/228/16/13/10ms
for all/core/docs/stress/module/borrow, versus the current 15/5/201/8/8/6ms summary. That supports
no aggregate regression, but it cannot prove an exact individual-case delta against either commit.
The stable six-run current ranges therefore close R6C without speculative profiling or restoring
deleted architecture.

Compare representative template, wrapper, slot, control-flow, Beandown and docs workloads against `c1ecc2c58` and `069a29acb`. Attribute consistent regressions with counters or profiling. Do not restore deleted architecture to hide a regression.

#### R6D - Roadmap handoff

- Review active plans touching templates, AST finalization, HIR handoff, module compilation, diagnostics or performance.
- Remove stale pre-TIR assumptions.
- Create one dedicated post-TIR `$md` and template-parser optimisation plan.
- Move source-slice text, formatter allocation, incremental caching, parallel folding and backend string-build work into that owner.
- Delete this temporary completion plan after final architecture docs own the contract.

#### R6 acceptance

- Hard grep gates pass.
- Common code gate passes.
- Recorded benchmark evidence is reviewed.
- Manual architecture audit finds no duplicate required path, compatibility shim, mixed owner or stale comment.
- Final docs match implemented code.

## Final acceptance

TIR is complete only when:

- one AST module build owns one `TemplateIrStore`
- parser-emitted TIR is the only structural authority
- all TIR identities are module-local
- `Template` carries only its TIR reference and source location
- `TirViewIdentity` contains every effective semantic dimension
- expression overlays require one lookup, not an external stack
- structural and nested-value transitions are explicit and shared
- one preparation owner decides foldable, runtime or helper
- one fold path consumes prepared constants
- runtime handoff consumes the same prepared view
- missing authority is always an internal error
- runtime dependence is never represented as an implementation fallback
- no TIR identity reaches HIR or a backend
- production and test code are materially smaller than `069a29acb`
- output, diagnostics, formatting, wrappers, slots, control flow and reactivity remain unchanged
- the mandatory post-TIR roadmap review checkpoint is complete before canonical module implementation begins
- validation and recorded benchmark gates pass

## Non-goals

- broad `$md` redesign
- new template language features
- HIR template nodes
- backend-specific render plans
- TIR persistence across modules or incremental builds
- parallel AST template parsing
- speculative multi-store support
- broad `StringTable` or source-text redesign
- benchmark-driven semantic changes
