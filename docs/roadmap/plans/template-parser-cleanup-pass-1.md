# Template Parser Cleanup Plan - Pass 1

## Goal

This pass is the low-risk cleanup and correctness pass for AST template parsing, formatting, and folding.

It should make the existing pipeline cheaper and easier to reason about without changing template semantics.

The focus is:

- Fix the known no-op formatting performance bug.
- Remove redundant legacy state and old test-only code.
- Reduce misleading comments and stale terminology.
- Add regression coverage around the current fragile points.
- Keep the current module structure mostly intact.

Do **not** centralize directive argument parsing, add slot-schema caching, or perform deeper template/render-plan unification in this pass. Those belong in Pass 2.

## Non-goals

- No redesign of slot composition.
- No rewrite of `TemplateContent` / `TemplateRenderPlan` ownership.
- No new template language features.
- No compatibility shims for removed APIs.
- No benchmark interpretation from a single run.

## Baseline performance benchmark

Before making code changes, collect a baseline with release builds and detailed timers.

Build once first so the first measured run does not include release compilation cost:

```bash
cargo build --release --features "detailed_timers"
```

Run each benchmark once as a warm-up and do not record it:

```bash
cargo run --release --features "detailed_timers" -- check speed-test.bst
cargo run --release --features "detailed_timers" -- check docs
```

Then run each command at least 5 times and record the timings:

```bash
cargo run --release --features "detailed_timers" -- check speed-test.bst
cargo run --release --features "detailed_timers" -- check docs
```

Record:

| Benchmark | Run 1 | Run 2 | Run 3 | Run 4 | Run 5 | Average | Notes |
|---|---:|---:|---:|---:|---:|---:|---|
| `check speed-test.bst` | | | | | | | |
| `check docs` | | | | | | | |

If the detailed timer output exposes AST/template-specific timings, record those separately as well. The full wall-clock result is useful, but the template-parser refactor should be judged mostly by frontend/template-stage movement.

## Step 1 - Fix no-op formatting change detection

### Problem

`apply_body_formatter` drains `plan.pieces` with `std::mem::take`, then compares the new pieces against the now-empty plan. This makes most non-empty plans look changed, even when formatting was a no-op.

That defeats the skip path in `finalize_template_after_formatting` and causes unnecessary render-plan/content round trips.

### Implementation

In `src/compiler_frontend/ast/templates/template_formatting.rs`:

1. Store the original pieces before processing.
2. Iterate over the stored original pieces.
3. Compare `new_plan_pieces` against the original pieces.
4. Assign `plan.pieces = new_plan_pieces` only after the comparison.

Target shape:

```rust
let original_pieces = std::mem::take(&mut plan.pieces);

for piece in original_pieces.iter().cloned() {
    // existing run processing
}

let content_changed = render_pieces_changed(&original_pieces, &new_plan_pieces);
plan.pieces = new_plan_pieces;
```

Extract the comparison into a small helper if it improves readability.

### Tests

Add a focused unit test proving that a no-op body-formatting pass returns `content_changed == false`.

Cover at least:

- Plain text with no explicit formatter.
- A template with no body text.
- A runtime/dynamic expression anchor that remains unchanged.

## Step 2 - Harden formatter anchor handling

### Problem

Formatter output maps opaque anchors back through direct indexing. A bad formatter output can panic the frontend.

Frontend code should return structured diagnostics for malformed user/project-provided behavior, not panic on invalid data.

### Implementation

In `template_formatting.rs`:

- Replace direct `anchor_side_table[anchor.id.0]` indexing with checked access.
- Return a compiler error if the anchor id is invalid.
- Include the anchor id and side-table length in the internal diagnostic.

Example diagnostic intent:

```text
Template formatter returned invalid opaque anchor id 12; only 3 anchors exist for this formatter run.
```

### Tests

Add a test formatter that emits an invalid anchor id and assert that formatting returns an error instead of panicking.

## Step 3 - Remove `Template::create_default(Vec<Template>)` inheritance parameter

### Problem

`Template::create_default` accepts inherited templates, converts them into `TemplateInheritance`, then discards the result. This is dead API surface and makes call sites misleading.

### Implementation

Replace:

```rust
Template::create_default(vec![])
```

with one of:

```rust
Template::empty()
```

or:

```rust
Template::default()
```

Prefer `Template::empty()` if `Default` would hide too much semantic meaning.

Update all production and test call sites.

### Constraints

- Do not add a compatibility wrapper.
- Do not keep the old function around.
- Make the final constructor name obvious at call sites.

## Step 4 - Delete `explicit_style`

### Problem

`Template` carries both `style` and `explicit_style`, but the current code updates them together through `apply_style` and `apply_style_updates`. That means they no longer model a useful distinction.

### Implementation

Remove `explicit_style` from:

- `Template`
- `clone_for_composition`
- `apply_style`
- `apply_style_updates`
- `$children` directive handling
- tests that assert or construct `explicit_style`

After this change, `style` is the single source of truth for template style state.

### Constraints

- Do not preserve `explicit_style` as dead state for future plans.
- If a real future distinction is needed, reintroduce it later with a concrete consumer and a documented invariant.

## Step 5 - Delete old test-only template concatenation code if still unused

### Problem

`concat_template` is currently test-only and appears to preserve an older template-concatenation concept.

### Implementation

Remove:

- `concat_template` from expression evaluation tests/support.
- The `#[cfg(test)]` re-export from `eval_expression/mod.rs`.
- Tests whose only purpose is validating this obsolete helper.

Keep any tests that still validate active expression behavior.

### Constraint

Only keep this helper if template concatenation is still a real language feature with production code using it. If it is only test ballast, delete it.

## Step 6 - Remove stale terminology and misleading comments

### Required cleanup

Search the codebase for comments using the old `scene` name for templates. Remove or rewrite them to use `template`.

Known target:

- `src/compiler_frontend/ast/templates/template_head_parser/head_parser.rs`

Also update comments that are now stale, especially comments that imply template folding still needs to be rebuilt on top of render plans when it already uses render plans.

### Comment standard

Keep comments that explain WHAT and WHY.

Delete comments that:

- Restate syntax.
- Preserve old terminology.
- Refer to obsolete implementation history without adding useful context.
- Say `MIGHT`, `EVENTUALLY`, or similar without a concrete TODO owner or reason.

## Step 7 - Remove trivial unused parameters and imports

Examples:

- Remove unused `_string_table` parameters where they are no longer needed.
- Move repeated inline imports to top-level imports when used more than once.
- Keep local imports only when they genuinely improve readability or avoid broad module coupling.

Do not churn unrelated files.

## Step 8 - Add or strengthen tests

Add tests for:

1. No-op formatting does not rebuild content unnecessarily.
2. Invalid formatter anchors return errors instead of panics.
3. `Template::empty()` / replacement constructor creates the same default semantic shape as before.
4. Removal of `explicit_style` does not change `$children`, `$fresh`, markdown, raw, or escape-html behavior.
5. Existing slot/wrapper docs-style table tests still pass.

Prefer behavior assertions over raw AST shape assertions where possible.

## Step 9 - After-change performance benchmark

After all changes and tests pass, rerun the exact same benchmark procedure from the baseline.

Build once:

```bash
cargo build --release --features "detailed_timers"
```

Warm up once without recording:

```bash
cargo run --release --features "detailed_timers" -- check speed-test.bst
cargo run --release --features "detailed_timers" -- check docs
```

Run each at least 5 recorded times:

```bash
cargo run --release --features "detailed_timers" -- check speed-test.bst
cargo run --release --features "detailed_timers" -- check docs
```

Record:

| Benchmark | Before avg | After avg | Change | Notes |
|---|---:|---:|---:|---|
| `check speed-test.bst` | | | | |
| `check docs` | | | | |

Expected result:

- No semantic regressions.
- Equal or slightly improved docs/speed-test timings.
- Fewer unnecessary template content/render-plan rebuilds.

If timings get worse, inspect whether a cleanup accidentally added clone churn or removed a fast path.

## Final validation

Run:

```bash
cargo fmt --check
cargo clippy
cargo test
cargo run tests
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" -- check speed-test.bst
cargo run --release --features "detailed_timers" -- check docs
```

If `just validate` is available and already includes the required checks, run it too:

```bash
just validate
```

## Final style-guide and comments pass

Before committing:

- Confirm no user-input frontend path gained a panic, `todo!`, or unsafe unwrap.
- Confirm removed APIs were not replaced with compatibility shims.
- Confirm comments explain WHAT and WHY.
- Confirm no comments still refer to templates as scenes.
- Confirm all template modules still have clear responsibility boundaries.
- Confirm names are descriptive and not abbreviated.
- Confirm redundant tests were removed or consolidated where they only covered deleted code.
- Confirm diagnostics still include useful locations and suggestions where practical.

## Completion checklist

- [ ] Baseline benchmark averages recorded.
- [ ] No-op formatting change detection fixed.
- [ ] Invalid formatter anchors return diagnostics.
- [ ] `Template::create_default(Vec<Template>)` removed/replaced.
- [ ] `explicit_style` deleted.
- [ ] Obsolete test-only concat helper deleted if unused.
- [ ] Old `scene` comments removed.
- [ ] Tests added/updated.
- [ ] Final benchmark averages recorded.
- [ ] Validation commands pass.
- [ ] Style-guide cleanup pass complete.
