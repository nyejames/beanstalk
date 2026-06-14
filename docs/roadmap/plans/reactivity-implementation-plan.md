# Beanstalk Reactivity V1 Implementation Plan

## Purpose

Implement Beanstalk's first reactivity slice without adding closures, anonymous function values, generic function values, or higher-order polymorphism.

Reactivity V1 is the constrained UI-oriented mechanism for explicit state, explicit template subscriptions, and backend-owned runtime updates. It should cover the core UI cases that normally push languages toward closure-heavy callback patterns while preserving Beanstalk's small, readable language surface.

V1 implements:

- reactive storage declarations: `name $Type = value` and `name $= value`;
- reactive parameters: `param $Type`;
- template subscriptions: `$(source)` inside template head/capture positions only;
- bare reactive source subscriptions only;
- reactive template metadata through AST, HIR, borrow validation, and backend validation;
- HTML-JS top-level runtime fragment mounting and whole-fragment rerendering;
- explicit deferred diagnostics for unsupported reactive sinks and unsupported backends.

V1 does not implement:

- general closures or function values;
- event/action/effect syntax;
- reactive template control flow;
- field/path subscriptions;
- expression dependency tracking;
- fine-grained DOM updates;
- reactive IO sinks;
- HTML-Wasm reactive runtime support.

Beanstalk is pre-release. Do not add compatibility syntax, compatibility diagnostics, compatibility wrappers, or transitional APIs.

---

## Final agreed design

### Language scope

- [ ] General closures, anonymous function values, generic function values, and higher-order polymorphism are outside the current language design scope.
- [ ] These features may remain permanently outside scope. Any post-Alpha reevaluation requires an explicit language-philosophy change.
- [ ] Reactivity is a constrained language/template mechanism intended to remove many closure-heavy UI use cases.
- [ ] Template-owned event/action/effect features are future UI work, not V1.

### Reactive declarations and parameters

- [ ] `$Type` is reactive access syntax, not a first-class wrapper type.
- [ ] `$Type` is valid only on reactive declarations and reactive parameters in V1.
- [ ] `$=` is an inferred reactive declaration form.
- [ ] `$` prefixes the whole ordinary type annotation: `${String}`, `${String = Int}`, `${4 String}`, `$User?`, etc.
- [ ] Reactive identity is binding/source metadata, not a semantic `TypeId`.
- [ ] A reactive declaration owns mutation-capable stable storage in its declaring scope.
- [ ] A reactive parameter is a read/subscription handle to an existing source. It does not grant mutation permission.
- [ ] `$T` parameters can receive only existing reactive sources.
- [ ] Passing a reactive source to an ordinary `T` parameter is a snapshot read unless a reactive template string value is passed through a `String` parameter.
- [ ] Reactive declarations are runtime-local only and cannot be exported or imported as top-level declarations.
- [ ] Reactive declarations are allowed in entry/start code and function bodies.
- [ ] Reactive locals captured by returned/runtime templates become template-instance state candidates.

### Mutation and invalidation

- [ ] Assignment to a reactive declaration updates the same stable source and invalidates subscribers.
- [ ] Mutating through a reactive source invalidates the whole source in V1.
- [ ] Source-level invalidation covers assignment, field write, collection/map mutation, and mutable write-through calls.
- [ ] V1 has no field/item/path-level invalidation.
- [ ] Reactive subscriptions are not mutable borrows and must not block later ordinary mutable access.

### Template subscriptions

- [ ] Template subscriptions use `$(source)`.
- [ ] `$(source)` is valid only in template head/capture positions.
- [ ] `$(` is recognized only when `$` is immediately followed by `(`.
- [ ] V1 accepts exactly one bare identifier inside `$(...)`.
- [ ] Field paths, calls, operators, computed expressions, mutable access forms, and nested templates inside `$(...)` are invalid/deferred.
- [ ] `$(source)` is invalid in compile-time templates and constants.
- [ ] `[source]` remains a snapshot template read.
- [ ] `$(source)` captures stable reactive source identity plus read-only subscription metadata.
- [ ] `$(source)` never captures a mutable borrow, copied value, or computed expression.

### Reactive template strings

- [ ] Reactive template expressions have semantic type `String`.
- [ ] Runtime representation may be a backend-owned template string value/function with dependency metadata.
- [ ] Reactive metadata propagates through direct assignment, direct argument passing, returns, and template composition.
- [ ] Ordinary `String` parameters may receive reactive template string values and preserve reactivity when inserted into another template.
- [ ] Ordinary string operations do not preserve reactivity in V1. They snapshot once or produce a structured diagnostic according to the sink/operation contract.
- [ ] String templates are the only subscription receivers. There is no direct reactive-value syntax for IO or other calls.

### Sinks and backend behavior

- [ ] V1 live sink: top-level runtime HTML fragments in the HTML-JS builder.
- [ ] HTML-JS V1 rerenders the whole runtime fragment/mount slot.
- [ ] Nested reactive regions and fine-grained updates are deferred backend optimizations.
- [ ] `io(...)` with reactive templates is deferred until the IO/side-effect model is designed.
- [ ] `assert(...)` must reject reactive/runtime template messages. Runtime template assertion messages are outside this plan.
- [ ] HTML-Wasm rejects reachable reactive runtime features with structured unsupported-backend diagnostics.
- [ ] Frontend/HIR must preserve enough metadata for a future Svelte-style backend strategy without requiring one in V1.

---

## Current repository anchors

Use these current owners. Do not create parallel paths when an existing owner can be extended.

### Documentation and planning

- `docs/roadmap/roadmap.md`
  - Main roadmap and outside-language-design-scope list.
  - Link this plan and add deferred follow-up bullets.
- `docs/src/docs/progress/#page.bst`
  - Implementation matrix. Add V1, deferred, and outside-scope rows.
- `docs/src/docs/reactivity/#page.bst`
  - Reactivity docs. Rewrite to current syntax and remove callback/closure wording.
- `docs/language-overview.md`
  - Compiler-facing syntax and semantic facts.
- `docs/compiler-design-overview.md`
  - Stage ownership and backend contracts.
- `docs/memory-management-design.md`
  - GC baseline and future ownership/lifetime note for reactive cells.

### Frontend

- `src/compiler_frontend/declaration_syntax/binding_mode.rs`
  - Current `BindingMode` already separates parse-time binding markers from `ValueMode` and contains a future-reactive note.
  - Add a reactive binding mode here rather than inventing another declaration-mode representation.
- `src/compiler_frontend/declaration_syntax/type_syntax/`
  - Owns ordinary type annotation parsing into `ParsedTypeRef` and explicitly does not own semantic type resolution.
  - Parse `$` as a declaration/parameter access prefix, then delegate the inner type to existing ordinary type parsing.
- `src/compiler_frontend/declaration_syntax/declaration_shell.rs` and signature/member parsers
  - Carry reactive declaration/parameter syntax to AST without resolving it semantically.
- `src/compiler_frontend/headers/`
  - Must continue to own top-level declaration discovery only. Reactive declarations are runtime statements, not importable headers.
- `src/compiler_frontend/ast/`
  - Owns semantic resolution, body-local declarations, template composition/folding, and runtime render-plan preparation.
  - Add reactive source identity, subscription validation, and reactive template metadata here.
- `src/compiler_frontend/hir/`
  - HIR owns backend-facing semantic metadata and validation.
  - Prefer extending HIR side-table/module metadata keyed by existing IDs over adding broad expression variants unless existing template lowering makes a dedicated variant simpler.
- `src/compiler_frontend/hir/reachability.rs`
  - Existing backend-neutral reachability tracks reachable external calls and map uses. Extend it for reachable reactive runtime features.
- `src/compiler_frontend/analysis/borrow_checker/`
  - Treat subscriptions as read-only source dependencies, not borrows. Emit/source write facts conservatively.

### Backend and HTML builder

- `src/backends/backend_feature_validation.rs`
  - Existing pre-lowering unsupported-feature validator rejects reachable map features for Wasm. Extend this pattern for reactive runtime features and unsupported sinks.
- `src/backends/js/runtime/mod.rs`
  - Runtime helper groups are modular. Add `runtime/reactivity.rs` and emit it only when reactive features are reachable/used.
- `src/backends/js/runtime/bindings.rs`
  - Existing JS runtime centralizes reference records and `__bs_read` / `__bs_write`. Extend this path for reactive binding invalidation instead of creating a parallel cell API.
- `src/backends/js/`
  - Lower HIR to readable JS under GC semantics. Keep generated reactive helpers backend-owned and not user-visible functions.
- `src/projects/html_project/js_path.rs`
  - Current JS-only HTML path emits `bst-slot-N` placeholders, calls `start()`, and inserts returned runtime fragments.
  - Replace direct insertion with a helper that handles plain strings and template string values.
- `src/projects/html_project/html_project_builder.rs`
  - Owns backend selection and calls feature validation before JS/Wasm lowering. Keep reactive target checks here or in the shared backend-feature validator.

---

## Implementation simplification targets

Use these constraints to keep implementation lean.

- [ ] Add one reactive binding mode instead of separate mutable/reactive booleans.
- [ ] Do not add a `$T` `TypeId`; underlying semantic type remains `T`.
- [ ] Use one reactive source metadata table per stage, not duplicated fields scattered across expressions, locals, diagnostics, and backend structures.
- [ ] Prefer HIR side-table metadata keyed by `HirValueId`, `LocalId`, `FunctionId`, and `HirNodeId` over large new IR variants when possible.
- [ ] Extend existing `PushRuntimeFragment` handling for live sinks instead of adding a second top-level fragment mechanism.
- [ ] Extend `HirReachability` for reactive uses instead of adding a second backend feature traversal.
- [ ] Extend JS binding helpers for reactive invalidation instead of building a separate reactive storage runtime.
- [ ] Use a single JS template string runtime representation with an empty dependency set for non-reactive template strings where this reduces branching and preserves composition.
- [ ] Keep unsupported sink policy centralized. Do not scatter ad hoc `io` / `assert` checks through backend lowering.
- [ ] Add no compatibility parser, no compatibility diagnostic path, and no API shims.

---

## Phase 0 — Planning and docs baseline

### Context

Before coding, make the repository describe one current design. This prevents agents from implementing non-current subscription syntax or closure/callback patterns.

### Steps

- [ ] Add this file at `docs/roadmap/plans/reactivity-v1-implementation-plan.md`.
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] Link this plan from the main TODO list.
  - [ ] Add deferred follow-ups: reactive template control flow, field/path subscriptions, expression dependency tracking, derived reactive values, event/action/effect syntax, `$bind(...)`, typed component messages, IO sinks, fine-grained DOM updates, keyed loops, and HTML-Wasm support.
  - [ ] Add/strengthen outside-scope wording for closures, anonymous function values, generic function values, and higher-order polymorphism.
- [ ] Update `docs/src/docs/progress/#page.bst`:
  - [ ] Add `Reactive declarations and parameters` as Deferred.
  - [ ] Add `Reactive template subscriptions` as Deferred.
  - [ ] Add `HTML-JS reactive runtime fragments` as Deferred.
  - [ ] Add deferred rows for V1 follow-ups.
  - [ ] Add outside-scope row(s) for closure/function-value surfaces.
- [ ] Rewrite `docs/src/docs/reactivity/#page.bst` around current syntax:
  - [ ] Use `$(source)` everywhere for subscriptions.
  - [ ] Remove callback/closure examples.
  - [ ] Describe V1 top-level runtime-fragment rerendering.
  - [ ] Mark event/action/effect syntax as future UI work.
- [ ] Update `docs/language-overview.md`, `docs/compiler-design-overview.md`, and `docs/memory-management-design.md` with concise compiler-facing facts.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Search docs for non-current subscription examples and remove them.
- [ ] Confirm closures/function values are not described as Reactivity follow-ups.
- [ ] Confirm no compatibility or transitional wording exists.

---

## Phase 1 — Reactive declaration and parameter syntax

### Context

Add reactive storage/access syntax first. This phase should not add template subscriptions or backend reactivity.

### Steps

- [ ] Extend `BindingMode` with a reactive runtime mode.
  - [ ] Map reactive declaration bindings to ordinary mutable-capable runtime storage at AST semantics.
  - [ ] Keep `BindingMode` out of borrow/backend layers as currently intended.
- [ ] Parse explicit reactive declarations: `name $Type = initializer`.
- [ ] Parse inferred reactive declarations: `name $= initializer`.
- [ ] Parse reactive parameters: `name $Type`.
- [ ] Parse `$` as a prefix to the full ordinary type annotation.
- [ ] Carry reactive syntax through declaration shells without semantic type resolution.
- [ ] Reject invalid syntax:
  - [ ] missing initializer for `$Type` declarations;
  - [ ] missing initializer for `$=` declarations;
  - [ ] `$` mixed with `~` or `#` binding forms;
  - [ ] `$` after initializer;
  - [ ] `$Type` in aliases, fields, returns, choice payloads, collection elements, generic arguments, const declarations, or ordinary type positions.
- [ ] Reject reactive declarations where top-level runtime statements are invalid.
  - [ ] Non-entry files cannot contribute importable reactive declarations.
  - [ ] `#mod.bst` cannot export reactive declarations.
- [ ] Add AST metadata for reactive locals and reactive parameters.
  - [ ] Store underlying `TypeId` normally.
  - [ ] Store reactive source identity separately.
- [ ] Add typed `CompilerDiagnostic` constructors and stable diagnostic codes.

### Tests

- [ ] Positive integration cases:
  - [ ] scalar reactive declaration;
  - [ ] inferred reactive declaration;
  - [ ] struct/choice/option reactive declaration;
  - [ ] collection/hashmap/fixed collection reactive declarations;
  - [ ] reactive parameter in a function signature.
- [ ] Negative integration cases:
  - [ ] missing initializer;
  - [ ] mixed `$`/`~`/`#` forms;
  - [ ] invalid ordinary type positions;
  - [ ] non-entry top-level reactive declaration;
  - [ ] facade-exported reactive declaration;
  - [ ] non-reactive value passed to `$T` parameter.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check declaration syntax still only parses shells.
- [ ] Check semantic type resolution remains AST-owned.
- [ ] Check no `$Type` semantic `TypeId` or wrapper type exists.

---

## Phase 2 — `$(source)` template subscription syntax

### Context

Add subscription syntax in templates. This phase validates source identity and records subscription metadata, but it does not require live runtime updates yet.

### Steps

- [ ] Extend template parsing to recognize `$(` only in template head/capture positions and only when `$` is immediately followed by `(`.
- [ ] Parse exactly one bare identifier inside `$(...)`.
- [ ] Reject invalid forms:
  - [ ] `$ (source)`;
  - [ ] `$()`;
  - [ ] `$(a, b)`;
  - [ ] `$(source.field)`;
  - [ ] `$(call())`;
  - [ ] `$(source + 1)`;
  - [ ] `$(~source)`;
  - [ ] nested template/literal forms;
  - [ ] use outside template head/capture positions.
- [ ] Resolve the identifier using normal scope lookup.
- [ ] Validate that the resolved binding is a reactive source or reactive parameter.
- [ ] Reject `$(source)` in compile-time templates and constants.
- [ ] Add AST template subscription nodes/fragments carrying:
  - [ ] reactive source identity;
  - [ ] underlying value `TypeId`;
  - [ ] source location.
- [ ] Preserve `[source]` as a snapshot read.

### Tests

- [ ] Positive integration cases:
  - [ ] `[: Count: [$(count)] ]` in entry runtime code;
  - [ ] `$(source)` in template head argument;
  - [ ] `$(source)` through a helper that takes `$T`;
  - [ ] `$(source)` in nested runtime template composition.
- [ ] Negative integration cases:
  - [ ] non-reactive source;
  - [ ] all invalid syntax forms listed above;
  - [ ] ordinary expression position;
  - [ ] compile-time template/constant context.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check parser owns syntax only.
- [ ] Check AST owns resolution and semantic validation.
- [ ] Check subscriptions create no borrow facts and no mutable access.

---

## Phase 3 — AST reactive template model and propagation

### Context

Runtime templates with subscriptions still have language type `String`, but AST must preserve template-string metadata through component-style value flow.

### Steps

- [ ] Add an AST reactive template metadata carrier:
  - [ ] source dependencies;
  - [ ] subscription capture locations;
  - [ ] whether a runtime template is reactive-capable;
  - [ ] whether a string value is plain, template-backed, or template-backed with dependencies.
- [ ] Make `$(source)` prevent compile-time folding of the containing template.
- [ ] Propagate template metadata through:
  - [ ] template composition;
  - [ ] assignment;
  - [ ] return;
  - [ ] direct argument passing;
  - [ ] ordinary `String` parameters inserted into templates.
- [ ] Do not propagate metadata through ordinary string operations.
- [ ] Record whether reactive template strings reach known sinks:
  - [ ] top-level runtime fragments;
  - [ ] `io(...)`;
  - [ ] `assert(...)`.
- [ ] Reject reactive template strings in constants and compile-time-only contexts.
- [ ] Mark reactive locals captured by returned/runtime templates as needing stable template-instance storage.

### Tests

- [ ] Positive cases:
  - [ ] assign reactive template to local and insert it later;
  - [ ] return reactive template from helper and insert it later;
  - [ ] pass reactive template through a `String` parameter and insert it;
  - [ ] `$T` parameter source identity preserved through helper calls.
- [ ] Negative/deferred cases:
  - [ ] reactive template in const context;
  - [ ] reactive template passed to `assert(...)`;
  - [ ] reactive template passed to `io(...)` produces the chosen deferred/unsupported diagnostic once backend validation is added.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check metadata propagation is direct and narrow.
- [ ] Check no implicit expression dependency graph exists.
- [ ] Check no closure/function-value representation is introduced.

---

## Phase 4 — HIR metadata, reachability, and validation

### Context

HIR is the backend-facing semantic boundary. It must preserve reactive sources and template metadata without becoming a backend render-plan language.

### Steps

- [ ] Add HIR reactive IDs/metadata:
  - [ ] `ReactiveSourceId`;
  - [ ] `ReactiveTemplateId` or equivalent metadata keyed by `HirValueId`;
  - [ ] dependency records with source IDs and locations.
- [ ] Extend HIR local/parameter metadata with reactive source identity.
- [ ] Preserve reactive template string metadata while keeping expression type `String`.
  - [ ] Prefer side-table metadata keyed by value/template IDs over a broad new expression variant.
  - [ ] Add a dedicated expression variant only if it is materially simpler than side-table lookup.
- [ ] Extend `PushRuntimeFragment` handling so pushed values can be recognized as plain or reactive template strings.
- [ ] Track unsupported reactive sinks in HIR metadata or reachability records.
- [ ] Extend HIR validation:
  - [ ] reactive source IDs resolve to real locals/parameters;
  - [ ] dependencies reference valid reactive sources;
  - [ ] compile-time fragments do not carry reactive metadata;
  - [ ] no parse-era `DataType` leaks into executable metadata.
- [ ] Extend `HirReachability` to collect reachable reactive runtime features and reactive sink uses.
- [ ] Update HIR display/debug output only where useful for tests and agent inspection.

### Tests

- [ ] HIR tests:
  - [ ] reactive declaration metadata;
  - [ ] reactive parameter metadata;
  - [ ] template dependency metadata;
  - [ ] top-level runtime fragment sink metadata;
  - [ ] reachability for reactive runtime features.
- [ ] Integration smoke tests that compile valid snippets through HIR and borrow validation before JS live lowering lands.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check HIR does not parse directives or reconstruct template plans.
- [ ] Check HIR validation uses `CompilerError` only for invariants.
- [ ] Check user-facing reactive errors remain `CompilerDiagnostic`.

---

## Phase 5 — Borrow validation and invalidation facts

### Context

Subscriptions are read-only metadata. Mutations still obey ordinary borrow rules, but backends need conservative source-level invalidation facts.

### Steps

- [ ] Treat reactive subscriptions as source identity dependencies, not active borrows.
- [ ] Preserve existing mutable/exclusive rules for reactive sources used as places.
- [ ] Reject mutation through `$T` parameters unless ordinary mutable access is separately available.
- [ ] Record source-level invalidation facts for:
  - [ ] assignment to reactive source local;
  - [ ] field writes through reactive source;
  - [ ] collection/map mutators through reactive source;
  - [ ] mutable/exclusive calls that may write through reactive source.
- [ ] Keep invalidation conservative and whole-source.
- [ ] Keep last-use/use-after-move analysis unchanged except where reactive metadata needs to prevent dropping live template-instance state.

### Tests

- [ ] Subscription followed by mutation is valid.
- [ ] Reactive source passed to `$T` parameter is not mutation permission.
- [ ] Reactive source passed to mutable function follows ordinary `~T` rules.
- [ ] Collection/map mutation through reactive source records invalidation.
- [ ] Existing borrow failures remain failures.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check borrow validation does not become backend lowering.
- [ ] Check invalidation facts are side-table metadata or narrow HIR metadata.
- [ ] Check subscriptions do not appear as mutable/shared borrow lifetimes.

---

## Phase 6 — JS runtime and lowering

### Context

The JS backend is GC-backed and already has centralized binding helpers. Reactivity should extend that model.

### Steps

- [ ] Add `src/backends/js/runtime/reactivity.rs`.
- [ ] Emit reactive helpers only when reachable emitted code uses reactive features.
- [ ] Add a reactive binding helper, for example `__bs_reactive_binding(value)`.
- [ ] Extend `__bs_write` / related assignment lowering:
  - [ ] update the stored value as before;
  - [ ] mark the reactive source dirty when the resolved target is reactive;
  - [ ] schedule a batched flush.
- [ ] Add scheduler helpers:
  - [ ] source-to-mounted-fragment dependency map;
  - [ ] dirty source set;
  - [ ] microtask batching or deterministic equivalent;
  - [ ] test-friendly flush hook if needed.
- [ ] Add a template string runtime representation:
  - [ ] render/snapshot function;
  - [ ] dependency collection;
  - [ ] empty dependencies for non-reactive template strings;
  - [ ] helper to render nested template string values inside other templates.
- [ ] Lower HTML page-bundle runtime templates through this common template string path when it simplifies propagation.
- [ ] Ensure ordinary string contexts can snapshot template string values where allowed.
- [ ] Ensure generated render functions/objects are backend artifacts, not user-visible closure/function values.

### Tests

- [ ] JS unit/golden tests:
  - [ ] helper emission gated to reactive programs;
  - [ ] reactive binding construction;
  - [ ] write invalidation;
  - [ ] snapshot rendering;
  - [ ] nested template string dependency collection;
  - [ ] non-reactive programs stay behaviorally unchanged.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check runtime helpers remain modular.
- [ ] Check no parallel non-binding reactive storage model exists.
- [ ] Check generated JS stays readable and deterministic.

---

## Phase 7 — HTML-JS mounting and backend feature diagnostics

### Context

Current HTML-JS inserts returned runtime fragment strings into `bst-slot-N` mount points. V1 extends that boundary: plain strings insert as before; template string values mount and rerender the whole slot.

### Steps

- [ ] Replace direct slot insertion with a helper such as `__bs_mount_template_fragment(slot_element, fragment_value)`.
- [ ] Plain string fragment behavior:
  - [ ] insert the snapshot exactly as today.
- [ ] Reactive/template string fragment behavior:
  - [ ] render initial HTML into the slot;
  - [ ] register the mounted fragment against every dependency source;
  - [ ] rerender the whole slot on dirty flush;
  - [ ] preserve source-order slot behavior.
- [ ] Keep top-level const fragments unchanged.
- [ ] Add backend feature validation:
  - [ ] HTML-JS permits top-level runtime fragment reactive sinks.
  - [ ] HTML-JS rejects or defers unsupported reactive sinks such as `io(...)` according to the chosen diagnostic policy.
  - [ ] `assert(...)` rejects reactive/runtime template messages.
  - [ ] HTML-Wasm rejects reachable reactive runtime features before Wasm lowering.
- [ ] Reuse `validate_hir_backend_feature_support` and `HirReachability` for target checks.
- [ ] Add stable diagnostic codes for unsupported sink/backend cases.

### Tests

- [ ] HTML integration cases:
  - [ ] non-reactive runtime fragments behave as before;
  - [ ] top-level runtime fragment with `$(count)` renders initial value;
  - [ ] multiple dependencies rerender the fragment;
  - [ ] batched mutations cause one flush;
  - [ ] helper returns reactive template string and top-level fragment mounts it;
  - [ ] `String` parameter receives reactive template content and preserves live mounting when inserted.
- [ ] Unsupported diagnostics:
  - [ ] HTML-Wasm reachable reactive feature rejected;
  - [ ] unreachable helper policy follows existing reachability rules;
  - [ ] reactive `io(...)` rejected/deferred;
  - [ ] reactive `assert(...)` rejected.

### Audit and validation

- [ ] Run `just validate`.
- [ ] Check `js_path.rs` remains the JS-only HTML bootstrap owner.
- [ ] Check backend-specific mount behavior does not leak into AST/HIR semantics.
- [ ] Check Wasm rejection occurs before Wasm lowering/byte emission.

---

## Phase 8 — Final docs, matrix, tests, and audit

### Context

Close the implementation by aligning docs, status matrix, roadmap, and tests with the completed behavior.

### Steps

- [ ] Update `docs/src/docs/reactivity/#page.bst` to match implementation exactly.
- [ ] Update `docs/language-overview.md` with final compiler-facing rules.
- [ ] Update `docs/compiler-design-overview.md` with final stage contracts.
- [ ] Update `docs/memory-management-design.md` with final GC/lifetime note.
- [ ] Update `docs/src/docs/progress/#page.bst` statuses:
  - [ ] V1 core syntax/metadata;
  - [ ] template subscription syntax;
  - [ ] HTML-JS runtime-fragment reactivity;
  - [ ] deferred follow-ups;
  - [ ] outside-scope closure/function-value surfaces.
- [ ] Update `docs/roadmap/roadmap.md`:
  - [ ] move completed V1 note if appropriate;
  - [ ] keep deferred follow-ups;
  - [ ] keep closure/function-value surfaces outside scope.
- [ ] Add canonical integration cases to `tests/cases/manifest.toml`.
- [ ] Update/prune stale tests and goldens.
- [ ] Search repo for non-current subscription examples and remove them.
- [ ] Search reactivity docs for closure/callback wording and remove/reclassify it.

### Final validation

- [ ] Run `just validate`.
- [ ] Run docs/site generation if separate from `just validate`.
- [ ] Manual final audit:
  - [ ] no compatibility syntax or compatibility parser path;
  - [ ] no transitional diagnostics;
  - [ ] no user-facing `CompilerError` diagnostics;
  - [ ] no `$Type` wrapper type;
  - [ ] no hidden closure/function-value model;
  - [ ] parser, AST, HIR, borrow, backend, and docs ownership boundaries are clean.

---

## Progress matrix entries

### V1 rows

- [ ] `Reactive declarations and parameters`
  - Initial status: Deferred.
  - Final V1 status: Supported or Partial depending on completed backend support.
  - Runtime target: Frontend / HIR / HTML-JS.
  - Watch points: `$Type` is metadata, not a wrapper type; no export/import surface.

- [ ] `Reactive template subscriptions`
  - Initial status: Deferred.
  - Final V1 status: Supported or Partial.
  - Runtime target: Frontend / HIR / HTML-JS.
  - Watch points: only `$(source)`, bare source only, template positions only.

- [ ] `HTML-JS reactive runtime fragments`
  - Initial status: Deferred.
  - Final V1 status: Partial.
  - Runtime target: HTML-JS.
  - Watch points: whole runtime fragment/mount-slot rerendering; fine-grained updates deferred.

### Deferred rows

- [ ] Reactive template control flow.
- [ ] Field/path subscriptions.
- [ ] Collection item subscriptions.
- [ ] Expression dependency tracking.
- [ ] Derived reactive values.
- [ ] Reactive IO sinks.
- [ ] Fine-grained DOM text/attribute/style updates.
- [ ] Nested reactive regions.
- [ ] Keyed loop diffing and stable child identity.
- [ ] Template-owned event/action/effect syntax.
- [ ] `$bind(...)` form binding helpers.
- [ ] Typed component events/messages.
- [ ] HTML-Wasm reactive runtime support.

### Outside-scope rows

- [ ] General closures.
- [ ] Anonymous function values.
- [ ] Generic function values.
- [ ] Higher-order polymorphism.

Use outside-scope wording: these are not current roadmap items and may be reevaluated after Alpha only if the language philosophy is explicitly changed.

---

## Roadmap text

Suggested main TODO entry:

```markdown
- Reactivity V1: explicit reactive sources, `$(source)` template subscriptions, HIR metadata propagation, and HTML-JS top-level runtime-fragment rerendering: `docs/roadmap/plans/reactivity-v1-implementation-plan.md`
```

Suggested follow-up note:

```markdown
- Reactivity follow-ups after V1: reactive template control flow, field/path subscriptions, expression dependency tracking, derived reactive values, template-owned event/action/effect syntax, `$bind(...)`, typed component messages, IO sink design, fine-grained DOM updates, keyed loop diffing, and HTML-Wasm support.
```

Suggested outside-scope note:

```markdown
- General closures, anonymous function values, generic function values, and higher-order polymorphism remain outside the current language design scope. Reactivity is the constrained UI-oriented mechanism intended to cover many closure-heavy UI patterns without adding general function-value semantics. Post-Alpha reevaluation is possible only if the language philosophy is explicitly changed.
```

---

## Implementation risks

- [ ] Reactive metadata could become a second type system.
  - Keep `TypeId` as the underlying value type and store source identity separately.
- [ ] Metadata propagation could become implicit expression dependency tracking.
  - Limit propagation to direct template/string value flow.
- [ ] Runtime template objects could become user-visible closures.
  - Keep them backend artifacts only.
- [ ] Whole-fragment rerendering could be inefficient.
  - Accept for V1 and track fine-grained rendering as backend optimization.
- [ ] Unsupported sinks could silently snapshot and surprise users.
  - Use explicit sink policy and tests.
- [ ] Wasm support could become accidentally partial.
  - Reject reachable reactive runtime features until a complete Wasm design exists.
- [ ] Docs could imply event/effect syntax or closures are planned.
  - Keep future UI syntax deferred and function-value features outside scope.

---

## Completion definition

Reactivity V1 is complete when:

- [ ] `$Type`, `$=`, `$T` parameters, and `$(source)` parse and validate correctly.
- [ ] Reactive identity is source metadata, not a user-facing type.
- [ ] Reactive template metadata survives assignment, return, direct argument passing, and template composition.
- [ ] Ordinary `String` parameters can preserve reactive template string values when inserted into templates.
- [ ] HTML-JS top-level runtime fragments mount reactive template strings and rerender the whole slot when dependencies mutate.
- [ ] HTML-Wasm rejects reachable reactive runtime features cleanly.
- [ ] Unsupported reactive sinks have structured diagnostics.
- [ ] Roadmap, progress matrix, language overview, compiler overview, memory design, and reactivity docs are updated.
- [ ] Integration tests cover valid syntax, invalid syntax, metadata propagation, HTML-JS behavior, and unsupported backend/sink diagnostics.
- [ ] `just validate` passes.
- [ ] Manual style/stage-boundary audit passes.
