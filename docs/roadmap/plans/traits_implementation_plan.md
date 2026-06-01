# Beanstalk Traits Implementation Plan

## Purpose

Implement traits as the final major Alpha language feature while preserving Beanstalk's current compiler architecture:

- header-first declaration discovery;
- `TypeId`-first semantic typing;
- receiver-method-based behaviour;
- explicit module/facade visibility;
- JS as the Alpha runtime backend;
- structured diagnostics for every user-facing source failure.

Traits must stay explicit, nominal, and readable. Do not introduce implicit structural conformance, hidden output coercion, Rust-style trait solving, or backend-side semantic rediscovery.

## Repo anchors

Primary docs:

- `docs/src/docs/traits/#page.bst`
- `docs/src/docs/generics/#page.bst`
- `docs/src/docs/progress/#page.bst`
- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/codebase-style-guide.md`
- `docs/roadmap/roadmap.md`

Primary compiler files:

- `src/compiler_frontend/reserved_trait_syntax.rs`
- `src/compiler_frontend/keywords.rs`
- `src/compiler_frontend/tokenizer/tokens.rs`
- `src/compiler_frontend/headers/types.rs`
- `src/compiler_frontend/headers/file_parser.rs`
- `src/compiler_frontend/headers/header_dispatch.rs`
- `src/compiler_frontend/headers/module_symbols.rs`
- `src/compiler_frontend/headers/import_environment.rs`
- `src/compiler_frontend/declaration_syntax/generic_parameters.rs`
- `src/compiler_frontend/declaration_syntax/signature_members.rs`
- `src/compiler_frontend/declaration_syntax/type_syntax/`
- `src/compiler_frontend/datatypes/`
- `src/compiler_frontend/type_coercion.rs`
- `src/compiler_frontend/ast/receiver_methods.rs`
- `src/compiler_frontend/ast/module_ast/environment/`
- `src/compiler_frontend/ast/module_ast/scope_context.rs`
- `src/compiler_frontend/ast/expressions/`
- `src/compiler_frontend/ast/generic_functions.rs`
- `src/compiler_frontend/hir/`
- `src/backends/js/`
- backend feature/reachability validation used by HTML-Wasm unsupported-feature checks
- `tests/cases/manifest.toml`

## Style and implementation constraints

- Follow `docs/codebase-style-guide.md` strictly.
- Use `CompilerDiagnostic` for all user-facing syntax/type/rule/import/borrow failures.
- Keep `CompilerError` for internal infrastructure or invariant failures only.
- Carry semantic `TypeId`s in type diagnostics; render through diagnostic context.
- Do not make semantic decisions from `DataType` spelling.
- Prefer context/input structs over long parameter lists.
- Prefer explicit loops for multi-stage validation.
- Do not add compatibility shims for old reserved-trait APIs.
- Move tests out of production files. Existing tests inside `reserved_trait_syntax.rs` should be migrated or deleted with the module.
- Use `just validate` as the default final validation. Phase-local `cargo test ...` runs are acceptable for faster iteration.

---

# Final v1 design contract

## Trait declarations

```bst
DISPLAYABLE must:
    display |This| -> String
;
```

Rules:

- `TRAIT must:` declares a trait contract.
- Trait names are all-caps identifiers. Invalid casing is an error.
- Trait declarations are top-level declarations and follow normal import/facade visibility.
- Trait blocks contain method requirements only.
- Empty marker traits are valid.
- Default methods, associated types, associated constants, static requirements, trait inheritance, generic traits, and generic trait methods are deferred.

## `This`

```bst
RESETTABLE must:
    reset |~This|
;
```

Rules:

- Trait requirements use `This` or `~This`, not lowercase `this`.
- Every v1 requirement must start with `This` or `~This`.
- `This` means immutable receiver requirement.
- `~This` means mutable receiver requirement and is valid only in the first parameter.
- Later `This` occurrences may appear in parameter or return positions and mean the implementing concrete type.
- `This` outside trait declarations is invalid.
- Composed `This` forms such as `{This}`, `This?`, and `This of T` are deferred.

## Explicit conformance

```bst
Circle must DISPLAYABLE
Circle must DISPLAYABLE, SERIALIZABLE
```

Rules:

- `Type must TRAIT` declares explicit conformance.
- There is no implicit structural conformance.
- Matching methods without `Type must TRAIT` are not enough.
- Conformance declarations are bodyless and top-level only.
- They are newline-terminated; do not write a trailing semicolon.
- A comma continues the conformance trait list across the current logical line.
- `Type must TRAIT,` is invalid.
- Conformance declarations may appear before or after the type, trait, and methods; validation runs after the relevant module metadata is built.

## Conformance ownership

A conformance is classified by the target type at the declaration site.

### Canonical evidence

Canonical evidence is produced when the target is a nominal struct, choice, or generic type constructor declared in the same file as the conformance.

Rules:

- Canonical evidence is visible wherever both the target type and trait are visible.
- Cross-module use requires the module facade to expose both the type and trait.
- A transparent facade type alias may make the underlying nominal target visible, but aliases never create new conformance targets.
- The trait may be local or imported.
- Canonical conformances are checked against methods declared in the same file as the conformance.

### File-local extension evidence

File-local extension evidence is produced when the target is a builtin, imported type, external opaque type, or any type not declared in the current file.

Rules:

- It is usable only in the declaring file.
- It is never exported as reusable evidence.
- It cannot override visible canonical evidence.
- It cannot satisfy public API obligations outside the file.
- It may construct/export a dynamic trait value; the value carries dispatch evidence, but the conformance fact does not escape.
- File-local extension receiver methods are allowed only in the declaring file and only as local extension methods for this evidence.
- Same-file direct calls to those extension methods are allowed if they resolve through the local receiver catalog, but they are never importable or exportable.

## Aliases and generic instances

- Type aliases cannot conform.
- Aliases inside method signatures are allowed; matching compares canonical resolved type identity.
- Specialized generic instance conformance is deferred:

```bst
Box of Int must DISPLAYABLE -- deferred
```

- Generic nominal conformance is base-level and unconditional:

```bst
Box must DISPLAYABLE
```

This applies to every valid `Box of T` instance.

## Requirement matching

Conformance uses strict signature matching.

Rules:

- Method name must match exactly.
- Receiver type and receiver mutability must match exactly.
- Non-receiver parameter count, value mode, and resolved type must match exactly.
- Return count, return type, and return channel must match exactly.
- Parameter names are not part of the contract.
- Duplicate method names inside one trait are invalid.
- No overloads.
- Requirement defaults and return aliases are rejected in v1.

## Generic trait bounds

Generic bounds use `is`, not `must`.

```bst
render type Item is DISPLAYABLE |item Item| -> String:
    return item.display()
;
```

Rules:

- `Type must TRAIT` = explicit conformance declaration.
- `type T is TRAIT` = static generic constraint.
- `and` separates multiple bounds on one parameter.
- `,` separates generic parameters.
- `where` clauses remain rejected.
- Bounds apply to generic functions, structs, and choices.
- Dynamic trait values do not satisfy static generic bounds.
- Initial lowering may monomorphize existing generic instances, but source semantics must remain compatible with a later witness/dictionary strategy.

## Static trait method calls

Trait evidence can make trait-surface methods callable without exporting the concrete receiver method itself.

Resolution rule:

1. Resolve ordinary visible concrete receiver methods first.
2. If no ordinary method is visible, resolve a unique method from visible trait evidence for the receiver type.
3. If multiple visible trait evidence surfaces expose the same method name/shape, reject as ambiguous.
4. Generic `type T is TRAIT` and dynamic trait calls use trait evidence directly.

This preserves normal direct-method visibility while allowing modules to expose behaviour through traits without exporting implementation methods.

## Dynamic trait values

A trait name in a normal type annotation means a dynamic trait value.

```bst
draw |shape DISPLAYABLE|:
    shape.display()
;
```

This is distinct from a generic bound:

```bst
draw_static type Shape is DISPLAYABLE |shape Shape|:
    shape.display()
;
```

Rules:

- `value TRAIT` = dynamic trait value; concrete type erased.
- `type T is TRAIT` = static generic constraint; concrete type preserved.
- A bare trait annotation never means an implicit generic parameter.
- Dynamic trait values are opaque owning wrappers in v1.
- No downcasting, reflection, pattern matching on hidden concrete type, or borrowed trait views in v1.
- Dynamic values expose only their trait surface.
- Dynamic values are runtime-only; no dynamic wrappers in `#` constants or const-folded template fragments.

## Dynamic-safe vs bound-only traits

All traits can be generic bounds. Only dynamic-safe traits can be value types.

A trait is dynamic-safe only if:

- no requirement returns `This`;
- no requirement takes `This` except as the receiver;
- no requirement is generic;
- no associated-type, default-method, or compile-time-only requirement exists;
- every required method can be called without recovering erased concrete identity.

Dynamic-safety diagnostics must be unusually clear. They should identify the offending requirement and suggest `type T is TRAIT` when static dispatch is intended.

## Dynamic coercion

Concrete values coerce to dynamic trait values only at explicit typed boundaries:

- annotated declarations;
- function arguments;
- function returns;
- struct fields;
- choice payloads;
- explicitly typed collection elements.

No unannotated local or collection literal infers a trait type.

## Backend scope

- Static traits and generic bounds are frontend semantics and should remain backend-independent.
- Dynamic trait value runtime lowering is JS-only for v1.
- WASM/HTML-Wasm must reject reachable dynamic trait runtime operations with structured unsupported-backend diagnostics.
- Unused dynamic trait functions should follow existing reachable-feature validation policy.

## `DISPLAYABLE` scaffold

Add a compiler/core-owned `DISPLAYABLE` trait scaffold only:

```bst
DISPLAYABLE must:
    display |This| -> String
;
```

Rules:

- Use it for trait registration and test fixtures.
- Do not add automatic primitive `DISPLAYABLE` conformances unless a later dedicated test explicitly requires isolated compiler-owned metadata.
- Prefer user-authored conformances in trait tests.
- Do not change `io(...)`, template interpolation, or string-content coercion.
- Future output coercion through `DISPLAYABLE` is a separate language/core behaviour task.
- `EQUATABLE`, ordering, hashing, iteration, serialization, formatting, deriving, and operator traits are deferred.

## Operators

- No operator-to-trait integration in v1.
- Use `is` for equality syntax in examples, never `==`.
- Mathematical operators will not support overloading.
- Boolean/equality keyword integration may be reconsidered later only under a separate operator plan.

---

# Architecture

## New trait subsystem

Add a focused trait subsystem under `src/compiler_frontend/traits/`:

```text
traits/
├── mod.rs
├── ids.rs
├── syntax.rs
├── definitions.rs
├── environment.rs
├── evidence.rs
├── visibility.rs
├── dynamic_safety.rs
└── diagnostics.rs
```

Keep responsibilities narrow:

- `syntax.rs`: parse-only shells and remapping helpers.
- `definitions.rs`: resolved trait/evidence data structures.
- `environment.rs`: resolved trait declarations and lookup by path/ID.
- `evidence.rs`: conformance validation and method maps.
- `visibility.rs`: evidence visibility derived from existing imports/facades.
- `dynamic_safety.rs`: dynamic-safe/bound-only classification.
- `diagnostics.rs`: typed diagnostic reason helpers only.

Do not put runtime wrapper representation in the trait environment. Runtime representation belongs in HIR/backend phases.

## Data flow

```text
Tokens
  -> Header shells
  -> Module symbols/import visibility
  -> AST trait environment
  -> Receiver method catalog
  -> Evidence validation
  -> Static trait-bound calls
  -> Dynamic trait TypeIds/coercion AST nodes
  -> Explicit HIR dynamic trait operations
  -> JS lowering or WASM unsupported-backend validation
```

## Simplification rules

- Replace `reserved_trait_syntax.rs`; do not keep it as a second parser.
- Reuse `signature_members.rs` for trait requirement signatures by adding a shared signature mode rather than cloning function-signature parsing.
- Extend `GenericParameterList` with bounds; do not create a parallel generic-bound side table unless a borrow/stage boundary forces it.
- Generalize the existing receiver catalog; do not create a separate trait-only method catalog.
- Prefer a semantic receiver key that can resolve to `TypeId` for matching, with source path metadata retained for locality and diagnostics.
- Derive evidence visibility from `ModuleSymbols` / `HeaderImportEnvironment`; do not add evidence import syntax.
- Put dynamic trait value types in `TypeEnvironment` as real semantic `TypeId`s, but keep trait declarations/evidence in the trait environment.
- Insert dynamic coercion before HIR. Backends must lower explicit HIR facts, not rediscover traits.

---

# Implementation phases

## Phase 0 — Baseline audit and stale-surface inventory

### Goal

Map the current deferred/reserved implementation before changing behaviour.

### Tasks

- [ ] Read the repo anchors listed above.
- [ ] Inventory current tests for:
  - [ ] reserved `must`;
  - [ ] reserved `This`;
  - [ ] deferred generic constraints;
  - [ ] generic receiver rejection;
  - [ ] receiver-method import/visibility behaviour.
- [ ] Identify stale comments that will become wrong once traits are implemented:
  - [ ] `reserved_trait_syntax.rs` file docs;
  - [ ] `keywords.rs` comments for `must` / `This`;
  - [ ] `tokens.rs` comments for `Must` / `TraitThis`;
  - [ ] `signature_members.rs` reserved-trait branches;
  - [ ] `type_syntax/parse.rs` reserved-trait branches;
  - [ ] deferred-feature reason names for trait declarations/generic constraints.
- [ ] Record which fixtures should become success cases and which should remain malformed diagnostics.

### Checkpoint

- [ ] No planned user-facing trait diagnostic routes through `CompilerError`.
- [ ] No planned semantic check uses `DataType` equality.
- [ ] Existing test baseline is known.
- [ ] Run `just validate` or, if too broad for this checkpoint, run `cargo test` plus the current integration test runner.

## Phase 1 — Syntax shells and header integration

### Goal

Parse trait declarations and bodyless conformance declarations as real headers.

### Tasks

- [ ] Add trait parse shells in `src/compiler_frontend/traits/syntax.rs` or equivalent:
  - [ ] `TraitDeclarationSyntax`;
  - [ ] `TraitRequirementSyntax`;
  - [ ] `TraitThisUsage`;
  - [ ] `TraitConformanceSyntax`;
  - [ ] `TraitReferenceSyntax`;
  - [ ] `ConformanceTargetSyntax`.
- [ ] Extend `HeaderKind`:
  - [ ] `Trait { requirements }`;
  - [ ] `TraitConformance { target, traits }`.
- [ ] Add remapping for every new header payload.
- [ ] Update header dispatch for `Symbol must`:
  - [ ] `must:` => trait declaration;
  - [ ] `must TRAIT` => conformance declaration.
- [ ] Update duplicate-header detection so `name must ...` is a declaration start.
- [ ] Parse conformance declarations:
  - [ ] newline-terminated;
  - [ ] comma-separated trait list;
  - [ ] no semicolon;
  - [ ] targeted trailing-comma diagnostic;
  - [ ] targeted missing-trait diagnostic.
- [ ] Parse trait declarations:
  - [ ] block closed by `;`;
  - [ ] empty marker blocks allowed;
  - [ ] no method bodies;
  - [ ] no lowercase `this`;
  - [ ] first parameter must be `This` or `~This`.
- [ ] Refactor signature parsing with a shared mode, for example:
  - [ ] `SignatureTerminator::BodyColon` for functions;
  - [ ] `SignatureTerminator::LineOrBlockEnd` for trait requirements;
  - [ ] policy flags for defaults and return aliases.
- [ ] Accept `~This`, not `This ~`.
- [ ] Reject `This` outside trait declarations with a targeted diagnostic.
- [ ] Remove the old reserved parser once replacement coverage exists.

### Tests

- [ ] Valid trait declaration.
- [ ] Valid empty marker trait.
- [ ] Valid `This` and `~This` requirements.
- [ ] Valid single and multi-trait conformance.
- [ ] Comma continuation for conformance lists.
- [ ] Rejections for lowercase `this`, missing receiver, `~This` outside receiver, semicolon after conformance, trailing comma, malformed `This` use, and invalid trait name casing.

### Checkpoint

- [ ] No long token-index rewinding paths remain.
- [ ] Trait parser tests live outside production files.
- [ ] Shared signature parser remains readable and still handles ordinary functions.
- [ ] Run `cargo fmt`, targeted parser tests, and current function-signature tests.

## Phase 2 — Trait identity, visibility, and `DISPLAYABLE` scaffold

### Goal

Resolve trait declarations into compile-time metadata with stable IDs and normal visibility.

### Tasks

- [ ] Add `TraitId` and requirement IDs.
- [ ] Add resolved trait definitions:
  - [ ] canonical path/name;
  - [ ] source file;
  - [ ] requirements;
  - [ ] resolved parameter/return `TypeId`s;
  - [ ] receiver mutability;
  - [ ] source locations;
  - [ ] dynamic-safety classification placeholder;
  - [ ] public/private visibility metadata.
- [ ] Extend module symbols/import/facade metadata for traits.
- [ ] Enforce all-caps trait names at declaration and reference sites.
- [ ] Resolve requirement signatures through the existing type-resolution path.
- [ ] Reject exported traits that expose private/non-exported method-surface types.
- [ ] Register `DISPLAYABLE` as compiler/core-owned trait metadata only.
- [ ] Do not add output coercion or template/`io` behaviour.
- [ ] Add typed diagnostics for unknown trait, duplicate trait, invalid trait name, private type leak, invalid `This`, and unsupported trait feature.

### Tests

- [ ] Trait resolves to stable identity.
- [ ] Imported trait reference resolves.
- [ ] Private trait visible only where imported/declared.
- [ ] Exported trait private-surface leak rejected.
- [ ] Duplicate trait rejected.
- [ ] `DISPLAYABLE` scaffold available for tests without changing `io` or templates.

### Checkpoint

- [ ] Trait definitions are not represented as `DataType`.
- [ ] Type diagnostics carry `TypeId` where relevant.
- [ ] Trait name rendering stays interned until diagnostic render.
- [ ] Run trait resolution tests and import/facade tests.

## Phase 3 — Receiver method generalization

### Goal

Make the existing receiver-method system capable of satisfying trait requirements without building a parallel method system.

### Tasks

- [ ] Generalize receiver keys/catalog entries to support:
  - [ ] structs;
  - [ ] choices;
  - [ ] builtin scalar receivers;
  - [ ] imported/external opaque targets for file-local extension methods;
  - [ ] generic nominal receiver forms needed for conformance.
- [ ] Prefer semantic matching by `TypeId` after resolution.
- [ ] Retain source-file/path metadata for same-file rules and diagnostics.
- [ ] Add choice receiver methods.
- [ ] Preserve struct field/method conflict checks for structs only.
- [ ] Add file-local extension receiver methods for imported/external targets.
- [ ] Keep extension receiver methods file-local and non-exportable.
- [ ] Add limited generic receiver methods for generic nominal conformance:
  - [ ] `display type T is DISPLAYABLE |this Box of T| -> String: ... ;`
  - [ ] method generic parameters must align with receiver generic parameters;
  - [ ] unsupported generic receiver methods still reject with targeted diagnostics.
- [ ] Update comments/docs that currently say receiver methods are only structs/builtin scalars.

### Tests

- [ ] Choice receiver method declaration and call.
- [ ] Choice receiver method satisfies a trait.
- [ ] Extension receiver method on imported/external target satisfies file-local evidence.
- [ ] Extension receiver method is not visible from another file.
- [ ] Generic receiver method satisfies generic nominal conformance.
- [ ] Unsupported generic receiver method forms still reject.

### Checkpoint

- [ ] Receiver catalog remains one deterministic indexed structure.
- [ ] No trait-specific receiver catalog exists.
- [ ] Existing receiver-method import/visibility tests still pass.
- [ ] Run receiver-method, choice, external receiver, and generic receiver tests.

## Phase 4 — Conformance evidence validation

### Goal

Validate explicit conformances and store selected method evidence.

### Tasks

- [ ] Define evidence metadata:
  - [ ] `TraitEvidenceId`;
  - [ ] `TraitEvidenceKind::{Canonical, FileLocalExtension, Builtin}`;
  - [ ] target semantic identity;
  - [ ] trait ID;
  - [ ] source file;
  - [ ] declaration location;
  - [ ] requirement-to-method map.
- [ ] Resolve conformance targets through normal type visibility.
- [ ] Classify canonical vs file-local extension evidence.
- [ ] Reject alias targets.
- [ ] Reject specialized generic instance targets with a deferred-feature diagnostic.
- [ ] Reject conformance declarations in `#mod.bst`.
- [ ] Validate duplicates/conflicts:
  - [ ] duplicate canonical evidence is an error;
  - [ ] duplicate file-local extension evidence in one file is an error;
  - [ ] file-local extension evidence cannot override visible canonical evidence;
  - [ ] compiler-owned builtin evidence cannot be overridden.
- [ ] Validate each requirement against same-file methods:
  - [ ] method exists;
  - [ ] receiver type/mutability matches;
  - [ ] parameter count/value modes/resolved types match;
  - [ ] return count/types/channels match;
  - [ ] aliases compare by resolved type identity;
  - [ ] parameter names ignored.
- [ ] Store selected method implementation in evidence metadata.
- [ ] Add diagnostics with conformance as primary location and requirement/method as secondary context.

### Tests

- [ ] Struct conformance success.
- [ ] Choice conformance success.
- [ ] Local type conforming to imported trait.
- [ ] File-local extension conformance for builtin/imported/external target.
- [ ] Missing method and every mismatch category.
- [ ] Duplicate canonical evidence.
- [ ] File-local extension override attempt.
- [ ] Alias target rejection.
- [ ] Specialized generic instance rejection.
- [ ] Same-file method requirement enforced.
- [ ] Order-independent type/trait/method/conformance validation.

### Checkpoint

- [ ] Evidence validation runs after trait definitions, nominal types, signatures, and receiver catalog are available.
- [ ] Evidence lookup is indexed; call sites do not scan raw headers.
- [ ] No implicit structural conformance path exists.
- [ ] Run conformance, receiver-method, import/facade, and full frontend pipeline tests.

## Phase 5 — Static generic bounds and static trait calls

### Goal

Implement `type T is TRAIT` bounds for generic functions, structs, and choices, and allow static calls through visible evidence.

### Tasks

- [ ] Extend parsed and canonical generic parameter metadata with trait bounds.
- [ ] Update generic parameter parsing:
  - [ ] `type T is TRAIT`;
  - [ ] `type T is TRAIT_A and TRAIT_B`;
  - [ ] `,` still separates parameters;
  - [ ] `must` in generic parameter lists is rejected with guidance to use `is`;
  - [ ] `where` remains rejected.
- [ ] Resolve bounds during AST environment construction.
- [ ] Validate public generic signatures expose public bound traits.
- [ ] Check evidence at generic instantiation sites.
- [ ] Reject dynamic trait values as static bound substitutions.
- [ ] Allow bound-provided receiver calls in generic bodies.
- [ ] Reject ambiguous method names from multiple bounds.
- [ ] Lower initially through existing concrete generic instance/monomorphized paths.
- [ ] Keep source semantics evidence-based so shared-body witnesses can be added later.
- [ ] Implement evidence-backed concrete receiver fallback:
  - [ ] ordinary visible concrete methods first;
  - [ ] unique visible trait evidence method second;
  - [ ] ambiguity rejected.

### Tests

- [ ] Generic function bound success.
- [ ] Multiple bounds with `and`.
- [ ] Multiple generic parameters with comma.
- [ ] Generic struct/choice bounds and instantiation.
- [ ] Missing evidence at call/instantiation site.
- [ ] Dynamic trait value rejected for static bound.
- [ ] Ambiguous bound method rejected.
- [ ] Cross-file, namespace, grouped import, and facade-wrapper bound calls.
- [ ] Evidence-backed concrete receiver call through public type+trait without exported method.
- [ ] Direct concrete receiver visibility still works as before.

### Checkpoint

- [ ] Generic inference remains immediate; no later-use inference added.
- [ ] Bounds are resolved once and carried as metadata.
- [ ] No string-based trait lookup during generic body emission.
- [ ] Run generic, trait-bound, import/facade, and JS integration tests.

## Phase 6 — Dynamic trait type annotations and safety classification

### Goal

Allow trait names in type positions as dynamic trait values when the trait is dynamic-safe.

### Tasks

- [ ] Implement dynamic-safety classification:
  - [ ] `DynamicSafe`;
  - [ ] `BoundOnly { reason, offending_requirement }`.
- [ ] Add rich diagnostics for bound-only trait type annotations.
- [ ] Resolve visible all-caps trait names in type annotations to dynamic trait value `TypeId`s.
- [ ] Add a `TypeEnvironment` entry for dynamic trait value types.
- [ ] Keep trait definitions/evidence outside `TypeEnvironment`; only dynamic trait values are types.
- [ ] Ensure dynamic trait type names render correctly in diagnostics.
- [ ] Reject dynamic trait composition in type positions.
- [ ] Reject dynamic trait values in `#` constants and const-folded template contexts.
- [ ] Ensure dynamic trait value types do not satisfy static bounds.

### Tests

- [ ] `DISPLAYABLE` dynamic annotation success.
- [ ] Bound-only trait annotation rejection with offending requirement.
- [ ] Bound-only trait still valid as generic bound.
- [ ] Dynamic trait composition rejected.
- [ ] Dynamic trait const rejected.
- [ ] Dynamic trait value rejected as static generic substitution.

### Checkpoint

- [ ] Dynamic trait values are `TypeId`-backed.
- [ ] Trait declarations remain compile-time metadata.
- [ ] Diagnostics clearly distinguish dynamic annotations from static bounds.
- [ ] Run type syntax, trait dynamic-safety, and generic tests.

## Phase 7 — Explicit dynamic coercion in AST and HIR

### Goal

Make concrete-to-dynamic trait conversion explicit before backend lowering.

### Tasks

- [ ] Extend type coercion policy for `Concrete -> DynamicTraitValue` only at explicit typed boundaries.
- [ ] Coercion metadata must include:
  - [ ] source concrete `TypeId`;
  - [ ] target dynamic trait `TypeId`;
  - [ ] target trait ID;
  - [ ] selected evidence ID;
  - [ ] source location.
- [ ] Insert explicit AST expression/coercion node.
- [ ] Add explicit HIR expression(s), for example:
  - [ ] `ConstructDynamicTraitValue`;
  - [ ] `CallDynamicTraitMethod` or equivalent dispatch form.
- [ ] Support explicit boundaries:
  - [ ] annotated declarations;
  - [ ] function arguments;
  - [ ] returns;
  - [ ] struct fields;
  - [ ] choice payloads;
  - [ ] explicitly typed collection elements.
- [ ] Reject unannotated inference to trait values.
- [ ] Reject least-common-trait collection inference.
- [ ] Enforce trait-only method surface for dynamic values.
- [ ] Enforce mutable dynamic method calls through mutable access paths.
- [ ] Keep dynamic wrapper construction out of const contexts.

### Tests

- [ ] Declaration, argument, return, field, payload, and collection coercion success.
- [ ] Unannotated local remains concrete.
- [ ] Mixed unannotated collection does not infer trait element type.
- [ ] Missing evidence at coercion site.
- [ ] Bound-only trait coercion rejected.
- [ ] Dynamic trait method call success.
- [ ] Concrete-only method through dynamic value rejected.
- [ ] Mutable dynamic method requires mutable access.

### Checkpoint

- [ ] AST owns every dynamic coercion decision.
- [ ] HIR carries explicit dynamic operations.
- [ ] HIR still uses `TypeId` for semantic expression types.
- [ ] Run type-coercion, HIR, and dynamic trait tests.

## Phase 8 — JS lowering and WASM diagnostics

### Goal

Lower dynamic trait values in JS and reject reachable dynamic trait runtime operations for WASM/HTML-Wasm.

### Tasks

- [ ] Design compact JS wrapper representation:
  - [ ] payload;
  - [ ] evidence marker or method table;
  - [ ] deterministic helper names.
- [ ] Generate method tables from evidence metadata carried through HIR/module data.
- [ ] Lower dynamic construction to wrapper creation.
- [ ] Lower dynamic method calls to method-table dispatch.
- [ ] Emit helpers only when reachable dynamic operations exist.
- [ ] Add debug guards if consistent with existing JS debug glue policy.
- [ ] Add backend validation for reachable dynamic trait HIR operations in WASM/HTML-Wasm.
- [ ] Reject reachable dynamic construction/dispatch/wrappers with structured unsupported-backend diagnostics.
- [ ] Do not reject static trait declarations, conformances, or generic bounds for WASM merely because dynamic runtime lowering is deferred.

### Tests

- [ ] JS runtime dynamic argument smoke test.
- [ ] JS runtime dynamic return smoke test.
- [ ] JS runtime dynamic collection smoke test.
- [ ] Public dynamic value hiding private concrete type.
- [ ] Public dynamic value carrying file-local extension evidence.
- [ ] Unused dynamic helper not emitted when no reachable dynamic operations exist.
- [ ] Reachable HTML-Wasm dynamic operation rejected.
- [ ] Unreachable dynamic helper ignored by reachability validation, matching existing backend policy.

### Checkpoint

- [ ] JS backend does not resolve traits or method shapes by itself.
- [ ] WASM diagnostics happen before backend lowering panics are possible.
- [ ] Existing JS output remains deterministic.
- [ ] Run JS, HTML project, backend artifact, and WASM unsupported-feature tests.

## Phase 9 — Documentation, roadmap, and stale-comment cleanup

### Goal

Make the implemented surface and deferred surface clear to users and future agents.

### Tasks

- [ ] Rewrite `docs/src/docs/traits/#page.bst` from design draft to Alpha docs.
- [ ] Include concise sections for:
  - [ ] declarations;
  - [ ] `This` / `~This`;
  - [ ] explicit conformance;
  - [ ] canonical vs file-local extension evidence;
  - [ ] method matching;
  - [ ] marker traits;
  - [ ] static generic bounds;
  - [ ] dynamic trait values;
  - [ ] dynamic-safe vs bound-only;
  - [ ] explicit dynamic coercion boundaries;
  - [ ] module/facade visibility;
  - [ ] `DISPLAYABLE` scaffold limits;
  - [ ] deferred features.
- [ ] Update `docs/src/docs/generics/#page.bst`:
  - [ ] trait bounds no longer deferred;
  - [ ] `is` bound syntax;
  - [ ] `and` for multiple bounds;
  - [ ] comma for parameter separation;
  - [ ] `where` still rejected;
  - [ ] dynamic trait values do not satisfy static bounds.
- [ ] Update `docs/language-overview.md` concisely:
  - [ ] syntax summary row for traits;
  - [ ] concise traits section;
  - [ ] generic-bound text updated from future direction to implemented surface;
  - [ ] no broad tutorial duplication.
- [ ] Update `docs/compiler-design-overview.md`:
  - [ ] trait environment ownership;
  - [ ] evidence validation placement;
  - [ ] dynamic trait HIR operations;
  - [ ] JS/WASM dynamic boundary.
- [ ] Update `docs/src/docs/progress/#page.bst`:
  - [ ] traits row moved out of deferred;
  - [ ] trait bounds row updated;
  - [ ] generic receiver methods marked partial;
  - [ ] dynamic trait values marked JS-supported/WASM-deferred as appropriate;
  - [ ] `DISPLAYABLE` output coercion deferred;
  - [ ] operator/standard-trait ecosystem deferred.
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] remove “Traits (plan todo)” after implementation;
  - [ ] add follow-ups for deferred trait ecosystem work.
- [ ] Clean stale comments in code files identified in Phase 0.

### Required docs wording

Use this wording prominently:

```text
A trait name in a normal type annotation means a dynamic trait value.
A trait name in a generic bound constrains a concrete generic parameter.
These are different features.
```

Also state clearly:

- `DISPLAYABLE` does not yet change `io(...)` or template interpolation.
- `Type must TRAIT` is required for implementation.
- Method shape alone is not conformance.
- Dynamic trait values are JS-supported first; WASM dynamic lowering is deferred.
- Operator-to-trait integration is deferred.

### Checkpoint

- [ ] Docs do not imply implicit conformance.
- [ ] Docs do not use `==` examples.
- [ ] Docs do not imply mathematical operator overloading.
- [ ] Docs do not imply automatic `DISPLAYABLE` string coercion.
- [ ] Run docs/site generation checks if available.

## Phase 10 — Final hardening and validation

### Goal

Remove stale paths, consolidate duplicate logic, and verify the feature as a coherent Alpha surface.

### Tasks

- [ ] Delete or reduce `reserved_trait_syntax.rs` to nothing if all uses are gone.
- [ ] Remove obsolete deferred diagnostic variants that no longer describe current behaviour.
- [ ] Review new trait code for:
  - [ ] duplicated type-resolution logic;
  - [ ] `DataType` semantic comparisons;
  - [ ] string-based resolved trait lookup;
  - [ ] call-site scans over raw headers;
  - [ ] backend semantic rediscovery;
  - [ ] broad generic receiver support beyond the agreed subset;
  - [ ] accidental `io`/template/display coercion changes.
- [ ] Add frontend counters only if useful and consistent with existing instrumentation:
  - [ ] trait declaration count;
  - [ ] conformance count;
  - [ ] static bound count;
  - [ ] dynamic trait coercion count.
- [ ] Prefer integration tests over excessive unit fixtures once parser/resolution pieces are stable.
- [ ] Prune obsolete unit tests that duplicate end-to-end fixtures.
- [ ] Run final validation:
  - [ ] `cargo fmt --check`
  - [ ] `cargo clippy --all-targets --all-features -- -D warnings`
  - [ ] `cargo test`
  - [ ] `cargo run -- tests`
  - [ ] `just validate`
  - [ ] docs build/site generation checks

### Final audit checklist

- [ ] Traits are explicit and nominal only.
- [ ] `must` is only trait declaration/conformance syntax.
- [ ] `is` is generic trait-bound syntax.
- [ ] All-caps trait names are enforced.
- [ ] `This` is trait-local.
- [ ] Conformance validation is strict and same-file-method based.
- [ ] Canonical vs file-local extension evidence rules are enforced.
- [ ] Evidence visibility derives from existing import/facade machinery.
- [ ] Static trait-bound calls work without dynamic runtime support.
- [ ] Dynamic-safe/bound-only split is enforced.
- [ ] Dynamic trait values coerce only at explicit typed boundaries.
- [ ] Dynamic trait operations are explicit in HIR.
- [ ] JS lowers dynamic trait operations.
- [ ] WASM rejects reachable dynamic trait operations cleanly.
- [ ] `DISPLAYABLE` is scaffolded but not magic.
- [ ] Operator integration remains deferred.
- [ ] Docs, progress matrix, and roadmap match implementation reality.

---

# Suggested integration fixture groups

Add cases under `tests/cases/` and register them in `tests/cases/manifest.toml`. Use stable `diagnostic_codes` for negative cases.

## Syntax and headers

- [ ] `trait_declaration_success`
- [ ] `trait_marker_success`
- [ ] `trait_this_receiver_success`
- [ ] `trait_mut_this_receiver_success`
- [ ] `trait_lowercase_this_rejected`
- [ ] `trait_this_outside_trait_rejected`
- [ ] `trait_duplicate_requirement_rejected`
- [ ] `trait_name_not_all_caps_rejected`
- [ ] `trait_conformance_success`
- [ ] `trait_conformance_multi_success`
- [ ] `trait_conformance_semicolon_rejected`
- [ ] `trait_conformance_trailing_comma_rejected`

## Conformance and evidence

- [ ] `trait_conformance_struct_success`
- [ ] `trait_conformance_choice_success`
- [ ] `trait_conformance_imported_trait_success`
- [ ] `trait_conformance_missing_method_rejected`
- [ ] `trait_conformance_wrong_receiver_mutability_rejected`
- [ ] `trait_conformance_parameter_type_mismatch_rejected`
- [ ] `trait_conformance_return_type_mismatch_rejected`
- [ ] `trait_conformance_alias_target_rejected`
- [ ] `trait_conformance_specialized_generic_rejected`
- [ ] `trait_conformance_duplicate_canonical_rejected`
- [ ] `trait_conformance_extension_builtin_success`
- [ ] `trait_conformance_extension_imported_success`
- [ ] `trait_conformance_extension_not_exported_rejected`

## Generic bounds

- [ ] `trait_bound_generic_function_success`
- [ ] `trait_bound_multiple_and_success`
- [ ] `trait_bound_multiple_parameters_success`
- [ ] `trait_bound_missing_evidence_rejected`
- [ ] `trait_bound_dynamic_value_rejected`
- [ ] `trait_bound_ambiguous_method_rejected`
- [ ] `trait_bound_generic_struct_success`
- [ ] `trait_bound_generic_choice_success`
- [ ] `trait_bound_where_rejected`
- [ ] `trait_bound_must_keyword_rejected`
- [ ] `trait_evidence_method_visible_without_concrete_method_export_success`

## Dynamic traits

- [ ] `dynamic_trait_annotation_success`
- [ ] `dynamic_trait_bound_only_rejected`
- [ ] `dynamic_trait_declaration_coercion_success`
- [ ] `dynamic_trait_argument_coercion_success`
- [ ] `dynamic_trait_return_coercion_success`
- [ ] `dynamic_trait_collection_success`
- [ ] `dynamic_trait_unannotated_inference_rejected`
- [ ] `dynamic_trait_missing_evidence_rejected`
- [ ] `dynamic_trait_concrete_method_rejected`
- [ ] `dynamic_trait_mutable_method_requires_mutable_receiver`
- [ ] `dynamic_trait_const_rejected`
- [ ] `dynamic_trait_composition_rejected`

## Visibility and backend

- [ ] `trait_facade_evidence_success`
- [ ] `trait_facade_type_without_trait_rejected`
- [ ] `trait_facade_trait_without_type_rejected`
- [ ] `trait_public_trait_private_surface_rejected`
- [ ] `dynamic_trait_private_type_public_return_success`
- [ ] `dynamic_trait_file_local_evidence_public_value_success`
- [ ] `dynamic_trait_wasm_reachable_rejected`
- [ ] `dynamic_trait_wasm_unreachable_ignored`

---

# Definition of done

Traits are Alpha-complete when:

- [ ] Trait declarations parse, resolve, import, and export.
- [ ] Explicit conformance declarations parse and validate.
- [ ] Canonical and file-local extension evidence rules are enforced.
- [ ] Static generic bounds use `type T is TRAIT` and support `and` bounds.
- [ ] Static trait-bound calls compile and run through JS where the called code is otherwise supported.
- [ ] Evidence-visible trait methods can be called without exporting concrete implementation methods.
- [ ] Choices can have receiver methods and conform to traits.
- [ ] Limited generic receiver methods support generic nominal conformance.
- [ ] Dynamic-safe traits can be used as dynamic value types.
- [ ] Dynamic trait coercion occurs only at explicit typed boundaries.
- [ ] Dynamic trait calls lower through explicit HIR and JS runtime wrappers.
- [ ] WASM dynamic trait values fail with structured unsupported-backend diagnostics.
- [ ] `DISPLAYABLE` exists only as a scaffold and does not affect `io(...)` or templates.
- [ ] Deferred trait ecosystem items are documented in the progress matrix and roadmap.
- [ ] `just validate` passes.
