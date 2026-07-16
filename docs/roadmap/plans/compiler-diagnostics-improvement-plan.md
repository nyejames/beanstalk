# Beanstalk Compiler Diagnostics Improvement Plan

## Purpose

Improve Beanstalk diagnostics without teaching invalid syntax, legacy behaviour or patterns that fight the language design.

Every retained item in this plan has been checked against the current language and compiler contracts. Several findings from the original audit have been corrected, consolidated or removed because their proposed fixes were stale, ambiguous or semantically wrong.

This is an implementation plan, not a research backlog. Each phase should leave one current diagnostic path, remove superseded variants and add end-to-end coverage for the user-visible behaviour it changes.

## Current state

ACTIVE_PLAN: `docs/roadmap/plans/compiler-diagnostics-improvement-plan.md`
STATUS: active
CURRENT_SLICE: Phase 2.2, accepted and ready to commit
LAST_ACCEPTED_COMMIT: `979703ea7`
WORKTREE: `main` at `979703ea7` with the reviewed Phase 2.2 implementation, tests, canonical docs, generated docs, hard-removal audit cleanup and plan updates
REQUIRED_RELOADS: startup files, this plan and current source/diff
RELEVANT_CONTEXT_NOW:
- docs: symbolic operator, assignment, compound-assignment and mutable-declaration spacing contracts plus tokenizer, diagnostics, testing and validation ownership
- code: tokenizer symbolic-operator scanning and tests, `DiagnosticOperator`, syntax diagnostic payload/reason/constructor/render/remap and existing spacing fixtures
ACCEPTANCE_CRITERIA:
- `BST-SYNTAX-0031` carries the exact binary operator, assignment, compound assignment or adjacent `~=` mutable declaration plus the missing whitespace side
- complete tokens such as `//=` are classified before spacing is checked
- internal whitespace in `name ~ = value` remains owned by `InvalidMutableBindingSpacing`
- valid operators, assignment, compound assignment and `name ~= value` tokenization are unchanged
VALIDATION_STATE:
- Phase 1.1 `just validate`: passed, including 3,511 Rust tests and 1,764 integration cases
- Phase 1.2a focused compiler-message tests: passed, 36 tests
- Phase 1.2a `just validate`: passed, including cross-target Clippy, 3,511 Rust tests, 1,766 integration cases, docs check and 28 benchmark cases
- Phase 1.2b `just validate`: passed, including cross-target Clippy, 3,512 Rust tests, 1,767 integration cases, docs check and 28 benchmark cases
- Phase 1.3 `just validate`: passed, including cross-target Clippy, 3,511 Rust tests, 1,755 integration cases, docs check and 28 benchmark cases
- Phase 2.1 `just validate`: passed, including cross-target Clippy, 3,520 Rust tests, 1,756 integration cases, docs check and 28 benchmark cases
- Phase 2.1 docs release build: passed, 72 files built before the final full gate
- Phase 2.2 `just validate`: passed, including cross-target Clippy, 3,523 Rust tests, 1,756 integration cases, docs check and 28 benchmark cases
- Phase 2.2 dev and release docs builds: passed, 72 files built in each mode before the final full gate
DOCS_IMPACT: canonical spacing docs now distinguish binary operators, assignment, compound assignment and adjacent `~=` outer spacing; generated docs were rebuilt; architecture/config docs now use only current postfix constant examples
BLOCKERS_OR_OPEN_DECISIONS: none
DELEGATION_DECISION: Ollama - explicitly required for every implementation worker slice
NEXT_WORKER_ORDER: Ollama only for implementation slices
STOP_REASON: none
NEXT_RESUME_ACTION: commit Phase 2.2, reload the plan and startup context, then prepare the bounded Phase 2.3 Ollama worker

## Confirmed design decisions

- Quoted strings support exactly these escapes:
  - `\\`
  - `\"`
  - `\n`
  - `\r`
  - `\t`
- Every other quoted-string escape is rejected.
- Raw backtick strings do not process escapes. Backslashes and newlines remain literal.
- Discontinued syntax receives no migration-specific compatibility diagnostics.
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
**Status:** Complete in `df65c9ced`

The terse renderer now uses one render-boundary fallback from an empty payload message to the
descriptor title. Focused and all-kind invariant tests protect non-empty messages without changing
stable codes, locations or the terminal and dev-server formats.

### 1.2 Remove stale source terminology and misleading guidance

**Original findings consolidated:** DIAG-015, DIAG-025, DIAG-030, DIAG-043, DIAG-044
**Status:** Complete in `fb4d26a3b` and `99ec971b9`

Phase 1.2a replaced category-only operator facts with diagnostic-owned `DiagnosticOperator`
values and exact authored spellings. Range renders as `to`. Generic-parameter operator failures
now explain that operators are compiler-owned and aren't granted by trait bounds.

Phase 1.2b removed source-facing `Result` and `Option<Type>` terminology, stale
top-level-only template guidance, internal AST/finalization and const-model wording plus
implementation-stage language around deferred `async`. Standalone unreceived templates now use
the existing standalone-statement family. Superseded reason variants, payload paths and renderer
branches were removed rather than preserved as compatibility aliases.
### 1.3 Remove obsolete compatibility-only diagnostic paths

**Original finding:** DIAG-039
**Decision:** Strict removal
**Status:** Complete in `2189b2107`

Compatibility-only diagnostic kinds, payloads, constructors, recognisers, pre-scans, fixtures and
migration prose for discontinued declaration syntax were deleted. Current binding, constant,
template and module-root syntax retained its existing positive coverage. Canonical docs were
updated and generated docs were rebuilt through the compiler.

The Phase 2.2 hard-removal audit also deleted the obsolete diagnostic display `#Config` for const
records and replaced ambiguous “top-level `#` constant” prose with exact current postfix examples.
Const-record diagnostics now render source-visible `const record Config` terminology.

## Phase 2: Tokenizer and structural syntax diagnostics

### 2.1 Implement the quoted-string escape contract

**Original finding:** DIAG-028
**Status:** Complete in `979703ea7`

Quoted strings now decode exactly `\\`, `\"`, `\n`, `\r` and `\t`. Typed
`BST-SYNTAX-0034` reasons distinguish unsupported escapes, physical LF/CRLF continuation and a
trailing backslash. Spans cover two authored characters for complete unsupported escapes and only
the backslash for continuation or trailing cases. Control characters render source-visibly.

Raw-token behavior is unchanged: backslashes and physical newlines stay literal, no escape
diagnostic is emitted and expression-position raw backticks remain rejected by the existing
ordinary syntax path. Canonical string docs, generated docs and the progress matrix record the
accepted contract. Tokenizer, renderer and integration coverage protect decoding, reasons, spans
and raw-token preservation.
### 2.2 Make symbolic spacing diagnostics exact

**Original findings:** DIAG-004, DIAG-005
**Status:** Accepted, commit pending
**Additional confirmed gap:** the same umbrella diagnostic currently labels plain assignment `=` and malformed mutable declaration spacing around `~=` as binary-operator errors, even though neither construct is a binary operator
**Additional confirmed doc drift:** canonical spacing prose groups assignment with binary operators
and says spacing is required around both tokens in `~=`, which can teach the invalid
`name ~ = value` form. This slice must state that `~=` stays adjacent and requires only outer
whitespace.

The tokenizer currently uses one `InvalidSymbolicBinaryOperatorSpacing` reason for binary operators and compound assignment. It does not carry the operator or missing side.

#### Implementation

Replace the umbrella reason with structured facts. The construct enum must own the
exact symbolic spelling so invalid construct/operator combinations cannot be formed:

```rust
enum SymbolicSpacingConstruct {
    BinaryOperator { operator: DiagnosticOperator },
    Assignment,
    CompoundAssignment { operator: DiagnosticCompoundAssignmentOperator },
    MutableDeclaration,
}

enum MissingWhitespace {
    Before,
    After,
    Both,
}

struct SymbolicSpacingError {
    construct: SymbolicSpacingConstruct,
    missing: MissingWhitespace,
}
```

`DiagnosticCompoundAssignmentOperator` contains only `+`, `-`, `*`, `/`, `//`, `%` and `^`.
This keeps unsupported combinations such as word-operator compound assignments unrepresentable.

- Determine the complete token first, including `//=`.
- Record leading and trailing whitespace independently.
- Render the exact construct and side:
  - `Binary operator '+' requires whitespace after it.`
  - `Assignment '=' requires whitespace before it.`
  - `Compound assignment '+=' requires whitespace before it.`
  - `Compound assignment '//=' requires whitespace on both sides.`
  - `Mutable declaration '~=' requires whitespace after it.`
- Keep the correction example minimal and valid.
- Do not describe compound assignment as a binary operator.
- Do not describe ordinary assignment or `~=` mutable declaration syntax as a binary operator.
- Keep valid `name ~= value` tokenization unchanged. This spacing diagnostic does not decide
  whether the name is a fresh declaration. Existing-name rejection remains owned by the normal
  no-shadowing path.
- Preserve `InvalidMutableBindingSpacing` as the declaration parser's owner for whitespace inside
  the marker pair, such as `name ~ = value`. `SymbolicSpacingConstruct::MutableDeclaration` owns
  only missing outer whitespace around the adjacent `~=` spelling. Do not merge these structurally
  different mistakes or weaken the marker-adjacency check.
- Preserve the existing `BST-SYNTAX-0031` family unless descriptor policy requires a dedicated compound-assignment code.

#### Tests

Cover every compound-assignment token at least once and cover before, after and both missing-side branches across the operator family. Also cover plain `=`, both outer sides of adjacent `~=` and the preserved internal-whitespace rejection so none can regress to binary-operator prose. Avoid one fixture per cosmetic variant when a tokenizer table test protects the same invariant.
`name~=value` must report `Both`, not whichever side the tokenizer encounters first.
Rename the stale `constant_mutable_spacing_error` integration owner to describe mutable-binding
internal spacing without calling a runtime binding a constant.

### 2.3 Recognise a missing `@` import prefix before `/` becomes an operator error

**Original finding:** DIAG-038
**Additional confirmed gap:** a payload-free reason cannot render the exact correction for the longer bare paths required by this phase and would misleadingly replace every path with the fixed `core/math` example
**Additional confirmed gap:** `import ./utils` is the same missing-`@` mistake but currently reaches
the generic `BST-SYNTAX-0019` "expected a path" branch. The structured reason must cover relative
bare paths as well as identifier-led paths.
**Additional confirmed path family:** direct JavaScript provider imports keep their `.js`
extension, so a missing-prefix path such as `import ./drawing.js` or
`import vendor/drawing.js as drawing` must preserve the extension and stop before the alias.
**Additional confirmed doc drift:** the canonical import-path reference and current compiler use
`@./file` for file-relative imports, but `docs/compiler-design-overview.md` still says `@./...`
has no supported meaning. Correct that architecture bullet in this slice. The narrow language
reference is authoritative.

`import core/math` currently reaches symbolic `/` spacing logic before header import parsing can explain the import mistake.

#### Implementation

- Extend the lexer's small left-context model so it recognises a bare identifier-led path
  immediately following `import`, including a single component such as `import core`, and
  recognises a relative `./...` spelling in the same import position before the generic
  import-clause fallback. A later `/` must not become the first point at which the compiler
  realises that the prefix is missing.
- Emit a dedicated structured `CommonSyntaxMistakeReason::ImportPathMissingAtPrefix { authored_path }` carrying the complete bare path spelling. Remap its `StringId` through the existing payload path.
- Message:

  > Import paths must begin with `@`. Write `import @<authored_path>`.

- Point the primary label at the bare path, not only the `/` that exposed the mistake.
- The lexer may scan this narrow path-like sequence to preserve the correction fact, but must not duplicate general import-path parsing or resolution.
- Keep ordinary division tokenization unchanged outside this structural import context.
- Do not move general import parsing into the tokenizer.

#### Tests

- `import core/math`
- a single-component bare path such as `import core`
- a longer bare path whose rendered correction preserves every authored component
- a bare path followed by a grouped import clause, proving the narrow scan stops before `{`
- a relative bare path such as `import ./utils`
- a relative `.js` provider path and a non-relative `.js` provider path followed by `as`, proving
  the scan preserves extensions and stops before aliases
- valid `import @core/math`
- ordinary division after a value
- `//` integer division and comments remain unaffected

### 2.4 Improve incomplete expression and declaration boundaries

**Original findings:** DIAG-012, DIAG-016, DIAG-018, DIAG-024, DIAG-053, DIAG-054
**Additional confirmed gap:** adjacent operands currently fall through to the generic `Invalid expression: no valid operands found during evaluation` message even though the source contains multiple valid operands and is missing an operator.
**Original DIAG-034 disposition:** Removed. Current expression typing already emits `BinaryRight`.

Add focused reasons at the parser that owns each boundary:

- missing condition after `if`
- missing `else` branch in a value-producing `if`
- missing value after `then`
- missing value after `else`
- missing member name after `.`
- missing return type after `->`
- missing declaration initializer after an authored `=`
- missing operator between adjacent expressions

#### Required messages

- `Expected a condition after 'if'.`
- `Value-producing 'if' requires an 'else' branch.`
- `Expected a value after 'then'.`
- `Expected a value after 'else'.`
- `Expected a field or method name after '.', but this access ends here.`
- `Function signature is missing a return type after '->'. Add a type followed by ':', or remove '->' for a no-value function.`
- `Declaration 'value' is missing an initializer expression after '='.`
- `Expected an operator before this expression.`

#### Ownership

- Reuse `InvalidControlFlowStatement` for value-`if` structure.
- Reuse `InvalidFieldAccessReason::ExpectedNameAfterDot` but render it without the `field_name` fallback.
- Add `InvalidFunctionSignatureReason::MissingReturnType`.
- Add `InvalidDeclarationReason::MissingInitializerExpression`.
- Add a structured adjacent-expression reason at the parser boundary. Point at the second expression and do not guess which operator the author intended.
- Do not route empty expressions into an RPN stack and wait for `No nodes found in expression`.
- Do not use the payload-free `InvalidExpression` fallback for a source sequence such as `value = 1 2`. If that generic diagnostic remains for defensive stack-shape validation, its prose must not falsely claim that no operands were found.

#### Tests

Use EOF, newline and closing-delimiter variants where they exercise distinct parser boundaries. Cover adjacent literal and identifier expressions without duplicating cosmetic cases. Assert source location at the missing-value or second-expression boundary rather than the next unrelated token.

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

### 2.6 Delete config-key migration diagnostics

**Additional audit finding:** `InvalidConfigReason::{DeprecatedSrcKey, ReplacedLibrariesKey,
ReplacedRootFoldersKey, ReplacedPackageFoldersKey}` and their Stage 0 name checks exist only to
teach pre-Alpha config-key migrations. The config registry already has a structured `UnknownKey`
path for every unregistered name. The canonical module plan also records this deletion as
accepted cleanup.

- Delete the four migration-only reason variants and render branches.
- Delete the special name checks for `src`, `libraries`, `root_folders` and `library_folders`.
- Let those unregistered names use the ordinary `UnknownKey` diagnostic without suggesting a
  historical replacement.
- Remove migration-only unit tests, integration fixtures and manifest entries.
- Preserve current `entry_root` and `package_folders` positive coverage plus generic unknown-key
  coverage.
- Do not reserve the removed names or add aliases, compatibility parsing or replacement
  diagnostics.

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

### 3.2 Correct immutable assignment and field-write guidance

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
- Replace the vague direct-binding `ImmutableVariable` message in the same assignment-target
  family. It currently says only to use `~`, which can be mistaken for assignment-target syntax or
  an illegal redeclaration. Carry the original binding location and render:

  > Cannot reassign `value` because its binding is immutable. Declare it with `~=` at the original declaration, then reassign it with ordinary `=`.

- When an explicit type is relevant, guidance may mention the existing `name ~Type = value`
  declaration form. Do not fabricate a full replacement declaration without its original
  initializer and type facts.
- Strengthen the existing direct immutable reassignment and immutable struct-field integration
  cases so the current declaration and assignment guidance is contractual. Remove fixture
  comments that call an ordinary immutable runtime binding a constant.

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

Keep the internal place classification, but make every rendered correction source-visible:

- `NonPlace` for literals and computed expressions:

  > `copy` requires an existing binding or field projection. Bind this value first, then copy that binding.

- Replace `FunctionValue`, which is inaccurate because function values aren't part of the current
  surface, with separate function-name and function-call facts when the following `(` makes the
  distinction available.
- A function name isn't a copyable value. A call should explain that its returned value must be
  received by a binding before that binding can be copied.
- Do not use compiler-facing `place` terminology or call source bindings variables in these
  messages.

Add focused coverage for a literal, computed expression, function name, function call and
`copy ~binding`. Keep one integration owner where the stable code and wording are user-visible.

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

### 3.6 Remove migration state from collection and map assignment diagnostics

**Additional audit finding:** `InvalidAssignmentTargetReason::{CollectionIndexedWriteRemoved,
MapIndexedWriteRemoved, MapPropertyWriteRemoved}` and the
`collection_indexed_write_removed` fixture encode implementation history instead of the current
source rule. These are ordinary invalid assignment targets, not compatibility diagnostics.

- Rename the reasons to factual current states such as `CollectionGetTargetNotWritable`,
  `MapGetTargetNotWritable` and `ReadOnlyMapProperty`.
- Render collection access as:

  > Cannot assign through collection `get(...)`. Use `~items.set(index, value)!` or recover from `~items.set(index, value) catch:`.

- Render map access with the equivalent `~map.set(key, value)` guidance.
- Keep the read-only `length` message, but remove migration wording from its reason name.
- Rename the integration fixture to describe current rejection rather than removal.
- Preserve the existing stable diagnostic code and valid `set` coverage. Do not add a parser path
  that recognises an older assignment feature.

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

Delete `RemovedBangFallbackSyntax` and `RemovedBangCatchHandlerSyntax`, their dedicated parser
recognisers, unit tests and `result_removed_err_bang_syntax_rejected` fixture. Those paths exist
only to identify discontinued handling syntax. The ordinary current grammar may reject the token
sequence without a migration-specific reason. Do not carry either reason into the replacement
matrix.

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

### 4.7 Delete legacy match-arm compatibility diagnostics

**Additional audit finding:** `InvalidMatchArmReason::{LegacyColonSyntax, LegacyElseSyntax}` and
the `current_line_contains_top_level_colon` pre-scan exist only to identify discontinued match-arm
grammar. They preserve migration history in the current parser and renderer.

- Delete both reason variants and their render branches.
- Delete the colon pre-scan and its one-caller helper when it becomes unused.
- Remove the migration-only unit test for colon arms.
- Let `pattern:` reach the ordinary current missing-`=>` or invalid-arm-header diagnostic.
- Let `else:` reach the ordinary current expected-`=>` diagnostic.
- Preserve `InvalidArrow` for authored `->`. That reason factually identifies the current token
  mistake and suggests `=>` without describing a former language feature.
- Do not add replacement recognisers, fixtures or compatibility messages.

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

#### Optional-pattern terminology

The same renderer still calls source values and captures `Option` in several match diagnostics.
Rename the affected `InvalidMatchPatternReason` and `NonExhaustiveMatchReason` variants to
optional-value terminology, then render:

> Optional value patterns require the optional value's inner type to support equality.

> An optional-present capture cannot be empty. Use `|name|` to capture the present value.

> Non-exhaustive optional-value match. Add `none =>` or `|name| =>` to cover all cases.

Apply the same terminology to the non-optional-scrutinee, type-annotation and missing-binding
reasons. Internal AST `OptionPresentCapture` node names are outside this diagnostic wording slice
and do not need a semantic refactor.

### 5.6 Make scalar type-call diagnostics factual

**Additional audit finding:** `InvalidBuiltinCallReason::ScalarConstructorRemoved`, several
`*_constructor_removed` fixtures and the dead `InvalidCastReason::ScalarConstructorRemoved`
encode history rather than the authored mistake. Unlike a discontinued compatibility-only parser
path, the current expression parser must still handle a builtin type token followed by `(`.

- Replace the builtin-call reason with a factual type-as-call reason such as
  `ScalarTypeCalledAsFunction`.
- Render:

  > `Int` is a type, not a conversion function. Use `cast` at an explicit typed boundary.

- Carry the exact builtin type name already available at the emission site.
- Remove the unused `InvalidCastReason::ScalarConstructorRemoved` variant and render branch.
- Rename the focused fixtures and tests around current rejection, then remove redundant
  per-scalar coverage where one table test or one primary integration case protects the same
  parser contract.
- Preserve positive `cast` coverage. Do not recognise a former scalar-constructor language
  feature or describe anything as removed.

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

Delete `InvalidLoopHeaderReason::RemovedInSyntax`, the dedicated `reject_removed_in_loop_syntax`
pre-scan and its migration-only unit test. Current collection and range loop forms already have
their own structured diagnostics. A discontinued `loop <binder> in ...` token sequence may fail
through the ordinary current grammar without a compatibility message.

### 7.4 Reject unsupported Wasm variant payloads before lowering

**Additional audit finding:** `cast_optional_success_wraps_inner_value` currently expects `BST-INFRA-0001` because a reachable optional payload reaches Wasm LIR lowering, where `HirExpressionKind::VariantConstruct` with fields is rejected as a transformation error. This is valid Beanstalk source using a target feature that HTML-Wasm does not yet lower. The existing pre-lowering `UnsupportedBackendFeature` lane owns this failure.

#### Implementation

- Extend Wasm backend feature validation to detect reachable variant constructions with payload fields before LIR lowering.
- Emit the existing `BST-RULE-0064` `UnsupportedBackendFeature` diagnostic at the source expression. Name the unsupported feature as variant payload values without exposing HIR or LIR.
- Preserve reachability policy. An unsupported variant payload in an unreachable helper must not block the selected target.
- Keep empty/unit variant construction on the supported path.
- Do not convert the Wasm lowering invariant itself into a user diagnostic. Backend feature validation must prevent valid reachable source from reaching that invariant.

#### Tests

- Update `cast_optional_success_wraps_inner_value` to expect `BST-RULE-0064` for HTML-Wasm while preserving HTML success.
- Add a focused backend-feature validation test for reachable and unreachable payload construction if the integration case cannot protect both reachability branches.
- Review and update the progress matrix because the target rejection lane and coverage change.

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

1. Phase 1 render fallback, terminology cleanup and compatibility deletion
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
- removal of obsolete compatibility migration references
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
- No deleted syntax has a dedicated compatibility surface.
- Every incompatible directive diagnostic names both items and explains the conflict.
- Mutable diagnostics distinguish missing `~`, immutable places, fresh values and invalid assignment markers.
- Fallible and optional diagnostics name the authored operator and correct Beanstalk carrier.
- Terse diagnostics never have an empty message field.
- Malformed user source in this plan never produces `BST-INFRA-0001`.
- `just validate` passes.
