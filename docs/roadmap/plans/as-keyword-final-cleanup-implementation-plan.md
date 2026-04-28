# Final cleanup plan: grouped import alias edge cases for `as`

## Goal

Close the remaining parser/diagnostic edge cases from the `as` keyword general-renaming work, without expanding the language surface.

This cleanup should make the implementation match the settled rules:

```beanstalk
-- Valid
import @components { render as render_component }
import @components/render as render_component
case Variant(original_name as local_name) => ...

-- Invalid
import @components { render } as render_component
import @components/render as render_component as other_name
import @components { render as bad-name }
```

The cleanup is intentionally narrow. It should not revisit import binding, external package resolution, Choice payload aliasing, HIR lowering, or docs status beyond documenting stricter diagnostics if needed.

## Current repo anchor

Relevant current files:

- `src/compiler_frontend/paths/const_paths.rs`
  - Owns path tokenization, grouped path expansion, per-entry alias parsing, and `parse_import_clause_items`.
  - Current grouped aliases are parsed inside `parse_grouped_entry`.
  - Current trailing import aliases are parsed in `parse_import_clause_items`.
- `src/compiler_frontend/tokenizer/tokens.rs`
  - Defines `PathTokenItem`.
  - `PathTokenItem` currently stores `path`, `alias`, `path_location`, and `alias_location`.
- `src/compiler_frontend/headers/file_parser.rs`
  - Consumes `parse_import_clause_items` and threads aliases into `FileImport`.
  - Should not need semantic changes for this cleanup.
- `tests/cases/manifest.toml`
  - Add canonical diagnostic cases here.
- `docs/language-overview.md`
  - Already documents aliases as file-local import syntax.
  - Only needs a small clarification if the diagnostics expose a user-visible grouped-alias rule not already obvious.
- `docs/src/docs/progress/#page.bst`
  - Already marks import aliases and Choice payload aliases as supported.
  - Only needs touch-up if the new tests should be named explicitly.

## Non-goals

- Do not change source/external import binding semantics.
- Do not change alias collision semantics.
- Do not change Choice payload aliasing.
- Do not add wildcard imports, namespace imports, or re-exports.
- Do not add path aliases outside import clauses.
- Do not relax alias identifier rules for grouped imports.
- Do not introduce compatibility wrappers for old token shapes.

---

# Phase 1 — Preserve grouped-origin metadata on path tokens

## Summary and reasoning

`parse_import_clause_items` can currently reject trailing aliases after grouped imports only when the expanded path token contains more than one item. This misses the single-entry grouped form:

```beanstalk
import @components { render } as render_component
```

This is semantically equivalent to `import @components/render as render_component`, but syntactically it violates the rule that grouped aliases must be per-entry. The parser needs to know whether a `PathTokenItem` came from grouped syntax, even when only one entry was produced.

The smallest reliable fix is to add a boolean to `PathTokenItem` such as `from_grouped: bool`.

## Implementation steps

### 1. Extend `PathTokenItem`

Update `src/compiler_frontend/tokenizer/tokens.rs`:

```rust
#[derive(Clone, Debug, PartialEq)]
pub struct PathTokenItem {
    pub path: InternedPath,
    pub alias: Option<StringId>,
    pub path_location: SourceLocation,
    pub alias_location: Option<SourceLocation>,
    pub from_grouped: bool,
}
```

Suggested field comment:

```rust
/// True when this entry came from grouped path syntax, even if the group
/// expanded to only one path.
```

Why this belongs on each item:

- The tokenizer already expands grouped paths into multiple entries.
- The import parser works from expanded entries.
- A path-token-level wrapper would be cleaner for “whole token was grouped”, but it causes more churn in all `TokenKind::Path` consumers.
- A per-item flag is enough for the only needed rule: reject trailing aliases if any item came from grouped syntax.

### 2. Populate `from_grouped`

Update `src/compiler_frontend/paths/const_paths.rs`.

For non-grouped path tokens, set:

```rust
from_grouped: false
```

This includes:

- exact `@/`
- ordinary `@path/to/symbol`

For grouped expansions, set:

```rust
from_grouped: true
```

This includes:

- `@base { a }`
- `@base { a, b }`
- nested grouped paths such as `@base { pages { home/render } }`

### 3. Update all `PathTokenItem` construction sites

Search for:

```rust
PathTokenItem {
```

Expected touched areas:

- `src/compiler_frontend/paths/const_paths.rs`
- tokenizer/path parser unit tests if they construct tokens manually

Prefer explicit field initialization over helper constructors unless repeated construction becomes noisy. This data shape is small and direct initialization is readable.

### 4. Reject trailing aliases after any grouped-origin path

Update `parse_import_clause_items` in `src/compiler_frontend/paths/const_paths.rs`.

Current logic effectively says:

```rust
if items.len() > 1 {
    reject group-level alias
}
```

Change to:

```rust
let path_uses_grouped_syntax = items.iter().any(|item| item.from_grouped);

if path_uses_grouped_syntax {
    return_syntax_error!(
        "Grouped imports cannot use a group-level alias. Add `as ...` to each grouped entry that needs renaming.",
        alias_token.location.clone(), {
            CompilationStage => "Header Parsing",
            PrimarySuggestion => "Write `import @path { item as local_name }`, or use `import @path/item as local_name` for a single import",
        }
    );
}
```

This rejects both:

```beanstalk
import @components { render, Button } as ui
import @components { render } as render_component
```

### 5. Keep non-import path consumers unchanged

Most non-import consumers should ignore `from_grouped`, because grouped paths are already legal in some path contexts and path aliases are already rejected where needed.

Do not add extra semantic meaning to grouped paths outside imports in this phase.

## Tests

Add focused parser or integration cases.

### Required test cases

1. `grouped_import_single_entry_trailing_alias_rejected`

```beanstalk
import @components { render } as render_component
```

Expected:

- Syntax or rule error from header/import parsing.
- Message should mention grouped imports cannot use group-level aliases.

2. Keep or add coverage for existing multi-entry rejection:

```beanstalk
import @components { render, Button } as ui
```

If no canonical case exists, add:

`grouped_import_multi_entry_trailing_alias_rejected`

### Optional unit coverage

If path parser tests already assert token payloads, add a small token-level test proving:

- `@components/render` produces `from_grouped = false`
- `@components { render }` produces `from_grouped = true`

This is not mandatory if integration coverage is enough, but it makes future path-token refactors safer.

## Audit, style guide review, and validation

This should be its own commit after Phase 1.

### Audit checklist

- `from_grouped` is populated at every `PathTokenItem` construction site.
- `parse_import_clause_items` rejects trailing aliases after grouped syntax even when the group has one item.
- Non-import path consumers are not given new semantic behavior accidentally.
- Error text is specific and actionable.
- No user-input `panic!`, `todo!`, unchecked indexing, or unnecessary `unwrap()` was added.
- Comments explain why grouped-origin metadata exists.

### Validation

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
```

If the project validation command exists:

```bash
just validate
```

### Commit

Suggested commit:

```text
reject grouped import trailing aliases
```

---

# Phase 2 — Validate grouped import alias identifiers like normal symbols

## Summary and reasoning

Single import aliases already go through `TokenKind::Symbol`, so syntax like this is rejected:

```beanstalk
import @components/render as bad-name
```

Grouped aliases currently parse the alias text through path-component parsing. That risks accepting path-valid but symbol-invalid aliases:

```beanstalk
import @components { render as bad-name }
import @components { render as 123render }
import @components { render as "render alias" }
```

Grouped import aliases should use the same identifier policy as normal aliases. The local visible name is a Beanstalk symbol, not a path component.

## Implementation steps

### 1. Add an alias-name validator helper

Update `src/compiler_frontend/paths/const_paths.rs`.

Add a helper near the grouped alias parser:

```rust
fn validate_import_alias_symbol(
    alias: &str,
    location: SourceLocation,
) -> Result<(), CompilerError> {
    // enforce same rules as tokenizer Symbol names
}
```

Do not invent a second identifier language if there is already a shared helper in the tokenizer. Search first for existing functions used when tokenizing `TokenKind::Symbol`.

Possible implementation choices:

#### Preferred

Reuse the tokenizer’s existing identifier validation logic by extracting it into a shared helper if it is currently private.

Potential location:

- `src/compiler_frontend/tokenizer/identifier.rs`
- or a small existing tokenizer syntax utility module

This avoids drift between grouped aliases and ordinary symbols.

#### Acceptable if no helper exists

Create a private helper in `const_paths.rs` that matches the current `Symbol` rules exactly enough for now:

- first character must be alphabetic or `_`
- remaining characters must be alphanumeric or `_`
- reject empty names
- reject quoted components
- reject names containing `-`, `.`, `/`, `\\`, whitespace, quotes, or path-reserved punctuation
- reject reserved keywords only if ordinary symbol tokenization rejects them as symbols

Before implementing, inspect the tokenizer’s symbol rules so this helper does not accidentally reject valid language identifiers.

### 2. Track whether the parsed alias was quoted

`parse_bare_component` returns `ParsedComponent { value, was_quoted }`, but grouped alias parsing currently calls `parse_bare_component`, so quoted aliases are already rejected before alias validation.

Keep that behavior. Do not support quoted aliases.

If the parser is refactored to use `parse_component`, explicitly reject `was_quoted`.

### 3. Apply validation in `parse_grouped_entry`

Current code:

```rust
let alias_component =
    parse_bare_component(stream, ParseComponentContext::GroupedEntry, string_table)?;

alias = Some(string_table.intern(&alias_component.value));
```

Change flow:

```rust
let alias_start = stream.position;
let alias_component =
    parse_bare_component(stream, ParseComponentContext::GroupedEntry, string_table)?;
let alias_end = stream.position;
let location = SourceLocation::new(stream.file_path.to_owned(), alias_start, alias_end);

validate_import_alias_symbol(&alias_component.value, location.clone())?;

alias = Some(string_table.intern(&alias_component.value));
alias_location = Some(location);
```

Diagnostic:

```text
Import alias must be a valid local binding name.
```

Suggested metadata:

- `CompilationStage => "Tokenization"` or `"Header Parsing"`
- `PrimarySuggestion => "Use a normal identifier such as `render_component`"`

Since this error arises while path tokenization is still running, `"Tokenization"` is acceptable. If the helper lives in import parsing later, use `"Header Parsing"`.

### 4. Add targeted missing-alias behavior if needed

Check what currently happens for:

```beanstalk
import @components { render as }
```

If it produces a path-component-empty error, make it more specific:

```text
Expected alias name after `as` in grouped import entry.
```

This can live in `parse_grouped_entry` immediately after `consume_keyword_as`.

Do not overbuild recovery. A single specific error is enough.

## Tests

### Required test cases

1. `grouped_import_alias_invalid_dash_rejected`

```beanstalk
import @components { render as bad-name }
```

Expected:

- structured syntax/rule error
- message mentions valid local binding / alias name

2. `grouped_import_alias_invalid_leading_digit_rejected`

```beanstalk
import @components { render as 123render }
```

Expected:

- structured syntax/rule error

3. `grouped_import_alias_missing_name_rejected`

```beanstalk
import @components { render as }
```

Expected:

- targeted missing alias diagnostic

### Optional test cases

4. `grouped_import_alias_keyword_rejected`

Only add this if the normal tokenizer rejects the chosen keyword as a symbol:

```beanstalk
import @components { render as if }
```

5. `grouped_import_alias_valid_underscore_success`

Only needed if there is no existing positive grouped alias test with snake_case:

```beanstalk
import @components { render as render_component }
```

## Audit, style guide review, and validation

This should be its own commit after Phase 2.

### Audit checklist

- Single import alias and grouped import alias naming rules are equivalent.
- Alias validation is not looser than path component validation.
- No duplicated identifier policy exists if a shared tokenizer helper was available.
- Diagnostics point to the alias span when possible.
- Missing alias after grouped `as` gives a targeted message.
- No unrelated import binding behavior changed.

### Validation

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
```

If available:

```bash
just validate
```

### Commit

Suggested commit:

```text
validate grouped import alias names
```

---

# Phase 3 — Add explicit double-alias diagnostics

## Summary and reasoning

The parser already rejects the combination of per-entry aliases plus trailing aliases:

```beanstalk
import @components { render as r } as x
```

But the single-import double-alias form should receive its own targeted diagnostic:

```beanstalk
import @components/render as render_component as other_name
```

Without an explicit check, the second `as` may be left behind and reported later as a confusing top-level or expression-level error.

## Implementation steps

### 1. Reject a second trailing `as`

Update `parse_import_clause_items` in `src/compiler_frontend/paths/const_paths.rs`.

After consuming a trailing alias and advancing `index`, check:

```rust
if tokens
    .get(index)
    .is_some_and(|token| matches!(token.kind, TokenKind::As))
{
    return_syntax_error!(
        "Import clauses can only have one alias.",
        tokens[index].location.clone(), {
            CompilationStage => "Header Parsing",
            PrimarySuggestion => "Remove the second `as ...` alias",
        }
    );
}
```

This catches:

```beanstalk
import @components/render as render_component as other_name
```

### 2. Reject grouped per-entry double alias if not already caught

For this:

```beanstalk
import @components { render as r as x }
```

The grouped parser may currently parse `r` and then leave `as x` where it expects a comma or `}`.

If the existing diagnostic is vague, add a specific check inside `parse_grouped_entry` after parsing the alias:

```rust
consume_all_whitespace(stream);
if stream.peek().copied() == Some('a') && peek_keyword_as(stream) {
    return_syntax_error!(
        "Grouped import entries can only have one alias.",
        stream.new_location(), {
            CompilationStage => "Tokenization",
            PrimarySuggestion => "Remove the second `as ...` alias",
        }
    );
}
```

Make sure this does not consume the token before reporting unless the source location remains accurate.

### 3. Keep per-entry + trailing alias diagnostic

The existing rejection remains useful:

```beanstalk
import @components { render as r } as x
```

After Phase 1, this should fail earlier because any grouped-origin path cannot use a trailing alias. That is fine.

Prefer the group-level alias diagnostic for this form. It is clearer than “double alias”.

## Tests

### Required test cases

1. `single_import_double_alias_rejected`

```beanstalk
import @components/render as render_component as other_name
```

Expected:

- targeted syntax/rule error
- message mentions only one alias / remove second alias

2. `grouped_import_entry_double_alias_rejected`

```beanstalk
import @components { render as render_component as other_name }
```

Expected:

- targeted syntax/rule error

3. `grouped_import_per_entry_and_trailing_alias_rejected`

```beanstalk
import @components { render as render_component } as other_name
```

Expected:

- grouped import group-level alias rejection

## Audit, style guide review, and validation

This should be its own commit after Phase 3.

### Audit checklist

- All double-alias forms produce specific diagnostics.
- Error locations point at the second `as` where possible.
- The parser does not leave stray `as` tokens to later stages.
- Existing valid forms still parse:
  - `import @x/y as z`
  - `import @x { y as z }`
  - `import @x { y, z as renamed_z }`
- Existing invalid group-level alias forms still reject.
- No downstream header parser/import binder changes were needed.

### Validation

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
```

If available:

```bash
just validate
```

### Commit

Suggested commit:

```text
diagnose duplicate import aliases
```

---

# Phase 4 — Documentation and progress-matrix touch-up

## Summary and reasoning

This cleanup does not add new language features. It tightens invalid syntax. Documentation should stay minimal: update only if the stricter grouped-alias rules are not already clear enough.

The goal is to avoid doc churn while making the maintenance matrix point at the new canonical diagnostic coverage.

## Implementation steps

### 1. Update `docs/language-overview.md` only if useful

The current import docs already show per-entry grouped aliases. Add one short rule only if not already present:

```md
- Grouped imports cannot use a trailing group-level alias. Alias individual entries instead:
  `import @components { render as render_component }`.
```

Do not add a long examples section.

### 2. Update `docs/src/docs/progress/#page.bst`

In the `Paths and imports` row, add the new diagnostic cases to coverage/watch points if the matrix lists specific import tests.

Suggested wording:

```text
Grouped import aliases reject group-level trailing aliases, invalid local alias names, and duplicate alias syntax with structured diagnostics.
```

### 3. Do not update Choice docs

Choice payload aliasing is unaffected.

## Tests

No additional runtime tests are needed in this phase unless docs builds fail.

## Audit, style guide review, and validation

This should be its own commit after Phase 4.

### Audit checklist

- Docs describe the language rule, not implementation internals.
- Progress matrix remains honest and not overly verbose.
- New test names in the matrix match `tests/cases/manifest.toml`.
- No release/generated docs are edited manually unless that is the repo convention.

### Validation

Run:

```bash
cargo fmt --check
cargo run --features "detailed_timers" docs
cargo run tests
```

If available:

```bash
just validate
```

### Commit

Suggested commit:

```text
document grouped import alias diagnostics
```

---

# Final review phase — Close the cleanup

## Summary and reasoning

After the parser and docs phases, do a final focused review of the touched area. The desired result is boring: all three edge cases fail cleanly, all existing valid alias syntax still works, and no semantic import code was disturbed.

## Final checklist

### Behavior

Valid forms still work:

```beanstalk
import @components/render as render_component
import @components { render as render_component }
import @components { render, Button as UiButton }
import @docs { pages/home/render as render_home }
```

Invalid forms now reject cleanly:

```beanstalk
import @components { render } as render_component
import @components { render, Button } as ui
import @components/render as render_component as other_name
import @components { render as render_component as other_name }
import @components { render as bad-name }
import @components { render as 123render }
import @components { render as }
```

### Code ownership

- `const_paths.rs` owns syntactic parsing and diagnostics for grouped path/import alias syntax.
- `tokens.rs` owns the token payload shape only.
- `headers/file_parser.rs` remains a consumer of parsed import items.
- `ast/import_bindings.rs` remains semantic import resolution only.
- No semantic resolver code changed for parser-only cleanup.

### Style guide

- New comments explain why `from_grouped` exists.
- No compatibility layers or old API shims were added.
- No user-input panics were added.
- Helpers are small and named by behavior.
- Diagnostics are structured and actionable.

### Test coverage

Confirm manifest entries exist for:

```text
grouped_import_single_entry_trailing_alias_rejected
grouped_import_alias_invalid_dash_rejected
grouped_import_alias_invalid_leading_digit_rejected
grouped_import_alias_missing_name_rejected
single_import_double_alias_rejected
grouped_import_entry_double_alias_rejected
grouped_import_per_entry_and_trailing_alias_rejected
```

Names can differ if the repo already uses a specific naming convention, but the behavior should be covered.

## Final validation

Run the full commit-gate suite:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

Or:

```bash
just validate
```

## Final commit

Suggested commit:

```text
review import alias parser cleanup
```

## Completion criteria

Development can move on when:

- All required invalid syntax forms are covered by tests.
- Valid grouped and single import aliases still pass.
- Docs/matrix are updated or deliberately left unchanged with a clear reason in the commit body.
- Full validation passes.
- The final review finds no changes needed in import binding, external package resolution, Choice payload matching, or HIR lowering.
