# Beanstalk Compiler Diagnostics Improvement Plan

## Purpose

Improve Beanstalk diagnostics without teaching invalid syntax, legacy behaviour or patterns that fight the language design.

Every retained item in this plan has been checked against the current language and compiler contracts. Several findings from the original audit have been corrected, consolidated or removed because their proposed fixes were stale, ambiguous or semantically wrong.

This is an implementation plan, not a research backlog. Each phase should leave one current diagnostic path, remove superseded variants and add end-to-end coverage for the user-visible behaviour it changes.

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/compiler-diagnostics-improvement-plan.md`
STATUS: active
CURRENT_SLICE: Phase 1.2a, carry exact operator facts and consolidate operator diagnostics
LAST_ACCEPTED_COMMIT: none
WORKTREE: `main` at `069a29acb`, Phase 1.1 accepted and awaiting checkpoint commit
REQUIRED_RELOADS: startup files, this plan and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: compiler diagnostics ownership, generic and trait bounds, operator semantics, style, testing and validation contracts
- code: exact AST `Operator`, operator-policy emitters, diagnostic payload/remapping and operator renderers/tests
ACCEPTANCE_CRITERIA:
- unsupported-operator diagnostics carry and render the exact source operator
- `not`, mixed String `+` and generic-parameter failures use the Phase 1.2 wording
- no user-facing message claims trait bounds are unsupported
VALIDATION_STATE:
- targeted compiler-message tests: passed, 36 tests
- full Rust tests and Clippy: passed in the Phase 1.1 Ollama worker
- `just validate`: passed for Phase 1.1, including 3,511 Rust tests and 1,764 integration cases
DOCS_IMPACT: plan state only; progress matrix unaffected by Phase 1.1
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: Ollama - explicitly required for every implementation worker slice
NEXT_WORKER_ORDER: Ollama only for implementation slices
STOP_REASON: none
NEXT_RESUME_ACTION: run `just validate`, commit Phase 1.1 and launch the Phase 1.2a Ollama worker

## Confirmed design decisions

- Quoted strings support exactly these escapes:
  - `\\`
  - `\"`
  - `\n`
  - `\r`
  - `\t`
- Every other quoted-string escape is rejected.
- Raw backtick strings do not process escapes. Backslashes and newlines remain literal.
- Legacy `#import` recognition, diagnostics, tests and documentation references are removed completely. It receives no migration-specific compatibility diagnostic.
- Incompatible template directives are rejected.
- A directive-conflict diagnostic must name both conflicting items and explain the semantic reason they cannot be combined.
- Existing mutable places require explicit `~place` only at mutable call and receiver-call boundaries.
- A binding must already be declared mutable before it can be reassigned or written through.
- Reassignment and field assignment use ordinary `=`. `~` is not written on assignment targets.
- Fresh values passed to mutable parameters remain valid without source `~`.
- External package namespace access and grouped imports are both valid for direct package exports. Diagnostics must not claim otherwise.
- Error handling uses Beanstalk's `Error!`, postfix `!` and `catch` terminology. User-facing diagnostics must not describe the source feature as a first-class `Result`.
- Multi-return calls cannot be received by a single declaration or assignment target. Beanstalk has no implicit dropped return slot and no documented discard syntax.

## Required implementation rules

- Emit user mistakes as typed `CompilerDiagnostic` values. Do not route malformed source through `CompilerError` or `BST-INFRA-0001`.
- Keep diagnostic payloads factual. Carry names, `TypeId`s, operators, counts, locations and reason enums rather than pre-rendered prose.
- Render type names through `DiagnosticRenderContext`.
- Reuse an existing stable code when the diagnostic category and contract remain the same. Add a new code only for a genuinely new diagnostic family.
- Preserve the source location that identifies the actual mistake. Add a secondary label for the related declaration, delimiter, conflicting directive or active access when available.
- Do not generate exact replacement source unless the compiler has enough structured facts to make it valid. Prefer a generic correction over a fabricated snippet.
- Keep suggestions source-visible. Do not mention internal AST, HIR, parameter receiver placeholders, borrow lattice states or compiler-introduced locals.
- Suggestions must favour normal Beanstalk patterns:
  - explicit mutable declarations followed by ordinary assignment
  - explicit `~place` at mutable call boundaries
  - templates for mixed textual interpolation
  - choices for runtime heterogeneity
  - concrete types or bound-provided receiver methods for generic behaviour
  - grouped imports or namespace qualification according to the actual visible package surface
- Do not preserve obsolete diagnostics for removed syntax.
- Prefer one primary integration case per user-visible contract. Add unit tests only for renderer, tokenizer, scanner or compatibility invariants that integration output cannot isolate.

## Phase 1: Diagnostic render and taxonomy integrity

### 1.1 Guarantee non-empty terse messages

**Original findings:** DIAG-013, DIAG-027
**Status:** Complete, pending checkpoint commit

`render::terse` currently emits only `RenderedPayload.message`. Payloads such as `DiagnosticPayload::None` can therefore produce an empty final field even when the diagnostic descriptor has a useful title.

#### Implementation

- Add one shared helper at the render boundary that returns:
  1. the rendered payload message when non-empty
  2. otherwise the diagnostic descriptor title
- Use it in the terse renderer.
- Do not duplicate fallback logic in individual diagnostic kinds.
- Keep terminal and dev-server formatting unchanged unless their equivalent output can also become empty.
- Add a focused unit test for representative syntax and rule diagnostics with empty payload messages.
- Add an invariant test that the terse result always has a non-empty message field.

#### Acceptance

- No terse diagnostic ends after the final `|`.
- `BST-RULE-0077`, `BST-SYNTAX-0006`, `BST-SYNTAX-0009` and other descriptor-only diagnostics render useful text.
- The stable code, path, line and column format does not change.

### 1.2 Remove stale source terminology and misleading guidance

**Original findings consolidated:** DIAG-015, DIAG-025, DIAG-030, DIAG-043, DIAG-044
**Additional confirmed drift:** generic operator guidance, standalone function-body templates, optional-type suggestion and statement-position `type`/`of` wording

Audit all messages touched by this plan for source terminology that no longer matches Beanstalk.

Implement this in two coherent slices:

1. **1.2a exact operator facts:** replace the broad operator-category payload with the exact source operator and complete the operator wording formerly listed in Phase 7.2. This must land first because the generic message cannot name `<op>` from `UnsupportedOperatorCategory`.
2. **1.2b remaining terminology:** correct fallible, optional, template and statement-position messages plus the source-hostile internal-stage wording listed below.

#### Required corrections

- Replace user-facing `Result` wording with `fallible expression`, `Error! return` or the exact source construct.
- Replace `Option` as a source type name with optional syntax such as `String?`.
- Change the stale generic-operator message that says trait bounds are unsupported. Trait bounds exist, but compiler-owned operators are not granted by generic bounds.
- Use this rule for generic operators:

  > Operator `<op>` is not available for generic parameter `<T>`. Beanstalk operators are compiler-owned and generic bounds do not provide operator support. Use a concrete type or a receiver method provided by an explicit bound.

- Change the standalone-template-in-function diagnostic. Templates are valid expressions inside functions. Only an unreceived standalone template is invalid:

  > A standalone template is not a valid statement here. Assign it, return it or pass it to a receiving expression.

- Change the compile-time `none` guidance from `Option<Type>` to a real optional annotation such as `value String? = none`.
- Change stale statement-position wording:
  - `type` is valid only in top-level generic declaration headers
  - `of` is valid only in generic type annotations
  - neither is a future reserved feature
- Change `InvalidResultHandlingReason::NotResultExpression`, which currently calls the operand a `Result`-valued expression, to source-visible `Error!` and fallible-expression terminology. Phase 4 still replaces the umbrella reason with authored-handler cases.
- Remove internal-stage wording from `InvalidConfigReason::ValueCouldNotFold`. Explain only that the value could not be evaluated at compile time and cannot depend on runtime evaluation.

#### Exact operator prerequisite and consolidated wording

- Add a diagnostic-owned exact operator enum that is independent of AST storage and can be reused by tokenizer spacing diagnostics in Phase 2.2.
- Map AST `Operator` to the diagnostic operator at the operator-policy emission boundary.
- Replace `UnsupportedOperatorTypes { category, ... }` with exact operator facts. Derive broad families only where generic fallback wording still needs them.
- Render exact operator messages in this slice:
  - `Operator 'not' requires a 'Bool' operand, found 'Int'.`
  - `Operator '+' cannot concatenate 'String' and 'Int'. Use a template for mixed textual interpolation.`
  - `Operator '<op>' is not available for generic parameter '<T>'. Beanstalk operators are compiler-owned and generic bounds do not provide operator support. Use a concrete type or a receiver method provided by an explicit bound.`
- Update payload remapping, constructors, AST operator-policy emitters and focused tests as one API replacement. Do not retain the category-only payload as a compatibility path.

#### Acceptance

- No user-facing message claims generic bounds are unimplemented.
- No user-facing message teaches `Option<Type>`.
- No user-facing message claims templates are top-level-only.
- No user-facing message calls an `Error!` expression a first-class `Result`.
- No user-facing message exposes AST construction or finalization as a correction concept.
- Repository search finds no stale wording after superseded variants are removed.

### 1.3 Remove legacy `#import` completely

**Original finding:** DIAG-039
**Decision:** Strict removal

#### Implementation

Remove:

- `RuleDiagnosticKind::LegacyImportSyntax`
- descriptor `BST-RULE-0025`
- its payload, constructor and renderer branches
- tokenizer or header recognition that exists only to identify `#import`
- dedicated fixtures and unit tests
- documentation and comments that describe `#import` as a legacy syntax
- generated documentation references through a normal docs rebuild, never direct edits under `docs/release/**`

Do not replace this with a new compatibility diagnostic. The remaining current grammar may reject the token sequence through its ordinary syntax rules.

#### Acceptance

A repository search for `LegacyImportSyntax`, `BST-RULE-0025` and source-facing ``#import`` migration prose returns no active implementation or documentation references.

## Phase 2: Tokenizer and structural syntax diagnostics

### 2.1 Implement the quoted-string escape contract

**Original finding:** DIAG-028
**Original DIAG-029 disposition:** Removed. Raw strings intentionally preserve backslashes and newlines.

The current quoted-string tokenizer discards `\` and accepts any following character. This does not implement a defined escape grammar.

#### Implementation

Add `SyntaxDiagnosticKind::InvalidStringEscape` with stable code `BST-SYNTAX-0034` and a structured reason:

```rust
enum InvalidStringEscapeReason {
    UnsupportedEscape { escaped: char },
    TrailingBackslash,
}
```

Update quoted-string tokenization to decode only:

| Source | Value |
|---|---|
| `\\` | backslash |
| `\"` | double quote |
| `\n` | newline |
| `\r` | carriage return |
| `\t` | tab |

Reject `\0`, `\x`, `\u`, `\q`, a backslash before a physical newline and every other escape.

The primary span should cover the backslash and escaped character where both exist. A trailing backslash should point at the backslash.

Raw backtick strings remain unchanged:

- no escape decoding
- no invalid-escape diagnostics
- physical newlines remain allowed
- backslashes remain literal

#### Tests

- Unit tests for every accepted escape and decoded value
- Unit tests for unsupported escapes and a trailing backslash
- Integration case proving invalid escapes use `BST-SYNTAX-0034`
- Positive raw-string cases containing `\n`, `\q`, backslashes and physical newlines

### 2.2 Make symbolic spacing diagnostics exact

**Original findings:** DIAG-004, DIAG-005

The tokenizer currently uses one `InvalidSymbolicBinaryOperatorSpacing` reason for binary operators and compound assignment. It does not carry the operator or missing side.

#### Implementation

Replace the umbrella reason with structured facts:

```rust
enum SymbolicSpacingConstruct {
    BinaryOperator,
    CompoundAssignment,
}

enum MissingWhitespace {
    Before,
    After,
    Both,
}

struct SymbolicSpacingError {
    construct: SymbolicSpacingConstruct,
    operator: DiagnosticOperator,
    missing: MissingWhitespace,
}
```

- Determine the complete token first, including `//=`.
- Record leading and trailing whitespace independently.
- Render the exact construct and side:
  - `Binary operator '+' requires whitespace after it.`
  - `Compound assignment '+=' requires whitespace before it.`
  - `Compound assignment '//=' requires whitespace on both sides.`
- Keep the correction example minimal and valid.
- Do not describe compound assignment as a binary operator.
- Preserve the existing `BST-SYNTAX-0031` family unless descriptor policy requires a dedicated compound-assignment code.

#### Tests

Cover every compound-assignment token at least once and cover before, after and both missing-side branches across the operator family. Avoid one fixture per cosmetic variant when a tokenizer table test protects the same invariant.

### 2.3 Recognise a missing `@` import prefix before `/` becomes an operator error

**Original finding:** DIAG-038

`import core/math` currently reaches symbolic `/` spacing logic before header import parsing can explain the import mistake.

#### Implementation

- Extend the lexer's small left-context model so `/` can recognise a path-like identifier immediately following `import`.
- Emit a dedicated `CommonSyntaxMistakeReason::ImportPathMissingAtPrefix`.
- Message:

  > Import paths must begin with `@`. Write `import @core/math`.

- Keep ordinary division tokenization unchanged outside this structural import context.
- Do not move general import parsing into the tokenizer.

#### Tests

- `import core/math`
- a longer bare path
- valid `import @core/math`
- ordinary division after a value
- `//` integer division and comments remain unaffected

### 2.4 Improve incomplete expression and declaration boundaries

**Original findings:** DIAG-012, DIAG-016, DIAG-018, DIAG-024, DIAG-053, DIAG-054
**Original DIAG-034 disposition:** Removed. Current expression typing already emits `BinaryRight`.

Add focused reasons at the parser that owns each boundary:

- missing condition after `if`
- missing `else` branch in a value-producing `if`
- missing value after `then`
- missing value after `else`
- missing member name after `.`
- missing return type after `->`
- missing declaration initializer after an authored `=`

#### Required messages

- `Expected a condition after 'if'.`
- `Value-producing 'if' requires an 'else' branch.`
- `Expected a value after 'then'.`
- `Expected a value after 'else'.`
- `Expected a field or method name after '.', but this access ends here.`
- `Function signature is missing a return type after '->'. Add a type followed by ':', or remove '->' for a no-value function.`
- `Declaration 'value' is missing an initializer expression after '='.`

#### Ownership

- Reuse `InvalidControlFlowStatement` for value-`if` structure.
- Reuse `InvalidFieldAccessReason::ExpectedNameAfterDot` but render it without the `field_name` fallback.
- Add `InvalidFunctionSignatureReason::MissingReturnType`.
- Add `InvalidDeclarationReason::MissingInitializerExpression`.
- Do not route empty expressions into an RPN stack and wait for `No nodes found in expression`.

#### Tests

Use EOF, newline and closing-delimiter variants where they exercise distinct parser boundaries. Assert source location at the missing-value boundary rather than the next unrelated token.

### 2.5 Convert user-input infrastructure failures

**Original findings:** DIAG-011, DIAG-012, DIAG-035

#### Malformed `$children(...)`

Use the existing `InvalidTemplateDirectiveReason::InvalidChildrenArgument`.

- Reject an empty, truncated or malformed argument before the general expression parser reaches an impossible empty stack.
- Primary label: malformed argument or closing delimiter.
- Secondary label: `$children(` opening location when available.
- Keep `BST-SYNTAX-0021`.

#### Duplicate function parameters

Duplicate parameter names are source declarations, not HIR invariants.

- Detect them while parsing or registering the function signature.
- Reuse `DuplicateDeclaration` and include:
  - duplicate name
  - current parameter location
  - previous parameter location
  - function name when available
- Do not create a function-scope `CompilerError`.

#### Value-producing `if`

The incomplete-value cases in 2.4 must return typed diagnostics before expression evaluation.

#### Acceptance

The retained malformed-source fixtures produce no `BST-INFRA-0001`.

## Phase 3: Mutability, assignment and explicit copy

### 3.1 Consolidate mutable call and receiver diagnostics

**Original findings:** DIAG-001, DIAG-008, DIAG-046
**Original DIAG-007 corrected below**

Mutable access diagnostics must distinguish these source states:

1. existing mutable place, missing `~`
2. existing immutable place
3. explicit `~` on an immutable place
4. explicit `~` on a fresh or computed non-place
5. plain fresh value passed to a mutable parameter
6. mutable receiver call missing `~`
7. immutable receiver for a mutating method

State 5 is valid and must not be diagnosed.

#### Implementation

Replace broad reasons with explicit call and receiver facts. The exact enum layout may remain split between `InvalidCallShapeReason` and `InvalidReceiverCallReason`, but both paths must use the same access classification from `value_mode` and place analysis.

Carry:

- callee or method name
- parameter name/index where applicable
- receiver or argument place name when it is a simple source place
- whether the place is mutable
- whether the marker was authored
- source location of the place and marker

#### Messages

Existing mutable place, missing marker:

> Call to `consume` requires explicit mutable access for parameter `values`. Prefix the existing mutable place with `~`.

Immutable place:

> Call to `consume` requires mutable access for parameter `values`, but `values` is immutable. Declare the binding as mutable, then pass `~values`.

Mutable receiver missing marker:

> Mutable receiver method `move` requires explicit mutable access. Prefix the receiver with `~`.

When the receiver is a simple named place, guidance may show `~p.move(...)`. Do not render internal placeholders such as `~this receiver`.

Immutable collection or map receiver:

> `push` requires a mutable collection receiver. Declare the collection binding as mutable, then call it with `~values.push(...)`.

Fallible collection and map examples must include `!` or `catch` when a complete example is shown.

#### Required AST correction

`receiver_access.rs` currently merges non-place and immutable-place receivers because it assumes the repair is identical. Split them. A temporary mutating receiver and an immutable named receiver have different explanations.

#### Tests

Use one matrix covering user functions, source receiver methods, collection builtins and map builtins. Include positive fresh-rvalue calls to prevent the change from requiring illegal `~` on fresh values.

### 3.2 Correct immutable field-write guidance

**Original finding:** DIAG-007
**Rejected original proposal:** `~p.x = 10` is not Beanstalk assignment syntax.

For:

```beanstalk
p = Point(x = 1, y = 2)
p.x = 10
```

the root binding is immutable. The correct pattern is:

```beanstalk
p ~= Point(x = 1, y = 2)
p.x = 10
```

#### Implementation

- Keep assignment syntax marker-free.
- When the target is a field, carry the field name and mutable root binding where available.
- Render:

  > Cannot assign to field `x` because root binding `p` is immutable. Declare `p` as mutable before this assignment.

- A secondary label should point to the immutable declaration.
- Keep a generic immutable-place fallback for projections whose root cannot be named cleanly.

### 3.3 Diagnose `~` on assignment targets accurately

**Original finding:** DIAG-021
**Rejected original proposal:** `x ~= 2` is a declaration form, not ordinary reassignment.

For:

```beanstalk
x = 1
~x = 2
```

render:

> `~` is not written on assignment targets. Declare the binding as mutable, then reassign it with ordinary `=`.

A complete correction is:

```beanstalk
x ~= 1
x = 2
```

Add a dedicated assignment-target reason. Do not reuse `MutableMarkerOnNonReceiverCall`, whose message is call-specific.

### 3.4 Diagnose `copy ~place` as an unnecessary access marker

**Original finding:** DIAG-020

`copy` accepts an existing place and creates independent value semantics. It does not take mutable-access syntax.

Add `InvalidCopyTargetReason::MutableMarkerNotAllowed`:

> `copy` does not take `~`. Use `copy x` to copy the value of a mutable binding.

Keep `NonPlace` for literals and computed expressions.

### 3.5 Improve borrow-conflict explanations without inventing lifetimes

**Original finding:** DIAG-056

Borrow diagnostics should explain source ordering and aliases, not tell users to manually end a borrow or mention internal analysis state.

#### Implementation

- Preserve the conflicting place and add its originating access location where known.
- Add a secondary label at the access that remains live.
- Render by access pair.

Mutable alias blocks shared read:

> Cannot read `data` while mutable alias `first` is still needed later. Read through `first`, or move the later use of `first` before this access.

Shared alias blocks mutation:

> Cannot mutably access `data` while shared alias `shared` is still needed later. Move the mutation after the last use of `shared`, or create an explicit copy when independent data is required.

Second mutable access:

> Cannot create another mutable access to `data` while `first` is active. Reuse `first`, or move the new access after its last use.

- Keep generic fallbacks when a conflicting source place or location is unavailable.
- Do not describe `~` as a move.
- Do not expose exact lifetimes or ownership flags.

#### Tests

Add passing counterparts that resolve the conflict by reordering the last use, reusing the active alias, narrowing a scope or using explicit `copy`.

## Phase 4: Error, option and value-flow diagnostics

### 4.1 Replace the umbrella `NotResultExpression` path

**Original findings:** DIAG-025, DIAG-043, DIAG-044

The current `InvalidResultHandlingReason::NotResultExpression` hardcodes postfix `!` wording and calls the source carrier a Result even when the authored construct is `catch` or the operand is optional.

#### Implementation

Replace the umbrella reason with explicit source cases:

```rust
enum InvalidFallibleHandlingReason {
    CatchOnNonFallible,
    CatchOnOptional,
    BangOnNonFallible,
    BangOnOptional,
    QuestionOnNonOptional,
    QuestionOnFallible,
    ErrorPropagationAtTopLevel,
    ErrorPropagationInNonFallibleFunction,
    OptionPropagationAtTopLevel,
    OptionPropagationInNonOptionalFunction,
    // retain other structurally distinct catch and propagation cases
}
```

Names may differ, but each branch must encode the authored handler, operand carrier and propagation boundary.

#### Messages

- `catch` on plain value:

  > `catch` handles fallible `Error!` expressions, but this expression is not fallible.

- `catch` on optional:

  > `catch` does not recover an optional value. Inspect it with `if value is |present| ... else ...`.

- `!` on optional:

  > Postfix `!` propagates an `Error!` return, but this expression is optional. Use postfix `?` only inside a compatible optional-returning function, or inspect the option explicitly.

- `?` on fallible call:

  > Postfix `?` propagates absence from an optional value, but this call can return `Error!`. Use `!` to propagate the error or `catch` to recover.

- top-level `!`:

  > Top-level code has no `Error!` return slot, so `!` cannot propagate here. Recover with `catch`, or call this from a function that returns `Error!`.

- `!` in a real non-fallible function:

  > This function does not declare an `Error!` return slot. Add one or recover locally with `catch`.

Keep the stable `BST-RULE-0051` family unless a branch belongs to an existing more precise code.

### 4.2 Add catch-recovery type context

**Original finding:** DIAG-026

Add `TypeMismatchContext::CatchRecovery`.

Message:

> Type mismatch in catch recovery: expected `Int`, found `String`.

Guidance may explain that each `then` value must match the corresponding success slot. Keep expected and found types as `TypeId`s.

### 4.3 Reject statement-match expression bodies at the expression

**Original finding:** DIAG-003

A statement match and a value-producing match are different source shapes. Do not suggest inserting `then` without also moving the match to a receiving site.

Message:

> This is a statement match, so this arm body must contain statements. To compute a value, place the match at a declaration, assignment, return, multi-bind or nested `then` receiver and use `then` in every producing arm.

The primary label belongs on the bare expression body, not the next arm header.

Tests need:

- statement match with a bare expression body
- valid statement match
- valid value-producing match at a declaration receiver
- exhaustive choice coverage or `else` so the fixture does not fail for an unrelated reason

### 4.4 Reject multi-return values received by one target

**Original finding:** DIAG-031

Do not silently discard success slots.

#### Implementation

Add a dedicated value-receiver diagnostic family rather than misusing `InvalidMultiBind`, because the invalid source has no multi-bind.

Carry:

- receiver kind: declaration, assignment or return
- target count
- produced slot count
- call or value-block location

Message:

> This expression produces 2 values, but the declaration has 1 target. Use one target per return slot, for example `name, count = pair()`.

Do not suggest `_` or another discard syntax. None is part of the current language.

Tests should cover declaration, assignment and nested value-producing block receivers if those paths are distinct.

### 4.5 Use the same `then` diagnostic at top level and in functions

**Original finding:** DIAG-032

Route top-level `then` through the existing structured `ThenWithNoActiveValueTarget` reason:

> `then` is only valid inside a value-producing `if`, full match or `catch` recovery that has an active receiving site.

Do not leave top-level parsing on generic `UnexpectedToken`.

### 4.6 Reject bodyless non-`else` match arms

**Original finding:** DIAG-052

Only bodyless `else =>` is the explicit statement no-op arm.

Add `InvalidMatchArmReason::MissingBody`:

> This match arm has no body. Add a statement after `=>`. Only `else =>` may be bodyless.

Primary label: the arm arrow or empty body boundary.

Keep positive coverage for bodyless `else =>`. Value-producing matches must continue to require produced values on every selected path.

## Phase 5: Names, imports, calls and match context

### 5.1 Correct generic constructor guidance

**Original finding:** DIAG-002
**Rejected original proposal:** `x ~Box of String = ...` and `Box of String { ... }` are not the intended constructor pattern.

`Box of String` is type annotation syntax. Construction still uses the nominal constructor name:

```beanstalk
x Box of String = Box(value = "hello")
```

#### Implementation

When a generic type application is parsed in value position, report:

> `Box of String` is a type annotation, not a value expression. Construct the value with `Box(...)` and provide a concrete receiving type when inference needs it.

Keep ordinary type-as-value diagnostics for non-generic cases.

### 5.2 Add scope-correct type-name suggestions

**Original finding:** DIAG-047
**Original DIAG-033 disposition:** Removed. An unknown uppercase name is not enough evidence that the author forgot a generic `type` parameter.

Add candidate data to `UnknownTypeName` from the active type-resolution scope:

- builtin types
- visible local nominal types
- visible aliases
- visible imported and external types
- active generic parameters

Use the existing bounded edit-distance policy. Suggest only a sufficiently close visible candidate.

Examples:

- `Strng` -> `String`
- close local type typo -> local type
- unrelated unknown name -> no suggestion

Do not search private, unimported or out-of-scope types.

### 5.3 Preserve namespace context and suggest direct members

**Original findings corrected:** DIAG-009, DIAG-048
**Original DIAG-040 disposition:** Removed. `math.PI` is valid namespace access.
**Original DIAG-022 disposition:** Removed. Grouped imports of direct external exports are valid.

Add a namespace-member diagnostic that carries:

- namespace path or local alias
- requested member
- direct visible member names
- package/source namespace kind

Messages:

- unqualified direct member that exists in an imported namespace:

  > Unknown value `PI`. It is available as `math.PI` from the imported `@core/math` namespace. Use a grouped import if you want the bare name.

- qualified typo:

  > Unknown member `pi` on namespace `math`. Did you mean `PI`?

- unrelated qualified member:

  > Unknown member `nope` on namespace `math`.

Rules:

- Source namespace records remain shallow.
- External namespace records may expose recursive package-local namespaces.
- Suggest only members of the exact resolved namespace node.
- Do not suggest receiver methods as namespace fields.
- A grouped import suggestion is valid only for a direct export.

Extend `MissingPackageSymbol` with the same bounded direct-export suggestion policy so `import @core/math { pi }` may suggest `PI` rather than claiming grouped imports are unsupported.

### 5.4 Include actual and expected argument counts

**Original finding:** DIAG-045

Extend `ExtraPositionalArgument` with `provided_count` and the extra argument location.

Message:

> Call to `add` has 3 positional arguments, but accepts 2.

Guidance:

> Remove the extra argument starting here.

Do not suggest named arguments. Renaming arguments does not fix excess arity.

Cover user functions, struct constructors and choice constructors where they share or intentionally differ in call-shape ownership.

### 5.5 Enrich choice match-pattern diagnostics

**Original findings:** DIAG-049, DIAG-050, DIAG-051

#### Unknown variant

Carry:

- source-visible choice name
- requested variant
- available variants

Message:

> Unknown variant `Color::Gren`. Did you mean `Color::Green`? Available variants: `Red`, `Green`, `Blue`.

Do not offer a close-name suggestion when the threshold is not met.

#### Qualifier mismatch

Carry the authored qualifier and the scrutinee's source-visible choice name:

> Match arm uses qualifier `Size`, but the scrutinee is `Color`. Use a `Color` variant or omit the qualifier.

Imported aliases must render using the visible source name where possible.

#### Payload captures

Carry expected field names, provided capture names and expected/found counts.

Messages:

- name mismatch:

  > Capture `missing` does not match payload field `value` in variant `Ok`. Use `Ok(value)` or rename it with `Ok(value as missing)`.

- count mismatch:

  > Variant `Ok` has 1 payload field, `value`, but this pattern captures 0.

Keep exact field lists bounded and deterministic.

## Phase 6: Template and external-JavaScript diagnostics

### 6.1 Report exact template-head conflicts and why they conflict

**Original finding:** DIAG-014
**Confirmed decision:** Every incompatible directive pair is rejected with both names and a semantic explanation.

The current compatibility state stores only aggregate tags. Once a conflict is detected, the parser has lost the identity and location of the earlier item.

#### Implementation

Replace the bitset-only diagnostic path with retained seen-item facts:

```rust
struct SeenTemplateHeadItem {
    display_name: TemplateHeadItemName,
    presence_tags: TemplateHeadTag,
    location: SourceLocation,
}

enum TemplateHeadConflictReason {
    MultipleFormatters,
    DirectiveMustBeOnlyMeaningfulItem,
    DuplicateExclusiveDirective,
    SlotDirectiveMustBeExclusive,
    // add narrow reasons required by registered compatibility rules
}
```

`TemplateHeadState` may still retain aggregate tags for fast checks, but it must also retain enough ordered metadata to identify the first concrete conflicting item.

Every compatibility rule that can reject a pair must provide a `TemplateHeadConflictReason`. Registry validation must reject a builder directive specification that blocks another item without declaring a renderable conflict reason.

Diagnostic payload:

- earlier item name and location
- incoming item name and location
- structured conflict reason

Examples:

> `$md` and `$raw` cannot be combined because both control template body formatting. Choose one formatter.

> `$note` cannot be combined with `$children` because a discarded comment directive must be the template's only meaningful head item.

- Primary label: incoming item
- Secondary label: earlier conflicting item
- Keep `BST-SYNTAX-0022`

#### Tests

- `$md` with `$raw` in both orders
- duplicate formatter
- comment directive with another meaningful item
- `$slot` exclusivity
- builder-registered directive conflict
- compatible combinations remain accepted

### 6.2 Improve unclosed-template EOF context

**Original finding:** DIAG-010

Track the opening `[` location in the template-mode stack.

Message:

> This template is not closed before the end of the file. Add `]`.

Labels:

- primary at EOF
- secondary at the opening `[` with `Template starts here`

Use the same opening-location support for truncated template heads, bodies and nested templates. Do not create separate ad hoc EOF strings for each parser.

### 6.3 Bind `@bst.sig` to the nearest JavaScript declaration

**Original finding:** DIAG-042

The annotation scanner currently sees only supported exports, so an annotation before a plain function may drift to a later export.

#### Implementation

- Scan an ordered stream of supported exported declarations and supported-looking unexported declarations.
- Bind `@bst.sig` only to the nearest following top-level declaration.
- Permit only whitespace and comments between annotation and declaration.
- Add a typed external-JS reason `MissingExportKeyword`.
- For a plain function:

  > `@bst.sig` for `add` applies to JavaScript function `add`, but the function is not exported. Add `export` before `function`.

- For a block-bodied arrow:

  > `@bst.sig` for `add` applies to JavaScript constant `add`, but it is not exported. Add `export` before `const`.

- Consume the matched unexported declaration after reporting so the annotation cannot drift.
- Keep a separate orphaned-annotation reason when no supported declaration follows.
- Preserve the typed provider reason through `InvalidExternalModule` conversion rather than flattening every failure to an opaque string.
- Primary label: insertion point at the declaration
- Secondary label: annotation

#### Tests

- plain function missing `export`
- block-bodied arrow missing `export const`
- missing-export declaration followed by a valid annotated export
- private helper between annotation and export prevents distant binding
- genuinely orphaned annotation
- provider-level rendering preserves `BST-IMPORT-0022` and the JavaScript source span

### 6.4 Keep standalone template-helper diagnostics source-visible

**Additional audit finding:** `InvalidTemplateStructureReason::HelperOutsideWrapperSlot` currently tells the user that a helper reached AST finalization. The source mistake is a standalone `$insert(...)` contribution, not the compiler stage that detected it.

Render:

> `$insert(...)` is a template contribution helper, not a standalone value. Keep it inside a template application that receives the contribution.

- Do not mention AST, TIR, finalization or internal helper-artifact policy.
- Keep the diagnostic at the standalone helper expression.
- Add an integration assertion for the existing standalone-insert rejection so the source-visible wording is contractual.

## Phase 7: Remaining focused quality improvements

### 7.1 Duplicate reactive declarations should use the normal no-shadowing path

**Original finding:** DIAG-023

A second reactive declaration is not invalid because `$` is unexpected. It is invalid because the visible name already exists.

- Parse the declaration shape before duplicate registration rejects it.
- Reuse `DuplicateDeclaration`.
- Message should state the general Beanstalk rule:

  > Cannot redeclare `name` while the existing binding is visible. Beanstalk does not allow shadowing.

- Label both declarations.
- Do not invent a separate reactive uniqueness rule.

### 7.2 Exact operator diagnostics consolidated into Phase 1.2

**Original findings:** DIAG-015, DIAG-030
**Status:** Moved earlier

The exact operator payload and the `not`, string concatenation and generic-parameter messages now belong to Phase 1.2a. The generic-bound terminology fix already requires exact operator facts, so retaining a later category-only-to-exact migration would create transitional API and duplicate renderer work.

### 7.3 Add actionable collection-loop guidance

**Original finding:** DIAG-055

Keep the found semantic `TypeId`.

Message:

> Collection loop source must be a collection, found `Int`. Use a collection after `loop`. For numeric iteration, use range syntax such as `loop 0 to count |i|:`.

Do not imply every non-collection source was intended as a range.

## Phase 8: Doc comments and code comments review

Do an extensive review of comments across the codebase that may be stale, drifting from design docs or are enabling misunderstanding features or language surface.

This review should focus on making sure areas where diagnostic improvements have been corrected by this plan are not also commented with incomplete or outdated information.

This report should be created by exploring the codebase in parallel, then coalesing the reports into a file kept in the tmp/ folder and reviewing them for accuracy before implementing the corrections.

Any bad, noisy or pointless comments that don't follow the style guide can be in scope for this review. Ideally, line counts for comments should be reduced, compressed and made more concise without losing important context rather than further bloated with much more information.


## Removed or superseded original findings

These items must not be implemented as originally proposed.

| Original | Disposition | Reason |
|---|---|---|
| DIAG-001 | Retained, rewritten | The original snippet used an immutable collection, so `~values.push(...)` alone was not a valid fix. |
| DIAG-002 | Retained, rewritten | Generic construction uses `Box(...)` at a concrete receiving type, not `Box of T { ... }` or `~Box`. |
| DIAG-006 | Removed | The snippet was not a const-required cast and bare `cast` with fallible evidence must first be handled with `cast!` or `catch`. The proposed literal-failure branch conflated evidence selection with const failure. |
| DIAG-007 | Retained, reversed | The binding does need to be mutable. Field assignment is `p.x = ...`, not `~p.x = ...`. |
| DIAG-017 | Removed as stale | Current rendering identifies `|`, not `/`. A broader parser-context change is not justified without a current reproducible failure. |
| DIAG-019 | Removed as ambiguous | `MyList {Int}` is a parseable typed declaration missing an initializer. The compiler cannot know it was intended as a type alias. |
| DIAG-022 | Removed as incorrect | Direct external package exports support grouped imports. A missing symbol may receive a close-name suggestion instead. |
| DIAG-029 | Removed as incorrect | Raw strings intentionally preserve backslashes and newlines. |
| DIAG-033 | Removed as ambiguous | Unknown type `A` does not prove that a generic `type A` declaration was intended. |
| DIAG-034 | Removed as resolved | Current expression typing already emits `BinaryRight` when the right operand is absent. |
| DIAG-036 | Removed as resolved | Mixed map entries and missing map values already use separate structured reasons. Rendering a complex key as a fabricated repair is not required. |
| DIAG-039 | Replaced by strict deletion | No legacy-specific rejection remains. |
| DIAG-040 | Removed as incorrect | External package namespace access is supported. The example also used the wrong member casing. |
| DIAG-013 and DIAG-027 | Consolidated | One terse-render fallback fixes the shared root cause. |
| DIAG-012 and DIAG-024 | Consolidated | Both belong to incomplete value-producing `if` parsing. |
| DIAG-025, DIAG-043 and DIAG-044 | Consolidated | One source-aware fallible/optional handling matrix owns the distinction. |

## Implementation order

Implement in this order to avoid parallel diagnostic paths:

1. Phase 1 render fallback, terminology cleanup and `#import` deletion
2. Phase 2 tokenizer and parser boundary reasons
3. Phase 3 mutable access, assignment and copy reasons
4. Phase 4 fallible/optional handling and value receivers
5. Phase 5 visibility-aware suggestions and match payload facts
6. Phase 6 template compatibility and external-JS scanner binding
7. Phase 7 remaining focused improvements
8. Phase 8 codebase comments audit
9. Final repository-wide diagnostic audit and validation

After each phase:

- remove superseded reason variants and render branches
- search for duplicate messages that encode the same rule
- check that no later stage compensates for a now-earlier diagnostic
- review fixture overlap and keep one primary owner
- review the progress matrix if accepted or rejected source behaviour changed

## Test strategy

### Integration cases

Use `tests/cases/` for:

- quoted-string accepted and rejected source behaviour
- import syntax
- malformed value-producing control flow
- mutable calls and receiver calls
- assignment and copy syntax
- result/option handling
- multi-return receiving
- namespace and type suggestions where rendered wording is contractual
- match patterns
- template directive conflicts
- malformed templates
- JavaScript import provider diagnostics
- borrow conflicts

Assert:

- stable diagnostic code
- expected source location when location is part of the fix
- a message fragment only where wording or named context is the feature

### Unit tests

Use focused subsystem tests for:

- quoted-string decoding
- operator-spacing side classification
- terse fallback
- edit-distance thresholds and candidate filtering
- template compatibility conflict selection
- JavaScript annotation-to-declaration binding
- diagnostic payload remapping for every new `StringId` or path field
- descriptor-code uniqueness after adding and removing kinds

### Regression requirements

- No negative fixture should pass only because a later unrelated diagnostic fires.
- Positive counterparts must protect the intended valid Beanstalk pattern.
- Do not use benchmark fixtures as diagnostic coverage.
- Remove fixtures tied only to deleted legacy paths.

## Documentation updates required during implementation

Update the narrowest canonical source documents:

- quoted-string escape set and raw-string preservation
- template directive incompatibility rule
- removal of all `#import` references
- any progress-matrix rows affected by:
  - newly rejected invalid string escapes
  - newly rejected single-target multi-return receiving
  - newly rejected bodyless non-`else` match arms
  - corrected infrastructure failures becoming structured diagnostics

Do not edit generated files under `docs/release/**`. Rebuild them through the compiler when documentation source changes.

## Final validation

Run targeted tests during each phase. Before completion:

```text
cargo fmt
just validate
```

Also perform the manual diagnostic audit required by the compiler style guide:

- every user mistake uses `CompilerDiagnostic`
- every new payload carries structured facts
- every source location points to the actual mistake
- no stale or legacy message path remains
- no suggestion teaches invalid syntax
- no duplicate diagnostic owner remains across tokenizer, headers, AST, HIR or borrow validation
- no type decision compares rendered syntax instead of `TypeId`
- no borrow diagnostic exposes compiler-internal lifetime or ownership machinery
- the progress matrix reflects changed rejection and coverage
- generated documentation was rebuilt rather than edited

## Completion criteria

- All retained cases have end-to-end coverage.
- All removed findings are absent from the implementation backlog.
- Quoted strings implement exactly the confirmed escape set.
- Raw strings preserve literal backslashes and newlines.
- `#import` has no dedicated compatibility surface.
- Every incompatible directive diagnostic names both items and explains the conflict.
- Mutable diagnostics distinguish missing `~`, immutable places, fresh values and invalid assignment markers.
- Fallible and optional diagnostics name the authored operator and correct Beanstalk carrier.
- Terse diagnostics never have an empty message field.
- Malformed user source in this plan never produces `BST-INFRA-0001`.
- `just validate` passes.
