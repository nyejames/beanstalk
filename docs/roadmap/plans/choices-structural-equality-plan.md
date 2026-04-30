# Choices Hardening and Structural Equality Implementation Plan

## Purpose

This plan hardens Beanstalk Choices and implements structural equality for choice values.

It is written as an agent-executable phase plan. Each phase must leave the repo in a validated state and must run:

```sh
just validate
```

Each phase must update the implementation matrix and docs when behavior changes.

## Scope

Included:

- audit current choice parser/AST/HIR/backend behavior
- structural equality for unit and payload choices
- structural equality diagnostics for unsupported payload fields
- immutable payload-field policy hardening
- constructor and payload extraction regression coverage
- docs and matrix reconciliation

Deferred:

- generic choices
- recursive choices
- direct payload field access outside pattern extraction
- mutable payload fields
- nested choice payload patterns
- variant defaults
- payload shorthand if still rejected
- custom equality overloads

Policy decisions:

- Structural equality should now be implemented.
- Payload fields remain immutable.
- Payload field mutation remains rejected.
- Direct payload field access remains deferred unless extracted through pattern matching.
- Nested choice payload patterns remain deferred and are covered by the pattern matching plan, not this plan.

## Required references

Before any phase, read:

- `AGENTS.md`
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`
- `tests/cases/manifest.toml`

Likely implementation areas:

- `src/compiler_frontend/declaration_syntax/choice*`
- `src/compiler_frontend/ast/`
- choice type representation
- expression/operator parsing and type checking
- HIR expression representation
- JS backend lowering for equality
- diagnostics and warnings
- integration tests under `tests/cases/`

Exact files may differ. Search the repo before editing.

---

## Phase 0 — Choice implementation audit

### Summary

Audit current Choice implementation state before changing equality behavior.

### Why

Choices already support unit variants, payload variants, constructor calls, imports, pattern matching, and JS carrier shapes. Structural equality must be threaded through those existing shapes rather than bolted on separately.

### Implementation steps

- [ ] Locate choice declaration parsing.
- [ ] Locate choice variant and payload type representation.
- [ ] Locate constructor call parsing and type checking.
- [ ] Locate choice value HIR representation.
- [ ] Locate JS carrier representation for unit and payload choices.
- [ ] Locate existing equality operator type checking/lowering.
- [ ] Locate tests for unit equality, payload equality rejection, constructor calls, imports, and payload extraction.
- [ ] Record current unsupported cases in the plan or commit notes.

### Matrix/docs updates

- [ ] Update matrix only if current wording is inaccurate.

### Audit / style review

- [ ] Do not change equality behavior in this audit phase.
- [ ] Identify duplicated choice-shape logic that later phases should consolidate.
- [ ] Identify user-input `panic!`, `todo!`, `unwrap`, or `expect` paths for later cleanup.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- The agent knows where choice declarations, values, equality, and JS lowering are owned.
- Existing tests are mapped.
- No speculative behavior changes are introduced.

---

## Phase 1 — Define the structural equality contract

### Summary

Document and encode the structural equality rules before implementation.

### Why

Structural equality must be predictable. The compiler should reject equality if any payload field cannot be structurally compared.

### Equality contract

Two choice values are equal when:

1. They have the same choice type.
2. They have the same variant.
3. If the variant has payload fields, every payload field is equal in declaration order.

Two choice values are not comparable when:

- their choice types differ
- either side is not a choice value and no existing equality rule applies
- any payload field type has no supported equality operation

### Supported payload equality

Support fields whose types already support equality, likely including:

- Int
- Float
- Bool
- Char
- String
- choices with supported structural equality
- options/results if they lower through choice-like equality and are already semantically comparable

Be conservative for Alpha. If collections, structs, function values, external opaque values, or backend objects do not have a solid equality contract, reject them with diagnostics.

### Implementation steps

- [ ] Add an internal helper for checking whether a `DataType` supports structural equality.
- [ ] Ensure it is recursive but cycle-safe.
- [ ] Reject recursive choices if they are already deferred.
- [ ] Keep equality type checking frontend-owned.
- [ ] Add clear comments explaining the structural equality contract.

### Diagnostics

Suggested:

```text
Choice payload equality is not supported because field '<field>' has type '<type>', which does not support equality.
```

For mismatched choice types:

```text
Cannot compare choices of different types: '<left>' and '<right>'.
```

### Tests

Add type-checking/failure cases:

```text
choice_equality_different_choice_types_rejected
choice_equality_payload_unsupported_struct_rejected
choice_equality_payload_unsupported_collection_rejected
choice_equality_payload_unsupported_external_rejected
choice_equality_recursive_choice_rejected_or_deferred
```

### Matrix/docs updates

- [ ] Update `docs/language-overview.md` with the equality contract.
- [ ] Update `docs/src/docs/progress/#page.bst`: structural equality is planned/being implemented; unsupported payload field types are rejected.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Equality rules are documented in code comments and docs.
- Unsupported equality cases fail during type checking.
- The helper is reusable by later phases.

---

## Phase 2 — Unit choice equality hardening

### Summary

Ensure unit choice equality is fully supported and tested.

### Why

Unit equality is the simplest case and should be rock solid before payload equality is added.

### Behavior

- [ ] Same-type unit choices compare by variant identity.
- [ ] Different variants of the same choice compare false.
- [ ] Same variants compare true.
- [ ] Different choice types are rejected.
- [ ] Imported and aliased choice variants compare correctly.
- [ ] Equality works in if conditions, const contexts only if normal equality is allowed there, and function returns.

### Tests

Add or confirm:

```text
choice_unit_equality_same_variant_true
choice_unit_equality_different_variant_false
choice_unit_equality_different_choice_type_rejected
choice_unit_equality_imported_variant_success
choice_unit_equality_aliased_import_success
choice_unit_equality_in_function_return
```

### Matrix/docs updates

- [ ] Update matrix only if current unit equality support status is inaccurate.
- [ ] Add docs examples if missing.

### Audit / style review

- [ ] Avoid JS-backend-only special cases if HIR can represent equality generically.
- [ ] Keep type checking separate from backend carrier shape.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Unit choice equality works through JS/HTML.
- Cross-type choice equality is rejected.
- Tests cover imports/aliases.

---

## Phase 3 — Payload structural equality type checking

### Summary

Allow equality on payload choices when all payload fields support equality.

### Why

This phase changes the frontend contract. It should not depend on backend lowering details yet.

### Behavior

- [ ] Equality between same-type payload choice values type-checks if payload fields are equality-supported.
- [ ] Unit and payload variants in the same choice can be compared.
- [ ] Different variants of the same choice are comparable and evaluate false.
- [ ] Same variant compares payload fields structurally.
- [ ] Payload field order follows declaration order.
- [ ] Payload field names remain useful for diagnostics.
- [ ] Unsupported payload fields reject the equality expression.

### Internal implementation

- [ ] Extend equality operator validation for choice types.
- [ ] Reuse the structural equality support helper from Phase 1.
- [ ] Preserve existing equality behavior for primitive types.
- [ ] Do not add custom equality overloads.

### Tests

Add:

```text
choice_payload_equality_same_payload_true_typechecks
choice_payload_equality_different_payload_typechecks
choice_payload_equality_unit_vs_payload_typechecks
choice_payload_equality_same_choice_different_variants_typechecks
choice_payload_equality_nested_supported_choice_typechecks
choice_payload_equality_unsupported_field_rejected
```

### Matrix/docs updates

- [ ] Update matrix to say payload structural equality type-checking is supported when all fields are equality-supported.
- [ ] Document unsupported field diagnostics.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Frontend accepts supported payload choice equality.
- Frontend rejects unsupported payload choice equality.
- No backend lowering changes are required to pass type-only tests unless existing tests execute code.

---

## Phase 4 — JS lowering for structural equality

### Summary

Lower choice structural equality to correct JS.

### Why

After type checking accepts payload equality, JS lowering must compare carrier shapes correctly.

### Behavior

- [ ] Compare choice type/variant identity first.
- [ ] If variants differ, result is false.
- [ ] If variants match and there is no payload, result is true.
- [ ] If variants match and payload exists, compare all payload fields structurally.
- [ ] Nested supported choice payloads compare recursively.
- [ ] Unsupported payload equality should already be rejected before lowering.
- [ ] Generated JS should avoid evaluating either side more than once.

### Implementation direction

Prefer a backend helper function if inline JS would duplicate logic or risk repeated evaluation. A helper is acceptable for Alpha if it is clear and tested.

Potential shape:

```js
function __bst_choice_eq(left, right) { ... }
```

But do not hardcode assumptions until the current carrier representation is audited.

### Tests

Add runtime/output tests:

```text
choice_payload_equality_same_payload_true
choice_payload_equality_different_payload_false
choice_payload_equality_different_variant_false
choice_payload_equality_unit_vs_payload_false
choice_payload_equality_nested_choice_true
choice_payload_equality_nested_choice_false
choice_payload_equality_side_effects_evaluated_once
```

### Matrix/docs updates

- [ ] Update matrix runtime target to JS/HTML support.
- [ ] Document any backend limitations.

### Audit / style review

- [ ] Do not scatter JS structural-equality fragments across unrelated lowerers.
- [ ] Preserve evaluation order.
- [ ] Keep helper naming internal and stable.
- [ ] No user-input panics.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Payload structural equality runs correctly in JS/HTML.
- Nested supported choice equality works.
- Generated code does not double-evaluate expressions.

---

## Phase 5 — Payload immutability hardening

### Summary

Make payload field immutability explicit and tested.

### Why

Payload fields remain immutable by language policy. Structural equality should not introduce hidden mutable payload handling or payload-place semantics.

### Behavior

- [ ] Payload fields are immutable after construction.
- [ ] Payload field mutation is rejected.
- [ ] Payload extraction through pattern matching binds immutable values unless explicitly copied/moved by existing language rules.
- [ ] Direct payload field access remains deferred/rejected.
- [ ] Structural equality does not expose payload mutation or field-place APIs.

### Tests

Add or confirm:

```text
choice_payload_field_mutation_rejected
choice_payload_extraction_binding_immutable
choice_payload_direct_field_access_rejected
choice_payload_structural_equality_does_not_allow_access
```

### Matrix/docs updates

- [ ] Update docs to say payload fields are immutable.
- [ ] Update matrix to say direct payload field access remains deferred.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Payload immutability is enforced.
- Direct payload field access remains rejected.
- Docs and matrix match policy.

---

## Phase 6 — Constructor and import regression pass

### Summary

Ensure structural equality works with existing constructor/import surfaces.

### Why

Choices interact heavily with module imports, aliases, constructor calls, and payload syntax. Equality must not regress these areas.

### Behavior

- [ ] Positional payload constructors still work.
- [ ] Named payload constructors still work.
- [ ] Imported choice constructors work in equality expressions.
- [ ] Aliased imports work in equality expressions.
- [ ] Re-exported choices work in equality expressions once re-export support exists.
- [ ] Constructor misuse diagnostics remain clear.

### Tests

Add:

```text
choice_equality_positional_constructor_success
choice_equality_named_constructor_success
choice_equality_imported_constructor_success
choice_equality_aliased_constructor_success
choice_equality_reexported_constructor_success_or_deferred
choice_equality_constructor_wrong_arity_rejected
choice_equality_constructor_wrong_field_rejected
```

### Matrix/docs updates

- [ ] Update matrix if import/re-export coverage changes.
- [ ] Add docs example for comparing constructed payload choices.

### Audit / style review

- [ ] Avoid duplicate constructor-resolution code.
- [ ] Equality should consume resolved choice type/variant IDs, not string matching where IDs exist.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Constructor/import behavior does not regress.
- Equality uses canonical resolved choice identity.

---

## Phase 7 — Diagnostics hardening for choices

### Summary

Tighten diagnostics for invalid choice/equality forms.

### Why

Structural equality will expose more invalid cases. Diagnostics need to distinguish type mismatch, unsupported field equality, constructor misuse, direct field access, and deferred features.

### Diagnostics to cover

- [ ] different choice types compared
- [ ] payload field has unsupported equality type
- [ ] recursive/generic choice equality deferred
- [ ] direct payload field access deferred
- [ ] payload mutation rejected
- [ ] nested payload pattern deferred
- [ ] constructor wrong arity
- [ ] constructor wrong named field
- [ ] equality with non-choice unsupported type

### Tests

Add targeted failure cases where missing:

```text
choice_diagnostic_different_choice_types
choice_diagnostic_unsupported_payload_equality
choice_diagnostic_direct_payload_access_deferred
choice_diagnostic_payload_mutation_rejected
choice_diagnostic_nested_payload_pattern_deferred
choice_diagnostic_constructor_wrong_arity
choice_diagnostic_constructor_wrong_named_field
```

### Matrix/docs updates

- [ ] Update matrix only if status changes.
- [ ] Add docs notes for common unsupported surfaces.

### Audit / style review

- [ ] Diagnostics should identify the choice type, variant, and field where practical.
- [ ] Diagnostics must use source locations.
- [ ] No user-input panics.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Invalid choice equality failures are clear.
- Deferred choice features are rejected with explicit diagnostics.

---

## Phase 8 — Final docs and matrix reconciliation

### Summary

Make docs and implementation matrix match final Choice behavior.

### Required matrix wording

In `docs/src/docs/progress/#page.bst`, the Choices row should say:

```text
Unit variants, payload variants, constructor calls, imports, assignment, return, JS carrier shape, payload matching, and structural equality are supported. Structural equality compares variant identity and payload fields recursively when every payload field supports equality. Payload fields are immutable. Payload shorthand, generic choices, recursive choices, nested payload patterns, direct payload field access, and variant defaults remain deferred or rejected with diagnostics.
```

### Docs updates

Update `docs/language-overview.md` with examples for:

- unit choice declaration
- payload choice declaration
- positional constructor
- named constructor
- unit equality
- payload structural equality
- unsupported payload equality diagnostic
- payload immutability
- direct payload field access deferral

### Manifest audit

- [ ] Ensure every new test is listed in `tests/cases/manifest.toml`.
- [ ] Use consistent tags:
  - `choices`
  - `equality`
  - `diagnostics`
  - `imports`
  - `js-backend`
  - `pattern-matching` where relevant

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Matrix and docs accurately describe the implemented choice surface.
- Structural equality is documented.
- Payload immutability is documented.
- Deferred surfaces are explicit.

---

## Recommended commit sequence

1. `frontend: audit choice implementation surface`
2. `frontend: define choice structural equality contract`
3. `frontend: harden unit choice equality`
4. `frontend: typecheck payload structural equality`
5. `js: lower choice structural equality`
6. `frontend: enforce choice payload immutability`
7. `frontend: cover choice equality constructors and imports`
8. `frontend: improve choice diagnostics`
9. `docs: reconcile choice matrix and docs`
