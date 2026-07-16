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

## Current state

```text
ACTIVE_PLAN: docs/roadmap/plans/final-tir-completion-plan.md
STATUS: active
CURRENT_PHASE: R0 - lock the single-store ownership boundary
LAST_ACCEPTED_COMMIT: c1ecc2c58
IMPLEMENTATION_BASE_COMMIT: 069a29acb
BRANCH: main
```

Use `069a29acb` as the implementation and regression base. Do not continue extending `FoldAuthorityWalk`, foreign-store traversal, external expression-overlay stacks or prepared foreign-wrapper proofs.

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

### Phase R1 - Collapse ownership and references

#### R1A - Require the shared store handle

- Make production `ScopeContext` construction require the module TIR handle.
- Remove default scratch registry/store allocation from `ScopeContext::new`.
- Update constant, type, trait, function-signature and body-emission contexts to pass the same handle.
- Keep test-only isolated store construction in local test helpers, not production constructors.

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

### Phase R2 - Make `TirView` complete

#### R2A - Replace overlay sets with value contexts

- Add copyable `TemplateViewContext` containing the three optional overlay IDs.
- Convert overlay IDs to `NonZeroU32`-backed newtypes so optional dimensions remain compact.
- Replace `TemplateOverlaySetId` fields on durable, child and wrapper references.
- Delete overlay-set storage, canonicalization and composition.
- Make the empty context `TemplateViewContext::default()`.

#### R2B - Build complete expression overlays

- Reuse the existing expression-site normalization collector.
- Merge structural descendant overrides into each effective root overlay once.
- Preserve outer-context precedence for reused sites.
- Assert that all expression overlay entries refer to valid module-global `ExpressionSiteId`s.
- Keep structural fallback for sites with no override.

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

### Phase R5 - Consolidate remaining consumers and tests

#### R5A - Remaining consumers

- Move reactive metadata traversal onto `TirView` transitions.
- Use prepared facts or the same view methods for const-required control-flow validation.
- Use exact finalized views for final type and debug validation.
- Remove raw-store authority helpers and registry-era comments.
- Keep distinct reducer matches where metadata, folding and handoff genuinely produce different results.

#### R5B - Remove duplicate durable state

- Remove `Template.kind` and synchronization methods.
- Read `TemplateIr.kind` through the shared store.
- Keep mutable kind only in parser-local state before the `TemplateIr` entry is created.

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
just bench-report
```

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
