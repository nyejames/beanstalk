# Beanstalk Traits v1 Follow-up Fix and Refactor Plan

## Goal

Fix the semantic drift found after the v1 traits landing, then simplify the largest trait-related implementation seams so the feature remains stage-owned, readable, and easy to extend.

This work is scoped after commit `96283cc7774d2bd16b77b0643170a643eeaf222e`.

## Design invariants to preserve

- Trait conformance is explicit only: `Type must TRAIT`.
- Static generic constraints use `type T is TRAIT`; multiple bounds use `and`.
- Normal `value TRAIT` annotations are dynamic trait values, only for dynamic-safe traits.
- Dynamic trait coercion is allowed only at explicit typed boundaries and must be represented in AST/HIR before backend lowering.
- `This` is trait-local and v1 direct-only.
- `DISPLAYABLE` is scaffold/core trait metadata only; it must not change `io(...)`, template interpolation, or string-content coercion in this follow-up.
- Dynamic trait runtime lowering is JS-only for now. Wasm dynamic trait values remain backend-deferred with structured reachable-use diagnostics.
- User-authored builtin/external/imported conformances are file-local extension evidence unless the compiler owns the evidence metadata.

## Repo anchors

Primary files and modules:

- `src/compiler_frontend/declaration_syntax/type_syntax/parse.rs`
- `src/compiler_frontend/declaration_syntax/signature_members.rs`
- `src/compiler_frontend/declaration_syntax/generic_parameters.rs`
- `src/compiler_frontend/headers/header_dispatch.rs`
- `src/compiler_frontend/headers/top_level_classifier.rs`
- `src/compiler_frontend/headers/types.rs`
- `src/compiler_frontend/ast/receiver_methods.rs`
- `src/compiler_frontend/ast/field_access/receiver_calls.rs`
- `src/compiler_frontend/ast/generic_bounds.rs`
- `src/compiler_frontend/ast/module_ast/environment/traits.rs`
- `src/compiler_frontend/ast/module_ast/environment/builder.rs`
- `src/compiler_frontend/ast/type_resolution/resolve_type.rs`
- `src/compiler_frontend/traits/`
- `src/compiler_frontend/type_coercion/contextual.rs`
- `src/compiler_frontend/type_coercion/dynamic_trait.rs`
- `src/compiler_frontend/hir/**` trait/dynamic trait additions
- `src/backends/backend_feature_validation.rs`
- `src/backends/js/dynamic_traits.rs`
- `src/backends/wasm/**` dynamic trait rejection sites
- `tests/cases/manifest.toml`
- Trait, generic-bound, receiver, dynamic-trait, and wasm dynamic-trait fixtures in `tests/cases/`

Documentation:

- `docs/language-overview.md`
- `docs/compiler-design-overview.md`
- `docs/roadmap/roadmap.md`
- `docs/src/docs/traits/#page.bst`
- `docs/src/docs/generics/#page.bst`
- `docs/src/docs/progress/#page.bst`

Style-guide constraints:

- User-facing source/type/rule/import diagnostics stay on `CompilerDiagnostic`.
- Internal invariants stay on `CompilerError`.
- Type decisions stay `TypeId`-first; `DataType` remains parse/diagnostic spelling after resolution.
- Prefer focused modules over broad helpers.
- Use named input/context structs for long helper signatures.
- Use explicit loops for fallible validation.
- Do not add compatibility wrappers for old trait-reserved paths.
- Remove stale comments and `#[allow(dead_code)]` annotations unless the comment explains the retained scaffold.

---

## Phase 1 — Restore strict v1 `This` rules

### Context

The agreed v1 rule is direct-only `This`:

```bst
EQUATABLE must:
    equals |This, other This| -> Bool
;
```

Allowed:

- receiver `This`
- receiver `~This`
- named direct non-receiver `This`, such as `other This`
- direct return `This`

Rejected/deferred:

- unnamed second `This`, such as `equals |This, This| -> Bool`
- `~This` outside the first receiver slot
- composed `This`: `{This}`, `This?`, `Box of This`, `Pair of Int, This`, `Result`-like forms if reachable

### Implementation

- [ ] Update `src/compiler_frontend/declaration_syntax/type_syntax/parse.rs`.
  - [ ] Add a small helper:

    ```rust
    fn reject_trait_this_composition(
        parsed_type: &ParsedTypeRef,
        context: TypeAnnotationContext,
        location: SourceLocation,
    ) -> Result<(), CompilerDiagnostic>
    ```

  - [ ] Reuse `parsed_type_contains_trait_this` and `InvalidTypeAnnotationReason::TraitThisMustBeDirect`.
  - [ ] In `parse_generic_type_argument`, reject any trait-requirement generic argument containing `This`.
  - [ ] Keep the existing collection and optional suffix rejections.
  - [ ] Keep this in declaration syntax parsing, not AST type resolution. This is a v1 syntax-shape rule.

- [ ] Update `src/compiler_frontend/declaration_syntax/signature_members.rs`.
  - [ ] In `SignatureMemberContext::TraitRequirement`, allow bare `This` and `~This` only when parsing the first member.
  - [ ] After the first member, require the readable named form `name This`.
  - [ ] Reject bare non-receiver `This` with a targeted diagnostic.
  - [ ] Keep lowercase `this` invalid inside trait requirements.

- [ ] Diagnostics.
  - [ ] Prefer existing structured diagnostics where they are clear enough.
  - [ ] If needed, add `InvalidSignatureMemberReason::TraitBareThisOnlyReceiver`.
  - [ ] Message direction: “Bare `This` is only valid as the first trait receiver parameter. Name non-receiver `This` parameters, for example `other This`.”
  - [ ] Keep payloads structured and locations precise.

### Tests

Add fixtures and register them in `tests/cases/manifest.toml`.

- [ ] `trait_requirement_generic_this_return_rejected`

  ```bst
  Box type T = |
      value T,
  |

  WRAPPER must:
      wrap |This| -> Box of This
  ;
  ```

- [ ] `trait_requirement_generic_this_param_rejected`

  ```bst
  Box type T = |
      value T,
  |

  WRAPPER must:
      wrap |This, values Box of This|
  ;
  ```

- [ ] `trait_requirement_bare_second_this_rejected`

  ```bst
  EQUATABLE must:
      equals |This, This| -> Bool
  ;
  ```

- [ ] `trait_requirement_named_this_param_success`

  ```bst
  EQUATABLE must:
      equals |This, other This| -> Bool
  ;
  ```

### Documentation

- [ ] Update `docs/src/docs/traits/#page.bst` with the direct-only `This` rules.
- [ ] Update the concise trait section in `docs/language-overview.md` if it omits this rule.
- [ ] Update `docs/src/docs/progress/#page.bst` to list composed `This` forms as deferred/rejected with structured diagnostics.

### Audit / style review / validation

- [ ] Confirm `This` composition is rejected by syntax parsing, not accidental type mismatch.
- [ ] Confirm no `DataType` semantic decisions were added.
- [ ] Confirm diagnostics use `CompilerDiagnostic`, structured reason enums, and useful `SourceLocation`s.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test trait --quiet
  cargo run -- tests --backend html --filter trait_requirement
  just validate
  ```

---

## Phase 2 — Correct builtin scalar extension evidence

### Context

The current implementation classifies builtin scalar conformance targets as file-local extension evidence, but user-authored builtin scalar receiver methods are still treated as canonical methods. That can accidentally make user-authored scalar methods/evidence look reusable or exported.

The v1 rule is:

- compiler-owned builtin evidence may be canonical metadata;
- user-authored builtin conformances are file-local extension evidence;
- user-authored builtin scalar receiver methods used for those conformances are also file-local extension methods.

### Implementation

- [ ] Update `src/compiler_frontend/ast/receiver_methods.rs`.
  - [ ] Change user-authored builtin scalar receiver methods from `ReceiverMethodKind::Canonical` to `ReceiverMethodKind::FileLocalExtension`.
  - [ ] Do not mark user-authored builtin scalar extension methods as exported.
  - [ ] Preserve compiler-owned/builder-owned builtin/external receiver metadata behavior.
  - [ ] If compiler-owned vs user-authored methods are not explicit enough, introduce a named classification enum rather than inferring from `ReceiverKey::BuiltinScalar`.

- [ ] Update `src/compiler_frontend/traits/evidence.rs` or its split successor.
  - [ ] In builtin scalar target resolution, set `required_method_kind` to `ReceiverMethodKind::FileLocalExtension` for user-authored builtin conformances.
  - [ ] Keep `TraitEvidenceKind::Builtin` reserved for compiler-owned evidence only.
  - [ ] Keep `TraitEvidenceKind::FileLocalExtension` for user-authored builtin conformances.

- [ ] Audit `src/compiler_frontend/type_coercion/dynamic_trait.rs`.
  - [ ] Keep evidence selection priority `builtin > canonical > file-local`.
  - [ ] Confirm file-local scalar evidence can only be selected from the same file.

- [ ] Audit concrete receiver fallback.
  - [ ] Confirm file-local scalar extension methods cannot be called from another file through direct receiver syntax or trait evidence fallback.

### Tests

Add or strengthen:

- [ ] `trait_conformance_builtin_extension_success`
  - Same file has `display |this Int| -> String` and `Int must DISPLAYABLE`.
  - It can be used locally where extension evidence is expected.

- [ ] `trait_conformance_builtin_extension_not_importable_rejected`
  - Extension method/conformance live in `extensions.bst`.
  - Another file cannot reuse them as global scalar behavior.

- [ ] `trait_conformance_builtin_extension_no_core_override_rejected`
  - Add only if compiler-owned builtin evidence is actually registered.
  - Confirms user source cannot override compiler-owned builtin evidence.

### Documentation and roadmap/matrix

- [ ] `docs/src/docs/traits/#page.bst`: document user-authored builtin conformances as file-local only.
- [ ] `docs/src/docs/progress/#page.bst`: mark compiler-owned builtin `DISPLAYABLE` conformances as scaffold/deferred unless they are registered.
- [ ] `docs/roadmap/roadmap.md`: keep broader standard/core trait taxonomy and builtin conformance policy as follow-up work.

### Audit / style review / validation

- [ ] Confirm `Canonical` means reusable/source-owned evidence, not “builtin receiver key”.
- [ ] Confirm diagnostics distinguish canonical, builtin, and file-local extension evidence.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test receiver --quiet
  cargo test trait --quiet
  cargo run -- tests --backend html --filter builtin_extension
  just validate
  ```

---

## Phase 3 — Remove low-value trait/generic indirection

### Context

Trait v1 added several correct but noisy seams. This phase removes avoidable lookup cost and parser bloat before the larger module splits.

Targets:

- generic parameter trait-bound lookup currently scans generic parameter lists;
- trait parsing makes `header_dispatch.rs` larger than necessary;
- `DISPLAYABLE` name lookup is repeated in a few places;
- stale “reserved traits” naming may remain where the code now owns active trait diagnostics.

### Implementation

#### Direct generic-bound index

- [ ] Update `src/compiler_frontend/datatypes/environment.rs`.
  - [ ] Add a direct index:

    ```rust
    trait_bounds_by_generic_parameter_id: FxHashMap<GenericParameterId, Vec<TraitId>>
    ```

  - [ ] Populate it in `register_generic_parameter_list`.
  - [ ] Update it in `update_generic_parameter_bounds`.
  - [ ] Change `trait_bounds_for_generic_parameter` to use the index.
  - [ ] Decide whether `GenericParameter.trait_bounds` remains necessary.
    - Preferred: one source of truth.
    - If both are kept, add an invariant comment and update both together.

- [ ] Add unit coverage in `src/compiler_frontend/datatypes/tests/generics_tests.rs`.
  - [ ] Lookup succeeds after normal registration.
  - [ ] Lookup succeeds after `update_generic_parameter_bounds`.
  - [ ] Unknown `GenericParameterId` returns `None`.

#### Extract trait header parsing

- [ ] Move trait declaration/conformance parser helpers out of `src/compiler_frontend/headers/header_dispatch.rs`.
  - [ ] New suggested file: `src/compiler_frontend/headers/trait_headers.rs`.
  - [ ] Move:
    - `parse_trait_declaration`
    - `parse_trait_requirement`
    - `parse_trait_conformance`
    - `parse_specialized_conformance_target`
    - `conformance_header_path`
    - trait-specific name validation helpers if not shared elsewhere
  - [ ] Keep `header_dispatch.rs` as dispatch/orchestration.
  - [ ] Reuse existing `parse_trait_requirement_signature_syntax`; do not fork signature parsing.

#### Tighten small helpers/comments

- [ ] Centralize core `DISPLAYABLE` name comparison through `DISPLAYABLE_TRAIT_NAME` / a local helper where it removes repetition without hiding meaning.
- [ ] Audit `src/compiler_frontend/reserved_trait_syntax.rs`.
  - [ ] If it still only owns invalid-keyword diagnostics, either keep the file with current comments or rename it to a clearer active name such as `trait_keyword_diagnostics.rs`.
  - [ ] Do not keep comments that imply trait syntax is broadly reserved/deferred.

### Tests

- [ ] Existing trait-header parser tests should continue passing.
- [ ] Add a small unit/parser regression if moving helpers risks conformance line termination behavior.

### Audit / style review / validation

- [ ] Confirm `header_dispatch.rs` is smaller and still reads as declaration dispatch.
- [ ] Confirm no compatibility wrappers were added for old helper names.
- [ ] Confirm generic-bound lookup has one obvious owner.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test generics --quiet
  cargo test trait --quiet
  cargo run -- tests --backend html --filter trait_conformance
  just validate
  ```

---

## Phase 4 — Split receiver-call dispatch by responsibility

### Context

`src/compiler_frontend/ast/field_access/receiver_calls.rs` now handles direct source methods, generic receiver method instantiation, generic-bound trait calls, dynamic trait calls, concrete trait evidence fallback, external package methods, access validation, call argument checking, and fallible result handling.

This should be split without changing behavior.

### Implementation

- [ ] Convert `receiver_calls.rs` into a directory module:

  ```text
  src/compiler_frontend/ast/field_access/receiver_calls/
      mod.rs
      shared.rs
      source_methods.rs
      generic_bound_methods.rs
      dynamic_trait_methods.rs
      concrete_trait_evidence.rs
      external_methods.rs
  ```

- [ ] Keep `mod.rs` as orchestration only.
  - [ ] Preserve current dispatch order:
    1. dynamic trait receiver method;
    2. visible declared source receiver method;
    3. generic-bound receiver method;
    4. external package receiver method;
    5. concrete trait evidence fallback.
  - [ ] Add a short comment explaining why concrete evidence fallback runs after direct/external methods.

- [ ] Move shared helpers into `shared.rs`.
  - [ ] `fallible_receiver_result_type_ids`
  - [ ] shared call-argument/result-handling helpers
  - [ ] `signature_from_trait_requirement`
  - [ ] `replace_trait_this_type`
  - [ ] small `Declaration` construction helpers used by trait-bound signatures

- [ ] Move source method dispatch into `source_methods.rs`.
  - [ ] `SourceReceiverMethodTarget`
  - [ ] generic receiver method instantiation for declared methods
  - [ ] source method call parsing

- [ ] Move static generic-bound dispatch into `generic_bound_methods.rs`.
  - [ ] generic parameter ID discovery
  - [ ] bound requirement candidate lookup
  - [ ] bound evidence lookup
  - [ ] ambiguity handling

- [ ] Move concrete evidence fallback into `concrete_trait_evidence.rs`.
  - [ ] concrete evidence candidate lookup
  - [ ] ambiguity handling
  - [ ] method path extraction from evidence

- [ ] Move dynamic trait method dispatch into `dynamic_trait_methods.rs`.
  - [ ] dynamic trait requirement lookup
  - [ ] dynamic method call AST node construction

- [ ] Move external receiver methods into `external_methods.rs`.
  - [ ] external package receiver lookup/lowering prep
  - [ ] external fallible result handling

### Refactor constraints

- [ ] Do not change semantics in this phase.
- [ ] Do not duplicate call-argument resolution logic in each module.
- [ ] Use narrow `pub(super)` APIs.
- [ ] Keep input structs where a function would otherwise take many arguments.

### Tests

Run existing fixtures; add no new semantic tests unless the split exposes a missing edge.

- [ ] source receiver methods
- [ ] choice receiver methods
- [ ] generic receiver methods
- [ ] trait-bound receiver calls
- [ ] dynamic trait methods
- [ ] external receiver methods

### Audit / style review / validation

- [ ] Every new file has a WHAT/WHY doc comment.
- [ ] `mod.rs` is a structural map and dispatch sequence.
- [ ] No new broad helper hides stage ownership.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test receiver --quiet
  cargo test trait --quiet
  cargo run -- tests --backend html --filter trait_bound_receiver
  cargo run -- tests --backend html --filter dynamic_trait_method
  just validate
  ```

---

## Phase 5 — Split trait evidence validation

### Context

`src/compiler_frontend/traits/evidence.rs` owns too many responsibilities:

- evidence storage/indexing;
- conformance target resolution;
- duplicate/override detection;
- requirement-to-method matching;
- signature compatibility;
- diagnostic labels.

Split it by responsibility while preserving behavior.

### Implementation

- [ ] Convert `src/compiler_frontend/traits/evidence.rs` into a directory module:

  ```text
  src/compiler_frontend/traits/evidence/
      mod.rs
      environment.rs
      target_resolution.rs
      validation.rs
      requirement_matching.rs
      diagnostics.rs
  ```

- [ ] `environment.rs`
  - [ ] `TraitEvidenceKind`
  - [ ] `TraitRequirementEvidence`
  - [ ] `TraitEvidenceDefinition`
  - [ ] `TraitEvidenceEnvironment`
  - [ ] evidence indexes and remapping

- [ ] `target_resolution.rs`
  - [ ] `ConformanceTarget`
  - [ ] `ResolveConformanceTargetContext`
  - [ ] source/imported/external/builtin/alias target resolution
  - [ ] specialized generic conformance deferral

- [ ] `validation.rs`
  - [ ] `ValidateTraitEvidenceInput`
  - [ ] `validate_trait_evidence`
  - [ ] pending evidence collection
  - [ ] duplicate canonical/file-local checks
  - [ ] file-local-overrides-canonical checks

- [ ] `requirement_matching.rs`
  - [ ] `ImplementationMethod`
  - [ ] `RequirementValidationContext`
  - [ ] same-file method lookup
  - [ ] receiver mutability matching
  - [ ] parameter/return count, mode, type, and channel matching
  - [ ] direct `This` substitution only

- [ ] `diagnostics.rs`
  - [ ] conformance diagnostic construction
  - [ ] previous declaration labels
  - [ ] requirement/method labels

- [ ] Keep `src/compiler_frontend/traits/mod.rs` as the subsystem map.
  - [ ] Re-export only the minimal public surface needed by AST/HIR/backends.

### Refactor constraints

- [ ] No behavior changes beyond Phase 1/2 fixes.
- [ ] No compatibility layers for old helper paths.
- [ ] Use named input/context structs.
- [ ] Use explicit loops for diagnostic-producing validation.

### Audit / style review / validation

- [ ] No file mixes target resolution, requirement matching, and diagnostics.
- [ ] Public visibility is as narrow as practical.
- [ ] No stale comments refer to the old monolithic file.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test trait --quiet
  cargo run -- tests --backend html --filter trait_conformance
  just validate
  ```

---

## Phase 6 — Dynamic trait/HIR/backend boundary audit

### Context

Dynamic trait coercion is correctly frontend-owned. This phase verifies the boundary remains clean after semantic fixes and module splits.

### Implementation audit

- [ ] `src/compiler_frontend/ast/type_resolution/resolve_type.rs`
  - [ ] Trait names in normal type position resolve to `DynamicTrait` only when visible and dynamic-safe.
  - [ ] Bound-only diagnostics name the trait and the offending requirement when available.
  - [ ] Generic-bound syntax never creates a dynamic trait type.
  - [ ] `DISPLAYABLE` scaffold does not affect `io(...)` or templates.

- [ ] `src/compiler_frontend/type_coercion/contextual.rs`
  - [ ] Dynamic trait coercion runs only at explicit typed boundaries.
  - [ ] It runs after exact/ordinary contextual coercions.
  - [ ] Missing-evidence diagnostics include the concrete type and target trait.

- [ ] `src/compiler_frontend/type_coercion/dynamic_trait.rs`
  - [ ] Evidence selection uses IDs, not signatures or rendered names.
  - [ ] JS/backend code does not rediscover evidence.

- [ ] HIR dynamic trait files.
  - [ ] HIR carries semantic IDs only: `TypeId`, `TraitId`, `TraitEvidenceId`, requirement IDs.
  - [ ] HIR validation reports invariant failures as `CompilerError`.
  - [ ] Borrow metadata accounts for dynamic dispatch without changing ownership semantics.

- [ ] JS backend.
  - [ ] Method table naming is deterministic.
  - [ ] Lowering consumes HIR-projected evidence/method facts.
  - [ ] Unreachable dynamic helpers are not emitted in reachable-only HTML builds.

- [ ] Wasm/backend feature validation.
  - [ ] Reachable dynamic trait runtime representation is rejected before unsafe lowering.
  - [ ] Unreachable dynamic trait code remains ignored where expected.

### Cleanup

- [ ] Remove stale `#[allow(dead_code)]` annotations where fields/methods are now used.
- [ ] For retained `#[allow(dead_code)]`, explain whether it is scaffold, diagnostic metadata, or deferred backend support.
- [ ] Remove comments that say traits or generic bounds are still only reserved/deferred where they are implemented.
- [ ] Update comments that say generic receiver methods are fully rejected; they are partial.

### Tests

Confirm coverage for:

- [ ] explicit dynamic trait annotation;
- [ ] argument boundary;
- [ ] return boundary;
- [ ] struct field boundary;
- [ ] choice payload boundary;
- [ ] collection element boundary;
- [ ] unannotated inference rejection;
- [ ] dynamic trait method calls;
- [ ] mutable dynamic trait methods requiring mutable receiver access;
- [ ] concrete-only method rejection through dynamic trait values;
- [ ] wasm reachable rejection;
- [ ] wasm unreachable ignored.

Add missing fixtures only where coverage is absent.

### Audit / style review / validation

- [ ] AST/HIR/backend boundaries match `docs/compiler-design-overview.md`.
- [ ] No backend performs frontend semantic decisions.
- [ ] Dynamic trait code does not use string-name matching where IDs are available, except at the `DISPLAYABLE` scaffold lookup/registration boundary.
- [ ] Run:

  ```bash
  cargo fmt
  cargo test hir --quiet
  cargo test borrow --quiet
  cargo run -- tests --backend html --filter dynamic_trait
  cargo run -- tests --backend html_wasm --filter dynamic_trait_wasm
  just validate
  ```

---

## Phase 7 — Documentation, roadmap, and matrix cleanup

### Context

Docs must describe the implemented v1 trait surface and distinguish it from deferred trait ecosystem work.

### Documentation changes

- [ ] `docs/src/docs/traits/#page.bst`
  - [ ] Document direct-only `This` rules.
  - [ ] Document canonical vs file-local extension evidence.
  - [ ] Document builtin scalar user extensions as file-local only.
  - [ ] Document dynamic-safe vs bound-only traits.
  - [ ] Document dynamic trait values as runtime-only in v1.
  - [ ] Document `DISPLAYABLE` as scaffold-only and not yet connected to output/string coercion.

- [ ] `docs/src/docs/generics/#page.bst`
  - [ ] Mark trait bounds as supported v1 surface.
  - [ ] Document `type T is TRAIT` and `type T is A and B, U is C`.
  - [ ] Document file-local extension evidence cannot satisfy public/static generic API obligations across files.
  - [ ] Document generic receiver methods as partial support for trait/generic receiver use, not a broad method redesign.

- [ ] `docs/language-overview.md`
  - [ ] Add/refine a concise compiler-facing trait section.
  - [ ] Remove stale “trait bounds wait for traits” wording.
  - [ ] Update generic receiver method deferred wording to partial support where accurate.
  - [ ] Keep this file compact and refer expanded examples to docs-site pages.

- [ ] `docs/compiler-design-overview.md`
  - [ ] Add concise architecture notes for:
    - trait header shells;
    - AST trait definitions/evidence;
    - HIR explicit dynamic trait operations;
    - backend JS lowering / Wasm unsupported validation.

- [ ] `docs/src/docs/progress/#page.bst`
  - [ ] Mark v1 traits as supported/partial with coverage notes.
  - [ ] Mark trait bounds as supported/partial instead of deferred.
  - [ ] Mark generic receiver methods as partial.
  - [ ] Explicitly defer:
    - generic traits;
    - associated types;
    - default methods;
    - static/associated trait requirements;
    - conditional/specialized conformances;
    - dynamic trait composition;
    - downcasting/pattern matching on dynamic trait values;
    - composed `This` forms;
    - operator/boolean keyword trait integration;
    - `DISPLAYABLE` integration with `io(...)` and templates;
    - Wasm dynamic trait runtime lowering;
    - broader standard/core trait taxonomy.

- [ ] `docs/roadmap/roadmap.md`
  - [ ] Replace broad “Traits (plan todo)” with targeted trait follow-ups.
  - [ ] Add concise follow-up notes for deferred trait ecosystem items above.

### Stale text search

- [ ] Run and resolve stale references:

  ```bash
  rg "trait.*deferred|traits.*reserved|Trait bounds.*Deferred|generic receiver methods.*rejected|interfaces" docs src tests
  ```

- [ ] Keep matches only when they deliberately describe deferred follow-ups.

### Audit / style review / validation

- [ ] Documentation does not contradict the v1 design invariants.
- [ ] Progress matrix distinguishes supported, partial, experimental, and deferred surfaces.
- [ ] Run:

  ```bash
  cargo run -- check docs
  just validate
  ```

---

## Phase 8 — Final touched-area style and redundancy audit

### Context

This final phase should not change semantics. It is a cleanup pass across all trait-touched areas to remove redundant code, stale comments, accidental compatibility paths, and unnecessary indirection.

### Audit checklist

- [ ] `src/compiler_frontend/traits/`
  - [ ] Each file has one clear owner.
  - [ ] `mod.rs` is a structural map.
  - [ ] Evidence lookup uses IDs, not rendered names.
  - [ ] Requirement matching compares canonical `TypeId`s and `ValueMode`s.
  - [ ] Parsed syntax shells and resolved metadata are not duplicating facts without a diagnostic need.

- [ ] `src/compiler_frontend/declaration_syntax/`
  - [ ] Trait-only rules are small context branches on shared parsers.
  - [ ] No parallel trait-only signature parser duplicates general signature parsing beyond missing-colon handling.
  - [ ] Diagnostic construction is not duplicated.

- [ ] `src/compiler_frontend/headers/`
  - [ ] Header dispatch stays dispatch-focused.
  - [ ] Trait dependency edges are header-owned.
  - [ ] No obsolete reserved-trait parser remains.

- [ ] `src/compiler_frontend/ast/module_ast/environment/traits.rs`
  - [ ] Trait definition resolution stays AST-owned.
  - [ ] Public trait surface validation uses `TypeId` and visibility facts.
  - [ ] AST does not rebuild import visibility.

- [ ] `src/compiler_frontend/ast/field_access/receiver_calls/`
  - [ ] Dispatch order is explicit.
  - [ ] Static bounds, dynamic traits, concrete evidence fallback, source methods, and external methods are separated.
  - [ ] Result-handling helpers are shared once.

- [ ] `src/compiler_frontend/type_coercion/`
  - [ ] Dynamic trait coercion has one policy path.
  - [ ] Call sites do not duplicate evidence selection.

- [ ] `src/compiler_frontend/hir/` and `src/backends/`
  - [ ] Dynamic trait nodes carry semantic IDs only.
  - [ ] JS lowering consumes explicit HIR facts.
  - [ ] Wasm rejection is reachable-code based and structured.

- [ ] Tests
  - [ ] Old reserved-trait fixtures are removed or renamed when they now test active v1 semantics.
  - [ ] Failure fixtures assert stable diagnostic codes.
  - [ ] Success fixtures assert output where possible.

### Optional cleanup if found

- [ ] Replace boolean-heavy helper arguments with enums where they encode meaningful states.
- [ ] Remove test-only fallback paths once tests can use production-like `ScopeContext` setup.
- [ ] Remove stale `traits_implementation_plan.md` references.
- [ ] Prune outdated unit tests that only pin old implementation shapes.

### Final validation

- [ ] Run:

  ```bash
  cargo fmt --check
  cargo clippy --all-targets --all-features -- -D warnings
  cargo test
  cargo run -- tests
  cargo run -- check docs
  just validate
  ```

- [ ] Manual frontend boundary review:
  - [ ] Header parsing owns syntax shells and dependency edges only.
  - [ ] AST owns semantic trait/type/evidence validation and user-facing diagnostics.
  - [ ] HIR carries explicit semantic facts, not unresolved syntax.
  - [ ] Borrow validation consumes HIR/side-table facts without changing semantics.
  - [ ] Backends lower explicit HIR facts or reject unsupported reachable runtime features.

---

## Final acceptance criteria

- [ ] `Box of This`, `{This}`, `This?`, `Pair of Int, This`, and unnamed non-receiver `This` are rejected in trait requirements with clear diagnostics.
- [ ] Named direct non-receiver `This` remains valid.
- [ ] User-authored builtin scalar conformances and receiver methods are file-local extension evidence only.
- [ ] Compiler-owned builtin evidence remains a distinct metadata path.
- [ ] Generic-bound lookup no longer scans every generic parameter list.
- [ ] Trait parsing is extracted from `header_dispatch.rs` if it remains noisy.
- [ ] Receiver-call dispatch is split by responsibility.
- [ ] Trait evidence validation is split by storage, target resolution, orchestration, requirement matching, and diagnostics.
- [ ] Dynamic trait coercion remains explicit in AST/HIR.
- [ ] JS dynamic dispatch uses frontend-selected evidence.
- [ ] Wasm dynamic trait runtime lowering remains deferred with structured reachable-use diagnostics.
- [ ] Docs, roadmap, and progress matrix accurately distinguish implemented v1 features from deferred trait follow-ups.
- [ ] `just validate` passes.
