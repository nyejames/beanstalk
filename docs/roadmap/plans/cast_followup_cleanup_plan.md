# Cast Follow-Up Cleanup and Policy Parity Implementation Plan

## Purpose

This plan tightens the `cast` implementation after the first merged slice. It focuses on correctness parity, stale-surface cleanup, parser/API maintainability, documentation accuracy, and coverage gaps without expanding the language surface.

The implementation must preserve the final cast design:

- `cast` targets remain closed to builtin targets only: `Bool`, `Int`, `String`, `Char`, `Float`, and `Error`.
- User-defined cast targets remain outside design scope.
- User-defined source evidence remains same-file nominal evidence through compiler-owned core cast traits.
- `cast` is only valid at explicit typed receiving boundaries.
- AST owns cast target/evidence resolution and const folding.
- HIR must not solve trait evidence.
- HTML JS is the current Alpha runtime target; HTML-Wasm must reject reachable runtime casts before lowering until Wasm cast lowering is implemented.
- Float-to-string backend/fold formatting parity is **not** solved in this plan. It must be recorded explicitly as roadmap/progress follow-up work.

## Current repo anchor

Current `main` is identical to commit:

```text
a151d8552f7493ed6c7dfed5df4870f9b7daf6cf
```

Target the current repo shape:

- Cast frontend owner:
  - `src/compiler_frontend/builtins/casts/mod.rs`
  - `src/compiler_frontend/builtins/casts/evidence.rs`
  - `src/compiler_frontend/builtins/casts/policies.rs`
  - `src/compiler_frontend/builtins/casts/resolution.rs`
  - `src/compiler_frontend/builtins/casts/targets.rs`
  - `src/compiler_frontend/builtins/casts/traits.rs`
- Parser target-context owner:
  - `src/compiler_frontend/type_coercion/parse_context.rs`
  - `src/compiler_frontend/ast/expressions/parse_expression.rs`
  - `src/compiler_frontend/ast/expressions/parse_expression_dispatch.rs`
- Constant folding:
  - `src/compiler_frontend/optimizers/constant_folding.rs`
- HIR cast representation/lowering:
  - `src/compiler_frontend/hir/expressions.rs`
  - `src/compiler_frontend/hir/statements.rs`
  - `src/compiler_frontend/hir/hir_expression.rs`
- JS backend runtime:
  - `src/backends/js/runtime/casts.rs`
  - `src/backends/js/js_expr.rs`
  - `src/backends/js/js_statement.rs`
  - `src/backends/js/emitter.rs`
- HTML-Wasm unsupported-feature gate:
  - `src/projects/html_project/wasm/artifacts.rs`
- Documentation:
  - `docs/language-overview.md`
  - `docs/compiler-design-overview.md`
  - `docs/src/docs/language-overview/#page.bst`
  - `docs/src/docs/progress/#page.bst`
  - `docs/roadmap/roadmap.md`
  - generated docs under `docs/release/**` if this repo expects generated docs to be committed with source docs changes.
- Tests:
  - `src/compiler_frontend/builtins/casts/tests/**`
  - `src/compiler_frontend/optimizers/tests/constant_folding_tests.rs`
  - `src/compiler_frontend/ast/expressions/tests/cast_boundary_tests.rs`
  - `tests/cases/manifest.toml`
  - relevant `tests/cases/cast_*` folders.

## Research answers and implementation implications

### 1. Complexity / indirection / noisy code

- The expression parser now threads `ExpectedType`, `CastTargetContext`, trailing policy, value mode, scope context, type interner, token stream, and string table through several long-argument wrappers.
- This violates the project preference for context/input structs for shared state.
- Replace the wrapper-heavy expression parse API with one input struct and named constructors for common parse modes.
- The core cast trait metadata has parallel representations: `BUILTIN_CAST_TRAIT_ROWS` and `CORE_CAST_TRAIT_KINDS`. Keep one authoritative row table.
- JS cast helper emission already pre-collects used policies, but still emits baseline numeric cast helpers unconditionally. Make helper emission fully demand-driven.

### 2. Legacy codepaths / stale comments / obsolete scaffolding

- `docs/roadmap/roadmap.md` still references `docs/roadmap/plans/cast_operator_implementation_plan.md`, but that plan file has been removed. Remove or replace the stale path.
- Several comments still refer to implementation-plan phases such as “phase 2 evidence table.” Replace with current permanent ownership language.
- `CastEvidenceKind` in `src/compiler_frontend/builtins/casts/targets.rs` appears redundant with `TraitEvidenceKind` and current resolved evidence enums. Remove unless a real current caller needs it.
- Remove `#[allow(dead_code)]` annotations that only protect metadata projection helpers used by tests. Prefer tests that validate the single row table directly.

### 3. Style guide compliance

- Use `CompilerDiagnostic` for user-facing policy diagnostics and `CompilerError` only for HIR/backend invariants.
- Keep new diagnostics structured through existing reason enums where possible.
- Use WHAT/WHY comments on new module/file boundaries and non-obvious policy decisions.
- Avoid compatibility wrappers after parser API refactor. Update call sites to the new API and delete old APIs in the same phase.
- Keep tests in test files, not production modules.
- Run `just validate` and a manual stage-boundary review after every phase.

### 4. Design drift

- HIR currently keeps explicit runtime cast operations only for compiler-owned builtin evidence. User-defined evidence lowers to direct user-function calls during HIR lowering. This is a simpler post-hardening design, but docs should explicitly say so.
- The Rust policy and JS runtime currently disagree on `Int` range for cast results. This is real semantic drift and must be fixed immediately.
- Float-to-string formatting parity is knowingly not fixed. Track it explicitly as roadmap/progress follow-up so it does not disappear as an implied TODO.

### 5. Test coverage gaps / redundancy

Missing or incomplete:

- Folded-vs-runtime parity for `String -> Int` and `Float -> Int` at integer boundary values.
- Optional receiving target recovery semantics: catch handlers produce the inner `T`, then the cast result wraps to `T?`.
- Fully demand-driven JS helper emission, if changed.
- Roadmap/progress docs expectations around float-to-string parity.
- Stale duplicate scalar constructor removal tests should be audited. Keep only cases that protect distinct parser/diagnostic paths.

### 6. Module/file structure opportunities

Create one small shared module:

```text
src/compiler_frontend/builtins/casts/numeric_limits.rs
```

Owner:

- Alpha cast integer range policy shared by Rust-side policy folding and JS helper emission.
- Clear WHAT/WHY comment explaining why the Alpha JS runtime target constrains `Int` cast materialization.

Do not split JS runtime cast helpers unless this cleanup makes `src/backends/js/runtime/casts.rs` substantially harder to review. The JS helper behavior is backend-local, so deepening the module is optional and should only be done if the file grows into mixed responsibilities.

---

## Phase 0 — Baseline audit and work boundary

Status: complete.

Baseline recorded:

- Branch/head: `main` at `a151d8552f7493ed6c7dfed5df4870f9b7daf6cf`.
- Worktree was not clean at startup because this plan and its roadmap link were newly added, plus an untracked `.DS_Store` existed under `docs/roadmap/plans/`. The user confirmed the plan link/setup and explicitly requested this plan proceed.
- Baseline `just validate` passed before implementation. It completed clippy for native/linux/windows targets, 2357 unit tests, 1538 integration cases, docs check, and benchmark check with no measurable change.
- Phase 0 drift matches this plan's expected cleanup scope: stale `cast_operator_implementation_plan.md` roadmap reference, `phase 2 evidence` comments, cast metadata duplication through `CORE_CAST_TRAIT_KINDS` / `CastEvidenceKind`, cast-only dead-code suppressions, expression parser `too_many_arguments` suppressions, and JS cast helper emission in all HTML goldens even when no cast policy is used.
- No unexpected repo drift after the anchor commit was found outside the active plan/setup and planned cleanup surfaces.

### Context

Start by proving the work is being done against the expected repo shape. The implementation should not begin by changing semantics; first establish a clean baseline, list exact touched surfaces, and confirm no newer commits changed the target code.

### Tasks

- [x] Confirm the current branch head and record it in the PR/commit message:

  ```bash
  git rev-parse HEAD
  git status --short
  ```

- [x] Confirm the working tree is clean before starting.
  - Exception recorded above: startup was intentionally dirty with this plan/setup plus `.DS_Store`.
- [x] Run baseline validation:

  ```bash
  just validate
  ```

- [x] Inspect the current cast implementation files listed in **Current repo anchor**.
- [x] Inspect the current roadmap/progress/docs files listed in **Current repo anchor**.
- [x] Search for stale old-cast identifiers:

  ```bash
  rg "BuiltinCastKind|BuiltinCast|Int\\(|Float\\(|Bool\\(|String\\(|Char\\(" src docs tests
  ```

  Keep test fixtures that intentionally assert removed constructor diagnostics. Remove or update stale production/docs mentions.

- [x] Search for new long-argument/parser wrappers and dead-code suppressions:

  ```bash
  rg "too_many_arguments|dead_code|CastEvidenceKind|CORE_CAST_TRAIT_KINDS" src/compiler_frontend
  ```

- [x] Record any additional drift found after `a151d8552f7493ed6c7dfed5df4870f9b7daf6cf`.

### Phase 0 audit / style / validation

- [x] No code changes yet except optional local notes.
- [x] Confirm baseline `just validate` result.
- [x] Confirm no unexpected repo drift.
- [x] Confirm the implementation remains scoped to cleanup/parity/documentation, not new cast targets or new conversion syntax.

---

## Phase 1 — Fix `Int` cast policy parity across AST folding and JS runtime

Status: complete.

Summary:

- Added `numeric_limits.rs` as the frontend cast owner for the Alpha JS-safe integer range.
- Updated Rust-side `String -> Int` and `Float -> Int` policies to reject values outside `-9007199254740991..=9007199254740991` with the existing out-of-range error codes.
- Updated JS cast helpers to emit the range from Rust constants and use one shared `__bs_cast_int_in_range` predicate in `__bs_cast_int` and `__bs_cast_float_to_int`.
- Added unit and integration coverage for safe-integer success, const rejection, runtime success, runtime recovery codes, and HTML-Wasm unsupported runtime cast behavior.
- Updated generated HTML goldens that embed the baseline JS cast helper prelude.
- Updated `docs/src/docs/progress/#page.bst` with the safe-integer policy and the `Float -> String` parity watch point. Broader language-doc wording remains scheduled for Phase 6.

### Context

Before this phase, the Rust-side builtin cast policy accepted `String -> Int` and `Float -> Int` across full `i64` range, while the JS backend could only faithfully represent safe JS integers through `Number`. That allowed compile-time casts to succeed where equivalent runtime casts failed or lost precision.

For the Alpha JS target, define one explicit portable cast policy:

```text
Builtin casts to Int materialize only JavaScript-safe integer values:
-9_007_199_254_740_991 through 9_007_199_254_740_991.
```

This phase only fixes explicit cast policies. It does not redesign all `Int` literals, arithmetic, or Wasm `i64` lowering.

### Tasks

#### Shared numeric limit owner

- [x] Add:

  ```text
  src/compiler_frontend/builtins/casts/numeric_limits.rs
  ```

- [x] Export it from:

  ```text
  src/compiler_frontend/builtins/casts/mod.rs
  ```

- [x] Define constants with WHAT/WHY docs:

  ```rust
  pub(crate) const JS_SAFE_INTEGER_MAX: i64 = 9_007_199_254_740_991;
  pub(crate) const JS_SAFE_INTEGER_MIN: i64 = -9_007_199_254_740_991;
  ```

- [x] Add helper functions if they improve readability:

  ```rust
  pub(crate) fn int_is_alpha_runtime_safe(value: i64) -> bool;
  ```

  `float_is_alpha_runtime_safe_integer` was not added because `FloatToInt` remains clearer when it truncates once, then checks the truncated `i64` through `int_is_alpha_runtime_safe`.

- [x] Keep comments precise:
  - This is an explicit cast materialization policy.
  - This is driven by the current Alpha JS runtime target.
  - Full-width `Int` runtime representation remains separate future work.

#### Rust-side policy update

- [x] Update `src/compiler_frontend/builtins/casts/policies.rs`.
- [x] Change `FloatToInt`:
  - reject non-finite values as today;
  - truncate toward zero as today;
  - reject truncated values outside the JS-safe integer range with `FloatCastToIntOutOfRange`.
- [x] Change `StringToInt`:
  - keep strict trimmed base-10 parsing with optional sign;
  - keep underscore text rejected;
  - after successful parse, reject values outside JS-safe integer range with `IntParseOutOfRange`.
- [x] Rename test descriptions and comments that still imply full `i64` range is the accepted runtime cast range.
- [x] Keep error code usage stable:
  - invalid syntax -> `IntParseInvalidFormat`;
  - out of cast materialization range -> `IntParseOutOfRange`;
  - non-finite float -> `FloatCastToIntInvalidValue`;
  - finite but unsafe/out of range float -> `FloatCastToIntOutOfRange`.

#### JS runtime helper update

- [x] Update `src/backends/js/runtime/casts.rs`.
- [x] Emit helper constants or helper predicates from Rust constants rather than hand-duplicating magic values:

  ```js
  const __BS_INT_CAST_MIN = -9007199254740991;
  const __BS_INT_CAST_MAX = 9007199254740991;
  ```

- [x] Replace repeated `Number.isSafeInteger(...)` checks with one emitted helper only if it improves readability:

  ```js
  function __bs_cast_int_in_range(value) {
      return Number.isInteger(value)
          && value >= __BS_INT_CAST_MIN
          && value <= __BS_INT_CAST_MAX;
  }
  ```

- [x] Use the same helper in `__bs_cast_int` and `__bs_cast_float_to_int`.
- [x] Ensure runtime errors use the same `BuiltinErrorCode` paths already used today.

#### Tests

- [x] Update `src/compiler_frontend/builtins/casts/tests/policies_tests.rs`:
  - `String -> Int` accepts `9007199254740991`;
  - `String -> Int` rejects `9007199254740992`;
  - `String -> Int` accepts `-9007199254740991`;
  - `String -> Int` rejects `-9007199254740992`;
  - `Float -> Int` accepts `9007199254740991.0`;
  - `Float -> Int` rejects the next unsafe integer boundary if representable in the chosen test form.
- [x] Add integration cases:
  - `cast_int_safe_integer_boundary_const_success`
  - `cast_int_safe_integer_boundary_const_rejected`
  - `cast_int_safe_integer_boundary_runtime_success`
  - `cast_int_safe_integer_boundary_runtime_catch`
- [x] Ensure the runtime and const cases prove the same boundary behavior.
- [x] Use stable `diagnostic_codes` in rejection fixtures.
- [x] Add JS output assertions for success/fallback behavior where possible.

### Phase 1 audit / style / validation

- [x] Run:

  ```bash
  cargo test -p beanstalk -- cast
  cargo run -- tests cast_int_safe_integer_boundary
  just validate
  ```

  Adjust commands to the repo’s actual test invocation if package names differ.

  Validation run:
  - `cargo test --quiet -- cast` passed with 98 tests.
  - `cargo run --quiet -- tests` passed with 1544 / 1544 expected integration outcomes. The integration runner does not accept the originally listed positional case filter, so the full suite covered the new safe-integer cases.
  - `just validate` passed after parent review and plan/progress updates.

- [x] Manual stage-boundary review:
  - policy decisions remain in `builtins/casts`;
  - AST folding uses policies and does not duplicate range checks;
  - JS runtime mirrors policy and does not silently widen/narrow behavior;
  - no user-facing cast failure routes through `CompilerError`.

- [x] Confirm no new `#[allow(dead_code)]` or `#[allow(clippy::too_many_arguments)]`.
- [x] Confirm comments explain the Alpha JS-safe integer decision.

---

## Phase 2 — Clarify HIR/user-defined cast contract and optional target recovery

Status: complete.

Summary:

- Updated compiler and language docs to clarify that AST owns cast target/evidence/fallibility decisions and optional target wrapping flags before HIR.
- Clarified HIR source comments: builtin runtime casts remain as `HirExpressionKind::Cast` / `HirStatementKind::CastOp`, user-defined evidence lowers to direct user-function calls, and generic-bound evidence is validation-only.
- Added optional cast integration coverage for ordinary optional wrapping, catch recovery producing inner `T`, and `then none` rejection.
- Fixed the optional catch recovery HIR bug found by the new tests: `T? = cast ... catch:` now merges inner `T` values from both success and recovery paths, then wraps the merged result once into `T?`.
- Removed the stale fallible-carrier success-transform helper left behind by that fix.
- Updated `docs/src/docs/progress/#page.bst` with optional cast catch recovery coverage and semantics.

### Context

The current HIR implementation preserves explicit runtime cast operations for builtin evidence, but lowers user-defined evidence to direct user-function calls. This is a good post-hardening simplification because HIR does not carry trait evidence or solve dispatch.

Optional receiving targets also need explicit documentation and tests: `T?` targets cast to inner `T`; a `catch` recovery handler produces `T`, and the final result is wrapped to `T?`.

### Tasks

#### HIR contract documentation

- [x] Update `docs/compiler-design-overview.md`:
  - AST resolves all cast targets, evidence, and fallibility.
  - AST folds builtin const casts before HIR where possible.
  - HIR carries explicit builtin runtime casts:
    - `HirExpressionKind::Cast { source, policy }`
    - `HirStatementKind::CastOp { policy, source, result }`
  - User-defined cast evidence lowers to a direct user-function call during HIR lowering.
  - `ResolvedCastEvidence::GenericBound` is validation-only and must not reach HIR.
- [x] Update comments in:
  - `src/compiler_frontend/hir/expressions.rs`
  - `src/compiler_frontend/hir/statements.rs`
  - `src/compiler_frontend/hir/hir_expression.rs`
- [x] Remove any wording that implies HIR carries user-defined trait evidence for runtime casts.

#### Optional target recovery docs and tests

- [x] Update `docs/language-overview.md`:
  - In `T?` receiving contexts, `cast` targets the inner builtin `T`.
  - `cast ... catch:` recovery handlers also produce inner `T`.
  - The compiler wraps the successful or recovered `T` into `T?` after the cast.
  - `then none` is invalid for `target T? = cast source catch:` unless the cast target itself is `None`-like, which is outside this cast target set.
- [x] Update `docs/src/docs/language-overview/#page.bst` with the same user-facing rule.
- [x] Add integration tests:
  - `cast_optional_catch_inner_value_success`
  - `cast_optional_catch_none_rejected`
  - `cast_optional_success_wraps_inner_value`
- [x] Add unit coverage only if an existing AST cast boundary test can directly inspect the optional target flag without duplicating integration behavior.
  - No unit test was added; the existing boundary tests would only duplicate the integration-visible behavior for this slice.
- [x] Ensure the invalid case asserts stable diagnostic codes.

### Phase 2 audit / style / validation

- [x] Run focused tests for optional cast cases.
  - `cargo test --quiet -- cast` passed with 98 tests.
  - `cargo run --quiet -- tests` passed with 1549 / 1549 expected integration outcomes after parent review.
- [x] Run `just validate`.
  - `just validate` passed after parent review and plan/progress updates.
- [x] Manual stage-boundary review:
  - HIR docs match actual code;
  - no trait evidence is introduced into HIR;
  - optional wrapping remains a HIR lowering step or explicit AST coercion step, not a new cast target.
- [x] Confirm generated docs are updated if the repo commits `docs/release/**`.
  - No generated `docs/release/**` files were updated in this slice; validation used the docs check path.

---

## Phase 3 — Replace long expression parser argument lists with an input struct

Status: complete.

Summary:

- Added `parse_expression_input.rs` with `ExpressionParseInput`, `ExpressionParseResources`, and named parse-mode constructors for ordinary, nested/no-catch, bounded, grouped, and fully custom expression parsing.
- Converted the central expression parser and bounded expression parser to consume `ExpressionParseInput`.
- Removed the cast-specific wrapper entrypoints and the old bounded-expression policy wrapper path.
- Updated expression, statement, value-production, collection/map, function-call, and parser tests to construct explicit parse inputs at their owned receiving boundaries.
- Removed all `too_many_arguments` suppressions under AST expression parsing, including the call-argument pre-routing helper by replacing it with a named request struct.
- Preserved behavior: grouped expressions still erase cast targets, `cast (left + right)` still parses, typed receiver boundaries still supply cast targets, and nested/non-boundary contexts still reject `catch` and expected-result evidence where they did before.

### Context

Cast target context extended an already-noisy parser API. The style guide prefers context structs over long argument lists. This phase reduces parser fragility before future expression-boundary work adds more state.

### Tasks

#### Add an expression parser input type

- [x] Add a new file:

  ```text
  src/compiler_frontend/ast/expressions/parse_expression_input.rs
  ```

- [x] Register it in the expressions module map.
- [x] Define a named input/context struct:

  ```rust
  pub(crate) struct ExpressionParseInput<'a, 'env> {
      pub(crate) token_stream: &'a mut FileTokens,
      pub(crate) scope_context: &'a ScopeContext,
      pub(crate) type_interner: &'a mut AstTypeInterner<'env>,
      pub(crate) expected_type: &'a mut ExpectedType,
      pub(crate) cast_target_context: &'a mut CastTargetContext,
      pub(crate) value_mode: &'a ValueMode,
      pub(crate) trailing_policy: ExpressionTrailingPolicy,
      pub(crate) string_table: &'a mut StringTable,
  }
  ```

- [x] Add small named constructors or builder helpers only where they are the current API, not compatibility shims:
  - `ordinary(...)`
  - `without_boundary_catch(...)`
  - `until(...)`
  - `grouped_without_cast_target(...)`
- [x] Include WHAT/WHY docs explaining:
  - `ExpectedType` is for context-sensitive literals;
  - `CastTargetContext` is for explicit cast target boundaries;
  - they intentionally stay separate.

#### Replace wrapper-heavy functions

- [x] Convert `create_expression_with_trailing_newline_policy` into the central implementation that accepts `ExpressionParseInput`.
- [x] Replace old wrappers:
  - `create_expression_with_cast_target`
  - `create_expression_without_boundary_catch_with_cast_target`
  - `create_expression_until_with_cast_target`
  - any other added cast-specific wrappers.
- [x] Update call sites in:
  - `declarations.rs`
  - `collections.rs`
  - `body_return.rs`
  - `functions.rs`
  - `function_calls.rs`
  - `struct_instance.rs`
  - `choice_constructor.rs`
  - `mutation.rs`
  - value-production parsers
  - receiver/field access call argument parsers.
- [x] Delete compatibility wrappers in the same phase.
- [x] Remove `#[allow(clippy::too_many_arguments)]` related to expression parse entrypoints.
- [x] Keep function names self-describing and avoid broad tuple returns.

#### Keep parser behavior unchanged

- [x] Confirm grouped expressions still erase cast target context so `(cast value)` stays invalid as an operator operand.
- [x] Confirm `cast (left + right)` still parses correctly.
- [x] Confirm function arguments, struct/choice fields, returns, assignments, collections, and maps still provide explicit cast targets.
- [x] Confirm conditions, loops, assertions, templates, untyped declarations, and inferred maps/collections still do not provide cast targets.

### Phase 3 audit / style / validation

- [x] Run parser-focused unit tests.
  - `cargo test --quiet -- expression` passed with 204 tests.
- [x] Run cast boundary tests.
  - `cargo test --quiet -- cast` passed with 98 tests.
- [x] Run full integration tests touching declarations, returns, function arguments, collection/map literals, and value-producing blocks.
  - `cargo run --quiet -- tests` passed with 1549 / 1549 expected outcomes.
- [x] Run `just validate`.
  - `just validate` passed after parent review and corrections. It completed clippy for native/linux/windows targets, 2366 unit tests, 1549 integration outcomes, docs check, and benchmark check with no measurable change.
- [x] Manual style review:
  - no compatibility wrappers;
  - no `too_many_arguments` allowances;
  - no clever builder API that hides stage ownership;
  - comments explain parser target-context separation.

---

## Phase 4 — Consolidate cast metadata and JS helper emission

Status: complete.

Summary:

- Made `BUILTIN_CAST_TRAIT_ROWS` the single authoritative core cast trait metadata table.
- Removed the duplicate `CORE_CAST_TRAIT_KINDS` table, redundant metadata projection helpers, `CastEvidenceKind`, and `builtin_evidence_kind()`.
- Updated cast evidence registration, user-defined/generic evidence selection, and trait incompatibility registration to derive from the row table.
- Updated metadata tests to assert 12 unique rows, unique names, and complete target/fallibility coverage directly from the row table.
- Made JS runtime cast helper emission demand-driven: `String -> Int`, `String -> Float`, `Float -> Int`, and other policy helpers emit only when their policies are reachable; `Int -> Float` emits no helper.
- Added backend unit coverage and integration cases for helper presence/absence, and regenerated affected HTML goldens to remove the unconditional numeric helper block.

### Context

The cast implementation has strong table-driven foundations, but a few redundant metadata paths and unconditional helper emissions add avoidable noise.

### Tasks

#### Collapse core cast trait metadata to one row table

- [x] In `src/compiler_frontend/builtins/casts/traits.rs`, make `BUILTIN_CAST_TRAIT_ROWS` the only authoritative source.
- [x] Remove `CORE_CAST_TRAIT_KINDS`.
- [x] Replace `core_cast_trait_kinds()` with one of:
  - `core_cast_trait_rows() -> &'static [CoreCastTraitMetadata]`, or
  - `core_cast_trait_kinds()` derived from rows without a second hardcoded list.
- [x] Update callers:
  - `src/compiler_frontend/builtins/casts/evidence.rs`
  - `src/compiler_frontend/builtins/casts/resolution.rs`
  - AST environment cast trait registration paths.
- [x] Update tests to validate row table completeness and uniqueness:
  - exactly 12 rows;
  - unique `CoreCastTrait`;
  - unique trait names;
  - target/fallibility pairs cover all builtin targets with infallible/fallible rows.

#### Remove redundant/unused metadata

- [x] Remove `CastEvidenceKind` from `targets.rs` unless there is a real production caller after the refactor.
- [x] Remove `builtin_evidence_kind()` if it exists only to pin a redundant enum.
- [x] Remove dead-code projection helpers used only by tests, or gate test-only helpers with `#[cfg(test)]`.
- [x] Replace stale comments such as “phase 2 evidence table” with current ownership comments such as “initial builtin evidence table.”

#### Make JS cast helper emission fully on-demand

- [x] Update `src/backends/js/runtime/casts.rs`:
  - emit `__bs_cast_int` only when `StringToInt` is used;
  - emit `__bs_cast_float` only when `StringToFloat` is used;
  - emit numeric normalization helpers only when one of those helpers is emitted;
  - keep fixed helper emission order deterministic.
- [x] Verify `IntToFloat` emits no helper.
- [x] Verify `CastOp` with policies that require helpers cannot reach JS emission without that helper in `used_cast_policies`.
- [x] Add unit-level backend tests only if an existing JS backend helper test owner exists. Otherwise add integration coverage with generated output assertions:
  - no `__bs_cast_int` helper when only `Int -> Float` is used;
  - `__bs_cast_int` appears when `String -> Int` runtime cast is used;
  - `__bs_cast_float` appears when `String -> Float` runtime cast is used.

### Phase 4 audit / style / validation

- [x] Run cast metadata tests.
  - `cargo test --quiet -- cast` passed with 99 tests.
- [x] Run JS backend tests/goldens affected by helper emission.
  - `cargo test --quiet -- prelude` passed with 18 tests.
  - `cargo test --quiet -- runtime_helpers` passed with 57 tests.
  - `cargo run --quiet -- tests` passed with 1553 / 1553 expected integration outcomes.
- [x] Run `just validate`.
  - `just validate` passed. It completed clippy for native/linux/windows targets, 2367 unit tests, 1553 integration outcomes, docs check, and benchmark check with no measurable change.
- [x] Manual style review:
  - one metadata source of truth;
  - no stale phase comments;
  - no unnecessary dead-code allowances;
  - JS helper emission remains deterministic and readable.

---

## Phase 5 — Test coverage pruning and parity expansion

Status: complete.

Summary:

- Audited Phase 1-4 cast coverage and avoided duplicating already-covered safe-integer boundaries, optional target recovery, `then none` rejection, and HTML-Wasm runtime-cast rejection cases.
- Added `cast_string_to_float_demand_helpers` to complete the demand-driven JS helper integration matrix: `String -> Float` emits `__bs_cast_float` and `__bs_normalize_numeric_text` without emitting the `String -> Int` helpers.
- Pruned duplicate scalar-constructor removal fixtures that asserted the same `BST-RULE-0046` removed-constructor diagnostic path as retained canonical fixtures.
- Retained one scalar-constructor removal fixture per distinct scalar token path: `Int`, `Float`, `Bool`, `String`, and `Char`.
- Updated `tests/cases/manifest.toml` for the added and removed fixtures.

### Context

The initial implementation added broad coverage. This phase keeps the suite useful by adding missing edge cases and pruning redundant old fixtures that no longer protect distinct behavior.

### Tasks

#### Add missing edge coverage

- [x] Add or update unit tests:
  - safe-integer cast boundaries in `policies_tests.rs`;
  - core cast row uniqueness/completeness after metadata consolidation;
  - const-folding behavior for safe-integer boundaries if not covered by integration tests.
  - No new unit tests were needed in this slice: `policies_tests.rs` already covered safe-integer policy boundaries, Phase 4 metadata tests covered core row uniqueness/completeness, and const boundary behavior is covered by end-to-end const fixtures.
- [x] Add integration cases:
  - safe integer const/runtime parity;
  - optional target catch recovery;
  - fully on-demand JS helper emission if practical;
  - optional `then none` rejection.
  - Added the missing `String -> Float` helper-emission fixture; earlier phases already supplied the safe-integer, optional-cast, and `String -> Int` / `Int -> Float` helper-emission cases.
- [x] Add HTML-Wasm regression coverage only if reachable runtime casts are not already tested with an unsupported-backend diagnostic.
  - Existing runtime cast fixtures already assert the HTML-Wasm unsupported-backend diagnostic, so no duplicate Wasm case was added.

#### Prune redundant fixtures

- [x] Audit scalar constructor removal cases:
  - `scalar_int_constructor_removed`
  - `scalar_float_constructor_removed`
  - `scalar_*_constructor_removed_with_catch`
  - `cast_scalar_bool_constructor_removed`
  - `cast_scalar_char_constructor_removed`
  - `cast_scalar_string_constructor_removed`
  - any other `cast_scalar_*_constructor_removed`.
- [x] Keep one canonical diagnostic fixture per distinct parser path:
  - old `Int(...)` and `Float(...)` legacy path if those have historical catch/fallible behavior;
  - `Bool(...)`, `String(...)`, `Char(...)` removal path if they exercise newly reserved scalar constructor tokens.
- [x] Remove duplicate fixtures that assert the same token-path, same diagnostic code, and same surface.
  - Removed `scalar_int_constructor_removed_with_catch`, `scalar_float_constructor_removed_with_catch`, and `constant_fold_scalar_constructor_removed`; the removed-constructor diagnostic is emitted before catch or const-folding semantics are relevant.
- [x] Update `tests/cases/manifest.toml` after pruning or adding cases.
- [x] Prefer stable `diagnostic_codes` in new negative tests.
- [x] Avoid fragile rendered-output checks unless message rendering itself is under test.

### Phase 5 audit / style / validation

- [x] Run integration test runner for affected cases.
  - `cargo run --quiet -- tests` passed with 1552 / 1552 expected integration outcomes.
- [x] Run `just validate`.
  - `just validate` passed. It completed clippy for native/linux/windows targets, 2367 unit tests, 1552 integration outcomes, docs check, and benchmark check with no measurable change.
- [x] Confirm coverage improved without duplicate fixture bloat.
- [x] Confirm removed test fixtures are deleted from both filesystem and manifest.

---

## Phase 6 — Documentation, roadmap, and progress matrix updates

Status: complete.

Summary:

- Added the Alpha JS-safe integer cast policy to the compiler-facing and user-facing language docs.
- Tightened compiler design wording so AST owns builtin/user/generic cast evidence selection, HIR carries only builtin runtime cast policies, user-defined evidence lowers as direct calls, and generic-bound evidence remains validation-only.
- Updated the progress matrix with demand-driven JS helper coverage, safe-integer coverage, optional target recovery, and the explicit `Float -> String` formatting parity watch point.
- Fixed the stale removed cast-plan roadmap link, kept the active cleanup plan linked, and added roadmap follow-ups for `Float -> String` formatting parity and full-width `Int` runtime semantics beyond the Alpha JS-safe cast policy.
- Regenerated tracked `docs/release/**` output. The generated diff includes the edited language/progress pages and removes the now-unused cast helper prelude from docs pages after demand-driven helper emission.

### Context

The implementation is mostly documented, but follow-up work must be visible in roadmap/progress. The roadmap also contains a stale link to a removed cast plan file.

### Tasks

#### Language and compiler docs

- [x] Update `docs/language-overview.md`:
  - document safe-integer Alpha cast policy for `String -> Int` and `Float -> Int`;
  - document optional target catch recovery producing inner `T`;
  - keep user-defined targets outside scope;
  - do not suggest scalar constructors.
- [x] Update `docs/src/docs/language-overview/#page.bst` with equivalent user-facing wording.
- [x] Update `docs/compiler-design-overview.md`:
  - document that HIR carries only builtin runtime casts;
  - user-defined cast evidence lowers to direct calls during HIR lowering;
  - generic-bound cast evidence is AST validation-only.
- [x] Regenerate `docs/release/**` if generated docs are committed for docs-site changes.

#### Roadmap

- [x] Update `docs/roadmap/roadmap.md`.
- [x] Remove or fix the stale reference to:

  ```text
  docs/roadmap/plans/cast_operator_implementation_plan.md
  ```

  because that file no longer exists.
- [x] Add explicit roadmap item:

  ```text
  - Float-to-string cast parity: define one formatting contract shared by AST folding and JS/Wasm/runtime lowering for `Float -> String`, including exponent thresholds, signed zero, non-finite rejection/formatting policy, and backend-stable output tests.
  ```

- [x] Add optional related roadmap item if this plan chooses JS-safe integer cast policy:

  ```text
  - Full-width `Int` runtime semantics beyond the Alpha JS-safe integer cast policy: decide whether JS uses BigInt/boxed integers or whether `Int` remains a portable safe-integer type for the JS target.
  ```

#### Progress matrix

- [x] Update `docs/src/docs/progress/#page.bst`:
  - mark `Float -> String` parity as an explicit watch point/follow-up;
  - document safe-integer cast policy for `Int` casts;
  - update coverage text after new tests are added;
  - remove any mention that implies float formatting parity is already solved.
- [x] Regenerate `docs/release/docs/progress/index.html` if docs release output is committed.

### Phase 6 audit / style / validation

- [x] Run docs/site generation command used by the repo, or full:

  ```bash
  just validate
  ```

- [x] Manually inspect generated docs diff for accidental broad churn.
  - Generated HTML changes are broad because the demand-driven JS cast helper emission now removes unused cast helper prelude code from docs pages. The content changes are limited to the language overview and progress matrix pages.
- [x] Confirm roadmap and progress matrix do not reference removed files.
- [x] Confirm float-to-string parity appears explicitly in both roadmap and progress matrix.

Validation run:

- `cargo run build docs --release` passed and rebuilt 31 docs files.
- `cargo run --quiet -- check docs` passed with no errors or warnings.
- `just validate` passed. It completed clippy for native/linux/windows targets, 2367 unit tests, 1552 integration outcomes, docs check, and benchmark check with no measurable change.

---

## Phase 7 — Final audit, validation, and handoff

### Context

This phase catches stage-boundary regressions, stale wrappers, and documentation drift before merging.

### Tasks

- [ ] Run full validation:

  ```bash
  just validate
  ```

- [ ] Run targeted searches:

  ```bash
  rg "BuiltinCastKind|BuiltinCast|cast_operator_implementation_plan.md" src docs tests
  rg "phase 2 evidence|phase 3|phase 4" src/compiler_frontend/builtins/casts docs
  rg "too_many_arguments" src/compiler_frontend/ast/expressions
  rg "CastEvidenceKind|CORE_CAST_TRAIT_KINDS" src/compiler_frontend
  ```

- [ ] Confirm any remaining matches are intentional tests or roadmap references.
- [ ] Confirm all new diagnostics use `CompilerDiagnostic`.
- [ ] Confirm all new HIR/backend invariant failures use `CompilerError`.
- [ ] Confirm no user-facing source error is introduced in HIR/backend lowering when AST can diagnose it earlier.
- [ ] Confirm new tests are behavior-focused and not brittle implementation snapshots.
- [ ] Confirm docs and generated docs are in sync.

### Final manual stage-boundary review checklist

- [ ] AST owns target resolution, evidence selection, fallibility validation, and const folding.
- [ ] HIR does not solve trait evidence.
- [ ] HIR only carries builtin runtime cast policies.
- [ ] User-defined cast evidence lowers to direct calls.
- [ ] JS runtime helpers mirror Rust policy semantics.
- [ ] HTML-Wasm rejects reachable runtime casts before LIR lowering.
- [ ] Float-to-string parity is tracked as follow-up, not silently claimed complete.
- [ ] Parser API is cleaner than before and does not preserve obsolete wrappers.
- [ ] No new cast targets, generic cast traits, or conversion constructors were added.

## Acceptance criteria

- [ ] Folded and runtime `String -> Int` / `Float -> Int` casts agree at the documented integer range boundary.
- [ ] Optional receiving target recovery behavior is documented and tested.
- [ ] HIR/user-defined cast contract is documented accurately.
- [ ] Expression parser cast-target plumbing uses a context/input struct instead of long argument lists.
- [ ] Redundant cast metadata tables are consolidated.
- [ ] JS cast helpers are emitted on demand.
- [ ] Stale cast plan references are removed or replaced.
- [ ] Float-to-string parity is explicitly listed in roadmap and progress matrix.
- [ ] Duplicate or obsolete cast fixtures are pruned.
- [ ] `just validate` passes.
