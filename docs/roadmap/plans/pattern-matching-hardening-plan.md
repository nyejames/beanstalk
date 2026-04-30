# Pattern Matching Hardening Implementation Plan

## Purpose

This plan hardens Beanstalk pattern matching and extends it with capture patterns.

It is written as an agent-executable phase plan. Each phase must leave the repo in a validated state and must run:

```sh
just validate
```

Each phase must update the implementation matrix and docs when behavior changes.

## Scope

Included:

- clearer pattern-matching implementation boundaries
- wildcard rejection as permanent language policy
- unreachable arm warnings
- relational string patterns
- capture patterns
- improved diagnostics and tests
- docs and matrix reconciliation

Deferred:

- nested choice payload patterns
- full relational overlap analysis
- full exhaustiveness proofs beyond the current Alpha surface
- destructuring patterns beyond the specific capture work in this plan

Policy decisions:

- `case _ =>` is rejected forever.
- Relational patterns support `String` as well as ordered scalar types.
- General capture patterns are now part of this plan.
- Nested choice payload patterns remain deferred.
- Overlapping relational pattern analysis is not worth doing now.
- Unreachable arms should produce warnings, not hard errors.

## Required references

Before any phase, read:

- `AGENTS.md`
- `docs/codebase-style-guide.md`
- `docs/compiler-design-overview.md`
- `docs/language-overview.md`
- `docs/src/docs/progress/#page.bst`
- `tests/cases/manifest.toml`

Likely implementation areas:

- `src/compiler_frontend/declaration_syntax/`
- `src/compiler_frontend/ast/`
- `src/compiler_frontend/ast/expressions/`
- `src/compiler_frontend/hir/`
- `src/compiler_frontend/compiler_warnings.rs`
- `src/compiler_frontend/compiler_errors.rs`
- JS backend lowering paths for match/control-flow output
- integration test cases under `tests/cases/`

Exact files may differ. Search the repo before editing.

---

## Phase 0 — Pattern matching implementation audit

### Summary

Audit the current parser, AST, HIR, diagnostics, warnings, and JS lowering for pattern matching.

### Why

Pattern matching already has partial support. Before adding capture patterns or warnings, the agent needs a precise map of current behavior and test coverage.

### Implementation steps

- [ ] Locate the parser for `if value is:` / match-style syntax.
- [ ] Locate AST representation for literal, choice, relational, guard, and else arms.
- [ ] Locate HIR representation and JS lowering for match arms.
- [ ] Locate current diagnostics for invalid pattern syntax.
- [ ] Locate current tests and manifest entries.
- [ ] Write a short implementation note in this plan or a local audit comment identifying the exact owner modules.
- [ ] Do not change language behavior in this phase unless fixing obvious docs drift.

### Matrix/docs updates

- [ ] Update `docs/src/docs/progress/#page.bst` only if the current matrix is inaccurate.
- [ ] Keep any docs changes factual and small.

### Audit / style review

- [ ] No behavior-changing refactors without tests.
- [ ] No new helper modules unless duplication is found.
- [ ] Identify any user-input `panic!`, `todo!`, `unwrap`, or `expect` paths for later cleanup.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- The implementation owner files are identified.
- Existing pattern tests are understood.
- No speculative implementation starts before the audit is complete.

---

## Phase 1 — Permanent wildcard rejection

### Summary

Make wildcard pattern rejection explicit, tested, and documented.

### Why

Beanstalk should use `else =>` for catch-all arms. `case _ =>` should not become a second spelling for the same concept.

### Behavior

- [ ] `case _ =>` is rejected.
- [ ] `_` should not be treated as a capture binding.
- [ ] Diagnostics should say to use `else =>`.
- [ ] The rejection should apply in all pattern positions, including future capture-pattern parsing paths.

### Diagnostics

Suggested message:

```text
Wildcard pattern '_' is not supported in Beanstalk. Use 'else =>' for a catch-all arm.
```

### Tests

Add or confirm:

```text
pattern_wildcard_rejected
pattern_wildcard_with_guard_rejected
pattern_wildcard_after_choice_rejected
pattern_wildcard_not_capture_binding
```

### Matrix/docs updates

- [ ] Update `docs/language-overview.md` pattern matching section.
- [ ] Update `docs/src/docs/progress/#page.bst` to state wildcard arms are permanently rejected and `else =>` is the supported catch-all.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- `_` cannot silently parse as a capture or ignored binding.
- Error points at the wildcard token.
- Docs and matrix match implementation.

---

## Phase 2 — Relational string patterns

### Summary

Extend relational patterns to support `String` comparisons.

### Why

Relational patterns already cover ordered scalar cases. Strings should be accepted as ordered values too, with backend-defined ordering for Alpha unless the docs define a stricter semantic contract.

### Behavior

- [ ] `case < "m" =>` works when the scrutinee is `String`.
- [ ] `case <= "m" =>` works for `String`.
- [ ] `case > "m" =>` works for `String`.
- [ ] `case >= "m" =>` works for `String`.
- [ ] Mixed relational pattern types are rejected.
- [ ] Non-ordered types remain rejected.
- [ ] Choice, struct, collection, and Bool relational patterns are rejected.

### Ordering policy

For Alpha, document string ordering as backend-defined unless the implementation already has a central comparison contract. For the JS backend this will likely match JavaScript string comparison behavior.

### Tests

Add:

```text
pattern_relational_string_less_success
pattern_relational_string_less_equal_success
pattern_relational_string_greater_success
pattern_relational_string_greater_equal_success
pattern_relational_string_mixed_number_rejected
pattern_relational_string_bool_rejected
pattern_relational_choice_rejected
pattern_relational_struct_rejected
pattern_relational_collection_rejected
```

### Matrix/docs updates

- [ ] Update `docs/language-overview.md` to include string relational patterns.
- [ ] State whether string ordering is backend-defined for Alpha.
- [ ] Update `docs/src/docs/progress/#page.bst` pattern matching row.

### Audit / style review

- [ ] Keep relational type validation in one place.
- [ ] Do not special-case JS lowering if the existing comparison operator lowering can handle strings.
- [ ] Avoid broad comparison semantics changes outside pattern matching unless required.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- String relational patterns compile and run through HTML/JS.
- Invalid relational scrutinees fail with structured diagnostics.
- Docs describe the ordering policy honestly.

---

## Phase 3 — Capture pattern syntax and AST/HIR representation

### Summary

Add general capture patterns.

### Why

Capture patterns are an important ergonomic gap. They allow binding the matched value into the arm scope without requiring nested payload destructuring.

### Intended syntax

```beanstalk
if value is:
    case captured =>
        io(captured)
    else =>
        io("fallback")
;
```

This binds the entire scrutinee value to `captured` for that arm.

### Behavior

- [ ] A bare symbol pattern that is not a known literal/choice constructor is parsed as a capture pattern.
- [ ] Capture binding is scoped only to the arm body.
- [ ] Capture binding has the scrutinee type.
- [ ] Capture binding is immutable.
- [ ] Capture name participates in normal local binding collision rules.
- [ ] Capture pattern should not be allowed to shadow an existing local in the same arm scope unless normal scope rules permit that.
- [ ] `case _ =>` remains rejected and never becomes a capture pattern.
- [ ] Capture patterns with guards are supported if guards already exist.
- [ ] Capture patterns do not imply narrowing or nested destructuring.

### Parser/representation work

- [ ] Add a distinct AST/HIR pattern variant for capture patterns.
- [ ] Do not lower capture as `else` with a synthetic assignment if that would bypass diagnostics/scope rules.
- [ ] Keep capture representation explicit enough for future exhaustiveness and warnings.

### Tests

Add:

```text
pattern_capture_binds_scrutinee_success
pattern_capture_arm_scope_only
pattern_capture_guard_success
pattern_capture_immutable_binding
pattern_capture_rejects_underscore
pattern_capture_name_collision_rejected
pattern_capture_after_literal_warning_or_reachable
pattern_capture_before_else_makes_else_unreachable_warning
```

### Matrix/docs updates

- [ ] Update `docs/language-overview.md` with capture pattern syntax.
- [ ] Update `docs/src/docs/progress/#page.bst` to mark capture patterns as supported.
- [ ] Explicitly state nested payload patterns remain deferred.

### Audit / style review

- [ ] Capture binding should reuse existing local variable/scope machinery where possible.
- [ ] Avoid duplicating arm-scope setup logic.
- [ ] Ensure diagnostics point to the capture name.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Capture patterns work in JS/HTML builds.
- Capture bindings are correctly scoped and typed.
- `_` remains rejected.
- Docs and matrix are updated.

---

## Phase 4 — Unreachable arm warnings

### Summary

Add warnings for unreachable match arms.

### Why

Unreachable arms should help users catch mistakes without blocking compilation. This should be warning-level, not error-level.

### Scope

Implement obvious unreachable cases only. Do **not** implement full relational interval analysis now.

### Warning cases to support

- [ ] Any arm after `else =>` is unreachable.
- [ ] Any arm after an unconditional capture pattern is unreachable.
- [ ] Duplicate literal pattern arms are unreachable after the first.
- [ ] Duplicate unit choice variant arms are unreachable after the first.
- [ ] Duplicate same relational pattern may warn if easy, but full overlap analysis is deferred.

### Explicitly deferred

Do not warn for partial relational overlap such as:

```beanstalk
if value is:
    case < 10 => ...
    case < 5 => ...
;
```

This is intentionally not worth doing now.

### Warning wording

Suggested:

```text
This pattern arm is unreachable because an earlier arm already matches this case.
```

For after `else`:

```text
This pattern arm is unreachable because 'else =>' must be the final arm.
```

### Tests

Add:

```text
pattern_unreachable_after_else_warns
pattern_unreachable_after_capture_warns
pattern_duplicate_literal_warns
pattern_duplicate_unit_choice_warns
pattern_relational_overlap_no_warning
pattern_unreachable_warnings_forbid_fails
pattern_unreachable_warnings_allow_success
```

### Matrix/docs updates

- [ ] Update pattern matching matrix row to say obvious unreachable arms warn.
- [ ] Document that full relational overlap analysis is deferred.

### Audit / style review

- [ ] Warnings should use existing compiler warning infrastructure.
- [ ] Do not turn warnings into hard errors unless test mode forbids warnings.
- [ ] Avoid adding a heavy reachability solver.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Obvious unreachable arms warn.
- Warning-forbid integration tests fail as expected.
- Relational overlap is intentionally not warned.

---

## Phase 5 — Capture patterns with choice payloads, without nested payloads

### Summary

Ensure capture patterns interact cleanly with existing choice payload matching without adding nested payload destructuring.

### Why

Capture patterns should work alongside choices but should not accidentally implement the deferred nested payload feature.

### Behavior

- [ ] Capture pattern can bind the whole choice value.
- [ ] Existing choice payload extraction syntax continues to work.
- [ ] Existing `field as local_name` payload aliases continue to work.
- [ ] Nested choice payload patterns remain rejected with a clear diagnostic.
- [ ] Capture inside a payload pattern is only supported if it is already part of the documented payload alias syntax.

### Tests

Add:

```text
pattern_capture_whole_choice_value_success
pattern_choice_payload_existing_capture_alias_success
pattern_nested_choice_payload_pattern_rejected
pattern_capture_does_not_enable_nested_payload
```

### Matrix/docs updates

- [ ] Update docs to clearly separate general capture patterns from choice payload extraction.
- [ ] Update matrix: capture patterns supported; nested choice payload patterns deferred.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Choice matching does not regress.
- Nested payload patterns remain rejected.
- Capture patterns are not confused with payload destructuring.

---

## Phase 6 — Diagnostics hardening

### Summary

Tighten diagnostics for invalid pattern forms.

### Why

Pattern matching touches many syntax forms. Bad diagnostics here make the language feel unstable.

### Diagnostics to audit

- [ ] wildcard pattern rejected
- [ ] relational pattern on unsupported type
- [ ] relational pattern with mismatched literal type
- [ ] capture name collision
- [ ] capture binding mutation attempt
- [ ] nested choice payload pattern deferred
- [ ] malformed guard
- [ ] `else` not final
- [ ] duplicate/unreachable arm warning

### Tests

Add targeted failure cases where missing:

```text
pattern_diagnostic_invalid_relational_type
pattern_diagnostic_capture_collision
pattern_diagnostic_nested_payload_deferred
pattern_diagnostic_else_not_final_or_warned
pattern_diagnostic_wildcard_rejected
```

### Matrix/docs updates

- [ ] Update matrix only if support status changes.
- [ ] Update docs examples if diagnostics reveal misleading examples.

### Audit / style review

- [ ] No user-input `panic!`, `todo!`, `.unwrap()`, or `.expect()` path.
- [ ] Diagnostics should include source locations.
- [ ] Suggestions should be concrete.

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Common invalid pattern forms produce clear diagnostics.
- Tests cover the important failure modes.

---

## Phase 7 — Final docs and matrix reconciliation

### Summary

Make the docs and implementation matrix match final behavior.

### Required matrix wording

In `docs/src/docs/progress/#page.bst`, the pattern matching row should say:

```text
Literal patterns, choice variant patterns, payload extraction, relational patterns including strings, guards, else arms, and general capture patterns are supported. Wildcard patterns using '_' are permanently rejected; use else => instead. Obvious unreachable arms warn. Full relational overlap analysis and nested choice payload patterns remain deferred.
```

### Docs updates

Update `docs/language-overview.md` with examples for:

- literal arms
- choice arms
- relational string arms
- capture arms
- guards
- `else =>`
- wildcard rejection
- nested payload deferral

### Manifest audit

- [ ] Ensure every new test is listed in `tests/cases/manifest.toml`.
- [ ] Use consistent tags:
  - `pattern-matching`
  - `diagnostics`
  - `warnings`
  - `choices` where relevant
  - `js-backend` where output is asserted

### Validation

- [ ] Run `just validate`.

### Acceptance criteria

- Matrix and docs accurately describe the implemented pattern surface.
- No stale docs claim wildcard support or omit capture patterns.
- Deferred nested payload patterns are explicit.

---

## Recommended commit sequence

1. `frontend: audit pattern matching implementation surface`
2. `frontend: permanently reject wildcard patterns`
3. `frontend: support string relational patterns`
4. `frontend: add capture pattern representation`
5. `frontend: warn for obvious unreachable pattern arms`
6. `frontend: harden capture and choice pattern interactions`
7. `frontend: improve pattern diagnostics`
8. `docs: reconcile pattern matching matrix and docs`
