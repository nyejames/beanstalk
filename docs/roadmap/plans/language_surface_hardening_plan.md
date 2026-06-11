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

- [ ] Confirm branch, working tree, and commit:

```bash
git status --short
git branch --show-current
git rev-parse HEAD
```

- [ ] Create or switch to a focused branch:

```bash
git switch -c language-surface-hardening
```

- [ ] Run the validation baseline before edits:

```bash
just validate
```

- [ ] Record any pre-existing failures. Do not mix unrelated fixes into this work.

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

- [ ] Create a local temporary note, not committed unless it becomes useful:

```text
/tmp/beanstalk-language-surface-hardening-audit.md
```

Record:

- [ ] current commit SHA;
- [ ] validation baseline;
- [ ] dynamic trait files and tests found;
- [ ] receiver extension/import files and tests found;
- [ ] `HASHABLE` / generic-map future mentions found;
- [ ] fixed-capacity expression files and tests found;
- [ ] docs pages requiring changes;
- [ ] diagnostic owners and likely test owners.

## Phase gate

- [ ] No production code has changed yet.
- [ ] The actual diagnostic owner is identified. If `src/compiler_frontend/deferred_feature_diagnostics.rs` does not exist, update docs and use the current `compiler_messages` structure instead of creating a stale path.
- [ ] Search results are complete enough to guide deletion and docs updates.

---

# Phase 1 — Canonical design-scope documentation

## Context

`docs/language-overview.md` is the compiler-facing language facts document. It should define the design boundary before the implementation is changed. Keep it factual and concise; put tutorials in `docs/src/docs/**`.

## Tasks

### 1.1 Add design-scope taxonomy to `docs/language-overview.md`

- [ ] Add this near the top, after `Design principles` and before `Related references`:

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

- [ ] Add this section immediately after it:

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

- [ ] Replace the Syntax Summary trait row with:

```markdown
| Traits | Trait declarations and conformances use `must`; generic bounds use `is`. Trait names are static contracts, not value types. |
```

- [ ] Replace fixed-capacity examples that use inline arithmetic:

```beanstalk
capacity #Int = 4
scratch ~{capacity String} = {}

larger_capacity #Int = capacity + 2
larger_scratch ~{larger_capacity String} = {}
```

- [ ] Replace the fixed-capacity rule with:

```markdown
- Fixed capacity in type position must be either a positive `Int` literal or a bare visible `#Int`
  constant name. Capacity position is not general expression position: arithmetic, function calls,
  field access, const-record projection, conditionals, and nested expressions are invalid there.
  Put any calculation in a named compile-time constant first.
```

- [ ] Replace hashmap key rules with:

```markdown
- Builtin hashmap key types are permanently limited to `String`, `Int`, `Bool`, and `Char`.
  This is a language-owned map surface, not a general hashing abstraction.
- `Float`, structs, choices, collections, hashmaps, traits, functions, external opaque types,
  templates as a distinct key type, and generic parameters are invalid keys.
```

- [ ] Replace the hashmap deferred list with:

```markdown
Outside the builtin hashmap design scope: hashsets as language syntax, user-defined hashers or
comparers, `Float` keys, user-defined key types, generic key maps through `HASHABLE`, map equality,
mutable entry APIs, indexing syntax, const hashmaps, fixed/capacity maps, and specialized map
variants. More sophisticated maps should be ordinary standard-library or user-defined structs.

Wasm hashmap runtime/lowering remains deferred backend work for the existing scalar-keyed builtin map
surface.
```

- [ ] Replace receiver-method rules with same-file nominal rules:

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

- [ ] Replace receiver visibility with:

```markdown
Receiver method visibility is tied to receiver type visibility.

- A source-authored receiver method is visible wherever its receiver type is visible.
- Receiver methods are not imported, aliased, or re-exported independently.
- Namespace imports may make the receiver type visible, but receiver methods are not namespace fields.
- A `#mod.bst` facade exposes a type's same-file receiver methods when it exposes the type.
- Use free functions for private helpers or operations on types owned by other files or packages.
```

- [ ] Replace the trait dynamic-value text with:

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

- [ ] Replace conformance evidence rules with:

```markdown
- Canonical conformance evidence for same-file structs, choices, and generic type constructors is
  reusable wherever both the type and trait are visible.
- User-authored conformance for builtins, imported types, dependency/library types, external opaque
  types, and types declared in another file is rejected.
```

- [ ] In the generics section, remove dynamic-trait references and replace with:

```markdown
Traits are static contracts only. Trait names cannot appear as value types, so there is no dynamic
trait value that can satisfy or fail a static generic bound.
```

- [ ] Split rejected/deferred generic list into true deferred work vs outside-scope work. Move these to outside-scope wording:
  - generic function values / higher-order polymorphism;
  - type values, type-returning functions, type-level `#if`, compile-time type inspection;
  - file-local evidence-backed generic bound dispatch;
  - parameterized generic aliases and partial type application;
  - higher-kinded types, const generics, lifetime parameters, specialization, associated types.

### 1.3 Update `docs/compiler-design-overview.md`

- [ ] Replace the `src/compiler_frontend/traits/` overview bullet with static-only ownership:

```markdown
`src/compiler_frontend/traits/` owns parsed trait shells, resolved trait definitions, explicit
same-file nominal conformance evidence, reusable evidence visibility, static generic-bound evidence
checks, and trait diagnostics. Trait metadata is compile-time frontend state, not a value-type or
backend rediscovery path.
```

- [ ] Remove or correct any stale reference to `src/compiler_frontend/deferred_feature_diagnostics.rs`. Preferred wording:

```markdown
Design-scope and deferred-feature diagnostics should be centralized through typed
`CompilerDiagnostic` constructors. Deferred features and outside-design-scope rejections must remain
distinct diagnostic reasons.
```

- [ ] Replace import/receiver-method visibility text with:

```markdown
Header import preparation does not import receiver methods as independent symbols. Source-authored
receiver methods belong to their receiver type's declaring file and become callable wherever the
receiver type is visible. Namespace imports may make a receiver type visible, but methods are never
namespace fields and cannot be grouped-imported or aliased independently.
```

- [ ] Replace the AST trait ownership bullet with:

```markdown
trait declaration resolution, trait visibility, same-file conformance evidence validation,
static generic-bound evidence checks, and evidence-backed static receiver fallback
```

- [ ] Replace the full Traits contract with:

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

- [ ] Remove HIR ownership bullets about dynamic trait construction/dispatch and trait metadata projected across the backend boundary.
- [ ] Delete the `### Dynamic trait operations` section.
- [ ] Update backend lowering text so reachable validation mentions external calls and scalar-keyed maps, not dynamic trait runtime operations.

### 1.4 Phase gate

- [x] Run:

```bash
just validate
```

- [ ] Search docs:

```bash
rg -n "dynamic trait values are|dynamic-safe|dynamic trait runtime|HASHABLE.*future|capacity \\+|grouped receiver|receiver method import|FileLocalExtension" docs README.md
```

- [ ] Remaining hits are only in outside-scope policy text or negative examples.
- [ ] Manual review confirms `docs/language-overview.md` remains compiler-facing, not tutorial-heavy.

---

# Phase 2 — Docs-site, roadmap, progress matrix, README

## Context

The docs site is real Beanstalk code and user-facing. The progress matrix and roadmap must not keep old “maybe later” entries that contradict the new design.

## Tasks

### 2.1 Add docs-site design-scope page

- [ ] Add `docs/src/docs/design-scope/#page.bst`.
- [ ] Keep it short. Use the same taxonomy as canonical docs.
- [ ] Include these examples:

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

- [ ] Add a concise outside-scope table. Do not duplicate every line of `docs/language-overview.md`; group related features.

### 2.2 Update docs index

Path: `docs/src/docs/#page.bst`

- [ ] Add under Introduction:

```beanstalk
- @./design-scope (Language Design Scope)
```

### 2.3 Update traits docs-site page

Path: `docs/src/docs/traits/#page.bst`

- [ ] Change page description to static-only traits.
- [ ] Remove sections for:
  - dynamic trait values;
  - dynamic-safe traits;
  - dynamic dispatch and backend runtime behavior;
  - file-local evidence.
- [ ] Rename “Canonical and file-local evidence” to “Canonical evidence.”
- [ ] State that conformances are reusable only for same-file structs, choices, and generic type constructors.
- [ ] State that conformance for builtins, imported/dependency/library types, external opaque types, and other-file types is rejected.
- [ ] Add “Runtime heterogeneity uses choices” with a short example.
- [ ] Replace “Deferred trait ecosystem” with “Outside trait design scope.” Include:
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

- [ ] Remove dynamic-trait references.
- [ ] Replace trait-bound explanation with:

```text
A trait name in a generic bound constrains a concrete generic parameter. Trait names are not value
types, so a parameter, field, collection element, or return type cannot be annotated directly with a
trait name.
```

- [ ] Replace file-local evidence-backed generic-bound dispatch with outside-scope wording.
- [ ] Split future list into:
  - **Deferred generic work**: only items still genuinely plausible, such as recursive generic types, generic external package functions/types, or nested `of` relaxation if still desired.
  - **Outside generic design scope**: explicit generic call syntax, higher-order polymorphism, type-level computation, `where` clauses, parameterized aliases, partial application, HKT, const generics beyond fixed capacity, lifetimes, specialization, associated types, dynamic reification.

### 2.5 Update collections docs-site page

Path: `docs/src/docs/collections/#page.bst`

- [ ] Replace capacity text with literal-or-bare-constant rule.
- [ ] Replace `{capacity + 2 String}` examples with named-constant examples.
- [ ] Replace stale hashmap section with implemented V1 syntax:

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

- [ ] State that builtin map keys are permanently `String`, `Int`, `Bool`, `Char`.
- [ ] State that sophisticated maps belong in library/user-defined structs.

### 2.6 Update structs and docs-site language overview pages

Paths:

- `docs/src/docs/structs/#page.bst`
- `docs/src/docs/language-overview/#page.bst`

- [ ] Replace builtin/imported/external receiver method text with same-file nominal rule.
- [ ] Remove grouped receiver-method import/alias text.
- [ ] State that receiver methods are visible through the receiver type, not as imported symbols.
- [ ] Update external JS signature docs: source-authored `@bst.sig` should expose free functions and opaque types, not `this` receiver methods.

### 2.7 Update roadmap

Path: `docs/roadmap/roadmap.md`

- [ ] Add an `Outside Language Design Scope` section near the top.
- [ ] Move these out of roadmap follow-ups:
  - dynamic trait values / trait objects / dynamic trait runtime lowering;
  - dynamic wrappers in Wasm ABI mapping;
  - `HASHABLE`, generic map keys, user-defined map keys, custom hashers/comparers, `Float` map keys, map equality, entry APIs, fixed/capacity maps, language hashsets;
  - default trait methods, associated types/constants, inheritance/composition, generic traits/methods, conditional/specialized/blanket/negative conformance, dynamic composition/downcasting/reflection, output coercion, operator integration;
  - first-class public `Result`, exceptions, reflection, type-level programming, HKT, parameterized aliases, partial application.

- [ ] Keep only legitimate deferred work:
  - Wasm runtime/lowering for existing scalar-keyed maps;
  - possible read-only map iteration only if it does not introduce `HASHABLE`, custom equality, or mutable entry APIs;
  - diagnostics/tooling polish for static traits;
  - backend work unrelated to removed dynamic trait runtime.

### 2.8 Update progress matrix

Path: `docs/src/docs/progress/#page.bst`

- [ ] Add status row:

```beanstalk
[data, red:
    [: Outside Scope]
    [: Intentionally not planned for Beanstalk's language design. Syntax should be rejected with structured diagnostics when encountered.]
]
```

- [ ] Update Traits row to static-only surface.
- [ ] Remove dynamic coercion, JS dynamic runtime, Wasm dynamic unsupported, dynamic-safe, dynamic wrappers, and dynamic dispatch coverage text.
- [ ] Update Structs/receiver row to same-file nominal rule.
- [ ] Update Collections row to narrow capacity syntax.
- [ ] Update Hash Maps row to permanent scalar-key policy and outside-scope map features.
- [ ] Update External Packages row so source-authored external imports expose free functions and opaque types. If builder-owned member syntax still exists temporarily, label it as builder-owned metadata and isolate it from source-authored receiver methods.

### 2.9 Update README

- [ ] Add one concise goal bullet:

```markdown
- A deliberately small language surface: static nominal types, explicit trait conformance, constrained generics, no general macro system, and no Rust-style type-level programming.
```

- [ ] Optionally add a link to the language design scope section/page if the README documentation list remains short.

## Phase gate

- [ ] Run:

```bash
just validate
```

- [ ] Search docs site and roadmap:

```bash
rg -n "dynamic trait|dynamic-safe|FileLocalExtension|file-local evidence|HASHABLE|capacity \\+|grouped receiver|receiver method import|receiver-method alias|this.*@bst.sig|deferred trait ecosystem" docs/src/docs docs/roadmap README.md
```

- [ ] Remaining hits are only outside-scope policy text or negative examples.

---

# Phase 3 — Diagnostic taxonomy and rejection paths

## Context

Out-of-scope features should be rejected as intentional language rules, not as missing implementations. Build the diagnostic vocabulary before deleting systems so parser/type-resolution changes can fail cleanly.

## Tasks

### 3.1 Locate the diagnostic owner

- [ ] Search current diagnostic structure:

```bash
fd diagnostic src/compiler_frontend
rg -n "DeferredFeature|deferred feature|not implemented|unsupported feature|DiagnosticPayload|Invalid.*Reason|Rule" src/compiler_frontend/compiler_messages src/compiler_frontend
```

- [ ] Add outside-scope reasons to the existing diagnostic structure. Do not create `src/compiler_frontend/deferred_feature_diagnostics.rs` unless that is already the live module pattern.
- [ ] Keep “deferred feature” and “outside design scope” as distinct typed reasons.

Recommended conceptual shape, adapted to existing enums:

```rust
pub enum LanguageSurfaceRejectionReason {
    Deferred(DeferredFeatureReason),
    OutsideDesignScope(OutsideDesignScopeReason),
}
```

Only add reason variants that current syntax/resolution can reach.

### 3.2 Required outside-scope diagnostic reasons

- [ ] Dynamic trait value type.
- [ ] Receiver method for nonlocal source type.
- [ ] Receiver method for builtin scalar.
- [ ] Receiver method for external opaque type.
- [ ] Receiver method import or alias.
- [ ] Trait conformance for builtin/imported/external/nonlocal type.
- [ ] General fixed-capacity expression.
- [ ] Unsupported builtin map key.
- [ ] `HASHABLE` / generic builtin map key.
- [ ] `where` clause, if currently parsed/rejected.
- [ ] Parameterized alias / partial application, if currently parsed/rejected.
- [ ] Dynamic trait composition/downcast syntax, if any reserved syntax exists.
- [ ] Trait default/associated/inheritance/generic method syntax, if any reserved syntax exists.

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

- [ ] Add or update negative integration tests asserting stable diagnostic codes for:
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

## Phase gate

- [ ] Run:

```bash
cargo fmt
cargo clippy
cargo test
cargo run -- tests
just validate
```

- [ ] Manual review:
  - [ ] diagnostics are typed and source-located;
  - [ ] no rendered strings are semantic state;
  - [ ] no user-facing source rejection uses `CompilerError`;
  - [ ] no diagnostic mentions old behavior;
  - [ ] no boolean-heavy helper where a reason enum would be clearer.

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

## Tasks

### 5.1 Source receiver policy

- [ ] Enforce:

```text
A source-authored receiver method is valid only when its receiver is a user-defined struct or choice
declared in the same source file as the method.
```

- [ ] For generic nominal receivers, allow only declaration-site parameters aligned with the receiver type’s own parameters.
- [ ] Reject receiver methods for:
  - builtins;
  - imported source types;
  - dependency/library types;
  - external opaque types;
  - same-module but different-file types;
  - concrete generic instances.

### 5.2 Refactor `src/compiler_frontend/ast/receiver_methods.rs`

- [ ] Remove `ReceiverMethodKind::FileLocalExtension`.
- [ ] Remove `ReceiverMethodKind` entirely if every source method is canonical.
- [ ] Replace `receiver_method_kind_for_declaration` with a direct validator:

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

- [ ] Remove visibility fallback that turns merely-visible receiver types into extension targets.
- [ ] Remove extension override checks.
- [ ] Remove duplicate logic needed only for canonical/extension collision.
- [ ] Keep same-receiver duplicate method diagnostics.
- [ ] Keep field-name conflict validation for struct methods.
- [ ] Keep receiver-call mutability rules.
- [ ] Audit `by_method_name`: keep only if it materially improves “called as free function” diagnostics; otherwise remove it.

### 5.3 Simplify receiver visibility/imports

- [ ] Remove independent receiver-method imports from header import parsing.
- [ ] Remove grouped receiver-method import parsing and aliasing.
- [ ] Remove `visible_receiver_methods` if it is no longer needed.
- [ ] Remove `visible_external_receiver_methods` if external methods are removed or isolated from source methods.
- [ ] Receiver methods should not reserve ordinary value/import aliases.
- [ ] Receiver method names may repeat across different receiver types.
- [ ] Method names must still not conflict with fields on the same struct if that remains the rule.

### 5.4 Facade rule

- [ ] Implement:

```text
A facade exposes a type's same-file receiver methods when it exposes the type.
Receiver methods are not independently exported or aliased.
```

- [ ] Remove explicit receiver-method re-export logic.
- [ ] Decide transparent alias behavior and document/test it.
  - Recommended: transparent aliases expose the same receiver method surface because they are the same type.

### 5.5 External package member surface

- [ ] Preferred strict target: external packages expose free functions and opaque types, not source-style receiver methods.
- [ ] Reject `this` receiver parameters in source-authored `@bst.sig` JS imports.
- [ ] Convert builder-owned external receiver APIs, such as `@web/canvas`, to free functions where feasible.
- [ ] If a builder-owned member API must remain temporarily, isolate it from the source receiver-method catalog and document it as builder-owned external metadata only. It must not use grouped receiver-method imports, file-local extension evidence, or source-authored receiver declaration paths.

### 5.6 Conformance ownership

- [ ] Enforce same-file nominal conformance:

```text
User-authored `Type must TRAIT` is valid only when `Type` is a user-defined struct, choice, or
generic type constructor declared in the same file.
```

- [ ] Reject conformance for builtins, imported/dependency/library types, external opaque types, and other-file types.
- [ ] Remove file-local extension evidence.
- [ ] Remove generic-bound dispatch through file-local evidence.
- [ ] Keep reusable evidence wherever both the type and trait are visible.

### 5.7 Tests

- [ ] Positive:
  - same-file struct method;
  - same-file choice method;
  - aligned generic nominal receiver method;
  - method call after importing receiver type;
  - method call through namespace import when receiver type is visible;
  - facade exposes type and methods together;
  - same-file nominal trait conformance;
  - generic bound using visible reusable evidence.

- [ ] Negative:
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

- [ ] Run:

```bash
rg -n "FileLocalExtension|file-local extension|file local extension|file-local evidence|file local evidence|visible_receiver_methods|visible_external_receiver_methods|grouped receiver|receiver method import|receiver-method import|receiver method alias|receiver-method alias|ExtensionOverridesCanonicalMethod" src docs tests libraries
```

- [ ] Remaining hits are only outside-scope docs or negative tests.
- [ ] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

- [ ] Manual review:
  - [ ] Header/import stage no longer owns independent receiver-method visibility.
  - [ ] AST receiver lookup uses receiver type visibility and same-file method catalog.
  - [ ] Trait evidence is not split into reusable vs file-local extension evidence.
  - [ ] No stale comments describe extension methods or file-local evidence as supported.

---

# Phase 6 — Harden scalar-keyed builtin hashmaps

## Context

Builtin maps are already close to the desired surface. The main work is removing future `HASHABLE` pressure and any over-generalized key scaffolding.

## Tasks

### 6.1 Key policy

- [ ] Ensure builtin map key validation accepts only:
  - `String`;
  - `Int`;
  - `Bool`;
  - `Char`.

- [ ] Ensure it rejects:
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

- [ ] Remove “before future `HASHABLE`” wording from diagnostics and comments.
- [ ] Remove any `HASHABLE`, custom hash/equality, or generic-key scaffolding not required by scalar-key validation.
- [ ] Keep value-side behavior according to current runtime-storable rules.

### 6.2 HIR/backend simplification

- [ ] Keep first-class HIR map literal/op nodes.
- [ ] Keep HIR reachability for map construction/use because HTML-Wasm still rejects reachable maps.
- [ ] Keep JS runtime/helper lowering for scalar-keyed maps.
- [ ] Simplify key normalization/runtime helpers to assume the scalar-key policy where possible.
- [ ] Remove hooks for custom hash/equality policies if any exist.

### 6.3 Tests

- [ ] Positive:
  - `String`, `Int`, `Bool`, `Char` keys;
  - alias to supported scalar key.

- [ ] Negative:
  - `Float` key;
  - struct key;
  - choice key;
  - collection key;
  - map key;
  - external opaque key;
  - generic parameter key;
  - alias to unsupported key.

## Phase gate

- [ ] Run:

```bash
rg -n "HASHABLE|hashable|generic key|generic map|user-defined key|user defined key|custom hasher|custom comparer|map equality|Float key|fixed map|capacity map|hashset" src docs tests libraries
```

- [ ] Remaining hits are only outside-scope docs, negative tests, or legitimate scalar-map backend work.
- [ ] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

- [ ] Manual review:
  - [ ] map key validation is direct and not solver/trait based;
  - [ ] diagnostics do not imply `HASHABLE` is coming;
  - [ ] backend code does not carry unused custom-key hooks.

---

# Phase 7 — Restrict fixed collection capacity syntax

## Context

Capacity arithmetic should remain possible in named constants, but type-position capacity itself should not be general expression syntax.

## Tasks

### 7.1 Parser/type representation

- [ ] Find the parsed representation for fixed collection capacity.
- [ ] Replace general expression/token storage with a narrow representation if practical:

```rust
pub enum ParsedFixedCapacity {
    Literal { value: i64, location: SourceLocation },
    ConstName { name: StringId, location: SourceLocation },
}
```

Adapt to existing parsed type structures and interned path conventions.

- [ ] Accept only:
  - positive integer literal;
  - bare visible `#Int` constant name.

- [ ] Reject:
  - arithmetic, including `capacity + 2` and `2 + 2`;
  - function calls;
  - field access / const-record projection;
  - conditionals;
  - nested/parenthesized expressions unless the parser already treats `(4)` as a literal; recommended: reject for simplicity;
  - namespace-qualified constants unless explicitly retained. Recommended strict rule: require a bare imported alias/local constant name.

### 7.2 Header dependency edges

- [ ] Keep dependency edges only for bare constant-name capacity.
- [ ] Remove general expression reference walking for capacity type position.
- [ ] Preserve current source-order/import rules for visible constants.
- [ ] If imported constants are allowed, prefer requiring a bare grouped import alias:

```beanstalk
import @sizes { default_capacity }
items {default_capacity String} = {}
```

### 7.3 AST capacity resolution

- [ ] Resolve literal capacity directly.
- [ ] Resolve bare `#Int` capacity by looking up the folded constant value.
- [ ] Reject non-`Int`, non-constant, non-positive, overflow, and unresolved names with structured diagnostics.
- [ ] Ensure diagnostics say to put arithmetic in a named compile-time constant first.
- [ ] HIR/backends continue consuming canonical collection shape only.

### 7.4 Tests

- [ ] Positive:
  - `{4 Int}`;
  - `{capacity Int}` with `capacity #Int = 4`;
  - `larger_capacity #Int = capacity + 2` then `{larger_capacity String}`;
  - fixed literal over-capacity rejection still works;
  - aliases preserve fixed shape.

- [ ] Negative:
  - `{capacity + 2 Int}`;
  - `{2 + 2 Int}`;
  - `{get_capacity() Int}`;
  - `{defaults.capacity Int}`;
  - runtime variable capacity;
  - non-`Int` constant capacity;
  - `{0 Int}`;
  - negative capacity if parsed.

## Phase gate

- [ ] Run:

```bash
rg -n "capacity expression|capacity expressions|ordinary compile-time arithmetic|capacity \\+|fold to a positive|ParsedFixedCapacity|CollectionShape" src docs tests libraries
```

- [ ] Remaining docs describe the new narrow rule.
- [ ] Run:

```bash
cargo fmt
cargo test
cargo run -- tests
cargo clippy
just validate
```

- [ ] Manual review:
  - [ ] type parser does not invoke general expression parsing for capacity;
  - [ ] header dependency generation is explicit and small;
  - [ ] AST owns semantic constant lookup;
  - [ ] HIR/backends consume canonical collection shape.

---

# Phase 8 — Test suite and fixture cleanup

## Context

The tests should describe the final language shape. Delete obsolete success tests rather than ignoring them.

## Tasks

### 8.1 Inventory and categorize

- [ ] Search tests and source fixtures:

```bash
rg -n "dynamic trait|DynamicTrait|dynamic-safe|CallDynamicTrait|ConstructDynamicTrait|FileLocalExtension|file-local evidence|HASHABLE|capacity \\+|grouped receiver|receiver method import|receiver-method alias" tests src
```

- [ ] Categorize each hit:
  - delete obsolete success case;
  - rewrite into static trait success case;
  - rewrite into choice heterogeneity success case;
  - convert to outside-scope negative diagnostic;
  - keep because it protects a current supported feature.

### 8.2 Required integration coverage

- [ ] Static trait declaration + same-file conformance success.
- [ ] Static generic-bound success.
- [ ] Method shape without `Type must TRAIT` does not imply conformance.
- [ ] Trait name as ordinary type rejected in parameter, return, struct field, choice payload, collection element, alias target.
- [ ] Choice-based heterogeneity success replacing dynamic trait examples.
- [ ] Same-file struct and choice receiver method success.
- [ ] Imported receiver type method call success when method was declared with the type.
- [ ] Receiver declaration for imported/builtin/external/nonlocal type rejected.
- [ ] Receiver method import and alias rejected.
- [ ] Scalar map key success and unsupported key failures.
- [ ] Literal and bare-constant fixed capacity success.
- [ ] Inline fixed-capacity expression failures.

### 8.3 Remove obsolete coverage

- [ ] Delete dynamic trait JS runtime success tests.
- [ ] Delete dynamic trait Wasm unsupported tests.
- [ ] Delete file-local extension evidence success tests.
- [ ] Delete grouped receiver-method import/alias success tests.
- [ ] Delete inline capacity-expression success tests.
- [ ] Delete `HASHABLE` future tests unless they are converted to outside-scope rejection tests.
- [ ] Update `tests/cases/manifest.toml` for renamed/deleted cases.

## Phase gate

- [ ] Run:

```bash
cargo run -- tests
just validate
```

- [ ] Confirm test names describe current rules, not removed features.
- [ ] Confirm failure tests assert stable diagnostic codes where practical.
- [ ] Confirm positive tests use realistic Beanstalk snippets.

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

- [ ] Delete if obsolete.
- [ ] Move to outside-scope docs if it is policy.
- [ ] Convert to a negative test if it is a rejection case.
- [ ] Keep only if it is a current supported feature or deliberately deferred backend work.

### 9.2 Simplification targets

#### Trait/type system

- [ ] Can `TraitEnvironment` and `TraitEvidenceEnvironment` be AST-only?
- [ ] Can stable evidence IDs be removed from backend-facing data?
- [ ] Are dynamic-safety fields/methods fully gone?
- [ ] Does type resolution reject trait names in ordinary type position early and clearly?
- [ ] Is generic-bound dispatch lowered to concrete calls before HIR?
- [ ] Are comments in `traits`, `datatypes`, AST, HIR updated to static-only language?

#### Receiver methods

- [ ] Does the source receiver catalog only contain same-file canonical methods?
- [ ] Can `ReceiverMethodKind` be deleted?
- [ ] Can `by_method_name` be removed or narrowed?
- [ ] Are method imports/aliases gone from header visibility structures?
- [ ] Are external package methods either converted to free functions or isolated as builder-owned metadata?
- [ ] Are source methods visible through the receiver type rather than an independent import surface?

#### Hashmaps

- [ ] Is key validation a direct scalar-key check rather than a trait/capability solver?
- [ ] Are custom hash/equality hooks gone?
- [ ] Are runtime helpers specialized enough for scalar keys without hiding future generic-key assumptions?
- [ ] Are diagnostics explicit about scalar-key-only policy?

#### Fixed capacity

- [ ] Is capacity syntax represented as literal-or-bare-const rather than expression tokens?
- [ ] Are dependency edges only for bare const names?
- [ ] Is arithmetic only allowed in constant declarations?
- [ ] Do HIR/backends only see canonical collection shape?

#### Backends

- [ ] Are dynamic trait runtime helpers/assets fully removed?
- [ ] Are Wasm dynamic-trait unsupported checks removed?
- [ ] Are scalar-keyed map unsupported checks still present for Wasm?
- [ ] Are unused runtime modules and imports gone?

### 9.3 Comment/doc-comment audit

- [ ] Search comments for removed concepts:

```bash
rg -n "//!|///|//" src | rg "dynamic trait|FileLocalExtension|file-local|HASHABLE|capacity expression|dynamic-safe|receiver method import|dynamic dispatch"
```

- [ ] Update file-level docs in touched modules, especially:
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

### 9.4 Documentation consistency audit

- [ ] Review:
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

- [ ] dynamic traits appear only as outside-scope/rejected;
- [ ] receiver extensions appear only as outside-scope/rejected;
- [ ] `HASHABLE` appears only as outside-scope/rejected;
- [ ] map future work is limited to backend/runtime work for scalar-keyed maps;
- [ ] fixed capacity examples never use inline arithmetic;
- [ ] public `Result` values and exceptions are consistently outside scope;
- [ ] general macros/type-level programming are outside scope.

## Phase gate

- [ ] Run:

```bash
cargo fmt
cargo clippy
cargo test
cargo run -- tests
just validate
```

- [ ] Manual stage-boundary review:
  - [ ] AST does not emit removed dynamic trait semantics.
  - [ ] HIR does not represent removed dynamic trait operations.
  - [ ] Backends contain no dead dynamic trait runtime paths.
  - [ ] Header/import visibility has no independent receiver-method import state.
  - [ ] Diagnostics distinguish deferred from outside scope.
  - [ ] No compatibility wrappers, forwarding shims, dead variants, or stale comments remain.
  - [ ] Tests cover behavior rather than implementation accidents.

---

# Final acceptance checklist

- [ ] `docs/language-overview.md` contains `Language Design Scope` and `Outside the Language Design Scope` sections.
- [ ] `docs/compiler-design-overview.md` describes static-only traits and no dynamic trait HIR/backend path.
- [ ] `docs/roadmap/roadmap.md` no longer lists dynamic traits, `HASHABLE`, broad trait features, or user-defined map keys as ordinary follow-ups.
- [ ] `docs/src/docs/progress/#page.bst` has an `Outside Scope` status and updated traits/receiver/collections/hashmap rows.
- [ ] `docs/src/docs/traits/#page.bst` teaches static-only traits.
- [ ] `docs/src/docs/generics/#page.bst` no longer refers to dynamic trait values.
- [ ] `docs/src/docs/collections/#page.bst` documents scalar-keyed hashmaps and narrow capacity syntax.
- [ ] `docs/src/docs/structs/#page.bst` documents same-file nominal receiver methods only.
- [ ] `README.md` mentions the deliberately small language surface.
- [ ] Trait names in ordinary type position are rejected with structured diagnostics.
- [ ] Dynamic trait `DataType`, `TypeDefinition`, AST coercions, HIR nodes, reachability facts, backend/runtime support, and tests are deleted or rewritten.
- [ ] Source-authored receiver extensions for builtin/imported/external/nonlocal types are rejected.
- [ ] Receiver methods are not independently imported, aliased, or re-exported.
- [ ] File-local extension evidence is removed.
- [ ] Builtin map keys are scalar-only and diagnostics do not mention future `HASHABLE`.
- [ ] Fixed capacity type syntax accepts only integer literals or bare `#Int` constant names.
- [ ] Obsolete comments, doc comments, enum variants, helper structs, tests, and runtime helpers are removed.
- [ ] `just validate` passes.
