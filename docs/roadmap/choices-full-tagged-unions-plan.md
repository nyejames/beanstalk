# Beanstalk Choices: Full Tagged Union Implementation Plan

## Purpose

This plan turns Choices from the current unit-variant enum subset into full non-generic tagged unions for the Alpha surface.

The design is intentionally narrower than "everything tagged unions could eventually do":

- Unit variants are values.
- Payload variants must use record-body syntax.
- Choice constructors use the same positional/named call rules as struct constructors.
- Match payload extraction uses constructor-like patterns.
- Options and Results should share the same core HIR representation for variant construction/matching, while retaining distinct semantic type kinds for diagnostics and optimization.
- Direct payload field access, recursive choices, generic choices, payload structural equality, nested exhaustiveness, and binding renames are deliberately deferred.

The implementation should be staged so each phase can land as a stable commit and then be followed by an audit/style/validation commit.

---

## Current repo anchors

The relevant current implementation is already fairly cleanly isolated:

- `src/compiler_frontend/declaration_syntax/choice.rs`
  - Parses choice declaration shells.
  - Currently accepts only unit variants.
  - Emits deferred diagnostics for payloads, `| ... |` variant bodies, defaults, and constructor-style declaration syntax.
- `src/compiler_frontend/datatypes.rs`
  - `DataType::Choices { nominal_path, variants }` carries frontend choice identity.
  - `ChoiceVariant` currently carries `id`, `data_type`, and `location`.
- `src/compiler_frontend/ast/expressions/parse_expression_identifiers.rs`
  - Parses `Choice::Variant` value expressions.
  - Rejects `Choice::Variant(...)` as deferred.
  - Enforces qualified construction outside match arms.
- `src/compiler_frontend/ast/statements/match_patterns.rs`
  - Parses literal, relational, and choice variant patterns.
  - Allows bare or qualified variant names in choice match arms.
  - Currently rejects capture/tagged patterns.
- `src/compiler_frontend/ast/expressions/expression.rs`
  - Has `ExpressionKind::ChoiceVariant`.
  - Treats unit choice values as foldable and compile-time literal-like values.
- `src/compiler_frontend/hir/expressions.rs`
  - Has `HirExpressionKind::ChoiceVariant`.
  - Also has separate `OptionConstruct` and `ResultConstruct`.
- `src/compiler_frontend/hir/patterns.rs`
  - Has `HirPattern::ChoiceVariant`.
- `src/compiler_frontend/hir/module.rs`
  - `HirChoiceVariant` stores only the variant name today.
- `src/backends/js/js_expr.rs`
  - Lowers choice variants to bare integer tags.
  - Options and Results already lower to object carriers.
- `src/backends/js/js_statement.rs`
  - Lowers choice match arms as integer comparisons.
- `docs/src/docs/progress/#page.bst`
  - Currently marks Choices as Partial.
  - Currently lists choice payload declarations, tagged/default declarations, and constructor use as deferred.
- `docs/src/docs/choices/#page.bst`
  - Currently documents unit variants as the Alpha-supported subset and payload/constructor forms as deferred.
- `docs/src/docs/generics.md`
  - Contains draft generic choice syntax, but this should remain future-facing.

The important point: do not add a parallel "new choice system". Extend the existing choice declaration, expression, pattern, HIR, and backend surfaces.

---

## Locked language decisions

### Valid declaration syntax

Unit variants:

```beanstalk
Response ::
    Success,
    Cancelled,
;
```

Payload variants:

```beanstalk
Response ::
    Err |
        message String,
    |,
    Pending |
        retry_count Int,
        message String,
    |,
    Success,
;
```

Invalid shorthand:

```beanstalk
Response :: Err String, Success;
```

This must be rejected with a clear diagnostic. It is not a future-compatible soft-deferred form.

### Valid construction syntax

Unit variants use value syntax:

```beanstalk
ok = Response::Success
```

Unit constructor-call syntax is invalid:

```beanstalk
bad = Response::Success()
```

Payload variants use constructor calls:

```beanstalk
error = Response::Err("bad")
pending_a = Response::Pending(3, "waiting")
pending_b = Response::Pending(retry_count = 3, message = "waiting")
pending_c = Response::Pending(3, message = "waiting")
```

Constructor argument rules should match struct constructor rules:

- Positional args before named args.
- No positional args after named args.
- Duplicate args rejected.
- Unknown field names rejected.
- Missing required payload fields rejected.
- Type mismatch rejected.
- Defaults should follow the same rules as struct field defaults if inherited through the shared field parser. If this is not currently robust, reject choice payload field defaults with a specific diagnostic rather than silently half-supporting them.

### Bare variant names

Bare names are allowed only in match arms, where the scrutinee type disambiguates them:

```beanstalk
if response is:
    case Success => io("ok")
    case Err(message) => io(message)
;
```

Everywhere else, construction is qualified:

```beanstalk
value = Response::Success
```

### Pattern payload extraction

Valid:

```beanstalk
if response is:
    case Response::Err(message) => io(message)
    case Pending(retry_count, message) => io(message)
    case Success => io("ok")
;
```

Invalid for now:

```beanstalk
case Err(message = text) => ...
case Err(text as message) => ...
case Err(text) => ...     -- if the payload field is named `message`
case Success() => ...
case Err => ...
```

For this milestone, captured names must exactly match the payload field names, in declaration order.

This is intentionally strict. It preserves readability and avoids inventing an aliasing sub-language before there is a real need.

### Future binding rename syntax

Add to docs/matrix as deferred:

```beanstalk
case Err(text as message) => ...
```

Meaning: bind field `message` as local `text`.

This is useful later for unavoidable local name collisions. It should not be implemented in this milestone.

### Payload access outside matching

Deferred:

```beanstalk
count = response.retry_count
```

For Alpha, payload fields are accessed only through pattern matching. Direct field access requires variant narrowing, optional returns, or richer control-flow refinement. That is too much for this milestone.

### Exhaustiveness

For this milestone, exhaustiveness remains tag-level:

```beanstalk
if response is:
    case Err(message) => ...
    case Pending(retry_count, message) => ...
    case Success => ...
;
```

This covers all variants.

Do not attempt nested payload exhaustiveness yet. Add a matrix note that nested patterns and nested exhaustiveness are deferred.

### Equality

Unit variant equality only:

```beanstalk
if response is Response::Success:
    ...
;
```

Payload structural equality is deferred.

### Recursive and generic choices

Deferred:

```beanstalk
Json ::
    Null,
    Array |
        values {Json},
    |,
;
```

Deferred:

```beanstalk
Result type T, E ::
    Ok |
        value T,
    |,
    Err |
        error E,
    |,
;
```

Add scaffolding comments where useful, but do not implement recursive layout or generic choice lowering in this milestone.

---

## Target runtime/HIR model

### Frontend semantic identity

Keep choices as nominal types:

```rust
DataType::Choices {
    nominal_path,
    variants,
}
```

But change `ChoiceVariant` from a unit-only shell to payload-aware metadata:

```rust
pub struct ChoiceVariant {
    pub id: StringId,
    pub payload: ChoiceVariantPayload,
    pub location: SourceLocation,
}

pub enum ChoiceVariantPayload {
    Unit,
    Record {
        fields: Vec<Declaration>,
    },
}
```

Do not keep the old `data_type: DataType` field. A variant is not itself a standalone type in the surface language. Its payload fields are part of the variant metadata.

### AST expression shape

Replace or extend:

```rust
ExpressionKind::ChoiceVariant { ... }
```

into a construction shape that can represent unit and payload variants:

```rust
ExpressionKind::ChoiceConstruct {
    nominal_path: InternedPath,
    variant: StringId,
    tag: usize,
    fields: Vec<Declaration>,
}
```

For unit variants, `fields` is empty.

Do not model choice constructors as function calls. Constructors have nominal variant identity, tag layout, payload metadata, const behavior, and pattern/exhaustiveness behavior. A fake function call will produce worse code later.

### HIR carrier representation

Add a shared representation for variant construction while preserving distinct type identity:

```rust
pub enum HirVariantCarrier {
    Choice { choice_id: ChoiceId },
    Option,
    Result,
}

pub struct HirVariantField {
    pub name: Option<StringId>,
    pub value: HirExpression,
}

pub enum HirExpressionKind {
    VariantConstruct {
        carrier: HirVariantCarrier,
        variant_index: usize,
        fields: Vec<HirVariantField>,
    },

    VariantPayloadGet {
        carrier: HirVariantCarrier,
        source: Box<HirExpression>,
        variant_index: usize,
        field_index: usize,
    },

    ...
}
```

Keep these HIR type kinds distinct:

```rust
HirTypeKind::Choice { choice_id }
HirTypeKind::Option { inner }
HirTypeKind::Result { ok, err }
```

This satisfies the target: shared representation for construction/matching, distinct semantic types for diagnostics and optimization.

Result-specific operations can remain separate at first:

```rust
ResultPropagate
ResultIsOk
ResultUnwrapOk
ResultUnwrapErr
```

They can lower through `VariantPayloadGet` internally later, but do not block choice work on rewriting all result handling at once.

### JS representation

Use readable object carriers now:

```js
{ tag: 0 }
{ tag: 1, message: "bad" }
{ tag: 2, retry_count: 3, message: "waiting" }
```

Use numeric tags for choices. This keeps Wasm-oriented lowering obvious later.

For Options/Results, keep current string tags during the transition if needed, but the shared HIR carrier should make it possible to later normalize them to numeric tags if desired. Do not force a JS runtime migration unless tests prove the shape is worth changing.

---

## Roadmap and matrix updates

### `docs/src/docs/progress/#page.bst`

Update these rows as the phases land.

#### Core Alpha surface: Choices

Current row says unit variants are the current supported subset and payloads/constructors are deferred. Replace with:

- Status: `Supported` after Phase 5.
- Coverage: `Broad`.
- Runtime target: `JS / HTML`.
- Watch points:
  - Unit variants use `Choice::Variant`, not `Choice::Variant()`.
  - Payload variants must use record-body syntax.
  - Payload construction supports positional and named constructor arguments.
  - Payload extraction is supported through match patterns only.
  - Direct payload field access remains deferred.
  - Generic and recursive choices remain deferred.

During Phases 1-4, keep status `Partial`, but update the watch points to show which sub-surface is implemented.

#### Deferred/reserved rows to replace

Current deferred rows should be split. Do not leave them vague.

Replace:

- `Choice payload declarations`
- `Choice tagged/default declarations`
- `Choice constructor use`
- `Full tagged unions`

with more precise rows:

| Surface | Status after plan | Notes |
|---|---:|---|
| Choice unit variants | Supported | Unit variants are constructed as `Choice::Variant`. Empty constructor calls are invalid. |
| Choice record payload variants | Supported | Only `Variant | field Type, ... |` syntax. |
| Choice payload shorthand | Rejected / Reserved | `Variant Type` is invalid. Diagnostic should point to record-body syntax. |
| Choice constructor use | Supported | Payload variants use constructor calls; unit variants do not. |
| Choice payload matching | Supported | Constructor-like patterns with original field names only. |
| Choice payload field access | Deferred | No `value.field` until variant narrowing/refinement is designed. |
| Choice binding renames | Deferred | Future syntax: `<new_name> as <original_name>`. |
| Choice nested payload patterns/exhaustiveness | Deferred | Exhaustiveness is tag-level for now. |
| Choice recursive types | Deferred | Requires layout/indirection design. |
| Choice generic declarations | Deferred | Syntax ideas live in `docs/src/docs/generics.md`; do not implement here. |
| Choice payload structural equality | Deferred | Unit equality only for now. |
| Choice default variant values | Deferred | If field defaults are inherited, document separately from variant defaults. |
| Wasm payload layout for choices | Experimental/Deferred | JS is the supported Alpha backend. |

#### Pattern matching row

Keep Pattern Matching as `Partial` if negated patterns and general wildcard/capture surfaces remain deferred. But add the supported sub-surface:

- Choice payload capture patterns: Supported after Phase 4/5.
- Generic capture/tagged patterns: Deferred.
- Rename captures: Deferred.
- Nested payload patterns: Deferred.
- `case _ =>`: still not supported; use `else =>`.

### `docs/roadmap/roadmap.md`

Add a link under the Choices/Pattern Matching target area:

```md
- [Full non-generic tagged union Choices](choices-full-tagged-unions-plan.md)
```

Add a short note:

> This plan promotes choice payload declarations, constructors, JS lowering, and payload match extraction. It explicitly defers generic choices, recursive choices, direct payload field access, nested payload exhaustiveness, and structural equality.

### `docs/src/docs/choices/#page.bst`

Replace the current "payloads deferred" section with:

- Unit variant construction.
- Record payload declaration.
- Named/positional payload construction.
- Pattern extraction.
- Invalid shorthand examples.
- Invalid unit constructor call examples.
- Deferred features list.

### `docs/language-overview.md`

Update the Choices section in the syntax summary and main body:

- Replace `Option2 String` shorthand with record-body syntax.
- Remove `Response::Success()` from valid examples.
- Add payload pattern examples.
- Add deferred direct field access note.

### `docs/src/docs/pattern-matching/#page.bst`

Add:

- Choice payload pattern examples.
- Field-name strictness.
- Capture name collision diagnostic note.
- Deferred rename syntax.
- Tag-level exhaustiveness rule.

### `tests/cases/manifest.toml`

Remove or repurpose current deferred choice cases:

- `choice_payload_decl_deferred`
- `choice_tagged_decl_deferred`
- `choice_constructor_use_deferred`
- `choice_constructor_imported_deferred_rejected`

Replace with success and targeted failure cases listed in each phase below.

Keep `choice_default_decl_deferred` if variant defaults remain unsupported.

---

# Implementation phases

## Phase 0 — Baseline design alignment and test inventory

### Context

Before changing code, make the intended syntax explicit in docs and test names. The current repo intentionally rejects payload choices. The first risk is accidentally leaving old "deferred for Alpha" diagnostics and tests around after the feature becomes supported.

### Implementation steps

1. Add this plan to the repo as:

   ```text
   docs/roadmap/choices-full-tagged-unions-plan.md
   ```

2. Update `docs/roadmap/roadmap.md` with a link to the plan.

3. Add TODO markers to `docs/src/docs/progress/#page.bst` but keep current statuses until code lands:
   - "Choice payload declarations: planned promotion."
   - "Choice constructor use: planned promotion."
   - "Capture/tagged patterns: split into payload capture vs general capture."

4. Inventory current choice tests:
   - Unit declaration/use.
   - Import visibility.
   - Exhaustive match.
   - Non-exhaustive diagnostics.
   - Unknown variant diagnostics.
   - JS carrier shape.
   - Constructor deferred diagnostics.
   - Payload deferred diagnostics.

5. Decide which deferred tests become:
   - Success tests.
   - Rejection tests for invalid old shorthand.
   - Rejection tests for unit constructor calls.
   - Rejection tests for still-deferred defaults/recursive/generic/direct-access cases.

### New/renamed tests

Add placeholder entries only if the runner tolerates them. Otherwise defer manifest edits to the phase that creates each case.

Planned test IDs:

```text
choice_payload_record_declaration_success
choice_payload_shorthand_rejected
choice_unit_constructor_call_rejected
choice_payload_constructor_positional_success
choice_payload_constructor_named_success
choice_payload_constructor_mixed_success
choice_payload_constructor_missing_field_rejected
choice_payload_constructor_unknown_field_rejected
choice_payload_constructor_duplicate_field_rejected
choice_payload_constructor_type_mismatch_rejected
choice_payload_match_capture_success
choice_payload_match_wrong_capture_name_rejected
choice_payload_match_rename_syntax_deferred
choice_payload_match_missing_payload_rejected
choice_payload_match_unit_parens_rejected
choice_payload_direct_field_access_deferred
choice_const_payload_success
choice_payload_structural_equality_deferred
choice_recursive_declaration_deferred
choice_generic_declaration_deferred
```

### Audit/style/validation commit

Commit name suggestion:

```text
docs: add full choices implementation plan
```

Audit checklist:

- The plan does not claim features are already supported.
- Matrix wording stays honest until code lands.
- No generated release docs are hand-edited.
- `docs/src/docs/generics.md` remains draft/future-facing.

Validation:

```bash
just validate
```

If this is docs-only and `just validate` is too slow during planning, at minimum run:

```bash
cargo run --features "detailed_timers" docs
```

---

## Phase 1 — Payload-aware declaration metadata

### Context

The parser currently treats any payload-looking token after a choice variant as a deferred feature. This phase should promote only the record-body payload syntax:

```beanstalk
Err |
    message String,
|,
```

Do not add constructor expressions or payload matching yet. This gives the type system and import/header machinery a stable metadata shape first.

### Implementation steps

1. Update `src/compiler_frontend/declaration_syntax/choice.rs`.

   Replace:

   ```rust
   pub struct ChoiceVariant {
       pub id: StringId,
       pub data_type: DataType,
       pub location: SourceLocation,
   }
   ```

   with:

   ```rust
   pub struct ChoiceVariant {
       pub id: StringId,
       pub payload: ChoiceVariantPayload,
       pub location: SourceLocation,
   }

   pub enum ChoiceVariantPayload {
       Unit,
       Record {
           fields: Vec<Declaration>,
       },
   }
   ```

2. Reuse struct field-list parsing.

   `src/compiler_frontend/declaration_syntax/struct.rs` already uses `parse_signature_members` through `parse_struct_shell`. Do not duplicate field parsing logic.

   Preferred direction:

   - Extract a shared helper if `parse_struct_shell` is too struct-specific.
   - Keep `parse_struct_shell` as the public struct parser.
   - Add a new helper such as:

     ```rust
     parse_record_field_shell(
         token_stream,
         context,
         string_table,
         SignatureMemberContext::StructField,
     )
     ```

   - Use it for struct fields and choice payload fields.

3. Add a parser path for:

   ```beanstalk
   Variant | field Type, other Type |
   ```

   Rules:

   - `Variant` alone => `ChoiceVariantPayload::Unit`.
   - `Variant | ... |` => `ChoiceVariantPayload::Record`.
   - Empty payload body should be rejected. Use a unit variant instead.
   - Duplicate field names rejected.
   - Field names should follow variable/function naming convention.
   - Payload field types create strict dependency edges like struct fields.

4. Reject old shorthand:

   ```beanstalk
   Response :: Err String, Success;
   ```

   Diagnostic direction:

   > Choice payload shorthand is not supported. Use a record payload body: `Err | message String |`.

   This should not say "deferred for Alpha". It is invalid by design.

5. Continue rejecting variant defaults:

   ```beanstalk
   Response :: Err | message String | = ...
   ```

   Diagnostic direction:

   > Choice variant default values are deferred. Construct a value explicitly with `Choice::Variant(...)`.

6. Continue rejecting constructor-style declaration syntax:

   ```beanstalk
   Response :: Err(message String), Success;
   ```

   Diagnostic direction:

   > Constructor-style choice declarations are not supported. Use `Err | message String |`.

7. Update `src/compiler_frontend/headers/types.rs` and dependency sorting code if strict dependency edges currently assume `ChoiceVariant.data_type`.

   Required behavior:

   - Payload field type references participate in top-level dependency sorting.
   - Cross-file imports for payload field types work.
   - Missing payload field types emit normal type diagnostics.

8. Update display/debug helpers that print choices.

   `DataType::display_with_table` should show payload-bearing choices without becoming noisy. A compact display is enough:

   ```text
   Response::{Success, Err(...), Pending(...)}
   ```

9. Update HIR choice registration minimally.

   `HirChoiceVariant` can gain payload metadata now, even if unused until later:

   ```rust
   pub struct HirChoiceVariant {
       pub name: StringId,
       pub fields: Vec<HirChoiceField>,
   }

   pub struct HirChoiceField {
       pub name: StringId,
       pub ty: TypeId,
   }
   ```

   In this phase, it is acceptable to lower only type metadata and keep runtime construction unsupported for payload variants.

### Tests

Add or update:

```text
choice_payload_record_declaration_success
choice_payload_shorthand_rejected
choice_payload_empty_record_rejected
choice_payload_duplicate_field_rejected
choice_payload_unknown_field_type_rejected
choice_payload_imported_field_type_success
choice_payload_constructor_still_rejected_until_phase_2
```

The success test can declare a payload choice and use only a unit variant, or use the choice in a type position, until constructor support lands.

### Matrix update after this phase

In `docs/src/docs/progress/#page.bst`:

- Change `Choice payload declarations` from `Deferred` to `Partial`.
- Add note: "Record-body payload metadata is parsed and type-checked. Value construction and payload matching land in later phases."
- Add `Choice payload shorthand` as `Rejected / Reserved`.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: validate choice payload declaration metadata
```

Audit checklist:

- No new user-input `panic!`, `todo!`, or unsafe `.unwrap()`.
- No duplicate field parsing logic if a shared parser can reasonably be used.
- `choice.rs` stays focused on declaration shell parsing.
- Header parsing still owns top-level declaration shape.
- AST does not rediscover choice payload syntax from raw tokens.
- Deferred diagnostic wording is updated: old shorthand is invalid, not deferred.
- Tests include both success and diagnostic paths.

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 2 — Choice constructor expressions and const choices

### Context

This phase makes `Choice::Variant` and `Choice::Variant(...)` produce real AST values for unit and payload variants.

The critical rule: unit variants do not use constructor-call syntax. Payload variants do.

### Implementation steps

1. Update `parse_choice_variant_value` in:

   ```text
   src/compiler_frontend/ast/expressions/parse_expression_identifiers.rs
   ```

2. Parse unit variants:

   ```beanstalk
   value = Response::Success
   ```

   Behavior:

   - Valid only when the variant has `ChoiceVariantPayload::Unit`.
   - Produces `ExpressionKind::ChoiceConstruct { fields: vec![] }`.
   - `Response::Success()` is rejected.

3. Parse payload variants:

   ```beanstalk
   value = Response::Err("bad")
   value = Response::Err(message = "bad")
   ```

   Behavior:

   - Valid only when the variant has `ChoiceVariantPayload::Record`.
   - Missing `(...)` is rejected.
   - Parentheses use existing constructor/call argument normalization rules.
   - Reuse struct constructor argument validation logic from `src/compiler_frontend/ast/expressions/struct_instance.rs` where possible.
   - Do not duplicate named/positional argument validation.

4. Add a dedicated helper module if `parse_expression_identifiers.rs` starts growing too much.

   Suggested new file:

   ```text
   src/compiler_frontend/ast/expressions/choice_constructor.rs
   ```

   Responsibilities:

   - Resolve variant metadata.
   - Validate unit vs payload construction syntax.
   - Normalize positional/named constructor args.
   - Apply defaults if supported.
   - Emit `ExpressionKind::ChoiceConstruct`.

5. Type-check payload fields.

   Required diagnostics:

   - Missing required payload field.
   - Unknown named payload field.
   - Duplicate field.
   - Positional argument after named argument.
   - Too many positional args.
   - Type mismatch, with expected field name/type.
   - Unit variant called as constructor.
   - Payload variant referenced without constructor args.
   - `~` on constructor args follows the same rules as normal call args.

6. Support const choices.

   This should work:

   ```beanstalk
   # ok = Response::Success
   # err = Response::Err("bad")
   ```

   Rules:

   - Unit variants are compile-time literal-like values.
   - Payload variants are compile-time composite values only if all fields are compile-time constants.
   - Update `Expression::const_value_kind`.
   - Update constant folding/display helpers as needed.

7. Equality behavior.

   Keep unit variant equality working if already supported through tag comparison.

   Reject or defer payload structural equality:

   ```beanstalk
   if response is Response::Err("bad"):
       ...
   ;
   ```

   Diagnostic direction:

   > Structural equality for payload choice variants is deferred. Use pattern matching and compare payload fields inside the arm.

### Tests

Add:

```text
choice_unit_constructor_call_rejected
choice_payload_constructor_positional_success
choice_payload_constructor_named_success
choice_payload_constructor_mixed_success
choice_payload_constructor_missing_field_rejected
choice_payload_constructor_unknown_field_rejected
choice_payload_constructor_duplicate_field_rejected
choice_payload_constructor_positional_after_named_rejected
choice_payload_constructor_type_mismatch_rejected
choice_payload_constructor_without_args_rejected
choice_const_unit_success
choice_const_payload_success
choice_payload_const_non_const_field_rejected
choice_payload_structural_equality_deferred
choice_imported_payload_constructor_success
```

### Matrix update after this phase

In `docs/src/docs/progress/#page.bst`:

- `Choice constructor use`: `Partial` or `Supported` depending on HIR/JS support in the same phase.
- Add explicit note:
  - Unit variant construction: `Choice::Variant`.
  - Payload variant construction: `Choice::Variant(...)`.
  - `Choice::Unit()` is invalid.
  - Payload structural equality remains deferred.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: validate choice constructors and const choice values
```

Audit checklist:

- Choice constructors are not lowered as fake function calls.
- Argument normalization is shared with struct constructors or extracted into a common helper.
- Unit and payload syntax diagnostics are precise.
- Const classification distinguishes literal-like unit variants from composite payload variants.
- No broad compatibility wrapper for old shorthand.
- No implicit bare construction outside match arms.

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 3 — HIR choice payload model and JS constructor lowering

### Context

Constructor expressions need a backend carrier. Current HIR lowers choices as bare integer tags, and JS compares them as integers. Payload choices need object-like carriers in JS and payload metadata in HIR.

This phase should keep backend behavior simple and readable. Optimize later.

### Implementation steps

1. Replace or extend HIR choice construction.

   In `src/compiler_frontend/hir/expressions.rs`, introduce shared variant construction:

   ```rust
   HirExpressionKind::VariantConstruct {
       carrier: HirVariantCarrier,
       variant_index: usize,
       fields: Vec<HirVariantField>,
   }
   ```

   Initially, use it for choices. Option/Result unification can be completed in Phase 6.

2. Preserve HIR type identity.

   The expression `ty` remains `HirTypeKind::Choice { choice_id }`.

3. Update `src/compiler_frontend/hir/hir_expression.rs`.

   Lower `ExpressionKind::ChoiceConstruct` into `VariantConstruct`.

   For each payload field:

   - Lower field expressions in source/constructor evaluation order.
   - Preserve prelude statements.
   - Use field names from declaration metadata.
   - Keep field order stable by declaration order, not by named-argument source order.

4. Update `HirChoice` registry.

   In `src/compiler_frontend/hir/module.rs`, store variant field names and types.

5. Update HIR display.

   In `src/compiler_frontend/hir/hir_display.rs`, print choices in readable debug output:

   ```text
   choice Response::Err { message: ... }
   ```

6. Update HIR validation.

   In `src/compiler_frontend/hir/hir_validation.rs`:

   - Choice ID must exist.
   - Variant index must exist.
   - Payload field count must match.
   - Payload field types must match.
   - Unit variants must carry zero fields.

7. Update JS expression lowering.

   In `src/backends/js/js_expr.rs`:

   Lower unit choice:

   ```js
   { tag: 0 }
   ```

   Lower payload choice:

   ```js
   { tag: 1, message: <expr> }
   ```

   Use escaped JS property names if field names need escaping. Since Beanstalk identifiers are controlled, direct identifiers are probably fine, but do not assume if the existing string table can contain non-JS-safe names.

8. Update choice match condition for unit choices.

   Since choices are no longer bare ints, change matching from:

   ```js
   __match === 0
   ```

   to:

   ```js
   __match.tag === 0
   ```

   This affects existing unit choice tests and JS carrier shape tests.

9. Decide the JS shape migration.

   Existing `js_choice_carrier_shape` currently likely expects integer tags. Rewrite it to expect object carriers.

   This is worth doing now. A single representation for unit and payload variants is cleaner than special-casing unit variants as ints forever.

### Tests

Update existing:

```text
js_choice_carrier_shape
js_choice_match_lowering_shape
choice_basic_declaration_and_use
choice_assignment_flow
choice_function_return_flow
choice_cross_file_carrier_shape
```

Add:

```text
choice_payload_js_carrier_shape
choice_payload_return_js_shape
choice_payload_assignment_js_shape
choice_payload_imported_js_shape
```

### Matrix update after this phase

- `Choice constructor use`: `Supported` for construction/lowering.
- `Choices` main row can still remain `Partial` until payload matching lands.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: validate HIR and JS choice payload lowering
```

Audit checklist:

- HIR has one choice construction representation, not separate unit/payload ad hoc nodes.
- JS uses one carrier shape for unit and payload choices.
- Existing unit choice behavior still works.
- HIR validation catches malformed internal carrier shapes.
- No stringly-typed variant names are used for runtime equality; use numeric tags.
- Existing Option/Result tests still pass.

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 4 — Payload pattern capture and match-arm scoped bindings

### Context

This is the semantic core of tagged unions. Payloads are only useful if match arms can destructure them.

The hard part is not parsing `Err(message)`. The hard part is introducing the captured payload fields into the correct arm scope without breaking no-shadowing, guards, HIR lowering, or borrow analysis.

### Implementation steps

1. Extend AST match patterns.

   In `src/compiler_frontend/ast/statements/match_patterns.rs`, replace:

   ```rust
   MatchPattern::ChoiceVariant {
       nominal_path,
       variant,
       tag,
       location,
   }
   ```

   with something like:

   ```rust
   MatchPattern::ChoiceVariant {
       nominal_path: InternedPath,
       variant: StringId,
       tag: usize,
       captures: Vec<ChoicePayloadCapture>,
       location: SourceLocation,
   }

   pub struct ChoicePayloadCapture {
       pub field_name: StringId,
       pub field_type: DataType,
       pub field_index: usize,
       pub binding_path: InternedPath,
       pub location: SourceLocation,
   }
   ```

2. Parse constructor-like patterns.

   Unit variants:

   ```beanstalk
   case Success =>
   case Response::Success =>
   ```

   Payload variants:

   ```beanstalk
   case Err(message) =>
   case Response::Pending(retry_count, message) =>
   ```

3. Reject invalid pattern forms.

   Required diagnostics:

   - Unit variant with parentheses:

     ```beanstalk
     case Success() =>
     ```

   - Payload variant without captures:

     ```beanstalk
     case Err =>
     ```

   - Wrong capture name:

     ```beanstalk
     case Err(text) =>
     ```

     Diagnostic should say:

     > Variant `Err` expects payload field `message` at position 1.

   - Named pattern assignment:

     ```beanstalk
     case Err(message = text) =>
     ```

     Diagnostic:

     > Payload pattern renaming is deferred. Use the declared field name `message`.

   - Future rename syntax:

     ```beanstalk
     case Err(text as message) =>
     ```

     Diagnostic:

     > Payload binding rename syntax is deferred.

   - Too many/few captures.
   - Duplicate capture names.
   - Capture name conflicts with an existing visible local due to Beanstalk's no-shadowing rule.

4. Add pattern capture bindings to arm scopes.

   Design requirement:

   - Captures should be available in the arm guard and arm body.
   - Captures are immutable references/values by default.
   - Captures should not leak outside the arm.
   - Captures must obey no-shadowing.

   Implementation direction:

   - Parse the pattern first.
   - Build a child `ScopeContext` for the arm.
   - Insert capture declarations into that arm scope before parsing the guard/body.
   - Guard parsing should use the arm scope so this works:

     ```beanstalk
     case Err(message) if message is not "" => io(message)
     ```

   If guard capture support creates too much churn, it is acceptable to stage it as:
   - Phase 4a: captures available in body.
   - Phase 4b: captures available in guards.

   But the final plan should support both.

5. Add HIR payload extraction.

   HIR needs a way to materialize captures in the selected arm block.

   Preferred direction:

   - Add `HirExpressionKind::VariantPayloadGet`.
   - During match lowering, at the start of each payload arm body block:
     - allocate locals for captures,
     - assign from `VariantPayloadGet { source: scrutinee, variant_index, field_index }`,
     - then lower the arm body.

   This keeps backend lowering simple: normal body references load locals.

6. Preserve exhaustiveness behavior.

   Existing choice exhaustiveness should continue to operate on variant tags only.

   - `case Err(message)` covers all `Err` values.
   - Guards still force `else =>`.
   - Duplicate variant arms still rejected.
   - The same variant cannot appear twice even with different payload captures.

7. Update HIR patterns.

   In `src/compiler_frontend/hir/patterns.rs`, either:

   - keep `HirPattern::ChoiceVariant { choice_id, variant_index }` and put capture extraction statements in arm blocks, or
   - add capture metadata for display/validation only.

   Do not make JS backend responsible for declaring capture variables in match conditions. Keep extraction in arm blocks.

8. Update JS match lowering.

   Match condition:

   ```js
   __match.tag === 1
   ```

   Arm block prelude:

   ```js
   __bs_assign_value(message, __match.message);
   ```

   If locals are alias wrappers, emit through the normal assignment machinery rather than raw `const`.

### Tests

Add:

```text
choice_payload_match_capture_success
choice_payload_match_qualified_capture_success
choice_payload_match_guard_uses_capture_success
choice_payload_match_wrong_capture_name_rejected
choice_payload_match_too_few_captures_rejected
choice_payload_match_too_many_captures_rejected
choice_payload_match_duplicate_capture_rejected
choice_payload_match_capture_shadowing_rejected
choice_payload_match_rename_syntax_deferred
choice_payload_match_named_assignment_rejected
choice_payload_match_missing_payload_rejected
choice_payload_match_unit_parens_rejected
choice_payload_match_duplicate_variant_rejected
choice_payload_match_guard_requires_else
choice_payload_match_exhaustive_tag_level_success
```

### Matrix update after this phase

- `Choice payload matching`: `Supported`.
- `Capture/tagged match patterns`: split:
  - `Choice payload capture patterns`: `Supported`.
  - `General capture/tagged patterns`: `Deferred`.
  - `Rename capture syntax`: `Deferred`.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: validate choice payload pattern capture
```

Audit checklist:

- Captures are scoped to exactly one arm.
- Captures are available in guards and bodies, or the limitation is explicitly tested/documented before finalizing.
- No-shadowing is enforced.
- Match exhaustiveness remains tag-level.
- Guarded arms still require `else =>`.
- HIR lowering materializes payloads before body use.
- JS backend does not inspect AST-only capture metadata.
- Diagnostics list expected payload field names.

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 5 — Docs, matrix, and roadmap promotion

### Context

After declaration, construction, HIR lowering, JS lowering, and payload matching are green, the implementation matrix can stop calling full tagged unions deferred. But the wording must still be precise: generic/recursive/nested equality/access features are not done.

### Implementation steps

1. Update `docs/src/docs/choices/#page.bst`.

   Suggested final example:

   ```beanstalk
   Response ::
       Success,
       Err |
           message String,
       |,
       Pending |
           retry_count Int,
           message String,
       |,
   ;

   ok = Response::Success
   err = Response::Err("bad")
   pending = Response::Pending(retry_count = 3, message = "waiting")

   if pending is:
       case Success => io("done")
       case Err(message) => io(message)
       case Pending(retry_count, message) => io(message)
   ;
   ```

2. Add invalid examples:

   ```beanstalk
   -- Invalid: unit variants are values.
   value = Response::Success()

   -- Invalid: payload shorthand is not supported.
   Response :: Err String, Success;

   -- Invalid: payload aliases are deferred.
   case Err(text as message) => ...
   ```

3. Update `docs/src/docs/pattern-matching/#page.bst`.

   Add:
   - choice payload captures,
   - strict field-name rule,
   - tag-level exhaustiveness,
   - rename syntax deferred.

4. Update `docs/language-overview.md`.

   Replace old shorthand payload examples.

5. Update `docs/src/docs/progress/#page.bst`.

   Final status suggestions:

   - `Choices`: `Supported`, coverage `Broad`, target `JS / HTML`.
   - `Pattern matching`: still `Partial` if negated/general captures are deferred, but with choice payload captures listed as supported.
   - Add precise deferred rows listed above.

6. Update `tests/cases/manifest.toml`.

   Ensure every new case is listed.

7. Do not hand-edit `docs/release/...`.

   Regenerate docs through the project build.

### Audit/style/validation commit

Commit name suggestion:

```text
docs: promote tagged union choice surface
```

Audit checklist:

- Docs no longer show `Err String` as valid.
- Docs no longer show `Response::Success()` as valid.
- Matrix does not overclaim generics, recursion, direct payload access, or structural equality.
- Progress page distinguishes supported payload captures from deferred general capture patterns.
- All examples compile if copied into tests, except explicitly invalid examples.
- Docs use current syntax and names consistently.

Validation:

```bash
cargo run --features "detailed_timers" docs
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 6 — Shared HIR carrier cleanup for Choices, Options, and Results

### Context

The design target is shared HIR representation for variant construction/matching without erasing semantic identity.

This phase should be done after Choice payloads work end-to-end, because changing Options/Results at the same time as payload choices will make failures harder to isolate.

### Implementation steps

1. Introduce shared HIR variant carrier types.

   Suggested location:

   ```text
   src/compiler_frontend/hir/variants.rs
   ```

   Or keep inside `hir/expressions.rs` if small.

   Suggested types:

   ```rust
   pub enum HirVariantCarrier {
       Choice { choice_id: ChoiceId },
       Option,
       Result,
   }

   pub struct HirVariantField {
       pub name: Option<StringId>,
       pub value: HirExpression,
   }
   ```

2. Convert constructors.

   Replace:

   ```rust
   OptionConstruct
   ResultConstruct
   ChoiceVariant / ChoiceConstruct
   ```

   with:

   ```rust
   VariantConstruct
   ```

   Or keep old public enum variants temporarily as thin lowering aliases only if that avoids a massive one-commit rewrite. Do not keep both long term.

3. Preserve result-specific operations.

   Keep:

   ```rust
   ResultPropagate
   ResultIsOk
   ResultUnwrapOk
   ResultUnwrapErr
   ```

   These are not generic variant construction. They are result-control-flow operations.

4. Update JS lowering.

   Option and Result can continue to lower to their current JS shape:

   ```js
   { tag: "none" }
   { tag: "ok", value: ... }
   ```

   The shared HIR representation does not require identical backend tags immediately.

   However, add one helper inside JS lowering:

   ```rust
   lower_variant_construct(...)
   ```

   It should branch on carrier identity, not on scattered expression enum variants.

5. Update HIR display and validation.

   Validation should know:
   - Choice payload arity from `HirChoice`.
   - Option variants:
     - `none`: 0 fields.
     - `some`: 1 field.
   - Result variants:
     - `ok`: 1 field.
     - `err`: 1 field.

6. Update tests.

   Existing Option/Result tests must pass unchanged.

   Add HIR unit tests if existing HIR test structure supports it:

   ```text
   hir_variant_construct_choice
   hir_variant_construct_option_none
   hir_variant_construct_result_ok
   ```

### Matrix update after this phase

Add to Choices watch points or HIR/internal notes:

> Choices, Options, and Results use a shared HIR variant construction representation, while retaining distinct semantic type kinds.

Do not expose this as user-facing syntax.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: validate shared HIR variant carrier model
```

Audit checklist:

- HIR no longer has three unrelated construction representations unless deliberately staged.
- Semantic type kinds remain distinct.
- Result propagation behavior is unchanged.
- Existing result/option tests remain green.
- JS helper code is more centralized, not more scattered.
- No backend starts depending on source syntax names.

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 7 — Deferred feature diagnostics and guardrails

### Context

Once the main feature works, the compiler needs sharp diagnostics for deliberately unsupported surfaces. This prevents design drift.

### Implementation steps

1. Add or update diagnostics for:

   - Direct payload field access.
   - Recursive choice declarations.
   - Generic choice declarations.
   - Payload shorthand.
   - Unit constructor calls.
   - Payload structural equality.
   - Nested payload pattern syntax if any is tempting to parse.
   - Capture rename syntax.
   - Variant default values.

2. Recursive choice detection.

   Minimum for this phase:

   - Direct recursion should be rejected with a structured diagnostic:

     ```beanstalk
     Node ::
         Branch |
             child Node,
         |,
         Leaf,
     ;
     ```

   - If indirect recursion is harder, add a conservative diagnostic or a TODO linked to dependency graph cycle handling.

   Do not let recursive payloads accidentally pass into HIR layout.

3. Generic choice declarations.

   If parser sees:

   ```beanstalk
   Result type T, E ::
   ```

   Diagnostic should say generics are future-facing and point to the generics design docs if appropriate.

   Do not implement generic scaffolding beyond comments/types that do not affect behavior.

4. Direct field access.

   If a choice value is used with `.field`, emit:

   > Choice payload field access is deferred. Use pattern matching to extract payload fields.

   This may belong in member/field access parsing, not choice code.

5. Payload equality.

   If comparing against a payload constructor in an `is` condition, reject with:

   > Payload structural equality is deferred. Use pattern matching and compare fields inside the matching arm.

### Tests

Add:

```text
choice_payload_direct_field_access_deferred
choice_recursive_direct_declaration_deferred
choice_generic_declaration_deferred
choice_payload_structural_equality_deferred
choice_variant_default_value_deferred
choice_capture_rename_deferred
choice_nested_payload_pattern_deferred
```

### Matrix update after this phase

Ensure every deliberately deferred item appears in `docs/src/docs/progress/#page.bst`.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: validate deferred choice feature diagnostics
```

Audit checklist:

- Every deferred feature has a canonical test.
- Deferred features fail with structured diagnostics, not parser fallthrough.
- Diagnostic wording is future-proof and precise.
- No unsupported syntax reaches HIR.
- No unsupported syntax reaches JS.

Validation:

```bash
cargo clippy
cargo test
cargo run tests
```

Prefer:

```bash
just validate
```

---

## Phase 8 — Final integration sweep

### Context

This is the "make it boring" phase. The feature is not done until it survives real examples, docs builds, and adversarial control flow.

### Implementation steps

1. Add a broad adversarial fixture:

   ```text
   tests/cases/adversarial_choice_payload_control_flow
   ```

   Include:

   - imported choice type,
   - unit and payload constructors,
   - const payload choice,
   - function returning choice,
   - function accepting choice,
   - nested match,
   - guarded payload arm,
   - loop containing match,
   - template output using captured payload fields,
   - result/option interop if currently ergonomic.

2. Add backend-contract tests:

   ```text
   js_choice_payload_carrier_shape
   js_choice_payload_match_shape
   ```

   Keep them robust enough not to overfit whitespace.

3. Add docs examples as integration tests if practical.

4. Review HIR output with debug flags:

   ```bash
   cargo run --features "show_hir" -- build tests/cases/choice_payload_match_capture_success/input/main.bst
   ```

5. Review JS output for shape and readability.

6. Run validation.

### Audit/style/validation commit

Commit name suggestion:

```text
audit: final tagged union integration sweep
```

Audit checklist:

- `choice.rs` still reads as declaration parsing, not AST construction.
- `parse_expression_identifiers.rs` has not grown into a god file; choice constructor logic is split if needed.
- Pattern capture scoping is documented with comments explaining what and why.
- HIR has no AST leftovers.
- JS has one coherent choice carrier shape.
- Existing unit choice tests still pass.
- Existing Option/Result tests still pass.
- Docs build.
- Speed test has no obvious regression.

Validation:

```bash
just validate
```

Also run the debug/perf commands if not covered by `just validate`:

```bash
cargo run --features "detailed_timers" docs
cargo run --release --features "detailed_timers" speed-test.bst
```

---

# Recommended commit stack

Use implementation commits followed by audit/validation commits.

```text
1. docs: add full choices implementation plan
2. audit: validate choices plan docs

3. frontend: parse choice record payload declarations
4. audit: validate choice payload declaration metadata

5. frontend: add choice constructor expressions
6. audit: validate choice constructors and const values

7. hir-js: lower choice payload carriers
8. audit: validate HIR and JS choice payload lowering

9. frontend-hir: add choice payload pattern captures
10. audit: validate choice payload pattern capture

11. docs: promote tagged union choice surface
12. audit: validate tagged union docs and matrix

13. hir: unify variant construction carriers
14. audit: validate shared HIR variant model

15. diagnostics: harden deferred choice feature errors
16. audit: validate deferred choice diagnostics

17. tests: add adversarial tagged union coverage
18. audit: final tagged union integration sweep
```

This is a lot of commits, but it is the right shape. Choices touch parsing, type checking, imports, HIR, borrow validation, JS lowering, docs, and tests. Trying to land this in one huge commit will hide bugs.

---

# Design risks and recommended constraints

## Risk: record payload fields become a second struct system

Avoid this.

Payload records should reuse existing field parsing and constructor argument validation, but they should not become named struct types.

A variant payload is part of the choice value, not a standalone nominal type.

## Risk: capture bindings weaken no-shadowing

Do not allow silent shadowing.

If a payload field name collides with an existing local, reject it for now. The future rename syntax exists exactly to solve that case.

## Risk: fake function-call constructors leak everywhere

Do not lower choice constructors as `FunctionCall`.

It will complicate diagnostics, HIR, backend lowering, and future exhaustiveness.

## Risk: over-unifying Options/Results too early

The shared HIR carrier is right, but do it after choice payloads work.

Result propagation is a control-flow feature, not just variant construction. Keep it separate until the carrier model is stable.

## Risk: docs overclaim "full tagged unions"

Use "full non-generic tagged unions" or "payload Choices" in docs.

Do not imply support for:
- generics,
- recursive types,
- direct narrowed access,
- structural equality,
- nested patterns,
- Wasm payload layout.

## Risk: JS carrier shape churn

Move unit choices from bare integer tags to object carriers in the same phase as payload JS lowering.

This causes test churn once, not twice.

---

# Final target behavior examples

## Basic use

```beanstalk
Response ::
    Success,
    Err |
        message String,
    |,
    Pending |
        retry_count Int,
        message String,
    |,
;

format_response |response Response| -> String:
    if response is:
        case Success => return "success"
        case Err(message) => return message
        case Pending(retry_count, message) => return [:
            Retry [retry_count]: [message]
        ]
    ;
;
```

## Named and positional construction

```beanstalk
a = Response::Err("bad")
b = Response::Err(message = "bad")
c = Response::Pending(3, message = "waiting")
```

## Const payload

```beanstalk
# default_error = Response::Err("Missing value")
```

## Invalid old syntax

```beanstalk
Response :: Err String, Success;
```

Diagnostic should point to:

```beanstalk
Response ::
    Err |
        message String,
    |,
    Success,
;
```

## Invalid unit constructor call

```beanstalk
value = Response::Success()
```

Diagnostic should say unit variants are values:

```beanstalk
value = Response::Success
```

## Invalid capture rename for now

```beanstalk
case Err(text as message) => io(text)
```

Diagnostic should say rename syntax is deferred and current syntax must use original field names:

```beanstalk
case Err(message) => io(message)
```

---

# Definition of done

This implementation is complete when:

- Record-body payload variants parse and type-check.
- Payload shorthand is rejected.
- Unit constructors reject `()`.
- Payload constructors support positional, named, and mixed arguments.
- Const unit and const payload choice values work.
- HIR has payload-aware choice metadata.
- JS lowers choices to a stable object carrier shape.
- Payload match captures work in arm bodies and guards.
- Exhaustiveness remains correct at the variant-tag level.
- Deferred feature diagnostics are explicit and tested.
- Options/Results share the same core HIR variant-construction representation, or there is a staged follow-up commit explicitly tracking the remaining conversion.
- `docs/src/docs/progress/#page.bst` accurately separates supported and deferred surfaces.
- `docs/src/docs/choices/#page.bst` and `docs/src/docs/pattern-matching/#page.bst` show current syntax.
- `docs/roadmap/roadmap.md` links to this full implementation plan.
- `just validate` passes.
