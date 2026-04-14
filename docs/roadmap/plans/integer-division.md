# Implementation plan: make `/` real division and `//` integer division

### Design decision

* `/` is always real division.
* `Int / Int` naturally evaluates to `Float`.
* `//` becomes integer division.
* `Int // Int` naturally evaluates to `Int`.
* In explicitly `Int` contexts, using `/` should produce a targeted type error suggesting `//` or `Int(...)`.
* `//=` should exist if `//` exists.
* The old `//` root operator should be removed and replaced later with an explicit builtin/function/method design.

### Current repo anchors

The current compiler shape already gives you clean ownership boundaries for this change:

* Contextual numeric coercion is still intentionally narrow and only handles `Int -> Float` at declaration/return sites in `src/compiler_frontend/type_coercion/numeric.rs` and `compatibility.rs`  
* The tokenizer currently treats `//` as `Root` and `//=` as `RootAssign` in `src/compiler_frontend/tokenizer/lexer.rs` and `tokens.rs`  
* Arithmetic typing still resolves `Int / Int` as `Int` in `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/arithmetic.rs` 
* Constant folding still performs integer division for `Int / Int` in `src/compiler_frontend/optimizers/constant_folding.rs` 
* HIR and JS backend still carry/lower `Root` as a real operator in `src/compiler_frontend/hir/hir_nodes.rs`, `hir_expression/operators.rs`, and `src/backends/js/js_expr.rs`   

## Phase 1: reclaim `//` in the tokenizer and AST

### Files

* `src/compiler_frontend/tokenizer/tokens.rs`
* `src/compiler_frontend/tokenizer/lexer.rs`
* `src/compiler_frontend/ast/expressions/expression.rs`
* `src/compiler_frontend/ast/expressions/parse_expression_dispatch.rs`

### Changes

* Rename token/operator concepts:

  * `TokenKind::Root` -> `TokenKind::IntDivide`
  * `TokenKind::RootAssign` -> `TokenKind::IntDivideAssign`
  * `Operator::Root` -> `Operator::IntDivide`
* Update lexer behavior:

  * `//` -> `IntDivide`
  * `//=` -> `IntDivideAssign`
* Update token helpers:

  * `is_assignment_operator()`
  * `continues_expression()`
* Update expression dispatch so `TokenKind::IntDivide` lowers to `Operator::IntDivide`
* Remove root-operator parsing entirely

### Notes

This should be a hard replacement, not a compatibility layer. Pre-alpha is the right time to delete the old syntax cleanly.

## Phase 2: change operator typing rules

### Files

* `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/arithmetic.rs`
* `src/compiler_frontend/ast/expressions/eval_expression/operator_policy/diagnostics.rs`

### New typing rules

* `Int + Int -> Int`
* `Int - Int -> Int`
* `Int * Int -> Int`
* `Int % Int -> Int`
* `Int / Int -> Float`
* `Int // Int -> Int`
* Mixed `Int`/`Float` arithmetic remains `Float`
* `//` should be `Int`-only for now

### Recommended restrictions

Reject:

* `Float // Float`
* `Int // Float`
* `Float // Int`

That keeps `//` simple and predictable.

### Diagnostics to add

When `/` appears in an explicitly `Int` context, emit a targeted type error like:

* “Regular division returns `Float`.”
* “Use `//` for integer division.”
* “Use `Int(...)` for an explicit conversion.”

That is much better than a generic expected/found message.

## Phase 3: keep contextual coercion narrow

### Files

* `src/compiler_frontend/type_coercion/compatibility.rs`
* `src/compiler_frontend/type_coercion/numeric.rs`

### Changes

Do not expand coercion policy.

Keep:

* implicit `Int -> Float` only
* only at contextual boundaries such as declarations and returns

Do not add:

* implicit `Float -> Int`
* general “expression-level float defaulting”
* special hidden coercion paths for `//`

### Why

The current frontend separation is good:

* operator typing decides the natural type of an expression
* contextual coercion applies afterwards only where the language explicitly allows it  

This change should preserve that architecture.

## Phase 4: fix constant folding to match runtime semantics

### File

* `src/compiler_frontend/optimizers/constant_folding.rs`

### Changes

Update constant folding so:

* `5 / 2` folds to `2.5` as `Float`
* `5 // 2` folds to `2` as `Int`

Add explicit support for `Operator::IntDivide`.

Keep zero-division checks for both operators.

### Recommended integer division rule

Use truncation toward zero.

Examples:

* `5 // 2 -> 2`
* `-5 // 2 -> -2`
* `5 // -2 -> -2`

That is the easiest rule to mirror consistently in Rust-style logic and in the JS backend.

### Important

Constant folding and runtime lowering must match exactly. This is not optional.

## Phase 5: update compound assignment

### File

* `src/compiler_frontend/ast/expressions/mutation.rs`

### Changes

Add support for:

* `//=`

Keep support for:

* `/=`

But change semantics:

* `x /= y` should only be valid when the target type can accept the division result
* `Int /= Int` should now fail because `/` produces `Float`
* `Int //= Int` should succeed
* `Float /= Int` should succeed

### Recommended behavior

```beanstalk
x Int ~= 10
x /= 4      -- error
x //= 4     -- ok, x becomes 2

y Float ~= 10
y /= 4      -- ok, y becomes 2.5
```

## Phase 6: thread the new operator through HIR

### Files

* `src/compiler_frontend/hir/hir_nodes.rs`
* `src/compiler_frontend/hir/hir_expression/operators.rs`
* `src/compiler_frontend/hir/hir_display.rs` if needed

### Changes

* Replace `HirBinOp::Root` with `HirBinOp::IntDiv`
* Map AST `Operator::IntDivide` to HIR `IntDiv`
* Update result-type inference:

  * `Div` may now produce `Float` even for two `Int` operands
  * `IntDiv` produces `Int`

### Important

The current HIR inference logic is still shaped around the old operator split, so this must be updated alongside AST typing, not later  

## Phase 7: update backend lowering

### Files

* `src/backends/js/js_expr.rs`
* any future Wasm/LIR operator-lowering sites

### Changes

* Keep `Div` lowered as `/`
* Add `IntDiv` lowering
* Remove root-operator lowering

### Recommended JS lowering

Lower integer division as truncation toward zero:

```text
Math.trunc(left / right)
```

That matches the recommended constant-folding rule.

### Why

The JS backend currently lowers `Div` as raw `/` and `Root` as `Math.pow(...)` . This is the exact place where backend semantics will drift if you do not update it.

## Test plan

Follow the existing repo preference for strong integration tests using real Beanstalk snippets and artifact/golden assertions, not just narrow unit tests 

### Unit tests

Add or update tests for:

* tokenization of `//`
* tokenization of `//=`
* `Int / Int -> Float`
* `Int // Int -> Int`
* invalid mixed `//` cases
* constant folding of `/`
* constant folding of `//`
* divide-by-zero for both
* `/=` on `Int` target failing
* `//=` on `Int` target succeeding

### Integration tests

Add cases for:

* top-level real division output
* top-level integer division output
* `Int` declaration rejecting `/`
* `Float` declaration accepting `/`
* `Int` return rejecting `/`
* `Int` return accepting `//`
* `Float /= Int`
* `Int /= Int` failure
* `Int //= Int` success
* mixed `Int`/`Float` real division
* invalid `//` mixed numeric usage

## Documentation updates

### `docs/language-overview.md`

Add or update a numeric semantics section.

Suggested content:

* Whole-number literals are `Int`
* Decimal literals are `Float`
* `+`, `-`, `*`, `%` preserve `Int` when both operands are `Int`
* `/` is real division and returns `Float`
* `//` is integer division and requires `Int` operands
* There is no implicit `Float -> Int`
* Use `//` for integer division
* Use `Int(...)` for explicit conversion when you really want one

Also update any operator tables/examples that still imply integer `/`.

### `docs/compiler-design-overview.md`

Update the “Type checking and coercion” section to reflect the new split:

* generic expression evaluation stays strict
* contextual coercion is still only `Int -> Float`
* `/` is an operator-owned typing rule, not contextual coercion
* `Int / Int` naturally evaluates to `Float`
* `//` is a separate integer-division operator

That keeps the docs aligned with the current compiler architecture rather than muddying the boundary between operator typing and contextual coercion 

### `docs/language-overview.md` and any syntax references mentioning roots

Remove operator-based root syntax.

Do not document the replacement root API in the same PR unless you are actually implementing it.

For this change, the cleaner move is:

* remove `//` as root syntax
* leave roots for a later explicit builtin/function/method design pass

## Cleanup checklist

* remove `Root` and `RootAssign` token names
* remove `Operator::Root`
* remove `HirBinOp::Root`
* remove JS lowering for root operator
* remove stale comments mentioning `//` as roots
* update any failing snapshots/goldens
* update diagnostics text that still describes `/` as integer-preserving