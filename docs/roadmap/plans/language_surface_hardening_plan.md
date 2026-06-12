# Beanstalk Language Surface Hardening Implementation Plan

## Goal

Harden Beanstalk around a small, static, nominal language surface:

- Traits are **static contracts only**. Trait names are not ordinary value types.
- Runtime heterogeneity uses **choices**, not dynamic trait values / trait objects.
- Source-authored receiver methods are allowed only for **user-defined structs/choices declared in the same source file**.
- Builtin hashmaps stay **scalar-keyed**: `String`, `Int`, `Bool`, `Char` only.
- Fixed collection capacity syntax accepts only **positive integer literals** or **bare visible `#Int` constant names**.
- Features that violate the simplicity target are moved from “deferred” to **outside language design scope**.
- Obsolete compatibility paths, stale comments, old diagnostics, and over-generalized support code are deleted rather than preserved.

This plan is written for an implementation agent. It is checkbox-driven, phase-bounded, and assumes a pre-release language where breaking old behavior is acceptable.

---

## Non-negotiable implementation rules

- [ ] Do not add a legacy mode, compatibility flag, compatibility wrapper, forwarding shim, or migration path for removed language behavior.
- [ ] Do not write diagnostics that mention old behavior or say that a removed feature “used to be allowed.”
- [ ] User-facing source rejections use typed `CompilerDiagnostic` payloads. `CompilerError` remains for infrastructure/internal compiler failures only.
- [ ] Keep AST/HIR/backend boundaries clear. Removed dynamic trait value semantics must not leak through dormant IR nodes or backend metadata.
- [ ] Delete obsolete enum variants, helper structs, runtime helpers, tests, and comments when a feature is removed.
- [ ] Prefer simplifying existing modules over adding a new abstraction layer.
- [ ] Follow `docs/codebase-style-guide.md`: readable control flow, named intermediate values, narrow helpers, stage ownership, no clever Rust, no dead-code suppressions without a current reason.
- [ ] Run `just validate` at every phase gate unless the phase explicitly stops before code changes.

---

## Current repo shape to verify before editing

The repo is active. Treat this as the current anchor from the design review, then verify in Phase 0 before changing code.

| Area | Current shape to verify |
|---|---|
| `README.md` | Alpha project, active development, docs site built from `docs/src`, goals around readability, static typing, borrow checking, backend agnosticism, and few dependencies. |
| `docs/language-overview.md` | Currently documents dynamic trait value annotations, broad receiver extension targets, general fixed-capacity expressions, future `HASHABLE`, and broad trait ecosystem follow-ups. |
| `docs/compiler-design-overview.md` | Currently documents trait dynamic-safety, dynamic trait value `TypeId`s, dynamic coercion insertion, dynamic trait HIR operations, and backend dynamic method tables / Wasm rejection. |
| `docs/roadmap/roadmap.md` | Currently lists dynamic trait runtime lowering, dynamic wrappers in Wasm ABI, `HASHABLE`, generic map keys, and broad trait features as future work. |
| `docs/src/docs/progress/#page.bst` | Current matrix marks traits as partial with dynamic coercion/runtime coverage and hashmaps with future `HASHABLE`/generic-key follow-ups. |
| `docs/src/docs/traits/#page.bst` | Currently teaches dynamic trait values, dynamic-safe traits, file-local evidence, and broad deferred trait ecosystem. |
| `docs/src/docs/generics/#page.bst` | Mostly aligned with constrained generics, but still references dynamic trait values and file-local evidence-backed dispatch. |
| `docs/src/docs/collections/#page.bst` | Capacity example currently uses `capacity + 2`; hashmap section may still say keyed behavior is deferred despite V1 implementation. |
| `docs/src/docs/structs/#page.bst` | Same-file struct method rule exists, but builtin scalar receiver methods are still described as source-style methods. |
| `docs/src/docs/language-overview/#page.bst` | Currently documents grouped receiver-method imports/aliases and `this` in JS signatures. |
| `src/compiler_frontend/ast/receiver_methods.rs` | Current catalog distinguishes canonical methods from `FileLocalExtension`. |
| `src/compiler_frontend/datatypes/*` | Current type system has `DataType::DynamicTrait` and `TypeDefinition::DynamicTrait`. |
| `src/compiler_frontend/hir/*` | Current HIR has dynamic trait construction/dispatch nodes and reachability tracking. |
| `src/compiler_frontend/mod.rs` | Verify whether any `deferred_feature_diagnostics` module exists. The compiler docs may mention a stale path. |

---

## Terminology to standardize

Use these terms consistently in canonical docs, docs-site pages, roadmap, progress matrix, diagnostics, tests, and comments.

| Term | Meaning |
|---|---|
| **Supported** | Implemented language surface expected to parse, type-check, lower, and run on the documented target. |
| **Deferred** | Fits Beanstalk’s design, but is not implemented yet or not part of Alpha. |
| **Outside language design scope** | Intentionally not planned because it would make the language broader, more implicit, more solver-heavy, or harder to reason about. |

Do not describe outside-scope features as “not implemented yet,” “not Alpha,” or “deferred.”

---

# Phase 0 — Preflight audit and synchronization

## Context

The repo may have changed since this plan was authored. This phase establishes the live baseline and prevents coding against stale observations.

## Tasks

### Repo baseline

- [x] Confirm branch, working tree, and commit:

```bash
git status --short
git branch --show-current
git rev-parse HEAD
```

- [x] Create or switch to a focused branch:

```bash
git switch -c language-surface-hardening
```

- [x] Run the validation baseline before edits:

```bash
just validate
```

- [x] Record any pre-existing failures. Do not mix unrelated fixes into this work.

### Full-repo search baseline

Run these from repo root:

```bash
rg -n "dynamic trait|DynamicTrait|dynamic-safe|dynamic_safe|trait object|trait value|dynamic wrapper|dynamic coercion|dynamic dispatch|CallDynamicTrait|ConstructDynamicTrait|reachable_dynamic_trait" .

rg -n "FileLocalExtension|file-local extension|file local extension|file-local evidence|file local evidence|visible_receiver_methods|visible_external_receiver_methods|grouped receiver|receiver method import|receiver-method import|receiver method alias|receiver-method alias|ExtensionOverridesCanonicalMethod" .

rg -n "HASHABLE|hashable|generic key|generic map|user-defined key|user defined key|custom hasher|custom comparer|map equality|map iteration|hashset|Float key|fixed map|capacity map" .

rg -n "capacity expression|capacity expressions|capacity \\+|ordinary compile-time arithmetic|compile-time arithmetic|fold to a positive|fixed capacity" docs src tests libraries README.md

rg -n "associated type|associated constant|default method|trait inheritance|trait composition|generic trait|generic trait method|specialization|conditional conformance|blanket conformance|negative conformance|operator-to-trait|downcast|reflection|DISPLAYABLE|DEBUG_DISPLAY" docs src tests libraries README.md

rg -n "deferred_feature_diagnostics|DeferredFeature|deferred feature|not implemented yet|not part of Alpha|Not Alpha|outside scope|outside language design scope" src docs tests libraries README.md
```

### Audit note

- [x] Create a local temporary note, not committed unless it becomes useful:

```text
/tmp/beanstalk-language-surface-hardening-audit.md
```

Record:

- [x] current commit SHA;
- [x] validation baseline;
- [x] dynamic trait files and tests found;
- [x] receiver extension/import files and tests found;
- [x] `HASHABLE` / generic-map future mentions found;
- [x] fixed-capacity expression files and tests found;
- [x] docs pages requiring changes;
- [x] diagnostic owners and likely test owners.

## Phase gate

- [x] No production code has changed yet.
- [x] The actual diagnostic owner is identified. If `src/compiler_frontend/deferred_feature_diagnostics.rs` does not exist, update docs and use the current `compiler_messages` structure instead of creating a stale path.
- [x] Search results are complete enough to guide deletion and docs updates.

---

# Phase 1 — Canonical design-scope documentation

## Context

`docs/language-overview.md` is the compiler-facing language facts document. It should define the design boundary before the implementation is changed. Keep it factual and concise; put tutorials in `docs/src/docs/**`.

## Tasks

### 1.1 Add design-scope taxonomy to `docs/language-overview.md`

- [x] Add this near the top, after `Design principles` and before `Related references`:

```markdown
## Language Design Scope

Beanstalk keeps the source language deliberately small. Compiler complexity is allowed when it
makes the programmer-facing model simpler, safer, and more predictable.

This document separates future work into two categories:

- **Deferred**: fits Beanstalk's language design but is not implemented yet or is not part of the
  current Alpha surface.
- **Outside language design scope**: intentionally not planned because it would make the language
  broader, more implicit, more solver-heavy, or harder to reason about.

A feature listed outside design scope should not be implemented unless the language philosophy is
explicitly changed first.
```

- [x] Add this section immediately after it:

```markdown
## Outside the Language Design Scope

The following surfaces are intentionally outside Beanstalk's language design scope.

| Surface | Reason |
|---|---|
| General-purpose macros, procedural macros, derive macros, and AST macros | They create a second compile-time language and make code harder to inspect. Templates, constants, and builder directives are the constrained compile-time surface. |
| Dynamic trait values / trait objects | They require erased wrappers, runtime dispatch, dynamic-safety rules, backend-specific runtime support, and a second meaning for trait names. Use choices for runtime heterogeneity and generic trait bounds for static reuse. |
| Trait inheritance, trait aliases, trait composition, default methods, associated types, and associated constants | These turn traits into a type-level programming system rather than simple method contracts. |
| Generic traits and generic trait methods | These make trait solving significantly more complex and create Rust-like abstraction patterns. |
| Blanket, conditional, negative, or specialized conformance | These require coherence and specialization rules outside Beanstalk's simplicity target. |
| Structural conformance | Matching method shapes must not silently imply conformance. `Type must TRAIT` stays explicit. |
| Type-set constraints, union constraints, underlying-type constraints, and user-defined numeric/operator constraints | Generic bounds name traits only. They are not a constraint sublanguage. |
| Operator overloading | Operators remain compiler-owned and predictable. |
| Source-authored receiver methods for builtins, imported types, dependency/library types, external opaque types, or types declared in another file | Source-authored receiver methods belong only to the same file as their nominal receiver type. Use free functions for other types. |
| User-defined `HASHABLE`, custom map hashers/comparers, and user-defined keys for builtin maps | Builtin map syntax stays scalar-keyed. More sophisticated maps belong in libraries as ordinary structs. |
| First-class public `Result` values and result pattern matching | `Error!`, postfix `!`, and `catch` are the language error path. Users can define ordinary choices when they want explicit result values. |
| Exceptions | Expected failures use `Error!`; invariants use `assert`. |
| Reflection, runtime type IDs, compile-time type inspection, and type-returning functions | These encourage generic meta-programming and weaken static readability. |
| Higher-kinded types, type functions, partial type application, and parameterized type aliases | These introduce a type-level abstraction language. |
| User const generics beyond fixed collection capacity | Capacity syntax remains a small built-in collection feature, not a general type parameter system. |
```

### 1.2 Update affected `docs/language-overview.md` sections

- [x] Replace the Syntax Summary trait row with:

```markdown
| Traits | Trait declarations and conformances use `must`; generic bounds use `is`. Trait names are static contracts, not value types. |
```

- [x] Replace fixed-capacity examples that use inline arithmetic:

```beanstalk
capacity #Int = 4
scratch ~{capacity String} = {}

larger_capacity #Int = capacity + 2
larger_scratch ~{larger_capacity String} = {}
```

- [x] Replace the fixed-capacity rule with:

```markdown
- Fixed capacity in type position must be either a positive `Int` literal or a bare visible `#Int`
  constant name. Capacity position is not general expression position: arithmetic, function calls,
  field access, const-record projection, conditionals, and nested expressions are invalid there.
  Put any calculation in a named compile-time constant first.
```

- [x] Replace hashmap key rules with:

```markdown
- Builtin hashmap key types are permanently limited to `String`, `Int`, `Bool`, and `Char`.
  This is a language-owned map surface, not a general hashing abstraction.
- `Float`, structs, choices, collections, hashmaps, traits, functions, external opaque types,
  templates as a distinct key type, and generic parameters are invalid keys.
```

- [x] Replace the hashmap deferred list with:

```markdown
Outside the builtin hashmap design scope: hashsets as language syntax, user-defined hashers or
comparers, `Float` keys, user-defined key types, generic key maps through `HASHABLE`, map equality,
mutable entry APIs, indexing syntax, const hashmaps, fixed/capacity maps, and specialized map
variants. More sophisticated maps should be ordinary standard-library or user-defined structs.

Wasm hashmap runtime/lowering remains deferred backend work for the existing scalar-keyed builtin map
surface.
```

- [x] Replace receiver-method rules with same-file nominal rules:

```markdown
Receiver method rules:
- A receiver method is a top-level function whose first parameter is named `this`.
- `this` is reserved and may appear only as the first receiver parameter and inside that method body.
- There may be exactly one `this` parameter.
- Supported source-authored receiver types are user-defined structs and choices declared in the
  same source file as the receiver method.
- Aligned declaration-site generic nominal receivers are valid when the method belongs to the same
  generic type declaration.
- Source-authored receiver methods for built-in scalars, imported source types, dependency/library
  types, external opaque types, and types declared in another file are rejected.
- Compiler-owned collection operations and compiler/builder-owned builtin operations are not user
  extension methods.
- `this T` is immutable; `this ~T` is mutable.
- Mutable receiver calls require explicit mutable/exclusive receiver syntax: `~value.method(...)`.
- Receiver methods are called only with receiver syntax; `method(value, ...)` is invalid.
- Mutable receiver methods require a mutable place receiver; temporaries and rvalues cannot be mutated.
- Field writes follow the same mutable-place rule.
```

- [x] Replace receiver visibility with:

```markdown
Receiver method visibility is tied to receiver type visibility.

- A source-authored receiver method is visible wherever its receiver type is visible.
- Receiver methods are not imported, aliased, or re-exported independently.
- Namespace imports may make the receiver type visible, but receiver methods are not namespace fields.
- A `#mod.bst` facade exposes a type's same-file receiver methods when it exposes the type.
- Use free functions for private helpers or operations on types owned by other files or packages.
```

- [x] Replace the trait dynamic-value text with:

```markdown
Traits are not value types.

A trait name may appear in:
- a trait declaration;
- an explicit conformance declaration;
- a generic bound.

A trait name is invalid as an ordinary variable, parameter, field, return, collection element,
choice payload, or alias target type.

Use a generic bound for static reuse:

```beanstalk
render type Item is DISPLAY_TEXT |item Item| -> String:
    return item.display()
;
```

Use a choice for runtime heterogeneity:

```beanstalk
Renderable ::
    Label | value Label |,
    Button | value Button |,
;
```

Dynamic trait values / trait objects are outside the current language design scope because they
require erased wrappers, runtime dispatch, dynamic-safety rules, backend-specific support, and a
second meaning for trait names.
```

- [x] Replace conformance evidence rules with:

```markdown
- Canonical conformance evidence for same-file structs, choices, and generic type constructors is
  reusable wherever both the type and trait are visible.
- User-authored conformance for builtins, imported types, dependency/library types, external opaque
  types, and types declared in another file is rejected.
```

- [x] In the generics section, remove dynamic-trait references and replace with:

```markdown
Traits are static contracts only. Trait names cannot appear as value types, so there is no dynamic
trait value that can satisfy or fail a static generic bound.
```

- [x] Split rejected/deferred generic list into true deferred work vs outside-scope work. Move these to outside-scope wording:
  - generic function values / higher-order polymorphism;
  - type values, type-returning functions, type-level `#if`, compile-time type inspection;
  - file-local evidence-backed generic bound dispatch;
  - parameterized generic aliases and partial type application;
  - higher-kinded types, const generics, lifetime parameters, specialization, associated types.

### 1.3 Update `docs/compiler-design-overview.md`

- [x] Replace the `src/compiler_frontend/traits/` overview bullet with static-only ownership:

```markdown
`src/compiler_frontend/traits/` owns parsed trait shells, resolved trait definitions, explicit
same-file nominal conformance evidence, reusable evidence visibility, static generic-bound evidence
checks, and trait diagnostics. Trait metadata is compile-time frontend state, not a value-type or
backend rediscovery path.
```

- [x] Remove or correct any stale reference to `src/compiler_frontend/deferred_feature_diagnostics.rs`. Preferred wording:

```markdown
Design-scope and deferred-feature diagnostics should be centralized through typed
`CompilerDiagnostic` constructors. Deferred features and outside-design-scope rejections must remain
distinct diagnostic reasons.
```

- [x] Replace import/receiver-method visibility text with:

```markdown
Header import preparation does not import receiver methods as independent symbols. Source-authored
receiver methods belong to their receiver type's declaring file and become callable wherever the
receiver type is visible. Namespace imports may make a receiver type visible, but methods are never
namespace fields and cannot be grouped-imported or aliased independently.
```

- [x] Replace the AST trait ownership bullet with:

```markdown
trait declaration resolution, trait visibility, same-file conformance evidence validation,
static generic-bound evidence checks, and evidence-backed static receiver fallback
```

- [x] Replace the full Traits contract with:

```markdown
### Traits contract

Trait declarations and conformances are resolved before HIR. Header parsing records trait and
conformance shells; AST owns semantic trait identity, requirement type resolution, conformance
evidence validation, reusable evidence visibility, and generic-bound evidence checks.

- Traits are compile-time metadata in `TraitEnvironment`, not `DataType` values.
- Trait names are valid in trait declarations, conformance declarations, and generic bounds only.
- A trait name in ordinary type position is rejected before HIR.
- Explicit same-file conformance evidence lives in `TraitEvidenceEnvironment` while AST resolves
  static calls.
- Static generic bounds use visible reusable evidence during generic function calls and concrete
  generic nominal instantiation.
- HIR must never carry dynamic trait values, trait-object construction, trait-object dispatch,
  unresolved generic calls, or unsolved trait-bound method calls.
- Backends must not resolve trait declarations or scan source headers for method shapes.
```

- [x] Remove HIR ownership bullets about dynamic trait construction/dispatch and trait metadata projected across the backend boundary.
- [x] Delete the `### Dynamic trait operations` section.
- [x] Update backend lowering text so reachable validation mentions external calls and scalar-keyed maps, not dynamic trait runtime operations.

### 1.4 Phase gate

- [x] Run:

```bash
just validate
```

- [x] Search docs:

```bash
rg -n "dynamic trait values are|dynamic-safe|dynamic trait runtime|HASHABLE.*future|capacity \\+|grouped receiver|receiver method import|FileLocalExtension" docs README.md
```

- [x] Remaining source-doc hits are only in outside-scope policy text, named-constant capacity examples, or roadmap-plan instructions.
- [x] Manual review confirms `docs/language-overview.md` remains compiler-facing, not tutorial-heavy.

---

# Phase 2 — Docs-site, roadmap, progress matrix, README

## Context

The docs site is real Beanstalk code and user-facing. The progress matrix and roadmap must not keep old “maybe later” entries that contradict the new design.

## Tasks

### 2.1 Add docs-site design-scope page

- [x] Add `docs/src/docs/design-scope/#page.bst`.
- [x] Keep it short. Use the same taxonomy as canonical docs.
- [x] Include these examples:

```beanstalk
render type Item is DISPLAY_TEXT |item Item| -> String:
    return item.display()
;
```

```beanstalk
Renderable ::
    Label | value Label |,
    Button | value Button |,
;
```

- [x] Add a concise outside-scope table. Do not duplicate every line of `docs/language-overview.md`; group related features.

### 2.2 Update docs index

Path: `docs/src/docs/#page.bst`

- [x] Add under Introduction:

```beanstalk
- @./design-scope (Language Design Scope)
```

### 2.3 Update traits docs-site page

Path: `docs/src/docs/traits/#page.bst`

- [x] Change page description to static-only traits.
- [x] Remove sections for:
  - dynamic trait values;
  - dynamic-safe traits;
  - dynamic dispatch and backend runtime behavior;
  - file-local evidence.
- [x] Rename “Canonical and file-local evidence” to “Canonical evidence.”
- [x] State that conformances are reusable only for same-file structs, choices, and generic type constructors.
- [x] State that conformance for builtins, imported/dependency/library types, external opaque types, and other-file types is rejected.
- [x] Add “Runtime heterogeneity uses choices” with a short example.
- [x] Replace “Deferred trait ecosystem” with “Outside trait design scope.” Include:
  - default methods;
  - associated types/constants;
  - inheritance/composition/aliases;
  - generic traits/methods;
  - blanket/conditional/negative/specialized conformance;
  - structural conformance;
  - dynamic trait values;
  - downcasting/reflection;
  - output coercion;
  - operator integration;
  - automatic primitive conformances;
  - derive/hash/order/format/iteration/serialization trait families.

### 2.4 Update generics docs-site page

Path: `docs/src/docs/generics/#page.bst`

- [x] Remove dynamic-trait references.
- [x] Replace trait-bound explanation with:

```text
A trait name in a generic bound constrains a concrete generic parameter. Trait names are not value
types, so a parameter, field, collection element, or return type cannot be annotated directly with a
trait name.
```

- [x] Replace file-local evidence-backed generic-bound dispatch with outside-scope wording.
- [x] Split future list into:
  - **Deferred generic work**: only items still genuinely plausible, such as recursive generic types, generic external package functions/types, or nested `of` relaxation if still desired.
  - **Outside generic design scope**: explicit generic call syntax, higher-order polymorphism, type-level computation, `where` clauses, parameterized aliases, partial application, HKT, const generics beyond fixed capacity, lifetimes, specialization, associated types, dynamic reification.

### 2.5 Update collections docs-site page

Path: `docs/src/docs/collections/#page.bst`

- [x] Replace capacity text with literal-or-bare-constant rule.
- [x] Replace `{capacity + 2 String}` examples with named-constant examples.
- [x] Replace stale hashmap section with implemented V1 syntax:

```beanstalk
scores ~= {"Ada" = 10, "Grace" = 12}
empty_scores ~{String = Int} = {}

score = scores.get("Ada") catch:
    then 0
;

~scores.set("Linus", 7) catch:
;

removed = ~scores.remove("Grace") catch:
    then 0
;

if scores.contains("Ada"):
    io("found")
;

count = scores.length
~scores.clear()
```

- [x] State that builtin map keys are permanently `String`, `Int`, `Bool`, `Char`.
- [x] State that sophisticated maps belong in library/user-defined structs.

### 2.6 Update structs and docs-site language overview pages

Paths:

- `docs/src/docs/structs/#page.bst`
- `docs/src/docs/language-overview/#page.bst`

- [x] Replace builtin/imported/external receiver method text with same-file nominal rule.
- [x] Remove grouped receiver-method import/alias text.
- [x] State that receiver methods are visible through the receiver type, not as imported symbols.
- [x] Update external JS signature docs: source-authored `@bst.sig` should expose free functions and opaque types, not `this` receiver methods.

### 2.7 Update roadmap

Path: `docs/roadmap/roadmap.md`

- [x] Add an `Outside Language Design Scope` section near the top.
- [x] Move these out of roadmap follow-ups:
  - dynamic trait values / trait objects / dynamic trait runtime lowering;
  - dynamic wrappers in Wasm ABI mapping;
  - `HASHABLE`, generic map keys, user-defined map keys, custom hashers/comparers, `Float` map keys, map equality, entry APIs, fixed/capacity maps, language hashsets;
  - default trait methods, associated types/constants, inheritance/composition, generic traits/methods, conditional/specialized/blanket/negative conformance, dynamic composition/downcasting/reflection, output coercion, operator integration;
  - first-class public `Result`, exceptions, reflection, type-level programming, HKT, parameterized aliases, partial application.

- [x] Keep only legitimate deferred work:
  - Wasm runtime/lowering for existing scalar-keyed maps;
  - possible read-only map iteration only if it does not introduce `HASHABLE`, custom equality, or mutable entry APIs;
  - diagnostics/tooling polish for static traits;
  - backend work unrelated to removed dynamic trait runtime.

### 2.8 Update progress matrix

Path: `docs/src/docs/progress/#page.bst`

- [x] Add status row:

```beanstalk
[data, red:
    [: Outside Scope]
    [: Intentionally not planned for Beanstalk's language design. Syntax should be rejected with structured diagnostics when encountered.]
]
```

- [x] Update Traits row to static-only surface.
- [x] Remove dynamic coercion, JS dynamic runtime, Wasm dynamic unsupported, dynamic-safe, dynamic wrappers, and dynamic dispatch coverage text.
- [x] Update Structs/receiver row to same-file nominal rule.
- [x] Update Collections row to narrow capacity syntax.
- [x] Update Hash Maps row to permanent scalar-key policy and outside-scope map features.
- [x] Update External Packages row so source-authored external imports expose free functions and opaque types. If builder-owned member syntax still exists temporarily, label it as builder-owned metadata and isolate it from source-authored receiver methods.

### 2.9 Update README

- [x] Add one concise goal bullet:

```markdown
- A deliberately small language surface: static nominal types, explicit trait conformance, constrained generics, no general macro system, and no Rust-style type-level programming.
```

- [x] Optionally add a link to the language design scope section/page if the README documentation list remains short.

## Phase gate

- [x] Run:

```bash
just validate
```

Validation passed for the documentation slice on 2026-06-11. Generated `docs/release/**` HTML was
not regenerated in this slice per the documentation source/output workflow.

- [x] Search docs site and roadmap:

```bash
rg -n "dynamic trait|dynamic-safe|FileLocalExtension|file-local evidence|HASHABLE|capacity \\+|grouped receiver|receiver method import|receiver-method alias|this.*@bst.sig|deferred trait ecosystem" docs/src/docs docs/roadmap README.md
```

- [x] Remaining source-doc hits are only outside-scope policy text, named-constant capacity examples, or roadmap-plan instructions.

---

# Phase 3 — Diagnostic taxonomy and rejection paths

## Context

Out-of-scope features should be rejected as intentional language rules, not as missing implementations. Build the diagnostic vocabulary before deleting systems so parser/type-resolution changes can fail cleanly.

## Tasks

### 3.1 Locate the diagnostic owner

- [x] Search current diagnostic structure:

```bash
fd diagnostic src/compiler_frontend
rg -n "DeferredFeature|deferred feature|not implemented|unsupported feature|DiagnosticPayload|Invalid.*Reason|Rule" src/compiler_frontend/compiler_messages src/compiler_frontend
```

- [x] Add outside-scope reasons to the existing diagnostic structure. Do not create `src/compiler_frontend/deferred_feature_diagnostics.rs` unless that is already the live module pattern.
- [x] Keep “deferred feature” and “outside design scope” as distinct typed reasons.

The live owner is the current `compiler_messages` diagnostic model, including
`src/compiler_frontend/compiler_messages/deferred_feature_diagnostics.rs` for true deferred
features. Outside-scope language rejections stay on existing typed rule/syntax payloads instead of
being routed through `DeferredFeatureReason`. `fd` was not installed in this workspace, so the
diagnostic-owner inventory used `rg --files` plus the payload/reason search.

Recommended conceptual shape, adapted to existing enums:

```rust
pub enum LanguageSurfaceRejectionReason {
    Deferred(DeferredFeatureReason),
    OutsideDesignScope(OutsideDesignScopeReason),
}
```

Only add reason variants that current syntax/resolution can reach.

### 3.2 Required outside-scope diagnostic reasons

- [x] Dynamic trait value type.
- [x] Receiver method for nonlocal source type.
- [x] Receiver method for builtin scalar.
- [x] Receiver method for external opaque type.
- [x] Receiver method import or alias.
- [x] Trait conformance for builtin/imported/external/nonlocal type.
- [x] General fixed-capacity expression.
- [x] Unsupported builtin map key.
- [x] `HASHABLE` / generic builtin map key.
- [x] `where` clause, if currently parsed/rejected.
- [x] Parameterized alias / partial application, if currently parsed/rejected.
- [x] Dynamic trait composition/downcast syntax, if any reserved syntax exists.
- [x] Trait default/associated/inheritance/generic method syntax, if any reserved syntax exists.

Phase 4 already routes trait names in ordinary type position through
`TraitNameUsedAsType`. Phase 5 already rejects source receiver-method imports and aliases through
`ReceiverMethodImportNotAllowed`. This slice split receiver declaration target failures into
nonlocal source, builtin scalar, and external opaque reasons; split trait conformance target
failures into nonlocal source, builtin, and external opaque reasons; moved `where` constraints from
`DeferredFeatureReason` to invalid function-signature diagnostics; and renamed the generic-trait
declaration reason to current outside-scope wording. No dynamic trait composition/downcast syntax
was found in the current parser surface.

### 3.3 Diagnostic wording

Use concise, current-language wording. Suggested messages:

- Dynamic trait value annotation:

```text
Trait `DISPLAY_TEXT` is a static contract, not a value type. Use a generic bound such as
`type Item is DISPLAY_TEXT`, or define a choice for runtime heterogeneity.
```

- Receiver method for non-owned type:

```text
Source-authored receiver methods must be declared in the same file as their user-defined receiver
type. Use a free function for values owned by another file or package.
```

- Trait conformance for non-owned type:

```text
User-authored trait conformance is allowed only for a user-defined type declared in the same file.
Use a local wrapper choice or struct when adapting an external or imported type.
```

- Fixed capacity expression:

```text
Fixed collection capacity must be a positive integer literal or a bare visible `#Int` constant name.
Put arithmetic in a named compile-time constant before the type annotation.
```

- Map key rejection:

```text
Builtin hashmap keys are limited to `String`, `Int`, `Bool`, and `Char`. Use a library or
user-defined map type for custom key behavior.
```

- Receiver method import/alias:

```text
Receiver methods are not imported or aliased independently. Import the receiver type; its same-file
methods are available through receiver-call syntax when the type is visible.
```

### 3.4 Diagnostic tests

- [x] Add or update negative integration tests asserting stable diagnostic codes for:
  - trait name used as parameter type;
  - trait name used as return type;
  - trait name used as struct field type;
  - trait name used as choice payload type;
  - trait name used as collection element type;
  - trait name used as alias target;
  - receiver method for imported type;
  - receiver method for same-module different-file type;
  - receiver method for builtin scalar;
  - receiver method for external opaque type;
  - grouped receiver method import;
  - receiver method alias;
  - trait conformance for imported/builtin/external/nonlocal type;
  - fixed capacity arithmetic;
  - fixed capacity field projection;
  - fixed capacity function call/runtime expression;
  - builtin map with generic key;
  - builtin map with struct/choice/float key.

Existing Phase 4, Phase 5, Phase 6, and Phase 7 fixtures cover most of this list. This slice
updated message assertions for the newly split receiver/conformance reasons, added a direct
external opaque conformance-target fixture, and moved the `where` fixture from a deferred-feature
code to the invalid function-signature code.

## Phase gate

- [x] Run:

```bash
cargo fmt
cargo clippy
cargo test
cargo run -- tests
just validate
```

Validation passed on 2026-06-12 with `cargo fmt`, focused header/function/diagnostic unit tests,
`cargo run -- tests`, and full `just validate`.

- [x] Manual review:
  - [x] diagnostics are typed and source-located;
  - [x] no rendered strings are semantic state;
  - [x] no user-facing source rejection uses `CompilerError`;
  - [x] no diagnostic mentions old behavior;
  - [x] no boolean-heavy helper where a reason enum would be clearer.

---

# Phase 4 — Remove dynamic trait values end-to-end

## Context

Dynamic trait values currently create type identity, coercions, HIR nodes, reachability facts, backend runtime paths, and Wasm unsupported checks. Static traits stay; dynamic trait values are deleted, not hidden.

## Tasks

### 4.1 Type system deletion

- [x] Remove `DataType::DynamicTrait`.
- [x] Remove `TypeDefinition::DynamicTrait` and `DynamicTraitTypeDefinition`.
- [x] Remove `TypeEnvironment` intern/query/display helpers for dynamic trait value types.
- [x] Remove type compatibility/coercion branches for dynamic trait values.
- [x] Replace trait-name-in-ordinary-type-position resolution with the outside-scope diagnostic.
- [x] Verify trait names remain valid only in:
  - trait declarations;
  - conformance declarations;
  - generic bounds.

Search while working:

```bash
rg -n "DynamicTrait|dynamic trait|dynamic_safe|dynamic-safe|intern_dynamic|trait value" src/compiler_frontend/datatypes src/compiler_frontend/type_coercion src/compiler_frontend/ast
```

### 4.2 Trait subsystem simplification

- [x] Remove dynamic-safety classification and fields.
- [x] Remove APIs whose only purpose is dynamic trait coercion or runtime table selection.
- [x] Keep static requirement validation for direct `This` forms:
  - receiver `This` / `~This`;
  - named non-receiver `other This`;
  - direct return `This`;
  - current rejection of composed `This?`, `{This}`, `Box of This`.
- [x] Keep evidence validation required for static generic-bound calls.
- [x] If `DISPLAYABLE` exists only for dynamic/output-coercion scaffolding, either remove it or keep it as a static-only test fixture with comments/docs updated. Do not leave output coercion or dynamic behavior implied.

### 4.3 AST boundary cleanup

- [x] Remove dynamic trait coercion insertion from contextual coercion sites:
  - declarations;
  - arguments;
  - returns;
  - struct fields;
  - choice payloads;
  - collection elements.
- [x] Remove dynamic trait AST nodes/expressions if present.
- [x] Audit whether `Ast` still needs to carry `TraitEnvironment` or `TraitEvidenceEnvironment` after HIR no longer uses dynamic traits.
  - Preferred: trait evidence is AST-only if static trait-bound calls are concrete before HIR.
  - Keep only if a current static feature consumes it, and document the exact consumer.

### 4.4 HIR deletion

- [x] Remove `HirExpressionKind::ConstructDynamicTraitValue`.
- [x] Remove `HirStatementKind::CallDynamicTraitMethod`.
- [x] Remove `HirDynamicTraitCallArgument` and `HirDynamicTraitCallArgumentEffect`.
- [x] Remove dynamic trait imports from HIR modules.
- [x] Update HIR lowering so static trait-bound method calls lower to concrete direct calls before HIR.
- [x] Update HIR validation/display/tests/goldens.

### 4.5 Reachability and backend deletion

- [x] Remove `reachable_dynamic_trait_operations` from `HirReachability`.
- [x] Remove `ReachableDynamicTraitOperation` and kind enums.
- [x] Remove dynamic trait unsupported-backend validation.
- [x] Remove JS dynamic method-table emission, wrapper lowering, and helper runtime assets.
- [x] Remove Wasm dynamic-trait unsupported diagnostics.
- [x] Remove dynamic trait runtime tests and rewrite any behavior coverage as static trait or choice tests.

### 4.6 Tests

- [x] Rewrite dynamic trait success cases into:
  - static generic-bound examples; or
  - choice heterogeneity examples.
- [x] Add negative tests for every ordinary type-position use of a trait name.
- [x] Ensure static trait bound tests still pass.
- [x] Ensure static trait-bound receiver calls do not require backend trait metadata.

## Phase gate

- [x] Run:

```bash
rg -n "DynamicTrait|dynamic trait|dynamic-safe|dynamic_safe|trait object|trait value|dynamic wrapper|dynamic coercion|CallDynamicTrait|ConstructDynamicTrait|reachable_dynamic_trait" src tests docs libraries
```

- [x] Remaining hits are only outside-scope docs or negative tests.
- [x] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

- [x] Manual stage-boundary review:
  - [x] HIR has no dynamic trait value or dispatch representation.
  - [x] Backends do not consume trait/evidence metadata for dynamic dispatch.
  - [x] Static trait-bound calls are concrete before HIR.
  - [x] Comments describe the new static-only invariant.

---

# Phase 5 — Simplify receiver methods and conformance ownership

## Context

The current `Canonical` vs `FileLocalExtension` split supports extension methods and file-local evidence. The new rule makes source-authored methods and conformances belong to the same file as the nominal type. This should remove import-time method aliasing and much of the duplicate visibility machinery.

Status: source-authored receiver methods and user-authored conformances now follow the same-file
nominal rule. File-local conformance evidence and source receiver-method import/export/alias paths
are removed. External packages now expose opaque types, constants, and free functions only.
Source-authored/provider-created JS external imports reject `this` receiver signatures, `@web/canvas`
uses raw free functions, and method-style canvas ergonomics are provided by the source-owned
`@html` `Canvas` wrapper.

## Tasks

### 5.1 Source receiver policy

- [x] Enforce:

```text
A source-authored receiver method is valid only when its receiver is a user-defined struct or choice
declared in the same source file as the method.
```

- [x] For generic nominal receivers, allow only declaration-site parameters aligned with the receiver type’s own parameters.
- [x] Reject receiver methods for:
  - builtins;
  - imported source types;
  - dependency/library types;
  - external opaque types;
  - same-module but different-file types;
  - concrete generic instances.

### 5.2 Refactor `src/compiler_frontend/ast/receiver_methods.rs`

- [x] Remove `ReceiverMethodKind::FileLocalExtension`.
- [x] Remove `ReceiverMethodKind` entirely if every source method is canonical.
- [x] Replace `receiver_method_kind_for_declaration` with a direct validator:

```rust
fn validate_source_receiver_method_declaration(
    receiver: &ReceiverKey,
    method_source_file: &InternedPath,
    struct_source_by_path: &FxHashMap<InternedPath, InternedPath>,
    choice_source_by_path: &FxHashMap<InternedPath, InternedPath>,
    location: SourceLocation,
) -> Result<(), CompilerDiagnostic> {
    // Same-file Struct/Choice only.
}
```

- [x] Remove visibility fallback that turns merely-visible receiver types into extension targets.
- [x] Remove extension override checks.
- [x] Remove duplicate logic needed only for canonical/extension collision.
- [x] Keep same-receiver duplicate method diagnostics.
- [x] Keep field-name conflict validation for struct methods.
- [x] Keep receiver-call mutability rules.
- [x] Audit `by_method_name`: retained only for “called as free function” diagnostics.

### 5.3 Simplify receiver visibility/imports

- [x] Remove independent source receiver-method imports from header import parsing.
- [x] Remove grouped source receiver-method import parsing and aliasing.
- [x] Retain `visible_receiver_methods` as receiver-call-only visibility derived from visible receiver types; it is no longer an independent import/export surface.
- [x] Remove `visible_external_receiver_methods` and the external receiver-method lookup path.
- [x] Source receiver methods do not reserve ordinary value/import aliases.
- [x] Receiver method names may repeat across different receiver types.
- [x] Method names still cannot conflict with fields on the same struct.

### 5.4 Facade rule

- [x] Implement:

```text
A facade exposes a type's same-file receiver methods when it exposes the type.
Receiver methods are not independently exported or aliased.
```

- [x] Remove explicit source receiver-method re-export logic.
- [x] Decide transparent alias behavior and document/test it.
  - Recommended: transparent aliases expose the same receiver method surface because they are the same type.

### 5.5 External package member surface

- [x] Preferred strict target for source-authored/provider-created JS packages: expose free functions and opaque types, not source-style receiver methods.
- [x] Reject `this` receiver parameters in source-authored `@bst.sig` JS imports.
- [x] Convert builder-owned external receiver APIs, such as `@web/canvas`, to free functions where feasible.
- [x] Remove builder-owned external receiver metadata from the source receiver-method catalog, import environment, AST receiver-call resolution, and external package metadata.
- [x] Remove the remaining grouped external receiver-method import surface because external packages no longer expose receiver methods.
- [x] Add source-owned HTML `Canvas` wrapper methods over the raw `@web/canvas` free functions.

Phase 5.5 final update: the remaining external receiver-method exception is removed. Raw
`@web/canvas` imports use free functions with the context/element as the first parameter, and
`@html` provides the user-facing method-style wrapper. Generated integration fixture `input/dev/`
and `input/release/` directories plus benchmark `dev/` and `release/` output directories are ignored
so local fixture and benchmark refreshes do not leave generated build output in Git status.
Validation passed on 2026-06-12 with the targeted canvas unit test,
`cargo run -- check docs/src/#page.bst`, `cargo run -- tests`, and full `just validate`.

### 5.6 Conformance ownership

- [x] Enforce same-file nominal conformance:

```text
User-authored `Type must TRAIT` is valid only when `Type` is a user-defined struct, choice, or
generic type constructor declared in the same file.
```

- [x] Reject conformance for builtins, imported/dependency/library types, external opaque types, and other-file types.
- [x] Remove file-local extension evidence.
- [x] Remove generic-bound dispatch through file-local evidence.
- [x] Keep reusable evidence wherever both the type and trait are visible.

### 5.7 Tests

- [x] Positive:
  - same-file struct method;
  - same-file choice method;
  - aligned generic nominal receiver method;
  - method call after importing receiver type;
  - method call through namespace import when receiver type is visible;
  - facade exposes type and methods together;
  - same-file nominal trait conformance;
  - generic bound using visible reusable evidence.

- [x] Negative:
  - method declared for imported type;
  - method declared for same-module different-file type;
  - method declared for dependency/library type;
  - method declared for builtin scalar;
  - method declared for external opaque;
  - grouped receiver-method import;
  - receiver-method alias;
  - method called as free function;
  - duplicate same receiver method;
  - conformance for builtin/imported/external/nonlocal type;
  - generic bound requiring file-local evidence.

## Phase gate

- [x] Run:

```bash
rg -n "FileLocalExtension|file-local extension|file local extension|file-local evidence|file local evidence|visible_receiver_methods|visible_external_receiver_methods|grouped receiver|receiver method import|receiver-method import|receiver method alias|receiver-method alias|ExtensionOverridesCanonicalMethod" src docs tests libraries
```

- [x] Remaining production-code hits are only the intentionally retained source receiver-call visibility map; broader docs hits are tracked by the documentation phases.
- [x] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

- [x] Manual review:
  - [x] Header/import stage no longer owns independent source receiver-method visibility.
  - [x] AST receiver lookup uses receiver type visibility and same-file method catalog.
  - [x] Trait evidence is not split into reusable vs file-local extension evidence.
  - [x] No stale production-code comments describe extension methods or file-local evidence as supported.

---

# Phase 6 — Harden scalar-keyed builtin hashmaps

## Context

Builtin maps are already close to the desired surface. The main work is removing future `HASHABLE` pressure and any over-generalized key scaffolding.

## Tasks

### 6.1 Key policy

- [x] Ensure builtin map key validation accepts only:
  - `String`;
  - `Int`;
  - `Bool`;
  - `Char`.

- [x] Ensure it rejects:
  - `Float`;
  - structs;
  - choices;
  - collections;
  - maps;
  - traits;
  - functions;
  - external opaque types;
  - templates as key type;
  - generic parameters;
  - aliases expanding to unsupported keys.

- [x] Remove “before future `HASHABLE`” wording from diagnostics and comments.
- [x] Remove any `HASHABLE`, custom hash/equality, or generic-key scaffolding not required by scalar-key validation.
- [x] Keep value-side behavior according to current runtime-storable rules.

Generic key parameters now use the same scalar-only unsupported-key diagnostic path as every
other unsupported key type. The old `HASHABLE`-specific diagnostic reason and rendering path were
removed.

### 6.2 HIR/backend simplification

- [x] Keep first-class HIR map literal/op nodes.
- [x] Keep HIR reachability for map construction/use because HTML-Wasm still rejects reachable maps.
- [x] Keep JS runtime/helper lowering for scalar-keyed maps.
- [x] Simplify key normalization/runtime helpers to assume the scalar-key policy where possible.
- [x] Remove hooks for custom hash/equality policies if any exist.

No HIR/backend map changes were needed: the stale generic-key branch lived in AST type-resolution
diagnostics, while runtime helpers and Wasm reachability already consume scalar-keyed maps.

### 6.3 Tests

- [x] Positive:
  - `String`, `Int`, `Bool`, `Char` keys;
  - alias to supported scalar key.

- [x] Negative:
  - `Float` key;
  - struct key;
  - choice key;
  - collection key;
  - map key;
  - external opaque key;
  - generic parameter key;
  - alias to unsupported key.

## Phase gate

- [x] Run:

```bash
rg -n "HASHABLE|hashable|generic key|generic map|user-defined key|user defined key|custom hasher|custom comparer|map equality|Float key|fixed map|capacity map|hashset" src docs tests libraries
```

- [x] Remaining hits are only outside-scope docs, negative tests, stale generated docs output, or
  legitimate scalar-map/backend terminology.
- [x] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

Validation passed on 2026-06-12 with `cargo fmt`, targeted type-resolution and diagnostic-model
tests, `cargo run -- tests`, and full `just validate`.

- [x] Manual review:
  - [x] map key validation is direct and not solver/trait based;
  - [x] diagnostics do not imply `HASHABLE` is coming;
  - [x] backend code does not carry unused custom-key hooks.

---

# Phase 7 — Restrict fixed collection capacity syntax

## Context

Capacity arithmetic should remain possible in named constants, but type-position capacity itself should not be general expression syntax.

## Tasks

### 7.1 Parser/type representation

- [x] Find the parsed representation for fixed collection capacity.
- [x] Replace general expression/token storage with a narrow representation:

```rust
pub enum ParsedCollectionCapacity {
    Literal { value: i64, location: SourceLocation },
    BareConstant { name: StringId, location: SourceLocation },
}
```

The parser now rejects arithmetic and other expression-shaped capacity prefixes instead of
carrying raw token slices into AST type resolution.

- [x] Accept only:
  - positive integer literal;
  - bare visible `#Int` constant name.

- [x] Reject:
  - arithmetic, including `capacity + 2` and `2 + 2`;
  - function calls;
  - field access / const-record projection;
  - conditionals;
  - nested/parenthesized expressions unless the parser already treats `(4)` as a literal; recommended: reject for simplicity;
  - namespace-qualified constants unless explicitly retained. Recommended strict rule: require a bare imported alias/local constant name.

### 7.2 Header dependency edges

- [x] Keep dependency edges only for bare constant-name capacity.
- [x] Remove general expression reference walking for capacity type position.
- [x] Preserve current source-order/import rules for visible constants.
- [x] If imported constants are allowed, prefer requiring a bare grouped import alias:

```beanstalk
import @sizes { default_capacity }
items {default_capacity String} = {}
```

### 7.3 AST capacity resolution

- [x] Resolve literal capacity directly.
- [x] Resolve bare `#Int` capacity by looking up the folded constant value.
- [x] Reject non-`Int`, non-constant, non-positive, overflow, and unresolved names with structured diagnostics.
- [x] Ensure diagnostics say to use a positive integer literal or bare `#Int` constant name.
- [x] HIR/backends continue consuming canonical collection shape only.

AST resolution now distinguishes explicit `#` constants from merely foldable runtime immutable
bindings, so `capacity = 4` is not valid capacity syntax while `capacity #Int = 4` remains valid.

### 7.4 Tests

- [x] Positive:
  - `{4 Int}`;
  - `{capacity Int}` with `capacity #Int = 4`;
  - `larger_capacity #Int = capacity + 2` then `{larger_capacity String}`;
  - fixed literal over-capacity rejection still works;
  - aliases preserve fixed shape.

- [x] Negative:
  - `{capacity + 2 Int}`;
  - `{2 + 2 Int}`;
  - `{get_capacity() Int}`;
  - `{defaults.capacity Int}`;
  - runtime variable capacity;
  - non-`Int` constant capacity;
  - `{0 Int}`;
  - negative capacity if parsed.

## Phase gate

- [x] Run:

```bash
rg -n "capacity expression|capacity expressions|ordinary compile-time arithmetic|capacity \\+|fold to a positive|ParsedFixedCapacity|CollectionShape" src docs tests libraries
```

- [x] Remaining source docs describe the new narrow rule. Generated `docs/release/**` and
  `docs/dev/**` output still contains stale generated HTML and was not edited directly.
- [x] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

Validation passed on 2026-06-12 with `cargo fmt`, targeted type/header/dependency tests,
`cargo run -- tests`, and full `just validate`.

- [x] Manual review:
  - [x] type parser does not invoke general expression parsing for capacity;
  - [x] header dependency generation is explicit and small;
  - [x] AST owns semantic constant lookup;
  - [x] HIR/backends consume canonical collection shape.

---

# Phase 8 — Test suite and fixture cleanup

## Context

The tests should describe the final language shape. Delete obsolete success tests rather than ignoring them.

## Tasks

### 8.1 Inventory and categorize

- [x] Search tests and source fixtures:

```bash
rg -n "dynamic trait|DynamicTrait|dynamic-safe|CallDynamicTrait|ConstructDynamicTrait|FileLocalExtension|file-local evidence|HASHABLE|capacity \\+|grouped receiver|receiver method import|receiver-method alias" tests src
```

- [x] Categorize each hit:
  - delete obsolete success case;
  - rewrite into static trait success case;
  - rewrite into choice heterogeneity success case;
  - convert to outside-scope negative diagnostic;
  - keep because it protects a current supported feature.

Inventory result: remaining hits are current intentional coverage: named-constant capacity
arithmetic success, inline capacity-expression rejection, receiver-method import rejection fixtures,
and one unrelated Rust allocation expression. Stale file-local/extension fixture names were renamed
to current rule names, and redundant override/extension fixtures whose old distinctions no longer
reach a distinct diagnostic path were deleted.

### 8.2 Required integration coverage

- [x] Static trait declaration + same-file conformance success.
- [x] Static generic-bound success.
- [x] Method shape without `Type must TRAIT` does not imply conformance.
- [x] Trait name as ordinary type rejected in parameter, return, struct field, choice payload, collection element, alias target.
- [x] Choice-based heterogeneity success replacing dynamic trait examples.
- [x] Same-file struct and choice receiver method success.
- [x] Imported receiver type method call success when method was declared with the type.
- [x] Receiver declaration for imported/builtin/external/nonlocal type rejected.
- [x] Receiver method import and alias rejected.
- [x] Scalar map key success and unsupported key failures.
- [x] Literal and bare-constant fixed capacity success.
- [x] Inline fixed-capacity expression failures.

### 8.3 Remove obsolete coverage

- [x] Delete dynamic trait JS runtime success tests.
- [x] Delete dynamic trait Wasm unsupported tests.
- [x] Delete file-local extension evidence success tests.
- [x] Delete grouped receiver-method import/alias success tests.
- [x] Delete inline capacity-expression success tests.
- [x] Delete `HASHABLE` future tests unless they are converted to outside-scope rejection tests.
- [x] Update `tests/cases/manifest.toml` for renamed/deleted cases.

## Phase gate

- [x] Run:

```bash
cargo run -- tests
just validate
```

- [x] Confirm test names describe current rules, not removed features.
- [x] Confirm failure tests assert stable diagnostic codes where practical.
- [x] Confirm positive tests use realistic Beanstalk snippets.

Validation passed on 2026-06-12 with `cargo run -- tests` and full `just validate`.

---

# Phase 9 — Redundancy, indirection, and noisy-code cleanup

## Context

The restrictions above should shrink the compiler. This phase is not optional: it prevents removed design surface from surviving as dormant abstractions.

## Tasks

### 9.1 Full deletion search

Run:

```bash
rg -n "DynamicTrait|dynamic trait|dynamic-safe|trait object|trait value|dynamic wrapper|dynamic dispatch|dynamic coercion|CallDynamicTrait|ConstructDynamicTrait|reachable_dynamic_trait" .

rg -n "FileLocalExtension|file-local extension|file local extension|visible_receiver_methods|visible_external_receiver_methods|receiver method import|receiver-method import|receiver method alias|receiver-method alias|ExtensionOverridesCanonicalMethod" .

rg -n "HASHABLE|hashable|generic key|generic map|custom hasher|custom comparer|user-defined key|Float key|map equality|entry API|fixed map|capacity map|hashset" .

rg -n "capacity expression|capacity expressions|ordinary compile-time arithmetic|capacity \\+|fold to a positive" .

rg -n "associated type|associated constant|default method|trait inheritance|trait composition|generic trait|generic trait method|specialization|conditional conformance|blanket conformance|negative conformance|operator-to-trait|downcast|reflection|DISPLAYABLE|DEBUG_DISPLAY" .
```

For every hit:

- [x] Delete if obsolete.
- [x] Move to outside-scope docs if it is policy.
- [x] Convert to a negative test if it is a rejection case.
- [x] Keep only if it is a current supported feature or deliberately deferred backend work.

Search result summary:
- Production dynamic-trait value, dynamic dispatch, dynamic-safety, and backend runtime paths were
  not found outside the current static trait metadata and diagnostics surface.
- Receiver-method hits are the retained source receiver-call visibility maps, diagnostic-only
  `by_method_name` lookup, and current rejection coverage/docs.
- Hashmap hits are outside-scope policy docs, negative fixtures, scalar-key diagnostics, ordinary
  Rust hash-map implementation details, or the current scalar-map runtime/backend path.
- Fixed-capacity hits are the current literal-or-bare-constant representation, allowed named-constant
  arithmetic examples, rejection fixtures, and canonical collection-shape consumers.
- Generated docs under `docs/release/**` and `docs/dev/**` remain stale in places and were not
  edited directly.

### 9.2 Simplification targets

#### Trait/type system

- [x] Can `TraitEnvironment` and `TraitEvidenceEnvironment` be AST-only?
- [x] Can stable evidence IDs be removed from backend-facing data?
- [x] Are dynamic-safety fields/methods fully gone?
- [x] Does type resolution reject trait names in ordinary type position early and clearly?
- [x] Is generic-bound dispatch lowered to concrete calls before HIR?
- [x] Are comments in `traits`, `datatypes`, AST, HIR updated to static-only language?

`TraitEnvironment`, `TraitEvidenceEnvironment`, and stable evidence IDs remain frontend/AST state
for generic bounds, concrete evidence receiver fallback, type resolution, and public-surface checks;
no backend-facing dynamic trait data remains.

#### Receiver methods

- [x] Does the source receiver catalog only contain same-file canonical methods?
- [x] Can `ReceiverMethodKind` be deleted?
- [x] Can `by_method_name` be removed or narrowed?
- [x] Are method imports/aliases gone from header visibility structures?
- [x] Are external package methods converted to free functions with no builder-owned receiver metadata left behind?
- [x] Are source methods visible through the receiver type rather than an independent import surface?

`ReceiverMethodKind` is gone. `by_method_name` is intentionally retained only for the targeted
"called as free function" diagnostic and does not drive dispatch. Builder-owned external receiver
metadata is removed; `@web/canvas` is a free-function external package and `@html` owns the
method-style wrapper type.

#### Hashmaps

- [x] Is key validation a direct scalar-key check rather than a trait/capability solver?
- [x] Are custom hash/equality hooks gone?
- [x] Are runtime helpers specialized enough for scalar keys without hiding future generic-key assumptions?
- [x] Are diagnostics explicit about scalar-key-only policy?

The remaining cleanup renamed the stale `unhashable` fixture/test wording to unsupported-key
wording and replaced "key capability" comments with scalar-key policy wording.

#### Fixed capacity

- [x] Is capacity syntax represented as literal-or-bare-const rather than expression tokens?
- [x] Are dependency edges only for bare const names?
- [x] Is arithmetic only allowed in constant declarations?
- [x] Do HIR/backends only see canonical collection shape?

#### Backends

- [x] Are dynamic trait runtime helpers/assets fully removed?
- [x] Are Wasm dynamic-trait unsupported checks removed?
- [x] Are scalar-keyed map unsupported checks still present for Wasm?
- [x] Are unused runtime modules and imports gone?

### 9.3 Comment/doc-comment audit

- [x] Search comments for removed concepts:

```bash
rg -n "//!|///|//" src | rg "dynamic trait|FileLocalExtension|file-local|HASHABLE|capacity expression|dynamic-safe|receiver method import|dynamic dispatch"
```

- [x] Update file-level docs in touched modules, especially:
  - `src/compiler_frontend/traits/mod.rs`;
  - `src/compiler_frontend/datatypes/mod.rs`;
  - `src/compiler_frontend/datatypes/definitions.rs`;
  - `src/compiler_frontend/ast/mod.rs`;
  - `src/compiler_frontend/ast/receiver_methods.rs`;
  - `src/compiler_frontend/hir/mod.rs`;
  - `src/compiler_frontend/hir/expressions.rs`;
  - `src/compiler_frontend/hir/statements.rs`;
  - `src/compiler_frontend/hir/reachability.rs`;
  - backend modules touched by dynamic trait deletion.

Comments should explain current invariants, not deleted history.

Comment audit result: remaining `file-local` source hits describe ordinary visibility/import
ownership, not file-local extension evidence. The surviving stale wording was tightened in the
map-key helper, receiver-method tests, and progress matrix.

### 9.4 Documentation consistency audit

- [x] Review:
  - `README.md`;
  - `docs/language-overview.md`;
  - `docs/compiler-design-overview.md`;
  - `docs/memory-management-design.md`;
  - `docs/codebase-style-guide.md`;
  - `docs/roadmap/roadmap.md`;
  - `docs/src/docs/#page.bst`;
  - `docs/src/docs/design-scope/#page.bst`;
  - `docs/src/docs/language-overview/#page.bst`;
  - `docs/src/docs/traits/#page.bst`;
  - `docs/src/docs/generics/#page.bst`;
  - `docs/src/docs/collections/#page.bst`;
  - `docs/src/docs/structs/#page.bst`;
  - `docs/src/docs/progress/#page.bst`.

Confirm:

- [x] dynamic traits appear only as outside-scope/rejected;
- [x] receiver extensions appear only as outside-scope/rejected;
- [x] `HASHABLE` appears only as outside-scope/rejected;
- [x] map future work is limited to backend/runtime work for scalar-keyed maps;
- [x] fixed capacity examples never use inline arithmetic;
- [x] public `Result` values and exceptions are consistently outside scope;
- [x] general macros/type-level programming are outside scope.

Documentation audit result: canonical docs, docs-source pages, roadmap, README, and progress matrix
now describe static traits, receiver-type visibility, scalar-keyed maps, and narrow fixed-capacity
syntax. Named constants may still use arithmetic before being referenced in capacity type position.

## Phase gate

- [x] Run:

```bash
cargo fmt
cargo clippy
cargo test
cargo run -- tests
just validate
```

- [x] Manual stage-boundary review:
  - [x] AST does not emit removed dynamic trait semantics.
  - [x] HIR does not represent removed dynamic trait operations.
  - [x] Backends contain no dead dynamic trait runtime paths.
  - [x] Header/import visibility has no independent receiver-method import state.
  - [x] Diagnostics distinguish deferred from outside scope.
  - [x] No compatibility wrappers, forwarding shims, dead variants, or stale comments remain.
  - [x] Tests cover behavior rather than implementation accidents.

Validation passed on 2026-06-12 with `cargo fmt`, focused map-key unit tests,
`cargo run -- tests`, and full `just validate`.

---

# Final acceptance checklist

- [x] `docs/language-overview.md` contains `Language Design Scope` and `Outside the Language Design Scope` sections.
- [x] `docs/compiler-design-overview.md` describes static-only traits and no dynamic trait HIR/backend path.
- [x] `docs/roadmap/roadmap.md` no longer lists dynamic traits, `HASHABLE`, broad trait features, or user-defined map keys as ordinary follow-ups.
- [x] `docs/src/docs/progress/#page.bst` has an `Outside Scope` status and updated traits/receiver/collections/hashmap rows.
- [x] `docs/src/docs/traits/#page.bst` teaches static-only traits.
- [x] `docs/src/docs/generics/#page.bst` no longer refers to dynamic trait values.
- [x] `docs/src/docs/collections/#page.bst` documents scalar-keyed hashmaps and narrow capacity syntax.
- [x] `docs/src/docs/structs/#page.bst` documents same-file nominal receiver methods only.
- [x] `README.md` mentions the deliberately small language surface.
- [x] Trait names in ordinary type position are rejected with structured diagnostics.
- [x] Dynamic trait `DataType`, `TypeDefinition`, AST coercions, HIR nodes, reachability facts, backend/runtime support, and tests are deleted or rewritten.
- [x] Source-authored receiver extensions for builtin/imported/external/nonlocal types are rejected.
- [x] Receiver methods are not independently imported, aliased, or re-exported.
- [x] External packages expose opaque types, constants, and free functions only.
- [x] `@web/canvas` uses a raw free-function API, and `@html` owns source wrapper methods for canvas ergonomics.
- [x] Generated integration fixture and benchmark build-output directories are ignored and benchmark `dev/` artifacts are untracked.
- [x] File-local extension evidence is removed.
- [x] Builtin map keys are scalar-only and diagnostics do not mention future `HASHABLE`.
- [x] Fixed capacity type syntax accepts only integer literals or bare `#Int` constant names.
- [x] Obsolete comments, doc comments, enum variants, helper structs, tests, and runtime helpers are removed.
- [x] `just validate` passes.
