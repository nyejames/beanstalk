# Beanstalk Error Syntax Migration Plan: `err!` / `! fallback` → `catch`

## Scope

This plan migrates Beanstalk's existing result/error call-site syntax to the new `catch` design.

Current old syntax:

```beanstalk
value = parse_number(text)!                    -- bubble
value = parse_number(text) ! 0                 -- fallback
value = parse_number(text) err! 0:             -- named handler + fallback
    io(err.message)
;
value = parse_number(text) err!:               -- named handler, must explicitly exit today
    return 0
;
```

Target syntax:

```beanstalk
value = parse_number(text)!                    -- bubble

value = parse_number(text) catch -> 0          -- fallback

value = parse_number(text) catch |err|:        -- handler + bubble on fallthrough
    io(err.message)
;

value = parse_number(text) catch |err| -> 0:   -- handler + fallback
    io(err.message)
;
```

## Non-goals

This is deliberately **not** the deeper generic `Result<T, E>` implementation.

Do not introduce:
- generic `Result` surface syntax
- `Ok` / `Err` pattern matching syntax
- expression-valued `match`
- implicit final-expression returns
- new result carrier layout
- new backend runtime representation
- compatibility support for old `err!` or `! fallback` syntax

The goal is a syntax and semantics update over the current internal result model.

## Design decisions to preserve

### `!` only means propagation or error return slot marking

`!` remains valid for:

```beanstalk
parse_number |text String| -> Int, Error!:
    return! Error("Parse", "int.empty", "Missing number")
;

value = parse_number(text)!
```

`!` must no longer mean fallback recovery.

Reject:

```beanstalk
value = parse_number(text) ! 0
```

### `catch` is the only new keyword

`catch` handles both old fallback and named-handler use cases.

Supported forms:

```beanstalk
expr catch -> fallback_values
expr catch |err|: body ;
expr catch |err| -> fallback_values: body ;
```

Do not add `recover`, `handle`, `rescue`, or `or` variants.

### `catch |err|:` fallthrough bubbles

This is the main semantic change from the current named-handler implementation.

```beanstalk
wrapper |text String| -> Int, Error!:
    value = parse_number(text) catch |err|:
        io(err.message)
    ;

    return value
;
```

If `parse_number(text)` succeeds, `value` receives the success value.

If it errors:
1. `err` is bound.
2. The catch body runs.
3. If the catch body reaches the end, the same error is returned from `wrapper`.

The catch body can override this with an explicit `return` or `return!`.

```beanstalk
recover |text String| -> Int:
    value = parse_number(text) catch |err|:
        io(err.message)
        return 0
    ;

    return value
;
```

This is valid because the catch body does not fall through.

### `catch -> fallback` has no error binding

```beanstalk
value = parse_number(text) catch -> 0
```

This recovers without exposing the error value.

### `catch |err| -> fallback:` runs the block before fallback

```beanstalk
value = parse_number(text) catch |err| -> 0:
    io(err.message)
;
```

On error:
1. Bind `err`.
2. Run the body.
3. If the body reaches the end, use the fallback values.
4. If the body explicitly returns, the fallback is not used.

## Current repo areas to update

Known current old-syntax owners:

```text
src/compiler_frontend/tokenizer/tokens.rs
src/compiler_frontend/tokenizer/lexer.rs

src/compiler_frontend/ast/expressions/function_calls.rs
src/compiler_frontend/ast/expressions/expression.rs

src/compiler_frontend/ast/statements/result_handling/mod.rs
src/compiler_frontend/ast/statements/result_handling/parser.rs
src/compiler_frontend/ast/statements/result_handling/named_handler.rs
src/compiler_frontend/ast/statements/result_handling/fallback.rs
src/compiler_frontend/ast/statements/result_handling/validation.rs
src/compiler_frontend/ast/statements/result_handling/propagation.rs

src/compiler_frontend/hir/hir_expression/calls.rs

src/compiler_frontend/ast/statements/tests/result_handling_tests.rs
src/compiler_frontend/ast/statements/tests/function_parsing_tests.rs
src/compiler_frontend/ast/statements/tests/collections_tests.rs

tests/cases/**/*
docs/src/docs/progress/#page.bst
```

Also search before and after the migration:

```bash
rg 'err!'
rg ' ! '
rg 'named handler|NamedResultHandler|named-handler'
rg 'expr ! fallback|call\(\.\.\.\) ! fallback|! fallback'
rg 'must be explicitly handled with .!. syntax'
rg 'parse_result_fallback_values'
rg 'ResultCallHandling::Handler'
```

Do not edit generated docs under `docs/release` directly. Rebuild them through the normal docs build.

---

# Phase 0 — Preflight audit and branch setup

## Context

The syntax is changing in a pre-alpha language. The implementation should remove old paths rather than preserving compatibility wrappers. This aligns with the project style guide: one current API shape, no parallel legacy entry points.

## Tasks

1. Create a branch.

   ```bash
   git checkout -b result-catch-syntax
   ```

2. Run targeted searches and save notes for files containing old syntax.

   ```bash
   rg 'err!' .
   rg ' ! ' src tests docs
   rg 'named handler|NamedResultHandler|named-handler' src tests docs
   rg 'ResultCallHandling::Handler|ResultCallHandling::Fallback' src
   ```

3. Classify hits into:
   - parser implementation
   - HIR lowering
   - diagnostics
   - unit tests
   - integration fixtures
   - docs source
   - generated docs

4. Confirm no unrelated result-generics work is included in the branch.

## Commit boundary

No code changes required.

Recommended commit if notes are kept in a local planning file:

```text
Audit old result handling syntax references
```

## Phase audit

Check:
- No generated docs edited manually.
- No generic `Result<T, E>` work added.
- No new compatibility layer planned for old syntax.

Validation:

```bash
cargo fmt --check
cargo test
cargo run tests
```

---

# Phase 1 — Add `catch` as a reserved keyword

## Context

`catch` must parse as a first-class token, not as a user symbol. This avoids fragile string comparisons against interned symbols and keeps result syntax centralized in parser code.

## Tasks

### 1. Update `TokenKind`

File:

```text
src/compiler_frontend/tokenizer/tokens.rs
```

Add:

```rust
Catch,
```

Place it near other control-flow/result-adjacent keywords. Good options:
- near `Return`
- near `Checked`
- near `Async`
- or under the `/// For Errors` comment before `Bang`

Preferred:

```rust
// Control Flow
If,
Else,
Return,
Catch,
Block,
Checked,
Async,
```

### 2. Update keyword tokenization

File:

```text
src/compiler_frontend/tokenizer/lexer.rs
```

In `keyword_or_variable`, add:

```rust
"catch" => return_token!(TokenKind::Catch, stream),
```

In `is_keyword`, add:

```rust
| "catch"
```

### 3. Check expression continuation behavior

Review:

```rust
TokenKind::continues_expression
```

Decide whether `Catch` should be included.

Recommended:
- Add `TokenKind::Catch` if multiline catch suffixes should be legal:

```beanstalk
value = parse_number(text)
    catch -> 0
```

- Otherwise leave it out and keep catch suffixes same-line for now.

Given Beanstalk's current newline sensitivity and desire for readable multiline syntax, I recommend adding `Catch` as an expression-continuation token.

### 4. Add tokenizer tests if this module has focused keyword tests

If no focused tokenizer keyword test exists, do not create a broad new tokenizer test module just for this. Parser tests will cover it.

## Commit boundary

```text
Tokenize catch as a result-handling keyword
```

## Phase audit

Check:
- `catch` cannot be used as a variable name after this phase.
- `is_keyword("catch")` returns true.
- No parser behavior changed yet.
- No old syntax was removed in this phase.

Validation:

```bash
cargo test tokenizer
cargo test
```

---

# Phase 2 — Rename parser concepts from “named handler” to “catch”

## Context

The current implementation uses “named handler” terminology because syntax was `err!`. Keeping that naming after the syntax changes will create design drift.

Do not do a huge semantic rewrite here. Rename the stage-local concepts first so the next parser changes are clear.

## Tasks

### 1. Rename files or keep files?

Recommended option:

Rename:

```text
src/compiler_frontend/ast/statements/result_handling/named_handler.rs
```

to:

```text
src/compiler_frontend/ast/statements/result_handling/catch_handler.rs
```

If the project prefers smaller diffs, keep the file name but rename types and comments. However, the file name will become misleading. I recommend renaming it.

### 2. Rename types

Old:

```rust
NamedResultHandler
NamedResultHandlerSite
parse_named_result_handler
validate_named_result_handler_binding
validate_named_result_handler_conflict
validate_named_result_handler_value_requirement
parse_named_handler_fallback
```

New:

```rust
ResultCatchHandler
ResultCatchSite
parse_result_catch
validate_result_catch_binding
validate_result_catch_conflict
validate_result_catch_fallthrough
parse_catch_fallback
```

Do not over-abstract. This remains result-specific.

### 3. Rename enum variant

File:

```text
src/compiler_frontend/ast/expressions/expression.rs
```

Current:

```rust
pub enum ResultCallHandling {
    Propagate,
    Fallback(Vec<Expression>),
    Handler {
        error_name: StringId,
        error_binding: InternedPath,
        fallback: Option<Vec<Expression>>,
        body: Vec<AstNode>,
    },
}
```

Recommended new shape:

```rust
pub enum ResultCallHandling {
    Propagate,
    Fallback(Vec<Expression>),
    Catch {
        error_name: Option<StringId>,
        error_binding: Option<InternedPath>,
        fallback: Option<Vec<Expression>>,
        body: Vec<AstNode>,
    },
}
```

Why keep `Fallback`?
- `catch -> fallback` has no body and no error binding.
- HIR lowering already has a simple fallback path.
- Keeping this variant avoids unnecessary branching complexity.

Why make catch binding optional?
- It allows future `catch:` support without changing the AST shape.
- It supports internal representation of `catch -> fallback` if later consolidated.
- For this migration, parser should still only produce `Catch` with `Some(error_name)` for block forms.

Alternative smaller shape:

```rust
Catch {
    error_name: StringId,
    error_binding: InternedPath,
    fallback: Option<Vec<Expression>>,
    body: Vec<AstNode>,
}
```

This is acceptable if `catch:` with no binding is deliberately rejected.

Recommended for now: keep `error_name` and `error_binding` non-optional unless you want to support no-bind catch blocks immediately.

### 4. Update all match sites

Known match sites:

```text
src/compiler_frontend/ast/module_ast/finalization/validate_types.rs
src/compiler_frontend/hir/hir_expression/calls.rs
src/compiler_frontend/ast/statements/tests/*.rs
src/compiler_frontend/hir/tests/hir_result_lowering_tests.rs
```

Only rename. Do not change logic yet.

## Commit boundary

```text
Rename result named handlers to catch handlers internally
```

## Phase audit

Check:
- No `NamedResultHandler` names remain unless intentionally in migration notes.
- No diagnostics still say “named handler”.
- Internal names do not imply old `err!` syntax.
- Semantics should still be equivalent after this phase.

Validation:

```bash
cargo fmt
cargo test
```

---

# Phase 3 — Replace `! fallback` with `catch -> fallback`

## Context

This is the first actual grammar change.

After this phase:
- `expr!` is propagation only.
- `expr ! fallback` is rejected.
- `expr catch -> fallback` parses as `ResultCallHandling::Fallback`.

## Tasks

### 1. Update expression result suffix parsing

File:

```text
src/compiler_frontend/ast/statements/result_handling/parser.rs
```

Current behavior:
- sees `TokenKind::Bang`
- if next token is propagation boundary, propagate
- otherwise parse fallback values

New behavior:
- sees `TokenKind::Bang`
- if next token is propagation boundary, propagate
- otherwise reject with diagnostic suggesting `catch ->`

Pseudo-shape:

```rust
if token_stream.current_token_kind() == &TokenKind::Bang {
    token_stream.advance();

    if !is_result_propagation_boundary(token_stream.current_token_kind()) {
        return_rule_error!(
            "The '!' suffix only propagates errors. Fallback recovery now uses 'catch ->'.",
            token_stream.current_location(),
            {
                CompilationStage => EXPRESSION_STAGE,
                PrimarySuggestion => "Write 'expr catch -> fallback' instead of 'expr ! fallback'",
            }
        );
    }

    // existing propagation validation
}
```

Then add catch fallback parsing:

```rust
if token_stream.current_token_kind() == &TokenKind::Catch {
    return parse_result_catch_for_expression(...);
}
```

For this phase, only implement:

```beanstalk
expr catch -> fallback_values
```

Block forms can be added in Phase 4.

### 2. Update call result suffix parsing

File:

```text
src/compiler_frontend/ast/expressions/function_calls.rs
```

Current behavior mirrors expression parsing. Apply the same split:
- `Bang` only propagates.
- `Bang` followed by non-boundary rejects with migration diagnostic.
- `Catch Arrow` parses fallback values.

Suggested diagnostic:

```text
The '!' call suffix only propagates errors. Fallback recovery now uses 'catch ->'.
```

Suggestion:

```text
Write 'call(...) catch -> fallback' instead of 'call(...) ! fallback'
```

### 3. Add shared helper for `catch -> fallback`

Avoid duplicating parsing between call and expression paths.

Recommended new helper in `result_handling/parser.rs` or a new small module:

```rust
parse_result_catch_fallback_values(
    token_stream,
    context,
    success_result_types,
    compilation_stage,
    string_table,
) -> Result<Vec<Expression>, CompilerError>
```

Behavior:
1. Current token is `Catch`.
2. Advance.
3. Require `Arrow`.
4. Advance.
5. Parse fallback values with existing `parse_result_fallback_values`.

Keep `parse_result_fallback_values` as the value-list parser, but update its comments so it is no longer tied to `!`.

### 4. Preserve success arity/type checks

`parse_result_fallback_values` already validates too many fallback values. Keep that centralized.

Ensure tests cover:
- single fallback
- multi fallback
- too many fallback values
- wrong fallback type

### 5. Old fallback syntax rejection

Add a focused negative unit test:

```beanstalk
recover |value String| -> String:
    return can_error(value) ! "fallback"
;
```

Expected message fragment:

```text
Fallback recovery now uses 'catch ->'
```

## Commit boundary

```text
Parse catch-arrow fallback result handling
```

## Phase audit

Check:
- No parser path still accepts `! fallback`.
- `parse_result_fallback_values` is still the only fallback value-list parser.
- Diagnostics give the new syntax.
- No HIR lowering changes should be necessary for plain fallback; it still lowers through `ResultCallHandling::Fallback`.

Validation:

```bash
cargo test result_handling
cargo test function_parsing
cargo test collections
```

---

# Phase 4 — Replace `err!` block syntax with `catch |err|`

## Context

This phase replaces old named handlers with catch blocks.

After this phase:
- `expr err! ...` is rejected.
- `expr catch |err|: ... ;` parses.
- `expr catch |err| -> fallback: ... ;` parses.

## Tasks

### 1. Parse catch block prefix

Current old shape:

```rust
Symbol(handler_name) Bang ...
```

New shape:

```rust
Catch TypeParameterBracket Symbol(handler_name) TypeParameterBracket ...
```

Syntax examples:

```beanstalk
catch |err|:
catch |err| -> 0:
catch |err| -> "guest", 0.0:
```

Parser steps:

1. Require `TokenKind::Catch`.
2. Advance.
3. If next token is `Arrow`, delegate to fallback-only parsing from Phase 3.
4. Otherwise require `TokenKind::TypeParameterBracket`.
5. Require `TokenKind::Symbol(handler_name)`.
6. Require closing `TokenKind::TypeParameterBracket`.
7. If next token is `Arrow`, parse fallback values, then require `Colon`.
8. If next token is `Colon`, parse no-fallback catch block.
9. Otherwise emit syntax error.

### 2. Reject unsupported catch shapes

Reject:

```beanstalk
expr catch
expr catch:
expr catch err:
expr catch err -> 0:
expr catch |err| ->:
expr catch |err| 0:
```

Diagnostics should be specific.

Suggested messages:
- `Expected '->' or '|err|' after 'catch'.`
- `Expected error binding between '|' markers after 'catch'.`
- `Expected ':' to start the catch block.`
- `Expected fallback values after '->'.`

### 3. Update old `err!` diagnostic

When the parser sees old shape:

```rust
TokenKind::Symbol(_) followed by TokenKind::Bang
```

after a result-valued expression or error-returning call, reject it explicitly.

Message:

```text
Old named error handler syntax is no longer supported.
```

Suggestion:

```text
Write 'catch |err| -> fallback: ... ;' or 'catch |err|: ... ;'
```

This is better than allowing a confusing generic parse error.

### 4. Update binding validation

Old validation should still apply:
- `err` cannot conflict with a visible declaration.
- `err` should probably remain the preferred binding name, but not required unless you deliberately want to enforce it.

Current docs show `catch |err|`, but if the old implementation allowed any handler name, keep allowing any valid binding.

Examples:

```beanstalk
value = parse_number(text) catch |error| -> 0:
    io(error.message)
;
```

Recommended:
- allow any legal lower_snake_case variable name
- warn or error according to existing binding-name policy, not a new special rule

### 5. Update comments

Replace comments like:

```rust
parses `err! ... : ... ;`
```

with:

```rust
parses `catch |err| ... : ... ;` result catch scopes
```

## Commit boundary

```text
Parse catch-block result handlers
```

## Phase audit

Check:
- No implementation path accepts `err!`.
- Old `err!` gives a targeted migration diagnostic.
- Catch binding uses normal declaration conflict rules.
- Parser logic is shared between call and expression result handling.
- No duplicate catch parser exists in call and expression paths.

Validation:

```bash
cargo fmt
cargo test result_handling
cargo test function_parsing
cargo test collections
```

---

# Phase 5 — Implement catch fallthrough bubbling semantics

## Context

The new `catch |err|:` semantics differ from old `err!:` semantics.

Old behavior:
- handler without fallback was valid only if the handler body explicitly returned when a value was required.

New behavior:
- handler without fallback may fall through if the surrounding function has a compatible error slot.
- fallthrough bubbles the caught error.
- if the surrounding function has no compatible error slot, the catch body must explicitly terminate.

This touches validation and HIR lowering.

## Tasks

### 1. Update AST validation

File likely:

```text
src/compiler_frontend/ast/statements/result_handling/validation.rs
```

Current logic likely rejects:

```text
Named handler without fallback can fall through
```

Replace with these rules:

#### Catch with fallback

```beanstalk
value = can_error() catch |err| -> fallback:
    body
;
```

Valid:
- if body falls through, fallback supplies success values
- if body explicitly returns, fallback is unreachable on that path but still allowed

Validation:
- fallback values must match success return arity/types

#### Catch without fallback, body always exits

```beanstalk
value = can_error() catch |err|:
    return 0
;
```

Valid even if surrounding function has no `Error!`.

Validation:
- body must be proven to terminate on all paths, or current function must have compatible error slot

#### Catch without fallback, body can fall through

```beanstalk
value = can_error() catch |err|:
    io(err.message)
;
```

Valid only if current function declares a compatible `Error!`.

Validation:
- if body can fall through and `context.expected_error_type` is absent, reject
- if present but mismatched, reject
- if matched, valid

### 2. Reuse existing control-flow termination analysis

Do not create a duplicate terminator checker.

Find the existing helper used by `validate_named_result_handler_value_requirement` or nearby control-flow validation. Extend/rename it rather than adding a parallel implementation.

### 3. Update HIR lowering for `Catch { fallback: None }`

File:

```text
src/compiler_frontend/hir/hir_expression/calls.rs
```

Current behavior for handler without fallback:
- if the handler body falls through and a merge value is required, HIR lowering errors.
- this must become automatic error bubbling.

New lowering for catch without fallback:

1. Result carrier branches.
2. Success branch unwraps `Ok` and assigns merge local.
3. Error branch unwraps `Err` into catch binding.
4. Lower catch body.
5. If catch body terminates, do not append anything.
6. If catch body falls through:
   - emit a return-error / result-propagation terminator using the caught error
   - do not jump to merge

Important: The error branch must terminate on fallthrough. It must not reach the merge block without a value.

### 4. Reuse return-error lowering

Do not create a second error-return lowering mechanism.

Search for the lowering path used by:

```beanstalk
return! err
```

or by result propagation. Use the same HIR terminator/statement shape.

If necessary, extract a small helper with a precise name, such as:

```rust
emit_return_error_from_expression(...)
```

Keep it HIR-local.

### 5. Preserve explicit return override

This must work:

```beanstalk
recover |text String| -> Int:
    value = parse_number(text) catch |err|:
        io(err.message)
        return 0
    ;

    return value
;
```

HIR should not append an automatic bubble after an explicit terminator.

### 6. Preserve fallback behavior

This must still recover:

```beanstalk
value = parse_number(text) catch |err| -> 0:
    io(err.message)
;
```

If the catch body falls through, assign fallback to merge local.

If the catch body explicitly returns, do not assign fallback afterward.

## Commit boundary

```text
Bubble caught errors on catch fallthrough
```

## Phase audit

Check:
- No HIR transformation error is used for valid catch fallthrough.
- Catch fallthrough produces normal user-facing semantics, not an internal compiler error.
- Automatic bubbling uses existing error-return machinery.
- Validation rejects impossible catch fallthrough before HIR where possible.
- No duplicate termination analysis was added.

Validation:

```bash
cargo test hir_result_lowering
cargo test result_handling
cargo test
```

---

# Phase 6 — Update compiler diagnostics

## Context

Diagnostics must teach the new syntax. Old messages mentioning `! fallback`, `err!`, or “named handler” will actively mislead users.

## Tasks

Search and update:

```bash
rg 'err!' src
rg '! fallback| ! fallback|expr !|call\(\.\.\.\) !' src
rg 'named handler|named-handler|Named handler' src
rg "handled with '!' syntax" src
```

Replace messages.

### Suggested diagnostic updates

Old:

```text
Calls to error-returning functions must be explicitly handled with '!' syntax
```

New:

```text
Calls to error-returning functions must be explicitly handled
```

Old suggestion:

```text
Use 'call(...)!' to propagate or 'call(...) ! fallback' to provide fallback values
```

New suggestion:

```text
Use 'call(...)!' to propagate or 'call(...) catch -> fallback' to recover
```

Old:

```text
Use 'expr!' for propagation, 'expr ! fallback' for fallback values, or 'expr err!: ... ;'
```

New:

```text
Use 'expr!' for propagation, 'expr catch -> fallback' to recover, or 'expr catch |err|: ... ;' for a catch block
```

Old:

```text
Bare 'err!' is invalid
```

New:

```text
Old 'err!' result handler syntax is no longer supported
```

Old:

```text
Expected '!' after named handler identifier.
```

New:
Should no longer exist after parser rewrite.

Old:

```text
Named handler without fallback can fall through
```

New:

```text
This catch block can fall through, but the surrounding function has no compatible error return slot
```

### io diagnostic

File:

```text
src/compiler_frontend/ast/expressions/function_calls.rs
```

Old suggestion likely says:

```text
Handle the Result with '!' syntax before passing it to io(...)
```

New:

```text
Handle the Result with '!' propagation or 'catch' recovery before passing it to io(...)
```

## Commit boundary

```text
Update result handling diagnostics for catch syntax
```

## Phase audit

Check:
- No user-facing diagnostic suggests old syntax.
- Error messages distinguish propagation from recovery.
- Old syntax rejection points to exact new replacement.

Validation:

```bash
cargo test
```

---

# Phase 7 — Update unit tests

## Context

Unit tests are dense with old result syntax. Update them before integration fixtures so parser behavior is stabilized locally.

## Tasks

Update at least:

```text
src/compiler_frontend/ast/statements/tests/result_handling_tests.rs
src/compiler_frontend/ast/statements/tests/function_parsing_tests.rs
src/compiler_frontend/ast/statements/tests/collections_tests.rs
src/compiler_frontend/hir/tests/hir_result_lowering_tests.rs
```

### Rewrite examples

Old:

```beanstalk
output = can_error(value) err! "fallback":
    io(err.message)
;
```

New:

```beanstalk
output = can_error(value) catch |err| -> "fallback":
    io(err.message)
;
```

Old:

```beanstalk
return can_error(value) ! "fallback"
```

New:

```beanstalk
return can_error(value) catch -> "fallback"
```

Old:

```beanstalk
return can_error(value) err!:
    return "recovered"
;
```

New:

```beanstalk
return can_error(value) catch |err|:
    return "recovered"
;
```

Old collection syntax:

```beanstalk
return values.get(idx) ! 0
```

New:

```beanstalk
return values.get(idx) catch -> 0
```

Old collection handler:

```beanstalk
return values.get(idx) err! 0:
    io(err.message)
;
```

New:

```beanstalk
return values.get(idx) catch |err| -> 0:
    io(err.message)
;
```

### Add focused new tests

Add or update tests for:

1. `catch -> fallback` in function-call expression position.
2. `catch -> fallback` for builtin result expression, such as collection `get`.
3. `catch |err| -> fallback:` in declaration RHS.
4. `catch |err|:` in a function with compatible `Error!` and fallthrough body.
5. `catch |err|:` in a function without `Error!` but explicit `return`.
6. `catch |err|:` in a function without `Error!` and fallthrough body is rejected.
7. old `err!` rejected with migration diagnostic.
8. old `! fallback` rejected with migration diagnostic.
9. fallback arity mismatch still rejected.
10. catch binding conflict still rejected.

### Update enum assertions

Old:

```rust
ResultCallHandling::Handler { .. }
```

New:

```rust
ResultCallHandling::Catch { .. }
```

If `Fallback` remains for `catch ->`, keep assertions for:

```rust
ResultCallHandling::Fallback(_)
```

## Commit boundary

```text
Update unit tests for catch result syntax
```

## Phase audit

Check:
- Unit tests no longer contain accepted old syntax.
- Negative tests intentionally contain old syntax and assert migration diagnostics.
- Tests cover call and expression paths.
- Tests cover collection result expressions.

Validation:

```bash
cargo test result_handling
cargo test function_parsing
cargo test collections
cargo test hir_result_lowering
cargo test
```

---

# Phase 8 — Update integration fixtures

## Context

Integration tests are the main user-visible language regression suite. They should reflect the new syntax and include rejection coverage for old syntax.

## Tasks

Search:

```bash
rg 'err!' tests/cases
rg ' ! ' tests/cases
rg 'named_handler|named_error_handler|result_handler' tests/cases/manifest.toml tests/cases
```

### Known likely fixtures to update

```text
tests/cases/error_field_access_in_handler/input/#page.bst
tests/cases/result_handler_nested_control_flow/input/#page.bst
tests/cases/result_handler_if_match_termination/input/#page.bst
tests/cases/error_helper_bubble_runtime_contract/input/#page.bst
tests/cases/result_named_handler_bare_err_rejected/input/#page.bst
tests/cases/adversarial_nested_named_error_handlers/input/#page.bst
tests/cases/error_helper_with_location_runtime_contract/input/#page.bst
tests/cases/result_handler_without_fallback_fallthrough_rejected/input/#page.bst
tests/cases/result_named_handler_scope_bubbles_error/input/#page.bst
tests/cases/error_helper_push_trace_runtime_contract/input/#page.bst
tests/cases/result_named_handler_scope_with_fallback/input/#page.bst
```

Also update any runtime result fallback cases:

```text
tests/cases/cast_float_invalid_format_fallback
tests/cases/cast_int_invalid_format_fallback
tests/cases/collection_get_out_of_bounds
tests/cases/collection_helpers_strict_runtime_contract
tests/cases/adversarial_*result*
```

### Rename fixture IDs where practical

Old names containing `named_handler` should be renamed to `catch`.

Examples:

```text
result_named_handler_scope_with_fallback
→ result_catch_scope_with_fallback

result_named_handler_scope_bubbles_error
→ result_catch_scope_bubbles_error

result_named_handler_bare_err_rejected
→ result_old_err_handler_rejected

adversarial_nested_named_error_handlers
→ adversarial_nested_result_catches
```

Update:

```text
tests/cases/manifest.toml
```

Avoid renaming if the churn is too large for the phase. But do not leave misleading fixture names long-term.

### Update expected diagnostics

For negative fixtures:
- old `err!` should now fail with the old-syntax migration diagnostic
- old `! fallback` should now fail with the fallback migration diagnostic
- catch fallthrough without compatible error slot should fail with the new catch fallthrough diagnostic

### Check generated artifacts

For success cases, update goldens only through the integration test workflow if required.

Do not manually guess generated JS/HTML changes.

## Commit boundary

```text
Update integration fixtures for catch result syntax
```

## Phase audit

Check:
- No success fixture uses old syntax.
- Negative fixtures using old syntax are intentionally named and expected.
- Manifest paths match renamed folders.
- Backend goldens changed only where expected.

Validation:

```bash
cargo run tests
```

---

# Phase 9 — Update docs, progress matrix, and implementation notes

## Context

The user has already updated the error page and language overview, but repo-wide references still need a final sweep. The implementation matrix must stay current when language behavior changes.

## Tasks

### 1. Search docs source

```bash
rg 'err!' docs
rg ' ! fallback| ! 0|catch ->|named handler' docs
```

Update source docs only.

Do not edit:

```text
docs/release/**
```

directly.

### 2. Update progress matrix

File:

```text
docs/src/docs/progress/#page.bst
```

Update any Result/error row that mentions:
- `err!`
- fallback through `!`
- named handlers
- old partial support notes

Suggested wording:

```text
Error propagation with trailing `!` is implemented.
Fallback recovery uses `catch ->`.
Catch blocks use `catch |err|:` and bubble on fallthrough unless they recover or explicitly return.
```

Keep status honest if HIR/backend support is still partial.

### 3. Update roadmap/audit docs only if they are intended active references

Likely file found earlier:

```text
docs/roadmap/audits/option-result-type-path-audit.md
```

If this is historical, add a short note rather than rewriting the whole audit:

```markdown
Note: result handler syntax has since moved from `err!` / `! fallback` to `catch`.
```

If it is intended as an active implementation reference, update examples fully.

### 4. Rebuild docs

Use the project command, not manual release edits.

Likely:

```bash
cargo run -- build docs --release
```

or the repo's existing `just validate` docs build command.

## Commit boundary

```text
Update result syntax docs and progress matrix
```

## Phase audit

Check:
- `docs/src` does not mention accepted old syntax.
- `docs/release` changes only come from rebuild.
- Progress matrix accurately reflects implementation state.
- Historical docs are either updated or clearly marked historical.

Validation:

```bash
cargo run -- build docs --release
cargo run tests
```

---

# Phase 10 — Final cleanup and validation

## Context

This phase removes migration leftovers and validates the whole compiler. It should be its own commit if cleanup changes are non-trivial.

## Tasks

### 1. Final search sweep

```bash
rg 'err!' .
rg ' ! fallback|expr !|call\(\.\.\.\) !|! 0' src tests docs
rg 'named handler|NamedResultHandler|named-handler' src tests docs
rg 'handled with .!. syntax' src tests docs
rg 'catch' src/compiler_frontend/tokenizer src/compiler_frontend/ast src/compiler_frontend/hir tests docs
```

Expected:
- `err!` appears only in negative old-syntax rejection tests or historical notes.
- `! fallback` appears only in negative old-syntax rejection tests or migration notes.
- `named handler` should be gone from active compiler code.

### 2. Remove obsolete helpers

If any helper exists only for old `err!` parsing, remove it.

Candidates:
- old `parse_named_result_handler`
- old `parse_named_handler_fallback`
- old diagnostic constants
- old test helper names

Do not keep wrappers that forward to new catch helpers.

### 3. Check comments and module docs

Update module docs in:

```text
src/compiler_frontend/ast/statements/result_handling/mod.rs
src/compiler_frontend/ast/statements/result_handling/*.rs
src/compiler_frontend/ast/expressions/function_calls.rs
```

Ensure comments say:
- `!` propagation
- `catch ->` fallback
- `catch |err|` scoped catch

### 4. Run full validation

Required:

```bash
just validate
```

If `just` is unavailable:

```bash
cargo clippy --all-targets --all-features -- -D warnings
cargo test
cargo run tests
cargo fmt --check
cargo run -- build docs --release
cargo run --release --features "detailed_timers" check speed-test.bst
```

Use the exact repo command if the Justfile differs.

## Commit boundary

```text
Clean up old result syntax references
```

## Phase audit

Check:
- No duplicated parser paths.
- No old syntax accepted accidentally.
- No stale docs or diagnostics.
- No generated docs manually edited.
- No unrelated generics/result-carrier changes.
- No user-input panics introduced.
- All new diagnostics include source locations.

---

# Expected final behavior matrix

| Source form | Expected behavior |
|---|---|
| `expr!` | Propagate error to surrounding compatible `Error!` function |
| `expr ! fallback` | Reject; suggest `expr catch -> fallback` |
| `expr catch -> fallback` | Recover with fallback values |
| `expr catch |err|: body ;` | Run body on error; bubble same error if body falls through |
| `expr catch |err| -> fallback: body ;` | Run body on error; recover with fallback if body falls through |
| `expr err! fallback: body ;` | Reject; suggest `expr catch |err| -> fallback: body ;` |
| `expr err!: body ;` | Reject; suggest `expr catch |err|: body ;` |
| `expr catch` | Reject incomplete catch |
| `expr catch:` | Reject unsupported no-binding block form |
| `expr catch |err| -> fallback` without `:` | Reject if block form was started and colon is missing |
| `catch |err|:` in non-error function with fallthrough | Reject |
| `catch |err|:` in non-error function with explicit return | Accept |
| `catch |err|:` in compatible error function with fallthrough | Accept; fallthrough bubbles |

---

# Risk notes

## HIR semantics are the highest-risk area

Parser migration is mechanical. The subtle part is catch fallthrough.

Do not allow this to become an internal HIR error:

```beanstalk
value = can_error() catch |err|:
    io(err.message)
;
```

In a compatible `Error!` function, this is valid and must lower to an error-branch terminator.

## Avoid duplicate fallback parsing

`catch -> fallback` and `catch |err| -> fallback:` should reuse the same fallback value-list parser.

## Avoid compatibility drift

Do not keep old `err!` as an alias.

This is pre-alpha. Rejection with a good diagnostic is better than two valid syntaxes.

## Keep `catch` result-specific

Do not make `catch` apply to options, booleans, or generic pattern matching in this migration.

---

# Suggested final PR / commit sequence

1. `Tokenize catch as a result-handling keyword`
2. `Rename result named handlers to catch handlers internally`
3. `Parse catch-arrow fallback result handling`
4. `Parse catch-block result handlers`
5. `Bubble caught errors on catch fallthrough`
6. `Update result handling diagnostics for catch syntax`
7. `Update unit tests for catch result syntax`
8. `Update integration fixtures for catch result syntax`
9. `Update result syntax docs and progress matrix`
10. `Clean up old result syntax references`

Each commit should pass at least the targeted tests listed in its phase. The final commit must pass `just validate`.
