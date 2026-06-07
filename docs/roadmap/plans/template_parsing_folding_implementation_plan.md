# Template Parsing and Folding Hardening — Implementation Plan

## Purpose

Implement the focused template parsing/folding cleanup identified by the audit. The goal is to harden the supported Alpha template surface without rewriting the subsystem.

The coding agent must preserve the current compiler stage boundaries:

- AST owns template parsing, composition, compile-time folding, helper elimination, control-flow validation, and runtime render-plan preparation.
- HIR only lowers finalized runtime templates and AST-prepared runtime slot source/site plans.
- User-facing diagnostics must stay as `CompilerDiagnostic`.
- Internal invariant failures may use `CompilerError`.

## Primary outcomes

1. Strict source-mode template parsing no longer silently accepts malformed EOF/truncated template input.
2. Template-head type validation no longer uses `DataType` for semantic decisions.
3. Slot child-wrapper classification is shared between compile-time composition and runtime slot planning.
4. Const template loop folding is simpler, streaming, and aligned with the aggregate render-plan path used by runtime lowering.
5. Touched code follows `docs/codebase-style-guide.md`, with stale comments, unused parameters, and broad suppressions removed.
6. Tests cover behavior through integration cases where possible, with focused unit tests only for internal invariants.
7. Documentation and the implementation matrix are updated where behavior, module structure, or coverage changes.

## Progress

- Phase 1 reviewed and corrected on 2026-06-07. Normal `.bst` template heads and bodies now report `BST-SYNTAX-0017` for EOF/truncation instead of accepting EOF as a boundary. `TemplateBodyBoundary::Eof` was removed, the head parser separator state is named, and integration coverage now includes unclosed heads, bodies, `if` suffix bodies, `loop` suffix bodies, nested children, and `$children(...)` argument templates. Beandown remains on the strict path because synthetic header preparation injects an explicit template close token; existing HTML integration coverage passed after the correction.
- Phase 2 implemented and parent-corrected on 2026-06-07. Template-head value validation now uses an AST-template-owned `TypeId` renderability classifier, inferred-type validation defers only for the local unresolved constant placeholder reference, unsupported-head diagnostics carry semantic `TypeId`s and render names at the diagnostic boundary, and the obsolete broad unresolved-placeholder table query was removed. Integration coverage now includes alias-to-string success, alias-to-struct rejection, generic struct instance rejection, external opaque rejection, and const-record rejection while a later unrelated constant placeholder is pending.
- Phase 3 implemented and parent-corrected on 2026-06-07. Compile-time slot composition and runtime slot-site planning now share an AST-template-owned child-contribution classifier for folded child output, direct template contributions, source child-template references, and `$fresh` / parent-wrapper skip behavior. The duplicate predicate helpers were removed from both paths without adding new integration fixtures because the existing runtime slot and adversarial `$children` / `$fresh` cases already cover the behavior.
- Phase 4 implemented and parent-corrected on 2026-06-07. Const loop folding now consumes the same prepared `TemplateAggregateRenderPlan` that runtime lowering uses for `[head, loop ...:]` aggregate wrapping instead of rebuilding shared-head content through a separate folding path. Existing aggregate-loop unit and integration coverage passed unchanged.
- Phase 5 implemented and parent-corrected on 2026-06-07. Const range-loop folding now streams counters through an AST-local cursor instead of preallocating range vectors, range and collection loops share one const iteration folding/output/signal handler, and dead vector helpers were removed. Integer stepping still rejects zero, `i64::MIN.abs()` overflow, and checked-add overflow. Float const ranges defensively require an explicit step, reject non-finite / zero / non-progressing steps, and retain descending support. Integration coverage now includes const float missing-step and zero-step parser diagnostics, non-progress folding diagnostics, descending float success, exact custom iteration-limit success, and inclusive/exclusive endpoint behavior.
- Phase 6 implemented on 2026-06-07. Touched template modules no longer carry stale EOF-leniency wording, unused `_directive_name` / `_string_table` plumbing, vector const-range helpers, or file-wide `clippy::needless_return` suppressions. Parser state-machine functions now carry local documented `needless_return` allowances where early exits make token boundaries clearer, and the top-level folding docs describe the current render-plan-based folding path.
- Phase 7 completed on 2026-06-07. No compiler-design update was needed because `template_folding.rs` stayed a single module. `docs/language-overview.md` already documents explicit `.bst` template closing, nested Beandown template close behavior, and float range `by` requirements. The progress matrix now records strict template EOF diagnostics, `TypeId` template-head validation, shared child-contribution routing, aggregate render-plan reuse, streamed const range folding, and the new float/inclusive/limit coverage.
- Phase 9 final audit completed on 2026-06-07. Targeted static checks found no stale EOF-leniency comments, no rendered `type_name` payload in `UnsupportedTypeInTemplateHead`, no `DataType` semantic gate in template-head validation, no duplicate slot contribution predicates, no vector-producing const range helpers, and no production `unwrap()` in touched template parser/folding code. Manual AST/HIR review confirmed template parsing, slot routing, folding, render-plan preparation, and helper elimination remain AST-owned while HIR only consumes finalized runtime template plans. `just validate` passed as the final repository gate.

## Current repo anchors

Use these files as the starting point:

```text
src/compiler_frontend/ast/templates/mod.rs
src/compiler_frontend/ast/templates/create_template_node.rs
src/compiler_frontend/ast/templates/template.rs
src/compiler_frontend/ast/templates/template_types.rs
src/compiler_frontend/ast/templates/template_body_parser.rs
src/compiler_frontend/ast/templates/template_body_sentinels.rs
src/compiler_frontend/ast/templates/template_head_parser/head_parser.rs
src/compiler_frontend/ast/templates/template_head_parser/head_expressions.rs
src/compiler_frontend/ast/templates/template_head_parser/control_flow_suffix.rs
src/compiler_frontend/ast/templates/template_render_units.rs
src/compiler_frontend/ast/templates/template_folding.rs
src/compiler_frontend/ast/templates/template_slots/contribution_shape.rs
src/compiler_frontend/ast/templates/template_slots/composition.rs
src/compiler_frontend/ast/templates/template_slots/runtime_plan/sites.rs
src/compiler_frontend/ast/templates/template_slots/runtime_plan/sources.rs
src/compiler_frontend/compiler_messages/compiler_diagnostic.rs
src/compiler_frontend/compiler_messages/diagnostic_payload/types.rs
src/compiler_frontend/compiler_messages/render/templates.rs
tests/cases/manifest.toml
docs/compiler-design-overview.md
docs/language-overview.md
docs/src/docs/progress/#page.bst
```

Existing useful diagnostic constructors:

```rust
CompilerDiagnostic::missing_closing_delimiter(...)
CompilerDiagnostic::unexpected_end_of_file(...)
CompilerDiagnostic::invalid_template_structure(...)
```

Prefer these over adding new diagnostics unless the existing payload cannot express the case clearly.

---

## Non-goals

- Do not rewrite the entire template subsystem.
- Do not move template semantic ownership from AST to HIR.
- Do not add compatibility wrappers or old/new parallel APIs.
- Do not add backend-specific template control-flow nodes.
- Do not make the template parser permissive for normal `.bst` source.
- Do not introduce rendered type names into diagnostic payloads.
- Do not create broad generic helpers that obscure AST/HIR ownership.

---

## Phase 0 — Preflight and baseline

### Tasks

1. Create a working branch.
2. Run a baseline validation if feasible:

   ```bash
   just validate
   ```

   If this is too slow locally, run at least:

   ```bash
   cargo test
   cargo run -- tests
   cargo clippy
   ```

3. Search current behavior and test anchors:

   ```bash
   rg "template repl atm|overly forgiving|TokenKind::Eof" src/compiler_frontend/ast/templates
   rg "UnsupportedTypeInTemplateHead|type_name" src/compiler_frontend
   rg "diagnostic_type" src/compiler_frontend/ast/templates
   rg "is_child_slot_contribution|contribution_skips_parent_child_wrappers|contribution_template_ref" src/compiler_frontend/ast/templates
   rg "ConstRangeCounter|int_range_iterations|float_range_iterations" src/compiler_frontend/ast/templates
   rg "template_const_loop_iteration_limit|template_unclosed|template_loop|template_runtime_slot" tests/cases
   ```

4. Record which template tests already cover the behavior before adding new cases.

### Acceptance criteria

- Baseline status is known.
- The agent knows which existing tests overlap with the new work.
- No code changes yet.

---

## Phase 1 — Harden EOF/truncated template parsing

### Problem

`template_head_parser/head_parser.rs` currently treats `TemplateClose` and `Eof` as successful head boundaries. The file comment describes old REPL leniency and warns that this can hide unclosed template heads. Normal source parsing should reject malformed/truncated templates with structured diagnostics.

`template_body_parser.rs` also treats `Eof` as a body boundary. This may be legitimate only for synthetic source-kind adapters such as Beandown if they intentionally model an implicit body. It should not be a general normal-source recovery path.

### Design decision

Use strict source parsing by default.

Only allow EOF as a valid template terminator if a current, real caller requires it for an implicit synthetic source body. If Beandown synthetic header preparation already injects a close token, do not add a permissive mode. If Beandown depends on EOF termination, add an explicit mode:

```rust
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateBoundaryMode {
    ExplicitCloseRequired,
    SyntheticEofAllowed,
}
```

Thread it only through the `.bd` synthetic path or the smallest possible parse-options boundary. Do not expose a general “REPL recovery” path unless there is an actual caller.

### Implementation steps

1. In `template_head_parser/head_parser.rs`, replace the loose EOF success path.

   Current broad shape to remove:

   ```rust
   if token == TokenKind::TemplateClose || token == TokenKind::Eof {
       return Ok(...);
   }
   ```

   New behavior:

   - `TemplateClose`: successful empty-body / no-body template head boundary.
   - `Eof`: `CompilerDiagnostic::unexpected_end_of_file(Some("]"), location)` or `missing_closing_delimiter("]", location)`.
   - Use `string_table.intern("]")` for the expected delimiter.

2. Replace `expecting_comma: bool` with a named state.

   Suggested type:

   ```rust
   #[derive(Clone, Copy, Debug, PartialEq, Eq)]
   enum TemplateHeadSeparatorState {
       ExpectItem,
       ExpectSeparatorOrBody,
   }
   ```

   This reduces boolean state ambiguity and matches the style guide preference for enums over meaningful booleans.

3. In `template_body_parser.rs`, audit every `TemplateBodyBoundary::Eof` use.

   - Normal template body parsing should return a diagnostic when EOF appears before the explicit `]`.
   - If `TemplateBodyBoundary::Eof` remains needed internally, rename it to communicate policy, for example:

     ```rust
     TemplateBodyBoundary::SyntheticEof
     ```

     or keep `Eof` private but only produce it under `TemplateBoundaryMode::SyntheticEofAllowed`.

4. Ensure direct else/loop sentinel parsing still reports the existing structured errors:
   - `MalformedTemplateElse`
   - `MalformedTemplateElseIf`
   - `MalformedTemplateBreak`
   - `MalformedTemplateContinue`
   - `MissingTemplateElseIfCondition`

5. Update parser tests and integration fixtures.

### Tests

Prefer integration tests under `tests/cases` with stable `diagnostic_codes`.

Add only missing cases after checking existing coverage:

```text
template_unclosed_head_rejected
template_unclosed_body_rejected
template_unclosed_if_suffix_rejected
template_unclosed_loop_suffix_rejected
template_unclosed_nested_child_rejected
template_children_unclosed_argument_rejected
```

If Beandown behavior is affected:

```text
beandown_nested_template_unclosed_rejected
beandown_implicit_body_still_folds
```

### Acceptance criteria

- Normal `.bst` templates cannot be silently accepted at EOF.
- Beandown implicit-body behavior still works if it is intended to be EOF-terminated.
- No new panic, unwrap, or `CompilerError` path is introduced for malformed source.
- Old REPL/stale comments are deleted or replaced with current WHAT/WHY comments.

---

## Phase 2 — Move template-head type validation from `DataType` to `TypeId`

### Problem

`template_head_parser/head_expressions.rs` currently validates allowed template-head value types by matching `expression.diagnostic_type` against `DataType`. This conflicts with the compiler contract that semantic type decisions use `TypeId` and `TypeEnvironment`; `DataType` is parse/diagnostic spelling only after semantic IDs exist.

The diagnostic payload currently stores both `type_id` and a display-only `type_name` in `InvalidTemplateStructureReason::UnsupportedTypeInTemplateHead`. That should be simplified to semantic facts only.

### Design decision

Use a single renderability classifier for template-head values.

First look for existing string/template coercion helpers in:

```text
src/compiler_frontend/type_coercion/string.rs
```

If an existing helper already classifies renderable expression/type kinds cleanly, extend it. Otherwise add a small AST template-owned module:

```text
src/compiler_frontend/ast/templates/template_renderability.rs
```

Suggested API:

```rust
pub(crate) enum TemplateHeadRenderability {
    Renderable,
    Unsupported(TemplateHeadTypeRejection),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TemplateHeadTypeRejection {
    UnsupportedType,
}

pub(crate) fn classify_template_head_type(
    type_id: TypeId,
    type_environment: &TypeEnvironment,
) -> TemplateHeadRenderability;
```

Avoid over-generalizing. This helper is for template-head value validation, not a new whole-language display trait system.

### Implementation steps

1. Add or extend the classifier.

   Allow by semantic `TypeId` / `TypeEnvironment` identity:

   - `String` / string slices as currently accepted.
   - Template and template-wrapper values.
   - `Int`, `Float`, `Bool`, `Char`.
   - Compile-time path values if they have a semantic type entry.

   Reject:

   - Structs and const records as renderable values.
   - Collections.
   - Functions.
   - Fallible/result carriers.
   - External opaque types unless an explicit existing string coercion rule already supports them.
   - Dynamic trait values unless explicitly supported by current type-coercion rules.

2. Update `validate_template_head_value_type` in `head_expressions.rs`.

   - Remove `DataType` matching.
   - Keep the existing fallible-carrier check, but base it on `type_environment.is_fallible_carrier(expression.type_id)`.
   - Use the new classifier for the rest.

3. Narrow inferred-type deferral.

   Current behavior broadly defers if any unresolved constant placeholder exists. Replace that with local logic:

   ```rust
   let should_defer_validation =
       is_unresolved_constant_placeholder_reference(&expression, context);
   ```

   Do not let unrelated expressions skip validation because some other placeholder exists.

4. Simplify the diagnostic payload.

   In `diagnostic_payload/types.rs`, change:

   ```rust
   UnsupportedTypeInTemplateHead {
       type_id: TypeId,
       type_name: StringId,
   }
   ```

   to:

   ```rust
   UnsupportedTypeInTemplateHead {
       type_id: TypeId,
   }
   ```

   Or, if useful:

   ```rust
   UnsupportedTypeInTemplateHead {
       type_id: TypeId,
       reason: TemplateHeadTypeRejection,
   }
   ```

   Do not store a rendered type name.

5. Update render code.

   `render/templates.rs` currently renders template structure messages without a `DiagnosticRenderContext`. Update the render path so the unsupported-template-head-type message can use `diagnostic_type_name(type_id, context)` at render time.

   Preferred shape:

   ```rust
   pub(crate) fn invalid_template_structure_message(
       reason: InvalidTemplateStructureReason,
       context: DiagnosticRenderContext<'_>,
   ) -> String
   ```

   If this causes too much churn, add a focused helper for only the unsupported-type case at the payload render boundary. Do not reintroduce `type_name`.

6. Update string-id remapping if any removed payload field was previously remapped.

7. Update all call sites constructing `UnsupportedTypeInTemplateHead`.

### Tests

Add or update integration cases:

```text
template_head_alias_to_string_success
template_head_alias_to_struct_rejected
template_head_external_opaque_rejected
template_head_generic_struct_instance_rejected
template_head_unrelated_unresolved_const_does_not_defer_invalid_type
```

Add dynamic trait coverage only if the repo already has concise dynamic trait fixtures that make this low-cost:

```text
template_head_dynamic_trait_value_rejected
```

Use `diagnostic_codes` for failures. Add rendered text fragments only if type-name rendering is explicitly being tested.

### Acceptance criteria

- `rg "UnsupportedTypeInTemplateHead .*type_name|type_name.*UnsupportedTypeInTemplateHead"` returns no live code.
- `head_expressions.rs` no longer uses `DataType` to decide template-head semantic validity.
- Unsupported template-head type diagnostics render type names through the diagnostic render context.
- Existing successful scalar/string/template interpolation behavior still passes.

---

## Phase 3 — Share slot child-contribution classification

### Problem

Compile-time slot composition and runtime slot-site planning duplicate the same child-contribution predicates:

- `is_child_slot_contribution`
- `contribution_template_ref`
- `contribution_skips_parent_child_wrappers`
- child-template-output checks

This is a drift risk for `$children(...)`, `$fresh`, repeated slots, runtime slot sites, and nested wrappers.

### Design decision

Extract only the classification layer. Keep compile-time expansion and runtime site planning separate because they produce different output shapes.

### Implementation steps

1. Add:

   ```text
   src/compiler_frontend/ast/templates/template_slots/contribution_shape.rs
   ```

2. Add the module in:

   ```rust
   // src/compiler_frontend/ast/templates/template_slots/mod.rs
   mod contribution_shape;
   ```

3. Implement a small shared classifier.

   Suggested shape:

   ```rust
   pub(super) struct ContributionShape<'a> {
       pub(super) template: Option<&'a Template>,
       pub(super) is_child_template_contribution: bool,
       pub(super) skips_parent_child_wrappers: bool,
   }

   pub(super) fn classify_contribution_atom(atom: &TemplateAtom) -> ContributionShape<'_> {
       ...
   }
   ```

4. Replace duplicate helpers in:

   ```text
   template_slots/composition.rs
   template_slots/runtime_plan/sites.rs
   ```

5. Keep naming explicit. Avoid a broad generic “slot utils” module.

6. Add unit tests only for the classifier if integration tests cannot inspect a behavior. Put tests in an existing test file under:

   ```text
   src/compiler_frontend/ast/templates/tests/
   ```

   or the current template-slot test area. Do not place tests inside production files.

### Integration tests

Add or update parity cases only where existing coverage is missing:

```text
template_children_fresh_const_runtime_parity
template_slot_nested_children_no_leakage
template_runtime_repeated_slot_site_local_wrappers
```

Before adding, check current cases:

```text
template_runtime_repeated_slot_different_wrappers
template_runtime_slot_site_nested_wrapper
template_runtime_slot_site_fresh
template_runtime_slot_site_children_per_child
adversarial_template_children_fresh_chain
```

If those already cover the behavior strongly, avoid duplicate integration cases and add a focused unit test for the shared classifier instead.

### Acceptance criteria

- Duplicate child-contribution predicate helpers are removed from compile-time and runtime paths.
- `$fresh` and direct-child wrapper behavior remains unchanged in existing tests.
- Runtime repeated slot site behavior remains unchanged.
- The new module has a file-level WHAT/WHY doc comment.

---

## Phase 4 — Unify aggregate render-plan handling for const and runtime loops

### Problem

Runtime loop aggregate wrapping uses `TemplateAggregateRenderPlan` built in `template_render_units.rs`, then consumed by HIR. Const folding separately recomposes loop shared-head content in `apply_loop_shared_head`. These two paths encode the same `[head, loop ...:] wraps the whole aggregate once` rule.

### Design decision

Make aggregate render-plan preparation the single AST-owned representation for both runtime and const loop aggregate wrapping.

### Implementation steps

1. In `template_render_units.rs`, rename and expose the aggregate plan helper:

   Current private helper:

   ```rust
   fn prepare_loop_aggregate_render_plan(...)
   ```

   Suggested exposed helper:

   ```rust
   pub(in crate::compiler_frontend::ast::templates) fn prepare_template_aggregate_render_plan(
       shared_head_prefix: &TemplateContent,
       string_table: &StringTable,
   ) -> Result<TemplateAggregateRenderPlan, TemplateError>
   ```

2. Update `prepare_control_flow_render_units` to call the renamed helper.

3. In `template_folding.rs`, replace `apply_loop_shared_head` with a fold helper that consumes `TemplateAggregateRenderPlan`.

   Suggested shape:

   ```rust
   fn fold_aggregate_render_plan(
       aggregate_plan: &TemplateAggregateRenderPlan,
       aggregate_output: StringId,
       fold_context: &mut TemplateFoldContext<'_>,
       fallback_location: &SourceLocation,
   ) -> Result<TemplateEmission, TemplateError>
   ```

4. For `TemplateAggregatePiece::Aggregate`, append the already-folded aggregate string.
5. For `TemplateAggregatePiece::Render(piece)`, fold the render piece through the same plan-piece folding logic used by `fold_plan_to_emission`.
6. Ensure loop-control pieces inside aggregate wrapper plans remain impossible or handled as internal invariant errors.
7. Keep aggregate wrapping in AST. Do not move any slot schema or aggregate-plan construction into HIR.

### Tests

Use existing aggregate tests first:

```text
template_loop_head_wraps_aggregate_once
template_loop_empty_skips_wrapper
template_loop_per_iteration_wrapper_inside_body
```

Add one explicit parity case only if missing:

```text
template_const_runtime_loop_aggregate_wrapper_parity
```

### Acceptance criteria

- Const loop aggregate wrapping and runtime aggregate wrapping use the same aggregate-plan construction.
- Existing runtime HIR lowering still consumes `TemplateAggregateRenderPlan`.
- No HIR-side schema validation or template plan reconstruction is added.

---

## Phase 5 — Stream const loop folding and harden numeric edge cases

### Problem

Const range loops currently allocate all counters before folding the body. Range and collection loops duplicate iteration folding, binding creation, output aggregation, and break/continue handling. Float ranges also need alignment with the language rule that `by` should be required for float ranges.

### Design decision

Deepen the folding module rather than creating cross-stage helpers. This behavior is not shared outside AST folding.

If the refactor is modest, keep `template_folding.rs` as one file and extract helper structs inside it. If changes become broad, convert it into a folder module:

```text
src/compiler_frontend/ast/templates/template_folding/
  mod.rs
  context.rs
  control_flow.rs
  loops.rs
  render_plan.rs
```

Keep the public API unchanged:

```rust
TemplateFoldContext
TemplateEmission
TemplateFoldBinding
Template::fold_into_stringid(...)
Template::fold_to_emission(...)
```

### Implementation steps

1. Extract one iteration result handler.

   Suggested helper:

   ```rust
   fn fold_loop_iteration_with_bindings(
       body_plan: &TemplateRenderPlan,
       iteration_bindings: Vec<TemplateFoldBinding>,
       fold_context: &mut TemplateFoldContext<'_>,
       diagnostic_location: &SourceLocation,
       aggregate: &mut String,
   ) -> Result<TemplateEmission, TemplateError>
   ```

   Reuse this for range and collection loops.

2. Replace `int_range_iterations` and `float_range_iterations` vector generation with streaming iterators or callback drivers.

   Suggested types:

   ```rust
   enum ConstRangeCursor {
       Int { current: i64, end: i64, end_kind: RangeEndKind, step: i64, ascending: bool },
       Float { current: f64, end: f64, end_kind: RangeEndKind, step: f64, ascending: bool },
   }
   ```

   Or implement simple loop drivers directly if clearer.

3. Enforce the existing iteration limit during streaming. Do not allocate up to the limit first.

4. Harden integer ranges:

   - `by 0` remains invalid.
   - `i64::MIN.abs()` overflow remains invalid.
   - `checked_add` overflow remains invalid.
   - Inclusive descending and ascending ranges must still include endpoints correctly.

5. Harden float ranges:

   - Reject non-finite start, end, or step.
   - Reject `by 0.0`.
   - Reject non-progressing steps where `current + step == current`.
   - Align with the docs: if either bound is `Float`, require an explicit `by`.

   If enforcing explicit float `by` globally is not feasible in this slice, enforce it in const template folding and update the progress matrix to call out any runtime-loop gap. Do not silently leave docs and implementation contradictory.

6. Remove now-dead helper types/functions:
   - `ConstRangeCounter` if no longer needed.
   - vector-returning `int_range_iterations`.
   - vector-returning `float_range_iterations`.

### Tests

Add missing targeted cases:

```text
template_const_float_range_missing_by_rejected
template_const_float_range_zero_by_rejected
template_const_float_range_non_progress_rejected
template_const_float_range_descending_success
template_const_loop_limit_exact_success
template_const_loop_limit_exceeded_rejected
template_const_range_inclusive_exclusive_edges
```

If equivalent tests already exist under different names, update them instead of duplicating.

### Acceptance criteria

- Const loops fold without allocating all range counters.
- Limit behavior is unchanged except for clearer diagnostics.
- Float range behavior is documented and tested.
- Break/continue output preservation remains intact for const loops.

---

## Phase 6 — Focused cleanup in touched files

### Tasks

1. Remove unused parameters and call-site noise:

   ```text
   head_parser.rs: _directive_name
   head_expressions.rs: _string_table in handle_template_value_in_template_head
   template_slots/composition.rs: _string_table in ensure_no_slot_insertions_remain
   ```

   Only keep a parameter if it is used to improve diagnostics.

2. Remove broad lint suppressions where practical:

   ```text
   #![allow(clippy::needless_return)]
   ```

   If explicit returns are intentionally clearer in parser state-machine code, narrow the allow and add a short rationale.

3. Update stale comments:

   - Remove “template repl atm” / old EOF leniency comments.
   - Update `template_folding.rs` docs if folding is now render-plan based.
   - Ensure new files have WHAT/WHY docs.

4. Normalize section banners in touched files to the style-guide format or remove unnecessary banners.

5. Keep function argument lists readable. Add input/context structs only where a call has become noisy from this plan.

### Acceptance criteria

- `rg "template repl atm|overly forgiving" src/compiler_frontend` returns nothing.
- `rg "_directive_name|_string_table" src/compiler_frontend/ast/templates` has no leftovers from touched paths unless justified.
- New modules have file-level docs.
- No tests are embedded in production files.

---

## Phase 7 — Documentation updates

Documentation updates are part of the implementation, not optional cleanup.

### Update `docs/compiler-design-overview.md` if module structure changes

If `template_folding.rs` becomes a folder module, update the AST Templates section or add a short note that template folding is internally split into context/control-flow/loop/render-plan helpers.

Do not describe private helper names in too much detail. Keep this as stage ownership and data-flow documentation.

### Update `docs/language-overview.md` if behavior is clarified or changed

Required updates if implemented:

1. State that normal `.bst` templates must close explicitly and truncated templates produce syntax diagnostics.
2. Preserve Beandown rules if `.bd` implicit bodies are intentionally EOF-terminated.
3. Clarify float range `by` behavior if the implementation now rejects no-step float ranges in const templates or all loops.

Do not update the language docs to describe implementation internals.

### Update `docs/src/docs/progress/#page.bst`

Update the implementation matrix if any of these changes are made:

- New strict unclosed-template diagnostics.
- New template-head semantic type validation behavior.
- Float const range loop behavior.
- New or changed template test coverage.
- Any remaining known gap between language docs and implementation.

If runtime float ranges without `by` remain accepted while const template float ranges reject them, list that as a watch point.

### Update `docs/codebase-style-guide.md` only if the standard changes

This plan should not require changing the style guide. Use it as validation criteria.

### Acceptance criteria

- Any behavior change is documented in either `language-overview.md` or the progress matrix.
- Any module-structure change is reflected in compiler-design docs.
- No stale docs claim the old parser leniency.

---

## Phase 8 — Test implementation details

### Integration test rules

Follow `tests/cases` structure:

```text
tests/cases/<case_name>/
  input/
    main.bst
  expect.toml
  golden/
    html/
    html_wasm/
```

For failure tests:

```toml
[backends.html]
mode = "failure"
diagnostic_codes = ["..."]
```

For success tests, prefer top-level template output over `io(...)` unless stdout behavior is being tested.

### Manifest

Every new case must be added to:

```text
tests/cases/manifest.toml
```

Use tags consistently:

```toml
tags = ["integration", "templates"]
tags = ["integration", "templates", "diagnostics"]
tags = ["integration", "templates", "config"]
```

### Redundancy policy

Before adding a case, search the manifest and fixtures. If a current case already tests the behavior, either:

- extend that case with one extra assertion, or
- add a focused unit test if the behavior is an internal invariant.

Do not create several near-duplicate integration cases for the same diagnostic.

---

## Phase 9 — Final audit, style-guide review, and validation

This phase is mandatory.

### Mechanical validation

Run:

```bash
cargo fmt
cargo clippy
cargo test
cargo run -- tests
just validate
```

If `just validate` already runs the others, still record its result as the final gate.

### Targeted static checks

Run and resolve or justify every hit:

```bash
rg "template repl atm|overly forgiving" src/compiler_frontend
rg "UnsupportedTypeInTemplateHead.*type_name|type_name.*UnsupportedTypeInTemplateHead" src/compiler_frontend
rg "diagnostic_type" src/compiler_frontend/ast/templates/template_head_parser/head_expressions.rs
rg "DataType" src/compiler_frontend/ast/templates/template_head_parser/head_expressions.rs
rg "is_child_slot_contribution|contribution_skips_parent_child_wrappers|contribution_template_ref" src/compiler_frontend/ast/templates/template_slots
rg "Vec<ConstRangeCounter>|int_range_iterations|float_range_iterations" src/compiler_frontend/ast/templates
rg "#!\[allow\(clippy::needless_return\)\]" src/compiler_frontend/ast/templates
rg "unwrap\(" src/compiler_frontend/ast/templates
```

Expected outcomes:

- No old EOF leniency comments.
- No `type_name` field in `UnsupportedTypeInTemplateHead`.
- No `DataType` semantic gate in template-head validation.
- Only one shared child-contribution classifier.
- No vector-producing const range loop helpers.
- No new user-input `unwrap()` in touched parser/folding code.

### Manual stage-boundary review

Check:

1. AST still owns:
   - template parsing;
   - slot schema/routing;
   - compile-time folding;
   - runtime render-plan preparation;
   - helper elimination.

2. HIR still only consumes:
   - finalized runtime templates;
   - `TemplateRenderPlan`;
   - `TemplateAggregateRenderPlan`;
   - `RuntimeSlotApplicationPlan`.

3. Diagnostics:
   - malformed source uses `CompilerDiagnostic`;
   - internal impossible states use `CompilerError`;
   - new diagnostics carry `SourceLocation`;
   - type diagnostics carry `TypeId`, not rendered names.

4. No compatibility scaffolding:
   - no old/new parser entrypoint pair;
   - no forwarding shim that only preserves the old API;
   - no duplicate slot predicate path.

5. Style-guide compliance:
   - file-level docs on new files;
   - readable named steps;
   - no broad noisy imports;
   - no stale comments;
   - tests outside production files;
   - no unnecessary `#[allow]`.

6. Documentation:
   - progress matrix reflects coverage and any known implementation gap;
   - language docs reflect behavior changes only;
   - compiler design docs reflect module-structure changes only.

### Final deliverables

The agent should finish with:

1. Code changes.
2. New or updated tests.
3. Updated docs:
   - `docs/src/docs/progress/#page.bst` when coverage/behavior changes.
   - `docs/language-overview.md` when language behavior is clarified/changed.
   - `docs/compiler-design-overview.md` when module shape changes.
4. Validation command output summary.
5. Manual audit summary:
   - AST/HIR boundary status.
   - Diagnostic payload status.
   - Style-guide status.
   - Any intentionally deferred follow-up.

---

## Suggested implementation order

Use this order to keep changes reviewable:

1. Parser EOF hardening and tests.
2. Template-head `TypeId` renderability and diagnostics.
3. Slot contribution classifier extraction.
4. Aggregate render-plan reuse.
5. Const loop streaming/hardening.
6. Cleanup pass.
7. Documentation updates.
8. Final audit/validation.

Do not start Phase 5 before Phase 4; const loop folding should reuse the aggregate-plan path after it exists.

---

## Risk notes

### Beandown EOF behavior

Beandown bodies are implicit compile-time markdown templates. Before rejecting EOF globally, inspect the Beandown synthetic-header path. If it synthesizes explicit template close tokens, strict EOF handling is safe. If not, add an explicit synthetic EOF mode and limit it to that source-kind path.

### Diagnostic renderer churn

Removing `type_name` from `UnsupportedTypeInTemplateHead` may require threading `DiagnosticRenderContext` into `invalid_template_structure_message`. Keep the change focused: do not redesign all renderers.

### Float loop policy

The language docs say float ranges should require `by`. If current generic loop parsing does not enforce this globally, either implement the global enforcement where type information is available or clearly document the current remaining runtime gap in the progress matrix.

### Test bloat

Template coverage is already broad. Prefer extending existing cases and adding focused unit tests for internal predicate/iterator behavior over adding many overlapping integration fixtures.

---

## Completion checklist

- [x] Normal source templates reject EOF/truncation with structured diagnostics.
- [x] Beandown implicit-body behavior remains correct and tested.
- [x] Template-head renderability uses `TypeId` / `TypeEnvironment`.
- [x] Unsupported template-head type diagnostics no longer carry rendered names.
- [x] Compile-time and runtime slot paths share child-contribution classification.
- [x] Const and runtime loop aggregate wrapping use the same aggregate-plan preparation.
- [x] Const range loops stream iterations instead of preallocating all counters.
- [x] Float const range behavior is explicit, tested, and documented.
- [x] Stale comments, unused parameters, broad clippy suppressions, and duplicate helpers are removed.
- [x] New docs are updated where behavior/module shape changed.
- [x] `just validate` passes.
- [x] Manual AST/HIR diagnostic/type boundary review is complete.
