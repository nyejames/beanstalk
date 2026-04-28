# Remaining Choices Cleanup Plan

## Goal

Bring the Choices implementation from “good and working” to “boring and maintainable.”

This is now polish/refactor work, not a major feature branch. The risky HIR/backend issues have mostly been fixed already: total `HirChoiceField.ty`, pre-registered choices, stronger HIR validation, centralized JS variant lowering, shared record-body parsing, and isolated match-capture lowering are all in place.      

---

## Phase 1 — Fix `DataType::Choices` equality

### Problem

`DataType::Choices` equality is still half-nominal, half-structural. It compares nominal path, variant count, variant names, and only the payload discriminant. It does not compare payload field names/types. 

That is the wrong middle ground. Choices are nominal. Equality should be nominal.

### Change

In `src/compiler_frontend/datatypes.rs`, replace the current `DataType::Choices` equality arm with:

```rust
(
    DataType::Choices {
        nominal_path: path_a,
        ..
    },
    DataType::Choices {
        nominal_path: path_b,
        ..
    },
) => path_a == path_b,
```

### Add tests

Add a focused unit test if there is already a datatype test module. Otherwise add an integration regression that ensures same-name choice identity behaves nominally through assignment/import/type checking.

### Commit

```text
types: make choice type equality nominal
```

### Validation

```bash
cargo test
cargo run tests
```

---

## Phase 2 — Remove the remaining panic in match payload parsing

### Problem

`match_patterns.rs` still has:

```rust
.expect("choice payload field must have a name")
```

inside payload capture validation. 

This is likely safe, but it is still frontend-derived state. The style guide prefers structured diagnostics/invariant errors over panics in active frontend paths. 

### Change

Add a helper in `match_patterns.rs`:

```rust
fn choice_payload_field_name(
    field: &Declaration,
    location: &SourceLocation,
) -> Result<StringId, CompilerError> {
    field.id.name().ok_or_else(|| {
        CompilerError::compiler_error(format!(
            "Choice payload field '{}' has no leaf name during match-pattern parsing",
            field.id
        ))
    })
}
```

Use the project’s preferred HIR/frontend compiler-error helper if available there. Keep it as an internal compiler invariant, not a user syntax error.

Replace:

```rust
let expected_field_name = field_decl
    .id
    .name()
    .expect("choice payload field must have a name");
```

with:

```rust
let expected_field_name =
    choice_payload_field_name(field_decl, &capture_location)?;
```

### Commit

```text
parser: remove panic from choice payload capture validation
```

### Validation

```bash
cargo clippy
cargo test
cargo run tests
```

---

## Phase 3 — Rename misleading shorthand detector

### Problem

`starts_choice_payload_type` does not detect a valid choice payload type. It detects the start of invalid shorthand syntax like:

```beanstalk
Response :: Err String, Success;
```

The current name makes the parser read backwards. 

### Change

Rename:

```rust
starts_choice_payload_type
```

to:

```rust
starts_rejected_choice_payload_shorthand
```

or:

```rust
starts_invalid_choice_payload_shorthand
```

I prefer **`starts_rejected_choice_payload_shorthand`** because it makes the parser intent explicit.

Update all call sites in `choice.rs`.

### Commit

```text
parser: clarify rejected choice payload shorthand detection
```

### Validation

```bash
cargo clippy
cargo test
```

---

## Phase 4 — Refresh stale match-pattern comments

### Problem

`match_patterns.rs` still has stale wording:

* “Alpha”
* choice patterns normalize to integer tag indices
* HIR treats choice arms like literal-int arms

That no longer matches the current implementation. Choices now have explicit HIR pattern and variant-carrier semantics. 

### Change

Replace this style of comment:

```rust
/// Resolve a choice variant pattern to its deterministic tag index.
///
/// WHAT: accepts bare (`Ready`) or qualified (`Status::Ready`) variant names and
/// normalizes them to the variant's positional index expression.
/// WHY: match lowering compares integer tag indices, so normalizing here lets HIR
/// treat choice arms identically to literal-int arms.
```

with:

```rust
/// Resolve a choice variant pattern to its deterministic variant index.
///
/// WHAT: accepts bare (`Ready`) or qualified (`Status::Ready`) variant names and
/// resolves them against the scrutinee choice metadata.
/// WHY: later lowering uses the stable variant index in `HirPattern::ChoiceVariant`,
/// while payload captures are materialized separately at arm entry.
```

Also replace:

```rust
// Alpha only supports exact choice-variant names in match patterns.
```

with:

```rust
// Choice patterns support exact variant names plus constructor-like payload captures.
```

Keep deferred-feature wording current:

```text
Capture/tagged patterns using '|...|' are deferred.
```

Do not call them “deferred for Alpha” unless the matrix still uses that phrase intentionally.

### Commit

```text
docs: refresh choice match-pattern implementation comments
```

### Validation

```bash
cargo clippy
```

---

## Phase 5 — Tighten `resolve_choice_id` error shape

### Problem

`resolve_choice_id` reports a missing pre-registered choice as a rule error with `SourceLocation::default()`. But this is an internal AST → HIR contract violation, not a user rule error. 

### Change

Change signature from:

```rust
pub(crate) fn resolve_choice_id(
    &self,
    nominal_path: &InternedPath,
) -> Result<ChoiceId, CompilerError>
```

to:

```rust
pub(crate) fn resolve_choice_id(
    &self,
    nominal_path: &InternedPath,
    location: &SourceLocation,
) -> Result<ChoiceId, CompilerError>
```

Then use `return_hir_transformation_error!` or equivalent:

```rust
let Some(choice_id) = self.choices_by_name.get(nominal_path).copied() else {
    return_hir_transformation_error!(
        format!(
            "Choice '{}' was not pre-registered during HIR declaration preparation",
            self.symbol_name_for_diagnostics(nominal_path)
        ),
        self.hir_error_location(location)
    );
};
```

Update call sites:

* `lower_expression` for `ChoiceConstruct`
* `lower_match_pattern`
* `match_captures.rs`
* any type lowering helper that resolves choices

### Commit

```text
hir: report missing choice registry entries as lowering invariants
```

### Validation

```bash
cargo clippy
cargo test
cargo run tests
```

---

## Phase 6 — Optional: split `match_patterns.rs`

### Problem

`match_patterns.rs` is still broad. It owns literal patterns, relational patterns, choice variant resolution, payload captures, and deferred diagnostics. 

This is not urgent, but pattern matching is still active roadmap work. Splitting now will make wildcard/negated/capture-renaming work easier.

### Change

Create:

```text
src/compiler_frontend/ast/statements/match_patterns/
    mod.rs
    types.rs
    literal.rs
    relational.rs
    choice.rs
    diagnostics.rs
```

Suggested ownership:

```text
types.rs
  MatchArm
  MatchPattern
  ChoicePayloadCapture
  ParsedChoicePayloadCapture
  ParsedChoicePattern
  RelationalPatternOp

literal.rs
  parse_non_choice_pattern
  parse_literal_pattern

relational.rs
  parse_relational_pattern
  ensure_relational_pattern_type

choice.rs
  parse_choice_variant_pattern
  parse_choice_pattern_captures
  parse_variant_name
  resolve_variant_to_tag
  qualifier_resolves_to_choice

diagnostics.rs
  reject_deferred_pattern_lead_token
  shared deferred-pattern diagnostics
```

Keep the public module surface unchanged so callers do not churn much.

### Commit

```text
parser: split match pattern parsing by pattern kind
```

### Validation

```bash
cargo clippy
cargo test
cargo run tests
```

---

## Phase 7 — Test polish

### Add targeted tests

Add these if not already present:

```text
choice_payload_field_default_rejected
choice_payload_mutable_field_rejected
choice_payload_capture_reassignment_rejected
choice_imported_alias_payload_capture_success
choice_direct_field_access_imported_deferred
```

### Add/verify unit tests

For HIR validation, add direct unit tests if the existing HIR test harness makes this easy:

```text
hir_variant_construct_option_invalid_index_rejected
hir_variant_construct_result_invalid_index_rejected
hir_variant_construct_choice_wrong_field_name_rejected
hir_variant_construct_choice_wrong_field_type_rejected
```

These are cheap and valuable because HIR validator bugs are easy to miss with only end-to-end tests.

### Retag diagnostics

The manifest previously had some rejected choice match cases tagged as `language` instead of `diagnostics`. Retag failure cases to make filtered test runs useful.

Recommended:

```toml
tags = ["integration", "diagnostics", "choices", "pattern-matching"]
```

### Commit

```text
tests: tighten remaining choice diagnostics coverage
```

### Validation

```bash
cargo run tests
```

---

# Recommended commit stack

```text
1. types: make choice type equality nominal
2. parser: remove panic from choice payload capture validation
3. parser: clarify rejected choice payload shorthand detection
4. docs: refresh choice match-pattern implementation comments
5. hir: report missing choice registry entries as lowering invariants
6. tests: tighten remaining choice diagnostics coverage
7. parser: split match pattern parsing by pattern kind
```

I would do commit 7 last or skip it for now. It is the only one with meaningful file churn.

---

# Best order

Do this first:

```text
1. DataType::Choices equality
2. remove expect(...)
3. resolve_choice_id(location)
```

Those are correctness/style fixes.

Then do:

```text
4. rename starts_choice_payload_type
5. comment sweep
6. tests
```

Then only split `match_patterns.rs` when you are about to continue Pattern Matching work. It is a good refactor, but not necessary to stabilize Choices.