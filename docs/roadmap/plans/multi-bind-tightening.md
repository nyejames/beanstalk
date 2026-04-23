# Plan: Tighten Multi-Bind to Explicit Multi-Return Surfaces

## Context

Multi-bind is currently implemented as a dedicated AST/HIR statement path rather than as part of normal declaration syntax.
That split is good and should remain.

The surface now needs to be tightened:

- multi-bind should **not** be supported for regular declarations
- multi-bind should remain supported for **function-call multiple returns**
- the compiler and docs should describe this as a **special-purpose surface**, not a general destructuring feature
- future expansion can happen later for other explicit expression-block surfaces such as pattern matching, but that is **not part of this plan**

This plan should be implemented **after the template hardening plan**.

## Goals

- Restrict multi-bind so it is no longer treated as a generic right-hand-side destructuring mechanism.
- Keep the current dedicated AST/HIR lowering path where it is still justified.
- Tighten diagnostics so the language rule is explicit and readable.
- Update docs so the language contract matches the implementation.
- Leave a clean future extension point for other explicit multi-value expression blocks.

## Non-goals

This plan does **not**:

- add pattern-match block multi-bind support
- add generic tuple / pack destructuring
- extend regular declaration syntax
- redesign HIR lowering for multi-bind
- add compatibility shims for older syntax behavior

## Desired language rule after this plan

Multi-bind is valid only when the right-hand side is an **explicit compiler-supported multi-value surface**.

For now, that means:

- **supported:** multi-return function calls
- **not supported:** regular declarations with arbitrary expressions
- **not supported:** generic expressions that merely happen to evaluate to multiple values
- **not supported yet:** future expression blocks such as pattern matching

Examples:

```beanstalk
pair || -> String, Int:
    return "Ana", 2
;

name, count = pair()
```

Valid.

```beanstalk
a, b = value
```

Invalid.

```beanstalk
a, b = some_expression_block
```

Invalid for now unless that surface is explicitly added later.

## Compiler areas affected

### AST statement parsing

Primary tightening point:

- `src/compiler_frontend/ast/statements/multi_bind.rs`

This module should stop describing multi-bind as a generic multi-value RHS form.
It should validate that the right-hand side belongs to an explicitly supported multi-bind-producing surface.

Initial supported surface:

- plain multi-return function calls

The implementation should be structured so that additional explicit surfaces can be added later without reopening regular declaration syntax.

### Symbol-led statement dispatch

- `src/compiler_frontend/ast/statements/body_symbol.rs`

This should continue routing comma-led symbol statements into the dedicated multi-bind parser before normal declaration parsing.
No major structural change should be needed here.

### Shared declaration syntax

- `src/compiler_frontend/declaration_syntax/declaration_shell.rs`
- `src/compiler_frontend/ast/statements/declarations.rs`

These files should stay single-target oriented.
No multi-bind behavior should be added here.

This is an important part of the simplification:
regular declaration syntax remains narrow and easy to reason about.

### AST node shape

- `src/compiler_frontend/ast/ast_nodes.rs`

`NodeKind::MultiBind` can remain.
No redesign is needed as long as the AST invariant becomes stricter:
this node should only exist for explicitly supported multi-bind-producing surfaces.

### HIR lowering

- `src/compiler_frontend/hir/hir_statement.rs`

HIR lowering can remain mostly unchanged.
It already lowers multi-bind by evaluating the RHS once and projecting slots in order.

The main change here is to tighten comments and invariants so the lowering clearly assumes an AST-validated multi-bind source rather than a generic destructuring expression.

## Implementation steps

## Part 1 — Tighten the AST rule

In `src/compiler_frontend/ast/statements/multi_bind.rs`:

- keep the dedicated target-list parsing
- keep target arity / target-name validation
- keep new-declaration vs existing-assignment target resolution
- replace the broad RHS acceptance rule with an explicit supported-surface check

The parser should:

1. parse exactly one RHS expression
2. classify whether that expression belongs to a supported multi-bind-producing surface
3. reject unsupported surfaces with a clear rule error
4. only then extract slot types and continue target resolution

## Part 2 — Introduce an explicit support boundary

The multi-bind parser should be organized around an explicit helper such as:

- `is_supported_multi_bind_rhs(...)`
- or `classify_multi_bind_rhs(...)`

This should make the current rule obvious and make future extension straightforward.

Initial classification should support:

- multi-return function calls

It should reject:

- variables
- literals
- field access
- arbitrary runtime expressions
- generic expressions that only happen to carry `Returns(...)`
- future expression blocks until they are explicitly added

## Part 3 — Tighten diagnostics

Diagnostics should stop talking about a generic “multi-value return pack” as the user-facing rule.
That is an implementation detail.

User-facing errors should instead say that:

- multi-bind is only supported for explicit multi-value surfaces
- for now, that means multi-return function calls

Suggested diagnostic shape:

- state that the RHS is not a supported multi-bind source
- state that regular declarations do not support multi-bind
- point toward using a multi-return function call instead

## Part 4 — Preserve future extension points cleanly

This plan should leave the code ready for later support of other explicit surfaces such as:

- pattern matching blocks
- other future expression blocks that intentionally produce multiple bindable values

That future work should happen by extending the supported multi-bind RHS classifier.
It should **not** happen by broadening regular declaration syntax.

## Part 5 — Update tests

### Parser / AST tests

Add or update failures for cases such as:

- `a, b = value`
- `a, b = 1`
- `a, b = thing.field`
- `a, b = [template]`
- any other arbitrary expression that currently reaches multi-bind parsing

Keep success coverage for:

- `a, b = pair()`
- reassignment targets where allowed
- arity mismatch
- malformed target lists

### HIR tests

Keep the existing call-based success coverage.

If needed, update comments/assertion names so they reflect the narrower rule:
HIR is validating lowering for call-result multi-bind, not generic destructuring.

### Integration tests

Review canonical `multi_bind_*` and `result_multi_bind_*` fixtures.

Make sure they only cover the intended supported surface.
Any fixture that implies generic RHS destructuring should be removed or rewritten.

## Part 6 — Update documentation

### Language docs

Update `docs/language-overview.md` so it explicitly says:

- multiple success values can be assigned through multi-bind
- this is currently intended for multi-return call results
- regular declarations remain single-target
- other multi-value expression blocks may be added later, but are not supported yet

### Compiler design docs

Update `docs/compiler-design-overview.md` so multi-bind is described as:

- an AST statement form
- currently limited to explicit multi-return-producing surfaces
- not part of general declaration syntax

### Surface / roadmap docs

Update any roadmap or matrix docs that currently describe multi-bind too broadly.
The supported surface should match the narrowed implementation exactly.

## Suggested file list

Primary implementation files:

- `src/compiler_frontend/ast/statements/multi_bind.rs`
- `src/compiler_frontend/ast/statements/body_symbol.rs`
- `src/compiler_frontend/hir/hir_statement.rs`
- `src/compiler_frontend/ast/tests/parser_error_recovery_tests.rs`
- `src/compiler_frontend/hir/tests/hir_result_lowering_tests.rs`
- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/roadmap/language-surface-integration-matrix.md`

## Acceptance criteria

This plan is complete when:

- multi-bind no longer works for regular declarations or arbitrary RHS expressions
- multi-bind still works for multi-return function calls
- the AST contains a clear, explicit supported-surface check for multi-bind RHS
- HIR lowering still evaluates the RHS once and lowers slot projection correctly
- diagnostics describe the actual language rule clearly
- docs no longer imply that multi-bind is a general destructuring feature
- the implementation leaves a clean future extension point for pattern-matching or other expression-block multi-bind surfaces without widening declaration syntax

## Design notes

The important simplification here is not removing the dedicated multi-bind path.
It is narrowing what that path means.

Multi-bind should remain a small, readable, special-purpose construct.
It should not become a second declaration system or a generic destructuring mechanism.

Future support for pattern matching or other multi-value expression blocks should be added deliberately as explicit language surfaces, one by one.