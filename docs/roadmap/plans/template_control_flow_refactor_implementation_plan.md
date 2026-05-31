# Template Control-Flow Runtime Slot Stabilization Refactor Plan

Target repo state: `nyejames/beanstalk` at commit `c182c0fd81ea187f0d7c1f5892769f8f1422fe19`.

## Purpose

This plan turns the completed template control-flow audit into an implementation sequence for stabilizing runtime template slot behavior, reducing indirection, and cleaning up stale post-plan artifacts.

The current architecture is the right direction:

- AST owns template composition, slot routing, helper elimination, const folding, runtime slot application plans, and runtime render-plan preparation.
- HIR consumes finalized runtime template plans and lowers them into ordinary CFG, locals, string accumulators, and appends.
- Template control flow remains limited to `if`, `else if`, `else`, `loop`, `break`, and `continue`.
- Template match/case syntax is not part of the language surface.

This refactor should not reintroduce template pattern matching or runtime directive dispatch. It should make the current design more precise, easier to maintain, and better tested.

---

## Current repo anchors

### Documentation and matrix

- `docs/language-overview.md`
  - User-facing template slot and template control-flow semantics.
- `docs/compiler-design-overview.md`
  - AST/HIR ownership boundary.
- `docs/src/docs/progress/#page.bst`
  - Language surface matrix and coverage summary.
- `docs/roadmap/roadmap.md`
  - Roadmap notes for completed template control-flow/runtime-slot work and future surfaces.
- `docs/codebase-style-guide.md`
  - Required implementation standards and validation workflow.

### AST template model and parsing

- `src/compiler_frontend/ast/templates/mod.rs`
  - Template module map and compilation-flow docs.
- `src/compiler_frontend/ast/templates/template_types.rs`
  - `Template`, runtime slot application field, control-flow fields, metadata lifecycle.
- `src/compiler_frontend/ast/templates/template_body_parser.rs`
  - Template body parsing, branch-chain body splitting, loop body parsing, loop-control marker construction.
- `src/compiler_frontend/ast/templates/template_body_sentinels.rs`
  - `[else]`, `[else if ...]`, `[break]`, `[continue]` classification, boundary trimming, diagnostics.
- `src/compiler_frontend/ast/templates/template_control_flow/`
  - Structured control-flow types, validation, const-eval helpers, folding/remap support.
- `src/compiler_frontend/ast/templates/template_render_units.rs`
  - Control-flow branch/body render-plan preparation and aggregate wrapper plans.
- `src/compiler_frontend/ast/templates/template_render_plan.rs`
  - Render-plan pieces, formatter anchor model, `RenderPiece::Slot`.

### Slot composition and runtime plans

- `src/compiler_frontend/ast/templates/template_slots/mod.rs`
  - Slot subsystem map and exports.
- `src/compiler_frontend/ast/templates/template_slots/schema.rs`
  - `SlotSchema`, ordered slot key iteration, target validation.
- `src/compiler_frontend/ast/templates/template_slots/contributions.rs`
  - `$insert(...)` extraction and loose contribution grouping.
- `src/compiler_frontend/ast/templates/template_slots/composition.rs`
  - Shared routing, compile-time placeholder expansion, `$children(...)` wrapper application.
- `src/compiler_frontend/ast/templates/template_slots/runtime_plan.rs`
  - AST-owned runtime slot application plans.
- `src/compiler_frontend/ast/templates/template_composition.rs`
  - `$children(...)` direct child wrapping and head-chain composition.

### HIR runtime template lowering

- `src/compiler_frontend/hir/hir_expression/templates/mod.rs`
  - Runtime template lowering entry point.
- `src/compiler_frontend/hir/hir_expression/templates/render_append.rs`
  - Shared runtime append path, append context, slot accumulator lookup.
- `src/compiler_frontend/hir/hir_expression/templates/slot_application.rs`
  - Runtime slot application expression lowering.
- `src/compiler_frontend/hir/hir_expression/templates/control_flow.rs`
  - Runtime template branch/loop CFG lowering.
- `src/compiler_frontend/hir/hir_expression/templates/loop_aggregate.rs`
  - Aggregate wrapper emission after real output.

### Diagnostics and keywords

- `src/compiler_frontend/keywords.rs`
  - Keyword mapping and reserved keyword shadows.
- `src/compiler_frontend/tokenizer/tokens.rs`
  - Token surface. `case` should remain absent.
- `src/compiler_frontend/compiler_messages/diagnostic_payload/types.rs`
  - `InvalidTemplateStructureReason`, slot diagnostics, control-flow diagnostics.
- Diagnostic renderers under `src/compiler_frontend/compiler_messages/`.

### Tests

- `tests/cases/manifest.toml`
  - Template integration cases and diagnostics cases.
- Template cases around:
  - `template_runtime_branch_slot_application`
  - `template_runtime_loop_slot_application`
  - `template_runtime_slot_children_fresh_control_flow`
  - `template_runtime_control_flow_slot_rejected`
  - `template_runtime_control_flow_insert_rejected`
  - `template_match_style_if_rejected`
  - `template_match_style_else_if_rejected`
  - `template_loop_*`
  - `template_else_*`

---

## Cross-phase invariants

These must hold through every phase.

### Language surface

- [ ] Do not reintroduce `case` as a keyword, token, template sentinel, deferred feature, or documented template concept.
- [ ] Template branching remains limited to:
  - Bool `if`
  - option-present capture `if maybe is |value|`
  - `[else if ...]`
  - `[else]`
- [ ] Ordinary statement/value pattern matching remains unchanged outside templates.
- [ ] Ordinary statement `else if` remains unsupported.
- [ ] `$slot`, `$insert`, `$children`, and `$fresh` remain static AST-owned template semantics, not runtime directives.

### Stage ownership

- [ ] AST owns slot schema extraction, target validation, loose contribution routing, helper elimination, runtime slot plan construction, and user-facing diagnostics.
- [ ] HIR does not parse directives, validate slot schemas, or emit user-facing source diagnostics.
- [ ] HIR only lowers finalized runtime template plans into normal CFG, locals, accumulators, and appends.
- [ ] User mistakes use `CompilerDiagnostic`; HIR-only failures use `CompilerError` / HIR transformation errors.

### Runtime behavior

- [ ] Runtime branch bodies remain lazy.
- [ ] Runtime loop sources evaluate at the same points documented in `docs/language-overview.md`.
- [ ] Runtime slot contributions remain lazy when inside inactive control-flow branches.
- [ ] Structural no-output remains distinct from output of an empty string.
- [ ] Missing slot contributions render as empty.
- [ ] Repeated slots replay the same contributed output.
- [ ] `[break]` / `[continue]` inside runtime slot applications propagate to the nearest active template loop when the slot application is lowered as part of template output appending.

### Code quality

- [ ] Prefer one current API shape over compatibility wrappers.
- [ ] Use context structs or clear plan structs instead of adding long parameter lists.
- [ ] Keep module docs current after moving or splitting code.
- [ ] Avoid broad boolean APIs when an enum communicates the state.
- [ ] Delete stale comments, old test fixture names, unused diagnostic reasons, and obsolete scaffolding.

---

# Phase 0 — Baseline audit and safety net

## Context

Before changing runtime slot lowering, establish the exact current state and prevent accidental regressions. This phase should not change behavior except for test/doc fixture organization where strictly needed. It exists so later phases can be reviewed as targeted corrections rather than broad churn.

## Tasks

### Repo audit

- [ ] Search for remaining template match/case references:
  - [ ] `case` as a template sentinel.
  - [ ] `TemplateCase` / `TemplateMatch` naming.
  - [ ] `TemplateMatchStyleControlFlowRemoved` and render text for that diagnostic.
  - [ ] Any docs saying template match/case is deferred or removed.
- [ ] Verify `case` remains absent from:
  - [ ] `TokenKind`
  - [ ] `keyword_token_kind`
  - [ ] `RESERVED_KEYWORD_SHADOWS`
- [ ] Record any stale test fixture names that preserve old wording.

### Focused baseline validation

- [ ] Run focused template tests before code changes:
  - [ ] `cargo test compiler_frontend::ast::templates::`
  - [ ] `cargo test runtime_template_control_flow`
  - [ ] `cargo run --quiet -- tests --backend html --filter template`
- [ ] Run broad validation before the refactor branch:
  - [ ] `cargo fmt --check`
  - [ ] `cargo check`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `just validate`

### Initial test planning

- [ ] Identify whether existing runtime slot tests assert output strongly enough.
- [ ] Identify duplicate fixtures that can be merged after new coverage lands.
- [ ] Plan fixture names that describe behavior, not implementation internals.

## Phase-end audit / style review / validation

- [ ] Confirm no production behavior changed.
- [ ] Confirm any fixture/doc naming notes are written into this plan or issue notes.
- [ ] Check the touched docs/tests against `docs/codebase-style-guide.md` testing guidance.
- [ ] Run `git diff --check`.
- [ ] Run `just validate` if any files changed.

---

# Phase 1 — Runtime slot applications inside loop append contexts

## Context

Runtime slot applications are currently lowered as value-producing template expressions. That path rejects `Break` / `Continue` as HIR invariants. This is correct for standalone value expression lowering, but not for runtime slot applications encountered while appending a template loop body.

The documented template surface allows `[break]` and `[continue]` inside nested template `if` / `else if` bodies in template loops. Runtime slot applications are now part of template control-flow output, so the append path must be able to propagate loop-control emissions when a runtime slot application appears inside an active loop body.

This phase adds an append-mode runtime slot application lowering path while preserving the existing value-producing path for ordinary template expression contexts.

## Design

Add a shared lowering helper that appends a `RuntimeSlotApplicationPlan` into an existing `RuntimeTemplateAppendContext`:

```rust
append_runtime_slot_application_with_context(
    plan: &RuntimeSlotApplicationPlan,
    append_context: RuntimeTemplateAppendContext<'_>,
    location: &SourceLocation,
) -> Result<TemplateBodyEmission, CompilerError>
```

Behavior:

- Allocate slot accumulators.
- Append routed slot contributions into their accumulators using the same append context semantics.
- Append the wrapper plan into `append_context.target_accumulator()` using slot accumulator lookup.
- Return `TemplateBodyEmission` so `Break` / `Continue` can propagate when the slot app is lowered from a template append context.
- Keep `lower_runtime_slot_application_template_expression` as the expression entry point, but implement it by calling the append-mode helper and rejecting escaped `Break` / `Continue` there.

## Implementation steps

### HIR runtime slot lowering

- [ ] In `src/compiler_frontend/hir/hir_expression/templates/slot_application.rs`, add an append-mode entry point:
  - [ ] Accept `RuntimeTemplateAppendContext<'_>`.
  - [ ] Return `TemplateBodyEmission`.
  - [ ] Reuse `initialize_runtime_slot_accumulators`.
  - [ ] Reuse contribution appending.
  - [ ] Reuse wrapper appending with slot accumulators.
- [ ] Refactor `lower_runtime_slot_application_template_expression` to:
  - [ ] Create the output accumulator.
  - [ ] Build a `RuntimeTemplateAppendContext` for that accumulator.
  - [ ] Call the append-mode helper.
  - [ ] Reject `Break` / `Continue` only at this expression boundary.
  - [ ] Return a `Copy` of the output accumulator.

### Render append dispatch

- [ ] In `src/compiler_frontend/hir/hir_expression/templates/render_append.rs`, update `append_render_piece_to_accumulator`:
  - [ ] In `RenderPiece::DynamicExpression`, if the expression is a template with `runtime_slot_application`, call append-mode slot lowering before ordinary expression lowering.
  - [ ] In `RenderPiece::ChildTemplate`, do the same.
  - [ ] Keep the `template.control_flow.is_some()` path intact.
  - [ ] Ensure runtime slot application detection happens before falling back to `lower_expression_value_to_current_block`.
- [ ] Preserve expression-level `!` propagation behavior for non-template dynamic expressions.

### Loop-control behavior

- [ ] Allow `TemplateBodyEmission::Break` / `Continue` from append-mode slot applications to propagate exactly like nested control-flow templates.
- [ ] Ensure appending stops after a terminated block.
- [ ] Verify `output-before-break` and `output-before-continue` still mark structural output correctly.

## Tests

Add or update integration cases:

- [ ] `template_runtime_slot_break_inside_loop`
  - Runtime slot application inside a template loop body emits `[break]` from a selected branch.
  - Assert output before break is preserved and later iterations stop.
- [ ] `template_runtime_slot_continue_inside_loop`
  - Runtime slot application emits `[continue]` before output.
  - Assert that iteration does not count as emitted output if nothing appeared before `continue`.
- [ ] `template_runtime_slot_output_then_continue_inside_loop`
  - Assert output before `continue` is preserved.
- [ ] `template_runtime_slot_loop_control_outside_loop_rejected`
  - Same runtime slot helper shape used outside any template loop should still produce a structured diagnostic or HIR invariant only if AST failed to reject it.
  - Prefer AST diagnostic coverage when possible.
- [ ] Add unit coverage if HIR runtime-template lowering already has focused tests for loop control.

## Documentation updates

- [ ] Update `docs/language-overview.md` only if the current wording does not make clear that runtime slot applications inside template loops obey the same `[break]` / `[continue]` semantics.
- [ ] Update `docs/src/docs/progress/#page.bst` coverage text if new tests materially improve runtime slot loop-control coverage.

## Phase-end audit / style review / validation

- [ ] Review `slot_application.rs` for one clear append-mode/value-mode split.
- [ ] Review `render_append.rs` for match arm readability and no repeated template-detection logic.
- [ ] Check that no user-facing diagnostics were added in HIR.
- [ ] Confirm comments explain WHAT/WHY for the append-mode helper.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo check`
  - [ ] `cargo test runtime_template_control_flow`
  - [ ] `cargo test compiler_frontend::ast::templates::`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_runtime_slot`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_loop`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 2 — Simplify runtime slot contribution payloads

## Context

`RuntimeSlotContributionContent` currently distinguishes static content from runtime render plans. HIR converts static content back into a render plan before appending. That adds an avoidable branch and keeps AST/HIR handoff less uniform than it needs to be.

Since both static and runtime contributions lower through render-plan appending, store contribution content as a `TemplateRenderPlan` during AST runtime-plan construction. This makes the next slot-site refactor easier because every contribution/source can be handled through one append path.

## Design

Replace:

```rust
RuntimeSlotContributionContent::Static(TemplateContent)
RuntimeSlotContributionContent::Runtime(TemplateRenderPlan)
```

with a single render-plan payload:

```rust
pub(crate) struct RuntimeSlotContribution {
    pub(crate) target: SlotKey,
    pub(crate) render_plan: TemplateRenderPlan,
    pub(crate) location: SourceLocation,
}
```

or, if Phase 3 will immediately introduce source IDs, use:

```rust
pub(crate) struct RuntimeSlotContributionSource {
    pub(crate) id: RuntimeSlotContributionSourceId,
    pub(crate) target: SlotKey,
    pub(crate) render_plan: TemplateRenderPlan,
    pub(crate) location: SourceLocation,
}
```

## Implementation steps

### AST runtime plan cleanup

- [ ] In `src/compiler_frontend/ast/templates/template_slots/runtime_plan.rs`:
  - [ ] Remove `RuntimeSlotContributionContent`.
  - [ ] Remove `classify_contribution_content` or reduce it to `build_contribution_render_plan`.
  - [ ] Build `TemplateRenderPlan` in AST for every non-empty contribution.
  - [ ] Update remap support to remap the render plan directly.
- [ ] In `template_slots/mod.rs`:
  - [ ] Remove exports for `RuntimeSlotContributionContent`.
  - [ ] Keep only plan/source structs that are needed outside the slot module.

### HIR cleanup

- [ ] In `slot_application.rs`:
  - [ ] Remove the static/runtime match.
  - [ ] Append `contribution.render_plan` directly.
  - [ ] Preserve `Break` / `Continue` propagation behavior from Phase 1.

### Docs/comments cleanup

- [ ] Update file-level docs in `runtime_plan.rs` to mention a uniform render-plan payload.
- [ ] Ensure comments say why AST prepares the render plan instead of making HIR rebuild it.

## Tests

- [ ] Existing runtime slot tests should pass unchanged.
- [ ] Add focused unit tests in the template slot test module if runtime-plan construction has test-only exports:
  - [ ] static contribution becomes a render plan.
  - [ ] runtime contribution becomes a render plan.
  - [ ] remap touches contribution render plans.

## Phase-end audit / style review / validation

- [ ] Confirm no HIR code rebuilds `TemplateRenderPlan` from static runtime slot content.
- [ ] Confirm AST/HIR handoff struct names still describe their job.
- [ ] Confirm no compatibility enum or wrapper remains.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo check`
  - [ ] `cargo test compiler_frontend::ast::templates::`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_runtime_slot`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 3 — Runtime slot site semantics

## Context

Runtime slot plans currently route contributions by `SlotKey` and find one placeholder for each key when applying `$children(...)` / `$fresh` wrapper behavior. This can drift from compile-time composition because compile-time composition expands every slot placeholder occurrence independently, and each `SlotPlaceholder` carries local wrapper metadata.

Repeated slots must replay the same contributed output. At the same time, each slot occurrence may have different local placeholder metadata. The runtime model should therefore separate:

1. contribution evaluation, which should happen once per authored contribution/source, and
2. slot-site rendering, which may apply site-local wrapper behavior before appending at each placeholder occurrence.

## Design

Move runtime slot planning to a two-level model.

### Source level

A source is evaluated once.

```rust
pub(crate) struct RuntimeSlotContributionSource {
    pub(crate) id: RuntimeSlotContributionSourceId,
    pub(crate) target: SlotKey,
    pub(crate) render_plan: TemplateRenderPlan,
    pub(crate) location: SourceLocation,
}
```

HIR lowers each source into a source accumulator once, preserving single-evaluation semantics for repeated slots.

### Site level

A site represents one slot placeholder occurrence in the wrapper tree.

```rust
pub(crate) struct RuntimeSlotSitePlan {
    pub(crate) id: RuntimeSlotSiteId,
    pub(crate) key: SlotKey,
    pub(crate) render_plan: RuntimeSlotSiteRenderPlan,
    pub(crate) location: SourceLocation,
}
```

The site render plan should be able to refer to source accumulators without re-lowering source expressions. Model it similarly to the loop aggregate plan:

```rust
pub(crate) enum RuntimeSlotSitePiece {
    Render(Box<RenderPiece>),
    ContributionSource(RuntimeSlotContributionSourceId),
}
```

The wrapper plan should refer to slot sites by ID. Prefer an explicit render piece over occurrence-index state:

```rust
RenderPiece::RuntimeSlotSite(RuntimeSlotSiteId)
```

or a narrowly scoped equivalent if the render-plan module owner prefers a separate wrapper-plan type. Avoid mutable “next occurrence for key” lookup during HIR lowering unless it is clearly simpler and documented, because that relies on traversal order staying synchronized across AST and HIR.

## Implementation steps

### AST data model

- [ ] In `runtime_plan.rs`, add:
  - [ ] `RuntimeSlotContributionSourceId`
  - [ ] `RuntimeSlotSiteId`
  - [ ] `RuntimeSlotContributionSource`
  - [ ] `RuntimeSlotSitePlan`
  - [ ] `RuntimeSlotSiteRenderPlan`
  - [ ] `RuntimeSlotSitePiece`
- [ ] Update `RuntimeSlotApplicationPlan` to carry:
  - [ ] wrapper plan with resolved slot-site references.
  - [ ] ordered contribution sources.
  - [ ] ordered slot site plans.
  - [ ] schema only if still needed for diagnostics/invariants; otherwise keep schema private to planning.

### Wrapper plan construction

- [ ] Replace key-only `slot_placeholder_for_key` logic.
- [ ] Walk wrapper content recursively in source order.
- [ ] For every `TemplateAtom::Slot(placeholder)`:
  - [ ] allocate a `RuntimeSlotSiteId`.
  - [ ] create a site plan from that exact placeholder metadata.
  - [ ] replace the slot render piece in the runtime wrapper plan with the site reference.
- [ ] Preserve nested template traversal behavior already used by schema discovery.
- [ ] Ensure repeated slots create distinct sites that can apply distinct local wrapper metadata.

### Source plan construction

- [ ] Build contribution sources once per routed contribution chunk.
- [ ] Keep deterministic order:
  - [ ] default slot
  - [ ] positional slots ascending
  - [ ] named slots ordered by resolved spelling
  - [ ] source order inside each target
- [ ] Preserve current loose contribution grouping semantics from `contributions.rs`.
- [ ] Preserve target validation and unknown-slot diagnostics in AST.

### Site render-plan construction

- [ ] Build site plans by expanding the exact placeholder metadata against source references, not by cloning source expressions.
- [ ] Ensure `$children(...)` wrappers apply to direct source references as child contributions without descending into grandchildren.
- [ ] Ensure `$fresh` on a source contribution suppresses only the immediate parent wrapper rules it already suppresses at compile time.
- [ ] Add comments explaining why site plans exist separately from source plans.

### HIR lowering

- [ ] In `slot_application.rs`:
  - [ ] Allocate one accumulator per contribution source.
  - [ ] Lower each source render plan exactly once.
  - [ ] Allocate one accumulator per site only if needed, or append site plans directly at wrapper sites.
  - [ ] Lower site render plans by appending either ordinary `RenderPiece`s or loading contribution source locals.
- [ ] In `render_append.rs`:
  - [ ] Add handling for runtime slot-site wrapper pieces if represented as `RenderPiece::RuntimeSlotSite`.
  - [ ] Use explicit site ID lookup, not key-only lookup.
- [ ] Keep current `RenderPiece::Slot` fallback behavior for wrapper-shaped templates that are not active runtime slot applications.

### Invariants

- [ ] HIR should error only if an AST-prepared site/source ID is missing.
- [ ] HIR must not perform schema target validation.
- [ ] Missing slot contributions should map to empty source/site output.
- [ ] Repeated slots must not re-evaluate dynamic contribution expressions.

## Tests

Add integration tests:

- [ ] `template_runtime_repeated_slot_single_evaluation`
  - Use a runtime contribution whose visible output would differ if evaluated twice. Prefer a mutable local/index/counter pattern if currently legal, or a loop contribution with deterministic output that can reveal duplication.
- [ ] `template_runtime_repeated_slot_different_wrappers`
  - Same slot key appears twice with different child-wrapper metadata; output should match compile-time semantics.
- [ ] `template_runtime_slot_site_nested_wrapper`
  - Slot placeholder appears inside nested wrapper content and receives runtime contribution.
- [ ] `template_runtime_slot_site_fresh`
  - `$fresh` source contribution suppresses immediate parent child wrappers only.
- [ ] `template_runtime_slot_site_children_per_child`
  - `$children(...)` applies per direct contribution, not to a whole aggregate string.
- [ ] `template_runtime_slot_mixed_static_runtime_order`
  - Static and runtime contributions preserve authored order under the same slot target.

Add unit tests if practical:

- [ ] Runtime plan construction produces one site per repeated placeholder.
- [ ] Runtime plan construction produces one source per contribution chunk.
- [ ] Slot site IDs remap and clone correctly.

## Documentation updates

- [ ] Update `docs/language-overview.md` only if extra wording is needed for repeated slots with wrappers.
- [ ] Update `docs/src/docs/progress/#page.bst` coverage text to include repeated runtime slot-site wrapper coverage if added.
- [ ] Update `docs/compiler-design-overview.md` if the plan types are renamed or moved enough to change the AST/HIR ownership wording.

## Phase-end audit / style review / validation

- [ ] Review `runtime_plan.rs` size and responsibility after this phase.
  - [ ] If it mixes routing, source/site construction, remapping, and helper traversal too heavily, split it in Phase 5.
- [ ] Review `render_append.rs` to ensure runtime slot-site handling is localized and comments explain WHY.
- [ ] Confirm source expressions are lowered once for repeated slots.
- [ ] Confirm HIR has no user-facing diagnostics or schema routing logic.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo check`
  - [ ] `cargo test compiler_frontend::ast::templates::`
  - [ ] `cargo test runtime_template_control_flow`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_runtime_slot`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_slots`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 4 — Naming, diagnostics, docs, and stale cleanup

## Context

After correctness fixes, clean up stale naming and comments so the repo reflects the intended language surface. This phase should not change behavior except diagnostic reason names/codes where explicitly accepted.

The goal is to avoid preserving removed concepts as user-facing terminology. `case` should not be described as removed/deprecated/deferred. It should just not exist as a keyword or template construct.

## Implementation steps

### Diagnostic naming

- [ ] Rename `InvalidTemplateStructureReason::TemplateMatchStyleControlFlowRemoved` to neutral wording:
  - Preferred: `TemplateMatchStyleControlFlowUnsupported`
  - Alternative: `TemplateMatchStyleControlFlowNotAllowed`
- [ ] Update constructors/call sites in:
  - [ ] `template_body_parser.rs`
  - [ ] diagnostic renderers
  - [ ] diagnostic code descriptors/golden expected codes
- [ ] Ensure rendered message says:
  - Template heads support Bool conditions and option-present capture only.
  - Use ordinary statement/value `if value is:` blocks for pattern matching outside templates.
  - Do not mention `case` as a replacement or removed syntax.

### Fixture names

- [ ] Rename fixtures if needed:
  - [ ] `template_match_style_if_rejected`
  - [ ] `template_match_style_else_if_rejected`
- [ ] Prefer names such as:
  - [ ] `template_if_match_style_unsupported`
  - [ ] `template_else_if_match_style_unsupported`
- [ ] Update `tests/cases/manifest.toml` and paths.
- [ ] Keep ordinary statement/value pattern matching fixtures unchanged.

### Comment cleanup

- [ ] Update `TemplateControlFlowValidationMode` docs in `template_control_flow/types.rs`:
  - [ ] Say runtime-capable templates reject only escaped helper artifacts after routing/composition.
  - [ ] Do not imply valid runtime slot plans are rejected.
- [ ] Update `template_slots/mod.rs` module docs:
  - [ ] Include `runtime_plan.rs` in the data-flow diagram.
  - [ ] Mention runtime plans share routing but HIR owns only lowering.
- [ ] Update `template_render_plan.rs` docs if a runtime slot site render piece was added.
- [ ] Update `template_types.rs` docs if `runtime_slot_application` fields were renamed or source/site plans were introduced.
- [ ] Update `render_append.rs` docs to mention slot-site/source accumulators if Phase 3 changed the shape.

### Dead/obsolete code cleanup

- [ ] Search for and remove unused helpers after the refactor:
  - [ ] key-only `slot_placeholder_for_key`
  - [ ] stale contribution classification helpers
  - [ ] unused `RuntimeSlotAccumulatorContext` key-only methods if replaced by source/site contexts
  - [ ] compatibility aliases or test-only exports that no longer serve tests
- [ ] Check `#[allow(dead_code)]` comments near changed remap helpers.
  - [ ] Keep only if the guide’s “todo or used only in tests” standard is satisfied.

### Documentation

- [ ] Update `docs/language-overview.md`:
  - [ ] Keep template control-flow surface concise.
  - [ ] Avoid “removed” wording for template match/case.
  - [ ] Add a short runtime slot loop-control example only if it improves clarity.
- [ ] Update `docs/compiler-design-overview.md` if plan type names changed.
- [ ] Update `docs/src/docs/progress/#page.bst` coverage text if new coverage was added.
- [ ] Update `docs/roadmap/roadmap.md` if current notes need more precise follow-up wording.

## Tests

- [ ] Update diagnostic code expectations for renamed diagnostic reasons.
- [ ] Add positive integration case: `case` as an ordinary identifier.
  - [ ] Example: `case = "label"` or `case = [:text]` if valid in the intended scope.
  - [ ] Assert generated output so the test proves `case` is not reserved.
- [ ] Add negative template match-style examples only for the current grammar boundary, not for `case`.
  - [ ] `template_if_match_style_unsupported`
  - [ ] `template_else_if_match_style_unsupported`

## Phase-end audit / style review / validation

- [ ] Search the repo for:
  - [ ] `case`
  - [ ] `TemplateCase`
  - [ ] `TemplateMatch`
  - [ ] `Removed`
  - [ ] old fixture paths
- [ ] Confirm any remaining `case` references are ordinary language pattern matching docs/tests or user identifiers, not template-specific syntax.
- [ ] Confirm module docs match actual submodules and exported types.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo check`
  - [ ] `cargo test compiler_frontend::ast::templates::`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_if_match`
  - [ ] `cargo run --quiet -- tests --backend html --filter case`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 5 — Module and API structure tightening

## Context

After the slot-site model lands, review whether `runtime_plan.rs` and `render_append.rs` have become too broad. Split only if the code now manages multiple distinct concepts. Follow the style guide rule: if behavior is shared outside a complex module, split it out into a shared module; if behavior is not shared, deepen the module with submodules.

This phase should be mostly movement and naming cleanup. It should not change behavior.

## Candidate splits

### AST slot runtime planning

Current likely owner: `src/compiler_frontend/ast/templates/template_slots/runtime_plan.rs`

Split if the file now owns too many concepts:

```text
src/compiler_frontend/ast/templates/template_slots/runtime_plan/
    mod.rs              -- public map and top-level plan types
    sources.rs          -- contribution source IDs and source plan building
    sites.rs            -- slot site IDs and site plan building
    wrapper_plan.rs     -- wrapper render-plan rewriting to site references
    remap.rs            -- remap support if it becomes noisy
```

Rules:

- [ ] `mod.rs` stays a map with concise WHAT/WHY docs.
- [ ] Routing remains in `composition.rs` / `contributions.rs` / `schema.rs` unless there is a clearer shared boundary.
- [ ] Runtime planning should consume routed contributions; it should not duplicate routing.

### HIR append context

Current likely owner: `src/compiler_frontend/hir/hir_expression/templates/render_append.rs`

Split if append context and slot/source lookup makes the file harder to scan:

```text
src/compiler_frontend/hir/hir_expression/templates/
    append_context.rs       -- RuntimeTemplateAppendContext and slot/source lookup contexts
    render_append.rs        -- render piece appending only
    slot_application.rs     -- runtime slot application lowering
```

Rules:

- [ ] `render_append.rs` remains the one path that appends render plans.
- [ ] `slot_application.rs` owns runtime slot application orchestration.
- [ ] Shared context types live in `append_context.rs` if used by multiple HIR template modules.

### Aggregate plan naming

Loop aggregate and conditional child wrapper plans currently share loop-specific naming. If this still reads awkwardly after Phases 1-3:

- [ ] Rename `TemplateLoopAggregateRenderPlan` to a neutral name, such as `TemplateAggregateRenderPlan` or `TemplateWrapperAggregateRenderPlan`.
- [ ] Rename `TemplateLoopAggregatePiece` accordingly.
- [ ] Keep loop-specific wrappers only at call sites that are truly loop-specific.
- [ ] Update docs/comments in:
  - [ ] `template_control_flow/types.rs`
  - [ ] `template_render_units.rs`
  - [ ] `template_folding.rs`
  - [ ] `loop_aggregate.rs`
  - [ ] `render_append.rs`

## Implementation steps

- [ ] Measure file size and concept count after Phase 3.
- [ ] Choose only the splits that reduce reader load.
- [ ] Move code without preserving stale wrappers.
- [ ] Update imports to avoid long inline paths.
- [ ] Add file-level docs to new files.
- [ ] Update `mod.rs` maps.
- [ ] Delete old owner code once new owners are wired.
- [ ] Keep tests outside production files.

## Phase-end audit / style review / validation

- [ ] Check every new file has a WHAT/WHY file doc.
- [ ] Check each module has one owner responsibility.
- [ ] Check moved APIs do not keep compatibility shims.
- [ ] Check comments are reading landmarks, not restatements of syntax.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo check`
  - [ ] `cargo test compiler_frontend::ast::templates::`
  - [ ] `cargo test runtime_template_control_flow`
  - [ ] `cargo run --quiet -- tests --backend html --filter template`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 6 — Test coverage consolidation and pruning

## Context

The completed template control-flow implementation has broad coverage. The follow-up refactor should add missing edge cases, then prune or rename redundant fixtures so the test suite stays useful rather than noisy.

The goal is behavior coverage, not implementation-accident coverage.

## Required coverage additions

### Runtime slot applications

- [ ] Slot application inside template loop can `break`.
- [ ] Slot application inside template loop can `continue`.
- [ ] Slot application output before loop-control is preserved.
- [ ] Slot application loop-control before output does not mark structural output.
- [ ] Repeated runtime slot does not re-evaluate source contribution.
- [ ] Repeated runtime slot with different placeholder-local wrapper metadata matches compile-time slot semantics.
- [ ] Runtime slot contribution mixing static and runtime content preserves order.
- [ ] Nested runtime slot applications work through the new site/source model.
- [ ] Runtime slot site with `$children(...)` wraps per direct child contribution, not aggregate output.
- [ ] Runtime slot site with `$fresh` suppresses only immediate parent child wrappers.

### Template grammar boundary

- [ ] `case` works as an ordinary identifier.
- [ ] Template match-style `if value is:` remains unsupported with a useful diagnostic.
- [ ] Template `[else if value is:]` remains unsupported with the same simple-selector rule.
- [ ] Ordinary statement/value pattern matching remains unaffected.
- [ ] Ordinary statement `else if` remains unsupported and documented as not changed by template `[else if ...]`.

### Diagnostics

- [ ] Runtime control-flow unresolved slot/insert diagnostics still trigger for escaped helper artifacts.
- [ ] Unknown runtime slot targets still use `InvalidTemplateSlotReason` diagnostics, not HIR errors.
- [ ] Missing / malformed / inline `[else if ...]` diagnostics still have stable codes.
- [ ] Malformed `[break]` / `[continue]` diagnostics still have stable codes.

## Redundancy/pruning review

- [ ] Review template integration fixtures for duplicate branch-chain success cases.
- [ ] Keep one canonical case each for:
  - [ ] runtime Bool `if`
  - [ ] runtime option-present `if`
  - [ ] const Bool `if`
  - [ ] const option-present `if`
  - [ ] `[else if ...]` branch chain
  - [ ] runtime conditional loop
  - [ ] runtime range loop
  - [ ] runtime collection loop
  - [ ] runtime slot application branch
  - [ ] runtime slot application loop
- [ ] Remove duplicate cases only when their behavior is fully covered by stronger cases.
- [ ] Do not remove adversarial cases unless they are clearly obsolete or duplicated.
- [ ] Prefer stable diagnostic code assertions over rendered text for failure fixtures.

## Documentation tests

- [ ] Ensure every valid example added to `docs/language-overview.md` is covered by:
  - [ ] `template_docs_examples_runtime_coverage`, or
  - [ ] a focused integration fixture.
- [ ] Ensure invalid examples are clearly labeled as invalid/unsupported.

## Phase-end audit / style review / validation

- [ ] Check fixture names describe user behavior.
- [ ] Check no fixture preserves old “removed case” wording.
- [ ] Check failure fixtures assert stable diagnostic codes where practical.
- [ ] Run:
  - [ ] `cargo fmt`
  - [ ] `cargo check`
  - [ ] `cargo test`
  - [ ] `cargo run --quiet -- tests`
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 7 — Final documentation and implementation matrix review

## Context

After the code is stable, sync public docs, compiler design docs, progress matrix, and roadmap notes. This phase should make the repository self-explanatory for future agents.

## Documentation tasks

### `docs/language-overview.md`

- [ ] Ensure the template control-flow section says exactly:
  - [ ] Template `if` supports Bool and option-present capture.
  - [ ] `[else if ...]` supports the same simple selectors.
  - [ ] `[else]` is standalone and belongs to the nearest active template `if`.
  - [ ] Template loops support conditional, range, and collection forms.
  - [ ] `[break]` / `[continue]` are standalone structural sentinels inside template loops.
  - [ ] Runtime slot applications support default/named/positional/loose routing, repeated slots, missing slots as empty, `$insert(...)`, `$children(...)`, and `$fresh`.
  - [ ] Runtime slot applications inside template loops follow the same loop-control semantics.
- [ ] Avoid saying template match/case syntax was removed/deprecated/deferred.
- [ ] Keep ordinary pattern matching documentation separate under statement/value `if value is:`.

### `docs/compiler-design-overview.md`

- [ ] Confirm AST template ownership includes:
  - [ ] slot schema extraction,
  - [ ] contribution routing,
  - [ ] runtime slot source/site plan construction,
  - [ ] helper artifact rejection.
- [ ] Confirm HIR ownership says:
  - [ ] HIR lowers runtime slot source/site plans through accumulators/appends.
  - [ ] HIR does not validate slot schemas or parse directives.
- [ ] Update plan/type names if Phase 3 or Phase 5 renamed them.

### `docs/src/docs/progress/#page.bst`

- [ ] Update coverage summary for new tests.
- [ ] Keep template surface status as Supported if all validation passes.
- [ ] List deliberate future work only if it is still real:
  - [ ] no template match/case future note,
  - [ ] no runtime directive execution note,
  - [ ] possible future template surfaces only if documented elsewhere.

### `docs/roadmap/roadmap.md`

- [ ] Keep the note concise.
- [ ] Remove any stale follow-up that this refactor completes.
- [ ] Do not add broad speculative template feature lists.

### `docs/src/docs/templates/#page.bst`

- [ ] Update if this doc page exists and still has separate template docs.
- [ ] Ensure examples match `language-overview.md`.

## Phase-end audit / style review / validation

- [ ] Search docs for stale wording:
  - [ ] `case` in template docs.
  - [ ] `removed` near template match/control-flow diagnostics.
  - [ ] `deferred` near runtime slots that are now supported.
- [ ] Confirm docs examples compile or are marked invalid.
- [ ] Run:
  - [ ] `cargo run --quiet -- check docs`
  - [ ] `cargo run --quiet -- tests --backend html --filter template_docs`
  - [ ] `git diff --check`
  - [ ] `just validate`

---

# Phase 8 — Final full validation and release-ready audit

## Context

This phase verifies that the refactor did not alter language boundaries, HIR invariants, or runtime output behavior outside the intended fixes.

## Final audit checklist

### Language surface

- [ ] `case` is ordinary identifier syntax, not a keyword.
- [ ] Template match/case syntax does not exist in parser, docs, matrix, or diagnostics as a supported/deferred concept.
- [ ] Template `if` / `else if` supports only Bool and option-present capture.
- [ ] Statement/value pattern matching remains intact.
- [ ] Ordinary statement `else if` remains unsupported.

### AST/HIR boundary

- [ ] Runtime slot source/site plans are built in AST.
- [ ] HIR does not duplicate schema extraction, target validation, or loose routing.
- [ ] HIR diagnostics for malformed AST plans are internal invariants only.
- [ ] User-facing slot/template mistakes still report through `CompilerDiagnostic`.

### Runtime semantics

- [ ] Runtime slot applications in inactive branches are not evaluated.
- [ ] Runtime slot contributions are evaluated once where repeated slots replay output.
- [ ] Slot-site wrapper metadata matches compile-time semantics.
- [ ] Runtime slot applications inside template loops propagate `[break]` / `[continue]` correctly.
- [ ] Structural no-output and empty-string output remain distinct.

### Code quality

- [ ] No compatibility wrappers around old runtime slot APIs remain.
- [ ] No stale comments from the old template match/case direction remain.
- [ ] No broad files gained unrelated responsibilities.
- [ ] New modules have file-level WHAT/WHY docs.
- [ ] Tests live outside production files.
- [ ] `#[allow(dead_code)]` uses are justified.

## Required validation commands

- [ ] `cargo fmt`
- [ ] `cargo check`
- [ ] `cargo clippy`
- [ ] `cargo test`
- [ ] `cargo run --quiet -- tests`
- [ ] `cargo run --quiet -- check docs`
- [ ] `git diff --check`
- [ ] `just validate`

## Completion criteria

- [ ] All new runtime slot edge cases pass.
- [ ] All docs and matrix updates are committed with the code changes.
- [ ] No old plan direction remains in docs or comments.
- [ ] The codebase has one clear runtime slot application model.
- [ ] The implementation is ready for normal compiler development to continue without a special template-control-flow cleanup context.
