# Beanstalk Choice / Tagged-Union Cleanup Plan

## Scope

This cleanup focuses on the areas touched by the completed Choices/tagged-union implementation:

- Choice declaration parsing
- Choice constructor parsing
- Shared call-argument validation
- Choice payload matching
- AST match-arm scope construction
- HIR variant carrier construction
- HIR choice metadata
- HIR match lowering
- HIR validation
- JavaScript backend variant lowering
- Test matrix and implementation matrix alignment

The implementation matrix currently marks **Choices** as supported with broad parser, declaration, constructor, match, import, assignment, return, JS carrier-shape, and payload coverage. It still marks **Pattern matching** as partial because general captures, capture renames, nested payload patterns, wildcard `case _`, and negated patterns remain deferred.

## Executive Priority Order

| Priority | Area | Action |
|---:|---|---|
| P0 | HIR choice metadata | Remove `Option<TypeId>`, remove silent `.ok()` lowering, remove `String` fallback for unresolved payload capture types. |
| P0 | HIR validation | Validate Option/Result variant indexes and full Choice field names/types. |
| P1 | Match capture lowering | Extract guard substitution, capture local registration, and capture assignment into a dedicated module/context. |
| P1 | Declaration parsing | Make record-body parsing owner-path-safe; stop mutating `token_stream.src_path` manually. |
| P1 | JS lowering | Emit variant fields with escaped string keys/bracket access. |
| P2 | Comments/dead code | Remove stale “Phase N”, “unit-only”, and “not walked in Alpha” comments. |
| P2 | Tests | Retag diagnostics, audit duplicate JS-shape cases, add field-default/mutable-field/imported-alias/guard-prelude coverage. |

---

# Findings and Refactor Tasks

## 1. HIR choice metadata is still transitional

### Problem

`HirChoiceField` currently stores an optional type:

```rust
pub ty: Option<TypeId>
```

This exists because choice metadata can be lazily registered before all field types are available. `lower_choice_variants` also silently swallows failed type lowering with `.ok()`, and `lower_capture_field_type` falls back to `String` when a `NamedType` cannot be resolved.

That violates the intended compiler pipeline: AST should resolve and validate types before HIR. HIR should not guess.

### Fix

Make HIR choice metadata total:

```rust
pub struct HirChoiceField {
    pub name: StringId,
    pub ty: TypeId,
}
```

Remove:

- `Option<TypeId>`
- `.ok()` around `lower_data_type`
- `DataType::NamedType(_) => None`
- fallback-to-`String`
- fake `"<anonymous>"` field names

If a `NamedType` reaches HIR, return a HIR transformation error. That state means AST type resolution failed to uphold its contract.

---

## 2. Lazy choice registration/backfill is design drift

### Problem

`resolve_or_create_choice_id` lazily creates `HirChoice` entries and backfills payload metadata if the choice was first discovered through a type reference.

This was useful during incremental implementation, but it is now a transitional layer. It also caused `HirChoiceField.ty` to become optional.

### Fix

Add a HIR builder initialization step:

```rust
register_choice_declarations_from_ast_headers()
```

Target flow:

1. AST resolves all choice payload field types.
2. HIR builder registers every choice once.
3. `choices_by_name` is complete before expression/statement lowering.
4. `lower_data_type(DataType::Choices)` only resolves an existing `ChoiceId`.
5. If missing, emit a HIR transformation error.

Then replace:

```rust
resolve_or_create_choice_id(...)
```

with:

```rust
resolve_choice_id(...)
```

and remove all backfill logic.

---

## 3. HIR validation accepts invalid Option/Result variant indexes

### Problem

Option validation checks field count using:

```rust
let expected = if variant_index == 0 { 0 } else { 1 };
```

So `variant_index = 99` with one field can validate as `Some`.

Result validation checks only that there is one field, not that the variant index is `0` or `1`.

### Fix

Add carrier-specific validation:

```rust
fn validate_variant_index_for_carrier(
    carrier: &HirVariantCarrier,
    variant_index: usize,
) -> Result<(), CompilerError>
```

Rules:

```text
Option: 0 = none, 1 = some
Result: 0 = ok, 1 = err
Choice: index < choice.variants.len()
```

Then validate field count after index validation.

---

## 4. HIR validation does not fully validate Choice payload fields

### Problem

For `VariantConstruct`, Choice validation checks:

- `ChoiceId` exists
- variant index is in range
- field count matches

It does not validate:

- field names match declared payload field names
- field types match declared payload field types

### Fix

After making `HirChoiceField.ty: TypeId`, validate:

```rust
for (actual, expected) in fields.iter().zip(variant.fields.iter()) {
    require(actual.name == Some(expected.name));
    require(actual.value.ty == expected.ty);
}
```

For unit variants, require `fields.is_empty()`.

---

## 5. Match capture lowering is too entangled with CFG lowering

### Problem

`hir_statement/control_flow.rs` now owns too many concerns:

- generic control-flow lowering
- match lowering
- capture local allocation
- temporary `locals_by_name` restoration
- guard capture substitution
- payload extraction assignments
- recursive HIR expression substitution

The logic is working, but the file is now managing multiple concepts.

### Fix

Create one of these layouts:

```text
src/compiler_frontend/hir/hir_statement/match_lowering.rs
src/compiler_frontend/hir/hir_statement/match_captures.rs
```

or:

```text
src/compiler_frontend/hir/match_lowering/
    mod.rs
    captures.rs
    guards.rs
    patterns.rs
```

Introduce a small context:

```rust
struct MatchCaptureLoweringContext<'a> {
    scrutinee_ast: &'a Expression,
    scrutinee_hir: HirExpression,
    choice_id: ChoiceId,
    parent_region: RegionId,
}
```

Move these operations behind named helpers:

```rust
register_arm_capture_locals(...)
build_guard_capture_substitutions(...)
emit_arm_capture_assignments(...)
with_arm_capture_bindings(...)
```

Add a scoped binding helper to `HirBuilder`:

```rust
fn with_temporary_local_bindings<T>(
    &mut self,
    bindings: impl IntoIterator<Item = (InternedPath, LocalId)>,
    f: impl FnOnce(&mut Self) -> Result<T, CompilerError>,
) -> Result<T, CompilerError>
```

This removes manual remove/insert restoration logic.

---

## 6. The HIR expression substitution walker should be centralized

### Problem

`substitute_locals_in_expression` recursively matches every `HirExpressionKind`. Every future HIR expression variant now requires updating this walker.

### Fix

Add:

```text
src/compiler_frontend/hir/expression_rewrite.rs
```

With a reusable traversal API:

```rust
pub fn rewrite_expression_bottom_up(
    expr: &HirExpression,
    rewrite: &mut impl FnMut(&HirExpression) -> Option<HirExpression>,
) -> HirExpression
```

Then guard capture substitution becomes a small use of a general API.

---

## 7. `MatchPattern` uses a partially-initialized capture type

### Problem

`ChoicePayloadCapture` has:

```rust
pub binding_path: Option<InternedPath>
```

It starts as `None`, then `branching.rs` mutates it after building the arm scope. HIR lowering later errors if it is still missing.

This is staged-construction drift. A typed AST node should not carry an optional field that is mandatory by HIR.

### Fix

Split the types:

```rust
struct ParsedChoicePayloadCapture {
    field_name: StringId,
    field_index: usize,
    field_type: DataType,
    location: SourceLocation,
}

struct ChoicePayloadCapture {
    field_name: StringId,
    field_index: usize,
    field_type: DataType,
    binding_path: InternedPath,
    location: SourceLocation,
}
```

`parse_choice_variant_pattern` returns parsed captures. `build_arm_scope_with_captures` consumes them and returns a fully resolved `MatchPattern`.

---

## 8. `match_patterns.rs` should be split

### Problem

`match_patterns.rs` now owns:

- literal patterns
- relational patterns
- choice variant name resolution
- choice payload capture parsing
- deferred syntax diagnostics
- capture validation

It is readable, but it is drifting into a broad grammar/semantics file.

### Fix

Create:

```text
src/compiler_frontend/ast/statements/match_patterns/
    mod.rs
    literal.rs
    relational.rs
    choice.rs
    diagnostics.rs
    types.rs
```

Keep `mod.rs` as the structural map and re-export layer.

---

## 9. Direct `token_stream.src_path` mutation is brittle

### Problem

`parse_choice_shell` temporarily sets:

```rust
token_stream.src_path = choice_path.to_owned();
let fields = parse_record_body(...)?;
token_stream.src_path = saved_src_path;
```

If parsing errors, restore is skipped. The parse aborts, so this may not corrupt later parsing, but it is still brittle and hides a dependency between field names and `FileTokens.src_path`.

### Fix

Preferred:

```rust
parse_record_body(
    token_stream,
    owner_path,
    context,
    string_table,
    member_context,
)
```

Then `parse_signature_members` should build field paths from `owner_path`, not from `token_stream.src_path`.

Acceptable alternative:

```rust
token_stream.with_src_path(choice_path, |token_stream| {
    parse_record_body(...)
})
```

This must restore on success and error.

---

## 10. `parse_record_body` belongs in its own module

### Problem

`parse_record_body` is shared by structs and choice payloads, but it lives in `declaration_syntax/struct.rs`.

### Fix

Move to:

```text
src/compiler_frontend/declaration_syntax/record_body.rs
```

Exports:

```rust
parse_record_body(...)
validate_record_default_values(...)
```

Then `struct.rs` becomes a thin struct-specific wrapper.

---

## 11. Choice payload fields may accidentally support defaults and mutability

### Problem

`SignatureMemberContext::ChoicePayloadField` shares the same syntax as struct fields:

- name
- optional `~`
- type
- optional `= default`

The implementation matrix says variant default values are deferred, but it does not clearly say whether **payload field defaults** are supported or rejected.

This is likely accidental surface expansion.

### Recommendation

Reject payload field defaults and mutability for Alpha.

Reject:

```beanstalk
Response ::
    Err |
        message String = "bad",
    |,
;
```

and:

```beanstalk
Response ::
    Pending |
        retry_count ~Int,
    |,
;
```

Add diagnostics:

- “Choice payload field defaults are deferred.”
- “Choice payload field mutability is not supported; mutability belongs to bindings, not variant payload declarations.”

If you decide to support them instead, update:

- `docs/src/docs/progress/#page.bst`
- `docs/language-overview.md`
- tests for constructor defaults and mutable payload behavior

Do not leave this implicit.

---

## 12. `DataType::Choices` equality is inconsistent

### Problem

`PartialEq` for `DataType::Choices` compares:

- nominal path
- variant count
- variant names
- only payload discriminant

It does not compare payload field names or types.

### Fix

Because choices are nominal, simplify equality:

```rust
DataType::Choices { nominal_path: a, .. },
DataType::Choices { nominal_path: b, .. } => a == b
```

Then enforce metadata consistency elsewhere:

- AST declaration resolution
- HIR choice registry validation
- tests for duplicate/conflicting declarations

Do not half-compare structure.

---

## 13. `DataType::Choices` carries too much declaration payload

### Problem

`DataType::Choices` stores `Vec<ChoiceVariant>`, and each payload variant stores `Vec<Declaration>`. That means type values carry parsed declaration structures and expression defaults through many passes.

### Longer-term Fix

Introduce a frontend nominal type registry:

```rust
ChoiceDeclId
StructDeclId
TypeDeclRegistry
```

Then:

```rust
DataType::Choices { nominal_path, choice_id }
```

or:

```rust
DataType::Choice(ChoiceDeclId)
```

Do this after the P0 HIR correctness work.

---

## 14. JS variant object emission should escape field names

### Problem

JS lowering emits object fields directly:

```js
{ tag: 1, message: value }
```

and payload access as:

```js
source.message
```

This assumes every Beanstalk field name is a safe JS identifier and not a reserved word.

### Fix

Use string keys consistently:

```js
{ tag: 1, "message": value }
source["message"]
```

Add helper:

```rust
fn js_object_property_key(name: &str) -> String {
    escape_js_string(name)
}
```

Lower payload get with bracket access:

```rust
format!("({source_js})[{}]", escape_js_string(field_name))
```

---

## 15. JS variant lowering should be extracted

### Problem

`lower_expr` contains inline logic for all variant carriers. This will grow as variant semantics expand.

### Fix

Add:

```rust
fn lower_variant_construct(
    &mut self,
    carrier: &HirVariantCarrier,
    variant_index: usize,
    fields: &[HirVariantField],
) -> Result<String, CompilerError>

fn lower_variant_payload_get(...)
```

Centralize tag policy:

```rust
fn js_variant_tag(
    carrier: &HirVariantCarrier,
    variant_index: usize,
) -> Result<String, CompilerError>
```

Policy:

```text
Choice: numeric tag
Option: "none" / "some"
Result: "ok" / "err"
```

---

## 16. Stale comments and `#[allow(dead_code)]` are misleading

### Problem

Some comments still describe implementation phases or old Alpha state:

- “Alpha scope supports unit variants only”
- “payload fields are intentionally omitted”
- “Phase 3”
- “Phase 6”
- “not walked in Alpha validation”
- “OptionVariant was removed during Phase 6”

These comments now create false context.

### Fix

Sweep touched files for:

```text
Phase
Alpha unit
deferred for Alpha
not walked
not read
unit-only
```

Replace with durable current comments.

Example:

```rust
/// Registry entry for a nominal choice type.
///
/// WHY: choices are nominal variant carriers. The registry gives HIR and
/// backends stable variant indexes and payload field metadata.
```

Remove obsolete `#[allow(dead_code)]` where fields are now used.

---

## 17. Some internal panics should become compiler invariant errors

### Problem

`match_patterns.rs` uses:

```rust
expect("choice payload field must have a name")
```

Payload fields should have names, but this is still safer as a structured compiler invariant error.

### Fix

Replace with:

```rust
fn choice_payload_field_name(field: &Declaration) -> Result<StringId, CompilerError>
```

Return `return_compiler_error!` if missing.

---

# Test Cleanup Plan

## Current State

The manifest shows broad choice coverage:

- payload declaration
- shorthand rejection
- constructors
- const values
- payload matching
- JS carrier shape
- imported constructors
- recursive/generic/default/deferred surfaces
- adversarial control flow

The remaining issues are edge coverage, duplicate cleanup, and tag clarity.

## Missing or unclear coverage

Add or verify these cases:

| Area | Test to add |
|---|---|
| Payload field defaults | `choice_payload_field_default_rejected` or success tests if intentionally supported |
| Mutable payload fields | `choice_payload_mutable_field_rejected` or documented support tests |
| Imported type alias payload field | Constructor + match capture of aliased imported type |
| Guard purity | Payload capture guard that would lower with prelude; must be supported deliberately or rejected clearly |
| Capture immutability | `choice_payload_capture_reassignment_rejected` |
| Option/Result HIR validation | Unit tests for invalid `VariantConstruct` indexes |
| Choice field type validation | HIR validation unit test with mismatched payload field type |
| JS reserved field names | Only needed if frontend allows names that collide with JS properties/keywords |
| Direct payload field access on imported choice | Ensures diagnostic is not same-file-only |
| Payload constructor defaults | Required if payload defaults are kept |

## Retag diagnostics

Some rejected match cases are tagged as `language` rather than `diagnostics`.

Retag cases such as:

- wrong capture name
- too few captures
- too many captures
- duplicate capture
- rename syntax
- missing payload
- unit parens

Recommended tags:

```toml
tags = ["integration", "diagnostics", "choices", "pattern-matching"]
```

## Audit duplicate backend-shape coverage

These cases may overlap:

- `choice_payload_js_carrier_shape`
- `js_choice_payload_carrier_shape`
- `choice_payload_return_js_shape`
- `choice_payload_assignment_js_shape`
- `js_choice_payload_match_shape`

Keep:

- one end-to-end behavior case
- one backend carrier-shape contract case
- one match-shape contract case

Delete or merge tests that assert the same emitted fragments.

---

# Implementation Phases

## Phase 1 — HIR variant metadata correctness

### Goal

Make HIR variant metadata fully resolved and validator-enforced.

### Changes

1. Change `HirChoiceField.ty` from `Option<TypeId>` to `TypeId`.
2. Remove silent `.ok()` in `lower_choice_variants`.
3. Remove `String` fallback from `lower_capture_field_type`.
4. Replace missing field-name fallback with compiler invariant error.
5. Add Option/Result variant index validation.
6. Validate Choice payload field names and types in `VariantConstruct`.

### Suggested commits

```text
hir: make choice payload metadata total
hir: validate variant carrier indexes and payload fields
```

### Validation

```bash
cargo clippy
cargo test
cargo run tests
```

---

## Phase 2 — Replace lazy choice registration

### Goal

Stop incomplete choice registration and backfilling.

### Changes

1. Add a HIR pre-registration step for choices after AST top-level type resolution.
2. Make `resolve_or_create_choice_id` into `resolve_choice_id`.
3. Make missing choice ID an internal HIR error.
4. Remove `needs_backfill` logic.

### Suggested commit

```text
hir: pre-register choice declarations before expression lowering
```

### Risk

This touches HIR builder initialization. Keep the commit narrow and avoid changing JS/backend behavior.

---

## Phase 3 — Extract match capture lowering

### Goal

Move capture mechanics out of generic control-flow lowering.

### New module

```text
src/compiler_frontend/hir/hir_statement/match_captures.rs
```

or:

```text
src/compiler_frontend/hir/match_lowering/captures.rs
```

### Move

- capture local registration
- guard substitution map construction
- capture assignment emission
- temporary binding restoration

### Also add

```rust
HirBuilder::with_temporary_local_bindings(...)
```

### Suggested commit

```text
hir: extract match payload capture lowering
```

---

## Phase 4 — Add generic HIR expression rewrite utility

### Goal

Avoid hand-maintained recursive rewrites inside match lowering.

### New file

```text
src/compiler_frontend/hir/expression_rewrite.rs
```

### Use it for

- guard capture substitution

### Suggested commit

```text
hir: add shared expression rewrite helper
```

---

## Phase 5 — Clean AST match pattern types

### Goal

Remove `Option<InternedPath>` staged AST state.

### Changes

1. Split parsed/resolved capture structs.
2. Make final `ChoicePayloadCapture.binding_path` non-optional.
3. Move capture-scope construction to a dedicated helper module.

### New file

```text
src/compiler_frontend/ast/statements/match_captures.rs
```

### Suggested commit

```text
ast: resolve choice payload capture bindings during match arm construction
```

---

## Phase 6 — Clean declaration syntax ownership

### Goal

Make shared record-body parsing explicit and safe.

### Changes

1. Move `parse_record_body` to `declaration_syntax/record_body.rs`.
2. Stop manually mutating `token_stream.src_path`.
3. Decide and enforce payload field defaults/mutability.
4. Rename `starts_choice_payload_type` to:

```rust
starts_rejected_choice_payload_shorthand
```

### Suggested commits

```text
parser: move shared record body parsing out of struct syntax
parser: harden choice payload declaration syntax
```

---

## Phase 7 — JS variant lowering cleanup

### Goal

Centralize variant JS shape and make field access safe.

### Changes

1. Add `lower_variant_construct`.
2. Add `lower_variant_payload_get`.
3. Emit string-keyed properties.
4. Use bracket access for payload gets.
5. Update backend-contract goldens.

### Suggested commit

```text
js: centralize variant carrier lowering
```

---

## Phase 8 — Comment/dead-code/docs sweep

### Goal

Remove stale phase language and old unit-only comments.

### Changes

1. Sweep touched files for stale comments.
2. Remove obsolete `#[allow(dead_code)]`.
3. Update module docs after file splits.
4. Update progress matrix if payload field defaults/mutability decision changes.

### Suggested commit

```text
docs: refresh choice implementation comments and matrix notes
```

---

## Phase 9 — Test cleanup and edge coverage

### Goal

Make tests match the supported/deferred surface precisely.

### Changes

1. Retag diagnostic tests.
2. Remove duplicate backend shape cases.
3. Add missing tests listed above.
4. Add HIR validation unit tests for invalid carrier metadata.
5. Ensure failure cases assert message fragments, not only failure mode.

### Suggested commit

```text
tests: tighten choice payload diagnostics and HIR carrier coverage
```

---

# Recommended Starting Point

Start with **Phase 1**.

The biggest risk is silent wrong HIR, not LOC.

The two lines of code most worth eliminating first are:

- `.ok()` while lowering choice payload field types
- fallback to `String` for unresolved capture field types

Those are the places where the compiler can become confidently wrong. Everything else is cleanup or maintainability.

## Final Validation

Run:

```bash
just validate
```

or at minimum:

```bash
cargo clippy
cargo test
cargo run tests
```
