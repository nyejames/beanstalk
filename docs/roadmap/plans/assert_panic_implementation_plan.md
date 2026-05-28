# Beanstalk `assert` / panic-surface implementation plan — reviewed revision

## Review result

The original plan was broadly correct and repo-anchored, but it needed several implementation-level clarifications before handing it to a coding agent.

Main improvements in this revision:

- Treat `assert(false, ...)` as a **statically terminal statement** so it can be used for explicit unrecoverable stops in non-`Void` functions.
- Avoid modelling assertion messages as general HIR expressions while runtime message expressions are deferred. Store assertion messages as literal/compile-time text data, not value-producing expressions.
- Remove the current `Panic(None)` HIR placeholder ambiguity by adding an explicit invalid-after-lowering placeholder terminator.
- Prefer reusing existing diagnostics (`InvalidBuiltinCall`, `TypeMismatch`, `InvalidResultHandling`, `DeferredFeature`) before adding a bespoke `InvalidAssert` diagnostic family.
- Do not reserve or special-case bare `panic` unless the current branch already has public panic syntax. The required public removal target is old `#panic`/panic syntax, not every user-defined identifier named `panic`.
- Explicitly list the documentation, roadmap, and matrix updates for implemented and deliberately deferred features.
- Add cleanup checkpoints for every touched system: tokenizer, AST, HIR, borrow checker, JS/Wasm lowering, integration tests, docs, generated docs, and final audit.

## Goal

Replace the old source-level panic surface with a small, intentional, always-on `assert` statement intrinsic.

Public Beanstalk syntax:

```bst
assert(condition)
assert(condition, "message")
assert(false)
assert(false, "message")
```

`assert(false, "message")` is the only source-level spelling for an intentional unrecoverable runtime stop for now. Panics remain a runtime/compiler concept, but not a convenient source-level error-handling mechanism. Expected failures continue to use `Error!`, `catch`, and optional values.

## Non-negotiable semantic contract

- [ ] `assert` is a language-owned **statement intrinsic**, not a user function.
- [ ] `assert` is valid only in runtime statement position inside function bodies and the entry `start()` body.
- [ ] `assert` is always checked in dev and release builds.
- [ ] `assert` returns no value.
- [ ] `assert` cannot be assigned, passed, imported, aliased, used as a receiver method, or used in expression position.
- [ ] `assert` does not produce `Error!`, `Result`, or `Option`.
- [ ] Failed `assert` is not catchable by Beanstalk `catch`.
- [ ] `assert(condition)` fails with a default message such as `"assertion failed"`.
- [ ] `assert(condition, "message")` fails with the provided message.
- [ ] `assert(false)` and `assert(false, "message")` are statically terminal and satisfy non-`Void` function control-flow requirements.
- [ ] Dynamic `assert(condition)` is not statically terminal; the pass path continues normally.
- [ ] The message argument is optional.
- [ ] For Alpha, the message is string data known before HIR lowering: a string literal is required; compile-time string constants may be accepted only if the existing constant-folding/facts path can normalize them without adding a new evaluator or stage leak.
- [ ] Arbitrary runtime message expressions are deferred until lazy failure-only evaluation is designed.
- [ ] Do not add `debug_assert`, `precondition`, `unreachable`, `todo`, `fatal`, `abort`, `recover`, `catch_panic`, custom panic payloads, or a public panic type in this work.
- [ ] Do not add a compatibility alias, wrapper, or deprecation path for old `panic` / `#panic`.

## Current repo anchors

Use these paths as the implementation map. Re-check them from the branch being edited before changing code.

| Area | Current anchor | Required action |
|---|---|---|
| Agent rules | `AGENTS.md` | Follow required reads, no user-input Rust panics, no compatibility wrappers, update matrix, and end each phase with audit/validation. |
| Style/diagnostics | `docs/codebase-style-guide.md` | Use `CompilerDiagnostic` for user-facing source failures and `CompilerError` only for internal/backend/tooling failures. |
| Compiler stages | `docs/compiler-design-overview.md` | Keep tokenizer, AST, HIR, borrow validation, and backend lowering responsibilities separate. |
| Roadmap | `docs/roadmap/roadmap.md` | Current TODO includes removing panic and adding `assert`; update when done and add deferred follow-ups. |
| Progress matrix | `docs/src/docs/progress/#page.bst` | Add assertion support and removed/deferred panic-related surfaces. |
| Errors docs | `docs/src/docs/errors/#page.bst` | Replace `#panic` docs with `assert` docs. |
| Generated docs | `docs/release/docs/errors/index.html`, `docs/release/docs/progress/index.html` | Do not hand-edit. Rebuild with `cargo run build docs --release`. |
| Keyword policy | `src/compiler_frontend/keywords.rs` | Add `assert` keyword and reserved shadow entry; remove public `panic` keyword mapping if present. |
| Token model | `src/compiler_frontend/tokenizer/tokens.rs` | Add `TokenKind::Assert`; ensure expression/statement dispatch handles it intentionally. |
| Statement dispatch | `src/compiler_frontend/ast/statements/body_dispatch.rs` | Route `TokenKind::Assert` to a dedicated statement parser. |
| Symbol statement parsing | `src/compiler_frontend/ast/statements/body_symbol.rs` | Confirm `assert` cannot fall through as an ordinary symbol; avoid special bare-`panic` handling unless current public syntax requires it. |
| Expression dispatch | `src/compiler_frontend/ast/expressions/parse_expression_dispatch.rs` | Reject `assert` in expression position with structured diagnostics. |
| Builtin parsing pattern | `src/compiler_frontend/builtins/expression_parsing.rs` | Reuse style/patterns for compiler-owned surfaces, but keep `assert` statement-owned. |
| AST node model | `src/compiler_frontend/ast/ast_nodes.rs` | Add `NodeKind::Assert` with typed condition and literal/compile-time message data. |
| HIR terminator | `src/compiler_frontend/hir/terminators.rs` | Replace `Panic` with explicit placeholder + assertion-failure terminator. |
| HIR placeholder | `src/compiler_frontend/hir/hir_builder.rs`, `src/compiler_frontend/hir/hir_statement/declarations.rs` | Replace `Panic(None)` placeholder with `HirTerminator::Uninitialized` or equivalent. |
| HIR validation | `src/compiler_frontend/hir/validation/blocks.rs` | Reject the placeholder after lowering; validate assertion-failure terminator. |
| HIR utils/display | `src/compiler_frontend/hir/utils.rs`, `src/compiler_frontend/hir/hir_display.rs` | Update successor traversal and display output. |
| Borrow checker | `src/compiler_frontend/analysis/borrow_checker/transfer/access/terminator.rs` | Update terminator match. If messages are stored as text data, no message-expression borrow reads are needed. |
| JS backend | `src/backends/js/js_statement.rs` | Replace `emit_panic_terminator` with assertion-failure lowering. |
| Wasm backend | `src/backends/wasm/hir_to_lir/terminator.rs` | Lower assertion failure to trap; message display remains deferred. |
| Integration tests | `tests/cases/manifest.toml` and `tests/cases/*` | Add success, runtime failure, diagnostics, control-flow, and generated-artifact cases. |

## Reuse and simplification decisions

Prefer these choices unless the branch state forces otherwise.

### Diagnostics

- [ ] First try to reuse existing diagnostics instead of adding a new diagnostic family.
- [ ] Use `InvalidBuiltinCall` for assert call-shape mistakes because `assert(...)` is compiler-owned call-shaped syntax.
- [ ] Extend `InvalidBuiltinCallReason` only with missing assert-specific reasons that cannot be represented well today, such as:
  - `MissingArgument`
  - `TooManyArguments`
  - `RuntimeMessageExpressionDeferred`
  - `ExpressionPositionNotAllowed`
- [ ] Use `TypeMismatch` with `TypeMismatchContext::Condition` for non-`Bool` conditions.
- [ ] Use `TypeMismatch` or `InvalidBuiltinCall` for non-string messages, depending on whether the parser has a semantic `TypeId` at that point.
- [ ] Use `InvalidResultHandlingReason::NotResultExpression` for `assert(...)!` / `assert(...) catch ...` if that produces a clear result-handling diagnostic; otherwise add one focused `InvalidBuiltinCallReason`.
- [ ] Use `DeferredFeatureReason::NamedFeature` for arbitrary runtime assertion messages if a dedicated reason is unnecessary.
- [ ] Add a new `InvalidAssertReason` only if reusing existing diagnostics produces unclear messages, unstable codes, or awkward payloads.

### Message representation

- [ ] Do **not** store assertion messages as `HirExpression` while runtime message expressions are deferred.
- [ ] Prefer `Option<String>` in HIR, or a small `AssertMessage` text payload, because only literal/compile-time messages are supported.
- [ ] This removes the need for borrow-checker message-expression traversal and avoids a fake runtime evaluation path.
- [ ] If compile-time string constants are supported, normalize them to text before HIR. Do not preserve them as runtime references.
- [ ] If normalization would require new constant-evaluation plumbing, defer compile-time-constant messages and document the deferral in the matrix/roadmap.

### Bare `panic` identifier policy

- [ ] Do not add a `panic` keyword.
- [ ] If the current branch already tokenizes `panic`, remove that tokenization.
- [ ] Do not add a public `panic(...)` function, prelude symbol, or compatibility shim.
- [ ] Prefer not to ban arbitrary user declarations named `panic` unless there is already an identifier policy for removed language-reserved names.
- [ ] Add targeted diagnostics for old `#panic` only if the current parser otherwise produces a vague or misleading error.

### HIR terminology

- [ ] Replace public/source-shaped `Panic` with `AssertFailure`.
- [ ] Do not introduce a broad `RuntimePanicKind` abstraction unless another existing non-assert runtime panic source needs it today.
- [ ] Add `HirTerminator::Uninitialized` (or `Placeholder`) only as a builder-internal invalid-after-lowering placeholder.
- [ ] Never use `AssertFailure(None)` as a placeholder; no-message assertion failure is valid.

---

# Phase 0 — Baseline audit and exact surface inventory

## Context

This phase prevents implementation drift. The current repo has documented `#panic`, HIR/backend `Panic` infrastructure, and `Panic(None)` placeholder logic. The coding agent must inventory the checked-out branch because the implementation may start from a later commit.

## Checklist

### Required reading

- [ ] Read `AGENTS.md`.
- [ ] Read `docs/codebase-style-guide.md`.
- [ ] Read `docs/compiler-design-overview.md`.
- [ ] Read `docs/language-overview.md`, especially statements, expressions, constants, diagnostics, and error handling.
- [ ] Read `docs/src/docs/errors/#page.bst`.
- [ ] Read `docs/src/docs/progress/#page.bst`.
- [ ] Read `docs/roadmap/roadmap.md`.

### Repo search

Run:

```bash
rg -n '\bpanic\b|#panic|\bPanic\b|\bassert\b' \
  AGENTS.md README.md Cargo.toml docs src tests benchmarks xtask
```

Classify every hit:

- [ ] Public Beanstalk syntax/documentation that must change.
- [ ] Rust `assert!` / test assertion infrastructure that should stay.
- [ ] Compiler policy text about Rust `panic!` that should stay.
- [ ] HIR/backend runtime-stop implementation to rename, replace, or keep internal.
- [ ] Generated docs under `docs/release` that must be regenerated, not hand-edited.
- [ ] Old fixtures/goldens that must be updated or removed.

### Current implementation inventory

- [ ] Confirm whether `TokenKind::Panic`, `TokenKind::Assert`, or equivalent exists.
- [ ] Confirm whether `keyword_token_kind("panic")` exists.
- [ ] Confirm whether `keyword_token_kind("assert")` exists.
- [ ] Confirm whether `panic` is reserved by identifier policy.
- [ ] Confirm whether old `#panic` parser logic exists or whether it is only documented.
- [ ] Confirm every current `HirTerminator::Panic` call site.
- [ ] Confirm where `Panic(None)` is used as a placeholder.
- [ ] Confirm JS and Wasm backend handling of `HirTerminator::Panic`.
- [ ] Confirm borrow-checker handling of `HirTerminator::Panic`.
- [ ] Confirm HIR display/golden output behavior for panic terminators.
- [ ] Confirm integration case registration style in `tests/cases/manifest.toml`.

### Baseline validation

Run:

```bash
cargo fmt --check
cargo check
cargo test
cargo run tests
just validate
```

If frontend-boundary-sensitive files will change, also run:

```bash
just audit-frontend-boundaries
```

## Phase 0 audit / style-guide review / validation gate

- [ ] Inventory is complete and attached to implementation notes.
- [ ] Public `panic` / `#panic` occurrences to remove are listed.
- [ ] Internal runtime-stop occurrences to rename/keep are listed.
- [ ] No generated `docs/release` file was edited directly.
- [ ] Baseline validation status is recorded.
- [ ] This plan is updated if branch state differs materially from these assumptions.

---

# Phase 1 — Reserve `assert` and wire minimal diagnostics

## Context

`assert` must be language-owned, not an importable or shadowable function. This phase establishes the token/reserved-name/diagnostic surface without runtime lowering.

## Checklist

### Keyword and reserved-name policy

- [ ] Add `Assert` to `TokenKind` in `src/compiler_frontend/tokenizer/tokens.rs`.
- [ ] Add exact keyword mapping in `src/compiler_frontend/keywords.rs`:

```rust
"assert" => Some(TokenKind::Assert)
```

- [ ] Add `"assert"` to `RESERVED_KEYWORD_SHADOWS`.
- [ ] Update the reserved keyword array length.
- [ ] Confirm existing reserved-name diagnostics reject `assert` as a variable, function, type alias, struct, choice, import alias, grouped import alias, receiver method alias, and local binding.
- [ ] Do not add `TokenKind::Panic`.
- [ ] Remove `panic` keyword mapping if it exists.
- [ ] Do not ban bare user-defined `panic` unless current policy already reserves removed language names.

### Diagnostic choices

- [ ] Inspect existing `InvalidBuiltinCallReason`, `TypeMismatchContext`, `InvalidResultHandlingReason`, and `DeferredFeatureReason`.
- [ ] Reuse them where possible.
- [ ] Add new `InvalidBuiltinCallReason` variants only for assert-specific gaps.
- [ ] Add a new `InvalidAssertReason` only if reuse is genuinely awkward.
- [ ] Ensure any new diagnostic kind/reason is wired through:
  - `diagnostic_payload/types.rs`
  - `diagnostic_payload/mod.rs` if a new payload is added
  - `compiler_messages/mod.rs` re-exports
  - `diagnostic_kind.rs` if a new kind is added
  - `diagnostic_kind_descriptors.rs` if a new kind is added
  - render code
  - remap code if the payload carries `StringId` / `InternedPath`
  - diagnostic model tests

### Diagnostic wording requirements

- [ ] Explain that `assert` is for invariants and does not return an error value.
- [ ] Suggest `Error!` / `catch` for expected failures where relevant.
- [ ] Suggest `assert(false, "message")` for old explicit panic use.
- [ ] Avoid wording that implies a public `panic` API still exists.

### Tests

- [ ] Add/update tokenizer/keyword tests for `assert`.
- [ ] Add tests that `assert` cannot be used as an identifier or alias through existing reserved-name paths.
- [ ] Add a diagnostic test for old `#panic` only if implementation adds a targeted diagnostic.
- [ ] Add a test confirming no `panic` keyword/builtin is introduced.
- [ ] Prefer stable diagnostic-code assertions over rendered prose.

## Phase 1 audit / style-guide review / validation gate

- [ ] `assert` is language-owned but not modelled as a user-visible function.
- [ ] No compatibility path for `panic` exists.
- [ ] User-authored invalid `assert` / legacy `#panic` failures use `CompilerDiagnostic`, not `CompilerError`.
- [ ] No broad new diagnostic family was added if existing diagnostics were enough.
- [ ] Comments explain non-obvious keyword/reserved-name behavior.
- [ ] Run `cargo fmt --check`.
- [ ] Run targeted tokenizer/keyword/diagnostic tests.
- [ ] Run `cargo test`.
- [ ] Run `cargo run tests`.
- [ ] Run `just validate`.
- [ ] Run `just audit-frontend-boundaries` if frontend-boundary-sensitive files changed.

---

# Phase 2 — Parse `assert` as a statement intrinsic and add AST support

## Context

A normal function-call parser would allow the wrong shape: expression use, named arguments, mutable markers, fallible suffixes, and runtime message expressions. `assert` needs a dedicated statement parser.

## Checklist

### AST shape

Add explicit AST support in `src/compiler_frontend/ast/ast_nodes.rs`.

Recommended shape:

```rust
pub struct AssertMessage {
    pub text: StringId,
    pub location: SourceLocation,
}

NodeKind::Assert {
    condition: Expression,
    message: Option<AssertMessage>,
}
```

If compile-time string constants are normalized to owned text during AST instead of `StringId`, use:

```rust
pub struct AssertMessage {
    pub text: String,
    pub location: SourceLocation,
}
```

Rules:

- [ ] Do not use `Expression` for the message unless runtime message expressions are intentionally implemented.
- [ ] Ensure AST debug/display/remap helpers handle the new message payload if needed.
- [ ] Ensure `AstNode::expression_type_id` and expression-only helpers reject `NodeKind::Assert` as non-expression.
- [ ] Add a WHAT/WHY comment explaining that `assert` is a runtime statement intrinsic, not a value expression.

### Parser module

- [ ] Add `src/compiler_frontend/ast/statements/asserts.rs`.
- [ ] Register it in the statements module.
- [ ] Keep parser logic statement-owned.

Parser behavior:

- [ ] Current token must be `TokenKind::Assert`.
- [ ] Preserve the source location of the `assert` keyword as the AST node location.
- [ ] Require `(` immediately after `assert`.
- [ ] Reject `assert()`.
- [ ] Parse exactly one condition expression first.
- [ ] Require the condition expression to type-check as `Bool` using semantic `TypeId`, not `DataType`.
- [ ] Allow an optional comma and message.
- [ ] Accept string literal messages.
- [ ] Accept compile-time string constants only if existing constant facts/folding can normalize them locally without new evaluator plumbing.
- [ ] Reject arbitrary runtime message expressions with a deferred-feature diagnostic.
- [ ] Reject more than two arguments.
- [ ] Reject named arguments.
- [ ] Reject mutable argument markers.
- [ ] Require the closing `)`.
- [ ] Reject `assert(...)!`.
- [ ] Reject `assert(...) catch:` and `assert(...) catch |err|:`.
- [ ] Do not evaluate or lower message expressions on the success path.

### Condition parsing

- [ ] Parse through existing expression machinery with `ExpectedType::Known(Bool)` if available.
- [ ] Let normal expression diagnostics handle invalid nested expressions.
- [ ] Confirm fallible calls in the condition must still be handled before the condition becomes `Bool`.
- [ ] Add a test for `assert(parse_bool(text)!)` inside a function with a compatible error slot if this pattern is already allowed by expression parsing.

### Message parsing

Preferred minimal Alpha behavior:

```bst
assert(condition, "message")
```

- [ ] If current token is `StringSliceLiteral`, capture its text and location directly.
- [ ] If current token is `Symbol`, attempt compile-time-string lookup only if an existing const path supports it clearly.
- [ ] If compile-time-string lookup is not straightforward, reject with a structured deferred-feature diagnostic and add matrix/roadmap follow-up.
- [ ] Reject templates, function calls, arithmetic/string concatenation, collection/struct values, and fallible expressions as assertion messages for now.

### Statement dispatch

- [ ] Update `src/compiler_frontend/ast/statements/body_dispatch.rs` to route `TokenKind::Assert` to the new parser.
- [ ] Allow it in ordinary function bodies and entry `start()` body.
- [ ] Reject top-level non-entry assertions through existing invalid-top-level runtime statement diagnostics.
- [ ] Allow `assert` inside catch handlers as an ordinary statement.
- [ ] Only `assert(false, ...)` counts as terminal for catch-handler fallthrough analysis; dynamic `assert(condition)` does not.

### Expression-position rejection

- [ ] Update `src/compiler_frontend/ast/expressions/parse_expression_dispatch.rs` to reject `TokenKind::Assert` in expression position.
- [ ] Prefer a targeted `InvalidBuiltinCall`/`InvalidStatementPosition` diagnostic rather than allowing a vague unexpected-token failure.

Negative examples:

```bst
value = assert(x)
return assert(x)
foo(assert(x))
[: [assert(x)] :]
```

### Legacy `#panic`

- [ ] Remove docs and fixtures that present `#panic` as valid.
- [ ] If the current parser sees `#panic` and reports a vague error, add a targeted diagnostic: old `#panic` has been removed; use `assert(false, "message")`.
- [ ] If the current old-prefix diagnostic is already clear and stable, do not add a special parser path.
- [ ] Do not add runtime behavior for `#panic`.
- [ ] Do not preserve `#panic` as a hidden alias.

### Tests

Add parser/integration diagnostics for:

- [ ] `assert(true)` accepted.
- [ ] `assert(true, "message")` accepted.
- [ ] `assert(false)` accepted.
- [ ] `assert(false, "message")` accepted.
- [ ] `assert(1)` rejected as non-`Bool` condition.
- [ ] `assert()` rejected.
- [ ] `assert(true, "a", "b")` rejected.
- [ ] `assert(true, 1)` rejected.
- [ ] `assert(true, name = "message")` rejected.
- [ ] `assert(true, ~message)` rejected.
- [ ] `assert(true)!` rejected.
- [ ] `assert(true) catch: ... ;` rejected.
- [ ] `value = assert(true)` rejected.
- [ ] `return assert(true)` rejected.
- [ ] `#panic "message"` rejected or covered by existing old-prefix diagnostic.

Suggested integration case IDs:

```text
assert_statement_success
assert_statement_message_success
assert_false_explicit_stop
assert_condition_must_be_bool_rejected
assert_message_must_be_string_literal_rejected
assert_expression_position_rejected
assert_catch_suffix_rejected
legacy_panic_removed_rejected
```

## Phase 2 audit / style-guide review / validation gate

- [ ] `assert` cannot leak into expression/value syntax.
- [ ] No ordinary user-function call parser path accepts `assert`.
- [ ] Message restrictions are enforced before HIR.
- [ ] Diagnostics are structured and specific.
- [ ] Parser code is stage-local and does not mix HIR/backend concerns.
- [ ] No generated docs were edited directly.
- [ ] Run `cargo fmt --check`.
- [ ] Run targeted parser/AST tests.
- [ ] Run `cargo test`.
- [ ] Run `cargo run tests`.
- [ ] Run `just validate`.
- [ ] Run `just audit-frontend-boundaries`.

---

# Phase 3 — Replace HIR panic placeholder infrastructure and lower `assert`

## Context

Current HIR uses `HirTerminator::Panic { message: None }` as an uninitialized block placeholder. That conflicts with the new semantics because a no-message assertion failure is valid. This phase separates internal block construction from real assertion failure and lowers assert CFG.

## Checklist

### Replace the placeholder terminator

- [ ] Add an explicit placeholder terminator variant.

Recommended:

```rust
HirTerminator::Uninitialized
```

- [ ] Update block creation in `src/compiler_frontend/hir/hir_statement/declarations.rs` to initialize entry blocks with `Uninitialized`.
- [ ] Update any other direct `HirBlock { terminator: ... }` construction.
- [ ] Update `HirBuilder::is_placeholder_terminator` in `src/compiler_frontend/hir/hir_builder.rs`.
- [ ] Update `set_block_terminator` to accept only `Uninitialized` as placeholder.
- [ ] Update HIR validation to reject `Uninitialized` in validated HIR.
- [ ] Remove all `Panic(None)` placeholder comments and error text.
- [ ] Add defensive backend/display/borrow-checker match arms for `Uninitialized` as required by Rust exhaustiveness. Backends should return `CompilerError` if it somehow reaches lowering.

### Replace generic `Panic` terminator

Preferred Alpha shape:

```rust
HirTerminator::AssertFailure {
    message: Option<String>,
}
```

Rules:

- [ ] Remove `HirTerminator::Panic { message }` unless another current internal runtime panic producer still needs it.
- [ ] Do not introduce a general `RuntimePanicKind` unless there is a real second runtime-panic source today.
- [ ] `AssertFailure { message: None }` is valid and means default assertion-failure message.
- [ ] `AssertFailure` has no CFG successors.
- [ ] `AssertFailure` does not require return-type compatibility.
- [ ] `AssertFailure` is terminal and can satisfy function/catch control-flow termination only when it is actually emitted as the current block terminator.

### Terminator utilities/display

Update:

- [ ] `src/compiler_frontend/hir/terminators.rs`
- [ ] `src/compiler_frontend/hir/utils.rs`
- [ ] `src/compiler_frontend/hir/hir_display.rs`
- [ ] Any HIR debug/golden output.

Rules:

- [ ] `Uninitialized` has no successors but is invalid after lowering.
- [ ] `AssertFailure` has no successors.
- [ ] HIR display renders something like `assert_failure` / `assert_failure "message"`, not `panic`.

### Lower `NodeKind::Assert`

Add a helper such as:

```rust
lower_assert_statement(condition, message, location)
```

Recommended lowering:

#### Case A: condition is statically `false`

- [ ] If the condition expression is `ExpressionKind::Bool(false)` after AST parsing/folding, lower directly to `AssertFailure { message }` in the current block.
- [ ] Do not create a pass block.
- [ ] This makes `assert(false, ...)` a true terminal statement.
- [ ] Add tests showing a non-`Void` function may end with `assert(false, "unreachable")` without a fallthrough diagnostic.

#### Case B: condition is statically `true`

- [ ] If the condition expression is `ExpressionKind::Bool(true)`, either lower it as a no-op or through the normal branch path.
- [ ] Prefer no-op only if it does not erase useful source mapping expected by tests.
- [ ] Do not add a complex optimizer here.

#### Case C: dynamic condition

1. [ ] Lower the condition expression in the current block.
2. [ ] Create a pass/continue block in the current function region.
3. [ ] Create a failure block in the current function region.
4. [ ] Emit an `If` terminator from the condition block:
   - `then_block` = pass block
   - `else_block` = failure block
5. [ ] Enter the failure block.
6. [ ] Emit `AssertFailure { message }` as the failure-block terminator.
7. [ ] Set the current block to the pass block.
8. [ ] Later statements continue only on the pass path.

Additional requirements:

- [ ] Map source locations for the condition branch terminator and the failure terminator.
- [ ] Preserve the assert statement location for backend comments.
- [ ] Do not lower message data as a runtime expression.
- [ ] Do not lower `assert(false, ...)` as dynamic branch if the literal false case is available.

### Borrow checker / analysis

- [ ] Update terminator transfer/access code for `AssertFailure` and `Uninitialized`.
- [ ] Because assertion messages are text data, the borrow checker only needs to visit the condition via the `If` terminator path.
- [ ] Confirm failed assertion paths are terminal for analysis.
- [ ] Confirm dynamic assertions produce normal branch facts for the condition read.
- [ ] Add tests involving borrowed/mutable locals in assert conditions.

### HIR validation

- [ ] Validate `AssertFailure` as a valid terminal terminator.
- [ ] Validate `Uninitialized` is never present after lowering.
- [ ] Remove validation text that treats `Panic(None)` as placeholder.
- [ ] Add/keep tests that no validated HIR block contains the placeholder.

### Tests

Add/update HIR tests for:

- [ ] `assert(true)` lowers successfully.
- [ ] `assert(condition)` creates an explicit pass/failure CFG.
- [ ] `assert(false)` lowers to a terminal `AssertFailure` with no pass block.
- [ ] `assert(false, "message")` lowers to terminal `AssertFailure { message: Some(...) }`.
- [ ] A non-`Void` function ending in `assert(false, "unreachable")` passes return-shape validation.
- [ ] Dynamic `assert(condition)` does not satisfy non-`Void` fallthrough requirements by itself.
- [ ] Assert failure path has no successors.
- [ ] Placeholder terminator is not `Panic(None)` anymore.
- [ ] Validation accepts `AssertFailure { message: None }`.
- [ ] Validation rejects `Uninitialized`.

## Phase 3 audit / style-guide review / validation gate

- [ ] Old `Panic(None)` placeholder infrastructure is fully removed.
- [ ] HIR no longer has a generic source-authored panic path.
- [ ] `assert(false, ...)` is statically terminal.
- [ ] Dynamic `assert(condition)` preserves pass/failure CFG behavior.
- [ ] Assertion messages are not runtime expressions.
- [ ] Stage boundaries are clean: AST parses, HIR lowers, backend emits.
- [ ] Borrow checker sees condition reads through normal HIR `If` handling.
- [ ] Run `cargo fmt --check`.
- [ ] Run targeted HIR and borrow-checker tests.
- [ ] Run `cargo test`.
- [ ] Run `cargo run tests`.
- [ ] Run `just validate`.
- [ ] Run `just audit-frontend-boundaries`.

---

# Phase 4 — Backend/runtime lowering

## Context

Backends should lower assertion failure as an unrecoverable runtime stop. JS may use `throw`, but Beanstalk source must not gain catchable panics. Wasm traps for now; trap message support is deferred.

## Checklist

### JavaScript backend

Update `src/backends/js/js_statement.rs`.

- [ ] Replace `emit_panic_terminator` with `emit_assert_failure_terminator`.
- [ ] Update dispatcher terminator matching for `AssertFailure`.
- [ ] For `Some(message)`, emit a JS runtime error with that message.
- [ ] For `None`, emit `throw new Error("assertion failed");` or the chosen stable default.
- [ ] Escape message strings through existing JS string-literal escaping/lowering helpers if available.
- [ ] Do not introduce a Beanstalk-level catch/recover mechanism.
- [ ] If adding a helper such as `__bs_assert_failed`, place it in the backend runtime helper owner, not inline in unrelated code.
- [ ] Ensure location comments still point to the source `assert`.

Recommended simple JS behavior:

```js
throw new Error("assertion failed");
throw new Error("message");
```

Do not add a Beanstalk `Error!` value here.

### Wasm backend

Update `src/backends/wasm/hir_to_lir/terminator.rs`.

- [ ] Lower `AssertFailure` to `WasmLirTerminator::Trap`.
- [ ] Ignore the message in Wasm for Alpha unless an existing trap-message channel already exists.
- [ ] Add/update comments that Wasm assertion messages/trap payloads are deferred.
- [ ] Keep Wasm support experimental in the matrix.
- [ ] Make `Uninitialized` produce a backend `CompilerError` if it somehow reaches lowering.

### Other backend/display/test areas

- [ ] Update tests that pattern-match `Panic`.
- [ ] Update HIR display/golden output from `panic` to `assert_failure` or the chosen name.
- [ ] Update source-language test names mentioning panic when they should now mention assert.
- [ ] Keep Rust test assertions named normally; do not rename unrelated `assert!` code.

### Runtime tests

Add/update integration tests for:

- [ ] `assert(true)` runs normally.
- [ ] `assert(true, "message")` runs normally.
- [ ] `assert(false)` fails at runtime with the default assertion-failure message.
- [ ] `assert(false, "message")` fails at runtime with the provided message.
- [ ] A false assert inside a function stops before later runtime output.
- [ ] A passing assert does not prevent later runtime output.
- [ ] `assert` does not produce an `Error!` value.
- [ ] Beanstalk `catch` cannot catch assertion failure.
- [ ] Generated JS contains an explicit failure path and does not call an `assert` user function.
- [ ] No old public `panic` helper is emitted for explicit assertions.
- [ ] Wasm lowering maps `AssertFailure` to trap if Wasm tests cover this path.

## Phase 4 audit / style-guide review / validation gate

- [ ] JS lowering treats assertion failure as runtime stop, not a fallible value.
- [ ] Wasm lowering traps cleanly and does not pretend to support messages.
- [ ] Generated JS does not expose a public panic API.
- [ ] Backend comments explain that JS `throw` is backend machinery only.
- [ ] Runtime tests cover message and no-message cases.
- [ ] Run `cargo fmt --check`.
- [ ] Run targeted JS backend tests.
- [ ] Run targeted Wasm lowering tests if available.
- [ ] Run `cargo test`.
- [ ] Run `cargo run tests`.
- [ ] Run `just validate`.

---

# Phase 5 — Documentation, roadmap, matrix, generated docs

## Context

This changes the public language surface. Documentation changes are required. The current errors page documents `#panic`; the matrix must explicitly list assertion support plus removed/deferred panic-related features.

## Checklist

### `docs/src/docs/errors/#page.bst`

- [ ] Replace the `#panic` sections.
- [ ] Change page title if desired from “Errors and Panics in Beanstalk” to “Errors, Options, and Assertions in Beanstalk”.
- [ ] Remove all `#panic` examples.
- [ ] Remove all statements implying public `panic` syntax.
- [ ] Add an `assert` section with examples:

```bst
assert(index < items.length)
assert(index < items.length, "index must be in bounds")
assert(false, "unimplemented backend path")
```

- [ ] State that `assert` is always checked.
- [ ] State that failed `assert` is not catchable.
- [ ] State that `assert` does not return `Error!`.
- [ ] State that `assert(false, "message")` is the only explicit source-level panic for now.
- [ ] State that expected failures should use `Error!` / `catch`.
- [ ] State that optional absence should use `?` / `none`.
- [ ] State that runtime assertion-message expressions are deferred if they remain rejected.
- [ ] Add a table comparing `Error!`, `T?`, and `assert`.
- [ ] Update the diagnostics section to include assert-specific diagnostics.
- [ ] Update the summary to say panics are caused explicitly only through failed `assert`.

### `docs/src/docs/progress/#page.bst`

Add/update rows.

#### Implemented row

- [ ] Surface: `Assertions`
- [ ] Status: `Supported` if JS/HTML Alpha support is complete; `Partial` only if a required Alpha sub-surface remains incomplete.
- [ ] Coverage: `Targeted` or `Broad`, matching tests.
- [ ] Runtime target: `Frontend / HIR / JS / HTML`; mention Wasm trap if covered.
- [ ] Watch points:
  - `assert(condition)` and `assert(condition, "message")` are statement-only.
  - Assertions are always checked.
  - Failed assertions are unrecoverable.
  - `assert(false, "message")` is the only explicit source-level panic.
  - Messages are optional.
  - Runtime message expressions are deferred unless implemented.
  - `catch` cannot catch assertion failure.

#### Removed/deferred rows

- [ ] `Source panic keyword/directive`
  - Status: `Removed / Rejected`
  - Notes: old `panic` / `#panic` source forms are not supported; use `assert(false, "message")`.
- [ ] `Debug-only assertions`
  - Status: `Deferred`
  - Notes: no `debug_assert`; `assert` is always checked.
- [ ] `Catchable/recoverable panics`
  - Status: `Deferred / Rejected`
  - Notes: no `recover`, no `catch_panic`, no Beanstalk catch support for assertion failure.
- [ ] `Additional explicit stop helpers`
  - Status: `Deferred`
  - Notes: no `todo`, `unreachable`, `fatal`, `abort`, or `precondition` builtins yet.
- [ ] `Runtime assertion-message expressions`
  - Status: `Deferred` if rejected.
  - Notes: arbitrary runtime message expressions require lazy failure-only evaluation design.
- [ ] `Compile-time constant assertion messages`
  - Status: `Supported` if implemented by reusing existing const facts; otherwise `Deferred`.
  - Notes: no new constant evaluator should be added for this feature alone.
- [ ] `Wasm assertion messages`
  - Status: `Deferred / Experimental`
  - Notes: Wasm lowers assertion failure to trap; message materialization is not Alpha.

### `docs/roadmap/roadmap.md`

When implementation is complete:

- [ ] Remove the active TODO item or mark it complete according to roadmap style.
- [ ] Fix typo `explcitly` if text remains.
- [ ] Add follow-up notes for deliberately deferred features:
  - debug-only assert
  - runtime/lazy assertion messages
  - compile-time constant messages, if not implemented
  - catchable/recoverable panics
  - `todo` / `unreachable` / `fatal` / `abort` / `precondition` helpers
  - Wasm trap message support
  - richer runtime panic metadata/stack traces, if not implemented
- [ ] Do not leave a roadmap item that sounds like public `panic` remains planned.

### `docs/language-overview.md`

If this document describes errors, control flow, directives, statements, or panic behavior:

- [ ] Replace any `panic` / `#panic` source-syntax references.
- [ ] Add `assert` statement syntax and semantics.
- [ ] Document that `assert` is not a function and cannot be used as an expression.
- [ ] Document that `assert(false, ...)` is statically terminal.
- [ ] Document that failed assert is unrecoverable and not caught by `catch`.

### Generated docs

- [ ] Do not hand-edit `docs/release/docs/errors/index.html`.
- [ ] Do not hand-edit `docs/release/docs/progress/index.html`.
- [ ] Rebuild generated docs:

```bash
cargo run build docs --release
```

- [ ] Inspect generated errors/progress pages for stale `#panic` content.
- [ ] Include regenerated docs artifacts if the repo tracks them.

### Search cleanup

After docs edits, run:

```bash
rg -n '#panic|panic keyword|panic syntax|Panic syntax|codesnippet:#panic|#panic' docs/src docs/roadmap src tests
```

- [ ] Remove or rewrite public-language stale references.
- [ ] Keep policy references to Rust `panic!` only where they are about compiler implementation discipline.
- [ ] Keep internal runtime “panic” terminology only if intentionally internal and not source-facing.

## Phase 5 audit / style-guide review / validation gate

- [ ] `docs/src/docs/errors/#page.bst` no longer documents `#panic`.
- [ ] `docs/src/docs/progress/#page.bst` contains assert status and deferred feature rows.
- [ ] `docs/roadmap/roadmap.md` no longer lists completed assert/panic work as active.
- [ ] Deliberately deferred features are explicitly documented.
- [ ] Generated docs were rebuilt, not manually edited.
- [ ] Run `cargo fmt --check`.
- [ ] Run `cargo run build docs --release`.
- [ ] Run `cargo test`.
- [ ] Run `cargo run tests`.
- [ ] Run `just validate`.

---

# Phase 6 — Final integration, cleanup, and handoff audit

## Context

This phase verifies the implementation is clean: no stale public panic API, no placeholder ambiguity, no duplicate public paths, no docs drift, no weak diagnostics.

## Checklist

### Final source search

Run:

```bash
rg -n '\bpanic\b|#panic|\bPanic\b' docs/src docs/roadmap src tests
```

For every remaining hit:

- [ ] Confirm it is internal compiler/runtime terminology or Rust implementation policy, not public source syntax.
- [ ] Confirm it is not stale documentation.
- [ ] Confirm it is not an old fixture/golden that should be updated.
- [ ] Confirm it is not `Panic(None)` placeholder logic.
- [ ] Confirm it is not a source-level parser path.

Run:

```bash
rg -n 'Panic\(None\)|HirTerminator::Panic|TokenKind::Panic|NodeKind::Panic|#panic' src docs tests
```

Expected result:

- [ ] No old public/placeholder hits.
- [ ] If an internal `Panic` type intentionally remains, document why and ensure it is not source-facing or placeholder-related.

Run:

```bash
rg -n '\bassert\b' src docs tests
```

Confirm:

- [ ] `assert` is in keyword/token policy.
- [ ] `assert` parser is statement-only.
- [ ] `assert` has tests.
- [ ] `assert` docs and matrix are present.
- [ ] No user-defined `assert` compatibility function exists.

### Final behavior checks

- [ ] `assert(true)` compiles and runs.
- [ ] `assert(true, "message")` compiles and runs.
- [ ] `assert(false)` compiles and fails at runtime with default assertion-failure behavior.
- [ ] `assert(false, "message")` compiles and fails at runtime with provided message.
- [ ] A function returning a value may end with `assert(false, "unreachable")` without fallthrough diagnostics.
- [ ] A function returning a value may not end with dynamic `assert(condition)` unless remaining paths return.
- [ ] `assert` with non-`Bool` condition is rejected before HIR.
- [ ] `assert` with runtime-computed message is rejected if deferred.
- [ ] `assert` in expression position is rejected before HIR.
- [ ] `assert` with `catch` / `!` is rejected before HIR.
- [ ] Old `#panic` is rejected or covered by a clear legacy diagnostic.
- [ ] No source-level `panic` keyword/function/directive is exposed by the language.
- [ ] Beanstalk `catch` handles `Error!` only and cannot catch assertion failure.

### Final validation commands

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run build docs --release
just audit-frontend-boundaries
just validate
```

If any command fails:

- [ ] Fix the root cause.
- [ ] Do not suppress warnings without justification.
- [ ] Re-run the failed command.
- [ ] Re-run `just validate` after fixes.

### Required final audit

- [ ] **Style-guide compliance:** Code is readable, uses descriptive names, has useful WHAT/WHY comments, avoids unnecessary cleverness, and keeps tests out of production files.
- [ ] **Architecture/stage-boundary compliance:** Tokenizer, AST, HIR, borrow checker, and backend responsibilities remain separate.
- [ ] **Language-semantics compliance:** `assert` is always-on, statement-only, not catchable, not a value error, and is the only explicit panic source spelling through `assert(false, ...)`.
- [ ] **Control-flow compliance:** `assert(false, ...)` is terminal; dynamic `assert(condition)` is not statically terminal.
- [ ] **Memory-model compliance:** Borrow checker sees assertion condition reads through HIR branch logic; assertion failure is terminal; no ownership/drop facts are bypassed.
- [ ] **Diagnostics quality:** Invalid uses produce structured `CompilerDiagnostic` values with stable codes and useful source labels.
- [ ] **Test coverage:** Success, runtime failure, parser rejection, HIR lowering, backend lowering, and docs/matrix behavior are covered without redundant fixtures.
- [ ] **Duplicated or obsolete logic:** Old `panic` / `#panic` public paths, `Panic(None)` placeholder logic, stale helpers, and legacy comments are removed.
- [ ] **Progress matrix accuracy:** Matrix explicitly states implemented assert surface and all deferred assert/panic-related features.
- [ ] **Roadmap accuracy:** Roadmap no longer lists the completed item as active work and records follow-up deferred features.
- [ ] **Validation status:** Required commands are recorded as passing.
- [ ] **Generated docs status:** `docs/release` files are regenerated from source docs, not manually edited.
- [ ] **No compatibility shims:** There is no old panic alias, wrapper, hidden parser fallback, or deprecated public path.

---

# Suggested implementation chunking for coding agents

Each chunk should fit in a single large coding-agent context.

## Chunk A — Surface reservation and diagnostics

Covers:

- Phase 0 inventory
- Phase 1 keyword/reserved-name updates
- Diagnostic reuse/extensions
- Keyword/reserved-name tests

Expected handoff:

- `assert` token/keyword exists.
- Diagnostics strategy is implemented or documented.
- No runtime behavior yet.

## Chunk B — AST parser

Covers:

- Phase 2 parser module
- `NodeKind::Assert`
- Statement dispatch
- Expression-position rejection
- Parser/AST tests

Expected handoff:

- `assert` parses as a statement.
- Invalid syntax rejected before HIR.
- Message data is literal/compile-time text only.

## Chunk C — HIR refactor and assert lowering

Covers:

- Phase 3 placeholder terminator replacement
- `AssertFailure` HIR terminator
- `NodeKind::Assert` lowering
- Static-false terminal semantics
- HIR validation
- Borrow-checker terminator updates
- HIR/borrow tests

Expected handoff:

- No `Panic(None)` placeholder.
- `assert(false, ...)` is terminal.
- Dynamic `assert(condition)` lowers to explicit pass/failure CFG.

## Chunk D — Backends and runtime tests

Covers:

- Phase 4 JS lowering
- Phase 4 Wasm trap lowering
- Runtime/integration tests
- Artifact/golden updates

Expected handoff:

- JS/HTML Alpha path runs.
- Wasm traps cleanly where applicable.
- Assertion failure is not catchable.

## Chunk E — Docs, matrix, roadmap, final validation

Covers:

- Phase 5 docs
- Matrix implemented/deferred rows
- Roadmap update
- Generated docs rebuild
- Phase 6 final audit and validation

Expected handoff:

- Public docs match implementation.
- Roadmap/matrix no longer stale.
- Full validation passes.

---

# Acceptance criteria

The task is complete only when all are true.

- [ ] `assert(condition)` is implemented.
- [ ] `assert(condition, "message")` is implemented for string literal messages.
- [ ] Compile-time string constant messages are either implemented by reusing existing const facts or explicitly marked deferred in roadmap/matrix.
- [ ] The message argument is optional.
- [ ] Assertions are always checked.
- [ ] Failed assertions are not catchable in Beanstalk source.
- [ ] `assert(false)` and `assert(false, "message")` are statically terminal.
- [ ] Dynamic `assert(condition)` is not statically terminal.
- [ ] `assert(false, "message")` is the only explicit source-level panic spelling.
- [ ] `assert` cannot be used as a value/function/expression/import/alias.
- [ ] Old `#panic` docs are removed.
- [ ] Old `#panic` source syntax is rejected or receives an existing clear legacy diagnostic.
- [ ] No public `panic` keyword/directive/function is provided.
- [ ] Old `Panic(None)` placeholder infrastructure is removed.
- [ ] HIR uses an explicit placeholder terminator and an explicit assertion-failure terminator.
- [ ] JS backend lowers assertion failure to an unrecoverable runtime error.
- [ ] Wasm backend lowers assertion failure to a trap, with message support documented as deferred if applicable.
- [ ] Errors docs explain `Error!` vs `?` vs `assert`.
- [ ] Progress matrix lists implemented assertion support and deferred assert/panic-related features.
- [ ] Roadmap is updated to remove/complete the active item and list follow-ups.
- [ ] `cargo run build docs --release` has regenerated docs.
- [ ] `just validate` passes.
- [ ] `just audit-frontend-boundaries` passes or any review-only warnings are documented and justified.
