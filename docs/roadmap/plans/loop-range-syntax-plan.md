# Loop Range Syntax Plan

## Goal

Refine Beanstalk range-loop syntax in two linked ways:

1. Replace the current inclusive range keyword `upto` with `&` as an **inclusive end marker** after `to`.
2. Add range sugar that allows omitting the leading `0` before `to`.

Target examples:

```beanstalk
loop 0 to 12:
loop 0 to & 12:

loop to 12:
loop to & 12:

loop 0 to get_count() |i|:
loop to & get_count() |i|:
```

This keeps `to` as the single visible range operator while making inclusivity a compact modifier on the end bound.

---

## Why this change

Current loop syntax uses `to` for exclusive ranges and `upto` for inclusive ranges. The compiler already models this internally as a simple exclusive/inclusive end distinction rather than as two fundamentally different loop forms, so the user-facing syntax can change without redesigning HIR semantics.

The current implementation already separates:

- tokenization of range markers in `src/compiler_frontend/tokenizer/lexer.rs`
- token definitions in `src/compiler_frontend/tokenizer/tokens.rs`
- loop header parsing in `src/compiler_frontend/ast/statements/loops.rs`
- inclusive vs exclusive lowering in `src/compiler_frontend/hir/hir_statement/loop_lowering.rs`

That means this is mainly a frontend surface-syntax update plus docs/tests migration, not a deep control-flow redesign.

---

## Final syntax contract

### Exclusive range

```beanstalk
loop start to end:
```

Examples:

```beanstalk
loop 0 to 10 |i|:
loop to 10:
loop to get_count() |i|:
```

### Inclusive range

```beanstalk
loop start to & end:
```

Examples:

```beanstalk
loop 0 to & 10 |i|:
loop to & 10:
loop 10 to & 0 by 2 |i|:
```

### Omitted-start sugar

When a range loop starts directly with `to`, it desugars to a range starting at `0`.

```beanstalk
loop to end:
```

becomes semantically equivalent to:

```beanstalk
loop 0 to end:
```

and:

```beanstalk
loop to & end:
```

becomes semantically equivalent to:

```beanstalk
loop 0 to & end:
```

### Spacing rule

`&` does **not** need to be adjacent to the end expression.

All of these should be valid and equivalent:

```beanstalk
loop 0 to &12:
loop 0 to & 12:
loop to &get_count():
loop to & get_count():
```

### Non-goals

This change does **not** introduce:

- bare integer loop sugar such as `loop 12:`
- any change to collection loop syntax
- any change to conditional loop syntax
- any HIR-level new loop kind

---

## Repo touchpoints

These are the main files an agent should inspect and update.

### 1. Token definitions

**File:** `src/compiler_frontend/tokenizer/tokens.rs`

Current relevant state:

- `TokenKind::ExclusiveRange` is used for `to`
- `TokenKind::InclusiveRange` is used for `upto`
- there is currently no token for `&`

Planned work:

- add a token for `&` (for example `TokenKind::Ampersand` or `TokenKind::InclusiveMarker`)
- remove the `upto` keyword mapping if it is being retired outright
- keep the existing `RangeEndKind::{Exclusive, Inclusive}` AST/HIR contract unchanged

### 2. Lexer keyword and symbol mapping

**File:** `src/compiler_frontend/tokenizer/lexer.rs`

Current relevant state:

- `keyword_or_variable()` maps `"to"` to `TokenKind::ExclusiveRange`
- `keyword_or_variable()` maps `"upto"` to `TokenKind::InclusiveRange`
- `get_token_kind()` currently does not tokenize `&`

Planned work:

- add `&` tokenization in `get_token_kind()`
- remove `"upto" => TokenKind::InclusiveRange` if the old keyword is being fully removed
- keep `"to"` as the only range keyword

### 3. Reserved keyword policy

**File:** `src/compiler_frontend/symbols/identifier_policy.rs`

Current relevant state:

- the reserved keyword shadow list includes both `to` and `upto`

Planned work:

- remove `upto` from the reserved keyword shadow list if the keyword is retired
- do not add `&` here because symbol tokens are not identifier-like keyword shadows

### 4. Loop header parsing

**File:** `src/compiler_frontend/ast/statements/loops.rs`

Current relevant state:

- range detection uses top-level presence of `ExclusiveRange` or `InclusiveRange`
- `parse_range_loop_spec_from_tokens()` expects a parsed start expression followed by either `to` or `upto`
- diagnostics currently mention `to` / `upto`

Planned work:

- preserve one range-loop AST shape: `RangeLoopSpec { start, end, end_kind, step }`
- update parsing to support:
  - `start to end`
  - `start to & end`
  - `to end`
  - `to & end`
- interpret `&` after `to` as `RangeEndKind::Inclusive`
- when the header begins with `to`, synthesize the start expression as integer literal `0`
- update diagnostics so they refer to `to` and `to &` instead of `to` and `upto`

### 5. HIR lowering

**File:** `src/compiler_frontend/hir/hir_statement/loop_lowering.rs`

Current relevant state:

- HIR lowering already branches on `RangeEndKind::Exclusive` vs `RangeEndKind::Inclusive`
- it does not care what source syntax produced that distinction

Planned work:

- no semantic redesign needed
- only verify that parser changes still produce the same `RangeEndKind` values
- keep existing lowering behavior unchanged

### 6. Unit-level loop parsing tests

**File:** `src/compiler_frontend/ast/statements/tests/loop_parsing_tests.rs`

Current relevant state:

- tests currently cover `upto` and `to`
- tests already cover many loop-header diagnostics

Planned work:

- replace `upto` coverage with `to &`
- add explicit tests for omitted-start range sugar
- add tests for flexible whitespace around `&`
- update error assertions that still mention `upto`

### 7. Integration fixtures

**Files:**

- `tests/cases/loop_conditional_and_range/input/#page.bst`
- `tests/cases/loop_range_direction_and_step/input/#page.bst`

Current relevant state:

- these fixtures currently use `upto`
- they validate runtime semantics for inclusive ranges and descending/step behavior

Planned work:

- migrate fixture syntax from `upto` to `to &`
- add or extend fixtures to cover omitted-start forms like `loop to 5:` and `loop to & 5:`
- keep expected rendered outputs semantically equivalent

### 8. User-facing loop docs

**Files:**

- `docs/language-overview.md`
- `docs/src/docs/loops/#page.bst`

Current relevant state:

- both docs explain `to` vs `upto`
- examples and prose assume the old inclusive keyword

Planned work:

- rewrite range-loop docs around:
  - `to` = exclusive
  - `to &` = inclusive
  - omitted `0` before `to`
- update examples, bullet lists, and migration wording

---

## Implementation steps

### Step 1: Update token surface

1. Add a token for `&` in `TokenKind`.
2. Teach the lexer to emit that token.
3. Remove `upto` keyword tokenization if the migration is intended to be immediate rather than transitional.

### Step 2: Update loop parser contract

In `parse_loop_header()` / `parse_range_loop_spec_from_tokens()`:

1. Continue using `to` as the syntax-defining range marker.
2. After consuming `to`, optionally consume `&` before parsing the end expression.
3. Map presence of `&` to `RangeEndKind::Inclusive`; absence means `Exclusive`.
4. Add omitted-start parsing:
   - if the header begins with `to`, synthesize a start expression for integer literal `0`
   - then parse optional `&`, then parse the end expression normally
5. Leave `by` parsing exactly where it is now.

### Step 3: Keep semantics stable

Do **not** add a new AST or HIR loop kind.

The parser should still produce the same `RangeLoopSpec` structure and the same `RangeEndKind` values used by HIR lowering.

### Step 4: Rewrite diagnostics

Update loop diagnostics so they consistently teach the new syntax.

Examples:

- replace “Use 'upto' for an inclusive end bound” with “Use `to & end` for an inclusive end bound”
- update missing-range-marker diagnostics to say “Range loops must include `to` between bounds”
- when helpful, show examples using both:
  - `loop 0 to 10 |i|:`
  - `loop 0 to & 10 |i|:`
  - `loop to 10:`

### Step 5: Update tests

#### Parser/unit tests

Add or migrate tests for:

```beanstalk
loop 0 to & 5 |i|:
loop 0 to &5 |i|:
loop to 5:
loop to & 5:
loop to get_count() |i|:
loop to & get_count() |i|:
```

Also add rejection tests for malformed headers such as:

```beanstalk
loop to:
loop to &:
loop 0 to &:
```

#### Integration tests

Cover semantic parity for:

- exclusive omitted-start loop
- inclusive omitted-start loop
- descending inclusive loop using `to &`
- negative step normalization still behaving the same

### Step 6: Update docs and examples

1. Rewrite loop docs in `docs/language-overview.md`
2. Rewrite the docs site page in `docs/src/docs/loops/#page.bst`
3. Update any other loop examples found by repo search for `upto`

### Step 7: Validation

Run the normal repo validation workflow from the style guide:

```bash
cargo clippy
cargo test
cargo run tests
```

---

## Migration choice

Decide this before implementation:

### Option A: hard replacement

- `upto` becomes invalid immediately
- all docs/tests/examples move to `to &`
- simplest long-term surface

### Option B: temporary compatibility window

- parser accepts both `upto` and `to &`
- diagnostics or warnings steer users toward `to &`
- remove `upto` later

**Recommendation:** use **Option A** unless there is a strong need to preserve authored examples temporarily. The repo is pre-alpha and the codebase guidance explicitly does not prioritize backward compatibility wrappers.

---

## Acceptance criteria

The work is complete when all of the following are true:

- `upto` is removed or intentionally deprecated across lexer, parser, docs, and tests
- `loop 0 to & 5:` parses as an inclusive range loop
- `loop to 5:` parses as `0 to 5`
- `loop to & 5:` parses as `0 to & 5`
- whitespace around `&` does not affect meaning
- HIR lowering behavior remains unchanged for inclusive/exclusive semantics
- parser tests and integration tests cover the new forms
- docs only teach the new syntax

---

## Suggested agent implementation order

1. `src/compiler_frontend/tokenizer/tokens.rs`
2. `src/compiler_frontend/tokenizer/lexer.rs`
3. `src/compiler_frontend/ast/statements/loops.rs`
4. `src/compiler_frontend/ast/statements/tests/loop_parsing_tests.rs`
5. `tests/cases/loop_conditional_and_range/input/#page.bst`
6. `tests/cases/loop_range_direction_and_step/input/#page.bst`
7. `docs/language-overview.md`
8. `docs/src/docs/loops/#page.bst`
9. full validation: `cargo clippy && cargo test && cargo run tests`

---

## Roadmap summary snippet

Planned loop range syntax cleanup:
replace inclusive `upto` with `to & end`, and add omitted-start sugar so `loop to end:` desugars to `loop 0 to end:`. This is a frontend/docs/tests migration anchored mainly in `src/compiler_frontend/tokenizer/{tokens,lexer}.rs`, `src/compiler_frontend/ast/statements/loops.rs`, loop parser tests, integration loop fixtures, and loop docs. Full plan: `docs/plans/loop-range-syntax-plan.md`
