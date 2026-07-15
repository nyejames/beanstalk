# Beanstalk Final TIR Completion Plan

## Purpose

Complete the Template IR (TIR) migration so template semantics have one AST-local authority, the durable `Template` value is a narrow handle, and no production or test path reconstructs template meaning from `TemplateContent`, `TemplateAtom`, old render plans, or migration-only fallback state.

Final architecture:

```text
template syntax
-> parser-local construction state
-> module-local AST TIR registry
-> TirView composition, formatting, folding, metadata, and finalization
-> folded StringSlice expressions or neutral owned runtime handoff payloads
-> HIR
```

Completion means one authoritative TIR path from parsing through AST finalization, with no TIR identities crossing into HIR or backends.

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/final-tir-completion-plan.md`
STATUS: active
CURRENT_SLICE: Slice 3D - bounded clone/allocation audit
LAST_ACCEPTED_COMMIT: `1ca82fefb` (`test: protect parsed TIR child folding`, prior checkpoint; Slice 3C is accepted in this plan-bearing commit)
BRANCH: `main`
WORKTREE: `main`, Slice 3C accepted and fully validated, no unrelated changes
REQUIRED_RELOADS: startup files, this plan, relevant template/language references and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: compiler AST template/TIR contract, focused template language references, testing and validation standards
- code: `tir/formatter_view.rs`, `tir/wrapper_sets.rs`, final TIR walkers and focused formatter tests
- Slice 3C consolidated identical summary updates on `TemplateIrSummary`, separated runtime slot cursor state and made derived TIR templates summarize their final nodes through the summary owner.
ACCEPTANCE_CRITERIA:
- Remove classification-only whole-node or node-kind clones in `tir/formatter_view.rs` and adjacent final walkers.
- Borrow effective nodes only long enough to derive cheap facts, then retain only required IDs and payloads.
- Add transient formatter-run input state only if it removes verified duplicate reads without becoming another render plan.
- Keep `$md` grammar and representation unchanged.
- Retain only changes with neutral or improved `just bench-check` evidence.
VALIDATION_STATE:
- Slice 3C focused TIR suite: passed, 432 tests
- Slice 3C final `just validate`: passed cross-target Clippy, 3416 unit tests, 1764 integration cases, docs checking and `bench-check` 28/28 with a 2 ms average improvement, 12 faster and 0 slower
DOCS_IMPACT: progress matrix unchanged for this representation-only slice. Phase 5 owns final docs and deferred-performance handoff
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: undecided - inspect exact 3D clone/read sites before launching the first implementation provider
NEXT_WORKER_ORDER: ollama, codex-cli, parent-direct
STOP_REASON: none
NEXT_RESUME_ACTION: reload the plan and inspect `tir/formatter_view.rs` for Slice 3D clone and repeated-read sites

SELF_AUDIT_NOTE: parser-owned text, head values, nested templates, slots, inserts, control flow, wrappers, formatting, and runtime handoff already have TIR owners. The remaining work is deletion, state thinning, final API consolidation, targeted low-risk efficiency cleanup, test ownership, documentation, and closure.

## Plan lifecycle and execution rules

This is a temporary implementation checklist.

- Implement one bounded slice at a time and keep the compiler valid after each slice.
- Split work by final owner, never by “old path versus new path”.
- Update only the `Current state` block, the active checkboxes, and one concise accepted-slice note when necessary. Do not rebuild a chronological progress log.
- Before editing, confirm `main`, inspect `git status`, and preserve unrelated user-owned changes.
- Read every edited owner and its focused tests before changing an API.
- Do not add compatibility wrappers, optional fallback parameters, duplicate structs, or broad utility modules.
- User-visible behavior belongs in integration cases. Focused unit tests protect only hidden final invariants.
- Code-bearing slices run `cargo fmt`, focused tests, and `just validate`. Run `just bench-check` when formatter, fold, traversal, allocation, or hot construction paths change.
- A strictly documentation-only slice follows the documentation release-build gate from `style-guide/validation.bd`; do not claim `just validate` unless it was actually run.
- Delete this plan after Phase 6 completes.

## Completed migration summary

The following work is closed and must not be re-planned:

- Parser head/body paths emit text, expressions, children, slots, inserts, loop control, and control-flow roots directly into the module TIR store.
- Registry-qualified refs, phases, overlays, cross-store child resolution, wrapper sets, slot composition, folding, formatter views, reactive traversal, classification, and owned runtime handoff exist.
- Production formatting and render-unit preparation no longer read `TemplateContent` or `TemplateAtom`.
- Production const evaluation, finalization, doc fragments, helper filtering, and HIR handoff use registry-backed effective TIR.
- Recursive wrapper `Template` storage, aggregate-wrapper source mirrors, content-to-TIR production conversion, current-state scratch-store classifiers, and legacy fold/handoff fallbacks were removed.
- Runtime handoff is neutral AST-owned data; HIR and backends do not receive TIR stores, refs, views, overlays, or registries.
- `Template.content`, detached content types, detached materialization and related converter/parity helpers are removed from production and tests.
- Remaining fixtures use parser-emitted or directly constructed registry-qualified TIR and protect final behavior or TIR invariants.

Git history is the detailed evidence. This plan records only the final architecture and remaining work.

## Binding design decisions

### Authority and stage boundary

- TIR is AST-local and is the only structural authority after parser emission.
- `TirView` is the production read API for effective roots and overlays.
- HIR receives only folded strings or neutral owned runtime handoff payloads.
- Missing required TIR authority is an internal compiler error, not permission to reconstruct from content or silently keep an older root.

### Phase and reference identity

```text
Parsed -> Composed -> Formatted -> Finalized
```

- Folding requires `Composed` or later.
- HIR handoff requires `Finalized`.
- Effective identity is store-qualified root + phase + overlay-set ID, resolved through the owning module registry.
- Remove `TemplateTirReference::is_composed`; `phase >= Composed` is the single source of that fact.
- Lifecycle state belongs to the reference phase, not to shape summaries. Remove formatter-pending booleans such as `TemplateIrSummary::has_formatter` and `suppress_formatter_summary_on_finish` unless an audit proves a distinct structural fact that cannot be derived from style + phase.
- Do not restamp foreign overlay IDs or treat store-local IDs as globally meaningful.
- Keep the store-owner proof only if the post-fixture audit proves registry identity alone cannot prevent wrong-store reuse in production.

### Slots and wrapper context

The accepted wrapper model remains:

```text
wrapper template effective identity
-> store-qualified root + phase + overlay-set ID
-> canonical wrapper-set entry
-> wrapper-context overlay keyed by ChildTemplateOccurrenceId
-> TirView resolves effective application
```

- Wrapper order is exact and outermost/innermost behavior must remain unchanged.
- `$fresh` suppresses only the immediate parent’s wrappers.
- Use `TirWrapperApplicationMode::Always` for ordinary children and `IfChildEmits` for structurally conditional children.
- Slot-bearing wrappers route child output as fill through slot-resolution overlays.
- Slotless wrappers preserve prepend/wrap semantics.
- Missing slots render empty; repeated slots replay the same routed contribution.
- False/no-else branches and zero-iteration loops produce structural no-output, so `IfChildEmits` wrappers do not render.
- Eager cross-store copying is not the primary model. Copy-on-write/subtree copying is allowed only for a derived local tree, fresh occurrence IDs, or owned handoff materialization.

### Final durable `Template`

`Template` must stop acting as both mutable parser state and durable AST value.

The target durable shape is:

```text
Template {
    tir_reference: TemplateTirReference,   // non-optional after construction
    kind: TemplateType,                    // only if a final audit proves it cannot be derived cheaply
    id: String,                            // only if still used outside diagnostics/debugging
    location: SourceLocation,
}
```

Delete durable copies of:

- `content`
- `control_flow`
- `style`
- `child_wrappers`
- parser builder/finalization state

Use the existing `TemplateConstructionContext` plus a small parser-local build-state record for mutable head/body metadata. Do not add another long-lived template representation.

### Diagnostics and failure behavior

- User syntax/rule failures remain `CompilerDiagnostic`.
- Missing stores, roots, overlays, body IDs, or impossible phase transitions are `CompilerError`.
- Replace `Option`/`.ok()?` fallback flows with `Result` when the state is required by the final architecture.
- Do not silently ignore overlay composition failures or missing TIR nodes.

## Scope boundary for performance work

This branch may include only cheap, behavior-preserving cleanup that naturally belongs to final TIR ownership:

- remove whole-node/kind clones used only for discriminant checks or formatter-run classification
- remove repeated effective-node reads when one narrow transient snapshot is measurably clearer and cheaper
- preserve existing byte-length/output-size summary hooks

Do not perform the broad `$md` parser/renderer rewrite during TIR closure. Phase 5 transfers that work, plus horizontal-rule support, into an explicit post-TIR roadmap item.

## Remaining implementation

### Phase 1 — Delete the test-only content bridge

#### Goal

Remove all detached-content reconstruction and representation-shaped tests without weakening behavior coverage.

#### Slice 1A - Inventory and classify

- [x] Run focused greps for:
  - [x] `TemplateContent`
  - [x] `TemplateAtom`
  - [x] `TemplateSegment` construction
  - [x] `finalized_template_tir_id`
  - [x] `build_finalized_tir_root_from_content`
  - [x] `build_finalized_tir_root_with_control_flow`
  - [x] `TemplateTirSyncMissReason`
  - [x] `ChildMaterializationContext`
  - [x] `classify_materialized_current_tir_template` in tests
- [x] Group every hit by final owner and distinct invariant:
  - [x] parser/create-template fixtures
  - [x] view/classification/remap fixtures
  - [x] slot/wrapper/control-flow fixtures
  - [x] folding/finalization fixtures
  - [x] HIR handoff fixtures
- [x] For each family, record one decision in the working notes: delete as redundant, rewrite through direct TIR construction, or move the unique assertion to the final owner.
- [x] Do not create a shared replacement fixture merely to preserve old test ergonomics.

Phase 1A inventory decisions:

- Rewrite parser TIR, AST normalization, HIR lowering, head validation and doc-fragment fixtures through direct registry-qualified TIR construction.
- Rewrite or remove their shared control-flow body helper only after its parser, HIR and head-validation callers migrate.
- Delete reactive metadata stale-content assertions. Keep their direct TIR subscription-discovery coverage.
- Delete the vacuous wrapper content-node counter. Replace it with a TIR invariant only if the owning test lacks effective structural coverage.
- Replace the static-fragment helper's content walk with an effective TIR view or folded-output assertion.
- Delete the test-only materializer in `tir/finalize_sync.rs` and the detached content types after every caller family has migrated.
- Process independent families first. The reactive metadata family is smallest, followed by wrapper counting, static fragments and doc fragments. The shared control-flow helper and bridge owner remain terminal.

#### Slice 1B — Remove one connected family per commit

For the selected family:

- [x] Compare its assertions with current parser-builder, `TirView`, fold, validation, and integration coverage.
- [x] Delete representation/layout assertions that are not semantic invariants.
- [x] Rebuild only genuinely unique internal invariants with direct TIR builders and registry-qualified refs.
- [x] Prefer an existing integration case when output or diagnostics can prove the behavior.
- [x] Remove one-caller helpers with the family.
- [x] Run the family’s focused suite, template/TIR tests, and `just validate`.

Repeat Slice 1B until only the bridge owner remains.

Completed Phase 1B families:

- Reactive metadata tests now protect formatted-root discovery, Parsed-phase gating and expression-overlay resolution directly through TIR. Obsolete stale-content setup and imports were removed.
- The docs-style wrapper stress test now measures a nonzero, bounded same-store TIR node count at Composed-or-later phase. Its vacuous compatibility-content walker was deleted.
- Static control-flow body text tests now traverse nested template child references through same-store TIR. Their detached `TemplateAtom` walker was deleted.
- Doc-fragment folding now uses a directly constructed formatted TIR fixture. The redundant stale-content precedence test was deleted.
- Const-required option-capture fixtures now build their branch-chain roots directly in the module TIR store. Their one-caller content materializer and finalizer helpers were deleted.
- Runtime-template HIR expression and Float tests now construct neutral owned handoff payloads directly. Their detached-content/TIR materializer fixture layer and two newly dead shared materializers were deleted.
- Raw HIR module-constant invariant fixtures now contain only the malformed raw Template shape required by the rejection path. Irrelevant literal content setup was deleted.
- AST normalization tests now construct registered TIR directly. Detached-content precedence cases and their finalized-content bridge helpers were deleted.
- Parser-created stale-content parity tests were deleted. Retained parser tests now name final TIR phase, root reuse, formatter output and root-shape invariants.
- Manual body-dynamic and reactive formatter fixtures now construct direct registered TIR and assert preserved reactive payloads. Obsolete nested-template conversion fixtures were deleted.
- Parser `$doc` and empty named-insert formatter fixtures now use real source parsing. Impossible explicit-formatter slot-definition coverage, tautological note/todo coverage and their detached-content attachment helper were deleted.
- Parser tests no longer call `finalized_template_tir_id` or assert detached-content absence. Bridge-only template-ID reuse fixtures were deleted while final phase, output and root-shape owners remain.
- Create-template tests no longer assert that obsolete detached content is empty. Their final TIR head, body and control-flow assertions remain the behavior owners.
- Const-eval, type-resolution, field-member and expression-dispatch registry fixtures now construct store-qualified slot templates without stale runtime content. Their distinct foreign-store and module-registry classification assertions remain.
- Const-required head and template-folding fixtures now use direct store-qualified TIR without detached payload content. Same-store active-borrow, foreign-store registry and no-substitution borrowing remain distinct test owners.
- The obsolete control-flow body-ref helper was deleted after proving it only walked detached content and never installed TIR. Its parser-created callers already own authoritative body roots.

#### Slice 1C — Delete the bridge owner

- [x] Delete `Template.content`.
- [x] Delete `TemplateContent`, `TemplateAtom`, and test-only `TemplateSegment`.
- [x] Delete `finalized_template_tir_id`.
- [x] Delete content-to-TIR builders and materialization-only enums/contexts from `tir/finalize_sync.rs`.
- [x] Delete test-only re-exports and imports in `tir/mod.rs`.
- [x] Remove obsolete counters, comments, and `#[cfg(test)]` branches that existed only for detached content.
- [x] Rename or dissolve `finalize_sync.rs` so its remaining production owner is explicit.
- [x] Confirm no test constructs old template authority for convenience.

#### Phase 1 acceptance

- [x] No `TemplateContent`, `TemplateAtom`, finalized-content bridge, or detached materializer remains anywhere under `src/compiler_frontend`.
- [x] Tests assert final behavior or final TIR invariants.
- [x] `just validate` passes.
- [x] `just bench-check` is unchanged or improved; no compatibility path is restored for timing.

---

### Phase 2 — Thin `Template` and remove duplicate semantic state

#### Goal

Separate parser-local mutable state from the durable handle and make TIR the sole owner of style, control-flow structure, wrapper context, and phase.

#### Slice 2A — Introduce explicit parser-local state

- [x] Reuse `TemplateConstructionContext` as the TIR/store/registry/location owner.
- [x] Add one small parser-local `TemplateBuildState` (or equivalent) for `kind`, `style`, direct-child wrapper refs, foldability, and control-flow parse metadata.
- [x] Audit the parse-time `can_fold` boolean against final effective-TIR classification. Delete it if classification already owns the complete decision; otherwise keep it parser-local and document the exact non-derivable fact it carries.
- [x] Change head/body parser requests to receive build state instead of `&mut Template`.
- [x] Remove `Template::empty()` as the parser accumulator.
- [x] Make construction finish return a non-optional `TemplateTirReference`; missing builder output is an internal invariant.
- [x] Construct the durable `Template` only after the authoritative reference exists.

#### Slice 2B — Remove duplicate control-flow objects

TIR `BranchChain`, `Loop`, and `LoopControl` nodes already own selectors/headers/body roots.

- [x] Make render-unit preparation operate on parser TIR node IDs/body refs and parse-local scratch directly.
- [x] Replace `TemplateControlFlowBodyScratch` with the smallest explicit parser result needed by preparation, or delete it if the authoritative control-flow node/body refs already provide the same information.
- [x] Remove `Template.control_flow` and the take/restore borrow workaround in `prepare_control_flow_render_units`.
- [x] Delete duplicate durable structs:
  - [x] `TemplateControlFlow`
  - [x] `TemplateBranchChain`
  - [x] `TemplateConditionalBranch`
  - [x] `TemplateFallbackBranch`
  - [x] `TemplateLoopControlFlow`
- [x] Replace `TemplateControlFlowTirReference` with `TemplateTirBodyReference` or direct TIR node/view identity; do not keep a wrapper that only forwards methods.
- [x] Keep shared semantic selector/header/loop-control types only where parser, TIR, fold, and HIR handoff genuinely share them.
- [x] Replace `template_contains_control_flow` dual checks with TIR summary/view classification only.
- [x] Remove body “sync/refreshed/previous-ref” fallback logic. Prepared body roots are required results.
- [x] Delete `suppress_formatter_summary_on_finish`; preparation must return/install an explicit formatted root and phase rather than mutating a builder-side lifecycle flag.

Phase 2B2 checkpoint: normalization, reactive annotation and owned runtime handoff now read selectors, headers, bodies and aggregate wrappers from one root `TirView`. One root expression overlay preserves earlier effective root and same-store child expressions. `TirSubtreeView`, per-body overlay storage and node-level handoff materialization are deleted.

Phase 2B3 checkpoint: control-flow structure now exists only in TIR. The obsolete body-reference layer and unreachable AST/TIR remap graph are deleted, with real build-boundary remapping retained at headers, diagnostics, parsed types, `TypeEnvironment` and HIR.

#### Slice 2C — Remove style and wrapper duplication

- [x] Keep effective style on `TemplateIr`; keep mutable parse-time style on `TemplateBuildState`.
- [x] Remove `Template.style`, `apply_style`, and `apply_style_updates`.
- [x] Normalize `$children(..)` arguments at the directive boundary and carry refs through parse-local state.
- [x] Attach final wrapper context through the existing wrapper-set/overlay owner.
- [x] Remove `Template.child_wrappers`.
- [x] Ensure folding, formatting, classification, and handoff read style/wrappers from the effective TIR view, never from the durable handle.

Phase 2C checkpoint: mutable style and wrapper references now end with parser-local build state. Durable templates carry neither, effective reads use TIR views and wrapper application remains registry-owned through canonical wrapper sets and context overlays.

#### Slice 2D — Simplify final references and classification markers

- [x] Make `Template.tir_reference` non-optional.
- [x] Delete `TemplateTirReference::is_composed`; use `phase.is_at_least(Composed)`.
- [x] Delete `TemplateIrSummary::has_formatter` if it is only pending-lifecycle state; derive pending formatting from effective style + reference phase.
- [x] Replace optional/missing-reference branches with explicit construction invariants.
- [x] Audit `store_owner` after detached tests are gone:
  - [x] retain it because registry-local store IDs can collide and direct-store consumers cannot always re-borrow the registry
  - [x] remove the obsolete detached-snapshot helper/test and keep one focused cross-registry collision invariant
- [x] Audit `Template.kind`:
  - [x] confirm it cannot move fully into TIR because foreign parser/head and store-less coercion boundaries do not always hold the originating registry
  - [x] keep it as a documented cached marker with one synchronization owner and focused consistency checks against `TemplateIr.kind`
- [x] Audit `Template.id`; remove it if it is debug-only and derivable from existing identity.
- [x] Remove obsolete convenience methods such as `tir_template_id`, `tir_root_node_id`, or `tir_store_owner` when direct reference access is clearer.
- [x] Audit the repeated registry + store handle + store-ID triple. Consolidate it only if one registered-store context removes identity duplication and debug assertions without forcing every parser write through registry lookup.

Phase 2D1a checkpoint: durable templates always carry authoritative TIR identity. Missing-reference production branches, empty handles and detached no-authority fixtures are deleted, while phase, registry and store-mismatch outcomes remain explicit.

Phase 2D1b1 checkpoint: `TemplateTirPhase` is the sole composed-lifecycle authority. Head-chain and wrapper-overlay composition advance Parsed roots without downgrading later phases, formatted-root installation stays Formatted and slot-insert diagnostics use the same Composed-or-later contract.

Phase 2D1b2 checkpoint: formatter lifecycle is derived from effective style plus reference phase, not shape summary. Child-template formatting uses the child's exact phase and overlay. Bare local insert IDs rely on the verified invariant that formatter-bearing helpers are formatted before recording; default-style foreign proxies preserve the existing whitespace path.

Phase 2D2 checkpoint: the logical store-origin token remains required. Registry-local store IDs can collide across module registries, while several production consumers hold a direct store borrow and must reject a foreign local ID without re-borrowing the registry. The unused detached-snapshot helper/test are deleted and one two-registry collision test owns the invariant.

Phase 2D3 checkpoint: `Template.kind` remains a narrow boundary cache because proven foreign parser/head paths can lack the originating registry. `TemplateIr.kind` is authoritative wherever a store, registry or view exists, one synchronization method updates TIR before the cache and focused tests protect construction consistency plus cross-registry identity.

Phase 2D4 checkpoint: the generated parser `Template.id` had no production reader, so durable and parser-local ID state, its head-parser assignment, all fixture fields and the now-unused `BS_VAR_PREFIX` constant are deleted without replacement. Store-qualified TIR identity remains the only template identity.

Phase 2D5 checkpoint: trivial `Template` forwarding methods for the TIR template ID and store-owner token are deleted. Callers now read the authoritative reference fields directly, owner comparisons avoid an unnecessary `Arc` clone and store/kind accessors that enforce real boundaries remain.

Phase 2D6 checkpoint: `RegisteredTemplateIrStore` now couples the registry, registry-level store ID and exact direct store handle for all four production carriers. Checked existing-store construction rejects missing IDs and same-ID foreign handles, parser writes still borrow the direct handle and the repeated debug pointer assertion is deleted.

#### Slice 2E — Remove the HIR raw-template shim

- [x] Delete HIR's `Template` import and `lower_runtime_template_expression(&Template, ...)` invariant shim.
- [x] Keep raw `ExpressionKind::Template` rejection at the AST/HIR normalization boundary or in the HIR dispatcher without a Template-specific lowering API.
- [x] Update stale "cutover" and "legacy" comments on runtime handoff variants.
- [x] Confirm HIR lowers owned runtime handoffs only.

Phase 2E checkpoint: raw templates now fail directly at the HIR expression dispatcher. HIR imports no `Template` type and runtime lowering accepts only neutral owned template or slot-application handoffs. A narrow malformed-expression fixture preserves dispatcher and module-constant invariant coverage without placing TIR construction under HIR.

#### Phase 2 acceptance

- [x] Durable `Template` has no content, control-flow, style, wrapper, or parser-state fields.
- [x] TIR phase is the only composed/formatted/finalized status.
- [x] HIR imports no `Template` type.
- [x] Parser diagnostics and all template integration outputs are unchanged.
- [x] `just validate` and `just bench-check` pass.

---

### Phase 3 — Consolidate final TIR owners and remove migration noise

#### Goal

Use existing final systems consistently, delete duplicate walkers/state, and make remaining hot paths explicit without starting a broad formatter rewrite.

#### Slice 3A — One classification/read path

- [x] Make `TirView` the production classification input.
- [x] Rename `MaterializedTirTemplateClassification` to `TirTemplateClassification`.
- [x] Replace `classify_materialized_current_tir_template` and other “current/materialized/fresh” entry points with one effective-view classifier.
- [x] Keep raw store recursion private to the view/classification owner only where required.
- [x] Reuse the existing registry-aware expression-payload walker for nested template/expression inspection; delete ad hoc recursion in head parsing, finalization, and helper filters when semantics match.
- [x] Preserve exact root + phase + overlay cycle identity.

Phase 3A1 checkpoint: full-template classification now has one effective-view entry and one neutral result type. Create-template classification carries the authoritative reference identity, effective slot policy distinguishes resolved sources from uncovered slots, standalone structural predicates retain their narrow owners and expression overlays have a focused Finalized-phase invariant test.

Phase 3A2 checkpoint: nested expression and effective-view predicate traversal now has one TIR owner and one exact root, phase and overlay visited set. Template-head runtime-slot detection delegates to it with conservative failures, while normalization's site-keyed collection and reactive annotation's environment-aware traversal remain separate because they carry distinct state and mutation policy.

#### Slice 3B — Make render-unit and overlay failures explicit

- [x] Rename `try_sync_*`, `body_sync_*`, `refreshed_*`, and similar migration names to final `prepare_*`/`prepared_*` terminology.
- [ ] Convert required `Option` returns and `.ok()?` chains to `Result`.
- [x] Do not fall back to a previous body root after a preparation error.
- [x] Move wrapper-context overlay collection out of `create_template_node.rs` into the existing TIR wrapper/overlay owner.
- [x] Make that traversal registry-aware and reuse existing wrapper-set canonicalization.
- [x] Propagate missing-node, missing-store, and overlay-compose failures as internal errors; do not silently return or ignore `Err`.
- [ ] Remove local recursive walkers that duplicate `tir/slot_composition`, `tir/render_unit`, or `TirView`.

Phase 3B1 checkpoint: wrapper-context construction now belongs to the TIR wrapper-set owner. The pass resolves same-store and foreign child metadata without re-entering the current store, validates exact root and overlay authority before allocation, reuses one canonical inherited wrapper set and reports missing authority or composition failures instead of silently skipping them.

Phase 3B2 checkpoint: linear formatter installation rejects wrong-store authority, and runtime control-flow artifact validation now has one required registry-backed effective-view path. Nested children retain exact root, phase and overlay identity, malformed stores/templates/nodes/overlays fail explicitly, and the raw same-store validator plus redundant pre-pass are deleted.

Phase 3B3 checkpoint: final type and debug TypeId validation require one Finalized effective view with exact direct-store and registry-store ownership. Insert contributions preserve inherited phase and overlay identity, while the tri-state authority attempt, raw same-store expression walker and its adapters are deleted.

Phase 3B4 checkpoint: const-required control-flow validation propagates missing effective authority through the compiler-error diagnostic lane. Runtime and const traversal share an exact root, phase and overlay active-cycle key, and recursive overlay coverage protects distinct effective identities.

Phase 3B5 checkpoint: AST normalization now has one required Finalized effective-view HIR handoff path. Ordinary runtime handoffs and missing template/store authority are required results, exact registered-store ownership rejects matching local IDs from foreign registries and only genuine runtime slot-plan absence remains optional.

Phase 3B6 checkpoint: view-backed fold-context handoff materialization requires a registry and validates registry-view ownership. The folded-child text shortcut propagates malformed overlay-set authority failures for `Composed`-or-later children while preserving the `Parsed`-phase shortcut-unavailable fallthrough for both same-store and cross-store child references. The `materialize_folded_child_text` handoff path preserves genuine shortcut-unavailable states as structural runtime handoff fallback. A pre-existing `module_inception` Clippy blocker from the package naming migration was fixed by renaming `builder_surface::builder_surface` to `builder_surface::definition`.

#### Slice 3C — Consolidate TIR summary construction

- [x] Audit duplicated manual updates to `TemplateIrSummary` in parser builder, subtree copy/materialization, derived wrapper construction, and summary walking.
- [x] Introduce one narrow summary accumulator in `tir/summary.rs` only where update semantics are identical.
- [x] Split runtime slot-site cursor state from summary accumulation if they have different callers.
- [x] Rename `CurrentStateMaterializationSummary` and `record_materialization_counters` to final TIR construction/copy terminology, or delete them if the shared accumulator replaces them.
- [x] Preserve existing text-byte, node-count, depth, slot, control-flow, and reactivity facts. Keep formatter presence as style data; do not preserve a redundant formatter-pending lifecycle bit.

Phase 3C checkpoint: identical incremental updates now belong to `TemplateIrSummary`, while `TirCopyState` composes summary facts, depth and a separate runtime slot-site cursor for recursive copy passes. Derived render-unit, fill and conditional-wrapper templates summarize their final TIR nodes through the existing summary walker, preserving side-table reactivity and nested body facts without a parallel counter path.

#### Slice 3D — Bounded clone/allocation audit

- [ ] In `tir/formatter_view.rs`, remove whole `TemplateIrNode` / `TemplateIrNodeKind` clones used only for eligibility, discriminant checks, or anchor classification.
- [ ] Derive cheap facts while the effective node is borrowed; snapshot only IDs, locations, subscriptions, and anchor kind needed after the borrow ends.
- [ ] Apply the same rule to wrapper-context collection and other final TIR walkers.
- [ ] Measure repeated effective-node reads in formatter-run preparation.
- [ ] Add a transient `FormatterRunInput` (or equivalent) only if it removes duplicate reads without becoming a second render plan.
- [ ] Do not change `$md` grammar, per-character atom representation, link/code parsing, or list rendering in this slice.
- [ ] Run `just bench-check` and retain only neutral/improved changes.

#### Slice 3E — Final module/API ownership

- [ ] Re-evaluate final owners after deletion:
  - [ ] `tir/finalize_sync.rs`
  - [ ] `tir/construction.rs`
  - [ ] `template_render_units.rs`
  - [ ] `template_folding.rs`
  - [ ] `template_control_flow/**`
  - [ ] `template_slots/**`
  - [ ] `template.rs` / `template_types.rs`
- [ ] Delete files whose remaining contents are test-only or forwarding-only.
- [ ] Rename files whose names describe migration rather than final responsibility.
- [ ] Keep a thin facade only where it marks a real AST substage boundary.
- [ ] Move surviving neutral vocabulary into the narrowest coherent owner; do not over-split into one-type files.
- [ ] Replace redundant long argument lists with one named context only when the same values travel together across several functions.
- [ ] Remove stale `#[allow(dead_code)]`, “legacy”, “mirror”, “current-state”, “sync”, and future-cutover comments.
- [ ] Update `templates/mod.rs` and `tir/mod.rs` to describe the final module map.

#### Phase 3 acceptance

- [ ] `TirView` is the single production effective-read path.
- [ ] No silent preparation fallback or ignored overlay error remains.
- [ ] No classification-only deep clone remains in the formatter adapter.
- [ ] Module names and comments describe final ownership.
- [ ] `just validate` and `just bench-check` pass.

---

### Phase 4 — Final behavior and invariant test ownership

#### Goal

Finish with a smaller test suite that protects observable behavior and real TIR invariants, not deleted representation.

#### Tasks

- [ ] Confirm one primary integration owner for:
  - [ ] `$md` formatting
  - [ ] Beandown implicit `$md`
  - [ ] child-template opacity and dynamic anchors
  - [ ] default/named/positional slots and missing-slot empty output
  - [ ] `$insert(...)` routing and diagnostics
  - [ ] repeated slot replay
  - [ ] `$children(...)`, `$fresh`, wrapper ordering, and no leakage
  - [ ] template `if` / `loop`, no-output, and output before break/continue
  - [ ] runtime slot applications inside control flow
  - [ ] reactive subscriptions
  - [ ] top-level const/runtime page fragments
  - [ ] malformed template diagnostics
- [ ] Retain focused TIR tests only for hidden facts: store/phase/overlay identity, cycle keys, wrapper-set reuse/equivalence, occurrence IDs, and malformed-store invariants.
- [ ] Remove duplicate parity tests, broad shared test helpers, and exact incidental sequence/vector layout assertions.
- [ ] Move tests with renamed owners.
- [ ] Confirm integration output and diagnostic codes did not change.

#### Phase 4 acceptance

- [ ] Tests have no old-authority terminology or fixtures.
- [ ] Behavior coverage is at least as strong as before deletion.
- [ ] `just validate` passes.
- [ ] `just bench-check` passes if test/fixture changes touch benchmark inputs; otherwise do not record benchmark history.

---

### Phase 5 — Final documentation and post-TIR roadmap handoff

#### Goal

Describe only the final TIR system and transfer non-closure feature/performance work into explicit follow-ups.

#### Final architecture docs

- [ ] Re-verify `docs/compiler-design-overview.md`, `templates/mod.rs`, and `tir/mod.rs`:
  - [ ] TIR is AST-local and registry-owned.
  - [ ] `Template` matches its actual thin final shape.
  - [ ] classification, formatting, folding, wrappers, slots, and handoff read effective TIR.
  - [ ] HIR receives owned handoff data only.
  - [ ] no migration/fallback wording remains.
- [ ] Keep the progress matrix behavior-focused; do not add rows for internal micro-optimizations.
- [ ] Close/delete the stale sibling `template-optimisation-and-tir-implementation-plan.md` and remove stale roadmap references to it.

#### Post-TIR `$md` feature and performance follow-up

Record a dedicated roadmap item or new plan with the following accepted scope. Do not implement it during TIR closure.

##### Horizontal-rule behavior

- Three or more contiguous ASCII `-` characters on their own logical line.
- Leading/trailing spaces and tabs are allowed; a terminal `\r` from CRLF is ignored. Internal whitespace is not allowed.
- Any non-whitespace non-dash atom or opaque anchor rejects the rule candidate.
- `- - -` remains list/plain syntax, not a rule.
- Emit exactly `<hr>`.
- A rule is an immediate paragraph and list boundary; no surrounding blank line is required.
- Near misses remain literal content with no diagnostic.
- Do not add Setext headings, `***`, `___`, Unicode dashes, or general CommonMark behavior.
- Primary behavior coverage: standalone and indented rules, longer runs, paragraph/list boundaries, near misses, opaque anchors, inline-code literals, explicit `$md`, runtime/const paths, and Beandown implicit `$md`.

##### `$md` allocation/algorithm follow-up

Profile before broad rewrites, then evaluate:

- borrowed heading/list/link/inline-code parse spans instead of owned vectors/strings
- slice-returning trim helpers
- direct rendering into one `MarkdownOutputBuilder`
- streaming list rendering without retained rendered-item vectors
- UTF-8 iterator/byte-index whitespace normalization instead of temporary `Vec<char>`
- resolved formatter-text transport that avoids intermediate `StringTable` intern/resolve cycles
- coalesced UTF-8 text spans instead of one `MarkdownInlineAtom::Char` per scalar
- a single-pass or cursor-based inline parser only if malformed-candidate profiling justifies it

Keep source-span-backed TIR text and `$md`’s internal text-span representation as separate design questions.

##### Other deferred template performance work

Retain the existing post-TIR roadmap scope without implementing it here:

- source-span-backed body text instead of eager source-text interning
- per-template parse and formatter-output caches
- dev-mode source-hash keyed TIR reuse and dependency-aware invalidation
- incremental module/template compilation after module-boundary incremental infrastructure exists
- profiling-backed parallel nested-template folding
- backend-neutral runtime string-build lowering (`StringBuild` / `StringAppend`)
- generated scaling cases for wrapper depth, slot replay, directive mix, control flow, reactivity, interpolation chunk count, and custom `$md` volume

##### Benchmark follow-up

Add a dedicated `$md` workload distinct from plain `.md` asset rendering:

- long paragraphs and many short lines
- headings and nested mixed lists
- valid links and inline-code spans
- malformed link/backtick candidates
- child-template and dynamic-expression anchors
- Unicode-heavy content
- horizontal rules and near misses after the feature lands

Do not treat `pulldown-cmark` `.md` asset cases as evidence for the custom `$md` formatter.

#### Phase 5 acceptance

- [ ] Docs describe final code, not intended-but-unlanded migration state.
- [ ] Roadmap contains the explicit `$md` follow-up and other deferred template performance items.
- [ ] Horizontal rules remain outside this TIR closure.
- [ ] Run the documentation-only release-build gate if this phase changes docs only; otherwise run `just validate`.

---

### Phase 6 — Final closure

#### Hard grep gates

Production and test hits must be zero unless explicitly justified as final vocabulary.

```bash
rg "TemplateContent|TemplateAtom|build_finalized_tir_root_from_content|finalized_template_tir_id|TemplateTirSyncMissReason|ChildMaterializationContext" src/compiler_frontend
rg "current_state|CurrentStateMaterialization|classify_materialized_current_tir|fresh_tir_root|mirror_skipped|ContentMirror|FormattedTir" src/compiler_frontend/ast/templates
rg "is_composed|control_flow: Option<TemplateControlFlow>|child_wrappers: Vec<TemplateWrapperReference>|pub style: Style" src/compiler_frontend/ast/templates
rg "legacy|fallback path|compatibility mirror|content mirror|current-state|try_sync_|body_sync" src/compiler_frontend/ast/templates
rg "TemplateRenderPlan|RenderPiece|render_plan" src/compiler_frontend/ast/templates
rg "template_types::Template" src/compiler_frontend/hir src/backends
rg "TemplateIrRegistry|TirView|TemplateRef|TemplateNodeRef|TemplateOverlaySet|TemplateIrStore" src/compiler_frontend/hir src/backends
```

Allowed final hits:

- neutral names such as `TemplateType`, `TemplateFormatter`, `TemplateTirReference`, `TemplateSegmentOrigin`, and owned runtime handoff types
- documentation that explicitly describes deferred follow-up work
- a narrowly justified store-owner field if Phase 2 proves it remains required

#### Validation and evidence

- [ ] Run `cargo fmt`.
- [ ] Run all hard grep gates and inspect every allowed hit.
- [ ] Run `just validate`.
- [ ] Run five independent recorded `just bench` invocations.
- [ ] Run `just bench-report`.
- [ ] Record a concise benchmark summary under `benchmarks/summaries/`.
- [ ] Include `check docs`, template stress/churn, Beandown or custom `$md` coverage where present, and slot/directive-heavy cases.
- [ ] If Phase 3 changed `tir/formatter_view.rs`, also run one fixed targeted `$md`/Beandown workload. Do not change the tracked comparison suite midway through the before/after series.
- [ ] Confirm no production old authority, TIR leak, raw HIR template path, or duplicate semantic state remains.
- [ ] Confirm user-visible output and diagnostics are behavior-preserving.
- [ ] Confirm docs and roadmap match the final system.
- [ ] Delete this plan and close/delete the stale sibling plan.

#### Final acceptance

Accept closure only when:

- [ ] all hard gates pass
- [ ] `just validate` passes
- [ ] benchmark evidence is recorded
- [ ] the durable `Template` is thin and has non-optional authoritative TIR identity
- [ ] TIR views are the sole effective semantic read path
- [ ] HIR receives owned handoff payloads only
- [ ] tests protect behavior/final invariants without compatibility scaffolding
- [ ] docs describe final TIR only

If timing regresses materially, do not restore old paths. Attribute the regression with counters/profiling and open a separate performance plan.

## Current source anchors

Review these before the relevant phase; do not assume names survive Phase 3:

```text
src/compiler_frontend/ast/templates/
├── create_template_node.rs
├── template.rs
├── template_types.rs
├── template_render_units.rs
├── template_control_flow/
├── template_slots/
├── formatter_contract.rs
├── runtime_handoff.rs
├── styles/markdown/
└── tir/
    ├── mod.rs
    ├── refs.rs
    ├── registry.rs
    ├── overlays.rs
    ├── view.rs
    ├── parser_builder_state.rs
    ├── construction_context.rs
    ├── classification.rs
    ├── formatter_view.rs
    ├── render_unit.rs
    ├── finalize_sync.rs
    ├── construction.rs
    ├── handoff_materialization.rs
    ├── slot_composition/
    └── tests/

src/compiler_frontend/hir/hir_expression/templates/
```
