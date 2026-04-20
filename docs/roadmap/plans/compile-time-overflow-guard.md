# Plan: Graceful Compile-Time Numeric Overflow Diagnostics

## Objective

Add structured compiler diagnostics for **compile-time numeric overflow** so constant folding never crashes with a Rust debug panic.

The main fix belongs in the AST constant folding path in `src/compiler_frontend/optimizers/constant_folding.rs`, which is where compile-time arithmetic is currently evaluated . Existing constant-folding unit tests already live in `src/compiler_frontend/optimizers/tests/constant_folding_tests.rs`, so this change should extend that test surface rather than creating a parallel one .

---

## Goal

When a user writes something like:

```beanstalk
# value = 2 ^ 63
```

the compiler should produce a **graceful compile-time error** such as:

> Compile-time integer overflow while evaluating '^'

It must **not**:

* panic in debug builds
* silently wrap in release builds
* leak Rust parser/runtime behaviour into frontend diagnostics

---

## Scope

### In scope

* Constant-folded integer overflow checks
* Constant-folded float non-finite checks (`inf`, `-inf`, `NaN`)
* Constant cast range checks during compile-time folding
* One lexer guard for oversized float literals
* Unit and integration regression tests

### Out of scope

* Changing runtime overflow semantics
* Reworking the numeric type system
* Adding bigint/arbitrary-precision folding
* Redesigning unary negative literal parsing
* Changing integer width or float width
* Adding a compiler-wide “overflow pass”

---

## Files to Touch

### Primary

* `src/compiler_frontend/optimizers/constant_folding.rs` 

### Tests

* `src/compiler_frontend/optimizers/tests/constant_folding_tests.rs` 
* one integration fixture under the existing `tests/cases/` matrix

### Secondary hardening

* `src/compiler_frontend/tokenizer/lexer.rs` for oversized float literal rejection 
* optionally `src/compiler_frontend/ast/expressions/parse_expression_literals.rs` for safe negation of parsed int literals if the agent chooses to close that edge now 

---

## Design Decision

Do **not** add a generic “guard before Rust folds a number” somewhere else in the compiler.

The fold site is already the place where Rust arithmetic is being used. The correct design is:

* keep overflow checks **inside constant folding**
* make the fold evaluator use **checked arithmetic**
* convert failures into **frontend diagnostics**

This keeps the logic local, avoids duplicate checks, and matches the AST-stage ownership of compile-time folding.

---

## Implementation Phases

## Phase 1 — Audit and Lock Down All Current Panic Paths

### Target

`src/compiler_frontend/optimizers/constant_folding.rs`

### Current risky operations

In `Expression::evaluate_operator`, integer folding currently uses raw Rust arithmetic. These are the panic/wrap candidates:

* `lhs_val + rhs_val`
* `lhs_val - rhs_val`
* `lhs_val * rhs_val`
* `lhs_val / rhs_val`
* `lhs_val % rhs_val`
* `lhs_val.pow(*rhs_val as u32)`

### Required result

Every integer fold path must become:

* deterministic
* checked
* diagnostic-producing on failure

### Agent tasks

* find every integer operation in `evaluate_operator`
* replace raw arithmetic with checked equivalents
* ensure each failure returns a structured frontend error, not `panic!`

---

## Phase 2 — Add Checked Integer Folding Helpers

### Target

`src/compiler_frontend/optimizers/constant_folding.rs`

### Add small local helpers

Do not create a new cross-compiler utility module yet. Keep the helpers local to constant folding.

Suggested helper split:

* `checked_int_binary_result(...)`
* `checked_float_result(...)`
* optionally small helpers like:

  * `integer_overflow_error(...)`
  * `float_non_finite_error(...)`

### Behaviour requirements

#### `Add`

Use `checked_add`

#### `Subtract`

Use `checked_sub`

#### `Multiply`

Use `checked_mul`

#### `IntDivide`

Use `checked_div`

This catches both:

* divide by zero
* `i64::MIN / -1` overflow

#### `Modulus`

Use `checked_rem`

This catches:

* modulus by zero
* `i64::MIN % -1` overflow edge

#### `Exponent` with `Int ^ Int`

* if exponent `< 0`, preserve current behaviour of promoting to float
* if exponent `>= 0`, use `checked_pow(rhs as u32)`

### Required diagnostics

#### Integer overflow

Message shape:

* `Compile-time integer overflow while evaluating '+'`
* `Compile-time integer overflow while evaluating '^'`

Metadata should include:

* `CompilationStage => "Constant Folding"`
* a practical suggestion such as:

  * `Reduce the value range or compute this at runtime instead`

### Important

Do not silently change semantics.

* `Int / Int` should still fold to `Float` if that is current language behaviour
* `Int // Int` should still fold to `Int`
* only the failure mode changes

---

## Phase 3 — Add Float Non-Finite Guards

### Target

`src/compiler_frontend/optimizers/constant_folding.rs`

Rust float arithmetic will not panic, but compile-time folding can still produce:

* `inf`
* `-inf`
* `NaN`

That should not be allowed to slip through silently unless the language explicitly wants non-finite compile-time values.

### Operations to guard

Every folded float result should be validated with:

```rust
value.is_finite()
```

This includes:

* `Float op Float`
* `Int op Float`
* `Float op Int`
* negative integer exponentiation promoted to float
* float casts from strings

### Required result

If the folded float is not finite:

* return a structured compile-time error
* do not emit an `ExpressionKind::Float`

### Required diagnostics

Message shape:

* `Compile-time float overflow or non-finite result while evaluating '^'`
* `Compile-time float overflow or non-finite result while evaluating '*'`

Suggestion:

* `Use smaller values or compute this at runtime instead`

### Notes

This is not only about overflow.
`NaN` should also be rejected here for compile-time folding consistency unless the language has decided otherwise.

---

## Phase 4 — Harden Compile-Time Casts

### Target

Still `src/compiler_frontend/optimizers/constant_folding.rs`

### Functions to update

* `eval_int_cast`
* `eval_float_cast`

### Problems to fix

#### `Float -> Int`

Current behaviour uses Rust casts after an exact-integer check. That is not enough.

Before converting a float to `i64`, require:

* finite
* within `i64` range
* exact integer value

### Required checks for `Float -> Int`

For both direct float expressions and string-parsed float expressions:

1. `value.is_finite()`
2. `value >= i64::MIN as f64`
3. `value <= i64::MAX as f64`
4. `value.fract() == 0.0`

Only then allow the cast.

### Required diagnostics

Examples:

* `Cannot cast Float 1e309 to Int because it is not finite`
* `Cannot cast Float 9223372036854775808.0 to Int because it exceeds Int range`
* `Cannot cast Float 1.5 to Int because it is not an exact integer value`

### `String -> Float`

After parsing string text to `f64`, also reject non-finite values.

### `String -> Int`

The existing `parse::<i64>()` path is already structurally safe. No wider redesign needed here.

---

## Phase 5 — Reject Oversized Float Literals in the Lexer

### Target

`src/compiler_frontend/tokenizer/lexer.rs` 

### Current behaviour

Numeric literal tokenization parses float literals with `parse::<f64>()` and accepts the parsed value directly.

That can allow a tokenized float literal to become non-finite before constant folding ever sees it.

### Required change

After `parse::<f64>()`, add:

* `if !parsed_value.is_finite() { ...error... }`

### Diagnostic

Message shape:

* `Float literal '...' is too large`
* or `Float literal '...' is not finite`

Suggestion:

* `Use a smaller float literal`

### Why this belongs in the lexer

This is a literal token validity issue, not a folding issue.

---

## Phase 6 — Decide Whether to Close the Negative-Literal Edge Now

### Optional but recommended

`src/compiler_frontend/ast/expressions/parse_expression_literals.rs` currently applies negative sign to parsed int literals after tokenization .

### Risk

If the code ever reaches a case equivalent to negating the minimum representable edge unsafely, it should not rely on raw unary negation.

### Minimal safe action

If the agent touches this file:

* replace raw `int = -int` with `checked_neg()`
* emit a structured parser-stage or rule-stage error on failure

### Important

Do **not** widen this into a redesign of how minimum signed integer literals are tokenized. That is separate.

---

## Phase 7 — Add Unit Tests

### Target

`src/compiler_frontend/optimizers/tests/constant_folding_tests.rs` 

### Add these unit tests

#### Integer overflow tests

* `evaluate_operator_rejects_integer_add_overflow`
* `evaluate_operator_rejects_integer_subtract_overflow`
* `evaluate_operator_rejects_integer_multiply_overflow`
* `evaluate_operator_rejects_integer_exponent_overflow`

Suggested cases:

* `i64::MAX + 1`
* `i64::MIN - 1`
* `i64::MAX * 2`
* `2 ^ 63`

#### Integer division overflow edge

* `evaluate_operator_rejects_integer_division_overflow`

Case:

* `i64::MIN // -1`

#### Integer remainder overflow edge

* `evaluate_operator_rejects_integer_modulus_overflow`

Case:

* `i64::MIN % -1`

#### Float non-finite tests

* `evaluate_operator_rejects_non_finite_float_exponent_result`
* `evaluate_operator_rejects_non_finite_float_multiply_result`

Suggested cases:

* a very large exponent
* a very large multiply

#### Cast tests

* `eval_int_cast_rejects_out_of_range_float`
* `eval_int_cast_rejects_non_finite_float`
* `eval_float_cast_rejects_non_finite_string_value`

### Assertions

Do not assert full error rendering text if that makes tests brittle.
Assert on:

* error returned
* key message fragment
* operation-specific fragment where useful

---

## Phase 8 — Add an Integration Regression Case

### Target

Add one canonical frontend failure case under the existing integration fixture layout.

### Suggested case

A small case with:

```beanstalk
# value = 2 ^ 63
```

### Expected outcome

* compilation fails
* error category is the frontend’s user-facing category for this case
* message includes `Compile-time integer overflow`

### Optional second integration case

A float overflow case, such as an extremely large exponent or literal, only if the fixture cost is low.

### Why an integration case matters

Unit tests prove helper correctness.
An integration test proves the compiler now surfaces the failure properly through the real pipeline.

---

## Exact Behaviour Requirements

## Integer constant folding

### Must error

* `i64::MAX + 1`
* `i64::MIN - 1`
* `i64::MAX * 2`
* `2 ^ 63`
* `i64::MIN // -1`
* `i64::MIN % -1`

### Must still work

* normal in-range integer operations
* negative exponent promotion to float
* divide by zero existing diagnostics
* normal comparison folding
* string concatenation folding
* non-overflowing casts

---

## Float constant folding

### Must error

Any compile-time fold result that is:

* `NaN`
* `inf`
* `-inf`

### Must still work

* ordinary finite float arithmetic
* mixed int/float arithmetic within range
* valid float string parsing

---

## Diagnostics Contract

Use user-facing structured diagnostics.

### Do not use

* `panic!`
* `.unwrap()`
* `expect()` on user-controlled numeric evaluation paths
* compiler-bug error category

### Do use

* existing frontend error helpers already used in constant folding
* operation-specific messages
* stage metadata set to `Constant Folding` where appropriate

### Preferred message wording

#### Integer overflow

`Compile-time integer overflow while evaluating '{op}'`

#### Float non-finite

`Compile-time float overflow or non-finite result while evaluating '{op}'`

#### Float literal too large

`Float literal '{literal}' is too large`

#### Int cast out of range

`Cannot cast Float {value} to Int because it exceeds Int range`

---

## Non-Goals and Guardrails

The agent should **not**:

* add a bigint dependency
* redesign expression evaluation
* change runtime codegen overflow behaviour
* widen this into arbitrary-precision compile-time math
* introduce a new compiler-wide numeric abstraction unless clearly required
* add compatibility layers or wrapper APIs

Keep the patch local and direct.

---

## Acceptance Criteria

The change is complete when all of the following are true:

* compile-time numeric overflow no longer causes Rust debug panics
* integer folding uses checked arithmetic for overflow-prone operations
* float folding rejects non-finite results
* compile-time numeric casts are range-checked
* oversized float literals are rejected before entering later stages
* new unit tests cover overflow and non-finite cases
* at least one integration fixture proves the full compiler emits a graceful diagnostic
* `cargo clippy`
* `cargo test`
* `cargo run tests`

all pass after the change

---

## Recommended Implementation Order

1. update integer fold operations in `constant_folding.rs`
2. add float finiteness guard helper
3. harden `eval_int_cast` and `eval_float_cast`
4. add lexer float literal finiteness rejection
5. add unit tests
6. add one integration fixture
7. run full validation
8. only then consider the optional `checked_neg` cleanup in literal parsing

---

## Agent Notes

* Keep helper names explicit and local.
* Prefer small functions over inflating `evaluate_operator`.
* Preserve current semantics wherever possible.
* Treat this as a **frontend correctness and diagnostics** fix, not a numeric-language redesign.
* If there is any uncertainty about runtime semantics, do not change them as part of this patch.
* If the minimum-negative-literal edge becomes distracting, leave it as a documented follow-up unless it is required to make this patch safe.