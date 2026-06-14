# Beanstalk Expression Refactor and Checked Numeric Semantics Implementation Plan

## Scope

This implementation covers four linked changes:

1. narrow expression representation so expression trees no longer carry broad `AstNode` fragments;
2. move numeric literal classification, sign metadata, exponent syntax, and symbolic-operator spacing checks into the tokenizer/front-loaded syntax layer;
3. define Alpha numeric runtime semantics: `Int = i32`, finite `Float = f64`, checked numeric operations, and context-sensitive recoverability through builtin `Error!` only;
4. define one Beanstalk-owned Float formatting and numeric-text grammar shared by source literals, string casts, AST folding, templates, JS lowering, and Wasm/runtime lowering or validation.

Out of scope:

- implicit arbitrary-precision constant folding;
- runtime `BigInt` or `Decimal`;
- rational/decimal const facts;
- numeric check elision or range analysis;
- explicit unchecked/narrow numeric types.

Future precision work should be explicit-type design: `BigInt`, `Decimal`, or a different high-precision numeric type.

## Current repo anchors

Recheck these before implementation starts because the repo may move:

- `src/compiler_frontend/tokenizer/numeric.rs` currently parses numeric literals into runtime-shaped `i64` / `f64` values.
- `src/compiler_frontend/tokenizer/tokens.rs` currently stores `TokenKind::IntLiteral(i64)` and `TokenKind::FloatLiteral(f64)`.
- `src/compiler_frontend/ast/expressions/parse_expression_literals.rs` currently builds `Expression::int(i64)` / `Expression::float(f64)` directly from tokens.
- `src/compiler_frontend/ast/expressions/expression_kind.rs` currently stores broad AST fragments in `ExpressionKind::Runtime(Vec<AstNode>)` and `ExpressionKind::Copy(Box<AstNode>)`.
- `src/compiler_frontend/ast/ast_nodes.rs` currently mixes statement nodes with expression-shaped nodes such as `Rvalue`, `Operator`, field access, function calls, method calls, collection/map builtin calls, and host calls.
- `AstNode::get_expr_with_optional_type_environment` currently reconstructs expressions from broad `NodeKind` variants and sometimes wraps `self.to_owned()` back into runtime RPN.
- `src/compiler_frontend/optimizers/constant_folding.rs` currently owns AST-time constant folding, but this behavior belongs under AST const evaluation.
- `src/compiler_frontend/hir/expressions.rs` currently stores `HirExpressionKind::Int(i64)`, `Float(f64)`, `BinOp`, and `UnaryOp`.
- `src/compiler_frontend/hir/statements.rs` already has statement-shaped compiler-owned operations such as `CastOp` and `MapOp`; checked numeric operations should follow this pattern.
- `src/compiler_frontend/hir/operators.rs` currently has plain operator enums with no checked numeric failure metadata.

## Design invariants

- Tokenizer owns lexical numeric shape and readability diagnostics, not semantic numeric materialization.
- AST owns semantic type resolution, expression parsing, contextual coercion, constant folding, and compile-time diagnostics.
- HIR owns explicit checked numeric effects, selected failure mode, and backend-facing control flow.
- Backends must preserve Beanstalk numeric semantics or reject reachable unsupported paths before lowering.
- `ExpressionKind::ValueBlock` is the only expression variant allowed to carry statement bodies.
- Source numeric operators type as plain `Int` or `Float`; recoverable numeric failure is internal control flow, not source-visible operator `Error!`.
- Automatic numeric failure recovery applies only when the enclosing function has builtin `Error!` exactly as its error return slot.
- Functions with custom fallible channels such as `String!`, `Bool!`, or user-defined error choices use trap mode for numeric failures.
- Top-level runtime/start code uses trap mode.
- Statically known numeric failures are compile-time diagnostics everywhere, including inside builtin `Error!` functions.

## Shared validation gate for every phase

At the end of each implementation phase:

- [ ] Run the narrowest relevant unit/integration tests for touched modules.
- [ ] Run `just validate` when code changed.
- [ ] Perform the style-guide review: readable control flow, no clever iterator-heavy validation, clear stage ownership, no obsolete wrappers, no stale comments, no suppressed lints without justification.
- [ ] Perform frontend boundary review: user-facing failures use `CompilerDiagnostic`; infrastructure failures use `CompilerError`; semantic types use `TypeId`; `DataType` stays diagnostic/parse-only.
- [ ] Search for newly introduced `unwrap`, `todo!`, `panic!`, broad inline imports, and compatibility shims.

Phase-specific gates below are additions to this shared gate.

---

## Phase 0 — Baseline recheck and diagnostic inventory

### Context

Lock the current repo shape, diagnostics, and public semantics before large structural work begins. This prevents later phases from inventing ad hoc diagnostics or preserving stale APIs.

### Checklist

- [ ] Reconfirm the current files listed in **Current repo anchors**.
- [ ] Map all `i64` usages that represent Beanstalk `Int`; separate them from unrelated sizes, indices, byte counts, or host values.
- [ ] Map current builtin cast policies and builtin error-code definitions.
- [ ] Map JS and Wasm numeric lowering owners.
- [ ] Add or reserve structured diagnostic reasons for:
  - uppercase exponent marker;
  - missing exponent digits;
  - invalid exponent sign placement;
  - invalid numeric separator placement;
  - invalid symbolic binary operator spacing;
  - invalid unary negation spacing;
  - unsupported unary plus;
  - numeric literal outside `Int` range;
  - non-finite `Float`;
  - compile-time numeric overflow;
  - compile-time divide/modulo by zero;
  - invalid exponent;
  - unsupported backend checked numeric feature.
- [ ] Add or reserve builtin runtime error codes for:
  - `IntOverflow`;
  - `DivideByZero`;
  - `InvalidExponent`;
  - `FloatNonFinite`;
  - `FloatBoundaryNonFinite`;
  - `FloatFormatInvariant` or equivalent defensive formatting failure.

### Phase-specific gate

- [ ] Confirm no new diagnostic stores pre-rendered prose where a structured reason enum is appropriate.
- [ ] Confirm stable diagnostic codes are planned for integration failures.

---

## Phase 1 — Shared numeric text grammar and tokenizer front-loading

### Context

Create one small grammar owner for Beanstalk numeric text, then use it from the tokenizer and later from string casts. This avoids duplicating separator, exponent, sign, and lowercase-`e` rules in several places.

### Target shape

Add a pure frontend grammar module with no AST/HIR/backend dependencies, for example:

```text
src/compiler_frontend/numeric_text/
    mod.rs
    grammar.rs
    token.rs
    parse.rs
    diagnostics.rs
```

Approximate token payload:

```rust
pub enum NumericLiteralKind {
    WholeNumber,
    DecimalPoint,
    Exponent,
}

pub enum NumericLiteralSign {
    Positive,
    Negative,
}

pub enum NumericExponentSign {
    None,
    Positive,
    Negative,
}

pub struct NumericLiteralToken {
    pub sign: NumericLiteralSign,
    pub normalized_text: StringId,
    pub kind: NumericLiteralKind,
    pub digit_count: u32,
    pub fractional_digit_count: u32,
    pub exponent_digit_count: u32,
    pub exponent_sign: NumericExponentSign,
}
```

`normalized_text` is unsigned, underscore-free, and includes lowercase exponent syntax when present, for example `"1"`, `"1.5"`, `"1e6"`, `"1.0e-6"`.

### Checklist

- [ ] Replace `TokenKind::IntLiteral(i64)` and `TokenKind::FloatLiteral(f64)` with `TokenKind::NumericLiteral(NumericLiteralToken)`.
- [ ] Update token remapping for `NumericLiteralToken.normalized_text`.
- [ ] Rewrite tokenizer numeric scanning to use the shared numeric grammar module.
- [ ] Support signed numeric literal tokens only for attached leading `-` in prefix position:
  - `-1`;
  - `-1.5`;
  - `-1e6`.
- [ ] Reject unary plus as unsupported syntax.
- [ ] Support lowercase exponent literals:
  - `1e6`;
  - `1e-6`;
  - `1e+6`;
  - `1.0e+21`.
- [ ] Reject uppercase `E` with a diagnostic suggesting lowercase `e`.
- [ ] Validate underscores in integer, fractional, and exponent sections.
- [ ] Count total digits, fractional digits, and exponent digits.
- [ ] Keep tokenizer from parsing to `i32` or `f64`.
- [ ] Add a lexer helper for “can the previous emitted token end an expression?” so `-` can be classified as signed literal, unary negation, binary operator, or spacing error.
- [ ] Enforce whitespace around symbolic binary operators:
  - `+`, `-`, `*`, `/`, `//`, `%`, `^`, `=`, `~=`, `<`, `<=`, `>`, `>=`.
- [ ] Do not apply binary spacing diagnostics to word operators/keywords or punctuation delimiters.
- [ ] Enforce unary negation attachment:
  - valid: `-count`, `-1`;
  - invalid: `- count`, `- 1`.
- [ ] Diagnose `a-1` and `a*-1` as spacing violations instead of interpreting `-1` as a signed literal.
- [ ] Preserve valid `a * -1`.
- [ ] Update lexer, token remap, and numeric tests.

### Tests

- [ ] Signed whole-number, decimal, and exponent tokens.
- [ ] Uppercase `E` diagnostic.
- [ ] Missing exponent digits: `1e`, `1e+`, `1e-`.
- [ ] Bad separators: `_1`, `1_`, `1__0`, `1_e2`, `1e_2`, `1e+_2`.
- [ ] Binary spacing failures: `a+b`, `a-1`, `a//b`, `count=1`, `count~=1`.
- [ ] Unary spacing failures: `- count`, `- 1`.
- [ ] Unary plus rejection.

### Phase-specific gate

- [ ] Confirm tokenizer diagnostics are syntax diagnostics.
- [ ] Confirm tokenizer only owns lexical shape and spacing, not semantic numeric range or finite checks.

---

## Phase 2 — Narrow expression and place representation

### Context

Introduce expression-only structures before migrating parser logic. This reduces noisy `NodeKind` conversions and gives constant folding/HIR lowering a smaller input language.

### Target shape

Prefer fewer, broader expression contracts over many parallel call variants. Use one call family where practical:

```rust
pub struct ExpressionRpn {
    pub items: Vec<ExpressionRpnItem>,
}

pub enum ExpressionRpnItem {
    Operand(Expression),
    Operator(Operator),
}

pub struct PlaceExpression {
    pub kind: PlaceExpressionKind,
    pub type_id: TypeId,
    pub diagnostic_type: DataType,
    pub value_mode: ValueMode,
    pub location: SourceLocation,
}

pub enum PlaceExpressionKind {
    Local(InternedPath),
    Field {
        base: Box<PlaceExpression>,
        field: StringId,
    },
}
```

Expression calls can be consolidated behind one expression-owned call target instead of duplicating many `NodeKind` variants:

```rust
pub enum ExpressionCallTarget {
    SourceFunction(InternedPath),
    ExternalFunction(ExternalFunctionId),
    ReceiverMethod {
        receiver: Box<Expression>,
        method_path: InternedPath,
        method: StringId,
    },
    CollectionBuiltin {
        receiver: Box<Expression>,
        op: CollectionBuiltinOp,
        receiver_requires_mutable: bool,
    },
    MapBuiltin {
        receiver: Box<Expression>,
        op: MapBuiltinOp,
        receiver_requires_mutable: bool,
    },
}
```

Exact names can differ. The important invariant is that expression contexts no longer store general `AstNode`.

### Checklist

- [ ] Add `ExpressionRpn` and `ExpressionRpnItem`.
- [ ] Add `PlaceExpression` and `PlaceExpressionKind`.
- [ ] Add expression-owned field access and call target contracts.
- [ ] Replace `ExpressionKind::Runtime(Vec<AstNode>)` with `ExpressionKind::Runtime(ExpressionRpn)`.
- [ ] Replace `ExpressionKind::Copy(Box<AstNode>)` with `ExpressionKind::Copy(PlaceExpression)`.
- [ ] Keep `ExpressionKind::ValueBlock { .. }` as the only expression variant allowed to carry statement bodies.
- [ ] Add type/location/value-mode helper methods on expression/place structures.
- [ ] Add string-ID remapping for new structures.
- [ ] Add debug/assertion validation that non-`ValueBlock` expression variants do not contain statement bodies.
- [ ] Update `ast/expressions` module docs to state the boundary.

### Phase-specific gate

- [ ] Confirm new expression contracts are frontend-only and do not import HIR/backend concepts.
- [ ] Confirm there is no compatibility wrapper preserving old `Vec<AstNode>` runtime expression APIs.

---

## Phase 3 — Migrate expression parser, operator typing, and runtime RPN to narrow types

### Context

Move the actual expression pipeline off broad `AstNode`. This should remove the need for expression-shaped `NodeKind` variants and the expensive `AstNode::get_expr` reconstruction paths.

### Checklist

- [ ] Change expression parser stacks from `Vec<AstNode>` to `ExpressionRpn` / `Vec<ExpressionRpnItem>`.
- [ ] Convert literal parsing to emit `Expression` operands directly.
- [ ] Convert identifier/reference parsing to emit `Expression` operands directly.
- [ ] Convert source/external/function/member/builtin call parsing to expression-owned call structures.
- [ ] Convert postfix field access to expression-owned field access.
- [ ] Convert mutable receiver and copy parsing to `PlaceExpression`.
- [ ] Convert ordering/shunting-yard logic to `ExpressionRpnItem`.
- [ ] Convert expression result-type resolution to `ExpressionRpnItem`.
- [ ] Convert const folding inputs to `ExpressionRpn`.
- [ ] Convert HIR runtime RPN lowering to `ExpressionRpn`.
- [ ] Remove or rewrite `AstNode::get_expr_with_optional_type_environment` so expression parsing no longer depends on it.
- [ ] Remove expression-shaped `NodeKind` variants once all callers are migrated:
  - `Rvalue`;
  - `Operator`;
  - field access;
  - function call variants;
  - method call;
  - collection/map builtin calls;
  - host call variants.
- [ ] Keep statement nodes in `AstNode`: declarations, assignment, control flow, returns, loops, matches, assertions, `ThenValue`, runtime-fragment push, and expression statements.
- [ ] Add or update expression parser tests for literals, references, function calls, receiver calls, collection/map builtins, field access, copy, mutable access, and mixed operators.

### Phase-specific gate

- [ ] Search for `ExpressionKind::Runtime(Vec<AstNode>)`, `ExpressionKind::Copy(Box<AstNode>)`, `NodeKind::Rvalue`, and `NodeKind::Operator`; none should remain in active expression paths.
- [ ] Confirm `ValueBlock` is the only remaining expression-to-statement-body bridge.

---

## Phase 4 — AST-owned const evaluation with runtime-parity semantics

### Context

Constant folding is semantic validation, not a generic optimizer. Move it under AST and keep it equivalent to runtime `i32` / finite-`f64` behavior.

### Target module

```text
src/compiler_frontend/ast/const_eval/
    mod.rs
    numeric.rs
    casts.rs
    rpn.rs
    diagnostics.rs
```

### Checklist

- [ ] Move folding out of `src/compiler_frontend/optimizers/constant_folding.rs`.
- [ ] Delete the old optimizer owner and module exports.
- [ ] Add AST const-eval module docs explaining:
  - AST owns semantic folding;
  - folding is runtime-parity only;
  - no arbitrary precision/rational/Decimal/BigInt behavior is implemented.
- [ ] Materialize `NumericLiteralKind::WholeNumber` to `i32` with sign-aware range checks:
  - `2147483647` valid;
  - `2147483648` invalid;
  - `-2147483648` valid;
  - `-2147483649` invalid.
- [ ] Materialize `DecimalPoint` and `Exponent` to finite `f64`.
- [ ] Fold `Int` arithmetic using checked `i32` operations.
- [ ] Fold `Float` arithmetic using finite `f64` operations.
- [ ] Fold mixed `Int`/`Float` arithmetic using the same conversions HIR/runtime will use.
- [ ] Fold `Int / Int -> Float` through `f64` conversion and finite division.
- [ ] Fold comparisons and boolean operations using existing operator typing policy.
- [ ] Produce compile-time diagnostics for statically known numeric failures anywhere in source.
- [ ] Update config and template folding consumers to use `ast::const_eval`.
- [ ] Keep partial folding conservative where folding would alter runtime-dependent numeric behavior.

### Tests

- [ ] i32 boundary literals.
- [ ] checked const overflow.
- [ ] compile-time divide/modulo by zero.
- [ ] invalid negative `Int` exponent.
- [ ] exponent literal materialization.
- [ ] finite `Float` rejection.
- [ ] mixed numeric folding.
- [ ] `Int / Int -> Float` folding.
- [ ] no precise decimal semantics claimed by tests.

### Phase-specific gate

- [ ] Confirm no `BigInt`, `Decimal`, rational, or high-precision dependency was added.
- [ ] Confirm HIR receives only materialized runtime values or runtime RPN.

---

## Phase 5 — Alpha `Int = i32`, casts, and Decimal quarantine

### Context

Replace old `i64`/JS-safe-integer assumptions with `i32` throughout runtime-facing compiler data. Quarantine inactive Decimal scaffold so it cannot affect active type/operator policy.

### Checklist

- [ ] Change `ExpressionKind::Int(i64)` to `ExpressionKind::Int(i32)`.
- [ ] Change `Expression::int` and related constructors to `i32`.
- [ ] Change `HirExpressionKind::Int(i64)` to `HirExpressionKind::Int(i32)`.
- [ ] Change `BuiltinCastLiteral::Int(i64)` to `BuiltinCastLiteral::Int(i32)`.
- [ ] Change builtin `Error.code` representation to `i32`.
- [ ] Replace every old JS-safe integer policy with i32 range policy.
- [ ] Update `Float -> Int` cast:
  - source must be finite;
  - truncate toward zero;
  - fail if outside i32 range.
- [ ] Update `String -> Int` cast to use shared numeric text grammar:
  - whole-number grammar only;
  - underscores accepted when valid;
  - lowercase `e` is not valid for `Int` strings;
  - no unary plus;
  - result must fit i32.
- [ ] Update `Char -> Int` to produce i32.
- [ ] Update `Bool -> Int` only if already supported in the current cast table; do not add new cast surface incidentally.
- [ ] Locate `DataType::Decimal`, `BuiltinTypeKey::Decimal`, `builtin_type_ids::DECIMAL`, `BuiltinTypes::decimal`, and Decimal operator policy.
- [ ] Remove Decimal from active parse/type/operator paths where practical.
- [ ] If removal is too invasive, quarantine it with TODO comments and deferred-feature diagnostics:
  - no parser access;
  - no inference path;
  - no operator policy;
  - no HIR/backend lowering;
  - no const folding.

### Tests

- [ ] `2147483647` valid.
- [ ] `2147483648` diagnostic.
- [ ] `-2147483648` valid.
- [ ] `-2147483649` diagnostic.
- [ ] `String -> Int` i32 range failure.
- [ ] `Float -> Int` i32 range failure.
- [ ] `Decimal` cannot be authored/inferred/used.

### Phase-specific gate

- [ ] Search all `i64` occurrences; every remaining Beanstalk-`Int` carrier must be replaced or justified.
- [ ] Confirm docs/tests no longer mention JS-safe integer semantics.

---

## Phase 6 — HIR checked numeric operations and context-sensitive recoverability

### Context

Checked numeric operations are semantic HIR effects. Backends must consume explicit HIR facts and must not rediscover source context.

### Target shapes

```rust
pub enum NumericFailureMode {
    ReturnError,
    Trap,
}

pub enum HirNumericOp {
    IntAdd,
    IntSub,
    IntMul,
    IntDiv,
    IntMod,
    IntPow,
    IntNeg,
    FloatAdd,
    FloatSub,
    FloatMul,
    FloatDiv,
    FloatMod,
    FloatPow,
    FloatNeg,
}
```

A simple HIR statement shape is preferred:

```rust
pub enum HirStatementKind {
    NumericOp {
        op: HirNumericOp,
        failure_mode: NumericFailureMode,
        operands: HirNumericOperands,
        result: LocalId,
        failure: Option<LocalId>, // only if needed for ReturnError branching
    },
}
```

The exact carrier can differ, but it must be HIR-only and not source-visible.

### Checklist

- [ ] Add `NumericFailureMode`.
- [ ] Add `HirNumericOp`.
- [ ] Add statement-shaped `NumericOp`.
- [ ] Add HIR builder helper such as `emit_checked_numeric_value(...)` that returns the success `HirExpression` and emits the required preludes/branches.
- [ ] Select failure mode during HIR lowering:
  - builtin `Error!` return slot exactly -> `ReturnError`;
  - any other `!` slot -> `Trap`;
  - no `!` slot -> `Trap`;
  - entry `start` -> `Trap`.
- [ ] Lower all runtime `Int`/`Float` arithmetic through `NumericOp`.
- [ ] Lower numeric unary negation through `NumericOp`.
- [ ] Lower `Int ^ Int` as checked `Int` with non-negative exponent requirement.
- [ ] Lower mixed `Int`/`Float` arithmetic by explicitly converting `Int` operands to `Float`, then emitting `Float*` `NumericOp`.
- [ ] Lower `Int / Int -> Float` by converting both operands to `Float`, then emitting checked `FloatDiv`.
- [ ] Keep comparisons and boolean operators as plain HIR expressions where they cannot fail.
- [ ] For `ReturnError`, emit explicit success/failure HIR control flow before borrow validation.
- [ ] For `Trap`, emit checked operation that traps on failure without constructing recoverable `Error`.
- [ ] Make compiler-generated numeric work, especially range-loop counter updates, use the same checked numeric path.
- [ ] Add HIR validation that no runtime numeric arithmetic remains in plain `BinOp` / `UnaryOp` unless it is non-failing comparison/boolean work.

### Tests

- [ ] Builtin `Error!` function returns recoverable `Error` on overflow/divide-by-zero/non-finite Float.
- [ ] Custom error channel traps instead of returning user error.
- [ ] No-error function traps.
- [ ] Entry start/top-level runtime traps.
- [ ] Arithmetic failures in conditions, loops, templates, and assertions use enclosing failure mode.
- [ ] Compiler-generated range-loop numeric operations are checked.

### Phase-specific gate

- [ ] Confirm AST expressions do not store `NumericFailureMode`.
- [ ] Confirm recoverable numeric failure creates visible HIR CFG edges before borrow validation.
- [ ] Confirm numeric carrier/internal locals are not exposed as source `TypeId`, `DataType`, ABI, or user local annotations.

---

## Phase 7 — Finite `Float`, Float formatting, and Float boundary validation

### Context

Beanstalk `Float` is finite `f64`. Formatting is Beanstalk-owned and must be shared by casts and templates instead of inheriting target-native stringification.

### Formatting contract

```text
finite f64 only
shortest round-trippable decimal
exponent form when abs(value) >= 1e21 or 0 < abs(value) < 1e-6
lowercase e
positive exponents include +
-0.0 -> "0"
omit trailing .0
```

Examples:

```text
1.0       -> "1"
1.5       -> "1.5"
0.000001  -> "0.000001"
0.0000001 -> "1e-7"
1e21      -> "1e+21"
-0.0      -> "0"
```

### Checklist

- [ ] Add frontend Float formatting contract module used by AST folding.
- [ ] Decide implementation detail for shortest-roundtrip formatting:
  - use an explicit helper/crate and post-process to Beanstalk thresholds; or
  - use a carefully wrapped standard formatter only if tests prove it matches every contract case.
- [ ] Keep `Float -> String` infallible at source level because valid Beanstalk `Float` is finite.
- [ ] Add `HirStatementKind::FormatFloat` with `NumericFailureMode`.
- [ ] Add `HirStatementKind::ValidateFloat` or an equivalent explicit boundary-validation path.
- [ ] Lower `cast Float -> String` through `FormatFloat`, not generic `CastOp`.
- [ ] Lower runtime Float template interpolation through `FormatFloat`.
- [ ] Validate Float values entering from external/backend boundaries before exposing them as ordinary Beanstalk Float.
- [ ] Keep `String -> Float` as explicit cast handling, not automatic `NumericFailureMode`, but use the shared numeric text grammar and finite check.
- [ ] Add defensive invariant handling for unexpected non-finite Float in formatter/validation helpers.

### Tests

- [ ] Formatting contract output cases.
- [ ] Runtime template Float interpolation output.
- [ ] `cast Float -> String` output.
- [ ] `String -> Float` valid grammar: `"1.5"`, `"-1.5"`, `"1e6"`, `"1e+21"`, `"1e-6"`, `"1_000.5e-2"`.
- [ ] `String -> Float` invalid grammar: `"1E6"`, `"NaN"`, `"Infinity"`, `"-Infinity"`, `"+1.0"`.
- [ ] External/backend non-finite Float boundary validation where testable.

### Phase-specific gate

- [ ] Confirm no semantic path uses direct Rust/JS/native float stringification.
- [ ] Confirm Float formatting path is shared by casts and templates.

---

## Phase 8 — JS backend checked numeric helpers and formatter

### Context

HTML-JS is the mandatory backend implementation target for this contract. Alpha `Int` is stored as JavaScript `number` constrained to signed i32 by helpers. Do not introduce JS `BigInt` or boxed integers.

### Checklist

- [ ] Add JS helper for i32 validation/assertion.
- [ ] Add JS checked i32 helpers:
  - add, sub, mul, div, mod, pow, neg.
- [ ] Add JS finite Float helpers:
  - add, sub, mul, div, mod, pow, neg;
  - validate finite Float.
- [ ] Add `bst_format_float` implementing Beanstalk formatting contract.
- [ ] Implement trap-mode helper behavior.
- [ ] Implement return-error-mode helper/control-flow behavior using builtin `Error` and stable builtin error codes.
- [ ] Lower HIR `NumericOp`, `FormatFloat`, and `ValidateFloat` to helpers.
- [ ] Validate external package Float returns before use.
- [ ] Ensure runtime template Float interpolation uses `bst_format_float`.
- [ ] Ensure `cast Float -> String` uses `bst_format_float`.

### Tests

- [ ] i32 overflow trap.
- [ ] i32 overflow returns builtin `Error` in builtin `Error!` function.
- [ ] custom fallible channel traps.
- [ ] Float non-finite arithmetic trap/Error.
- [ ] formatter output parity.
- [ ] external Float boundary validation.

### Phase-specific gate

- [ ] Confirm no JS lowering emits unchecked arithmetic for Beanstalk numeric source operators or generated numeric work.
- [ ] Confirm no JS `String(float)`, template literal native interpolation, or implicit JS stringification is used for semantic Float formatting.

---

## Phase 9 — Wasm/backend parity or explicit reachable rejection

### Context

Every backend must preserve Beanstalk semantics. Incomplete backends should reject reachable unsupported checked numeric/formatting/Float-validation paths before lowering invalid artifacts.

### Checklist

- [ ] Identify Wasm support for checked i32 ops, finite-f64 ops, recoverable builtin `Error!` numeric failures, trap failures, Float formatting, and external Float validation.
- [ ] Implement equivalent helpers where runtime support exists.
- [ ] Otherwise add structured unsupported-backend diagnostics for reachable unsupported paths.
- [ ] Ensure unreachable functions/helpers do not block backend builds.
- [ ] Add backend matrix tests for JS success and Wasm success or structured rejection.
- [ ] Update progress matrix for Wasm support status.

### Phase-specific gate

- [ ] Confirm no backend silently lowers Beanstalk numeric operations to unchecked target-native arithmetic.
- [ ] Confirm unsupported paths are rejected before artifact emission.

---

## Phase 10 — Documentation, roadmap, and progress matrix

### Context

The language semantics change. Documentation, roadmap, and progress matrix updates are part of the implementation, not follow-up work.

### `docs/language-overview.md`

- [ ] Update numeric semantics:
  - `Int = i32`;
  - `Float = finite f64`;
  - source exponent syntax with lowercase `e` only;
  - signed numeric literal behavior;
  - symbolic binary operator spacing;
  - checked numeric operations;
  - builtin `Error!` numeric recovery vs trap mode.
- [ ] Update casts:
  - remove JS-safe integer policy;
  - `String -> Int` i32 range and numeric grammar;
  - `String -> Float` grammar and finite check;
  - `Float -> String` Beanstalk formatting contract.
- [ ] Update templates:
  - Float interpolation uses Beanstalk formatter.
- [ ] Update loops:
  - range-loop generated numeric operations are checked.
- [ ] Update error handling:
  - automatic numeric recovery applies only to builtin `Error!`.

### `docs/compiler-design-overview.md`

- [ ] Update tokenizer responsibilities.
- [ ] Update AST expression narrowing boundary.
- [ ] Update AST const-eval ownership.
- [ ] Update HIR `NumericOp`, `NumericFailureMode`, `FormatFloat`, and `ValidateFloat` responsibilities.
- [ ] Update backend checked numeric contract and Wasm rejection/implementation policy.

### `docs/memory-management-design.md`

- [ ] Note recoverable numeric failures create ordinary HIR control flow before borrow validation if this affects borrow/drop facts.

### Roadmap/progress

- [ ] Add deferred explicit `Decimal` / `BigInt` / high-precision numeric type design.
- [ ] Add deferred numeric check elision / range analysis.
- [ ] Add deferred trap-mode numeric lowering optimization.
- [ ] Add deferred explicit unchecked/narrow numeric types as opt-in safety/performance tradeoffs.
- [ ] Add Wasm checked numeric/formatting support status.
- [ ] Mark JS checked numeric semantics and Float formatting parity when complete.

### Phase-specific gate

- [ ] Confirm docs do not imply implicit precise folding.
- [ ] Confirm docs do not mention old JS-safe integer policy.
- [ ] Confirm docs match actual implemented backend support.

---

## Phase 11 — Integration tests and backend-stable goldens

### Context

Integration tests should prove language behavior through real Beanstalk snippets and backend outputs. Prefer diagnostic codes for failure cases.

### Success tests

- [ ] i32 boundary values.
- [ ] exponent Float literals.
- [ ] lowercase exponent formatting.
- [ ] Float template interpolation.
- [ ] `Float -> String` cast output.
- [ ] checked arithmetic inside builtin `Error!` function returns recoverable builtin `Error`.
- [ ] custom error channel does not recover numeric failures.
- [ ] range loops with checked generated increments.
- [ ] external Float boundary validation where practical.

### Failure tests

- [ ] uppercase `E`.
- [ ] malformed exponent.
- [ ] invalid separators.
- [ ] missing binary operator spaces.
- [ ] unary negation spacing.
- [ ] unary plus.
- [ ] i32 literal out of range.
- [ ] const overflow.
- [ ] const divide/modulo by zero.
- [ ] invalid integer exponent.
- [ ] non-finite Float literal/materialization.
- [ ] `String -> Int` out of range.
- [ ] `String -> Float` invalid grammar.
- [ ] Wasm unsupported checked numeric/formatting paths where deferred.

### Backend-stable formatting outputs

- [ ] `1`.
- [ ] `1.5`.
- [ ] `0.000001`.
- [ ] `1e-7`.
- [ ] `1e+21`.
- [ ] `0` for negative zero.

### Phase-specific gate

- [ ] Failure fixtures assert stable diagnostic codes wherever practical.
- [ ] Backend goldens do not rely on host-native numeric formatting.
- [ ] Run `cargo test`, `cargo run -- tests`, and `just validate`.

---

## Phase 12 — Final cleanup and obsolete path removal

### Context

After migration, remove stale paths. Do not preserve old APIs for compatibility.

### Checklist

- [ ] Remove obsolete broad expression reconstruction helpers.
- [ ] Remove obsolete expression-shaped `NodeKind` variants.
- [ ] Remove old runtime RPN `Vec<AstNode>` paths.
- [ ] Remove old `i64` Int utilities/tests where they represented Beanstalk `Int`.
- [ ] Remove old JS-safe integer docs/tests.
- [ ] Remove old native Float formatting paths.
- [ ] Remove old `optimizers/constant_folding` module exports.
- [ ] Search for stale comments referring to the old expression or numeric design.
- [ ] Search for compatibility wrappers and delete them.
- [ ] Update module maps and file-level docs.

### Phase-specific gate

- [ ] Apply the full style-guide checklist.
- [ ] Run `cargo clippy`.
- [ ] Run `cargo test`.
- [ ] Run `cargo run -- tests`.
- [ ] Run `just validate`.
- [ ] Perform explicit frontend boundary cleanup review.

## Acceptance criteria

- [ ] Numeric tokens carry lexical metadata, sign, normalized text, exponent information, and no `i32`/`f64` values.
- [ ] Lowercase exponent source syntax is supported; uppercase `E` is diagnosed.
- [ ] Symbolic binary operator spacing is enforced.
- [ ] Expressions no longer store broad `AstNode` fragments except `ValueBlock` statement bodies.
- [ ] AST constant folding is AST-owned and runtime-parity only.
- [ ] Beanstalk `Int` is `i32` end-to-end.
- [ ] Beanstalk `Float` is finite `f64`.
- [ ] Runtime numeric operations are checked.
- [ ] Builtin `Error!` functions recover numeric failures through builtin `Error`.
- [ ] Custom fallible channels and top-level runtime snippets trap on numeric failures.
- [ ] HIR explicitly represents checked numeric operations, Float formatting, and Float validation paths.
- [ ] Recoverable numeric failures create visible HIR control flow before borrow validation.
- [ ] JS backend implements checked numeric helpers and Beanstalk Float formatting.
- [ ] Wasm either implements parity or rejects reachable unsupported paths.
- [ ] String numeric casts use the Beanstalk numeric text grammar.
- [ ] Existing Decimal scaffold is quarantined or removed from active semantics.
- [ ] Roadmap/progress matrix documents deferred precision and numeric optimization work.
- [ ] `just validate` passes.

## Deferred work to document explicitly

- [ ] Numeric check elision / range analysis.
- [ ] More aggressive trap-mode lowering where recoverable `Error` construction is not needed.
- [ ] Explicit unchecked or narrower numeric types as future opt-in safety/performance tradeoffs.
- [ ] Explicit `Decimal`, `BigInt`, or high-precision numeric type design.
- [ ] Full Wasm parity for checked numeric helpers and Float formatting if not completed in this implementation.
