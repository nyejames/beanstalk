# Beanstalk Final TIR Completion Plan

## Purpose

Complete the Template IR (TIR) migration so template semantics have one AST-local authority, the durable `Template` value is a narrow handle, and no production or test path reconstructs template meaning from `TemplateContent`, `TemplateAtom`, old render plans, or migration-only fallback state.

Final architecture:

```text
template syntax
-> parser-local construction state
-> module-local AST TIR registry
-> TirView / TirSubtreeView composition, formatting, folding, metadata, and finalization
-> folded StringSlice expressions or neutral owned runtime handoff payloads
-> HIR
```

Completion means one authoritative TIR path from parsing through AST finalization, with no TIR identities crossing into HIR or backends.

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/final-tir-completion-plan.md`
STATUS: active
CURRENT_SLICE: Phase 1B family 3 - rewrite HIR lowering fixtures through direct registry-qualified TIR and remove their finalized-content bridge calls
LAST_ACCEPTED_COMMIT: `5b24c8013` (`test: build doc fragments from TIR`)
BRANCH: `main`
WORKTREE: Phase 1B family 5 accepted candidate on `main` at `5b24c8013` in `/Users/aneirinjames/projects/beanstalk/beanstalk`
REQUIRED_RELOADS: startup files, this plan, `docs/language-overview.md`, `docs/src/docs/templates/#page.bst`, and the current source/diff
RELEVANT_CONTEXT_NOW:
- Production parsing, composition, formatting, folding, classification, reactive metadata, const handling, and runtime handoff are TIR-backed.
- Detached content reconstruction is test-only. Reactive metadata, wrapper stress, static-fragment, doc-fragment and option-capture head tests no longer read or mutate it. The main remaining fixture owners are parser TIR tests, HIR lowering tests, control-flow body helpers, AST normalization tests and the compatibility builder in `tir/finalize_sync.rs`.
- The durable `Template` still duplicates TIR-owned state through `control_flow`, `style`, `child_wrappers`, optional TIR identity, and a redundant `TemplateTirReference::is_composed` flag.
- HIR consumes owned runtime handoffs. Its remaining raw-`Template` entry is an invariant-error shim, not a real lowering path.
ACCEPTANCE_CRITERIA:
- remove one connected compatibility-fixture family per slice; retain only distinct behavior or final TIR invariants
- delete the test-only content bridge before thinning the durable `Template`
- make TIR views and store-qualified references the only semantic read path
- remove duplicate state, silent fallbacks, migration terminology, and classification-only deep clones
- preserve parser diagnostics, source locations, markdown/formatter behavior, slots, wrappers, control flow, const folding, reactive metadata, fragment ordering, and the AST/HIR boundary
VALIDATION_STATE:
- Phase 1A passed `cargo run --quiet -- build docs --release`: 72 files.
- Current `main` last passed full `just validate` at the completed hash-root checkpoint `cf36d5945`: cross-target Clippy, 3358 unit tests, 1756 integration cases, docs check, and `bench-check` 28/28.
- Phase 1B family 7 focused validation passed: `cargo test --quiet reactive_template_metadata -- --format terse`, 17 passed.
- Phase 1B family 7 passed full `just validate`: cross-target Clippy, 3358 unit tests, 1756 integration cases, docs check, and `bench-check` 28/28 with a 0 ms average delta, 2 faster and 1 slower.
- Phase 1B family 8 focused validation passed: `cargo test --quiet docs_style_data_wrapper -- --format terse`, 1 passed, and `cargo test --quiet wrapper_tests -- --format terse`, 11 passed.
- Phase 1B family 8 passed full `just validate`: cross-target Clippy, 3358 unit tests, 1756 integration cases, docs check, and `bench-check` 28/28 with a 0 ms average delta, 2 faster and 1 slower.
- Phase 1B family 9 focused validation passed: `cargo test --quiet create_template_node -- --format terse`, 298 passed.
- Phase 1B family 9 passed full `just validate`: cross-target Clippy, 3358 unit tests, 1756 integration cases, docs check, and `bench-check` 28/28 with a 0 ms average delta, 2 faster and 1 slower.
- Phase 1B family 6 focused validation passed: `cargo test --quiet doc_fragment -- --format terse`, 5 passed, and `cargo test --quiet 'template_tests::' -- --format terse`, 10 passed.
- Phase 1B family 6 passed full `just validate`: cross-target Clippy, 3357 unit tests, 1756 integration cases, docs check, and `bench-check` 28/28 with a 0 ms average delta, 2 faster and 1 slower.
- Phase 1B family 5 focused validation passed: `cargo test --quiet const_required_template_option_capture -- --format terse`, 5 passed, and `cargo test --quiet head_tests -- --format terse`, 91 passed.
- Phase 1B family 5 passed full `just validate`: cross-target Clippy, 3357 unit tests, 1756 integration cases, docs check, and `bench-check` 28/28 with a 0 ms average delta, 2 faster and 1 slower.
- Re-run the required gate after every new TIR code slice.
DOCS_IMPACT: progress matrix unchanged for representation-only slices; Phase 5 owns final docs and deferred-performance handoff
BLOCKERS_OR_OPEN_DECISIONS:
- Remaining old authority is test-only, but its caller graph must be removed in bounded owner-based slices.
- `Template.kind` and `TemplateTirReference::store_owner` may remain only if a final audit proves they carry distinct, non-derivable semantics.
DELEGATION_DECISION: family 5 passed final Ollama review and parent validation; use an Ollama implementation worker for family 3 after committing it, with Codex CLI only after a clean Ollama availability blocker
NEXT_WORKER_ORDER: Ollama, Codex CLI after a clean blocker, then parent-direct
STOP_REASON: none
NEXT_RESUME_ACTION: commit family 5, then delegate family 3 to Ollama

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
- `Template.content`, `TemplateContent`, `TemplateAtom`, detached materialization, and related converter/parity helpers are now test-only.
- Old parity suites have already been reduced substantially. Remaining fixtures must be judged against final builder/view/fold/validation coverage rather than preserved mechanically.

Git history is the detailed evidence. This plan records only the final architecture and remaining work.

## Binding design decisions

### Authority and stage boundary

- TIR is AST-local and is the only structural authority after parser emission.
- `TirView` / `TirSubtreeView` are the production read APIs for effective roots and overlays.
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

- [ ] Compare its assertions with current parser-builder, `TirView`, fold, validation, and integration coverage.
- [ ] Delete representation/layout assertions that are not semantic invariants.
- [ ] Rebuild only genuinely unique internal invariants with direct TIR builders and registry-qualified refs.
- [ ] Prefer an existing integration case when output or diagnostics can prove the behavior.
- [ ] Remove one-caller helpers with the family.
- [ ] Run the family’s focused suite, template/TIR tests, and `just validate`.

Repeat Slice 1B until only the bridge owner remains.

Completed Phase 1B families:

- Reactive metadata tests now protect formatted-root discovery, Parsed-phase gating and expression-overlay resolution directly through TIR. Obsolete stale-content setup and imports were removed.
- The docs-style wrapper stress test now measures a nonzero, bounded same-store TIR node count at Composed-or-later phase. Its vacuous compatibility-content walker was deleted.
- Static control-flow body text tests now traverse nested template child references through same-store TIR. Their detached `TemplateAtom` walker was deleted.
- Doc-fragment folding now uses a directly constructed formatted TIR fixture. The redundant stale-content precedence test was deleted.
- Const-required option-capture fixtures now build their branch-chain roots directly in the module TIR store. Their one-caller content materializer and finalizer helpers were deleted.

#### Slice 1C — Delete the bridge owner

- [ ] Delete `Template.content`.
- [ ] Delete `TemplateContent`, `TemplateAtom`, and test-only `TemplateSegment`.
- [ ] Delete `finalized_template_tir_id`.
- [ ] Delete content-to-TIR builders and materialization-only enums/contexts from `tir/finalize_sync.rs`.
- [ ] Delete test-only re-exports and imports in `tir/mod.rs`.
- [ ] Remove obsolete counters, comments, and `#[cfg(test)]` branches that existed only for detached content.
- [ ] Rename or dissolve `finalize_sync.rs` so its remaining production owner is explicit.
- [ ] Confirm no test constructs old template authority for convenience.

#### Phase 1 acceptance

- [ ] No `TemplateContent`, `TemplateAtom`, finalized-content bridge, or detached materializer remains anywhere under `src/compiler_frontend`.
- [ ] Tests assert final behavior or final TIR invariants.
- [ ] `just validate` passes.
- [ ] `just bench-check` is unchanged or improved; no compatibility path is restored for timing.

---

### Phase 2 — Thin `Template` and remove duplicate semantic state

#### Goal

Separate parser-local mutable state from the durable handle and make TIR the sole owner of style, control-flow structure, wrapper context, and phase.

#### Slice 2A — Introduce explicit parser-local state

- [ ] Reuse `TemplateConstructionContext` as the TIR/store/registry/location owner.
- [ ] Add one small parser-local `TemplateBuildState` (or equivalent) for `kind`, `style`, direct-child wrapper refs, foldability, and control-flow parse metadata.
- [ ] Audit the parse-time `can_fold` boolean against final effective-TIR classification. Delete it if classification already owns the complete decision; otherwise keep it parser-local and document the exact non-derivable fact it carries.
- [ ] Change head/body parser requests to receive build state instead of `&mut Template`.
- [ ] Remove `Template::empty()` as the parser accumulator.
- [ ] Make construction finish return a non-optional `TemplateTirReference`; missing builder output is an internal invariant.
- [ ] Construct the durable `Template` only after the authoritative reference exists.

#### Slice 2B — Remove duplicate control-flow objects

TIR `BranchChain`, `Loop`, and `LoopControl` nodes already own selectors/headers/body roots.

- [ ] Make render-unit preparation operate on parser TIR node IDs/body refs and parse-local scratch directly.
- [ ] Replace `TemplateControlFlowBodyScratch` with the smallest explicit parser result needed by preparation, or delete it if the authoritative control-flow node/body refs already provide the same information.
- [ ] Remove `Template.control_flow` and the take/restore borrow workaround in `prepare_control_flow_render_units`.
- [ ] Delete duplicate durable structs:
  - [ ] `TemplateControlFlow`
  - [ ] `TemplateBranchChain`
  - [ ] `TemplateConditionalBranch`
  - [ ] `TemplateFallbackBranch`
  - [ ] `TemplateLoopControlFlow`
- [ ] Replace `TemplateControlFlowTirReference` with `TemplateTirBodyReference` or direct TIR node/view identity; do not keep a wrapper that only forwards methods.
- [ ] Keep shared semantic selector/header/loop-control types only where parser, TIR, fold, and HIR handoff genuinely share them.
- [ ] Replace `template_contains_control_flow` dual checks with TIR summary/view classification only.
- [ ] Remove body “sync/refreshed/previous-ref” fallback logic. Prepared body roots are required results.
- [ ] Delete `suppress_formatter_summary_on_finish`; preparation must return/install an explicit formatted root and phase rather than mutating a builder-side lifecycle flag.

#### Slice 2C — Remove style and wrapper duplication

- [ ] Keep effective style on `TemplateIr`; keep mutable parse-time style on `TemplateBuildState`.
- [ ] Remove `Template.style`, `apply_style`, and `apply_style_updates`.
- [ ] Normalize `$children(..)` arguments at the directive boundary and carry refs through parse-local state.
- [ ] Attach final wrapper context through the existing wrapper-set/overlay owner.
- [ ] Remove `Template.child_wrappers`.
- [ ] Ensure folding, formatting, classification, and handoff read style/wrappers from the effective TIR view, never from the durable handle.

#### Slice 2D — Simplify final references and classification markers

- [ ] Make `Template.tir_reference` non-optional.
- [ ] Delete `TemplateTirReference::is_composed`; use `phase.is_at_least(Composed)`.
- [ ] Delete `TemplateIrSummary::has_formatter` if it is only pending-lifecycle state; derive pending formatting from effective style + reference phase.
- [ ] Replace optional/missing-reference branches with explicit construction invariants.
- [ ] Audit `store_owner` after detached tests are gone:
  - [ ] retain it only if production can otherwise resolve the wrong registry/store instance
  - [ ] otherwise remove it and use registry-qualified identity consistently
- [ ] Audit `Template.kind`:
  - [ ] move helper markers into TIR if all callers already hold a view
  - [ ] otherwise keep it as a documented cached marker with one write owner and a validation check against `TemplateIr.kind`
- [ ] Audit `Template.id`; remove it if it is debug-only and derivable from existing identity.
- [ ] Remove obsolete convenience methods such as `tir_template_id`, `tir_root_node_id`, or `tir_store_owner` when direct reference access is clearer.
- [ ] Audit the repeated registry + store handle + store-ID triple. Consolidate it only if one registered-store context removes identity duplication and debug assertions without forcing every parser write through registry lookup.

#### Slice 2E — Remove the HIR raw-template shim

- [ ] Delete HIR’s `Template` import and `lower_runtime_template_expression(&Template, ...)` invariant shim.
- [ ] Keep raw `ExpressionKind::Template` rejection at the AST/HIR normalization boundary or in the HIR dispatcher without a Template-specific lowering API.
- [ ] Update stale “cutover” and “legacy” comments on runtime handoff variants.
- [ ] Confirm HIR lowers owned runtime handoffs only.

#### Phase 2 acceptance

- [ ] Durable `Template` has no content, control-flow, style, wrapper, or parser-state fields.
- [ ] TIR phase is the only composed/formatted/finalized status.
- [ ] HIR imports no `Template` type.
- [ ] Parser diagnostics and all template integration outputs are unchanged.
- [ ] `just validate` and `just bench-check` pass.

---

### Phase 3 — Consolidate final TIR owners and remove migration noise

#### Goal

Use existing final systems consistently, delete duplicate walkers/state, and make remaining hot paths explicit without starting a broad formatter rewrite.

#### Slice 3A — One classification/read path

- [ ] Make `TirView` / `TirSubtreeView` the production classification input.
- [ ] Rename `MaterializedTirTemplateClassification` to `TirTemplateClassification`.
- [ ] Replace `classify_materialized_current_tir_template` and other “current/materialized/fresh” entry points with one effective-view classifier.
- [ ] Keep raw store recursion private to the view/classification owner only where required.
- [ ] Reuse the existing registry-aware expression-payload walker for nested template/expression inspection; delete ad hoc recursion in head parsing, finalization, and helper filters when semantics match.
- [ ] Preserve exact root + phase + overlay cycle identity.

#### Slice 3B — Make render-unit and overlay failures explicit

- [ ] Rename `try_sync_*`, `body_sync_*`, `refreshed_*`, and similar migration names to final `prepare_*`/`prepared_*` terminology.
- [ ] Convert required `Option` returns and `.ok()?` chains to `Result`.
- [ ] Do not fall back to a previous body root after a preparation error.
- [ ] Move wrapper-context overlay collection out of `create_template_node.rs` into the existing TIR wrapper/overlay owner.
- [ ] Make that traversal registry-aware and reuse existing wrapper-set canonicalization.
- [ ] Propagate missing-node, missing-store, and overlay-compose failures as internal errors; do not silently return or ignore `Err`.
- [ ] Remove local recursive walkers that duplicate `tir/slot_composition`, `tir/render_unit`, or `TirView`.

#### Slice 3C — Consolidate TIR summary construction

- [ ] Audit duplicated manual updates to `TemplateIrSummary` in parser builder, subtree copy/materialization, derived wrapper construction, and summary walking.
- [ ] Introduce one narrow summary accumulator in `tir/summary.rs` only where update semantics are identical.
- [ ] Split runtime slot-site cursor state from summary accumulation if they have different callers.
- [ ] Rename `CurrentStateMaterializationSummary` and `record_materialization_counters` to final TIR construction/copy terminology, or delete them if the shared accumulator replaces them.
- [ ] Preserve existing text-byte, node-count, depth, slot, control-flow, and reactivity facts. Keep formatter presence as style data; do not preserve a redundant formatter-pending lifecycle bit.

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
rg "TemplateIrRegistry|TirView|TirSubtreeView|TemplateRef|TemplateNodeRef|TemplateOverlaySet|TemplateIrStore" src/compiler_frontend/hir src/backends
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
