# Beanstalk Generics Hardening Implementation Plan

## Purpose

This plan freezes Beanstalk's Alpha-facing generics surface and turns the remaining compiler work into implementation-sized phases.

The final language direction is intentionally restrained:

```beanstalk
identity type A |value A| -> A:
    return value
;

value = identity(42)
```

Generics are declaration-site abstractions. Generic calls use ordinary function-call syntax, and type arguments are inferred from immediate local evidence only.

## Final generics rules

- Generic declarations use the existing canonical syntax: `name type A, B |...|`.
- Generic function calls use normal call syntax only.
- Generic inference uses:
  - immediate call arguments;
  - immediate expected result context at closed receiving sites.
- Generic inference does **not** use:
  - later mutation;
  - later use;
  - whole-program analysis;
  - HIR or borrow validation;
  - expected parameter context from an outer function call into a nested generic call.
- Users guide inference with ordinary type annotations:
  ```beanstalk
  value Int = identity(42)
  items {Int} = empty()
  ```
- Concrete generic type aliases are supported:
  ```beanstalk
  StringBox as Box of String
  ```
- Parameterized generic aliases are rejected/deferred in this hardening plan. If revisited later, the only acceptable direction is explicit alias parameters, not partial application:
  ```beanstalk
  -- Deferred, not implemented in this plan.
  StringMap type A as Map of String, A
  ```
- Partial type application is rejected.
- Nested `of` type application remains rejected/deferred.
- Generic receiver methods are rejected.
- Generic function values and higher-order polymorphism are rejected/deferred.
- External package functions and external package types are concrete only.
- Types are not compile-time values.
- No type-returning functions, type-level `#if`, or CTFE type inspection.
- Future generic constraints wait for traits/interfaces and use compact declaration-site syntax:
  ```beanstalk
  max type A is Ordered |left A, right A| -> A:
      ...
  ;
  ```
- No `where` syntax.
- No `<T>` syntax.
- No inline generic sugar such as `|value type A|`.
- No explicit generic call-site syntax such as `identity of Int(42)`, `identity<Int>(42)`, `identity[Int](42)`, or `identity(42 Int)`.

## Non-goals

Do not implement any of the following in this plan:

- traits/interfaces;
- generic constraints beyond reservation/rejection diagnostics;
- parameterized generic alias substitution;
- partial type application;
- nested `of` type application;
- explicit generic call-site type arguments;
- generic function values;
- higher-order polymorphism;
- generic receiver method lookup;
- generic external package functions;
- type values or type-level CTFE;
- roadmap edits. The roadmap should link to this plan separately.

## Repository anchors

| Area | Files | Current role |
|---|---|---|
| Generic parameter parsing | `src/compiler_frontend/declaration_syntax/generic_parameters.rs` | Parses declaration-site `type A, B`; currently rejects some bounds-like tokens. |
| Type syntax | `src/compiler_frontend/declaration_syntax/type_syntax/parse.rs` | Parses annotations and `of`; rejects nested `of`; rejects `type` in type position. |
| Parsed type model | `src/compiler_frontend/datatypes/parsed.rs` | Syntax-only type refs before semantic resolution. |
| Generic scopes | `src/compiler_frontend/datatypes/generic_parameters.rs` | Parsed generic parameter lists, scopes, active generic body context. |
| Generic inference | `src/compiler_frontend/datatypes/generic_bindings.rs` | `GenericParameterId -> TypeId` bindings and conflicts. |
| Type identity | `src/compiler_frontend/datatypes/environment.rs` | Canonical generic parameter registration, generic instances, substitution. |
| Generic free functions | `src/compiler_frontend/ast/generic_functions/` | Templates, calls, diagnostics, body validation, concrete instance identity. |
| Source call dispatch | `src/compiler_frontend/ast/expressions/source_function_calls.rs` | Generic/non-generic source call routing and generic value-use rejection. |
| Signature resolution | `src/compiler_frontend/ast/module_ast/environment/function_signatures.rs` | Registers generic function parameter lists, builds templates, rejects generic receiver methods. |
| AST emission | `src/compiler_frontend/ast/module_ast/emission/emitter.rs` | Validates generic templates and emits concrete generic instances. |
| Type resolution | `src/compiler_frontend/ast/type_resolution/resolve_type.rs` | Resolves parsed types to `TypeId`; generic nominal instantiation; bare generic name rejection. |
| Type aliases | `src/compiler_frontend/ast/module_ast/environment/type_aliases.rs` | Resolves concrete aliases; no generic alias substitution. |
| Header metadata | `src/compiler_frontend/headers/module_symbols.rs`, `src/compiler_frontend/headers/symbol_collection.rs` | Stores and records generic declaration metadata. |
| Diagnostics | `src/compiler_frontend/compiler_messages/` | Structured diagnostics, reasons, labels, rendering. |
| HIR boundary | `src/compiler_frontend/hir/validation.rs`, `src/compiler_frontend/hir/validation/*` | Invariant validation; HIR must not carry unresolved generic executable types. |
| Documentation | `docs/language-overview.md`, `docs/compiler-design-overview.md`, `docs/src/docs/progress/#page.bst` | Final user semantics, compiler-stage contract, implementation matrix. |
| Tests | `tests/cases/manifest.toml`, `tests/cases/generic_*` | Canonical integration coverage. |
| Standards | `docs/codebase-style-guide.md`, `docs/memory-management-design.md` | Stage ownership, diagnostics, validation, HIR/borrow assumptions. |

## Complexity-reduction targets

Use the hardening work to simplify the current generics implementation where practical.

- Do not add alternate syntax paths or compatibility shims.
- Do not add new AST/HIR nodes for rejected generic syntax.
- Keep all new user-facing generic failures on `CompilerDiagnostic`, not `CompilerError`.
- Prefer existing systems:
  - `GenericTypeBindings` for inference;
  - `TypeEnvironment` / `TypeId` for semantic identity;
  - `DiagnosticRenderContext` for type-name rendering;
  - `deferred_feature_diagnostics` only when a feature is intentionally reserved rather than invalid.
- Do not expand semantic use of diagnostic-only `DataType`.
- Remove generic type-alias metadata paths if generic alias parameters are rejected before metadata registration.
- Replace boolean-heavy generic inference APIs with a small named context enum or input struct if it makes call-site behavior clearer.
- Consolidate repeated generic binding-evidence collection into a named helper only if it improves diagnostics and reduces duplicated argument/result evidence logic.
- Avoid broad refactors outside touched generic paths.

## Common phase closeout checklist

Every phase must end with this checklist.

- [ ] Style guide review
  - [ ] Code reads as named stages, not dense expression chains.
  - [ ] No obsolete wrappers, compatibility shims, or parallel APIs were added.
  - [ ] New helpers have one clear owner and file-level docs are updated when ownership changes.
  - [ ] New user-facing errors use `CompilerDiagnostic` with typed payloads/reasons.
  - [ ] Type diagnostics carry semantic `TypeId`s where type identity is involved.
  - [ ] No user input can trigger `panic!`, `todo!`, or user-data-driven `.unwrap()`.
- [ ] Stage-boundary review
  - [ ] Header parsing owns declaration discovery and metadata only.
  - [ ] AST owns generic resolution, inference, template validation, and concrete instance emission.
  - [ ] HIR receives concrete types/functions only.
  - [ ] Borrow validation receives concrete HIR only.
  - [ ] Backends do not solve generics.
- [ ] Test review
  - [ ] Success cases assert behavior/output where practical.
  - [ ] Failure cases assert stable diagnostic codes.
  - [ ] Rendered text assertions are used only where wording is intentional behavior.
  - [ ] Obsolete tests are rewritten or deleted instead of preserved as compatibility coverage.
- [ ] Documentation review
  - [ ] `docs/language-overview.md`, `docs/compiler-design-overview.md`, and `docs/src/docs/progress/#page.bst` remain consistent.
  - [ ] Docs never suggest unsupported explicit generic call syntax.
  - [ ] Docs do not imply traits, `where`, type values, or generic receiver methods are implemented.
- [ ] Validation
  - [ ] `just validate`
  - [ ] Run targeted `generics` cases if the test runner supports filtering.

---

# Phase 0 — Baseline audit and fixture inventory

## Context

Start by confirming the current implementation shape and test coverage. This phase should normally make no production-code changes. Its output is a concrete implementation inventory for the following phases.

## Steps

- [x] Audit all `generics` cases in `tests/cases/manifest.toml`.
  - [x] Group cases by structs, choices, functions, imports/facades, inference success, inference failure, aliases, rejected syntax, receiver methods, diagnostics, and backend contracts.
  - [x] Note any case names that imply obsolete design decisions.
  - [x] Identify dense fixtures that should be split or rewritten only if they obscure the final rule being tested.
- [x] Confirm existing coverage for:
  - [x] generic struct instantiation;
  - [x] generic choice instantiation/matching/equality;
  - [x] generic function declaration and identity call;
  - [x] generic expected-result inference;
  - [x] cross-file grouped and namespace generic calls;
  - [x] facade wrapper generic calls;
  - [x] generic function value-use rejection;
  - [x] generic receiver rejection;
  - [x] explicit generic call syntax rejection;
  - [x] later-use inference rejection;
  - [x] concrete generic alias success;
  - [x] generic alias rejection;
  - [x] nested `of` rejection and alias workaround;
  - [x] recursive generic type rejection.
- [x] Audit current diagnostics in `compiler_messages`.
  - [x] Locate `InvalidGenericInstantiationReason` variants.
  - [x] Locate `DeferredFeatureReason` variants.
  - [x] Identify where explicit generic call syntax, generic values, generic aliases, generic receivers, nested `of`, and constraints currently fail.
- [x] Audit docs for drift.
  - [x] Search for `<T>`, `where`, `identity of`, inline `type A`, type values, and generic function values.
  - [x] Record every location that must be updated in Phase 1.
- [x] Run baseline validation.
  - [x] `just validate`

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Produce a short inventory note in the implementation PR description or issue thread. Do not add a long-lived inventory document unless maintainers request it.

## Accepted Phase 0 inventory summary

- Baseline validation passed with `just validate`: clippy passed, 1969 unit tests passed, 1185 integration cases behaved as expected, docs check passed, and bench-check reported no measurable change.
- Existing fixtures cover generic structs, choices, function identity/wrapper calls, expected-result inference, cross-file grouped and namespace calls, facade generic calls, later-use inference rejection, concrete aliases, generic alias rejection, nested `of` rejection plus alias workaround, and direct/mutual recursive generic rejection.
- Partial coverage remains intentional follow-up scope: explicit generic call syntax only covers `identity[Int](42)` and currently reports generic-function-value rejection; generic receiver coverage lacks the declaration-site generic receiver form `map type A |this Box of A|`; nested expected-parameter inference and external generic signatures have no fixtures.
- Diagnostic anchors are `InvalidGenericInstantiationReason` and `DeferredFeatureReason` in `src/compiler_frontend/compiler_messages/diagnostic_payload/types.rs`, with rendering in `src/compiler_frontend/compiler_messages/render/mod.rs`.
- Phase 1 documentation work should add the language generics section, add the compiler generics contract, and refine the progress matrix rows for explicit call syntax, inference limits, generic function values, receiver methods, aliases, nested `of`, type values, type-level CTFE, and external generics.
- Phase 2 should split or replace `generic_explicit_call_syntax_still_rejected` because the name is broad and the fixture covers only one rejected call form.

---

# Phase 1 — Documentation and implementation matrix finalization

## Context

The docs must freeze the final design before more compiler work lands. This prevents unsupported syntax from being treated as incomplete implementation work later.

## Steps

### `docs/language-overview.md`

- [x] Add a dedicated `### Generics` section near type aliases or near structs/choices.
- [x] Document canonical generic declarations:
  ```beanstalk
  identity type A |value A| -> A:
      return value
  ;

  Box type A = |
      value A,
  |

  Maybe type A ::
      Some | value A |,
      None,
  ;
  ```
- [x] Document generic parameter rules.
  - [x] Declaration-scoped.
  - [x] Type-name style.
  - [x] Compile-time placeholders, not runtime values.
  - [x] Cannot collide with visible concrete types, aliases, external types, builtins, or other generic parameters.
- [x] Document inference-only function calls.
  ```beanstalk
  value = identity(42)
  value Int = identity(42)
  items {Int} = empty()
  ```
- [x] Document inference limits.
  - [x] Immediate arguments only.
  - [x] Immediate expected result context only.
  - [x] No later-use inference.
  - [x] No whole-program inference.
  - [x] No nested expected-parameter inference yet.
  - [x] Use intermediate annotations for nested generic calls:
    ```beanstalk
    user User = parse_json(text)!
    save_user(user)
    ```
- [x] Document allowed unconstrained generic behavior.
  - [x] pass-through;
  - [x] return;
  - [x] store in generic structs/choices;
  - [x] forward to other generic functions when immediate inference solves the call;
  - [x] use generic parameters in local annotations.
- [x] Document behavior rejected until traits/constraints.
  - [x] arithmetic;
  - [x] equality/comparison;
  - [x] field access;
  - [x] receiver calls;
  - [x] template interpolation requiring string-like behavior;
  - [x] external/IO behavior requiring concrete types.
- [x] Document concrete generic aliases.
  ```beanstalk
  StringBox as Box of String
  ```
- [x] Document rejected/deferred generic surfaces in one concise list.
  - [x] no `<T>`;
  - [x] no inline generic sugar;
  - [x] no explicit generic call-site syntax;
  - [x] no argument type-ascription such as `identity(42 Int)`;
  - [x] no generic function values;
  - [x] no higher-order polymorphism;
  - [x] no type values or type-returning functions;
  - [x] no type-level `#if`;
  - [x] no generic receiver methods;
  - [x] no generic external package functions;
  - [x] no recursive generic types;
  - [x] no nested `of`;
  - [x] no parameterized generic aliases in the current implementation;
  - [x] no partial type application.
- [x] Document future constraints as deferred.
  ```beanstalk
  max type A is Ordered |left A, right A| -> A:
      ...
  ;
  ```
  - [x] No `where` syntax.
  - [x] No types-as-values model.

### `docs/compiler-design-overview.md`

- [x] Add a `### Generics contract` subsection under AST construction or type identity.
- [x] State:
  - [x] header parsing records generic declaration metadata;
  - [x] AST registers generic parameter lists in `TypeEnvironment`;
  - [x] AST resolves generic signatures to canonical `TypeId`s;
  - [x] AST stores generic free-function templates;
  - [x] AST validates generic bodies before concrete calls are emitted;
  - [x] AST infers generic function calls from immediate evidence only;
  - [x] AST emits concrete generic function instances before HIR;
  - [x] HIR must never carry unresolved generic executable types;
  - [x] backends never solve generics;
  - [x] borrow validation sees concrete HIR only.

### `docs/src/docs/progress/#page.bst`

- [x] Update or split generic matrix rows so status is explicit.
  - [x] `Generic type infrastructure`: Supported.
  - [x] `Generic structs`: Supported.
  - [x] `Generic choices`: Supported.
  - [x] `Generic free functions`: Supported or Partial for inference-only free functions, depending on current confidence.
  - [x] `Trait bounds on generics`: Deferred.
  - [x] `Explicit generic call-site application`: Rejected / Not Alpha.
  - [x] `Generic function values`: Rejected / Deferred.
  - [x] `Higher-order polymorphism`: Rejected / Deferred.
  - [x] `Generic receiver methods`: Rejected / Deferred; use free functions.
  - [x] `Concrete generic aliases`: Supported.
  - [x] `Parameterized generic aliases`: Deferred / rejected for current implementation.
  - [x] `Partial type application`: Rejected.
  - [x] `Nested generic type application`: Deferred / rejected; use concrete aliases.
  - [x] `Generic external package functions`: Deferred / rejected for Alpha.
  - [x] `Type values / type-level CTFE`: Not supported.
- [x] Ensure the matrix names rejected call forms: `identity of Int(42)`, `identity<Int>(42)`, `identity[Int](42)`, and `identity(42 Int)`.
- [x] Ensure the matrix states immediate inference only and no nested expected-parameter inference yet.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm no `docs/roadmap/roadmap.md` edit is included.

## Accepted Phase 1 documentation summary

- Added the canonical generics language contract to `docs/language-overview.md`: declaration-site syntax, inference-only calls, inference limits, allowed unconstrained behavior, concrete aliases, rejected surfaces, and future constraint direction.
- Added the compiler generics stage contract to `docs/compiler-design-overview.md`, keeping generic inference and concrete instance emission owned by AST before HIR and borrow validation.
- Updated `docs/src/docs/progress/#page.bst` to split supported, rejected, and deferred generic surfaces, including explicit call-site syntax, generic function values, higher-order polymorphism, receiver methods, concrete and parameterized aliases, partial type application, generic external package functions, and type values/type-level CTFE.
- Aligned `docs/src/docs/generics/#page.bst` with the same final Alpha-facing rules because it is an official docs source page for this feature.

---

# Phase 2 — Generic diagnostics consolidation and unsupported call syntax rejection

## Context

Explicit generic call syntax is not a future Alpha feature. Rejection should be targeted, structured, and consistent. This phase should not introduce new expression forms or AST/HIR nodes.

## Steps

### Diagnostic model

- [x] Add or refine a structured `InvalidGenericInstantiationReason` for explicit call-site type arguments.
  - Preferred shape:
    ```rust
    ExplicitCallTypeArgumentsUnsupported
    ```
  - Keep one general reason unless separate cases need different structured facts.
- [x] Add render text:
  ```text
  Explicit generic call-site type arguments are not supported.
  Add an ordinary type annotation to the receiving declaration or argument instead.
  ```
- [x] Do not route these failures through `CompilerError`.
- [x] Do not add prose-only diagnostics when a structured reason enum is practical.

### Parser/expression rejection

- [x] Reject expression-position `of` after a visible generic function.
  ```beanstalk
  identity of Int(42)
  identity of Int (42)
  ```
  - [x] Prefer a targeted generic diagnostic when the left-hand identifier is a visible generic function.
  - [x] For non-generic values, keep existing syntax/namespace diagnostics if they are already precise.
- [x] Reject angle-bracket call syntax.
  ```beanstalk
  identity<Int>(42)
  ```
  - [x] Ensure it fails with a structured diagnostic and never lowers as comparison syntax.
- [x] Reject square-bracket call syntax.
  ```beanstalk
  identity[Int](42)
  ```
  - [x] Avoid implying square brackets are valid for collections or generic calls; they are templates.
- [x] Reject argument-attached type syntax.
  ```beanstalk
  identity(42 Int)
  ```
  - [x] Ensure this is not treated as two valid positional arguments.
  - [x] Avoid suggesting argument type-ascription exists.
- [x] If detection for a foreign syntax would require noisy speculative parsing, prefer a high-quality existing syntax diagnostic over broad parser indirection.

### Tests

- [x] Update `generic_explicit_call_syntax_still_rejected` or split into focused canonical fixtures.
- [x] Cover:
  - [x] `identity of Int(42)`;
  - [x] `identity of Int (42)`;
  - [x] `identity<Int>(42)`;
  - [x] `identity[Int](42)`;
  - [x] `identity(42 Int)`.
- [x] Add or keep a success companion:
  ```beanstalk
  value Int = identity(42)
  ```
- [x] Assert stable diagnostic codes.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm no explicit generic call path exists in AST or HIR.

## Accepted Phase 2 implementation summary

- Added `InvalidGenericInstantiationReason::ExplicitCallTypeArgumentsUnsupported` with stable `BST-RULE-0057` rendering that tells users to add ordinary type annotations instead of explicit call-site type arguments.
- Rejected visible generic function uses of `of`, `<...>`, and `[...]` before they can be interpreted as generic function values, comparisons, or templates.
- Routed generic function argument parsing through a generic-call context and rejected the narrow `identity(42 Int)` argument-attached type syntax without broad speculative parsing of arbitrary type-looking arguments.
- Replaced the misleading square-bracket fixture name with `generic_explicit_call_square_syntax_rejected`, added focused fixtures for `of`, spaced `of`, angle, and argument-attached syntax, and added the annotated inference success companion.
- Updated the progress matrix to record explicit generic call-site application as rejected/not Alpha with the new fixture coverage.
- Broader custom-type argument ascription such as `identity(value User)` intentionally remains on existing syntax diagnostics because recognizing that form precisely would require speculative parsing in the shared call parser.

---

# Phase 3 — Rejected type-level generic surfaces and alias cleanup

## Context

Concrete generic aliases are useful and supported. Parameterized generic aliases, partial application, nested `of`, and type values are not part of the current implementation. This phase hardens those rejections and removes misleading generic alias metadata paths where practical.

## Steps

### Parameterized generic aliases

- [x] Reject parameterized alias declarations:
  ```beanstalk
  StringMap type A as Map of String, A
  ```
- [x] Choose the earliest clean owner: header parsing rejects parameterized aliases before a valid alias shell is produced.
- [x] Prefer a targeted diagnostic:
  ```text
  Generic type aliases with parameters are not supported.
  Alias a fully concrete generic instance instead.
  ```
- [x] Preserve concrete generic aliases:
  ```beanstalk
  StringBox as Box of String
  ```
- [x] Do not implement alias parameter substitution.
- [x] Do not register generic alias parameter lists in `TypeEnvironment`.

### Remove redundant generic alias metadata if possible

- [x] If parameterized aliases are rejected before `ModuleSymbols` generic metadata registration, remove generic-alias metadata plumbing:
  - [x] remove `GenericDeclarationKind::TypeAlias` if it becomes unused;
  - [x] remove branches that only exist to support generic alias metadata;
  - [x] keep concrete alias visibility through `visible_type_aliases` and `resolved_type_aliases_by_path`.
- [x] If removal would broaden the phase too much, leave only the minimum needed shape and add a clear comment explaining that alias parameters are rejected before semantic use. Not needed because the obsolete alias metadata path was removed outright.
- [x] Update `build_generic_parameter_scope` collision logic if `GenericDeclarationKind::TypeAlias` is removed. Concrete aliases should still be covered by `visible_type_aliases`.

### Partial type application

- [x] Keep partial application rejected.
  ```beanstalk
  StringMap as Map of String
  ```
- [x] Use existing wrong-arity diagnostics if they are precise.
- [x] Add a more specific diagnostic only if current output is unclear.
- [x] Do not introduce placeholder syntax such as `_`.

### Nested `of`

- [x] Keep nested `of` rejected in `type_syntax/parse.rs`.
- [x] Ensure diagnostic clearly says nested generic type application is not supported.
- [x] Document and test the concrete alias workaround:
  ```beanstalk
  InnerInt as Inner of Int
  value Outer of InnerInt = ...
  ```
- [x] Keep/refresh:
  - [x] `generic_nested_of_rejected`;
  - [x] `generic_nested_alias_workaround`.

### Type values

- [x] Reject use of type names as expression values through existing namespace/type misuse paths.
  ```beanstalk
  value = Int
  value = Box of Int
  ```
- [x] Add tests if missing.
- [x] Do not add a `Type` meta-type, `ExpressionKind::TypeValue`, or compile-time type value representation.

### Recursive generic types

- [x] Keep recursive generic type rejections.
- [x] Confirm direct and mutual recursion cases are covered.
- [x] Keep diagnostics tied to layout/indirection being deferred.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm alias cleanup did not break concrete alias support.
- [x] Confirm no half-implemented generic alias substitution code remains.

## Accepted Phase 3 implementation summary

- Parameterized generic aliases are rejected during header parsing with `InvalidDeclarationReason::ParameterizedGenericTypeAlias` and stable `BST-RULE-0043` diagnostics.
- Removed generic type-alias parameter metadata plumbing: `HeaderKind::TypeAlias` now stores only its concrete target, `GenericDeclarationKind::TypeAlias` is gone, and concrete aliases continue through `type_alias_paths`, `visible_type_aliases`, and `resolved_type_aliases_by_path`.
- Added canonical fixtures for parameterized alias rejection, partial type application rejection, bare type-name value misuse, type-application value misuse, and namespace type-member value misuse.
- Preserved existing concrete alias, imported concrete alias, nested `of` rejection, nested alias workaround, and direct/mutual recursive generic rejection coverage.
- Corrected type-name-as-value rejection to use nominal declaration path metadata rather than naming-case heuristics.
- Validation: `cargo check`, `git diff --check`, and `just validate` passed after parent review corrections.

---

# Phase 4 — Generic call inference boundary and evidence model

## Context

Generic inference must stay local and explainable. This phase hardens inference sources and simplifies call inference APIs if the current raw slices/booleans obscure the boundary.

## Steps

### Clarify call expected-context API

- [x] Review `GenericFunctionCallParseInput` in `ast/generic_functions/calls.rs`.
- [x] Replace raw `expected_result_type_ids: &[TypeId]` with a named context if it improves readability:
  ```rust
  enum GenericCallExpectedContext {
      ImmediateResult(Vec<TypeId>),
      None,
  }
  ```
  or an equivalent context struct.
- [x] Keep the API narrow. Do not add fields for later-use inference, outer parameter inference, or whole-program inference.
- [x] In `source_function_calls.rs`, keep expected result evidence only when the generic call is the boundary-leading expression.

### Consolidate binding evidence only where useful

- [x] Review `collect_call_argument_bindings` and `collect_expected_result_bindings`.
- [x] If diagnostics are being enriched in Phase 5, introduce a small evidence struct:
  ```rust
  struct GenericBindingEvidence {
      template_type_id: TypeId,
      concrete_type_id: TypeId,
      location: SourceLocation,
      source: GenericBindingEvidenceSource,
  }
  ```
  Phase 4 did not introduce this helper because the boundary fix only needed
  named expected-result context. Phase 5 still owns richer evidence locations
  if diagnostics need previous/current inference source spans.
- [x] Use one binding-evidence collection path if it reduces duplicated conflict handling.
- [x] Do not introduce generic abstraction that hides the distinction between argument evidence and expected-result evidence in diagnostics.

### Enforce inference boundaries

- [x] Confirm argument inference uses only immediate typed call arguments.
- [x] Confirm expected-result inference uses only immediate closed receiving sites.
- [x] Confirm `none` alone cannot infer an optional inner generic parameter.
- [x] Add/keep rejection for later-use inference:
  ```beanstalk
  items = empty()
  ~items.push(1) -- must not infer A = Int from this later use
  ```
- [x] Add/keep rejection for nested expected-parameter inference:
  ```beanstalk
  save_user(parse_user(text)) -- rejected for now if parse_user's A only comes from save_user's parameter
  ```
- [x] Add/keep accepted intermediate annotation form:
  ```beanstalk
  user User = parse_user(text)
  save_user(user)
  ```

### Tests

- [x] Success cases:
  - [x] argument inference;
  - [x] declaration expected-result inference;
  - [x] empty collection return with explicit receiving type;
  - [x] optional return with explicit receiving type;
  - [x] fallible generic success/error slots where immediate evidence solves parameters.
- [x] Rejection cases:
  - [x] ambiguous return-only generic;
  - [x] later-use inference;
  - [x] nested expected-parameter inference;
  - [x] `none` alone cannot infer optional inner type.
- [x] Diagnostics must suggest ordinary annotations, never explicit generic call syntax.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm inference remains AST expression parsing only.

## Accepted Phase 4 implementation summary

- Added `GenericCallExpectedContext` so generic free-function inference accepts explicit immediate-result evidence or no expected-result evidence instead of raw slices.
- Split expression parsing policy so expected-result evidence is independent from `catch` recovery boundaries: function arguments, casts, collection items, and inferred value-list parsing do not inherit outer expected results, while parenthesized calls at a receiving site keep the receiving-site evidence.
- Cleared inherited expected result types from condition scopes so conditions cannot solve return-only generic calls from surrounding declarations or returns.
- Added canonical fixtures for nested expected-parameter rejection and intermediate-annotation success, and extended the expected-context success fixture to cover parenthesized direct receiving-site inference.
- Existing fixtures continue covering argument inference, declaration expected-result inference, empty collection returns, optional/fallible generic returns, ambiguous return-only rejection, later-use inference rejection, and `none` inference rejection.
- Validation passed: `cargo fmt`, `cargo check`, `cargo run -- tests`, `git diff --check`, and `just validate` (including bench-check, `+6ms avg`).

---

# Phase 5 — Generic diagnostics hardening

## Context

Generic failures should explain what the compiler inferred, where inference came from, and which concrete instantiation failed. Current diagnostics have the right basic shape; this phase enriches them without moving generic logic out of AST.

## Steps

### Cannot-infer diagnostics

- [x] Preserve missing generic parameter names.
- [x] Add help text that suggests receiving type annotations:
  ```text
  Add a type annotation to the receiving declaration, for example `value Int = ...`.
  ```
- [x] Do not suggest explicit generic call syntax.

### Conflict diagnostics

- [x] Reuse `BindingConflict` from `datatypes/generic_bindings.rs`.
- [x] Carry:
  - [x] generic parameter ID/name;
  - [x] existing concrete `TypeId`;
  - [x] replacement concrete `TypeId`;
  - [x] current evidence location;
  - [x] previous evidence location if cheaply available through Phase 4's evidence model.
- [x] Update `InvalidGenericInstantiationReason::ConflictingFunctionArgument` or add a richer variant.
- [x] Render through the diagnostic type context so output can say:
  ```text
  Generic parameter `A` was inferred as both `Int` and `String`.
  ```
- [x] Keep semantic `TypeId`s in payloads. Do not store rendered type strings as semantic facts.

### Instantiated body diagnostics

- [x] Replace or extend `with_generic_instantiation_context` with a context helper that can include:
  - [x] call-site primary label;
  - [x] generic body failure secondary label;
  - [x] generic declaration-site secondary label;
  - [x] substitution context, for example `A = Int, B = String`.
- [x] Introduce a named context struct if it improves clarity:
  ```rust
  struct GenericInstantiationDiagnosticContext {
      call_location: SourceLocation,
      declaration_location: SourceLocation,
      substitutions: Vec<GenericSubstitutionDiagnostic>,
  }
  ```
- [x] Update `emit_generic_function_instance` to pass declaration location and concrete type arguments into the helper.
- [x] Keep rendering at the diagnostic render boundary.

### Recursive instantiation diagnostics

- [x] Keep recursive generic instantiation rejection.
- [x] Add active template/call context only if it can be done without broad stack rendering complexity.
  No broad recursive stack rendering was added in this phase; the existing rejection remains targeted.
- [x] Do not add generic recursion handling that emits partial HIR.

### Tests

- [x] Update conflict tests:
  - [x] same-file conflict;
  - [x] cross-file conflict;
  - [x] facade/import conflict if already covered.
    No existing facade conflict fixture covered this exact diagnostic path, so no new facade-specific fixture was added in Phase 5.
- [x] Update concrete-body failure test:
  - [x] call site is primary;
  - [x] body/declaration context is available;
  - [x] substitution context is rendered or structurally testable.
- [x] Update recursive instantiation test if labels or codes change.
  Recursive labels and codes did not change.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm diagnostics are more specific but do not expose internal synthetic instance paths as user API.

## Accepted Phase 5 implementation summary

- Cannot-infer diagnostics now keep missing parameter names and include guidance for ordinary receiving-site annotations without suggesting explicit generic call syntax.
- Generic inference conflict diagnostics now carry `BindingConflict` facts, semantic `TypeId`s for existing and replacement concrete types, the current evidence location, and a best-effort previous evidence location.
- Conflict rendering resolves concrete type names through `DiagnosticRenderContext`, so messages can name both inferred types without storing rendered strings in payloads.
- Generic instance body diagnostics now use `GenericInstantiationDiagnosticContext` with call-site, declaration-site, and structured substitution labels; instance emission passes declaration location and concrete type arguments into that helper.
- Recursive generic function instantiation remains rejected without emitting partial HIR or adding broad stack rendering.
- Validation passed: `cargo fmt`, `cargo test generic`, `cargo check`, `cargo run -- tests --backend html`, `git diff --check`, and `just validate` (including bench-check, `+5ms avg`).

---

# Phase 6 — Generic function value, receiver, and external generic rejection hardening

## Context

These surfaces should not silently half-work. All should reject early with stable diagnostics and documentation. This phase closes value/method/external routes without adding implementation support.

## Steps

### Generic function values

- [x] Keep immediate `(` requirement for generic source functions in `source_function_calls.rs`.
- [x] Audit all value-position contexts:
  - [x] declaration/assignment initializer: `f = identity`;
  - [x] function argument: `use_fn(identity)`;
  - [x] collection element: `values = {identity}`;
  - [x] return: `return identity`;
  - [x] namespace member: `f = helpers.identity`;
  - [x] grouped import alias: `import @helpers { identity as id }` then `f = id`;
  - [x] facade wrapper surface.
- [x] Ensure all value-position routes produce the same structured reason.
- [x] Message should say:
  ```text
  Generic functions cannot be used as values.
  Call the function directly or write a concrete wrapper function.
  ```
- [x] Do not introduce a generic function value type.

### Generic receiver methods

- [x] Keep generic receiver methods rejected:
  ```beanstalk
  map type A |this Box of A| -> A:
      ...
  ;
  ```
- [x] Reject methods on concrete generic instances:
  ```beanstalk
  value |this Box of Int| -> Int:
      ...
  ;
  ```
- [x] Reject namespaced/imported generic receiver targets if parseable:
  ```beanstalk
  value |this models.Box of Int| -> Int:
      ...
  ;
  ```
- [x] Prefer diagnostic text:
  ```text
  Receiver methods on generic types are not supported.
  Use a free function instead.
  ```
- [x] Do not extend `ReceiverKey` for generic instance identity.
- [x] Do not add generic method lookup or method auto-import through generic aliases.

### External generic functions and types

- [x] Audit JS/provider `@bst.sig` parsing.
- [x] Reject generic-looking external signatures:
  ```js
  /**
   * @bst.sig identity type A |value A| -> A
   */
  ```
- [x] Diagnostic should say:
  ```text
  External package functions cannot be generic.
  Expose concrete external functions or wrap them with source Beanstalk generic functions.
  ```
- [x] Ensure external opaque types reject `of` arguments:
  ```beanstalk
  Canvas of Int
  ```
- [x] Do not add generic metadata to external package definitions or backend glue.

### Tests

- [x] Keep/update generic function value rejection fixtures.
- [x] Keep/update `generic_receiver_rejected` and split if needed.
- [x] Add receiver-on-instantiated-generic rejection if missing.
- [x] Add external generic signature rejection if provider test infrastructure exists.
- [x] Add external opaque type `of` rejection if missing.

### Documentation/matrix

- [x] `language-overview.md`: generic functions are not first-class values; generic receiver methods are unsupported; external generics are unsupported.
  Already aligned from Phase 1; no source-page edit was needed for this phase.
- [x] `progress/#page.bst`: mark each surface as rejected/deferred, not implementation-in-progress.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm no new receiver catalog or external package generic infrastructure was added.

## Accepted Phase 6 implementation summary

- Generic function value diagnostics now use the same structured rejection reason across grouped imports, grouped aliases, namespace members, function arguments, collection elements, returns, and facade-exported generic functions.
- Generic receiver diagnostics now use the final rejection wording, with fixtures for both declaration-site generic receiver methods and receiver methods on concrete generic instances.
- JS external import parsing rejects generic-looking `@bst.sig` preambles and `@bst.opaque ... of ...` declarations without storing generic metadata in package definitions.
- AST type resolution now rejects source-level external opaque type applications such as `Canvas of Int` through a targeted invalid generic instantiation reason instead of treating the external type as a value namespace misuse.
- Validation passed: `cargo fmt`, `cargo test external_js::parser::tests`, `cargo check`, `cargo run -- tests --backend html`, `git diff --check`, and `just validate` (including bench-check, `+5ms avg`).

---

# Phase 7 — Constraint syntax reservation and bounds rejection

## Context

Constraints are planned after traits/interfaces. The only intended syntax direction is compact declaration-site syntax, not `where` and not type values. This phase reserves/rejects the syntax cleanly without implementing constraints.

## Steps

### Diagnostic model

- [x] Add or refine a deferred reason:
  ```rust
  GenericConstraints
  ```
  or:
  ```rust
  TraitBoundsOnGenerics
  ```
- [x] Render:
  ```text
  Generic constraints are deferred until traits/interfaces are implemented.
  ```
- [x] If rejecting `type A is Trait`, optionally include:
  ```text
  The planned constraint form is `type A is Trait`, but traits are not implemented yet.
  ```

### Parser rejection

- [x] Review `parse_generic_parameter_list_after_type_keyword`.
- [x] Reject planned constraint syntax:
  ```beanstalk
  max type A is Ordered |left A, right A| -> A:
      return left
  ;
  ```
  - [x] Detect `is` after a generic parameter if the token stream supports it.
  - [x] Prefer a targeted constraints-deferred diagnostic.
- [x] Reject `where`-like syntax if parseable:
  ```beanstalk
  max type A |left A, right A| -> A where A is Ordered:
      return left
  ;
  ```
- [x] Do not add constraint data fields to `GenericParameterList` unless needed for clear diagnostics now.
- [x] Do not store constraints in `TypeEnvironment` until traits exist.

### Keep unconstrained body validation conservative

- [x] Ensure generic body validation still rejects behavior-dependent operations:
  - [x] arithmetic;
  - [x] comparison/equality;
  - [x] field access;
  - [x] receiver calls;
  - [x] IO/external behavior;
  - [x] template interpolation requiring concrete/string-like behavior.
- [x] Improve messages only if they can mention constraints without implying they currently work.

### Tests

- [x] Add constraint syntax rejection fixture.
- [x] Add no-`where` rejection fixture.
- [x] Keep existing behavior-dependent operation rejection fixtures.
- [x] Assert stable diagnostic codes.

### Documentation/matrix

- [x] `language-overview.md`: constraints are future work after traits; syntax direction is `type A is Trait`; no `where`.
  Already aligned from Phase 1; no source-page edit was needed for this phase.
- [x] `progress/#page.bst`: trait bounds remain deferred; unsupported bounds syntax has structured diagnostics.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm no half-implemented trait/constraint model exists.

## Accepted Phase 7 implementation summary

- Generic declaration-site constraints using `type A is Trait` now stop in the shared generic-parameter parser with a structured deferred-feature diagnostic and planned-syntax guidance.
- `where`-style constraint syntax after function return lists now stops in signature parsing with a targeted deferred-feature diagnostic that states `where` is not the planned generic constraint form.
- No constraint fields were added to `GenericParameterList`, no constraint metadata was registered in `TypeEnvironment`, and no trait/interface model was introduced.
- Existing conservative generic body validation remains unchanged; behavior-dependent operations still reject before concrete generic function instances are emitted.
- Added `generic_constraint_syntax_deferred` and `generic_where_constraint_syntax_rejected` fixtures with stable `BST-DEFERRED-0001` assertions, and updated the progress matrix trait-bounds row.
- Validation passed: `cargo fmt`, `cargo check`, `cargo test generic`, `cargo run --quiet -- tests --backend html`, `git diff --check`, and `just validate` (including bench-check, `+4ms avg`).

---

# Phase 8 — HIR generic boundary, synthetic instance, and deduplication audit

## Context

Generics are AST-owned. HIR must see only concrete TypeIds, concrete functions, and ordinary calls. Synthetic generic instance names are internal and must not become public API.

## Steps

### HIR validation

- [x] Audit HIR validation for unresolved generic parameter `TypeId`s.
- [x] Confirm validation covers:
  - [x] function parameters;
  - [x] returns;
  - [x] locals;
  - [x] struct fields;
  - [x] choice payloads;
  - [x] call result types;
  - [x] expression types where applicable.
- [x] Add invariant checks if missing.
- [x] If unresolved generics reach HIR, report infrastructure failure because user diagnostics should have happened earlier.

### Synthetic generic function instances

- [x] Review `generic_function_instance_path`.
- [x] Keep generated instance paths internal.
- [x] Do not expose synthetic instance paths through:
  - [x] namespace records;
  - [x] grouped imports;
  - [x] facade exports;
  - [x] direct source imports.
- [x] Keep or add tests:
  - [x] namespace synthetic instance inaccessible;
  - [x] direct import of synthetic path rejected;
  - [x] facade cannot re-export synthetic path.

### Instance identity and deduplication

- [x] Confirm `GenericFunctionInstanceKey` uses canonical source function path and canonical `TypeId` arguments.
- [x] Confirm repeated same-module substitutions deduplicate.
- [x] Keep/update `generic_fn_multiple_substitutions_same_module_success`.
- [x] Confirm recursive generic instantiation checks run before an active instance is marked complete.

### Borrow validation assumptions

- [x] Confirm generic instances lower as ordinary functions before borrow validation.
- [x] Confirm borrow validation does not consume generic template state.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Confirm HIR and borrow validation remain generic-free in user-visible semantics.

## Accepted Phase 8 implementation summary

- HIR validation already recursively rejects unresolved generic parameter `TypeId`s through constructed, function, nominal-instance, local, struct-field, function return, function parameter, choice payload, and expression type surfaces; no production invariant gap was found.
- Added focused HIR validation regression tests for function returns, parameter locals, choice payload fields, and expression types.
- Confirmed generic function instance identity is canonical source function path plus canonical `TypeId` arguments, repeated same-module substitutions deduplicate by `GenericFunctionInstanceKey`, and recursive instantiation is checked while the active instance stack is still visible.
- Synthetic generic instance paths remain internal; coverage now includes namespace records, grouped import, direct source symbol-path import, and facade import surfaces.
- Borrow validation consumes ordinary concrete HIR call targets and type facts; it does not consume generic template state.
- Validation passed: `cargo fmt`, `cargo test hir_validation --quiet`, `cargo run --quiet -- tests --backend html`, `cargo check`, `git diff --check`, and `just validate`.

---

# Phase 9 — Canonical test suite and final matrix closeout

## Context

After behavior and docs are hardened, normalize tests around the final language rules. Avoid preserving old exploratory test names or duplicate cases unless they protect distinct behavior.

## Steps

### Canonical success coverage

- [x] Generic identity function.
- [x] Generic wrapper/constructor function.
- [x] Generic function with expected result context.
- [x] Generic function returning a constructed collection.
- [x] Generic optional return with expected context.
- [x] Generic fallible return/error slot.
- [x] Cross-file grouped import generic function call.
- [x] Cross-file namespace generic function call.
- [x] Facade wrapper generic function call.
- [x] Generic struct constructor inference.
- [x] Generic choice constructor, match, and equality.
- [x] Concrete generic alias success.
- [x] Nested generic type workaround through concrete aliases.

### Canonical rejection coverage

- [x] Explicit generic call syntaxes rejected.
- [x] Generic function value use rejected in all value-position routes.
- [x] Later-use inference rejected.
- [x] Nested expected-parameter inference rejected.
- [x] Ambiguous return-only generic rejected.
- [x] Conflicting generic inference rejected with concrete type context.
- [x] `none` alone cannot infer optional inner type.
- [x] Generic receiver method rejected.
- [x] Receiver method on instantiated generic type rejected.
- [x] Parameterized generic alias rejected.
- [x] Partial type application rejected.
- [x] Nested `of` rejected.
- [x] Recursive generic type rejected.
- [x] Trait constraint syntax deferred.
- [x] `where` syntax rejected.
- [x] Generic external function signature rejected if provider fixtures support it.
- [x] Type name used as runtime value rejected.

### Fixture hygiene

- [x] Prefer real Beanstalk snippets over narrow parser-only fragments.
- [x] Prefer integration tests over unit tests for source behavior.
- [x] Use `diagnostic_codes` for failures.
- [x] Avoid excessive rendered text assertions.
- [x] Add output assertions for success cases where practical.
- [x] Remove obsolete duplicate tests or rewrite them into canonical fixtures.

### Final docs and matrix closeout

- [x] Re-read `docs/language-overview.md` generics section.
- [x] Re-read `docs/compiler-design-overview.md` generics contract.
- [x] Re-read `docs/src/docs/progress/#page.bst` generic rows.
- [x] Ensure all deliberately unsupported surfaces are marked `Deferred`, `Rejected`, or `Not supported`; do not leave vague “missing” language.

## Phase closeout

- [x] Complete the common phase closeout checklist.
- [x] Run `just validate` after all fixture/doc changes.
- [x] Optionally build or inspect rendered docs output if documentation rendering is affected.

## Accepted Phase 9 implementation summary

- Audited canonical generic success and rejection fixture coverage; the checklist is covered by existing integration fixtures plus the Phase 7 and Phase 8 additions.
- Renamed stale `generic_nonexported_import_rejected` coverage to `generic_same_module_imported_type_success` because the fixture correctly demonstrates same-module imported generic type support rather than a rejection path.
- Added an output assertion to the renamed same-module imported generic type fixture.
- Failure fixtures use stable diagnostic codes where practical; rendered text assertions remain only for wording-sensitive guidance diagnostics.
- Re-read the generics language section, compiler generics contract, and progress matrix rows. The progress matrix now avoids the stale non-exported-import wording and keeps unsupported surfaces marked as rejected, deferred, or unsupported.
- Validation passed: `cargo run --quiet -- tests --backend html`, `git diff --check`, and `just validate`.

---

# Final implementation acceptance checklist

- [x] Generic declaration syntax is finalized as `name type A, B |...|`.
- [x] Generic calls are inference-only ordinary calls.
- [x] Explicit generic call syntax is rejected with structured diagnostics.
- [x] Generic inference uses immediate arguments and immediate expected result context only.
- [x] Later-use inference is rejected.
- [x] Nested expected-parameter inference is rejected.
- [x] Generic function values are rejected.
- [x] Higher-order polymorphism is documented as unsupported/deferred.
- [x] Generic receiver methods are rejected.
- [x] Receiver methods on instantiated generic types are rejected.
- [x] Concrete generic aliases remain supported.
- [x] Parameterized generic aliases are rejected/deferred for current implementation.
- [x] Partial type application is rejected.
- [x] Nested `of` is rejected/deferred.
- [x] Recursive generic types are rejected/deferred.
- [x] Generic external package functions are rejected/deferred.
- [x] Type values and type-level CTFE are documented as unsupported.
- [x] Constraint syntax `type A is Trait` is reserved/rejected until traits exist.
- [x] `where` syntax is rejected/not documented as planned.
- [x] Unsupported generic syntax never creates AST or HIR generic call/value/type nodes.
- [x] AST owns all generic inference and concrete function-instance emission.
- [x] HIR validation rejects unresolved generic parameter leakage.
- [x] Backends do not solve generics.
- [x] Borrow validation sees concrete HIR only.
- [x] Diagnostics use structured payloads/reasons and stable codes.
- [x] Docs and matrix match the implementation.
- [x] `just validate` passes.

## Final plan review summary

- Final changed-area audit found no blocking correctness, stage-boundary, panic-risk, duplicate-path, or diagnostics-routing issues.
- Audit closeout fixed stale compiler-design wording for type alias declaration shells and marked this final acceptance checklist complete.
- Final beautification was file-local and behavior-preserving. It left `generic_functions/calls.rs` unchanged, clarified generic instantiation diagnostic comments, removed one duplicate call-argument error branch, cleaned one timer-output formatting artifact, improved JS signature-parser comments, and moved HIR validation helpers above the tests that use them.
- The roadmap item was removed from active TODOs and recorded as complete in `docs/roadmap/roadmap.md`.
- Final validation passed with `just validate`.

## Suggested implementation order

1. Phase 0 — Baseline audit and fixture inventory.
2. Phase 1 — Documentation and implementation matrix finalization.
3. Phase 2 — Generic diagnostics consolidation and unsupported call syntax rejection.
4. Phase 3 — Rejected type-level generic surfaces and alias cleanup.
5. Phase 4 — Generic call inference boundary and evidence model.
6. Phase 5 — Generic diagnostics hardening.
7. Phase 6 — Generic function value, receiver, and external generic rejection hardening.
8. Phase 7 — Constraint syntax reservation and bounds rejection.
9. Phase 8 — HIR generic boundary, synthetic instance, and deduplication audit.
10. Phase 9 — Canonical test suite and final matrix closeout.

Each phase is intended to be a single coding-agent-sized chunk with an explicit validation gate. Do not merge phases that touch unrelated compiler stages unless the change is mechanically tiny and the closeout checklist remains explicit.
