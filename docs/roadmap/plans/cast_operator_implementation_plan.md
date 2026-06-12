# Beanstalk `cast` Operator Implementation Plan

**Prerequisite:** the language surface hardening work is already merged on `main`.

**Goal:** add `cast` as Beanstalk's explicit conversion marker for conversions **to compiler-supported builtin core types only**, while removing scalar constructor-style conversion syntax and reusing the simplified static trait/evidence system.

---

## 1. Current repository anchor

This plan is anchored against the post-hardening shape of `nyejames/beanstalk` on `main` observed during planning. Re-run Phase 0 before editing because the repository is active.

| Area | Current shape to rely on |
|---|---|
| Trait metadata | `ResolvedTraitDefinition` stores static trait identity, requirements, source location, and visibility. Dynamic-safety fields are gone. |
| Trait environment | `TraitEnvironment` owns trait IDs and currently has a compiler-owned `DISPLAYABLE` registration path. This is the model to generalize for core cast traits. |
| Trait evidence | `TraitEvidenceKind` has `Canonical` and `Builtin`; file-local extension evidence is gone. `TraitEvidenceEnvironment` indexes canonical/builtin evidence by `(TypeId, TraitId)` and exposes reusable evidence by target type. |
| Conformance target resolution | User-authored conformance targets are same-file nominal structs/choices only. Builtins, aliases, external opaque types, and nonlocal source types are rejected. |
| Receiver methods | `ReceiverMethodCatalog` enforces the same-file nominal receiver invariant. Builtin, imported, and external receiver extensions are rejected. |
| Type coercion | `type_coercion::contextual` owns implicit contextual coercions such as `Int -> Float` and `T -> T?`. Keep this separate from explicit `cast`. |
| Existing cast code | `ExpressionKind::BuiltinCast`, `BuiltinCastKind`, `HirExpressionKind::BuiltinCast`, and `HirBuiltinCastKind` currently model old `Int(...)` / `Float(...)` conversions. Replace these rather than adding a parallel system. |
| Builtin errors | `builtins/error_codes.rs` owns compiler-generated error code values and messages. Reuse it for runtime cast errors. |
| Diagnostics | `compiler_messages` owns typed user diagnostics. Use `CompilerDiagnostic` for source rejections and `CompilerError` only for internal invariants. |

### Phase 0 preflight searches

Run these before implementation:

```bash
rg -n "BuiltinCast|BuiltinCastKind|HirBuiltinCastKind|parse_builtin_cast_expression|Int\(|Float\(|Bool\(|String\(|Char\(" src docs tests libraries
rg -n "DISPLAYABLE|TraitEnvironment|TraitEvidenceKind|TraitEvidenceEnvironment|resolve_trait_reference|InvalidTraitConformanceReason" src/compiler_frontend
rg -n "cast|CASTABLE_TO|TRY_CASTABLE_TO|must not" src docs tests libraries
rg -n "DynamicTrait|FileLocalExtension|file-local extension|receiver extension" src docs tests libraries
rg -n "IntParseInvalidFormat|FloatParseInvalidFormat|BuiltinErrorCode" src/compiler_frontend
```

Record current commit SHA, validation baseline, and any unrelated pre-existing failures in a local note.

---

## 2. Final language design

### 2.1 Purpose

`cast` is the explicit conversion marker for converting a value to one of Beanstalk's compiler-supported builtin core types:

```text
Bool
Int
String
Char
Float
Error
```

It does not create a general conversion system. User-defined cast targets, external opaque cast targets, generic cast targets, generic cast traits, conversion constructors, return-type-directed overloads, and target-associated conversion traits are outside the `cast` design scope.

### 2.2 Syntax

Supported forms:

```beanstalk
value Target = cast expression
value Target = cast! expression
value Target = cast expression catch:
    then fallback
;
value Target = cast expression catch |err|:
    then fallback
;
```

Rules:

- `cast` is a reserved keyword everywhere.
- `cast!` follows the same attachment rule as `return!`; the `!` must be attached to `cast`.
- `cast ! value` and `cast !value` are invalid.
- `cast! value catch:` is invalid because propagation and local recovery are mutually exclusive.
- `cast` is a low-precedence prefix boundary. It consumes the operand expression until the current receiving-site boundary.
- Parentheses narrow the operand after `cast`: `value Int = cast (left + right)`.
- `cast` itself is not an ordinary unary operator and must not be valid as an operator operand.

### 2.3 Explicit target sites

`cast` is valid only when the immediate frontend boundary owns an explicit expected builtin target type.

Valid target providers:

```beanstalk
value Int = cast source

count ~Int = 0
count = cast! text

return cast source

draw(cast x, cast y)

Point(x = cast text_x, y = cast text_y)

values {Int} = {cast "1", cast "2"}

scores {String = Int} = {
    cast 1001 = cast! "42",
}
```

Also valid:

- function parameter defaults when the parameter type provides the target;
- struct field defaults when the field type provides the target;
- `then` arms in value-producing blocks when the enclosing value-producing block has an explicit receiver target.

Invalid target providers:

```beanstalk
value = cast source
consume(cast text)
value Int = identity(cast text)
if cast value:
loop cast value:
assert(cast value)
[: [cast value]]
(cast 1) + 2.5
cast value
```

Generic inference must not depend on `cast`. A generic parameter slot is never a cast target. A generic bound proves that a generic value can be cast **inside** the generic function once the function body has an explicit builtin target.

### 2.4 Optional contexts

`T?` is not a cast target. If the receiving type is optional, the cast target is the inner builtin `T`, then the existing `T -> T?` contextual wrapping applies.

```beanstalk
thing String? = cast 1 + 1
```

means:

```text
Int expression -> cast to String -> normal String-to-String? wrapping
```

There are no `CASTABLE_TO_OPTIONAL_*` traits.

Optional source values are not auto-unwrapped:

```beanstalk
maybe_text String? = "42"
count Int = cast! maybe_text -- invalid
```

Use explicit option inspection.

### 2.5 Same-type and existing implicit coercions

`cast` requires a real source-target conversion.

Invalid:

```beanstalk
value Int = cast 1
text String = cast "hello"
```

Diagnostic intent:

```text
This value already has type `Int`.
Remove `cast`.
```

Implicit `Int -> Float` promotion stays. Explicit `cast` is still allowed for `Int -> Float` because it is a real conversion:

```beanstalk
ratio Float = cast count
```

But this is invalid if the full operand naturally resolves to `Float` before the cast:

```beanstalk
ratio Float = cast 1 + 2.5
```

### 2.6 Fallibility and operand failures

| Form | Required evidence | Behavior |
|---|---|---|
| `cast expression` | infallible | Produces target value. Rejects fallible-only evidence. |
| `cast! expression` | fallible | Propagates cast failure through the current `Error!` return slot. |
| `cast expression catch:` | fallible | Recovers cast failure locally. |

`cast!` and `cast ... catch:` handle only failures produced by the cast evidence. Operand `Error!` must be handled before the cast.

```beanstalk
count Int = cast! load_text()!

count Int = cast load_text()! catch:
    then 0
;
```

If the operand itself is fallible and unhandled, emit a result-handling diagnostic that points at the operand, not a cast-target diagnostic.

`cast!` outside a function with an `Error!` return slot should use the existing result-propagation diagnostic category.

### 2.7 Scalar constructor-style conversions

Remove scalar constructor-style conversion syntax:

```beanstalk
Int(value)
Float(value)
Bool(value)
String(value)
Char(value)
```

Keep `Error(...)` as a real builtin constructor:

```beanstalk
err = Error("Missing number", 200)
```

Use `cast` for conversion to `Error`:

```beanstalk
err Error = cast "Missing number"
```

### 2.8 Templates

Template interpolation remains separate from `cast`.

Invalid:

```beanstalk
[: [cast value]]
```

Templates keep their own implicit string-rendering behavior. Future template-head support may accept user-defined types that implement `CASTABLE_TO_STRING`, but only with an explicit `cast` at a typed template-head boundary. Template interpolation itself must not become a `CASTABLE_TO_STRING` target.

### 2.9 Conditions and assertions

Condition positions are not cast targets:

```beanstalk
if cast value:
loop cast value:
assert(cast value)
```

Use a typed declaration first:

```beanstalk
condition Bool = cast value
if condition:
    ...
;
```

### 2.10 Single-value rule

`cast` always converts exactly one source value to exactly one target value.

- The operand must resolve to one ordinary success value.
- The target context must provide exactly one expected value type.
- `cast!` may propagate an `Error!` channel, but the success side is still one value.
- Multi-return operands and multi-slot targets are invalid. Use per-slot casts.

---

## 3. Cast traits and evidence

### 3.1 Core cast trait names

The compiler owns these globally visible static trait names:

```text
CASTABLE_TO_BOOL
TRY_CASTABLE_TO_BOOL
CASTABLE_TO_INT
TRY_CASTABLE_TO_INT
CASTABLE_TO_STRING
TRY_CASTABLE_TO_STRING
CASTABLE_TO_CHAR
TRY_CASTABLE_TO_CHAR
CASTABLE_TO_FLOAT
TRY_CASTABLE_TO_FLOAT
CASTABLE_TO_ERROR
TRY_CASTABLE_TO_ERROR
```

Rules:

- No import is required.
- Users cannot declare, import, export, alias, or shadow these names.
- The names are valid in `Type must TRAIT`, `type T is TRAIT`, and `TRAIT must not TRAIT` only.
- They are static contracts, not value types.
- They do not create ordinary receiver methods for builtin source types.

### 3.2 Core requirements

| Trait | Requirement |
|---|---|
| `CASTABLE_TO_INT` | `to_int \|This\| -> Int` |
| `TRY_CASTABLE_TO_INT` | `try_to_int \|This\| -> Int, Error!` |
| `CASTABLE_TO_FLOAT` | `to_float \|This\| -> Float` |
| `TRY_CASTABLE_TO_FLOAT` | `try_to_float \|This\| -> Float, Error!` |
| `CASTABLE_TO_BOOL` | `to_bool \|This\| -> Bool` |
| `TRY_CASTABLE_TO_BOOL` | `try_to_bool \|This\| -> Bool, Error!` |
| `CASTABLE_TO_STRING` | `to_string \|This\| -> String` |
| `TRY_CASTABLE_TO_STRING` | `try_to_string \|This\| -> String, Error!` |
| `CASTABLE_TO_CHAR` | `to_char \|This\| -> Char` |
| `TRY_CASTABLE_TO_CHAR` | `try_to_char \|This\| -> Char, Error!` |
| `CASTABLE_TO_ERROR` | `to_error \|This\| -> Error` |
| `TRY_CASTABLE_TO_ERROR` | `try_to_error \|This\| -> Error, Error!` |

Requirement receivers are always immutable `This`. `cast` must not require mutable access or consume the source.

User-defined source implementations use ordinary same-file receiver methods. Those methods remain callable if visible.

### 3.3 User-authored source evidence

Allowed:

```beanstalk
UserId = |
    value String,
|

to_string |this UserId| -> String:
    return this.value
;

UserId must CASTABLE_TO_STRING
```

Allowed for same-file generic nominal constructors when the normal static trait rules allow the method body:

```beanstalk
Box type A = |
    value A,
|

to_string type A |this Box of A| -> String:
    return "box"
;

Box must CASTABLE_TO_STRING
```

Rejected:

- conformance for builtins;
- conformance for imported/dependency/library types;
- conformance for external opaque types;
- conformance for types declared in another file;
- conditional/specialized/blanket conformance;
- conformance to both infallible and fallible cast traits for the same builtin target.

One source type may implement cast traits for multiple builtin targets.

### 3.4 Builtin evidence table

Initial compiler-owned evidence:

| Source | Target | Fallibility | Trait | Policy |
|---|---|---|---|---|
| `Int` | `Float` | infallible | `CASTABLE_TO_FLOAT` | exact numeric widening |
| `Int` | `String` | infallible | `CASTABLE_TO_STRING` | decimal formatting |
| `Float` | `String` | infallible | `CASTABLE_TO_STRING` | shortest stable decimal formatting |
| `Bool` | `String` | infallible | `CASTABLE_TO_STRING` | `true` / `false` |
| `Char` | `String` | infallible | `CASTABLE_TO_STRING` | one Unicode scalar |
| `Char` | `Int` | infallible | `CASTABLE_TO_INT` | Unicode scalar value |
| `String` | `Error` | infallible | `CASTABLE_TO_ERROR` | `Error(message = text, code = 0)` |
| `Error` | `String` | infallible | `CASTABLE_TO_STRING` | `error.message` |
| `Float` | `Int` | fallible | `TRY_CASTABLE_TO_INT` | finite, in range, truncate toward zero |
| `Int` | `Char` | fallible | `TRY_CASTABLE_TO_CHAR` | valid Unicode scalar value |
| `String` | `Int` | fallible | `TRY_CASTABLE_TO_INT` | strict base-10 signed integer parse |
| `String` | `Float` | fallible | `TRY_CASTABLE_TO_FLOAT` | strict decimal/exponent parse |
| `String` | `Bool` | fallible | `TRY_CASTABLE_TO_BOOL` | `true` / `false` only |
| `String` | `Char` | fallible | `TRY_CASTABLE_TO_CHAR` | exactly one Unicode scalar |

Skipped intentionally:

```text
Bool -> Int
Int -> Bool
Float -> Bool
Bool -> Float
Float -> Char
Char -> Float
```

### 3.5 `TRAIT must not TRAIT`

Add narrow trait incompatibility metadata:

```beanstalk
A must not B, C
```

Meaning:

> No concrete type may explicitly conform to both traits.

Rules:

- Bodyless top-level trait relation.
- Both sides must resolve to visible traits before the relation.
- The relation is symmetric for validation even if written once.
- It affects explicit conformance validation only.
- It does not add requirements, remove requirements, affect method lookup, or imply negative conformance.
- `Type must not TRAIT` is outside language design scope.
- `TRAIT must TRAIT` / trait composition is outside language design scope.
- `A must not A` is invalid.
- Conflicts must be caught within one conformance declaration and across separate declarations.
- If both traits are visible at a conformance site, their relation is active.
- Public trait metadata must not expose a private trait through an exported `must not` relation.

Compiler-owned cast trait pairs register `must not` automatically:

```text
CASTABLE_TO_INT       must not TRY_CASTABLE_TO_INT
CASTABLE_TO_FLOAT     must not TRY_CASTABLE_TO_FLOAT
CASTABLE_TO_BOOL      must not TRY_CASTABLE_TO_BOOL
CASTABLE_TO_STRING    must not TRY_CASTABLE_TO_STRING
CASTABLE_TO_CHAR      must not TRY_CASTABLE_TO_CHAR
CASTABLE_TO_ERROR     must not TRY_CASTABLE_TO_ERROR
```

Registering one direction is enough internally if validation normalizes pairs symmetrically.

---

## 4. Builtin cast policies

Create a focused policy owner under `src/compiler_frontend/builtins/casts/`. Keep it compact; split only when a file becomes hard to scan.

Recommended structure:

```text
src/compiler_frontend/builtins/casts/
├── mod.rs              -- public module map and central type exports
├── targets.rs          -- BuiltinCastTarget, BuiltinCastPolicyId, source/target classification
├── traits.rs           -- core trait names, requirement names, core registration helpers
├── evidence.rs         -- builtin evidence table and lookup helpers
├── policies.rs         -- pure policy functions and fold/runtime metadata
└── resolution.rs       -- AST cast resolver, if keeping it out of expression parsing is cleaner
```

If `policies.rs` grows too large, split into:

```text
policies/
├── mod.rs
├── numeric.rs
├── text.rs
└── error.rs
```

### Policy rules

| Cast | Policy |
|---|---|
| `Float -> Int` | Reject `NaN`/infinity; reject outside `Int` range; truncate toward zero. |
| `Int -> Char` | Accept only valid Unicode scalar values; reject negatives, surrogate range, and values above `0x10FFFF`. |
| `Char -> Int` | Return Unicode scalar/code point as `Int`. |
| `String -> Int` | Trim leading/trailing Unicode whitespace; parse base-10 signed integer; require whole remaining string consumed; fail on overflow. Do not accept decimal text or underscores. |
| `String -> Float` | Trim leading/trailing Unicode whitespace; parse ordinary decimal/exponent text; require whole remaining string consumed; reject `NaN`/`Infinity` text and non-finite parse results. |
| `String -> Bool` | Trim whitespace; accept only lowercase ASCII `true` and `false`. |
| `String -> Char` | Do not trim; succeed only if the string contains exactly one Unicode scalar value. |
| `Int -> String` | Signed base-10 decimal, no separators. |
| `Float -> String` | Shortest stable round-trippable decimal. Ensure JS/Wasm/folding parity; add helper if backend defaults diverge. |
| `Bool -> String` | `true` or `false`. |
| `Char -> String` | One-character string containing the scalar. |
| `String -> Error` | `Error(message = text, code = 0)`. |
| `Error -> String` | `error.message`, not debug formatting. |

### Error codes

Reuse existing `BuiltinErrorCode` parse codes:

| Failure | Code |
|---|---|
| `String -> Int`, invalid format | `IntParseInvalidFormat = 200` |
| `String -> Int`, out of range | `IntParseOutOfRange = 201` |
| `String -> Float`, invalid format | `FloatParseInvalidFormat = 210` |
| `String -> Float`, out of range / non-finite | `FloatParseOutOfRange = 211` |

Add cast-only codes:

```rust
StringParseBoolInvalidFormat = 220
StringParseCharInvalidFormat = 230
FloatCastToIntInvalidValue = 240
FloatCastToIntOutOfRange = 241
IntCastToCharInvalidCodepoint = 250
```

Names may be adjusted to match project naming, but the numeric values must be centralized in `BuiltinErrorCode`, with messages in `default_message()`.

User-defined fallible casts return their own `Error!`; the compiler must propagate or bind that error unchanged.

---

## 5. Complexity reduction requirements

Use `cast` to simplify the existing compiler surface rather than layering a second conversion path on top.

- [ ] Replace `ExpressionKind::BuiltinCast` with a general `ExpressionKind::Cast` / `ResolvedCastExpression`.
- [ ] Replace `BuiltinCastKind` with `BuiltinCastTarget`, `BuiltinCastPolicyId`, and `ResolvedCastEvidence`.
- [ ] Replace `HirExpressionKind::BuiltinCast` with `HirExpressionKind::Cast`.
- [ ] Delete `HirBuiltinCastKind` after backend lowering is migrated.
- [ ] Delete `parse_builtin_cast_expression` and replace old scalar constructor-style calls with diagnostics.
- [ ] Move old numeric cast folding logic out of `constant_folding.rs` into builtin cast policy helpers.
- [ ] Do not keep compatibility wrappers for `Int(...)` / `Float(...)`.
- [ ] Generalize `TraitEnvironment` core trait registration instead of adding one field per cast trait.
- [ ] Prefer one static table for all core cast trait names, requirement names, targets, and incompatibility pairs.
- [ ] Do not reintroduce backend-facing trait metadata. HIR cast evidence must be resolved enough for backends to lower without trait solving.
- [ ] Keep target-context ownership at frontend boundaries. Do not make ordinary expression parsing globally type-directed.

Recommended core trait environment refactor:

```rust
pub(crate) struct TraitEnvironment {
    definitions: Vec<ResolvedTraitDefinition>,
    ids_by_path: FxHashMap<InternedPath, TraitId>,
    core_traits_by_name: FxHashMap<StringId, TraitId>,
    incompatible_traits: FxHashMap<TraitId, Vec<TraitIncompatibility>>,
}
```

Then migrate the existing `DISPLAYABLE` special lookup to the same `core_trait_id_for_name` path. This reduces repeated one-off core-trait helpers before adding twelve cast traits.

---

# Implementation phases

Each phase should end with `just validate` unless earlier targeted checks fail. For non-trivial phases also run `cargo fmt`, targeted unit tests, and `cargo run -- tests` before `just validate` to make failures easier to isolate.

Every phase ends with a manual style/stage-boundary audit:

- [ ] User-facing source failures are `CompilerDiagnostic`, not `CompilerError`.
- [ ] Diagnostics carry typed facts and stable reason enums.
- [ ] No compatibility shim or stale comment preserves old conversion syntax.
- [ ] `DataType` is not used for semantic equality when `TypeId` is available.
- [ ] New files have file-level docs.
- [ ] Large logic is split by owner, not by convenience.
- [ ] No broad boolean-heavy APIs where an enum would explain state.

---

## Phase 0 — Preflight and current-shape audit

### Context

The hardening pass changed the repo shape. This phase verifies the current contracts before adding `cast`.

### Checklist

- [ ] Confirm branch, commit, and clean working tree:

  ```bash
  git status --short
  git branch --show-current
  git rev-parse HEAD
  ```

- [ ] Run baseline validation:

  ```bash
  just validate
  ```

- [ ] Record unrelated pre-existing failures separately.
- [ ] Run the searches from Section 1.
- [ ] Identify every consumer of:
  - `ExpressionKind::BuiltinCast`;
  - `BuiltinCastKind`;
  - `HirExpressionKind::BuiltinCast`;
  - `HirBuiltinCastKind`;
  - `parse_builtin_cast_expression`;
  - old `Int(...)` / `Float(...)` diagnostics;
  - scalar constructor docs/tests.
- [ ] Confirm `TraitEvidenceKind` still has only `Canonical` and `Builtin`.
- [ ] Confirm user-authored conformance targets are still same-file nominal only.
- [ ] Confirm HIR/backend code no longer carries dynamic trait runtime machinery.
- [ ] Confirm stale dynamic-trait comments remain absent and remove any newly discovered stale comments before continuing.

### Gate

- [ ] Baseline and audit notes are complete.
- [ ] No production changes except obvious formatting in touched files.

---

## Phase 1 — Diagnostics, keyword reservation, and removed scalar constructors

### Context

Syntax and diagnostics should be established before semantic resolution. This phase also removes the old conversion surface so no agent accidentally preserves it while adding `cast`.

### Checklist

#### Keyword and name reservation

- [ ] Add `TokenKind::Cast`.
- [ ] Add `"cast"` to `keyword_token_kind` in `keywords.rs`.
- [ ] Add `"cast"` to `RESERVED_KEYWORD_SHADOWS` so `_cast`, `Cast`, etc. are rejected by the existing keyword-shadow policy.
- [ ] Add/update keyword tests under `src/compiler_frontend/tests/keyword_tests` or the current keyword test owner.
- [ ] Do not add a `CastBang` token unless reusing the `return!` attachment machinery truly requires it. Prefer `Cast` + `Bang` with an attachment check.

#### Diagnostics

Add typed reason variants and constructors for:

- [ ] `cast` without an explicit target;
- [ ] target not one of the builtin cast targets;
- [ ] same-type cast;
- [ ] fallible cast used with plain `cast`;
- [ ] infallible cast used with `cast!` or `catch`;
- [ ] `cast!` outside an `Error!` function;
- [ ] `cast! ... catch:` conflict;
- [ ] unhandled operand `Error!` before cast;
- [ ] `cast` operand has multiple success values;
- [ ] cast target has multiple slots;
- [ ] separated `cast !` / `cast !value`;
- [ ] old scalar constructor-style conversion removed.

Recommended new typed enum:

```rust
pub enum InvalidCastReason {
    MissingExplicitTarget,
    TargetNotBuiltin,
    TargetIsGenericParameter,
    SameSourceAndTarget,
    SourceIsOptional,
    OperandIsFallible,
    OperandArityMismatch,
    TargetArityMismatch,
    FallibleEvidenceRequiresHandling,
    InfallibleEvidenceCannotUseFallibleForm,
    PropagationRequiresErrorReturn,
    PropagationAndRecoveryConflict,
    BangMustAttachToCast,
    ScalarConstructorRemoved,
    NoEvidence,
    UserDefinedEvidenceNotConstFoldable,
    BuiltinCastFailedInConst,
}
```

Use the project’s existing diagnostic taxonomy if another name fits better. Keep structured `TypeId`s on payloads where relevant.

#### Remove scalar constructor conversion parsing

- [ ] Remove `parse_builtin_cast_expression` as a successful parse path.
- [ ] In expression dispatch, replace `DatatypeInt` / `DatatypeFloat` constructor handling with a diagnostic for removed scalar constructor conversions.
- [ ] Add the same removed-constructor diagnostic for `Bool(...)`, `String(...)`, and `Char(...)` if those currently parse through generic call or type-token paths.
- [ ] Keep `Error(...)` unchanged.
- [ ] Delete or rewrite old `Int(...)` / `Float(...)` success tests.

### Gate

- [ ] `cast` is reserved everywhere.
- [ ] Old scalar conversion constructors fail with stable diagnostics.
- [ ] `Error(...)` tests still pass.
- [ ] No old constructor-style conversion path remains hidden behind a compatibility wrapper.
- [ ] Run `just validate`.

---

## Phase 2 — Builtin cast module, targets, policies, and error codes

### Context

The cast policy should be a reusable compiler-owned module, not embedded in expression parsing, constant folding, or backend lowering.

### Checklist

- [ ] Add `src/compiler_frontend/builtins/casts/` and expose it from `builtins/mod.rs`.
- [ ] Define:

  ```rust
  pub(crate) enum BuiltinCastTarget { Bool, Int, String, Char, Float, Error }
  pub(crate) enum BuiltinCastFallibility { Infallible, Fallible }
  pub(crate) enum BuiltinCastPolicyId { ... }
  pub(crate) enum CastEvidenceKind { Builtin, UserDefined }
  ```

  Use project naming conventions. Avoid exposing more than callers need.

- [ ] Add one helper to classify cast targets from `TypeId`:

  ```rust
  fn builtin_cast_target_for_type(type_id: TypeId, type_environment: &TypeEnvironment) -> Option<BuiltinCastTarget>
  ```

- [ ] Add one helper for optional receiving contexts:

  ```rust
  fn cast_target_for_receiving_type(type_id: TypeId, type_environment: &TypeEnvironment) -> Option<CastTargetResolution>
  ```

  It should unwrap `T?` only for target classification and record that optional wrapping must still happen after the cast.

- [ ] Add policy functions for every builtin evidence row.
- [ ] Add `BuiltinErrorCode` variants for cast-only failures.
- [ ] Reuse existing numeric parse error codes for `String -> Int` and `String -> Float`.
- [ ] Replace old constant-folding numeric cast helpers with calls into this module.
- [ ] Add focused unit tests for policy functions:
  - `Float -> Int` truncates toward zero;
  - `NaN`/infinity fail;
  - out-of-range float fails;
  - valid/invalid Unicode scalar conversions;
  - strict string parsing;
  - string formatting stability where testable;
  - error code selection.

### Gate

- [ ] Cast policies are pure and independent from parser/HIR/backend state.
- [ ] Error codes are centralized.
- [ ] No policy is duplicated in constant folding or backend code.
- [ ] Run targeted unit tests and `just validate`.

---

## Phase 3 — Core cast traits, builtin evidence, and `must not`

### Context

Cast traits are static evidence hooks for user-defined same-file nominal source types. They are not dynamic traits, not user-defined targets, and not backend trait metadata.

### Checklist

#### Core trait registration

- [ ] Generalize `TraitEnvironment` core trait lookup so `DISPLAYABLE` and cast traits use one path.
- [ ] Add static metadata for all core cast traits:
  - trait name;
  - target;
  - fallibility;
  - requirement name;
  - return type;
  - error return channel if fallible.
- [ ] Add `register_core_cast_traits` or `register_core_traits` called during AST environment construction after builtin `TypeId`s are available.
- [ ] Add exact immutable `This` requirements.
- [ ] Ensure source declarations/import aliases cannot collide with core cast trait names.
- [ ] Ensure `resolve_trait_reference` finds core cast traits without imports.

#### Builtin evidence registration

- [ ] Register compiler-owned builtin evidence for the initial table in Section 3.4.
- [ ] Store builtin evidence in `TraitEvidenceEnvironment::insert_builtin`.
- [ ] Builtin evidence should satisfy static generic bounds.
- [ ] Builtin evidence must not create receiver methods on builtin types.

#### `must not` syntax and metadata

- [ ] Add parse-only syntax shell for `TRAIT must not TRAIT, TRAIT`.
- [ ] Add a header kind or declaration shell for trait incompatibility metadata.
- [ ] Enforce source order: both traits must already be visible/resolved when the relation is processed.
- [ ] Store normalized symmetric incompatibility pairs in `TraitEnvironment`.
- [ ] Register core cast trait incompatibility pairs automatically.
- [ ] Validate conflicts:
  - same `Type must A, B` declaration;
  - separate conformance declarations for the same type;
  - builtin evidence vs user evidence;
  - duplicate relation handling;
  - self-exclusion rejection.
- [ ] Add facade/public metadata validation so exported trait metadata does not expose private traits through `must not`.

#### Tests

- [ ] Positive `A must not B` relation parsing.
- [ ] Reject `A must not A`.
- [ ] Reject forward relation before trait names are visible.
- [ ] Reject conformance to incompatible traits in one declaration.
- [ ] Reject conformance to incompatible traits across declarations.
- [ ] Core cast trait names resolve without imports.
- [ ] Core cast trait names cannot be redeclared, imported, exported, aliased, or used as ordinary values.
- [ ] Builtin evidence satisfies generic bounds.

### Gate

- [ ] No dynamic trait machinery is reintroduced.
- [ ] Core trait registration is table-driven, not twelve repeated methods.
- [ ] `must not` remains trait-metadata-only, not negative conformance.
- [ ] Run trait/evidence tests and `just validate`.

---

## Phase 4 — Cast-aware parsing and AST resolution

### Context

The current parse-time expected-type system deliberately keeps ordinary expressions strict. Do not widen `ExpectedType` globally just to support `cast`. Instead, add an explicit cast-target channel owned by boundary callers.

### Checklist

#### Cast target context

- [ ] Add a narrow `CastTargetContext` / `ExplicitCastTarget` carried into expression parsing separately from `ExpectedType`.

  Recommended shape:

  ```rust
  pub(crate) enum CastTargetContext {
      None,
      ExplicitBoundary {
          target_type_id: TypeId,
          boundary: CastBoundaryKind,
      },
  }
  ```

- [ ] Keep `parse_expectation_for_type_id` behavior unchanged for ordinary expression parsing. It should still pass `Known` only for option/collection/map contexts unless another existing rule requires it.
- [ ] Boundary owners pass both:
  - existing `ExpectedType` for context-sensitive literals;
  - `CastTargetContext` for explicit casts.

#### Boundary owners to update

- [ ] Annotated declarations.
- [ ] Existing mutable assignment targets after normal place resolution succeeds.
- [ ] Explicit return slots.
- [ ] Concrete function parameters.
- [ ] Struct fields and constructor arguments.
- [ ] Function parameter defaults.
- [ ] Struct field defaults.
- [ ] Typed collection elements.
- [ ] Typed map keys and values.
- [ ] Value-producing `then` slots with an enclosing explicit receiver.

Do not pass cast targets into:

- operator operands;
- generic parameter slots;
- condition/match/assert positions;
- template interpolation;
- generic inference-only expected contexts;
- arbitrary nested expressions.

#### Parse shape

- [ ] In expression dispatch, accept `TokenKind::Cast` only when the expression starts at an explicit boundary.
- [ ] Parse optional attached `Bang` using the same adjacency rule as `return!`.
- [ ] Parse the operand as a low-precedence boundary expression until the current boundary stop token.
- [ ] For `cast expression catch:` parse the handler using existing `FallibleHandling::Handler` structures.
- [ ] Reject `cast! expression catch:`.
- [ ] Reject separated `cast !` forms.
- [ ] Reject `cast` after an expression has already started, unless a future explicit boundary grammar needs it. Do not allow operand-level casts.

Recommended AST shape:

```rust
pub(crate) struct ResolvedCastExpression {
    pub(crate) source: Box<Expression>,
    pub(crate) source_type_id: TypeId,
    pub(crate) target_type_id: TypeId,
    pub(crate) target: BuiltinCastTarget,
    pub(crate) optional_wrap_after_cast: bool,
    pub(crate) evidence: ResolvedCastEvidence,
    pub(crate) handling: CastHandling,
    pub(crate) location: SourceLocation,
}

pub(crate) enum CastHandling {
    Infallible,
    Propagate,
    Recover(FallibleHandling),
}

pub(crate) enum ResolvedCastEvidence {
    Builtin { policy: BuiltinCastPolicyId },
    UserDefined { evidence_id: TraitEvidenceId, method_path: InternedPath },
}
```

Use names that match the repo, but keep the same ownership.

#### Resolver behavior

- [ ] Resolve source natural type first.
- [ ] Reject optional source types.
- [ ] Reject same source/target `TypeId` after optional-target unwrapping.
- [ ] Reject non-builtin target.
- [ ] Select exact builtin evidence or user-defined canonical evidence.
- [ ] Use generic-bound evidence only inside concrete generic body emission; HIR must still receive resolved builtin or user-defined evidence.
- [ ] Enforce fallibility form against selected evidence.
- [ ] Reject no evidence with a cast-specific diagnostic.
- [ ] After a successful inner cast to `T`, apply existing contextual optional wrapping if the receiver was `T?`.

#### User-defined evidence

- [ ] Use existing `TraitEvidenceEnvironment` for source nominal evidence.
- [ ] Validate exact requirement method through existing trait requirement matching.
- [ ] Lower user-defined evidence methods as ordinary immutable receiver method calls.
- [ ] Do not support user-authored conformance for builtin/imported/external/nonlocal source types.

### Gate

- [ ] `cast` works only at explicit boundary sites.
- [ ] Generic inference cannot use `cast` to solve targets.
- [ ] Operator operand casts are rejected.
- [ ] Optional target wrapping uses the existing coercion machinery after cast resolution.
- [ ] Run parser/AST tests and `just validate`.

---

## Phase 5 — Constant folding, defaults, and const-required contexts

### Context

Compile-time folding remains AST-owned. HIR `Cast` represents only runtime casts that survive folding.

### Checklist

- [ ] Extend `fold_compile_time_expression` to handle `ExpressionKind::Cast`.
- [ ] Fold only compiler-owned builtin evidence marked const-foldable.
- [ ] Reject user-defined evidence in const-required contexts.
- [ ] Reject generic-bound evidence in const-required contexts unless concrete emission has already selected builtin evidence.
- [ ] On successful builtin fallible cast in a const context, fold to the success value.
- [ ] On failed builtin fallible cast in a const-required context, emit a compile-time diagnostic, not a runtime `Error` value.
- [ ] Allow `cast value catch:` in const-required contexts only when:
  - selected evidence is builtin;
  - source folds;
  - handler body folds;
  - result matches the target.
- [ ] Defaults:
  - Function parameter defaults and struct field defaults may use builtin casts only when they fold during default validation.
  - User-defined evidence in defaults is rejected unless defaults become runtime expressions in a separate feature.
  - Fallible builtin default casts are valid only when they fold successfully or recover through a fully foldable `catch:`.

### Gate

- [ ] All builtin policy folding is driven by `builtins/casts/policies`.
- [ ] `constant_folding.rs` no longer contains ad hoc numeric cast policy code.
- [ ] Const diagnostics are typed and stable.
- [ ] Run constant-folding tests and `just validate`.

---

## Phase 6 — HIR and backend lowering

### Context

HIR should carry explicit runtime cast operations for casts that survive AST folding. Do not keep the old `BuiltinCast` node alongside the new representation.

### Checklist

#### HIR data model

- [ ] Replace `HirExpressionKind::BuiltinCast` with `HirExpressionKind::Cast`.
- [ ] Delete `HirBuiltinCastKind`.
- [ ] Add a compact HIR evidence enum that is fully resolved:

  ```rust
  pub enum HirCastEvidence {
      Builtin { policy: BuiltinCastPolicyId },
      UserDefined { call_target: CallTarget },
  }
  ```

  Use existing HIR call identity types if available. Do not carry unresolved `TraitId` / `TraitEvidenceId` unless a local lowering invariant truly needs them.

- [ ] Include source expression, source type, target type, policy/evidence, and fallibility/handling shape needed for lowering.
- [ ] Remove stale dynamic-trait comments if touching `hir/expressions.rs`.

#### HIR lowering

- [ ] Lower builtin runtime casts to `HirExpressionKind::Cast`.
- [ ] Lower user-defined runtime casts to `HirExpressionKind::Cast` with a resolved user call target, or to a dedicated cast prelude that still keeps authored cast semantics inspectable. Prefer the explicit HIR `Cast` node unless it fights existing HIR invariants.
- [ ] Lower fallible casts through existing fallible-carrier/control-flow helpers.
- [ ] Ensure cast source access is immutable/shared.
- [ ] Ensure borrow validation sees the same access shape it would see for the equivalent immutable receiver call or builtin value read.
- [ ] Ensure `cast!` and `cast catch:` lower only cast failure, not operand failure.

#### Backend lowering

- [ ] Replace all backend handling of `HirBuiltinCastKind` with `BuiltinCastPolicyId` handling.
- [ ] Implement builtin casts for HTML JS.
- [ ] Implement builtin casts for HTML-Wasm where involved source/target runtime types already exist.
- [ ] If a specific Wasm cast cannot be supported, add a structured unsupported-backend diagnostic and a progress-matrix/roadmap entry. Prefer implementing the initial scalar/string/Error table for both active backends.
- [ ] Lower user-defined evidence casts through the selected direct function/method call.
- [ ] Do not make backends resolve traits or scan evidence tables.
- [ ] Keep JS/Wasm runtime helper names table-driven by `BuiltinCastPolicyId`.
- [ ] Ensure runtime failed builtin casts construct `Error` with the selected `BuiltinErrorCode`.

### Gate

- [ ] No `BuiltinCastKind` / `HirBuiltinCastKind` remains.
- [ ] Runtime casts that survive folding have a dedicated HIR representation.
- [ ] Backends lower cast operations without trait solving.
- [ ] JS and Wasm behavior matches const-folded policy tests.
- [ ] Run HIR/backend tests and `just validate`.

---

## Phase 7 — Integration tests

### Context

Integration tests are the main user-facing behavior check. Prefer real Beanstalk snippets and stable diagnostic codes.

### Required success cases

- [ ] Builtin infallible casts:
  - `Int -> Float`;
  - `Int/Float/Bool/Char/Error -> String`;
  - `Char -> Int`;
  - `String -> Error`.
- [ ] Builtin fallible casts with `cast!`:
  - `String -> Int`;
  - `String -> Float`;
  - `String -> Bool`;
  - `String -> Char`;
  - `Float -> Int`;
  - `Int -> Char`.
- [ ] Builtin fallible casts with `catch:` and `catch |err|:`.
- [ ] Runtime and const cases for foldable builtin casts.
- [ ] Typed optional receiving context: `value String? = cast 1`.
- [ ] Typed collection elements.
- [ ] Typed map keys and values.
- [ ] Struct field and function parameter targets.
- [ ] Function parameter default and struct field default with foldable builtin casts.
- [ ] Value-producing `then` arm target propagation.
- [ ] User-defined same-file nominal source cast to `String`.
- [ ] User-defined same-file nominal source cast to `Error`.
- [ ] Generic helper using `type T is CASTABLE_TO_STRING` and builtin evidence.
- [ ] Generic helper using user-defined nominal evidence.
- [ ] One source type implements cast traits for multiple builtin targets.

### Required failure cases

- [ ] Missing target: `value = cast text`.
- [ ] Generic target: `consume(cast text)` where slot is `T`.
- [ ] Generic inference target: `value Int = identity(cast text)`.
- [ ] Operator operand: `(cast 1) + 2.5`.
- [ ] Condition/assert/template interpolation target rejection.
- [ ] Same-type cast.
- [ ] Optional source cast.
- [ ] Operand `Error!` not handled before cast.
- [ ] `cast!` in a function without `Error!` return slot.
- [ ] `cast! value catch:`.
- [ ] Plain `cast` with fallible-only evidence.
- [ ] `cast!` with infallible-only evidence.
- [ ] No evidence.
- [ ] Old scalar constructor conversions: `Int(...)`, `Float(...)`, `Bool(...)`, `String(...)`, `Char(...)`.
- [ ] `Error(...)` still succeeds as constructor.
- [ ] Core cast trait redeclaration/import/export/alias/name collision.
- [ ] Conformance to incompatible cast trait pair.
- [ ] User conformance for builtin/imported/external/nonlocal source type.
- [ ] User-defined evidence in const-required context.
- [ ] Failed builtin const cast.
- [ ] Unsupported skipped casts such as `Bool -> Int`.

### Backend matrix

For each representative runtime success case:

- [ ] HTML JS success with output assertion.
- [ ] HTML-Wasm success or structured unsupported diagnostic if a policy is genuinely not lowerable.
- [ ] Avoid strict goldens unless exact emitted code is contractual.

### Gate

- [ ] Positive cases assert rendered output when possible.
- [ ] Negative cases assert stable diagnostic codes.
- [ ] No tests rely on old constructor-style conversions.
- [ ] Run `cargo run -- tests` and `just validate`.

---

## Phase 8 — Documentation, roadmap, and progress matrix

### Context

Docs must present `cast` as narrow explicit conversion to builtin core types, not as a general typeclass/conversion system.

### Checklist

#### `docs/language-overview.md`

- [ ] Add `cast` to syntax summary.
- [ ] Replace numeric text that says `Use Int(...)` with `cast!` / `cast` examples.
- [ ] Add a dedicated `Explicit Casts` section after numeric semantics.
- [ ] Document supported target set and outside-scope targets.
- [ ] Document fallibility forms.
- [ ] Document optional receiving behavior.
- [ ] Document target-site rules and invalid sites.
- [ ] Document core cast traits and same-file nominal source conformance.
- [ ] Document `TRAIT must not TRAIT` narrowly.
- [ ] Document removed scalar constructor-style conversions.
- [ ] Keep template rendering separate.

#### Docs site

Update or add pages under `docs/src/docs/**`:

- [ ] language overview page;
- [ ] traits page: add `must not` and core cast traits as static contracts;
- [ ] generics page: cast trait bounds examples;
- [ ] progress page: add `cast` row or update conversion/type-system row;
- [ ] any examples still using `Int(...)` / `Float(...)`.

#### Roadmap / matrix

- [ ] Mark user-defined cast targets outside cast design scope.
- [ ] Mark scalar constructor-style casts removed.
- [ ] Add backend support status for builtin runtime casts.
- [ ] Mention skipped conversions intentionally unsupported.
- [ ] Keep broad conversion features out of deferred lists.

#### Compiler design overview

- [ ] Update `type_coercion` description: it owns implicit contextual coercions, not explicit casts.
- [ ] Update `builtins` description to include compiler-owned cast policies/core trait metadata.
- [ ] Update AST ownership to include cast resolution and builtin cast folding.
- [ ] Update HIR ownership to include explicit cast operations.
- [ ] Remove stale references to `Int(...)` / `Float(...)` as explicit builtin casts.

### Gate

- [ ] Search docs/tests for old constructor-style conversion examples.
- [ ] Search docs for user-defined cast target language.
- [ ] Run docs build if separate, then `just validate`.

---

## Phase 9 — Final simplification audit

### Search audit

Run:

```bash
rg -n "BuiltinCast|BuiltinCastKind|HirBuiltinCastKind|parse_builtin_cast_expression" src tests docs libraries
rg -n "Int\(|Float\(|Bool\(|String\(|Char\(" docs tests libraries src
rg -n "CAST_TO_|TRY_CAST_TO_|CASTABLE_TO_COLOR|TRY_CASTABLE_TO_COLOR|user-defined cast target|generic cast target" docs src tests libraries
rg -n "DynamicTrait|FileLocalExtension|file-local extension|receiver extension" src docs tests libraries
rg -n "Cannot cast|InvalidNumericCast|IntParseInvalidFormat|FloatParseInvalidFormat" src tests
```

Expected remaining hits:

- `CASTABLE_TO_*` / `TRY_CASTABLE_TO_*` core trait names;
- negative tests for removed constructor-style conversions;
- docs explaining removed/outside-scope surfaces;
- new cast policy/error code code paths.

### Redundancy audit

- [ ] `TraitEnvironment` has one core-trait lookup path.
- [ ] Core cast trait metadata is table-driven.
- [ ] Builtin evidence table is single-source.
- [ ] Cast target classification is single-source.
- [ ] Fallibility validation is single-source.
- [ ] Builtin policy behavior is single-source.
- [ ] AST and HIR do not both contain separate builtin-only cast enums.
- [ ] Backends do not duplicate parse/format policies.
- [ ] No comments describe `Int(...)` / `Float(...)` as supported.
- [ ] No comments describe dynamic trait or file-local extension evidence as supported.
- [ ] No compatibility wrappers remain.

### Final validation

```bash
cargo fmt
cargo clippy
cargo test
cargo run -- tests
just validate
```

Manual stage-boundary review:

- [ ] Tokenizer only recognizes syntax and reserved keywords.
- [ ] Header parsing owns trait relation shell parsing but not semantic trait solving.
- [ ] AST owns cast target resolution, evidence selection, fallibility validation, and const folding.
- [ ] HIR carries runtime cast operations only after AST folding.
- [ ] Borrow validation sees normal immutable source access.
- [ ] Backends lower resolved cast operations without trait solving.
- [ ] Diagnostics are structured and stable.

---

## Final acceptance checklist

- [ ] `cast` and `cast!` syntax works at explicit typed boundaries only.
- [ ] `cast` is reserved everywhere.
- [ ] Scalar constructor-style conversions are removed.
- [ ] `Error(...)` remains valid.
- [ ] Cast targets are limited to `Bool`, `Int`, `String`, `Char`, `Float`, and `Error`.
- [ ] Optional receiving contexts cast to the inner builtin type and then use existing optional wrapping.
- [ ] Optional source casts are rejected.
- [ ] Same-type casts are rejected.
- [ ] Existing implicit `Int -> Float` coercion remains.
- [ ] `CASTABLE_TO_*` / `TRY_CASTABLE_TO_*` core traits are registered and globally visible.
- [ ] Core cast trait names cannot be redeclared, imported, exported, aliased, or shadowed.
- [ ] User-defined same-file nominal source casts to builtin targets work.
- [ ] User-defined cast targets are outside design scope and rejected.
- [ ] `TRAIT must not TRAIT` works as narrow trait incompatibility metadata.
- [ ] Cast trait fallibility pairs are mutually exclusive through `must not`.
- [ ] Builtin evidence satisfies static generic cast bounds.
- [ ] Builtin runtime cast errors use centralized `BuiltinErrorCode` values.
- [ ] Builtin cast policies are centralized and reused by folding and backends.
- [ ] User-defined fallible casts propagate their own `Error!` unchanged.
- [ ] Const folding applies only to compiler-owned builtin cast evidence.
- [ ] HIR carries explicit `Cast` operations for runtime casts that survive AST folding.
- [ ] HTML JS and HTML-Wasm behavior is implemented or has structured unsupported diagnostics where unavoidable.
- [ ] Docs, roadmap, and progress matrix describe the final narrow design.
- [ ] `just validate` passes.
