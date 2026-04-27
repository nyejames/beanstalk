# Template Parser Unification Plan - Pass 2

## Goal

This pass is the deeper structural cleanup for AST template parsing, slot composition, directive parsing, and render-plan/content lifecycle management.

It should happen only after `template-parser-cleanup-pass-1.md` is complete.

The focus is:

- Centralize template directive argument parsing.
- Split slot schema, contributions, and composition into clearer units.
- Reduce clone-heavy slot/render-plan paths.
- Add slot-schema caching or equivalent metadata if benchmarks justify it.
- Tighten the relationship between `TemplateContent`, `TemplateRenderPlan`, and template metadata.
- Make the template pipeline converge on fewer authoritative states.

This pass may touch more files than Pass 1, but it must remain a refactor. Template semantics should not change.

## Prerequisites

Before starting:

- Pass 1 is merged.
- `explicit_style` is gone.
- `Template::create_default(Vec<Template>)` is gone.
- No comments still refer to templates as `scene`.
- No-op formatting change detection is fixed.
- Invalid formatter anchors return diagnostics, not panics.
- The test suite is green.

## Non-goals

- No new template syntax.
- No new formatter behavior.
- No changes to top-level fragment semantics.
- No HIR changes unless a compile error exposes a stale assumption.
- No partial compatibility layer for old template APIs.

## Baseline performance benchmark

Before making code changes, collect a fresh baseline after Pass 1.

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

Also record any detailed timer sections related to:

- Tokenization.
- AST construction.
- Template parsing/composition/folding, if exposed.
- Docs build total.

If timing variance is high, run 10 measured iterations and compare median as well as average.

## Step 1 - Centralize directive argument parsing

### Problem

Template-head directive argument parsing is currently repeated across:

- `$children(...)` parsing.
- Handler-based style directive parsing.
- `$slot(...)` parsing.
- `$insert(...)` parsing.

This creates drift risk and makes directive syntax harder to maintain.

### Implementation

Create:

```text
src/compiler_frontend/ast/templates/template_head_parser/directive_args.rs
```

Move or create helpers for:

```rust
parse_optional_parenthesized_expression(...)
parse_required_parenthesized_expression(...)
parse_required_single_compile_time_expression(...)
parse_required_string_literal_argument(...)
parse_optional_slot_key_argument(...)
parse_required_named_slot_key_argument(...)
expect_close_parenthesis_after_directive_argument(...)
reject_unexpected_directive_arguments(...)
```

The exact function names can differ, but the ownership must be clear:

- Token-level directive argument syntax belongs in `directive_args.rs`.
- Directive-specific semantic validation stays in the directive module.
- Slot composition should not parse token streams.

### Move from `template_slots.rs`

Move these out of slot composition:

- `parse_slot_definition_target_argument`
- `parse_required_named_slot_insert_argument`

After the move, `template_slots.rs` should own slot schema/contribution/composition behavior only.

### Tests

Add unit tests for directive arg helpers covering:

- Missing parentheses.
- Empty parentheses.
- Extra comma / extra argument.
- Missing closing parenthesis.
- Wrong argument kind.
- Positive positional slot.
- Zero/negative positional slot rejection.
- Named slot/insert string literal parsing.

Keep higher-level template tests for behavior, but avoid duplicating every parser-helper case at integration level.

## Step 2 - Split slot schema, contribution bucketing, and composition

### Problem

`template_slots.rs` currently mixes:

- Slot schema discovery.
- Slot insert extraction.
- Loose contribution bucketing.
- Wrapper atom recursion.
- Child wrapper application during slot expansion.
- Token-level `$slot` / `$insert` argument parsing.
- Slot diagnostics.

This makes the file harder to modify without breaking behavior.

### Implementation

Convert `template_slots.rs` into a submodule directory:

```text
src/compiler_frontend/ast/templates/template_slots/
    mod.rs
    schema.rs
    contributions.rs
    composition.rs
    diagnostics.rs
```

Suggested ownership:

- `schema.rs`
  - `SlotSchema`
  - `collect_slot_schema`
  - duplicate default slot validation
  - target acceptance checks

- `contributions.rs`
  - `SlotContributions`
  - `SlotInsertContribution`
  - loose contribution grouping
  - direct `$insert` extraction from fill content

- `composition.rs`
  - `compose_template_with_slots`
  - recursive wrapper atom composition
  - child-wrapper expansion for slot contributions

- `diagnostics.rs`
  - unknown slot target errors
  - loose content without default slot errors
  - overflow errors

- `mod.rs`
  - public surface and module-level WHAT/WHY docs
  - re-exports only what other template modules need

### Constraints

- Preserve current public visibility as narrowly as possible.
- Do not expose internal slot structs outside the template subsystem unless needed by tests.
- Keep the `mod.rs` small and explanatory.
- Avoid moving code without also tightening names and comments.

## Step 3 - Reduce clone-heavy contribution handling

### Problem

Slot composition clones vectors of atoms in several places. Some cloning is necessary because repeated slots replay the same contribution, but the current code clones earlier than needed.

### Implementation

Change `SlotContributions::atoms_for_slot` from returning an owned `Vec<TemplateAtom>` to returning borrowed atoms where possible.

Candidate shape:

```rust
fn atoms_for_slot(&self, key: &SlotKey) -> &[TemplateAtom]
```

or, if default empty slices are awkward:

```rust
fn atoms_for_slot<'a>(&'a self, key: &SlotKey) -> impl Iterator<Item = &'a TemplateAtom>
```

Then clone only at the final expansion boundary where owned atoms are required.

### Tests

Existing slot behavior tests should continue passing.

Add or preserve tests for:

- Repeated named slots replay the same content.
- Repeated positional slots replay the same content.
- Loose contributions still route to positional slots first.
- Default slot still receives overflow content.

## Step 4 - Pre-scan before head/body splitting in head-chain composition

### Problem

`compose_template_head_chain` splits content into head and body vectors before it knows whether composition is needed.

### Implementation

Add a cheap pre-scan:

```rust
let has_head_atoms = content.atoms.iter().any(is_head_content_atom);
if !has_head_atoms {
    return Ok(content.to_owned());
}
```

Then split only when head atoms exist.

Also consider scanning specifically for receiver-capable head templates. If there are head atoms but none can open a receiving layer, the function may still be able to return a cheaper content clone.

### Constraint

Do not sacrifice readability for a tiny micro-optimization. Keep the early exits obvious.

## Step 5 - Introduce explicit template metadata lifecycle

### Problem

The current lifecycle has several loosely connected fields and methods:

- `content`
- `unformatted_content`
- `content_needs_formatting`
- `render_plan`
- `kind`
- `refresh_kind_from_content`
- `resync_runtime_metadata`
- `resync_composition_metadata`

This works, but it lets too many phases decide which representation is authoritative.

### Implementation option A - minimal convergence

Keep the current `Template` fields, but add a small finalization helper:

```rust
struct TemplateFinalizationResult {
    content: TemplateContent,
    unformatted_content: TemplateContent,
    render_plan: TemplateRenderPlan,
    kind: TemplateType,
}
```

Then make `Template::new_nested_template` assign finalized fields once at the end.

### Implementation option B - deeper convergence

Introduce:

```rust
pub struct TemplateMetadata {
    pub render_plan: Option<TemplateRenderPlan>,
    pub slot_schema: Option<SlotSchema>,
}
```

and keep metadata updates behind explicit methods:

```rust
fn invalidate_composition_metadata(&mut self)
fn finalize_after_content_change(&mut self, policy: RenderPlanPolicy)
fn ensure_render_plan(&mut self)
fn ensure_slot_schema(&mut self)
```

Prefer option A if option B starts spreading too far.

### Required invariant comments

Add concise comments documenting:

- When `content` is authoritative.
- When `render_plan` is authoritative.
- Why `unformatted_content` exists.
- Why HIR expects runtime templates to already carry render plans.
- Why formatters see body-origin text and opaque anchors, not nested child bytes.

## Step 6 - Add slot-schema caching only if justified

### Problem

Slot schema collection recursively walks wrapper content each time composition runs. This may be hot for docs tables and repeated wrapper use.

### Implementation

After Step 5 clarifies metadata invalidation, add lazy slot-schema caching if benchmarks or code inspection justify it.

Candidate rules:

- Slot schema is computed from finalized wrapper content.
- Any mutation of `Template.content` invalidates the cached schema.
- Composition can use cached schema only when the wrapper is not being mutated in-place.

Possible API:

```rust
fn slot_schema(&mut self) -> Result<&SlotSchema, CompilerError>
```

or, if mutation through `&mut Template` is awkward:

```rust
fn collect_or_reuse_slot_schema(wrapper: &Template, cache: &mut TemplateCompositionCache) -> Result<SlotSchema, CompilerError>
```

Use the second shape if it avoids complicating `Template` ownership.

### Constraint

Do not add caching if it makes invalidation unclear. A correct linear walk is better than a subtle stale-schema bug.

## Step 7 - Unify content/render-plan rebuild paths

### Problem

`TemplateRenderPlan::from_content` and `TemplateRenderPlan::rebuild_content` convert back and forth. Some information, such as `source_child_template`, is currently not preserved on rebuild.

### Implementation

Review and tighten the conversion boundary:

1. Decide whether `source_child_template` is intentionally pre-format only.
2. If yes, document it at the rebuild site.
3. If no, preserve enough metadata in `RenderChildPiece` to rebuild it correctly.
4. Ensure `rebuild_content` is only used when rebuilding is semantically required.
5. Keep child templates opaque through formatter passes.

Consider adding a helper:

```rust
fn rebuild_content_from_final_plan(plan: &TemplateRenderPlan) -> TemplateContent
```

with comments explaining what information is intentionally not restored.

### Tests

Add tests for:

- Child template output remains identifiable after a formatting/rebuild path if later composition needs it.
- Formatter rebuild does not accidentally expose child template bytes to parent formatters.
- Markdown/table wrappers still preserve row/cell counts.

## Step 8 - Optional: composition cache for a single template parse

If repeated resolution remains expensive after the above changes, add a parse-local cache object.

Candidate:

```rust
struct TemplateCompositionContext<'a> {
    string_table: &'a StringTable,
    resolved_layers: FxHashMap<usize, Rc<Template>>,
    slot_schemas: FxHashMap<TemplateCacheKey, SlotSchema>,
}
```

Use this only inside one parse/composition call chain. Do not introduce long-lived global caching.

The existing head-chain layer cache is a good precedent. Preserve that locality.

## Step 9 - Test suite cleanup and additions

### Add tests

Add or strengthen tests for:

1. Directive arg parser helper coverage.
2. Slot schema duplicate default validation.
3. Slot schema accepts named/positional/default targets correctly.
4. Contribution bucketing with interleaved whitespace and child templates.
5. Repeated slots replay contributions without consuming them.
6. Formatter rebuild path preserves required template structure.
7. Deep wrapper composition remains bounded for many rows.
8. Unknown named insert target still points at the insert helper location.
9. Runtime wrapper slot filling still works.
10. Top-level const fragment + runtime fragment behavior remains unchanged.

### Consolidate redundant tests

The table-cell nested inline template behavior is currently covered in more than one place. Keep one low-level unit test and one higher-level behavior/integration test if both add value. Remove exact duplicates.

### Prefer integration where behavior is user-visible

For stable template behavior, prefer `tests/cases` fixture coverage with rendered output assertions where practical.

Unit tests are still appropriate for:

- Slot schema internals.
- Contribution bucketing.
- Directive arg parser errors.
- Render-plan reconstruction invariants.

## Step 10 - After-change performance benchmark

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

Also compare AST/template-specific timer sections where available.

Expected result:

- Similar or better total compile time.
- Reduced template-stage clone/rebuild overhead.
- No structural growth regressions in docs-style tables/wrappers.

If timings regress:

1. Check whether slot-schema caching added clone or invalidation overhead.
2. Check whether directive arg centralization added extra expression parsing.
3. Check whether render-plan/content unification rebuilt plans more often.
4. Use `speed-test.bst` and docs as separate signals. A docs regression is more important for real template use.

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

- Confirm each template module has one clear responsibility.
- Confirm `mod.rs` files explain the module structure and data flow.
- Confirm no old template APIs remain as compatibility shims.
- Confirm no comments use old terminology such as `scene`.
- Confirm comments explain WHAT and WHY rather than restating code.
- Confirm every new file has a top-level doc comment.
- Confirm no frontend path gained a user-input panic.
- Confirm diagnostics remain structured and point at useful source locations.
- Confirm tests are not bloated by duplicated coverage.
- Confirm benchmark averages are recorded in the PR/commit notes.

## Completion checklist

- [ ] Pass 1 completed and green.
- [ ] Baseline benchmark averages recorded.
- [ ] Directive argument parsing centralized.
- [ ] Slot parsing removed from slot composition module.
- [ ] Slot schema/contribution/composition split completed.
- [ ] Clone-heavy contribution handling reduced.
- [ ] Head-chain composition pre-scan added.
- [ ] Template metadata lifecycle tightened.
- [ ] Slot-schema caching added only if justified and safe.
- [ ] Render-plan/content rebuild boundary documented or improved.
- [ ] Tests added and redundant tests consolidated.
- [ ] Final benchmark averages recorded.
- [ ] Validation commands pass.
- [ ] Style-guide cleanup pass complete.
