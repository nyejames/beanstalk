# Beanstalk `Number` / numeric semantics implementation plan

**Target repository:** `nyejames/beanstalk`  
**Active branch context:** templates/TIR refactor line, with AST-local TIR merged into the frontend design docs  
**Plan path:** `docs/roadmap/plans/number-type-numeric-semantics-implementation-plan.md`  
**Primary goal:** implement the unified `Number` / `NumberN` high-precision numeric family through frontend, AST const folding, HIR, backend validation, and HTML-JS lowering; scaffold `Byte` in frontend/HIR with backend/runtime support deliberately deferred; add the first conservative numeric check-elision pass.

---

## Active context capsule

ACTIVE_PLAN:
- `docs/roadmap/plans/number-type-numeric-semantics-implementation-plan.md`

CURRENT_SLICE:
- Phase: 0
- Checklist item: Land this plan and update roadmap/progress placeholders.
- Goal: Make the implementation direction discoverable before code changes.
- Non-goals: No compiler behavior changes, no Wasm implementation, no Byte runtime implementation.

LAST_GOOD_COMMIT:
- `none`

CURRENT_WORKTREE_STATE:
- Clean / known changes: unknown until refreshed with `git status -sb`
- Branch: templates/TIR refactor branch or descendant; refresh with `git branch --show-current`
- Dedicated worker worktrees: none recorded

RELEVANT_DOCS_THIS_SLICE:
- `AGENTS.md` if present
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`
- `docs/roadmap/roadmap.md`

RELEVANT_CODE:
- `Cargo.toml`: add arbitrary integer dependencies only when compiler-side `NumberValue` is implemented
- `src/compiler_frontend/datatypes/`: canonical `TypeId`, builtin keys, `NumberScale`, `Byte`, and type display
- `src/compiler_frontend/ast/type_resolution/`: `NumberN` and `Byte` type-name resolution
- `src/compiler_frontend/numeric_text/`: existing grammar owner; reused, not expanded into type/runtime semantics
- `src/compiler_frontend/numeric_values/`: new pure exact `Number` semantic owner
- `src/compiler_frontend/ast/const_eval/`: const `Number` folding and cast folding
- `src/compiler_frontend/ast/templates/` and `src/compiler_frontend/ast/templates/tir/`: template value-to-string integration; no new Number-specific TIR node model
- `src/compiler_frontend/builtins/casts/`: dynamic scale-aware Number cast policy
- `src/compiler_frontend/hir/numeric.rs`: refactor to domain/operator numeric op shape
- `src/compiler_frontend/hir/reachability.rs`: keep syntactic reachability; add type-gated backend scan only where needed
- `src/compiler_frontend/analysis/numeric/`: new side-table optimization facts
- `src/backends/backend_feature_validation.rs`: Wasm Number and Byte target gates; JS Byte target gate
- `src/backends/js/`: Number runtime helpers, casts, string formatting, and numeric op lowering
- `tests/cases/manifest.toml`: integration case registration

ACCEPTANCE_CRITERIA:
- `Number` / `NumberN` resolves, folds, lowers, and runs on HTML-JS.
- `Number` runtime failures follow existing `NumericFailureMode` rules.
- `Number` is rejected cleanly for Wasm until the Wasm backend plan implements parity.
- `Byte` resolves and literal-validates in frontend/HIR, but runtime backend use is rejected cleanly.
- `HirNumericOp` no longer uses domain-specific enum variants such as `IntAdd` / `FloatAdd`.
- Numeric check elision exists for proven-safe trap-mode `Int` operations in JS only.
- Docs and progress matrix distinguish implemented, partial, deferred, and outside-scope surfaces.
- `just validate` passes at phase checkpoints.

DECISIONS_ALREADY_MADE:
- decision: `Number` replaces separate public `BigInt` and `Decimal`.
  reason: one compiler-owned high-precision family is simpler and avoids a broader numeric tower.
  source/user/date: user agreement, 2026-07-05
- decision: `Number` is `Number0`; `NumberN` is fixed-scale arbitrary-precision decimal with `N` decimal places.
  reason: fixed scale matches the desired arbitrary whole-number range and predictable decimal precision.
  source/user/date: user agreement, 2026-07-05
- decision: rounding happens after every source/HIR numeric operation result.
  reason: prevents context-sensitive behavior caused by hidden extended-precision temporaries.
  source/user/date: user agreement, 2026-07-05
- decision: mixed `Number` scales are different types; explicit casts are required.
  reason: scale changes are semantic and should not be inferred.
  source/user/date: user agreement, 2026-07-05
- decision: `Int` may participate with `NumberN` operators through exact implicit scaling.
  reason: exact conversion is ergonomic and does not hide rounding or precision loss.
  source/user/date: user agreement, 2026-07-05
- decision: `Float <-> NumberN` lossy conversions are deferred named helpers, not `cast` policies.
  reason: binary-float decimal conversion needs explicit user intent.
  source/user/date: user agreement, 2026-07-05
- decision: implicit entry `start()` remains infallible.
  reason: preserves the current HTML runtime-fragment model and avoids builder-owned error-fragment policy.
  source/user/date: user correction, 2026-07-05
- decision: `Byte` is scaffolded in frontend/HIR only; backend/runtime support is deferred.
  reason: useful future ABI/storage type, but not part of this runtime implementation slice.
  source/user/date: user agreement, 2026-07-05

BLOCKERS / RISKS:
- The TIR branch is the relevant frontend shape; refresh `docs/compiler-design-overview.md` and `src/compiler_frontend/ast/templates/tir/` before any template-related slice.
- Current GitHub default branch may lag the templates/TIR branch. Prefer the active worktree over default-branch assumptions.
- Dynamic Number casts can make the existing static cast table noisy if implemented as scale-pair enumeration.
- Runtime `Byte` can leak into JS/Wasm lowerers unless backend validation is added before any Byte HIR value reaches lowering.
- Numeric analysis should not be added to the compiled module handoff until more than one backend consumes it.

VALIDATION_STATE:
- last command: none
- result: not run
- known unrelated failures: none recorded

DOCS_IMPACT:
- progress matrix needed: yes; add/adjust `Number`, numeric check elision, `Byte`, Number follow-ups, and Wasm deferral rows
- other docs stale: `docs/language-overview.md`, `docs/compiler-design-overview.md`, `docs/roadmap/roadmap.md`
- authorized docs updates: this plan authorizes updates to the files listed above; user-facing tutorial pages are optional and should not block implementation

NEXT_ACTION:
- Refresh worktree state, commit hash, and current TIR files; update this capsule; then complete Phase 0 documentation changes.

---

## 1. Implementation principles

- Keep the language surface small: operators remain compiler-owned; no operator overloading; no user-authored numeric trait system.
- Keep `Int` and `Float` behavior intact: `Int` remains checked signed `i32`; `Float` remains finite `f64`.
- Keep tokenization lexical: `NumberN` validation belongs in type-reference parsing/resolution, not tokenizer logic.
- Keep TIR behavior-preserving: `Number` formatting must use a shared value-to-string path consumed by TIR and legacy template fallback, not new Number-specific template IR nodes.
- Keep HIR backend-neutral: HIR carries numeric domain/operator facts and typed values, not source spelling or backend helper names.
- Keep user diagnostics typed: source/type/rule/backend-target failures use `CompilerDiagnostic`; `CompilerError` remains for internal invariants and infrastructure.
- Keep optimization facts side-table-only: numeric check elision must not mutate HIR and must not produce diagnostics.
- Keep backend rollout explicit: HTML-JS implements `Number`; Wasm rejects reachable `Number`; both JS and Wasm reject reachable runtime `Byte` until a later Byte runtime plan.

---

## 2. Final `Number` and `Byte` semantics

### 2.1 Type family

| Spelling | Meaning | Canonical display |
|---|---|---|
| `Number` | fixed scale 0; arbitrary-precision integer | `Number` |
| `Number0` | accepted alias of scale 0 | `Number` |
| `Number1` ... `Number256` | fixed-scale arbitrary-precision decimal | `NumberN` |
| `Number01` | invalid leading-zero scale | diagnostic |
| `Number257+` | invalid over `MAX_NUMBER_SCALE` | diagnostic |
| `Byte` | unsigned 8-bit integer, `0..255` | `Byte` |

`MAX_NUMBER_SCALE = 256` for Alpha.

### 2.2 Runtime value model

`NumberN` represents:

```text
semantic value = scaled_integer / 10^N
```

Compiler-side value:

```rust
pub(crate) struct NumberValue {
    pub(crate) scaled_integer: BigInt,
    pub(crate) scale: NumberScale,
}
```

HTML-JS runtime value:

```javascript
Object.freeze({ __bs_number: true, scale, value }) // value is a signed BigInt
```

### 2.3 Operators

| Operation | `Number` / `Number0` | `NumberN`, `N > 0` |
|---|---|---|
| `+` | exact | exact |
| `-` | exact | exact |
| `*` | exact | exact internal result, then round to scale `N` |
| `/` | invalid; use `//`, `%`, or cast to positive-scale `NumberN` | checked divide-by-zero; round to scale `N` |
| `//` | integer division, truncating toward zero | invalid initially |
| `%` | integer remainder | checked decimal remainder, result `NumberN` |
| unary `-` | exact | exact |
| `^ Int` | non-negative exponent only | non-negative exponent only; round to scale `N` where needed |

Rounding mode: **round half to even**.

Rounding/canonicalization happens at every source/HIR numeric operation result boundary. A backend helper may use wider temporaries internally, but it must preserve the same observable rounding points.

### 2.4 Mixed operands

| Expression | Policy |
|---|---|
| `NumberN op NumberN` | valid when scales match |
| `NumberN op Int` / `Int op NumberN` | valid; exact implicit scaling of `Int` |
| `NumberN op NumberM`, `N != M` | invalid; explicit scale cast required |
| `NumberN op Float` / `Float op NumberN` | invalid; deferred lossy helper method required |
| `NumberN ^ Int` | valid |
| `NumberN ^ NumberN` | invalid |

### 2.5 Literals

- Whole-number literal to `Number` / `NumberN`: contextual, exact, infallible.
- Decimal literal to `NumberN`: contextual, exact only.
- `Number2 = 1.239`: diagnostic; no hidden literal rounding.
- Decimal/exponent literal to `Number`: valid only when exactly integral after grammar materialization.

### 2.6 Casts

| Conversion | Policy |
|---|---|
| `Int -> NumberN` | infallible exact cast; multiply by `10^N` |
| `NumberN -> Int` | fallible; discarded scale digits must be zero and value must fit `i32` |
| `NumberN -> NumberM`, `M > N` | infallible scale widening |
| `NumberN -> NumberM`, `M < N` | fallible exact scale narrowing; no rounding |
| `NumberN -> String` | infallible canonical decimal formatting |
| `String -> NumberN` | fallible exact parse using Beanstalk numeric text grammar |
| `Float -> NumberN` | deferred named helper/method |
| `NumberN -> Float` | deferred named helper/method |

`cast` is exact conversion. Rounding to narrower scale is a future named helper, not a cast.

### 2.7 Formatting

Default `NumberN -> String` and template interpolation use canonical decimal formatting:

- trim redundant fractional trailing zeroes;
- never emit exponent notation;
- never emit negative zero;
- emit a leading `0` before fractional values;
- fixed-width display such as `1.20` is deferred to a named helper.

### 2.8 `Byte` scaffold

- `Byte` type and contextual literal validation are in scope.
- `Byte` arithmetic is not in scope.
- Runtime backend representation, casts, formatting, collections/storage specialization, ABI, and binary buffers are deferred.
- Reachable runtime `Byte` values must be rejected before JS/Wasm lowering.

---

## 3. Current repository anchor

The current branch shape matters for this plan.

- AST construction now includes TIR: `templates/tir/` owns AST-local Template IR as a typed-ID store for parsed/finalized template nodes, summaries, wrapper sets, slot plans, and HIR handoff. TIR is behavior-preserving; legacy `TemplateContent` and `TemplateRenderPlan` remain fallbacks for unresolved slots or cross-store children.
- AST still owns type resolution, expression parsing, type checking, contextual coercion, cast resolution/folding, constant folding, template composition/folding, and TIR-based template representation.
- Tokenization owns numeric literal scanning and spacing diagnostics; `numeric_text/` owns grammar normalization and numeric text parsing helpers.
- `TypeEnvironment` is canonical semantic type identity. `DataType` is parse-only or diagnostic-only and must not drive executable AST/HIR semantic decisions.
- HIR owns explicit checked numeric effects as `HirStatementKind::NumericOp`, with `NumericFailureMode`, plus separate `FormatFloat` and `ValidateFloat` statements.
- HIR validation treats arithmetic that should have become `NumericOp` as an internal HIR shape error.
- Borrow validation already models later-stage facts as side tables keyed by HIR/value identity and does not mutate HIR.
- JS lowering currently has checked `Int`/`Float` helpers and demand-driven numeric helper emission.
- Wasm backend support remains experimental and target-gated through backend feature validation.

Implementation consequences:

- Do not add Number-specific TIR nodes.
- Do not duplicate template formatting in both TIR and legacy template paths. Add or extend one shared frontend value-to-string service that both paths can call.
- Do not add numeric analysis facts to the compiled module handoff for V1. JS is the only consumer; run the analysis inside JS lowering or the JS builder path until a second backend needs it.
- Prefer a reusable semantic backend-type scanner over new one-off reachability vectors for `Number` and `Byte` literals. Use HIR syntactic reachability to find reachable blocks, then inspect typed expressions/locals with `TypeEnvironment` predicates.

---

## 4. Complexity-reduction rules for this implementation

- Remove inactive `Decimal` naming and comments when introducing `Number`; do not keep `Decimal` aliases, shims, or compatibility paths.
- Use one exact semantic module for compiler-side Number behavior. Do not reimplement parsing, scale conversion, rounding, or formatting separately in const eval, cast policies, diagnostics, and tests.
- Keep `numeric_text` grammar-only. Put materialized exact decimal behavior in `numeric_values`.
- Use a single cast policy context struct for all builtin cast policy application. Do not maintain parallel “old scalar” and “new Number” call paths.
- Keep dynamic Number cast evidence scale-aware through source/target `TypeId`; do not enumerate all scale pairs.
- Prefer type predicates plus one backend-validation scanner for target-gated runtime types (`Number` in Wasm, `Byte` in JS/Wasm) instead of adding separate reachability lists for every future scalar.
- Split `src/backends/js/runtime/numeric.rs` into a module directory if adding Number helpers would make the file noisy:

```text
src/backends/js/runtime/numeric/
    mod.rs
    int_float.rs
    number.rs
    trap.rs
```

- Implement Number JS with the existing carrier contract first. Direct trap helpers are a separate phase and must not block Number correctness.
- Keep Byte deliberately small: type identity, contextual literal validation, HIR expression shape if needed, backend validation. No JS helper stubs.
- Use `Box<NumberValue>` in AST/HIR expression variants if storing `NumberValue` directly would bloat common expression enums. Avoid `Arc` unless profiling or clone churn justifies it.
- Avoid broad helper traits or macros. Use named structs/enums for states such as cast policy input, Number operation, rounding result, and backend target-gate reason.

---

## 5. Phase structure

Each phase must end with:

- [ ] `cargo fmt`
- [ ] `just validate`
- [ ] Style-guide review: stage ownership, clear module responsibility, no user-input panics, no stale comments, no compatibility wrappers
- [ ] Diagnostic review: user-facing failures use typed `CompilerDiagnostic`; internal invariants use `CompilerError`
- [ ] Test review: behavior in integration tests where possible; unit tests only for internal invariants, pure math, side-table facts, or backend policy
- [ ] Active context capsule refresh: phase, checklist item, commit, branch, worktree state, validation state, next action

If a slice cannot keep the repo compiling, split it smaller. Prefer vertical slices over temporary unsupported production states.

---

## Phase 0 — Planning docs and active context setup

**Context:** Make the new plan discoverable, update current-state tracking, and remove stale BigInt/Decimal roadmap direction before code changes begin.

- [ ] Add this file at `docs/roadmap/plans/number-type-numeric-semantics-implementation-plan.md`.
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] Replace separate BigInt/Decimal wording with unified `Number` / `NumberN`.
  - [ ] Add explicit Wasm deferral: Number runtime, checked numeric helper parity, Float formatting parity, Float boundary validation parity, and Byte runtime support belong to the later Wasm backend plan.
  - [ ] Add Byte follow-up: runtime representation, casts, JS/Wasm lowering, collection storage, ABI/export behavior, binary buffers, and formatting/parsing helpers.
  - [ ] Add Number follow-ups: lossy Float conversions, fixed-width formatting, non-default rounding helpers, scale-narrowing with rounding, broader numeric library APIs.
- [ ] Update `docs/src/docs/progress/#page.bst` with concise rows:
  - [ ] `Number / NumberN`: planned/partial, frontend+HIR+JS target, Wasm deferred.
  - [ ] `Numeric check elision`: partial, V1 Int trap-mode JS only.
  - [ ] `Byte`: partial, frontend/HIR scaffold only, backend/runtime deferred.
  - [ ] `Number follow-ups`: deferred, lossy conversions/formatting/rounding/Wasm parity.
- [ ] Update `docs/language-overview.md` with compiler-facing semantics only; avoid tutorial expansion.
- [ ] Update `docs/compiler-design-overview.md` only for stage ownership changes:
  - [ ] `NumberN` type names are resolved in AST type resolution, not tokenization.
  - [ ] `numeric_values` owns exact Number semantics.
  - [ ] TIR and legacy templates consume shared value-to-string formatting.
  - [ ] HIR numeric ops use domain/operator shape.
  - [ ] Numeric analysis is side-table optimization data.
- [ ] Do not update docs-site tutorial pages unless a later docs pass intentionally adds examples.

**Acceptance:** Documentation reflects the new direction and no code behavior changes are made.

---

## Phase 1 — Type identity, builtin names, and type-name diagnostics

**Context:** Establish semantic identity first. `NumberN` must be a canonical type, not a tokenizer feature or diagnostic-only spelling.

- [ ] Update `src/compiler_frontend/datatypes/ids.rs`:
  - [ ] Add `NumberScale(pub u16)` and `MAX_NUMBER_SCALE: u16 = 256`.
  - [ ] Replace inactive `Decimal` builtin key/constant with `Number` at the same builtin slot where practical.
  - [ ] Append `Byte` as a new builtin so existing stable IDs do not shift beyond the replaced inactive slot.
  - [ ] Add `BuiltinTypeConstructor::Number { scale: NumberScale }` for positive scales.
- [ ] Update `src/compiler_frontend/datatypes/environment.rs`:
  - [ ] Rename `BuiltinTypes.decimal` to `BuiltinTypes.number`.
  - [ ] Add `BuiltinTypes.byte`.
  - [ ] Add `intern_number(scale)`, `number_scale(type_id)`, `is_number_type(type_id)`, and `is_byte_type(type_id)`.
  - [ ] Canonicalize `NumberScale(0)` to `builtins.number`.
  - [ ] Intern positive-scale Number types as constructed builtin types with no type arguments.
- [ ] Update type display/diagnostic rendering:
  - [ ] Render scale 0 as `Number`.
  - [ ] Render positive scales as `NumberN`.
  - [ ] Render `Byte` as `Byte`.
- [ ] Update AST type-reference resolution:
  - [ ] Resolve `Number`, `Number0`, `Number1` ... `Number256`.
  - [ ] Reject leading-zero suffixes such as `Number01`.
  - [ ] Reject scales greater than `MAX_NUMBER_SCALE`.
  - [ ] Resolve `Byte`.
- [ ] Update name reservation/collision logic:
  - [ ] Reserve `Number`, `Number[0-9]+`, and `Byte` in type namespace.
  - [ ] Reject declarations, aliases, imports, and generic parameters that collide with these names.
- [ ] Remove or replace stale inactive Decimal comments. Do not leave `Decimal` compatibility APIs.
- [ ] Add tests for type resolution, display, scale bounds, leading-zero diagnostics, and namespace collisions.

**Acceptance:** `Number`/`NumberN`/`Byte` type names resolve or fail with typed diagnostics; no runtime values are introduced yet.

---

## Phase 2 — Pure compiler-side Number semantics

**Context:** Add one pure semantic owner for exact parsing, arithmetic, rounding, scale conversion, and formatting. Every later compiler stage and test should reuse this instead of duplicating math rules.

- [ ] Add dependencies when this phase starts:

```toml
num-bigint = "0.4"
num-traits = "0.2"
```

- [ ] Add `src/compiler_frontend/numeric_values/`:
  - [ ] `mod.rs`: owner map and boundary docs.
  - [ ] `number.rs`: `NumberValue` and invariants.
  - [ ] `parse.rs`: exact literal/string materialization from `numeric_text` grammar.
  - [ ] `format.rs`: canonical decimal formatter.
  - [ ] `rounding.rs`: round-half-to-even.
  - [ ] `ops.rs`: arithmetic and scale conversion.
  - [ ] `tests/`: pure unit tests.
- [ ] Add `NumberValue` with signed `BigInt` scaled integer and `NumberScale`.
- [ ] Add exact materialization helpers:
  - [ ] whole numbers;
  - [ ] decimals;
  - [ ] exponent forms when exactly representable at target scale;
  - [ ] signed text without unary plus;
  - [ ] failure for too many fractional digits or non-integral scale-zero values.
- [ ] Add canonical formatting:
  - [ ] trim fractional trailing zeroes;
  - [ ] no exponent notation;
  - [ ] no negative zero;
  - [ ] leading zero for fractional values.
- [ ] Add scale conversion:
  - [ ] infallible widening;
  - [ ] exact fallible narrowing;
  - [ ] no rounding in cast conversion.
- [ ] Add operations:
  - [ ] add/subtract/negate exact;
  - [ ] multiply/divide/power round to result scale where needed;
  - [ ] divide/modulo detect divide-by-zero;
  - [ ] scale-zero `//` truncates toward zero;
  - [ ] positive-scale `//` stays invalid at AST type-checking, not here.
- [ ] Add a small error enum that stays pure and stage-agnostic; map it to diagnostics or runtime error codes at call sites.
- [ ] Unit-test parsing, formatting, rounding ties, negative values, scale conversion, arithmetic, divide-by-zero, invalid exponent, and i32 range conversion.

**Acceptance:** Number semantics are fully tested without AST, HIR, template, cast, or backend dependencies.

---

## Phase 3 — AST literal values, contextual materialization, and shared value formatting

**Context:** AST can now represent Number and Byte values and materialize contextual literals. This phase must account for TIR: template formatting should be shared, not added separately to TIR and fallback paths.

- [ ] Add AST expression variants:
  - [ ] `ExpressionKind::Number(Box<NumberValue>)` or direct `NumberValue` if enum size remains acceptable.
  - [ ] `ExpressionKind::Byte(u8)`.
- [ ] Add expression constructors for Number and Byte.
- [ ] Mark Number and Byte foldability appropriately.
- [ ] Add contextual literal materialization:
  - [ ] `Number` / `NumberN` typed boundaries use `numeric_values` exact materialization.
  - [ ] `Byte` typed boundaries accept whole-number literals `0..255` only.
  - [ ] Default untyped whole literals remain `Int`.
  - [ ] Default untyped decimal/exponent literals remain `Float`.
- [ ] Add typed diagnostics for Number scale/literal failures and Byte out-of-range failures.
- [ ] Add or consolidate one shared compile-time value-to-string formatter used by:
  - [ ] const template folding;
  - [ ] TIR formatter view / TIR fold output where applicable;
  - [ ] legacy `TemplateContent` / `TemplateRenderPlan` fallback where applicable;
  - [ ] `cast` folding when converting `NumberN -> String` in later phases.
- [ ] Do not add Number-specific TIR node kinds.
- [ ] Do not add Byte template interpolation support unless Byte runtime/string policy is explicitly implemented. Prefer a clean deferred diagnostic for direct Byte string coercion.
- [ ] Add tests:
  - [ ] contextual Number literals;
  - [ ] contextual Byte literals;
  - [ ] const Number template interpolation through TIR-synced templates;
  - [ ] one fallback-template case if existing fixtures expose legacy fallback behavior;
  - [ ] diagnostics for inexact Number literals and Byte range failures.

**Acceptance:** AST can store Number/Byte literals, const Number interpolation formats canonically, and template paths share one formatter.

---

## Phase 4 — Scale-aware builtin cast policy

**Context:** Current cast evidence is mostly static. Number casts are dynamic because fallibility depends on source and target scales. Add scale-aware casts without enumerating every scale pair.

- [ ] Introduce a single cast policy input struct, for example:

```rust
pub(crate) struct BuiltinCastPolicyInput<'a> {
    pub(crate) policy: BuiltinCastPolicyId,
    pub(crate) source: &'a BuiltinCastLiteral,
    pub(crate) source_type: TypeId,
    pub(crate) target_type: TypeId,
    pub(crate) type_environment: &'a TypeEnvironment,
}
```

- [ ] Replace old policy application call sites with the context-based API. Do not preserve a parallel legacy wrapper except as a private helper during one slice, and delete it before phase completion.
- [ ] Extend `BuiltinCastLiteral` with `Number` and, if needed for const-only support, `Byte`.
- [ ] Add dynamic Number-family evidence resolution:
  - [ ] static scalar table remains for existing scalar casts;
  - [ ] Number casts are resolved from source/target `TypeId` and `TypeEnvironment`;
  - [ ] same-type casts remain invalid;
  - [ ] no scale-pair evidence table.
- [ ] Implement const-foldable Number casts:
  - [ ] `Int -> NumberN`;
  - [ ] `NumberN -> Int`;
  - [ ] `NumberN -> NumberM` widening/narrowing;
  - [ ] `NumberN -> String`;
  - [ ] `String -> NumberN`.
- [ ] Reject/defer `Float <-> NumberN` casts with targeted diagnostics. Point users to future lossy helpers.
- [ ] Decide Byte cast scaffold:
  - [ ] If added now, implement const `Int -> Byte` and `Byte -> Int` only.
  - [ ] Runtime Byte casts remain backend-gated.
- [ ] Update core cast trait metadata without scale-specific trait names.
- [ ] Add tests for Number casts, invalid Float/Number casts, exact narrowing success/failure, String parse success/failure, and Byte const casts if included.

**Acceptance:** Number casts fold in const contexts and runtime cast resolution can carry scale-aware policy to HIR/backend without redundant APIs.

---

## Phase 5 — AST Number operators and const folding

**Context:** Number arithmetic and comparisons become part of the typed AST. Statically known failures remain diagnostics; runtime-dependent expressions are preserved for HIR.

- [ ] Update operator type checking:
  - [ ] same-scale Number arithmetic;
  - [ ] exact `Int` participation;
  - [ ] cross-scale diagnostics;
  - [ ] Float/Number diagnostics;
  - [ ] scale-zero `/` diagnostic;
  - [ ] positive-scale `//` diagnostic;
  - [ ] `NumberN ^ Int` only.
- [ ] Update constant folding:
  - [ ] fold Number add/subtract/multiply/divide/modulo/power/negation through `numeric_values`;
  - [ ] preserve per-operation rounding;
  - [ ] report divide-by-zero and invalid exponent as typed compile-time diagnostics;
  - [ ] keep runtime RPN for non-foldable operands.
- [ ] Update equality/comparison:
  - [ ] same-scale Number valid;
  - [ ] Number/Int valid through exact scaling;
  - [ ] cross-scale Number invalid;
  - [ ] Number/Float invalid.
- [ ] Re-check template value-to-string behavior after folded Number operations.
- [ ] Add tests for const arithmetic, runtime-preserved expressions, mixed Int/Number, cross-scale diagnostics, Float diagnostics, scale-zero `/`, positive-scale `//`, and observable per-operation rounding.

**Acceptance:** Number source semantics are enforced before HIR; const and runtime-preserved behavior are tested.

---

## Phase 6 — Behavior-preserving HIR numeric op refactor

**Context:** Refactor HIR numeric shape before adding Number runtime lowering. This phase should preserve existing Int/Float behavior and reduce later churn.

- [ ] Replace old `HirNumericOp` enum variants with:

```rust
pub(crate) enum HirNumericDomain {
    Int,
    Float,
    Number { scale: NumberScale },
}

pub(crate) enum HirNumericOperator {
    Add,
    Subtract,
    Multiply,
    Divide,
    IntDivide,
    Modulo,
    Power,
    Negate,
}

pub(crate) struct HirNumericOp {
    pub(crate) domain: HirNumericDomain,
    pub(crate) operator: HirNumericOperator,
}
```

- [ ] Leave `Byte` out of arithmetic domains for V1 unless a scaffold-only marker is needed for validation. Prefer no Byte numeric op domain until Byte arithmetic is designed.
- [ ] Update all Int/Float call sites:
  - [ ] AST/HIR numeric classification;
  - [ ] range-loop counter updates;
  - [ ] HIR display/debug;
  - [ ] validation;
  - [ ] reachability;
  - [ ] JS helper mapping;
  - [ ] tests.
- [ ] Preserve `NumericFailureMode` unchanged.
- [ ] Preserve `HirNumericOperands` unless a named struct materially simplifies validation.
- [ ] Add HIR expression variants for `Number` and `Byte` only if earlier phases require HIR to carry these values before backend support. Use boxed Number values if enum size becomes noisy.
- [ ] Search and remove stale old variant names: `IntAdd`, `FloatAdd`, etc.
- [ ] Add HIR tests proving the refactor did not change existing Int/Float lowering behavior.

**Acceptance:** Existing checked Int/Float arithmetic passes all tests with the new domain/operator HIR shape.

---

## Phase 7 — Number HIR lowering and HTML-JS vertical slice

**Context:** Avoid exposing reachable runtime Number to JS without lowering support. This phase wires Number HIR and JS runtime together as one vertical implementation slice.

- [ ] Lower runtime Number operations to `HirStatementKind::NumericOp` with `HirNumericDomain::Number { scale }`.
- [ ] Ensure HIR result type helpers return `type_environment.intern_number(scale)` for Number domains.
- [ ] Decide operand shape:
  - [ ] Preferred: convert `Int` operands to explicit Number values before the HIR numeric op so backend helpers receive uniform Number operands.
  - [ ] Do not make JS helpers infer source types from raw JS values except at one documented conversion boundary.
- [ ] Validate Number HIR invariants:
  - [ ] operator arity;
  - [ ] result local scalar/carrier type;
  - [ ] operand scale consistency;
  - [ ] no invalid source operation reaches HIR.
- [ ] Split JS numeric runtime if needed:

```text
src/backends/js/runtime/numeric/
    mod.rs
    int_float.rs
    number.rs
    carriers.rs
```

- [ ] Extend demand-driven JS runtime usage flags for Number operations, Number formatting, and Number casts.
- [ ] Add JS Number runtime helpers:
  - [ ] constructor and brand/scale validation;
  - [ ] literal construction from scaled string and scale;
  - [ ] canonical formatter;
  - [ ] exact scale widening/narrowing;
  - [ ] Int-to-Number and Number-to-Int;
  - [ ] String-to-Number exact parser;
  - [ ] arithmetic helpers for add/subtract/multiply/divide/modulo/power/negation;
  - [ ] half-even rounding.
- [ ] Use the existing carrier contract for checked failures:
  - [ ] `ReturnError` helpers return `{ tag: "ok" | "err", value }`.
  - [ ] `Trap` may initially use existing trap wrapper if direct trap helpers are deferred to Phase 8.
- [ ] Update JS expression lowering for Number literals.
- [ ] Update JS statement lowering for Number `NumericOp`.
- [ ] Update JS cast lowering for scale-aware Number policies.
- [ ] Update JS value-to-string/template interpolation path to recognize branded Number values.
- [ ] Update JS clone/copy/equality helpers as needed:
  - [ ] Number is immutable; copy can return the same object.
  - [ ] same-scale equality compares scaled integer.
- [ ] Add integration tests:
  - [ ] runtime Number literals and canonical formatting;
  - [ ] runtime arithmetic and half-even division;
  - [ ] observable per-operation rounding;
  - [ ] mixed Int/Number arithmetic;
  - [ ] casts;
  - [ ] template interpolation;
  - [ ] recoverable divide-by-zero in builtin `Error!` function;
  - [ ] top-level trap behavior.

**Acceptance:** Number runs through HTML-JS with correct formatting, arithmetic, casts, and failure behavior.

---

## Phase 8 — Backend validation gates for Wasm Number and runtime Byte

**Context:** Keep backend lowerers from seeing unsupported target features. Prefer reusable semantic type scanning over one-off reachability vectors.

- [ ] Add a reusable backend validation helper that scans reachable blocks and expressions with a `TypeEnvironment` predicate, similar in spirit to existing semantic generic-runtime-value validation.
- [ ] Use syntactic reachability roots already selected by backend validation; do not invent a separate root policy.
- [ ] Wasm rejects reachable:
  - [ ] Number literals/values;
  - [ ] Number numeric ops;
  - [ ] Number casts;
  - [ ] Number formatting/template interpolation.
- [ ] JS rejects reachable runtime Byte values, Byte casts, and Byte operations.
- [ ] Wasm rejects reachable runtime Byte values, Byte casts, and Byte operations.
- [ ] Dead/unreachable helpers containing Number or Byte should not block builds for selected roots.
- [ ] Diagnostics use `CompilerDiagnostic::unsupported_backend_feature` or a more precise typed diagnostic.
- [ ] Add tests:
  - [ ] HTML-Wasm rejects reachable Number.
  - [ ] HTML-Wasm does not reject unreachable Number helper.
  - [ ] HTML-JS rejects reachable Byte runtime value.
  - [ ] HTML-Wasm rejects reachable Byte runtime value.
  - [ ] Byte/Number backend diagnostics include useful feature names.

**Acceptance:** Unsupported target use fails before lowering with structured diagnostics; JS Number remains supported.

---

## Phase 9 — Direct trap-mode numeric lowering cleanup

**Context:** Trap mode does not need builtin `Error` construction. This is an optimization/cleanup phase after Number correctness is stable.

- [ ] Split JS checked numeric helper strategy:
  - [ ] carrier helpers for `NumericFailureMode::ReturnError`;
  - [ ] direct trap helpers for `NumericFailureMode::Trap`.
- [ ] Implement direct trap helpers for Int, Float, and Number where straightforward.
- [ ] Throw directly on trap failures without constructing builtin `Error` carriers.
- [ ] Preserve recoverable `ReturnError` behavior exactly.
- [ ] Keep `__bs_numeric_trap` only for remaining shared cases if there is a documented reason.
- [ ] Add tests:
  - [ ] top-level numeric failure traps;
  - [ ] builtin `Error!` function still recovers;
  - [ ] golden/artifact assertion that trap-only numeric paths do not call `__bs_error_result` for numeric failures.

**Acceptance:** Trap-mode JS numeric lowering avoids unnecessary builtin Error carrier construction without changing recoverable semantics.

---

## Phase 10 — Numeric analysis and Int trap-mode JS check elision

**Context:** Add optimization facts without changing HIR or user diagnostics. V1 has one consumer: JS lowering. Keep the report backend-local until another backend needs it.

- [ ] Add `src/compiler_frontend/analysis/numeric/`:
  - [ ] `mod.rs`
  - [ ] `report.rs`
  - [ ] `ranges.rs`
  - [ ] `transfer.rs`
  - [ ] `tests/`
- [ ] Export it from `analysis/mod.rs`.
- [ ] Define facts:

```rust
pub(crate) struct NumericAnalysisReport {
    pub(crate) statement_facts: FxHashMap<HirNodeId, NumericStatementFact>,
    pub(crate) value_ranges: FxHashMap<HirValueId, NumericRange>,
}

pub(crate) enum NumericCheckDisposition {
    ProvenSafe,
    NeedsRuntimeCheck,
}
```

- [ ] Do not include user-facing `ProvenFailure` diagnostics in V1.
- [ ] Implement conservative Int range facts:
  - [ ] exact literal intervals;
  - [ ] unknown;
  - [ ] bounded inclusive interval;
  - [ ] non-zero fact when easy.
- [ ] Implement limited transfer:
  - [ ] simple assignment/copy propagation;
  - [ ] simple relational branch refinement where HIR shape makes it straightforward;
  - [ ] Int add/subtract/multiply/divide/modulo/negation/power safety checks.
- [ ] Invoke analysis inside JS lowering before emission, not in the compiled module handoff.
- [ ] JS lowering uses facts only when:
  - [ ] `NumericFailureMode::Trap`;
  - [ ] domain is `Int`;
  - [ ] fact is `ProvenSafe`.
- [ ] Direct arithmetic must preserve Int normalization such as JS `-0` handling.
- [ ] Do not elide checks for `ReturnError`, Float, Number rounding, Number divide-by-zero, or Byte.
- [ ] Add tests:
  - [ ] unit tests for ranges and dispositions;
  - [ ] JS artifact tests showing helper omission for proven-safe trap-mode Int;
  - [ ] uncertain operations still use checked helpers;
  - [ ] `ReturnError` operations still use carrier helpers;
  - [ ] Number operations are not fused or rounded away.

**Acceptance:** JS emits direct arithmetic for proven-safe trap-mode Int operations only; all other semantics remain checked.

---

## Phase 11 — Byte scaffold closure

**Context:** Finish the frontend/HIR Byte slice and make backend deferral deliberate.

- [ ] Confirm `Byte` type resolution, display, and reserved-name diagnostics.
- [ ] Confirm contextual literal validation for `0..255`.
- [ ] Decide const-only Byte casts:
  - [ ] recommended: `Int -> Byte` exact/range-checked and `Byte -> Int` const-foldable;
  - [ ] runtime Byte casts remain backend-gated.
- [ ] Reject Byte arithmetic with a typed diagnostic. Do not add wrapping behavior.
- [ ] Ensure backend validation catches every reachable runtime Byte value before JS/Wasm lowerers.
- [ ] Update docs/progress/roadmap to state Byte support is scaffold-only.
- [ ] Add tests for literals, range errors, cast scaffold if implemented, arithmetic rejection, backend unsupported diagnostics, and name collisions.

**Acceptance:** Byte is visible as a planned type scaffold with clean diagnostics and no backend leakage.

---

## Phase 12 — Final docs, stale-term audit, and integration matrix

**Context:** Ensure the final repository state is coherent after multi-slice implementation.

- [ ] Update `docs/language-overview.md`:
  - [ ] Number type family;
  - [ ] operator table;
  - [ ] per-operation rounding;
  - [ ] casts;
  - [ ] template interpolation;
  - [ ] Byte scaffold;
  - [ ] deferred helper list.
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] `NumberScale` type identity;
  - [ ] `numeric_values` owner;
  - [ ] TIR shared value formatting path;
  - [ ] HIR numeric domain/operator shape;
  - [ ] backend type-gating;
  - [ ] numeric analysis side-table ownership.
- [ ] Update `docs/src/docs/progress/#page.bst` to match actual support.
- [ ] Update `docs/roadmap/roadmap.md` to remove stale BigInt/Decimal future direction and list deferred follow-ups.
- [ ] Search and clean stale terms:
  - [ ] `Decimal` comments/stubs unless intentionally historical and marked inactive replaced;
  - [ ] `BigInt` roadmap references;
  - [ ] old `HirNumericOp::IntAdd` / `FloatAdd` variants;
  - [ ] temporary TODOs from implementation phases.
- [ ] Required integration cases:
  - [ ] Number literal success and diagnostics;
  - [ ] Number arithmetic JS output;
  - [ ] mixed Int/Number success;
  - [ ] mixed scale / Float / invalid operator diagnostics;
  - [ ] Number casts;
  - [ ] Number Error recovery vs top-level trap;
  - [ ] Number Wasm unsupported;
  - [ ] Byte scaffold diagnostics;
  - [ ] Int check-elision JS artifact assertions.
- [ ] Required unit tests:
  - [ ] `numeric_values` math;
  - [ ] TypeEnvironment Number interning;
  - [ ] dynamic cast resolution;
  - [ ] HIR numeric validation;
  - [ ] backend target-gate scanner;
  - [ ] numeric analysis facts.

**Acceptance:** Documentation, tests, and implementation status agree; `just validate` passes; no stale BigInt/Decimal direction remains.

---

## 6. Deferred features that must remain explicit

### Wasm

- Number runtime representation and helpers.
- Number casts.
- Number formatting/interpolation.
- Byte runtime representation.
- Byte casts/collections/ABI.
- Wasm use of numeric analysis facts.
- Full checked numeric helper parity, Float formatting parity, and external Float boundary validation parity.

### Number helpers

- Lossy `Float -> NumberN` helper/method.
- Lossy `NumberN -> Float` helper/method.
- Fixed-width formatting.
- Non-default rounding helpers.
- Scale-narrowing with rounding.
- Broader numeric library APIs.

### Optimization

- ReturnError CFG simplification for proven-safe numeric operations.
- Float finite-proof check elision.
- Number rounding/check fusion where equivalent.
- Broader range analysis over loops, relational patterns, fixed capacities, collection lengths, and map facts.
- Numeric facts shared with Wasm.

### Byte

- JS runtime representation.
- Wasm runtime representation.
- Byte collections/storage specialization.
- Byte ABI/export behavior.
- Binary buffer integration.
- Byte formatting/parsing helpers.
- Byte arithmetic policy.

---

## 7. Risk register

| Risk | Mitigation |
|---|---|
| TIR and legacy template paths diverge for Number formatting. | Use one shared frontend value-to-string formatter consumed by TIR and fallback paths. Add tests for both where practical. |
| Dynamic Number casts make the cast subsystem noisy. | Use one cast policy context and scale-aware resolver; do not enumerate scale pairs. |
| Numeric analysis causes broad Module/API churn. | Run V1 analysis inside JS lowering; promote to compiled-module metadata only after another backend consumes it. |
| Byte scaffold leaks to backend lowerers. | Add backend type-gate validation before JS/Wasm lowering and test reachable/unreachable cases. |
| Per-operation rounding gets optimized away. | Treat rounding as semantic; add observable rounding tests. Elide/fuse only when proven equivalent. |
| Inactive Decimal compatibility remains. | Remove/rename stale Decimal paths; do not add aliases or shims. |
| User diagnostics route through CompilerError. | Phase review requires typed `CompilerDiagnostic` for source/type/rule/backend deferrals. |
| JS helper file becomes monolithic. | Split `runtime/numeric` into submodules before adding large Number helper surface. |

---

## 8. Completion checklist

- [ ] Active context capsule is updated to the latest accepted slice.
- [ ] `Number` / `NumberN` type syntax resolves and canonicalizes.
- [ ] `Number` literals and contextual materialization are exact.
- [ ] Const Number arithmetic, casts, formatting, and template interpolation work.
- [ ] Runtime Number arithmetic, casts, formatting, and template interpolation work on HTML-JS.
- [ ] Number failures trap or recover according to existing `NumericFailureMode`.
- [ ] Wasm rejects reachable Number with structured diagnostics.
- [ ] Byte frontend/HIR scaffold exists and runtime backend use is rejected cleanly.
- [ ] HIR numeric ops use domain/operator shape.
- [ ] Numeric analysis side table exists.
- [ ] JS elides proven-safe trap-mode Int checks only.
- [ ] Docs and progress matrix match the implemented/deferred state.
- [ ] `just validate` passes.
